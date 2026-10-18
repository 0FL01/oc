use super::*;
use oc_core::queries::{HistoryPartCursor, HistoryReadWindow, TranscriptPart};

#[tokio::test]
async fn canonical_message_pages_keep_partial_turns_at_the_continuation_edge() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    let global = root.path().join("global");
    for path in [&project, &data, &global] {
        std::fs::create_dir_all(path).unwrap();
    }
    std::fs::write(
        project.join("opencode.json"),
        serde_json::json!({"model":"fixture/m","providers":{"fixture":{
            "settings":{"baseURL":"http://127.0.0.1:1/v1","apiKey":"fixture-only"},
            "models":{"m":{}}
        }}})
        .to_string(),
    )
    .unwrap();
    let session = SessionId("partial-turn".into());
    let db = Db::open(&data).unwrap();
    db.create_bound_session(
        &session.0,
        &project.canonicalize().unwrap().to_string_lossy(),
    )
    .unwrap();
    let user = db
        .append_message(&session.0, "user", "original task")
        .unwrap();
    db.begin_turn("large", &session.0, "original task").unwrap();
    let large = "λ".repeat(5 * 1024 * 1024);
    let raw = serde_json::json!({
        "display_parts":[{"message":0},{"message":1}],
        "input":[
            {"content":[{"type":"output_text","text":large}]},
            {"content":[{"type":"output_text","text":"final"}]}
        ]
    })
    .to_string();
    db.commit_turn("large", "completed", Some(&raw), Some("aggregate final"))
        .unwrap();
    db.append_message(&session.0, "user", "later task").unwrap();
    db.append_message(&session.0, "assistant", "later answer")
        .unwrap();
    let mut before = db.read_history_page_typed(&session.0, 20, None).unwrap();
    before.reverse();
    let original_seq = before.iter().find(|r| r.id.0 == user).unwrap().seq;
    let raw_before = db.turn_result("large").unwrap();
    drop(db);

    let env = BTreeMap::from([(
        "OPENCODE_CONFIG_DIR".into(),
        global.to_string_lossy().into_owned(),
    )]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let tail = app
        .transcript_page(session.clone(), None, None, 20)
        .await
        .unwrap();
    assert_eq!(
        tail.rows.len(),
        3,
        "a partial prefix cannot be hidden inside a message page"
    );
    let anchor = tail.rows[0].id.clone();
    let turn = tail.rows[0].turn.as_ref().unwrap();
    assert_eq!(turn.window.as_ref().unwrap().start, 1);
    assert_eq!(turn.parts, [TranscriptPart::Text("final".into())]);

    let prefix = app
        .history_window(
            session.clone(),
            HistoryReadWindow {
                part: Some(HistoryPartCursor {
                    message: anchor.clone(),
                    sequence: 1,
                    newer: false,
                }),
                ..Default::default()
            },
            20,
        )
        .await
        .unwrap();
    let turn = prefix.rows[0].turn.as_ref().unwrap();
    assert_eq!(turn.window.as_ref().unwrap().start, 0);
    assert_eq!(turn.parts, [TranscriptPart::Text(large.clone())]);

    let forward = app
        .transcript_page(session.clone(), None, Some(original_seq), 20)
        .await
        .unwrap();
    assert_eq!(
        forward.rows.len(),
        1,
        "forward traversal enters at the first part, not the tail"
    );
    assert_eq!(forward.rows[0].id, anchor);
    let turn = forward.rows[0].turn.as_ref().unwrap();
    assert_eq!(turn.window.as_ref().unwrap().start, 0);
    assert_eq!(turn.parts, [TranscriptPart::Text(large)]);

    let end = app
        .history_window(
            session.clone(),
            HistoryReadWindow {
                part: Some(HistoryPartCursor {
                    message: anchor,
                    sequence: 0,
                    newer: true,
                }),
                ..Default::default()
            },
            20,
        )
        .await
        .unwrap();
    assert_eq!(
        end.rows[0].turn.as_ref().unwrap().parts,
        [TranscriptPart::Text("final".into())]
    );
    let all = app.history_page(session, None, None, 20).await.unwrap();
    assert_eq!(
        all.rows
            .iter()
            .map(|r| (&r.id, r.seq, &r.text))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|r| (&r.id, r.seq, &r.text))
            .collect::<Vec<_>>()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert_eq!(db.turn_result("large").unwrap(), raw_before);
}
