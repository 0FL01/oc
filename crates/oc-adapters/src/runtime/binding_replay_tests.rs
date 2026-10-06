//! Actual dispatch/reopen/fork regression: settled tools are history, not effects.
use super::*;
use crate::provider::protocol::Protocol;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn owner<'a>(db: &'a Db, project: &std::path::Path) -> Runtime<'a> {
    Runtime::new(
        db,
        "work",
        Generation {
            permissions: BTreeMap::from([("read".into(), Permission::Allow)]),
            ..Default::default()
        },
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

async fn request(listener: &tokio::net::TcpListener) -> (tokio::net::TcpStream, Value) {
    let (mut socket, _) = listener.accept().await.unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let (head, length) = loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0 && bytes.len() < 1_048_576);
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(head) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
            let length = String::from_utf8_lossy(&bytes[..head])
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap();
            break (head + 4, length);
        }
    };
    while bytes.len() < head + length {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&chunk[..n]);
    }
    (
        socket,
        serde_json::from_slice(&bytes[head..head + length]).unwrap(),
    )
}

fn frames(protocol: Protocol, tool: bool) -> String {
    let output = match protocol {
        Protocol::Responses => vec![
            json!({"type":"response.completed","response":{"status":"completed","output":if tool { vec![
            json!({"type":"reasoning","id":"r","summary":[{"type":"summary_text","text":"PUBLIC_PLAN"}],"encrypted_content":"OPAQUE_CANARY"}),
            json!({"type":"function_call","id":"f","call_id":"settled-call","name":"read","arguments":"{\"path\":\"note.txt\"}"})
        ] } else { vec![json!({"type":"message","id":"done","role":"assistant","content":[{"type":"output_text","text":"FINAL_PUBLIC"}]})] }}}),
        ],
        Protocol::Chat => vec![json!({"choices":[{"index":0,"delta":if tool {
            json!({"reasoning_content":"PUBLIC_PLAN","tool_calls":[{"index":0,"id":"settled-call","type":"function","function":{"name":"read","arguments":"{\"path\":\"note.txt\"}"}}]})
        } else { json!({"content":"FINAL_PUBLIC"}) },"finish_reason":if tool {"tool_calls"} else {"stop"}}]})],
        Protocol::Messages => {
            let mut v =
                vec![json!({"type":"message_start","message":{"role":"assistant","usage":{}}})];
            if tool {
                v.extend([
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"PUBLIC_PLAN","signature":"OPAQUE_CANARY"}}),
                    json!({"type":"content_block_stop","index":0}),
                    json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"settled-call","name":"read","input":{"path":"note.txt"}}}),
                    json!({"type":"content_block_stop","index":1}),
                ]);
            } else {
                v.extend([json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"FINAL_PUBLIC"}}), json!({"type":"content_block_stop","index":0})]);
            }
            v.extend([json!({"type":"message_delta","delta":{"stop_reason":if tool {"tool_use"} else {"end_turn"}},"usage":{}}), json!({"type":"message_stop"})]);
            v
        }
    };
    let mut wire = output
        .into_iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect::<String>();
    if protocol == Protocol::Chat {
        wire.push_str("data: [DONE]\n\n");
    }
    wire
}

async fn respond(socket: &mut tokio::net::TcpStream, protocol: Protocol, tool: bool) {
    let wire = frames(protocol, tool);
    socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{wire}", wire.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn go03_chat_and_messages_runtime_retry_records_partial_without_tool_replay() {
    for protocol in [Protocol::Chat, Protocol::Messages] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("note.txt"), "SETTLED_NOTE").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut provider = ResponsesConfig {
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
        provider.wire.protocol = protocol;
        provider.wire.messages_bearer = true;
        let models = models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "m".into(),
                json!({"limit":{"context":500000,"output":2048}}),
            )]),
        };
        let peer = tokio::spawn(async move {
            let (mut socket, first) = request(&listener).await;
            socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            drop(socket);
            let (mut socket, retry) = request(&listener).await;
            assert_eq!(
                first, retry,
                "pre-output retry must retain the captured request"
            );
            respond(&mut socket, protocol, true).await;
            drop(socket);
            let (mut socket, followup) = request(&listener).await;
            assert!(followup.to_string().contains("SETTLED_NOTE"));
            let partial = match protocol {
                Protocol::Chat => vec![json!({"choices":[{"index":0,"delta":{
                    "content":"RECORDED_PARTIAL", "tool_calls":[{"index":0,"id":"unsettled-call","type":"function",
                    "function":{"name":"read","arguments":"{\"path\":"}}]},"finish_reason":null}]})],
                Protocol::Messages => vec![
                    json!({"type":"message_start","message":{"role":"assistant"}}),
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"RECORDED_PARTIAL"}}),
                    json!({"type":"content_block_stop","index":0}),
                    json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"unsettled-call","name":"read","input":{}}}),
                    json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
                ],
                Protocol::Responses => unreachable!(),
            }.iter().map(|value| format!("data: {value}\n\n")).collect::<String>();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{partial}",partial.len()).as_bytes()).await.unwrap();
            drop(socket); // No genuine terminal, so neither partial call can execute.
            let (mut socket, continued) = request(&listener).await;
            let text = continued.to_string();
            assert!(text.contains("RECORDED_PARTIAL") && text.contains("SETTLED_NOTE"));
            assert!(!text.contains("unsettled-call"));
            respond(&mut socket, protocol, false).await;
        });
        let db = Db::open(data.path()).unwrap();
        let runtime = owner(&db, project.path());
        runtime.create_session("retry").unwrap();
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "retry".into(),
                prompt: "read the note".into(),
                invocation: None,
                catalog: &models,
                model_id: "m".into(),
                variant: None,
                max_output: 1000,
                provider: provider.clone(),
                cancel: &cancel,
            })
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(report.calls.len(), 1);
        assert_eq!(db.list_tool_ops("retry").unwrap().len(), 1);
        peer.await.unwrap();
        let original = db.turn_result(&report.turn_id).unwrap().1.unwrap();
        let log: Value = serde_json::from_str(&original).unwrap();
        assert_eq!(log["requests"].as_array().unwrap().len(), 4);
        let failed = log["spans"]
            .as_array()
            .unwrap()
            .iter()
            .find(|span| span["status"] == "failed")
            .unwrap();
        assert_eq!(failed["retry"]["attempt"], 2);
        assert!(failed["error"].as_str().unwrap().contains("incomplete"));
        assert!(original.contains("RECORDED_PARTIAL"));
        let sql = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
        assert_eq!(
            sql.query_row(
                "SELECT count(*) FROM events WHERE kind='generation_dispatched'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            4
        );
        assert_eq!(
            sql.query_row(
                "SELECT count(*) FROM events WHERE kind='retry_scheduled'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        drop(sql);
        drop(runtime);
        drop(db);
        let reopened = Db::open(data.path()).unwrap();
        assert_eq!(
            reopened.turn_result(&report.turn_id).unwrap().1.unwrap(),
            original
        );
        assert_eq!(reopened.list_tool_ops("retry").unwrap().len(), 1);
    }
}

#[tokio::test]
async fn go04_actual_wires_reopen_and_fork_preserve_settled_pairs_without_opaque_or_effect_replay()
{
    for (source, destination) in [
        (Protocol::Responses, Protocol::Messages),
        (Protocol::Messages, Protocol::Chat),
        (Protocol::Chat, Protocol::Responses),
    ] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("note.txt"), "SETTLED_NOTE").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut provider = ResponsesConfig {
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
        provider.wire.protocol = source;
        provider.wire.messages_bearer = true;
        provider.wire.chat.insert(
            "m".into(),
            crate::provider::chat::ChatCompat {
                reasoning_field: Some("reasoning_content".into()),
                ..Default::default()
            },
        );
        let models = models::ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "m".into(),
                json!({"limit":{"context":500_000,"output":4096}}),
            )]),
        };
        let peer = tokio::spawn(async move {
            let (mut socket, _) = request(&listener).await;
            respond(&mut socket, source, true).await;
            let (mut socket, followup) = request(&listener).await;
            assert!(followup.to_string().contains("SETTLED_NOTE"));
            if source != Protocol::Chat {
                assert!(followup.to_string().contains("OPAQUE_CANARY"));
            }
            respond(&mut socket, source, false).await;
            for _ in 0..2 {
                let (mut socket, replay) = request(&listener).await;
                let text = replay.to_string();
                assert!(
                    text.contains("SETTLED_NOTE")
                        && text.contains("settled-call")
                        && text.contains("PUBLIC_PLAN"),
                    "{source:?}->{destination:?}: note={}, call={}, plan={}",
                    text.contains("SETTLED_NOTE"),
                    text.contains("settled-call"),
                    text.contains("PUBLIC_PLAN")
                );
                assert!(
                    !text.contains("OPAQUE_CANARY")
                        && !text.contains("messages_thinking")
                        && !text.contains("reasoning_content")
                );
                respond(&mut socket, destination, false).await;
            }
        });
        let db = Db::open(data.path()).unwrap();
        let runtime = owner(&db, project.path());
        runtime.create_session("s").unwrap();
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "s".into(),
                prompt: "read the note".into(),
                invocation: None,
                catalog: &models,
                model_id: "m".into(),
                variant: None,
                max_output: 1000,
                provider: provider.clone(),
                cancel: &cancel,
            })
            .await
            .unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        let original = db.turn_result(&report.turn_id).unwrap().1.unwrap();
        let journal = TurnLog::from_json(&serde_json::from_str(&original).unwrap()).unwrap();
        // A fork copies the prefix before its user boundary; the completed
        // tool exchange must precede that boundary rather than be excluded.
        let boundary = db
            .accept_turn(
                "boundary",
                "s",
                "boundary",
                "boundary",
                &oc_core::queries::ModelRef {
                    provider: "fixture".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap();
        let mut boundary_log = TurnLog::new("boundary", "m", "fixture");
        boundary_log.user_message = Some(boundary.user_message.clone());
        boundary_log.input = vec![
            InputItem::message(InputRole::User, "boundary"),
            InputItem::message(InputRole::Assistant, "boundary answer"),
        ];
        db.commit_turn(
            "boundary",
            "completed",
            Some(&boundary_log.to_json().to_string()),
            Some("boundary answer"),
        )
        .unwrap();
        let fork = db
            .fork_session("s", &boundary.user_message, "work", "fixture", "{}")
            .unwrap();
        let before = db.read_history_full("s").unwrap();
        assert_eq!(db.list_tool_ops("s").unwrap().len(), 1);
        drop(runtime);
        drop(db);
        let db = Db::open(data.path()).unwrap();
        let runtime = owner(&db, project.path());
        provider.wire.protocol = destination;
        provider.wire.api_model = Some("changed-api-model".into());
        for session in [&fork.session.0, &"s".to_owned()] {
            let report = runtime
                .run_turn(TurnParams {
                    session: session.clone(),
                    prompt: "continue without tools".into(),
                    invocation: None,
                    catalog: &models,
                    model_id: "m".into(),
                    variant: None,
                    max_output: 1000,
                    provider: provider.clone(),
                    cancel: &cancel,
                })
                .await
                .unwrap();
            assert_eq!(report.status, TurnStatus::Completed);
            assert_eq!(db.list_tool_ops(session).unwrap().len(), 1);
        }
        assert_eq!(
            db.turn_result(&journal.turn_id).unwrap().1.as_deref(),
            Some(original.as_str())
        );
        assert_eq!(
            &db.read_history_full("s").unwrap()[..before.len()],
            before.as_slice()
        );
        peer.await.unwrap();
    }
}
