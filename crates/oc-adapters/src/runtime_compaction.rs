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
    boundary: String,
    native: Option<(String, String)>,
    removed: BTreeMap<String, u64>,
    selection: Vec<serde_json::Value>,
    identities: BTreeMap<crate::storage::DcpCallKey, crate::storage::DcpCallIdentity>,
    marks: crate::storage::DcpToolProjection,
    tail: Vec<String>,
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
    Irreducible,
    ProtectionAdmissionRefused,
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
fn serialized_size<T: serde::Serialize>(value: &T) -> Result<u64, RuntimeError> {
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
            log.input_for(&model.id, &model.provider)
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
                        .input_for(&model.id, &model.provider),
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
            .map(serde_json::Value::to_string)
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

impl Runtime<'_> {
    fn protection_admission_receipt(
        &self,
        snapshot: &CompactionSnapshot,
        irreducible: bool,
    ) -> Result<(), RuntimeError> {
        let value = serde_json::to_value(ProgressReceipt {
            operation: snapshot.id.clone(),
            outcome: if irreducible {
                ProgressOutcome::Irreducible
            } else {
                ProgressOutcome::ProtectionAdmissionRefused
            },
            source_selected_json_bytes: 0,
            replacement_selected_json_bytes: 0,
            omitted_whole_prefix: false,
            prior_selection_json_bytes: 0,
            replacement_selection_json_bytes: 0,
        })
        .map_err(|_| RuntimeError::Storage)?;
        self.db
            .record_compaction_progress(&snapshot.session, &value)?;
        Ok(())
    }
    pub(super) fn compaction_estimate(
        &self,
        session: &str,
        scope: &str,
        history: &[InputItem],
        tail: &[InputItem],
        fallback: u64,
    ) -> Result<Option<u64>, RuntimeError> {
        let input: Vec<_> = history.iter().chain(tail).cloned().collect();
        if let Some(anchor) = self.db.usage_anchor(session)?
            && let Some(measured) = anchor.estimate(scope, &input)
        {
            return Ok(Some(measured));
        }
        // A completed checkpoint must get one primary response before another
        // automatic compaction. Opaque replacement sizes cannot be inferred.
        Ok(self
            .db
            .session_checkpoint(session)?
            .is_none()
            .then_some(fallback))
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
                        .map_err(|_| RuntimeError::Provider)?)
        {
            return Err(RuntimeError::InvalidArgs(
                "provider checkpoint route unsupported; restore an earlier conversation point"
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
        self.deliver_compaction_hot(
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
        if let Err(error) = self.admit_provider_variant(catalog, model, variant, provider) {
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
                    let config = self.dcp_config.read().expect("dcp config").clone();
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
                                            &config,
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
                    if self
                        .db
                        .commit_closed_turn_segment_selected(
                            log,
                            &segment,
                            &hot,
                            Some((&snapshot, &prepared.removed)),
                            &prepared.selection,
                            Some((&projection, &progress, &retire)),
                        )
                        .is_ok()
                    {
                        **log = hot;
                    } else {
                        snapshot.state = CompactionState::Failed;
                        snapshot.error =
                            Some("closed turn checkpoint commit failed; context preserved".into());
                    }
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
                            &prepared.receipt(&snapshot, None)?,
                            &prepared.identities.keys().cloned().collect::<Vec<_>>(),
                        )),
                    )
                    .is_err()
                {
                    snapshot.state = CompactionState::Failed;
                    snapshot.error = Some("checkpoint commit failed".into());
                }
            }
            Ok(None) => {
                snapshot.state = CompactionState::Failed;
                snapshot.error = Some("no conversation to compact".into());
            }
            _ => {
                snapshot.state = CompactionState::Failed;
                snapshot
                    .error
                    .get_or_insert_with(|| "summary request failed or invalid response".into());
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
        session: &str,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
        cancel: &AtomicBool,
        snapshot: &mut CompactionSnapshot,
        current: Option<(&crate::tools::TurnLog, [usize; 7])>,
    ) -> Result<Option<PreparedCheckpoint>, RuntimeError> {
        self.validate_checkpoint_route(session, &catalog.provider, model, provider)?;
        snapshot.model = Some(oc_core::queries::ModelRef {
            provider: catalog.provider.clone(),
            id: model.to_string(),
            variant: variant.map(str::to_owned),
        });
        let config = self.current.read().expect("generation lock").config.clone();
        let base = models::select_model(catalog, model)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let selection = models::select_variant(&base, variant)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let fallback = config
            .providers
            .get(&catalog.provider)
            .map(|p| p.options.native_fallback_limits)
            .unwrap_or_default();
        let budget = models::budget(&selection, 32_000, fallback);
        let content_budget = usize::try_from(budget.input.saturating_mul(4))
            .unwrap_or(usize::MAX)
            .saturating_sub(PROMPT.len() + 4096)
            .min(ACTIVE_CONTEXT_BYTES_CAP);
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
            let dcp = self.dcp_config.read().expect("dcp config").clone();
            let retain_selection = dcp.protect_user_messages
                || dcp.protect_tags
                || !dcp.protected_file_patterns.is_empty()
                || !dcp.protected_tools.is_empty()
                || !dcp.dedup_protected_tools.is_empty()
                || !dcp.purge_protected_tools.is_empty()
                || self.db.has_active_shell_jobs(session)?;
            let floor = self
                .db
                .compaction_content_floor(session, after, cutoff, content_budget)?;
            if floor > after && retain_selection {
                self.protection_admission_receipt(
                    snapshot,
                    dcp.protect_user_messages
                        && self.db.protected_user_over_budget(
                            session,
                            after,
                            floor,
                            content_budget,
                        )?,
                )?;
                return Err(RuntimeError::InvalidArgs(
                    "protected past input cannot be admitted without recalling omitted content"
                        .into(),
                ));
            }
            omitted_prefix |= floor > after;
            let mut context = self.active_projection_selected(
                session,
                floor,
                cutoff,
                true,
                content_budget,
                retain_selection,
            )?;
            if retain_selection {
                self.renew_compaction_selection(session, &mut context)?;
            }
            let prior = context
                .projected
                .iter()
                .position(|row| row.0 == anchor)
                .map_or(context.projected.as_slice(), |index| {
                    &context.projected[..index]
                });
            let mut history = self.wire_history(
                session,
                prior,
                &context.blocks,
                model,
                &catalog.provider,
                Some("__compaction__"),
                context.after_seq,
            )?;
            let logs =
                self.projected_wire_logs(session, context.after_seq, prior, &context.blocks)?;
            retained_selection =
                self.select_history_facts(&logs, &self.dcp_config.read().expect("dcp config"))?;
            retained_selection.extend(self.selected_message_facts(
                session,
                prior,
                &context.blocks,
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
            history.extend(prefix.input_for(model, &catalog.provider));
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
            let Some((cutoff, boundary)) =
                self.db
                    .compaction_boundary(session, after, config.compaction.keep_tokens)?
            else {
                return Ok(None);
            };
            let dcp = self.dcp_config.read().expect("dcp config").clone();
            let retain_selection = dcp.protect_user_messages
                || dcp.protect_tags
                || !dcp.protected_file_patterns.is_empty()
                || !dcp.protected_tools.is_empty()
                || !dcp.dedup_protected_tools.is_empty()
                || !dcp.purge_protected_tools.is_empty()
                || self.db.has_active_shell_jobs(session)?;
            let floor = self
                .db
                .compaction_content_floor(session, after, cutoff, content_budget)?;
            if floor > after && retain_selection {
                self.protection_admission_receipt(
                    snapshot,
                    dcp.protect_user_messages
                        && self.db.protected_user_over_budget(
                            session,
                            after,
                            floor,
                            content_budget,
                        )?,
                )?;
                snapshot.error = Some(
                    "irreducible explicitly protected compaction input exceeds admitted budget"
                        .into(),
                );
                return Err(RuntimeError::InvalidArgs(snapshot.error.clone().unwrap()));
            }
            omitted_prefix |= floor > after;
            let mut context = self.active_projection_selected(
                session,
                floor,
                cutoff,
                !recover_native,
                content_budget,
                retain_selection,
            )?;
            if retain_selection {
                self.renew_compaction_selection(session, &mut context)?;
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
            let mut history = self.wire_history(
                session,
                &prefix,
                &context.blocks,
                model,
                &catalog.provider,
                Some("__compaction__"),
                context.after_seq,
            )?;
            let offset_history = if recover_native {
                let source_logs =
                    self.projected_wire_logs(session, context.after_seq, &prefix, &context.blocks)?;
                retained_selection = self.select_history_facts(&source_logs, &dcp)?;
                let after = self
                    .db
                    .session_checkpoint(session)?
                    .map(|(seq, _)| seq)
                    .unwrap_or(0);
                self.wire_history(
                    session,
                    &prefix,
                    &context.blocks,
                    model,
                    &catalog.provider,
                    Some("__compaction__"),
                    after,
                )?
            } else {
                let source_logs =
                    self.projected_wire_logs(session, context.after_seq, &prefix, &context.blocks)?;
                retained_selection = self.select_history_facts(&source_logs, &dcp)?;
                history.clone()
            };
            retained_selection.extend(self.selected_message_facts(
                session,
                &prefix,
                &context.blocks,
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
        let source_bytes = serialized_size(&history)?;
        models::admit_budget(
            &selection,
            estimate_tokens(&serde_json::to_string(&history).map_err(|_| RuntimeError::Storage)?),
            &budget,
        )
        .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
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
            let route = crate::compaction::route_identity(&catalog.provider, model, provider)
                .map_err(|_| RuntimeError::Provider)?;
            let native = loop {
                match strategy
                    .compact(&route, &history, cancel, &mut || {
                        self.db
                            .generation_dispatch(session, &snapshot.id, "native_compaction")
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
        'summary: loop {
            let generation = loop {
                snapshot.summary.clear();
                match crate::provider::stream_input_counted(
                    provider,
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
                        self.db
                            .generation_dispatch(session, &operation, "compaction")
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
