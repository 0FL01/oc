//! SQLite storage worker for T04.
//!
//! Own data-root lock, WAL + `synchronous=FULL`, short transactions, bounded
//! content-addressed blobs and crash recovery. No upstream DB is ever opened
//! for writing; no migration of foreign databases happens here.
//! Main unit scenarios live in storage/tests.rs; existing storage_* parts stay separate.
//!
//! Layout under `<root>/`:
//! `oc.lock` (advisory exclusive flock, never deleted), `oc.sqlite`,
//! `oc.sqlite-wal/shm` (SQLite), `blobs/<sha256>` (content-addressed).

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::MetadataExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use fs2::FileExt as _;
use rusqlite::{Connection, OptionalExtension as _, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

#[path = "storage_children.rs"]
mod children;
#[path = "storage_compaction.rs"]
mod compaction;
#[path = "storage_conversation.rs"]
mod conversation;
#[path = "storage_credentials.rs"]
mod credentials;
#[path = "storage_effort.rs"]
mod effort;
pub use credentials::{AccountSummary, CredentialKind, CredentialMaterial, OAuthAccountMetadata};
#[path = "storage_dcp_view.rs"]
mod dcp_view;
#[path = "storage_fork.rs"]
mod fork;
#[path = "storage_grants.rs"]
mod grants;
#[path = "storage_instructions.rs"]
mod instructions;
mod prompt_history;
#[path = "storage_session_move.rs"]
mod session_move;
#[path = "storage_shell_jobs.rs"]
mod shell_jobs;
#[path = "storage_terminals.rs"]
mod terminals;
#[path = "storage_tool_output.rs"]
pub(crate) mod tool_output;
#[path = "storage_turn_history.rs"]
mod turn_history;
pub(crate) use session_move::MoveRecord;
pub(crate) use shell_jobs::AdmittedUserShell;

/// Bounded page projection retaining the exact persisted message identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPageRow {
    pub id: oc_core::session::MessageId,
    pub seq: i64,
    pub role: String,
    pub text: String,
}

// Bound newly projected identity allocations even for malformed/legacy stores.
const HISTORY_PAGE_ID_BYTES: usize = 256 * 1024;

fn page_message_id(
    row: &rusqlite::Row<'_>,
    remaining: &mut usize,
) -> rusqlite::Result<oc_core::session::MessageId> {
    let id = row.get_ref(3)?.as_str()?;
    if id.trim().is_empty() || id.len() > *remaining {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid or oversized history message identity",
            )),
        ));
    }
    *remaining -= id.len();
    Ok(oc_core::session::MessageId(id.to_owned()))
}

/// Product blob quota reference (2 GiB, see `examples/oc-rs.toml`).
pub const DEFAULT_BLOB_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Schema version applied by T04.
pub const SCHEMA_VERSION: i64 = 1;
/// Additive child-session schema version (T43): nullable `sessions` columns
/// plus a `parent_id` index. Version 2 is the DCP migration.
pub const CHILD_SESSION_SCHEMA_VERSION: i64 = 3;
/// Preference namespace for root session Location ownership.
pub(crate) const SESSION_LOCATION_PREFIX: &str = "tui.session_location.";

/// Resolve actual repository ownership, including linked worktrees, without
/// deriving a project identity from a session ID or frontend display path.
pub(crate) fn session_project_root(directory: &Path) -> Option<PathBuf> {
    let root = directory
        .ancestors()
        .find(|path| path.join(".git").exists())?;
    let git = root.join(".git");
    if git.is_dir() {
        return std::fs::canonicalize(root).ok();
    }
    if std::fs::metadata(&git).ok()?.len() > 4096 {
        return None;
    }
    let text = std::fs::read_to_string(git).ok()?;
    let dir = root.join(text.trim().strip_prefix("gitdir: ")?);
    let common = dir.join("commondir");
    if std::fs::metadata(&common).ok()?.len() > 4096 {
        return None;
    }
    let common =
        std::fs::canonicalize(dir.join(std::fs::read_to_string(common).ok()?.trim())).ok()?;
    std::fs::canonicalize(common.parent()?).ok()
}
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
    /// Invalid account metadata or material (never includes the rejected input).
    #[error("invalid credential")]
    InvalidCredential,
    /// Account does not exist in the requested namespace.
    #[error("credential account not found")]
    CredentialNotFound,
    /// Credential persistence failed; SQLite diagnostics may contain material.
    #[error("credential storage failure")]
    CredentialStorage,
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
    conn: Arc<Mutex<Connection>>,
    output_dir: Arc<File>,
    output_readers: Arc<Mutex<std::collections::HashMap<String, usize>>>,
    // Actual payload transfers: HOT, explicit RAW page, bounded UI window.
    history_reads: Arc<[std::sync::atomic::AtomicU64; 10]>,
    public_catalog: Arc<std::sync::OnceLock<Arc<crate::models_dev::GoCatalog>>>,
    // One built-in OpenAI refresh flight across every same-root shared handle.
    pub(crate) credential_refresh: Arc<tokio::sync::Mutex<()>>,
    // Ephemeral authorization fences; live attempts never survive root reopen.
    credential_epochs: Arc<Mutex<std::collections::BTreeMap<String, u64>>>,
    pub(crate) response_channels: crate::provider::websocket::Channels,
    // Fields drop in declaration order: release ownership after SQLite closes.
    _lock: Arc<RootLock>,
}

/// Exact result of the committed acceptance transaction, without a second
/// history query that could observe a later turn or silently fail.
pub(crate) struct AcceptedTurn {
    pub user_message: String,
    pub model_switch: Option<oc_core::queries::ModelSwitchNotice>,
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
    pub output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
    pub question: Option<oc_core::question::QuestionResult>,
    /// Canonical parsed compression arguments, independent of raw UI input.
    pub dcp_topic: Option<String>,
    /// Frozen DCP commit; absent for other tools and legacy operations.
    pub dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
    /// Bounded confirmed effects, independently of the legacy text preview.
    pub patch_effects: Option<oc_core::patch::PatchEffects>,
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

/// Optional read outcome and its UI facts share the instructions transaction.
pub(crate) struct RecordedToolOutcome<'a> {
    pub operation: &'a str,
    pub state: &'a str,
    pub output: &'a str,
    pub presentation: Option<&'a oc_core::tool_output::Presentation>,
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
pub const TOOL_OP_PREVIEW_BYTES: usize = oc_core::tool_output::PREVIEW_BYTES;
/// Active-history page size for bounded projection reads.
pub const ACTIVE_HISTORY_PAGE: usize = 256;

/// Durable compression block row with ordered membership (T17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionBlockRow {
    /// Optional versioned native working selection.
    pub hot: Option<String>,
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
/// Immutable accounting identity: durable turn, provider call, local occurrence.
pub(crate) type DcpCallIdentity = (String, String, u64);

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
    pub(crate) measurement: &'a crate::dcp::DcpMeasurement,
    pub(crate) session: &'a str,
    pub(crate) blocks: &'a [CompressionBlockRow],
    pub(crate) consumed_blocks: &'a [String],
    pub(crate) expected_next: u64,
    pub(crate) expected_revision: i64,
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
        secure_sqlite_file(&db_path, true)?;
        secure_sqlite_file(&root.join("oc.sqlite-wal"), false)?;
        secure_sqlite_file(&root.join("oc.sqlite-shm"), false)?;
        let conn = Connection::open(&db_path)?;
        conn.create_scalar_function(
            "oc_session_lower",
            1,
            rusqlite::functions::FunctionFlags::SQLITE_UTF8
                | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
            |context| Ok(context.get::<String>(0)?.to_lowercase()),
        )?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        apply_schema(&conn)?;
        Self::compaction_schema(&conn)?;
        Self::renewal_schema(&conn)?;
        Self::conversation_schema(&conn)?;
        Self::session_list_schema(&conn)?;
        Self::shell_jobs_schema(&conn)?;
        Self::child_jobs_schema(&conn)?;
        Self::session_move_schema(&conn)?;
        Self::tool_output_schema(&conn)?;
        Self::apply_turn_history_schema(&conn)?;
        Self::credentials_schema(&conn)?;
        Self::terminals_schema(&conn)?;
        let output_path = root.join("tool-output");
        match fs::create_dir(&output_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let output_dir = tool_output::open_directory(&output_path)?;
        if output_dir.metadata()?.uid() != fs::metadata(&root)?.uid() {
            return Err(StorageError::UnsafeRoot(
                "tool output directory owner".into(),
            ));
        }
        output_dir.set_permissions(fs::Permissions::from_mode(0o700))?;
        // Same journal, indexed anchor lookup: history paging must not parse
        // every archived turn. Legacy non-JSON results are excluded safely.
        conn.execute_batch("CREATE INDEX IF NOT EXISTS turns_display_anchor ON turns(session_id, COALESCE(json_extract(result,'$.assistant_message'),json_extract(result,'$.user_message'))) WHERE json_valid(result)")?;

        let db = Self {
            root,
            blob_dir,
            quota_bytes,
            conn: Arc::new(Mutex::new(conn)),
            output_dir: Arc::new(output_dir),
            output_readers: Arc::new(Mutex::new(std::collections::HashMap::new())),
            history_reads: Arc::new(std::array::from_fn(|_| {
                std::sync::atomic::AtomicU64::new(0)
            })),
            public_catalog: Arc::new(std::sync::OnceLock::new()),
            credential_refresh: Arc::new(tokio::sync::Mutex::new(())),
            credential_epochs: Arc::new(Mutex::new(std::collections::BTreeMap::new())),
            response_channels: Default::default(),
            _lock: Arc::new(lock),
        };
        db.expire_tool_outputs(tool_output::timestamp())?;
        Ok(db)
    }

    // Completion workers retain this same connection and flock. This cannot open
    // a second database or release ownership while a process is still supervised.
    pub(crate) fn shared_handle(&self) -> Self {
        Self {
            root: self.root.clone(),
            blob_dir: self.blob_dir.clone(),
            quota_bytes: self.quota_bytes,
            conn: self.conn.clone(),
            output_dir: self.output_dir.clone(),
            output_readers: self.output_readers.clone(),
            history_reads: self.history_reads.clone(),
            public_catalog: self.public_catalog.clone(),
            credential_refresh: self.credential_refresh.clone(),
            credential_epochs: self.credential_epochs.clone(),
            response_channels: self.response_channels.clone(),
            _lock: self._lock.clone(),
        }
    }

    /// Data-root path (owned).
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn public_catalog(&self) -> Arc<crate::models_dev::GoCatalog> {
        self.public_catalog
            .get_or_init(|| Arc::new(crate::models_dev::GoCatalog::open(self)))
            .clone()
    }

    /// Create a root session; duplicate ids fail.
    pub fn create_session(&self, id: &str) -> Result<(), StorageError> {
        self.create_root_session(id, None, None)?;
        Ok(())
    }

    /// Atomically create a root, its event, and its Location binding.
    /// Existing bindings are idempotent only for the same Location; a root
    /// without a binding remains a duplicate rather than being claimed.
    #[cfg(test)]
    pub(crate) fn create_bound_session(
        &self,
        id: &str,
        location: &str,
    ) -> Result<BoundSessionCreation, StorageError> {
        self.create_root_session(id, Some(location), None)
    }

    pub(crate) fn create_bound_session_with_reminder(
        &self,
        id: &str,
        location: &str,
        reminder: Option<&str>,
    ) -> Result<BoundSessionCreation, StorageError> {
        self.create_root_session(id, Some(location), reminder)
    }

    /// Fresh-session durable acceptance: root, creation event, Location
    /// binding, optional prevalidated session selection, started turn/event
    /// and user message/event commit together.
    /// Unlike `create_bound_session`, an existing root (even one bound to this
    /// Location) is always a duplicate; no existing history is modified.
    #[allow(clippy::too_many_arguments)]
    #[cfg(test)]
    pub(crate) fn create_bound_session_and_accept_turn(
        &self,
        id: &str,
        location: &str,
        turn: &str,
        prompt: &str,
        user_text: &str,
        initial_selection: Option<(&str, &str)>,
        model: &oc_core::queries::ModelRef,
    ) -> Result<AcceptedTurn, StorageError> {
        self.create_bound_session_and_accept_turn_with_reminder(
            id,
            location,
            turn,
            prompt,
            user_text,
            initial_selection,
            model,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_bound_session_and_accept_turn_with_reminder(
        &self,
        id: &str,
        location: &str,
        turn: &str,
        prompt: &str,
        user_text: &str,
        initial_selection: Option<(&str, &str)>,
        model: &oc_core::queries::ModelRef,
        reminder: Option<&str>,
        history_input: Option<&str>,
    ) -> Result<AcceptedTurn, StorageError> {
        let marker = Self::fresh_root_marker(id, location, initial_selection)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::check_fresh_root_deck(&tx, location)?;
        Self::insert_root_session(&tx, id)?;
        Self::insert_location_binding(&tx, id, location)?;
        let accepted = Self::insert_accepted_turn(
            &tx,
            turn,
            id,
            prompt,
            user_text,
            model,
            reminder,
            history_input,
        )?;
        if let Some((key, value)) = initial_selection {
            Self::upsert_pref(&tx, key, value)?;
        }
        tx.execute(
            "INSERT INTO prefs(key, value, updated_at) VALUES (?1, ?2, ?3)",
            params![marker, TAB_ADOPTION_VALUE, now_rfc3339()],
        )?;
        tx.commit()?;
        Ok(accepted)
    }

    // Normal prompts and explicit user Shell admission create the same bounded
    // root/deck owner; neither may orphan a root when its admission rolls back.
    fn fresh_root_marker(
        id: &str,
        location: &str,
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
        Ok(marker)
    }

    fn check_fresh_root_deck(conn: &Connection, location: &str) -> Result<(), StorageError> {
        // Admission and the root write must see the same scoped deck and
        // marker set. Reject before any root/turn/event insert.
        let stored =
            match Self::get_pref_bounded_in(conn, &tab_deck_key(location), MAX_TAB_DECK_BYTES)? {
                BoundedPref::Missing => parse_stored_deck(None)?,
                BoundedPref::TooLarge => return Err(invalid_stored_tab_deck()),
                BoundedPref::Value(raw) => parse_stored_deck(Some(&raw))?,
            };
        let mut occupied: HashSet<String> = stored.sessions.into_iter().collect();
        occupied.extend(Self::tab_adoptions_in(conn, location)?);
        if occupied.len() >= MAX_TABS {
            return Err(invalid_stored_tab_deck());
        }
        Ok(())
    }

    fn create_root_session(
        &self,
        id: &str,
        location: Option<&str>,
        reminder: Option<&str>,
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
        if let Some(text) = reminder {
            Self::insert_message(&tx, id, "system", text)?;
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
            "SELECT role, text FROM messages WHERE session_id = ?1 AND role != 'model_switch' ORDER BY seq ASC",
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

    fn session_list_schema(conn: &Connection) -> Result<(), StorageError> {
        let has_updated = conn
            .prepare("PRAGMA table_info(sessions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == "updated_at");
        if !has_updated {
            // No truthful last-update fact exists for old rows. Do not backfill
            // creation time, migration time, IDs or journal insertion order.
            conn.execute_batch("ALTER TABLE sessions ADD COLUMN updated_at TEXT")?;
        }
        conn.execute_batch("BEGIN;
            CREATE INDEX IF NOT EXISTS sessions_picker_updated ON sessions(parent_id, updated_at DESC, id);
            DROP TRIGGER IF EXISTS sessions_picker_created;
            DROP TRIGGER IF EXISTS sessions_picker_title;
            DROP TRIGGER IF EXISTS sessions_picker_event;
            CREATE TRIGGER sessions_picker_created AFTER INSERT ON sessions BEGIN
                UPDATE sessions SET updated_at = strftime('%s','now') || substr(strftime('%f','now'),3) WHERE id = NEW.id;
            END;
            CREATE TRIGGER sessions_picker_title AFTER UPDATE OF title ON sessions BEGIN
                UPDATE sessions SET updated_at = strftime('%s','now') || substr(strftime('%f','now'),3) WHERE id = NEW.id;
            END;
            CREATE TRIGGER sessions_picker_event AFTER INSERT ON events BEGIN
                UPDATE sessions SET updated_at = strftime('%s','now') || substr(strftime('%f','now'),3) WHERE id = NEW.session_id;
            END; COMMIT;")?;
        Ok(())
    }

    /// At most 50 roots, ordered by observed update time; query before LIMIT.
    pub fn session_list(
        &self,
        search: &str,
        location: Option<&str>,
    ) -> Result<Vec<oc_core::queries::SessionListEntry>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached("SELECT s.id,
            substr(COALESCE(s.title, CASE WHEN s.created_at != '' AND s.created_at NOT GLOB '*[^0-9]*'
                THEN 'New session - ' || strftime('%Y-%m-%dT%H:%M:%fZ', CAST(s.created_at AS INTEGER), 'unixepoch')
                ELSE 'New session — creation time unavailable' END),1,512) AS label,
            p.value, s.created_at, s.updated_at,
            CASE WHEN s.updated_at IS NULL OR s.updated_at = '' OR s.updated_at GLOB '*[^0-9.]*' THEN 'Update time unavailable'
                 WHEN date(CAST(s.updated_at AS INTEGER),'unixepoch','localtime') = date('now','localtime') THEN 'Today'
                 ELSE substr('SunMonTueWedThuFriSat', CAST(strftime('%w',CAST(s.updated_at AS INTEGER),'unixepoch','localtime') AS INTEGER)*3+1,3) || ' ' ||
                      substr('JanFebMarAprMayJunJulAugSepOctNovDec', (CAST(strftime('%m',CAST(s.updated_at AS INTEGER),'unixepoch','localtime') AS INTEGER)-1)*3+1,3) ||
                      strftime(' %d %Y',CAST(s.updated_at AS INTEGER),'unixepoch','localtime') END,
            EXISTS(WITH RECURSIVE family(id) AS (SELECT s.id UNION SELECT c.id FROM sessions c JOIN family f ON c.parent_id=f.id)
                SELECT 1 FROM turns t JOIN family f ON t.session_id=f.id WHERE t.status = 'started')
            FROM sessions s LEFT JOIN prefs p ON p.key = 'tui.session_location.' || s.id
            WHERE s.parent_id IS NULL AND (?1 IS NULL OR p.value = ?1)
              AND instr(oc_session_lower(label), oc_session_lower(?2)) > 0
            ORDER BY s.updated_at IS NULL, CAST(s.updated_at AS REAL) DESC,
                COALESCE((SELECT MAX(e.seq) FROM events e WHERE e.session_id=s.id),0) DESC, s.id LIMIT 50")?;
        let rows = stmt.query_map(params![location, search.trim()], |row| {
            Ok(oc_core::queries::SessionListEntry {
                id: oc_core::domain::SessionId(row.get(0)?),
                title: row.get(1)?,
                directory: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                date_group: row.get(5)?,
                running: row.get(6)?,
                worktree: None,
            })
        })?;
        let mut entries = rows.collect::<Result<Vec<_>, _>>()?;
        for entry in &mut entries {
            if let Some(directory) = &entry.directory {
                let directory = Path::new(directory);
                if let Some(canonical) = session_project_root(directory)
                    && let Some(root) = directory
                        .ancestors()
                        .find(|path| path.join(".git").exists())
                    && !root.starts_with(&canonical)
                {
                    entry.worktree = root
                        .file_name()
                        .map(|name| name.to_string_lossy().chars().take(25).collect());
                }
            }
        }
        Ok(entries)
    }

    /// A real active turn on any parent_id descendant marks its root running.
    pub fn session_family_running(&self, session: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Ok(conn.query_row("WITH RECURSIVE family(id) AS (SELECT id FROM sessions WHERE id=?1 UNION SELECT c.id FROM sessions c JOIN family f ON c.parent_id=f.id)
            SELECT EXISTS(SELECT 1 FROM turns t JOIN family f ON t.session_id=f.id WHERE t.status='started')", [session], |r| r.get(0))?)
    }

    /// Explicit deletion is the sole archive-removal operation. Descend only
    /// parent_id edges: fork provenance never implies family membership.
    pub fn delete_root_family(
        &self,
        session: &str,
        location: &str,
    ) -> Result<oc_core::queries::TabDeckSnapshot, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id WHERE s.id=?1 AND s.parent_id IS NULL AND p.value=?2)", params![session,location], |r| r.get(0))?;
        if !valid {
            return Err(StorageError::SessionNotFound);
        }
        tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS deleting_family(id TEXT PRIMARY KEY); DELETE FROM deleting_family;")?;
        tx.execute("INSERT INTO deleting_family WITH RECURSIVE family(id) AS (SELECT ?1 UNION SELECT s.id FROM sessions s JOIN family f ON s.parent_id=f.id) SELECT id FROM family", [session])?;
        let busy: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE session_id IN deleting_family AND status='started') OR EXISTS(SELECT 1 FROM tool_output_resources WHERE session_id IN deleting_family AND state='Active') OR EXISTS(SELECT 1 FROM shell_jobs j WHERE session_id IN deleting_family AND (phase!='terminal' OR (message_id IS NULL AND (NOT EXISTS(SELECT 1 FROM events e WHERE e.kind='shell_foreground' AND e.payload=j.operation_id) OR EXISTS(SELECT 1 FROM events e WHERE e.kind='shell_background' AND e.payload=j.operation_id)))))", [], |r| r.get(0))?;
        let busy = busy || tx.query_row("SELECT EXISTS(SELECT 1 FROM child_jobs WHERE parent_id IN deleting_family AND (state IN ('admitted','running') OR message_id IS NULL))",[],|r|r.get::<_,bool>(0))?;
        // Do not cascade away the recovery identity of a still-owned PTY.
        // Hiding/navigating is not cancellation; remove terminals explicitly.
        let busy = busy || tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM terminals WHERE session_id IN deleting_family AND live=1)",
            [],
            |r| r.get::<_, bool>(0),
        )?;
        if busy {
            return Err(StorageError::Io(std::io::Error::other(
                "session family active",
            )));
        }
        let key = tab_deck_key(location);
        let raw = match Self::get_pref_bounded_in(&tx, &key, MAX_TAB_DECK_BYTES)? {
            BoundedPref::Missing => None,
            BoundedPref::Value(raw) => Some(raw),
            BoundedPref::TooLarge => return Err(invalid_stored_tab_deck()),
        };
        let mut deck = parse_stored_deck(raw.as_deref())?;
        let pending = Self::tab_adoptions_in(&tx, location)?;
        for id in &pending {
            if !deck.sessions.contains(id) {
                deck.sessions.push(id.clone());
            }
            deck.active = Some(id.clone());
        }
        if deck.sessions.len() > MAX_TABS
            || (deck.active.is_none() && deck.sessions.len() == MAX_TABS)
            || deck
                .active
                .as_ref()
                .is_some_and(|id| !deck.sessions.contains(id))
        {
            return Err(invalid_stored_tab_deck());
        }
        let mut seen = HashSet::new();
        for id in &deck.sessions {
            let owned: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id WHERE s.id=?1 AND s.parent_id IS NULL AND p.value=?2)", params![id,location], |r|r.get(0))?;
            if !valid_tab_id(id) || !seen.insert(id) || !owned {
                return Err(invalid_stored_tab_deck());
            }
        }
        if let Some(index) = deck.sessions.iter().position(|id| id == session) {
            deck.sessions.remove(index);
            if deck.active.as_deref() == Some(session) {
                deck.active = deck.sessions.get(index.saturating_sub(1)).cloned();
            }
        }
        let encoded = serde_json::to_string(&deck).map_err(|_| invalid_stored_tab_deck())?;
        let accepted = oc_core::queries::TabDeckSnapshot {
            location: location.into(),
            revision: Some(pref_revision(&encoded)),
            sessions: deck
                .sessions
                .into_iter()
                .map(oc_core::domain::SessionId)
                .collect(),
            active: deck.active.map(oc_core::domain::SessionId),
        };
        tx.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,strftime('%s','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at", params![key,encoded])?;
        for id in pending {
            tx.execute(
                "DELETE FROM prefs WHERE key=?1",
                [tab_adoption_key(location, &id)],
            )?;
        }
        // Child tables before their foreign-key parents; optional DCP tables
        // exist only after first DCP use. Shared blobs/context objects remain.
        for table in [
            "dcp_accounting",
            "dcp_run_views",
            "dcp_coverage",
            "dcp_run_identity",
            "compression_members",
            "conversation_points",
            "session_checkpoint",
            "session_usage_anchor",
            "session_compactions",
            "conversation_redo",
            "conversation_exclusions",
            "conversation_state",
            "turn_acceptances",
            "tool_output_resources",
            "shell_jobs",
            "child_jobs",
            "tool_operations",
            "events",
            "messages",
            "turns",
            "dcp_tool_projection",
            "dcp_tool_projection_v2",
            "prune_marks",
            "compression_blocks",
            "conversation_versions",
        ] {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )?;
            if exists {
                let predicate = if table == "compression_members" {
                    "block_id IN (SELECT id FROM compression_blocks WHERE session_id IN deleting_family)"
                } else if table == "child_jobs" {
                    "parent_id IN deleting_family"
                } else {
                    "session_id IN deleting_family"
                };
                tx.execute(&format!("DELETE FROM {table} WHERE {predicate}"), [])?;
            }
        }
        tx.execute("DELETE FROM prefs WHERE key IN (SELECT 'tui.session_location.'||id FROM deleting_family) OR key IN (SELECT 'cache.lineage.'||id FROM deleting_family) OR key IN (SELECT 'dcp.projection_owned.'||id FROM deleting_family) OR key IN (SELECT ?1||json_array(?2,id) FROM deleting_family) OR (substr(key,1,22)='tui.selection.session:' AND json_valid(substr(key,23)) AND json_extract(substr(key,23),'$[2]') IN deleting_family) OR EXISTS(SELECT 1 FROM deleting_family f WHERE substr(CAST(key AS BLOB),1,length(CAST('dcp.nudge.'||f.id||char(0) AS BLOB)))=CAST('dcp.nudge.'||f.id||char(0) AS BLOB))", params![TAB_ADOPTION_PREFIX,location])?;
        tx.execute("DELETE FROM sessions WHERE id IN deleting_family", [])?;
        tx.execute_batch("DELETE FROM deleting_family;")?;
        tx.commit()?;
        Ok(accepted)
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

    /// Cache lineage is not affinity or routing. Forks copy the original root
    /// token transactionally; a child has its own session lineage and affinity.
    pub(crate) fn cache_lineage(&self, session: &str) -> Result<String, StorageError> {
        Self::cache_lineage_in(&self.conn.lock().expect("db mutex"), session)
    }

    pub(crate) fn cache_lineage_in(
        conn: &Connection,
        session: &str,
    ) -> Result<String, StorageError> {
        Self::require_session(conn, session)?;
        match Self::get_pref_bounded_in(conn, &format!("cache.lineage.{session}"), 4096)? {
            BoundedPref::Missing => Ok(session.to_owned()),
            BoundedPref::Value(value)
                if !value.is_empty() && !value.chars().any(char::is_control) =>
            {
                Ok(value)
            }
            _ => Err(StorageError::SessionNotFound),
        }
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
                "SELECT COUNT(*) FROM conversation_messages WHERE session_id = ?1",
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
        Ok(self
            .read_history_page_typed(session, limit, before_seq)?
            .into_iter()
            .map(|row| (row.seq, row.role, row.text))
            .collect())
    }

    /// Exact current-branch lookup, with the same durable-ID bound as paging.
    pub(crate) fn read_history_message_typed(
        &self,
        session: &str,
        message: &oc_core::session::MessageId,
    ) -> Result<Option<HistoryPageRow>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut remaining = HISTORY_PAGE_ID_BYTES;
        conn.query_row(
            "SELECT seq,role,text,id FROM conversation_messages WHERE session_id=?1 AND id=?2",
            params![session, message.0],
            |row| {
                Ok(HistoryPageRow {
                    id: page_message_id(row, &mut remaining)?,
                    seq: row.get(0)?,
                    role: row.get(1)?,
                    text: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Newest-first bounded page with exact durable IDs.
    pub fn read_history_page_typed(
        &self,
        session: &str,
        limit: usize,
        before_seq: Option<i64>,
    ) -> Result<Vec<HistoryPageRow>, StorageError> {
        let limit = (limit.min(HISTORY_PAGE_MAX) as i64).max(0);
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT seq, role, text, id FROM conversation_messages
             WHERE session_id = ?1 AND (?2 IS NULL OR seq < ?2)
             ORDER BY seq DESC LIMIT ?3",
        )?;
        let mut remaining = HISTORY_PAGE_ID_BYTES;
        let rows = stmt.query_map(params![session, before_seq, limit], |row| {
            let id = page_message_id(row, &mut remaining)?;
            let seq: i64 = row.get(0)?;
            let role: String = row.get(1)?;
            let text: String = row.get(2)?;
            Ok(HistoryPageRow {
                id,
                seq,
                role,
                text,
            })
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
        Ok(self
            .read_history_after_typed(session, limit, after_seq)?
            .into_iter()
            .map(|row| (row.seq, row.role, row.text))
            .collect())
    }

    /// Oldest-first bounded page with exact durable IDs.
    pub fn read_history_after_typed(
        &self,
        session: &str,
        limit: usize,
        after_seq: i64,
    ) -> Result<Vec<HistoryPageRow>, StorageError> {
        let limit = (limit.min(HISTORY_PAGE_MAX) as i64).max(0);
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached(
            "SELECT seq, role, text, id FROM conversation_messages
                   WHERE session_id = ?1 AND seq > ?2
             ORDER BY seq ASC LIMIT ?3",
        )?;
        let mut remaining = HISTORY_PAGE_ID_BYTES;
        let rows = stmt.query_map(params![session, after_seq, limit], |row| {
            Ok(HistoryPageRow {
                id: page_message_id(row, &mut remaining)?,
                seq: row.get(0)?,
                role: row.get(1)?,
                text: row.get(2)?,
            })
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

    /// Public notice adjacent to the last accepted user row, if present.
    /// Called only after the acceptance transaction commits.
    #[cfg(test)]
    pub(crate) fn latest_model_switch(
        &self,
        session: &str,
    ) -> Result<Option<oc_core::queries::ModelSwitchNotice>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw: Option<String> = conn
            .query_row(
                "SELECT n.text FROM messages u JOIN messages n ON n.session_id=u.session_id
             AND n.seq=(SELECT MAX(seq) FROM messages WHERE session_id=u.session_id AND seq<u.seq)
             WHERE u.session_id=?1 AND u.role='user' AND n.role='model_switch'
               AND u.seq=(SELECT MAX(seq) FROM messages WHERE session_id=?1)
             LIMIT 1",
                [session],
                |row| row.get(0),
            )
            .optional()?;
        raw.map(|text| {
            serde_json::from_str(&text).map_err(|_| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid model switch notice",
                ))
            })
        })
        .transpose()
    }

    /// Committed message seq bounds `(min, max)`; `None` for an empty session.
    pub fn history_bounds(
        &self,
        session: &str,
    ) -> Result<(Option<i64>, Option<i64>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let bounds: Option<(Option<i64>, Option<i64>)> = conn
            .query_row(
                "SELECT MIN(seq), MAX(seq) FROM conversation_messages WHERE session_id = ?1",
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
                    length(CAST(output AS BLOB)), archive_rowid,
                    (SELECT metadata FROM patch_effects WHERE op_id=conversation_tools.id),
                    CASE WHEN name='question' THEN COALESCE((SELECT COALESCE(json_extract(e.payload,'$.result'),json_extract(e.payload,'$.question_resource')) FROM events e WHERE e.session_id=conversation_tools.session_id AND e.kind='tool_output_question' AND json_extract(e.payload,'$.operation')=conversation_tools.id LIMIT 1),substr(output,1,131136)) ELSE NULL END
               FROM conversation_tools
              WHERE session_id = ?1 AND (?2 IS NULL OR archive_rowid < ?2)
              ORDER BY archive_rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            params![session, before_rowid, limit, TOOL_OP_PREVIEW_BYTES as i64],
            |row| {
                let raw: Option<String> = row.get(5)?;
                let bytes: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
                let (output, truncated) = bound_preview(raw, bytes);
                let dcp = Self::dcp_run_in(&conn, session, &row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                let name: String = row.get(2)?;
                let input: Option<String> = row.get(4)?;
                let dcp_topic = if name == "compress" {
                    dcp.as_ref()
                        .map(|run| run.topic.clone())
                        .or_else(|| crate::dcp::presentation_topic(&name, input.as_deref()))
                } else {
                    None
                };
                Ok(ToolOpRow {
                    output_presentation: Self::tool_presentation_in(
                        &conn,
                        session,
                        &row.get::<_, String>(0)?,
                    )
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    question: self.question_presentation_in(
                        &conn,
                        &name,
                        &row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(9)?.as_deref(),
                    ),
                    dcp_topic,
                    dcp,
                    patch_effects: decode_patch_effects(row.get(8)?),
                    op: row.get(0)?,
                    turn: row.get(1)?,
                    name,
                    state: row.get(3)?,
                    input,
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
        if let Some(resource) = self.output_for_history(op)? {
            return self
                .open_tool_output(&resource.session, &resource.path)?
                .byte_page(offset, limit);
        }
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
        drop(conn);
        if let Some(resource) = self.output_for_history(op)? {
            return self
                .open_tool_output(session, &resource.path)?
                .byte_page(offset, limit.clamp(4, TOOL_OP_PREVIEW_BYTES));
        }
        let conn = self.conn.lock().expect("db mutex");
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
                "SELECT MIN(archive_rowid), MAX(archive_rowid) FROM conversation_tools WHERE session_id = ?1",
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
                "SELECT COUNT(*) FROM conversation_tools WHERE session_id = ?1",
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
                    length(CAST(output AS BLOB)), rowid,
                    (SELECT metadata FROM patch_effects WHERE op_id=tool_operations.id),
                    CASE WHEN name='question' THEN COALESCE((SELECT COALESCE(json_extract(e.payload,'$.result'),json_extract(e.payload,'$.question_resource')) FROM events e WHERE e.session_id=tool_operations.session_id AND e.kind='tool_output_question' AND json_extract(e.payload,'$.operation')=tool_operations.id LIMIT 1),substr(output,1,131136)) ELSE NULL END
               FROM tool_operations
              WHERE session_id = ?1 ORDER BY rowid ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![session, TOOL_OPS_MAX as i64, TOOL_OP_PREVIEW_BYTES as i64],
            |row| {
                let raw: Option<String> = row.get(5)?;
                let bytes: i64 = row.get::<_, Option<i64>>(6)?.unwrap_or(0);
                let (output, truncated) = bound_preview(raw, bytes);
                let dcp = Self::dcp_run_in(&conn, session, &row.get::<_, String>(0)?)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                let name: String = row.get(2)?;
                let input: Option<String> = row.get(4)?;
                let dcp_topic = if name == "compress" {
                    dcp.as_ref()
                        .map(|run| run.topic.clone())
                        .or_else(|| crate::dcp::presentation_topic(&name, input.as_deref()))
                } else {
                    None
                };
                Ok(ToolOpRow {
                    output_presentation: Self::tool_presentation_in(
                        &conn,
                        session,
                        &row.get::<_, String>(0)?,
                    )
                    .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    question: self.question_presentation_in(
                        &conn,
                        &name,
                        &row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(9)?.as_deref(),
                    ),
                    dcp_topic,
                    dcp,
                    patch_effects: decode_patch_effects(row.get(8)?),
                    op: row.get(0)?,
                    turn: row.get(1)?,
                    name,
                    state: row.get(3)?,
                    input,
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
            "SELECT b.id,(SELECT MIN(m.seq) FROM conversation_messages m JOIN messages a ON a.id=b.start_msg JOIN messages z ON z.id=b.end_msg WHERE m.session_id=b.session_id AND m.seq>?2 AND m.seq BETWEEN a.seq AND z.seq) FROM compression_blocks b WHERE b.session_id=?1 AND b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 AND json_extract(b.hot,'$.standalone')=1 AND EXISTS(SELECT 1 FROM conversation_messages m WHERE m.id=b.end_msg AND m.seq>?2)
             UNION ALL SELECT cm.block_id, MIN(m.seq) FROM compression_members cm
               JOIN messages m ON m.id = cm.message_id
               JOIN compression_blocks b ON b.id = cm.block_id
               WHERE b.session_id = ?1 AND m.seq > ?2 AND (b.hot IS NULL OR json_extract(b.hot,'$.active')=1) AND COALESCE(json_extract(b.hot,'$.standalone'),0)=0
               GROUP BY cm.block_id ORDER BY 2 ASC",
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
        self.active_history_range(session, after_seq, i64::MAX, budget)
    }
    pub(crate) fn active_history_range(
        &self,
        session: &str,
        after_seq: i64,
        until: i64,
        budget: usize,
    ) -> Result<ActiveHistory, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut rows: Vec<(String, String, String)> = Vec::new();
        let mut bytes: u64 = 0;
        let mut cursor = after_seq;
        let intervals = Self::active_intervals_in(&conn, session, after_seq)?;
        let mut rows_read = 0usize;
        loop {
            if let Some((_, end)) = intervals
                .iter()
                .find(|(start, end)| cursor >= start.saturating_sub(1) && cursor < *end)
            {
                cursor = *end;
                continue;
            }
            let gap_end = intervals
                .iter()
                .filter(|(start, _)| *start > cursor)
                .map(|(start, _)| *start)
                .min()
                .unwrap_or(i64::MAX)
                .min(until.saturating_add(1));
            if cursor >= until {
                break;
            }
            let mut stmt = conn.prepare_cached(
                "SELECT id, role, seq, length(CAST(text AS BLOB)), CASE WHEN SUM(length(CAST(text AS BLOB))) OVER(ORDER BY seq)<=?4 THEN text ELSE '' END FROM conversation_messages
                       WHERE session_id = ?1 AND seq > ?2 AND seq<?5 AND role != 'model_switch'
                     AND NOT EXISTS (SELECT 1 FROM compression_members cm JOIN compression_blocks cb ON cb.id=cm.block_id
                                      WHERE cm.message_id = conversation_messages.id AND (cb.hot IS NULL OR json_extract(cb.hot,'$.active')=1))
                     AND NOT EXISTS(SELECT 1 FROM compression_blocks cb JOIN messages a ON a.id=cb.start_msg JOIN messages z ON z.id=cb.end_msg WHERE cb.session_id=conversation_messages.session_id AND cb.hot IS NOT NULL AND json_valid(cb.hot) AND json_extract(cb.hot,'$.active')=1 AND json_extract(cb.hot,'$.standalone')=1 AND conversation_messages.seq BETWEEN a.seq AND z.seq)
                  ORDER BY seq ASC LIMIT ?3",
            )?;
            let page: Vec<(String, String, i64, i64, String)> = stmt
                .query_map(
                    params![
                        session,
                        cursor,
                        ACTIVE_HISTORY_PAGE as i64,
                        budget.saturating_sub(bytes.min(usize::MAX as u64) as usize) as i64,
                        gap_end
                    ],
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
                if gap_end != i64::MAX && gap_end <= until {
                    cursor = gap_end - 1;
                    continue;
                }
                if rows.is_empty() && cursor == after_seq {
                    Self::require_session(&conn, session)?;
                }
                break;
            }
            let full_page = page.len() == ACTIVE_HISTORY_PAGE;
            for (id, role, seq, size, text) in page {
                cursor = seq;
                bytes = bytes.saturating_add(size.max(0) as u64);
                rows_read += 1;
                if bytes <= budget as u64 {
                    rows.push((id, role, text));
                } else {
                    rows.clear();
                }
            }
            if !full_page {
                if gap_end != i64::MAX && gap_end <= until {
                    cursor = gap_end - 1;
                    continue;
                } else {
                    break;
                }
            }
        }
        Ok(ActiveHistory {
            rows_read,
            bytes,
            rows,
            overflow: bytes > budget as u64,
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
    ) -> Result<Vec<(String, Option<String>)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let page = (page.min(TOOL_OPS_MAX) as i64).max(1);
        let mut out = Vec::new();
        let mut upper: Option<i64> = None;
        loop {
            let mut stmt = conn.prepare_cached(
                "SELECT archive_rowid, result, prompt FROM conversation_turns
                  WHERE session_id = ?1 AND result IS NOT NULL
                    AND (?2 IS NULL OR archive_rowid < ?2)
                  ORDER BY archive_rowid DESC LIMIT ?3",
            )?;
            let batch: Vec<(i64, String, Option<String>)> = stmt
                .query_map(params![session, upper, page], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let full_page = batch.len() == page as usize;
            if let Some((rowid, _, _)) = batch.last() {
                upper = Some(*rowid);
            }
            let mut oldest = i64::MAX;
            for (_, result, prompt) in batch {
                match anchor_seq(&result) {
                    Some(seq) if seq > floor_seq => out.push((result, prompt)),
                    Some(seq) => oldest = oldest.min(seq),
                    None => out.push((result, prompt)),
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
            "SELECT id, role, text FROM messages WHERE session_id = ?1 AND role != 'model_switch' ORDER BY seq ASC",
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

    /// Active conversation archive for explicit owner actions; raw archival
    /// callers retain `read_history_full`.
    pub(crate) fn conversation_history_full(
        &self,
        session: &str,
    ) -> Result<Vec<(String, String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut stmt = conn.prepare_cached("SELECT id,role,text FROM conversation_messages WHERE session_id=?1 AND role!='model_switch' ORDER BY seq")?;
        Ok(stmt
            .query_map([session], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?)
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

    /// CAS-save both picker Location decks and retire included adoptions in
    /// one transaction, without changing or copying any conversation records.
    pub(crate) fn save_picker_decks(
        &self,
        decks: &mut [oc_core::queries::TabDeckSnapshot],
    ) -> Result<(), oc_core::session::CoreError> {
        use oc_core::session::CoreError;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| CoreError::TabDeckStorage)?;
        for deck in decks.iter_mut() {
            if deck.projected()
                || deck.location.is_empty()
                || deck.sessions.len() > MAX_TABS
                || (deck.active.is_none() && deck.sessions.len() == MAX_TABS)
                || deck
                    .active
                    .as_ref()
                    .is_some_and(|id| !deck.sessions.contains(id))
            {
                return Err(CoreError::InvalidTabDeck);
            }
            let key = tab_deck_key(&deck.location);
            let current = match Self::get_pref_bounded_in(&tx, &key, MAX_TAB_DECK_BYTES)
                .map_err(|_| CoreError::TabDeckStorage)?
            {
                BoundedPref::Missing => None,
                BoundedPref::Value(raw) => Some(raw),
                BoundedPref::TooLarge => return Err(CoreError::StoredTabDeck),
            };
            if current.as_deref().map(pref_revision) != deck.revision {
                return Err(CoreError::TabDeckConflict);
            }
            let mut seen = HashSet::new();
            for id in &deck.sessions {
                let valid:bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id WHERE s.id=?1 AND s.parent_id IS NULL AND p.value=?2)",params![id.0,deck.location],|r|r.get(0)).map_err(|_|CoreError::TabDeckStorage)?;
                if !valid_tab_id(&id.0) || !seen.insert(&id.0) || !valid {
                    return Err(CoreError::InvalidTabDeck);
                }
            }
            let pending = Self::tab_adoptions_in(&tx, &deck.location)
                .map_err(|_| CoreError::StoredTabDeck)?;
            if pending
                .iter()
                .any(|id| !deck.sessions.iter().any(|session| &session.0 == id))
            {
                return Err(CoreError::TabDeckConflict);
            }
            let raw = serde_json::to_string(&StoredDeck {
                version: 1,
                sessions: deck.sessions.iter().map(|id| id.0.clone()).collect(),
                active: deck.active.as_ref().map(|id| id.0.clone()),
            })
            .map_err(|_| CoreError::InvalidTabDeck)?;
            if raw.len() > MAX_TAB_DECK_BYTES {
                return Err(CoreError::InvalidTabDeck);
            }
            Self::upsert_pref(&tx, &key, &raw).map_err(|_| CoreError::TabDeckStorage)?;
            for id in pending {
                tx.execute(
                    "DELETE FROM prefs WHERE key=?1",
                    [tab_adoption_key(&deck.location, &id)],
                )
                .map_err(|_| CoreError::TabDeckStorage)?;
            }
            deck.revision = Some(pref_revision(&raw));
        }
        tx.commit().map_err(|_| CoreError::TabDeckStorage)?;
        Ok(())
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

    /// Selection and its irreducible event commit in the same existing owner.
    pub(crate) fn commit_session_model_choice(
        &self,
        values: &[(String, String)],
        session: &str,
        payload: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        for (key, value) in values {
            Self::upsert_pref(&tx, key, value)?;
        }
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES(?1,'session_model_selected',?2)",
            params![session, payload],
        )?;
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

    pub(crate) fn commit_session_agent_choice(
        &self,
        values: &[(String, String)],
        session: &str,
        reminder: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        for (key, value) in values {
            Self::upsert_pref(&tx, key, value)?;
        }
        if let Some(text) = reminder {
            Self::insert_message(&tx, session, "system", text)?;
        }
        tx.commit()?;
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

    /// Catalog-only process, public metadata only. No Db owner, schema/recovery,
    /// chmod, or SQL writes; an inactive store may take a short shared read lock.
    /// SQLite's Unix VFS must map existing WAL shared memory read-only too;
    /// plain mode=ro may otherwise modify the live owner's -shm file.
    pub(crate) fn public_cache_read_only(root: &Path) -> Option<String> {
        let conn = Self::public_read_only_connection(root)?;
        match Self::get_pref_bounded_in(
            &conn,
            crate::models_dev::CACHE_KEY,
            crate::discovery::DISCOVERY_BODY_CAP,
        )
        .ok()?
        {
            BoundedPref::Value(raw) => Some(raw),
            _ => None,
        }
    }

    /// A catalog-only consumer needs only the admitted method identity, never
    /// token material or a refresh. Reuses the same protected read-only SQLite path.
    pub(crate) fn openai_subscription_read_only(root: &Path) -> bool {
        let Some(conn) = Self::public_read_only_connection(root) else {
            return false;
        };
        let Ok(scope) =
            crate::auth::AuthScope::admit(crate::models_dev::OPENAI, crate::auth::OPENAI_BASE_URL)
        else {
            return false;
        };
        conn.query_row(
            "SELECT json_extract(tagged_value_json, '$.type') = 'oauth' AND refresh_pending = 0
            AND json_extract(tagged_value_json, '$.methodID') IN ('chatgpt-browser', 'chatgpt-headless')
            AND json_extract(tagged_value_json, '$.expires_at') > 0
            FROM credential_accounts WHERE provider_namespace = ?1 AND active = 1",
            [scope.namespace()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
    }

    fn public_read_only_connection(root: &Path) -> Option<Connection> {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and does not change process state.
        let uid = unsafe { libc::geteuid() };
        let root_meta = std::fs::symlink_metadata(root).ok()?;
        if !root_meta.is_dir() || root_meta.uid() != uid {
            return None;
        }
        for name in ["oc.sqlite", "oc.sqlite-wal", "oc.sqlite-shm"] {
            match std::fs::symlink_metadata(root.join(name)) {
                Ok(meta) if meta.is_file() && meta.nlink() == 1 && meta.uid() == uid => {}
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound && name != "oc.sqlite" => {}
                _ => return None,
            }
        }
        let mut uri = reqwest::Url::from_file_path(root.join("oc.sqlite")).ok()?;
        // A clean, inactive store has no WAL/SHM. It can be read immutable only
        // while a shared existing root lock excludes every native writer. Never
        // use immutable on the live owner's WAL or as an unguarded fallback.
        let read_lock = (|| {
            use std::os::unix::fs::OpenOptionsExt;
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(root.join("oc.lock"))
                .ok()?;
            let meta = file.metadata().ok()?;
            if !meta.is_file() || meta.nlink() != 1 || meta.uid() != uid {
                return None;
            }
            fs2::FileExt::try_lock_shared(&file).ok()?;
            Some(file)
        })();
        let no_wal = match std::fs::symlink_metadata(root.join("oc.sqlite-wal")) {
            Ok(meta) => meta.len() == 0,
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        };
        uri.set_query(Some(if read_lock.is_some() && no_wal {
            "mode=ro&immutable=1"
        } else {
            "mode=ro&readonly_shm=1"
        }));
        let conn = Connection::open_with_flags(
            uri.as_str(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_URI
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .ok()?;
        conn.busy_timeout(std::time::Duration::from_millis(100))
            .ok()?;
        Some(conn)
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
    #[cfg(test)]
    pub(crate) fn accept_turn(
        &self,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
        model: &oc_core::queries::ModelRef,
    ) -> Result<AcceptedTurn, StorageError> {
        self.accept_turn_with_reminder(turn, session, prompt, user_text, model, None)
    }

    pub(crate) fn accept_turn_with_reminder(
        &self,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
        model: &oc_core::queries::ModelRef,
        reminder: Option<&str>,
    ) -> Result<AcceptedTurn, StorageError> {
        self.accept_turn_with_selection(
            turn, session, prompt, user_text, model, reminder, None, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn accept_turn_with_selection(
        &self,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
        model: &oc_core::queries::ModelRef,
        reminder: Option<&str>,
        selection: Option<(&str, &str)>,
        history_input: Option<&str>,
    ) -> Result<AcceptedTurn, StorageError> {
        if let Some((key, _)) = selection {
            let valid = key
                .strip_prefix("tui.selection.session:")
                .and_then(|parts| serde_json::from_str::<Vec<String>>(parts).ok())
                .is_some_and(|parts| parts.len() == 3 && parts[2] == session);
            if !valid {
                return Err(invalid_tab_adoption());
            }
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let accepted = Self::insert_accepted_turn(
            &tx,
            turn,
            session,
            prompt,
            user_text,
            model,
            reminder,
            history_input,
        )?;
        if let Some((key, value)) = selection {
            Self::upsert_pref(&tx, key, value)?;
        }
        tx.commit()?;
        Ok(accepted)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_accepted_turn(
        conn: &Connection,
        turn: &str,
        session: &str,
        prompt: &str,
        user_text: &str,
        model: &oc_core::queries::ModelRef,
        reminder: Option<&str>,
        history_input: Option<&str>,
    ) -> Result<AcceptedTurn, StorageError> {
        use oc_core::queries::ModelRef;
        Self::conversation_admit(conn, turn, session)?;
        if let Some(text) = reminder {
            Self::insert_message(conn, session, "system", text)?;
        }
        Self::insert_turn(conn, turn, session, prompt)?;
        // The event journal, unlike the picker preference or later turn
        // checkpoint, records the model of the last *accepted* prompt.
        let prior: Option<String> = conn.query_row(
            "SELECT a.model_ref FROM turn_acceptances a JOIN conversation_messages m ON m.id=a.user_message WHERE a.session_id=?1 ORDER BY m.seq DESC LIMIT 1",
            [session], |row| row.get(0),
        ).optional()?;
        let mut switch = None;
        if let Some(previous) = prior {
            let previous: ModelRef = serde_json::from_str(&previous).map_err(|_| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid accepted model reference",
                ))
            })?;
            if previous != *model {
                let notice = oc_core::queries::ModelSwitchNotice {
                    previous,
                    current: model.clone(),
                    display_name: None,
                };
                let text = serde_json::to_string(&notice).map_err(std::io::Error::other)?;
                Self::insert_message(conn, session, "model_switch", &text)?;
                switch = Some(notice);
            }
        }
        let reference = serde_json::to_string(model).map_err(std::io::Error::other)?;
        conn.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES (?1,'accepted_model',?2)",
            params![session, reference],
        )?;
        let user_message = Self::insert_message(conn, session, "user", user_text)?;
        conn.execute(
            "INSERT INTO turn_acceptances(turn_id,session_id,user_message,model_ref) VALUES (?1,?2,?3,?4)",
            params![turn,session,user_message,reference],
        )?;
        if let Some(text) = history_input {
            Self::prompt_history_in(conn, Some(text))?;
        }
        Ok(AcceptedTurn {
            user_message,
            model_switch: switch,
        })
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
        let (session, existing_status): (String, String) = tx.query_row(
            "SELECT session_id,status FROM turns WHERE id = ?1",
            [turn],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // First settlement owns the assistant row, wire journal and saved
        // context atomically. Retries cannot replace any of those artifacts.
        if existing_status != "started" {
            return Ok(());
        }
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
        Self::conversation_complete(&tx, turn, &session)?;
        tx.commit()?;
        Ok(())
    }

    /// Persist a completed generation before dispatch, without finishing its turn.
    pub(crate) fn checkpoint_turn(&self, turn: &str, result: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let affected = conn.execute(
            "UPDATE turns SET result = ?1 WHERE id = ?2 AND status = 'started'",
            params![result, turn],
        )?;
        if affected != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        Ok(())
    }

    /// Journal and retry event commit together, before any runtime wait.
    pub(crate) fn checkpoint_retry(
        &self,
        session: &str,
        turn: &str,
        span: &str,
        result: &str,
        retry: &oc_core::queries::RetryFact,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        if tx.execute(
            "UPDATE turns SET result=?1 WHERE id=?2 AND session_id=?3 AND status='started'",
            params![result, turn, session],
        )? != 1
        {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES (?1,'retry_scheduled',?2)",
            params![
                session,
                serde_json::json!({"turn":turn,"span":span,"retry":retry}).to_string()
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Count actual admitted send dispatches, independently of logical rounds.
    pub(crate) fn generation_dispatch(
        &self,
        session: &str,
        operation: &str,
        lane: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        // An inherited operation remains fixed even when a later root turn starts.
        let root_operation: Option<String> = conn.query_row(
            "SELECT COALESCE(json_extract(result,'$.display.owning_operation'),id) FROM turns WHERE id=?1 OR (session_id=?2 AND status='started') ORDER BY id=?1 DESC,rowid DESC LIMIT 1",
            params![operation,session], |r|r.get(0)).optional()?;
        conn.execute("INSERT INTO events(session_id,kind,payload) VALUES (?1,'generation_dispatched',?2)", params![session,serde_json::json!({"operation":root_operation.as_deref().unwrap_or(operation),"owner":operation,"lane":lane}).to_string()])?;
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
        self.tool_outcome_with_log_and_effects(op, state, output, turn, log, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tool_outcome_with_log_and_effects(
        &self,
        op: &str,
        state: &str,
        output: &str,
        turn: &str,
        log: &str,
        effects: Option<&oc_core::patch::PatchEffects>,
        presentation: Option<&oc_core::tool_output::Presentation>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let started: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND status='started')",
            [turn],
            |r| r.get(0),
        )?;
        if !started {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        let affected = tx.execute(
            "UPDATE tool_operations SET state = ?1, output = ?2 WHERE id = ?3 AND turn_id=?4 AND state='started'",
            params![state, output, op,turn],
        )?;
        if affected != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        if let Some(effects) = effects {
            let json = serde_json::to_string(effects).expect("serializable effects");
            tx.execute(
                "INSERT INTO patch_effects(op_id,metadata) VALUES(?1,?2)",
                params![op, json],
            )?;
        }
        tx.execute(
            "UPDATE turns SET result = ?1 WHERE id = ?2 AND status='started'",
            params![log, turn],
        )?;
        Self::record_tool_presentation_in(&tx, op, presentation)?;
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
        self.rename_session(session, title, true)
    }

    /// Existing manual-title transaction, with explicit native self-child access.
    pub(crate) fn rename_session(
        &self,
        session: &str,
        title: &str,
        root_only: bool,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let affected = tx.execute(
            "UPDATE sessions SET title = ?2 WHERE id = ?1 AND (?3 = 0 OR parent_id IS NULL)",
            params![session, title, root_only],
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

    /// Snapshot title and last title-event sequence in one SQLite read. The
    /// event fence also detects A -> B -> A renames during provider work.
    pub(crate) fn root_title_stamp(
        &self,
        session: &str,
    ) -> Result<Option<(Option<String>, i64)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "SELECT title, COALESCE((SELECT MAX(seq) FROM events WHERE session_id = sessions.id AND kind IN ('session_updated','conversation_changed')), 0) FROM sessions WHERE id = ?1 AND parent_id IS NULL",
            params![session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    /// Regeneration replaces only the title and event sequence observed
    /// before provider work. The event and update share a transaction.
    pub(crate) fn compare_and_set_root_title(
        &self,
        session: &str,
        expected: Option<&str>,
        expected_event: i64,
        title: &str,
    ) -> Result<bool, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        if expected == Some(title) {
            let unchanged = tx
                .query_row(
                    "SELECT 1 FROM sessions WHERE id = ?1 AND parent_id IS NULL AND title IS ?2 AND COALESCE((SELECT MAX(seq) FROM events WHERE session_id = sessions.id AND kind IN ('session_updated','conversation_changed')), 0) = ?3",
                    params![session, expected, expected_event],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            return Ok(unchanged);
        }
        let affected = tx.execute(
            "UPDATE sessions SET title = ?4 WHERE id = ?1 AND parent_id IS NULL AND title IS ?2 AND title IS NOT ?4 AND COALESCE((SELECT MAX(seq) FROM events WHERE session_id = sessions.id AND kind IN ('session_updated','conversation_changed')), 0) = ?3",
            params![session, expected, expected_event, title],
        )?;
        if affected == 0 {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'session_updated', ?2)",
            params![session, serde_json::json!({"title": title}).to_string()],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Bound the first request and recent public conversation before making a
    /// provider request. No journal, tool output or full transcript is loaded.
    pub(crate) fn title_context(
        &self,
        session: &str,
        has_title: bool,
    ) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let first: Option<(i64, String)> = conn.query_row(
            "SELECT seq, substr(text, 1, 2048) FROM conversation_messages WHERE session_id=?1 AND role='user' ORDER BY seq ASC LIMIT 1",
            params![session], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        let Some((first_seq, first)) = first else {
            return Ok(None);
        };
        if !has_title {
            return Ok(Some(first));
        }
        let original = format!("Original request:\n{first}");
        let mut stmt = conn.prepare_cached(
            "SELECT role, substr(text, 1, 2048) FROM conversation_messages WHERE session_id=?1 AND seq != ?2 AND role IN ('user','assistant') ORDER BY seq DESC LIMIT 12"
        )?;
        let rows = stmt.query_map(params![session, first_seq], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let recent = rows.collect::<Result<Vec<_>, _>>()?;
        let recent = recent
            .into_iter()
            .rev()
            .map(|(role, text)| format!("{role}: {text}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        if recent.is_empty() {
            return Ok(Some(original));
        }
        let mut context = format!("{original}\n\nRecent conversation:\n");
        let start = recent.floor_char_boundary(recent.len().saturating_sub(8192));
        context.push_str(&recent[start..]);
        Ok(Some(context))
    }

    /// Set a generated title once; explicit/child titles always win.
    #[cfg(test)]
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
            revision: conn.query_row("SELECT COALESCE(MAX(seq),0) FROM events WHERE session_id=?1",[session],|r|r.get::<_,i64>(0))? as u64,
            physical_requests: conn.query_row("SELECT count(*) FROM events WHERE kind='generation_dispatched' AND json_extract(payload,'$.operation')=?1",[&id],|r|r.get::<_,i64>(0))? as u64,
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
        // Bounded presentation window, not a runtime step/retry admission limit.
        turn.spans = self.latest_turn_spans(&conn, &id)?;
        let total: Option<i64> = conn.query_row(
            "SELECT COALESCE(json_extract(result,'$.raw_prefix.ends[4]'),0)+json_array_length(result,'$.display_parts') FROM turns WHERE id=?1",
            [&id],
            |r| r.get(0),
        )?;
        turn.legacy_text_only = total.is_none();
        // Per-turn part and byte serving budgets, independent of archive size.
        let refs = self.latest_turn_parts(&conn, &id)?;
        // Reserve settled metadata before admitting any text or tool input.
        // Each effect is indivisible: replay uses the canonical producer DTO.
        let mut reserved = 0i64;
        for (_, _, part) in &refs {
            if let Some(op) = part["tool"].as_str() {
                reserved+=conn.query_row("SELECT COALESCE(SUM(length(CAST(metadata AS BLOB))),0) FROM patch_effects WHERE op_id=?1",[op],|r| r.get::<_,i64>(0))?;
            }
        }
        let mut effects_budget = oc_core::patch::EFFECT_PREVIEW_BYTES_CAP;
        let mut budget = effects_budget.saturating_sub(reserved as usize);
        for (sequence, ordinal, part) in refs {
            let mut state = PartState {
                model_label: None,
                sequence,
                status: turn.status.clone(),
                truncated: part["truncated"].as_bool().unwrap_or(false),
                input_omitted: false,
            };
            let older_span = if let Some(span_id) = part["span"].as_str()
                && !turn.spans.iter().any(|s| s.id == span_id)
            {
                let raw: Option<Option<String>>=conn.query_row("WITH source AS (SELECT result AS payload FROM turns WHERE id=?1 AND ?3 IS NULL UNION ALL SELECT json_extract(payload,'$.journal') FROM turn_raw_segments WHERE turn_id=?1 AND ordinal=?3) SELECT CASE WHEN length(CAST(s.value AS BLOB))<=?4 THEN s.value END FROM source,json_each(source.payload,'$.spans') s WHERE s.value->>'$.id'=?2 LIMIT 1",params![id,span_id,ordinal,crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64],|r| r.get(0)).optional()?;
                if raw == Some(None) {
                    return Err(StorageError::CompressionConflict);
                }
                let raw = raw.flatten();
                raw.as_deref()
                    .map(serde_json::from_str::<oc_core::queries::AssistantSpan>)
                    .transpose()
                    .map_err(|_| StorageError::CompressionConflict)?
            } else {
                None
            };
            if let Some(span) = part["span"]
                .as_str()
                .and_then(|id| turn.spans.iter().find(|s| s.id == id))
                .or(older_span.as_ref())
            {
                state.status = span.status.clone();
                state.model_label = span
                    .request
                    .as_ref()
                    .map(|request| request.model_label.clone());
            }
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
                let path = format!(
                    "$.{}input[{index}].content",
                    if ordinal.is_some() { "journal." } else { "" }
                );
                let bytes: i64 = conn.query_row("WITH source AS (SELECT result AS payload FROM turns WHERE id=?1 AND ?3 IS NULL UNION ALL SELECT payload FROM turn_raw_segments WHERE turn_id=?1 AND ordinal=?3) SELECT COALESCE(SUM(length(CAST(c.value ->> '$.text' AS BLOB))),0) FROM source,json_each(source.payload,?2) c WHERE c.value ->> '$.type'='output_text'",params![id,path,ordinal],|r|r.get(0))?;
                state.truncated |= bytes as usize > budget;
                let mut texts=conn.prepare_cached("WITH source AS (SELECT result AS payload FROM turns WHERE id=?1 AND ?4 IS NULL UNION ALL SELECT payload FROM turn_raw_segments WHERE turn_id=?1 AND ordinal=?4) SELECT substr(c.value ->> '$.text',1,?3) FROM source,json_each(source.payload,?2) c WHERE c.value ->> '$.type'='output_text'")?;
                let mut text = String::new();
                for item in texts.query_map(params![id, path, budget as i64, ordinal], |r| {
                    r.get::<_, String>(0)
                })? {
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
                let view=conn.query_row("SELECT rowid,name,state,CASE WHEN length(CAST(input AS BLOB))<=?4 THEN input ELSE NULL END,substr(output,1,?3),length(CAST(output AS BLOB)),(SELECT metadata FROM patch_effects WHERE op_id=tool_operations.id),CASE WHEN name='question' THEN COALESCE((SELECT COALESCE(json_extract(e.payload,'$.result'),json_extract(e.payload,'$.question_resource')) FROM events e WHERE e.session_id=tool_operations.session_id AND e.kind='tool_output_question' AND json_extract(e.payload,'$.operation')=tool_operations.id LIMIT 1),substr(output,1,131136)) ELSE NULL END FROM tool_operations WHERE id=?1 AND turn_id=?2",params![op,id,TOOL_OP_PREVIEW_BYTES as i64,budget.saturating_sub(TOOL_OP_PREVIEW_BYTES) as i64],|r| {
                    let bytes=r.get::<_,Option<i64>>(5)?.unwrap_or(0);
                    let (output,output_truncated)=bound_preview(r.get(4)?,bytes);
                    let dcp = Self::dcp_run_in(&conn,session,op).map_err(|_| rusqlite::Error::InvalidQuery)?;
                    let name: String = r.get(1)?;
                    let input: Option<String> = r.get(3)?;
                    let dcp_topic = if name == "compress" { dcp.as_ref().map(|run|run.topic.clone()).or_else(||crate::dcp::presentation_topic(&name,input.as_deref())) } else { None };
                    let raw: Option<String> = r.get(7)?;
                    let state: String = r.get(2)?;
                    let question = self.question_presentation_in(&conn, &name, &state, raw.as_deref());
                    let output_presentation=Self::tool_presentation_in(&conn,session,op).map_err(|_|rusqlite::Error::InvalidQuery)?;
                    Ok(ToolOpView{output_presentation,question,dcp_topic,dcp,patch_effects:decode_patch_effects(r.get(6)?),op:op.to_string(),rowid:r.get(0)?,name,state,input,output,output_bytes:bytes,output_truncated})
                }).optional()?;
                if let Some(mut view) = view {
                    if let Some(question) = &view.question {
                        let bytes = serde_json::to_vec(question)
                            .expect("question metadata")
                            .len();
                        if bytes > effects_budget {
                            continue;
                        }
                        effects_budget -= bytes;
                    }
                    if let Some(effects) = &view.patch_effects {
                        let bytes = serde_json::to_vec(effects).expect("effects").len();
                        if bytes > effects_budget {
                            continue;
                        }
                        effects_budget -= bytes;
                        state.truncated |= effects.truncated;
                    }
                    // Output is admitted after metadata and structured input;
                    // never let the independent SQL preview exceed the turn cap.
                    let mut remaining =
                        budget.saturating_sub(view.input.as_ref().map_or(0, String::len));
                    let presentation_bytes = view
                        .output_presentation
                        .as_ref()
                        .map_or(0, |presentation| presentation.retained_bytes());
                    let presentation_bytes = if presentation_bytes > remaining {
                        view.output_presentation = None;
                        state.truncated = true;
                        0
                    } else {
                        remaining -= presentation_bytes;
                        presentation_bytes
                    };
                    if let Some(output) = &mut view.output
                        && output.len() > remaining
                    {
                        output.truncate(output.floor_char_boundary(remaining));
                        view.output_truncated = true;
                    }
                    state.status = view.state.clone();
                    state.input_omitted = view.input.is_none();
                    state.truncated |= state.input_omitted || view.output_truncated;
                    budget = budget.saturating_sub(
                        view.input.as_ref().map_or(0, String::len)
                            + view.output.as_ref().map_or(0, String::len)
                            + presentation_bytes,
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
        let bytes: i64 = conn.query_row(
            "SELECT COALESCE(length(CAST(result AS BLOB)),0) FROM turns WHERE id=?1",
            [turn],
            |r| r.get(0),
        )?;
        if bytes < 0 || bytes as u64 > crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as u64 {
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "turn hot journal exceeds existing context byte budget",
            )));
        }
        let descriptor: Option<String> = conn.query_row("SELECT CASE WHEN json_valid(result) THEN json_extract(result,'$.raw_prefix') END FROM turns WHERE id=?1", [turn], |r| r.get(0))?;
        let prefix = descriptor
            .as_deref()
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(|_| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid raw prefix",
                ))
            })?;
        let prefix =
            crate::tools::turn_history::RawPrefix::parse(prefix.as_ref()).map_err(|e| {
                StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
        Self::validate_raw_prefix(&conn, turn, prefix.as_ref())?;
        let result = conn
            .query_row(
                "SELECT status, result FROM turns WHERE id = ?1",
                params![turn],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map_err(StorageError::Sqlite)?;
        if let Some(raw) = &result.1 {
            self.count_history_read(0, raw.len());
        }
        Ok(result)
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
             CREATE TABLE IF NOT EXISTS compression_identity(singleton INTEGER PRIMARY KEY CHECK(singleton=1), high_water INTEGER NOT NULL);
             INSERT INTO compression_identity(singleton,high_water) SELECT 1,COALESCE(MAX(CAST(SUBSTR(id,2) AS INTEGER)),0) FROM compression_blocks WHERE true
               ON CONFLICT(singleton) DO UPDATE SET high_water=MAX(high_water,excluded.high_water);
              CREATE TRIGGER IF NOT EXISTS compression_identity_insert AFTER INSERT ON compression_blocks BEGIN
               UPDATE compression_identity SET high_water=MAX(high_water,CAST(SUBSTR(NEW.id,2) AS INTEGER)) WHERE singleton=1;
              END;
              CREATE TABLE IF NOT EXISTS dcp_accounting(session_id TEXT PRIMARY KEY REFERENCES sessions(id), snapshot TEXT NOT NULL);
              CREATE TABLE IF NOT EXISTS dcp_run_views(operation_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), snapshot TEXT NOT NULL);
              CREATE INDEX IF NOT EXISTS dcp_run_session ON dcp_run_views(session_id);
              CREATE TABLE IF NOT EXISTS dcp_coverage(session_id TEXT NOT NULL REFERENCES sessions(id), kind TEXT NOT NULL, identity TEXT NOT NULL, PRIMARY KEY(session_id,kind,identity));
              CREATE TABLE IF NOT EXISTS dcp_run_identity(session_id TEXT PRIMARY KEY REFERENCES sessions(id), high_water INTEGER NOT NULL);
              INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (2, 't17');",
        )?;
        Self::renewal_schema(&conn)?;
        Self::install_context_tracking(&conn)
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
            "SELECT high_water FROM compression_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let id = format!("b{:04}", max.unwrap_or(0) + 1);
        let now = now_rfc3339();
        tx.execute(
            "INSERT INTO compression_blocks(id, session_id, topic, summary, start_msg, end_msg, created_at,hot)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,?8)",
            params![id, session, topic, summary, start_msg, end_msg, now,serde_json::json!({"version":1,"active":!members.is_empty(),"standalone":false,"protected":[],"logs":[]}).to_string()],
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
            "SELECT id, topic, summary, start_msg, end_msg,hot FROM compression_blocks
             WHERE session_id = ?1 ORDER BY id ASC",
        )?;
        let blocks = stmt.query_map(params![session], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for block in blocks {
            let (id, topic, summary, start_msg, end_msg, hot) = block?;
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
                hot,
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
    pub(crate) fn compression_commit_snapshot(
        &self,
        session: &str,
    ) -> Result<(i64, Option<String>, u64), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let revision = Self::dcp_projection_revision_in(&conn, session)?;
        let prune = conn
            .query_row(
                "SELECT up_to_msg FROM prune_marks WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        let high: i64 = conn.query_row(
            "SELECT high_water FROM compression_identity WHERE singleton=1",
            [],
            |r| r.get(0),
        )?;
        Ok((revision, prune, (high.max(0) as u64).saturating_add(1)))
    }

    pub(super) fn dcp_projection_revision_in(
        conn: &Connection,
        session: &str,
    ) -> Result<i64, StorageError> {
        Ok(conn.query_row("SELECT COALESCE(MAX(MAX(valid_from,COALESCE(valid_to,0))),0) FROM conversation_versions WHERE session_id=?1 AND kind IN (0,1,2,3,4)",[session],|r|r.get(0))?)
    }

    /// Latest durable continuation route, without loading its journal payload.
    pub(crate) fn dcp_wire_route(
        &self,
        session: &str,
    ) -> Result<Option<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Ok(conn.query_row("SELECT json_extract(result,'$.model'),json_extract(result,'$.provider') FROM conversation_turns WHERE session_id=?1 AND json_valid(result) AND json_extract(result,'$.model') IS NOT NULL AND json_extract(result,'$.provider') IS NOT NULL ORDER BY archive_rowid DESC LIMIT 1",[session],|r|Ok((r.get(0)?,r.get(1)?))).optional()?)
    }

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
            "SELECT high_water FROM compression_identity WHERE singleton=1",
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
    ) -> Result<oc_core::dcp_view::DcpRunSnapshot, StorageError> {
        let CompressionPlanCommit {
            session,
            blocks,
            consumed_blocks,
            expected_next,
            expected_revision,
            expected_prune,
            hidden_calls,
            purged_calls,
            tool,
            measurement,
        } = plan;
        if tool.is_some_and(|tool| tool.operation_state != "completed") {
            return Err(StorageError::CompressionConflict);
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::require_session(&tx, session)?;

        let max: Option<i64> = tx.query_row(
            "SELECT high_water FROM compression_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let actual_next = u64::try_from(max.unwrap_or(0))
            .unwrap_or(0)
            .saturating_add(1);
        if actual_next != expected_next {
            return Err(StorageError::CompressionConflict);
        }
        let actual_revision = Self::dcp_projection_revision_in(&tx, session)?;
        let actual_prune = tx
            .query_row(
                "SELECT up_to_msg FROM prune_marks WHERE session_id = ?1",
                params![session],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if actual_revision != expected_revision || actual_prune.as_deref() != expected_prune {
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
            if block.hot.is_some() {
                let bounds:Option<(i64,i64)>=tx.query_row("SELECT a.seq,z.seq FROM conversation_messages a JOIN conversation_messages z ON z.session_id=a.session_id WHERE a.session_id=?1 AND a.id=?2 AND z.id=?3",params![session,block.start_msg,block.end_msg],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                let (start, end) = bounds.ok_or(StorageError::CompressionConflict)?;
                if start > end {
                    return Err(StorageError::CompressionConflict);
                }
                let mut overlaps=tx.prepare("SELECT b.id,a.seq,z.seq FROM compression_blocks b JOIN messages a ON a.id=b.start_msg JOIN messages z ON z.id=b.end_msg WHERE b.session_id=?1 AND a.seq<=?3 AND z.seq>=?2 AND (b.hot IS NOT NULL AND json_valid(b.hot) AND json_extract(b.hot,'$.active')=1 OR b.hot IS NULL AND EXISTS(SELECT 1 FROM compression_members cm WHERE cm.block_id=b.id)) LIMIT 4097")?;
                for (index, row) in overlaps
                    .query_map(params![session, start, end], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, i64>(2)?,
                        ))
                    })?
                    .enumerate()
                {
                    let (id, a, z) = row?;
                    if index >= 4096 || !consumed_set.contains(&id) || a < start || z > end {
                        return Err(StorageError::CompressionConflict);
                    }
                }
            }
            for member in &block.members {
                if !candidate_members.insert(member.as_str()) {
                    return Err(StorageError::CompressionConflict);
                }
                let existing_block = tx
                    .query_row(
                        "SELECT cm.block_id FROM compression_members AS cm
                         JOIN compression_blocks AS cb ON cb.id = cm.block_id
                          WHERE cb.session_id = ?1 AND cm.message_id = ?2 AND (cb.hot IS NULL OR json_extract(cb.hot,'$.active')=1) LIMIT 1",
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
            let native = blocks.iter().any(|b| b.hot.is_some());
            if native {
                tx.execute("UPDATE compression_blocks SET hot=json_set(COALESCE(hot,'{\"version\":1,\"standalone\":false}'),'$.active',json('false'),'$.logs',json('[]')) WHERE id=?1",[block])?;
            } else {
                tx.execute("DELETE FROM compression_members WHERE block_id=?1", [block])?;
            }
        }
        for block in blocks {
            tx.execute(
                "INSERT INTO compression_blocks(id, session_id, topic, summary, start_msg, end_msg, created_at,hot)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,?8)",
                params![
                    block.id,
                    session,
                    block.topic,
                    block.summary,
                    block.start_msg,
                    block.end_msg,
                    now
                    ,block.hot
                ],
            )?;
            for member in block.members.iter().filter(|_| block.hot.is_none()) {
                tx.execute(
                    "INSERT INTO compression_members(block_id, message_id) VALUES (?1, ?2)",
                    params![block.id, member],
                )?;
            }
        }
        if let Some(replacement) = &measurement.replacement_projection {
            Self::replace_compaction_marks(&tx, session, replacement, &measurement.retire_keys)?;
        }
        for (call_id, occurrence) in hidden_calls
            .iter()
            .filter(|_| measurement.replacement_projection.is_none())
        {
            let occurrence =
                i64::try_from(*occurrence).map_err(|_| StorageError::CompressionConflict)?;
            tx.execute(
                "INSERT INTO dcp_tool_projection_v2(session_id, call_id, occurrence, action,active)
                 VALUES (?1, ?2, ?3, 'hidden',1)
                 ON CONFLICT(session_id, call_id, occurrence) DO UPDATE SET action = 'hidden',active=1",
                params![session, call_id, occurrence],
            )?;
        }
        for (call_id, occurrence) in purged_calls
            .iter()
            .filter(|_| measurement.replacement_projection.is_none())
        {
            if !hidden_calls.contains(&(call_id.clone(), *occurrence)) {
                let occurrence =
                    i64::try_from(*occurrence).map_err(|_| StorageError::CompressionConflict)?;
                tx.execute(
                    "INSERT INTO dcp_tool_projection_v2(session_id, call_id, occurrence, action,active)
                     VALUES (?1, ?2, ?3, 'purged',1)
                     ON CONFLICT(session_id, call_id, occurrence) DO UPDATE SET action = 'purged',active=1",
                    params![session, call_id, occurrence],
                )?;
            }
        }

        let snapshot = Self::commit_dcp_view(&tx, session, blocks, measurement, tool)?;
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
        // A standalone operation revises the genuine settled tip as well as
        // ContextVersion rows; running turns publish at their normal settlement.
        Self::publish_settled_context(&tx, session)?;
        tx.commit()?;
        Ok(snapshot)
    }

    /// Unit assertions only; production reads the indexed admitted wire window.
    #[cfg(test)]
    pub(crate) fn load_dcp_tool_projection(
        &self,
        session: &str,
    ) -> Result<DcpToolProjection, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::load_dcp_tool_projection_in(&conn, session)
    }

    #[cfg(test)]
    pub(super) fn load_dcp_tool_projection_in(
        conn: &Connection,
        session: &str,
    ) -> Result<DcpToolProjection, StorageError> {
        Self::require_session(conn, session)?;
        let mut statement = conn.prepare_cached(
            "SELECT call_id, occurrence, action FROM dcp_tool_projection_v2 WHERE session_id = ?1 AND (active=1 OR(active IS NULL AND NOT EXISTS(SELECT 1 FROM prefs WHERE key='dcp.projection_owned.'||?1 AND value='true')))",
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
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let old_seq: i64 = tx.query_row("SELECT COALESCE((SELECT seq FROM messages WHERE session_id=?1 AND id=(SELECT up_to_msg FROM prune_marks WHERE session_id=?1)),0)",[session],|r|r.get(0))?;
        let new_seq: Option<i64> = tx
            .query_row(
                "SELECT seq FROM conversation_messages WHERE session_id=?1 AND id=?2",
                params![session, up_to],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(new_seq) = new_seq {
            Self::account_prune(&tx, session, old_seq, new_seq)?;
        }
        tx.prepare_cached(
            "INSERT INTO prune_marks(session_id, up_to_msg, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET up_to_msg = ?2, created_at = ?3",
        )?
        .execute(params![session, up_to, now_rfc3339()])?;
        tx.commit()?;
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
        let identity = Self::call_identity(op, turn, journal)?;
        tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output,provider_call_id,call_occurrence,original_input_index) VALUES(?1,?2,?3,?4,'started',?5,NULL,?6,?7,?8)", params![op,session,turn,name,input,identity.as_ref().map(|v|&v.0),identity.as_ref().map(|v|v.1),identity.as_ref().map(|v|v.2)])?;
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
            .prepare_cached("UPDATE tool_operations SET state = ?1, output = ?2 WHERE id = ?3 AND (turn_id IS NULL OR EXISTS(SELECT 1 FROM turns WHERE turns.id=tool_operations.turn_id AND turns.status='started'))")?
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
        let session: String =
            tx.query_row("SELECT session_id FROM turns WHERE id=?1", [turn], |r| {
                r.get(0)
            })?;
        Self::conversation_complete(&tx, turn, &session)?;
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
        tx.execute("INSERT INTO events(session_id, kind, payload) SELECT session_id, 'turn_unknown', id FROM turns WHERE status = 'started' AND NOT EXISTS(SELECT 1 FROM child_jobs j WHERE j.child_turn=turns.id AND j.state='running')", [])?;
        let interrupted: Vec<(String, String)> = {
            let mut stmt = tx.prepare("SELECT id,session_id FROM turns WHERE status='started' AND NOT EXISTS(SELECT 1 FROM child_jobs j WHERE j.child_turn=turns.id AND j.state='running')")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        for (turn, session) in interrupted {
            Self::conversation_complete(&tx, &turn, &session)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            tx.execute("UPDATE turns SET result=json_set(result,'$.spans['||(json_array_length(result,'$.spans')-1)||'].status','unknown','$.spans['||(json_array_length(result,'$.spans')-1)||'].completed',?2) WHERE id=?1 AND json_valid(result) AND json_array_length(result,'$.spans')>0 AND json_extract(result,'$.spans['||(json_array_length(result,'$.spans')-1)||'].completed') IS NULL",params![turn,now])?;
        }
        tx.execute(
            "UPDATE turns SET status = 'unknown' WHERE status = 'started' AND NOT EXISTS(SELECT 1 FROM child_jobs j WHERE j.child_turn=turns.id AND j.state='running')",
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
        let mut used = (data.len() as u64).saturating_add(self.output_disk_bytes()?);
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

fn secure_sqlite_file(path: &Path, create: bool) -> Result<(), StorageError> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if !create && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    // SAFETY: geteuid takes no pointers and has no preconditions.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.uid() != owner {
        return Err(StorageError::UnsafeRoot("sqlite file owner/type".into()));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(())
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
          CREATE TABLE IF NOT EXISTS turn_acceptances(
            turn_id TEXT PRIMARY KEY REFERENCES turns(id),
            session_id TEXT NOT NULL REFERENCES sessions(id),
            user_message TEXT NOT NULL UNIQUE REFERENCES messages(id),
            model_ref TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS tool_operations(
           id TEXT PRIMARY KEY, session_id TEXT NOT NULL, turn_id TEXT,
           name TEXT NOT NULL, state TEXT NOT NULL, input TEXT, output TEXT);
          CREATE TABLE IF NOT EXISTS events(
            seq INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL,
            kind TEXT NOT NULL, payload TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS patch_effects(
            op_id TEXT PRIMARY KEY REFERENCES tool_operations(id) ON DELETE CASCADE,
            metadata TEXT NOT NULL);
          CREATE INDEX IF NOT EXISTS events_accepted_model ON events(session_id, seq) WHERE kind='accepted_model';
          CREATE INDEX IF NOT EXISTS events_generation_operation ON events(json_extract(payload,'$.operation')) WHERE kind='generation_dispatched';
         CREATE TABLE IF NOT EXISTS blobs(digest TEXT PRIMARY KEY, size INTEGER NOT NULL, path TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS prefs(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
         INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, 't04');",
    )?;
    apply_child_session_schema(conn)?;
    apply_turn_acceptance_schema(conn)
}

const TURN_ACCEPTANCE_SCHEMA_VERSION: i64 = 4;
fn decode_patch_effects(json: Option<String>) -> Option<oc_core::patch::PatchEffects> {
    json.and_then(|json| serde_json::from_str(&json).ok())
}
const MAX_ACCEPTANCE_MIGRATION_FIELD_BYTES: i64 = 65536;

/// Recover admissions, including turns with no checkpoint, in one ordered
/// journal pass. Retain only one session's current admission, never combinations
/// of later events. The index, backfill and completion marker commit together.
/// Total journal size is not a retained-memory bound; only each pending field
/// is bounded, so a large valid database can still finish migration.
fn apply_turn_acceptance_schema(conn: &Connection) -> Result<(), StorageError> {
    let tx = conn.unchecked_transaction()?;
    if tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=?1)",
        [TURN_ACCEPTANCE_SCHEMA_VERSION],
        |r| r.get::<_, bool>(0),
    )? {
        return Ok(());
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS events_session_seq ON events(session_id,seq)")?;
    let budget_error = || {
        StorageError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "turn acceptance migration budget exceeded",
        ))
    };
    {
        let mut stmt = tx.prepare(
            "SELECT CASE WHEN length(CAST(e.session_id AS BLOB))<=?1 THEN e.session_id END,e.kind,
                    CASE WHEN length(CAST(e.payload AS BLOB))<=?1 THEN e.payload END,
                    t.id IS NOT NULL AND a.turn_id IS NULL
             FROM events e INDEXED BY events_session_seq
             LEFT JOIN messages m ON e.kind='message' AND m.id=e.payload AND m.session_id=e.session_id
             LEFT JOIN turns t ON e.kind='turn_started' AND t.id=e.payload AND t.session_id=e.session_id
             LEFT JOIN turn_acceptances a ON a.turn_id=t.id
             WHERE e.kind IN ('turn_started','turn_finished','accepted_model')
                OR (e.kind='message' AND m.role='user')
             ORDER BY e.session_id,e.seq")?;
        let mut rows = stmt.query([MAX_ACCEPTANCE_MIGRATION_FIELD_BYTES])?;
        let mut session = String::new();
        let mut admission: Option<(String, Option<String>)> = None;
        while let Some(row) = rows.next()? {
            let current = row.get::<_, Option<String>>(0)?.ok_or_else(budget_error)?;
            if current != session {
                session = current;
                admission = None;
            }
            let kind: String = row.get(1)?;
            match kind.as_str() {
                "turn_started" => {
                    admission = if row.get::<_, bool>(3)? {
                        Some((
                            row.get::<_, Option<String>>(2)?.ok_or_else(budget_error)?,
                            None,
                        ))
                    } else {
                        None
                    };
                }
                "turn_finished" => admission = None,
                "accepted_model" => {
                    if let Some((_, model)) = &mut admission {
                        *model = Some(row.get::<_, Option<String>>(2)?.ok_or_else(budget_error)?);
                    }
                }
                "message" => {
                    if let Some((turn, Some(model))) = admission.take() {
                        let user = row.get::<_, Option<String>>(2)?.ok_or_else(budget_error)?;
                        tx.execute("INSERT OR IGNORE INTO turn_acceptances(turn_id,session_id,user_message,model_ref) VALUES (?1,?2,?3,?4)",params![turn,session,user,model])?;
                    }
                }
                _ => unreachable!("filtered admission journal"),
            }
        }
    }
    tx.execute(
        "INSERT INTO schema_migrations(version,applied_at) VALUES (?1,'t44-fork')",
        [TURN_ACCEPTANCE_SCHEMA_VERSION],
    )?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests;
