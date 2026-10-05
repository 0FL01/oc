//! Irreducible selection effort facts share the existing atomic event owner.
use super::{Db, StorageError};
use crate::provider::InputItem;
use rusqlite::params;

pub(crate) struct EffortFact {
    pub event_seq: i64,
    pub before_message: Option<String>,
    pub item: InputItem,
}

impl Db {
    pub(crate) fn effort_facts(
        &self,
        session: &str,
        after_event: i64,
    ) -> Result<Vec<EffortFact>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut query = conn.prepare_cached(
            "SELECT e.seq, json_extract(e.payload, '$.effort_update'),
               (SELECT m.payload FROM events m WHERE m.session_id=e.session_id
                AND m.kind='message' AND m.seq>e.seq ORDER BY m.seq LIMIT 1)
             FROM events e WHERE e.session_id=?1 AND e.kind='session_model_selected'
             AND e.seq>?2 AND json_valid(e.payload)
             AND json_type(e.payload,'$.effort_update')='object' ORDER BY e.seq",
        )?;
        let rows = query.query_map(params![session, after_event], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (event_seq, raw, before_message) = row?;
            let mut value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| StorageError::SessionNotFound)?;
            value["type"] = "effort_update".into();
            value["event_seq"] = event_seq.into();
            let item = serde_json::from_value(value).map_err(|_| StorageError::SessionNotFound)?;
            Ok(EffortFact {
                event_seq,
                before_message,
                item,
            })
        })
        .collect()
    }
}
