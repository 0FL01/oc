use super::Db;
use oc_core::queries::TranscriptPart;
use serde_json::json;

#[test]
fn canonical_encoding_of_admitted_arguments_is_not_a_raw_wire_limit() {
    let wire = format!(
        "{{\"values\":[{}]}}",
        std::iter::repeat_n("1e10", 90_000)
            .collect::<Vec<_>>()
            .join(",")
    );
    assert!(wire.len() < crate::provider::ARGUMENT_BYTE_CAP);
    let canonical = serde_json::from_str::<serde_json::Value>(&wire)
        .unwrap()
        .to_string();
    assert!(canonical.len() > crate::provider::ARGUMENT_BYTE_CAP);
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    db.record_tool_intent("op", "s", Some("t"), "fixture", &canonical)
        .unwrap();
    db.checkpoint_turn("t", r#"{"display_parts":[{"tool":"op"}]}"#)
        .unwrap();
    let view = db.turn_presentation("s", "t").unwrap().unwrap();
    let TranscriptPart::Tool(op) = &view.parts[0] else {
        panic!("canonical input missing")
    };
    assert_eq!(op.input.as_deref(), Some(canonical.as_str()));
    assert!(!view.part_states[0].input_omitted);
}

#[test]
fn completed_transcript_does_not_spend_later_arguments_or_answer_on_earlier_previews() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    db.create_session("session").unwrap();
    db.begin_turn("turn", "session", "inspect").unwrap();
    let mut parts = Vec::new();
    for index in 0..5 {
        parts.push(json!({"reasoning": format!("{index}:{}", "подробности ".repeat(2048))}));
    }
    let inputs = [
        ("read", json!({"path":"src/late.rs"})),
        ("grep", json!({"pattern":"canonical_marker", "path":"src"})),
        ("glob", json!({"pattern":"**/*.rs", "path":"src"})),
    ];
    for (name, input) in &inputs {
        db.record_tool_intent(name, "session", Some("turn"), name, &input.to_string())
            .unwrap();
        parts.push(json!({"tool":name}));
    }
    parts.push(json!({"message":0}));
    let journal = json!({
        "display_parts":parts,
        "input":[{"role":"assistant", "content":[{"type":"output_text", "text":"Канонический итог — сохранён."}]}]
    }).to_string();
    db.checkpoint_turn("turn", &journal).unwrap();
    let projection = db.turn_presentation("session", "turn").unwrap().unwrap();
    for (index, part) in projection.parts[..5].iter().enumerate() {
        assert_eq!(
            part,
            &TranscriptPart::Reasoning {
                text: format!("{index}:{}", "подробности ".repeat(2048)),
                duration_ms: None
            }
        );
    }
    for (index, (name, input)) in inputs.iter().enumerate() {
        let TranscriptPart::Tool(tool) = &projection.parts[index + 5] else {
            panic!("missing canonical tool {name}");
        };
        assert_eq!(tool.input.as_deref(), Some(input.to_string().as_str()));
        assert!(!projection.part_states[index + 5].input_omitted);
    }
    assert_eq!(
        projection.parts.last(),
        Some(&TranscriptPart::Text(
            "Канонический итог — сохранён.".into()
        ))
    );
    assert_eq!(
        db.turn_result("turn").unwrap().1.as_deref(),
        Some(journal.as_str())
    );
    drop(db);
    let db = Db::open(&root.path().join("data")).unwrap();
    assert_eq!(
        db.turn_presentation("session", "turn").unwrap().unwrap(),
        projection
    );
}

#[test]
fn transcript_byte_windows_preserve_whole_utf8_inputs_and_bidirectional_progress() {
    use oc_core::queries::HistoryPartCursor;
    use oc_core::session::MessageId;
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    let input = json!({"pattern":"λ\\\n".repeat(16*1024),"path":"."}).to_string();
    let mut parts = Vec::new();
    {
        let mut conn = db.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        for sequence in 0..130 {
            let op = format!("op-{sequence}");
            tx.execute("INSERT INTO tool_operations(id,session_id,turn_id,name,state,input,output) VALUES(?1,'s','t','fixture','completed',?2,'known')",
                rusqlite::params![op,input]).unwrap();
            parts.push(json!({"tool":op}));
        }
        tx.commit().unwrap();
    }
    let raw = json!({"display_parts":parts}).to_string();
    db.checkpoint_turn("t", &raw).unwrap();
    let tail = db.turn_presentation("s", "t").unwrap().unwrap();
    assert!(tail.window.as_ref().unwrap().start > 0);
    assert_eq!(tail.window.as_ref().unwrap().end, 130);
    let mut seen = std::collections::BTreeSet::new();
    let mut page = tail.clone();
    loop {
        assert!(!page.truncated);
        assert_eq!(page.omitted_parts, 0);
        assert!(
            page.parts
                .iter()
                .map(|part| serde_json::to_vec(part).unwrap().len())
                .sum::<usize>()
                <= super::TRANSCRIPT_PAGE_BYTES
        );
        for (state, part) in page.part_states.iter().zip(&page.parts) {
            let TranscriptPart::Tool(tool) = part else {
                panic!("tool")
            };
            assert_eq!(tool.input.as_deref(), Some(input.as_str()));
            assert!(seen.insert(state.sequence), "no overlap or non-progress");
        }
        let start = page.window.as_ref().unwrap().start;
        if start == 0 {
            break;
        }
        page = db
            .turn_presentation_window(
                "s",
                "t",
                Some(&HistoryPartCursor {
                    message: MessageId("canonical".into()),
                    sequence: start,
                    newer: false,
                }),
            )
            .unwrap()
            .unwrap();
        assert!(page.window.as_ref().unwrap().start < start);
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        (0..130).collect::<Vec<_>>()
    );
    let next = db
        .turn_presentation_window(
            "s",
            "t",
            Some(&HistoryPartCursor {
                message: MessageId("canonical".into()),
                sequence: page.window.unwrap().end - 1,
                newer: true,
            }),
        )
        .unwrap()
        .unwrap();
    assert!(next.window.as_ref().unwrap().start > 0);
    assert_eq!(
        db.turn_result("t").unwrap().1.as_deref(),
        Some(raw.as_str())
    );
    drop(db);
    let db = Db::open(root.path()).unwrap();
    assert_eq!(db.turn_presentation("s", "t").unwrap().unwrap(), tail);
}

#[test]
fn one_large_admitted_text_is_served_whole_without_an_aggregate_copy() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    let text = "λ".repeat(5 * 1024 * 1024);
    assert!(text.len() > super::TRANSCRIPT_PAGE_BYTES);
    let raw=json!({"display_parts":[{"message":0}],"input":[{"content":[{"type":"output_text","text":text}]}]}).to_string();
    db.commit_turn("t", "completed", Some(&raw), Some(&text))
        .unwrap();
    let rows = db
        .read_transcript_rows("s", &Default::default(), 100)
        .unwrap();
    let answer = rows.iter().find(|row| row.role == "assistant").unwrap();
    assert!(
        answer.text.is_empty(),
        "canonical transcript need not copy aggregate text"
    );
    let view = db
        .history_turn_window("s", answer.seq, None, false)
        .unwrap()
        .unwrap();
    assert_eq!(view.parts, vec![TranscriptPart::Text(text.clone())]);
    assert!(!view.truncated);
    assert_eq!(view.window.unwrap().end, 1);
    // The existing message API and immutable durable text remain complete.
    assert_eq!(
        db.read_history_message_typed("s", &answer.id)
            .unwrap()
            .unwrap()
            .text,
        text
    );
}

#[test]
fn one_large_public_reference_has_whole_part_continuations() {
    use oc_core::queries::{HistoryPartCursor, HistoryPartWindow};
    use oc_core::session::MessageId;
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.begin_turn("t", "s", "inspect").unwrap();
    let text = "λ".repeat(crate::runtime::ACTIVE_CONTEXT_BYTES_CAP / 2 + 1);
    let raw = json!({
        "display_parts": [{"message":0},{"reasoning":text},{"message":1}],
        "input": [{"content":[{"type":"output_text","text":"before"}]},
                  {"content":[{"type":"output_text","text":"after"}]}]
    })
    .to_string();
    db.commit_turn("t", "completed", Some(&raw), Some("beforeafter"))
        .unwrap();
    // Settlement adds its authoritative assistant-message identity. Freeze the
    // post-commit bytes before any presentation read, using this private fixture.
    let raw: String = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT result FROM turns WHERE id='t'", [], |row| {
            row.get(0)
        })
        .unwrap();
    let tail = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(tail.parts, vec![TranscriptPart::Text("after".into())]);
    let middle = db
        .turn_presentation_window(
            "s",
            "t",
            Some(&HistoryPartCursor {
                message: MessageId("unused-direct-owner".into()),
                sequence: 2,
                newer: false,
            }),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        middle.parts,
        vec![TranscriptPart::Reasoning {
            text,
            duration_ms: None
        }]
    );
    assert_eq!(
        middle.window,
        Some(HistoryPartWindow {
            start: 1,
            end: 2,
            total: 3
        })
    );
    assert!(!middle.truncated);
    let prefix = db
        .turn_presentation_window(
            "s",
            "t",
            Some(&HistoryPartCursor {
                message: MessageId("unused-direct-owner".into()),
                sequence: 1,
                newer: false,
            }),
        )
        .unwrap()
        .unwrap();
    assert_eq!(prefix.parts, vec![TranscriptPart::Text("before".into())]);
    // The RAW/HOT execution reader has its own 16 MiB guard. Compare exact
    // stored bytes in SQLite instead of materializing that oversized journal.
    assert!(
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT result=?2 FROM turns WHERE id=?1",
                rusqlite::params!["t", raw],
                |row| row.get::<_, bool>(0)
            )
            .unwrap()
    );
}
