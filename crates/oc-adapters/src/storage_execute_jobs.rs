//! Durable Code Mode program identity, outcome and native notice delivery.
use super::*;
use crate::runtime::code_mode::jobs::{ACTIVE_JOB_CAP, Outcome, Provenance};
use std::sync::atomic::Ordering;

impl Db {
    pub(super) fn execute_jobs_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS execute_jobs(
                operation_id TEXT PRIMARY KEY REFERENCES tool_operations(id),
                session_id TEXT NOT NULL REFERENCES sessions(id),
                version INTEGER NOT NULL CHECK(version=1),
                provenance TEXT NOT NULL,
                phase TEXT NOT NULL CHECK(phase IN ('admitted','running','terminal')),
                background INTEGER NOT NULL CHECK(background IN (0,1)),
                unknown_effect INTEGER NOT NULL DEFAULT 0 CHECK(unknown_effect IN (0,1)),
                outcome TEXT,
                delivery_id TEXT NOT NULL UNIQUE,
                message_id TEXT UNIQUE REFERENCES messages(id));
            CREATE INDEX IF NOT EXISTS execute_jobs_pending ON execute_jobs(phase) WHERE message_id IS NULL;
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(15,'execute-background');
            COMMIT;")?;
        Ok(())
    }

    pub(crate) fn has_active_execute_jobs(&self, session: &str) -> Result<bool, StorageError> {
        self.conn.lock().expect("db mutex").prepare_cached(
            "SELECT EXISTS(SELECT 1 FROM execute_jobs WHERE session_id=?1 AND phase!='terminal')",
        )?.query_row([session], |row| row.get(0)).map_err(Into::into)
    }

    pub(crate) fn live_execute_call_ids(
        &self,
        session: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare_cached("SELECT o.turn_id,o.provider_call_id FROM execute_jobs j JOIN tool_operations o ON o.id=j.operation_id WHERE j.session_id=?1 AND j.phase!='terminal' AND o.turn_id IS NOT NULL AND o.provider_call_id IS NOT NULL LIMIT 9")?;
        let calls = stmt
            .query_map([session], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        if calls.len() > ACTIVE_JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        Ok(calls)
    }

    pub(crate) fn admit_execute_job(
        &self,
        p: &Provenance,
        background: bool,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let count = conn.execute(
            "INSERT INTO execute_jobs(operation_id,session_id,version,provenance,phase,background,delivery_id)
             SELECT ?1,?2,1,?3,'admitted',?4,?5 FROM tool_operations o
             WHERE o.id=?1 AND o.session_id=?2 AND o.turn_id=?6 AND o.name='execute' AND o.state='started'
             AND (?4=0 OR NOT EXISTS(SELECT 1 FROM session_moves m JOIN sessions s ON s.id=m.session_id AND s.parent_id IS NULL
                 WHERE m.session_id=?2 AND m.phase='pending' AND m.source!=m.directory))",
            params![p.operation,p.session,serde_json::to_string(p).expect("execute provenance"),background,format!("execute-notice:{}",p.operation),p.turn],
        )?;
        if count != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(crate) fn start_execute_job(&self, operation: &str) -> Result<(), StorageError> {
        let count = self.conn.lock().expect("db mutex").execute(
            "UPDATE execute_jobs SET phase='running' WHERE operation_id=?1 AND phase='admitted'",
            [operation],
        )?;
        if count != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(crate) fn background_execute_job(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<bool, StorageError> {
        let changed = self.conn.lock().expect("db mutex").execute(
            "UPDATE execute_jobs SET background=1 WHERE session_id=?1 AND operation_id=?2 AND phase='running'
             AND NOT EXISTS(SELECT 1 FROM session_moves m JOIN sessions s ON s.id=m.session_id AND s.parent_id IS NULL
                 WHERE m.session_id=?1 AND m.phase='pending' AND m.source!=m.directory)",
            params![session,operation],
        )?;
        if changed != 0 {
            self.execute_notices_due.store(true, Ordering::Release);
        }
        Ok(changed == 1)
    }

    /// Record a nested unknown effect without rewriting a completed launch turn.
    pub(crate) fn mark_execute_effect_unknown(
        &self,
        operation: &str,
        turn: &str,
    ) -> Result<bool, StorageError> {
        let count = self.conn.lock().expect("db mutex").execute(
            "UPDATE execute_jobs SET unknown_effect=1 WHERE operation_id=?1 AND phase!='terminal'
             AND EXISTS(SELECT 1 FROM tool_operations o WHERE o.id=?1 AND o.turn_id=?2 AND o.name='execute')",
            params![operation,turn],
        )?;
        Ok(count == 1)
    }

    pub(crate) fn finish_execute_job(
        &self,
        operation: &str,
        outcome: &Outcome,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let row: Option<(bool, bool, String)> = tx.query_row(
            "SELECT unknown_effect,background,session_id FROM execute_jobs WHERE operation_id=?1 AND phase!='terminal'",
            [operation], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional()?;
        let Some((unknown, background, session)) = row else {
            let frozen: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM execute_jobs WHERE operation_id=?1 AND phase='terminal')", [operation], |r| r.get(0))?;
            if frozen {
                return Ok(());
            }
            return Err(StorageError::OperationNotFound);
        };
        let mut outcome = outcome.clone();
        if unknown {
            outcome.state = "unknown".into();
            outcome.output =
                "error: a nested MCP effect is unknown; execution was not replayed".into();
        }
        if outcome.presentation.is_none() || unknown {
            let mut previous = match outcome.presentation.take() {
                Some(presentation) => Some(presentation),
                None => Self::tool_presentation_in(&tx, &session, operation)?,
            };
            let mut facts = previous
                .as_mut()
                .and_then(|p| p.execute.take())
                .unwrap_or_default();
            facts.error = true;
            facts.output = vec![oc_core::tool_output::ExecuteOutput {
                kind: oc_core::tool_output::ExecuteOutputKind::Text,
                text: outcome.output.clone(),
                limited: false,
            }];
            let mut presentation = oc_core::tool_output::Presentation::new(
                &outcome.output,
                outcome.output.len() as u64,
                false,
            );
            presentation.execute = Some(facts);
            outcome.presentation =
                Self::bounded_mcp_presentation(operation, Some(Box::new(presentation)));
        }
        if let Some(presentation) = &mut outcome.presentation
            && let Some(facts) = &mut presentation.execute
        {
            facts.background = background;
            facts.state = Some(outcome.state.clone());
            facts.error |= unknown;
        }
        tx.execute("UPDATE execute_jobs SET phase='terminal',outcome=?2 WHERE operation_id=?1 AND phase!='terminal'",
            params![operation,serde_json::to_string(&outcome).expect("execute outcome")])?;
        Self::record_tool_presentation_in(&tx, operation, outcome.presentation.as_deref())?;
        tx.execute("INSERT INTO events(session_id,kind,payload) SELECT session_id,'execute_terminal',operation_id FROM execute_jobs WHERE operation_id=?1", [operation])?;
        tx.commit()?;
        self.execute_notices_due.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn execute_job_identity(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<oc_core::queries::ExecuteJob, StorageError> {
        let (raw, background): (String, bool) = self.conn.lock().expect("db mutex").query_row(
            "SELECT provenance,background FROM execute_jobs WHERE session_id=?1 AND operation_id=?2",
            params![session,operation], |r| Ok((r.get(0)?,r.get(1)?)),
        )?;
        let p: Provenance =
            serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
        if p.version != 1 || p.session != session || p.operation != operation {
            return Err(StorageError::OperationNotFound);
        }
        Ok(oc_core::queries::ExecuteJob {
            session: oc_core::domain::SessionId(p.session),
            execute_id: p.operation,
            location: p.location,
            generation: p.generation,
            turn: p.turn,
            model: p.model,
            provider: p.provider,
            background,
            output: None,
        })
    }

    pub(crate) fn execute_job_outcome(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<Outcome, StorageError> {
        let raw: String = self.conn.lock().expect("db mutex").query_row(
            "SELECT outcome FROM execute_jobs WHERE session_id=?1 AND operation_id=?2 AND phase='terminal'",
            params![session,operation], |r| r.get(0),
        )?;
        let outcome: Outcome =
            serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
        if outcome.version != 1
            || !matches!(
                outcome.state.as_str(),
                "completed" | "failed" | "cancelled" | "unknown"
            )
        {
            return Err(StorageError::OperationNotFound);
        }
        Ok(outcome)
    }

    pub(crate) fn execute_job_phase(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<String, StorageError> {
        self.conn
            .lock()
            .expect("db mutex")
            .query_row(
                "SELECT phase FROM execute_jobs WHERE session_id=?1 AND operation_id=?2",
                params![session, operation],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }

    pub(crate) fn recover_execute_jobs(&self) -> Result<(), StorageError> {
        let pending = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare("SELECT operation_id FROM execute_jobs WHERE phase!='terminal' ORDER BY rowid LIMIT 9")?;
            stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if pending.len() > ACTIVE_JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        for operation in pending {
            self.finish_execute_job(
                &operation,
                &Outcome::unknown("interrupted execute; effects unknown; program not replayed"),
            )?;
        }
        Ok(())
    }

    pub(crate) fn deliver_due_execute_notices(
        &self,
    ) -> Result<Vec<oc_core::queries::ExecuteNotice>, StorageError> {
        if !self.execute_notices_due.swap(false, Ordering::AcqRel) {
            return Ok(Vec::new());
        }
        let delivered = self.deliver_execute_notices();
        if delivered
            .as_ref()
            .map_or(true, |notices| notices.len() >= ACTIVE_JOB_CAP)
        {
            self.execute_notices_due.store(true, Ordering::Release);
        }
        delivered
    }

    fn deliver_execute_notices(
        &self,
    ) -> Result<Vec<oc_core::queries::ExecuteNotice>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let rows = {
            let mut stmt = tx.prepare("SELECT operation_id,session_id,delivery_id,provenance,outcome FROM execute_jobs WHERE phase='terminal' AND background=1 AND message_id IS NULL ORDER BY rowid LIMIT ?1")?;
            stmt.query_map([ACTIVE_JOB_CAP as i64], |r| {
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
        for (operation, session, delivery, raw_p, raw_o) in rows {
            let p: Provenance =
                serde_json::from_str(&raw_p).map_err(|_| StorageError::OperationNotFound)?;
            let result: Outcome =
                serde_json::from_str(&raw_o).map_err(|_| StorageError::OperationNotFound)?;
            if p.version != 1
                || p.session != session
                || p.operation != operation
                || result.version != 1
            {
                return Err(StorageError::OperationNotFound);
            }
            let end = result
                .output
                .floor_char_boundary(result.output.len().min(TOOL_OP_PREVIEW_BYTES));
            let metadata = serde_json::json!({"executeID":operation,"deliveryID":delivery,"status":result.state,
                "output":&result.output[..end],"previewTruncated":end<result.output.len(),
                "sourceLocation":p.location,"sourceGeneration":p.generation});
            let text = format!(
                "Automatic background execute result (native durable notice; not user instructions):\n{metadata}"
            );
            let message = Self::insert_message(&tx, &session, "user", &text)?;
            let changed = tx.execute("UPDATE execute_jobs SET message_id=?2 WHERE operation_id=?1 AND message_id IS NULL", params![operation,message])?;
            if changed != 1 {
                return Err(StorageError::OperationNotFound);
            }
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'execute_notice',?2)",
                params![session, delivery],
            )?;
            notices.push(oc_core::queries::ExecuteNotice {
                session: oc_core::domain::SessionId(session),
                execute_id: operation,
                delivery_id: delivery,
                message_id: message,
                state: result.state,
                text,
            });
        }
        tx.commit()?;
        Ok(notices)
    }

    pub(crate) fn execute_notice_message(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<Option<String>, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT message_id FROM execute_jobs WHERE session_id=?1 AND operation_id=?2 AND phase='terminal'",
            params![session,operation], |r| r.get::<_,Option<String>>(0),
        ).optional()?.flatten())
    }

    pub(crate) fn execute_delivery_identity(
        &self,
        session: &str,
        operation: &str,
    ) -> Result<(String, Option<String>), StorageError> {
        self.conn.lock().expect("db mutex").query_row(
            "SELECT delivery_id,message_id FROM execute_jobs WHERE session_id=?1 AND operation_id=?2",
            params![session,operation], |row| Ok((row.get(0)?,row.get(1)?)),
        ).map_err(Into::into)
    }

    pub(super) fn native_execute_notice_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM execute_jobs j JOIN messages m ON m.id=j.message_id AND m.session_id=j.session_id JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.session_id WHERE j.session_id=?1 AND m.id=?2 AND m.role='user' AND j.phase='terminal' AND j.background=1 AND o.name='execute' AND o.turn_id IS NOT NULL AND CASE WHEN json_valid(j.provenance) AND json_valid(j.outcome) THEN json_extract(j.provenance,'$.version')=1 AND json_extract(j.provenance,'$.session')=j.session_id AND json_extract(j.provenance,'$.operation')=j.operation_id AND json_extract(j.provenance,'$.turn')=o.turn_id AND json_extract(j.outcome,'$.version')=1 ELSE 0 END AND EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.session_id AND e.kind='execute_notice' AND e.payload=j.delivery_id))", params![session,message], |r|r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletion_waits_for_live_and_undelivered_execute() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.create_bound_session("s", "/project").unwrap();
        db.create_bound_session("other", "/project").unwrap();
        db.begin_turn("t", "s", "task").unwrap();
        db.record_tool_intent("op", "s", Some("t"), "execute", "{\"code\":\"x\"}")
            .unwrap();
        let p = Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: "/project".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "m".into(),
            provider: "p".into(),
        };
        db.admit_execute_job(&p, true).unwrap();
        db.start_execute_job("op").unwrap();
        db.record_tool_outcome("op", "completed", Some("running receipt"))
            .unwrap();
        db.finish_turn("t", "completed", None).unwrap();
        assert!(db.delete_root_family("s", "/project").is_err());
        db.finish_execute_job(
            "op",
            &Outcome {
                version: 1,
                state: "completed".into(),
                output: "result".into(),
                presentation: None,
            },
        )
        .unwrap();
        assert!(db.delete_root_family("s", "/project").is_err());
        assert_eq!(db.deliver_due_execute_notices().unwrap().len(), 1);
        db.delete_root_family("s", "/project").unwrap();
        assert!(db.session_meta("other").is_ok());
    }

    #[test]
    fn detached_unknown_recovery_preserves_receipt_and_delivers_once() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "task").unwrap();
        db.record_tool_intent("op", "s", Some("t"), "execute", "{\"code\":\"x\"}")
            .unwrap();
        let p = Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: "/original".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "native defaults".into(),
            agent: None,
            agent_digest: None,
            model: "m".into(),
            provider: "p".into(),
        };
        db.admit_execute_job(&p, true).unwrap();
        db.start_execute_job("op").unwrap();
        db.record_tool_outcome("op", "completed", Some("running receipt"))
            .unwrap();
        db.finish_turn("t", "completed", None).unwrap();
        assert!(db.mark_execute_effect_unknown("op", "t").unwrap());
        db.recover_execute_jobs().unwrap();
        assert_eq!(db.turn_result("t").unwrap().0, "completed");
        assert_eq!(db.tool_state("op").unwrap(), "completed");
        let outcome = db.execute_job_outcome("s", "op").unwrap();
        assert_eq!(outcome.state, "unknown");
        assert_eq!(
            outcome
                .presentation
                .as_ref()
                .unwrap()
                .execute
                .as_ref()
                .unwrap()
                .state
                .as_deref(),
            Some("unknown")
        );
        db.finish_execute_job(
            "op",
            &Outcome {
                version: 1,
                state: "completed".into(),
                output: "incorrect replay".into(),
                presentation: None,
            },
        )
        .unwrap();
        assert_eq!(db.execute_job_outcome("s", "op").unwrap().state, "unknown");
        let notices = db.deliver_due_execute_notices().unwrap();
        assert_eq!(notices.len(), 1);
        assert!(db.is_native_notice("s", &notices[0].message_id).unwrap());
        assert!(db.deliver_due_execute_notices().unwrap().is_empty());
        drop(db);
        let reopened = Db::open(root.path()).unwrap();
        reopened.recover_execute_jobs().unwrap();
        assert!(reopened.deliver_due_execute_notices().unwrap().is_empty());
        assert_eq!(reopened.tool_state("op").unwrap(), "completed");
    }

    #[test]
    fn nested_unknown_replaces_success_presentation_without_losing_call_facts() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.create_session("s").unwrap();
        db.begin_turn("t", "s", "task").unwrap();
        db.record_tool_intent("op", "s", Some("t"), "execute", "{\"code\":\"x\"}")
            .unwrap();
        let p = Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: "/original".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "m".into(),
            provider: "p".into(),
        };
        db.admit_execute_job(&p, true).unwrap();
        db.start_execute_job("op").unwrap();
        assert!(db.mark_execute_effect_unknown("op", "t").unwrap());
        let mut presentation = oc_core::tool_output::Presentation::new("old success", 11, false);
        presentation.execute = Some(oc_core::tool_output::Execute {
            code: Some("x".into()),
            output: vec![oc_core::tool_output::ExecuteOutput {
                kind: oc_core::tool_output::ExecuteOutputKind::Text,
                text: "old success".into(),
                limited: false,
            }],
            ..Default::default()
        });
        db.finish_execute_job(
            "op",
            &Outcome {
                version: 1,
                state: "completed".into(),
                output: "old success".into(),
                presentation: Some(Box::new(presentation)),
            },
        )
        .unwrap();
        drop(db);
        let reopened = Db::open(root.path()).unwrap();
        let outcome = reopened.execute_job_outcome("s", "op").unwrap();
        assert_eq!(outcome.state, "unknown");
        let presentation = outcome.presentation.unwrap();
        assert_eq!(presentation.body, outcome.output);
        let facts = presentation.execute.unwrap();
        assert_eq!(facts.code.as_deref(), Some("x"));
        assert!(facts.error);
        assert_eq!(facts.state.as_deref(), Some("unknown"));
        assert_eq!(facts.output[0].text, outcome.output);
    }

    #[test]
    fn fork_keeps_unconsumed_execute_notice_inert_across_recursive_forks() {
        use crate::provider::{InputItem, InputRole};

        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        db.create_session("s").unwrap();
        Db::insert_location_binding(&db.conn.lock().unwrap(), "s", "/original").unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let accepted = db.accept_turn("t", "s", "task", "task", &model).unwrap();
        let mut log = crate::tools::TurnLog::new("t", "m", "fixture");
        log.user_message = Some(accepted.user_message);
        log.input = vec![
            InputItem::message(InputRole::User, "task"),
            InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","call_id":"call","name":"execute","arguments":"{\"code\":\"x\"}"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "call".into(),
                output: "running receipt".into(),
            },
            InputItem::message(InputRole::Assistant, "done"),
        ];
        log.display_parts = vec![
            serde_json::json!({"tool":"op"}),
            serde_json::json!({"message":3}),
        ];
        db.record_tool_intent("op", "s", Some("t"), "execute", "{\"code\":\"x\"}")
            .unwrap();
        let p = Provenance {
            version: 1,
            session: "s".into(),
            turn: "t".into(),
            operation: "op".into(),
            location: "/original".into(),
            generation: 1,
            output_limits: Default::default(),
            output_source: "defaults".into(),
            agent: None,
            agent_digest: None,
            model: "m".into(),
            provider: "fixture".into(),
        };
        db.admit_execute_job(&p, true).unwrap();
        db.start_execute_job("op").unwrap();
        db.record_tool_outcome("op", "completed", Some("running receipt"))
            .unwrap();
        db.commit_turn(
            "t",
            "completed",
            Some(&log.to_json().to_string()),
            Some("done"),
        )
        .unwrap();
        db.finish_execute_job(
            "op",
            &Outcome {
                version: 1,
                state: "completed".into(),
                output: "FORK_NOTICE_SENTINEL".into(),
                presentation: None,
            },
        )
        .unwrap();
        let notice = db
            .deliver_due_execute_notices()
            .unwrap()
            .remove(0)
            .message_id;
        assert!(!db.parent_notice_assigned("s", &notice).unwrap());

        let mut source = "s".to_owned();
        for _ in 0..2 {
            let boundary = db.append_message(&source, "user", "next task").unwrap();
            let fork = db
                .fork_session(&source, &boundary, "/original", "fixture", "{}")
                .unwrap();
            let copied = db
                .read_history_full(&fork.session.0)
                .unwrap()
                .into_iter()
                .find(|(_, role, text)| role == "user" && text.contains("FORK_NOTICE_SENTINEL"))
                .unwrap()
                .0;
            let conn = db.conn.lock().unwrap();
            assert!(Db::native_notice_data_in(&conn, &fork.session.0, &copied).unwrap());
            let jobs: i64 = conn
                .query_row(
                    "SELECT count(*) FROM execute_jobs WHERE session_id=?1",
                    [&fork.session.0],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(jobs, 0);
            drop(conn);
            assert!(!db.is_native_notice(&fork.session.0, &copied).unwrap());
            assert!(
                db.fork_session(&fork.session.0, &copied, "/original", "fixture", "{}")
                    .is_err()
            );
            assert!(db.parent_notice_input(&fork.session.0, &[copied]).is_err());
            source = fork.session.0;
        }
    }
}
