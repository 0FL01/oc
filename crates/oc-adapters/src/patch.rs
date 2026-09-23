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

mod fs;

/// Single model-visible tool name; `write`/`edit` must never appear.
pub const MODEL_TOOL_NAMES: &[&str] = &["apply_patch"];
/// Patch text cap (mirrors `tool_argument_bytes` 2 MiB).
pub const PATCH_BYTES_CAP: usize = 2 * 1024 * 1024;
/// Per-file content cap after application.
pub const FILE_BYTES_CAP: usize = 8 * 1024 * 1024;
/// Max hunks per updated file.
pub const HUNKS_CAP: usize = 1000;
// Full preflight retains before/after images. Bound that aggregate explicitly.
const PLAN_BYTES_CAP: usize = 64 * 1024 * 1024;

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
    context: Option<String>,
    eof: bool,
}

enum FileOp {
    Add {
        path: String,
        content: Vec<String>,
        crlf: bool,
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
            FileOp::Update { move_to, .. } => move_to.as_deref(),
            FileOp::Add { .. } | FileOp::Delete { .. } => None,
        }
    }
}

/// Affected paths parsed from `patchText` before execution (DCP04).
///
/// Sorted deduplicated op paths plus rename targets. Used by protection
/// checks so every touched path is verified, not just a `filePath` param.
pub fn affected_paths(patch_text: &str) -> Result<Vec<String>, PatchError> {
    let ops = parse_plan(patch_text).map_err(|failure| failure.error)?;
    let mut paths = std::collections::BTreeSet::new();
    for op in &ops {
        paths.insert(op.path().to_string());
        if let Some(target) = op.move_to() {
            paths.insert(target.to_string());
        }
    }
    Ok(paths.into_iter().collect())
}

/// Max files summarised for a diff card (bounded UI state).
pub const DIFF_FILES_CAP: usize = 8;
/// Max hunks counted per file (counts stay exact; this only bounds work).
const DIFF_HUNKS_CAP: usize = 4096;

/// One file in a bounded `apply_patch` diff representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFile {
    /// Op path as written in the patch (never invented).
    pub path: String,
    /// Add / Update / Delete.
    pub change: &'static str,
    /// Rename target, when the op moves a file.
    pub move_to: Option<String>,
    /// Added lines (`+`), exact for updates and adds.
    pub additions: usize,
    /// Removed lines (`-`), exact for updates.
    pub removals: usize,
    /// `@@` hunk sections in this op.
    pub hunks: usize,
}

/// Bounded diff summary of an `apply_patch` payload for tool cards.
///
/// Parsed from the same grammar as execution, but never touching the
/// filesystem and never keeping a second copy of the patch: only counts and
/// paths, capped at [`DIFF_FILES_CAP`] files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffSummary {
    /// Per-file summaries, in patch order.
    pub files: Vec<DiffFile>,
    /// Exact added-line total over all ops.
    pub additions: usize,
    /// Exact removed-line total over all ops.
    pub removals: usize,
    /// True when more files exist than [`DIFF_FILES_CAP`].
    pub truncated: bool,
    /// True when the payload is not a well-formed patch envelope.
    pub malformed: bool,
}

/// Summarise an `apply_patch` payload (bounded, no filesystem access).
pub fn diff_summary(patch_text: &str) -> DiffSummary {
    let mut summary = DiffSummary::default();
    if patch_text.len() > PATCH_BYTES_CAP {
        summary.malformed = true;
        return summary;
    }
    let mut current: Option<DiffFile> = None;
    let mut began = false;
    for raw in patch_text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            began = true;
            finish_file(&mut summary, current.take());
            current = Some(DiffFile {
                path: path.trim().to_string(),
                change: "Add",
                move_to: None,
                additions: 0,
                removals: 0,
                hunks: 0,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Update File: ") {
            began = true;
            finish_file(&mut summary, current.take());
            current = Some(DiffFile {
                path: path.trim().to_string(),
                change: "Update",
                move_to: None,
                additions: 0,
                removals: 0,
                hunks: 0,
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("*** Delete File: ") {
            began = true;
            finish_file(&mut summary, current.take());
            current = Some(DiffFile {
                path: path.trim().to_string(),
                change: "Delete",
                move_to: None,
                additions: 0,
                removals: 0,
                hunks: 0,
            });
            continue;
        }
        let Some(file) = current.as_mut() else {
            continue;
        };
        if let Some(target) = line.strip_prefix("*** Move to: ") {
            file.move_to = Some(target.trim().to_string());
            continue;
        }
        if line.starts_with("@@") {
            if file.hunks < DIFF_HUNKS_CAP {
                file.hunks += 1;
            }
            continue;
        }
        if line.starts_with("*** ") {
            continue;
        }
        if line.starts_with('+') {
            file.additions += 1;
            summary.additions += 1;
        } else if line.starts_with('-') {
            file.removals += 1;
            summary.removals += 1;
        }
    }
    finish_file(&mut summary, current.take());
    if !began {
        summary.malformed = true;
    }
    summary
}

fn finish_file(summary: &mut DiffSummary, file: Option<DiffFile>) {
    let Some(file) = file else {
        return;
    };
    if summary.files.len() < DIFF_FILES_CAP {
        summary.files.push(file);
    } else {
        summary.truncated = true;
    }
}

/// Max rendered diff lines kept per file for a tool card (bounded UI state).
pub const DIFF_RENDER_LINES_CAP: usize = 60;
/// Max rendered hunks kept per file for a tool card.
pub const DIFF_RENDER_HUNKS_CAP: usize = 8;

/// One rendered diff line kind (`diff.text.{added,removed,context}` roles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    /// Unchanged context line.
    Context,
    /// Added line (`+`).
    Added,
    /// Removed line (`-`).
    Removed,
}

/// One bounded rendered diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    /// Added / removed / context.
    pub kind: DiffLineKind,
    /// Line text without the diff marker.
    pub text: String,
    /// Exact 1-based line number in the resulting file for added content of
    /// an Add-file op (the file did not exist before, so the numbering is
    /// exact). `None` for updates and deletes: this patch grammar matches
    /// hunks by context, so the base position is unknown before execution and
    /// is never invented.
    pub line_number: Option<usize>,
}

/// One bounded rendered hunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    /// `@@` anchor context, when the hunk header carried one.
    pub anchor: Option<String>,
    /// Rendered lines.
    pub lines: Vec<DiffLine>,
    /// True when lines were dropped at [`DIFF_RENDER_LINES_CAP`].
    pub truncated: bool,
}

/// One bounded per-file diff for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFileRender {
    /// Op path as written in the patch (never invented).
    pub path: String,
    /// Add / Update / Delete.
    pub change: &'static str,
    /// Rename target, when the op moves a file.
    pub move_to: Option<String>,
    /// Bounded hunks (Add-file content is one synthetic hunk).
    pub hunks: Vec<DiffHunk>,
    /// Exact added-line total for this op.
    pub additions: usize,
    /// Exact removed-line total for this op.
    pub removals: usize,
    /// True when hunks or lines were dropped at the render caps.
    pub truncated: bool,
}

/// Bounded per-file hunks for rendering one `apply_patch` payload.
///
/// Parsed from the same grammar as execution and never touching the
/// filesystem. State is bounded by [`DIFF_FILES_CAP`] files,
/// [`DIFF_RENDER_HUNKS_CAP`] hunks and [`DIFF_RENDER_LINES_CAP`] lines per
/// file; a malformed payload yields an empty vector (the caller keeps the raw
/// error text instead of an invented diff).
pub fn diff_render(patch_text: &str) -> Vec<DiffFileRender> {
    let Ok(ops) = parse_plan(patch_text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for op in ops.iter().take(DIFF_FILES_CAP) {
        let mut render = DiffFileRender {
            path: op.path().to_string(),
            change: match op {
                FileOp::Add { .. } => "Add",
                FileOp::Update { .. } => "Update",
                FileOp::Delete { .. } => "Delete",
            },
            move_to: op.move_to().map(str::to_string),
            hunks: Vec::new(),
            additions: 0,
            removals: 0,
            truncated: false,
        };
        match op {
            FileOp::Add { content, .. } => {
                render.additions = content.len();
                let mut lines = Vec::new();
                for (index, text) in content.iter().enumerate() {
                    if lines.len() >= DIFF_RENDER_LINES_CAP {
                        render.truncated = true;
                        break;
                    }
                    lines.push(DiffLine {
                        kind: DiffLineKind::Added,
                        text: text.clone(),
                        line_number: Some(index + 1),
                    });
                }
                render.hunks.push(DiffHunk {
                    anchor: None,
                    truncated: render.truncated,
                    lines,
                });
            }
            FileOp::Update { hunks, .. } => {
                render.additions = hunks
                    .iter()
                    .flat_map(|hunk| hunk.lines.iter())
                    .filter(|line| matches!(line, HunkLine::Add(_)))
                    .count();
                render.removals = hunks
                    .iter()
                    .flat_map(|hunk| hunk.lines.iter())
                    .filter(|line| matches!(line, HunkLine::Remove(_)))
                    .count();
                for hunk in hunks.iter().take(DIFF_RENDER_HUNKS_CAP) {
                    let mut rendered = DiffHunk {
                        anchor: hunk.context.clone(),
                        lines: Vec::new(),
                        truncated: false,
                    };
                    for line in &hunk.lines {
                        if rendered.lines.len() >= DIFF_RENDER_LINES_CAP {
                            rendered.truncated = true;
                            render.truncated = true;
                            break;
                        }
                        match line {
                            HunkLine::Context(text) => rendered.lines.push(DiffLine {
                                kind: DiffLineKind::Context,
                                text: text.clone(),
                                line_number: None,
                            }),
                            HunkLine::Add(text) => rendered.lines.push(DiffLine {
                                kind: DiffLineKind::Added,
                                text: text.clone(),
                                line_number: None,
                            }),
                            HunkLine::Remove(text) => rendered.lines.push(DiffLine {
                                kind: DiffLineKind::Removed,
                                text: text.clone(),
                                line_number: None,
                            }),
                        }
                    }
                    render.hunks.push(rendered);
                }
                if hunks.len() > DIFF_RENDER_HUNKS_CAP {
                    render.truncated = true;
                }
            }
            FileOp::Delete { .. } => {}
        }
        out.push(render);
    }
    out
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
                let Some(body) = lines[i].strip_prefix('+') else {
                    return plan_err(ops.len(), "Add File lines must start with +");
                };
                content.push(body.to_string());
                i += 1;
            }
            ops.push(FileOp::Add {
                path,
                content,
                crlf,
            });
        } else if let Some(path) = line.strip_prefix("*** Update File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                return plan_err(ops.len(), "empty update path");
            }
            i += 1;
            let move_to = if let Some(target) = lines[i].strip_prefix("*** Move to:") {
                if target.trim().is_empty() {
                    return plan_err(ops.len(), "empty move target");
                }
                i += 1;
                Some(target.trim().to_string())
            } else {
                None
            };
            let mut hunks: Vec<Hunk> = vec![Hunk {
                lines: Vec::new(),
                context: None,
                eof: false,
            }];
            let mut header_seen = false;
            while i < end && !lines[i].starts_with("*** ") {
                let body = lines[i].as_str();
                if body == "@@" || body.starts_with("@@ ") {
                    if hunks.last().is_some_and(|h| h.lines.is_empty()) {
                        if header_seen {
                            return plan_err(ops.len(), "empty update hunk");
                        }
                    } else {
                        hunks.push(Hunk {
                            lines: Vec::new(),
                            context: None,
                            eof: false,
                        });
                    }
                    hunks.last_mut().expect("hunk").context =
                        body.strip_prefix("@@ ").map(str::to_string);
                    header_seen = true;
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
            if i < end && lines[i] == "*** End of File" {
                hunks.last_mut().expect("hunk").eof = true;
                i += 1;
                while i < end && lines[i].is_empty() {
                    i += 1;
                }
            }
            if hunks.iter().any(|h| h.lines.is_empty()) {
                return plan_err(ops.len(), "empty update hunk");
            }
            if hunks.len() > HUNKS_CAP {
                return plan_err(ops.len(), "too many hunks");
            }
            ops.push(FileOp::Update {
                path,
                hunks,
                move_to,
            });
        } else if let Some(path) = line.strip_prefix("*** Delete File:") {
            let path = path.trim().to_string();
            if path.is_empty() {
                return plan_err(ops.len(), "empty delete path");
            }
            ops.push(FileOp::Delete { path });
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

    let canonical_root = std::fs::canonicalize(project_root).map_err(|_| {
        op_fail(
            0,
            "$patch",
            PatchError::Io {
                path: "$patch".into(),
            },
        )
    })?;
    let root = fs::Root::new(&canonical_root).map_err(|_| {
        op_fail(
            0,
            "$patch",
            PatchError::Io {
                path: "$patch".into(),
            },
        )
    })?;
    // Preflight: all deterministic failures, including every hunk, before
    // creating even a destination directory or a staging file.
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut prepared = Vec::new();
    let mut plan_bytes = 0usize;
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
        for path in std::iter::once(rel).chain(op.move_to()) {
            let normalized = map_resolve(&files, path).map_err(|e| op_fail(idx, path, e))?;
            if seen
                .iter()
                .any(|other| normalized.starts_with(other) || other.starts_with(&normalized))
            {
                return Err(op_fail(
                    idx,
                    path,
                    PatchError::Conflict {
                        path: path.to_string(),
                        reason: "duplicate-or-overlapping-path".to_string(),
                    },
                ));
            }
            seen.push(normalized);
        }
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
        let path = abs
            .strip_prefix(&canonical_root)
            .map_err(|_| fail("outside root"))?
            .to_path_buf();
        let io = |_| {
            op_fail(
                idx,
                rel,
                PatchError::Io {
                    path: rel.to_string(),
                },
            )
        };
        root.writable_parent(&path).map_err(io)?;
        let before = match op {
            FileOp::Add { .. } => {
                if !root.absent(&path).map_err(io)? {
                    return Err(op_fail(
                        idx,
                        rel,
                        PatchError::Conflict {
                            path: rel.to_string(),
                            reason: "already-exists".into(),
                        },
                    ));
                }
                None
            }
            _ => Some(root.snapshot(&path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::FileTooLarge {
                    op_fail(
                        idx,
                        rel,
                        PatchError::TooLarge {
                            path: rel.into(),
                            reason: "file cap".into(),
                        },
                    )
                } else {
                    io(error)
                }
            })?),
        };
        let after = match op {
            FileOp::Add { content, crlf, .. } => {
                Some(render_doc(content, *crlf, !content.is_empty()))
            }
            FileOp::Update { hunks, .. } => {
                let doc = split_doc(&before.as_ref().expect("snapshot").bytes, rel)
                    .map_err(|e| op_fail(idx, rel, e))?;
                let trailing_nl = doc.lines.is_empty() || doc.trailing_nl;
                let mut lines = doc.lines;
                apply_hunks(&mut lines, hunks, rel).map_err(|e| op_fail(idx, rel, e))?;
                Some(render_doc(&lines, doc.crlf, trailing_nl))
            }
            FileOp::Delete { .. } => None,
        };
        if after
            .as_ref()
            .is_some_and(|bytes| bytes.len() > FILE_BYTES_CAP)
        {
            return Err(op_fail(
                idx,
                rel,
                PatchError::TooLarge {
                    path: rel.to_string(),
                    reason: "file cap".into(),
                },
            ));
        }
        plan_bytes +=
            before.as_ref().map_or(0, |s| s.bytes.len()) + after.as_ref().map_or(0, Vec::len);
        if plan_bytes > PLAN_BYTES_CAP {
            return Err(op_fail(
                idx,
                rel,
                PatchError::TooLarge {
                    path: rel.to_string(),
                    reason: "plan preimages and results exceed 64 MiB".into(),
                },
            ));
        }
        let target = op
            .move_to()
            .map(|target| {
                let abs = map_resolve(&files, target)?;
                let relative = abs
                    .strip_prefix(&canonical_root)
                    .map_err(|_| PatchError::OutsideRoot {
                        path: target.into(),
                    })?
                    .to_path_buf();
                if !root.absent(&relative).map_err(|_| PatchError::Io {
                    path: target.into(),
                })? {
                    return Err(PatchError::Conflict {
                        path: target.into(),
                        reason: "move-target-exists".into(),
                    });
                }
                root.writable_parent(&relative)
                    .map_err(|_| PatchError::Io {
                        path: target.into(),
                    })?;
                Ok(relative)
            })
            .transpose()
            .map_err(|e| op_fail(idx, rel, e))?;
        prepared.push(Prepared {
            path,
            target,
            before,
            after,
        });
    }

    // Execution: per-file commits, stop at first runtime failure.
    let mut done: Vec<FileResult> = Vec::new();
    for (idx, (op, prepared)) in ops.iter().zip(&prepared).enumerate() {
        match execute_op(&root, op, prepared, policy, &mut done) {
            Ok(()) => {}
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

struct Prepared {
    path: PathBuf,
    target: Option<PathBuf>,
    before: Option<fs::Snapshot>,
    after: Option<Vec<u8>>,
}

fn execute_op(
    root: &fs::Root,
    op: &FileOp,
    prepared: &Prepared,
    policy: &dyn WritePolicy,
    done: &mut Vec<FileResult>,
) -> Result<(), PatchError> {
    let rel = op.path();
    policy.check(rel)?;
    let io = |_| PatchError::Io {
        path: rel.to_string(),
    };
    let entry = root
        .entry(&prepared.path, prepared.before.is_none())
        .map_err(io)?;
    let staged = prepared
        .after
        .as_ref()
        .map(|after| entry.stage(after, prepared.before.as_ref().map(|before| before.mode)))
        .transpose()
        .map_err(io)?;
    if let Some(before) = &prepared.before
        && !entry.unchanged(before).map_err(io)?
    {
        return Err(PatchError::Conflict {
            path: rel.to_string(),
            reason: "stale-preimage".into(),
        });
    }
    if let Some(staged) = staged {
        staged.commit(prepared.before.is_none()).map_err(io)?;
    } else {
        entry.remove().map_err(io)?;
    }
    // Record the namespace change before sync/move, which can fail after it.
    done.push(FileResult {
        path: rel.to_string(),
        new_path: None,
        op: match op {
            FileOp::Add { .. } => "add",
            FileOp::Update { .. } => "update",
            FileOp::Delete { .. } => "delete",
        },
        hash_before: prepared
            .before
            .as_ref()
            .map(|before| sha_hex(&before.bytes)),
        hash_after: prepared.after.as_ref().map(|after| sha_hex(after)),
    });
    entry.sync().map_err(io)?;
    if let Some(target) = &prepared.target {
        let updated = entry.snapshot().map_err(io)?;
        policy.check(op.move_to().expect("target"))?;
        let target = root.entry(target, true).map_err(io)?;
        if !entry.unchanged(&updated).map_err(io)? {
            return Err(PatchError::Conflict {
                path: rel.to_string(),
                reason: "stale-preimage".into(),
            });
        }
        entry.move_to(&target).map_err(io)?;
        done.last_mut().expect("committed update").new_path = op.move_to().map(str::to_string);
        target.sync().map_err(io)?;
        entry.sync().map_err(io)?;
    }
    Ok(())
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
        if let Some(context) = &hunk.context {
            let positions: Vec<_> = (offset..lines.len())
                .filter(|&index| lines[index] == *context)
                .collect();
            if positions.len() != 1 {
                return Err(PatchError::Conflict {
                    path: rel.to_string(),
                    reason: "missing-or-ambiguous-context".to_string(),
                });
            }
            offset = positions[0] + 1;
        }
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
        let old_len = hunk
            .lines
            .iter()
            .filter(|l| !matches!(l, HunkLine::Add(_)))
            .count();
        let positions: Vec<_> = (offset..lines.len())
            .filter(|&probe| {
                (!hunk.eof || probe + old_len == lines.len())
                    && matches_at(lines, probe, &hunk.lines)
            })
            .collect();
        if positions.len() > 1 {
            return Err(PatchError::Conflict {
                path: rel.to_string(),
                reason: "ambiguous-preimage".to_string(),
            });
        }
        let pos = positions
            .first()
            .copied()
            .ok_or_else(|| PatchError::Conflict {
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
        offset = next.len();
        next.extend_from_slice(&lines[cursor..]);
        *lines = next;
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

#[cfg(test)]
mod tests {
    use super::{
        AllowAll, ApplyFailure, DIFF_FILES_CAP, DIFF_RENDER_LINES_CAP, DiffLineKind, FileResult,
        MODEL_TOOL_NAMES, PatchError, ProtectedGlobs, apply_patch, diff_render, diff_summary,
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
    fn v06b_partial_result_records_only_committed_effects() {
        struct DenySecond(std::sync::atomic::AtomicUsize);
        impl super::WritePolicy for DenySecond {
            fn check(&self, path: &str) -> Result<(), PatchError> {
                // Both intents pass preflight; only the second execution is
                // refused, after the first namespace change has committed.
                if path == "blocked.txt"
                    && self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1
                {
                    Err(PatchError::Conflict {
                        path: path.into(),
                        reason: "refused during execution".into(),
                    })
                } else {
                    Ok(())
                }
            }
        }
        let (_tmp, project, data) = setup();
        let patch = "*** Begin Patch\n*** Add File: applied.txt\n+real\n*** Add File: blocked.txt\n+not-applied\n*** End Patch";
        let result = apply_patch(
            &project,
            &data,
            patch,
            &DenySecond(std::sync::atomic::AtomicUsize::new(0)),
        )
        .unwrap_err();
        assert_eq!(result.failed_op, 1);
        assert_eq!(result.done.len(), 1);
        assert_eq!(result.done[0].path, "applied.txt");
        assert_eq!(
            fs::read_to_string(project.join("applied.txt")).unwrap(),
            "real\n"
        );
        assert!(!project.join("blocked.txt").exists());
        let output = crate::tools::patch_outcome(Err(result));
        assert!(
            output.starts_with("error: partial op 1 (blocked.txt):")
                && output.contains("\ndone add applied.txt "),
            "{output}"
        );
    }

    #[test]
    fn tool02_new_text_file_unicode_crlf() {
        let (_tmp, project, data) = setup();
        let patch =
            "*** Begin Patch\n*** Add File: hello.txt\n+hello \u{1F30D}\n+second\n*** End Patch\n";
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
            "*** Move to: taken.txt\n",
            "@@\n",
            "-v1\n",
            "+v2\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("move conflict");
        assert!(err.done.is_empty());
        assert_eq!(fs::read(project.join("src.txt")).expect("read"), b"v1\n");
        // Free target moves with updated content.
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src.txt\n",
            "*** Move to: dst.txt\n",
            "@@\n",
            "-v1\n",
            "+v2\n",
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
            "*** Begin Patch\n*** Add File: dup.txt\n+new\n*** End Patch\n",
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
            "*** Add File: ok.txt\n+content\n",
            "*** Frobnicate: nope\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("grammar");
        assert!(err.done.is_empty());
        assert_eq!(err.failed_op, 1);
        assert!(!project.join("ok.txt").exists());
        // A stale hunk is deterministic: it must fail preflight, not leave
        // an earlier add behind. Actual I/O partial failure is in patch_audit.
        fs::write(project.join("second.txt"), b"real\n").expect("seed");
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: ok.txt\n+content\n",
            "*** Update File: second.txt\n",
            "@@\n",
            "-imagined\n",
            "+new\n",
            "*** End Patch\n",
        );
        let err = run(&project, &data, patch).expect_err("preflight");
        assert!(err.done.is_empty());
        assert_eq!(err.failed_op, 1);
        assert!(!project.join("ok.txt").exists());
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
            "*** Begin Patch\n*** Add File: secrets/k.txt\n+x\n*** End Patch\n",
            &policy,
        )
        .expect_err("protected");
        assert!(matches!(err.error, PatchError::Protected { .. }));
        let outside = run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: ../evil.txt\n+x\n*** End Patch\n",
        )
        .expect_err("outside");
        assert!(matches!(
            outside.error,
            PatchError::OutsideRoot { .. } | PatchError::OwnDataRoot { .. }
        ));
        let dataroot = run(
            &project,
            &data,
            "*** Begin Patch\n*** Add File: ../data/evil.txt\n+x\n*** End Patch\n",
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
    #[test]
    fn diff_summary_is_exact_and_bounded() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src/lib.rs\n",
            "@@\n",
            "-a\n",
            "+b\n",
            "+c\n",
            "*** Add File: tests/new.rs\n",
            "+x\n",
            "*** Delete File: old.txt\n",
            "*** End Patch",
        );
        let summary = diff_summary(patch);
        assert_eq!(summary.files.len(), 3);
        assert_eq!(summary.files[0].path, "src/lib.rs");
        assert_eq!(summary.files[0].change, "Update");
        assert_eq!(summary.files[0].hunks, 1);
        assert_eq!(
            (summary.files[0].additions, summary.files[0].removals),
            (2, 1)
        );
        assert_eq!(summary.files[1].change, "Add");
        assert_eq!(summary.files[2].change, "Delete");
        assert_eq!((summary.additions, summary.removals), (3, 1));
        assert!(!summary.truncated && !summary.malformed);

        // Rename target and hunk count stay visible.
        let moved = diff_summary(
            "*** Begin Patch\n*** Update File: a.txt\n*** Move to: b.txt\n@@\n@@\n-x\n*** End Patch",
        );
        assert_eq!(moved.files[0].move_to.as_deref(), Some("b.txt"));
        assert_eq!(moved.files[0].hunks, 2);

        // More files than the cap: bounded list, exact totals.
        let mut big = String::from("*** Begin Patch\n");
        for index in 0..(DIFF_FILES_CAP + 3) {
            big.push_str(&format!("*** Add File: f{index}.rs\n+x\n"));
        }
        let summary = diff_summary(&big);
        assert_eq!(summary.files.len(), DIFF_FILES_CAP);
        assert!(summary.truncated);
        assert_eq!(summary.additions, DIFF_FILES_CAP + 3);

        // Non-patch payloads are marked, never invented.
        let summary = diff_summary("{\"not\":\"a patch\"}");
        assert!(summary.files.is_empty() && summary.malformed);
    }

    #[test]
    fn diff_render_keeps_hunks_bounded_and_never_invents_line_numbers() {
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Update File: src/lib.rs\n",
            "@@ fn main\n",
            " ctx\n",
            "-old\n",
            "+new\n",
            "*** Add File: tests/new.rs\n",
            "+x\n",
            "+y\n",
            "*** Delete File: old.txt\n",
            "*** End Patch",
        );
        let files = diff_render(patch);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/lib.rs");
        assert_eq!(files[0].change, "Update");
        assert_eq!((files[0].additions, files[0].removals), (1, 1));
        assert_eq!(files[0].hunks.len(), 1);
        assert_eq!(files[0].hunks[0].anchor.as_deref(), Some("fn main"));
        let kinds: Vec<DiffLineKind> = files[0].hunks[0]
            .lines
            .iter()
            .map(|line| line.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::Removed,
                DiffLineKind::Added
            ]
        );
        assert!(
            files[0].hunks[0]
                .lines
                .iter()
                .all(|line| line.line_number.is_none()),
            "update hunk positions are unknown before execution"
        );

        // Add-file content is numbered exactly from 1.
        assert_eq!(files[1].change, "Add");
        assert_eq!(files[1].hunks[0].lines[0].line_number, Some(1));
        assert_eq!(files[1].hunks[0].lines[1].line_number, Some(2));
        assert_eq!(files[1].additions, 2);
        // Delete carries no body in this grammar: nothing is invented.
        assert_eq!(files[2].change, "Delete");
        assert!(files[2].hunks.is_empty());

        // Caps: per-file lines and hunks stay bounded, counts stay exact.
        let mut big = String::from("*** Begin Patch\n*** Update File: big.rs\n@@\n");
        for index in 0..(DIFF_RENDER_LINES_CAP + 5) {
            big.push_str(&format!("+line {index}\n"));
        }
        big.push_str("*** End Patch");
        let files = diff_render(&big);
        assert_eq!(files[0].hunks[0].lines.len(), DIFF_RENDER_LINES_CAP);
        assert!(files[0].truncated && files[0].hunks[0].truncated);
        assert_eq!(files[0].additions, DIFF_RENDER_LINES_CAP + 5);

        // Malformed payloads render nothing instead of an invented diff.
        assert!(diff_render("not a patch").is_empty());
        assert!(diff_render("*** Begin Patch\n*** End Patch").is_empty());
    }
}
