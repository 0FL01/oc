use super::*;

fn normalized() -> Vec<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../../../evidence/T55/sparse-completed.fixture.json"
    ))
    .unwrap()["events"]
        .as_array()
        .unwrap()
        .clone()
}

async fn generation(
    events: Vec<serde_json::Value>,
) -> Result<crate::provider::Generation, ProviderError> {
    let server = TestServer::spawn(Arc::new(move |_| Action {
        status: "200 OK",
        headers: vec![("x-should-retry", "true".into())],
        chunks: events
            .iter()
            .map(|value| (event(value.clone()), 0))
            .collect(),
        abort_after: None,
    }))
    .await;
    let result = stream_generation(
        &test_config(&server.base),
        "fixture",
        None,
        "input",
        &[],
        &NO_CANCEL,
        None,
    )
    .await;
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    result
}

#[tokio::test]
async fn prov09_sparse_completed_retains_actual_done_call() {
    let events = normalized();
    let call = events[3]["item"].clone();
    let result = generation(events).await.unwrap();
    assert_eq!(result.output, vec![call]);
    assert_eq!(result.usage, Some((100, 40)));
    assert_eq!(result.finish, crate::provider::FinishReason::Stop);
}

fn done(index: u64, item: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"response.output_item.done","output_index":index,"item":item})
}

fn terminal(output: Option<Vec<serde_json::Value>>) -> serde_json::Value {
    let mut value = serde_json::json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":100,"output_tokens":40}}});
    if let Some(output) = output {
        value["response"]["output"] = output.into();
    }
    value
}

fn call() -> serde_json::Value {
    normalized()[3]["item"].clone()
}

fn local(error: ProviderError) -> (crate::provider::OutputStage, crate::provider::OutputCode) {
    let diagnostic = format!("{error:?} {error}");
    for canary in [
        "fc_probe",
        "call_probe",
        "apply_patch",
        "patchText",
        "hello",
        "CANARY",
    ] {
        assert!(
            !diagnostic.contains(canary),
            "structural error leaked content"
        );
    }
    assert!(
        !error.is_incomplete(),
        "local rejection is not transport truncation"
    );
    match error {
        ProviderError::OutputStructure { stage, code } => (stage, code),
        other => panic!("expected local structural facts, got {other:?}"),
    }
}

#[tokio::test]
async fn prov09_snapshots_preserve_order_text_opaque_and_usage() {
    let call = call();
    let reasoning = serde_json::json!({"id":"opaque","type":"reasoning","status":"completed","encrypted_content":"OPAQUE-CANARY","summary":[]});
    let message = serde_json::json!({"id":"message","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"MESSAGE-CANARY"}]});
    let expected = vec![reasoning.clone(), call.clone(), message.clone()];
    for snapshot in [
        None,
        Some(vec![]),
        Some(expected.clone()),
        Some(vec![call.clone()]),
        Some(vec![call.clone(), message.clone()]),
        Some(vec![
            reasoning.clone(),
            call.clone(),
            call.clone(),
            message.clone(),
        ]),
        Some(vec![
            reasoning.clone(),
            call.clone(),
            reasoning.clone(),
            message.clone(),
        ]),
    ] {
        // Done events may arrive out of order; canonical output follows indices.
        let result = generation(vec![
            serde_json::json!({"type":"response.output_text.delta","delta":"text"}),
            done(2, &message),
            done(0, &reasoning),
            done(1, &call),
            done(1, &call),
            terminal(snapshot),
        ])
        .await
        .unwrap();
        assert_eq!(result.output, expected);
        assert_eq!(result.text, "text");
        assert_eq!(result.usage, Some((100, 40)));
        assert_eq!(
            result
                .items
                .iter()
                .filter(|item| matches!(item, StreamItem::OpaqueItem { .. }))
                .count(),
            1,
            "identical done must not emit opaque state twice"
        );
    }
    // Whole completed terminal evidence may fill an actually missing item.
    let result = generation(vec![
        done(0, &reasoning),
        done(2, &message),
        terminal(Some(expected.clone())),
    ])
    .await
    .unwrap();
    assert_eq!(result.output, expected);
    let result = generation(vec![terminal(Some(vec![call.clone()]))])
        .await
        .unwrap();
    assert_eq!(
        result.output,
        vec![call],
        "retain completed-only terminal compatibility"
    );
}

#[tokio::test]
async fn prov09_indexless_done_allows_terminal_positions_without_losing_fields() {
    let first = serde_json::json!({"type":"reasoning","id":"r1","status":"completed","encrypted_content":"OPAQUE-CANARY","summary":[]});
    let second = serde_json::json!({"type":"reasoning","id":"r2","status":"completed","encrypted_content":"SECOND-CANARY"});
    let message = serde_json::json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"middle"}]});
    let mut snapshot_first = first.clone();
    snapshot_first.as_object_mut().unwrap().remove("summary");
    let result = generation(vec![
        serde_json::json!({"type":"response.output_item.done","item":first}),
        serde_json::json!({"type":"response.output_item.done","item":second}),
        terminal(Some(vec![snapshot_first, message.clone(), second.clone()])),
    ])
    .await
    .unwrap();
    assert_eq!(result.output, vec![first.clone(), message, second]);
    let result = generation(vec![
        done(0, &first),
        terminal(Some(vec![{
            let mut sparse = first.clone();
            sparse.as_object_mut().unwrap().remove("encrypted_content");
            sparse
        }])),
    ])
    .await
    .unwrap();
    assert_eq!(
        result.output,
        vec![first],
        "terminal omission never erases completed opaque content"
    );
}

#[tokio::test]
async fn prov09_sparse_terminal_recovery_keeps_omitted_observed_prefix() {
    let reasoning = serde_json::json!({"type":"reasoning","id":"observed-prefix","encrypted_content":"OPAQUE-CANARY","summary":[]});
    let read = serde_json::json!({"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"probe.txt\"}"});
    let patch = call();
    for index in [None, Some(0)] {
        let mut observed = serde_json::json!({"type":"response.output_item.done","item":reasoning});
        if let Some(index) = index {
            observed["output_index"] = index.into();
        }
        let result = generation(vec![
            observed,
            terminal(Some(vec![read.clone(), patch.clone()])),
        ])
        .await
        .unwrap();
        assert_eq!(
            result.output,
            vec![reasoning.clone(), read.clone(), patch.clone()]
        );
        assert_eq!(result.usage, Some((100, 40)));
    }
}

#[tokio::test]
async fn prov09_done_and_terminal_conflicts_are_local_and_safe() {
    use crate::provider::OutputStage;
    let call = call();
    for field in ["id", "call_id", "name", "arguments"] {
        let mut changed = call.clone();
        changed[field] = if field == "arguments" {
            serde_json::json!("{\"ARG-CANARY\":true}")
        } else {
            serde_json::json!("FIELD-CANARY")
        };
        for at_terminal in [false, true] {
            let mut events = vec![done(0, &call)];
            if !at_terminal {
                events.push(done(0, &changed));
            }
            events.push(terminal(Some(vec![if at_terminal {
                changed.clone()
            } else {
                call.clone()
            }])));
            assert_eq!(
                local(generation(events).await.unwrap_err()).0,
                if at_terminal {
                    OutputStage::Completion
                } else {
                    OutputStage::Done
                }
            );
        }
    }
    let mut other = call.clone();
    other["id"] = serde_json::json!("other");
    local(
        generation(vec![done(0, &call), done(1, &other), terminal(None)])
            .await
            .unwrap_err(),
    );
    local(
        generation(vec![done(0, &call), done(1, &call), terminal(None)])
            .await
            .unwrap_err(),
    );
    other["call_id"] = serde_json::json!("other-call");
    local(
        generation(vec![done(0, &call), done(0, &other), terminal(None)])
            .await
            .unwrap_err(),
    );
    local(
        generation(vec![
            done(0, &call),
            done(1, &other),
            terminal(Some(vec![other.clone(), call.clone()])),
        ])
        .await
        .unwrap_err(),
    );
    local(
        generation(vec![terminal(Some(vec![call.clone(), other.clone(), {
            let mut duplicate = call.clone();
            duplicate["id"] = serde_json::json!("alias");
            duplicate
        }]))])
        .await
        .unwrap_err(),
    );
    let opaque = serde_json::json!({"type":"reasoning","id":"opaque","encrypted_content":"OPAQUE-CANARY","status":"completed"});
    let mut changed = opaque.clone();
    changed["encrypted_content"] = serde_json::json!("CHANGED-CANARY");
    local(
        generation(vec![done(0, &opaque), done(0, &changed), terminal(None)])
            .await
            .unwrap_err(),
    );
    changed = opaque.clone();
    changed["summary"] = serde_json::json!([{"type":"summary_text","text":"SUMMARY-CANARY"}]);
    let mut summarized = opaque.clone();
    summarized["summary"] = serde_json::json!([]);
    local(
        generation(vec![done(0, &summarized), terminal(Some(vec![changed]))])
            .await
            .unwrap_err(),
    );
}

#[tokio::test]
async fn prov09_completed_reencrypts_reasoning_but_replays_actual_done() {
    // Pinned continuation.ts:193–200 and openai-responses.test.ts:733–782.
    let reasoning = serde_json::json!({"type":"reasoning","id":"reasoning","status":"completed","summary":[],"encrypted_content":"DONE-CANARY"});
    let mut completion = reasoning.clone();
    completion["encrypted_content"] = serde_json::json!("TERMINAL-CANARY");
    let call = call();
    let result = generation(vec![
        done(0, &reasoning),
        done(1, &call),
        terminal(Some(vec![completion, call.clone()])),
    ])
    .await
    .unwrap();
    assert_eq!(result.output, vec![reasoning, call]);
    assert_eq!(result.finish, crate::provider::FinishReason::Stop);
    assert_eq!(result.usage, Some((100, 40)));
}

#[tokio::test]
async fn prov09_reasoning_replay_exception_requires_emitted_ciphertext_and_stop() {
    let reasoning = serde_json::json!({"type":"reasoning","id":"reasoning","status":"completed","summary":[],"encrypted_content":"DONE-CANARY"});
    let mut changed = reasoning.clone();
    changed["encrypted_content"] = serde_json::json!("TERMINAL-CANARY");
    assert_eq!(
        generation(vec![terminal(Some(vec![changed.clone()]))])
            .await
            .unwrap()
            .output,
        vec![changed.clone()],
        "terminal-only reasoning retains its actual bytes"
    );
    let mut without_ciphertext = reasoning.clone();
    without_ciphertext
        .as_object_mut()
        .unwrap()
        .remove("encrypted_content");
    assert_eq!(
        generation(vec![
            done(0, &reasoning),
            terminal(Some(vec![without_ciphertext.clone()]))
        ])
        .await
        .unwrap()
        .output,
        vec![reasoning.clone()],
        "omitted optional ciphertext never erases actual done bytes"
    );
    // Inferred arrival slots are relocated when a full terminal inserts a
    // message. Relocation must retain actual done provenance and bytes.
    let mut observed = done(0, &reasoning);
    observed.as_object_mut().unwrap().remove("output_index");
    let message =
        serde_json::json!({"type":"message","role":"assistant","status":"completed","content":[]});
    let result = generation(vec![
        observed,
        terminal(Some(vec![message.clone(), changed.clone()])),
    ])
    .await
    .unwrap();
    assert_eq!(result.output, vec![message, reasoning.clone()]);
    // Terminal-only recovery is not actual emitted ciphertext. Neither a
    // repeated snapshot nor filling an absent optional field creates that fact.
    local(
        generation(vec![terminal(Some(vec![
            reasoning.clone(),
            changed.clone(),
        ]))])
        .await
        .unwrap_err(),
    );
    local(
        generation(vec![
            done(0, &without_ciphertext),
            terminal(Some(vec![reasoning.clone(), changed.clone()])),
        ])
        .await
        .unwrap_err(),
    );
    let mut length = terminal(Some(vec![changed.clone()]));
    length["type"] = serde_json::json!("response.incomplete");
    length["response"]["incomplete_details"] = serde_json::json!({"reason":"max_output_tokens"});
    local(
        generation(vec![done(0, &reasoning), length])
            .await
            .unwrap_err(),
    );
    for kind in ["compaction", "message"] {
        let mut prior = reasoning.clone();
        prior["type"] = serde_json::json!(kind);
        let mut terminal_item = prior.clone();
        terminal_item["encrypted_content"] = changed["encrypted_content"].clone();
        local(
            generation(vec![done(0, &prior), terminal(Some(vec![terminal_item]))])
                .await
                .unwrap_err(),
        );
    }
    // Other shared reasoning fields retain ordinary validation/conflict rules.
    for (field, value) in [
        ("status", serde_json::json!("in_progress")),
        ("type", serde_json::json!("compaction")),
        (
            "summary",
            serde_json::json!([{"type":"summary_text","text":"SUMMARY-CANARY"}]),
        ),
        ("encrypted_content", serde_json::Value::Null),
    ] {
        let mut terminal_item = changed.clone();
        terminal_item[field] = value;
        local(
            generation(vec![
                done(0, &reasoning),
                terminal(Some(vec![terminal_item])),
            ])
            .await
            .unwrap_err(),
        );
    }
}

#[tokio::test]
async fn prov09_announced_index_identity_and_final_arguments_agree() {
    let call = call();
    for field in ["id", "call_id", "name"] {
        let mut changed = call.clone();
        changed[field] = serde_json::json!("IDENTITY-CANARY");
        local(
            generation(vec![
                normalized()[0].clone(),
                done(0, &changed),
                terminal(None),
            ])
            .await
            .unwrap_err(),
        );
    }
    local(
        generation(vec![
            normalized()[0].clone(),
            done(1, &call),
            terminal(None),
        ])
        .await
        .unwrap_err(),
    );
    let mut invalid_index = done(0, &call);
    invalid_index["output_index"] = serde_json::json!(-1);
    local(
        generation(vec![invalid_index, terminal(None)])
            .await
            .unwrap_err(),
    );
    let mut wrong_arguments = normalized()[2].clone();
    wrong_arguments["arguments"] = serde_json::json!("{\"ARG-CANARY\":true}");
    local(
        generation(vec![
            normalized()[0].clone(),
            wrong_arguments,
            done(0, &call),
            terminal(None),
        ])
        .await
        .unwrap_err(),
    );
    let mut changed_announcement = normalized()[0].clone();
    changed_announcement["item"]["name"] = serde_json::json!("NAME-CANARY");
    local(
        generation(vec![
            normalized()[0].clone(),
            changed_announcement,
            done(0, &call),
            terminal(None),
        ])
        .await
        .unwrap_err(),
    );
    let mut late_added = normalized()[0].clone();
    late_added["item"]["call_id"] = serde_json::json!("CALL-CANARY");
    local(
        generation(vec![done(0, &call), late_added, terminal(None)])
            .await
            .unwrap_err(),
    );
    local(
        generation(vec![
            done(0, &call),
            normalized()[1].clone(),
            terminal(None),
        ])
        .await
        .unwrap_err(),
    );
    let mut events = normalized();
    events.insert(1, events[0].clone());
    let result = generation(events).await.unwrap();
    assert_eq!(result.output, vec![call.clone()]);
    assert_eq!(
        result
            .items
            .iter()
            .filter(|item| matches!(item, StreamItem::ToolCallStarted { .. }))
            .count(),
        1
    );
    for omitted_first in [false, true] {
        let indexed = normalized()[0].clone();
        let mut indexless = indexed.clone();
        indexless.as_object_mut().unwrap().remove("output_index");
        let announcements = if omitted_first {
            vec![indexless, indexed]
        } else {
            vec![indexed, indexless]
        };
        let result = generation(
            announcements
                .into_iter()
                .chain([done(0, &call), terminal(None)])
                .collect(),
        )
        .await
        .expect("an omitted optional index does not contradict observed identity");
        assert_eq!(result.output, vec![call.clone()]);
        assert_eq!(
            result
                .items
                .iter()
                .filter(|item| matches!(item, StreamItem::ToolCallStarted { .. }))
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn prov09_delta_only_and_bad_json_never_manufacture_completion() {
    local(
        generation(vec![
            normalized()[0].clone(),
            normalized()[1].clone(),
            terminal(Some(vec![])),
        ])
        .await
        .unwrap_err(),
    );
    for field in ["arguments", "call_id", "name", "type", "status"] {
        let mut item = call();
        item[field] = if field == "arguments" {
            serde_json::json!("{ARG-CANARY")
        } else {
            serde_json::Value::Null
        };
        local(
            generation(vec![done(0, &item), terminal(None)])
                .await
                .unwrap_err(),
        );
    }
    let error = generation(vec![serde_json::json!({"not_type":"MESSAGE-CANARY"})])
        .await
        .unwrap_err();
    local(error);
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "200 OK",
        headers: vec![],
        chunks: vec![(b"data: {INVALID-CANARY\n\n".to_vec(), 0)],
        abort_after: None,
    }))
    .await;
    local(
        stream_generation(
            &test_config(&server.base),
            "fixture",
            None,
            "input",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .unwrap_err(),
    );
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
}

#[tokio::test]
async fn prov09_missing_terminal_and_provider_failure_stay_distinct() {
    use crate::provider::FailureKind;
    for final_event in [
        None,
        Some(
            serde_json::json!({"type":"response.failed","response":{"error":{"code":"insufficient_quota"}}}),
        ),
        Some(serde_json::json!({"type":"response.incomplete","response":{"status":"incomplete"}})),
    ] {
        let mut events = normalized()[..4].to_vec();
        if let Some(event) = final_event.clone() {
            events.push(event);
        }
        let error = generation(events).await.unwrap_err();
        assert!(
            matches!(error, ProviderError::Request(ref failure) if failure.kind ==
            if final_event.as_ref().is_some_and(|event| event["type"] == "response.failed") { FailureKind::Quota } else { FailureKind::IncompleteStream })
        );
    }
    let events = normalized()[..3].to_vec();
    assert!(
        generation(events).await.unwrap_err().is_incomplete(),
        "valid deltas without terminal remain actual EOF"
    );
    let mut events = normalized();
    events[4] = serde_json::json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"},"output":[]}});
    let result = generation(events).await.unwrap();
    assert_eq!(result.finish, crate::provider::FinishReason::Length);
    assert_eq!(result.output, vec![call()]);
    let terminal_only = serde_json::json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"max_output_tokens"},"output":[call()]}});
    local(generation(vec![terminal_only]).await.unwrap_err());
    for kind in ["reasoning", "compaction", "message"] {
        let item = serde_json::json!({"type":kind,"id":"OPAQUE-CANARY","role":"assistant","status":"failed","encrypted_content":"SECRET-CANARY"});
        local(
            generation(vec![done(0, &item), terminal(None)])
                .await
                .unwrap_err(),
        );
    }
}

#[tokio::test]
async fn prov09_done_on_held_response_cancels_without_generation() {
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "200 OK",
        headers: vec![],
        chunks: vec![
            (event(done(0, &call())), 0),
            (event(terminal(Some(vec![]))), 30000),
        ],
        abort_after: None,
    }))
    .await;
    let cancel = Arc::new(AtomicBool::new(false));
    let cancelling = cancel.clone();
    let config = test_config(&server.base);
    let attempt = tokio::spawn(async move {
        stream_generation(&config, "fixture", None, "input", &[], &cancelling, None).await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    cancel.store(true, Ordering::SeqCst);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), attempt)
            .await
            .unwrap()
            .unwrap(),
        Err(ProviderError::Cancelled)
    );
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
}
