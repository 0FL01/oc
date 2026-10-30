//! A turn's journal grows with every tool call; its writes must not.
use super::*;
use oc_core::queries::ModelRef;
use serde_json::{Value, json};

/// The journal after `calls` calls, shaped like `TurnLog::to_json`: growing
/// requests, spans and opaque state; each call inserts its display part
/// before the trailing message part and appends its call and output items.
fn journal(turn: &str, calls: usize, settled: bool) -> String {
    let mut parts: Vec<Value> = (0..calls)
        .map(|k| json!({"tool": format!("{turn}-op-{k}")}))
        .collect();
    parts.push(json!({"message": 0}));
    let spans: Vec<Value> = (0..=calls)
        .map(|k| json!({"id": k, "status": if k < calls || settled {"completed"} else {"running"}}))
        .collect();
    let mut input = vec![json!({"role": "user", "content": "question"})];
    for k in 0..calls {
        input.push(json!({"type": "function_call", "call_id": format!("call-{k}"), "arguments": format!("{{\"path\":\"src/f{k}.rs\"}}")}));
        input.push(json!({"type": "function_call_output", "call_id": format!("call-{k}"), "output": format!("file {k}: {}", "fn body() {}\n".repeat(200))}));
    }
    json!({
        "protocol": "responses",
        "binding": {"model": "m"},
        "requests": (0..=calls).map(|k| json!({"request": k})).collect::<Vec<_>>(),
        "instruction_references": [],
        "display": {},
        "spans": spans,
        "display_parts": parts,
        "turn_id": turn,
        "model": "m",
        "provider": "fixture",
        "opaque": (0..calls).map(|k| json!({"reasoning": format!("{k}:{}", "x".repeat(800))})).collect::<Vec<_>>(),
        "usage": {},
        "user_message": Value::Null,
        "input": input,
    })
    .to_string()
}

/// WAL pages written by a turn of `calls` calls, checkpointed after each
/// call's intent and outcome; the live journal reads back exactly.
fn turn_frames(calls: usize) -> u64 {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.conn
        .skip_sync
        .store(true, std::sync::atomic::Ordering::SeqCst);
    db.create_session("s").unwrap();
    let model = ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    db.accept_turn("t", "s", "question", "question", &model)
        .unwrap();
    let before = db.storage_metrics().wal_frames;
    for k in 1..=calls {
        for log in [
            journal("t", k, false),
            journal("t", k, false).replace("\"running\"", "\"waiting\""),
        ] {
            db.checkpoint_turn("t", &log).unwrap();
            let read: String = db
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT result FROM turn_logs WHERE id='t'", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert!(read == log, "the live journal reads back byte for byte");
        }
    }
    let frames = db.storage_metrics().wal_frames - before;
    let last = journal("t", calls, true);
    db.commit_turn("t", "completed", Some(&last), Some("answer"))
        .unwrap();
    let (stored, chunks): (String, i64) = db.conn.lock().unwrap().query_row(
        "SELECT result,(SELECT count(*) FROM turn_chunks WHERE key BETWEEN turns.rowid*68719476736 AND turns.rowid*68719476736+68719476735) FROM turns WHERE id='t'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    // Settlement adds the assistant message to the whole journal.
    let mut settled: Value = serde_json::from_str(&stored).unwrap();
    settled.as_object_mut().unwrap().remove("assistant_message");
    let last: Value = serde_json::from_str(&last).unwrap();
    assert!(
        chunks == 0 && settled == last,
        "settlement stores the whole journal once"
    );
    frames
}

#[test]
fn a_turn_writes_linear_volume_in_its_tool_calls() {
    let (half, full) = (turn_frames(50), turn_frames(100));
    eprintln!("WAL pages: 50 calls {half}, 100 calls {full}");
    // Rewriting the journal whole is quadratic: twice the calls wrote 3.6x
    // the pages (1476 -> 5299). Chunked, each checkpoint's cost only grows
    // with the b-tree depth.
    assert!(
        full * 10 <= half * 25,
        "50 calls wrote {half} pages, 100 calls {full}"
    );
}

/// A status change out of `started` without a new journal (an interrupted
/// turn) writes the chunked journal back whole.
#[test]
fn leaving_started_without_a_journal_keeps_the_chunked_one() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    let model = ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    db.accept_turn("t", "s", "question", "question", &model)
        .unwrap();
    let log = journal("t", 3, false);
    db.checkpoint_turn("t", &log).unwrap();
    let stub: String = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT result FROM turns WHERE id='t'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(
        stub.len() < log.len() / 4 && stub.contains("\"input\":[]"),
        "{stub}"
    );
    db.recover_interrupted_tools().unwrap();
    let (status, stored): (String, String) = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT status,result FROM turns WHERE id='t'", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(status, "unknown");
    let expected: String = db.conn.lock().unwrap().query_row("SELECT json_set(?1,'$.spans[3].status','unknown','$.spans[3].completed',json_extract(?2,'$.spans[3].completed'))", params![log, stored], |row| row.get(0)).unwrap();
    assert!(
        stored == expected,
        "recovery patched the live span of the whole journal"
    );
}
