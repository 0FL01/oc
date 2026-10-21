//! Search semantics use the same regex/ignore engines as ripgrep. Traversal and
//! ignore-file reads remain descriptor-relative, bounded and no-follow.
use super::*;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::overrides::{Override, OverrideBuilder};
use regex::RegexBuilder;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const SEARCH_DEPTH_CAP: usize = 64;
const REGEX_COMPILE_BYTES_CAP: usize = 10 * 1024 * 1024;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(crate) struct GlobOptions<'a> {
    pub pattern: &'a str,
    pub path: &'a str,
    pub hidden: bool,
    pub offset: usize,
    pub limit: usize,
}

pub(crate) struct GrepOptions<'a> {
    pub pattern: &'a str,
    pub path: &'a str,
    pub include: Option<&'a str>,
    pub literal: bool,
    pub case_sensitive: bool,
    pub offset: usize,
    pub limit: usize,
}

pub(crate) fn validate_pattern(pattern: &str) -> Result<(), FileToolError> {
    if pattern.is_empty() || pattern.contains('\0') || pattern.len() > SEARCH_PATTERN_BYTES_CAP {
        return Err(FileToolError::InvalidPattern(
            "empty, NUL or oversized pattern".into(),
        ));
    }
    Ok(())
}

fn overrides(pattern: Option<&str>, hidden: bool) -> Result<Override, FileToolError> {
    let mut builder = OverrideBuilder::new("");
    if let Some(pattern) = pattern {
        validate_pattern(pattern)?;
        if pattern.split('/').count() > GLOB_SEGMENTS_CAP {
            return Err(FileToolError::InvalidPattern(
                "too many glob segments".into(),
            ));
        }
        builder
            .add(pattern)
            .map_err(|_| FileToolError::InvalidPattern("invalid glob".into()))?;
    }
    if !hidden {
        builder.add("!**/.*").expect("fixed hidden exclusion");
    }
    builder.add("!**/.git/**").expect("fixed git exclusion");
    builder
        .build()
        .map_err(|_| FileToolError::InvalidPattern("glob complexity limit".into()))
}

fn regex(options: &GrepOptions<'_>) -> Result<regex::Regex, FileToolError> {
    validate_pattern(options.pattern)?;
    let pattern = if options.literal {
        regex::escape(options.pattern)
    } else {
        options.pattern.into()
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .size_limit(REGEX_COMPILE_BYTES_CAP)
        .dfa_size_limit(REGEX_COMPILE_BYTES_CAP)
        .build()
        // Library diagnostics echo the expression, which can contain secrets.
        .map_err(|error| match error {
            regex::Error::CompiledTooBig(limit) => Budget::new(None).exhausted(
                "regex_compile",
                "compile",
                limit as u64,
                limit as u64 + 1,
            ),
            _ => FileToolError::InvalidPattern("invalid or unsupported regex".into()),
        })
}

impl GlobOptions<'_> {
    pub(crate) fn validate(&self) -> Result<(), FileToolError> {
        overrides(Some(self.pattern), self.hidden).map(|_| ())
    }
}

impl GrepOptions<'_> {
    pub(crate) fn validate(&self) -> Result<(), FileToolError> {
        regex(self)?;
        overrides(self.include, true).map(|_| ())
    }
}

pub(super) struct Budget<'a> {
    start: Instant,
    entries: usize,
    bytes: u64,
    cancel: Option<&'a AtomicBool>,
    #[cfg(test)]
    barrier: Option<std::sync::Arc<ScanBarrier>>,
}

#[cfg(test)]
pub(super) fn read_checkpoint(files: &Files, cancel: &AtomicBool) -> Result<(), FileToolError> {
    let mut budget = Budget::new(Some(cancel));
    budget.barrier = files.scan_barrier.clone();
    budget.checkpoint("read")
}

impl<'a> Budget<'a> {
    pub(super) fn exhausted(
        &self,
        budget: &'static str,
        phase: &'static str,
        limit: u64,
        observed: u64,
    ) -> FileToolError {
        FileToolError::BudgetExhausted(FileBudget {
            budget,
            phase,
            limit,
            observed,
            scanned_entries: self.entries as u64,
            bytes: self.bytes,
            elapsed_ms: self.start.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
    pub(super) fn new(cancel: Option<&'a AtomicBool>) -> Self {
        Self {
            start: Instant::now(),
            entries: 0,
            bytes: 0,
            cancel,
            #[cfg(test)]
            barrier: None,
        }
    }
    pub(super) fn check(&self) -> Result<(), FileToolError> {
        if self
            .cancel
            .is_some_and(|cancel| cancel.load(Ordering::Acquire))
        {
            return Err(FileToolError::Cancelled);
        }
        if self.start.elapsed() >= SEARCH_TIMEOUT {
            Err(self.exhausted(
                "time",
                "scan",
                SEARCH_TIMEOUT.as_millis() as u64,
                self.start.elapsed().as_millis() as u64,
            ))
        } else if self.entries > WALK_FILES_CAP {
            Err(self.exhausted(
                "entries",
                "discovery",
                WALK_FILES_CAP as u64,
                self.entries as u64,
            ))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _phase: &'static str) -> Result<(), FileToolError> {
        #[cfg(test)]
        if let Some(barrier) = &self.barrier
            && barrier.phase == _phase
            && let Some(entered) = barrier.entered.lock().unwrap().take()
        {
            entered.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            let release = barrier.release.lock().unwrap();
            let released = loop {
                if self
                    .cancel
                    .is_some_and(|cancel| cancel.load(Ordering::Acquire))
                    && let Some(seen) = barrier.cancel_seen.lock().unwrap().take()
                {
                    seen.send(()).unwrap();
                }
                match release.recv_timeout(Duration::from_millis(1)) {
                    Ok(()) => break true,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                        if Instant::now() < deadline => {}
                    Err(_) => break false,
                }
            };
            barrier
                .released
                .store(released, std::sync::atomic::Ordering::Release);
            if !released {
                return Err(self.exhausted("time", "test_checkpoint", 2000, 2000));
            }
        }
        self.check()
    }
    fn read(&mut self, file: File, content: bool) -> Result<Option<Vec<u8>>, FileToolError> {
        self.check()?;
        let meta = file.metadata().map_err(|_| FileToolError::Io)?;
        if !meta.is_file() {
            return Ok(None);
        }
        if meta.len() > GREP_FILE_BYTES_CAP {
            return Err(self.exhausted(
                "file_bytes",
                if content { "content" } else { "ignore_file" },
                GREP_FILE_BYTES_CAP,
                meta.len(),
            ));
        }
        let remaining = GREP_SCAN_BYTES_CAP.saturating_sub(self.bytes);
        let mut bytes = Vec::new();
        let mut file = file.take(GREP_FILE_BYTES_CAP.min(remaining) + 1);
        let mut chunk = [0u8; 8192];
        loop {
            self.check()?;
            let read = file.read(&mut chunk).map_err(|_| FileToolError::Io)?;
            self.bytes += read as u64;
            self.check()?;
            if read == 0 {
                break;
            }
            if self.bytes > GREP_SCAN_BYTES_CAP {
                return Err(self.exhausted(
                    "scan_bytes",
                    if content { "content" } else { "ignore_file" },
                    GREP_SCAN_BYTES_CAP,
                    self.bytes,
                ));
            }
            if bytes.len() as u64 + read as u64 > GREP_FILE_BYTES_CAP {
                return Err(self.exhausted(
                    "file_bytes",
                    if content { "content" } else { "ignore_file" },
                    GREP_FILE_BYTES_CAP,
                    bytes.len() as u64 + read as u64,
                ));
            }
            // Binary candidates cannot contribute text hits. Stop within the
            // existing chunk budget; ignore files must still be fully parsed.
            if content && chunk[..read].contains(&0) {
                return Ok(None);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        self.check()?;
        Ok(Some(bytes))
    }
}

// Separate matcher classes preserve ripgrep precedence across directory depth:
// .rgignore > .ignore > .gitignore > .git/info/exclude; nearest/last wins.
struct IgnoreLevel {
    matchers: [Gitignore; 4],
    repository: bool,
    starts_repository: bool,
}

fn ignore_level(
    dir: &File,
    relative: &Path,
    repository: bool,
    budget: &mut Budget<'_>,
    allowed: &impl Fn(&Path) -> bool,
) -> Result<IgnoreLevel, FileToolError> {
    let starts_repository = suggest_openat(dir, c".git", libc::O_PATH).is_ok();
    let repository = repository || starts_repository;
    let mut matchers = Vec::new();
    for name in [".git/info/exclude", ".gitignore", ".ignore", ".rgignore"] {
        let mut builder = GitignoreBuilder::new(relative);
        if allowed(&relative.join(name))
            && match name {
                ".git/info/exclude" => starts_repository,
                ".gitignore" => repository,
                _ => true,
            }
            && let Ok(file) = open_relative(dir, Path::new(name), budget)
            && let Some(bytes) = budget.read(file, false)?
        {
            let text = std::str::from_utf8(&bytes).map_err(|_| {
                FileToolError::InvalidPattern("invalid ignore file encoding".into())
            })?;
            for line in text.lines() {
                budget.check()?;
                if line.len() > SEARCH_PATTERN_BYTES_CAP {
                    return Err(budget.exhausted(
                        "ignore_rule_bytes",
                        "ignore_rule",
                        SEARCH_PATTERN_BYTES_CAP as u64,
                        line.len() as u64,
                    ));
                }
                builder
                    .add_line(None, line)
                    .map_err(|_| FileToolError::InvalidPattern("invalid ignore rule".into()))?;
            }
        }
        budget.check()?;
        matchers.push(
            builder
                .build()
                .map_err(|_| FileToolError::InvalidPattern("ignore complexity limit".into()))?,
        );
        budget.check()?;
    }
    Ok(IgnoreLevel {
        matchers: matchers.try_into().expect("four ignore classes"),
        repository,
        starts_repository,
    })
}

fn ignored(levels: &[IgnoreLevel], path: &Path, is_dir: bool) -> bool {
    for class in (0..4).rev() {
        for level in levels.iter().rev() {
            let found = level.matchers[class].matched(path, is_dir);
            if !found.is_none() {
                return found.is_ignore();
            }
            if class <= 1 && level.starts_repository {
                break;
            }
        }
    }
    false
}

fn open_relative(root: &File, path: &Path, budget: &Budget<'_>) -> Result<File, FileToolError> {
    let mut dir = root.try_clone().map_err(|_| FileToolError::Io)?;
    let parts: Vec<_> = path.components().collect();
    for (index, component) in parts.iter().enumerate() {
        budget.check()?;
        let Component::Normal(name) = component else {
            return Err(FileToolError::OutsideRoot);
        };
        let name = CString::new(name.as_bytes()).map_err(|_| FileToolError::NotFound)?;
        let flags = if index + 1 == parts.len() {
            libc::O_RDONLY | libc::O_NONBLOCK
        } else {
            libc::O_RDONLY | libc::O_DIRECTORY
        };
        dir = suggest_openat(&dir, &name, flags).map_err(|_| FileToolError::SymlinkEscape)?;
    }
    Ok(dir)
}

impl Files {
    fn search_ancestors(
        &self,
        scope: &Path,
        budget: &mut Budget<'_>,
        allowed: &impl Fn(&Path) -> bool,
    ) -> Result<Vec<IgnoreLevel>, FileToolError> {
        // External scans observe only their explicitly admitted subtree. Never
        // load HOME/ancestor ignore files or discover additional roots.
        if !self.is_project_scope(scope) {
            return Ok(Vec::new());
        }
        let relative = scope
            .strip_prefix(&self.root)
            .map_err(|_| FileToolError::OutsideRoot)?;
        let mut levels = Vec::new();
        let mut directory = self.root.clone();
        let mut pinned = open_suggest_root(&directory)?;
        for component in relative.components() {
            budget.check()?;
            if levels.len() >= SEARCH_DEPTH_CAP {
                return Err(budget.exhausted(
                    "depth",
                    "ancestors",
                    SEARCH_DEPTH_CAP as u64,
                    levels.len() as u64,
                ));
            }
            let inherited = levels
                .last()
                .is_some_and(|level: &IgnoreLevel| level.repository);
            levels.push(ignore_level(
                &pinned,
                &directory,
                inherited,
                budget,
                &|path| !path.starts_with(&self.data_root) && allowed(path),
            )?);
            directory.push(component);
            let name = CString::new(component.as_os_str().as_bytes())
                .map_err(|_| FileToolError::NotFound)?;
            pinned = suggest_openat(&pinned, &name, libc::O_RDONLY | libc::O_DIRECTORY)
                .map_err(|_| FileToolError::SymlinkEscape)?;
        }
        Ok(levels)
    }

    pub(crate) fn search_scope(&self, path: &str) -> Result<PathBuf, FileToolError> {
        if self.external.is_some() {
            return self.resolve_read(path);
        }
        let relative = if Path::new(path).is_absolute() {
            Path::new(path)
                .strip_prefix(&self.root)
                .map_err(|_| FileToolError::OutsideRoot)?
                .to_str()
                .ok_or(FileToolError::NotFound)?
        } else {
            path
        };
        self.resolve(if relative.is_empty() { "." } else { relative })
    }

    pub(crate) fn glob_search(
        &self,
        options: &GlobOptions<'_>,
        allowed: impl Fn(&Path) -> bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<Vec<String>, FileToolError> {
        let overrides = overrides(Some(options.pattern), options.hidden)?;
        let mut budget = Budget::new(cancel);
        #[cfg(test)]
        {
            budget.barrier = self.scan_barrier.clone();
        }
        budget.check()?;
        let scope = self.search_scope(options.path)?;
        let root = self.open_read_scope(&scope, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let mut paths = Vec::new();
        let mut levels = self.search_ancestors(&scope, &mut budget, &allowed)?;
        self.search_walk(
            &root,
            &scope,
            Path::new(""),
            &overrides,
            &mut levels,
            &mut budget,
            &allowed,
            &mut paths,
        )?;
        paths.sort();
        budget.check()?;
        Ok(paths
            .into_iter()
            .skip(options.offset)
            .take(options.limit)
            .map(|path| self.relative_hit(&scope, &path))
            .collect())
    }

    pub(crate) fn grep_search(
        &self,
        options: &GrepOptions<'_>,
        allowed: impl Fn(&Path) -> bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<GrepResult, FileToolError> {
        let regex = regex(options)?;
        let overrides = overrides(options.include, true)?;
        let mut budget = Budget::new(cancel);
        #[cfg(test)]
        {
            budget.barrier = self.scan_barrier.clone();
        }
        budget.check()?;
        let scope = self.search_scope(options.path)?;
        // Pin every ancestor before examining file type. File scopes bypass ignore
        // filtering like rg's explicit file argument, but never data/policy guards.
        let parent = scope.parent().ok_or(FileToolError::OutsideRoot)?;
        // Directory scopes can equal the pinned external root: its parent is
        // deliberately outside the invocation authority.
        let entry = self.open_read_scope(&scope, libc::O_PATH)?;
        let meta = entry.metadata().map_err(|_| FileToolError::Io)?;
        let parent_fd = if meta.is_dir() {
            self.open_read_scope(&scope, libc::O_RDONLY | libc::O_DIRECTORY)?
        } else {
            self.open_read_scope(parent, libc::O_RDONLY | libc::O_DIRECTORY)?
        };
        let (root, base, mut paths) = if meta.is_dir() {
            let root = parent_fd;
            let mut paths = Vec::new();
            let mut levels = self.search_ancestors(&scope, &mut budget, &allowed)?;
            let discovery = self.search_walk(
                &root,
                &scope,
                Path::new(""),
                &overrides,
                &mut levels,
                &mut budget,
                &allowed,
                &mut paths,
            );
            if let Err(error) = discovery {
                if error == FileToolError::Cancelled {
                    return Err(error);
                }
                return Ok(GrepResult {
                    hits: Vec::new(),
                    incomplete: Some(error),
                    unsearched_files: 0,
                });
            }
            (root, scope, paths)
        } else if meta.is_file() && allowed(&scope) {
            (
                parent_fd,
                parent.to_path_buf(),
                vec![PathBuf::from(scope.file_name().expect("scope name"))],
            )
        } else {
            return Ok(GrepResult {
                hits: Vec::new(),
                incomplete: None,
                unsearched_files: 0,
            });
        };
        paths.sort();
        budget.check()?;
        let mut hits = Vec::new();
        let mut skipped = 0;
        let mut incomplete = None;
        let mut unsearched_files = 0;
        for path in paths {
            let bytes = match budget
                .check()
                .and_then(|()| open_relative(&root, &path, &budget))
                .and_then(|file| budget.read(file, true))
            {
                Ok(bytes) => bytes,
                Err(FileToolError::Cancelled) => return Err(FileToolError::Cancelled),
                Err(error) => {
                    let oversized = matches!(&error, FileToolError::BudgetExhausted(facts) if facts.budget == "file_bytes");
                    if oversized {
                        unsearched_files += 1;
                    }
                    if incomplete.is_none() || !oversized {
                        incomplete = Some(error);
                    }
                    if oversized {
                        continue;
                    }
                    break;
                }
            };
            let Some(bytes) = bytes else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            for (index, line) in text.lines().enumerate() {
                if let Err(error) = budget.check() {
                    if error == FileToolError::Cancelled {
                        return Err(error);
                    }
                    return Ok(GrepResult {
                        hits,
                        incomplete: Some(error),
                        unsearched_files,
                    });
                }
                if !regex.is_match(line) {
                    continue;
                }
                if skipped < options.offset {
                    skipped += 1;
                    continue;
                }
                let mut end = line.len().min(GREP_HIT_BYTES_CAP);
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                hits.push(GrepHit {
                    path: self.relative_hit(&base, &path),
                    line: index as u64 + 1,
                    text: line[..end].into(),
                    text_truncated: end < line.len(),
                });
                if let Err(error) = budget.checkpoint("hit") {
                    if error == FileToolError::Cancelled {
                        return Err(error);
                    }
                    return Ok(GrepResult {
                        hits,
                        incomplete: Some(error),
                        unsearched_files,
                    });
                }
                if hits.len() >= options.limit {
                    return Ok(GrepResult {
                        hits,
                        incomplete,
                        unsearched_files,
                    });
                }
            }
        }
        if let Err(error) = budget.check() {
            if error == FileToolError::Cancelled {
                return Err(error);
            }
            incomplete = Some(error);
        }
        Ok(GrepResult {
            hits,
            incomplete,
            unsearched_files,
        })
    }

    fn relative_hit(&self, scope: &Path, path: &Path) -> String {
        let absolute = scope.join(path);
        absolute
            .strip_prefix(&self.root)
            .unwrap_or(&absolute)
            .to_string_lossy()
            .into_owned()
    }

    #[allow(clippy::too_many_arguments)]
    fn search_walk(
        &self,
        dir: &File,
        scope: &Path,
        relative: &Path,
        overrides: &Override,
        levels: &mut Vec<IgnoreLevel>,
        budget: &mut Budget<'_>,
        allowed: &impl Fn(&Path) -> bool,
        paths: &mut Vec<PathBuf>,
    ) -> Result<(), FileToolError> {
        budget.check()?;
        if levels.len() >= SEARCH_DEPTH_CAP {
            return Err(budget.exhausted(
                "depth",
                "discovery",
                SEARCH_DEPTH_CAP as u64,
                levels.len() as u64,
            ));
        }
        let repository = levels.last().is_some_and(|level| level.repository);
        levels.push(ignore_level(
            dir,
            &scope.join(relative),
            repository,
            budget,
            &|path| !path.starts_with(&self.data_root) && allowed(path),
        )?);
        let mut truncated = false;
        let mut inspected = budget.entries;
        let mut names =
            suggest_dir_names(dir, &mut inspected, &mut truncated, WALK_FILES_CAP, || {
                budget.check()
            })?;
        budget.entries = inspected;
        if truncated {
            return Err(budget.exhausted(
                "entries",
                "discovery",
                WALK_FILES_CAP as u64,
                inspected as u64 + 1,
            ));
        }
        names.sort();
        budget.check()?;
        for name in names {
            budget.check()?;
            let child = relative.join(&name);
            let absolute = scope.join(&child);
            if absolute.starts_with(&self.data_root) || !allowed(&absolute) || name == ".git" {
                continue;
            }
            let name_c = CString::new(name.as_bytes()).map_err(|_| FileToolError::Io)?;
            let entry =
                suggest_openat(dir, &name_c, libc::O_PATH).map_err(|_| FileToolError::Io)?;
            let meta = entry.metadata().map_err(|_| FileToolError::Io)?;
            if meta.is_symlink() || (!meta.is_dir() && !meta.is_file()) {
                continue;
            }
            let matched = overrides.matched(&child, meta.is_dir());
            if matched.is_ignore()
                || (matched.is_none() && ignored(levels, &absolute, meta.is_dir()))
            {
                continue;
            }
            if meta.is_dir() {
                let pinned = suggest_openat(dir, &name_c, libc::O_RDONLY | libc::O_DIRECTORY)
                    .map_err(|_| FileToolError::SymlinkEscape)?;
                self.search_walk(
                    &pinned, scope, &child, overrides, levels, budget, allowed, paths,
                )?;
            } else {
                paths.push(child);
                budget.checkpoint("candidate")?;
            }
        }
        levels.pop();
        Ok(())
    }
}
