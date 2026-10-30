//! Current instruction descriptors and immutable source facts in existing Db.
use super::*;
use crate::instructions::{Fact, Origin, Reference, Source};
use oc_core::tool_output::{InstructionLoads, PREVIEW_BYTES, Presentation, RECORD_BYTES};

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
            let raw: Option<String> = conn.prepare_cached("SELECT CASE WHEN length(CAST(payload AS BLOB))<=?2 THEN payload END FROM events WHERE seq=?1 AND kind='instructions_updated'")?.query_row(params![event, (crate::defs::MAX_INSTRUCTIONS_FILE * 8 + 16384) as i64], |r|r.get(0))?;
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
    /// Return the same bounded presentation committed here for live publication.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn checkpoint_instructions(
        &self,
        turn: &str,
        log: &mut crate::tools::TurnLog,
        expected: u64,
        desired: &[Source],
        index: usize,
        redactions: &[String],
        outcome: Option<RecordedToolOutcome<'_>>,
    ) -> Result<Option<Presentation>, StorageError> {
        crate::instructions::validate_sources(desired).map_err(std::io::Error::other)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let session: String = tx
            .prepare_cached(
                "SELECT session_id FROM conversation_turns WHERE id=?1 AND status='started'",
            )?
            .query_row([turn], |r| r.get(0))?;
        let (revision, previous) = Self::instruction_view_in(&tx, &session)?;
        if revision != expected {
            return Err(StorageError::Io(std::io::Error::other(
                "stale instruction revision",
            )));
        }
        let mut next = Vec::new();
        let mut references = Vec::new();
        let mut removed_sources = Vec::new();
        let mut loads = InstructionLoads {
            paths: Vec::new(),
            total: 0,
        };
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
            if source.origin == Origin::Nested
                && source.content.is_some()
                && outcome
                    .as_ref()
                    .is_some_and(|outcome| outcome.state == "completed")
            {
                loads.total += 1;
                let path = std::path::Path::new(&source.path);
                let mut label = path
                    .strip_prefix(&source.root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned();
                // Redact complete labels before bounding, just like Write facts.
                crate::tools::output::redact_string(&mut label, redactions);
                let label: String = label
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                if !label.trim().is_empty()
                    && loads.paths.len() < 128
                    && loads.retained_bytes() + label.len() <= PREVIEW_BYTES
                    && !loads.paths.contains(&label)
                {
                    loads.paths.push(label);
                }
            }
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
        if let Some(outcome) = &outcome
            && tx.execute("UPDATE tool_operations SET state=?1,output=?2 WHERE id=?3 AND turn_id=?4 AND state='started'",
                params![outcome.state,outcome.output,outcome.operation,turn])? != 1
        {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        Self::write_turn_result(&tx, turn, None, true, Some(&staged.to_string()))?;
        let mut presentation = outcome
            .as_ref()
            .and_then(|outcome| outcome.presentation.cloned());
        if let Some(facts) = &mut presentation {
            // A caller's earlier snapshot is not evidence of loads in this CAS.
            facts.instructions = None;
        }
        if loads.total > 0 {
            let facts = presentation.get_or_insert_with(|| {
                // No second read or reconstruction from model-facing output.
                Presentation::new(
                    "",
                    outcome.as_ref().map_or(0, |o| o.output.len() as u64),
                    false,
                )
            });
            facts.instructions = Some(loads);
        }
        if let Some(outcome) = &outcome {
            presentation =
                Self::bounded_mcp_presentation(outcome.operation, presentation.map(Box::new))
                    .map(|facts| *facts);
            // Include the real escaped operation ID. Optional receipts must not
            // turn a successful read into an uncommitted/unknown effect.
            while presentation.as_ref().is_some_and(|facts| {
                serde_json::json!({"operation":outcome.operation,"presentation":facts})
                    .to_string()
                    .len()
                    > RECORD_BYTES
            }) {
                let facts = presentation.as_mut().expect("presentation");
                if let Some(loads) = &mut facts.instructions
                    && loads.paths.pop().is_some()
                {
                    continue;
                }
                if facts.instructions.take().is_some() {
                    continue;
                }
                presentation = None;
            }
            Self::record_tool_presentation_in(&tx, outcome.operation, presentation.as_ref())?;
        }
        tx.commit()?;
        log.instruction_references.extend(references);
        Ok(presentation)
    }
}

#[cfg(test)]
#[path = "storage_instructions_tests.rs"]
mod tests;
