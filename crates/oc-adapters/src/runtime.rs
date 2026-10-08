//! Authoritative runtime turn loop (T24): one prompt assembler,
//! generation-guarded turns, a unified permission path for built-ins/MCP/
//! compress, Location-scoped sessions, MCP/DCP/reasoning cleanup, and
//! config reload between turns only.
//!
//! The provider receives typed messages and complete Responses output items.
//! Durable per-turn wire journals preserve stateless continuation. Tool effects are
//! non-transactional; the staleness guarantee covers durable records
//! (turn result, messages, compression blocks), which never commit under
//! a superseded generation.
//! State/facade live here; turn/context/MCP implementations are owner children.
//! Identity/VIS38 tests are in runtime/tests.rs; existing compaction stays separate.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oc_core::context_plan::ProtectedSpec;
use oc_core::session::{Message, MessageId, Role};

use crate::config::{Generation, Permission};
use crate::dcp_auto::{DcpConfig, DcpStats, NudgeState, evaluate_request};
use crate::mcp_remote::{self, CodexWebClient, McpError};
use crate::mcp_stdio::{StdioClient, StdioConfig, StdioError};

use crate::models::{self, ModelCatalog};
use crate::patch::ProtectedGlobs;
use crate::provider::{InputItem, InputRole, ResponsesConfig, ToolDef};
use crate::storage::{BoundSessionCreation, Db, SessionMeta, StorageError};
use crate::tools::{
    Assembled, CallFailure, SUBAGENT_NO_TEXT, SUBAGENT_TOOL, SkillSnapshot, SubagentOutcome,
    SubagentRequest, SubagentRunner, ToolContext, ToolError, ToolPolicy, ToolRoots, TurnLog,
    assemble_calls, execute_batch,
};
pub(crate) mod children;
#[path = "runtime_compaction.rs"]
mod compaction;
mod environment;
mod retry;
mod terminals;

/// Max tool-output bytes kept in the turn report.
pub const REPORT_OUTPUT_CAP: usize = 2_048;
/// Max enabled MCP servers attached in one application generation.
pub const MAX_MCP_SERVERS: usize = 8;
/// Generation-wide budget for closing every owned MCP resource.
pub const MCP_CLOSE_BUDGET: Duration = Duration::from_secs(10);
/// Max command definition/invocation bytes for one expansion.
///
/// Upstream opencode has no command size limit; the owner config ships a
/// 41 KiB command, so the previous 4 KiB cap made loading succeed but
/// invocation fail. Kept as a generous serving bound (audited contract: the
/// expansion stays bounded), never reached by realistic command files.
pub const COMMAND_BYTES_CAP: usize = 1024 * 1024;
/// Prefs key prefix binding sessions to Locations.
pub const SESSION_LOCATION_PREFIX: &str = crate::storage::SESSION_LOCATION_PREFIX;
/// Permission name gating manual compress execution.
pub const COMPRESS_TOOL: &str = "compress";

/// Typed runtime errors (kinds and ids only, no secrets or payloads).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    QuestionRequired,
    ApprovalRequired {
        tool: String,
    },
    /// Another turn already runs on this runtime (single-flight).
    TurnActive,
    /// Session id is unknown to storage.
    SessionNotFound,
    /// Session belongs to another Location.
    LocationMismatch {
        /// Session id.
        session: String,
        /// Owning Location.
        location: String,
    },
    /// Generation was superseded before durable commit.
    StaleGeneration {
        /// Turn generation.
        want: u64,
        /// Current generation.
        got: u64,
    },
    /// Central policy denial (Ask fails everywhere: no approval channel).
    PermissionDenied {
        /// Tool name.
        tool: String,
    },
    /// MCP server attach failure (fail fast, never silent absence).
    McpAttach {
        /// Server id.
        server: String,
        /// Safe operation stage, never an endpoint or command.
        stage: &'static str,
        /// Allowlisted error class, never a raw SDK error.
        safe_code: &'static str,
        /// Whether a new explicit attempt may succeed without config changes.
        retryable: bool,
    },
    /// One or more owned MCP resources could not confirm shutdown/reap.
    McpShutdown,
    /// Combined generation-owned MCP metadata exceeded its independent cap.
    McpCatalogLimit,
    /// Provider failure (kind only).
    Provider,
    /// Typed credential/catalog refusal before durable turn/effect acceptance.
    ProviderUnavailable(oc_core::queries::ServiceDiagnostic),
    /// Storage failure (kind only).
    Storage,
    /// Compress argument/apply failure (reason only).
    Compress(String),
    /// Active context exceeded the independent byte safety budget.
    ContextOverflow {
        /// Bytes the active projection needs.
        bytes: u64,
        /// Configured safety budget.
        cap: usize,
    },
    /// Malformed runtime input (empty Location, bad command, over-cap).
    InvalidArgs(String),
    /// Explicit cancellation drained to durable records.
    Cancelled,
}

pub(crate) fn mcp_diagnostic(
    source: &str,
    error: &RuntimeError,
) -> oc_core::queries::ServiceDiagnostic {
    // Preserve the existing typed MCP tag/stage/action projection. This is the
    // request-level native service; per-entry identity remains in MCP inventory.
    let mut diagnostic = mcp::connection_diagnostic("native-mcp", source, error);
    diagnostic.service = "native-mcp".into();
    diagnostic.field = vec!["mcp".into()];
    diagnostic
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QuestionRequired => write!(
                f,
                "question required: use the interactive TUI to answer; --auto only accepts permissions"
            ),
            Self::ApprovalRequired { tool } => {
                write!(f, "approval required for {tool}: no consumer")
            }
            Self::TurnActive => write!(f, "turn already active"),
            Self::SessionNotFound => write!(f, "session not found"),
            Self::LocationMismatch { session, location } => {
                write!(f, "session {session} belongs to location {location}")
            }
            Self::StaleGeneration { want, got } => {
                write!(f, "stale generation {want}; current is {got}")
            }
            Self::PermissionDenied { tool } => write!(f, "denied {tool}"),
            Self::McpAttach {
                server,
                stage,
                safe_code,
                retryable,
            } => write!(
                f,
                "mcp {server} {stage}: {safe_code} (retryable={retryable})"
            ),
            Self::McpShutdown => write!(f, "mcp shutdown failed"),
            Self::McpCatalogLimit => write!(f, "mcp generation catalog capacity exceeded"),
            Self::Provider => write!(f, "provider error"),
            Self::ProviderUnavailable(diagnostic) => write!(f, "{diagnostic}"),
            Self::Storage => write!(f, "storage error"),
            Self::Compress(reason) => write!(f, "compress: {reason}"),
            Self::ContextOverflow { bytes, cap } => write!(
                f,
                "active context is {bytes} bytes, above the {cap} byte safety budget; compress the session or start a new one"
            ),
            Self::InvalidArgs(reason) => write!(f, "invalid args: {reason}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl From<StorageError> for RuntimeError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::SessionNotFound => Self::SessionNotFound,
            _ => Self::Storage,
        }
    }
}

impl From<crate::auth::AuthError> for RuntimeError {
    fn from(error: crate::auth::AuthError) -> Self {
        use oc_core::queries::{
            ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
        };
        match error {
            crate::auth::AuthError::Storage(_) => Self::Storage,
            crate::auth::AuthError::Cancelled => Self::Cancelled,
            other => {
                let (code, action) = match other {
                    crate::auth::AuthError::ModelUnavailable => {
                        (ServiceCode::ModelUnavailable, ServiceAction::SelectModel)
                    }
                    crate::auth::AuthError::Authority | crate::auth::AuthError::Conflict => (
                        ServiceCode::InvalidConfig,
                        ServiceAction::ReviewConfiguration,
                    ),
                    _ => (
                        ServiceCode::MissingCredential,
                        ServiceAction::Reauthenticate,
                    ),
                };
                Self::ProviderUnavailable(ServiceDiagnostic {
                    kind: ServiceKind::Provider,
                    service: "openai".into(),
                    source: "native OpenAI request".into(),
                    field: vec![
                        "provider".into(),
                        "openai".into(),
                        if code == ServiceCode::ModelUnavailable {
                            "model"
                        } else {
                            "authPolicy"
                        }
                        .into(),
                    ],
                    stage: ServiceStage::Admission,
                    code,
                    action,
                })
            }
        }
    }
}

/// One published immutable generation (monotonic id, atomic swap).
#[derive(Debug, Clone)]
pub struct PublishedGeneration {
    /// Monotonic publication id.
    pub id: u64,
    /// Immutable config snapshot.
    pub config: Generation,
}

/// Central permission bridge: config `Permission` map over `ToolPolicy`.
///
/// Unlisted tools are denied (invalid/untrusted config never allows).
/// Synchronous `Ask` fails closed unless the owner attached an exact-call permit.
#[derive(Clone)]
pub struct RuntimePolicy<'a> {
    permissions: &'a BTreeMap<String, Permission>,
    rules: Option<&'a crate::permissions::PermissionRules>,
    root: Option<&'a std::path::Path>,
    mcp: &'a [mcp_remote::RegistryEntry],
    permit: Option<InvocationPermit>,
}

#[derive(Clone)]
struct InvocationPermit {
    call: crate::tools::ToolCall,
    resources: Vec<String>,
    patch_preimage: Option<String>,
    shell_cwd: Option<Arc<crate::shell::PinnedCwd>>,
    compression_plan: Option<crate::dcp::CompressionPlan>,
}

enum AdmissionFailure {
    Required,
    Denied(String),
    Rejected(Option<String>),
    Cancelled,
    Invalid(String),
}
impl From<String> for AdmissionFailure {
    fn from(value: String) -> Self {
        Self::Invalid(value)
    }
}
impl From<&str> for AdmissionFailure {
    fn from(value: &str) -> Self {
        Self::Invalid(value.into())
    }
}
impl std::fmt::Display for AdmissionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Required => write!(f, "approval required: no consumer"),
            Self::Denied(tool) => write!(f, "denied {tool}"),
            Self::Rejected(_) => write!(f, "approval rejected"),
            Self::Cancelled => write!(f, "approval cancelled"),
            Self::Invalid(value) => f.write_str(value),
        }
    }
}

impl<'a> RuntimePolicy<'a> {
    fn tool_visible(&self, tool: &str) -> bool {
        let alias = self.mcp_alias(tool);
        let mut actions = vec![tool];
        if let Some(alias) = &alias {
            actions.push(alias);
        }
        self.rules
            .unwrap_or(&crate::permissions::PermissionRules::default())
            .actions_visible(self.permissions, &actions, alias.is_some())
    }
    /// Bridge one permission map.
    pub fn new(permissions: &'a BTreeMap<String, Permission>) -> Self {
        Self {
            permissions,
            rules: None,
            root: None,
            mcp: &[],
            permit: None,
        }
    }

    /// Bridge ordered action/resource rules (scalar map is compatibility fallback).
    pub fn with_rules(
        permissions: &'a BTreeMap<String, Permission>,
        rules: &'a crate::permissions::PermissionRules,
    ) -> Self {
        Self {
            permissions,
            rules: Some(rules),
            root: None,
            mcp: &[],
            permit: None,
        }
    }

    /// Bind path resources to the immutable Location directory.
    pub fn with_root(mut self, root: &'a std::path::Path) -> Self {
        self.root = Some(root);
        self
    }

    /// Preserve upstream MCP action identity separately from provider wire names.
    pub fn with_mcp(mut self, entries: &'a [mcp_remote::RegistryEntry]) -> Self {
        self.mcp = entries;
        self
    }

    /// Resolve the permission effect without turning ask into allow.
    pub fn effect(&self, tool: &str, resource: &str) -> Permission {
        let alias = self.mcp_alias(tool);
        let mut actions = vec![crate::config::legacy_key(tool)];
        if let Some(alias) = &alias {
            actions.push(alias);
        }
        self.rules
            .unwrap_or(&crate::permissions::PermissionRules::default())
            .evaluate_registered(
                self.permissions,
                &actions,
                resource,
                alias.is_some(),
                self.root,
            )
    }

    fn mcp_alias(&self, tool: &str) -> Option<String> {
        self.mcp
            .iter()
            .find(|entry| entry.namespaced == tool)
            .map(|entry| {
                let sanitize = |value: &str| {
                    value
                        .chars()
                        .map(|c| {
                            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                                c
                            } else {
                                '_'
                            }
                        })
                        .collect::<String>()
                };
                format!("{}_{}", sanitize(&entry.server), sanitize(&entry.tool))
            })
    }

    /// Live instruction fetches obey effective read authority and explicit
    /// ceilings in both resolved and Location forms. Initial roots are separate
    /// admitted config snapshots and never pass through this live-read hook.
    pub(crate) fn instruction_path_denied(&self, resource: &str) -> bool {
        let normalized = permission_path(self.root, resource);
        self.effect("read", &normalized) == Permission::Deny
            || self.rules.map_or(
                self.permissions.get("read") == Some(&Permission::Deny),
                |rules| {
                    rules.denied_by_rule(self.permissions, "read", resource)
                        || rules.denied_by_rule(self.permissions, "read", &normalized)
                },
            )
    }

    /// Automatic source IO needs permanent Allow for that source. Keep the
    /// admitted original-file guard above distinct: approved Ask reads complete.
    pub(crate) fn automatic_instruction_source_allowed(&self, resource: &str) -> bool {
        !self.instruction_path_denied(resource)
            && self.rules.map_or(
                self.permissions.get("read") == Some(&Permission::Allow),
                |rules| {
                    rules.automatic_source_allowed(
                        self.permissions,
                        resource,
                        &permission_path(self.root, resource),
                    )
                },
            )
    }
}

impl ToolPolicy for RuntimePolicy<'_> {
    fn search_path_denied(&self, resource: &str) -> bool {
        let resource = permission_path(self.root, resource);
        self.rules.map_or(
            self.permissions.get("read") == Some(&Permission::Deny),
            |rules| rules.denied_by_rule(self.permissions, "read", &resource),
        )
    }
    fn approved_shell_cwd(&self) -> Option<Arc<crate::shell::PinnedCwd>> {
        self.permit.as_ref().and_then(|p| p.shell_cwd.clone())
    }
    fn approved_patch_preimage(&self) -> Option<&str> {
        self.permit
            .as_ref()
            .and_then(|p| p.patch_preimage.as_deref())
    }
    fn check(&self, tool: &str) -> Result<(), ToolError> {
        self.check_resource(tool, "*")
    }

    fn check_resource(&self, tool: &str, resource: &str) -> Result<(), ToolError> {
        let normalized;
        let resource = if matches!(crate::config::legacy_key(tool), "read" | "apply_patch")
            && resource != "*"
        {
            normalized = permission_path(self.root, resource);
            normalized.as_str()
        } else {
            resource
        };
        match self.effect(tool, resource) {
            Permission::Allow => Ok(()),
            Permission::Ask
                if self.permit.as_ref().is_some_and(|p| {
                    p.call.name == tool && p.resources.iter().any(|r| r == resource)
                }) =>
            {
                Ok(())
            }
            Permission::Ask => Err(ToolError::ApprovalRequired {
                tool: tool.to_string(),
            }),
            Permission::Deny => Err(ToolError::Denied {
                tool: tool.to_string(),
            }),
        }
    }

    fn check_call(&self, call: &crate::tools::ToolCall) -> Result<(), ToolError> {
        if let Some(permit) = &self.permit
            && (permit.call.id != call.id
                || permit.call.name != call.name
                || permit.call.arguments != call.arguments)
        {
            return Err(ToolError::Denied {
                tool: call.name.clone(),
            });
        }
        for resource in crate::tools::permission_resources(call)? {
            self.check_resource(&call.name, &resource)?;
        }
        Ok(())
    }
}

fn permission_path(root: Option<&std::path::Path>, resource: &str) -> String {
    use std::path::{Component, Path, PathBuf};
    let path = Path::new(resource);
    let path = match root {
        Some(root) => root.join(path),
        None => path.to_path_buf(),
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    root.and_then(|root| normalized.strip_prefix(root).ok())
        .unwrap_or(&normalized)
        .to_string_lossy()
        .into_owned()
}

/// Map a policy denial to the runtime error (same path, all tool kinds).
fn denied(tool: &str) -> RuntimeError {
    RuntimeError::PermissionDenied {
        tool: tool.to_string(),
    }
}

/// Built-in tool definitions advertised to the provider.
pub fn builtin_tool_defs() -> Vec<ToolDef> {
    let schema = |properties: serde_json::Value, required: &[&str]| {
        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        })
    };
    vec![
        ToolDef {
            name: "opencode_models".into(),
            description: "Search the admitted model catalog without changing your model or making generation requests. Includes the credential-free public OpenCode Go catalog with a bounded cached metadata GET. Own provider first, newest known family by default. Unknown metadata stays null. Variant IDs use the shared effective effort order. Native 64KiB page budget; other unselected configured providers expose static metadata only.".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false,"required":[],"properties":{
                "query":{"type":"string","maxLength":4096},"provider":{"type":"string","maxLength":4096},
                "all":{"type":"boolean","default":false},"limit":{"type":"integer","minimum":1,"maximum":100,"default":20},
                "offset":{"type":"integer","minimum":0,"default":0}}}),
        },
        ToolDef {
            name: "opencode_session_rename".into(),
            description: "Rename the current session, or an explicitly locally known idle root in this Location. Children may rename only themselves. Title is trimmed, nonempty, at most 256 UTF-8 bytes with no control/invisible text. Effective session policy applies to the exact session ID; success means durable title and session_updated event.".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false,"required":["title"],"properties":{
                "title":{"type":"string","minLength":1,"maxLength":256},"sessionID":{"type":"string","minLength":1,"maxLength":256}}}),
        },
        ToolDef {
            name: "opencode_session_move".into(),
            description: "Admit a same-session directory move. Omit sessionID for current; explicit targets must be locally known idle roots. Children may move only themselves; General/Explore cannot move. Relative paths and ~ use the target's original context and admitted HOME. Existing no-follow/data-root/trust guards apply; external_directory must already be Allow. Returns pending with a durable operation ID: placement changes only after the FULL source turn is durably terminal. All same-turn tools retain source context. A destination request must be a distinct new turn. No saved grant is created.".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false,"required":["directory"],"properties":{
                "directory":{"type":"string","minLength":1,"maxLength":4096},"sessionID":{"type":"string","minLength":1,"maxLength":256}}}),
        },
        ToolDef {
            name: "read".to_string(),
            description: "Read a project file or directory. Text has 1-based line references; directories are sorted with directories first. offset is 1-based, limit defaults to 2000. Validated PNG/JPEG/GIF/WebP images are inline content for image-capable selected Responses models; PDF is unsupported. Native no-follow, permission, data-root, 1 MiB file scan and 65536-byte page budgets apply."
                .to_string(),
            parameters: schema(
                serde_json::json!({
                    "path": {"type": "string"},
                    "offset": {"type": "integer", "minimum":1, "maximum":crate::storage::tool_output::CAP+1, "default":1,"description":"Ordinary files: at most 1000000; exact registered tool-output artifacts: bounded by the 16 MiB capture extent."},
                    "limit": {"type": "integer", "minimum":1, "maximum":2000, "default":2000},
                }),
                &["path"],
            ),
        },
        ToolDef {
            name: "glob".to_string(),
            description: "Search file paths using ripgrep glob syntax. `path` is an admitted project directory (default root); `hidden` defaults false. Positive patterns override ignore files; .git is excluded. Results are Location-relative, lexicographically sorted and paginated; default limit 100. Native trust/no-follow/data-root and entry/byte/30-second budgets apply.".to_string(),
            parameters: serde_json::json!({"type":"object", "properties": {
                    "pattern": {"type": "string"},
                    "path": {"type": "string"},
                    "hidden": {"type": "boolean", "default": false},
                    "offset": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 100}
                }, "required":["pattern"], "additionalProperties":false}),
        },
        ToolDef {
            name: "grep".to_string(),
            description: "Search text with ripgrep's default Rust regex syntax (no look-around/backreferences); `literal:true` searches exact text. `path` scopes an admitted project file/directory; `include` is a ripgrep glob filter. caseSensitive defaults true; hidden files are searched, .git excluded, positive include overrides ignore files. Results are Location-relative, sorted by path/line and paginated; default limit 100. Native trust/no-follow/data-root and compile/entry/byte/30-second budgets apply.".to_string(),
            parameters: serde_json::json!({"type":"object", "properties": {
                    "pattern": {"type": "string"},
                    "path": {"type": "string"},
                    "include": {"type": "string"},
                    "literal": {"type": "boolean", "default": false},
                    "caseSensitive": {"type": "boolean", "default": true},
                    "offset": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 1000, "default": 100}
                }, "required":["pattern"], "additionalProperties":false}),
        },
        ToolDef {
            name: "apply_patch".to_string(),
            description: "Apply a patch to project files. `patchText` must start with `*** Begin Patch` and end with `*** End Patch`. For `*** Add File: <relative path>`, prefix every content line with `+`; no content lines creates an empty file. Update hunks use `*** Update File: <relative path>`, optionally immediately followed by `*** Move to: <relative path>`, then `@@` and lines where context starts with a space, removals with `-`, additions with `+`, all matching file bytes exactly. Delete uses `*** Delete File: <relative path>`. Paths stay inside the project root."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"patchText": {"type": "string"}},
                "required": ["patchText"],
                "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "shell".to_string(),
            description: "Execute a Linux shell command using the compatible inherited SHELL (fallback PATH bash, then /bin/sh), with -c semantics. Quote paths containing spaces; prefer dedicated tools. `workdir` is relative to the admitted project root (default root). `timeout` is milliseconds (foreground default 120000, background default 0; 0 disables only execution timeout; native positive ceiling 600000). Background returns running/shellID after launch; you will be notified automatically when it completes. DO NOT poll; continue independent work or end your response. Output is bounded; cancellation and process-group teardown remain active. The child receives a minimal credential-free environment."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object", "properties": {
                    "command": {"type": "string", "minLength": 1, "maxLength": crate::shell::ARG_BYTES_CAP},
                    "workdir": {"type": "string"},
                    "timeout": {"type": "integer", "minimum": 0, "maximum": crate::tools::BASH_TIMEOUT_CAP_MS},
                    "background": {"type": "boolean", "default": false},
                }, "required": ["command"], "additionalProperties": false,
            }),
        },
        ToolDef {
            name: "write".into(),
            description: "Write a project text file, overwriting if it exists and creating missing parent directories. content preserves supplied newlines and EOF; an existing or supplied UTF-8 BOM is retained once. Use edit for partial changes. Shared mutation permissions, no-follow/data-root/preimage guards and bounded confirmed diffs apply; no preceding read call is required.".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false,"required":["path","content"],"properties":{"path":{"type":"string"},"content":{"type":"string"}}}),
        },
        ToolDef {
            name: "edit".into(),
            description: "Edit an existing project text file by replacing oldString with newString. Omit Read line-number prefixes and preserve indentation. oldString must be nonempty and differ from newString; newString may be empty to delete text. UNIQUE match is required unless replaceAll is true. Exact nonoverlapping matches precede typographic normalization, then trailing-whitespace line matching. CRLF/BOM and actual EOF are preserved. Shared mutation permission/preimage/no-follow/bounded-diff safeguards apply; no preceding read call is required.".into(),
            parameters: serde_json::json!({"type":"object","additionalProperties":false,"required":["path","oldString","newString"],"properties":{"path":{"type":"string"},"oldString":{"type":"string"},"newString":{"type":"string"},"replaceAll":{"type":"boolean","default":false}}}),
        },
        ToolDef {
            name: "webfetch".to_string(),
            description: "Read-only HTTP/HTTPS GET. format text|markdown|html defaults to markdown; HTML converts to readable Unicode with useful structure, other textual MIME stays original. timeout is seconds (>0, maximum120, default30), one total DNS/redirect/body/conversion deadline. Returns original/final URL, status, content type and requested format metadata. Native 1MiB body/output caps and per-hop actual-dial public-egress guard apply. No browser/JS/search, cookies, auth or inherited proxy.".to_string(),
            parameters: serde_json::json!({"type":"object", "properties":{
                "url":{"type":"string","minLength":1,"maxLength":crate::webfetch::URL_CAP_BYTES},
                "format":{"type":"string","enum":["text","markdown","html"],"default":"markdown"},
                "timeout":{"type":"number","exclusiveMinimum":0,"maximum":120,"default":30}
            },"required":["url"],"additionalProperties":false}),
        },
        ToolDef {
            name: "skill".to_string(),
            description: "Invoke a pinned skill".to_string(),
            parameters: schema(serde_json::json!({"id": {"type": "string"}}), &["id"]),
        },
        ToolDef {
            name: "question".into(),
            description: "Ask the user questions during execution and wait for their answers. A Type your own answer option is added automatically; do not include it in options. multiple defaults false; set true for multiple choices. Put a recommended choice first with (Recommended) in its label. Interactive TUI required; permission autoaccept never answers. Native limits: 16 questions, 16 options each, 64KiB input/reply.".into(),
            parameters: serde_json::json!({"type":"object", "additionalProperties":false, "required":["questions"], "properties":{
                "questions":{"type":"array","minItems":1,"maxItems":16,"items":{
                    "type":"object","additionalProperties":false,"required":["question","header","options"],"properties":{
                        "question":{"type":"string","minLength":1,"maxLength":4096},
                        "header":{"type":"string","minLength":1,"maxLength":30},
                        "options":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"required":["label","description"],"properties":{
                            "label":{"type":"string","minLength":1,"maxLength":256},"description":{"type":"string","maxLength":4096}}}},
                        "multiple":{"type":"boolean","default":false}
                    }}}}}),
        },
        ToolDef {
            name: COMPRESS_TOOL.to_string(),
            description: "Replace one or more closed transcript ranges with durable summaries. Use only stable startId/endId anchors from the DCP context lane; never include the unfinished final anchor in a range. Exception: to renew the current task's working memory before finishing, send one range whose startId and endId are both that final anchor (the current task message) with a standalone summary of what still matters; it replaces the task text from the next request while the original stays durable.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "topic": {"type": "string", "minLength": 1, "maxLength": crate::dcp::TOPIC_CAP},
                    "content": {"type": "array", "minItems": 1, "maxItems": crate::dcp::RANGES_CAP, "items": {
                        "type": "object",
                        "properties": {
                            "startId": {"type": "string"},
                            "endId": {"type": "string"},
                            "summary": {"type": "string", "minLength": 1, "maxLength": crate::dcp::SUMMARY_CAP}
                        },
                        "required": ["startId", "endId", "summary"],
                        "additionalProperties": false
                    }}
                },
                "required": ["topic", "content"],
                "additionalProperties": false
            }),
        },
    ]
}

/// Exact pinned OC2 predicate; this selects tools only, never provider routing.
pub(crate) fn selected_tool_defs(model_id: &str) -> Vec<ToolDef> {
    let patch =
        model_id.contains("gpt-") && !model_id.contains("oss") && !model_id.contains("gpt-4");
    builtin_tool_defs()
        .into_iter()
        .filter(|tool| match tool.name.as_str() {
            "apply_patch" => patch,
            "edit" | "write" => !patch,
            _ => true,
        })
        .collect()
}

fn file_tool_guidance(tools: &[ToolDef]) -> Option<InputItem> {
    let names = tools
        .iter()
        .filter(|tool| matches!(tool.name.as_str(), "apply_patch" | "edit" | "write"))
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>();
    if names.is_empty() {
        return None;
    }
    Some(InputItem::message(
        InputRole::Developer,
        format!(
            "Current request file-mutation tools: {}. Use only this request's advertised tools; permission and preimage admission still apply.",
            names.join(", ")
        ),
    ))
}

/// Rough token estimate: the bytes/4 heuristic, not a counter for any
/// specific proxy model. Admission compares it against the selected entry's
/// declared `limit.context`/`limit.output`; a real model may count more or
/// fewer tokens, so the estimate is an explicit approximation and the byte
/// safety budget stays an independent guard.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.len() / 4) as u64
}

/// One bounded literal command expansion: `$ARGUMENTS` gets all args,
/// `$1..$9` get positionals; no recursion, no shell interpolation.
pub fn expand_command(body: &str, args: &[String]) -> Result<String, RuntimeError> {
    if body.len() > COMMAND_BYTES_CAP {
        return Err(RuntimeError::InvalidArgs(
            "command body too large".to_string(),
        ));
    }
    let joined_len: usize = args.iter().map(String::len).sum();
    if joined_len > COMMAND_BYTES_CAP {
        return Err(RuntimeError::InvalidArgs(
            "command args too large".to_string(),
        ));
    }
    let all = args.join(" ");
    let mut out = String::with_capacity(body.len().saturating_add(all.len()));
    let mut rest = body;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("$ARGUMENTS") {
            out.push_str(&all);
            rest = after;
        } else if rest.starts_with('$') && rest.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
            let index = usize::from(rest.as_bytes()[1] - b'0');
            if index > 0 {
                if let Some(arg) = args.get(index - 1) {
                    out.push_str(arg);
                } else {
                    out.push_str(&rest[..2]);
                }
                rest = &rest[2..];
            } else {
                out.push('$');
                rest = &rest[1..];
            }
        } else {
            let character = rest.chars().next().expect("nonempty");
            out.push(character);
            rest = &rest[character.len_utf8()..];
        }
        if out.len() > COMMAND_BYTES_CAP {
            return Err(RuntimeError::InvalidArgs(
                "expanded command too large".to_string(),
            ));
        }
    }
    Ok(out)
}

/// Terminal turn status persisted to storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnStatus {
    /// Turn completed all rounds.
    Completed,
    /// Explicit cancellation drained to records.
    Cancelled,
    /// Provider/storage/attach failure recorded.
    Failed,
    /// Generation superseded before durable commit.
    Interrupted,
    /// Successful response requested more work than the round budget permits.
    Incomplete,
}

impl TurnStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Incomplete => "incomplete",
        }
    }
}

/// One executed tool call for the report (output capped).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRecord {
    /// Registry tool name.
    pub name: String,
    /// Storage state (`completed`/`failed`/`denied`/`cancelled`).
    pub state: String,
    /// Capped output.
    pub output: String,
}

/// Tool-call lifecycle notification for live frontends (TUI tool cards).
///
/// Durable variants emit only after the record exists: `Started` after the intent
/// insert (before the side effect), `Finished` after the outcome update. The
/// payloads are bounded by the recorded caps; `output` is capped at
/// [`REPORT_OUTPUT_CAP`] with `output_bytes`/`output_truncated` describing the
/// full stored value, so a frontend never sees an unbounded field.
#[derive(Debug, Clone, PartialEq, Eq)]
// A Finished event publishes one bounded indivisible operation snapshot, also
// carried by history. Keep that DTO identical across both existing channels.
#[allow(clippy::large_enum_variant)]
pub enum ToolCallEvent {
    /// Live-only argument presentation, never a durable tool intent.
    ArgumentStream(oc_core::tool_stream::ToolStreamEvent),
    /// Intent durably recorded; the call may now run.
    Started {
        /// Topic from the admitted compression plan; absent before validation.
        dcp_topic: Option<String>,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// Recorded arguments JSON (bounded by the tool argument caps).
        input: String,
    },
    /// Terminal outcome durably recorded.
    Finished {
        output_presentation: Option<Box<oc_core::tool_output::Presentation>>,
        question: Option<oc_core::question::QuestionResult>,
        /// Frozen successful DCP presentation; independent of model output.
        dcp: Option<oc_core::dcp_view::DcpRunSnapshot>,
        /// Confirmed public mutation preview; never part of provider output.
        patch_effects: Option<oc_core::patch::PatchEffects>,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// Storage state (`completed`/`failed`/`denied`/`cancelled`/`no_gain`).
        state: String,
        /// Capped outcome preview.
        output: String,
        /// Full stored output size in bytes.
        output_bytes: i64,
        /// True when `output` is shorter than the stored value.
        output_truncated: bool,
    },
}

mod tool_stream;
use tool_stream::PendingToolStreams;

/// Turn outcome: durable records committed, transient report returned.
#[derive(Debug, Clone)]
pub struct TurnReport {
    /// Turn id.
    pub turn_id: String,
    /// Terminal status.
    pub status: TurnStatus,
    /// Sanitized provider error kind; never raw remote text or payloads.
    pub diagnostic: Option<String>,
    /// Accumulated model text.
    pub text: String,
    /// Rounds executed.
    pub rounds: u32,
    /// Billed usage, if reported: last reported input tokens and the output
    /// tokens summed over the turn's rounds (never synthesized).
    pub usage: Option<(u64, u64)>,
    /// Latest provider-reported generation input/output pair, even if other
    /// rounds omitted usage; not a billed turn total.
    pub context_usage: Option<(u64, u64)>,
    /// Provider-active streaming time in milliseconds, summed over rounds
    /// (upstream `time.streamed - time.created` per assistant step).
    pub streamed_ms: u64,
    /// Whole turn wall time in milliseconds, accept to commit (`turnDuration`).
    pub duration_ms: u64,
    /// Executed calls in order.
    pub calls: Vec<CallRecord>,
    /// Transient nudge hint (never persisted).
    pub nudge_hint: Option<String>,
    /// Sanitized per-server MCP degradation notices for this turn's generation
    /// (`mcp <id> <stage>: <code> (retryable=<bool>)`); never URLs or secrets.
    pub warnings: Vec<String>,
    /// Exact background-service subset of the legacy warning list. No text
    /// classification and no change to headless delivery or stored outcomes.
    pub service_warning_range: std::ops::Range<usize>,
}

/// Compress execution report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressReport {
    /// Stored block ids in order.
    pub blocks: Vec<String>,
    /// Rough saved tokens.
    pub saved_tokens: u64,
    /// True when the projection shrank.
    pub shrank: bool,
}

/// Independent byte safety budget for one assembled active projection.
///
/// This is a memory guard, not the model admission rule: token admission
/// (`models::admit`) still runs against the selected entry's context limit
/// and the estimate stays a heuristic. Exceeding this budget refuses the
/// turn with an explicit diagnostic instead of silently dropping facts.
pub const ACTIVE_CONTEXT_BYTES_CAP: usize = 16 * 1024 * 1024;

/// Turn-log page size for the bounded wire replay.
pub const WIRE_LOG_PAGE: usize = 64;

/// Turn parameters (everything a turn needs, nothing ambient).
pub struct TurnParams<'c> {
    /// Session id (Location-bound).
    pub session: String,
    /// User prompt, or expanded command text.
    pub prompt: String,
    /// Original command invocation, stored as the user message.
    pub invocation: Option<String>,
    /// Model catalog for exact selection (no fallback).
    pub catalog: &'c ModelCatalog,
    /// Requested model id.
    pub model_id: String,
    /// Requested variant, if any.
    pub variant: Option<String>,
    /// Requested output tokens, clamped during admission; zero selects the
    /// configured native default (4096 unless overridden).
    pub max_output: u64,
    /// Provider connection (exact configured URL inside).
    pub provider: ResponsesConfig,
    /// Cancellation flag (also ends streaming promptly).
    pub cancel: &'c AtomicBool,
}

/// Captured application-owned command route; never supplied by a model response.
pub(crate) struct NativeCommand {
    pub id: String,
    pub child: Option<CommandChild>,
    pub selection: Option<(String, String)>,
}

pub(crate) struct CommandChild {
    pub agent: String,
    pub provider: String,
    pub model_id: String,
    pub variant: Option<String>,
    pub description: String,
}

#[derive(Clone, Default)]
struct RuntimeWorkspace {
    agent_id: Option<String>,
    agent_color_index: Option<usize>,
    fixed_input: Vec<InputItem>,
    skills: SkillSnapshot,
    agent_digest: Option<String>,
    agent_permissions: BTreeMap<String, Permission>,
    agent_permission_rules: crate::permissions::PermissionRules,
    agent_request: crate::provider::RequestOverlay,
    instructions: String,
    instruction_roots: Vec<crate::instructions::Root>,
    skills_projection: Option<String>,
    subagents: Option<SubagentCatalog>,
    command_digest: Option<String>,
}

/// Fixed developer input + central policy for one turn lane.
///
/// The primary lane mirrors the published workspace. A child lane replaces
/// the agent prompt and narrows permissions with the child agent's rules.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TurnLane {
    manual_compression: bool,
    owning_operation: Option<String>,
    agent_id: Option<String>,
    agent_color_index: Option<usize>,
    fixed_input: Vec<InputItem>,
    agent_digest: Option<String>,
    permissions: BTreeMap<String, Permission>,
    permission_rules: crate::permissions::PermissionRules,
    /// Captured profile overlay; configuration, never persisted with the lane.
    #[serde(skip)]
    request: crate::provider::RequestOverlay,
}

/// Created only by the explicit application command owner, never prompt text.
pub(crate) struct ManualCompressionTrigger {
    session: String,
    generation: u64,
}

/// One spawnable agent profile snapshotted for the `subagent` tool.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SubagentAgent {
    /// Agent id (exact match for the tool's `agent` parameter).
    pub id: String,
    /// Description shown in the `Available subagents` list.
    pub description: String,
    /// Primary-only profiles resolve but are rejected as subagents.
    pub primary: bool,
    /// Pinned model (`provider/model[#variant]`), if any.
    pub model: Option<String>,
    /// Pinned variant, if any.
    pub variant: Option<String>,
    /// Agent prompt/body for the child lane.
    pub prompt: String,
    /// Agent permission rules; they can only narrow the parent lane.
    pub permissions: BTreeMap<String, Permission>,
    /// Ordered resource-aware narrowing.
    pub permission_rules: crate::permissions::PermissionRules,
    /// Hidden from discovery but explicitly callable if permitted.
    pub hidden: bool,
    /// Behavior digest pinning the child wire lane.
    pub digest: Option<String>,
    /// Profile request overlay for the child's own generations.
    #[serde(skip)]
    pub request: crate::provider::RequestOverlay,
    /// Explicit presentation color; such a lane pins no categorical slot.
    #[serde(skip)]
    pub color: Option<String>,
}

impl SubagentAgent {
    fn model_reference(&self) -> Option<String> {
        self.model.as_ref().map(|model| {
            if model.contains('#') {
                model.clone()
            } else {
                format!(
                    "{model}{}",
                    self.variant
                        .as_ref()
                        .map(|variant| format!("#{variant}"))
                        .unwrap_or_default()
                )
            }
        })
    }
}

/// Published subagent catalog + depth budget for this Location generation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SubagentCatalog {
    /// Profiles by id (both primary-only and subagent-capable).
    pub agents: BTreeMap<String, SubagentAgent>,
    /// Maximum nesting depth (`experimental.subagent_depth`).
    pub depth_limit: u32,
}

/// One bounded active projection: rows the model actually sees.
struct ActiveContext {
    /// Prune floor: no row at or below this seq is projected.
    after_seq: i64,
    /// Projected rows (active rows with block summaries placed in order).
    projected: Vec<(String, String, String)>,
    /// Compression blocks (full list: covered anchors stay replayable).
    blocks: Vec<crate::dcp::CompressionBlock>,
}

/// Authoritative runtime: Location + published generation + DCP counters.
///
/// Single-flight: one active turn at a time (mirrors the single-turn
/// worker); reload and DCP config changes only land between turns.
pub struct Runtime<'a> {
    prepared_moves: Mutex<BTreeMap<String, crate::application::session_move::Prepared>>,
    pub(crate) shell_jobs: Arc<crate::shell::jobs::Jobs>,
    pub(crate) child_jobs: Arc<children::Jobs>,
    approvals: Arc<oc_core::approval::ApprovalQueue>,
    questions: Arc<oc_core::question::QuestionQueue>,
    compactions: Mutex<BTreeMap<String, compaction::Work>>,
    compaction_events: Mutex<Option<tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>>>,
    native_compaction: RwLock<Option<Arc<dyn crate::compaction::NativeCompaction>>>,
    db: children::Database<'a>,
    location: String,
    current: RwLock<Arc<PublishedGeneration>>,
    provider_state: RwLock<Option<crate::composition::ProviderState>>,
    active: AtomicBool,
    protected: ProtectedGlobs,
    files: crate::files::Files,
    shell: crate::shell::Shell,
    parent_env: BTreeMap<String, String>,
    roots: ToolRoots,
    webfetch_auth: Option<String>,
    webfetch_allow_private: bool,
    dcp_config: RwLock<crate::dcp_auto::DcpConfig>,
    dcp_protected: RwLock<ProtectedSpec>,
    workspace: RwLock<RuntimeWorkspace>,
    nudge_state: Mutex<BTreeMap<String, NudgeState>>,
    // Sibling runtimes share the durable block allocator. Hold only across the
    // synchronous refresh/measure/commit section, never provider IO or approval.
    compression_commit: Arc<Mutex<()>>,
    stats: Mutex<crate::dcp_auto::DcpStats>,
    mcp_generation: RwLock<Arc<mcp::McpOwner>>,
    mcp_activation: RwLock<Option<Arc<crate::composition::McpActivation>>>,
    mcp_unsafe_retry: AtomicBool,
    mcp_cleanup_failed: AtomicBool,
    subagent_seq: AtomicU64,
}

/// Single-flight lease that releases the runtime even when the owning future
/// is dropped before returning.
struct ActiveLease<'a>(&'a AtomicBool);

impl Drop for ActiveLease<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn session_location_key(id: &str) -> String {
    format!("{SESSION_LOCATION_PREFIX}{id}")
}

/// Catalog-validated child model selection.
pub(crate) struct ResolvedModel {
    pub(crate) id: String,
    pub(crate) variant: Option<String>,
}

impl ResolvedModel {
    /// Stored `provider/model[#variant]` form for the child session row.
    fn stored(&self, provider: &str) -> String {
        match &self.variant {
            Some(variant) if !variant.is_empty() => format!("{provider}/{}#{variant}", self.id),
            _ => format!("{provider}/{}", self.id),
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let kept = text.floor_char_boundary(max);
    format!("{}…[+{}]", &text[..kept], text.len() - kept)
}

/// Whole milliseconds in a duration, saturating at `u64::MAX`.
fn streamed_ms(streamed: Duration) -> u64 {
    streamed.as_millis().min(u128::from(u64::MAX)) as u64
}

fn millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

// Unique across every runtime/Location in this process, even if the wall clock
// repeats or moves backwards. SQLite turns.id PRIMARY KEY rejects historical
// collisions before acceptance; no old in-memory events survive process restart.
pub(crate) fn next_turn_id(session: &str, timestamp: u64) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("turn identity space exhausted");
    format!("t{session}-{timestamp}-{}-{serial}", std::process::id())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod media_tests;

impl<'a> Runtime<'a> {
    /// Build a runtime over one Location and an initial generation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: &'a Db,
        location: &str,
        generation: Generation,
        protected: ProtectedGlobs,
        files: crate::files::Files,
        shell: crate::shell::Shell,
        parent_env: BTreeMap<String, String>,
        roots: ToolRoots,
        webfetch_auth: Option<String>,
        webfetch_allow_private: bool,
        dcp_config: DcpConfig,
    ) -> Result<Self, RuntimeError> {
        if location.trim().is_empty() {
            return Err(RuntimeError::InvalidArgs("empty location".to_string()));
        }
        // Outbound projection reads compression tables: ensure the
        // additive idempotent DCP schema before the first turn.
        crate::dcp::apply_dcp_schema(db).map_err(|_| RuntimeError::Storage)?;
        db.grants_schema()?;
        Ok(Self {
            prepared_moves: Mutex::new(BTreeMap::new()),
            shell_jobs: crate::shell::jobs::Jobs::new(db),
            child_jobs: children::Jobs::new(db),
            approvals: Arc::new(oc_core::approval::ApprovalQueue::default()),
            questions: Arc::new(oc_core::question::QuestionQueue::default()),
            compactions: Mutex::new(BTreeMap::new()),
            compaction_events: Mutex::new(None),
            native_compaction: RwLock::new(None),
            db: children::Database::Borrowed(db),
            location: location.to_string(),
            current: RwLock::new(Arc::new(PublishedGeneration {
                id: 1,
                config: generation,
            })),
            provider_state: RwLock::new(None),
            active: AtomicBool::new(false),
            protected,
            files,
            shell,
            parent_env,
            roots,
            webfetch_auth,
            webfetch_allow_private,
            dcp_config: RwLock::new(dcp_config),
            dcp_protected: RwLock::new(ProtectedSpec::default()),
            workspace: RwLock::new(RuntimeWorkspace::default()),
            nudge_state: Mutex::new(BTreeMap::new()),
            compression_commit: Arc::new(Mutex::new(())),
            stats: Mutex::new(DcpStats::default()),
            mcp_generation: RwLock::new(Arc::new(mcp::McpOwner::new(location, 1))),
            mcp_activation: RwLock::new(None),
            mcp_unsafe_retry: AtomicBool::new(false),
            mcp_cleanup_failed: AtomicBool::new(false),
            subagent_seq: AtomicU64::new(0),
        })
    }

    /// Current publication id.
    pub fn generation_id(&self) -> u64 {
        self.current.read().expect("generation lock").id
    }

    pub(crate) fn publish_provider_state(
        &self,
        state: crate::composition::ProviderState,
    ) -> Result<(), RuntimeError> {
        let _lease = self.begin_active()?;
        *self.provider_state.write().expect("provider state") = Some(state);
        Ok(())
    }

    /// Credential-only idle publication: no MCP restart, policy reload, or selection change.
    pub(crate) fn publish_provider_credentials(
        &self,
        generation: &Generation,
    ) -> Result<(), RuntimeError> {
        let _lease = self.begin_active()?;
        let mut current = self.current.write().expect("generation lock");
        let mut next = current.config.clone();
        for (id, entry) in &generation.providers {
            if let Some(target) = next.providers.get_mut(id) {
                target.options.request_bindings = entry.options.request_bindings.clone();
                target.options.redaction_material = entry.options.redaction_material.clone();
            }
        }
        *current = Arc::new(PublishedGeneration {
            id: current.id,
            config: next,
        });
        Ok(())
    }

    /// One shared admission for root/profile/child lanes and direct runtime callers.
    pub(crate) fn admit_provider_variant(
        &self,
        catalog: &ModelCatalog,
        model: &str,
        variant: Option<&str>,
        provider: &ResponsesConfig,
    ) -> Result<(), RuntimeError> {
        let selected = provider.for_selection(model, variant);
        let captured_selection = !std::ptr::eq(selected, provider);
        let published = self.current.read().expect("generation lock").clone();
        let admitted = provider
            .wire
            .provider_state
            .clone()
            .or_else(|| self.provider_state.read().expect("provider state").clone());
        let availability_known = admitted.is_some();
        let mut state = admitted.unwrap_or_else(|| {
            // Standalone public constructors still obey a declared config
            // policy. TurnParams cannot manufacture a key to bypass it. The
            // native application publishes resolved auth separately; admitted
            // model captures may have independent scoped credentials.
            let declared = published
                .config
                .providers
                .get(&catalog.provider)
                // Legacy standalone callers may supply budget metadata only;
                // without a declared connection TurnParams owns the transport.
                .filter(|entry| !entry.options.base_url.is_empty());
            let policy = declared
                .map(|entry| entry.options.auth_policy)
                .unwrap_or(selected.wire.auth_policy);
            let credential = declared
                .map(|entry| {
                    policy == crate::auth::AuthPolicy::None
                        || (policy == crate::auth::AuthPolicy::Key
                            && !entry.options.api_key.trim().is_empty())
                })
                .unwrap_or(selected.auth_ready());
            crate::composition::ProviderState::new(
                &catalog.provider,
                published
                    .config
                    .provenance
                    .get(&format!("provider.{}", catalog.provider))
                    .map(String::as_str)
                    .unwrap_or("native config"),
                credential,
                false,
            )
            .with_auth_policy(policy)
        });
        if captured_selection {
            state.set_auth(selected.auth_ready(), selected.wire.auth_policy);
        }
        if selected.wire.unsupported {
            state.set_unsupported();
        }
        state
            // With no native owner publication, preserve existing model/budget
            // validation contracts; credential admission is still central.
            .admit(
                model,
                !availability_known || catalog.models.contains_key(model),
            )
            .map_err(RuntimeError::ProviderUnavailable)?;
        if !selected.auth_ready() {
            let missing = crate::composition::ProviderState::new(
                &catalog.provider,
                published
                    .config
                    .provenance
                    .get(&format!("provider.{}", catalog.provider))
                    .map(String::as_str)
                    .unwrap_or("native config"),
                false,
                false,
            )
            .with_auth_policy(selected.wire.auth_policy);
            missing
                .admit(model, true)
                .map_err(RuntimeError::ProviderUnavailable)?;
        }
        crate::provider::request_headers(selected)
            .map_err(|_| RuntimeError::InvalidArgs("invalid provider HTTP configuration".into()))?;
        Ok(())
    }

    pub fn register_approval_consumer(&self, auto_once: bool) {
        self.approvals.register_consumer(auto_once);
        if auto_once
            && let Some(events) = self.compaction_events.lock().expect("events lock").clone()
        {
            for request in self.approvals.pending() {
                let _ = self.approvals.resolve(
                    oc_core::approval::ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: oc_core::approval::ApprovalDecision::Once,
                    },
                    &events,
                );
            }
        }
    }
    pub fn pending_approvals(&self) -> Vec<oc_core::approval::ApprovalRequest> {
        self.approvals.pending()
    }
    pub fn register_question_consumer(&self) {
        self.questions.register_consumer();
    }
    pub fn pending_questions(&self) -> Vec<oc_core::question::QuestionRequest> {
        self.questions.pending()
    }
    pub fn reply_question(
        &self,
        reply: oc_core::question::QuestionReply,
    ) -> Result<(), oc_core::question::QuestionReplyError> {
        use oc_core::question::QuestionReplyError;
        if reply.binding.generation != self.generation_id()
            || reply.binding.location != self.location
        {
            if let Some(source) = self.child_jobs.source_for(&reply.binding) {
                return source.reply_question(reply);
            }
            return Err(QuestionReplyError::BindingMismatch);
        }
        let events = self
            .compaction_events
            .lock()
            .expect("events lock")
            .clone()
            .ok_or(QuestionReplyError::Unavailable)?;
        self.questions.resolve(reply, &events)
    }
    pub(crate) fn cancel_pending_approvals(&self) {
        if let Some(events) = self.compaction_events.lock().expect("events lock").as_ref() {
            self.approvals.cancel(None, events);
            self.questions.cancel(None, events);
        }
    }
    /// Bind live hints to the same owner event bus used by application queries.
    pub fn set_approval_events(
        &self,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) {
        *self.compaction_events.lock().expect("events lock") = Some(events.clone());
        self.mcp_owner().bind_events(events);
    }
    pub fn reply_approval(
        &self,
        reply: oc_core::approval::ApprovalReply,
    ) -> Result<(), RuntimeError> {
        use oc_core::approval::ApprovalDecision;
        let request = self
            .approvals
            .pending()
            .into_iter()
            .find(|r| r.id == reply.id && r.binding == reply.binding)
            .ok_or_else(|| {
                RuntimeError::InvalidArgs("stale or mismatched approval reply".into())
            })?;
        if request.binding.generation != self.generation_id()
            || request.binding.location != self.location
        {
            if let Some(source) = self.child_jobs.source_for(&reply.binding) {
                return source.reply_approval(reply);
            }
            return Err(RuntimeError::InvalidArgs(
                "stale approval generation".into(),
            ));
        }
        if crate::approval::project_identity(&self.roots.project)
            .map_err(RuntimeError::InvalidArgs)?
            != request.project
        {
            return Err(RuntimeError::InvalidArgs("approval project changed".into()));
        }
        if matches!(reply.decision, ApprovalDecision::Cancelled) {
            return Err(RuntimeError::InvalidArgs(
                "consumer cannot synthesize cancellation".into(),
            ));
        }
        if let ApprovalDecision::Reject {
            feedback: Some(feedback),
        } = &reply.decision
            && feedback.len() > 16 * 1024
        {
            return Err(RuntimeError::InvalidArgs(
                "approval feedback too large".into(),
            ));
        }
        let events = self
            .compaction_events
            .lock()
            .expect("events lock")
            .clone()
            .ok_or_else(|| RuntimeError::InvalidArgs("approval owner unavailable".into()))?;
        let always = reply.decision == ApprovalDecision::Always;
        if always && request.save_patterns.is_empty() {
            return Err(RuntimeError::InvalidArgs(
                "approval has no save patterns".into(),
            ));
        }
        let plain_reject = matches!(&reply.decision, ApprovalDecision::Reject { feedback: None });
        self.approvals
            .resolve_with(reply, &events, |request| {
                if always {
                    self.db
                        .save_permission_grants(
                            &request.project,
                            &request.action,
                            &request.save_patterns,
                        )
                        .map_err(|_| "permission grant commit failed")?;
                }
                Ok(())
            })
            .map_err(|e| RuntimeError::InvalidArgs(e.into()))?;
        if plain_reject {
            self.approvals
                .cancel(Some(&request.binding.session), &events);
        }
        Ok(())
    }

    fn preflight_compression(
        &self,
        session: &str,
        call: &crate::tools::ToolCall,
        request_available: bool,
    ) -> Result<crate::dcp::CompressionPlan, String> {
        let config = self.dcp_config.read().expect("dcp lock").clone();
        if !request_available {
            return Err("compress excluded by issuing request".into());
        }
        let (_, ranges) =
            crate::dcp::validate_range_args(&call.arguments).map_err(|e| e.to_string())?;
        let spec = self
            .dcp_protected
            .read()
            .expect("dcp protection lock")
            .clone();
        self.prepare_dcp_plan(session, &ranges, &spec, Some(&config))
            .map_err(|e| e.to_string())
    }

    pub(crate) fn compression_availability(
        &self,
        session: &str,
    ) -> Result<oc_core::dcp_view::DcpAvailability, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.session_compression_lane(session, &published)?;
        let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules);
        self.compression_availability_with_policy(session, &policy)
    }

    /// Current query resolves the requested session's profile without rebinding
    /// a prepared request or mutating the published execution workspace.
    pub(crate) fn compression_availability_for_profile(
        &self,
        session: &str,
        profile: Option<(
            &BTreeMap<String, Permission>,
            &crate::permissions::PermissionRules,
        )>,
    ) -> Result<oc_core::dcp_view::DcpAvailability, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.session_compression_lane(session, &published)?;
        let mut rules = lane.permission_rules;
        if let Some((permissions, profile_rules)) = profile {
            rules.narrow(&lane.permissions, permissions, profile_rules);
        }
        self.compression_availability_with_policy(
            session,
            &RuntimePolicy::with_rules(&lane.permissions, &rules),
        )
    }

    fn compression_availability_with_policy(
        &self,
        session: &str,
        policy: &RuntimePolicy<'_>,
    ) -> Result<oc_core::dcp_view::DcpAvailability, RuntimeError> {
        let config = self.dcp_config.read().expect("dcp lock");
        let permission = effective_compress_permission(&config, policy);
        let child = self.db.session_meta(session)?.parent_id.is_some();
        let ordinary_refusal = self.compression_refusal(&config, permission, child, false);
        let manual_refusal = self.compression_refusal(&config, permission, child, true);
        Ok(oc_core::dcp_view::DcpAvailability {
            ordinary_refusal,
            manual_refusal,
        })
    }

    fn compression_refusal(
        &self,
        config: &DcpConfig,
        permission: Permission,
        child: bool,
        explicit: bool,
    ) -> Option<oc_core::dcp_view::DcpUnavailable> {
        config
            .compression_refusal(permission, child, explicit)
            .or_else(|| {
                (permission == Permission::Ask && !self.approvals.has_consumer())
                    .then_some(oc_core::dcp_view::DcpUnavailable::ApprovalConsumerRequired)
            })
    }

    pub(crate) fn admit_manual_compression(
        &self,
        session: &str,
    ) -> Result<ManualCompressionTrigger, RuntimeError> {
        self.open_session(session)?;
        if !self.dcp_config.read().expect("dcp lock").commands_enabled {
            return Err(RuntimeError::Compress(
                oc_core::dcp_view::DcpUnavailable::CommandsOff
                    .reason()
                    .into(),
            ));
        }
        if let Some(reason) = self.compression_availability(session)?.manual_refusal {
            return Err(RuntimeError::Compress(reason.reason().into()));
        }
        Ok(ManualCompressionTrigger {
            session: session.into(),
            generation: self.generation_id(),
        })
    }

    fn dcp_debug(&self, event: &'static str) {
        if self.dcp_config.read().expect("dcp lock").debug {
            crate::trace::log(
                "dcp.debug",
                &crate::dcp_auto::debug_line(event, &self.stats.lock().expect("stats lock")),
            );
        }
    }

    /// Owning Location.
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Capture the trusted file root and its owning Location together before
    /// handing a synchronous walk to the blocking pool. A later switch cannot
    /// rebind this clone to a different project.
    pub(crate) fn file_suggestion_source(&self) -> (crate::files::Files, String) {
        (self.files.clone(), self.location.clone())
    }

    /// Atomically publish a fully built candidate generation. Only lands
    /// between turns; returns the new id.
    pub async fn reload(&self, generation: Generation) -> Result<u64, RuntimeError> {
        let _lease = self.begin_active()?;
        self.shutdown_mcp().await?;
        // This low-level replacement has no newly pinned source descriptors.
        // Application reload builds a complete Composition/runtime instead.
        self.mcp_activation.write().expect("MCP activation").take();
        let id = {
            let mut current = self.current.write().expect("generation lock");
            let id = current.id + 1;
            *current = Arc::new(PublishedGeneration {
                id,
                config: generation,
            });
            id
        };
        self.replace_mcp_owner();
        self.start_mcp()?;
        Ok(id)
    }

    /// Claim the single-flight lease; the guard releases it on every path,
    /// including a future dropped mid-operation.
    fn begin_active(&self) -> Result<ActiveLease<'_>, RuntimeError> {
        if self.active.swap(true, Ordering::SeqCst) {
            return Err(RuntimeError::TurnActive);
        }
        Ok(ActiveLease(&self.active))
    }

    /// Close the current Location/config generation's MCP resources.
    pub async fn shutdown_mcp(&self) -> Result<(), RuntimeError> {
        if let Some(events) = self.compaction_events.lock().expect("events lock").clone() {
            self.approvals.cancel(None, &events);
            self.questions.cancel(None, &events);
        }
        let owner = self.mcp_owner();
        if owner.remote_unknown() {
            self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
        }
        if owner.cleanup_failed() {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
        }
        if let Err(error) = owner.stop().await {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
            return Err(error);
        }
        // Cancellation of an owned lookup during retirement can discover an
        // unknown remote outcome; transfer it after every job was joined.
        if owner.remote_unknown() {
            self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
        }
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            Err(RuntimeError::McpShutdown)
        } else {
            Ok(())
        }
    }

    fn mcp_owner(&self) -> Arc<mcp::McpOwner> {
        self.mcp_generation.read().expect("MCP owner").clone()
    }

    fn replace_mcp_owner(&self) {
        let owner = Arc::new(mcp::McpOwner::new(&self.location, self.generation_id()));
        if let Some(events) = self.compaction_events.lock().expect("events lock").as_ref() {
            owner.bind_events(events);
        }
        *self.mcp_generation.write().expect("MCP owner") = owner;
    }

    /// Launch independent admitted initial connections before application ready.
    /// Target Location runtimes call this only after quarantine is transferred.
    pub fn start_mcp(&self) -> Result<(), RuntimeError> {
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpShutdown);
        }
        let mut config = self.current.read().expect("generation lock").config.clone();
        if self.remote_retry_quarantined() {
            for (server, entry) in &mut config.mcp {
                if entry.kind == "remote" && entry.enabled {
                    let source = config
                        .provenance
                        .get(&format!("mcp.{server}"))
                        .map(String::as_str)
                        .unwrap_or("native config");
                    let mut failure = crate::config::mcp::failure(
                        server,
                        source,
                        "connection",
                        oc_core::queries::ServiceCode::UnsafeRetry,
                    );
                    failure.stage = oc_core::queries::ServiceStage::Call;
                    failure.action = oc_core::queries::ServiceAction::RestartApplication;
                    entry.failure = Some(failure);
                }
            }
        }
        self.mcp_owner().start(
            config,
            self.roots.project.clone(),
            self.parent_env.clone(),
            self.mcp_activation.read().expect("MCP activation").clone(),
        )
    }

    pub fn mcp_status(&self) -> oc_core::queries::McpSnapshot {
        self.mcp_owner().snapshot()
    }

    pub(crate) async fn wait_mcp_failure(&self) -> RuntimeError {
        self.mcp_owner().wait_failure().await
    }

    pub(crate) fn mcp_control_server(
        &self,
        control: &oc_core::queries::McpControl,
    ) -> Result<String, RuntimeError> {
        let config = &self.current.read().expect("generation lock").config;
        let server = self.mcp_owner().server_name(control, config)?;
        if control.action != oc_core::queries::McpAction::Disconnect
            && config.mcp[&server].kind == "remote"
            && self.remote_retry_quarantined()
        {
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        Ok(server)
    }

    pub(crate) fn enqueue_mcp_control(
        &self,
        control: oc_core::queries::McpControl,
        ack: tokio::sync::oneshot::Sender<
            Result<oc_core::queries::McpSnapshot, oc_core::session::CoreError>,
        >,
    ) {
        self.mcp_owner().control(control, ack);
    }

    pub(crate) fn mcp_lookup_server(
        &self,
        query: &oc_core::queries::McpLookup,
    ) -> Result<String, oc_core::queries::McpLookupError> {
        self.mcp_owner().lookup_server(query)
    }
    pub(crate) fn enqueue_mcp_lookup(
        &self,
        query: oc_core::queries::McpLookup,
        cancel: Arc<AtomicBool>,
        ack: tokio::sync::oneshot::Sender<
            Result<oc_core::queries::McpLookupReply, oc_core::queries::McpLookupError>,
        >,
    ) {
        self.mcp_owner().enqueue_lookup(query, cancel, ack);
    }

    pub(crate) fn set_mcp_activation(&self, activation: Arc<crate::composition::McpActivation>) {
        *self.mcp_activation.write().expect("MCP activation") = Some(activation);
    }

    async fn retire_poisoned_mcp(&self) -> Result<(), RuntimeError> {
        if let Some(error) = self.mcp_owner().fatal() {
            return Err(error);
        }
        if self.mcp_owner().poisoned() {
            self.shutdown_mcp().await?;
            self.replace_mcp_owner();
            self.start_mcp()?;
        }
        Ok(())
    }

    async fn request_mcp(&self, cancel: &AtomicBool) -> Result<Arc<McpGeneration>, RuntimeError> {
        if self.mcp_owner().remote_unknown() {
            self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        self.retire_poisoned_mcp().await?;
        self.start_mcp()?;
        if self.remote_retry_quarantined()
            && self
                .current
                .read()
                .expect("generation lock")
                .config
                .mcp
                .values()
                .any(|entry| entry.enabled && entry.kind == "remote")
        {
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        self.mcp_owner().request(cancel).await
    }

    /// Owner-only status, read after shutdown has retired any poisoned MCP
    /// generation. The application carries it across Location runtimes.
    pub(crate) fn remote_retry_quarantined(&self) -> bool {
        self.mcp_unsafe_retry.load(Ordering::SeqCst) || self.child_jobs.remote_unknown()
    }

    pub(crate) fn quarantine_remote_retries(&self) {
        self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
    }

    /// Replace the DCP config between turns.
    pub fn reload_dcp(&self, config: DcpConfig) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        *self.dcp_config.write().expect("dcp lock") = config;
        Ok(())
    }

    /// Publish context-preservation rules independently of file permissions.
    pub fn publish_dcp_protection(&self, spec: ProtectedSpec) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        *self.dcp_protected.write().expect("dcp protection lock") = spec;
        Ok(())
    }

    /// Publish pinned skill `(id, SKILL.md)` files between turns.
    ///
    /// The next turn snapshots these for the native `skill` tool;
    /// invalid entries stay out so calls fail visibly as unknown.
    pub fn publish_skills(&self, files: Vec<(String, String)>) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        let snapshot = SkillSnapshot::build(&files).0;
        self.workspace.write().expect("workspace lock").skills = snapshot;
        Ok(())
    }

    /// Publish fixed instructions, primary prompt/policy and pinned skills together.
    #[allow(clippy::too_many_arguments)]
    pub fn publish_workspace(
        &self,
        agent_prompt: Option<&str>,
        instructions: &str,
        files: Vec<(String, String)>,
        skill_errors: BTreeMap<String, String>,
        agent_digest: Option<String>,
        agent_id: Option<String>,
        agent_color_index: Option<usize>,
        agent_permissions: BTreeMap<String, Permission>,
        mut agent_permission_rules: crate::permissions::PermissionRules,
        agent_request: crate::provider::RequestOverlay,
    ) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        let (mut skills, warnings) = SkillSnapshot::build(&files);
        if !warnings.is_empty() {
            return Err(RuntimeError::InvalidArgs(warnings.join("; ")));
        }
        skills.errors = skill_errors;
        let projection = skills.projection();
        let skills_projection = if projection.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&projection).map_err(|_| RuntimeError::Storage)?)
        };
        let shared_sources = !self
            .workspace
            .read()
            .expect("workspace lock")
            .instruction_roots
            .is_empty();
        // The skills preview is appended per lane after its final policy.
        let fixed_input = lane_fixed_input(
            agent_prompt,
            if shared_sources { "" } else { instructions },
            None,
        );
        if let Some(home) = self.parent_env.get("HOME") {
            agent_permission_rules.expand_home(home);
        }
        agent_permission_rules.bind_plan_project(&self.roots.project);
        let mut workspace = self.workspace.write().expect("workspace lock");
        workspace.fixed_input = fixed_input;
        workspace.skills = skills;
        workspace.agent_digest = agent_digest;
        workspace.agent_permissions = agent_permissions;
        workspace.agent_permission_rules = agent_permission_rules;
        workspace.agent_request = agent_request;
        workspace.agent_id = agent_id;
        workspace.agent_color_index = agent_color_index;
        workspace.instructions = if shared_sources {
            String::new()
        } else {
            instructions.to_string()
        };
        workspace.skills_projection = skills_projection;
        Ok(())
    }

    /// Publish the subagent catalog + depth budget between turns.
    pub fn publish_subagents(
        &self,
        subagents: Option<SubagentCatalog>,
    ) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        self.workspace.write().expect("workspace lock").subagents = subagents;
        Ok(())
    }

    /// Pin command source metadata for child recovery between turns.
    pub(crate) fn publish_command_digest(&self, digest: String) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        self.workspace
            .write()
            .expect("workspace lock")
            .command_digest = Some(digest);
        Ok(())
    }

    /// Runtime DCP stats snapshot (counts only).
    pub fn dcp_stats(&self) -> DcpStats {
        self.stats.lock().expect("stats lock").clone()
    }

    /// True while conversational or standalone compaction execution owns the route.
    pub fn turn_active(&self) -> bool {
        self.active.load(Ordering::Relaxed) || self.compaction_active()
    }

    pub(crate) fn publish_model_selection(
        &self,
        session: oc_core::domain::SessionId,
        commit: oc_core::queries::ModelCommit,
    ) {
        if let Some(events) = self
            .compaction_events
            .lock()
            .expect("event publisher")
            .as_ref()
        {
            let _ =
                events.send(oc_core::core_app::CoreEvent::SessionModelSelected { session, commit });
        }
    }

    /// Per-session DCP turn counters, if the session ever ran a turn.
    pub fn dcp_turn_state(&self, session: &str) -> Option<NudgeState> {
        let prefix = format!("dcp.nudge.{session}\0");
        self.nudge_state
            .lock()
            .expect("nudge lock")
            .iter()
            .find(|(key, _)| key.starts_with(&prefix))
            .map(|(_, state)| state.clone())
    }

    /// Create a Location-scoped session (idempotent for the same Location).
    pub fn create_session(&self, id: &str) -> Result<(), RuntimeError> {
        let reminder = (self
            .workspace
            .read()
            .expect("workspace lock")
            .agent_id
            .as_deref()
            == Some("plan"))
        .then(|| crate::plan::enter(self.parent_env.get("HOME").map(String::as_str)));
        match self
            .db
            .create_bound_session_with_reminder(id, &self.location, reminder.as_deref())?
        {
            BoundSessionCreation::Created | BoundSessionCreation::AlreadyBound => Ok(()),
            BoundSessionCreation::BoundElsewhere(owner) => Err(RuntimeError::LocationMismatch {
                session: id.to_string(),
                location: owner,
            }),
        }
    }

    /// Open a session after verifying its Location binding.
    pub fn open_session(&self, id: &str) -> Result<(), RuntimeError> {
        match self.db.get_pref(&session_location_key(id))? {
            Some(owner) if owner == self.location => Ok(()),
            Some(owner) => Err(RuntimeError::LocationMismatch {
                session: id.to_string(),
                location: owner,
            }),
            None => Err(RuntimeError::SessionNotFound),
        }
    }

    /// Run one generation-guarded turn to durable records.
    pub async fn run_turn(&self, params: TurnParams<'_>) -> Result<TurnReport, RuntimeError> {
        self.run_turn_with_events(params, |_| {}, |_, _| {}, |_, _| {})
            .await
    }

    /// Notify the application only after validated input is durably accepted.
    /// `text_delta` and `reasoning_delta` forward provider deltas while the
    /// turn streams; reasoning text is never persisted, only projected.
    /// Tool-call lifecycle notifications are not forwarded here (use
    /// [`Runtime::run_turn_with_tool_events`] when a frontend renders cards).
    pub async fn run_turn_with_events(
        &self,
        params: TurnParams<'_>,
        accepted: impl FnMut(&str) + Send,
        text_delta: impl FnMut(&str, &str) + Send,
        reasoning_delta: impl FnMut(&str, &str) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        self.run_turn_with_tool_events(params, accepted, text_delta, reasoning_delta, |_, _| {})
            .await
    }

    /// Like [`Runtime::run_turn_with_events`] but also forwards one
    /// [`ToolCallEvent`] per recorded tool intent/outcome, in execution order,
    /// plus bounded live-only argument snapshots. Only `Started`, `Finished`
    /// and argument `Linked` imply a durable record; `Pending` never does.
    pub async fn run_turn_with_tool_events(
        &self,
        params: TurnParams<'_>,
        accepted: impl FnMut(&str) + Send,
        text_delta: impl FnMut(&str, &str) + Send,
        reasoning_delta: impl FnMut(&str, &str) + Send,
        tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        self.run_turn_with_reasoning_items(
            params,
            accepted,
            text_delta,
            reasoning_delta,
            |_| {},
            tool_event,
        )
        .await
    }

    /// Also forwards the end of each public reasoning output item, without its
    /// opaque continuation payload or provider id.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_turn_with_reasoning_items(
        &self,
        params: TurnParams<'_>,
        mut accepted: impl FnMut(&str) + Send,
        text_delta: impl FnMut(&str, &str) + Send,
        reasoning_delta: impl FnMut(&str, &str) + Send,
        reasoning_item_ended: impl FnMut(&str) + Send,
        tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        self.run_turn_with_reasoning_items_and_notice(
            params,
            None,
            None,
            |id, _| accepted(id),
            text_delta,
            reasoning_delta,
            reasoning_item_ended,
            tool_event,
        )
        .await
    }

    /// Application-owner callback receives the notice from the committed
    /// acceptance transaction, never from a post-commit history lookup.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_turn_with_reasoning_items_and_notice(
        &self,
        params: TurnParams<'_>,
        manual: Option<ManualCompressionTrigger>,
        command: Option<&NativeCommand>,
        mut accepted: impl FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send,
        mut text_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_item_ended: impl FnMut(&str) + Send,
        mut tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        let started = std::time::Instant::now();
        let _lease = self.begin_active()?;
        self.admit_provider_variant(
            params.catalog,
            &params.model_id,
            params.variant.as_deref(),
            &params.provider,
        )?;
        let published = self.current.read().expect("generation lock").clone();
        let mut lane = self.primary_lane(&published);
        if let Some(trigger) = manual {
            if trigger.session != params.session || trigger.generation != published.id {
                return Err(RuntimeError::Compress(
                    "stale manual compression scope".into(),
                ));
            }
            if let Some(reason) = self
                .compression_availability(&params.session)?
                .manual_refusal
            {
                return Err(RuntimeError::Compress(reason.reason().into()));
            }
            lane.manual_compression = true;
        }
        let attached = self.request_mcp(params.cancel).await?;
        // request_mcp may replace a poisoned owner. Bind diagnostics only after
        // it has returned the exact immutable lease for this turn.
        let mcp_owner = self.mcp_owner();
        let mcp_binding = mcp_owner.snapshot().binding;
        let parent = params.session.clone();
        let cancel = params.cancel;
        let result = self
            .child_jobs
            .observe_parent(
                &parent,
                cancel,
                self.run_turn_inner(
                    params,
                    &lane,
                    &attached,
                    None,
                    command,
                    &mut accepted,
                    &mut text_delta,
                    &mut reasoning_delta,
                    &mut reasoning_item_ended,
                    &mut tool_event,
                ),
            )
            .await;
        let mcp_warnings = mcp_owner.completion_warnings(&mcp_binding, &attached);
        drop(attached);
        self.retire_poisoned_mcp().await?;
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        result.and_then(|mut report| {
            report.duration_ms = duration_ms;
            let service_start = report.warnings.len();
            report.warnings.extend(mcp_warnings);
            report.service_warning_range = service_start..report.warnings.len();
            self.db.update_turn_display(
                &report.turn_id,
                &serde_json::json!({
                    "duration_ms": duration_ms, "streamed_ms": report.streamed_ms,
                    "usage": report.usage, "context_usage": report.context_usage,
                }),
            )?;
            Ok(report)
        })
    }

    /// Accept the first turn of a new Location-bound root in one transaction.
    /// `params.session` must be an unused candidate id. `initial_selection`, if
    /// present, is an application-prevalidated session-scoped preference pair;
    /// its key must target this id. The acceptance callback runs only after the
    /// root, binding, selection, turn and user message have committed.
    #[allow(clippy::too_many_arguments)]
    pub async fn run_fresh_turn_with_tool_events(
        &self,
        params: TurnParams<'_>,
        initial_selection: Option<(&str, &str)>,
        accepted: impl FnMut(&str) + Send,
        text_delta: impl FnMut(&str, &str) + Send,
        reasoning_delta: impl FnMut(&str, &str) + Send,
        tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        self.run_fresh_turn_with_reasoning_items(
            params,
            initial_selection,
            accepted,
            text_delta,
            reasoning_delta,
            |_| {},
            tool_event,
        )
        .await
    }

    /// Fresh-root equivalent of [`Runtime::run_turn_with_reasoning_items`].
    #[allow(clippy::too_many_arguments)]
    pub async fn run_fresh_turn_with_reasoning_items(
        &self,
        params: TurnParams<'_>,
        initial_selection: Option<(&str, &str)>,
        mut accepted: impl FnMut(&str) + Send,
        text_delta: impl FnMut(&str, &str) + Send,
        reasoning_delta: impl FnMut(&str, &str) + Send,
        reasoning_item_ended: impl FnMut(&str) + Send,
        tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        self.run_fresh_turn_with_reasoning_items_and_notice(
            params,
            initial_selection,
            None,
            |id, _| accepted(id),
            text_delta,
            reasoning_delta,
            reasoning_item_ended,
            tool_event,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_fresh_turn_with_reasoning_items_and_notice(
        &self,
        params: TurnParams<'_>,
        initial_selection: Option<(&str, &str)>,
        command: Option<&NativeCommand>,
        mut accepted: impl FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send,
        mut text_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_item_ended: impl FnMut(&str) + Send,
        mut tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        let started = std::time::Instant::now();
        let _lease = self.begin_active()?;
        self.admit_provider_variant(
            params.catalog,
            &params.model_id,
            params.variant.as_deref(),
            &params.provider,
        )?;
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        let attached = self.request_mcp(params.cancel).await?;
        let mcp_owner = self.mcp_owner();
        let mcp_binding = mcp_owner.snapshot().binding;
        let parent = params.session.clone();
        let cancel = params.cancel;
        let result = self
            .child_jobs
            .observe_parent(
                &parent,
                cancel,
                self.run_turn_inner(
                    params,
                    &lane,
                    &attached,
                    Some(initial_selection),
                    command,
                    &mut accepted,
                    &mut text_delta,
                    &mut reasoning_delta,
                    &mut reasoning_item_ended,
                    &mut tool_event,
                ),
            )
            .await;
        let mcp_warnings = mcp_owner.completion_warnings(&mcp_binding, &attached);
        // A committed fresh root must remain observable through the acceptance
        // callback even if cleanup or display-metadata writes later fail.
        drop(attached);
        let cleanup = self.retire_poisoned_mcp().await;
        cleanup?;
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        result.and_then(|mut report| {
            report.duration_ms = duration_ms;
            let service_start = report.warnings.len();
            report.warnings.extend(mcp_warnings);
            report.service_warning_range = service_start..report.warnings.len();
            self.db.update_turn_display(
                &report.turn_id,
                &serde_json::json!({
                    "duration_ms": duration_ms, "streamed_ms": report.streamed_ms,
                    "usage": report.usage, "context_usage": report.context_usage,
                }),
            )?;
            Ok(report)
        })
    }

    /// Fixed input + policy of the primary (published) lane.
    fn primary_lane(&self, published: &PublishedGeneration) -> TurnLane {
        let workspace = self.workspace.read().expect("workspace lock");
        let permissions = published.config.permissions.clone();
        let mut permission_rules = published.config.permission_rules.clone();
        permission_rules.narrow(
            &permissions,
            &workspace.agent_permissions,
            &workspace.agent_permission_rules,
        );
        permission_rules.bind_plan_project(&self.roots.project);
        let mut fixed_input = workspace.fixed_input.clone();
        fixed_input.extend(skills_input(
            &workspace.skills,
            &permissions,
            &permission_rules,
        ));
        TurnLane {
            manual_compression: false,
            owning_operation: None,
            agent_id: workspace.agent_id.clone(),
            agent_color_index: workspace.agent_color_index,
            fixed_input,
            agent_digest: workspace.agent_digest.clone(),
            permissions,
            permission_rules,
            request: workspace.agent_request.clone(),
        }
    }

    /// Query/direct API authority follows the requested child's lineage, never
    /// a root-only lane. Actual turns keep their already captured ChildLane.
    fn session_compression_lane(
        &self,
        session: &str,
        published: &PublishedGeneration,
    ) -> Result<TurnLane, RuntimeError> {
        let mut lane = self.primary_lane(published);
        let catalog = self
            .workspace
            .read()
            .expect("workspace lock")
            .subagents
            .clone();
        let mut current = self.db.session_meta(session)?;
        let mut profiles = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        while let Some(parent) = current.parent_id.clone() {
            if profiles.len() >= 64 || !seen.insert(parent.clone()) {
                return Err(RuntimeError::InvalidArgs(
                    "child authority lineage invalid".into(),
                ));
            }
            profiles.push(current.agent.clone());
            current = self.db.session_meta(&parent)?;
        }
        if profiles.is_empty() {
            return Ok(lane);
        }
        // Include the stored parent's profile as well as the currently selected
        // root ceiling. A switch in the parent's UI cannot widen an old child.
        if current.agent.is_some() {
            profiles.push(current.agent);
        }
        for id in profiles.into_iter().rev() {
            if let Some(agent) = id.as_ref().and_then(|id| catalog.as_ref()?.agents.get(id)) {
                lane = self.child_lane(agent, &lane, "compression-query");
            } else {
                lane.permission_rules
                    .module_permission(COMPRESS_TOOL, Permission::Deny);
            }
        }
        Ok(lane)
    }

    /// Saved DCP preferences were restored by the owner; refresh lazy caches.
    pub(crate) fn conversation_changed(&self, session: &str) {
        let prefix = format!("dcp.nudge.{session}\0");
        let restored = self.db.conversation_nudges(session).unwrap_or_default();
        let mut states = self.nudge_state.lock().expect("nudge lock");
        states.retain(|key, _| !key.starts_with(&prefix));
        for (key, raw) in restored {
            if let Ok(state) = serde_json::from_str(&raw) {
                states.insert(key, state);
            }
        }
    }
}

fn effective_compress_permission(config: &DcpConfig, policy: &RuntimePolicy<'_>) -> Permission {
    let effective = policy.effect(COMPRESS_TOOL, "*");
    match config.compress_permission {
        Some(level) if turn::permission_rank(level) > turn::permission_rank(effective) => level,
        _ => effective,
    }
}

mod context;
mod instructions;
mod mcp;
mod turn;
mod user_shell;
pub(crate) use user_shell::UserShellParams;

use context::{active_summary_tokens, dcp_config_input, dcp_continuation};
pub(crate) use context::{
    apply_dcp_projection, dcp_call_contents, dcp_call_identities, dcp_contents,
};
use mcp::{McpGeneration, mcp_instruction_input};
use turn::{lane_fixed_input, skills_input};
