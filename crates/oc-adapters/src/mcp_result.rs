//! Shared native MCP projection into the current textual tool-output surface.
//! Never copy arbitrary server error messages into diagnostics.

use serde_json::Value;

/// Maximum projected initialize instructions per server.
pub const INSTRUCTIONS_BYTES_CAP: usize = 8 * 1024;

/// Safe, useful error categories. No variant can carry remote payload text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FailureDetail {
    /// Request arguments need correction.
    #[error("invalid arguments; check the tool schema and required parameters")]
    InvalidArguments,
    /// The requested item does not exist.
    #[error("not found; check the requested resource or tool")]
    NotFound,
    /// The remote service throttled the request.
    #[error("rate limited; wait before retrying")]
    RateLimited,
    /// Authentication/authorization was refused.
    #[error("access denied; check server credentials and permissions")]
    AccessDenied,
    /// The remote operation timed out.
    #[error("server timeout; retry only if the operation is safe to repeat")]
    Timeout,
}

pub(crate) enum ResultError {
    Failed(Option<FailureDetail>),
    Unsupported,
    BadResult,
}

/// Replacement runs before bounding, so a secret straddling the boundary cannot
/// leave a visible prefix. This is literal configured-value redaction, not a
/// guarantee against an adversarial server encoding a secret in ordinary data.
pub(crate) fn redact(text: &str, secrets: &[String]) -> String {
    let mut secrets: Vec<&str> = secrets
        .iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    secrets.sort_unstable_by_key(|secret| std::cmp::Reverse(secret.len()));
    secrets.dedup();
    let mut text = text.to_string();
    for secret in secrets {
        text = text.replace(secret, "[redacted]");
    }
    text.retain(|c| !c.is_control() || matches!(c, '\n' | '\t'));
    text
}

pub(crate) fn instructions(text: Option<&str>, secrets: &[String]) -> Option<String> {
    let text = redact(text?, secrets);
    if text.trim().is_empty() {
        return None;
    }
    if text.len() <= INSTRUCTIONS_BYTES_CAP {
        return Some(text);
    }
    let marker = "\n[MCP instructions truncated]";
    let end = text.floor_char_boundary(INSTRUCTIONS_BYTES_CAP - marker.len());
    Some(format!("{}{marker}", &text[..end]))
}

/// Native optional convention, not a universal MCP error schema: exact string
/// code at /structuredContent/error/code, or /structuredContent/code when the
/// nested code is absent. A present unknown/null nested code stays opaque.
/// Only these five uppercase values are recognized; never inspect prose.
fn failure_detail(result: &Value) -> Option<FailureDetail> {
    let code = result
        .pointer("/structuredContent/error/code")
        .or_else(|| result.pointer("/structuredContent/code"))?
        .as_str()?;
    match code {
        "INVALID_ARGUMENTS" => Some(FailureDetail::InvalidArguments),
        "NOT_FOUND" => Some(FailureDetail::NotFound),
        "RATE_LIMITED" => Some(FailureDetail::RateLimited),
        "ACCESS_DENIED" => Some(FailureDetail::AccessDenied),
        "TIMEOUT" => Some(FailureDetail::Timeout),
        _ => None,
    }
}

pub(crate) fn rpc_failure(error: &rmcp::service::ServiceError) -> Option<FailureDetail> {
    let rmcp::service::ServiceError::McpError(error) = error else {
        return None;
    };
    match error.code.0 {
        -32602 => Some(FailureDetail::InvalidArguments),
        -32601 => Some(FailureDetail::NotFound),
        _ => None,
    }
}

fn redact_json(value: &mut Value, secrets: &[String]) {
    match value {
        Value::String(text) => *text = redact(text, secrets),
        Value::Array(values) => values.iter_mut().for_each(|v| redact_json(v, secrets)),
        Value::Object(map) => {
            let old = std::mem::take(map);
            for (key, mut value) in old {
                let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
                if [
                    "authorization",
                    "apikey",
                    "token",
                    "accesstoken",
                    "refreshtoken",
                    "password",
                    "secret",
                    "headers",
                    "env",
                    "environment",
                ]
                .contains(&normalized.as_str())
                {
                    value = Value::String("[redacted]".into());
                } else {
                    redact_json(&mut value, secrets);
                }
                map.insert(redact(&key, secrets), value);
            }
        }
        _ => {}
    }
}

pub(crate) fn project(
    result: rmcp::model::CallToolResult,
    secrets: &[String],
) -> Result<String, ResultError> {
    let value = serde_json::to_value(result).map_err(|_| ResultError::BadResult)?;
    // Cap the complete serialized result, including structured JSON and every
    // content block, before formatting/redaction allocates additional copies.
    if serde_json::to_vec(&value)
        .map_err(|_| ResultError::BadResult)?
        .len()
        > crate::mcp_remote::RESULT_TEXT_BYTES_CAP
    {
        return Err(ResultError::BadResult);
    }
    if value["isError"] == true {
        return Err(ResultError::Failed(failure_detail(&value)));
    }
    let mut parts = Vec::new();
    for block in value["content"].as_array().ok_or(ResultError::BadResult)? {
        match block["type"].as_str() {
            Some("text") => {
                let text = block["text"].as_str().ok_or(ResultError::BadResult)?;
                if !text.trim().is_empty() {
                    parts.push(redact(text, secrets));
                }
            }
            Some("resource") if block["resource"]["text"].is_string() => {
                let mut resource = block["resource"].clone();
                redact_json(&mut resource, secrets);
                parts.push(serde_json::json!({"resource": resource}).to_string());
            }
            Some("resource_link") => {
                let mut link = block.clone();
                redact_json(&mut link, secrets);
                parts.push(link.to_string());
            }
            // FunctionCallOutput is text-only today. Do not pretend base64 is
            // visible image/audio data or silently discard part of a result.
            _ => return Err(ResultError::Unsupported),
        }
    }
    if let Some(structured) = value.get("structuredContent") {
        let mut structured = structured.clone();
        redact_json(&mut structured, secrets);
        parts.push(serde_json::json!({"structuredContent": structured}).to_string());
    }
    if parts.is_empty() {
        return Err(ResultError::BadResult);
    }
    let output = parts.join("\n");
    if output.len() > crate::mcp_remote::RESULT_TEXT_BYTES_CAP {
        return Err(ResultError::BadResult);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bounded_instructions_redact_before_utf8_truncation() {
        let secret = "VERY-PRIVATE-CANARY".to_string();
        let text = format!(
            "{}{}{}",
            "界".repeat(INSTRUCTIONS_BYTES_CAP / 3 - 2),
            secret,
            "more".repeat(100)
        );
        let output = instructions(Some(&text), &[secret]).unwrap();
        assert!(output.len() <= INSTRUCTIONS_BYTES_CAP);
        assert!(!output.contains("VERY"));
        assert!(output.ends_with("[MCP instructions truncated]"));
    }

    #[test]
    fn mixed_unsupported_content_and_aggregate_caps_are_not_silent_success() {
        for block in [
            json!({"type":"image","data":"AA==","mimeType":"image/png"}),
            json!({"type":"audio","data":"AA==","mimeType":"audio/wav"}),
            json!({"type":"resource","resource":{"uri":"file:///fixture","blob":"AA=="}}),
        ] {
            let result = serde_json::from_value(json!({"content":[{"type":"text","text":"partial"},block], "structuredContent":{"answer":42}})).unwrap();
            assert!(matches!(
                project(result, &[]),
                Err(ResultError::Unsupported)
            ));
        }
        for result in [
            json!({"content":[],"structuredContent":{"large":"x".repeat(crate::mcp_remote::RESULT_TEXT_BYTES_CAP)}}),
            json!({"content":[{"type":"text","text":"x".repeat(600_000)},{"type":"text","text":"y".repeat(600_000)}]}),
        ] {
            assert!(matches!(
                project(serde_json::from_value(result).unwrap(), &[]),
                Err(ResultError::BadResult)
            ));
        }
    }

    #[test]
    fn arbitrary_error_canaries_never_escape_even_without_known_secrets() {
        for text in [
            "UNKNOWN-CANARY",
            "invalid parameters UNKNOWN-CANARY",
            "not found UNKNOWN-CANARY",
            "permission denied UNKNOWN-CANARY",
            "timed out UNKNOWN-CANARY",
        ] {
            let result = serde_json::from_value(
                json!({"content":[{"type":"text","text":text}],"isError":true}),
            )
            .unwrap();
            let Err(ResultError::Failed(actual)) = project(result, &[]) else {
                panic!("error must fail")
            };
            assert_eq!(actual, None);
        }
    }

    #[test]
    fn structured_failure_codes_are_exact_and_prose_cannot_override_them() {
        for (code, expected) in [
            ("INVALID_ARGUMENTS", Some(FailureDetail::InvalidArguments)),
            ("NOT_FOUND", Some(FailureDetail::NotFound)),
            ("RATE_LIMITED", Some(FailureDetail::RateLimited)),
            ("ACCESS_DENIED", Some(FailureDetail::AccessDenied)),
            ("TIMEOUT", Some(FailureDetail::Timeout)),
            ("EIO", None),
            ("rate_limited", None),
            ("TIMEOUT CANARY", None),
        ] {
            for structured in [
                json!({"error":{"code":code},"timeout":false}),
                json!({"code":code,"timeout":false}),
            ] {
                let result = serde_json::from_value(json!({"isError":true,"structuredContent":structured,"content":[{"type":"text","text":"invalid parameter; UNKNOWN-CANARY"}]})).unwrap();
                let Err(ResultError::Failed(actual)) = project(result, &[]) else {
                    panic!("isError must fail")
                };
                assert_eq!(actual, expected, "code {code}");
            }
        }
        for structured in [
            json!({"timeout":false}),
            json!({"error":{"code":"EIO"},"timeout":false}),
            json!({"other":{"code":"TIMEOUT"}}),
            json!({"error":{"code":null},"code":"TIMEOUT"}),
            json!({"error":{"code":"EIO"},"code":"TIMEOUT"}),
        ] {
            let result = serde_json::from_value(
                json!({"isError":true,"structuredContent":structured,"content":[]}),
            )
            .unwrap();
            assert!(matches!(
                project(result, &[]),
                Err(ResultError::Failed(None))
            ));
        }
    }

    #[test]
    fn rpc_failure_uses_only_standard_numeric_codes() {
        for (code, expected) in [
            (-32602, Some(FailureDetail::InvalidArguments)),
            (-32601, Some(FailureDetail::NotFound)),
            (-32603, None),
            (-32000, None),
        ] {
            let error = rmcp::service::ServiceError::McpError(serde_json::from_value(json!({"code":code,"message":"invalid parameter rate limited timeout UNKNOWN-CANARY","data":{"code":"TIMEOUT"}})).unwrap());
            assert_eq!(rpc_failure(&error), expected);
        }
    }

    #[test]
    fn secrets_do_not_mutate_wire_discriminators_before_projection() {
        let result =
            serde_json::from_value(json!({"content":[{"type":"text","text":"answer text"}]}))
                .unwrap();
        let Ok(output) = project(result, &["text".into()]) else {
            panic!("a secret matching a protocol word must not invalidate ordinary data")
        };
        assert_eq!(output, "answer [redacted]");
    }
}
