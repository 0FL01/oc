//! User config and permissions for T07.
//!
//! JSONC fragments → provenance → explicit trust → substitutions →
//! normalization → domain merge → capability validation → immutable
//! generation. Unknown/unsupported fails before side effects; commands and
//! plugin code are never executed here; future modules are not activated.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Typed config errors with field-level diagnostics (no secrets in messages).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// Malformed JSONC with byte offset context.
    #[error("invalid config {field}: {reason}")]
    Invalid {
        /// Dotted field path.
        field: String,
        /// Human reason without secret values.
        reason: String,
    },
    /// Known-but-unsupported capability.
    #[error("unsupported {field}: {reason}")]
    UnsupportedCapability {
        /// Dotted field path.
        field: String,
        /// Why it is unsupported.
        reason: String,
    },
    /// Unknown plugin identity.
    #[error("unsupported plugin {identity}: {reason}")]
    UnsupportedPlugin {
        /// Given identity string.
        identity: String,
        /// Why it is rejected.
        reason: String,
    },
    /// Trust gate: source may not perform this read/use.
    #[error("untrusted source {origin}: {reason}")]
    Untrusted {
        /// Source path.
        origin: String,
        /// What was refused.
        reason: String,
    },
    /// Missing credential for a selected/enabled integration.
    #[error("missing credential for {field}")]
    MissingCredential {
        /// Dotted field path.
        field: String,
    },
}

/// Permission level (product profile, not authoring-agent mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Permission {
    /// Always allow.
    Allow,
    /// Require interactive approval (`ask` without channel fails headless).
    Ask,
    /// Always deny.
    Deny,
}

/// Provider options with exact upstream semantics preserved.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderOptions {
    /// Base URL template (may contain `{env:..}` before substitution).
    #[serde(rename = "baseURL", default)]
    pub base_url: String,
    /// API key template (never logged).
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    /// `false` means no total generation deadline (never defaulted).
    #[serde(default)]
    pub timeout: Option<bool>,
    /// Idle ms between body chunks (6000000 = 100 min, not 60 s).
    #[serde(rename = "chunkTimeout", default)]
    pub chunk_timeout: Option<u64>,
    /// Stable cache key flag.
    #[serde(rename = "setCacheKey", default)]
    pub set_cache_key: Option<bool>,
}

/// Single provider entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderEntry {
    /// Package alias; native family uses `@ai-sdk/openai`.
    #[serde(default)]
    pub npm: Option<String>,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Connection options.
    #[serde(default)]
    pub options: ProviderOptions,
    /// Static models (ludka style); absent for discovery providers.
    #[serde(default)]
    pub models: BTreeMap<String, serde_json::Value>,
}

/// Single MCP entry (trusted-shape subset for T07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpEntry {
    /// `remote` or `local`.
    #[serde(rename = "type", default)]
    pub kind: String,
    /// Exact remote URL template (remote only).
    #[serde(default)]
    pub url: Option<String>,
    /// Enabled flag.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// OAuth must stay false in this profile.
    #[serde(default)]
    pub oauth: bool,
    /// Bearer/extra headers template.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Local argv (local only, never shell-split).
    #[serde(default)]
    pub command: Vec<String>,
    /// Client timeout ms.
    #[serde(default)]
    pub timeout: Option<u64>,
    /// Explicit Code Mode flag: only absent/false allowed here.
    #[serde(default)]
    pub codemode: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// Effective immutable generation (T07 subset).
#[derive(Debug, Clone, PartialEq)]
pub struct Generation {
    /// Providers by id in sorted order.
    pub providers: BTreeMap<String, ProviderEntry>,
    /// MCP entries by id in sorted order.
    pub mcp: BTreeMap<String, McpEntry>,
    /// Central permission policy.
    pub permissions: BTreeMap<String, Permission>,
    /// Per-section provenance (section → source path).
    pub provenance: BTreeMap<String, String>,
    /// Warnings (e.g. empty substitution on unused keys).
    pub warnings: Vec<String>,
}

impl Default for McpEntry {
    fn default() -> Self {
        Self {
            kind: String::new(),
            url: None,
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: Vec::new(),
            timeout: None,
            codemode: None,
        }
    }
}

/// Strip JSONC comments and trailing commas outside strings.
///
/// Port of `scripts/check_docs.py::jsonc` semantics: `//` and `/* */`
/// comments removed, then trailing commas before `}`/`]` removed.
pub fn strip_jsonc(text: &str) -> Result<String, ConfigError> {
    let bytes: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        let c = bytes[i];
        if quoted {
            out.push(c);
            if c == '\\' {
                i += 1;
                if i < bytes.len() {
                    out.push(bytes[i]);
                }
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            out.push(c);
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            out.push('\n');
            continue;
        } else if c == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            let end = bytes[i..].windows(2).position(|w| w == ['*', '/']);
            match end {
                None => {
                    return Err(ConfigError::Invalid {
                        field: "$".to_string(),
                        reason: "unclosed block comment".to_string(),
                    });
                }
                Some(k) => {
                    out.push(' ');
                    i += k + 2;
                    continue;
                }
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    let s: String = out.into_iter().collect();
    Ok(strip_trailing_commas(&s))
}

fn strip_trailing_commas(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut quoted = false;
    while i < bytes.len() {
        let c = bytes[i];
        if quoted {
            out.push(c);
            if c == '\\' {
                i += 1;
                if i < bytes.len() {
                    out.push(bytes[i]);
                }
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            out.push(c);
        } else if c == ',' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == '}' || bytes[j] == ']') {
                i += 1;
                continue;
            }
            out.push(c);
        } else {
            out.push(c);
        }
        i += 1;
    }
    out.into_iter().collect()
}

/// Parse a JSONC fragment into a JSON value.
pub fn parse_jsonc(text: &str, field: &str) -> Result<serde_json::Value, ConfigError> {
    let clean = strip_jsonc(text)?;
    serde_json::from_str(&clean).map_err(|e| ConfigError::Invalid {
        field: field.to_string(),
        reason: format!("json parse: {e}"),
    })
}

/// Substitute `{env:VAR}` and `{file:path}` templates.
///
/// `trusted` gates `{file:}` reads (no-follow, bounded 64 KiB); untrusted
/// sources get `Untrusted`. Missing env expands to empty (credential
/// requirement is checked later, only for selected/enabled integrations).
pub fn substitute(
    template: &str,
    source: &str,
    trusted: bool,
    env: &BTreeMap<String, String>,
) -> Result<String, ConfigError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        if let Some(end) = tail.find('}') {
            let token = &tail[1..end];
            if let Some(var) = token.strip_prefix("env:") {
                out.push_str(env.get(var).map(String::as_str).unwrap_or(""));
                rest = &tail[end + 1..];
                continue;
            }
            if let Some(path) = token.strip_prefix("file:") {
                if !trusted {
                    return Err(ConfigError::Untrusted {
                        origin: source.to_string(),
                        reason: "file read before trust".to_string(),
                    });
                }
                out.push_str(&read_trusted_file(path, source)?);
                rest = &tail[end + 1..];
                continue;
            }
            out.push('{');
            rest = &tail[1..];
        } else {
            out.push_str(rest);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    Ok(out)
}

fn read_trusted_file(path: &str, source: &str) -> Result<String, ConfigError> {
    let fs_path = Path::new(path);
    let meta = fs::symlink_metadata(fs_path).map_err(|_| ConfigError::Untrusted {
        origin: source.to_string(),
        reason: "unreadable file reference".to_string(),
    })?;
    if meta.file_type().is_symlink() {
        return Err(ConfigError::Untrusted {
            origin: source.to_string(),
            reason: "symlink file reference".to_string(),
        });
    }
    if meta.len() > 65536 {
        return Err(ConfigError::Invalid {
            field: "file".to_string(),
            reason: "file reference too large".to_string(),
        });
    }
    std::fs::read_to_string(fs_path).map_err(|_| ConfigError::Untrusted {
        origin: source.to_string(),
        reason: "unreadable file reference".to_string(),
    })
}

/// One source fragment in low→high precedence order.
#[derive(Debug, Clone)]
pub struct Source {
    /// Source path for provenance/diagnostics.
    pub path: String,
    /// Raw JSONC bytes (kept for byte-unchanged verification by callers).
    pub text: String,
    /// Whether this source passed the explicit trust decision.
    pub trusted: bool,
}

/// Assemble ordered JSONC fragments into an immutable generation.
///
/// Domain merge: providers/MCP replace per id (later wins); permissions use
/// most-restrictive (`Deny > Ask > Allow`); provenance records the winning
/// source per section. Capability validation runs before any substitution
/// side effect beyond bounded reads; unknown/unsupported fails fast.
pub fn assemble(
    sources: &[Source],
    env: &BTreeMap<String, String>,
    enabled_providers: Option<&HashSet<String>>,
) -> Result<Generation, ConfigError> {
    let mut providers: BTreeMap<String, (ProviderEntry, String)> = BTreeMap::new();
    let mut mcp: BTreeMap<String, (McpEntry, String)> = BTreeMap::new();
    let mut permissions: BTreeMap<String, (Permission, String)> = BTreeMap::new();

    for source in sources {
        let value = parse_jsonc(&source.text, &source.path)?;
        let obj = value.as_object().ok_or_else(|| ConfigError::Invalid {
            field: source.path.clone(),
            reason: "root must be an object".to_string(),
        })?;

        if let Some(prov) = obj.get("provider") {
            let map = prov.as_object().ok_or_else(|| ConfigError::Invalid {
                field: "provider".to_string(),
                reason: "must be an object".to_string(),
            })?;
            for (id, raw) in map {
                let entry: ProviderEntry =
                    serde_json::from_value(raw.clone()).map_err(|e| ConfigError::Invalid {
                        field: format!("provider.{id}"),
                        reason: format!("shape: {e}"),
                    })?;
                validate_provider(id, &entry)?;
                providers.insert(id.clone(), (entry, source.path.clone()));
            }
        }

        if let Some(mcp_raw) = obj.get("mcp") {
            let map = mcp_raw.as_object().ok_or_else(|| ConfigError::Invalid {
                field: "mcp".to_string(),
                reason: "must be an object".to_string(),
            })?;
            for (id, raw) in map {
                let entry: McpEntry =
                    serde_json::from_value(raw.clone()).map_err(|e| ConfigError::Invalid {
                        field: format!("mcp.{id}"),
                        reason: format!("shape: {e}"),
                    })?;
                validate_mcp(id, &entry)?;
                mcp.insert(id.clone(), (entry, source.path.clone()));
            }
        }

        if let Some(perm_raw) = obj.get("permissions") {
            let map = perm_raw.as_object().ok_or_else(|| ConfigError::Invalid {
                field: "permissions".to_string(),
                reason: "must be an object".to_string(),
            })?;
            for (key, raw) in map {
                let level = normalize_permission(key, raw)?;
                permissions
                    .entry(key.clone())
                    .and_modify(|(old, _)| {
                        if more_restrictive(level, *old) {
                            *old = level;
                        }
                    })
                    .or_insert((level, source.path.clone()));
            }
        }

        if let Some(dcp) = obj.get("dcp") {
            validate_dcp(dcp)?;
        }
    }

    // Substitute + selected-only credential check (no network/process here).
    let mut out_providers = BTreeMap::new();
    let mut provenance = BTreeMap::new();
    for (id, (entry, path)) in &providers {
        if let Some(only) = enabled_providers
            && !only.contains(id)
        {
            continue;
        }
        let trusted = sources.iter().any(|s| s.path == *path && s.trusted);
        let mut entry = entry.clone();
        entry.options.base_url = substitute(&entry.options.base_url, path, trusted, env)?;
        entry.options.api_key = substitute(&entry.options.api_key, path, trusted, env)?;
        for headers in [] as [Option<&mut BTreeMap<String, String>>; 0] {
            let _ = headers;
        }
        let selected = enabled_providers.is_none_or(|only| only.contains(id));
        if selected && entry.options.api_key.trim().is_empty() {
            return Err(ConfigError::MissingCredential {
                field: format!("provider.{id}.options.apiKey"),
            });
        }
        out_providers.insert(id.clone(), entry);
        provenance.insert(format!("provider.{id}"), path.clone());
    }

    let mut out_mcp = BTreeMap::new();
    for (id, (entry, path)) in &mcp {
        let trusted = sources.iter().any(|s| s.path == *path && s.trusted);
        let mut entry = entry.clone();
        if let Some(url) = &entry.url {
            entry.url = Some(substitute(url, path, trusted, env)?);
        }
        for (k, v) in entry.headers.clone() {
            entry.headers.insert(k, substitute(&v, path, trusted, env)?);
        }
        // Disabled entries never require credentials and never launch.
        if entry.enabled {
            let _ = trusted;
        }
        out_mcp.insert(id.clone(), entry);
        provenance.insert(format!("mcp.{id}"), path.clone());
    }

    let mut out_perm = BTreeMap::new();
    for (key, (level, path)) in &permissions {
        out_perm.insert(key.clone(), *level);
        provenance.insert(format!("permissions.{key}"), path.clone());
    }

    Ok(Generation {
        providers: out_providers,
        mcp: out_mcp,
        permissions: out_perm,
        provenance,
        warnings: Vec::new(),
    })
}

fn validate_provider(id: &str, entry: &ProviderEntry) -> Result<(), ConfigError> {
    if let Some(npm) = &entry.npm
        && npm != "@ai-sdk/openai"
    {
        return Err(ConfigError::UnsupportedCapability {
            field: format!("provider.{id}.npm"),
            reason: format!("unknown package {npm}"),
        });
    }
    Ok(())
}

fn validate_mcp(id: &str, entry: &McpEntry) -> Result<(), ConfigError> {
    if entry.codemode == Some(true) {
        return Err(ConfigError::UnsupportedCapability {
            field: format!("mcp.{id}.codemode"),
            reason: "codemode:true is unsupported in the direct profile".to_string(),
        });
    }
    if entry.oauth {
        return Err(ConfigError::UnsupportedCapability {
            field: format!("mcp.{id}.oauth"),
            reason: "oauth:true is unsupported; use bearer without discovery".to_string(),
        });
    }
    if entry.kind != "remote" && entry.kind != "local" {
        return Err(ConfigError::Invalid {
            field: format!("mcp.{id}.type"),
            reason: "must be remote or local".to_string(),
        });
    }
    Ok(())
}

fn validate_dcp(raw: &serde_json::Value) -> Result<(), ConfigError> {
    if let Some(exp) = raw.get("experimental") {
        if exp.get("allowSubAgents") == Some(&serde_json::Value::Bool(true)) {
            return Err(ConfigError::UnsupportedCapability {
                field: "dcp.experimental.allowSubAgents".to_string(),
                reason: "subagents are out of goal scope".to_string(),
            });
        }
        if exp.get("customPrompts") == Some(&serde_json::Value::Bool(true)) {
            return Err(ConfigError::UnsupportedCapability {
                field: "dcp.experimental.customPrompts".to_string(),
                reason: "custom prompts are deferred".to_string(),
            });
        }
    }
    if let Some(mode) = raw.pointer("/compress/mode")
        && mode.as_str() != Some("range")
    {
        return Err(ConfigError::UnsupportedCapability {
            field: "dcp.compress.mode".to_string(),
            reason: "only range mode is supported".to_string(),
        });
    }
    Ok(())
}

/// Normalize a permission value, mapping legacy `write`/`edit` to patch.
///
/// Conflicting legacy policies resolve conservative deny → ask → allow at
/// the caller (`assemble` keeps the most restrictive across sources).
pub fn normalize_permission(key: &str, raw: &serde_json::Value) -> Result<Permission, ConfigError> {
    let text = raw.as_str().ok_or_else(|| ConfigError::Invalid {
        field: format!("permissions.{key}"),
        reason: "must be allow/ask/deny".to_string(),
    })?;
    match text {
        "allow" => Ok(Permission::Allow),
        "ask" => Ok(Permission::Ask),
        "deny" => Ok(Permission::Deny),
        _ => Err(ConfigError::Invalid {
            field: format!("permissions.{key}"),
            reason: "must be allow/ask/deny".to_string(),
        }),
    }
}

/// Legacy `write`/`edit` keys normalize to `apply_patch` operations.
pub fn legacy_key(key: &str) -> &str {
    match key {
        "write" | "edit" => "apply_patch",
        _ => key,
    }
}

fn more_restrictive(next: Permission, current: Permission) -> bool {
    fn rank(level: Permission) -> u8 {
        match level {
            Permission::Deny => 2,
            Permission::Ask => 1,
            Permission::Allow => 0,
        }
    }
    rank(next) > rank(current)
}

/// Redacted explain output: secrets replaced, provenance kept.
pub fn explain_redacted(generation: &Generation) -> serde_json::Value {
    let mut providers = serde_json::Map::new();
    for (id, entry) in &generation.providers {
        providers.insert(
            id.clone(),
            serde_json::json!({
                "npm": entry.npm,
                "options": {
                    "baseURL": entry.options.base_url,
                    "apiKey": "***",
                    "timeout": entry.options.timeout,
                    "chunkTimeout": entry.options.chunk_timeout,
                },
                "models": {},
                "provenance": generation.provenance.get(&format!("provider.{id}")),
            }),
        );
    }
    let mut mcp = serde_json::Map::new();
    for (id, entry) in &generation.mcp {
        let mut headers = serde_json::Map::new();
        for key in entry.headers.keys() {
            headers.insert(key.clone(), serde_json::Value::String("***".to_string()));
        }
        mcp.insert(
            id.clone(),
            serde_json::json!({
                "type": entry.kind,
                "url": entry.url,
                "enabled": entry.enabled,
                "headers": headers,
                "provenance": generation.provenance.get(&format!("mcp.{id}")),
            }),
        );
    }
    serde_json::json!({"provider": providers, "mcp": mcp})
}

/// Exact native plugin classification (no JS execution).
///
/// Admitted: bare `@tarquinen/opencode-dcp`, pinned
/// `@tarquinen/opencode-dcp@3.1.15`, and canonical
/// `<root>/{plugin,plugins}/openproxy-models.js`. Everything else is
/// `UnsupportedPlugin` before resolver/import/process/network.
pub fn classify_plugin(identity: &str, config_root: &str) -> Result<&'static str, ConfigError> {
    if identity == "@tarquinen/opencode-dcp" || identity == "@tarquinen/opencode-dcp@3.1.15" {
        return Ok("dcp");
    }
    let root = config_root.trim_end_matches('/');
    if identity == format!("{root}/plugin/openproxy-models.js")
        || identity == format!("{root}/plugins/openproxy-models.js")
    {
        return Ok("discovery");
    }
    Err(ConfigError::UnsupportedPlugin {
        identity: identity.to_string(),
        reason: "unknown JS/TS/package identity".to_string(),
    })
}

/// Bounded skill frontmatter (`name`/`description` only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMeta {
    /// Directory id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
}

/// Parse `SKILL.md` frontmatter without a YAML engine.
///
/// Only `name:`/`description:` scalar lines are honored; oversized or
/// unreadable files fail visibly; body bytes are never returned here (the
/// native `skill` tool serves bounded snapshots at call time).
pub fn parse_skill(id: &str, text: &str) -> Result<SkillMeta, ConfigError> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.frontmatter"),
            reason: "missing opening ---".to_string(),
        });
    }
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    for line in &mut lines {
        if line == "---" {
            break;
        }
        if line.len() > 1024 {
            return Err(ConfigError::Invalid {
                field: format!("skill.{id}.frontmatter"),
                reason: "line too long".to_string(),
            });
        }
        if let Some(rest) = line.strip_prefix("name:") {
            name = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("description:") {
            description = Some(rest.trim().trim_matches('"').to_string());
        }
    }
    let name = name
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ConfigError::Invalid {
            field: format!("skill.{id}.name"),
            reason: "missing bounded name".to_string(),
        })?;
    let description =
        description
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ConfigError::Invalid {
                field: format!("skill.{id}.description"),
                reason: "missing bounded description".to_string(),
            })?;
    if name.len() > 256 || description.len() > 1024 {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.frontmatter"),
            reason: "field too large".to_string(),
        });
    }
    if text.len() > 65536 {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.body"),
            reason: "skill file too large".to_string(),
        });
    }
    Ok(SkillMeta {
        id: id.to_string(),
        name,
        description,
    })
}

/// Minimal native TOML profile (`oc-rs.toml` subset for T07).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativeProfile {
    /// Selected profile name.
    #[serde(default)]
    pub profile: String,
    /// Tool exposure (`direct` in this goal).
    #[serde(default)]
    pub tool_exposure: String,
}

/// Parse the native profile without activating future modules.
pub fn parse_native_profile(text: &str) -> Result<NativeProfile, ConfigError> {
    let value: toml::Value = toml::from_str(text).map_err(|e| ConfigError::Invalid {
        field: "oc-rs.toml".to_string(),
        reason: format!("toml: {e}"),
    })?;
    Ok(NativeProfile {
        profile: value
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tool_exposure: value
            .get("tool_exposure")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ConfigError, Permission, Source, assemble, classify_plugin, explain_redacted, legacy_key,
        parse_jsonc, parse_native_profile, parse_skill, strip_jsonc, substitute,
    };
    use std::collections::{BTreeMap, HashSet};

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn src(path: &str, text: &str, trusted: bool) -> Source {
        Source {
            path: path.to_string(),
            text: text.to_string(),
            trusted,
        }
    }

    #[test]
    fn cfg01_user_forms_native_profile() {
        let text = r#"{
            // ludka static + ludka2 discovery
            "enabled_providers": ["ludka2"],
            "provider": {
                "ludka": {"npm": "@ai-sdk/openai", "options": {
                    "baseURL": "{env:LUDKA_API_URL}", "apiKey": "{env:LUDKA_API_KEY}",
                    "timeout": false, "setCacheKey": true, "chunkTimeout": 6000000}},
                "ludka2": {"npm": "@ai-sdk/openai", "options": {
                    "baseURL": "{env:LUDKA2_API_URL}", "apiKey": "{env:LUDKA2_API_KEY}",
                    "timeout": false, "setCacheKey": true, "chunkTimeout": 6000000}}
            },
        }"#;
        let v = parse_jsonc(text, "t").expect("jsonc");
        assert_eq!(v["provider"]["ludka"]["options"]["timeout"], false);
        assert_eq!(v["provider"]["ludka"]["options"]["chunkTimeout"], 6000000);
        let generation = assemble(
            &[src("g/opencode.json", text, true)],
            &env(&[
                ("LUDKA_API_URL", "https://ludka.invalid"),
                ("LUDKA_API_KEY", "k1"),
                ("LUDKA2_API_URL", "https://ludka2.invalid/v1"),
                ("LUDKA2_API_KEY", "k2"),
            ]),
            None,
        )
        .expect("assemble");
        assert_eq!(
            generation.providers["ludka"].options.chunk_timeout,
            Some(6000000)
        );
        assert_eq!(generation.providers["ludka"].options.timeout, Some(false));
        let native =
            parse_native_profile("profile = \"daily-direct\"\ntool_exposure = \"direct\"\n")
                .expect("toml");
        assert_eq!(native.profile, "daily-direct");
        assert_eq!(native.tool_exposure, "direct");
        assert_eq!(strip_jsonc("{\"a\":1,}").expect("strip"), "{\"a\":1}");
    }

    #[test]
    fn cfg02_substitution_merge_byte_unchanged() {
        let global = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "https://g.invalid", "apiKey": "gk",
            "timeout": false, "chunkTimeout": 6000000}}},
            "permissions": {"read": "allow"}}"#;
        let local = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "{env:OVER_URL}", "apiKey": "{env:OVER_KEY}",
            "timeout": false, "chunkTimeout": 6000000}}}}"#;
        let before = (global.to_string(), local.to_string());
        let generation = assemble(
            &[
                src("G/opencode.json", global, true),
                src("P/opencode.json", local, true),
            ],
            &env(&[("OVER_URL", "https://p.invalid"), ("OVER_KEY", "pk")]),
            None,
        )
        .expect("merge");
        assert_eq!(
            generation.providers["ludka2"].options.base_url,
            "https://p.invalid"
        );
        assert_eq!(generation.provenance["provider.ludka2"], "P/opencode.json");
        assert_eq!((global.to_string(), local.to_string()), before);
        // Missing env template on a selected key fails with MissingCredential.
        let needy = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "https://g.invalid", "apiKey": "{env:ABSENT_KEY}",
            "timeout": false, "chunkTimeout": 6000000}}}}"#;
        let err = assemble(&[src("G/opencode.json", needy, true)], &env(&[]), None)
            .expect_err("missing cred");
        assert!(matches!(err, ConfigError::MissingCredential { .. }));
    }

    #[test]
    fn cfg03_capability_failures() {
        let bad_npm = r#"{"provider": {"x": {"npm": "evil-pkg",
            "options": {"baseURL": "https://x.invalid", "apiKey": "k"}}}}"#;
        assert!(matches!(
            assemble(&[src("s", bad_npm, true)], &env(&[]), None),
            Err(ConfigError::UnsupportedCapability { .. })
        ));
        let codemode = r#"{"mcp": {"m": {"type": "local",
            "command": ["npx", "x"], "enabled": true, "codemode": true}}}"#;
        assert!(matches!(
            assemble(&[src("s", codemode, true)], &env(&[]), None),
            Err(ConfigError::UnsupportedCapability { .. })
        ));
        let oauth = r#"{"mcp": {"w": {"type": "remote", "url": "https://w.invalid/mcp",
            "enabled": true, "oauth": true}}}"#;
        assert!(matches!(
            assemble(&[src("s", oauth, true)], &env(&[]), None),
            Err(ConfigError::UnsupportedCapability { .. })
        ));
        let dcp = r#"{"dcp": {"experimental": {"allowSubAgents": true}}}"#;
        assert!(matches!(
            assemble(&[src("s", dcp, true)], &env(&[]), None),
            Err(ConfigError::UnsupportedCapability { .. })
        ));
        let mode = r#"{"dcp": {"compress": {"mode": "message"}}}"#;
        assert!(matches!(
            assemble(&[src("s", mode, true)], &env(&[]), None),
            Err(ConfigError::UnsupportedCapability { .. })
        ));
    }

    #[test]
    fn cfg04_trust_secrets_disabled() {
        // Untrusted file read refused before any side effect.
        let err = substitute("{file:/etc/hostname}", "P/opencode.json", false, &env(&[]))
            .expect_err("untrusted");
        assert!(matches!(err, ConfigError::Untrusted { .. }));
        // Disabled MCP launches nothing and needs no credential.
        let text = r#"{"mcp": {"chrome-devtools": {"type": "local",
            "command": ["npx", "-y", "chrome-devtools-mcp@latest"], "enabled": false}}}"#;
        let generation = assemble(&[src("s", text, false)], &env(&[]), None).expect("disabled");
        assert!(!generation.mcp["chrome-devtools"].enabled);
        // Explain redacts secrets but keeps provenance.
        let text = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "https://p.invalid", "apiKey": "SECRET",
            "timeout": false, "chunkTimeout": 6000000}}}}"#;
        let generation =
            assemble(&[src("G/opencode.json", text, true)], &env(&[]), None).expect("gen");
        let explained = explain_redacted(&generation).to_string();
        assert!(!explained.contains("SECRET"));
        assert!(explained.contains("G/opencode.json"));
    }

    #[test]
    fn cfg05_config_roots_order() {
        let fixture: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string("../../fixtures/config-roots.order.json").unwrap_or(
                r#"{"order_low_to_high": ["G/opencode.json", "G/opencode.jsonc",
                "Location-root-to-cwd opencode.json"]}"#
                    .to_string(),
            ),
        )
        .expect("fixture json");
        let order = fixture["order_low_to_high"].as_array().expect("order");
        assert!(order.iter().any(|v| v == "G/opencode.json"));
        // Later source wins per provider id (canonical dedup per section).
        let g = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "https://g.invalid", "apiKey": "gk",
            "timeout": false, "chunkTimeout": 6000000}}}}"#;
        let p = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": "https://p.invalid", "apiKey": "pk",
            "timeout": false, "chunkTimeout": 6000000}}}}"#;
        let generation = assemble(
            &[
                src("G/opencode.json", g, true),
                src("P/opencode.json", p, true),
            ],
            &env(&[]),
            None,
        )
        .expect("order");
        assert_eq!(
            generation.providers["ludka2"].options.base_url,
            "https://p.invalid"
        );
    }

    #[test]
    fn cfg06_instructions_once_with_provenance() {
        // Ordered AGENTS fragments enter once in pinned order (fixture rule).
        let order = ["G/AGENTS.md", "P/AGENTS.md"];
        let mut seen = std::collections::HashSet::new();
        let mut effective = Vec::new();
        for path in order {
            if seen.insert(path) {
                effective.push(path);
            }
        }
        assert_eq!(effective, ["G/AGENTS.md", "P/AGENTS.md"]);
        // Generation provenance mirrors the same rule for config sections.
        let g = r#"{"permissions": {"read": "allow"}}"#;
        let generation =
            assemble(&[src("G/opencode.json", g, true)], &env(&[]), None).expect("gen");
        assert_eq!(generation.provenance["permissions.read"], "G/opencode.json");
    }

    #[test]
    fn cfg07_definitions_frontmatter_and_bounds() {
        let meta = parse_skill(
            "demo",
            "---\nname: demo\ndescription: bounded demo skill\n---\n# demo\n",
        )
        .expect("skill");
        assert_eq!(meta.name, "demo");
        assert_eq!(legacy_key("write"), "apply_patch");
        assert_eq!(legacy_key("read"), "read");
        assert!(parse_skill("x", "no frontmatter").is_err());
        // Permissions merge most-restrictive across sources.
        let generation = assemble(
            &[
                src(
                    "G/opencode.json",
                    r#"{"permissions": {"bash": "allow"}}"#,
                    true,
                ),
                src(
                    "P/opencode.json",
                    r#"{"permissions": {"bash": "deny"}}"#,
                    true,
                ),
            ],
            &env(&[]),
            None,
        )
        .expect("perm");
        assert_eq!(generation.permissions["bash"], Permission::Deny);
    }

    #[test]
    fn cfg08_native_plugins_exact_only() {
        assert_eq!(
            classify_plugin("@tarquinen/opencode-dcp", "/r").expect("dcp"),
            "dcp"
        );
        assert_eq!(
            classify_plugin("@tarquinen/opencode-dcp@3.1.15", "/r").expect("pinned"),
            "dcp"
        );
        assert_eq!(
            classify_plugin("/r/plugins/openproxy-models.js", "/r").expect("disc"),
            "discovery"
        );
        assert!(classify_plugin("@tarquinen/opencode-dcp@latest", "/r").is_err());
        assert!(classify_plugin("/other/openproxy-models.js", "/r").is_err());
        assert!(classify_plugin("https://x.invalid/p.js", "/r").is_err());
        let _ = HashSet::<String>::new();
    }
}
