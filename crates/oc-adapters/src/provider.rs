//! OpenProxy native Responses adapter for T12 (PROV01/02/06/07).
//!
//! Exact generation URL (configured prefix preserved + `/responses`, never a
//! blind `/v1` add/remove), `Bearer` auth, `store:false`, stable
//! `prompt_cache_key`, and ordinary function tool schemas — no OAuth, no API
//! fallback. Bounded incremental SSE (arbitrary byte splits incl. split
//! UTF-8, multiline `data:`, CRLF, comments), text/argument deltas and usage
//! metadata with an event cap. Errors are typed (401/403/429/5xx,
//! incomplete/EOF); exactly one retry lives here and only before any event
//! is committed — callers never repeat a committed generation or tool call.
//! `timeout:false` means no total deadline; the 6 000 000 ms chunk idle
//! budget is enforced between bytes; explicit cancel drops the connection.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::models::SelectedVariant;

/// Idle budget between SSE bytes (6 000 000 ms = 100 min, not 6 s).
pub const CHUNK_TIMEOUT_MS: u64 = 6_000_000;
/// Max SSE events decoded per response.
pub const EVENT_CAP: usize = 10_000;
/// Max attempts per generation: initial + exactly one retry.
pub const MAX_ATTEMPTS: usize = 2;

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
}

/// Ordinary function tool definition for the request schema.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// A function-call item was announced (`output_item.added`).
    ToolCallStarted {
        /// Item id the following argument deltas attach to.
        item_id: String,
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
    data: Vec<String>,
    /// Current event's `event:` field, if any.
    event_name: Option<String>,
    /// Events decoded so far (cap enforcement).
    events: usize,
}

impl SseParser {
    /// Feed bytes; returns decoded stream items (may be empty mid-line).
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamItem>, ProviderError> {
        self.pending.extend_from_slice(bytes);
        // Split off the longest valid UTF-8 prefix; hold an incomplete tail.
        let valid_len = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(e) => {
                if e.error_len().is_none() {
                    e.valid_up_to()
                } else {
                    // Malformed bytes: lossy-decode the valid head, drop one
                    // byte past it, keep the rest for the next push.
                    let head =
                        String::from_utf8_lossy(&self.pending[..e.valid_up_to()]).into_owned();
                    let mut rest = self.pending[e.valid_up_to() + 1..].to_vec();
                    std::mem::swap(&mut self.pending, &mut rest);
                    let items = self.consume_text(&head)?;
                    let mut tail = self.push(&[])?;
                    let mut out = items;
                    out.append(&mut tail);
                    return Ok(out);
                }
            }
        };
        let text = std::str::from_utf8(&self.pending[..valid_len])
            .map_err(|_| ProviderError::Incomplete)?
            .to_string();
        self.pending.drain(..valid_len);
        // Complete events decoded from the valid prefix are returned even
        // when a split UTF-8 tail (or partial line) stays pending.
        self.consume_text(&text)
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

    fn consume_text(&mut self, text: &str) -> Result<Vec<StreamItem>, ProviderError> {
        let mut out = Vec::new();
        // Retain a trailing partial line for the next push.
        let mut lines: Vec<&str> = text.split('\n').collect();
        let tail = if text.ends_with('\n') {
            None
        } else {
            lines.pop()
        };
        for line in lines {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() {
                if let Some(item) = self.dispatch()? {
                    out.push(item);
                }
                continue;
            }
            if line.starts_with(':') {
                continue; // Comment / heartbeat.
            }
            if let Some(name) = line.strip_prefix("event:") {
                self.event_name = Some(name.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.strip_prefix(' ').unwrap_or(data);
                self.data.push(data.to_string());
            }
        }
        if let Some(tail) = tail {
            let tail = tail.strip_suffix('\r').unwrap_or(tail);
            self.pending.splice(..0, tail.as_bytes().iter().cloned());
        }
        Ok(out)
    }

    fn dispatch(&mut self) -> Result<Option<StreamItem>, ProviderError> {
        if self.data.is_empty() && self.event_name.is_none() {
            return Ok(None);
        }
        self.events += 1;
        if self.events > EVENT_CAP {
            return Err(ProviderError::EventCap);
        }
        let payload = self.data.join("\n");
        self.data.clear();
        self.event_name = None;
        if payload == "[DONE]" {
            return Ok(None);
        }
        let value: serde_json::Value =
            serde_json::from_str(&payload).map_err(|_| ProviderError::Incomplete)?;
        Ok(map_event(&value))
    }
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
            if item.get("type").and_then(|t| t.as_str()) != Some("function_call") {
                return None;
            }
            Some(StreamItem::ToolCallStarted {
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
        Some("response.reasoning.delta") => value
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
                _ => None,
            }
        }
        Some("response.completed") => {
            let usage = value.pointer("/response/usage");
            let input = usage
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output = usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
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
/// `cancel` is polled between chunks; setting it drops the connection and
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
    let url = config.generation_url()?;
    guard_private_url(&url, config.allow_private).await?;
    let body = request_body(model, variant, prompt, tools);
    let chunk_timeout = chunk_timeout.unwrap_or(Duration::from_millis(CHUNK_TIMEOUT_MS));

    let builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(config.connect_timeout);
    // timeout:false (or absent) means no total deadline: never set one.
    if config.timeout == Some(true) {
        return Err(ProviderError::InvalidConfig);
    }
    let client = builder.build().map_err(|_| ProviderError::Transport)?;

    let mut attempts = 0usize;
    loop {
        attempts += 1;
        match stream_attempt(&client, &url, &config.api_key, &body, cancel, chunk_timeout).await {
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

async fn guard_private_url(url: &str, allow_private: bool) -> Result<(), ProviderError> {
    let host = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("");
    let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return Err(ProviderError::InvalidConfig);
    }
    let port = 80u16;
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
async fn stream_attempt(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    body: &serde_json::Value,
    cancel: &AtomicBool,
    chunk_timeout: Duration,
) -> Result<Generation, (ProviderError, bool)> {
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .header("Accept", "text/event-stream")
        .json(body)
        .send()
        .await
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
        && !peer.ip().is_loopback()
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
        return Err((ProviderError::Transport, false));
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
        let chunk = match tokio::time::timeout(chunk_timeout, resp.chunk()).await {
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
                let mut fresh = parser.push(&bytes).map_err(|e| (e, committed))?;
                if !fresh.is_empty() {
                    committed = true;
                }
                items.append(&mut fresh);
            }
        }
    }
    let tail = parser.finish().map_err(|e| (e, committed))?;
    items.extend(tail);
    if items.is_empty() {
        // An eventless EOF is a truncated stream, never an empty success.
        return Err((ProviderError::Incomplete, false));
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
    Ok(Generation { items, text, usage })
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
                        for line in lines {
                            if line.is_empty() {
                                break;
                            }
                            if let Some((k, v)) = line.split_once(':') {
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
        format!("data: {{\"type\":\"response.completed\",\"response\":{{\"usage\":{{\"input_tokens\":{input},\"output_tokens\":{output}}}}}}}\n\n").into_bytes()
    }

    fn test_config(base: &str) -> ResponsesConfig {
        ResponsesConfig {
            base_url: base.to_string(),
            api_key: "test-key".to_string(),
            timeout: Some(false),
            chunk_timeout_ms: CHUNK_TIMEOUT_MS,
            connect_timeout: Duration::from_secs(5),
            allow_private: true,
        }
    }

    static NO_CANCEL: AtomicBool = AtomicBool::new(false);

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
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":20}}}\n\n".to_string(),
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
                    chunks: vec![(sse_delta("ok"), 0)],
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
                    chunks: vec![(sse_delta("ok"), 0)],
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
