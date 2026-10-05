use super::*;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn request(
    listener: &tokio::net::TcpListener,
) -> (tokio::net::TcpStream, String, serde_json::Value) {
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut bytes = Vec::new();
    let mut buf = [0; 4096];
    let boundary = loop {
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
    };
    let headers = String::from_utf8(bytes[..boundary].to_vec())
        .unwrap()
        .to_ascii_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|s| s.strip_prefix("content-length:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    while bytes.len() < boundary + length {
        let n = stream.read(&mut buf).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buf[..n]);
    }
    (
        stream,
        headers,
        serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap(),
    )
}

async fn respond(stream: &mut tokio::net::TcpStream, blocks: &[serde_json::Value], finish: &str) {
    let mut frames = vec![
        json!({"type":"message_start","message":{"role":"assistant","usage":{"input_tokens":20,"output_tokens":0}}}),
    ];
    for (index, block) in blocks.iter().enumerate() {
        frames.push(json!({"type":"content_block_start","index":index,"content_block":block}));
        frames.push(json!({"type":"content_block_stop","index":index}));
    }
    frames.push(
        json!({"type":"message_delta","delta":{"stop_reason":finish},"usage":{"output_tokens":5}}),
    );
    frames.push(json!({"type":"message_stop"}));
    let body = frames
        .iter()
        .map(|f| format!("data: {f}\n\n"))
        .collect::<String>();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn go03_messages_package_runs_complete_tool_roundtrip_for_both_auth_schemes() {
    for bearer in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("note.txt"), "MESSAGES_NOTE").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/custom/prefix", listener.local_addr().unwrap());
        let mut options = json!({"baseURL":base});
        options[if bearer { "authToken" } else { "apiKey" }] = "synthetic-key".into();
        std::fs::write(project.join("opencode.json"),json!({
            "model":"local/messages-model","provider":{"local":{"npm":"@ai-sdk/anthropic","options":options,
            "models":{"messages-model":{"limit":{"context":100000,"output":4000}}}}},"permission":{"*":"allow"}
        }).to_string()).unwrap();
        let note = project.join("note.txt").to_string_lossy().into_owned();
        let server = tokio::spawn(async move {
            let (mut stream, headers, body) = request(&listener).await;
            let mut seen = vec![(headers, body)];
            respond(
                &mut stream,
                &[
                    json!({"type":"thinking","thinking":"plan","signature":"SIGNED"}),
                    json!({"type":"redacted_thinking","data":"REDACTED"}),
                    json!({"type":"tool_use","id":"call_read","name":"read","input":{"path":note}}),
                ],
                "tool_use",
            )
            .await;
            loop {
                let (mut stream, headers, body) = request(&listener).await;
                let title = body["max_tokens"] == 256 && body.get("tools").is_none();
                seen.push((headers, body));
                respond(&mut stream,&[json!({"type":"text","text":if title {"Messages title"} else {"MESSAGES_FINAL"}})],"end_turn").await;
                if !title {
                    return seen;
                }
            }
        });
        let env = BTreeMap::from([
            (
                "HOME".into(),
                root.path().join("home").to_string_lossy().into_owned(),
            ),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        let session = SessionId::new("messages-wire").unwrap();
        app.create_session(session.clone()).await.unwrap();
        app.rename_session(session.clone(), "fixed".into())
            .await
            .unwrap();
        let mut events = app.subscribe();
        app.submit(session, "read note".into()).await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                match events.recv().await.unwrap() {
                    CoreEvent::TurnFinished { .. } => break,
                    CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        let seen = server.await.unwrap();
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        for (headers, body) in &seen {
            assert!(headers.starts_with("post /custom/prefix/messages "));
            assert_eq!(
                headers.contains("authorization: bearer synthetic-key"),
                bearer
            );
            assert_eq!(headers.contains("x-api-key: synthetic-key"), !bearer);
            assert!(headers.contains("anthropic-version: 2023-06-01"));
            assert!(headers.contains("interleaved-thinking-2025-05-14"));
            assert_eq!(body["model"], "messages-model");
            assert!(body.get("store").is_none() && body.get("input").is_none());
        }
        let main: Vec<_> = seen
            .iter()
            .filter(|(_, b)| b.get("tools").is_some())
            .collect();
        assert_eq!(main.len(), 2);
        let follow = main[1].1["messages"].as_array().unwrap();
        let assistant = follow.iter().find(|m| m["role"] == "assistant").unwrap();
        assert_eq!(assistant["content"][0]["signature"], "SIGNED");
        assert_eq!(assistant["content"][1]["data"], "REDACTED");
        assert_eq!(assistant["content"][2]["id"], "call_read");
        let result = follow
            .iter()
            .flat_map(|m| m["content"].as_array().unwrap())
            .find(|b| b["type"] == "tool_result")
            .unwrap();
        assert_eq!(result["tool_use_id"], "call_read");
        assert!(
            result["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("MESSAGES_NOTE")
        );
        assert!(
            Db::open(&data)
                .unwrap()
                .read_history("messages-wire")
                .unwrap()
                .iter()
                .any(|(role, text)| role == "assistant" && text == "MESSAGES_FINAL")
        );
    }
}
