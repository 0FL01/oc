//! OpenProxy native Responses adapter for T12 (PROV01/02/06/07).
//!
//! Exact generation URL (configured prefix preserved + `/responses`, never a
//! blind `/v1` add/remove), `Bearer` auth, `store:false`, stable
//! `prompt_cache_key`, and ordinary function tool schemas — no OAuth, no API
//! fallback. Bounded incremental SSE (arbitrary byte splits incl. split
//! UTF-8, multiline `data:`, CRLF, comments), text/argument deltas and usage
//! metadata with event/byte caps. Errors distinguish failed/incomplete/EOF;
//! exactly one physical attempt lives here. Logical retry/continuation belongs
//! to the runtime owner; this adapter never repeats generation or tool calls.
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

pub(crate) mod chat;
pub(crate) mod context;
mod failure;
pub(crate) mod messages;
pub(crate) mod protocol;
pub(crate) mod settings;
pub(crate) mod websocket;
pub use failure::{Delivery, FailureKind, Operation, PhysicalFailure, RetryHeaders, TransportKind};

#[cfg(test)]
#[path = "provider/timeout_tests.rs"]
mod timeout_tests;

/// Idle budget between SSE bytes (6 000 000 ms = 100 min, not 6 s).
pub const CHUNK_TIMEOUT_MS: u64 = 6_000_000;
/// Max SSE events decoded per response.
pub const EVENT_CAP: usize = 10_000;
/// Physical attempts per adapter invocation (no internal HTTP retry).
pub const MAX_ATTEMPTS: usize = 1;
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
    /// The owning operation could not persist its dispatch accounting.
    #[error("generation dispatch accounting unavailable")]
    DispatchRefused,
    /// One physical provider request, with bounded classified wire facts.
    #[error("{0}")]
    Request(Box<PhysicalFailure>),
    /// Invalid local protocol decoding/validation, never generic provider retry.
    #[error("invalid provider output")]
    InvalidOutput,
    /// Bounded local reconciliation facts, never provider/transport retry facts.
    #[error("invalid provider output ({stage:?}/{code:?})")]
    OutputStructure {
        stage: OutputStage,
        code: OutputCode,
    },
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
    /// Bounded structured provider diagnostic with configured credentials redacted.
    #[error("HTTP status {status}: {message}")]
    HttpDiagnostic { status: u16, message: String },
    /// Byte ceiling reached before appending data.
    #[error("{0} byte limit exceeded")]
    ByteLimit(&'static str),
    /// Malformed UTF-8 is never silently replaced.
    #[error("invalid UTF-8 in stream")]
    InvalidUtf8,
    /// An explicit provider context-window error, eligible for one checkpoint rebuild.
    #[error("context window exceeded")]
    ContextOverflow,
    /// Admitted content this wire cannot carry; refused before dispatch.
    #[error("unsupported content modality for this wire")]
    UnsupportedModality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStage {
    Decode,
    Added,
    Done,
    Completion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCode {
    InvalidJson,
    InvalidField,
    InvalidIndex,
    InvalidStatus,
    InvalidArguments,
    IdentityConflict,
    IndexConflict,
    MissingDone,
}

fn structural(stage: OutputStage, code: OutputCode) -> ProviderError {
    ProviderError::OutputStructure { stage, code }
}

impl ProviderError {
    /// Existing compaction path consumes the typed context-overflow category.
    pub fn is_context_overflow(&self) -> bool {
        matches!(self, Self::ContextOverflow)
            || matches!(self, Self::Request(failure) if failure.kind == FailureKind::ContextOverflow)
    }

    /// Truthful incomplete status for existing callers, without retry dispatch.
    pub fn is_incomplete(&self) -> bool {
        matches!(self, Self::Incomplete | Self::ResponseIncomplete)
            || matches!(self, Self::Request(failure) if failure.kind == FailureKind::IncompleteStream)
    }
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
    /// Runtime-owned chronological selection fact, never a provider output or UI text.
    EffortUpdate {
        event_seq: i64,
        effort: Option<String>,
        previous: Option<String>,
    },
    /// A typed message.
    Message {
        role: InputRole,
        content: Vec<InputContent>,
    },
    /// A tool result linked to the function call's call_id, never its item id.
    FunctionCallOutput { call_id: String, output: String },
    /// Runtime-owned MCP result. Serialized as standard Responses output; native
    /// facts are attached by TurnLog, never inferred by this deserializer.
    #[serde(rename = "function_call_output")]
    McpFunctionCallOutput {
        call_id: String,
        output: crate::mcp_result::McpToolOutput,
    },
    /// Native validated local read image, with separate local-source provenance.
    #[serde(rename = "function_call_output")]
    ReadFunctionCallOutput {
        call_id: String,
        output: crate::tools::read::ReadToolOutput,
    },
    /// Complete output item, including opaque fields and assistant phase.
    #[serde(untagged)]
    ProviderOutput(serde_json::Value),
}

impl<'de> Deserialize<'de> for InputItem {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if value["type"] == "effort_update" {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Marker {
                #[serde(rename = "type")]
                _kind: String,
                event_seq: i64,
                effort: Option<String>,
                previous: Option<String>,
            }
            let marker: Marker = serde_json::from_value(value)
                .map_err(|_| serde::de::Error::custom("invalid effort update"))?;
            if marker.event_seq <= 0
                || [&marker.effort, &marker.previous]
                    .into_iter()
                    .flatten()
                    .any(|s| s.is_empty() || s.chars().any(char::is_control))
            {
                return Err(serde::de::Error::custom("invalid effort update"));
            }
            return Ok(Self::EffortUpdate {
                event_seq: marker.event_seq,
                effort: marker.effort,
                previous: marker.previous,
            });
        }
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
            if let Some(output) = value["output"].as_str() {
                return Ok(Self::FunctionCallOutput {
                    call_id,
                    output: output.into(),
                });
            }
            // Provider-authored arrays remain opaque, never native MCP facts.
            if value["output"].is_array() {
                return Ok(Self::ProviderOutput(value));
            }
            return Err(serde::de::Error::custom("missing output"));
        }
        Ok(Self::ProviderOutput(value))
    }
}

impl InputItem {
    /// Exact tool-result graph identity for either legacy or native MCP output.
    pub(crate) fn call_output(&self) -> Option<(&str, &str)> {
        match self {
            Self::FunctionCallOutput { call_id, output } => Some((call_id, output)),
            Self::McpFunctionCallOutput { call_id, output } => Some((call_id, output.display())),
            Self::ReadFunctionCallOutput { call_id, output } => Some((call_id, output.display())),
            _ => None,
        }
    }

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
    /// Explicit terminal meaning; length never implies an untruncated answer.
    pub finish: FinishReason,
    pub compaction_usage: Option<oc_core::compaction::CompactionUsage>,
    /// Full completed output, including opaque continuation state.
    pub output: Vec<serde_json::Value>,
    /// Ordered stream items (deltas in arrival order).
    pub items: Vec<StreamItem>,
    /// Full model text (concatenated deltas).
    pub text: String,
    /// Usage when reported, else `None`.
    pub usage: Option<(u64, u64)>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    #[default]
    Stop,
    Length,
}

/// Adapter configuration (secret `api_key` never appears in `Debug`).
#[derive(Clone, PartialEq, Eq)]
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
    /// Enable supported cache fields using captured session/fork lineage.
    pub set_cache_key: bool,
    /// Admitted wire protocol and per-model compatibility facts.
    pub wire: WireBinding,
}

/// Immutable admitted wire binding: protocol plus explicit Chat facts by model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WireBinding {
    pub(crate) selection_scope: Option<String>,
    pub(crate) provider_state: Option<crate::composition::ProviderState>,
    pub(crate) providers: BTreeMap<String, CapturedProvider>,
    pub(crate) auth_input: Option<AuthInput>,
    pub(crate) openai: Option<crate::auth::OpenAiBinding>,
    pub(crate) transport: Option<websocket::Transport>,
    pub(crate) channels: Option<websocket::Channels>,
    pub(crate) chronology: BTreeMap<String, protocol::Chronology>,
    pub(crate) total_timeout_ms: Option<u64>,
    pub(crate) go: bool,
    pub(crate) context: Option<context::RequestContext>,
    pub(crate) unsupported: bool,
    pub(crate) requests: BTreeMap<(String, Option<String>), ResponsesConfig>,
    pub(crate) api_model: Option<String>,
    pub(crate) settings: settings::WireSettings,
    pub(crate) endpoint: Option<crate::endpoint::EndpointBinding>,
    pub(crate) auth_policy: crate::auth::AuthPolicy,
    pub(crate) protocol: protocol::Protocol,
    pub(crate) chat: BTreeMap<String, chat::ChatCompat>,
    pub(crate) messages_bearer: bool,
    /// Opt-in qualification only; absent from production builds and JSON config.
    #[cfg(test)]
    pub(crate) live_campaign: Option<std::path::PathBuf>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct CapturedProvider {
    pub(crate) catalog: crate::models::ModelCatalog,
    pub(crate) config: ResponsesConfig,
}

impl std::fmt::Debug for CapturedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapturedProvider").finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct AuthInput {
    policy: crate::auth::AuthPolicy,
    key: String,
    base_url: String,
    endpoint: Option<crate::endpoint::EndpointBinding>,
    unsupported: bool,
}
impl std::fmt::Debug for AuthInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthInput")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl WireBinding {
    /// Chat Completions binding with explicit per-model compatibility facts.
    pub(crate) fn chat(chat: BTreeMap<String, chat::ChatCompat>) -> Self {
        Self {
            protocol: protocol::Protocol::Chat,
            chat,
            ..Self::default()
        }
    }

    pub(crate) fn messages(bearer: bool) -> Self {
        Self {
            protocol: protocol::Protocol::Messages,
            messages_bearer: bearer,
            ..Self::default()
        }
    }
}

impl std::fmt::Debug for ResponsesConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponsesConfig")
            .field("base_url", &"[configured]")
            .field("api_key", &"***")
            .field("timeout", &self.timeout)
            .field("chunk_timeout_ms", &self.chunk_timeout_ms)
            .finish()
    }
}

impl ResponsesConfig {
    pub(crate) fn subscription(&self) -> bool {
        self.wire
            .openai
            .as_ref()
            .is_some_and(|binding| binding.subscription)
    }
    /// Restore configured inputs, never reuse a previously resolved account key.
    pub(crate) fn restore_auth_input(&mut self) {
        let input = self.wire.auth_input.get_or_insert_with(|| AuthInput {
            policy: self.wire.auth_policy,
            key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            endpoint: self.wire.endpoint.clone(),
            unsupported: self.wire.unsupported,
        });
        self.api_key = input.key.clone();
        self.wire.auth_policy = input.policy;
        self.base_url = input.base_url.clone();
        self.wire.endpoint = input.endpoint.clone();
        self.wire.openai = None;
        self.wire.unsupported = input.unsupported;
    }
    /// Capture replay authority from the same immutable binding that will send
    /// this request. No session/cache/affinity metadata or plaintext secrets.
    pub(crate) fn provenance(
        &self,
        provider: &str,
        model: &str,
    ) -> Result<oc_core::queries::WireProvenance, ProviderError> {
        use oc_core::queries::{NativeProtocol, WireProvenance};
        use protocol::Protocol;
        let url = reqwest::Url::parse(self.base_url.trim_end_matches('/'))
            .map_err(|_| ProviderError::InvalidConfig)?;
        let headers = request_headers(self)?;
        let headers = headers
            .iter()
            .filter(|(name, _)| {
                self.wire.openai.is_none()
                    || !matches!(name.as_str(), "authorization" | "session-id")
            })
            .map(|(name, value)| (name.as_str(), value.as_bytes()))
            .collect::<BTreeMap<_, _>>();
        let policy = format!("{:?}", self.wire.auth_policy);
        let auth_scope = if let Some(binding) = &self.wire.openai {
            crate::compaction::fingerprint(&(policy, &binding.scope, &headers))
        } else {
            // Preserve existing Go/custom receipt identity byte-for-byte.
            crate::compaction::fingerprint(&(policy, &headers))
        };
        Ok(WireProvenance {
            provider: provider.into(),
            api_model: self.wire.api_model.as_deref().unwrap_or(model).into(),
            protocol: match self.wire.protocol {
                Protocol::Responses => NativeProtocol::Responses,
                Protocol::Chat => NativeProtocol::Chat,
                Protocol::Messages => NativeProtocol::Messages,
            },
            deployment: crate::compaction::fingerprint(&(
                url.as_str(),
                self.wire.endpoint.as_ref().map(|e| e.provenance()),
            )),
            auth_scope,
        })
    }
    /// Capture once per operation; all selected bindings and retries share it.
    pub(crate) fn with_context(&self, context: context::RequestContext) -> Self {
        let mut captured = self.clone();
        captured.wire.context = Some(context.clone());
        for binding in captured.wire.requests.values_mut() {
            binding.wire.context = Some(context.clone());
        }
        captured
    }
    /// Resolve from this immutable generation, never from current UI/storage state.
    pub(crate) fn for_selection(&self, model: &str, variant: Option<&str>) -> &Self {
        self.wire
            .requests
            .get(&(model.to_owned(), variant.map(str::to_owned)))
            .or_else(|| self.wire.requests.get(&(model.to_owned(), None)))
            .unwrap_or(self)
    }
    /// Captured authentication readiness, distinct from an empty required key.
    pub fn auth_ready(&self) -> bool {
        !self.wire.unsupported
            && (self.wire.auth_policy == crate::auth::AuthPolicy::None
                || ((self.wire.auth_policy == crate::auth::AuthPolicy::Key
                    || (self.wire.auth_policy == crate::auth::AuthPolicy::OAuth
                        && self.wire.openai.as_ref().is_some_and(|b| b.subscription)))
                    && !self.api_key.trim().is_empty()))
    }
    /// Exact generation URL: trimmed prefix + `/responses`.
    pub fn generation_url(&self) -> Result<String, ProviderError> {
        let base = self.base_url.trim();
        if base.is_empty() || base.contains(' ') || base.contains('\0') {
            return Err(ProviderError::InvalidConfig);
        }
        Ok(format!("{}/responses", base.trim_end_matches('/')))
    }

    /// Exact Chat Completions URL: trimmed prefix + `/chat/completions`.
    pub(crate) fn chat_url(&self) -> Result<String, ProviderError> {
        let responses = self.generation_url()?;
        Ok(format!(
            "{}/chat/completions",
            responses
                .strip_suffix("/responses")
                .expect("responses suffix")
        ))
    }

    /// Compare actual canonical routing/auth headers, never config spelling or
    /// Debug output. Reserved header overrides share request_headers' authority.
    pub(crate) fn same_request_binding(&self, other: &Self) -> Result<bool, ProviderError> {
        let current = reqwest::Url::parse(&self.generation_url()?)
            .map_err(|_| ProviderError::InvalidConfig)?;
        let next = reqwest::Url::parse(&other.generation_url()?)
            .map_err(|_| ProviderError::InvalidConfig)?;
        Ok(current == next
            && self.wire.endpoint == other.wire.endpoint
            && self.wire.protocol == other.wire.protocol
            && self.wire.api_model == other.wire.api_model
            && request_headers(self)? == request_headers(other)?)
    }
}

/// Profile `request` overlay (donor `agents.<id>.request` headers/body; V1
/// `options`/`temperature`/`top_p` migrate into `body`). Header values are
/// never included in `Debug`; native request fields stay runtime-owned.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RequestOverlay {
    /// Extra lowercase request headers, applied over provider headers.
    pub headers: BTreeMap<String, String>,
    /// Extra top-level JSON body fields.
    pub body: serde_json::Map<String, serde_json::Value>,
}

impl std::fmt::Debug for RequestOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestOverlay")
            .field("headers", &self.headers.keys().collect::<Vec<_>>())
            .field("body", &self.body.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Body fields the native Responses adapter owns; an overlay never replaces them.
pub const RESERVED_BODY_FIELDS: [&str; 18] = [
    "model",
    "store",
    "stream",
    "input",
    "include",
    "max_output_tokens",
    "tools",
    "reasoning",
    "prompt_cache_key",
    "messages",
    "system",
    "max_tokens",
    "max_completion_tokens",
    "thinking",
    "output_config",
    "cache_control",
    "stream_options",
    "reasoning_effort",
];

impl RequestOverlay {
    /// Explicit load-time refusal for routing/auth/framing headers, invalid
    /// header syntax and runtime-owned body fields.
    pub fn validate(&self) -> Result<(), String> {
        use reqwest::header::{HeaderName, HeaderValue};
        for (name, value) in &self.headers {
            let header = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| format!("request.headers.{name}: invalid header name"))?;
            if RESERVED_HEADERS.contains(&header.as_str()) {
                return Err(format!("request.headers.{name}: reserved header"));
            }
            HeaderValue::from_str(value)
                .map_err(|_| format!("request.headers.{name}: invalid header value"))?;
        }
        if let Some(field) = self
            .body
            .keys()
            .find(|key| RESERVED_BODY_FIELDS.contains(&key.as_str()))
        {
            return Err(format!("request.body.{field}: reserved request field"));
        }
        Ok(())
    }

    /// Key-level merge, matching donor `Object.assign` for headers and body.
    pub fn extend(&mut self, other: RequestOverlay) {
        self.headers.extend(other.headers);
        self.body.extend(other.body);
    }

    pub fn is_empty(&self) -> bool {
        self.headers.is_empty() && self.body.is_empty()
    }
}

const RESERVED_HEADERS: [&str; 14] = [
    "authorization",
    "x-api-key",
    "anthropic-version",
    "accept",
    "content-type",
    "host",
    "content-length",
    "transfer-encoding",
    "connection",
    "upgrade",
    "trailer",
    "te",
    "proxy-authorization",
    "proxy-connection",
];

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
#[derive(Debug)]
struct CompletedItem {
    item: serde_json::Value,
    /// A real done/added index, as opposed to the legacy arrival-order fallback.
    indexed: bool,
    /// String ciphertext from a validated reasoning output_item.done, not
    /// optional fields filled by terminal-only recovery.
    emitted_reasoning: bool,
}

#[derive(Debug, Default)]
pub struct SseParser {
    /// Ephemeral request-local redactions, never response/header archives.
    redactions: reqwest::header::HeaderMap,
    finish: FinishReason,
    compaction_usage: Option<oc_core::compaction::CompactionUsage>,
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
    arguments: BTreeMap<String, (usize, Option<serde_json::Value>)>,
    announced_calls: BTreeMap<String, (Option<u64>, String, String)>,
    output_done: BTreeMap<u64, CompletedItem>,
    output: Option<Vec<serde_json::Value>>,
    /// Chat decoding state; `None` decodes Responses events.
    chat: Option<chat::ChatState>,
    messages: Option<messages::MessagesState>,
}

impl SseParser {
    pub(crate) fn messages() -> Self {
        Self {
            messages: Some(Default::default()),
            ..Self::default()
        }
    }
    /// Parser for the Chat wire; framing and caps are shared.
    pub(crate) fn chat(compat: &chat::ChatCompat) -> Self {
        Self {
            chat: Some(chat::ChatState::new(compat)),
            ..Self::default()
        }
    }

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
                for item in self.dispatch()? {
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
            // Chat may end at EOF after its finish reason without `[DONE]`.
            if self.chat.is_some() && !self.completed {
                chat::finalize(self)?;
            }
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

    fn dispatch(&mut self) -> Result<Vec<StreamItem>, ProviderError> {
        if self.data.is_empty() && self.event_name.is_none() {
            return Ok(Vec::new());
        }
        self.events += 1;
        if self.events > EVENT_CAP {
            return Err(ProviderError::EventCap);
        }
        let payload = std::mem::take(&mut self.data);
        self.event_name = None;
        if self.chat.is_some() {
            return chat::dispatch(self, &payload);
        }
        if self.messages.is_some() {
            return messages::dispatch(self, &payload);
        }
        Ok(self.dispatch_responses(payload)?.into_iter().collect())
    }

    fn dispatch_responses(&mut self, payload: String) -> Result<Option<StreamItem>, ProviderError> {
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
        let value: serde_json::Value = serde_json::from_str(&payload)
            .map_err(|_| structural(OutputStage::Decode, OutputCode::InvalidJson))?;
        if value.get("type").is_none() && value.get("error").is_some() {
            self.terminal = true;
            return Err(self.provider_failure(&value));
        }
        if value["type"].as_str().is_none() {
            return Err(structural(OutputStage::Decode, OutputCode::InvalidField));
        }
        match value["type"].as_str() {
            Some("response.output_item.added") if value["item"]["type"] == "function_call" => {
                let item = &value["item"];
                let id = required_identity(item, "id", OutputStage::Added)?;
                let call = (
                    output_index(&value, OutputStage::Added)?,
                    required_identity(item, "call_id", OutputStage::Added)?.to_owned(),
                    required_identity(item, "name", OutputStage::Added)?.to_owned(),
                );
                if let Some(previous) = self.announced_calls.get(id) {
                    if previous.1 != call.1
                        || previous.2 != call.2
                        || (previous.0.is_some() && call.0.is_some() && previous.0 != call.0)
                        || self.output_done.iter().any(|(index, done)| {
                            same_identity(&done.item, item)
                                && call.0.is_some_and(|observed| observed != *index)
                        })
                    {
                        return Err(structural(OutputStage::Added, OutputCode::IdentityConflict));
                    }
                    if previous.0.is_none() && call.0.is_some() {
                        if self
                            .announced_calls
                            .iter()
                            .any(|(other, prior)| other != id && prior.0 == call.0)
                        {
                            return Err(structural(OutputStage::Added, OutputCode::IndexConflict));
                        }
                        self.announced_calls.get_mut(id).expect("observed call").0 = call.0;
                    }
                    return Ok(None);
                }
                if self.announced_calls.iter().any(|(other, prior)| {
                    other != id && (prior.1 == call.1 || (call.0.is_some() && prior.0 == call.0))
                }) {
                    return Err(structural(OutputStage::Added, OutputCode::IdentityConflict));
                }
                self.check_observation(item, call.0, OutputStage::Added)?;
                self.announced_calls.insert(id.to_owned(), call);
                if item["arguments"]
                    .as_str()
                    .is_some_and(|args| args.len() > ARGUMENT_BYTE_CAP)
                {
                    return Err(ProviderError::ByteLimit("arguments"));
                }
            }
            Some("response.failed" | "error") => {
                self.terminal = true;
                return Err(self.provider_failure(&value));
            }
            Some("response.incomplete") => {
                self.terminal = true;
                match value
                    .pointer("/response/incomplete_details/reason")
                    .and_then(|v| v.as_str())
                {
                    Some("max_output_tokens") => {
                        self.finish = FinishReason::Length;
                        self.complete_response(&value)?;
                    }
                    Some("content_filter") => {
                        let mut error = failure::classified(None, None, true);
                        error.kind = FailureKind::ContentPolicy;
                        return Err(ProviderError::Request(Box::new(error)));
                    }
                    _ => return Err(ProviderError::ResponseIncomplete),
                }
            }
            Some("response.function_call_arguments.delta") => {
                let id = required_identity(&value, "item_id", OutputStage::Decode)?;
                let delta = value["delta"]
                    .as_str()
                    .ok_or_else(|| structural(OutputStage::Decode, OutputCode::InvalidField))?;
                if self
                    .output_done
                    .values()
                    .any(|done| done.item["id"].as_str() == Some(id))
                {
                    return Err(structural(
                        OutputStage::Decode,
                        OutputCode::IdentityConflict,
                    ));
                }
                let (size, final_arguments) = self.arguments.entry(id.to_owned()).or_default();
                if final_arguments.is_some() {
                    return Err(structural(
                        OutputStage::Decode,
                        OutputCode::IdentityConflict,
                    ));
                }
                if size.saturating_add(delta.len()) > ARGUMENT_BYTE_CAP {
                    return Err(ProviderError::ByteLimit("arguments"));
                }
                *size += delta.len();
            }
            Some("response.function_call_arguments.done") => {
                let id = required_identity(&value, "item_id", OutputStage::Done)?;
                let arguments = parsed_arguments(&value, OutputStage::Done)?;
                if let Some((_, Some(previous))) = self.arguments.get(id)
                    && previous != &arguments
                {
                    return Err(structural(OutputStage::Done, OutputCode::IdentityConflict));
                }
                if let Some(done) = self
                    .output_done
                    .values()
                    .find(|done| done.item["id"].as_str() == Some(id))
                    && parsed_arguments(&done.item, OutputStage::Done)? != arguments
                {
                    return Err(structural(OutputStage::Done, OutputCode::IdentityConflict));
                }
                self.arguments.entry(id.to_owned()).or_default().1 = Some(arguments);
            }
            Some("response.output_item.done") => {
                let item = value.get("item").ok_or(ProviderError::InvalidOutput)?;
                validate_output(item, OutputStage::Done)?;
                let explicit_index = output_index(&value, OutputStage::Done)?;
                let indexed = explicit_index.is_some()
                    || item["id"].as_str().is_some_and(|id| {
                        self.announced_calls
                            .get(id)
                            .is_some_and(|call| call.0.is_some())
                    });
                let index = explicit_index
                    .or_else(|| self.observed_index(item))
                    .unwrap_or_else(|| self.output_done.last_key_value().map_or(0, |(n, _)| n + 1));
                self.check_observation(item, Some(index), OutputStage::Done)?;
                if !self.retain_completed(index, item, indexed, OutputStage::Done)? {
                    return Ok(None);
                }
            }
            Some("response.completed") => {
                self.terminal = true;
                if value
                    .pointer("/response/status")
                    .is_some_and(|s| !s.is_string())
                {
                    return Err(structural(
                        OutputStage::Completion,
                        OutputCode::InvalidStatus,
                    ));
                }
                match value
                    .pointer("/response/status")
                    .and_then(serde_json::Value::as_str)
                {
                    Some("failed") => return Err(failure::event_failure(&value)),
                    Some("incomplete" | "in_progress" | "cancelled" | "queued") => {
                        return Err(ProviderError::ResponseIncomplete);
                    }
                    Some("completed") | None => {}
                    _ => {
                        return Err(structural(
                            OutputStage::Completion,
                            OutputCode::InvalidStatus,
                        ));
                    }
                }
                self.complete_response(&value)?;
            }
            _ => {}
        }
        Ok(map_event(&value))
    }

    fn provider_failure(&self, value: &serde_json::Value) -> ProviderError {
        let mut error = failure::event_failure(value);
        if let ProviderError::Request(failure) = &mut error
            && !matches!(
                failure.kind,
                FailureKind::UnknownProvider | FailureKind::IncompleteStream
            )
        {
            // Unclassified/incomplete raw explanations retain their existing
            // withheld contract. An admitted policy/category explanation is
            // redacted before any Debug, runtime event or durable publication.
            failure.message = error_message(value, &self.redactions);
        }
        error
    }

    fn complete_response(&mut self, value: &serde_json::Value) -> Result<(), ProviderError> {
        if let Some(output) = value.pointer("/response/output") {
            let output = output
                .as_array()
                .ok_or_else(|| structural(OutputStage::Completion, OutputCode::InvalidField))?;
            // An index-less done event only supplied arrival order. When the
            // snapshot covers the observations it can insert previously unseen
            // items between them. Real observed indices are never relocated.
            if self.output_done.values().all(|done| {
                output
                    .iter()
                    .any(|item| done.item == *item || same_identity(&done.item, item))
            }) {
                let mut identities = Vec::<&serde_json::Value>::new();
                for item in output {
                    if !identities
                        .iter()
                        .any(|prior| **prior == *item || same_identity(prior, item))
                    {
                        identities.push(item);
                    }
                }
                let inferred: Vec<_> = self
                    .output_done
                    .iter()
                    .filter_map(|(index, done)| (!done.indexed).then_some(*index))
                    .collect();
                let moved: Vec<_> = inferred
                    .into_iter()
                    .map(|index| self.output_done.remove(&index).expect("inferred item"))
                    .collect();
                for done in moved {
                    let index = identities
                        .iter()
                        .position(|item| **item == done.item || same_identity(&done.item, item))
                        .expect("covered item") as u64;
                    self.retain_completed(index, &done.item, false, OutputStage::Completion)?;
                    self.output_done
                        .get_mut(&index)
                        .expect("relocated item")
                        .emitted_reasoning |= done.emitted_reasoning;
                }
            }
            let mut previous_index = None;
            let mut terminal_indices = std::collections::BTreeSet::new();
            for item in output {
                // Length admits only genuinely partial assistant messages;
                // opaque items and failed messages retain ordinary validation.
                let partial_message = self.finish == FinishReason::Length
                    && item["type"] == "message"
                    && item["role"] == "assistant"
                    && matches!(item["status"].as_str(), Some("in_progress" | "incomplete"));
                if !partial_message {
                    validate_output(item, OutputStage::Completion)?;
                }
                if self.finish == FinishReason::Length && item["type"] == "function_call" {
                    // Donor recovery is exclusive to response.completed. An
                    // incomplete finish cannot manufacture a call completion
                    // or change the body of an actual prior output_item.done.
                    let observed =
                        item["id"]
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .is_some_and(|id| {
                                self.output_done.values().any(|done| {
                                    done.item["type"] == "function_call"
                                        && done.item["id"].as_str() == Some(id)
                                        && done.item == *item
                                })
                            });
                    if !observed {
                        return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
                    }
                }
                // A sparse snapshot's array position may be compacted. Actual
                // observed identity/index takes precedence over that position.
                let position = previous_index.map_or(0, |n| n + 1);
                let index = self.observed_index(item).unwrap_or_else(|| {
                    if partial_message && self.output_done.contains_key(&position) {
                        return self.output_done.last_key_value().map_or(0, |(n, _)| n + 1);
                    }
                    // A terminal-only item has no observed index. Its compacted
                    // position cannot overwrite an omitted completed prefix.
                    // Keep every observed slot and fill the next vacant one;
                    // identity/announcement and ordering checks stay strict.
                    let mut index = position;
                    while self.output_done.contains_key(&index) {
                        index += 1;
                    }
                    index
                });
                if previous_index.is_some_and(|previous| index < previous)
                    && !terminal_indices.contains(&index)
                {
                    return Err(structural(
                        OutputStage::Completion,
                        OutputCode::IndexConflict,
                    ));
                }
                self.check_observation(item, Some(index), OutputStage::Completion)?;
                self.retain_completed(index, item, true, OutputStage::Completion)?;
                if terminal_indices.insert(index) {
                    previous_index = Some(index);
                }
            }
        }
        for id in self.announced_calls.keys().chain(self.arguments.keys()) {
            let complete = |item: &serde_json::Value| {
                item["type"] == "function_call" && item["id"].as_str() == Some(id.as_str())
            };
            let found = self.output_done.values().any(|done| complete(&done.item));
            if !found {
                return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
            }
        }
        if self.finish == FinishReason::Length
            && self.output_done.values().any(|done| {
                done.item["type"] == "function_call"
                    && done.item["id"].as_str().is_none_or(str::is_empty)
            })
        {
            return Err(structural(OutputStage::Completion, OutputCode::MissingDone));
        }
        // Move, rather than duplicate, the single completed-item owner after all
        // evidence has been checked. The HTTP owner still closes before return.
        self.output = Some(
            std::mem::take(&mut self.output_done)
                .into_values()
                .map(|done| done.item)
                .collect(),
        );
        if let Some(usage) = value.pointer("/response/usage") {
            let input = usage["input_tokens"].as_u64().unwrap_or(0);
            let output = usage["output_tokens"].as_u64().unwrap_or(0);
            let cache_read = usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let reasoning = usage
                .pointer("/output_tokens_details/reasoning_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            self.compaction_usage = Some(oc_core::compaction::CompactionUsage {
                input_tokens: input.saturating_sub(cache_read),
                output_tokens: output.saturating_sub(reasoning),
                cache_read_tokens: cache_read,
                cache_write_tokens: 0,
                reasoning_tokens: reasoning,
            });
        }
        self.completed = true;
        Ok(())
    }

    fn observed_index(&self, item: &serde_json::Value) -> Option<u64> {
        self.output_done
            .iter()
            .find_map(|(index, done)| {
                (done.item == *item || same_identity(&done.item, item)).then_some(*index)
            })
            .or_else(|| {
                item["id"]
                    .as_str()
                    .and_then(|id| self.announced_calls.get(id)?.0)
            })
    }

    fn check_observation(
        &self,
        item: &serde_json::Value,
        index: Option<u64>,
        stage: OutputStage,
    ) -> Result<(), ProviderError> {
        if stage == OutputStage::Added {
            for (prior_index, done) in &self.output_done {
                if same_identity(&done.item, item)
                    && (item["id"] != done.item["id"]
                        || item["type"] != done.item["type"]
                        || item["call_id"] != done.item["call_id"]
                        || item["name"] != done.item["name"]
                        || index.is_some_and(|index| index != *prior_index))
                {
                    return Err(structural(stage, OutputCode::IdentityConflict));
                }
                if index == Some(*prior_index) && !same_identity(&done.item, item) {
                    return Err(structural(stage, OutputCode::IndexConflict));
                }
            }
        }
        if let Some(id) = item["id"].as_str() {
            if let Some((announced_index, call_id, name)) = self.announced_calls.get(id)
                && (item["type"] != "function_call"
                    || item["call_id"].as_str() != Some(call_id)
                    || item["name"].as_str() != Some(name)
                    || (index.is_some() && announced_index.is_some() && index != *announced_index))
            {
                return Err(structural(stage, OutputCode::IdentityConflict));
            }
            if item["type"] == "function_call"
                && let Some((_, Some(arguments))) = self.arguments.get(id)
                && parsed_arguments(item, stage)? != *arguments
            {
                return Err(structural(stage, OutputCode::IdentityConflict));
            }
        }
        if self
            .announced_calls
            .iter()
            .any(|(id, (announced_index, call_id, _))| {
                item["id"].as_str() != Some(id.as_str())
                    && (item["call_id"].as_str() == Some(call_id.as_str())
                        || (index.is_some()
                            && announced_index.is_some()
                            && index == *announced_index))
            })
        {
            return Err(structural(stage, OutputCode::IdentityConflict));
        }
        Ok(())
    }

    fn retain_completed(
        &mut self,
        index: u64,
        item: &serde_json::Value,
        indexed: bool,
        stage: OutputStage,
    ) -> Result<bool, ProviderError> {
        let emitted_reasoning = stage == OutputStage::Done
            && item["type"] == "reasoning"
            && item["encrypted_content"].is_string();
        for (prior_index, prior) in &mut self.output_done {
            if *prior_index == index {
                // Pinned continuation.ts:193–200: successful completion may
                // re-encrypt reasoning, but replay owns the actual emitted
                // bytes. Every other jointly observed field remains strict.
                let preserve_ciphertext = stage == OutputStage::Completion
                    && self.finish == FinishReason::Stop
                    && prior.emitted_reasoning
                    && prior.item["type"] == "reasoning"
                    && item["type"] == "reasoning"
                    && item["id"].as_str().is_some_and(|id| !id.is_empty())
                    && item["id"] == prior.item["id"]
                    && prior.item["encrypted_content"].is_string()
                    && item["encrypted_content"].is_string();
                // Independent snapshots may omit optional fields. Preserve the
                // union only when all jointly observed fields agree; identities
                // cannot be replaced by an unrelated item at the same index.
                if prior.item == *item
                    || (same_identity(&prior.item, item)
                        && (item["type"] != "function_call" || item["id"] == prior.item["id"])
                        && item.as_object().is_some_and(|fields| {
                            fields.iter().all(|(key, value)| {
                                (preserve_ciphertext && key == "encrypted_content")
                                    || prior.item.get(key).is_none_or(|previous| previous == value)
                            })
                        }))
                {
                    prior.item.as_object_mut().expect("validated item").extend(
                        item.as_object()
                            .expect("validated item")
                            .iter()
                            .filter(|(key, _)| !preserve_ciphertext || *key != "encrypted_content")
                            .map(|(key, value)| (key.clone(), value.clone())),
                    );
                    prior.indexed |= indexed;
                    prior.emitted_reasoning |= emitted_reasoning;
                    return Ok(false);
                }
                return Err(structural(stage, OutputCode::IndexConflict));
            }
            if same_identity(&prior.item, item) {
                return Err(structural(stage, OutputCode::IdentityConflict));
            }
        }
        self.output_done.insert(
            index,
            CompletedItem {
                item: item.clone(),
                indexed,
                emitted_reasoning,
            },
        );
        Ok(true)
    }
}

fn output_index(
    value: &serde_json::Value,
    stage: OutputStage,
) -> Result<Option<u64>, ProviderError> {
    value
        .get("output_index")
        .map(|index| {
            index
                .as_u64()
                .filter(|index| *index < EVENT_CAP as u64)
                .ok_or_else(|| structural(stage, OutputCode::InvalidIndex))
        })
        .transpose()
}

fn required_identity<'a>(
    item: &'a serde_json::Value,
    field: &str,
    stage: OutputStage,
) -> Result<&'a str, ProviderError> {
    item[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| structural(stage, OutputCode::InvalidField))
}

fn same_identity(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    a["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .is_some_and(|id| b["id"].as_str() == Some(id))
        || (a["type"] == "function_call"
            && b["type"] == "function_call"
            && a["call_id"]
                .as_str()
                .is_some_and(|id| b["call_id"].as_str() == Some(id)))
}

fn parsed_arguments(
    item: &serde_json::Value,
    stage: OutputStage,
) -> Result<serde_json::Value, ProviderError> {
    let arguments = required_identity(item, "arguments", stage)?;
    if arguments.len() > ARGUMENT_BYTE_CAP {
        return Err(ProviderError::ByteLimit("arguments"));
    }
    serde_json::from_str(arguments).map_err(|_| structural(stage, OutputCode::InvalidArguments))
}

fn validate_output(item: &serde_json::Value, stage: OutputStage) -> Result<(), ProviderError> {
    required_identity(item, "type", stage)?;
    if item.get("id").is_some() && (item["type"] == "function_call" || !item["id"].is_string()) {
        required_identity(item, "id", stage)?;
    }
    if item
        .get("status")
        .is_some_and(|s| s.as_str() != Some("completed"))
    {
        return Err(structural(stage, OutputCode::InvalidStatus));
    }
    if item["type"] == "function_call" {
        required_identity(item, "call_id", stage)?;
        required_identity(item, "name", stage)?;
        parsed_arguments(item, stage)?;
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
        Some("response.completed" | "response.incomplete") => {
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
/// Exactly one physical attempt; no retry or backoff is owned here.
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
        None,
        &BTreeMap::new(),
        bounded_json(&body)?,
        cancel,
        chunk_timeout.unwrap_or(Duration::from_millis(config.chunk_timeout_ms)),
        observe,
        &mut || async { Ok(()) },
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
    stream_input_counted(
        config,
        model,
        variant,
        input,
        tools,
        max_output,
        cancel,
        observe,
        &mut || async { Ok(()) },
    )
    .await
}

/// Owning-operation dispatch boundary, after local admission and before send.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn stream_input_counted<F: Future<Output = Result<(), ProviderError>> + Send>(
    config: &ResponsesConfig,
    model: &str,
    variant: Option<&SelectedVariant>,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
    dispatch: &mut (impl FnMut() -> F + Send),
) -> Result<Generation, ProviderError> {
    stream_input_overlaid(
        config,
        &RequestOverlay::default(),
        model,
        variant,
        input,
        tools,
        max_output,
        cancel,
        observe,
        dispatch,
    )
    .await
}

/// [`stream_input_counted`] with the selected profile's request overlay.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn stream_input_overlaid<F: Future<Output = Result<(), ProviderError>> + Send>(
    config: &ResponsesConfig,
    overlay: &RequestOverlay,
    model: &str,
    variant: Option<&SelectedVariant>,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
    dispatch: &mut (impl FnMut() -> F + Send),
) -> Result<Generation, ProviderError> {
    let config = config.for_selection(model, variant.map(|v| v.name.as_str()));
    if !config.auth_ready() {
        return Err(ProviderError::InvalidConfig);
    }
    let catalog_model = model;
    let model = config.wire.api_model.as_deref().unwrap_or(model);
    overlay
        .validate()
        .map_err(|_| ProviderError::InvalidConfig)?;
    let effort = variant
        .and_then(|v| v.reasoning_effort.as_deref())
        .or(config.wire.settings.current_effort());
    let chronology = config
        .wire
        .chronology
        .get(catalog_model)
        .copied()
        .unwrap_or_default();
    let (input, effort) =
        protocol::lower_effort(config.wire.protocol, input, effort, chronology.effort);
    let input = input.as_slice();
    let effort = effort.as_deref();
    let effective_variant = effort.map(|effort| SelectedVariant {
        name: variant.map(|v| v.name.clone()).unwrap_or_default(),
        reasoning_effort: Some(effort.to_owned()),
    });
    let variant = effective_variant.as_ref();
    if config.wire.protocol == protocol::Protocol::Chat {
        let compat = config
            .wire
            .chat
            .get(catalog_model)
            .cloned()
            .unwrap_or_default();
        return stream_chat_overlaid(
            config, &compat, overlay, model, variant, input, tools, max_output, cancel, observe,
            dispatch,
        )
        .await;
    }
    if config.wire.protocol == protocol::Protocol::Messages {
        if max_output == 0 {
            return Err(ProviderError::InvalidConfig);
        }
        bounded_json(&(model, input, tools))?;
        let mut body = messages::request_body_chronological(
            model,
            input,
            tools,
            max_output,
            variant.and_then(|v| v.reasoning_effort.as_deref()),
            chronology.system,
        )?;
        config
            .wire
            .settings
            .apply(protocol::Protocol::Messages, &mut body, effort);
        // A kept marker may freeze the initial effort at Default. Do not let
        // configured output_config restore the captured final effort there.
        if input.iter().any(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] == "messages_effort_update"))
            && let Some(output) = body.get_mut("output_config").and_then(serde_json::Value::as_object_mut) {
            if let Some(effort) = effort { output.insert("effort".into(), effort.into()); }
            else { output.remove("effort"); }
        }
        settings::merge(&mut body, &serde_json::json!(overlay.body));
        if config.set_cache_key
            && let Some(context) = &config.wire.context
        {
            context.cache_body(protocol::Protocol::Messages, &mut body, false);
        }
        return stream_body(
            config,
            None,
            &overlay.headers,
            bounded_json(&body)?,
            cancel,
            Duration::from_millis(config.chunk_timeout_ms),
            observe,
            dispatch,
        )
        .await;
    }
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
    // GO03: chronological system updates lower to Responses `developer` in
    // place on every lane; the initial prompt is already a developer item.
    let input = protocol::lower_chronological_system(protocol::Protocol::Responses, input, false);
    let input = input.as_ref();
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
    config
        .wire
        .settings
        .apply(protocol::Protocol::Responses, &mut body, effort);
    settings::merge(&mut body, &serde_json::json!(overlay.body));
    if config.set_cache_key
        && let Some(context) = &config.wire.context
    {
        context.cache_body(protocol::Protocol::Responses, &mut body, false);
    }
    stream_body(
        config,
        None,
        &overlay.headers,
        bounded_json(&body)?,
        cancel,
        Duration::from_millis(config.chunk_timeout_ms),
        observe,
        dispatch,
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

/// Chat Completions dispatch with the selected profile's request overlay; the
/// same transport, cancel, caps, typed failures and one-attempt contract apply.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn stream_chat_overlaid<F: Future<Output = Result<(), ProviderError>> + Send>(
    config: &ResponsesConfig,
    compat: &chat::ChatCompat,
    overlay: &RequestOverlay,
    model: &str,
    variant: Option<&SelectedVariant>,
    input: &[InputItem],
    tools: &[ToolDef],
    max_output: u64,
    cancel: &AtomicBool,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
    dispatch: &mut (impl FnMut() -> F + Send),
) -> Result<Generation, ProviderError> {
    overlay
        .validate()
        .map_err(|_| ProviderError::InvalidConfig)?;
    if max_output == 0 {
        return Err(ProviderError::InvalidConfig);
    }
    bounded_json(&(model, input, tools))?;
    let mut body = chat::request_body(
        model,
        input,
        tools,
        max_output,
        variant.and_then(|v| v.reasoning_effort.as_deref()),
        compat,
    )?;
    config.wire.settings.apply(
        protocol::Protocol::Chat,
        &mut body,
        variant.and_then(|v| v.reasoning_effort.as_deref()),
    );
    settings::merge(&mut body, &serde_json::json!(overlay.body));
    if config.set_cache_key
        && let Some(context) = &config.wire.context
    {
        context.cache_body(
            protocol::Protocol::Chat,
            &mut body,
            compat.supports_prompt_cache_key,
        );
    }
    stream_body(
        config,
        Some(compat),
        &overlay.headers,
        bounded_json(&body)?,
        cancel,
        Duration::from_millis(config.chunk_timeout_ms),
        observe,
        dispatch,
    )
    .await
}

/// Cancellation waiter shared with native protocol adapters. No wakeup depends
/// on network activity; callers select this against DNS/header/body futures.
pub(crate) async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn stream_body<F: Future<Output = Result<(), ProviderError>> + Send>(
    config: &ResponsesConfig,
    chat: Option<&chat::ChatCompat>,
    extra_headers: &BTreeMap<String, String>,
    body: Vec<u8>,
    cancel: &AtomicBool,
    chunk_timeout: Duration,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
    dispatch: &mut (impl FnMut() -> F + Send),
) -> Result<Generation, ProviderError> {
    if config.timeout == Some(true) {
        return Err(ProviderError::InvalidConfig);
    }
    // One numeric deadline covers DNS, connecting, headers and the entire body.
    // The independent idle timeout still applies; no adapter retry is added.
    let total_deadline = config
        .wire
        .total_timeout_ms
        .map(|ms| {
            tokio::time::Instant::now()
                .checked_add(Duration::from_millis(ms))
                .ok_or(ProviderError::InvalidConfig)
        })
        .transpose()?;
    let mut headers = request_headers(config)?;
    for (name, value) in extra_headers {
        let name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| ProviderError::InvalidConfig)?;
        let merged;
        let value =
            if config.wire.protocol == protocol::Protocol::Messages && name == "anthropic-beta" {
                merged = format!(
                    "{},{}",
                    headers
                        .get(&name)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default(),
                    value
                );
                &merged
            } else {
                value
            };
        let mut value = reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| ProviderError::InvalidConfig)?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    if config.wire.protocol == protocol::Protocol::Messages {
        messages_headers(&mut headers)?;
        let value: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| ProviderError::InvalidConfig)?;
        if value["messages"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|m| m["role"] == "system" && m.get("output_config").is_some())
        }) {
            let beta = format!(
                "{},mid-conversation-output-config-2026-07-01",
                headers["anthropic-beta"]
                    .to_str()
                    .map_err(|_| ProviderError::InvalidConfig)?
            );
            headers.insert(
                "anthropic-beta",
                reqwest::header::HeaderValue::from_str(&beta)
                    .map_err(|_| ProviderError::InvalidConfig)?,
            );
            messages_headers(&mut headers)?;
        }
    }
    if config.wire.go {
        let context = config
            .wire
            .context
            .as_ref()
            .ok_or(ProviderError::InvalidConfig)?;
        context.go_headers(&mut headers)?;
        headers.remove("x-api-key");
        headers.remove("authorization");
        let resolved = request_headers(config)?;
        for name in ["authorization", "x-api-key"] {
            if let Some(auth) = resolved.get(name) {
                headers.insert(name, auth.clone());
            }
        }
    }
    if config.wire.openai.is_some() {
        // Admitted profile overlays cannot replace captured credential/account/
        // actor identity. Key requests also strip Codex-only injected headers.
        let resolved = request_headers(config)?;
        for name in [
            "authorization",
            "x-api-key",
            "originator",
            "x-codex-beta-features",
            "chatgpt-account-id",
            "session-id",
        ] {
            headers.remove(name);
            if let Some(value) = resolved.get(name) {
                headers.insert(name, value.clone());
            }
        }
    }
    let protocol = if chat.is_some() {
        protocol::Protocol::Chat
    } else {
        config.wire.protocol
    };
    let url = match protocol {
        protocol::Protocol::Chat => config.chat_url()?,
        protocol::Protocol::Responses => config.generation_url()?,
        protocol::Protocol::Messages => config
            .generation_url()?
            .strip_suffix("/responses")
            .map(|base| format!("{base}/messages"))
            .expect("Responses suffix"),
    };
    let channel = config.wire.transport == Some(websocket::Transport::WebSocket)
        || (config.wire.transport.is_none()
            && config.wire.openai.is_some()
            && protocol == protocol::Protocol::Responses);
    if channel {
        if config.wire.openai.is_none() || protocol != protocol::Protocol::Responses {
            return Err(ProviderError::InvalidConfig);
        }
        let channels = config.wire.channels.clone().unwrap_or_default();
        if let Some(generation) = channels
            .exchange(
                config,
                &url,
                &headers,
                &body,
                cancel,
                chunk_timeout,
                total_deadline,
                observe,
                dispatch,
            )
            .await?
        {
            return Ok(generation);
        }
        // Affirmative connect/not-sent/size rejection only. The same final
        // captured URL, body, model and credential continue through HTTP below.
    }
    // Test campaigns reserve durably before DNS/dial without changing authority,
    // proxies, peer checks, production retry policy or successful-step limits.
    #[cfg(test)]
    let body = if config.wire.live_campaign.is_some() {
        go_live_tests::tool_smoke_body(&body, protocol)?
    } else {
        body
    };
    #[cfg(test)]
    let live_sequence = config
        .wire
        .live_campaign
        .as_ref()
        .map(|path| go_live_tests::reserve(path, &url, &body, protocol))
        .transpose()?;
    let dns_deadline = tokio::time::Instant::now() + config.connect_timeout;
    let dns_deadline = total_deadline.map_or(dns_deadline, |total| total.min(dns_deadline));
    let (host, addresses) = tokio::select! {
        biased;
        () = wait_cancel(cancel) => return Err(ProviderError::Cancelled),
        result = tokio::time::timeout_at(dns_deadline, crate::endpoint::resolve(&url, config.wire.endpoint.as_ref(), config.allow_private)) => {
            result.map_err(|_| ProviderError::Deadline)??
        }
    };

    let builder = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .user_agent(crate::USER_AGENT)
        .resolve_to_addrs(&host, &addresses)
        .connect_timeout(config.connect_timeout);
    let builder = if let Some(deadline) = total_deadline {
        let remaining = deadline
            .checked_duration_since(tokio::time::Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or(ProviderError::Deadline)?;
        builder.timeout(remaining)
    } else {
        // timeout:false (or absent) means no total deadline: never set one.
        builder
    };
    let client = builder.build().map_err(|_| ProviderError::InvalidConfig)?;

    let result = stream_attempt(
        &client,
        chat,
        protocol,
        &url,
        &headers,
        &body,
        config.allow_private,
        config.wire.endpoint.as_ref(),
        cancel,
        chunk_timeout,
        observe,
        dispatch,
    )
    .await
    .map_err(|(error, _)| error);
    #[cfg(test)]
    if let (Some(path), Some(sequence)) = (&config.wire.live_campaign, live_sequence) {
        go_live_tests::finish(path, sequence, &result)?;
    }
    result
}

#[cfg(test)]
#[path = "provider/go_live_tests.rs"]
mod go_live_tests;

pub(crate) fn request_headers(
    config: &ResponsesConfig,
) -> Result<reqwest::header::HeaderMap, ProviderError> {
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
    let anonymous = config.wire.auth_policy == crate::auth::AuthPolicy::None;
    if anonymous
        && (!config.api_key.is_empty()
            || config.headers.keys().any(|name| {
                name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
            }))
    {
        return Err(ProviderError::InvalidConfig);
    }
    if config.wire.auth_policy == crate::auth::AuthPolicy::OAuth
        && !config.wire.openai.as_ref().is_some_and(|b| {
            b.subscription && config.wire.protocol == protocol::Protocol::Responses
        })
    {
        return Err(ProviderError::InvalidConfig);
    }
    let mut headers = HeaderMap::new();
    for (name, value) in &config.headers {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| ProviderError::InvalidConfig)?;
        match name.as_str() {
            "authorization" | "accept" | "content-type" => continue,
            "x-api-key" if config.wire.go || config.wire.openai.is_some() => continue,
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
    let messages = config.wire.protocol == protocol::Protocol::Messages;
    if messages
        && !config.wire.go
        && config.headers.keys().any(|name| {
            name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
        })
    {
        return Err(ProviderError::InvalidConfig);
    }
    if !anonymous {
        let bearer = !messages || (!config.wire.go && config.wire.messages_bearer);
        let mut auth = HeaderValue::from_str(&if bearer {
            format!("Bearer {}", config.api_key)
        } else {
            config.api_key.clone()
        })
        .map_err(|_| ProviderError::InvalidConfig)?;
        auth.set_sensitive(true);
        headers.insert(if bearer { "authorization" } else { "x-api-key" }, auth);
    }
    if messages {
        messages_headers(&mut headers)?;
    }
    if let Some(binding) = &config.wire.openai {
        for name in [
            "originator",
            "x-codex-beta-features",
            "chatgpt-account-id",
            "session-id",
        ] {
            headers.remove(name);
        }
        if binding.subscription {
            headers.insert("originator", HeaderValue::from_static("opencode"));
            headers.insert(
                "x-codex-beta-features",
                HeaderValue::from_static("remote_compaction_v2"),
            );
            if let Some(account) = &binding.account {
                let mut value =
                    HeaderValue::from_str(account).map_err(|_| ProviderError::InvalidConfig)?;
                value.set_sensitive(true);
                headers.insert("chatgpt-account-id", value);
            }
            if let Some(context) = &config.wire.context {
                context.openai_headers(&mut headers)?;
            }
        }
    }
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn messages_headers(headers: &mut reqwest::header::HeaderMap) -> Result<(), ProviderError> {
    use reqwest::header::HeaderValue;
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    let mut betas: Vec<String> = Vec::new();
    for beta in headers
        .get("anthropic-beta")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .chain(["interleaved-thinking-2025-05-14"])
    {
        if !betas.iter().any(|b| b == beta) {
            betas.push(beta.to_owned());
        }
    }
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_str(&betas.join(",")).map_err(|_| ProviderError::InvalidConfig)?,
    );
    Ok(())
}

/// One POST→SSE attempt. Returns the terminal error plus whether any event
/// was already committed (which forbids retry).
#[allow(clippy::too_many_arguments)]
async fn stream_attempt<F: Future<Output = Result<(), ProviderError>> + Send>(
    client: &reqwest::Client,
    chat: Option<&chat::ChatCompat>,
    protocol: protocol::Protocol,
    url: &str,
    headers: &reqwest::header::HeaderMap,
    body: &[u8],
    allow_private: bool,
    endpoint: Option<&crate::endpoint::EndpointBinding>,
    cancel: &AtomicBool,
    chunk_timeout: Duration,
    observe: &mut (dyn FnMut(&StreamItem) + Send),
    dispatch: &mut (impl FnMut() -> F + Send),
) -> Result<Generation, (ProviderError, bool)> {
    let request = client
        .post(url)
        .headers(headers.clone())
        .body(body.to_vec());
    let send = async {
        if cancel.load(Ordering::Relaxed) {
            return Err((ProviderError::Cancelled, false));
        }
        dispatch().await.map_err(|e| (e, false))?;
        Ok(request.send().await)
    };
    let resp = tokio::select! {
        biased;
        () = wait_cancel(cancel) => return Err((ProviderError::Cancelled, false)),
        result = tokio::time::timeout(chunk_timeout, send) => result.map_err(|_| {
            let mut failure = PhysicalFailure::transport(Operation::Request, Delivery::Unknown, None);
            failure.transport = Some(TransportKind::IdleTimeout);
            (ProviderError::Request(Box::new(failure)), false)
        })??,
    }
        .map_err(|e| {
            // Only true timeouts are deadlines; refusals/DNS failures are
            // transport errors (connect_timeout expiry surfaces is_timeout).
            let mut failure = PhysicalFailure::transport(Operation::Request,
                if e.is_connect() { Delivery::NotSent } else { Delivery::Unknown }, None);
            if e.is_timeout() { failure.transport = Some(TransportKind::Deadline); }
            (ProviderError::Request(Box::new(failure)), false)
        })?;
    // Post-dial rebinding guard on the connected peer.
    if let Some(peer) = resp.remote_addr()
        && !crate::endpoint::peer_allowed(endpoint, peer.ip(), allow_private)
    {
        return Err((ProviderError::PrivateHost, false));
    }
    let status = resp.status().as_u16();
    let retry_headers = RetryHeaders::observed(resp.headers(), status >= 400);
    if status != 200 {
        let mut response = resp;
        let mut bytes = Vec::new();
        let deadline = tokio::time::Instant::now() + chunk_timeout;
        loop {
            let chunk = tokio::select! {
                () = wait_cancel(cancel) => return Err((ProviderError::Cancelled,false)),
                result = tokio::time::timeout_at(deadline,response.chunk()) => result,
            };
            match chunk {
                Ok(Ok(Some(chunk))) if bytes.len() + chunk.len() <= 16 * 1024 => {
                    bytes.extend_from_slice(&chunk)
                }
                Ok(Ok(None)) => break,
                // Status remains affirmative even when the bounded diagnostic
                // body is unavailable. Do not decode a truncated body.
                _ => {
                    bytes.clear();
                    break;
                }
            }
        }
        let diagnostic = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
        let mut failure = failure::classified(diagnostic.as_ref(), Some(status), false);
        failure.headers = retry_headers;
        if !matches!(
            failure.kind,
            FailureKind::RateLimit | FailureKind::ProviderInternal
        ) {
            failure.headers.retry_after_ms = None;
        }
        failure.message = diagnostic.as_ref().and_then(|v| error_message(v, headers));
        return Err((ProviderError::Request(Box::new(failure)), false));
    }

    let mut parser = match protocol {
        protocol::Protocol::Messages => SseParser::messages(),
        _ => chat.map_or_else(SseParser::default, SseParser::chat),
    };
    parser.redactions = headers.clone();
    let mut items: Vec<StreamItem> = Vec::new();
    let mut committed = false;
    let read_failure = |error, committed| {
        let mut failure = match error {
            ProviderError::Request(failure) => *failure,
            ProviderError::Incomplete | ProviderError::ResponseIncomplete => {
                let mut failure =
                    PhysicalFailure::transport(Operation::Read, Delivery::Accepted, Some(status));
                failure.kind = FailureKind::IncompleteStream;
                failure.transport = None;
                failure
            }
            ProviderError::Transport | ProviderError::Deadline | ProviderError::IdleTimeout => {
                let mut failure =
                    PhysicalFailure::transport(Operation::Read, Delivery::Accepted, Some(status));
                failure.transport = Some(match error {
                    ProviderError::Deadline => TransportKind::Deadline,
                    ProviderError::IdleTimeout => TransportKind::IdleTimeout,
                    _ => TransportKind::Network,
                });
                failure
            }
            // Parsing, UTF-8, policy and resource limits are local terminal
            // errors: never inherit provider override eligibility.
            local => return (local, committed),
        };
        failure.http_status = Some(status);
        failure.headers = retry_headers;
        failure.output_committed = committed;
        (ProviderError::Request(Box::new(failure)), committed)
    };
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
            Err(_) => return Err(read_failure(ProviderError::IdleTimeout, committed)),
            Ok(Err(e)) => {
                let error = if e.is_timeout() {
                    ProviderError::Deadline
                } else {
                    ProviderError::Transport
                };
                return Err(read_failure(error, committed));
            }
            Ok(Ok(chunk)) => chunk,
        };
        match chunk {
            None => break,
            Some(bytes) => {
                if bytes.is_empty() {
                    continue;
                }
                // A Content-Length response can coalesce thousands of frames
                // into one ready chunk. Give existing bounded event consumers
                // and cancellation a turn between dispatched SSE frames.
                for segment in bytes.split_inclusive(|byte| *byte == b'\n') {
                    if cancel.load(Ordering::Relaxed) {
                        return Err((ProviderError::Cancelled, committed));
                    }
                    let events = parser.events;
                    let mut forward = |item: &StreamItem| {
                        committed |= !matches!(
                            item,
                            StreamItem::Usage { .. } | StreamItem::MessageBoundary { .. }
                        );
                        observe(item);
                    };
                    let result = parser.push_observed(segment, &mut forward);
                    let mut fresh = result.map_err(|e| read_failure(e, committed))?;
                    items.append(&mut fresh);
                    if parser.completed {
                        break;
                    }
                    if parser.events != events {
                        tokio::task::yield_now().await;
                    }
                }
                if parser.completed {
                    break;
                }
            }
        }
    }
    let tail = parser.finish().map_err(|e| read_failure(e, committed))?;
    for item in &tail {
        observe(item);
    }
    items.extend(tail);
    if !parser.completed {
        return Err(read_failure(ProviderError::Incomplete, committed));
    }
    Ok(collect_generation(parser, items))
}

fn collect_generation(parser: SseParser, items: Vec<StreamItem>) -> Generation {
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
    let output = parser.output.unwrap_or_else(|| {
        parser
            .output_done
            .into_values()
            .map(|done| done.item)
            .collect()
    });
    Generation {
        finish: parser.finish,
        compaction_usage: parser.compaction_usage,
        output,
        items,
        text,
        usage,
    }
}

fn error_message(
    value: &serde_json::Value,
    headers: &reqwest::header::HeaderMap,
) -> Option<String> {
    [
        "/error/message",
        "/response/error/message",
        "/message",
        "/error",
        "/response/error",
    ]
    .iter()
    .find_map(|path| value.pointer(path).and_then(serde_json::Value::as_str))
    .and_then(|message| safe_diagnostic(message, headers))
}

fn safe_diagnostic(message: &str, headers: &reqwest::header::HeaderMap) -> Option<String> {
    // Only the structured error.message field is projected, never headers or
    // the raw body. Bound it before processing and remove configured secrets.
    if message.trim().is_empty() {
        return None;
    }
    if message.len() > 4096 {
        return Some("provider diagnostic omitted (input exceeded 4096 bytes)".into());
    }
    let mut message = message.to_owned();
    for secret in headers.values().filter_map(|value| value.to_str().ok()) {
        let secret = secret.strip_prefix("Bearer ").unwrap_or(secret);
        if !secret.is_empty() {
            message = message.replace(secret, "[redacted]");
        }
    }
    const PUBLIC_URI: &str = "https://platform.openai.com/settings/organization/status-and-access";
    let boundary = |c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '\'' | '"');
    let safe_uri = message.match_indices(PUBLIC_URI).all(|(start, uri)| {
        message[..start].chars().next_back().is_none_or(boundary)
            && message[start + uri.len()..]
                .chars()
                .next()
                .is_none_or(boundary)
    });
    let lower = if safe_uri {
        message
            .replace(PUBLIC_URI, "[public provider URI]")
            .to_ascii_lowercase()
    } else {
        message.to_ascii_lowercase()
    };
    if [
        "authorization:",
        "bearer ",
        "cookie:",
        "api_key=",
        "apikey=",
        "://",
        "environment",
        "env=",
    ]
    .iter()
    .any(|key| lower.contains(key))
        || message
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Some("provider diagnostic contained private data (redacted)".into());
    }
    let mut chars = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c });
    let mut safe: String = chars.by_ref().take(512).collect();
    if chars.next().is_some() {
        // Keep the existing 512-character output ceiling, with honest clipping.
        safe = safe.chars().take(502).collect();
        safe.push_str(" [clipped]");
    }
    Some(safe)
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
    mod openai;
    mod reconciliation;
    mod typed_failures;

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
            wire: Default::default(),
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
                assert!(matches!(
                    result,
                    Err(ProviderError::OutputStructure {
                        stage: super::OutputStage::Completion,
                        code: super::OutputCode::MissingDone,
                    })
                ));
            }
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            server.shutdown();
        }
    }

    #[tokio::test]
    async fn aud11_terminal_failures_never_retry_and_success_does_not_wait_for_eof() {
        for (kind, expected) in [
            ("response.failed", super::FailureKind::UnknownProvider),
            ("response.incomplete", super::FailureKind::IncompleteStream),
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
            assert!(
                matches!(&result, Err(ProviderError::Request(error)) if error.kind == expected)
            );
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
    async fn go03_chat_wire_one_attempt_url_auth_body_and_common_output() {
        use super::{InputItem, InputRole, chat::ChatCompat, stream_chat_overlaid};
        let frames = [
            serde_json::json!({"choices":[{"delta":{"content":"hi"}}]}),
            serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":3,"completion_tokens":2}}),
        ];
        let mut body = frames
            .iter()
            .map(|frame| format!("data: {frame}\n\n"))
            .collect::<String>();
        body.push_str("data: [DONE]\n\n");
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "200 OK",
            headers: vec![("Content-Type", "text/event-stream".to_string())],
            chunks: vec![(body.clone().into_bytes(), 0)],
            abort_after: None,
        }))
        .await;
        let config = test_config(&server.base);
        let input = vec![
            InputItem::message(InputRole::Developer, "BASE"),
            InputItem::message(InputRole::User, "hello"),
        ];
        let mut observed = Vec::new();
        let generation = stream_chat_overlaid(
            &config,
            &ChatCompat::default(),
            &Default::default(),
            "chat-model",
            None,
            &input,
            &tools(),
            64,
            &NO_CANCEL,
            &mut |item| observed.push(item.clone()),
            &mut || async { Ok(()) },
        )
        .await
        .expect("chat generation");
        assert_eq!(generation.text, "hi");
        assert_eq!(generation.usage, Some((3, 2)));
        assert_eq!(generation.output[1]["type"], "function_call");
        assert_eq!(generation.output[1]["call_id"], "call_1");
        assert!(observed.iter().any(
            |i| matches!(i, StreamItem::ToolCallStarted { call_id, .. } if call_id == "call_1")
        ));
        let seen = server.seen.lock().expect("seen");
        assert_eq!(seen.len(), 1, "one physical attempt");
        assert_eq!(seen[0].path, "/chat/completions");
        assert_eq!(seen[0].headers["authorization"], "Bearer test-key");
        let sent: serde_json::Value = serde_json::from_slice(&seen[0].body).expect("body");
        assert_eq!(sent["model"], "chat-model");
        assert_eq!(
            sent["messages"][0],
            serde_json::json!({"role":"system","content":"BASE"})
        );
        assert_eq!(sent["max_tokens"], 64);
        assert!(sent.get("input").is_none() && sent.get("store").is_none());
        drop(seen);
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
        // GO03: the chronological system update is lowered to developer in place.
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(body["input"][0]["content"][0]["text"], "system");
        assert_eq!(body["input"][1]["role"], "developer");
        assert!(!body["input"].to_string().contains("\"role\":\"system\""));
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
        assert!(matches!(
            super::stream_input_observed(&config, "m", None, &[], &[], 5, &NO_CANCEL, &mut |_| {}).await,
            Err(ProviderError::Request(error)) if error.transport == Some(super::TransportKind::IdleTimeout)
        ));
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
        assert!(
            matches!(stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None).await,
            Err(ProviderError::Request(error)) if error.kind == super::FailureKind::InvalidRequest && error.http_status == Some(400))
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
        assert!(result.as_ref().unwrap_err().is_incomplete());
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
        for status in ["401 Unauthorized", "403 Forbidden"] {
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
            assert!(
                matches!(err, ProviderError::Request(error) if error.kind == super::FailureKind::Authentication)
            );
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            server.shutdown();
        }
        // R1 supersedes T12: a second scripted success must remain unrequested.
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
        let error = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect_err("one physical attempt");
        assert!(
            matches!(error, ProviderError::Request(error) if error.kind == super::FailureKind::RateLimit)
        );
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
        // Empty EOF also cannot cause an adapter-owned retry.
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
        let error = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect_err("incomplete physical result");
        assert!(error.is_incomplete());
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
        // Persistent 500: one observed server failure, no second POST.
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
        assert!(
            matches!(err, ProviderError::Request(error) if error.kind == super::FailureKind::ProviderInternal)
        );
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
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
        assert!(err.is_incomplete());
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
        assert!(
            matches!(err, ProviderError::Request(error) if error.transport == Some(super::TransportKind::IdleTimeout))
        );
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
        assert!(
            matches!(err, ProviderError::Request(error) if error.kind == super::FailureKind::Transport && error.delivery == super::Delivery::NotSent && error.retry_eligible())
        );
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
