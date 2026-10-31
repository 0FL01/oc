//! Write-ahead command interpolation, including effects before a fresh session
//! exists. This is not conversation history and never confers replay authority.
use super::*;

pub(crate) const RECOVERY_CAP: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Effect {
    pub command: String,
    pub state: String,
    pub identity: Option<crate::shell::jobs::ProcessIdentity>,
    pub output: Option<String>,
}

impl Db {
    pub(super) fn command_schema(conn: &Connection) -> Result<(), StorageError> {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS command_resolutions(
            id TEXT PRIMARY KEY, location TEXT NOT NULL, session_id TEXT NOT NULL,
            original TEXT NOT NULL, state TEXT NOT NULL, effects TEXT NOT NULL,
            prompt TEXT, recoverable INTEGER NOT NULL DEFAULT 0);
            CREATE INDEX IF NOT EXISTS command_resolutions_location ON command_resolutions(location);")?;
        Ok(())
    }

    pub(crate) fn begin_command_resolution(
        &self,
        id: &str,
        location: &str,
        session: &str,
        original: &str,
        commands: &[String],
    ) -> Result<(), StorageError> {
        if original.len() > oc_core::session::MAX_INPUT_BYTES || commands.len() > 256 {
            return Err(StorageError::InboxFull);
        }
        let effects: Vec<_> = commands
            .iter()
            .map(|command| Effect {
                command: command.clone(),
                state: "pending".into(),
                identity: None,
                output: None,
            })
            .collect();
        let raw = serde_json::to_string(&effects).map_err(std::io::Error::other)?;
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM command_resolutions WHERE location=?1",
            [location],
            |r| r.get(0),
        )?;
        if count >= RECOVERY_CAP as i64 {
            return Err(StorageError::InboxFull);
        }
        tx.execute("INSERT INTO command_resolutions(id,location,session_id,original,state,effects) VALUES(?1,?2,?3,?4,'resolving',?5)", params![id,location,session,original,raw])?;
        tx.commit()?;
        Ok(())
    }

    /// Every mutation is fenced by the parent state and occurrence index. The
    /// caller waits for the writer's durability fence before spawning.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn command_effect(
        &self,
        id: &str,
        ordinal: usize,
        expected: &str,
        state: &str,
        identity: Option<&crate::shell::jobs::ProcessIdentity>,
        output: Option<&str>,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let raw: String = tx.query_row(
            "SELECT effects FROM command_resolutions WHERE id=?1 AND state='resolving'",
            [id],
            |r| r.get(0),
        )?;
        let mut effects: Vec<Effect> = serde_json::from_str(&raw).map_err(std::io::Error::other)?;
        let effect = effects
            .get_mut(ordinal)
            .ok_or(StorageError::OperationNotFound)?;
        if effect.state != expected {
            return Err(StorageError::OperationNotFound);
        }
        effect.state = state.into();
        if let Some(identity) = identity {
            effect.identity = Some(identity.clone());
        }
        if let Some(output) = output {
            effect.output = Some(output.into());
        }
        if effects
            .iter()
            .filter_map(|e| e.output.as_ref())
            .map(String::len)
            .sum::<usize>()
            > oc_core::session::MAX_INPUT_BYTES
        {
            return Err(StorageError::InboxFull);
        }
        tx.execute(
            "UPDATE command_resolutions SET effects=?2,recoverable=1 WHERE id=?1",
            params![
                id,
                serde_json::to_string(&effects).map_err(std::io::Error::other)?
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn resolve_command_result(
        &self,
        id: &str,
        prompt: &str,
    ) -> Result<(), StorageError> {
        if prompt.len() > oc_core::session::MAX_INPUT_BYTES {
            return Err(StorageError::InboxFull);
        }
        let conn = self.conn.lock().expect("db mutex");
        let changed = conn.execute("UPDATE command_resolutions SET state='resolved',prompt=?2 WHERE id=?1 AND state='resolving' AND NOT EXISTS(SELECT 1 FROM json_each(effects) WHERE json_extract(value,'$.state')!='completed')", params![id,prompt])?;
        if changed != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(crate) fn fail_command_resolution(&self, id: &str) -> Result<(), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        conn.execute(
            "DELETE FROM command_resolutions WHERE id=?1 AND recoverable=0",
            [id],
        )?;
        conn.execute("UPDATE command_resolutions SET state=CASE WHEN EXISTS(SELECT 1 FROM json_each(effects) WHERE json_extract(value,'$.state') IN ('started','unknown')) THEN 'unknown' ELSE 'failed' END WHERE id=?1 AND state IN ('resolving','resolved')", [id])?;
        Ok(())
    }

    pub(super) fn accept_command_resolution_in(
        conn: &Connection,
        id: &str,
        session: &str,
        prompt: &str,
    ) -> Result<(), StorageError> {
        let changed = conn.execute("DELETE FROM command_resolutions WHERE id=?1 AND session_id=?2 AND prompt=?3 AND state='resolved'", params![id,session,prompt])?;
        if changed != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(crate) fn command_recovery(
        &self,
        location: &str,
        action: oc_core::queries::CommandRecoveryAction,
    ) -> Result<oc_core::queries::CommandRecovery, StorageError> {
        use oc_core::queries::{CommandRecovery, CommandRecoveryAction, InterruptedCommand};
        let conn = self.conn.lock().expect("db mutex");
        let mut original = None;
        match action {
            CommandRecoveryAction::List => {}
            CommandRecoveryAction::Read { id } => {
                original = Some(conn.query_row("SELECT original FROM command_resolutions WHERE id=?1 AND location=?2 AND state IN ('failed','unknown')", params![id,location], |r| r.get(0))?);
            }
            CommandRecoveryAction::Dismiss { id } => {
                conn.execute("DELETE FROM command_resolutions WHERE id=?1 AND location=?2 AND state IN ('failed','unknown')", params![id,location])?;
            }
        }
        let items = conn.prepare_cached("SELECT id,state FROM command_resolutions WHERE location=?1 AND state IN ('failed','unknown') ORDER BY rowid LIMIT 32")?
            .query_map([location], |r| Ok(InterruptedCommand {id:r.get(0)?,state:r.get(1)?}))?.collect::<Result<_,_>>()?;
        Ok(CommandRecovery { items, original })
    }

    pub(crate) fn recover_command_resolutions(&self) -> Result<(), StorageError> {
        // The pending quota also bounds recovery per Location. Never spawn on reopen.
        let rows: Vec<(String,String)> = self.conn.lock().expect("db mutex").prepare("SELECT id,effects FROM command_resolutions WHERE state IN ('resolving','resolved')")?
            .query_map([], |r| Ok((r.get(0)?,r.get(1)?)))?.collect::<Result<_,_>>()?;
        for (id, raw) in rows {
            let effects: Vec<Effect> = serde_json::from_str(&raw).map_err(std::io::Error::other)?;
            for effect in effects {
                if effect.state == "started"
                    && let Some(identity) = effect.identity
                {
                    identity.quarantine(self.root());
                }
            }
            let conn = self.conn.lock().expect("db mutex");
            conn.execute(
                "DELETE FROM command_resolutions WHERE id=?1 AND recoverable=0",
                [&id],
            )?;
            conn.execute(
                "UPDATE command_resolutions SET state='unknown' WHERE id=?1",
                [&id],
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::queries::CommandRecoveryAction;

    #[test]
    fn command_shell_crash_recovery_is_location_scoped_and_never_replays_partial_effects() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let effect = dir.path().join("effect");
        let source = format!("printf X >> {}", effect.display());
        db.begin_command_resolution(
            "resolution",
            "/first",
            "candidate",
            "/check Ω",
            &[source.clone(), source],
        )
        .unwrap();
        db.command_effect("resolution", 0, "pending", "started", None, None)
            .unwrap();
        fs::write(&effect, "X").unwrap();
        db.command_effect("resolution", 0, "started", "completed", None, Some("first"))
            .unwrap();
        db.command_effect("resolution", 1, "pending", "started", None, None)
            .unwrap();
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        db.recover_command_resolutions().unwrap();
        db.recover_command_resolutions().unwrap();
        assert_eq!(fs::read_to_string(&effect).unwrap(), "X");
        let items = db
            .command_recovery("/first", CommandRecoveryAction::List)
            .unwrap()
            .items;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].state, "unknown");
        assert!(
            db.command_recovery("/second", CommandRecoveryAction::List)
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            db.command_recovery(
                "/second",
                CommandRecoveryAction::Read {
                    id: "resolution".into()
                }
            )
            .is_err()
        );
        assert_eq!(
            db.command_recovery(
                "/first",
                CommandRecoveryAction::Read {
                    id: "resolution".into()
                }
            )
            .unwrap()
            .original
            .as_deref(),
            Some("/check Ω")
        );
        assert!(db.list_sessions().unwrap().is_empty());
        assert!(
            db.command_effect("resolution", 1, "pending", "started", None, None)
                .is_err()
        );
        assert!(
            db.command_recovery(
                "/first",
                CommandRecoveryAction::Dismiss {
                    id: "resolution".into()
                }
            )
            .unwrap()
            .items
            .is_empty()
        );
    }
}
