//! Source/MCP commit ownership and coherent fresh request capture.
//!
//! The supervisor owns an accepted commit even if its caller loses the ACK. The
//! short synchronous gate covers only desired-state reconciliation and Arc swaps,
//! never candidate construction, discovery, provider IO or issued tool execution.
use super::*;

/// Finite, already-admitted workspace inputs supplied by the application owner.
/// No loader, resource owner or independently mutable execution state lives here.
pub(crate) struct WorkspacePublication {
    pub agent_prompt: Option<String>,
    pub instructions: String,
    pub files: Vec<(String, String)>,
    pub skill_errors: BTreeMap<String, String>,
    pub agent_digest: Option<String>,
    pub agent_id: Option<String>,
    pub agent_color_index: Option<usize>,
    pub agent_permissions: BTreeMap<String, Permission>,
    pub agent_permission_rules: crate::permissions::PermissionRules,
    pub agent_request: crate::provider::RequestOverlay,
    pub instruction_roots: Vec<crate::instructions::Root>,
    pub subagents: Option<SubagentCatalog>,
    pub command_digest: Option<String>,
    pub profile_issue: Option<oc_core::queries::ServiceDiagnostic>,
}

/// One complete source replacement, not a second Composition or runtime.
pub(crate) struct SourcePublication {
    pub generation: Generation,
    pub workspace: WorkspacePublication,
    pub dcp_config: DcpConfig,
    pub dcp_protected: ProtectedSpec,
    pub provider_state: crate::composition::ProviderState,
    pub request_provider: Option<ResponsesConfig>,
    pub mcp_activation: Option<Arc<crate::composition::McpActivation>>,
}

pub(super) struct Commit {
    pub(super) gate: Arc<Mutex<()>>,
    current: Arc<RwLock<Arc<ExecutionGeneration>>>,
    expected: Arc<ExecutionGeneration>,
    candidate: Arc<ExecutionGeneration>,
}

impl Commit {
    pub(super) fn admitted(&self) -> bool {
        Arc::ptr_eq(
            &self.current.read().expect("generation lock"),
            &self.expected,
        )
    }

    pub(super) fn publish(&self) {
        *self.current.write().expect("generation lock") = self.candidate.clone();
    }
}

impl Runtime<'_> {
    fn prepare_workspace(
        &self,
        mut input: WorkspacePublication,
    ) -> Result<RuntimeWorkspace, RuntimeError> {
        let (mut skills, warnings) = SkillSnapshot::build(&input.files);
        if !warnings.is_empty() {
            return Err(RuntimeError::InvalidArgs(warnings.join("; ")));
        }
        skills.errors = input.skill_errors;
        let projection = skills.projection();
        let skills_projection = if projection.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&projection).map_err(|_| RuntimeError::Storage)?)
        };
        let shared_sources = !input.instruction_roots.is_empty();
        let instructions = if shared_sources {
            String::new()
        } else {
            input.instructions
        };
        if let Some(home) = self.parent_env.get("HOME") {
            input.agent_permission_rules.expand_home(home);
        }
        input
            .agent_permission_rules
            .bind_plan_project(&self.roots.project);
        Ok(RuntimeWorkspace {
            fixed_input: lane_fixed_input(input.agent_prompt.as_deref(), &instructions, None),
            skills,
            skills_projection,
            agent_digest: input.agent_digest,
            agent_id: input.agent_id,
            agent_color_index: input.agent_color_index,
            profile_issue: input.profile_issue,
            agent_permissions: input.agent_permissions,
            agent_permission_rules: input.agent_permission_rules,
            agent_request: input.agent_request,
            instructions,
            instruction_roots: input.instruction_roots,
            subagents: input.subagents,
            command_digest: input.command_digest,
        })
    }

    pub(crate) fn publish_native_workspace(
        &self,
        input: WorkspacePublication,
    ) -> Result<(), RuntimeError> {
        let _active = self.begin_active()?;
        let workspace = self.prepare_workspace(input)?;
        self.update_execution(|view| view.workspace = workspace);
        Ok(())
    }

    /// All construction/validation precedes the supervisor-owned atomic commit.
    /// Only the serialized source owner calls this; issued requests retain their
    /// immutable leases instead of acquiring the foreground execution reservation.
    pub(crate) async fn publish_source(
        &self,
        input: SourcePublication,
        force: bool,
    ) -> Result<u64, RuntimeError> {
        let expected = self.current.read().expect("generation lock").clone();
        let id = expected.id.checked_add(1).ok_or(RuntimeError::Storage)?;
        let candidate = ExecutionGeneration {
            generation: PublishedGeneration {
                id,
                config: input.generation.clone(),
            },
            workspace: self.prepare_workspace(input.workspace)?,
            dcp_config: input.dcp_config,
            dcp_protected: input.dcp_protected,
            provider_state: Some(input.provider_state),
            request_provider: input.request_provider,
            mcp_activation: input.mcp_activation.clone(),
        };
        self.start_mcp()?;
        self.mcp_owner()
            .reconcile(
                self.quarantined_mcp_config(input.generation),
                id,
                input.mcp_activation,
                force,
                Some(self.source_commit(expected, candidate)),
            )
            .await?;
        Ok(id)
    }

    pub(super) fn source_commit(
        &self,
        expected: Arc<ExecutionGeneration>,
        candidate: ExecutionGeneration,
    ) -> Commit {
        Commit {
            gate: self.publication_commit.clone(),
            current: self.current.clone(),
            expected,
            candidate: Arc::new(candidate),
        }
    }

    /// Capture services and source together before any fresh preparation awaits.
    pub(super) fn fresh_request_view(
        &self,
    ) -> Result<(Arc<ExecutionGeneration>, Arc<McpGeneration>), RuntimeError> {
        let _commit = self.publication_commit.lock().expect("publication commit");
        let published = self.current.read().expect("generation lock").clone();
        let attached = self.mcp_view_in(&published)?;
        Ok((published, attached))
    }
}
