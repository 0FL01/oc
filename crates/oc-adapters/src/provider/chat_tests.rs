use super::*;

fn feed(
    parser: &mut SseParser,
    frames: &[serde_json::Value],
) -> Result<Vec<StreamItem>, ProviderError> {
    let mut items = Vec::new();
    for frame in frames {
        items.extend(parser.push(format!("data: {frame}\n\n").as_bytes())?);
    }
    Ok(items)
}

fn tool_defs() -> Vec<ToolDef> {
    vec![ToolDef {
        name: "read".into(),
        description: "read a file".into(),
        parameters: serde_json::json!({"type": "object"}),
    }]
}

#[test]
fn go03_chat_request_lowers_common_history_in_order() {
    let input = vec![
        InputItem::message(InputRole::Developer, "BASE"),
        InputItem::message(InputRole::Developer, "AGENTS"),
        InputItem::message(InputRole::User, "first"),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"reasoning","id":"rs","encrypted_content":"ALIEN"}),
        ),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"reasoning","id":"chat_reasoning_0","summary":[],"text":"THOUGHT","chat_reasoning_field":"reasoning_content"}),
        ),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[{"type":"output_text","text":"looking"}]}),
        ),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","id":"f1","call_id":"c1","name":"read","arguments":"{\"path\":\"a\"}","status":"completed"}),
        ),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","id":"f2","call_id":"c2","name":"read","arguments":"{\"path\":\"b\"}","status":"completed"}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "c1".into(),
            output: "A".into(),
        },
        InputItem::FunctionCallOutput {
            call_id: "c2".into(),
            output: "B".into(),
        },
        InputItem::message(InputRole::System, "plan <updated>"),
        InputItem::message(InputRole::Developer, "NEW INSTRUCTION"),
        InputItem::Message {
            role: InputRole::User,
            content: vec![
                InputContent::InputText { text: "see".into() },
                InputContent::InputImage {
                    image_url: "data:image/png;base64,AA==".into(),
                    detail: Some("low".into()),
                },
            ],
        },
    ];
    let compat = ChatCompat {
        reasoning_field: Some("reasoning_content".into()),
        max_completion_tokens: true,
        supports_prompt_cache_key: false,
    };
    let body = request_body("m", &input, &tool_defs(), 321, Some("high"), &compat).unwrap();
    let messages = body["messages"].as_array().unwrap();
    assert_eq!(
        messages[0],
        serde_json::json!({"role":"system","content":"BASE\n\nAGENTS"})
    );
    assert_eq!(
        messages[1],
        serde_json::json!({"role":"user","content":"first"})
    );
    assert_eq!(messages[2]["role"], "assistant");
    assert_eq!(messages[2]["content"], "looking");
    assert_eq!(messages[2]["reasoning_content"], "THOUGHT");
    assert_eq!(messages[2]["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(messages[2]["tool_calls"][1]["id"], "c2");
    assert_eq!(
        messages[3],
        serde_json::json!({"role":"tool","tool_call_id":"c1","content":"A"})
    );
    assert_eq!(
        messages[4],
        serde_json::json!({"role":"tool","tool_call_id":"c2","content":"B"})
    );
    assert_eq!(
        messages[5],
        serde_json::json!({"role":"user","content":"<system-update>\nplan &lt;updated&gt;\n</system-update>"})
    );
    assert_eq!(
        messages[6]["content"],
        "<system-update>\nNEW INSTRUCTION\n</system-update>"
    );
    assert_eq!(
        messages[7]["content"][1]["image_url"]["url"],
        "data:image/png;base64,AA=="
    );
    assert!(
        !body.to_string().contains("ALIEN"),
        "alien opaque state never crosses wires"
    );
    assert_eq!(body["max_completion_tokens"], 321);
    assert!(body.get("max_tokens").is_none());
    assert_eq!(body["reasoning_effort"], "high");
    assert_eq!(body["stream_options"]["include_usage"], true);
    assert!(body.get("store").is_none() && body.get("include").is_none());
    assert_eq!(body["tools"][0]["function"]["name"], "read");

    let plain = request_body("m", &input[..3], &[], 5, None, &ChatCompat::default()).unwrap();
    assert_eq!(plain["max_tokens"], 5);
    assert!(plain.get("tools").is_none() && plain.get("reasoning_effort").is_none());
    let no_field = request_body("m", &input, &[], 5, None, &ChatCompat::default()).unwrap();
    assert!(no_field["messages"][2].get("reasoning_content").is_none());
    assert_eq!(
        no_field["tools"],
        serde_json::json!([]),
        "explicit empty list with tool history"
    );
    let reserved = ChatCompat {
        reasoning_field: Some("content".into()),
        ..ChatCompat::default()
    };
    assert!(request_body("m", &input, &[], 5, None, &reserved).is_err());
}

#[test]
fn go03_chat_stream_assembles_fragmented_parallel_tools_reasoning_and_usage() {
    let compat = ChatCompat {
        reasoning_field: Some("reasoning_content".into()),
        ..ChatCompat::default()
    };
    let mut parser = SseParser::chat(&compat);
    let items = feed(&mut parser, &[
        serde_json::json!({"choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"think "}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{"content":"Hel"}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{"content":"lo"}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"read","arguments":"{\"pa"}}]}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"id":"call_b","function":{"name":"read","arguments":"{}"}}]}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"x\"}"}}]}}]}),
        serde_json::json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}),
        serde_json::json!({"choices":[],"usage":{"prompt_tokens":11,"completion_tokens":7}}),
    ]).unwrap();
    assert!(!parser.completed, "usage may follow the finish reason");
    parser.push(b"data: [DONE]\n\n").unwrap();
    assert!(parser.completed);
    assert!(items.contains(&StreamItem::ReasoningDelta("think ".into())));
    assert!(items.contains(&StreamItem::TextDelta("lo".into())));
    assert!(items.contains(&StreamItem::Usage {
        input_tokens: 11,
        output_tokens: 7
    }));
    let started: Vec<_> = items
        .iter()
        .filter(|i| matches!(i, StreamItem::ToolCallStarted { .. }))
        .collect();
    assert_eq!(started.len(), 2);
    let output = parser.output.clone().unwrap();
    assert_eq!(output[0]["type"], "reasoning");
    assert_eq!(output[0]["chat_reasoning_field"], "reasoning_content");
    assert_eq!(output[1]["content"][0]["text"], "Hello");
    assert_eq!(output[2]["call_id"], "call_a");
    assert_eq!(output[2]["arguments"], "{\"path\":\"x\"}");
    assert_eq!(output[3]["call_id"], "call_b");
    assert_eq!(parser.finish, FinishReason::Stop);

    // EOF after a finish reason is a genuine terminal; `length` stays explicit.
    let mut eof = SseParser::chat(&ChatCompat::default());
    feed(
        &mut eof,
        &[serde_json::json!({"choices":[{"delta":{"content":"cut"},"finish_reason":"length"}]})],
    )
    .unwrap();
    eof.finish().unwrap();
    assert!(eof.completed && eof.finish == FinishReason::Length);
}

#[test]
fn go03_chat_refusal_is_visible() {
    let compat = ChatCompat::default();
    let mut parser = SseParser::chat(&compat);
    let items = feed(
        &mut parser,
        &[
            serde_json::json!({"choices":[{"delta":{"content":"Sorry: ","refusal":"cannot "}}]}),
            serde_json::json!({"choices":[{"delta":{"refusal":"help"},"finish_reason":"stop"}]}),
            serde_json::json!({"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":4}}),
        ],
    )
    .unwrap();
    parser.push(b"data: [DONE]\n\n").unwrap();
    assert_eq!(
        items
            .iter()
            .filter_map(|item| match item {
                StreamItem::TextDelta(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>(),
        "Sorry: cannot help"
    );
    assert_eq!(
        parser.output.as_ref().unwrap()[0]["content"][0]["text"],
        "Sorry: cannot help"
    );
    assert!(items.contains(&StreamItem::Usage {
        input_tokens: 3,
        output_tokens: 4
    }));
}

#[test]
fn go03_chat_reasoning_and_refusal_obey_finish_boundary() {
    let compat = ChatCompat {
        reasoning_field: Some("private_thought".into()),
        ..ChatCompat::default()
    };
    for field in [
        "private_thought",
        "reasoning_content",
        "reasoning",
        "reasoning_text",
        "refusal",
    ] {
        let mut parser = SseParser::chat(&compat);
        feed(&mut parser, &[
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}),
        ]).unwrap();
        let late = serde_json::json!({"choices":[{"delta":{field:"late"}}]});
        assert!(
            matches!(
                feed(&mut parser, &[late]),
                Err(ProviderError::OutputStructure {
                    stage: OutputStage::Decode,
                    code: OutputCode::InvalidField
                })
            ),
            "late {field}"
        );
        assert!(
            !parser.completed && parser.output.is_none(),
            "late {field} must not publish tools"
        );
    }
    let mut parser = SseParser::chat(&compat);
    feed(
        &mut parser,
        &[serde_json::json!({"choices":[{"delta":{},"finish_reason":"length"}]})],
    )
    .unwrap();
    let late = serde_json::json!({"choices":[{"delta":{"reasoning_details":[{"text":"late"}]}}]});
    assert!(feed(&mut parser, &[late]).is_err());
    assert!(!parser.completed && parser.output.is_none());

    let mut parser = SseParser::chat(&compat);
    feed(&mut parser, &[
        serde_json::json!({"choices":[{"delta":{"content":"cut"},"finish_reason":"length"}]}),
        serde_json::json!({"choices":[{"delta":{"content":"","refusal":"","private_thought":"","reasoning_details":[],"tool_calls":[]},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":2}}),
    ]).unwrap();
    parser.finish().unwrap();
    assert_eq!(
        parser.finish,
        FinishReason::Length,
        "empty post-finish frame may carry usage, not rewrite the terminal"
    );
}

#[test]
fn go03_chat_stream_refuses_untrustworthy_terminals() {
    let compat = ChatCompat::default();
    // EOF without a finish reason is incomplete, never success.
    let mut parser = SseParser::chat(&compat);
    feed(
        &mut parser,
        &[serde_json::json!({"choices":[{"delta":{"content":"partial"}}]})],
    )
    .unwrap();
    assert!(matches!(parser.finish(), Err(ProviderError::Incomplete)));
    // [DONE] without a finish reason is not a terminal either.
    let mut parser = SseParser::chat(&compat);
    assert!(parser.push(b"data: [DONE]\n\n").is_err());
    // Content after the finish reason, invalid arguments, missing ids, filters and errors.
    let cases = [
        vec![
            serde_json::json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
            serde_json::json!({"choices":[{"delta":{"content":"late"}}]}),
        ],
        vec![
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"read","arguments":"{bad"}}]},"finish_reason":"tool_calls"}]}),
        ],
        vec![
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}),
        ],
        vec![
            serde_json::json!({"choices":[{"delta":{"content":"x"},"finish_reason":"content_filter"}]}),
        ],
        vec![serde_json::json!({"error":{"message":"overloaded","code":529}})],
        vec![
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"read"}}]}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"b","function":{"arguments":"{}"}}]}}]}),
        ],
    ];
    for (index, frames) in cases.iter().enumerate() {
        let mut parser = SseParser::chat(&compat);
        let result = feed(&mut parser, frames).and_then(|_| parser.push(b"data: [DONE]\n\n"));
        assert!(result.is_err(), "case {index}");
        assert!(
            parser.output.is_none(),
            "case {index}: no partial output becomes success"
        );
    }
}
