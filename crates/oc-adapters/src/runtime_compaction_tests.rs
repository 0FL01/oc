use super::*;
use oc_core::queries::{ConversationAction, ModelRef};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn catalog() -> ModelCatalog {
    ModelCatalog {
        provider: "fixture".into(),
        models: [(
            "m".into(),
            serde_json::json!({"limit":{"context":500_000,"output":8_000}}),
        )]
        .into_iter()
        .collect(),
    }
}
fn runtime<'a>(db: &'a Db, project: &std::path::Path, config: Generation) -> Runtime<'a> {
    Runtime::new(
        db,
        "work",
        config,
        ProtectedGlobs { patterns: vec![] },
        crate::files::Files::new(project, db.root()).unwrap(),
        crate::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        ToolRoots {
            project: project.into(),
            data: db.root().into(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap()
}
fn seed(db: &Db, turn: &str, text: &str) -> String {
    let user = db
        .accept_turn(
            turn,
            "s",
            text,
            text,
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap()
        .user_message;
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(user.clone());
    log.input = vec![
        InputItem::message(InputRole::User, text),
        InputItem::message(InputRole::Assistant, format!("answer {text}")),
    ];
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some(&format!("answer {text}")),
    )
    .unwrap();
    user
}
async fn read_request(
    listener: &tokio::net::TcpListener,
) -> (tokio::net::TcpStream, serde_json::Value) {
    let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 4096];
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let len = String::from_utf8_lossy(&bytes[..end])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.parse::<usize>().ok())
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
fn config(listener: &tokio::net::TcpListener) -> ResponsesConfig {
    ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "dummy".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(2),
        allow_private: true,
        headers: BTreeMap::new(),
        set_cache_key: false,
    }
}
fn sse(text: &str) -> String {
    format!(
        "data: {}\n\ndata: {}\n\n",
        serde_json::json!({"type":"response.output_text.delta","delta":text}),
        serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":100,"output_tokens":30,"input_tokens_details":{"cached_tokens":40},"output_tokens_details":{"reasoning_tokens":10}}}})
    )
}
async fn respond(socket: &mut tokio::net::TcpStream, body: &str) {
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn compaction_real_summary_wire_usage_tail_dcp_undo_redo_reopen_fork() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = seed(&db, "one", &"OLD HISTORY CANARY ".repeat(100));
    db.save_compression_block(
        "s",
        "closed",
        "DCP retained facts",
        &first,
        &first,
        std::slice::from_ref(&first),
    )
    .unwrap();
    seed(&db, "two", "RECENT TAIL");
    assert_eq!(
        db.compaction_boundary("s", 0, 15_000).unwrap().unwrap().0,
        2,
        "when all history fits, preserve the latest exchange"
    );
    let before = db.read_history_full("s").unwrap();
    let mut generation = Generation::default();
    generation.compaction.keep_tokens = 0;
    let runtime = runtime(&db, project.path(), generation);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        assert_eq!(request["tools"], serde_json::json!([]));
        let wire = request["input"].to_string();
        assert!(wire.contains("DCP retained facts"));
        assert!(!wire.contains("RECENT TAIL"));
        respond(&mut socket, &sse("## Objective\nContinue work.")).await;
    });
    let queued = runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert_eq!(
        runtime
            .queue_compaction("s", CompactionReason::Manual)
            .unwrap()
            .id,
        queued.id
    );
    assert!(
        runtime
            .deliver_compaction("s", &catalog(), "m", None, &provider)
            .await
            .unwrap()
    );
    task.await.unwrap();
    assert_eq!(db.read_history_full("s").unwrap(), before);
    let completed = db.compaction_history("s").unwrap().remove(0);
    assert_eq!(completed.state, CompactionState::Completed);
    let usage = completed.usage.unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.cache_read_tokens,
            usage.output_tokens,
            usage.reasoning_tokens
        ),
        (60, 40, 20, 10)
    );
    assert_eq!(db.load_compression_blocks("s").unwrap().len(), 1);
    let active = runtime.active_projection("s").unwrap();
    let wire = runtime
        .wire_history(
            "s",
            &active.projected,
            &active.blocks,
            "m",
            "fixture",
            None,
            active.after_seq,
        )
        .unwrap();
    let raw = serde_json::to_string(&wire).unwrap();
    assert!(raw.contains("Continue work."));
    assert!(raw.contains("RECENT TAIL"));
    assert!(!raw.contains("OLD HISTORY CANARY"));
    let third = seed(&db, "three", "NEXT USER");
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_some());
    db.change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_none());
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_some());
    let fork = db
        .fork_session("s", &third, "work", "fixture", "{}")
        .unwrap();
    assert!(db.session_checkpoint(&fork.session.0).unwrap().is_some());
    let fork_compaction = db.compaction_history(&fork.session.0).unwrap();
    assert_eq!(fork_compaction[0].state, CompactionState::Completed);
    assert_eq!(fork_compaction[0].summary, "## Objective\nContinue work.");
    drop(runtime);
    drop(db);
    let reopened = Db::open(data.path()).unwrap();
    assert!(reopened.session_checkpoint("s").unwrap().is_some());
    assert_eq!(
        reopened.compaction_history("s").unwrap()[0].state,
        CompactionState::Completed
    );
    assert_eq!(reopened.read_history_full("s").unwrap().len(), 6);
    reopened
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    reopened
        .change_conversation("s", ConversationAction::Undo)
        .unwrap();
    assert!(reopened.session_checkpoint("s").unwrap().is_none());
    seed(&reopened, "branch", "new branch");
    assert!(
        reopened
            .change_conversation("s", ConversationAction::Redo)
            .is_err()
    );
    // All abandoned rows survive in the archive, but their checkpoint is not active.
    assert_eq!(reopened.read_history_full("s").unwrap().len(), 8);
    drop(reopened);
    let reopened = Db::open(data.path()).unwrap();
    assert!(reopened.session_checkpoint("s").unwrap().is_none());
}

#[tokio::test]
async fn compaction_invalid_summary_and_cancel_preserve_context() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "one", "original context");
    let runtime = runtime(&db, project.path(), Generation::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (mut socket, _) = read_request(&listener).await;
        respond(&mut socket, &sse("   ")).await;
        let (mut correction, request) = read_request(&listener).await;
        assert!(
            request["input"]
                .to_string()
                .contains("required summary template")
        );
        respond(&mut correction, &sse("   ")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        !runtime
            .deliver_compaction("s", &catalog(), "m", None, &provider)
            .await
            .unwrap()
    );
    task.await.unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_none());
    assert_eq!(
        db.compaction_history("s").unwrap()[0].state,
        CompactionState::Failed
    );
    let queued = runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    runtime.cancel_compaction("s").unwrap();
    assert_eq!(db.compaction_history("s").unwrap()[0].id, queued.id);
    assert_eq!(
        db.compaction_history("s").unwrap()[0].state,
        CompactionState::Cancelled
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (_socket, _) = read_request(&listener).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    let catalog = catalog();
    let operation = runtime.deliver_compaction("s", &catalog, "m", None, &provider);
    let cancelling = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        runtime.cancel_compaction("s").unwrap();
    };
    let (result, ()) = tokio::join!(operation, cancelling);
    assert!(!result.unwrap());
    task.await.unwrap();
    assert!(db.session_checkpoint("s").unwrap().is_none());
    assert_eq!(
        db.compaction_history("s").unwrap()[0].state,
        CompactionState::Cancelled
    );
}

#[test]
fn compaction_config_native_legacy_and_unsupported_prune() {
    let mut cfg = crate::compaction::CompactionConfig::default();
    let notes = cfg.merge(&serde_json::json!({"auto":false,"reserved":1,"buffer":2,"preserve_recent_tokens":3,"keep":{"tokens":4}}));
    assert_eq!(notes.len(), 2);
    assert_eq!((cfg.auto, cfg.buffer, cfg.keep_tokens), (false, 2, 4));
    assert_eq!(
        cfg.merge(&serde_json::json!({"prune":true}))[0].kind,
        oc_core::queries::ConfigDiagnosticKind::Unsupported
    );
    assert_eq!(
        cfg.merge(&serde_json::json!({"keep":{"tokens":-1}}))[0].kind,
        oc_core::queries::ConfigDiagnosticKind::Invalid
    );
    assert_eq!((cfg.auto, cfg.buffer, cfg.keep_tokens), (false, 2, 4));
    let sources=[crate::config::Source { path:"global".into(),trusted:true,text:serde_json::json!({"compaction":{"reserved":123,"preserve_recent_tokens":321,"auto":false}}).to_string() },crate::config::Source { path:"project".into(),trusted:true,text:serde_json::json!({"compaction":{"buffer":456,"keep":{"tokens":654}}}).to_string() }];
    let generation = crate::config::assemble(&sources, &BTreeMap::new(), None).unwrap();
    assert_eq!(
        (
            generation.compaction.auto,
            generation.compaction.buffer,
            generation.compaction.keep_tokens
        ),
        (false, 456, 654)
    );
    assert_eq!(generation.provenance["compaction"], "project");
}

#[tokio::test]
async fn compaction_auto_threshold_and_known_overflow_keep_tool_effect_once() {
    for (overflow, auto) in [(false, true), (true, true), (false, false), (true, false)] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        seed(&db, "old", &"OLD CANARY ".repeat(1000));
        let mut generation = Generation::default();
        generation.compaction.auto = auto;
        generation.compaction.buffer = if overflow { 0 } else { 499_000 };
        generation
            .permissions
            .insert("bash".into(), Permission::Allow);
        let runtime = runtime(&db, project.path(), generation);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = config(&listener);
        let task = tokio::spawn(async move {
            if !overflow && auto {
                let (mut socket, body) = read_request(&listener).await;
                assert_eq!(body["tools"], serde_json::json!([]));
                respond(&mut socket, &sse("## Objective\nResume coding.")).await;
                let (mut socket, body) = read_request(&listener).await;
                assert!(body["input"].to_string().contains("Resume coding."));
                assert!(!body["input"].to_string().contains("OLD CANARY"));
                respond(&mut socket, &sse("done")).await;
            } else if !overflow {
                let (mut socket, body) = read_request(&listener).await;
                assert!(!body["tools"].as_array().unwrap().is_empty());
                assert!(body["input"].to_string().contains("OLD CANARY"));
                respond(&mut socket, &sse("done")).await;
            } else {
                let (mut socket, _) = read_request(&listener).await;
                let event = serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","id":"fc","call_id":"once","name":"bash","arguments":"{\"argv\":[\"/bin/sh\",\"-c\",\"printf x >> effect\"]}"}]}});
                respond(&mut socket, &format!("data: {event}\n\n")).await;
                let (mut socket, body) = read_request(&listener).await;
                assert!(
                    body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|i| i["type"] == "function_call_output")
                );
                respond(&mut socket,"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"context_length_exceeded\"}}}\n\n").await;
                if !auto {
                    assert!(
                        tokio::time::timeout(Duration::from_millis(100), listener.accept())
                            .await
                            .is_err()
                    );
                    return;
                }
                let (mut socket, body) = read_request(&listener).await;
                assert_eq!(body["tools"], serde_json::json!([]));
                respond(&mut socket, &sse("## Objective\nResume coding.")).await;
                let (mut socket, body) = read_request(&listener).await;
                assert!(body["input"].to_string().contains("Resume coding."));
                assert!(!body["input"].to_string().contains("OLD CANARY"));
                assert_eq!(
                    body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|i| i["type"] == "function_call_output")
                        .count(),
                    1
                );
                respond(&mut socket, &sse("done")).await;
            }
        });
        let catalog = catalog();
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "s".into(),
                prompt: "continue".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider: provider.clone(),
                cancel: &cancel,
                max_rounds: 4,
            })
            .await
            .unwrap();
        task.await.unwrap();
        assert_eq!(
            report.status,
            if overflow && !auto {
                TurnStatus::Failed
            } else {
                TurnStatus::Completed
            }
        );
        let compact = db.compaction_history("s").unwrap();
        if !auto {
            assert!(compact.is_empty());
            assert!(db.session_checkpoint("s").unwrap().is_none());
            if overflow {
                assert_eq!(std::fs::read(project.path().join("effect")).unwrap(), b"x");
                assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
            }
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let provider = config(&listener);
            let task = tokio::spawn(async move {
                let (mut socket, body) = read_request(&listener).await;
                assert_eq!(body["tools"], serde_json::json!([]));
                respond(&mut socket, &sse("## Objective\nManual remains available")).await;
            });
            runtime
                .queue_compaction("s", CompactionReason::Manual)
                .unwrap();
            assert!(
                runtime
                    .deliver_compaction("s", &catalog, "m", None, &provider)
                    .await
                    .unwrap()
            );
            task.await.unwrap();
            assert!(db.usage_anchor("s").unwrap().is_none());
            drop(runtime);
            drop(db);
            let reopened = Db::open(data.path()).unwrap();
            let mut generation = Generation::default();
            generation.compaction.buffer = 499_999; // without the checkpoint guard always due
            let runtime = self::runtime(&reopened, project.path(), generation);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let provider = config(&listener);
            let task = tokio::spawn(async move {
                let (mut socket, body) = read_request(&listener).await;
                assert!(!body["tools"].as_array().unwrap().is_empty());
                assert!(
                    body["input"]
                        .to_string()
                        .contains("Manual remains available")
                );
                respond(&mut socket, &sse("checkpoint primary resumed")).await;
            });
            let resumed = runtime
                .run_turn(TurnParams {
                    session: "s".into(),
                    prompt: "resume checkpoint".into(),
                    invocation: None,
                    catalog: &catalog,
                    model_id: "m".into(),
                    variant: None,
                    max_output: 1024,
                    provider,
                    cancel: &cancel,
                    max_rounds: 4,
                })
                .await
                .unwrap();
            task.await.unwrap();
            assert_eq!(resumed.status, TurnStatus::Completed);
            assert_eq!(reopened.compaction_history("s").unwrap().len(), 1);
            assert_eq!(
                reopened.list_tool_ops("s").unwrap().len(),
                usize::from(overflow)
            );
            continue;
        }
        assert_eq!(
            compact[0].reason,
            if overflow {
                CompactionReason::Overflow
            } else {
                CompactionReason::Automatic
            }
        );
        assert_eq!(compact[0].state, CompactionState::Completed);
        if overflow {
            assert_eq!(report.calls[0].state, "completed", "{:?}", report.calls);
            assert_eq!(std::fs::read(project.path().join("effect")).unwrap(), b"x");
            assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
        }
    }
}

struct Native(std::sync::atomic::AtomicUsize);

#[tokio::test]
async fn compaction_failed_overflow_summary_does_not_retry_the_main_request() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", &"preserved ".repeat(100));
    let mut generation = Generation::default();
    generation.compaction.buffer = 0;
    let runtime = runtime(&db, project.path(), generation);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (mut socket, _) = read_request(&listener).await;
        respond(&mut socket,"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"context_length_exceeded\"}}}\n\n").await;
        let (mut socket, body) = read_request(&listener).await;
        assert_eq!(body["tools"], serde_json::json!([]));
        respond(&mut socket, &sse("")).await;
        let (mut socket, body) = read_request(&listener).await;
        assert_eq!(body["tools"], serde_json::json!([]));
        respond(&mut socket, &sse("")).await;
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let catalog = catalog();
    let cancel = AtomicBool::new(false);
    let report = runtime
        .run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1024,
            provider,
            cancel: &cancel,
            max_rounds: 4,
        })
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert!(db.session_checkpoint("s").unwrap().is_none());
    assert_eq!(
        db.compaction_history("s").unwrap()[0].state,
        CompactionState::Failed
    );
    task.await.unwrap();
}
impl crate::compaction::NativeCompaction for Native {
    fn compact<'a>(
        &'a self,
        route: &'a str,
        _input: &'a [InputItem],
        _cancel: &'a AtomicBool,
        _dispatch: &'a mut (dyn FnMut() -> Result<(), crate::provider::ProviderError> + Send),
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<crate::compaction::NativeCheckpoint>,
                        crate::provider::ProviderError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(Some(crate::compaction::NativeCheckpoint {
                route: route.into(),
                replacement: serde_json::json!({"type":"compaction","encrypted_content":"opaque provider bytes"}),
                usage: None,
            }))
        })
    }
}

struct NativeResponses(ResponsesConfig);
impl crate::compaction::NativeCompaction for NativeResponses {
    fn compact<'a>(
        &'a self,
        route: &'a str,
        input: &'a [InputItem],
        cancel: &'a AtomicBool,
        dispatch: &'a mut (dyn FnMut() -> Result<(), crate::provider::ProviderError> + Send),
    ) -> std::pin::Pin<
        Box<
            dyn Future<
                    Output = Result<
                        Option<crate::compaction::NativeCheckpoint>,
                        crate::provider::ProviderError,
                    >,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let generation = crate::provider::stream_input_counted(
                &self.0,
                "m",
                None,
                input,
                &[],
                8000,
                cancel,
                &mut |_| {},
                &mut || std::future::ready(dispatch()),
            )
            .await?;
            Ok(Some(crate::compaction::NativeCheckpoint {
                route: route.into(),
                replacement: generation.output[0].clone(),
                usage: generation.compaction_usage,
            }))
        })
    }
}

#[tokio::test]
async fn ret01_native_auxiliary_retry_counts_actual_sockets() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "old user");
    let runtime = runtime(&db, project.path(), Generation::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    runtime.register_native_compaction(Arc::new(NativeResponses(provider.clone())));
    let server = tokio::spawn(async move {
        let (mut first, request) = read_request(&listener).await;
        respond(
            &mut first,
            "data: {\"type\":\"error\",\"error\":{\"code\":\"server_error\"}}\n\n",
        )
        .await;
        drop(first);
        let (mut second, retry) = read_request(&listener).await;
        assert_eq!(request, retry);
        respond(&mut second,"data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"compaction\",\"status\":\"completed\",\"encrypted_content\":\"opaque\"}]}}\n\n").await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        runtime
            .deliver_compaction("s", &catalog(), "m", None, &provider)
            .await
            .unwrap()
    );
    server.await.unwrap();
    let conn = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    assert_eq!(conn.query_row("SELECT count(*) FROM events WHERE kind='generation_dispatched' AND json_extract(payload,'$.lane')='native_compaction'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
}
#[tokio::test]
async fn compaction_native_opaque_route_checkpoint_without_summary_body() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "old user");
    let runtime = runtime(&db, project.path(), Generation::default());
    runtime.register_native_compaction(Arc::new(Native(std::sync::atomic::AtomicUsize::new(0))));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        runtime
            .deliver_compaction("s", &catalog(), "m", None, &provider)
            .await
            .unwrap()
    );
    let snapshot = db.compaction_history("s").unwrap().remove(0);
    assert!(snapshot.provider_native);
    assert!(snapshot.summary.is_empty());
    let active = runtime.active_projection("s").unwrap();
    let wire = runtime
        .wire_history(
            "s",
            &active.projected,
            &active.blocks,
            "m",
            "fixture",
            None,
            active.after_seq,
        )
        .unwrap();
    assert!(
        matches!(&wire[0],InputItem::ProviderOutput(v) if v["encrypted_content"]=="opaque provider bytes")
    );
    assert!(
        runtime
            .validate_checkpoint_route("s", "fixture", "another-model", &provider)
            .is_err()
    );
    let mut different = provider.clone();
    different
        .headers
        .insert("X-Tenant".into(), "another-tenant-secret".into());
    assert!(
        runtime
            .validate_checkpoint_route("s", "fixture", "m", &different)
            .is_err()
    );
    let cancel = AtomicBool::new(false);
    assert!(
        runtime
            .run_turn(TurnParams {
                session: "s".into(),
                prompt: "continue".into(),
                invocation: None,
                catalog: &catalog(),
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider: different.clone(),
                cancel: &cancel,
                max_rounds: 1
            })
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
    different.base_url = "http://different-route/v1".into();
    assert!(
        runtime
            .validate_checkpoint_route("s", "fixture", "m", &different)
            .is_err()
    );
}

#[test]
fn compaction_route_hash_uses_effective_case_insensitive_tenant_auth_and_endpoint() {
    let mut provider = ResponsesConfig {
        base_url: "https://example.test/v1".into(),
        api_key: "secret".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: BTreeMap::from([("X-Tenant".into(), "private-tenant".into())]),
        set_cache_key: false,
    };
    let route = crate::compaction::route_identity("p", "m", &provider).unwrap();
    assert_eq!(route.len(), 64);
    assert!(!route.contains("secret") && !route.contains("private-tenant"));
    provider.headers = BTreeMap::from([
        ("x-tenant".into(), "private-tenant".into()),
        ("AUTHORIZATION".into(), "ignored".into()),
    ]);
    provider.base_url.push('/');
    assert_eq!(
        route,
        crate::compaction::route_identity("p", "m", &provider).unwrap()
    );
    provider
        .headers
        .insert("x-tenant".into(), "different".into());
    assert_ne!(
        route,
        crate::compaction::route_identity("p", "m", &provider).unwrap()
    );
    provider
        .headers
        .insert("x-tenant".into(), "private-tenant".into());
    provider.api_key = "different-credential".into();
    assert_ne!(
        route,
        crate::compaction::route_identity("p", "m", &provider).unwrap()
    );
    provider.headers.insert("Host".into(), "invalid".into());
    assert!(crate::compaction::route_identity("p", "m", &provider).is_err());
}

#[tokio::test]
async fn compaction_auto_precancel_and_held_caller_cancel_install_no_checkpoint() {
    for mode in ["precancel", "automatic", "overflow"] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        seed(
            &db,
            "old",
            &"old history ".repeat(if mode == "automatic" { 7800 } else { 1000 }),
        );
        let mut generation = Generation::default();
        generation.compaction.buffer = if mode == "overflow" { 0 } else { 499_000 };
        generation
            .permissions
            .insert("bash".into(), Permission::Allow);
        let runtime = runtime(&db, project.path(), generation);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = config(&listener);
        let cancel = AtomicBool::new(mode == "precancel");
        let (held_tx, held_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if mode == "precancel" {
                assert!(
                    tokio::time::timeout(Duration::from_millis(60), listener.accept())
                        .await
                        .is_err()
                );
                return;
            }
            if mode == "overflow" {
                let (mut socket, _) = read_request(&listener).await;
                let event = serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"function_call","id":"fc","call_id":"once","name":"bash","arguments":"{\"argv\":[\"/bin/sh\",\"-c\",\"printf x >> effect\"]}"}]}});
                respond(&mut socket, &format!("data: {event}\n\n")).await;
                let (mut socket, _) = read_request(&listener).await;
                respond(&mut socket,"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"context_length_exceeded\"}}}\n\n").await;
            }
            let (socket, body) = read_request(&listener).await;
            assert_eq!(body["tools"], serde_json::json!([]));
            held_tx.send(()).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(socket);
            assert!(
                tokio::time::timeout(Duration::from_millis(30), listener.accept())
                    .await
                    .is_err()
            );
        });
        let mut catalog = catalog();
        if mode == "automatic" {
            catalog.models.get_mut("m").unwrap()["limit"] =
                serde_json::json!({"context":50_000,"output":1000});
        }
        let operation = runtime.run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1024,
            provider,
            cancel: &cancel,
            max_rounds: 4,
        });
        let cancelling = async {
            if mode != "precancel" {
                held_rx.await.unwrap();
                cancel.store(true, Ordering::Relaxed);
            }
        };
        let (result, ()) = tokio::join!(operation, cancelling);
        assert_eq!(result.unwrap().status, TurnStatus::Cancelled);
        assert!(db.session_checkpoint("s").unwrap().is_none());
        assert!(!runtime.compaction_active());
        if mode != "precancel" {
            assert_eq!(
                db.compaction_history("s").unwrap()[0].state,
                CompactionState::Cancelled
            );
        }
        if mode == "overflow" {
            assert_eq!(std::fs::read(project.path().join("effect")).unwrap(), b"x");
            assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
        }
        task.await.unwrap();
    }
}

#[tokio::test]
async fn compaction_manual_plain_canonical_text_is_independent_of_caller_cancel() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "archive");
    let runtime = runtime(&db, project.path(), Generation::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (mut socket, _) = read_request(&listener).await;
        let body = format!(
            "data: {}\n\ndata: {}\n\n",
            serde_json::json!({"type":"response.output_text.delta","delta":"partial stream"}),
            serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Objective\nPreserve the completed implementation and next action."}]}]}})
        );
        respond(&mut socket, &body).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        runtime
            .deliver_compaction_bound(
                "s",
                &catalog(),
                "m",
                None,
                &provider,
                Some(&AtomicBool::new(true))
            )
            .await
            .unwrap()
    );
    assert_eq!(
        db.compaction_history("s").unwrap()[0].summary,
        "## Objective\nPreserve the completed implementation and next action."
    );
    task.await.unwrap();
}

#[tokio::test]
async fn compaction_native_model_budget_guard_precedes_paid_capability() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", &"history ".repeat(1000));
    let runtime = runtime(&db, project.path(), Generation::default());
    let native = Arc::new(Native(std::sync::atomic::AtomicUsize::new(0)));
    runtime.register_native_compaction(native.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let mut catalog = catalog();
    catalog.models.get_mut("m").unwrap()["limit"] = serde_json::json!({"context":100,"output":20});
    for model in ["m", "missing"] {
        runtime
            .queue_compaction("s", CompactionReason::Manual)
            .unwrap();
        assert!(
            !runtime
                .deliver_compaction("s", &catalog, model, None, &provider)
                .await
                .unwrap()
        );
    }
    assert_eq!(native.0.load(Ordering::Relaxed), 0);
    assert!(db.session_checkpoint("s").unwrap().is_none());
}

#[tokio::test]
async fn compaction_schema_inclusive_irreducible_mcp_admission_spends_no_summary() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "archive");
    let mut generation = Generation::default();
    generation.compaction.buffer = 20_000;
    let runtime = runtime(&db, project.path(), generation);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut catalog = catalog();
    catalog.models.get_mut("m").unwrap()["limit"] =
        serde_json::json!({"context":20_000,"output":1024});
    let mut attached = McpGeneration::empty(0, Arc::new(tokio::sync::Notify::new()));
    attached.entries = vec![mcp_remote::RegistryEntry {
        namespaced: "test__large".into(),
        server: "test".into(),
        tool: "large".into(),
        description: None,
        input_schema: serde_json::json!({"type":"object","description":"schema ".repeat(15_000)}),
    }];
    let published = runtime.current.read().unwrap().clone();
    let lane = runtime.primary_lane(&published);
    let cancel = AtomicBool::new(false);
    let result = runtime
        .run_turn_inner(
            TurnParams {
                session: "s".into(),
                prompt: "continue".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider: config(&listener),
                cancel: &cancel,
                max_rounds: 1,
            },
            &lane,
            &attached,
            None,
            &mut |_, _| {},
            &mut |_, _| {},
            &mut |_, _| {},
            &mut |_| {},
            &mut |_, _| {},
        )
        .await;
    assert!(matches!(result, Err(RuntimeError::InvalidArgs(_))));
    assert!(db.compaction_history("s").unwrap().is_empty());
    assert!(db.session_checkpoint("s").unwrap().is_none());
    assert_eq!(db.read_history_full("s").unwrap().len(), 2);
    assert!(
        tokio::time::timeout(Duration::from_millis(30), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn compaction_publication_failure_releases_and_recovers_running_and_final() {
    for stage in ["running", "completed"] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        seed(&db, "old", "archive");
        let runtime = runtime(&db, project.path(), Generation::default());
        let native = Arc::new(Native(std::sync::atomic::AtomicUsize::new(0)));
        runtime.register_native_compaction(native.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = config(&listener);
        runtime
            .queue_compaction("s", CompactionReason::Manual)
            .unwrap();
        let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
        // Completed is first saved by the atomic checkpoint transaction; fail
        // the following lifecycle publication, not checkpoint installation.
        conn.execute_batch(&format!("CREATE TABLE publish_count(n INTEGER); INSERT INTO publish_count VALUES(0); CREATE TRIGGER fail_publication BEFORE UPDATE ON session_compactions WHEN json_extract(NEW.snapshot,'$.state')='{stage}' BEGIN UPDATE publish_count SET n=n+1; SELECT CASE WHEN (SELECT n FROM publish_count)>{} THEN RAISE(ABORT,'injected publication failure') END; END;",if stage=="completed" {1} else {0})).unwrap();
        assert!(
            runtime
                .deliver_compaction("s", &catalog(), "m", None, &provider)
                .await
                .is_err()
        );
        assert!(!runtime.compaction_active());
        assert!(runtime.pending_compaction().is_none());
        assert_eq!(
            db.compaction_history("s").unwrap()[0].state,
            CompactionState::Failed
        );
        assert_eq!(
            native.0.load(Ordering::Relaxed),
            usize::from(stage == "completed")
        );
        assert_eq!(
            db.session_checkpoint("s").unwrap().is_some(),
            stage == "completed"
        );
        db.set_pref("test.selection", "new").unwrap();
        db.change_conversation("s", ConversationAction::Undo)
            .unwrap();
        db.change_conversation("s", ConversationAction::Redo)
            .unwrap();
        conn.execute_batch("DROP TRIGGER fail_publication;")
            .unwrap();
        runtime
            .queue_compaction("s", CompactionReason::Manual)
            .unwrap();
        runtime.cancel_compaction("s").unwrap();
        drop(runtime);
        drop(db);
        drop(conn);
        let reopened = Db::open(data.path()).unwrap();
        assert!(
            reopened
                .compaction_history("s")
                .unwrap()
                .iter()
                .any(|snapshot| snapshot.state == CompactionState::Failed)
        );
    }
}
