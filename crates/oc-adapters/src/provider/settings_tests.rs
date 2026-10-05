use super::*;

fn admitted(protocol: Protocol, raw: Value, body: Value) -> Result<WireSettings, ProviderError> {
    WireSettings::admit(
        protocol,
        &serde_json::from_value(raw).unwrap(),
        body.as_object().unwrap(),
    )
}

#[test]
fn go03_wire_options_are_typed_and_protocol_owned() {
    for thinking in [
        json!({"type":"enabled","budgetTokens":1024}),
        json!({"type":"enabled","budget_tokens":1024}),
        json!({"type":"adaptive","display":"omitted","block_binding":{"prefix_mismatch_behavior":"drop_block"}}),
        json!({"type":"disabled"}),
    ] {
        let settings = admitted(Protocol::Messages, json!({"thinking":thinking,"outputConfig":{"effort":"future-effort","format":{"type":"json_schema","schema":{"type":"object"}}}}), json!({})).unwrap();
        let mut body = json!({});
        settings.apply(Protocol::Messages, &mut body, Some("high"));
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(body["output_config"]["format"]["schema"]["type"], "object");
        assert_eq!(body["thinking"]["type"], thinking["type"]);
        if thinking["type"] == "enabled" {
            assert_eq!(body["thinking"]["budget_tokens"], 1024);
            assert!(body["thinking"].get("budgetTokens").is_none());
        }
    }
    for bad in [
        json!({"thinking":{"type":"enabled"}}),
        json!({"thinking":{"type":"enabled","budget_tokens":0}}),
        json!({"thinking":{"type":"enabled","budget_tokens":-1}}),
        json!({"thinking":{"type":"enabled","budget_tokens":1,"budgetTokens":1}}),
        json!({"thinking":{"type":"adaptive","budget_tokens":10}}),
        json!({"thinking":{"type":"disabled","display":"omitted"}}),
        json!({"thinking":{"type":"unknown"}}),
        json!({"outputConfig":{"effort":1}}),
        json!({"outputConfig":{"effort":"high"},"output_config":{"effort":"low"}}),
        json!({"outputConfig":{"format":{"type":"json_schema","schema":[]}}}),
        json!({"outputConfig":{"secretCanary":"DO_NOT_ECHO"}}),
    ] {
        let result = admitted(Protocol::Messages, bad.clone(), json!({}));
        assert!(result.is_err(), "accepted invalid synthetic case: {bad}");
        assert!(!format!("{:?}", result.unwrap_err()).contains("DO_NOT_ECHO"));
    }
    for protocol in [Protocol::Chat, Protocol::Responses] {
        assert!(admitted(protocol, json!({"thinking":{"type":"adaptive"}}), json!({})).is_err());
        assert!(admitted(protocol, json!({"output_config":{}}), json!({})).is_err());
    }
    assert!(
        admitted(
            Protocol::Messages,
            json!({"reasoningSummary":"auto"}),
            json!({})
        )
        .is_err()
    );
    assert!(admitted(Protocol::Chat, json!({"textVerbosity":"high"}), json!({})).is_err());
    assert!(admitted(Protocol::Responses, json!({"textVerbosity":1}), json!({})).is_err());
    for field in super::super::RESERVED_BODY_FIELDS {
        let body = json!({field:"DO_NOT_ECHO"});
        assert!(
            admitted(Protocol::Responses, json!({}), body).is_err(),
            "{field}"
        );
    }
}

#[test]
fn go03_responses_settings_and_nested_body_preserve_profile_priority() {
    let settings = admitted(
        Protocol::Responses,
        json!({"reasoningEffort":"high","reasoningSummary":"auto","textVerbosity":"low"}),
        json!({"nested":{"provider":1,"shared":"provider"},"text":{"format":{"type":"text"}}}),
    )
    .unwrap();
    let mut body = json!({"model":"selected","reasoning":{"effort":"medium"}});
    settings.apply(Protocol::Responses, &mut body, Some("medium"));
    merge(
        &mut body,
        &json!({"nested":{"profile":2,"shared":"profile"}}),
    );
    assert_eq!(
        body["reasoning"],
        json!({"effort":"medium","summary":"auto"})
    );
    assert_eq!(
        body["text"],
        json!({"verbosity":"low","format":{"type":"text"}})
    );
    assert_eq!(
        body["nested"],
        json!({"provider":1,"profile":2,"shared":"profile"})
    );
    assert_eq!(body["model"], "selected");
    assert!(!format!("{settings:?}").contains("provider"));
}

#[tokio::test]
async fn go03_captured_options_reach_each_native_wire_with_selected_effort() {
    use super::super::{InputItem, ResponsesConfig, stream_input_overlaid};
    use std::{sync::atomic::AtomicBool, time::Duration};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    for (package, path, options, terminal) in [
        (
            "@ai-sdk/openai",
            "/prefix/responses",
            json!({"reasoningEffort":"high","reasoningSummary":"auto","textVerbosity":"low"}),
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n",
        ),
        (
            "@ai-sdk/openai-compatible",
            "/prefix/chat/completions",
            json!({"reasoningEffort":"high"}),
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ),
        (
            "@ai-sdk/anthropic",
            "/prefix/messages",
            json!({"reasoningEffort":"high","thinking":{"type":"adaptive"},"output_config":{"effort":"medium"}}),
            "data: {\"type\":\"message_start\",\"message\":{\"role\":\"assistant\"}}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
        ),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/prefix", listener.local_addr().unwrap());
        let mut entry: crate::config::ProviderEntry = serde_json::from_value(
            json!({"npm":package,"options":options,"models":{"catalog":{}}}),
        )
        .unwrap();
        entry.options.body = json!({"nested":{"provider":1,"shared":"provider"}})
            .as_object()
            .unwrap()
            .clone();
        let config = ResponsesConfig {
            base_url: base,
            api_key: "synthetic".into(),
            timeout: Some(false),
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: Default::default(),
            set_cache_key: false,
            wire: crate::config::provider_wire("fixture", &entry).unwrap(),
        };
        let server = tokio::spawn(async move {
            let mut seen = Vec::new();
            for _ in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let boundary = loop {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                    if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                        break i + 4;
                    }
                };
                let header = String::from_utf8(bytes[..boundary].to_vec())
                    .unwrap()
                    .to_ascii_lowercase();
                assert!(header.starts_with(&format!("post {path} ")));
                let length: usize = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                while bytes.len() < boundary + length {
                    let mut buf = [0; 4096];
                    let n = socket.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                seen.push(
                    serde_json::from_slice::<Value>(&bytes[boundary..boundary + length]).unwrap(),
                );
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{terminal}",terminal.len()).as_bytes()).await.unwrap();
            }
            seen
        });
        let overlay = RequestOverlay {
            headers: Default::default(),
            body: json!({"nested":{"profile":2,"shared":"profile"}})
                .as_object()
                .unwrap()
                .clone(),
        };
        let selected = crate::models::SelectedVariant {
            name: "low".into(),
            reasoning_effort: Some("low".into()),
        };
        for variant in [None, Some(&selected)] {
            let mut dispatch = || async { Ok(()) };
            stream_input_overlaid(
                &config,
                &overlay,
                "catalog",
                variant,
                &[InputItem::Message {
                    role: super::super::InputRole::User,
                    content: vec![super::super::InputContent::InputText {
                        text: "hello".into(),
                    }],
                }],
                &[],
                2048,
                &AtomicBool::new(false),
                &mut |_| {},
                &mut dispatch,
            )
            .await
            .unwrap();
        }
        for (index, body) in server.await.unwrap().iter().enumerate() {
            let effort = if index == 0 { "high" } else { "low" };
            assert_eq!(
                body["nested"],
                json!({"provider":1,"profile":2,"shared":"profile"})
            );
            assert_eq!(body["model"], "catalog");
            match config.wire.protocol {
                Protocol::Responses => {
                    assert_eq!(body["reasoning"], json!({"effort":effort,"summary":"auto"}));
                    assert_eq!(body["text"]["verbosity"], "low");
                }
                Protocol::Chat => assert_eq!(body["reasoning_effort"], effort),
                Protocol::Messages => {
                    assert_eq!(body["output_config"]["effort"], effort);
                    assert_eq!(body["thinking"], json!({"type":"adaptive"}));
                }
            }
        }
    }
}
