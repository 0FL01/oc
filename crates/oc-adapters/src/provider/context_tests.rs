use super::*;
use crate::provider::{
    InputItem, InputRole, RequestOverlay, ResponsesConfig, WireBinding, stream_input_overlaid,
};
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool, time::Duration};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, serde_json::Value) {
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
        String::from_utf8_lossy(&bytes[..head]).to_ascii_lowercase(),
        serde_json::from_slice(&bytes[head..head + length]).unwrap(),
    )
}

async fn reply(socket: &mut tokio::net::TcpStream, protocol: Protocol) {
    let response = match protocol {
        Protocol::Responses => format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[]}})
        ),
        Protocol::Chat => format!(
            "data: {}\n\ndata: [DONE]\n\n",
            json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
        ),
        Protocol::Messages => [
            json!({"type":"message_start","message":{"role":"assistant"}}),
            json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}}),
            json!({"type":"message_stop"}),
        ]
        .iter()
        .map(|v| format!("data: {v}\n\n"))
        .collect(),
    };
    socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
}

fn cache_blocks(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Array(values) => values.iter().map(cache_blocks).sum(),
        serde_json::Value::Object(values) => {
            usize::from(values.contains_key("cache_control"))
                + values.values().map(cache_blocks).sum::<usize>()
        }
        _ => 0,
    }
}

#[tokio::test]
async fn cfg15_shared_v2_cache_cases_reach_actual_requests() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/v2-config-semantics.json"
    ))
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/prefix", listener.local_addr().unwrap());
    let expected = fixture.clone();
    let peer = tokio::spawn(async move {
        for package in expected["protocols"].as_array().unwrap() {
            let name = expected["wire_names"][package.as_str().unwrap()]
                .as_str()
                .unwrap();
            let protocol = match name {
                "responses" => Protocol::Responses,
                "chat" => Protocol::Chat,
                "messages" => Protocol::Messages,
                _ => unreachable!("closed fixture protocol"),
            };
            for case in expected["cache_requests"].as_array().unwrap() {
                let (mut socket, _) = listener.accept().await.unwrap();
                let (headers, body) = read_request(&mut socket).await;
                assert_eq!(body["model"], "wire-model");
                if case["expected_key"][name].as_bool().unwrap() {
                    assert_eq!(
                        body["prompt_cache_key"], expected["prompt_cache_key"],
                        "{} / {name}",
                        case["id"]
                    );
                } else {
                    assert!(
                        body.get("prompt_cache_key").is_none(),
                        "{} / {name}",
                        case["id"]
                    );
                }
                assert!(body.get("setCacheKey").is_none());
                if protocol == Protocol::Messages && case["minimal_input"] == true {
                    assert!(body.get("system").is_none() && body.get("tools").is_none());
                }
                assert_eq!(
                    cache_blocks(&body),
                    if protocol == Protocol::Messages {
                        case["expected_blocks"].as_u64().unwrap() as usize
                    } else {
                        0
                    },
                    "{} / {name}",
                    case["id"]
                );
                for (key, value) in expected["base"]["providers"]["fixture"]["headers"]
                    .as_object()
                    .unwrap()
                {
                    assert!(
                        headers.contains(&format!("{key}: {}\r\n", value.as_str().unwrap())),
                        "affinity survives {} / {name}",
                        case["id"]
                    );
                }
                reply(&mut socket, protocol).await;
            }
        }
    });
    for package in fixture["protocols"].as_array().unwrap() {
        for case in fixture["cache_requests"].as_array().unwrap() {
            let documents = std::iter::once(fixture["base"].clone())
                .chain(case["documents"].as_array().unwrap().iter().cloned())
                .chain(std::iter::once(
                    json!({"providers":{"fixture":{"package":package}}}),
                ));
            let sources = documents
                .enumerate()
                .map(|(index, document)| crate::config::Source {
                    path: format!("/fixture-{index}.json"),
                    trusted: false,
                    text: document.to_string(),
                })
                .collect::<Vec<_>>();
            let generation = crate::config::assemble(&sources, &BTreeMap::new(), None).unwrap();
            let mut config = generation.providers["fixture"].options.request_bindings
                [&("m".to_owned(), None)]
                .clone();
            // Only the test transport is rebound to its local peer; canonical
            // documents and captured model/settings/compatibility are unchanged.
            config.base_url = base.clone();
            config.wire.endpoint = None;
            config.allow_private = true;
            config
                .wire
                .requests
                .insert(("m".to_owned(), None), config.clone());
            config.cache = if case["cache"] == "none" {
                CacheMode::None
            } else {
                CacheMode::Auto
            };
            let config = if case["provide_key"] == false {
                config
            } else {
                config.with_context(RequestContext {
                    project: "project-fixture".into(),
                    session: "session-fixture".into(),
                    parent: None,
                    cache: fixture["prompt_cache_key"].as_str().unwrap().into(),
                    operation: None,
                })
            };
            let input = [
                InputItem::message(InputRole::Developer, "initial"),
                InputItem::message(InputRole::User, "first"),
                InputItem::message(InputRole::Assistant, "answer"),
                InputItem::message(InputRole::User, "last"),
            ];
            let tools = [crate::provider::ToolDef {
                name: "read".into(),
                description: "read".into(),
                parameters: json!({"type":"object"}),
            }];
            stream_input_overlaid(
                &config,
                &RequestOverlay::default(),
                "m",
                None,
                &input[usize::from(case["minimal_input"] == true)..],
                if case["minimal_input"] == true {
                    &[]
                } else {
                    &tools
                },
                64,
                &AtomicBool::new(false),
                &mut |_| {},
                &mut || async { Ok(()) },
            )
            .await
            .unwrap();
        }
    }
    peer.await.unwrap();
}

#[tokio::test]
async fn go03_immutable_identity_and_lineage_cache_reach_every_wire() {
    let root = tempfile::tempdir().unwrap();
    let db = crate::storage::Db::open(&root.path().join("data")).unwrap();
    db.create_session("root-session").unwrap();
    db.create_child_session("root-session", "child-session", None, None, None)
        .unwrap();
    let context = RequestContext::capture(&db, root.path(), "child-session").unwrap();
    let root_context = RequestContext::capture(&db, root.path(), "root-session").unwrap();
    assert_ne!(context.cache, root_context.cache);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/prefix", listener.local_addr().unwrap());
    let expected = context.clone();
    let peer = tokio::spawn(async move {
        for (protocol, cached, supported) in [
            (Protocol::Responses, true, false),
            (Protocol::Responses, true, false),
            (Protocol::Chat, true, false),
            (Protocol::Chat, true, true),
            (Protocol::Messages, true, false),
            (Protocol::Messages, false, false),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (headers, body) = read_request(&mut socket).await;
            for (name, value) in [
                ("user-agent", crate::USER_AGENT),
                ("x-opencode-client", "oc"),
                ("x-opencode-project", expected.project.as_str()),
                ("x-opencode-session", "child-session"),
                ("x-session-affinity", "child-session"),
                ("x-session-id", "child-session"),
                ("x-parent-session-id", "root-session"),
            ] {
                assert!(
                    headers.contains(&format!("{name}: {}\r\n", value.to_ascii_lowercase())),
                    "missing {name}"
                );
            }
            assert!(!headers.contains("spoof") && !headers.contains(root.path().to_str().unwrap()));
            if protocol == Protocol::Messages {
                assert!(headers.contains("x-api-key: resolved-key\r\n"));
                assert!(!headers.contains("authorization:"));
            } else {
                assert!(headers.contains("authorization: bearer resolved-key\r\n"));
                assert!(!headers.contains("x-api-key:"));
            }
            match protocol {
                Protocol::Responses => assert_eq!(body["prompt_cache_key"], expected.cache),
                Protocol::Chat if supported => assert_eq!(body["prompt_cache_key"], expected.cache),
                Protocol::Chat => assert!(body.get("prompt_cache_key").is_none()),
                Protocol::Messages => {
                    let count = cache_blocks(&body);
                    assert_eq!(count, if cached { 3 } else { 0 });
                    assert!(body.get("prompt_cache_key").is_none());
                    if cached {
                        assert_eq!(
                            body["tools"][0]["cache_control"],
                            json!({"type":"ephemeral"})
                        );
                        assert_eq!(
                            body["system"][0]["cache_control"],
                            json!({"type":"ephemeral"})
                        );
                    }
                }
            }
            reply(&mut socket, protocol).await;
        }
    });
    for (index, protocol, cached, supported) in [
        (0, Protocol::Responses, true, false),
        (1, Protocol::Responses, true, false),
        (2, Protocol::Chat, true, false),
        (3, Protocol::Chat, true, true),
        (4, Protocol::Messages, true, false),
        (5, Protocol::Messages, false, false),
    ] {
        let config = ResponsesConfig {
            base_url: base.clone(),
            api_key: "resolved-key".into(),
            timeout: None,
            chunk_timeout_ms: 1000,
            connect_timeout: Duration::from_secs(1),
            allow_private: true,
            headers: BTreeMap::from([
                ("X-SESSION-ID".into(), "spoof-config".into()),
                ("X-API-Key".into(), "spoof-auth".into()),
            ]),
            cache: if cached {
                crate::provider::CacheMode::Auto
            } else {
                crate::provider::CacheMode::None
            },
            wire: WireBinding {
                go: true,
                protocol,
                // A custom authToken flag cannot replace the Go native scheme.
                messages_bearer: true,
                compatibility: BTreeMap::from([(
                    "model".into(),
                    crate::provider::chat::ChatCompat {
                        supports_prompt_cache_key: (protocol == Protocol::Chat)
                            .then_some(supported),
                        ..Default::default()
                    },
                )]),
                ..Default::default()
            },
        }
        .with_context(context.clone());
        let overlay = RequestOverlay {
            headers: BTreeMap::from([
                ("x-opencode-project".into(), "spoof-profile".into()),
                ("USER-AGENT".into(), "spoof-profile".into()),
            ]),
            ..Default::default()
        };
        let input = [
            InputItem::message(InputRole::Developer, "initial"),
            InputItem::message(InputRole::User, "first"),
            InputItem::message(InputRole::Assistant, "answer"),
            InputItem::message(InputRole::User, format!("changed request {index}")),
        ];
        let tools = [crate::provider::ToolDef {
            name: "read".into(),
            description: "read".into(),
            parameters: json!({"type":"object"}),
        }];
        stream_input_overlaid(
            &config,
            &overlay,
            "model",
            None,
            &input,
            &tools,
            32,
            &AtomicBool::new(false),
            &mut |_| {},
            &mut || async { Ok(()) },
        )
        .await
        .unwrap();
    }
    peer.await.unwrap();
}

#[test]
fn go03_custom_headers_and_identity_compatibility_are_not_go_policy() {
    let a = RequestContext {
        project: "project".into(),
        session: "a".into(),
        parent: None,
        cache: "lineage".into(),
        operation: None,
    };
    let b = RequestContext {
        session: "b".into(),
        parent: Some("parent".into()),
        ..a.clone()
    };
    let config = ResponsesConfig {
        base_url: "https://example.com/v1".into(),
        api_key: "key".into(),
        timeout: None,
        chunk_timeout_ms: 1000,
        connect_timeout: Duration::from_secs(1),
        allow_private: false,
        headers: BTreeMap::from([
            ("User-Agent".into(), "custom-agent".into()),
            ("X-Session-Id".into(), "custom-session".into()),
        ]),
        cache: crate::provider::CacheMode::Auto,
        wire: WireBinding::default(),
    };
    assert!(
        config
            .with_context(a)
            .same_request_binding(&config.with_context(b))
            .unwrap()
    );
    let headers = crate::provider::request_headers(&config).unwrap();
    assert_eq!(headers["user-agent"], "custom-agent");
    assert_eq!(headers["x-session-id"], "custom-session");
}
