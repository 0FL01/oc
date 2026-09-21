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

/// Max rows retained by the window.
pub const WINDOW_ROWS: usize = 240;
/// Max retained bytes (`role.len() + text.len()`) in the window.
pub const WINDOW_BYTES: usize = 256 * 1024;
/// Max preview chars per card field.
pub const CARD_PREVIEW: usize = 512;
/// Max files listed on a patch card.
pub const CARD_FILES: usize = 5;

/// One history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    /// Message sequence (ordering key for paging); `i64::MAX` for synthetic
    /// rows that are not committed yet.
    pub seq: i64,
    /// Render prefix (`user` / `assistant` for committed rows).
    pub role: String,
    /// Message text.
    pub text: String,
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
        self.rows = page.rows.iter().map(row_from_page).collect();
        self.total = page.total;
        self.has_older = page.has_older;
        self.has_newer = page.has_newer;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
        }
    }

    /// Add an older page at the front; returns rows added. Evicts newest
    /// rows while over a cap and flags `has_newer` when it does.
    pub fn prepend_older(&mut self, page: &HistoryPage) -> usize {
        let added = page.rows.len();
        let mut combined: Vec<HistoryRow> = page.rows.iter().map(row_from_page).collect();
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
        let added = page.rows.len();
        self.rows.extend(page.rows.iter().map(row_from_page));
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

    /// Sum of `role.len() + text.len()` over retained rows.
    pub fn retained_bytes(&self) -> usize {
        self.rows
            .iter()
            .map(|row| row.role.len() + row.text.len())
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
    pub(crate) fn push_synthetic(&mut self, role: &str, text: &str) {
        self.rows.push(HistoryRow {
            seq: i64::MAX,
            role: role.to_string(),
            text: text.to_string(),
        });
        self.has_newer = false;
        if self.enforce(Evict::Oldest) {
            self.has_older = true;
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

fn row_from_page(row: &HistoryMessage) -> HistoryRow {
    HistoryRow {
        seq: row.seq,
        role: match row.role {
            Role::User => "user".to_string(),
            Role::Assistant => "assistant".to_string(),
        },
        text: row.text.clone(),
    }
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
}

/// Build one bounded card from a recorded tool operation.
pub fn card_from_row(row: &ToolOpView) -> ToolCard {
    let files = patch_files(&row.name, row.input.as_deref());
    let files_truncated = files.len() > CARD_FILES;
    let diff = patch_diff(&row.name, row.input.as_deref());
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
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>, total: usize, older: bool, newer: bool) -> HistoryPage {
        HistoryPage {
            rows,
            total,
            has_older: older,
            has_newer: newer,
        }
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
