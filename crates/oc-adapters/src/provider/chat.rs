//! Thin native Chat Completions wire (T53/R3, GO03).
//!
//! Shares the bounded HTTP/SSE framing of `SseParser`; only event decoding and
//! request lowering are Chat-specific. Completed output is synthesized into the
//! same common items the runtime already journals (`message`,
//! `function_call`, Chat-origin `reasoning`), so tool execution, DCP and replay
//! keep one owner. Pinned OC2 reference: `packages/ai/src/protocols/openai-chat.ts`.
//! Native difference: no host/vendor/model dialect detection; only explicit
//! compatibility facts select `reasoningField`/`maxTokensField`.
use std::collections::BTreeMap;

use super::protocol::{Protocol, lower_chronological_system, wrap_system_update};
use super::{
    FinishReason, InputContent, InputItem, InputRole, OutputCode, OutputStage, ProviderError,
    SseParser, StreamItem, ToolDef, failure, structural,
};

/// Fields a configured reasoning field may never shadow.
const RESERVED_REASONING_FIELDS: [&str; 4] = ["role", "content", "refusal", "tool_calls"];

/// Explicit Chat compatibility facts from the admitted model binding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChatCompat {
    /// Assistant reasoning field (e.g. `reasoning_content`).
    pub reasoning_field: Option<String>,
    /// `max_completion_tokens` instead of the default `max_tokens`.
    pub max_completion_tokens: bool,
    /// The endpoint accepts `prompt_cache_key`.
    pub supports_prompt_cache_key: bool,
}

/// Build the Chat request body. Effort is the exact selected value only.
pub(crate) fn request_body(
    model: &str,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    effort: Option<&str>,
    compat: &ChatCompat,
) -> Result<serde_json::Value, ProviderError> {
    if compat
        .reasoning_field
        .as_deref()
        .is_some_and(|field| RESERVED_REASONING_FIELDS.contains(&field))
    {
        return Err(ProviderError::InvalidConfig);
    }
    let lowered = lower_chronological_system(Protocol::Chat, input, false);
    let messages = lower_messages(&lowered, compat)?;
    let has_tool_history = messages
        .iter()
        .any(|message| message.get("tool_calls").is_some() || message["role"] == "tool");
    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    body[if compat.max_completion_tokens {
        "max_completion_tokens"
    } else {
        "max_tokens"
    }] = max_output.into();
    if !tools.is_empty() {
        body["tools"] = tools
            .iter()
            .map(|tool| {
                serde_json::json!({"type": "function", "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }})
            })
            .collect::<Vec<_>>()
            .into();
    } else if has_tool_history {
        // Donor: keep an explicit empty list when history carries tool calls.
        body["tools"] = serde_json::json!([]);
    }
    if let Some(effort) = effort {
        body["reasoning_effort"] = effort.into();
    }
    Ok(body)
}

/// Lower common input into Chat messages. Leading developer items form the
/// initial system message; later developer items are chronological updates and
/// use the escaped user-text fallback. Alien opaque items are dropped.
fn lower_messages(
    input: &[InputItem],
    compat: &ChatCompat,
) -> Result<Vec<serde_json::Value>, ProviderError> {
    let mut messages: Vec<serde_json::Value> = Vec::new();
    let mut initial = Vec::new();
    let mut leading = true;
    // The assistant message assembled from consecutive provider output items.
    let mut assistant: Option<serde_json::Value> = None;
    let mut pending_reasoning: Option<(String, String)> = None;
    let flush = |messages: &mut Vec<serde_json::Value>,
                 assistant: &mut Option<serde_json::Value>| {
        if let Some(message) = assistant.take() {
            messages.push(message);
        }
    };
    for item in input {
        if leading
            && let InputItem::Message {
                role: InputRole::Developer,
                content,
            } = item
        {
            initial.push(text_of(content));
            continue;
        }
        if leading {
            leading = false;
            if !initial.is_empty() {
                messages
                    .push(serde_json::json!({"role": "system", "content": initial.join("\n\n")}));
            }
        }
        match item {
            InputItem::Message { role, content } => {
                flush(&mut messages, &mut assistant);
                match role {
                    InputRole::Developer | InputRole::System => messages.push(serde_json::json!({
                        "role": "user",
                        "content": wrap_system_update(&text_of(content)),
                    })),
                    InputRole::User => messages.push(user_message(content)),
                    InputRole::Assistant => messages.push(
                        serde_json::json!({"role": "assistant", "content": text_of(content)}),
                    ),
                }
            }
            InputItem::ProviderOutput(value) => match value["type"].as_str() {
                Some("message") if value["role"] == "assistant" => {
                    let text = value["content"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<String>();
                    let message = assistant.get_or_insert_with(
                        || serde_json::json!({"role": "assistant", "content": null}),
                    );
                    let existing = message["content"].as_str().unwrap_or_default().to_owned();
                    message["content"] = format!("{existing}{text}").into();
                }
                Some("function_call") => {
                    let message = assistant.get_or_insert_with(
                        || serde_json::json!({"role": "assistant", "content": null}),
                    );
                    if message.get("tool_calls").is_none() {
                        message["tool_calls"] = serde_json::json!([]);
                    }
                    message["tool_calls"]
                        .as_array_mut()
                        .expect("tool call array")
                        .push(serde_json::json!({
                            "id": value["call_id"],
                            "type": "function",
                            "function": {"name": value["name"], "arguments": value["arguments"]},
                        }));
                }
                Some("reasoning") => {
                    // Only Chat-origin reasoning replays, through its own field.
                    if let (Some(field), Some(text)) = (
                        value["chat_reasoning_field"].as_str(),
                        value["text"].as_str(),
                    ) {
                        pending_reasoning = Some((field.to_owned(), text.to_owned()));
                    }
                }
                _ => {}
            },
            InputItem::FunctionCallOutput { call_id, output } => {
                flush(&mut messages, &mut assistant);
                messages.push(
                    serde_json::json!({"role": "tool", "tool_call_id": call_id, "content": output}),
                );
            }
            other => {
                flush(&mut messages, &mut assistant);
                // MCP/read results serialize as Responses outputs; Chat tool
                // content is text only and other modalities refuse explicitly.
                let value =
                    serde_json::to_value(other).map_err(|_| ProviderError::InvalidConfig)?;
                let content = match &value["output"] {
                    serde_json::Value::String(text) => text.clone(),
                    serde_json::Value::Array(parts) => {
                        let mut text = Vec::new();
                        for part in parts {
                            match part["type"].as_str() {
                                Some("input_text" | "output_text" | "text") => {
                                    text.push(part["text"].as_str().unwrap_or_default().to_owned())
                                }
                                _ => return Err(ProviderError::UnsupportedModality),
                            }
                        }
                        text.join("\n")
                    }
                    _ => return Err(ProviderError::UnsupportedModality),
                };
                messages.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": value["call_id"],
                    "content": content,
                }));
            }
        }
        if let (Some(message), Some((field, text))) =
            (assistant.as_mut(), pending_reasoning.as_ref())
            && compat.reasoning_field.as_deref() == Some(field.as_str())
        {
            message[field.as_str()] = text.clone().into();
        }
    }
    flush(&mut messages, &mut assistant);
    if leading && !initial.is_empty() {
        messages.push(serde_json::json!({"role": "system", "content": initial.join("\n\n")}));
    }
    Ok(messages)
}

fn text_of(content: &[InputContent]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } | InputContent::OutputText { text } => {
                Some(text.as_str())
            }
            InputContent::InputImage { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn user_message(content: &[InputContent]) -> serde_json::Value {
    if content
        .iter()
        .all(|part| !matches!(part, InputContent::InputImage { .. }))
    {
        return serde_json::json!({"role": "user", "content": text_of(content)});
    }
    let parts = content
        .iter()
        .map(|part| match part {
            InputContent::InputText { text } | InputContent::OutputText { text } => {
                serde_json::json!({"type": "text", "text": text})
            }
            InputContent::InputImage { image_url, detail } => {
                let mut image = serde_json::json!({"url": image_url});
                if let Some(detail) = detail {
                    image["detail"] = detail.clone().into();
                }
                serde_json::json!({"type": "image_url", "image_url": image})
            }
        })
        .collect::<Vec<_>>();
    serde_json::json!({"role": "user", "content": parts})
}

/// Incremental Chat decoding state owned by one `SseParser`.
#[derive(Debug, Default)]
pub(crate) struct ChatState {
    reasoning_field: Option<String>,
    text: String,
    reasoning: String,
    observed_reasoning_field: Option<String>,
    /// Tool calls by stream index: (id, name, arguments).
    tools: BTreeMap<u64, (String, String, String)>,
    finish: Option<String>,
}

impl ChatState {
    pub(crate) fn new(compat: &ChatCompat) -> Self {
        Self {
            reasoning_field: compat.reasoning_field.clone(),
            ..Self::default()
        }
    }
}

/// Decode one Chat SSE payload (framing, caps and event counting stay shared).
pub(super) fn dispatch(
    parser: &mut SseParser,
    payload: &str,
) -> Result<Vec<StreamItem>, ProviderError> {
    if payload == "[DONE]" {
        return finalize(parser).map(|()| Vec::new());
    }
    if payload.is_empty() {
        return Ok(Vec::new());
    }
    let cost = payload.len().saturating_mul(4).saturating_add(256);
    if parser.retained.saturating_add(cost) > super::GENERATION_BYTE_CAP {
        return Err(ProviderError::ByteLimit("generation"));
    }
    parser.retained += cost;
    let event: serde_json::Value = serde_json::from_str(payload)
        .map_err(|_| structural(OutputStage::Decode, OutputCode::InvalidJson))?;
    if event.get("error").is_some_and(|error| !error.is_null()) {
        return Err(failure::event_failure(&event));
    }
    let chat = parser.chat.as_mut().expect("chat parser");
    let mut out = Vec::new();
    let choice = event.pointer("/choices/0");
    let delta = choice.and_then(|choice| choice.get("delta"));
    let fields = [
        chat.reasoning_field.as_deref(),
        Some("reasoning_content"),
        Some("reasoning"),
        Some("reasoning_text"),
    ];
    let reasoning = delta.and_then(|delta| {
        fields.into_iter().flatten().find_map(|field| {
            delta[field]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|text| (field, text))
        })
    });
    let late = |delta: Option<&serde_json::Value>| {
        delta.is_some_and(|delta| {
            delta["content"].as_str().is_some_and(|s| !s.is_empty())
                || delta["refusal"].as_str().is_some_and(|s| !s.is_empty())
                || reasoning.is_some()
                || delta["reasoning_details"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
                || delta["tool_calls"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty())
        })
    };
    if chat.finish.is_some() && late(delta) {
        return Err(structural(OutputStage::Decode, OutputCode::InvalidField));
    }
    if let Some(delta) = delta {
        for text in ["content", "refusal"]
            .into_iter()
            .filter_map(|field| delta[field].as_str().filter(|s| !s.is_empty()))
        {
            chat.text.push_str(text);
            out.push(StreamItem::TextDelta(text.to_owned()));
        }
        if let Some((field, text)) = reasoning {
            chat.observed_reasoning_field
                .get_or_insert_with(|| field.to_owned());
            chat.reasoning.push_str(text);
            out.push(StreamItem::ReasoningDelta(text.to_owned()));
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"]
                .as_u64()
                .ok_or_else(|| structural(OutputStage::Added, OutputCode::InvalidIndex))?;
            let entry = chat.tools.entry(index).or_default();
            let announced = !entry.0.is_empty() && !entry.1.is_empty();
            if let Some(id) = call["id"].as_str().filter(|s| !s.is_empty()) {
                if !entry.0.is_empty() && entry.0 != id {
                    return Err(structural(OutputStage::Added, OutputCode::IdentityConflict));
                }
                entry.0 = id.to_owned();
            }
            if let Some(name) = call.pointer("/function/name").and_then(|v| v.as_str()) {
                entry.1.push_str(name);
            }
            if !announced && !entry.0.is_empty() && !entry.1.is_empty() {
                out.push(StreamItem::ToolCallStarted {
                    item_id: item_id(index),
                    call_id: entry.0.clone(),
                    name: entry.1.clone(),
                });
            }
            if let Some(arguments) = call
                .pointer("/function/arguments")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                entry.2.push_str(arguments);
                out.push(StreamItem::ArgDelta {
                    item_id: item_id(index),
                    delta: arguments.to_owned(),
                });
            }
        }
    }
    if let Some(reason) = choice
        .and_then(|choice| choice["finish_reason"].as_str())
        .filter(|s| !s.is_empty())
    {
        chat.finish.get_or_insert_with(|| reason.to_owned());
    }
    let usage = event.get("usage").filter(|u| u.is_object()).or_else(|| {
        choice
            .and_then(|c| c.get("usage"))
            .filter(|u| u.is_object())
    });
    if let Some(usage) = usage
        && let (Some(input), Some(output)) = (
            usage["prompt_tokens"].as_u64(),
            usage["completion_tokens"].as_u64(),
        )
    {
        out.push(StreamItem::Usage {
            input_tokens: input,
            output_tokens: output,
        });
    }
    Ok(out)
}

fn item_id(index: u64) -> String {
    format!("chat_fc_{index}")
}

/// Genuine terminal: `[DONE]` or EOF after a finish reason. Validates every
/// tool call and synthesizes the common completed output.
pub(super) fn finalize(parser: &mut SseParser) -> Result<(), ProviderError> {
    if parser.completed {
        return Ok(());
    }
    let chat = parser.chat.as_ref().expect("chat parser");
    let Some(reason) = chat.finish.as_deref() else {
        return Err(ProviderError::Incomplete);
    };
    parser.finish = match reason {
        "stop" | "end" | "tool_calls" | "function_call" => FinishReason::Stop,
        "length" => FinishReason::Length,
        "content_filter" => {
            return Err(failure::event_failure(
                &serde_json::json!({"error": {"code": "content_filter", "message": "Provider finish_reason: content_filter"}}),
            ));
        }
        other => {
            return Err(failure::event_failure(
                &serde_json::json!({"error": {"message": format!("Provider finish_reason: {other}")}}),
            ));
        }
    };
    let mut output = Vec::new();
    if !chat.reasoning.is_empty() {
        output.push(serde_json::json!({
            "type": "reasoning",
            "id": "chat_reasoning_0",
            "summary": [],
            "text": chat.reasoning,
            "chat_reasoning_field": chat.observed_reasoning_field,
        }));
    }
    if !chat.text.is_empty() {
        output.push(serde_json::json!({
            "type": "message",
            "id": "chat_msg_0",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": chat.text, "annotations": []}],
        }));
    }
    for (index, (id, name, arguments)) in &chat.tools {
        if id.is_empty() || name.is_empty() {
            return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
        }
        let arguments = if arguments.is_empty() {
            "{}"
        } else {
            arguments
        };
        if serde_json::from_str::<serde_json::Value>(arguments)
            .map_or(true, |value| !value.is_object())
        {
            return Err(structural(
                OutputStage::Completion,
                OutputCode::InvalidArguments,
            ));
        }
        output.push(serde_json::json!({
            "type": "function_call",
            "id": item_id(*index),
            "call_id": id,
            "name": name,
            "arguments": arguments,
            "status": "completed",
        }));
    }
    parser.output = Some(output);
    parser.completed = true;
    Ok(())
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
