//! Durable checkpoints participate in the existing incremental ContextVersion timeline.
use super::*;
use oc_core::compaction::{CompactionSnapshot, CompactionState};

impl Db {
    pub(crate) fn compaction_anchor(
        &self,
        session: &str,
    ) -> Result<oc_core::compaction::CompactionAnchor, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let message=conn.query_row("SELECT id FROM conversation_messages WHERE session_id=?1 ORDER BY seq DESC LIMIT 1",[session],|r|r.get(0)).optional()?;
        let turn: Option<String>=conn.query_row("SELECT id FROM conversation_turns WHERE session_id=?1 ORDER BY archive_rowid DESC LIMIT 1",[session],|r|r.get(0)).optional()?;
        let tool = if let Some(turn) = &turn {
            conn.query_row("SELECT id FROM conversation_tools WHERE session_id=?1 AND turn_id=?2 ORDER BY archive_rowid DESC LIMIT 1",params![session,turn],|r|r.get(0)).optional()?
        } else {
            None
        };
        Ok(oc_core::compaction::CompactionAnchor {
            message,
            turn,
            tool,
        })
    }
    pub(super) fn compaction_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS session_checkpoint(
          session_id TEXT PRIMARY KEY REFERENCES sessions(id), boundary_message TEXT NOT NULL,
          summary TEXT NOT NULL, route TEXT, opaque TEXT, operation_id TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS session_compactions(
          id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id), snapshot TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS session_usage_anchor(
          session_id TEXT PRIMARY KEY REFERENCES sessions(id), anchor TEXT NOT NULL);")?;
        // A lost stream never installs partial context. Reopening marks it cancelled.
        conn.execute("UPDATE session_compactions SET snapshot=json_set(snapshot,'$.state','cancelled','$.error','interrupted by restart') WHERE json_extract(snapshot,'$.state') IN ('queued','running')", [])?;
        Ok(())
    }
    pub(crate) fn save_compaction(
        &self,
        snapshot: &CompactionSnapshot,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw = serde_json::to_string(snapshot).map_err(|_| StorageError::CompressionConflict)?;
        conn.execute("INSERT INTO session_compactions VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET snapshot=excluded.snapshot", params![snapshot.id,snapshot.session,raw])?;
        Ok(())
    }
    pub(crate) fn usage_anchor(
        &self,
        session: &str,
    ) -> Result<Option<crate::compaction::UsageAnchor>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw: Option<String> = conn
            .query_row(
                "SELECT anchor FROM session_usage_anchor WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|raw| serde_json::from_str(&raw).map_err(|_| StorageError::CompressionConflict))
            .transpose()
    }
    pub(crate) fn save_usage_anchor(
        &self,
        session: &str,
        anchor: &crate::compaction::UsageAnchor,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw = serde_json::to_string(anchor).map_err(|_| StorageError::CompressionConflict)?;
        conn.execute("INSERT INTO session_usage_anchor VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET anchor=excluded.anchor", params![session,raw])?;
        Ok(())
    }
    pub fn compaction_history(
        &self,
        session: &str,
    ) -> Result<Vec<CompactionSnapshot>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare("SELECT snapshot FROM session_compactions c WHERE session_id=?1 AND (json_extract(snapshot,'$.anchor.message') IS NULL OR EXISTS(SELECT 1 FROM conversation_messages m WHERE m.session_id=c.session_id AND m.id=json_extract(c.snapshot,'$.anchor.message'))) ORDER BY rowid DESC LIMIT 100")?;
        let rows = stmt.query_map([session], |r| r.get::<_, String>(0))?;
        rows.map(|raw| serde_json::from_str(&raw?).map_err(|_| StorageError::CompressionConflict))
            .collect()
    }
    pub(crate) fn session_checkpoint(
        &self,
        session: &str,
    ) -> Result<Option<(i64, String)>, StorageError> {
        Ok(self
            .checkpoint_record(session)?
            .map(|(seq, summary, _, _)| (seq, summary)))
    }
    #[allow(clippy::type_complexity)]
    pub(crate) fn checkpoint_record(
        &self,
        session: &str,
    ) -> Result<Option<(i64, String, Option<String>, Option<String>)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row("SELECT m.seq,c.summary,c.route,c.opaque FROM session_checkpoint c JOIN conversation_messages m ON m.id=c.boundary_message AND m.session_id=c.session_id WHERE c.session_id=?1",[session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(Into::into)
    }

    pub(crate) fn checkpoint_model(
        &self,
        session: &str,
    ) -> Result<Option<oc_core::queries::ModelRef>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let raw:Option<String>=conn.query_row("SELECT c.snapshot ->> '$.model' FROM session_checkpoint p JOIN session_compactions c ON c.id=p.operation_id WHERE p.session_id=?1",[session],|row|row.get(0)).optional()?.flatten();
        raw.map(|raw| serde_json::from_str(&raw).map_err(|_| StorageError::CompressionConflict))
            .transpose()
    }
    /// Whole exchanges only: always keep the latest exchange if an older prefix exists.
    pub(crate) fn compaction_boundary(
        &self,
        session: &str,
        after: i64,
        keep: u64,
    ) -> Result<Option<(i64, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt = conn.prepare("SELECT id,seq,role,length(CAST(text AS BLOB)) FROM conversation_messages WHERE session_id=?1 AND seq>?2 ORDER BY seq DESC")?;
        let mut rows = stmt.query(params![session, after])?;
        let mut total = 0u64;
        let mut tail_user = None;
        let mut latest_user = None;
        let mut oldest = None;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            let seq: i64 = row.get(1)?;
            let role: String = row.get(2)?;
            let bytes = row.get::<_, i64>(3)?.max(0) as u64;
            total = total.saturating_add(bytes.div_ceil(4));
            oldest = Some((seq, id));
            if role == "user" {
                latest_user.get_or_insert(seq);
                if tail_user.is_some() && total > keep {
                    break;
                }
                tail_user = Some(seq);
            }
        }
        let Some((oldest_seq, _)) = oldest else {
            return Ok(None);
        };
        let tail = tail_user
            .filter(|seq| *seq > oldest_seq)
            .or(latest_user.filter(|seq| *seq > oldest_seq));
        match tail {
            Some(seq) => conn.query_row("SELECT seq,id FROM conversation_messages WHERE session_id=?1 AND seq>?2 AND seq<?3 ORDER BY seq DESC LIMIT 1", params![session,after,seq],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(Into::into),
            None => conn.query_row("SELECT seq,id FROM conversation_messages WHERE session_id=?1 AND seq>?2 AND seq<COALESCE((SELECT MIN(m.seq) FROM turn_acceptances a JOIN turns t ON t.id=a.turn_id JOIN messages m ON m.id=a.user_message WHERE a.session_id=?1 AND t.status='started'),9223372036854775807) ORDER BY seq DESC LIMIT 1",params![session,after],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(Into::into),
        }
    }
    pub(crate) fn message_seq(&self, session: &str, id: &str) -> Result<Option<i64>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "SELECT seq FROM conversation_messages WHERE session_id=?1 AND id=?2",
            params![session, id],
            |r| r.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
    pub(crate) fn commit_checkpoint(
        &self,
        snapshot: &CompactionSnapshot,
        boundary: &str,
        native: Option<(&str, &str)>,
        removed: &std::collections::BTreeMap<String, u64>,
    ) -> Result<(), StorageError> {
        if snapshot.state != CompactionState::Completed
            || (snapshot.summary.trim().is_empty() && native.is_none())
        {
            return Err(StorageError::CompressionConflict);
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_messages WHERE session_id=?1 AND id=?2)",
            params![snapshot.session, boundary],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(StorageError::CompressionConflict);
        }
        // DCP occurrence keys are relative to the active wire window. Retire
        // marks for removed calls and shift surviving marks atomically, without
        // allowing an old hidden call to hide a newer reused provider call ID.
        for (call_id, count) in removed {
            let mut statement=tx.prepare("SELECT occurrence,action FROM dcp_tool_projection_v2 WHERE session_id=?1 AND call_id=?2 AND occurrence>=?3 ORDER BY occurrence")?;
            let rows = statement
                .query_map(params![snapshot.session, call_id, *count as i64], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            tx.execute(
                "DELETE FROM dcp_tool_projection_v2 WHERE session_id=?1 AND call_id=?2",
                params![snapshot.session, call_id],
            )?;
            for (occurrence, action) in rows {
                tx.execute(
                    "INSERT INTO dcp_tool_projection_v2 VALUES(?1,?2,?3,?4)",
                    params![
                        snapshot.session,
                        call_id,
                        occurrence - *count as i64,
                        action
                    ],
                )?;
            }
        }
        tx.execute("INSERT INTO session_checkpoint VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(session_id) DO UPDATE SET boundary_message=excluded.boundary_message,summary=excluded.summary,route=excluded.route,opaque=excluded.opaque,operation_id=excluded.operation_id",params![snapshot.session,boundary,snapshot.summary,native.map(|n|n.0),native.map(|n|n.1),snapshot.id])?;
        Self::refresh_checkpoint_dcp_accounting(&tx, &snapshot.session)?;
        // A summarizer's usage is not a measurement of the replacement window.
        tx.execute(
            "DELETE FROM session_usage_anchor WHERE session_id=?1",
            [&snapshot.session],
        )?;
        let raw = serde_json::to_string(snapshot).map_err(|_| StorageError::CompressionConflict)?;
        tx.execute(
            "UPDATE session_compactions SET snapshot=?2 WHERE id=?1",
            params![snapshot.id, raw],
        )?;
        // Standalone compaction is a real settled context fact at the current
        // tip. Redo must restore it even when no later user turn was admitted.
        // During a running turn, that turn's normal settlement saves the revision.
        Self::publish_settled_context(&tx, &snapshot.session)?;
        tx.commit()?;
        Ok(())
    }

    pub(super) fn refresh_checkpoint_dcp_accounting(
        conn: &Connection,
        session: &str,
    ) -> Result<(), StorageError> {
        // No backfill: legacy/unmeasured sessions still have no accounting.
        let raw: Option<String> = conn.query_row(
            "SELECT CASE WHEN length(CAST(snapshot AS BLOB))<=16384 THEN snapshot END FROM dcp_accounting WHERE session_id=?1",
            [session], |r| r.get(0),
        ).optional()?.flatten();
        let Some(raw) = raw else { return Ok(()) };
        let mut accounting: oc_core::dcp_view::DcpAccounting =
            serde_json::from_str(&raw).map_err(|_| StorageError::CompressionConflict)?;
        let after: i64 = conn.query_row(
            "SELECT MAX(COALESCE((SELECT m.seq FROM session_checkpoint c JOIN conversation_messages m ON m.id=c.boundary_message AND m.session_id=c.session_id WHERE c.session_id=?1),0),COALESCE((SELECT m.seq FROM prune_marks p JOIN conversation_messages m ON m.id=p.up_to_msg AND m.session_id=p.session_id WHERE p.session_id=?1),0))",
            [session], |r| r.get(0),
        )?;
        let graph = Self::active_compression_graph_in(conn, session, after)?;
        let by_id = graph.into_iter().map(|b| (b.id.clone(), b)).collect();
        let mut roots = conn.prepare("SELECT cb.id FROM compression_blocks cb WHERE cb.session_id=?1 AND EXISTS(SELECT 1 FROM compression_members cm JOIN conversation_messages m ON m.id=cm.message_id WHERE cm.block_id=cb.id AND m.session_id=?1 AND m.seq>?2) LIMIT 4097")?;
        let mut active_summary = 0u64;
        let mut expanded_bytes = 0usize;
        for root in roots.query_map(params![session, after], |r| r.get::<_, String>(0))? {
            let summary = crate::dcp::expand_block(&by_id, &root?, 0, &mut Vec::new())
                .map_err(|_| StorageError::CompressionConflict)?;
            expanded_bytes = expanded_bytes.saturating_add(summary.len());
            if expanded_bytes > crate::runtime::ACTIVE_CONTEXT_BYTES_CAP {
                return Err(StorageError::CompressionConflict);
            }
            active_summary =
                active_summary.saturating_add(oc_core::dcp_view::estimate_content(&summary));
        }
        accounting.active_summary = active_summary;
        // Checkpoint text is not a DCP range summary. Historical run snapshots
        // and cumulative removal/savings counters are deliberately frozen.
        conn.execute(
            "UPDATE dcp_accounting SET snapshot=?2 WHERE session_id=?1",
            params![
                session,
                serde_json::to_string(&accounting)
                    .map_err(|_| StorageError::CompressionConflict)?
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::compaction::{CompactionAnchor, CompactionReason};
    #[test]
    fn compaction_checkpoint_rebases_dcp_occurrences_atomically_and_undo_restores_them() {
        let data = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_session("s").unwrap();
        db.apply_dcp_schema().unwrap();
        let boundary = db.append_message("s", "user", "old exchange").unwrap();
        db.conn.lock().unwrap().execute_batch("INSERT INTO dcp_tool_projection_v2 VALUES('s','reused',0,'hidden'),('s','reused',1,'purged'),('s','unrelated',0,'hidden');").unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        db.accept_turn("next", "s", "new exchange", "new exchange", &model)
            .unwrap();
        db.commit_turn("next", "completed", None, Some("answer"))
            .unwrap();
        let snapshot = CompactionSnapshot {
            model: None,
            anchor: CompactionAnchor::default(),
            id: "compact".into(),
            session: "s".into(),
            reason: CompactionReason::Manual,
            state: CompactionState::Completed,
            summary: "## Objective\nContinue".into(),
            usage: None,
            provider_native: false,
            error: None,
        };
        db.save_compaction(&snapshot).unwrap();
        db.commit_checkpoint(
            &snapshot,
            &boundary,
            None,
            &[("reused".into(), 1)].into_iter().collect(),
        )
        .unwrap();
        let projection = db.load_dcp_tool_projection("s").unwrap();
        assert!(!projection.hidden.contains(&("reused".into(), 0)));
        assert!(projection.purged.contains(&("reused".into(), 0)));
        assert!(projection.hidden.contains(&("unrelated".into(), 0)));
        db.change_conversation("s", oc_core::queries::ConversationAction::Undo)
            .unwrap();
        let restored = db.load_dcp_tool_projection("s").unwrap();
        assert!(restored.hidden.contains(&("reused".into(), 0)));
        assert!(restored.purged.contains(&("reused".into(), 1)));
        assert!(db.session_checkpoint("s").unwrap().is_none());
        db.change_conversation("s", oc_core::queries::ConversationAction::Redo)
            .unwrap();
        assert!(
            db.load_dcp_tool_projection("s")
                .unwrap()
                .purged
                .contains(&("reused".into(), 0))
        );
        assert!(db.session_checkpoint("s").unwrap().is_some());
    }
}

#[cfg(test)]
#[path = "storage_dcp_lifecycle_fixture.rs"]
pub(super) mod dcp_lifecycle_fixture;

#[cfg(test)]
#[path = "storage_checkpoint_dcp_tests.rs"]
mod dcp_tests;
