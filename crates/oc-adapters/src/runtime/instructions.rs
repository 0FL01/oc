//! Runtime source reconciliation shared by root/child request lanes.
use super::*;
use crate::instructions::{Origin, Source};

impl Runtime<'_> {
    pub(super) fn instruction_sources(
        &self,
        published: &ExecutionGeneration,
        session: &str,
        policy: &RuntimePolicy<'_>,
        cancel: &AtomicBool,
    ) -> Result<(u64, Vec<Source>), RuntimeError> {
        let (revision, previous) = self.db.instruction_view(session)?;
        let roots = &published.workspace.instruction_roots;
        let mut sources = Vec::new();
        for root in roots {
            let Some(baseline) = &root.baseline else {
                continue;
            };
            if std::path::Path::new(&baseline.path).starts_with(&self.roots.data) {
                continue;
            }
            let mut source = baseline.as_ref().clone();
            source.generation = published.id;
            if !sources.iter().any(|s: &Source| s.path == source.path) {
                sources.push(source);
            }
        }
        for fact in previous {
            if fact.source.origin != Origin::Nested || fact.source.content.is_none() {
                continue;
            }
            if let Some(root) = roots.iter().find(|r| {
                r.origin == Origin::Project && r.path.to_string_lossy() == fact.source.root
            }) {
                let path = std::path::Path::new(&fact.source.path)
                    .strip_prefix(&root.path)
                    .map_err(|_| RuntimeError::Storage)?;
                sources.push(
                    crate::instructions::read_source(
                        root,
                        path,
                        Origin::Nested,
                        published.id,
                        &self.roots.data,
                        policy,
                        cancel,
                    )
                    .map_err(RuntimeError::InvalidArgs)?,
                );
            }
        }
        crate::instructions::validate_sources(&sources).map_err(RuntimeError::InvalidArgs)?;
        Ok((revision, sources))
    }
}
