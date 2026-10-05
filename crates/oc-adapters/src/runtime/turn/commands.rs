//! Native command launches reuse exact turn admission and the existing Jobs owner.
use super::*;

pub(super) struct CommandAdmission {
    request: SubagentRequest,
    child: String,
    reservation: children::Reservation,
    catalog: SubagentCatalog,
}

fn failed(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::InvalidArgs(error.to_string())
}

impl Runtime<'_> {
    pub(in crate::runtime) fn command_child_receipt(
        &self,
        fence: &serde_json::Value,
        identity: &oc_core::queries::ChildJob,
    ) -> Result<bool, RuntimeError> {
        let parent_turn = fence["parent_turn"].as_str().ok_or(RuntimeError::Storage)?;
        let (_, log) = self.db.turn_result(parent_turn)?;
        let Some(log) = log else {
            return Ok(false);
        };
        let value: serde_json::Value =
            serde_json::from_str(&log).map_err(|_| RuntimeError::Storage)?;
        let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
        Ok(log.display["command"]["source"] == "native_command"
            && log.display["command"]["background"] == true
            && log.display["command"]["child_agent"] == identity.agent
            && log.display_parts.iter().any(|part| {
                part["tool"] == identity.operation && part["source"] == "native_command"
            }))
    }

    pub(super) async fn admit_command(
        &self,
        params: &TurnParams<'_>,
        lane: &TurnLane,
        attached: &McpGeneration,
        fresh: bool,
        command: &NativeCommand,
    ) -> Result<Option<CommandAdmission>, RuntimeError> {
        let Some(child) = &command.child else {
            return Ok(None);
        };
        let workspace = self.workspace.read().expect("workspace lock").clone();
        let mut catalog = workspace
            .subagents
            .clone()
            .ok_or_else(|| failed("command profiles unavailable"))?;
        let agent = catalog
            .agents
            .get_mut(&child.agent)
            .ok_or_else(|| failed("command agent unavailable"))?;
        // Donor commands may launch primary profiles. This private catalog is
        // passed only to the command runner; model tool eligibility is unchanged.
        agent.primary = false;
        let model = ResolvedModel {
            id: child.model_id.clone(),
            variant: child.variant.clone(),
        };
        let request = SubagentRequest {
            call_id: format!("native-command:{}", next_turn_id(&params.session, millis())),
            background: true,
            agent: child.agent.clone(),
            description: child.description.clone(),
            prompt: params.prompt.clone(),
            model: Some(model.stored(&params.catalog.provider)),
            session_id: None,
            context_message_ids: Vec::new(),
        };
        let runner = TurnSubagent {
            turn_id: String::new(),
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
            fresh_parent: fresh,
        };
        let (agent, _, _) = runner.resolve_request(&request).map_err(failed)?;
        let child_id = self.new_child_id(&params.session);
        let child_lane = self.child_lane(agent, lane, &request.call_id);
        let child_params = TurnParams {
            session: child_id.clone(),
            prompt: format!(
                "You are a subagent spawned by another session.\n{}",
                params.prompt
            ),
            invocation: None,
            catalog: params.catalog,
            model_id: model.id.clone(),
            variant: model.variant.clone(),
            max_output: 0,
            provider: params.provider.clone(),
            cancel: params.cancel,
        };
        let published = self.current.read().expect("generation lock").clone();
        let base = models::select_model(params.catalog, &model.id).map_err(failed)?;
        let fallback = published
            .config
            .providers
            .get(&params.catalog.provider)
            .map(|p| p.options.native_fallback_limits)
            .unwrap_or_default();
        let budget = models::budget(&base, 0, fallback);
        // Exact same schema-inclusive preflight as execution; no turn/session,
        // provider dispatch, tool intent or selection transaction in probe mode.
        let proof = Box::pin(self.run_turn_admitted(
            child_params,
            &child_lane,
            attached,
            Some(None),
            &mut |_, _| {},
            &mut |_, _| {},
            &mut |_, _| {},
            &mut |_| {},
            &mut |_, _| {},
            &budget,
            None,
            None,
            true,
        ))
        .await?;
        if !matches!(proof, TurnExecution::Admitted) {
            return Err(RuntimeError::Storage);
        }
        let call = crate::tools::ToolCall {
            id: request.call_id.clone(),
            name: SUBAGENT_TOOL.into(),
            arguments: serde_json::json!({"agent":request.agent,"description":request.description,"prompt":request.prompt,"model":request.model,"background":true}),
        };
        let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
            .with_root(&self.roots.project)
            .with_mcp(&attached.entries);
        let ctx = ToolContext {
            files: &self.files,
            shell: &self.shell,
            parent_env: &self.parent_env,
            webfetch_auth: self.webfetch_auth.clone(),
            webfetch_allow_private: self.webfetch_allow_private,
            policy: &policy,
            subagent: Some(&runner),
            snapshot: &workspace.skills,
            cancel: params.cancel,
            roots: Some(self.roots.clone()),
        };
        self.admit_tool(
            &params.session,
            &request.call_id,
            &request.call_id,
            &call,
            &ctx,
            &policy,
            params.cancel,
            lane.agent_id.clone(),
            lane.agent_digest.clone(),
            false,
        )
        .await
        .map_err(failed)?;
        if params.cancel.load(Ordering::Acquire) {
            return Err(failed("command cancelled before admission"));
        }
        let reservation = self
            .child_jobs
            .reserve(&params.session, &child_id)
            .map_err(failed)?;
        Ok(Some(CommandAdmission {
            request,
            child: child_id,
            reservation,
            catalog,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn launch_command<'x>(
        &'x self,
        params: &'x TurnParams<'x>,
        lane: &'x TurnLane,
        attached: &'x McpGeneration,
        log: &'x mut TurnLog,
        admission: CommandAdmission,
        events: &'x mut (dyn FnMut(&str, &ToolCallEvent) + Send),
        published: &'x PublishedGeneration,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TurnReport, RuntimeError>> + Send + 'x>,
    > {
        Box::pin(async move {
            let turn = log.turn_id.clone();
            let request = admission.request;
            let op = format!("{turn}:command");
            let input = serde_json::json!({"agent":request.agent,"description":request.description,"prompt":request.prompt,"model":request.model,"background":true}).to_string();
            let index = log.input.len();
            log.input.push(InputItem::ProviderOutput(serde_json::json!({"type":"function_call","id":request.call_id,"call_id":request.call_id,"name":SUBAGENT_TOOL,"arguments":input,"status":"completed"})));
            log.call_occurrences.insert(
                index,
                self.db.next_call_occurrence(&turn, &request.call_id)?,
            );
            log.display_parts.push(
                serde_json::json!({"tool":op,"call_input_index":index,"source":"native_command"}),
            );
            self.db.record_turn_tool_intent(
                &op,
                &params.session,
                &turn,
                SUBAGENT_TOOL,
                &input,
                &log.to_json().to_string(),
            )?;
            events(
                &turn,
                &ToolCallEvent::Started {
                    op: op.clone(),
                    name: SUBAGENT_TOOL.into(),
                    input,
                    dcp_topic: None,
                },
            );
            let runner = TurnSubagent {
                turn_id: turn.clone(),
                runtime: self,
                parent_session: params.session.clone(),
                parent_model_id: params.model_id.clone(),
                parent_variant: params.variant.clone(),
                catalog: params.catalog,
                provider: &params.provider,
                cancel: params.cancel,
                attached,
                subagents: admission.catalog,
                parent_lane: lane,
                fresh_parent: false,
            };
            let outcome = runner
                .spawn_reserved(request.clone(), admission.child, admission.reservation)
                .await;
            let (state, output) = match outcome {
                Ok(SubagentOutcome::Running {
                    session_id,
                    operation,
                    generation,
                    delivery_id,
                }) => (
                    "running",
                    serde_json::json!({
                        "sessionID": session_id,
                        "status": "running",
                        "jobGeneration": operation,
                        "sourceGeneration": generation,
                        "deliveryID": delivery_id,
                    })
                    .to_string(),
                ),
                Ok(_) => return Err(RuntimeError::Storage),
                Err(error) => ("failed", format!("error: {error}")),
            };
            log.input.push(InputItem::FunctionCallOutput {
                call_id: request.call_id,
                output: output.clone(),
            });
            record_tool_finish(
                &self.db,
                events,
                &op,
                SUBAGENT_TOOL,
                state,
                &output,
                &turn,
                log,
            )?;
            self.commit_turn(
                log,
                turn,
                &params.session,
                if state == "running" {
                    TurnStatus::Completed
                } else {
                    TurnStatus::Failed
                },
                String::new(),
                0,
                0,
                None,
                None,
                vec![CallRecord {
                    name: SUBAGENT_TOOL.into(),
                    state: state.into(),
                    output,
                }],
                None,
                published,
            )
        })
    }
}
