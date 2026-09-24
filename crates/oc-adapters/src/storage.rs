//! SQLite storage worker for T04.
//!
//! Own data-root lock, WAL + `synchronous=FULL`, short transactions, bounded
//! content-addressed blobs and crash recovery. No upstream DB is ever opened
//! for writing; no migration of foreign databases happens here.
//!
//! Layout under `<root>/`:
//! `oc.lock` (advisory exclusive flock, never deleted), `oc.sqlite`,
//! `oc.sqlite-wal/shm` (SQLite), `blobs/<sha256>` (content-addressed).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use fs2::FileExt as _;
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// Product blob quota reference (2 GiB, see `examples/oc-rs.toml`).
pub const DEFAULT_BLOB_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Schema version applied by T04.
pub const SCHEMA_VERSION: i64 = 1;
/// Additive child-session schema version (T43): nullable `sessions` columns
/// plus a `parent_id` index. Version 2 is the DCP migration.
pub const CHILD_SESSION_SCHEMA_VERSION: i64 = 3;
/// Preference namespace for root session Location ownership.
pub(crate) const SESSION_LOCATION_PREFIX: &str = "tui.session_location.";
const TAB_ADOPTION_PREFIX: &str = "tui.selection.tab_adoption:";
const MAX_TAB_ADOPTIONS: usize = 16;
const MAX_TAB_ADOPTION_KEY_BYTES: usize = 8192;
const TAB_ADOPTION_VALUE: &str = "pending";
pub(crate) const MAX_TABS: usize = 16;
pub(crate) const MAX_TAB_DECK_BYTES: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredDeck {
    pub(crate) version: u8,
    pub(crate) sessions: Vec<String>,
    pub(crate) active: Option<String>,
}

pub(crate) fn tab_deck_key(location: &str) -> String {
    format!(
        "tui.selection.tab_deck:{}",
        serde_json::to_string(&[location]).expect("strings")
    )
}

pub(crate) fn parse_stored_deck(raw: Option<&str>) -> Result<StoredDeck, StorageError> {
    let deck = match raw {
        Some(raw) => serde_json::from_str(raw).map_err(|_| invalid_stored_tab_deck())?,
        None => StoredDeck {
            version: 1,
            sessions: Vec::new(),
            active: None,
        },
    };
    if deck.version != 1 || deck.sessions.len() > MAX_TABS {
        return Err(invalid_stored_tab_deck());
    }
    Ok(deck)
}

pub(crate) fn valid_tab_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id == id.trim() && !id.chars().any(char::is_control)
}

fn tab_adoption_key(location: &str, id: &str) -> String {
    format!(
        "{TAB_ADOPTION_PREFIX}{}",
        serde_json::to_string(&[location, id]).expect("strings")
    )
}

fn tab_adoption_scope(location: &str) -> String {
    format!(
        "{TAB_ADOPTION_PREFIX}[{},",
        serde_json::to_string(location).expect("string")
    )
}

fn invalid_tab_adoption() -> StorageError {
    StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid pending tab adoption",
    ))
}

fn invalid_stored_tab_deck() -> StorageError {
    StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "stored tab deck invalid or full",
    ))
}

/// Nullable `sessions` columns added by [`CHILD_SESSION_SCHEMA_VERSION`].
const CHILD_SESSION_COLUMNS: [(&str, &str); 4] = [
    ("parent_id", "TEXT"),
    ("agent", "TEXT"),
    ("model", "TEXT"),
    ("title", "TEXT"),
];

/// Typed storage errors (redacted; no paths with secrets, no raw payloads).
#[derive(Debug, Error)]
pub enum StorageError {
    /// Second owner holds the data root.
    #[error("data root busy")]
    DataRootBusy,
    /// Unsafe or unusable data root.
    #[error("unsafe data root: {0}")]
    UnsafeRoot(String),
    /// Quota or disk resource exhausted.
    #[error("storage full")]
    StorageFull,
    /// Blob digest is unknown.
    #[error("blob not found")]
    BlobNotFound,
    /// Session id is unknown.
    #[error("session not found")]
    SessionNotFound,
    /// Tool operation id is unknown.
    #[error("tool operation not found")]
    OperationNotFound,
    /// The exact session primary key already exists.
    #[error("session already exists")]
    SessionAlreadyExists,
    /// Compression state changed or conflicts with the candidate plan.
    #[error("compression state conflict")]
    CompressionConflict,
    /// Underlying SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Underlying I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Bounded active-projection read ([`Db::active_history`]).
pub struct ActiveHistory {
    /// Rows in seq order (`id`, `role`, `text`).
    pub rows: Vec<(String, String, String)>,
    /// Retained active text bytes.
    pub bytes: u64,
    /// Rows the active window holds (including rows released on overflow).
    pub rows_read: usize,
    /// True when the active text exceeded the caller's byte budget.
    pub overflow: bool,
}

/// Seq encoded in a message id (`m0017`), if it has the expected shape.
fn anchor_seq(result: &str) -> Option<i64> {
    let value: serde_json::Value = serde_json::from_str(result).ok()?;
    let id = value.get("user_message")?.as_str()?;
    id.strip_prefix('m')?.parse::<i64>().ok()
}

/// Byte length of the longest prefix of `bytes` that ends on a char boundary.
fn complete_bytes(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
    }
}

/// Bound one stored tool-operation output to a UI preview.
///
/// `raw` is a prefix supplied by SQL (`substr`), `bytes` the full stored
/// size: a truncated preview carries the exact dropped byte count so the
/// caller can continue through [`Db::read_tool_op_output`].
fn bound_preview(raw: Option<String>, bytes: i64) -> (Option<String>, bool) {
    let Some(text) = raw else {
        return (None, false);
    };
    let total = bytes.max(0) as usize;
    if total <= TOOL_OP_PREVIEW_BYTES && total <= text.len() {
        // The SQL prefix already holds every stored byte.
        return (Some(text), false);
    }
    let kept = text.floor_char_boundary(TOOL_OP_PREVIEW_BYTES.min(text.len()));
    (
        Some(format!(
            "{}…[+{}]",
            &text[..kept],
            total.saturating_sub(kept)
        )),
        true,
    )
}

#[cfg(test)]
#[test]
fn v06b_multibyte_output_preview_is_byte_bounded() {
    let text = "é".repeat(1500);
    let (preview, truncated) = bound_preview(Some(text.clone()), text.len() as i64);
    assert!(truncated);
    let preview = preview.unwrap();
    assert!(preview.starts_with(&"é".repeat(TOOL_OP_PREVIEW_BYTES / 2)));
    assert!(preview.ends_with("…[+952]"));
    assert!(preview.len() < text.len());
}

/// Owned storage handle: lock file + SQLite connection + blob dir.
///
/// The lock is held for the whole lifetime; dropping closes SQLite before
/// explicitly releasing the flock. The lockfile inode is never deleted by PID.
pub struct Db {
    root: PathBuf,
    blob_dir: PathBuf,
    quota_bytes: u64,
    conn: Mutex<Connection>,
    // Fields drop in declaration order: release ownership after SQLite closes.
    _lock: RootLock,
}

/// Constructed only after successful acquisition of the exclusive flock.
struct RootLock(File);

impl Drop for RootLock {
    fn drop(&mut self) {
        // Close alone can leave the flock held by a concurrently forked child
        // until exec closes its inherited descriptor. Release it explicitly.
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

/// Stored metadata of one session row (`sessions` table, child columns).
///
/// A root session has all-`None` metadata; a child session records its
/// `parent_id` plus the agent/model/title it was created with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMeta {
    /// Parent session id; `None` for a root session.
    pub parent_id: Option<String>,
    /// Agent id the session runs as.
    pub agent: Option<String>,
    /// Model id the session was created with (`provider/model[#variant]`).
    pub model: Option<String>,
    /// Human title (the subagent call's short description).
    pub title: Option<String>,
}

/// Outcome of a Location-scoped root creation under the storage transaction.
pub(crate) enum BoundSessionCreation {
    Created,
    AlreadyBound,
    BoundElsewhere(String),
}

/// A preference read with a SQL-enforced byte limit; an absent row is not
/// interchangeable with a present row whose value exceeds the limit.
pub(crate) enum BoundedPref {
    Missing,
    TooLarge,
    Value(String),
}

/// One tool operation row for TUI tool cards (T22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOpRow {
    /// Operation id.
    pub op: String,
    /// Owning turn, if any.
    pub turn: Option<String>,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `unknown`.
    pub state: String,
    /// Bounded input snapshot.
    pub input: Option<String>,
    /// Bounded output preview (never the whole result; see
    /// [`read_tool_op_output`] for the continuation).
    pub output: Option<String>,
    /// Full stored output size in bytes.
    pub output_bytes: i64,
    /// Whether [`ToolOpRow::output`] is a truncated preview.
    pub output_truncated: bool,
    /// Insertion order cursor (newest-first paging).
    pub rowid: i64,
}

/// Max rows per history page (UI03 bounds the backing store).
pub const HISTORY_PAGE_MAX: usize = 100;
/// Max tool operations listed per session (UI03 cards).
pub const TOOL_OPS_MAX: usize = 200;
/// Output preview bytes kept in a UI-facing tool-operation row.
///
/// The durable result is never rewritten: previews carry an explicit
/// `…[+N]` marker and the continuation is available through
/// [`Db::read_tool_op_output`].
pub const TOOL_OP_PREVIEW_BYTES: usize = 2_048;
/// Active-history page size for bounded projection reads.
pub const ACTIVE_HISTORY_PAGE: usize = 256;

/// Durable compression block row with ordered membership (T17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionBlockRow {
    /// Block id (`b0001`, … per session).
    pub id: String,
    /// Owning session.
    pub session: String,
    /// Range topic.
    pub topic: String,
    /// Model-authored summary.
    pub summary: String,
    /// Covered start message id.
    pub start_msg: String,
    /// Covered end message id.
    pub end_msg: String,
    /// Covered message ids in order (references only, never text).
    pub members: Vec<String>,
}

/// Stable occurrence of a provider call ID in session wire order.
pub(crate) type DcpCallKey = (String, u64);

/// Durable DCP tool projection decisions. A provider may reuse call IDs in
/// different turns, so identity includes the occurrence in immutable wire order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DcpToolProjection {
    pub hidden: std::collections::BTreeSet<DcpCallKey>,
    pub purged: std::collections::BTreeSet<DcpCallKey>,
}

/// Existing durable tool outcome to commit with compression blocks.
pub(crate) struct ToolOutcomeLogCommit<'a> {
    pub(crate) operation_id: &'a str,
    pub(crate) operation_state: &'a str,
    pub(crate) operation_output: &'a str,
    pub(crate) turn_id: Option<&'a str>,
    pub(crate) turn_log: Option<&'a str>,
    pub(crate) preference_updates: &'a [(String, String)],
}

/// One prevalidated DCP transaction request.
pub(crate) struct CompressionPlanCommit<'a> {
    pub(crate) session: &'a str,
    pub(crate) blocks: &'a [CompressionBlockRow],
    pub(crate) consumed_blocks: &'a [String],
    pub(crate) expected_next: u64,
    pub(crate) expected_existing: &'a [String],
    pub(crate) expected_prune: Option<&'a str>,
    pub(crate) hidden_calls: &'a [DcpCallKey],
    pub(crate) purged_calls: &'a [DcpCallKey],
    pub(crate) tool: Option<&'a ToolOutcomeLogCommit<'a>>,
}

impl Db {
    /// Open (or create) a data root with default quota.
    pub fn open(root: &Path) -> Result<Self, StorageError> {
        Self::open_with_quota(root, DEFAULT_BLOB_QUOTA_BYTES)
    }

    /// Open with explicit blob quota (tests use small quotas).
    pub fn open_with_quota(root: &Path, quota_bytes: u64) -> Result<Self, StorageError> {
        let root = validate_root(root)?;
        fs::create_dir_all(&root)?;
        // Ownership first (refuse foreign dirs); then enforce 0700 on both
        // fresh and existing roots so a normal 0755 --data-dir is secured
        // rather than refused. Symlink/owner failures stay hard errors.
        check_owner(&root)?;
        #[cfg(unix)]
        {
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        check_restrictive(&root)?;
        let blob_dir = root.join("blobs");
        fs::create_dir_all(&blob_dir)?;
        #[cfg(unix)]
        {
            fs::set_permissions(&blob_dir, fs::Permissions::from_mode(0o700))?;
        }

        // Advisory exclusive lock, non-blocking: second owner => DataRootBusy.
        let lock_path = root.join("oc.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        try_lock_exclusive(&lock)?;
        let lock = RootLock(lock);

        let db_path = root.join("oc.sqlite");
        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        apply_schema(&conn)?;
        // Same journal, indexed anchor lookup: history paging must not parse
        // every archived turn. Legacy non-JSON results are excluded safely.
        conn.execute_batch("CREATE INDEX IF NOT EXISTS turns_display_anchor ON turns(session_id, COALESCE(json_extract(result,'$.assistant_message'),json_extract(result,'$.user_message'))) WHERE json_valid(result)")?;

        Ok(Self {
            root,
            blob_dir,
            quota_bytes,
            conn: Mutex::new(conn),
            _lock: lock,
        })
    }

    /// Data-root path (owned).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a root session; duplicate ids fail.
    pub fn create_session(&self, id: &str) -> Result<(), StorageError> {
        self.create_root_session(id, None)?;
        Ok(())
    }

    /// Atomically create a root, its event, and its Location binding.
    /// Existing bindings are idempotent only for the same Location; a root
    /// without a binding remains a duplicate rather than being claimed.
    pub(crate) fn create_bound_session(
        &self,
        id: &str,
        location: &str,
    ) -> Result<BoundSessionCreation, StorageError> {
        self.create_root_session(id, Some(location))
    }

    /// Fresh-session durable acceptance: root, creation event, Location
    /// binding, optional prevalidated session selection, started turn/event
    /// and user message/event commit together.
    /// Unlike `create_bound_session`, an existing root (even one bound to this
    /// Location) is always a duplicate; no existing history is modified.
    pub(crate) fn create_bound_session_and_accept_turn(
        &self,
        id: &str,
        location: &str,
        turn: &str,
        prompt: &str,
        user_text: &str,
        initial_selection: Option<(&str, &str)>,
    ) -> Result<String, StorageError> {
        if !valid_tab_id(id) || tab_adoption_scope(location).len() > MAX_TAB_ADOPTION_KEY_BYTES {
            return Err(invalid_tab_adoption());
        }
        let marker = tab_adoption_key(location, id);
        if marker.len() > MAX_TAB_ADOPTION_KEY_BYTES {
            return Err(invalid_tab_adoption());
        }
        if let Some((key, _)) = initial_selection {
            // The application owns selection validation and encoding. Accept
            // only its session-scoped key for this exact root; never let the
            // optional UPSERT replace a Location binding or global preference.
            let belongs_to_session = key
                .strip_prefix("tui.selection.session:")
                .and_then(|parts| serde_json::from_str::<Vec<String>>(parts).ok())
                .is_some_and(|parts| parts.len() == 3 && parts[2] == id);
            if !belongs_to_session {
                return Err(StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "invalid initial session selection key",
                )));
            }
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        // Admission and the root write must see the same scoped deck and
        // marker set. Reject before any root/turn/event insert.
        let stored =
            match Self::get_pref_bounded_in(&tx, &tab_deck_key(location), MAX_TAB_DECK_BYTES)? {
                BoundedPref::Missing => parse_stored_deck(None)?,
                BoundedPref::TooLarge => return Err(invalid_stored_tab_deck()),
                BoundedPref::Value(raw) => parse_stored_deck(Some(&raw))?,
            };
        let mut occupied: HashSet<String> = stored.sessions.into_iter().collect();
        occupied.extend(Self::tab_adoptions_in(&tx, location)?);
        if occupied.len() >= MAX_TABS {
            return Err(invalid_stored_tab_deck());
        }
        Self::insert_root_session(&tx, id)?;
        Self::insert_location_binding(&tx, id, location)?;
        let message = Self::insert_accepted_turn(&tx, turn, id, prompt, user_text)?;
        if let Some((key, value)) = initial_selection {
            Self::upsert_pref(&tx, key, value)?;
        }
        // The marker is inserted last; failures roll back the entire turn.
        tx.execute(
            "INSERT INTO prefs(key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![marker, TAB_ADOPTION_VALUE, now_rfc3339()],
        )?;
        tx.commit()?;
        Ok(message)
    }

    fn create_root_session(
        &self,
        id: &str,
        location: Option<&str>,
    ) -> Result<BoundSessionCreation, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        if let Some(location) = location {
            let key = format!("{SESSION_LOCATION_PREFIX}{id}");
            let owner: Option<String> = tx
                .query_row("SELECT value FROM prefs WHERE key = ?1", [key], |row| {
                    row.get(0)
                })
                .optional()?;
            if let Some(owner) = owner {
                return Ok(if owner == location {
                    BoundSessionCreation::AlreadyBound
                } else {
                    BoundSessionCreation::BoundElsewhere(owner)
                });
            }
        }
        Self::insert_root_session(&tx, id)?;
        if let Some(location) = location {
            Self::insert_location_binding(&tx, id, location)?;
        }
        tx.commit()?;
        Ok(BoundSessionCreation::Created)
    }

    fn insert_location_binding(
        conn: &Connection,
        id: &str,
        location: &str,
    ) -> Result<(), StorageError> {
        conn.execute(
            "INSERT INTO prefs(key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![
                format!("{SESSION_LOCATION_PREFIX}{id}"),
                location,
                now_rfc3339()
            ],
        )?;
        Ok(())
    }

    fn insert_root_session(conn: &Connection, id: &str) -> Result<(), StorageError> {
        Self::insert_session_row(conn, id, None, None, None, None)?;
        conn.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'session_created', ?2)",
            params![id, "{}"],
        )?;
        Ok(())
    }

    /// Create a child session owned by `parent`; duplicate ids fail.
    ///
    /// The parent must exist ([`StorageError::SessionNotFound`]); its
    /// history is never touched. `agent`, `model` and `title` are stored
    /// verbatim and read back through [`Db::session_meta`].
    pub fn create_child_session(
        &self,
        parent: &str,
        id: &str,
        agent: Option<&str>,
        model: Option<&str>,
        title: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::require_session(&tx, parent)?;
        Self::insert_session_row(&tx, id, Some(parent), agent, model, title)?;
        tx.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'session_created', ?2)",
            params![id, "{}"],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Insert one `sessions` row; duplicate ids map to `SessionAlreadyExists`.
    fn insert_session_row(
        conn: &Connection,
        id: &str,
        parent: Option<&str>,
        agent: Option<&str>,
        model: Option<&str>,
        title: Option<&str>,
    ) -> Result<(), StorageError> {
        let now = now_rfc3339();
        let res = conn.execute(
            "INSERT INTO sessions(id, created_at, parent_id, agent, model, title)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, now, parent, agent, model, title],
        );
        match res {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
            {
                Err(StorageError::SessionAlreadyExists)
            }
            Err(other) => Err(StorageError::Sqlite(other)),
        }
    }

    /// Append a message and durable event in one transaction.
    ///
    /// Returns the new message id (`m0001`, …). Durable ack happens only
    /// after commit; callers must not ack UI before this returns.
    pub fn append_message(
        &self,
        session: &str,
        role: &str,
        text: &str,
    ) -> Result<String, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let id = Self::insert_message(&tx, session, role, text)?;
        tx.commit()?;
        Ok(id)
    }

    fn insert_message(
        conn: &Connection,
        session: &str,
        role: &str,
        text: &str,
    ) -> Result<String, StorageError> {
        Self::require_session(conn, session)?;
        let seq: i64 = conn.query_row(
            // Global sequence: message ids are unique across sessions
            // (the PRIMARY KEY is global); per-session order still holds
            // because the sequence is monotonic.
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages",
            params![],
            |row| row.get(0),
        )?;
        let id = format!("m{seq:04}");
        conn.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, session, seq, role, text],
        )?;
        conn.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'message', ?2)",
            params![session, id],
        )?;
        Ok(id)
    }

    /// Read committed history in seq order.
    pub fn read_history(&self, session: &str) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT role, text FROM messages WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![session], |row| {
            let role: String = row.get(0)?;
            let text: String = row.get(1)?;
            Ok((role, text))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty()
            && conn
                .query_row(
                    "SELECT 1 FROM sessions WHERE id = ?1",
                    params![session],
                    |_| Ok(()),
                )
                .is_err()
        {
            return Err(StorageError::SessionNotFound);
        }
        Ok(out)
    }

    /// List every session id (roots and children) sorted by id ascending.
    ///
    /// Ordering and query are unchanged for legacy callers; child rows are
    /// included because no `parent_id` filter is applied. Hierarchy-aware
    /// callers use [`Db::session_meta`] / [`Db::children_of`].
    pub fn list_sessions(&self) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached("SELECT id FROM sessions ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Read one session's stored metadata; unknown ids are `SessionNotFound`.
    pub fn session_meta(&self, session: &str) -> Result<SessionMeta, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "SELECT parent_id, agent, model, title FROM sessions WHERE id = ?1",
            params![session],
            |row| {
                Ok(SessionMeta {
                    parent_id: row.get(0)?,
                    agent: row.get(1)?,
                    model: row.get(2)?,
                    title: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or(StorageError::SessionNotFound)
    }

    /// Direct child session ids of `parent` in insertion order.
    ///
    /// Only direct children are returned. Order is the `sessions` row
    /// insertion order (`rowid` ascending); this store never runs `VACUUM`,
    /// so the order is stable for the lifetime of the database. The parent
    /// must exist ([`StorageError::SessionNotFound`]).
    pub fn children_of(&self, parent: &str) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, parent)?;
        let mut stmt =
            conn.prepare_cached("SELECT id FROM sessions WHERE parent_id = ?1 ORDER BY rowid ASC")?;
        let rows = stmt.query_map(params![parent], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Count committed messages (UI03 pager total).
    pub fn history_len(&self, session: &str) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let count: Option<i64> = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE session_id = ?1",
                params![session],
                |row| row.get(0),
            )
            .optional()?;
        let count = count.unwrap_or(0).max(0) as usize;
        if count == 0 {
            Self::require_session(&conn, session)?;
        }
        Ok(count)
    }

    /// Read one newest-first page: `(seq, role, text)` with `seq` below
    /// `before_seq` when given. Never renders the whole history.
    pub fn read_history_page(
        &self,
        session: &str,
        limit: usize,
        before_seq: Option<i64>,
    ) -> Result<Vec<(i64, String, String)>, StorageError> {
        let limit = (limit.min(HISTORY_PAGE_MAX) as i64).max(0);
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT seq, role, text FROM messages
             WHERE session_id = ?1 AND (?2 IS NULL OR seq < ?2)
             ORDER BY seq DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![session, before_seq, limit], |row| {
            let seq: i64 = row.get(0)?;
            let role: String = row.get(1)?;
            let text: String = row.get(2)?;
            Ok((seq, role, text))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty() {
            Self::require_session(&conn, session)?;
        }
        Ok(out)
    }

    /// Read one oldest-first page of rows newer than `after_seq`.
    pub fn read_history_after(
        &self,
        session: &str,
        limit: usize,
        after_seq: i64,
    ) -> Result<Vec<(i64, String, String)>, StorageError> {
        let limit = (limit.min(HISTORY_PAGE_MAX) as i64).max(0);
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT seq, role, text FROM messages
             WHERE session_id = ?1 AND seq > ?2
             ORDER BY seq ASC LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![session, after_seq, limit], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty() {
            Self::require_session(&conn, session)?;
        }
        Ok(out)
    }

    /// Committed message seq bounds `(min, max)`; `None` for an empty session.
    pub fn history_bounds(
        &self,
        session: &str,
    ) -> Result<(Option<i64>, Option<i64>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let bounds: Option<(Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT MIN(seq), MAX(seq) FROM messages WHERE session_id = ?1",
                params![session],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (min, max) = bounds.unwrap_or((None, None));
        if min.is_none() {
            Self::require_session(&conn, session)?;
        }
        Ok((min, max))
    }

    /// Read one newest-first tool-operation page below `before_rowid`.
    pub fn list_tool_ops_page(
        &self,
        session: &str,
        limit: usize,
        before_rowid: Option<i64>,
    ) -> Result<Vec<ToolOpRow>, StorageError> {
        let limit = (limit.min(TOOL_OPS_MAX) as i64).max(0);
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT id, turn_id, name, state, input,
                    CASE WHEN length(CAST(output AS BLOB)) > ?4
                         THEN substr(output, 1, ?4) ELSE output END,
                    length(CAST(output AS BLOB)), rowid
               FROM tool_operations
              WHERE session_id = ?1 AND (?2 IS NULL OR rowid < ?2)
              ORDER BY rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![session, before_rowid, limit, TOOL_OP_PREVIEW_BYTES as i64],
            |row| {
                let raw: Option<String> = row.get(5)?;
                let bytes: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
                let (output, truncated) = bound_preview(raw, bytes);
                Ok(ToolOpRow {
                    op: row.get(0)?,
                    turn: row.get(1)?,
                    name: row.get(2)?,
                    state: row.get(3)?,
                    input: row.get(4)?,
                    output,
                    output_bytes: bytes,
                    output_truncated: truncated,
                    rowid: row.get(7)?,
                })
            },
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty() {
            Self::require_session(&conn, session)?;
        }
        Ok(out)
    }

    /// Read a byte window of one tool operation's durable output.
    ///
    /// `offset` and `limit` specify a byte window in the stored text.
    /// Requests starting inside a UTF-8 char or beyond the end are rejected;
    /// a trailing partial char is withheld, so the next call starts on a
    /// boundary. Returns
    /// `(text, total_bytes, next_offset)`; `next_offset` is `None` once the
    /// end of the result is reached.
    pub fn read_tool_op_output(
        &self,
        op: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(String, i64, Option<i64>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::read_tool_op_output_on(&conn, op, offset, limit)
    }

    fn read_tool_op_output_on(
        conn: &Connection,
        op: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(String, i64, Option<i64>), StorageError> {
        // SQLite BLOB substr is one-indexed; never cast/truncate or add past
        // the signed index range. The owner API requests at least four bytes.
        let index = i64::try_from(offset)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(StorageError::OperationNotFound)?;
        let limit = i64::try_from(limit).map_err(|_| StorageError::OperationNotFound)?;
        let (window, total): (Option<Vec<u8>>, i64) = conn
            .query_row(
                "SELECT substr(CAST(output AS BLOB), ?2, ?3),
                        length(CAST(output AS BLOB))
                   FROM tool_operations WHERE id = ?1",
                params![op, index, limit],
                |row| {
                    Ok((
                        row.get::<_, Option<Vec<u8>>>(0)?,
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    ))
                },
            )
            .optional()?
            .ok_or(StorageError::OperationNotFound)?;
        let bytes = window.unwrap_or_default();
        if total < 0
            || offset > total as usize
            || bytes
                .first()
                .is_some_and(|byte| byte & 0b1100_0000 == 0b1000_0000)
        {
            return Err(StorageError::OperationNotFound);
        }
        // Withhold an incomplete trailing char so no byte is ever skipped.
        let kept = complete_bytes(&bytes);
        let text = String::from_utf8_lossy(&bytes[..kept]).to_string();
        let next = offset + kept;
        if next == offset && next < total as usize {
            return Err(StorageError::OperationNotFound);
        }
        let next_offset = (next < total as usize).then_some(next as i64);
        Ok((text, total, next_offset))
    }

    /// Owner-facing continuation: reject an operation from any other session
    /// before reading its result. One page is limited like the cards preview.
    pub fn read_session_tool_output(
        &self,
        session: &str,
        op: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(String, i64, Option<i64>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let belongs: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tool_operations WHERE id=?1 AND session_id=?2)",
            params![op, session],
            |row| row.get(0),
        )?;
        if !belongs {
            return Err(StorageError::OperationNotFound);
        }
        Self::read_tool_op_output_on(&conn, op, offset, limit.clamp(4, TOOL_OP_PREVIEW_BYTES))
    }

    /// Recorded tool-operation rowid bounds `(min, max)`; `None` when empty.
    pub fn tool_ops_bounds(
        &self,
        session: &str,
    ) -> Result<(Option<i64>, Option<i64>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let bounds: Option<(Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT MIN(rowid), MAX(rowid) FROM tool_operations WHERE session_id = ?1",
                params![session],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (min, max) = bounds.unwrap_or((None, None));
        if min.is_none() {
            Self::require_session(&conn, session)?;
        }
        Ok((min, max))
    }

    /// Total recorded tool operations for a session.
    pub fn tool_ops_len(&self, session: &str) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let count: Option<i64> = conn
            .query_row(
                "SELECT COUNT(*) FROM tool_operations WHERE session_id = ?1",
                params![session],
                |row| row.get(0),
            )
            .optional()?;
        let count = count.unwrap_or(0).max(0) as usize;
        if count == 0 {
            Self::require_session(&conn, session)?;
        }
        Ok(count)
    }

    fn require_session(conn: &Connection, session: &str) -> Result<(), StorageError> {
        conn.query_row(
            "SELECT 1 FROM sessions WHERE id = ?1",
            params![session],
            |_| Ok(()),
        )
        .optional()?
        .ok_or(StorageError::SessionNotFound)
    }

    /// List tool operations in row order, bounded (UI03 cards).
    pub fn list_tool_ops(&self, session: &str) -> Result<Vec<ToolOpRow>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT id, turn_id, name, state, input,
                    CASE WHEN length(CAST(output AS BLOB)) > ?3
                         THEN substr(output, 1, ?3) ELSE output END,
                    length(CAST(output AS BLOB)), rowid
               FROM tool_operations
              WHERE session_id = ?1 ORDER BY rowid ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![session, TOOL_OPS_MAX as i64, TOOL_OP_PREVIEW_BYTES as i64],
            |row| {
                let raw: Option<String> = row.get(5)?;
                let bytes: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
                let (output, truncated) = bound_preview(raw, bytes);
                Ok(ToolOpRow {
                    op: row.get(0)?,
                    turn: row.get(1)?,
                    name: row.get(2)?,
                    state: row.get(3)?,
                    input: row.get(4)?,
                    output,
                    output_bytes: bytes,
                    output_truncated: truncated,
                    rowid: row.get(7)?,
                })
            },
        )?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty() {
            Self::require_session(&conn, session)?;
        }
        Ok(out)
    }

    /// Prune mark id plus the seq of the marked message.
    ///
    /// A mark whose message row is gone is reported as absent: the caller
    /// then projects from the full history (more context, never less).
    pub fn prune_bound(&self, session: &str) -> Result<Option<(String, i64)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "SELECT p.up_to_msg, m.seq FROM prune_marks p
               JOIN messages m ON m.id = p.up_to_msg
              WHERE p.session_id = ?1",
            params![session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(StorageError::Sqlite)
    }

    /// Lowest in-window seq of each compression block's members, ascending.
    ///
    /// Used to place block summaries inside a bounded active projection
    /// without materialising the covered rows themselves.
    pub fn block_positions(
        &self,
        session: &str,
        after_seq: i64,
    ) -> Result<Vec<(String, i64)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT cm.block_id, MIN(m.seq) FROM compression_members cm
               JOIN messages m ON m.id = cm.message_id
               JOIN compression_blocks b ON b.id = cm.block_id
              WHERE b.session_id = ?1 AND m.seq > ?2
              GROUP BY cm.block_id ORDER BY MIN(m.seq) ASC",
        )?;
        let rows = stmt.query_map(params![session, after_seq], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Read the active projection: rows newer than `after_seq` that no
    /// compression block covers, in seq order.
    ///
    /// Retained memory is bounded by `budget` plus one page; when the active
    /// text exceeds `budget` the rows are released and the exact totals are
    /// reported with `overflow = true`, so the caller can refuse the turn
    /// with a diagnostic instead of silently dropping facts.
    pub fn active_history(
        &self,
        session: &str,
        after_seq: i64,
        budget: usize,
    ) -> Result<ActiveHistory, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut rows: Vec<(String, String, String)> = Vec::new();
        let mut bytes: u64 = 0;
        let mut cursor = after_seq;
        loop {
            let mut stmt = conn.prepare_cached(
                "SELECT id, role, seq, length(CAST(text AS BLOB)), text FROM messages
                  WHERE session_id = ?1 AND seq > ?2
                    AND NOT EXISTS (SELECT 1 FROM compression_members cm
                                     WHERE cm.message_id = messages.id)
                  ORDER BY seq ASC LIMIT ?3",
            )?;
            let page: Vec<(String, String, i64, i64, String)> = stmt
                .query_map(
                    params![session, cursor, ACTIVE_HISTORY_PAGE as i64],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                            row.get(4)?,
                        ))
                    },
                )?
                .collect::<Result<Vec<_>, _>>()?;
            if page.is_empty() {
                if rows.is_empty() && cursor == after_seq {
                    Self::require_session(&conn, session)?;
                }
                break;
            }
            let full_page = page.len() == ACTIVE_HISTORY_PAGE;
            for (id, role, seq, size, text) in page {
                cursor = seq;
                bytes = bytes.saturating_add(size.max(0) as u64);
                rows.push((id, role, text));
            }
            if bytes > budget as u64 {
                // Exact remaining totals without materialising any text.
                let (extra_rows, extra_bytes): (i64, i64) = conn.query_row(
                    "SELECT COUNT(*), COALESCE(SUM(length(CAST(text AS BLOB))), 0)
                       FROM messages
                      WHERE session_id = ?1 AND seq > ?2
                        AND NOT EXISTS (SELECT 1 FROM compression_members cm
                                         WHERE cm.message_id = messages.id)",
                    params![session, cursor],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                return Ok(ActiveHistory {
                    rows_read: rows.len() + extra_rows.max(0) as usize,
                    bytes: bytes.saturating_add(extra_bytes.max(0) as u64),
                    rows: Vec::new(),
                    overflow: true,
                });
            }
            if !full_page {
                break;
            }
        }
        Ok(ActiveHistory {
            rows_read: rows.len(),
            bytes,
            rows,
            overflow: false,
        })
    }

    /// Turn logs whose anchor belongs to the active window.
    ///
    /// Scans newest-first in bounded pages and stops once the anchor seq
    /// falls to or below `floor_seq`: turn rows are inserted in anchor order,
    /// so nothing newer can appear below that point. Logs without a parseable
    /// anchor are kept (never drop history on an unknown shape).
    pub fn wire_logs_for_window(
        &self,
        session: &str,
        floor_seq: i64,
        page: usize,
    ) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let page = (page.min(TOOL_OPS_MAX) as i64).max(1);
        let mut out = Vec::new();
        let mut upper: Option<i64> = None;
        loop {
            let mut stmt = conn.prepare_cached(
                "SELECT rowid, result FROM turns
                  WHERE session_id = ?1 AND result IS NOT NULL
                    AND (?2 IS NULL OR rowid < ?2)
                  ORDER BY rowid DESC LIMIT ?3",
            )?;
            let batch: Vec<(i64, String)> = stmt
                .query_map(params![session, upper, page], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let full_page = batch.len() == page as usize;
            if let Some((rowid, _)) = batch.last() {
                upper = Some(*rowid);
            }
            let mut oldest = i64::MAX;
            for (_, result) in batch {
                match anchor_seq(&result) {
                    Some(seq) if seq > floor_seq => out.push(result),
                    Some(seq) => oldest = oldest.min(seq),
                    None => out.push(result),
                }
            }
            if oldest <= floor_seq || !full_page {
                break;
            }
        }
        // Restore insertion order: replay must see turns oldest-first.
        out.reverse();
        Ok(out)
    }

    /// Read committed history with stable ids in seq order (DCP ranges).
    pub fn read_history_full(
        &self,
        session: &str,
    ) -> Result<Vec<(String, String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT id, role, text FROM messages WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![session], |row| {
            let id: String = row.get(0)?;
            let role: String = row.get(1)?;
            let text: String = row.get(2)?;
            Ok((id, role, text))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        if out.is_empty() {
            Self::require_session(&conn, session)?;
        }
        Ok(out)
    }

    /// Delete a compression block with its membership (compensation only).
    pub fn delete_compression_block(&self, block: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.prepare_cached("DELETE FROM compression_members WHERE block_id = ?1")?
            .execute(params![block])?;
        conn.prepare_cached("DELETE FROM compression_blocks WHERE id = ?1")?
            .execute(params![block])?;
        Ok(())
    }

    /// Upsert a namespaced UI preference (callers use `tui.*` keys).
    pub fn set_pref(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.set_prefs(&[(key.to_string(), value.to_string())])
    }

    /// Bounded, strictly decoded pending roots for one Location, in acceptance
    /// order. A key encodes both scope and root; the value is only a tag.
    pub(crate) fn tab_adoptions(&self, location: &str) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::tab_adoptions_in(&conn, location)
    }

    fn tab_adoptions_in(conn: &Connection, location: &str) -> Result<Vec<String>, StorageError> {
        let scope = tab_adoption_scope(location);
        if scope.len() > MAX_TAB_ADOPTION_KEY_BYTES {
            return Err(invalid_tab_adoption());
        }
        let mut stmt = conn.prepare(
            "SELECT CASE WHEN length(CAST(key AS BLOB)) <= ?2 THEN key END,
                    CASE WHEN length(CAST(value AS BLOB)) <= ?3 THEN value END
               FROM prefs WHERE substr(key, 1, length(?1)) = ?1
              ORDER BY rowid LIMIT 17",
        )?;
        let rows = stmt.query_map(
            params![
                scope,
                MAX_TAB_ADOPTION_KEY_BYTES as i64,
                TAB_ADOPTION_VALUE.len() as i64
            ],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )?;
        let mut ids = Vec::new();
        for row in rows {
            let (key, value) = row?;
            let key = key.ok_or_else(invalid_tab_adoption)?;
            let suffix = key
                .strip_prefix(TAB_ADOPTION_PREFIX)
                .ok_or_else(invalid_tab_adoption)?;
            let parts: Vec<String> =
                serde_json::from_str(suffix).map_err(|_| invalid_tab_adoption())?;
            if parts.len() != 2
                || parts[0] != location
                || !valid_tab_id(&parts[1])
                || tab_adoption_key(location, &parts[1]) != key
                || value.as_deref() != Some(TAB_ADOPTION_VALUE)
                || ids.contains(&parts[1])
            {
                return Err(invalid_tab_adoption());
            }
            ids.push(parts[1].clone());
            if ids.len() > MAX_TAB_ADOPTIONS {
                return Err(invalid_tab_adoption());
            }
        }
        Ok(ids)
    }

    /// CAS preference and retire only included adoption markers in one write
    /// transaction. A late acceptance absent from the candidate refuses CAS.
    pub(crate) fn compare_set_tab_deck(
        &self,
        key: &str,
        expected: Option<&str>,
        value: &str,
        location: &str,
        included: &[String],
    ) -> Result<bool, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let current: Option<String> = tx
            .query_row("SELECT value FROM prefs WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        if current.as_deref().map(pref_revision) != expected.map(str::to_owned) {
            return Ok(false);
        }
        let pending = Self::tab_adoptions_in(&tx, location)?;
        if pending.iter().any(|id| !included.contains(id)) {
            return Ok(false);
        }
        Self::upsert_pref(&tx, key, value)?;
        for id in pending {
            tx.execute(
                "DELETE FROM prefs WHERE key = ?1 AND value = ?2",
                params![tab_adoption_key(location, &id), TAB_ADOPTION_VALUE],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    /// Atomically persist a selection and its associated model preference.
    pub fn set_prefs(&self, values: &[(String, String)]) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        for (key, value) in values {
            Self::upsert_pref(&tx, key, value)?;
        }
        tx.commit()?;
        Ok(())
    }

    fn upsert_pref(conn: &Connection, key: &str, value: &str) -> Result<(), StorageError> {
        conn.prepare_cached(
            "INSERT INTO prefs(key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )?
        .execute(params![key, value, now_rfc3339()])?;
        Ok(())
    }

    /// Read a UI preference, if set.
    pub fn get_pref(&self, key: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM prefs WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Materialize a preference only when its stored UTF-8 byte length fits.
    pub(crate) fn get_pref_bounded(
        &self,
        key: &str,
        max_bytes: usize,
    ) -> Result<BoundedPref, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::get_pref_bounded_in(&conn, key, max_bytes)
    }

    fn get_pref_bounded_in(
        conn: &Connection,
        key: &str,
        max_bytes: usize,
    ) -> Result<BoundedPref, StorageError> {
        let value: Option<Option<String>> = conn
            .query_row(
                "SELECT CASE WHEN length(CAST(value AS BLOB)) <= ?2 THEN value END
                   FROM prefs WHERE key = ?1",
                params![key, i64::try_from(max_bytes).unwrap_or(i64::MAX)],
                |row| row.get(0),
            )
            .optional()?;
        Ok(match value {
            None => BoundedPref::Missing,
            Some(None) => BoundedPref::TooLarge,
            Some(Some(raw)) => BoundedPref::Value(raw),
        })
    }

    /// Begin a turn (durable intent before any side effect).
    pub fn begin_turn(&self, turn: &str, session: &str, prompt: &str) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::insert_turn(&tx, turn, session, prompt)?;
        tx.commit()?;
        Ok(())
    }

    /// Input acceptance, state transition and both events form one durable ack.
    pub(crate) fn accept_turn(
        &self,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
    ) -> Result<String, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let message = Self::insert_accepted_turn(&tx, turn, session, prompt, user_text)?;
        tx.commit()?;
        Ok(message)
    }

    fn insert_accepted_turn(
        conn: &Connection,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
    ) -> Result<String, StorageError> {
        Self::insert_turn(conn, turn, session, prompt)?;
        Self::insert_message(conn, session, "user", user_text)
    }

    fn insert_turn(
        conn: &Connection,
        turn: &str,
        session: &str,
        prompt: &str,
    ) -> Result<(), StorageError> {
        conn.prepare_cached(
            "INSERT INTO turns(id, session_id, status, prompt, result) VALUES (?1, ?2, 'started', ?3, NULL)",
        )?
        .execute(params![turn, session, prompt])?;
        conn.prepare_cached(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'turn_started', ?2)",
        )?
        .execute(params![session, turn])?;
        Ok(())
    }

    /// Finish a turn with terminal status and optional result.
    pub fn finish_turn(
        &self,
        turn: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<(), StorageError> {
        self.commit_turn(turn, status, result, None)
    }

    /// Commit terminal state, event and optional assistant message atomically.
    pub(crate) fn commit_turn(
        &self,
        turn: &str,
        status: &str,
        result: Option<&str>,
        assistant: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let session: String =
            tx.query_row("SELECT session_id FROM turns WHERE id = ?1", [turn], |r| {
                r.get(0)
            })?;
        let mut result = result.map(str::to_owned);
        if let Some(text) = assistant {
            let message = Self::insert_message(&tx, &session, "assistant", text)?;
            if let Some(raw) = &mut result {
                let mut value: serde_json::Value = serde_json::from_str(raw).map_err(|_| {
                    StorageError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid turn log",
                    ))
                })?;
                value["assistant_message"] = message.into();
                *raw = value.to_string();
            }
        }
        let n = tx
            .prepare_cached("UPDATE turns SET status = ?1, result = ?2 WHERE id = ?3")?
            .execute(params![status, result, turn])?;
        if n == 0 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'turn_finished', ?2)",
            params![session, turn],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Persist a completed generation before dispatch, without finishing its turn.
    pub(crate) fn checkpoint_turn(&self, turn: &str, result: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute(
            "UPDATE turns SET result = ?1 WHERE id = ?2 AND status = 'started'",
            params![result, turn],
        )?;
        Ok(())
    }

    /// Outcome and its replayable wire item are a single durable boundary.
    pub(crate) fn tool_outcome_with_log(
        &self,
        op: &str,
        state: &str,
        output: &str,
        turn: &str,
        log: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE tool_operations SET state = ?1, output = ?2 WHERE id = ?3",
            params![state, output, op],
        )?;
        tx.execute(
            "UPDATE turns SET result = ?1 WHERE id = ?2",
            params![log, turn],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Per-turn wire journals; never exposed by history/UI readers.
    /// Seq of each requested message id (order-preserving input list).
    pub fn message_seqs(
        &self,
        session: &str,
        ids: &[String],
    ) -> Result<Vec<(String, i64)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut out = Vec::with_capacity(ids.len());
        let mut stmt =
            conn.prepare_cached("SELECT seq FROM messages WHERE session_id = ?1 AND id = ?2")?;
        for id in ids {
            let seq: Option<i64> = stmt
                .query_row(params![session, id], |row| row.get(0))
                .optional()?;
            if let Some(seq) = seq {
                out.push((id.clone(), seq));
            }
        }
        Ok(out)
    }

    /// Read a turn's terminal status and result JSON (turn-log replay).
    /// Add safe measured metadata without rewriting the wire journal or history.
    pub(crate) fn update_turn_display(
        &self,
        turn: &str,
        metadata: &serde_json::Value,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute("UPDATE turns SET result = json_set(result, '$.display', json_patch(COALESCE(json_extract(result, '$.display'), '{}'), json(?2))) WHERE id = ?1", params![turn, metadata.to_string()])?;
        Ok(())
    }

    /// Set a manual root title even when one was already generated. The event
    /// and title commit together; no child or missing row can be renamed.
    pub(crate) fn rename_root_session(
        &self,
        session: &str,
        title: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let affected = tx.execute(
            "UPDATE sessions SET title = ?2 WHERE id = ?1 AND parent_id IS NULL",
            params![session, title],
        )?;
        if affected == 0 {
            return Err(StorageError::SessionNotFound);
        }
        tx.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'session_updated', ?2)",
            params![session, serde_json::json!({"title": title}).to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Set a generated title once; explicit/child titles always win.
    pub(crate) fn set_generated_title(
        &self,
        session: &str,
        title: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute(
            "UPDATE sessions SET title = ?2 WHERE id = ?1 AND parent_id IS NULL AND title IS NULL",
            params![session, title],
        )?;
        Ok(())
    }

    /// Public projection, selecting only whitelisted JSON fields in SQL. The
    /// opaque wire journal never crosses the application/UI boundary. A legacy
    /// row remains text-only; interrupted turns attach to their accepted user row.
    pub(crate) fn history_turn(
        &self,
        session: &str,
        seq: i64,
    ) -> Result<Option<oc_core::queries::HistoryTurn>, StorageError> {
        let id = {
            let conn = self.conn.lock().expect("db mutex");
            conn.query_row("SELECT t.id FROM turns t JOIN messages m ON m.session_id=t.session_id WHERE json_valid(t.result) AND t.session_id=?1 AND m.seq=?2 AND m.id=COALESCE(json_extract(t.result,'$.assistant_message'),json_extract(t.result,'$.user_message')) LIMIT 1", params![session,seq], |r|r.get::<_,String>(0)).optional()?
        };
        match id {
            Some(id) => self.turn_presentation(session, &id),
            None => Ok(None),
        }
    }

    /// Same bounded projection for a running checkpoint and later replay.
    pub(crate) fn turn_presentation(
        &self,
        session: &str,
        id: &str,
    ) -> Result<Option<oc_core::queries::HistoryTurn>, StorageError> {
        use oc_core::queries::{HistoryTurn, PartState, ToolOpView, TranscriptPart};
        let conn = self.conn.lock().expect("db mutex");
        let record: Option<(String, String, String, String)> = conn.query_row(
            "SELECT t.id, t.status, COALESCE(json_extract(t.result,'$.display'),'{}'), COALESCE(json_extract(t.result,'$.model'),'')
             FROM turns t WHERE json_valid(t.result) AND t.session_id=?1 AND t.id=?2 LIMIT 1",
            params![session,id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((id, status, metadata, model)) = record else {
            return Ok(None);
        };
        let meta: serde_json::Value = serde_json::from_str(&metadata).unwrap_or_default();
        let mut turn = HistoryTurn {
            id: id.clone(),
            status,
            agent: meta["agent"].as_str().map(str::to_string),
            agent_color_index: meta["agent_color_index"].as_u64().map(|v| v as usize),
            model_label: meta["model_label"].as_str().unwrap_or(&model).to_string(),
            usage: meta["usage"]
                .as_array()
                .and_then(|v| Some((v.first()?.as_u64()?, v.get(1)?.as_u64()?))),
            context_usage: meta["context_usage"]
                .as_array()
                .and_then(|v| Some((v.first()?.as_u64()?, v.get(1)?.as_u64()?))),
            duration_ms: meta["duration_ms"].as_u64(),
            streamed_ms: meta["streamed_ms"].as_u64(),
            parts: Vec::new(),
            ..HistoryTurn::default()
        };
        let total: Option<i64> = conn.query_row(
            "SELECT json_array_length(result,'$.display_parts') FROM turns WHERE id=?1",
            [&id],
            |r| r.get(0),
        )?;
        turn.legacy_text_only = total.is_none();
        // Per-turn part and byte serving budgets, independent of archive size.
        let mut stmt=conn.prepare_cached("SELECT p.value FROM turns t, json_each(t.result,'$.display_parts') p WHERE t.id=?1 ORDER BY CAST(p.key AS INTEGER) LIMIT 240")?;
        let refs = stmt.query_map([&id], |r| r.get::<_, String>(0))?;
        let mut budget = 64 * 1024usize;
        for (sequence, part) in refs.enumerate() {
            let part: serde_json::Value = serde_json::from_str(&part?).unwrap_or_default();
            if budget == 0 {
                break;
            }
            let mut state = PartState {
                sequence,
                status: turn.status.clone(),
                truncated: part["truncated"].as_bool().unwrap_or(false),
                input_omitted: false,
            };
            let before = turn.parts.len();
            if let Some(text) = part["reasoning"].as_str() {
                state.truncated |= text.len() > budget.min(16 * 1024);
                let text = text[..text.floor_char_boundary(budget.min(16 * 1024).min(text.len()))]
                    .to_string();
                budget = budget.saturating_sub(text.len());
                turn.parts.push(TranscriptPart::Reasoning {
                    text,
                    duration_ms: part["duration_ms"].as_u64(),
                });
            } else if let Some(index) = part["message"].as_u64() {
                let path = format!("$.input[{index}].content");
                let bytes: i64 = conn.query_row("SELECT COALESCE(SUM(length(CAST(c.value ->> '$.text' AS BLOB))),0) FROM turns t,json_each(t.result,?2) c WHERE t.id=?1 AND c.value ->> '$.type'='output_text'",params![id,path],|r|r.get(0))?;
                state.truncated |= bytes as usize > budget;
                let mut texts=conn.prepare_cached("SELECT substr(c.value ->> '$.text',1,?3) FROM turns t,json_each(t.result,?2) c WHERE t.id=?1 AND c.value ->> '$.type'='output_text'")?;
                let mut text = String::new();
                for item in
                    texts.query_map(params![id, path, budget as i64], |r| r.get::<_, String>(0))?
                {
                    let item = item?;
                    let kept =
                        item.floor_char_boundary(budget.saturating_sub(text.len()).min(item.len()));
                    text.push_str(&item[..kept]);
                }
                budget = budget.saturating_sub(text.len());
                turn.parts.push(TranscriptPart::Text(text));
            } else if let Some(op) = part["tool"].as_str() {
                // Input is structured JSON: cutting it at the output-preview
                // boundary destroys patch/path metadata. Serve complete admitted
                // input within the turn budget, otherwise honestly omit it.
                let view=conn.query_row("SELECT rowid,name,state,CASE WHEN length(CAST(input AS BLOB))<=?4 THEN input ELSE NULL END,substr(output,1,?3),length(CAST(output AS BLOB)) FROM tool_operations WHERE id=?1 AND turn_id=?2",params![op,id,TOOL_OP_PREVIEW_BYTES as i64,budget.saturating_sub(TOOL_OP_PREVIEW_BYTES) as i64],|r| {
                    let bytes=r.get::<_,Option<i64>>(5)?.unwrap_or(0);
                    let (output,output_truncated)=bound_preview(r.get(4)?,bytes);
                    Ok(ToolOpView{op:op.to_string(),rowid:r.get(0)?,name:r.get(1)?,state:r.get(2)?,input:r.get(3)?,output,output_bytes:bytes,output_truncated})
                }).optional()?;
                if let Some(view) = view {
                    state.status = view.state.clone();
                    state.input_omitted = view.input.is_none();
                    state.truncated |= state.input_omitted || view.output_truncated;
                    budget = budget.saturating_sub(
                        view.input.as_ref().map_or(0, String::len)
                            + view.output.as_ref().map_or(0, String::len),
                    );
                    turn.parts.push(TranscriptPart::Tool(view));
                }
            }
            if turn.parts.len() > before {
                turn.truncated |= state.truncated;
                turn.part_states.push(state);
            }
        }
        turn.omitted_parts = (total.unwrap_or(0) as usize).saturating_sub(turn.parts.len());
        turn.truncated |= turn.omitted_parts > 0;
        Ok(Some(turn))
    }

    /// Read a turn's terminal status and result JSON (turn-log replay).
    pub fn turn_result(&self, turn: &str) -> Result<(String, Option<String>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "SELECT status, result FROM turns WHERE id = ?1",
            params![turn],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(StorageError::Sqlite)
    }

    /// Apply the DCP v2 schema migration (idempotent, additive only).
    pub fn apply_dcp_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS compression_blocks(
               id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
               topic TEXT NOT NULL, summary TEXT NOT NULL,
               start_msg TEXT NOT NULL, end_msg TEXT NOT NULL,
               created_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS compression_members(
               block_id TEXT NOT NULL REFERENCES compression_blocks(id),
               message_id TEXT NOT NULL,
               PRIMARY KEY(block_id, message_id));
             CREATE TABLE IF NOT EXISTS prune_marks(
                session_id TEXT PRIMARY KEY REFERENCES sessions(id),
                up_to_msg TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS compression_members_message
                ON compression_members(message_id);
             CREATE TABLE IF NOT EXISTS dcp_tool_projection(
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 call_id TEXT NOT NULL, action TEXT NOT NULL,
                 PRIMARY KEY(session_id, call_id));
             CREATE TABLE IF NOT EXISTS dcp_tool_projection_v2(
                 session_id TEXT NOT NULL REFERENCES sessions(id),
                 call_id TEXT NOT NULL, occurrence INTEGER NOT NULL,
                 action TEXT NOT NULL,
                 PRIMARY KEY(session_id, call_id, occurrence));
             INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (2, 't17');",
        )?;
        Ok(())
    }

    /// Persist one compression block with explicit membership rows.
    ///
    /// Returns the block id (`b0001`, … per session). Raw message text is
    /// never copied: members reference stable message ids only.
    #[allow(clippy::too_many_arguments)]
    pub fn save_compression_block(
        &self,
        session: &str,
        topic: &str,
        summary: &str,
        start_msg: &str,
        end_msg: &str,
        members: &[String],
    ) -> Result<String, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        // Block ids are a global sequence: per-session COUNT would hand a
        // second session the same `b0001` and violate the primary key.
        let max: Option<i64> = tx.query_row(
            "SELECT MAX(CAST(SUBSTR(id, 2) AS INTEGER)) FROM compression_blocks",
            [],
            |row| row.get(0),
        )?;
        let id = format!("b{:04}", max.unwrap_or(0) + 1);
        let now = now_rfc3339();
        tx.execute(
            "INSERT INTO compression_blocks(id, session_id, topic, summary, start_msg, end_msg, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, session, topic, summary, start_msg, end_msg, now],
        )?;
        for message_id in members {
            tx.execute(
                "INSERT INTO compression_members(block_id, message_id) VALUES (?1, ?2)",
                params![id, message_id],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    /// Load a session's blocks in id order with ordered membership rows.
    pub fn load_compression_blocks(
        &self,
        session: &str,
    ) -> Result<Vec<CompressionBlockRow>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::load_compression_blocks_from(&conn, session)
    }

    fn load_compression_blocks_from(
        conn: &Connection,
        session: &str,
    ) -> Result<Vec<CompressionBlockRow>, StorageError> {
        let mut stmt = conn.prepare_cached(
            "SELECT id, topic, summary, start_msg, end_msg FROM compression_blocks
             WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let blocks = stmt.query_map(params![session], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for block in blocks {
            let (id, topic, summary, start_msg, end_msg) = block?;
            let mut members = conn.prepare_cached(
                "SELECT cm.message_id FROM compression_members AS cm
                 LEFT JOIN messages AS m ON m.id = cm.message_id
                 WHERE cm.block_id = ?1
                 ORDER BY m.seq IS NULL, m.seq ASC, cm.rowid ASC",
            )?;
            let rows = members.query_map(params![id], |row| row.get::<_, String>(0))?;
            let mut member_ids = Vec::new();
            for row in rows {
                member_ids.push(row?);
            }
            out.push(CompressionBlockRow {
                id,
                session: session.to_string(),
                topic,
                summary,
                start_msg,
                end_msg,
                members: member_ids,
            });
        }
        Ok(out)
    }

    /// Read one consistent DCP planning snapshot.
    pub(crate) fn compression_snapshot(
        &self,
        session: &str,
    ) -> Result<(Vec<CompressionBlockRow>, Option<String>, u64), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let blocks = Self::load_compression_blocks_from(&conn, session)?;
        let prune = conn
            .query_row(
                "SELECT up_to_msg FROM prune_marks WHERE session_id = ?1",
                params![session],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let max: Option<i64> = conn.query_row(
            "SELECT MAX(CAST(SUBSTR(id, 2) AS INTEGER)) FROM compression_blocks",
            [],
            |row| row.get(0),
        )?;
        let next = u64::try_from(max.unwrap_or(0))
            .unwrap_or(0)
            .saturating_add(1);
        Ok((blocks, prune, next))
    }

    /// Commit a prevalidated compression plan as one SQLite transaction.
    pub(crate) fn commit_compression_plan(
        &self,
        plan: CompressionPlanCommit<'_>,
    ) -> Result<(), StorageError> {
        let CompressionPlanCommit {
            session,
            blocks,
            consumed_blocks,
            expected_next,
            expected_existing,
            expected_prune,
            hidden_calls,
            purged_calls,
            tool,
        } = plan;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::require_session(&tx, session)?;

        let max: Option<i64> = tx.query_row(
            "SELECT MAX(CAST(SUBSTR(id, 2) AS INTEGER)) FROM compression_blocks",
            [],
            |row| row.get(0),
        )?;
        let actual_next = u64::try_from(max.unwrap_or(0))
            .unwrap_or(0)
            .saturating_add(1);
        if actual_next != expected_next {
            return Err(StorageError::CompressionConflict);
        }
        let actual_existing = {
            let mut statement = tx.prepare_cached(
                "SELECT id FROM compression_blocks WHERE session_id = ?1 ORDER BY id ASC",
            )?;
            statement
                .query_map(params![session], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let actual_prune = tx
            .query_row(
                "SELECT up_to_msg FROM prune_marks WHERE session_id = ?1",
                params![session],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if actual_existing != expected_existing || actual_prune.as_deref() != expected_prune {
            return Err(StorageError::CompressionConflict);
        }

        let mut candidate_members = std::collections::HashSet::new();
        let consumed_set = consumed_blocks
            .iter()
            .collect::<std::collections::HashSet<_>>();
        for block in consumed_blocks {
            let owned = tx
                .query_row(
                    "SELECT 1 FROM compression_blocks WHERE id = ?1 AND session_id = ?2",
                    params![block, session],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if !owned {
                return Err(StorageError::CompressionConflict);
            }
        }
        for (offset, block) in blocks.iter().enumerate() {
            let number = expected_next.saturating_add(offset as u64);
            if block.session != session || block.id != format!("b{number:04}") {
                return Err(StorageError::CompressionConflict);
            }
            for member in &block.members {
                if !candidate_members.insert(member.as_str()) {
                    return Err(StorageError::CompressionConflict);
                }
                let existing_block = tx
                    .query_row(
                        "SELECT cm.block_id FROM compression_members AS cm
                         JOIN compression_blocks AS cb ON cb.id = cm.block_id
                         WHERE cb.session_id = ?1 AND cm.message_id = ?2 LIMIT 1",
                        params![session, member],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()?;
                if existing_block
                    .as_ref()
                    .is_some_and(|block| !consumed_set.contains(block))
                {
                    return Err(StorageError::CompressionConflict);
                }
            }
        }

        if let Some(tool) = tool {
            if tool.turn_id.is_some() != tool.turn_log.is_some() {
                return Err(StorageError::CompressionConflict);
            }
            let operation_exists = match tool.turn_id {
                Some(turn) => tx
                    .query_row(
                        "SELECT 1 FROM tool_operations
                         WHERE id = ?1 AND session_id = ?2 AND turn_id = ?3",
                        params![tool.operation_id, session, turn],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some(),
                None => tx
                    .query_row(
                        "SELECT 1 FROM tool_operations
                         WHERE id = ?1 AND session_id = ?2 AND turn_id IS NULL",
                        params![tool.operation_id, session],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some(),
            };
            let turn_exists = match tool.turn_id {
                Some(turn) => tx
                    .query_row(
                        "SELECT 1 FROM turns WHERE id = ?1 AND session_id = ?2",
                        params![turn, session],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some(),
                None => true,
            };
            if !operation_exists || !turn_exists {
                return Err(StorageError::CompressionConflict);
            }
        }

        let now = now_rfc3339();
        for block in consumed_blocks {
            tx.execute(
                "DELETE FROM compression_members WHERE block_id = ?1",
                [block],
            )?;
        }
        for block in blocks {
            tx.execute(
                "INSERT INTO compression_blocks(id, session_id, topic, summary, start_msg, end_msg, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    block.id,
                    session,
                    block.topic,
                    block.summary,
                    block.start_msg,
                    block.end_msg,
                    now
                ],
            )?;
            for member in &block.members {
                tx.execute(
                    "INSERT INTO compression_members(block_id, message_id) VALUES (?1, ?2)",
                    params![block.id, member],
                )?;
            }
        }
        for (call_id, occurrence) in hidden_calls {
            let occurrence =
                i64::try_from(*occurrence).map_err(|_| StorageError::CompressionConflict)?;
            tx.execute(
                "INSERT INTO dcp_tool_projection_v2(session_id, call_id, occurrence, action)
                 VALUES (?1, ?2, ?3, 'hidden')
                 ON CONFLICT(session_id, call_id, occurrence) DO UPDATE SET action = 'hidden'",
                params![session, call_id, occurrence],
            )?;
        }
        for (call_id, occurrence) in purged_calls {
            if !hidden_calls.contains(&(call_id.clone(), *occurrence)) {
                let occurrence =
                    i64::try_from(*occurrence).map_err(|_| StorageError::CompressionConflict)?;
                tx.execute(
                    "INSERT INTO dcp_tool_projection_v2(session_id, call_id, occurrence, action)
                     VALUES (?1, ?2, ?3, 'purged')
                     ON CONFLICT(session_id, call_id, occurrence) DO UPDATE SET action = 'purged'",
                    params![session, call_id, occurrence],
                )?;
            }
        }

        if let Some(tool) = tool {
            tx.execute(
                "UPDATE tool_operations SET state = ?1, output = ?2 WHERE id = ?3",
                params![
                    tool.operation_state,
                    tool.operation_output,
                    tool.operation_id
                ],
            )?;
            if let (Some(turn), Some(log)) = (tool.turn_id, tool.turn_log) {
                tx.execute(
                    "UPDATE turns SET result = ?1 WHERE id = ?2",
                    params![log, turn],
                )?;
            }
            for (key, value) in tool.preference_updates {
                tx.execute(
                    "INSERT INTO prefs(key, value, updated_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
                    params![key, value, now_rfc3339()],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Load durable tool projection decisions for one session.
    pub(crate) fn load_dcp_tool_projection(
        &self,
        session: &str,
    ) -> Result<DcpToolProjection, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut statement = conn.prepare_cached(
            "SELECT call_id, occurrence, action FROM dcp_tool_projection_v2 WHERE session_id = ?1",
        )?;
        let rows = statement.query_map([session], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut projection = DcpToolProjection::default();
        for row in rows {
            let (call_id, occurrence, action) = row?;
            let occurrence =
                u64::try_from(occurrence).map_err(|_| StorageError::CompressionConflict)?;
            match action.as_str() {
                "hidden" => {
                    projection.hidden.insert((call_id, occurrence));
                }
                "purged" => {
                    projection.purged.insert((call_id, occurrence));
                }
                _ => return Err(StorageError::CompressionConflict),
            }
        }
        Ok(projection)
    }

    /// Record a prune mark: outbound context drops the prefix through `up_to`.
    pub fn save_prune_mark(&self, session: &str, up_to: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.prepare_cached(
            "INSERT INTO prune_marks(session_id, up_to_msg, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET up_to_msg = ?2, created_at = ?3",
        )?
        .execute(params![session, up_to, now_rfc3339()])?;
        Ok(())
    }

    /// Read the prune mark, if any.
    pub fn load_prune_mark(&self, session: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        match conn.query_row(
            "SELECT up_to_msg FROM prune_marks WHERE session_id = ?1",
            params![session],
            |row| row.get::<_, String>(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StorageError::Sqlite(e)),
        }
    }

    /// Record a tool intent (`started`) before the side effect.
    pub fn record_tool_intent(
        &self,
        op: &str,
        session: &str,
        turn: Option<&str>,
        name: &str,
        input: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.prepare_cached(
            "INSERT INTO tool_operations(id, session_id, turn_id, name, state, input, output)
             VALUES (?1, ?2, ?3, ?4, 'started', ?5, NULL)",
        )?
        .execute(params![op, session, turn, name, input])?;
        Ok(())
    }

    /// Commit the operation intent and its ordered display reference together,
    /// before execution; a crash cannot strand an invisible unknown operation.
    pub(crate) fn record_turn_tool_intent(
        &self,
        op: &str,
        session: &str,
        turn: &str,
        name: &str,
        input: &str,
        journal: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES(?1,?2,?3,?4,'started',?5,NULL)", params![op,session,turn,name,input])?;
        let n = tx.execute(
            "UPDATE turns SET result=?2 WHERE id=?1 AND session_id=?3 AND status='started'",
            params![turn, journal, session],
        )?;
        if n != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.commit()?;
        Ok(())
    }

    /// Record a tool outcome after the side effect.
    pub fn record_tool_outcome(
        &self,
        op: &str,
        state: &str,
        output: Option<&str>,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let n = conn
            .prepare_cached("UPDATE tool_operations SET state = ?1, output = ?2 WHERE id = ?3")?
            .execute(params![state, output, op])?;
        if n == 0 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        Ok(())
    }

    /// An in-flight MCP call lost its owning future. Preserve its checkpointed
    /// wire log and atomically mark both intent and turn unknown without replay.
    pub(crate) fn mark_dropped_mcp_call_unknown(
        &self,
        op: &str,
        turn: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let count = tx.execute(
            "UPDATE tool_operations SET state='unknown', output='error: mcp outcome unknown; retry may duplicate side effects' \
             WHERE id=?1 AND turn_id=?2 AND state='started'",
            params![op, turn],
        )?;
        if count != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        let count = tx.execute(
            "UPDATE turns SET status='unknown' WHERE id=?1 AND status='started'",
            [turn],
        )?;
        if count != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) SELECT session_id,'turn_unknown',id FROM turns WHERE id=?1",
            [turn],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Crash recovery: mark `started` operations and turns as `unknown`.
    ///
    /// Never replays the mutation; a new explicit attempt must use a new
    /// operation id. Returns the number of marked rows.
    pub fn recover_interrupted_tools(&self) -> Result<usize, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let n = tx
            .prepare_cached("UPDATE tool_operations SET state = 'unknown' WHERE state = 'started'")?
            .execute([])?;
        tx.execute("INSERT INTO events(session_id, kind, payload) SELECT session_id, 'turn_unknown', id FROM turns WHERE status = 'started'", [])?;
        tx.execute(
            "UPDATE turns SET status = 'unknown' WHERE status = 'started'",
            [],
        )?;
        tx.commit()?;
        Ok(n)
    }

    /// Read a tool operation state (tests + recovery inspection).
    pub fn tool_state(&self, op: &str) -> Result<String, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let state: String = conn.query_row(
            "SELECT state FROM tool_operations WHERE id = ?1",
            params![op],
            |row| row.get(0),
        )?;
        Ok(state)
    }

    /// Store bytes as a content-addressed blob.
    ///
    /// Order: quota check → atomic temp/write/fsync/rename → directory fsync
    /// → durable DB row. Quota includes regular orphan/temp files on disk.
    /// A crash between file and row leaves a safe unreferenced orphan that
    /// [`Db::gc_orphans`] collects after a grace period; referenced blobs are
    /// never deleted.
    pub fn write_blob(&self, data: &[u8]) -> Result<String, StorageError> {
        let digest = hex_digest(data);
        let target = self.blob_path(&digest)?;
        // Keep publication and GC under the same lock, including filesystem I/O.
        let conn = self.conn.lock().expect("db mutex");
        let existing = match fs::symlink_metadata(&target) {
            Ok(meta) if meta.is_file() => {
                if fs::read(&target)? != data {
                    return Err(invalid_blob());
                }
                true
            }
            Ok(_) => return Err(invalid_blob()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
            Err(err) => return Err(err.into()),
        };
        let mut used = data.len() as u64;
        for entry in fs::read_dir(&self.blob_dir)? {
            let entry = entry?;
            let meta = fs::symlink_metadata(entry.path())?;
            if meta.is_file() && entry.path() != target {
                used = used.saturating_add(meta.len());
            }
        }
        if used > self.quota_bytes {
            return Err(StorageError::StorageFull);
        }
        if existing {
            // An orphan may have been left before file/directory sync completed.
            File::open(&target)?.sync_all()?;
        } else {
            self.publish_blob(&target, &digest, data)?;
        }
        File::open(&self.blob_dir)?.sync_all()?;
        // Also persist the blob-directory entry when the data root is fresh.
        File::open(&self.root)?.sync_all()?;
        let size = i64::try_from(data.len()).map_err(|_| StorageError::StorageFull)?;
        // File existence alone is insufficient: repair absent/stale metadata.
        conn.execute(
            "INSERT INTO blobs(digest, size, path) VALUES (?1, ?2, ?1)
             ON CONFLICT(digest) DO UPDATE SET size = excluded.size, path = excluded.path",
            params![digest, size],
        )?;
        Ok(digest)
    }

    fn publish_blob(&self, target: &Path, digest: &str, data: &[u8]) -> Result<(), StorageError> {
        // Unique create_new names allow retries despite abandoned crash temps.
        static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let (tmp_path, mut tmp) = loop {
            let serial = NEXT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = self.blob_dir.join(format!(
                ".tmp-{}-{}-{serial}",
                std::process::id(),
                &digest[..16]
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => break (path, file),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => return Err(err.into()),
            }
        };
        let result = (|| {
            tmp.write_all(data)?;
            tmp.sync_all()?;
            fs::rename(&tmp_path, target)
        })();
        if result.is_err() {
            // Only remove the temporary file this invocation actually created.
            fs::remove_file(&tmp_path)?;
            File::open(&self.blob_dir)?.sync_all()?;
        }
        result.map_err(StorageError::from)
    }

    /// Read blob bytes by digest.
    pub fn read_blob(&self, digest: &str) -> Result<Vec<u8>, StorageError> {
        let path = self.blob_path(digest)?;
        let conn = self.conn.lock().expect("db mutex");
        let metadata: Option<(i64, String)> = conn
            .query_row(
                "SELECT size, path FROM blobs WHERE digest = ?1",
                params![digest],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (size, stored_path) = metadata.ok_or(StorageError::BlobNotFound)?;
        let meta = fs::symlink_metadata(&path)?;
        if stored_path != digest || u64::try_from(size).ok() != Some(meta.len()) || !meta.is_file()
        {
            return Err(invalid_blob());
        }
        let bytes = fs::read(path)?;
        if hex_digest(&bytes) != digest {
            return Err(invalid_blob());
        }
        Ok(bytes)
    }

    /// Collect unreferenced blob files older than `grace`.
    ///
    /// Never deletes referenced blobs. Never follows symlinks outside the
    /// blob dir and never deletes outside it (cleanup-escape refusal).
    pub fn gc_orphans(&self, grace: Duration) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let referenced: std::collections::HashSet<String> = {
            let mut stmt = conn.prepare_cached("SELECT digest FROM blobs")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut set = std::collections::HashSet::new();
            for row in rows {
                set.insert(row?);
            }
            set
        };
        let mut removed = 0;
        for entry in fs::read_dir(&self.blob_dir)? {
            let entry = entry?;
            let path = entry.path();
            // Never follow symlinks; never touch anything outside blob_dir.
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() {
                continue;
            }
            if !path.starts_with(&self.blob_dir) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if referenced.contains(&name) {
                continue;
            }
            if is_older_than(&path, grace)? {
                fs::remove_file(&path)?;
                File::open(&self.blob_dir)?.sync_all()?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn blob_path(&self, digest: &str) -> Result<PathBuf, StorageError> {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StorageError::BlobNotFound);
        }
        let path = self.blob_dir.join(digest);
        if !path.starts_with(&self.blob_dir) {
            return Err(StorageError::UnsafeRoot("blob escape".to_string()));
        }
        Ok(path)
    }
}

fn hex_digest(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Revision of the exact raw pref value. No raw JSON crosses the app boundary.
pub(crate) fn pref_revision(raw: &str) -> String {
    hex_digest(raw.as_bytes())
}

fn invalid_blob() -> StorageError {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid blob content or metadata",
    )
    .into()
}

fn now_rfc3339() -> String {
    // Normalized second-precision UTC without external time crates.
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

fn validate_root(root: &Path) -> Result<PathBuf, StorageError> {
    if !root.is_absolute() {
        return Err(StorageError::UnsafeRoot("relative data root".to_string()));
    }
    if root == Path::new("/") || root == Path::new("/tmp") {
        return Err(StorageError::UnsafeRoot("system data root".to_string()));
    }
    for comp in root.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(StorageError::UnsafeRoot("parent escape".to_string()));
        }
    }
    // No-follow: the root itself must not be a symlink.
    if let Ok(meta) = fs::symlink_metadata(root)
        && meta.file_type().is_symlink()
    {
        return Err(StorageError::UnsafeRoot("symlink data root".to_string()));
    }
    Ok(root.to_path_buf())
}

fn check_owner(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let meta = fs::metadata(path)?;
        let uid = meta.uid();
        // SAFETY: getuid has no failure mode and does not touch Rust memory.
        let me: u32 = unsafe { libc::getuid() };
        if uid != me {
            return Err(StorageError::UnsafeRoot(
                "foreign data root owner".to_string(),
            ));
        }
    }
    Ok(())
}

fn check_restrictive(path: &Path) -> Result<(), StorageError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let meta = fs::metadata(path)?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(StorageError::UnsafeRoot(
                "permissive data root mode".to_string(),
            ));
        }
    }
    Ok(())
}

fn try_lock_exclusive(lock: &File) -> Result<(), StorageError> {
    match lock.try_lock_exclusive() {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Err(StorageError::DataRootBusy),
        Err(err) => Err(StorageError::Io(err)),
    }
}

fn is_older_than(path: &Path, grace: Duration) -> Result<bool, StorageError> {
    let mtime = fs::metadata(path)?.modified()?;
    let age = SystemTime::now().duration_since(mtime).unwrap_or_default();
    Ok(age >= grace)
}

/// Apply the additive child-session migration (schema version 3).
///
/// Adds nullable `parent_id`/`agent`/`model`/`title` columns and the
/// `parent_id` index to `sessions`. Idempotent: a database that already
/// recorded version 3 is left untouched, and a fresh database reaches the
/// same schema as an upgraded one. `ALTER TABLE ... ADD COLUMN` cannot
/// express `IF NOT EXISTS`, so columns are checked before being added.
fn apply_child_session_schema(conn: &Connection) -> Result<(), StorageError> {
    let applied: Option<i64> = conn
        .query_row(
            "SELECT version FROM schema_migrations WHERE version = ?1",
            params![CHILD_SESSION_SCHEMA_VERSION],
            |row| row.get(0),
        )
        .optional()?;
    if applied.is_some() {
        return Ok(());
    }
    for (column, decl) in CHILD_SESSION_COLUMNS {
        if !column_exists(conn, "sessions", column)? {
            conn.execute_batch(&format!("ALTER TABLE sessions ADD COLUMN {column} {decl}"))?;
        }
    }
    conn.execute_batch("CREATE INDEX IF NOT EXISTS sessions_parent ON sessions(parent_id)")?;
    conn.execute(
        "INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (?1, 't43')",
        params![CHILD_SESSION_SCHEMA_VERSION],
    )?;
    Ok(())
}

/// Whether `table` already has a column named `column`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, StorageError> {
    let mut stmt = conn.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2")?;
    let found: Option<i64> = stmt
        .query_row(params![table, column], |row| row.get(0))
        .optional()?;
    Ok(found.is_some())
}

fn apply_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS messages(
           id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
           seq INTEGER NOT NULL, role TEXT NOT NULL, text TEXT NOT NULL,
           UNIQUE(session_id, seq));
         CREATE TABLE IF NOT EXISTS turns(
           id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
           status TEXT NOT NULL, prompt TEXT NOT NULL, result TEXT);
         CREATE TABLE IF NOT EXISTS tool_operations(
           id TEXT PRIMARY KEY, session_id TEXT NOT NULL, turn_id TEXT,
           name TEXT NOT NULL, state TEXT NOT NULL, input TEXT, output TEXT);
         CREATE TABLE IF NOT EXISTS events(
           seq INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL,
           kind TEXT NOT NULL, payload TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS blobs(digest TEXT PRIMARY KEY, size INTEGER NOT NULL, path TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS prefs(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
         INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, 't04');",
    )?;
    apply_child_session_schema(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Db, SESSION_LOCATION_PREFIX, SessionMeta, StorageError, StoredDeck, TAB_ADOPTION_VALUE,
        tab_adoption_key, tab_deck_key,
    };
    use rusqlite::OptionalExtension as _;
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use std::time::Duration;

    /// `(name, type)` of a table's columns in declaration order.
    fn session_columns(db: &Db) -> Vec<(String, String)> {
        let conn = db.conn.lock().expect("db mutex");
        let mut stmt = conn
            .prepare("SELECT name, type FROM pragma_table_info('sessions') ORDER BY cid ASC")
            .expect("prepare");
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .expect("rows");
        rows.collect::<Result<Vec<_>, _>>().expect("columns")
    }

    /// `(seq, role, text)` rows as stored, never via a projection.
    fn raw_history(db: &Db, session: &str) -> Vec<(i64, String, String)> {
        let conn = db.conn.lock().expect("db mutex");
        let mut stmt = conn
            .prepare("SELECT seq, role, text FROM messages WHERE session_id = ?1 ORDER BY seq ASC")
            .expect("prepare");
        let rows = stmt
            .query_map([session], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("rows");
        rows.collect::<Result<Vec<_>, _>>().expect("rows")
    }

    /// Applied migration versions in order.
    fn migrations(db: &Db) -> Vec<i64> {
        let conn = db.conn.lock().expect("db mutex");
        let mut stmt = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .expect("prepare");
        let rows = stmt.query_map([], |row| row.get(0)).expect("rows");
        rows.collect::<Result<Vec<_>, _>>().expect("rows")
    }

    fn tmp_root(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        // Keep the name in the test log without moving the dir.
        let _ = name;
        dir
    }

    #[test]
    fn manual_root_title_overrides_generated_and_rolls_back_with_event_failure() {
        let tmp = tmp_root("manual-title");
        let db = Db::open(tmp.path()).unwrap();
        db.create_session("root").unwrap();
        db.create_child_session("root", "child", None, None, Some("child title"))
            .unwrap();
        db.set_generated_title("root", "generated").unwrap();
        db.rename_root_session("root", "manual").unwrap();
        db.set_generated_title("root", "late generation").unwrap();
        assert_eq!(
            db.session_meta("root").unwrap().title.as_deref(),
            Some("manual")
        );
        assert!(matches!(
            db.rename_root_session("child", "wrong"),
            Err(StorageError::SessionNotFound)
        ));
        assert!(matches!(
            db.rename_root_session("missing", "wrong"),
            Err(StorageError::SessionNotFound)
        ));
        assert_eq!(
            db.session_meta("child").unwrap().title.as_deref(),
            Some("child title")
        );
        let conn = db.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM events WHERE session_id='root' AND kind='session_updated'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        conn.execute_batch("CREATE TRIGGER fail_title_event BEFORE INSERT ON events WHEN NEW.kind='session_updated' BEGIN SELECT RAISE(ABORT, 'injected'); END;").unwrap();
        drop(conn);
        assert!(db.rename_root_session("root", "should roll back").is_err());
        assert_eq!(
            db.session_meta("root").unwrap().title.as_deref(),
            Some("manual")
        );
    }

    #[test]
    fn second_owner_is_refused() {
        let tmp = tmp_root("busy");
        let root = tmp.path().join("data");
        let _first = Db::open(&root).expect("first open");
        let second = Db::open(&root);
        assert!(matches!(second, Err(StorageError::DataRootBusy)));
    }

    #[test]
    fn v02_bounded_projection_exposes_loss_and_legacy_availability() {
        use serde_json::json;
        let tmp = tmp_root("projection");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "inspect").unwrap();
        let parts: Vec<_> = (0..250).map(|_| json!({"reasoning":"thought"})).collect();
        db.checkpoint_turn("t", &json!({"display_parts":parts}).to_string())
            .unwrap();
        let projection = db.turn_presentation("s", "t").unwrap().unwrap();
        assert_eq!(projection.parts.len(), 240);
        assert_eq!(projection.omitted_parts, 10);
        assert!(projection.truncated);
        assert_eq!(projection.part_states[239].sequence, 239);
        db.record_tool_intent("op", "s", Some("t"), "apply_patch", &"x".repeat(70 * 1024))
            .unwrap();
        db.checkpoint_turn("t",&json!({"display_parts":[{"tool":"op"},{"message":0}],"input":[{"content":[{"type":"output_text","text":"é".repeat(70*1024)}]}]}).to_string()).unwrap();
        let projection = db.turn_presentation("s", "t").unwrap().unwrap();
        assert!(projection.part_states[0].input_omitted);
        assert!(projection.part_states[1].truncated);
        assert!(
            matches!(&projection.parts[0],oc_core::queries::TranscriptPart::Tool(op) if op.op=="op")
        );
        db.checkpoint_turn("t", "{}").unwrap();
        let legacy = db.turn_presentation("s", "t").unwrap().unwrap();
        assert!(legacy.legacy_text_only);
        assert!(legacy.parts.is_empty());
    }

    #[test]
    fn context_usage_projection_requires_a_reported_pair_and_keeps_billing_separate() {
        use serde_json::json;

        let tmp = tmp_root("context-usage");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "inspect").unwrap();
        db.checkpoint_turn("t", &json!({"display_parts":[]}).to_string())
            .unwrap();
        db.update_turn_display("t", &json!({"context_usage":[6000,763]}))
            .unwrap();
        let projected = db.turn_presentation("s", "t").unwrap().unwrap();
        assert_eq!(projected.context_usage, Some((6000, 763)));
        assert_eq!(projected.usage, None);

        drop(db);
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let reopened = db.turn_presentation("s", "t").unwrap().unwrap();
        assert_eq!(reopened.context_usage, Some((6000, 763)));
        assert_eq!(reopened.usage, None);

        db.update_turn_display("t", &json!({"context_usage":[6000]}))
            .unwrap();
        assert_eq!(
            db.turn_presentation("s", "t")
                .unwrap()
                .unwrap()
                .context_usage,
            None
        );
        db.update_turn_display("t", &json!({"context_usage":null}))
            .unwrap();
        assert_eq!(
            db.turn_presentation("s", "t")
                .unwrap()
                .unwrap()
                .context_usage,
            None
        );
    }

    #[test]
    fn turn_tool_reference_and_intent_are_atomic() {
        let tmp = tmp_root("display-intent");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "inspect").unwrap();
        db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_display BEFORE UPDATE OF result ON turns BEGIN SELECT RAISE(ABORT, 'injected display failure'); END;").unwrap();
        assert!(
            db.record_turn_tool_intent(
                "op",
                "s",
                "t",
                "read",
                "{}",
                "{\"display_parts\":[{\"tool\":\"op\"}]}"
            )
            .is_err()
        );
        assert!(
            db.tool_state("op").is_err(),
            "no orphaned intent without its display reference"
        );
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_display;")
            .unwrap();
        db.record_turn_tool_intent(
            "op",
            "s",
            "t",
            "read",
            "{}",
            "{\"display_parts\":[{\"tool\":\"op\"}]}",
        )
        .unwrap();
        assert_eq!(db.tool_state("op").unwrap(), "started");
        assert!(
            db.turn_result("t")
                .unwrap()
                .1
                .unwrap()
                .contains("display_parts")
        );
    }

    #[test]
    fn durable_input_turn_event_roundtrip() {
        let tmp = tmp_root("durable");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        db.create_session("s-1").expect("session");
        let m1 = db.append_message("s-1", "user", "hello").expect("msg");
        assert_eq!(m1, "m0001");
        db.begin_turn("t-1", "s-1", "hello").expect("begin");
        db.record_tool_intent("op-1", "s-1", Some("t-1"), "read", "{}")
            .expect("intent");
        db.record_tool_outcome("op-1", "succeeded", Some("ok"))
            .expect("outcome");
        db.finish_turn("t-1", "completed", Some("done"))
            .expect("finish");
        let history = db.read_history("s-1").expect("history");
        assert_eq!(history, vec![("user".to_string(), "hello".to_string())]);
        assert_eq!(db.list_sessions().expect("list"), vec!["s-1".to_string()]);
        assert_eq!(db.tool_state("op-1").expect("state"), "succeeded");
    }

    #[test]
    fn crash_intent_recovers_unknown_without_replay() {
        let tmp = tmp_root("crash");
        let root = tmp.path().join("data");
        {
            let db = Db::open(&root).expect("open");
            db.create_session("s-c").expect("session");
            db.record_tool_intent("op-crash", "s-c", None, "bash", "sleep 1")
                .expect("intent");
            // Drop without outcome: simulated kill after intent, before side-effect ack.
        }
        {
            let db = Db::open(&root).expect("reopen");
            let marked = db.recover_interrupted_tools().expect("recover");
            assert_eq!(marked, 1);
            assert_eq!(db.tool_state("op-crash").expect("state"), "unknown");
            // Second recovery is a no-op; never autoreplays the mutation.
            assert_eq!(db.recover_interrupted_tools().expect("again"), 0);
        }
    }

    #[test]
    fn blob_orphan_collected_referenced_kept() {
        let tmp = tmp_root("blob");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        let digest = db.write_blob(b"referenced-bytes").expect("write");
        assert_eq!(db.read_blob(&digest).expect("read"), b"referenced-bytes");
        // Simulate crash: file durable, DB row missing.
        let orphan = db.root().join("blobs").join("orphan-file");
        fs::write(&orphan, b"orphan").expect("orphan write");
        // Grace zero collects only the orphan; the referenced blob survives.
        let removed = db.gc_orphans(Duration::ZERO).expect("gc");
        assert_eq!(removed, 1);
        assert!(!orphan.exists());
        assert!(db.root().join("blobs").join(&digest).exists());
        // Referenced deletion is never performed by GC.
        let removed = db.gc_orphans(Duration::ZERO).expect("gc2");
        assert_eq!(removed, 0);
    }

    #[test]
    fn unsafe_roots_refused() {
        assert!(matches!(
            Db::open(Path::new("/")),
            Err(StorageError::UnsafeRoot(_))
        ));
        assert!(matches!(
            Db::open(Path::new("/tmp")),
            Err(StorageError::UnsafeRoot(_))
        ));
        assert!(matches!(
            Db::open(Path::new("relative/path")),
            Err(StorageError::UnsafeRoot(_))
        ));
        assert!(matches!(
            Db::open(Path::new("/tmp/../tmp/data-test-oc")),
            Err(StorageError::UnsafeRoot(_))
        ));
    }

    #[test]
    fn symlink_root_and_escape_not_followed() {
        let tmp = tmp_root("link");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).expect("outside");
        let link = tmp.path().join("link-root");
        std::os::unix::fs::symlink(&outside, &link).expect("symlink");
        assert!(matches!(Db::open(&link), Err(StorageError::UnsafeRoot(_))));

        let db = Db::open(&tmp.path().join("data")).expect("open");
        // Symlink inside blob dir pointing outside must not be followed/deleted.
        let victim = outside.join("victim.txt");
        fs::write(&victim, b"do-not-delete").expect("victim");
        std::os::unix::fs::symlink(&victim, db.root().join("blobs").join("evil-link"))
            .expect("evil link");
        let removed = db.gc_orphans(Duration::ZERO).expect("gc");
        assert!(
            victim.exists(),
            "outside file must survive, removed={removed}"
        );
    }

    #[test]
    fn quota_and_permissions_enforced() {
        let tmp = tmp_root("quota");
        let db = Db::open_with_quota(&tmp.path().join("data"), 16).expect("open");
        db.write_blob(b"12345678").expect("fits");
        let err = db.write_blob(b"234567890").expect_err("over quota");
        assert!(matches!(err, StorageError::StorageFull));

        // Permissive existing root is secured to 0700 on next open.
        let root2 = tmp.path().join("loose");
        {
            let _db = Db::open(&root2).expect("open loose");
        }
        fs::set_permissions(&root2, fs::Permissions::from_mode(0o777)).expect("chmod");
        let _db = Db::open(&root2).expect("reopen secures");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&root2).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn history_pages_and_tool_ops_are_bounded() {
        let tmp = tmp_root("tui22");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        assert!(db.read_history_page("ghost", 10, None).is_err());
        assert!(db.list_tool_ops("ghost").is_err());
        db.create_session("s").expect("session");
        assert_eq!(db.history_len("s").expect("len"), 0);
        for i in 0..5 {
            db.append_message("s", "user", &format!("m{i}"))
                .expect("msg");
        }
        assert_eq!(db.history_len("s").expect("len"), 5);
        let page = db.read_history_page("s", 2, None).expect("page");
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].0, 5);
        assert_eq!(page[1].2, "m3");
        let rest = db
            .read_history_page("s", 100, Some(page[1].0))
            .expect("rest");
        assert_eq!(rest.len(), 3);
        // Over-limit requests clamp instead of growing the store.
        let clamped = db.read_history_page("s", 10_000, None).expect("clamp");
        assert_eq!(clamped.len(), 5);

        db.record_tool_intent("op1", "s", None, "read", "{}")
            .expect("intent");
        db.record_tool_outcome("op1", "completed", Some("ok"))
            .expect("outcome");
        let ops = db.list_tool_ops("s").expect("ops");
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].name, "read");
        assert_eq!(ops[0].state, "completed");
    }

    #[test]
    fn prefs_roundtrip() {
        let tmp = tmp_root("prefs");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        assert_eq!(db.get_pref("tui.x").expect("get"), None);
        db.set_pref("tui.x", "{\"a\":1}").expect("set");
        assert_eq!(
            db.get_pref("tui.x").expect("get"),
            Some("{\"a\":1}".to_string())
        );
        db.set_pref("tui.x", "v2").expect("overwrite");
        assert_eq!(db.get_pref("tui.x").expect("get"), Some("v2".to_string()));
    }

    #[test]
    fn child_schema_migration_is_idempotent_across_reopen() {
        let tmp = tmp_root("child-migrate");
        let root = tmp.path().join("data");
        let fresh = {
            let db = Db::open(&root).expect("open");
            let columns = session_columns(&db);
            let names: Vec<&str> = columns.iter().map(|(name, _)| name.as_str()).collect();
            assert_eq!(
                names,
                ["id", "created_at", "parent_id", "agent", "model", "title"]
            );
            assert!(columns.iter().all(|(_, ty)| ty == "TEXT"));
            assert_eq!(db.list_sessions().expect("list"), Vec::<String>::new());
            // One `sessions` row per session; the index exists even on fresh DBs.
            let conn = db.conn.lock().expect("db mutex");
            let index: Option<String> = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'sessions_parent'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .expect("index lookup");
            assert_eq!(index.as_deref(), Some("sessions_parent"));
            // Sentinel proves a reopen skips the migration instead of
            // re-running the guarded ALTERs.
            conn.execute(
                "UPDATE schema_migrations SET applied_at = 'sentinel' WHERE version = 3",
                [],
            )
            .expect("mark");
            columns
        };
        let db = Db::open(&root).expect("reopen");
        assert_eq!(session_columns(&db), fresh, "reopen keeps the schema");
        assert_eq!(migrations(&db), vec![1, 3]);
        let conn = db.conn.lock().expect("db mutex");
        let applied: String = conn
            .query_row(
                "SELECT applied_at FROM schema_migrations WHERE version = 3",
                [],
                |row| row.get(0),
            )
            .expect("v3 row");
        assert_eq!(applied, "sentinel", "reopen re-applied migration 3");
    }

    #[test]
    fn child_schema_upgrades_legacy_database_to_same_schema() {
        let tmp = tmp_root("child-upgrade");
        let root = tmp.path().join("data");
        fs::create_dir_all(&root).expect("root");
        // A v1 database: sessions without the child columns, migration 1 only.
        {
            let conn = rusqlite::Connection::open(root.join("oc.sqlite")).expect("legacy sqlite");
            conn.execute_batch(
                "CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);
                 CREATE TABLE sessions(id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
                 INSERT INTO schema_migrations(version, applied_at) VALUES (1, 't04');
                 INSERT INTO sessions(id, created_at) VALUES ('legacy', '1');",
            )
            .expect("legacy schema");
        }
        let db = Db::open(&root).expect("open legacy");
        assert_eq!(migrations(&db), vec![1, 3]);
        let legacy = db.session_meta("legacy").expect("legacy meta");
        assert_eq!(
            legacy,
            SessionMeta {
                parent_id: None,
                agent: None,
                model: None,
                title: None,
            }
        );
        assert_eq!(
            db.children_of("legacy").expect("children"),
            Vec::<String>::new()
        );

        let fresh_root = tmp.path().join("fresh");
        let fresh = Db::open(&fresh_root).expect("fresh open");
        assert_eq!(
            session_columns(&db),
            session_columns(&fresh),
            "upgraded schema equals fresh schema"
        );
    }

    #[test]
    fn child_rows_persist_across_reopen() {
        let tmp = tmp_root("child-persist");
        let root = tmp.path().join("data");
        {
            let db = Db::open(&root).expect("open");
            db.create_session("root").expect("root");
            db.create_child_session(
                "root",
                "kid",
                Some("explore"),
                Some("openai/gpt-5#low"),
                Some("Review code"),
            )
            .expect("child");
            db.append_message("kid", "user", "hello").expect("msg");
        }
        let db = Db::open(&root).expect("reopen");
        assert_eq!(
            db.session_meta("kid").expect("meta"),
            SessionMeta {
                parent_id: Some("root".to_string()),
                agent: Some("explore".to_string()),
                model: Some("openai/gpt-5#low".to_string()),
                title: Some("Review code".to_string()),
            }
        );
        assert_eq!(
            db.children_of("root").expect("children"),
            vec!["kid".to_string()]
        );
        assert_eq!(
            db.read_history("kid").expect("history"),
            vec![("user".to_string(), "hello".to_string())]
        );
        let root_meta = db.session_meta("root").expect("root meta");
        assert_eq!(root_meta.parent_id, None);
        assert_eq!(root_meta.agent, None);
        assert_eq!(root_meta.model, None);
        assert_eq!(root_meta.title, None);
    }

    #[test]
    fn children_of_returns_only_direct_children_in_insertion_order() {
        let tmp = tmp_root("child-order");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        db.create_session("p").expect("p");
        db.create_session("other").expect("other");
        // Ids are deliberately out of lexical order: insertion order wins.
        db.create_child_session("p", "c-b", None, None, None)
            .expect("c-b");
        db.create_child_session("p", "c-a", None, None, None)
            .expect("c-a");
        db.create_child_session("other", "c-x", None, None, None)
            .expect("c-x");
        db.create_child_session("c-b", "g", None, None, None)
            .expect("g");
        db.create_child_session("p", "c-c", None, None, None)
            .expect("c-c");
        assert_eq!(db.children_of("p").expect("p"), vec!["c-b", "c-a", "c-c"]);
        assert_eq!(db.children_of("c-b").expect("c-b"), vec!["g"]);
        assert_eq!(db.children_of("other").expect("other"), vec!["c-x"]);
        assert_eq!(db.children_of("g").expect("leaf"), Vec::<String>::new());
        assert!(matches!(
            db.children_of("ghost"),
            Err(StorageError::SessionNotFound)
        ));
    }

    #[test]
    fn child_creation_never_touches_parent_history() {
        let tmp = tmp_root("child-immutable");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        db.create_session("p").expect("p");
        db.append_message("p", "user", "one").expect("m1");
        db.append_message("p", "assistant", "two").expect("m2");
        let before = raw_history(&db, "p");
        db.create_child_session("p", "k1", Some("general"), None, None)
            .expect("k1");
        db.append_message("k1", "user", "child text")
            .expect("child msg");
        db.create_child_session("p", "k2", None, Some("model"), Some("title"))
            .expect("k2");
        assert_eq!(raw_history(&db, "p"), before);
        assert_eq!(db.history_len("p").expect("len"), 2);
    }

    #[test]
    fn create_child_session_requires_parent_and_unique_ids() {
        let tmp = tmp_root("child-errors");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        assert!(matches!(
            db.create_child_session("ghost", "k", None, None, None),
            Err(StorageError::SessionNotFound)
        ));
        assert!(matches!(
            db.session_meta("k"),
            Err(StorageError::SessionNotFound)
        ));
        db.create_session("p").expect("p");
        db.create_child_session("p", "k", None, None, None)
            .expect("child");
        assert!(matches!(
            db.create_child_session("p", "k", None, None, None),
            Err(StorageError::SessionAlreadyExists)
        ));
        assert!(matches!(
            db.create_session("k"),
            Err(StorageError::SessionAlreadyExists)
        ));
    }

    #[test]
    fn root_creation_rolls_back_when_event_insert_fails() {
        let tmp = tmp_root("root-atomic");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        db.conn
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_session_created BEFORE INSERT ON events
             WHEN NEW.kind = 'session_created'
             BEGIN SELECT RAISE(ABORT, 'injected event failure'); END;",
            )
            .unwrap();

        assert!(matches!(
            db.create_session("root"),
            Err(StorageError::Sqlite(_))
        ));
        {
            let conn = db.conn.lock().unwrap();
            let sessions: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sessions WHERE id = 'root'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let events: i64 = conn
                .query_row(
                    "SELECT count(*) FROM events WHERE session_id = 'root'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(sessions, 0, "failed creation must not leave a session row");
            assert_eq!(events, 0, "failed creation must not leave an event");
            conn.execute_batch("DROP TRIGGER fail_session_created;")
                .unwrap();
        }

        db.create_session("root")
            .expect("retry must not encounter a duplicate");
        assert!(matches!(
            db.create_session("root"),
            Err(StorageError::SessionAlreadyExists)
        ));
        let conn = db.conn.lock().unwrap();
        let sessions: i64 = conn
            .query_row(
                "SELECT count(*) FROM sessions WHERE id = 'root'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let events: i64 = conn
            .query_row("SELECT count(*) FROM events WHERE session_id = 'root' AND kind = 'session_created'", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sessions, 1);
        assert_eq!(events, 1);
    }

    /// Verify every durable component, including the binding, through the
    /// backing tables rather than through the public session projection.
    fn fresh_turn_rows(db: &Db, session: &str, turn: &str) -> (i64, i64, i64, i64, i64) {
        let conn = db.conn.lock().unwrap();
        let count = |sql: &str, value: &str| -> i64 {
            conn.query_row(sql, [value], |row| row.get(0)).unwrap()
        };
        (
            count("SELECT count(*) FROM sessions WHERE id = ?1", session),
            count(
                "SELECT count(*) FROM prefs WHERE key = ?1",
                &format!("{SESSION_LOCATION_PREFIX}{session}"),
            ),
            count("SELECT count(*) FROM events WHERE session_id = ?1", session),
            count("SELECT count(*) FROM turns WHERE id = ?1", turn),
            count(
                "SELECT count(*) FROM messages WHERE session_id = ?1",
                session,
            ),
        )
    }

    #[test]
    fn fresh_admission_checks_union_before_root_insert_and_deduplicates_markers() {
        let tmp = tmp_root("fresh-deck-union");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let location = "/project";
        let key = tab_deck_key(location);
        let names: Vec<String> = (0..15).map(|i| format!("saved-{i}")).collect();
        let deck = StoredDeck {
            version: 1,
            sessions: names,
            active: Some("saved-0".into()),
        };
        db.set_pref(&key, &serde_json::to_string(&deck).unwrap())
            .unwrap();
        // A marker already represented by the stored preference must not
        // consume a second slot.
        db.create_bound_session("saved-0", location).unwrap();
        db.set_pref(&tab_adoption_key(location, "saved-0"), TAB_ADOPTION_VALUE)
            .unwrap();
        db.conn.lock().unwrap().execute_batch(
            "CREATE TRIGGER reject_root BEFORE INSERT ON sessions
             WHEN NEW.id = 'blocked' BEGIN SELECT RAISE(ABORT, 'root inserted before admission'); END;"
        ).unwrap();
        let accepted = db.create_bound_session_and_accept_turn(
            "sixteenth",
            location,
            "t1",
            "prompt",
            "user",
            None,
        );
        assert_eq!(accepted.unwrap(), "m0001");
        let refusal = db.create_bound_session_and_accept_turn(
            "blocked", location, "t2", "prompt", "user", None,
        );
        assert!(
            matches!(refusal, Err(StorageError::Io(ref e)) if e.kind() == std::io::ErrorKind::InvalidData)
        );
        assert_eq!(fresh_turn_rows(&db, "blocked", "t2"), (0, 0, 0, 0, 0));
        assert_eq!(
            db.get_pref(&tab_adoption_key(location, "blocked")).unwrap(),
            None
        );
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_root")
            .unwrap();
        // The trigger is gone; the full union still refuses the root.
        assert!(matches!(
            db.create_bound_session_and_accept_turn("blocked", location, "t2", "prompt", "user", None),
            Err(StorageError::Io(ref e)) if e.kind() == std::io::ErrorKind::InvalidData
        ));
    }

    #[test]
    fn fresh_turn_sql_failures_roll_back_root_binding_and_every_event() {
        let tmp = tmp_root("fresh-turn-failure");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let selection_key = |session: &str| {
            format!("tui.selection.session:[\"/project\",\"provider\",\"{session}\"]")
        };
        for (table, session, turn) in [
            ("turns", "turn-fail", "t1"),
            ("messages", "message-fail", "t2"),
        ] {
            let trigger = format!(
                "CREATE TRIGGER fail_fresh BEFORE INSERT ON {table}
                 BEGIN SELECT RAISE(ABORT, 'injected fresh turn failure'); END;"
            );
            db.conn.lock().unwrap().execute_batch(&trigger).unwrap();
            assert!(matches!(
                db.create_bound_session_and_accept_turn(
                    session,
                    "/project",
                    turn,
                    "prompt",
                    "visible input",
                    Some((&selection_key(session), "choice"))
                ),
                Err(StorageError::Sqlite(_))
            ));
            assert_eq!(fresh_turn_rows(&db, session, turn), (0, 0, 0, 0, 0));
            assert_eq!(db.get_pref(&selection_key(session)).unwrap(), None);
            db.conn
                .lock()
                .unwrap()
                .execute_batch("DROP TRIGGER fail_fresh;")
                .unwrap();
            assert_eq!(
                db.create_bound_session_and_accept_turn(
                    session,
                    "/project",
                    turn,
                    "prompt",
                    "visible input",
                    Some((&selection_key(session), "choice"))
                )
                .unwrap(),
                if session == "turn-fail" {
                    "m0001"
                } else {
                    "m0002"
                }
            );
            assert_eq!(fresh_turn_rows(&db, session, turn), (1, 1, 3, 1, 1));
            assert_eq!(
                db.get_pref(&selection_key(session)).unwrap().as_deref(),
                Some("choice")
            );
        }
    }

    #[test]
    fn fresh_turn_selection_insert_failure_rolls_back_everything_and_retries() {
        let tmp = tmp_root("fresh-selection-failure");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let key = "tui.selection.session:[\"/project\",\"provider\",\"fresh\"]";
        db.conn
            .lock()
            .unwrap()
            .execute_batch(
                "CREATE TRIGGER fail_selection BEFORE INSERT ON prefs
             WHEN NEW.key LIKE 'tui.selection.session:%'
             BEGIN SELECT RAISE(ABORT, 'injected selection failure'); END;",
            )
            .unwrap();
        assert!(matches!(
            db.create_bound_session_and_accept_turn(
                "fresh",
                "/project",
                "turn",
                "prompt",
                "user",
                Some((key, "choice"))
            ),
            Err(StorageError::Sqlite(_))
        ));
        assert_eq!(fresh_turn_rows(&db, "fresh", "turn"), (0, 0, 0, 0, 0));
        assert_eq!(db.get_pref(key).unwrap(), None);
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_selection;")
            .unwrap();
        assert_eq!(
            db.create_bound_session_and_accept_turn(
                "fresh",
                "/project",
                "turn",
                "prompt",
                "user",
                Some((key, "choice"))
            )
            .unwrap(),
            "m0001"
        );
        assert_eq!(fresh_turn_rows(&db, "fresh", "turn"), (1, 1, 3, 1, 1));
        assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("choice"));
    }

    #[test]
    fn fresh_turn_selection_key_cannot_override_binding_or_other_prefs() {
        let tmp = tmp_root("fresh-selection-key");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let unrelated = "tui.selection.variant:[\"provider\",\"model\"]";
        db.set_pref(unrelated, "original").unwrap();
        for (index, key) in [
            "tui.session_location.fresh",
            "tui.session_location.other",
            unrelated,
            "dcp.nudge.fresh",
            "tui.selection.session:[\"/project\",\"provider\",\"other\"]",
            "tui.selection.session:not-json",
        ]
        .iter()
        .enumerate()
        {
            assert!(
                db.create_bound_session_and_accept_turn(
                    "fresh",
                    "/project",
                    &format!("turn{index}"),
                    "prompt",
                    "user",
                    Some((key, "bad"))
                )
                .is_err()
            );
            assert_eq!(
                fresh_turn_rows(&db, "fresh", &format!("turn{index}")),
                (0, 0, 0, 0, 0)
            );
            assert_eq!(db.get_pref("tui.session_location.fresh").unwrap(), None);
            assert_eq!(db.get_pref("tui.session_location.other").unwrap(), None);
        }
        assert_eq!(db.get_pref(unrelated).unwrap().as_deref(), Some("original"));
        assert_eq!(db.get_pref("dcp.nudge.fresh").unwrap(), None);
    }

    #[test]
    fn fresh_turn_is_fresh_only_and_preserves_existing_accept_turn() {
        let tmp = tmp_root("fresh-turn-success");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        let key = "tui.selection.session:[\"/project\",\"provider\",\"new\"]";
        db.set_pref(key, "old choice").unwrap();
        let first = db
            .create_bound_session_and_accept_turn(
                "new",
                "/project",
                "first",
                "raw prompt",
                "rendered user text",
                Some((key, "new choice")),
            )
            .unwrap();
        assert_eq!(first, "m0001");
        assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("new choice"));
        assert_eq!(
            db.get_pref("tui.session_location.new").unwrap().as_deref(),
            Some("/project")
        );
        assert_eq!(db.session_meta("new").unwrap().parent_id, None);
        assert_eq!(
            db.read_history_full("new").unwrap(),
            vec![(first.clone(), "user".into(), "rendered user text".into())]
        );
        {
            let conn = db.conn.lock().unwrap();
            let turn: (String, String, Option<String>) = conn
                .query_row(
                    "SELECT status, prompt, result FROM turns WHERE id = 'first'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(turn, ("started".into(), "raw prompt".into(), None));
            let events: Vec<(String, String)> = conn
                .prepare("SELECT kind, payload FROM events WHERE session_id = 'new' ORDER BY seq")
                .unwrap()
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert_eq!(
                events,
                vec![
                    ("session_created".into(), "{}".into()),
                    ("turn_started".into(), "first".into()),
                    ("message".into(), first),
                ]
            );
        }
        for location in ["/project", "/other"] {
            assert!(matches!(
                db.create_bound_session_and_accept_turn(
                    "new",
                    location,
                    "duplicate",
                    "bad",
                    "bad",
                    Some((key, "bad choice"))
                ),
                Err(StorageError::SessionAlreadyExists)
            ));
        }
        assert_eq!(db.get_pref(key).unwrap().as_deref(), Some("new choice"));
        assert_eq!(fresh_turn_rows(&db, "new", "duplicate"), (1, 1, 3, 0, 1));
        db.create_session("unbound").unwrap();
        assert!(matches!(
            db.create_bound_session_and_accept_turn(
                "unbound",
                "/project",
                "duplicate",
                "bad",
                "bad",
                None
            ),
            Err(StorageError::SessionAlreadyExists)
        ));
        assert_eq!(
            fresh_turn_rows(&db, "unbound", "duplicate"),
            (1, 0, 1, 0, 0)
        );
        assert_eq!(
            db.accept_turn("next", "new", "another prompt", "next user")
                .unwrap(),
            "m0002"
        );
        assert_eq!(fresh_turn_rows(&db, "new", "next"), (1, 1, 5, 1, 2));
        assert!(matches!(
            db.accept_turn("next", "new", "bad", "bad"),
            Err(StorageError::Sqlite(_))
        ));
        assert_eq!(fresh_turn_rows(&db, "new", "next"), (1, 1, 5, 1, 2));
    }

    #[test]
    fn fresh_turn_duplicate_turn_id_cannot_strand_new_session() {
        let tmp = tmp_root("fresh-duplicate-turn");
        let db = Db::open(&tmp.path().join("data")).unwrap();
        db.create_session("existing").unwrap();
        db.accept_turn("taken", "existing", "first", "first")
            .unwrap();
        assert!(matches!(
            db.create_bound_session_and_accept_turn(
                "new", "/project", "taken", "second", "second", None
            ),
            Err(StorageError::Sqlite(_))
        ));
        assert_eq!(fresh_turn_rows(&db, "new", "taken"), (0, 0, 0, 1, 0));
        assert_eq!(
            db.read_history("existing").unwrap(),
            vec![("user".into(), "first".into())]
        );
        assert_eq!(
            db.create_bound_session_and_accept_turn(
                "new", "/project", "free", "second", "second", None
            )
            .unwrap(),
            "m0002"
        );
        assert_eq!(fresh_turn_rows(&db, "new", "free"), (1, 1, 3, 1, 1));
    }

    #[test]
    fn list_sessions_keeps_id_order_and_includes_children() {
        let tmp = tmp_root("child-list");
        let db = Db::open(&tmp.path().join("data")).expect("open");
        db.create_session("a-root").expect("a-root");
        db.create_session("z-root").expect("z-root");
        db.create_child_session("a-root", "m-child", None, None, None)
            .expect("m-child");
        db.create_child_session("a-root", "b-child", None, None, None)
            .expect("b-child");
        // Legacy behavior: every session row, no parent filter, id ascending.
        assert_eq!(
            db.list_sessions().expect("list"),
            vec!["a-root", "b-child", "m-child", "z-root"]
        );
    }
}
