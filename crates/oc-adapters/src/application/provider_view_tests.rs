use super::*;
use oc_core::queries::{AccountAction, AccountAuthSource, KeyInput};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn request(stream: &mut tokio::net::TcpStream) -> (String, serde_json::Value) {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    let boundary = loop {
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
        if let Some(i) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..boundary]).to_ascii_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    while bytes.len() < boundary + length {
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    (
        headers,
        serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap(),
    )
}

#[tokio::test]
async fn go05_qualified_child_pins_overrides_and_commands_capture_own_authority() {
    for mode in ["profile", "override", "command"] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir(&project).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::fs::write(project.join("opencode.json"),serde_json::json!({
            "model":"alpha/same/slash","disabled_providers":["opencode-go"],"permission":{"*":"allow"},
            "agent":{"scout":{"mode":"subagent","model":if mode=="profile" {"beta/same/slash"} else {"alpha/same/slash"}}},
            "command":{"inspect":{"template":"child task","agent":"scout","model":"beta/same/slash","subtask":true}},
            "providers":{
                "alpha":{"package":"@ai-sdk/openai","settings":{"baseURL":format!("{base}/alpha"),"apiKey":"PARENT_KEY"},"models":{"same/slash":{"modelID":"parent-api","limit":{"context":100000,"output":2048}}}},
                "beta":{"package":"@ai-sdk/anthropic","settings":{"baseURL":format!("{base}/beta"),"apiKey":"CHILD_KEY"},"models":{"same/slash":{"modelID":"child-api","limit":{"context":100000,"output":2048}}}}
            }
        }).to_string()).unwrap();
        let peer = tokio::spawn(async move {
            let count = if mode == "command" { 1 } else { 3 };
            for step in 0..count {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (headers, body) = request(&mut stream).await;
                let child = mode == "command" || step == 1;
                let sse = if child {
                    assert!(headers.starts_with("post /beta/messages "));
                    assert!(headers.contains("x-api-key: child_key"));
                    assert!(!headers.contains("parent_key"));
                    assert_eq!(body["model"], "child-api");
                    [serde_json::json!({"type":"message_start","message":{"role":"assistant"}}),
                     serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"QUALIFIED_CHILD_FINAL"}}),
                     serde_json::json!({"type":"content_block_stop","index":0}),
                     serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
                     serde_json::json!({"type":"message_stop"})]
                     .iter().map(|frame|format!("data: {frame}\n\n")).collect::<String>()
                } else {
                    assert!(headers.starts_with("post /alpha/responses "));
                    assert!(headers.contains("authorization: bearer parent_key"));
                    assert!(!headers.contains("child_key"));
                    assert_eq!(body["model"], "parent-api");
                    let output = if step == 0 {
                        let mut args = serde_json::json!({"agent":"scout","description":"qualified child","prompt":"child task"});
                        if mode == "override" {
                            args["model"] = "beta/same/slash".into();
                        }
                        serde_json::json!([{"type":"function_call","id":"child-item","call_id":"child-call","name":"subagent","arguments":args.to_string()}])
                    } else {
                        assert!(body.to_string().contains("QUALIFIED_CHILD_FINAL"));
                        serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"QUALIFIED_PARENT_FINAL"}]}])
                    };
                    format!(
                        "data: {}\n\n",
                        serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})
                    )
                };
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await.unwrap();
            }
        });
        let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
            .await
            .unwrap();
        let session = SessionId::new("parent").unwrap();
        app.create_session(session.clone()).await.unwrap();
        app.rename_session(session.clone(), "fixed".into())
            .await
            .unwrap();
        app.submit(
            session.clone(),
            if mode == "command" {
                "/inspect"
            } else {
                "child task"
            }
            .into(),
        )
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), peer)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let jobs = app.child_jobs(session.clone()).await.unwrap();
                if jobs
                    .iter()
                    .any(|job| job.state == oc_core::queries::ChildState::Completed)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let jobs = app.child_jobs(session).await.unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].model, "beta/same/slash");
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let db = Db::open(&data).unwrap();
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        let raw: String = conn
            .query_row(
                "SELECT result FROM turns WHERE session_id=?1",
                [&jobs[0].child.0],
                |r| r.get(0),
            )
            .unwrap();
        let log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
        assert_eq!(log.binding.unwrap().provider, "beta");
        assert!(
            db.session_meta(&jobs[0].child.0)
                .unwrap()
                .model
                .unwrap()
                .starts_with("beta/")
        );
    }
}

fn commit(
    snapshot: &oc_core::queries::CatalogSnapshot,
    provider: &str,
) -> oc_core::queries::SessionSelectionAction {
    oc_core::queries::SessionSelectionAction::Commit(oc_core::queries::ModelCommit {
        caller: 7,
        binding: oc_core::queries::SelectionBinding {
            location: snapshot.chrome.location.clone(),
            generation: snapshot.chrome.selection_generation,
            provider: provider.into(),
            agent_id: snapshot.agent_id.clone(),
        },
        model_id: "same/slash".into(),
        variant: None,
        draft_revision: 1,
    })
}

#[tokio::test]
async fn go05_qualified_busy_switch_keeps_prepared_authority_and_reopens_home_and_session() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("note.txt"), "SETTLED_QUALIFIED_NOTE").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"alpha/same/slash","disabled_providers":["opencode-go"],"permission":{"*":"allow"},
        "providers": {
            "alpha":{"package":"@ai-sdk/openai","settings":{"baseURL":format!("{base}/alpha"),"apiKey":"ALPHA_CANARY"},"models":{"same/slash":{"modelID":"alpha-api","limit":{"context":100000,"output":2048}}}},
            "beta":{"package":"@ai-sdk/anthropic","settings":{"baseURL":format!("{base}/beta"),"apiKey":"BETA_CANARY"},"models":{"same/slash":{"modelID":"beta-api","limit":{"context":50000,"output":1024}}}}
        }
    }).to_string()).unwrap();
    let (arrived, arrival) = tokio::sync::oneshot::channel();
    let (release, gate) = tokio::sync::oneshot::channel();
    let (second, second_arrival) = tokio::sync::oneshot::channel();
    let (second_release, second_gate) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let mut gates = [Some(gate), Some(second_gate)];
        let mut notices = [Some(arrived), Some(second)];
        for step in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let boundary = loop {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(i) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..boundary]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();
            while bytes.len() < boundary + length {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap();
            if step == 1 {
                assert!(headers.starts_with("post /beta/messages "));
                assert!(headers.contains("x-api-key: beta_canary"));
                assert!(!headers.contains("alpha_canary"));
                assert_eq!(body["model"], "beta-api");
                let wire = body.to_string();
                assert!(wire.contains("SETTLED_QUALIFIED_NOTE"));
                assert!(!wire.contains("ALPHA_OPAQUE"));
            } else {
                assert!(headers.starts_with("post /alpha/responses "));
                assert!(headers.contains("authorization: bearer alpha_canary"));
                assert!(!headers.contains("beta_canary"));
                assert_eq!(body["model"], "alpha-api");
                if step == 2 {
                    let wire = body.to_string();
                    assert!(wire.contains("SETTLED_QUALIFIED_NOTE"));
                    assert!(!wire.contains("BETA_SIGNATURE"));
                }
            }
            if step < 2 {
                notices[step].take().unwrap().send(()).unwrap();
                gates[step].take().unwrap().await.unwrap();
            }
            let sse = if step == 1 {
                let frames = [
                    serde_json::json!({"type":"message_start","message":{"role":"assistant","usage":{"input_tokens":5}}}),
                    serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"public beta plan","signature":"BETA_SIGNATURE"}}),
                    serde_json::json!({"type":"content_block_stop","index":0}),
                    serde_json::json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"beta-call","name":"read","input":{"path":"note.txt"}}}),
                    serde_json::json!({"type":"content_block_stop","index":1}),
                    serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
                    serde_json::json!({"type":"message_stop"}),
                ];
                frames
                    .iter()
                    .map(|frame| format!("data: {frame}\n\n"))
                    .collect::<String>()
            } else {
                let output = if step == 0 {
                    serde_json::json!([
                        {"type":"reasoning","id":"alpha-reasoning","encrypted_content":"ALPHA_OPAQUE","summary":[]},
                        {"type":"function_call","id":"alpha-item","call_id":"alpha-call","name":"read","arguments":"{\"path\":\"note.txt\"}"}
                    ])
                } else {
                    serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"QUALIFIED_FINAL"}]}])
                };
                format!(
                    "data: {}\n\n",
                    serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})
                )
            };
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",sse.len()).as_bytes()).await.unwrap();
        }
    });
    let env = BTreeMap::from([(
        "HOME".into(),
        root.path().join("home").to_string_lossy().into_owned(),
    )]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let session = SessionId::new("qualified-owner").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "fixed".into())
        .await
        .unwrap();
    let snapshot = app
        .session_selection(
            session.clone(),
            false,
            oc_core::queries::SessionSelectionAction::Current,
        )
        .await
        .unwrap();
    app.submit(session.clone(), "one task".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), arrival)
        .await
        .unwrap()
        .unwrap();
    let beta = app
        .session_selection(session.clone(), false, commit(&snapshot, "beta"))
        .await
        .unwrap();
    assert_eq!(beta.selected_model().unwrap().provider, "beta");
    assert_eq!(beta.models[0].context, 50000);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), second_arrival)
        .await
        .unwrap()
        .unwrap();
    let alpha = app
        .session_selection(session.clone(), false, commit(&beta, "alpha"))
        .await
        .unwrap();
    assert_eq!(alpha.selected_model().unwrap().provider, "alpha");
    second_release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app
                .history_page(session.clone(), None, None, 100)
                .await
                .unwrap()
                .rows
                .iter()
                .any(|row| row.text.contains("QUALIFIED_FINAL"))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let home = app.home_selection(commit(&alpha, "beta")).await.unwrap();
    assert_eq!(home.selected_model().unwrap().provider, "beta");
    // Capture another qualified session choice, then reopen both distinct owners.
    app.session_selection(session.clone(), false, commit(&alpha, "beta"))
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let boundary;
    let original;
    {
        let db = Db::open(&data).unwrap();
        assert_eq!(db.list_tool_ops("qualified-owner").unwrap().len(), 2);
        let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        let raw:String=conn.query_row("SELECT result FROM turns WHERE session_id='qualified-owner' AND status='completed'",[],|row|row.get(0)).unwrap();
        let log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
        original = raw;
        assert_eq!(
            log.requests
                .iter()
                .map(|receipt| receipt.binding.as_ref().unwrap().provider.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "beta", "alpha"]
        );
        boundary = db
            .accept_turn(
                "fork-boundary",
                "qualified-owner",
                "next prompt",
                "next prompt",
                &oc_core::queries::ModelRef {
                    provider: "beta".into(),
                    id: "same/slash".into(),
                    variant: None,
                },
            )
            .unwrap()
            .user_message;
        let mut tail = crate::tools::TurnLog::new("same/slash", "beta", "next prompt");
        tail.turn_id = "fork-boundary".into();
        tail.user_message = Some(boundary.clone());
        db.finish_turn(
            "fork-boundary",
            "completed",
            Some(&tail.to_json().to_string()),
        )
        .unwrap();
        // A single-provider caller must still reject foreign receipt authority.
        assert!(
            db.fork_session(
                "qualified-owner",
                &boundary,
                &project.to_string_lossy(),
                "alpha",
                "{}"
            )
            .is_err()
        );
    }
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    for snapshot in [
        app.home_selection(oc_core::queries::SessionSelectionAction::Current)
            .await
            .unwrap(),
        app.session_selection(
            session.clone(),
            false,
            oc_core::queries::SessionSelectionAction::Current,
        )
        .await
        .unwrap(),
    ] {
        let choice = snapshot.selected_model().unwrap();
        assert_eq!(
            (choice.provider.as_str(), choice.id.as_str()),
            ("beta", "same/slash")
        );
    }
    let fork = app
        .fork_session(session, MessageId(boundary))
        .await
        .unwrap();
    let fork_choice = app
        .session_selection(
            fork.session.clone(),
            false,
            oc_core::queries::SessionSelectionAction::Current,
        )
        .await
        .unwrap();
    assert_eq!(fork_choice.selected_model().unwrap().provider, "beta");
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    let conn = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    let copy: String = conn
        .query_row(
            "SELECT result FROM turns WHERE session_id=?1 AND prompt='one task'",
            [&fork.session.0],
            |r| r.get(0),
        )
        .unwrap();
    let copy = crate::tools::TurnLog::from_json(&serde_json::from_str(&copy).unwrap()).unwrap();
    let original =
        crate::tools::TurnLog::from_json(&serde_json::from_str(&original).unwrap()).unwrap();
    assert_eq!(copy.requests, original.requests);
    assert_eq!(copy.binding, original.binding);
    assert_eq!(db.list_tool_ops(&fork.session.0).unwrap().len(), 2);
}

#[tokio::test]
async fn go05_provider_views_keep_same_slash_id_and_scoped_auth_without_selection_effects() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let config = serde_json::json!({
        "model":"alpha/same/slash",
        "providers":{
            "alpha":{"package":"@ai-sdk/openai","settings":{"baseURL":"https://example.com/alpha","apiKey":"ALPHA_PRIVATE_CANARY"},"models":{"same/slash":{"name":"Alpha","limit":{"context":10000,"output":1024}}}},
            "beta":{"package":"@ai-sdk/anthropic","settings":{"baseURL":"https://example.com/beta"},"models":{"same/slash":{"name":"Beta","settings":{"thinking":{"type":"adaptive"}},"limit":{"context":20000,"output":2048}}}},
            "foreign":{"package":"unknown-package","settings":{"baseURL":"https://example.com/foreign","apiKey":"{file:must-not-read}"},"models":{"other":{"name":"Foreign"}}}
        }
    });
    let file = project.join("opencode.json");
    std::fs::write(&file, config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.set_pref(crate::models_dev::CACHE_KEY, &serde_json::json!({
            "source":crate::models_dev::SOURCE,"fetched_at_ms":composition::go_catalog::now_ms(),
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{
                "same/slash":{"id":"same/slash","name":"Go public","tool_call":true,"limit":{"context":30000,"output":2048}}
            }}
        }).to_string()).unwrap();
    }
    let env = BTreeMap::from([("OPENCODE_API_KEY".into(), "GO_ENV_PRIVATE_CANARY".into())]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let alpha = app.catalog().await.unwrap();
    assert_eq!(alpha.selected_model().unwrap().provider, "alpha");
    let beta = app.provider_catalog("beta".into()).await.unwrap();
    assert_eq!(beta.provider, "beta");
    assert!(beta.selected_model().is_none());
    assert_eq!(beta.models[0].id, "same/slash");
    assert_eq!(beta.models[0].display_name, "Beta");
    assert_eq!(beta.models[0].context, 20000);
    assert_eq!(
        app.provider_accounts("beta".into(), None)
            .await
            .unwrap()
            .effective,
        AccountAuthSource::Missing
    );
    let account = app
        .provider_accounts(
            "beta".into(),
            Some(AccountAction::AddKey {
                label: "Beta account".into(),
                key: KeyInput::new("BETA_PRIVATE_CANARY".into()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(account.effective, AccountAuthSource::Stored);
    assert!(!format!("{account:?} {beta:?}").contains("PRIVATE_CANARY"));
    let go = app.provider_catalog("opencode-go".into()).await.unwrap();
    assert_eq!(go.models[0].display_name, "Go public");
    assert_eq!(go.models[0].id, beta.models[0].id);
    assert!(go.selected_model().is_none());
    assert!(app.provider_catalog("foreign".into()).await.is_err());
    assert_eq!(
        app.catalog().await.unwrap().selected_model(),
        alpha.selected_model()
    );
    let mut edited = config.clone();
    edited["providers"]["beta"]["models"]["same/slash"]["name"] = "Unadmitted edit".into();
    std::fs::write(&file, edited.to_string()).unwrap();
    assert_eq!(
        app.provider_catalog("beta".into()).await.unwrap().models[0].display_name,
        "Beta"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert!(db.list_sessions().unwrap().is_empty());
    let mut c = composition::load_local_with_env(&project, env.clone())
        .await
        .unwrap();
    c.resolve_credentials(&db).unwrap();
    c.attach_public_catalog(&db).await;
    let beta_view = &c.provider_views["beta"];
    assert_eq!(beta_view.provider.api_key, "BETA_PRIVATE_CANARY");
    assert_eq!(
        beta_view.provider.wire.protocol,
        crate::provider::protocol::Protocol::Messages
    );
    assert_eq!(c.provider.api_key, "ALPHA_PRIVATE_CANARY");
    assert_eq!(
        c.provider_views["opencode-go"].provider.api_key,
        "GO_ENV_PRIVATE_CANARY"
    );
    drop(c);
    drop(db);
    std::fs::write(&file, config.to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    assert_eq!(
        app.provider_accounts("beta".into(), None).await.unwrap(),
        account
    );
    assert_eq!(
        app.catalog().await.unwrap().selected_model(),
        alpha.selected_model()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
