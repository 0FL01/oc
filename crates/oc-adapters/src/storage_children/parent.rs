//! Root continuation admission from exact native child reports, not UI hints.
use super::*;
use crate::tools::{TurnLog, turn_history::NativeNoticeInput};

impl Db {
    pub(in crate::storage) fn native_child_notice_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM messages m JOIN child_jobs j ON j.message_id=m.id AND j.parent_id=m.session_id JOIN sessions c ON c.id=j.child_id AND c.parent_id=j.parent_id JOIN tool_operations o ON o.id=j.operation_id AND o.session_id=j.parent_id AND o.name='subagent' WHERE m.session_id=?1 AND m.id=?2 AND m.role='user' AND j.state NOT IN ('admitted','running') AND CASE WHEN json_valid(j.identity) THEN json_extract(j.identity,'$.parent')=j.parent_id AND json_extract(j.identity,'$.child')=j.child_id AND json_extract(j.identity,'$.operation')=j.operation_id AND json_extract(j.identity,'$.delivery_id')=j.delivery_id ELSE 0 END AND EXISTS(SELECT 1 FROM events e WHERE e.session_id=j.parent_id AND e.kind='subagent_notice' AND e.payload=j.delivery_id))",params![session,message],|row|row.get(0))?)
    }

    /// Typed journal provenance survives inert forks, which have no child jobs.
    pub(in crate::storage) fn native_notice_data_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        if Self::native_child_notice_in(conn, session, message)? {
            return Ok(true);
        }
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM turns t WHERE t.session_id=?1 AND CASE WHEN json_valid(t.result) THEN json_extract(t.result,'$.native_input.version')=1 AND json_extract(t.result,'$.native_input.kind')='subagent_notices' AND json_type(t.result,'$.native_input.messages')='array' AND json_array_length(t.result,'$.native_input.messages')<=?3 AND (json_extract(t.result,'$.user_message')=?2 OR EXISTS(SELECT 1 FROM json_each(t.result,'$.native_input.messages') n WHERE n.type='text' AND n.value=?2)) ELSE 0 END)",params![session,message,crate::runtime::children::JOB_CAP as i64],|row|row.get(0))?)
    }

    pub(crate) fn parent_notice_assigned(
        &self,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Self::parent_notice_assigned_in(&self.conn.lock().expect("db mutex"), session, message)
    }

    pub(crate) fn is_native_child_notice(
        &self,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Self::native_child_notice_in(&self.conn.lock().expect("db mutex"), session, message)
    }

    /// A journal identity is not enough: the exact canonical report must still
    /// be an emitted user item, rather than a HOT summary or a loaded-only log.
    pub(crate) fn emitted_child_notices(
        &self,
        session: &str,
        messages: &[String],
        input: &[crate::provider::InputItem],
    ) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut represented = Vec::new();
        for id in messages {
            if represented.contains(id) || !Self::native_child_notice_in(&conn, session, id)? {
                continue;
            }
            let body: Option<String> = conn.query_row("SELECT text FROM messages WHERE session_id=?1 AND id=?2 AND length(CAST(text AS BLOB))<=?3",params![session,id,crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64],|row|row.get(0)).optional()?;
            if let Some(body) = body
                && input.contains(&crate::provider::InputItem::message(
                    crate::provider::InputRole::User,
                    body,
                ))
            {
                represented.push(id.clone());
            }
        }
        Ok(represented)
    }

    fn parent_notice_assigned_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM events WHERE session_id=?1 AND kind='subagent_parent_notice_consumed' AND CASE WHEN json_valid(payload) THEN json_extract(payload,'$.message')=?2 ELSE 0 END)",params![session,message],|row|row.get(0))?)
    }

    fn assign_parent_notice_in(
        conn: &Connection,
        session: &str,
        message: &str,
        turn: &str,
    ) -> Result<(), StorageError> {
        if !Self::parent_notice_assigned_in(conn, session, message)? {
            conn.execute("INSERT INTO events(session_id,kind,payload) VALUES(?1,'subagent_parent_notice_consumed',?2)",params![session,serde_json::json!({"message":message,"turn":turn}).to_string()])?;
        }
        Ok(())
    }

    /// Complete, chronological bodies after SQL length preflight; no composer
    /// limit or result truncation. The full request still has its normal budget.
    pub(crate) fn parent_notice_input(
        &self,
        session: &str,
        messages: &[String],
    ) -> Result<(Vec<String>, crate::provider::InputItem), StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        if messages.is_empty() || messages.len() > crate::runtime::children::JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        let mut ordered = Vec::new();
        let mut bytes = 0usize;
        for message in messages {
            if ordered.iter().any(|(_, id, _)| id == message) {
                continue;
            }
            if !Self::native_child_notice_in(&conn, session, message)? {
                return Err(StorageError::OperationNotFound);
            }
            let (seq, length): (i64, i64) = conn.query_row("SELECT seq,length(CAST(text AS BLOB)) FROM conversation_messages WHERE session_id=?1 AND id=?2",params![session,message],|row|Ok((row.get(0)?,row.get(1)?)))?;
            let length = usize::try_from(length).map_err(|_| StorageError::OperationNotFound)?;
            bytes = bytes.saturating_add(length).saturating_add(2);
            if bytes > crate::runtime::ACTIVE_CONTEXT_BYTES_CAP {
                return Err(StorageError::OperationNotFound);
            }
            ordered.push((seq, message.clone(), length));
        }
        ordered.sort_by_key(|(seq, ..)| *seq);
        let mut text = String::with_capacity(bytes);
        let mut ids = Vec::new();
        for (_, id, _) in ordered {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            let body: String = conn.query_row(
                "SELECT text FROM conversation_messages WHERE session_id=?1 AND id=?2",
                params![session, id],
                |row| row.get(0),
            )?;
            text.push_str(&body);
            ids.push(id);
        }
        Ok((
            ids,
            crate::provider::InputItem::message(crate::provider::InputRole::User, text),
        ))
    }

    pub(crate) fn accept_parent_notice_batch(
        &self,
        session: &str,
        location: &str,
        messages: &[String],
        model: &oc_core::queries::ModelRef,
        reminder: Option<&str>,
        seed: &mut TurnLog,
    ) -> Result<Option<AcceptedTurn>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        if messages.is_empty() || messages.len() > crate::runtime::children::JOB_CAP {
            return Err(StorageError::OperationNotFound);
        }
        let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN prefs p ON p.key='tui.session_location.'||s.id WHERE s.id=?1 AND s.parent_id IS NULL AND p.value=?2) AND NOT EXISTS(SELECT 1 FROM conversation_state WHERE session_id=?1 AND upper_seq IS NOT NULL) AND NOT EXISTS(SELECT 1 FROM turns WHERE session_id=?1 AND status='started')",params![session,location],|row|row.get(0))?;
        if !valid {
            return Ok(None);
        }
        for (index, message) in messages.iter().enumerate() {
            if messages[..index].contains(message) || !Self::native_child_notice_in(&tx, session, message)? || Self::parent_notice_assigned_in(&tx, session, message)? || !tx.query_row("SELECT EXISTS(SELECT 1 FROM conversation_messages WHERE session_id=?1 AND id=?2)",params![session,message],|row|row.get::<_,bool>(0))? { return Ok(None) }
        }
        seed.native_input = Some(NativeNoticeInput::new(messages.to_vec()));
        let accepted = Self::insert_accepted_turn(
            &tx,
            &seed.turn_id.clone(),
            session,
            "",
            "",
            model,
            reminder,
            None,
            Some(seed),
        )?;
        for message in messages {
            Self::assign_parent_notice_in(&tx, session, message, &seed.turn_id)?;
        }
        tx.commit()?;
        Ok(Some(accepted))
    }

    /// Assignment means durable request admission, never remote exactly-once.
    pub(crate) fn checkpoint_turn_with_child_notices(
        &self,
        turn: &str,
        session: &str,
        result: &str,
        represented: &[String],
    ) -> Result<(), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        if tx.execute(
            "UPDATE turns SET result=?1 WHERE id=?2 AND session_id=?3 AND status='started'",
            params![result, turn, session],
        )? != 1
        {
            return Err(StorageError::OperationNotFound);
        }
        for message in represented {
            if Self::native_child_notice_in(&tx, session, message)? {
                Self::assign_parent_notice_in(&tx, session, message, turn)?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
