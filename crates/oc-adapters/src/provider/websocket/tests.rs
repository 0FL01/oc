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
}

#[derive(Clone, Copy)]
enum Reply {
    Complete,
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
                        if (matches!(reply,Reply::RejectAppend) && sequence == 2) || (matches!(reply,Reply::RejectLimit) && sequence <= 2) {
                            let code = if matches!(reply,Reply::RejectAppend) {"previous_response_not_found"} else {"websocket_connection_limit_reached"};
                            socket.send(event(json!({"error":{"code":code,"status":400,"message":"channel continuation rejected"}}))).await.unwrap();
                            break;
                        }
                        if matches!(reply, Reply::Policy) {
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
                        if matches!(reply, Reply::Partial) {
                            socket.send(event(json!({"type":"response.output_text.delta","delta":"partial"}))).await.unwrap();
                            break;
                        }
                        let result = message_output(sequence);
                        let mut added = result.clone();
                        added["status"] = "in_progress".into();
                        added["content"] = json!([]);
                        socket.send(event(json!({"type":"response.output_item.added","output_index":0,"item":added}))).await.unwrap();
                        socket.send(event(json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"item_id":result["id"],"delta":format!("answer-{sequence}")}))).await.unwrap();
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
