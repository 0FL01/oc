//! Unified `apply_patch` model tool for T09 (TOOL02–TOOL04).
//!
//! Single JSON argument `patchText` in upstream-style grammar:
//! `*** Begin Patch` / `*** Add File:` / `*** Update File:` /
//! `*** Delete File:` / `*** Move to:` / `*** End Patch`. This is unrelated
//! to any provider-hosted Responses `apply_patch` schema.
//!
//! Plan-first: the whole text parses and validates (grammar, sizes, paths,
//! conflicts, permissions) before any write. Execution commits per file
//! (temp file in the same directory, fsync, atomic rename, mode preserved);
//! the first runtime failure stops the plan and returns the partial outcome
//! — earlier commits stand, never auto-rolled back, never reported success.
//! Preimage = hunk context + removals matched exactly; stale content is a
//! conflict, not a fuzzy merge. No `write`/`edit` registry entries exist.

use std::path::PathBuf;

use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::files::{Files, glob_match};

/// Single model-visible tool name; `write`/`edit` must never appear.
pub const MODEL_TOOL_NAMES: &[&str] = &["apply_patch"];
/// Patch text cap (mirrors `tool_argument_bytes` 2 MiB).
pub const PATCH_BYTES_CAP: usize = 2 * 1024 * 1024;
/// Per-file content cap after application.
pub const FILE_BYTES_CAP: usize = 8 * 1024 * 1024;
/// Max hunks per updated file.
pub const HUNKS_CAP: usize = 1000;

/// Typed patch errors (paths only, no contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PatchError {
    /// Grammar failure at plan stage (`entry` = 0-based op index).
    #[error("invalid patch entry {entry}: {reason}")]
    InvalidPatch {
        /// Operation index under parse.
        entry: usize,
        /// Human reason.
        reason: String,
    },
    /// Pre-execution or execution conflict.
    #[error("conflict at {path}: {reason}")]
    Conflict {
        /// Relative path.
        path: String,
        /// Reason (`already-exists`, `stale-preimage`, `missing`, ...).
        reason: String,
    },
    /// Protected path (glob list).
    #[error("protected path {path}")]
    Protected {
        /// Relative path.
        path: String,
    },
    /// Outside the trusted project root.
    #[error("outside trusted root {path}")]
    OutsideRoot {
        /// Given path.
        path: String,
    },
    /// Own data root target.
    #[error("own data root {path}")]
    OwnDataRoot {
        /// Given path.
        path: String,
    },
    /// Symlink involved in a write path.
    #[error("symlink refused {path}")]
    Symlink {
        /// Given path.
        path: String,
    },
    /// Binary content with precise diagnostic.
    #[error("binary content {path}: {reason}")]
    Binary {
        /// Relative path.
        path: String,
        /// Reason.
        reason: String,
    },
    /// Size cap exceeded.
    #[error("too large {path}: {reason}")]
    TooLarge {
        /// Relative path or `$patch`.
        path: String,
        /// Reason.
        reason: String,
    },
    /// Write policy denial.
    #[error("denied {path}")]
    Denied {
        /// Relative path.
        path: String,
    },
    /// I/O failure (kind only).
    #[error("io error at {path}")]
    Io {
        /// Relative path.
        path: String,
    },
}

/// Per-file commit record: sufficient operation metadata for the runtime to
/// persist delete/rename history in own storage (no snapshot/undo subsystem).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    /// Relative path the op addressed.
    pub path: String,
    /// Rename target, if any.
    pub new_path: Option<String>,
    /// `add` / `update` / `delete`.
    pub op: &'static str,
    /// sha256 of bytes before (None for add).
    pub hash_before: Option<String>,
    /// sha256 of bytes after (None for delete).
    pub hash_after: Option<String>,
}

/// Partial failure: earlier commits stand, overall status is failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyFailure {
    /// Committed file results before the failure.
    pub done: Vec<FileResult>,
    /// 0-based op index that failed.
    pub failed_op: usize,
    /// Relative path of the failed op (`$patch` for plan errors).
    pub failed_path: String,
    /// Failure cause.
    pub error: PatchError,
}

/// Write policy hook (central permission pipeline plugs in here).
pub trait WritePolicy {
    /// Authorize a relative write path or deny it.
    fn check(&self, path: &str) -> Result<(), PatchError>;
}

/// Allow-everything policy (tests; production passes the central policy).
#[derive(Debug, Clone, Copy)]
pub struct AllowAll;

impl WritePolicy for AllowAll {
    fn check(&self, _path: &str) -> Result<(), PatchError> {
        Ok(())
    }
}

/// Protected-glob policy: matching paths are refused before any write.
#[derive(Debug, Clone)]
pub struct ProtectedGlobs {
    /// Glob patterns (`*`/`?`/`**`, slash-aware).
    pub patterns: Vec<String>,
}

impl WritePolicy for ProtectedGlobs {
    fn check(&self, path: &str) -> Result<(), PatchError> {
        if self.patterns.iter().any(|p| glob_match(p, path)) {
            return Err(PatchError::Protected {
                path: path.to_string(),
            });
        }
        Ok(())
    }
}

enum HunkLine {
    Context(String),
    Remove(String),
    Add(String),
}

struct Hunk {
    lines: Vec<HunkLine>,
}

enum FileOp {
    Add {
        path: String,
        content: Vec<String>,
        crlf: bool,
        move_to: Option<String>,
    },
    Update {
        path: String,
        hunks: Vec<Hunk>,
        move_to: Option<String>,
    },
    Delete {
        path: String,
    },
}

impl FileOp {
    fn path(&self) -> &str {
        match self {
            FileOp::Add { path, .. } | FileOp::Update { path, .. } | FileOp::Delete { path } => {
                path
            }
        }
    }

    fn move_to(&self) -> Option<&str> {
        match self {
            FileOp::Add { move_to, .. } | FileOp::Update { move_to, .. } => move_to.as_deref(),
            FileOp::Delete { .. } => None,
        }
    }
}

/// Parse `patchText` into an execution plan (no filesystem access).
fn parse_plan(text: &str) -> Result<Vec<FileOp>, ApplyFailure> {
    if text.len() > PATCH_BYTES_CAP {
        return Err(ApplyFailure {
            done: Vec::new(),
            failed_op: 0,
            failed_path: "$patch".to_string(),
            error: PatchError::TooLarge {
                path: "$patch".to_string(),
                reason: "patch exceeds 2 MiB".to_string(),
            },
        });
    }
    // Strip one trailing \r per line for marker detection (CRLF patches).
    let raw_lines: Vec<&str> = text.split('\n').collect();
    // Drop the artifacts of a trailing newline: the last non-empty line
    // must be the End marker; body blank lines before it are preserved.
    let mut raw_lines = raw_lines;
    while raw_lines.len() > 1 && raw_lines.last() == Some(&"") {
        raw_lines.pop();
    }
    let lines: Vec<String> = raw_lines
        .iter()
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect();
    if lines.first().map(String::as_str) != Some("*** Begin Patch") {
        return Err(ApplyFailure {
            done: Vec::new(),
            failed_op: 0,
            failed_path: "$patch".to_string(),
            error: PatchError::InvalidPatch {
                entry: 0,
                reason: "must start with *** Begin Patch".to_string(),
            },
        });
    }
    if lines.last().map(String::as_str) != Some("*** End Patch") {
        return Err(ApplyFailure {
            done: Vec::new(),
            failed_op: 0,
            failed_path: "$patch".to_string(),
            error: PatchError::InvalidPatch {
                entry: 0,
                reason: "must end with *** End Patch".to_string(),
            },
        });
    }

    let mut ops: Vec<FileOp> = Vec::new();
    let mut i = 1usize;
    let end = lines.len() - 1;
    while i < end {
        let line = lines[i].as_str();
        if let Some(path) = line.strip_prefix("*** Add File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                return plan_err(ops.len(), "empty add path");
            }
            i += 1;
            let mut content = Vec::new();
            let mut crlf = false;
            while i < end && !lines[i].starts_with("*** ") {
                let raw = raw_lines[i];
                if raw.ends_with('\r') {
                    crlf = true;
                }
                if raw.contains('\0') {
                    return plan_err(ops.len(), "NUL byte in add body");
                }
                content.push(lines[i].clone());
                i += 1;
            }
            ops.push(FileOp::Add {
                path,
                content,
                crlf,
                move_to: None,
            });
        } else if let Some(path) = line.strip_prefix("*** Update File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                return plan_err(ops.len(), "empty update path");
            }
            i += 1;
            let mut hunks: Vec<Hunk> = vec![Hunk { lines: Vec::new() }];
            while i < end && !lines[i].starts_with("*** ") {
                let body = lines[i].as_str();
                if body.starts_with("@@") {
                    if !hunks.last().map(|h| h.lines.is_empty()).unwrap_or(false) {
                        hunks.push(Hunk { lines: Vec::new() });
                    }
                } else if let Some(rest) = body.strip_prefix(' ') {
                    hunks
                        .last_mut()
                        .expect("hunk")
                        .lines
                        .push(HunkLine::Context(rest.to_string()));
                } else if let Some(rest) = body.strip_prefix('-') {
                    hunks
                        .last_mut()
                        .expect("hunk")
                        .lines
                        .push(HunkLine::Remove(rest.to_string()));
                } else if let Some(rest) = body.strip_prefix('+') {
                    hunks
                        .last_mut()
                        .expect("hunk")
                        .lines
                        .push(HunkLine::Add(rest.to_string()));
                } else if body.is_empty() {
                    hunks
                        .last_mut()
                        .expect("hunk")
                        .lines
                        .push(HunkLine::Context(String::new()));
                } else {
                    return plan_err(ops.len(), "hunk lines must start with space/-/+/@@");
                }
                if raw_lines[i].contains('\0') {
                    return plan_err(ops.len(), "NUL byte in update body");
                }
                i += 1;
            }
            if hunks.len() > HUNKS_CAP {
                return plan_err(ops.len(), "too many hunks");
            }
            ops.push(FileOp::Update {
                path,
                hunks,
                move_to: None,
            });
        } else if let Some(path) = line.strip_prefix("*** Delete File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                return plan_err(ops.len(), "empty delete path");
            }
            ops.push(FileOp::Delete { path });
            i += 1;
        } else if let Some(target) = line.strip_prefix("*** Move to:") {
            let target = target.trim().to_string();
            if target.is_empty() {
                return plan_err(ops.len(), "empty move target");
            }
            let last = ops.last_mut().ok_or_else(|| ApplyFailure {
                done: Vec::new(),
                failed_op: 0,
                failed_path: "$patch".to_string(),
                error: PatchError::InvalidPatch {
                    entry: 0,
                    reason: "*** Move to without a preceding file op".to_string(),
                },
            })?;
            match last {
                FileOp::Add { move_to, .. } | FileOp::Update { move_to, .. } => {
                    if move_to.is_some() {
                        return plan_err(ops.len().saturating_sub(1), "duplicate move target");
                    }
                    *move_to = Some(target);
                }
                FileOp::Delete { .. } => {
                    return plan_err(ops.len().saturating_sub(1), "move after delete");
                }
            }
            i += 1;
        } else {
            return plan_err(
                ops.len(),
                "unknown directive; expected Add/Update/Delete/Move to/End Patch",
            );
        }
    }
    Ok(ops)
}

fn plan_err<T>(entry: usize, reason: &str) -> Result<T, ApplyFailure> {
    Err(ApplyFailure {
        done: Vec::new(),
        failed_op: entry,
        failed_path: "$patch".to_string(),
        error: PatchError::InvalidPatch {
            entry,
            reason: reason.to_string(),
        },
    })
}

struct TextDoc {
    lines: Vec<String>,
    crlf: bool,
    trailing_nl: bool,
}

fn split_doc(bytes: &[u8], rel: &str) -> Result<TextDoc, PatchError> {
    if bytes.contains(&0) {
        return Err(PatchError::Binary {
            path: rel.to_string(),
            reason: "NUL byte in file content".to_string(),
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| PatchError::Binary {
        path: rel.to_string(),
        reason: "non-UTF-8 content".to_string(),
    })?;
    Ok(TextDoc {
        lines: text
            .lines()
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
            .collect(),
        crlf: bytes.windows(2).any(|w| w == b"\r\n"),
        trailing_nl: bytes.ends_with(b"\n"),
    })
}

fn render_doc(lines: &[String], crlf: bool, trailing_nl: bool) -> Vec<u8> {
    if lines.is_empty() {
        return Vec::new();
    }
    let sep = if crlf { "\r\n" } else { "\n" };
    let mut out = lines.join(sep);
    if trailing_nl {
        out.push_str(sep);
    }
    out.into_bytes()
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Apply a patch plan: parse → validate → per-file commits.
///
/// Returns `Ok` only when every op committed; any failure (plan or runtime)
/// returns `Err(ApplyFailure)` with earlier commits listed.
pub fn apply_patch(
    project_root: &std::path::Path,
    data_root: &std::path::Path,
    patch_text: &str,
    policy: &dyn WritePolicy,
) -> Result<Vec<FileResult>, ApplyFailure> {
    let ops = parse_plan(patch_text)?;
    let files = Files::new(project_root, data_root).map_err(|_| ApplyFailure {
        done: Vec::new(),
        failed_op: 0,
        failed_path: "$patch".to_string(),
        error: PatchError::InvalidPatch {
            entry: 0,
            reason: "bad roots".to_string(),
        },
    })?;

    // Preflight: sizes, paths, conflicts, permissions — before any write.
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for (idx, op) in ops.iter().enumerate() {
        let rel = op.path();
        let fail = |reason: &str| ApplyFailure {
            done: Vec::new(),
            failed_op: idx,
            failed_path: rel.to_string(),
            error: PatchError::InvalidPatch {
                entry: idx,
                reason: reason.to_string(),
            },
        };
        if seen.insert(rel.to_string(), idx).is_some() {
            return Err(ApplyFailure {
                done: Vec::new(),
                failed_op: idx,
                failed_path: rel.to_string(),
                error: PatchError::Conflict {
                    path: rel.to_string(),
                    reason: "duplicate op".to_string(),
                },
            });
        }
        let abs = match files.resolve_path(rel) {
            Ok(abs) => abs,
            Err(crate::files::FileToolError::OutsideRoot) => {
                return Err(op_fail(
                    idx,
                    rel,
                    PatchError::OutsideRoot {
                        path: rel.to_string(),
                    },
                ));
            }
            Err(crate::files::FileToolError::OwnDataRoot) => {
                return Err(op_fail(
                    idx,
                    rel,
                    PatchError::OwnDataRoot {
                        path: rel.to_string(),
                    },
                ));
            }
            Err(crate::files::FileToolError::SymlinkEscape) => {
                return Err(op_fail(
                    idx,
                    rel,
                    PatchError::Symlink {
                        path: rel.to_string(),
                    },
                ));
            }
            Err(_) => return Err(fail("unresolvable path")),
        };
        // Refuse symlink mutation in the first profile, even for add/update.
        if let Ok(meta) = std::fs::symlink_metadata(&abs)
            && meta.file_type().is_symlink()
        {
            return Err(op_fail(
                idx,
                rel,
                PatchError::Symlink {
                    path: rel.to_string(),
                },
            ));
        }
        if let Err(e) = policy.check(rel) {
            let path = rel.to_string();
            return Err(ApplyFailure {
                done: Vec::new(),
                failed_op: idx,
                failed_path: path,
                error: e,
            });
        }
        if let Some(target) = op.move_to() {
            if files.resolve_path(target).is_err() {
                return Err(op_fail(
                    idx,
                    target,
                    PatchError::InvalidPatch {
                        entry: idx,
                        reason: "bad move target".to_string(),
                    },
                ));
            }
            if let Err(e) = policy.check(target) {
                return Err(ApplyFailure {
                    done: Vec::new(),
                    failed_op: idx,
                    failed_path: target.to_string(),
                    error: e,
                });
            }
        }
        // Preflight existence: add/move-target must be absent, update/delete present.
        match op {
            FileOp::Add { .. } => {
                if std::fs::symlink_metadata(&abs).is_ok() {
                    return Err(op_fail(
                        idx,
                        rel,
                        PatchError::Conflict {
                            path: rel.to_string(),
                            reason: "already-exists".to_string(),
                        },
                    ));
                }
            }
            FileOp::Update { .. } | FileOp::Delete { .. } => {
                if std::fs::symlink_metadata(&abs).is_err() {
                    return Err(op_fail(
                        idx,
                        rel,
                        PatchError::Conflict {
                            path: rel.to_string(),
                            reason: "missing".to_string(),
                        },
                    ));
                }
            }
        }
        if let Some(target) = op.move_to() {
            let tabs = files
                .resolve_path(target)
                .map_err(|_| fail("unresolvable move target"))?;
            if std::fs::symlink_metadata(&tabs).is_ok() {
                return Err(op_fail(
                    idx,
                    target,
                    PatchError::Conflict {
                        path: target.to_string(),
                        reason: "move-target-exists".to_string(),
                    },
                ));
            }
        }
    }

    // Execution: per-file commits, stop at first runtime failure.
    let mut done: Vec<FileResult> = Vec::new();
    for (idx, op) in ops.iter().enumerate() {
        match execute_op(&files, op, policy) {
            Ok(result) => done.push(result),
            Err(error) => {
                return Err(ApplyFailure {
                    done,
                    failed_op: idx,
                    failed_path: op.path().to_string(),
                    error,
                });
            }
        }
    }
    Ok(done)
}

fn op_fail(idx: usize, rel: &str, error: PatchError) -> ApplyFailure {
    ApplyFailure {
        done: Vec::new(),
        failed_op: idx,
        failed_path: rel.to_string(),
        error,
    }
}

fn execute_op(
    files: &Files,
    op: &FileOp,
    policy: &dyn WritePolicy,
) -> Result<FileResult, PatchError> {
    // Policy re-checked at execution for TOCTOU narrowness (cheap, local).
    policy.check(op.path())?;
    match op {
        FileOp::Add {
            path,
            content,
            crlf,
            move_to,
        } => {
            let abs = map_resolve(files, path)?;
            if std::fs::symlink_metadata(&abs).is_ok() {
                return Err(PatchError::Conflict {
                    path: path.clone(),
                    reason: "already-exists".to_string(),
                });
            }
            let bytes = render_doc(content, *crlf, !content.is_empty());
            if bytes.len() > FILE_BYTES_CAP {
                return Err(PatchError::TooLarge {
                    path: path.clone(),
                    reason: "file cap".to_string(),
                });
            }
            let mode = None;
            write_atomic(&abs, &bytes, mode)?;
            let mut result = FileResult {
                path: path.clone(),
                new_path: None,
                op: "add",
                hash_before: None,
                hash_after: Some(sha_hex(&bytes)),
            };
            if let Some(target) = move_to {
                policy.check(target)?;
                rename_checked(files, path, target)?;
                result.new_path = Some(target.clone());
            }
            Ok(result)
        }
        FileOp::Update {
            path,
            hunks,
            move_to,
        } => {
            let abs = map_resolve(files, path)?;
            let before = std::fs::read(&abs).map_err(|_| PatchError::Io { path: path.clone() })?;
            let doc = split_doc(&before, path)?;
            if before.len() > FILE_BYTES_CAP {
                return Err(PatchError::TooLarge {
                    path: path.clone(),
                    reason: "file cap".to_string(),
                });
            }
            let mode = std::fs::metadata(&abs).ok().map(|m| {
                use std::os::unix::fs::PermissionsExt as _;
                m.permissions().mode()
            });
            let mut lines = doc.lines;
            // An empty file has no newline style; added lines bring
            // newline-terminated endings. Otherwise the file's existing
            // trailing-newline flag is preserved verbatim.
            let trailing_nl = if lines.is_empty() {
                true
            } else {
                doc.trailing_nl
            };
            apply_hunks(&mut lines, hunks, path)?;
            let after = render_doc(&lines, doc.crlf, trailing_nl);
            if after.len() > FILE_BYTES_CAP {
                return Err(PatchError::TooLarge {
                    path: path.clone(),
                    reason: "file cap".to_string(),
                });
            }
            write_atomic(&abs, &after, mode)?;
            let mut result = FileResult {
                path: path.clone(),
                new_path: None,
                op: "update",
                hash_before: Some(sha_hex(&before)),
                hash_after: Some(sha_hex(&after)),
            };
            if let Some(target) = move_to {
                policy.check(target)?;
                rename_checked(files, path, target)?;
                result.new_path = Some(target.clone());
            }
            Ok(result)
        }
        FileOp::Delete { path } => {
            let abs = map_resolve(files, path)?;
            let before = std::fs::read(&abs).map_err(|_| PatchError::Conflict {
                path: path.clone(),
                reason: "missing".to_string(),
            })?;
            std::fs::remove_file(&abs).map_err(|_| PatchError::Io { path: path.clone() })?;
            Ok(FileResult {
                path: path.clone(),
                new_path: None,
                op: "delete",
                hash_before: Some(sha_hex(&before)),
                hash_after: None,
            })
        }
    }
}

fn map_resolve(files: &Files, rel: &str) -> Result<PathBuf, PatchError> {
    files.resolve_path(rel).map_err(|e| match e {
        crate::files::FileToolError::OutsideRoot => PatchError::OutsideRoot {
            path: rel.to_string(),
        },
        crate::files::FileToolError::OwnDataRoot => PatchError::OwnDataRoot {
            path: rel.to_string(),
        },
        crate::files::FileToolError::SymlinkEscape => PatchError::Symlink {
            path: rel.to_string(),
        },
        crate::files::FileToolError::NotFound => PatchError::Conflict {
            path: rel.to_string(),
            reason: "missing".to_string(),
        },
        _ => PatchError::Io {
            path: rel.to_string(),
        },
    })
}

fn apply_hunks(lines: &mut Vec<String>, hunks: &[Hunk], rel: &str) -> Result<(), PatchError> {
    let mut offset = 0usize;
    for hunk in hunks {
        // Pure-addition hunks (no anchor) append at the end — this is how
        // text is added to an existing empty file via Update.
        let anchored = hunk
            .lines
            .iter()
            .any(|l| matches!(l, HunkLine::Context(_) | HunkLine::Remove(_)));
        if !anchored {
            for line in &hunk.lines {
                if let HunkLine::Add(text) = line {
                    lines.push(text.clone());
                }
            }
            offset = lines.len();
            continue;
        }
        let mut found: Option<usize> = None;
        let mut probe = offset.min(lines.len());
        while probe < lines.len() || (probe == 0 && lines.is_empty()) {
            if matches_at(lines, probe, &hunk.lines) {
                found = Some(probe);
                break;
            }
            probe += 1;
            if probe > lines.len() {
                break;
            }
        }
        // Also probe positions before the offset (edits may reorder).
        if found.is_none() {
            for probe in 0..offset.min(lines.len()) {
                if matches_at(lines, probe, &hunk.lines) {
                    found = Some(probe);
                    break;
                }
            }
        }
        let pos = found.ok_or_else(|| PatchError::Conflict {
            path: rel.to_string(),
            reason: "stale-preimage".to_string(),
        })?;
        let mut next: Vec<String> = Vec::with_capacity(lines.len() + 8);
        next.extend_from_slice(&lines[..pos]);
        let mut cursor = pos;
        for line in &hunk.lines {
            match line {
                HunkLine::Context(_) | HunkLine::Remove(_) => cursor += 1,
                HunkLine::Add(text) => next.push(text.clone()),
            }
            if matches!(line, HunkLine::Context(_)) {
                next.push(lines[cursor - 1].clone());
            }
        }
        next.extend_from_slice(&lines[cursor..]);
        *lines = next;
        offset = pos + 1;
    }
    Ok(())
}

fn matches_at(lines: &[String], pos: usize, hunk: &[HunkLine]) -> bool {
    let mut cursor = pos;
    for line in hunk {
        match line {
            HunkLine::Add(_) => {}
            HunkLine::Context(expected) | HunkLine::Remove(expected) => {
                if lines.get(cursor).map(String::as_str) != Some(expected.as_str()) {
                    return false;
                }
                cursor += 1;
            }
        }
    }
    true
}

fn write_atomic(abs: &std::path::Path, bytes: &[u8], mode: Option<u32>) -> Result<(), PatchError> {
    let rel = abs.to_string_lossy().into_owned();
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|_| PatchError::Io { path: rel.clone() })?;
    }
    let mut tmp = abs.as_os_str().to_os_string();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).map_err(|_| PatchError::Io { path: rel.clone() })?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .open(&tmp)
        .map_err(|_| PatchError::Io { path: rel.clone() })?;
    file.sync_all()
        .map_err(|_| PatchError::Io { path: rel.clone() })?;
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode & 0o777))
            .map_err(|_| PatchError::Io { path: rel.clone() })?;
    }
    std::fs::rename(&tmp, abs).map_err(|_| PatchError::Io { path: rel.clone() })?;
    Ok(())
}

fn rename_checked(files: &Files, from_rel: &str, to_rel: &str) -> Result<(), PatchError> {
    let from = map_resolve(files, from_rel)?;
    let to = map_resolve(files, to_rel)?;
    if std::fs::symlink_metadata(&to).is_ok() {
        return Err(PatchError::Conflict {
            path: to_rel.to_string(),
            reason: "move-target-exists".to_string(),
        });
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|_| PatchError::Io {
            path: to_rel.to_string(),
        })?;
    }
    std::fs::rename(&from, &to).map_err(|_| PatchError::Io {
        path: from_rel.to_string(),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AllowAll, ApplyFailure, FileResult, MODEL_TOOL_NAMES, PatchError, ProtectedGlobs,
        apply_patch,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::os::unix::fs::symlink;

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let tmp = tempfile::tempdir().expect("temp");
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        fs::create_dir_all(&project).expect("project");
        fs::create_dir_all(&data).expect("data");
        (tmp, project, data)
    }

    fn run(
        project: &std::path::Path,
        data: &std::path::Path,
        text: &str,
    ) -> Result<Vec<FileResult>, ApplyFailure> {
        apply_patch(project, data, text, &AllowAll)
    }

    #[test]
    fn tool02_new_text_file_unicode_crlf() {
        let (_tmp, project, data) = setup();
        let patch =
            "*** Begin Patch\n*** Add File: hello.txt\nhello \u{1F30D}\nsecond\n*** End Patch\n";
        let done = run(&project, &data, patch).expect("add");
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].op, "add");
        assert_eq!(done[0].hash_before, None);
        let bytes = fs::read(project.join("hello.txt")).expect("read");
        assert_eq!(bytes, "hello \u{1F30D}\nsecond\n".as_bytes());
    }

    #[test]
    fn tool02_new_empty_file_and_append_via_update() {
        let (_tmp, project, data) = setup();
        run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: empty.txt\n*** End Patch\n",
        )
        .expect("empty add");
        assert_eq!(fs::read(project.join("empty.txt")).expect("read").len(), 0);
        // Append to the existing empty file through Update (pure-add hunk).
        let patch =
            "*** Begin Patch\n*** Update File: empty.txt\n@@\n+first\n+second\n*** End Patch\n";
        run(&project, &data, patch).expect("append");
        assert_eq!(
            fs::read(project.join("empty.txt")).expect("read"),
            b"first\nsecond\n"
        );
    }

    #[test]
    fn tool02_crlf_and_trailing_newline_preserved() {
        let (_tmp, project, data) = setup();
        fs::write(project.join("win.txt"), b"a\r\nb\r\n").expect("seed");
        let patch = "*** Begin Patch\n*** Update File: win.txt\n@@\n a\n-b\n+B\n*** End Patch\n";
        run(&project, &data, patch).expect("update");
        assert_eq!(
            fs::read(project.join("win.txt")).expect("read"),
            b"a\r\nB\r\n"
        );
        // No-trailing-newline files keep no trailing newline.
        fs::write(project.join("noeol.txt"), b"x\ny").expect("seed2");
        let patch = "*** Begin Patch\n*** Update File: noeol.txt\n@@\n x\n-y\n+Y\n*** End Patch\n";
        run(&project, &data, patch).expect("update2");
        assert_eq!(fs::read(project.join("noeol.txt")).expect("read"), b"x\nY");
    }

    #[test]
    fn tool03_targeted_hunks_and_full_replace() {
        let (_tmp, project, data) = setup();
        fs::write(project.join("m.txt"), b"1\n2\n3\n4\n5\n").expect("seed");
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: m.txt\n",
            "@@\n",
            " 1\n",
            "-2\n",
            "+two\n",
            " 3\n",
            "@@\n",
            " 4\n",
            "-5\n",
            "+five\n",
            "*** End Patch\n",
        );
        run(&project, &data, patch).expect("hunks");
        assert_eq!(
            fs::read(project.join("m.txt")).expect("read"),
            b"1\ntwo\n3\n4\nfive\n"
        );
        // Full replacement through all-remove/all-add hunks.
        let patch = "*** Begin Patch\n*** Update File: m.txt\n@@\n-1\n-two\n-3\n-4\n-five\n+replaced\n*** End Patch\n";
        run(&project, &data, patch).expect("replace");
        assert_eq!(
            fs::read(project.join("m.txt")).expect("read"),
            b"replaced\n"
        );
    }

    #[test]
    fn tool03_delete_and_move_without_overwrite() {
        let (_tmp, project, data) = setup();
        fs::write(project.join("gone.txt"), b"bye\n").expect("seed");
        let done = run(
            &project,
            &data,
            "*** Begin Patch\n*** Delete File: gone.txt\n*** End Patch\n",
        )
        .expect("delete");
        assert_eq!(done[0].op, "delete");
        assert!(done[0].hash_before.is_some());
        assert!(!project.join("gone.txt").exists());
        // Move after update; existing target blocks the whole plan.
        fs::write(project.join("src.txt"), b"v1\n").expect("src");
        fs::write(project.join("taken.txt"), b"other\n").expect("taken");
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src.txt\n",
            "@@\n",
            "-v1\n",
            "+v2\n",
            "*** Move to: taken.txt\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("move conflict");
        assert!(err.done.is_empty());
        assert_eq!(fs::read(project.join("src.txt")).expect("read"), b"v1\n");
        // Free target moves with updated content.
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src.txt\n",
            "@@\n",
            "-v1\n",
            "+v2\n",
            "*** Move to: dst.txt\n",
            "*** End Patch\n",
        );
        let done = run(&project, &data, patch).expect("move");
        assert_eq!(done[0].new_path.as_deref(), Some("dst.txt"));
        assert_eq!(fs::read(project.join("dst.txt")).expect("read"), b"v2\n");
        assert!(!project.join("src.txt").exists());
    }

    #[test]
    fn tool03_registry_has_no_write_edit() {
        assert_eq!(MODEL_TOOL_NAMES, &["apply_patch"]);
        assert!(!MODEL_TOOL_NAMES.contains(&"write"));
        assert!(!MODEL_TOOL_NAMES.contains(&"edit"));
    }

    #[test]
    fn tool04_add_existing_and_stale_preimage() {
        let (_tmp, project, data) = setup();
        fs::write(project.join("dup.txt"), b"old\n").expect("seed");
        let err = run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: dup.txt\nnew\n*** End Patch\n",
        )
        .expect_err("dup");
        assert!(matches!(err.error, PatchError::Conflict { .. }));
        assert!(err.done.is_empty());
        // Patch authored against v1; file now at v2 → stale preimage.
        let stale = concat!(
            "*** Begin Patch\n",
            "*** Update File: dup.txt\n",
            "@@\n",
            "-old\n",
            "+new\n",
            "*** End Patch\n",
        );
        fs::write(project.join("dup.txt"), b"changed\n").expect("change");
        let err = run(&project, &data, stale).expect_err("stale");
        assert_eq!(
            err.error,
            PatchError::Conflict {
                path: "dup.txt".to_string(),
                reason: "stale-preimage".to_string()
            }
        );
        assert_eq!(
            fs::read(project.join("dup.txt")).expect("read"),
            b"changed\n"
        );
    }

    #[test]
    fn tool04_invalid_later_entry_and_partial_runtime() {
        let (_tmp, project, data) = setup();
        // Grammar failure in the second op: plan stage, nothing committed.
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: ok.txt\ncontent\n",
            "*** Frobnicate: nope\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("grammar");
        assert!(err.done.is_empty());
        assert_eq!(err.failed_op, 1);
        assert!(!project.join("ok.txt").exists());
        // Runtime failure in the second op: first file stands, no success.
        fs::write(project.join("second.txt"), b"real\n").expect("seed");
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: ok.txt\ncontent\n",
            "*** Update File: second.txt\n",
            "@@\n",
            "-imagined\n",
            "+new\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("partial");
        assert_eq!(err.done.len(), 1);
        assert_eq!(err.done[0].path, "ok.txt");
        assert_eq!(err.failed_op, 1);
        assert!(project.join("ok.txt").exists());
    }

    #[test]
    fn tool04_protected_outside_dataroot_symlink() {
        let (tmp, project, data) = setup();
        fs::write(data.join("s.txt"), b"s").expect("data seed");
        let policy = ProtectedGlobs {
            patterns: vec!["secrets/**".to_string()],
        };
        let err = apply_patch(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: secrets/k.txt\nx\n*** End Patch\n",
            &policy,
        )
        .expect_err("protected");
        assert!(matches!(err.error, PatchError::Protected { .. }));
        let outside = run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: ../evil.txt\nx\n*** End Patch\n",
        )
        .expect_err("outside");
        assert!(matches!(
            outside.error,
            PatchError::OutsideRoot { .. } | PatchError::OwnDataRoot { .. }
        ));
        let dataroot = run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: ../data/evil.txt\nx\n*** End Patch\n",
        )
        .expect_err("dataroot");
        assert!(matches!(dataroot.error, PatchError::OwnDataRoot { .. }));
        symlink(data.join("s.txt"), project.join("link.txt")).expect("link");
        // A link landing inside the data root reports OwnDataRoot ...
        let datalink = run(
            &project,
            &data,
            "*** Begin Patch\n*** Delete File: link.txt\n*** End Patch\n",
        )
        .expect_err("datalink");
        assert!(matches!(datalink.error, PatchError::OwnDataRoot { .. }));
        // ... while a link escaping elsewhere reports Symlink.
        let outside = tmp.path().join("outside.txt");
        fs::write(&outside, b"o").expect("outside");
        symlink(&outside, project.join("link2.txt")).expect("link2");
        let sym = run(
            &project,
            &data,
            "*** Begin Patch\n*** Delete File: link2.txt\n*** End Patch\n",
        )
        .expect_err("symlink");
        assert!(matches!(sym.error, PatchError::Symlink { .. }));
        assert!(tmp.path().join("data").join("s.txt").exists());
    }

    #[test]
    fn tool04_mode_preserved_and_binary_refused() {
        let (_tmp, project, data) = setup();
        let p = project.join("mode.sh");
        fs::write(&p, b"echo\n").expect("seed");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod");
        run(
            &project,
            &data,
            "*** Begin Patch\n*** Update File: mode.sh\n@@\n-echo\n+echo hi\n*** End Patch\n",
        )
        .expect("update");
        assert_eq!(
            fs::metadata(&p).expect("meta").permissions().mode() & 0o777,
            0o755
        );
        fs::write(project.join("bin.dat"), [0x41, 0x00]).expect("bin");
        let err = run(
            &project,
            &data,
            "*** Begin Patch\n*** Update File: bin.dat\n@@\n-A\n+B\n*** End Patch\n",
        )
        .expect_err("binary");
        assert!(matches!(err.error, PatchError::Binary { .. }));
    }
}
