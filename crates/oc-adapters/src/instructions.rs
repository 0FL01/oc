//! Shared admitted AGENTS sources and chronological provider projection (R10).
//! Source bodies are instruction data, not configuration or permission grants.
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::provider::{InputItem, InputRole};
use crate::runtime::RuntimePolicy;
use crate::tools::ToolPolicy as _;

const SOURCES_CAP: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Origin {
    Global,
    Project,
    Nested,
}

/// Descriptor pinned by the same admission that owns config/definitions.
#[derive(Clone)]
pub(crate) struct Root {
    pub path: PathBuf,
    pub dir: Arc<File>,
    pub origin: Origin,
    /// Same admitted config generation as profiles/policy, never a later file
    /// reopen. Nested reads still use the pinned descriptor and live sources.
    pub baseline: Option<Arc<Source>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Source {
    pub path: String,
    pub root: String,
    pub origin: Origin,
    pub digest: Option<String>,
    pub content: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Fact {
    pub event: i64,
    pub source: Source,
    pub change: String,
    pub revision: u64,
}

/// Only source event identities/positions are copied into a turn journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reference {
    pub event: i64,
    pub index: usize,
}

impl Fact {
    pub fn input(&self) -> InputItem {
        // JSON quoting bounds/escapes the *label*, never interprets the body.
        let escape_label = |value: String| value.replace('<', "\\u003c").replace('>', "\\u003e");
        let path = escape_label(serde_json::to_string(&self.source.path).expect("path string"));
        let metadata = escape_label(
            serde_json::json!({"path":self.source.path,"root":self.source.root,
            "origin":self.source.origin,"digest":self.source.digest,
            "generation":self.source.generation,"revision":self.revision,
            "event":self.event,"change":self.change})
            .to_string(),
        );
        let text = match self.source.content.as_deref() {
            Some(content) => format!(
                "Instructions from: {path}\n<!-- oc-instructions-source: {} -->\n{content}",
                metadata
            ),
            None => format!(
                "The instructions from {path} no longer apply.\n<!-- oc-instructions-source: {metadata} -->"
            ),
        };
        InputItem::message(InputRole::Developer, text)
    }
}

pub(crate) fn validate_sources(sources: &[Source]) -> Result<(), String> {
    if sources.len() > SOURCES_CAP
        || sources
            .iter()
            .map(|s| s.content.as_ref().map_or(0, String::len))
            .sum::<usize>()
            > crate::defs::MAX_INSTRUCTIONS_TOTAL
    {
        return Err("instructions budget exceeded".into());
    }
    Ok(())
}

impl Source {
    pub fn baseline(path: &Path, root: &Path, origin: Origin, content: &str) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            root: root.to_string_lossy().into_owned(),
            origin,
            digest: Some(format!("{:x}", Sha256::digest(content.as_bytes()))),
            content: Some(content.into()),
            generation: 0,
        }
    }
}

/// Read only a resolved source with permanent Allow. All authority/data-root
/// checks precede the descriptor open; every component is no-follow.
#[allow(clippy::too_many_arguments)]
pub(crate) fn read_source(
    root: &Root,
    relative: &Path,
    origin: Origin,
    generation: u64,
    data_root: &Path,
    policy: &RuntimePolicy<'_>,
    cancel: &AtomicBool,
) -> Result<Source, String> {
    let path = root.path.join(relative);
    let mut source = Source {
        path: path.to_string_lossy().into_owned(),
        root: root.path.to_string_lossy().into_owned(),
        origin,
        digest: None,
        content: None,
        generation,
    };
    if cancel.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    if path.starts_with(data_root) || !policy.automatic_instruction_source_allowed(&source.path) {
        return Ok(source);
    }
    let mut file =
        match crate::admitted_fs::open_beneath_no_symlinks(&root.dir, relative, libc::O_RDONLY) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(source),
            // Unavailable/non-regular/symlink sources never keep stale instruction text.
            Err(_) => return Ok(source),
        };
    let meta = file.metadata().map_err(|_| "unreadable instructions")?;
    if !meta.is_file() || meta.len() > crate::defs::MAX_INSTRUCTIONS_FILE as u64 {
        return Err("instructions are not a bounded regular file".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(crate::defs::MAX_INSTRUCTIONS_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable instructions")?;
    if cancel.load(Ordering::Acquire) {
        return Err("cancelled".into());
    }
    if bytes.len() > crate::defs::MAX_INSTRUCTIONS_FILE {
        return Err("instructions file too large".into());
    }
    source.digest = Some(format!("{:x}", Sha256::digest(&bytes)));
    source.content = Some(String::from_utf8(bytes).map_err(|_| "instructions not UTF-8")?);
    Ok(source)
}

/// Successful read hook, equally usable by future directory/media reads.
/// Caller invokes it only after a successful admitted read. It never grants
/// access and does not implement a second tool or directory reader.
#[allow(clippy::too_many_arguments)]
pub(crate) fn after_read(
    roots: &[Root],
    files: &crate::files::Files,
    path: &str,
    scope_dir: bool,
    generation: u64,
    data_root: &Path,
    policy: &RuntimePolicy<'_>,
    cancel: &AtomicBool,
) -> Result<Vec<Source>, String> {
    let Some(root) = roots.iter().find(|r| r.origin == Origin::Project) else {
        return Ok(Vec::new());
    };
    if cancel.load(Ordering::Acquire) || policy.search_path_denied(path) {
        return Err("read instructions not admitted".into());
    }
    let resolved = files.resolve_path(path).map_err(|e| e.to_string())?;
    if policy.instruction_path_denied(&resolved.to_string_lossy()) {
        return Err("read instructions not admitted".into());
    }
    let scope = if scope_dir {
        resolved.as_path()
    } else {
        resolved.parent().ok_or("read has no scope")?
    };
    let relative = scope
        .strip_prefix(&root.path)
        .map_err(|_| "outside instruction root")?;
    // Nearest first, matching pinned donor discovery, stopping at Location.
    let mut result = Vec::new();
    for ancestor in relative.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        if result.len() >= SOURCES_CAP {
            return Err("instructions source budget exceeded".into());
        }
        let source = read_source(
            root,
            &ancestor.join("AGENTS.md"),
            Origin::Nested,
            generation,
            data_root,
            policy,
            cancel,
        )?;
        result.push(source);
    }
    validate_sources(&result)?;
    Ok(result)
}

/// Insert only latest typed facts at their original chronological positions.
/// Stale facts remain immutable in storage, but are absent from provider input.
pub(crate) fn project(
    input: &[InputItem],
    references: &[Reference],
    current: &[Fact],
) -> Vec<InputItem> {
    let mut output = Vec::new();
    for index in 0..=input.len() {
        for reference in references.iter().filter(|r| r.index == index) {
            if let Some(fact) = current.iter().find(|f| f.event == reference.event) {
                output.push(fact.input());
            }
        }
        if let Some(item) = input.get(index) {
            output.push(item.clone());
        }
    }
    output
}

/// Reconcile rules removed by a history/context boundary, without resurrecting
/// old source bodies. Exact equality compares only runtime-rendered typed facts.
pub(crate) fn reconcile(fixed: &mut Vec<InputItem>, history: &[InputItem], current: &[Fact]) {
    for fact in current {
        let item = fact.input();
        if !history.contains(&item) && !fixed.contains(&item) {
            fixed.push(item);
        }
    }
}

#[cfg(test)]
mod tests;
