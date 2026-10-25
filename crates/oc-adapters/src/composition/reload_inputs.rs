//! Original admitted local inputs, captured before credentials/public/discovered
//! catalogs mutate the executable composition. One shared original snapshot
//! supports semantic/per-provider comparison, never serialization, logging or
//! execution/trust grants.
use super::*;
use std::os::unix::fs::MetadataExt as _;

#[derive(PartialEq)]
pub(crate) struct ReloadInputs {
    generation: config::Generation,
    project: PathBuf,
    requests: Option<provider::ResponsesConfig>,
    selection: (String, String, Option<String>, Option<String>, u32),
    agents: BTreeMap<String, defs::AgentDef>,
    agent_order: Vec<String>,
    skills: Vec<(String, String)>,
    skill_errors: BTreeMap<String, String>,
    commands: BTreeMap<String, String>,
    command_defs: BTreeMap<String, defs::CommandDef>,
    command_descriptions: BTreeMap<String, String>,
    builtin_review: bool,
    instructions: String,
    roots: Vec<InstructionInput>,
    native_modules: BTreeSet<String>,
    plugins: PluginInventory,
    dcp: dcp_auto::DcpConfig,
    dcp_prompt_notes: Vec<ServiceDiagnostic>,
    protected: oc_core::context_plan::ProtectedSpec,
}

#[derive(PartialEq)]
struct InstructionInput {
    path: PathBuf,
    origin: crate::instructions::Origin,
    baseline: Option<crate::instructions::Source>,
    device: u64,
    inode: u64,
}

/// Finite admitted provider captures needed by a read-only candidate build, not
/// the application/Composition owner and not another catalog registry.
pub(crate) struct CatalogReuse {
    local: Option<std::sync::Arc<ReloadInputs>>,
    requests: Option<provider::ResponsesConfig>,
}

fn same_leaf(left: Option<&ReloadInputs>, right: Option<&ReloadInputs>, id: &str) -> bool {
    fn leaf(input: Option<&ReloadInputs>) -> Option<&provider::ResponsesConfig> {
        input.and_then(|input| input.requests.as_ref())
    }
    let (Some(left), Some(right)) = (leaf(left), leaf(right)) else {
        return false;
    };
    let (Some(left), Some(right)) = (left.wire.providers.get(id), right.wire.providers.get(id))
    else {
        return false;
    };
    let connection = |capture: &provider::CapturedProvider| {
        let mut config = capture.config.clone();
        let dynamic =
            config.wire.provider_state.as_ref().is_some_and(|state| {
                state.catalog_status == oc_core::queries::ProviderStatus::Pending
            });
        config.wire.provider_state = None;
        config.wire.selection_scope = None;
        (config, dynamic)
    };
    left.catalog == right.catalog && connection(left) == connection(right)
}

impl Composition {
    pub(crate) fn catalog_reuse(&self) -> CatalogReuse {
        CatalogReuse {
            local: self.local_inputs.clone(),
            requests: self.request_provider(&self.catalog.provider),
        }
    }
    pub(super) fn retain_local_inputs(&mut self) -> Result<(), LoadFailure> {
        self.local_inputs = Some(std::sync::Arc::new(self.reload_inputs()?));
        Ok(())
    }

    /// Compare actual admitted leaf captures, not the selected provider or a
    /// catalog already extended with remote models. Source-wide prompt/policy
    /// edits need not invalidate an unchanged provider's discovery result.
    pub(crate) fn same_provider_inputs(&self, other: &CatalogReuse, id: &str) -> bool {
        same_leaf(self.local_inputs.as_deref(), other.local.as_deref(), id)
    }

    /// Called after resolving the candidate's current credentials. Configured
    /// equality alone cannot carry catalog/auth facts across an account change.
    pub(crate) fn retain_catalog_reuse(&mut self, previous: &CatalogReuse) -> BTreeSet<String> {
        let ids = self
            .local_inputs
            .as_ref()
            .and_then(|input| input.requests.as_ref())
            .map(|request| request.wire.providers.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut retained = BTreeSet::new();
        for id in ids {
            let old = previous
                .requests
                .as_ref()
                .and_then(|request| request.wire.providers.get(&id));
            if !self.same_provider_inputs(previous, &id)
                || !matches!(
                    (self.provider_for(&id), old.map(|old| &old.config)),
                    (Some(next), Some(old)) if next.same_request_binding(old).unwrap_or(false)
                )
            {
                continue;
            }
            let Some(old) = old else {
                continue;
            };
            let catalog = old.catalog.clone();
            let Some(state) = old.config.wire.provider_state.clone() else {
                continue;
            };
            if id == self.catalog.provider {
                self.catalog = catalog;
                self.provider_state = state;
            } else if let Some(view) = self.provider_views.get_mut(&id) {
                view.catalog = catalog;
                view.state = state;
            } else {
                continue;
            }
            retained.insert(id);
        }
        self.tui_chrome.provider =
            Some(self.selected_provider_readiness(&self.model_id, self.variant.as_deref()));
        retained
    }

    /// Only call on a fresh local admission, before attaching saved/public or
    /// remote catalogs. In particular merged remote models are not local inputs.
    pub(crate) fn reload_inputs(&self) -> Result<ReloadInputs, LoadFailure> {
        let roots = self
            .instruction_roots
            .iter()
            .map(|root| {
                let meta = root.dir.metadata().map_err(|_| {
                    failure(
                        "native reload inputs",
                        &["instructions"],
                        ServiceStage::Admission,
                        ServiceCode::SourceUnavailable,
                    )
                })?;
                Ok(InstructionInput {
                    path: root.path.clone(),
                    origin: root.origin.clone(),
                    baseline: root.baseline.as_deref().cloned(),
                    device: meta.dev(),
                    inode: meta.ino(),
                })
            })
            .collect::<Result<_, LoadFailure>>()?;
        Ok(ReloadInputs {
            generation: self.generation.clone(),
            project: self.project.clone(),
            requests: self.request_provider(&self.catalog.provider),
            selection: (
                self.catalog.provider.clone(),
                self.model_id.clone(),
                self.variant.clone(),
                self.default_agent.clone(),
                self.subagent_depth,
            ),
            agents: self.agents.clone(),
            agent_order: self.agent_order.clone(),
            skills: self.skills.clone(),
            skill_errors: self.skill_errors.clone(),
            commands: self.commands.clone(),
            command_defs: self.command_defs.clone(),
            command_descriptions: self.command_descriptions.clone(),
            builtin_review: self.builtin_review,
            instructions: self.instructions.clone(),
            roots,
            native_modules: self.native_modules.clone(),
            plugins: self.tui_chrome.plugins.clone(),
            dcp: self.dcp_config.clone(),
            // Empty/invalid prompt edits can keep the same fallback bytes while
            // adding/removing a source-qualified warning. Publish those too.
            dcp_prompt_notes: self
                .tui_chrome
                .service_diagnostics
                .iter()
                .filter(|note| {
                    note.field
                        .iter()
                        .map(String::as_str)
                        .take(2)
                        .eq(["dcp", "prompts"])
                })
                .cloned()
                .collect(),
            protected: self.dcp_protected.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
