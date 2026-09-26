use super::*;
use crate::provider::{InputItem, InputRole};
use crate::tools::TurnLog;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn fork_owner_refuses_foreign_provider_without_request_or_durable_root() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"current/m","provider":{"current":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let boundary;
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("source", &project.to_string_lossy())
            .unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "foreign".into(),
            id: "m".into(),
            variant: None,
        };
        let user = db
            .accept_turn("foreign-turn", "source", "prefix", "prefix", &model)
            .unwrap()
            .user_message;
        let mut log = TurnLog::new("foreign-turn", "m", "foreign");
        log.user_message = Some(user);
        log.input = vec![
            InputItem::message(InputRole::User, "prefix"),
            InputItem::ProviderOutput(
                serde_json::json!({"type":"reasoning","id":"foreign-item","encrypted_content":"foreign-canary"}),
            ),
            InputItem::message(InputRole::Assistant, "answer"),
        ];
        db.commit_turn(
            "foreign-turn",
            "completed",
            Some(&log.to_json().to_string()),
            Some("answer"),
        )
        .unwrap();
        boundary = MessageId(db.append_message("source", "user", "draft").unwrap());
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let original = app.read_history(SessionId("source".into())).await.unwrap();
    let error = app
        .fork_session(SessionId("source".into()), boundary)
        .await
        .unwrap_err();
    assert!(matches!(error,CoreError::Application(reason) if reason.contains("provider")));
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(
        app.list_sessions().await.unwrap(),
        [SessionId("source".into())]
    );
    assert!(app.tab_deck().await.unwrap().sessions.is_empty());
    assert_eq!(
        app.read_history(SessionId("source".into())).await.unwrap(),
        original
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert_eq!(db.list_sessions().unwrap(), ["source"]);
    assert!(
        db.tab_adoptions(&project.to_string_lossy())
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn fork_owner_is_request_free_routes_rebased_tool_context_and_rejects_busy() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let source = SessionId("source".into());
    let boundary;
    let original;
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session(&source.0, &project.to_string_lossy())
            .unwrap();
        db.rename_root_session(&source.0, "source title").unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let user = db
            .accept_turn("prior-turn", &source.0, "prefix", "prefix", &model)
            .unwrap()
            .user_message;
        let mut log = TurnLog::new("prior-turn", "m", "fixture");
        log.user_message = Some(user);
        log.input = vec![
            InputItem::message(InputRole::User, "prefix"),
            InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","id":"wire-item","call_id":"wire-call","name":"bash","arguments":"{\"command\":\"true\"}"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "wire-call".into(),
                output: "tool output".into(),
            },
            InputItem::message(InputRole::Assistant, "prefix answer"),
        ];
        log.display_parts = vec![
            serde_json::json!({"tool":"prior-op"}),
            serde_json::json!({"message":3}),
        ];
        db.record_turn_tool_intent(
            "prior-op",
            &source.0,
            "prior-turn",
            "bash",
            "{\"command\":\"true\"}",
            &log.to_json().to_string(),
        )
        .unwrap();
        db.record_tool_outcome("prior-op", "completed", Some("tool output"))
            .unwrap();
        db.commit_turn(
            "prior-turn",
            "completed",
            Some(&log.to_json().to_string()),
            Some("prefix answer"),
        )
        .unwrap();
        db.apply_dcp_schema().unwrap();
        db.save_prune_mark(&source.0, log.user_message.as_deref().unwrap())
            .unwrap();
        boundary = MessageId(
            db.append_message(&source.0, "user", "unsent draft")
                .unwrap(),
        );
        db.append_message(&source.0, "assistant", "future answer")
            .unwrap();
        original = db.read_history_full(&source.0).unwrap();
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let fork = app
        .fork_session(source.clone(), boundary.clone())
        .await
        .unwrap();
    assert_eq!(fork.prompt, "unsent draft");
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let page = app
        .history_page(fork.session.clone(), None, None, 100)
        .await
        .unwrap();
    assert_eq!(page.parent_id, None);
    assert_eq!(page.title.as_deref(), Some("source title (fork)"));
    assert_eq!(page.rows.len(), 2);
    assert_eq!(page.rows[0].text, "prefix");
    assert_eq!(page.rows[1].text, "prefix answer");
    assert!(
        page.rows[1]
            .turn
            .as_ref()
            .unwrap()
            .parts
            .iter()
            .any(|p| matches!(p, oc_core::queries::TranscriptPart::Tool(_)))
    );
    let deck = app.tab_deck().await.unwrap();
    assert!(deck.sessions.contains(&fork.session));
    assert_eq!(deck.active.as_ref(), Some(&fork.session));
    let stale = oc_core::queries::TabDeckSnapshot {
        sessions: vec![source.clone()],
        active: Some(source.clone()),
        ..deck.clone()
    };
    assert!(app.save_tab_deck(stale).await.is_err());
    app.save_tab_deck(deck).await.unwrap();
    let mut events = app.subscribe();
    app.submit(fork.session.clone(), "new prompt".into())
        .await
        .unwrap();
    let (mut socket, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        app.fork_session(source.clone(), boundary).await,
        Err(CoreError::TurnBusy)
    );
    assert_eq!(
        app.delete_session(source.clone()).await,
        Err(CoreError::TurnBusy)
    );
    assert!(matches!(
        app.open_picker_session(
            source.clone(),
            String::new(),
            false,
            app.tab_deck().await.unwrap()
        )
        .await,
        Err(CoreError::TurnBusy)
    ));
    assert_eq!(
        app.rename_session(source.clone(), "busy rename".into())
            .await,
        Err(CoreError::TurnBusy)
    );
    let mut request = Vec::new();
    let mut buf = [0; 4096];
    let body = loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        request.extend_from_slice(&buf[..n]);
        if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
            let len = String::from_utf8_lossy(&request[..end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap();
            if request.len() >= end + 4 + len {
                break serde_json::from_slice::<serde_json::Value>(
                    &request[end + 4..end + 4 + len],
                )
                .unwrap();
            }
        }
    };
    let input = body["input"].as_array().unwrap();
    let prior_user = input
        .iter()
        .position(|v| v["role"] == "user" && v["content"][0]["text"] == "prefix")
        .expect("accepted prefix prompt in real wire log");
    let call = input
        .iter()
        .position(|v| v["type"] == "function_call")
        .unwrap();
    assert!(prior_user < call);
    assert_eq!(input[call]["call_id"], "wire-call");
    assert_eq!(input[call + 1]["type"], "function_call_output");
    assert_eq!(input[call + 1]["call_id"], "wire-call");
    assert_eq!(input[call + 1]["output"], "tool output");
    assert!(!body.to_string().contains("unsent draft"));
    assert!(!body.to_string().contains("future answer"));
    let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"continued\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await.unwrap();
    loop {
        match timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { .. } => break,
            CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
            _ => {}
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    assert_eq!(
        app.read_history(fork.session.clone()).await.unwrap().len(),
        4
    );
    assert!(
        app.tab_deck()
            .await
            .unwrap()
            .sessions
            .contains(&fork.session)
    );
    assert_eq!(
        app.read_history(source.clone())
            .await
            .unwrap()
            .iter()
            .map(|m| (&m.id.0, &m.text))
            .collect::<Vec<_>>(),
        original.iter().map(|r| (&r.0, &r.2)).collect::<Vec<_>>()
    );
    app.delete_session(source).await.unwrap();
    assert_eq!(
        app.read_history(fork.session.clone()).await.unwrap().len(),
        4
    );
    assert!(
        app.tab_deck()
            .await
            .unwrap()
            .sessions
            .contains(&fork.session)
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    assert_eq!(app.read_history(fork.session).await.unwrap().len(), 4);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
