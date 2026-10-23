//! Durable pending input. Payloads are hydrated individually; list/event reads
//! carry bounded previews only. Delivery and its acceptance share a transaction.
use super::*;
use oc_core::{
    prompt::PromptInput,
    queries::{InboxDelivered, InboxDelivery, InboxItem},
    session::CoreError,
};

pub const MAX_INBOX_ITEMS: usize = oc_core::session::MAX_QUEUE_ITEMS;
const MAX_PENDING_BYTES: usize = MAX_INBOX_ITEMS * oc_core::session::MAX_INPUT_BYTES;
const MAX_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CommandCapture {
    pub id: String,
    pub generation: u64,
    #[serde(default)]
    pub digest: Option<String>,
    pub original: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct InboxPayload {
    pub prompt: PromptInput,
    pub command: Option<CommandCapture>,
}

impl InboxPayload {
    pub(crate) fn original(&self) -> PromptInput {
        self.command
            .as_ref()
            .map_or_else(|| self.prompt.clone(), |c| c.original.clone().into())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PendingInput {
    pub item: InboxItem,
    pub payload: InboxPayload,
}

pub(crate) struct InboxSteer {
    pub pending: PendingInput,
    pub input_index: usize,
}

fn invalid() -> StorageError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid pending input").into()
}

impl Db {
    pub(super) fn inbox_schema(conn: &Connection) -> Result<(), StorageError> {
        let tx = conn.unchecked_transaction()?;
        let conn = &tx;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS session_inbox(
            id TEXT PRIMARY KEY, session_id TEXT NOT NULL REFERENCES sessions(id),
            delivery TEXT NOT NULL CHECK(delivery IN ('steer','queue')),
            text TEXT NOT NULL, message_id TEXT UNIQUE REFERENCES messages(id));
            CREATE INDEX IF NOT EXISTS session_inbox_pending
            ON session_inbox(session_id) WHERE message_id IS NULL;",
        )?;
        // Forward upgrade of the previously admitted text-only inbox. Delivered
        // links remain immutable; absent payload means an original plain prompt.
        let columns = conn
            .prepare("PRAGMA table_info(session_inbox)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<HashSet<_>, _>>()?;
        for (name, ty) in [
            ("payload", "TEXT"),
            ("images", "INTEGER NOT NULL DEFAULT 0"),
            ("error", "TEXT"),
            ("turn_id", "TEXT REFERENCES turns(id)"),
            ("user_images", "TEXT"),
            ("command", "TEXT"),
        ] {
            if !columns.contains(name) {
                conn.execute_batch(&format!("ALTER TABLE session_inbox ADD COLUMN {name} {ty}"))?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn inbox_add(
        &self,
        session: &str,
        delivery: InboxDelivery,
        text: &str,
    ) -> Result<InboxItem, StorageError> {
        self.inbox_add_rich(
            session,
            delivery,
            &InboxPayload {
                prompt: text.into(),
                command: None,
            },
            None,
        )
        .map(|(item, _)| item)
    }

    pub(crate) fn inbox_add_rich(
        &self,
        session: &str,
        delivery: InboxDelivery,
        payload: &InboxPayload,
        selection: Option<(&str, &str)>,
    ) -> Result<(InboxItem, Vec<InboxItem>), StorageError> {
        payload.prompt.check_budget().map_err(|_| invalid())?;
        let raw = serde_json::to_string(payload).map_err(|_| invalid())?;
        if raw.len() > MAX_PAYLOAD_BYTES {
            return Err(StorageError::InboxFull);
        }
        let original = payload.original();
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::require_session(&tx, session)?;
        let (count, bytes): (i64, i64) = tx
            .prepare_cached(
                "SELECT count(*),COALESCE(sum(length(CAST(COALESCE(payload,text) AS BLOB))),0)
            FROM session_inbox WHERE session_id=?1 AND message_id IS NULL",
            )?
            .query_row([session], |r| Ok((r.get(0)?, r.get(1)?)))?;
        if count as usize >= MAX_INBOX_ITEMS
            || (bytes as usize).saturating_add(raw.len()) > MAX_PENDING_BYTES
        {
            return Err(StorageError::InboxFull);
        }
        let id: String = tx
            .prepare_cached("SELECT 'in-'||lower(hex(randomblob(16)))")?
            .query_row([], |r| r.get(0))?;
        tx.execute(
            "INSERT INTO session_inbox(id,session_id,delivery,text,payload,images)
            VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                id,
                session,
                delivery.as_str(),
                original.text,
                raw,
                original.images.len() as i64
            ],
        )?;
        if let Some((key, value)) = selection {
            Self::upsert_pref(&tx, key, value)?;
        }
        let item = Self::inbox_item_in(&tx, session, &id)?.ok_or_else(invalid)?;
        let items = Self::inbox_pending_in(&tx, session)?;
        tx.commit()?;
        Ok((item, items))
    }

    fn inbox_item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<InboxItem> {
        let delivery: String = row.get(1)?;
        let mut text: String = row.get(2)?;
        let bytes = oc_core::tool_output::PREVIEW_BYTES;
        if text.len() > bytes {
            text.truncate(text.floor_char_boundary(bytes));
            text.push('…');
        }
        let raw: Option<String> = row.get(4)?;
        Ok(InboxItem {
            id: row.get(0)?,
            delivery: InboxDelivery::parse(&delivery).ok_or(rusqlite::Error::InvalidQuery)?,
            text,
            images: usize::try_from(row.get::<_, i64>(3)?)
                .map_err(|_| rusqlite::Error::InvalidQuery)?,
            error: raw
                .map(|r| serde_json::from_str(&r).map_err(|_| rusqlite::Error::InvalidQuery))
                .transpose()?,
        })
    }

    fn inbox_item_in(
        conn: &Connection,
        session: &str,
        id: &str,
    ) -> Result<Option<InboxItem>, StorageError> {
        Ok(conn.prepare_cached("SELECT id,delivery,substr(text,1,2049),images,CASE WHEN error IS NULL OR length(CAST(error AS BLOB))<=65536 THEN error ELSE 'invalid' END FROM session_inbox
            WHERE session_id=?1 AND id=?2 AND message_id IS NULL")?.query_row(params![session,id],Self::inbox_item_row).optional()?)
    }

    pub fn inbox_pending(&self, session: &str) -> Result<Vec<InboxItem>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Self::inbox_pending_in(&conn, session)
    }

    fn inbox_pending_in(conn: &Connection, session: &str) -> Result<Vec<InboxItem>, StorageError> {
        Ok(conn.prepare("SELECT id,delivery,substr(text,1,2049),images,CASE WHEN error IS NULL OR length(CAST(error AS BLOB))<=65536 THEN error ELSE 'invalid' END FROM session_inbox
            WHERE session_id=?1 AND message_id IS NULL ORDER BY rowid")?
            .query_map([session],Self::inbox_item_row)?.collect::<Result<_,_>>()?)
    }

    pub(crate) fn inbox_read(
        &self,
        session: &str,
        id: &str,
    ) -> Result<Option<PendingInput>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let Some(item) = Self::inbox_item_in(&conn, session, id)? else {
            return Ok(None);
        };
        let (raw, text, legacy): (Option<String>, String, bool) = conn
            .prepare_cached(
                "SELECT CASE WHEN length(CAST(payload AS BLOB))<=?3 THEN payload END,
            CASE WHEN length(CAST(text AS BLOB))<=?4 THEN text ELSE NULL END,payload IS NULL
            FROM session_inbox WHERE session_id=?1 AND id=?2 AND message_id IS NULL",
            )?
            .query_row(
                params![
                    session,
                    id,
                    MAX_PAYLOAD_BYTES as i64,
                    oc_core::session::MAX_INPUT_BYTES as i64
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
        let payload: InboxPayload = match raw {
            Some(raw) => serde_json::from_str(&raw).map_err(|_| invalid())?,
            None if legacy => InboxPayload {
                prompt: text.into(),
                command: None,
            },
            None => return Err(invalid()),
        };
        payload.prompt.check_budget().map_err(|_| invalid())?;
        Ok(Some(PendingInput { item, payload }))
    }

    pub fn inbox_set_delivery(
        &self,
        session: &str,
        id: &str,
        delivery: InboxDelivery,
    ) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").execute(
            "UPDATE session_inbox SET delivery=?3,error=NULL
            WHERE session_id=?1 AND id=?2 AND message_id IS NULL",
            params![session, id, delivery.as_str()],
        )? == 1)
    }

    pub(crate) fn inbox_fail(
        &self,
        session: &str,
        item: &InboxItem,
        error: &CoreError,
    ) -> Result<(), StorageError> {
        let raw = serde_json::to_string(error).map_err(|_| invalid())?;
        if raw.len() > 65536 {
            return Err(invalid());
        }
        self.conn.lock().expect("db mutex").execute(
            "UPDATE session_inbox SET error=?3
            WHERE session_id=?1 AND id=?2 AND message_id IS NULL AND delivery=?4",
            params![session, item.id, raw, item.delivery.as_str()],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn inbox_remove(&self, session: &str, id: &str) -> Result<Option<InboxItem>, StorageError> {
        self.inbox_remove_snapshot(session, id)
            .map(|(item, _)| item)
    }

    pub(crate) fn inbox_remove_snapshot(
        &self,
        session: &str,
        id: &str,
    ) -> Result<(Option<InboxItem>, Vec<InboxItem>), StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let item = Self::inbox_item_in(&tx, session, id)?;
        tx.execute(
            "DELETE FROM session_inbox WHERE session_id=?1 AND id=?2 AND message_id IS NULL",
            params![session, id],
        )?;
        let items = Self::inbox_pending_in(&tx, session)?;
        tx.commit()?;
        Ok((item, items))
    }

    pub(crate) fn inbox_next(
        &self,
        session: &str,
        queue: bool,
    ) -> Result<Option<PendingInput>, StorageError> {
        let id: Option<String> = self.conn.lock().expect("db mutex").query_row(
            "SELECT id FROM session_inbox WHERE session_id=?1 AND message_id IS NULL AND error IS NULL
            AND (delivery='steer' OR ?2) ORDER BY delivery='steer' DESC,rowid LIMIT 1",
            params![session,queue], |r|r.get(0)).optional()?;
        id.map(|id| self.inbox_read(session, &id))
            .transpose()
            .map(Option::flatten)
    }

    pub(super) fn consume_inbox_turn_in(
        conn: &Connection,
        session: &str,
        item: &InboxItem,
    ) -> Result<(), StorageError> {
        // Selecting a steer is not a claim: a concurrent move to Queue must
        // still win until this acceptance transaction consumes the input.
        if conn.execute("DELETE FROM session_inbox WHERE session_id=?1 AND id=?2 AND message_id IS NULL AND error IS NULL AND (delivery='steer' OR ?3='queue')",params![session,item.id,item.delivery.as_str()])? != 1 {
            return Err(StorageError::OperationNotFound);
        }
        Ok(())
    }

    pub(super) fn deliver_inbox_in(
        conn: &Connection,
        session: &str,
        turn: &str,
        log: &mut crate::tools::TurnLog,
        steer: &InboxSteer,
    ) -> Result<InboxDelivered, StorageError> {
        let pending = &steer.pending;
        let eligible: bool = conn.prepare_cached("SELECT EXISTS(SELECT 1 FROM session_inbox
            WHERE session_id=?1 AND id=?2 AND message_id IS NULL AND error IS NULL AND delivery='steer')")?.query_row(params![session,pending.item.id],|r|r.get(0))?;
        if !eligible {
            return Err(StorageError::CompressionConflict);
        }
        let original = pending.payload.original();
        let command = pending
            .payload
            .command
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|_| invalid())?;
        let message = Self::insert_message(conn, session, "user", &original.text)?;
        let images = serde_json::to_string(&oc_core::prompt::UserImages {
            input_index: steer.input_index,
            images: original.images.iter().map(|i| i.mark.clone()).collect(),
        })
        .map_err(|_| invalid())?;
        conn.execute("UPDATE session_inbox SET message_id=?2,turn_id=?3,user_images=?4,command=?5,payload=NULL WHERE id=?1",
            params![pending.item.id,message,turn,images,command])?;
        Self::prompt_history_in(conn, Some(&original.text))?;
        log.shell_notice_messages.push(message.clone());
        Ok(InboxDelivered {
            item: pending.item.clone(),
            message: oc_core::session::MessageId(message),
        })
    }

    pub(super) fn inbox_message_in(
        conn: &Connection,
        session: &str,
        message: &str,
    ) -> Result<bool, StorageError> {
        Ok(conn
            .prepare_cached(
                "SELECT EXISTS(SELECT 1 FROM session_inbox WHERE session_id=?1 AND message_id=?2)",
            )?
            .query_row(params![session, message], |r| r.get(0))?)
    }

    pub(crate) fn admit_inbox_command(
        &self,
        op: &str,
        session: &str,
        input: &str,
        log: &mut crate::tools::TurnLog,
        steer: &InboxSteer,
    ) -> Result<Option<InboxDelivered>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let mut next = log.clone();
        let turn = next.turn_id.clone();
        let delivered = match Self::deliver_inbox_in(&tx, session, &turn, &mut next, steer) {
            Ok(delivered) => delivered,
            Err(StorageError::CompressionConflict) => return Ok(None),
            Err(error) => return Err(error),
        };
        let journal = next.to_json().to_string();
        let identity = Self::call_identity(op, &turn, &journal)?;
        tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,provider_call_id,call_occurrence,original_input_index)
            VALUES(?1,?2,?3,'subagent','started',?4,?5,?6,?7)",params![op,session,turn,input,
            identity.as_ref().map(|v|&v.0),identity.as_ref().map(|v|v.1),identity.as_ref().map(|v|v.2)])?;
        if tx.execute(
            "UPDATE turns SET result=?2 WHERE id=?1 AND session_id=?3 AND status='started'",
            params![turn, journal, session],
        )? != 1
        {
            return Err(StorageError::OperationNotFound);
        }
        tx.commit()?;
        *log = next;
        Ok(Some(delivered))
    }

    pub fn inbox_has_steer(&self, session: &str) -> Result<bool, StorageError> {
        Ok(self.conn.lock().expect("db mutex").query_row(
            "SELECT EXISTS(SELECT 1 FROM session_inbox
            WHERE session_id=?1 AND message_id IS NULL AND error IS NULL AND delivery='steer')",
            [session],
            |r| r.get(0),
        )?)
    }
}

#[cfg(test)]
#[path = "storage_inbox_tests.rs"]
mod tests;
