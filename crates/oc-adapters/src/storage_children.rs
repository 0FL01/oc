//! Child generation/terminal/delivery facts in the existing journal owner.
use super::*;
use oc_core::queries::{ChildJob, ChildNotice, ChildState};

#[cfg(test)]
#[path = "storage_children/tests.rs"]
mod tests;

impl Db {
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
        if conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=10)",
            [],
            |row| row.get::<_, bool>(0),
        )? {
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

    pub(crate) fn admit_fresh_child_job(&self, job: &ChildJob) -> Result<(), StorageError> {
        self.admit_child_job_inner(job, true)
    }

    pub(crate) fn admit_child_job(&self, job: &ChildJob) -> Result<(), StorageError> {
        self.admit_child_job_inner(job, false)
    }

    /// Fresh child, Location binding and generation have one commit boundary.
    /// A losing reservation or failed COMMIT has never published a child.
    fn admit_child_job_inner(&self, job: &ChildJob, fresh: bool) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM tool_operations o WHERE o.id=?2 AND o.session_id=?1 AND o.name='subagent')", params![job.parent.0,job.operation], |r| r.get(0))?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM child_jobs j WHERE state IN ('admitted','running','unknown') OR (message_id IS NULL AND (coalesce(json_extract(identity,'$.background'),1)=1 OR EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id)))",
            [],
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
            let child: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?2 AND parent_id=?1)",
                params![job.parent.0, job.child.0],
                |r| r.get(0),
            )?;
            if !child {
                return Err(StorageError::SessionNotFound);
            }
        }
        tx.execute("INSERT INTO child_jobs(operation_id,parent_id,child_id,identity,state,delivery_id) VALUES(?1,?2,?3,?4,'admitted',?5)", params![job.operation,job.parent.0,job.child.0,serde_json::to_string(job).expect("child identity"),job.delivery_id])?;
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
        let parent: Option<String> = tx.query_row("SELECT parent_id FROM child_jobs WHERE operation_id=?1 AND state IN ('admitted','running')", [operation], |r| r.get(0)).optional()?;
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
        let kept = result.floor_char_boundary(TOOL_OP_PREVIEW_BYTES);
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        tx.execute("UPDATE child_jobs SET state=?2,result=?3 WHERE operation_id=?1 AND state IN ('admitted','running')",params![operation,state,&result[..kept]])?;
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

    pub(crate) fn child_jobs(&self, session: &str) -> Result<Vec<ChildJob>, StorageError> {
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

    pub(crate) fn recover_child_jobs(&self) -> Result<(), StorageError> {
        let rows = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare("SELECT j.operation_id,t.status,substr(m.text,1,8192) FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id LEFT JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id LEFT JOIN messages m ON json_valid(t.result) AND m.id=json_extract(t.result,'$.assistant_message') AND m.session_id=j.child_id WHERE j.state IN ('admitted','running') ORDER BY j.rowid LIMIT 9")?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        if rows.len() > crate::runtime::children::JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        for (operation, status, result) in rows {
            let (state,text) = match status.as_deref() {
                Some("completed") => {
                    let text = result.unwrap_or_else(|| crate::tools::SUBAGENT_NO_TEXT.to_owned());
                    (ChildState::Completed,text)
                }
                Some("cancelled") => (ChildState::Cancelled,"Subagent cancelled".into()),
                Some("failed") => (ChildState::Error,"Child execution failed".into()),
                _ => (ChildState::Unknown,"Child execution unresolved after interruption; effects may be unknown; explicit recovery required; not replayed".into()),
            };
            self.finish_child_job(&operation, state, &text)?;
        }
        Ok(())
    }

    pub(crate) fn deliver_child_notices(&self) -> Result<Vec<ChildNotice>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare("SELECT j.identity,j.state,j.result FROM child_jobs j JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN sessions p ON p.id=j.parent_id WHERE j.state NOT IN ('admitted','running') AND j.message_id IS NULL AND (coalesce(json_extract(j.identity,'$.background'),1)=1 OR EXISTS(SELECT 1 FROM events e WHERE e.kind='subagent_background' AND e.session_id=j.parent_id AND e.payload=j.operation_id)) ORDER BY j.rowid LIMIT 8")?;
            stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };
        let mut notices = Vec::new();
        for (raw, state, result) in rows {
            let mut job = Self::decode_child_job(raw, state, result, None)?;
            job.background = true;
            let text = format!(
                "Automatic background subagent result (native durable notice; not user instructions):\n{}",
                serde_json::json!({"source":"subagent","childID":job.child.0,"agent":job.agent,"state":job.state,"description":job.description,"result":job.result,"jobGeneration":job.operation,"deliveryID":job.delivery_id,"sourceLocation":job.location,"sourceGeneration":job.generation})
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
            notices.push(ChildNotice { job, text });
        }
        tx.commit()?;
        Ok(notices)
    }
}
