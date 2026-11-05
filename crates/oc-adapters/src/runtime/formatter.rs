//! Captured configured programs continue the original, admitted mutation.
use super::*;
use crate::shell::Shell;
use crate::shell::jobs::{Jobs, Provenance};
use crate::tools::output::{Context, DiffCapture, Prepared};

struct Programs {
    definitions: Vec<crate::config::Formatter>,
    selected: Vec<usize>,
    jobs: Arc<Jobs>,
    shell: Shell,
    env: BTreeMap<String, String>,
    provenance: Provenance,
    cancel: Arc<AtomicBool>,
    slot: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl crate::patch::Formatter for Programs {
    fn prepare(&mut self, path: &std::path::Path) -> Result<bool, crate::patch::PatchError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(crate::patch::PatchError::Conflict {
                path: path.to_string_lossy().into_owned(),
                reason: "formatter cancelled before mutation".into(),
            });
        }
        let extension = path
            .extension()
            .map(|ext| format!(".{}", ext.to_string_lossy()))
            .unwrap_or_default();
        self.selected = self
            .definitions
            .iter()
            .enumerate()
            .filter_map(|(index, definition)| {
                (!definition.disabled
                    && !definition.command.is_empty()
                    && definition.extensions.contains(&extension))
                .then_some(index)
            })
            .collect();
        let matches = !self.selected.is_empty();
        if matches {
            self.slot =
                Some(
                    self.jobs
                        .reserve()
                        .ok_or_else(|| crate::patch::PatchError::Conflict {
                            path: path.to_string_lossy().into_owned(),
                            reason: "formatter capacity exhausted before mutation".into(),
                        })?,
                );
        }
        Ok(matches)
    }

    fn format(&mut self, path: &std::path::Path) -> Result<(), crate::patch::PatchError> {
        let unknown = || crate::patch::PatchError::Conflict {
            path: path.to_string_lossy().into_owned(),
            reason: "formatter-result-unknown".into(),
        };
        let cancelled = || crate::patch::PatchError::Conflict {
            path: path.to_string_lossy().into_owned(),
            reason: "formatter-cancelled".into(),
        };
        // Selection belongs to the logical mutation path; $FILE still names
        // the pinned, resolved target, whose extension can differ for aliases.
        for &index in &self.selected {
            let definition = &self.definitions[index];
            if self.cancel.load(Ordering::Acquire) {
                return Err(cancelled());
            }
            let file = path.to_str().ok_or_else(unknown)?;
            let argv: Vec<String> = definition
                .command
                .iter()
                .map(|arg| arg.replacen("$FILE", file, 1))
                .collect();
            let mut env = self.env.clone();
            env.extend(definition.environment.clone());
            let pinned = self.shell.pin_cwd(&argv, ".").map_err(|_| unknown())?;
            let slot = self
                .slot
                .take()
                .or_else(|| self.jobs.reserve())
                .ok_or_else(unknown)?;
            let outcome = self
                .jobs
                .format(
                    &self.shell,
                    &env,
                    &argv,
                    &pinned,
                    self.provenance.clone(),
                    slot,
                    self.cancel.clone(),
                )
                .map_err(|_| unknown())?;
            if outcome.state == "cancelled" {
                // Jobs has joined the process group. The mutation owner still
                // verifies actual bytes; unreadable results remain unknown.
                return Err(cancelled());
            }
            if !matches!(outcome.state.as_str(), "completed" | "failed") {
                return Err(unknown());
            }
            if outcome.exit == Some(0) {
                break;
            }
        }
        Ok(())
    }
}

impl Runtime<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn formatted_mutation(
        &self,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        call: &crate::tools::ToolCall,
        output: &Context<'_>,
        definitions: &[crate::config::Formatter],
        provenance: Provenance,
    ) -> Result<(&'static str, Prepared, Option<oc_core::patch::PatchEffects>), RuntimeError> {
        let permissions = policy.permissions.clone();
        let rules = policy.rules.cloned();
        let root = policy.root.map(std::path::Path::to_path_buf);
        let mcp = policy.mcp.to_vec();
        let permit = policy.permit.clone();
        let files = ctx.files.clone();
        let shell = ctx.shell.clone();
        let env = ctx.parent_env.clone();
        let snapshot = ctx.snapshot.clone();
        let roots = ctx.roots.clone();
        let call = call.clone();
        let db = self.db.shared_handle();
        let jobs = self.shell_jobs.clone();
        let operation = output.operation.to_owned();
        let session = output.session.to_owned();
        let location = output.location.to_owned();
        let generation = output.generation;
        let source = output.source.to_owned();
        let limits = output.limits;
        let definitions = definitions.to_vec();
        let cancel = Arc::new(AtomicBool::new(ctx.cancel.load(Ordering::Acquire)));
        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let cleanup = CancelOnDrop(cancel.clone());
        let origin_work = self.native_origin.as_ref().map(|origin| origin.work());
        let mut worker = tokio::task::spawn_blocking(move || {
            let _origin_work = origin_work;
            let policy = RuntimePolicy {
                permissions: &permissions,
                rules: rules.as_ref(),
                root: root.as_deref(),
                mcp: &mcp,
                permit,
            };
            let ctx = ToolContext {
                files: &files,
                shell: &shell,
                parent_env: &env,
                webfetch_auth: None,
                policy: &policy,
                subagent: None,
                snapshot: &snapshot,
                cancel: &cancel,
                roots,
            };
            let output = Context {
                db: &db,
                operation: &operation,
                session: &session,
                location: &location,
                generation,
                source: &source,
                limits,
            };
            let mut diff = DiffCapture::new(&output);
            let mut programs = Programs {
                definitions,
                selected: Vec::new(),
                jobs,
                shell: shell.clone(),
                env: env.clone(),
                provenance,
                cancel: cancel.clone(),
                slot: None,
            };
            let (state, text, effects) = if call.name == "apply_patch" {
                crate::tools::tool_patch_formatted(
                    &ctx,
                    &call,
                    &mut |text| diff.append(text),
                    Some(&mut programs),
                )
            } else {
                crate::tools::tool_mutation_formatted(
                    &ctx,
                    &call,
                    &mut |text| diff.append(text),
                    Some(&mut programs),
                )
            };
            (state, diff.finish(text), effects)
        });
        let result = tokio::select! { biased;
            () = crate::provider::wait_cancel(ctx.cancel) => { cleanup.0.store(true, Ordering::Release); (&mut worker).await },
            result = &mut worker => result,
        }.map_err(|_| RuntimeError::Storage)?;
        drop(cleanup);
        Ok(result)
    }
}
