//! Turn admission, provider/tool execution, child lanes and durable completion.

use super::*;

/// Bound for walking a session's parent chain (cycle guard).
const SUBAGENT_DEPTH_WALK_CAP: u32 = 64;

/// Permission strictness rank (`Deny > Ask > Allow`).
fn permission_rank(level: Permission) -> u8 {
    match level {
        Permission::Allow => 0,
        Permission::Ask => 1,
        Permission::Deny => 2,
    }
}

/// Developer messages a lane starts from: agent prompt, instructions, skills.
pub(super) fn lane_fixed_input(
    agent_prompt: Option<&str>,
    instructions: &str,
    skills_projection: Option<&str>,
) -> Vec<InputItem> {
    let mut fixed_input = Vec::new();
    if let Some(prompt) = agent_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        fixed_input.push(InputItem::message(InputRole::Developer, prompt));
    }
    if !instructions.trim().is_empty() {
        fixed_input.push(InputItem::message(InputRole::Developer, instructions));
    }
    if let Some(projection) = skills_projection {
        fixed_input.push(InputItem::message(
            InputRole::Developer,
            format!(
                "Available native skills (metadata only; call skill by id for body): {projection}"
            ),
        ));
    }
    fixed_input
}

/// `subagent` tool definition with the upstream `Available subagents` list.
fn subagent_tool_def(catalog: &SubagentCatalog, policy: &RuntimePolicy<'_>) -> ToolDef {
    let mut description = String::from(
        "Spawns an agent in a child session to work on the specified task.\n\
         The output includes a sessionID you can pass back later to continue that specific conversation with the subagent.\n\
         New child sessions start with fresh context, so include all relevant context and instructions when you don't pass a sessionID.\n\
         Foreground (default) runs the subagent to completion and returns its final response.",
    );
    {
        let available = catalog
            .agents
            .values()
            .filter(|agent| {
                !agent.primary
                    && !agent.hidden
                    && policy.effect(SUBAGENT_TOOL, &agent.id) != Permission::Deny
            })
            .collect::<Vec<_>>();
        if !available.is_empty() {
            description.push_str("\n\nAvailable subagents:");
            for agent in available {
                let fallback = "This subagent should only be called when explicitly requested.";
                let summary = if agent.description.trim().is_empty() {
                    fallback
                } else {
                    agent.description.trim()
                };
                description.push_str(&format!("\n- {}: {summary}", agent.id));
            }
        }
    }
    ToolDef {
        name: SUBAGENT_TOOL.to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task.",
                },
                "description": {
                    "type": "string",
                    "description": "A short 3-5 word label for the task, displayed to the user",
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the subagent to perform",
                },
                "model": {
                    "type": "string",
                    "description": "NEVER set this unless the user explicitly asks for a particular model or variant. The value is written as \"providerID/modelID\", or \"providerID/modelID#variant\" to include a variant. Do not guess the ID.",
                },
                "sessionID": {
                    "type": "string",
                    "description": "Continue a specific previous subagent conversation by passing its sessionID. Calls without a sessionID start a new conversation.",
                },
                "background": {
                    "type": "boolean",
                    "description": "Not supported yet: calls with true fail without creating a child session.",
                },
            },
            "required": ["agent", "description", "prompt"],
            "additionalProperties": false,
        }),
    }
}

/// Parse and validate `provider/model[#variant]` against the catalog.
pub(crate) fn resolve_subagent_model(
    catalog: &ModelCatalog,
    raw: &str,
) -> Result<ResolvedModel, String> {
    let invalid = || {
        format!(
            "Invalid model \"{raw}\". Use \"providerID/modelID\" or \"providerID/modelID#variant\"."
        )
    };
    let (provider, rest) = raw.split_once('/').ok_or_else(invalid)?;
    let (id, variant) = match rest.split_once('#') {
        Some((id, variant)) => (id, Some(variant)),
        None => (rest, None),
    };
    if provider.is_empty() || id.is_empty() {
        return Err(invalid());
    }
    if provider != catalog.provider || !catalog.models.contains_key(id) {
        return Err(format!(
            "Model \"{provider}/{id}\" is not available. Use the models tool to see what is available."
        ));
    }
    match variant {
        None => Ok(ResolvedModel {
            id: id.to_string(),
            variant: None,
        }),
        Some(variant) => {
            let base = models::select_model(catalog, id).map_err(|_| invalid())?;
            match models::select_variant(&base, Some(variant)) {
                Ok(selection) => Ok(ResolvedModel {
                    id: id.to_string(),
                    variant: selection.variant.map(|variant| variant.name),
                }),
                Err(models::SelectError::UnavailableVariant { enabled, .. })
                    if enabled.is_empty() =>
                {
                    Err(format!(
                        "Model \"{provider}/{id}\" has no variants. Omit the variant."
                    ))
                }
                Err(models::SelectError::UnavailableVariant { enabled, .. }) => Err(format!(
                    "Variant \"{variant}\" is not available for \"{provider}/{id}\". Available: {enabled}."
                )),
                Err(_) => Err(invalid()),
            }
        }
    }
}

/// Resolve the child model per upstream order: explicit override, else the
/// child agent's model, else the existing child's stored model, else the
/// parent session model.
#[allow(clippy::too_many_arguments)]
fn resolve_child_model(
    catalog: &ModelCatalog,
    request_model: Option<&str>,
    agent_model: Option<&str>,
    existing: Option<&SessionMeta>,
    switched: bool,
    parent_model_id: &str,
    parent_variant: Option<&str>,
) -> Result<ResolvedModel, String> {
    let parent = || ResolvedModel {
        id: parent_model_id.to_string(),
        variant: parent_variant.map(str::to_string),
    };
    let resolve = |raw: &str| resolve_subagent_model(catalog, raw);
    if let Some(raw) = request_model {
        return resolve(raw);
    }
    match existing {
        None => match agent_model {
            Some(raw) => resolve(raw),
            None => Ok(parent()),
        },
        Some(meta) if switched => match agent_model {
            Some(raw) => resolve(raw),
            None => match meta.model.as_deref() {
                Some(raw) => resolve(raw),
                None => Ok(parent()),
            },
        },
        Some(meta) => match meta.model.as_deref() {
            Some(raw) => resolve(raw),
            None => match agent_model {
                Some(raw) => resolve(raw),
                None => Ok(parent()),
            },
        },
    }
}

/// Foreground child runner for one calling turn.
struct TurnSubagent<'r, 'a> {
    owning_operation: String,
    runtime: &'r Runtime<'a>,
    parent_session: String,
    parent_model_id: String,
    parent_variant: Option<String>,
    catalog: &'r ModelCatalog,
    provider: &'r ResponsesConfig,
    cancel: &'r AtomicBool,
    attached: &'r McpGeneration,
    subagents: SubagentCatalog,
    parent_lane: &'r TurnLane,
}

impl SubagentRunner for TurnSubagent<'_, '_> {
    fn preflight(&self, request: &SubagentRequest) -> Result<(), ToolError> {
        self.resolve_request(request).map(|_| ())
    }
    fn spawn<'x>(
        &'x self,
        request: SubagentRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SubagentOutcome, ToolError>> + Send + 'x>,
    > {
        Box::pin(self.spawn_inner(request))
    }
}

impl TurnSubagent<'_, '_> {
    /// Shared by admission, post-wait revalidation, and actual child execution.
    /// This resolver never allocates child IDs or writes session/turn state.
    fn resolve_request(
        &self,
        request: &SubagentRequest,
    ) -> Result<(&SubagentAgent, ResolvedModel), ToolError> {
        let failed = |reason: String| ToolError::Failed {
            tool: SUBAGENT_TOOL.into(),
            reason,
        };
        let limit = self.subagents.depth_limit;
        let depth = self
            .runtime
            .session_depth(&self.parent_session)
            .map_err(|error| ToolError::Failed {
                tool: SUBAGENT_TOOL.to_string(),
                reason: error.to_string(),
            })?;
        if depth >= limit {
            return Err(failed(format!(
                "Subagent depth limit reached ({limit}). Increase \"experimental.subagent_depth\" to allow nested subagents."
            )));
        }
        let Some(agent) = self.subagents.agents.get(&request.agent) else {
            return Err(failed(format!("Unknown agent: {}", request.agent)));
        };
        if agent.primary {
            return Err(failed(format!(
                "Agent {} cannot run as a subagent",
                request.agent
            )));
        }
        let existing = match &request.session_id {
            None => None,
            Some(id) => match self.runtime.db.session_meta(id) {
                Ok(meta) if meta.parent_id.as_deref() == Some(self.parent_session.as_str()) => {
                    Some(meta)
                }
                Ok(_) => {
                    return Err(failed(format!(
                        "Session {id} is not a child of the current session"
                    )));
                }
                Err(StorageError::SessionNotFound) => {
                    return Err(failed(format!("Subagent session not found: {id}")));
                }
                Err(error) => {
                    return Err(ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    });
                }
            },
        };
        let switched = existing
            .as_ref()
            .is_some_and(|meta| meta.agent.as_deref() != Some(agent.id.as_str()));
        let model = resolve_child_model(
            self.catalog,
            request.model.as_deref(),
            agent.model.as_deref(),
            existing.as_ref(),
            switched,
            &self.parent_model_id,
            self.parent_variant.as_deref(),
        )
        .map_err(failed)?;
        self.runtime
            .admit_provider(self.catalog, &model.id, self.provider)
            .map_err(|error| failed(error.to_string()))?;
        Ok((agent, model))
    }

    async fn spawn_inner(&self, request: SubagentRequest) -> Result<SubagentOutcome, ToolError> {
        let (agent, model) = self.resolve_request(&request)?;
        let (child_session, fresh) = match &request.session_id {
            Some(id) => (id.clone(), false),
            None => {
                let id = self.runtime.new_child_id(&self.parent_session);
                self.runtime
                    .db
                    .create_child_session(
                        &self.parent_session,
                        &id,
                        Some(&agent.id),
                        Some(&model.stored(&self.catalog.provider)),
                        Some(&request.description),
                    )
                    .map_err(|error| ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    })?;
                self.runtime
                    .db
                    .set_pref(&session_location_key(&id), self.runtime.location())
                    .map_err(|error| ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    })?;
                (id, true)
            }
        };
        let prompt = if fresh {
            format!(
                "You are a subagent spawned by another session.\n{}",
                request.prompt
            )
        } else {
            request.prompt.clone()
        };
        let report = self
            .runtime
            .run_child_turn(
                agent,
                self.parent_lane,
                &self.owning_operation,
                &child_session,
                prompt,
                &model,
                0, // Resolve the same native output default as the primary turn.
                self.catalog,
                self.provider,
                self.attached,
                self.cancel,
            )
            .await
            .map_err(|error| ToolError::Failed {
                tool: SUBAGENT_TOOL.to_string(),
                reason: error.to_string(),
            })?;
        // Child warnings must survive the existing text-only tool result DTO.
        let with_warnings = |text: String| {
            if report.warnings.is_empty() {
                text
            } else {
                format!("Warning: {}\n\n{text}", report.warnings.join("\nWarning: "))
            }
        };
        Ok(match report.status {
            TurnStatus::Completed => SubagentOutcome::Completed {
                session_id: child_session,
                text: with_warnings(if report.text.is_empty() {
                    SUBAGENT_NO_TEXT.to_string()
                } else {
                    report.text
                }),
            },
            TurnStatus::Cancelled => SubagentOutcome::Cancelled {
                session_id: child_session,
            },
            _ => SubagentOutcome::Failed {
                session_id: Some(child_session),
                reason: with_warnings(
                    report
                        .diagnostic
                        .unwrap_or_else(|| "subagent turn did not complete".to_string()),
                ),
            },
        })
    }
}

fn is_builtin(name: &str) -> bool {
    crate::tools::MODEL_TOOL_NAMES.contains(&name) || name == "bash" || name == SUBAGENT_TOOL
}

fn units_have_calls(units: &[Assembled]) -> bool {
    units.iter().any(|unit| matches!(unit, Assembled::Call(_)))
}

fn output_state(output: &str) -> &'static str {
    if output == "error: cancelled" {
        "cancelled"
    } else if output.starts_with("error: ") {
        "failed"
    } else {
        "completed"
    }
}

/// Forward one terminal tool-call state to the live event sink, bounded
/// exactly like the turn report.
fn emit_tool_finish(
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    turn_id: &str,
    op: &str,
    name: &str,
    state: &str,
    output: &str,
) {
    emit_tool_finish_with_effects(tool_event, turn_id, op, name, state, output, None);
}

#[allow(clippy::too_many_arguments)]
fn emit_tool_finish_with_effects(
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    turn_id: &str,
    op: &str,
    name: &str,
    state: &str,
    output: &str,
    patch_effects: Option<oc_core::patch::PatchEffects>,
) {
    emit_tool_finish_with_metadata(
        tool_event,
        turn_id,
        op,
        name,
        state,
        output,
        patch_effects,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_tool_finish_with_metadata(
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    turn_id: &str,
    op: &str,
    name: &str,
    state: &str,
    output: &str,
    patch_effects: Option<oc_core::patch::PatchEffects>,
    dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
) {
    tool_event(
        turn_id,
        &ToolCallEvent::Finished {
            question: oc_core::question::QuestionResult::from_output(name, state, Some(output)),
            dcp,
            patch_effects,
            op: op.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            output: truncate(output, REPORT_OUTPUT_CAP),
            output_bytes: output.len() as i64,
            output_truncated: output.len() > REPORT_OUTPUT_CAP,
        },
    );
}

/// Persist one tool outcome, then notify the live event sink. The event can
/// never describe an outcome that was not durably recorded.
#[allow(clippy::too_many_arguments)]
fn record_tool_finish(
    db: &Db,
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    op: &str,
    name: &str,
    state: &str,
    output: &str,
    turn_id: &str,
    turn_log: &TurnLog,
) -> Result<(), RuntimeError> {
    db.tool_outcome_with_log(op, state, output, turn_id, &turn_log.to_json().to_string())?;
    emit_tool_finish(tool_event, turn_id, op, name, state, output);
    Ok(())
}

impl<'a> Runtime<'a> {
    #[allow(clippy::too_many_arguments)]
    async fn admit_tool<'p>(
        &self,
        session: &str,
        turn: &str,
        op: &str,
        call: &crate::tools::ToolCall,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'p>,
        cancel: &AtomicBool,
        agent: Option<String>,
        agent_digest: Option<String>,
    ) -> Result<RuntimePolicy<'p>, AdmissionFailure> {
        use oc_core::approval::*;
        let raw = crate::tools::permission_resources(call).map_err(|e| e.to_string())?;
        let resources: Vec<_> = raw
            .iter()
            .map(|r| {
                if matches!(call.name.as_str(), "read" | "apply_patch") {
                    permission_path(policy.root, r)
                } else {
                    r.clone()
                }
            })
            .collect();
        if resources
            .iter()
            .any(|r| policy.effect(&call.name, r) == Permission::Deny)
        {
            return Err(format!("denied {}", call.name).into());
        }
        let mut permitted = policy.clone();
        if call.name == SUBAGENT_TOOL {
            crate::tools::preflight_subagent(ctx, call).map_err(|e| match e {
                ToolError::Failed { reason, .. } => reason,
                other => other.to_string(),
            })?;
        }
        let compression_plan = if call.name == COMPRESS_TOOL {
            Some(self.preflight_compression(session, call)?)
        } else {
            None
        };
        if resources
            .iter()
            .all(|r| policy.effect(&call.name, r) == Permission::Allow)
        {
            if matches!(call.name.as_str(), "shell" | "bash") || compression_plan.is_some() {
                let shell_cwd = if matches!(call.name.as_str(), "shell" | "bash") {
                    crate::approval::prepare(ctx, call, &resources).await?.3
                } else {
                    None
                };
                permitted.permit = Some(InvocationPermit {
                    call: call.clone(),
                    resources,
                    patch_preimage: None,
                    shell_cwd,
                    compression_plan,
                });
            }
            return Ok(permitted);
        }
        let project = crate::approval::project_identity(&self.roots.project)?;
        let grant_resources =
            crate::approval::grant_resources(&self.roots.project, &call.name, &resources);
        let mut saved = true;
        for (resource, granted) in resources.iter().zip(&grant_resources) {
            if policy.effect(&call.name, resource) != Permission::Allow
                && !self
                    .db
                    .permission_grant_matches(
                        &project,
                        crate::config::legacy_key(&call.name),
                        granted,
                    )
                    .map_err(|_| "grant storage unavailable")?
            {
                saved = false;
                break;
            }
        }
        let (preview, digest, patch_preimage, shell_cwd) =
            crate::approval::prepare(ctx, call, &resources).await?;
        if !saved {
            let events = self
                .compaction_events
                .lock()
                .expect("events lock")
                .clone()
                .ok_or(AdmissionFailure::Required)?;
            let request = ApprovalRequest {
                id: 0,
                binding: ApprovalBinding {
                    session: session.into(),
                    turn: turn.into(),
                    call: call.id.clone(),
                    operation: op.into(),
                    input_digest: digest.clone(),
                    location: self.location.clone(),
                    generation: self.generation_id(),
                    agent,
                    agent_digest,
                },
                project: project.clone(),
                action: crate::config::legacy_key(&call.name).into(),
                resources: resources.clone(),
                save_patterns: crate::approval::save_patterns(&call.name, &grant_resources),
                preview,
            };
            let waiting = self.approvals.wait(request, &events);
            tokio::pin!(waiting);
            let decision = loop {
                tokio::select! {
                    result = &mut waiting => break result.map_err(|e| if e == "approval required: no consumer" { AdmissionFailure::Required } else { AdmissionFailure::Invalid(e.into()) })?,
                    () = tokio::time::sleep(std::time::Duration::from_millis(5)) => if cancel.load(Ordering::Acquire) { return Err(AdmissionFailure::Cancelled); }
                }
            };
            match decision {
                ApprovalDecision::Once | ApprovalDecision::Always => {}
                ApprovalDecision::Reject {
                    feedback: Some(feedback),
                } => return Err(AdmissionFailure::Rejected(Some(feedback))),
                ApprovalDecision::Reject { feedback: None } => {
                    return Err(AdmissionFailure::Rejected(None));
                }
                ApprovalDecision::Cancelled => return Err(AdmissionFailure::Cancelled),
            }
        }
        if cancel.load(Ordering::Acquire) {
            return Err("approval cancelled".into());
        }
        let (_, rechecked, _, _) = crate::approval::prepare(ctx, call, &resources).await?;
        if digest != rechecked {
            return Err("approval prerequisites changed".into());
        }
        if compression_plan.is_some()
            && compression_plan.as_ref() != Some(&self.preflight_compression(session, call)?)
        {
            return Err("approval compression plan changed".into());
        }
        if crate::approval::project_identity(&self.roots.project)? != project {
            return Err("approval project changed".into());
        }
        if resources
            .iter()
            .any(|r| policy.effect(&call.name, r) == Permission::Deny)
        {
            return Err("policy denied after approval".into());
        }
        permitted.permit = Some(InvocationPermit {
            call: call.clone(),
            resources,
            patch_preimage,
            shell_cwd,
            compression_plan,
        });
        Ok(permitted)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn run_turn_inner(
        &self,
        params: TurnParams<'_>,
        lane: &TurnLane,
        attached: &McpGeneration,
        fresh_selection: Option<Option<(&str, &str)>>,
        accepted: &mut (dyn FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send),
        text_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_item_ended: &mut (dyn FnMut(&str) + Send),
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) -> Result<TurnReport, RuntimeError> {
        self.admit_provider(params.catalog, &params.model_id, &params.provider)?;
        let published = self.current.read().expect("generation lock").clone();
        let base = models::select_model(params.catalog, &params.model_id)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let fallback = published
            .config
            .providers
            .get(&params.catalog.provider)
            .map(|provider| provider.options.native_fallback_limits)
            .unwrap_or_default();
        let budget = models::budget(&base, params.max_output, fallback);
        let mut report = self
            .run_turn_admitted(
                params,
                lane,
                attached,
                fresh_selection,
                accepted,
                text_delta,
                reasoning_delta,
                reasoning_item_ended,
                tool_event,
                &budget,
            )
            .await?;
        if let Some(warning) = budget.warning {
            report.warnings.push(warning);
        }
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_turn_admitted(
        &self,
        params: TurnParams<'_>,
        lane: &TurnLane,
        attached: &McpGeneration,
        fresh_selection: Option<Option<(&str, &str)>>,
        accepted: &mut (dyn FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send),
        text_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_item_ended: &mut (dyn FnMut(&str) + Send),
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
        budget: &models::AdmissionBudget,
    ) -> Result<TurnReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        if fresh_selection.is_some() {
            if params.session.trim().is_empty() {
                return Err(RuntimeError::InvalidArgs("empty session id".into()));
            }
            match self.db.session_meta(&params.session) {
                Err(StorageError::SessionNotFound) => {}
                Ok(_) => {
                    return Err(RuntimeError::InvalidArgs(
                        "session id already exists".into(),
                    ));
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            self.open_session(&params.session)?;
        }
        // Exact model selection + admission before any side effect.
        let base = models::select_model(params.catalog, &params.model_id)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let selection = models::select_variant(&base, params.variant.as_deref())
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        self.validate_checkpoint_route(
            &params.session,
            &params.catalog.provider,
            &selection.id,
            &params.provider,
        )?;
        let workspace = self.workspace.read().expect("workspace lock").clone();
        // Outbound context honors compression blocks + prune mark: covered
        // members collapse to summaries, raw history is never rewritten.
        let (mut projected, mut history, history_scope) = if fresh_selection.is_some() {
            (Vec::new(), Vec::new(), None)
        } else {
            let ActiveContext {
                after_seq,
                projected,
                blocks,
            } = self.active_projection(&params.session)?;
            let history = self.wire_history(
                &params.session,
                &projected,
                &blocks,
                &selection.id,
                &params.catalog.provider,
                lane.agent_digest.as_deref(),
                after_seq,
            )?;
            (projected, history, Some((after_seq, blocks)))
        };
        let dcp_config = self.dcp_config.read().expect("dcp lock").clone();
        let compress_available = dcp_config.enabled
            && !dcp_config.manual_mode
            && RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
                .effect(COMPRESS_TOOL, "*")
                != Permission::Deny;
        let model_context = budget.context;
        let dcp_model_key = format!("{}/{}", params.catalog.provider, selection.id);
        let dcp_thresholds = dcp_config.effective_for_context(&dcp_model_key, model_context);
        if dcp_thresholds.min_context > dcp_thresholds.max_context {
            return Err(RuntimeError::InvalidArgs(
                "effective DCP minContextLimit exceeds maxContextLimit".into(),
            ));
        }
        let mut tool_projection = if fresh_selection.is_some() {
            Default::default()
        } else {
            self.db
                .dcp_tool_projection_for_input(&params.session, &history)?
        };
        let mut anchors = compress_available
            .then(|| dcp_config_input(&projected, &dcp_config))
            .flatten();
        let state_key = format!(
            "dcp.nudge.{}\0{}\0{}",
            params.session, params.catalog.provider, selection.id
        );
        {
            let mut states = self.nudge_state.lock().expect("nudge lock");
            if !states.contains_key(&state_key) {
                let state = match fresh_selection {
                    Some(_) => NudgeState::default(),
                    None => match self.db.get_pref(&state_key)? {
                        Some(raw) => {
                            serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?
                        }
                        None => NudgeState::default(),
                    },
                };
                states.insert(state_key.clone(), state);
            }
        }
        let mut nudge_hint = None;
        // Early admission before durable intent; full request admission is
        // repeated below for every round, including tool schemas and results.
        let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
            .with_root(&self.roots.project)
            .with_mcp(&attached.entries);
        let mut fixed_input = lane.fixed_input.clone();
        fixed_input.extend(mcp_instruction_input(attached, &policy));
        let (instruction_revision, instruction_sources) =
            self.instruction_sources(&params.session, &policy, params.cancel)?;
        let prior_instruction_facts = self.db.instruction_view(&params.session)?.1;
        let prior_instruction_input = prior_instruction_facts
            .iter()
            .map(crate::instructions::Fact::input)
            .collect::<Vec<_>>();
        history.retain(|item| !prior_instruction_input.contains(item));
        // Pre-admission counts the same source layer. Durable identities are
        // allocated only with the accepted turn's checkpoint below.
        for source in &instruction_sources {
            if source.content.is_some() {
                fixed_input.push(
                    crate::instructions::Fact {
                        event: 0,
                        source: source.clone(),
                        change: "initial".into(),
                        revision: instruction_revision + 1,
                    }
                    .input(),
                );
            }
        }
        let mut tool_defs = builtin_tool_defs();
        tool_defs.retain(|tool| policy.tool_visible(&tool.name));
        if !compress_available {
            tool_defs.retain(|tool| tool.name != COMPRESS_TOOL);
        }
        let subagents = workspace.subagents.clone();
        if let Some(catalog) = &subagents
            && policy.tool_visible(SUBAGENT_TOOL)
        {
            tool_defs.push(subagent_tool_def(catalog, &policy));
        }
        for entry in &attached.entries {
            tool_defs.push(ToolDef {
                name: entry.namespaced.clone(),
                description: entry
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("mcp {} tool", entry.tool)),
                parameters: entry.input_schema.clone(),
            });
        }
        let prompt_input = [InputItem::message(InputRole::User, &params.prompt)];
        let admitted_history = dcp_continuation(&history, &[], &tool_projection);
        let assembled_estimate = estimate_tokens(
            &serde_json::to_string(&(&fixed_input, &admitted_history, &prompt_input, &tool_defs))
                .map_err(|_| RuntimeError::Storage)?,
        )
        .saturating_add(
            admitted_history
                .iter()
                .filter_map(|item| match item {
                    InputItem::ReadFunctionCallOutput { output, .. } => Some(
                        output
                            .estimated_tokens()
                            .saturating_add((output.facts().to_string().len() as u64).div_ceil(4)),
                    ),
                    _ => None,
                })
                .sum::<u64>(),
        );
        let compaction_config = published.config.compaction.clone();
        let usage_scope = crate::compaction::fingerprint(&(
            crate::compaction::route_identity(
                &params.catalog.provider,
                &selection.id,
                &params.provider,
            )
            .map_err(|_| RuntimeError::Provider)?,
            &fixed_input,
            &tool_defs,
            &params.variant,
        ));
        let compaction_context = selection
            .entry
            .pointer("/limit/context")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        // A new prompt that cannot fit even with an empty history cannot be
        // repaired by summarization. Refuse before spending a summary request.
        let irreducible = estimate_tokens(
            &serde_json::to_string(&(&fixed_input, &prompt_input, &tool_defs))
                .map_err(|_| RuntimeError::Storage)?,
        );
        models::admit_budget(&selection, irreducible, budget)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let compaction_output = selection
            .entry
            .pointer("/limit/output")
            .and_then(|v| v.as_u64())
            .unwrap_or(budget.output);
        let compaction_input = selection
            .entry
            .pointer("/limit/input")
            .and_then(|v| v.as_u64());
        let compaction_due = fresh_selection.is_none()
            && !params.cancel.load(Ordering::Relaxed)
            && !history.is_empty()
            && self
                .compaction_estimate(
                    &params.session,
                    &usage_scope,
                    &admitted_history,
                    &prompt_input,
                    crate::compaction::estimate_context(
                        &fixed_input,
                        &admitted_history,
                        &prompt_input,
                        &tool_defs,
                    ),
                )?
                .is_some_and(|estimate| {
                    compaction_config.required(
                        estimate,
                        compaction_context,
                        compaction_output,
                        compaction_input,
                    )
                });
        // Irreducible schema-inclusive admission has passed. If repairable
        // history requires compaction, promote the genuine user first; its
        // admission receipt and pre-context precede the summary lifecycle.
        if !compaction_due && !params.cancel.load(Ordering::Relaxed) {
            models::admit_budget(&selection, assembled_estimate, budget).map_err(|error| {
                RuntimeError::InvalidArgs(match &budget.warning {
                    Some(warning) => format!("{error}; {warning}"),
                    None => error.to_string(),
                })
            })?;
        }
        // Durable intent before any side effect.
        let turn_id = next_turn_id(&params.session, millis());
        let user_text = params.invocation.as_deref().unwrap_or(&params.prompt);
        let model_ref = oc_core::queries::ModelRef {
            provider: params.catalog.provider.clone(),
            id: selection.id.clone(),
            variant: selection
                .variant
                .as_ref()
                .map(|variant| variant.name.clone())
                .filter(|name| name != "default"),
        };
        let accepted_turn = if let Some(initial_selection) = fresh_selection {
            self.db.create_bound_session_and_accept_turn(
                &params.session,
                &self.location,
                &turn_id,
                &params.prompt,
                user_text,
                initial_selection,
                &model_ref,
            )?
        } else {
            self.db.accept_turn(
                &turn_id,
                &params.session,
                &params.prompt,
                user_text,
                &model_ref,
            )?
        };
        accepted(&turn_id, accepted_turn.model_switch.as_ref());
        let user_message = accepted_turn.user_message;
        let snapshot = workspace.skills;
        // Children retain the parent's exact admitted MCP capability view.
        let primary_request = self.db.session_meta(&params.session)?.parent_id.is_none();
        let mut text = String::new();
        let mut usage = None;
        let mut context_usage = None;
        let mut usage_complete = true;
        let mut streamed = Duration::ZERO;
        let mut calls = Vec::new();
        let mut turn_log = TurnLog::new(&turn_id, &selection.id, &params.catalog.provider);
        turn_log.display = serde_json::json!({
            "config_generation":published.id,
            "owning_operation":lane.owning_operation.as_deref().unwrap_or(&turn_id),
            "model_label":selection.entry.get("name").and_then(|v|v.as_str()).unwrap_or(&selection.id),
            "agent":lane.agent_id,
            "agent_color_index":lane.agent_color_index,
        });
        turn_log.agent_digest = lane.agent_digest.clone();
        let mut shell_notice_seq = projected
            .iter()
            .filter_map(|(id, _, _)| id.strip_prefix('m')?.parse::<i64>().ok())
            .max()
            .unwrap_or(0);
        turn_log.user_message = Some(user_message);
        turn_log
            .input
            .push(InputItem::message(InputRole::User, &params.prompt));
        self.db.checkpoint_instructions(
            &turn_id,
            &mut turn_log,
            instruction_revision,
            &instruction_sources,
            usize::from(instruction_revision != 0),
            None,
        )?;
        if let Some((after_seq, blocks)) = history_scope {
            history = self.wire_history(
                &params.session,
                &projected,
                &blocks,
                &selection.id,
                &params.catalog.provider,
                lane.agent_digest.as_deref(),
                after_seq,
            )?;
        }
        let mut previous_instruction_input = self
            .db
            .instruction_view(&params.session)?
            .1
            .iter()
            .map(crate::instructions::Fact::input)
            .collect::<Vec<_>>();
        let max_rounds = params.max_rounds.clamp(1, ROUND_CAP);
        let mut rounds = 0u32;
        let mut overflow_recovered = false;
        let mut overflow_pending = false;
        let mut last_compacted_round = None;
        let mut retry_policy = retry::RetryPolicy::default();
        'step: loop {
            if let Some(events) = self
                .compaction_events
                .lock()
                .expect("event publisher")
                .as_ref()
            {
                self.shell_jobs.deliver(events)?;
            }
            for (id, notice) in self
                .db
                .shell_notices_after(&params.session, shell_notice_seq)?
            {
                shell_notice_seq = id
                    .strip_prefix('m')
                    .and_then(|value| value.parse().ok())
                    .ok_or(RuntimeError::Storage)?;
                turn_log.shell_notice_messages.push(id);
                turn_log
                    .input
                    .push(InputItem::message(InputRole::User, notice));
            }
            if params.cancel.load(Ordering::Relaxed) {
                return self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Cancelled,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
            // A completed connection/control is adopted only before building
            // the next provider request. Its clients, schemas, guidance and
            // execution context then stay leased through that request's tools.
            // Notification relists still occur at the outer turn boundary.
            let current_mcp = if primary_request {
                let view = self.mcp_owner().request_view()?;
                (view.publication == attached.publication).then_some(view)
            } else {
                None
            };
            let attached = current_mcp.as_deref().unwrap_or(attached);
            let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
                .with_root(&self.roots.project)
                .with_mcp(&attached.entries);
            let mut fixed_input = lane.fixed_input.clone();
            fixed_input.extend(mcp_instruction_input(attached, &policy));
            let mut tool_defs = builtin_tool_defs();
            tool_defs.retain(|tool| policy.tool_visible(&tool.name));
            if !compress_available {
                tool_defs.retain(|tool| tool.name != COMPRESS_TOOL);
            }
            if let Some(catalog) = &subagents
                && policy.tool_visible(SUBAGENT_TOOL)
            {
                tool_defs.push(subagent_tool_def(catalog, &policy));
            }
            for entry in &attached.entries {
                tool_defs.push(ToolDef {
                    name: entry.namespaced.clone(),
                    description: entry
                        .description
                        .clone()
                        .unwrap_or_else(|| format!("mcp {} tool", entry.tool)),
                    parameters: entry.input_schema.clone(),
                });
            }
            let usage_scope = crate::compaction::fingerprint(&(
                crate::compaction::route_identity(
                    &params.catalog.provider,
                    &selection.id,
                    &params.provider,
                )
                .map_err(|_| RuntimeError::Provider)?,
                &fixed_input,
                &tool_defs,
                &params.variant,
            ));
            let runner = subagents.as_ref().map(|catalog| TurnSubagent {
                owning_operation: lane
                    .owning_operation
                    .clone()
                    .unwrap_or_else(|| turn_id.clone()),
                runtime: self,
                parent_session: params.session.clone(),
                parent_model_id: params.model_id.clone(),
                parent_variant: params.variant.clone(),
                catalog: params.catalog,
                provider: &params.provider,
                cancel: params.cancel,
                attached,
                subagents: catalog.clone(),
                parent_lane: lane,
            });
            let ctx = ToolContext {
                files: &self.files,
                shell: &self.shell,
                parent_env: &self.parent_env,
                webfetch_auth: self.webfetch_auth.clone(),
                webfetch_allow_private: self.webfetch_allow_private,
                policy: &policy,
                subagent: runner.as_ref().map(|runner| runner as &dyn SubagentRunner),
                snapshot: &snapshot,
                cancel: params.cancel,
                roots: Some(self.roots.clone()),
            };
            let instruction_facts = self.db.instruction_view(&params.session)?.1;
            let latest_instruction_input = instruction_facts
                .iter()
                .map(crate::instructions::Fact::input)
                .collect::<Vec<_>>();
            if previous_instruction_input != latest_instruction_input {
                // Only typed source projections changed. Keep the already
                // admitted conversational/notice window pinned; a full refresh
                // here could deliver a current-turn shell notice twice.
                history.retain(|item| {
                    !previous_instruction_input.contains(item)
                        || latest_instruction_input.contains(item)
                });
                previous_instruction_input = latest_instruction_input;
            }
            let current_instruction_input = crate::instructions::project(
                &turn_log.input,
                &turn_log.instruction_references,
                &instruction_facts,
            );
            let projected_continuation =
                dcp_continuation(&history, &current_instruction_input, &tool_projection);
            crate::instructions::reconcile(
                &mut fixed_input,
                &projected_continuation,
                &instruction_facts,
            );
            let (nudge, persisted_nudge) = {
                let estimate = estimate_tokens(
                    &serde_json::to_string(&(
                        fixed_input.as_slice(),
                        projected_continuation.as_slice(),
                    ))
                    .map_err(|_| RuntimeError::Storage)?,
                );
                let summary_tokens = active_summary_tokens(&projected);
                let mut states = self.nudge_state.lock().expect("nudge lock");
                let state = states.entry(state_key.clone()).or_default();
                let nudge = if compress_available {
                    state.on_turn();
                    evaluate(
                        &dcp_config,
                        state,
                        &dcp_model_key,
                        model_context,
                        estimate,
                        summary_tokens,
                    )
                } else {
                    None
                };
                let persisted = serde_json::to_string(state).map_err(|_| RuntimeError::Storage)?;
                (nudge, persisted)
            };
            self.db.set_pref(&state_key, &persisted_nudge)?;
            let nudge_input = nudge.as_ref().map(|nudge| {
                let force = match nudge.force {
                    crate::dcp_auto::NudgeForce::Soft => "advisory",
                    crate::dcp_auto::NudgeForce::Hard => "required before more work",
                };
                InputItem::message(
                    InputRole::Developer,
                    format!("DCP reminder ({force}): {}", nudge.text),
                )
            });
            if let Some(nudge) = &nudge {
                nudge_hint = Some(nudge.text.clone());
                self.stats.lock().expect("stats lock").nudges_emitted += 1;
            }
            let boundary_estimate = crate::compaction::estimate_context(
                &fixed_input,
                &projected_continuation,
                &[],
                &tool_defs,
            );
            let automatic_due = last_compacted_round != Some(rounds)
                && !history.is_empty()
                && self
                    .compaction_estimate(
                        &params.session,
                        &usage_scope,
                        &projected_continuation,
                        &[],
                        boundary_estimate,
                    )?
                    .is_some_and(|estimate| {
                        compaction_config.required(
                            estimate,
                            compaction_context,
                            compaction_output,
                            compaction_input,
                        )
                    });
            if automatic_due {
                self.queue_compaction(
                    &params.session,
                    oc_core::compaction::CompactionReason::Automatic,
                )?;
            }
            let compacted = self
                .deliver_compaction_bound(
                    &params.session,
                    params.catalog,
                    &selection.id,
                    params.variant.as_deref(),
                    &params.provider,
                    Some(params.cancel),
                )
                .await?;
            if params.cancel.load(Ordering::Relaxed) {
                continue;
            }
            if (overflow_pending || automatic_due) && !compacted {
                let mut report = self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Failed,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                )?;
                report.diagnostic = Some(
                    if overflow_pending {
                        "context overflow; compaction failed without changing context"
                    } else {
                        "automatic compaction failed without changing context"
                    }
                    .into(),
                );
                return Ok(report);
            }
            overflow_pending = false;
            if compacted {
                last_compacted_round = Some(rounds);
                let refreshed = self.active_projection(&params.session)?;
                history = self.wire_history(
                    &params.session,
                    &refreshed.projected,
                    &refreshed.blocks,
                    &selection.id,
                    &params.catalog.provider,
                    lane.agent_digest.as_deref(),
                    refreshed.after_seq,
                )?;
                // The running turn's journal is supplied separately below.
                // Its active user row must not appear twice after refresh.
                if let Some(anchor) = turn_log.user_message.as_deref()
                    && let Some(index) = refreshed.projected.iter().position(|row| row.0 == anchor)
                {
                    let prior = &refreshed.projected[..index];
                    history = self.wire_history(
                        &params.session,
                        prior,
                        &refreshed.blocks,
                        &selection.id,
                        &params.catalog.provider,
                        lane.agent_digest.as_deref(),
                        refreshed.after_seq,
                    )?;
                }
                projected = refreshed.projected;
                let raw_context: Vec<_> = history.iter().chain(&turn_log.input).cloned().collect();
                tool_projection = self
                    .db
                    .dcp_tool_projection_for_input(&params.session, &raw_context)?;
                anchors = compress_available
                    .then(|| dcp_config_input(&projected, &dcp_config))
                    .flatten();
            }
            let continuation =
                dcp_continuation(&history, &current_instruction_input, &tool_projection);
            crate::instructions::reconcile(&mut fixed_input, &continuation, &instruction_facts);
            let mut input: Vec<InputItem> = fixed_input
                .iter()
                .chain(nudge_input.iter())
                .chain(anchors.iter())
                .chain(&continuation)
                .cloned()
                .collect();
            let request_estimate = estimate_tokens(
                &serde_json::to_string(&(&input, &tool_defs)).map_err(|_| RuntimeError::Storage)?,
            )
            .saturating_add(
                input
                    .iter()
                    .filter_map(|item| match item {
                        InputItem::ReadFunctionCallOutput { output, .. } => {
                            Some(output.estimated_tokens().saturating_add(
                                (output.facts().to_string().len() as u64).div_ceil(4),
                            ))
                        }
                        _ => None,
                    })
                    .sum::<u64>(),
            );
            let read_log_base = turn_log.to_json().to_string().len();
            let read_context_bytes = serde_json::to_vec(&input)
                .map_err(|_| RuntimeError::Storage)?
                .len()
                .saturating_add(
                    input
                        .iter()
                        .filter_map(|item| match item {
                            InputItem::ReadFunctionCallOutput { output, .. } => {
                                Some(output.facts().to_string().len())
                            }
                            _ => None,
                        })
                        .sum::<usize>(),
                );
            if let Err(error) = models::admit_budget(&selection, request_estimate, budget) {
                let mut report = self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Failed,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                )?;
                report.diagnostic = Some(error.to_string());
                return Ok(report);
            }
            let mut stream_started = std::time::Instant::now();
            let mut reasoning_started: Option<std::time::Instant> = None;
            let mut reasoning_closed = false;
            // Stream slots are private until canonical output supplies input
            // references. Their positions preserve text between reasoning items.
            struct TextSlot {
                text: String,
                part: usize,
                item_id: String,
                output_index: Option<u64>,
            }
            let mut text_slots: Vec<TextSlot> = Vec::new();
            let mut active_text: Option<usize> = None;
            let mut reasoning_anchors: Vec<(usize, String)> = Vec::new();
            let mut pending_tools = PendingToolStreams::new(rounds + 1);
            if let Some(previous) = turn_log.spans.last_mut()
                && previous.completed.is_none()
            {
                previous.retry = None;
                previous.error = None;
                previous.finish = None;
                previous.status = "interrupted".into();
                previous.completed = Some(millis());
            }
            turn_log.spans.push(oc_core::queries::AssistantSpan {
                id: next_turn_id("assistant", millis()),
                step: rounds + 1,
                status: "started".into(),
                started: millis(),
                completed: None,
                retry: None,
                error: None,
                finish: None,
            });
            let generation = loop {
                let mut partial_opaque = Vec::new();
                let mut semantic_started = false;
                let mut checkpoint_failed = false;
                self.db
                    .checkpoint_turn(&turn_id, &turn_log.to_json().to_string())?;
                let result = {
                    let mut observe = |item: &crate::provider::StreamItem| {
                        if !semantic_started
                            && !matches!(
                                item,
                                crate::provider::StreamItem::Usage { .. }
                                    | crate::provider::StreamItem::MessageBoundary { .. }
                            )
                        {
                            semantic_started = true;
                            let span = turn_log.spans.last_mut().expect("active span");
                            span.retry = None;
                            span.error = None;
                            span.finish = None;
                            span.started = millis();
                            if self
                                .db
                                .checkpoint_turn(&turn_id, &turn_log.to_json().to_string())
                                .is_err()
                                || self
                                    .publish_span_projection(&params.session, &turn_id)
                                    .is_err()
                            {
                                checkpoint_failed = true;
                            }
                        }
                        match item {
                            crate::provider::StreamItem::ToolCallStarted {
                                item_id,
                                call_id,
                                name,
                            } => {
                                if let Some(event) = pending_tools.announce(item_id, call_id, name)
                                {
                                    tool_event(&turn_id, &ToolCallEvent::ArgumentStream(event));
                                }
                            }
                            crate::provider::StreamItem::ArgDelta { item_id, delta } => {
                                if let Some(event) = pending_tools.delta(item_id, delta) {
                                    tool_event(&turn_id, &ToolCallEvent::ArgumentStream(event));
                                }
                            }
                            crate::provider::StreamItem::TextDelta(delta) => {
                                if !delta.is_empty() && active_text.is_none() {
                                    active_text = Some(text_slots.len());
                                    text_slots.push(TextSlot {
                                        text: String::new(),
                                        part: turn_log.display_parts.len(),
                                        item_id: String::new(),
                                        output_index: None,
                                    });
                                    turn_log
                                        .display_parts
                                        .push(serde_json::json!({"pending_text":true}));
                                }
                                if let Some(index) = active_text {
                                    text_slots[index].text.push_str(delta);
                                }
                                text_delta(&turn_id, delta);
                            }
                            crate::provider::StreamItem::MessageBoundary {
                                item_id,
                                output_index,
                                done,
                            } => {
                                let slot = active_text
                                    .filter(|&index| {
                                        let current = &text_slots[index];
                                        (item_id.is_empty()
                                            || current.item_id.is_empty()
                                            || current.item_id == *item_id)
                                            && (output_index.is_none()
                                                || current.output_index.is_none()
                                                || current.output_index == *output_index)
                                    })
                                    .or_else(|| {
                                        text_slots.iter().position(|slot| {
                                            (!item_id.is_empty() && slot.item_id == *item_id)
                                                || (output_index.is_some()
                                                    && slot.output_index == *output_index)
                                        })
                                    });
                                let slot = slot.unwrap_or_else(|| {
                                    let index = text_slots.len();
                                    text_slots.push(TextSlot {
                                        text: String::new(),
                                        part: turn_log.display_parts.len(),
                                        item_id: String::new(),
                                        output_index: None,
                                    });
                                    turn_log
                                        .display_parts
                                        .push(serde_json::json!({"pending_text":true}));
                                    index
                                });
                                if !item_id.is_empty() {
                                    text_slots[slot].item_id.clone_from(item_id);
                                }
                                if output_index.is_some() {
                                    text_slots[slot].output_index = *output_index;
                                }
                                active_text = (!done).then_some(slot);
                            }
                            crate::provider::StreamItem::ReasoningDelta(delta)
                                if !delta.is_empty() =>
                            {
                                active_text = None;
                                reasoning_started.get_or_insert_with(std::time::Instant::now);
                                if let Some(last) = turn_log.display_parts.last_mut()
                                    && let Some(text) =
                                        last.get("reasoning").and_then(|v| v.as_str())
                                    && !reasoning_closed
                                {
                                    let mut text = text.to_string();
                                    if text.len() + delta.len() > 16 * 1024 {
                                        last["truncated"] = true.into();
                                    }
                                    if text.len() < 16 * 1024 {
                                        text.push_str(delta);
                                    }
                                    last["reasoning"] = truncate(&text, 16 * 1024).into();
                                } else {
                                    turn_log.display_parts.push(serde_json::json!({"reasoning":truncate(delta, 16 * 1024), "truncated":delta.len()>16*1024}));
                                }
                                reasoning_closed = false;
                                reasoning_delta(&turn_id, delta);
                            }
                            crate::provider::StreamItem::OpaqueItem { item_id, payload } => {
                                if payload["type"] != "function_call"
                                    && payload["type"] != "message"
                                {
                                    partial_opaque.push((
                                        turn_log.display_parts.len().saturating_sub(1),
                                        payload.clone(),
                                    ));
                                }
                                if !item_id.is_empty()
                                    && payload.get("type").and_then(|v| v.as_str())
                                        == Some("reasoning")
                                    && reasoning_started.is_some()
                                {
                                    let part = turn_log.display_parts.len().saturating_sub(1);
                                    if let Some(last) = turn_log.display_parts.last_mut()
                                        && last.get("reasoning").is_some()
                                        && !reasoning_closed
                                    {
                                        last["duration_ms"] = streamed_ms(
                                            reasoning_started
                                                .take()
                                                .expect("active reasoning")
                                                .elapsed(),
                                        )
                                        .into();
                                        reasoning_anchors.push((part, item_id.clone()));
                                        reasoning_closed = true;
                                        reasoning_item_ended(&turn_id);
                                    }
                                }
                            }
                            _ => {}
                        }
                    };
                    crate::provider::stream_input_counted(
                        &params.provider,
                        &selection.id,
                        selection.variant.as_ref(),
                        &input,
                        &tool_defs,
                        budget.output,
                        params.cancel,
                        &mut observe,
                        &mut || async {
                            self.db
                                .generation_dispatch(
                                    &params.session,
                                    &turn_id,
                                    if primary_request { "main" } else { "child" },
                                )
                                .map_err(|_| crate::provider::ProviderError::DispatchRefused)
                        },
                    )
                    .await
                };
                match result {
                    Ok(generation) => {
                        if checkpoint_failed {
                            return Err(RuntimeError::Storage);
                        }
                        streamed += stream_started.elapsed();
                        let span = turn_log.spans.last_mut().expect("active span");
                        span.status = "completed".into();
                        span.finish = Some(
                            if generation.finish == crate::provider::FinishReason::Length {
                                "length"
                            } else {
                                "stop"
                            }
                            .into(),
                        );
                        span.completed = Some(millis());
                        break generation;
                    }
                    Err(error) => {
                        if checkpoint_failed {
                            return Err(RuntimeError::Storage);
                        }
                        turn_log.spans.last_mut().expect("active span").error =
                            Some(error.to_string());
                        tool_event(
                            &turn_id,
                            &ToolCallEvent::ArgumentStream(
                                oc_core::tool_stream::ToolStreamEvent::Clear { round: rounds + 1 },
                            ),
                        );
                        if error.is_context_overflow()
                            && !semantic_started
                            && !overflow_recovered
                            && compaction_config.auto
                        {
                            overflow_recovered = true;
                            overflow_pending = true;
                            self.queue_compaction(
                                &params.session,
                                oc_core::compaction::CompactionReason::Overflow,
                            )?;
                            // Continue the same logical step/journal. No tool execution is retried.
                            continue 'step;
                        }
                        streamed += stream_started.elapsed();
                        let output_started = matches!(&error, crate::provider::ProviderError::Request(f) if f.output_committed);
                        if output_started {
                            let mut partial_items = partial_opaque
                                .into_iter()
                                .map(|(part, opaque)| {
                                    turn_log.opaque.push(opaque.clone());
                                    (part, InputItem::ProviderOutput(opaque), false)
                                })
                                .collect::<Vec<_>>();
                            for slot in &text_slots {
                                if !slot.text.is_empty() {
                                    partial_items.push((slot.part,InputItem::ProviderOutput(serde_json::json!({
                                "type":"message", "role":"assistant", "status":"incomplete",
                                "content":[{"type":"output_text","text":slot.text}]
                            })),true));
                                }
                            }
                            partial_items.sort_by_key(|(part, _, _)| *part);
                            for (part, item, message) in partial_items {
                                let index = turn_log.input.len();
                                // Explicit partial status: never invent canonical completion.
                                turn_log.input.push(item);
                                if message {
                                    turn_log.display_parts[part] = serde_json::json!({"message":index,"span":turn_log.spans.last().expect("active").id});
                                }
                            }
                            let span = turn_log.spans.last_mut().expect("active span");
                            span.status = "failed".into();
                            span.finish = Some("error".into());
                            span.completed = Some(millis());
                        }
                        turn_log
                            .display_parts
                            .retain(|part| part.get("pending_text").is_none());
                        if !params.cancel.load(Ordering::Relaxed)
                            && let Some(decision) = retry_policy.decide(&error, millis())
                        {
                            turn_log.spans.last_mut().expect("active span").retry =
                                Some(decision.clone());
                            self.publish_retry(&params.session, &turn_log, &decision)?;
                            if retry::wait(&decision, params.cancel).await {
                                if output_started {
                                    // Resume from the committed journal and current DCP
                                    // projection, keeping the admitted binding/tools fixed.
                                    let (status, raw) = self.db.turn_result(&turn_id)?;
                                    if status != "started" {
                                        return Err(RuntimeError::Storage);
                                    }
                                    let saved: serde_json::Value =
                                        serde_json::from_str(&raw.ok_or(RuntimeError::Storage)?)
                                            .map_err(|_| RuntimeError::Storage)?;
                                    turn_log = TurnLog::from_json(&saved)
                                        .map_err(|_| RuntimeError::Storage)?;
                                    let refreshed = self.active_projection(&params.session)?;
                                    let prior = turn_log
                                        .user_message
                                        .as_deref()
                                        .and_then(|anchor| {
                                            refreshed
                                                .projected
                                                .iter()
                                                .position(|row| row.0 == anchor)
                                        })
                                        .map_or(refreshed.projected.as_slice(), |index| {
                                            &refreshed.projected[..index]
                                        });
                                    history = self.wire_history(
                                        &params.session,
                                        prior,
                                        &refreshed.blocks,
                                        &selection.id,
                                        &params.catalog.provider,
                                        lane.agent_digest.as_deref(),
                                        refreshed.after_seq,
                                    )?;
                                    projected = refreshed.projected;
                                    let raw_context: Vec<_> =
                                        history.iter().chain(&turn_log.input).cloned().collect();
                                    tool_projection = self.db.dcp_tool_projection_for_input(
                                        &params.session,
                                        &raw_context,
                                    )?;
                                    anchors = compress_available
                                        .then(|| dcp_config_input(&projected, &dcp_config))
                                        .flatten();
                                    turn_log.input.push(InputItem::message(InputRole::Developer,
                                    "The previous response was interrupted. Continue from where you left off without repeating completed content."));
                                    input = fixed_input
                                        .iter()
                                        .chain(nudge_input.iter())
                                        .chain(anchors.iter())
                                        .chain(
                                            dcp_continuation(
                                                &history,
                                                &crate::instructions::project(
                                                    &turn_log.input,
                                                    &turn_log.instruction_references,
                                                    &instruction_facts,
                                                ),
                                                &tool_projection,
                                            )
                                            .iter(),
                                        )
                                        .cloned()
                                        .collect();
                                    turn_log.spans.push(oc_core::queries::AssistantSpan {
                                        id: next_turn_id("assistant", millis()),
                                        step: rounds + 1,
                                        status: "started".into(),
                                        started: millis(),
                                        completed: None,
                                        retry: None,
                                        error: None,
                                        finish: None,
                                    });
                                }
                                if let Err(error) = models::admit_budget(
                                    &selection,
                                    estimate_tokens(
                                        &serde_json::to_string(&(&input, &tool_defs))
                                            .map_err(|_| RuntimeError::Storage)?,
                                    ),
                                    budget,
                                ) {
                                    let mut report = self.commit_turn(
                                        &turn_log,
                                        turn_id,
                                        &params.session,
                                        TurnStatus::Failed,
                                        text,
                                        rounds,
                                        streamed_ms(streamed),
                                        usage,
                                        context_usage,
                                        calls,
                                        nudge_hint,
                                        &published,
                                    )?;
                                    report.diagnostic = Some(error.to_string());
                                    return Ok(report);
                                }
                                text_slots.clear();
                                active_text = None;
                                reasoning_anchors.clear();
                                reasoning_started = None;
                                reasoning_closed = false;
                                pending_tools = PendingToolStreams::new(rounds + 1);
                                stream_started = std::time::Instant::now();
                                continue;
                            }
                        }
                        let status = if params.cancel.load(Ordering::Relaxed) {
                            TurnStatus::Cancelled
                        } else if error.is_incomplete() {
                            TurnStatus::Incomplete
                        } else {
                            TurnStatus::Failed
                        };
                        let mut report = self.commit_turn(
                            &turn_log,
                            turn_id,
                            &params.session,
                            status,
                            text,
                            rounds,
                            streamed_ms(streamed),
                            usage,
                            context_usage,
                            calls,
                            nudge_hint,
                            &published,
                        )?;
                        report.diagnostic = Some(error.to_string());
                        return Ok(report);
                    }
                }
            };
            rounds += 1;
            retry_policy = retry::RetryPolicy::default();
            for event in pending_tools.flush() {
                tool_event(&turn_id, &ToolCallEvent::ArgumentStream(event));
            }
            // Pinned runner owns one overflow rebuild per logical LLM step.
            // A successfully settled response starts the next step's allowance.
            overflow_recovered = false;
            if generation.finish == crate::provider::FinishReason::Length {
                turn_log.display["finish_reason"] = serde_json::json!("length");
            }
            for item in &generation.items {
                turn_log.ingest(item);
            }
            // Completed structured messages are authoritative even when a
            // proxy omitted output_text.delta (or only streamed one of several
            // message items). Never derive public text from reasoning payloads.
            let canonical_text: String = generation
                .output
                .iter()
                .filter(|item| item["type"] == "message" && item["role"] == "assistant")
                .filter_map(|item| item["content"].as_array())
                .flat_map(|content| content.iter())
                .filter(|part| part["type"] == "output_text")
                .filter_map(|part| part["text"].as_str())
                .collect();
            text.push_str(if canonical_text.is_empty() {
                &generation.text
            } else {
                &canonical_text
            });
            // A generation's measured context remains useful for display when
            // another round omitted usage and the billed turn total is unknown.
            if let Some(reported) = generation.usage {
                context_usage = Some(reported);
            }
            // Usage: the last round's input tokens, output summed over rounds
            // (upstream aggregates per-step output for tok/s, runtime.rs docs).
            usage_complete &= generation.usage.is_some();
            if usage_complete && let Some((input, output)) = generation.usage {
                let total = usage.map(|(_, previous)| previous).unwrap_or(0) + output;
                usage = Some((input, total));
            } else {
                // A partially reported sum is not a known turn total.
                usage = None;
            }
            // Calls come only from complete canonical output, never partial deltas.
            let mut call_items = Vec::new();
            for item in &generation.output {
                if item["type"] == "function_call" {
                    call_items.push(crate::provider::StreamItem::ToolCallStarted {
                        item_id: item["id"].as_str().unwrap_or_default().to_string(),
                        call_id: item["call_id"].as_str().unwrap_or_default().to_string(),
                        name: item["name"].as_str().unwrap_or_default().to_string(),
                    });
                    call_items.push(crate::provider::StreamItem::ArgDelta {
                        item_id: item["id"].as_str().unwrap_or_default().to_string(),
                        delta: item["arguments"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            let units = match assemble_calls(&call_items) {
                Ok(units) => units,
                Err(error) => {
                    turn_log
                        .display_parts
                        .retain(|part| part.get("pending_text").is_none());
                    tool_event(
                        &turn_id,
                        &ToolCallEvent::ArgumentStream(
                            oc_core::tool_stream::ToolStreamEvent::Clear { round: rounds },
                        ),
                    );
                    calls.push(CallRecord {
                        name: "unknown".to_string(),
                        state: "failed".to_string(),
                        output: truncate(
                            &format!("error: batch assembly: {error}"),
                            REPORT_OUTPUT_CAP,
                        ),
                    });
                    return self.commit_turn(
                        &turn_log,
                        turn_id,
                        &params.session,
                        TurnStatus::Failed,
                        text,
                        rounds,
                        streamed_ms(streamed),
                        usage,
                        context_usage,
                        calls,
                        nudge_hint,
                        &published,
                    );
                }
            };
            let has_message = generation
                .output
                .iter()
                .any(|value| value["type"] == "message");
            let generation_output_positions = generation
                .output
                .iter()
                .enumerate()
                .filter_map(|(index, output)| {
                    output["id"].as_str().map(|id| (index, id.to_owned()))
                })
                .collect::<Vec<_>>();
            // Assembly is ordered by the canonical function_call items. Match
            // both provider identities before using an output position; absent
            // or ambiguous identities retain the append-at-intent fallback.
            let call_positions = units
                .iter()
                .map(|unit| {
                    let id = match unit {
                        Assembled::Call(call) => &call.id,
                        Assembled::Failed(failure) => &failure.id,
                    };
                    if id.is_empty() {
                        return None;
                    }
                    let mut matches = generation.output.iter().enumerate().filter(|(_, item)| {
                        item["type"] == "function_call"
                            && item["id"]
                                .as_str()
                                .is_some_and(|item_id| !item_id.is_empty())
                            && item["call_id"].as_str() == Some(id.as_str())
                    });
                    let (index, _) = matches.next()?;
                    matches.next().is_none().then_some(index)
                })
                .collect::<Vec<_>>();
            let mut messages = Vec::new();
            let stream_identities = call_positions
                .iter()
                .map(|position| {
                    let item = generation.output.get((*position)?)?;
                    Some(oc_core::tool_stream::ToolStreamIdentity {
                        round: rounds,
                        item_id: item["id"].as_str()?.to_string(),
                        call_id: item["call_id"].as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>();
            for (index, output) in generation.output.into_iter().enumerate() {
                if output["type"] == "message" && output["role"] == "assistant" {
                    messages.push((
                        index,
                        output["id"].as_str().unwrap_or("").to_owned(),
                        turn_log.input.len(),
                    ));
                }
                turn_log.input.push(InputItem::ProviderOutput(output));
            }
            // Text-only synthetic peers may omit canonical messages. This is plain
            // assistant text, never reconstruction of reasoning or function calls.
            if !generation.text.is_empty() && !has_message {
                messages.push((usize::MAX, String::new(), turn_log.input.len()));
                turn_log
                    .input
                    .push(InputItem::message(InputRole::Assistant, &generation.text));
            }
            // Responses input totals already include cached input; output totals
            // already include reasoning. Count each exactly once. Never use the
            // accumulated billing/display turn total as the next context size.
            if let Some((input, output)) = generation.usage
                && input > 0
            {
                let measured_prefix = dcp_continuation(&history, &turn_log.input, &tool_projection);
                self.db.save_usage_anchor(
                    &params.session,
                    &crate::compaction::UsageAnchor {
                        scope: usage_scope.clone(),
                        prefix: crate::compaction::fingerprint(&measured_prefix),
                        items: measured_prefix.len(),
                        tokens: input.saturating_add(output),
                    },
                )?;
            }
            let mut assigned = vec![false; messages.len()];
            let mut positioned = reasoning_anchors
                .iter()
                .filter_map(|(part, id)| {
                    generation_output_positions
                        .iter()
                        .find(|(_, output_id)| output_id == id)
                        .map(|(index, _)| (*index, *part))
                })
                .collect::<Vec<_>>();
            // Claim observed identities first: an earlier anonymous slot must
            // not steal the message belonging to a later identified slot.
            for slot in text_slots
                .iter()
                .filter(|slot| slot.output_index.is_some() || !slot.item_id.is_empty())
            {
                let mut matches = messages.iter().enumerate().filter(|(i, (index, id, _))| {
                    !assigned[*i]
                        && (slot
                            .output_index
                            .is_some_and(|position| position == *index as u64)
                            || (!slot.item_id.is_empty() && slot.item_id == *id))
                });
                if let Some((i, _)) = matches.next()
                    && matches.next().is_none()
                {
                    assigned[i] = true;
                    positioned.push((messages[i].0, slot.part));
                    turn_log.display_parts[slot.part] =
                        serde_json::json!({"message":messages[i].2});
                }
            }
            // Without an identity, a stream slot only identifies a message if
            // there is exactly one remaining. Otherwise remove the placeholder
            // and insert the canonical messages against reasoning anchors below.
            for slot in text_slots
                .iter()
                .filter(|slot| slot.output_index.is_none() && slot.item_id.is_empty())
            {
                let mut candidates = assigned.iter().enumerate().filter(|(_, used)| !**used);
                if let Some((i, _)) = candidates.next()
                    && candidates.next().is_none()
                {
                    assigned[i] = true;
                    positioned.push((messages[i].0, slot.part));
                    turn_log.display_parts[slot.part] =
                        serde_json::json!({"message":messages[i].2});
                }
            }
            let removed = text_slots
                .iter()
                .filter(|slot| {
                    turn_log.display_parts[slot.part]
                        .get("pending_text")
                        .is_some()
                })
                .map(|slot| slot.part)
                .collect::<Vec<_>>();
            for (_, part) in &mut positioned {
                *part -= removed.iter().filter(|removed| **removed < *part).count();
            }
            turn_log
                .display_parts
                .retain(|part| part.get("pending_text").is_none());
            for (i, (index, _, input_index)) in messages.into_iter().enumerate() {
                if !assigned[i] {
                    let position = positioned
                        .iter()
                        .filter(|(other, _)| *other > index)
                        .map(|(_, part)| *part)
                        .min()
                        .unwrap_or(turn_log.display_parts.len());
                    turn_log
                        .display_parts
                        .insert(position, serde_json::json!({"message":input_index}));
                    for (_, part) in &mut positioned {
                        if *part >= position {
                            *part += 1;
                        }
                    }
                    positioned.push((index, position));
                }
            }
            self.db
                .checkpoint_turn(&turn_id, &turn_log.to_json().to_string())?;
            let executed = self
                .execute_units(
                    &turn_id,
                    &params.session,
                    &units,
                    &ctx,
                    &policy,
                    attached,
                    params.cancel,
                    rounds,
                    &mut turn_log,
                    &mut positioned,
                    &call_positions,
                    &stream_identities,
                    &state_key,
                    &mut tool_projection,
                    tool_event,
                    selection
                        .entry
                        .pointer("/modalities/input")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|m| m.iter().any(|v| v == "image")),
                    budget.input.saturating_sub(request_estimate),
                    ACTIVE_CONTEXT_BYTES_CAP.saturating_sub(read_context_bytes),
                    read_log_base,
                )
                .await;
            tool_event(
                &turn_id,
                &ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Clear {
                    round: rounds,
                }),
            );
            let (round_calls, projection_changed, permission_rejected) = executed?;
            calls.extend(round_calls);
            if permission_rejected {
                return self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Cancelled,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
            if calls.iter().any(|call| call.state == "unknown") {
                let mut report = self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Failed,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                )?;
                report.diagnostic =
                    Some("MCP outcome unknown; retry may duplicate side effects".into());
                return Ok(report);
            }
            if projection_changed {
                let refreshed = self.active_projection(&params.session)?;
                projected = refreshed.projected;
                // The current turn is appended separately below. Rehydrating
                // its checkpoint into history would duplicate its user prompt
                // and compress call/output in the actual continuation request.
                let prior = projected
                    .iter()
                    .filter(|row| {
                        turn_log.user_message.as_deref() != Some(row.0.as_str())
                            && !turn_log.shell_notice_messages.contains(&row.0)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                history = self.wire_history(
                    &params.session,
                    &prior,
                    &refreshed.blocks,
                    &selection.id,
                    &params.catalog.provider,
                    lane.agent_digest.as_deref(),
                    refreshed.after_seq,
                )?;
                anchors = compress_available
                    .then(|| dcp_config_input(&projected, &dcp_config))
                    .flatten();
            }
            if !units_have_calls(&units) {
                break;
            }
            if rounds >= max_rounds {
                return self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Incomplete,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
        }
        let mut report = self.commit_turn(
            &turn_log,
            turn_id,
            &params.session,
            TurnStatus::Completed,
            text,
            rounds,
            streamed_ms(streamed),
            usage,
            context_usage,
            calls,
            nudge_hint,
            &published,
        )?;
        if turn_log.display["finish_reason"] == "length" {
            report.diagnostic = Some("provider finish=length (max_output_tokens)".into());
        }
        Ok(report)
    }

    /// Run one child turn through the inner path, without the single-flight
    /// lease (the parent turn already holds it) and sharing the parent cancel
    /// flag so one Cancel stops both.
    #[allow(clippy::too_many_arguments)]
    async fn run_child_turn(
        &self,
        agent: &SubagentAgent,
        parent_lane: &TurnLane,
        owning_operation: &str,
        session: &str,
        prompt: String,
        model: &ResolvedModel,
        max_output: u64,
        catalog: &ModelCatalog,
        provider: &ResponsesConfig,
        attached: &McpGeneration,
        cancel: &AtomicBool,
    ) -> Result<TurnReport, RuntimeError> {
        let workspace = self.workspace.read().expect("workspace lock").clone();
        // Parent generation ∩ child agent rules: an agent rule can only make
        // the child lane stricter, never widen the caller's authority.
        let mut permissions = parent_lane.permissions.clone();
        let mut permission_rules = parent_lane.permission_rules.clone();
        let mut agent_rules = agent.permission_rules.clone();
        if let Some(home) = self.parent_env.get("HOME") {
            agent_rules.expand_home(home);
        }
        permission_rules.narrow(&permissions, &agent.permissions, &agent_rules);
        for (tool, level) in &agent.permissions {
            permissions
                .entry(tool.clone())
                .and_modify(|current| {
                    if permission_rank(*level) > permission_rank(*current) {
                        *current = *level;
                    }
                })
                .or_insert(*level);
        }
        let lane = TurnLane {
            owning_operation: Some(owning_operation.into()),
            agent_id: Some(agent.id.clone()),
            agent_color_index: workspace
                .subagents
                .as_ref()
                .and_then(|catalog| catalog.agents.keys().position(|id| id == &agent.id)),
            fixed_input: lane_fixed_input(
                Some(&agent.prompt),
                if workspace.instruction_roots.is_empty() {
                    &workspace.instructions
                } else {
                    ""
                },
                workspace.skills_projection.as_deref(),
            ),
            agent_digest: agent.digest.clone(),
            permissions,
            permission_rules,
        };
        let params = TurnParams {
            session: session.to_string(),
            prompt,
            invocation: None,
            catalog,
            model_id: model.id.clone(),
            variant: model.variant.clone(),
            max_output,
            provider: provider.clone(),
            cancel,
            max_rounds: MAX_ROUNDS,
        };
        self.run_turn_inner(
            params,
            &lane,
            attached,
            None,
            &mut |_: &str, _| {},
            &mut |_: &str, _: &str| {},
            &mut |_: &str, _: &str| {},
            &mut |_: &str| {},
            &mut |_: &str, _: &ToolCallEvent| {},
        )
        .await
    }

    /// Ancestor depth of a session (root = 0, direct child = 1).
    fn session_depth(&self, session: &str) -> Result<u32, RuntimeError> {
        let mut depth = 0u32;
        let mut current = session.to_string();
        for _ in 0..=SUBAGENT_DEPTH_WALK_CAP {
            match self.db.session_meta(&current)?.parent_id {
                Some(parent) => {
                    depth = depth.saturating_add(1);
                    current = parent;
                }
                None => return Ok(depth),
            }
        }
        Err(RuntimeError::InvalidArgs(
            "session parent chain is too deep".to_string(),
        ))
    }

    /// Unique child session id for one spawn (monotonic within the runtime).
    fn new_child_id(&self, parent: &str) -> String {
        let seq = self.subagent_seq.fetch_add(1, Ordering::Relaxed);
        format!("{parent}-sub-{}-{seq}", millis())
    }

    /// Commit durable records under a freshness check, then report.
    ///
    /// The assistant message and turn log persist only under the
    /// still-current generation (DCP08: no half-applied projection).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn commit_turn(
        &self,
        turn_log: &TurnLog,
        turn_id: String,
        _session: &str,
        status: TurnStatus,
        text: String,
        rounds: u32,
        streamed_ms: u64,
        usage: Option<(u64, u64)>,
        context_usage: Option<(u64, u64)>,
        calls: Vec<CallRecord>,
        nudge_hint: Option<String>,
        published: &PublishedGeneration,
    ) -> Result<TurnReport, RuntimeError> {
        if self.generation_id() != published.id {
            self.db
                .finish_turn(&turn_id, TurnStatus::Interrupted.as_str(), None)?;
            return Err(RuntimeError::StaleGeneration {
                want: published.id,
                got: self.generation_id(),
            });
        }
        let assistant =
            (status == TurnStatus::Completed && !text.is_empty()).then_some(text.as_str());
        let mut turn_log = turn_log.clone();
        if let Some(span) = turn_log.spans.last_mut()
            && span.completed.is_none()
        {
            span.status = status.as_str().into();
            if span.error.is_some() {
                span.finish = Some("error".into());
            }
            span.completed = Some(millis());
        }
        self.db.commit_turn(
            &turn_id,
            status.as_str(),
            Some(&turn_log.to_json().to_string()),
            assistant,
        )?;
        Ok(TurnReport {
            turn_id,
            status,
            diagnostic: None,
            text,
            rounds,
            usage,
            context_usage,
            streamed_ms,
            duration_ms: 0,
            calls,
            nudge_hint,
            warnings: Vec::new(),
        })
    }

    /// Execute one assembled batch: builtins via the executor, MCP via
    /// attached clients, failures as visible outputs. Every unit gets
    /// intent + outcome rows under the same policy object.
    #[allow(clippy::too_many_arguments)]
    async fn execute_units(
        &self,
        turn_id: &str,
        session: &str,
        units: &[Assembled],
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        attached: &McpGeneration,
        cancel: &AtomicBool,
        round: u32,
        turn_log: &mut TurnLog,
        positioned: &mut Vec<(usize, usize)>,
        call_positions: &[Option<usize>],
        stream_identities: &[Option<oc_core::tool_stream::ToolStreamIdentity>],
        nudge_key: &str,
        tool_projection: &mut crate::storage::DcpToolProjection,
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
        read_images: bool,
        read_tokens: u64,
        read_bytes: usize,
        read_log_base: usize,
    ) -> Result<(Vec<CallRecord>, bool, bool), RuntimeError> {
        let mut records = Vec::new();
        let mut projection_changed = false;
        let mut permission_rejected = false;
        let mut read_extra_bytes = 0usize;
        let mut read_visual_tokens = 0u64;
        for (i, unit) in units.iter().enumerate() {
            let (id, name, input) = match unit {
                Assembled::Call(call) => (&call.id, call.name.as_str(), call.arguments.to_string()),
                Assembled::Failed(failure) => (&failure.id, "unknown", "{}".to_string()),
            };
            // Keep the original provider identifier in the durable operation id.
            let op = format!("{turn_id}-r{round}-c{i}-{id}");
            let guarded = self.guard_patch(unit.clone());
            let admission = match &guarded {
                Assembled::Call(call)
                    if !permission_rejected
                        && !cancel.load(Ordering::Acquire)
                        && (!is_builtin(&call.name)
                            || crate::tools::validate_call(call).is_ok()) =>
                {
                    self.admit_tool(
                        session,
                        turn_id,
                        &op,
                        call,
                        ctx,
                        policy,
                        cancel,
                        turn_log.display["agent"].as_str().map(str::to_string),
                        turn_log.agent_digest.clone(),
                    )
                    .await
                }
                _ => Ok(policy.clone()),
            };
            if matches!(&admission, Err(AdmissionFailure::Required)) {
                return Err(RuntimeError::ApprovalRequired { tool: name.into() });
            }
            let invocation_policy = admission.as_ref().unwrap_or(policy);
            let invocation_ctx = ToolContext {
                files: ctx.files,
                shell: ctx.shell,
                parent_env: ctx.parent_env,
                webfetch_auth: ctx.webfetch_auth.clone(),
                webfetch_allow_private: ctx.webfetch_allow_private,
                policy: invocation_policy,
                subagent: ctx.subagent,
                snapshot: ctx.snapshot,
                cancel: ctx.cancel,
                roots: ctx.roots.clone(),
            };
            let ctx = &invocation_ctx;
            let mut rejection = match &guarded {
                Assembled::Failed(failure) => Some(("failed", format!("error: {}", failure.error))),
                Assembled::Call(call)
                    if is_builtin(&call.name) && crate::tools::validate_call(call).is_err() =>
                {
                    Some((
                        "failed",
                        format!(
                            "error: {}",
                            crate::tools::validate_call(call).expect_err("invalid shape")
                        ),
                    ))
                }
                Assembled::Call(call)
                    if call.name == COMPRESS_TOOL && {
                        let config = self.dcp_config.read().expect("dcp lock");
                        !config.enabled || config.manual_mode
                    } =>
                {
                    Some((
                        "failed",
                        "error: compress unavailable in disabled/manual DCP mode".to_string(),
                    ))
                }
                Assembled::Call(_) if matches!(&admission, Err(AdmissionFailure::Rejected(_))) => {
                    let Err(AdmissionFailure::Rejected(feedback)) = &admission else {
                        unreachable!()
                    };
                    Some((
                        "denied",
                        serde_json::json!({"status":"permission_rejected", "feedback":feedback})
                            .to_string(),
                    ))
                }
                Assembled::Call(_) if matches!(&admission, Err(AdmissionFailure::Cancelled)) => {
                    Some((
                        "cancelled",
                        serde_json::json!({"status":"permission_cancelled"}).to_string(),
                    ))
                }
                Assembled::Call(_) if admission.is_err() => Some((
                    if is_builtin(name) { "failed" } else { "denied" },
                    format!(
                        "error: {}",
                        admission.as_ref().err().expect("admission error")
                    ),
                )),
                Assembled::Call(call) if invocation_policy.check_call(call).is_err() => {
                    let state = if is_builtin(&call.name) {
                        "failed"
                    } else {
                        "denied"
                    };
                    Some((
                        state,
                        format!(
                            "error: {}",
                            invocation_policy.check_call(call).expect_err("rejected")
                        ),
                    ))
                }
                _ if permission_rejected || cancel.load(Ordering::Relaxed) => {
                    Some(("cancelled", "error: cancelled".to_string()))
                }
                _ => None,
            };
            let mut shell_slot = None;
            if rejection.is_none()
                && matches!(&guarded,Assembled::Call(call) if call.name=="shell" && call.arguments["background"]==true)
            {
                shell_slot = self.shell_jobs.reserve();
                if shell_slot.is_none() {
                    rejection = Some((
                        "failed",
                        "error: active background shell resource ceiling (8 jobs)".into(),
                    ));
                }
            }
            permission_rejected |= matches!(&admission, Err(AdmissionFailure::Rejected(None)));
            // Fail closed. No built-in or MCP dispatch can precede this commit.
            let position = call_positions[i]
                .and_then(|index| {
                    positioned
                        .iter()
                        .filter(|(other, _)| *other > index)
                        .map(|(_, part)| *part)
                        .min()
                })
                .unwrap_or(turn_log.display_parts.len());
            turn_log
                .display_parts
                .insert(position, serde_json::json!({"tool":op}));
            for (_, part) in positioned.iter_mut() {
                if *part >= position {
                    *part += 1;
                }
            }
            if let Some(index) = call_positions[i] {
                positioned.push((index, position));
            }
            if let Some((state, output)) = &rejection {
                turn_log.input.push(InputItem::FunctionCallOutput {
                    call_id: id.clone(),
                    output: output.clone(),
                });
                self.db.record_turn_tool_refusal(
                    &op,
                    session,
                    turn_id,
                    name,
                    &input,
                    state,
                    output,
                    &turn_log.to_json().to_string(),
                )?;
                if let Some(identity) = &stream_identities[i] {
                    tool_event(
                        turn_id,
                        &ToolCallEvent::ArgumentStream(
                            oc_core::tool_stream::ToolStreamEvent::Linked {
                                identity: identity.clone(),
                                op: op.clone(),
                            },
                        ),
                    );
                }
                emit_tool_finish_with_effects(tool_event, turn_id, &op, name, state, output, None);
                records.push(CallRecord {
                    name: name.to_string(),
                    state: state.to_string(),
                    output: truncate(output, REPORT_OUTPUT_CAP),
                });
                continue;
            }
            self.db.record_turn_tool_intent(
                &op,
                session,
                turn_id,
                name,
                &input,
                &turn_log.to_json().to_string(),
            )?;
            if let Some(identity) = &stream_identities[i] {
                tool_event(
                    turn_id,
                    &ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Linked {
                        identity: identity.clone(),
                        op: op.clone(),
                    }),
                );
            }
            tool_event(
                turn_id,
                &ToolCallEvent::Started {
                    dcp_topic: invocation_policy
                        .permit
                        .as_ref()
                        .and_then(|permit| permit.compression_plan.as_ref())
                        .and_then(|plan| plan.blocks.first())
                        .map(|block| block.topic.clone()),
                    op: op.clone(),
                    name: name.to_string(),
                    input: input.clone(),
                },
            );
            if rejection.is_none()
                && let Assembled::Call(call) = unit
                && call.name == COMPRESS_TOOL
            {
                let config = self.dcp_config.read().expect("dcp lock").clone();
                let prepared = invocation_policy
                    .permit
                    .as_ref()
                    .and_then(|p| p.compression_plan.clone())
                    .expect("compress admission prepared plan");
                match Ok::<_, crate::dcp::DcpError>(prepared) {
                    Ok(mut plan) => {
                        let published = self.current.read().expect("generation lock").clone();
                        let lane = self.primary_lane(&published);
                        let measured = self.measure_dcp_plan(
                            session,
                            &mut plan,
                            Some(turn_log),
                            &lane,
                            &config,
                        );
                        let (strategy_delta, candidate_tool_projection) = match measured {
                            Ok(value) => value,
                            Err(error) => {
                                let output = format!("error: {error}");
                                turn_log.input.push(InputItem::FunctionCallOutput {
                                    call_id: id.clone(),
                                    output: output.clone(),
                                });
                                record_tool_finish(
                                    self.db, tool_event, &op, name, "failed", &output, turn_id,
                                    turn_log,
                                )?;
                                records.push(CallRecord {
                                    name: name.into(),
                                    state: "failed".into(),
                                    output: truncate(&output, REPORT_OUTPUT_CAP),
                                });
                                continue;
                            }
                        };
                        let before_bytes = plan.before_bytes;
                        let after_bytes = plan.after_bytes;
                        if after_bytes >= before_bytes {
                            let output = serde_json::json!({
                                "status": "no_gain",
                                "beforeBytes": before_bytes,
                                "afterBytes": after_bytes,
                            })
                            .to_string();
                            turn_log.input.push(InputItem::FunctionCallOutput {
                                call_id: id.clone(),
                                output: output.clone(),
                            });
                            record_tool_finish(
                                self.db, tool_event, &op, name, "no_gain", &output, turn_id,
                                turn_log,
                            )?;
                            records.push(CallRecord {
                                name: name.to_string(),
                                state: "no_gain".to_string(),
                                output: truncate(&output, REPORT_OUTPUT_CAP),
                            });
                            continue;
                        }
                        let saved_tokens = plan.saved_tokens;
                        if cancel.load(Ordering::Acquire) {
                            let output = "error: cancelled".to_string();
                            turn_log.input.push(InputItem::FunctionCallOutput {
                                call_id: id.clone(),
                                output: output.clone(),
                            });
                            record_tool_finish(
                                self.db,
                                tool_event,
                                &op,
                                name,
                                "cancelled",
                                &output,
                                turn_id,
                                turn_log,
                            )?;
                            records.push(CallRecord {
                                name: name.into(),
                                state: "cancelled".into(),
                                output,
                            });
                            continue;
                        }
                        let output = serde_json::json!({
                            "status": "compressed",
                            "blocks": plan.blocks.iter().map(|block| block.id.clone()).collect::<Vec<_>>(),
                            "savedTokens": saved_tokens,
                            "beforeBytes": before_bytes,
                            "afterBytes": after_bytes,
                        })
                        .to_string();
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        let log = turn_log.to_json().to_string();
                        let next_nudge = {
                            let states = self.nudge_state.lock().expect("nudge lock");
                            let mut state = states.get(nudge_key).cloned().unwrap_or_default();
                            state.on_compress_success();
                            state
                        };
                        let preference_updates = vec![(
                            nudge_key.to_string(),
                            serde_json::to_string(&next_nudge)
                                .map_err(|_| RuntimeError::Storage)?,
                        )];
                        let metadata = crate::dcp::CompressionCommitMetadata {
                            operation_id: &op,
                            operation_state: "completed",
                            operation_output: &output,
                            turn_id: Some(turn_id),
                            turn_log: Some(&log),
                            preference_updates: &preference_updates,
                        };
                        let hidden = strategy_delta.hidden.iter().cloned().collect::<Vec<_>>();
                        let purged = strategy_delta.purged.iter().cloned().collect::<Vec<_>>();
                        let report = crate::dcp::commit_compression_with_projection(
                            self.db,
                            session,
                            plan,
                            Some(&metadata),
                            &hidden,
                            &purged,
                        )
                        .map_err(|error| RuntimeError::Compress(error.to_string()))?;
                        *tool_projection = candidate_tool_projection;
                        self.nudge_state
                            .lock()
                            .expect("nudge lock")
                            .insert(nudge_key.to_string(), next_nudge);
                        self.stats.lock().expect("stats lock").compressions += 1;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "completed".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        debug_assert!(!report.blocks.is_empty());
                        projection_changed = true;
                        emit_tool_finish_with_metadata(
                            tool_event,
                            turn_id,
                            &op,
                            name,
                            "completed",
                            &output,
                            None,
                            Some(report.snapshot),
                        );
                        continue;
                    }
                    Err(crate::dcp::DcpError::NoGain {
                        before_bytes,
                        after_bytes,
                    }) => {
                        let output = serde_json::json!({
                            "status": "no_gain",
                            "beforeBytes": before_bytes,
                            "afterBytes": after_bytes,
                        })
                        .to_string();
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        record_tool_finish(
                            self.db, tool_event, &op, name, "no_gain", &output, turn_id, turn_log,
                        )?;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "no_gain".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        continue;
                    }
                    Err(error) => {
                        let output = format!("error: {error}");
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        record_tool_finish(
                            self.db, tool_event, &op, name, "failed", &output, turn_id, turn_log,
                        )?;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "failed".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        continue;
                    }
                }
            }
            let mut patch_effects = None;
            let mut native_mcp_result = None;
            let mut native_read_result = None;
            let mut read_directory = false;
            let (state, output) = if let Some(rejection) = rejection {
                rejection
            } else {
                match unit {
                    Assembled::Call(call) if call.name == "question" => {
                        use oc_core::question::*;
                        use sha2::{Digest as _, Sha256};
                        let input =
                            QuestionInput::parse(&call.arguments).expect("validated question");
                        let events = self.compaction_events.lock().expect("events lock").clone();
                        let decision = if let Some(events) = events {
                            let request = QuestionRequest {
                                id: 0,
                                binding: oc_core::approval::ApprovalBinding {
                                    session: session.into(),
                                    turn: turn_id.into(),
                                    call: call.id.clone(),
                                    operation: op.clone(),
                                    input_digest: format!(
                                        "{:x}",
                                        Sha256::digest(call.arguments.to_string())
                                    ),
                                    location: self.location.clone(),
                                    generation: self.generation_id(),
                                    agent: turn_log.display["agent"].as_str().map(str::to_owned),
                                    agent_digest: turn_log.agent_digest.clone(),
                                },
                                input: input.clone(),
                            };
                            let waiting = self.questions.wait(request, &events);
                            tokio::pin!(waiting);
                            loop {
                                tokio::select! {
                                    value = &mut waiting => break value,
                                    () = tokio::time::sleep(Duration::from_millis(5)) => {
                                        if cancel.load(Ordering::Acquire) { break Ok(QuestionDecision::Cancelled); }
                                    }
                                }
                            }
                        } else {
                            Err(QuestionWaitError::NoConsumer)
                        };
                        match decision {
                            Ok(QuestionDecision::Answers(answers))
                                if !cancel.load(Ordering::Acquire) =>
                            {
                                (
                                    "completed",
                                    serde_json::to_string(&QuestionResult {
                                        questions: input.questions,
                                        answers,
                                    })
                                    .expect("typed question result"),
                                )
                            }
                            Ok(_) => {
                                cancel.store(true, Ordering::Release);
                                permission_rejected = true;
                                ("cancelled", "error: question dismissed".into())
                            }
                            Err(error) => {
                                let error = match error {
                                    QuestionWaitError::NoConsumer => RuntimeError::QuestionRequired,
                                    QuestionWaitError::InvalidInput => {
                                        RuntimeError::InvalidArgs("invalid question binding".into())
                                    }
                                    QuestionWaitError::Capacity => RuntimeError::InvalidArgs(
                                        "question capacity exceeded".into(),
                                    ),
                                    QuestionWaitError::OwnerClosed => {
                                        RuntimeError::InvalidArgs("question owner closed".into())
                                    }
                                };
                                let output = format!("error: {error}");
                                turn_log.input.push(InputItem::FunctionCallOutput {
                                    call_id: id.clone(),
                                    output: output.clone(),
                                });
                                record_tool_finish(
                                    self.db, tool_event, &op, name, "failed", &output, turn_id,
                                    turn_log,
                                )?;
                                return Err(error);
                            }
                        }
                    }
                    Assembled::Call(call) if call.name == "apply_patch" => {
                        let (output, effects) = crate::tools::tool_patch_typed(ctx, call);
                        patch_effects = effects;
                        (output_state(&output), output)
                    }
                    Assembled::Call(call)
                        if matches!(call.name.as_str(), "read" | "glob" | "grep") =>
                    {
                        // Same bounded spawn_blocking/join strategy as foreground
                        // shell. Capture this invocation's immutable authority,
                        // not a reloaded generation or another session's token.
                        let files = ctx.files.clone();
                        let call = call.clone();
                        let permissions = invocation_policy.permissions.clone();
                        let rules = invocation_policy.rules.cloned();
                        let root = invocation_policy.root.map(std::path::Path::to_path_buf);
                        let permit = invocation_policy.permit.clone();
                        let worker_cancel =
                            Arc::new(AtomicBool::new(ctx.cancel.load(Ordering::Acquire)));
                        struct CancelOnDrop(Arc<AtomicBool>);
                        impl Drop for CancelOnDrop {
                            fn drop(&mut self) {
                                self.0.store(true, Ordering::Release);
                            }
                        }
                        let _cleanup = CancelOnDrop(worker_cancel.clone());
                        let token = worker_cancel.clone();
                        let mut task = tokio::task::spawn_blocking(move || {
                            let policy = RuntimePolicy {
                                permissions: &permissions,
                                rules: rules.as_ref(),
                                root: root.as_deref(),
                                mcp: &[],
                                permit,
                            };
                            if call.name == "read" {
                                crate::tools::read::execute(
                                    &files,
                                    &policy,
                                    &token,
                                    &call,
                                    read_images,
                                )
                            } else {
                                let (state, output) =
                                    crate::tools::execute_search(&files, &policy, &token, &call);
                                crate::tools::read::Outcome {
                                    state,
                                    output,
                                    image: None,
                                    directory: false,
                                }
                            }
                        });
                        let result = loop {
                            tokio::select! {
                                result = &mut task => break result,
                                () = tokio::time::sleep(Duration::from_millis(5)) => {
                                    if ctx.cancel.load(Ordering::Acquire) { worker_cancel.store(true, Ordering::Release); }
                                }
                            }
                        };
                        // A cancellation racing the final worker checkpoint must
                        // also discard its result. Join precedes durable finish.
                        if ctx.cancel.load(Ordering::Acquire) {
                            ("cancelled", "error: cancelled".into())
                        } else {
                            let mut result = result.unwrap_or_else(|_| {
                                crate::tools::read::Outcome::error(
                                    "failed",
                                    "read/search worker failed",
                                )
                            });
                            if let Some(image) = &result.image {
                                let retained = image.retained_bytes();
                                let growth = turn_log
                                    .to_json()
                                    .to_string()
                                    .len()
                                    .saturating_sub(read_log_base);
                                let bytes = growth
                                    .saturating_add(read_extra_bytes)
                                    .saturating_add(retained);
                                let visual =
                                    read_visual_tokens.saturating_add(image.estimated_tokens());
                                let tokens = visual.saturating_add((bytes as u64).div_ceil(4));
                                if tokens > read_tokens || bytes > read_bytes {
                                    result = crate::tools::read::Outcome::error(
                                        "failed",
                                        "image exceeds selected model or retained-context budget; compress context or use a smaller image",
                                    );
                                } else {
                                    read_extra_bytes = read_extra_bytes.saturating_add(retained);
                                    read_visual_tokens = visual;
                                }
                            }
                            native_read_result = result.image;
                            read_directory = result.directory;
                            (result.state, result.output)
                        }
                    }
                    Assembled::Call(call) if matches!(call.name.as_str(), "shell" | "bash") => {
                        let invocation = crate::tools::shell_call::invocation(call, ctx.parent_env)
                            .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
                        if invocation.background {
                            let pinned =
                                invocation_policy.approved_shell_cwd().ok_or_else(|| {
                                    RuntimeError::InvalidArgs(
                                        "background shell cwd not pinned".into(),
                                    )
                                })?;
                            let provenance = crate::shell::jobs::Provenance {
                                version: 1,
                                session: session.into(),
                                turn: turn_id.into(),
                                operation: op.clone(),
                                location: self.location.clone(),
                                generation: self.generation_id(),
                                agent: turn_log.display["agent"].as_str().map(str::to_owned),
                                agent_digest: turn_log.agent_digest.clone(),
                                model: turn_log.model.clone(),
                                provider: turn_log.provider.clone(),
                                command: call.arguments["command"]
                                    .as_str()
                                    .expect("validated command")
                                    .into(),
                                cwd: pinned.path.to_string_lossy().into(),
                                selected_shell: invocation.argv[0].clone(),
                            };
                            match self.shell_jobs.launch(self.shell.clone(), self.parent_env.clone(),
                                invocation.argv, invocation.cwd, invocation.timeout, pinned, provenance,
                                shell_slot.take().expect("background reservation before intent")).await {
                                Ok(shell_id) => ("completed", serde_json::json!({"status":"running", "shellID":shell_id,
                                    "truncated":false,"output":"Background command launched. You will be notified automatically when it completes. DO NOT poll; continue independent work or end your response."}).to_string()),
                                Err(_) => ("failed", "error: background shell admission or launch failed".into()),
                            }
                        } else {
                            crate::tools::execute_shell_typed(ctx, call).await
                        }
                    }
                    Assembled::Call(call) if is_builtin(&call.name) => {
                        let output = execute_batch(ctx, vec![guarded]).await.remove(0).output;
                        (output_state(&output), output)
                    }
                    Assembled::Call(call) => {
                        self.execute_mcp(
                            call,
                            &op,
                            turn_id,
                            attached,
                            cancel,
                            &mut native_mcp_result,
                        )
                        .await
                    }
                    Assembled::Failed(_) => unreachable!("assembly failure rejected above"),
                }
            };
            // Failure here leaves started/unknown; never continue the batch.
            turn_log
                .input
                .push(if let Some(native) = native_mcp_result {
                    InputItem::McpFunctionCallOutput {
                        call_id: id.clone(),
                        output: native,
                    }
                } else if let Some(native) = native_read_result {
                    InputItem::ReadFunctionCallOutput {
                        call_id: id.clone(),
                        output: native,
                    }
                } else {
                    InputItem::FunctionCallOutput {
                        call_id: id.clone(),
                        output: output.clone(),
                    }
                });
            if name == "read" && state == "completed" && !cancel.load(Ordering::Acquire) {
                let instruction_generation = turn_log.display["config_generation"]
                    .as_u64()
                    .ok_or(RuntimeError::Storage)?;
                if instruction_generation != self.generation_id() {
                    return Err(RuntimeError::StaleGeneration {
                        want: instruction_generation,
                        got: self.generation_id(),
                    });
                }
                let roots = self
                    .workspace
                    .read()
                    .expect("workspace lock")
                    .instruction_roots
                    .clone();
                let (revision, facts) = self.db.instruction_view(session)?;
                let mut desired = facts.into_iter().map(|f| f.source).collect::<Vec<_>>();
                let path = match unit {
                    Assembled::Call(call) => call.arguments["path"].as_str(),
                    _ => None,
                }
                .expect("validated read path");
                let admitted = crate::instructions::after_read(
                    &roots,
                    &self.files,
                    path,
                    read_directory,
                    instruction_generation,
                    &self.roots.data,
                    invocation_policy,
                    cancel,
                )
                .map_err(RuntimeError::InvalidArgs)?;
                for mut source in admitted {
                    if let Some(old) = desired.iter_mut().find(|s| s.path == source.path) {
                        // A canonical root alias may also be discovered by a
                        // nested read; retain its original root provenance.
                        if old.origin != crate::instructions::Origin::Nested {
                            source.origin = old.origin.clone();
                            source.root = old.root.clone();
                        }
                        *old = source;
                    } else if source.content.is_some() {
                        desired.push(source);
                    }
                }
                let index = turn_log.input.len();
                if turn_log.display["config_generation"].as_u64() != Some(self.generation_id()) {
                    return Err(RuntimeError::StaleGeneration {
                        want: turn_log.display["config_generation"].as_u64().unwrap_or(0),
                        got: self.generation_id(),
                    });
                }
                self.db.checkpoint_instructions(
                    turn_id,
                    turn_log,
                    revision,
                    &desired,
                    index,
                    Some((&op, state, &output)),
                )?;
            } else {
                self.db.tool_outcome_with_log_and_effects(
                    &op,
                    state,
                    &output,
                    turn_id,
                    &turn_log.to_json().to_string(),
                    patch_effects.as_ref(),
                )?;
            }
            emit_tool_finish_with_effects(
                tool_event,
                turn_id,
                &op,
                name,
                state,
                &output,
                patch_effects,
            );
            records.push(CallRecord {
                name: name.to_string(),
                state: state.to_string(),
                output: truncate(&output, REPORT_OUTPUT_CAP),
            });
            if state == "unknown" {
                break; // no later side effects in this provider batch
            }
        }
        Ok((records, projection_changed, permission_rejected))
    }

    /// Legacy patch deny wins: a protected patch never reaches the executor.
    fn guard_patch(&self, unit: Assembled) -> Assembled {
        let Assembled::Call(call) = &unit else {
            return unit;
        };
        if call.name != "apply_patch" {
            return unit;
        }
        let patch = call
            .arguments
            .get("patchText")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let violations = crate::dcp::check_patch_protected(patch, &self.protected.patterns);
        match violations {
            Ok(_) => unit,
            Err(error) => Assembled::Failed(CallFailure {
                id: call.id.clone(),
                error: format!("patch refused: {error}"),
            }),
        }
    }
}
