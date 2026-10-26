//! Submission receipts, worker events and live tool/DCP projections.

use super::transcript::{card_row, footer_row};
use super::*;

/// Rebuild the adapter catalog view from the application snapshot so the
/// picker keeps its exact-id/variant validation over fresh discovery data.
fn catalog_from_snapshot(snapshot: &CatalogSnapshot) -> ModelCatalog {
    let mut models = BTreeMap::new();
    for entry in &snapshot.models {
        let variants: serde_json::Map<String, serde_json::Value> = entry
            .variants
            .iter()
            .map(|variant| {
                let mut value = serde_json::Map::new();
                if variant.disabled {
                    value.insert("disabled".to_string(), serde_json::json!(true));
                }
                if let Some(effort) = &variant.reasoning_effort {
                    value.insert("reasoningEffort".to_string(), serde_json::json!(effort));
                }
                let value = serde_json::Value::Object(value);
                (variant.name.clone(), value)
            })
            .collect();
        let mut model = serde_json::json!({
            "name": if entry.display_name.is_empty() { &entry.id } else { &entry.display_name },
            "provider_name": if entry.provider_name.is_empty() { &snapshot.provider } else { &entry.provider_name },
            "cost": entry.price.as_ref().map(|p|serde_json::json!({"input":p.input,"output":p.output})),
            "limit": { "context": entry.context_known.then_some(entry.context), "output": entry.output_known.then_some(entry.output) },
            "modalities": {"input":entry.capabilities.input, "output":entry.capabilities.output},
            "attachment": entry.capabilities.attachment,
            "reasoning": entry.capabilities.reasoning,
            "tool_call": entry.capabilities.tools,
            "partial_capabilities": {
                "image_input":entry.capabilities.partial.image_input,
                "pdf_input":entry.capabilities.partial.pdf_input,
                "reasoning":entry.capabilities.partial.reasoning,
            },
        });
        if entry.variants_known {
            model["variants"] = serde_json::Value::Object(variants);
        }
        models.insert(entry.id.clone(), model);
    }
    ModelCatalog {
        provider: snapshot.provider.clone(),
        models: models.into(),
    }
}

/// Scripted driver used by tests: holds one broadcast subscription like the
/// real binary event loop and applies worker events in order.
pub struct ScriptDriver {
    rx: tokio::sync::broadcast::Receiver<CoreEvent>,
}

impl ScriptDriver {
    /// Attach to the same handle the `TuiState` uses.
    pub fn attach(app: &CoreApp) -> Self {
        Self {
            rx: app.subscribe(),
        }
    }

    /// Pump worker events into `state` until idle (no active turn) or
    /// timeout. Returns terminal text or interrupt marker.
    pub async fn pump_until_idle(
        &mut self,
        state: &mut TuiState,
        timeout: Duration,
    ) -> PumpOutcome {
        loop {
            if !state.is_busy() && state.status != TuiStatus::Streaming {
                return PumpOutcome::Idle;
            }
            let event = tokio::time::timeout(timeout, self.rx.recv()).await;
            state.poll_submission();
            if let Ok(Ok(
                CoreEvent::TurnStarted { session, .. }
                | CoreEvent::TurnFinished { session, .. }
                | CoreEvent::TurnFailed { session, .. }
                | CoreEvent::TurnInterrupted { session, .. },
            )) = &event
                && state.attached_session() != Some(session)
                && state.has_subagent_cards()
                && let Some(parent) = state.attached_session().cloned()
                && let Ok(jobs) = state.app.child_jobs(parent).await
            {
                state.apply_child_jobs(jobs);
            }
            match event {
                Ok(Ok(CoreEvent::ParentContinuationRefused {
                    session,
                    diagnostic,
                })) => {
                    if state.attached_session() == Some(&session) {
                        state.push_transient_note(
                            &format!("Background continuation refused: {diagnostic}"),
                            NoteVariant::Error,
                        );
                    }
                }
                Ok(Ok(
                    CoreEvent::Projection { .. }
                    | CoreEvent::WorkspaceChanged { .. }
                    | CoreEvent::SessionViewed { .. }
                    | CoreEvent::ProjectionGap
                    | CoreEvent::OwnerDisconnected,
                )) => {}
                Ok(Ok(CoreEvent::SessionModelSelected { session, commit })) => {
                    state.apply_session_model_selected(&session, &commit)
                }
                Ok(Ok(CoreEvent::SelectionNoticeCommitted { session, message })) => {
                    if state.attached_session() == Some(&session)
                        && let Ok(exact) = state.app.history_message(session.clone(), message).await
                        && !exact.rows.is_empty()
                    {
                        if let Ok(recent) =
                            state.app.transcript_page(session, None, None, 100).await
                        {
                            state.refresh_child_notice_page(&recent);
                        }
                        state.refresh_child_notice_page(&exact);
                    }
                }
                Ok(Ok(CoreEvent::InboxChanged { session, items, .. })) => {
                    state.apply_inbox(&session, items)
                }
                Ok(Ok(CoreEvent::PermissionAsked(_)))
                | Ok(Ok(CoreEvent::PermissionResolved { .. })) => {
                    if let Ok(pending) = state.app.pending_approvals().await {
                        let requests = pending
                            .into_iter()
                            .filter(|r| {
                                state
                                    .user_shell_admission_session()
                                    .is_some_and(|s| s.0 == r.binding.session)
                                    || state
                                        .session
                                        .as_ref()
                                        .is_some_and(|s| s.0 == r.binding.session)
                            })
                            .map(|r| (r, state.parent_id.is_some()))
                            .collect();
                        state.approvals.reconcile(requests);
                        state.cancel_execute_pointer();
                    }
                }
                Ok(Ok(CoreEvent::QuestionAsked(_)))
                | Ok(Ok(CoreEvent::QuestionResolved { .. })) => {
                    if let Ok(pending) = state.app.pending_questions().await {
                        let visible = pending
                            .iter()
                            .filter(|r| {
                                state
                                    .session
                                    .as_ref()
                                    .is_some_and(|s| s.0 == r.binding.session)
                            })
                            .cloned()
                            .collect();
                        state.questions.reconcile(&pending, visible);
                    }
                }
                Ok(Ok(CoreEvent::Compaction(snapshot))) => state.apply_compaction(snapshot),
                Ok(Ok(CoreEvent::McpChanged(snapshot))) => state.apply_mcp_snapshot(snapshot),
                Ok(Ok(CoreEvent::ProviderChanged)) => {
                    let catalog = if let Some(session) = state.attached_session() {
                        state
                            .app
                            .session_selection(
                                session.clone(),
                                false,
                                oc_core::queries::SessionSelectionAction::Current,
                            )
                            .await
                    } else {
                        state
                            .app
                            .home_selection(oc_core::queries::SessionSelectionAction::Current)
                            .await
                    };
                    if let Ok(catalog) = catalog {
                        state.apply_catalog(catalog);
                    }
                }
                Ok(Ok(CoreEvent::SessionTitleUpdated { session, title })) => {
                    if state.attached_session() == Some(&session) {
                        state.session_title = Some(title);
                    }
                }
                Ok(Ok(CoreEvent::SessionMoved {
                    session,
                    location: Some(snapshot),
                    ..
                })) => {
                    if state.attached_session() == Some(&session) {
                        state.apply_catalog(snapshot.catalog);
                    }
                }
                Ok(Ok(CoreEvent::SessionMoved { .. })) => {}
                Ok(Ok(CoreEvent::ShellNotice(notice))) => {
                    if state.attached_session() == Some(&notice.session) {
                        if notice.user_requested {
                            if let Ok(page) = state
                                .app
                                .history_message(
                                    notice.session.clone(),
                                    oc_core::session::MessageId(notice.message_id),
                                )
                                .await
                                && page.rows.iter().any(|row| row.user_shell.is_some())
                            {
                                if !state.is_busy()
                                    && let Ok(recent) = state
                                        .app
                                        .history_page(notice.session, None, None, 100)
                                        .await
                                {
                                    state.refresh_completed_page(&recent);
                                }
                                state.refresh_user_shell_page(&page);
                            }
                        } else if let Ok(exact) = state
                            .app
                            .history_message(
                                notice.session.clone(),
                                oc_core::session::MessageId(notice.message_id),
                            )
                            .await
                            && exact.rows.iter().any(|row| {
                                row.shell_notice
                                    .as_ref()
                                    .is_some_and(|metadata| metadata.operation == notice.shell_id)
                            })
                        {
                            if state.has_model_shell_card(&notice.shell_id)
                                && let Ok(snapshot) = state
                                    .app
                                    .shell_snapshot(notice.session.clone(), notice.shell_id.clone())
                                    .await
                            {
                                state.apply_model_shell_snapshot(&snapshot);
                            }
                            if let Ok(recent) = state
                                .app
                                .history_page(notice.session, None, None, 100)
                                .await
                            {
                                if state.is_busy() {
                                    state.refresh_child_notice_page(&recent);
                                } else {
                                    state.refresh_completed_page(&recent);
                                }
                            }
                            state.refresh_child_notice_page(&exact);
                        }
                    }
                }
                Ok(Ok(CoreEvent::ChildNotice(notice))) => {
                    if state.attached_session() == Some(&notice.job.parent)
                        && let Some(message) = notice.job.message_id
                        && let Ok(page) = state
                            .app
                            .history_message(
                                notice.job.parent.clone(),
                                oc_core::session::MessageId(message),
                            )
                            .await
                    {
                        if page.rows.iter().any(|row| {
                            matches!(row.child, Some(oc_core::queries::ChildHistory::Notice(_)))
                        }) && state.has_subagent_cards()
                            && let Ok(jobs) = state.app.child_jobs(notice.job.parent.clone()).await
                        {
                            state.apply_child_jobs(jobs);
                        }
                        if page.rows.iter().any(|row| {
                            matches!(row.child, Some(oc_core::queries::ChildHistory::Notice(_)))
                        }) && let Ok(recent) = state
                            .app
                            .history_page(notice.job.parent, None, None, 100)
                            .await
                        {
                            if state.is_busy() {
                                state.refresh_child_notice_page(&recent);
                            } else {
                                state.refresh_completed_page(&recent);
                            }
                        }
                        state.refresh_child_notice_page(&page);
                    }
                }
                Ok(Ok(
                    event @ (CoreEvent::ShellChanged { .. } | CoreEvent::TerminalChanged { .. }),
                )) => {
                    let inventory_visible = match &event {
                        CoreEvent::ShellChanged { session } => {
                            state.attached_session() == Some(session) || state.shells_open()
                        }
                        CoreEvent::TerminalChanged { .. } => state.shells_open(),
                        _ => unreachable!("shell/terminal event"),
                    };
                    if inventory_visible && let Some(session) = state.attached_session().cloned() {
                        if let Ok(rows) = state.app.shell_jobs(session).await {
                            state.apply_shell_jobs(rows);
                        }
                        if let Some(job) = state.shell_viewer().cloned() {
                            match state
                                .app
                                .shell_snapshot(job.session.clone(), job.shell_id.clone())
                                .await
                            {
                                Ok(snapshot) => state.apply_shell_snapshot(snapshot),
                                Err(_) => state.apply_shell_read_failure(&job),
                            }
                        }
                    }
                }
                Err(_) => return PumpOutcome::Timeout,
                Ok(Err(_)) => return PumpOutcome::Closed,
                Ok(Ok(CoreEvent::TurnStarted {
                    turn, model_switch, ..
                })) => {
                    if let Some(notice) = model_switch {
                        state.apply_model_switch(&turn, &notice);
                    }
                }
                Ok(Ok(CoreEvent::TurnPresentation {
                    turn, projection, ..
                })) => state.apply_presentation(&turn, &projection),
                Ok(Ok(CoreEvent::RetryScheduled {
                    turn, span, retry, ..
                })) => state.apply_retry(&turn, &span, &retry),
                Ok(Ok(CoreEvent::TurnFailed {
                    turn,
                    error,
                    warnings,
                    service_warning_range,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_failed(&turn, &error);
                        state.apply_turn_warnings(warnings, service_warning_range);
                        return PumpOutcome::Closed;
                    }
                }
                Ok(Ok(CoreEvent::TextDelta { turn, delta, .. })) => {
                    state.apply_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ReasoningDelta { turn, delta, .. })) => {
                    state.apply_reasoning_delta(&turn, &delta);
                }
                Ok(Ok(CoreEvent::ReasoningItemEnded { turn, .. })) => {
                    state.apply_reasoning_item_ended(&turn);
                }
                Ok(Ok(CoreEvent::ToolArgumentStream { turn, event, .. })) => {
                    state.apply_tool_argument_stream(&turn, &event);
                }
                Ok(Ok(CoreEvent::ToolCallStarted {
                    output_presentation,
                    turn,
                    op,
                    name,
                    input,
                    dcp_topic,
                    ..
                })) => {
                    state.apply_tool_started_with_output_presentation(
                        &turn,
                        &op,
                        &name,
                        &input,
                        dcp_topic,
                        output_presentation,
                    );
                    if name == "subagent"
                        && state.active_turn() == Some(&turn)
                        && let Some(parent) = state.attached_session().cloned()
                        && let Ok(jobs) = state.app.child_jobs(parent).await
                    {
                        state.apply_child_jobs(jobs);
                    }
                }
                Ok(Ok(CoreEvent::ToolCallProgress {
                    turn,
                    op,
                    output_presentation,
                    ..
                })) => {
                    state.apply_tool_progress(&turn, &op, output_presentation);
                }
                Ok(Ok(CoreEvent::ToolCallFinished {
                    turn,
                    op,
                    name,
                    state: tool_state,
                    output,
                    output_bytes,
                    output_truncated,
                    output_presentation,
                    patch_effects,
                    dcp,
                    question,
                    ..
                })) => {
                    state.apply_tool_finished_with_output_presentation(
                        &turn,
                        &op,
                        &name,
                        &tool_state,
                        &output,
                        output_bytes,
                        output_truncated,
                        patch_effects,
                        dcp,
                        question,
                        output_presentation,
                    );
                    if name == "subagent"
                        && state.active_turn() == Some(&turn)
                        && let Some(parent) = state.attached_session().cloned()
                        && let Ok(jobs) = state.app.child_jobs(parent).await
                    {
                        state.apply_child_jobs(jobs);
                    }
                }
                Ok(Ok(CoreEvent::TurnContextUsage {
                    turn,
                    input_tokens,
                    output_tokens,
                    context_limit,
                    ..
                })) => {
                    state.apply_context_usage(&turn, input_tokens, output_tokens, context_limit);
                }
                Ok(Ok(CoreEvent::TurnUsage {
                    turn,
                    input_tokens,
                    output_tokens,
                    streamed_ms,
                    ..
                })) => {
                    state.apply_usage(&turn, input_tokens, output_tokens, streamed_ms);
                }
                Ok(Ok(CoreEvent::TurnFinished {
                    turn,
                    text,
                    duration_ms,
                    warnings,
                    service_warning_range,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_finished(&turn, &text, duration_ms);
                        state.apply_turn_warnings(warnings, service_warning_range);
                        return PumpOutcome::Finished(text);
                    }
                }
                Ok(Ok(CoreEvent::TurnInterrupted {
                    turn,
                    partial,
                    duration_ms,
                    ..
                })) => {
                    if Some(&turn) == state.active_turn.as_ref() {
                        state.apply_interrupted(&turn, &partial, duration_ms);
                        return PumpOutcome::Interrupted(partial);
                    }
                }
            }
        }
    }
}

/// Terminal pump result.
#[derive(Debug, PartialEq, Eq)]
pub enum PumpOutcome {
    /// Turn finished with full text.
    Finished(String),
    /// Turn interrupted with partial text.
    Interrupted(String),
    /// Already idle (no active turn).
    Idle,
    /// Timed out waiting for events.
    Timeout,
    /// Channel closed.
    Closed,
}

impl TuiState {
    /// Reactivating this exact retained owner invalidates filesystem/read
    /// projections, not its captured admission. No receipt is moved to another
    /// route and no owner effect is repeated.
    pub fn resume_retained_view(&mut self) {
        self.resume_transcript_projection();
        let previous = self.generation;
        self.invalidate_file_suggestions();
        if let Some(pending) = self.pending.as_mut()
            && pending.generation == previous
        {
            pending.generation = self.generation;
        }
        self.retain_user_shell_generation(previous);
    }

    /// Active-turn only: parked views receive this through the same scoped router.
    pub fn apply_retry(
        &mut self,
        turn: &WorkerTurnId,
        span: &str,
        retry: &oc_core::queries::RetryFact,
    ) {
        if self.active_turn.as_ref() != Some(turn) {
            return;
        }
        if self
            .live_span
            .as_ref()
            .is_some_and(|(owner, current)| owner == turn && current != span)
        {
            return;
        }
        if self
            .live_retry_seen
            .as_ref()
            .is_some_and(|(owner, id, attempt)| {
                owner == turn
                    && id == span
                    && (retry.attempt < *attempt
                        || (retry.attempt == *attempt && self.live_retry.is_none()))
            })
        {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.retry_due =
            (retry.at > now).then(|| Instant::now() + Duration::from_millis(retry.at - now));
        self.live_retry = Some((span.into(), retry.clone()));
        self.live_retry_seen = Some((turn.clone(), span.into(), retry.attempt));
        self.invalidate_transcript();
    }

    pub fn retry_notice(&self) -> Option<String> {
        self.active_turn.as_ref()?;
        self.live_retry.as_ref().map(|(_, retry)| {
            format!(
                "{} · attempt {} · {}",
                if self.retry_due.is_some() {
                    "Retry scheduled"
                } else {
                    "Retry due"
                },
                retry.attempt,
                retry.safe_error
            )
        })
    }
    /// Replace from the owner's conversation-filtered replay projection.
    pub fn apply_compaction_history(
        &mut self,
        snapshots: Vec<oc_core::compaction::CompactionSnapshot>,
    ) {
        self.invalidate_transcript();
        self.compactions = snapshots
            .into_iter()
            // The owner's bounded journal query returns newest-first.
            .rev()
            .filter(|s| self.session.as_ref().is_some_and(|id| id.0 == s.session))
            .map(Into::into)
            .collect();
        self.sync_compaction_clock();
    }

    pub fn apply_compaction(&mut self, snapshot: oc_core::compaction::CompactionSnapshot) {
        if self
            .session
            .as_ref()
            .is_none_or(|id| id.0 != snapshot.session)
        {
            return;
        }
        if let Some(current) = self.compactions.iter_mut().find(|s| s.id == snapshot.id) {
            // Admission may arrive after a running/completed broadcast.
            if !crate::compaction::active(current) && crate::compaction::active(&snapshot)
                || current.state == oc_core::compaction::CompactionState::Running
                    && snapshot.state == oc_core::compaction::CompactionState::Queued
            {
                return;
            }
            *current = snapshot.into();
        } else {
            self.compactions.push(snapshot.into());
        }
        if self.compactions.len() > 100 {
            self.compactions.remove(0);
        }
        self.invalidate_transcript();
        self.sync_compaction_clock();
    }

    /// Palette admission preserves the composer, including a literal `/compact` draft.
    pub fn compaction_request_revision(&self) -> Option<u64> {
        (self.panel != TuiPanel::Commands).then_some(self.input_revision)
    }

    pub fn compaction_admitted(
        &mut self,
        snapshot: oc_core::compaction::CompactionSnapshot,
        revision: Option<u64>,
    ) {
        if self
            .session
            .as_ref()
            .is_none_or(|id| id.0 != snapshot.session)
        {
            return;
        }
        if Some(self.input_revision) == revision
            && dispatch(self.input.trim()) == Some(CommandAction::CompactSession)
        {
            self.input.clear();
            self.editor.clear();
            self.input_revision += 1;
        }
        if self.panel == TuiPanel::Commands {
            self.close_panel();
        }
        self.apply_compaction(snapshot);
    }

    fn sync_compaction_clock(&mut self) {
        if self
            .compactions
            .iter()
            .any(|s| s.state == oc_core::compaction::CompactionState::Running)
            || self.live_parts.iter().any(|part| matches!(part,
                LivePart::Tool { card, .. } if card.name == "compress" && matches!(card.state.as_str(), "started" | "running")))
        {
            self.compaction_at.get_or_insert_with(Instant::now);
        } else {
            self.compaction_at = None;
            self.compaction_frame = 0;
        }
    }

    pub(super) fn tick_compaction(&mut self, now: Instant) -> bool {
        if self.chrome.animations == Some(false) {
            return false;
        }
        let Some(at) = self.compaction_at else {
            return false;
        };
        let elapsed = now.saturating_duration_since(at).as_millis();
        let steps = elapsed / 80;
        if steps == 0 {
            return false;
        }
        self.compaction_frame = (self.compaction_frame + (steps % 10) as usize) % 10;
        self.compaction_at = now.checked_sub(Duration::from_millis((elapsed % 80) as u64));
        true
    }

    // ---- snapshots from the binary -------------------------------------

    /// Replace only the browse view after a provider-scoped owner query. This
    /// must not manufacture a committed model or change composer chrome/agent.
    pub fn apply_picker_catalog(&mut self, snapshot: CatalogSnapshot) {
        if self.chrome.location != snapshot.chrome.location
            || self.chrome.selection_generation != snapshot.chrome.selection_generation
        {
            return;
        }
        self.bind_picker(&snapshot);
        let catalog = catalog_from_snapshot(&snapshot);
        match self.picker.as_mut() {
            Some(picker) if picker.provider() == snapshot.provider => picker.refresh(catalog),
            _ => self.picker = Some(ModelPicker::new(catalog)),
        }
        self.catalog_loaded = true;
        self.refresh_subagent_cards();
        self.sync_modal_cursor();
    }

    /// Apply a catalog snapshot: picker, agents and the effective selection.
    pub fn apply_catalog(&mut self, snapshot: CatalogSnapshot) {
        self.apply_catalog_inner(snapshot, true);
    }

    fn apply_catalog_inner(&mut self, snapshot: CatalogSnapshot, refresh_composer: bool) {
        self.invalidate_sidebar_pointer();
        let dialog_changed = self.chrome.dialog_shortcuts != snapshot.chrome.dialog_shortcuts;
        let mut previous_issues = self.service_issues();
        self.invalidate_transcript();
        if self.active_agent.as_deref() != snapshot.agent_id.as_deref() {
            // A cached refusal belongs to its queried profile. Defer admission
            // to the owner until the replacement profile's current query arrives.
            self.dcp.invalidate_availability();
        }
        if self.chrome.conversation_shortcuts.leader
            != snapshot.chrome.conversation_shortcuts.leader
            || self.chrome.leader_timeout_ms() != snapshot.chrome.leader_timeout_ms()
            || self.chrome.command_palette_shortcut != snapshot.chrome.command_palette_shortcut
            || self.chrome.dialog_shortcuts != snapshot.chrome.dialog_shortcuts
            || self.chrome.prompt_history_shortcuts != snapshot.chrome.prompt_history_shortcuts
        {
            self.leader = None;
        }
        self.set_conversation_shortcuts(
            Some(snapshot.chrome.conversation_shortcuts.undo.clone()),
            Some(snapshot.chrome.conversation_shortcuts.redo.clone()),
        );
        if self.chrome.location != snapshot.chrome.location {
            self.reset_sidebar_scroll();
            previous_issues.clear();
            self.mcp_snapshot = None;
            if self
                .mcp_detail
                .as_ref()
                .is_some_and(|detail| detail.copy_pending)
            {
                self.pending_copy = None;
            }
            self.mcp_detail = None;
            self.mcp_focused = None;
            self.mcp_action_down = None;
            self.service_pending_issues.clear();
            self.refused_submission = None;
            self.generation += 1;
            self.clear_mentions();
        }
        self.apply_owner_clipboard_mode(snapshot.chrome.terminal_copy);
        if self.chrome.location != snapshot.chrome.location {
            self.tab_view.get_mut().reset_rail_width();
        } else if self.chrome.vertical_tabs_width != snapshot.chrome.vertical_tabs_width {
            // An accepted Location preference is independent of layout mode;
            // horizontal config ignores it, but invalidates the old gesture.
            self.tab_view.get_mut().cancel_rail_drag();
        }
        if (self.chrome.animations != Some(false)) != (snapshot.chrome.animations != Some(false)) {
            for clock in self
                .tool_spinner_clocks
                .values_mut()
                .chain(self.reasoning_spinner_clock.iter_mut())
            {
                *clock = ToolSpinnerClock::default();
            }
            for part in &mut self.live_parts {
                if let LivePart::Tool {
                    delegation: Some(clock),
                    ..
                } = part
                {
                    clock.frame = 0;
                    clock.at = None;
                }
            }
        }
        self.chrome = snapshot.chrome.clone();
        if dialog_changed {
            self.select.clear_action_focus();
            self.mcp_action_down = None;
        }
        self.auto_accept = snapshot.auto_accept;
        // A staged next-send agent survives canonical refreshes of this view;
        // the composer keeps showing it until the owner reports it committed.
        if refresh_composer {
            let agent_draft = self.agent_draft.take().filter(|draft| {
                draft.agent_id != snapshot.agent_id
                    && draft.chrome.location == snapshot.chrome.location
                    && draft.chrome.selection_generation == snapshot.chrome.selection_generation
            });
            self.picker = Some(self.composer_picker(agent_draft.as_ref().unwrap_or(&snapshot)));
            self.agent_draft = agent_draft;
        }
        self.reconcile_service_feedback(previous_issues);
        self.commands = snapshot.commands;
        self.command_descriptions = snapshot.command_descriptions;
        self.active_agent = snapshot.agent_id.clone().map(|id| {
            if snapshot.chrome.selection.as_ref().is_some_and(|issue| {
                issue.diagnostic.code == oc_core::queries::ServiceCode::AgentUnavailable
            }) {
                format!("{id} (unavailable)")
            } else {
                id
            }
        });
        self.agents = snapshot.agents;
        self.sync_agents_cursor();
        self.catalog_loaded = true;
        self.sync_modal_cursor();
        self.refresh_subagent_cards();
    }

    /// Composer picker and model binding for one (canonical or staged) agent.
    fn composer_picker(&mut self, snapshot: &CatalogSnapshot) -> ModelPicker {
        let draft = self.composer_catalog(snapshot);
        let catalog = draft
            .as_ref()
            .filter(|draft| draft.model.provider != snapshot.provider)
            .map_or_else(
                || catalog_from_snapshot(snapshot),
                |draft| draft.catalog.clone(),
            );
        let mut picker = ModelPicker::new(catalog);
        if !snapshot.model_id.is_empty() {
            let record = serde_json::json!({
                "provider": snapshot.provider,
                "id": snapshot.model_id,
                "variant": snapshot.variant,
            });
            picker.load_persisted_raw(Some(&record.to_string()));
            picker.focus_id(&snapshot.model_id);
        }
        if let Some(draft) = draft {
            picker.load_persisted_raw(Some(
                &serde_json::to_string(&draft.model).expect("model ref"),
            ));
            picker.focus_id(&draft.model.id);
        }
        if snapshot.chrome.selection.is_none()
            && snapshot
                .chrome
                .provider
                .as_ref()
                .is_none_or(|provider| provider.status == oc_core::queries::ProviderStatus::Ready)
            && let Some(error) = picker.last_error()
        {
            self.push_note(error);
        }
        picker
    }

    fn sync_agents_cursor(&mut self) {
        self.agents_cursor = self
            .composer_agent()
            .and_then(|id| self.agents.iter().position(|agent| agent.id == id))
            .unwrap_or(usize::MAX);
    }

    /// Fence older preparation while the next-send agent is being resolved.
    pub fn begin_agent_selection(&mut self) {
        // Supersede older preparation without guessing the new profile. The
        // controller keeps sending disabled until this owned preview resolves.
        self.agent_draft_revision = self.agent_draft_revision.wrapping_add(1);
    }

    pub fn agent_selection_revision(&self) -> u64 {
        self.agent_draft_revision
    }

    pub fn agent_picker_identity(&self) -> Option<(&str, usize)> {
        (self.panel == TuiPanel::Agents).then_some((self.select.query.as_str(), self.agents_cursor))
    }

    /// Stage an owner `Preview` as the composer's next-send agent (OC2 local
    /// agent draft). The running turn keeps its profile; nothing is committed.
    pub fn apply_agent_preview(&mut self, snapshot: CatalogSnapshot) -> Result<(), String> {
        if snapshot.chrome.location != self.chrome.location
            || snapshot.chrome.selection_generation != self.chrome.selection_generation
        {
            return Err("agent preview is stale; try again".into());
        }
        self.picker = Some(self.composer_picker(&snapshot));
        self.agent_draft_revision = self.agent_draft_revision.wrapping_add(1);
        self.agent_draft = (self.active_agent.as_deref() != snapshot.agent_id.as_deref()
            || self.has_pending_submission())
        .then_some(snapshot);
        self.sync_agents_cursor();
        self.sync_modal_cursor();
        Ok(())
    }

    /// The staged next-send agent, if it differs from the session's own.
    pub fn staged_agent(&self) -> Option<String> {
        self.agent_draft
            .as_ref()
            .and_then(|draft| draft.agent_id.clone())
    }

    /// Next-send agent: the staged draft, otherwise the session's own.
    pub fn composer_agent(&self) -> Option<&str> {
        match &self.agent_draft {
            Some(draft) => draft.agent_id.as_deref(),
            None => self.active_agent.as_deref(),
        }
    }

    /// Apply the session list snapshot.
    pub fn apply_sessions(&mut self, sessions: Vec<String>) {
        self.session_entries.clear();
        self.sessions = sessions;
        self.sessions_cursor = 0;
        self.sessions_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply the bounded application-owned root metadata page.
    pub fn apply_session_entries(&mut self, entries: Vec<oc_core::queries::SessionListEntry>) {
        self.sessions = entries.iter().map(|entry| entry.id.0.clone()).collect();
        self.session_entries = entries;
        self.sessions_loaded = true;
        if self.select.query.is_empty() {
            self.select.cursor = self
                .sessions
                .iter()
                .position(|id| {
                    self.attached_session()
                        .is_some_and(|current| &current.0 == id)
                })
                .unwrap_or(0);
            self.select.follow_selection();
        }
        self.sync_modal_cursor();
    }

    pub fn apply_session_picker_context(
        &mut self,
        context: oc_core::queries::SessionPickerContext,
    ) {
        self.sessions_all_projects = context.all_projects;
        self.session_project_name = context.project_name;
    }

    pub fn selected_session_renamed(&mut self, id: &str, title: String) {
        if self
            .attached_session()
            .is_some_and(|session| session.0 == id)
        {
            self.session_title = Some(title.clone());
        }
        if self.rename_selected.as_deref() == Some(id) {
            self.close_panel();
        }
    }

    pub fn session_delete_rejected(&mut self, message: String) {
        self.session_delete_confirm = None;
        self.apply_intent_error(message);
    }

    /// Apply the skill card snapshot (bodies never reach the view).
    pub fn apply_skills(&mut self, cards: Vec<SkillCard>) {
        self.skills = cards;
        self.skills_cursor = 0;
        self.skills_loaded = true;
        self.sync_modal_cursor();
    }

    /// Apply a DCP context/stats snapshot.
    pub fn apply_dcp_snapshot(&mut self, snapshot: DcpSnapshot) {
        self.dcp.set_snapshot(snapshot);
    }

    /// Apply a newest-first tool-card page; rendered as bounded rows.
    pub fn apply_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        self.card_output = None;
        self.card_scroll = 0;
        self.card_seen.set(0);
        self.card_ops = cards.iter().map(|card| card.op.clone()).collect();
        self.cards = cards.iter().map(card_row).collect();
        self.cards_cursor = 0;
        self.cards_loaded = true;
        self.cards_has_older = has_older;
    }

    /// Prepend an older tool-card page (paging up in the Cards panel).
    pub fn prepend_cards(&mut self, cards: Vec<ToolCard>, has_older: bool) {
        let mut ops: Vec<String> = cards.iter().map(|card| card.op.clone()).collect();
        ops.append(&mut self.card_ops);
        ops.truncate(CARDS_MAX);
        self.card_ops = ops;
        let mut rows: Vec<HistoryRow> = cards.iter().map(card_row).collect();
        rows.append(&mut self.cards);
        rows.truncate(CARDS_MAX);
        self.cards = rows;
        self.cards_has_older = has_older;
    }

    /// True when older tool cards exist before the loaded page.
    pub fn cards_need_older(&self) -> bool {
        self.cards_has_older
    }

    /// Replace a single bounded output page. The text remains application-owned;
    /// this preview disappears when the panel/session is closed.
    pub fn apply_card_output(
        &mut self,
        op: String,
        offset: usize,
        page: oc_core::queries::ToolOutputPage,
    ) {
        if self.panel == TuiPanel::Cards && self.card_ops.contains(&op) {
            self.card_output = Some(CardOutput { op, offset, page });
            self.card_scroll = 0;
            self.card_seen.set(0);
            self.select.reset();
        }
    }

    /// Report a runtime DCP outcome: transient notice, never chat history.
    pub fn notify_dcp(&mut self, outcome: DcpOutcome) {
        let failure = matches!(outcome, DcpOutcome::Failed { .. });
        self.dcp.set_outcome(outcome);
        if (failure || self.chrome.dcp.notification != oc_core::dcp_view::DcpNotificationMode::Off)
            && let Some(notice) = self.dcp.notice().map(str::to_string)
        {
            self.push_transient_note(
                &notice,
                if failure {
                    NoteVariant::Error
                } else {
                    NoteVariant::Info
                },
            );
        }
        self.dcp.clear_notice();
    }

    /// Query only committed block IDs retained by this view, with a hard batch
    /// bound. Disabled/minimal/toast views never request summary contents.
    pub fn dcp_summary_requests(&self) -> Vec<(String, usize)> {
        use oc_core::dcp_view::{DcpNotificationChannel, DcpNotificationMode};
        if !self.chrome.dcp.show_compression
            || self.chrome.dcp.notification != DcpNotificationMode::Detailed
            || self.chrome.dcp.channel != DcpNotificationChannel::Chat
        {
            return Vec::new();
        }
        self.compose_structure(true)
            .0
            .into_iter()
            .rev()
            .filter_map(|row| row.tool)
            .flat_map(|card| {
                let crate::tools::ToolRender::Dcp(view) = &card.render else {
                    return Vec::new();
                };
                let Some(run) = &view.snapshot else {
                    return Vec::new();
                };
                run.block_ids
                    .iter()
                    .enumerate()
                    .filter(|(_, id)| !view.summaries.iter().any(|page| &page.block_id == *id))
                    .map(|(index, _)| (card.op.clone(), index))
                    .collect::<Vec<_>>()
            })
            .take(32)
            .collect()
    }

    pub fn apply_dcp_summary(
        &mut self,
        session: &SessionId,
        op: &str,
        page: oc_core::dcp_view::DcpSummaryPage,
    ) {
        if self.session.as_ref() != Some(session) {
            return;
        }
        self.invalidate_transcript();
        if self.window.apply_dcp_summary(op, page.clone()) {
            return;
        }
        for part in &mut self.live_parts {
            if let LivePart::Tool { card, .. } = part
                && card.op == op
                && let crate::tools::ToolRender::Dcp(view) =
                    &mut std::sync::Arc::make_mut(card).render
            {
                view.apply_summary(page);
                self.enforce_parts();
                return;
            }
        }
    }

    /// Report that an intent could not be applied; the input is kept so the
    /// user can retry or edit it.
    pub fn apply_intent_error(&mut self, message: String) {
        self.push_note(&message);
    }

    /// Call only after the application owner has accepted the title update.
    /// This also updates the active tab until the next owner deck snapshot.
    pub fn rename_session_applied(&mut self, title: String) {
        if self.panel != TuiPanel::Rename || self.rename_pending.as_deref() != Some(&title) {
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        self.close_panel();
        self.note = None;
    }

    /// Owner refusal retains the focused title and prompt draft for retry.
    pub fn rename_session_rejected(&mut self, message: String) {
        if self.panel == TuiPanel::Rename {
            self.rename_pending = None;
            self.apply_intent_error(message);
        }
    }

    /// ACK a direct `/rename <title>` only when its matching owner request completed.
    /// Preserve edits made to the composer while the owner was working.
    pub fn rename_session_direct_applied(&mut self, title: String) {
        let Some((pending, revision)) = self.rename_direct_pending.take() else {
            return;
        };
        if pending != title {
            self.rename_direct_pending = Some((pending, revision));
            return;
        }
        self.session_title = Some(title.clone());
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.title = Some(title);
        }
        if self.input_revision == revision {
            self.accept_intent();
            self.input_revision += 1;
        }
        self.note = None;
    }

    /// A failed direct request leaves the original slash draft available for correction.
    pub fn rename_session_direct_rejected(&mut self, message: String) {
        if self.rename_direct_pending.take().is_some() {
            self.apply_intent_error(message);
        }
    }

    /// Resolve only the matching slash request, preserving edits typed while
    /// the provider was running. A completion on another tab is not applied.
    pub fn regenerated_title(&mut self, result: Result<String, String>) {
        let Some(revision) = self.regenerate_pending.take() else {
            return;
        };
        match result {
            Ok(title) => {
                self.session_title = Some(title.clone());
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.title = Some(title);
                }
                if self.input_revision == revision {
                    self.accept_intent();
                    self.input_revision += 1;
                }
                self.note = None;
            }
            Err(message) => self.apply_intent_error(message),
        }
    }

    /// Report that an intent was accepted and applied; clears the input.
    pub fn input_revision(&self) -> u64 {
        self.input_revision
    }

    pub fn accept_intent(&mut self) {
        self.input.clear();
        self.editor.clear();
    }

    /// The accepted compress turn starts streaming: status, turn, DCP panel.
    pub fn begin_compress_turn(&mut self, turn: WorkerTurnId) {
        self.invalidate_transcript();
        self.reset_scanner();
        self.live_preview_truncated = false;
        self.live_part_states.clear();
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_agent_color_index = None;
        self.compress_turn = Some(turn.clone());
        self.live_retry = None;
        self.live_retry_seen = None;
        self.retry_due = None;
        self.live_span = None;
        self.live_projection_revision = None;
        self.active_turn = Some(turn);
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
        self.status = TuiStatus::Streaming;
        self.panel = TuiPanel::Dcp;
        self.clear_mouse_position();
        self.input.clear();
        self.editor.clear();
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.live_part_offset = 0;
        self.live_part_sources.clear();
        self.live_checkpoint_end = None;
        self.reasoning_down = None;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.turn_context_usage = None;
        self.scroll = 0;
        self.wheel_motion = None;
    }

    /// Whether the active turn was accepted from a manual compression request.
    pub fn is_compress_turn(&self, turn: &WorkerTurnId) -> bool {
        self.compress_turn.as_ref() == Some(turn) && self.active_turn.as_ref() == Some(turn)
    }

    /// Enqueue manual compression using the same draft/receipt lifecycle as text.
    pub fn request_compress(&mut self, focus: String) -> Result<(), CoreError> {
        if self.is_busy() || self.has_occupied_submission() {
            return Err(CoreError::TurnBusy);
        }
        if self.editor.has_images() {
            return Err(CoreError::Application(
                "commands do not accept image attachments".into(),
            ));
        }
        if !self.chrome.dcp.commands_enabled {
            return Err(CoreError::Application(
                oc_core::dcp_view::DcpUnavailable::CommandsOff
                    .reason()
                    .into(),
            ));
        }
        if let Some(reason) = self.dcp.manual_refusal() {
            return Err(CoreError::Application(reason.reason().into()));
        }
        let session = self.session.clone().ok_or(CoreError::SessionNotFound)?;
        let receipt = self.app.request_compress(session.clone(), focus)?;
        let mut pending = self.capture_submission(session, SubmissionPurpose::Compress);
        pending.phase = SubmissionPhase::AwaitingTurn(receipt);
        self.occupy_submission(pending);
        Ok(())
    }

    fn capture_submission(
        &self,
        session: SessionId,
        purpose: SubmissionPurpose,
    ) -> PendingSubmission {
        let agent = self.composer_agent().map(str::to_owned);
        let agent_color_index = agent.as_deref().and_then(|agent| {
            self.agents
                .iter()
                .find(|entry| entry.id == agent)
                .map(|entry| entry.color_index)
        });
        let prompt = self.editor.prompt(&self.input).trimmed();
        PendingSubmission {
            selection: self.captured_model_commit(),
            request_id: self.request_id.wrapping_add(1),
            generation: self.generation,
            session,
            fresh: self.session.is_none(),
            draft: crate::ComposerDraft::default(),
            source: prompt.text.as_str().into(),
            prompt,
            mentions: self.editor.submitted_mentions(&self.input),
            phase: SubmissionPhase::Failed(String::new()),
            purpose,
            promote: matches!(purpose, SubmissionPurpose::Empty)
                .then(|| self.promote_first_queued())
                .flatten(),
            agent_revision: self.agent_draft_revision,
            agent,
            agent_color_index,
            cancelling: false,
        }
    }

    fn occupy_submission(&mut self, mut pending: PendingSubmission) {
        if pending.fresh {
            self.provisional_session = Some(pending.session.clone());
        }
        self.request_id = pending.request_id;
        if !matches!(pending.purpose, SubmissionPurpose::Empty) {
            pending.draft = crate::ComposerDraft {
                text: std::mem::take(&mut self.input),
                editor: std::mem::take(&mut self.editor),
            };
            self.input_revision = self.input_revision.wrapping_add(1);
            self.slash_selected = 0;
            self.slash_dismissed = None;
            self.clear_mentions();
        }
        self.pending = Some(pending);
        if self.active_turn.is_none() {
            self.status = TuiStatus::PendingSubmission;
        }
        self.invalidate_transcript();
    }

    fn request_captured_input(
        &self,
        pending: &PendingSubmission,
    ) -> Result<SubmissionPhase, CoreError> {
        if let SubmissionPurpose::Inbox(delivery) = pending.purpose {
            return self
                .app
                .request_enqueue_selected(
                    pending.session.clone(),
                    pending.prompt.clone(),
                    delivery,
                    pending.selection.clone(),
                )
                .map(SubmissionPhase::AwaitingInbox);
        }
        let receipt = if pending.fresh {
            let selection =
                pending
                    .selection
                    .clone()
                    .map(|commit| oc_core::core_app::FreshSelection {
                        binding: Some(commit.binding.clone()),
                        agent_id: commit.binding.agent_id,
                        model_id: commit.model_id,
                        variant: commit.variant,
                    });
            self.app.request_submit_input_fresh(
                pending.session.clone(),
                pending.prompt.clone(),
                selection,
            )
        } else {
            self.app.request_submit_input_selected(
                pending.session.clone(),
                pending.prompt.clone(),
                pending.selection.clone(),
            )
        }?;
        Ok(SubmissionPhase::AwaitingTurn(receipt))
    }

    /// Capture once, then transfer the rich draft only after bounded local admission.
    pub(super) fn request_composer_submission(&mut self, purpose: SubmissionPurpose) -> KeyOutcome {
        if self.has_occupied_submission() {
            return KeyOutcome {
                note: Some(self.submission_occupied_note().into()),
                ..Default::default()
            };
        }
        if self.parent_id.is_some() {
            return KeyOutcome {
                note: Some("child session is read-only".into()),
                ..Default::default()
            };
        }
        let session = self.session.clone().unwrap_or_else(fresh_session_id);
        let mut pending = self.capture_submission(session, purpose);
        if let Err(error) = pending.prompt.check_budget() {
            return KeyOutcome {
                note: Some(self.submission_note(&error)),
                ..Default::default()
            };
        }
        let phase = if !pending.fresh && !self.home && self.staged_agent().is_some() {
            self.app
                .request_session_selection(
                    pending.session.clone(),
                    false,
                    match &pending.selection {
                        Some(commit) => {
                            oc_core::queries::SessionSelectionAction::AgentCommit(commit.clone())
                        }
                        None => oc_core::queries::SessionSelectionAction::Agent(
                            pending.agent.clone().expect("staged agent"),
                        ),
                    },
                )
                .map(SubmissionPhase::PreparingAgent)
        } else {
            self.request_captured_input(&pending)
        };
        match phase {
            Ok(phase) => {
                pending.phase = phase;
                self.occupy_submission(pending);
                KeyOutcome::default()
            }
            Err(error) => KeyOutcome {
                note: Some(self.submission_note(&error)),
                ..Default::default()
            },
        }
    }

    pub(super) fn submission_occupied_note(&self) -> &str {
        if self.pending.as_ref().is_some_and(|p| !p.outstanding()) {
            "last request retained; Settings → Last request: Ctrl+U restore into an empty editor, Ctrl+D discard"
        } else {
            "submission pending; Esc to cancel"
        }
    }

    pub(super) fn failed_submission_note(&self) -> Option<&str> {
        match &self.pending.as_ref()?.phase {
            SubmissionPhase::Failed(note) => Some(note),
            _ => None,
        }
    }

    fn fail_submission(&mut self, mut pending: PendingSubmission, error: CoreError) {
        let unknown = error == CoreError::Shutdown;
        let detail = self.submission_note(&error);
        let note = if unknown {
            format!(
                "Request outcome unknown; it may already be accepted. No automatic resend. {detail}. Ctrl+U restores only into an empty editor; Ctrl+D discards (Settings → Last request)."
            )
        } else {
            format!(
                "{detail}. Draft retained; Ctrl+U restores only into an empty editor; Ctrl+D discards (Settings → Last request)."
            )
        };
        pending.phase = SubmissionPhase::Failed(note.clone());
        self.pending = Some(pending);
        if self.status != TuiStatus::Quit {
            self.status = if self.active_turn.is_some() {
                TuiStatus::Streaming
            } else {
                TuiStatus::Idle
            };
        }
        self.push_note(if unknown { &note } else { &detail });
        self.invalidate_transcript();
    }

    /// Reconcile the unique acceptance receipt before applying queued turn
    /// events. A deferred blank-Enter inbox intent retains its captured item ID.
    pub fn poll_submission(&mut self) -> Option<PanelIntent> {
        self.poll_submission_receipts();
        self.submission_intent.take()
    }

    // Input/paint callers must not consume a deferred controller action.
    pub(super) fn poll_submission_receipts(&mut self) {
        if let Some(intent) = self.poll_submission_inner() {
            self.submission_intent = Some(intent);
        }
    }

    fn poll_submission_inner(&mut self) -> Option<PanelIntent> {
        self.poll_model_commits();
        self.poll_user_shell();
        let mut pending = self.pending.take()?;
        if pending.generation != self.generation || pending.request_id != self.request_id {
            return None;
        }
        if self.status == TuiStatus::Quit {
            self.pending = Some(pending);
            return None;
        }
        match &mut pending.phase {
            SubmissionPhase::PreparingAgent(receipt) => {
                if let Some(result) = receipt.try_result() {
                    match result {
                        Ok(snapshot) => {
                            if snapshot.chrome.location != self.chrome.location
                                || snapshot.chrome.selection_generation
                                    != self.chrome.selection_generation
                                || snapshot.agent_id != pending.agent
                                || pending.selection.as_ref().is_some_and(|commit| {
                                    snapshot.provider != commit.binding.provider
                                        || snapshot.model_id != commit.model_id
                                        || snapshot.variant.as_deref().filter(|v| *v != "default")
                                            != commit.variant.as_deref()
                                })
                            {
                                self.fail_submission(
                                    pending,
                                    CoreError::Application(
                                        "agent preparation: stale acknowledgement".into(),
                                    ),
                                );
                                return None;
                            }
                            self.apply_catalog_inner(snapshot, false);
                            self.model_submission_accepted(
                                &pending.session,
                                pending.selection.as_ref(),
                            );
                            if self.agent_draft_revision == pending.agent_revision {
                                self.agent_draft = None;
                            }
                            if pending.cancelling {
                                self.fail_submission(
                                    pending,
                                    CoreError::Application(
                                        "cancelled before input admission".into(),
                                    ),
                                );
                                return None;
                            }
                            if matches!(pending.purpose, SubmissionPurpose::Empty) {
                                if self.active_turn.is_none() {
                                    self.status = TuiStatus::Idle;
                                }
                                self.invalidate_transcript();
                                return pending.promote;
                            }
                            match self.request_captured_input(&pending) {
                                Ok(phase) => pending.phase = phase,
                                Err(error) => {
                                    self.fail_submission(pending, error);
                                    return None;
                                }
                            }
                        }
                        Err(error) => {
                            self.fail_submission(pending, error);
                            return None;
                        }
                    }
                }
            }
            SubmissionPhase::AwaitingTurn(receipt) => {
                if let Some(result) = receipt.try_result() {
                    self.reconcile_submission(pending, result, false);
                    return None;
                }
            }
            SubmissionPhase::AwaitingInbox(receipt) => {
                if let Some(result) = receipt.try_result() {
                    match result {
                        Ok(item) => {
                            self.model_submission_accepted(
                                &pending.session,
                                pending.selection.as_ref(),
                            );
                            if !self.inbox.iter().any(|known| known.id == item.id) {
                                self.inbox.push(item);
                            }
                            self.invalidate_transcript();
                            return None;
                        }
                        Err(CoreError::TurnNotActive) if !pending.cancelling => {
                            // The sole definitive fallback. Never re-enter handle_enter,
                            // recapture a draft or retry a dropped/unknown receipt.
                            pending.purpose = SubmissionPurpose::Turn;
                            match self.request_captured_input(&pending) {
                                Ok(phase) => pending.phase = phase,
                                Err(error) => {
                                    self.fail_submission(pending, error);
                                    return None;
                                }
                            }
                        }
                        Err(error) => {
                            self.fail_submission(pending, error);
                            return None;
                        }
                    }
                }
            }
            SubmissionPhase::Failed(_) => {}
        };
        self.pending = Some(pending);
        None
    }

    /// Quit must not discard an in-flight Home root after the owner commits
    /// it. Cancel through the owner first (so tool work is stopped safely),
    /// then resolve the *same* receipt before the application shuts down.
    /// Existing-session turns already have a durable tab and need no wait.
    pub async fn reconcile_fresh_quit(&mut self) -> Result<(), CoreError> {
        // Native acceptance/adoption is owned through ACK by the authority, even
        // after this viewer leaves. Waiting here before releasing the TUI lease
        // would deadlock a last-viewer quit on a pre-acceptance Ask.
        if self.app.shares_workspace() {
            return Ok(());
        }
        self.reconcile_user_shell_quit().await?;
        let Some(pending) = self.pending.as_mut().filter(|p| p.fresh && p.outstanding()) else {
            return Ok(());
        };
        pending.cancelling = true;
        let session = pending.session.clone();
        // Cancel follows SubmitFresh in the owner's inbox. A rejected or
        // already-finished turn has nothing left to cancel.
        match if self.app.shares_workspace() {
            Ok(())
        } else {
            self.app.cancel(session).await
        } {
            Ok(()) | Err(CoreError::TurnNotActive) => {}
            Err(error) => return Err(error),
        }
        let SubmissionPhase::AwaitingTurn(receipt) = &mut self
            .pending
            .as_mut()
            .expect("fresh receipt still pending")
            .phase
        else {
            return Ok(());
        };
        let result = receipt.wait().await;
        let pending = self.pending.take().expect("fresh receipt still pending");
        if result == Err(CoreError::Shutdown) {
            return Err(CoreError::Shutdown);
        }
        self.reconcile_submission(pending, result, true);
        Ok(())
    }

    fn reconcile_submission(
        &mut self,
        pending: PendingSubmission,
        result: Result<WorkerTurnId, CoreError>,
        exiting: bool,
    ) {
        if (self.status == TuiStatus::Quit && !exiting)
            || pending.request_id != self.request_id
            || pending.generation != self.generation
            || (pending.fresh != self.session.is_none())
            || (!pending.fresh && self.session.as_ref() != Some(&pending.session))
        {
            return;
        }
        match result {
            Ok(turn) => {
                self.invalidate_transcript();
                self.retire_reverted_boundary();
                if pending.fresh {
                    self.session = Some(pending.session.clone());
                }
                self.model_submission_accepted(&pending.session, pending.selection.as_ref());
                self.live_preview_truncated = false;
                self.live_part_states.clear();
                self.live_terminal_status = None;
                self.live_model_label = None;
                self.live_agent_color_index = None;
                if !matches!(pending.purpose, SubmissionPurpose::Compress) {
                    self.home = false;
                    self.editor
                        .accepted_mentions(&pending.prompt.text, pending.mentions);
                    self.window.push_synthetic(
                        "user",
                        &pending.prompt.text,
                        pending.agent,
                        pending.agent_color_index,
                    );
                    self.window
                        .set_latest_user_images(pending.prompt.images.len());
                    self.prune_reasoning();
                }
                self.compress_turn =
                    matches!(pending.purpose, SubmissionPurpose::Compress).then(|| turn.clone());
                self.conversation_available = Some((true, false));
                self.live_text.clear();
                self.live_reasoning.clear();
                self.live_parts.clear();
                self.pending_tool_seen.clear();
                self.pending_tool_round = 0;
                self.live_part_offset = 0;
                self.live_part_sources.clear();
                self.live_checkpoint_end = None;
                self.reasoning_down = None;
                self.reasoning_started = None;
                self.reasoning_finished = None;
                self.turn_usage = None;
                self.turn_context_usage = None;
                self.live_retry = None;
                self.live_retry_seen = None;
                self.retry_due = None;
                self.live_span = None;
                self.live_projection_revision = None;
                self.active_turn = Some(turn);
                self.projection_epoch = self.projection_epoch.wrapping_add(1);
                self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
                if !exiting {
                    self.reset_scanner();
                    self.status = TuiStatus::Streaming;
                }
                self.scroll = 0;
                self.wheel_motion = None;
                self.dcp.clear_notice();
                self.note = None;
                self.refused_submission = None;
            }
            Err(error) => {
                self.fail_submission(pending, error);
            }
        }
    }

    // ---- worker events --------------------------------------------------

    /// Native stream coverage belongs to this retained session view, not to a
    /// turn ID or the shared receiver. Other sessions still receive their hints.
    pub fn projection_floor(&self) -> u64 {
        self.projection_floor
    }

    pub fn cover_projection(&mut self, revision: u64, truncated: bool) {
        self.projection_floor = self.projection_floor.max(revision);
        self.live_preview_truncated |= truncated;
    }

    /// Replace only disposable turn presentation after a canonical/live repair.
    /// The caller prepares all fallible reads first; composer, history paging,
    /// linked authority, controls and durable expansion state stay in this view.
    pub fn reset_live_projection(&mut self) {
        self.live_text.clear();
        self.live_reasoning.clear();
        self.live_parts.clear();
        self.pending_tool_seen.clear();
        self.pending_tool_round = 0;
        self.live_part_offset = 0;
        self.live_part_sources.clear();
        self.live_checkpoint_end = None;
        self.live_part_states.clear();
        self.live_agent_color_index = None;
        self.live_terminal_status = None;
        self.live_model_label = None;
        self.live_mixed_models = false;
        self.live_preview_truncated = false;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.reasoning_down = None;
        self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
        self.turn_usage = None;
        self.turn_context_usage = None;
        self.live_retry = None;
        self.live_retry_seen = None;
        self.retry_due = None;
        self.live_span = None;
        self.live_projection_revision = None;
        self.active_turn = None;
        self.compress_turn = None;
        self.retire_delegation_clocks();
        self.reset_scanner();
        if self.status != TuiStatus::Quit {
            self.status = TuiStatus::Idle;
        }
        self.invalidate_transcript();
    }

    /// Observe an owner-accepted child without admitting synthetic user input.
    pub fn begin_linked_turn(&mut self, turn: WorkerTurnId) {
        if self.active_turn.as_ref() == Some(&turn) {
            return;
        }
        self.live_part_sources.clear();
        self.live_checkpoint_end = None;
        self.retire_delegation_clocks();
        self.active_turn = Some(turn);
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        self.status = TuiStatus::Streaming;
        self.turn_usage = None;
        self.turn_context_usage = None;
        self.invalidate_transcript();
    }

    pub fn reconcile_linked_terminal(
        &mut self,
        job: &oc_core::queries::ChildJob,
        page: &oc_core::queries::HistoryPage,
    ) {
        if let Some(turn) = job.turn.as_ref().map(|id| WorkerTurnId(id.clone()))
            && self.active_turn() == Some(&turn)
        {
            let row = page
                .rows
                .iter()
                .find(|r| r.turn.as_ref().is_some_and(|t| t.id == turn.0));
            let text = row.map_or("", |r| r.text.as_str());
            let duration = row.and_then(|r| r.turn.as_ref()?.duration_ms).unwrap_or(0);
            match job.state {
                oc_core::queries::ChildState::Completed => {
                    self.apply_finished(&turn, text, duration);
                }
                oc_core::queries::ChildState::Cancelled => {
                    self.apply_interrupted(&turn, text, duration);
                }
                oc_core::queries::ChildState::Error | oc_core::queries::ChildState::Unknown => {
                    self.apply_failed(
                        &turn,
                        &oc_core::session::CoreError::Application("child execution failed".into()),
                    );
                }
                _ => return,
            }
        }
        self.refresh_completed_page(page);
    }

    pub fn apply_model_switch(
        &mut self,
        turn: &WorkerTurnId,
        notice: &oc_core::queries::ModelSwitchNotice,
    ) {
        if self.active_turn.as_ref() != Some(turn) {
            return;
        }
        self.window.insert_before_live_user(notice);
        self.invalidate_transcript();
    }

    /// Apply a worker text delta to the live line (turn-scoped: deltas for
    /// a stale turn are ignored, so a late event can never corrupt the view).
    pub fn apply_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        if self.live_text.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_text.len();
            self.live_text.push_str(crate::truncate_utf8(delta, room));
            self.invalidate_live_text();
        }
    }

    /// Apply a worker reasoning delta to the live reasoning block
    /// (turn-scoped, bounded, never persisted).
    pub fn apply_reasoning_delta(&mut self, turn: &WorkerTurnId, delta: &str) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if self.live_reasoning.is_empty() && !self.live_text.is_empty() {
            self.freeze_text();
            self.enforce_parts();
            self.invalidate_transcript();
        }
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(Instant::now());
        }
        if self.live_reasoning.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_reasoning.len();
            self.live_reasoning
                .push_str(crate::truncate_utf8(delta, room));
            self.invalidate_live_text();
        }
    }

    /// Close one active public reasoning item before the next output item.
    /// Duplicates and boundaries without public text do not create rows.
    pub fn apply_reasoning_item_ended(&mut self, turn: &WorkerTurnId) {
        if Some(turn) != self.active_turn.as_ref() || self.live_reasoning.is_empty() {
            return;
        }
        self.reasoning_finished = Some(Instant::now());
        self.freeze_reasoning();
        self.enforce_parts();
    }

    /// Adopt explicit checkpoint identities and pinned presentation only for
    /// the current generation/turn. Deltas remain transient until checkpointed.
    pub fn apply_presentation(
        &mut self,
        turn: &WorkerTurnId,
        projection: &oc_core::queries::HistoryTurn,
    ) {
        if self.active_turn.as_ref() != Some(turn) || projection.id != turn.0 {
            return;
        }
        if self
            .live_projection_revision
            .as_ref()
            .is_some_and(|(owner, revision)| owner == turn && *revision > projection.revision)
        {
            return;
        }
        self.live_projection_revision = Some((turn.clone(), projection.revision));
        self.window.apply_notice_order(projection);
        // In-flight checkpoints retain measured usage for recovery before tools
        // run. Only the settled-step event may advance live footer usage; a
        // terminal projection can recover it after completion or failure.
        if projection.status != "started"
            && let Some((input, output)) = projection.context_usage
        {
            self.apply_context_usage(
                turn,
                input,
                output,
                projection.context_usage_limit.unwrap_or(0),
            );
        }
        self.live_part_states = projection.part_states.clone();
        if projection.window.is_some() && !projection.legacy_text_only {
            self.adopt_checkpoint_parts(projection);
        }
        let first = projection
            .spans
            .iter()
            .find_map(|span| span.request.as_ref().map(|r| &r.model));
        self.live_mixed_models = projection
            .spans
            .iter()
            .filter_map(|span| span.request.as_ref())
            .any(|r| Some(&r.model) != first);
        if let Some(span) = projection.spans.last() {
            self.live_span = Some((turn.clone(), span.id.clone()));
            if projection.status == "started"
                && let Some(retry) = &span.retry
            {
                self.apply_retry(turn, &span.id, retry);
            } else {
                self.live_retry = None;
                self.retry_due = None;
            }
        }
        self.invalidate_transcript();
        self.live_preview_truncated |= projection.truncated;
        self.live_agent_color_index = projection.agent_color_index;
        if projection.status != "started" {
            self.live_terminal_status = Some(projection.status.clone());
            self.discard_pending_tools();
        }
        self.live_model_label = projection
            .spans
            .last()
            .and_then(|span| span.request.as_ref())
            .map(|request| request.model_label.clone())
            .or_else(|| {
                (!projection.model_label.is_empty()).then(|| projection.model_label.clone())
            });
    }

    fn adopt_checkpoint_parts(&mut self, projection: &oc_core::queries::HistoryTurn) {
        use oc_core::queries::TranscriptPart;
        // Checkpoints are published after a generation is frozen, before the
        // next generation's deltas. Replace the covered hints with their ordered
        // canonical parts, including messages that never had a text delta.
        let previous = std::mem::take(&mut self.live_parts);
        let old_sources = std::mem::take(&mut self.live_part_sources);
        self.live_part_offset = projection.window.as_ref().map_or(0, |window| window.start);
        self.live_checkpoint_end = projection.window.as_ref().map(|window| window.end);
        self.live_text.clear();
        self.live_reasoning.clear();
        self.reasoning_started = None;
        self.reasoning_finished = None;
        for (index, part) in projection.parts.iter().enumerate() {
            let Some(state) = projection.part_states.get(index) else {
                continue;
            };
            let slot = self.live_part_offset + self.live_parts.len();
            self.live_part_sources.insert(slot, state.sequence);
            self.live_parts.push(match part {
                TranscriptPart::Text(text) => LivePart::Text(text.clone().into()),
                TranscriptPart::Reasoning { text, duration_ms } => LivePart::Reasoning {
                    text: text.clone().into(),
                    duration_ms: *duration_ms,
                },
                TranscriptPart::Tool(row) => {
                    let mut card = card_from_row(row);
                    if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
                        view.color_index = projection.agent_color_index;
                    }
                    LivePart::Tool {
                        card: std::sync::Arc::new(card),
                        input: row.input.clone().unwrap_or_default(),
                        delegation: None,
                    }
                }
            });
        }
        // Unadmitted argument streams are not part of this checkpoint yet.
        for part in previous {
            if let LivePart::Tool { card, delegation, .. } = &part
                && (card.state == "argument_stream" || delegation.is_some())
                && !self.live_parts.iter().any(|current| matches!(current, LivePart::Tool { card: canonical, .. } if canonical.op == card.op))
            {
                self.live_parts.push(part);
            }
        }
        let remapped: Vec<_> = old_sources
            .iter()
            .filter(|(slot, _)| {
                self.reasoning_expanded
                    .contains(&crate::messages::ReasoningIdentity::Live(
                        self.reasoning_epoch,
                        **slot,
                    ))
            })
            .filter_map(|(_, sequence)| {
                self.live_part_sources.iter().find_map(|(slot, source)| {
                    (source == sequence).then_some(crate::messages::ReasoningIdentity::Live(
                        self.reasoning_epoch,
                        *slot,
                    ))
                })
            })
            .collect();
        self.reasoning_expanded.extend(remapped);
        self.enforce_parts();
    }

    /// Apply one completed generation's measured context to its active owner.
    /// This does not create billed usage or a token rate.
    pub fn apply_context_usage(
        &mut self,
        turn: &WorkerTurnId,
        input_tokens: u64,
        output_tokens: u64,
        context_limit: u64,
    ) {
        if Some(turn) == self.active_turn.as_ref() {
            self.turn_context_usage = Some((input_tokens, output_tokens, context_limit));
        }
    }

    /// Apply provider-reported usage for the active turn; without it the
    /// footer omits `tok/s` instead of inventing a rate.
    pub fn apply_usage(
        &mut self,
        turn: &WorkerTurnId,
        input_tokens: u64,
        output_tokens: u64,
        streamed_ms: u64,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.turn_usage = Some(TurnUsage {
            input_tokens,
            output_tokens,
            streamed_ms,
        });
    }

    /// Apply a worker turn-finished event: commit the live answer with its
    /// reasoning block and footer metadata, then release the turn. When tool
    /// cards or frozen segments exist, every part keeps its upstream position
    /// and the footer becomes its own row after them; the committed text is
    /// the concatenation of the frozen segments (the deltas already shown).
    pub fn apply_finished(&mut self, turn: &WorkerTurnId, text: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let canonical_ahead = self.canonical_history_ahead();
        self.invalidate_transcript();
        let meta = self.finish_meta(false, duration_ms);
        self.reset_scanner();
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if self.live_parts.is_empty() && !canonical_ahead {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                position: None,
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: text.into(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
            self.window.set_latest_local_owner(&turn.0, Some(0), true);
            self.prune_reasoning();
            return;
        }
        let parts = self.commit_live_parts(reasoning);
        if !canonical_ahead {
            self.push_committed_parts(parts, turn);
        }
        self.push_terminal_footer(turn, meta);
        self.prune_reasoning();
    }

    /// Apply a worker turn-interrupted event: keep the partial text (never
    /// committed to storage), mark the footer `interrupted`, and release the
    /// turn.
    pub fn apply_interrupted(&mut self, turn: &WorkerTurnId, partial: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let canonical_ahead = self.canonical_history_ahead();
        self.invalidate_transcript();
        let meta = self.finish_meta(true, duration_ms);
        self.reset_scanner();
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Cancelled;
        if self.live_parts.is_empty() && !canonical_ahead {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                position: None,
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: partial.into(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
            self.window.set_latest_local_owner(&turn.0, Some(0), true);
            self.prune_reasoning();
            return;
        }
        // The partial text is already the frozen trailing segment.
        let parts = self.commit_live_parts(reasoning);
        if !canonical_ahead {
            self.push_committed_parts(parts, turn);
        }
        self.push_terminal_footer(turn, meta);
        self.prune_reasoning();
    }

    /// Release a failed turn and show its error, never a successful answer.
    pub fn apply_failed(&mut self, turn: &WorkerTurnId, error: &CoreError) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let canonical_ahead = self.canonical_history_ahead();
        self.invalidate_transcript();
        let mut meta = self.finish_meta(false, 0);
        self.reset_scanner();
        if meta.status.is_none() {
            meta.status = Some("failed".into());
        }
        let reasoning = self.take_reasoning();
        let parts = self.commit_live_parts(reasoning);
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if !parts.is_empty() && !canonical_ahead {
            // Cards already shown stay visible; the error follows them.
            self.push_committed_parts(parts, turn);
        }
        self.push_terminal_footer(turn, meta);
        self.window
            .push_synthetic("", &format!("(error: {error})"), None, None);
        self.window.set_latest_local_owner(&turn.0, None, true);
        self.prune_reasoning();
    }

    /// Freeze the open live segments and return the whole part list in
    /// arrival order, including reasoning after an earlier tool round.
    fn commit_live_parts(&mut self, reasoning: Option<ReasoningBlock>) -> Vec<LivePart> {
        self.discard_pending_tools();
        self.pending_tool_seen.clear();
        self.pending_tool_round = 0;
        if let Some(reasoning) = reasoning {
            self.live_parts.push(LivePart::Reasoning {
                text: reasoning.text,
                duration_ms: reasoning.duration_ms,
            });
        }
        self.freeze_reasoning();
        self.freeze_text();
        self.live_reasoning.clear();
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        let parts = std::mem::take(&mut self.live_parts);
        self.sync_compaction_clock();
        parts
    }

    /// Push committed part rows into the window (bounded like any row).
    fn push_terminal_footer(&mut self, turn: &WorkerTurnId, meta: AssistantMeta) {
        let mut row = footer_row(self.active_agent.clone(), meta);
        row.position = Some(crate::history::HistoryPosition::local(&turn.0, None, true));
        self.window
            .commit_live_row(row, &turn.0, Some(crate::history::HistoryComponent::Footer));
    }

    fn push_committed_parts(&mut self, parts: Vec<LivePart>, turn: &WorkerTurnId) {
        for (ordinal, part) in parts.into_iter().enumerate() {
            if matches!(part, LivePart::Vacant) {
                continue;
            }
            let mut row = part.to_row(
                self.active_agent.clone(),
                Some(crate::messages::ReasoningIdentity::Live(
                    self.reasoning_epoch,
                    self.live_part_offset + ordinal,
                )),
            );
            row.position = Some(crate::history::HistoryPosition::local(
                &turn.0,
                Some(
                    self.live_part_sources
                        .get(&(self.live_part_offset + ordinal))
                        .copied()
                        .unwrap_or(self.live_part_offset + ordinal),
                ),
                false,
            ));
            if self.live_mixed_models
                && row.tool.is_none()
                && let Some(model) = self
                    .live_part_states
                    .iter()
                    .find(|state| {
                        state.sequence
                            == self
                                .live_part_sources
                                .get(&(self.live_part_offset + ordinal))
                                .copied()
                                .unwrap_or(self.live_part_offset + ordinal)
                    })
                    .and_then(|state| state.model_label.clone())
            {
                row.meta = Some(AssistantMeta {
                    model: Some(model),
                    ..Default::default()
                });
            }
            self.window.commit_live_row(
                row,
                &turn.0,
                self.live_part_sources
                    .get(&(self.live_part_offset + ordinal))
                    .copied()
                    .map(crate::history::HistoryComponent::Part),
            );
        }
    }

    /// Freeze the open text segment (a tool call follows it).
    fn freeze_text(&mut self) {
        if !self.live_text.is_empty() {
            self.live_parts
                .push(LivePart::Text(std::mem::take(&mut self.live_text)));
        }
    }

    /// Freeze the open reasoning segment with its measured window.
    pub(super) fn freeze_reasoning(&mut self) {
        if self.live_reasoning.is_empty() {
            return;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            _ => None,
        };
        self.live_parts.push(LivePart::Reasoning {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
        });
        self.reasoning_started = None;
        self.reasoning_finished = None;
    }

    /// Apply a recorded tool-call intent: freeze the open segments, then
    /// append the running card in upstream part order.
    pub fn apply_tool_started(&mut self, turn: &WorkerTurnId, op: &str, name: &str, input: &str) {
        self.apply_tool_started_with_presentation(turn, op, name, input, None);
    }

    /// Owner-derived pending DCP topic; input remains an opaque recorded payload.
    pub fn apply_tool_started_with_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        input: &str,
        dcp_topic: Option<String>,
    ) {
        self.apply_tool_started_with_output_presentation(turn, op, name, input, dcp_topic, None);
    }

    pub fn apply_tool_started_with_output_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        input: &str,
        dcp_topic: Option<String>,
        output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        if name == "compress" && self.live_parts.iter().any(|part| matches!(part,
            LivePart::Tool { card, .. } if card.op == op && matches!(&card.render, crate::tools::ToolRender::Dcp(view) if view.snapshot.is_some()))) {
            return;
        }
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        self.freeze_reasoning();
        self.freeze_text();
        let mut card = card_from_row(&ToolOpView {
            child_job: None,
            question: None,
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: "started".to_string(),
            input: Some(input.to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
            output_presentation: output_presentation.filter(|p| p.label_only && p.is_valid()),
            patch_effects: None,
            dcp: None,
            dcp_topic,
        });
        card.started_at = Some(Instant::now());
        if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
            view.color_index = self.live_agent_color_index;
        }
        if let Some(part) = self
            .live_parts
            .iter_mut()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            if let LivePart::Tool { card: previous, .. } = part {
                card.started_at = previous.started_at.or(card.started_at);
            }
            *part = LivePart::Tool {
                card: std::sync::Arc::new(card),
                input: input.to_string(),
                delegation: None,
            };
            self.enforce_parts();
            self.sync_compaction_clock();
            self.refresh_running_user_shell_output();
            return;
        }
        self.live_parts.push(LivePart::Tool {
            card: std::sync::Arc::new(card),
            input: input.to_string(),
            delegation: None,
        });
        self.enforce_parts();
        self.sync_compaction_clock();
        self.refresh_running_user_shell_output();
    }

    /// Replace disposable facts on the existing operation; never create a row
    /// or let a delayed update resurrect a terminal operation.
    pub fn apply_tool_progress(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        presentation: Box<oc_core::tool_output::Presentation>,
    ) {
        if self.active_turn.as_ref() != Some(turn)
            || !presentation.label_only
            || !presentation.is_valid()
            || presentation.execute.is_none()
        {
            return;
        }
        if let Some(LivePart::Tool { card, .. }) = self.live_parts.iter_mut().find(|part| {
            matches!(part, LivePart::Tool { card, .. }
                if card.op == op && matches!(card.state.as_str(), "started" | "running"))
        }) {
            let card = std::sync::Arc::make_mut(card);
            card.output_presentation = Some(presentation);
            card.render = crate::tools::ToolRender::Inline(crate::tools::InlineRender::Execute);
        }
        self.enforce_parts();
    }

    /// Apply disposable provider snapshots. Raw fragments never enter the
    /// argument parser, diff renderer, durable projection or tool executor.
    pub fn project_pending_approvals(&mut self, requests: &[oc_core::approval::ApprovalRequest]) {
        if requests.is_empty()
            && !self.live_parts.iter().any(|part| {
                matches!(part,
            LivePart::Tool { card, .. } if card.state == "permission_pending")
            })
        {
            return;
        }
        use oc_core::approval::ApprovalPreview;
        let eligible: Vec<_> = requests
            .iter()
            .filter(|r| {
                self.session
                    .as_ref()
                    .is_some_and(|s| s.0 == r.binding.session)
                    && self
                        .active_turn
                        .as_ref()
                        .is_some_and(|t| t.0 == r.binding.turn)
            })
            .collect();
        for request in &eligible {
            let present = self.live_parts.iter().any(|part| matches!(part, LivePart::Tool { card, .. } if card.op == request.binding.operation || card.op.splitn(3, ':').nth(2).and_then(|json| serde_json::from_str::<[String; 2]>(json).ok()).is_some_and(|ids| ids[1] == request.binding.call)));
            if !present && self.live_parts.iter().filter(|p| matches!(p, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending"))).count() < oc_core::tool_stream::PENDING_TOOL_MAX {
                let card = card_from_row(&ToolOpView { child_job: None, question: None, rowid: 0, op: request.binding.operation.clone(), name: request.action.clone(), state: "argument_stream".into(), input: None, output: None, output_bytes: 0, output_truncated: false, output_presentation: None, patch_effects: None, dcp: None, dcp_topic: None });
                self.live_parts.push(LivePart::Tool { card: std::sync::Arc::new(card), input: String::new(), delegation: None });
            }
        }
        for part in &mut self.live_parts {
            let LivePart::Tool {
                card, delegation, ..
            } = part
            else {
                continue;
            };
            if !matches!(
                card.state.as_str(),
                "argument_stream" | "permission_pending"
            ) && delegation.is_none()
            {
                continue;
            }
            let request = eligible.iter().find(|r| {
                card.op == r.binding.operation
                    || card
                        .op
                        .splitn(3, ':')
                        .nth(2)
                        .and_then(|json| serde_json::from_str::<[String; 2]>(json).ok())
                        .is_some_and(|ids| ids[1] == r.binding.call)
            });
            let Some(request) = request else {
                if card.state == "permission_pending" {
                    std::sync::Arc::make_mut(card).state = "argument_stream".into();
                }
                continue;
            };
            // This is a disposable view of owner-prepared bytes, never an execution
            // intent or a parser of unfinished provider argument fragments.
            // Bind the presentation to its prepared operation so a rejection can
            // reconcile without a Started/Linked event (neither may precede consent).
            let card = std::sync::Arc::make_mut(card);
            card.op.clone_from(&request.binding.operation);
            card.state = "permission_pending".into();
            *delegation = None;
            card.input_preview.clear();
            if let ApprovalPreview::Patch {
                files,
                total_files,
                truncated,
            } = &request.preview
            {
                card.files = files
                    .iter()
                    .take(crate::history::CARD_FILES)
                    .map(|file| file.destination.as_ref().unwrap_or(&file.path).clone())
                    .collect();
                card.files_truncated = *truncated || *total_files > card.files.len();
            }
            card.render = match &request.preview {
                ApprovalPreview::Shell { command, cwd } => {
                    crate::tools::ToolRender::Shell(crate::tools::ShellRender {
                        command: command.clone(),
                        cwd: Some(cwd.clone()),
                        ..Default::default()
                    })
                }
                ApprovalPreview::Resource { values } if card.name == "read" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Read {
                        path: values.first().cloned().unwrap_or_default(),
                        range: None,
                    })
                }
                ApprovalPreview::Resource { values } if card.name == "webfetch" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::WebFetch {
                        url: values.first().cloned().unwrap_or_default(),
                    })
                }
                ApprovalPreview::Search { pattern, .. } if card.name == "glob" => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Glob {
                        pattern: pattern.clone(),
                        path: String::new(),
                        matches: None,
                    })
                }
                ApprovalPreview::Search { pattern, .. } => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Grep {
                        pattern: pattern.clone(),
                        path: String::new(),
                        matches: None,
                    })
                }
                _ => crate::tools::ToolRender::Inline(crate::tools::InlineRender::Generic {
                    summary: request
                        .resources
                        .iter()
                        .take(4)
                        .map(|resource| {
                            format!(
                                "resource={}",
                                crate::truncate_utf8(resource, crate::tools::GENERIC_ARG_CHARS)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", "),
                    arguments_limited: request.resources.len() > 4,
                    args: request
                        .resources
                        .iter()
                        .take(4)
                        .map(|resource| ("resource".into(), resource.clone()))
                        .collect(),
                }),
            };
        }
        self.enforce_parts();
    }

    pub fn apply_tool_argument_stream(
        &mut self,
        turn: &WorkerTurnId,
        event: &oc_core::tool_stream::ToolStreamEvent,
    ) {
        use oc_core::tool_stream::{PENDING_TOOL_MAX, ToolStreamEvent};
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let key = |id: &oc_core::tool_stream::ToolStreamIdentity| {
            format!(
                "pending:{}:{}",
                id.round,
                serde_json::json!([id.item_id, id.call_id])
            )
        };
        match event {
            ToolStreamEvent::Pending {
                output_presentation,
                identity,
                name,
                preview,
                ..
            } => {
                if identity.item_id.is_empty()
                    || identity.call_id.is_empty()
                    || identity.item_id.len() > 512
                    || identity.call_id.len() > 512
                    || name.is_empty()
                    || name.len() > 512
                {
                    return;
                }
                if identity.round < self.pending_tool_round {
                    return;
                }
                if identity.round > self.pending_tool_round {
                    // Round-local provider drafts can expire. Owner-prepared
                    // waits recovered ahead of this broadcast remain authoritative.
                    for part in &mut self.live_parts {
                        if matches!(part, LivePart::Tool { card, delegation, .. } if card.state == "argument_stream" || delegation.is_some())
                        {
                            *part = LivePart::Vacant;
                        }
                    }
                    self.pending_tool_seen.clear();
                    self.pending_tool_round = identity.round;
                    self.invalidate_transcript();
                }
                let op = key(identity);
                // A queue snapshot can recover the prepared card before older
                // broadcast argument snapshots are drained. Never replace it
                // with unfinished bytes or create a second card for that call.
                if self.session.as_ref().and_then(|session| self.approvals.request_for_call(&session.0, &turn.0, &identity.call_id)).is_some_and(|request| self.live_parts.iter().any(|part| matches!(part, LivePart::Tool { card, .. } if card.state == "permission_pending" && (card.op == request.binding.operation || card.op.splitn(3, ':').nth(2).and_then(|json| serde_json::from_str::<[String; 2]>(json).ok()).is_some_and(|ids| ids[1] == identity.call_id))))) { return; }
                let found = self.live_parts.iter().position(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op && card.state == "argument_stream"));
                if found.is_none()
                    && (self.pending_tool_seen.contains(&op)
                        || self.pending_tool_seen.len() >= PENDING_TOOL_MAX)
                {
                    return;
                }
                let mut card = card_from_row(&ToolOpView {
                    child_job: None,
                    question: None,
                    rowid: 0,
                    op: op.clone(),
                    name: name.clone(),
                    state: "argument_stream".into(),
                    input: None,
                    output: None,
                    output_bytes: 0,
                    output_truncated: false,
                    output_presentation: output_presentation
                        .clone()
                        .filter(|p| p.label_only && p.is_valid()),
                    patch_effects: None,
                    dcp: None,
                    dcp_topic: None,
                });
                // The name-derived kind has no arguments (`input: None`), like
                // upstream ToolPart's empty streaming input (`index.tsx:2416`).
                // Bounded raw prefix is presentation text only, not input JSON.
                let mut end = preview
                    .len()
                    .min(oc_core::tool_stream::ARGUMENT_PREVIEW_MAX);
                while !preview.is_char_boundary(end) {
                    end -= 1;
                }
                card.input_preview = preview[..end].to_string();
                let delegation =
                    matches!(card.render, crate::tools::ToolRender::Subagent(_)).then(|| {
                        found
                            .and_then(|index| match &self.live_parts[index] {
                                LivePart::Tool { delegation, .. } => delegation.clone(),
                                _ => None,
                            })
                            .unwrap_or(DelegationSpinner {
                                round: identity.round,
                                frame: 0,
                                at: None,
                            })
                    });
                let part = LivePart::Tool {
                    card: std::sync::Arc::new(card),
                    input: String::new(),
                    delegation,
                };
                if let Some(index) = found {
                    self.live_parts[index] = part;
                } else {
                    self.freeze_reasoning();
                    self.freeze_text();
                    self.pending_tool_seen.push(op);
                    self.live_parts.push(part);
                }
                self.enforce_parts();
            }
            ToolStreamEvent::Linked { identity, op } => {
                let key = key(identity);
                let prefix = format!("pending:{}:", identity.round);
                let mut changed = false;
                for part in &mut self.live_parts {
                    let LivePart::Tool { card, .. } = part else {
                        continue;
                    };
                    if !matches!(
                        card.state.as_str(),
                        "argument_stream" | "permission_pending"
                    ) {
                        continue;
                    }
                    if card.op == key {
                        changed = true;
                        if let Some(detail) = self.execute_detail.as_mut()
                            && detail.operation == key
                        {
                            detail.operation.clone_from(op);
                        }
                        // A streaming exploration group is keyed by its first
                        // operation; admission must not collapse it.
                        if self.exploration_expanded.remove(&key) {
                            self.exploration_expanded.insert(op.clone());
                        }
                        if let Some(clock) = self.tool_spinner_clocks.remove(&key) {
                            self.tool_spinner_clocks.entry(op.clone()).or_insert(clock);
                        }
                        let card = std::sync::Arc::make_mut(card);
                        card.op.clone_from(op);
                        card.state = "started".into();
                        card.input_preview.clear();
                    } else if card
                        .op
                        .strip_prefix(&prefix)
                        .and_then(|json| serde_json::from_str::<[String; 2]>(json).ok())
                        .is_some_and(|ids| ids[1] == identity.call_id)
                    {
                        // A canonical call supersedes conflicting announcements;
                        // never link the wrong provider item or show both cards.
                        *part = LivePart::Vacant;
                        changed = true;
                    }
                }
                if changed {
                    self.invalidate_transcript();
                }
            }
            ToolStreamEvent::Clear { round } => {
                let prefix = format!("pending:{round}:");
                let mut changed = false;
                for part in &mut self.live_parts {
                    if matches!(part, LivePart::Tool { card, delegation, .. } if delegation.as_ref().is_some_and(|clock| clock.round == *round) || matches!(card.state.as_str(), "argument_stream" | "permission_pending") && card.op.starts_with(&prefix))
                    {
                        *part = LivePart::Vacant;
                        changed = true;
                    }
                }
                self.prune_reasoning();
                if changed {
                    self.invalidate_transcript();
                }
            }
        }
    }

    fn discard_pending_tools(&mut self) {
        for part in &mut self.live_parts {
            if matches!(part, LivePart::Tool { card, delegation, .. } if delegation.is_some() || matches!(card.state.as_str(), "argument_stream" | "permission_pending"))
            {
                *part = LivePart::Vacant;
            }
        }
    }

    /// Apply a recorded tool-call outcome: rebuild the matching card from the
    /// stored input plus the outcome, then drop the transient input.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
    ) {
        self.apply_tool_finished_with_effects(
            turn,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            None,
        );
    }

    /// Apply the owner's bounded result-derived mutation projection.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished_with_effects(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
        patch_effects: Option<oc_core::patch::PatchEffects>,
    ) {
        self.apply_tool_finished_with_presentation(
            turn,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            patch_effects,
            None,
            None,
        );
    }

    /// The same frozen DCP metadata as durable history, attached to one operation.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished_with_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
        patch_effects: Option<oc_core::patch::PatchEffects>,
        dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
        question: Option<oc_core::question::QuestionResult>,
    ) {
        self.apply_tool_finished_with_output_presentation(
            turn,
            op,
            name,
            state,
            output,
            output_bytes,
            output_truncated,
            patch_effects,
            dcp,
            question,
            None,
        );
    }

    /// Operation-owned body/guidance facts; absent legacy provenance stays unknown.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_tool_finished_with_output_presentation(
        &mut self,
        turn: &WorkerTurnId,
        op: &str,
        name: &str,
        state: &str,
        output: &str,
        output_bytes: i64,
        output_truncated: bool,
        patch_effects: Option<oc_core::patch::PatchEffects>,
        dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
        question: Option<oc_core::question::QuestionResult>,
        output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
    ) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        let dcp = dcp.filter(|run| {
            run.operation_id == op && self.session.as_ref().is_some_and(|id| id.0 == run.session)
        });
        let already_confirmed = self.live_parts.iter().any(|part| matches!(part,
            LivePart::Tool { card, .. } if card.op == op && matches!(&card.render, crate::tools::ToolRender::Dcp(view) if view.snapshot.is_some())));
        if already_confirmed {
            return;
        }
        if state == "completed"
            && let Some(notice) = dcp
                .as_ref()
                .and_then(|run| crate::dcp_view::toast_text(run, &self.chrome.dcp))
        {
            self.push_transient_note(&notice, NoteVariant::Info);
        }
        let outcome = ToolOpView {
            child_job: None,
            question,
            rowid: 0,
            op: op.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: None,
            output: Some(output.to_string()),
            output_bytes,
            output_truncated,
            output_presentation,
            patch_effects,
            dcp,
            dcp_topic: None,
        };
        if let Some(LivePart::Tool {
            card,
            input,
            delegation,
        }) = self
            .live_parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            let mut row = outcome;
            row.name = card.name.clone();
            row.input = Some(std::mem::take(input));
            row.child_job = card.child_job.clone();
            if let crate::tools::ToolRender::Dcp(view) = &card.render {
                // Renewal completes without a run snapshot; keep the typed topic
                // from Started instead of reconstructing it from raw UI input.
                row.dcp_topic = view.topic.clone();
            }
            if let Some(label) = card
                .output_presentation
                .as_ref()
                .filter(|facts| facts.is_valid())
                .and_then(|facts| facts.mcp_label.clone())
            {
                // An explicit result presentation may deliberately omit the
                // optional name to fit its operation-owned event budget.
                if row.output_presentation.is_none() {
                    row.output_presentation =
                        Some(Box::new(oc_core::tool_output::Presentation::label(label)));
                }
            }
            let mut finished = card_from_row(&row);
            finished.started_at = card.started_at;
            if let crate::tools::ToolRender::Dcp(view) = &mut finished.render {
                view.color_index = self.live_agent_color_index;
            }
            // Reject never emits Started, so its transient card may have only
            // owner-prepared targets rather than canonical arguments. Retain
            // those labels, not a proposed diff or a claim of applied effects.
            if card.name == "apply_patch"
                && finished.files.is_empty()
                && matches!(state, "denied" | "cancelled")
                && crate::history::permission_output(output).is_some()
                && finished
                    .patch_effects
                    .as_ref()
                    .is_none_or(|effects| effects.files.is_empty())
            {
                finished.files = card.files.clone();
                finished.files_truncated = card.files_truncated;
            }
            *card = std::sync::Arc::new(finished);
            *delegation = None;
            self.enforce_parts();
            self.sync_compaction_clock();
            self.refresh_running_user_shell_output();
            return;
        }
        // The intent event was not observed (e.g. a late subscription): the
        // card appears with the outcome only, never with an invented input.
        let mut card = card_from_row(&outcome);
        if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
            view.color_index = self.live_agent_color_index;
        }
        self.live_parts.push(LivePart::Tool {
            card: std::sync::Arc::new(card),
            input: String::new(),
            delegation: None,
        });
        self.enforce_parts();
        self.sync_compaction_clock();
        self.refresh_running_user_shell_output();
    }

    /// Evict oldest live parts while the count or byte cap is exceeded; the
    /// parts are transient (a reload restores committed history).
    pub(super) fn enforce_parts(&mut self) {
        let _ = self.execute_capture();
        self.invalidate_transcript();
        while self.live_parts.len() > LIVE_PARTS_MAX
            || self
                .live_parts
                .iter()
                .map(LivePart::retained_bytes)
                .sum::<usize>()
                > WINDOW_BYTES
        {
            self.live_parts.remove(0);
            self.live_part_sources.remove(&self.live_part_offset);
            self.live_part_offset += 1;
            self.live_preview_truncated = true;
        }
        self.prune_reasoning();
    }

    /// Footer metadata for the finished turn from real state: the effective
    /// model label, the measured turn duration, provider usage and the
    /// interrupt marker. Absent data stays `None` (the footer omits it).
    fn finish_meta(&mut self, interrupted: bool, duration_ms: u64) -> AssistantMeta {
        let model = self.live_model_label.take().or_else(|| {
            self.picker.as_ref().and_then(|picker| {
                let selection = picker.selection()?;
                Some(
                    selection
                        .entry
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&selection.id)
                        .to_string(),
                )
            })
        });
        let usage = self.turn_usage.take();
        let context = self.turn_context_usage.take();
        AssistantMeta {
            model,
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            input_tokens: usage.map(|usage| usage.input_tokens),
            output_tokens: usage.map(|usage| usage.output_tokens),
            context_usage: context.map(|(input, output, _)| (input, output)),
            context_usage_limit: context.map(|(_, _, limit)| limit),
            streamed_ms: usage.map(|usage| usage.streamed_ms),
            session_tps: None,
            interrupted,
            status: self.live_terminal_status.take(),
            agent_color_index: self.live_agent_color_index,
            preview_limited: self.live_preview_truncated,
        }
    }

    /// Completed reasoning block for the turn, with its measured duration
    /// (`part.time.completed - part.time.created` when text followed, else the
    /// elapsed reasoning window).
    fn take_reasoning(&mut self) -> Option<ReasoningBlock> {
        if self.live_reasoning.is_empty() {
            return None;
        }
        let duration_ms = match (
            self.reasoning_started.take(),
            self.reasoning_finished.take(),
        ) {
            (Some(started), Some(finished)) => {
                Some(finished.saturating_duration_since(started).as_millis() as u64)
            }
            _ => None,
        };
        Some(ReasoningBlock {
            text: std::mem::take(&mut self.live_reasoning),
            duration_ms,
            running: false,
            expanded: false,
            toggleable: true,
            spinner: None,
            identity: Some(crate::messages::ReasoningIdentity::Live(
                self.reasoning_epoch,
                self.live_part_offset + self.live_parts.len(),
            )),
        })
    }
}
