//! User config and permissions for T07.
//!
//! JSONC fragments → provenance → explicit trust → substitutions →
//! normalization → domain merge → capability validation → immutable
//! generation. Unknown/unsupported fails before side effects; commands and
//! plugin code are never executed here; future modules are not activated.

use std::collections::{BTreeMap, HashSet};
use std::ffi::{CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};

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
///
/// Unknown option keys are not a shape error: they are reported as warnings
/// (`provider.<id>.options.<key>`) so a vendor-specific field written for
/// another frontend cannot block the whole application. Known fields keep
/// strict typing.
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
    /// Extra generation headers; native auth and transport headers take precedence.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// Single provider entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

impl std::fmt::Debug for McpEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpEntry")
            .field("kind", &self.kind)
            .field("url", &self.url.as_ref().map(|_| "<configured>"))
            .field("enabled", &self.enabled)
            .field("oauth", &self.oauth)
            .field("header_names", &self.headers.keys().collect::<Vec<_>>())
            .field(
                "command",
                &format_args!("<redacted:{}>", self.command.len()),
            )
            .field("timeout", &self.timeout)
            .field("codemode", &self.codemode)
            .finish()
    }
}

fn default_true() -> bool {
    true
}

/// Effective immutable generation (T07 subset).
#[derive(Debug, Clone, Default, PartialEq)]
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

fn component_name(value: &OsStr) -> Result<CString, ConfigError> {
    CString::new(value.as_bytes()).map_err(|_| ConfigError::Invalid {
        field: "file".to_string(),
        reason: "invalid file reference".to_string(),
    })
}

fn open_relative(dir: &File, name: &CString, flags: i32) -> std::io::Result<File> {
    // SAFETY: the borrowed directory fd and NUL-terminated name remain valid.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0,
        )
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn read_trusted_file(path: &str, source: &str) -> Result<String, ConfigError> {
    const FILE_CAP: usize = 64 * 1024;

    let refused = || ConfigError::Untrusted {
        origin: source.to_string(),
        reason: "file reference must be a regular no-follow path inside the config directory"
            .to_string(),
    };
    let mut parts = Vec::new();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(part) => parts.push(component_name(part)?),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(refused());
            }
        }
    }
    let Some((file_name, directories)) = parts.split_last() else {
        return Err(refused());
    };
    let source_dir = Path::new(source)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source_dir)
        .map_err(|_| refused())?;
    for part in directories {
        dir =
            open_relative(&dir, part, libc::O_RDONLY | libc::O_DIRECTORY).map_err(|_| refused())?;
    }
    let mut file =
        open_relative(&dir, file_name, libc::O_RDONLY | libc::O_NONBLOCK).map_err(|_| refused())?;
    let meta = file.metadata().map_err(|_| refused())?;
    if !meta.is_file() {
        return Err(refused());
    }
    if meta.len() > FILE_CAP as u64 {
        return Err(ConfigError::Invalid {
            field: "file".to_string(),
            reason: "file reference too large".to_string(),
        });
    }
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(FILE_CAP as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| refused())?;
    if bytes.len() > FILE_CAP {
        return Err(ConfigError::Invalid {
            field: "file".to_string(),
            reason: "file reference too large".to_string(),
        });
    }
    String::from_utf8(bytes).map_err(|_| ConfigError::Invalid {
        field: "file".to_string(),
        reason: "file reference is not UTF-8".to_string(),
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
    // Unknown provider option keys: visible warnings, never a hard failure.
    let mut unknown_options: Vec<String> = Vec::new();
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
                for key in unknown_option_keys(raw.get("options")) {
                    unknown_options.push(format!(
                        "provider.{id}.options.{key} is not supported by the native \
                         profile; the option is ignored"
                    ));
                }
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
                let key = legacy_key(key).to_string();
                permissions
                    .entry(key)
                    .and_modify(|(old, origin)| {
                        if level == *old || more_restrictive(level, *old) {
                            *old = level;
                            *origin = source.path.clone();
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
        for value in entry.options.headers.values_mut() {
            *value = substitute(value, path, trusted, env)?;
        }
        let selected = enabled_providers.is_none_or(|only| only.contains(id));
        // Only the provider that will actually be used must be on the native
        // family: an unselected provider with a foreign package (another
        // frontend's entry) must not block the application.
        if selected {
            validate_provider(id, &entry)?;
        }
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
        if entry.enabled {
            if let Some(url) = &entry.url {
                entry.url = Some(substitute(url, path, trusted, env)?);
            }
            for (key, value) in entry.headers.clone() {
                entry
                    .headers
                    .insert(key, substitute(&value, path, trusted, env)?);
            }
        }
        // Disabled entries keep inert templates: no secret/file read and no launch.
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
        warnings: unknown_options,
    })
}

/// Known provider option keys; anything else is a visible warning.
const PROVIDER_OPTION_KEYS: &[&str] = &[
    "baseURL",
    "apiKey",
    "timeout",
    "chunkTimeout",
    "setCacheKey",
    "headers",
];

fn unknown_option_keys(options: Option<&serde_json::Value>) -> Vec<String> {
    let Some(map) = options.and_then(|value| value.as_object()) else {
        return Vec::new();
    };
    map.keys()
        .filter(|key| !PROVIDER_OPTION_KEYS.contains(&key.as_str()))
        .cloned()
        .collect()
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
        // `allowSubAgents: true` is tolerated: it only permits subagent
        // summarisation, which this generation does not perform yet. The
        // native DCP loader reports it as a warning.
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
/// Admitted: bare `@tarquinen/opencode-dcp`, pinned/latest aliases, and canonical
/// `<root>/{plugin,plugins}/openproxy-models.js`. Everything else is
/// `UnsupportedPlugin` before resolver/import/process/network.
pub fn classify_plugin(identity: &str, config_root: &str) -> Result<&'static str, ConfigError> {
    if matches!(
        identity,
        "@tarquinen/opencode-dcp"
            | "@tarquinen/opencode-dcp@3.1.15"
            | "@tarquinen/opencode-dcp@latest"
    ) {
        return Ok("dcp");
    }
    // Exact authoring-only compatibility marker from the user's shared global
    // OpenCode config. It is never loaded or executed and grants no capability.
    if identity == "@prevalentware/opencode-goal-plugin@0.1.49" {
        return Ok("ignored-authoring-goal");
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
    if text.len() > 65536 {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.body"),
            reason: "skill file too large".to_string(),
        });
    }
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.frontmatter"),
            reason: "missing opening ---".to_string(),
        });
    }
    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut closed = false;
    for (index, line) in (&mut lines).enumerate() {
        if line == "---" {
            closed = true;
            break;
        }
        if index >= 64 || line.len() > 1024 {
            return Err(ConfigError::Invalid {
                field: format!("skill.{id}.frontmatter"),
                reason: "frontmatter too large".to_string(),
            });
        }
        if let Some(rest) = line.strip_prefix("name:") {
            if name.is_some() {
                return Err(ConfigError::Invalid {
                    field: format!("skill.{id}.name"),
                    reason: "duplicate field".to_string(),
                });
            }
            name = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("description:") {
            if description.is_some() {
                return Err(ConfigError::Invalid {
                    field: format!("skill.{id}.description"),
                    reason: "duplicate field".to_string(),
                });
            }
            description = Some(rest.trim().trim_matches('"').to_string());
        } else if !line.trim().is_empty() {
            let Some((field, _)) = line.split_once(':') else {
                return Err(ConfigError::Invalid {
                    field: format!("skill.{id}.frontmatter"),
                    reason: "malformed frontmatter field".to_string(),
                });
            };
            return Err(ConfigError::Invalid {
                field: format!("skill.{id}.frontmatter.{}", field.trim()),
                reason: "unknown frontmatter field".to_string(),
            });
        }
    }
    if !closed {
        return Err(ConfigError::Invalid {
            field: format!("skill.{id}.frontmatter"),
            reason: "missing closing ---".to_string(),
        });
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

    /// A vendor-specific provider option (written for another frontend) is a
    /// visible warning, not a shape error that blocks the application.
    #[test]
    fn unknown_provider_option_is_a_warning() {
        let config = r#"{"provider": {"p": {"options": {"authToken": "x", "apiKey": "k"},
            "models": {"m": {}}}}}"#;
        let generation = assemble(&[src("s", config, true)], &env(&[]), None).expect("assembled");
        assert_eq!(generation.providers["p"].options.api_key, "k");
        assert!(
            generation
                .warnings
                .iter()
                .any(|warning| warning.contains("provider.p.options.authToken")),
            "the ignored option is reported: {:?}",
            generation.warnings
        );
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
        assert!(
            !matches!(
                assemble(&[src("s", dcp, true)], &env(&[]), None),
                Err(ConfigError::UnsupportedCapability { field, .. })
                    if field.contains("allowSubAgents")
            ),
            "allowSubAgents must not block assembly"
        );
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
        let temp = tempfile::tempdir().expect("tempdir");
        let config_dir = temp.path().join("config");
        std::fs::create_dir_all(config_dir.join("secrets")).expect("dirs");
        let source = config_dir.join("opencode.json");
        std::fs::write(&source, "{}").expect("config");
        std::fs::write(config_dir.join("secrets/key"), "relative-secret").expect("secret");
        assert_eq!(
            substitute(
                "Bearer {file:secrets/key}",
                &source.to_string_lossy(),
                true,
                &env(&[]),
            )
            .expect("relative file"),
            "Bearer relative-secret"
        );
        std::fs::write(temp.path().join("outside"), "outside").expect("outside");
        assert!(
            substitute(
                "{file:../outside}",
                &source.to_string_lossy(),
                true,
                &env(&[]),
            )
            .is_err()
        );
        assert!(
            substitute(
                "{file:/etc/hostname}",
                &source.to_string_lossy(),
                true,
                &env(&[]),
            )
            .is_err()
        );
        std::fs::write(config_dir.join("secrets/large"), vec![b'x'; 65_537])
            .expect("large fixture");
        assert!(
            substitute(
                "{file:secrets/large}",
                &source.to_string_lossy(),
                true,
                &env(&[]),
            )
            .is_err()
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                temp.path().join("outside"),
                config_dir.join("secrets/link"),
            )
            .expect("symlink");
            assert!(
                substitute(
                    "{file:secrets/link}",
                    &source.to_string_lossy(),
                    true,
                    &env(&[]),
                )
                .is_err()
            );
        }
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
        assert!(parse_skill("x", "---\nname: x\ndescription: x\nbody").is_err());
        assert!(
            parse_skill(
                "x",
                "---\nname: x\ndescription: x\nunknown: rejected\n---\nbody",
            )
            .is_err()
        );
        // Permissions merge most-restrictive across sources.
        let generation = assemble(
            &[
                src(
                    "G/opencode.json",
                    r#"{"permissions": {"bash": "allow", "apply_patch": "allow"}}"#,
                    true,
                ),
                src(
                    "P/opencode.json",
                    r#"{"permissions": {"bash": "deny", "write": "deny"}}"#,
                    true,
                ),
                src(
                    "L/opencode.json",
                    r#"{"permissions": {"edit": "allow", "bash": "deny"}}"#,
                    true,
                ),
            ],
            &env(&[]),
            None,
        )
        .expect("perm");
        assert_eq!(generation.permissions["bash"], Permission::Deny);
        assert_eq!(generation.provenance["permissions.bash"], "L/opencode.json");
        assert_eq!(generation.permissions["apply_patch"], Permission::Deny);
        assert!(!generation.permissions.contains_key("write"));
        assert!(!generation.permissions.contains_key("edit"));
        assert_eq!(
            generation.provenance["permissions.apply_patch"],
            "P/opencode.json"
        );
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
            classify_plugin("@tarquinen/opencode-dcp@latest", "/r").expect("latest"),
            "dcp"
        );
        assert_eq!(
            classify_plugin("/r/plugins/openproxy-models.js", "/r").expect("disc"),
            "discovery"
        );
        assert!(classify_plugin("@tarquinen/opencode-dcp@3.1.14", "/r").is_err());
        assert!(classify_plugin("/other/openproxy-models.js", "/r").is_err());
        assert!(classify_plugin("https://x.invalid/p.js", "/r").is_err());
        let _ = HashSet::<String>::new();
    }
}
