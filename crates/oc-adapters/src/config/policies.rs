//! Shared decoding for native provider and tool deny hooks.
use super::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    ProviderUse,
    ToolUse,
}

pub(crate) struct Statement {
    pub action: Action,
    pub resource: String,
    pub allow: bool,
}

/// Root rules precede experimental rules, preserving native source priority.
pub(crate) fn decode(value: &serde_json::Value) -> Result<Vec<Statement>, ConfigError> {
    let mut statements = Vec::new();
    for (rules, field) in [
        (value.get("policies"), "policies"),
        (
            value.pointer("/experimental/policies"),
            "experimental.policies",
        ),
    ] {
        let Some(rules) = rules else { continue };
        let invalid = || ConfigError::Invalid {
            field: field.into(),
            reason: "expected policy action, string resource and allow or deny effect".into(),
        };
        for rule in rules.as_array().ok_or_else(invalid)? {
            let text = |key: &str| rule.get(key).and_then(serde_json::Value::as_str);
            let allow = match text("effect") {
                Some("allow") => true,
                Some("deny") => false,
                _ => return Err(invalid()),
            };
            let resource = text("resource").ok_or_else(invalid)?.to_owned();
            let action = match text("action") {
                Some("provider.use") => Action::ProviderUse,
                Some("tool.use") => Action::ToolUse,
                Some("permission") => {
                    return Err(ConfigError::UnsupportedCapability {
                        field: field.into(),
                        reason: "permission policy hooks are not supported".into(),
                    });
                }
                _ => return Err(invalid()),
            };
            statements.push(Statement {
                action,
                resource,
                allow,
            });
        }
    }
    Ok(statements)
}
