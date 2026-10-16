//! Minimal identity/lifecycle/selection metadata, in the existing Db/flock.
use super::*;
use crate::shell::jobs::ProcessIdentity;
use oc_core::queries::{TerminalEntry, TerminalRef, TerminalState};

impl Db {
    pub(crate) fn terminal_order(&self, id: &str) -> Result<i64, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT rowid FROM terminals WHERE id=?1",
            [id],
            |r| r.get(0),
        )?)
    }
    pub(super) fn terminals_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS terminals(
                id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                entry TEXT NOT NULL, process TEXT, live INTEGER NOT NULL CHECK(live IN (0,1)));
            CREATE INDEX IF NOT EXISTS terminals_live ON terminals(live) WHERE live=1;
            CREATE TABLE IF NOT EXISTS terminal_selection(
                session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
                terminal_id TEXT REFERENCES terminals(id) ON DELETE SET NULL);
            INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(12,'t56-terminals');
            COMMIT;")?;
        Ok(())
    }

    pub(crate) fn record_terminal(
        &self,
        entry: &TerminalEntry,
        process: &ProcessIdentity,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, &entry.target.session.0)?;
        conn.execute(
            "INSERT INTO terminals(id,session_id,entry,process,live) VALUES(?1,?2,?3,?4,1)",
            params![
                entry.target.id,
                entry.target.session.0,
                serde_json::to_string(entry).map_err(|_| StorageError::OperationNotFound)?,
                serde_json::to_string(process).map_err(|_| StorageError::OperationNotFound)?
            ],
        )?;
        Ok(())
    }

    pub(crate) fn finish_terminal(&self, entry: &TerminalEntry) -> Result<(), StorageError> {
        self.update_terminal(entry, false)
    }

    pub(crate) fn fail_terminal_cleanup(&self, entry: &TerminalEntry) -> Result<(), StorageError> {
        self.update_terminal(entry, true)
    }

    fn update_terminal(&self, entry: &TerminalEntry, live: bool) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE terminals SET entry=?2,live=?3 WHERE id=?1",
            params![
                entry.target.id,
                serde_json::to_string(entry).map_err(|_| StorageError::OperationNotFound)?,
                live
            ],
        )?;
        tx.execute(
            "UPDATE terminal_selection SET terminal_id=NULL WHERE terminal_id=?1",
            [&entry.target.id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn select_terminal(
        &self,
        session: &str,
        selected: Option<&TerminalRef>,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        conn.execute("INSERT INTO terminal_selection(session_id,terminal_id) VALUES(?1,?2) ON CONFLICT(session_id) DO UPDATE SET terminal_id=excluded.terminal_id WHERE terminal_selection.terminal_id IS NOT excluded.terminal_id", params![session, selected.map(|s| &s.id)])?;
        Ok(())
    }

    pub(crate) fn selected_terminal(&self, session: &str) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::require_session(&conn, session)?;
        Ok(conn
            .query_row(
                "SELECT terminal_id FROM terminal_selection WHERE session_id=?1",
                [session],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    pub(crate) fn recover_terminals(&self) -> Result<(), StorageError> {
        let pending = {
            let conn = self.conn.lock().expect("db mutex");
            let mut stmt = conn.prepare(
                "SELECT entry,process FROM terminals WHERE live=1 ORDER BY rowid LIMIT 9",
            )?;
            stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?
        };
        if pending.len() > crate::terminals::TERMINAL_CAP {
            return Err(StorageError::OperationNotFound);
        }
        for (raw, process) in pending {
            let mut entry: TerminalEntry =
                serde_json::from_str(&raw).map_err(|_| StorageError::OperationNotFound)?;
            let process: ProcessIdentity =
                serde_json::from_str(&process).map_err(|_| StorageError::OperationNotFound)?;
            crate::terminals::quarantine(&process, self.root())
                .map_err(|_| StorageError::OperationNotFound)?;
            entry.state = TerminalState::Interrupted;
            entry.exit = None;
            self.finish_terminal(&entry)?;
        }
        Ok(())
    }
}
