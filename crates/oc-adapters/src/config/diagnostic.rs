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
        details: None,
    }
}

/// Schema fragments only. Never preserve provider/permission/env/definition IDs.
pub(crate) fn schema_field(field: &str) -> Vec<String> {
    const SCHEMA: &[&str] = &[
        "providers",
        "package",
        "settings",
        "servers",
        "plugins",
        "commands",
        "policies",
        "legacy",
        "provider",
        "options",
        "models",
        "npm",
        "baseURL",
        "apiKey",
        "authToken",
        "authPolicy",
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
        "headerTimeout",
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
        "tool_output",
        "max_lines",
        "max_bytes",
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
        "agents",
        "steps",
        "maxSteps",
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
        "provider"
            | "providers"
            | "commands"
            | "mcp"
            | "agent"
            | "agents"
            | "command"
            | "permissions"
            | "permission"
            | "tools"
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
            vec!["plugins".into()],
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

pub(crate) fn from_note(note: &oc_core::queries::ConfigDiagnostic) -> ServiceDiagnostic {
    use oc_core::queries::ConfigDiagnosticKind;
    let code = match note.kind {
        ConfigDiagnosticKind::Invalid => ServiceCode::InvalidConfig,
        ConfigDiagnosticKind::FileUnavailable => ServiceCode::SourceUnavailable,
        _ => ServiceCode::IgnoredSetting,
    };
    let mut diagnostic = failure(
        "native configuration",
        &[],
        ServiceStage::Config,
        code,
        ServiceAction::ReviewConfiguration,
    );
    diagnostic.source = note.source.clone();
    diagnostic.field = note.field.clone();
    if note.kind == ConfigDiagnosticKind::Unsupported
        && note.field.first().is_some_and(|field| {
            matches!(
                field.as_str(),
                "snapshots" | "share" | "username" | "watcher" | "formatter"
            )
        })
    {
        diagnostic.details = Some(Box::new(vec![
            "This setting is unsupported and ignored.".into(),
        ]));
    }
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
