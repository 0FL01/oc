//! Durable terminal/read watermarks. Family busy masks unread; it never rewrites it.
use super::*;
use oc_core::{
    domain::SessionId,
    queries::{SessionActivity, SessionIdleOutcome},
};

impl Db {
    pub(super) fn activity_schema(conn: &Connection) -> Result<(), StorageError> {
        let columns = conn
            .prepare("PRAGMA table_info(sessions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        for (name, ty) in [
            ("idle_seq", "INTEGER"),
            ("viewed_seq", "INTEGER"),
            ("idle_outcome", "TEXT"),
        ] {
            if !columns.iter().any(|column| column == name) {
                // Legacy activity is unknown: do not create unread from old history.
                conn.execute_batch(&format!("ALTER TABLE sessions ADD COLUMN {name} {ty}"))?;
            }
        }
        Ok(())
    }

    pub(crate) fn session_root(&self, session: &str) -> Result<SessionId, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.query_row(
            "WITH RECURSIVE lineage(id,parent_id) AS (
            SELECT id,parent_id FROM sessions WHERE id=?1
            UNION SELECT s.id,s.parent_id FROM sessions s JOIN lineage c ON c.parent_id=s.id)
            SELECT id FROM lineage WHERE parent_id IS NULL",
            [session],
            |row| row.get(0),
        )
        .optional()?
        .map(SessionId)
        .ok_or(StorageError::SessionNotFound)
    }

    fn activity_in(conn: &Connection, session: &str) -> Result<SessionActivity, StorageError> {
        conn.query_row("SELECT idle_seq,viewed_seq,idle_outcome,
            EXISTS(WITH RECURSIVE family(id) AS (
                SELECT ?1 UNION SELECT s.id FROM sessions s JOIN family f ON s.parent_id=f.id)
                SELECT 1 FROM turns t JOIN family f ON t.session_id=f.id WHERE t.status='started'
                UNION ALL SELECT 1 FROM session_inbox i JOIN family f ON i.session_id=f.id WHERE i.message_id IS NULL
                UNION ALL SELECT 1 FROM child_jobs j JOIN family f ON j.parent_id=f.id WHERE j.state IN ('admitted','running'))
            FROM sessions WHERE id=?1", [session], |row| {
                let outcome: Option<String> = row.get(2)?;
                Ok(SessionActivity {
                    session: SessionId(session.to_owned()),
                    idle_seq: row.get::<_, Option<i64>>(0)?.map(|seq| seq as u64),
                    viewed_seq: row.get::<_, Option<i64>>(1)?.map(|seq| seq as u64),
                    outcome: outcome.map(|value| if value == "error" { SessionIdleOutcome::Error } else { SessionIdleOutcome::Activity }),
                    busy: row.get(3)?,
                })
            }).optional()?.ok_or(StorageError::SessionNotFound)
    }

    pub(crate) fn session_activity(
        &self,
        sessions: &[SessionId],
    ) -> Result<Vec<SessionActivity>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        sessions
            .iter()
            .map(|session| Self::activity_in(&conn, &session.0))
            .collect()
    }

    pub(crate) fn session_viewed(
        &self,
        session: &str,
        idle_seq: u64,
    ) -> Result<(SessionActivity, bool), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let before = Self::activity_in(&tx, session)?;
        // A stale captured watermark may advance viewed, but cannot cover a newer terminal.
        let changed = before.idle_seq.is_some_and(|idle| idle_seq <= idle)
            && before.viewed_seq.is_none_or(|viewed| viewed < idle_seq);
        if changed {
            tx.execute(
                "UPDATE sessions SET viewed_seq=?2 WHERE id=?1",
                params![
                    session,
                    i64::try_from(idle_seq).expect("bounded by stored idle")
                ],
            )?;
        }
        let activity = Self::activity_in(&tx, session)?;
        tx.commit()?;
        Ok((activity, changed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_work_masks_root_activity_without_replacing_its_terminal() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_session("root").unwrap();
        db.create_child_session("root", "child", None, None, None)
            .unwrap();
        let ids = [SessionId("root".into())];
        assert_eq!(db.session_root("child").unwrap(), ids[0]);
        let queued = db
            .inbox_add("child", oc_core::queries::InboxDelivery::Queue, "pending")
            .unwrap();
        db.begin_turn("root-turn", "root", "one").unwrap();
        db.finish_turn("root-turn", "completed", None).unwrap();
        let pending = db.session_activity(&ids).unwrap().remove(0);
        assert!(pending.busy);
        assert_eq!(
            pending.unread(),
            Some(SessionIdleOutcome::Activity),
            "busy masks, not delays, idle metadata"
        );
        db.inbox_remove("child", &queued.id).unwrap();
        db.begin_turn("child-turn", "child", "two").unwrap();
        assert!(db.session_activity(&ids).unwrap()[0].busy);
        db.finish_turn("child-turn", "failed", None).unwrap();
        let idle = db.session_activity(&ids).unwrap().remove(0);
        assert!(!idle.busy);
        assert_eq!(idle.idle_seq, pending.idle_seq);
        assert_eq!(
            idle.unread(),
            Some(SessionIdleOutcome::Activity),
            "a child error is not the root outcome"
        );
    }

    #[test]
    fn terminal_watermarks_survive_reopen_and_captured_views_cannot_cover_new_activity() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open(tmp.path()).unwrap();
        db.create_session("root").unwrap();
        let ids = [SessionId("root".into())];
        assert_eq!(db.session_activity(&ids).unwrap()[0].unread(), None);
        db.begin_turn("first", "root", "one").unwrap();
        db.finish_turn("first", "completed", None).unwrap();
        let first = db.session_activity(&ids).unwrap().remove(0);
        let idle = first.idle_seq.unwrap();
        assert_eq!(first.unread(), Some(SessionIdleOutcome::Activity));
        db.finish_turn("first", "failed", None).unwrap();
        assert_eq!(
            db.session_activity(&ids).unwrap()[0],
            first,
            "first settlement wins"
        );
        db.begin_turn("second", "root", "two").unwrap();
        db.finish_turn("second", "failed", None).unwrap();
        let second = db.session_activity(&ids).unwrap().remove(0);
        assert!(second.idle_seq.unwrap() > idle);
        let updated = db.session_list("", None).unwrap()[0].updated_at.clone();
        assert!(
            !db.session_viewed("root", second.idle_seq.unwrap() + 1)
                .unwrap()
                .1
        );
        let (stale, changed) = db.session_viewed("root", idle).unwrap();
        assert!(changed);
        assert_eq!(stale.unread(), Some(SessionIdleOutcome::Error));
        assert!(!db.session_viewed("root", idle).unwrap().1);
        assert_eq!(db.session_list("", None).unwrap()[0].updated_at, updated);
        drop(db);
        let db = Db::open(tmp.path()).unwrap();
        assert_eq!(db.session_activity(&ids).unwrap()[0], stale);
        let (read, changed) = db.session_viewed("root", second.idle_seq.unwrap()).unwrap();
        assert!(changed);
        assert_eq!(read.unread(), None);
        db.begin_turn("shutdown", "root", "three").unwrap();
        db.commit_turn_activity("shutdown", "interrupted", None, None, false)
            .unwrap();
        assert_eq!(
            db.session_activity(&ids).unwrap()[0],
            read,
            "owner shutdown is not unread activity"
        );
        db.begin_turn("cancel", "root", "four").unwrap();
        db.finish_turn("cancel", "cancelled", None).unwrap();
        assert_eq!(
            db.session_activity(&ids).unwrap()[0].unread(),
            Some(SessionIdleOutcome::Activity)
        );
        assert!(matches!(
            db.session_viewed("missing", idle),
            Err(StorageError::SessionNotFound)
        ));
    }
}
