use super::*;
use crate::provider::{InputItem, InputRole};
use crate::tools::TurnLog;
use oc_core::queries::ConversationAction;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn compaction_owner_automatic_failure_and_cancel_keep_promoted_user_and_historical_context() {
    use oc_core::compaction::CompactionState;
    for cancelled in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = serde_json::json!({"model":"fixture/m","compaction":{"auto":true,"keep":{"tokens":0},"buffer":20000},"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"SECRET-API","headers":{"x-tenant":"SECRET-TENANT"}},"models":{"m":{"limit":{"context":40000,"output":2048}}}}}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        {
            let db = Db::open(&data).unwrap();
            db.create_bound_session("s", &project.to_string_lossy())
                .unwrap();
            db.rename_root_session("s", "automatic lifecycle fixture")
                .unwrap();
            // Legacy native scoped selection had an absent agent. Resolve it
            // to the genuine builtin default while retaining its model draft.
            db.set_pref(
                &format!(
                    "tui.selection.session:{}",
                    serde_json::json!([project.to_string_lossy(), "fixture", "s"])
                ),
                r#"{"agent":null,"models":{"":{"id":"m","variant":null}},"epoch":0}"#,
            )
            .unwrap();
        }
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        assert_eq!(
            app.catalog().await.unwrap().agent_id.as_deref(),
            Some("build")
        );
        let session = SessionId("s".into());
        let mut events = app.subscribe();
        app.submit(session.clone(), "old accepted source".into())
            .await
            .unwrap();
        let (mut socket, _) = request(&listener).await;
        respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"old answer\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":23000,\"output_tokens\":1800}}}\n\n").await;
        finished(&mut events).await;
        let old = app.read_history(session.clone()).await.unwrap();
        let mut receipt = app
            .request_submit(session.clone(), "genuine triggering user".into())
            .unwrap();
        receipt.wait().await.unwrap();
        let (mut held, body) = request(&listener).await;
        assert_eq!(body["tools"], serde_json::json!([]));
        assert!(
            !body["input"]
                .to_string()
                .contains("genuine triggering user")
        );
        let accepted_user = app
            .read_history(session.clone())
            .await
            .unwrap()
            .last()
            .unwrap()
            .clone();
        assert_eq!(accepted_user.text, "genuine triggering user");
        let mut started = false;
        let running = loop {
            match timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::TurnStarted { .. } => started = true,
                CoreEvent::Compaction(snapshot) if snapshot.state == CompactionState::Running => {
                    assert!(started);
                    assert_eq!(
                        snapshot.anchor.message.as_deref(),
                        Some(accepted_user.id.0.as_str())
                    );
                    break snapshot;
                }
                _ => {}
            }
        };
        if cancelled {
            app.cancel(session.clone()).await.unwrap();
        } else {
            let body = serde_json::json!({"error":{"code":"fixture_summary_failure","message":"VIS34 bounded summary failure SECRET-API SECRET-TENANT"},"headers":{"authorization":"UNRELATED-RAW-SECRET"}}).to_string();
            held.write_all(format!("HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
        let mut compaction_terminal = false;
        loop {
            match timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap()
            {
                CoreEvent::Compaction(snapshot)
                    if snapshot.id == running.id
                        && matches!(
                            snapshot.state,
                            CompactionState::Failed | CompactionState::Cancelled
                        ) =>
                {
                    assert_eq!(
                        snapshot.state,
                        if cancelled {
                            CompactionState::Cancelled
                        } else {
                            CompactionState::Failed
                        }
                    );
                    if !cancelled {
                        let diagnostic = snapshot.error.unwrap();
                        assert!(diagnostic.contains("VIS34 bounded summary failure"));
                        assert!(!diagnostic.contains("SECRET-"));
                        assert!(!diagnostic.contains("UNRELATED-RAW"));
                    }
                    compaction_terminal = true;
                }
                CoreEvent::TurnFailed { .. } if !cancelled => {
                    assert!(compaction_terminal);
                    break;
                }
                CoreEvent::TurnInterrupted { .. } if cancelled => {
                    assert!(compaction_terminal);
                    break;
                }
                CoreEvent::TurnFinished { .. } => {
                    panic!("failed compaction must not continue the primary request")
                }
                _ => {}
            }
        }
        let settled = app.read_history(session.clone()).await.unwrap();
        assert_eq!(
            settled
                .iter()
                .filter(|m| m.text == "genuine triggering user")
                .count(),
            1
        );
        assert_eq!(&settled[..old.len()], &old);
        app.change_conversation(session.clone(), ConversationAction::Undo)
            .await
            .unwrap();
        assert_eq!(app.read_history(session.clone()).await.unwrap(), old);
        app.change_conversation(session.clone(), ConversationAction::Redo)
            .await
            .unwrap();
        assert_eq!(app.read_history(session.clone()).await.unwrap(), settled);
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let db = Db::open(&data).unwrap();
        assert!(db.session_checkpoint("s").unwrap().is_none());
        let log: serde_json::Value = serde_json::from_str(
            &db.turn_result(running.anchor.turn.as_deref().unwrap())
                .unwrap()
                .1
                .unwrap(),
        )
        .unwrap();
        assert_eq!(log["display"]["agent"], "build");
        assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 24800);
        assert!(db.list_tool_ops("s").unwrap().is_empty());
        assert!(
            timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn compaction_owner_measured_23000_context_survives_restart_and_history_controls() {
    // Same catalog and usage as actual-binary VIS34 attempt 07. The transcript
    // itself is small, so only the provider measurement can trigger compaction.
    for settings in [
        serde_json::json!({"auto":true,"keep":{"tokens":0},"buffer":20000,"prune":true}),
        serde_json::json!({"auto":true,"preserve_recent_tokens":0,"reserved":20000,"prune":false}),
        serde_json::json!({"auto":false,"keep":{"tokens":0},"buffer":20000}),
    ] {
        let enabled = settings["auto"].as_bool().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = serde_json::json!({"model":"fixture/m","compaction":settings,"permission":{"bash":"allow"},"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{"limit":{"context":40000,"output":2048}}}}}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        {
            let db = Db::open(&data).unwrap();
            db.create_bound_session("s", &project.to_string_lossy())
                .unwrap();
            db.rename_root_session("s", "measured usage fixture")
                .unwrap();
        }
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, notes) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
        let typed = app.catalog().await.unwrap().chrome.config_diagnostics;
        if settings.get("prune").is_some() {
            assert_eq!(typed.len(), 1);
            assert_eq!(
                typed[0].source,
                project.join("opencode.json").to_string_lossy()
            );
            assert_eq!(typed[0].field, vec!["compaction", "prune"]);
            assert_eq!(
                typed[0].kind,
                oc_core::queries::ConfigDiagnosticKind::Unsupported
            );
            assert_eq!(
                typed[0].action,
                oc_core::queries::ConfigDiagnosticAction::Skip
            );
            assert!(notes.iter().any(|s| s.contains("compaction.prune")
                && s.contains("omitted unsupported legacy setting")));
        } else {
            assert!(typed.is_empty());
        }
        let session = SessionId("s".into());
        let mut events = app.subscribe();
        for index in 1..=3 {
            app.submit(session.clone(), format!("ARCHIVE-{index}- request"))
                .await
                .unwrap();
            let (mut socket, _) = request(&listener).await;
            if index == 1 {
                let tool = serde_json::json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":6000,"output_tokens":10},"output":[{"type":"function_call","id":"fc","call_id":"once","name":"bash","arguments":"{\"argv\":[\"/bin/sh\",\"-c\",\"printf x >> effect\"]}"}]}});
                respond(&mut socket, &format!("data: {tool}\n\n")).await;
                let continuation = request(&listener).await;
                socket = continuation.0;
                assert!(
                    continuation.1["input"]
                        .to_string()
                        .contains("function_call_output")
                );
            }
            let input = if index == 3 { 23000 } else { 6000 };
            let response = serde_json::json!({"type":"response.completed","response":{"status":"completed","usage":{"input_tokens":input,"input_tokens_details":{"cached_tokens":5000},"output_tokens":1800,"output_tokens_details":{"reasoning_tokens":800}},"output":[{"type":"message","id":format!("a{index}"),"role":"assistant","content":[{"type":"output_text","text":format!("ARCHIVE-{index}- answer")}]}]}});
            respond(&mut socket, &format!("data: {response}\n\n")).await;
            finished(&mut events).await;
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let db = Db::open(&data).unwrap();
        let archived = db.read_history_full("s").unwrap();
        let high = db.usage_anchor("s").unwrap().unwrap();
        // Cache and reasoning are contained in the Responses totals, not added twice.
        assert_eq!(high.tokens, 24800);
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 7800);
        db.change_conversation("s", ConversationAction::Redo)
            .unwrap();
        assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 24800);
        // The fork before the third admission imports the second response's
        // historical measurement, not the current source tip's 23K input.
        let fork = db
            .fork_session(
                "s",
                &archived[4].0,
                &project.to_string_lossy(),
                "fixture",
                "{}",
            )
            .unwrap();
        assert_eq!(
            db.usage_anchor(&fork.session.0).unwrap().unwrap().tokens,
            7800
        );
        db.change_conversation(&fork.session.0, ConversationAction::Undo)
            .unwrap();
        db.change_conversation(&fork.session.0, ConversationAction::Redo)
            .unwrap();
        assert_eq!(
            db.usage_anchor(&fork.session.0).unwrap().unwrap().tokens,
            7800
        );
        drop(db);
        let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
        let mut events = app.subscribe();
        let mut receipt = app
            .request_submit(session.clone(), "fourth request".into())
            .unwrap();
        let (mut socket, body) = request(&listener).await;
        receipt.wait().await.unwrap();
        if enabled {
            let user = app
                .read_history(session.clone())
                .await
                .unwrap()
                .last()
                .unwrap()
                .clone();
            assert_eq!(user.text, "fourth request");
            let mut accepted = false;
            let mut queued = false;
            let running = loop {
                match timeout(Duration::from_secs(5), events.recv())
                    .await
                    .unwrap()
                    .unwrap()
                {
                    CoreEvent::TurnStarted { .. } => accepted = true,
                    CoreEvent::Compaction(snapshot) => {
                        assert!(accepted, "compaction must follow genuine durable admission");
                        if snapshot.state == oc_core::compaction::CompactionState::Queued {
                            queued = true;
                        }
                        if snapshot.state == oc_core::compaction::CompactionState::Running {
                            assert!(queued);
                            break snapshot;
                        }
                    }
                    _ => {}
                }
            };
            assert_eq!(running.anchor.message.as_deref(), Some(user.id.0.as_str()));
            assert_eq!(
                app.compact_session(session.clone()).await.unwrap().id,
                running.id
            );
            assert_eq!(
                app.compact_session(session.clone()).await.unwrap().id,
                running.id
            );
            assert_eq!(body["tools"], serde_json::json!([]));
            assert!(body["input"].to_string().contains("ARCHIVE-1-"));
            assert!(!body["input"].to_string().contains("fourth request"));
            // Deliberately huge summary usage cannot become a new context anchor.
            respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"VIS34-CHECKPOINT continue\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":35000,\"output_tokens\":1800}}}\n\n").await;
            let rebuilt = request(&listener).await;
            socket = rebuilt.0;
            let wire = rebuilt.1["input"].to_string();
            assert!(wire.contains("VIS34-CHECKPOINT"));
            assert!(!wire.contains("ARCHIVE-1-"));
            assert_eq!(wire.matches("fourth request").count(), 1);
        } else {
            assert_ne!(body["tools"], serde_json::json!([]));
            assert!(body["input"].to_string().contains("ARCHIVE-1-"));
        }
        respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"continued\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").await;
        finished(&mut events).await;
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let db = Db::open(&data).unwrap();
        assert_eq!(
            &db.read_history_full("s").unwrap()[..archived.len()],
            &archived
        );
        assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
        assert_eq!(std::fs::read(project.join("effect")).unwrap(), b"x");
        assert_eq!(
            db.compaction_history("s").unwrap().len(),
            usize::from(enabled)
        );
        assert_eq!(db.usage_anchor("s").unwrap().is_none(), enabled);
        if enabled {
            // The summary belongs to the triggering turn's post-context, not
            // the previously settled turn's context or a presentation reanchor.
            db.change_conversation("s", ConversationAction::Undo)
                .unwrap();
            assert!(db.session_checkpoint("s").unwrap().is_none());
            assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 24800);
            db.change_conversation("s", ConversationAction::Redo)
                .unwrap();
            assert!(db.session_checkpoint("s").unwrap().is_some());
            assert!(db.usage_anchor("s").unwrap().is_none());
        }
        assert!(
            timeout(Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
        if !enabled {
            drop(db);
            let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
            let mut events = app.subscribe();
            // Abandon the unreported fourth and measured third response, then
            // execute a real replacement branch from the second response.
            app.change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            app.change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            app.submit(session.clone(), "replacement branch".into())
                .await
                .unwrap();
            let (mut socket, body) = request(&listener).await;
            assert!(!body["input"].to_string().contains("ARCHIVE-3-"));
            respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"replacement\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":1000,\"output_tokens\":100}}}\n\n").await;
            finished(&mut events).await;
            assert!(
                app.change_conversation(session.clone(), ConversationAction::Redo)
                    .await
                    .is_err()
            );
            app.shutdown().await.unwrap();
            guard.join().await.unwrap();
            let db = Db::open(&data).unwrap();
            assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 1100);
            db.change_conversation("s", ConversationAction::Undo)
                .unwrap();
            assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 7800);
            db.change_conversation("s", ConversationAction::Redo)
                .unwrap();
            assert_eq!(db.usage_anchor("s").unwrap().unwrap().tokens, 1100);
            assert_eq!(std::fs::read(project.join("effect")).unwrap(), b"x");
        }
    }
}

#[tokio::test]
async fn compaction_owner_manual_during_tool_delivers_before_next_request_and_shutdown_drains() {
    use oc_core::compaction::CompactionState;
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","compaction":{"auto":false,"keep":{"tokens":0}},"permission":{"bash":"allow"},"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{"limit":{"context":500000,"output":8000}}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("s", &project.to_string_lossy())
            .unwrap();
        db.rename_root_session("s", "compaction fixture").unwrap();
        let old = "OLD CONTEXT ".repeat(100);
        let accepted = db
            .accept_turn(
                "old",
                "s",
                &old,
                &old,
                &oc_core::queries::ModelRef {
                    provider: "fixture".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap();
        let mut log = TurnLog::new("old", "m", "fixture");
        log.user_message = Some(accepted.user_message);
        log.input = vec![InputItem::message(InputRole::User, &old)];
        db.commit_turn(
            "old",
            "completed",
            Some(&log.to_json().to_string()),
            Some("old answer"),
        )
        .unwrap();
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let mut events = app.subscribe();
    let session = SessionId("s".into());
    app.submit(session.clone(), "continue".into())
        .await
        .unwrap();
    let (mut socket, _) = request(&listener).await;
    let tool = serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","id":"fc","call_id":"once","name":"bash","arguments":"{\"argv\":[\"/bin/sh\",\"-c\",\"sleep 0.2; printf x >> effect\"]}"}]}});
    respond(&mut socket, &format!("data: {tool}\n\n")).await;
    loop {
        if matches!(
            timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap(),
            CoreEvent::ToolCallStarted { .. }
        ) {
            break;
        }
    }
    let queued = app.compact_session(session.clone()).await.unwrap();
    assert_eq!(queued.state, CompactionState::Queued);
    assert_eq!(
        app.compact_session(session.clone()).await.unwrap().id,
        queued.id
    );
    let (mut summary, body) = request(&listener).await;
    assert_eq!(std::fs::read(project.join("effect")).unwrap(), b"x");
    assert_eq!(body["tools"], serde_json::json!([]));
    respond(&mut summary,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"## Objective\\nContinue safely.\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":55,\"output_tokens\":8}}}\n\n").await;
    let (mut socket, body) = request(&listener).await;
    assert!(body["input"].to_string().contains("Continue safely."));
    assert!(!body["input"].to_string().contains("OLD CONTEXT"));
    assert_eq!(
        body["input"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["type"] == "function_call_output")
            .count(),
        1
    );
    respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").await;
    finished(&mut events).await;
    let history = app.compaction_history(session.clone()).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].state, CompactionState::Completed);
    assert_eq!(app.read_history(session.clone()).await.unwrap().len(), 4);
    // An idle summary owns its stream; queued input survives its cancellation.
    app.compact_session(session.clone()).await.unwrap();
    let (_held, _) = request(&listener).await;
    let mut receipt = app
        .request_submit(session.clone(), "preserved input".into())
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(receipt.try_result().is_none());
    app.cancel_compaction(session.clone()).await.unwrap();
    receipt.wait().await.unwrap();
    let (mut socket, body) = request(&listener).await;
    assert!(body["input"].to_string().contains("preserved input"));
    respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"continued\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").await;
    finished(&mut events).await;
    app.compact_session(session.clone()).await.unwrap();
    let (_held, _) = request(&listener).await;
    app.shutdown().await.unwrap();
    timeout(Duration::from_secs(5), guard.join())
        .await
        .unwrap()
        .unwrap();
    let reopened = Db::open(&data).unwrap();
    assert_eq!(
        reopened.compaction_history("s").unwrap()[0].state,
        CompactionState::Cancelled
    );
    assert_eq!(std::fs::read(project.join("effect")).unwrap(), b"x");
}

#[tokio::test]
async fn compaction_owner_publication_failures_surface_and_allow_selection_conversation_requeue() {
    use oc_core::compaction::CompactionState;
    for stage in ["running", "completed"] {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = serde_json::json!({"model":"fixture/m","compaction":{"auto":false,"keep":{"tokens":0}},"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{"limit":{"context":500000,"output":8000}}}}}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        {
            let db = Db::open(&data).unwrap();
            db.create_bound_session("s", &project.to_string_lossy())
                .unwrap();
            db.rename_root_session("s", "publication fixture").unwrap();
            let accepted = db
                .accept_turn(
                    "old",
                    "s",
                    "history",
                    "history",
                    &oc_core::queries::ModelRef {
                        provider: "fixture".into(),
                        id: "m".into(),
                        variant: None,
                    },
                )
                .unwrap();
            let mut log = TurnLog::new("old", "m", "fixture");
            log.user_message = Some(accepted.user_message);
            log.input = vec![InputItem::message(InputRole::User, "history")];
            db.commit_turn(
                "old",
                "completed",
                Some(&log.to_json().to_string()),
                Some("answer"),
            )
            .unwrap();
        }
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        let mut events = app.subscribe();
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        conn.execute_batch(&format!("CREATE TABLE publish_count(n INTEGER); INSERT INTO publish_count VALUES(0); CREATE TRIGGER fail_publication BEFORE UPDATE ON session_compactions WHEN json_extract(NEW.snapshot,'$.state')='{stage}' BEGIN UPDATE publish_count SET n=n+1; SELECT CASE WHEN (SELECT n FROM publish_count)>{} THEN RAISE(ABORT,'injected publication failure') END; END;",if stage=="completed" {1} else {0})).unwrap();
        let session = SessionId("s".into());
        let queued = app.compact_session(session.clone()).await.unwrap();
        if stage == "completed" {
            let (mut socket, body) = request(&listener).await;
            assert_eq!(body["tools"], serde_json::json!([]));
            respond(&mut socket,"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Preserve the completed work.\"}]}]}}\n\n").await;
        }
        loop {
            if let CoreEvent::Compaction(snapshot) = timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap()
                && snapshot.id == queued.id
                && snapshot.state == CompactionState::Failed
            {
                break;
            }
        }
        assert_eq!(
            app.compaction_history(session.clone()).await.unwrap()[0].state,
            CompactionState::Failed
        );
        app.select_model("m".into(), None).await.unwrap();
        app.change_conversation(session.clone(), ConversationAction::Undo)
            .await
            .unwrap();
        app.change_conversation(session.clone(), ConversationAction::Redo)
            .await
            .unwrap();
        conn.execute_batch("DROP TRIGGER fail_publication;")
            .unwrap();
        let retry = app.compact_session(session.clone()).await.unwrap();
        assert_ne!(retry.id, queued.id);
        if stage == "running" {
            let (_held, _) = request(&listener).await;
            app.cancel_compaction(session.clone()).await.unwrap();
        }
        app.shutdown().await.unwrap();
        timeout(Duration::from_secs(5), guard.join())
            .await
            .unwrap()
            .unwrap();
    }
}

async fn request(listener: &tokio::net::TcpListener) -> (tokio::net::TcpStream, serde_json::Value) {
    let (mut socket, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let len = String::from_utf8_lossy(&bytes[..end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap();
            if bytes.len() >= end + 4 + len {
                return (
                    socket,
                    serde_json::from_slice(&bytes[end + 4..end + 4 + len]).unwrap(),
                );
            }
        }
    }
}

async fn respond(socket: &mut tokio::net::TcpStream, sse: &str) {
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await.unwrap();
}

async fn finished(events: &mut tokio::sync::broadcast::Receiver<CoreEvent>) {
    loop {
        match timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { .. } => return,
            CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn conversation_undo_cancels_held_automatic_title_without_late_write() {
    held_title_undo(false).await;
}

#[tokio::test]
async fn conversation_fork_genuine_points_revert_redo_restart_and_real_wire() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let boundary;
    let original;
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("source", &project.to_string_lossy())
            .unwrap();
        db.rename_root_session("source", "source fixture").unwrap();
        db.apply_dcp_schema().unwrap();
        let model = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let first = db
            .accept_turn(
                "first",
                "source",
                "retained original",
                "retained original",
                &model,
            )
            .unwrap()
            .user_message;
        let block = db
            .save_compression_block(
                "source",
                "old",
                "OLD CAUSAL FORK SUMMARY",
                &first,
                &first,
                std::slice::from_ref(&first),
            )
            .unwrap();
        let mut log = TurnLog::new("first", "m", "fixture");
        log.user_message = Some(first);
        log.input = vec![
            InputItem::message(InputRole::User, "retained original"),
            InputItem::message(InputRole::Assistant, "old answer"),
        ];
        db.commit_turn(
            "first",
            "completed",
            Some(&log.to_json().to_string()),
            Some("old answer"),
        )
        .unwrap();
        let second = db
            .accept_turn(
                "second",
                "source",
                "EXCLUDED TAIL CANARY",
                "EXCLUDED TAIL CANARY",
                &model,
            )
            .unwrap()
            .user_message;
        boundary = MessageId(second.clone());
        db.delete_compression_block(&block).unwrap();
        db.save_compression_block(
            "source",
            "future",
            "FUTURE FORK SUMMARY LEAK",
            &second,
            &second,
            std::slice::from_ref(&second),
        )
        .unwrap();
        let mut log = TurnLog::new("second", "m", "fixture");
        log.user_message = Some(second);
        log.input = vec![InputItem::message(InputRole::User, "EXCLUDED TAIL CANARY")];
        db.commit_turn(
            "second",
            "completed",
            Some(&log.to_json().to_string()),
            Some("tail answer"),
        )
        .unwrap();
        original = db.read_history_full("source").unwrap();
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let fork = app
        .fork_session(SessionId("source".into()), boundary)
        .await
        .unwrap();
    let page = app
        .history_page(fork.session.clone(), None, None, 100)
        .await
        .unwrap();
    assert_eq!(page.rows.len(), 2);
    let first = page.rows[0].id.clone();
    for action in [
        ConversationAction::Revert { message: first },
        ConversationAction::Redo,
        ConversationAction::Undo,
        ConversationAction::Redo,
        ConversationAction::Undo,
    ] {
        app.change_conversation(fork.session.clone(), action)
            .await
            .unwrap();
    }
    assert!(
        app.read_history(fork.session.clone())
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let rebased_block;
    {
        let db = Db::open(&data).unwrap();
        assert_eq!(db.read_history_full("source").unwrap(), original);
        db.change_conversation(&fork.session.0, ConversationAction::Redo)
            .unwrap();
        let blocks = db.load_compression_blocks(&fork.session.0).unwrap();
        assert_eq!(blocks[0].summary, "OLD CAUSAL FORK SUMMARY");
        rebased_block = blocks[0].id.clone();
        assert_eq!(blocks[0].members, [page.rows[0].id.0.clone()]);
    }
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let mut events = app.subscribe();
    app.submit(fork.session.clone(), "continue restored fork".into())
        .await
        .unwrap();
    let (mut socket, body) = request(&listener).await;
    let input = body["input"].to_string();
    assert!(input.contains("OLD CAUSAL FORK SUMMARY"), "{input}");
    assert!(input.contains(&rebased_block), "{input}");
    assert!(input.contains("continue restored fork"));
    assert!(!input.contains("FUTURE FORK SUMMARY LEAK"));
    assert!(!input.contains("EXCLUDED TAIL CANARY"));
    assert!(!input.contains("tail answer"));
    respond(&mut socket,"data: {\"type\":\"response.output_text.delta\",\"delta\":\"continued\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n").await;
    finished(&mut events).await;
    assert!(
        timeout(Duration::from_millis(60), listener.accept())
            .await
            .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert_eq!(db.read_history_full("source").unwrap(), original);
    assert_eq!(
        db.load_compression_blocks("source").unwrap()[0].summary,
        "FUTURE FORK SUMMARY LEAK"
    );
}

#[tokio::test]
async fn conversation_undo_cancels_held_explicit_title_without_late_write() {
    held_title_undo(true).await;
}

async fn held_title_undo(explicit: bool) {
    held_title_action(explicit, false).await;
}

#[tokio::test]
async fn picker_delete_cancels_held_manual_title_and_allows_next_location_switch() {
    held_title_action(true, true).await;
}

#[tokio::test]
async fn picker_delete_cancels_held_automatic_title_and_allows_next_location_switch() {
    held_title_action(false, true).await;
}

async fn held_title_action(explicit: bool, delete: bool) {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let other = tmp.path().join("other-location");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("opencode.json"), config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("s", &project.to_string_lossy())
            .unwrap();
        if explicit {
            db.rename_root_session("s", "existing title").unwrap();
        }
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let session = SessionId("s".into());
    let mut events = app.subscribe();
    app.submit(session.clone(), "removed title source canary".into())
        .await
        .unwrap();
    let terminal =
        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    let title_body = |body: &serde_json::Value| {
        body["input"]
            .to_string()
            .contains("Generate a short session title")
    };
    let (mut first, body) = request(&listener).await;
    let (mut held, regeneration) = if explicit {
        assert!(!title_body(&body));
        respond(&mut first,&format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}}\n\n{terminal}")).await;
        finished(&mut events).await;
        let app = app.clone();
        let target = session.clone();
        let task = tokio::spawn(async move { app.regenerate_title(target).await });
        let (held, body) = request(&listener).await;
        assert!(title_body(&body));
        (held, Some(task))
    } else {
        let (mut second, other) = request(&listener).await;
        assert_ne!(title_body(&body), title_body(&other));
        if title_body(&body) {
            respond(&mut second,&format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}}\n\n{terminal}")).await;
            (first, None)
        } else {
            respond(&mut first,&format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"answer\"}}\n\n{terminal}")).await;
            (second, None)
        }
    };
    if !explicit {
        finished(&mut events).await;
    }
    if delete {
        timeout(
            Duration::from_secs(2),
            app.picker_session_action(
                session.clone(),
                String::new(),
                false,
                oc_core::queries::SessionPickerAction::Delete,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(!app.list_sessions().await.unwrap().contains(&session));
        let mut byte = [0u8; 1];
        assert_eq!(
            timeout(Duration::from_secs(2), held.read(&mut byte))
                .await
                .unwrap()
                .unwrap(),
            0,
            "deleted root must release its held provider connection"
        );
        timeout(
            Duration::from_secs(2),
            app.switch_location_home(other.display().to_string()),
        )
        .await
        .unwrap()
        .unwrap();
    } else {
        let undone = timeout(
            Duration::from_secs(2),
            app.change_conversation(session.clone(), ConversationAction::Undo),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(undone.draft.as_deref(), Some("removed title source canary"));
    }
    // Release the already accepted title only AFTER the boundary ACK. An
    // aborted socket may already be closed; either way no result can commit.
    let sse = format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":\"STALE TITLE CANARY\"}}\n\n{terminal}"
    );
    let _ = held.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await;
    if let Some(task) = regeneration {
        assert!(
            timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
    }
    tokio::time::sleep(Duration::from_millis(60)).await;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, CoreEvent::SessionTitleUpdated { .. }));
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    if delete {
        assert!(matches!(
            db.session_meta("s"),
            Err(crate::storage::StorageError::SessionNotFound)
        ));
        return;
    }
    assert_eq!(
        db.session_meta("s").unwrap().title.as_deref(),
        explicit.then_some("existing title")
    );
    assert!(db.title_context("s", explicit).unwrap().is_none());
}

#[tokio::test]
async fn conversation_redo_restores_real_tool_pairs_without_reexecuting_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","permission":{"*":"allow"},"provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("s", &project.to_string_lossy())
            .unwrap();
        db.rename_root_session("s", "tool fixture").unwrap();
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let session = SessionId("s".into());
    let mut events = app.subscribe();
    app.submit(session.clone(), "execute shell once".into())
        .await
        .unwrap();
    let (mut socket, _) = request(&listener).await;
    let args = serde_json::json!({"argv":["touch","effect-marker"]});
    let done = serde_json::json!({"type":"response.output_item.done","item":{"type":"function_call","id":"fc_once","call_id":"once","name":"bash","arguments":args.to_string(),"status":"completed"}});
    let terminal =
        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
    respond(&mut socket, &format!("data: {done}\n\n{terminal}")).await;
    let (mut socket, body) = request(&listener).await;
    assert!(body["input"].to_string().contains("function_call_output"));
    let opaque = serde_json::json!({"type":"response.output_item.done","item":{"type":"reasoning","id":"opaque_saved","summary":[],"encrypted_content":"opaque-ciphertext-canary"}});
    respond(&mut socket,&format!("data: {opaque}\n\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"executed once\"}}\n\n{terminal}")).await;
    finished(&mut events).await;
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    let original_tools = db.list_tool_ops("s").unwrap();
    let op = original_tools[0].op.clone();
    let turn_id = original_tools[0].turn.clone().unwrap();
    let original_log = db.turn_result(&turn_id).unwrap().1.unwrap();
    let original_history = db.read_history_full("s").unwrap();
    let original_presentation = db.turn_presentation("s", &turn_id).unwrap();
    for _ in 0..3 {
        db.finish_turn(&turn_id, "failed", Some("{\"input\":[]}"))
            .unwrap();
        db.commit_turn(
            &turn_id,
            "completed",
            Some("{}"),
            Some("duplicate assistant canary"),
        )
        .unwrap();
        assert!(db.checkpoint_turn(&turn_id, "{}").is_err());
        assert!(
            db.tool_outcome_with_log(&op, "failed", "late outcome canary", &turn_id, "{}")
                .is_err()
        );
        assert!(
            db.record_tool_outcome(&op, "failed", Some("late outcome canary"))
                .is_err()
        );
    }
    assert_eq!(db.read_history_full("s").unwrap(), original_history);
    assert_eq!(
        db.turn_presentation("s", &turn_id).unwrap(),
        original_presentation
    );
    assert_eq!(db.list_tool_ops("s").unwrap(), original_tools);
    let saved_log = db.turn_result(&turn_id).unwrap().1.unwrap();
    assert_eq!(saved_log, original_log);
    drop(db);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let mut events = app.subscribe();
    assert!(project.join("effect-marker").exists());
    // A replay would recreate this missing external effect.
    std::fs::remove_file(project.join("effect-marker")).unwrap();
    app.change_conversation(session.clone(), ConversationAction::Undo)
        .await
        .unwrap();
    assert!(
        app.tool_ops_page(session.clone(), None, 100)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    app.change_conversation(session.clone(), ConversationAction::Redo)
        .await
        .unwrap();
    assert_eq!(
        app.tool_ops_page(session.clone(), None, 100)
            .await
            .unwrap()
            .rows
            .len(),
        1
    );
    assert!(!project.join("effect-marker").exists());
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    app.submit(session.clone(), "continue from saved turn".into())
        .await
        .unwrap();
    let (mut socket, body) = request(&listener).await;
    let items = body["input"].as_array().unwrap();
    let log: serde_json::Value = serde_json::from_str(&original_log).unwrap();
    // Compare the opaque provider lane exactly, not just public transcript text.
    let saved: Vec<_> = log["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| {
            matches!(
                v["type"].as_str(),
                Some("function_call" | "function_call_output" | "reasoning")
            )
        })
        .cloned()
        .collect();
    let replayed: Vec<_> = items
        .iter()
        .filter(|v| {
            matches!(
                v["type"].as_str(),
                Some("function_call" | "function_call_output" | "reasoning")
            )
        })
        .cloned()
        .collect();
    assert_eq!(replayed, saved);
    assert!(
        body["input"]
            .to_string()
            .contains("opaque-ciphertext-canary")
    );
    let call = items
        .iter()
        .position(|v| v["type"] == "function_call")
        .unwrap();
    assert_eq!(items[call]["call_id"], "once");
    assert_eq!(items[call + 1]["type"], "function_call_output");
    assert_eq!(items[call + 1]["call_id"], "once");
    respond(&mut socket,&format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"continued\"}}\n\n{terminal}")).await;
    finished(&mut events).await;
    assert!(!project.join("effect-marker").exists());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

fn seed(db: &Db, id: &str, prompt: &str) -> String {
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let user = db
        .accept_turn(id, "s", prompt, prompt, &model)
        .unwrap()
        .user_message;
    let mut log = TurnLog::new(id, "m", "fixture");
    log.user_message = Some(user.clone());
    log.input = vec![
        InputItem::message(InputRole::User, prompt),
        InputItem::message(InputRole::Assistant, format!("answer {prompt}")),
    ];
    db.commit_turn(
        id,
        "completed",
        Some(&log.to_json().to_string()),
        Some(&format!("answer {prompt}")),
    )
    .unwrap();
    user
}

fn git(project: &std::path::Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(project)
        .output()
        .unwrap();
    assert!(output.status.success(), "git failed: {:?}", output.stderr);
    output.stdout
}

#[tokio::test]
async fn conversation_reverted_count_survives_reopen_independent_of_fifty_row_page() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let first;
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("s", &project.to_string_lossy())
            .unwrap();
        db.rename_root_session("s", "count fixture").unwrap();
        first = seed(&db, "first", "first");
        for i in 1..60 {
            seed(&db, &format!("turn-{i}"), &format!("prompt-{i}"));
        }
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let session = SessionId("s".into());
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    assert_eq!(
        app.history_page(session.clone(), None, None, 50)
            .await
            .unwrap()
            .rows
            .len(),
        50
    );
    app.change_conversation(session.clone(), ConversationAction::Undo)
        .await
        .unwrap();
    let undo = app
        .change_conversation(session.clone(), ConversationAction::Undo)
        .await
        .unwrap();
    assert_eq!(undo.reverted.unwrap().user_messages, 2);
    let redo = app
        .change_conversation(session.clone(), ConversationAction::Redo)
        .await
        .unwrap();
    assert!(redo.reverted.is_none());
    assert_eq!(
        app.history_page(session.clone(), None, None, 50)
            .await
            .unwrap()
            .total,
        120
    );
    let revert = app
        .change_conversation(
            session.clone(),
            ConversationAction::Revert {
                message: oc_core::session::MessageId(first),
            },
        )
        .await
        .unwrap();
    assert_eq!(revert.reverted.as_ref().unwrap().user_messages, 60);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let page = app
        .history_page(session.clone(), None, None, 50)
        .await
        .unwrap();
    assert!(page.rows.is_empty());
    assert_eq!(page.reverted, revert.reverted);
    app.change_conversation(session.clone(), ConversationAction::Redo)
        .await
        .unwrap();
    let page = app.history_page(session, None, None, 50).await.unwrap();
    assert!(page.reverted.is_none());
    assert_eq!(page.total, 120);
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn conversation_owner_restores_actual_wire_context_redo_is_request_free_and_git_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    std::fs::write(project.join("tracked"), "original").unwrap();
    git(&project, &["init", "-q"]);
    git(&project, &["add", "tracked"]);
    git(
        &project,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    std::fs::write(project.join("tracked"), "new workspace remains new").unwrap();
    std::fs::write(project.join("untracked"), "untracked survives").unwrap();
    let head = git(&project, &["rev-parse", "HEAD"]);
    let index = std::fs::read(project.join(".git/index")).unwrap();
    let status = git(&project, &["status", "--porcelain"]);
    {
        let db = Db::open(&data).unwrap();
        db.create_bound_session("s", &project.to_string_lossy())
            .unwrap();
        db.rename_root_session("s", "conversation fixture").unwrap();
        db.apply_dcp_schema().unwrap();
        let first = seed(&db, "one", "prefix secret fact");
        let old = db
            .save_compression_block(
                "s",
                "old",
                "OLDER DCP SUMMARY",
                &first,
                &first,
                std::slice::from_ref(&first),
            )
            .unwrap();
        let selected = oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        let second = db
            .accept_turn(
                "two",
                "s",
                "undone tail canary",
                "undone tail canary",
                &selected,
            )
            .unwrap()
            .user_message;
        db.delete_compression_block(&old).unwrap();
        db.save_compression_block(
            "s",
            "new",
            "LATER SUMMARY LEAK",
            &first,
            &first,
            std::slice::from_ref(&first),
        )
        .unwrap();
        let mut log = TurnLog::new("two", "m", "fixture");
        log.user_message = Some(second);
        log.input = vec![
            InputItem::message(InputRole::User, "undone tail canary"),
            InputItem::message(InputRole::Assistant, "undone answer canary"),
        ];
        db.commit_turn(
            "two",
            "completed",
            Some(&log.to_json().to_string()),
            Some("undone answer canary"),
        )
        .unwrap();
    }
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let session = SessionId("s".into());
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let snapshot = app
        .change_conversation(session.clone(), ConversationAction::Undo)
        .await
        .unwrap();
    assert_eq!(snapshot.draft.as_deref(), Some("undone tail canary"));
    app.change_conversation(session.clone(), ConversationAction::Redo)
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    app.change_conversation(session.clone(), ConversationAction::Undo)
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let page = app
        .history_page(session.clone(), None, None, 100)
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.rows.len(), 2);
    let mut events = app.subscribe();
    app.submit(session.clone(), "new branch".into())
        .await
        .unwrap();
    let (mut socket, body) = request(&listener).await;
    let input = body["input"].to_string();
    assert!(input.contains("OLDER DCP SUMMARY"), "{input}");
    assert!(!input.contains("LATER SUMMARY LEAK"), "{input}");
    assert!(!input.contains("undone tail canary"), "{input}");
    assert!(input.contains("new branch"));
    let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"branch answer\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n";
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
    assert!(
        app.change_conversation(session.clone(), ConversationAction::Redo)
            .await
            .is_err()
    );
    // Cancellation waits for the outstanding request and terminal journal cleanup.
    app.submit(session.clone(), "cancelled turn".into())
        .await
        .unwrap();
    let next = request(&listener);
    tokio::pin!(next);
    let (_socket, _) = loop {
        tokio::select! {
            request = &mut next => break request,
            event = events.recv() => if let CoreEvent::TurnFailed { error, .. } = event.unwrap() {panic!("cancelled-turn admission failed: {error}");},
        }
    };
    let undone = timeout(
        Duration::from_secs(5),
        app.change_conversation(session.clone(), ConversationAction::Undo),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(undone.draft.as_deref(), Some("cancelled turn"));
    assert!(undone.can_redo);
    app.change_conversation(session.clone(), ConversationAction::Redo)
        .await
        .unwrap();
    assert!(
        timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(
        std::fs::read(project.join("tracked")).unwrap(),
        b"new workspace remains new"
    );
    assert_eq!(
        std::fs::read(project.join("untracked")).unwrap(),
        b"untracked survives"
    );
    assert_eq!(git(&project, &["rev-parse", "HEAD"]), head);
    assert_eq!(std::fs::read(project.join(".git/index")).unwrap(), index);
    assert_eq!(git(&project, &["status", "--porcelain"]), status);
    let db = Db::open(&data).unwrap();
    assert!(
        db.read_history_full("s")
            .unwrap()
            .iter()
            .any(|r| r.2 == "undone tail canary")
    );
    assert!(
        !db.conversation_history_full("s")
            .unwrap()
            .iter()
            .any(|r| r.2 == "undone tail canary")
    );
}
