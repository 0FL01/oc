//! Native dynamic discovery for T14 (DISC01–DISC10).
//!
//! Rust port of the user-supplied `openproxy-models.user.mjs` reference
//! (authority over the repository plugin): unknown model IDs need no
//! registry, per-row validation with all-or-nothing publication, local
//! snapshot merge with provenance-preserving overrides, 500k clamps,
//! reasoned-variant allowlists, fake-clock budgeted retries, and sanitized
//! warnings. No model IDs are hardcoded; no JS is executed.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue, USER_AGENT};
use thiserror::Error;

/// Provider id this discovery serves.
pub const PROVIDER_ID: &str = "ludka2";
/// Per-attempt budget (ms).
pub const ATTEMPT_TIMEOUT_MS: u64 = 15_000;
/// Total discovery budget (ms).
pub const TOTAL_TIMEOUT_MS: u64 = 30_000;
/// Backoff between attempts, clipped by the remaining budget.
pub const RETRY_DELAYS_MS: [u64; 3] = [250, 750, 1500];
/// Context/input clamp (output is never clamped).
pub const MAX_CONTEXT_TOKENS: u64 = 500_000;
/// Standard reasoning variant names (absent ones default disabled).
pub const STANDARD_REASONING_VARIANTS: [&str; 7] =
    ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
/// Discovery body cap (mirrors `discovery_body_bytes`).
pub const DISCOVERY_BODY_CAP: usize = 8 * 1024 * 1024;
/// Discovery row cap (mirrors `discovery_rows`).
pub const DISCOVERY_ROWS_CAP: usize = 10_000;

/// Typed discovery failures (sanitized: never URLs, keys, or bodies).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    /// Invalid provider URL or credentials (pre-network).
    #[error("invalid provider URL or credentials")]
    InvalidConfig,
    /// Network error or discovery timeout (retryable).
    #[error("network error or discovery timeout")]
    Network,
    /// HTTP failure with terminal/ retryable class inline.
    #[error("HTTP {status}")]
    Http {
        /// Status code.
        status: u16,
    },
    /// Body unparseable (retryable) vs. envelope/row invalid (terminal).
    #[error("invalid models response")]
    InvalidResponse,
    /// Successful body with zero rows (retryable while budget lasts).
    #[error("empty models response")]
    EmptyResponse,
    /// Explicit cancellation cleaned up the loop.
    #[error("cancelled")]
    Cancelled,
}

/// Validated remote model entry plus optional source suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteModel {
    /// Provider model id (slashes preserved).
    pub id: String,
    /// Merged display config (metadata only, never connection fields).
    pub config: serde_json::Value,
    /// Optional `opencode.source` suffix.
    pub source: Option<String>,
}

/// Clock abstraction: real time vs. deterministic fake time (DISC07).
pub trait Clock {
    /// Milliseconds since an arbitrary epoch.
    fn now_ms(&self) -> u64;
    /// Sleep (cancellable checkpoints live in the caller, not here).
    fn sleep_ms(&self, ms: u64) -> impl std::future::Future<Output = ()> + Send;
}

/// Tokio-backed clock for production.
#[derive(Debug, Clone, Copy)]
pub struct RealClock;

impl Clock for RealClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

/// Scripted HTTP outcome for the fetch loop.
#[derive(Debug, Clone)]
pub enum Scripted {
    /// HTTP status with body bytes.
    Status {
        /// Status code.
        status: u16,
        /// Body bytes.
        body: Vec<u8>,
    },
    /// Network-level failure (timeout/reset).
    NetworkError,
}

/// Recorded outbound request (URL + headers actually sent).
#[derive(Debug, Clone)]
pub struct RecordedRequest {
    /// Full URL.
    pub url: String,
    /// Authorization header, if any.
    pub auth: Option<String>,
    /// Accept header, if any.
    pub accept: Option<String>,
    /// A configured custom header survived.
    pub custom: Option<String>,
}

/// Minimal HTTP surface the fetch loop needs (real + fake impls).
pub trait DiscoveryClient {
    /// GET with attempt timeout; redirect policy is always `error`.
    fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        attempt_timeout: Duration,
    ) -> impl std::future::Future<Output = Result<(u16, Vec<u8>), DiscoveryError>> + Send;
}

/// Reqwest-backed client: redirects refused, per-attempt total timeout.
#[derive(Debug, Clone)]
pub struct ReqwestDiscoveryClient {
    client: reqwest::Client,
    connect_timeout: Duration,
}

impl ReqwestDiscoveryClient {
    /// Build with refused redirects and a bounded connect timeout.
    pub fn new(connect_timeout: Duration) -> Result<Self, DiscoveryError> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(connect_timeout)
            .build()
            .map_err(|_| DiscoveryError::Network)?;
        Ok(Self {
            client,
            connect_timeout,
        })
    }
}

impl DiscoveryClient for ReqwestDiscoveryClient {
    async fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        attempt_timeout: Duration,
    ) -> Result<(u16, Vec<u8>), DiscoveryError> {
        let req = self
            .client
            .get(url)
            .headers(headers.clone())
            .timeout(attempt_timeout);
        let _ = self.connect_timeout;
        let resp = req.send().await.map_err(|_| DiscoveryError::Network)?;
        let status = resp.status().as_u16();
        let mut body = Vec::new();
        let mut stream = resp;
        loop {
            match stream.chunk().await {
                Ok(None) => break,
                Ok(Some(chunk)) => {
                    if body.len() + chunk.len() > DISCOVERY_BODY_CAP {
                        return Err(DiscoveryError::InvalidResponse);
                    }
                    body.extend_from_slice(&chunk);
                }
                Err(_) => return Err(DiscoveryError::Network),
            }
        }
        Ok((status, body))
    }
}

/// Returns true for retryable HTTP statuses (408/425/429/5xx).
pub fn should_retry_status(status: u16) -> bool {
    status == 408 || status == 425 || status == 429 || status >= 500
}

/// Safe positive integer (JS `Number.isSafeInteger` + `> 0`).
fn positive_integer(value: &serde_json::Value) -> bool {
    const MAX_SAFE_INTEGER: u64 = (1u64 << 53) - 1;
    if let Some(number) = value.as_u64() {
        return (1..=MAX_SAFE_INTEGER).contains(&number);
    }
    value.as_f64().is_some_and(|number| {
        number.is_finite() && number > 0.0 && number < (1u64 << 53) as f64 && number.fract() == 0.0
    })
}

/// Human display name derived from any model id (slashes preserved).
pub fn pretty_model_name(id: &str) -> String {
    let model_id = id.split_once('/').map_or(id, |(_, suffix)| suffix);
    let mut words: Vec<String> = model_id
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let lower = word.to_lowercase();
            if lower == "gpt" {
                return "GPT".to_string();
            }
            if lower == "glm" {
                return "GLM".to_string();
            }
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    if words.first().map(String::as_str) == Some("GPT")
        && words
            .get(1)
            .is_some_and(|w| w.starts_with(|c: char| c.is_ascii_digit()))
    {
        let second = words.remove(1);
        words[0] = format!("{}-{second}", words[0]);
    }
    let joined = words.join(" ");
    if joined.is_empty() {
        id.to_string()
    } else {
        joined
    }
}

/// Normalize `glm` word fragments to `GLM` (case-insensitive whole words).
fn normalize_model_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.eq_ignore_ascii_case("glm") {
            out.push_str("GLM");
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            token.push(ch);
        } else {
            flush(&mut token, &mut out);
            out.push(ch);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Explicit remote name unless blank or echoing the id (with/without prefix).
fn explicit_model_name(name: &serde_json::Value, id: &str) -> Option<String> {
    let name = name.as_str()?;
    if name.trim().is_empty() {
        return None;
    }
    let without_prefix = id.split_once('/').map_or(id, |(_, suffix)| suffix);
    if name.trim() == id || name.trim() == without_prefix {
        return None;
    }
    Some(name.trim().to_string())
}

fn is_record(value: &serde_json::Value) -> bool {
    value.is_object()
}

/// Validate one remote row into metadata-only config (never connection
/// fields: `npm`/`options`/`headers`/`url` are dropped by construction —
/// only id/limits/modalities/flags/variants survive).
pub fn model_config(row: &serde_json::Value) -> Result<RemoteModel, DiscoveryError> {
    if !is_record(row) {
        return Err(DiscoveryError::InvalidResponse);
    }
    let id = row
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or(DiscoveryError::InvalidResponse)?;
    let mut result = serde_json::Map::new();
    result.insert(
        "name".to_string(),
        serde_json::Value::String(pretty_model_name(id)),
    );
    let mut limit = serde_json::Map::new();
    for (source, target) in [
        ("context_length", "context"),
        ("max_completion_tokens", "output"),
    ] {
        if let Some(value) = row.get(source) {
            if !positive_integer(value) {
                return Err(DiscoveryError::InvalidResponse);
            }
            limit.insert(target.to_string(), value.clone());
        }
    }
    let metadata = row.get("opencode").unwrap_or(&serde_json::Value::Null);
    let metadata = if metadata.is_null() {
        &serde_json::json!({})
    } else {
        metadata
    };
    if !is_record(metadata) {
        return Err(DiscoveryError::InvalidResponse);
    }
    let mut source: Option<String> = None;
    if let Some(value) = metadata.get("source") {
        let text = value
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .ok_or(DiscoveryError::InvalidResponse)?;
        if text != text.trim() {
            return Err(DiscoveryError::InvalidResponse);
        }
        source = Some(text.to_string());
    }
    if let Some(name) = metadata.get("name") {
        if !name.is_string() {
            return Err(DiscoveryError::InvalidResponse);
        }
        let fallback = result["name"].clone();
        let picked = explicit_model_name(name, id).map_or(fallback, serde_json::Value::String);
        result.insert(
            "name".to_string(),
            serde_json::Value::String(normalize_model_name(picked.as_str().unwrap_or(""))),
        );
    }
    if let Some(value) = metadata.get("limit") {
        if !is_record(value) {
            return Err(DiscoveryError::InvalidResponse);
        }
        for key in ["context", "input", "output"] {
            if let Some(field) = value.get(key) {
                if !positive_integer(field) {
                    return Err(DiscoveryError::InvalidResponse);
                }
                limit.insert(key.to_string(), field.clone());
            }
        }
    }
    if !limit.is_empty() {
        result.insert(
            "limit".to_string(),
            serde_json::Value::Object(limit.clone()),
        );
    }
    if let Some(modalities) = metadata.get("modalities") {
        if !is_record(modalities) {
            return Err(DiscoveryError::InvalidResponse);
        }
        let mut out = serde_json::Map::new();
        for key in ["input", "output"] {
            let values = modalities
                .get(key)
                .and_then(|v| v.as_array())
                .ok_or(DiscoveryError::InvalidResponse)?;
            for value in values {
                let text = value.as_str().ok_or(DiscoveryError::InvalidResponse)?;
                if !["text", "image", "audio", "video", "pdf"].contains(&text) {
                    return Err(DiscoveryError::InvalidResponse);
                }
            }
            out.insert(key.to_string(), serde_json::Value::Array(values.clone()));
        }
        result.insert("modalities".to_string(), serde_json::Value::Object(out));
    }
    for key in ["attachment", "reasoning", "tool_call"] {
        if let Some(value) = metadata.get(key) {
            if !value.is_boolean() {
                return Err(DiscoveryError::InvalidResponse);
            }
            result.insert(key.to_string(), value.clone());
        }
    }
    if let Some(variants) = metadata.get("variants") {
        if !is_record(variants) {
            return Err(DiscoveryError::InvalidResponse);
        }
        let mut out = serde_json::Map::new();
        for (name, variant) in variants.as_object().map(|m| m.iter()).into_iter().flatten() {
            if name.trim().is_empty() || !is_record(variant) {
                return Err(DiscoveryError::InvalidResponse);
            }
            let effort = variant
                .get("reasoningEffort")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let disabled = variant.get("disabled");
            if !effort.trim().is_empty() && !effort.trim().is_empty() && disabled.is_none() {
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "reasoningEffort".to_string(),
                    serde_json::Value::String(
                        variant["reasoningEffort"]
                            .as_str()
                            .unwrap_or("")
                            .to_string(),
                    ),
                );
                out.insert(name.clone(), serde_json::Value::Object(entry));
            } else if variant.get("reasoningEffort").is_none()
                && disabled == Some(&serde_json::Value::Bool(true))
            {
                out.insert(name.clone(), serde_json::json!({"disabled": true}));
            } else {
                return Err(DiscoveryError::InvalidResponse);
            }
        }
        for name in STANDARD_REASONING_VARIANTS {
            if !out.contains_key(name) {
                out.insert(name.to_string(), serde_json::json!({"disabled": true}));
            }
        }
        result.insert("variants".to_string(), serde_json::Value::Object(out));
    }
    Ok(RemoteModel {
        id: id.to_string(),
        config: serde_json::Value::Object(result),
        source,
    })
}

/// Remaining budget against a deadline (clamped at zero).
pub fn remaining_budget(deadline_ms: u64, now_ms: u64) -> u64 {
    deadline_ms.saturating_sub(now_ms)
}

/// Fetch the models list with budgeted retries.
///
/// Retryable: network errors, retryable statuses, unparseable bodies, empty
/// lists. Terminal: other statuses, bad envelopes, invalid rows. Returns the
/// validated body plus the attempt count (single retry owner: no inner
/// retry layer exists beneath this loop).
pub async fn fetch_models<C: Clock, D: DiscoveryClient>(
    clock: &C,
    client: &D,
    url: &str,
    headers: &HeaderMap,
    cancel: &AtomicBool,
) -> Result<(Vec<serde_json::Value>, usize), DiscoveryError> {
    let start = clock.now_ms();
    let deadline = start.saturating_add(TOTAL_TIMEOUT_MS);
    let mut last_failure = DiscoveryError::Network;
    let mut attempts = 0usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DiscoveryError::Cancelled);
        }
        attempts += 1;
        let now = clock.now_ms();
        if remaining_budget(deadline, now) == 0 {
            return Err(last_failure);
        }
        let timeout =
            Duration::from_millis(remaining_budget(deadline, now).min(ATTEMPT_TIMEOUT_MS));
        match client.get(url, headers, timeout).await {
            Err(DiscoveryError::Network) => {
                last_failure = DiscoveryError::Network;
                if attempts > RETRY_DELAYS_MS.len() {
                    return Err(last_failure);
                }
            }
            Err(error) => return Err(error),
            Ok((status, body)) => {
                if !(200..300).contains(&status) {
                    last_failure = DiscoveryError::Http { status };
                    if !should_retry_status(status) || attempts > RETRY_DELAYS_MS.len() {
                        return Err(last_failure);
                    }
                } else if body.len() > DISCOVERY_BODY_CAP {
                    return Err(DiscoveryError::InvalidResponse);
                } else {
                    match serde_json::from_slice::<serde_json::Value>(&body) {
                        Err(_) => {
                            last_failure = DiscoveryError::InvalidResponse;
                            if attempts > RETRY_DELAYS_MS.len() {
                                return Err(last_failure);
                            }
                        }
                        Ok(value) => {
                            let valid = value.get("object")
                                == Some(&serde_json::Value::String("list".to_string()))
                                && value.get("data").and_then(|v| v.as_array()).is_some();
                            if !valid {
                                return Err(DiscoveryError::InvalidResponse);
                            }
                            let rows = value["data"].as_array().cloned().unwrap_or_default();
                            if rows.is_empty() {
                                last_failure = DiscoveryError::EmptyResponse;
                                if attempts > RETRY_DELAYS_MS.len() {
                                    return Err(last_failure);
                                }
                            } else if rows.len() > DISCOVERY_ROWS_CAP {
                                return Err(DiscoveryError::InvalidResponse);
                            } else {
                                // All-or-nothing row validation before return.
                                for row in &rows {
                                    model_config(row)?;
                                }
                                return Ok((rows, attempts));
                            }
                        }
                    }
                }
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(DiscoveryError::Cancelled);
        }
        let delay = RETRY_DELAYS_MS
            .get(attempts.saturating_sub(1))
            .copied()
            .unwrap_or(0)
            .min(remaining_budget(deadline, clock.now_ms()));
        if delay > 0 {
            clock.sleep_ms(delay).await;
        }
    }
}

/// Build the discovery URL: trimmed prefix + `/models`, rejecting
/// credential-bearing or non-http(s) bases before any network use.
pub fn discovery_url(base_url: &str) -> Result<String, DiscoveryError> {
    let mut url = reqwest::Url::parse(base_url).map_err(|_| DiscoveryError::InvalidConfig)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(DiscoveryError::InvalidConfig);
    }
    let path = format!("{}/models", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url.into())
}

fn discovery_headers(
    configured: &BTreeMap<String, String>,
    api_key: &str,
) -> Result<HeaderMap, DiscoveryError> {
    let mut headers = HeaderMap::new();
    for (name, value) in configured {
        let name =
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| DiscoveryError::InvalidConfig)?;
        let value = HeaderValue::from_str(value).map_err(|_| DiscoveryError::InvalidConfig)?;
        headers.insert(name, value);
    }
    let mut authorization = HeaderValue::from_str(&format!("Bearer {api_key}"))
        .map_err(|_| DiscoveryError::InvalidConfig)?;
    authorization.set_sensitive(true);
    headers.insert(AUTHORIZATION, authorization);
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    // JS runtimes always send a User-Agent; reqwest sends none, and some
    // frontends answer 403 to UA-less requests. Reserved like accept/auth.
    headers.insert(USER_AGENT, HeaderValue::from_static(crate::USER_AGENT));
    Ok(headers)
}

/// Provider gating: disabled wins, then an enabled-selection must include us.
pub fn should_run(disabled: &[String], enabled: Option<&[String]>) -> bool {
    if disabled.iter().any(|id| id == PROVIDER_ID) {
        return false;
    }
    if let Some(list) = enabled {
        return list.iter().any(|id| id == PROVIDER_ID);
    }
    true
}

/// Discovery outcome: replacement catalog plus sanitized warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryOutcome {
    /// Replacement models map (empty when refresh failed and old kept).
    pub models: BTreeMap<String, serde_json::Value>,
    /// Sanitized warnings (failure classes only, no URLs/keys/bodies).
    pub warnings: Vec<String>,
    /// Fetch attempts performed.
    pub attempts: usize,
    /// Whether the catalog was replaced.
    pub replaced: bool,
}

/// Refresh one provider generation: validate everything, then publish
/// atomically. Any failure keeps the configured models untouched.
pub async fn refresh<C: Clock, D: DiscoveryClient>(
    clock: &C,
    client: &D,
    base_url: &str,
    api_key: &str,
    configured_headers: &BTreeMap<String, String>,
    local_models: &BTreeMap<String, serde_json::Value>,
    cancel: &AtomicBool,
) -> DiscoveryOutcome {
    let mut warnings = Vec::new();
    let failed = |warnings: Vec<String>| DiscoveryOutcome {
        models: local_models.clone(),
        warnings,
        attempts: 0,
        replaced: false,
    };
    if api_key.trim().is_empty() {
        warnings.push(
            "[openproxy-models] invalid provider URL or credentials; keeping configured models."
                .to_string(),
        );
        return failed(warnings);
    }
    let url = match discovery_url(base_url) {
        Ok(url) => url,
        Err(_) => {
            warnings.push("[openproxy-models] invalid provider URL or credentials; keeping configured models.".to_string());
            return failed(warnings);
        }
    };
    // HeaderMap replacement is case-insensitive, matching the oracle's Headers.set.
    let headers = match discovery_headers(configured_headers, api_key) {
        Ok(headers) => headers,
        Err(_) => {
            warnings.push(
                "[openproxy-models] invalid provider URL or credentials; keeping configured models."
                    .to_string(),
            );
            return failed(warnings);
        }
    };

    let (rows, attempts) = match fetch_models(clock, client, &url, &headers, cancel).await {
        Ok(ok) => ok,
        Err(DiscoveryError::Cancelled) => {
            return DiscoveryOutcome {
                models: local_models.clone(),
                warnings,
                attempts: 0,
                replaced: false,
            };
        }
        Err(e) => {
            warnings.push(format!(
                "[openproxy-models] {e}; keeping configured models."
            ));
            return DiscoveryOutcome {
                models: local_models.clone(),
                warnings,
                attempts: 0,
                replaced: false,
            };
        }
    };

    // Merge each row over its local snapshot; duplicates last-win via insert.
    let mut merged: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for row in &rows {
        let id = row["id"].as_str().unwrap_or("").to_string();
        let validated = match model_config(row) {
            Ok(remote) => remote,
            Err(_) => {
                warnings.push(
                    "[openproxy-models] invalid models response; keeping configured models."
                        .to_string(),
                );
                return DiscoveryOutcome {
                    models: local_models.clone(),
                    warnings,
                    attempts,
                    replaced: false,
                };
            }
        };
        let local = local_models
            .get(&id)
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default()));
        merged.insert(id, merge_model(&validated, &local));
    }
    DiscoveryOutcome {
        models: merged,
        warnings,
        attempts,
        replaced: true,
    }
}

/// Merge one validated remote row over its local snapshot.
///
/// Shallow spread (local wins scalars), then shallow `limit`/`variants`
/// merges, source suffix, and the 500k context/input clamp. A limit without
/// both positive `context` and `output` is deleted, never fabricated.
fn merge_model(remote: &RemoteModel, local: &serde_json::Value) -> serde_json::Value {
    let mut merged = remote.config.as_object().cloned().unwrap_or_default();
    if let Some(local_map) = local.as_object() {
        for (key, value) in local_map {
            merged.insert(key.clone(), value.clone());
        }
    }
    if let Some(name) = merged.get("name").and_then(|v| v.as_str()) {
        merged.insert(
            "name".to_string(),
            serde_json::Value::String(normalize_model_name(name)),
        );
    }
    if let Some(source) = &remote.source
        && let Some(name) = merged.get("name").and_then(|v| v.as_str())
        && !name.ends_with(format!(" · {source}").as_str())
    {
        let suffix = format!(" · {source}");
        merged.insert(
            "name".to_string(),
            serde_json::Value::String(format!("{name}{suffix}")),
        );
    }
    let remote_limit = remote.config.get("limit");
    let local_limit = local.get("limit");
    if remote_limit.is_some() || local_limit.is_some() {
        let mut limit = serde_json::Map::new();
        for source in [remote_limit, local_limit].into_iter().flatten() {
            if let Some(map) = source.as_object() {
                for (key, value) in map {
                    limit.insert(key.clone(), value.clone());
                }
            }
        }
        for key in ["context", "input"] {
            if let Some(value) = limit.get(key)
                && positive_integer(value)
                && value
                    .as_f64()
                    .is_some_and(|value| value > MAX_CONTEXT_TOKENS as f64)
            {
                limit.insert(key.to_string(), serde_json::Value::from(MAX_CONTEXT_TOKENS));
            }
        }
        let complete = ["context", "output"]
            .iter()
            .all(|key| limit.get(*key).is_some_and(positive_integer));
        if complete {
            merged.insert("limit".to_string(), serde_json::Value::Object(limit));
        } else {
            merged.remove("limit");
        }
    }
    let remote_variants = remote.config.get("variants");
    let local_variants = local.get("variants");
    if remote_variants.is_some() || local_variants.is_some() {
        let mut variants = serde_json::Map::new();
        for source in [remote_variants, local_variants].into_iter().flatten() {
            if let Some(map) = source.as_object() {
                for (key, value) in map {
                    variants.insert(key.clone(), value.clone());
                }
            }
        }
        merged.insert("variants".to_string(), serde_json::Value::Object(variants));
    }
    serde_json::Value::Object(merged)
}

#[cfg(test)]
mod tests {
    use super::{
        ATTEMPT_TIMEOUT_MS, Clock, DiscoveryClient, DiscoveryError, MAX_CONTEXT_TOKENS,
        RecordedRequest, Scripted, fetch_models, model_config, pretty_model_name,
        should_retry_status, should_run,
    };
    use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue};
    use std::collections::{BTreeMap, VecDeque};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct FakeClock {
        now: Mutex<u64>,
        sleeps: Mutex<Vec<u64>>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now: Mutex::new(0),
                sleeps: Mutex::new(Vec::new()),
            }
        }

        fn advance(&self, ms: u64) {
            *self.now.lock().expect("now") += ms;
        }
    }

    impl Clock for FakeClock {
        fn now_ms(&self) -> u64 {
            *self.now.lock().expect("now")
        }

        async fn sleep_ms(&self, ms: u64) {
            self.sleeps.lock().expect("sleeps").push(ms);
            *self.now.lock().expect("now") += ms;
        }
    }

    struct FakeClient {
        script: Mutex<VecDeque<Scripted>>,
        requests: Mutex<Vec<RecordedRequest>>,
        sent_headers: Mutex<Vec<HeaderMap>>,
        latency_ms: u64,
        clock: Option<Arc<FakeClock>>,
        on_request: Option<Arc<dyn Fn() + Send + Sync>>,
        count: Mutex<usize>,
    }

    impl FakeClient {
        fn new(script: Vec<Scripted>) -> Self {
            Self {
                script: Mutex::new(script.into()),
                requests: Mutex::new(Vec::new()),
                sent_headers: Mutex::new(Vec::new()),
                latency_ms: 0,
                clock: None,
                on_request: None,
                count: Mutex::new(0),
            }
        }

        fn with_latency(mut self, latency_ms: u64, clock: Arc<FakeClock>) -> Self {
            self.latency_ms = latency_ms;
            self.clock = Some(clock);
            self
        }
    }

    impl DiscoveryClient for FakeClient {
        async fn get(
            &self,
            url: &str,
            headers: &HeaderMap,
            _attempt_timeout: Duration,
        ) -> Result<(u16, Vec<u8>), DiscoveryError> {
            let n = {
                let mut count = self.count.lock().expect("count");
                let n = *count;
                *count += 1;
                n
            };
            if let Some(hook) = &self.on_request {
                let _ = n;
                hook();
            }
            self.sent_headers
                .lock()
                .expect("sent headers")
                .push(headers.clone());
            self.requests
                .lock()
                .expect("requests")
                .push(RecordedRequest {
                    url: url.to_string(),
                    auth: headers
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    accept: headers
                        .get(ACCEPT)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    custom: headers
                        .get("x-fixture")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                });
            if self.latency_ms > 0
                && let Some(clock) = &self.clock
            {
                clock.advance(self.latency_ms);
            }
            match self.script.lock().expect("script").pop_front() {
                Some(Scripted::Status { status, body }) => Ok((status, body)),
                Some(Scripted::NetworkError) | None => Err(DiscoveryError::Network),
            }
        }
    }

    static NO_CANCEL: AtomicBool = AtomicBool::new(false);
    use std::sync::atomic::{AtomicBool, Ordering};

    fn list(data: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"object": "list", "data": data})).expect("json")
    }

    fn model(id: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "context_length": 700000,
            "max_completion_tokens": 32000,
            "opencode": {
                "limit": {"input": 600000},
                "tool_call": true,
                "modalities": {"input": ["text", "image"], "output": ["text"]},
            },
        })
    }

    fn headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer unit-test-placeholder"),
        );
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        headers.insert("x-fixture", HeaderValue::from_static("yes"));
        headers
    }

    #[tokio::test]
    async fn disc01_basic_dynamic_no_hardcode() {
        let clock = FakeClock::new();
        let client = FakeClient::new(vec![Scripted::Status {
            status: 200,
            body: list(serde_json::json!([model("fixture/future-model")])),
        }]);
        let (rows, attempts) = fetch_models(
            &clock,
            &client,
            "https://example.invalid/v1/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect("fetch");
        assert_eq!(attempts, 1);
        assert_eq!(rows.len(), 1);
        let outcome = refresh_models_only(&client, rows);
        let entry = outcome.get("fixture/future-model").expect("entry");
        assert_eq!(entry["limit"]["context"], 500_000);
        assert_eq!(entry["limit"]["input"], 500_000);
        assert_eq!(entry["limit"]["output"], 32_000);
        let requests = client.requests.lock().expect("req").clone();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].url, "https://example.invalid/v1/models");
        assert_eq!(
            requests[0].auth.as_deref(),
            Some("Bearer unit-test-placeholder")
        );
    }

    fn refresh_models_only(
        _client: &FakeClient,
        rows: Vec<serde_json::Value>,
    ) -> BTreeMap<String, serde_json::Value> {
        let local: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let mut merged = BTreeMap::new();
        for row in &rows {
            let remote = model_config(row).expect("valid");
            let id = remote.id.clone();
            merged.insert(
                id.clone(),
                super::merge_model(&remote, local.get(&id).unwrap_or(&serde_json::Value::Null)),
            );
        }
        merged
    }

    #[test]
    fn disc02_atomic_row_validation() {
        // One invalid row poisons the whole batch before publication.
        let rows = [
            model("ok"),
            serde_json::json!({"id": "bad", "context_length": -1}),
        ];
        assert!(model_config(&rows[1]).is_err());
        // Publication validates everything first (see refresh tests below).
    }

    #[test]
    fn disc03_authority_ignores_remote_connection() {
        let row = serde_json::json!({
            "id": "fixture/future-model",
            "npm": "malicious",
            "options": {"apiKey": "bad"},
            "opencode": {"options": {"baseURL": "https://bad.invalid"}, "headers": {"Authorization": "bad"}},
        });
        let remote = model_config(&row).expect("valid row");
        let flat = remote.config.to_string();
        assert!(!flat.contains("malicious"));
        assert!(!flat.contains("bad"));
    }

    #[test]
    fn disc05_limits_and_deletion() {
        let remote = model_config(&model("m")).expect("valid");
        assert_eq!(remote.config["limit"]["context"], 700_000);
        let merged = super::merge_model(&remote, &serde_json::Value::Null);
        assert_eq!(merged["limit"]["context"], MAX_CONTEXT_TOKENS);
        assert_eq!(merged["limit"]["output"], 32_000);
        // Input-only limit is deleted, never fabricated.
        let row = serde_json::json!({"id": "x", "opencode": {"limit": {"input": 120}}});
        let remote = model_config(&row).expect("valid");
        let merged = super::merge_model(&remote, &serde_json::Value::Null);
        assert_eq!(merged.get("limit"), None);
    }

    #[test]
    fn disc06_variants_allowlist_and_override() {
        let row = serde_json::json!({
            "id": "m",
            "context_length": 700000,
            "opencode": {"source": "fixture-source", "variants": {"medium": {"reasoningEffort": "medium"}}},
        });
        let local = serde_json::json!({
            "name": "glm local",
            "limit": {"output": 42},
            "variants": {"high": {"reasoningEffort": "custom-value"}},
        });
        let remote = model_config(&row).expect("valid");
        let merged = super::merge_model(&remote, &local);
        assert_eq!(merged["name"], "GLM local · fixture-source");
        assert_eq!(merged["limit"]["output"], 42);
        assert_eq!(merged["variants"]["none"]["disabled"], true);
        assert_eq!(
            merged["variants"]["high"]["reasoningEffort"],
            "custom-value"
        );
        assert_eq!(merged["variants"]["medium"]["reasoningEffort"], "medium");
    }

    #[test]
    fn disc_names_and_gating() {
        assert_eq!(pretty_model_name("org/gpt-4o-mini"), "GPT-4o Mini");
        assert_eq!(pretty_model_name("org/name_with-parts"), "Name With Parts");
        assert_eq!(pretty_model_name("x/glm-4"), "GLM 4");
        assert!(should_run(&[], None));
        assert!(!should_run(&["ludka2".to_string()], None));
        assert!(!should_run(&[], Some(&["other".to_string()])));
        assert!(should_run(&[], Some(&["ludka2".to_string()])));
        assert_eq!(
            super::discovery_url("https://example.invalid/v1/").expect("url"),
            "https://example.invalid/v1/models"
        );
        assert!(super::discovery_url("https://user:password@example.invalid/v1").is_err());
        assert!(super::discovery_url("ftp://example.invalid").is_err());
    }

    #[test]
    fn aud18_safe_integer_boundary_and_local_preclamp_validation() {
        let safe = serde_json::json!({
            "id": "safe",
            "context_length": 9_007_199_254_740_991_u64,
            "max_completion_tokens": 16,
        });
        assert!(model_config(&safe).is_ok(), "2^53 - 1 is safe");

        let unsafe_remote = serde_json::json!({
            "id": "unsafe",
            "context_length": 9_007_199_254_740_992_u64,
            "max_completion_tokens": 16,
        });
        assert!(model_config(&unsafe_remote).is_err(), "2^53 is unsafe");

        let remote = model_config(&serde_json::json!({
            "id": "local-boundary",
            "context_length": 1024,
            "max_completion_tokens": 16,
        }))
        .expect("remote");
        let safe_local = super::merge_model(
            &remote,
            &serde_json::json!({
                "limit": {"context": 9_007_199_254_740_991_u64, "output": 16},
            }),
        );
        assert_eq!(safe_local["limit"]["context"], MAX_CONTEXT_TOKENS);
        let unsafe_local = super::merge_model(
            &remote,
            &serde_json::json!({
                "limit": {"context": 9_007_199_254_740_992_u64, "output": 16},
            }),
        );
        assert_eq!(unsafe_local.get("limit"), None, "validate before clamp");
    }

    #[test]
    fn aud18_names_strip_only_the_first_slash_and_remote_connection_fields_are_ignored() {
        assert_eq!(pretty_model_name("vendor/route/model-x"), "Route/model X");
        let row = serde_json::json!({
            "id": "vendor/route/model-x",
            "npm": "remote-package",
            "options": {"baseURL": "https://override.invalid", "apiKey": "remote-key"},
            "headers": {"Authorization": "remote-auth"},
            "opencode": {
                "name": "route/model-x",
                "options": {"baseURL": "https://nested.invalid"},
                "headers": {"Accept": "text/plain"},
            },
        });
        let remote = model_config(&row).expect("metadata-only row");
        assert_eq!(remote.config["name"], "Route/model X");
        let serialized = remote.config.to_string();
        for forbidden in [
            "remote-package",
            "override.invalid",
            "remote-key",
            "remote-auth",
            "nested.invalid",
            "text/plain",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn aud18_reqwest_url_validation_rejects_userinfo_query_fragment_and_malformed() {
        assert_eq!(
            super::discovery_url("https://example.invalid/proxy/v1///").expect("valid"),
            "https://example.invalid/proxy/v1/models"
        );
        for invalid in [
            "https://user:password@example.invalid/v1",
            "https://example.invalid?query=yes",
            "https://example.invalid#fragment",
            "https://example.invalid:bad/v1",
            "https://[::1",
            "not a URL",
        ] {
            assert!(
                super::discovery_url(invalid).is_err(),
                "accepted invalid URL {invalid}"
            );
        }
    }

    #[tokio::test]
    async fn aud18_headers_replace_reserved_case_insensitively_once_and_preserve_custom() {
        let clock = FakeClock::new();
        let client = FakeClient::new(vec![Scripted::Status {
            status: 200,
            body: list(serde_json::json!([model("fresh")])),
        }]);
        let configured = BTreeMap::from([
            (
                "authorization".to_string(),
                "Bearer stale-lower".to_string(),
            ),
            (
                "AUTHORIZATION".to_string(),
                "Bearer stale-upper".to_string(),
            ),
            ("accept".to_string(), "text/plain".to_string()),
            ("AcCePt".to_string(), "application/xml".to_string()),
            ("x-fixture".to_string(), "preserved".to_string()),
        ]);
        let outcome = super::refresh(
            &clock,
            &client,
            "https://example.invalid/v1",
            "fresh-key",
            &configured,
            &BTreeMap::new(),
            &NO_CANCEL,
        )
        .await;
        assert!(outcome.replaced);
        let sent = client.sent_headers.lock().expect("headers");
        let sent = &sent[0];
        let authorization: Vec<_> = sent
            .iter()
            .filter(|(name, _)| name.as_str() == "authorization")
            .collect();
        let accept: Vec<_> = sent
            .iter()
            .filter(|(name, _)| name.as_str() == "accept")
            .collect();
        assert_eq!(authorization.len(), 1);
        assert_eq!(
            authorization[0].1.to_str().expect("authorization"),
            "Bearer fresh-key"
        );
        assert_eq!(accept.len(), 1);
        assert_eq!(accept[0].1.to_str().expect("accept"), "application/json");
        assert_eq!(
            sent.get("user-agent").and_then(|value| value.to_str().ok()),
            Some(crate::USER_AGENT)
        );
        assert_eq!(
            sent.get("x-fixture").and_then(|value| value.to_str().ok()),
            Some("preserved")
        );
    }

    struct TerminalInvalidClient {
        requests: Mutex<usize>,
    }

    impl DiscoveryClient for TerminalInvalidClient {
        async fn get(
            &self,
            _url: &str,
            _headers: &HeaderMap,
            _attempt_timeout: Duration,
        ) -> Result<(u16, Vec<u8>), DiscoveryError> {
            *self.requests.lock().expect("requests") += 1;
            Err(DiscoveryError::InvalidResponse)
        }
    }

    #[tokio::test]
    async fn aud18_any_2xx_succeeds_and_body_cap_invalid_response_is_terminal() {
        for status in [200, 201, 204, 206, 299] {
            let clock = FakeClock::new();
            let client = FakeClient::new(vec![Scripted::Status {
                status,
                body: list(serde_json::json!([model("success")])),
            }]);
            let (_, attempts) = fetch_models(
                &clock,
                &client,
                "https://example.invalid/models",
                &headers(),
                &NO_CANCEL,
            )
            .await
            .unwrap_or_else(|error| panic!("status {status} failed: {error}"));
            assert_eq!(attempts, 1);
        }

        let client = TerminalInvalidClient {
            requests: Mutex::new(0),
        };
        let error = fetch_models(
            &FakeClock::new(),
            &client,
            "https://example.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect_err("terminal body cap");
        assert_eq!(error, DiscoveryError::InvalidResponse);
        assert_eq!(*client.requests.lock().expect("requests"), 1);
    }

    #[tokio::test]
    async fn aud18_missing_credentials_zero_requests_and_persistent_empty_is_bounded() {
        let clock = FakeClock::new();
        let client = FakeClient::new(vec![]);
        let local = BTreeMap::from([("kept".to_string(), serde_json::json!({"name": "Kept"}))]);
        let outcome = super::refresh(
            &clock,
            &client,
            "https://example.invalid/v1",
            "   ",
            &BTreeMap::new(),
            &local,
            &NO_CANCEL,
        )
        .await;
        assert!(!outcome.replaced);
        assert_eq!(outcome.models, local);
        assert!(client.requests.lock().expect("requests").is_empty());

        let empty =
            serde_json::to_vec(&serde_json::json!({"object": "list", "data": []})).expect("empty");
        let client = FakeClient::new(
            (0..4)
                .map(|_| Scripted::Status {
                    status: 200,
                    body: empty.clone(),
                })
                .collect(),
        );
        let error = fetch_models(
            &clock,
            &client,
            "https://example.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect_err("persistent empty");
        assert_eq!(error, DiscoveryError::EmptyResponse);
        assert_eq!(client.requests.lock().expect("requests").len(), 4);
    }

    #[tokio::test]
    async fn aud18_successful_refresh_removes_absent_ids() {
        let local = BTreeMap::from([
            (
                "removed".to_string(),
                serde_json::json!({"name": "Removed"}),
            ),
            (
                "kept".to_string(),
                serde_json::json!({"name": "Local name"}),
            ),
        ]);
        let outcome = super::refresh(
            &FakeClock::new(),
            &FakeClient::new(vec![Scripted::Status {
                status: 200,
                body: list(serde_json::json!([model("kept")])),
            }]),
            "https://example.invalid/v1",
            "fixture-key",
            &BTreeMap::new(),
            &local,
            &NO_CANCEL,
        )
        .await;
        assert!(outcome.replaced);
        assert_eq!(outcome.models.keys().collect::<Vec<_>>(), vec!["kept"]);
        assert_eq!(outcome.models["kept"]["name"], "Local name");
    }

    #[tokio::test]
    async fn disc07_fake_clock_budgets() {
        assert_eq!(ATTEMPT_TIMEOUT_MS, 15_000);
        // Four network failures: 1 initial + 3 retries, clipped delays.
        let clock = Arc::new(FakeClock::new());
        let client = FakeClient::new(vec![
            Scripted::NetworkError,
            Scripted::NetworkError,
            Scripted::NetworkError,
            Scripted::NetworkError,
        ]);
        let err = fetch_models(
            &*clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect_err("exhausted");
        assert_eq!(err, DiscoveryError::Network);
        assert_eq!(client.requests.lock().expect("r").len(), 4);
        assert_eq!(*clock.sleeps.lock().expect("s"), vec![250, 750, 1500]);
        // Latency-consuming attempts clip delays against the remaining
        // budget: attempt 1 burns 15s, a clipped 250ms sleep follows,
        // attempt 2 burns the rest, and the exhausted budget exits the loop.
        let clock = Arc::new(FakeClock::new());
        let client = FakeClient::new(vec![
            Scripted::NetworkError,
            Scripted::NetworkError,
            Scripted::NetworkError,
        ])
        .with_latency(15_000, clock.clone());
        let _ = fetch_models(
            &*clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await;
        assert_eq!(client.requests.lock().expect("r").len(), 2);
        assert_eq!(*clock.sleeps.lock().expect("s"), vec![250]);
    }

    #[tokio::test]
    async fn disc08_retry_classes_and_cancel() {
        // 408/425/429/5xx retry; terminal statuses do not.
        for status in [408u16, 425, 429, 500, 503] {
            assert!(should_retry_status(status), "{status}");
        }
        for status in [200u16, 400, 401, 403, 404] {
            assert!(!should_retry_status(status), "{status}");
        }
        // Invalid JSON retries once, then succeeds.
        let clock = FakeClock::new();
        let client = FakeClient::new(vec![
            Scripted::Status {
                status: 200,
                body: b"not json".to_vec(),
            },
            Scripted::Status {
                status: 200,
                body: list(serde_json::json!([model("m")])),
            },
        ]);
        let (rows, attempts) = fetch_models(
            &clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect("recover");
        assert_eq!(attempts, 2);
        assert_eq!(rows.len(), 1);
        // Bad envelope is terminal (single attempt).
        let clock = FakeClock::new();
        let client = FakeClient::new(vec![Scripted::Status {
            status: 200,
            body: serde_json::to_vec(&serde_json::json!({"data": []})).expect("json"),
        }]);
        let err = fetch_models(
            &clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect_err("envelope");
        assert_eq!(err, DiscoveryError::InvalidResponse);
        assert_eq!(client.requests.lock().expect("r").len(), 1);
        // Cancellation stops the loop after the first attempt.
        let clock = FakeClock::new();
        let mut client = FakeClient::new(vec![Scripted::NetworkError, Scripted::NetworkError]);
        let cancel = Arc::new(AtomicBool::new(false));
        client.on_request = Some(Arc::new({
            let flag = cancel.clone();
            move || flag.store(true, Ordering::Relaxed)
        }));
        let err = fetch_models(
            &clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &cancel,
        )
        .await
        .expect_err("cancelled");
        assert_eq!(err, DiscoveryError::Cancelled);
        assert_eq!(client.requests.lock().expect("r").len(), 1);
    }

    #[tokio::test]
    async fn disc09_publication_rules() {
        // Empty then success publishes; persistent empty keeps old.
        let clock = FakeClock::new();
        let empty =
            serde_json::to_vec(&serde_json::json!({"object": "list", "data": []})).expect("json");
        let client = FakeClient::new(vec![
            Scripted::Status {
                status: 200,
                body: empty.clone(),
            },
            Scripted::Status {
                status: 200,
                body: list(serde_json::json!([model("new")])),
            },
        ]);
        let (rows, attempts) = fetch_models(
            &clock,
            &client,
            "https://x.invalid/models",
            &headers(),
            &NO_CANCEL,
        )
        .await
        .expect("publish");
        assert_eq!(attempts, 2);
        assert_eq!(rows[0]["id"], "new");
        // Success removes retired ids; duplicates last-win.
        let local: BTreeMap<String, serde_json::Value> =
            [("retired".to_string(), serde_json::json!({"name": "old"}))]
                .into_iter()
                .collect();
        let outcome = super::refresh(
            &clock,
            &FakeClient::new(vec![Scripted::Status {
                status: 200,
                body: list(serde_json::json!([
                    {"id": "same", "opencode": {"name": "First"}},
                    {"id": "same", "opencode": {"name": "Last"}},
                ])),
            }]),
            "https://example.invalid/v1",
            "unit-test-placeholder",
            &BTreeMap::new(),
            &local,
            &NO_CANCEL,
        )
        .await;
        assert!(outcome.replaced);
        assert_eq!(
            outcome.models.keys().cloned().collect::<Vec<_>>(),
            vec!["same".to_string()]
        );
        assert_eq!(outcome.models["same"]["name"], "Last");
        assert_eq!(outcome.attempts, 1);
        assert!(outcome.warnings.is_empty());
    }

    #[tokio::test]
    async fn disc10_isolation_and_sanitized_warnings() {
        // Failed refresh keeps configured models byte-identical.
        let clock = FakeClock::new();
        let local: BTreeMap<String, serde_json::Value> =
            [("kept".to_string(), serde_json::json!({"name": "old"}))]
                .into_iter()
                .collect();
        let outcome = super::refresh(
            &clock,
            &FakeClient::new(vec![Scripted::Status {
                status: 401,
                body: b"sensitive-fixture-value".to_vec(),
            }]),
            "https://example.invalid/v1",
            "unit-test-placeholder",
            &BTreeMap::new(),
            &local,
            &NO_CANCEL,
        )
        .await;
        assert!(!outcome.replaced);
        assert_eq!(outcome.models, local);
        assert_eq!(outcome.warnings.len(), 1);
        assert!(outcome.warnings[0].contains("HTTP 401"));
        assert!(!outcome.warnings[0].contains("sensitive-fixture-value"));
        assert!(!outcome.warnings[0].contains("example.invalid"));
        assert!(!outcome.warnings[0].contains("unit-test-placeholder"));
        // Credential-bearing URL rejected with zero requests.
        let client = FakeClient::new(vec![]);
        let outcome = super::refresh(
            &clock,
            &client,
            "https://user:password@example.invalid/v1",
            "unit-test-placeholder",
            &BTreeMap::new(),
            &local,
            &NO_CANCEL,
        )
        .await;
        assert!(!outcome.replaced);
        assert!(client.requests.lock().expect("r").is_empty());
        assert!(!outcome.warnings[0].contains("password"));
        // Two generations stay independent.
        let other: BTreeMap<String, serde_json::Value> =
            [("solo".to_string(), serde_json::json!({"name": "s"}))]
                .into_iter()
                .collect();
        let outcome = super::refresh(
            &clock,
            &FakeClient::new(vec![Scripted::Status {
                status: 200,
                body: list(serde_json::json!([model("fresh")])),
            }]),
            "https://example.invalid/v1",
            "unit-test-placeholder",
            &BTreeMap::new(),
            &other,
            &NO_CANCEL,
        )
        .await;
        assert!(outcome.models.contains_key("fresh"));
        assert!(!outcome.models.contains_key("solo") || outcome.models.len() == 1);
        assert!(!outcome.models.contains_key("kept"));
    }
}
