//! Immutable closed turn deltas and atomic installation of their bounded HOT.
use super::*;
use crate::tools::{TurnLog, turn_history::RawPrefix};

fn invalid(message: &str) -> StorageError {
    StorageError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message,
    ))
}

fn call_window(log: &TurnLog) -> Result<Vec<(DcpCallKey, usize)>, StorageError> {
    let selected = log
        .working
        .as_ref()
        .map(TurnLog::from_json)
        .transpose()
        .map_err(|e| invalid(&e))?;
    let mut calls = Vec::new();
    let mut counts = std::collections::BTreeMap::<String, u64>::new();
    for source in selected.iter().chain(std::iter::once(log)) {
        for (index, item) in source.input.iter().enumerate() {
            if let crate::provider::InputItem::ProviderOutput(v) = item
                && v["type"] == "function_call"
                && let Some(id) = v["call_id"].as_str()
            {
                let occurrence = counts.entry(id.into()).or_default();
                calls.push(((id.into(), *occurrence), source.original_input_index(index)));
                *occurrence += 1;
            }
        }
    }
    Ok(calls)
}

impl Db {
    pub(super) fn count_history_read(&self, kind: usize, bytes: usize) {
        use std::sync::atomic::Ordering::Relaxed;
        self.history_reads[kind].fetch_add(1, Relaxed);
        self.history_reads[kind + 1].fetch_add(bytes as u64, Relaxed);
    }

    pub(crate) fn history_read_counters(&self) -> [u64; 6] {
        std::array::from_fn(|i| self.history_reads[i].load(std::sync::atomic::Ordering::Relaxed))
    }
    pub(crate) fn live_shell_call_ids(&self, turn: &str) -> Result<Vec<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let mut stmt=conn.prepare("SELECT o.provider_call_id FROM shell_jobs j JOIN tool_operations o ON o.id=j.operation_id WHERE o.turn_id=?1 AND j.phase!='terminal' AND o.provider_call_id IS NOT NULL LIMIT 9")?;
        let ids = stmt
            .query_map([turn], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > 8 {
            return Err(invalid("active shell resource guard"));
        }
        Ok(ids)
    }
    pub(super) fn copy_turn_raw_prefix(
        conn: &Connection,
        source: &str,
        destination: &str,
        messages: &std::collections::HashMap<String, String>,
        operations: &std::collections::HashMap<String, String>,
        seqs: &std::collections::BTreeMap<i64, i64>,
    ) -> Result<(), StorageError> {
        // The fork owner has already admitted total requested rows and bytes.
        // Transfer one bounded segment at a time; source rows never change.
        let source_raw: String =
            conn.query_row("SELECT result FROM turns WHERE id=?1", [source], |r| {
                r.get(0)
            })?;
        let source_log = TurnLog::from_json(
            &serde_json::from_str(&source_raw).map_err(|_| invalid("invalid source journal"))?,
        )
        .map_err(|e| invalid(&e))?;
        Self::validate_raw_prefix(conn, source, source_log.raw_prefix.as_ref())?;
        let mut stmt=conn.prepare("SELECT ordinal,input_end,span_end,part_end,descriptor FROM turn_raw_segments WHERE turn_id=?1 ORDER BY ordinal")?;
        let mut rows = stmt.query([source])?;
        while let Some(row) = rows.next()? {
            let mut descriptor: RawPrefix = serde_json::from_str(&row.get::<_, String>(4)?)
                .map_err(|_| invalid("invalid fork raw descriptor"))?;
            descriptor.delivered_notice_seq = seqs
                .range(..=descriptor.delivered_notice_seq)
                .next_back()
                .map_or(0, |(_, seq)| *seq);
            let raw = Self::validated_raw_page_in(
                conn,
                source,
                row.get(0)?,
                crate::runtime::ACTIVE_CONTEXT_BYTES_CAP,
            )?
            .ok_or_else(|| invalid("missing fork raw page"))?;
            let mut payload: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| invalid("invalid fork raw segment"))?;
            if payload["journal"]["turn_id"] != source {
                return Err(invalid("foreign raw segment journal"));
            }
            Self::rebase_turn_journal(&mut payload["journal"], destination, messages, operations)?;
            for key in ["base", "end"] {
                let seq = payload[key]["delivered_notice_seq"]
                    .as_i64()
                    .ok_or_else(|| invalid("invalid fork notice cursor"))?;
                payload[key]["delivered_notice_seq"] = seqs
                    .range(..=seq)
                    .next_back()
                    .map_or(0, |(_, seq)| *seq)
                    .into();
            }
            conn.execute(
                "INSERT INTO turn_raw_segments VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    destination,
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    serde_json::to_string(&descriptor).expect("descriptor"),
                    payload.to_string(),
                    payload["base"].to_string()
                ],
            )?;
        }
        let raw: String =
            conn.query_row("SELECT result FROM turns WHERE id=?1", [destination], |r| {
                r.get(0)
            })?;
        let mut journal: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| invalid("invalid fork hot journal"))?;
        Self::rebase_turn_journal(&mut journal, destination, messages, operations)?;
        if let Some(prefix) = journal.get_mut("raw_prefix") {
            let seq = prefix["delivered_notice_seq"]
                .as_i64()
                .ok_or_else(|| invalid("invalid fork hot cursor"))?;
            prefix["delivered_notice_seq"] = seqs
                .range(..=seq)
                .next_back()
                .map_or(0, |(_, seq)| *seq)
                .into();
        }
        conn.execute(
            "UPDATE turns SET result=?1 WHERE id=?2",
            params![journal.to_string(), destination],
        )?;
        Ok(())
    }

    fn rebase_turn_journal(
        journal: &mut serde_json::Value,
        turn: &str,
        messages: &std::collections::HashMap<String, String>,
        operations: &std::collections::HashMap<String, String>,
    ) -> Result<(), StorageError> {
        journal["turn_id"] = turn.into();
        for key in ["user_message", "assistant_message"] {
            if let Some(id) = journal[key].as_str() {
                if let Some(new) = messages.get(id) {
                    journal[key] = new.clone().into();
                } else if !messages.values().any(|new| new == id) {
                    return Err(invalid("raw fork message outside requested prefix"));
                }
            }
        }
        if let Some(notices) = journal["shell_notice_messages"].as_array_mut() {
            for notice in notices {
                let id = notice
                    .as_str()
                    .ok_or_else(|| invalid("invalid raw fork notice"))?;
                if let Some(new) = messages.get(id) {
                    *notice = new.clone().into();
                } else if !messages.values().any(|new| new == id) {
                    return Err(invalid("raw fork notice outside requested prefix"));
                }
            }
        }
        if let Some(parts) = journal["display_parts"].as_array_mut() {
            for part in parts {
                if let Some(op) = part["tool"].as_str() {
                    // HOT was already rebased by the fork owner; RAW was not.
                    if let Some(new) = operations.get(op) {
                        part["tool"] = new.clone().into();
                    } else if !operations.values().any(|id| id == op) {
                        return Err(invalid("raw fork operation outside requested prefix"));
                    }
                }
            }
        }
        if let Some(working) = journal.get_mut("working") {
            Self::rebase_turn_journal(working, turn, messages, operations)?;
        }
        TurnLog::from_json(journal).map_err(|e| invalid(&e))?;
        Ok(())
    }
    pub(super) fn apply_turn_history_schema(conn: &Connection) -> Result<(), StorageError> {
        let applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version=8)",
            [],
            |r| r.get(0),
        )?;
        if applied {
            return Ok(());
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch("ALTER TABLE tool_operations ADD COLUMN provider_call_id TEXT;
          ALTER TABLE tool_operations ADD COLUMN call_occurrence INTEGER;
          ALTER TABLE tool_operations ADD COLUMN original_input_index INTEGER;
          CREATE INDEX tool_original_call ON tool_operations(turn_id,provider_call_id,call_occurrence DESC);
          CREATE TABLE IF NOT EXISTS turn_raw_segments(
           turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL,
          input_end INTEGER NOT NULL, span_end INTEGER NOT NULL, part_end INTEGER NOT NULL,
           descriptor TEXT NOT NULL, payload TEXT NOT NULL, base_descriptor TEXT NOT NULL,
          PRIMARY KEY(turn_id,ordinal));
          CREATE INDEX IF NOT EXISTS turn_raw_input ON turn_raw_segments(turn_id,input_end);
          CREATE INDEX IF NOT EXISTS turn_raw_spans ON turn_raw_segments(turn_id,span_end);
          CREATE INDEX IF NOT EXISTS turn_raw_parts ON turn_raw_segments(turn_id,part_end);
          CREATE TRIGGER IF NOT EXISTS immutable_turn_raw_update BEFORE UPDATE ON turn_raw_segments
          BEGIN SELECT RAISE(ABORT,'immutable raw segment'); END;
           CREATE TRIGGER IF NOT EXISTS immutable_turn_raw_delete BEFORE DELETE ON turn_raw_segments
           WHEN EXISTS(SELECT 1 FROM turns WHERE id=OLD.turn_id)
          BEGIN SELECT RAISE(ABORT,'immutable raw segment'); END;
          INSERT OR IGNORE INTO schema_migrations(version,applied_at) VALUES(8,'t45-turn-raw');")?;
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn next_call_occurrence(
        &self,
        turn: &str,
        call_id: &str,
    ) -> Result<u64, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let previous: Option<i64> = conn.query_row("SELECT call_occurrence FROM tool_operations WHERE turn_id=?1 AND provider_call_id=?2 ORDER BY call_occurrence DESC LIMIT 1",params![turn,call_id],|r| r.get(0)).optional()?.flatten();
        previous.map_or(Ok(0), |n| {
            u64::try_from(n)
                .ok()
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| invalid("invalid call occurrence"))
        })
    }

    /// Typed display reference comes from the issuing canonical call, not an op
    /// name parser. Store occurrence and intent/refusal in the same transaction.
    pub(super) fn call_identity(
        op: &str,
        turn: &str,
        journal: &str,
    ) -> Result<Option<(String, i64, i64)>, StorageError> {
        let value: serde_json::Value = match serde_json::from_str(journal) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };
        let Some(part) = value["display_parts"]
            .as_array()
            .and_then(|parts| parts.iter().rev().find(|p| p["tool"] == op))
        else {
            return Ok(None);
        };
        let Some(index) = part["call_input_index"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
        else {
            return Ok(None);
        };
        let log = TurnLog::from_json(&value).map_err(|e| invalid(&e))?;
        if log.turn_id != turn {
            return Err(invalid("foreign call intent journal"));
        }
        let Some(crate::provider::InputItem::ProviderOutput(call)) = log.input.get(index) else {
            return Err(invalid("call identity is not canonical"));
        };
        let Some(id) = call["call_id"]
            .as_str()
            .filter(|_| call["type"] == "function_call")
        else {
            return Err(invalid("invalid canonical call identity"));
        };
        let occurrence = log
            .call_occurrences
            .get(&index)
            .ok_or_else(|| invalid("missing canonical call occurrence"))?;
        Ok(Some((
            id.to_owned(),
            i64::try_from(*occurrence).map_err(|_| invalid("call occurrence overflow"))?,
            i64::try_from(log.original_input_index(index))
                .map_err(|_| invalid("call coordinate overflow"))?,
        )))
    }

    /// Exact-checkpoint CAS. Rollback preserves durable and resident old HOT.
    pub(crate) fn commit_closed_turn_segment(
        &self,
        old: &TurnLog,
        segment: &serde_json::Value,
        hot: &TurnLog,
        snapshot: Option<(
            &oc_core::compaction::CompactionSnapshot,
            &std::collections::BTreeMap<String, u64>,
        )>,
    ) -> Result<(), StorageError> {
        let base: RawPrefix = serde_json::from_value(segment["base"].clone())
            .map_err(|_| invalid("invalid raw base"))?;
        let end: RawPrefix = serde_json::from_value(segment["end"].clone())
            .map_err(|_| invalid("invalid raw end"))?;
        if old.raw_prefix.clone().unwrap_or_default() != base
            || hot.raw_prefix.as_ref() != Some(&end)
            || hot.turn_id != old.turn_id
            || segment["journal"]["turn_id"] != old.turn_id
            || end.ordinal
                != base
                    .ordinal
                    .checked_add(1)
                    .ok_or_else(|| invalid("raw ordinal overflow"))?
        {
            return Err(invalid("raw/hot boundary mismatch"));
        }
        let delta = TurnLog::from_json(&segment["journal"]).map_err(|e| invalid(&e))?;
        let (expected_segment, expected_hot) = old
            .prepare_closed_segment(
                delta.closed_counts(),
                hot.working
                    .clone()
                    .ok_or_else(|| invalid("missing selected HOT"))?,
                end.delivered_notice_seq,
            )
            .map_err(|e| invalid(&e))?;
        if expected_segment != *segment || expected_hot != *hot {
            return Err(invalid("raw delta differs from canonical checkpoint"));
        }
        if delta
            .closed_counts()
            .iter()
            .enumerate()
            .any(|(i, n)| base.ends[i].checked_add(*n) != Some(end.ends[i]))
            || delta.spans.iter().any(|s| s.completed.is_none())
        {
            return Err(invalid("raw segment is not closed"));
        }
        let mut conn = self.conn.lock().expect("db mutex");
        let tx = conn.transaction()?;
        Self::validate_raw_prefix(&tx, &old.turn_id, old.raw_prefix.as_ref())?;
        let current: Option<String> = tx.query_row(
            "SELECT result FROM turns WHERE id=?1 AND status='started'",
            [&old.turn_id],
            |r| r.get(0),
        )?;
        if current.as_deref() != Some(old.to_json().to_string().as_str()) {
            return Err(invalid("stale closed checkpoint"));
        }
        for part in &delta.display_parts {
            if let Some(op) = part["tool"].as_str() {
                let state: String = tx.query_row(
                    "SELECT state FROM tool_operations WHERE id=?1 AND turn_id=?2",
                    params![op, old.turn_id],
                    |r| r.get(0),
                )?;
                if !matches!(
                    state.as_str(),
                    "completed" | "failed" | "denied" | "cancelled" | "no_gain"
                ) {
                    return Err(invalid("unsettled closed effect"));
                }
            }
        }
        tx.execute("INSERT INTO turn_raw_segments(turn_id,ordinal,input_end,span_end,part_end,descriptor,payload,base_descriptor) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![old.turn_id,i64::try_from(end.ordinal).map_err(|_| invalid("raw ordinal overflow"))?,i64::try_from(end.ends[0]).map_err(|_| invalid("raw input overflow"))?,i64::try_from(end.ends[2]).map_err(|_| invalid("raw span overflow"))?,i64::try_from(end.ends[4]).map_err(|_| invalid("raw part overflow"))?,serde_json::to_string(&end).expect("descriptor"),segment.to_string(),serde_json::to_string(&base).expect("descriptor")])?;
        if tx.execute(
            "UPDATE turns SET result=?1 WHERE id=?2 AND status='started'",
            params![hot.to_json().to_string(), old.turn_id],
        )? != 1
        {
            return Err(invalid("turn settled during raw commit"));
        }
        if let Some((snapshot, past_removed)) = snapshot {
            tx.execute(
                "UPDATE session_compactions SET snapshot=?2 WHERE id=?1 AND session_id=?3",
                params![
                    snapshot.id,
                    serde_json::to_string(snapshot).map_err(|_| invalid("compaction metadata"))?,
                    snapshot.session
                ],
            )?;
            tx.execute(
                "DELETE FROM session_usage_anchor WHERE session_id=?1",
                [&snapshot.session],
            )?;
            // Remap only marks for the bounded removed current window. Coverage
            // identities stay immutable; window-local marks cannot leak forward.
            let new = call_window(hot)?
                .into_iter()
                .map(|((id, n), origin)| ((id.clone(), origin), (id, n)))
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut retained = Vec::new();
            // All past facts participated in the admitted consolidation above.
            // Retire their marks, then remap the retained current window.
            for (id, count) in past_removed {
                tx.execute("DELETE FROM dcp_tool_projection_v2 WHERE session_id=?1 AND call_id=?2 AND occurrence<?3",params![snapshot.session,id,*count as i64])?;
            }
            for ((id, n), origin) in call_window(old)? {
                let old_n = n + past_removed.get(&id).copied().unwrap_or(0);
                let action: Option<String>=tx.query_row("SELECT action FROM dcp_tool_projection_v2 WHERE session_id=?1 AND call_id=?2 AND occurrence=?3",params![snapshot.session,id,old_n as i64],|r| r.get(0)).optional()?;
                tx.execute("DELETE FROM dcp_tool_projection_v2 WHERE session_id=?1 AND call_id=?2 AND occurrence=?3",params![snapshot.session,id,old_n as i64])?;
                if let Some(action) = action
                    && let Some(key) = new.get(&(id, origin))
                {
                    retained.push((key.clone(), action));
                }
            }
            for ((id, n), action) in retained {
                tx.execute(
                    "INSERT INTO dcp_tool_projection_v2 VALUES(?1,?2,?3,?4)",
                    params![snapshot.session, id, n as i64, action],
                )?;
            }
            let boundary: Option<String> = tx.query_row("SELECT id FROM conversation_messages WHERE session_id=?1 AND seq<(SELECT seq FROM conversation_messages WHERE session_id=?1 AND id=?2) ORDER BY seq DESC LIMIT 1",params![snapshot.session,old.user_message],|r|r.get(0)).optional()?;
            if let Some(boundary) = boundary {
                tx.execute("INSERT INTO prune_marks VALUES(?1,?2,'turn-consolidation') ON CONFLICT(session_id) DO UPDATE SET up_to_msg=excluded.up_to_msg,created_at=excluded.created_at",params![snapshot.session,boundary])?;
            }
            tx.execute(
                "DELETE FROM session_checkpoint WHERE session_id=?1",
                [&snapshot.session],
            )?;
            Self::refresh_checkpoint_dcp_accounting(&tx, &snapshot.session)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(super) fn validate_raw_prefix(
        conn: &Connection,
        turn: &str,
        prefix: Option<&RawPrefix>,
    ) -> Result<(), StorageError> {
        let last: Option<Option<String>> = conn.query_row("SELECT CASE WHEN length(CAST(descriptor AS BLOB))<=16384 THEN descriptor END FROM turn_raw_segments WHERE turn_id=?1 ORDER BY ordinal DESC LIMIT 1", [turn], |r| r.get(0)).optional()?;
        if last == Some(None) {
            return Err(invalid("raw descriptor exceeds existing snapshot budget"));
        }
        let last = last.flatten();
        let descriptor = last
            .as_deref()
            .map(serde_json::from_str::<RawPrefix>)
            .transpose()
            .map_err(|_| invalid("corrupt raw segment descriptor"))?;
        if descriptor.as_ref() != prefix {
            return Err(invalid("raw prefix disagrees with immutable journal"));
        }
        Ok(())
    }

    /// Explicit original-history page. Size checked before payload transfer.
    pub fn raw_turn_segment(
        &self,
        turn: &str,
        ordinal: u64,
        byte_budget: usize,
    ) -> Result<Option<String>, StorageError> {
        let conn = self.conn.lock().expect("db mutex");
        let ordinal = i64::try_from(ordinal).map_err(|_| invalid("raw ordinal overflow"))?;
        let tx = conn.unchecked_transaction()?;
        let result = Self::validated_raw_page_in(&tx, turn, ordinal, byte_budget)?;
        tx.commit()?;
        if let Some(raw) = &result {
            self.count_history_read(2, raw.len());
        }
        Ok(result)
    }

    fn raw_descriptor_in(
        conn: &Connection,
        turn: &str,
        ordinal: i64,
    ) -> Result<Option<(RawPrefix, RawPrefix)>, StorageError> {
        let row: Option<([i64;3],Option<String>,Option<String>)> = conn.query_row(
            "SELECT input_end,span_end,part_end,CASE WHEN length(CAST(descriptor AS BLOB))<=16384 THEN descriptor END,CASE WHEN length(CAST(base_descriptor AS BLOB))<=16384 THEN base_descriptor END FROM turn_raw_segments WHERE turn_id=?1 AND ordinal=?2",
            params![turn,ordinal], |r| Ok(([r.get(0)?,r.get(1)?,r.get(2)?],r.get(3)?,r.get(4)?))).optional()?;
        let Some((indexed, raw, base)) = row else {
            return Ok(None);
        };
        let value = serde_json::from_str::<serde_json::Value>(
            &raw.ok_or_else(|| invalid("raw descriptor exceeds snapshot budget"))?,
        )
        .map_err(|_| invalid("corrupt raw descriptor"))?;
        let descriptor = RawPrefix::parse(Some(&value))
            .map_err(|e| invalid(&e))?
            .ok_or_else(|| invalid("missing raw descriptor"))?;
        if descriptor.ordinal != ordinal as u64
            || indexed
                != [
                    descriptor.ends[0] as i64,
                    descriptor.ends[2] as i64,
                    descriptor.ends[4] as i64,
                ]
        {
            return Err(invalid("raw indexed endpoints disagree with descriptor"));
        }
        let base: RawPrefix =
            serde_json::from_str(&base.ok_or_else(|| invalid("raw base exceeds snapshot budget"))?)
                .map_err(|_| invalid("corrupt indexed raw base"))?;
        if ordinal == 1 {
            if base != RawPrefix::default() {
                return Err(invalid("invalid initial raw base"));
            }
        } else {
            RawPrefix::parse(Some(&serde_json::to_value(&base).expect("descriptor")))
                .map_err(|e| invalid(&e))?;
        }
        if base.ordinal.checked_add(1) != Some(descriptor.ordinal)
            || base.ends.iter().zip(descriptor.ends).any(|(b, e)| *b > e)
            || base.ends[0] >= descriptor.ends[0]
            || base.ends[2] >= descriptor.ends[2]
            || base.delivered_notice_seq > descriptor.delivered_notice_seq
        {
            return Err(invalid("invalid raw descriptor interval"));
        }
        Ok(Some((base, descriptor)))
    }

    /// One admitted payload and at most two indexed adjacent descriptors. No
    /// historical payload scan, and the caller owns a consistent read snapshot.
    fn validated_raw_page_in(
        conn: &Connection,
        turn: &str,
        ordinal: i64,
        budget: usize,
    ) -> Result<Option<String>, StorageError> {
        let budget = budget.min(crate::runtime::ACTIVE_CONTEXT_BYTES_CAP);
        if ordinal <= 0 {
            return Err(invalid("invalid raw page ordinal"));
        }
        let Some((indexed_base, end)) = Self::raw_descriptor_in(conn, turn, ordinal)? else {
            let tail: Option<i64>=conn.query_row("SELECT CASE WHEN json_valid(result) THEN json_extract(result,'$.raw_prefix.ordinal') END FROM turns WHERE id=?1",[turn],|r|r.get(0)).optional()?.flatten();
            if tail.is_some_and(|tail| tail >= ordinal) {
                return Err(invalid("missing raw page inside committed prefix"));
            }
            return Ok(None);
        };
        let raw: Option<String> = conn.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=?3 THEN payload END FROM turn_raw_segments WHERE turn_id=?1 AND ordinal=?2",params![turn,ordinal,i64::try_from(budget).unwrap_or(i64::MAX)],|r|r.get(0))?;
        let raw = raw.ok_or_else(|| invalid("raw history page exceeds byte budget"))?;
        let value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|_| invalid("corrupt raw page JSON"))?;
        let base: RawPrefix = serde_json::from_value(value["base"].clone())
            .map_err(|_| invalid("corrupt raw page base"))?;
        if base != indexed_base
            || value["end"] != serde_json::to_value(&end).expect("descriptor")
            || value["journal"]["turn_id"] != turn
        {
            return Err(invalid("raw page descriptor or turn ownership mismatch"));
        }
        let mut journal = TurnLog::from_json(&value["journal"]).map_err(|e| invalid(&e))?;
        if journal.raw_prefix.is_some()
            || journal.working.is_some()
            || !journal.input_origins.is_empty()
        {
            return Err(invalid("raw page contains selected projection"));
        }
        let previous: Option<i64> = conn.query_row("SELECT ordinal FROM turn_raw_segments WHERE turn_id=?1 AND ordinal<?2 ORDER BY ordinal DESC LIMIT 1",params![turn,ordinal],|r|r.get(0)).optional()?;
        let expected_base = if ordinal == 1 {
            RawPrefix::default()
        } else {
            if previous != Some(ordinal - 1) {
                return Err(invalid("missing adjacent raw predecessor"));
            }
            Self::raw_descriptor_in(conn, turn, ordinal - 1)?
                .ok_or_else(|| invalid("missing raw predecessor"))?
                .1
        };
        if base != expected_base {
            return Err(invalid("raw page base disagrees with predecessor"));
        }
        // Reuse the canonical capture validator for counts, closed call/result
        // groups, span settlement and local media/reference coordinates.
        let selected = TurnLog::new(turn, &journal.model, &journal.provider).to_json();
        if ordinal > 1 {
            journal.raw_prefix = Some(base);
        }
        let (expected, _) = journal
            .prepare_closed_segment(journal.closed_counts(), selected, end.delivered_notice_seq)
            .map_err(|e| invalid(&e))?;
        if expected != value {
            return Err(invalid("raw page schema, counts or coordinates disagree"));
        }
        if journal.requests.iter().any(|r| {
            r.input_start < expected_base.ends[0]
                || r.input_start > end.ends[0]
                || !journal.spans.iter().any(|s| s.id == r.span)
        }) || journal
            .spans
            .iter()
            .filter_map(|s| s.request.as_ref())
            .any(|r| !journal.requests.contains(r))
        {
            return Err(invalid("raw receipt outside original segment"));
        }
        for part in &journal.display_parts {
            if let Some(op) = part["tool"].as_str() {
                let owned: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM tool_operations WHERE id=?1 AND turn_id=?2)",
                    params![op, turn],
                    |r| r.get(0),
                )?;
                if !owned {
                    return Err(invalid("foreign raw tool reference"));
                }
            }
        }
        let next: Option<i64> = conn.query_row("SELECT ordinal FROM turn_raw_segments WHERE turn_id=?1 AND ordinal>?2 ORDER BY ordinal LIMIT 1",params![turn,ordinal],|r|r.get(0)).optional()?;
        if let Some(next) = next {
            if ordinal.checked_add(1) != Some(next) {
                return Err(invalid("missing adjacent raw successor"));
            }
            let (next_base, _) = Self::raw_descriptor_in(conn, turn, next)?
                .ok_or_else(|| invalid("missing raw successor"))?;
            if next_base != end {
                return Err(invalid("raw successor base disagrees with page end"));
            }
        } else {
            let hot: Option<String> = conn.query_row("SELECT CASE WHEN json_valid(result) AND length(CAST(json_extract(result,'$.raw_prefix') AS BLOB))<=16384 THEN json_extract(result,'$.raw_prefix') END FROM turns WHERE id=?1",[turn],|r|r.get(0))?;
            let hot: serde_json::Value = serde_json::from_str(
                &hot.ok_or_else(|| invalid("missing bounded hot descriptor"))?,
            )
            .map_err(|_| invalid("corrupt hot descriptor"))?;
            if RawPrefix::parse(Some(&hot))
                .map_err(|e| invalid(&e))?
                .as_ref()
                != Some(&end)
            {
                return Err(invalid("raw tail disagrees with hot descriptor"));
            }
        }
        Ok(Some(raw))
    }

    pub(super) fn latest_turn_spans(
        &self,
        conn: &Connection,
        turn: &str,
    ) -> Result<Vec<oc_core::queries::AssistantSpan>, StorageError> {
        let total: i64 = conn.query_row("SELECT COALESCE(json_extract(result,'$.raw_prefix.ends[2]'),0)+COALESCE(json_array_length(result,'$.spans'),0) FROM turns WHERE id=?1",[turn],|r| r.get(0))?;
        let floor = (total - 192).max(0);
        let mut stmt=conn.prepare("WITH recent AS (SELECT ordinal,payload FROM turn_raw_segments WHERE turn_id=?1 AND span_end>?2 ORDER BY ordinal DESC LIMIT 192), spans AS (
          SELECT json_extract(r.payload,'$.base.ends[2]')+s.key AS seq,s.value FROM recent r,json_each(r.payload,'$.journal.spans') s WHERE json_extract(r.payload,'$.base.ends[2]')+s.key>=?2
          UNION ALL SELECT COALESCE(json_extract(t.result,'$.raw_prefix.ends[2]'),0)+s.key,s.value FROM turns t,json_each(t.result,'$.spans') s WHERE t.id=?1)
          SELECT CASE WHEN SUM(length(CAST(value AS BLOB))) OVER (ORDER BY seq)<=?3 THEN value END FROM (SELECT seq,value FROM spans ORDER BY seq DESC LIMIT 192) ORDER BY seq")?;
        stmt.query_map(
            params![turn, floor, crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64],
            |r| r.get::<_, Option<String>>(0),
        )?
        .map(|row| {
            let raw =
                row?.ok_or_else(|| invalid("span window exceeds existing context byte budget"))?;
            self.count_history_read(4, raw.len());
            serde_json::from_str(&raw).map_err(|_| invalid("invalid recorded span"))
        })
        .collect()
    }

    pub(super) fn latest_turn_parts(
        &self,
        conn: &Connection,
        turn: &str,
    ) -> Result<Vec<(usize, Option<i64>, serde_json::Value)>, StorageError> {
        let total: i64=conn.query_row("SELECT COALESCE(json_extract(result,'$.raw_prefix.ends[4]'),0)+COALESCE(json_array_length(result,'$.display_parts'),0) FROM turns WHERE id=?1",[turn],|r| r.get(0))?;
        let floor = (total - 240).max(0);
        let mut stmt=conn.prepare("WITH recent AS (SELECT ordinal,payload FROM turn_raw_segments WHERE turn_id=?1 AND part_end>?2 ORDER BY ordinal DESC LIMIT 240), parts AS (
          SELECT json_extract(r.payload,'$.base.ends[4]')+p.key AS seq,r.ordinal,p.value FROM recent r,json_each(r.payload,'$.journal.display_parts') p WHERE json_extract(r.payload,'$.base.ends[4]')+p.key>=?2
          UNION ALL SELECT COALESCE(json_extract(t.result,'$.raw_prefix.ends[4]'),0)+p.key,NULL,p.value FROM turns t,json_each(t.result,'$.display_parts') p WHERE t.id=?1)
          SELECT seq,ordinal,CASE WHEN SUM(length(CAST(value AS BLOB))) OVER (ORDER BY seq)<=?3 THEN value END FROM (SELECT * FROM parts ORDER BY seq DESC LIMIT 240) ORDER BY seq")?;
        stmt.query_map(
            params![turn, floor, crate::runtime::ACTIVE_CONTEXT_BYTES_CAP as i64],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )?
        .map(|row| {
            let (seq, ordinal, raw) = row?;
            let raw =
                raw.ok_or_else(|| invalid("part window exceeds existing context byte budget"))?;
            self.count_history_read(4, raw.len());
            Ok((
                seq as usize,
                ordinal,
                serde_json::from_str(&raw).map_err(|_| invalid("invalid display reference"))?,
            ))
        })
        .collect()
    }
}

#[cfg(test)]
#[path = "storage_turn_history/tests.rs"]
mod tests;
