//! History projection, viewport/selection hit tests and transcript copy.

use super::*;

pub(super) struct ChildNoticePress {
    target: crate::messages::CapturedChildTarget,
    tool: bool,
    frame: Rect,
    generation: u64,
    point: (u16, u16),
}

/// Retain only semantic matching data across a history-window replacement.
/// This never pins an OPEN mutable source: legacy matching retains only the
/// existing bounded presentation prefix, not another transcript payload.
#[derive(Clone)]
pub(super) enum CompletionPart {
    Durable {
        message: std::sync::Arc<oc_core::session::MessageId>,
        position: Option<crate::history::HistoryPosition>,
        ordinal: usize,
    },
    Live {
        role: String,
        operation: Option<String>,
        text: String,
        reasoning: Option<String>,
        text_bytes: Option<usize>,
        reasoning_bytes: Option<usize>,
        ordinal: usize,
    },
}

impl CompletionPart {
    pub(super) fn capture(rows: &[HistoryRow], index: usize, open: bool) -> Self {
        let row = &rows[index];
        if let Some(message) = &row.message_id {
            Self::Durable {
                message: message.clone(),
                position: row.position.clone(),
                ordinal: rows[..index]
                    .iter()
                    .filter(|row| row.message_id.as_ref() == Some(message))
                    .count(),
            }
        } else {
            let mut part = Self::Live {
                role: row.role.clone(),
                operation: row.tool.as_ref().map(|tool| tool.op.clone()),
                text: row
                    .text
                    .completion_prefix(crate::messages::LIVE_MARKDOWN_BYTES)
                    .to_owned(),
                reasoning: row.reasoning.as_ref().map(|reasoning| {
                    reasoning
                        .text
                        .completion_prefix(crate::messages::LIVE_MARKDOWN_BYTES)
                        .to_owned()
                }),
                text_bytes: (!open).then_some(row.text.len()),
                reasoning_bytes: (!open)
                    .then(|| row.reasoning.as_ref().map_or(0, |part| part.text.len())),
                ordinal: 0,
            };
            let after = rows[index + 1..]
                .iter()
                .filter(|row| part.matches_live(row))
                .count();
            if let Self::Live { ordinal, .. } = &mut part {
                *ordinal = after;
            }
            part
        }
    }

    fn matches_live(&self, row: &HistoryRow) -> bool {
        let Self::Live {
            role,
            operation,
            text,
            reasoning,
            text_bytes,
            reasoning_bytes,
            ..
        } = self
        else {
            return false;
        };
        row.role == *role
            && match (operation, &row.tool) {
                (Some(old), Some(new)) => *old == new.op,
                (None, None) => {
                    text_bytes.is_none_or(|bytes| bytes == row.text.len())
                        && row.text.starts_with(text)
                        && match (reasoning, &row.reasoning) {
                            (Some(old), Some(new)) => {
                                reasoning_bytes.is_none_or(|bytes| bytes == new.text.len())
                                    && new.text.starts_with(old)
                            }
                            (None, None) => true,
                            _ => false,
                        }
                }
                _ => false,
            }
    }

    pub(super) fn resolve(&self, rows: &[HistoryRow]) -> Option<usize> {
        match self {
            Self::Durable {
                message,
                position: Some(position),
                ..
            } => rows.iter().position(|row| {
                row.position.as_ref().is_some_and(|new| {
                    new.same_turn_component(position)
                        || (row.message_id.as_ref() == Some(message)
                            && new.component == position.component)
                })
            }),
            Self::Durable {
                message, ordinal, ..
            } => rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.message_id.as_ref() == Some(message))
                .nth(*ordinal)
                .map(|(index, _)| index),
            Self::Live {
                ordinal,
                text,
                reasoning,
                text_bytes,
                reasoning_bytes,
                operation,
                ..
            } => {
                let mut candidates = rows
                    .iter()
                    .enumerate()
                    .rev()
                    .filter(|(_, row)| self.matches_live(row));
                // Without a durable component, an ambiguous truncated prefix is
                // not evidence that two canonical payloads are the same part.
                let bounded = operation.is_none()
                    && (text_bytes.is_none_or(|bytes| bytes > text.len())
                        || reasoning_bytes
                            .is_none_or(|bytes| bytes > reasoning.as_ref().map_or(0, String::len)));
                if bounded {
                    let first = candidates.next()?;
                    candidates.next().is_none().then_some(first.0)
                } else {
                    candidates.nth(*ordinal).map(|(index, _)| index)
                }
            }
        }
    }
}

fn open_snapshot(
    source: &crate::history::SourceText,
    reasoning: bool,
) -> crate::history::SourceText {
    let marker = if reasoning {
        "\n\n… [reasoning preview limited]"
    } else {
        "\n\n… [Live Markdown preview limited; full response available in history after completion]"
    };
    if reasoning {
        source.reasoning_preview(crate::messages::LIVE_MARKDOWN_BYTES, marker)
    } else {
        source.preview(crate::messages::LIVE_MARKDOWN_BYTES, marker)
    }
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
        position: None,
        message_id: None,
        seq: i64::MAX,
        role: String::new(),
        text: format!(
            "{} {}{}{} ({}){}",
            card.name, card.state, files, output, card.op, facts
        )
        .into(),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        // The list text may ellipsize; the existing detail viewer still needs
        // bounded owner facts without another query or a parallel store.
        tool: Some(std::sync::Arc::new(card.clone())),
        child_notice: None,
        shell_notice: None,
    }
}

/// Assistant footer row: no text, no reasoning, footer metadata only; the
/// upstream footer follows every part of the assistant message
/// (`routes/session/index.tsx:1934-1985`).
pub(super) fn footer_row(agent: Option<String>, meta: AssistantMeta) -> HistoryRow {
    HistoryRow {
        position: None,
        message_id: None,
        seq: i64::MAX,
        role: "assistant".to_string(),
        text: String::new().into(),
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

// Layout padding is a separate unstyled span, also on wrapped list rows.
fn selectable_start(line: &Line) -> usize {
    match line.spans() {
        [padding, content, ..]
            if padding.style() == ratatui::style::Style::default()
                && !padding.content().is_empty()
                && padding.content().bytes().all(|byte| byte == b' ')
                && !content.content().is_empty() =>
        {
            padding.content().len()
        }
        _ => 0,
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
        self.tool_spinner_clocks_attached = true;
        let _ = self.execute_capture();
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
        self.inline_errors_expanded.clear();
        self.instructions_expanded.clear();
        self.reasoning_down = None;
        self.reasoning_expanded.clear();
        self.viewport.set(None);
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.window.reset(page);
        self.execute_calls_expanded.retain(|operation, calls| {
            if let Some(facts) = self.window.rows().iter().find_map(|row| {
                row.tool
                    .as_ref()
                    .filter(|card| &card.op == operation)
                    .and_then(|card| crate::tools::execute::facts(card))
            }) {
                calls.retain(|index| *index < facts.calls.len());
                !calls.is_empty()
            } else {
                false
            }
        });
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
        let revision = self.window.revision();
        if let Some(turn) = &self.active_turn {
            self.window.correlate_live_prompt(page, &turn.0);
        }
        let notices = self.window.refresh_owner_notices(page);
        self.window.refresh_notice_orders(page);
        if notices || revision != self.window.revision() {
            self.invalidate_transcript();
        }
    }

    /// Project an exact owner-qualified notice after any history replacement.
    /// It can update a retained launch outside the sixteen-job inventory.
    pub fn apply_child_notice(&mut self, receipt: &oc_core::queries::ChildJob) {
        if self.session.as_ref() != Some(&receipt.parent) {
            return;
        }
        let mut changed =
            self.children
                .apply_notice(receipt, &mut self.window, self.picker.as_ref());
        for part in &mut self.live_parts {
            if let LivePart::Tool { card, .. } = part {
                changed |= crate::history::edit_card(
                    card,
                    |card| matches!(card.render, crate::tools::ToolRender::Subagent(_)),
                    |card| {
                        self.children
                            .apply_live_notice(receipt, card, self.picker.as_ref())
                    },
                );
            }
        }
        if changed {
            self.enforce_parts();
        }
        self.sync_foreground_hint(Instant::now());
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
                let shell = &snapshot.job.shell_id;
                changed |= crate::history::edit_card(
                    card,
                    |card| card.is_model_shell(shell),
                    |card| card.finish_model_shell_preview(snapshot),
                );
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
                    let turn = self.active_turn.as_ref().map(|turn| turn.0.as_str());
                    changed |= crate::history::edit_card(
                        card,
                        crate::history::ToolCard::may_refresh_model_shell,
                        |card| self.shells.refresh_live_card(session, turn, card),
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
                    changed |= crate::history::edit_card(
                        card,
                        |card| matches!(card.render, crate::tools::ToolRender::Subagent(_)),
                        |card| {
                            self.children
                                .refresh_live_card(session, card, self.picker.as_ref())
                        },
                    );
                }
            }
        }
        if changed {
            self.enforce_parts();
        }
        self.sync_foreground_hint(Instant::now());
    }

    /// Same-session TurnFinished receipt only. Explicit routing/conversation
    /// resets use attach_page. Preserve a painted part, not a total-row delta:
    /// durable projection may replace synthetic parts and the paging window.
    pub fn refresh_completed_page(&mut self, page: &HistoryPage) {
        self.tool_spinner_clocks_attached = true;
        let _ = self.execute_capture();
        if let Some(job) = page.child_job.as_deref()
            && self.attached_session() == Some(&job.child)
        {
            self.refresh_linked_child(job.clone());
        }
        self.remember_compaction_turns(page);
        let view = self.viewport.get().filter(|_| self.scroll > 0);
        let (old_rows, old_live, _) = self.compose_structure(true);
        let anchor = view
            .and_then(|_| self.projection_work.borrow().painted_top.clone())
            .filter(|top| top.requested_scroll == self.scroll)
            .map(|top| (top.part, top.row));
        let expanded: Vec<_> = old_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let identity = row.reasoning.as_ref()?.identity?;
                self.reasoning_expanded
                    .contains(&identity)
                    .then(|| CompletionPart::capture(&old_rows, index, old_live == Some(index)))
            })
            .collect();
        drop(old_rows);
        self.reverted = page.reverted.clone();
        self.parent_id = page.parent_id.clone();
        self.session_title = page.title.clone();
        self.clear_transcript_selection();
        self.window.refresh_completed(page, self.scroll > 0);
        self.refresh_running_user_shell_output();
        self.invalidate_transcript();
        self.completion_anchor.get_mut().take();
        let (rows, _, _) = self.compose_structure(true);
        self.reasoning_expanded = expanded
            .iter()
            .filter_map(|part| {
                let index = part.resolve(&rows)?;
                rows[index].reasoning.as_ref()?.identity
            })
            .collect();
        if let Some((part, row_offset)) = anchor {
            let matched = part.resolve(&rows);
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
                    component: rows[index]
                        .position
                        .as_ref()
                        .map(|position| position.component.clone()),
                    row: row_offset,
                    requested_scroll: self.scroll,
                    pending: true,
                });
            }
        }
        // Keep the painted geometry for sticky-bottom and fallback clamping.
        // The semantic anchor resolves against the new cached part positions.
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
    #[cfg(test)]
    pub(crate) fn paint_transcript_at(
        &self,
        area: Rect,
        rows: &[Line],
        total: usize,
        scroll: usize,
        frame: Option<Rect>,
        user_targets: &[Option<crate::messages::UserMessageTarget>],
    ) -> Vec<Line> {
        self.paint_transcript_rows(area, rows, total, scroll, frame, user_targets)
            .into_iter()
            .map(std::borrow::Cow::into_owned)
            .collect()
    }

    /// Painted rows borrowing every unchanged source row; only hovered,
    /// selected or animated rows are copied.
    pub(crate) fn paint_transcript_rows<'a>(
        &self,
        area: Rect,
        rows: &'a [Line],
        total: usize,
        scroll: usize,
        frame: Option<Rect>,
        user_targets: &[Option<crate::messages::UserMessageTarget>],
    ) -> Vec<std::borrow::Cow<'a, Line>> {
        use std::borrow::Cow;
        // Rows of no projection (loading) carry no hover. A projection still
        // painting while its successor builds keeps its own hover regions, so
        // live output does not blink the hovered block off on every update.
        if let Some(frame) = frame
            && !self.projection_painted((area.width, frame.width, area.height))
        {
            if self.painted_transcript.borrow_mut().take().is_some() {
                self.paint_generation
                    .set(self.paint_generation.get().wrapping_add(1));
            }
            return rows.iter().map(Cow::Borrowed).collect();
        }
        if let Some(view) = self.visible_projection.borrow().as_ref()
            && same_rows(&view.lines, rows)
            && view.total == total
            && view.scroll == scroll
        {
            self.projection_work.borrow_mut().painted_top = view.top.clone();
        }
        let theme = Theme::dark();
        // Hover targets of the painted projection (the caller's are empty while
        // a newer projection builds; those stay strict for clicks).
        let painted_targets = self.visible_projection.borrow().as_ref().and_then(|view| {
            (same_rows(&view.lines, rows) && view.targets.len() >= rows.len())
                .then(|| view.targets.clone())
        });
        let hover_targets = painted_targets.as_deref().unwrap_or(user_targets);
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
            hover_targets.get((y - area.y) as usize).cloned().flatten()
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
            match self.painted_transcript_hit(frame, x, y) {
                Some(crate::messages::TranscriptHit::Exploration(_, range)) => Some(range),
                _ => None,
            }
        });
        // Derive child hover from this frame's unhovered geometry, including a
        // just-expanded row. Navigation captures only its painted label cells.
        {
            let projection = self.visible_projection.borrow();
            let current = projection.as_ref().filter(|view| {
                same_rows(&view.lines, rows)
                    && view.total == total
                    && view.scroll == scroll
                    && view.dimensions.0 == area.width
                    && view.dimensions.2 == area.height
            });
            let children = current
                .map(|view| view.child_targets.clone())
                .unwrap_or_default();
            let mut painted = self.painted_transcript.borrow_mut();
            if !painted.as_ref().is_some_and(|previous| {
                previous.area == area
                    && previous.total == total
                    && previous.scroll == scroll
                    && same_rows(&previous.rows, rows)
                    && previous.user_targets == user_targets
                    && previous.child_targets == children
            }) {
                self.paint_generation
                    .set(self.paint_generation.get().wrapping_add(1));
                *painted = Some(PaintedTranscript {
                    area,
                    rows: current.map_or_else(|| rows.into(), |view| view.lines.clone()),
                    user_targets: user_targets.to_vec(),
                    child_targets: children,
                    total,
                    scroll,
                });
            }
        }
        let hover_pending = frame.and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            if owner != frame
                || self.panel != TuiPanel::None
                || self.approvals.active().is_some()
                || self.questions.active().is_some()
                || self.composer_contains(frame, (x, y))
                || self.shell_viewer().is_some()
                || !area.contains((x, y).into())
                || self.transcript_overpainted(frame, x, y)
            {
                return None;
            }
            match self.painted_transcript_hit(frame, x, y) {
                Some(crate::messages::TranscriptHit::PendingSubagentHover(range)) => Some(range),
                _ => None,
            }
        });
        let hover_child = frame.filter(|_| hover_pending.is_none()).and_then(|frame| {
            let (x, y, owner) = self.last_mouse?;
            (owner == frame).then_some(())?;
            self.child_notice_hit(frame, x, y, true)
        });
        let painted = self.painted_transcript.borrow();
        let selected = self.selection.as_ref().filter(|selected| {
            self.panel == TuiPanel::None
                && painted
                    .as_ref()
                    .is_some_and(|current| self.transcript_selection_current(selected, current))
        });
        let mut painted_rows: Vec<Cow<'a, Line>> = rows
            .iter()
            .enumerate()
            .map(|(row, line)| {
                let hovered;
                let overlaid = if hovered_user.is_some()
                    && hover_targets.get(row).cloned().flatten() == hovered_user
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
                } else if hover_pending
                    .as_ref()
                    .is_some_and(|range| range.contains(&row))
                {
                    hovered = crate::messages::hover_subagent_content(line, theme);
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
                            crate::messages::CapturedChildTarget::ErrorDetails(_)
                            | crate::messages::CapturedChildTarget::InlineError(_)
                            | crate::messages::CapturedChildTarget::Instructions(_) => line.clone(),
                            crate::messages::CapturedChildTarget::Execute { .. } => {
                                crate::tools::execute::hover(line, theme)
                            }
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
                let line = overlaid;
                let copied = !std::ptr::eq(line, &rows[row]);
                if let Some(selected) = selected
                    && let Some(source_row) = (total
                        .saturating_sub(area.height as usize)
                        .saturating_sub(scroll)
                        + row)
                        .checked_sub(selected.painted.top())
                    && selected.painted.rows.get(source_row) == rows.get(row)
                {
                    let (start, end) = selected.bounds();
                    let begin = if source_row == start.row {
                        start.byte
                    } else if source_row > start.row {
                        0
                    } else {
                        line.plain_text().len()
                    };
                    let finish = if source_row == end.row {
                        end.byte
                    } else if source_row < end.row {
                        line.plain_text().len()
                    } else {
                        0
                    };
                    let begin = begin.max(selectable_start(line));
                    if begin < finish && !line.plain_text().trim().is_empty() {
                        return Cow::Owned(line.highlight(
                            begin,
                            finish,
                            theme.text(),
                            theme.background(),
                        ));
                    }
                }
                if copied {
                    Cow::Owned(line.clone())
                } else {
                    Cow::Borrowed(&rows[row])
                }
            })
            .collect();
        if let Some(view) = self.visible_projection.borrow().as_ref().filter(|view| {
            same_rows(&view.lines, rows) && view.total == total && view.scroll == scroll
        }) {
            for (row, byte, source) in &view.animations {
                if let Some(line) = painted_rows.get_mut(*row) {
                    line.to_mut()
                        .patch_cell(*byte, self.animation_glyph(source));
                }
            }
        }
        painted_rows
    }

    fn animation_glyph(&self, source: &crate::styled::AnimationSource) -> &'static str {
        use crate::styled::AnimationSource;
        if self.chrome.animations == Some(false)
            || (!self.tool_spinner_clocks_attached
                && matches!(
                    source,
                    AnimationSource::Tool(_) | AnimationSource::Reasoning
                ))
        {
            return crate::tools::SPINNER;
        }
        let frame = match source {
            AnimationSource::Tool(op) => self
                .tool_spinner_clocks
                .get(op)
                .map_or(0, |clock| clock.frame),
            AnimationSource::Delegation(slot) => slot
                .checked_sub(self.live_part_offset)
                .and_then(|index| self.live_parts.get(index))
                .and_then(|part| match part {
                    LivePart::Tool {
                        delegation: Some(clock),
                        ..
                    } => Some(clock.frame),
                    _ => None,
                })
                .unwrap_or(0),
            AnimationSource::Reasoning => self
                .reasoning_spinner_clock
                .as_ref()
                .map_or(0, |clock| clock.frame),
            AnimationSource::Compaction => self.compaction_frame,
        };
        crate::compaction::FRAMES[frame]
    }

    /// Configuration/test override of the platform's native clipboard mode.
    pub fn set_clipboard_mode(&mut self, mode: ClipboardMode) {
        if self.clipboard_mode != mode {
            self.cancel_text_selection();
        }
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
        self.cancel_text_selection();
        self.clear_transcript_selection();
        self.apply_owner_clipboard_mode(mode);
    }

    pub fn disable_clipboard_until_catalog(&mut self) {
        self.cancel_text_selection();
        self.clear_transcript_selection();
        self.clipboard_mode = ClipboardMode::Disabled;
    }

    pub(super) fn apply_owner_clipboard_mode(
        &mut self,
        mode: Option<oc_core::queries::TerminalCopyMode>,
    ) {
        self.owner_clipboard_mode = mode;
        let previous = self.clipboard_mode;
        self.chrome.terminal_copy = mode;
        self.clipboard_mode = match mode {
            Some(oc_core::queries::TerminalCopyMode::Select) => ClipboardMode::Select,
            Some(oc_core::queries::TerminalCopyMode::Manual) => ClipboardMode::Manual,
            None => ClipboardMode::default(),
        };
        if previous != self.clipboard_mode {
            self.cancel_text_selection();
        }
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
        self.cancel_text_selection();
        self.input = text;
        self.editor.clear();
        self.editor.cursor = self.input.len();
        self.input_revision += 1;
    }

    pub fn restore_prompt_input(&mut self, prompt: oc_core::prompt::PromptInput) {
        self.restore_prompt(prompt.text);
        self.editor.restore_images(&self.input, prompt.images);
    }

    pub fn composer_draft(&self) -> crate::ComposerDraft {
        crate::ComposerDraft {
            text: self.input.clone(),
            editor: self.editor.clone(),
        }
    }

    pub fn restore_composer_draft(&mut self, draft: crate::ComposerDraft) {
        self.cancel_text_selection();
        self.input = draft.text;
        self.editor = draft.editor;
        self.input_revision += 1;
    }

    pub fn conversation_applied(
        &mut self,
        snapshot: &oc_core::queries::ConversationSnapshot,
        page: &HistoryPage,
    ) {
        self.projection_epoch = self.projection_epoch.wrapping_add(1);
        let draft = snapshot
            .draft
            .clone()
            .map(|text| oc_core::prompt::PromptInput {
                text,
                images: snapshot.images.clone(),
            })
            .or_else(|| {
                if matches!(
                    dispatch(self.input.trim()),
                    Some(CommandAction::UndoConversation | CommandAction::RedoConversation)
                ) {
                    Some(oc_core::prompt::PromptInput::default())
                } else {
                    None
                }
            });
        self.set_session(snapshot.session.clone());
        self.attach_page(page);
        if let Some(draft) = draft {
            self.restore_prompt_input(draft);
        }
        self.conversation_available = Some((snapshot.can_undo, snapshot.can_redo));
        self.reverted = snapshot.reverted.clone();
        self.dcp = DcpPanelState::default();
    }

    /// The binary reports the actual asynchronous clipboard write outcome.
    pub fn report_clipboard_result(&mut self, result: Result<(), String>) {
        if let Some(detail) = &mut self.execute_detail
            && let Some(key) = detail.copy_pending.take()
        {
            if result.is_ok() {
                detail.copied = Some(key);
            }
            if let Err(error) = result {
                self.push_transient_note(&error, NoteVariant::Error);
            }
            return;
        }
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

    pub(super) fn retire_reverted_boundary(&mut self) {
        if self.reverted.take().is_some() {
            self.clear_transcript_selection();
            self.invalidate_transcript();
        }
    }

    pub(crate) fn clear_transcript_selection(&mut self) {
        self.cancel_transcript_capture();
        self.child_notice_down = None;
        self.reverted_down = None;
        self.message_down = None;
        self.paint_generation
            .set(self.paint_generation.get().wrapping_add(1));
        self.selection = None;
        self.selection_gesture = false;
        self.click = None;
        if !self.has_new_text_selection() {
            self.pending_copy = None;
        }
        *self.painted_transcript.borrow_mut() = None;
    }

    fn transcript_selection_current(
        &self,
        selected: &TranscriptSelection,
        current: &PaintedTranscript,
    ) -> bool {
        selected.transcript_revision == self.transcript_revision
            && selected.window_revision == self.window.revision()
            && selected.painted.area == current.area
            && selected.painted.total == current.total
            && current.rows.iter().enumerate().all(|(row, line)| {
                (current.top() + row)
                    .checked_sub(selected.painted.top())
                    .and_then(|row| selected.painted.rows.get(row))
                    .is_none_or(|old| old == line)
            })
    }

    fn release_transcript_scroll_press(&mut self) {
        if self
            .selection
            .as_ref()
            .is_some_and(|selected| selected.dragging)
        {
            self.clear_transcript_selection();
            return;
        }
        self.cancel_transcript_capture();
        self.selection_gesture = false;
        self.click = None;
        self.child_notice_down = None;
        self.reverted_down = None;
        self.message_down = None;
        self.paint_generation
            .set(self.paint_generation.get().wrapping_add(1));
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
        if !self.transcript_selection_current(selected, current) {
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
            // A whitespace-only painted spacer has no selectable content.
            if text.trim().is_empty() {
                continue;
            }
            let begin = if row == start.row { start.byte } else { 0 };
            let begin = begin.max(selectable_start(line));
            let finish = if row == end.row { end.byte } else { text.len() };
            if begin >= finish {
                continue;
            }
            if !text.is_char_boundary(begin) || !text.is_char_boundary(finish) {
                return Ok(None);
            }
            let addition = finish - begin + usize::from(!result.is_empty());
            if result.len().saturating_add(addition) > MAX_SELECTION_BYTES {
                return Err(TOO_LARGE);
            }
            if !result.is_empty() {
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
        if !(self
            .painted_transcript
            .borrow()
            .as_ref()
            .is_some_and(|painted| {
                painted.total == total && painted.scroll == scroll && painted.rows[..] == rows[..]
            })
            || matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
                && event.modifiers.is_empty()
                && self
                    .child_navigation_hit(area, event.column, event.row)
                    .is_some())
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
                    transcript_revision: self.transcript_revision,
                    window_revision: self.window.revision(),
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
        match self.cached_transcript_hit(area, x, y) {
            Some(crate::messages::TranscriptHit::Exploration(op, range)) => Some((op, range)),
            _ => None,
        }
    }

    fn cached_transcript_hit(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::TranscriptHit> {
        self.transcript_hit_in(frame, x, y, false)
    }

    /// The hit in the painted projection, also while a newer one builds:
    /// for hover paint only, never for actions.
    fn painted_transcript_hit(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<crate::messages::TranscriptHit> {
        self.transcript_hit_in(frame, x, y, true)
    }

    fn transcript_hit_in(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
        painted: bool,
    ) -> Option<crate::messages::TranscriptHit> {
        let rect = crate::shell::transcript_area(self, frame);
        if !rect.contains((x, y).into()) {
            return None;
        }
        self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let dimensions = (rect.width, frame.width, rect.height);
        if !(self.projection_ready(dimensions) || painted && self.projection_painted(dimensions)) {
            return None;
        }
        let view = self.visible_projection.borrow();
        crate::messages::transcript_hit(
            &view.as_ref()?.hits,
            (x - rect.x) as usize,
            (y - rect.y) as usize,
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
        match self.cached_transcript_hit(area, x, y) {
            Some(crate::messages::TranscriptHit::Reasoning(id)) => Some(id),
            _ => None,
        }
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
            self.tab_rail_width(),
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

    fn child_navigation_hit(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
    ) -> Option<(
        crate::messages::CapturedChildTarget,
        std::ops::Range<usize>,
        bool,
    )> {
        let rect = crate::shell::transcript_area(self, frame);
        if self.panel != TuiPanel::None
            || self.approvals.active().is_some()
            || self.questions.active().is_some()
            || self.composer_contains(frame, (x, y))
            || self.shell_viewer().is_some()
            || self.transcript_overpainted(frame, x, y)
            || !rect.contains((x, y).into())
        {
            return None;
        }
        self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let painted = self.painted_transcript.borrow();
        let painted = painted.as_ref().filter(|painted| painted.area == rect)?;
        let point = ((x - rect.x) as usize, (y - rect.y) as usize);
        let down = painted
            .child_targets
            .iter()
            .find(|hit| hit.contains(point.0, point.1))?;
        let current = self.visible_projection.borrow();
        let hit = current.as_ref()?.child_targets.iter().find(|hit| {
            hit.contains(point.0, point.1)
                && hit.tool == down.tool
                && crate::messages::same_child_launch(&hit.job, &down.job)
        })?;
        (self.attached_session() == Some(&hit.job.parent)).then(|| {
            (
                crate::messages::CapturedChildTarget::Navigate(hit.job.clone()),
                hit.range.clone(),
                hit.tool,
            )
        })
    }

    /// No-drag navigation owns its release before text selection rejects an
    /// unrelated repaint. Drag/multiclick still belongs to selection.
    pub(super) fn release_child_navigation(
        &mut self,
        event: MouseEvent,
        frame: Rect,
    ) -> Option<KeyOutcome> {
        if matches!(event.kind, MouseEventKind::Up(MouseButton::Left))
            && self.child_notice_down.as_ref().is_some_and(|pressed| {
                matches!(
                    pressed.target,
                    crate::messages::CapturedChildTarget::Navigate(_)
                )
            })
            && !self
                .selection
                .as_ref()
                .is_some_and(|selection| selection.dragging)
            && self.click.is_none_or(|click| click.count == 1)
        {
            let outcome = self.child_notice_mouse(event, frame);
            if outcome.is_some() {
                self.finish_transcript_click();
            }
            return outcome;
        }
        None
    }

    /// `painted`: for hover paint, from the painted projection even while a
    /// newer one builds; actions require the current projection.
    fn child_notice_hit(
        &self,
        frame: Rect,
        x: u16,
        y: u16,
        painted: bool,
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
                    .is_some_and(|card| matches!(card.render,crate::tools::ToolRender::Subagent(_) | crate::tools::ToolRender::Inline(crate::tools::InlineRender::Execute)) || crate::tools::inline_error_expandable(card) || card.output_presentation.as_ref().is_some_and(|p| p.instructions.is_some()))
        }) && !self
            .live_parts
            .iter()
            .any(|part| matches!(part,LivePart::Tool {card,..} if matches!(card.render,crate::tools::ToolRender::Subagent(_) | crate::tools::ToolRender::Inline(crate::tools::InlineRender::Execute)) || crate::tools::inline_error_expandable(card) || card.output_presentation.as_ref().is_some_and(|p| p.instructions.is_some())))
            || self.panel != TuiPanel::None
            || self.approvals.active().is_some()
            || self.questions.active().is_some()
            || self.composer_contains(frame, (x, y))
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
                    && painted.rows[..] == visible[..]
            })
        {
            return None;
        }
        let hit = if painted {
            self.painted_transcript_hit(frame, x, y)
        } else {
            self.cached_transcript_hit(frame, x, y)
        };
        match hit {
            Some(crate::messages::TranscriptHit::ChildNotice(target, range, tool)) => {
                Some((target, range, tool))
            }
            _ => None,
        }
        .filter(|(target, _, _)| match target {
            crate::messages::CapturedChildTarget::Navigate(job) => {
                self.attached_session() == Some(&job.parent)
            }
            crate::messages::CapturedChildTarget::ErrorDetails(_) => true,
            crate::messages::CapturedChildTarget::InlineError(_) => true,
            crate::messages::CapturedChildTarget::Instructions(_) => true,
            crate::messages::CapturedChildTarget::Execute { .. } => true,
        })
    }

    pub(super) fn child_notice_mouse(
        &mut self,
        event: MouseEvent,
        frame: Rect,
    ) -> Option<KeyOutcome> {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.child_notice_down = self
                    .child_navigation_hit(frame, event.column, event.row)
                    .or_else(|| self.child_notice_hit(frame, event.column, event.row, false))
                    .map(|(target, _, tool)| ChildNoticePress {
                        target,
                        tool,
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
                    || self.click.is_some_and(|click| click.count != 1)
                    || !matches!(self.selection_text(), Ok(None))
                {
                    return None;
                }
                let navigation = matches!(
                    pressed.target,
                    crate::messages::CapturedChildTarget::Navigate(_)
                );
                if !navigation && pressed.generation != self.paint_generation.get() {
                    return None;
                }
                let (target, _, tool) = if navigation {
                    self.child_navigation_hit(frame, event.column, event.row)?
                } else {
                    self.child_notice_hit(frame, event.column, event.row, false)?
                };
                let same = match (&target, &pressed.target) {
                    (
                        crate::messages::CapturedChildTarget::Navigate(current),
                        crate::messages::CapturedChildTarget::Navigate(previous),
                    ) => crate::messages::same_child_launch(current, previous),
                    _ => target == pressed.target,
                };
                if !same || tool != pressed.tool {
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
                    crate::messages::CapturedChildTarget::InlineError(operation) => {
                        self.toggle_inline_error_at(&operation, frame);
                        KeyOutcome::default()
                    }
                    crate::messages::CapturedChildTarget::Instructions(operation) => {
                        self.toggle_instructions_at(&operation, frame);
                        KeyOutcome::default()
                    }
                    crate::messages::CapturedChildTarget::Execute { operation, call } => {
                        self.activate_execute(&operation, call, frame);
                        KeyOutcome::default()
                    }
                });
            }
            _ => {}
        }
        None
    }

    pub(super) fn tool_expanded(&self, target: crate::messages::ToolExpansion<'_>) -> bool {
        match target {
            crate::messages::ToolExpansion::Operation(op) => self.exploration_expanded.contains(op),
            crate::messages::ToolExpansion::InlineError(op) => {
                self.inline_errors_expanded.contains(op)
            }
            crate::messages::ToolExpansion::Instructions(op) => {
                self.instructions_expanded.contains(op)
            }
            crate::messages::ToolExpansion::ExecuteCall(op, index) => self
                .execute_calls_expanded
                .get(op)
                .is_some_and(|calls| calls.contains(&index)),
        }
    }

    fn toggle_inline_error_at(&mut self, operation: &str, frame: Rect) {
        let rect = crate::shell::transcript_area(self, frame);
        let (_, before, displayed) =
            self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let first = before
            .saturating_sub(usize::from(rect.height))
            .saturating_sub(displayed);
        let (rows, _, _) = self.compose_structure(true);
        self.inline_errors_expanded.retain(|id| {
            rows.iter()
                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
        });
        if !self.inline_errors_expanded.insert(operation.to_owned()) {
            self.inline_errors_expanded.remove(operation);
        }
        self.invalidate_transcript();
        let (_, after) = self.visible_transcript(rect.width, frame.width, rect.height);
        if !self.projection_ready((rect.width, frame.width, rect.height)) {
            return;
        }
        self.scroll = if displayed == 0 && !self.window.has_newer() {
            0
        } else {
            after
                .saturating_sub(usize::from(rect.height))
                .saturating_sub(first)
        };
        self.observe_transcript_viewport(rect.width, frame.width, rect.height, after, self.scroll);
    }

    fn toggle_instructions_at(&mut self, operation: &str, frame: Rect) {
        let rect = crate::shell::transcript_area(self, frame);
        let (_, before, displayed) =
            self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let first = before
            .saturating_sub(usize::from(rect.height))
            .saturating_sub(displayed);
        let (rows, _, _) = self.compose_structure(true);
        self.instructions_expanded.retain(|id| {
            rows.iter()
                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
        });
        if !self.instructions_expanded.insert(operation.to_owned()) {
            self.instructions_expanded.remove(operation);
        }
        self.invalidate_transcript();
        let (_, after) = self.visible_transcript(rect.width, frame.width, rect.height);
        if !self.projection_ready((rect.width, frame.width, rect.height)) {
            return;
        }
        self.scroll = if displayed == 0 && !self.window.has_newer() {
            0
        } else {
            after
                .saturating_sub(usize::from(rect.height))
                .saturating_sub(first)
        };
        self.observe_transcript_viewport(rect.width, frame.width, rect.height, after, self.scroll);
    }

    pub(super) fn toggle_tool_expansion_at(&mut self, operation: &str, frame: Rect) {
        let rect = crate::shell::transcript_area(self, frame);
        let height = rect.height as usize;
        let (_, before, displayed) =
            self.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
        let first = before.saturating_sub(height).saturating_sub(displayed);
        let (rows, _, _) = self.compose_structure(true);
        // GenericTool keeps ScrollBox sticky follow; the Explored group has
        // a separate header-anchor interaction that must remain unchanged.
        let following = displayed == 0
            && !self.window.has_newer()
            && rows.iter().any(|row| {
                row.tool.as_ref().is_some_and(|card| {
                    card.op == operation
                        && matches!(
                            &card.render,
                            crate::tools::ToolRender::Inline(
                                crate::tools::InlineRender::Generic { .. }
                            )
                        )
                })
            });
        self.exploration_expanded.retain(|id| {
            rows.iter()
                .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == id))
        });
        if !self.exploration_expanded.insert(operation.to_owned()) {
            self.exploration_expanded.remove(operation);
        }
        let (_, after) = self.visible_transcript(rect.width, frame.width, rect.height);
        if !self.projection_ready((rect.width, frame.width, rect.height)) {
            return;
        }
        self.scroll = if following {
            0
        } else {
            after.saturating_sub(height).saturating_sub(first)
        };
        self.observe_transcript_viewport(rect.width, frame.width, rect.height, after, self.scroll);
    }

    pub(super) fn prune_reasoning(&mut self) {
        let retained: BTreeSet<_> = self
            .compose_structure(true)
            .0
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

    pub(super) fn canonical_history_ahead(&self) -> bool {
        self.active_turn.as_ref().is_some_and(|turn| {
            self.window.rows().iter().any(|row| {
                row.position.as_ref().is_some_and(|position| {
                    position.belongs_to(&turn.0)
                        && matches!(position.component, crate::history::HistoryComponent::Part(sequence)
                            if self.live_checkpoint_end.is_none_or(|end| sequence >= end))
                })
            })
        })
    }

    /// Borrow canonical/live winners, including non-tool replacements, before
    /// applying actual-running eligibility. This never clones transcript rows.
    pub(super) fn effective_tool_cards(&self) -> BTreeMap<&str, &ToolCard> {
        let mut cards: Vec<_> = self
            .window
            .rows()
            .iter()
            .map(|row| {
                let sequence = row.position.as_ref().and_then(|position| {
                    if self
                        .active_turn
                        .as_ref()
                        .is_some_and(|turn| position.belongs_to(&turn.0))
                        && let crate::history::HistoryComponent::Part(sequence) = position.component
                    {
                        Some(sequence)
                    } else {
                        None
                    }
                });
                (sequence, row.tool.as_ref())
            })
            .collect();
        if !self.canonical_history_ahead() {
            for (ordinal, part) in self.live_parts.iter().enumerate() {
                if matches!(part, LivePart::Vacant) {
                    continue;
                }
                let source = self
                    .live_part_sources
                    .get(&(self.live_part_offset + ordinal));
                let card = match part {
                    LivePart::Tool { card, .. } => Some(card),
                    _ => None,
                };
                if let Some(index) = source.and_then(|source| {
                    cards
                        .iter()
                        .position(|(sequence, _)| *sequence == Some(*source))
                }) {
                    cards[index].1 = card;
                } else {
                    cards.push((None, card));
                }
            }
        }
        cards
            .into_iter()
            .filter_map(|(_, card)| card.map(|card| (card.op.as_str(), &**card)))
            .collect()
    }

    /// Render rows of the transcript: the bounded window plus the live parts
    /// (frozen text/reasoning segments and tool cards) and the open live
    /// answer (reasoning block and streaming text) while a turn is active.
    pub fn transcript_rows(&self) -> Vec<HistoryRow> {
        self.compose_transcript().0
    }

    pub(super) fn compose_transcript(&self) -> (Vec<HistoryRow>, Option<usize>) {
        let (mut rows, live, runs) = self.compose_structure(false);
        projection::finish_reasoning(
            &mut rows,
            &runs,
            self.thinking_expanded,
            &self.reasoning_expanded,
        );
        self.reasoning_spinner_running.set(
            self.tool_spinner_clocks_attached && crate::messages::has_running_reasoning(&rows),
        );
        (rows, live)
    }

    pub(super) fn compose_structure(
        &self,
        bounded_open: bool,
    ) -> (
        Vec<HistoryRow>,
        Option<usize>,
        Vec<Vec<crate::messages::ReasoningIdentity>>,
    ) {
        #[cfg(test)]
        self.transcript_row_copies
            .set(self.transcript_row_copies.get() + self.window.len());
        // Collapse adjacent effort choices only in the render copy, retaining
        // every durable row and paging identity in the history window.
        let mut rows: Vec<_> = self
            .window
            .rows()
            .iter()
            .enumerate()
            .filter(|(index, row)| {
                row.role != "variant_switch"
                    || self
                        .window
                        .rows()
                        .get(index + 1)
                        .is_none_or(|next| next.role != "variant_switch")
            })
            .map(|(_, row)| row.clone())
            .collect();
        let canonical_ahead = self.canonical_history_ahead();
        if let Some(turn) = &self.active_turn {
            rows.retain(|row| {
                !row.position.as_ref().is_some_and(|position| {
                    position.belongs_to(&turn.0)
                        && position.component == crate::history::HistoryComponent::Footer
                })
            });
        }
        let mut live_sources = vec![None; rows.len()];
        for (ordinal, part) in self.live_parts.iter().enumerate() {
            if matches!(part, LivePart::Vacant) {
                continue;
            }
            let slot = self.live_part_offset + ordinal;
            let source = self.live_part_sources.get(&slot).copied();
            let canonical = rows.iter().position(|row| {
                row.position.as_ref().is_some_and(|position| {
                    self.active_turn.as_ref().is_some_and(|turn| position.belongs_to(&turn.0))
                        && matches!(position.component, crate::history::HistoryComponent::Part(sequence)
                            if Some(sequence) == source)
                })
            });
            // A history read can overtake queued hints. Its canonical identities
            // win until the ordered checkpoint arrives; never compare payloads
            // to guess which repeated text a hint belongs to.
            if canonical_ahead {
                continue;
            }
            let mut row = part.to_row(
                self.active_agent.clone(),
                Some(crate::messages::ReasoningIdentity::Live(
                    self.reasoning_epoch,
                    self.live_part_offset + ordinal,
                )),
            );
            if let LivePart::Tool {
                delegation: Some(clock),
                ..
            } = part
                && let Some(card) = row.tool.as_mut()
                && let crate::tools::ToolRender::Subagent(view) =
                    &mut std::sync::Arc::make_mut(card).render
            {
                view.spinner = Some(if self.chrome.animations == Some(false) {
                    "⋯"
                } else {
                    crate::compaction::FRAMES[clock.frame]
                });
            }
            if let Some(index) = canonical {
                row.position.clone_from(&rows[index].position);
                row.message_id.clone_from(&rows[index].message_id);
                row.seq = rows[index].seq;
                row.agent.clone_from(&rows[index].agent);
                row.agent_color_index = rows[index].agent_color_index;
                rows[index] = row;
                live_sources[index] = source.or(Some(slot));
            } else {
                rows.push(row);
                live_sources.push(source.or(Some(slot)));
            }
        }
        let live =
            !canonical_ahead && (!self.live_text.is_empty() || !self.live_reasoning.is_empty());
        if live {
            rows.push(HistoryRow {
                position: None,
                seq: i64::MAX,
                message_id: None,
                role: "assistant".to_string(),
                text: if bounded_open {
                    open_snapshot(&self.live_text, false)
                } else {
                    self.live_text.clone()
                },
                agent: self.active_agent.clone(),
                agent_color_index: self.live_agent_color_index,
                chips: Vec::new(),
                reasoning: (!self.live_reasoning.is_empty()).then(|| ReasoningBlock {
                    text: if bounded_open {
                        open_snapshot(&self.live_reasoning, true)
                    } else {
                        self.live_reasoning.clone()
                    },
                    duration_ms: None,
                    running: true,
                    expanded: false,
                    toggleable: true,
                    spinner: None,
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
            live_sources.push(Some(self.live_part_offset + self.live_parts.len()));
        }
        let mut live_row = live.then(|| rows.len() - 1);
        self.place_child_notices(&mut rows, &live_sources, &mut live_row);
        // Keep the pre-compaction run order. Visibility/cleanup is worker work.
        let mut runs: Vec<Vec<crate::messages::ReasoningIdentity>> = Vec::new();
        let mut adjacent = false;
        for row in &rows {
            let member = crate::messages::reasoning_group_member(row);
            if !member || !adjacent {
                runs.push(Vec::new());
            }
            if let Some(id) = row
                .reasoning
                .as_ref()
                .and_then(|reasoning| reasoning.identity)
            {
                runs.last_mut().expect("reasoning run").push(id);
            }
            adjacent = member;
        }
        runs.retain(|run| !run.is_empty());
        if let Some(reverted) = &self.reverted
            && !self.window.has_newer()
        {
            rows.push(HistoryRow {
                position: None,
                message_id: Some(std::sync::Arc::new(reverted.message.clone())),
                seq: i64::MAX,
                role: "reverted".into(),
                text: reverted.user_messages.to_string().into(),
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
        self.interleave_compactions(&mut rows, &mut live_row);
        if !self.window.has_newer() {
            rows.extend(self.inbox_rows());
            if let Some(pending) = &self.pending
                && !matches!(pending.purpose, SubmissionPurpose::Empty)
                && !pending.outstanding()
            {
                rows.push(HistoryRow {
                    position: None,
                    message_id: None,
                    seq: i64::MAX,
                    role: "request retained".into(),
                    text: pending.source.clone(),
                    agent: pending.agent.clone(),
                    agent_color_index: pending.agent_color_index,
                    chips: Vec::new(),
                    reasoning: None,
                    meta: None,
                    tool: None,
                    child_notice: None,
                    shell_notice: None,
                });
            }
        }
        if let Some(key) = self.background_hint_key() {
            rows.push(HistoryRow {
                position: None,
                message_id: None,
                seq: i64::MAX,
                role: "background_hint".into(),
                text: key.into(),
                agent: None,
                agent_color_index: None,
                chips: Vec::new(),
                reasoning: None,
                meta: None,
                tool: None,
                child_notice: None,
                shell_notice: None,
            });
        }
        let mut overlays = std::collections::HashMap::new();
        // Apply current owner configuration to the render copy, including
        // replayed/parked footer metadata. Retain every measured statistic in
        // the history/live state. Indexed footers are generated outside the
        // Markdown body cache, so config changes cannot reuse a stale footer.
        for row in &mut rows {
            if row.role == "user" {
                // OC2 UserMessage paints the current session profile, not the
                // submitted turn's historical slot. Override only this render
                // copy; message/turn ownership and generation remain recorded.
                row.agent.clone_from(&self.active_agent);
                row.agent_color_index = None;
            }
            if let Some(reasoning) = &mut row.reasoning {
                // A running group clones its first visible part, which may
                // already be completed, so supply the glyph to every part.
                reasoning.spinner = self.tool_spinner_clocks_attached.then(|| {
                    if self.chrome.animations == Some(false) {
                        crate::tools::SPINNER
                    } else {
                        crate::compaction::FRAMES[self
                            .reasoning_spinner_clock
                            .as_ref()
                            .map_or(0, |clock| clock.frame)]
                    }
                });
            }
            if let Some(card) = &mut row.tool {
                // Cards are shared with the window; write only actual changes so
                // an unchanged card keeps its identity (and its cached layout).
                if self.tool_spinner_clocks_attached && crate::tools::tool_spinner_animated(card) {
                    let frame = self
                        .tool_spinner_clocks
                        .get(&card.op)
                        .map_or(0, |clock| clock.frame);
                    let spinner = Some(if self.chrome.animations == Some(false) {
                        crate::tools::SPINNER
                    } else {
                        crate::compaction::FRAMES[frame]
                    });
                    let subagent = matches!(
                        &card.render,
                        crate::tools::ToolRender::Subagent(view) if view.spinner != spinner
                    );
                    if card.spinner != spinner || subagent {
                        let card = std::sync::Arc::make_mut(card);
                        card.spinner = spinner;
                        if let crate::tools::ToolRender::Subagent(view) = &mut card.render {
                            view.spinner = spinner;
                        }
                    }
                }
                let dcp = if let crate::tools::ToolRender::Dcp(view) = &card.render {
                    let color_index = row.agent_color_index.or(view.color_index).or_else(|| {
                        row.agent.as_ref().and_then(|id| {
                            self.agents
                                .iter()
                                .find(|entry| &entry.id == id)
                                .map(|entry| entry.color_index)
                        })
                    });
                    let spinner = (self.chrome.animations != Some(false))
                        .then(|| crate::compaction::FRAMES[self.compaction_frame].to_string());
                    (view.config != self.chrome.dcp
                        || view.color_index != color_index
                        || view.spinner != spinner)
                        .then(|| (self.chrome.dcp.clone(), color_index, spinner))
                } else {
                    None
                };
                if card.diff_settings != self.chrome.diffs || dcp.is_some() {
                    let key = OverlayKey {
                        diffs: self.chrome.diffs,
                        dcp,
                    };
                    let source = std::sync::Arc::as_ptr(card) as usize;
                    let reused = self.card_overlays.borrow().get(&source).and_then(|entry| {
                        (std::sync::Arc::ptr_eq(&entry.source, card) && entry.key == key)
                            .then(|| entry.derived.clone())
                    });
                    let original = card.clone();
                    if let Some(derived) = reused {
                        *card = derived;
                    } else {
                        let derived = std::sync::Arc::make_mut(card);
                        derived.diff_settings = key.diffs;
                        if let (
                            Some((config, color_index, spinner)),
                            crate::tools::ToolRender::Dcp(view),
                        ) = (key.dcp.clone(), &mut derived.render)
                        {
                            view.config = config;
                            view.color_index = color_index;
                            view.spinner = spinner;
                        }
                    }
                    overlays.insert(
                        source,
                        CardOverlay {
                            source: original,
                            key,
                            derived: card.clone(),
                        },
                    );
                }
            }
            if let Some(meta) = &mut row.meta {
                meta.session_tps = self.chrome.session_tps;
                // Explicit admitted profile colors override categorical slots,
                // only in this render copy. Preserve the durable generation slot
                // as the honest fallback when no valid explicit color exists.
                if row
                    .agent
                    .as_ref()
                    .and_then(|id| self.chrome.agent_colors.get(id))
                    .and_then(|hex| crate::theme::Rgba::from_hex(hex))
                    .is_some()
                {
                    meta.agent_color_index = None;
                }
            }
        }
        *self.card_overlays.borrow_mut() = overlays;
        (rows, live_row, runs)
    }

    fn place_child_notices(
        &self,
        rows: &mut Vec<HistoryRow>,
        sources: &[Option<usize>],
        live_row: &mut Option<usize>,
    ) {
        use oc_core::queries::NoticePlacement;
        // The tags move with their rows. Never infer the open Markdown row from
        // the final tail: that tail may now be a separately delivered notice.
        let mut tagged: Vec<_> = std::mem::take(rows)
            .into_iter()
            .enumerate()
            .map(|(i, row)| (row, sources[i], *live_row == Some(i)))
            .collect();
        for (turn, order) in self.window.notice_orders() {
            let Some(prompt) = tagged.iter().position(|(row, _, _)| {
                matches!(row.role.as_str(), "user" | "native_control")
                    && row.message_id.as_deref() == Some(&order.accepted_prompt)
            }) else {
                continue;
            };
            // An OC2 steered prompt is a placed user row of this turn, not a
            // turn boundary: the owner lists it among the turn's notices.
            let placed = |row: &HistoryRow| {
                row.child_notice.is_some()
                    || row.role == "user"
                        && order
                            .notices
                            .iter()
                            .any(|notice| row.message_id.as_deref() == Some(&notice.message))
            };
            let end = tagged
                .iter()
                .enumerate()
                .skip(prompt + 1)
                .find_map(|(i, (row, _, _))| {
                    ((matches!(
                        row.role.as_str(),
                        "user"
                            | "native_control"
                            | "model_switch"
                            | "variant_switch"
                            | "agent_switch"
                    ) || row.shell_notice.is_some())
                        && !placed(row))
                    .then_some(i)
                })
                .unwrap_or(tagged.len());
            let own = |entry: &(HistoryRow, Option<usize>, bool)| {
                entry
                    .0
                    .position
                    .as_ref()
                    .is_some_and(|p| p.belongs_to(turn))
                    || (self
                        .active_turn
                        .as_ref()
                        .is_some_and(|active| active.0 == turn)
                        && entry.1.is_some())
            };
            // A disjoint page or an unrelated response is a barrier, not a row
            // to skip over. No prose, child turn id or screen position is proof.
            if tagged[prompt + 1..end]
                .iter()
                .any(|entry| !placed(&entry.0) && !own(entry))
            {
                continue;
            }
            for notice in &order.notices {
                let Some(from) = tagged
                    .iter()
                    .enumerate()
                    .take(end)
                    .skip(prompt + 1)
                    .find_map(|(i, (row, _, _))| {
                        (placed(row) && row.message_id.as_deref() == Some(&notice.message))
                            .then_some(i)
                    })
                else {
                    continue;
                };
                let target = match &notice.placement {
                    NoticePlacement::BeforeRequest {
                        span,
                        before_part_sequence,
                    } => tagged
                        .iter()
                        .enumerate()
                        .skip(prompt + 1)
                        .take(end - prompt - 1)
                        .find_map(|(i, entry)| {
                            let canonical = entry.0.position.as_ref().is_some_and(|p| {
                                p.belongs_to(turn)
                                    && (p.component == crate::history::HistoryComponent::Part(*before_part_sequence)
                                        || matches!(p.component, crate::history::HistoryComponent::Local { sequence: Some(sequence), .. } if sequence == *before_part_sequence))
                            });
                            let live = self
                                .active_turn
                                .as_ref()
                                .is_some_and(|active| active.0 == turn)
                                && entry.1 == Some(*before_part_sequence)
                                && (!entry.2
                                    || self
                                        .live_span
                                        .as_ref()
                                        .is_some_and(|(_, current)| current == span));
                            (canonical || live).then_some(i)
                        }).or_else(|| {
                            let frontier = self.active_turn.as_ref().is_some_and(|active| active.0 == turn)
                                && self.live_checkpoint_end == Some(*before_part_sequence)
                                && self.live_span.as_ref().is_some_and(|(_, current)| current == span);
                            frontier.then(|| tagged.iter().enumerate().take(end).skip(prompt + 1)
                                .filter(|(_, entry)| own(entry)).map(|(i, _)| i + 1).next_back()).flatten()
                        }),
                    NoticePlacement::Tail => {
                        let complete = self
                            .active_turn
                            .as_ref()
                            .is_some_and(|active| active.0 == turn)
                            && !self.window.has_newer()
                            || tagged[prompt + 1..end].iter().any(|entry| {
                                entry.0.position.as_ref().is_some_and(|p| {
                                    p.belongs_to(turn)
                                        && (p.component == crate::history::HistoryComponent::Footer
                                            || matches!(p.component, crate::history::HistoryComponent::Local { terminal: true, .. }))
                                })
                            });
                        complete
                            .then(|| {
                                tagged
                                    .iter()
                                    .enumerate()
                                    .take(end)
                                    .skip(prompt + 1)
                                    .filter(|(_, entry)| own(entry))
                                    .map(|(i, _)| i + 1)
                                    .next_back()
                            })
                            .flatten()
                    }
                };
                let Some(mut target) = target else { continue };
                if matches!(notice.placement, NoticePlacement::Tail) {
                    while target < end
                        && placed(&tagged[target].0)
                        && tagged[target].0.seq < tagged[from].0.seq
                    {
                        target += 1;
                    }
                }
                let entry = tagged.remove(from);
                if from < target {
                    target -= 1;
                }
                tagged.insert(target, entry);
            }
        }
        *live_row = tagged.iter().position(|(_, _, live)| *live);
        *rows = tagged.into_iter().map(|(row, _, _)| row).collect();
    }

    fn interleave_compactions(&self, rows: &mut Vec<HistoryRow>, live_row: &mut Option<usize>) {
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
            if let Some(live) = live_row.as_mut()
                && position <= *live
            {
                *live += 1;
            }
            rows.insert(
                position,
                snapshot.row(self.compaction_frame, self.chrome.animations != Some(false)),
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
        if self.has_pending_transcript_projection()
            || (tokio::runtime::Handle::try_current().is_ok() && self.large_projection())
        {
            let previous = self.viewport.get();
            return self
                .visible_transcript_at_viewport(
                    if width == 0 {
                        previous.map_or(80, |view| view.width)
                    } else {
                        width
                    },
                    if terminal_width == u16::MAX {
                        previous.map_or(80, |view| view.terminal_width)
                    } else {
                        terminal_width
                    },
                    previous.map_or(VIEWPORT_LINES as u16, |view| view.height),
                )
                .0;
        }
        let theme = Theme::dark();
        crate::messages::transcript_with_expansion(
            &self.transcript_rows(),
            theme,
            width,
            terminal_width,
            |agent| self.agent_color(agent),
            Some(&self.markdown_cache),
            &|target| self.tool_expanded(target),
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
        (lines.to_vec(), total, scroll)
    }

    pub(crate) fn visible_transcript_at_viewport_with_targets(
        &self,
        width: u16,
        terminal_width: u16,
        height: u16,
    ) -> (
        std::sync::Arc<[Line]>,
        usize,
        usize,
        Vec<Option<crate::messages::UserMessageTarget>>,
    ) {
        self.poll_transcript_projection();
        let dimensions = (width, terminal_width, height);
        self.request_projection(dimensions);
        let ready = self.projection_ready(dimensions);
        if let Some(view) = self.visible_projection.borrow().as_ref() {
            let lines = if view.lines.len() <= height as usize {
                view.lines.clone()
            } else {
                view.lines[..height as usize].into()
            };
            let targets = if ready {
                view.targets.clone()
            } else {
                vec![None; lines.len()]
            };
            return (lines, view.total, view.scroll, targets);
        }
        // Unknown geometry is deliberately not observed or authorized. Zero is
        // an absence sentinel, never a replacement exact height/scroll anchor.
        (
            std::sync::Arc::from([Line::styled(
                "   … Loading transcript…",
                ratatui::style::Style::default().fg(Theme::dark().text_muted()),
            )]),
            0,
            0,
            Vec::new(),
        )
    }

    pub(super) fn invalidate_transcript(&mut self) {
        self.transcript_revision = self.transcript_revision.wrapping_add(1);
        self.transcript_structure = self.transcript_structure.wrapping_add(1);
    }

    /// An append to the open live text or reasoning: repaint, but rows and
    /// tool cards are unchanged.
    pub(super) fn invalidate_live_text(&mut self) {
        self.transcript_revision = self.transcript_revision.wrapping_add(1);
    }

    /// Wheel/scrollbox navigation never changes the focused editor, even
    /// when keyboard Up/Down would move its caret or recall prompt history.
    pub fn scroll_transcript(&mut self, up: bool) -> KeyOutcome {
        self.wheel_motion = None;
        self.poll_submission_receipts();
        if self.panel != TuiPanel::None {
            return KeyOutcome::default();
        }
        self.release_transcript_scroll_press();
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
        self.release_transcript_scroll_press();
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
        self.release_transcript_scroll_press();
        self.exploration_down = None;
        self.reasoning_down = None;
        true
    }

    fn line_count(&self) -> usize {
        if self.has_pending_transcript_projection()
            || (tokio::runtime::Handle::try_current().is_ok() && self.large_projection())
        {
            return self.viewport.get().map_or(0, |view| view.total);
        }
        self.transcript_lines(0, u16::MAX).len()
    }

    pub(super) fn max_scroll(&self) -> usize {
        self.viewport.get().map_or_else(
            || self.line_count().saturating_sub(VIEWPORT_LINES),
            |view| view.total.saturating_sub(view.height as usize),
        )
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
        if (width != 0 && !self.projection_ready((width, terminal_width, height)))
            || self.has_pending_transcript_projection() && width == 0
        {
            return;
        }
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
        if let Some((input, output, limit)) = self.turn_context_usage {
            return Some((input.saturating_add(output), (limit > 0).then_some(limit)));
        }
        let (usage, captured_limit) = self
            .turn_usage
            .as_ref()
            .map(|u| ((u.input_tokens, u.output_tokens), None))
            .or_else(|| {
                // Live usage is handled above. Retained in-flight checkpoints
                // carry accounting facts, not a settled step's UI measurement.
                self.window.rows().iter().rev().find_map(|r| {
                    let m = r.meta.as_ref()?;
                    if m.status.as_deref() == Some("started") {
                        return None;
                    }
                    m.context_usage
                        .map(|usage| (usage, m.context_usage_limit))
                        .or_else(|| Some(((m.input_tokens?, m.output_tokens?), None)))
                })
            })?;
        if let Some(limit) = captured_limit {
            return Some((
                usage.0.saturating_add(usage.1),
                (limit > 0).then_some(limit),
            ));
        }
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

/// The same painted rows: one shared projection (pointer) or equal content.
fn same_rows(a: &[Line], b: &[Line]) -> bool {
    (a.len() == b.len() && std::ptr::eq(a.as_ptr(), b.as_ptr())) || a == b
}

/// Overlay inputs that turn a shared window card into its rendered form.
#[derive(PartialEq)]
pub(super) struct OverlayKey {
    diffs: oc_core::queries::DiffSettings,
    dcp: Option<(
        oc_core::dcp_view::DcpDisplayConfig,
        Option<usize>,
        Option<String>,
    )>,
}

/// One remembered overlay: the window's source card, its overlay inputs and
/// the derived card handed to projections (identity keys the layout cache).
pub(super) struct CardOverlay {
    source: std::sync::Arc<crate::history::ToolCard>,
    key: OverlayKey,
    derived: std::sync::Arc<crate::history::ToolCard>,
}
