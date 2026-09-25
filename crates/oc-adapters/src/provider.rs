//! OpenProxy native Responses adapter for T12 (PROV01/02/06/07).
//!
//! Exact generation URL (configured prefix preserved + `/responses`, never a
//! blind `/v1` add/remove), `Bearer` auth, `store:false`, stable
//! `prompt_cache_key`, and ordinary function tool schemas — no OAuth, no API
//! fallback. Bounded incremental SSE (arbitrary byte splits incl. split
//! UTF-8, multiline `data:`, CRLF, comments), text/argument deltas and usage
//! metadata with event/byte caps. Errors distinguish failed/incomplete/EOF;
//! exactly one retry lives here and only before any event
//! is committed — callers never repeat a committed generation or tool call.
//! `timeout:false` means no total deadline; the 6 000 000 ms chunk idle
//! default budget is enforced between bytes; effective config may override it.
//! Explicit cancel interrupts DNS, header and body waits.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::models::SelectedVariant;

/// Idle budget between SSE bytes (6 000 000 ms = 100 min, not 6 s).
pub const CHUNK_TIMEOUT_MS: u64 = 6_000_000;
/// Max SSE events decoded per response.
pub const EVENT_CAP: usize = 10_000;
/// Max attempts per generation: initial + exactly one retry.
pub const MAX_ATTEMPTS: usize = 2;
/// Maximum pending SSE line and complete event, in bytes.
pub const SSE_BYTE_CAP: usize = 2 * 1024 * 1024;
/// Maximum arguments for one call, including all deltas.
pub const ARGUMENT_BYTE_CAP: usize = 1024 * 1024;
/// Conservative retained generation budget (payload copies and item overhead).
pub const GENERATION_BYTE_CAP: usize = 32 * 1024 * 1024;
/// Maximum serialized request, matching the pinned proxy's body ceiling.
pub const REQUEST_BYTE_CAP: usize = 32 * 1024 * 1024;
/// Largest text fragment passed to an observer.
pub const TEXT_DELTA_BYTE_CAP: usize = 16 * 1024;

/// Typed provider errors (no credentials, no prompt contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// 401: key rejected; never retried, never logged with the key.
    #[error("unauthorized")]
    Unauthorized,
    /// 403: forbidden.
    #[error("forbidden")]
    Forbidden,
    /// 429: rate limited (retryable once, pre-commit only).
    #[error("rate limited")]
    RateLimited,
    /// 5xx: server failure (retryable once, pre-commit only).
    #[error("server error")]
    Server,
    /// Truncated stream: incomplete bytes or early EOF.
    #[error("incomplete stream")]
    Incomplete,
    /// SSRF guard: non-public dial target.
    #[error("private host refused")]
    PrivateHost,
    /// Too many redirects.
    #[error("too many redirects")]
    TooManyRedirects,
    /// Chunk idle budget exhausted.
    #[error("chunk idle timeout")]
    IdleTimeout,
    /// Explicit cancellation closed the request.
    #[error("cancelled")]
    Cancelled,
    /// SSE event cap exceeded.
    #[error("event cap exceeded")]
    EventCap,
    /// Transport failure (kind only).
    #[error("transport error")]
    Transport,
    /// Deadline exceeded (connect).
    #[error("deadline exceeded")]
    Deadline,
    /// Misconfiguration (bad base URL, oversize body).
    #[error("invalid config")]
    InvalidConfig,
    /// Terminal provider failure; raw error messages are deliberately withheld.
    #[error("response failed")]
    Failed,
    /// Terminal incomplete response (distinct from transport EOF).
    #[error("response incomplete")]
    ResponseIncomplete,
    /// Nonretryable HTTP status, without potentially sensitive response body.
    #[error("HTTP status {0}")]
    HttpStatus(u16),
    /// Byte ceiling reached before appending data.
    #[error("{0} byte limit exceeded")]
    ByteLimit(&'static str),
    /// Malformed UTF-8 is never silently replaced.
    #[error("invalid UTF-8 in stream")]
    InvalidUtf8,
}

/// Responses message role, independent of UI event kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputRole {
    /// System instructions.
    System,
    /// Developer instructions.
    Developer,
    /// User content.
    User,
    /// Previous assistant content.
    Assistant,
}

/// Typed Responses content parts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    /// Text supplied to the model.
    InputText { text: String },
    /// URL or data URL; omitted detail uses the provider's default.
    InputImage {
        image_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// Prior assistant text.
    OutputText { text: String },
}

/// Canonical continuation input. Provider output is replayed without alteration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    /// A typed message.
    Message {
        role: InputRole,
        content: Vec<InputContent>,
    },
    /// A tool result linked to the function call's call_id, never its item id.
    FunctionCallOutput { call_id: String, output: String },
    /// Complete output item, including opaque fields and assistant phase.
    #[serde(untagged)]
    ProviderOutput(serde_json::Value),
}

impl<'de> Deserialize<'de> for InputItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        // Canonical messages carry ids/phase/status and must never be decoded
        // through the narrower user-authored message representation.
        if value["type"] == "message"
            && value.get("id").is_none()
            && value.get("phase").is_none()
            && value.get("status").is_none()
        {
            let role =
                serde_json::from_value(value["role"].clone()).map_err(serde::de::Error::custom)?;
            let content = serde_json::from_value(value["content"].clone())
                .map_err(serde::de::Error::custom)?;
            return Ok(Self::Message { role, content });
        }
        if value["type"] == "function_call_output" {
            let call_id = value["call_id"]
                .as_str()
                .ok_or_else(|| serde::de::Error::custom("missing call_id"))?
                .to_owned();
            let output = value["output"]
                .as_str()
                .ok_or_else(|| serde::de::Error::custom("missing output"))?
                .to_owned();
            return Ok(Self::FunctionCallOutput { call_id, output });
        }
        Ok(Self::ProviderOutput(value))
    }
}

impl InputItem {
    /// Construct a text message (canonical model output should use ProviderOutput).
    pub fn message(role: InputRole, text: impl Into<String>) -> Self {
        Self::Message {
            role,
            content: vec![if role == InputRole::Assistant {
                InputContent::OutputText { text: text.into() }
            } else {
                InputContent::InputText { text: text.into() }
            }],
        }
    }
}

/// Ordinary function tool definition for the request schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolDef {
    /// Function name.
    pub name: String,
    /// Human description.
    pub description: String,
    /// JSON schema value.
    pub parameters: serde_json::Value,
}

/// Stream items decoded from SSE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamItem {
    /// Incremental model text.
    TextDelta(String),
    /// Position of an assistant message output item; only its identity and
    /// index are forwarded, never its content or opaque fields.
    MessageBoundary {
        /// Canonical output item id, when supplied.
        item_id: String,
        /// Position in the completed Responses output, when supplied.
        output_index: Option<u64>,
        /// Whether this is the end (rather than the start) of the item.
        done: bool,
    },
    /// A function-call item was announced (`output_item.added`).
    ToolCallStarted {
        /// Item id the following argument deltas attach to.
        item_id: String,
        /// Function invocation id used by function_call_output.
        call_id: String,
        /// Model-facing tool name.
        name: String,
    },
    /// Incremental function-call arguments for `item_id`.
    ArgDelta {
        /// Item id under construction.
        item_id: String,
        /// Argument JSON fragment.
        delta: String,
    },
    /// Incremental reasoning text (never opaque-UI-logged; see turn log).
    ReasoningDelta(String),
    /// Opaque provider item (e.g. encrypted reasoning): replayed verbatim
    /// at the continuation boundary, never rendered or logged.
    OpaqueItem {
        /// Item id for boundary matching.
        item_id: String,
        /// Verbatim provider payload.
        payload: serde_json::Value,
    },
    /// Terminal usage metadata.
    Usage {
        /// Input tokens billed.
        input_tokens: u64,
        /// Output tokens billed.
        output_tokens: u64,
    },
}

/// Complete streamed generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    /// Full completed output, including opaque continuation state.
    pub output: Vec<serde_json::Value>,
    /// Ordered stream items (deltas in arrival order).
    pub items: Vec<StreamItem>,
    /// Full model text (concatenated deltas).
    pub text: String,
    /// Usage when reported, else `None`.
    pub usage: Option<(u64, u64)>,
}

/// Adapter configuration (secret `api_key` never appears in `Debug`).
#[derive(Clone)]
pub struct ResponsesConfig {
    /// Configured base prefix (e.g. `https://proxy/v1`), slash-tolerant.
    pub base_url: String,
    /// API key (sent as `Bearer`, never logged).
    pub api_key: String,
    /// `timeout:false`/absent means no total deadline.
    pub timeout: Option<bool>,
    /// Idle ms between body bytes (default [`CHUNK_TIMEOUT_MS`]).
    pub chunk_timeout_ms: u64,
    /// Bounded connect timeout.
    pub connect_timeout: Duration,
    /// Test-only private-network exception (mirrors webfetch).
    pub allow_private: bool,
    /// Extra request headers (values are never included in Debug).
    pub headers: BTreeMap<String, String>,
    /// Whether to send a deterministic prompt_cache_key.
    pub set_cache_key: bool,
}

impl std::fmt::Debug for ResponsesConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponsesConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &"***")
            .field("timeout", &self.timeout)
            .field("chunk_timeout_ms", &self.chunk_timeout_ms)
            .finish()
    }
}

impl ResponsesConfig {
    /// Exact generation URL: trimmed prefix + `/responses`.
    pub fn generation_url(&self) -> Result<String, ProviderError> {
        let base = self.base_url.trim();
        if base.is_empty() || base.contains(' ') || base.contains('\0') {
            return Err(ProviderError::InvalidConfig);
        }
        Ok(format!("{}/responses", base.trim_end_matches('/')))
    }
}

/// Stable cache key over logical request content (prompt + sorted tools).
pub fn prompt_cache_key(prompt: &str, tools: &[ToolDef]) -> String {
    let mut sorted: Vec<&ToolDef> = tools.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let canonical = serde_json::json!({
        "prompt": prompt,
        "tools": sorted
            .iter()
            .map(|t| serde_json::json!({
                "name": t.name,
                "description": t.description,
                "parameters": t.parameters,
            }))
            .collect::<Vec<_>>(),
    });
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
}

/// Request body: exact selected `model`, `store:false`, input message,
/// ordinary function tools. The variant contributes only an explicit
/// `reasoning.effort`; nothing is guessed from the model name.
pub fn request_body(
    model: &str,
    variant: Option<&SelectedVariant>,
    prompt: &str,
    tools: &[ToolDef],
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": model,
        "store": false,
        "stream": true,
        "input": [{"role": "user", "content": prompt}],
        "tools": tools.iter().map(|t| serde_json::json!({
            "type": "function",
            "name": t.name,
            "description": t.description,
            "parameters": t.parameters,
        })).collect::<Vec<_>>(),
        "prompt_cache_key": prompt_cache_key(prompt, tools),
    });
    if let Some(effort) = variant.and_then(|v| v.reasoning_effort.as_deref()) {
        body["reasoning"] = serde_json::json!({"effort": effort});
    }
    body
}

/// Incremental SSE decoder over an arbitrary byte stream.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Pending bytes (incomplete UTF-8 tail or partial line).
    pending: Vec<u8>,
    /// Accumulated `data:` lines for the current event.
    data: String,
    /// Current event's `event:` field, if any.
    event_name: Option<String>,
    /// Events decoded so far (cap enforcement).
    events: usize,
    completed: bool,
    terminal: bool,
    retained: usize,
    arguments: BTreeMap<String, usize>,
    announced_calls: std::collections::BTreeSet<String>,
    output_done: BTreeMap<u64, serde_json::Value>,
    output: Option<Vec<serde_json::Value>>,
}

impl SseParser {
    /// Feed bytes; returns decoded stream items (may be empty mid-line).
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamItem>, ProviderError> {
        self.push_observed(bytes, &mut |_| {})
    }

    fn push_observed(
        &mut self,
        bytes: &[u8],
        observe: &mut (dyn FnMut(&StreamItem) + Send),
    ) -> Result<Vec<StreamItem>, ProviderError> {
        let mut out = Vec::new();
        for segment in bytes.split_inclusive(|b| *b == b'\n') {
            if self.completed {
                break;
            }
            if self.pending.len().saturating_add(segment.len()) > SSE_BYTE_CAP {
                return Err(ProviderError::ByteLimit("SSE line"));
            }
            self.pending.extend_from_slice(segment);
            if !segment.ends_with(b"\n") {
                if let Err(error) = std::str::from_utf8(&self.pending)
                    && error.error_len().is_some()
                {
                    return Err(ProviderError::InvalidUtf8);
                }
                continue;
            }
            let pending = std::mem::take(&mut self.pending);
            let line = std::str::from_utf8(&pending).map_err(|_| ProviderError::InvalidUtf8)?;
            let line = line.strip_suffix('\n').unwrap_or(line);
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                if let Some(item) = self.dispatch()? {
                    // Split at UTF-8 boundaries before observing or retaining text.
                    if let StreamItem::TextDelta(text) = item {
                        let mut rest = text.as_str();
                        while !rest.is_empty() {
                            let mut end = rest.len().min(TEXT_DELTA_BYTE_CAP);
                            while !rest.is_char_boundary(end) {
                                end -= 1;
                            }
                            let item = StreamItem::TextDelta(rest[..end].to_owned());
                            observe(&item);
                            out.push(item);
                            rest = &rest[end..];
                        }
                    } else {
                        observe(&item);
                        out.push(item);
                    }
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                if self.data.len().saturating_add(data.len()).saturating_add(1) > SSE_BYTE_CAP {
                    return Err(ProviderError::ByteLimit("SSE event"));
                }
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(data);
            } else if let Some(name) = line.strip_prefix("event:") {
                self.event_name = Some(name.trim().to_owned());
            }
        }
        Ok(out)
    }

    /// Flush at EOF: a non-empty tail without a dispatching blank line is
    /// an incomplete stream, not a silent drop.
    pub fn finish(&mut self) -> Result<Vec<StreamItem>, ProviderError> {
        if self.pending.is_empty() && self.data.is_empty() {
            return Ok(Vec::new());
        }
        if !self.pending.is_empty() && std::str::from_utf8(&self.pending).is_err() {
            return Err(ProviderError::Incomplete);
        }
        if !self.data.is_empty() {
            return Err(ProviderError::Incomplete);
        }
        if !self.pending.is_empty() {
            return Err(ProviderError::Incomplete);
        }
        Ok(Vec::new())
    }

    fn dispatch(&mut self) -> Result<Option<StreamItem>, ProviderError> {
        if self.data.is_empty() && self.event_name.is_none() {
            return Ok(None);
        }
        self.events += 1;
        if self.events > EVENT_CAP {
            return Err(ProviderError::EventCap);
        }
        let payload = std::mem::take(&mut self.data);
        self.event_name = None;
        if payload == "[DONE]" || payload.is_empty() {
            return Ok(None);
        }
        // Account for parser JSON, retained item, full text and canonical output
        // copies, plus per-event container overhead, before parsing/cloning.
        let cost = payload.len().saturating_mul(4).saturating_add(256);
        if self.retained.saturating_add(cost) > GENERATION_BYTE_CAP {
            return Err(ProviderError::ByteLimit("generation"));
        }
        self.retained += cost;
        let value: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| ProviderError::Incomplete)?;
        match value["type"].as_str() {
            Some("response.output_item.added") if value["item"]["type"] == "function_call" => {
                let item = &value["item"];
                let id = item["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or(ProviderError::Incomplete)?;
                self.announced_calls.insert(id.to_owned());
                if item["arguments"]
                    .as_str()
                    .is_some_and(|args| args.len() > ARGUMENT_BYTE_CAP)
                {
                    return Err(ProviderError::ByteLimit("arguments"));
                }
            }
            Some("response.failed" | "error") => {
                self.terminal = true;
                return Err(ProviderError::Failed);
            }
            Some("response.incomplete") => {
                self.terminal = true;
                return Err(ProviderError::ResponseIncomplete);
            }
            Some("response.function_call_arguments.delta") => {
                let id = value["item_id"].as_str().ok_or(ProviderError::Incomplete)?;
                let delta = value["delta"].as_str().ok_or(ProviderError::Incomplete)?;
                let size = self.arguments.entry(id.to_owned()).or_default();
                if size.saturating_add(delta.len()) > ARGUMENT_BYTE_CAP {
                    return Err(ProviderError::ByteLimit("arguments"));
                }
                *size += delta.len();
            }
            Some("response.output_item.done") => {
                let item = value.get("item").ok_or(ProviderError::Incomplete)?;
                validate_output(item)?;
                let index = value["output_index"]
                    .as_u64()
                    .unwrap_or(self.output_done.len() as u64);
                self.output_done.insert(index, item.clone());
            }
            Some("response.completed") => {
                self.terminal = true;
                match value
                    .pointer("/response/status")
                    .and_then(serde_json::Value::as_str)
                {
                    Some("failed") => return Err(ProviderError::Failed),
                    Some("incomplete" | "in_progress" | "cancelled" | "queued") => {
                        return Err(ProviderError::ResponseIncomplete);
                    }
                    Some("completed") | None => {}
                    _ => return Err(ProviderError::Incomplete),
                }
                if let Some(output) = value.pointer("/response/output") {
                    let output = output.as_array().ok_or(ProviderError::Incomplete)?;
                    for item in output {
                        validate_output(item)?;
                    }
                    self.output = Some(output.clone());
                }
                for id in self.announced_calls.iter().chain(self.arguments.keys()) {
                    let complete = |item: &serde_json::Value| {
                        item["type"] == "function_call" && item["id"].as_str() == Some(id.as_str())
                    };
                    let found = match &self.output {
                        Some(output) => output.iter().any(complete),
                        None => self.output_done.values().any(complete),
                    };
                    if !found {
                        return Err(ProviderError::ResponseIncomplete);
                    }
                }
                self.completed = true;
            }
            _ => {}
        }
        Ok(map_event(&value))
    }
}

fn validate_output(item: &serde_json::Value) -> Result<(), ProviderError> {
    if item["status"].as_str().is_some_and(|s| s != "completed") {
        return Err(ProviderError::ResponseIncomplete);
    }
    if item["type"] == "function_call" {
        let arguments = item["arguments"]
            .as_str()
            .ok_or(ProviderError::Incomplete)?;
        if arguments.len() > ARGUMENT_BYTE_CAP {
            return Err(ProviderError::ByteLimit("arguments"));
        }
        if item["call_id"].as_str().is_none_or(str::is_empty)
            || item["name"].as_str().is_none_or(str::is_empty)
            || serde_json::from_str::<serde_json::Value>(arguments).is_err()
        {
            return Err(ProviderError::Incomplete);
        }
    }
    Ok(())
}

/// Map a Responses event object to a stream item (`None` = ignorable).
fn map_event(value: &serde_json::Value) -> Option<StreamItem> {
    match value.get("type").and_then(|t| t.as_str()) {
        Some("response.output_text.delta") => value
            .get("delta")
            .and_then(|d| d.as_str())
            .map(|d| StreamItem::TextDelta(d.to_string())),
        Some("response.function_call_arguments.delta") => {
            let item_id = value
                .get("item_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            value
                .get("delta")
                .and_then(|d| d.as_str())
                .map(|d| StreamItem::ArgDelta {
                    item_id,
                    delta: d.to_string(),
                })
        }
        Some("response.output_item.added") => {
            let item = value.get("item")?;
            if item["type"] == "message" && item["role"] == "assistant" {
                return Some(StreamItem::MessageBoundary {
                    item_id: item["id"].as_str().unwrap_or("").to_owned(),
                    output_index: value["output_index"].as_u64(),
                    done: false,
                });
            }
            if item.get("type").and_then(|t| t.as_str()) != Some("function_call") {
                return None;
            }
            Some(StreamItem::ToolCallStarted {
                call_id: item
                    .get("call_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned(),
                item_id: item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                name: item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        }
        Some(
            "response.reasoning.delta"
            | "response.reasoning_text.delta"
            | "response.reasoning_summary_text.delta",
        ) => value
            .get("delta")
            .and_then(|d| d.as_str())
            .map(|d| StreamItem::ReasoningDelta(d.to_string())),
        Some("response.output_item.done") => {
            let item = value.get("item")?;
            match item.get("type").and_then(|t| t.as_str()) {
                Some("reasoning") => Some(StreamItem::OpaqueItem {
                    item_id: item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    payload: item.clone(),
                }),
                Some("message") if item["role"] == "assistant" => {
                    Some(StreamItem::MessageBoundary {
                        item_id: item["id"].as_str().unwrap_or("").to_owned(),
                        output_index: value["output_index"].as_u64(),
                        done: true,
                    })
                }
                _ => None,
            }
        }
        Some("response.completed") => {
            let usage = value.pointer("/response/usage");
            let input = usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())?;
            let output = usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())?;
            Some(StreamItem::Usage {
                input_tokens: input,
                output_tokens: output,
            })
        }
        _ => None,
    }
}

/// Stream one generation: POST → SSE → items.
///
/// Exactly one retry is owned here, and only before the first event commits.
/// `cancel` interrupts network waits; setting it drops the connection and
/// reports [`ProviderError::Cancelled`].
pub async fn stream_generation(
    config: &ResponsesConfig,
    model: &str,
    variant: Option<&SelectedVariant>,
    prompt: &str,
    tools: &[ToolDef],
    cancel: &AtomicBool,
    chunk_timeout: Option<Duration>,
) -> Result<Generation, ProviderError> {
    stream_generation_observed(
        config,
        model,
        variant,
        prompt,
        tools,
        cancel,
        chunk_timeout,
        &mut |_| {},
    )
    .await
}

/// Stream to the application as items arrive, in addition to the turn report.
#[allow(clippy::too_many_arguments)]
pub async fn stream_generation_observed(
    config: &ResponsesConfig,
    model: &str,
    variant: Option<&SelectedVariant>,
    prompt: &str,
    tools: &[ToolDef],
    cancel: &AtomicBool,
    chunk_timeout: Option<Duration>,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, ProviderError> {
    bounded_json(&(
        model,
        prompt,
        tools,
        variant.and_then(|v| v.reasoning_effort.as_deref()),
    ))?;
    let mut body = request_body(model, variant, prompt, tools);
    if !config.set_cache_key {
        body.as_object_mut()
            .expect("request object")
            .remove("prompt_cache_key");
    }
    stream_body(
        config,
        bounded_json(&body)?,
        cancel,
        chunk_timeout.unwrap_or(Duration::from_millis(config.chunk_timeout_ms)),
        observe,
    )
    .await
}

/// Runtime entry point: typed, stateless Responses continuation with effective options.
#[allow(clippy::too_many_arguments)]
pub async fn stream_input_observed(
    config: &ResponsesConfig,
    model: &str,
    variant: Option<&SelectedVariant>,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, ProviderError> {
    if max_output == 0 {
        return Err(ProviderError::InvalidConfig);
    }
    // Preflight borrowed input before constructing any owned request copies.
    bounded_json(&(
        model,
        input,
        tools,
        variant.and_then(|v| v.reasoning_effort.as_deref()),
    ))?;
    let mut body = serde_json::json!({
        "model": model, "store": false, "stream": true, "input": input,
        "include": ["reasoning.encrypted_content"],
        "max_output_tokens": max_output,
        "tools": tools.iter().map(|tool| serde_json::json!({
            "type": "function", "name": tool.name,
            "description": tool.description, "parameters": tool.parameters,
        })).collect::<Vec<_>>(),
    });
    if let Some(effort) = variant.and_then(|v| v.reasoning_effort.as_deref()) {
        body["reasoning"] = serde_json::json!({"effort": effort});
    }
    if config.set_cache_key {
        body["prompt_cache_key"] = format!("{:x}", Sha256::digest(bounded_json(&body)?)).into();
    }
    stream_body(
        config,
        bounded_json(&body)?,
        cancel,
        Duration::from_millis(config.chunk_timeout_ms),
        observe,
    )
    .await
}

/// Serializer refuses bytes before allocation/append beyond the request cap.
fn bounded_json(value: &impl Serialize) -> Result<Vec<u8>, ProviderError> {
    struct Writer(Vec<u8>);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > REQUEST_BYTE_CAP {
                return Err(std::io::Error::other("request byte limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(Vec::new());
    serde_json::to_writer(&mut writer, value).map_err(|_| ProviderError::ByteLimit("request"))?;
    Ok(writer.0)
}

/// Cancellation waiter shared with native protocol adapters. No wakeup depends
/// on network activity; callers select this against DNS/header/body futures.
pub(crate) async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn stream_body(
    config: &ResponsesConfig,
    body: Vec<u8>,
    cancel: &AtomicBool,
    chunk_timeout: Duration,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, ProviderError> {
    if config.timeout == Some(true) {
        return Err(ProviderError::InvalidConfig);
    }
    let headers = request_headers(config)?;
    let url = config.generation_url()?;
    tokio::select! {
        biased;
        () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
        result = tokio::time::timeout(config.connect_timeout, guard_private_url(&url, config.allow_private)) => {
            result.map_err(|_| ProviderError::Deadline)??;
        }
    }

    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(crate::USER_AGENT)
        .connect_timeout(config.connect_timeout);
    // timeout:false (or absent) means no total deadline: never set one.
    let client = builder.build().map_err(|_| ProviderError::Transport)?;

    let mut attempts = 0usize;
    loop {
        attempts += 1;
        match stream_attempt(
            &client,
            &url,
            &headers,
            &body,
            config.allow_private,
            cancel,
            chunk_timeout,
            observe,
        )
        .await
        {
            Ok(generation) => return Ok(generation),
            Err((error, committed)) => {
                // Retryable pre-commit only: truncated/idle streams are
                // transient transport faults; nothing executed yet, so a
                // fresh attempt cannot double-apply tool effects.
                let retryable = matches!(
                    error,
                    ProviderError::RateLimited
                        | ProviderError::Server
                        | ProviderError::Transport
                        | ProviderError::Incomplete
                        | ProviderError::Deadline
                        | ProviderError::IdleTimeout
                );
                if retryable && !committed && attempts < MAX_ATTEMPTS {
                    continue;
                }
                return Err(error);
            }
        }
    }
}

fn request_headers(config: &ResponsesConfig) -> Result<reqwest::header::HeaderMap, ProviderError> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let mut headers = HeaderMap::new();
    for (name, value) in &config.headers {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| ProviderError::InvalidConfig)?;
        match name.as_str() {
            "authorization" | "accept" | "content-type" => continue,
            "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "upgrade"
            | "trailer"
            | "te"
            | "proxy-authorization"
            | "proxy-connection" => return Err(ProviderError::InvalidConfig),
            _ => {}
        }
        let mut value = HeaderValue::from_str(value).map_err(|_| ProviderError::InvalidConfig)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", config.api_key))
        .map_err(|_| ProviderError::InvalidConfig)?;
    auth.set_sensitive(true);
    headers.insert("authorization", auth);
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(headers)
}

async fn guard_private_url(url: &str, allow_private: bool) -> Result<(), ProviderError> {
    let url = reqwest::Url::parse(url).map_err(|_| ProviderError::InvalidConfig)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ProviderError::InvalidConfig);
    }
    let host = url
        .host_str()
        .ok_or(ProviderError::InvalidConfig)?
        .trim_matches(['[', ']']);
    let port = url
        .port_or_known_default()
        .ok_or(ProviderError::InvalidConfig)?;
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| ProviderError::PrivateHost)?;
    let mut any = false;
    for addr in addrs {
        any = true;
        let ip = addr.ip();
        if allow_private && ip.is_loopback() {
            continue;
        }
        if !crate::webfetch::ip_is_public(ip) {
            return Err(ProviderError::PrivateHost);
        }
    }
    if any {
        Ok(())
    } else {
        Err(ProviderError::PrivateHost)
    }
}

/// One POST→SSE attempt. Returns the terminal error plus whether any event
/// was already committed (which forbids retry).
#[allow(clippy::too_many_arguments)]
async fn stream_attempt(
    client: &reqwest::Client,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    body: &[u8],
    allow_private: bool,
    cancel: &AtomicBool,
    chunk_timeout: Duration,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
) -> Result<Generation, (ProviderError, bool)> {
    let request = client
        .post(url)
        .headers(headers.clone())
        .body(body.to_vec());
    let resp = tokio::select! {
        biased;
        () = wait_cancel(cancel) => return Err((ProviderError::Cancelled, false)),
        result = tokio::time::timeout(chunk_timeout, request.send()) => result.map_err(|_| (ProviderError::IdleTimeout, false))?,
    }
        .map_err(|e| {
            // Only true timeouts are deadlines; refusals/DNS failures are
            // transport errors (connect_timeout expiry surfaces is_timeout).
            let error = if e.is_timeout() {
                ProviderError::Deadline
            } else {
                ProviderError::Transport
            };
            (error, false)
        })?;
    // Post-dial rebinding guard on the connected peer.
    if let Some(peer) = resp.remote_addr()
        && !crate::webfetch::ip_is_public(peer.ip())
        && !(allow_private && peer.ip().is_loopback())
    {
        return Err((ProviderError::PrivateHost, false));
    }
    let status = resp.status().as_u16();
    if status == 401 {
        return Err((ProviderError::Unauthorized, false));
    }
    if status == 403 {
        return Err((ProviderError::Forbidden, false));
    }
    if status == 429 {
        return Err((ProviderError::RateLimited, false));
    }
    if status >= 500 {
        return Err((ProviderError::Server, false));
    }
    if status != 200 {
        return Err((ProviderError::HttpStatus(status), false));
    }

    let mut parser = SseParser::default();
    let mut items: Vec<StreamItem> = Vec::new();
    let mut committed = false;
    let mut resp = resp;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err((ProviderError::Cancelled, committed));
        }
        // The idle budget applies to the wait itself, not just between
        // polls: a silent gap longer than chunk_timeout fails the stream.
        let result = tokio::select! {
            biased;
            () = wait_cancel(cancel) => return Err((ProviderError::Cancelled, committed)),
            result = tokio::time::timeout(chunk_timeout, resp.chunk()) => result,
        };
        let chunk = match result {
            Err(_) => return Err((ProviderError::IdleTimeout, committed)),
            Ok(Err(e)) => {
                let error = if e.is_timeout() {
                    ProviderError::Deadline
                } else {
                    ProviderError::Incomplete
                };
                return Err((error, committed));
            }
            Ok(Ok(chunk)) => chunk,
        };
        match chunk {
            None => break,
            Some(bytes) => {
                if bytes.is_empty() {
                    continue;
                }
                let mut forward = |item: &StreamItem| {
                    committed = true;
                    observe(item);
                };
                let result = parser.push_observed(&bytes, &mut forward);
                committed |= parser.events > 0 || parser.terminal;
                let mut fresh = result.map_err(|e| (e, committed))?;
                items.append(&mut fresh);
                if parser.completed {
                    break;
                }
            }
        }
    }
    let tail = parser.finish().map_err(|e| (e, committed))?;
    for item in &tail {
        observe(item);
    }
    items.extend(tail);
    if !parser.completed {
        return Err((ProviderError::Incomplete, committed));
    }

    let mut text = String::new();
    let mut usage = None;
    for item in &items {
        match item {
            StreamItem::TextDelta(delta) => text.push_str(delta),
            // First terminal marker wins; replayed `done` events never
            // overwrite committed outcomes or re-execute anything.
            StreamItem::Usage {
                input_tokens,
                output_tokens,
            } => {
                if usage.is_none() {
                    usage = Some((*input_tokens, *output_tokens));
                }
            }
            _ => {}
        }
    }
    let output = parser
        .output
        .unwrap_or_else(|| parser.output_done.into_values().collect());
    Ok(Generation {
        output,
        items,
        text,
        usage,
    })
}

/// Redacted request preview for diagnostics (auth/contents withheld).
pub fn describe_request(
    config: &ResponsesConfig,
    prompt_len: usize,
    tools: usize,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    out.insert(
        "url".to_string(),
        config.generation_url().unwrap_or_else(|_| "?".to_string()),
    );
    out.insert("auth".to_string(), "Bearer ***".to_string());
    out.insert("prompt_bytes".to_string(), prompt_len.to_string());
    out.insert("tools".to_string(), tools.to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::{
        CHUNK_TIMEOUT_MS, EVENT_CAP, ProviderError, ResponsesConfig, SseParser, StreamItem,
        ToolDef, prompt_cache_key, request_body, stream_generation,
    };
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// What the test server sends back.
    #[derive(Clone)]
    struct Action {
        status: &'static str,
        headers: Vec<(&'static str, String)>,
        chunks: Vec<(Vec<u8>, u64)>,
        abort_after: Option<usize>,
    }

    #[derive(Debug, Clone)]
    struct Seen {
        method: String,
        path: String,
        auth: Option<String>,
        body: Vec<u8>,
        headers: BTreeMap<String, String>,
    }

    struct TestServer {
        base: String,
        seen: Arc<Mutex<Vec<Seen>>>,
        attempts: Arc<AtomicUsize>,
        handle: tokio::task::JoinHandle<()>,
    }

    impl TestServer {
        async fn spawn(behavior: Arc<dyn Fn(usize) -> Action + Send + Sync>) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
            let attempts = Arc::new(AtomicUsize::new(0));
            let seen_task = seen.clone();
            let attempts_task = attempts.clone();
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    let seen = seen_task.clone();
                    let attempts = attempts_task.clone();
                    let behavior = behavior.clone();
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                        let mut head = Vec::new();
                        let mut buf = [0u8; 4096];
                        loop {
                            match sock.read(&mut buf).await {
                                Ok(0) => return,
                                Ok(n) => {
                                    head.extend_from_slice(&buf[..n]);
                                    if head.len() > 1 << 20
                                        || head.windows(4).any(|w| w == b"\r\n\r\n")
                                    {
                                        break;
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                        let text = String::from_utf8_lossy(&head).into_owned();
                        let mut lines = text.lines();
                        let request_line = lines.next().unwrap_or("");
                        let mut parts = request_line.split_whitespace();
                        let method = parts.next().unwrap_or("").to_string();
                        let path = parts.next().unwrap_or("/").to_string();
                        let mut content_len = 0usize;
                        let mut auth = None;
                        let mut headers = BTreeMap::new();
                        for line in lines {
                            if line.is_empty() {
                                break;
                            }
                            if let Some((k, v)) = line.split_once(':') {
                                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
                                if k.trim().eq_ignore_ascii_case("content-length") {
                                    content_len = v.trim().parse().unwrap_or(0);
                                }
                                if k.trim().eq_ignore_ascii_case("authorization") {
                                    auth = Some(v.trim().to_string());
                                }
                            }
                        }
                        let mut body = vec![0u8; content_len];
                        let mut read = 0;
                        // Body may already sit in `head` past the header end.
                        if let Some(pos) = head.windows(4).position(|w| w == b"\r\n\r\n") {
                            let have = &head[pos + 4..];
                            let take = have.len().min(content_len);
                            body[..take].copy_from_slice(&have[..take]);
                            read = take;
                        }
                        while read < content_len {
                            match sock.read(&mut body[read..]).await {
                                Ok(0) => break,
                                Ok(n) => read += n,
                                Err(_) => break,
                            }
                        }
                        let n = attempts.fetch_add(1, Ordering::SeqCst);
                        seen.lock().expect("seen").push(Seen {
                            method,
                            path,
                            auth,
                            body,
                            headers,
                        });
                        let action = behavior(n);
                        let mut head_out =
                            format!("HTTP/1.1 {}\r\nConnection: close\r\n", action.status);
                        for (k, v) in &action.headers {
                            head_out.push_str(&format!("{k}: {v}\r\n"));
                        }
                        head_out.push_str("\r\n");
                        if sock.write_all(head_out.as_bytes()).await.is_err() {
                            return;
                        }
                        for (i, (chunk, delay)) in action.chunks.iter().enumerate() {
                            if let Some(abort) = action.abort_after
                                && i >= abort
                            {
                                return; // Abrupt close: incomplete stream.
                            }
                            if *delay > 0 {
                                tokio::time::sleep(Duration::from_millis(*delay)).await;
                            }
                            if sock.write_all(chunk).await.is_err() {
                                return;
                            }
                        }
                    });
                }
            });
            Self {
                base,
                seen,
                attempts,
                handle,
            }
        }

        fn shutdown(self) {
            self.handle.abort();
        }
    }

    fn sse_delta(text: &str) -> Vec<u8> {
        format!(
            "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
            serde_json::Value::String(text.to_string())
        )
        .into_bytes()
    }

    fn sse_completed(input: u64, output: u64) -> Vec<u8> {
        format!("data: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"usage\":{{\"input_tokens\":{input},\"output_tokens\":{output}}}}}}}\n\n").into_bytes()
    }

    fn test_config(base: &str) -> ResponsesConfig {
        ResponsesConfig {
            base_url: base.to_string(),
            api_key: "test-key".to_string(),
            timeout: Some(false),
            chunk_timeout_ms: CHUNK_TIMEOUT_MS,
            connect_timeout: Duration::from_secs(5),
            allow_private: true,
            headers: BTreeMap::new(),
            set_cache_key: true,
        }
    }

    static NO_CANCEL: AtomicBool = AtomicBool::new(false);

    fn event(value: serde_json::Value) -> Vec<u8> {
        format!("data: {value}\n\n").into_bytes()
    }

    #[tokio::test]
    async fn aud11_done_fallback_and_unfinished_call_rejection() {
        let call = serde_json::json!({"type":"function_call","id":"fc_A","call_id":"call_B","name":"read","arguments":"{}","status":"completed"});
        for complete in [false, true] {
            let call = call.clone();
            let server = TestServer::spawn(Arc::new(move |_| {
                let mut chunks = vec![(event(serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"fc_A","call_id":"call_B","name":"read"}})), 0)];
                if complete {
                    chunks.push((event(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call})), 0));
                }
                chunks.push((sse_completed(0, 0), 0));
                Action { status: "200 OK", headers: vec![], chunks, abort_after: None }
            })).await;
            let result = stream_generation(
                &test_config(&server.base),
                "m",
                None,
                "x",
                &[],
                &NO_CANCEL,
                None,
            )
            .await;
            if complete {
                assert_eq!(
                    result.expect("done fallback").output,
                    vec![
                        serde_json::json!({"type":"function_call","id":"fc_A","call_id":"call_B","name":"read","arguments":"{}","status":"completed"})
                    ]
                );
            } else {
                assert_eq!(result, Err(ProviderError::ResponseIncomplete));
            }
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            server.shutdown();
        }
    }

    #[tokio::test]
    async fn aud11_terminal_failures_never_retry_and_success_does_not_wait_for_eof() {
        for (kind, error) in [
            ("response.failed", ProviderError::Failed),
            ("response.incomplete", ProviderError::ResponseIncomplete),
        ] {
            let server = TestServer::spawn(Arc::new(move |_| Action {
                status: "200 OK",
                headers: vec![],
                chunks: vec![(
                    event(
                        serde_json::json!({"type":kind,"response":{"error":{"message":"SECRET"}}}),
                    ),
                    0,
                )],
                abort_after: None,
            }))
            .await;
            let result = stream_generation(
                &test_config(&server.base),
                "m",
                None,
                "x",
                &[],
                &NO_CANCEL,
                None,
            )
            .await;
            assert_eq!(result, Err(error));
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            assert!(!format!("{result:?}").contains("SECRET"));
            server.shutdown();
        }
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![],
            chunks: vec![(sse_completed(1, 2), 0), (b"garbage".to_vec(), 5000)],
            abort_after: None,
        }))
        .await;
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            stream_generation(
                &test_config(&server.base),
                "m",
                None,
                "x",
                &[],
                &NO_CANCEL,
                None,
            ),
        )
        .await;
        assert!(result.expect("terminal must return before EOF").is_ok());
        server.shutdown();
    }

    #[tokio::test]
    async fn aud09_aud10_canonical_wire_and_output() {
        use super::{InputContent, InputItem, InputRole, stream_input_observed};
        let output = vec![
            serde_json::json!({"type":"reasoning", "id":"rs_A", "encrypted_content":"opaque", "summary":[]}),
            serde_json::json!({"type":"message", "id":"msg_A", "role":"assistant", "status":"completed", "phase":"commentary", "content":[{"type":"output_text","text":"inspect","annotations":[]}]}),
            serde_json::json!({"type":"function_call", "id":"fc_A", "call_id":"call_B", "name":"read", "arguments":"{}", "status":"completed"}),
        ];
        let terminal_output = output.clone();
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "200 OK", headers: vec![], chunks: vec![
                (event(serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"fc_A","call_id":"call_B","name":"read"}})), 0),
                (event(serde_json::json!({"type":"response.completed","response":{"status":"completed","output":terminal_output}})), 0),
            ], abort_after: None,
        })).await;
        let mut config = test_config(&server.base);
        config.set_cache_key = false;
        config.headers = BTreeMap::from([
            ("X-Trace".into(), "private-extra-value".into()),
            ("aUtHoRiZaTiOn".into(), "wrong".into()),
            ("ACCEPT".into(), "wrong".into()),
            ("Content-Type".into(), "wrong".into()),
        ]);
        let mut input = vec![
            InputItem::message(InputRole::System, "system"),
            InputItem::message(InputRole::Developer, "developer"),
            InputItem::message(InputRole::User, "user"),
            InputItem::Message {
                role: InputRole::User,
                content: vec![InputContent::InputImage {
                    image_url: "data:image/png;base64,AA==".into(),
                    detail: Some("low".into()),
                }],
            },
        ];
        input.extend(output.iter().cloned().map(InputItem::ProviderOutput));
        input.push(InputItem::FunctionCallOutput {
            call_id: "call_B".into(),
            output: "tool result".into(),
        });
        let restored: Vec<InputItem> =
            serde_json::from_slice(&serde_json::to_vec(&input).expect("serialize"))
                .expect("deserialize");
        assert_eq!(
            restored, input,
            "opaque/phase-bearing output survives restart serialization"
        );
        let generation = stream_input_observed(
            &config,
            "unknown-model",
            None,
            &input,
            &tools(),
            789,
            &NO_CANCEL,
            &mut |_| {},
        )
        .await
        .expect("generation");
        assert_eq!(generation.output, output);
        assert!(generation.items.iter().any(|i| matches!(i, StreamItem::ToolCallStarted { item_id, call_id, .. } if item_id == "fc_A" && call_id == "call_B")));
        let seen = server.seen.lock().expect("seen");
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("body");
        assert_eq!(body["input"][3]["content"][0]["type"], "input_image");
        assert_eq!(body["input"][4], output[0]);
        assert_eq!(body["input"][5], output[1]);
        assert_eq!(body["input"][6], output[2]);
        assert_eq!(
            body["input"][7],
            serde_json::json!({"type":"function_call_output","call_id":"call_B","output":"tool result"})
        );
        assert_eq!(body["max_output_tokens"], 789);
        assert_eq!(
            body["include"],
            serde_json::json!(["reasoning.encrypted_content"])
        );
        assert!(body.get("prompt_cache_key").is_none());
        assert_eq!(seen[0].headers["x-trace"], "private-extra-value");
        assert_eq!(seen[0].headers["authorization"], "Bearer test-key");
        assert_eq!(seen[0].headers["accept"], "text/event-stream");
        assert_eq!(seen[0].headers["content-type"], "application/json");
        assert_eq!(seen[0].headers["user-agent"], crate::USER_AGENT);
        assert!(!format!("{config:?}").contains("private-extra-value"));
        drop(seen);
        server.shutdown();
    }

    #[tokio::test]
    async fn sequential_reasoning_done_items_preserve_order_and_opaque_payload() {
        use super::stream_input_observed;
        let items = vec![
            serde_json::json!({"type":"reasoning", "id":"rs_1", "encrypted_content":"private-1"}),
            serde_json::json!({"type":"reasoning", "id":"rs_2", "encrypted_content":"private-2"}),
        ];
        let output = items.clone();
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "200 OK", headers: vec![], chunks: vec![
                (event(serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"First"})), 0),
                (event(serde_json::json!({"type":"response.output_item.done","item":items[0]})), 0),
                (event(serde_json::json!({"type":"response.reasoning_summary_text.delta","delta":"Second"})), 0),
                (event(serde_json::json!({"type":"response.output_item.done","item":items[1]})), 0),
                (event(serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})), 0),
            ], abort_after: None,
        })).await;
        let mut seen = Vec::new();
        let generation = stream_input_observed(
            &test_config(&server.base),
            "m",
            None,
            &[],
            &[],
            500,
            &NO_CANCEL,
            &mut |item| seen.push(item.clone()),
        )
        .await
        .unwrap();
        assert_eq!(seen, generation.items);
        assert!(
            matches!(&seen[..], [StreamItem::ReasoningDelta(a), StreamItem::OpaqueItem { item_id: first, payload: one }, StreamItem::ReasoningDelta(b), StreamItem::OpaqueItem { item_id: second, payload: two }]
            if a == "First" && b == "Second" && first == "rs_1" && second == "rs_2"
                && one["encrypted_content"] == "private-1" && two["encrypted_content"] == "private-2")
        );
        assert_eq!(generation.output.len(), 2);
        server.shutdown();
    }

    #[tokio::test]
    async fn aud13_effective_idle_timeout_400_and_unsafe_headers() {
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![],
            chunks: vec![(sse_delta("first"), 0), (sse_completed(0, 0), 5000)],
            abort_after: None,
        }))
        .await;
        let mut config = test_config(&server.base);
        config.chunk_timeout_ms = 30;
        let start = std::time::Instant::now();
        assert_eq!(
            super::stream_input_observed(&config, "m", None, &[], &[], 5, &NO_CANCEL, &mut |_| {})
                .await,
            Err(ProviderError::IdleTimeout)
        );
        assert!(start.elapsed() < Duration::from_millis(500));
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "400 Bad Request",
            headers: vec![],
            chunks: vec![],
            abort_after: None,
        }))
        .await;
        let mut config = test_config(&server.base);
        assert_eq!(
            stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None).await,
            Err(ProviderError::HttpStatus(400))
        );
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        config.headers.insert("hOsT".into(), "evil".into());
        assert_eq!(
            stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None).await,
            Err(ProviderError::InvalidConfig)
        );
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }

    #[tokio::test]
    async fn aud13_byte_bounds_utf8_and_incremental_observer() {
        use super::{ARGUMENT_BYTE_CAP, SSE_BYTE_CAP, TEXT_DELTA_BYTE_CAP};
        let mut parser = SseParser::default();
        assert_eq!(
            parser.push(&vec![b'x'; SSE_BYTE_CAP + 1]),
            Err(ProviderError::ByteLimit("SSE line"))
        );
        assert!(parser.pending.is_empty());
        let mut parser = SseParser::default();
        let line = format!("data: {}\n", " ".repeat(SSE_BYTE_CAP / 2));
        parser.push(line.as_bytes()).expect("half event");
        assert_eq!(
            parser.push(line.as_bytes()),
            Err(ProviderError::ByteLimit("SSE event"))
        );
        let mut parser = SseParser::default();
        assert_eq!(
            parser.push(b"data: \xff\n\n"),
            Err(ProviderError::InvalidUtf8)
        );
        let mut parser = SseParser::default();
        let args = event(
            serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"fc_A","delta":"a".repeat(ARGUMENT_BYTE_CAP)}),
        );
        parser.push(&args).expect("at cap");
        assert_eq!(parser.push(&event(serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"fc_A","delta":"b"}))), Err(ProviderError::ByteLimit("arguments")));
        let mut parser = SseParser::default();
        let mut text = String::new();
        let expected = "🌍".repeat(TEXT_DELTA_BYTE_CAP);
        parser
            .push_observed(&sse_delta(&expected), &mut |item| {
                if let StreamItem::TextDelta(delta) = item {
                    assert!(delta.len() <= TEXT_DELTA_BYTE_CAP);
                    text.push_str(delta);
                }
            })
            .expect("bounded observer");
        assert_eq!(text, expected);
        assert!(!parser.completed, "observed before terminal");
        let mut parser = SseParser::default();
        let large = sse_delta(&"x".repeat(256 * 1024));
        let error = (0..100).find_map(|_| parser.push(&large).err());
        assert_eq!(error, Some(ProviderError::ByteLimit("generation")));
        assert!(parser.retained <= super::GENERATION_BYTE_CAP);
        let oversized = "x".repeat(super::REQUEST_BYTE_CAP + 1);
        assert_eq!(
            super::stream_input_observed(
                &test_config("http://127.0.0.1:9"),
                "m",
                None,
                &[super::InputItem::message(super::InputRole::User, oversized)],
                &[],
                100,
                &NO_CANCEL,
                &mut |_| {},
            )
            .await,
            Err(ProviderError::ByteLimit("request"))
        );
    }

    #[tokio::test]
    async fn aud11_complete_text_event_eof_is_not_success() {
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![],
            chunks: vec![(sse_delta("not completed"), 0)],
            abort_after: None,
        }))
        .await;
        let result = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await;
        assert_eq!(result, Err(ProviderError::Incomplete));
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }

    #[tokio::test]
    async fn aud12_cancel_silent_body_and_headers() {
        use tokio::io::AsyncWriteExt as _;
        for send_headers in [false, true] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let config = test_config(&format!("http://{}", listener.local_addr().expect("addr")));
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.expect("accept");
                if send_headers {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n")
                        .await
                        .expect("headers");
                    socket.write_all(&sse_delta("first")).await.expect("delta");
                }
                let _ = ready_tx.send(());
                tokio::time::sleep(Duration::from_secs(10)).await;
            });
            let cancel = AtomicBool::new(false);
            let trigger = async {
                ready_rx.await.expect("ready");
                tokio::time::sleep(Duration::from_millis(50)).await;
                cancel.store(true, Ordering::Relaxed);
            };
            let request = async {
                tokio::time::timeout(
                    Duration::from_millis(500),
                    stream_generation(&config, "m", None, "x", &[], &cancel, None),
                )
                .await
            };
            let (result, ()) = tokio::join!(request, trigger);
            server.abort();
            assert_eq!(
                result,
                Ok(Err(ProviderError::Cancelled)),
                "headers={send_headers}"
            );
        }
    }

    fn tools() -> Vec<ToolDef> {
        vec![ToolDef {
            name: "read".to_string(),
            description: "read a file".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }]
    }

    #[tokio::test]
    async fn prov01_exact_url_auth_store_cachekey() {
        let behavior: Arc<dyn Fn(usize) -> Action + Send + Sync> = Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![(sse_delta("hi"), 0), (sse_completed(3, 2), 0)],
            abort_after: None,
        });
        // Base with a /v1 prefix: exactly one /responses suffix, no doubling.
        let server = TestServer::spawn(behavior).await;
        let config = ResponsesConfig {
            base_url: format!("{}/v1", server.base),
            ..test_config(&server.base)
        };
        let generation = stream_generation(&config, "m", None, "hi", &tools(), &NO_CANCEL, None)
            .await
            .expect("stream");
        assert_eq!(generation.text, "hi");
        assert_eq!(generation.usage, Some((3, 2)));
        let seen = server.seen.lock().expect("seen").clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, "POST");
        assert!(
            seen[0].path.ends_with("/v1/responses"),
            "path={}",
            seen[0].path
        );
        assert!(!seen[0].path.contains("/v1/v1"));
        assert_eq!(seen[0].auth.as_deref(), Some("Bearer test-key"));
        let body: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("json body");
        assert_eq!(body["store"], false);
        assert_eq!(body["input"][0]["content"], "hi");
        assert_eq!(body["tools"][0]["name"], "read");
        let key1 = body["prompt_cache_key"].as_str().expect("key").to_string();
        // Stable key: identical logical request reproduces it byte-for-byte.
        let server2 = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![(sse_completed(0, 0), 0)],
            abort_after: None,
        }))
        .await;
        let config2 = ResponsesConfig {
            base_url: server2.base.clone(),
            ..test_config(&server2.base)
        };
        stream_generation(&config2, "m", None, "hi", &tools(), &NO_CANCEL, None)
            .await
            .expect("s2");
        let body2: serde_json::Value =
            serde_json::from_slice(&server2.seen.lock().expect("s").clone()[0].body).expect("b2");
        assert_eq!(body2["prompt_cache_key"].as_str(), Some(key1.as_str()));
        assert_ne!(prompt_cache_key("other", &tools()), key1);
        // Trailing-slash base is tolerated without doubling.
        assert!(
            ResponsesConfig {
                base_url: "https://x.invalid/v1/".to_string(),
                ..test_config("https://x.invalid")
            }
            .generation_url()
            .expect("url")
            .ends_with("/v1/responses")
        );
        server.shutdown();
        server2.shutdown();
    }

    #[test]
    fn prov02_splits_crlf_comments_usage_cap() {
        // One logical stream, fed whole vs 1-byte pieces vs odd chunks.
        let wire = [
            ": heartbeat\n".to_string(),
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"a\"}\r\n\r\n".to_string(),
            "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"i1\",\"delta\":\"{\\\"a\\\"\"}\n\n".to_string(),
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[{\"type\":\"function_call\",\"id\":\"i1\",\"call_id\":\"call1\",\"name\":\"read\",\"arguments\":\"{\\\"a\\\":1}\",\"status\":\"completed\"}],\"usage\":{\"input_tokens\":10,\"output_tokens\":20}}}\n\n".to_string(),
        ]
        .concat();
        let mut whole = SseParser::default();
        let mut expected = whole.push(wire.as_bytes()).expect("parse");
        expected.append(&mut whole.finish().expect("finish"));

        for chunk_size in [1usize, 3, 7] {
            let mut parser = SseParser::default();
            let mut got = Vec::new();
            for chunk in wire.as_bytes().chunks(chunk_size) {
                got.append(&mut parser.push(chunk).expect("chunk"));
            }
            got.append(&mut parser.finish().expect("finish"));
            assert_eq!(got, expected, "chunk_size={chunk_size}");
        }
        assert!(expected.contains(&StreamItem::TextDelta("a".to_string())));
        assert!(expected.contains(&StreamItem::ArgDelta {
            item_id: "i1".to_string(),
            delta: "{\"a\"".to_string()
        }));
        assert!(expected.contains(&StreamItem::Usage {
            input_tokens: 10,
            output_tokens: 20
        }));

        // Split multibyte UTF-8 across pushes.
        let emoji = "🌍";
        let bytes =
            format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"{emoji}\"}}\n\n")
                .into_bytes();
        let mut parser = SseParser::default();
        let mut got = Vec::new();
        for chunk in bytes.chunks(2) {
            got.append(&mut parser.push(chunk).expect("emoji"));
        }
        got.append(&mut parser.finish().expect("finish"));
        assert_eq!(got, vec![StreamItem::TextDelta(emoji.to_string())]);

        // Event cap enforced.
        let mut parser = SseParser::default();
        let mut over = false;
        for _ in 0..=EVENT_CAP + 5 {
            let items = parser.push(b"data: {\"type\":\"x\"}\n\n");
            if items == Err(ProviderError::EventCap) {
                over = true;
                break;
            }
            let _ = items.expect("parse");
        }
        assert!(over);
    }

    #[test]
    fn sse_event_data_flush_split() {
        // Live gateways flush `event:` and `data:` in separate chunks; a
        // chunk ending right after an `event:` line must not dispatch a
        // data-less event (T16 live finding: spurious Incomplete).
        let mut parser = SseParser::default();
        let first = parser
            .push(b"event: response.output_text.delta\n")
            .expect("event flush");
        assert!(first.is_empty(), "no data-less dispatch");
        let second = parser
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n")
            .expect("data flush");
        assert_eq!(second, vec![StreamItem::TextDelta("hi".to_string())]);
        let tail = parser.finish().expect("finish");
        assert!(tail.is_empty());

        // Same bytes one-per-chunk stay identical.
        let wire = b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";
        let mut split = SseParser::default();
        let mut got = Vec::new();
        for chunk in wire.chunks(1) {
            got.append(&mut split.push(chunk).expect("byte"));
        }
        got.append(&mut split.finish().expect("finish"));
        assert_eq!(got, vec![StreamItem::TextDelta("hi".to_string())]);
    }

    #[tokio::test]
    async fn prov06_statuses_retry_owner() {
        // 401/403 typed, never retried.
        for (status, error) in [
            ("401 Unauthorized", ProviderError::Unauthorized),
            ("403 Forbidden", ProviderError::Forbidden),
        ] {
            let server = TestServer::spawn(Arc::new(move |_| Action {
                status,
                headers: vec![],
                chunks: vec![],
                abort_after: None,
            }))
            .await;
            let config = test_config(&server.base);
            let err = stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None)
                .await
                .expect_err("err");
            assert_eq!(err, error);
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            server.shutdown();
        }
        // 429 then success: exactly one retry owned here.
        let server = TestServer::spawn(Arc::new(|n| {
            if n == 0 {
                Action {
                    status: "429 Too Many Requests",
                    headers: vec![],
                    chunks: vec![],
                    abort_after: None,
                }
            } else {
                Action {
                    status: "200 OK",
                    headers: vec![("Content-Type", "text/event-stream".to_string())],
                    chunks: vec![(sse_delta("ok"), 0), (sse_completed(0, 0), 0)],
                    abort_after: None,
                }
            }
        }))
        .await;
        let generation = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect("retry ok");
        assert_eq!(generation.text, "ok");
        assert_eq!(server.attempts.load(Ordering::SeqCst), 2);
        server.shutdown();
        // Truncated stream (EOF, no events) then success: pre-commit
        // Incomplete retries once; a committed truncation never retries.
        let server = TestServer::spawn(Arc::new(|n| {
            if n == 0 {
                Action {
                    status: "200 OK",
                    headers: vec![("Content-Type", "text/event-stream".to_string())],
                    chunks: vec![],
                    abort_after: None,
                }
            } else {
                Action {
                    status: "200 OK",
                    headers: vec![("Content-Type", "text/event-stream".to_string())],
                    chunks: vec![(sse_delta("ok"), 0), (sse_completed(0, 0), 0)],
                    abort_after: None,
                }
            }
        }))
        .await;
        let generation = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect("incomplete retry ok");
        assert_eq!(generation.text, "ok");
        assert_eq!(server.attempts.load(Ordering::SeqCst), 2);
        server.shutdown();
        // Persistent 500: initial + one retry, then Server (never more).
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "500 Internal Error",
            headers: vec![],
            chunks: vec![],
            abort_after: None,
        }))
        .await;
        let err = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect_err("500");
        assert_eq!(err, ProviderError::Server);
        assert_eq!(server.attempts.load(Ordering::SeqCst), 2);
        server.shutdown();
        // Failure after a committed delta: Incomplete, never retried.
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![
                (sse_delta("part"), 0),
                (b"data: {\"type\":\"broken".to_vec(), 0),
            ],
            abort_after: Some(2),
        }))
        .await;
        let err = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect_err("partial");
        assert_eq!(err, ProviderError::Incomplete);
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }

    #[tokio::test]
    async fn prov07_timeouts_and_cancel() {
        assert_eq!(CHUNK_TIMEOUT_MS, 6_000_000);
        // Idle gap beyond the override budget fails; timeout:false keeps no
        // total deadline (slow-but-chatting streams succeed).
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![(sse_delta("a"), 0), (sse_delta("b"), 300)],
            abort_after: None,
        }))
        .await;
        let err = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            Some(Duration::from_millis(100)),
        )
        .await
        .expect_err("idle");
        assert_eq!(err, ProviderError::IdleTimeout);
        server.shutdown();

        // Cancel mid-stream drops the connection and reports Cancelled.
        let server = TestServer::spawn(Arc::new(|_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![(sse_delta("a"), 50), (sse_delta("b"), 2000)],
            abort_after: None,
        }))
        .await;
        let config = test_config(&server.base);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            flag.store(true, Ordering::Relaxed);
        });
        let outcome = stream_generation(
            &config,
            "m",
            None,
            "x",
            &[],
            &cancel,
            Some(Duration::from_secs(30)),
        )
        .await;
        assert_eq!(outcome, Err(ProviderError::Cancelled));
        server.shutdown();
        // Closed port with private allowance: fast transport error, no hang.
        let mut closed = test_config("http://127.0.0.1");
        closed.base_url = "http://127.0.0.1:9".to_string();
        closed.connect_timeout = Duration::from_millis(500);
        let start = std::time::Instant::now();
        let err = stream_generation(&closed, "m", None, "x", &[], &NO_CANCEL, None)
            .await
            .expect_err("closed");
        assert_eq!(err, ProviderError::Transport);
        assert!(start.elapsed() < Duration::from_secs(10));
        // timeout:true is meaningless: rejected as invalid config.
        let mut bad = test_config("http://127.0.0.1:9");
        bad.timeout = Some(true);
        assert_eq!(
            stream_generation(&bad, "m", None, "x", &[], &NO_CANCEL, None).await,
            Err(ProviderError::InvalidConfig)
        );
    }

    #[test]
    fn units_request_shape() {
        let body = request_body("m", None, "hi", &tools());
        assert_eq!(body["model"], "m");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("reasoning").is_none(), "no guessed effort");
        assert!(body["prompt_cache_key"].as_str().is_some());
        let debug = format!("{:?}", test_config("https://x.invalid"));
        assert!(!debug.contains("test-key"));
    }
}
