//! Child generation/terminal/delivery facts in the existing journal owner.
use super::*;
use crate::tools::diagnostic::Diagnostic;
use oc_core::queries::{ChildHistory, ChildJob, ChildNotice, ChildState};

#[path = "storage_children/parent.rs"]
mod parent;

#[cfg(test)]
#[path = "storage_children/tests.rs"]
mod tests;

impl Db {
    /// Read only owner-authored metadata from the exact accepted child turn.
    /// The connection-taking form also serves notice delivery's transaction.
    fn child_diagnostic_in(
        conn: &Connection,
        operation: &str,
    ) -> Result<Option<Diagnostic>, StorageError> {
        let raw: Option<String> = conn.prepare_cached("SELECT CASE WHEN json_valid(t.result) AND json_valid(j.identity) THEN CASE WHEN t.status IN ('completed','cancelled','interrupted','failed','incomplete')
              AND json_extract(t.result,'$.display.owning_operation')=j.operation_id
              AND json_extract(j.identity,'$.operation')=j.operation_id
              AND json_extract(j.identity,'$.child')=j.child_id
              AND json_extract(j.identity,'$.parent')=j.parent_id
              AND length(CAST(json_extract(t.result,'$.display.terminal_diagnostic') AS BLOB))<=8192
              THEN json_extract(t.result,'$.display.terminal_diagnostic') END END
             FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id
             JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent'
             LEFT JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id WHERE j.operation_id=?1")?.query_row([operation], |row| row.get(0),
        ).optional()?.flatten();
        Ok(raw.and_then(|raw| serde_json::from_str::<Diagnostic>(&raw).ok()))
    }

    pub(crate) fn child_diagnostic(
        &self,
        operation: &str,
    ) -> Result<Option<Diagnostic>, StorageError> {
        Self::child_diagnostic_in(&self.conn.lock().expect("db mutex"), operation)
    }

    pub(crate) fn child_terminal_fact(
        &self,
        operation: &str,
    ) -> Result<Option<(ChildState, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let (status, text): (Option<String>, Option<String>) = conn.prepare_cached("SELECT t.status,m.text FROM child_jobs j LEFT JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id
             LEFT JOIN messages m ON m.id=CASE WHEN json_valid(t.result) THEN json_extract(t.result,'$.assistant_message') END AND m.session_id=j.child_id
             WHERE j.operation_id=?1")?.query_row([operation], |r| Ok((r.get(0)?,r.get(1)?)))?;
        let diagnostic = Self::child_diagnostic_in(&conn, operation)?;
        let reason = diagnostic.as_ref().map(|d| d.message.clone());
        Ok(match status.as_deref() {
            Some("completed") => Some((
                ChildState::Completed,
                text.unwrap_or_else(|| crate::tools::SUBAGENT_NO_TEXT.into()),
            )),
            Some("cancelled") | Some("interrupted") if diagnostic.is_some() => Some((
                if diagnostic.as_ref().is_some_and(|d| d.code == "infra_error") {
                    ChildState::Error
                } else {
                    ChildState::Cancelled
                },
                reason.unwrap_or_else(|| "Subagent cancelled".into()),
            )),
            Some("cancelled") => Some((ChildState::Cancelled, "Subagent cancelled".into())),
            Some("failed" | "incomplete") => Some((
                ChildState::Error,
                reason.unwrap_or_else(|| "Child execution failed".into()),
            )),
            _ => None,
        })
    }

    /// Display/read-only card association from the original launch transaction.
    /// Never recovers a child capability from tool output or copied fork input.
    pub(super) fn child_tool_job_in(
        conn: &Connection,
        session: &str,
        operation: &str,
    ) -> Result<Option<ChildJob>, StorageError> {
        let row = conn.prepare_cached("SELECT j.identity,j.state,j.parent_id,j.child_id,j.delivery_id,j.child_turn,j.message_id,
             EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.parent_id AND e.kind='subagent_background' AND e.payload=j.operation_id),
             (SELECT e.payload FROM events e WHERE e.session_id=j.parent_id AND e.kind='subagent_launch'
              AND length(CAST(e.payload AS BLOB))<=?3
              AND CASE WHEN json_valid(e.payload) THEN json_extract(e.payload,'$.operation')=j.operation_id ELSE 0 END LIMIT 1)
             FROM child_jobs j JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent'
             JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id
             WHERE j.parent_id=?1 AND j.operation_id=?2 AND length(CAST(j.identity AS BLOB))<=?3")?.query_row(params![session,operation,oc_core::tool_output::RECORD_BYTES as i64],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,Option<String>>(5)?,row.get::<_,Option<String>>(6)?,row.get::<_,bool>(7)?,row.get::<_,Option<String>>(8)?)),
        ).optional()?;
        let Some((raw, state, parent, child, delivery, turn, message, converted, launch)) = row
        else {
            return Ok(None);
        };
        let Ok(mut job) = Self::decode_child_job(raw, state, None, message) else {
            return Ok(None);
        };
        let Some(launch) =
            launch.and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        else {
            return Ok(None);
        };
        if job.parent.0 != parent
            || job.child.0 != child
            || job.operation != operation
            || job.delivery_id != delivery
            || launch["operation"].as_str() != Some(operation)
            || launch["origin_operation"].as_str() != Some(operation)
            || launch["parentID"].as_str() != Some(parent.as_str())
            || launch["childID"].as_str() != Some(child.as_str())
            || launch["deliveryID"].as_str() != Some(delivery.as_str())
            || launch["location"].as_str() != Some(job.location.as_str())
            || launch["generation"].as_u64() != Some(job.generation)
            || [
                &job.parent.0,
                &job.child.0,
                &job.operation,
                &job.delivery_id,
                &job.location,
                &job.agent,
                &job.model,
            ]
            .iter()
            .any(|field| field.len() > 4096)
            || turn.as_ref().is_some_and(|turn| turn.len() > 4096)
            || job.message_id.as_ref().is_some_and(|id| id.len() > 4096)
        {
            return Ok(None);
        }
        job.background |= converted;
        job.turn = turn;
        Ok(Self::child_job_view(job))
    }

    pub(crate) fn child_tool_job(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<Option<ChildJob>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::child_tool_job_in(&conn, session, operation)
    }

    /// Exact original owner identity. Only result consumers opt into the full
    /// report; read-only family/status fences omit it before SQL allocation.
    /// UI preview limits must not reject or truncate an accepted identity here.
    pub(crate) fn child_job(
        &self,
        session: &str,
        operation: &str,
        include_result: bool,
    ) -> Result<Option<ChildJob>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let row = conn.prepare_cached("SELECT j.identity,j.state,CASE WHEN ?3 THEN j.result END,j.message_id,j.parent_id,j.child_id,j.delivery_id,j.child_turn,
             EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.parent_id AND e.kind='subagent_background' AND e.payload=j.operation_id)
             FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id
             JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent'
             WHERE j.parent_id=?1 AND j.operation_id=?2")?.query_row(params![session,operation,include_result],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?,row.get::<_,Option<String>>(7)?,row.get::<_,bool>(8)?)),
        ).optional()?;
        let Some((raw, state, result, message, parent, child, delivery, turn, converted)) = row
        else {
            return Ok(None);
        };
        let job = Self::decode_child_job(raw, state, result, message)?;
        if job.parent.0 != parent
            || job.child.0 != child
            || job.operation != operation
            || job.delivery_id != delivery
        {
            return Ok(None);
        }
        Ok(Some(ChildJob {
            background: job.background || converted,
            turn,
            ..job
        }))
    }

    /// UI-only classification by journal identities, never by message prose.
    pub(crate) fn child_history(
        &self,
        session: &str,
        message: &oc_core::session::MessageId,
    ) -> Result<Option<ChildHistory>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        if !conn
            .prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM conversation_messages WHERE session_id=?1 AND id=?2)",
            )?
            .query_row(params![session, message.0], |row| row.get::<_, bool>(0))?
        {
            return Ok(None);
        }
        if let Some(job) = Self::child_notice_in(&conn, session, &message.0)? {
            return Ok(Some(ChildHistory::Notice(Box::new(job))));
        }
        if Self::native_notice_data_in(&conn, session, &message.0)?
            && conn.prepare_cached("SELECT EXISTS(SELECT 1 FROM turn_acceptances WHERE session_id=?1 AND user_message=?2)")?.query_row(params![session,message.0],|row|row.get::<_,bool>(0))? {
            return Ok(Some(ChildHistory::NativeControl));
        }
        // Read the original structured tool argument of this exact acceptance,
        // not delimiters inside its transformed native host/context-pack text.
        let task = conn.prepare_cached("SELECT substr(CAST(json_extract(o.input,'$.prompt') AS BLOB),1,65536),length(CAST(json_extract(o.input,'$.prompt') AS BLOB))
             FROM conversation_messages m JOIN turn_acceptances a ON a.user_message=m.id AND a.session_id=m.session_id
             JOIN child_jobs j ON j.child_turn=a.turn_id AND j.child_id=a.session_id
             JOIN turns t ON t.id=a.turn_id AND t.session_id=j.child_id
             JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id
             JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent'
             WHERE m.session_id=?1 AND m.id=?2 AND m.role='user'
             AND CASE WHEN json_valid(o.input) AND json_valid(j.identity) THEN
                json_type(o.input,'$.prompt')='text'
                AND json_extract(j.identity,'$.operation')=j.operation_id
                AND json_extract(j.identity,'$.parent')=j.parent_id
                AND json_extract(j.identity,'$.child')=j.child_id
                AND json_extract(j.identity,'$.delivery_id')=j.delivery_id
                AND json_extract(j.identity,'$.agent')=json_extract(o.input,'$.agent')
             ELSE 0 END")?.query_row(params![session,message.0],
            |row| Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,i64>(1)?)),
        ).optional()?;
        Ok(task.map(|(text, bytes)| {
            let end = std::str::from_utf8(&text).map_or_else(|error| error.valid_up_to(), str::len);
            ChildHistory::Task {
                text: String::from_utf8_lossy(&text[..end]).into_owned(),
                limited: bytes > end as i64,
            }
        }))
    }

    /// Exact delivery identity, including hidden RAW rows for Revert counting.
    /// Display callers independently require the current conversation view.
    pub(super) fn child_notice_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<Option<ChildJob>, StorageError> {
        let notice = conn.prepare_cached("SELECT j.identity,j.state,j.parent_id,j.child_id,j.operation_id,j.delivery_id,j.child_turn
             FROM messages m JOIN child_jobs j ON j.message_id=m.id AND j.parent_id=m.session_id
             JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id
             JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent'
             WHERE m.session_id=?1 AND m.id=?2 AND m.role='user' AND j.state NOT IN ('admitted','running')
             AND length(CAST(j.identity AS BLOB))<=?3
             AND EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.parent_id AND e.kind='subagent_notice' AND e.payload=j.delivery_id)")?.query_row(params![session,message,oc_core::tool_output::RECORD_BYTES as i64],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,Option<String>>(6)?)),
        ).optional()?;
        if let Some((raw, state, parent, child, operation, delivery, turn)) = notice {
            let Ok(mut job) = Self::decode_child_job(raw, state, None, Some(message.to_owned()))
            else {
                return Ok(None);
            };
            if job.parent.0 != parent
                || job.child.0 != child
                || job.operation != operation
                || job.delivery_id != delivery
                || [
                    &job.parent.0,
                    &job.child.0,
                    &job.operation,
                    &job.delivery_id,
                    &job.location,
                    &job.agent,
                    &job.model,
                ]
                .iter()
                .any(|field| field.len() > 4096)
                || turn.as_ref().is_some_and(|turn| turn.len() > 4096)
            {
                return Ok(None);
            }
            // The exact delivery event proves conversion even for foreground
            // launch identities. Result prose is not needed by this UI link.
            job.background = true;
            job.turn = turn;
            return Ok(Self::child_job_view(job));
        }
        Ok(None)
    }

    pub(crate) fn child_launch_fingerprint(&self, operation: &str) -> Result<String, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let facts = conn.prepare_cached("SELECT id,session_id,turn_id,input,provider_call_id,call_occurrence,original_input_index FROM tool_operations WHERE id=?1 AND name='subagent'")?.query_row([operation],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,String>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,Option<i64>>(5)?,r.get::<_,Option<i64>>(6)?)))?;
        Ok(crate::compaction::fingerprint(&facts))
    }
    pub(crate) fn child_job_unresolved(&self, child: &str) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row("SELECT EXISTS(SELECT 1 FROM child_jobs WHERE child_id=?1 AND state IN ('admitted','running','unknown'))",[child],|row|row.get(0))?)
    }
    pub(crate) fn child_job_outstanding(&self) -> Result<i64, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT count(*) FROM child_jobs j WHERE state IN ('admitted','running','unknown') OR (message_id IS NULL AND (coalesce(json_extract(identity,'$.background'),1)=1 OR EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id)))",
            [],
            |row| row.get(0),
        )?)
    }
    pub(crate) fn child_job_live(&self, operation: &str) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row("SELECT EXISTS(SELECT 1 FROM child_jobs WHERE operation_id=?1 AND state IN ('admitted','running'))",[operation],|r|r.get(0))?)
    }
    pub(crate) fn child_launch_operation(
        &self,
        turn: &str,
        call: &str,
    ) -> Result<String, StorageError> {
        self.conn.lock().expect("db mutex").query_row("SELECT id FROM tool_operations WHERE turn_id=?1 AND provider_call_id=?2 AND name='subagent' AND state='started' ORDER BY rowid DESC LIMIT 1",params![turn,call],|r|r.get(0)).map_err(Into::into)
    }
    pub(super) fn child_jobs_schema(conn: &Connection) -> Result<(), StorageError> {
        if conn
            .prepare_cached("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=10)")?
            .query_row([], |row| row.get::<_, bool>(0))?
        {
            return Ok(());
        }
        let tx =
            rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
        tx.execute_batch("
            CREATE TABLE IF NOT EXISTS child_jobs(
                operation_id TEXT PRIMARY KEY REFERENCES tool_operations(id),
                parent_id TEXT NOT NULL REFERENCES sessions(id),
                child_id TEXT NOT NULL REFERENCES sessions(id),
                identity TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('admitted','running','completed','cancelled','error','unknown')),
                child_turn TEXT REFERENCES turns(id),
                result TEXT,
                delivery_id TEXT NOT NULL UNIQUE,
                message_id TEXT UNIQUE REFERENCES messages(id));
            CREATE UNIQUE INDEX IF NOT EXISTS child_job_live ON child_jobs(child_id) WHERE state IN ('admitted','running');
            CREATE INDEX IF NOT EXISTS child_job_family ON child_jobs(parent_id);
            CREATE INDEX IF NOT EXISTS child_job_pending ON child_jobs(state) WHERE message_id IS NULL;
            INSERT INTO schema_migrations(version,applied_at) VALUES(10,'t45-child-jobs');")?;
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn admit_fresh_child_job(&self, job: &ChildJob) -> Result<(), StorageError> {
        self.admit_child_job_inner(job, true, None)
    }

    #[cfg(test)]
    pub(crate) fn admit_child_job(&self, job: &ChildJob) -> Result<(), StorageError> {
        self.admit_child_job_inner(job, false, None)
    }

    pub(crate) fn admit_recoverable_child_job(
        &self,
        job: &ChildJob,
        fresh: bool,
        fence: &serde_json::Value,
    ) -> Result<(), StorageError> {
        self.admit_child_job_inner(job, fresh, Some(fence))
    }

    /// Fresh child, Location binding and generation have one commit boundary.
    /// A losing reservation or failed COMMIT has never published a child.
    fn admit_child_job_inner(
        &self,
        job: &ChildJob,
        fresh: bool,
        fence: Option<&serde_json::Value>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let valid: bool = tx.prepare_cached("SELECT EXISTS(SELECT 1 FROM tool_operations o WHERE o.id=?2 AND o.session_id=?1 AND o.name='subagent')")?.query_row(params![job.parent.0,job.operation], |r| r.get(0))?;
        let count: i64 = tx.prepare_cached("SELECT count(*) FROM child_jobs j WHERE state IN ('admitted','running','unknown') OR (message_id IS NULL AND (coalesce(json_extract(identity,'$.background'),1)=1 OR EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id)))")?.query_row([],
            |r| r.get(0),
        )?;
        if !valid || count >= crate::runtime::children::JOB_CAP as i64 {
            return Err(StorageError::OperationNotFound);
        }
        if fresh {
            Self::require_session(&tx, &job.parent.0)?;
            Self::insert_session_row(
                &tx,
                &job.child.0,
                Some(&job.parent.0),
                Some(&job.agent),
                Some(&job.model),
                Some(&job.description),
            )?;
            Self::insert_location_binding(&tx, &job.child.0, &job.location)?;
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'session_created','{}')",
                [&job.child.0],
            )?;
        } else {
            let child: bool = tx
                .prepare_cached(
                    "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?2 AND parent_id=?1)",
                )?
                .query_row(params![job.parent.0, job.child.0], |r| r.get(0))?;
            if !child {
                return Err(StorageError::SessionNotFound);
            }
        }
        tx.execute("INSERT INTO child_jobs(operation_id,parent_id,child_id,identity,state,delivery_id) VALUES(?1,?2,?3,?4,'admitted',?5)", params![job.operation,job.parent.0,job.child.0,serde_json::to_string(job).expect("child identity"),job.delivery_id])?;
        if let Some(fence) = fence {
            tx.execute("INSERT INTO events(session_id,kind,payload) VALUES(?1,'subagent_recovery_fence',?2)", params![job.child.0,serde_json::json!({"operation":job.operation,"fence":fence}).to_string()])?;
        }
        tx.execute("INSERT INTO events(session_id,kind,payload) VALUES(?1,'subagent_launch',?2)",params![job.parent.0,serde_json::json!({"operation":job.operation,"origin_operation":job.operation,"childID":job.child.0,"parentID":job.parent.0,"generation":job.generation,"location":job.location,"deliveryID":job.delivery_id}).to_string()])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn start_child_job(&self, operation: &str, turn: &str) -> Result<(), StorageError> {
        let n = self.conn.lock().expect("db mutex").execute("UPDATE child_jobs SET state='running',child_turn=?2 WHERE operation_id=?1 AND state='admitted' AND EXISTS(SELECT 1 FROM turns t WHERE t.id=?2 AND t.session_id=child_jobs.child_id)",params![operation,turn])?;
        if n != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    /// Serialize conversion with terminal settlement in the existing journal.
    pub(crate) fn background_child_job(&self, operation: &str) -> Result<bool, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let parent: Option<String> = tx.prepare_cached("SELECT parent_id FROM child_jobs WHERE operation_id=?1 AND state IN ('admitted','running')")?.query_row([operation], |r| r.get(0)).optional()?;
        let Some(parent) = parent else {
            return Ok(false);
        };
        tx.execute("INSERT INTO events(session_id,kind,payload) SELECT ?1,'subagent_background',?2 WHERE NOT EXISTS(SELECT 1 FROM events WHERE session_id=?1 AND kind='subagent_background' AND payload=?2)", params![parent,operation])?;
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn child_job_active(&self, operation: &str) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row("SELECT EXISTS(SELECT 1 FROM child_jobs WHERE operation_id=?1 AND state IN ('admitted','running'))",[operation],|r| r.get(0))?)
    }

    pub(crate) fn finish_child_job(
        &self,
        operation: &str,
        state: ChildState,
        result: &str,
    ) -> Result<(), StorageError> {
        let state = match state {
            ChildState::Completed => "completed",
            ChildState::Cancelled => "cancelled",
            ChildState::Error => "error",
            ChildState::Unknown => "unknown",
            _ => return Err(StorageError::OperationNotFound),
        };
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        tx.execute("UPDATE child_jobs SET state=?2,result=?3 WHERE operation_id=?1 AND state IN ('admitted','running')",params![operation,state,result])?;
        tx.commit()?;
        Ok(())
    }

    fn decode_child_job(
        raw: String,
        state: String,
        result: Option<String>,
        message: Option<String>,
    ) -> Result<ChildJob, StorageError> {
        let mut job: ChildJob =
            serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
        job.state = serde_json::from_value(serde_json::Value::String(state))
            .map_err(|_| StorageError::OperationNotFound)?;
        job.result = result;
        job.message_id = message;
        Ok(job)
    }

    #[cfg(test)]
    pub(crate) fn child_jobs(&self, session: &str) -> Result<Vec<ChildJob>, StorageError> {
        // Test inspection only. Production status reads never load report bodies;
        // foreground consumers use the exact operation rather than this window.
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut stmt = conn.prepare("SELECT j.identity,j.state,j.result,j.message_id,EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id),j.child_turn FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id WHERE j.parent_id=?1 ORDER BY (j.state IN ('admitted','running')) DESC,j.rowid DESC LIMIT 16")?;
        stmt.query_map([session], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?
        .map(|row| {
            let (raw, state, result, message, converted, turn) = row?;
            let mut job = Self::decode_child_job(raw, state, result, message)?;
            job.background |= converted;
            job.turn = turn;
            Ok(job)
        })
        .collect()
    }

    /// Bounded positive launch projections for the existing frontend inventory.
    pub(crate) fn child_tool_jobs(&self, session: &str) -> Result<Vec<ChildJob>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        let mut stmt = conn.prepare("SELECT j.operation_id FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id WHERE j.parent_id=?1 ORDER BY (j.state IN ('admitted','running')) DESC,j.rowid DESC LIMIT 16")?;
        let rows = stmt.query_map([session], |r| r.get::<_, String>(0))?;
        let mut jobs = Vec::new();
        for row in rows {
            let operation = row?;
            // Live inventory and cold tool projection use the same positive
            // original-launch fence; neither copied nor malformed data can link.
            if let Some(job) = Self::child_tool_job_in(&conn, session, &operation)? {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    pub(crate) fn recover_child_jobs(&self) -> Result<(), StorageError> {
        let rows = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare("SELECT j.operation_id FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id WHERE j.state IN ('admitted','running') ORDER BY j.rowid LIMIT 9")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if rows.len() > crate::runtime::children::JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        for operation in rows {
            if let Some((state, text)) = self.child_terminal_fact(&operation)? {
                self.finish_child_job(&operation, state, &text)?;
                continue;
            }
            // Resolve one exact canonical assistant at a time, never a preview
            // or the child's latest message. The batch above retains only IDs.
            let (status, result) = {
                let conn = self.conn.lock().expect("db mutex");
                conn.prepare_cached("SELECT t.status,m.text FROM child_jobs j LEFT JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id LEFT JOIN messages m ON json_valid(t.result) AND m.id=json_extract(t.result,'$.assistant_message') AND m.session_id=j.child_id WHERE j.operation_id=?1")?.query_row([&operation], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?)))?
            };
            let (state,text) = match status.as_deref() {
                Some("completed") => {
                    let text = result.unwrap_or_else(|| crate::tools::SUBAGENT_NO_TEXT.to_owned());
                    (ChildState::Completed,text)
                }
                Some("cancelled") => (ChildState::Cancelled,"Subagent cancelled".into()),
                Some("failed") => (ChildState::Error,"Child execution failed".into()),
                Some("started") if self.child_checkpoint_safe(&operation).unwrap_or(false) => continue,
                _ => (ChildState::Unknown,"Child execution unresolved after interruption; effects may be unknown; explicit recovery required; not replayed".into()),
            };
            self.finish_child_job(&operation, state, &text)?;
        }
        Ok(())
    }

    /// Positive, typed journal safety. No pending call is executed by recovery.
    /// A completed R9 task renewal is an atomic native HOT commit with no
    /// external effect; other compress outcomes stay conservatively unsafe.
    fn child_checkpoint_safe(&self, operation: &str) -> Result<bool, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw: Option<String> = conn.prepare_cached("SELECT t.result FROM child_jobs j JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id WHERE j.operation_id=?1")?.query_row([operation], |r|r.get(0))?;
        let Some(raw) = raw.filter(|s| s.len() <= crate::runtime::ACTIVE_CONTEXT_BYTES_CAP) else {
            return Ok(false);
        };
        let log = serde_json::from_str(&raw)
            .ok()
            .and_then(|v| crate::tools::TurnLog::from_json(&v).ok());
        let Some(log) = log else {
            return Ok(false);
        };
        let unsafe_ops: bool = conn.prepare_cached("SELECT EXISTS(SELECT 1 FROM tool_operations o JOIN child_jobs j ON j.child_turn=o.turn_id WHERE j.operation_id=?1 AND (o.state NOT IN ('completed','failed','denied') OR (o.name NOT IN ('read','glob','grep','webfetch','skill','opencode_models') AND NOT (o.name='compress' AND o.state='completed' AND json_extract(o.output,'$.status')='task_renewal_accepted'))))")?.query_row([operation], |r|r.get(0))?;
        if unsafe_ops {
            return Ok(false);
        }
        let working = log
            .working
            .as_ref()
            .map(crate::tools::TurnLog::from_json)
            .transpose()
            .map_err(|_| StorageError::OperationNotFound)?;
        let mut pending = std::collections::BTreeMap::new();
        for (source, index, item) in
            working
                .iter()
                .chain(std::iter::once(&log))
                .flat_map(|source| {
                    source
                        .input
                        .iter()
                        .enumerate()
                        .map(move |(i, item)| (source, i, item))
                })
        {
            if let crate::provider::InputItem::ProviderOutput(v) = item
                && v["type"] == "function_call"
            {
                let Some(id) = v["call_id"].as_str() else {
                    return Ok(false);
                };
                let Some(name) = v["name"].as_str().filter(|n| {
                    matches!(
                        *n,
                        "read" | "glob" | "grep" | "webfetch" | "skill" | "opencode_models"
                    )
                }) else {
                    return Ok(false);
                };
                let Some(arguments) = v["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                else {
                    return Ok(false);
                };
                if pending
                    .insert(
                        id.to_owned(),
                        (
                            name.to_owned(),
                            arguments.to_string(),
                            source.original_input_index(index),
                        ),
                    )
                    .is_some()
                {
                    return Ok(false);
                }
            }
            if let Some((id, output)) = item.call_output() {
                let Some((name, arguments, original_index)) = pending.remove(id) else {
                    return Ok(false);
                };
                let causal: bool = conn.prepare_cached("SELECT EXISTS(SELECT 1 FROM tool_operations o WHERE o.turn_id=?1 AND o.session_id=(SELECT child_id FROM child_jobs WHERE operation_id=?2) AND o.provider_call_id=?3 AND o.original_input_index=?4 AND o.name=?5 AND o.input=?6 AND o.output=?7 AND o.state IN ('completed','failed','denied'))")?.query_row(params![log.turn_id,operation,id,original_index as i64,name,arguments,output],|r|r.get(0))?;
                if !causal {
                    return Ok(false);
                }
            }
        }
        Ok(pending.is_empty())
    }

    pub(crate) fn child_recovery_candidates(&self) -> Result<Vec<ChildJob>, StorageError> {
        let jobs = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare("SELECT j.identity,j.state,j.message_id,j.child_turn,EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id) FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id WHERE j.state IN ('admitted','running') ORDER BY j.rowid LIMIT 9")?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(raw, state, message, turn, converted)| {
                let mut job = Self::decode_child_job(raw, state, None, message)?;
                job.turn = turn;
                job.background |= converted;
                Ok(job)
            })
            .collect::<Result<Vec<_>, StorageError>>()?
        };
        if jobs.len() > crate::runtime::children::JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        Ok(jobs)
    }

    pub(crate) fn child_recovery_fence(
        &self,
        job: &ChildJob,
    ) -> Result<serde_json::Value, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw: String = conn.prepare_cached("SELECT payload FROM events WHERE session_id=?1 AND kind='subagent_recovery_fence' AND json_extract(payload,'$.operation')=?2 ORDER BY seq LIMIT 1")?.query_row(params![job.child.0,job.operation], |r|r.get(0))?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
        Ok(value["fence"].clone())
    }

    /// The data-root lock proves previous ownership dead. Attempts and claim are
    /// one durable fact before dispatch; a crash leaves the same task resumable.
    pub(crate) fn claim_child_resume(
        &self,
        job: &ChildJob,
        checkpoint: &str,
    ) -> Result<(), StorageError> {
        if !self.child_checkpoint_safe(&job.operation)? {
            return Err(StorageError::OperationNotFound);
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let valid: bool = tx.prepare_cached("SELECT EXISTS(SELECT 1 FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent' JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id JOIN turn_acceptances a ON a.turn_id=t.id AND a.session_id=c.id JOIN conversation_messages m ON m.id=a.user_message AND m.session_id=c.id JOIN prefs l ON l.key='tui.session_location.'||c.id WHERE j.operation_id=?1 AND j.parent_id=?2 AND j.child_id=?3 AND j.delivery_id=?4 AND j.state='running' AND t.status='started' AND t.result=?5 AND a.user_message=json_extract(t.result,'$.user_message') AND json_extract(a.model_ref,'$.provider')=json_extract(t.result,'$.provider') AND json_extract(a.model_ref,'$.id')=json_extract(t.result,'$.model') AND l.value=?6 AND c.agent=?7 AND c.model=?8)")?.query_row(params![job.operation,job.parent.0,job.child.0,job.delivery_id,checkpoint,job.location,job.agent,job.model],|r|r.get(0))?;
        let attempts: i64 = tx.prepare_cached("SELECT count(*) FROM events WHERE session_id=?1 AND kind='subagent_resume_claim' AND json_extract(payload,'$.operation')=?2")?.query_row(params![job.child.0,job.operation],|r|r.get(0))?;
        if !valid || attempts >= 10 {
            return Err(StorageError::OperationNotFound);
        }
        tx.execute("INSERT INTO events(session_id,kind,payload) VALUES(?1,'subagent_resume_claim',?2)",params![job.child.0,serde_json::json!({"operation":job.operation,"turn":job.turn,"attempt":attempts+1,"checkpoint":crate::compaction::fingerprint(&checkpoint)}).to_string()])?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn deliver_child_notices(&self) -> Result<Vec<ChildNotice>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare("SELECT j.operation_id FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id WHERE j.state NOT IN ('admitted','running') AND j.message_id IS NULL AND (coalesce(json_extract(j.identity,'$.background'),1)=1 OR EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id)) ORDER BY j.rowid LIMIT 8")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut notices = Vec::new();
        for operation in rows {
            let (raw, state, result, turn) = tx
                .prepare_cached(
                    "SELECT identity,state,result,child_turn FROM child_jobs WHERE operation_id=?1",
                )?
                .query_row([&operation], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })?;
            let mut job = Self::decode_child_job(raw, state, result, None)?;
            let valid: bool = tx.prepare_cached("SELECT EXISTS(SELECT 1 FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent' WHERE j.operation_id=?1 AND j.parent_id=?2 AND j.child_id=?3 AND j.delivery_id=?4 AND j.state=?5 AND json_extract(j.identity,'$.operation')=j.operation_id AND json_extract(j.identity,'$.parent')=j.parent_id AND json_extract(j.identity,'$.child')=j.child_id AND json_extract(j.identity,'$.delivery_id')=j.delivery_id)")?.query_row(params![job.operation,job.parent.0,job.child.0,job.delivery_id,serde_json::to_value(job.state).expect("state").as_str()],|r|r.get(0))?;
            if !valid {
                return Err(StorageError::OperationNotFound);
            }
            job.background = true;
            job.turn = turn;
            let mut facts = serde_json::json!({"source":"subagent","childID":job.child.0,"agent":job.agent,"state":job.state,"description":job.description,"result":job.result,"jobGeneration":job.operation,"deliveryID":job.delivery_id,"sourceLocation":job.location,"sourceGeneration":job.generation});
            if let Some(diagnostic) = Self::child_diagnostic_in(&tx, &operation)? {
                facts["diagnostic"] = serde_json::to_value(diagnostic).expect("diagnostic");
            }
            let text = format!(
                "Automatic background subagent result (native durable notice; not user instructions):\n{}",
                facts
            );
            let message = Self::insert_message(&tx, &job.parent.0, "user", &text)?;
            let n = tx.execute(
                "UPDATE child_jobs SET message_id=?2 WHERE operation_id=?1 AND message_id IS NULL",
                params![job.operation, message],
            )?;
            if n != 1 {
                return Err(StorageError::OperationNotFound);
            }
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'subagent_notice',?2)",
                params![job.parent.0, job.delivery_id],
            )?;
            job.message_id = Some(message);
            // The durable message above is the report transport. Broadcast only
            // a bounded refresh/link, not two additional full report copies.
            if let Some(job) = Self::child_job_view(job) {
                notices.push(ChildNotice {
                    job,
                    text: "Subagent result committed; complete report in linked history.".into(),
                });
            }
        }
        tx.commit()?;
        Ok(notices)
    }

    fn child_job_view(mut job: ChildJob) -> Option<ChildJob> {
        job.result = None;
        if job.description.len() > TOOL_OP_PREVIEW_BYTES {
            let end = job
                .description
                .floor_char_boundary(TOOL_OP_PREVIEW_BYTES - '…'.len_utf8());
            job.description.truncate(end);
            job.description.push('…');
        }
        (job.retained_bytes() <= oc_core::tool_output::RECORD_BYTES).then_some(job)
    }
}
