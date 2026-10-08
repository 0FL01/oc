//! Same DB/transaction owner: irreducible move admission and placement facts.
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MoveRecord {
    pub operation: String,
    pub session: String,
    pub source_turn: String,
    pub source: String,
    pub directory: String,
    pub fingerprint: String,
}

impl Db {
    pub(super) fn session_move_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS session_moves(
                operation_id TEXT PRIMARY KEY REFERENCES tool_operations(id) ON DELETE CASCADE,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                source_turn TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
                source TEXT NOT NULL CHECK(length(CAST(source AS BLOB))<=4096),
                directory TEXT NOT NULL CHECK(length(CAST(directory AS BLOB))<=4096),
                fingerprint TEXT NOT NULL CHECK(length(fingerprint)=64),
                phase TEXT NOT NULL CHECK(phase IN ('pending','applied')));
            CREATE UNIQUE INDEX IF NOT EXISTS session_moves_one_pending ON session_moves(session_id) WHERE phase='pending';
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(6,'t50-session-move');
            COMMIT;")?;
        Ok(())
    }

    pub(crate) fn has_pending_move(&self, session: &str) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT EXISTS(SELECT 1 FROM session_moves WHERE session_id=?1 AND phase='pending') OR (SELECT count(*) FROM session_moves WHERE phase='pending')>=8",
            [session],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn session_move_epoch(&self, session: &str) -> Result<i64, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT COALESCE(MAX(seq),0) FROM events WHERE session_id=?1 AND kind='session_moved'",
            [session],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn move_source_is_target(&self, record: &MoveRecord) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT session_id=?2 FROM turns WHERE id=?1",
            params![record.source_turn, record.session],
            |r| r.get(0),
        )?)
    }

    pub(crate) fn check_move_deck(
        &self,
        session: &str,
        directory: &str,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let root: bool = conn.query_row(
            "SELECT parent_id IS NULL FROM sessions WHERE id=?1",
            [session],
            |r| r.get(0),
        )?;
        if root {
            Self::check_move_deck_in(&conn, session, directory)?;
        }
        Ok(())
    }

    fn check_move_deck_in(
        conn: &Connection,
        session: &str,
        directory: &str,
    ) -> Result<(), StorageError> {
        let raw =
            match Self::get_pref_bounded_in(conn, &tab_deck_key(directory), MAX_TAB_DECK_BYTES)? {
                BoundedPref::Missing => None,
                BoundedPref::Value(raw) => Some(raw),
                BoundedPref::TooLarge => return Err(invalid_stored_tab_deck()),
            };
        let deck = parse_stored_deck(raw.as_deref())?;
        let mut ids = deck
            .sessions
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        ids.extend(Self::tab_adoptions_in(conn, directory)?);
        let mut stmt=conn.prepare("SELECT m.session_id FROM session_moves m JOIN sessions s ON s.id=m.session_id WHERE m.directory=?1 AND m.phase='pending' AND m.session_id!=?2 AND s.parent_id IS NULL LIMIT 9")?;
        let pending = stmt
            .query_map(params![directory, session], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if pending.len() > 8 {
            return Err(invalid_stored_tab_deck());
        }
        ids.extend(pending);
        if ids.len() >= MAX_TABS && !ids.contains(session) {
            return Err(invalid_stored_tab_deck());
        }
        Ok(())
    }

    pub(crate) fn admit_session_move(&self, record: &MoveRecord) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id JOIN turns t ON t.id=?3 JOIN tool_operations o ON o.id=?4 AND o.turn_id=t.id WHERE s.id=?1 AND p.value=?2 AND t.status='started' AND o.state='started' AND o.name='opencode_session_move') AND (SELECT count(*) FROM session_moves WHERE phase='pending')<8",params![record.session,record.source,record.source_turn,record.operation], |r|r.get(0))?;
        if !valid {
            return Err(StorageError::SessionNotFound);
        }
        tx.execute("INSERT INTO session_moves(operation_id,session_id,source_turn,source,directory,fingerprint,phase) VALUES(?1,?2,?3,?4,?5,?6,'pending')",params![record.operation,record.session,record.source_turn,record.source,record.directory,record.fingerprint])?;
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES(?1,'session_move_admitted',?2)",
            params![
                record.session,
                serde_json::to_string(record).expect("record")
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn ready_session_moves(
        &self,
        source: &str,
    ) -> Result<Vec<MoveRecord>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        // Apply idle/child targets before the calling root retires this source
        // runtime; the authoritative DB records, not cache insertion order, own it.
        let mut stmt = conn.prepare("SELECT m.operation_id,m.session_id,m.source_turn,m.source,m.directory,m.fingerprint FROM session_moves m JOIN turns t ON t.id=m.source_turn JOIN sessions s ON s.id=m.session_id WHERE m.phase='pending' AND m.source=?1 AND t.status IN ('completed','failed','cancelled','incomplete') AND NOT EXISTS(WITH RECURSIVE family(id) AS (SELECT m.session_id UNION SELECT s.id FROM sessions s JOIN family f ON s.parent_id=f.id) SELECT 1 FROM turns t JOIN family f ON f.id=t.session_id WHERE t.status='started') ORDER BY (m.session_id=t.session_id AND s.parent_id IS NULL),m.rowid LIMIT 9")?;
        let rows = stmt
            .query_map([source], |r| {
                Ok(MoveRecord {
                    operation: r.get(0)?,
                    session: r.get(1)?,
                    source_turn: r.get(2)?,
                    source: r.get(3)?,
                    directory: r.get(4)?,
                    fingerprint: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > 8 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(rows)
    }

    pub(crate) fn apply_session_move(&self, record: &MoveRecord) -> Result<bool, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let phase: Option<String> = tx
            .query_row(
                "SELECT phase FROM session_moves WHERE operation_id=?1",
                [&record.operation],
                |r| r.get(0),
            )
            .optional()?;
        if phase.as_deref() == Some("applied") {
            return Ok(false);
        }
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM session_moves m JOIN turns t ON t.id=m.source_turn JOIN prefs p ON p.key='tui.session_location.'||m.session_id WHERE m.operation_id=?1 AND m.session_id=?2 AND m.source=?3 AND m.directory=?4 AND m.fingerprint=?5 AND m.phase='pending' AND p.value=m.source AND t.status IN ('completed','failed','cancelled','incomplete'))",params![record.operation,record.session,record.source,record.directory,record.fingerprint],|r|r.get(0))?;
        let busy: bool = tx.query_row(
            "WITH RECURSIVE family(id) AS (SELECT ?1 UNION SELECT s.id FROM sessions s JOIN family f ON s.parent_id=f.id) SELECT EXISTS(SELECT 1 FROM turns t JOIN family f ON f.id=t.session_id WHERE t.status='started')",
            [&record.session],
            |r| r.get(0),
        )?;
        if !valid || busy {
            return Err(StorageError::SessionNotFound);
        }
        let root: bool = tx.query_row(
            "SELECT parent_id IS NULL FROM sessions WHERE id=?1",
            [&record.session],
            |r| r.get(0),
        )?;
        if root && record.source != record.directory {
            Self::check_move_deck_in(&tx, &record.session, &record.directory)?;
            let key = tab_deck_key(&record.source);
            let raw = match Self::get_pref_bounded_in(&tx, &key, MAX_TAB_DECK_BYTES)? {
                BoundedPref::Missing => None,
                BoundedPref::Value(raw) => Some(raw),
                BoundedPref::TooLarge => return Err(invalid_stored_tab_deck()),
            };
            let mut deck = parse_stored_deck(raw.as_deref())?;
            if let Some(index) = deck.sessions.iter().position(|s| s == &record.session) {
                deck.sessions.remove(index);
                if !deck.new_session_titles.is_empty() {
                    deck.new_session_titles.remove(index);
                    if !deck.new_session_titles.iter().any(|value| *value) {
                        deck.new_session_titles.clear();
                    }
                }
            }
            if deck.active.as_deref() == Some(&record.session) {
                deck.active = deck.sessions.last().cloned();
            }
            if raw.is_some() {
                tx.execute(
                    "UPDATE prefs SET value=?2,updated_at=strftime('%s','now') WHERE key=?1",
                    params![key, serde_json::to_string(&deck).expect("deck")],
                )?;
            }
            tx.execute(
                "DELETE FROM prefs WHERE key=?1",
                [tab_adoption_key(&record.source, &record.session)],
            )?;
            tx.execute("INSERT OR IGNORE INTO prefs(key,value,updated_at) VALUES(?1,?2,strftime('%s','now'))",params![tab_adoption_key(&record.directory,&record.session),TAB_ADOPTION_VALUE])?;
        }
        tx.execute(
            "UPDATE prefs SET value=?2,updated_at=strftime('%s','now') WHERE key=?1",
            params![
                format!("{SESSION_LOCATION_PREFIX}{}", record.session),
                record.directory
            ],
        )?;
        tx.execute(
            "UPDATE session_moves SET phase='applied' WHERE operation_id=?1",
            [&record.operation],
        )?;
        tx.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES(?1,'session_moved',?2)",
            params![
                record.session,
                serde_json::to_string(record).expect("record")
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "storage/session_move_tests.rs"]
mod tests;
