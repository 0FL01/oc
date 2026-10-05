use super::*;

fn marker(position: usize, effort: Option<&str>, previous: Option<&str>) -> EffortMarker {
    EffortMarker {
        position,
        effort: effort.map(str::to_owned),
        previous: previous.map(str::to_owned),
    }
}

fn conversation() -> Vec<InputItem> {
    vec![
        InputItem::message(InputRole::Developer, "INITIAL_PROMPT"),
        InputItem::message(InputRole::User, "first"),
        InputItem::message(InputRole::System, "plan </system-update> & <b>"),
        InputItem::message(InputRole::Assistant, "answer"),
        InputItem::message(InputRole::User, "second"),
    ]
}

#[test]
fn go03_chronological_system_lowers_per_protocol_in_place() {
    let input = conversation();
    let responses = lower_chronological_system(Protocol::Responses, &input, false);
    assert_eq!(
        responses[2],
        InputItem::message(InputRole::Developer, "plan </system-update> & <b>")
    );
    assert_eq!(
        responses[0], input[0],
        "initial prompt is not a chronological update"
    );
    for (protocol, native) in [(Protocol::Chat, true), (Protocol::Messages, false)] {
        let lowered = lower_chronological_system(protocol, &input, native);
        assert_eq!(
            lowered[2],
            InputItem::message(
                InputRole::User,
                "<system-update>\nplan &lt;/system-update&gt; &amp; &lt;b&gt;\n</system-update>"
            ),
            "{protocol:?}: escaped lower-authority fallback in position"
        );
        assert_eq!(lowered.len(), input.len());
    }
    let native = lower_chronological_system(Protocol::Messages, &input, true);
    assert_eq!(
        native[2], input[2],
        "declared Messages support keeps the native update"
    );
    let plain = vec![InputItem::message(InputRole::User, "no updates")];
    assert!(matches!(
        lower_chronological_system(Protocol::Chat, &plain, false),
        Cow::Borrowed(_)
    ));
}

#[test]
fn go03_effort_markers_follow_donor_resolution() {
    // No markers / drift strip markers and use the captured current effort.
    assert_eq!(
        resolve_effort_updates(&[], Some("low")),
        (false, Some("low".into()))
    );
    let drift = [marker(1, Some("low"), Some("high"))];
    assert_eq!(
        resolve_effort_updates(&drift, Some("medium")),
        (false, Some("medium".into()))
    );
    assert_eq!(
        resolve_effort_updates(&drift, Some("low")),
        (true, Some("high".into()))
    );

    let input = conversation();
    let (unsupported, top) = lower_responses_effort(input.clone(), &drift, Some("low"), false);
    assert_eq!((unsupported, top), (input.clone(), Some("low".into())));

    let (lowered, top) = lower_responses_effort(input.clone(), &drift, Some("low"), true);
    assert_eq!(
        top.as_deref(),
        Some("high"),
        "frozen at the first marker's previous"
    );
    assert_eq!(lowered.len(), input.len() + 1);
    assert_eq!(
        lowered[1],
        InputItem::ProviderOutput(
            serde_json::json!({"type":"configuration_update","reasoning":{"effort":"low"}})
        )
    );
    assert_eq!(
        lowered[2], input[1],
        "update applies only from its change point"
    );

    // Consecutive markers coalesce; newest wins.
    let coalesce = [
        marker(4, Some("low"), Some("medium")),
        marker(4, Some("xhigh"), Some("low")),
    ];
    let (lowered, top) = lower_responses_effort(input.clone(), &coalesce, Some("xhigh"), true);
    assert_eq!(top.as_deref(), Some("medium"));
    let updates: Vec<_> = lowered
        .iter()
        .filter(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] == "configuration_update"))
        .collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        *updates[0],
        InputItem::ProviderOutput(
            serde_json::json!({"type":"configuration_update","reasoning":{"effort":"xhigh"}})
        )
    );

    // Model default: no top-level effort; a switch back to it sends `medium`.
    let defaults = [marker(1, Some("low"), None), marker(4, None, Some("low"))];
    let (lowered, top) = lower_responses_effort(input.clone(), &defaults, None, true);
    assert_eq!(top, None);
    let efforts: Vec<_> = lowered
        .iter()
        .filter_map(|item| match item {
            InputItem::ProviderOutput(v) if v["type"] == "configuration_update" => {
                v["reasoning"]["effort"].as_str().map(str::to_owned)
            }
            _ => None,
        })
        .collect();
    assert_eq!(efforts, ["low", "medium"]);

    // A trailing marker after the last item is still lowered in order.
    let (lowered, _) = lower_responses_effort(
        input.clone(),
        &[marker(5, Some("low"), Some("high"))],
        Some("low"),
        true,
    );
    assert!(
        matches!(lowered.last(), Some(InputItem::ProviderOutput(v)) if v["type"] == "configuration_update")
    );
}

#[test]
fn go03_durable_markers_and_messages_system_placement_are_typed() {
    use crate::provider::messages::request_body_chronological;
    let update = InputItem::EffortUpdate {
        event_seq: 7,
        effort: Some("low".into()),
        previous: None,
    };
    assert_eq!(
        serde_json::from_value::<InputItem>(serde_json::to_value(&update).unwrap()).unwrap(),
        update
    );
    for raw in [
        serde_json::json!({"type":"effort_update","event_seq":0}),
        serde_json::json!({"type":"effort_update","event_seq":1,"effort":42}),
        serde_json::json!({"type":"effort_update","event_seq":1,"effort":"low","secret":"canary"}),
    ] {
        assert!(serde_json::from_value::<InputItem>(raw).is_err());
    }
    let input = vec![
        InputItem::message(InputRole::Developer, "initial"),
        InputItem::message(InputRole::User, "question"),
        update.clone(),
        InputItem::message(InputRole::System, "</system-update>&"),
        InputItem::message(InputRole::Assistant, "answer"),
    ];
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Messages] {
        for supported in [false, true] {
            let (lowered, top) = lower_effort(protocol, &input, Some("low"), supported);
            assert!(
                lowered
                    .iter()
                    .all(|item| !matches!(item, InputItem::EffortUpdate { .. }))
            );
            assert_eq!(
                top.as_deref(),
                if supported && protocol != Protocol::Chat {
                    None
                } else {
                    Some("low")
                }
            );
        }
    }
    let simple = conversation();
    let native = request_body_chronological("any-id", &simple, &[], 32, None, true).unwrap();
    assert_eq!(native["messages"][1]["role"], "system");
    let fallback = request_body_chronological("any-id", &simple, &[], 32, None, false).unwrap();
    assert_eq!(
        fallback["messages"][0]["content"][1]["text"],
        "<system-update>\nplan &lt;/system-update&gt; &amp; &lt;b&gt;\n</system-update>"
    );
    let misplaced = vec![
        InputItem::message(InputRole::Assistant, "old"),
        InputItem::message(InputRole::System, "update"),
        InputItem::message(InputRole::User, "next"),
    ];
    let body = request_body_chronological("any-id", &misplaced, &[], 32, None, true).unwrap();
    assert!(
        body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["role"] != "system")
    );
    let split = vec![
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","call_id":"call","name":"read","arguments":"{}"}),
        ),
        InputItem::message(InputRole::System, "update"),
        InputItem::FunctionCallOutput {
            call_id: "call".into(),
            output: "result".into(),
        },
    ];
    for supported in [false, true] {
        assert!(request_body_chronological("any-id", &split, &[], 32, None, supported).is_err());
    }
}

#[tokio::test]
async fn go03_chronological_effort_reaches_wire_and_messages_beta_is_conditional() {
    use crate::provider::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for protocol in [Protocol::Responses, Protocol::Chat, Protocol::Messages] {
        for (supported, current) in [
            (true, Some("low")),
            (false, Some("low")),
            (true, Some("high")),
            (true, None),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}/prefix", listener.local_addr().unwrap());
            let kept = protocol != Protocol::Chat && supported && current != Some("high");
            let peer = tokio::spawn(async move {
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
                let headers = String::from_utf8_lossy(&bytes[..boundary]).to_ascii_lowercase();
                let len = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                while bytes.len() < boundary + len {
                    let n = stream.read(&mut buf).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&buf[..n]);
                }
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes[boundary..boundary + len]).unwrap();
                assert!(!body.to_string().contains("effort_update"));
                let top = match protocol {
                    Protocol::Responses => &body["reasoning"]["effort"],
                    Protocol::Chat => &body["reasoning_effort"],
                    Protocol::Messages => &body["output_config"]["effort"],
                };
                assert_eq!(top.as_str(), if kept { None } else { current });
                if protocol == Protocol::Messages {
                    let native_text: Vec<_> = body["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|message| {
                            message["role"] == "system" && message.get("output_config").is_none()
                        })
                        .collect();
                    assert_eq!(native_text.len(), usize::from(supported));
                    assert_eq!(
                        headers.contains("mid-conversation-output-config-2026-07-01"),
                        kept
                    );
                    let updates: Vec<_> = body["messages"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|m| m["output_config"]["effort"].as_str())
                        .collect();
                    assert_eq!(
                        updates,
                        if kept {
                            vec![if current.is_none() { "medium" } else { "low" }]
                        } else {
                            vec![]
                        }
                    );
                } else if protocol == Protocol::Responses {
                    let updates: Vec<_> = body["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|v| v["type"] == "configuration_update")
                        .map(|v| v["reasoning"]["effort"].as_str().unwrap())
                        .collect();
                    assert_eq!(
                        updates,
                        if kept {
                            vec![if current.is_none() { "medium" } else { "low" }]
                        } else {
                            vec![]
                        }
                    );
                }
                let terminal = match protocol {
                    Protocol::Responses => {
                        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n"
                    }
                    Protocol::Chat => {
                        "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
                    }
                    Protocol::Messages => {
                        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"role\":\"assistant\",\"usage\":{}}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
                    }
                };
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{terminal}", terminal.len()).as_bytes()).await.unwrap();
            });
            let config = ResponsesConfig {
                base_url,
                api_key: "synthetic".into(),
                timeout: None,
                chunk_timeout_ms: 1000,
                connect_timeout: std::time::Duration::from_secs(1),
                allow_private: true,
                headers: Default::default(),
                set_cache_key: false,
                wire: WireBinding {
                    protocol,
                    chronology: std::collections::BTreeMap::from([(
                        "catalog".into(),
                        Chronology {
                            effort: supported,
                            system: supported,
                        },
                    )]),
                    ..Default::default()
                },
            };
            let input = vec![
                InputItem::message(InputRole::User, "before"),
                InputItem::message(InputRole::System, "native text <update>"),
                InputItem::message(InputRole::Assistant, "old answer"),
                InputItem::EffortUpdate {
                    event_seq: 1,
                    effort: current
                        .filter(|value| *value != "high")
                        .map(str::to_owned)
                        .or_else(|| current.map(|_| "low".into())),
                    previous: None,
                },
                InputItem::message(InputRole::User, "after"),
            ];
            let variant = current.map(|value| crate::models::SelectedVariant {
                name: "selected".into(),
                reasoning_effort: Some(value.into()),
            });
            stream_input_observed(
                &config,
                "catalog",
                variant.as_ref(),
                &input,
                &[],
                32,
                &std::sync::atomic::AtomicBool::new(false),
                &mut |_| {},
            )
            .await
            .unwrap();
            peer.await.unwrap();
        }
    }
}
