//! Captured generation measurements across provider-only checkpoint recovery.
use super::super::tests::binding_replay_tests::{owner, request, respond};
use super::*;
use crate::provider::protocol::Protocol;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

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
        chunk_timeout_ms: 3000,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
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
        runtime
            .run_turn_admitted(
                parameters(&models, &provider, &cancel),
                &lane,
                &attached,
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
