//! Metadata-only history projection and explicit original-prompt restoration.
use super::*;
use oc_core::prompt::{ImageMark, PromptInput};

fn invalid() -> StorageError {
    StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "original image input unavailable or invalid",
    ))
}

impl Db {
    pub(crate) fn user_images(
        &self,
        session: &str,
        message: &str,
        text: &str,
    ) -> Result<Vec<ImageMark>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        Ok(Self::user_image_metadata_in(&conn, session, message, text)?
            .map(|(_, metadata)| metadata.images)
            .unwrap_or_default())
    }

    fn user_image_metadata_in(
        conn: &Connection,
        session: &str,
        message: &str,
        text: &str,
    ) -> Result<Option<(String, oc_core::prompt::UserImages)>, StorageError> {
        // Do not transfer/decode an image-bearing journal for a visible row.
        let row: Option<(String, String)> = conn.prepare_cached("SELECT t.id,CASE WHEN length(CAST(json_extract(coalesce((SELECT th.head FROM turn_heads th WHERE th.turn_id=t.id),t.result),'$.display.user_images') AS BLOB))<=?3 THEN json_object('user_images',json_extract(coalesce((SELECT th.head FROM turn_heads th WHERE th.turn_id=t.id),t.result),'$.display.user_images')) ELSE '{}' END FROM turns t JOIN turn_acceptances a ON a.turn_id=t.id WHERE a.session_id=?1 AND a.user_message=?2 AND json_valid(coalesce((SELECT th.head FROM turn_heads th WHERE th.turn_id=t.id),t.result)) AND json_type(coalesce((SELECT th.head FROM turn_heads th WHERE th.turn_id=t.id),t.result),'$.display.user_images') IS NOT NULL")?.query_row(params![session, message, oc_core::session::MAX_INPUT_BYTES as i64], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
        let row = match row {
            Some(row) => Some(row),
            None => conn.prepare_cached("SELECT turn_id,CASE WHEN length(CAST(user_images AS BLOB))<=?3 AND json_valid(user_images) THEN json_object('user_images',json(user_images)) ELSE '{}' END FROM session_inbox WHERE session_id=?1 AND message_id=?2 AND images>0 AND turn_id IS NOT NULL")?.query_row(params![session,message,oc_core::session::MAX_INPUT_BYTES as i64],|r|Ok((r.get(0)?,r.get(1)?))).optional()?,
        };
        row.map(|(turn, raw)| {
            let value = serde_json::from_str(&raw).map_err(|_| invalid())?;
            let metadata = crate::attachments::image_metadata(text, &value)
                .map_err(|_| invalid())?
                .ok_or_else(invalid)?;
            Ok((turn, metadata))
        })
        .transpose()
    }

    pub(super) fn user_prompt_in(
        conn: &Connection,
        session: &str,
        message: &str,
        text: String,
    ) -> Result<PromptInput, StorageError> {
        let Some((turn, metadata)) = Self::user_image_metadata_in(conn, session, message, &text)?
        else {
            return Ok(text.into());
        };
        let ordinal: Option<i64> = conn.prepare_cached("SELECT ordinal FROM turn_raw_segments WHERE turn_id=?1 AND input_end>?2 ORDER BY ordinal LIMIT 1")?.query_row(params![turn, i64::try_from(metadata.input_index).map_err(|_| invalid())?], |r| r.get(0)).optional()?;
        let item = if let Some(ordinal) = ordinal {
            let (base, end) = Self::raw_descriptor_in(conn, &turn, ordinal)?.ok_or_else(invalid)?;
            if metadata.input_index < base.ends[0] || metadata.input_index >= end.ends[0] {
                return Err(invalid());
            }
            let raw = Self::validated_raw_page_in(
                conn,
                &turn,
                ordinal,
                crate::runtime::ACTIVE_CONTEXT_BYTES_CAP,
            )?
            .ok_or_else(invalid)?;
            let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| invalid())?;
            value["journal"]["input"]
                .get(metadata.input_index - base.ends[0])
                .cloned()
                .ok_or_else(invalid)?
        } else {
            // Original coordinates only: a missing retired segment is an error,
            // never permission to reinterpret a HOT summary as original input.
            let prefix: i64 = conn.prepare_cached("SELECT COALESCE(json_extract(coalesce((SELECT th.head FROM turn_heads th WHERE th.turn_id=turns.id),result),'$.raw_prefix.ends[0]'),0) FROM turns WHERE id=?1")?.query_row([&turn], |r| r.get(0))?;
            let index = metadata
                .input_index
                .checked_sub(usize::try_from(prefix).map_err(|_| invalid())?)
                .ok_or_else(invalid)?;
            let raw: Option<String> = conn.prepare_cached("SELECT CASE WHEN length(CAST(json_extract(result,?2) AS BLOB))<=?3 THEN json_extract(result,?2) END FROM turn_logs WHERE id=?1")?.query_row(params![turn, format!("$.input[{index}]"), crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64], |r| r.get(0))?;
            serde_json::from_str(&raw.ok_or_else(invalid)?).map_err(|_| invalid())?
        };
        let item = serde_json::from_value(item).map_err(|_| invalid())?;
        crate::attachments::restore_prompt(text, &metadata, &item).map_err(|_| invalid())
    }
}
