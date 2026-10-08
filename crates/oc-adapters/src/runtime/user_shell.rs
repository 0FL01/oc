//! Explicit user commands share policy admission and the supervised Shell owner,
//! but never create a model turn or a fabricated tool-call/result graph.
use super::*;

pub(crate) struct UserShellParams<'a> {
    pub session: &'a str,
    pub command: &'a str,
    pub catalog: &'a ModelCatalog,
    pub model: &'a str,
    pub variant: Option<&'a str>,
    pub fresh: bool,
    pub selection: Option<(&'a str, &'a str)>,
    pub cancel: &'a AtomicBool,
}

impl Runtime<'_> {
    pub(crate) async fn run_user_shell(
        &self,
        params: UserShellParams<'_>,
        mut accepted: impl FnMut(&str) + Send,
    ) -> Result<String, RuntimeError> {
        let _lease = self.begin_active()?;
        if params.command.trim().is_empty()
            || params.command.len() > oc_core::session::MAX_INPUT_BYTES
        {
            return Err(RuntimeError::InvalidArgs("invalid user shell input".into()));
        }
        if params.fresh {
            match self.db.session_meta(params.session) {
                Err(StorageError::SessionNotFound) => {}
                Ok(_) => {
                    return Err(RuntimeError::InvalidArgs(
                        "session id already exists".into(),
                    ));
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            self.open_session(params.session)?;
            if self.db.session_meta(params.session)?.parent_id.is_some() {
                return Err(RuntimeError::InvalidArgs(
                    "child shell input is read-only".into(),
                ));
            }
        }
        // Local catalog validation only: Shell does not acquire credentials or
        // prepare/send a model request merely to capture provenance.
        let model = models::select_model(params.catalog, params.model)
            .and_then(|model| models::select_variant(&model, params.variant))
            .map_err(|_| RuntimeError::InvalidArgs("user shell selection unavailable".into()))?;
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        let workspace = self.workspace.read().expect("workspace lock").clone();
        let operation = format!("user-shell:{}", next_turn_id(params.session, millis()));
        let call = crate::tools::ToolCall {
            id: operation.clone(),
            name: "shell".into(),
            arguments: serde_json::json!({"command":params.command,"background":true}),
        };
        crate::tools::validate_call(&call)
            .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
        let invocation = crate::tools::shell_call::invocation(&call, &self.parent_env)
            .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
        let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
            .with_root(&self.roots.project);
        let ctx = ToolContext {
            files: &self.files,
            shell: &self.shell,
            parent_env: &self.parent_env,
            webfetch_auth: self.webfetch_auth.clone(),
            webfetch_allow_private: self.webfetch_allow_private,
            policy: &policy,
            subagent: None,
            snapshot: &workspace.skills,
            cancel: params.cancel,
            roots: Some(self.roots.clone()),
        };
        let permitted = self
            .admit_tool(
                params.session,
                &operation,
                &operation,
                &call,
                &ctx,
                &policy,
                params.cancel,
                lane.agent_id.clone(),
                lane.agent_digest.clone(),
                false,
            )
            .await
            .map_err(|error| match error {
                AdmissionFailure::Required => RuntimeError::ApprovalRequired {
                    tool: "shell".into(),
                },
                AdmissionFailure::Denied(_) | AdmissionFailure::Rejected(_) => {
                    RuntimeError::PermissionDenied {
                        tool: "shell".into(),
                    }
                }
                AdmissionFailure::Cancelled => RuntimeError::Cancelled,
                AdmissionFailure::Invalid(_) => {
                    RuntimeError::InvalidArgs("shell admission prerequisites unavailable".into())
                }
            })?;
        if params.cancel.load(Ordering::Acquire) {
            return Err(RuntimeError::Cancelled);
        }
        let pinned = permitted
            .approved_shell_cwd()
            .ok_or_else(|| RuntimeError::InvalidArgs("shell cwd not pinned".into()))?;
        // Reserve before durable acceptance; queue/cap refusal cannot consume
        // a draft or leave an accepted command that never had an owner slot.
        let slot = self
            .shell_jobs
            .reserve()
            .ok_or_else(|| RuntimeError::InvalidArgs("running shell job limit".into()))?;
        let provenance = crate::shell::jobs::Provenance {
            version: 1,
            session: params.session.into(),
            turn: String::new(),
            operation: operation.clone(),
            location: self.location.clone(),
            generation: published.id,
            output_limits: published.config.tool_output,
            output_source: crate::config::mcp::safe_source_id(
                published
                    .config
                    .provenance
                    .get("tool_output")
                    .map_or("native defaults", String::as_str),
            ),
            agent: lane.agent_id,
            agent_digest: lane.agent_digest,
            model: model.id,
            provider: params.catalog.provider.clone(),
            command: params.command.into(),
            cwd: pinned.path.to_string_lossy().into(),
            selected_shell: invocation.argv[0].clone(),
        };
        let admission =
            self.db
                .admit_user_shell_job(&provenance, params.fresh, params.selection)?;
        // From here a launch failure is an admitted operation's honest outcome,
        // not a pre-effect rejection or an invitation to replay unknown work.
        accepted(&admission.provenance().operation);
        self.shell_jobs
            .launch_user(
                self.shell.clone(),
                self.parent_env.clone(),
                invocation.argv,
                invocation.cwd,
                invocation.timeout,
                pinned,
                admission,
                slot,
                mcp::mcp_redactions(&published.config, &self.parent_env),
            )
            .await?;
        if params.cancel.load(Ordering::Acquire) {
            self.shell_jobs.cancel_job(params.session, &operation);
        }
        Ok(operation)
    }
}
