//! Model-facing read/search tools for T08 (TOOL01).
//!
//! Bounded `read` (path/offset/limit with truncated/next cursor), stable
//! sorted paginated `glob`/`grep`, binary/large diagnostics, and a hard
//! refusal of the own data root by direct path, `..` recursion and symlink
//! escape. Plain literal and regex modes are explicit; no custom regex
//! engine is written — a small bounded matcher covers `*`/`?`/`**` for glob
//! and substring search for literal grep, while `regex` grep uses the
//! `regex-lite`-free hand matcher limited to `.`/`*`/`[]` classes? No:
//! regex mode is refused until a vetted engine is pinned (see below).

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

/// Per-call read byte cap (mirrors `preview_bytes` 65536).
pub const READ_BYTES_CAP: usize = 65536;
/// Max lines returned per read call.
pub const READ_LINES_CAP: usize = 2000;
/// Max files walked per glob/grep call.
pub const WALK_FILES_CAP: usize = 10000;
/// Per-file grep scan cap (bytes).
pub const GREP_FILE_BYTES_CAP: u64 = 1024 * 1024;
/// Default page size for glob/grep.
pub const DEFAULT_PAGE_LIMIT: usize = 50;

/// Typed file-tool errors (no file contents in messages).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FileToolError {
    /// Path does not exist under the trusted root.
    #[error("not found")]
    NotFound,
    /// Path escapes the trusted project root.
    #[error("outside trusted root")]
    OutsideRoot,
    /// Path targets the own data root (rejected always).
    #[error("own data root is not readable by model tools")]
    OwnDataRoot,
    /// Symlink component or escape.
    #[error("symlink escape refused")]
    SymlinkEscape,
    /// Binary content (NUL byte); not loaded.
    #[error("binary file; use bounded preview metadata only")]
    Binary,
    /// Invalid pattern.
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
    /// Walk/scan budget exhausted.
    #[error("search budget exhausted")]
    BudgetExhausted,
    /// Underlying I/O failure (kind only).
    #[error("io error")]
    Io,
}

/// Bounded read result with pagination cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadResult {
    /// 1-based first line number in this chunk.
    pub offset: u64,
    /// Lines returned.
    pub lines: Vec<String>,
    /// True when more lines remain.
    pub truncated: bool,
    /// Cursor for the next call (`None` when complete).
    pub next_offset: Option<u64>,
}

/// Single grep hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    /// Project-relative path with `/` separators.
    pub path: String,
    /// 1-based line number.
    pub line: u64,
    /// Line text (without trailing newline).
    pub text: String,
}

/// Tools bound to one trusted project root and one forbidden data root.
#[derive(Debug, Clone)]
pub struct Files {
    root: PathBuf,
    data_root: PathBuf,
}

impl Files {
    /// Bind roots. Both must be absolute; data root may equal root only if
    /// the project itself is rejected — instead require them disjoint.
    pub fn new(project_root: &Path, data_root: &Path) -> Result<Self, FileToolError> {
        if !project_root.is_absolute() || !data_root.is_absolute() {
            return Err(FileToolError::OutsideRoot);
        }
        Ok(Self {
            root: project_root.to_path_buf(),
            data_root: data_root.to_path_buf(),
        })
    }

    /// Bounded file/range read.
    pub fn read(&self, path: &str, offset: u64, limit: usize) -> Result<ReadResult, FileToolError> {
        let abs = self.resolve(path)?;
        let meta = std::fs::symlink_metadata(&abs).map_err(|_| FileToolError::NotFound)?;
        if meta.file_type().is_dir() {
            return Err(FileToolError::InvalidPattern("is a directory".to_string()));
        }
        if !meta.file_type().is_file() {
            return Err(FileToolError::NotFound);
        }
        let bytes = std::fs::read(&abs).map_err(|_| FileToolError::Io)?;
        if bytes.contains(&0) {
            return Err(FileToolError::Binary);
        }
        let text = String::from_utf8_lossy(&bytes);
        let all: Vec<&str> = text.lines().collect();
        let start = offset.saturating_sub(1).min(all.len() as u64) as usize;
        let want = limit.clamp(1, READ_LINES_CAP);
        let mut used = 0usize;
        let mut lines = Vec::new();
        for line in all.iter().skip(start).take(want) {
            used += line.len() + 1;
            if used > READ_BYTES_CAP && !lines.is_empty() {
                break;
            }
            lines.push(line.to_string());
        }
        let end = start + lines.len();
        let truncated = end < all.len();
        Ok(ReadResult {
            offset: offset.max(1),
            lines,
            truncated,
            next_offset: truncated.then_some(end as u64 + 1),
        })
    }

    /// Stable sorted paginated glob over relative paths.
    pub fn glob(
        &self,
        pattern: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, FileToolError> {
        if pattern.is_empty() || pattern.contains('\0') {
            return Err(FileToolError::InvalidPattern("empty pattern".to_string()));
        }
        let mut all = Vec::new();
        let mut walked = 0usize;
        self.walk("".to_string(), &mut walked, &mut all)?;
        let mut hits: Vec<String> = all.into_iter().filter(|p| glob_match(pattern, p)).collect();
        hits.sort();
        let limit = limit.clamp(1, 1000);
        Ok(hits.into_iter().skip(offset).take(limit).collect())
    }

    /// Stable sorted paginated grep (literal substring or `regex:` prefix).
    ///
    /// Regex mode is intentionally unsupported in T08: patterns starting
    /// with `regex:` return `InvalidPattern` until a vetted engine is pinned.
    /// Callers use literal mode; a future task wires the pinned engine.
    pub fn grep(
        &self,
        pattern: &str,
        literal: bool,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<GrepHit>, FileToolError> {
        if pattern.is_empty() {
            return Err(FileToolError::InvalidPattern("empty pattern".to_string()));
        }
        if !literal {
            return Err(FileToolError::InvalidPattern(
                "regex mode unsupported in T08; pass literal=true".to_string(),
            ));
        }
        let mut files = Vec::new();
        let mut walked = 0usize;
        self.walk("".to_string(), &mut walked, &mut files)?;
        files.sort();
        let limit = limit.clamp(1, 1000);
        let mut hits = Vec::new();
        let mut skipped = 0usize;
        for rel in files {
            if hits.len() >= limit {
                break;
            }
            let abs = self.root.join(&rel);
            let meta = std::fs::symlink_metadata(&abs).map_err(|_| FileToolError::Io)?;
            if meta.len() > GREP_FILE_BYTES_CAP {
                continue;
            }
            let bytes = std::fs::read(&abs).map_err(|_| FileToolError::Io)?;
            if bytes.contains(&0) {
                continue;
            }
            let text = String::from_utf8_lossy(&bytes);
            for (idx, line) in text.lines().enumerate() {
                if line.contains(pattern) {
                    if skipped < offset {
                        skipped += 1;
                        continue;
                    }
                    hits.push(GrepHit {
                        path: rel.clone(),
                        line: idx as u64 + 1,
                        text: line.to_string(),
                    });
                    if hits.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(hits)
    }

    /// Public path resolution for sibling tools (e.g. `apply_patch`).
    ///
    /// Same rules: lexical walk, no-follow symlink refusal, data-root and
    /// outside-root rejection.
    pub fn resolve_path(&self, path: &str) -> Result<PathBuf, FileToolError> {
        self.resolve(path)
    }

    /// Lexical walk from the project root so `..` recursion keeps its real
    /// landing (data root → `OwnDataRoot`, elsewhere outside → `OutsideRoot`),
    /// no-follow symlink refusal, and own-data-root rejection for direct and
    /// symlink-escape landings.
    fn resolve(&self, path: &str) -> Result<PathBuf, FileToolError> {
        if path.is_empty() {
            return Err(FileToolError::NotFound);
        }
        let mut abs = self.root.clone();
        for comp in Path::new(path).components() {
            match comp {
                Component::Prefix(_) | Component::RootDir => {
                    return Err(FileToolError::OutsideRoot);
                }
                Component::ParentDir => {
                    abs.pop();
                }
                Component::CurDir => {}
                Component::Normal(part) => abs.push(part),
            }
        }
        // No-follow: inspect prefixes at/below our roots; a link landing
        // inside the data root is an escape attempt, others are refused.
        let mut prefix = PathBuf::from("/");
        for comp in abs.components() {
            match comp {
                Component::Prefix(_) | Component::RootDir | Component::CurDir => continue,
                Component::ParentDir => {
                    prefix.pop();
                    continue;
                }
                Component::Normal(_) => {}
            }
            prefix.push(comp);
            if !prefix.starts_with(&self.root) && !prefix.starts_with(&self.data_root) {
                continue;
            }
            if let Ok(meta) = std::fs::symlink_metadata(&prefix)
                && meta.file_type().is_symlink()
                && let Ok(target) = std::fs::read_link(&prefix)
            {
                let abs_target = if target.is_absolute() {
                    target
                } else {
                    prefix.parent().unwrap_or(&self.root).join(target)
                };
                if abs_target.starts_with(&self.data_root) {
                    return Err(FileToolError::OwnDataRoot);
                }
                return Err(FileToolError::SymlinkEscape);
            }
            if let Ok(meta) = std::fs::symlink_metadata(&prefix)
                && meta.file_type().is_symlink()
            {
                return Err(FileToolError::SymlinkEscape);
            }
        }
        if abs.starts_with(&self.data_root) {
            return Err(FileToolError::OwnDataRoot);
        }
        if !abs.starts_with(&self.root) {
            return Err(FileToolError::OutsideRoot);
        }
        Ok(abs)
    }

    fn walk(
        &self,
        rel: String,
        walked: &mut usize,
        out: &mut Vec<String>,
    ) -> Result<(), FileToolError> {
        if *walked > WALK_FILES_CAP {
            return Err(FileToolError::BudgetExhausted);
        }
        let abs = if rel.is_empty() {
            self.root.clone()
        } else {
            self.root.join(&rel)
        };
        let entries = std::fs::read_dir(&abs).map_err(|_| FileToolError::Io)?;
        let mut names: BTreeMap<String, PathBuf> = BTreeMap::new();
        for entry in entries {
            let entry = entry.map_err(|_| FileToolError::Io)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            names.insert(name, entry.path());
        }
        for (name, path) in names {
            *walked += 1;
            if *walked > WALK_FILES_CAP {
                return Err(FileToolError::BudgetExhausted);
            }
            // Never descend into the own data root or follow symlinks.
            if path.starts_with(&self.data_root) {
                continue;
            }
            if let Ok(meta) = std::fs::symlink_metadata(&path) {
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.file_type().is_dir() {
                    let child = if rel.is_empty() {
                        name
                    } else {
                        format!("{rel}/{name}")
                    };
                    self.walk(child, walked, out)?;
                    continue;
                }
            }
            let rel_path = if rel.is_empty() {
                name
            } else {
                format!("{rel}/{name}")
            };
            out.push(rel_path);
        }
        Ok(())
    }
}

/// Minimal glob matcher supporting `*`, `?` and `**` (slash-aware).
pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let pat_segs: Vec<&str> = pattern.split('/').collect();
    let path_segs: Vec<&str> = path.split('/').collect();
    match_segments(&pat_segs, &path_segs)
}

fn match_segments(pat: &[&str], path: &[&str]) -> bool {
    if pat.is_empty() {
        return path.is_empty();
    }
    if pat[0] == "**" {
        return (0..=path.len()).any(|i| match_segments(&pat[1..], &path[i..]));
    }
    if path.is_empty() {
        return false;
    }
    if match_segment(pat[0], path[0]) {
        return match_segments(&pat[1..], &path[1..]);
    }
    false
}

fn match_segment(pat: &str, text: &str) -> bool {
    let (mut p, mut t) = (pat.as_bytes(), text.as_bytes());
    let mut star: Option<&[u8]> = None;
    let mut mark: &[u8] = b"";
    loop {
        match (p.first(), t.first()) {
            (Some(b'*'), _) => {
                star = Some(&p[1..]);
                mark = t;
                p = &p[1..];
            }
            (Some(b'?'), Some(_)) => {
                p = &p[1..];
                t = &t[1..];
            }
            (Some(a), Some(b)) if a == b => {
                p = &p[1..];
                t = &t[1..];
            }
            _ => {
                if let Some(rest) = star {
                    if mark.is_empty() {
                        return false;
                    }
                    mark = &mark[1..];
                    t = mark;
                    p = rest;
                } else {
                    return p.is_empty() && t.is_empty();
                }
            }
        }
        if p.is_empty() && t.is_empty() {
            return true;
        }
        if p.is_empty() && star.is_none() {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_PAGE_LIMIT, FileToolError, Files};
    use std::fs;
    use std::os::unix::fs::symlink;

    fn setup() -> (tempfile::TempDir, Files) {
        let tmp = tempfile::tempdir().expect("temp");
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        fs::create_dir_all(&project).expect("project");
        fs::create_dir_all(&data).expect("data");
        let files = Files::new(&project, &data).expect("bind");
        (tmp, files)
    }

    fn project_of(files: &Files) -> std::path::PathBuf {
        files.root.clone()
    }

    #[test]
    fn tool01_bounded_chunks_with_cursor() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        let body = (1..=50)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(root.join("a.txt"), body).expect("write");
        let first = files.read("a.txt", 1, 10).expect("read");
        assert_eq!(first.offset, 1);
        assert_eq!(first.lines.len(), 10);
        assert_eq!(first.lines[0], "line 1");
        assert!(first.truncated);
        let next = first.next_offset.expect("cursor");
        let second = files.read("a.txt", next, 100).expect("read2");
        assert_eq!(second.lines[0], "line 11");
        assert_eq!(second.lines.len(), 40);
        assert!(!second.truncated);
        assert_eq!(second.next_offset, None);
    }

    #[test]
    fn tool01_glob_sorted_paginated() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        for name in ["b.txt", "a.txt", "c.txt", "sub/d.txt"] {
            let p = root.join(name);
            fs::create_dir_all(p.parent().expect("parent")).expect("dir");
            fs::write(p, "x").expect("write");
        }
        let page1 = files.glob("*.txt", 0, 2).expect("glob");
        assert_eq!(page1, vec!["a.txt".to_string(), "b.txt".to_string()]);
        let page2 = files.glob("*.txt", 2, 2).expect("glob2");
        assert_eq!(page2, vec!["c.txt".to_string()]);
        let deep = files.glob("**/*.txt", 0, DEFAULT_PAGE_LIMIT).expect("deep");
        assert_eq!(deep.len(), 4);
        assert_eq!(deep[0], "a.txt");
    }

    #[test]
    fn tool01_grep_literal_sorted_paginated() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        fs::write(root.join("one.txt"), "foo bar\nfoo baz\n").expect("w1");
        fs::write(root.join("two.txt"), "hello foo\n").expect("w2");
        let hits = files.glob("*.txt", 0, 10).expect("glob");
        assert_eq!(hits.len(), 2);
        let found = files.grep("foo", true, 0, 10).expect("grep");
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].path, "one.txt");
        assert_eq!(found[0].line, 1);
        assert_eq!(found[2].path, "two.txt");
        let page = files.grep("foo", true, 2, 10).expect("page");
        assert_eq!(page.len(), 1);
        // Regex mode is refused until a vetted engine is pinned.
        assert!(matches!(
            files.grep("f.o", false, 0, 10),
            Err(FileToolError::InvalidPattern(_))
        ));
    }

    #[test]
    fn tool01_binary_and_large_diagnostics() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        fs::write(root.join("bin.dat"), [0x41, 0x00, 0x42]).expect("bin");
        assert_eq!(files.read("bin.dat", 1, 10), Err(FileToolError::Binary));
        let big = "y".repeat(super::READ_BYTES_CAP + 1000);
        fs::write(root.join("big.txt"), format!("{big}\n tail")).expect("big");
        let chunk = files
            .read("big.txt", 1, super::READ_LINES_CAP)
            .expect("read");
        assert!(chunk.truncated || chunk.lines[0].len() <= super::READ_BYTES_CAP + 1);
    }

    #[test]
    fn tool01_own_data_root_refused_all_shapes() {
        let (tmp, files) = setup();
        let data = tmp.path().join("data");
        fs::write(data.join("secret.txt"), "s3cret").expect("secret");
        // Direct path via .. recursion.
        assert_eq!(
            files.read("../data/secret.txt", 1, 10),
            Err(FileToolError::OwnDataRoot)
        );
        // Absolute path outside root.
        assert_eq!(
            files.read("/etc/hostname", 1, 10),
            Err(FileToolError::OutsideRoot)
        );
        // Symlink escape from project into the data root.
        let root = project_of(&files);
        symlink(data.join("secret.txt"), root.join("evil.txt")).expect("link");
        assert_eq!(
            files.read("evil.txt", 1, 10),
            Err(FileToolError::OwnDataRoot)
        );
        // Glob/grep never surface data-root contents either.
        let hits = files.glob("**/*", 0, DEFAULT_PAGE_LIMIT).expect("glob");
        assert!(!hits.iter().any(|h| h.contains("secret")));
        let found = files.grep("s3cret", true, 0, 10).expect("grep");
        assert!(found.is_empty());
    }
}
