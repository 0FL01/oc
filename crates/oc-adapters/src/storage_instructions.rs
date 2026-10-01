//! Current instruction descriptors and immutable source facts in existing Db.
use super::*;
use crate::instructions::{Fact, Reference, Source};

#[derive(Default, Serialize, Deserialize)]
struct State {
    revision: u64,
    events: Vec<i64>,
}

fn key(session: &str) -> String {
    format!("instructions.{session}")
}

impl Db {
    pub(crate) fn instruction_view(&self, session: &str) -> Result<(u64, Vec<Fact>), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::instruction_view_in(&conn, session)
    }

    fn instruction_view_in(
        conn: &Connection,
        session: &str,
    ) -> Result<(u64, Vec<Fact>), StorageError> {
        let state = match Self::get_pref_bounded_in(conn, &key(session), 65536)? {
            BoundedPref::Missing => State::default(),
            BoundedPref::Value(raw) => serde_json::from_str(&raw).map_err(std::io::Error::other)?,
            BoundedPref::TooLarge => {
                return Err(StorageError::Io(std::io::Error::other(
                    "instruction metadata budget exceeded",
                )));
            }
        };
        if state.events.len() > 256 {
            return Err(StorageError::Io(std::io::Error::other(
                "instruction source budget exceeded",
            )));
        }
        let mut facts = Vec::new();
        let mut bytes = 0usize;
        for event in state.events {
            let raw: Option<String> = conn.query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=?2 THEN payload END FROM events WHERE seq=?1 AND kind='instructions_updated'",
                params![event, (crate::defs::MAX_INSTRUCTIONS_FILE * 8 + 16384) as i64], |r|r.get(0))?;
            let raw = raw.ok_or_else(|| {
                StorageError::Io(std::io::Error::other("instruction fact budget exceeded"))
            })?;
            let mut fact: Fact = serde_json::from_str(&raw).map_err(std::io::Error::other)?;
            fact.event = event;
            bytes += fact.source.content.as_ref().map_or(0, String::len);
            if bytes > crate::defs::MAX_INSTRUCTIONS_TOTAL {
                return Err(StorageError::Io(std::io::Error::other(
                    "instruction body budget exceeded",
                )));
            }
            facts.push(fact);
        }
        Ok((state.revision, facts))
    }

    /// Source IO is already complete. CAS, facts, descriptor update, positions,
    /// optional read outcome and the normal journal checkpoint commit together.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn checkpoint_instructions(
        &self,
        turn: &str,
        log: &mut crate::tools::TurnLog,
        expected: u64,
        desired: &[Source],
        index: usize,
        outcome: Option<(&str, &str, &str)>,
    ) -> Result<(), StorageError> {
        crate::instructions::validate_sources(desired).map_err(std::io::Error::other)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let session: String = tx.query_row(
            "SELECT session_id FROM conversation_turns WHERE id=?1 AND status='started'",
            [turn],
            |r| r.get(0),
        )?;
        let (revision, previous) = Self::instruction_view_in(&tx, &session)?;
        if revision != expected {
            return Err(StorageError::Io(std::io::Error::other(
                "stale instruction revision",
            )));
        }
        let mut next = Vec::new();
        let mut references = Vec::new();
        let mut removed_sources = Vec::new();
        for old in &previous {
            if !desired.iter().any(|s| s.path == old.source.path) && old.source.content.is_some() {
                let mut removed = old.source.clone();
                removed.content = None;
                removed.digest = None;
                removed_sources.push(removed);
            }
        }
        for source in desired.iter().chain(&removed_sources) {
            let old = previous.iter().find(|f| f.source.path == source.path);
            if let Some(old) = old
                && old.source.digest == source.digest
                && old.source.origin == source.origin
                && old.source.root == source.root
            {
                next.push(old.event);
                continue;
            }
            if old.is_none() && source.content.is_none() {
                continue;
            }
            let change = if source.content.is_none() {
                "removed"
            } else if old.is_some_and(|f| f.source.content.is_some()) {
                "changed"
            } else if expected == 0 {
                "initial"
            } else {
                "admitted"
            };
            let fact = Fact {
                event: 0,
                source: source.clone(),
                change: change.into(),
                revision: revision + 1,
            };
            tx.execute(
                "INSERT INTO events(session_id,kind,payload) VALUES(?1,'instructions_updated',?2)",
                params![
                    session,
                    serde_json::to_string(&fact).map_err(std::io::Error::other)?
                ],
            )?;
            let event = tx.last_insert_rowid();
            next.push(event);
            references.push(Reference { event, index });
        }
        let mut staged = log.to_json();
        staged["instruction_references"] = serde_json::to_value(
            log.instruction_references
                .iter()
                .chain(&references)
                .collect::<Vec<_>>(),
        )
        .map_err(std::io::Error::other)?;
        let state = State {
            revision: revision + 1,
            events: next,
        };
        if state.events.len() > 256 {
            return Err(StorageError::Io(std::io::Error::other(
                "instruction source budget exceeded",
            )));
        }
        tx.execute("INSERT INTO prefs(key,value,updated_at) VALUES(?1,?2,strftime('%s','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
            params![key(&session),serde_json::to_string(&state).map_err(std::io::Error::other)?])?;
        if let Some((op, status, output)) = outcome
            && tx.execute("UPDATE tool_operations SET state=?1,output=?2 WHERE id=?3 AND turn_id=?4 AND state='started'",
                params![status,output,op,turn])? != 1
        {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.execute(
            "UPDATE turns SET result=?1 WHERE id=?2 AND status='started'",
            params![staged.to_string(), turn],
        )?;
        tx.commit()?;
        log.instruction_references.extend(references);
        Ok(())
    }
}

#[cfg(test)]
#[path = "storage_instructions_tests.rs"]
mod tests;
