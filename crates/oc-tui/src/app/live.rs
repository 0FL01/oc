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
        models.insert(
            entry.id.clone(),
            serde_json::json!({
                "name": if entry.display_name.is_empty() { &entry.id } else { &entry.display_name },
                "provider_name": if entry.provider_name.is_empty() { &snapshot.provider } else { &entry.provider_name },
                "cost": entry.price.as_ref().map(|p|serde_json::json!({"input":p.input,"output":p.output})),
                "limit": { "context": entry.context_known.then_some(entry.context), "output": entry.output_known.then_some(entry.output) },
                "variants": variants,
            }),
        );
    }
    ModelCatalog {
        provider: snapshot.provider.clone(),
        models,
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
                Ok(Ok(CoreEvent::SessionModelSelected { session, commit })) => {
                    state.apply_session_model_selected(&session, &commit)
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
                    turn,
                    op,
                    name,
                    input,
                    dcp_topic,
                    ..
                })) => {
                    state
                        .apply_tool_started_with_presentation(&turn, &op, &name, &input, dcp_topic);
                    if name == "subagent"
                        && state.active_turn() == Some(&turn)
                        && let Some(parent) = state.attached_session().cloned()
                        && let Ok(jobs) = state.app.child_jobs(parent).await
                    {
                        state.apply_child_jobs(jobs);
                    }
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
            *current = snapshot;
        } else {
            self.compactions.push(snapshot);
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
        self.chrome = snapshot.chrome.clone();
        if dialog_changed {
            self.select.clear_action_focus();
            self.mcp_action_down = None;
        }
        self.auto_accept = snapshot.auto_accept;
        let mut picker = ModelPicker::new(catalog_from_snapshot(&snapshot));
        let draft = self.composer_catalog(&snapshot);
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
            picker.load_persisted_raw(Some(&serde_json::to_string(&draft).expect("model ref")));
            picker.focus_id(&draft.id);
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
        self.reconcile_service_feedback(previous_issues);
        self.picker = Some(picker);
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
        self.agents_cursor = snapshot
            .agent_id
            .as_ref()
            .and_then(|id| self.agents.iter().position(|agent| &agent.id == id))
            .unwrap_or(usize::MAX);
        self.catalog_loaded = true;
        self.sync_modal_cursor();
        self.refresh_subagent_cards();
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
        self.transcript_rows()
            .into_iter()
            .rev()
            .filter_map(|row| row.tool)
            .flat_map(|card| {
                let crate::tools::ToolRender::Dcp(view) = card.render else {
                    return Vec::new();
                };
                let Some(run) = view.snapshot else {
                    return Vec::new();
                };
                run.block_ids
                    .into_iter()
                    .enumerate()
                    .filter(|(_, id)| !view.summaries.iter().any(|page| &page.block_id == id))
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
                && let crate::tools::ToolRender::Dcp(view) = &mut card.render
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
        self.reasoning_down = None;
        self.reasoning_started = None;
        self.reasoning_finished = None;
        self.turn_usage = None;
        self.scroll = 0;
        self.wheel_motion = None;
    }

    /// Whether the active turn was accepted from a manual compression request.
    pub fn is_compress_turn(&self, turn: &WorkerTurnId) -> bool {
        self.compress_turn.as_ref() == Some(turn) && self.active_turn.as_ref() == Some(turn)
    }

    /// Enqueue manual compression using the same draft/receipt lifecycle as text.
    pub fn request_compress(&mut self, focus: String) -> Result<(), CoreError> {
        if self.is_busy() {
            return Err(CoreError::TurnBusy);
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
        self.begin_submission(receipt, session, false, true);
        Ok(())
    }

    pub(super) fn begin_submission(
        &mut self,
        receipt: SubmissionReceipt,
        session: SessionId,
        fresh: bool,
        compress: bool,
    ) {
        self.request_id += 1;
        let agent = self.active_agent.clone();
        let agent_color_index = agent.as_deref().and_then(|agent| {
            self.agents
                .iter()
                .find(|entry| entry.id == agent)
                .map(|entry| entry.color_index)
        });
        self.pending = Some(PendingSubmission {
            selection: self.captured_model_commit(),
            request_id: self.request_id,
            generation: self.generation,
            session,
            fresh,
            draft: self.input.clone(),
            mentions: self.editor.submitted_mentions(&self.input),
            revision: self.input_revision,
            receipt,
            agent,
            agent_color_index,
            cancelling: false,
            compress,
        });
        self.status = TuiStatus::PendingSubmission;
        self.push_note("submission pending; Esc to cancel");
    }

    /// Reconcile the unique acceptance receipt before applying queued turn
    /// events. Failure leaves the editable draft intact. No worker is spawned.
    pub fn poll_submission(&mut self) {
        self.poll_model_commits();
        self.poll_user_shell();
        let Some(result) = self.pending.as_mut().and_then(|p| p.receipt.try_result()) else {
            return;
        };
        let pending = self.pending.take().expect("polled receipt");
        self.reconcile_submission(pending, result, false);
    }

    /// Quit must not discard an in-flight Home root after the owner commits
    /// it. Cancel through the owner first (so tool work is stopped safely),
    /// then resolve the *same* receipt before the application shuts down.
    /// Existing-session turns already have a durable tab and need no wait.
    pub async fn reconcile_fresh_quit(&mut self) -> Result<(), CoreError> {
        self.reconcile_user_shell_quit().await?;
        let Some(pending) = self.pending.as_mut().filter(|p| p.fresh) else {
            return Ok(());
        };
        pending.cancelling = true;
        let session = pending.session.clone();
        // Cancel follows SubmitFresh in the owner's inbox. A rejected or
        // already-finished turn has nothing left to cancel.
        match self.app.cancel(session).await {
            Ok(()) | Err(CoreError::TurnNotActive) => {}
            Err(error) => return Err(error),
        }
        let result = self
            .pending
            .as_mut()
            .expect("fresh receipt still pending")
            .receipt
            .wait()
            .await;
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
                if pending.fresh {
                    self.session = Some(pending.session.clone());
                }
                self.model_submission_accepted(&pending.session, pending.selection.as_ref());
                self.live_preview_truncated = false;
                self.live_part_states.clear();
                self.live_terminal_status = None;
                self.live_model_label = None;
                self.live_agent_color_index = None;
                if !pending.compress {
                    self.home = false;
                    self.editor
                        .accepted_mentions(pending.draft.trim(), pending.mentions);
                    self.window.push_synthetic(
                        "user",
                        pending.draft.trim(),
                        pending.agent,
                        pending.agent_color_index,
                    );
                    self.prune_reasoning();
                }
                self.compress_turn = pending.compress.then(|| turn.clone());
                if !pending.compress {
                    self.conversation_available = Some((true, false));
                }
                self.live_text.clear();
                self.live_reasoning.clear();
                self.live_parts.clear();
                self.pending_tool_seen.clear();
                self.pending_tool_round = 0;
                self.live_part_offset = 0;
                self.reasoning_down = None;
                self.reasoning_started = None;
                self.reasoning_finished = None;
                self.turn_usage = None;
                self.live_retry = None;
                self.live_retry_seen = None;
                self.retry_due = None;
                self.live_span = None;
                self.live_projection_revision = None;
                self.active_turn = Some(turn);
                self.reasoning_epoch = self.reasoning_epoch.wrapping_add(1);
                if !exiting {
                    self.reset_scanner();
                    self.status = TuiStatus::Streaming;
                }
                self.scroll = 0;
                self.wheel_motion = None;
                if self.input_revision == pending.revision && !pending.cancelling {
                    self.input.clear();
                    self.editor.clear_submitted_draft();
                }
                self.dcp.clear_notice();
                self.note = None;
                self.refused_submission = None;
            }
            Err(error) => {
                if !exiting {
                    self.status = TuiStatus::Idle;
                }
                let note = self.submission_note(&error);
                self.push_note(&note);
            }
        }
    }

    // ---- worker events --------------------------------------------------

    /// Observe an owner-accepted child without admitting synthetic user input.
    pub fn begin_linked_turn(&mut self, turn: WorkerTurnId) {
        if self.active_turn.as_ref() == Some(&turn) {
            return;
        }
        self.active_turn = Some(turn);
        self.status = TuiStatus::Streaming;
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
        self.window
            .insert_before_live_user(crate::history::model_switch_text(notice));
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
            self.invalidate_transcript();
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
        }
        if self.reasoning_started.is_none() {
            self.reasoning_started = Some(Instant::now());
        }
        if self.live_reasoning.len() < WINDOW_BYTES {
            let room = WINDOW_BYTES - self.live_reasoning.len();
            self.live_reasoning
                .push_str(crate::truncate_utf8(delta, room));
            self.invalidate_transcript();
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
        self.live_part_states = projection.part_states.clone();
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
        self.live_terminal_status = Some(projection.status.clone());
        self.live_model_label = projection
            .spans
            .last()
            .and_then(|span| span.request.as_ref())
            .map(|request| request.model_label.clone())
            .or_else(|| {
                (!projection.model_label.is_empty()).then(|| projection.model_label.clone())
            });
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
        self.invalidate_transcript();
        let meta = self.finish_meta(false, duration_ms);
        self.reset_scanner();
        if !self.live_reasoning.is_empty() && self.reasoning_finished.is_none() {
            self.reasoning_finished = Some(Instant::now());
        }
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Idle;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: text.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
            self.prune_reasoning();
            return;
        }
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.prune_reasoning();
    }

    /// Apply a worker turn-interrupted event: keep the partial text (never
    /// committed to storage), mark the footer `interrupted`, and release the
    /// turn.
    pub fn apply_interrupted(&mut self, turn: &WorkerTurnId, partial: &str, duration_ms: u64) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
        self.invalidate_transcript();
        let meta = self.finish_meta(true, duration_ms);
        self.reset_scanner();
        let reasoning = self.take_reasoning();
        self.active_turn = None;
        self.status = TuiStatus::Cancelled;
        if self.live_parts.is_empty() {
            self.live_text.clear();
            self.live_reasoning.clear();
            self.reasoning_started = None;
            self.reasoning_finished = None;
            self.turn_usage = None;
            self.window.push_row(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: partial.to_string(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning,
                meta: Some(meta),
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
            self.prune_reasoning();
            return;
        }
        // The partial text is already the frozen trailing segment.
        let parts = self.commit_live_parts(reasoning);
        self.push_committed_parts(parts);
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.prune_reasoning();
    }

    /// Release a failed turn and show its error, never a successful answer.
    pub fn apply_failed(&mut self, turn: &WorkerTurnId, error: &CoreError) {
        if Some(turn) != self.active_turn.as_ref() {
            return;
        }
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
        if !parts.is_empty() {
            // Cards already shown stay visible; the error follows them.
            self.push_committed_parts(parts);
        }
        self.window
            .push_row(footer_row(self.active_agent.clone(), meta));
        self.window
            .push_synthetic("", &format!("(error: {error})"), None, None);
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
    fn push_committed_parts(&mut self, parts: Vec<LivePart>) {
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
            if self.live_mixed_models
                && row.tool.is_none()
                && let Some(model) = self
                    .live_part_states
                    .iter()
                    .find(|state| state.sequence == self.live_part_offset + ordinal)
                    .and_then(|state| state.model_label.clone())
            {
                row.meta = Some(AssistantMeta {
                    model: Some(model),
                    ..Default::default()
                });
            }
            self.window.push_row(row);
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
            output_presentation: None,
            patch_effects: None,
            dcp: None,
            dcp_topic,
        });
        if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
            view.color_index = self.live_agent_color_index;
        }
        if let Some(part) = self
            .live_parts
            .iter_mut()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            *part = LivePart::Tool {
                card: Box::new(card),
                input: input.to_string(),
            };
            self.enforce_parts();
            self.sync_compaction_clock();
            self.refresh_running_user_shell_output();
            return;
        }
        self.live_parts.push(LivePart::Tool {
            card: Box::new(card),
            input: input.to_string(),
        });
        self.enforce_parts();
        self.sync_compaction_clock();
        self.refresh_running_user_shell_output();
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
                self.live_parts.push(LivePart::Tool { card: Box::new(card), input: String::new() });
            }
        }
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
                    card.state = "argument_stream".into();
                }
                continue;
            };
            // This is a disposable view of owner-prepared bytes, never an execution
            // intent or a parser of unfinished provider argument fragments.
            // Bind the presentation to its prepared operation so a rejection can
            // reconcile without a Started/Linked event (neither may precede consent).
            card.op.clone_from(&request.binding.operation);
            card.state = "permission_pending".into();
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
                        matches: None,
                    })
                }
                ApprovalPreview::Search { pattern, .. } => {
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Grep {
                        pattern: pattern.clone(),
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
                        if matches!(part, LivePart::Tool { card, .. } if card.state == "argument_stream")
                        {
                            *part = LivePart::Vacant;
                        }
                    }
                    self.pending_tool_seen.clear();
                    self.pending_tool_round = identity.round;
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
                    output_presentation: None,
                    patch_effects: None,
                    dcp: None,
                    dcp_topic: None,
                });
                card.render =
                    crate::tools::ToolRender::Inline(crate::tools::InlineRender::Generic {
                        args: Vec::new(),
                        summary: String::new(),
                        arguments_limited: false,
                    });
                // Bounded raw prefix is presentation text only, not input JSON.
                let mut end = preview
                    .len()
                    .min(oc_core::tool_stream::ARGUMENT_PREVIEW_MAX);
                while !preview.is_char_boundary(end) {
                    end -= 1;
                }
                card.input_preview = preview[..end].to_string();
                let part = LivePart::Tool {
                    card: Box::new(card),
                    input: String::new(),
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
                    }
                }
            }
            ToolStreamEvent::Clear { round } => {
                let prefix = format!("pending:{round}:");
                for part in &mut self.live_parts {
                    if matches!(part, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending") && card.op.starts_with(&prefix))
                    {
                        *part = LivePart::Vacant;
                    }
                }
                self.prune_reasoning();
            }
        }
    }

    fn discard_pending_tools(&mut self) {
        for part in &mut self.live_parts {
            if matches!(part, LivePart::Tool { card, .. } if matches!(card.state.as_str(), "argument_stream" | "permission_pending"))
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
        if let Some(LivePart::Tool { card, input }) = self
            .live_parts
            .iter_mut()
            .rev()
            .find(|part| matches!(part, LivePart::Tool { card, .. } if card.op == op))
        {
            let mut row = outcome;
            row.name = card.name.clone();
            row.input = Some(std::mem::take(input));
            row.child_job = card.child_job.clone();
            let mut finished = card_from_row(&row);
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
            **card = finished;
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
            card: Box::new(card),
            input: String::new(),
        });
        self.enforce_parts();
        self.sync_compaction_clock();
        self.refresh_running_user_shell_output();
    }

    /// Evict oldest live parts while the count or byte cap is exceeded; the
    /// parts are transient (a reload restores committed history).
    pub(super) fn enforce_parts(&mut self) {
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
        AssistantMeta {
            model,
            duration_ms: (duration_ms > 0).then_some(duration_ms),
            input_tokens: usage.map(|usage| usage.input_tokens),
            output_tokens: usage.map(|usage| usage.output_tokens),
            context_usage: None,
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
            identity: Some(crate::messages::ReasoningIdentity::Live(
                self.reasoning_epoch,
                self.live_part_offset + self.live_parts.len(),
            )),
        })
    }
}
