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
use std::sync::atomic::{AtomicBool, Ordering};
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
use crate::storage::{Db, StorageError};
use crate::tools::{
    Assembled, CallFailure, SkillSnapshot, ToolContext, ToolError, ToolPolicy, ToolRoots, TurnLog,
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
pub const SESSION_LOCATION_PREFIX: &str = "tui.session_location.";
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
            Self::McpAttach { server } => write!(f, "mcp attach failed for {server}"),
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
}

impl<'a> RuntimePolicy<'a> {
    /// Bridge one permission map.
    pub fn new(permissions: &'a BTreeMap<String, Permission>) -> Self {
        Self { permissions }
    }
}

impl ToolPolicy for RuntimePolicy<'_> {
    fn check(&self, tool: &str) -> Result<(), ToolError> {
        match self.permissions.get(tool) {
            Some(Permission::Allow) => Ok(()),
            Some(Permission::Deny) | Some(Permission::Ask) | None => Err(ToolError::Denied {
                tool: tool.to_string(),
            }),
        }
    }
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
struct McpGeneration {
    publication: u64,
    servers: Vec<AttachedMcp>,
    entries: Vec<mcp_remote::RegistryEntry>,
}

impl McpGeneration {
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
                Ok(registry) => replacements[index] = Some(registry),
                Err(error) => {
                    self.restore_dirty(&claims);
                    return Err(error);
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
    /// Billed usage, if reported.
    pub usage: Option<(u64, u64)>,
    /// Executed calls in order.
    pub calls: Vec<CallRecord>,
    /// Transient nudge hint (never persisted).
    pub nudge_hint: Option<String>,
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
    /// Requested max output tokens (admission).
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
    fixed_input: Vec<InputItem>,
    skills: SkillSnapshot,
    agent_digest: Option<String>,
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

    /// Atomically publish a fully built candidate generation. Only lands
    /// between turns; returns the new id.
    pub async fn reload(&self, generation: Generation) -> Result<u64, RuntimeError> {
        let _lease = self.begin_active()?;
        if let Some(old) = self.mcp_generation.lock().await.take() {
            close_generation(old).await?;
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
        if let Some(generation) = self.mcp_generation.lock().await.take() {
            close_generation(generation).await?;
        }
        Ok(())
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

    /// Publish fixed instructions, primary prompt and pinned skills together.
    pub fn publish_workspace(
        &self,
        agent_prompt: Option<&str>,
        instructions: &str,
        files: Vec<(String, String)>,
        skill_errors: BTreeMap<String, String>,
        agent_digest: Option<String>,
    ) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        let (mut skills, warnings) = SkillSnapshot::build(&files);
        if !warnings.is_empty() {
            return Err(RuntimeError::InvalidArgs(warnings.join("; ")));
        }
        skills.errors = skill_errors;
        let mut fixed_input = Vec::new();
        if let Some(prompt) = agent_prompt.filter(|prompt| !prompt.trim().is_empty()) {
            fixed_input.push(InputItem::message(InputRole::Developer, prompt));
        }
        if !instructions.trim().is_empty() {
            fixed_input.push(InputItem::message(InputRole::Developer, instructions));
        }
        let projection = skills.projection();
        if !projection.is_empty() {
            let catalog = serde_json::to_string(&projection).map_err(|_| RuntimeError::Storage)?;
            fixed_input.push(InputItem::message(
                InputRole::Developer,
                format!(
                    "Available native skills (metadata only; call skill by id for body): {catalog}"
                ),
            ));
        }
        *self.workspace.write().expect("workspace lock") = RuntimeWorkspace {
            fixed_input,
            skills,
            agent_digest,
        };
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
        let key = session_location_key(id);
        if let Some(owner) = self.db.get_pref(&key)? {
            if owner != self.location {
                return Err(RuntimeError::LocationMismatch {
                    session: id.to_string(),
                    location: owner,
                });
            }
            return Ok(());
        }
        self.db.create_session(id)?;
        self.db.set_pref(&key, &self.location)?;
        Ok(())
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
        self.run_turn_with_events(params, |_| {}, |_, _| {}).await
    }

    /// Notify the application only after validated input is durably accepted.
    pub async fn run_turn_with_events(
        &self,
        params: TurnParams<'_>,
        mut accepted: impl FnMut(&str) + Send,
        mut text_delta: impl FnMut(&str, &str) + Send,
    ) -> Result<TurnReport, RuntimeError> {
        let _lease = self.begin_active()?;
        let published = self.current.read().expect("generation lock").clone();
        let mut mcp = self.mcp_generation.lock().await;
        let attached = self
            .ensure_mcp_generation(&mut mcp, &published, params.cancel)
            .await?;
        let result = self
            .run_turn_inner(params, attached, &mut accepted, &mut text_delta)
            .await;
        drop(mcp);
        result
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
        {
            let policy = RuntimePolicy::new(&published.config.permissions);
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
        let history = self.db.read_history_full(session)?;
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

    async fn run_turn_inner(
        &self,
        params: TurnParams<'_>,
        attached: &McpGeneration,
        accepted: &mut (dyn FnMut(&str) + Send),
        text_delta: &mut (dyn FnMut(&str, &str) + Send),
    ) -> Result<TurnReport, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        self.open_session(&params.session)?;
        // Exact model selection + admission before any side effect.
        let base = models::select_model(params.catalog, &params.model_id)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let selection = models::select_variant(&base, params.variant.as_deref())
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        let workspace = self.workspace.read().expect("workspace lock").clone();
        // Outbound context honors compression blocks + prune mark: covered
        // members collapse to summaries, raw history is never rewritten.
        let ActiveContext {
            after_seq,
            mut projected,
            blocks: sblocks,
        } = self.active_projection(&params.session)?;
        let mut history = self.wire_history(
            &params.session,
            &projected,
            &sblocks,
            &selection.id,
            &params.catalog.provider,
            workspace.agent_digest.as_deref(),
            after_seq,
        )?;
        let dcp_config = self.dcp_config.read().expect("dcp lock").clone();
        let compress_available = dcp_config.enabled
            && !dcp_config.manual_mode
            && published.config.permissions.get(COMPRESS_TOOL) == Some(&Permission::Allow);
        let model_context = selection
            .entry
            .pointer("/limit/context")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                RuntimeError::InvalidArgs("selected model has no context limit".into())
            })?;
        let dcp_model_key = format!("{}/{}", params.catalog.provider, selection.id);
        let dcp_thresholds = dcp_config.effective_for_context(&dcp_model_key, model_context);
        if dcp_thresholds.min_context > dcp_thresholds.max_context {
            return Err(RuntimeError::InvalidArgs(
                "effective DCP minContextLimit exceeds maxContextLimit".into(),
            ));
        }
        let mut tool_projection = self.db.load_dcp_tool_projection(&params.session)?;
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
                let state = match self.db.get_pref(&state_key)? {
                    Some(raw) => serde_json::from_str(&raw).map_err(|_| RuntimeError::Storage)?,
                    None => NudgeState::default(),
                };
                states.insert(state_key.clone(), state);
            }
        }
        let mut nudge_hint = None;
        // Admission against the entry limits with the assembled estimate.
        let assembled_estimate = estimate_tokens(
            &serde_json::to_string(&(workspace.fixed_input.as_slice(), history.as_slice()))
                .map_err(|_| RuntimeError::Storage)?,
        ) + estimate_tokens(&params.prompt);
        models::admit(&selection, assembled_estimate, params.max_output)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        // Durable intent before any side effect.
        let turn_id = format!("t{}-{}", params.session, millis());
        let user_text = params.invocation.as_deref().unwrap_or(&params.prompt);
        let user_message =
            self.db
                .accept_turn(&turn_id, &params.session, &params.prompt, user_text)?;
        accepted(&turn_id);
        let mut tool_defs = builtin_tool_defs();
        if !compress_available {
            tool_defs.retain(|tool| tool.name != COMPRESS_TOOL);
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
        let policy = RuntimePolicy::new(&published.config.permissions);
        let ctx = ToolContext {
            files: &self.files,
            shell: &self.shell,
            parent_env: &self.parent_env,
            webfetch_auth: self.webfetch_auth.clone(),
            webfetch_allow_private: self.webfetch_allow_private,
            policy: &policy,
            snapshot: &snapshot,
            cancel: params.cancel,
            roots: Some(self.roots.clone()),
        };
        let mut text = String::new();
        let mut usage = None;
        let mut calls = Vec::new();
        let mut turn_log = TurnLog::new(&turn_id, &selection.id, &params.catalog.provider);
        turn_log.agent_digest = workspace.agent_digest.clone();
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
                    usage,
                    calls,
                    nudge_hint,
                    &published,
                );
            }
            let (nudge, persisted_nudge) = {
                let estimate = estimate_tokens(
                    &serde_json::to_string(&(
                        workspace.fixed_input.as_slice(),
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
            let input: Vec<InputItem> = workspace
                .fixed_input
                .iter()
                .chain(nudge_input.iter())
                .chain(anchors.iter())
                .chain(&history)
                .chain(&turn_log.input)
                .cloned()
                .collect();
            let generation = match crate::provider::stream_input_observed(
                &params.provider,
                &selection.id,
                selection.variant.as_ref(),
                &input,
                &tool_defs,
                params.max_output,
                params.cancel,
                &mut |item| {
                    if let crate::provider::StreamItem::TextDelta(delta) = item {
                        text_delta(&turn_id, delta);
                    }
                },
            )
            .await
            {
                Ok(generation) => generation,
                Err(error) => {
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
                        usage,
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
            text.push_str(&generation.text);
            if generation.usage.is_some() {
                usage = generation.usage;
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
                        usage,
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
            turn_log
                .input
                .extend(generation.output.into_iter().map(InputItem::ProviderOutput));
            // Text-only synthetic peers may omit canonical messages. This is plain
            // assistant text, never reconstruction of reasoning or function calls.
            if !generation.text.is_empty() && !has_message {
                turn_log
                    .input
                    .push(InputItem::message(InputRole::Assistant, &generation.text));
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
                    &state_key,
                    &mut tool_projection,
                )
                .await?;
            calls.extend(round_calls);
            if projection_changed {
                let refreshed = self.active_projection(&params.session)?;
                projected = refreshed.projected;
                history = self.wire_history(
                    &params.session,
                    &projected,
                    &refreshed.blocks,
                    &selection.id,
                    &params.catalog.provider,
                    workspace.agent_digest.as_deref(),
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
                    usage,
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
            usage,
            calls,
            nudge_hint,
            &published,
        )
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
        usage: Option<(u64, u64)>,
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
            calls,
            nudge_hint,
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
        let mut represented = std::collections::BTreeSet::new();
        let covered_anchors = blocks
            .iter()
            .flat_map(|block| block.members.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        for raw in self
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
            if log.model != model || log.provider != provider {
                return Err(RuntimeError::InvalidArgs(
                    "session wire history belongs to a different provider/model".to_string(),
                ));
            }
            if log.agent_digest.as_deref() != agent_digest {
                // Agent behavior changed: start a fresh provider causality lane
                // from immutable raw messages, never replay old opaque/tool state.
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
        nudge_key: &str,
        tool_projection: &mut crate::storage::DcpToolProjection,
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
                Assembled::Call(call) if policy.check(&call.name).is_err() => {
                    let state = if is_builtin(&call.name) {
                        "failed"
                    } else {
                        "denied"
                    };
                    Some((state, format!("error: denied {}", call.name)))
                }
                _ if cancel.load(Ordering::Relaxed) => {
                    Some(("cancelled", "error: cancelled".to_string()))
                }
                _ => None,
            };
            // Fail closed. No built-in or MCP dispatch can precede this commit.
            self.db
                .record_tool_intent(&op, session, Some(turn_id), name, &input)?;
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
                            self.db.tool_outcome_with_log(
                                &op,
                                "no_gain",
                                &output,
                                turn_id,
                                &turn_log.to_json().to_string(),
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
                        self.db.tool_outcome_with_log(
                            &op,
                            "no_gain",
                            &output,
                            turn_id,
                            &turn_log.to_json().to_string(),
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
                        self.db.tool_outcome_with_log(
                            &op,
                            "failed",
                            &output,
                            turn_id,
                            &turn_log.to_json().to_string(),
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
                    Assembled::Call(call) if is_builtin(&call.name) => {
                        let output = execute_batch(ctx, vec![guarded]).await.remove(0).output;
                        (output_state(&output), output)
                    }
                    Assembled::Call(call) => Self::execute_mcp(call, attached, cancel).await,
                    Assembled::Failed(_) => unreachable!("assembly failure rejected above"),
                }
            };
            // Failure here leaves started/unknown; never continue the batch.
            turn_log.input.push(InputItem::FunctionCallOutput {
                call_id: id.clone(),
                output: output.clone(),
            });
            self.db.tool_outcome_with_log(
                &op,
                state,
                &output,
                turn_id,
                &turn_log.to_json().to_string(),
            )?;
            records.push(CallRecord {
                name: name.to_string(),
                state: state.to_string(),
                output: truncate(&output, REPORT_OUTPUT_CAP),
            });
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
        call: &crate::tools::ToolCall,
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
        match &server.client {
            AttachedServer::Remote(client) => client
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| remote_mcp_failure(error, cancel),
                    |text| ("completed", text),
                ),
            AttachedServer::Stdio(child) => child
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| stdio_mcp_failure(error, cancel),
                    |text| ("completed", text),
                ),
        }
    }

    /// Reuse one connected MCP registry for the whole Location/config generation.
    async fn ensure_mcp_generation<'m>(
        &self,
        slot: &'m mut Option<McpGeneration>,
        published: &PublishedGeneration,
        cancel: &AtomicBool,
    ) -> Result<&'m McpGeneration, RuntimeError> {
        let reusable = slot
            .as_ref()
            .is_some_and(|generation| generation.publication == published.id);
        if !reusable {
            if let Some(old) = slot.take() {
                close_generation(old).await?;
            }
            *slot = Some(self.attach_mcp(published, cancel).await?);
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
        let mut ids: Vec<&String> = published.config.mcp.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &published.config.mcp[id];
            if !entry.enabled {
                continue;
            }
            let result = if entry.kind == "remote" {
                let config = mcp_remote::CodexWebConfig::from_entry(entry)
                    .map(|mut config| {
                        config.allow_private = self
                            .parent_env
                            .get("OC_TEST_ALLOW_LOOPBACK")
                            .is_some_and(|value| value == "1");
                        config
                    })
                    .map_err(|error| remote_attach_error(id, error));
                match config {
                    Ok(config) => {
                        let client = tokio::select! {
                            biased;
                            _ = crate::provider::wait_cancel(cancel) => Err(RuntimeError::Cancelled),
                            result = CodexWebClient::connect(&config) => {
                                result.map_err(|error| remote_attach_error(id, error))
                            }
                        };
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
                        .map_err(|error| stdio_attach_error(id, error));
                match config {
                    Ok(config) => {
                        let client = tokio::select! {
                            biased;
                            _ = crate::provider::wait_cancel(cancel) => Err(RuntimeError::Cancelled),
                            result = StdioClient::launch(&config) => {
                                result.map_err(|error| stdio_attach_error(id, error))
                            }
                        };
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
                Err(RuntimeError::McpAttach { server: id.clone() })
            };
            match result {
                Ok((server, registry)) => {
                    attached.push(server);
                    registries.push(registry);
                }
                Err(error) => {
                    let cleanup = close_generation(McpGeneration {
                        publication: published.id,
                        servers: attached,
                        entries: Vec::new(),
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
            }),
            Err(error) => {
                let cleanup = close_generation(McpGeneration {
                    publication: published.id,
                    servers: attached,
                    entries: Vec::new(),
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

fn is_builtin(name: &str) -> bool {
    crate::tools::MODEL_TOOL_NAMES.contains(&name)
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

fn remote_mcp_failure(error: McpError, cancel: &AtomicBool) -> (&'static str, String) {
    if cancel.load(Ordering::Relaxed) || error == McpError::Cancelled {
        return ("cancelled", "error: cancelled".into());
    }
    let message = match error {
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

fn remote_attach_error(server: &str, error: McpError) -> RuntimeError {
    match error {
        McpError::Cancelled => RuntimeError::Cancelled,
        McpError::InvalidHeader(_)
        | McpError::ConflictingHeader(_)
        | McpError::CatalogLimited
        | McpError::Catalog(_) => RuntimeError::InvalidArgs(format!("mcp {server}: {error}")),
        _ => RuntimeError::McpAttach {
            server: server.to_string(),
        },
    }
}

fn stdio_attach_error(server: &str, error: StdioError) -> RuntimeError {
    match error {
        StdioError::Cancelled => RuntimeError::Cancelled,
        StdioError::CatalogLimited => RuntimeError::InvalidArgs(format!("mcp {server}: {error}")),
        _ => RuntimeError::McpAttach {
            server: server.to_string(),
        },
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
