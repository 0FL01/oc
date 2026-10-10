//! Bounded history window and tool cards (UI03).
//!
//! Pages arrive as application DTOs ([`HistoryPage`]); the view never holds
//! a storage handle and never renders the whole transcript.
//! [`HistoryWindow`] enforces both row and byte caps on every insertion, so
//! retained bytes stay bounded no matter how many pages are pushed. Tool
//! cards pair recorded intents with outcomes; `apply_patch` cards
//! additionally list affected paths parsed from the recorded `patchText`
//! (parse failures show no files, never invented ones).

use oc_core::queries::{HistoryMessage, HistoryPage, ToolOpView};
use oc_core::session::Role;

use crate::messages::{AssistantMeta, Chip, ReasoningBlock, ReasoningIdentity};
use crate::tools::ToolRender;

/// Max rows retained by the window.
pub const WINDOW_ROWS: usize = 240;
/// Max retained payload bytes (including durable identity and presentation metadata).
pub const WINDOW_BYTES: usize = 256 * 1024;
/// Max preview chars per card field.
pub const CARD_PREVIEW: usize = 512;
/// Max files listed on a patch card.
pub const CARD_FILES: usize = 5;

/// One history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    /// Storage-owned identity shared by derived rows; absent for live/synthetic rows.
    pub message_id: Option<std::sync::Arc<oc_core::session::MessageId>>,
    /// Message sequence (ordering key for paging); `i64::MAX` for synthetic
    /// rows that are not committed yet.
    pub seq: i64,
    /// Render prefix (`user` / `assistant` for committed rows).
    pub role: String,
    /// Message text. A hidden `shell_input_delivered` row retains only its
    /// exact native operation identity here, not RAW text or a render payload.
    pub text: String,
    /// Agent that owns the row: the session agent for user rows, the turn
    /// agent for live assistant rows. `None` when unknown; the renderer then
    /// falls back to the session agent / upstream's default agent color.
    pub agent: Option<String>,
    /// Categorical color slot pinned when the owning turn was accepted.
    /// Legacy rows without this projection resolve through `agent` instead.
    pub agent_color_index: Option<usize>,
    /// Skill/file chips the user message carried. Storage keeps no
    /// per-message attachments, so committed rows stay empty.
    pub chips: Vec<Chip>,
    /// Reasoning block attached to an assistant row.
    pub reasoning: Option<ReasoningBlock>,
    /// Assistant footer data, live or projected from the durable turn.
    pub meta: Option<AssistantMeta>,
    /// Tool card attached to a `tool` row (live turns and cards panel rows
    /// stay plain text; the transcript renders the card).
    pub tool: Option<ToolCard>,
    /// Exact delivered child notice; not a user block or assistant tool part.
    pub child_notice: Option<Box<oc_core::queries::ChildJob>>,
    /// Exact background Shell data notice; intentionally not a navigation link.
    pub shell_notice: Option<oc_core::queries::ShellHistoryNotice>,
}

/// Which end of the deque is dropped when a cap is exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Evict {
    /// Drop the oldest retained row.
    Oldest,
    /// Drop the newest retained row.
    Newest,
}

/// Bounded, oldest-first window over one session history.
#[derive(Debug, Clone, Default)]
pub struct HistoryWindow {
    rows: Vec<HistoryRow>,
    revision: u64,
    total: usize,
    has_older: bool,
    has_newer: bool,
}

impl HistoryWindow {
    /// Empty window.
    pub fn new() -> Self {
        Self::default()
    }

    /// Newest page becomes the whole window.
    pub fn reset(&mut self, page: &HistoryPage) {
        self.revision = self.revision.wrapping_add(1);
        self.rows = page.rows.iter().flat_map(rows_from_page).collect();
        self.reconcile_shell_inputs();
        self.total = page.total;
        self.has_older = page.has_older;
        self.has_newer = page.has_newer;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// Replace the overlapping durable tail after completion, retaining loaded
    /// older pages. A disjoint newest page must not fabricate a contiguous gap:
    /// keep the reader's window and let normal newer paging reach the tail.
    pub(crate) fn refresh_completed(&mut self, page: &HistoryPage, detached: bool) {
        self.revision = self.revision.wrapping_add(1);
        let first = page.rows.first().map(|row| row.seq);
        let overlaps = page.rows.iter().any(|message| {
            self.rows
                .iter()
                .any(|row| row.message_id.as_deref() == Some(&message.id))
        });
        if detached && !overlaps && self.rows.iter().any(|row| row.message_id.is_some()) {
            self.rows.retain(|row| row.message_id.is_some());
            self.total = page.total;
            self.has_newer = true;
            return;
        }
        let older = detached.then(|| {
            self.rows
                .iter()
                .filter(|row| row.message_id.is_some() && first.is_some_and(|seq| row.seq < seq))
                .cloned()
                .collect::<Vec<_>>()
        });
        let has_older = self.has_older;
        self.reset(page);
        if let Some(mut older) = older
            && !older.is_empty()
        {
            older.append(&mut self.rows);
            self.rows = older;
            self.reconcile_shell_inputs();
            self.has_older = has_older;
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
    }

    /// Add an older page at the front; returns rows added. Evicts newest
    /// rows while over a cap and flags `has_newer` when it does.
    pub fn prepend_older(&mut self, page: &HistoryPage) -> usize {
        self.revision = self.revision.wrapping_add(1);
        let mut combined: Vec<HistoryRow> = page.rows.iter().flat_map(rows_from_page).collect();
        let added = combined.len();
        combined.append(&mut self.rows);
        self.rows = combined;
        self.reconcile_shell_inputs();
        self.total = page.total;
        self.has_older = page.has_older;
        if self.enforce(Evict::Newest) {
            self.has_newer = true;
        }
        added
    }

    /// Add a newer page at the back; returns rows added. Evicts oldest rows
    /// while over a cap and flags `has_older` when it does.
    pub fn append_newer(&mut self, page: &HistoryPage) -> usize {
        self.revision = self.revision.wrapping_add(1);
        let before = self.rows.len();
        self.rows.extend(page.rows.iter().flat_map(rows_from_page));
        self.reconcile_shell_inputs();
        let added = self.rows.len() - before;
        self.total = page.total;
        self.has_newer = page.has_newer;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
        added
    }

    /// Rows in render order (oldest first).
    pub fn rows(&self) -> &[HistoryRow] {
        &self.rows
    }

    /// Content/identity generation, without rescanning or copying row payloads.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn apply_dcp_summary(
        &mut self,
        op: &str,
        page: oc_core::dcp_view::DcpSummaryPage,
    ) -> bool {
        let changed = self.rows.iter_mut().any(|row| {
            let Some(card) = row.tool.as_mut().filter(|card| card.op == op) else {
                return false;
            };
            let ToolRender::Dcp(view) = &mut card.render else {
                return false;
            };
            view.apply_summary(page.clone())
        });
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        if changed && self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
        changed
    }

    /// Retained row count.
    pub(crate) fn refresh_subagent_cards(
        &mut self,
        session: &oc_core::domain::SessionId,
        jobs: &[oc_core::queries::ChildJob],
        picker: Option<&crate::picker::ModelPicker>,
    ) -> bool {
        let mut changed = false;
        for row in &mut self.rows {
            if row.role == "tool"
                && let Some(card) = &mut row.tool
            {
                changed |= card.refresh_subagent_job(session, jobs, picker);
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        if changed && self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
        changed
    }

    /// Retained row count.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// True when no row is retained.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Retained payload bytes; shared IDs are conservatively counted per row.
    pub fn retained_bytes(&self) -> usize {
        self.rows
            .iter()
            .map(|row| {
                row.role.len()
                    + row.message_id.as_ref().map_or(0, |id| id.0.len())
                    + row.text.len()
                    + row.agent.as_ref().map_or(0, String::len)
                    + row.reasoning.as_ref().map_or(0, |r| r.text.len())
                    + row
                        .meta
                        .as_ref()
                        .and_then(|m| m.model.as_ref())
                        .map_or(0, String::len)
                    + row.tool.as_ref().map_or(0, ToolCard::retained_bytes)
                    + row.child_notice.as_deref().map_or(0, child_notice_bytes)
                    + row.shell_notice.as_ref().map_or(0, |notice| {
                        notice.operation.len() + notice.state.len() + notice.command.len()
                    })
            })
            .sum()
    }

    /// Total committed messages in the session (as reported by pages).
    pub fn total(&self) -> usize {
        self.total
    }

    /// Older committed rows exist before the window.
    pub fn has_older(&self) -> bool {
        self.has_older
    }

    /// Newer committed rows exist after the window.
    pub fn has_newer(&self) -> bool {
        self.has_newer
    }

    /// Append one locally produced row (prompt echo, live answer, notice):
    /// never a committed history row. The window becomes the newest tail
    /// again; oldest rows are evicted while a cap is exceeded.
    pub(crate) fn push_synthetic(
        &mut self,
        role: &str,
        text: &str,
        agent: Option<String>,
        agent_color_index: Option<usize>,
    ) {
        self.revision = self.revision.wrapping_add(1);
        self.rows.push(HistoryRow {
            message_id: None,
            seq: i64::MAX,
            role: role.to_string(),
            text: text.to_string(),
            agent,
            agent_color_index,
            chips: Vec::new(),
            reasoning: None,
            meta: None,
            tool: None,
            child_notice: None,
            shell_notice: None,
        });
        self.has_newer = false;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// Append one fully rendered row (live assistant message with reasoning
    /// and footer metadata); replayed rows use the same safe presentation data.
    pub(crate) fn push_row(&mut self, row: HistoryRow) {
        self.revision = self.revision.wrapping_add(1);
        self.rows.push(row);
        self.has_newer = false;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// Receipt-owned echo: real operation ID, no invented durable message/turn.
    pub(crate) fn push_user_shell(&mut self, operation: String, command: &str) {
        if self.rows.iter().any(|row| {
            row.role == "shell_input_delivered" && row.text == operation
                || matches!(row.role.as_str(), "shell" | "shell_input")
                    && row.tool.as_ref().is_some_and(|card| card.op == operation)
        }) {
            return;
        }
        let end = command.floor_char_boundary(oc_core::tool_output::PREVIEW_BYTES);
        let card = user_shell_card(&oc_core::queries::UserShellResult {
            input: true,
            superseded_input: false,
            operation,
            command: command[..end].into(),
            command_limited: end < command.len(),
            state: "started".into(),
            output: Box::new(oc_core::tool_output::Presentation::new("", 0, false)),
            diagnostic: None,
        });
        self.push_row(HistoryRow {
            message_id: None,
            seq: i64::MAX,
            role: "shell_input".into(),
            text: String::new(),
            agent: None,
            agent_color_index: None,
            chips: Vec::new(),
            reasoning: None,
            meta: None,
            tool: Some(card),
            child_notice: None,
            shell_notice: None,
        });
    }

    /// Selective native Shell update while the live assistant owns its parts.
    /// Do not attach a second projection of an executing model turn.
    pub(crate) fn refresh_user_shell(&mut self, page: &HistoryPage) -> bool {
        self.refresh_owner_notices(page)
    }

    /// Correlate the current acknowledged prompt, without projecting any of its
    /// executing assistant parts. Only its durable paging identity changes.
    pub(crate) fn correlate_live_prompt(&mut self, page: &HistoryPage, turn: &str) {
        let Some(message) = page.rows.iter().find(|message| {
            message.role == Role::User
                && message.user_shell.is_none()
                && message.child.is_none()
                && message.shell_notice.is_none()
                && message.turn.as_ref().is_some_and(|owner| owner.id == turn)
        }) else {
            return;
        };
        if self
            .rows
            .iter()
            .any(|row| row.message_id.as_deref() == Some(&message.id))
        {
            return;
        }
        if let Some(row) = self
            .rows
            .iter_mut()
            .rev()
            .find(|row| row.role == "user" && row.message_id.is_none())
        {
            row.message_id = Some(std::sync::Arc::new(message.id.clone()));
            row.seq = message.seq;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Selective typed data rows, never an executing model turn projection.
    pub(crate) fn refresh_owner_notices(&mut self, page: &HistoryPage) -> bool {
        let mut changed = false;
        let tail = self.rows.iter().rev().find(|row| row.message_id.is_some());
        let tail_seq = tail.map(|row| row.seq);
        let tail_covered = tail.is_none_or(|row| {
            page.rows
                .iter()
                .any(|message| row.message_id.as_deref() == Some(&message.id))
        });
        for message in &page.rows {
            let shell = message.user_shell.as_ref();
            if shell.is_none()
                && message.shell_notice.is_none()
                && !matches!(
                    message.child,
                    Some(oc_core::queries::ChildHistory::Notice(_))
                )
            {
                continue;
            }
            // The result may be outside a detached window. Its exact operation
            // still retires a retained input, without inventing a durable row or
            // losing the input's paging identity.
            if let Some(shell) = shell.filter(|shell| !shell.input) {
                for row in &mut self.rows {
                    if row.role == "shell_input"
                        && row
                            .tool
                            .as_ref()
                            .is_some_and(|card| card.op == shell.operation)
                    {
                        row.role = "shell_input_delivered".into();
                        row.tool = None;
                        row.text = shell.operation.clone();
                        changed = true;
                    }
                }
            }
            let replacement = rows_from_page(message).remove(0);
            let index = self.rows.iter().position(|row| {
                row.message_id.as_deref() == Some(&message.id)
                    || shell.is_some_and(|shell| {
                        shell.input
                            && row.message_id.is_none()
                            && row.role == "shell_input"
                            && row
                                .tool
                                .as_ref()
                                .is_some_and(|card| card.op == shell.operation)
                    })
            });
            if let Some(index) = index {
                if self.rows[index] != replacement {
                    let synthetic = self.rows[index].message_id.is_none();
                    if synthetic {
                        self.rows.remove(index);
                        self.insert_durable(replacement);
                    } else {
                        self.rows[index] = replacement;
                    }
                    changed = true;
                }
            } else if tail_seq.is_none_or(|seq| message.seq > seq) {
                // A bulk page can contain an already-evicted older Shell. Never
                // append it behind the newer retained rows. Nor may a selective
                // update jump over unseen ordinary/model records.
                let gap = !tail_covered
                    || page.rows.iter().any(|between| {
                        tail_seq.is_some_and(|seq| between.seq > seq)
                            && between.seq < message.seq
                            && between.user_shell.is_none()
                            && between.shell_notice.is_none()
                            && !matches!(
                                between.child,
                                Some(oc_core::queries::ChildHistory::Notice(_))
                            )
                            && !self
                                .rows
                                .iter()
                                .any(|row| row.message_id.as_deref() == Some(&between.id))
                    });
                if !self.has_newer && !gap {
                    self.insert_durable(replacement);
                    changed = true;
                } else if !self.has_newer {
                    self.has_newer = true;
                    changed = true;
                }
            }
        }
        if self.total != page.total {
            self.total = page.total;
            changed = true;
        }
        if changed {
            self.reconcile_shell_inputs();
            self.revision = self.revision.wrapping_add(1);
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
        changed
    }

    /// An inventory observation cannot add a message, revive a terminal result,
    /// rewrite command identity, or touch a model-owned tool/assistant part.
    pub(crate) fn refresh_running_user_shell_output(
        &mut self,
        session: &oc_core::domain::SessionId,
        jobs: &[oc_core::queries::ShellJob],
    ) -> bool {
        let mut changed = false;
        for row in &mut self.rows {
            if row.role != "shell_input" {
                continue;
            }
            let Some(card) = row.tool.as_mut().filter(|card| card.state == "started") else {
                continue;
            };
            let ToolRender::Shell(render) = &card.render else {
                continue;
            };
            if !render.direct_user {
                continue;
            }
            let Some(output) = jobs
                .iter()
                .find(|job| {
                    &job.session == session && job.shell_id == card.op && job.turn.is_empty()
                })
                .and_then(|job| job.output.as_ref())
                .filter(|output| {
                    output.is_valid()
                        && output.shell.as_ref().is_some_and(|facts| {
                            facts.exit.is_none()
                                && facts.signal.is_none()
                                && !facts.timed_out
                                && !facts.cancelled
                        })
                })
            else {
                continue;
            };
            if card.output_presentation.as_ref() == Some(output) {
                continue;
            }
            *card = user_shell_card(&oc_core::queries::UserShellResult {
                input: true,
                superseded_input: false,
                operation: card.op.clone(),
                command: render.command.clone(),
                command_limited: render.command_limited,
                state: "started".into(),
                output: output.clone(),
                diagnostic: None,
            });
            changed = true;
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
        changed
    }

    /// Existing model tool parts may observe their exact owned process without
    /// adding graph rows or borrowing another session's job.
    pub(crate) fn refresh_running_model_shell_output(
        &mut self,
        session: &oc_core::domain::SessionId,
        jobs: &[oc_core::queries::ShellJob],
    ) -> bool {
        let mut changed = false;
        for row in &mut self.rows {
            if row.role == "tool"
                && let Some(card) = &mut row.tool
            {
                changed |= card.refresh_model_shell_output(session, None, jobs);
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
        changed
    }

    pub(crate) fn finish_model_shell_preview(
        &mut self,
        snapshot: &oc_core::queries::ShellSnapshot,
    ) -> bool {
        let mut changed = false;
        for row in &mut self.rows {
            if row.role == "tool"
                && let Some(card) = &mut row.tool
            {
                changed |= card.finish_model_shell_preview(snapshot);
            }
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
        changed
    }

    fn insert_durable(&mut self, row: HistoryRow) {
        let index = self
            .rows
            .iter()
            .position(|existing| existing.message_id.is_none() || existing.seq > row.seq)
            .unwrap_or(self.rows.len());
        self.rows.insert(index, row);
    }

    fn reconcile_shell_inputs(&mut self) {
        let results = self
            .rows
            .iter()
            .filter(|row| row.role == "shell")
            .filter_map(|row| row.tool.as_ref().map(|card| card.op.clone()))
            .collect::<Vec<_>>();
        for row in &mut self.rows {
            if row.role == "shell_input"
                && row
                    .tool
                    .as_ref()
                    .is_some_and(|card| results.contains(&card.op))
            {
                row.text = row.tool.as_ref().expect("matched Shell input").op.clone();
                row.role = "shell_input_delivered".into();
                row.tool = None;
            }
        }
    }

    /// A live acceptance notice arrives after the prompt echo receipt.
    pub(crate) fn insert_before_live_user(&mut self, text: String) {
        if let Some(index) = self
            .rows
            .iter()
            .rposition(|row| row.seq == i64::MAX && row.role == "user")
        {
            self.revision = self.revision.wrapping_add(1);
            self.rows.insert(
                index,
                HistoryRow {
                    message_id: None,
                    seq: i64::MAX,
                    role: "model_switch".into(),
                    text,
                    agent: None,
                    agent_color_index: None,
                    chips: Vec::new(),
                    reasoning: None,
                    meta: None,
                    tool: None,
                    child_notice: None,
                    shell_notice: None,
                },
            );
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
    }

    /// Drop rows from `side` until both caps hold; returns true when any row
    /// was evicted.
    fn enforce(&mut self, side: Evict) -> bool {
        let mut evicted = false;
        while self.rows.len() > WINDOW_ROWS || self.retained_bytes() > WINDOW_BYTES {
            match side {
                Evict::Oldest => {
                    self.rows.remove(0);
                }
                Evict::Newest => {
                    self.rows.pop();
                }
            }
            evicted = true;
        }
        evicted
    }
}

fn row_from_page(
    row: &HistoryMessage,
    message_id: &std::sync::Arc<oc_core::session::MessageId>,
) -> HistoryRow {
    HistoryRow {
        message_id: Some(message_id.clone()),
        seq: row.seq,
        role: if row.model_switch.is_some() {
            "model_switch".into()
        } else {
            match row.role {
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
            }
        },
        text: row
            .child
            .as_ref()
            .and_then(|child| match child {
                oc_core::queries::ChildHistory::Task { text, limited } => Some(format!(
                    "{text}{}",
                    if *limited { "\n[Task preview]" } else { "" }
                )),
                _ => None,
            })
            .unwrap_or_else(|| {
                row.model_switch
                    .as_ref()
                    .map_or_else(|| row.text.clone(), model_switch_text)
            }),
        agent: row.turn.as_ref().and_then(|turn| turn.agent.clone()),
        agent_color_index: row.turn.as_ref().and_then(|turn| turn.agent_color_index),
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        tool: None,
        child_notice: None,
        shell_notice: None,
    }
}

pub(crate) fn model_switch_text(notice: &oc_core::queries::ModelSwitchNotice) -> String {
    let current = &notice.current;
    let text = if notice.previous.provider == current.provider && notice.previous.id == current.id {
        format!(
            "Switched variant to {}",
            current.variant.as_deref().unwrap_or("default")
        )
    } else if let Some(name) = &notice.display_name {
        let variant = current
            .variant
            .as_deref()
            .filter(|value| *value != "default");
        format!(
            "Switched model to {name}{}",
            variant.map_or(String::new(), |v| format!(" ({v})"))
        )
    } else {
        let variant = current
            .variant
            .as_deref()
            .map_or(String::new(), |v| format!("/{v}"));
        format!(
            "Switched model to {}/{}{variant}",
            current.provider, current.id
        )
    };
    // Provider/model IDs and older journal rows may be unbounded. Never let
    // their text expand an unbounded TUI row or
    // inject control characters into the terminal.
    text.chars().filter(|c| !c.is_control()).take(512).collect()
}

fn rows_from_page(row: &HistoryMessage) -> Vec<HistoryRow> {
    use oc_core::queries::TranscriptPart;
    let message_id = std::sync::Arc::new(row.id.clone());
    if let Some(notice) = &row.shell_notice {
        let mut result = row_from_page(row, &message_id);
        result.role = "shell_notice".into();
        result.text.clear();
        result.shell_notice = Some(notice.clone());
        return vec![result];
    }
    if let Some(oc_core::queries::ChildHistory::Notice(job)) = &row.child {
        let mut result = row_from_page(row, &message_id);
        result.role = "child_notice".into();
        result.text.clear();
        result.child_notice = Some(job.clone());
        return vec![result];
    }
    if let Some(shell) = &row.user_shell {
        let mut result = row_from_page(row, &message_id);
        result.text.clear();
        if shell.superseded_input {
            result.role = "shell_input_delivered".into();
            result.text = shell.operation.clone();
            return vec![result];
        }
        result.role = if shell.input { "shell_input" } else { "shell" }.into();
        result.tool = Some(user_shell_card(shell));
        return vec![result];
    }
    if row.model_switch.is_some() {
        return vec![row_from_page(row, &message_id)];
    }
    let Some(turn) = &row.turn else {
        return vec![row_from_page(row, &message_id)];
    };
    let mut rows = Vec::new();
    if turn.legacy_text_only {
        rows.push(row_from_page(row, &message_id));
        let mut notice = row_from_page(row, &message_id);
        notice.text = "[Legacy text-only history: reasoning and part order were not recorded; tool records remain available in /cards]".into();
        notice.role = "assistant".into();
        rows.push(notice);
        return rows;
    }
    if row.role == Role::User {
        rows.push(row_from_page(row, &message_id));
    }
    // Avoid cloning the aggregate message once per projected part.
    let empty_row = || HistoryRow {
        message_id: Some(message_id.clone()),
        seq: row.seq,
        role: "assistant".to_string(),
        text: String::new(),
        agent: turn.agent.clone(),
        agent_color_index: turn.agent_color_index,
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        tool: None,
        child_notice: None,
        shell_notice: None,
    };
    let first = turn
        .spans
        .iter()
        .find_map(|span| span.request.as_ref().map(|request| &request.model));
    let mixed = turn
        .spans
        .iter()
        .filter_map(|span| span.request.as_ref())
        .any(|request| Some(&request.model) != first);
    // Interrupted turns may attach to the accepted user row, which precedes
    // the projected parts but has no entry in part_states.
    let part_start = rows.len();
    for (index, part) in turn.parts.iter().enumerate() {
        let mut part_row = empty_row();
        match part {
            TranscriptPart::Text(text) => part_row.text = text.clone(),
            TranscriptPart::Reasoning { text, duration_ms } => {
                part_row.reasoning = Some(ReasoningBlock {
                    text: text.clone(),
                    duration_ms: *duration_ms,
                    running: turn.status == "started" && duration_ms.is_none(),
                    expanded: false,
                    toggleable: true,
                    identity: Some(ReasoningIdentity::Durable(row.seq, index)),
                })
            }
            TranscriptPart::Tool(op) => {
                part_row.role = "tool".to_string();
                part_row.tool = Some(card_from_row(op));
            }
        }
        if let Some(state) = turn.part_states.get(index)
            && matches!(
                part,
                TranscriptPart::Text(_) | TranscriptPart::Reasoning { .. }
            )
            && matches!(
                state.status.as_str(),
                "failed" | "unknown" | "interrupted" | "incomplete" | "cancelled"
            )
        {
            part_row.meta = Some(AssistantMeta {
                status: Some(state.status.clone()),
                ..AssistantMeta::default()
            });
        }
        rows.push(part_row);
        if mixed
            && !matches!(part, TranscriptPart::Tool(_))
            && let Some(model) = turn
                .part_states
                .get(index)
                .and_then(|state| state.model_label.clone())
        {
            rows.last_mut()
                .expect("part row")
                .meta
                .get_or_insert_with(Default::default)
                .model = Some(model);
        }
    }
    for span in &turn.spans {
        if let Some(retry) = &span.retry {
            let mut notice = empty_row();
            notice.text = format!(
                "[Historical retry · {} · attempt {} · at {} · {}]",
                span.status,
                retry.attempt,
                retry.at,
                retry
                    .safe_error
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect::<String>()
            );
            rows.push(notice);
        }
    }
    let mut footer = empty_row();
    footer.meta = Some(AssistantMeta {
        model: Some(
            turn.spans
                .last()
                .and_then(|span| span.request.as_ref())
                .map_or_else(
                    || turn.model_label.clone(),
                    |request| request.model_label.clone(),
                ),
        ),
        duration_ms: turn.duration_ms,
        input_tokens: turn.usage.map(|v| v.0),
        output_tokens: turn.usage.map(|v| v.1),
        context_usage: turn.context_usage,
        streamed_ms: turn.streamed_ms,
        session_tps: None,
        interrupted: turn.status == "cancelled",
        status: Some(turn.status.clone()),
        agent_color_index: turn.agent_color_index,
        preview_limited: turn.omitted_parts > 0
            || turn.part_states.iter().enumerate().any(|(index, state)| {
                state.input_omitted
                    || (state.truncated
                        && rows
                            .get(part_start + index)
                            .and_then(|row| row.tool.as_ref())
                            .filter(|card| card.output_presentation.is_some())
                            .is_none_or(ToolCard::preview_limited))
            }),
    });
    rows.push(footer);
    rows
}

fn child_notice_bytes(job: &oc_core::queries::ChildJob) -> usize {
    [
        &job.parent.0,
        &job.child.0,
        &job.operation,
        &job.location,
        &job.agent,
        &job.model,
        &job.description,
        &job.delivery_id,
    ]
    .iter()
    .map(|field| field.len())
    .sum::<usize>()
        + job.turn.as_ref().map_or(0, String::len)
        + job.result.as_ref().map_or(0, String::len)
        + job.message_id.as_ref().map_or(0, String::len)
}

fn user_shell_card(shell: &oc_core::queries::UserShellResult) -> ToolCard {
    let facts = shell.output.shell.as_ref();
    let render = crate::tools::ShellRender {
        command: shell.command.clone(),
        direct_user: true,
        live_running: false,
        process_state: None,
        background: false,
        command_limited: shell.command_limited,
        diagnostic: shell.diagnostic.clone(),
        exit: facts.and_then(|facts| facts.exit.map(i64::from)),
        signal: facts.is_some_and(|facts| facts.signal.is_some()),
        stdout: facts.map_or_else(Vec::new, |facts| {
            facts.stdout.lines().map(str::to_owned).collect()
        }),
        stderr: facts.map_or_else(Vec::new, |facts| {
            facts.stderr.lines().map(str::to_owned).collect()
        }),
        timed_out: facts.is_some_and(|facts| facts.timed_out),
        cancelled: facts.is_some_and(|facts| facts.cancelled),
        output_ends_with_newline: false,
        cwd: None,
    };
    // A UI block over the real NULL-turn operation, not a synthetic
    // TranscriptPart::Tool or provider ToolCallResult.
    ToolCard {
        op: shell.operation.clone(),
        started_at: None,
        child_job: None,
        name: "shell".into(),
        state: shell.state.clone(),
        input_preview: preview(Some(&shell.command)),
        output_preview: shell.output.body.clone(),
        output_bytes: shell.output.body_bytes.min(i64::MAX as u64) as i64,
        output_truncated: shell.output.body_limited,
        output_presentation: Some(shell.output.clone()),
        files: Vec::new(),
        files_truncated: false,
        diff: None,
        patch_effects: None,
        question: None,
        diff_settings: Default::default(),
        render: ToolRender::Shell(render),
    }
}

/// One tool card: intent + outcome + bounded previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCard {
    /// Observed admission instant of a live call; absent for cold/missed starts.
    pub started_at: Option<std::time::Instant>,
    /// Original positively owned child-generation fence, never output prose.
    pub child_job: Option<Box<oc_core::queries::ChildJob>>,
    /// Operation id.
    pub op: String,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `unknown`.
    pub state: String,
    /// Bounded input preview.
    pub input_preview: String,
    /// Available bounded body; legacy rows retain their recorded output preview.
    pub output_preview: String,
    /// Full stored output size in bytes.
    pub output_bytes: i64,
    /// True when the durable result is longer than the preview.
    pub output_truncated: bool,
    /// Known bounded body/capture facts, separate from unchanged stored output.
    pub output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
    /// Confirmed effect paths; permission-only rows carry prepared/requested
    /// targets when no effects exist. These names alone never confirm mutation.
    pub files: Vec<String>,
    /// True when more files exist than listed.
    pub files_truncated: bool,
    /// Legacy request-only diff summary, never confirmation of applied effects.
    pub diff: Option<oc_adapters::patch::DiffSummary>,
    /// Owner-confirmed effects, independent of request previews and workspace state.
    pub patch_effects: Option<oc_core::patch::PatchEffects>,
    pub question: Option<oc_core::question::QuestionResult>,
    pub diff_settings: oc_core::queries::DiffSettings,
    /// Presentation data parsed once from the recorded input/output
    /// (bounded); the transcript renders the card from it.
    pub render: ToolRender,
}

impl ToolCard {
    pub(crate) fn refresh_subagent_job(
        &mut self,
        session: &oc_core::domain::SessionId,
        jobs: &[oc_core::queries::ChildJob],
        picker: Option<&crate::picker::ModelPicker>,
    ) -> bool {
        use oc_core::queries::ChildState;
        let ToolRender::Subagent(render) = &mut self.render else {
            return false;
        };
        let before = render.clone();
        let job = jobs.iter().find(|job| {
            job.parent == *session
                && job.operation == self.op
                && job.retained_bytes() <= oc_core::tool_output::RECORD_BYTES
        });
        let job = job.filter(|job| {
            self.child_job.as_ref().is_none_or(|captured| {
                captured.parent == job.parent
                    && captured.child == job.child
                    && captured.operation == job.operation
                    && captured.generation == job.generation
                    && captured.location == job.location
                    && captured.delivery_id == job.delivery_id
            })
        });
        let mut changed = false;
        if let Some(job) = job {
            let terminal = self.child_job.as_ref().is_some_and(|job| {
                !matches!(job.state, ChildState::Admitted | ChildState::Running)
            });
            if !terminal || !matches!(job.state, ChildState::Admitted | ChildState::Running) {
                let mut job = job.clone();
                job.result = None;
                let job = Box::new(job);
                changed = self.child_job.as_ref() != Some(&job);
                self.child_job = Some(job);
            }
        }
        render.live_running = job.is_some_and(|_| {
            self.child_job
                .as_ref()
                .is_some_and(|job| matches!(job.state, ChildState::Admitted | ChildState::Running))
        }) || self.child_job.as_ref().is_some_and(|captured| {
            // Current child status is distinct from immutable original launch
            // provenance. A genuine continuation may animate a historical row,
            // but never replaces its original operation/generation or link.
            jobs.iter().any(|current| {
                current.parent == *session
                    && current.parent == captured.parent
                    && current.child == captured.child
                    && current.operation != captured.operation
                    && current.retained_bytes() <= oc_core::tool_output::RECORD_BYTES
                    && matches!(current.state, ChildState::Admitted | ChildState::Running)
            })
        });
        render.model_label = render
            .model
            .as_deref()
            .and_then(|value| crate::tools::subagent_model_label(value, picker));
        changed || before != *render
    }

    pub(crate) fn is_model_shell(&self, operation: &str) -> bool {
        self.op == operation
            && matches!(&self.render, ToolRender::Shell(shell) if !shell.direct_user)
    }

    pub(crate) fn finish_model_shell_preview(
        &mut self,
        snapshot: &oc_core::queries::ShellSnapshot,
    ) -> bool {
        if !self.is_model_shell(&snapshot.job.shell_id)
            || snapshot.job.turn.is_empty()
            || !matches!(
                snapshot.state.as_str(),
                "completed" | "failed" | "cancelled" | "timed_out" | "unknown"
            )
            || snapshot.display.len() > 65536
        {
            return false;
        }
        let ToolRender::Shell(shell) = &mut self.render else {
            return false;
        };
        shell.live_running = false;
        shell.process_state = Some(snapshot.state.clone());
        shell.background = snapshot.job.background;
        shell.exit = snapshot.exit.map(i64::from);
        shell.signal = snapshot.signal.is_some();
        shell.cancelled = snapshot.state == "cancelled";
        shell.timed_out = snapshot.state == "timed_out";
        let start = snapshot.display.ceil_char_boundary(
            snapshot
                .display
                .len()
                .saturating_sub(oc_core::tool_output::PREVIEW_BYTES),
        );
        let mut output = oc_core::tool_output::Presentation::new(
            &snapshot.display[start..],
            snapshot.display.len() as u64,
            false,
        );
        output.body_limited |= snapshot.display_omitted;
        self.output_preview = output.body.clone();
        self.output_bytes = output.body_bytes.min(i64::MAX as u64) as i64;
        self.output_truncated = output.body_limited;
        self.output_presentation = Some(Box::new(output));
        true
    }

    pub(crate) fn refresh_model_shell_output(
        &mut self,
        session: &oc_core::domain::SessionId,
        turn: Option<&str>,
        jobs: &[oc_core::queries::ShellJob],
    ) -> bool {
        let ToolRender::Shell(shell) = &mut self.render else {
            return false;
        };
        if shell.direct_user {
            return false;
        }
        if shell.process_state.is_some()
            || shell.exit.is_some()
            || shell.signal
            || shell.timed_out
            || shell.cancelled
        {
            return std::mem::replace(&mut shell.live_running, false);
        }
        let job = jobs.iter().find(|job| {
            &job.session == session
                && job.shell_id == self.op
                && !job.turn.is_empty()
                && turn.is_none_or(|turn| turn == job.turn)
                && (matches!(self.state.as_str(), "started" | "running")
                    || self.state == "completed" && job.background)
        });
        let Some((job, output)) = job.and_then(|job| {
            job.output
                .as_ref()
                .filter(|output| {
                    output.is_valid()
                        && output.shell.as_ref().is_some_and(|facts| {
                            facts.background == job.background
                                && facts.process_state.is_none()
                                && facts.exit.is_none()
                                && facts.signal.is_none()
                                && !facts.timed_out
                                && !facts.cancelled
                        })
                })
                .map(|output| (job, output))
        }) else {
            return std::mem::replace(&mut shell.live_running, false);
        };
        if shell.live_running
            && shell.background == job.background
            && self.output_presentation.as_ref() == Some(output)
        {
            return false;
        }
        shell.live_running = true;
        shell.background = job.background;
        shell.exit = None;
        shell.signal = false;
        shell.timed_out = false;
        shell.cancelled = false;
        self.output_preview = output.body.clone();
        self.output_bytes = output.body_bytes.min(i64::MAX as u64) as i64;
        self.output_truncated = output.body_limited;
        self.output_presentation = Some(output.clone());
        true
    }

    /// Projection loss is independent of producer/capture loss. No text matching
    /// or cold output loading is needed to report the current viewing limit.
    pub(crate) fn preview_limited(&self) -> bool {
        matches!(&self.render, ToolRender::Shell(shell) if shell.command_limited)
            || matches!(
                &self.render,
                ToolRender::Inline(crate::tools::InlineRender::Generic {
                    arguments_limited: true,
                    ..
                })
            )
            || self
                .output_presentation
                .as_ref()
                .map_or(self.output_truncated, |presentation| {
                    presentation.body_limited
                        || presentation
                            .shell
                            .as_ref()
                            .is_some_and(|shell| shell.stdout_limited || shell.stderr_limited)
                })
    }

    /// Retained payload bytes including parsed card/diff strings.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.op.len()
            + self
                .child_job
                .as_ref()
                .map_or(0, |job| job.retained_bytes())
            + self.name.len()
            + self.state.len()
            + self.input_preview.len()
            + self.output_preview.len()
            + self
                .output_presentation
                .as_ref()
                .map_or(0, |presentation| presentation.retained_bytes())
            + self.files.iter().map(String::len).sum::<usize>()
            + self.render.retained_bytes()
            + self.question.as_ref().map_or(0, |result| {
                result
                    .questions
                    .iter()
                    .map(|q| {
                        q.question.len()
                            + q.header.len()
                            + q.options
                                .iter()
                                .map(|o| o.label.len() + o.description.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
                    + result
                        .answers
                        .iter()
                        .flatten()
                        .map(String::len)
                        .sum::<usize>()
            })
            + self.patch_effects.as_ref().map_or(0, |effects| {
                effects
                    .files
                    .iter()
                    .map(|f| {
                        f.path.len()
                            + f.destination.as_ref().map_or(0, String::len)
                            + f.hunks
                                .iter()
                                .flat_map(|h| &h.lines)
                                .map(|l| l.text.len())
                                .sum::<usize>()
                    })
                    .sum::<usize>()
            })
            + self.diff.as_ref().map_or(0, |d| {
                d.files
                    .iter()
                    .map(|f| f.path.len() + f.move_to.as_ref().map_or(0, String::len))
                    .sum::<usize>()
            })
    }
}

/// Build one bounded card from a recorded tool operation.
pub fn card_from_row(row: &ToolOpView) -> ToolCard {
    let output_presentation = row
        .output_presentation
        .clone()
        .filter(|presentation| presentation.is_valid());
    // Presentation only: keep the durable/provider structured rejection envelope
    // and its byte offsets intact; render its typed meaning to a human.
    let human = if row.name != "compress" && matches!(row.state.as_str(), "denied" | "cancelled") {
        row.output.as_deref().and_then(permission_output)
    } else {
        None
    };
    let output = human
        .as_deref()
        .or_else(|| {
            output_presentation
                .as_ref()
                .map(|presentation| presentation.body.as_str())
        })
        .or(row.output.as_deref());
    let files = row
        .patch_effects
        .as_ref()
        .filter(|effects| !(effects.files.is_empty() && human.is_some()))
        .map_or_else(
            || patch_files(&row.name, row.input.as_deref()),
            |effects| {
                effects
                    .files
                    .iter()
                    .map(|f| f.destination.as_ref().unwrap_or(&f.path).clone())
                    .collect()
            },
        );
    let files_truncated = files.len() > CARD_FILES
        || row
            .patch_effects
            .as_ref()
            .is_some_and(|effects| effects.total_files > files.len());
    let diff = if row.patch_effects.is_some() {
        None
    } else {
        patch_diff(&row.name, row.input.as_deref())
    };
    let mut render = if row.name == "compress" {
        ToolRender::Dcp(Box::new(crate::dcp_view::DcpRender {
            snapshot: row.dcp.clone().filter(|run| run.operation_id == row.op),
            topic: row.dcp_topic.clone(),
            ..Default::default()
        }))
    } else if matches!(row.name.as_str(), "apply_patch" | "edit" | "write")
        && row.patch_effects.is_some()
    {
        ToolRender::Patch(Default::default())
    } else {
        ToolRender::parse(&row.name, row.input.as_deref(), output, &row.state)
    };
    if let ToolRender::Shell(shell) = &mut render
        && let Some(facts) = output_presentation
            .as_ref()
            .and_then(|presentation| presentation.shell.as_ref())
    {
        shell.background = facts.background;
        shell.process_state = facts.process_state.clone();
        shell.stdout = facts.stdout.lines().map(str::to_owned).collect();
        shell.stderr = facts.stderr.lines().map(str::to_owned).collect();
        shell.exit = facts.exit.map(i64::from);
        shell.signal = facts.signal.is_some();
        shell.timed_out = facts.timed_out;
        shell.cancelled = facts.cancelled;
        shell.output_ends_with_newline = if facts.stderr.is_empty() {
            facts.stdout.ends_with('\n')
        } else {
            facts.stderr.ends_with('\n')
        };
    }
    if let ToolRender::Subagent(render) = &mut render {
        render.live_running = row.child_job.as_ref().is_some_and(|job| {
            job.operation == row.op
                && matches!(
                    job.state,
                    oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running
                )
        });
    }
    ToolCard {
        child_job: row.child_job.clone().filter(|job| {
            row.name == "subagent"
                && job.operation == row.op
                && job.retained_bytes() <= oc_core::tool_output::RECORD_BYTES
        }),
        started_at: None,
        op: row.op.clone(),
        name: row.name.clone(),
        state: row.state.clone(),
        input_preview: preview(row.input.as_deref()),
        output_preview: if human.is_none() && output_presentation.is_some() {
            output.unwrap_or_default().to_owned()
        } else {
            preview(output)
        },
        output_bytes: row.output_bytes,
        output_truncated: row.output_truncated,
        output_presentation,
        files: files.into_iter().take(CARD_FILES).collect(),
        files_truncated,
        diff,
        patch_effects: row.patch_effects.clone(),
        question: row.question.clone(),
        diff_settings: Default::default(),
        render,
    }
}

pub(crate) fn permission_output(output: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(output).ok()?;
    match value.get("status")?.as_str()? {
        "permission_rejected" => match value.get("feedback") {
            None | Some(serde_json::Value::Null) => Some("The user declined this tool call".into()),
            Some(serde_json::Value::String(feedback)) => Some(if feedback.is_empty() {
                "The user declined this tool call".into()
            } else {
                format!("The user declined this tool call: {feedback}")
            }),
            _ => None,
        },
        "permission_cancelled" => Some("The permission request was cancelled".into()),
        _ => None,
    }
}

/// Bounded diff summary for `apply_patch` input (parsed, never invented).
fn patch_diff(tool: &str, input: Option<&str>) -> Option<oc_adapters::patch::DiffSummary> {
    if tool != "apply_patch" {
        return None;
    }
    let patch = patch_text(input)?;
    let summary = oc_adapters::patch::diff_summary(&patch);
    if summary.malformed || summary.files.is_empty() {
        return None;
    }
    Some(summary)
}

fn patch_text(input: Option<&str>) -> Option<String> {
    let input = input?;
    let value = serde_json::from_str::<serde_json::Value>(input).ok()?;
    value
        .get("patchText")
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Build bounded cards for a page of recorded tool operations.
pub fn cards_from_rows(rows: &[ToolOpView]) -> Vec<ToolCard> {
    rows.iter().map(card_from_row).collect()
}

fn patch_files(tool: &str, input: Option<&str>) -> Vec<String> {
    if tool != "apply_patch" {
        return Vec::new();
    }
    let Some(patch) = patch_text(input) else {
        return Vec::new();
    };
    oc_adapters::patch::affected_paths(&patch).unwrap_or_default()
}

fn preview(value: Option<&str>) -> String {
    let text = value.unwrap_or("");
    if text.len() <= CARD_PREVIEW {
        return text.to_string();
    }
    let cut = crate::truncate_utf8(text, CARD_PREVIEW).len();
    format!("{}…[+{}]", &text[..cut], text.len() - cut)
}

#[cfg(test)]
mod user_shell_tests;

#[cfg(test)]
#[path = "history/child_tests.rs"]
mod child_tests;

#[cfg(test)]
mod tests {
    use super::{
        CARD_FILES, CARD_PREVIEW, HistoryWindow, WINDOW_BYTES, WINDOW_ROWS, card_from_row,
        cards_from_rows,
    };
    use oc_core::queries::{HistoryMessage, HistoryPage, ToolOpView};
    use oc_core::session::Role;

    fn row(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            id: oc_core::session::MessageId(format!("fixture-{seq}")),
            turn: None,
            model_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
            seq,
            role,
            text: text.to_string(),
        }
    }

    #[test]
    fn model_switch_replays_before_user_across_pages_and_live_receipts() {
        let notice = oc_core::queries::ModelSwitchNotice {
            previous: oc_core::queries::ModelRef {
                provider: "p".into(),
                id: "old".into(),
                variant: None,
            },
            current: oc_core::queries::ModelRef {
                provider: "p".into(),
                id: "new".into(),
                variant: Some("high".into()),
            },
            display_name: Some("Catalog Name".into()),
        };
        let mut marker = row(12, Role::Assistant, "");
        marker.model_switch = Some(notice);
        let user = row(13, Role::User, "accepted prompt");
        let mut window = super::HistoryWindow::new();
        window.reset(&page(vec![user.clone()], 2, true, false));
        window.prepend_older(&page(vec![marker.clone()], 2, false, true));
        assert_eq!(window.rows()[0].seq, 12);
        assert_eq!(window.rows()[0].role, "model_switch");
        assert_eq!(
            window.rows()[0].text,
            "Switched model to Catalog Name (high)"
        );
        assert_eq!(window.rows()[1].text, "accepted prompt");
        window.reset(&page(vec![marker.clone(), user], 2, false, false));
        assert_eq!(
            window.rows()[0].text,
            "Switched model to Catalog Name (high)"
        );
        window.push_synthetic("user", "next", None, None);
        window.insert_before_live_user(super::model_switch_text(
            marker.model_switch.as_ref().unwrap(),
        ));
        assert_eq!(window.rows()[2].role, "model_switch");
        assert_eq!(
            window.rows()[2].text,
            "Switched model to Catalog Name (high)"
        );
        assert_eq!(window.rows()[3].text, "next");
    }

    #[test]
    fn durable_identity_is_shared_by_derived_rows_and_absent_from_synthetic_rows() {
        use oc_core::queries::{HistoryTurn, TranscriptPart};
        let mut message = row(71, Role::Assistant, "aggregate");
        message.id = oc_core::session::MessageId("opaque-durable-id".into());
        message.turn = Some(HistoryTurn {
            parts: vec![
                TranscriptPart::Text("first".into()),
                TranscriptPart::Text("second".into()),
            ],
            ..Default::default()
        });
        let mut window = super::HistoryWindow::new();
        window.reset(&page(vec![message.clone()], 1, false, false));
        assert!(window.rows().len() >= 2);
        let id = window.rows()[0].message_id.as_ref().unwrap();
        for derived in window.rows() {
            assert_eq!(derived.message_id.as_deref(), Some(&message.id));
            assert!(std::sync::Arc::ptr_eq(
                id,
                derived.message_id.as_ref().unwrap()
            ));
        }
        window.push_synthetic("user", "live", None, None);
        assert!(window.rows().last().unwrap().message_id.is_none());
        window.insert_before_live_user("switch".into());
        assert!(window.rows()[window.len() - 2].message_id.is_none());
        let mut oversized = row(72, Role::User, "small text");
        oversized.id.0 = "x".repeat(super::WINDOW_BYTES + 1);
        window.reset(&page(vec![oversized], 1, false, false));
        assert!(
            window.is_empty(),
            "identity bytes participate in the retained cap"
        );
    }

    #[test]
    fn model_switch_labels_variant_default_and_missing_catalog_safely() {
        use oc_core::queries::{ModelRef, ModelSwitchNotice};
        let old = ModelRef {
            provider: "p".into(),
            id: "old".into(),
            variant: None,
        };
        let mut notice = ModelSwitchNotice {
            previous: old.clone(),
            current: ModelRef {
                provider: "p".into(),
                id: "new".into(),
                variant: Some("fast".into()),
            },
            display_name: None,
        };
        assert_eq!(
            super::model_switch_text(&notice),
            "Switched model to p/new/fast"
        );
        notice.display_name = Some("Readable".into());
        notice.current.variant = None;
        assert_eq!(
            super::model_switch_text(&notice),
            "Switched model to Readable"
        );
        notice.previous = notice.current.clone();
        notice.current.variant = Some("fast".into());
        assert_eq!(
            super::model_switch_text(&notice),
            "Switched variant to fast"
        );
        notice.current.variant = None;
        assert_eq!(
            super::model_switch_text(&notice),
            "Switched variant to default"
        );
        notice.previous = old;
        notice.current.id = "new".repeat(500);
        notice.display_name = None;
        assert!(super::model_switch_text(&notice).chars().count() <= 512);
    }

    fn page(rows: Vec<HistoryMessage>, total: usize, older: bool, newer: bool) -> HistoryPage {
        HistoryPage {
            child_job: None,
            parent_id: None,
            title: None,
            reverted: None,
            rows,
            total,
            has_older: older,
            has_newer: newer,
        }
    }

    #[test]
    fn v02_availability_markers_and_exact_terminal_states() {
        use oc_core::queries::{HistoryTurn, PartState, TranscriptPart};
        let mut message = row(1, Role::Assistant, "legacy answer");
        message.turn = Some(HistoryTurn {
            legacy_text_only: true,
            ..Default::default()
        });
        let legacy = super::rows_from_page(&message);
        assert!(legacy.iter().any(|r| r.text.contains("Legacy text-only")));
        assert!(legacy.iter().all(|r| r.reasoning.is_none()));
        for status in ["failed", "cancelled", "incomplete", "unknown"] {
            message.turn = Some(HistoryTurn {
                status: status.into(),
                agent: Some("old-agent".into()),
                agent_color_index: Some(3),
                parts: vec![TranscriptPart::Text("preview".into())],
                part_states: vec![PartState {
                    truncated: true,
                    ..Default::default()
                }],
                omitted_parts: 8,
                truncated: true,
                ..Default::default()
            });
            let rows = super::rows_from_page(&message);
            assert_eq!(
                rows.len(),
                2,
                "available body and actual terminal footer only"
            );
            assert_eq!(rows[0].text, "preview");
            let meta = rows.last().unwrap().meta.as_ref().unwrap();
            assert!(meta.preview_limited);
            assert_eq!(meta.status.as_deref(), Some(status));
            assert_eq!(meta.agent_color_index, Some(3));
        }
    }

    #[test]
    fn started_turn_with_two_completed_reasoning_parts_replays_as_completed_group() {
        use oc_core::queries::{HistoryTurn, TranscriptPart};
        use ratatui::style::Color;

        let mut message = row(12, Role::Assistant, "");
        message.turn = Some(HistoryTurn {
            status: "started".into(),
            parts: vec![
                TranscriptPart::Reasoning {
                    text: "**Inspecting**\n\nfirst".into(),
                    duration_ms: Some(5),
                },
                TranscriptPart::Reasoning {
                    text: "**Verifying**\n\nsecond".into(),
                    duration_ms: Some(7),
                },
            ],
            ..Default::default()
        });
        let rows = super::rows_from_page(&message);
        assert_eq!(rows.len(), 3, "adjacent parts and a started footer");
        assert!(
            rows[..2]
                .iter()
                .all(|row| !row.reasoning.as_ref().unwrap().running)
        );
        assert_eq!(
            rows[2].meta.as_ref().unwrap().status.as_deref(),
            Some("started")
        );
        let lines = crate::messages::transcript(&rows, crate::theme::Theme::dark(), 80, 80, |_| {
            Color::Reset
        });
        assert!(lines.iter().any(|line| {
            line.plain_text()
                .contains("Thought: Verifying · 2 steps · 12ms")
        }));
        assert!(
            !lines
                .iter()
                .any(|line| line.plain_text().contains("Thinking"))
        );
    }

    #[test]
    fn started_turn_with_open_reasoning_part_replays_as_running_group() {
        use oc_core::queries::{HistoryTurn, TranscriptPart};
        use ratatui::style::Color;

        let mut message = row(13, Role::Assistant, "");
        message.turn = Some(HistoryTurn {
            status: "started".into(),
            parts: vec![
                TranscriptPart::Reasoning {
                    text: "**Inspecting**\n\nfirst".into(),
                    duration_ms: Some(5),
                },
                TranscriptPart::Reasoning {
                    text: "**Verifying**\n\nstill open".into(),
                    duration_ms: None,
                },
            ],
            ..Default::default()
        });
        let rows = super::rows_from_page(&message);
        assert_eq!(rows[0].reasoning.as_ref().unwrap().duration_ms, Some(5));
        assert!(!rows[0].reasoning.as_ref().unwrap().running);
        assert_eq!(rows[1].reasoning.as_ref().unwrap().duration_ms, None);
        assert!(rows[1].reasoning.as_ref().unwrap().running);
        let lines = crate::messages::transcript(&rows, crate::theme::Theme::dark(), 80, 80, |_| {
            Color::Reset
        });
        assert!(
            lines
                .iter()
                .any(|line| line.plain_text().contains("Thinking: Verifying"))
        );

        for status in ["failed", "incomplete", "cancelled"] {
            message.turn.as_mut().unwrap().status = status.into();
            let rows = super::rows_from_page(&message);
            assert!(
                rows[..2]
                    .iter()
                    .all(|row| !row.reasoning.as_ref().unwrap().running)
            );
            assert_eq!(rows[1].reasoning.as_ref().unwrap().duration_ms, None);
        }
    }

    #[test]
    fn truncated_reasoning_preview_keeps_adjacent_steps_and_indexed_click() {
        use crate::messages::{MarkdownCache, ReasoningIdentity};
        use oc_core::queries::{HistoryTurn, PartState, TranscriptPart};
        use ratatui::style::Color;
        use std::cell::RefCell;

        // The stored preview is already bounded: never reconstruct omitted
        // content from a part-state flag or substitute a notice for a part.
        let prefix = "**Inspecting**\n\nfirst body\n\n";
        let original = format!(
            "{prefix}{}hidden marker",
            "x".repeat(16 * 1024 - prefix.len())
        );
        let first = original[..16 * 1024].to_string();
        assert!(original.len() > 16 * 1024);
        let mut message = row(42, Role::Assistant, "aggregate must not be replayed");
        message.turn = Some(HistoryTurn {
            status: "completed".into(),
            parts: vec![
                TranscriptPart::Reasoning {
                    text: first.clone(),
                    duration_ms: Some(5),
                },
                TranscriptPart::Reasoning {
                    text: "**Verifying**\n\nsecond body".into(),
                    duration_ms: Some(7),
                },
                TranscriptPart::Text("answer".into()),
            ],
            part_states: vec![
                PartState {
                    truncated: true,
                    ..Default::default()
                },
                PartState::default(),
                PartState::default(),
            ],
            ..Default::default()
        });
        let mut window = HistoryWindow::new();
        window.reset(&page(
            vec![message, row(43, Role::User, "next prompt")],
            2,
            false,
            false,
        ));
        let rows = window.rows();
        assert!(
            !rows
                .iter()
                .any(|row| row.text == "[Part preview truncated]"),
            "projection loss belongs to compact viewing status, not an answer row"
        );
        assert_eq!(rows.len(), 5);
        assert!(window.retained_bytes() <= WINDOW_BYTES);
        assert_eq!(rows[0].reasoning.as_ref().unwrap().text, first);
        assert_eq!(rows[0].reasoning.as_ref().unwrap().text.len(), 16 * 1024);
        assert_eq!(
            rows[0].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(42, 0))
        );
        assert_eq!(
            rows[1].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(42, 1))
        );
        assert_eq!(rows[2].text, "answer");
        assert!(rows[3].meta.as_ref().unwrap().preview_limited);
        assert_eq!(rows[4].role, "user");
        let theme = crate::theme::Theme::dark();
        let plain = |rows: &[super::HistoryRow]| {
            crate::messages::transcript(rows, theme, 80, 80, |_| Color::Reset)
                .iter()
                .map(|line| line.plain_text())
                .collect::<Vec<_>>()
        };
        let collapsed = plain(rows);
        assert_eq!(
            collapsed
                .iter()
                .filter(|line| line.contains("· 2 steps"))
                .count(),
            1
        );
        assert!(
            collapsed
                .iter()
                .any(|line| line.contains("Thought: Verifying · 2 steps · 12ms"))
        );
        assert!(
            !collapsed
                .iter()
                .any(|line| line.contains("first body") || line.contains("second body"))
        );
        assert!(
            !collapsed
                .iter()
                .any(|line| line.contains("[Part preview truncated]"))
        );
        assert!(
            !collapsed
                .iter()
                .any(|line| line.contains("aggregate must not be replayed")
                    || line.contains("hidden marker"))
        );

        let cache = RefCell::new(MarkdownCache::default());
        let (indexed, total) = crate::messages::visible_transcript_expanded(
            rows,
            theme,
            (80, 80),
            (40, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert!(total >= indexed.len());
        assert_eq!(
            crate::messages::reasoning_header_at(
                rows,
                theme,
                (80, 80),
                (total, 0, None),
                |_| Color::Reset,
                &cache,
                (&|_| false, (3, 2)),
            ),
            Some(ReasoningIdentity::Durable(42, 0)),
        );
        let mut expanded_rows = rows.to_vec();
        expanded_rows[0].reasoning.as_mut().unwrap().expanded = true;
        let expanded = plain(&expanded_rows);
        for expected in ["first body", "second body", "answer", "next prompt"] {
            assert!(
                expanded.iter().any(|line| line.contains(expected)),
                "missing {expected}"
            );
        }
        assert!(!expanded.iter().any(|line| line.contains("hidden marker")));
        assert!(
            !expanded
                .iter()
                .any(|line| line.contains("[Part preview truncated]"))
        );
        assert_eq!(
            expanded
                .iter()
                .filter(|line| line.contains("· 2 steps"))
                .count(),
            1
        );
        let (visible, total) = crate::messages::visible_transcript_expanded(
            &expanded_rows,
            theme,
            (80, 80),
            (expanded.len() + 1, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(total, visible.len());
        assert_eq!(
            crate::messages::reasoning_header_at(
                &expanded_rows,
                theme,
                (80, 80),
                (total, 0, None),
                |_| Color::Reset,
                &cache,
                (&|_| false, (3, 2)),
            ),
            Some(ReasoningIdentity::Durable(42, 0)),
        );
        for expected in ["first body", "second body"] {
            assert!(
                visible
                    .iter()
                    .any(|line| line.plain_text().contains(expected)),
                "indexed replay missing {expected}"
            );
        }
    }

    #[test]
    fn limited_parts_keep_order_and_reasoning_groups_without_synthetic_rows() {
        use crate::messages::ReasoningIdentity;
        use oc_core::queries::{HistoryTurn, PartState, TranscriptPart};
        use ratatui::style::Color;

        let reason = |text: &str| TranscriptPart::Reasoning {
            text: text.into(),
            duration_ms: Some(1),
        };
        let mut message = row(7, Role::Assistant, "");
        message.turn = Some(HistoryTurn {
            parts: vec![
                reason("**One**\n\nbody 1"),
                reason("**Two**\n\nbody 2"),
                TranscriptPart::Tool(ToolOpView {
                    child_job: None,
                    output_presentation: None,
                    question: None,
                    rowid: 0,
                    op: "op-1".into(),
                    name: "bash".into(),
                    state: "completed".into(),
                    input: None,
                    output: None,
                    output_bytes: 0,
                    output_truncated: false,
                    patch_effects: None,
                    dcp: None,
                    dcp_topic: None,
                }),
                reason("**Three**\n\nbody 3"),
                TranscriptPart::Text("separator [Part preview truncated]".into()),
                reason("**Four**\n\nbody 4"),
            ],
            part_states: vec![
                PartState {
                    truncated: true,
                    ..Default::default()
                },
                PartState {
                    truncated: true,
                    ..Default::default()
                },
                PartState {
                    truncated: true,
                    input_omitted: true,
                    ..Default::default()
                },
                PartState {
                    truncated: true,
                    ..Default::default()
                },
                PartState {
                    truncated: true,
                    ..Default::default()
                },
                PartState {
                    truncated: true,
                    ..Default::default()
                },
            ],
            omitted_parts: 2,
            ..Default::default()
        });
        let rows = super::rows_from_page(&message);
        assert_eq!(rows.len(), 7);
        assert_eq!(
            rows[0].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 0))
        );
        assert_eq!(
            rows[1].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 1))
        );
        assert_eq!(rows[2].tool.as_ref().unwrap().op, "op-1");
        assert_eq!(
            rows[3].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 3))
        );
        assert_eq!(rows[4].text, "separator [Part preview truncated]");
        assert_eq!(
            rows[5].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 5))
        );
        assert!(rows[6].meta.as_ref().unwrap().preview_limited);
        let lines = crate::messages::transcript(&rows, crate::theme::Theme::dark(), 80, 80, |_| {
            Color::Reset
        });
        let plain: Vec<_> = lines.iter().map(|line| line.plain_text()).collect();
        assert_eq!(
            plain
                .iter()
                .filter(|line| line.contains("· 2 steps"))
                .count(),
            1
        );
        assert!(
            plain
                .iter()
                .any(|line| line.contains("Thought: Two · 2 steps"))
        );
        assert!(plain.iter().any(|line| line.contains("Thought: Three")));
        assert!(plain.iter().any(|line| line.contains("Thought: Four")));
        assert_eq!(
            plain
                .iter()
                .filter(|line| line.contains("[Part preview truncated]"))
                .count(),
            1,
            "literal response text is not a generated viewing notice"
        );
        assert!(
            !plain
                .iter()
                .any(|line| line.contains("parts omitted") || line.contains("Tool input omitted"))
        );
    }

    #[test]
    fn user_history_row_restores_its_turn_agent_for_message_color() {
        use oc_core::queries::HistoryTurn;

        let mut message = row(1, Role::User, "sent as orange");
        message.turn = Some(HistoryTurn {
            agent: Some("orange-profile".into()),
            agent_color_index: Some(1),
            ..Default::default()
        });

        let rows = super::rows_from_page(&message);
        let user = rows
            .iter()
            .find(|row| row.role == "user")
            .expect("user history row");
        assert_eq!(user.agent.as_deref(), Some("orange-profile"));
        assert_eq!(user.agent_color_index, Some(1));
    }

    #[test]
    fn expanded_parts_count_as_rendered_rows_for_scroll_anchoring() {
        use oc_core::queries::{HistoryTurn, TranscriptPart};
        let mut message = row(2, Role::Assistant, "aggregate");
        message.turn = Some(HistoryTurn {
            parts: vec![
                TranscriptPart::Text("first".into()),
                TranscriptPart::Text("second".into()),
            ],
            ..Default::default()
        });
        let page = page(vec![message], 1, false, false);
        let mut window = super::HistoryWindow::new();
        assert_eq!(window.append_newer(&page), 3, "two parts plus footer");
        assert_eq!(
            window.prepend_older(&page),
            3,
            "scroll offset counts projected parts"
        );
    }

    #[test]
    fn paging_newest_first_sets_flags() {
        let mut window = HistoryWindow::new();
        // Newest page (m3, m4): older rows exist, nothing newer.
        window.reset(&page(
            vec![row(3, Role::User, "m3"), row(4, Role::Assistant, "m4")],
            5,
            true,
            false,
        ));
        assert_eq!(window.len(), 2);
        assert_eq!(window.total(), 5);
        assert!(window.has_older());
        assert!(!window.has_newer());
        assert_eq!(window.rows()[0].text, "m3");
        assert_eq!(window.rows()[1].role, "assistant");

        // One older page: window keeps the newest rows, has_newer stays false.
        assert_eq!(
            window.prepend_older(&page(vec![row(2, Role::Assistant, "m2")], 5, true, true)),
            1
        );
        assert!(window.has_older());
        assert!(!window.has_newer());
        assert_eq!(window.rows().first().expect("first").text, "m2");

        // Oldest page: no older rows remain.
        window.prepend_older(&page(vec![row(1, Role::User, "m1")], 5, false, true));
        assert!(!window.has_older());
        assert_eq!(window.rows().len(), 4);

        // Newer append makes the window live at the tail again.
        window.append_newer(&page(vec![row(5, Role::User, "m5")], 5, false, false));
        assert!(!window.has_newer());
        assert_eq!(window.rows().last().expect("last").text, "m5");
        assert_eq!(window.total(), 5);
    }

    #[test]
    fn reset_evicts_over_row_cap() {
        let rows: Vec<HistoryMessage> = (0..WINDOW_ROWS + 10)
            .map(|i| row(i as i64, Role::User, "x"))
            .collect();
        let mut window = HistoryWindow::new();
        window.reset(&page(rows, WINDOW_ROWS + 10, false, false));
        assert_eq!(window.len(), WINDOW_ROWS);
        assert!(window.has_older(), "evicted rows must stay reachable");
    }

    #[test]
    fn prepend_evicts_newest_and_flags_it() {
        let mut window = HistoryWindow::new();
        window.reset(&page(vec![row(9, Role::Assistant, "live")], 9, true, false));
        let bulk: Vec<HistoryMessage> = (0..WINDOW_ROWS + 5)
            .map(|i| row(i as i64, Role::User, "older"))
            .collect();
        let added = window.prepend_older(&page(bulk, 500, true, true));
        assert_eq!(added, WINDOW_ROWS + 5);
        assert_eq!(window.len(), WINDOW_ROWS);
        assert!(window.has_newer(), "newest rows were evicted");
        assert!(
            !window.rows().iter().any(|r| r.text == "live"),
            "evicted newest row must be gone"
        );
    }

    #[test]
    fn append_evicts_oldest_and_flags_it() {
        let mut window = HistoryWindow::new();
        window.reset(&page(vec![row(1, Role::User, "start")], 2, false, true));
        let bulk: Vec<HistoryMessage> = (0..WINDOW_ROWS + 5)
            .map(|i| row(10 + i as i64, Role::Assistant, "newer"))
            .collect();
        window.append_newer(&page(bulk, 500, true, false));
        assert_eq!(window.len(), WINDOW_ROWS);
        assert!(window.has_older(), "oldest rows were evicted");
        assert!(!window.has_newer());
    }

    #[test]
    fn byte_cap_is_enforced_on_every_insertion() {
        let blob = "y".repeat(4096);
        let mut window = HistoryWindow::new();
        for round in 0..40 {
            let rows: Vec<HistoryMessage> = (0..8)
                .map(|i| row(round * 8 + i, Role::Assistant, &blob))
                .collect();
            window.append_newer(&page(rows, 1000, true, round < 39));
            assert!(
                window.retained_bytes() <= WINDOW_BYTES,
                "round {round}: {} bytes retained",
                window.retained_bytes()
            );
            assert!(window.len() <= WINDOW_ROWS);
        }
        assert!(window.has_older());
    }

    #[test]
    fn card_from_row_bounds_previews_and_parses_patch_text_only() {
        let patch = "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** Update File: old.txt\n*** Move to: new.txt\n@@\n-old\n+new\n*** End Patch\n";
        let card = card_from_row(&ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 0,
            op: "op1".to_string(),
            name: "apply_patch".to_string(),
            state: "completed".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: Some("ok".to_string()),
            output_bytes: 0,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        assert_eq!(card.op, "op1");
        assert_eq!(card.state, "completed");
        assert_eq!(card.output_preview, "ok");
        assert_eq!(card.files, ["added.txt", "new.txt", "old.txt"]);
        assert!(!card.files_truncated);

        // Alias keys are never consulted, parse failures invent nothing.
        for alias in ["patch", "text"] {
            let card = card_from_row(&ToolOpView {
                child_job: None,
                output_presentation: None,
                question: None,
                rowid: 0,
                op: "op".to_string(),
                name: "apply_patch".to_string(),
                state: "started".to_string(),
                input: Some(serde_json::json!({ (alias): patch }).to_string()),
                output: None,
                output_bytes: 0,
                output_truncated: false,
                patch_effects: None,
                dcp: None,
                dcp_topic: None,
            });
            assert!(card.files.is_empty(), "alias {alias} must be ignored");
        }
        let card = card_from_row(&ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 0,
            op: "op".to_string(),
            name: "apply_patch".to_string(),
            state: "started".to_string(),
            input: Some("not json".to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        assert!(card.files.is_empty());
        assert!(card.output_preview.is_empty());

        // Non-apply_patch ops never list files, long fields are bounded.
        let long = "z".repeat(CARD_PREVIEW * 4);
        let card = card_from_row(&ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 0,
            op: "op".to_string(),
            name: "read".to_string(),
            state: "started".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: Some(long),
            output_bytes: 0,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        assert!(card.files.is_empty());
        assert!(card.input_preview.len() <= CARD_PREVIEW + 16);
        assert!(card.output_preview.len() <= CARD_PREVIEW + 16);

        // At most CARD_FILES + a truncation flag.
        let mut files = String::new();
        for i in 0..CARD_FILES + 3 {
            files.push_str(&format!("*** Add File: f{i}.txt\n+x\n"));
        }
        let patch = format!("*** Begin Patch\n{files}*** End Patch\n");
        let card = card_from_row(&ToolOpView {
            child_job: None,
            output_presentation: None,
            question: None,
            rowid: 0,
            op: "op".to_string(),
            name: "apply_patch".to_string(),
            state: "started".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
            patch_effects: None,
            dcp: None,
            dcp_topic: None,
        });
        assert_eq!(card.files.len(), CARD_FILES);
        assert!(card.files_truncated);
    }

    #[test]
    fn cards_from_rows_maps_every_row() {
        let rows = vec![
            ToolOpView {
                child_job: None,
                output_presentation: None,
                question: None,
                rowid: 0,
                op: "a".to_string(),
                name: "read".to_string(),
                state: "started".to_string(),
                input: None,
                output: None,
                output_bytes: 0,
                output_truncated: false,
                patch_effects: None,
                dcp: None,
                dcp_topic: None,
            },
            ToolOpView {
                child_job: None,
                output_presentation: None,
                question: None,
                rowid: 0,
                op: "b".to_string(),
                name: "bash".to_string(),
                state: "failed".to_string(),
                input: Some("{}".to_string()),
                output: Some("boom".to_string()),
                output_bytes: 0,
                output_truncated: false,
                patch_effects: None,
                dcp: None,
                dcp_topic: None,
            },
        ];
        let cards = cards_from_rows(&rows);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1].output_preview, "boom");
    }
}
