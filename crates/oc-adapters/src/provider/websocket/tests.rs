use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    _root: tempfile::TempDir,
    db: crate::storage::Db,
    project: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let db = crate::storage::Db::open(&root.path().join("data")).unwrap();
        db.create_session("session").unwrap();
        Self {
            _root: root,
            db,
            project,
        }
    }
    fn config(&self, base: &str, subscription: bool) -> ResponsesConfig {
        ResponsesConfig {
            base_url: base.into(),
            api_key: "ACCESS_CANARY".into(),
            timeout: None,
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            set_cache_key: false,
            wire: WireBinding {
                endpoint: Some(
                    crate::endpoint::EndpointBinding::admit(base, true, "fixture").unwrap(),
                ),
                auth_policy: if subscription {
                    crate::auth::AuthPolicy::OAuth
                } else {
                    crate::auth::AuthPolicy::Key
                },
                openai: Some(crate::auth::OpenAiBinding {
                    scope: format!("fixture-{subscription}"),
                    subscription,
                    account: Some("ACCOUNT_CANARY".into()),
                }),
                channels: Some(self.db.response_channels.clone()),
                ..Default::default()
            },
        }
        .with_context(context::RequestContext::capture(&self.db, &self.project, "session").unwrap())
    }
    fn runtime(&self) -> crate::runtime::Runtime<'_> {
        crate::runtime::Runtime::new(
            &self.db,
            "work",
            crate::config::Generation {
                permissions: BTreeMap::from([
                    ("read".into(), crate::config::Permission::Allow),
                    ("subagent".into(), crate::config::Permission::Allow),
                ]),
                compaction: crate::compaction::CompactionConfig {
                    keep_tokens: 0,
                    ..Default::default()
                },
                ..Default::default()
            },
            crate::patch::ProtectedGlobs { patterns: vec![] },
            crate::files::Files::new(&self.project, self.db.root()).unwrap(),
            crate::shell::Shell::new(&self.project).unwrap(),
            BTreeMap::new(),
            crate::tools::ToolRoots {
                project: self.project.clone(),
                data: self.db.root().into(),
            },
            None,
            false,
            crate::dcp_auto::DcpConfig::default(),
        )
        .unwrap()
    }
}

#[derive(Clone, Copy)]
enum Reply {
    Complete,
    Tool,
    ToolRejected,
    Child,
    ChildPolicy,
    Ambiguous,
    Partial,
    Policy,
    Size,
    BadIdentity,
    RejectAppend,
    RejectLimit,
    Extra,
}
struct Peer {
    base: String,
    headers: Arc<Mutex<Vec<reqwest::header::HeaderMap>>>,
    frames: Arc<Mutex<Vec<Value>>>,
    http: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}
impl Peer {
    async fn new(reply: Reply) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let headers = Arc::new(Mutex::new(Vec::new()));
        let frames = Arc::new(Mutex::new(Vec::new()));
        let http = Arc::new(Mutex::new(Vec::new()));
        let recorded_headers = headers.clone();
        let recorded_frames = frames.clone();
        let recorded_http = http.clone();
        let (stop, mut stopping) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                let (tcp, _) = tokio::select! {
                    _ = &mut stopping => break,
                    next = listener.accept() => next.unwrap(),
                };
                let headers = recorded_headers.clone();
                let frames = recorded_frames.clone();
                let http = recorded_http.clone();
                tasks.spawn(async move {
                    let mut tcp = tcp;
                    let mut prefix = [0;4];
                    while tcp.peek(&mut prefix).await.unwrap() < 4 {
                        tokio::task::yield_now().await;
                    }
                    if prefix == *b"POST" {
                        let bytes = http_request(&mut tcp).await;
                        http.lock().unwrap().push(bytes);
                        let body = b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n";
                        tcp.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                        tcp.write_all(body).await.unwrap();
                        return;
                    }
                    let Ok(mut socket) = tokio_tungstenite::accept_hdr_async(tcp, move |request: &tungstenite::handshake::server::Request, mut response: tungstenite::handshake::server::Response| {
                        assert_eq!(request.uri().path(), "/v1/responses");
                        headers.lock().unwrap().push(request.headers().clone());
                        // Deliberately not an attempt-local retry override.
                        response.headers_mut().insert("x-should-retry", reqwest::header::HeaderValue::from_static("true"));
                        Ok(response)
                    }).await else { return; };
                    while let Some(Ok(message)) = socket.next().await {
                        let Message::Text(text) = message else { continue; };
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        let sequence = {
                            let mut frames = frames.lock().unwrap();
                            frames.push(frame);
                            frames.len()
                        };
                        let id = format!("r{sequence}");
                        let event = |value: Value| Message::text(serde_json::to_string_pretty(&value).unwrap());
                        if (matches!(reply,Reply::RejectAppend | Reply::ToolRejected) && sequence == 2) || (matches!(reply,Reply::RejectLimit) && sequence <= 2) {
                            let code = if matches!(reply,Reply::RejectAppend | Reply::ToolRejected) {"previous_response_not_found"} else {"websocket_connection_limit_reached"};
                            socket.send(event(json!({"error":{"code":code,"status":400,"message":"channel continuation rejected"}}))).await.unwrap();
                            break;
                        }
                        if matches!(reply, Reply::Policy) || (matches!(reply, Reply::ChildPolicy) && sequence == 2) {
                            socket.send(event(json!({"error":{"code":"cyber_policy","message":"This content was flagged for possible cybersecurity risk. Review https://platform.openai.com/settings/organization/status-and-access before retrying."}}))).await.unwrap();
                            let _ = socket.close(None).await;
                            break;
                        }
                        if matches!(reply, Reply::Size) {
                            let _ = socket.close(Some(tungstenite::protocol::CloseFrame {code:tungstenite::protocol::frame::coding::CloseCode::Size,reason:"rejected".into()})).await;
                            break;
                        }
                        if matches!(reply, Reply::Ambiguous) { break; }
                        socket.send(event(json!({"type":"response.created","response":{"id":id}}))).await.unwrap();
                        if matches!(reply, Reply::Child | Reply::ChildPolicy) && sequence == 1 {
                            let call = json!({"id":"child-item","type":"function_call","call_id":"child-call","name":"subagent","arguments":"{\"agent\":\"helper\",\"description\":\"child channel\",\"prompt\":\"child task\"}","status":"completed"});
                            let mut added = call.clone();
                            added["arguments"] = "".into();
                            added["status"] = "in_progress".into();
                            socket.send(event(json!({"type":"response.output_item.added","output_index":0,"item":added}))).await.unwrap();
                            socket.send(event(json!({"type":"response.output_item.done","output_index":0,"item":call}))).await.unwrap();
                            socket.send(event(json!({"type":"response.completed","response":{"id":id,"status":"completed","output":[call],"usage":{"input_tokens":4,"output_tokens":2}}}))).await.unwrap();
                            continue;
                        }
                        if matches!(reply, Reply::Tool | Reply::ToolRejected) && sequence == 1 {
                            let reasoning = json!({"id":"reasoning-tool","type":"reasoning","summary":[],"encrypted_content":"EARLY_OPAQUE_CANARY"});
                            socket.send(event(json!({"type":"response.output_item.added","output_index":0,"item":reasoning}))).await.unwrap();
                            socket.send(event(json!({"type":"response.output_item.done","output_index":0,"item":reasoning}))).await.unwrap();
                            let call = json!({"id":"read-item","type":"function_call","call_id":"settled-read","name":"read","arguments":"{\"path\":\"note.txt\"}","status":"completed"});
                            let mut added = call.clone();
                            added["arguments"] = "".into();
                            added["status"] = "in_progress".into();
                            socket.send(event(json!({"type":"response.output_item.added","output_index":1,"item":added}))).await.unwrap();
                            socket.send(event(json!({"type":"response.function_call_arguments.delta","output_index":1,"item_id":"read-item","delta":call["arguments"]}))).await.unwrap();
                            socket.send(event(json!({"type":"response.output_item.done","output_index":1,"item":call}))).await.unwrap();
                            let mut completed_reasoning = reasoning.clone();
                            completed_reasoning["encrypted_content"] = "COMPLETION_ROTATED_CANARY".into();
                            socket.send(event(json!({"type":"response.completed","response":{"id":id,"status":"completed","output":[completed_reasoning,call],"usage":{"input_tokens":4,"output_tokens":2}}}))).await.unwrap();
                            continue;
                        }
                        if matches!(reply, Reply::Partial) {
                            socket.send(event(json!({"type":"response.output_text.delta","delta":"partial"}))).await.unwrap();
                            break;
                        }
                        let mut result = message_output(sequence);
                        if matches!(reply, Reply::Child) && sequence >= 5 {
                            result["content"][0]["text"] = "## Objective\nPreserve completed child result.\n## Next Move\nContinue.".into();
                        }
                        let mut added = result.clone();
                        added["status"] = "in_progress".into();
                        added["content"] = json!([]);
                        socket.send(event(json!({"type":"response.output_item.added","output_index":0,"item":added}))).await.unwrap();
                        socket.send(event(json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"item_id":result["id"],"delta":result["content"][0]["text"]}))).await.unwrap();
                        socket.send(event(json!({"type":"response.output_item.done","output_index":0,"item":result}))).await.unwrap();
                        let completed = event(json!({"type":"response.completed","response":{"id":if matches!(reply,Reply::BadIdentity) {"alien"} else {&id},"status":"completed","output":[result],"usage":{"input_tokens":4,"output_tokens":2}}}));
                        if matches!(reply,Reply::Extra) {
                            socket.feed(completed).await.unwrap();
                            socket.feed(event(json!({"type":"unsolicited-data"}))).await.unwrap();
                            socket.flush().await.unwrap();
                            break;
                        }
                        socket.send(completed).await.unwrap();
                    }
                });
            }
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
        });
        Self {
            base,
            headers,
            frames,
            http,
            stop,
            task,
        }
    }
    async fn shutdown(self) {
        let _ = self.stop.send(());
        self.task.await.unwrap();
    }
}
fn message_output(sequence: usize) -> Value {
    json!({"id":format!("o{sequence}"),"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":format!("answer-{sequence}"),"annotations":[]}]})
}

#[tokio::test]
async fn auth04_runtime_channels_settle_read_followup_fork_and_partition_opaque_authority() {
    use crate::runtime::{TurnParams, TurnStatus};

    for subscription in [false, true] {
        let f = Fixture::new();
        std::fs::write(f.project.join("note.txt"), "SETTLED_NOTE").unwrap();
        let peer = Peer::new(Reply::Tool).await;
        // This is a captured native-binding fixture, not an issuer override or
        // a real OpenAI account. The runtime still owns the actual tool effect.
        let config = f.config(&peer.base, subscription);
        let catalog = crate::models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "gpt-5.5".into(),
                json!({"limit":{"context":500000,"output":2048}}),
            )]),
        };
        let runtime = f.runtime();
        runtime.create_session("runtime").unwrap();
        let cancel = AtomicBool::new(false);
        let params = |session: &str, provider: ResponsesConfig| TurnParams {
            session: session.into(),
            prompt: "read the note once".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "gpt-5.5".into(),
            variant: None,
            max_output: 1000,
            provider,
            cancel: &cancel,
        };
        let report = runtime
            .run_turn(params("runtime", config.clone()))
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(report.calls.len(), 1);
        assert_eq!(report.text, "answer-2");
        assert_eq!(f.db.list_tool_ops("runtime").unwrap().len(), 1);
        let frames = peer.frames.lock().unwrap().clone();
        assert_eq!(frames.len(), 2);
        assert!(
            frames[0]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|tool| tool["name"] == "read")
        );
        assert!(frames[1].to_string().contains("SETTLED_NOTE"));
        assert!(!frames[1].to_string().contains("COMPLETION_ROTATED_CANARY"));
        let raw = f.db.turn_result(&report.turn_id).unwrap().1.unwrap();
        assert!(raw.contains("EARLY_OPAQUE_CANARY") && !raw.contains("COMPLETION_ROTATED_CANARY"));
        let log: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(log["requests"].as_array().unwrap().len(), 2);
        assert!(
            log["requests"]
                .as_array()
                .unwrap()
                .iter()
                .all(|request| request["binding"]["auth_scope"].is_string())
        );
        assert!(peer.http.lock().unwrap().is_empty());
        let headers = peer.headers.lock().unwrap().clone();
        assert_eq!(headers.len(), 1);
        if subscription {
            assert_eq!(headers[0]["session-id"], "runtime");
        }

        let boundary =
            f.db.accept_turn(
                "boundary",
                "runtime",
                "boundary",
                "boundary",
                &oc_core::queries::ModelRef {
                    provider: "fixture".into(),
                    id: "gpt-5.5".into(),
                    variant: None,
                },
            )
            .unwrap();
        let mut boundary_log = crate::tools::TurnLog::new("boundary", "gpt-5.5", "fixture");
        boundary_log.user_message = Some(boundary.user_message.clone());
        boundary_log.input = vec![
            InputItem::message(InputRole::User, "boundary"),
            InputItem::message(InputRole::Assistant, "boundary answer"),
        ];
        f.db.commit_turn(
            "boundary",
            "completed",
            Some(&boundary_log.to_json().to_string()),
            Some("boundary answer"),
        )
        .unwrap();
        let fork =
            f.db.fork_session("runtime", &boundary.user_message, "work", "fixture", "{}")
                .unwrap();
        runtime
            .run_turn(params(&fork.session.0, config.clone()))
            .await
            .unwrap();
        assert_eq!(f.db.list_tool_ops("runtime").unwrap().len(), 1);
        assert_eq!(
            f.db.list_tool_ops(&fork.session.0).unwrap().len(),
            1,
            "copied settled receipt, no new effect"
        );
        let fork_frame = peer.frames.lock().unwrap()[2].clone();
        assert!(
            fork_frame.get("previous_response_id").is_none(),
            "fork has its own channel"
        );
        assert!(fork_frame.to_string().contains("SETTLED_NOTE"));

        let mut other = config;
        other.api_key = "OTHER_ACCESS_CANARY".into();
        other.wire.openai.as_mut().unwrap().scope = "other-local-account".into();
        runtime.run_turn(params("runtime", other)).await.unwrap();
        let other_frame = peer.frames.lock().unwrap()[3].clone();
        assert!(other_frame.get("previous_response_id").is_none());
        assert!(!other_frame.to_string().contains("EARLY_OPAQUE_CANARY"));
        assert!(other_frame.to_string().contains("SETTLED_NOTE"));
        assert_eq!(f.db.list_tool_ops("runtime").unwrap().len(), 1);
        assert_eq!(
            f.db.turn_result(&report.turn_id).unwrap().1.unwrap(),
            raw,
            "raw receipt is immutable"
        );
        f.db.response_channels.shutdown().await.unwrap();
        peer.shutdown().await;
    }
}

#[tokio::test]
async fn auth04_runtime_channel_recovery_is_counted_and_delivered_failures_do_not_replay() {
    use crate::runtime::{TurnParams, TurnStatus};
    for reply in [
        Reply::ToolRejected,
        Reply::Partial,
        Reply::Policy,
        Reply::Ambiguous,
    ] {
        let f = Fixture::new();
        std::fs::write(f.project.join("note.txt"), "SETTLED_NOTE").unwrap();
        let runtime = f.runtime();
        runtime.create_session("runtime").unwrap();
        let peer = Peer::new(reply).await;
        let catalog = crate::models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "gpt-5.5".into(),
                json!({"limit":{"context":500000,"output":2048}}),
            )]),
        };
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "runtime".into(),
                prompt: "read the note once".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "gpt-5.5".into(),
                variant: None,
                max_output: 1000,
                provider: f.config(&peer.base, true),
                cancel: &cancel,
            })
            .await
            .unwrap();
        let raw = f.db.turn_result(&report.turn_id).unwrap().1.unwrap();
        let log: Value = serde_json::from_str(&raw).unwrap();
        let frames = peer.frames.lock().unwrap().clone();
        if matches!(reply, Reply::ToolRejected) {
            assert_eq!(report.status, TurnStatus::Completed);
            assert_eq!(report.calls.len(), 1);
            assert_eq!(f.db.list_tool_ops("runtime").unwrap().len(), 1);
            assert_eq!(
                frames.len(),
                3,
                "one affirmative rejection, one existing-owner retry"
            );
            assert_eq!(log["requests"].as_array().unwrap().len(), 3);
            assert!(frames[2].get("previous_response_id").is_none());
            assert!(frames[2].to_string().contains("SETTLED_NOTE"));
        } else {
            assert_eq!(report.status, TurnStatus::Failed);
            assert_eq!(
                frames.len(),
                1,
                "delivered channel failures cannot dispatch again"
            );
            assert_eq!(log["requests"].as_array().unwrap().len(), 1);
            assert!(f.db.list_tool_ops("runtime").unwrap().is_empty());
            assert!(
                log["spans"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|span| span["retry"].is_null())
            );
            if matches!(reply, Reply::Policy) {
                assert!(
                    raw.contains("content policy")
                        && raw.contains(
                            "https://platform.openai.com/settings/organization/status-and-access"
                        )
                );
                assert!(!raw.contains("ACCESS_CANARY") && !raw.contains("ACCOUNT_CANARY"));
            }
        }
        let sql = rusqlite::Connection::open(f.db.root().join("oc.sqlite")).unwrap();
        let dispatched: i64 = sql
            .query_row(
                "SELECT count(*) FROM events WHERE kind='generation_dispatched'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(dispatched, frames.len() as i64);
        let retries: i64 = sql
            .query_row(
                "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retries, i64::from(matches!(reply, Reply::ToolRejected)));
        if retries != 0 {
            let attempt: i64 = sql.query_row("SELECT json_extract(payload,'$.retry.attempt') FROM events WHERE kind='retry_scheduled'", [], |row| row.get(0)).unwrap();
            assert_eq!(attempt, 2, "first scheduled retry is physical attempt two");
        }
        assert!(peer.http.lock().unwrap().is_empty());
        f.db.response_channels.shutdown().await.unwrap();
        peer.shutdown().await;
    }
}

#[tokio::test]
async fn auth04_runtime_child_and_summary_channels_keep_actual_actors_and_restricted_outcomes() {
    use crate::runtime::{SubagentAgent, SubagentCatalog, TurnParams, TurnStatus};
    for reply in [Reply::Child, Reply::ChildPolicy] {
        let f = Fixture::new();
        let runtime = f.runtime();
        runtime
            .publish_subagents(Some(SubagentCatalog {
                depth_limit: 1,
                agents: BTreeMap::from([(
                    "helper".into(),
                    SubagentAgent {
                        id: "helper".into(),
                        description: "child channel".into(),
                        primary: false,
                        model: None,
                        variant: None,
                        prompt: "Child channel fixture.".into(),
                        permissions: Default::default(),
                        permission_rules: Default::default(),
                        hidden: false,
                        digest: Some("helper-digest".into()),
                        request: Default::default(),
                        color: None,
                    },
                )]),
            }))
            .unwrap();
        runtime.create_session("runtime").unwrap();
        let peer = Peer::new(reply).await;
        let config = f.config(&peer.base, true);
        let catalog = crate::models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "gpt-5.5".into(),
                json!({"limit":{"context":500000,"output":2048}}),
            )]),
        };
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "runtime".into(),
                prompt: "delegate the child task".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "gpt-5.5".into(),
                variant: None,
                max_output: 1000,
                provider: config.clone(),
                cancel: &cancel,
            })
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(report.calls.len(), 1);
        let children = f.db.children_of("runtime").unwrap();
        assert_eq!(children.len(), 1);
        let child = &children[0];
        let headers = peer.headers.lock().unwrap().clone();
        assert_eq!(
            headers.len(),
            2,
            "root and actual child have separate channels"
        );
        assert_eq!(headers[0]["session-id"], "runtime");
        assert_eq!(headers[1]["session-id"], child.as_str());
        assert!(
            headers
                .iter()
                .all(|header| header["chatgpt-account-id"] == "ACCOUNT_CANARY"
                    && header["authorization"] == "Bearer ACCESS_CANARY")
        );
        let frames = peer.frames.lock().unwrap().clone();
        assert_eq!(frames.len(), 3);
        assert!(frames[1].get("previous_response_id").is_none());
        assert!(frames[1].to_string().contains("child task"));
        assert!(frames[2].to_string().contains("child-call"));
        let child_history = f.db.read_history_full(child).unwrap();
        let sql = rusqlite::Connection::open(f.db.root().join("oc.sqlite")).unwrap();
        let child_status: String = sql
            .query_row(
                "SELECT status FROM turns WHERE session_id=?1",
                [child],
                |row| row.get(0),
            )
            .unwrap();
        if matches!(reply, Reply::ChildPolicy) {
            assert_eq!(child_status, "failed");
            assert!(report.calls[0].output.contains("content policy"));
            assert!(
                report.calls[0].output.contains(
                    "https://platform.openai.com/settings/organization/status-and-access"
                )
            );
            assert!(!report.calls[0].output.contains("ACCESS_CANARY"));
        } else {
            assert_eq!(child_status, "completed");
            assert!(!child_history.is_empty());
            runtime
                .run_turn(TurnParams {
                    session: "runtime".into(),
                    prompt: "next root boundary".into(),
                    invocation: None,
                    catalog: &catalog,
                    model_id: "gpt-5.5".into(),
                    variant: None,
                    max_output: 1000,
                    provider: config.clone(),
                    cancel: &cancel,
                })
                .await
                .unwrap();
            runtime
                .queue_compaction("runtime", oc_core::compaction::CompactionReason::Manual)
                .unwrap();
            assert!(
                runtime
                    .deliver_compaction("runtime", &catalog, "gpt-5.5", None, &config)
                    .await
                    .unwrap()
            );
            let frames = peer.frames.lock().unwrap().clone();
            assert_eq!(frames.len(), 5);
            assert_eq!(frames[4]["tools"], json!([]));
            assert!(
                frames[4].get("previous_response_id").is_none(),
                "summary invariants differ from the primary request"
            );
            let lane: String = sql.query_row("SELECT json_extract(payload,'$.lane') FROM events WHERE kind='generation_dispatched' ORDER BY rowid DESC LIMIT 1", [], |row| row.get(0)).unwrap();
            assert_eq!(lane, "compaction");
        }
        assert!(peer.http.lock().unwrap().is_empty());
        runtime.child_jobs.shutdown().await.unwrap();
        f.db.response_channels.shutdown().await.unwrap();
        peer.shutdown().await;
    }
}
async fn request(
    config: &ResponsesConfig,
    input: &[InputItem],
    cancel: &AtomicBool,
    sends: &std::sync::atomic::AtomicUsize,
) -> Result<Generation, ProviderError> {
    stream_input_counted(
        config,
        "gpt-5.5",
        None,
        input,
        &[],
        64,
        cancel,
        &mut |_| {},
        &mut || async {
            sends.fetch_add(1, Ordering::SeqCst);
            Ok(())
        },
    )
    .await
}

#[tokio::test]
async fn auth04_channels_default_key_oauth_append_affinity_and_joined_shutdown() {
    let f = Fixture::new();
    let peer = Peer::new(Reply::Complete).await;
    let mut config = f.config(&peer.base, false);
    let sends = std::sync::atomic::AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let mut input = vec![InputItem::message(InputRole::User, "first")];
    let first = request(&config, &input, &cancel, &sends).await.unwrap();
    assert_eq!(first.text, "answer-1");
    assert_eq!(first.usage, Some((4, 2)));
    input.push(InputItem::ProviderOutput(first.output[0].clone()));
    input.push(InputItem::message(InputRole::User, "second"));
    request(&config, &input, &cancel, &sends).await.unwrap();
    assert_eq!(peer.headers.lock().unwrap().len(), 1);
    let frames = peer.frames.lock().unwrap().clone();
    assert_eq!(frames[0]["type"], "response.create");
    assert!(frames[0].get("stream").is_none());
    assert_eq!(frames[1]["previous_response_id"], "r1");
    assert_eq!(frames[1]["input"].as_array().unwrap().len(), 1);
    assert_eq!(frames[0]["max_output_tokens"], 64);
    config = f.config(&peer.base, true);
    request(&config, &input, &cancel, &sends).await.unwrap();
    let headers = peer.headers.lock().unwrap().clone();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0]["openai-beta"], BETA);
    assert!(headers[0].get("chatgpt-account-id").is_none());
    assert_eq!(headers[1]["chatgpt-account-id"], "ACCOUNT_CANARY");
    assert_eq!(headers[1]["session-id"], "session");
    assert_eq!(headers[1]["originator"], "opencode");
    assert!(
        peer.frames.lock().unwrap()[2]
            .get("previous_response_id")
            .is_none()
    );
    let owner = f.db.response_channels.slot("session").unwrap();
    owner.lock().await.connection.as_mut().unwrap().opened -= ROTATE;
    request(&config, &input, &cancel, &sends).await.unwrap();
    assert_eq!(peer.headers.lock().unwrap().len(), 3);
    assert_eq!(sends.load(Ordering::SeqCst), 4);
    f.db.response_channels.shutdown().await.unwrap();
    assert!(matches!(
        request(&config, &input, &cancel, &sends).await,
        Err(ProviderError::Cancelled)
    ));
    assert!(!format!("{config:?} {:?}", f.db.response_channels).contains("CANARY"));
    peer.shutdown().await;
}

#[tokio::test]
async fn auth04_delivered_channel_loss_policy_and_identity_never_replay_http() {
    for reply in [
        Reply::Ambiguous,
        Reply::Partial,
        Reply::Policy,
        Reply::BadIdentity,
    ] {
        let f = Fixture::new();
        let peer = Peer::new(reply).await;
        let sends = std::sync::atomic::AtomicUsize::new(0);
        let error = request(
            &f.config(&peer.base, true),
            &[],
            &AtomicBool::new(false),
            &sends,
        )
        .await
        .unwrap_err();
        match reply {
            Reply::Policy => {
                let ProviderError::Request(failure) = error else {
                    panic!("typed policy failure required")
                };
                assert_eq!(failure.kind, FailureKind::ContentPolicy);
                assert_eq!(failure.http_status, Some(101));
                assert_eq!(failure.headers, RetryHeaders::default());
                assert!(!failure.retry_eligible());
                assert!(failure.message.unwrap().contains(
                    "https://platform.openai.com/settings/organization/status-and-access"
                ));
            }
            Reply::BadIdentity => assert!(matches!(
                error,
                ProviderError::OutputStructure {
                    code: OutputCode::IdentityConflict,
                    ..
                }
            )),
            _ => {
                let ProviderError::Request(failure) = error else {
                    panic!("typed delivery failure required")
                };
                assert_eq!(failure.http_status, Some(101));
                assert!(!failure.retry_eligible());
                assert_eq!(failure.output_committed, matches!(reply, Reply::Partial));
                let mut http = *failure;
                http.http_status = Some(200);
                assert!(
                    http.retry_eligible(),
                    "unchanged HTTP continuation eligibility"
                );
            }
        }
        assert_eq!(sends.load(Ordering::SeqCst), 1);
        assert_eq!(peer.headers.lock().unwrap().len(), 1);
        assert_eq!(peer.frames.lock().unwrap().len(), 1);
        assert!(peer.http.lock().unwrap().is_empty());
        f.db.response_channels.shutdown().await.unwrap();
        peer.shutdown().await;
    }
}

async fn http_request(socket: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let mut buf = [0; 4096];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n != 0 && bytes.len() + n < 64 * 1024);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
            let size = String::from_utf8_lossy(&bytes[..end])
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= end + 4 + size {
                return bytes;
            }
        }
    }
}

#[tokio::test]
async fn auth04_continuation_rejection_has_one_owner_managed_full_recovery() {
    for reply in [Reply::RejectAppend, Reply::RejectLimit] {
        let f = Fixture::new();
        let peer = Peer::new(reply).await;
        let config = f.config(&peer.base, true);
        let sends = std::sync::atomic::AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        let mut input = vec![InputItem::message(InputRole::User, "first")];
        if matches!(reply, Reply::RejectAppend) {
            let first = request(&config, &input, &cancel, &sends).await.unwrap();
            input.push(InputItem::ProviderOutput(first.output[0].clone()));
            input.push(InputItem::message(InputRole::User, "next"));
        }
        let ProviderError::Request(rejected) =
            request(&config, &input, &cancel, &sends).await.unwrap_err()
        else {
            panic!("typed rejection required")
        };
        assert_eq!(rejected.delivery, Delivery::Rejected);
        assert_eq!(rejected.http_status, Some(101));
        assert_eq!(rejected.event_status, Some(400));
        assert!(rejected.retry_eligible());
        assert_eq!(rejected.headers, RetryHeaders::default());
        // This explicit subsequent caller attempt represents the one existing
        // runtime allowance. The adapter itself has issued no hidden retry.
        let next = request(&config, &input, &cancel, &sends).await;
        if matches!(reply, Reply::RejectAppend) {
            assert_eq!(next.unwrap().text, "answer-3");
            assert!(
                peer.frames.lock().unwrap()[1]
                    .get("previous_response_id")
                    .is_some()
            );
            assert!(
                peer.frames.lock().unwrap()[2]
                    .get("previous_response_id")
                    .is_none()
            );
        } else {
            let ProviderError::Request(failure) = next.unwrap_err() else {
                panic!("typed repeated rejection required")
            };
            assert!(!failure.retry_eligible());
        }
        assert!(peer.http.lock().unwrap().is_empty());
        assert_eq!(
            sends.load(Ordering::SeqCst),
            if matches!(reply, Reply::RejectAppend) {
                3
            } else {
                2
            }
        );
        f.db.response_channels.shutdown().await.unwrap();
        peer.shutdown().await;
    }
}

#[tokio::test]
async fn auth04_cancellation_during_decode_never_installs_append_checkpoint() {
    let f = Fixture::new();
    let peer = Peer::new(Reply::Complete).await;
    let config = f.config(&peer.base, false);
    let cancel = AtomicBool::new(false);
    let result = stream_input_counted(
        &config,
        "gpt-5.5",
        None,
        &[],
        &[],
        64,
        &cancel,
        &mut |item| {
            if matches!(item, StreamItem::TextDelta(_)) {
                cancel.store(true, Ordering::SeqCst);
            }
        },
        &mut || async { Ok(()) },
    )
    .await;
    assert!(matches!(result, Err(ProviderError::Cancelled)));
    let slot = f.db.response_channels.slot("session").unwrap();
    assert!(slot.lock().await.connection.is_none());
    assert_eq!(peer.frames.lock().unwrap().len(), 1);
    f.db.response_channels.shutdown().await.unwrap();
    peer.shutdown().await;
}

#[test]
fn auth04_append_proof_uses_canonical_complete_pairs_and_rejects_changed_window() {
    let full = json!({"type":"response.create","model":"gpt-5.5","input":[{"role":"user","content":[{"type":"input_text","text":"first"}]}],"tools":[],"max_output_tokens":64});
    let output = vec![
        json!({"id":"reasoning-a","type":"reasoning","summary":[],"encrypted_content":"cipher-a"}),
        json!({"id":"call-item-a","type":"function_call","call_id":"call-a","name":"read","arguments":"{\"a\":1,\"b\":2}"}),
        message_output(1),
    ];
    let proof = checkpoint(&full, &output, Some("r1")).unwrap();
    let mut next = full.clone();
    let input = next["input"].as_array_mut().unwrap();
    input.extend(output);
    input[1]["id"] = "new-reasoning-id".into();
    input[2]["arguments"] = "{ \"b\": 2, \"a\": 1 }".into();
    input[3]["id"] = "new-message-id".into();
    input[3]["content"][0]["annotations"] = json!([{"ignored":true}]);
    input.push(json!({"type":"function_call_output","call_id":"call-a","output":"result"}));
    let delta = incremental(&next, Some(&proof)).unwrap();
    assert_eq!(delta["previous_response_id"], "r1");
    assert_eq!(delta["input"].as_array().unwrap().len(), 1);
    let mut changed = next.clone();
    changed["model"] = "other".into();
    assert!(incremental(&changed, Some(&proof)).is_none());
    changed = next.clone();
    changed["input"][1]["encrypted_content"] = "alien-cipher".into();
    assert!(incremental(&changed, Some(&proof)).is_none());
    changed = next.clone();
    changed["input"].as_array_mut().unwrap().pop();
    assert!(incremental(&changed, Some(&proof)).is_none());
    changed["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"type":"compaction_trigger"}));
    assert!(checkpoint(&changed, &[], Some("r2")).is_none());
    let sent = "{\"type\":\"response.create\"}";
    assert!(affirmatively_not_sent(
        &tungstenite::Error::WriteBufferFull(Box::new(Message::text(sent))),
        sent
    ));
    assert!(!affirmatively_not_sent(
        &tungstenite::Error::WriteBufferFull(Box::new(Message::text("different"))),
        sent
    ));
    assert!(!affirmatively_not_sent(
        &tungstenite::Error::ConnectionClosed,
        sent
    ));
}

#[tokio::test]
async fn auth04_queued_data_and_dispatch_deadline_withhold_checkpoint_and_send() {
    let f = Fixture::new();
    let peer = Peer::new(Reply::Extra).await;
    let config = f.config(&peer.base, false);
    let sends = std::sync::atomic::AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let result = request(&config, &[], &cancel, &sends).await;
    assert!(matches!(result, Err(ProviderError::InvalidOutput)));
    assert!(
        f.db.response_channels
            .slot("session")
            .unwrap()
            .lock()
            .await
            .connection
            .is_none()
    );
    assert!(peer.http.lock().unwrap().is_empty());
    peer.shutdown().await;
    let peer = Peer::new(Reply::Complete).await;
    let mut config = f.config(&peer.base, false);
    config.wire.total_timeout_ms = Some(100);
    let result = stream_input_counted(
        &config,
        "gpt-5.5",
        None,
        &[],
        &[],
        64,
        &cancel,
        &mut |_| {},
        &mut || std::future::pending::<Result<(), ProviderError>>(),
    )
    .await;
    let Err(ProviderError::Request(failure)) = result else {
        panic!("deadline facts required")
    };
    assert_eq!(failure.delivery, Delivery::NotSent);
    assert_eq!(failure.transport, Some(TransportKind::Deadline));
    assert_eq!(failure.http_status, Some(101));
    assert!(failure.retry_eligible());
    assert!(peer.frames.lock().unwrap().is_empty());
    f.db.response_channels.shutdown().await.unwrap();
    peer.shutdown().await;
}

#[tokio::test]
async fn auth04_affirmative_size_rejection_falls_back_once_without_changing_authority() {
    let f = Fixture::new();
    let peer = Peer::new(Reply::Size).await;
    let config = f.config(&peer.base, true);
    let sends = std::sync::atomic::AtomicUsize::new(0);
    let input = [InputItem::message(InputRole::User, "same captured input")];
    request(&config, &input, &AtomicBool::new(false), &sends)
        .await
        .unwrap();
    request(&config, &input, &AtomicBool::new(false), &sends)
        .await
        .unwrap();
    assert_eq!(peer.headers.lock().unwrap().len(), 1);
    assert_eq!(peer.frames.lock().unwrap().len(), 1);
    let http = peer.http.lock().unwrap().clone();
    assert_eq!(http.len(), 2);
    for bytes in http {
        let end = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let headers = String::from_utf8_lossy(&bytes[..end]);
        assert!(headers.contains("Bearer ACCESS_CANARY") && headers.contains("ACCOUNT_CANARY"));
        let value: Value = serde_json::from_slice(&bytes[end + 4..]).unwrap();
        assert_eq!(value["model"], "gpt-5.5");
        assert_eq!(value["stream"], true);
    }
    assert_eq!(sends.load(Ordering::SeqCst), 3);
    f.db.response_channels.shutdown().await.unwrap();
    peer.shutdown().await;
}
#[tokio::test]
async fn auth04_connect_rejection_pins_same_http_body_and_explicit_http_skips_upgrade() {
    let f = Fixture::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = f.config(
        &format!("http://{}/v1", listener.local_addr().unwrap()),
        false,
    );
    let recorded = Arc::new(Mutex::new(Vec::new()));
    let observed = recorded.clone();
    let task = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let bytes = http_request(&mut socket).await;
            let upgrade = bytes.starts_with(b"GET ");
            observed.lock().unwrap().push(bytes);
            if upgrade {
                socket.write_all(b"HTTP/1.1 400 Unsupported\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            } else {
                let body = b"data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n";
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            }
        }
    });
    let sends = std::sync::atomic::AtomicUsize::new(0);
    let input = [InputItem::message(InputRole::User, "captured input")];
    request(&config, &input, &AtomicBool::new(false), &sends)
        .await
        .unwrap();
    request(&config, &input, &AtomicBool::new(false), &sends)
        .await
        .unwrap();
    let mut explicit = config.clone();
    explicit.wire.transport = Some(Transport::Http);
    request(&explicit, &input, &AtomicBool::new(false), &sends)
        .await
        .unwrap();
    task.await.unwrap();
    let recorded = recorded.lock().unwrap().clone();
    assert_eq!(recorded.len(), 4);
    assert!(recorded[0].starts_with(b"GET /v1/responses"));
    let mut prior = None;
    for bytes in &recorded[1..] {
        assert!(bytes.starts_with(b"POST /v1/responses"));
        let end = bytes.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let headers = String::from_utf8_lossy(&bytes[..end]);
        assert!(headers.contains("Bearer ACCESS_CANARY"));
        let value: Value = serde_json::from_slice(&bytes[end + 4..]).unwrap();
        assert_eq!(value["stream"], true);
        assert_eq!(value["model"], "gpt-5.5");
        if let Some(prior) = &prior {
            assert_eq!(&value, prior);
        } else {
            prior = Some(value);
        }
    }
    assert_eq!(sends.load(Ordering::SeqCst), 3);
    f.db.response_channels.shutdown().await.unwrap();
}

#[test]
fn auth04_scoped_error_classification_safe_public_uri_and_no_prose_matching() {
    let headers = reqwest::header::HeaderMap::from_iter([(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static("Bearer ACCESS_CANARY"),
    )]);
    let exact = "This content was flagged for possible cybersecurity risk. Review https://platform.openai.com/settings/organization/status-and-access before retrying.";
    assert_eq!(
        failure::classified(Some(&json!({"error":{"message":exact}})), None, true).kind,
        FailureKind::ContentPolicy
    );
    assert_eq!(
        failure::classified(
            Some(&json!({"error":{"code":"cyber_policy"}})),
            Some(500),
            false
        )
        .kind,
        FailureKind::ContentPolicy
    );
    assert_ne!(
        failure::classified(Some(&json!({"error":{"message":exact}})), Some(500), false).kind,
        FailureKind::ContentPolicy
    );
    assert_ne!(failure::classified(Some(&json!({"message":"This content was flagged for possible", "error":{"message":"cybersecurity risk"}})),None,true).kind,FailureKind::ContentPolicy);
    assert_eq!(safe_diagnostic(exact, &headers).as_deref(), Some(exact));
    for suffix in ["?key=CANARY", "#CANARY", "/extra", ":443", "--lookalike"] {
        let unsafe_message = exact.replace(" before", &format!("{suffix} before"));
        assert_eq!(
            safe_diagnostic(&unsafe_message, &headers).as_deref(),
            Some("provider diagnostic contained private data (redacted)")
        );
    }
    let redacted = safe_diagnostic(&format!("{exact} ACCESS_CANARY"), &headers).unwrap();
    assert!(!redacted.contains("CANARY"));
    assert!(
        redacted.contains("https://platform.openai.com/settings/organization/status-and-access")
    );
    assert!(redacted.ends_with("[redacted]"));
    assert_eq!(
        safe_diagnostic(&exact.replace("platform", "plat\u{1b}form"), &headers).as_deref(),
        Some("provider diagnostic contained private data (redacted)")
    );
    assert!(
        safe_diagnostic(&"a".repeat(600), &headers)
            .unwrap()
            .ends_with("[clipped]")
    );
    assert!(
        safe_diagnostic(&"a".repeat(5000), &headers)
            .unwrap()
            .contains("omitted")
    );
    let mut parser = SseParser::default();
    parser
        .push(
            format!(
                "data: {}\n\n",
                json!({"type":"response.output_text.delta","delta":exact})
            )
            .as_bytes(),
        )
        .unwrap();
    assert!(
        !parser.terminal,
        "ordinary assistant prose is never provider-error classification input"
    );
}
#[test]
fn auth04_channel_settings_are_typed_and_foreign_channels_are_unready() {
    let entry = |npm: &str, base: &str, transport: serde_json::Value| {
        serde_json::from_value::<crate::config::ProviderEntry>(serde_json::json!({
            "npm":npm,"options":{"baseURL":base,"transport":transport}
        }))
        .unwrap()
    };
    let native = entry(
        "@ai-sdk/openai",
        crate::auth::OPENAI_BASE_URL,
        serde_json::json!("websocket"),
    );
    let wire = crate::config::provider_wire("openai", &native).unwrap();
    assert_eq!(wire.transport, Some(Transport::WebSocket));
    assert!(!wire.unsupported);
    assert!(
        crate::config::provider_wire("foreign", &native)
            .unwrap()
            .unsupported
    );
    let foreign = entry(
        "@ai-sdk/openai",
        "https://example.invalid/v1",
        serde_json::json!("websocket"),
    );
    assert!(
        crate::config::provider_wire("openai", &foreign)
            .unwrap()
            .unsupported
    );
    let http = entry(
        "@ai-sdk/anthropic",
        "https://example.invalid/v1",
        serde_json::json!("http"),
    );
    assert_eq!(
        crate::config::provider_wire("foreign", &http)
            .unwrap()
            .transport,
        Some(Transport::Http)
    );
    for invalid in [serde_json::json!(true), serde_json::json!("automatic")] {
        assert!(
            crate::config::provider_wire(
                "openai",
                &entry("@ai-sdk/openai", crate::auth::OPENAI_BASE_URL, invalid)
            )
            .is_err()
        );
    }
    assert!(
        crate::config::provider_wire(
            "foreign",
            &entry(
                "@ai-sdk/anthropic",
                "https://example.invalid/v1",
                serde_json::json!("websocket")
            )
        )
        .is_err()
    );
}
