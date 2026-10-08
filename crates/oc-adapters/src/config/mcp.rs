//! Pinned MCP forms → one canonical domain; narrow per-entry failure boundary.
use super::{ConfigError, McpEntry, Source};
use oc_core::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

/// Donor stage timeout overlays. Legacy numeric timeout never changes startup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpTimeouts {
    pub startup: Option<u64>,
    pub catalog: Option<u64>,
    pub execution: Option<u64>,
}

impl McpTimeouts {
    pub fn overlay(self, later: Self) -> Self {
        Self {
            startup: later.startup.or(self.startup),
            catalog: later.catalog.or(self.catalog),
            execution: later.execution.or(self.execution),
        }
    }
    pub fn startup_ms(self) -> u64 {
        self.startup.unwrap_or(30_000)
    }
    pub fn catalog_ms(self, legacy: Option<u64>) -> u64 {
        self.catalog.or(legacy).unwrap_or(30_000)
    }
    pub fn execution_ms(self, legacy: Option<u64>) -> u64 {
        self.execution.or(legacy).unwrap_or(43_200_000)
    }
}

pub(super) fn legacy_protocol() -> String {
    "legacy".into()
}

/// An identity is presentation, never a path, secret or free-text channel.
/// Even a syntactically ordinary name can equal configured credential material.
pub(crate) fn safe_identity(id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(id.as_bytes()));
    format!("server-{}", &digest[..56])
}

pub(crate) fn failure(id: &str, source: &str, field: &str, code: ServiceCode) -> ServiceDiagnostic {
    let fields = super::diagnostic::schema_field(&format!("mcp.entry.{field}"));
    ServiceDiagnostic {
        kind: ServiceKind::Mcp,
        service: safe_identity(id),
        source: safe_source_id(source),
        field: fields,
        stage: if matches!(
            code,
            ServiceCode::UnsupportedCapability | ServiceCode::UnsupportedProtocol
        ) {
            ServiceStage::Capability
        } else {
            ServiceStage::Config
        },
        code,
        action: ServiceAction::ReviewConfiguration,
    }
}

/// Stable source qualifier shared by optional-service diagnostics. Never exposes
/// an arbitrary basename, configured directory or control sequence.
pub(crate) fn safe_source_id(source: &str) -> String {
    let basename = std::path::Path::new(source)
        .file_name()
        .and_then(|p| p.to_str())
        .unwrap_or("config");
    format!(
        "source-{:x}/{}",
        u32::from_be_bytes(
            Sha256::digest(source.as_bytes())[..4]
                .try_into()
                .expect("digest prefix")
        ),
        if matches!(basename, "opencode.json" | "opencode.jsonc") {
            basename
        } else {
            "config"
        }
    )
}

pub(super) fn merge_document(
    source: &Source,
    obj: &Map<String, Value>,
    entries: &mut BTreeMap<String, (McpEntry, String)>,
    timeout: &mut McpTimeouts,
    provenance: &mut BTreeMap<String, String>,
) -> Result<(), ConfigError> {
    let Some(raw) = obj.get("mcp") else {
        return Ok(());
    };
    let map = raw.as_object().ok_or_else(|| ConfigError::Invalid {
        field: "mcp".into(),
        reason: "must be an object".into(),
    })?;
    let direct = |raw: &Value| {
        matches!(
            raw.get("type").and_then(Value::as_str),
            Some("local" | "remote")
        )
    };
    // Pinned normalize.ts:269/275: these are legacy names only with a direct type.
    for (id, raw) in map {
        if id == "servers" && !direct(raw) {
            continue;
        }
        if id == "timeout" && !direct(raw) {
            let layer = parse_timeout(raw, false).map_err(|field| ConfigError::Invalid {
                field: format!("mcp.timeout.{field}"),
                reason: "must be positive milliseconds".into(),
            })?;
            for (field, leaf) in [
                ("startup", layer.startup),
                ("catalog", layer.catalog),
                ("execution", layer.execution),
            ] {
                if leaf.is_some() {
                    provenance.insert(format!("mcp.timeout.{field}"), source.path.clone());
                }
            }
            *timeout = timeout.overlay(layer);
            continue;
        }
        entries.insert(
            id.clone(),
            (normalize(id, &source.path, raw, false), source.path.clone()),
        );
    }
    if let Some(raw) = map.get("servers").filter(|raw| !direct(raw)) {
        let servers = raw.as_object().ok_or_else(|| ConfigError::Invalid {
            field: "mcp.servers".into(),
            reason: "must be an object".into(),
        })?;
        for (id, raw) in servers {
            entries.insert(
                id.clone(),
                (normalize(id, &source.path, raw, true), source.path.clone()),
            );
        }
    }
    // Failed/disabled inventory is bounded too. Existing enabled/generation caps
    // remain fatal at runtime; this is not an optional-service catch-all.
    if entries.len() > 64 {
        return Err(ConfigError::Invalid {
            field: "mcp".into(),
            reason: "server inventory limit exceeded".into(),
        });
    }
    Ok(())
}

/// Credential authority belongs to the admitted source, even when a later
/// Location entry replaces its provider/server. Inspect cached source text and
/// the product env snapshot only: never open an inactive `{file:}` credential.
pub(crate) fn source_credential_values(
    source: &Source,
    env: &BTreeMap<String, String>,
) -> Result<Vec<String>, ConfigError> {
    let value = super::parse_jsonc(&source.text, "config")?;
    let Some(obj) = value.as_object() else {
        return Ok(Vec::new());
    };
    let mut raw = Vec::new();
    if let Some(providers) = obj.get("provider").and_then(Value::as_object) {
        for provider in providers.values() {
            if let Some(options) = provider.get("options").and_then(Value::as_object) {
                raw.extend(options.get("apiKey").and_then(Value::as_str));
                if let Some(headers) = options.get("headers").and_then(Value::as_object) {
                    raw.extend(headers.values().filter_map(Value::as_str));
                }
            }
        }
    }
    let mut entries = BTreeMap::new();
    merge_document(
        source,
        obj,
        &mut entries,
        &mut McpTimeouts::default(),
        &mut BTreeMap::new(),
    )?;
    for (entry, _) in entries.values() {
        if entry.enabled && entry.failure.is_none() {
            raw.extend(entry.headers.values().map(String::as_str));
            raw.extend(entry.environment.values().map(String::as_str));
        }
    }
    let mut values = Vec::new();
    for value in raw {
        if !value.contains("{file:") {
            let value = super::substitute(value, &source.path, source.trusted, env)?;
            if let Some(token) = value.strip_prefix("Bearer ") {
                values.push(token.to_string());
            }
            values.push(value);
        }
    }
    Ok(values)
}

/// Label protection includes inactive/failed definitions, but must not change
/// the credential-domain inputs used to authorize a local process launch.
pub(crate) fn source_protected_values(
    source: &Source,
    env: &BTreeMap<String, String>,
) -> Result<Vec<String>, ConfigError> {
    let value = super::parse_jsonc(&source.text, "config")?;
    let Some(obj) = value.as_object() else {
        return Ok(Vec::new());
    };
    let mut raw = Vec::new();
    for namespace in ["provider", "providers"] {
        if let Some(providers) = obj.get(namespace).and_then(Value::as_object) {
            for provider in providers.values() {
                source_provider_credentials(provider, &mut raw);
                if let Some(models) = provider.get("models").and_then(Value::as_object) {
                    for model in models.values() {
                        source_provider_credentials(model, &mut raw);
                        if let Some(variants) = model.get("variants") {
                            match variants {
                                Value::Array(items) => {
                                    for variant in items {
                                        source_provider_credentials(variant, &mut raw);
                                    }
                                }
                                Value::Object(items) => {
                                    for variant in items.values() {
                                        source_provider_credentials(variant, &mut raw);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }
    let mut entries = BTreeMap::new();
    merge_document(
        source,
        obj,
        &mut entries,
        &mut McpTimeouts::default(),
        &mut BTreeMap::new(),
    )?;
    // Normalization can stop before sensitive fields on a failed/unsupported
    // entry. Inspect only recognized fields in the original cached document.
    if let Some(map) = obj.get("mcp").and_then(Value::as_object) {
        for (name, entry) in map {
            if name == "servers"
                && !matches!(
                    entry.get("type").and_then(Value::as_str),
                    Some("local" | "remote")
                )
            {
                if let Some(servers) = entry.as_object() {
                    for entry in servers.values() {
                        source_entry_credentials(entry, &mut raw);
                    }
                }
            } else {
                source_entry_credentials(entry, &mut raw);
            }
        }
    }
    let mut values = Vec::new();
    for value in raw {
        // Observe each already-known env input independently of the completed
        // field: a later/earlier inactive file template may prevent substitution.
        let mut rest = value;
        while let Some((_, tail)) = rest.split_once('{') {
            let Some((tag, next)) = tail.split_once('}') else {
                break;
            };
            if let Some(key) = tag.strip_prefix("env:")
                && let Some(value) = env.get(key)
            {
                protected_value(value, &mut values);
            }
            rest = next;
        }
        if !value.contains("{file:") {
            let value = super::substitute(value, &source.path, source.trusted, env)?;
            protected_value(&value, &mut values);
        }
    }
    Ok(values)
}

fn source_provider_credentials<'a>(provider: &'a Value, values: &mut Vec<&'a str>) {
    // Legacy provider/model endpoint aliases are recognized before normalization,
    // including inactive model overrides whose credentials stay unresolved.
    values.extend(provider.get("api").and_then(Value::as_str));
    values.extend(
        provider
            .get("provider")
            .and_then(|provider| provider.get("api"))
            .and_then(Value::as_str),
    );
    for options in [
        provider.get("options"),
        provider.get("settings"),
        Some(provider),
    ]
    .into_iter()
    .flatten()
    {
        for field in ["apiKey", "authToken", "baseURL", "headers"] {
            if let Some(value) = options.get(field) {
                source_sensitive_strings(value, values);
            }
        }
    }
}

fn source_entry_credentials<'a>(entry: &'a Value, values: &mut Vec<&'a str>) {
    values.extend(entry.get("url").and_then(Value::as_str));
    for field in [
        "headers",
        "environment",
        "env",
        "credentials",
        "command",
        "cwd",
    ] {
        if let Some(value) = entry.get(field) {
            source_sensitive_strings(value, values);
        }
    }
    if let Some(oauth) = entry.get("oauth") {
        for field in ["clientSecret", "client_secret", "clientId", "client_id"] {
            if let Some(value) = oauth.get(field) {
                source_sensitive_strings(value, values);
            }
        }
    }
}

fn source_sensitive_strings<'a>(value: &'a Value, values: &mut Vec<&'a str>) {
    match value {
        Value::String(value) => values.push(value),
        Value::Array(items) => {
            for item in items {
                source_sensitive_strings(item, values);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                source_sensitive_strings(value, values);
            }
        }
        _ => {}
    }
}

pub(crate) fn protected_value(value: &str, values: &mut Vec<String>) {
    if value.is_empty() {
        return;
    }
    values.push(value.to_string());
    let trimmed = value.trim();
    if trimmed != value && !trimmed.is_empty() {
        values.push(trimmed.to_string());
    }
    if let Some((_, token)) = trimmed.split_once(' ')
        && !token.is_empty()
    {
        values.push(token.to_string());
    }
    let mut fields = trimmed.split_ascii_whitespace();
    if fields
        .next()
        .is_some_and(|field| field.eq_ignore_ascii_case("bearer"))
        && let Some(token) = fields.next()
        && fields.next().is_none()
    {
        values.push(token.to_string());
    }
    if let Ok(mut url) = reqwest::Url::parse(trimmed) {
        let credentials = [
            Some(url.username().to_string()),
            url.password().map(str::to_string),
        ];
        for value in credentials.into_iter().flatten() {
            if !value.is_empty() {
                values.push(value.clone());
                // Reuse the URL parser's percent decoder, preserving literal
                // '+'/'&' in userinfo rather than applying form semantics to it.
                let query = format!(
                    "credential={}",
                    value.replace('+', "%2B").replace('&', "%26")
                );
                url.set_query(Some(&query));
                if let Some((_, decoded)) = url.query_pairs().next() {
                    values.push(decoded.into_owned());
                }
            }
        }
    }
}

pub(crate) fn entry_credential_values(
    entry: &McpEntry,
    env: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut values = Vec::new();
    for value in entry
        .environment
        .values()
        .chain(entry.blocked_inherited_values.iter())
        .chain(entry.url.iter())
        .chain(entry.headers.values())
        .chain(entry.command.iter())
        .chain(entry.cwd.iter())
    {
        protected_value(value, &mut values);
    }
    if entry.kind == "local" {
        let (environment, secrets) = crate::mcp_stdio::effective_environment(entry, env);
        for value in environment.values().chain(secrets.iter()) {
            protected_value(value, &mut values);
        }
    }
    values
}

// JSON.parse in the pinned donor erases integer-valued decimal/exponent
// spellings. Preserve PositiveInt semantics while checking the native range.
fn positive_integer(raw: &Value) -> Option<u64> {
    raw.as_u64()
        .or_else(|| {
            raw.as_f64()
                .filter(|value| {
                    value.is_finite()
                        && value.fract() == 0.0
                        && *value > 0.0
                        && *value < u64::MAX as f64
                })
                .map(|value| value as u64)
        })
        .filter(|value| *value > 0)
}

fn parse_timeout(raw: &Value, numeric: bool) -> Result<McpTimeouts, &'static str> {
    let positive = |v: &Value| {
        positive_integer(v).filter(|ms| {
            tokio::time::Instant::now()
                .checked_add(std::time::Duration::from_millis(*ms))
                .is_some()
        })
    };
    if numeric && raw.is_number() {
        let ms = positive(raw).ok_or("timeout")?;
        return Ok(McpTimeouts {
            catalog: Some(ms),
            execution: Some(ms),
            ..Default::default()
        });
    }
    let map = raw.as_object().ok_or("timeout")?;
    let mut out = McpTimeouts::default();
    for (field, target) in [
        ("startup", &mut out.startup),
        ("catalog", &mut out.catalog),
        ("execution", &mut out.execution),
    ] {
        if let Some(v) = map.get(field) {
            *target = Some(positive(v).ok_or(field)?);
        }
    }
    Ok(out)
}

fn normalize(id: &str, source: &str, raw: &Value, canonical: bool) -> McpEntry {
    let mut entry = McpEntry::default();
    let parsed = parse_entry(raw, canonical, &mut entry);
    if let Err((field, code)) = parsed {
        entry.failure = Some(failure(id, source, field, code));
    }
    entry
}

fn parse_entry(
    raw: &Value,
    canonical: bool,
    entry: &mut McpEntry,
) -> Result<(), (&'static str, ServiceCode)> {
    use ServiceCode::{InvalidConfig, UnsupportedCapability, UnsupportedProtocol};
    let map = raw.as_object().ok_or(("entry", InvalidConfig))?;
    let string = |field: &'static str| {
        map.get(field)
            .and_then(Value::as_str)
            .ok_or((field, InvalidConfig))
    };
    let boolean = |field: &'static str| {
        map.get(field)
            .map(|v| v.as_bool().ok_or((field, InvalidConfig)))
            .transpose()
    };
    entry.enabled = if canonical {
        !boolean("disabled")?.unwrap_or(false)
    } else {
        boolean("enabled")?.unwrap_or(true)
    };
    boolean("enabled")?;
    boolean("disabled")?;
    entry.kind = string("type")?.to_string();
    if !matches!(entry.kind.as_str(), "local" | "remote") {
        return Err(("type", InvalidConfig));
    }
    // Recognized security fields never become donor excess-field omission.
    for field in [
        "permission",
        "permissions",
        "trust",
        "credentials",
        "sandbox",
        "env",
        "inherit_credentials",
        "resource_admitted",
        "blocked_inherited_values",
    ] {
        if map.contains_key(field) {
            return Err((field, UnsupportedCapability));
        }
    }
    if entry.kind == "local" {
        entry.command = map
            .get("command")
            .and_then(Value::as_array)
            .ok_or(("command", InvalidConfig))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or(("command", InvalidConfig))
            })
            .collect::<Result<_, _>>()?;
        if entry.command.is_empty()
            || entry.command[0].is_empty()
            || entry.command.iter().any(|s| s.contains('\0'))
        {
            return Err(("command", InvalidConfig));
        }
        if map.contains_key("cwd") {
            entry.cwd = Some(string("cwd")?.to_string());
        }
        if let Some(raw) = map.get("environment") {
            entry.environment = string_map(raw).ok_or(("environment", InvalidConfig))?;
        }
        if entry
            .environment
            .iter()
            .any(|(k, v)| k.is_empty() || k.contains(['=', '\0']) || v.contains('\0'))
        {
            return Err(("environment", InvalidConfig));
        }
        for field in ["url", "headers", "oauth"] {
            if map.contains_key(field) {
                return Err((field, UnsupportedCapability));
            }
        }
    } else {
        entry.url = Some(string("url")?.to_string());
        if !entry
            .url
            .as_deref()
            .is_some_and(|url| url.contains("{env:") || url.contains("{file:"))
        {
            let mut validate = entry.clone();
            validate.enabled = true;
            if crate::mcp_remote::CodexWebConfig::from_remote_entry(&validate).is_err() {
                return Err(("url", InvalidConfig));
            }
        }
        if let Some(raw) = map.get("headers") {
            entry.headers = string_map(raw).ok_or(("headers", InvalidConfig))?;
        }
        for field in ["command", "cwd", "environment"] {
            if map.contains_key(field) {
                return Err((field, UnsupportedCapability));
            }
        }
        if let Some(oauth) = map.get("oauth") {
            if oauth == &Value::Bool(false) {
            } else if oauth == &Value::Bool(true) || oauth.is_object() {
                if let Some(object) = oauth.as_object() {
                    for (name, field) in [
                        ("clientId", "oauth.clientId"),
                        ("clientSecret", "oauth.clientSecret"),
                        ("scope", "oauth.scope"),
                        ("redirectUri", "oauth.redirectUri"),
                        ("client_id", "oauth.client_id"),
                        ("client_secret", "oauth.client_secret"),
                        ("redirect_uri", "oauth.redirect_uri"),
                        ("auth_server_metadata_url", "oauth.auth_server_metadata_url"),
                    ] {
                        if object.get(name).is_some_and(|v| !v.is_string()) {
                            return Err((field, InvalidConfig));
                        }
                    }
                    for (name, field) in [
                        ("callbackPort", "oauth.callbackPort"),
                        ("callback_port", "oauth.callback_port"),
                    ] {
                        if object.get(name).is_some_and(|v| {
                            !positive_integer(v).is_some_and(|p| (1..=65535).contains(&p))
                        }) {
                            return Err((field, InvalidConfig));
                        }
                    }
                }
                return Err(("oauth", UnsupportedCapability));
            } else {
                return Err(("oauth", InvalidConfig));
            }
        }
    }
    if let Some(raw) = map.get("timeout") {
        entry.timeouts = parse_timeout(raw, !canonical).map_err(|_| ("timeout", InvalidConfig))?;
        entry.timeout = positive_integer(raw);
    }
    entry.codemode = boolean("codemode")?;
    if let Some(raw) = map.get("protocol") {
        entry.protocol = raw.as_str().ok_or(("protocol", InvalidConfig))?.to_string();
    }
    if entry.protocol != "legacy" {
        return Err((
            "protocol",
            if matches!(entry.protocol.as_str(), "auto" | "2026-07-28") {
                UnsupportedProtocol
            } else {
                InvalidConfig
            },
        ));
    }
    if entry.codemode == Some(true) {
        return Err(("codemode", UnsupportedCapability));
    }
    if entry.cwd.as_ref().is_some_and(|v| v.contains('\0')) {
        return Err(("cwd", InvalidConfig));
    }
    // Header conflicts/typing are validated even on a disabled entry. Templates
    // remain inert until enabled; URI/credential validation follows substitution.
    check_headers(entry).map_err(|code| ("headers", code))?;
    Ok(())
}

fn string_map(raw: &Value) -> Option<BTreeMap<String, String>> {
    raw.as_object()?
        .iter()
        .map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect()
}

fn check_headers(entry: &McpEntry) -> Result<(), ServiceCode> {
    match crate::mcp_remote::normalize_headers(&entry.headers) {
        Ok(_) => Ok(()),
        Err(crate::mcp_remote::McpError::ConflictingHeader(name)) => {
            Err(if name == "authorization" {
                ServiceCode::AuthorizationHeaderConflict
            } else {
                ServiceCode::HeaderConflict
            })
        }
        Err(_) => Err(ServiceCode::InvalidHeader),
    }
}

pub(super) fn validate_effective(id: &str, source: &str, entry: &mut McpEntry) {
    let issue = if entry.kind == "remote" {
        check_headers(entry)
            .err()
            .map(|code| ("headers", code))
            .or_else(|| {
                if id == "codex_web"
                    && !entry.headers.iter().any(|(name, value)| {
                        name.eq_ignore_ascii_case("authorization")
                            && value
                                .strip_prefix("Bearer ")
                                .is_some_and(|token| !token.trim().is_empty())
                    })
                {
                    return Some(("headers.authorization", ServiceCode::MissingCredential));
                }
                let result = if id == "codex_web" {
                    crate::mcp_remote::CodexWebConfig::from_entry(entry)
                } else {
                    crate::mcp_remote::CodexWebConfig::from_remote_entry(entry)
                };
                result.err().map(|_| ("url", ServiceCode::InvalidConfig))
            })
    } else if entry.command.is_empty()
        || entry.command[0].is_empty()
        || entry.command.iter().any(|s| s.contains('\0'))
    {
        Some(("command", ServiceCode::InvalidConfig))
    } else if entry
        .environment
        .iter()
        .any(|(k, v)| k.contains(['=', '\0']) || v.contains('\0'))
    {
        Some(("environment", ServiceCode::InvalidConfig))
    } else {
        None
    };
    if let Some((field, code)) = issue {
        entry.failure = Some(failure(id, source, field, code));
    }
}
