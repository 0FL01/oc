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
use std::ffi::{CStr, CString, OsString};
use std::fs::File;
use std::io::Read as _;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::OpenOptionsExt as _;
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
/// Aggregate bytes inspected by one grep call.
pub const GREP_SCAN_BYTES_CAP: u64 = 16 * 1024 * 1024;
/// Maximum model-supplied glob/grep pattern bytes.
pub const SEARCH_PATTERN_BYTES_CAP: usize = 4096;
/// Maximum slash-delimited glob segments.
pub const GLOB_SEGMENTS_CAP: usize = 64;
/// Maximum text bytes retained in one grep hit.
pub const GREP_HIT_BYTES_CAP: usize = 2048;
/// Default page size for glob/grep.
pub const DEFAULT_PAGE_LIMIT: usize = 50;
/// Maximum directory entries inspected for one file suggestion query.
pub const SUGGEST_ENTRIES_CAP: usize = 2048;
/// Maximum paths returned by one file suggestion query.
pub const SUGGEST_RESULTS_CAP: usize = 20;
const SUGGEST_QUERY_BYTES_CAP: usize = 256;
const SUGGEST_PATH_BYTES_CAP: usize = 4096;
const SUGGEST_DEPTH_CAP: usize = 64;

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

/// Location-relative file suggestions; `truncated` indicates more matches or an incomplete scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSuggestionResult {
    /// Sorted project-relative paths with `/` separators.
    pub paths: Vec<String>,
    /// More matches exist, or the scan was incomplete.
    pub truncated: bool,
}

/// Tools bound to one trusted project root and one forbidden data root.
#[derive(Debug, Clone)]
pub struct Files {
    root: PathBuf,
    data_root: PathBuf,
}

fn normalize_suggest_root(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::from("/");
    for part in path.components() {
        match part {
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
            _ => {}
        }
    }
    normalized
}

fn suggest_openat(dir: &File, name: &CStr, flags: libc::c_int) -> std::io::Result<File> {
    // SAFETY: the borrowed directory fd and NUL-terminated component remain valid.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: openat returned a new owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

fn open_suggest_root(path: &Path) -> Result<File, FileToolError> {
    let mut dir = File::open("/").map_err(|_| FileToolError::Io)?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            continue;
        };
        let name = CString::new(name.as_bytes()).map_err(|_| FileToolError::Io)?;
        dir = suggest_openat(&dir, &name, libc::O_RDONLY | libc::O_DIRECTORY).map_err(|_| {
            if suggest_openat(&dir, &name, libc::O_PATH)
                .and_then(|entry| entry.metadata())
                .is_ok_and(|meta| meta.file_type().is_symlink())
            {
                FileToolError::SymlinkEscape
            } else {
                FileToolError::Io
            }
        })?;
    }
    Ok(dir)
}

struct SuggestDir(*mut libc::DIR);

impl Drop for SuggestDir {
    fn drop(&mut self) {
        // SAFETY: fdopendir transferred ownership of the duplicated descriptor.
        unsafe {
            libc::closedir(self.0);
        }
    }
}

fn suggest_dir_names(
    dir: &File,
    inspected: &mut usize,
    truncated: &mut bool,
) -> Result<Vec<OsString>, FileToolError> {
    // fdopendir consumes its fd; duplicate so the caller keeps its pinned directory.
    // SAFETY: dir owns a live directory descriptor during this call.
    let fd = unsafe { libc::fcntl(dir.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        return Err(FileToolError::Io);
    }
    // SAFETY: fd is a new directory descriptor owned by fdopendir on success.
    let raw = unsafe { libc::fdopendir(fd) };
    if raw.is_null() {
        // SAFETY: fdopendir failed and did not take ownership of fd.
        unsafe {
            libc::close(fd);
        }
        return Err(FileToolError::Io);
    }
    let reader = SuggestDir(raw);
    let mut names = Vec::new();
    loop {
        // SAFETY: errno is thread-local and this thread is the only writer here.
        let errno = unsafe { libc::__errno_location() };
        // SAFETY: errno points to this thread's live error slot.
        unsafe {
            *errno = 0;
        }
        // SAFETY: reader owns a live DIR; its returned entry is valid until the next call.
        let entry = unsafe { libc::readdir(reader.0) };
        if entry.is_null() {
            // SAFETY: errno points to this thread's live error slot.
            if unsafe { *errno } != 0 {
                *truncated = true;
            }
            break;
        }
        // SAFETY: readdir returned a live dirent with a NUL-terminated name.
        let name_ptr = unsafe { (*entry).d_name.as_ptr() };
        // SAFETY: name_ptr remains valid until the next readdir call.
        let name = unsafe { CStr::from_ptr(name_ptr) }.to_bytes();
        if name == b"." || name == b".." {
            continue;
        }
        if *inspected >= SUGGEST_ENTRIES_CAP {
            *truncated = true;
            break;
        }
        *inspected += 1;
        names.push(OsString::from(std::ffi::OsStr::from_bytes(name)));
    }
    Ok(names)
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
        if pattern.is_empty()
            || pattern.contains('\0')
            || pattern.len() > SEARCH_PATTERN_BYTES_CAP
            || pattern.split('/').count() > GLOB_SEGMENTS_CAP
        {
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

    /// Suggest files under this Location without reading file contents.
    ///
    /// Returns the lexicographically smallest matches among bounded inspected
    /// entries. An empty query lists the first matches.
    pub fn suggest(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<FileSuggestionResult, FileToolError> {
        if query.len() > SUGGEST_QUERY_BYTES_CAP
            || query.starts_with('/')
            || query.contains(|c: char| c.is_control() || c.is_whitespace() || c == '\\')
            || query.split('/').any(|part| part == "." || part == "..")
            || query.contains("//")
        {
            return Err(FileToolError::InvalidPattern(
                "invalid file suggestion query".to_string(),
            ));
        }
        let root = normalize_suggest_root(&self.root);
        let data_root = std::fs::canonicalize(&self.data_root)
            .unwrap_or_else(|_| normalize_suggest_root(&self.data_root));
        if root.starts_with(&data_root) {
            return Ok(FileSuggestionResult {
                paths: Vec::new(),
                truncated: false,
            });
        }
        let mut result = FileSuggestionResult {
            paths: Vec::new(),
            truncated: false,
        };
        let mut inspected = 0;
        let root_dir = open_suggest_root(&root)?;
        Self::suggest_walk(
            (&root, &data_root),
            &root_dir,
            Path::new(""),
            0,
            &mut inspected,
            (query, limit.clamp(1, SUGGEST_RESULTS_CAP)),
            &mut result,
        )?;
        Ok(result)
    }

    fn suggest_walk(
        roots: (&Path, &Path),
        dir: &File,
        rel: &Path,
        depth: usize,
        inspected: &mut usize,
        (query, limit): (&str, usize),
        result: &mut FileSuggestionResult,
    ) -> Result<(), FileToolError> {
        let (root, data_root) = roots;
        let mut names = suggest_dir_names(dir, inspected, &mut result.truncated)?;
        names.sort();
        for name in names {
            let Some(name_str) = name.to_str() else {
                continue;
            };
            let child = rel.join(name_str);
            let path = root.join(&child);
            if path.starts_with(data_root) {
                continue;
            }
            let Some(relative) = child.to_str() else {
                continue;
            };
            if relative.len() > SUGGEST_PATH_BYTES_CAP {
                result.truncated = true;
                continue;
            }
            let Ok(name_c) = CString::new(name.as_bytes()) else {
                continue;
            };
            let Ok(entry) = suggest_openat(dir, &name_c, libc::O_PATH) else {
                result.truncated = true;
                continue;
            };
            let Ok(meta) = entry.metadata() else {
                result.truncated = true;
                continue;
            };
            if meta.file_type().is_dir() {
                if matches!(
                    name_str,
                    ".git"
                        | ".hg"
                        | ".svn"
                        | "target"
                        | "node_modules"
                        | "dist"
                        | "build"
                        | ".next"
                        | ".turbo"
                ) {
                    continue;
                }
                if depth >= SUGGEST_DEPTH_CAP {
                    result.truncated = true;
                    continue;
                }
                let Ok(child_dir) =
                    suggest_openat(dir, &name_c, libc::O_RDONLY | libc::O_DIRECTORY)
                else {
                    result.truncated = true;
                    continue;
                };
                // An entry may change between O_PATH and the directory open;
                // only the second, pinned descriptor is used for traversal.
                if let Err(_err) = Self::suggest_walk(
                    roots,
                    &child_dir,
                    &child,
                    depth + 1,
                    inspected,
                    (query, limit),
                    result,
                ) {
                    result.truncated = true;
                }
            } else if meta.file_type().is_file() && relative.contains(query) {
                match result.paths.binary_search_by(|p| p.as_str().cmp(relative)) {
                    Ok(_) => {}
                    Err(index) if index < limit => {
                        result.paths.insert(index, relative.to_string());
                        if result.paths.len() > limit {
                            result.paths.pop();
                            result.truncated = true;
                        }
                    }
                    Err(_) => result.truncated = true,
                }
            }
        }
        Ok(())
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
        if pattern.is_empty() || pattern.len() > SEARCH_PATTERN_BYTES_CAP {
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
        let mut scanned = 0u64;
        for rel in files {
            if hits.len() >= limit {
                break;
            }
            let abs = self.root.join(&rel);
            let meta = std::fs::symlink_metadata(&abs).map_err(|_| FileToolError::Io)?;
            if meta.len() > GREP_FILE_BYTES_CAP {
                continue;
            }
            let remaining = GREP_SCAN_BYTES_CAP.saturating_sub(scanned);
            if remaining == 0 {
                return Err(FileToolError::BudgetExhausted);
            }
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&abs)
                .map_err(|_| FileToolError::Io)?;
            if !file
                .metadata()
                .map_err(|_| FileToolError::Io)?
                .file_type()
                .is_file()
            {
                continue;
            }
            let read_cap = GREP_FILE_BYTES_CAP.min(remaining).saturating_add(1);
            let mut bytes = Vec::new();
            file.by_ref()
                .take(read_cap)
                .read_to_end(&mut bytes)
                .map_err(|_| FileToolError::Io)?;
            if bytes.len() as u64 > remaining {
                return Err(FileToolError::BudgetExhausted);
            }
            scanned = scanned.saturating_add(bytes.len() as u64);
            if bytes.len() as u64 > GREP_FILE_BYTES_CAP {
                continue;
            }
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
                    let mut text = line.to_string();
                    if text.len() > GREP_HIT_BYTES_CAP {
                        let mut end = GREP_HIT_BYTES_CAP;
                        while !text.is_char_boundary(end) {
                            end -= 1;
                        }
                        text.truncate(end);
                    }
                    hits.push(GrepHit {
                        path: rel.clone(),
                        line: idx as u64 + 1,
                        text,
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
            *walked += 1;
            if *walked > WALK_FILES_CAP {
                return Err(FileToolError::BudgetExhausted);
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            names.insert(name, entry.path());
        }
        for (name, path) in names {
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
                if !meta.file_type().is_file() {
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
    let mut memo = std::collections::HashMap::new();
    match_segments(&pat_segs, &path_segs, 0, 0, &mut memo)
}

fn match_segments(
    pat: &[&str],
    path: &[&str],
    p: usize,
    s: usize,
    memo: &mut std::collections::HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(result) = memo.get(&(p, s)) {
        return *result;
    }
    let result = if p == pat.len() {
        s == path.len()
    } else if pat[p] == "**" {
        match_segments(pat, path, p + 1, s, memo)
            || (s < path.len() && match_segments(pat, path, p, s + 1, memo))
    } else {
        s < path.len()
            && match_segment(pat[p], path[s])
            && match_segments(pat, path, p + 1, s + 1, memo)
    };
    memo.insert((p, s), result);
    result
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
    fn suggest_sorted_relative_filtered_and_skips_vcs_build_directories() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        for name in [
            "zeta.rs",
            "src/beta.rs",
            "src/alpha.rs",
            "src/note.txt",
            ".git/private.rs",
            ".hg/private.rs",
            ".svn/private.rs",
            "target/private.rs",
            "node_modules/private.rs",
            "dist/private.rs",
            "build/private.rs",
            ".next/private.rs",
            ".turbo/private.rs",
        ] {
            let path = root.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "not inspected").unwrap();
        }
        let all = files.suggest("", 20).unwrap();
        assert_eq!(
            all.paths,
            ["src/alpha.rs", "src/beta.rs", "src/note.txt", "zeta.rs"]
        );
        assert!(!all.truncated);
        assert_eq!(
            files.suggest("src/", 20).unwrap().paths,
            ["src/alpha.rs", "src/beta.rs", "src/note.txt"]
        );
        assert_eq!(files.suggest(".rs", 20).unwrap().paths.len(), 3);
        // The suggestion-specific excludes must not alter the existing glob.
        assert_eq!(files.glob(".git/*.rs", 0, 20).unwrap(), [".git/private.rs"]);
    }

    #[test]
    fn suggest_refuses_symlinks_and_own_data_root() {
        let (tmp, files) = setup();
        let root = project_of(&files);
        fs::write(root.join("visible.rs"), "ok").unwrap();
        fs::write(tmp.path().join("data/secret.rs"), "secret").unwrap();
        symlink(tmp.path().join("data/secret.rs"), root.join("file-link.rs")).unwrap();
        symlink(tmp.path().join("data"), root.join("dir-link")).unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("outside.rs"), "outside").unwrap();
        symlink(&outside, root.join("outside-link")).unwrap();
        let nested_data = root.join("private");
        fs::create_dir(&nested_data).unwrap();
        fs::write(nested_data.join("secret.rs"), "secret").unwrap();
        let nested = Files::new(&root, &nested_data).unwrap();
        assert_eq!(nested.suggest("", 20).unwrap().paths, ["visible.rs"]);
        assert_eq!(
            files.suggest("", 20).unwrap().paths,
            ["private/secret.rs", "visible.rs"]
        );

        let root_link = tmp.path().join("project-link");
        symlink(&root, &root_link).unwrap();
        assert_eq!(
            Files::new(&root_link, &tmp.path().join("data"))
                .unwrap()
                .suggest("", 20),
            Err(FileToolError::SymlinkEscape)
        );
        let ancestor_link = tmp.path().join("ancestor-link");
        symlink(tmp.path(), &ancestor_link).unwrap();
        assert_eq!(
            Files::new(&ancestor_link.join("project"), &tmp.path().join("data"))
                .unwrap()
                .suggest("", 20),
            Err(FileToolError::SymlinkEscape)
        );
        assert!(
            Files::new(&root.join("../data"), &tmp.path().join("data"))
                .unwrap()
                .suggest("", 20)
                .unwrap()
                .paths
                .is_empty()
        );
    }

    #[test]
    fn suggest_caps_results_and_entries_with_partial_results() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        for index in 0..=super::SUGGEST_RESULTS_CAP {
            fs::write(root.join(format!("match-{index:03}.rs")), "").unwrap();
        }
        let small = files.suggest("match", 2).unwrap();
        assert_eq!(small.paths.len(), 2);
        assert!(small.paths.windows(2).all(|w| w[0] < w[1]));
        assert!(small.truncated);
        let capped = files.suggest("match", usize::MAX).unwrap();
        assert_eq!(capped.paths.len(), super::SUGGEST_RESULTS_CAP);
        assert!(capped.truncated);

        for index in 0..super::SUGGEST_ENTRIES_CAP {
            fs::write(root.join(format!("other-{index:04}")), "").unwrap();
        }
        let exhausted = files.suggest("no-such-file", 20).unwrap();
        assert!(exhausted.paths.is_empty());
        assert!(exhausted.truncated);
    }

    #[test]
    fn suggest_returns_lexicographic_top_even_when_created_in_reverse_order() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        for index in (0..60).rev() {
            let path = root.join(format!("sub/{index:03}.rs"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        for index in (0..60).rev() {
            fs::write(root.join(format!("{index:03}.rs")), "").unwrap();
        }
        let matches = files.suggest(".rs", 3).unwrap();
        assert_eq!(matches.paths, ["000.rs", "001.rs", "002.rs"]);
        assert!(matches.truncated);
        let nested = files.suggest("sub/", 3).unwrap();
        assert_eq!(nested.paths, ["sub/000.rs", "sub/001.rs", "sub/002.rs"]);
        assert!(nested.truncated);
    }

    #[test]
    fn suggest_skips_unreadable_nested_directory_with_partial_result() {
        use std::os::unix::fs::PermissionsExt as _;
        // SAFETY: geteuid reads only the process credentials.
        if unsafe { libc::geteuid() } == 0 {
            return; // root can read a directory even with mode 000.
        }
        let (_tmp, files) = setup();
        let root = project_of(&files);
        fs::create_dir(root.join("blocked")).unwrap();
        fs::write(root.join("blocked/secret.rs"), "").unwrap();
        fs::write(root.join("good.rs"), "").unwrap();
        fs::set_permissions(root.join("blocked"), fs::Permissions::from_mode(0o0)).unwrap();
        let result = files.suggest(".rs", 20);
        fs::set_permissions(root.join("blocked"), fs::Permissions::from_mode(0o700)).unwrap();
        let result = result.unwrap();
        assert_eq!(result.paths, ["good.rs"]);
        assert!(result.truncated);
    }

    #[test]
    fn suggest_concurrent_symlink_swap_never_reports_outside_or_data_files() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let (tmp, files) = setup();
        let root = project_of(&files);
        let inside = root.join("slot");
        let parked = root.join("parked");
        let outside = tmp.path().join("outside");
        fs::create_dir(&inside).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(inside.join("safe.rs"), "").unwrap();
        fs::write(outside.join("outside-secret.rs"), "").unwrap();
        let data = tmp.path().join("data");
        fs::write(data.join("data-secret.rs"), "").unwrap();
        fs::write(root.join("good.rs"), "").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            for i in 0..400 {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                fs::rename(&inside, &parked).unwrap();
                symlink(if i % 2 == 0 { &outside } else { &data }, &inside).unwrap();
                std::thread::yield_now();
                fs::remove_file(&inside).unwrap();
                fs::rename(&parked, &inside).unwrap();
            }
        });
        for _ in 0..200 {
            let found = files.suggest(".rs", 20).unwrap();
            assert!(found.paths.contains(&"good.rs".to_string()));
            assert!(
                found.paths.iter().all(|p| !p.contains("secret")),
                "{found:?}"
            );
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().unwrap();
    }

    #[test]
    fn suggest_rejects_malformed_or_oversized_queries_without_root_in_error() {
        let (_tmp, files) = setup();
        for query in [
            "/etc/passwd".to_string(),
            "../data".to_string(),
            "src/./file".to_string(),
            "src//file".to_string(),
            "src\\file".to_string(),
            "a b".to_string(),
            "a\n".to_string(),
            "a\0".to_string(),
            "x".repeat(super::SUGGEST_QUERY_BYTES_CAP + 1),
        ] {
            let err = files.suggest(&query, 20).unwrap_err();
            assert!(matches!(err, FileToolError::InvalidPattern(_)));
            assert!(!err.to_string().contains(files.root.to_str().unwrap()));
        }
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
    fn aud19_search_patterns_scan_and_hit_text_are_byte_bounded() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        assert!(matches!(
            files.glob(&"x".repeat(super::SEARCH_PATTERN_BYTES_CAP + 1), 0, 1),
            Err(FileToolError::InvalidPattern(_))
        ));
        assert!(matches!(
            files.glob(&vec!["**"; super::GLOB_SEGMENTS_CAP + 1].join("/"), 0, 1),
            Err(FileToolError::InvalidPattern(_))
        ));
        fs::write(
            root.join("long.txt"),
            format!("needle {}", "🌍".repeat(super::GREP_HIT_BYTES_CAP)),
        )
        .unwrap();
        let hit = files.grep("needle", true, 0, 1).unwrap().remove(0);
        assert!(hit.text.len() <= super::GREP_HIT_BYTES_CAP);
        assert!(hit.text.is_char_boundary(hit.text.len()));

        fs::remove_file(root.join("long.txt")).unwrap();
        for index in 0..=super::GREP_SCAN_BYTES_CAP / super::GREP_FILE_BYTES_CAP {
            fs::write(
                root.join(format!("scan-{index:02}.txt")),
                vec![b'x'; super::GREP_FILE_BYTES_CAP as usize],
            )
            .unwrap();
        }
        assert_eq!(
            files.grep("absent", true, 0, 1),
            Err(FileToolError::BudgetExhausted)
        );
    }

    #[test]
    fn aud19_wide_directory_stops_at_walk_budget() {
        let (_tmp, files) = setup();
        let root = project_of(&files);
        for index in 0..=super::WALK_FILES_CAP {
            fs::write(root.join(format!("wide-{index:05}")), b"").unwrap();
        }
        assert_eq!(files.glob("*", 0, 1), Err(FileToolError::BudgetExhausted));
    }

    #[test]
    fn aud19_grep_skips_fifo_without_blocking() {
        use std::ffi::CString;

        let (_tmp, files) = setup();
        let fifo = project_of(&files).join("model-controlled.fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: the NUL-terminated path points into this test's temporary root.
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert!(files.grep("anything", true, 0, 10).unwrap().is_empty());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(files.glob("*", 0, 10).unwrap().is_empty());
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
