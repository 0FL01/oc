//! Pinned subscription-only catalog overlay, not general provider/reasoning routing.
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub(crate) fn transform(models: &mut BTreeMap<String, Value>, subscription: bool) {
    if !subscription {
        return;
    }
    models.retain(|id, model| {
        if model
            .pointer("/body/reasoning/mode")
            .and_then(Value::as_str)
            == Some("pro")
        {
            return false;
        }
        let api = model.get("modelID").and_then(Value::as_str).unwrap_or(id);
        if !eligible(api) {
            return false;
        }
        model["cost"] = json!([]);
        if !model.get("limit").is_some_and(Value::is_object) {
            model["limit"] = json!({});
        }
        model["limit"]["context"] = json!(400_000);
        model["limit"]["input"] = json!(272_000);
        true
    });
}

fn eligible(api: &str) -> bool {
    // Exactly the donor's explicit allow/deny precedence and numeric prefix regex.
    if matches!(api, "gpt-5.5" | "gpt-5.3-codex-spark") {
        return true;
    }
    if matches!(api, "gpt-5.5-pro" | "gpt-5.6") {
        return false;
    }
    let Some(tail) = api.strip_prefix("gpt-") else {
        return false;
    };
    let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return false;
    }
    let major = tail[..digits].trim_start_matches('0');
    if greater(major, b'5') {
        return true;
    }
    if major != "5" {
        return false;
    }
    let Some(minor) = tail[digits..].strip_prefix('.') else {
        return false;
    };
    let digits = minor.bytes().take_while(u8::is_ascii_digit).count();
    greater(minor[..digits].trim_start_matches('0'), b'4')
}

fn greater(digits: &str, limit: u8) -> bool {
    digits.len() > 1 || digits.as_bytes().first().is_some_and(|n| *n > limit)
}

#[cfg(test)]
mod tests;
