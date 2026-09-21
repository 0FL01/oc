//! Authoritative runtime turn loop (T24): one prompt assembler,
//! generation-guarded turns, a unified permission path for built-ins/MCP/
//! compress, Location-scoped sessions, MCP/DCP/reasoning cleanup, and
//! config reload between turns only.
//!
//! The provider layer streams one text prompt per round, so multi-round
//! tool continuation carries prior results as bounded text (documented
//! carryover, not a hidden protocol). Tool side effects are inherently
//! non-transactional; the staleness guarantee covers durable records
//! (turn result, messages, compression blocks), which never commit under
//! a superseded generation.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use oc_core::context_plan::ProtectedSpec;
use oc_core::session::{Message, MessageId, Role};

use crate::config::{Generation, Permission};
use crate::dcp_auto::{DcpConfig, DcpStats, NudgeState, evaluate};
use crate::mcp_remote::{self, CodexWebClient, RemoteTool};
use crate::mcp_stdio::{StdioClient, StdioConfig};
use crate::models::{self, ModelCatalog};
use crate::patch::ProtectedGlobs;
use crate::provider::{ResponsesConfig, ToolDef};
use crate::storage::{Db, StorageError};
use crate::tools::{
    Assembled, CallFailure, FunctionCallOutput, SkillSnapshot, ToolContext, ToolError, ToolPolicy,
    ToolRoots, TurnLog, assemble_calls, execute_batch,
};

/// Max tool rounds per turn (bounded agent loop).
pub const MAX_ROUNDS: u32 = 8;
/// Hard cap for a caller-supplied round limit.
pub const ROUND_CAP: u32 = 16;
/// Max assembled prompt bytes (oldest history drops first).
pub const INPUT_BYTES_CAP: usize = 65_536;
/// Max tool-output bytes carried into the next round.
pub const PRIOR_OUTPUT_CAP: usize = 4_096;
/// Max tool-output bytes kept in the turn report.
pub const REPORT_OUTPUT_CAP: usize = 2_048;
/// Max command definition/invocation bytes for one expansion.
pub const COMMAND_BYTES_CAP: usize = 4_096;
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
    /// Provider failure (kind only).
    Provider,
    /// Storage failure (kind only).
    Storage,
    /// Compress argument/apply failure (reason only).
    Compress(String),
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
            Self::Provider => write!(f, "provider error"),
            Self::Storage => write!(f, "storage error"),
            Self::Compress(reason) => write!(f, "compress: {reason}"),
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
            name: "apply_patch".to_string(),
            description: "Apply a patch to project files. `patch` must start with `*** Begin Patch` and end with `*** End Patch`; update hunks look like `*** Update File: <relative path>` then `@@` then lines where context starts with a space, removals with `-`, additions with `+`, all matching file bytes exactly. Paths stay inside the project root."
                .to_string(),
            parameters: schema(serde_json::json!({"patch": {"type": "string"}}), &["patch"]),
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
    ]
}

/// One prior round carried into the next prompt as bounded text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundSummary {
    /// Model text of the round.
    pub text: String,
    /// `(tool name, bounded output)` pairs in order.
    pub calls: Vec<(String, String)>,
}

/// The single prompt assembler: history projection + user text + prior
/// round carryover, capped at [`INPUT_BYTES_CAP`] by dropping oldest
/// history first (user text and the latest round always survive).
pub fn assemble_turn_input(
    history: &[(String, String)],
    user: &str,
    prior: &[RoundSummary],
) -> String {
    let mut sections = vec![format!("user: {user}")];
    for round in prior {
        let mut section = String::from("previous round model text:\n");
        section.push_str(&round.text);
        for (name, output) in &round.calls {
            section.push_str(&format!("\ntool {name} output:\n{output}"));
        }
        sections.push(section);
    }
    let mut history_lines: Vec<String> = history
        .iter()
        .map(|(role, text)| format!("{role}: {text}"))
        .collect();
    while assembled_len(&history_lines, &sections) > INPUT_BYTES_CAP && !history_lines.is_empty() {
        history_lines.remove(0);
    }
    history_lines
        .into_iter()
        .chain(sections)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn assembled_len(history: &[String], sections: &[String]) -> usize {
    history.iter().map(String::len).sum::<usize>()
        + sections.iter().map(String::len).sum::<usize>()
        + (history.len() + sections.len()) * 2
}

/// Rough token estimate (bytes/4 heuristic, documented at call sites).
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
    let mut out = body.replace("$ARGUMENTS", &args.join(" "));
    for (i, arg) in args.iter().take(9).enumerate() {
        out = out.replace(&format!("${}", i + 1), arg);
    }
    if out.len() > COMMAND_BYTES_CAP {
        return Err(RuntimeError::InvalidArgs(
            "expanded command too large".to_string(),
        ));
    }
    Ok(out)
}

/// Attached MCP server: remote or stdio behind one call shape.
enum AttachedServer {
    Remote(CodexWebClient),
    Stdio(StdioClient),
}

/// Attached servers for one turn; [`McpAttachment::close`] reaps children.
struct McpAttachment {
    servers: Vec<AttachedMcp>,
}

impl McpAttachment {
    /// Shut down stdio children (reap ladder); remote clients drop-close.
    async fn close(self) {
        for server in self.servers {
            if let AttachedServer::Stdio(client) = server.client {
                let _ = client.shutdown().await;
            }
        }
    }
}

/// Server id + client + namespaced registry entries.
struct AttachedMcp {
    server_id: String,
    client: AttachedServer,
    entries: Vec<RegistryEntry>,
}

/// Registry entry shared across MCP transports.
#[derive(Debug, Clone)]
struct RegistryEntry {
    namespaced: String,
    tool: String,
    schema: serde_json::Value,
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
}

impl TurnStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
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
    skills: RwLock<Vec<(String, String)>>,
    nudge_state: Mutex<NudgeState>,
    stats: Mutex<crate::dcp_auto::DcpStats>,
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
            skills: RwLock::new(Vec::new()),
            nudge_state: Mutex::new(NudgeState::default()),
            stats: Mutex::new(DcpStats::default()),
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
    pub fn reload(&self, generation: Generation) -> Result<u64, RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        let mut current = self.current.write().expect("generation lock");
        let id = current.id + 1;
        *current = Arc::new(PublishedGeneration {
            id,
            config: generation,
        });
        Ok(id)
    }

    /// Replace the DCP config between turns.
    pub fn reload_dcp(&self, config: DcpConfig) -> Result<(), RuntimeError> {
        if self.active.load(Ordering::Relaxed) {
            return Err(RuntimeError::TurnActive);
        }
        *self.dcp_config.write().expect("dcp lock") = config;
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
        *self.skills.write().expect("skills lock") = files;
        Ok(())
    }

    /// Runtime DCP stats snapshot (counts only).
    pub fn dcp_stats(&self) -> DcpStats {
        self.stats.lock().expect("stats lock").clone()
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
        if self.active.swap(true, Ordering::SeqCst) {
            return Err(RuntimeError::TurnActive);
        }
        // Attach inside the single-flight guard so spawns serialize.
        let attachment = match self.attach_mcp_current().await {
            Ok(attachment) => attachment,
            Err(error) => {
                self.active.store(false, Ordering::SeqCst);
                return Err(error);
            }
        };
        let result = self
            .run_turn_inner(params, &attachment.servers, &mut accepted, &mut text_delta)
            .await;
        // Reap own children on every path (STORE05); remote drops close.
        attachment.close().await;
        self.active.store(false, Ordering::SeqCst);
        result
    }

    /// Execute a manual compress over validated ranges (same permission path).
    pub fn run_compress(
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
        let history = self.db.read_history_full(session)?;
        let messages = map_messages(&history)?;
        let before: Vec<String> = self
            .db
            .load_compression_blocks(session)
            .map_err(|_| RuntimeError::Storage)?
            .into_iter()
            .map(|block| block.id)
            .collect();
        let blocks = match crate::dcp::compress_ranges(self.db, session, &messages, &ranges, spec) {
            Ok(blocks) => blocks,
            Err(error) => {
                // Compensate: remove blocks this attempt stored before failing.
                if let Ok(after) = self.db.load_compression_blocks(session) {
                    for block in after {
                        if !before.contains(&block.id) {
                            let _ = self.db.delete_compression_block(&block.id);
                        }
                    }
                }
                return Err(RuntimeError::Compress(error.to_string()));
            }
        };
        if self.generation_id() != published.id {
            // Superseded mid-apply: roll back this attempt, never half-apply.
            for block in &blocks {
                let _ = self.db.delete_compression_block(block);
            }
            return Err(RuntimeError::StaleGeneration {
                want: published.id,
                got: self.generation_id(),
            });
        }
        // Rough savings: covered history estimate minus summary estimate.
        let history_bytes: usize = messages.iter().map(|message| message.text.len()).sum();
        let summary_bytes: usize = ranges.iter().map(|range| range.summary.len()).sum();
        let saved_tokens = (history_bytes as u64 / 4).saturating_sub(summary_bytes as u64 / 4);
        let outcome = crate::dcp::decide_outcome(saved_tokens);
        let shrank = matches!(outcome, crate::dcp::CompressOutcome::Compressed { .. });
        {
            let mut stats = self.stats.lock().expect("stats lock");
            stats.compressions += 1;
            self.nudge_state
                .lock()
                .expect("nudge lock")
                .on_compress_success();
        }
        Ok(CompressReport {
            blocks,
            saved_tokens,
            shrank,
        })
    }

    async fn run_turn_inner(
        &self,
        params: TurnParams<'_>,
        attached: &[AttachedMcp],
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
        // Attach MCP servers for this generation (fail fast, never silent).
        let mut tool_defs = builtin_tool_defs();
        for server in attached {
            for entry in &server.entries {
                tool_defs.push(ToolDef {
                    name: entry.namespaced.clone(),
                    description: format!("mcp {} tool", entry.tool),
                    parameters: entry.schema.clone(),
                });
            }
        }
        // Outbound context honors compression blocks + prune mark: covered
        // members collapse to summaries, raw history is never rewritten.
        let full = self.db.read_history_full(&params.session)?;
        let sblocks =
            crate::dcp::load_blocks(self.db, &params.session).map_err(|_| RuntimeError::Storage)?;
        let prune = self
            .db
            .load_prune_mark(&params.session)
            .map_err(|_| RuntimeError::Storage)?;
        let history: Vec<(String, String)> =
            crate::dcp::project_history(&full, &sblocks, prune.as_deref());
        let nudge_hint = {
            let mut state = self.nudge_state.lock().expect("nudge lock");
            state.on_turn();
            let config = self.dcp_config.read().expect("dcp lock").clone();
            let estimate =
                estimate_tokens(&history_text(&history)) + estimate_tokens(&params.prompt);
            let hint =
                evaluate(&config, &mut state, &selection.id, estimate).map(|nudge| nudge.text);
            if hint.is_some() {
                self.stats.lock().expect("stats lock").nudges_emitted += 1;
            }
            hint
        };
        // Admission against the entry limits with the assembled estimate.
        let assembled_estimate =
            estimate_tokens(&assemble_turn_input(&history, &params.prompt, &[]));
        models::admit(&selection, assembled_estimate, params.max_output)
            .map_err(|e| RuntimeError::InvalidArgs(e.to_string()))?;
        // Durable intent before any side effect.
        let turn_id = format!("t{}-{}", params.session, millis());
        self.db
            .begin_turn(&turn_id, &params.session, &params.prompt)?;
        let user_text = params.invocation.as_deref().unwrap_or(&params.prompt);
        self.db.append_message(&params.session, "user", user_text)?;
        accepted(&turn_id);
        let snapshot = {
            let skills = self.skills.read().expect("skills lock");
            SkillSnapshot::build(&skills).0
        };
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
        let mut prior = Vec::new();
        let mut turn_log = TurnLog::new(&turn_id, &selection.id, "runtime");
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
            let prompt = assemble_turn_input(&history, &params.prompt, &prior);
            let generation = match crate::provider::stream_generation_observed(
                &params.provider,
                &selection.id,
                selection.variant.as_ref(),
                &prompt,
                &tool_defs,
                params.cancel,
                None,
                &mut |item| {
                    if let crate::provider::StreamItem::TextDelta(delta) = item {
                        text_delta(&turn_id, delta);
                    }
                },
            )
            .await
            {
                Ok(generation) => generation,
                Err(_) => {
                    let status = if params.cancel.load(Ordering::Relaxed) {
                        TurnStatus::Cancelled
                    } else {
                        TurnStatus::Failed
                    };
                    return self.commit_turn(
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
                    );
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
            let units = match assemble_calls(&generation.items) {
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
            let round_calls = self
                .execute_units(
                    &turn_id,
                    &params.session,
                    &units,
                    &ctx,
                    &policy,
                    attached,
                    params.cancel,
                    rounds,
                )
                .await?;
            let summaries: Vec<(String, String)> = round_calls
                .iter()
                .map(|record| {
                    (
                        record.name.clone(),
                        truncate(&record.output, PRIOR_OUTPUT_CAP),
                    )
                })
                .collect();
            calls.extend(round_calls);
            if units_have_calls(&units) {
                prior.push(RoundSummary {
                    text: generation.text.clone(),
                    calls: summaries,
                });
            }
            if !units_have_calls(&units) || rounds >= max_rounds {
                break;
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
        session: &str,
        status: TurnStatus,
        text: String,
        rounds: u32,
        usage: Option<(u64, u64)>,
        calls: Vec<CallRecord>,
        nudge_hint: Option<String>,
        published: &PublishedGeneration,
    ) -> Result<TurnReport, RuntimeError> {
        if self.generation_id() != published.id {
            let _ = self
                .db
                .finish_turn(&turn_id, TurnStatus::Interrupted.as_str(), None);
            return Err(RuntimeError::StaleGeneration {
                want: published.id,
                got: self.generation_id(),
            });
        }
        if status == TurnStatus::Completed && !text.is_empty() {
            self.db.append_message(session, "assistant", &text)?;
        }
        turn_log
            .save(self.db, status.as_str())
            .map_err(|_| RuntimeError::Storage)?;
        Ok(TurnReport {
            turn_id,
            status,
            text,
            rounds,
            usage,
            calls,
            nudge_hint,
        })
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
        attached: &[AttachedMcp],
        cancel: &AtomicBool,
        round: u32,
    ) -> Result<Vec<CallRecord>, RuntimeError> {
        let mut records = Vec::new();
        let mut builtin_units = Vec::new();
        // Partition first: builtins run through the executor, MCP direct.
        for unit in units {
            if matches!(unit, Assembled::Call(call) if is_builtin(&call.name)) {
                builtin_units.push(unit.clone());
            }
        }
        let builtin_outputs: Vec<FunctionCallOutput> = if builtin_units.is_empty() {
            Vec::new()
        } else {
            // Legacy patch deny wins before the executor runs.
            let mut guarded = Vec::with_capacity(builtin_units.len());
            for unit in builtin_units {
                guarded.push(self.guard_patch(unit));
            }
            execute_batch(ctx, guarded).await
        };
        let mut builtin_cursor = 0;
        for (i, unit) in units.iter().enumerate() {
            let op = format!("{turn_id}-r{round}-c{i}");
            match unit {
                Assembled::Call(call) if is_builtin(&call.name) => {
                    let output = builtin_outputs
                        .get(builtin_cursor)
                        .map(|out| out.output.clone())
                        .unwrap_or_else(|| "error: missing output".to_string());
                    builtin_cursor += 1;
                    let state = output_state(&output).to_string();
                    self.record_call(
                        &op,
                        session,
                        Some(turn_id),
                        &call.name,
                        &call.arguments,
                        &output,
                        &state,
                    )?;
                    records.push(CallRecord {
                        name: call.name.clone(),
                        state,
                        output: truncate(&output, REPORT_OUTPUT_CAP),
                    });
                }
                Assembled::Call(call) => {
                    let record = self
                        .execute_mcp(&op, session, Some(turn_id), call, policy, attached, cancel)
                        .await;
                    records.push(record);
                }
                Assembled::Failed(failure) => {
                    let output = format!(
                        "error: assembly failed for {}: {}",
                        failure.id, failure.error
                    );
                    self.db
                        .record_tool_intent(&op, session, Some(turn_id), "unknown", "{}")
                        .map_err(|_| RuntimeError::Storage)?;
                    self.db
                        .record_tool_outcome(&op, "failed", Some(&output))
                        .map_err(|_| RuntimeError::Storage)?;
                    records.push(CallRecord {
                        name: "unknown".to_string(),
                        state: "failed".to_string(),
                        output: truncate(&output, REPORT_OUTPUT_CAP),
                    });
                }
            }
        }
        Ok(records)
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
            .get("patch")
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

    /// Execute one MCP call under the same policy object.
    #[allow(clippy::too_many_arguments)]
    async fn execute_mcp(
        &self,
        op: &str,
        session: &str,
        turn: Option<&str>,
        call: &crate::tools::ToolCall,
        policy: &RuntimePolicy<'_>,
        attached: &[AttachedMcp],
        cancel: &AtomicBool,
    ) -> CallRecord {
        let (server_id, tool) = match call.name.split_once("__") {
            Some((server, tool)) => (server, tool),
            None => {
                let output = format!("error: unknown tool {}", call.name);
                let _ = self
                    .db
                    .record_tool_intent(op, session, turn, &call.name, "{}");
                let _ = self.db.record_tool_outcome(op, "failed", Some(&output));
                return CallRecord {
                    name: call.name.clone(),
                    state: "failed".to_string(),
                    output,
                };
            }
        };
        let server = attached.iter().find(|server| server.server_id == server_id);
        let Some(server) = server else {
            let output = format!("error: unknown mcp server {server_id}");
            let _ = self
                .db
                .record_tool_intent(op, session, turn, &call.name, "{}");
            let _ = self.db.record_tool_outcome(op, "failed", Some(&output));
            return CallRecord {
                name: call.name.clone(),
                state: "failed".to_string(),
                output,
            };
        };
        let input = call.arguments.to_string();
        let _ = self
            .db
            .record_tool_intent(op, session, turn, &call.name, &input);
        if policy.check(&call.name).is_err() {
            let output = format!("error: denied {}", call.name);
            let _ = self.db.record_tool_outcome(op, "denied", Some(&output));
            return CallRecord {
                name: call.name.clone(),
                state: "denied".to_string(),
                output,
            };
        }
        let result = match &server.client {
            AttachedServer::Remote(client) => client
                .call_tool(tool, call.arguments.clone(), cancel)
                .await
                .map_err(|e| e.to_string()),
            AttachedServer::Stdio(child) => child
                .call_tool(tool, call.arguments.clone(), cancel)
                .await
                .map_err(|e| e.to_string()),
        };
        let (state, output) = match result {
            Ok(text) => ("completed", text),
            Err(_) if cancel.load(Ordering::Relaxed) => {
                ("cancelled", "error: cancelled".to_string())
            }
            Err(_) => ("failed", "error: mcp call failed".to_string()),
        };
        let _ = self.db.record_tool_outcome(op, state, Some(&output));
        CallRecord {
            name: call.name.clone(),
            state: state.to_string(),
            output: truncate(&output, REPORT_OUTPUT_CAP),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record_call(
        &self,
        op: &str,
        session: &str,
        turn: Option<&str>,
        name: &str,
        arguments: &serde_json::Value,
        output: &str,
        state: &str,
    ) -> Result<(), RuntimeError> {
        self.db
            .record_tool_intent(op, session, turn, name, &arguments.to_string())
            .map_err(|_| RuntimeError::Storage)?;
        self.db
            .record_tool_outcome(op, state, Some(output))
            .map_err(|_| RuntimeError::Storage)?;
        Ok(())
    }

    /// Attach enabled MCP servers for the current generation (fail fast).
    async fn attach_mcp_current(&self) -> Result<McpAttachment, RuntimeError> {
        let published = self.current.read().expect("generation lock").clone();
        self.attach_mcp(&published)
            .await
            .map(|servers| McpAttachment { servers })
    }

    /// Attached servers for one turn; closing reaps stdio children.
    async fn attach_mcp(
        &self,
        published: &PublishedGeneration,
    ) -> Result<Vec<AttachedMcp>, RuntimeError> {
        let mut attached = Vec::new();
        let mut ids: Vec<&String> = published.config.mcp.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &published.config.mcp[id];
            if !entry.enabled {
                continue;
            }
            if entry.kind == "remote" {
                let config = mcp_remote::CodexWebConfig::from_entry(entry)
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                // Attach uses a bounded dial timeout, not the turn cancel flag.
                let dial_cancel = AtomicBool::new(false);
                let client = CodexWebClient::connect(&config)
                    .await
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                let tools = client
                    .list_tools(&dial_cancel)
                    .await
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                attached.push(AttachedMcp {
                    server_id: id.clone(),
                    client: AttachedServer::Remote(client),
                    entries: registry_entries(id, &tools),
                });
            } else if entry.kind == "local" {
                let config = StdioConfig::from_entry(id, entry)
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                let client = StdioClient::launch(&config)
                    .await
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                let dial_cancel = AtomicBool::new(false);
                let tools = client
                    .list_tools(&dial_cancel)
                    .await
                    .map_err(|_| RuntimeError::McpAttach { server: id.clone() })?;
                attached.push(AttachedMcp {
                    server_id: id.clone(),
                    client: AttachedServer::Stdio(client),
                    entries: registry_entries(id, &tools),
                });
            } else {
                return Err(RuntimeError::McpAttach { server: id.clone() });
            }
        }
        Ok(attached)
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

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    format!("{}…[+{}]", &text[..max], text.len() - max)
}

fn registry_entries(server_id: &str, tools: &[RemoteTool]) -> Vec<RegistryEntry> {
    tools
        .iter()
        .map(|tool| RegistryEntry {
            namespaced: format!("{server_id}__{}", tool.name),
            tool: tool.name.clone(),
            schema: tool.input_schema.clone(),
        })
        .collect()
}

fn history_text(history: &[(String, String)]) -> String {
    history
        .iter()
        .map(|(role, text)| format!("{role}: {text}"))
        .collect::<Vec<_>>()
        .join("\n")
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

fn millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
