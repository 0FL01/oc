use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Read one HTTP request (path + JSON body) from a raw loopback socket.
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
    let headers = String::from_utf8_lossy(&bytes[..boundary]).to_string();
    let path = headers.split_whitespace().nth(1).unwrap().to_string();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap())
        })
        .unwrap();
    while bytes.len() < boundary + length {
        let n = stream.read(&mut buf).await.unwrap();
        bytes.extend_from_slice(&buf[..n]);
    }
    let body = serde_json::from_slice(&bytes[boundary..boundary + length]).unwrap();
    (stream, path, body)
}

async fn respond(stream: &mut tokio::net::TcpStream, frames: &[serde_json::Value]) {
    let mut body = frames
        .iter()
        .map(|f| format!("data: {f}\n\n"))
        .collect::<String>();
    body.push_str("data: [DONE]\n\n");
    stream
        .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
        .await
        .unwrap();
}

#[tokio::test]
async fn go03_openai_compatible_package_runs_a_complete_chat_tool_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("note.txt"), "CHAT_NOTE").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model": "local/chat-model",
        "provider": {"local": {"npm": "@ai-sdk/openai-compatible", "options": {"baseURL": base, "apiKey": "dummy"},
            "models": {"chat-model": {"limit": {"context": 100000, "output": 4000},
                "compatibility": {"reasoningField": "reasoning_content", "maxTokensField": "max_completion_tokens"}}}}},
        "permission": {"*": "allow"}
    }).to_string()).unwrap();
    let note = project.join("note.txt").to_string_lossy().into_owned();
    let server = tokio::spawn(async move {
        let mut seen = Vec::new();
        let (mut stream, path, body) = request(&listener).await;
        seen.push((path, body));
        respond(&mut stream, &[
            serde_json::json!({"choices":[{"delta":{"reasoning_content":"plan to read"}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_read","function":{"name":"read","arguments":serde_json::json!({"path": note}).to_string()}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":5}}),
        ]).await;
        let mut followed = false;
        loop {
            let (mut stream, path, body) = request(&listener).await;
            let auxiliary =
                body["max_completion_tokens"].as_u64() == Some(256) && body.get("tools").is_none();
            seen.push((path, body));
            if auxiliary {
                respond(&mut stream, &[serde_json::json!({"choices":[{"delta":{"content":"Chat title"},"finish_reason":"stop"}]})]).await;
                continue;
            }
            if !followed {
                followed = true;
                // This response intentionally has no reasoning: the next
                // request must not copy the first response's reasoning onto it.
                respond(&mut stream, &[
                    serde_json::json!({"choices":[{"delta":{"content":"reading again"}}]}),
                    serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_again","function":{"name":"read","arguments":serde_json::json!({"path": note}).to_string()}}]},"finish_reason":"tool_calls"}]}),
                ]).await;
                continue;
            }
            respond(&mut stream, &[serde_json::json!({"choices":[{"delta":{"content":"CHAT_FINAL"},"finish_reason":"stop"}],"usage":{"prompt_tokens":30,"completion_tokens":3}})]).await;
            return seen;
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
    let session = SessionId::new("chat-wire").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "fixed".into())
        .await
        .unwrap();
    let mut events = app.subscribe();
    app.submit(session.clone(), "read the note".into())
        .await
        .unwrap();
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
    let main: Vec<_> = seen
        .iter()
        .filter(|(_, body)| body.get("tools").is_some())
        .collect();
    assert_eq!(main.len(), 3, "first request and two tool follow-ups");
    for (path, body) in &main {
        assert_eq!(path, "/v1/chat/completions");
        assert_eq!(body["model"], "chat-model");
        assert!(body.get("max_completion_tokens").is_some() && body.get("max_tokens").is_none());
        assert!(body.get("input").is_none() && body.get("store").is_none());
        assert_eq!(body["messages"][0]["role"], "system");
    }
    let follow = &main[1].1["messages"];
    let assistant = follow
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "assistant")
        .expect("assistant tool call message");
    assert_eq!(assistant["tool_calls"][0]["id"], "call_read");
    assert_eq!(assistant["reasoning_content"], "plan to read");
    let tool = follow
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["role"] == "tool")
        .expect("tool result");
    assert_eq!(tool["tool_call_id"], "call_read");
    assert!(tool["content"].as_str().unwrap().contains("CHAT_NOTE"));
    let assistants: Vec<_> = main[2].1["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "assistant")
        .collect();
    assert_eq!(assistants.len(), 2);
    assert_eq!(assistants[0]["reasoning_content"], "plan to read");
    assert_eq!(assistants[1]["content"], "reading again");
    assert_eq!(assistants[1]["tool_calls"][0]["id"], "call_again");
    assert!(
        assistants[1].get("reasoning_content").is_none(),
        "wire replay must not invent reasoning for the second response"
    );
    let results: Vec<_> = main[2].1["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "tool")
        .collect();
    assert_eq!(results.len(), 2);
    assert_eq!(results[1]["tool_call_id"], "call_again");
    assert!(
        results[1]["content"]
            .as_str()
            .unwrap()
            .contains("CHAT_NOTE")
    );
    let history = Db::open(&data).unwrap().read_history("chat-wire").unwrap();
    assert!(history.iter().any(|(role, text)| role == "assistant"
        && text.contains("reading again")
        && text.ends_with("CHAT_FINAL")));
}
