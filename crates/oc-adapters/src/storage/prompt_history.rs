//! Shared input history uses the existing Db preference transaction, never RAW.
use super::{BoundedPref, Db, StorageError};
use oc_core::{queries::append_prompt_history, session::MAX_INPUT_BYTES};
use rusqlite::Connection;
use serde::de::{Deserialize, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};

const KEY: &str = "tui.prompt_history.v1";
// JSON can encode one admitted byte as six ASCII bytes. This bound derives
// only from the frozen newest-50 and existing input limits, not a new text cap.
const MAX_BYTES: usize =
    oc_core::queries::MAX_PROMPT_HISTORY_ENTRIES * (MAX_INPUT_BYTES * 6 + 3) + 2;

impl Db {
    pub(crate) fn prompt_history(&self, append: Option<&str>) -> Result<Vec<String>, StorageError> {
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        let entries = Self::prompt_history_in(&tx, append)?;
        tx.commit()?;
        Ok(entries)
    }

    pub(super) fn prompt_history_in(
        conn: &Connection,
        append: Option<&str>,
    ) -> Result<Vec<String>, StorageError> {
        let raw = match Self::get_pref_bounded_in(conn, KEY, MAX_BYTES)? {
            BoundedPref::Value(raw) => Some(raw),
            BoundedPref::Missing | BoundedPref::TooLarge => None,
        };
        let mut entries = raw
            .as_deref()
            .and_then(|text| {
                let mut decoder = serde_json::Deserializer::from_str(text);
                let entries = decoder.deserialize_seq(HistoryVisitor).ok()?;
                decoder.end().ok()?;
                Some(entries)
            })
            .unwrap_or_default();
        if let Some(text) = append {
            append_prompt_history(&mut entries, text).map_err(|_| {
                StorageError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "prompt history input too large",
                ))
            })?;
        }
        let encoded = serde_json::to_string(&entries).map_err(std::io::Error::other)?;
        if raw.as_deref() != Some(encoded.as_str()) {
            Self::upsert_pref(conn, KEY, &encoded)?;
        }
        Ok(entries)
    }
}

struct HistoryVisitor;
impl<'de> Visitor<'de> for HistoryVisitor {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("prompt input history")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        while let Some(item) = seq.next_element::<HistoryText>()? {
            if let Some(text) = item.0 {
                // Invalid/legacy entries cannot reserve a slot or break valid
                // consecutive dedup. Unknown containers are skipped, not kept.
                let _ = append_prompt_history(&mut entries, &text);
            }
        }
        Ok(entries)
    }
}

struct HistoryText(Option<String>);
impl<'de> Deserialize<'de> for HistoryText {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(TextVisitor)
    }
}
struct TextVisitor;
impl<'de> Visitor<'de> for TextVisitor {
    type Value = HistoryText;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a bounded prompt or legacy entry")
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(HistoryText(
            (!value.trim().is_empty() && value.len() <= MAX_INPUT_BYTES).then(|| value.into()),
        ))
    }
    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(HistoryText(
            (!value.trim().is_empty() && value.len() <= MAX_INPUT_BYTES).then_some(value),
        ))
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(HistoryText(None))
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(HistoryText(None))
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(HistoryText(None))
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(HistoryText(None))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(HistoryText(None))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(HistoryText(None))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(HistoryText(None))
    }
}

#[cfg(test)]
mod tests;
