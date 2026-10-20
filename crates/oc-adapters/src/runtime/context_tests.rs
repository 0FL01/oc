//! Captured generation measurements across provider-only checkpoint recovery.
use super::super::tests::binding_replay_tests::{owner, request, respond};
use super::*;
use crate::provider::protocol::Protocol;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn full_background_notice_overflow_keeps_rejected_input_out_of_parent_checkpoint() {
    use oc_core::queries::{ChildJob, ChildState};
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("note.txt"), "SETTLED_BEFORE_OVERFLOW").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        header_timeout_ms: Some(3000),
        chunk_timeout_ms: Some(3000),
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        cache: crate::provider::CacheMode::None,
        wire: Default::default(),
    };
    // Deliberately enough model capacity: the byte guard, not token admission
    // or automatic compaction, owns this refusal.
    let catalog = models::ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":50_000_000,"output":2048}}),
        )]),
    };
    let db = Db::open(data.path()).unwrap();
    let runtime = owner(&db, project.path());
    runtime.create_session("notice-parent").unwrap();
    let cancel = AtomicBool::new(false);
    let running = runtime.run_turn(TurnParams {
        session: "notice-parent".into(),
        prompt: "read the note".into(),
        invocation: None,
        catalog: &catalog,
        model_id: "m".into(),
        variant: None,
        max_output: 2048,
        provider,
        cancel: &cancel,
    });
    tokio::pin!(running);
    let report = "\\".repeat(ACTIVE_CONTEXT_BYTES_CAP / 4);
    let first = async {
        let (mut socket, _) = request(&listener).await;
        // Arrives after initial request admission. A complete producer report
        // fits storage/notice preflight, but double JSON escaping crosses the
        // existing aggregate request limit.
        db.record_tool_intent("notice-launch", "notice-parent", None, "subagent", "{}")
            .unwrap();
        db.admit_fresh_child_job(&ChildJob {
            parent: oc_core::domain::SessionId("notice-parent".into()),
            child: oc_core::domain::SessionId("notice-child".into()),
            operation: "notice-launch".into(),
            generation: runtime.generation_id(),
            location: "work".into(),
            agent: "helper".into(),
            model: "fixture/m".into(),
            description: "escaped report".into(),
            delivery_id: "notice-delivery".into(),
            state: ChildState::Admitted,
            background: true,
            turn: None,
            result: None,
            message_id: None,
        })
        .unwrap();
        db.finish_child_job("notice-launch", ChildState::Completed, &report)
            .unwrap();
        respond(&mut socket, Protocol::Responses, true).await;
    };
    tokio::select! {
        result = &mut running => panic!("first physical request must be held: {result:?}"),
        () = first => {},
    }
    let (result, unexpected_post) = tokio::select! {
        result = &mut running => (result, false),
        (mut socket, _) = request(&listener) => {
            respond(&mut socket, Protocol::Responses, false).await;
            (running.await, true)
        }
    };
    let result = result.unwrap();
    assert_eq!(result.status, TurnStatus::Failed);
    assert!(
        result
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("active context")
    );
    assert!(
        !unexpected_post,
        "rejected escaped input cannot reach the provider"
    );
    let operations = db.list_tool_ops("notice-parent").unwrap();
    let read = operations.iter().find(|op| op.name == "read").unwrap();
    assert_eq!(read.state, "completed");
    let (state, checkpoint) = db.turn_result(read.turn.as_deref().unwrap()).unwrap();
    assert_eq!(state, "failed");
    let checkpoint: Value = serde_json::from_str(&checkpoint.unwrap()).unwrap();
    let log = TurnLog::from_json(&checkpoint).unwrap();
    assert!(log.shell_notice_messages.is_empty());
    assert_eq!(log.requests.len(), 1);
    assert!(!log.to_json().to_string().contains("notice-delivery"));
    let conn = rusqlite::Connection::open_with_flags(
        db.root().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let (kept, notice_bytes): (i64, i64) = conn.query_row("SELECT length(CAST(j.result AS BLOB)),length(CAST(m.text AS BLOB)) FROM child_jobs j JOIN messages m ON m.id=j.message_id WHERE j.operation_id='notice-launch'", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    assert_eq!(kept, report.len() as i64);
    assert!(notice_bytes > report.len() as i64 && notice_bytes < ACTIVE_CONTEXT_BYTES_CAP as i64);
}

#[tokio::test]
async fn resumed_provider_checkpoint_keeps_reported_context_without_usage_or_tool_replay() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("note.txt"), "SETTLED_NOTE").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        header_timeout_ms: Some(crate::provider::HEADER_TIMEOUT_MS),
        chunk_timeout_ms: Some(3000),
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        cache: crate::provider::CacheMode::None,
        wire: Default::default(),
    };
    let mut models = models::ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":2048}}),
        )]),
    };
    let db = Db::open(data.path()).unwrap();
    let runtime = owner(&db, project.path());
    runtime.create_session("resume-context").unwrap();
    let cancel = AtomicBool::new(false);
    fn parameters<'a>(
        catalog: &'a models::ModelCatalog,
        provider: &ResponsesConfig,
        cancel: &'a AtomicBool,
    ) -> TurnParams<'a> {
        TurnParams {
            session: "resume-context".into(),
            prompt: "read the note".into(),
            invocation: None,
            catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 2048,
            provider: provider.clone(),
            cancel,
        }
    }
    // Stop only while awaiting the next provider response, after a genuinely
    // completed measured generation and its one settled tool result.
    {
        let running = runtime.run_turn(parameters(&models, &provider, &cancel));
        tokio::pin!(running);
        let peer = async {
            let (mut socket, _) = request(&listener).await;
            let event = json!({"type":"response.completed","response":{
                "status":"completed", "usage":{"input_tokens":6000,"output_tokens":763},
                "output":[{"type":"function_call","id":"f","call_id":"settled-call",
                    "name":"read","arguments":"{\"path\":\"note.txt\"}"}]
            }});
            let wire = format!("data: {event}\n\n");
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{wire}", wire.len()).as_bytes()).await.unwrap();
            drop(socket);
            let (pending, followup) = request(&listener).await;
            assert!(followup.to_string().contains("SETTLED_NOTE"));
            pending
        };
        tokio::select! {
            result = &mut running => panic!("provider should still be pending: {result:?}"),
            pending = peer => drop(pending),
        }
    }
    let operations = db.list_tool_ops("resume-context").unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].state, "completed");
    let turn_id = operations[0].turn.clone().unwrap();
    let (status, checkpoint) = db.turn_result(&turn_id).unwrap();
    assert_eq!(status, "started");
    let checkpoint: Value = serde_json::from_str(&checkpoint.unwrap()).unwrap();
    assert_eq!(checkpoint["display"]["context_usage"], json!([6000, 763]));
    assert_eq!(checkpoint["display"]["context_usage_limit"], 500000);
    let log = TurnLog::from_json(&checkpoint).unwrap();
    let user_message = log.user_message.clone();
    let published = runtime.current.read().unwrap().clone();
    let lane = runtime.primary_lane(&published);
    let attached = runtime.request_mcp(&cancel).await.unwrap();
    // The new request's limit is not the limit of the older reported pair.
    models.models.get_mut("m").unwrap()["limit"]["context"] = 1000000.into();
    let selection = models::select_model(&models, "m").unwrap();
    let budget = models::budget(&selection, 2048, Default::default());
    let peer = async {
        let (mut socket, resumed) = request(&listener).await;
        assert!(resumed.to_string().contains("SETTLED_NOTE"));
        respond(&mut socket, Protocol::Responses, false).await; // no usage
    };
    let continuation = Box::pin(async {
        // This fixture enters the private continuation path directly. Production
        // recovery holds the execution lease before reaching this boundary.
        let _lease = runtime.begin_active().unwrap();
        runtime
            .run_turn_admitted(
                published.clone(),
                parameters(&models, &provider, &cancel),
                &lane,
                attached.clone(),
                None,
                &mut |accepted, _| assert_eq!(accepted, turn_id),
                &mut |_, _| {},
                &mut |_, _| {},
                &mut |_| {},
                &mut |_, event| {
                    assert!(
                        !matches!(
                            event,
                            ToolCallEvent::Started { .. } | ToolCallEvent::Finished { .. }
                        ),
                        "a resumed provider step must not replay the settled tool"
                    )
                },
                &budget,
                Some(log),
                None,
                None,
                false,
            )
            .await
    });
    let (report, ()) = tokio::join!(continuation, peer);
    let report = report.unwrap().report().unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "FINAL_PUBLIC");
    assert_eq!(report.context_usage, Some((6000, 763)));
    assert_eq!(report.usage, None);
    assert_eq!(db.list_tool_ops("resume-context").unwrap(), operations);
    let (status, result) = db.turn_result(&turn_id).unwrap();
    assert_eq!(status, "completed");
    let result: Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(result["display"]["context_usage"], json!([6000, 763]));
    assert_eq!(result["display"]["context_usage_limit"], 500000);
    assert_eq!(result["user_message"].as_str(), user_message.as_deref());
}
