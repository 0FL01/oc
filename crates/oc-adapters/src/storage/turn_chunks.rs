//! The journal of a live turn as content-defined chunks.
//!
//! A live turn's journal (`TurnLog` JSON) is checkpointed after every tool
//! intent and outcome; rewriting `turns.result` whole each time made a turn
//! with N tool calls write O(N²) bytes. While a turn is `started`, its exact
//! journal bytes live in `turn_chunks` instead, cut at content-defined
//! boundaries: an edit anywhere in the document — an appended input item, a
//! display part inserted before later ones, a live span's status — rewrites
//! only the chunks around it. `turns.result` then holds a stub: the same
//! object with every growing value emptied, which keeps the small fields and
//! the expression indexes over them current. `turn_logs` reassembles the
//! journal byte for byte; leaving `started` writes it back into
//! `turns.result` once (trigger `turn_chunks_settle`), so settled turns are
//! read exactly as before.
//!
//! A settled journal can be megabytes, and reading any field of it reads all
//! of it. `turn_heads` holds the settled journal without its heavy values
//! (`HEAVY`), and with only the message items of `input` (items with a role
//! or content; others are `null`, so indexes keep): small fields, spans, display parts and message
//! text, a small part of the bytes. History presentation reads the head
//! (`turn_log_heads`, which for a live turn is its whole journal); readers of
//! the heavy values read `turn_logs.result`. Immutable RAW segments get the
//! same head of their journal in `turn_raw_heads`.
use super::StorageError;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Top-level journal values that grow with the turn's calls. Readers of
/// their content (or of the whole journal) use `turn_logs`.
const GROWING: [&str; 13] = [
    "requests",
    "spans",
    "display_parts",
    "opaque",
    "input",
    "instruction_references",
    "shell_notice_messages",
    "input_origins",
    "call_occurrences",
    "native_mcp_results",
    "native_read_results",
    "native_question_results",
    "working",
];

/// A chunk's key is its turn's rowid (stable: this store never runs
/// `VACUUM`) times `TURN_KEYS` plus its position, so a turn's chunks are one
/// range of the table's own b-tree and need no second index.
const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS turn_chunks(key INTEGER PRIMARY KEY, hash BLOB NOT NULL, bytes TEXT NOT NULL);
    DROP VIEW IF EXISTS turn_logs;
    CREATE VIEW turn_logs AS
        SELECT t.rowid AS turn_rowid, t.id, t.session_id, t.status, t.prompt,
            CASE WHEN EXISTS(SELECT 1 FROM turn_chunks c WHERE c.key BETWEEN t.rowid*68719476736 AND t.rowid*68719476736+68719476735)
                THEN (SELECT group_concat(c.bytes, '' ORDER BY c.key) FROM turn_chunks c WHERE c.key BETWEEN t.rowid*68719476736 AND t.rowid*68719476736+68719476735)
                ELSE t.result END AS result
        FROM turns t;
    DROP VIEW IF EXISTS turn_log_heads;
    CREATE VIEW turn_log_heads AS
        SELECT t.id, t.session_id,
            CASE WHEN EXISTS(SELECT 1 FROM turn_chunks c WHERE c.key BETWEEN t.rowid*68719476736 AND t.rowid*68719476736+68719476735)
                THEN (SELECT group_concat(c.bytes, '' ORDER BY c.key) FROM turn_chunks c WHERE c.key BETWEEN t.rowid*68719476736 AND t.rowid*68719476736+68719476735)
                ELSE COALESCE((SELECT th.head FROM turn_heads th WHERE th.turn_id=t.id), t.result) END AS head
        FROM turns t;
    CREATE TRIGGER IF NOT EXISTS turn_chunks_settle AFTER UPDATE OF status ON turns
        WHEN OLD.status='started' AND NEW.status<>'started'
            AND EXISTS(SELECT 1 FROM turn_chunks WHERE key BETWEEN NEW.rowid*68719476736 AND NEW.rowid*68719476736+68719476735)
    BEGIN
        UPDATE turns SET result=(SELECT group_concat(bytes, '' ORDER BY key) FROM turn_chunks WHERE key BETWEEN NEW.rowid*68719476736 AND NEW.rowid*68719476736+68719476735)
            WHERE id=NEW.id AND NEW.result IS OLD.result;
        DELETE FROM turn_chunks WHERE key BETWEEN NEW.rowid*68719476736 AND NEW.rowid*68719476736+68719476735;
    END;
    CREATE TRIGGER IF NOT EXISTS turn_chunks_delete AFTER DELETE ON turns BEGIN
        DELETE FROM turn_chunks WHERE key BETWEEN OLD.rowid*68719476736 AND OLD.rowid*68719476736+68719476735;
    END;
    CREATE TABLE IF NOT EXISTS turn_heads(
        turn_id TEXT PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE, head TEXT);
    CREATE TRIGGER IF NOT EXISTS turns_head_update AFTER UPDATE OF status, result ON turns
        WHEN NEW.status<>'started' OR OLD.status<>'started'
    BEGIN
        DELETE FROM turn_heads WHERE turn_id=NEW.id;
        INSERT INTO turn_heads(turn_id, head) SELECT id, HEAD FROM turns WHERE id=NEW.id AND status<>'started';
    END;
    CREATE TRIGGER IF NOT EXISTS turns_head_insert AFTER INSERT ON turns WHEN NEW.status<>'started' BEGIN
        INSERT OR REPLACE INTO turn_heads(turn_id, head) SELECT id, HEAD FROM turns WHERE id=NEW.id;
    END;";

/// Top-level values a settled journal's head leaves empty: everything that
/// grows with the turn except its spans, display parts and messages.
const HEAVY: [&str; 10] = [
    "requests",
    "opaque",
    "instruction_references",
    "shell_notice_messages",
    "input_origins",
    "call_occurrences",
    "native_mcp_results",
    "native_read_results",
    "native_question_results",
    "working",
];

/// SQL computing the head of the journal `value` (at `prefix` inside it).
/// Triggers read the row as it is, so their firing order does not matter.
fn head_sql(value: &str, prefix: &str) -> String {
    let mut pairs: Vec<String> = HEAVY
        .iter()
        .map(|key| {
            let empty = if *key == "working" {
                "json_object()"
            } else {
                "json_array()"
            };
            format!("'$.{prefix}{key}',{empty}")
        })
        .collect();
    pairs.push(format!(
        "'$.{prefix}input',json((SELECT json_group_array(CASE WHEN json_type(i.value,'$.role') IS NOT NULL OR json_type(i.value,'$.content') IS NOT NULL THEN json(i.value) END) FROM json_each({value},'$.{prefix}input') i))"
    ));
    format!(
        "(CASE WHEN json_valid({value}) THEN json_replace({value},{}) END)",
        pairs.join(",")
    )
}

/// The journal store's schema step: chunks, heads (backfilled once for
/// settled turns), the reassembling view and the triggers.
pub(super) fn schema(conn: &Connection) -> Result<(), StorageError> {
    let exists = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='turn_heads'")?
        .exists([])?;
    conn.execute_batch(&SCHEMA.replace("HEAD", &head_sql("result", "")))?;
    if !exists {
        conn.execute(
            &format!(
                "INSERT OR IGNORE INTO turn_heads(turn_id, head) SELECT id, {} FROM turns WHERE status<>'started'",
                head_sql("result", "")
            ),
            [],
        )?;
    }
    Ok(())
}

/// Heads of the immutable RAW segments, after their table exists.
pub(super) fn raw_schema(conn: &Connection) -> Result<(), StorageError> {
    let exists = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='turn_raw_heads'")?
        .exists([])?;
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS turn_raw_heads(
            turn_id TEXT NOT NULL, ordinal INTEGER NOT NULL, head TEXT,
            PRIMARY KEY(turn_id, ordinal),
            FOREIGN KEY(turn_id, ordinal) REFERENCES turn_raw_segments(turn_id, ordinal) ON DELETE CASCADE);
        CREATE TRIGGER IF NOT EXISTS turn_raw_head_insert AFTER INSERT ON turn_raw_segments BEGIN
            INSERT OR REPLACE INTO turn_raw_heads(turn_id, ordinal, head) VALUES(NEW.turn_id, NEW.ordinal, {});
        END;",
        head_sql("NEW.payload", "journal.")
    ))?;
    if !exists {
        conn.execute(
            &format!(
                "INSERT OR IGNORE INTO turn_raw_heads(turn_id, ordinal, head) SELECT turn_id, ordinal, {} FROM turn_raw_segments",
                head_sql("payload", "journal.")
            ),
            [],
        )?;
    }
    Ok(())
}

/// Positions per turn in a chunk key (2^36, the literal in `SCHEMA`).
const TURN_KEYS: i64 = 1 << 36;

/// Chunk sizes in bytes. A boundary falls where the top 12 bits of the
/// rolling hash (over the last 64 bytes) are zero, on average every 4 KiB
/// after the minimum. A chunk reaching the maximum is cut at its last weaker
/// boundary (top 8 bits zero): also content-defined, so even a repetitive
/// region is cut at the same content after an edit before it.
const MIN_CHUNK: usize = 1024;
const MAX_CHUNK: usize = 16384;
const BOUNDARY_SHIFT: u32 = 64 - 12;
const WEAK_SHIFT: u32 = 64 - 8;

/// Position spacing: rewritten whole journals space chunks widely, chunks
/// inserted into a gap are packed at its start, so repeated insertion at one
/// place (an array growing in the middle of the document) keeps finding room.
const SPACING: i64 = 1 << 16;
const PACKING: i64 = 1 << 4;

const GEAR: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    let mut i = 0;
    while i < 256 {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        table[i] = z ^ (z >> 31);
        i += 1;
    }
    table
};

/// Content-defined chunks of `text`, each a whole number of UTF-8 characters.
fn chunks(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut at, mut hash, mut weak) = (0, 0, 0u64, None);
    while at < bytes.len() {
        hash = (hash << 1).wrapping_add(GEAR[usize::from(bytes[at])]);
        at += 1;
        if at - start < MIN_CHUNK || !text.is_char_boundary(at) {
            continue;
        }
        let cut = if hash >> BOUNDARY_SHIFT == 0 {
            Some(at)
        } else {
            if hash >> WEAK_SHIFT == 0 {
                weak = Some(at);
            }
            (at - start >= MAX_CHUNK).then(|| weak.unwrap_or(at))
        };
        if let Some(cut) = cut {
            out.push(&text[start..cut]);
            (start, at, hash, weak) = (cut, cut, 0, None);
        }
    }
    if start < bytes.len() {
        out.push(&text[start..]);
    }
    out
}

fn digest(chunk: &str) -> Vec<u8> {
    Sha256::digest(chunk.as_bytes())[..16].to_vec()
}

/// The journal with every growing top-level value emptied, or `None` when it
/// is not a JSON object (such a value is stored whole).
fn stub(log: &str) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_str(log).ok()?;
    let object = value.as_object_mut()?;
    for key in GROWING {
        match object.get_mut(key) {
            Some(serde_json::Value::Array(items)) => items.clear(),
            Some(serde_json::Value::Object(fields)) => fields.clear(),
            _ => {}
        }
    }
    serde_json::to_string(&value).ok()
}

/// Store the journal of live turn `turn` (the caller checked it is
/// `started`): chunks that did not change are kept, `turns.result` gets the
/// stub. A journal that is not an object is stored whole.
pub(super) fn store_live(conn: &Connection, turn: &str, log: &str) -> Result<(), StorageError> {
    let Some(stub) = stub(log) else {
        clear(conn, turn)?;
        conn.execute("UPDATE turns SET result=?2 WHERE id=?1", params![turn, log])?;
        return Ok(());
    };
    let base = base(conn, turn)?;
    let old: Vec<(i64, Vec<u8>)> = conn
        .prepare_cached(
            "SELECT key-?1,hash FROM turn_chunks WHERE key BETWEEN ?1 AND ?1+?2-1 ORDER BY key",
        )?
        .query_map(params![base, TURN_KEYS], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?
        .collect::<Result<_, _>>()?;
    let new: Vec<(&str, Vec<u8>)> = chunks(log)
        .into_iter()
        .map(|chunk| (chunk, digest(chunk)))
        .collect();
    // Keep old chunks that reappear in order; an edit resynchronizes the
    // content-defined boundaries within a chunk or two.
    let mut at: HashMap<&[u8], Vec<usize>> = HashMap::new();
    for (index, (_, hash)) in old.iter().enumerate() {
        at.entry(hash.as_slice()).or_default().push(index);
    }
    let mut next = 0;
    let kept: Vec<Option<usize>> = new
        .iter()
        .map(|(_, hash)| {
            let found = at
                .get(hash.as_slice())
                .and_then(|indexes| indexes.iter().copied().find(|index| *index >= next));
            if let Some(index) = found {
                next = index + 1;
            }
            found
        })
        .collect();
    // Positions: kept chunks keep theirs; each run of new chunks is packed
    // at the start of the gap before its next kept neighbour.
    let mut positions = vec![0i64; new.len()];
    let mut start = 0;
    while start < new.len() {
        if let Some(index) = kept[start] {
            positions[start] = old[index].0;
            start += 1;
            continue;
        }
        let end = (start..new.len())
            .find(|k| kept[*k].is_some())
            .unwrap_or(new.len());
        let low = if start == 0 { 0 } else { positions[start - 1] };
        let run = (end - start) as i64;
        let step = match kept.get(end).copied().flatten() {
            None => SPACING,
            Some(index) => PACKING.min((old[index].0 - low) / (run + 1)),
        };
        if step == 0 || low + step * (run + 1) >= TURN_KEYS {
            return renumber(conn, base, &new, &stub, turn);
        }
        for (offset, slot) in positions[start..end].iter_mut().enumerate() {
            *slot = low + step * (offset as i64 + 1);
        }
        start = end;
    }
    let mut retained = vec![false; old.len()];
    for index in kept.iter().flatten() {
        retained[*index] = true;
    }
    let mut delete = conn.prepare_cached("DELETE FROM turn_chunks WHERE key=?1")?;
    for ((pos, _), keep) in old.iter().zip(&retained) {
        if !keep {
            delete.execute([base + pos])?;
        }
    }
    let mut insert =
        conn.prepare_cached("INSERT INTO turn_chunks(key,hash,bytes) VALUES(?1,?2,?3)")?;
    for (k, (chunk, hash)) in new.iter().enumerate() {
        if kept[k].is_none() {
            insert.execute(params![base + positions[k], hash, chunk])?;
        }
    }
    conn.execute(
        "UPDATE turns SET result=?2 WHERE id=?1",
        params![turn, stub],
    )?;
    Ok(())
}

/// First key of `turn`'s range.
fn base(conn: &Connection, turn: &str) -> Result<i64, StorageError> {
    let rowid: i64 = conn
        .prepare_cached("SELECT rowid FROM turns WHERE id=?1")?
        .query_row([turn], |row| row.get(0))?;
    Ok(rowid * TURN_KEYS)
}

/// Drop `turn`'s chunks (its journal is then stored whole).
pub(super) fn clear(conn: &Connection, turn: &str) -> Result<(), StorageError> {
    conn.prepare_cached(
        "DELETE FROM turn_chunks WHERE key BETWEEN (SELECT rowid FROM turns WHERE id=?1)*?2 AND (SELECT rowid FROM turns WHERE id=?1)*?2+?2-1",
    )?
    .execute(params![turn, TURN_KEYS])?;
    Ok(())
}

/// A gap ran out of positions (repeated insertion at one place deep inside
/// packed chunks): rewrite the turn's chunks with fresh spacing.
fn renumber(
    conn: &Connection,
    base: i64,
    new: &[(&str, Vec<u8>)],
    stub: &str,
    turn: &str,
) -> Result<(), StorageError> {
    clear(conn, turn)?;
    let mut insert =
        conn.prepare_cached("INSERT INTO turn_chunks(key,hash,bytes) VALUES(?1,?2,?3)")?;
    let step = SPACING.min((TURN_KEYS - 1) / (new.len() as i64 + 1));
    for (k, (chunk, hash)) in new.iter().enumerate() {
        insert.execute(params![base + step * (k as i64 + 1), hash, chunk])?;
    }
    conn.execute(
        "UPDATE turns SET result=?2 WHERE id=?1",
        params![turn, stub],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Varied text, like a journal of real tool output and reasoning state.
    fn words(seed: u64, count: usize) -> String {
        let mut state = seed;
        (0..count)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                format!("w{}é ", state >> 50)
            })
            .collect()
    }

    #[test]
    fn chunks_cover_the_text_at_character_boundaries() {
        let text = words(1, 20_000) + &"x".repeat(40_000);
        let parts = chunks(&text);
        assert_eq!(parts.concat(), text);
        assert!(parts.len() > 20, "{} chunks", parts.len());
        assert!(parts.iter().all(|part| part.len() <= MAX_CHUNK + 3));
    }

    #[test]
    fn an_insertion_changes_only_nearby_chunks() {
        let before: Vec<String> = (0..400).map(|i| words(i, 60)).collect();
        let mut after = before.clone();
        after.insert(200, words(9_999, 60));
        let (before, after) = (before.join(","), after.join(","));
        let old: std::collections::HashSet<&str> = chunks(&before).into_iter().collect();
        let changed = chunks(&after)
            .into_iter()
            .filter(|chunk| !old.contains(chunk))
            .count();
        let total = chunks(&after).len();
        assert!(
            total > 30 && changed <= 3,
            "{changed} of {total} chunks changed"
        );
    }

    #[test]
    fn the_stub_empties_growing_values_in_place() {
        let log = r#"{"protocol":"p","spans":[{"a":1}],"working":{"x":[1]},"user_message":"m1","input":[1,2],"model":"m"}"#;
        assert_eq!(
            stub(log).unwrap(),
            r#"{"protocol":"p","spans":[],"working":{},"user_message":"m1","input":[],"model":"m"}"#
        );
        assert_eq!(stub("not json"), None);
        assert_eq!(stub("[1]"), None);
    }
}
