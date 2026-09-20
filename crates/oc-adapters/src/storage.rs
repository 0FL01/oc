//! SQLite storage worker for T04.
//!
//! Own data-root lock, WAL + `synchronous=FULL`, short transactions, bounded
//! content-addressed blobs and crash recovery. No upstream DB is ever opened
//! for writing; no migration of foreign databases happens here.
//!
//! Layout under `<root>/`:
//! `oc.lock` (advisory exclusive flock, never deleted), `oc.sqlite`,
//! `oc.sqlite-wal/shm` (SQLite), `blobs/<sha256>` (content-addressed).

use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use fs2::FileExt as _;
use rusqlite::{Connection, params};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

/// Product blob quota reference (2 GiB, see `examples/oc-rs.toml`).
pub const DEFAULT_BLOB_QUOTA_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Schema version applied by T04.
pub const SCHEMA_VERSION: i64 = 1;

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
    /// Underlying SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// Underlying I/O failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Owned storage handle: lock file + SQLite connection + blob dir.
///
/// The lock `File` is held for the whole lifetime; dropping releases the
/// flock. The lockfile inode is never deleted by PID.
pub struct Db {
    root: PathBuf,
    blob_dir: PathBuf,
    quota_bytes: u64,
    _lock: File,
    conn: Mutex<Connection>,
}

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

        let db_path = root.join("oc.sqlite");
        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        apply_schema(&conn)?;

        Ok(Self {
            root,
            blob_dir,
            quota_bytes,
            _lock: lock,
            conn: Mutex::new(conn),
        })
    }

    /// Data-root path (owned).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Create a session; duplicate ids fail.
    pub fn create_session(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let now = now_rfc3339();
        let mut stmt =
            conn.prepare_cached("INSERT INTO sessions(id, created_at) VALUES (?1, ?2)")?;
        let res = stmt.execute(params![id, now]);
        match res {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
            {
                return Err(StorageError::Sqlite(rusqlite::Error::SqliteFailure(
                    err, None,
                )));
            }
            Err(other) => return Err(StorageError::Sqlite(other)),
        }
        conn.prepare_cached(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'session_created', ?2)",
        )?
        .execute(params![id, "{}"])?;
        Ok(())
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
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE session_id = ?1",
            params![session],
            |row| row.get(0),
        )?;
        if tx
            .query_row(
                "SELECT 1 FROM sessions WHERE id = ?1",
                params![session],
                |_| Ok(()),
            )
            .is_err()
        {
            return Err(StorageError::SessionNotFound);
        }
        let id = format!("m{seq:04}");
        tx.execute(
            "INSERT INTO messages(id, session_id, seq, role, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, session, seq, role, text],
        )?;
        tx.execute(
            "INSERT INTO events(session_id, kind, payload) VALUES (?1, 'message', ?2)",
            params![session, id],
        )?;
        tx.commit()?;
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

    /// List sessions in sorted order.
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

    /// Begin a turn (durable intent before any side effect).
    pub fn begin_turn(&self, turn: &str, session: &str, prompt: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
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
        let conn = self.conn.lock().expect("db mutex");
        let n = conn
            .prepare_cached("UPDATE turns SET status = ?1, result = ?2 WHERE id = ?3")?
            .execute(params![status, result, turn])?;
        if n == 0 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        Ok(())
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
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM compression_blocks WHERE session_id = ?1",
            params![session],
            |row| row.get(0),
        )?;
        let id = format!("b{:04}", count + 1);
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
                "SELECT message_id FROM compression_members WHERE block_id = ?1 ORDER BY message_id ASC",
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

    /// Crash recovery: mark `started` operations as `unknown`.
    ///
    /// Never replays the mutation; a new explicit attempt must use a new
    /// operation id. Returns the number of marked rows.
    pub fn recover_interrupted_tools(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let n = conn
            .prepare_cached("UPDATE tool_operations SET state = 'unknown' WHERE state = 'started'")?
            .execute([])?;
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
    /// Order: quota check → atomic temp/write/fsync/rename → durable DB row.
    /// A crash between file and row leaves a safe unreferenced orphan that
    /// [`Db::gc_orphans`] collects after a grace period; referenced blobs are
    /// never deleted.
    pub fn write_blob(&self, data: &[u8]) -> Result<String, StorageError> {
        let digest = hex_digest(data);
        let target = self.blob_path(&digest)?;
        if target.exists() {
            return Ok(digest);
        }
        let used: i64 = self.conn.lock().expect("db mutex").query_row(
            "SELECT COALESCE(SUM(size), 0) FROM blobs",
            [],
            |row| row.get(0),
        )?;
        if (used as u64).saturating_add(data.len() as u64) > self.quota_bytes {
            return Err(StorageError::StorageFull);
        }
        // Temp file in the same directory for atomic rename.
        let tmp_name = format!(".tmp-{}-{}", std::process::id(), &digest[..16]);
        let tmp_path = self.blob_dir.join(tmp_name);
        {
            let mut tmp = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)?;
            tmp.write_all(data)?;
            tmp.sync_all()?;
        }
        fs::rename(&tmp_path, &target)?;
        // Durable DB reference after the file is durable.
        self.conn
            .lock()
            .expect("db mutex")
            .prepare_cached("INSERT OR IGNORE INTO blobs(digest, size, path) VALUES (?1, ?2, ?3)")?
            .execute(params![digest, data.len() as i64, digest])?;
        Ok(digest)
    }

    /// Read blob bytes by digest.
    pub fn read_blob(&self, digest: &str) -> Result<Vec<u8>, StorageError> {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(StorageError::BlobNotFound);
        }
        let conn = self.conn.lock().expect("db mutex");
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM blobs WHERE digest = ?1",
                params![digest],
                |_| Ok(()),
            )
            .is_ok();
        if !exists {
            return Err(StorageError::BlobNotFound);
        }
        let path = self.blob_path(digest)?;
        Ok(fs::read(path)?)
    }

    /// Collect unreferenced blob files older than `grace`.
    ///
    /// Never deletes referenced blobs. Never follows symlinks outside the
    /// blob dir and never deletes outside it (cleanup-escape refusal).
    pub fn gc_orphans(&self, grace: Duration) -> Result<usize, StorageError> {
        let referenced: std::collections::HashSet<String> = {
            let conn = self.conn.lock().expect("db mutex");
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
            if meta.file_type().is_symlink() {
                continue;
            }
            if !path.starts_with(&self.blob_dir) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".tmp-") {
                // Crash temp older than grace is safe to remove.
                if is_older_than(&path, grace)? {
                    fs::remove_file(&path)?;
                    removed += 1;
                }
                continue;
            }
            if referenced.contains(&name) {
                continue;
            }
            if is_older_than(&path, grace)? {
                fs::remove_file(&path)?;
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
         INSERT OR IGNORE INTO schema_migrations(version, applied_at) VALUES (1, 't04');",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Db, StorageError};
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;
    use std::time::Duration;

    fn tmp_root(name: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        // Keep the name in the test log without moving the dir.
        let _ = name;
        dir
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
}
