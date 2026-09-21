//! Session history pager and tool cards (UI03).
//!
//! Newest-first pages from [`Db`] keep the backing store bounded: the view
//! never renders the whole history. Tool cards pair recorded intents with
//! outcomes; `apply_patch` cards additionally list affected paths parsed
//! from the recorded input (parse failures show no files, never invented
//! ones).

use oc_adapters::storage::{Db, StorageError};

/// Max preview chars per card field.
pub const CARD_PREVIEW: usize = 512;
/// Max files listed on a patch card.
pub const CARD_FILES: usize = 5;

/// One history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    /// Message sequence (ordering key for paging).
    pub seq: i64,
    /// `user` / `assistant`.
    pub role: String,
    /// Message text.
    pub text: String,
}

/// Newest-first pager over one session.
pub struct HistoryPager {
    session: String,
    /// Total committed messages.
    pub total: usize,
    /// Loaded rows, oldest-first (render order).
    loaded: Vec<HistoryRow>,
    exhausted: bool,
}

impl HistoryPager {
    /// Open a pager; unknown sessions fail instead of showing empty.
    pub fn open(db: &Db, session: &str) -> Result<Self, StorageError> {
        let total = db.history_len(session)?;
        Ok(Self {
            session: session.to_string(),
            total,
            loaded: Vec::new(),
            exhausted: false,
        })
    }

    /// Session id this pager reads.
    pub fn session(&self) -> &str {
        &self.session
    }

    /// Loaded rows in render order (oldest first).
    pub fn rows(&self) -> &[HistoryRow] {
        &self.loaded
    }

    /// True when every committed message is loaded.
    pub fn exhausted(&self) -> bool {
        self.exhausted
    }

    /// Load the next older page; returns rows added.
    pub fn load_older(&mut self, db: &Db, limit: usize) -> Result<usize, StorageError> {
        if self.exhausted {
            return Ok(0);
        }
        let before = self.loaded.first().map(|row| row.seq);
        let page = db.read_history_page(&self.session, limit, before)?;
        if page.is_empty() {
            self.exhausted = true;
            return Ok(0);
        }
        let mut rows: Vec<HistoryRow> = page
            .into_iter()
            .map(|(seq, role, text)| HistoryRow { seq, role, text })
            .collect();
        rows.reverse(); // oldest-first for rendering
        let added = rows.len();
        rows.append(&mut self.loaded);
        self.loaded = rows;
        if self.loaded.len() >= self.total {
            self.exhausted = true;
        }
        Ok(added)
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
    /// Affected paths for `apply_patch` (parsed, never invented).
    pub files: Vec<String>,
    /// True when more files exist than listed.
    pub files_truncated: bool,
}

/// Load tool cards for a session (bounded by storage).
pub fn tool_cards(db: &Db, session: &str) -> Result<Vec<ToolCard>, StorageError> {
    let mut cards = Vec::new();
    for row in db.list_tool_ops(session)? {
        let files = patch_files(&row.name, row.input.as_deref());
        let files_truncated = files.len() > CARD_FILES;
        cards.push(ToolCard {
            op: row.op,
            name: row.name,
            state: row.state,
            input_preview: preview(row.input.as_deref()),
            output_preview: preview(row.output.as_deref()),
            files: files.into_iter().take(CARD_FILES).collect(),
            files_truncated,
        });
    }
    Ok(cards)
}

fn patch_files(tool: &str, input: Option<&str>) -> Vec<String> {
    if tool != "apply_patch" {
        return Vec::new();
    }
    let Some(input) = input else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(input) else {
        return Vec::new();
    };
    let Some(patch) = value.get("patchText").and_then(|value| value.as_str()) else {
        return Vec::new();
    };
    oc_adapters::patch::affected_paths(patch).unwrap_or_default()
}

fn preview(value: Option<&str>) -> String {
    let text = value.unwrap_or("");
    if text.len() <= CARD_PREVIEW {
        return text.to_string();
    }
    format!("{}…[+{}]", &text[..CARD_PREVIEW], text.len() - CARD_PREVIEW)
}

#[cfg(test)]
mod tests {
    use super::{CARD_PREVIEW, HistoryPager, tool_cards};
    use oc_adapters::storage::Db;

    fn test_db(name: &str) -> Db {
        let root =
            std::env::temp_dir().join(format!("oc-tui-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        Db::open(&root).expect("db")
    }

    fn seed(session: &str, db: &Db, n: i64) {
        db.create_session(session).expect("session");
        for i in 0..n {
            let role = if i % 2 == 0 { "user" } else { "assistant" };
            db.append_message(session, role, &format!("m{i}"))
                .expect("msg");
        }
    }

    #[test]
    fn pages_newest_first_without_whole_history() {
        let db = test_db("pages");
        seed("s", &db, 7);
        let mut pager = HistoryPager::open(&db, "s").expect("open");
        assert_eq!(pager.total, 7);
        assert_eq!(pager.load_older(&db, 3).expect("page1"), 3);
        assert_eq!(pager.rows().last().expect("last").text, "m6");
        assert!(!pager.exhausted());
        assert_eq!(pager.load_older(&db, 3).expect("page2"), 3);
        assert_eq!(pager.load_older(&db, 3).expect("page3"), 1);
        assert_eq!(pager.rows().first().expect("first").text, "m0");
        assert!(pager.exhausted());
        assert_eq!(pager.load_older(&db, 3).expect("empty"), 0);
    }

    #[test]
    fn unknown_session_fails() {
        let db = test_db("unknown");
        assert!(HistoryPager::open(&db, "nope").is_err());
    }

    #[test]
    fn patch_cards_use_only_patch_text_and_include_move_target() {
        let patch = "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** Update File: old.txt\n*** Move to: new.txt\n@@\n-old\n+new\n*** End Patch\n";
        assert_eq!(
            super::patch_files(
                "apply_patch",
                Some(&serde_json::json!({"patchText": patch}).to_string())
            ),
            vec!["added.txt", "new.txt", "old.txt"]
        );
        for alias in ["patch", "text"] {
            assert!(
                super::patch_files(
                    "apply_patch",
                    Some(&serde_json::json!({(alias): patch}).to_string())
                )
                .is_empty()
            );
        }
    }

    #[test]
    fn cards_pair_intent_outcome_and_patch_files() {
        let db = test_db("cards");
        db.create_session("s").expect("session");
        db.record_tool_intent(
            "op1",
            "s",
            Some("t1"),
            "apply_patch",
            &serde_json::json!({"patchText": "*** Begin Patch\n*** Update File: a.txt\n@@\n-x\n+y\n*** End Patch"}).to_string(),
        )
        .expect("intent");
        db.record_tool_outcome("op1", "completed", Some("ok"))
            .expect("outcome");
        db.record_tool_intent("op2", "s", Some("t1"), "read", &"x".repeat(2000))
            .expect("intent2");
        let cards = tool_cards(&db, "s").expect("cards");
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].state, "completed");
        assert_eq!(cards[0].output_preview, "ok");
        assert!(
            cards[0].files.contains(&"a.txt".to_string()),
            "{:?}",
            cards[0].files
        );
        assert!(cards[1].input_preview.len() <= CARD_PREVIEW + 16);
        assert!(cards[1].files.is_empty());
    }
}
