//! Local, immutable request templates. Secrets resolve only after endpoint admission.
use super::super::{ConfigError, ProviderEntry, Source, substitute_with, validate_provider};
use super::{invalid, merge};
use crate::provider::ResponsesConfig;
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

pub(in crate::config) fn option_origin_key(id: &str, field: &str) -> String {
    origin_key(id, None, None, field)
}

fn origin_key(id: &str, model: Option<&str>, variant: Option<&str>, field: &str) -> String {
    format!("request-origin:{}", json!([id, model, variant, field]))
}

pub(in crate::config) fn record_origins(
    id: &str,
    value: &Value,
    source: &str,
    out: &mut BTreeMap<String, String>,
) {
    // Structured tuples keep literal provider/model/variant IDs (including '/'
    // and '.') from aliasing another connection's source/trust provenance.
    let mut record = |model, variant, settings: &Value, headers: Option<&Value>| {
        if let Some(map) = settings.as_object() {
            for key in map.keys().filter(|key| key.as_str() != "headers") {
                out.insert(origin_key(id, model, variant, key), source.to_owned());
            }
        }
        if let Some(headers) = headers.and_then(Value::as_object) {
            for name in headers.keys() {
                out.insert(
                    origin_key(
                        id,
                        model,
                        variant,
                        &format!("headers.{}", name.to_ascii_lowercase()),
                    ),
                    source.to_owned(),
                );
            }
        }
    };
    if let Some(options) = value.get("options") {
        record(None, None, options, options.get("headers"));
    }
    if let Some(models) = value.get("models").and_then(Value::as_object) {
        for (model, value) in models {
            record(
                Some(model.as_str()),
                None,
                value.get("settings").unwrap_or(&Value::Null),
                value.get("headers"),
            );
            if let Some(variants) = value.get("variants").and_then(Value::as_array) {
                for variant in variants {
                    if let Some(name) = variant.get("id").and_then(Value::as_str) {
                        record(
                            Some(model.as_str()),
                            Some(name),
                            variant.get("settings").unwrap_or(&Value::Null),
                            variant.get("headers"),
                        );
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::config) fn request_bindings(
    id: &str,
    raw: &Value,
    fallback_source: &str,
    origins: &BTreeMap<String, String>,
    sources: &[Source],
    env: &BTreeMap<String, String>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
) -> Result<BTreeMap<(String, Option<String>), ResponsesConfig>, ConfigError> {
    let mut requests = BTreeMap::new();
    let Some(models) = raw.get("models").and_then(Value::as_object) else {
        return Ok(requests);
    };
    for (model_id, model) in models {
        if model.get("disabled").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let mut variants = vec![(None, None)];
        if let Some(array) = model.get("variants").and_then(Value::as_array) {
            for variant in array {
                if variant.get("disabled").and_then(Value::as_bool) != Some(true) {
                    variants.push((variant.get("id").and_then(Value::as_str), Some(variant)));
                }
            }
        }
        for (variant_id, variant) in variants {
            let mut options = raw.get("options").cloned().unwrap_or_else(|| json!({}));
            let mut field_origins = BTreeMap::new();
            let mut overlay = |value: &Value, variant: Option<&str>| {
                let mut settings = value.get("settings").cloned().unwrap_or_else(|| json!({}));
                for key in ["headers", "body"] {
                    if let Some(value) = value.get(key) {
                        settings[key] = value.clone();
                    }
                }
                if let Some(map) = settings.as_object() {
                    for (key, value) in map {
                        if key == "headers" {
                            if let Some(headers) = value.as_object() {
                                for name in headers.keys() {
                                    let field = format!("headers.{}", name.to_ascii_lowercase());
                                    field_origins.insert(
                                        field,
                                        origin_key(
                                            id,
                                            Some(model_id),
                                            variant,
                                            &format!("headers.{}", name.to_ascii_lowercase()),
                                        ),
                                    );
                                }
                            }
                        } else {
                            field_origins
                                .insert(key.clone(), origin_key(id, Some(model_id), variant, key));
                        }
                    }
                }
                merge(&mut options, settings);
            };
            let model_prefix = format!("provider.{id}.models.{model_id}");
            overlay(model, None);
            if let (Some(variant), Some(variant_id)) = (variant, variant_id) {
                overlay(variant, Some(variant_id));
            }
            let origin = |field: &str| {
                let key = field_origins
                    .get(field)
                    .cloned()
                    .unwrap_or_else(|| option_origin_key(id, field));
                let source = origins
                    .get(&key)
                    .map(String::as_str)
                    .unwrap_or(fallback_source);
                (
                    source,
                    sources.iter().any(|s| s.path == source && s.trusted),
                )
            };
            let mut entry: ProviderEntry = serde_json::from_value(json!({
                "npm": model.get("package").or_else(|| raw.get("npm")),
                "options": options,
                "models": {model_id: model},
            }))
            .map_err(|_| invalid(&model_prefix))?;
            validate_provider(id, &entry).map_err(|_| invalid(&model_prefix))?;
            let (source, trusted) = origin("baseURL");
            entry.options.base_url =
                substitute_with(&entry.options.base_url, source, trusted, env, reader)?;
            if entry.options.base_url.is_empty() {
                continue;
            }
            // Never consult/read any credential input before admitting the overlay endpoint.
            crate::auth::AuthScope::admit(id, &entry.options.base_url)
                .map_err(|_| invalid(&model_prefix))?;
            crate::endpoint::EndpointBinding::admit(&entry.options.base_url, trusted, source)
                .map_err(|_| invalid(&model_prefix))?;
            entry.options.endpoint_source = Some(source.to_owned());
            entry.options.endpoint_trusted = trusted;
            let (source, trusted) = origin("apiKey");
            entry.options.api_key =
                substitute_with(&entry.options.api_key, source, trusted, env, reader)?;
            if let Some(token) = &mut entry.options.auth_token {
                let (source, trusted) = origin("authToken");
                *token = substitute_with(token, source, trusted, env, reader)?;
            }
            for (name, value) in &mut entry.options.headers {
                let (source, trusted) = origin(&format!("headers.{}", name.to_ascii_lowercase()));
                *value = substitute_with(value, source, trusted, env, reader)?;
            }
            validate_provider(id, &entry).map_err(|_| invalid(&model_prefix))?;
            if let Some(token) = entry.options.auth_token.take() {
                entry.options.api_key = token;
                entry.options.messages_bearer = true;
            }
            let api_model = match model.get("modelID") {
                None => model_id.as_str(),
                Some(Value::String(id))
                    if !id.trim().is_empty() && !id.chars().any(char::is_control) =>
                {
                    id
                }
                _ => return Err(invalid(&model_prefix)),
            };
            let mut wire =
                super::super::provider_wire(id, &entry).map_err(|_| invalid(&model_prefix))?;
            wire.api_model = Some(api_model.to_owned());
            let request = ResponsesConfig {
                base_url: entry.options.base_url,
                api_key: entry.options.api_key,
                timeout: entry
                    .options
                    .timeout
                    .and_then(crate::config::ProviderTimeout::legacy_flag),
                chunk_timeout_ms: entry
                    .options
                    .chunk_timeout
                    .unwrap_or(crate::provider::CHUNK_TIMEOUT_MS),
                connect_timeout: Duration::from_secs(10),
                allow_private: false,
                headers: entry.options.headers,
                set_cache_key: entry.options.set_cache_key.unwrap_or(false),
                wire,
            };
            let mut shape = request.clone();
            if shape.wire.auth_policy == crate::auth::AuthPolicy::OAuth {
                shape.wire.auth_policy = crate::auth::AuthPolicy::Key;
            }
            crate::provider::request_headers(&shape).map_err(|_| invalid(&model_prefix))?;
            requests.insert((model_id.clone(), variant_id.map(str::to_owned)), request);
        }
    }
    Ok(requests)
}

#[cfg(test)]
#[path = "request_bindings_tests.rs"]
mod tests;
