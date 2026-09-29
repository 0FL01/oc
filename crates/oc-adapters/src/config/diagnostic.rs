//! Safe projection at config admission; legacy public assembler errors stay compatible.
use super::ConfigError;
use oc_core::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage};

pub(crate) fn failure(
    source: &str,
    field: &[&str],
    stage: ServiceStage,
    code: ServiceCode,
    action: ServiceAction,
) -> ServiceDiagnostic {
    ServiceDiagnostic {
        kind: ServiceKind::Configuration,
        service: "native".into(),
        source: super::mcp::safe_source_id(source),
        field: field.iter().map(|field| (*field).into()).collect(),
        stage,
        code,
        action,
    }
}

/// Schema fragments only. Never preserve provider/permission/env/definition IDs.
pub(crate) fn schema_field(field: &str) -> Vec<String> {
    const SCHEMA: &[&str] = &[
        "provider",
        "options",
        "models",
        "npm",
        "baseURL",
        "apiKey",
        "headers",
        "url",
        "type",
        "protocolVersion",
        "protocol",
        "codemode",
        "oauth",
        "callback_port",
        "environment",
        "env",
        "cwd",
        "timeouts",
        "timeout",
        "chunkTimeout",
        "setCacheKey",
        "nativeFallbackLimits",
        "context",
        "output",
        "permissions",
        "permission",
        "tools",
        "dcp",
        "mcp",
        "startup",
        "catalog",
        "execution",
        "snapshot",
        "snapshots",
        "animations",
        "terminal",
        "copy",
        "compaction",
        "auto",
        "keep",
        "tokens",
        "buffer",
        "prune",
        "tail_turns",
        "reserved",
        "preserve_recent_tokens",
        "keybinds",
        "leader",
        "leader_timeout",
        "experimental",
        "subagent_depth",
        "model",
        "default_agent",
        "enabled_providers",
        "disabled_providers",
        "plugin",
        "agent",
        "command",
        "skill",
        "definitions",
        "frontmatter",
        "instructions",
        "roots",
        "session",
        "sidebar",
        "tps",
        "diffs",
        "view",
        "wrap",
        "tabs",
        "layout",
        "scope",
        "indicators",
        "debug",
        "devtools",
        "field",
        "file",
        "document",
    ];
    let mut parts = field.split('.');
    let Some(first) = parts.next().filter(|part| SCHEMA.contains(part)) else {
        return vec!["document".into()];
    };
    let entry_domain = matches!(
        first,
        "provider" | "mcp" | "agent" | "command" | "permissions" | "permission" | "tools"
    );
    let mut safe = vec![first.into()];
    for (index, part) in parts.take(5).enumerate() {
        safe.push(if (entry_domain && index == 0) || !SCHEMA.contains(&part) {
            "entry".into()
        } else {
            part.into()
        });
    }
    safe
}

pub(crate) fn from_error(source: &str, error: &ConfigError) -> ServiceDiagnostic {
    let (field, stage, code) = match error {
        ConfigError::Invalid { field, .. } => (
            schema_field(field),
            ServiceStage::Config,
            ServiceCode::InvalidConfig,
        ),
        ConfigError::UnsupportedCapability { field, .. } => (
            schema_field(field),
            ServiceStage::Capability,
            ServiceCode::UnsupportedCapability,
        ),
        ConfigError::UnsupportedPlugin { .. } => (
            vec!["plugin".into()],
            ServiceStage::Capability,
            ServiceCode::UnsupportedPlugin,
        ),
        ConfigError::Untrusted { .. } => (
            vec!["file".into()],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        ),
        ConfigError::MissingCredential { field } => (
            schema_field(field),
            ServiceStage::Config,
            ServiceCode::MissingCredential,
        ),
    };
    let mut diagnostic = failure(source, &[], stage, code, ServiceAction::ReviewConfiguration);
    diagnostic.field = field;
    diagnostic
}

pub(super) struct LocatedError {
    pub(super) error: Box<ConfigError>,
    pub(crate) diagnostic: ServiceDiagnostic,
}

impl LocatedError {
    pub(super) fn new(source: &str, error: ConfigError) -> Self {
        let diagnostic = from_error(source, &error);
        Self {
            error: Box::new(error),
            diagnostic,
        }
    }
}
