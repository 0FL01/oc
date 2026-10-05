//! User config and permissions for T07.
//!
//! JSONC fragments → provenance → explicit trust → substitutions →
//! normalization → domain merge → capability validation → immutable
//! generation. Unknown/unsupported fails before side effects; commands and
//! plugin code are never executed here; future modules are not activated.

use std::collections::{BTreeMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub(crate) mod diagnostic;
pub(crate) mod mcp;
pub use crate::tools::output::Limits as ToolOutputLimits;
pub use mcp::McpTimeouts;

#[cfg(test)]
#[path = "config/tool_output_tests.rs"]
mod tool_output_tests;

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
        /// Stable opaque requested identity, never raw package/URL/path text.
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
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderOptions {
    /// Base URL template (may contain `{env:..}` before substitution).
    #[serde(rename = "baseURL", default)]
    pub base_url: String,
    /// API key template (never logged).
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    /// Static Messages Bearer credential, not OAuth execution.
    #[serde(rename = "authToken", default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// Auth scheme captured after substitution into the common key slot.
    #[serde(skip)]
    pub messages_bearer: bool,
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
    /// Native admission caps for unknown model limits (never discovery metadata).
    #[serde(rename = "nativeFallbackLimits", default)]
    pub native_fallback_limits: crate::models::FallbackLimits,
}

impl std::fmt::Debug for ProviderOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderOptions")
            .field("base_url", &"[configured]")
            .field("credentials", &"[redacted]")
            .field("timeout", &self.timeout)
            .field("chunk_timeout", &self.chunk_timeout)
            .field("set_cache_key", &self.set_cache_key)
            .finish_non_exhaustive()
    }
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

/// Normalized MCP entry. Failed entries remain in the generation, but cannot launch.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
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
    /// Workspace-relative before resource admission, canonical afterwards.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Admitted configured overlay (values must never be logged).
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    /// Separate normalized startup/catalog/execution milliseconds.
    #[serde(default)]
    pub timeouts: McpTimeouts,
    /// This adapter only admits legacy initialize negotiation.
    #[serde(default = "mcp::legacy_protocol")]
    pub protocol: String,
    /// Typed payload-free failed inventory; no fallback to a previous entry.
    #[serde(skip)]
    pub failure: Option<oc_core::queries::ServiceDiagnostic>,
    /// Composition's source/credential-domain decision, never configurable.
    #[serde(skip)]
    pub inherit_credentials: bool,
    /// Values withheld from a lower-trust process even under innocuous env names.
    #[serde(skip)]
    pub blocked_inherited_values: Vec<String>,
    /// Existing resource authority admitted the exact canonical cwd.
    #[serde(skip)]
    pub resource_admitted: bool,
}

impl std::fmt::Debug for McpEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpEntry")
            .field(
                "kind",
                &if matches!(self.kind.as_str(), "local" | "remote") {
                    self.kind.as_str()
                } else {
                    "<invalid>"
                },
            )
            .field("url", &self.url.as_ref().map(|_| "<configured>"))
            .field("enabled", &self.enabled)
            .field("oauth", &self.oauth)
            .field(
                "headers",
                &format_args!("<redacted:{}>", self.headers.len()),
            )
            .field(
                "command",
                &format_args!("<redacted:{}>", self.command.len()),
            )
            .field("timeout", &self.timeout)
            .field("codemode", &self.codemode)
            .field("cwd", &self.cwd.as_ref().map(|_| "<configured>"))
            .field(
                "environment",
                &format_args!("<redacted:{}>", self.environment.len()),
            )
            .field("timeouts", &self.timeouts)
            .field(
                "protocol",
                &if self.protocol == "legacy" {
                    "legacy"
                } else {
                    "<unsupported>"
                },
            )
            .field("failure", &self.failure)
            .finish()
    }
}

fn default_true() -> bool {
    true
}

/// Keep leader references unresolved until every admitted config layer is merged.
pub(crate) struct ConversationKeybinds {
    fullscreen: String,
    exit: String,
    leader: String,
    undo: String,
    redo: String,
    timeout: Option<u64>,
    legacy_timeout: Option<u64>,
    palette: String,
}

impl Default for ConversationKeybinds {
    fn default() -> Self {
        Self {
            fullscreen: "ctrl+f".into(),
            exit: "ctrl+c,ctrl+d,<leader>q".into(),
            leader: "ctrl+x".into(),
            undo: "<leader>u".into(),
            redo: "<leader>r".into(),
            timeout: None,
            legacy_timeout: None,
            palette: "ctrl+p".into(),
        }
    }
}

impl ConversationKeybinds {
    pub(crate) fn merge(&mut self, value: &serde_json::Value) -> Result<(), ConfigError> {
        if let Some(leader) = value.get("leader")
            && !leader.is_object()
        {
            return Err(ConfigError::Invalid {
                field: "leader".into(),
                reason: "must be an object".into(),
            });
        }
        for (field, leaf, target) in [
            (
                "leader.timeout",
                value.pointer("/leader/timeout"),
                &mut self.timeout,
            ),
            (
                "leader_timeout",
                value.get("leader_timeout"),
                &mut self.legacy_timeout,
            ),
        ] {
            if let Some(leaf) = leaf {
                *target = Some(leaf.as_u64().filter(|ms| *ms > 0).ok_or_else(|| {
                    ConfigError::Invalid {
                        field: field.into(),
                        reason: "must be a positive integer".into(),
                    }
                })?);
            }
        }
        let Some(bindings) = value.get("keybinds") else {
            return Ok(());
        };
        let bindings = bindings.as_object().ok_or_else(|| ConfigError::Invalid {
            field: "keybinds".into(),
            reason: "must be an object".into(),
        })?;
        for (names, target) in [
            (&["leader"][..], &mut self.leader),
            (&["session_undo", "session.undo"][..], &mut self.undo),
            (&["session_redo", "session.redo"][..], &mut self.redo),
            (
                &["command_list", "command.palette.show"][..],
                &mut self.palette,
            ),
            (
                &[
                    "permission_prompt_fullscreen",
                    "permission.prompt.fullscreen",
                ][..],
                &mut self.fullscreen,
            ),
            (&["app_exit", "app.exit"][..], &mut self.exit),
        ] {
            for name in names {
                if let Some(value) = bindings.get(*name) {
                    *target = value
                        .as_str()
                        .or_else(|| (value.as_bool() == Some(false)).then_some("none"))
                        .ok_or_else(|| ConfigError::Invalid {
                            field: format!("keybinds.{name}"),
                            reason: "must be a string or false".into(),
                        })?
                        .to_string();
                }
            }
        }
        Ok(())
    }

    pub(crate) fn permission_shortcuts(&self) -> oc_core::queries::PermissionShortcuts {
        let resolved = Self {
            leader: self.leader.clone(),
            undo: self.fullscreen.clone(),
            redo: self.exit.clone(),
            fullscreen: String::new(),
            exit: String::new(),
            timeout: None,
            legacy_timeout: None,
            palette: String::new(),
        }
        .resolve();
        oc_core::queries::PermissionShortcuts {
            fullscreen: resolved.undo,
            exit: resolved.redo,
        }
    }
    pub(crate) fn leader_timeout_ms(&self) -> Option<u64> {
        self.timeout.or(self.legacy_timeout)
    }
    pub(crate) fn command_palette_shortcut(&self) -> String {
        Self {
            leader: self.leader.clone(),
            undo: self.palette.clone(),
            ..Self::default()
        }
        .resolve()
        .undo
    }
    pub(crate) fn resolve(self) -> oc_core::queries::ConversationShortcuts {
        let leaders: Vec<_> = self
            .leader
            .split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty() && *key != "none")
            .collect();
        let resolve = |binding: &str| {
            binding
                .split(',')
                .map(str::trim)
                .filter(|key| !key.is_empty() && *key != "none")
                .flat_map(|key| {
                    let key = key
                        .strip_prefix("leader+")
                        .map_or_else(|| key.to_string(), |suffix| format!("<leader>{suffix}"));
                    if key.contains("<leader>") {
                        leaders
                            .iter()
                            .map(|leader| key.replace("<leader>", &format!("{leader} ")))
                            .collect::<Vec<_>>()
                    } else {
                        vec![key]
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        oc_core::queries::ConversationShortcuts {
            leader: leaders.join(","),
            undo: resolve(&self.undo),
            redo: resolve(&self.redo),
        }
    }
}

/// Effective immutable generation (T07 subset).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Generation {
    pub tool_output: ToolOutputLimits,
    pub config_diagnostics: Vec<oc_core::queries::ConfigDiagnostic>,
    pub compaction: crate::compaction::CompactionConfig,
    /// Explicit animation preference from the ordered config sources.
    pub animations: Option<bool>,
    /// Providers by id in sorted order.
    pub providers: BTreeMap<String, ProviderEntry>,
    /// MCP entries by id in sorted order.
    pub mcp: BTreeMap<String, McpEntry>,
    /// Legacy scalar policy/summary; runtime also requires `permission_rules`.
    pub permissions: BTreeMap<String, Permission>,
    /// Ordered resource-aware authority; `permissions` is a legacy summary.
    pub permission_rules: crate::permissions::PermissionRules,
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
            cwd: None,
            environment: BTreeMap::new(),
            timeouts: McpTimeouts::default(),
            protocol: mcp::legacy_protocol(),
            failure: None,
            inherit_credentials: false,
            blocked_inherited_values: Vec::new(),
            resource_admitted: false,
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
    substitute_with(template, source, trusted, env, &read_trusted_file)
}

fn substitute_with(
    template: &str,
    source: &str,
    trusted: bool,
    env: &BTreeMap<String, String>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
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
                out.push_str(&reader(path, source)?);
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
    // Public callers supply their own trust decision, without an admitted root.
    // Independently verify every source-directory ancestor (no symlinks), then
    // pin that exact directory for the entire reference read. Never fall back
    // from an admitted-root failure to this path-based consumer.
    let source_dir = Path::new(source)
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let (anchor, relative) = if source_dir.is_absolute() {
        (
            Path::new("/"),
            source_dir.strip_prefix("/").expect("absolute"),
        )
    } else {
        (Path::new("."), source_dir)
    };
    let refused = || file_refused(source);
    let anchor = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(anchor)
        .map_err(|_| refused())?;
    let source_dir = crate::admitted_fs::open_beneath_no_symlinks(
        &anchor,
        relative,
        libc::O_RDONLY | libc::O_DIRECTORY,
    )
    .map_err(|_| refused())?;
    read_trusted_file_rooted(path, source, &source_dir, Path::new(""))
}

fn file_refused(source: &str) -> ConfigError {
    ConfigError::Untrusted {
        origin: source.to_string(),
        reason: "file reference must be a regular no-follow path inside the config directory"
            .to_string(),
    }
}

pub(crate) fn read_trusted_file_rooted(
    path: &str,
    source: &str,
    root: &File,
    source_directory: &Path,
) -> Result<String, ConfigError> {
    const FILE_CAP: usize = 64 * 1024;

    let refused = || file_refused(source);
    let mut relative = PathBuf::from(source_directory);
    let mut has_name = false;
    for part in Path::new(path).components() {
        match part {
            Component::Normal(part) => {
                relative.push(part);
                has_name = true;
            }
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(refused());
            }
        }
    }
    if !has_name {
        return Err(refused());
    }
    let mut file = crate::admitted_fs::open_beneath_no_symlinks(root, &relative, libc::O_RDONLY)
        .map_err(|_| refused())?;
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
    assemble_with_reader(sources, env, enabled_providers, &read_trusted_file)
        .map(|(generation, _)| generation)
}

/// Application-only substitution authority. Every admitted source is paired
/// with its pinned root and the source's canonical directory relative to it.
/// A missing entry fails closed; the public Source trust bit is not a directory
/// capability and cannot expand the application's read boundary. Return the
/// generation and presentation choice from one pinned source traversal.
pub(crate) fn assemble_admitted_with_terminal_copy(
    sources: &[Source],
    env: &BTreeMap<String, String>,
    enabled_providers: Option<&HashSet<String>>,
    roots: &BTreeMap<String, (&File, PathBuf)>,
) -> Result<
    (Generation, Option<oc_core::queries::TerminalCopyMode>),
    oc_core::queries::ServiceDiagnostic,
> {
    assemble_with_admission(
        sources,
        env,
        enabled_providers,
        &|path, source| {
            let (root, directory) = roots.get(source).ok_or_else(|| file_refused(source))?;
            read_trusted_file_rooted(path, source, root, directory)
        },
        false,
        false,
    )
    .map_err(|error| error.diagnostic)
}

fn assemble_with_reader(
    sources: &[Source],
    env: &BTreeMap<String, String>,
    enabled_providers: Option<&HashSet<String>>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
) -> Result<(Generation, Option<oc_core::queries::TerminalCopyMode>), ConfigError> {
    assemble_with_admission(sources, env, enabled_providers, reader, true, false)
        .map_err(|error| *error.error)
}

/// Catalog admission shares normalization/policy and pinned substitution authority,
/// but resolves only the dynamic catalog binding and leaves MCP templates inert.
pub(crate) fn assemble_catalog_admitted(
    sources: &[Source],
    env: &BTreeMap<String, String>,
    dynamic: &HashSet<String>,
    roots: &BTreeMap<String, (&File, PathBuf)>,
) -> Result<Generation, oc_core::queries::ServiceDiagnostic> {
    assemble_with_admission(
        sources,
        env,
        Some(dynamic),
        &|path, source| {
            let (root, directory) = roots.get(source).ok_or_else(|| file_refused(source))?;
            read_trusted_file_rooted(path, source, root, directory)
        },
        false,
        true,
    )
    .map(|(generation, _)| generation)
    .map_err(|error| error.diagnostic)
}

fn assemble_with_admission(
    sources: &[Source],
    env: &BTreeMap<String, String>,
    enabled_providers: Option<&HashSet<String>>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
    require_credential: bool,
    catalog_only: bool,
) -> Result<(Generation, Option<oc_core::queries::TerminalCopyMode>), diagnostic::LocatedError> {
    let mut providers: BTreeMap<String, (ProviderEntry, String)> = BTreeMap::new();
    // Unknown provider option keys: visible warnings, never a hard failure.
    let mut unknown_options: Vec<String> = Vec::new();
    let mut mcp: BTreeMap<String, (McpEntry, String)> = BTreeMap::new();
    let mut mcp_timeout = McpTimeouts::default();
    let mut mcp_provenance = BTreeMap::new();
    let mut permissions: BTreeMap<String, (Permission, String)> = BTreeMap::new();
    let mut permission_rules = crate::permissions::PermissionRules::default();
    let mut terminal_copy_source = None;
    let mut terminal_copy = None;
    let mut animations_source = None;
    let mut animations = None;
    let mut compaction = crate::compaction::CompactionConfig::default();
    let mut compaction_source = None;
    let mut tool_output = ToolOutputLimits::default();
    let mut tool_output_source = None;
    let mut config_diagnostics = Vec::new();
    let mut compaction_provenance = BTreeMap::new();

    for source in sources {
        let value =
            parse_jsonc(&source.text, &source.path).map_err(|error| diagnostic::LocatedError {
                error: Box::new(error),
                diagnostic: diagnostic::failure(
                    &source.path,
                    &["document"],
                    oc_core::queries::ServiceStage::Config,
                    oc_core::queries::ServiceCode::InvalidDocument,
                    oc_core::queries::ServiceAction::ReviewConfiguration,
                ),
            })?;
        (|| -> Result<(), ConfigError> {
        let obj = value.as_object().ok_or_else(|| ConfigError::Invalid {
            field: source.path.clone(),
            reason: "root must be an object".to_string(),
        })?;

        for field in ["snapshot", "snapshots"] {
            if let Some(value) = obj.get(field) {
                let enabled = value.as_bool().ok_or_else(|| ConfigError::Invalid {
                    field: field.to_string(),
                    reason: "must be a boolean".to_string(),
                })?;
                if enabled {
                    return Err(ConfigError::UnsupportedCapability {
                        field: field.to_string(),
                        reason: "filesystem snapshots are unsupported; conversation undo does not change files".to_string(),
                    });
                }
            }
        }
        if let Some(mode) = terminal_copy_value(obj)? {
            terminal_copy_source = Some(source.path.clone());
            terminal_copy = Some(mode);
        }
        if let Some(value) = obj.get("animations") {
            animations = Some(value.as_bool().ok_or_else(|| ConfigError::Invalid {
                field: "animations".to_string(),
                reason: "must be a boolean".to_string(),
            })?);
            animations_source = Some(source.path.clone());
        }
        if let Some(value) = obj.get("tool_output") {
            tool_output = ToolOutputLimits::parse(value)?;
            tool_output_source = Some(source.path.clone());
        }
        if let Some(value) = obj.get("compaction") {
            let (normalized, mut notes) = compaction.merge_from(&source.path, value);
            if !require_credential {
                for note in &mut notes { note.source = mcp::safe_source_id(&note.source); }
            }
            config_diagnostics.extend(notes);
            if let Some(normalized) = normalized {
                compaction_source = Some(source.path.clone());
                for (field, pointer) in [
                    ("auto", "/auto"),
                    ("keep.tokens", "/keep/tokens"),
                    ("buffer", "/buffer"),
                ] {
                    if normalized.pointer(pointer).is_some() {
                        compaction_provenance
                            .insert(format!("compaction.{field}"), source.path.clone());
                    }
                }
            }
        }

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
                    unknown_options.push(if require_credential { format!(
                        "provider.{id}.options.{key} is not supported by the native \
                         profile; the option is ignored"
                    ) } else {
                        diagnostic::failure(&source.path, &["provider", "entry", "options", "entry"], oc_core::queries::ServiceStage::Config, oc_core::queries::ServiceCode::IgnoredSetting, oc_core::queries::ServiceAction::ReviewConfiguration).to_string()
                    });
                }
                providers.insert(id.clone(), (entry, source.path.clone()));
            }
        }

        mcp::merge_document(source, obj, &mut mcp, &mut mcp_timeout, &mut mcp_provenance)?;

        let mut source_rules = crate::permissions::PermissionRules::from_config(&value)?;
        if let Some(home) = env.get("HOME") {
            source_rules.expand_home(home);
        }
        permission_rules.extend(source_rules);
        if let Some(perm_raw) = obj.get("permissions").filter(|raw| raw.is_object()) {
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
        Ok(())
        })().map_err(|error| diagnostic::LocatedError::new(&source.path, error))?;
    }

    // Substitute + selected-only credential check (no network/process here).
    let mut out_providers = BTreeMap::new();
    let mut provenance = BTreeMap::new();
    for (id, (entry, path)) in &providers {
        if catalog_only {
            if package_protocol(id, entry.npm.as_deref()).is_err() {
                // An inert foreign protocol never gains a catalog binding or
                // permission to resolve its endpoint/credential templates.
                continue;
            }
            validate_provider(id, entry)
                .map_err(|error| diagnostic::LocatedError::new(path, error))?;
        }
        if let Some(only) = enabled_providers
            && !only.contains(id)
        {
            if !require_credential && !entry.models.is_empty() {
                // Inert, shape-admitted public metadata in the existing generation.
                // Never resolve unselected credentials/endpoints or admit routing.
                out_providers.insert(
                    id.clone(),
                    ProviderEntry {
                        npm: entry.npm.clone(),
                        name: entry.name.clone(),
                        options: ProviderOptions::default(),
                        models: entry.models.clone(),
                    },
                );
                provenance.insert(format!("provider.{id}"), path.clone());
            }
            continue;
        }
        let trusted = sources.iter().any(|s| s.path == *path && s.trusted);
        let mut entry = entry.clone();
        (|| -> Result<(), ConfigError> {
            entry.options.base_url =
                substitute_with(&entry.options.base_url, path, trusted, env, reader)?;
            entry.options.api_key =
                substitute_with(&entry.options.api_key, path, trusted, env, reader)?;
            if let Some(token) = &mut entry.options.auth_token {
                *token = substitute_with(token, path, trusted, env, reader)?;
            }
            for value in entry.options.headers.values_mut() {
                *value = substitute_with(value, path, trusted, env, reader)?;
            }
            let selected = enabled_providers.is_none_or(|only| only.contains(id));
            // Only the provider that will actually be used must be on the native
            // family: an unselected provider with a foreign package (another
            // frontend's entry) must not block the application.
            if selected {
                validate_provider(id, &entry)?;
            }
            if let Some(token) = entry.options.auth_token.take() {
                entry.options.api_key = token;
                entry.options.messages_bearer = true;
            }
            if require_credential && selected && entry.options.api_key.trim().is_empty() {
                return Err(ConfigError::MissingCredential {
                    field: format!("provider.{id}.options.apiKey"),
                });
            }
            Ok(())
        })()
        .map_err(|error| diagnostic::LocatedError::new(path, error))?;
        out_providers.insert(id.clone(), entry);
        provenance.insert(format!("provider.{id}"), path.clone());
    }

    let mut out_mcp = BTreeMap::new();
    for (id, (entry, path)) in &mcp {
        let trusted = sources.iter().any(|s| s.path == *path && s.trusted);
        let mut entry = entry.clone();
        entry.timeouts = mcp_timeout.overlay(entry.timeouts);
        if !catalog_only && entry.enabled && entry.failure.is_none() {
            activate_mcp_entry(id, path, trusted, &mut entry, env, reader)
                .map_err(|error| diagnostic::LocatedError::new(path, error))?;
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
    if let Some(path) = terminal_copy_source {
        provenance.insert("terminal.copy".to_string(), path);
    }
    if let Some(path) = animations_source {
        provenance.insert("animations".to_string(), path);
    }
    if let Some(path) = compaction_source {
        provenance.insert("compaction".into(), path);
    }
    provenance.extend(compaction_provenance);
    provenance.extend(mcp_provenance);
    if let Some(source) = tool_output_source {
        provenance.insert("tool_output".into(), source);
    }

    Ok((
        Generation {
            tool_output,
            config_diagnostics,
            compaction,
            animations,
            providers: out_providers,
            mcp: out_mcp,
            permissions: out_perm,
            permission_rules,
            provenance,
            warnings: unknown_options,
        },
        terminal_copy,
    ))
}

fn terminal_copy_value(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<oc_core::queries::TerminalCopyMode>, ConfigError> {
    use oc_core::queries::TerminalCopyMode;
    let Some(terminal) = obj.get("terminal") else {
        return Ok(None);
    };
    let terminal = terminal.as_object().ok_or_else(|| ConfigError::Invalid {
        field: "terminal".to_string(),
        reason: "must be an object".to_string(),
    })?;
    let Some(copy) = terminal.get("copy") else {
        return Ok(None);
    };
    match copy.as_str() {
        Some("select") => Ok(Some(TerminalCopyMode::Select)),
        Some("manual") => Ok(Some(TerminalCopyMode::Manual)),
        _ => Err(ConfigError::Invalid {
            field: "terminal.copy".to_string(),
            reason: "must be select or manual".to_string(),
        }),
    }
}

/// One entry's effect-free trust/substitution/effective validation path. Used
/// both at startup and explicit activation of an inert disabled template.
pub(crate) fn activate_mcp_entry(
    id: &str,
    path: &str,
    trusted: bool,
    entry: &mut McpEntry,
    env: &BTreeMap<String, String>,
    reader: &impl Fn(&str, &str) -> Result<String, ConfigError>,
) -> Result<(), ConfigError> {
    if !trusted {
        return Err(ConfigError::Untrusted {
            origin: path.into(),
            reason: "MCP endpoint/command before source trust".into(),
        });
    }
    entry.enabled = true;
    if let Some(url) = &entry.url {
        entry.url = Some(substitute_with(url, path, trusted, env, reader)?);
    }
    for value in entry
        .headers
        .values_mut()
        .chain(entry.environment.values_mut())
    {
        *value = substitute_with(value, path, trusted, env, reader)?;
    }
    for arg in &mut entry.command {
        *arg = substitute_with(arg, path, trusted, env, reader)?;
    }
    if let Some(cwd) = &mut entry.cwd {
        *cwd = substitute_with(cwd, path, trusted, env, reader)?;
    }
    mcp::validate_effective(id, path, entry);
    Ok(())
}

/// Known provider option keys; anything else is a visible warning.
const PROVIDER_OPTION_KEYS: &[&str] = &[
    "baseURL",
    "apiKey",
    "authToken",
    "timeout",
    "chunkTimeout",
    "setCacheKey",
    "headers",
    "nativeFallbackLimits",
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
    let fallback = entry.options.native_fallback_limits;
    if fallback
        .output
        .get()
        .saturating_add(crate::models::SAFETY_MARGIN)
        >= fallback.context.get()
    {
        return Err(ConfigError::Invalid {
            field: format!("provider.{id}.options.nativeFallbackLimits"),
            reason: "context must exceed output plus the 1024-token safety margin".to_string(),
        });
    }
    provider_wire(id, entry)?;
    Ok(())
}

/// T53 finite package → wire mapping. Omitted package keeps the existing
/// Responses default; an explicit unknown (or not yet admitted) package is
/// unsupported before effects. Compatible aliases never imply a fallback.
pub(crate) fn package_protocol(
    id: &str,
    npm: Option<&str>,
) -> Result<crate::provider::protocol::Protocol, ConfigError> {
    use crate::provider::protocol::Protocol;
    match npm {
        None | Some("@ai-sdk/openai" | "@opencode/ai/providers/openai") => Ok(Protocol::Responses),
        Some("@ai-sdk/openai-compatible" | "@opencode/ai/providers/openai-compatible") => {
            Ok(Protocol::Chat)
        }
        Some("@ai-sdk/anthropic" | "@opencode/ai/providers/anthropic") => Ok(Protocol::Messages),
        Some(npm) => Err(ConfigError::UnsupportedCapability {
            field: format!("provider.{id}.npm"),
            reason: format!("unknown package {npm}"),
        }),
    }
}

/// Admitted wire binding with validated explicit per-model compatibility.
pub(crate) fn provider_wire(
    id: &str,
    entry: &ProviderEntry,
) -> Result<crate::provider::WireBinding, ConfigError> {
    use crate::provider::protocol::Protocol;
    let protocol = package_protocol(id, entry.npm.as_deref())?;
    if entry.options.auth_token.is_some()
        && (protocol != Protocol::Messages || !entry.options.api_key.is_empty())
    {
        return Err(ConfigError::Invalid {
            field: format!("provider.{id}.options.authToken"),
            reason: "requires Messages and cannot coexist with apiKey".into(),
        });
    }
    if protocol == Protocol::Messages
        && entry.options.headers.keys().any(|name| {
            name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
        })
    {
        return Err(ConfigError::Invalid {
            field: format!("provider.{id}.options.headers"),
            reason: "competing auth header".into(),
        });
    }
    let mut chat = BTreeMap::new();
    for (model, value) in &entry.models {
        let compat = chat_compat(&format!("provider.{id}.models.{model}"), value)?;
        if protocol == Protocol::Chat && compat != Default::default() {
            chat.insert(model.clone(), compat);
        }
    }
    Ok(match protocol {
        Protocol::Chat => crate::provider::WireBinding::chat(chat),
        Protocol::Messages => crate::provider::WireBinding::messages(
            entry.options.messages_bearer || entry.options.auth_token.is_some(),
        ),
        _ => Default::default(),
    })
}

/// OC2 `Model.Compatibility` subset with actual native semantics, plus the
/// legacy `interleaved` string/`{field}` → `reasoningField` migration.
fn chat_compat(
    field: &str,
    model: &serde_json::Value,
) -> Result<crate::provider::chat::ChatCompat, ConfigError> {
    let invalid = |name: &str, reason: &str| ConfigError::Invalid {
        field: format!("{field}.{name}"),
        reason: reason.to_string(),
    };
    let reasoning_field = |value: &serde_json::Value, name: &str| match value {
        serde_json::Value::String(field) if !field.is_empty() => Ok(Some(field.clone())),
        serde_json::Value::Object(object) => match object.get("field") {
            Some(serde_json::Value::String(field)) if !field.is_empty() => Ok(Some(field.clone())),
            _ => Err(invalid(name, "must name a field")),
        },
        serde_json::Value::Bool(_) => Ok(None),
        _ => Err(invalid(name, "must be a field name or {field}")),
    };
    let mut compat = crate::provider::chat::ChatCompat::default();
    if let Some(value) = model.get("interleaved") {
        compat.reasoning_field = reasoning_field(value, "interleaved")?;
    }
    let Some(object) = model.get("compatibility") else {
        return Ok(compat);
    };
    let object = object
        .as_object()
        .ok_or_else(|| invalid("compatibility", "must be an object"))?;
    for (key, value) in object {
        match key.as_str() {
            "reasoningField" => {
                compat.reasoning_field = reasoning_field(value, "compatibility.reasoningField")?
            }
            "maxTokensField" => {
                compat.max_completion_tokens = match value.as_str() {
                    Some("max_tokens") => false,
                    Some("max_completion_tokens") => true,
                    _ => {
                        return Err(invalid(
                            "compatibility.maxTokensField",
                            "must be max_tokens or max_completion_tokens",
                        ));
                    }
                }
            }
            "supportsPromptCacheKey" => {
                compat.supports_prompt_cache_key = value.as_bool().ok_or_else(|| {
                    invalid("compatibility.supportsPromptCacheKey", "must be boolean")
                })?
            }
            other => {
                return Err(ConfigError::UnsupportedCapability {
                    field: format!("{field}.compatibility.{other}"),
                    reason: "compatibility setting has no native semantics".to_string(),
                });
            }
        }
    }
    Ok(compat)
}

fn validate_dcp(raw: &serde_json::Value) -> Result<(), ConfigError> {
    if let Some(exp) = raw.get("experimental") {
        // Child compression availability is validated by the native DCP loader.
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

/// Legacy scalar summary for diagnostics and older callers. Runtime authority
/// MUST use `PermissionRules`, which retains the ordered resource maps.
///
/// Upstream `Rule = Union([Action, Record(String, Action)])` with a
/// `Record(String, Rule)` catch-all: a scalar `allow|ask|deny` is accepted
/// for any identifier key (unknown action names never fail loading), and a
/// glob→action map (the shape used for `external_directory`, `edit`,
/// `apply_patch`, `bash` and `webfetch`) folds to the most restrictive
/// level. An empty map keeps the conservative `ask`. Conflicting legacy
/// policies resolve conservative deny → ask → allow at the caller
/// (`assemble` keeps the most restrictive across sources).
pub fn normalize_permission(key: &str, raw: &serde_json::Value) -> Result<Permission, ConfigError> {
    match raw {
        serde_json::Value::String(text) => action_level(key, text),
        serde_json::Value::Object(rules) => {
            let mut level: Option<Permission> = None;
            for (glob, rule) in rules {
                let text = rule.as_str().ok_or_else(|| ConfigError::Invalid {
                    field: format!("permissions.{key}.{glob}"),
                    reason: "must be allow/ask/deny".to_string(),
                })?;
                let next = action_level(&format!("{key}.{glob}"), text)?;
                level = Some(match level {
                    Some(current) if !more_restrictive(next, current) => current,
                    _ => next,
                });
            }
            Ok(level.unwrap_or(Permission::Ask))
        }
        _ => Err(ConfigError::Invalid {
            field: format!("permissions.{key}"),
            reason: "must be allow/ask/deny".to_string(),
        }),
    }
}

fn action_level(key: &str, text: &str) -> Result<Permission, ConfigError> {
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
        "write" | "edit" | "patch" => "apply_patch",
        "shell" => "bash",
        // v1 `task` is the v2.0.12 `subagent` action.
        "task" => "subagent",
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
                "type": if matches!(entry.kind.as_str(),"remote"|"local") { entry.kind.as_str() } else { "<invalid>" },
                "url": entry.url.as_ref().map(|_| "***"),
                "enabled": entry.enabled,
                "headers": headers,
                "environment": entry.environment.keys().map(|key| (key.clone(), "***")).collect::<BTreeMap<_,_>>(),
                "cwd": entry.cwd.as_ref().map(|_| "***"),
                "timeout": entry.timeouts,
                "failure": entry.failure,
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
            | "@tarquinen/opencode-dcp@3.2.0"
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
        identity: safe_plugin_id(identity),
        reason: "unknown JS/TS/package identity".to_string(),
    })
}

pub(crate) fn safe_plugin_id(identity: &str) -> String {
    use sha2::{Digest as _, Sha256};
    format!("plugin-{:x}", Sha256::digest(identity.as_bytes()))
}

/// Skill frontmatter metadata (all fields optional, upstream parity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMeta {
    /// Directory id.
    pub id: String,
    /// Name (`id` when frontmatter omits it).
    pub name: String,
    /// Description (empty when frontmatter omits it).
    pub description: String,
    /// Donor `metadata."opencode/autoinvoke"`; `Some(false)` keeps the skill
    /// callable by id but out of the automatic preview.
    pub autoinvoke: Option<bool>,
}

/// One frontmatter line with its indentation width.
struct FrontmatterLine<'a> {
    indent: usize,
    content: &'a str,
}

/// Split Markdown frontmatter using the admitted YAML subset: nested
/// mappings, scalar sequences, quoted scalars and `#` comments (gray-matter +
/// js-yaml parity for the shapes the owner writes). A line whose first
/// non-space character is `#` is a comment, and a `#` preceded by whitespace
/// starts a trailing comment (`provider/model#variant` stays intact).
/// Duplicate mapping keys at any level are a parse error; unknown fields are
/// the caller's concern. Returns the parsed mapping (empty object when the
/// file has no frontmatter) and the remaining body.
pub(crate) fn split_frontmatter_value(text: &str) -> Result<(serde_json::Value, &str), String> {
    let after = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        Some(after) => after,
        None => return Ok((serde_json::Value::Object(serde_json::Map::new()), text)),
    };
    let mut offset = 0usize;
    let mut lines: Vec<FrontmatterLine<'_>> = Vec::new();
    let mut closed = false;
    for line in after.split_inclusive('\n') {
        let next_offset = offset + line.len();
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            offset = next_offset;
            closed = true;
            break;
        }
        offset = next_offset;
        let indent = trimmed.len() - trimmed.trim_start_matches(' ').len();
        let content = &trimmed[indent..];
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        if content.starts_with('\t') {
            return Err("unsupported frontmatter indentation".to_string());
        }
        lines.push(FrontmatterLine { indent, content });
    }
    if !closed {
        return Err("missing closing frontmatter ---".to_string());
    }
    let mut index = 0usize;
    let value = if lines.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        parse_frontmatter_block(&lines, &mut index, 0)?
    };
    if index < lines.len() {
        return Err("unsupported frontmatter indentation".to_string());
    }
    Ok((value, &after[offset..]))
}

/// Parse one indentation-delimited mapping or scalar sequence.
fn parse_frontmatter_block(
    lines: &[FrontmatterLine<'_>],
    index: &mut usize,
    indent: usize,
) -> Result<serde_json::Value, String> {
    let Some(first) = lines.get(*index) else {
        return Ok(serde_json::Value::Null);
    };
    if first.content == "-" || first.content.starts_with("- ") {
        let mut items = Vec::new();
        while let Some(line) = lines.get(*index) {
            if line.indent < indent {
                break;
            }
            if line.indent != indent {
                return Err("unsupported frontmatter indentation".to_string());
            }
            let item = line
                .content
                .strip_prefix('-')
                .ok_or_else(|| "unsupported frontmatter structure".to_string())?;
            items.push(parse_frontmatter_scalar(item.trim())?);
            *index += 1;
        }
        return Ok(serde_json::Value::Array(items));
    }
    let mut map = serde_json::Map::new();
    while let Some(line) = lines.get(*index) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err("unsupported frontmatter indentation".to_string());
        }
        let (raw_key, raw_value) = line
            .content
            .split_once(':')
            .ok_or_else(|| "malformed frontmatter field".to_string())?;
        let key = unquote_frontmatter_scalar(raw_key.trim())?;
        if key.is_empty() {
            return Err("malformed frontmatter field".to_string());
        }
        if map.contains_key(&key) {
            return Err(format!("duplicate frontmatter field {key}"));
        }
        let value_text = strip_frontmatter_comment(raw_value).trim().to_string();
        *index += 1;
        if value_text.is_empty() {
            let nested = lines
                .get(*index)
                .filter(|next| next.indent > indent)
                .map(|next| next.indent);
            match nested {
                Some(nested_indent) => {
                    map.insert(key, parse_frontmatter_block(lines, index, nested_indent)?);
                }
                None => {
                    map.insert(key, serde_json::Value::Null);
                }
            }
        } else {
            if lines.get(*index).is_some_and(|next| next.indent > indent) {
                return Err("unsupported nested frontmatter".to_string());
            }
            map.insert(key, parse_frontmatter_scalar(&value_text)?);
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// Strip a trailing `#` comment outside quotes (YAML comment rules).
fn strip_frontmatter_comment(value: &str) -> &str {
    let mut quote: Option<u8> = None;
    let mut preceded_by_space = true;
    for (index, byte) in value.bytes().enumerate() {
        match quote {
            Some(active) => {
                if byte == active {
                    quote = None;
                }
            }
            None => {
                if byte == b'"' || byte == b'\'' {
                    quote = Some(byte);
                } else if byte == b'#' && preceded_by_space {
                    return &value[..index];
                }
            }
        }
        preceded_by_space = byte == b' ' || byte == b'\t';
    }
    value
}

/// Parse a scalar into a JSON value; only `true`/`false`/`null` are typed.
fn parse_frontmatter_scalar(text: &str) -> Result<serde_json::Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return Ok(serde_json::Value::String(unquote_frontmatter_scalar(
            trimmed,
        )?));
    }
    match trimmed {
        "true" => Ok(serde_json::Value::Bool(true)),
        "false" => Ok(serde_json::Value::Bool(false)),
        "null" | "~" => Ok(serde_json::Value::Null),
        _ => Ok(frontmatter_number(trimmed)
            .unwrap_or_else(|| serde_json::Value::String(trimmed.to_string()))),
    }
}

/// YAML 1.2 core-schema decimal int/float for an unquoted plain scalar.
fn frontmatter_number(text: &str) -> Option<serde_json::Value> {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    let (mantissa, exponent) = match digits.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (digits, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let decimal = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() && fraction.is_empty()
        || !decimal(whole)
        || !decimal(fraction)
        || exponent.is_some_and(|exponent| {
            let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
            exponent.is_empty() || !decimal(exponent)
        })
    {
        return None;
    }
    if !mantissa.contains('.')
        && exponent.is_none()
        && let Ok(value) = text.parse::<i64>()
    {
        return Some(value.into());
    }
    let value = text.parse::<f64>().ok()?;
    serde_json::Number::from_f64(value).map(serde_json::Value::Number)
}

/// Remove matching surrounding quotes (YAML single/double quote escapes).
fn unquote_frontmatter_scalar(text: &str) -> Result<String, String> {
    let trimmed = text.trim();
    if let Some(inner) = trimmed
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                out.push(chars.next().unwrap_or('\\'));
            } else {
                out.push(c);
            }
        }
        return Ok(out);
    }
    if let Some(inner) = trimmed
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        return Ok(inner.replace("''", "'"));
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return Err("malformed frontmatter scalar".to_string());
    }
    Ok(trimmed.to_string())
}

/// Parse `SKILL.md` frontmatter without a YAML engine.
///
/// Upstream parity: every field is optional (`name?`, `description?`,
/// `metadata?`), unknown fields are silently ignored, and a file without
/// frontmatter still loads with the path id and no description. Body bytes
/// are never returned here (the native `skill` tool serves bounded snapshots
/// at call time).
pub fn parse_skill(id: &str, text: &str) -> Result<SkillMeta, ConfigError> {
    let (value, _body) = split_frontmatter_value(text).map_err(|reason| ConfigError::Invalid {
        field: format!("skill.{id}.frontmatter"),
        reason,
    })?;
    let object = value.as_object().ok_or_else(|| ConfigError::Invalid {
        field: format!("skill.{id}.frontmatter"),
        reason: "must be a mapping".to_string(),
    })?;
    let name = match object.get("name") {
        None | Some(serde_json::Value::Null) => id.to_string(),
        Some(serde_json::Value::String(name)) if name.trim().is_empty() => id.to_string(),
        Some(serde_json::Value::String(name)) => name.trim().to_string(),
        Some(_) => {
            return Err(ConfigError::Invalid {
                field: format!("skill.{id}.name"),
                reason: "must be a string".to_string(),
            });
        }
    };
    let description = match object.get("description") {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(description)) => description.trim().to_string(),
        Some(_) => {
            return Err(ConfigError::Invalid {
                field: format!("skill.{id}.description"),
                reason: "must be a string".to_string(),
            });
        }
    };
    // Pinned config/plugin/skill-file.ts metadataBoolean: boolean or a
    // case-insensitive "true"/"false" string; anything else is unset.
    let autoinvoke = object
        .get("metadata")
        .and_then(|metadata| metadata.get("opencode/autoinvoke"))
        .and_then(|value| match value {
            serde_json::Value::Bool(value) => Some(*value),
            serde_json::Value::String(value) => match value.trim().to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            _ => None,
        });
    Ok(SkillMeta {
        id: id.to_string(),
        name,
        description,
        autoinvoke,
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
#[path = "config/mcp_tests.rs"]
mod mcp_tests;
#[cfg(test)]
#[path = "config/plugin_tests.rs"]
mod plugin_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn leader_timeout_precedence_and_invalid_leaf_diagnostics() {
        use super::{ConfigError, ConversationKeybinds};
        let mut bindings = ConversationKeybinds::default();
        assert_eq!(bindings.leader_timeout_ms(), None);
        bindings
            .merge(&serde_json::json!({"leader_timeout": 500}))
            .unwrap();
        assert_eq!(bindings.leader_timeout_ms(), Some(500));
        bindings
            .merge(&serde_json::json!({"leader": {"timeout": 321}}))
            .unwrap();
        bindings
            .merge(&serde_json::json!({"leader_timeout": 900}))
            .unwrap();
        assert_eq!(bindings.leader_timeout_ms(), Some(321));
        for field in ["leader.timeout", "leader_timeout"] {
            for invalid in [
                serde_json::json!(0),
                serde_json::json!(-1),
                serde_json::json!(1.5),
                serde_json::json!("secret-fixture"),
                serde_json::json!(null),
            ] {
                let layer = if field == "leader.timeout" {
                    serde_json::json!({"leader": {"timeout": invalid}})
                } else {
                    serde_json::json!({"leader_timeout": invalid})
                };
                assert_eq!(
                    bindings.merge(&layer).unwrap_err(),
                    ConfigError::Invalid {
                        field: field.into(),
                        reason: "must be a positive integer".into(),
                    }
                );
            }
        }
    }

    #[test]
    fn conversation_shortcuts_resolve_final_leader_and_disabled_alternatives() {
        use oc_core::queries::ConversationShortcuts;
        assert_eq!(
            super::ConversationKeybinds::default().resolve(),
            ConversationShortcuts::default()
        );
        let mut bindings = super::ConversationKeybinds::default();
        bindings
            .merge(&serde_json::json!({"keybinds": {
                "leader": "ctrl+a", "session_undo": "leader+z,alt+u",
                "session_redo": "<leader>y,none", "command.palette.show": "<leader>p"
            }}))
            .unwrap();
        bindings
            .merge(&serde_json::json!({"keybinds": {
                "leader": "ctrl+b,ctrl+g", "session.redo": false
            }}))
            .unwrap();
        assert_eq!(bindings.command_palette_shortcut(), "ctrl+b p,ctrl+g p");
        assert_eq!(
            bindings.resolve(),
            ConversationShortcuts {
                leader: "ctrl+b,ctrl+g".into(),
                undo: "ctrl+b z,ctrl+g z,alt+u".into(),
                redo: String::new(),
            }
        );
        for invalid in [
            serde_json::json!(true),
            serde_json::json!(null),
            serde_json::json!(17),
        ] {
            let mut bindings = super::ConversationKeybinds::default();
            assert!(
                bindings
                    .merge(&serde_json::json!({"keybinds": {"session.undo": invalid}}))
                    .is_err()
            );
        }
    }

    #[test]
    fn animations_are_strictly_validated_per_source_with_winning_provenance() {
        let source = |path: &str, text: &str| super::Source {
            path: path.into(),
            text: text.into(),
            trusted: true,
        };
        let global = source(
            "global/opencode.json",
            r#"{"animations":false,"terminal":{"copy":"manual"}}"#,
        );
        let project = source(
            "project/opencode.jsonc",
            "{ // project wins\n \"animations\": true, \"terminal\": {\"copy\": \"select\"},}",
        );
        let (generation, copy) = super::assemble_with_reader(
            &[global.clone(), project],
            &Default::default(),
            None,
            &|_, _| unreachable!("no file substitutions"),
        )
        .unwrap();
        assert_eq!(generation.animations, Some(true));
        assert_eq!(
            generation.provenance["animations"],
            "project/opencode.jsonc"
        );
        assert_eq!(copy, Some(oc_core::queries::TerminalCopyMode::Select));
        assert_eq!(
            generation.provenance["terminal.copy"],
            "project/opencode.jsonc"
        );

        let (generation, copy) = super::assemble_with_reader(
            &[global, source("project/opencode.json", "{}")],
            &Default::default(),
            None,
            &|_, _| unreachable!("no file substitutions"),
        )
        .unwrap();
        assert_eq!(generation.animations, Some(false));
        assert_eq!(generation.provenance["animations"], "global/opencode.json");
        assert_eq!(copy, Some(oc_core::queries::TerminalCopyMode::Manual));
        let generation =
            super::assemble(&[source("empty", "{}")], &Default::default(), None).unwrap();
        assert_eq!(generation.animations, None);
        assert!(!generation.provenance.contains_key("animations"));
        assert!(!generation.provenance.contains_key("terminal.copy"));

        for invalid in ["null", "0", "{}", r#""false""#, r#""sensitive-fixture""#] {
            let invalid_source = source("invalid", &format!("{{\"animations\":{invalid}}}"));
            for sources in [
                vec![invalid_source.clone()],
                vec![
                    invalid_source.clone(),
                    source("later", r#"{"animations":true}"#),
                ],
                vec![source("earlier", r#"{"animations":false}"#), invalid_source],
            ] {
                let error = super::assemble(&sources, &Default::default(), None).unwrap_err();
                assert_eq!(
                    error,
                    super::ConfigError::Invalid {
                        field: "animations".into(),
                        reason: "must be a boolean".into(),
                    }
                );
                assert!(!error.to_string().contains("sensitive-fixture"));
            }
        }
    }

    #[test]
    fn terminal_copy_is_validated_in_each_source_with_winning_provenance() {
        use oc_core::queries::TerminalCopyMode;
        let source = |path: &str, text: &str| super::Source {
            path: path.into(),
            text: text.into(),
            trusted: true,
        };
        let global = source("global/opencode.json", r#"{"terminal":{"copy":"manual"}}"#);
        let local = source(
            "project/opencode.jsonc",
            r#"{// override
            "terminal":{"copy":"select"},}"#,
        );
        let sources = [global.clone(), local];
        let assembled = |sources: &[super::Source]| {
            super::assemble_with_reader(sources, &Default::default(), None, &|_, _| {
                unreachable!("fixture has no file substitutions")
            })
        };
        let (generation, mode) = assembled(&sources).unwrap();
        assert_eq!(mode, Some(TerminalCopyMode::Select));
        assert_eq!(
            generation.provenance["terminal.copy"],
            "project/opencode.jsonc"
        );
        let (generation, mode) = assembled(&[global]).unwrap();
        assert_eq!(mode, Some(TerminalCopyMode::Manual));
        assert_eq!(
            generation.provenance["terminal.copy"],
            "global/opencode.json"
        );
        let absent = source("empty", "{}");
        let (generation, mode) = assembled(&[absent]).unwrap();
        assert_eq!(mode, None);
        assert!(!generation.provenance.contains_key("terminal.copy"));

        for invalid in [
            "null",
            "false",
            "42",
            "{}",
            r#""MANUAL""#,
            r#""sensitive-fixture""#,
        ] {
            let invalid_source = source(
                "invalid",
                &format!("{{\"terminal\":{{\"copy\":{invalid}}}}}"),
            );
            for sources in [
                vec![invalid_source.clone()],
                vec![
                    invalid_source,
                    source("later", r#"{"terminal":{"copy":"select"}}"#),
                ],
            ] {
                let error = super::assemble(&sources, &Default::default(), None).unwrap_err();
                assert_eq!(
                    error,
                    super::ConfigError::Invalid {
                        field: "terminal.copy".into(),
                        reason: "must be select or manual".into()
                    }
                );
                assert!(!error.to_string().contains("sensitive-fixture"));
            }
        }
        let error = super::assemble(
            &[source("bad", r#"{"terminal":"sensitive-fixture"}"#)],
            &Default::default(),
            None,
        )
        .unwrap_err();
        assert_eq!(
            error,
            super::ConfigError::Invalid {
                field: "terminal".into(),
                reason: "must be an object".into()
            }
        );
    }

    #[test]
    fn native_fallback_limits_are_explicit_validated_options() {
        let assemble_caps = |caps: serde_json::Value| {
            super::assemble(&[super::Source {
                path: "fallback.json".into(), trusted: true,
                text: serde_json::json!({"provider":{"fixture":{"options":{"apiKey":"fixture-key","nativeFallbackLimits":caps}}}}).to_string(),
            }], &std::collections::BTreeMap::new(), None)
        };
        let generation =
            assemble_caps(serde_json::json!({"context": 16_384, "output": 512})).unwrap();
        assert!(generation.warnings.is_empty());
        assert_eq!(
            generation.providers["fixture"]
                .options
                .native_fallback_limits
                .context
                .get(),
            16_384
        );
        assert_eq!(
            generation.providers["fixture"]
                .options
                .native_fallback_limits
                .output
                .get(),
            512
        );
        for caps in [
            serde_json::json!({"context":0}),
            serde_json::json!({"output":0}),
            serde_json::json!({"output":-1}),
            serde_json::json!({"context":"32768"}),
            serde_json::json!({"context":1024, "output":512}),
            serde_json::json!({"context":u64::MAX, "output":u64::MAX}),
            serde_json::json!({"typo":100}),
        ] {
            assert!(assemble_caps(caps).is_err());
        }
        let defaults = assemble_caps(serde_json::json!({})).unwrap();
        assert_eq!(
            defaults.providers["fixture"].options.native_fallback_limits,
            crate::models::FallbackLimits::default()
        );
    }

    #[test]
    fn conversation_snapshots_default_off_and_false_aliases_are_safe() {
        for raw in [
            serde_json::json!({}),
            serde_json::json!({"snapshot":false}),
            serde_json::json!({"snapshots":false}),
            serde_json::json!({"snapshot":false,"snapshots":false}),
        ] {
            assemble(
                &[Source {
                    path: "fixture".into(),
                    trusted: true,
                    text: raw.to_string(),
                }],
                &BTreeMap::new(),
                None,
            )
            .unwrap();
        }
        for field in ["snapshot", "snapshots"] {
            for value in [serde_json::json!(true), serde_json::json!("false")] {
                let raw = serde_json::json!({field:value});
                let error = assemble(
                    &[Source {
                        path: "fixture".into(),
                        trusted: true,
                        text: raw.to_string(),
                    }],
                    &BTreeMap::new(),
                    None,
                )
                .unwrap_err();
                if value == true {
                    assert!(
                        matches!(error,ConfigError::UnsupportedCapability { field:actual,.. } if actual==field)
                    );
                } else {
                    assert!(
                        matches!(error,ConfigError::Invalid { field:actual,.. } if actual==field)
                    );
                }
            }
        }
    }

    use super::{
        ConfigError, Permission, Source, assemble, classify_plugin, explain_redacted, legacy_key,
        parse_jsonc, parse_native_profile, parse_skill, split_frontmatter_value, strip_jsonc,
        substitute,
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
        let config = r#"{"provider": {"p": {"options": {"vendorExtension": "x", "apiKey": "k"},
            "models": {"m": {}}}}}"#;
        let generation = assemble(&[src("s", config, true)], &env(&[]), None).expect("assembled");
        assert_eq!(generation.providers["p"].options.api_key, "k");
        assert!(
            generation
                .warnings
                .iter()
                .any(|warning| warning.contains("provider.p.options.vendorExtension")),
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
        assert_eq!(
            assemble(&[src("s", codemode, true)], &env(&[]), None)
                .unwrap()
                .mcp["m"]
                .failure
                .as_ref()
                .unwrap()
                .code,
            oc_core::queries::ServiceCode::UnsupportedCapability
        );
        let oauth = r#"{"mcp": {"w": {"type": "remote", "url": "https://w.invalid/mcp",
            "enabled": true, "oauth": true}}}"#;
        assert_eq!(
            assemble(&[src("s", oauth, true)], &env(&[]), None)
                .unwrap()
                .mcp["w"]
                .failure
                .as_ref()
                .unwrap()
                .code,
            oc_core::queries::ServiceCode::UnsupportedCapability
        );
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
    fn v07c_public_source_uses_verified_directory_not_replaced_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let source_dir = temp.path().join("branch/source");
        let external = temp.path().join("external/source");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(source_dir.join("opencode.json"), "{}").unwrap();
        std::fs::write(source_dir.join("key"), "inside").unwrap();
        std::fs::write(external.join("key"), "EXTERNAL_V07C_SECRET_734a").unwrap();
        let source = source_dir
            .join("opencode.json")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            substitute("{file:key}", &source, true, &env(&[])).unwrap(),
            "inside"
        );
        std::fs::rename(temp.path().join("branch"), temp.path().join("old")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("external"), temp.path().join("branch"))
            .unwrap();
        let error = substitute("{file:key}", &source, true, &env(&[])).unwrap_err();
        assert!(matches!(error, ConfigError::Untrusted { .. }));
        assert!(!error.to_string().contains("EXTERNAL_V07C_SECRET_734a"));
        let error = assemble(
            &[src(
                &source,
                r#"{"provider":{"fixture":{"options":{"apiKey":"{file:key}"}}}}"#,
                true,
            )],
            &env(&[]),
            None,
        )
        .expect_err("public assembler has the same explicit policy");
        assert!(matches!(error, ConfigError::Untrusted { .. }));
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
    fn r10_skill_autoinvoke_metadata_follows_pinned_boolean_parsing() {
        let parse = |metadata: &str| {
            parse_skill(
                "s",
                &format!(
                    "---\ndescription: d\nmetadata:\n  opencode/autoinvoke: {metadata}\n---\nbody"
                ),
            )
            .expect("skill")
            .autoinvoke
        };
        assert_eq!(parse("false"), Some(false));
        assert_eq!(parse("true"), Some(true));
        assert_eq!(parse("\" FaLsE \""), Some(false));
        assert_eq!(parse("\"no\""), None);
        assert_eq!(parse("0"), None);
        assert_eq!(
            parse_skill("s", "---\ndescription: d\n---\nbody")
                .unwrap()
                .autoinvoke,
            None
        );
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
        assert_eq!(legacy_key("task"), "subagent");
        assert_eq!(legacy_key("read"), "read");
        // Upstream parity: no frontmatter still loads with the path id, and
        // unknown frontmatter fields are silently ignored.
        let bare = parse_skill("x", "no frontmatter").expect("bare skill");
        assert_eq!(bare.id, "x");
        assert_eq!(bare.name, "x");
        assert_eq!(bare.description, "");
        let unknown = parse_skill(
            "x",
            "---\nname: x\ndescription: x\nunknown: rejected\nlicense: MIT\n---\nbody",
        )
        .expect("unknown fields are ignored");
        assert_eq!(unknown.description, "x");
        assert!(parse_skill("x", "---\nname: x\ndescription: x\nbody").is_err());
        assert!(
            parse_skill("x", "---\nname: x\nname: dup\n---\nbody").is_err(),
            "duplicate keys stay a parse error"
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
    fn permission_glob_maps_fold_conservatively_and_unknown_actions_pass() {
        // Owner shape: `external_directory` with glob → action entries.
        let generation = assemble(
            &[src(
                "G/opencode.json",
                r#"{"permissions": {
                    "external_directory": {"*": "ask", "~/.cargo/**": "allow"},
                    "bash": {"*": "deny", "git *": "allow"},
                    "webfetch": {"*": "allow"},
                    "some_future_action": "allow"
                }}"#,
                true,
            )],
            &env(&[]),
            None,
        )
        .expect("glob maps");
        assert_eq!(
            generation.permissions["external_directory"],
            Permission::Ask
        );
        assert_eq!(generation.permissions["bash"], Permission::Deny);
        assert_eq!(generation.permissions["webfetch"], Permission::Allow);
        assert_eq!(
            generation.permissions["some_future_action"],
            Permission::Allow
        );
        // A malformed nested action is still a precise error.
        let bad = assemble(
            &[src(
                "G/opencode.json",
                r#"{"permissions": {"bash": {"*": "maybe"}}}"#,
                true,
            )],
            &env(&[]),
            None,
        )
        .expect_err("bad action");
        assert!(matches!(bad, ConfigError::Invalid { .. }));
    }

    #[test]
    fn skill_frontmatter_comments_and_optional_metadata() {
        // Commented-out fields never count as duplicates or unknowns.
        let meta = parse_skill(
            "demo",
            "---\n#model: vendor/commented\nname: demo\ndescription: use #hash\nmetadata:\n  author: someone\n  tags:\n    - a\n    - b\ncompatibility: opencode\nlicense: MIT\n---\nbody\n",
        )
        .expect("skill");
        assert_eq!(meta.name, "demo");
        assert_eq!(meta.description, "use");
        let missing_description =
            parse_skill("demo", "---\nname: demo\n---\nbody\n").expect("description optional");
        assert_eq!(missing_description.description, "");
        let (value, body) = split_frontmatter_value(
            "---\nmodel: vendor/model#variant # trailing\nmode: subagent\n---\nrest\n",
        )
        .expect("frontmatter");
        assert_eq!(value["model"], "vendor/model#variant");
        assert_eq!(value["mode"], "subagent");
        assert_eq!(body, "rest\n");
    }

    #[test]
    fn go03_package_protocol_and_explicit_chat_compatibility() {
        use crate::config::{ProviderEntry, ProviderOptions, package_protocol, provider_wire};
        use crate::provider::protocol::Protocol;
        for (npm, protocol) in [
            (None, Some(Protocol::Responses)),
            (Some("@ai-sdk/openai"), Some(Protocol::Responses)),
            (
                Some("@opencode/ai/providers/openai"),
                Some(Protocol::Responses),
            ),
            (Some("@ai-sdk/openai-compatible"), Some(Protocol::Chat)),
            (
                Some("@opencode/ai/providers/openai-compatible"),
                Some(Protocol::Chat),
            ),
            (Some("@ai-sdk/anthropic"), Some(Protocol::Messages)),
            (
                Some("@opencode/ai/providers/anthropic"),
                Some(Protocol::Messages),
            ),
            (Some("evil-pkg"), None),
        ] {
            assert_eq!(package_protocol("p", npm).ok(), protocol, "{npm:?}");
        }
        let entry = |models: serde_json::Value| ProviderEntry {
            npm: Some("@ai-sdk/openai-compatible".into()),
            name: None,
            options: ProviderOptions::default(),
            models: serde_json::from_value(models).unwrap(),
        };
        let wire = provider_wire(
            "p",
            &entry(serde_json::json!({
                "a": {"compatibility": {"reasoningField": "reasoning_content", "maxTokensField": "max_completion_tokens", "supportsPromptCacheKey": true}},
                "b": {"interleaved": {"field": "reasoning_details"}},
                "c": {"interleaved": true},
                "d": {}
            })),
        )
        .unwrap();
        assert_eq!(wire.protocol, Protocol::Chat);
        let a = &wire.chat["a"];
        assert_eq!(a.reasoning_field.as_deref(), Some("reasoning_content"));
        assert!(a.max_completion_tokens && a.supports_prompt_cache_key);
        assert_eq!(
            wire.chat["b"].reasoning_field.as_deref(),
            Some("reasoning_details")
        );
        assert!(
            !wire.chat.contains_key("c") && !wire.chat.contains_key("d"),
            "boolean interleaved invents no field"
        );
        for bad in [
            serde_json::json!({"m": {"compatibility": {"maxTokensField": "tokens"}}}),
            serde_json::json!({"m": {"compatibility": {"requireReasoning": true}}}),
            serde_json::json!({"m": {"compatibility": "yes"}}),
            serde_json::json!({"m": {"interleaved": 3}}),
        ] {
            assert!(provider_wire("p", &entry(bad.clone())).is_err(), "{bad}");
        }
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
        assert_eq!(
            classify_plugin("@tarquinen/opencode-dcp@3.2.0", "/r").expect("current"),
            "dcp"
        );
        for rejected in [
            "@3.1.16",
            "@3.2.1",
            "@^3.2.0",
            "@>=3.2.0",
            "@3.2",
            "@3.2.0-beta",
        ] {
            assert!(
                classify_plugin(&format!("@tarquinen/opencode-dcp{rejected}"), "/r").is_err(),
                "{rejected}"
            );
        }
        assert!(classify_plugin("@tarquinen/opencode-dcp@3.1.14", "/r").is_err());
        assert!(classify_plugin("/other/openproxy-models.js", "/r").is_err());
        assert!(classify_plugin("https://x.invalid/p.js", "/r").is_err());
        let _ = HashSet::<String>::new();
    }
}
