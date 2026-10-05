//! Thin native Messages lowering/decoding. HTTP, SSE framing, caps, cancellation
//! and the one-physical-attempt contract remain owned by the provider facade.
//! Common completed items keep the existing runtime/tool/journal owners; signed
//! and redacted thinking are marked Messages-origin, never Responses ciphertext.
use std::collections::BTreeMap;

use super::protocol::{Protocol, lower_chronological_system, wrap_system_update};
use super::{
    FinishReason, InputContent, InputItem, InputRole, OutputCode, OutputStage, ProviderError,
    SseParser, StreamItem, ToolDef, failure, structural,
};

pub(crate) fn request_body(
    model: &str,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    effort: Option<&str>,
) -> Result<serde_json::Value, ProviderError> {
    let input = lower_chronological_system(Protocol::Messages, input, false);
    let mut system = Vec::new();
    let mut messages = Vec::new();
    let mut leading = true;
    for item in input.iter() {
        if leading
            && let InputItem::Message {
                role: InputRole::Developer,
                content,
            } = item
        {
            system.extend(parts(content)?);
            continue;
        }
        leading = false;
        match item {
            InputItem::Message { role, content } => match role {
                InputRole::Developer | InputRole::System => {
                    let text = content
                        .iter()
                        .filter_map(|p| match p {
                            InputContent::InputText { text }
                            | InputContent::OutputText { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    append(
                        &mut messages,
                        "user",
                        vec![serde_json::json!({"type":"text", "text":wrap_system_update(&text)})],
                    );
                }
                InputRole::User => append(&mut messages, "user", parts(content)?),
                InputRole::Assistant => append(&mut messages, "assistant", parts(content)?),
            },
            InputItem::ProviderOutput(value) => match value["type"].as_str() {
                Some("message") if value["role"] == "assistant" => {
                    let blocks = value["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| p["text"].as_str())
                        .map(|text| serde_json::json!({"type":"text","text":text}))
                        .collect();
                    append(&mut messages, "assistant", blocks);
                }
                Some("function_call") => {
                    let args = value["arguments"]
                        .as_str()
                        .ok_or(ProviderError::InvalidOutput)?;
                    let args: serde_json::Value =
                        serde_json::from_str(args).map_err(|_| ProviderError::InvalidOutput)?;
                    if !args.is_object() {
                        return Err(ProviderError::InvalidOutput);
                    }
                    append(
                        &mut messages,
                        "assistant",
                        vec![
                            serde_json::json!({"type":"tool_use", "id":value["call_id"], "name":value["name"], "input":args}),
                        ],
                    );
                }
                Some("reasoning") => {
                    if let Some(block) = value.get("messages_thinking") {
                        // Durable compatibility filtering belongs to the runtime;
                        // never interpret alien Chat/Responses reasoning as a signature.
                        let block = if block["type"] == "thinking"
                            && block["signature"].as_str().is_none_or(|s| s.is_empty())
                        {
                            serde_json::json!({"type":"text", "text":block["thinking"]})
                        } else {
                            block.clone()
                        };
                        append(&mut messages, "assistant", vec![block]);
                    }
                }
                Some("function_call_output") => result(&mut messages, value)?,
                _ => {}
            },
            other => result(
                &mut messages,
                &serde_json::to_value(other).map_err(|_| ProviderError::InvalidConfig)?,
            )?,
        }
    }
    let mut body = serde_json::json!({"model":model,"messages":messages,"stream":true,"max_tokens":max_output});
    if !system.is_empty() {
        body["system"] = system.into();
    }
    if !tools.is_empty() {
        body["tools"] = tools.iter().map(|t| serde_json::json!({"name":t.name,"description":t.description,"input_schema":t.parameters})).collect::<Vec<_>>().into();
    }
    if let Some(effort) = effort {
        body["output_config"] = serde_json::json!({"effort":effort});
    }
    Ok(body)
}

fn append(messages: &mut Vec<serde_json::Value>, role: &str, mut blocks: Vec<serde_json::Value>) {
    if blocks.is_empty() {
        return;
    }
    if let Some(last) = messages.last_mut().filter(|m| m["role"] == role) {
        last["content"]
            .as_array_mut()
            .expect("content blocks")
            .append(&mut blocks);
    } else {
        messages.push(serde_json::json!({"role":role,"content":blocks}));
    }
}

fn parts(content: &[InputContent]) -> Result<Vec<serde_json::Value>, ProviderError> {
    content
        .iter()
        .map(|p| match p {
            InputContent::InputText { text } | InputContent::OutputText { text } => {
                Ok(serde_json::json!({"type":"text","text":text}))
            }
            InputContent::InputImage { image_url, .. } => image(image_url),
        })
        .collect()
}

fn image(url: &str) -> Result<serde_json::Value, ProviderError> {
    if let Some(data) = url.strip_prefix("data:") {
        let (mime, data) = data
            .split_once(";base64,")
            .ok_or(ProviderError::UnsupportedModality)?;
        if !matches!(
            mime,
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) {
            return Err(ProviderError::UnsupportedModality);
        }
        Ok(
            serde_json::json!({"type":"image","source":{"type":"base64","media_type":mime,"data":data}}),
        )
    } else if reqwest::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) {
        Ok(serde_json::json!({"type":"image","source":{"type":"url","url":url}}))
    } else {
        Err(ProviderError::UnsupportedModality)
    }
}

fn result(
    messages: &mut Vec<serde_json::Value>,
    value: &serde_json::Value,
) -> Result<(), ProviderError> {
    let content = match &value["output"] {
        serde_json::Value::String(text) => vec![serde_json::json!({"type":"text","text":text})],
        serde_json::Value::Array(parts) => parts
            .iter()
            .map(|p| match p["type"].as_str() {
                Some("input_text" | "output_text" | "text") => {
                    Ok(serde_json::json!({"type":"text","text":p["text"]}))
                }
                Some("input_image") => image(
                    p["image_url"]
                        .as_str()
                        .ok_or(ProviderError::UnsupportedModality)?,
                ),
                _ => Err(ProviderError::UnsupportedModality),
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err(ProviderError::UnsupportedModality),
    };
    let id = value["call_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(ProviderError::InvalidOutput)?;
    append(
        messages,
        "user",
        vec![serde_json::json!({"type":"tool_result","tool_use_id":id,"content":content})],
    );
    Ok(())
}

#[derive(Debug)]
struct Block {
    value: serde_json::Value,
    arguments: String,
    closed: bool,
}

#[derive(Debug, Default)]
pub(crate) struct MessagesState {
    started: bool,
    blocks: BTreeMap<u64, Block>,
    finish: Option<String>,
    usage: serde_json::Map<String, serde_json::Value>,
}

pub(super) fn dispatch(
    parser: &mut SseParser,
    payload: &str,
) -> Result<Vec<StreamItem>, ProviderError> {
    let cost = payload.len().saturating_mul(4).saturating_add(256);
    if parser.retained.saturating_add(cost) > super::GENERATION_BYTE_CAP {
        return Err(ProviderError::ByteLimit("generation"));
    }
    parser.retained += cost;
    let event: serde_json::Value = serde_json::from_str(payload)
        .map_err(|_| structural(OutputStage::Decode, OutputCode::InvalidJson))?;
    let kind = event["type"]
        .as_str()
        .ok_or_else(|| structural(OutputStage::Decode, OutputCode::InvalidField))?;
    if kind == "error" {
        return Err(failure::event_failure(&event));
    }
    if kind == "ping" {
        return Ok(Vec::new());
    }
    let state = parser.messages.as_mut().expect("Messages parser");
    let mut out = Vec::new();
    match kind {
        "message_start" => {
            if state.started || event["message"]["role"] != "assistant" {
                return Err(structural(OutputStage::Added, OutputCode::IdentityConflict));
            }
            state.started = true;
            if let Some(usage) = event["message"]["usage"].as_object() {
                state.usage.extend(usage.clone());
            }
        }
        "content_block_start" => {
            if !state.started || state.finish.is_some() {
                return Err(structural(OutputStage::Added, OutputCode::InvalidStatus));
            }
            let index = index(&event)?;
            if state.blocks.contains_key(&index) {
                return Err(structural(OutputStage::Added, OutputCode::IndexConflict));
            }
            let value = event["content_block"].clone();
            match value["type"].as_str() {
                Some("text") => {
                    if value["text"].as_str().is_none() {
                        return Err(ProviderError::InvalidOutput);
                    }
                    if let Some(text) = value["text"].as_str().filter(|s| !s.is_empty()) {
                        out.push(StreamItem::TextDelta(text.into()));
                    }
                }
                Some("thinking") => {
                    if value["thinking"].as_str().is_none() {
                        return Err(ProviderError::InvalidOutput);
                    }
                    if let Some(text) = value["thinking"].as_str().filter(|s| !s.is_empty()) {
                        out.push(StreamItem::ReasoningDelta(text.into()));
                    }
                }
                Some("redacted_thinking") => {
                    if value["data"].as_str().is_none() {
                        return Err(ProviderError::InvalidOutput);
                    }
                }
                Some("tool_use") => {
                    if value["input"].to_string().len() > super::ARGUMENT_BYTE_CAP {
                        return Err(ProviderError::ByteLimit("arguments"));
                    }
                    let id = identity(&value, "id")?;
                    let name = identity(&value, "name")?;
                    if state
                        .blocks
                        .values()
                        .any(|b| b.value["type"] == "tool_use" && b.value["id"] == id)
                    {
                        return Err(structural(OutputStage::Added, OutputCode::IdentityConflict));
                    }
                    out.push(StreamItem::ToolCallStarted {
                        item_id: item_id(index),
                        call_id: id.into(),
                        name: name.into(),
                    });
                }
                _ => return Err(structural(OutputStage::Added, OutputCode::InvalidField)),
            }
            state.blocks.insert(
                index,
                Block {
                    value,
                    arguments: String::new(),
                    closed: false,
                },
            );
        }
        "content_block_delta" => {
            if state.finish.is_some() {
                return Err(structural(OutputStage::Decode, OutputCode::InvalidStatus));
            }
            let index = index(&event)?;
            let block = state
                .blocks
                .get_mut(&index)
                .filter(|b| !b.closed)
                .ok_or_else(|| structural(OutputStage::Decode, OutputCode::InvalidIndex))?;
            let delta = &event["delta"];
            let (field, expected, text, item) = match delta["type"].as_str() {
                Some("text_delta") => ("text", "text", identity(delta, "text")?, 0),
                Some("thinking_delta") => ("thinking", "thinking", identity(delta, "thinking")?, 1),
                Some("signature_delta") => {
                    ("signature", "thinking", identity(delta, "signature")?, 2)
                }
                Some("input_json_delta") => (
                    "input",
                    "tool_use",
                    delta["partial_json"]
                        .as_str()
                        .ok_or(ProviderError::InvalidOutput)?,
                    3,
                ),
                _ => return Err(structural(OutputStage::Decode, OutputCode::InvalidField)),
            };
            if block.value["type"] != expected {
                return Err(structural(OutputStage::Decode, OutputCode::InvalidField));
            }
            if item == 3 {
                if block.arguments.len().saturating_add(text.len()) > super::ARGUMENT_BYTE_CAP {
                    return Err(ProviderError::ByteLimit("arguments"));
                }
                block.arguments.push_str(text);
                out.push(StreamItem::ArgDelta {
                    item_id: item_id(index),
                    delta: text.into(),
                });
            } else {
                let mut complete = block.value[field].as_str().unwrap_or_default().to_owned();
                complete.push_str(text);
                block.value[field] = complete.into();
                if item == 0 {
                    out.push(StreamItem::TextDelta(text.into()));
                }
                if item == 1 {
                    out.push(StreamItem::ReasoningDelta(text.into()));
                }
            }
        }
        "content_block_stop" => {
            let block = state
                .blocks
                .get_mut(&index(&event)?)
                .filter(|b| !b.closed)
                .ok_or_else(|| structural(OutputStage::Done, OutputCode::InvalidIndex))?;
            if block.value["type"] == "tool_use" {
                let args = if block.arguments.is_empty() {
                    block.value["input"].clone()
                } else {
                    serde_json::from_str(&block.arguments)
                        .map_err(|_| structural(OutputStage::Done, OutputCode::InvalidArguments))?
                };
                if !args.is_object() {
                    return Err(structural(OutputStage::Done, OutputCode::InvalidArguments));
                }
                block.value["input"] = args;
            }
            block.closed = true;
        }
        "message_delta" => {
            if !state.started || state.blocks.values().any(|b| !b.closed) {
                return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
            }
            if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                if state.finish.as_deref().is_some_and(|old| old != reason) {
                    return Err(structural(
                        OutputStage::Completion,
                        OutputCode::InvalidStatus,
                    ));
                }
                state.finish = Some(reason.into());
            }
            if let Some(usage) = event["usage"].as_object() {
                state.usage.extend(usage.clone());
            }
        }
        "message_stop" => {
            parser.finish = match state.finish.as_deref() {
                Some("end_turn" | "stop_sequence" | "pause_turn" | "tool_use") => {
                    FinishReason::Stop
                }
                Some("max_tokens" | "model_context_window_exceeded") => FinishReason::Length,
                Some("refusal") => {
                    return Err(failure::event_failure(
                        &serde_json::json!({"error":{"code":"content_filter"}}),
                    ));
                }
                Some(_) => return Err(ProviderError::ResponseIncomplete),
                None => return Err(ProviderError::Incomplete),
            };
            if state.blocks.values().any(|b| !b.closed) {
                return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
            }
            let mut output = Vec::new();
            for (index, block) in &state.blocks {
                let value = &block.value;
                output.push(match value["type"].as_str() {
                    Some("text") => serde_json::json!({"type":"message","id":format!("messages_msg_{index}"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":value["text"],"annotations":[]}]}),
                    Some("tool_use") => serde_json::json!({"type":"function_call","id":item_id(*index),"call_id":value["id"],"name":value["name"],"arguments":value["input"].to_string(),"status":"completed"}),
                    _ => serde_json::json!({"type":"reasoning","id":format!("messages_reasoning_{index}"),"summary":[],"messages_thinking":value}),
                });
            }
            if let (Some(input), Some(output_tokens)) = (
                state.usage.get("input_tokens").and_then(|v| v.as_u64()),
                state.usage.get("output_tokens").and_then(|v| v.as_u64()),
            ) {
                let cached = ["cache_read_input_tokens", "cache_creation_input_tokens"]
                    .iter()
                    .filter_map(|key| state.usage.get(*key).and_then(|v| v.as_u64()))
                    .fold(0u64, u64::saturating_add);
                out.push(StreamItem::Usage {
                    input_tokens: input.saturating_add(cached),
                    output_tokens,
                });
            }
            parser.output = Some(output);
            parser.completed = true;
            parser.terminal = true;
        }
        _ => return Err(structural(OutputStage::Decode, OutputCode::InvalidField)),
    }
    Ok(out)
}

fn index(event: &serde_json::Value) -> Result<u64, ProviderError> {
    event["index"]
        .as_u64()
        .ok_or_else(|| structural(OutputStage::Decode, OutputCode::InvalidIndex))
}
fn identity<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str, ProviderError> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| structural(OutputStage::Decode, OutputCode::InvalidField))
}
fn item_id(index: u64) -> String {
    format!("messages_fc_{index}")
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
