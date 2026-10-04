//! Scoped foreground delegation groups. Admission and publication stay ordered;
//! distinct child sessions progress together, duplicate continuations chain.

use super::*;
use futures_util::FutureExt as _;
use futures_util::future::{BoxFuture, Shared, join_all};

impl Runtime<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_foreground_children(
        &self,
        turn: &str,
        session: &str,
        units: &[Assembled],
        offset: usize,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        cancel: &AtomicBool,
        round: u32,
        log: &mut TurnLog,
        positioned: &mut Vec<(usize, usize)>,
        positions: &[Option<usize>],
        identities: &[Option<oc_core::tool_stream::ToolStreamIdentity>],
        events: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
        request_tools: &[ToolDef],
        mut rejected: bool,
    ) -> Result<(Vec<CallRecord>, bool), RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let secrets = super::super::mcp::mcp_redactions(&published.config, &self.parent_env);
        let mut admitted = Vec::new();
        let mut admissions = Vec::new();
        // Nothing executes until all ordered admissions/intents have succeeded.
        for (i, unit) in units.iter().enumerate() {
            let Assembled::Call(call) = unit else {
                unreachable!("child group")
            };
            let op = format!("{turn}-r{round}-c{}-{}", offset + i, call.id);
            let input = call.arguments.to_string();
            let admission = if rejected || cancel.load(Ordering::Acquire) {
                Err(AdmissionFailure::Cancelled)
            } else if let Err(error) = self.validate_tool_call(call) {
                Err(AdmissionFailure::Invalid(error.to_string()))
            } else {
                self.admit_tool(
                    session,
                    turn,
                    &op,
                    call,
                    ctx,
                    policy,
                    cancel,
                    log.display["agent"].as_str().map(str::to_owned),
                    log.agent_digest.clone(),
                    request_tools.iter().any(|tool| tool.name == COMPRESS_TOOL),
                )
                .await
            };
            if matches!(admission, Err(AdmissionFailure::Required)) {
                return Err(RuntimeError::ApprovalRequired {
                    tool: SUBAGENT_TOOL.into(),
                });
            }
            rejected |= matches!(admission, Err(AdmissionFailure::Rejected(None)));
            admissions.push((call, op, input, admission));
        }
        for (i, (call, op, input, admission)) in admissions.into_iter().enumerate() {
            let refusal = match &admission {
                Err(AdmissionFailure::Rejected(feedback)) => Some((
                    "denied",
                    serde_json::json!({"status":"permission_rejected","feedback":feedback})
                        .to_string(),
                )),
                Err(AdmissionFailure::Cancelled) => Some(("cancelled", "error: cancelled".into())),
                Err(error) => Some(("failed", format!("error: {error}"))),
                Ok(permitted) => permitted
                    .check_call(call)
                    .err()
                    .map(|error| ("failed", format!("error: {error}"))),
            };
            let position = positions[i]
                .and_then(|index| {
                    positioned
                        .iter()
                        .filter(|(other, _)| *other > index)
                        .map(|(_, part)| *part)
                        .min()
                })
                .unwrap_or(log.display_parts.len());
            log.display_parts.insert(
                position,
                serde_json::json!({"tool":op,"span":log.spans.last().map(|span|span.id.as_str())}),
            );
            if let Some(index) = log.input.iter().rposition(|item| matches!(item,InputItem::ProviderOutput(value) if value["type"]=="function_call" && value["call_id"].as_str()==Some(call.id.as_str()))) {
                if let std::collections::btree_map::Entry::Vacant(entry) = log.call_occurrences.entry(index) {
                    entry.insert(self.db.next_call_occurrence(turn, &call.id)?);
                }
                log.display_parts[position]["call_input_index"] = index.into();
            }
            for (_, part) in positioned.iter_mut() {
                if *part >= position {
                    *part += 1;
                }
            }
            if let Some(index) = positions[i] {
                positioned.push((index, position));
            }
            if refusal.is_none() {
                self.db.record_turn_tool_intent(
                    &op,
                    session,
                    turn,
                    SUBAGENT_TOOL,
                    &input,
                    &log.to_json().to_string(),
                )?;
            }
            if let Some(identity) = &identities[i] {
                events(
                    turn,
                    &ToolCallEvent::ArgumentStream(oc_core::tool_stream::ToolStreamEvent::Linked {
                        identity: identity.clone(),
                        op: op.clone(),
                    }),
                );
            }
            if refusal.is_none() {
                events(
                    turn,
                    &ToolCallEvent::Started {
                        op: op.clone(),
                        name: SUBAGENT_TOOL.into(),
                        input,
                        dcp_topic: None,
                    },
                );
            }
            admitted.push((call, op, admission.ok(), refusal));
        }
        // Futures borrow this batch and cannot outlive its owner. Shared
        // predecessor joins serialize ONLY the explicitly named same child.
        let mut previous: BTreeMap<String, Shared<BoxFuture<'_, String>>> = BTreeMap::new();
        let mut work = Vec::new();
        for (call, _, permitted, refusal) in &admitted {
            let key = call.arguments["sessionID"].as_str().map(str::to_owned);
            let predecessor = key.as_ref().and_then(|key| previous.get(key)).cloned();
            let future = async move {
                if let Some(predecessor) = predecessor {
                    predecessor.await;
                }
                if let Some((_, output)) = refusal {
                    return output.clone();
                }
                let context = ToolContext {
                    policy: permitted.as_ref().expect("admitted child"),
                    files: ctx.files,
                    shell: ctx.shell,
                    parent_env: ctx.parent_env,
                    webfetch_auth: ctx.webfetch_auth.clone(),
                    webfetch_allow_private: ctx.webfetch_allow_private,
                    subagent: ctx.subagent,
                    snapshot: ctx.snapshot,
                    cancel: ctx.cancel,
                    roots: ctx.roots.clone(),
                };
                execute_batch(&context, vec![Assembled::Call((*call).clone())])
                    .await
                    .remove(0)
                    .output
            }
            .boxed()
            .shared();
            if let Some(key) = key {
                previous.insert(key, future.clone());
            }
            work.push(future);
        }
        // join_all never short-circuits on child failure/cancellation. Every
        // admitted provider/tool leaf has settled before parent publication.
        let outputs = join_all(work).await;
        drop(previous);
        let mut records = Vec::new();
        for ((call, op, _, refusal), output) in admitted.into_iter().zip(outputs) {
            let mut state = refusal
                .as_ref()
                .map_or_else(|| output_state(&output), |(state, _)| *state);
            let source = crate::config::mcp::safe_source_id(
                published
                    .config
                    .provenance
                    .get("tool_output")
                    .map_or("native defaults", String::as_str),
            );
            let preparation = crate::tools::output::Context {
                db: self.db,
                operation: &op,
                session,
                location: &self.location,
                generation: published.id,
                source: &source,
                limits: published.config.tool_output,
                secrets: secrets.clone(),
            };
            let prepared = preparation.prepare_envelope(output, false, None);
            if prepared.logging_failed {
                self.db.record_output_execution(&op, state)?;
                state = "failed";
            }
            log.input.push(InputItem::FunctionCallOutput {
                call_id: call.id.clone(),
                output: prepared.text.clone(),
            });
            if refusal.is_some() {
                self.db.record_turn_tool_refusal(
                    &op,
                    session,
                    turn,
                    SUBAGENT_TOOL,
                    &call.arguments.to_string(),
                    state,
                    &prepared.text,
                    &log.to_json().to_string(),
                )?;
                emit_tool_finish(events, turn, &op, SUBAGENT_TOOL, state, &prepared.text);
            } else {
                record_tool_finish(
                    self.db,
                    events,
                    &op,
                    SUBAGENT_TOOL,
                    state,
                    &prepared.text,
                    turn,
                    log,
                )?;
            }
            records.push(CallRecord {
                name: SUBAGENT_TOOL.into(),
                state: state.into(),
                output: truncate(&prepared.text, REPORT_OUTPUT_CAP),
            });
        }
        Ok((records, rejected))
    }
}
