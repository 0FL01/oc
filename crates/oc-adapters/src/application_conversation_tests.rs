use super::*;
use crate::provider::{InputItem, InputRole};
use crate::tools::TurnLog;
use oc_core::queries::ConversationAction;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};

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
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = serde_json::json!({"model":"fixture/m","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":format!("http://{}/v1",listener.local_addr().unwrap()),"apiKey":"dummy"},"models":{"m":{}}}}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
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
    let undone = timeout(
        Duration::from_secs(2),
        app.change_conversation(session.clone(), ConversationAction::Undo),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(undone.draft.as_deref(), Some("removed title source canary"));
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
