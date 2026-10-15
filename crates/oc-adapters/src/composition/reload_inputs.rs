//! Original admitted local inputs, captured before credentials/public/discovered
//! catalogs mutate the executable composition. This is an ephemeral comparison,
//! never serialized, logged, or used as an execution/trust grant.
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

impl Composition {
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
            protected: self.dcp_protected.clone(),
        })
    }
}

#[cfg(test)]
mod tests;
