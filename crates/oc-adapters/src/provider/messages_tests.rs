use super::*;
use serde_json::json;

fn frames() -> Vec<serde_json::Value> {
    vec![
        json!({"type":"message_start","message":{"id":"msg","role":"assistant","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":7,"cache_creation_input_tokens":3}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"SIGNED"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"читаю"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call","name":"read","input":{}}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"\"a\"}"}}),
        json!({"type":"content_block_stop","index":2}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5}}),
        json!({"type":"message_stop"}),
    ]
}

fn bytes(frames: &[serde_json::Value]) -> Vec<u8> {
    frames
        .iter()
        .map(|v| {
            format!(
                "event: {}\r\ndata: {v}\r\n\r\n",
                v["type"].as_str().unwrap()
            )
        })
        .collect::<String>()
        .into_bytes()
}

#[test]
fn go03_messages_split_stream_tool_reasoning_usage_and_lowering() {
    let wire = bytes(&frames());
    for split in [1, 7, 4096] {
        let mut parser = SseParser::messages();
        let mut items = Vec::new();
        for chunk in wire.chunks(split) {
            items.extend(parser.push(chunk).unwrap());
        }
        parser.finish().unwrap();
        assert!(parser.completed);
        assert_eq!(parser.finish, FinishReason::Stop);
        assert!(items.contains(&StreamItem::Usage {
            input_tokens: 20,
            output_tokens: 5
        }));
        assert!(items.contains(&StreamItem::ReasoningDelta("plan".into())));
        let output = parser.output.unwrap();
        assert_eq!(output[0]["messages_thinking"]["signature"], "SIGNED");
        assert_eq!(output[2]["arguments"], "{\"path\":\"a\"}");
        let mut input = vec![
            InputItem::message(InputRole::Developer, "BASE"),
            InputItem::message(InputRole::User, "read"),
        ];
        input.extend(output.into_iter().map(InputItem::ProviderOutput));
        input.push(InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: "RESULT".into(),
        });
        input.push(InputItem::message(InputRole::System, "new <plan>"));
        let body = request_body(
            "api-id",
            &input,
            &[ToolDef {
                name: "read".into(),
                description: "read".into(),
                parameters: json!({"type":"object"}),
            }],
            444,
            Some("high"),
        )
        .unwrap();
        assert_eq!(body["system"][0]["text"], "BASE");
        assert_eq!(body["messages"][1]["content"][0]["type"], "thinking");
        assert_eq!(body["messages"][1]["content"][2]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call");
        assert_eq!(
            body["messages"][2]["content"][1]["text"],
            "<system-update>\nnew &lt;plan&gt;\n</system-update>"
        );
        assert_eq!(body["max_tokens"], 444);
        assert_eq!(body["output_config"]["effort"], "high");
        assert!(body.get("store").is_none() && body.get("input").is_none());
    }
}

#[test]
fn go03_messages_require_genuine_terminal_and_closed_valid_blocks() {
    for mutation in 0..7 {
        let mut frames = frames();
        match mutation {
            0 => { frames.pop(); }
            1 => { frames.remove(11); }
            2 => frames[10]["delta"]["partial_json"] = "bad".into(),
            3 => frames[12]["delta"]["stop_reason"] = "refusal".into(),
            4 => frames[12]["delta"]["stop_reason"] = "unknown".into(),
            5 => frames[9]["index"] = 90.into(),
            6 => frames.insert(12,json!({"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{}"}})),
            _ => unreachable!(),
        }
        let mut parser = SseParser::messages();
        let result = parser.push(&bytes(&frames)).and_then(|_| parser.finish());
        assert!(result.is_err() || !parser.completed, "mutation {mutation}");
        assert!(parser.output.is_none());
    }
    let mut frames = frames();
    frames[12]["delta"]["stop_reason"] = "max_tokens".into();
    let mut parser = SseParser::messages();
    parser.push(&bytes(&frames)).unwrap();
    assert_eq!(parser.finish, FinishReason::Length);
}

#[test]
fn go03_messages_images_and_alien_reasoning() {
    let input = vec![
        InputItem::message(InputRole::Developer, "BASE"),
        InputItem::Message {
            role: InputRole::User,
            content: vec![InputContent::InputImage {
                image_url: "data:image/png;base64,AA==".into(),
                detail: None,
            }],
        },
        InputItem::ProviderOutput(json!({"type":"reasoning","encrypted_content":"ALIEN"})),
        InputItem::ProviderOutput(
            json!({"type":"reasoning","messages_thinking":{"type":"redacted_thinking","data":"REDACTED"}}),
        ),
    ];
    let body = request_body("m", &input, &[], 100, None).unwrap();
    assert_eq!(
        body["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(
        body["messages"][1]["content"][0]["type"],
        "redacted_thinking"
    );
    assert!(!body.to_string().contains("ALIEN"));
    let bad = vec![InputItem::Message {
        role: InputRole::User,
        content: vec![InputContent::InputImage {
            image_url: "data:application/pdf;base64,AA==".into(),
            detail: None,
        }],
    }];
    assert_eq!(
        request_body("m", &bad, &[], 100, None),
        Err(ProviderError::UnsupportedModality)
    );
}

#[test]
fn go01_messages_auth_is_exclusive_and_debug_is_secret_safe() {
    use crate::config::{ProviderEntry, provider_wire};
    for bearer in [false, true] {
        let key = "SYNTHETIC_SECRET";
        let entry: ProviderEntry = serde_json::from_value(json!({"npm":"@ai-sdk/anthropic", "options": if bearer {json!({"authToken":key})} else {json!({"apiKey":key})}})).unwrap();
        let mut config = super::super::ResponsesConfig {
            base_url: "https://example.invalid/prefix".into(),
            api_key: key.into(),
            timeout: Some(false),
            chunk_timeout_ms: 6000000,
            connect_timeout: std::time::Duration::from_secs(5),
            allow_private: false,
            headers: BTreeMap::new(),
            set_cache_key: false,
            wire: provider_wire("local", &entry).unwrap(),
        };
        config
            .headers
            .insert("Anthropic-Beta".into(), "custom-beta".into());
        let headers = super::super::request_headers(&config).unwrap();
        assert_eq!(headers.contains_key("authorization"), bearer);
        assert_eq!(headers.contains_key("x-api-key"), !bearer);
        assert_eq!(headers["anthropic-version"], "2023-06-01");
        assert_eq!(
            headers["anthropic-beta"],
            "custom-beta,interleaved-thinking-2025-05-14"
        );
        assert!(!format!("{entry:?} {config:?}").contains(key));
    }
    for options in [
        json!({"apiKey":"a","authToken":"b"}),
        json!({"apiKey":"a","headers":{"X-Api-Key":"b"}}),
    ] {
        let entry: ProviderEntry =
            serde_json::from_value(json!({"npm":"@ai-sdk/anthropic","options":options})).unwrap();
        assert!(provider_wire("local", &entry).is_err());
    }
    let mut overlay = super::super::RequestOverlay::default();
    overlay.headers.insert("X-Api-Key".into(), "b".into());
    assert!(overlay.validate().is_err());
}

#[tokio::test]
async fn go03_messages_transport_failure_and_cancel_share_one_attempt_owner() {
    use super::super::{
        FailureKind, PhysicalFailure, RequestOverlay, ResponsesConfig, WireBinding,
        stream_input_overlaid,
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for mode in 0..3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config = ResponsesConfig {
            base_url: format!("http://{}/prefix", listener.local_addr().unwrap()),
            api_key: "synthetic".into(),
            timeout: Some(false),
            chunk_timeout_ms: 5000,
            connect_timeout: std::time::Duration::from_secs(2),
            allow_private: true,
            headers: BTreeMap::from([("anthropic-beta".into(), "provider-beta".into())]),
            set_cache_key: false,
            wire: WireBinding::messages(false),
        };
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let server_cancel = cancel.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = [0; 8192];
            let n = socket.read(&mut buf).await.unwrap();
            assert!(String::from_utf8_lossy(&buf[..n]).starts_with("POST /prefix/messages "));
            assert!(
                String::from_utf8_lossy(&buf[..n])
                    .contains("provider-beta,interleaved-thinking-2025-05-14,profile-beta")
            );
            if mode == 0 {
                socket.write_all(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
            } else {
                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
                if mode == 1 {
                    socket.write_all(&bytes(&frames()[..3])).await.unwrap();
                } else {
                    server_cancel.store(true, Ordering::Relaxed);
                    let mut end = [0; 1];
                    let _ = socket.read(&mut end).await;
                }
            }
        });
        let mut attempts = 0;
        let mut dispatch = || {
            attempts += 1;
            async { Ok(()) }
        };
        let result = stream_input_overlaid(
            &config,
            &RequestOverlay {
                headers: BTreeMap::from([("anthropic-beta".into(), "profile-beta".into())]),
                ..Default::default()
            },
            "m",
            None,
            &[InputItem::message(InputRole::User, "hello")],
            &[],
            100,
            &cancel,
            &mut |_| {},
            &mut dispatch,
        )
        .await;
        assert_eq!(attempts, 1);
        match mode {
            0 => assert!(
                matches!(result,Err(ProviderError::Request(f)) if matches!(*f, PhysicalFailure {kind:FailureKind::RateLimit,..}) && f.headers.retry_after_ms == Some(1000))
            ),
            1 => assert!(
                matches!(result,Err(ProviderError::Request(f)) if f.kind == FailureKind::IncompleteStream && f.output_committed)
            ),
            _ => assert_eq!(result, Err(ProviderError::Cancelled)),
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }
}
