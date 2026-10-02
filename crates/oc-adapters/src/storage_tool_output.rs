//! Expirable tool text under the existing Db/flock/connection and blob quota.
//! No payload is stored in SQLite. Every descriptor is tied to an original intent.
use super::*;
use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;

pub(crate) const CAP: u64 = 16 * 1024 * 1024;
pub(crate) const TTL: i64 = 7 * 24 * 60 * 60;
#[path = "storage_tool_output/access.rs"]
mod access;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum CaptureState {
    Active,
    Complete,
    ProducerLimited,
    ArtifactCap,
    Quota,
    Io,
    RegisterFailure,
    Interrupted,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Resource {
    pub id: String,
    pub operation: String,
    pub session: String,
    pub location: String,
    pub generation: u64,
    pub source: String,
    pub path: String,
    pub bytes: u64,
    pub lines: u64,
    pub admitted_bytes: u64,
    pub admitted_lines: u64,
    pub state: CaptureState,
}

pub(crate) struct Writer {
    db: Db,
    file: File,
    resource: Resource,
    name: String,
    pending: String,
    secrets: Vec<String>,
    last_newline: bool,
    admitted_newlines: u64,
    published_newlines: u64,
    finished: bool,
}

pub(crate) struct Reader {
    db: Db,
    pub resource: Resource,
    pub file: File,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionResource {
    resource_id: String,
    source_operation: String,
    source_session: String,
}

fn invalid() -> StorageError {
    StorageError::UnsafeRoot("tool output unavailable, expired, or identity changed; rerun only an explicitly safe operation".into())
}

pub(super) fn open_directory(path: &Path) -> Result<File, StorageError> {
    let mut dir = File::open("/")?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            continue;
        };
        dir = openat(
            &dir,
            name.as_encoded_bytes(),
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
    }
    Ok(dir)
}

fn openat(dir: &File, name: &[u8], flags: i32) -> Result<File, StorageError> {
    let name = CString::new(name).map_err(|_| invalid())?;
    // SAFETY: name is terminated and dir owns a live descriptor. O_NONBLOCK
    // prevents swapped FIFOs/devices from blocking before the regular-file check.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn unlink(dir: &File, name: &str) -> Result<(), StorageError> {
    let name = CString::new(name).map_err(|_| invalid())?;
    // SAFETY: borrowed directory and terminated exact generated basename.
    if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(error.into());
        }
    }
    Ok(())
}

pub(super) fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .min(i64::MAX as u64) as i64
}

impl Db {
    pub(super) fn tool_output_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
          CREATE TABLE IF NOT EXISTS tool_output_resources(
            id TEXT PRIMARY KEY, operation_id TEXT NOT NULL UNIQUE REFERENCES tool_operations(id),
            session_id TEXT NOT NULL REFERENCES sessions(id), name TEXT NOT NULL UNIQUE,
            descriptor TEXT NOT NULL CHECK(length(CAST(descriptor AS BLOB))<=16384),
            dev INTEGER NOT NULL, ino INTEGER NOT NULL, extent INTEGER NOT NULL,
            state TEXT NOT NULL, completed INTEGER);
          CREATE INDEX IF NOT EXISTS tool_output_operation ON tool_output_resources(operation_id);
          INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(7,'t50-tool-output');
          UPDATE tool_output_resources SET state='Interrupted', completed=unixepoch(),
            descriptor=json_set(descriptor,'$.state','Interrupted') WHERE state='Active';
          COMMIT;",
        )?;
        Ok(())
    }

    fn verify_output_dir(&self) -> Result<(), StorageError> {
        let actual = open_directory(&self.root.join("tool-output"))?.metadata()?;
        let pinned = self.output_dir.metadata()?;
        if actual.dev() != pinned.dev() || actual.ino() != pinned.ino() {
            return Err(invalid());
        }
        Ok(())
    }

    pub(super) fn output_disk_bytes(&self) -> Result<u64, StorageError> {
        self.verify_output_dir()?;
        let mut bytes = 0u64;
        for entry in fs::read_dir(self.root.join("tool-output"))? {
            let meta = fs::symlink_metadata(entry?.path())?;
            // symlinks are not followed by the subsequent descriptor open; reject
            // unsafe entries rather than silently granting extra quota.
            if !meta.is_file() {
                return Err(invalid());
            }
            bytes = bytes
                .checked_add(meta.len())
                .ok_or(StorageError::StorageFull)?;
        }
        Ok(bytes)
    }

    fn resource_used(&self) -> Result<u64, StorageError> {
        let mut used = self.output_disk_bytes()?;
        for entry in fs::read_dir(&self.blob_dir)? {
            let meta = fs::symlink_metadata(entry?.path())?;
            if meta.is_file() {
                used = used
                    .checked_add(meta.len())
                    .ok_or(StorageError::StorageFull)?;
            }
        }
        Ok(used)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_tool_output(
        &self,
        operation: &str,
        session: &str,
        location: &str,
        generation: u64,
        source: &str,
        secrets: Vec<String>,
    ) -> Result<Writer, StorageError> {
        self.expire_tool_outputs(timestamp())?;
        let conn = self.conn.lock().expect("db mutex");
        self.verify_output_dir()?;
        let belongs: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tool_operations WHERE id=?1 AND session_id=?2)",
            params![operation, session],
            |r| r.get(0),
        )?;
        if !belongs
            || location.len() > 4096
            || source.len() > 4096
            || secrets.iter().any(|s| s.len() > 65536)
        {
            return Err(invalid());
        }
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = format!(
            "tool_{}-{}-{}",
            timestamp(),
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let name = format!("{id}.part");
        let path = self
            .root
            .join("tool-output")
            .join(&name)
            .to_str()
            .filter(|s| s.len() <= 4096 && !s.chars().any(char::is_control))
            .ok_or_else(invalid)?
            .to_owned();
        let file = openat(
            &self.output_dir,
            name.as_bytes(),
            libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        )?;
        let meta = file.metadata()?;
        let resource = Resource {
            id,
            operation: operation.into(),
            session: session.into(),
            location: location.into(),
            generation,
            source: source.into(),
            path,
            bytes: 0,
            lines: 0,
            admitted_bytes: 0,
            admitted_lines: 0,
            state: CaptureState::Active,
        };
        file.sync_all()?;
        self.output_dir.sync_all()?;
        File::open(&self.root)?.sync_all()?;
        let raw = serde_json::to_string(&resource).map_err(|_| invalid())?;
        if let Err(error) = conn.execute(
            "INSERT INTO tool_output_resources VALUES(?1,?2,?3,?4,?5,?6,?7,0,'Active',NULL)",
            params![
                resource.id,
                operation,
                session,
                name,
                raw,
                meta.dev() as i64,
                meta.ino() as i64
            ],
        ) {
            unlink(&self.output_dir, &name)?;
            self.output_dir.sync_all()?;
            return Err(error.into());
        }
        Ok(Writer {
            db: self.shared_handle(),
            file,
            resource,
            name,
            pending: String::new(),
            secrets,
            last_newline: false,
            admitted_newlines: 0,
            published_newlines: 0,
            finished: false,
        })
    }

    /// Exact session or its already-owned descendants; a copied reference, fork,
    /// moved placement, or child text does not create a new ancestry grant.
    pub(crate) fn open_tool_output(
        &self,
        session: &str,
        path: &str,
    ) -> Result<Reader, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        self.open_tool_output_in(&conn, session, path)
    }

    fn open_tool_output_in(
        &self,
        conn: &Connection,
        session: &str,
        path: &str,
    ) -> Result<Reader, StorageError> {
        self.verify_output_dir()?;
        let (raw,name,dev,ino,extent,state):(String,String,i64,i64,i64,String)=conn.query_row("WITH RECURSIVE owned(id,depth) AS (SELECT ?1,0 UNION ALL SELECT s.id,owned.depth+1 FROM sessions s JOIN owned ON s.parent_id=owned.id WHERE owned.depth<32) SELECT r.descriptor,r.name,r.dev,r.ino,r.extent,r.state FROM tool_output_resources r JOIN tool_operations o ON o.id=r.operation_id AND o.session_id=r.session_id WHERE r.session_id IN (SELECT id FROM owned) AND json_extract(r.descriptor,'$.path')=?2",params![session,path],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?.ok_or_else(invalid)?;
        let extent = u64::try_from(extent).map_err(|_| invalid())?;
        if state == "Expired" {
            return Err(invalid());
        }
        let resource: Resource = serde_json::from_str(&raw).map_err(|_| invalid())?;
        if resource.path != path
            || resource.bytes != extent
            || self.root.join("tool-output").join(&name) != Path::new(path)
            || name.contains('/')
        {
            return Err(invalid());
        }
        let file = openat(&self.output_dir, name.as_bytes(), libc::O_RDONLY)?;
        let meta = file.metadata()?;
        if !meta.is_file()
            || meta.dev() as i64 != dev
            || meta.ino() as i64 != ino
            || meta.len() < extent
            || (state != "Active" && meta.len() != extent)
        {
            return Err(invalid());
        }
        *self
            .output_readers
            .lock()
            .expect("reader leases")
            .entry(resource.id.clone())
            .or_default() += 1;
        Ok(Reader {
            db: self.shared_handle(),
            resource,
            file,
        })
    }

    pub(crate) fn output_for_operation(&self, op: &str) -> Result<Option<Resource>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw: Option<String> = conn
            .query_row(
                "SELECT descriptor FROM tool_output_resources WHERE operation_id=?1",
                [op],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|raw| serde_json::from_str(&raw).map_err(|_| invalid()))
            .transpose()
    }

    pub(crate) fn is_tool_output_path(&self, path: &str) -> bool {
        Path::new(path).is_absolute() && Path::new(path).starts_with(self.root.join("tool-output"))
    }

    pub(crate) fn record_output_execution(
        &self,
        op: &str,
        state: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute("INSERT INTO events(session_id,kind,payload) SELECT session_id,'tool_output_execution',?2 FROM tool_operations WHERE id=?1",params![op,serde_json::json!({"operation":op,"execution_state":state,"logging_state":"failed"}).to_string()])?;
        Ok(())
    }

    pub(crate) fn record_tool_output_question(
        &self,
        op: &str,
        session: &str,
    ) -> Result<(), StorageError> {
        let resource = self.output_for_operation(op)?.ok_or_else(invalid)?;
        if resource.session != session || resource.state != CaptureState::Complete {
            return Err(invalid());
        }
        let reference = QuestionResource {
            resource_id: resource.id,
            source_operation: op.into(),
            source_session: session.into(),
        };
        let payload = serde_json::json!({"operation":op,"question_resource":reference}).to_string();
        if payload.len() > 16384 {
            return Err(invalid());
        }
        let conn = self.conn.lock().expect("db mutex");
        let n=conn.execute("INSERT INTO events(session_id,kind,payload) SELECT session_id,'tool_output_question',?3 FROM tool_operations WHERE id=?1 AND session_id=?2 AND name='question'",params![op,session,payload])?;
        if n != 1 {
            return Err(invalid());
        }
        Ok(())
    }

    /// Presentation-only read of native facts. A transaction-copied fork event
    /// can present its source question, but cannot grant model read/grep access.
    /// Never called by wire/history/DCP reconstruction. Missing/expired/swapped
    /// resources leave the immutable bounded outcome/reference visible.
    pub(super) fn question_presentation_in(
        &self,
        conn: &Connection,
        name: &str,
        state: &str,
        raw: Option<&str>,
    ) -> Option<oc_core::question::QuestionResult> {
        use oc_core::question::{QuestionInput, QuestionResult, RESULT_BYTES_CAP};
        if let Some(result) = QuestionResult::from_output(name, state, raw) {
            return Some(result);
        }
        if name != "question" || state != "completed" {
            return None;
        }
        let reference: QuestionResource = serde_json::from_str(raw?).ok()?;
        let (descriptor, original_name): (String,String) = conn.query_row(
            "SELECT r.descriptor,o.name FROM tool_output_resources r JOIN tool_operations o ON o.id=r.operation_id AND o.session_id=r.session_id WHERE r.id=?1 AND r.operation_id=?2 AND r.session_id=?3 AND r.state='Complete'",
            params![reference.resource_id,reference.source_operation,reference.source_session], |r| Ok((r.get(0)?,r.get(1)?))).ok()?;
        if original_name != "question" {
            return None;
        }
        let resource: Resource = serde_json::from_str(&descriptor).ok()?;
        if resource.bytes > RESULT_BYTES_CAP as u64 {
            return None;
        }
        let lease = self
            .open_tool_output_in(conn, &reference.source_session, &resource.path)
            .ok()?;
        // Parsing is bounded by the original native question cap, not the generic
        // 16-MiB capture cap. Hold the identity-checked reader lease throughout.
        let result: QuestionResult =
            serde_json::from_reader(std::io::BufReader::new((&lease.file).take(resource.bytes)))
                .ok()?;
        let input = QuestionInput {
            questions: result.questions.clone(),
        };
        input.validate().ok()?;
        input.validate_answers(&result.answers).ok()?;
        Some(result)
    }

    /// Bounded owner cleanup. Immutable operation output/reference is retained.
    pub(crate) fn expire_tool_outputs(&self, now: i64) -> Result<usize, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        self.verify_output_dir()?;
        let readers = self.output_readers.lock().expect("reader leases");
        let rows:Vec<(String,String)>=conn.prepare("SELECT id,name FROM tool_output_resources WHERE state NOT IN ('Active','Expired') AND completed<=?1 ORDER BY completed LIMIT 128")?.query_map([now.saturating_sub(TTL)],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<_,_>>()?;
        let mut removed = 0;
        for (id, name) in rows {
            if readers.get(&id).copied().unwrap_or(0) > 0 {
                continue;
            }
            if name.contains('/') {
                return Err(invalid());
            }
            unlink(&self.output_dir, &name)?;
            self.output_dir.sync_all()?;
            conn.execute("UPDATE tool_output_resources SET state='Expired',descriptor=json_set(descriptor,'$.state','Expired') WHERE id=?1",[id])?;
            removed += 1;
        }
        // Crash temps/orphans count against quota until this bounded cleanup.
        for (inspected, entry) in fs::read_dir(self.root.join("tool-output"))?.enumerate() {
            if inspected >= 128 {
                break;
            }
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("tool_") || !(name.ends_with(".part") || name.ends_with(".txt")) {
                continue;
            }
            let id = name
                .strip_suffix(".part")
                .or_else(|| name.strip_suffix(".txt"))
                .expect("generated suffix");
            if readers.get(id).copied().unwrap_or(0) > 0 {
                continue;
            }
            let registered: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM tool_output_resources WHERE name=?1)",
                [&name],
                |r| r.get(0),
            )?;
            let meta = fs::symlink_metadata(entry.path())?;
            let age = meta
                .modified()?
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            if !registered && meta.is_file() && age <= now.saturating_sub(TTL) {
                unlink(&self.output_dir, &name)?;
                self.output_dir.sync_all()?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl Writer {
    /// Accept normalized UTF-8 chunks. Withhold enough suffix for any literal
    /// configured secret crossing a chunk boundary, before disk or live extent.
    pub(crate) fn append(&mut self, text: &str) -> Result<(), StorageError> {
        if self.finished {
            return Err(invalid());
        }
        // Bound intermediate redactor storage even when the caller supplies a
        // multi-megabyte admitted result in one call.
        let mut start = 0;
        while start < text.len() {
            let end = text.floor_char_boundary((start + 8192).min(text.len()));
            self.pending.push_str(&text[start..end]);
            self.flush_redacted(false)?;
            start = end;
        }
        self.publish_extent()
    }

    fn flush_redacted(&mut self, final_flush: bool) -> Result<(), StorageError> {
        let hold = self
            .secrets
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(0)
            .saturating_sub(1);
        let mut end = if final_flush {
            self.pending.len()
        } else {
            self.pending
                .floor_char_boundary(self.pending.len().saturating_sub(hold))
        };
        // A secret starting in the publishable prefix can end in the held suffix.
        // Move the boundary to its start; the next chunk sees the whole secret.
        for secret in &self.secrets {
            if secret.is_empty() {
                continue;
            }
            for (at, _) in self.pending.match_indices(secret) {
                if at < end && at + secret.len() > end {
                    end = at;
                }
            }
        }
        if end == 0 {
            return Ok(());
        }
        let mut admitted = self.pending[..end].to_owned();
        crate::tools::output::redact_string(&mut admitted, &self.secrets);
        self.pending.drain(..end);
        self.write_admitted(&admitted)
    }

    fn write_admitted(&mut self, text: &str) -> Result<(), StorageError> {
        self.resource.admitted_bytes = self
            .resource
            .admitted_bytes
            .checked_add(text.len() as u64)
            .ok_or(StorageError::StorageFull)?;
        self.admitted_newlines += text.bytes().filter(|b| *b == b'\n').count() as u64;
        if !text.is_empty() {
            self.last_newline = text.ends_with('\n');
        }
        self.resource.admitted_lines = self.admitted_newlines
            + u64::from(self.resource.admitted_bytes > 0 && !self.last_newline);
        let _conn = self.db.conn.lock().expect("db mutex");
        if self.resource.state == CaptureState::Active {
            let available = CAP.saturating_sub(self.resource.bytes);
            let quota = self.db.quota_bytes.saturating_sub(self.db.resource_used()?);
            let kept = text.floor_char_boundary(text.len().min(available.min(quota) as usize));
            if kept > 0 {
                if self.file.write_all(&text.as_bytes()[..kept]).is_err() {
                    self.file.set_len(self.resource.bytes)?;
                    self.file.sync_all()?;
                    self.resource.state = CaptureState::Io;
                } else {
                    self.resource.bytes += kept as u64;
                    self.published_newlines +=
                        text[..kept].bytes().filter(|b| *b == b'\n').count() as u64;
                    self.resource.lines = self.published_newlines
                        + u64::from(self.resource.bytes > 0 && !text[..kept].ends_with('\n'));
                }
            }
            if kept < text.len() && self.resource.state == CaptureState::Active {
                self.resource.state = if available <= quota {
                    CaptureState::ArtifactCap
                } else {
                    CaptureState::Quota
                };
            }
        }
        Ok(())
    }

    // One caller append is the publication boundary. Internal redactor chunks
    // stay bounded, quota-counted on disk, and unadvertised until synced here.
    fn publish_extent(&mut self) -> Result<(), StorageError> {
        let conn = self.db.conn.lock().expect("db mutex");
        self.file.sync_all()?;
        conn.execute(
            "UPDATE tool_output_resources SET descriptor=?2,extent=?3 WHERE id=?1",
            params![
                self.resource.id,
                serde_json::to_string(&self.resource).map_err(|_| invalid())?,
                self.resource.bytes as i64
            ],
        )?;
        Ok(())
    }

    pub(crate) fn finish(mut self, producer_limited: bool) -> Result<Resource, StorageError> {
        self.flush_redacted(true)?;
        self.publish_extent()?;
        let conn = self.db.conn.lock().expect("db mutex");
        self.db.verify_output_dir()?;
        self.file.sync_all()?;
        let name = format!("{}.txt", self.resource.id);
        let old = CString::new(self.name.as_str()).map_err(|_| invalid())?;
        let new = CString::new(name.as_str()).map_err(|_| invalid())?;
        // SAFETY: both basenames and the pinned directory descriptor are live.
        if unsafe {
            libc::renameat(
                self.db.output_dir.as_raw_fd(),
                old.as_ptr(),
                self.db.output_dir.as_raw_fd(),
                new.as_ptr(),
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        self.db.output_dir.sync_all()?;
        self.name = name;
        self.resource.path = self
            .db
            .root
            .join("tool-output")
            .join(&self.name)
            .to_string_lossy()
            .into_owned();
        if self.resource.state == CaptureState::Active {
            self.resource.state = if producer_limited {
                CaptureState::ProducerLimited
            } else {
                CaptureState::Complete
            };
        }
        let raw = serde_json::to_string(&self.resource).map_err(|_| invalid())?;
        conn.execute("UPDATE tool_output_resources SET name=?2,descriptor=?3,extent=?4,state=?5,completed=?6 WHERE id=?1",params![self.resource.id,self.name,raw,self.resource.bytes as i64,format!("{:?}",self.resource.state),timestamp()])?;
        self.finished = true;
        Ok(self.resource.clone())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // No execution restart. Registered published extent remains the only
        // readable prefix; an unregistered renamed orphan is never advertised.
        if let Ok(conn) = self.db.conn.lock() {
            // An append/publication failure must not promote unsynced bytes.
            if let Ok(raw) = conn.query_row(
                "SELECT descriptor FROM tool_output_resources WHERE id=?1",
                [&self.resource.id],
                |row| row.get::<_, String>(0),
            ) && let Ok(published) = serde_json::from_str::<Resource>(&raw)
            {
                self.resource = published;
                let _ = self.file.set_len(self.resource.bytes);
                let _ = self.file.sync_all();
            }
            self.resource.state = CaptureState::Interrupted;
            if let Ok(raw) = serde_json::to_string(&self.resource) {
                let _=conn.execute("UPDATE tool_output_resources SET descriptor=?2,state='Interrupted',completed=?3 WHERE id=?1",params![self.resource.id,raw,timestamp()]);
            }
        }
    }
}

impl Reader {
    pub(crate) fn byte_page(
        &mut self,
        offset: usize,
        limit: usize,
    ) -> Result<(String, i64, Option<i64>), StorageError> {
        let total = self.resource.bytes;
        if offset as u64 > total {
            return Err(invalid());
        }
        self.file.seek(SeekFrom::Start(offset as u64))?;
        let mut bytes = vec![0; limit.clamp(4, 65536).min((total - offset as u64) as usize)];
        self.file.read_exact(&mut bytes)?;
        if bytes.first().is_some_and(|b| b & 0xc0 == 0x80) {
            return Err(invalid());
        }
        let kept = complete_bytes(&bytes);
        let text = std::str::from_utf8(&bytes[..kept])
            .map_err(|_| invalid())?
            .to_owned();
        let next = offset + kept;
        Ok((
            text,
            total as i64,
            ((next as u64) < total).then_some(next as i64),
        ))
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        let mut leases = self.db.output_readers.lock().expect("reader leases");
        if let Some(count) = leases.get_mut(&self.resource.id) {
            *count -= 1;
            if *count == 0 {
                leases.remove(&self.resource.id);
            }
        }
    }
}

#[cfg(test)]
#[path = "storage_tool_output/tests.rs"]
mod tests;
