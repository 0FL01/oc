//! GET-only custom catalog discovery. The OpenProxy oracle remains unchanged.
use crate::discovery::{self, Clock, DiscoveryClient, DiscoveryError};
use oc_core::queries::{CustomProviderAuth, CustomProviderInput, NativeProtocol};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(test)]
mod tests;

pub(crate) struct CustomCatalog {
    pub(crate) models: BTreeMap<String, Value>,
    /// An OpenAI list is deliberately ambiguous. Only the Anthropic envelope is evidence.
    pub(crate) suggested: Option<NativeProtocol>,
}

fn text(value: &Value, limit: usize) -> Result<&str, DiscoveryError> {
    value
        .as_str()
        .filter(|s| !s.trim().is_empty() && s.len() <= limit && !s.chars().any(char::is_control))
        .ok_or(DiscoveryError::InvalidResponse)
}

/// Reuse the rich opencode row parser, normalizing nullable/partial custom metadata only.
fn model(row: &Value, anthropic: bool) -> Result<discovery::RemoteModel, DiscoveryError> {
    let id = text(row.get("id").ok_or(DiscoveryError::InvalidResponse)?, 4096)?;
    let mut metadata = match row.get("opencode") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(value)) => value.clone(),
        _ => return Err(DiscoveryError::InvalidResponse),
    };
    // Null is unknown, not false/zero/declared empty. Never pass an extension bag to execution.
    metadata.retain(|_, value| !value.is_null());
    if let Some(Value::Object(limits)) = metadata.get_mut("limit") {
        limits.retain(|_, value| !value.is_null());
    }
    let modalities = metadata.remove("modalities");
    let mut normalized = row.clone();
    normalized["id"] = Value::String(id.into());
    normalized["opencode"] = Value::Object(metadata);
    for key in ["context_length", "max_completion_tokens"] {
        if normalized.get(key).is_some_and(Value::is_null) {
            normalized
                .as_object_mut()
                .ok_or(DiscoveryError::InvalidResponse)?
                .remove(key);
        }
    }
    let mut model = discovery::model_config(&normalized)?;
    if let Some(modalities) = modalities {
        let source = modalities
            .as_object()
            .ok_or(DiscoveryError::InvalidResponse)?;
        let mut out = Map::new();
        for key in ["input", "output"] {
            if let Some(values) = source.get(key).filter(|v| !v.is_null()) {
                let values = values.as_array().ok_or(DiscoveryError::InvalidResponse)?;
                if values.iter().any(|v| {
                    !matches!(
                        v.as_str(),
                        Some("text" | "image" | "audio" | "video" | "pdf")
                    )
                }) {
                    return Err(DiscoveryError::InvalidResponse);
                }
                out.insert(key.into(), Value::Array(values.clone()));
            }
        }
        model.config["modalities"] = Value::Object(out);
    }
    if anthropic {
        if row.pointer("/opencode/name").is_none_or(Value::is_null)
            && let Some(name) = row.get("display_name").filter(|v| !v.is_null())
        {
            model.config["name"] = Value::String(text(name, 1024)?.into());
        }
        for (source, target) in [("max_input_tokens", "input"), ("max_tokens", "output")] {
            if let Some(value) = row.get(source).filter(|v| !v.is_null()) {
                if value.as_u64().is_none_or(|n| n == 0 || n >= (1 << 53)) {
                    return Err(DiscoveryError::InvalidResponse);
                }
                if model.config.pointer(&format!("/limit/{target}")).is_none() {
                    if model.config.get("limit").is_none() {
                        model.config["limit"] = json!({});
                    }
                    model.config["limit"][target] = value.clone();
                }
            }
        }
        if let Some(capabilities) = row.get("capabilities").filter(|v| !v.is_null()) {
            let capabilities = capabilities
                .as_object()
                .ok_or(DiscoveryError::InvalidResponse)?;
            let mut partial = Map::new();
            for (source, target) in [
                ("image_input", "image_input"),
                ("pdf_input", "pdf_input"),
                ("thinking", "reasoning"),
            ] {
                if let Some(supported) = support(capabilities.get(source))? {
                    partial.insert(target.into(), Value::Bool(supported));
                }
            }
            if !partial.is_empty() {
                model.config["partial_capabilities"] = Value::Object(partial);
            }
            if model.config.get("variants").is_none()
                && let Some(effort) = capabilities.get("effort").filter(|v| !v.is_null())
            {
                let declared = support(Some(effort))?;
                let levels = effort.as_object().ok_or(DiscoveryError::InvalidResponse)?;
                let mut variants = Map::new();
                let mut known = declared.is_some();
                for level in ["low", "medium", "high", "xhigh", "max"] {
                    let level_support = support(levels.get(level))?;
                    known |= level_support.is_some();
                    if level_support == Some(true) && declared != Some(false) {
                        variants.insert(level.into(), json!({"reasoningEffort":level}));
                    }
                }
                if known {
                    // The existing parser supplies disabled standard entries, never selectable defaults.
                    model.config["variants"] = Value::Object(variants);
                    let parsed = discovery::model_config(
                        &json!({"id":id,"opencode":{"variants":model.config["variants"]}}),
                    )?;
                    model.config["variants"] = parsed.config["variants"].clone();
                }
            }
        }
    }
    // Safe display fields are bounded too; terminal controls never enter a picker/cache.
    text(&model.config["name"], 1024)?;
    if let Some(variants) = model.config.get("variants").and_then(Value::as_object) {
        for (id, value) in variants {
            text(&Value::String(id.clone()), 256)?;
            if let Some(effort) = value.get("reasoningEffort") {
                text(effort, 256)?;
            }
        }
    }
    Ok(model)
}

fn support(value: Option<&Value>) -> Result<Option<bool>, DiscoveryError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => match value.get("supported") {
            None | Some(Value::Null) if value.is_object() => Ok(None),
            Some(Value::Bool(supported)) => Ok(Some(*supported)),
            _ => Err(DiscoveryError::InvalidResponse),
        },
    }
}

/// Validate a persisted safe projection without reading credentials or trusting new fields.
pub(crate) fn sanitize_snapshot(
    models: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, DiscoveryError> {
    if models.len() > discovery::DISCOVERY_ROWS_CAP {
        return Err(DiscoveryError::InvalidResponse);
    }
    models
        .iter()
        .map(|(id, config)| {
            let mut parsed = model(&json!({"id":id,"opencode":config}), false)?;
            if let Some(partial) = config.get("partial_capabilities") {
                let partial = partial.as_object().ok_or(DiscoveryError::InvalidResponse)?;
                let mut out = Map::new();
                for key in ["image_input", "pdf_input", "reasoning"] {
                    if let Some(value) = partial.get(key).filter(|v| !v.is_null()) {
                        if !value.is_boolean() {
                            return Err(DiscoveryError::InvalidResponse);
                        }
                        out.insert(key.into(), value.clone());
                    }
                }
                parsed.config["partial_capabilities"] = Value::Object(out);
            }
            Ok((id.clone(), parsed.config))
        })
        .collect()
}

fn headers(key: Option<&str>, anthropic: bool) -> Result<HeaderMap, DiscoveryError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static(crate::USER_AGENT));
    if anthropic {
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    }
    if let Some(key) = key {
        let value = if anthropic {
            key.into()
        } else {
            format!("Bearer {key}")
        };
        let mut value = HeaderValue::from_str(&value).map_err(|_| DiscoveryError::InvalidConfig)?;
        value.set_sensitive(true);
        headers.insert(
            if anthropic {
                "x-api-key"
            } else {
                AUTHORIZATION.as_str()
            },
            value,
        );
    }
    Ok(headers)
}

pub(crate) async fn probe(
    input: &CustomProviderInput,
    protocol: Option<NativeProtocol>,
) -> Result<CustomCatalog, DiscoveryError> {
    crate::storage::provider_connections::admit_input(input)
        .map_err(|_| DiscoveryError::InvalidConfig)?;
    let mut provider = crate::provider::ResponsesConfig {
        base_url: input.base_url.clone(),
        api_key: String::new(),
        headers: BTreeMap::new(),
        timeout: None,
        chunk_timeout_ms: crate::provider::CHUNK_TIMEOUT_MS,
        connect_timeout: Duration::from_secs(10),
        allow_private: false,
        set_cache_key: false,
        wire: Default::default(),
    };
    provider.wire.endpoint = Some(
        crate::endpoint::EndpointBinding::admit(&input.base_url, true, "custom connection")
            .map_err(|_| DiscoveryError::InvalidConfig)?,
    );
    let key = match &input.auth {
        CustomProviderAuth::Key(key) => Some(key.clone().into_secret()),
        CustomProviderAuth::Anonymous => None,
    };
    let client = discovery::ReqwestDiscoveryClient::captured(&provider)?;
    discover(
        &discovery::RealClock,
        &client,
        &input.base_url,
        key.as_deref(),
        protocol,
        &AtomicBool::new(false),
    )
    .await
}

pub(crate) async fn refresh_provider(
    provider: crate::provider::ResponsesConfig,
) -> Result<CustomCatalog, DiscoveryError> {
    if !provider.auth_ready() {
        return Err(DiscoveryError::InvalidConfig);
    }
    let protocol = match provider.wire.protocol {
        crate::provider::protocol::Protocol::Responses => NativeProtocol::Responses,
        crate::provider::protocol::Protocol::Chat => NativeProtocol::Chat,
        crate::provider::protocol::Protocol::Messages => NativeProtocol::Messages,
    };
    let client = discovery::ReqwestDiscoveryClient::captured(&provider)?;
    discover(
        &discovery::RealClock,
        &client,
        &provider.base_url,
        (provider.wire.auth_policy == crate::auth::AuthPolicy::Key)
            .then_some(provider.api_key.as_str()),
        Some(protocol),
        &AtomicBool::new(false),
    )
    .await
}

pub(crate) async fn discover(
    clock: &impl Clock,
    client: &impl DiscoveryClient,
    base: &str,
    key: Option<&str>,
    protocol: Option<NativeProtocol>,
    cancel: &AtomicBool,
) -> Result<CustomCatalog, DiscoveryError> {
    let base = discovery::discovery_url(base)?;
    let deadline = clock.now_ms().saturating_add(discovery::TOTAL_TIMEOUT_MS);
    let initial_anthropic = protocol == Some(NativeProtocol::Messages);
    let mut auth_anthropic = initial_anthropic;
    let mut tried_alternate = false;
    let mut cumulative = 0usize;
    let mut models = BTreeMap::new();
    let mut cursors = BTreeSet::new();
    let mut cursor: Option<String> = None;
    let mut family = None;
    loop {
        let mut url = reqwest::Url::parse(&base).map_err(|_| DiscoveryError::InvalidConfig)?;
        if auth_anthropic || cursor.is_some() {
            url.query_pairs_mut().append_pair("limit", "1000");
        }
        if let Some(cursor) = &cursor {
            url.query_pairs_mut().append_pair("after_id", cursor);
        }
        let headers = headers(key, auth_anthropic)?;
        let mut attempt = 0;
        let value = loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(DiscoveryError::Cancelled);
            }
            let remaining = discovery::remaining_budget(deadline, clock.now_ms());
            if remaining == 0 {
                return Err(DiscoveryError::Network);
            }
            let mut body = Vec::new();
            let response = client
                .get_chunks(
                    url.as_str(),
                    &headers,
                    Duration::from_millis(remaining.min(discovery::ATTEMPT_TIMEOUT_MS)),
                    |chunk| {
                        cumulative = cumulative
                            .checked_add(chunk.len())
                            .filter(|n| *n <= discovery::DISCOVERY_BODY_CAP)
                            .ok_or(DiscoveryError::InvalidResponse)?;
                        discovery::append_discovery_body(&mut body, chunk)
                    },
                )
                .await;
            if clock.now_ms() >= deadline {
                return Err(DiscoveryError::Network);
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(DiscoveryError::Cancelled);
            }
            let value = match response {
                Ok(status) if (200..300).contains(&status) => {
                    serde_json::from_slice::<Value>(&body)
                        .map_err(|_| DiscoveryError::InvalidResponse)
                }
                Ok(status) => Err(DiscoveryError::Http { status }),
                Err(error) => Err(error),
            };
            match value {
                Ok(value) => break Some(value),
                Err(DiscoveryError::Http {
                    status: 400 | 401 | 403,
                }) if key.is_some() && !tried_alternate && cursor.is_none() => {
                    tried_alternate = true;
                    auth_anthropic = !initial_anthropic;
                    break None;
                }
                Err(error) => {
                    let transient = matches!(error, DiscoveryError::Network)
                        || matches!(error, DiscoveryError::Http { status } if discovery::should_retry_status(status));
                    if !transient || attempt >= discovery::RETRY_DELAYS_MS.len() {
                        return Err(error);
                    }
                    let delay = discovery::RETRY_DELAYS_MS[attempt]
                        .min(discovery::remaining_budget(deadline, clock.now_ms()));
                    attempt += 1;
                    clock.sleep_ms(delay).await;
                }
            }
        };
        let Some(value) = value else {
            continue;
        };
        let rows = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or(DiscoveryError::InvalidResponse)?;
        let anthropic = value.get("has_more").is_some() && value.get("object").is_none();
        if !anthropic && value.get("object").and_then(Value::as_str) != Some("list") {
            return Err(DiscoveryError::InvalidResponse);
        }
        if family.is_some_and(|old| old != anthropic) {
            return Err(DiscoveryError::InvalidResponse);
        }
        family = Some(anthropic);
        for row in rows {
            if models.len() >= discovery::DISCOVERY_ROWS_CAP {
                return Err(DiscoveryError::InvalidResponse);
            }
            let model = model(row, anthropic)?;
            if models.insert(model.id, model.config).is_some() {
                return Err(DiscoveryError::InvalidResponse);
            }
        }
        if !anthropic || value.get("has_more").and_then(Value::as_bool) == Some(false) {
            let encoded =
                serde_json::to_string(&models).map_err(|_| DiscoveryError::InvalidResponse)?;
            if encoded.len() > discovery::DISCOVERY_BODY_CAP
                || key.is_some_and(|key| encoded.contains(key))
            {
                return Err(DiscoveryError::InvalidResponse);
            }
            return Ok(CustomCatalog {
                models,
                suggested: anthropic.then_some(NativeProtocol::Messages),
            });
        }
        if value.get("has_more").and_then(Value::as_bool) != Some(true) || rows.is_empty() {
            return Err(DiscoveryError::InvalidResponse);
        }
        let next = text(
            value
                .get("last_id")
                .ok_or(DiscoveryError::InvalidResponse)?,
            4096,
        )?;
        if rows
            .last()
            .and_then(|row| row.get("id"))
            .and_then(Value::as_str)
            != Some(next)
            || !cursors.insert(next.to_owned())
        {
            return Err(DiscoveryError::InvalidResponse);
        }
        cursor = Some(next.into());
    }
}
