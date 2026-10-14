//! Finite observations produced by admission, not filesystem discovery or trust.
//! Bodies, environment values and credentials never enter this topology.

use std::collections::BTreeSet;
use std::fs::File;
use std::os::fd::AsRawFd as _;
use std::path::{Path, PathBuf};

use super::{AdmittedRoot, LoadFailure, failure};
use crate::defs::InputKind;
use oc_core::queries::{ServiceCode, ServiceStage};

// Bound topology independently of source bodies and kernel watch allocation.
// Duplicate observations (including repeated provider admission) cost nothing.
const MAX_TARGETS: usize = 4096;
const MAX_PATH_BYTES: usize = 1024 * 1024;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Target {
    pub(crate) path: PathBuf,
    pub(crate) kind: InputKind,
}

#[derive(Clone, Default)]
pub(crate) struct WatchInputs {
    pub(crate) targets: BTreeSet<Target>,
    bytes: usize,
    exceeded: bool,
    unavailable: bool,
}

impl WatchInputs {
    pub(super) fn admitted(roots: &[PathBuf], admitted: &[Option<AdmittedRoot>]) -> Self {
        let mut inputs = Self::default();
        for (index, lexical) in roots.iter().enumerate() {
            // Include the known missing lexical path and the already-admitted
            // canonical root. No parent traversal introduces additional inputs.
            inputs.root(
                lexical,
                index == 0 && roots.len() == 3 || index == roots.len() - 1,
            );
            if let Some(root) = &admitted[index] {
                inputs.root(
                    &root.path,
                    index == 0 && roots.len() == 3 || index == roots.len() - 1,
                );
                for name in [
                    "opencode.json",
                    "opencode.jsonc",
                    "dcp.json",
                    "dcp.jsonc",
                    "AGENTS.md",
                ] {
                    if name == "AGENTS.md" && index == roots.len() - 1 {
                        continue;
                    }
                    // These are already-known input names. An alias target must
                    // remain inside the admitted root, just like the reader.
                    if let Ok(path) = root.path.join(name).canonicalize()
                        && path.starts_with(&root.path)
                    {
                        inputs.file(&path);
                    }
                }
            }
            // Baseline instructions are global + Location, not .opencode.
            if index != roots.len() - 1 {
                inputs.file(&lexical.join("AGENTS.md"));
                if let Some(root) = &admitted[index] {
                    inputs.file(&root.path.join("AGENTS.md"));
                }
            }
        }
        inputs
    }

    fn root(&mut self, root: &Path, definitions: bool) {
        for name in ["opencode.json", "opencode.jsonc", "dcp.json", "dcp.jsonc"] {
            self.file(&root.join(name));
        }
        if definitions {
            for (name, kind) in [
                ("agent", InputKind::AgentTree),
                ("agents", InputKind::AgentTree),
                ("mode", InputKind::MarkdownDirectory),
                ("modes", InputKind::MarkdownDirectory),
                ("command", InputKind::MarkdownDirectory),
                ("commands", InputKind::MarkdownDirectory),
                ("skill", InputKind::Skills),
                ("skills", InputKind::Skills),
            ] {
                self.insert(&root.join(name), kind);
            }
        }
    }

    pub(super) fn file(&mut self, path: &Path) {
        self.insert(path, InputKind::File);
    }

    /// Successful descriptor opens supply the canonical target without re-opening
    /// a lexical alias that could have been swapped outside the admitted root.
    pub(super) fn opened(&mut self, path: &Path, file: &File, kind: InputKind) {
        self.insert(path, kind);
        match std::fs::canonicalize(format!("/proc/self/fd/{}", file.as_raw_fd())) {
            Ok(canonical) => self.insert(&canonical, kind),
            Err(_) => self.unavailable = true,
        }
    }

    fn insert(&mut self, path: &Path, kind: InputKind) {
        if self.exceeded {
            return;
        }
        let path = match std::path::absolute(path) {
            Ok(path) => path,
            Err(_) => {
                self.unavailable = true;
                return;
            }
        };
        let target = Target {
            path: path.clone(),
            kind,
        };
        if self.targets.contains(&target) {
            return;
        }
        let bytes = path.as_os_str().len();
        if self.targets.len() == MAX_TARGETS || bytes > MAX_PATH_BYTES.saturating_sub(self.bytes) {
            self.exceeded = true;
            return;
        }
        self.bytes += bytes;
        self.targets.insert(target);
    }

    pub(super) fn validate(&self) -> Result<(), LoadFailure> {
        if self.exceeded || self.unavailable {
            return Err(failure(
                "native reload inputs",
                &["watch", "sources"],
                ServiceStage::Admission,
                if self.exceeded {
                    ServiceCode::CapacityExceeded
                } else {
                    ServiceCode::SourceUnavailable
                },
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
