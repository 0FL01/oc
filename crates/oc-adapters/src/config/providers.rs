//! Local provider/model normalization. Remote discovery never uses this owner.
use super::ConfigError;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

#[path = "request_bindings.rs"]
mod bindings;
pub(super) use bindings::{option_origin_key, record_origins, request_bindings};

fn invalid(field: &str) -> ConfigError {
    ConfigError::Invalid {
        field: field.into(),
        reason: "invalid or conflicting provider overlay".into(),
    }
}

fn object(value: &Value, field: &str) -> Result<Map<String, Value>, ConfigError> {
    value.as_object().cloned().ok_or_else(|| invalid(field))
}

/// Recursive objects and case-insensitive headers; arrays/scalars replace.
pub(super) fn merge(target: &mut Value, overlay: Value) {
    let (Some(target), Value::Object(overlay)) = (target.as_object_mut(), &overlay) else {
        *target = overlay;
        return;
    };
    for (key, value) in overlay {
        if key == "headers"
            && let Some(headers) = value.as_object()
        {
            let existing = target.entry(key.clone()).or_insert_with(|| json!({}));
            if let Some(existing) = existing.as_object_mut() {
                for (name, value) in headers {
                    existing.retain(|old, _| !old.eq_ignore_ascii_case(name));
                    existing.insert(name.clone(), value.clone());
                }
                continue;
            }
        }
        // Variants merge by exact ID, preserving first declaration order.
        if key == "variants"
            && value.is_array()
            && let Some(existing) = target.get_mut(key).and_then(Value::as_array_mut)
        {
            for variant in value.as_array().expect("checked") {
                if let Some(old) = existing
                    .iter_mut()
                    .find(|old| old.get("id") == variant.get("id"))
                {
                    merge(old, variant.clone());
                } else {
                    existing.push(variant.clone());
                }
            }
            continue;
        }
        match target.get_mut(key) {
            Some(old) => merge(old, value.clone()),
            None => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn overlays(raw: &mut Map<String, Value>, legacy: bool, field: &str) -> Result<(), ConfigError> {
    let name = if legacy { "options" } else { "settings" };
    let mut settings = match raw.remove(name) {
        Some(value) => object(&value, field)?,
        None => Map::new(),
    };
    if !legacy
        && settings
            .keys()
            .any(|key| !super::PROVIDER_OPTION_KEYS.contains(&key.as_str()) && key != "extraBody")
    {
        return Err(invalid(field));
    }
    for (from, to) in [
        ("headers", "headers"),
        ("body", "body"),
        ("extraBody", "body"),
    ] {
        if let Some(value) = settings.remove(from) {
            object(&value, field)?;
            let target = raw.entry(to).or_insert_with(|| json!({}));
            merge(target, value);
        }
    }
    for key in ["headers", "body"] {
        if let Some(value) = raw.get(key) {
            object(value, field)?;
        }
    }
    if let Some(body) = raw.get("body") {
        let overlay = crate::provider::RequestOverlay {
            headers: Default::default(),
            body: object(body, field)?,
        };
        overlay.validate().map_err(|_| invalid(field))?;
    }
    if !settings.is_empty() {
        raw.insert("settings".into(), Value::Object(settings));
    }
    Ok(())
}

fn model(value: &Value, legacy: bool, field: &str) -> Result<Value, ConfigError> {
    let mut raw = object(value, field)?;
    overlays(&mut raw, legacy, field)?;
    if legacy {
        if let Some(id) = raw.remove("id") {
            raw.insert("modelID".into(), id);
        }
        if let Some(provider) = raw.remove("provider") {
            let mut provider = object(&provider, field)?;
            if let Some(package) = provider.remove("npm") {
                raw.insert("package".into(), package);
            }
            if let Some(api) = provider.remove("api") {
                raw.entry("settings").or_insert_with(|| json!({}))["baseURL"] = api;
            }
        }
        let tools = raw.get("tool_call").cloned();
        let modalities = raw.get("modalities").cloned();
        if tools.is_some() || modalities.is_some() {
            let mut capabilities = Map::new();
            if let Some(tools) = tools {
                capabilities.insert("tools".into(), tools);
            }
            if let Some(modalities) = modalities {
                for (key, value) in object(&modalities, field)? {
                    capabilities.insert(key, value);
                }
            }
            raw.insert("capabilities".into(), Value::Object(capabilities));
        }
        if let Some(interleaved) = raw.get("interleaved") {
            let reasoning = interleaved
                .as_str()
                .or_else(|| interleaved.get("field").and_then(Value::as_str));
            if let Some(reasoning) = reasoning {
                let reasoning = reasoning.to_owned();
                raw.entry("compatibility").or_insert_with(|| json!({}))["reasoningField"] =
                    json!(reasoning);
            }
        }
        if raw.get("status").and_then(Value::as_str) == Some("deprecated") {
            raw.insert("disabled".into(), json!(true));
        }
    }
    if let Some(variants) = raw.remove("variants") {
        let variants = if let Some(map) = variants.as_object() {
            let mut out = Vec::new();
            for (id, value) in map {
                let mut variant = object(value, field)?;
                let disabled = variant.remove("disabled");
                let mut normalized = json!({"id":id, "settings":variant});
                if let Some(disabled) = disabled {
                    normalized["disabled"] = disabled;
                }
                let mut normalized = object(&normalized, field)?;
                overlays(&mut normalized, false, field)?;
                out.push(Value::Object(normalized));
            }
            out
        } else {
            variants.as_array().cloned().ok_or_else(|| invalid(field))?
        };
        let mut ids = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for variant in variants {
            let mut variant = object(&variant, field)?;
            let id = variant
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| invalid(field))?;
            if !ids.insert(id.to_owned()) {
                return Err(invalid(field));
            }
            overlays(&mut variant, false, field)?;
            out.push(Value::Object(variant));
        }
        raw.insert("variants".into(), Value::Array(out));
    }
    // Keep the existing native metadata readers interoperable during migration.
    if let Some(capabilities) = raw.get("capabilities").cloned() {
        if let Some(tools) = capabilities.get("tools") {
            raw.insert("tool_call".into(), tools.clone());
        }
        let mut modalities = Map::new();
        for key in ["input", "output"] {
            if let Some(value) = capabilities.get(key) {
                modalities.insert(key.into(), value.clone());
            }
        }
        if !modalities.is_empty() {
            raw.insert("modalities".into(), Value::Object(modalities));
        }
    }
    Ok(Value::Object(raw))
}

fn provider(value: &Value, legacy: bool, field: &str) -> Result<Value, ConfigError> {
    let mut raw = object(value, field)?;
    overlays(&mut raw, legacy, field)?;
    if legacy {
        if let Some(package) = raw.remove("npm") {
            raw.insert("package".into(), package);
        }
        if let Some(api) = raw.remove("api") {
            raw.entry("settings").or_insert_with(|| json!({}))["baseURL"] = api;
        }
    }
    if let Some(models) = raw.get_mut("models") {
        let mut out = Map::new();
        for (id, value) in object(models, field)? {
            out.insert(
                id.clone(),
                model(&value, legacy, &format!("{field}.models.{id}"))?,
            );
        }
        *models = Value::Object(out);
    }
    // The existing internal config shape remains the native public contract.
    if let Some(package) = raw.remove("package") {
        raw.insert("npm".into(), package);
    }
    let mut settings = raw.remove("settings").unwrap_or_else(|| json!({}));
    for key in ["headers", "body"] {
        if let Some(value) = raw.remove(key) {
            settings[key] = value;
        }
    }
    if !settings.as_object().is_some_and(Map::is_empty) {
        raw.insert("options".into(), settings);
    }
    Ok(Value::Object(raw))
}

/// Both roots may coexist only when their normalized overlapping fields agree.
pub(super) fn document(obj: &Map<String, Value>) -> Result<BTreeMap<String, Value>, ConfigError> {
    let mut out = BTreeMap::new();
    for (root, legacy) in [("provider", true), ("providers", false)] {
        if let Some(value) = obj.get(root) {
            for (id, value) in object(value, root)? {
                let value = provider(&value, legacy, &format!("{root}.{id}"))?;
                if let Some(previous) = out.get_mut(&id) {
                    fn conflicts(a: &Value, b: &Value) -> bool {
                        match (a.as_object(), b.as_object()) {
                            (Some(a), Some(b)) => b.iter().any(|(key, value)| {
                                a.get(key).is_some_and(|old| conflicts(old, value))
                            }),
                            _ => a != b,
                        }
                    }
                    if conflicts(previous, &value) {
                        return Err(invalid(&format!("providers.{id}")));
                    }
                    merge(previous, value);
                } else {
                    out.insert(id, value);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
