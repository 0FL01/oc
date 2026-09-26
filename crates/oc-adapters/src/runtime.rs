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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oc_core::context_plan::ProtectedSpec;
use oc_core::session::{Message, MessageId, Role};

use crate::config::{Generation, Permission};
use crate::dcp_auto::{DcpConfig, DcpStats, NudgeState, evaluate};
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

/// Max tool rounds per turn (bounded agent loop).
pub const MAX_ROUNDS: u32 = 8;
/// Hard cap for a caller-supplied round limit.
pub const ROUND_CAP: u32 = 16;
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
    /// Provider failure (kind only).
    Provider,
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

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
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
            Self::Provider => write!(f, "provider error"),
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
/// `Ask` fails everywhere: headless has no approval channel and T24 builds
/// none for the TUI either — the denial names the tool.
pub struct RuntimePolicy<'a> {
    permissions: &'a BTreeMap<String, Permission>,
    rules: Option<&'a crate::permissions::PermissionRules>,
    root: Option<&'a std::path::Path>,
    mcp: &'a [mcp_remote::RegistryEntry],
}

impl<'a> RuntimePolicy<'a> {
    /// Bridge one permission map.
    pub fn new(permissions: &'a BTreeMap<String, Permission>) -> Self {
        Self {
            permissions,
            rules: None,
            root: None,
            mcp: &[],
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
        let alias = self
            .mcp
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
            });
        let mut actions = vec![tool];
        if let Some(alias) = &alias {
            actions.push(alias);
        }
        self.rules.map_or_else(
            || {
                crate::permissions::PermissionRules::default().evaluate_actions(
                    self.permissions,
                    &actions,
                    resource,
                )
            },
            |rules| rules.evaluate_actions(self.permissions, &actions, resource),
        )
    }
}

impl ToolPolicy for RuntimePolicy<'_> {
    fn check(&self, tool: &str) -> Result<(), ToolError> {
        self.check_resource(tool, "*")
    }

    fn check_resource(&self, tool: &str, resource: &str) -> Result<(), ToolError> {
        let normalized;
        let resource = if matches!(tool, "read" | "apply_patch") && resource != "*" {
            normalized = permission_path(self.root, resource);
            normalized.as_str()
        } else {
            resource
        };
        match self.effect(tool, resource) {
            Permission::Allow => Ok(()),
            Permission::Ask => Err(ToolError::ApprovalRequired {
                tool: tool.to_string(),
            }),
            Permission::Deny => Err(ToolError::Denied {
                tool: tool.to_string(),
            }),
        }
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
            name: "read".to_string(),
            description: "Read a project file slice. `path` is relative to the project root; `offset` is the 1-based first line, `limit` caps lines (default 50)."
                .to_string(),
            parameters: schema(
                serde_json::json!({
                    "path": {"type": "string"},
                    "offset": {"type": "integer"},
                    "limit": {"type": "integer"},
                }),
                &["path"],
            ),
        },
        ToolDef {
            name: "glob".to_string(),
            description: "List project paths matching a bounded glob. Results are sorted and paginated.".to_string(),
            parameters: schema(
                serde_json::json!({
                    "pattern": {"type": "string"},
                    "offset": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
                }),
                &["pattern"],
            ),
        },
        ToolDef {
            name: "grep".to_string(),
            description: "Search project text with a bounded literal pattern. Results are sorted and paginated; regex mode is unsupported.".to_string(),
            parameters: schema(
                serde_json::json!({
                    "pattern": {"type": "string"},
                    "literal": {"type": "boolean"},
                    "offset": {"type": "integer", "minimum": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 1000}
                }),
                &["pattern"],
            ),
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
            name: "bash".to_string(),
            description: "Run a supervised command. `argv` is the exact executable plus args (no shell); omit `cwd` to run in the project root — any `cwd` must stay inside the project root. `timeout_ms` caps the run (default 30000)."
                .to_string(),
            parameters: schema(
                serde_json::json!({
                    "argv": {"type": "array", "items": {"type": "string"}},
                    "cwd": {"type": "string"},
                    "timeout_ms": {"type": "integer"},
                }),
                &["argv"],
            ),
        },
        ToolDef {
            name: "webfetch".to_string(),
            description: "Fetch a URL as text".to_string(),
            parameters: schema(serde_json::json!({"url": {"type": "string"}}), &["url"]),
        },
        ToolDef {
            name: "skill".to_string(),
            description: "Invoke a pinned skill".to_string(),
            parameters: schema(serde_json::json!({"id": {"type": "string"}}), &["id"]),
        },
        ToolDef {
            name: COMPRESS_TOOL.to_string(),
            description: "Replace one or more closed transcript ranges with durable summaries. Use only stable startId/endId anchors from the DCP context lane; never include the unfinished final anchor.".to_string(),
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

/// Attached MCP server: remote or stdio behind one call shape.
enum AttachedServer {
    Remote(CodexWebClient),
    Stdio(StdioClient),
}

/// One Location/config generation owns connected clients and its exact dispatch map.
///
/// `degraded` records enabled servers that could not be attached (or whose
/// catalog refresh failed) as sanitized `McpAttach` notices. They publish no
/// tools and never abort the turn: upstream opencode v2.0.12 keeps a per-server
/// `failed` status and continues the session.
struct McpGeneration {
    publication: u64,
    servers: Vec<AttachedMcp>,
    entries: Vec<mcp_remote::RegistryEntry>,
    degraded: Vec<RuntimeError>,
    /// An in-flight call has no proven result: this entire generation is retired.
    poisoned: AtomicBool,
    /// A remote owner cannot prove its server stopped merely by closing HTTP.
    remote_unknown: AtomicBool,
    /// The request-specific cancellation notification could not be confirmed.
    cleanup_error: AtomicBool,
}

/// If an owning turn future is dropped in the middle of an MCP call, the
/// request result is unknown. A later turn must retire the old generation.
struct McpCallLease<'a> {
    generation: &'a McpGeneration,
    db: &'a Db,
    op: &'a str,
    turn: &'a str,
    remote: bool,
    armed: bool,
}

impl Drop for McpCallLease<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.generation.poisoned.store(true, Ordering::SeqCst);
            if self.remote {
                self.generation.remote_unknown.store(true, Ordering::SeqCst);
            }
            if self
                .db
                .mark_dropped_mcp_call_unknown(self.op, self.turn)
                .is_err()
            {
                self.generation.cleanup_error.store(true, Ordering::SeqCst);
            }
        }
    }
}

impl McpGeneration {
    /// Sanitized per-server degradation notices (server id, stage, safe code).
    fn warnings(&self) -> Vec<String> {
        self.degraded.iter().map(ToString::to_string).collect()
    }

    /// Explicitly close every owned client under one generation-wide budget.
    ///
    /// A client that times out is dropped, which keeps the stdio process-group
    /// SIGKILL fallback armed; the generic budget prevents one unresponsive
    /// server from multiplying per-client timeouts across the generation.
    async fn close(self) -> Result<(), RuntimeError> {
        let deadline = tokio::time::Instant::now() + MCP_CLOSE_BUDGET;
        let mut failed = false;
        for server in self.servers {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                failed = true;
                continue;
            }
            match tokio::time::timeout(remaining, close_attached(server)).await {
                Ok(Ok(())) => {}
                Ok(Err(())) | Err(_) => failed = true,
            }
        }
        if failed {
            Err(RuntimeError::McpShutdown)
        } else {
            Ok(())
        }
    }

    /// Claim dirty servers, relist exactly those, and restore the claim on any
    /// failure. Notifications arriving during the relist stay claimed for the
    /// next turn because the flag is claimed before the request is sent.
    async fn refresh_if_changed(&mut self, cancel: &AtomicBool) -> Result<(), RuntimeError> {
        let claims = self.claim_dirty();
        if !claims.iter().any(|claimed| *claimed) {
            return Ok(());
        }
        let mut replacements: Vec<Option<Vec<mcp_remote::RegistryEntry>>> =
            vec![None; self.servers.len()];
        for (index, server) in self.servers.iter().enumerate() {
            if !claims[index] {
                continue;
            }
            let listed = match &server.client {
                AttachedServer::Remote(client) => client
                    .list_tools(cancel)
                    .await
                    .map_err(|error| remote_attach_error(&server.server_id, error)),
                AttachedServer::Stdio(client) => client
                    .list_tools(cancel)
                    .await
                    .map_err(|error| stdio_attach_error(&server.server_id, error)),
            };
            let registry = match listed {
                Ok(tools) => mcp_remote::map_registry(
                    &server.server_id,
                    tools
                        .into_iter()
                        .map(|tool| (tool.name, tool.description, tool.input_schema))
                        .collect(),
                )
                .map_err(|error| remote_attach_error(&server.server_id, error)),
                Err(error) => Err(error),
            };
            match registry {
                Ok(registry) => {
                    replacements[index] = Some(registry);
                    clear_degradation(&mut self.degraded, &server.server_id);
                }
                Err(error) => {
                    // Upstream ignores a failed relist: the previous catalog
                    // stays published and the server is reported as degraded.
                    self.restore_dirty(&claims);
                    record_degradation(&mut self.degraded, error);
                }
            }
        }
        let mut registries = Vec::with_capacity(self.servers.len());
        for (index, server) in self.servers.iter().enumerate() {
            registries.push(
                replacements[index]
                    .clone()
                    .unwrap_or_else(|| server.registry.clone()),
            );
        }
        match mcp_remote::merge_registries(registries) {
            Ok(entries) => {
                for (index, replacement) in replacements.into_iter().enumerate() {
                    if let Some(registry) = replacement {
                        self.servers[index].registry = registry;
                    }
                }
                self.entries = entries;
                Ok(())
            }
            Err(error) => {
                self.restore_dirty(&claims);
                Err(remote_attach_error("generation", error))
            }
        }
    }

    fn claim_dirty(&self) -> Vec<bool> {
        self.servers
            .iter()
            .map(|server| match &server.client {
                AttachedServer::Remote(client) => client.claim_catalog_changed(),
                AttachedServer::Stdio(client) => client.claim_catalog_changed(),
            })
            .collect()
    }

    fn restore_dirty(&self, claims: &[bool]) {
        for (index, server) in self.servers.iter().enumerate() {
            if claims[index] {
                match &server.client {
                    AttachedServer::Remote(client) => client.restore_catalog_changed(),
                    AttachedServer::Stdio(client) => client.restore_catalog_changed(),
                }
            }
        }
    }
}

/// Close one attached client with a typed failure result.
async fn close_attached(server: AttachedMcp) -> Result<(), ()> {
    match server.client {
        AttachedServer::Remote(client) => client.close().await.map_err(|_| ()),
        AttachedServer::Stdio(client) => client.shutdown().await.map_err(|_| ()),
    }
}

/// Close a generation in an owned task so cleanup still runs to completion
/// when the awaiting caller future is dropped.
async fn close_generation(generation: McpGeneration) -> Result<(), RuntimeError> {
    match tokio::spawn(generation.close()).await {
        Ok(result) => result,
        Err(_) => Err(RuntimeError::McpShutdown),
    }
}

/// Record one server's degradation, replacing any earlier notice for it.
///
/// Only sanitized `McpAttach` notices are recorded; any other error stays fatal
/// at the call site.
fn record_degradation(degraded: &mut Vec<RuntimeError>, error: RuntimeError) {
    if let RuntimeError::McpAttach { server, .. } = &error {
        clear_degradation(degraded, server);
    }
    degraded.push(error);
}

/// Drop the recorded degradation of a server that is healthy again.
fn clear_degradation(degraded: &mut Vec<RuntimeError>, server: &str) {
    degraded.retain(|existing| match existing {
        RuntimeError::McpAttach { server: known, .. } => known != server,
        _ => true,
    });
}

/// Server id + client + per-server base registry.
struct AttachedMcp {
    server_id: String,
    client: AttachedServer,
    registry: Vec<mcp_remote::RegistryEntry>,
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
/// Emitted only after the durable record exists: `Started` after the intent
/// insert (before the side effect), `Finished` after the outcome update. The
/// payloads are bounded by the recorded caps; `output` is capped at
/// [`REPORT_OUTPUT_CAP`] with `output_bytes`/`output_truncated` describing the
/// full stored value, so a frontend never sees an unbounded field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallEvent {
    /// Intent durably recorded; the call may now run.
    Started {
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// Recorded arguments JSON (bounded by the tool argument caps).
        input: String,
    },
    /// Terminal outcome durably recorded.
    Finished {
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
    /// Max tool rounds (capped at [`ROUND_CAP`]).
    pub max_rounds: u32,
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
    instructions: String,
    skills_projection: Option<String>,
    subagents: Option<SubagentCatalog>,
}

/// Fixed developer input + central policy for one turn lane.
///
/// The primary lane mirrors the published workspace. A child lane replaces
/// the agent prompt and narrows permissions with the child agent's rules.
struct TurnLane {
    agent_id: Option<String>,
    agent_color_index: Option<usize>,
    fixed_input: Vec<InputItem>,
    agent_digest: Option<String>,
    permissions: BTreeMap<String, Permission>,
    permission_rules: crate::permissions::PermissionRules,
}

/// One spawnable agent profile snapshotted for the `subagent` tool.
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// Published subagent catalog + depth budget for this Location generation.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    db: &'a Db,
    location: String,
    current: RwLock<Arc<PublishedGeneration>>,
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
    stats: Mutex<crate::dcp_auto::DcpStats>,
    mcp_generation: tokio::sync::Mutex<Option<McpGeneration>>,
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
        Ok(Self {
            db,
            location: location.to_string(),
            current: RwLock::new(Arc::new(PublishedGeneration {
                id: 1,
                config: generation,
            })),
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
            stats: Mutex::new(DcpStats::default()),
            mcp_generation: tokio::sync::Mutex::new(None),
            mcp_unsafe_retry: AtomicBool::new(false),
            mcp_cleanup_failed: AtomicBool::new(false),
            subagent_seq: AtomicU64::new(0),
        })
    }

    /// Current publication id.
    pub fn generation_id(&self) -> u64 {
        self.current.read().expect("generation lock").id
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
        let mut slot = self.mcp_generation.lock().await;
        self.retire_poisoned(&mut slot).await?;
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpShutdown);
        }
        if let Some(old) = slot.take()
            && let Err(error) = close_generation(old).await
        {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
            return Err(error);
        }
        let mut current = self.current.write().expect("generation lock");
        let id = current.id + 1;
        *current = Arc::new(PublishedGeneration {
            id,
            config: generation,
        });
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
        let mut slot = self.mcp_generation.lock().await;
        self.retire_poisoned(&mut slot).await?;
        if let Some(generation) = slot.take()
            && let Err(error) = close_generation(generation).await
        {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
            return Err(error);
        }
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            Err(RuntimeError::McpShutdown)
        } else {
            Ok(())
        }
    }

    /// Owner-only status, read after shutdown has retired any poisoned MCP
    /// generation. The application carries it across Location runtimes.
    pub(crate) fn remote_retry_quarantined(&self) -> bool {
        self.mcp_unsafe_retry.load(Ordering::SeqCst)
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
        let fixed_input =
            lane_fixed_input(agent_prompt, instructions, skills_projection.as_deref());
        if let Some(home) = self.parent_env.get("HOME") {
            agent_permission_rules.expand_home(home);
        }
        let mut workspace = self.workspace.write().expect("workspace lock");
        workspace.fixed_input = fixed_input;
        workspace.skills = skills;
        workspace.agent_digest = agent_digest;
        workspace.agent_permissions = agent_permissions;
        workspace.agent_permission_rules = agent_permission_rules;
        workspace.agent_id = agent_id;
        workspace.agent_color_index = agent_color_index;
        workspace.instructions = instructions.to_string();
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

    /// Runtime DCP stats snapshot (counts only).
    pub fn dcp_stats(&self) -> DcpStats {
        self.stats.lock().expect("stats lock").clone()
    }

    /// True while a turn holds the single-flight lease.
    pub fn turn_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
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
        match self.db.create_bound_session(id, &self.location)? {
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
    /// [`ToolCallEvent`] per recorded tool intent/outcome, in execution order.
    /// `tool_event` fires after the durable write, so a frontend can never
    /// show a card for a call that was not recorded.
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
        mut accepted: impl FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send,
        mut text_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_item_ended: impl FnMut(&str) + Send,
        mut tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        let started = std::time::Instant::now();
        let _lease = self.begin_active()?;
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        let mut mcp = self.mcp_generation.lock().await;
        let attached = self
            .ensure_mcp_generation(&mut mcp, &published, params.cancel)
            .await?;
        let mcp_warnings = attached.warnings();
        let result = self
            .run_turn_inner(
                params,
                &lane,
                attached,
                None,
                &mut accepted,
                &mut text_delta,
                &mut reasoning_delta,
                &mut reasoning_item_ended,
                &mut tool_event,
            )
            .await;
        self.retire_poisoned(&mut mcp).await?;
        drop(mcp);
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        result.and_then(|mut report| {
            report.duration_ms = duration_ms;
            report.warnings.extend(mcp_warnings);
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
        mut accepted: impl FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send,
        mut text_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_delta: impl FnMut(&str, &str) + Send,
        mut reasoning_item_ended: impl FnMut(&str) + Send,
        mut tool_event: impl FnMut(&str, &ToolCallEvent) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        let started = std::time::Instant::now();
        let _lease = self.begin_active()?;
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        let mut mcp = self.mcp_generation.lock().await;
        let attached = self
            .ensure_mcp_generation(&mut mcp, &published, params.cancel)
            .await?;
        let mcp_warnings = attached.warnings();
        let result = self
            .run_turn_inner(
                params,
                &lane,
                attached,
                Some(initial_selection),
                &mut accepted,
                &mut text_delta,
                &mut reasoning_delta,
                &mut reasoning_item_ended,
                &mut tool_event,
            )
            .await;
        // A committed fresh root must remain observable through the acceptance
        // callback even if cleanup or display-metadata writes later fail.
        let cleanup = self.retire_poisoned(&mut mcp).await;
        drop(mcp);
        cleanup?;
        let duration_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        result.and_then(|mut report| {
            report.duration_ms = duration_ms;
            report.warnings.extend(mcp_warnings);
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
        TurnLane {
            agent_id: workspace.agent_id.clone(),
            agent_color_index: workspace.agent_color_index,
            fixed_input: workspace.fixed_input.clone(),
            agent_digest: workspace.agent_digest.clone(),
            permissions,
            permission_rules,
        }
    }

    /// Execute a manual compress over validated ranges (same permission path).
    pub fn run_compress(
        &self,
        session: &str,
        args: &serde_json::Value,
        spec: &ProtectedSpec,
    ) -> Result<CompressReport, RuntimeError> {
        let _lease = self.begin_active()?;
        self.run_compress_inner(session, args, spec)
    }

    fn run_compress_inner(
        &self,
        session: &str,
        args: &serde_json::Value,
        spec: &ProtectedSpec,
    ) -> Result<CompressReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let lane = self.primary_lane(&published);
        {
            let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules);
            if policy.check(COMPRESS_TOOL).is_err() {
                return Err(denied(COMPRESS_TOOL));
            }
        }
        self.open_session(session)?;
        let (_topic, ranges) = crate::dcp::validate_range_args(args)
            .map_err(|e| RuntimeError::Compress(e.to_string()))?;
        // Manual compress is an explicit owner action over the visible
        // transcript: ranges may address rows the provider projection has
        // already dropped, so the addressed history is materialised here
        // (never on the per-turn path).
        let history = self.db.conversation_history_full(session)?;
        let messages = map_messages(&history)?;
        let op = format!(
            "compress-{session}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.db
            .record_tool_intent(&op, session, None, COMPRESS_TOOL, &args.to_string())?;
        let plan =
            match crate::dcp::prepare_compression(self.db, session, &messages, &ranges, spec, true)
            {
                Ok(plan) => plan,
                Err(error) => {
                    self.db
                        .record_tool_outcome(&op, "failed", Some(&error.to_string()))?;
                    return Err(RuntimeError::Compress(error.to_string()));
                }
            };
        let blocks = plan
            .blocks
            .iter()
            .map(|block| block.id.clone())
            .collect::<Vec<_>>();
        let output = serde_json::json!({
            "status": "compressed",
            "blocks": blocks,
            "savedTokens": plan.saved_tokens,
            "beforeBytes": plan.before_bytes,
            "afterBytes": plan.after_bytes,
        })
        .to_string();
        let mut next_states = self.nudge_state.lock().expect("nudge lock").clone();
        for (key, state) in &mut next_states {
            if key.starts_with(&format!("dcp.nudge.{session}\0")) {
                state.on_compress_success();
            }
        }
        let preference_updates = next_states
            .iter()
            .filter(|(key, _)| key.starts_with(&format!("dcp.nudge.{session}\0")))
            .map(|(key, state)| {
                serde_json::to_string(state)
                    .map(|value| (key.clone(), value))
                    .map_err(|_| RuntimeError::Storage)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let metadata = crate::dcp::CompressionCommitMetadata {
            operation_id: &op,
            operation_state: "completed",
            operation_output: &output,
            turn_id: None,
            turn_log: None,
            preference_updates: &preference_updates,
        };
        let report = crate::dcp::commit_compression(self.db, session, plan, Some(&metadata))
            .map_err(|error| RuntimeError::Compress(error.to_string()))?;
        *self.nudge_state.lock().expect("nudge lock") = next_states;
        self.stats.lock().expect("stats lock").compressions += 1;
        Ok(CompressReport {
            blocks: report.blocks.iter().map(|block| block.id.clone()).collect(),
            saved_tokens: report.saved_tokens,
            shrank: true,
        })
    }

    /// Bounded active projection: prune-bounded rows plus block summaries.
    ///
    /// Never materialises the covered archive; an active window above
    /// [`ACTIVE_CONTEXT_BYTES_CAP`] refuses the turn with an explicit
    /// diagnostic instead of silently dropping facts.
    fn active_projection(&self, session: &str) -> Result<ActiveContext, RuntimeError> {
        let after_seq = self
            .db
            .prune_bound(session)?
            .map(|(_, seq)| seq)
            .unwrap_or(0);
        let active = self
            .db
            .active_history(session, after_seq, ACTIVE_CONTEXT_BYTES_CAP)?;
        if active.overflow {
            return Err(RuntimeError::ContextOverflow {
                bytes: active.bytes,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        let blocks =
            crate::dcp::load_blocks(self.db, session).map_err(|_| RuntimeError::Storage)?;
        let positions = self.db.block_positions(session, after_seq)?;
        let projected = crate::dcp::project_active_rows(&active.rows, &blocks, &positions)
            .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
        Ok(ActiveContext {
            after_seq,
            projected,
            blocks,
        })
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

    /// Active rows only (prune-bounded, no block placement).
    #[allow(clippy::type_complexity)]
    fn active_rows(
        &self,
        session: &str,
    ) -> Result<(i64, Vec<(String, String, String)>), RuntimeError> {
        let after_seq = self
            .db
            .prune_bound(session)?
            .map(|(_, seq)| seq)
            .unwrap_or(0);
        let active = self
            .db
            .active_history(session, after_seq, ACTIVE_CONTEXT_BYTES_CAP)?;
        if active.overflow {
            return Err(RuntimeError::ContextOverflow {
                bytes: active.bytes,
                cap: ACTIVE_CONTEXT_BYTES_CAP,
            });
        }
        Ok((after_seq, active.rows))
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_turn_inner(
        &self,
        params: TurnParams<'_>,
        lane: &TurnLane,
        attached: &McpGeneration,
        fresh_selection: Option<Option<(&str, &str)>>,
        accepted: &mut (dyn FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send),
        text_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_item_ended: &mut (dyn FnMut(&str) + Send),
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) -> Result<TurnReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        let base = models::select_model(params.catalog, &params.model_id)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let fallback = published
            .config
            .providers
            .get(&params.catalog.provider)
            .map(|provider| provider.options.native_fallback_limits)
            .unwrap_or_default();
        let budget = models::budget(&base, params.max_output, fallback);
        let mut report = self
            .run_turn_admitted(
                params,
                lane,
                attached,
                fresh_selection,
                accepted,
                text_delta,
                reasoning_delta,
                reasoning_item_ended,
                tool_event,
                &budget,
            )
            .await?;
        if let Some(warning) = budget.warning {
            report.warnings.push(warning);
        }
        Ok(report)
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_turn_admitted(
        &self,
        params: TurnParams<'_>,
        lane: &TurnLane,
        attached: &McpGeneration,
        fresh_selection: Option<Option<(&str, &str)>>,
        accepted: &mut (dyn FnMut(&str, Option<&oc_core::queries::ModelSwitchNotice>) + Send),
        text_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_delta: &mut (dyn FnMut(&str, &str) + Send),
        reasoning_item_ended: &mut (dyn FnMut(&str) + Send),
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
        budget: &models::AdmissionBudget,
    ) -> Result<TurnReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        if fresh_selection.is_some() {
            if params.session.trim().is_empty() {
                return Err(RuntimeError::InvalidArgs("empty session id".into()));
            }
            match self.db.session_meta(&params.session) {
                Err(StorageError::SessionNotFound) => {}
                Ok(_) => {
                    return Err(RuntimeError::InvalidArgs(
                        "session id already exists".into(),
                    ));
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            self.open_session(&params.session)?;
        }
        // Exact model selection + admission before any side effect.
        let base = models::select_model(params.catalog, &params.model_id)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let selection = models::select_variant(&base, params.variant.as_deref())
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let workspace = self.workspace.read().expect("workspace lock").clone();
        // Outbound context honors compression blocks + prune mark: covered
        // members collapse to summaries, raw history is never rewritten.
        let (mut projected, mut history) = if fresh_selection.is_some() {
            (Vec::new(), Vec::new())
        } else {
            let ActiveContext {
                after_seq,
                projected,
                blocks,
            } = self.active_projection(&params.session)?;
            let history = self.wire_history(
                &params.session,
                &projected,
                &blocks,
                &selection.id,
                &params.catalog.provider,
                lane.agent_digest.as_deref(),
                after_seq,
            )?;
            (projected, history)
        };
        let dcp_config = self.dcp_config.read().expect("dcp lock").clone();
        let compress_available = dcp_config.enabled
            && !dcp_config.manual_mode
            && RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
                .check(COMPRESS_TOOL)
                .is_ok();
        let model_context = budget.context;
        let dcp_model_key = format!("{}/{}", params.catalog.provider, selection.id);
        let dcp_thresholds = dcp_config.effective_for_context(&dcp_model_key, model_context);
        if dcp_thresholds.min_context > dcp_thresholds.max_context {
            return Err(RuntimeError::InvalidArgs(
                "effective DCP minContextLimit exceeds maxContextLimit".into(),
            ));
        }
        let mut tool_projection = if fresh_selection.is_some() {
            Default::default()
        } else {
            self.db.load_dcp_tool_projection(&params.session)?
        };
        apply_dcp_projection(&mut history, &tool_projection);
        let mut anchors = compress_available
            .then(|| dcp_config_input(&projected, &dcp_config))
            .flatten();
        let state_key = format!(
            "dcp.nudge.{}\0{}\0{}",
            params.session, params.catalog.provider, selection.id
        );
        {
            let mut states = self.nudge_state.lock().expect("nudge lock");
            if !states.contains_key(&state_key) {
                let state = match fresh_selection {
                    Some(_) => NudgeState::default(),
                    None => match self.db.get_pref(&state_key)? {
                        Some(raw) => {
                            serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?
                        }
                        None => NudgeState::default(),
                    },
                };
                states.insert(state_key.clone(), state);
            }
        }
        let mut nudge_hint = None;
        // Early admission before durable intent; full request admission is
        // repeated below for every round, including tool schemas and results.
        let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
            .with_root(&self.roots.project)
            .with_mcp(&attached.entries);
        let mut fixed_input = lane.fixed_input.clone();
        fixed_input.extend(mcp_instruction_input(attached, &policy));
        let assembled_estimate = estimate_tokens(
            &serde_json::to_string(&(fixed_input.as_slice(), history.as_slice()))
                .map_err(|_| RuntimeError::Storage)?,
        ) + estimate_tokens(&params.prompt);
        models::admit_budget(&selection, assembled_estimate, budget).map_err(|error| {
            RuntimeError::InvalidArgs(match &budget.warning {
                Some(warning) => format!("{error}; {warning}"),
                None => error.to_string(),
            })
        })?;
        // Durable intent before any side effect.
        let turn_id = next_turn_id(&params.session, millis());
        let user_text = params.invocation.as_deref().unwrap_or(&params.prompt);
        let model_ref = oc_core::queries::ModelRef {
            provider: params.catalog.provider.clone(),
            id: selection.id.clone(),
            variant: selection
                .variant
                .as_ref()
                .map(|variant| variant.name.clone())
                .filter(|name| name != "default"),
        };
        let accepted_turn = if let Some(initial_selection) = fresh_selection {
            self.db.create_bound_session_and_accept_turn(
                &params.session,
                &self.location,
                &turn_id,
                &params.prompt,
                user_text,
                initial_selection,
                &model_ref,
            )?
        } else {
            self.db.accept_turn(
                &turn_id,
                &params.session,
                &params.prompt,
                user_text,
                &model_ref,
            )?
        };
        accepted(&turn_id, accepted_turn.model_switch.as_ref());
        let user_message = accepted_turn.user_message;
        let mut tool_defs = builtin_tool_defs();
        if !compress_available {
            tool_defs.retain(|tool| tool.name != COMPRESS_TOOL);
        }
        let subagents = workspace.subagents.clone();
        if let Some(catalog) = &subagents {
            tool_defs.push(subagent_tool_def(catalog, &policy));
        }
        for entry in &attached.entries {
            tool_defs.push(ToolDef {
                name: entry.namespaced.clone(),
                description: entry
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("mcp {} tool", entry.tool)),
                parameters: entry.input_schema.clone(),
            });
        }
        let snapshot = workspace.skills;
        let runner = subagents.as_ref().map(|catalog| TurnSubagent {
            runtime: self,
            parent_session: params.session.clone(),
            parent_model_id: params.model_id.clone(),
            parent_variant: params.variant.clone(),
            catalog: params.catalog,
            provider: &params.provider,
            cancel: params.cancel,
            attached,
            subagents: catalog.clone(),
            parent_lane: lane,
        });
        let ctx = ToolContext {
            files: &self.files,
            shell: &self.shell,
            parent_env: &self.parent_env,
            webfetch_auth: self.webfetch_auth.clone(),
            webfetch_allow_private: self.webfetch_allow_private,
            policy: &policy,
            subagent: runner.as_ref().map(|runner| runner as &dyn SubagentRunner),
            snapshot: &snapshot,
            cancel: params.cancel,
            roots: Some(self.roots.clone()),
        };
        let mut text = String::new();
        let mut usage = None;
        let mut context_usage = None;
        let mut usage_complete = true;
        let mut streamed = Duration::ZERO;
        let mut calls = Vec::new();
        let mut turn_log = TurnLog::new(&turn_id, &selection.id, &params.catalog.provider);
        turn_log.display = serde_json::json!({
            "model_label":selection.entry.get("name").and_then(|v|v.as_str()).unwrap_or(&selection.id),
            "agent":lane.agent_id,
            "agent_color_index":lane.agent_color_index,
        });
        turn_log.agent_digest = lane.agent_digest.clone();
        turn_log.user_message = Some(user_message);
        turn_log
            .input
            .push(InputItem::message(InputRole::User, &params.prompt));
        self.db
            .checkpoint_turn(&turn_id, &turn_log.to_json().to_string())?;
        let max_rounds = params.max_rounds.clamp(1, ROUND_CAP);
        let mut rounds = 0u32;
        loop {
            if params.cancel.load(Ordering::Relaxed) {
                return self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Cancelled,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
            let (nudge, persisted_nudge) = {
                let estimate = estimate_tokens(
                    &serde_json::to_string(&(
                        fixed_input.as_slice(),
                        history.as_slice(),
                        turn_log.input.as_slice(),
                    ))
                    .map_err(|_| RuntimeError::Storage)?,
                );
                let summary_tokens = active_summary_tokens(&projected);
                let mut states = self.nudge_state.lock().expect("nudge lock");
                let state = states.entry(state_key.clone()).or_default();
                let nudge = if compress_available {
                    state.on_turn();
                    evaluate(
                        &dcp_config,
                        state,
                        &dcp_model_key,
                        model_context,
                        estimate,
                        summary_tokens,
                    )
                } else {
                    None
                };
                let persisted = serde_json::to_string(state).map_err(|_| RuntimeError::Storage)?;
                (nudge, persisted)
            };
            self.db.set_pref(&state_key, &persisted_nudge)?;
            let nudge_input = nudge.as_ref().map(|nudge| {
                let force = match nudge.force {
                    crate::dcp_auto::NudgeForce::Soft => "advisory",
                    crate::dcp_auto::NudgeForce::Hard => "required before more work",
                };
                InputItem::message(
                    InputRole::Developer,
                    format!("DCP reminder ({force}): {}", nudge.text),
                )
            });
            if let Some(nudge) = &nudge {
                nudge_hint = Some(nudge.text.clone());
                self.stats.lock().expect("stats lock").nudges_emitted += 1;
            }
            let input: Vec<InputItem> = fixed_input
                .iter()
                .chain(nudge_input.iter())
                .chain(anchors.iter())
                .chain(&history)
                .chain(&turn_log.input)
                .cloned()
                .collect();
            let request_estimate = estimate_tokens(
                &serde_json::to_string(&(&input, &tool_defs)).map_err(|_| RuntimeError::Storage)?,
            );
            if let Err(error) = models::admit_budget(&selection, request_estimate, budget) {
                let mut report = self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Failed,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                )?;
                report.diagnostic = Some(error.to_string());
                return Ok(report);
            }
            let stream_started = std::time::Instant::now();
            let mut reasoning_started: Option<std::time::Instant> = None;
            let mut reasoning_closed = false;
            // Stream slots are private until canonical output supplies input
            // references. Their positions preserve text between reasoning items.
            struct TextSlot {
                part: usize,
                item_id: String,
                output_index: Option<u64>,
            }
            let mut text_slots: Vec<TextSlot> = Vec::new();
            let mut active_text: Option<usize> = None;
            let mut reasoning_anchors: Vec<(usize, String)> = Vec::new();
            let generation = match crate::provider::stream_input_observed(
                &params.provider,
                &selection.id,
                selection.variant.as_ref(),
                &input,
                &tool_defs,
                budget.output,
                params.cancel,
                &mut |item| match item {
                    crate::provider::StreamItem::TextDelta(delta) => {
                        if !delta.is_empty() && active_text.is_none() {
                            active_text = Some(text_slots.len());
                            text_slots.push(TextSlot {
                                part: turn_log.display_parts.len(),
                                item_id: String::new(),
                                output_index: None,
                            });
                            turn_log.display_parts.push(serde_json::json!({"pending_text":true}));
                        }
                        text_delta(&turn_id, delta);
                    }
                    crate::provider::StreamItem::MessageBoundary { item_id, output_index, done } => {
                        let slot = active_text.filter(|&index| {
                            let current = &text_slots[index];
                            (item_id.is_empty() || current.item_id.is_empty() || current.item_id == *item_id)
                                && (output_index.is_none() || current.output_index.is_none() || current.output_index == *output_index)
                        }).or_else(|| {
                            text_slots.iter().position(|slot| {
                                (!item_id.is_empty() && slot.item_id == *item_id)
                                    || (output_index.is_some() && slot.output_index == *output_index)
                            })
                        });
                        let slot = slot.unwrap_or_else(|| {
                            let index = text_slots.len();
                            text_slots.push(TextSlot {
                                part: turn_log.display_parts.len(),
                                item_id: String::new(),
                                output_index: None,
                            });
                            turn_log.display_parts.push(serde_json::json!({"pending_text":true}));
                            index
                        });
                        if !item_id.is_empty() { text_slots[slot].item_id.clone_from(item_id); }
                        if output_index.is_some() { text_slots[slot].output_index = *output_index; }
                        active_text = (!done).then_some(slot);
                    }
                    crate::provider::StreamItem::ReasoningDelta(delta) if !delta.is_empty() => {
                        active_text = None;
                        reasoning_started.get_or_insert_with(std::time::Instant::now);
                        if let Some(last) = turn_log.display_parts.last_mut()
                            && let Some(text) = last.get("reasoning").and_then(|v| v.as_str())
                            && !reasoning_closed
                        {
                            let mut text = text.to_string();
                            if text.len() + delta.len() > 16 * 1024 { last["truncated"] = true.into(); }
                            if text.len() < 16 * 1024 { text.push_str(delta); }
                            last["reasoning"] = truncate(&text, 16 * 1024).into();
                        } else {
                            turn_log.display_parts.push(serde_json::json!({"reasoning":truncate(delta, 16 * 1024), "truncated":delta.len()>16*1024}));
                        }
                        reasoning_closed = false;
                        reasoning_delta(&turn_id, delta);
                    }
                    crate::provider::StreamItem::OpaqueItem { item_id, payload }
                        if !item_id.is_empty() && payload.get("type").and_then(|v| v.as_str()) == Some("reasoning")
                            && reasoning_started.is_some() => {
                        let part = turn_log.display_parts.len().saturating_sub(1);
                        if let Some(last) = turn_log.display_parts.last_mut()
                            && last.get("reasoning").is_some()
                            && !reasoning_closed
                        {
                            last["duration_ms"] = streamed_ms(reasoning_started.take().expect("active reasoning").elapsed()).into();
                            reasoning_anchors.push((part, item_id.clone()));
                            reasoning_closed = true;
                            reasoning_item_ended(&turn_id);
                        }
                    }
                    _ => {}
                },
            )
            .await
            {
                Ok(generation) => {
                    streamed += stream_started.elapsed();
                    generation
                }
                Err(error) => {
                    turn_log.display_parts.retain(|part| part.get("pending_text").is_none());
                    streamed += stream_started.elapsed();
                    let status = if params.cancel.load(Ordering::Relaxed) {
                        TurnStatus::Cancelled
                    } else if matches!(
                        error,
                        crate::provider::ProviderError::Incomplete
                            | crate::provider::ProviderError::ResponseIncomplete
                    ) {
                        TurnStatus::Incomplete
                    } else {
                        TurnStatus::Failed
                    };
                    let mut report = self.commit_turn(
                        &turn_log,
                        turn_id,
                        &params.session,
                        status,
                        text,
                        rounds,
                        streamed_ms(streamed),
                        usage,
                        context_usage,
                        calls,
                        nudge_hint,
                        &published,
                    )?;
                    report.diagnostic = Some(error.to_string());
                    return Ok(report);
                }
            };
            rounds += 1;
            for item in &generation.items {
                turn_log.ingest(item);
            }
            // Completed structured messages are authoritative even when a
            // proxy omitted output_text.delta (or only streamed one of several
            // message items). Never derive public text from reasoning payloads.
            let canonical_text: String = generation
                .output
                .iter()
                .filter(|item| item["type"] == "message" && item["role"] == "assistant")
                .filter_map(|item| item["content"].as_array())
                .flat_map(|content| content.iter())
                .filter(|part| part["type"] == "output_text")
                .filter_map(|part| part["text"].as_str())
                .collect();
            text.push_str(if canonical_text.is_empty() {
                &generation.text
            } else {
                &canonical_text
            });
            // A generation's measured context remains useful for display when
            // another round omitted usage and the billed turn total is unknown.
            if let Some(reported) = generation.usage {
                context_usage = Some(reported);
            }
            // Usage: the last round's input tokens, output summed over rounds
            // (upstream aggregates per-step output for tok/s, runtime.rs docs).
            usage_complete &= generation.usage.is_some();
            if usage_complete && let Some((input, output)) = generation.usage {
                let total = usage.map(|(_, previous)| previous).unwrap_or(0) + output;
                usage = Some((input, total));
            } else {
                // A partially reported sum is not a known turn total.
                usage = None;
            }
            // Calls come only from complete canonical output, never partial deltas.
            let mut call_items = Vec::new();
            for item in &generation.output {
                if item["type"] == "function_call" {
                    call_items.push(crate::provider::StreamItem::ToolCallStarted {
                        item_id: item["id"].as_str().unwrap_or_default().to_string(),
                        call_id: item["call_id"].as_str().unwrap_or_default().to_string(),
                        name: item["name"].as_str().unwrap_or_default().to_string(),
                    });
                    call_items.push(crate::provider::StreamItem::ArgDelta {
                        item_id: item["id"].as_str().unwrap_or_default().to_string(),
                        delta: item["arguments"].as_str().unwrap_or_default().to_string(),
                    });
                }
            }
            let units = match assemble_calls(&call_items) {
                Ok(units) => units,
                Err(error) => {
                    turn_log
                        .display_parts
                        .retain(|part| part.get("pending_text").is_none());
                    calls.push(CallRecord {
                        name: "unknown".to_string(),
                        state: "failed".to_string(),
                        output: truncate(
                            &format!("error: batch assembly: {error}"),
                            REPORT_OUTPUT_CAP,
                        ),
                    });
                    return self.commit_turn(
                        &turn_log,
                        turn_id,
                        &params.session,
                        TurnStatus::Failed,
                        text,
                        rounds,
                        streamed_ms(streamed),
                        usage,
                        context_usage,
                        calls,
                        nudge_hint,
                        &published,
                    );
                }
            };
            let has_message = generation
                .output
                .iter()
                .any(|value| value["type"] == "message");
            let generation_output_positions = generation
                .output
                .iter()
                .enumerate()
                .filter_map(|(index, output)| {
                    output["id"].as_str().map(|id| (index, id.to_owned()))
                })
                .collect::<Vec<_>>();
            // Assembly is ordered by the canonical function_call items. Match
            // both provider identities before using an output position; absent
            // or ambiguous identities retain the append-at-intent fallback.
            let call_positions = units
                .iter()
                .map(|unit| {
                    let id = match unit {
                        Assembled::Call(call) => &call.id,
                        Assembled::Failed(failure) => &failure.id,
                    };
                    if id.is_empty() {
                        return None;
                    }
                    let mut matches = generation.output.iter().enumerate().filter(|(_, item)| {
                        item["type"] == "function_call"
                            && item["id"]
                                .as_str()
                                .is_some_and(|item_id| !item_id.is_empty())
                            && item["call_id"].as_str() == Some(id.as_str())
                    });
                    let (index, _) = matches.next()?;
                    matches.next().is_none().then_some(index)
                })
                .collect::<Vec<_>>();
            let mut messages = Vec::new();
            for (index, output) in generation.output.into_iter().enumerate() {
                if output["type"] == "message" && output["role"] == "assistant" {
                    messages.push((
                        index,
                        output["id"].as_str().unwrap_or("").to_owned(),
                        turn_log.input.len(),
                    ));
                }
                turn_log.input.push(InputItem::ProviderOutput(output));
            }
            // Text-only synthetic peers may omit canonical messages. This is plain
            // assistant text, never reconstruction of reasoning or function calls.
            if !generation.text.is_empty() && !has_message {
                messages.push((usize::MAX, String::new(), turn_log.input.len()));
                turn_log
                    .input
                    .push(InputItem::message(InputRole::Assistant, &generation.text));
            }
            let mut assigned = vec![false; messages.len()];
            let mut positioned = reasoning_anchors
                .iter()
                .filter_map(|(part, id)| {
                    generation_output_positions
                        .iter()
                        .find(|(_, output_id)| output_id == id)
                        .map(|(index, _)| (*index, *part))
                })
                .collect::<Vec<_>>();
            // Claim observed identities first: an earlier anonymous slot must
            // not steal the message belonging to a later identified slot.
            for slot in text_slots
                .iter()
                .filter(|slot| slot.output_index.is_some() || !slot.item_id.is_empty())
            {
                let mut matches = messages.iter().enumerate().filter(|(i, (index, id, _))| {
                    !assigned[*i]
                        && (slot
                            .output_index
                            .is_some_and(|position| position == *index as u64)
                            || (!slot.item_id.is_empty() && slot.item_id == *id))
                });
                if let Some((i, _)) = matches.next()
                    && matches.next().is_none()
                {
                    assigned[i] = true;
                    positioned.push((messages[i].0, slot.part));
                    turn_log.display_parts[slot.part] =
                        serde_json::json!({"message":messages[i].2});
                }
            }
            // Without an identity, a stream slot only identifies a message if
            // there is exactly one remaining. Otherwise remove the placeholder
            // and insert the canonical messages against reasoning anchors below.
            for slot in text_slots
                .iter()
                .filter(|slot| slot.output_index.is_none() && slot.item_id.is_empty())
            {
                let mut candidates = assigned.iter().enumerate().filter(|(_, used)| !**used);
                if let Some((i, _)) = candidates.next()
                    && candidates.next().is_none()
                {
                    assigned[i] = true;
                    positioned.push((messages[i].0, slot.part));
                    turn_log.display_parts[slot.part] =
                        serde_json::json!({"message":messages[i].2});
                }
            }
            let removed = text_slots
                .iter()
                .filter(|slot| {
                    turn_log.display_parts[slot.part]
                        .get("pending_text")
                        .is_some()
                })
                .map(|slot| slot.part)
                .collect::<Vec<_>>();
            for (_, part) in &mut positioned {
                *part -= removed.iter().filter(|removed| **removed < *part).count();
            }
            turn_log
                .display_parts
                .retain(|part| part.get("pending_text").is_none());
            for (i, (index, _, input_index)) in messages.into_iter().enumerate() {
                if !assigned[i] {
                    let position = positioned
                        .iter()
                        .filter(|(other, _)| *other > index)
                        .map(|(_, part)| *part)
                        .min()
                        .unwrap_or(turn_log.display_parts.len());
                    turn_log
                        .display_parts
                        .insert(position, serde_json::json!({"message":input_index}));
                    for (_, part) in &mut positioned {
                        if *part >= position {
                            *part += 1;
                        }
                    }
                    positioned.push((index, position));
                }
            }
            self.db
                .checkpoint_turn(&turn_id, &turn_log.to_json().to_string())?;
            let (round_calls, projection_changed) = self
                .execute_units(
                    &turn_id,
                    &params.session,
                    &units,
                    &ctx,
                    &policy,
                    attached,
                    params.cancel,
                    rounds,
                    &mut turn_log,
                    &mut positioned,
                    &call_positions,
                    &state_key,
                    &mut tool_projection,
                    tool_event,
                )
                .await?;
            calls.extend(round_calls);
            if calls.iter().any(|call| call.state == "unknown") {
                let mut report = self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Failed,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                )?;
                report.diagnostic =
                    Some("MCP outcome unknown; retry may duplicate side effects".into());
                return Ok(report);
            }
            if projection_changed {
                let refreshed = self.active_projection(&params.session)?;
                projected = refreshed.projected;
                history = self.wire_history(
                    &params.session,
                    &projected,
                    &refreshed.blocks,
                    &selection.id,
                    &params.catalog.provider,
                    lane.agent_digest.as_deref(),
                    refreshed.after_seq,
                )?;
                apply_dcp_projection(&mut history, &tool_projection);
                anchors = compress_available
                    .then(|| dcp_config_input(&projected, &dcp_config))
                    .flatten();
            }
            if !units_have_calls(&units) {
                break;
            }
            if rounds >= max_rounds {
                return self.commit_turn(
                    &turn_log,
                    turn_id,
                    &params.session,
                    TurnStatus::Incomplete,
                    text,
                    rounds,
                    streamed_ms(streamed),
                    usage,
                    context_usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
        }
        self.commit_turn(
            &turn_log,
            turn_id,
            &params.session,
            TurnStatus::Completed,
            text,
            rounds,
            streamed_ms(streamed),
            usage,
            context_usage,
            calls,
            nudge_hint,
            &published,
        )
    }

    /// Run one child turn through the inner path, without the single-flight
    /// lease (the parent turn already holds it) and sharing the parent cancel
    /// flag so one Cancel stops both.
    #[allow(clippy::too_many_arguments)]
    async fn run_child_turn(
        &self,
        agent: &SubagentAgent,
        parent_lane: &TurnLane,
        session: &str,
        prompt: String,
        model: &ResolvedModel,
        max_output: u64,
        catalog: &ModelCatalog,
        provider: &ResponsesConfig,
        attached: &McpGeneration,
        cancel: &AtomicBool,
    ) -> Result<TurnReport, RuntimeError> {
        let workspace = self.workspace.read().expect("workspace lock").clone();
        // Parent generation ∩ child agent rules: an agent rule can only make
        // the child lane stricter, never widen the caller's authority.
        let mut permissions = parent_lane.permissions.clone();
        let mut permission_rules = parent_lane.permission_rules.clone();
        let mut agent_rules = agent.permission_rules.clone();
        if let Some(home) = self.parent_env.get("HOME") {
            agent_rules.expand_home(home);
        }
        permission_rules.narrow(&permissions, &agent.permissions, &agent_rules);
        for (tool, level) in &agent.permissions {
            permissions
                .entry(tool.clone())
                .and_modify(|current| {
                    if permission_rank(*level) > permission_rank(*current) {
                        *current = *level;
                    }
                })
                .or_insert(*level);
        }
        let lane = TurnLane {
            agent_id: Some(agent.id.clone()),
            agent_color_index: workspace
                .subagents
                .as_ref()
                .and_then(|catalog| catalog.agents.keys().position(|id| id == &agent.id)),
            fixed_input: lane_fixed_input(
                Some(&agent.prompt),
                &workspace.instructions,
                workspace.skills_projection.as_deref(),
            ),
            agent_digest: agent.digest.clone(),
            permissions,
            permission_rules,
        };
        let params = TurnParams {
            session: session.to_string(),
            prompt,
            invocation: None,
            catalog,
            model_id: model.id.clone(),
            variant: model.variant.clone(),
            max_output,
            provider: provider.clone(),
            cancel,
            max_rounds: MAX_ROUNDS,
        };
        self.run_turn_inner(
            params,
            &lane,
            attached,
            None,
            &mut |_: &str, _| {},
            &mut |_: &str, _: &str| {},
            &mut |_: &str, _: &str| {},
            &mut |_: &str| {},
            &mut |_: &str, _: &ToolCallEvent| {},
        )
        .await
    }

    /// Ancestor depth of a session (root = 0, direct child = 1).
    fn session_depth(&self, session: &str) -> Result<u32, RuntimeError> {
        let mut depth = 0u32;
        let mut current = session.to_string();
        for _ in 0..=SUBAGENT_DEPTH_WALK_CAP {
            match self.db.session_meta(&current)?.parent_id {
                Some(parent) => {
                    depth = depth.saturating_add(1);
                    current = parent;
                }
                None => return Ok(depth),
            }
        }
        Err(RuntimeError::InvalidArgs(
            "session parent chain is too deep".to_string(),
        ))
    }

    /// Unique child session id for one spawn (monotonic within the runtime).
    fn new_child_id(&self, parent: &str) -> String {
        let seq = self.subagent_seq.fetch_add(1, Ordering::Relaxed);
        format!("{parent}-sub-{}-{seq}", millis())
    }

    /// Commit durable records under a freshness check, then report.
    ///
    /// The assistant message and turn log persist only under the
    /// still-current generation (DCP08: no half-applied projection).
    #[allow(clippy::too_many_arguments)]
    fn commit_turn(
        &self,
        turn_log: &TurnLog,
        turn_id: String,
        _session: &str,
        status: TurnStatus,
        text: String,
        rounds: u32,
        streamed_ms: u64,
        usage: Option<(u64, u64)>,
        context_usage: Option<(u64, u64)>,
        calls: Vec<CallRecord>,
        nudge_hint: Option<String>,
        published: &PublishedGeneration,
    ) -> Result<TurnReport, RuntimeError> {
        if self.generation_id() != published.id {
            self.db
                .finish_turn(&turn_id, TurnStatus::Interrupted.as_str(), None)?;
            return Err(RuntimeError::StaleGeneration {
                want: published.id,
                got: self.generation_id(),
            });
        }
        let assistant =
            (status == TurnStatus::Completed && !text.is_empty()).then_some(text.as_str());
        self.db.commit_turn(
            &turn_id,
            status.as_str(),
            Some(&turn_log.to_json().to_string()),
            assistant,
        )?;
        Ok(TurnReport {
            turn_id,
            status,
            diagnostic: None,
            text,
            rounds,
            usage,
            context_usage,
            streamed_ms,
            duration_ms: 0,
            calls,
            nudge_hint,
            warnings: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn wire_history(
        &self,
        session: &str,
        projected: &[(String, String, String)],
        blocks: &[crate::dcp::CompressionBlock],
        model: &str,
        provider: &str,
        agent_digest: Option<&str>,
        after_seq: i64,
    ) -> Result<Vec<InputItem>, RuntimeError> {
        let mut turns = BTreeMap::new();
        let mut changed_lane_prompts = BTreeMap::new();
        let mut represented = std::collections::BTreeSet::new();
        let covered_anchors = blocks
            .iter()
            .flat_map(|block| block.members.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        for (raw, prompt) in self
            .db
            .wire_logs_for_window(session, after_seq, WIRE_LOG_PAGE)?
        {
            let value: serde_json::Value =
                serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?;
            let log = TurnLog::from_json(&value).map_err(|_| RuntimeError::Storage)?;
            let Some(anchor) = log.user_message else {
                continue;
            };
            if !projected.iter().any(|(id, _, _)| id == &anchor)
                && !covered_anchors.contains(&anchor)
            {
                continue;
            }
            if log.provider != provider {
                return Err(RuntimeError::InvalidArgs(
                    "session wire history belongs to a different provider/model".to_string(),
                ));
            }
            if log.model != model || log.agent_digest.as_deref() != agent_digest {
                // Model or agent behavior changed: start a fresh causality lane
                // from public messages, but retain the originally expanded user
                // prompt for an uncompressed anchor. Never re-expand an invocation
                // using the current workspace's potentially changed commands.
                if let Some(prompt) = prompt.filter(|prompt| !prompt.is_empty()) {
                    changed_lane_prompts.insert(anchor, prompt);
                }
                continue;
            }
            if let Some(id) = value["assistant_message"].as_str() {
                represented.insert(id.to_string());
            }
            // Unknown operations cannot be replayed or assigned invented results.
            // Retain every committed pair; omit only unanswered function calls.
            let answered: std::collections::BTreeSet<String> = log
                .input
                .iter()
                .filter_map(|item| match item {
                    InputItem::FunctionCallOutput { call_id, .. } => Some(call_id.clone()),
                    _ => None,
                })
                .collect();
            let input = log
                .input
                .into_iter()
                .filter(|item| match item {
                    InputItem::ProviderOutput(value) if value["type"] == "function_call" => {
                        value["call_id"]
                            .as_str()
                            .is_some_and(|id| answered.contains(id))
                    }
                    _ => true,
                })
                .collect::<Vec<_>>();
            turns.insert(anchor, input);
        }
        let mut input = Vec::new();
        let block_members = blocks
            .iter()
            .map(|block| (block.id.as_str(), block.members.as_slice()))
            .collect::<BTreeMap<_, _>>();
        for (id, role, text) in projected {
            if let Some(items) = turns.remove(id) {
                input.extend(items);
            } else if !represented.contains(id) {
                let role = match role.as_str() {
                    "system" => InputRole::System,
                    "developer" => InputRole::Developer,
                    "user" => InputRole::User,
                    "assistant" => InputRole::Assistant,
                    _ => {
                        return Err(RuntimeError::InvalidArgs(
                            "history contains an unpaired tool message".to_string(),
                        ));
                    }
                };
                let text = if role == InputRole::User {
                    changed_lane_prompts
                        .get(id)
                        .map_or(text.as_str(), String::as_str)
                } else {
                    text
                };
                input.push(InputItem::message(role, text));
                if let Some(anchors) = block_members.get(id.as_str()) {
                    for anchor in *anchors {
                        if let Some(items) = turns.remove(anchor) {
                            let answered = items
                                .iter()
                                .filter_map(|item| match item {
                                    InputItem::FunctionCallOutput { call_id, .. } => {
                                        Some(call_id.clone())
                                    }
                                    _ => None,
                                })
                                .collect::<std::collections::BTreeSet<_>>();
                            input.extend(items.into_iter().filter(|item| {
                                match item {
                                    InputItem::Message { .. } => false,
                                    InputItem::ProviderOutput(value)
                                        if value["type"] == "function_call" =>
                                    {
                                        value["call_id"]
                                            .as_str()
                                            .is_some_and(|call_id| answered.contains(call_id))
                                    }
                                    _ => true,
                                }
                            }));
                        }
                    }
                }
            }
        }
        Ok(input)
    }

    /// Execute one assembled batch: builtins via the executor, MCP via
    /// attached clients, failures as visible outputs. Every unit gets
    /// intent + outcome rows under the same policy object.
    #[allow(clippy::too_many_arguments)]
    async fn execute_units(
        &self,
        turn_id: &str,
        session: &str,
        units: &[Assembled],
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        attached: &McpGeneration,
        cancel: &AtomicBool,
        round: u32,
        turn_log: &mut TurnLog,
        positioned: &mut Vec<(usize, usize)>,
        call_positions: &[Option<usize>],
        nudge_key: &str,
        tool_projection: &mut crate::storage::DcpToolProjection,
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) -> Result<(Vec<CallRecord>, bool), RuntimeError> {
        let mut records = Vec::new();
        let mut projection_changed = false;
        for (i, unit) in units.iter().enumerate() {
            let (id, name, input) = match unit {
                Assembled::Call(call) => (&call.id, call.name.as_str(), call.arguments.to_string()),
                Assembled::Failed(failure) => (&failure.id, "unknown", "{}".to_string()),
            };
            // Keep the original provider identifier in the durable operation id.
            let op = format!("{turn_id}-r{round}-c{i}-{id}");
            let guarded = self.guard_patch(unit.clone());
            let rejection = match &guarded {
                Assembled::Failed(failure) => Some(("failed", format!("error: {}", failure.error))),
                Assembled::Call(call)
                    if is_builtin(&call.name) && crate::tools::validate_call(call).is_err() =>
                {
                    Some((
                        "failed",
                        format!("error: invalid arguments for {}", call.name),
                    ))
                }
                Assembled::Call(call)
                    if call.name == COMPRESS_TOOL && {
                        let config = self.dcp_config.read().expect("dcp lock");
                        !config.enabled || config.manual_mode
                    } =>
                {
                    Some((
                        "failed",
                        "error: compress unavailable in disabled/manual DCP mode".to_string(),
                    ))
                }
                Assembled::Call(call) if policy.check_call(call).is_err() => {
                    let state = if is_builtin(&call.name) {
                        "failed"
                    } else {
                        "denied"
                    };
                    Some((
                        state,
                        format!("error: {}", policy.check_call(call).expect_err("rejected")),
                    ))
                }
                _ if cancel.load(Ordering::Relaxed) => {
                    Some(("cancelled", "error: cancelled".to_string()))
                }
                _ => None,
            };
            // Fail closed. No built-in or MCP dispatch can precede this commit.
            let position = call_positions[i]
                .and_then(|index| {
                    positioned
                        .iter()
                        .filter(|(other, _)| *other > index)
                        .map(|(_, part)| *part)
                        .min()
                })
                .unwrap_or(turn_log.display_parts.len());
            turn_log
                .display_parts
                .insert(position, serde_json::json!({"tool":op}));
            for (_, part) in positioned.iter_mut() {
                if *part >= position {
                    *part += 1;
                }
            }
            if let Some(index) = call_positions[i] {
                positioned.push((index, position));
            }
            self.db.record_turn_tool_intent(
                &op,
                session,
                turn_id,
                name,
                &input,
                &turn_log.to_json().to_string(),
            )?;
            tool_event(
                turn_id,
                &ToolCallEvent::Started {
                    op: op.clone(),
                    name: name.to_string(),
                    input: input.clone(),
                },
            );
            if rejection.is_none()
                && let Assembled::Call(call) = unit
                && call.name == COMPRESS_TOOL
            {
                let (_, ranges) = crate::dcp::validate_range_args(&call.arguments)
                    .map_err(|error| RuntimeError::Compress(error.to_string()))?;
                let context = self.active_projection(session)?;
                let after_seq = context.after_seq;
                let full = {
                    let (_, rows) = self.active_rows(session)?;
                    rows
                };
                let messages = map_messages(&full)?;
                let config = self.dcp_config.read().expect("dcp lock").clone();
                let mut spec = self
                    .dcp_protected
                    .read()
                    .expect("dcp protection lock")
                    .clone();
                apply_turn_protection(&full, &config, &mut spec);
                match crate::dcp::prepare_compression(
                    self.db, session, &messages, &ranges, &spec, false,
                ) {
                    Ok(plan) => {
                        let existing = crate::dcp::load_blocks(self.db, session)
                            .map_err(|_| RuntimeError::Storage)?;
                        let rows = full.clone();
                        let before_rows = context.projected.clone();
                        let mut before_wire = self.wire_history(
                            session,
                            &before_rows,
                            &existing,
                            &turn_log.model,
                            &turn_log.provider,
                            turn_log.agent_digest.as_deref(),
                            after_seq,
                        )?;
                        let strategy_delta =
                            plan_dcp_strategies(&before_wire, &config, tool_projection);
                        apply_dcp_projection(&mut before_wire, tool_projection);
                        let mut candidate_tool_projection = tool_projection.clone();
                        candidate_tool_projection
                            .hidden
                            .extend(strategy_delta.hidden.iter().cloned());
                        candidate_tool_projection
                            .purged
                            .extend(strategy_delta.purged.iter().cloned());
                        let mut candidate = existing;
                        for block in &mut candidate {
                            if plan.consumed_blocks.contains(&block.id) {
                                block.members.clear();
                            }
                        }
                        candidate.extend(plan.blocks.iter().cloned());
                        let member_ids = candidate
                            .iter()
                            .flat_map(|block| block.members.iter().cloned())
                            .collect::<Vec<_>>();
                        let seqs = self
                            .db
                            .message_seqs(session, &member_ids)?
                            .into_iter()
                            .collect::<std::collections::BTreeMap<_, _>>();
                        let after_positions = crate::dcp::member_positions(&candidate, &seqs);
                        let after_rows =
                            crate::dcp::project_active_rows(&rows, &candidate, &after_positions)
                                .map_err(|error| RuntimeError::InvalidArgs(error.to_string()))?;
                        let mut after_wire = self.wire_history(
                            session,
                            &after_rows,
                            &candidate,
                            &turn_log.model,
                            &turn_log.provider,
                            turn_log.agent_digest.as_deref(),
                            after_seq,
                        )?;
                        apply_dcp_projection(&mut after_wire, &candidate_tool_projection);
                        let before_bytes = serde_json::to_vec(&(
                            dcp_config_input(&before_rows, &config),
                            before_wire,
                        ))
                        .map_err(|_| RuntimeError::Storage)?
                        .len();
                        let after_bytes = serde_json::to_vec(&(
                            dcp_config_input(&after_rows, &config),
                            after_wire,
                        ))
                        .map_err(|_| RuntimeError::Storage)?
                        .len();
                        if after_bytes >= before_bytes {
                            let output = serde_json::json!({
                                "status": "no_gain",
                                "beforeBytes": before_bytes,
                                "afterBytes": after_bytes,
                            })
                            .to_string();
                            turn_log.input.push(InputItem::FunctionCallOutput {
                                call_id: id.clone(),
                                output: output.clone(),
                            });
                            record_tool_finish(
                                self.db, tool_event, &op, name, "no_gain", &output, turn_id,
                                turn_log,
                            )?;
                            records.push(CallRecord {
                                name: name.to_string(),
                                state: "no_gain".to_string(),
                                output: truncate(&output, REPORT_OUTPUT_CAP),
                            });
                            continue;
                        }
                        let saved_tokens = u64::try_from(before_bytes - after_bytes)
                            .unwrap_or(u64::MAX)
                            .div_ceil(4);
                        let output = serde_json::json!({
                            "status": "compressed",
                            "blocks": plan.blocks.iter().map(|block| block.id.clone()).collect::<Vec<_>>(),
                            "savedTokens": saved_tokens,
                            "beforeBytes": before_bytes,
                            "afterBytes": after_bytes,
                        })
                        .to_string();
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        let log = turn_log.to_json().to_string();
                        let next_nudge = {
                            let states = self.nudge_state.lock().expect("nudge lock");
                            let mut state = states.get(nudge_key).cloned().unwrap_or_default();
                            state.on_compress_success();
                            state
                        };
                        let preference_updates = vec![(
                            nudge_key.to_string(),
                            serde_json::to_string(&next_nudge)
                                .map_err(|_| RuntimeError::Storage)?,
                        )];
                        let metadata = crate::dcp::CompressionCommitMetadata {
                            operation_id: &op,
                            operation_state: "completed",
                            operation_output: &output,
                            turn_id: Some(turn_id),
                            turn_log: Some(&log),
                            preference_updates: &preference_updates,
                        };
                        let hidden = strategy_delta.hidden.iter().cloned().collect::<Vec<_>>();
                        let purged = strategy_delta.purged.iter().cloned().collect::<Vec<_>>();
                        let report = crate::dcp::commit_compression_with_projection(
                            self.db,
                            session,
                            plan,
                            Some(&metadata),
                            &hidden,
                            &purged,
                        )
                        .map_err(|error| RuntimeError::Compress(error.to_string()))?;
                        *tool_projection = candidate_tool_projection;
                        self.nudge_state
                            .lock()
                            .expect("nudge lock")
                            .insert(nudge_key.to_string(), next_nudge);
                        self.stats.lock().expect("stats lock").compressions += 1;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "completed".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        debug_assert!(!report.blocks.is_empty());
                        projection_changed = true;
                        emit_tool_finish(tool_event, turn_id, &op, name, "completed", &output);
                        continue;
                    }
                    Err(crate::dcp::DcpError::NoGain {
                        before_bytes,
                        after_bytes,
                    }) => {
                        let output = serde_json::json!({
                            "status": "no_gain",
                            "beforeBytes": before_bytes,
                            "afterBytes": after_bytes,
                        })
                        .to_string();
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        record_tool_finish(
                            self.db, tool_event, &op, name, "no_gain", &output, turn_id, turn_log,
                        )?;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "no_gain".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        continue;
                    }
                    Err(error) => {
                        let output = format!("error: {error}");
                        turn_log.input.push(InputItem::FunctionCallOutput {
                            call_id: id.clone(),
                            output: output.clone(),
                        });
                        record_tool_finish(
                            self.db, tool_event, &op, name, "failed", &output, turn_id, turn_log,
                        )?;
                        records.push(CallRecord {
                            name: name.to_string(),
                            state: "failed".to_string(),
                            output: truncate(&output, REPORT_OUTPUT_CAP),
                        });
                        continue;
                    }
                }
            }
            let (state, output) = if let Some(rejection) = rejection {
                rejection
            } else {
                match unit {
                    Assembled::Call(call) if call.name == "bash" => {
                        crate::tools::execute_bash_typed(ctx, call).await
                    }
                    Assembled::Call(call) if is_builtin(&call.name) => {
                        let output = execute_batch(ctx, vec![guarded]).await.remove(0).output;
                        (output_state(&output), output)
                    }
                    Assembled::Call(call) => {
                        self.execute_mcp(call, &op, turn_id, attached, cancel).await
                    }
                    Assembled::Failed(_) => unreachable!("assembly failure rejected above"),
                }
            };
            // Failure here leaves started/unknown; never continue the batch.
            turn_log.input.push(InputItem::FunctionCallOutput {
                call_id: id.clone(),
                output: output.clone(),
            });
            record_tool_finish(
                self.db, tool_event, &op, name, state, &output, turn_id, turn_log,
            )?;
            records.push(CallRecord {
                name: name.to_string(),
                state: state.to_string(),
                output: truncate(&output, REPORT_OUTPUT_CAP),
            });
            if state == "unknown" {
                break; // no later side effects in this provider batch
            }
        }
        Ok((records, projection_changed))
    }

    /// Legacy patch deny wins: a protected patch never reaches the executor.
    fn guard_patch(&self, unit: Assembled) -> Assembled {
        let Assembled::Call(call) = &unit else {
            return unit;
        };
        if call.name != "apply_patch" {
            return unit;
        }
        let patch = call
            .arguments
            .get("patchText")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let violations = crate::dcp::check_patch_protected(patch, &self.protected.patterns);
        match violations {
            Ok(_) => unit,
            Err(error) => Assembled::Failed(CallFailure {
                id: call.id.clone(),
                error: format!("patch refused: {error}"),
            }),
        }
    }

    /// Dispatch only after the common permission and durable intent path.
    async fn execute_mcp(
        &self,
        call: &crate::tools::ToolCall,
        op: &str,
        turn: &str,
        attached: &McpGeneration,
        cancel: &AtomicBool,
    ) -> (&'static str, String) {
        let Some(entry) = attached
            .entries
            .iter()
            .find(|entry| entry.namespaced == call.name)
        else {
            return ("failed", format!("error: unknown mcp tool {}", call.name));
        };
        let server = attached
            .servers
            .iter()
            .find(|server| server.server_id == entry.server);
        let Some(server) = server else {
            return (
                "failed",
                format!("error: unknown mcp server {}", entry.server),
            );
        };
        let mut lease = McpCallLease {
            generation: attached,
            db: self.db,
            op,
            turn,
            remote: matches!(&server.client, AttachedServer::Remote(_)),
            armed: true,
        };
        let result = match &server.client {
            AttachedServer::Remote(client) => client
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| {
                        if matches!(
                            error,
                            McpError::Cancelled
                                | McpError::Deadline
                                | McpError::Transport
                                | McpError::CleanupFailed
                                | McpError::UnsupportedResult
                                | McpError::BadResult
                        ) || cancel.load(Ordering::Relaxed)
                        {
                            attached.poisoned.store(true, Ordering::SeqCst);
                            attached.remote_unknown.store(true, Ordering::SeqCst);
                            if error == McpError::CleanupFailed {
                                attached.cleanup_error.store(true, Ordering::SeqCst);
                            }
                            return (
                                "unknown",
                                "error: mcp outcome unknown; remote retry unsafe".into(),
                            );
                        }
                        remote_mcp_failure(error, cancel)
                    },
                    |text| ("completed", text),
                ),
            AttachedServer::Stdio(child) => child
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| {
                        if matches!(
                            error,
                            StdioError::Cancelled
                                | StdioError::Deadline
                                | StdioError::Transport
                                | StdioError::CleanupFailed
                                | StdioError::UnsupportedModality
                                | StdioError::BadResult
                        ) || cancel.load(Ordering::Relaxed)
                        {
                            attached.poisoned.store(true, Ordering::SeqCst);
                            if error == StdioError::CleanupFailed {
                                attached.cleanup_error.store(true, Ordering::SeqCst);
                            }
                            return (
                                "unknown",
                                "error: mcp outcome unknown; local owner stopping".into(),
                            );
                        }
                        stdio_mcp_failure(error, cancel)
                    },
                    |text| ("completed", text),
                ),
        };
        lease.armed = false;
        result
    }

    async fn retire_poisoned(&self, slot: &mut Option<McpGeneration>) -> Result<(), RuntimeError> {
        if !slot
            .as_ref()
            .is_some_and(|g| g.poisoned.load(Ordering::SeqCst))
        {
            return Ok(());
        }
        if slot
            .as_ref()
            .is_some_and(|g| g.remote_unknown.load(Ordering::SeqCst))
        {
            self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
        }
        let cleanup_error = slot
            .as_ref()
            .is_some_and(|g| g.cleanup_error.load(Ordering::SeqCst));
        if cleanup_error {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
        }
        if let Some(generation) = slot.take()
            && let Err(error) = close_generation(generation).await
        {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
            return Err(error);
        }
        if cleanup_error {
            Err(RuntimeError::McpShutdown)
        } else {
            Ok(())
        }
    }

    /// Reuse one connected MCP registry for the whole Location/config generation.
    async fn ensure_mcp_generation<'m>(
        &self,
        slot: &'m mut Option<McpGeneration>,
        published: &PublishedGeneration,
        cancel: &AtomicBool,
    ) -> Result<&'m McpGeneration, RuntimeError> {
        self.retire_poisoned(slot).await?;
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpShutdown);
        }
        if self.mcp_unsafe_retry.load(Ordering::SeqCst)
            && published
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
        let reusable = slot
            .as_ref()
            .is_some_and(|generation| generation.publication == published.id);
        if !reusable {
            if let Some(old) = slot.take()
                && let Err(error) = close_generation(old).await
            {
                self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                return Err(error);
            }
            *slot = Some(match self.attach_mcp(published, cancel).await {
                Ok(generation) => generation,
                Err(error) => {
                    if error == RuntimeError::McpShutdown {
                        self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                    }
                    return Err(error);
                }
            });
        } else if let Some(generation) = slot.as_mut() {
            generation.refresh_if_changed(cancel).await?;
        }
        Ok(slot.as_ref().expect("MCP generation published"))
    }

    /// Attach and validate every enabled server before publishing the generation.
    async fn attach_mcp(
        &self,
        published: &PublishedGeneration,
        cancel: &AtomicBool,
    ) -> Result<McpGeneration, RuntimeError> {
        let enabled = published
            .config
            .mcp
            .values()
            .filter(|entry| entry.enabled)
            .count();
        if enabled > MAX_MCP_SERVERS {
            return Err(RuntimeError::InvalidArgs(format!(
                "too many enabled MCP servers: {enabled} exceeds {MAX_MCP_SERVERS}"
            )));
        }
        let mut attached = Vec::new();
        let mut registries = Vec::new();
        let mut degraded = Vec::new();
        let redactions = mcp_redactions(&published.config, &self.parent_env);
        let mut ids: Vec<&String> = published.config.mcp.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &published.config.mcp[id];
            if !entry.enabled {
                continue;
            }
            let result = if entry.kind == "remote" {
                // codex_web has an explicit exact-version/static-bearer contract;
                // other configured remote servers use ordinary SDK negotiation.
                let codex_web = id == "codex_web";
                let config = if codex_web {
                    mcp_remote::CodexWebConfig::from_entry(entry)
                } else {
                    mcp_remote::CodexWebConfig::from_remote_entry(entry)
                }
                .map(|mut config| {
                    config.allow_private = self
                        .parent_env
                        .get("OC_TEST_ALLOW_LOOPBACK")
                        .is_some_and(|value| value == "1");
                    config
                })
                .map_err(|error| remote_attach_error_at(id, "config", error));
                match config {
                    Ok(config) => {
                        let client = CodexWebClient::connect_redacted(
                            &config,
                            codex_web,
                            cancel,
                            &redactions,
                        )
                        .await
                        .map_err(|error| remote_attach_error_at(id, "initialize", error));
                        match client {
                            Ok(client) => match client.list_tools(cancel).await {
                                Ok(tools) => {
                                    let registry = mcp_remote::map_registry(
                                        id,
                                        tools
                                            .into_iter()
                                            .map(|tool| {
                                                (tool.name, tool.description, tool.input_schema)
                                            })
                                            .collect(),
                                    )
                                    .map_err(|error| remote_attach_error(id, error));
                                    match registry {
                                        Ok(registry) => Ok((
                                            AttachedMcp {
                                                server_id: id.clone(),
                                                client: AttachedServer::Remote(client),
                                                registry: registry.clone(),
                                            },
                                            registry,
                                        )),
                                        Err(error) => {
                                            let cleanup = close_attached(AttachedMcp {
                                                server_id: id.clone(),
                                                client: AttachedServer::Remote(client),
                                                registry: Vec::new(),
                                            })
                                            .await;
                                            cleanup.map_err(|_| RuntimeError::McpShutdown)?;
                                            Err(error)
                                        }
                                    }
                                }
                                Err(error) => {
                                    let mapped = remote_attach_error(id, error);
                                    let cleanup = close_attached(AttachedMcp {
                                        server_id: id.clone(),
                                        client: AttachedServer::Remote(client),
                                        registry: Vec::new(),
                                    })
                                    .await;
                                    cleanup.map_err(|_| RuntimeError::McpShutdown)?;
                                    Err(mapped)
                                }
                            },
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            } else if entry.kind == "local" {
                let config =
                    StdioConfig::from_entry(id, entry, &self.roots.project, &self.parent_env)
                        .map_err(|error| stdio_attach_error_at(id, "config", error));
                match config {
                    Ok(config) => {
                        let client = StdioClient::launch_redacted(&config, cancel, &redactions)
                            .await
                            .map_err(|error| stdio_attach_error_at(id, "initialize", error));
                        match client {
                            Ok(client) => match client.list_tools(cancel).await {
                                Ok(tools) => {
                                    let registry = mcp_remote::map_registry(
                                        id,
                                        tools
                                            .into_iter()
                                            .map(|tool| {
                                                (tool.name, tool.description, tool.input_schema)
                                            })
                                            .collect(),
                                    )
                                    .map_err(|error| remote_attach_error(id, error));
                                    match registry {
                                        Ok(registry) => Ok((
                                            AttachedMcp {
                                                server_id: id.clone(),
                                                client: AttachedServer::Stdio(client),
                                                registry: registry.clone(),
                                            },
                                            registry,
                                        )),
                                        Err(error) => {
                                            let cleanup = close_attached(AttachedMcp {
                                                server_id: id.clone(),
                                                client: AttachedServer::Stdio(client),
                                                registry: Vec::new(),
                                            })
                                            .await;
                                            cleanup.map_err(|_| RuntimeError::McpShutdown)?;
                                            Err(error)
                                        }
                                    }
                                }
                                Err(error) => {
                                    let mapped = stdio_attach_error(id, error);
                                    let cleanup = close_attached(AttachedMcp {
                                        server_id: id.clone(),
                                        client: AttachedServer::Stdio(client),
                                        registry: Vec::new(),
                                    })
                                    .await;
                                    cleanup.map_err(|_| RuntimeError::McpShutdown)?;
                                    Err(mapped)
                                }
                            },
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                }
            } else {
                Err(RuntimeError::McpAttach {
                    server: safe_server_id(id),
                    stage: "config",
                    safe_code: "unsupported_transport",
                    retryable: false,
                })
            };
            match result {
                Ok((server, registry)) => {
                    attached.push(server);
                    registries.push(registry);
                }
                // Per-server attach failure degrades that server only: upstream
                // opencode v2.0.12 marks the server failed and keeps the turn.
                Err(error @ RuntimeError::McpAttach { .. }) => {
                    if matches!(
                        &error,
                        RuntimeError::McpAttach {
                            stage: "cleanup",
                            ..
                        }
                    ) {
                        self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                        let cleanup = close_generation(McpGeneration {
                            publication: published.id,
                            servers: attached,
                            entries: Vec::new(),
                            degraded: Vec::new(),
                            poisoned: AtomicBool::new(false),
                            remote_unknown: AtomicBool::new(false),
                            cleanup_error: AtomicBool::new(false),
                        })
                        .await;
                        cleanup?;
                        return Err(RuntimeError::McpShutdown);
                    }
                    record_degradation(&mut degraded, error);
                }
                Err(error) => {
                    let cleanup = close_generation(McpGeneration {
                        publication: published.id,
                        servers: attached,
                        entries: Vec::new(),
                        degraded: Vec::new(),
                        poisoned: AtomicBool::new(false),
                        remote_unknown: AtomicBool::new(false),
                        cleanup_error: AtomicBool::new(false),
                    })
                    .await;
                    cleanup?;
                    return Err(error);
                }
            }
        }
        match mcp_remote::merge_registries(registries) {
            Ok(entries) => Ok(McpGeneration {
                publication: published.id,
                servers: attached,
                entries,
                degraded,
                poisoned: AtomicBool::new(false),
                remote_unknown: AtomicBool::new(false),
                cleanup_error: AtomicBool::new(false),
            }),
            Err(error) => {
                let cleanup = close_generation(McpGeneration {
                    publication: published.id,
                    servers: attached,
                    entries: Vec::new(),
                    degraded: Vec::new(),
                    poisoned: AtomicBool::new(false),
                    remote_unknown: AtomicBool::new(false),
                    cleanup_error: AtomicBool::new(false),
                })
                .await;
                cleanup?;
                Err(remote_attach_error("generation", error))
            }
        }
    }
}

fn session_location_key(id: &str) -> String {
    format!("{SESSION_LOCATION_PREFIX}{id}")
}

/// Bound for walking a session's parent chain (cycle guard).
const SUBAGENT_DEPTH_WALK_CAP: u32 = 64;

/// Permission strictness rank (`Deny > Ask > Allow`).
fn permission_rank(level: Permission) -> u8 {
    match level {
        Permission::Allow => 0,
        Permission::Ask => 1,
        Permission::Deny => 2,
    }
}

/// Developer messages a lane starts from: agent prompt, instructions, skills.
fn lane_fixed_input(
    agent_prompt: Option<&str>,
    instructions: &str,
    skills_projection: Option<&str>,
) -> Vec<InputItem> {
    let mut fixed_input = Vec::new();
    if let Some(prompt) = agent_prompt.filter(|prompt| !prompt.trim().is_empty()) {
        fixed_input.push(InputItem::message(InputRole::Developer, prompt));
    }
    if !instructions.trim().is_empty() {
        fixed_input.push(InputItem::message(InputRole::Developer, instructions));
    }
    if let Some(projection) = skills_projection {
        fixed_input.push(InputItem::message(
            InputRole::Developer,
            format!(
                "Available native skills (metadata only; call skill by id for body): {projection}"
            ),
        ));
    }
    fixed_input
}

/// `subagent` tool definition with the upstream `Available subagents` list.
fn subagent_tool_def(catalog: &SubagentCatalog, policy: &RuntimePolicy<'_>) -> ToolDef {
    let mut description = String::from(
        "Spawns an agent in a child session to work on the specified task.\n\
         The output includes a sessionID you can pass back later to continue that specific conversation with the subagent.\n\
         New child sessions start with fresh context, so include all relevant context and instructions when you don't pass a sessionID.\n\
         Foreground (default) runs the subagent to completion and returns its final response.",
    );
    {
        let available = catalog
            .agents
            .values()
            .filter(|agent| {
                !agent.primary
                    && !agent.hidden
                    && policy.effect(SUBAGENT_TOOL, &agent.id) != Permission::Deny
            })
            .collect::<Vec<_>>();
        if !available.is_empty() {
            description.push_str("\n\nAvailable subagents:");
            for agent in available {
                let fallback = "This subagent should only be called when explicitly requested.";
                let summary = if agent.description.trim().is_empty() {
                    fallback
                } else {
                    agent.description.trim()
                };
                description.push_str(&format!("\n- {}: {summary}", agent.id));
            }
        }
    }
    ToolDef {
        name: SUBAGENT_TOOL.to_string(),
        description,
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "agent": {
                    "type": "string",
                    "description": "The type of specialized agent to use for this task.",
                },
                "description": {
                    "type": "string",
                    "description": "A short 3-5 word label for the task, displayed to the user",
                },
                "prompt": {
                    "type": "string",
                    "description": "The task for the subagent to perform",
                },
                "model": {
                    "type": "string",
                    "description": "NEVER set this unless the user explicitly asks for a particular model or variant. The value is written as \"providerID/modelID\", or \"providerID/modelID#variant\" to include a variant. Do not guess the ID.",
                },
                "sessionID": {
                    "type": "string",
                    "description": "Continue a specific previous subagent conversation by passing its sessionID. Calls without a sessionID start a new conversation.",
                },
                "background": {
                    "type": "boolean",
                    "description": "Not supported yet: calls with true fail without creating a child session.",
                },
            },
            "required": ["agent", "description", "prompt"],
            "additionalProperties": false,
        }),
    }
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

/// Parse and validate `provider/model[#variant]` against the catalog.
pub(crate) fn resolve_subagent_model(
    catalog: &ModelCatalog,
    raw: &str,
) -> Result<ResolvedModel, String> {
    let invalid = || {
        format!(
            "Invalid model \"{raw}\". Use \"providerID/modelID\" or \"providerID/modelID#variant\"."
        )
    };
    let (provider, rest) = raw.split_once('/').ok_or_else(invalid)?;
    let (id, variant) = match rest.split_once('#') {
        Some((id, variant)) => (id, Some(variant)),
        None => (rest, None),
    };
    if provider.is_empty() || id.is_empty() {
        return Err(invalid());
    }
    if provider != catalog.provider || !catalog.models.contains_key(id) {
        return Err(format!(
            "Model \"{provider}/{id}\" is not available. Use the models tool to see what is available."
        ));
    }
    match variant {
        None => Ok(ResolvedModel {
            id: id.to_string(),
            variant: None,
        }),
        Some(variant) => {
            let base = models::select_model(catalog, id).map_err(|_| invalid())?;
            match models::select_variant(&base, Some(variant)) {
                Ok(selection) => Ok(ResolvedModel {
                    id: id.to_string(),
                    variant: selection.variant.map(|variant| variant.name),
                }),
                Err(models::SelectError::UnavailableVariant { enabled, .. })
                    if enabled.is_empty() =>
                {
                    Err(format!(
                        "Model \"{provider}/{id}\" has no variants. Omit the variant."
                    ))
                }
                Err(models::SelectError::UnavailableVariant { enabled, .. }) => Err(format!(
                    "Variant \"{variant}\" is not available for \"{provider}/{id}\". Available: {enabled}."
                )),
                Err(_) => Err(invalid()),
            }
        }
    }
}

/// Resolve the child model per upstream order: explicit override, else the
/// child agent's model, else the existing child's stored model, else the
/// parent session model.
#[allow(clippy::too_many_arguments)]
fn resolve_child_model(
    catalog: &ModelCatalog,
    request_model: Option<&str>,
    agent_model: Option<&str>,
    existing: Option<&SessionMeta>,
    switched: bool,
    parent_model_id: &str,
    parent_variant: Option<&str>,
) -> Result<ResolvedModel, String> {
    let parent = || ResolvedModel {
        id: parent_model_id.to_string(),
        variant: parent_variant.map(str::to_string),
    };
    let resolve = |raw: &str| resolve_subagent_model(catalog, raw);
    if let Some(raw) = request_model {
        return resolve(raw);
    }
    match existing {
        None => match agent_model {
            Some(raw) => resolve(raw),
            None => Ok(parent()),
        },
        Some(meta) if switched => match agent_model {
            Some(raw) => resolve(raw),
            None => match meta.model.as_deref() {
                Some(raw) => resolve(raw),
                None => Ok(parent()),
            },
        },
        Some(meta) => match meta.model.as_deref() {
            Some(raw) => resolve(raw),
            None => match agent_model {
                Some(raw) => resolve(raw),
                None => Ok(parent()),
            },
        },
    }
}

/// Foreground child runner for one calling turn.
struct TurnSubagent<'r, 'a> {
    runtime: &'r Runtime<'a>,
    parent_session: String,
    parent_model_id: String,
    parent_variant: Option<String>,
    catalog: &'r ModelCatalog,
    provider: &'r ResponsesConfig,
    cancel: &'r AtomicBool,
    attached: &'r McpGeneration,
    subagents: SubagentCatalog,
    parent_lane: &'r TurnLane,
}

impl SubagentRunner for TurnSubagent<'_, '_> {
    fn spawn<'x>(
        &'x self,
        request: SubagentRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<SubagentOutcome, ToolError>> + Send + 'x>,
    > {
        Box::pin(self.spawn_inner(request))
    }
}

impl TurnSubagent<'_, '_> {
    async fn spawn_inner(&self, request: SubagentRequest) -> Result<SubagentOutcome, ToolError> {
        let failed = |reason: String| {
            Ok(SubagentOutcome::Failed {
                session_id: None,
                reason,
            })
        };
        let limit = self.subagents.depth_limit;
        let depth = self
            .runtime
            .session_depth(&self.parent_session)
            .map_err(|error| ToolError::Failed {
                tool: SUBAGENT_TOOL.to_string(),
                reason: error.to_string(),
            })?;
        if depth >= limit {
            return failed(format!(
                "Subagent depth limit reached ({limit}). Increase \"experimental.subagent_depth\" to allow nested subagents."
            ));
        }
        let Some(agent) = self.subagents.agents.get(&request.agent) else {
            return failed(format!("Unknown agent: {}", request.agent));
        };
        if agent.primary {
            return failed(format!("Agent {} cannot run as a subagent", request.agent));
        }
        let existing = match &request.session_id {
            None => None,
            Some(id) => match self.runtime.db.session_meta(id) {
                Ok(meta) if meta.parent_id.as_deref() == Some(self.parent_session.as_str()) => {
                    Some(meta)
                }
                Ok(_) => {
                    return failed(format!(
                        "Session {id} is not a child of the current session"
                    ));
                }
                Err(StorageError::SessionNotFound) => {
                    return failed(format!("Subagent session not found: {id}"));
                }
                Err(error) => {
                    return Err(ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    });
                }
            },
        };
        let switched = existing
            .as_ref()
            .is_some_and(|meta| meta.agent.as_deref() != Some(agent.id.as_str()));
        let model = match resolve_child_model(
            self.catalog,
            request.model.as_deref(),
            agent.model.as_deref(),
            existing.as_ref(),
            switched,
            &self.parent_model_id,
            self.parent_variant.as_deref(),
        ) {
            Ok(model) => model,
            Err(reason) => return failed(reason),
        };
        let (child_session, fresh) = match &request.session_id {
            Some(id) => (id.clone(), false),
            None => {
                let id = self.runtime.new_child_id(&self.parent_session);
                self.runtime
                    .db
                    .create_child_session(
                        &self.parent_session,
                        &id,
                        Some(&agent.id),
                        Some(&model.stored(&self.catalog.provider)),
                        Some(&request.description),
                    )
                    .map_err(|error| ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    })?;
                self.runtime
                    .db
                    .set_pref(&session_location_key(&id), self.runtime.location())
                    .map_err(|error| ToolError::Failed {
                        tool: SUBAGENT_TOOL.to_string(),
                        reason: error.to_string(),
                    })?;
                (id, true)
            }
        };
        let prompt = if fresh {
            format!(
                "You are a subagent spawned by another session.\n{}",
                request.prompt
            )
        } else {
            request.prompt.clone()
        };
        let report = self
            .runtime
            .run_child_turn(
                agent,
                self.parent_lane,
                &child_session,
                prompt,
                &model,
                0, // Resolve the same native output default as the primary turn.
                self.catalog,
                self.provider,
                self.attached,
                self.cancel,
            )
            .await
            .map_err(|error| ToolError::Failed {
                tool: SUBAGENT_TOOL.to_string(),
                reason: error.to_string(),
            })?;
        // Child warnings must survive the existing text-only tool result DTO.
        let with_warnings = |text: String| {
            if report.warnings.is_empty() {
                text
            } else {
                format!("Warning: {}\n\n{text}", report.warnings.join("\nWarning: "))
            }
        };
        Ok(match report.status {
            TurnStatus::Completed => SubagentOutcome::Completed {
                session_id: child_session,
                text: with_warnings(if report.text.is_empty() {
                    SUBAGENT_NO_TEXT.to_string()
                } else {
                    report.text
                }),
            },
            TurnStatus::Cancelled => SubagentOutcome::Cancelled {
                session_id: child_session,
            },
            _ => SubagentOutcome::Failed {
                session_id: Some(child_session),
                reason: with_warnings(
                    report
                        .diagnostic
                        .unwrap_or_else(|| "subagent turn did not complete".to_string()),
                ),
            },
        })
    }
}

fn is_builtin(name: &str) -> bool {
    crate::tools::MODEL_TOOL_NAMES.contains(&name) || name == SUBAGENT_TOOL
}

fn units_have_calls(units: &[Assembled]) -> bool {
    units.iter().any(|unit| matches!(unit, Assembled::Call(_)))
}

fn output_state(output: &str) -> &'static str {
    if output == "error: cancelled" {
        "cancelled"
    } else if output.starts_with("error: ") {
        "failed"
    } else {
        "completed"
    }
}

/// Forward one terminal tool-call state to the live event sink, bounded
/// exactly like the turn report.
fn emit_tool_finish(
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    turn_id: &str,
    op: &str,
    name: &str,
    state: &str,
    output: &str,
) {
    tool_event(
        turn_id,
        &ToolCallEvent::Finished {
            op: op.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            output: truncate(output, REPORT_OUTPUT_CAP),
            output_bytes: output.len() as i64,
            output_truncated: output.len() > REPORT_OUTPUT_CAP,
        },
    );
}

/// Persist one tool outcome, then notify the live event sink. The event can
/// never describe an outcome that was not durably recorded.
#[allow(clippy::too_many_arguments)]
fn record_tool_finish(
    db: &Db,
    tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    op: &str,
    name: &str,
    state: &str,
    output: &str,
    turn_id: &str,
    turn_log: &TurnLog,
) -> Result<(), RuntimeError> {
    db.tool_outcome_with_log(op, state, output, turn_id, &turn_log.to_json().to_string())?;
    emit_tool_finish(tool_event, turn_id, op, name, state, output);
    Ok(())
}

/// Initialize guidance belongs to the ephemeral request, never TurnLog/history.
/// A server must own at least one attached tool permitted in this exact lane.
fn mcp_instruction_input(attached: &McpGeneration, policy: &RuntimePolicy<'_>) -> Vec<InputItem> {
    attached
        .servers
        .iter()
        .filter_map(|server| {
            let tools: Vec<&str> = attached
                .entries
                .iter()
                .filter(|entry| {
                    entry.server == server.server_id && policy.check(&entry.namespaced).is_ok()
                })
                .map(|entry| entry.namespaced.as_str())
                .collect();
            if tools.is_empty() {
                return None;
            }
            let instructions = match &server.client {
                AttachedServer::Remote(client) => client.instructions(),
                AttachedServer::Stdio(client) => client.instructions(),
            };
            let instructions = instructions?;
            Some(InputItem::message(
                InputRole::Developer,
                format!(
                    "MCP server guidance for permitted tools [{}]:\n{instructions}",
                    tools.join(", ")
                ),
            ))
        })
        .collect()
}

fn mcp_redactions(config: &Generation, parent_env: &BTreeMap<String, String>) -> Vec<String> {
    let mut secrets: Vec<String> = parent_env
        .iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_uppercase();
            [
                "KEY",
                "TOKEN",
                "SECRET",
                "PASSWORD",
                "CREDENTIAL",
                "AUTH",
                "COOKIE",
                "BEARER",
            ]
            .iter()
            .any(|part| name.contains(part))
        })
        .map(|(_, value)| value.clone())
        .collect();
    for provider in config.providers.values() {
        secrets.extend([
            provider.options.api_key.clone(),
            provider.options.base_url.clone(),
        ]);
        secrets.extend(provider.options.headers.values().cloned());
    }
    for entry in config.mcp.values() {
        secrets.extend(entry.url.iter().cloned());
        secrets.extend(entry.headers.values().cloned());
        // A header may be reflected as its token without the Bearer prefix.
        secrets.extend(
            entry
                .headers
                .values()
                .filter_map(|v| v.split_once(' ').map(|(_, token)| token.to_string())),
        );
    }
    secrets
}

fn remote_mcp_failure(error: McpError, cancel: &AtomicBool) -> (&'static str, String) {
    if cancel.load(Ordering::Relaxed) || error == McpError::Cancelled {
        return ("cancelled", "error: cancelled".into());
    }
    let message = match error {
        McpError::ToolFailedDetail(detail) => return ("failed", format!("error: mcp {detail}")),
        McpError::ToolFailed => "error: mcp tool reported failure",
        McpError::UnsupportedResult | McpError::BadResult => "error: unsupported mcp result",
        McpError::InvalidArguments => "error: invalid mcp arguments",
        _ => "error: mcp transport failure",
    };
    ("failed", message.into())
}

fn stdio_mcp_failure(error: StdioError, cancel: &AtomicBool) -> (&'static str, String) {
    if cancel.load(Ordering::Relaxed) || error == StdioError::Cancelled {
        return ("cancelled", "error: cancelled".into());
    }
    let message = match error {
        StdioError::ToolFailedDetail(detail) => return ("failed", format!("error: mcp {detail}")),
        StdioError::ToolFailed => "error: mcp tool reported failure",
        StdioError::UnsupportedModality | StdioError::BadResult => "error: unsupported mcp result",
        StdioError::NonObjectArguments => "error: invalid mcp arguments",
        _ => "error: mcp transport failure",
    };
    ("failed", message.into())
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

fn remote_attach_error(server: &str, error: McpError) -> RuntimeError {
    remote_attach_error_at(server, "tools-list", error)
}

fn stdio_attach_error(server: &str, error: StdioError) -> RuntimeError {
    stdio_attach_error_at(server, "tools-list", error)
}

fn safe_server_id(server: &str) -> String {
    server
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn remote_attach_error_at(server: &str, stage: &'static str, error: McpError) -> RuntimeError {
    let (stage, safe_code, retryable) = match error {
        McpError::Cancelled => return RuntimeError::Cancelled,
        McpError::InvalidConfig => ("config", "invalid_config", false),
        McpError::InvalidHeader(_) => ("config", "invalid_header", false),
        McpError::ConflictingHeader(name) if name == "authorization" => {
            ("config", "authorization_header_conflict", false)
        }
        McpError::ConflictingHeader(_) => ("config", "header_conflict", false),
        McpError::PrivateHost => ("DNS", "private_host", false),
        McpError::Dns => ("DNS", "resolution_failed", true),
        McpError::Connect => ("connect", "connection_failed", true),
        McpError::CleanupFailed => ("cleanup", "cleanup_failed", false),
        McpError::Unauthorized => (stage, "unauthorized", false),
        McpError::Forbidden => (stage, "forbidden", false),
        McpError::Deadline => (stage, "deadline", true),
        McpError::Transport => (stage, "transport", true),
        McpError::ProtocolMismatch => (stage, "protocol_mismatch", false),
        McpError::CatalogLimited => (stage, "catalog_limit", false),
        McpError::Catalog(_) => (stage, "invalid_catalog", false),
        McpError::ToolFailed | McpError::ToolFailedDetail(_) => (stage, "tool_failed", false),
        McpError::InvalidArguments => (stage, "invalid_arguments", false),
        McpError::BadResult => (stage, "bad_result", false),
        McpError::UnsupportedResult => (stage, "unsupported_result", false),
    };
    RuntimeError::McpAttach {
        server: safe_server_id(server),
        stage,
        safe_code,
        retryable,
    }
}

fn stdio_attach_error_at(server: &str, stage: &'static str, error: StdioError) -> RuntimeError {
    let (stage, safe_code, retryable) = match error {
        StdioError::Cancelled => return RuntimeError::Cancelled,
        StdioError::InvalidConfig => ("config", "invalid_config", false),
        StdioError::Disabled => ("config", "disabled", false),
        StdioError::Spawn => ("spawn", "spawn_failed", false),
        StdioError::Deadline => (stage, "deadline", true),
        StdioError::Transport => (stage, "transport", true),
        StdioError::CleanupFailed => ("cleanup", "cleanup_failed", false),
        StdioError::CatalogLimited => (stage, "catalog_limit", false),
        StdioError::ToolFailed | StdioError::ToolFailedDetail(_) => (stage, "tool_failed", false),
        StdioError::NonObjectArguments => (stage, "invalid_arguments", false),
        StdioError::UnsupportedModality => (stage, "unsupported_result", false),
        StdioError::BadResult => (stage, "bad_result", false),
    };
    RuntimeError::McpAttach {
        server: safe_server_id(server),
        stage,
        safe_code,
        retryable,
    }
}

fn map_messages(history: &[(String, String, String)]) -> Result<Vec<Message>, RuntimeError> {
    let mut out = Vec::with_capacity(history.len());
    for (id, role, text) in history {
        let role = match role.as_str() {
            "user" => Role::User,
            "assistant" => Role::Assistant,
            _ => {
                return Err(RuntimeError::Compress(format!("unknown role for {id}")));
            }
        };
        let Some(id) = MessageId::new(id.clone()) else {
            return Err(RuntimeError::Compress("empty message id".to_string()));
        };
        out.push(Message {
            id,
            role,
            text: text.clone(),
        });
    }
    Ok(out)
}

fn dcp_config_input(
    projected: &[(String, String, String)],
    config: &DcpConfig,
) -> Option<InputItem> {
    if !config.enabled || config.manual_mode {
        return None;
    }
    let anchors = projected
        .iter()
        .enumerate()
        .map(|(index, (id, role, _))| {
            serde_json::json!({
                "id": id,
                "role": role,
                "closed": index + 1 < projected.len(),
            })
        })
        .collect::<Vec<_>>();
    Some(InputItem::message(
        InputRole::Developer,
        format!(
            "DCP context anchors in order. Compress only closed=true spans; the final anchor is unfinished: {}",
            serde_json::Value::Array(anchors)
        ),
    ))
}

fn plan_dcp_strategies(
    input: &[InputItem],
    config: &DcpConfig,
    existing: &crate::storage::DcpToolProjection,
) -> crate::storage::DcpToolProjection {
    let mut projection = crate::storage::DcpToolProjection::default();
    if !config.enabled || (config.manual_mode && !config.automatic_strategies) {
        return projection;
    }
    let total_users = input
        .iter()
        .filter(|item| {
            matches!(
                item,
                InputItem::Message {
                    role: InputRole::User,
                    ..
                }
            )
        })
        .count() as u64;
    let mut outputs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in input {
        if let InputItem::FunctionCallOutput { call_id, output } = item {
            outputs
                .entry(call_id.clone())
                .or_default()
                .push(output.clone());
        }
    }
    let mut users_seen = 0u64;
    let mut calls = Vec::new();
    let mut occurrences: BTreeMap<String, u64> = BTreeMap::new();
    for (index, item) in input.iter().enumerate() {
        if matches!(
            item,
            InputItem::Message {
                role: InputRole::User,
                ..
            }
        ) {
            users_seen += 1;
        }
        let InputItem::ProviderOutput(value) = item else {
            continue;
        };
        if value["type"] != "function_call" {
            continue;
        }
        let Some(call_id) = value["call_id"].as_str().map(str::to_owned) else {
            continue;
        };
        let occurrence = occurrences.entry(call_id.clone()).or_default();
        let call_key = (call_id.clone(), *occurrence);
        *occurrence += 1;
        if existing.hidden.contains(&call_key) {
            continue;
        }
        let name = value["name"].as_str().unwrap_or_default().to_owned();
        let arguments = value["arguments"].as_str().unwrap_or_default().to_owned();
        let file_protected = dcp_call_has_protected_path(&name, &arguments, config);
        let generally_protected =
            crate::dcp_auto::tool_is_protected(&config.protected_tools, &name);
        let dedup_protected = file_protected
            || generally_protected
            || crate::dcp_auto::tool_is_protected(&config.dedup_protected_tools, &name);
        let purge_protected = file_protected
            || generally_protected
            || crate::dcp_auto::tool_is_protected(&config.purge_protected_tools, &name);
        let age = total_users.saturating_sub(users_seen);
        let turn_protected = config.turn_protection && age <= config.turn_protection_turns;
        let dedup_protected = dedup_protected || turn_protected;
        let purge_protected = purge_protected || turn_protected;
        if config.purge_errors
            && !purge_protected
            && arguments.len() > crate::dcp_auto::LARGE_INPUT_BYTES
            && age >= config.purge_after_turns
            && outputs
                .get(&call_id)
                .and_then(|values| values.get(call_key.1 as usize))
                .is_some_and(|output| output.starts_with("error:"))
        {
            projection.purged.insert(call_key.clone());
        }
        calls.push((
            index,
            call_key,
            name,
            canonical_json(&arguments),
            dedup_protected,
        ));
    }
    if !config.deduplication {
        return projection;
    }
    let mut last = BTreeMap::new();
    for (index, _, name, arguments, _) in &calls {
        last.insert((name.clone(), arguments.clone()), *index);
    }
    projection.hidden = calls
        .into_iter()
        .filter(|(index, _, name, arguments, protected)| {
            !protected && last.get(&(name.clone(), arguments.clone())) != Some(index)
        })
        .map(|(_, call_key, _, _, _)| call_key)
        .collect();
    projection
}

fn apply_turn_protection(
    history: &[(String, String, String)],
    config: &DcpConfig,
    spec: &mut ProtectedSpec,
) {
    if !config.turn_protection || config.turn_protection_turns == 0 {
        return;
    }
    let closed_end = if history.last().is_some_and(|(_, role, _)| role == "user") {
        history.len().saturating_sub(1)
    } else {
        history.len()
    };
    let Some(start) = history[..closed_end]
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, (_, role, _))| role == "user")
        .nth(usize::try_from(config.turn_protection_turns.saturating_sub(1)).unwrap_or(usize::MAX))
        .map(|(index, _)| index)
        .or_else(|| {
            history[..closed_end]
                .iter()
                .position(|(_, role, _)| role == "user")
        })
    else {
        return;
    };
    spec.protected_message_ids.extend(
        history[start..closed_end]
            .iter()
            .map(|(id, _, _)| id.clone()),
    );
}

fn apply_dcp_projection(
    input: &mut Vec<InputItem>,
    projection: &crate::storage::DcpToolProjection,
) {
    let mut calls: BTreeMap<String, u64> = BTreeMap::new();
    let mut outputs: BTreeMap<String, u64> = BTreeMap::new();
    input.retain_mut(|item| match item {
        InputItem::ProviderOutput(value) if value["type"] == "function_call" => {
            let Some(call_id) = value["call_id"].as_str().map(str::to_owned) else {
                return true;
            };
            let occurrence = calls.entry(call_id.clone()).or_default();
            let key = (call_id, *occurrence);
            *occurrence += 1;
            if projection.purged.contains(&key) {
                value["arguments"] = serde_json::Value::String(
                    serde_json::json!({"purged": "large error input"}).to_string(),
                );
            }
            !projection.hidden.contains(&key)
        }
        InputItem::FunctionCallOutput { call_id, .. } => {
            let occurrence = outputs.entry(call_id.clone()).or_default();
            let key = (call_id.clone(), *occurrence);
            *occurrence += 1;
            !projection.hidden.contains(&key)
        }
        _ => true,
    });
}

fn canonical_json(raw: &str) -> String {
    fn sort(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(object) => serde_json::Value::Object(
                object
                    .into_iter()
                    .map(|(key, value)| (key, sort(value)))
                    .collect::<std::collections::BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(sort).collect())
            }
            value => value,
        }
    }
    serde_json::from_str(raw)
        .map(sort)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| raw.to_string())
}

fn dcp_call_has_protected_path(name: &str, arguments: &str, config: &DcpConfig) -> bool {
    fn contains_path(value: &serde_json::Value, patterns: &[String]) -> bool {
        match value {
            serde_json::Value::String(value) => crate::dcp::path_is_protected(patterns, value),
            serde_json::Value::Array(values) => {
                values.iter().any(|value| contains_path(value, patterns))
            }
            serde_json::Value::Object(values) => {
                values.values().any(|value| contains_path(value, patterns))
            }
            _ => false,
        }
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(arguments) else {
        return name == "apply_patch";
    };
    if name == "apply_patch"
        && let Some(patch) = value.get("patchText").and_then(|value| value.as_str())
    {
        return crate::patch::affected_paths(patch).map_or(true, |paths| {
            paths
                .iter()
                .any(|path| crate::dcp::path_is_protected(&config.protected_file_patterns, path))
        });
    }
    contains_path(&value, &config.protected_file_patterns)
}

fn active_summary_tokens(projected: &[(String, String, String)]) -> u64 {
    projected
        .iter()
        .filter(|(id, _, _)| id.starts_with('b'))
        .map(|(_, _, text)| estimate_tokens(text))
        .sum()
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
fn next_turn_id(session: &str, timestamp: u64) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("turn identity space exhausted");
    format!("t{session}-{timestamp}-{}-{serial}", std::process::id())
}

#[cfg(test)]
mod identity_tests {
    #[test]
    fn turn_ids_do_not_repeat_when_clock_repeats_or_rolls_back() {
        let mut ids = std::collections::BTreeSet::new();
        for timestamp in [123, 123, 122, 123] {
            for session in ["a", "b", "a"] {
                assert!(ids.insert(super::next_turn_id(session, timestamp)));
            }
        }
    }
}
