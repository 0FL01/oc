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

fn overlays(raw: &mut Map<String, Value>, field: &str) -> Result<(), ConfigError> {
    let mut settings = match raw.remove("settings") {
        Some(value) => object(&value, field)?,
        None => Map::new(),
    };
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

fn model(value: &Value, field: &str) -> Result<Value, ConfigError> {
    let mut raw = object(value, field)?;
    overlays(&mut raw, field)?;
    if let Some(variants) = raw.remove("variants") {
        let variants = variants.as_array().cloned().ok_or_else(|| invalid(field))?;
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
            overlays(&mut variant, field)?;
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

fn provider(value: &Value, field: &str) -> Result<Value, ConfigError> {
    let mut raw = object(value, field)?;
    overlays(&mut raw, field)?;
    if let Some(models) = raw.get_mut("models") {
        let mut out = Map::new();
        for (id, value) in object(models, field)? {
            out.insert(id.clone(), model(&value, &format!("{field}.models.{id}"))?);
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

/// Canonical `providers` only; other root keys are refused by the T66 allowlist.
pub(super) fn document(obj: &Map<String, Value>) -> Result<BTreeMap<String, Value>, ConfigError> {
    let mut out = BTreeMap::new();
    if let Some(value) = obj.get("providers") {
        for (id, value) in object(value, "providers")? {
            let value = provider(&value, &format!("providers.{id}"))?;
            out.insert(id, value);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
