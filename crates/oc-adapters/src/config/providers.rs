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
    raw.retain(|key, _| super::admission::MODEL.contains(&key.as_str()));
    for (key, allowed) in [
        ("capabilities", super::admission::CAPABILITIES),
        ("limit", super::admission::LIMIT),
    ] {
        if let Some(value) = raw.get_mut(key).and_then(Value::as_object_mut) {
            value.retain(|key, _| allowed.contains(&key.as_str()));
        }
    }
    overlays(&mut raw, field)?;
    if let Some(variants) = raw.remove("variants") {
        let variants = variants.as_array().cloned().ok_or_else(|| invalid(field))?;
        let mut ids = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for variant in variants {
            let mut variant = object(&variant, field)?;
            variant.retain(|key, _| super::admission::VARIANT.contains(&key.as_str()));
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
    raw.retain(|key, _| super::admission::PROVIDER.contains(&key.as_str()));
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
    if let Some(limits) = settings
        .get_mut("nativeFallbackLimits")
        .and_then(Value::as_object_mut)
    {
        limits.retain(|key, _| matches!(key.as_str(), "context" | "output"));
    }
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

// The deliberately supported v1 subset is normalized before ordinary native
// shape/overlay admission. Native entries replace legacy entries within a source.
fn legacy_options(
    raw: &mut Map<String, Value>,
    value: &Value,
    compatible: bool,
    field: &str,
) -> Result<(), ConfigError> {
    let options = object(value, field)?;
    if raw.contains_key("settings") {
        return Ok(());
    }
    let mut settings = Map::new();
    let mut body = Map::new();
    for (key, value) in options {
        if compatible
            && !matches!(
                key.as_str(),
                "apiKey"
                    | "baseURL"
                    | "headers"
                    | "body"
                    | "extraBody"
                    | "timeout"
                    | "chunkTimeout"
                    | "headerTimeout"
                    | "disabled"
                    | "reasoningEffort"
                    | "textVerbosity"
                    | "user"
                    | "strictJsonSchema"
            )
        {
            body.insert(key, value);
        } else {
            settings.insert(key, value);
        }
    }
    raw.entry("settings").or_insert(Value::Object(settings));
    if !body.is_empty() {
        raw.entry("body").or_insert(Value::Object(body));
    }
    Ok(())
}

fn legacy_model(value: &Value, package: Option<&str>, field: &str) -> Result<Value, ConfigError> {
    let mut raw = object(value, field)?;
    let connection = raw
        .remove("provider")
        .map(|value| object(&value, field))
        .transpose()?
        .unwrap_or_default();
    let compatible = raw
        .get("package")
        .and_then(Value::as_str)
        .or_else(|| connection.get("npm").and_then(Value::as_str))
        .or(package)
        == Some("@ai-sdk/openai-compatible");
    if let Some(options) = raw.remove("options") {
        legacy_options(&mut raw, &options, compatible, field)?;
    }
    if let Some(id) = raw.remove("id") {
        raw.entry("modelID").or_insert(id);
    }
    if let Some(package) = connection.get("npm") {
        raw.entry("package").or_insert_with(|| package.clone());
    }
    // Native settings win; among legacy inputs provider.api overrides options.baseURL.
    if let Some(api) = connection.get("api")
        && value
            .get("settings")
            .and_then(|settings| settings.get("baseURL"))
            .is_none()
    {
        let settings = raw.entry("settings").or_insert_with(|| json!({}));
        settings
            .as_object_mut()
            .ok_or_else(|| invalid(field))?
            .insert("baseURL".into(), api.clone());
    }
    if !raw.contains_key("capabilities")
        && (raw.contains_key("tool_call") || raw.contains_key("modalities"))
    {
        let mut caps = raw
            .get("modalities")
            .map(|v| object(v, field))
            .transpose()?
            .unwrap_or_default();
        if let Some(tools) = raw.get("tool_call") {
            caps.insert("tools".into(), tools.clone());
        }
        raw.insert("capabilities".into(), Value::Object(caps));
    }
    if raw.get("status").and_then(Value::as_str) == Some("deprecated") {
        raw.entry("disabled").or_insert(json!(true));
    }
    if let Some(variants) = raw.get_mut("variants")
        && variants.is_object()
    {
        let mut out = Vec::new();
        for (id, options) in object(variants, field)? {
            let mut variant = Map::from_iter([("id".into(), Value::String(id))]);
            legacy_options(&mut variant, &options, compatible, field)?;
            out.push(Value::Object(variant));
        }
        *variants = Value::Array(out);
    }
    Ok(Value::Object(raw))
}

/// Native entries take whole-entry precedence over the supported legacy map.
pub(super) fn document(obj: &Map<String, Value>) -> Result<BTreeMap<String, Value>, ConfigError> {
    let mut out = BTreeMap::new();
    if let Some(value) = obj.get("provider") {
        for (id, value) in object(value, "provider")? {
            let field = format!("provider.{id}");
            let mut raw = object(&value, &field)?;
            if let Some(package) = raw.remove("npm") {
                raw.entry("package").or_insert(package);
            }
            if let Some(options) = raw.remove("options") {
                legacy_options(&mut raw, &options, false, &field)?;
            }
            let package = raw
                .get("package")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(models) = raw.get_mut("models") {
                let mut out_models = Map::new();
                for (model, value) in object(models, &field)? {
                    out_models.insert(
                        model.clone(),
                        legacy_model(
                            &value,
                            package.as_deref(),
                            &format!("{field}.models.{model}"),
                        )?,
                    );
                }
                *models = Value::Object(out_models);
            }
            out.insert(id, provider(&Value::Object(raw), &field)?);
        }
    }
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
