//! History projection, viewport/selection hit tests and transcript copy.

use super::*;

pub(super) struct ChildNoticePress {
    target: crate::messages::CapturedChildTarget,
    frame: Rect,
    generation: u64,
    point: (u16, u16),
}

/// One bounded render row for a tool card.
pub(super) fn card_row(card: &ToolCard) -> HistoryRow {
    // `apply_patch` shows a bounded diff (touched files with +/- counts and
    // hunk counts) instead of the raw patch bytes; other tools keep the
    // parsed path list. Never a second copy of a large payload.
    let files = if let Some(effects) = &card.patch_effects {
        let listed = effects
            .files
            .iter()
            .take(crate::history::CARD_FILES)
            .map(|file| {
                let mut text = format!(
                    "{} +{} -{}",
                    file.destination.as_ref().unwrap_or(&file.path),
                    file.additions,
                    file.deletions
                );
                if !file.hunks.is_empty() {
                    text.push_str(&format!(
                        " ({}{}h{})",
                        if file.truncated { "≥" } else { "" },
                        file.hunks.len(),
                        if file.truncated { " preview" } else { "" }
                    ));
                }
                text
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            " [{listed}{}] confirmed {}f +{} -{}",
            if effects.truncated { ", …" } else { "" },
            effects.total_files,
            effects.additions,
            effects.deletions
        )
    } else {
        match &card.diff {
            Some(diff) => {
                let listed = diff
                    .files
                    .iter()
                    .take(crate::history::CARD_FILES)
                    .map(|file| {
                        let marker = match file.change {
                            "Add" => "+",
                            "Delete" => "-",
                            _ => "~",
                        };
                        let mut text = format!(
                            "{marker}{} +{} -{}",
                            file.path, file.additions, file.removals
                        );
                        if file.hunks > 0 {
                            text.push_str(&format!(" ({}h)", file.hunks));
                        }
                        if let Some(target) = &file.move_to {
                            text.push_str(&format!(" -> {target}"));
                        }
                        text
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let suffix = if diff.truncated || diff.files.len() > crate::history::CARD_FILES {
                    ", …"
                } else {
                    ""
                };
                format!(
                    " [{listed}{suffix}] request preview (not confirmed) {}f +{} -{}",
                    diff.files.len(),
                    diff.additions,
                    diff.removals
                )
            }
            None if card.files.is_empty() => String::new(),
            None => {
                let suffix = if card.files_truncated { ", …" } else { "" };
                format!(" [{}{suffix}]", card.files.join(", "))
            }
        }
    };
    let output = if card.output_preview.is_empty() {
        String::new()
    } else if card.output_presentation.is_none() && card.output_truncated {
        format!(
            " -> {}…[+{} bytes stored]",
            card.output_preview,
            card.output_bytes.max(0)
        )
    } else {
        format!(" -> {}", card.output_preview)
    };
    let facts = card
        .output_presentation
        .as_ref()
        .map_or_else(String::new, |presentation| {
            let details = crate::tools::presentation_details(presentation);
            if details.is_empty() {
                String::new()
            } else {
                format!("\n{}", details.join("\n"))
            }
        });
    // Diff first: a long operation id must never push the diff off a narrow
    // panel row.
    HistoryRow {
        message_id: None,
        seq: i64::MAX,
        role: String::new(),
        text: format!(
            "{} {}{}{} ({}){}",
            card.name, card.state, files, output, card.op, facts
        ),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        // The list text may ellipsize; the existing detail viewer still needs
        // bounded owner facts without another query or a parallel store.
        tool: Some(card.clone()),
        child_notice: None,
        shell_notice: None,
    }
}

/// Assistant footer row: no text, no reasoning, footer metadata only; the
/// upstream footer follows every part of the assistant message
/// (`routes/session/index.tsx:1934-1985`).
pub(super) fn footer_row(agent: Option<String>, meta: AssistantMeta) -> HistoryRow {
    HistoryRow {
        message_id: None,
        seq: i64::MAX,
        role: "assistant".to_string(),
        text: String::new(),
        agent,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: Some(meta),
        tool: None,
        child_notice: None,
        shell_notice: None,
    }
}

impl TuiState {
    /// An existing card consumes child inventory even with its composer closed.
    /// This predicate never creates a model graph part or navigation authority.
    pub fn has_subagent_cards(&self) -> bool {
        self.window.rows().iter().filter_map(|row|row.tool.as_ref())
            .any(|card|matches!(card.render,crate::tools::ToolRender::Subagent(_)))
            || self.live_parts.iter().any(|part|matches!(part,LivePart::Tool{card,..} if matches!(card.render,crate::tools::ToolRender::Subagent(_))))
    }
    pub(crate) fn set_detail_area(&self, area: ratatui::layout::Rect) {
        self.detail_area.set(area);
    }

    /// Last actual frame geometry, shared with the binary's PTY resize consumer.
    pub fn detail_area(&self) -> ratatui::layout::Rect {
        self.detail_area.get()
    }

    pub(crate) fn card_scroll(&self) -> usize {
        self.card_scroll
    }

    pub(crate) fn card_seen(&self) -> usize {
        self.card_seen.get()
    }

    /// Only rows actually painted in a frame count as accessible. An End key
    /// or repeated key events without drawing cannot skip a result window.
    pub(crate) fn card_rows_painted(&self, start: usize, end: usize) {
        if start <= self.card_seen.get() {
            self.card_seen.set(self.card_seen.get().max(end));
        }
    }
    /// Transcript scroll offset in rows (0 = pinned to the newest row).
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Effective offset in the last drawn viewport; requested offset survives resize.
    pub fn display_scroll(&self) -> usize {
        self.viewport
            .get()
            .filter(|view| view.requested_scroll == self.scroll)
            .map_or_else(
                || self.scroll.min(self.max_scroll()),
                |view| view.displayed_scroll,
            )
    }

    /// Newest page becomes the whole window; scroll pins to the newest row.
    pub fn attach_page(&mut self, page: &HistoryPage) {
        if let Some(job) = page.child_job.as_deref()
            && self.attached_session() == Some(&job.child)
        {
            self.refresh_linked_child(job.clone());
        }
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.completion_anchor.get_mut().take();
        self.reverted = page.reverted.clone();
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.exploration_expanded.clear();
        self.reasoning_down = None;
        self.reasoning_expanded.clear();
        self.viewport.set(None);
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.window.reset(page);
        self.refresh_running_user_shell_output();
        self.scroll = 0;
        self.wheel_motion = None;
    }

    /// Native command notices can settle during an assistant stream. Update
    /// only their owner-projected records, preserving its live overlay/draft.
    pub fn refresh_user_shell_page(&mut self, page: &HistoryPage) {
        if self.window.refresh_user_shell(page) {
            self.invalidate_transcript();
        }
        self.refresh_running_user_shell_output();
    }

    /// A child completion may arrive while another parent request streams.
    /// Keep that request's parts/draft and the window's paging boundaries.
    pub fn refresh_child_notice_page(&mut self, page: &HistoryPage) {
        if let Some(turn) = &self.active_turn {
            self.window.correlate_live_prompt(page, &turn.0);
        }
        if self.window.refresh_owner_notices(page) {
            self.invalidate_transcript();
        }
    }

    /// The existing inventory can arrive before a receipt/history attachment.
    pub fn has_model_shell_card(&self, operation: &str) -> bool {
        self.window.rows().iter().any(|row| {
            row.tool
                .as_ref()
                .is_some_and(|card| card.is_model_shell(operation))
        }) || self.live_parts.iter().any(
            |part| matches!(part, LivePart::Tool { card, .. } if card.is_model_shell(operation)),
        )
    }

    /// One terminal owner receipt flushes an already-known model card. It does
    /// not attach provider parts or retarget the captured operation.
    pub fn apply_model_shell_snapshot(&mut self, snapshot: &oc_core::queries::ShellSnapshot) {
        if self.session.as_ref() != Some(&snapshot.job.session) {
            return;
        }
        let mut changed = self.window.finish_model_shell_preview(snapshot);
        for part in &mut self.live_parts {
            if let LivePart::Tool { card, .. } = part
                && self
                    .active_turn
                    .as_ref()
                    .is_some_and(|turn| turn.0 == snapshot.job.turn)
            {
                changed |= card.finish_model_shell_preview(snapshot);
            }
        }
        if changed {
            self.enforce_parts();
        }
    }

    /// The existing inventory can arrive before a receipt/history attachment.
    /// Update only existing user inputs and operation-associated model cards;
    /// neither path creates another executing model part.
    pub(crate) fn refresh_running_user_shell_output(&mut self) {
        if let Some(session) = &self.session {
            let mut changed = self.shells.refresh_transcript(session, &mut self.window);
            for part in &mut self.live_parts {
                if let LivePart::Tool { card, .. } = part {
                    changed |= self.shells.refresh_live_card(
                        session,
                        self.active_turn.as_ref().map(|turn| turn.0.as_str()),
                        card,
                    );
                }
            }
            if changed {
                self.enforce_parts();
            }
        }
        self.refresh_subagent_cards();
    }

    /// Same-session TurnFinished receipt only. Explicit routing/conversation
    pub(crate) fn refresh_subagent_cards(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let mut changed =
            self.children
                .refresh_transcript(session, &mut self.window, self.picker.as_ref());
        if self.active_turn.is_some() {
            for part in &mut self.live_parts {
                if let LivePart::Tool { card, .. } = part {
                    changed |= self
                        .children
                        .refresh_live_card(session, card, self.picker.as_ref());
                }
            }
        }
        if changed {
            self.enforce_parts();
        }
    }

    /// Same-session TurnFinished receipt only. Explicit routing/conversation
    /// resets use attach_page. Preserve a painted part, not a total-row delta:
    /// durable projection may replace synthetic parts and the paging window.
    pub fn refresh_completed_page(&mut self, page: &HistoryPage) {
        if let Some(job) = page.child_job.as_deref()
            && self.attached_session() == Some(&job.child)
        {
            self.refresh_linked_child(job.clone());
        }
        self.remember_compaction_turns(page);
        let view = self.viewport.get().filter(|_| self.scroll > 0);
        let old_rows = self.transcript_rows();
        let mut anchor = None;
        if let Some(view) = view {
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            self.transcript_part_positions(
                &old_rows,
                (view.width, view.terminal_width),
                |index, start, end| {
                    if start <= top && top < end {
                        anchor = Some((index, top - start));
                    }
                },
            );
        }
        self.reverted = page.reverted.clone();
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.clear_transcript_selection();
        self.window.refresh_completed(page, self.scroll > 0);
        self.refresh_running_user_shell_output();
        self.invalidate_transcript();
        self.completion_anchor.get_mut().take();
        let rows = self.transcript_rows();
        self.reasoning_expanded = old_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let identity = row.reasoning.as_ref()?.identity?;
                if !self.reasoning_expanded.contains(&identity) {
                    return None;
                }
                let index = Self::refreshed_part(&old_rows, &rows, index)?;
                rows[index].reasoning.as_ref()?.identity
            })
            .collect();
        if let Some((index, row_offset)) = anchor {
            let matched = Self::refreshed_part(&old_rows, &rows, index);
            if let Some(index) = matched
                && let Some(message) = rows[index].message_id.clone()
            {
                let part = rows[..index]
                    .iter()
                    .filter(|row| row.message_id.as_ref() == Some(&message))
                    .count();
                *self.completion_anchor.get_mut() = Some(CompletionAnchor {
                    message,
                    part,
                    row: row_offset,
                    requested_scroll: self.scroll,
                    pending: true,
                });
            }
        }
        // Keep the painted geometry for sticky-bottom and fallback clamping.
        // The semantic anchor resolves against the new cached part positions.
    }

    fn refreshed_part(old_rows: &[HistoryRow], rows: &[HistoryRow], index: usize) -> Option<usize> {
        let old = &old_rows[index];
        if let Some(id) = &old.message_id {
            let ordinal = old_rows[..index]
                .iter()
                .filter(|row| row.message_id.as_ref() == Some(id))
                .count();
            rows.iter()
                .enumerate()
                .filter(|(_, row)| row.message_id.as_ref() == Some(id))
                .nth(ordinal)
                .map(|(index, _)| index)
        } else {
            // Live parts have no durable ID yet; operation ID or the exact
            // final text binds them to the newly committed part once.
            let same = |row: &HistoryRow| {
                row.role == old.role
                    && match (&old.tool, &row.tool) {
                        (Some(old), Some(new)) => old.op == new.op,
                        (None, None) => {
                            old.text == row.text
                                && old.reasoning.as_ref().map(|r| &r.text)
                                    == row.reasoning.as_ref().map(|r| &r.text)
                        }
                        _ => false,
                    }
            };
            let ordinal = old_rows[index + 1..].iter().filter(|row| same(row)).count();
            rows.iter()
                .enumerate()
                .rev()
                .filter(|(_, row)| same(row))
                .nth(ordinal)
                .map(|(index, _)| index)
        }
    }

    fn transcript_part_positions(
        &self,
        rows: &[HistoryRow],
        widths: (u16, u16),
        part: impl FnMut(usize, usize, usize),
    ) -> usize {
        crate::messages::transcript_part_positions(
            rows,
            Theme::dark(),
            (widths.0, widths.1, None),
            &|agent| self.agent_color(agent),
            &self.markdown_cache,
            &|op| self.exploration_expanded.contains(op),
            part,
        )
    }

    /// Add an older page at the front of the window.
    pub fn prepend_page(&mut self, page: &HistoryPage) {
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.completion_anchor.get_mut().take();
        self.reverted = page.reverted.clone();
        self.scroll = self.display_scroll();
        self.clear_transcript_selection();
        self.viewport.set(None);
        self.window.prepend_older(page);
        self.prune_reasoning();
    }

    /// Add a newer page at the back of the window.
    pub fn append_page(&mut self, page: &HistoryPage) {
        self.invalidate_transcript();
        self.remember_compaction_turns(page);
        self.reverted = page.reverted.clone();
        self.clear_transcript_selection();
        self.window.append_newer(page);
        self.prune_reasoning();
    }

    /// Older committed rows exist before the loaded window.
    pub fn needs_older(&self) -> bool {
        self.window.has_older()
    }

    /// Newer committed rows exist after the loaded window.
    pub fn needs_newer(&self) -> bool {
        self.window.has_newer()
    }

    /// Bounded history window (rows, caps and paging flags).
    pub fn history(&self) -> &HistoryWindow {
        &self.window
    }

    /// Snapshot only the rows actually painted. This same bounded set supplies
    /// the rendered highlight and the clipboard; a new frame or resize makes
    /// stale selections unusable until the next valid mouse selection.
    #[cfg(test)]
    pub(crate) fn paint_transcript(
        &self,
        area: Rect,
        rows: &[Line],
        total: usize,
        scroll: usize,
    ) -> Vec<Line> {
        self.paint_transcript_at(area, rows, total, scroll, None, &[])
    }

    /// The terminal frame is the pointer's owner. The visible rows already
    /// have the clipped header and styles; drawing must not re-index history.
    pub(crate) fn paint_transcript_at(
        &self,
        area: Rect,
        rows: &[Line],
        total: usize,
        scroll: usize,
        frame: Option<Rect>,
        user_targets: &[Option<crate::messages::UserMessageTarget>],
    ) -> Vec<Line> {
        let theme = Theme::dark();
        let hovered_user = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
                || x <= area.x
            {
                return None;
            }
            user_targets.get((y - area.y) as usize).cloned().flatten()
        });
        let hover_row = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || self.thinking_expanded
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
            {
                return None;
            }
            let row = (y - area.y) as usize;
            crate::messages::collapsed_thought_header(rows.get(row)?, theme, (x - area.x) as usize)
                .then_some(row)
        });
        let hover_tool = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || area != crate::shell::transcript_area(self, frame)
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
            {
                return None;
            }
            let row = (y - area.y) as usize;
            // A short transcript does not paint every row in its viewport.
            // A pointer retained from the composer can land in that blank
            // space after submission; it cannot authorize a tool hover.
            let painted = rows.get(row)?;
            let range = crate::messages::tool_hover_range(rows, theme, row);
            let block = painted.style().bg == Some(theme.background_raised());
            let header = rows.get(range.start)?.plain_text();
            // Reject other painted surfaces before another indexed hit lookup.
            // Text is only a cheap candidate filter: the owner hit still decides.
            if !block
                && !header.trim_start().starts_with("→ Explored")
                && !header.trim_start().starts_with("⋯ Exploring")
                && !header.trim_start().starts_with(['✓', '✗', '⋯', '│', '↳'])
            {
                return None;
            }
            self.exploration_hit_with_range(frame, x, y)
                .map(|(_, range)| range)
        });
        let hover_child = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            (owner == frame).then_some(())?;
            self.child_notice_hit(frame, x, y)
        });
        let selected = self.selection.as_ref().filter(|selected| {
            selected.painted.area == area
                && selected.painted.total == total
                && selected.painted.scroll == scroll
                && selected.painted.rows == rows
                && self.panel == TuiPanel::None
        });
        let result = rows
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let hovered;
                let line = if hovered_user.is_some()
                    && user_targets.get(row).cloned().flatten() == hovered_user
                {
                    hovered = if hovered_user.as_ref().is_some_and(|target| target.reverted) {
                        crate::messages::hover_reverted_content(line, theme)
                    } else {
                        crate::messages::hover_user_content(line, theme)
                    };
                    &hovered
                } else if hover_row == Some(row) {
                    hovered = crate::messages::hover_collapsed_thought(line, theme);
                    &hovered
                } else if let Some((target, _, tool)) = hover_child
                    .as_ref()
                    .filter(|(_, range, _)| range.contains(&row))
                {
                    hovered = if *tool {
                        crate::messages::hover_subagent_content(line, theme)
                    } else {
                        match target {
                            crate::messages::CapturedChildTarget::Navigate(job) => {
                                crate::messages::hover_child_notice(line, theme, job)
                            }
                            crate::messages::CapturedChildTarget::ErrorDetails(_) => line.clone(),
                        }
                    };
                    &hovered
                } else if hover_tool
                    .as_ref()
                    .is_some_and(|range| range.contains(&row))
                {
                    hovered = crate::messages::hover_tool_content(line, theme);
                    &hovered
                } else {
                    line
                };
                if let Some(selected) = selected {
                    let (start, end) = selected.bounds();
                    let begin = if row == start.row {
                        start.byte
                    } else if row > start.row {
                        0
                    } else {
                        line.plain_text().len()
                    };
                    let finish = if row == end.row {
                        end.byte
                    } else if row < end.row {
                        line.plain_text().len()
                    } else {
                        0
                    };
                    if begin < finish {
                        return line.highlight(begin, finish, theme.text(), theme.background());
                    }
                }
                line.clone()
            })
            .collect();
        let mut painted = self.painted_transcript.borrow_mut();
        if !painted.as_ref().is_some_and(|previous| {
            previous.area == area
                && previous.total == total
                && previous.scroll == scroll
                && previous.rows == rows
                && previous.user_targets == user_targets
        }) {
            self.paint_generation
                .set(self.paint_generation.get().wrapping_add(1));
            *painted = Some(PaintedTranscript {
                area,
                rows: rows.to_vec(),
                user_targets: user_targets.to_vec(),
                total,
                scroll,
            });
        }
        result
    }

    /// Configuration/test override of the platform's native clipboard mode.
    pub fn set_clipboard_mode(&mut self, mode: ClipboardMode) {
        self.clipboard_mode = mode;
    }

    pub fn clipboard_mode(&self) -> ClipboardMode {
        self.clipboard_mode
    }

    /// A parked route may carry an older catalog. Route activation adopts the
    /// current view's last successful owner projection, not the parked value.
    pub fn sync_clipboard_mode_from(&mut self, current: &Self) {
        self.chrome.session_tps = current.chrome.session_tps;
        self.chrome.diffs = current.chrome.diffs;
        self.set_conversation_shortcuts(
            Some(current.conversation_shortcut(true)),
            Some(current.conversation_shortcut(false)),
        );
        self.chrome.conversation_shortcuts = current.chrome.conversation_shortcuts.clone();
        if current.clipboard_mode == ClipboardMode::Disabled {
            self.disable_clipboard_until_catalog();
        } else {
            self.apply_owner_clipboard_mode(current.owner_clipboard_mode);
        }
    }

    /// A reload receipt is already published even if a later route-specific
    /// catalog query fails. Apply this safety-sensitive owner setting first so
    /// a stale view cannot keep auto-copy enabled after switching to manual.
    pub fn refresh_clipboard_mode(&mut self, mode: Option<oc_core::queries::TerminalCopyMode>) {
        self.clear_transcript_selection();
        self.apply_owner_clipboard_mode(mode);
    }

    pub fn disable_clipboard_until_catalog(&mut self) {
        self.clear_transcript_selection();
        self.clipboard_mode = ClipboardMode::Disabled;
    }

    pub(super) fn apply_owner_clipboard_mode(
        &mut self,
        mode: Option<oc_core::queries::TerminalCopyMode>,
    ) {
        self.owner_clipboard_mode = mode;
        self.chrome.terminal_copy = mode;
        self.clipboard_mode = match mode {
            Some(oc_core::queries::TerminalCopyMode::Select) => ClipboardMode::Select,
            Some(oc_core::queries::TerminalCopyMode::Manual) => ClipboardMode::Manual,
            None => ClipboardMode::default(),
        };
    }

    /// Drain the mouse copy request once. The caller performs the actual
    /// clipboard write and reports its asynchronous result separately.
    pub fn take_copy_request(&mut self) -> Option<String> {
        self.pending_copy.take()
    }

    /// Exact owner text, never a wrapped or locally shortened preview.
    pub fn copy_message_text(&mut self, text: String) -> Result<(), String> {
        if self.clipboard_mode == ClipboardMode::Disabled {
            return Err("clipboard configuration unavailable".into());
        }
        if text.len() > MAX_SELECTION_BYTES {
            return Err("Message exceeds clipboard size limit".into());
        }
        self.pending_copy = Some(text);
        Ok(())
    }

    pub fn restore_prompt(&mut self, text: String) {
        self.input = text;
        self.editor.clear();
        self.editor.cursor = self.input.len();
        self.input_revision += 1;
    }

    pub fn conversation_applied(
        &mut self,
        snapshot: &oc_core::queries::ConversationSnapshot,
        page: &HistoryPage,
    ) {
        let draft = snapshot.draft.clone().unwrap_or_else(|| {
            if matches!(
                dispatch(self.input.trim()),
                Some(CommandAction::UndoConversation | CommandAction::RedoConversation)
            ) {
                String::new()
            } else {
                self.input.clone()
            }
        });
        self.set_session(snapshot.session.clone());
        self.attach_page(page);
        self.restore_prompt(draft);
        self.conversation_available = Some((snapshot.can_undo, snapshot.can_redo));
        self.reverted = snapshot.reverted.clone();
        self.dcp = DcpPanelState::default();
    }

    /// The binary reports the actual asynchronous clipboard write outcome.
    pub fn report_clipboard_result(&mut self, result: Result<(), String>) {
        if let Some(detail) = &mut self.mcp_detail
            && detail.copy_pending
        {
            detail.copy_pending = false;
            detail.copied = result.is_ok();
            if let Err(error) = result {
                self.push_transient_note(&error, NoteVariant::Error);
            }
            return;
        }
        match result {
            Ok(()) => {
                if matches!(self.panel, TuiPanel::MessageActions { .. }) {
                    self.close_panel();
                }
                self.push_transient_note("Copied to clipboard", NoteVariant::Info);
            }
            Err(error) => self.push_transient_note(&error, NoteVariant::Error),
        }
    }

    pub(crate) fn clear_transcript_selection(&mut self) {
        self.child_notice_down = None;
        self.reverted_down = None;
        self.message_down = None;
        self.paint_generation
            .set(self.paint_generation.get().wrapping_add(1));
        self.selection = None;
        self.selection_gesture = false;
        self.click = None;
        self.pending_copy = None;
        *self.painted_transcript.borrow_mut() = None;
    }

    pub(super) fn selection_text(&self) -> Result<Option<String>, &'static str> {
        const TOO_LARGE: &str = "Selection exceeds clipboard size limit";
        let Some(selected) = self.selection.as_ref() else {
            return Ok(None);
        };
        let painted = self.painted_transcript.borrow();
        let Some(current) = painted.as_ref() else {
            return Ok(None);
        };
        if selected.painted.area != current.area
            || selected.painted.total != current.total
            || selected.painted.scroll != current.scroll
            || selected.painted.rows != current.rows
        {
            return Ok(None);
        }
        let (start, end) = selected.bounds();
        if start == end {
            return Ok(None);
        }
        let mut result = String::new();
        for row in start.row..=end.row {
            let Some(line) = selected.painted.rows.get(row) else {
                return Ok(None);
            };
            let text = line.plain_text();
            let begin = if row == start.row { start.byte } else { 0 };
            let finish = if row == end.row { end.byte } else { text.len() };
            if begin > finish || !text.is_char_boundary(begin) || !text.is_char_boundary(finish) {
                return Ok(None);
            }
            let addition = finish - begin + usize::from(row > start.row);
            if result.len().saturating_add(addition) > MAX_SELECTION_BYTES {
                return Err(TOO_LARGE);
            }
            if row > start.row {
                result.push('\n');
            }
            result.push_str(&text[begin..finish]);
        }
        Ok((!result.is_empty()).then_some(result))
    }

    pub(super) fn request_selection_copy(&mut self) {
        self.pending_copy = None;
        match self.selection_text() {
            Ok(Some(text)) => self.pending_copy = Some(text),
            Ok(None) => {}
            Err(error) => self.push_transient_note(error, NoteVariant::Error),
        }
    }

    pub(super) fn handle_transcript_selection(&mut self, event: MouseEvent, area: Rect) {
        let relevant = matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && event.modifiers.is_empty()
            || matches!(event.kind, MouseEventKind::Down(MouseButton::Right))
                && self.clipboard_mode == ClipboardMode::Manual
            || self.selection_gesture
                && matches!(event.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_));
        if !relevant {
            // A new press on any other surface breaks the multi-click chain,
            // without destroying the completed selection for manual copy.
            if matches!(event.kind, MouseEventKind::Down(_)) {
                self.selection_gesture = false;
                self.click = None;
            }
            return;
        }
        let current = crate::shell::transcript_area(self, area);
        let painted_valid = self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| painted.area == current);
        if !painted_valid {
            self.clear_transcript_selection();
            return;
        }
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            && !current.contains((event.column, event.row).into())
        {
            self.selection_gesture = false;
            self.click = None;
            return;
        }
        let (rows, total, scroll) =
            self.visible_transcript_at_viewport(current.width, area.width, current.height);
        if !self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| {
                painted.total == total && painted.scroll == scroll && painted.rows == rows
            })
        {
            self.clear_transcript_selection();
            return;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Right)
                if self.clipboard_mode == ClipboardMode::Manual =>
            {
                self.selection_gesture = false;
                self.click = None;
                self.request_selection_copy();
            }
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                let painted = self
                    .painted_transcript
                    .borrow()
                    .clone()
                    .expect("validated paint");
                let Some(at) = self.transcript_point(&painted, area, event.column, event.row)
                else {
                    self.selection_gesture = false;
                    self.click = None;
                    return;
                };
                let now = Instant::now();
                let count = self
                    .click
                    .filter(|click| {
                        click.x == event.column
                            && click.y == event.row
                            && now.duration_since(click.at) <= Duration::from_millis(500)
                    })
                    .map_or(1, |click| (click.count % 3) + 1);
                self.click = Some(TranscriptClick {
                    x: event.column,
                    y: event.row,
                    at: now,
                    count,
                });
                let mut selected = TranscriptSelection {
                    anchor: at,
                    focus: at,
                    painted,
                    dragging: count > 1,
                };
                self.selection_gesture = true;
                if count == 2 || count == 3 {
                    let text = selected.painted.rows[at.row].plain_text();
                    if count == 3 {
                        // OpenTUI paints the content of the display row, not
                        // the indentation before its first visible glyph.
                        selected.anchor.byte = text.len() - text.trim_start().len();
                        selected.focus.byte = text.len();
                    } else {
                        // OpenTUI's painted word includes internal hyphens
                        // (GEOMETRY-SHORT), but not the adjacent colon. Group
                        // Unicode words on this already-wrapped display row.
                        let words: Vec<_> = text.unicode_word_indices().collect();
                        if let Some((index, (offset, word))) =
                            words.iter().enumerate().find(|(_, (offset, word))| {
                                *offset <= at.byte && at.byte < *offset + word.len()
                            })
                        {
                            let mut start = *offset;
                            let mut end = start + word.len();
                            for (next_offset, next_word) in words[index + 1..].iter() {
                                if text.get(end..*next_offset) != Some("-") {
                                    break;
                                }
                                end = next_offset + next_word.len();
                            }
                            for (previous_offset, previous_word) in words[..index].iter().rev() {
                                if text.get(previous_offset + previous_word.len()..start)
                                    != Some("-")
                                {
                                    break;
                                }
                                start = *previous_offset;
                            }
                            selected.anchor.byte = start;
                            selected.focus.byte = end;
                        }
                    }
                }
                self.selection = Some(selected);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let at = self
                    .painted_transcript
                    .borrow()
                    .as_ref()
                    .and_then(|painted| {
                        self.transcript_point(painted, area, event.column, event.row)
                    });
                if let Some(at) = at
                    && let Some(selected) = &mut self.selection
                {
                    selected.dragging = true;
                    selected.focus = at;
                }
                self.click = None;
            }
            MouseEventKind::Drag(_) => {
                // Only a left press owns this selection. An unrelated button
                // must never turn an old highlight into a new drag/copy.
                self.selection_gesture = false;
                self.click = None;
            }
            MouseEventKind::Up(_) => {
                self.selection_gesture = false;
                let at = self
                    .painted_transcript
                    .borrow()
                    .as_ref()
                    .and_then(|painted| {
                        self.transcript_point(painted, area, event.column, event.row)
                    });
                if let Some(selected) = &mut self.selection
                    && selected.dragging
                {
                    if let Some(at) = at {
                        // Repeated-click word/line selection keeps its
                        // expanded endpoints on a no-motion release.
                        if self.click.is_none_or(|click| {
                            click.count == 1 || (click.x, click.y) != (event.column, event.row)
                        }) {
                            selected.focus = at;
                        }
                    }
                    selected.dragging = false;
                    if self.clipboard_mode == ClipboardMode::Select {
                        self.request_selection_copy();
                    }
                }
            }
            MouseEventKind::Down(_) => {
                self.selection_gesture = false;
                self.click = None;
            }
            _ => {}
        }
    }

    pub(super) fn user_message_target_at(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::UserMessageTarget> {
        let area = crate::shell::transcript_area(self, frame);
        if self.panel != TuiPanel::None
            || !area.contains((x, y).into())
            || x <= area.x
            || self.transcript_overpainted(frame, x, y)
            || self.click.is_none_or(|click| click.count != 1)
            || self.selection_gesture
            || !matches!(self.selection_text(), Ok(None))
        {
            return None;
        }
        self.painted_user_message_target_at(frame, x, y)
    }

    pub(super) fn painted_user_message_target_at(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::UserMessageTarget> {
        let area = crate::shell::transcript_area(self, frame);
        if self.panel != TuiPanel::None
            || !area.contains((x, y).into())
            || x <= area.x
            || self.transcript_overpainted(frame, x, y)
        {
            return None;
        }
        let (rows, total, scroll, targets) =
            self.visible_transcript_at_viewport_with_targets(area.width, frame.width, area.height);
        let painted = self.painted_transcript.borrow();
        let painted = painted.as_ref()?;
        if painted.area != area
            || painted.total != total
            || painted.scroll != scroll
            || painted.rows != rows
            || painted.user_targets != targets
        {
            return None;
        }
        targets.get((y - area.y) as usize).cloned().flatten()
    }

    fn transcript_point(
        &self,
        painted: &PaintedTranscript,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<TextPoint> {
        let rect = painted.area;
        if !rect.contains((x, y).into()) || self.transcript_overpainted(area, x, y) {
            return None;
        }
        let row = (y - rect.y) as usize;
        Some(TextPoint {
            row,
            byte: painted.rows.get(row)?.byte_at_cell((x - rect.x) as usize),
        })
    }

    pub(super) fn exploration_hit(&self, area: Rect, x: u16, y: u16) -> Option<String> {
        self.exploration_hit_with_range(area, x, y)
            .map(|(operation, _)| operation)
    }

    fn exploration_hit_with_range(
        &self,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<(String, std::ops::Range<usize>)> {
        let rect = crate::shell::transcript_area(self, area);
        if rect.width == 0 || rect.height == 0 || !rect.contains((x, y).into()) {
            return None;
        }
        let rows = self.transcript_rows();
        let live_row = (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| {
            rows.len() - 1 - usize::from(rows.last().is_some_and(|row| row.role == "reverted"))
        });
        crate::messages::exploration_header_at_with_range(
            &rows,
            Theme::dark(),
            (rect.width, area.width),
            (
                rect.height as usize,
                self.scroll_for_current_view(),
                live_row,
            ),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
    }

    pub(super) fn reasoning_hit(
        &self,
        area: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::ReasoningIdentity> {
        if self.thinking_expanded || self.transcript_overpainted(area, x, y) {
            return None;
        }
        let rect = crate::shell::transcript_area(self, area);
        if rect.width == 0 || rect.height == 0 || !rect.contains((x, y).into()) {
            return None;
        }
        let rows = self.transcript_rows();
        let live_row =
            (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| rows.len() - 1);
        crate::messages::reasoning_header_at(
            &rows,
            Theme::dark(),
            (rect.width, area.width),
            (
                rect.height as usize,
                self.scroll_for_current_view(),
                live_row,
            ),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
    }

    pub(super) fn transcript_overpainted(&self, area: Rect, x: u16, y: u16) -> bool {
        if crate::shell::toast_rect(self, area).is_some_and(|rect| rect.contains((x, y).into())) {
            return true;
        }
        if self.slash_options().is_none() && self.mention_options().is_none() {
            return false;
        }
        // Recover the session main column from the exact painted transcript
        // rectangle, then use the same prompt allocation as shell::render_session.
        let transcript = crate::shell::transcript_area(self, area);
        if transcript.width == 0 {
            return false;
        }
        let shell = crate::layout::configured_shell_regions(
            area,
            self.chrome.devtools_visible(),
            self.chrome.vertical_tabs_width,
        );
        let pad = transcript.x.saturating_sub(shell.session.x);
        let main = Rect::new(
            shell.session.x,
            shell.session.y,
            transcript.width.saturating_add(2 * pad),
            shell.session.height,
        );
        let text_width = main.width.saturating_sub(4 * pad + 1).max(1);
        let input_height = (self.prompt_layout(text_width as usize).0.len() as u16)
            .min((area.height / 3).max(6))
            .max(1);
        let body = crate::layout::dynamic_session_regions(main, 0, input_height + 3).prompt;
        let covers = |count: usize| {
            let height = (count.clamp(1, 10) as u16).min(body.y.saturating_sub(area.y));
            let rect = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
            rect.width >= 3 && rect.height > 0 && rect.contains((x, y).into())
        };
        self.slash_options()
            .is_some_and(|options| covers(options.len()))
            || self.mention_options().is_some_and(|options| {
                let height = (options.paths.len().clamp(1, MENTION_LIMIT) as u16)
                    .min(body.y.saturating_sub(area.y));
                let rect = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
                rect.width >= 3 && rect.height > 0 && rect.contains((x, y).into())
            })
    }

    fn child_notice_hit(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<(
        crate::messages::CapturedChildTarget,
        std::ops::Range<usize>,
        bool,
    )> {
        if !self.window.rows().iter().any(|row| {
            row.child_notice.is_some()
                || row
                    .tool
                    .as_ref()
                    .is_some_and(|card| matches!(card.render,crate::tools::ToolRender::Subagent(_)))
        }) && !self
            .live_parts
            .iter()
            .any(|part| matches!(part,LivePart::Tool {card,..} if matches!(card.render,crate::tools::ToolRender::Subagent(_))))
            || self.panel != TuiPanel::None
            || self.approvals.active().is_some()
            || self.questions.active().is_some()
            || self.composer_open()
            || self.shell_viewer().is_some()
            || self.transcript_overpainted(frame, x, y)
        {
            return None;
        }
        let rect = crate::shell::transcript_area(self, frame);
        if !rect.contains((x, y).into()) {
            return None;
        }
        let (visible, total, scroll) =
            self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        if !self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| {
                painted.area == rect
                    && painted.total == total
                    && painted.scroll == scroll
                    && painted.rows == visible
            })
        {
            return None;
        }
        let rows = self.transcript_rows();
        let live =
            (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| rows.len() - 1);
        crate::messages::captured_child_at_with_range(
            &rows,
            Theme::dark(),
            (rect.width, frame.width),
            (rect.height as usize, scroll, live),
            |agent| self.agent_color(agent),
            &self.markdown_cache,
            (
                &|op| self.exploration_expanded.contains(op),
                ((x - rect.x) as usize, (y - rect.y) as usize),
            ),
        )
        .filter(|(target, _, _)| match target {
            crate::messages::CapturedChildTarget::Navigate(job) => {
                self.attached_session() == Some(&job.parent)
            }
            crate::messages::CapturedChildTarget::ErrorDetails(_) => true,
        })
    }

    pub(super) fn child_notice_mouse(
        &mut self,
        event: MouseEvent,
        frame: Rect,
    ) -> Option<KeyOutcome> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.child_notice_down =
                    self.child_notice_hit(frame, event.column, event.row)
                        .map(|(target, _, _)| ChildNoticePress {
                            target,
                            frame,
                            generation: self.paint_generation.get(),
                            point: (event.column, event.row),
                        });
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self.child_notice_down.take()?;
                if !event.modifiers.is_empty()
                    || pressed.frame != frame
                    || pressed.point != (event.column, event.row)
                    || pressed.generation != self.paint_generation.get()
                    || self.click.is_some_and(|click| click.count != 1)
                    || !matches!(self.selection_text(), Ok(None))
                {
                    return None;
                }
                let (target, _, _) = self.child_notice_hit(frame, event.column, event.row)?;
                if target != pressed.target {
                    return None;
                }
                self.clear_transcript_selection();
                return Some(match target {
                    crate::messages::CapturedChildTarget::Navigate(job) => KeyOutcome {
                        intent: Some(PanelIntent::OpenChild { selected: *job }),
                        ..KeyOutcome::default()
                    },
                    crate::messages::CapturedChildTarget::ErrorDetails(operation) => {
                        self.toggle_tool_expansion_at(&operation, frame);
                        KeyOutcome::default()
                    }
                });
            }
            _ => {}
        }
        None
    }

    pub(super) fn toggle_tool_expansion_at(&mut self, operation: &str, frame: Rect) {
        let rect = crate::shell::transcript_area(self, frame);
        let height = rect.height as usize;
        let (_, before, displayed) =
            self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let first = before.saturating_sub(height).saturating_sub(displayed);
        let rows = self.transcript_rows();
        self.exploration_expanded.retain(|id| {
            rows.iter()
                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
        });
        if !self.exploration_expanded.insert(operation.to_owned()) {
            self.exploration_expanded.remove(operation);
        }
        let (_, after) = self.visible_transcript(rect.width, frame.width, rect.height);
        self.scroll = after.saturating_sub(height).saturating_sub(first);
        self.observe_transcript_viewport(rect.width, frame.width, rect.height, after, self.scroll);
    }

    pub(super) fn prune_reasoning(&mut self) {
        let retained: BTreeSet<_> = self
            .transcript_rows()
            .iter()
            .filter_map(|row| row.reasoning.as_ref()?.identity)
            .collect();
        self.reasoning_expanded.retain(|id| retained.contains(id));
        self.reasoning_down = self
            .reasoning_down
            .take()
            .filter(|(id, ..)| retained.contains(id));
    }

    /// Visible viewport lines (bounded, scroll-aware, live answer last).
    ///
    /// Unstyled and unwrapped; [`TuiState::transcript_lines`] is the styled
    /// renderer the shell uses.
    pub fn viewport(&self) -> Vec<String> {
        let lines = self.transcript_lines(0, u16::MAX);
        let texts: Vec<String> = lines
            .iter()
            .map(|line| line.plain_text().trim_end().to_string())
            .collect();
        let total = texts.len();
        let max_scroll = total.saturating_sub(VIEWPORT_LINES);
        let scroll = self.scroll.min(max_scroll);
        let end = total - scroll;
        let start = end.saturating_sub(VIEWPORT_LINES);
        texts[start..end].to_vec()
    }

    /// Render rows of the transcript: the bounded window plus the live parts
    /// (frozen text/reasoning segments and tool cards) and the open live
    /// answer (reasoning block and streaming text) while a turn is active.
    pub fn transcript_rows(&self) -> Vec<HistoryRow> {
        #[cfg(test)]
        self.transcript_row_copies
            .set(self.transcript_row_copies.get() + self.window.len());
        let mut rows = self.window.rows().to_vec();
        for (ordinal, part) in self.live_parts.iter().enumerate() {
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
            rows.push(row);
        }
        let live = !self.live_text.is_empty() || !self.live_reasoning.is_empty();
        if live {
            rows.push(HistoryRow {
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: self.live_text.clone(),
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning: (!self.live_reasoning.is_empty()).then(|| ReasoningBlock {
                    text: self.live_reasoning.clone(),
                    duration_ms: None,
                    running: true,
                    expanded: false,
                    toggleable: true,
                    identity: Some(crate::messages::ReasoningIdentity::Live(
                        self.reasoning_epoch,
                        self.live_part_offset + self.live_parts.len(),
                    )),
                }),
                meta: None,
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
        }
        let mut group_first = None;
        for row in &mut rows {
            let adjacent = crate::messages::reasoning_group_member(row);
            if !adjacent {
                group_first = None;
            }
            let visible = crate::messages::visible_reasoning(row);
            if let Some(reasoning) = &mut row.reasoning {
                let id = reasoning.identity;
                let first = if adjacent && visible {
                    *group_first.get_or_insert(id)
                } else {
                    id
                };
                reasoning.expanded = self.thinking_expanded
                    || first.is_some_and(|id| self.reasoning_expanded.contains(&id));
                reasoning.toggleable = !self.thinking_expanded;
            }
        }
        if let Some(reverted) = &self.reverted
            && !self.window.has_newer()
        {
            rows.push(HistoryRow {
                message_id: Some(std::sync::Arc::new(reverted.message.clone())),
                seq: i64::MAX,
                role: "reverted".into(),
                text: reverted.user_messages.to_string(),
                agent: Some(self.conversation_shortcut(false)),
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
        }
        self.interleave_compactions(&mut rows);
        // Apply current owner configuration to the render copy, including
        // replayed/parked footer metadata. Retain every measured statistic in
        // the history/live state. Indexed footers are generated outside the
        // Markdown body cache, so config changes cannot reuse a stale footer.
        for row in &mut rows {
            if let Some(card) = &mut row.tool {
                card.diff_settings = self.chrome.diffs;
                if let crate::tools::ToolRender::Dcp(view) = &mut card.render {
                    view.config = self.chrome.dcp.clone();
                    view.color_index = row.agent_color_index.or(view.color_index).or_else(|| {
                        row.agent.as_ref().and_then(|id| {
                            self.agents
                                .iter()
                                .find(|entry| &entry.id == id)
                                .map(|entry| entry.color_index)
                        })
                    });
                    view.spinner = (self.chrome.animations != Some(false))
                        .then(|| crate::compaction::FRAMES[self.compaction_frame].to_string());
                }
            }
            if let Some(meta) = &mut row.meta {
                meta.session_tps = self.chrome.session_tps;
            }
        }
        rows
    }

    fn interleave_compactions(&self, rows: &mut Vec<HistoryRow>) {
        // Traverse newest first so multiple checkpoints sharing one anchor stay ordered.
        for snapshot in self.compactions.iter().rev() {
            let anchor = &snapshot.anchor;
            let at = [
                anchor.tool.as_ref().and_then(|tool| {
                    rows.iter()
                        .rposition(|r| r.tool.as_ref().is_some_and(|card| &card.op == tool))
                }),
                anchor.message.as_ref().and_then(|message| {
                    rows.iter()
                        .rposition(|r| r.message_id.as_ref().is_some_and(|id| &id.0 == message))
                }),
            ]
            .into_iter()
            .flatten()
            .max()
            // A turn can acquire continuation text after this checkpoint. Its
            // replay message is a fallback, never a replacement for an exact
            // tool/message boundary (OC2 compaction20260927-10 completed frame).
            .or_else(|| {
                anchor
                    .turn
                    .as_ref()
                    .and_then(|turn| self.compaction_turn_messages.get(turn))
                    .and_then(|message| {
                        rows.iter()
                            .rposition(|r| r.message_id.as_ref().is_some_and(|id| &id.0 == message))
                    })
            });
            let position = if let Some(at) = at {
                at + 1
            } else if anchor.message.is_none()
                && anchor.tool.is_none()
                && anchor.turn.is_none()
                && !self.window.has_older()
            {
                0
            } else if crate::compaction::active(snapshot) && !self.window.has_newer() {
                rows.len()
            } else {
                continue;
            };
            rows.insert(
                position,
                crate::compaction::row(
                    snapshot,
                    self.compaction_frame,
                    self.chrome.animations != Some(false),
                ),
            );
        }
    }

    fn remember_compaction_turns(&mut self, page: &HistoryPage) {
        for message in &page.rows {
            if let Some(turn) = &message.turn {
                self.compaction_turn_messages
                    .insert(turn.id.clone(), message.id.0.clone());
            }
        }
        while self.compaction_turn_messages.len() > crate::history::WINDOW_ROWS {
            self.compaction_turn_messages.pop_first();
        }
    }

    /// Styled transcript lines wrapped to the content-box `width`
    /// (upstream row model: user block, assistant markdown, collapsed
    /// reasoning, assistant footer). `width == 0` is the unbounded text
    /// projection used for scroll metrics and plain-text assertions.
    pub fn transcript_lines(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let theme = Theme::dark();
        crate::messages::transcript_with_expansion(
            &self.transcript_rows(),
            theme,
            width,
            terminal_width,
            |agent| self.agent_color(agent),
            Some(&self.markdown_cache),
            &|op| self.exploration_expanded.contains(op),
        )
    }

    /// Bounded wrapped content, including the scrollbox's top padding. Padding
    /// scrolls away with long history; short history starts below that one row.
    pub fn rendered_transcript(&self, width: u16, terminal_width: u16) -> Vec<Line> {
        let mut lines = vec![Line::plain("")];
        lines.extend(crate::styled::wrap_lines(
            &self.transcript_lines(width, terminal_width),
            width as usize,
        ));
        lines
    }

    /// Materialize no more than the visible viewport, counting bounded parts
    /// through the per-session Markdown cache instead of building all rows.
    pub fn visible_transcript(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize) {
        let (lines, total, _) = self.visible_transcript_at_viewport(width, terminal_width, height);
        (lines, total)
    }

    /// Resolve a resize against the last painted top row without changing
    /// the input-owned scroll request or materializing the whole transcript.
    pub(crate) fn visible_transcript_at_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (Vec<Line>, usize, usize) {
        let (lines, total, scroll, _) =
            self.visible_transcript_at_viewport_with_targets(width, terminal_width, height);
        (lines, total, scroll)
    }

    pub(crate) fn visible_transcript_at_viewport_with_targets(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (
        Vec<Line>,
        usize,
        usize,
        Vec<Option<crate::messages::UserMessageTarget>>,
    ) {
        let theme = Theme::dark();
        if let Some(view) = self.visible_projection.borrow().as_ref().filter(|view| {
            view.revision == self.transcript_revision
                && view.window_revision == self.window.revision()
                && view.viewport == self.viewport.get()
                && view.live_lengths == (self.live_text.len(), self.live_reasoning.len())
                && view.dimensions == (width, terminal_width, height)
                && view.requested_scroll == self.scroll
                && view.chrome == self.chrome
                && view.theme == theme.mode()
                && view.thinking == self.thinking_expanded
                && view.reasoning == self.reasoning_expanded
                && view.exploration == self.exploration_expanded
                && view.compaction_frame == self.compaction_frame
        }) {
            return (
                view.lines.clone(),
                view.total,
                view.scroll,
                view.targets.clone(),
            );
        }
        #[cfg(test)]
        self.visible_projection_builds
            .set(self.visible_projection_builds.get() + 1);
        let rows = self.transcript_rows();
        let live_row =
            (!self.live_text.is_empty() || !self.live_reasoning.is_empty()).then(|| rows.len() - 1);
        let render = |height, scroll| {
            crate::messages::visible_transcript_user_targets(
                &rows,
                Theme::dark(),
                (width, terminal_width),
                (height, scroll, live_row),
                |agent| self.agent_color(agent),
                &self.markdown_cache,
                &|op| self.exploration_expanded.contains(op),
            )
        };
        let previous = self.viewport.get();
        let resized = previous.is_some_and(|view| {
            (view.width, view.terminal_width, view.height) != (width, terminal_width, height)
        });
        let semantic_scroll = self
            .completion_anchor
            .borrow()
            .as_ref()
            .filter(|anchor| {
                anchor.requested_scroll == self.scroll
                    && self.scroll > 0
                    && (anchor.pending || resized)
            })
            .and_then(|anchor| {
                let index = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.message_id.as_ref() == Some(&anchor.message))
                    .nth(anchor.part)
                    .map(|(index, _)| index)?;
                let mut top = None;
                let total = self.transcript_part_positions(
                    &rows,
                    (width, terminal_width),
                    |part, start, end| {
                        if part == index && end > start {
                            top = Some(start + anchor.row.min(end - start - 1));
                        }
                    },
                );
                top.map(|top| total.saturating_sub(height as usize).saturating_sub(top))
            });
        let scroll = if self.scroll == 0 {
            0
        } else if let Some(scroll) = semantic_scroll {
            scroll
        } else if resized && previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            let view = previous.expect("resized viewport");
            // Count-only indexing is needed on resize, not on every draw.
            let (_, total, _) = render(0, 0);
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            total.saturating_sub(height as usize).saturating_sub(top)
        } else if previous.is_some_and(|view| view.requested_scroll == self.scroll) {
            self.display_scroll()
        } else {
            self.scroll
        };
        let (mut lines, total, mut targets) = render(height as usize, scroll);
        // A bottom-relative offset alone follows appended rows even when the
        // reader detached. Preserve the last painted top row during live tail
        // growth. Prepending deliberately clears the viewport: its new rows
        // precede that anchor, so the bottom-relative offset already holds it.
        let scroll = if self.scroll > 0
            && semantic_scroll.is_none()
            && !resized
            && let Some(view) = previous.filter(|view| view.requested_scroll == self.scroll)
            && view.total != total
        {
            let top = view
                .total
                .saturating_sub(view.height as usize)
                .saturating_sub(view.displayed_scroll);
            let anchored = total.saturating_sub(height as usize).saturating_sub(top);
            (lines, _, targets) = render(height as usize, anchored);
            anchored
        } else {
            scroll
        };
        let scroll = scroll.min(total.saturating_sub(height as usize));
        *self.visible_projection.borrow_mut() = Some(VisibleTranscriptProjection {
            revision: self.transcript_revision,
            window_revision: self.window.revision(),
            viewport: Some(TranscriptViewport {
                width,
                terminal_width,
                height,
                total,
                requested_scroll: self.scroll,
                displayed_scroll: scroll,
            }),
            live_lengths: (self.live_text.len(), self.live_reasoning.len()),
            dimensions: (width, terminal_width, height),
            requested_scroll: self.scroll,
            chrome: self.chrome.clone(),
            theme: theme.mode(),
            thinking: self.thinking_expanded,
            reasoning: self.reasoning_expanded.clone(),
            exploration: self.exploration_expanded.clone(),
            compaction_frame: self.compaction_frame,
            lines: lines.clone(),
            total,
            scroll,
            targets: targets.clone(),
        });
        (lines, total, scroll, targets)
    }

    pub(super) fn invalidate_transcript(&mut self) {
        self.transcript_revision = self.transcript_revision.wrapping_add(1);
        self.visible_projection.get_mut().take();
    }

    /// Wheel/scrollbox navigation never changes the focused editor, even
    /// when keyboard Up/Down would move its caret or recall prompt history.
    pub fn scroll_transcript(&mut self, up: bool) -> KeyOutcome {
        self.wheel_motion = None;
        self.poll_submission();
        if self.panel != TuiPanel::None {
            return KeyOutcome::default();
        }
        self.clear_transcript_selection();
        if up {
            let max_scroll = self.max_scroll();
            self.scroll = self.display_scroll();
            if self.scroll < max_scroll {
                self.scroll += 1;
            }
            KeyOutcome {
                intent: (self.window.has_older() && self.scroll >= self.max_scroll())
                    .then_some(PanelIntent::LoadOlder),
                ..KeyOutcome::default()
            }
        } else if self.scroll > 0 {
            // Resize can clamp the displayed position below the retained
            // request. The first Down must move from that visible row.
            self.scroll = self.display_scroll().saturating_sub(1);
            KeyOutcome::default()
        } else {
            KeyOutcome {
                intent: self.window.has_newer().then_some(PanelIntent::LoadNewer),
                ..KeyOutcome::default()
            }
        }
    }

    /// A compatible directional wheel burst, in original arrival order. Only
    /// adjacent events with the same owner/direction/modifiers may be grouped.
    /// OC2 CustomSpeedScroll(3) times the terminal's unit delta supplies the
    /// target; keyboard navigation deliberately retains its separate step.
    pub fn wheel_transcript_at(&mut self, up: bool, ticks: usize, now: Instant) -> KeyOutcome {
        if self.panel != TuiPanel::None || ticks == 0 {
            return KeyOutcome::default();
        }
        self.tick_scroll_animation(now);
        let visible = self.display_scroll();
        let continuing = self.wheel_motion.filter(|motion| motion.up == up);
        let pending = continuing.map_or(0, |motion| motion.distance - motion.applied);
        // A reversal discards the old presentation debt and starts at the
        // visible position. It must not wait for an obsolete target to settle.
        let distance = ticks.saturating_mul(3).saturating_add(pending);
        let distance = distance.min(if up {
            self.max_scroll().saturating_sub(visible)
        } else {
            visible
        });
        self.scroll = visible;
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        self.wheel_motion = (distance > 0).then_some(WheelMotion {
            started: continuing.map_or(now, |motion| motion.started),
            distance: distance.saturating_add(continuing.map_or(0, |motion| motion.applied)),
            applied: continuing.map_or(0, |motion| motion.applied),
            up,
        });
        KeyOutcome {
            intent: if up && visible.saturating_add(distance) >= self.max_scroll() {
                self.window.has_older().then_some(PanelIntent::LoadOlder)
            } else if !up && distance == visible {
                self.window.has_newer().then_some(PanelIntent::LoadNewer)
            } else {
                None
            },
            ..KeyOutcome::default()
        }
    }

    /// Next changed-row deadline, absent at rest. The binary folds this into
    /// its existing active render deadline; there is no independent timer.
    pub fn next_scroll_animation_deadline(&self) -> Option<Instant> {
        let motion = self.wheel_motion?;
        if self.panel != TuiPanel::None {
            return None;
        }
        let nanos = (WHEEL_PRESENTATION.as_nanos() * (motion.applied + 1) as u128)
            .div_ceil(motion.distance as u128);
        Some(motion.started + Duration::from_nanos(nanos as u64))
    }

    /// Elapsed progress is bounded by one presentation budget, independent of
    /// scheduler frequency. Returns dirty only when a painted row changes.
    pub fn tick_scroll_animation(&mut self, now: Instant) -> bool {
        let Some(mut motion) = self.wheel_motion else {
            return false;
        };
        if self.panel != TuiPanel::None {
            self.wheel_motion = None;
            return false;
        }
        let elapsed = now
            .saturating_duration_since(motion.started)
            .as_nanos()
            .min(WHEEL_PRESENTATION.as_nanos());
        let applied = (motion.distance as u128 * elapsed / WHEEL_PRESENTATION.as_nanos()) as usize;
        let step = applied.saturating_sub(motion.applied);
        if step == 0 {
            return false;
        }
        let visible = self.display_scroll();
        self.scroll = if motion.up {
            visible.saturating_add(step).min(self.max_scroll())
        } else {
            visible.saturating_sub(step)
        };
        motion.applied = applied;
        let edge = if motion.up {
            self.scroll == self.max_scroll()
        } else {
            self.scroll == 0
        };
        self.wheel_motion = (!edge && applied < motion.distance).then_some(motion);
        if visible == self.scroll {
            return false;
        }
        // Movement invalidates every press/release target from the old paint,
        // including selection-guarded user/reverted and expandable tool rows.
        self.clear_transcript_selection();
        self.exploration_down = None;
        self.reasoning_down = None;
        true
    }

    fn line_count(&self) -> usize {
        self.transcript_lines(0, u16::MAX).len()
    }

    pub(super) fn max_scroll(&self) -> usize {
        self.viewport.get().map_or_else(
            || self.line_count().saturating_sub(VIEWPORT_LINES),
            |view| view.total.saturating_sub(view.height as usize),
        )
    }

    fn scroll_for_current_view(&self) -> usize {
        if self
            .viewport
            .get()
            .is_some_and(|view| view.requested_scroll == self.scroll)
        {
            self.display_scroll()
        } else {
            self.scroll
        }
    }

    /// Actual rendered geometry for input-driven row scrolling.
    pub fn observe_viewport(&self, height: u16, rendered_rows: usize) {
        self.observe_transcript_viewport(0, 0, height, rendered_rows, self.scroll);
    }

    pub(crate) fn observe_transcript_viewport(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
        total: usize,
        displayed_scroll: usize,
    ) {
        let mut anchor = self.completion_anchor.borrow_mut();
        if anchor
            .as_ref()
            .is_some_and(|anchor| anchor.requested_scroll != self.scroll)
        {
            anchor.take();
        } else if let Some(anchor) = anchor.as_mut() {
            anchor.pending = false;
        }
        self.viewport.set(Some(TranscriptViewport {
            width,
            terminal_width,
            height,
            total,
            requested_scroll: self.scroll,
            displayed_scroll: displayed_scroll.min(total.saturating_sub(height as usize)),
        }));
    }

    /// Latest measured context, never DCP's estimate or a renderer constant.
    pub fn context_usage(&self) -> Option<(u64, Option<u64>)> {
        let usage = self
            .turn_usage
            .as_ref()
            .map(|u| (u.input_tokens, u.output_tokens))
            .or_else(|| {
                // Live usage is handled above; frozen/live/compaction rows
                // have no footer measurement. Borrow the retained durable tail.
                self.window.rows().iter().rev().find_map(|r| {
                    let m = r.meta.as_ref()?;
                    m.context_usage
                        .or_else(|| Some((m.input_tokens?, m.output_tokens?)))
                })
            })?;
        let limit = self
            .picker
            .as_ref()
            .and_then(|p| p.selection())
            .and_then(|s| s.entry.pointer("/limit/context"))
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0);
        Some((usage.0.saturating_add(usage.1), limit))
    }
}
