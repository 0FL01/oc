//! Safe-boundary session summarization. DCP projection is input, never overwritten.
use super::*;
use oc_core::compaction::{CompactionReason, CompactionSnapshot, CompactionState};
use oc_core::core_app::CoreEvent;
#[cfg(test)]
#[path = "runtime_compaction_tests.rs"]
mod tests;

pub(super) struct Work {
    snapshot: CompactionSnapshot,
    cancel: Arc<AtomicBool>,
}
struct PreparedCheckpoint {
    binding: oc_core::queries::WireProvenance,
    boundary: String,
    native: Option<(String, String)>,
    removed: BTreeMap<String, u64>,
    selection: Vec<serde_json::Value>,
    identities: BTreeMap<crate::storage::DcpCallKey, crate::storage::DcpCallIdentity>,
    marks: crate::storage::DcpToolProjection,
    tail: Vec<crate::storage::WireLog>,
    source_bytes: u64,
    omitted_prefix: bool,
    prior_selection_bytes: u64,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ProgressOutcome {
    MeasuredShrink,
    AdmittedRenewal,
    NoGain,
}
#[derive(serde::Serialize)]
struct ProgressReceipt {
    operation: String,
    outcome: ProgressOutcome,
    source_selected_json_bytes: u64,
    replacement_selected_json_bytes: u64,
    omitted_whole_prefix: bool,
    prior_selection_json_bytes: u64,
    replacement_selection_json_bytes: u64,
}
pub(super) fn serialized_size<T: serde::Serialize>(value: &T) -> Result<u64, RuntimeError> {
    struct Counter(u64);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len() as u64);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(|_| RuntimeError::Storage)?;
    Ok(counter.0)
}
impl PreparedCheckpoint {
    fn receipt(
        &self,
        snapshot: &CompactionSnapshot,
        current: Option<&crate::tools::TurnLog>,
    ) -> Result<serde_json::Value, RuntimeError> {
        let model = snapshot.model.as_ref().ok_or(RuntimeError::Storage)?;
        let mut input = if let Some(log) = current {
            log.input_for_bound(
                &model.id,
                &model.provider,
                Some(&self.binding),
                snapshot.profile.as_ref(),
            )
        } else {
            vec![InputItem::message(InputRole::Developer, &snapshot.summary)]
        };
        for fact in &self.selection {
            if let Some(message) = fact.get("protected_message") {
                input.push(InputItem::message(
                    if message["role"] == "user" {
                        InputRole::User
                    } else {
                        InputRole::Assistant
                    },
                    message["text"].as_str().ok_or(RuntimeError::Storage)?,
                ));
            } else if let Some(facts) = fact.get("legacy_protected") {
                let mut text = String::new();
                crate::dcp::append_legacy_protection(
                    &mut text,
                    facts.as_array().ok_or(RuntimeError::Storage)?,
                )
                .map_err(|_| RuntimeError::Storage)?;
                input.push(InputItem::message(InputRole::Developer, text));
            } else {
                input.extend(
                    crate::tools::TurnLog::from_json(fact)
                        .map_err(|_| RuntimeError::Storage)?
                        .input_for_bound(
                            &model.id,
                            &model.provider,
                            Some(&self.binding),
                            snapshot.profile.as_ref(),
                        ),
                );
            }
        }
        if let Some((_, opaque)) = &self.native {
            input.push(InputItem::ProviderOutput(
                serde_json::from_str(opaque).map_err(|_| RuntimeError::Storage)?,
            ));
        }
        let replacement = serialized_size(&input)?;
        let selection_bytes = serialized_size(&self.selection)?;
        serde_json::to_value(ProgressReceipt {
            operation: snapshot.id.clone(),
            outcome: if replacement < self.source_bytes {
                ProgressOutcome::MeasuredShrink
            } else if self.omitted_prefix || selection_bytes < self.prior_selection_bytes {
                ProgressOutcome::AdmittedRenewal
            } else {
                ProgressOutcome::NoGain
            },
            source_selected_json_bytes: self.source_bytes,
            replacement_selected_json_bytes: replacement,
            omitted_whole_prefix: self.omitted_prefix,
            prior_selection_json_bytes: self.prior_selection_bytes,
            replacement_selection_json_bytes: selection_bytes,
        })
        .map_err(|_| RuntimeError::Storage)
    }
    fn projection(
        &self,
        current: Option<&crate::tools::TurnLog>,
    ) -> Result<crate::storage::DcpToolProjection, RuntimeError> {
        let logs = self
            .selection
            .iter()
            .filter(|v| v.get("protected_message").is_none() && v.get("legacy_protected").is_none())
            .map(|value| crate::storage::WireLog::from(value.to_string()))
            .chain(self.tail.iter().cloned())
            .collect::<Vec<_>>();
        Ok(remap_projection(
            &self.identities,
            &self.marks,
            super::context::dcp_call_identities(&logs, current)?,
        ))
    }
}
fn remap_projection(
    before: &BTreeMap<crate::storage::DcpCallKey, crate::storage::DcpCallIdentity>,
    marks: &crate::storage::DcpToolProjection,
    after: BTreeMap<crate::storage::DcpCallKey, crate::storage::DcpCallIdentity>,
) -> crate::storage::DcpToolProjection {
    let after = after
        .into_iter()
        .map(|(key, id)| (id, key))
        .collect::<BTreeMap<_, _>>();
    let mut result = crate::storage::DcpToolProjection::default();
    for (old, new) in [
        (&marks.hidden, &mut result.hidden),
        (&marks.purged, &mut result.purged),
    ] {
        for key in old {
            if let Some(identity) = before.get(key)
                && let Some(key) = after.get(identity)
            {
                new.insert(key.clone());
            }
        }
    }
    result
}

// Release on every exit, including publication errors and a dropped future.
struct DeliveryOwnership<'a> {
    work: &'a Mutex<BTreeMap<String, Work>>,
    session: &'a str,
}
impl Drop for DeliveryOwnership<'_> {
    fn drop(&mut self) {
        self.work.lock().expect("compactions").remove(self.session);
    }
}

const PROMPT: &str = "Summarize only what the user and assistant said and did. Preserve user requirements, decisions, unresolved questions, exact paths and consequential work state. Do not repeat system instructions, AGENTS.md or environment setup. Do not continue the task or call tools. Return only concise Markdown with applicable sections: ## Objective, ## Requirements, ## Decisions, ## Work State, ## Next Move, ## Relevant Files, ## Important Context. Update a previous checkpoint into one consolidated summary; newer history takes precedence.";

pub(super) fn usage_scope<'a>(
    legacy: &str,
    sources: impl Iterator<Item = &'a crate::instructions::Source>,
) -> String {
    let sources = sources
        .filter(|source| source.content.is_some())
        .map(|source| {
            (
                &source.path,
                &source.root,
                &source.origin,
                &source.digest,
                &source.content,
            )
        })
        .collect::<Vec<_>>();
    // Fact event/revision/placement are delivery metadata, not changed source
    // semantics. Preserve source order and bind the actual admitted content.
    crate::compaction::fingerprint(&("usage-v2", legacy, sources))
}

pub(super) fn usage_causal<'a>(
    input: impl Iterator<Item = &'a InputItem>,
    facts: &[InputItem],
) -> Vec<&'a InputItem> {
    // Match only whole typed Fact renderings supplied by the runtime. User text
    // resembling an instruction header remains part of the causal prefix.
    input
        .filter(|item| !facts.contains(item) && !super::context::is_step_marker(item))
        .collect()
}

pub(super) fn usage_tokens(input: &[&InputItem]) -> Result<u64, RuntimeError> {
    Ok(estimate_input_tokens_from_bytes(
        serialized_size(&input)?,
        input.iter().copied(),
    ))
}

pub(super) fn usage_estimate(
    anchor: Option<&crate::compaction::UsageAnchor>,
    scopes: (&str, &str),
    input: &[&InputItem],
    legacy_input: &[&InputItem],
    noncausal_tokens: u64,
) -> Result<Option<u64>, RuntimeError> {
    let Some(anchor) = anchor else {
        return Ok(None);
    };
    let (scope, input, previous) = match anchor.noncausal_tokens {
        Some(previous) => (scopes.0, input, previous),
        // Validate v0 literally, with the producer's pre-reconciliation scope
        // and original item indices. Unknown old overhead receives no credit;
        // only a genuine subsequent primary usage writes a new-format anchor.
        None => (scopes.1, legacy_input, 0),
    };
    let Some(suffix) = anchor.suffix(scope, input) else {
        return Ok(None);
    };
    Ok(Some(
        anchor
            .tokens
            .saturating_add(usage_tokens(suffix)?)
            .saturating_add(noncausal_tokens.saturating_sub(previous)),
    ))
}

impl Runtime<'_> {
    fn protection_admission_receipt(
        &self,
        snapshot: &CompactionSnapshot,
    ) -> Result<(), RuntimeError> {
        // The omitted source was not inspected. Report the enforced bound,
        // not fabricated zero sizes or an unproved mandatory-payload overflow.
        let value = serde_json::json!({
            "operation": snapshot.id,
            "outcome": "protection_admission_refused",
            "source_limit_json_bytes": ACTIVE_CONTEXT_BYTES_CAP,
        });
        self.db
            .record_compaction_progress(&snapshot.session, &value)?;
        Ok(())
    }
    pub(super) fn compaction_estimate(
        &self,
        session: &str,
        measured: Option<u64>,
        fallback: u64,
        admitted_input: u64,
    ) -> Result<Option<u64>, RuntimeError> {
        if measured.is_some() {
            return Ok(measured);
        }
        // Avoid soft-trigger loops before a checkpoint gets a measurement, but
        // never strand a request that still exceeds its actual input budget.
        if fallback > admitted_input {
            return Ok(Some(fallback));
        }
        Ok(self
            .db
            .session_checkpoint(session)?
            .is_none()
            .then_some(fallback))
    }
    /// Keep whole exchanges by their captured provider projection, not public
    /// message text (which excludes most tool/reasoning payloads).
    #[allow(clippy::too_many_arguments)]
    fn priced_compaction_boundary(
        &self,
        published: &ExecutionGeneration,
        session: &str,
        after: i64,
        model: &str,
        provider: &str,
        binding: &oc_core::queries::WireProvenance,
        profile: &oc_core::queries::RequestProfile,
    ) -> Result<Option<(i64, String)>, RuntimeError> {
        let minimum = self.db.compaction_boundary(session, after)?;
        if minimum.is_none() {
            return Ok(None);
        }
        let context = match self.active_projection_range(
            session,
            after,
            i64::MAX,
            false,
            ACTIVE_CONTEXT_BYTES_CAP,
        ) {
            Ok(context) => context,
            // Manual escape must remain reachable even when the full history
            // exceeds the host bound. Source acquisition below stays bounded.
            Err(RuntimeError::ContextOverflow { .. }) => return Ok(minimum),
            Err(error) => return Err(error),
        };
        let mut boundaries = Vec::new();
        for (index, row) in context.projected.iter().enumerate().skip(1) {
            if row.1 != "user" && !context.blocks.iter().any(|block| block.id == row.0) {
                continue;
            }
            let previous = &context.projected[index - 1].0;
            let boundary = context
                .blocks
                .iter()
                .find(|block| block.id == *previous)
                .map_or(previous.as_str(), |block| block.end_msg.as_str());
            if let Some(seq) = self.db.message_seq(session, boundary)? {
                boundaries.push((index, seq, boundary.to_owned()));
            }
        }
        let Some((_, seq, id)) = boundaries.last() else {
            return Ok(minimum);
        };
        let mut chosen = (*seq, id.clone());
        for (index, seq, id) in boundaries.into_iter().rev() {
            let mut tail = match self.wire_history_view(
                session,
                &context.projected[index..],
                &context.blocks,
                model,
                provider,
                profile.agent_digest.as_deref(),
                after,
                Some(binding),
                false,
                &published.dcp_config,
            ) {
                Ok(tail) => tail,
                Err(RuntimeError::ContextOverflow { .. }) => break,
                Err(error) => return Err(error),
            };
            let marks = self.db.dcp_tool_projection_for_input(session, &tail)?;
            apply_dcp_projection(&mut tail, &marks);
            if usage_tokens(&tail.iter().collect::<Vec<_>>())?
                > published.config.compaction.keep_tokens
            {
                break;
            }
            chosen = (seq, id);
        }
        Ok(Some(chosen))
    }
    pub(super) fn compaction_active(&self) -> bool {
        self.compactions
            .lock()
            .expect("compactions")
            .values()
            .any(|w| w.snapshot.state == CompactionState::Running)
    }
    /// Register a real provider mechanism explicitly; absent capabilities use summary requests.
    pub fn register_native_compaction(
        &self,
        strategy: Arc<dyn crate::compaction::NativeCompaction>,
    ) {
        *self.native_compaction.write().expect("native compaction") = Some(strategy);
    }
    pub(super) fn validate_checkpoint_route(
        &self,
        session: &str,
        provider_id: &str,
        model: &str,
        provider: &ResponsesConfig,
        profile: &oc_core::queries::RequestProfile,
    ) -> Result<(), RuntimeError> {
        if let Some((_, _, Some(route), Some(_))) = self.db.checkpoint_record(session)?
            && let Some(origin) = self.db.checkpoint_model(session)?
            && origin.provider == provider_id
            && origin.id != model
            && route
                == crate::compaction::route_identity(provider_id, &origin.id, provider)
                    .map_err(|_| RuntimeError::Provider)?
        {
            // Same verified credential/endpoint scope, different model: preserve
            // public tail while withholding this native checkpoint entirely.
            return Ok(());
        }
        if let Some((_, _, Some(route), Some(_))) = self.db.checkpoint_record(session)?
            && (self
                .native_compaction
                .read()
                .expect("native compaction")
                .is_none()
                || route
                    != crate::compaction::route_identity(provider_id, model, provider)
                        .map_err(|_| RuntimeError::Provider)?
                || !self.db.checkpoint_profile(session)?.as_ref().map_or_else(
                    || {
                        profile.agent_digest.is_none()
                            || profile.agent_digest.as_deref()
                                == Some(
                                    crate::defs::agent_digest(&crate::defs::builtin_build())
                                        .as_str(),
                                )
                    },
                    |origin| origin == profile,
                ))
        {
            return Err(RuntimeError::InvalidArgs(
                "provider checkpoint route/profile unsupported; restore an earlier conversation point"
                    .into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn set_compaction_events(&self, events: &tokio::sync::broadcast::Sender<CoreEvent>) {
        *self.compaction_events.lock().expect("compaction events") = Some(events.clone());
    }
    fn publish_compaction(&self, snapshot: &CompactionSnapshot) -> Result<(), RuntimeError> {
        self.db.save_compaction(snapshot)?;
        if let Some(events) = self
            .compaction_events
            .lock()
            .expect("compaction events")
            .as_ref()
        {
            let _ = events.send(CoreEvent::Compaction(snapshot.clone()));
        }
        Ok(())
    }
    pub(crate) fn queue_compaction(
        &self,
        session: &str,
        reason: CompactionReason,
    ) -> Result<CompactionSnapshot, RuntimeError> {
        self.open_session(session)?;
        let mut work = self.compactions.lock().expect("compactions");
        if let Some(work) = work.get(session) {
            return Ok(work.snapshot.clone());
        }
        if work.len() >= 16 {
            return Err(RuntimeError::TurnActive);
        }
        let snapshot = CompactionSnapshot {
            model: None,
            profile: None,
            anchor: self.db.compaction_anchor(session)?,
            id: next_turn_id("compaction", millis()),
            session: session.into(),
            reason,
            state: CompactionState::Queued,
            summary: String::new(),
            usage: None,
            provider_native: false,
            error: None,
        };
        self.publish_compaction(&snapshot)?;
        work.insert(
            session.into(),
            Work {
                snapshot: snapshot.clone(),
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(snapshot)
    }
    pub(crate) fn pending_compaction(&self) -> Option<String> {
        self.compactions
            .lock()
            .expect("compactions")
            .iter()
            .find(|(_, w)| w.snapshot.state == CompactionState::Queued)
            .map(|(id, _)| id.clone())
    }
    pub(crate) fn refuse_compaction(&self, session: &str) -> Result<(), RuntimeError> {
        self.refuse_compaction_with_error(session, "selected model unavailable".into())
    }
    fn refuse_compaction_with_error(
        &self,
        session: &str,
        error: String,
    ) -> Result<(), RuntimeError> {
        if let Some(mut work) = self
            .compactions
            .lock()
            .expect("compactions")
            .remove(session)
        {
            work.snapshot.state = CompactionState::Failed;
            work.snapshot.error = Some(error);
            self.publish_compaction(&work.snapshot)?;
        }
        Ok(())
    }
    pub(crate) fn cancel_compaction(&self, session: &str) -> Result<(), RuntimeError> {
        let mut work = self.compactions.lock().expect("compactions");
        if let Some(w) = work.get_mut(session) {
            w.cancel.store(true, Ordering::Relaxed);
            if w.snapshot.state == CompactionState::Queued {
                let mut snapshot = w.snapshot.clone();
                snapshot.state = CompactionState::Cancelled;
                work.remove(session);
                self.publish_compaction(&snapshot)?;
            }
        }
        Ok(())
    }
    pub(crate) fn cancel_all_compactions(&self) {
        let sessions: Vec<_> = self
            .compactions
            .lock()
            .expect("compactions")
            .keys()
            .cloned()
            .collect();
        for session in sessions {
            let _ = self.cancel_compaction(&session);
        }
    }
    pub(crate) async fn deliver_compaction(
        &self,
        session: &str,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
    ) -> Result<bool, RuntimeError> {
        self.deliver_compaction_bound(session, catalog, model, variant, provider, None)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn deliver_compaction_bound(
        &self,
        session: &str,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
        caller_cancel: Option<&AtomicBool>,
    ) -> Result<bool, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        self.deliver_compaction_hot(
            &published,
            session,
            catalog,
            model,
            variant,
            provider,
            caller_cancel,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn deliver_compaction_hot(
        &self,
        published: &ExecutionGeneration,
        session: &str,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
        caller_cancel: Option<&AtomicBool>,
        mut current: Option<(&mut crate::tools::TurnLog, [usize; 7], i64)>,
    ) -> Result<bool, RuntimeError> {
        if !self
            .compactions
            .lock()
            .expect("compactions")
            .get(session)
            .is_some_and(|work| work.snapshot.state == CompactionState::Queued)
        {
            return Ok(false);
        }
        if let Err(error) =
            self.admit_provider_variant_in(published, catalog, model, variant, provider)
        {
            self.refuse_compaction_with_error(session, error.to_string())?;
            return Err(error);
        }
        let (mut snapshot, cancel) = {
            let mut work = self.compactions.lock().expect("compactions");
            let Some(w) = work.get_mut(session) else {
                return Ok(false);
            };
            if w.snapshot.state != CompactionState::Queued {
                return Ok(false);
            }
            w.snapshot.state = CompactionState::Running;
            (w.snapshot.clone(), w.cancel.clone())
        };
        let _ownership = DeliveryOwnership {
            work: &self.compactions,
            session,
        };
        match self.db.compaction_anchor(session) {
            Ok(anchor) => snapshot.anchor = anchor,
            Err(error) => {
                self.recover_publication_failure(&mut snapshot);
                return Err(error.into());
            }
        }
        // A steered manual operation owns its independent lifetime. Automatic
        // and overflow work belongs to the calling execution, including admission.
        let caller_cancel = caller_cancel.filter(|_| snapshot.reason != CompactionReason::Manual);
        let cancelled = || {
            cancel.load(Ordering::Relaxed)
                || caller_cancel.is_some_and(|c| c.load(Ordering::Relaxed))
        };
        if cancelled() {
            snapshot.state = CompactionState::Cancelled;
            if let Err(error) = self.publish_compaction(&snapshot) {
                self.recover_publication_failure(&mut snapshot);
                return Err(error);
            }
            return Ok(false);
        }
        if let Err(error) = self.publish_compaction(&snapshot) {
            self.recover_publication_failure(&mut snapshot);
            return Err(error);
        }
        if let Some((log, _, _)) = &current {
            self.db
                .checkpoint_turn(&log.turn_id, &log.to_json().to_string())?;
        }
        let result = {
            let summary = self.summarize_session(
                published,
                session,
                catalog,
                model,
                variant,
                provider,
                &cancel,
                &mut snapshot,
                current.as_ref().map(|(log, counts, _)| (&**log, *counts)),
            );
            tokio::pin!(summary);
            tokio::select! {
                biased;
                () = async {
                    loop {
                        if caller_cancel.is_some_and(|c| c.load(Ordering::Relaxed)) { break; }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                }, if caller_cancel.is_some() => {
                    cancel.store(true, Ordering::Relaxed);
                    Err(RuntimeError::Provider)
                }
                result = &mut summary => result,
            }
        };
        match result {
            _ if cancelled() => {
                snapshot.state = CompactionState::Cancelled;
            }
            Ok(Some(prepared)) => {
                snapshot.state = CompactionState::Completed;
                if let Some((log, counts, notice_seq)) = current.as_mut() {
                    let config = &published.dcp_config;
                    let live_calls = self.db.live_shell_call_ids(&log.turn_id)?;
                    let working = log
                        .current_working_checkpoint(&snapshot.summary, *counts, |group| {
                            group.iter().any(|item| {
                                if let InputItem::ProviderOutput(v) = item
                                    && v["type"] == "function_call"
                                {
                                    let name = v["name"].as_str().unwrap_or_default();
                                    live_calls
                                        .iter()
                                        .any(|id| v["call_id"].as_str() == Some(id.as_str()))
                                        || crate::dcp_auto::tool_is_protected(
                                            &config.protected_tools,
                                            name,
                                        )
                                        || crate::dcp_auto::tool_is_protected(
                                            &config.dedup_protected_tools,
                                            name,
                                        )
                                        || crate::dcp_auto::tool_is_protected(
                                            &config.purge_protected_tools,
                                            name,
                                        )
                                        || super::context::dcp_call_has_protected_path(
                                            name,
                                            v["arguments"].as_str().unwrap_or_default(),
                                            config,
                                        )
                                } else if config.protect_tags {
                                    let text = serde_json::to_string(item).unwrap_or_default();
                                    oc_core::context_plan::extract_protect_tags(&text)
                                        .next()
                                        .is_some()
                                } else {
                                    false
                                }
                            })
                        })
                        .map_err(|_| RuntimeError::Storage)?;
                    let (segment, hot) = log
                        .prepare_closed_segment(*counts, working, *notice_seq)
                        .map_err(|_| RuntimeError::Storage)?;
                    let projection = prepared.projection(Some(&hot))?;
                    let progress = prepared.receipt(&snapshot, Some(&hot))?;
                    let retire = prepared.identities.keys().cloned().collect::<Vec<_>>();
                    if snapshot.reason != CompactionReason::Manual
                        && progress["outcome"] == "no_gain"
                    {
                        snapshot.state = CompactionState::Failed;
                        snapshot.error = Some("compaction no_gain; context preserved".into());
                    } else if self
                        .db
                        .commit_closed_turn_segment_selected(
                            log,
                            &segment,
                            &hot,
                            Some((&snapshot, &prepared.removed)),
                            &prepared.selection,
                            Some((&projection, Some(&progress), &retire)),
                            &[],
                        )
                        .is_ok()
                    {
                        **log = hot;
                    } else {
                        snapshot.state = CompactionState::Failed;
                        snapshot.error =
                            Some("closed turn checkpoint commit failed; context preserved".into());
                    }
                } else {
                    let progress = prepared.receipt(&snapshot, None)?;
                    if snapshot.reason != CompactionReason::Manual
                        && progress["outcome"] == "no_gain"
                    {
                        snapshot.state = CompactionState::Failed;
                        snapshot.error = Some("compaction no_gain; context preserved".into());
                    } else if self
                        .db
                        .commit_checkpoint_selected(
                            &snapshot,
                            &prepared.boundary,
                            prepared
                                .native
                                .as_ref()
                                .map(|(route, opaque)| (route.as_str(), opaque.as_str())),
                            &prepared.removed,
                            &prepared.selection,
                            Some((
                                &prepared.projection(None)?,
                                &progress,
                                &prepared.identities.keys().cloned().collect::<Vec<_>>(),
                            )),
                        )
                        .is_err()
                    {
                        snapshot.state = CompactionState::Failed;
                        snapshot.error = Some("checkpoint commit failed".into());
                    }
                }
            }
            Ok(None) => {
                snapshot.state = CompactionState::Failed;
                snapshot.error = Some("no conversation to compact".into());
            }
            Err(error) => {
                snapshot.state = CompactionState::Failed;
                snapshot.error.get_or_insert_with(|| error.to_string());
            }
        }
        if let Err(error) = self.publish_compaction(&snapshot) {
            self.recover_publication_failure(&mut snapshot);
            return Err(error);
        }
        Ok(snapshot.state == CompactionState::Completed)
    }
    fn recover_publication_failure(&self, snapshot: &mut CompactionSnapshot) {
        let committed = snapshot.state == CompactionState::Completed;
        snapshot.state = CompactionState::Failed;
        snapshot.error = Some(
            if committed {
                "checkpoint installed; lifecycle publication failed"
            } else {
                "lifecycle publication failed; context preserved"
            }
            .into(),
        );
        // Bypass the failing publication path. If storage itself is unavailable,
        // its last queued/running row is recoverable by normal reopen recovery.
        let _ = self.db.save_compaction(snapshot);
        if let Some(events) = self
            .compaction_events
            .lock()
            .expect("compaction events")
            .as_ref()
        {
            let _ = events.send(CoreEvent::Compaction(snapshot.clone()));
        }
    }
    #[allow(clippy::too_many_arguments)]
    async fn summarize_session(
        &self,
        published: &ExecutionGeneration,
        session: &str,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
        cancel: &AtomicBool,
        snapshot: &mut CompactionSnapshot,
        current: Option<(&crate::tools::TurnLog, [usize; 7])>,
    ) -> Result<Option<PreparedCheckpoint>, RuntimeError> {
        let base = models::select_model(catalog, model)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let mut selection = models::select_variant(&base, variant)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let prepared_provider = crate::auth::prepare_request(
            provider,
            &self.db,
            &self.parent_env,
            &catalog.provider,
            model,
            variant,
            cancel,
        )
        .await?;
        crate::composition::openai_catalog::prepare_selection(&mut selection, &prepared_provider);
        let provider = prepared_provider.for_selection(model, variant);
        let binding = provider
            .provenance(&catalog.provider, model)
            .map_err(|_| RuntimeError::Provider)?;
        let profile = if let Some(launch) = &self.child_launch {
            self.child_request_lane(published, launch)?
                .request_profile()
        } else {
            self.primary_lane(published).request_profile()
        };
        self.validate_checkpoint_route(session, &catalog.provider, model, provider, &profile)?;
        snapshot.profile = Some(profile.clone());
        snapshot.model = Some(oc_core::queries::ModelRef {
            provider: catalog.provider.clone(),
            id: model.to_string(),
            variant: variant.map(str::to_owned),
        });
        let config = &published.config;
        let fallback = config
            .providers
            .get(&catalog.provider)
            .map(|p| p.options.native_fallback_limits)
            .unwrap_or_default();
        let budget = models::budget(&selection, 32_000, fallback);
        let recover_native = snapshot.reason == CompactionReason::Overflow
            && self
                .db
                .checkpoint_record(session)?
                .is_some_and(|(_, _, _, opaque)| opaque.is_some());
        let mut retained_selection;
        let original_floor = self.db.prune_bound(session)?.map_or(0, |(_, seq)| seq).max(
            self.db
                .session_checkpoint(session)?
                .map_or(0, |(seq, _)| seq),
        );
        let prior_selection_bytes = self.db.selected_payload_bytes(session, original_floor)?;
        let identities = self.db.wire_call_metadata(session, original_floor)?;
        let source_marks = self.db.dcp_tool_projection_for_keys(
            session,
            &identities.keys().cloned().collect::<Vec<_>>(),
        )?;
        let mut tail_logs = Vec::new();
        let mut omitted_prefix = false;
        let (boundary, history, removed) = if let Some((log, counts)) = current {
            let mut prefix = log.clone();
            prefix.input.truncate(counts[0]);
            prefix.requests.truncate(counts[1]);
            prefix.spans.truncate(counts[2]);
            prefix.instruction_references.truncate(counts[5]);
            // Consolidate the previously chosen checkpoint and bounded past
            // window with current work, rather than pinning either forever.
            let anchor = log.user_message.as_deref().ok_or(RuntimeError::Storage)?;
            let cutoff = self
                .db
                .message_seq(session, anchor)?
                .ok_or(RuntimeError::Storage)?
                .saturating_sub(1);
            let after = self.db.prune_bound(session)?.map_or(0, |(_, seq)| seq).max(
                self.db
                    .session_checkpoint(session)?
                    .map_or(0, |(seq, _)| seq),
            );
            let dcp = &published.dcp_config;
            let retain_selection = dcp.protect_user_messages
                || dcp.protect_tags
                || dcp.turn_protection
                || !dcp.protected_file_patterns.is_empty()
                || !dcp.protected_tools.is_empty()
                || !dcp.dedup_protected_tools.is_empty()
                || !dcp.purge_protected_tools.is_empty()
                || self.db.has_active_shell_jobs(session)?;
            // Archive/source acquisition is a host-resource decision, not a
            // model-token estimate. Inspect the full eligible source before
            // selecting protections; final provider admission remains below.
            let source_budget = ACTIVE_CONTEXT_BYTES_CAP;
            let floor = self
                .db
                .compaction_content_floor(session, after, cutoff, source_budget)?;
            if floor > after && retain_selection {
                self.protection_admission_receipt(snapshot)?;
                return Err(RuntimeError::InvalidArgs(
                    "protected active history could not be inspected within source limits".into(),
                ));
            }
            omitted_prefix |= floor > after;
            let mut context = self.active_projection_selected(
                session,
                floor,
                cutoff,
                true,
                source_budget,
                retain_selection,
            )?;
            if retain_selection {
                self.renew_compaction_selection(session, &mut context, dcp)?;
            }
            let prior = context
                .projected
                .iter()
                .position(|row| row.0 == anchor)
                .map_or(context.projected.as_slice(), |index| {
                    &context.projected[..index]
                });
            let mut history = self.wire_history_view(
                session,
                prior,
                &context.blocks,
                model,
                &catalog.provider,
                profile.agent_digest.as_deref(),
                context.after_seq,
                Some(&binding),
                false,
                dcp,
            )?;
            let logs =
                self.projected_wire_logs(session, context.after_seq, prior, &context.blocks)?;
            retained_selection = self.select_history_facts(&logs, &published.dcp_config)?;
            retained_selection.extend(self.selected_message_facts(
                session,
                prior,
                &context.blocks,
                dcp,
            )?);
            let mut removed = BTreeMap::new();
            for item in &history {
                if let InputItem::ProviderOutput(v) = item
                    && v["type"] == "function_call"
                    && let Some(id) = v["call_id"].as_str()
                {
                    *removed.entry(id.to_owned()).or_default() += 1;
                }
            }
            let marks = remap_projection(
                &identities,
                &source_marks,
                super::context::dcp_call_identities(&logs, None)?,
            );
            apply_dcp_projection(&mut history, &marks);
            history.extend(prefix.input_for_bound(
                model,
                &catalog.provider,
                Some(&binding),
                Some(&profile),
            ));
            (
                log.user_message.clone().ok_or(RuntimeError::Storage)?,
                history,
                removed,
            )
        } else {
            let after =
                self.db
                    .prune_bound(session)?
                    .map_or(0, |(_, seq)| seq)
                    .max(if recover_native {
                        0
                    } else {
                        self.db
                            .session_checkpoint(session)?
                            .map_or(0, |(seq, _)| seq)
                    });
            let Some((cutoff, boundary)) = self.priced_compaction_boundary(
                published,
                session,
                after,
                model,
                &catalog.provider,
                &binding,
                &profile,
            )?
            else {
                return Ok(None);
            };
            let dcp = &published.dcp_config;
            let retain_selection = dcp.protect_user_messages
                || dcp.protect_tags
                || dcp.turn_protection
                || !dcp.protected_file_patterns.is_empty()
                || !dcp.protected_tools.is_empty()
                || !dcp.dedup_protected_tools.is_empty()
                || !dcp.purge_protected_tools.is_empty()
                || self.db.has_active_shell_jobs(session)?;
            let source_budget = ACTIVE_CONTEXT_BYTES_CAP;
            let floor = self
                .db
                .compaction_content_floor(session, after, cutoff, source_budget)?;
            if floor > after && retain_selection {
                self.protection_admission_receipt(snapshot)?;
                snapshot.error = Some(
                    "protected active history could not be inspected within source limits".into(),
                );
                return Err(RuntimeError::InvalidArgs(snapshot.error.clone().unwrap()));
            }
            omitted_prefix |= floor > after;
            let mut context = self.active_projection_selected(
                session,
                floor,
                cutoff,
                !recover_native,
                source_budget,
                retain_selection,
            )?;
            if retain_selection {
                self.renew_compaction_selection(session, &mut context, dcp)?;
            }
            let mut prefix = Vec::new();
            for row in context.projected {
                let seq = self.db.message_seq(session, &row.0)?;
                let block_seq = context
                    .blocks
                    .iter()
                    .find(|b| b.id == row.0)
                    .and_then(|b| b.members.last())
                    .map(|id| self.db.message_seq(session, id))
                    .transpose()?
                    .flatten();
                if row.0.starts_with("session-checkpoint")
                    || seq.or(block_seq).is_some_and(|seq| seq <= cutoff)
                {
                    prefix.push(row);
                }
            }
            let mut history = self.wire_history_view(
                session,
                &prefix,
                &context.blocks,
                model,
                &catalog.provider,
                profile.agent_digest.as_deref(),
                context.after_seq,
                Some(&binding),
                false,
                dcp,
            )?;
            let offset_history = if recover_native {
                let source_logs =
                    self.projected_wire_logs(session, context.after_seq, &prefix, &context.blocks)?;
                retained_selection = self.select_history_facts(&source_logs, dcp)?;
                let after = self
                    .db
                    .session_checkpoint(session)?
                    .map(|(seq, _)| seq)
                    .unwrap_or(0);
                self.wire_history_view(
                    session,
                    &prefix,
                    &context.blocks,
                    model,
                    &catalog.provider,
                    profile.agent_digest.as_deref(),
                    after,
                    Some(&binding),
                    false,
                    dcp,
                )?
            } else {
                let source_logs =
                    self.projected_wire_logs(session, context.after_seq, &prefix, &context.blocks)?;
                retained_selection = self.select_history_facts(&source_logs, dcp)?;
                history.clone()
            };
            retained_selection.extend(self.selected_message_facts(
                session,
                &prefix,
                &context.blocks,
                dcp,
            )?);
            let mut removed = BTreeMap::<String, u64>::new();
            for item in offset_history {
                if let InputItem::ProviderOutput(v) = item
                    && v["type"] == "function_call"
                    && let Some(id) = v["call_id"].as_str()
                {
                    *removed.entry(id.into()).or_default() += 1;
                }
            }
            let source_logs =
                self.projected_wire_logs(session, context.after_seq, &prefix, &context.blocks)?;
            let marks = remap_projection(
                &identities,
                &source_marks,
                super::context::dcp_call_identities(&source_logs, None)?,
            );
            let tail = self.active_projection_range(
                session,
                cutoff,
                i64::MAX,
                false,
                ACTIVE_CONTEXT_BYTES_CAP,
            )?;
            tail_logs = self.projected_wire_logs(session, cutoff, &tail.projected, &tail.blocks)?;
            apply_dcp_projection(&mut history, &marks);
            (boundary, history, removed)
        };
        let selection_bytes = serialized_size(&retained_selection)?;
        if selection_bytes > ACTIVE_CONTEXT_BYTES_CAP as u64 {
            return Err(RuntimeError::ContextOverflow {
                bytes: selection_bytes,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        let source_bytes = serialized_size(&history)?;
        if cancel.load(Ordering::Relaxed) {
            return Err(RuntimeError::Provider);
        }
        let strategy = self
            .native_compaction
            .read()
            .expect("native compaction")
            .clone();
        let mut retry_policy = retry::RetryPolicy::default();
        // Known overflow must use the durable transcript, never repeat native overflow.
        if current.is_none()
            && snapshot.reason != CompactionReason::Overflow
            && let Some(strategy) = strategy
        {
            models::admit_budget(
                &selection,
                estimate_tokens(
                    &serde_json::to_string(&history).map_err(|_| RuntimeError::Storage)?,
                ),
                &budget,
            )
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
            let route = crate::compaction::route_identity(&catalog.provider, model, provider)
                .map_err(|_| RuntimeError::Provider)?;
            let native = loop {
                let aggregate_dispatch = retry_policy.aggregate();
                match strategy
                    .compact(&route, &history, cancel, &mut || {
                        if aggregate_dispatch {
                            self.db.generation_retry_dispatch(
                                session,
                                &snapshot.id,
                                "native_compaction",
                                &snapshot.id,
                            )
                        } else {
                            self.db
                                .generation_dispatch(session, &snapshot.id, "native_compaction")
                        }
                        .map_err(|_| crate::provider::ProviderError::DispatchRefused)
                    })
                    .await
                {
                    Ok(native) => break native,
                    Err(error) => {
                        if let Some(decision) = retry_policy.decide(&error, millis())
                            && retry::wait(&decision, cancel).await
                        {
                            continue;
                        }
                        snapshot.error = Some(error.to_string());
                        return Err(RuntimeError::Provider);
                    }
                }
            };
            if let Some(native) = native {
                if native.route != route
                    || native.replacement["type"] != "compaction"
                    || native.replacement["encrypted_content"]
                        .as_str()
                        .is_none_or(str::is_empty)
                {
                    return Err(RuntimeError::Provider);
                }
                snapshot.provider_native = true;
                snapshot.summary.clear();
                snapshot.usage = native.usage;
                return Ok(Some(PreparedCheckpoint {
                    binding,
                    boundary,
                    native: Some((route, native.replacement.to_string())),
                    removed,
                    selection: retained_selection,
                    identities,
                    marks: source_marks,
                    tail: tail_logs,
                    source_bytes,
                    omitted_prefix,
                    prior_selection_bytes,
                }));
            }
        }
        // The transcript stays unprivileged data. Binary native results must be
        // actual Responses content, not base64 inside quoted JSON text. Carry
        // each such result with its original call; no function tools are offered.
        let mut transcript = history.clone();
        let mut media_pairs = Vec::new();
        for (index, item) in history.iter().enumerate() {
            let media = match item {
                InputItem::McpFunctionCallOutput { call_id, output } if output.has_media() => {
                    Some((call_id, output.display()))
                }
                InputItem::ReadFunctionCallOutput { call_id, output } => {
                    Some((call_id, output.display()))
                }
                _ => None,
            };
            if let Some((call_id, display)) = media {
                let call = history[..index].iter().rev().find(|i| {
                    matches!(i, InputItem::ProviderOutput(v) if v["type"] == "function_call" && v["call_id"].as_str() == Some(call_id.as_str()))
                }).ok_or(RuntimeError::Storage)?;
                media_pairs.extend([call.clone(), item.clone()]);
                transcript[index] = InputItem::FunctionCallOutput {
                    call_id: call_id.clone(),
                    output: display.into(),
                };
            }
        }
        // Opaque continuation is meaningful only to its original provider, not
        // as ciphertext quoted to a text summarizer. Keep public reasoning and
        // complete tool pairs; RAW, source pricing and native input are intact.
        let transcript = transcript.into_iter().filter_map(|item| {
            if matches!(&item, InputItem::ProviderOutput(value) if matches!(value["type"].as_str(), Some("reasoning" | "compaction"))) {
                crate::tools::model_history::neutral(&item)
            } else {
                Some(item)
            }
        }).collect::<Vec<_>>();
        let transcript = serde_json::to_string(&transcript).map_err(|_| RuntimeError::Storage)?;
        let mut input = vec![
            InputItem::message(InputRole::Developer, PROMPT),
            self.environment_input(),
            InputItem::message(InputRole::User, transcript),
        ];
        input.extend(media_pairs);
        models::admit_budget(
            &selection,
            estimate_tokens(&serde_json::to_string(&input).map_err(|_| RuntimeError::Storage)?),
            &budget,
        )
        .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let mut publication_failed = false;
        let operation = snapshot.id.clone();
        let mut corrected = false;
        let provider = provider.with_context(
            crate::provider::context::RequestContext::capture(
                &self.db,
                &self.roots.project,
                session,
            )?
            .for_operation(&operation),
        );
        'summary: loop {
            let generation = loop {
                let aggregate_dispatch = retry_policy.aggregate();
                snapshot.summary.clear();
                let prepared = crate::auth::prepare_request(
                    &provider,
                    &self.db,
                    &self.parent_env,
                    &catalog.provider,
                    model,
                    variant,
                    cancel,
                )
                .await?;
                let prepared = prepared.for_selection(model, variant);
                if prepared
                    .provenance(&catalog.provider, model)
                    .map_err(|_| RuntimeError::Provider)?
                    != binding
                {
                    // This summary's selected checkpoint/transcript was already
                    // projected for the original authority. Never retry it under
                    // an account/route switch or publish a falsely bound result.
                    snapshot.error = Some("summary request authority changed".into());
                    return Err(RuntimeError::Provider);
                }
                match crate::provider::stream_input_counted(
                    prepared,
                    model,
                    selection.variant.as_ref(),
                    &input,
                    &[],
                    budget.output,
                    cancel,
                    &mut |item| {
                        if let crate::provider::StreamItem::TextDelta(delta) = item
                            && snapshot.summary.len().saturating_add(delta.len()) <= 256 * 1024
                        {
                            snapshot.summary.push_str(delta);
                            if self.publish_compaction(snapshot).is_err() {
                                publication_failed = true;
                            }
                        }
                    },
                    &mut || async {
                        if aggregate_dispatch {
                            self.db.generation_retry_dispatch(
                                session,
                                &operation,
                                "compaction",
                                &operation,
                            )
                        } else {
                            self.db
                                .generation_dispatch(session, &operation, "compaction")
                        }
                        .map_err(|_| crate::provider::ProviderError::DispatchRefused)
                    },
                )
                .await
                {
                    Ok(generation) => break generation,
                    Err(error) => {
                        if !publication_failed
                            && let Some(decision) = retry_policy.decide(&error, millis())
                            && retry::wait(&decision, cancel).await
                        {
                            continue;
                        }
                        snapshot.error = Some(error.to_string());
                        return Err(if publication_failed {
                            RuntimeError::Storage
                        } else {
                            RuntimeError::Provider
                        });
                    }
                }
            };
            if publication_failed {
                return Err(RuntimeError::Storage);
            }
            if let Some(usage) = generation.compaction_usage {
                if let Some(total) = &mut snapshot.usage {
                    total.input_tokens += usage.input_tokens;
                    total.output_tokens += usage.output_tokens;
                    total.cache_read_tokens += usage.cache_read_tokens;
                    total.cache_write_tokens += usage.cache_write_tokens;
                    total.reasoning_tokens += usage.reasoning_tokens;
                } else {
                    snapshot.usage = Some(usage);
                }
            }
            if generation.finish == crate::provider::FinishReason::Length {
                return Err(RuntimeError::Provider);
            }
            if generation
                .output
                .iter()
                .any(|item| item["type"] == "function_call")
            {
                return Err(RuntimeError::Provider);
            }
            let canonical = generation
                .output
                .iter()
                .filter(|i| i["type"] == "message" && i["role"] == "assistant")
                .filter_map(|i| i["content"].as_array())
                .flatten()
                .filter(|p| p["type"] == "output_text")
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>();
            let summary = if canonical.is_empty() {
                generation.text
            } else {
                canonical.join("")
            };
            if summary.len() > 256 * 1024 {
                return Err(RuntimeError::Provider);
            }
            let has_section = summary.lines().any(|line| {
                [
                    "## Objective",
                    "## Requirements",
                    "## Decisions",
                    "## Work State",
                    "### Completed",
                    "### Active",
                    "### Blocked",
                    "## Next Move",
                    "## Relevant Files",
                    "## Important Context",
                ]
                .contains(&line.trim())
            });
            if !has_section && !corrected {
                corrected = true;
                input.push(InputItem::message(InputRole::User,"The previous response did not fill in the required summary template. Do not call tools. Return the summary as text using the exact section headings from the template."));
                models::admit_budget(
                    &selection,
                    estimate_tokens(
                        &serde_json::to_string(&input).map_err(|_| RuntimeError::Storage)?,
                    ),
                    &budget,
                )
                .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
                continue 'summary;
            }
            if !has_section {
                return Err(RuntimeError::Provider);
            }
            snapshot.summary = summary;
            return Ok(Some(PreparedCheckpoint {
                binding,
                boundary,
                native: None,
                removed,
                selection: retained_selection,
                identities,
                marks: source_marks,
                tail: tail_logs,
                source_bytes,
                omitted_prefix,
                prior_selection_bytes,
            }));
        }
    }
}
