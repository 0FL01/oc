//! Narrow parser-to-effect admission proof; actual-ELF qualification is T55 R3.
use super::*;

fn event(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}

#[tokio::test]
async fn prov09_local_rejection_after_partial_has_no_retry_intent_or_effect() {
    for kind in [
        "conflict",
        "delta-only",
        "bad-arguments",
        "eof",
        "quota",
        "incomplete",
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("reject").unwrap();
        let call = serde_json::json!({"type":"function_call","id":"ITEM-CANARY","call_id":"CALL-CANARY","name":"bash","arguments":serde_json::json!({"argv":["/bin/sh","-c","printf effect >> rejection-effects"]}).to_string(),"status":"completed"});
        let done = event(
            serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call}),
        );
        let wire = match kind {
            "conflict" => {
                let mut changed = call.clone();
                changed["arguments"] = serde_json::json!("{\"ARG-CANARY\":true}");
                done.clone() + &sse_completed_output(vec![changed])
            }
            "delta-only" => {
                event(
                    serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"ITEM-CANARY","call_id":"CALL-CANARY","name":"bash"}}),
                ) + &event(
                    serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"ITEM-CANARY","delta":call["arguments"]}),
                ) + &sse_completed_output(vec![])
            }
            "bad-arguments" => {
                let mut malformed = call.clone();
                malformed["arguments"] = serde_json::json!("{ARG-CANARY");
                event(
                    serde_json::json!({"type":"response.output_item.done","output_index":0,"item":malformed}),
                ) + &sse_completed_output(vec![])
            }
            "eof" => done.clone(),
            "quota" => {
                done.clone()
                    + &event(
                        serde_json::json!({"type":"response.failed","response":{"error":{"code":"insufficient_quota"}}}),
                    )
            }
            "incomplete" => {
                done.clone()
                    + &event(
                        serde_json::json!({"type":"response.incomplete","response":{"status":"incomplete"}}),
                    )
            }
            _ => unreachable!(),
        };
        let (base, hits) = Fake::start(
            vec![
                sse_delta("partial") + &wire,
                event(serde_json::json!({"type":"error","error":{"code":"insufficient_quota"}})),
            ],
            Duration::ZERO,
        );
        let report = runtime
            .run_turn(params(
                "reject",
                "input",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Failed);
        assert!(report.calls.is_empty());
        assert!(harness.db.list_tool_ops("reject").unwrap().is_empty());
        assert!(!harness._project.path().join("rejection-effects").exists());
        let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        let retry_count: i64 = sql
            .query_row(
                "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let should_continue = matches!(kind, "eof" | "incomplete");
        assert_eq!(retry_count, i64::from(should_continue));
        assert_eq!(
            *hits.lock().unwrap(),
            if should_continue { 2 } else { 1 },
            "{kind}: local rejection must not inherit partial-read retry"
        );
        let diagnostic = report.diagnostic.unwrap();
        assert!(!diagnostic.contains("CANARY"));
        if !should_continue && kind != "quota" {
            assert!(diagnostic.contains("invalid provider output"));
        }
    }
}

#[tokio::test]
async fn prov09_sparse_done_admits_once_and_preserves_result_pair() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("sparse").unwrap();
    let call = serde_json::json!({"type":"function_call","id":"item","call_id":"call","name":"bash","arguments":serde_json::json!({"argv":["/bin/sh","-c","printf effect >> sparse-effects"]}).to_string(),"status":"completed"});
    let done =
        event(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call}));
    let (base, hits, requests) = Fake::start_recording(
        vec![
            done.clone() + &done + &sse_completed_output(vec![]),
            sse_delta("final") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "sparse",
            "input",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(harness.db.list_tool_ops("sparse").unwrap().len(), 1);
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("sparse-effects")).unwrap(),
        "effect"
    );
    assert!(function_output(&requests.lock().unwrap()[1], "call").is_some());
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[tokio::test]
async fn prov09_reencrypted_completion_replays_done_bytes_and_executes_once() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("reencrypted").unwrap();
    let reasoning = serde_json::json!({"type":"reasoning","id":"reasoning","status":"completed","summary":[],"encrypted_content":"DONE-CANARY"});
    let mut completion = reasoning.clone();
    completion["encrypted_content"] = serde_json::json!("TERMINAL-CANARY");
    let call = serde_json::json!({"type":"function_call","id":"item","call_id":"call","name":"bash","arguments":serde_json::json!({"argv":["/bin/sh","-c","printf effect >> replay-effects"]}).to_string(),"status":"completed"});
    let (base, hits, requests) = Fake::start_recording(
        vec![
            event(
                serde_json::json!({"type":"response.output_item.done","output_index":0,"item":reasoning}),
            ) + &event(
                serde_json::json!({"type":"response.output_item.done","output_index":1,"item":call}),
            ) + &sse_completed_output(vec![completion, call]),
            sse_delta("final") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "reencrypted",
            "input",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(harness.db.list_tool_ops("reencrypted").unwrap().len(), 1);
    assert_eq!(
        std::fs::read(harness._project.path().join("replay-effects")).unwrap(),
        b"effect"
    );
    let requests = requests.lock().unwrap();
    let input = requests[1]["input"].as_array().unwrap();
    assert_eq!(
        input
            .iter()
            .filter(|item| item["id"] == "reasoning")
            .collect::<Vec<_>>(),
        vec![&reasoning]
    );
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call" && item["call_id"] == "call")
            .count(),
        1
    );
    assert!(function_output(&requests[1], "call").is_some());
    assert!(!requests[1].to_string().contains("TERMINAL-CANARY"));
    let stored: serde_json::Value =
        serde_json::from_str(&harness.db.turn_result(&report.turn_id).unwrap().1.unwrap()).unwrap();
    assert!(stored["input"].as_array().unwrap().contains(&reasoning));
    assert!(!stored.to_string().contains("TERMINAL-CANARY"));
    assert!(!stored["display_parts"].to_string().contains("CANARY"));
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
