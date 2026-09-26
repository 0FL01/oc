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
    /// Message text.
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
        self.rows = page.rows.iter().flat_map(rows_from_page).collect();
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
            self.has_older = has_older;
            if self.enforce(Evict::Oldest) {
                self.has_older = true;
            }
        }
    }

    /// Add an older page at the front; returns rows added. Evicts newest
    /// rows while over a cap and flags `has_newer` when it does.
    pub fn prepend_older(&mut self, page: &HistoryPage) -> usize {
        let mut combined: Vec<HistoryRow> = page.rows.iter().flat_map(rows_from_page).collect();
        let added = combined.len();
        combined.append(&mut self.rows);
        self.rows = combined;
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
        let before = self.rows.len();
        self.rows.extend(page.rows.iter().flat_map(rows_from_page));
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
        });
        self.has_newer = false;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// Append one fully rendered row (live assistant message with reasoning
    /// and footer metadata); replayed rows use the same safe presentation data.
    pub(crate) fn push_row(&mut self, row: HistoryRow) {
        self.rows.push(row);
        self.has_newer = false;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// A live acceptance notice arrives after the prompt echo receipt.
    pub(crate) fn insert_before_live_user(&mut self, text: String) {
        if let Some(index) = self
            .rows
            .iter()
            .rposition(|row| row.seq == i64::MAX && row.role == "user")
        {
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
            .model_switch
            .as_ref()
            .map_or_else(|| row.text.clone(), model_switch_text),
        agent: row.turn.as_ref().and_then(|turn| turn.agent.clone()),
        agent_color_index: row.turn.as_ref().and_then(|turn| turn.agent_color_index),
        chips: Vec::new(),
        reasoning: None,
        meta: None,
        tool: None,
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
    };
    // A preview notice is not a durable part. Keep adjacent reasoning refs
    // adjacent for grouping, then show every notice before the next part.
    let mut reasoning_notices = Vec::new();
    for (index, part) in turn.parts.iter().enumerate() {
        if !matches!(part, TranscriptPart::Reasoning { .. }) {
            rows.append(&mut reasoning_notices);
        }
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
        rows.push(part_row);
        if let Some(state) = turn.part_states.get(index).filter(|s| s.truncated) {
            let mut notice = empty_row();
            notice.text = if state.input_omitted {
                match part {
                    TranscriptPart::Tool(op) => format!(
                        "[Tool input omitted from preview; operation {} retained; see /cards]",
                        op.op
                    ),
                    _ => "[Part preview truncated]".into(),
                }
            } else {
                "[Part preview truncated]".into()
            };
            if matches!(part, TranscriptPart::Reasoning { .. }) {
                reasoning_notices.push(notice);
            } else {
                rows.push(notice);
            }
        }
    }
    rows.append(&mut reasoning_notices);
    let mut footer = empty_row();
    if turn.omitted_parts > 0 {
        let mut notice = empty_row();
        notice.text = format!(
            "[{} parts omitted from bounded history preview; durable records retained]",
            turn.omitted_parts
        );
        rows.push(notice);
    }
    footer.meta = Some(AssistantMeta {
        model: Some(turn.model_label.clone()),
        duration_ms: turn.duration_ms,
        input_tokens: turn.usage.map(|v| v.0),
        output_tokens: turn.usage.map(|v| v.1),
        context_usage: turn.context_usage,
        streamed_ms: turn.streamed_ms,
        interrupted: turn.status == "cancelled",
        status: Some(turn.status.clone()),
        agent_color_index: turn.agent_color_index,
    });
    rows.push(footer);
    rows
}

/// One tool card: intent + outcome + bounded previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCard {
    /// Operation id.
    pub op: String,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `unknown`.
    pub state: String,
    /// Bounded input preview.
    pub input_preview: String,
    /// Bounded output preview (empty when none recorded).
    pub output_preview: String,
    /// Full stored output size in bytes.
    pub output_bytes: i64,
    /// True when the durable result is longer than the preview.
    pub output_truncated: bool,
    /// Affected paths for `apply_patch` (parsed, never invented).
    pub files: Vec<String>,
    /// True when more files exist than listed.
    pub files_truncated: bool,
    /// Bounded diff representation for `apply_patch` (counts and paths only,
    /// never a second copy of the patch bytes); `None` for other tools.
    pub diff: Option<oc_adapters::patch::DiffSummary>,
    /// Presentation data parsed once from the recorded input/output
    /// (bounded); the transcript renders the card from it.
    pub render: ToolRender,
}

impl ToolCard {
    /// Retained payload bytes including parsed card/diff strings.
    pub(crate) fn retained_bytes(&self) -> usize {
        self.op.len()
            + self.name.len()
            + self.state.len()
            + self.input_preview.len()
            + self.output_preview.len()
            + self.files.iter().map(String::len).sum::<usize>()
            + self.render.retained_bytes()
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
    let files = patch_files(&row.name, row.input.as_deref());
    let files_truncated = files.len() > CARD_FILES;
    let diff = patch_diff(&row.name, row.input.as_deref());
    let render = ToolRender::parse(
        &row.name,
        row.input.as_deref(),
        row.output.as_deref(),
        &row.state,
    );
    ToolCard {
        op: row.op.clone(),
        name: row.name.clone(),
        state: row.state.clone(),
        input_preview: preview(row.input.as_deref()),
        output_preview: preview(row.output.as_deref()),
        output_bytes: row.output_bytes,
        output_truncated: row.output_truncated,
        files: files.into_iter().take(CARD_FILES).collect(),
        files_truncated,
        diff,
        render,
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
            assert!(rows.iter().any(|r| r.text.contains("preview truncated")));
            assert!(rows.iter().any(|r| r.text.contains("8 parts omitted")));
            let meta = rows.last().unwrap().meta.as_ref().unwrap();
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
        assert_eq!(rows.len(), 6);
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
        assert_eq!(rows[2].text, "[Part preview truncated]");
        assert_eq!(rows[3].text, "answer");
        assert!(rows[4].meta.is_some());
        assert_eq!(rows[5].role, "user");
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
            collapsed
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
        for expected in [
            "first body",
            "second body",
            "[Part preview truncated]",
            "answer",
            "next prompt",
        ] {
            assert!(
                expanded.iter().any(|line| line.contains(expected)),
                "missing {expected}"
            );
        }
        assert!(!expanded.iter().any(|line| line.contains("hidden marker")));
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
        for expected in ["first body", "second body", "[Part preview truncated]"] {
            assert!(
                visible
                    .iter()
                    .any(|line| line.plain_text().contains(expected)),
                "indexed replay missing {expected}"
            );
        }
    }

    #[test]
    fn reasoning_notices_flush_before_text_tool_and_footer_without_hiding_other_notices() {
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
                    rowid: 0,
                    op: "op-1".into(),
                    name: "bash".into(),
                    state: "completed".into(),
                    input: None,
                    output: None,
                    output_bytes: 0,
                    output_truncated: false,
                }),
                reason("**Three**\n\nbody 3"),
                TranscriptPart::Text("separator".into()),
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
        assert_eq!(rows.len(), 14);
        assert_eq!(
            rows[0].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 0))
        );
        assert_eq!(
            rows[1].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 1))
        );
        assert_eq!(rows[2].text, "[Part preview truncated]");
        assert_eq!(rows[3].text, "[Part preview truncated]");
        assert_eq!(rows[4].tool.as_ref().unwrap().op, "op-1");
        assert_eq!(
            rows[5].text,
            "[Tool input omitted from preview; operation op-1 retained; see /cards]"
        );
        assert_eq!(
            rows[6].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 3))
        );
        assert_eq!(rows[7].text, "[Part preview truncated]");
        assert_eq!(rows[8].text, "separator");
        assert_eq!(rows[9].text, "[Part preview truncated]");
        assert_eq!(
            rows[10].reasoning.as_ref().unwrap().identity,
            Some(ReasoningIdentity::Durable(7, 5))
        );
        assert_eq!(rows[11].text, "[Part preview truncated]");
        assert!(rows[12].text.contains("2 parts omitted"));
        assert!(rows[13].meta.is_some());
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
            rowid: 0,
            op: "op1".to_string(),
            name: "apply_patch".to_string(),
            state: "completed".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: Some("ok".to_string()),
            output_bytes: 0,
            output_truncated: false,
        });
        assert_eq!(card.op, "op1");
        assert_eq!(card.state, "completed");
        assert_eq!(card.output_preview, "ok");
        assert_eq!(card.files, ["added.txt", "new.txt", "old.txt"]);
        assert!(!card.files_truncated);

        // Alias keys are never consulted, parse failures invent nothing.
        for alias in ["patch", "text"] {
            let card = card_from_row(&ToolOpView {
                rowid: 0,
                op: "op".to_string(),
                name: "apply_patch".to_string(),
                state: "started".to_string(),
                input: Some(serde_json::json!({ (alias): patch }).to_string()),
                output: None,
                output_bytes: 0,
                output_truncated: false,
            });
            assert!(card.files.is_empty(), "alias {alias} must be ignored");
        }
        let card = card_from_row(&ToolOpView {
            rowid: 0,
            op: "op".to_string(),
            name: "apply_patch".to_string(),
            state: "started".to_string(),
            input: Some("not json".to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
        });
        assert!(card.files.is_empty());
        assert!(card.output_preview.is_empty());

        // Non-apply_patch ops never list files, long fields are bounded.
        let long = "z".repeat(CARD_PREVIEW * 4);
        let card = card_from_row(&ToolOpView {
            rowid: 0,
            op: "op".to_string(),
            name: "read".to_string(),
            state: "started".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: Some(long),
            output_bytes: 0,
            output_truncated: false,
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
            rowid: 0,
            op: "op".to_string(),
            name: "apply_patch".to_string(),
            state: "started".to_string(),
            input: Some(serde_json::json!({ "patchText": patch }).to_string()),
            output: None,
            output_bytes: 0,
            output_truncated: false,
        });
        assert_eq!(card.files.len(), CARD_FILES);
        assert!(card.files_truncated);
    }

    #[test]
    fn cards_from_rows_maps_every_row() {
        let rows = vec![
            ToolOpView {
                rowid: 0,
                op: "a".to_string(),
                name: "read".to_string(),
                state: "started".to_string(),
                input: None,
                output: None,
                output_bytes: 0,
                output_truncated: false,
            },
            ToolOpView {
                rowid: 0,
                op: "b".to_string(),
                name: "bash".to_string(),
                state: "failed".to_string(),
                input: Some("{}".to_string()),
                output: Some("boom".to_string()),
                output_bytes: 0,
                output_truncated: false,
            },
        ];
        let cards = cards_from_rows(&rows);
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[1].output_preview, "boom");
    }
}
