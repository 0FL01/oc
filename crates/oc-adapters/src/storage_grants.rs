//! Project-scoped saved Ask allowances; never override configured Deny.
use super::*;
impl Db {
    /// A refused invocation is a terminal tool result, never an execution intent.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_turn_tool_refusal(
        &self,
        op: &str,
        session: &str,
        turn: &str,
        name: &str,
        input: &str,
        state: &str,
        output: &str,
        journal: &str,
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        let identity = Self::call_identity(op, turn, journal)?;
        tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output,provider_call_id,call_occurrence,original_input_index) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![op,session,turn,name,state,input,output,identity.as_ref().map(|v|&v.0),identity.as_ref().map(|v|v.1),identity.as_ref().map(|v|v.2)])?;
        let n = tx.execute(
            "UPDATE turns SET result=?2 WHERE id=?1 AND session_id=?3 AND status='started'",
            params![turn, journal, session],
        )?;
        if n != 1 {
            return Err(StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows));
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn grants_schema(&self) -> Result<(), StorageError> {
        self.conn.lock().expect("db lock").execute_batch("CREATE TABLE IF NOT EXISTS permission_grants(project TEXT NOT NULL, action TEXT NOT NULL, pattern TEXT NOT NULL, PRIMARY KEY(project,action,pattern));")?;
        Ok(())
    }
    pub(crate) fn permission_grant_matches(
        &self,
        project: &str,
        action: &str,
        resource: &str,
    ) -> Result<bool, StorageError> {
        let conn = self.conn.lock().expect("db lock");
        let exact: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM permission_grants WHERE project=?1 AND action=?2 AND pattern=?3)", params![project,action,resource], |row| row.get(0))?;
        if exact {
            return Ok(true);
        }
        // Streaming cursor bounds memory to one persisted pattern, without
        // dropping a successfully acknowledged grant due to lexical position.
        let mut stmt = conn.prepare("SELECT pattern FROM permission_grants WHERE project=?1 AND action=?2 AND (instr(pattern,'*')>0 OR instr(pattern,'?')>0)")?;
        let mut rows = stmt.query(params![project, action])?;
        while let Some(row) = rows.next()? {
            let pattern: String = row.get(0)?;
            // Command source text is not the legacy argv display domain. Old
            // wildcard grants never acquire command interpretation authority.
            let prefix = crate::tools::shell_call::COMMAND_GRANT_PREFIX;
            if (action != "bash" || resource.starts_with(prefix) == pattern.starts_with(prefix))
                && crate::permissions::wildcard_preserving_identity(resource, &pattern)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub(crate) fn save_permission_grants(
        &self,
        project: &str,
        action: &str,
        patterns: &[String],
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db lock");
        let tx = conn.transaction()?;
        for pattern in patterns {
            tx.execute(
                "INSERT OR IGNORE INTO permission_grants(project,action,pattern) VALUES (?1,?2,?3)",
                params![project, action, pattern],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}
