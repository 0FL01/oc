//! Conversation points reference shared immutable DCP metadata, never transcripts.
use super::*;
use oc_core::queries::{ConversationAction, ConversationSnapshot, RevertedConversation};
use oc_core::{domain::SessionId, session::CoreError};

const TABLES: &[(&str, &str, &str)] = &[
    (
        "compression_blocks",
        "id,session_id,topic,summary,start_msg,end_msg,created_at",
        "session_id=?1",
    ),
    (
        "compression_members",
        "block_id,message_id",
        "block_id IN (SELECT id FROM compression_blocks WHERE session_id=?1)",
    ),
    (
        "prune_marks",
        "session_id,up_to_msg,created_at",
        "session_id=?1",
    ),
    (
        "dcp_tool_projection",
        "session_id,call_id,action",
        "session_id=?1",
    ),
    (
        "dcp_tool_projection_v2",
        "session_id,call_id,occurrence,action",
        "session_id=?1",
    ),
    (
        "prefs",
        "key,value,updated_at",
        "substr(CAST(key AS BLOB),1,length(CAST('dcp.nudge.'||?1||char(0) AS BLOB)))=CAST('dcp.nudge.'||?1||char(0) AS BLOB)",
    ),
];

fn unavailable(message: &str) -> StorageError {
    StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

fn key_count(kind: usize) -> usize {
    match kind {
        1 | 3 => 2,
        4 => 3,
        _ => 1,
    }
}

impl Db {
    pub(super) fn conversation_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS conversation_state(
            session_id TEXT PRIMARY KEY REFERENCES sessions(id), upper_seq INTEGER);
          CREATE TABLE IF NOT EXISTS conversation_exclusions(
            session_id TEXT NOT NULL REFERENCES sessions(id), lower_seq INTEGER NOT NULL, upper_seq INTEGER NOT NULL);
          CREATE INDEX IF NOT EXISTS conversation_exclusion_session ON conversation_exclusions(session_id,lower_seq,upper_seq);
          CREATE TABLE IF NOT EXISTS conversation_contexts(digest TEXT PRIMARY KEY, metadata TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS conversation_redo(
            session_id TEXT PRIMARY KEY REFERENCES sessions(id), tip_seq INTEGER NOT NULL,
            context TEXT NOT NULL REFERENCES conversation_contexts(digest),
            pending_context TEXT NOT NULL REFERENCES conversation_contexts(digest));
          CREATE TABLE IF NOT EXISTS conversation_points(
            turn_id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
            pre_seq INTEGER NOT NULL, pre_context TEXT NOT NULL REFERENCES conversation_contexts(digest),
            post_seq INTEGER, post_context TEXT REFERENCES conversation_contexts(digest), active INTEGER NOT NULL DEFAULT 1);
          CREATE INDEX IF NOT EXISTS conversation_point_session ON conversation_points(session_id,active,pre_seq);
          CREATE VIEW IF NOT EXISTS conversation_messages AS SELECT m.* FROM messages m
            WHERE NOT EXISTS(SELECT 1 FROM conversation_state s WHERE s.session_id=m.session_id AND m.seq>s.upper_seq)
            AND NOT EXISTS(SELECT 1 FROM conversation_exclusions e WHERE e.session_id=m.session_id AND m.seq>e.lower_seq AND m.seq<=e.upper_seq);
          CREATE VIEW IF NOT EXISTS conversation_turns AS SELECT t.*,t.rowid AS archive_rowid FROM turns t
            WHERE NOT EXISTS(SELECT 1 FROM conversation_points p WHERE p.turn_id=t.id AND
              (p.active=0 OR EXISTS(SELECT 1 FROM conversation_state s WHERE s.session_id=t.session_id AND p.pre_seq>=s.upper_seq)));
          CREATE VIEW IF NOT EXISTS conversation_tools AS SELECT o.*,o.rowid AS archive_rowid FROM tool_operations o
            WHERE o.turn_id IS NULL OR EXISTS(SELECT 1 FROM conversation_turns t WHERE t.id=o.turn_id);"
        )?;
        Self::install_context_tracking(conn)
    }

    // Each mutation closes a validity interval and shares an immutable row
    // object. A point is one revision number, regardless of session size.
    // SQLite streams bootstrap/restore; Rust never loads a session's metadata.
    pub(super) fn install_context_tracking(conn: &Connection) -> Result<(), StorageError> {
        let tx = conn.unchecked_transaction()?;
        Self::install_context_tracking_inner(&tx)?;
        tx.commit()?;
        Ok(())
    }

    fn install_context_tracking_inner(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS conversation_revision(id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL);
          INSERT OR IGNORE INTO conversation_revision VALUES(1,0);
          CREATE TABLE IF NOT EXISTS conversation_objects(id INTEGER PRIMARY KEY, kind INTEGER NOT NULL, payload TEXT NOT NULL, UNIQUE(kind,payload));
          CREATE TABLE IF NOT EXISTS conversation_versions(session_id TEXT NOT NULL,kind INTEGER NOT NULL,row_key TEXT NOT NULL,object_id INTEGER NOT NULL REFERENCES conversation_objects(id),valid_from INTEGER NOT NULL,valid_to INTEGER);
          CREATE INDEX IF NOT EXISTS conversation_version_point ON conversation_versions(session_id,kind,valid_from,valid_to);
          CREATE UNIQUE INDEX IF NOT EXISTS conversation_version_current ON conversation_versions(kind,row_key) WHERE valid_to IS NULL;
          CREATE TABLE IF NOT EXISTS conversation_tracked(kind INTEGER PRIMARY KEY);")?;
        for (kind, (table, columns, _)) in TABLES.iter().enumerate() {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |r| r.get(0),
            )?;
            if !exists {
                continue;
            }
            let tracked: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_tracked WHERE kind=?1)",
                [kind as i64],
                |r| r.get(0),
            )?;
            if tracked {
                continue;
            }
            let fields: Vec<_> = columns.split(',').collect();
            let key_count = key_count(kind);
            let key = |alias: &str| {
                fields[..key_count]
                    .iter()
                    .map(|c| format!("{alias}{c}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let payload = |alias: &str| {
                fields
                    .iter()
                    .map(|c| format!("{alias}{c}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let session = |alias: &str| match kind {
                1 => {
                    format!("(SELECT session_id FROM compression_blocks WHERE id={alias}block_id)")
                }
                5 => format!(
                    "CAST(substr(CAST({alias}key AS BLOB),11,instr(substr(CAST({alias}key AS BLOB),11),x'00')-1) AS TEXT)"
                ),
                _ => format!("{alias}session_id"),
            };
            let guard = |alias: &str| {
                if kind == 5 {
                    format!(
                        "substr(CAST({alias}key AS BLOB),1,10)=CAST('dcp.nudge.' AS BLOB) AND instr(CAST({alias}key AS BLOB),x'00')>10"
                    )
                } else {
                    "1".into()
                }
            };
            // One-time migration captures only the currently real metadata;
            // legacy points without a historical version remain unavailable.
            conn.execute_batch(&format!("INSERT OR IGNORE INTO conversation_objects(kind,payload) SELECT {kind},json_array({columns}) FROM {table} WHERE {};
              INSERT INTO conversation_versions SELECT {},{kind},json_array({}),o.id,(SELECT revision FROM conversation_revision),NULL FROM {table} JOIN conversation_objects o ON o.kind={kind} AND o.payload=json_array({}) WHERE {};
              INSERT INTO conversation_tracked VALUES({kind});", guard(""), session(""), key(&format!("{table}.")), payload(&format!("{table}.")), guard(&format!("{table}."))))?;
            for operation in ["INSERT", "UPDATE", "DELETE"] {
                let close = if operation == "INSERT" {
                    String::new()
                } else {
                    format!(
                        "UPDATE conversation_versions SET valid_to=(SELECT revision FROM conversation_revision) WHERE kind={kind} AND row_key=json_array({}) AND valid_to IS NULL;",
                        key("OLD.")
                    )
                };
                let insert = if operation == "DELETE" {
                    String::new()
                } else {
                    // An outer UPSERT may override a trigger's OR IGNORE
                    // conflict policy. Avoid the conflict altogether.
                    format!("INSERT INTO conversation_objects(kind,payload) SELECT {kind},json_array({}) WHERE NOT EXISTS(SELECT 1 FROM conversation_objects WHERE kind={kind} AND payload=json_array({}));
                  INSERT INTO conversation_versions SELECT {},{kind},json_array({}),id,(SELECT revision FROM conversation_revision),NULL FROM conversation_objects WHERE kind={kind} AND payload=json_array({}) AND {};",payload("NEW."),payload("NEW."),session("NEW."),key("NEW."),payload("NEW."),guard("NEW."))
                };
                let when = match operation {
                    "UPDATE" => format!(
                        "({} OR {}) AND json_array({}) IS NOT json_array({})",
                        guard("OLD."),
                        guard("NEW."),
                        payload("OLD."),
                        payload("NEW.")
                    ),
                    "DELETE" => guard("OLD."),
                    _ => guard("NEW."),
                };
                conn.execute_batch(&format!("CREATE TRIGGER conversation_track_{kind}_{operation} AFTER {operation} ON {table} WHEN {} BEGIN
                  UPDATE conversation_revision SET revision=revision+1;
                  {close} {insert} END;",when))?;
            }
        }
        Ok(())
    }

    pub(super) fn save_context(conn: &Connection, _session: &str) -> Result<String, StorageError> {
        let revision: i64 =
            conn.query_row("SELECT revision FROM conversation_revision", [], |r| {
                r.get(0)
            })?;
        let digest = format!("revision:{revision}");
        conn.execute(
            "INSERT OR IGNORE INTO conversation_contexts(digest,metadata) VALUES (?1,'revision')",
            [&digest],
        )?;
        Ok(digest)
    }

    fn restore_context(conn: &Connection, session: &str, digest: &str) -> Result<(), StorageError> {
        Self::load_context_restore(conn, session, digest)?;
        Self::apply_context_restore(conn, session)
    }

    pub(super) fn load_context_restore(
        conn: &Connection,
        session: &str,
        digest: &str,
    ) -> Result<(), StorageError> {
        let saved: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_contexts WHERE digest=?1)",
            [digest],
            |r| r.get(0),
        )?;
        if !saved {
            return Err(unavailable("no saved historical context"));
        }
        conn.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS conversation_restore(kind INTEGER,object_id INTEGER,row_key TEXT);
          CREATE INDEX IF NOT EXISTS conversation_restore_key ON conversation_restore(kind,row_key);
          DELETE FROM conversation_restore;",
        )?;
        if let Some(revision) = digest.strip_prefix("revision:") {
            let revision: i64 = revision
                .parse()
                .map_err(|_| unavailable("invalid saved conversation revision"))?;
            conn.execute("INSERT INTO conversation_restore SELECT kind,object_id,row_key FROM conversation_versions WHERE session_id=?1 AND valid_from<=?2 AND (valid_to IS NULL OR valid_to>?2)",params![session,revision])?;
        } else {
            // Previously saved JSON is genuine historical data. Migrate it in
            // SQL row by row rather than fabricating a legacy context or loading
            // an unbounded aggregate into Rust.
            for kind in 0..TABLES.len() {
                conn.execute("INSERT OR IGNORE INTO conversation_objects(kind,payload) SELECT ?2,j.value FROM conversation_contexts c,json_each(c.metadata,'$['||?2||']') j WHERE c.digest=?1",params![digest,kind as i64])?;
                let key = (0..key_count(kind))
                    .map(|i| format!("json_extract(o.payload,'$[{i}]')"))
                    .collect::<Vec<_>>()
                    .join(",");
                conn.execute(&format!("INSERT INTO conversation_restore SELECT ?2,o.id,json_array({key}) FROM conversation_contexts c,json_each(c.metadata,'$['||?2||']') j JOIN conversation_objects o ON o.kind=?2 AND o.payload=j.value WHERE c.digest=?1"),params![digest,kind as i64])?;
            }
        }
        Ok(())
    }

    pub(super) fn apply_context_restore(
        conn: &Connection,
        session: &str,
    ) -> Result<(), StorageError> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='compression_blocks')",
            [],
            |r| r.get(0),
        )?;
        if !exists {
            let has_dcp: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_restore WHERE kind<5)",
                [],
                |r| r.get(0),
            )?;
            if has_dcp {
                return Err(unavailable("saved DCP schema unavailable"));
            }
        }
        // Reconcile only changed/deleted rows. Unchanged objects AND validity
        // intervals survive restore, rather than copying a full reference set
        // whenever the boundary moves. Members are removed before parents.
        for index in [1, 0, 2, 3, 4, 5] {
            if !exists && index < 5 {
                continue;
            }
            let (table, columns, predicate) = TABLES[index];
            let key = columns
                .split(',')
                .take(key_count(index))
                .map(|c| format!("{table}.{c}"))
                .collect::<Vec<_>>()
                .join(",");
            conn.execute(&format!("DELETE FROM {table} WHERE {predicate} AND NOT EXISTS(SELECT 1 FROM conversation_restore r WHERE r.kind={index} AND r.row_key=json_array({key}))"), [session])?;
        }
        for (kind, (table, columns, _)) in TABLES.iter().enumerate() {
            if !exists && kind < 5 {
                continue;
            }
            let values = columns
                .split(',')
                .enumerate()
                .map(|(i, _)| format!("json_extract(o.payload,'$[{i}]')"))
                .collect::<Vec<_>>()
                .join(",");
            let fields: Vec<_> = columns.split(',').collect();
            let keys = fields[..key_count(kind)].join(",");
            let updates = fields[key_count(kind)..]
                .iter()
                .map(|c| format!("{c}=excluded.{c}"))
                .collect::<Vec<_>>()
                .join(",");
            let conflict = if updates.is_empty() {
                "DO NOTHING".into()
            } else {
                let current = fields
                    .iter()
                    .map(|c| format!("{table}.{c}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let new = fields
                    .iter()
                    .map(|c| format!("excluded.{c}"))
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "DO UPDATE SET {updates} WHERE json_array({current}) IS NOT json_array({new})"
                )
            };
            conn.execute(&format!("INSERT INTO {table}({columns}) SELECT {values} FROM conversation_restore r JOIN conversation_objects o ON o.id=r.object_id WHERE r.kind=?1 ON CONFLICT({keys}) {conflict}"),[kind as i64])?;
        }
        conn.execute("DELETE FROM conversation_restore", [])?;
        Ok(())
    }

    pub(super) fn conversation_admit(
        conn: &Connection,
        turn: &str,
        session: &str,
    ) -> Result<(), StorageError> {
        let upper: Option<i64> = conn
            .query_row(
                "SELECT upper_seq FROM conversation_state WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if let Some(upper) = upper {
            conn.execute(
                "DELETE FROM conversation_redo WHERE session_id=?1",
                [session],
            )?;
            conn.execute("INSERT INTO conversation_exclusions(session_id,lower_seq,upper_seq) SELECT ?1,?2,COALESCE(MAX(seq),?2) FROM messages WHERE session_id=?1", params![session,upper])?;
            conn.execute(
                "UPDATE conversation_points SET active=0 WHERE session_id=?1 AND pre_seq>=?2",
                params![session, upper],
            )?;
            conn.execute(
                "UPDATE conversation_state SET upper_seq=NULL WHERE session_id=?1",
                [session],
            )?;
        }
        let pre: i64 = conn.query_row(
            "SELECT COALESCE(MAX(seq),0) FROM conversation_messages WHERE session_id=?1",
            [session],
            |r| r.get(0),
        )?;
        let context = Self::save_context(conn, session)?;
        conn.execute("INSERT INTO conversation_points(turn_id,session_id,pre_seq,pre_context) VALUES (?1,?2,?3,?4)",params![turn,session,pre,context])?;
        Ok(())
    }

    pub(super) fn conversation_complete(
        conn: &Connection,
        turn: &str,
        session: &str,
    ) -> Result<(), StorageError> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_points WHERE turn_id=?1 AND post_context IS NULL)",
            [turn],
            |r| r.get(0),
        )?;
        if exists {
            let post: i64 = conn.query_row(
                "SELECT COALESCE(MAX(seq),0) FROM conversation_messages WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )?;
            let context = Self::save_context(conn, session)?;
            conn.execute(
                "UPDATE conversation_points SET post_seq=?2,post_context=?3 WHERE turn_id=?1",
                params![turn, post, context],
            )?;
        }
        Ok(())
    }

    pub(crate) fn change_conversation(
        &self,
        session: &str,
        action: ConversationAction,
    ) -> Result<ConversationSnapshot, CoreError> {
        let run = || -> Result<ConversationSnapshot, StorageError> {
            let mut conn = self.conn.lock().expect("db mutex");
            let tx = conn.transaction()?;
            Self::require_session(&tx, session)?;
            let upper: i64 = tx.query_row("SELECT COALESCE((SELECT upper_seq FROM conversation_state WHERE session_id=?1),(SELECT COALESCE(MAX(seq),0) FROM messages WHERE session_id=?1))",[session],|r|r.get(0))?;
            let (turn, seq, context): (String,i64,String) = match action {
                ConversationAction::Undo => tx.query_row("SELECT p.turn_id,p.pre_seq,p.pre_context FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id WHERE p.session_id=?1 AND p.active=1 AND a.user_message=(SELECT id FROM conversation_messages WHERE session_id=?1 AND role='user' AND length(text)>0 AND seq<=?2 ORDER BY seq DESC LIMIT 1)",params![session,upper],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||unavailable("conversation undo boundary unavailable: no saved historical context"))?,
                ConversationAction::Redo => tx.query_row("SELECT '',tip_seq,context FROM conversation_redo WHERE session_id=?1",[session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||unavailable("conversation redo boundary unavailable"))?,
                ConversationAction::Revert { ref message } => tx.query_row("SELECT p.turn_id,p.pre_seq,p.pre_context FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id JOIN messages m ON m.id=a.user_message WHERE p.session_id=?1 AND p.active=1 AND a.user_message=?2 AND m.role='user'",params![session,message.0],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?.ok_or_else(||unavailable("conversation revert boundary unavailable: no saved historical context"))?,
            };
            let draft = if matches!(action, ConversationAction::Redo) {
                None
            } else {
                Some(tx.query_row("SELECT m.text FROM turn_acceptances a JOIN messages m ON m.id=a.user_message WHERE a.turn_id=?1",[turn],|r|r.get(0))?)
            };
            if matches!(action, ConversationAction::Redo) {
                let pending: String = tx.query_row(
                    "SELECT pending_context FROM conversation_redo WHERE session_id=?1",
                    [session],
                    |r| r.get(0),
                )?;
                Self::load_context_restore(&tx, session, &pending)?;
                tx.execute_batch("CREATE TEMP TABLE IF NOT EXISTS conversation_pending(kind INTEGER,object_id INTEGER,row_key TEXT); DELETE FROM conversation_pending; INSERT INTO conversation_pending SELECT * FROM conversation_restore WHERE kind=5;")?;
                Self::load_context_restore(&tx, session, &context)?;
                tx.execute_batch("DELETE FROM conversation_restore WHERE kind=5; INSERT INTO conversation_restore SELECT * FROM conversation_pending; DELETE FROM conversation_pending;")?;
                Self::apply_context_restore(&tx, session)?;
                tx.execute(
                    "DELETE FROM conversation_redo WHERE session_id=?1",
                    [session],
                )?;
            } else {
                let staged: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM conversation_state WHERE session_id=?1 AND upper_seq IS NOT NULL)",
                    [session],
                    |r| r.get(0),
                )?;
                if !staged {
                    // Preserve the last genuinely saved post revision at the original
                    // branch tip, plus DCP nudge state persisted after completion.
                    let tip: Option<(i64, String)> = tx.query_row("SELECT (SELECT MAX(seq) FROM conversation_messages WHERE session_id=?1),p.post_context FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id WHERE p.session_id=?1 AND p.active=1 AND a.user_message=(SELECT id FROM conversation_messages WHERE session_id=?1 AND role='user' ORDER BY seq DESC LIMIT 1) AND p.post_context IS NOT NULL", [session], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
                    if let Some((tip_seq, tip_context)) = tip {
                        let pending = Self::save_context(&tx, session)?;
                        tx.execute(
                            "INSERT INTO conversation_redo VALUES (?1,?2,?3,?4)",
                            params![session, tip_seq, tip_context, pending],
                        )?;
                    }
                }
                Self::restore_context(&tx, session, &context)?;
            }
            tx.execute("INSERT INTO conversation_state(session_id,upper_seq) VALUES (?1,?2) ON CONFLICT(session_id) DO UPDATE SET upper_seq=excluded.upper_seq",params![session,seq])?;
            let can_undo: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM conversation_points p JOIN turn_acceptances a ON a.turn_id=p.turn_id WHERE p.session_id=?1 AND p.active=1 AND a.user_message=(SELECT id FROM conversation_messages WHERE session_id=?1 AND role='user' AND length(text)>0 AND seq<=?2 ORDER BY seq DESC LIMIT 1))",params![session,seq],|r|r.get(0))?;
            let can_redo: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM conversation_redo WHERE session_id=?1)",
                [session],
                |r| r.get(0),
            )?;
            // Head must admit future rows (manual DCP or the next accepted turn).
            if !can_redo && matches!(action, ConversationAction::Redo) {
                tx.execute(
                    "UPDATE conversation_state SET upper_seq=NULL WHERE session_id=?1 AND NOT EXISTS(SELECT 1 FROM messages m WHERE m.session_id=?1 AND m.seq>?2 AND NOT EXISTS(SELECT 1 FROM conversation_exclusions e WHERE e.session_id=?1 AND m.seq>e.lower_seq AND m.seq<=e.upper_seq))",
                    params![session,seq],
                )?;
            }
            // Invalidate title CAS even when a provider result was already
            // queued. Moving away and back must not revive its old source.
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'conversation_changed',?2)",
                params![session, seq.to_string()],
            )?;
            let reverted = Self::reverted_projection(&tx, session)?;
            tx.commit()?;
            Ok(ConversationSnapshot {
                session: SessionId(session.into()),
                draft,
                can_undo,
                can_redo,
                reverted,
            })
        };
        run().map_err(|e| CoreError::Application(e.to_string()))
    }

    fn reverted_projection(
        conn: &Connection,
        session: &str,
    ) -> Result<Option<RevertedConversation>, StorageError> {
        let boundary: Option<i64> = conn
            .query_row(
                "SELECT upper_seq FROM conversation_state WHERE session_id=?1",
                [session],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(boundary) = boundary else {
            return Ok(None);
        };
        let (message, count): (Option<String>, i64) = conn.query_row("SELECT (SELECT m.id FROM messages m WHERE m.session_id=?1 AND m.role='user' AND m.seq>?2 AND NOT EXISTS(SELECT 1 FROM conversation_exclusions e WHERE e.session_id=?1 AND m.seq>e.lower_seq AND m.seq<=e.upper_seq) ORDER BY m.seq LIMIT 1), COUNT(*) FROM messages m WHERE m.session_id=?1 AND m.role='user' AND m.seq>?2 AND NOT EXISTS(SELECT 1 FROM conversation_exclusions e WHERE e.session_id=?1 AND m.seq>e.lower_seq AND m.seq<=e.upper_seq)", params![session,boundary], |r| Ok((r.get(0)?,r.get(1)?)))?;
        Ok(message.map(|message| RevertedConversation {
            message: oc_core::session::MessageId(message),
            user_messages: count as u64,
        }))
    }

    pub(crate) fn reverted_conversation(
        &self,
        session: &str,
    ) -> Result<Option<RevertedConversation>, StorageError> {
        Self::reverted_projection(&self.conn.lock().expect("db mutex"), session)
    }

    pub(crate) fn conversation_nudges(
        &self,
        session: &str,
    ) -> Result<Vec<(String, String)>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut statement = conn.prepare("SELECT key,value FROM prefs WHERE substr(CAST(key AS BLOB),1,length(CAST('dcp.nudge.'||?1||char(0) AS BLOB)))=CAST('dcp.nudge.'||?1||char(0) AS BLOB) ORDER BY key")?;
        Ok(statement
            .query_map([session], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
#[path = "storage_conversation_tests.rs"]
mod tests;
