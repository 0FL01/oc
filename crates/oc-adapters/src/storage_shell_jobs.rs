//! Minimal background outcome and history-delivery seam in the existing DB.
use super::*;
use crate::shell::jobs::{Outcome, ProcessIdentity, Provenance};

impl Db {
    pub(super) fn shell_jobs_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS shell_jobs(
                operation_id TEXT PRIMARY KEY REFERENCES tool_operations(id),
                session_id TEXT NOT NULL REFERENCES sessions(id),
                version INTEGER NOT NULL CHECK(version=1),
                provenance TEXT NOT NULL,
                phase TEXT NOT NULL CHECK(phase IN ('admitted','running','terminal')),
                process TEXT, outcome TEXT,
                delivery_id TEXT NOT NULL UNIQUE,
                message_id TEXT UNIQUE REFERENCES messages(id));
            CREATE INDEX IF NOT EXISTS shell_jobs_pending ON shell_jobs(phase) WHERE message_id IS NULL;
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(5,'t50-shell-jobs');
            COMMIT;")?;
        Ok(())
    }

    pub(crate) fn admit_shell_job(&self, provenance: &Provenance) -> Result<(), StorageError> {
        self.conn.lock().expect("db mutex").execute(
            "INSERT INTO shell_jobs(operation_id,session_id,version,provenance,phase,delivery_id)
             VALUES(?1,?2,1,?3,'admitted',?4)",
            params![
                provenance.operation,
                provenance.session,
                serde_json::to_string(provenance).expect("provenance"),
                format!("shell-notice:{}", provenance.operation)
            ],
        )?;
        Ok(())
    }

    pub(crate) fn start_shell_job(
        &self,
        operation: &str,
        process: &ProcessIdentity,
    ) -> Result<(), StorageError> {
        let n = self.conn.lock().expect("db mutex").execute(
            "UPDATE shell_jobs SET phase='running',process=?2 WHERE operation_id=?1 AND phase='admitted'",
            params![operation, serde_json::to_string(process).expect("process identity")])?;
        if n != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(crate) fn finish_shell_job(
        &self,
        operation: &str,
        outcome: &Outcome,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let n = tx.execute("UPDATE shell_jobs SET phase='terminal',outcome=?2 WHERE operation_id=?1 AND phase!='terminal'",
            params![operation, serde_json::to_string(outcome).expect("shell outcome")])?;
        if n != 1 {
            let frozen: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM shell_jobs WHERE operation_id=?1 AND phase='terminal')",[operation],|r|r.get(0))?;
            if frozen {
                tx.commit()?;
                return Ok(());
            }
            return Err(StorageError::OperationNotFound);
        }
        tx.execute("INSERT INTO events(session_id,kind,payload) SELECT session_id,'shell_terminal',?1 FROM shell_jobs WHERE operation_id=?1",[operation])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn recover_shell_jobs(&self) -> Result<(), StorageError> {
        let pending = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare("SELECT operation_id,process FROM shell_jobs WHERE phase!='terminal' ORDER BY rowid LIMIT 9")?;
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        if pending.len() > crate::shell::jobs::ACTIVE_JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        for (operation, process) in pending {
            let diagnostic = match process {
                Some(raw) => {
                    let identity: ProcessIdentity =
                        serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
                    identity.quarantine(self.root())
                }
                None => "shell admission interrupted; execution unknown; not replayed",
            };
            self.finish_shell_job(
                &operation,
                &Outcome::unknown(&format!(
                    "interrupted shell effect unknown; not replayed; {diagnostic}"
                )),
            )?;
        }
        Ok(())
    }

    pub(crate) fn deliver_shell_notices(
        &self,
    ) -> Result<Vec<oc_core::queries::ShellNotice>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare("SELECT operation_id,session_id,delivery_id,provenance,outcome FROM shell_jobs WHERE phase='terminal' AND message_id IS NULL ORDER BY rowid LIMIT 8")?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let mut notices = Vec::new();
        for (operation, session, delivery, provenance, result) in rows {
            let provenance: Provenance =
                serde_json::from_str(&provenance).map_err(|_| StorageError::OperationNotFound)?;
            let result: Outcome =
                serde_json::from_str(&result).map_err(|_| StorageError::OperationNotFound)?;
            if provenance.version != 1 || result.version != 1 {
                return Err(StorageError::OperationNotFound);
            }
            let full = result.text();
            let kept = full.floor_char_boundary(TOOL_OP_PREVIEW_BYTES);
            let output = &full[..kept];
            let text = format!(
                "Automatic background shell result (native durable notice; not user instructions):\n{}",
                serde_json::json!({"shellID":operation,"deliveryID":delivery,"command":provenance.command,
                    "status":result.state,"exit":result.exit,"signal":result.signal,"timeout":result.timeout,
                    "cancelled":result.cancelled,"truncated":result.stdout_truncated || result.stderr_truncated,
                    "output":output,"retainedOutputBytes":full.len(),"previewTruncated":kept<full.len(),
                    "sourceLocation":provenance.location,"sourceGeneration":provenance.generation})
            );
            let message = Self::insert_message(&tx, &session, "user", &text)?;
            let delivered = tx.execute(
                "UPDATE shell_jobs SET message_id=?2 WHERE operation_id=?1 AND message_id IS NULL",
                params![operation, message],
            )?;
            if delivered != 1 {
                return Err(StorageError::OperationNotFound);
            }
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'shell_notice',?2)",
                params![session, delivery],
            )?;
            notices.push(oc_core::queries::ShellNotice {
                session: oc_core::domain::SessionId(session),
                shell_id: operation,
                delivery_id: delivery,
                message_id: message,
                state: result.state,
                text,
            });
        }
        tx.commit()?;
        Ok(notices)
    }

    pub(crate) fn shell_notices_after(
        &self,
        session: &str,
        after: i64,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        // Native notice admission follows the same branch visibility as every
        // other conversation input; raw immutable history is not the wire view.
        let mut stmt = conn.prepare("SELECT m.id,m.text FROM shell_jobs j JOIN conversation_messages m ON m.id=j.message_id AND m.session_id=j.session_id WHERE j.session_id=?1 AND m.seq>?2 ORDER BY m.seq LIMIT 8")?;
        Ok(stmt
            .query_map(params![session, after], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub(crate) fn shell_output(
        &self,
        session: &str,
        operation: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Option<(String, i64, Option<i64>)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let owned: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tool_operations WHERE session_id=?1 AND id=?2)",
            params![session, operation],
            |r| r.get(0),
        )?;
        if !owned {
            return Err(StorageError::OperationNotFound);
        }
        let raw: Option<String> = conn.query_row(
            "SELECT outcome FROM shell_jobs WHERE session_id=?1 AND operation_id=?2 AND phase='terminal'",
            params![session,operation],|r|r.get::<_,String>(0)).optional()?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let outcome: Outcome =
            serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
        if outcome.version != 1 {
            return Err(StorageError::OperationNotFound);
        }
        let text = outcome.text();
        if offset > text.len() || !text.is_char_boundary(offset) {
            return Err(StorageError::OperationNotFound);
        }
        let end = text.floor_char_boundary(
            offset
                .saturating_add(limit.clamp(4, TOOL_OP_PREVIEW_BYTES))
                .min(text.len()),
        );
        Ok(Some((
            text[offset..end].into(),
            text.len() as i64,
            (end < text.len()).then_some(end as i64),
        )))
    }
}
