//! Native tool executor for T13 (PROV03/04/05, TOOL10/11).
//!
//! Complete tool batch over assembled [`StreamItem`] calls: argument
//! accumulation per item id, unique-ID validation, sequential execution in
//! first-appearance order through [`ToolContext`], and `function_call_output`
//! items keyed to the original call IDs for the next response. Unknown
//! tools, invalid JSON and replayed terminals never execute; duplicates
//! refuse the whole batch before any side effect.
//!
//! Registry (and only registry): `read`, `glob`, `grep`, `apply_patch`, `edit`, `write`, `shell`,
//! `webfetch`, `skill`, `compress`, plus hidden `bash(argv)` compatibility and
//! the per-lane `subagent` tool.
//! Model-selected `write`/`edit` share patch's mutation owner. Reasoning/opaque
//! provider items accumulate in [`TurnLog`] (durable JSON,
//! same-model/provider replay boundary) and are stripped from the UI
//! projection.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use thiserror::Error;

use crate::config::parse_skill;
use crate::files::Files;
use crate::patch::{ApplyFailure, FileResult, PatchError, WritePolicy};
use crate::provider::StreamItem;
use crate::shell::{Shell, ShellLimits};
use crate::storage::Db;

pub mod read;
pub(crate) mod shell_call;

/// Executable built-ins; the model-selected request view narrows file tools.
pub const MODEL_TOOL_NAMES: &[&str] = &[
    "read",
    "glob",
    "grep",
    "apply_patch",
    "edit",
    "write",
    "shell",
    "webfetch",
    "skill",
    "question",
    "compress",
    "opencode_models",
    "opencode_session_rename",
    "opencode_session_move",
];
/// Subagent tool name, advertised only when the runtime published a
/// subagent catalog for the lane; deliberately outside [`MODEL_TOOL_NAMES`].
pub const SUBAGENT_TOOL: &str = "subagent";
/// Upstream `SubagentCompletion.NO_TEXT` fallback for an empty child result.
pub const SUBAGENT_NO_TEXT: &str = "Subagent completed without a text response.";
/// Skill body snapshot cap (bytes).
///
/// Upstream opencode has no skill size limit; the previous 16 KiB cap could
/// truncate real SKILL.md files. Kept as a generous snapshot bound (audited
/// contract: oversized bodies are skipped with a visible warning).
pub const SKILL_BODY_CAP: usize = 1024 * 1024;
/// Native positive execution-timeout resource ceiling for both shell forms (ms).
pub const BASH_TIMEOUT_CAP_MS: u64 = 600_000;
/// Highest accepted zero-based search cursor.
const SEARCH_OFFSET_CAP: u64 = 1_000_000;
/// Highest accepted glob/grep page size (the `Files` API cap).
const SEARCH_LIMIT_CAP: u64 = 1_000;

/// Typed tool errors (no argument contents, no secrets).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// Known capability not implemented; refused before execution intent.
    #[error("tool {tool} unsupported: {feature}")]
    Unsupported { tool: String, feature: String },
    /// Tool name is not in the registry.
    #[error("unknown tool {tool}")]
    UnknownTool {
        /// Given name.
        tool: String,
    },
    /// Arguments malformed for the named tool.
    #[error("invalid arguments for {tool}: {reason}")]
    InvalidArgs {
        /// Tool name.
        tool: String,
        /// Human reason.
        reason: String,
    },
    /// Central policy denial.
    #[error("denied {tool}")]
    Denied {
        /// Tool name.
        tool: String,
    },
    /// Ask is not an allow; this runtime has no interactive approval channel.
    #[error("denied {tool}: approval required (no approval channel)")]
    ApprovalRequired {
        /// Tool name.
        tool: String,
    },
    /// Tool ran and failed visibly.
    #[error("tool {tool} failed: {reason}")]
    Failed {
        /// Tool name.
        tool: String,
        /// Human reason.
        reason: String,
    },
    /// Explicit cancellation before/while running.
    #[error("cancelled")]
    Cancelled,
}

/// Batch-level assembly failure: nothing executes.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BatchError {
    /// Duplicate call id across `output_item.added` announcements.
    #[error("duplicate call id {id}")]
    DuplicateId {
        /// Repeated id.
        id: String,
    },
}

/// Central permission hook (the product policy plugs in here).
pub trait ToolPolicy: Sync {
    fn approved_shell_cwd(&self) -> Option<std::sync::Arc<crate::shell::PinnedCwd>> {
        None
    }
    /// Executor-only preimage ceiling attached to an exact invocation permit.
    fn approved_patch_preimage(&self) -> Option<&str> {
        None
    }
    /// Authorize a tool invocation or deny it.
    fn check(&self, tool: &str) -> Result<(), ToolError>;
    /// Authorize the actual tool resource, rather than just its action name.
    fn check_resource(&self, tool: &str, _resource: &str) -> Result<(), ToolError> {
        self.check(tool)
    }
    /// Search is independently authorized, bounded by explicit read Deny rules.
    fn search_path_denied(&self, resource: &str) -> bool {
        matches!(
            self.check_resource("read", resource),
            Err(ToolError::Denied { .. })
        )
    }
    /// Every resource must pass before any invocation side effect.
    fn check_call(&self, call: &ToolCall) -> Result<(), ToolError> {
        for resource in permission_resources(call)? {
            self.check_resource(&call.name, &resource)?;
        }
        Ok(())
    }
}

/// Canonical command text and the preserved legacy argv resource spelling.
pub(crate) fn permission_resources(call: &ToolCall) -> Result<Vec<String>, ToolError> {
    let invalid = || ToolError::InvalidArgs {
        tool: call.name.clone(),
        reason: "missing permission resource".into(),
    };
    let string = |key| {
        call.arguments
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ToolError::InvalidArgs {
                tool: call.name.clone(),
                reason: format!("missing {key}"),
            })
    };
    Ok(match call.name.as_str() {
        "read" | "edit" | "write" => vec![string("path")?],
        "glob" | "grep" => vec![string("pattern")?],
        "webfetch" => vec![string("url")?],
        "skill" => vec![string("id")?],
        "subagent" => vec![string("agent")?],
        "opencode_session_rename" => vec![string("sessionID")?],
        "opencode_session_move" => vec![string("sessionID")?, string("directory")?],
        // Internal boundary admission only; never an executable/catalog tool.
        "external_directory" => vec![string("directory")?],
        "shell" => vec![string("command")?],
        "apply_patch" => {
            crate::patch::affected_paths(&string("patchText")?).map_err(|_| invalid())?
        }
        "bash" => {
            let argv = call
                .arguments
                .get("argv")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(invalid)?;
            if argv.is_empty() {
                return Err(invalid());
            }
            let args = argv
                .iter()
                .map(|arg| arg.as_str().ok_or_else(invalid))
                .collect::<Result<Vec<_>, _>>()?;
            // Preserve argument boundaries. A shell -c payload is quoted as one
            // argument, not mistaken for an independently allowed bare command.
            vec![
                args.iter()
                    .map(|arg| {
                        if !arg.is_empty()
                            && arg
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || "_./-=:,@%+".contains(c))
                        {
                            (*arg).to_string()
                        } else {
                            format!("'{}'", arg.replace('\'', "'\\''"))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            ]
        }
        // MCP uses the namespaced action with literal wildcard resource.
        _ => vec!["*".into()],
    })
}

/// Allow-everything policy (tests).
#[derive(Debug, Clone, Copy)]
pub struct AllowAllPolicy;

impl ToolPolicy for AllowAllPolicy {
    fn check(&self, _tool: &str) -> Result<(), ToolError> {
        Ok(())
    }
}

/// Deny-list policy (tests + narrow profiles).
#[derive(Debug, Clone)]
pub struct DenyListPolicy {
    /// Denied tool names.
    pub denied: Vec<String>,
}

impl ToolPolicy for DenyListPolicy {
    fn check(&self, tool: &str) -> Result<(), ToolError> {
        if self
            .denied
            .iter()
            .any(|d| crate::config::legacy_key(d) == crate::config::legacy_key(tool))
        {
            return Err(ToolError::Denied {
                tool: tool.to_string(),
            });
        }
        Ok(())
    }
}

/// Assembled tool call with validated JSON arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// Provider function call_id (not the streamed item id).
    pub id: String,
    /// Registry tool name.
    pub name: String,
    /// Parsed argument object.
    pub arguments: serde_json::Value,
}

/// Assembled-but-unrunnable call (invalid JSON, unknown item, stale skill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFailure {
    /// Provider item id (or `unknown` when unannounced).
    pub id: String,
    /// Human reason.
    pub error: String,
}

/// Ordered assembly unit: calls and failures in first-appearance order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assembled {
    /// Runnable call.
    Call(ToolCall),
    /// Visible failure (still produces an output for its id).
    Failed(CallFailure),
}

/// Assemble argument deltas into ordered units.
///
/// Duplicate announced ids refuse the whole batch before any execution.
/// Deltas for unannounced ids and unparseable JSON become [`Assembled::Failed`].
pub fn assemble_calls(items: &[StreamItem]) -> Result<Vec<Assembled>, BatchError> {
    let mut names: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut args: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for item in items {
        match item {
            StreamItem::ToolCallStarted {
                item_id,
                call_id,
                name,
            } => {
                if names.contains_key(item_id) || names.values().any(|(_, id)| id == call_id) {
                    return Err(BatchError::DuplicateId {
                        id: item_id.clone(),
                    });
                }
                order.push(item_id.clone());
                names.insert(item_id.clone(), (name.clone(), call_id.clone()));
            }
            StreamItem::ArgDelta { item_id, delta } => {
                if !names.contains_key(item_id) {
                    if !args.contains_key(item_id) {
                        order.push(item_id.clone());
                    }
                    args.entry(item_id.clone()).or_default().push_str(delta);
                    continue;
                }
                args.entry(item_id.clone()).or_default().push_str(delta);
            }
            _ => {}
        }
    }
    let mut units = Vec::new();
    for id in order {
        match names.remove(&id) {
            Some((name, call_id)) => {
                let raw = args.remove(&id).unwrap_or_default();
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(arguments) if arguments.is_object() => {
                        units.push(Assembled::Call(ToolCall {
                            id: call_id,
                            name,
                            arguments,
                        }));
                    }
                    _ => units.push(Assembled::Failed(CallFailure {
                        id: call_id,
                        error: "invalid JSON arguments".to_string(),
                    })),
                }
            }
            None => units.push(Assembled::Failed(CallFailure {
                id,
                error: "arguments for unannounced item".to_string(),
            })),
        }
    }
    Ok(units)
}

/// Durable tool-call output keyed to the original call id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCallOutput {
    /// Original provider function call_id.
    pub call_id: String,
    /// Bounded result text (or visible error).
    pub output: String,
}

/// Render outputs as next-response `function_call_output` input items.
pub fn to_input_items(outputs: &[FunctionCallOutput]) -> serde_json::Value {
    serde_json::Value::Array(
        outputs
            .iter()
            .map(|o| {
                serde_json::json!({
                    "type": "function_call_output",
                    "call_id": o.call_id,
                    "output": o.output,
                })
            })
            .collect(),
    )
}

/// One validated foreground `subagent` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentRequest {
    /// Agent id to run in the child session.
    pub agent: String,
    /// Short child title (3-5 words upstream).
    pub description: String,
    /// Child task text; the runner prefixes it for a fresh child.
    pub prompt: String,
    /// Explicit `provider/model[#variant]` override.
    pub model: Option<String>,
    /// Existing child session to continue.
    pub session_id: Option<String>,
}

/// Terminal outcome of one foreground child turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubagentOutcome {
    /// Child completed; `text` is never empty (upstream `NO_TEXT` fallback).
    Completed {
        /// Child session id.
        session_id: String,
        /// Final child text.
        text: String,
    },
    /// Child failed or the request was rejected before any child turn.
    Failed {
        /// Child session id when a turn ran.
        session_id: Option<String>,
        /// Upstream-shaped reason.
        reason: String,
    },
    /// The parent cancellation flag stopped the child in flight.
    Cancelled {
        /// Child session id.
        session_id: String,
    },
}

/// Foreground child-turn runner (runtime-owned, invoked by the tool).
///
/// The boxed future keeps the trait object-safe and breaks the async
/// recursion between the turn loop and the nested turn at the type level.
pub trait SubagentRunner: Sync {
    /// Resolve structural eligibility without creating a child, turn or intent.
    fn preflight(&self, request: &SubagentRequest) -> Result<(), ToolError>;
    /// Run one child to a terminal report. Cancellation is the caller's flag.
    fn spawn<'x>(
        &'x self,
        request: SubagentRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SubagentOutcome, ToolError>> + Send + 'x>>;
}

/// Pinned skill catalog: id → bounded entry (generation-time snapshot).
#[derive(Debug, Clone, Default)]
pub struct SkillSnapshot {
    /// Pinned entries.
    pub entries: BTreeMap<String, SkillEntry>,
    /// Invalid discovered ids with precise pinned diagnostics.
    pub errors: BTreeMap<String, String>,
}

/// One pinned skill entry (body snapshot included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    /// Bounded display name.
    pub name: String,
    /// Bounded description.
    pub description: String,
    /// Bounded body snapshot.
    pub body: String,
}

/// Model-facing skill view: id/name/description only, never bodies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillView {
    /// Skill id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
}

impl SkillSnapshot {
    /// Pin `(id, SKILL.md text)` pairs; oversized/invalid files are skipped
    /// with warnings so later calls for them fail visibly as unknown.
    pub fn build(files: &[(String, String)]) -> (Self, Vec<String>) {
        let mut entries = BTreeMap::new();
        let mut warnings = Vec::new();
        for (id, text) in files {
            if text.len() > SKILL_BODY_CAP {
                warnings.push(format!("skill {id}: oversized body"));
                continue;
            }
            match parse_skill(id, text) {
                Ok(meta) => {
                    entries.insert(
                        id.clone(),
                        SkillEntry {
                            name: meta.name,
                            description: meta.description,
                            body: text.clone(),
                        },
                    );
                }
                Err(e) => warnings.push(format!("skill {id}: {e}")),
            }
        }
        (
            Self {
                entries,
                errors: BTreeMap::new(),
            },
            warnings,
        )
    }

    /// Model projection: bounded id/name/description without bodies.
    ///
    /// Skills without a description remain callable by id but stay out of
    /// the auto-invoke guidance (upstream parity).
    pub fn projection(&self) -> Vec<SkillView> {
        self.entries
            .iter()
            .filter(|(_, entry)| !entry.description.trim().is_empty())
            .map(|(id, entry)| SkillView {
                id: id.clone(),
                name: entry.name.clone(),
                description: entry.description.clone(),
            })
            .collect()
    }
}

/// Execution context: trusted roots, scrubbed env, auth, policy, snapshot.
pub struct ToolContext<'a> {
    /// Trusted file tools.
    pub files: &'a Files,
    /// Shell supervisor.
    pub shell: &'a Shell,
    /// Caller-observed environment (scrubbed by the supervisor).
    pub parent_env: &'a BTreeMap<String, String>,
    /// Patch roots (project + data) for `apply_patch` resolution.
    /// Legacy client bearer setting; native model webfetch never sends auth.
    pub webfetch_auth: Option<String>,
    /// Test-only webfetch loopback exception.
    pub webfetch_allow_private: bool,
    /// Central policy hook.
    pub policy: &'a dyn ToolPolicy,
    /// Foreground child-turn runner; `None` disables `subagent`.
    pub subagent: Option<&'a dyn SubagentRunner>,
    /// Pinned skill snapshot.
    pub snapshot: &'a SkillSnapshot,
    /// Cancellation flag (checked between calls and inside the shell supervisor).
    pub cancel: &'a AtomicBool,
    /// Patch roots (always `Some` in production; `None` only in narrow tests).
    pub roots: Option<ToolRoots>,
}

/// Project/data root pair for patch resolution.
#[derive(Debug, Clone)]
pub struct ToolRoots {
    /// Trusted project root.
    pub project: std::path::PathBuf,
    /// Forbidden own data root.
    pub data: std::path::PathBuf,
}

struct PolicyBridge<'a>(&'a dyn ToolPolicy, &'a str);

impl WritePolicy for PolicyBridge<'_> {
    fn check(&self, path: &str) -> Result<(), PatchError> {
        self.0
            .check_resource(self.1, path)
            .map_err(|_| PatchError::Denied {
                path: path.to_string(),
            })
    }
}

/// Execute an assembled batch sequentially in order.
///
/// Every unit produces exactly one output keyed to its id, including
/// failures. Duplicate-id batches never reach this function.
pub async fn execute_batch(
    ctx: &ToolContext<'_>,
    units: Vec<Assembled>,
) -> Vec<FunctionCallOutput> {
    use std::sync::atomic::Ordering;
    let mut outputs = Vec::with_capacity(units.len());
    for unit in units {
        if ctx.cancel.load(Ordering::Relaxed) {
            let id = match &unit {
                Assembled::Call(call) => call.id.clone(),
                Assembled::Failed(failure) => failure.id.clone(),
            };
            outputs.push(FunctionCallOutput {
                call_id: id,
                output: "error: cancelled".to_string(),
            });
            continue;
        }
        match unit {
            Assembled::Failed(failure) => outputs.push(FunctionCallOutput {
                call_id: failure.id,
                output: format!("error: {}", failure.error),
            }),
            Assembled::Call(call) => {
                let id = call.id.clone();
                let output = execute_call(ctx, &call).await;
                outputs.push(FunctionCallOutput {
                    call_id: id,
                    output,
                });
            }
        }
    }
    outputs
}

async fn execute_call(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    if let Err(e) = ctx.policy.check_call(call) {
        return format!("error: {e}");
    }
    match call.name.as_str() {
        "read" => tool_read(ctx, call),
        "glob" | "grep" => execute_search(ctx.files, ctx.policy, ctx.cancel, call).1,
        "apply_patch" => tool_patch(ctx, call),
        "edit" | "write" => tool_mutation_typed(ctx, call).0,
        "shell" | "bash" => execute_shell_typed(ctx, call).await.1,
        "webfetch" => tool_webfetch(ctx, call).await,
        "skill" => tool_skill(ctx, call),
        "subagent" => tool_subagent(ctx, call).await,
        other => format!("error: unknown tool {other}"),
    }
}

/// Required argument shape before the runtime commits an execution intent.
/// Filesystem, URL and command-policy checks remain with their owning tools.
pub(crate) fn validate_call(call: &ToolCall) -> Result<(), String> {
    let args = &call.arguments;
    let nonempty = |key| {
        args.get(key)
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty())
    };
    let valid = match call.name.as_str() {
        "read" => return read::parse(call).map(|_| ()),
        "opencode_models" => return crate::models::lookup::parse(args).map(|_| ()),
        "opencode_session_rename" => return rename_input(args).map(|_| ()),
        "opencode_session_move" => return move_input(args).map(|_| ()),
        "glob" => {
            return parse_glob_args(call)
                .map(|_| ())
                .map_err(|reason| format!("invalid arguments for glob: {reason}"));
        }
        "grep" => {
            return parse_grep_args(call)
                .map(|_| ())
                .map_err(|reason| format!("invalid arguments for grep: {reason}"));
        }
        "apply_patch" => args.as_object().is_some_and(|a| a.len() == 1) && nonempty("patchText"),
        "edit" | "write" => {
            return crate::patch::mutation::Input::parse(&call.name, args).map(|_| ());
        }
        "shell" | "bash" => {
            return shell_call::invocation(call, &BTreeMap::new())
                .map(|_| ())
                .map_err(|error| error.to_string());
        }
        "webfetch" => {
            return crate::webfetch::invocation(args).map(|_| ());
        }
        "skill" => nonempty("id"),
        "question" => {
            return oc_core::question::QuestionInput::parse(args)
                .map(|_| ())
                .map_err(str::to_string);
        }
        "compress" => crate::dcp::validate_range_args(args).is_ok(),
        "subagent" => validate_subagent_args(args).is_ok(),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("invalid arguments for {}", call.name))
    }
}

pub(crate) fn rename_input(args: &serde_json::Value) -> Result<(&str, Option<&str>), String> {
    let o = args.as_object().ok_or("invalid session rename arguments")?;
    if o.keys()
        .any(|k| !matches!(k.as_str(), "title" | "sessionID"))
    {
        return Err("invalid session rename arguments".into());
    }
    let title = o
        .get("title")
        .and_then(serde_json::Value::as_str)
        .and_then(oc_core::core_app::normalized_session_title)
        .ok_or("invalid session title")?;
    let target = o
        .get("sessionID")
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                .ok_or("invalid session target")
        })
        .transpose()?;
    Ok((title, target))
}

pub(crate) fn move_input(args: &serde_json::Value) -> Result<(&str, Option<&str>), String> {
    let o = args.as_object().ok_or("invalid session move arguments")?;
    if o.keys()
        .any(|k| !matches!(k.as_str(), "directory" | "sessionID"))
    {
        return Err("invalid session move arguments".into());
    }
    let directory = o
        .get("directory")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
        .ok_or("invalid move directory")?;
    let target = o
        .get("sessionID")
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                .ok_or("invalid session target")
        })
        .transpose()?;
    Ok((directory, target))
}

/// Required `subagent` shape. Agent/model resolution stays with the runner.
fn validate_subagent_args(args: &serde_json::Value) -> Result<(), String> {
    let object = args
        .as_object()
        .ok_or_else(|| "expected an object".to_string())?;
    if object.keys().any(|key| {
        !matches!(
            key.as_str(),
            "agent" | "description" | "prompt" | "model" | "sessionID" | "background"
        )
    }) {
        return Err("unexpected property".to_string());
    }
    for key in ["agent", "prompt"] {
        if object
            .get(key)
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.is_empty())
        {
            return Err(format!("missing {key}"));
        }
    }
    for key in ["description", "model", "sessionID"] {
        if object
            .get(key)
            .is_some_and(|value| value.as_str().is_none())
        {
            return Err(format!("{key} must be a string"));
        }
    }
    if object
        .get("background")
        .is_some_and(|value| value.as_bool().is_none())
    {
        return Err("background must be a boolean".to_string());
    }
    Ok(())
}

fn parse_glob_args(call: &ToolCall) -> Result<crate::files::search::GlobOptions<'_>, String> {
    let args = call
        .arguments
        .as_object()
        .ok_or_else(|| "expected an object".to_string())?;
    if args.keys().any(|key| {
        !matches!(
            key.as_str(),
            "pattern" | "path" | "hidden" | "offset" | "limit"
        )
    }) {
        return Err("unexpected property".to_string());
    }
    let pattern = args
        .get("pattern")
        .and_then(|value| value.as_str())
        .filter(|pattern| {
            !pattern.is_empty()
                && pattern.len() <= crate::files::SEARCH_PATTERN_BYTES_CAP
                && pattern.split('/').count() <= crate::files::GLOB_SEGMENTS_CAP
        })
        .ok_or_else(|| "missing pattern".to_string())?;
    let (offset, limit) = parse_search_page(args)?;
    let options = crate::files::search::GlobOptions {
        pattern,
        path: search_string(args, "path")?.unwrap_or("."),
        hidden: search_bool(args, "hidden", false)?,
        offset,
        limit,
    };
    options.validate().map_err(|error| error.to_string())?;
    Ok(options)
}

pub(crate) fn parse_grep_args(
    call: &ToolCall,
) -> Result<crate::files::search::GrepOptions<'_>, String> {
    parse_grep_args_with_offset_cap(call, SEARCH_OFFSET_CAP)
}
pub(crate) fn parse_grep_args_with_offset_cap(
    call: &ToolCall,
    offset_cap: u64,
) -> Result<crate::files::search::GrepOptions<'_>, String> {
    let args = call
        .arguments
        .as_object()
        .ok_or_else(|| "expected an object".to_string())?;
    if args.keys().any(|key| {
        !matches!(
            key.as_str(),
            "pattern" | "path" | "include" | "caseSensitive" | "literal" | "offset" | "limit"
        )
    }) {
        return Err("unexpected property".to_string());
    }
    let pattern = args
        .get("pattern")
        .and_then(|value| value.as_str())
        .filter(|pattern| {
            !pattern.is_empty() && pattern.len() <= crate::files::SEARCH_PATTERN_BYTES_CAP
        })
        .ok_or_else(|| "missing pattern".to_string())?;
    let (offset, limit) = parse_search_page_with_offset_cap(args, offset_cap)?;
    let options = crate::files::search::GrepOptions {
        pattern,
        path: search_string(args, "path")?.unwrap_or("."),
        include: search_string(args, "include")?,
        literal: search_bool(args, "literal", false)?,
        case_sensitive: search_bool(args, "caseSensitive", true)?,
        offset,
        limit,
    };
    options.validate().map_err(|error| error.to_string())?;
    Ok(options)
}

fn search_string<'a>(
    args: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<&'a str>, String> {
    args.get(key)
        .map(|value| {
            value
                .as_str()
                .filter(|text| {
                    !text.is_empty()
                        && !text.contains('\0')
                        && text.len() <= crate::files::SEARCH_PATTERN_BYTES_CAP
                })
                .ok_or_else(|| format!("{key} must be a nonempty bounded string"))
        })
        .transpose()
}

fn search_bool(
    args: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    default: bool,
) -> Result<bool, String> {
    args.get(key).map_or(Ok(default), |value| {
        value
            .as_bool()
            .ok_or_else(|| format!("{key} must be a boolean"))
    })
}

pub(crate) fn search_preflight(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
) -> Result<std::path::PathBuf, String> {
    search_preflight_for(ctx.files, ctx.policy, call)
}

fn search_preflight_for(
    files: &Files,
    policy: &dyn ToolPolicy,
    call: &ToolCall,
) -> Result<std::path::PathBuf, String> {
    let path = if call.name == "glob" {
        parse_glob_args(call)?.path
    } else {
        parse_grep_args(call)?.path
    };
    let scope = files
        .search_scope(path)
        .map_err(|error| error.to_string())?;
    if !search_path_allowed(policy, &scope) {
        return Err(format!("denied {}: read scope", call.name));
    }
    Ok(scope)
}

fn parse_search_page(
    args: &serde_json::Map<String, serde_json::Value>,
) -> Result<(usize, usize), String> {
    parse_search_page_with_offset_cap(args, SEARCH_OFFSET_CAP)
}
fn parse_search_page_with_offset_cap(
    args: &serde_json::Map<String, serde_json::Value>,
    offset_cap: u64,
) -> Result<(usize, usize), String> {
    let offset = match args.get("offset") {
        Some(value) => value
            .as_u64()
            .filter(|offset| *offset <= offset_cap)
            .ok_or_else(|| format!("offset must be between 0 and {offset_cap}"))?,
        None => 0,
    };
    let limit = match args.get("limit") {
        Some(value) => value
            .as_u64()
            .filter(|limit| (1..=SEARCH_LIMIT_CAP).contains(limit))
            .ok_or_else(|| format!("limit must be between 1 and {SEARCH_LIMIT_CAP}"))?,
        None => crate::files::DEFAULT_PAGE_LIMIT as u64,
    };
    Ok((
        usize::try_from(offset).map_err(|_| "offset is too large".to_string())?,
        usize::try_from(limit).map_err(|_| "limit is too large".to_string())?,
    ))
}

fn tool_read(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    read::execute(ctx.files, ctx.policy, ctx.cancel, call, false).output
}

/// The same typed search path is used by direct batches and the runtime's
/// owned blocking dispatch. Its cancellation token is never inferred from policy.
pub(crate) fn execute_search(
    files: &Files,
    policy: &dyn ToolPolicy,
    cancel: &AtomicBool,
    call: &ToolCall,
) -> (&'static str, String) {
    if let Err(error) = policy.check_call(call) {
        return ("failed", format!("error: {error}"));
    }
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return ("cancelled", "error: cancelled".into());
    }
    let output = if call.name == "glob" {
        tool_glob(files, policy, cancel, call)
    } else {
        tool_grep(files, policy, cancel, call)
    };
    if cancel.load(std::sync::atomic::Ordering::Acquire) || output == "error: cancelled" {
        ("cancelled", "error: cancelled".into())
    } else if output.starts_with("error:") {
        ("failed", output)
    } else {
        ("completed", output)
    }
}

fn tool_glob(
    files: &Files,
    policy: &dyn ToolPolicy,
    cancel: &AtomicBool,
    call: &ToolCall,
) -> String {
    let mut options = match parse_glob_args(call) {
        Ok(args) => args,
        Err(reason) => return format!("error: invalid arguments for glob: {reason}"),
    };
    if let Err(error) = search_preflight_for(files, policy, call) {
        return format!("error: {error}");
    }
    let (offset, limit) = (options.offset, options.limit);
    options.limit = limit + 1;
    match files.glob_search(
        &options,
        |path| search_path_allowed(policy, path),
        Some(cancel),
    ) {
        Ok(mut items) => {
            let truncated = items.len() > limit;
            items.truncate(limit);
            let returned = items.len();
            serde_json::json!({
                "items": items,
                "pagination": pagination(offset, limit, returned, truncated),
            })
            .to_string()
        }
        Err(error) => format!("error: {error}"),
    }
}

fn tool_grep(
    files: &Files,
    policy: &dyn ToolPolicy,
    cancel: &AtomicBool,
    call: &ToolCall,
) -> String {
    let mut options = match parse_grep_args(call) {
        Ok(args) => args,
        Err(reason) => return format!("error: invalid arguments for grep: {reason}"),
    };
    if let Err(error) = search_preflight_for(files, policy, call) {
        return format!("error: {error}");
    }
    let (offset, limit) = (options.offset, options.limit);
    options.limit = limit + 1;
    match files.grep_search(
        &options,
        |path| search_path_allowed(policy, path),
        Some(cancel),
    ) {
        Ok(mut hits) => {
            let truncated = hits.len() > limit;
            hits.truncate(limit);
            let returned = hits.len();
            let truncated_line_previews = hits.iter().filter(|hit| hit.text_truncated).count();
            let matches = hits
                .into_iter()
                .map(|hit| {
                    serde_json::json!({
                        "path": hit.path,
                        "line": hit.line,
                        "text": hit.text,
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "matches": matches,
                "diagnostics": {"truncated_line_previews": truncated_line_previews},
                "pagination": pagination(offset, limit, returned, truncated),
            })
            .to_string()
        }
        Err(error) => format!("error: {error}"),
    }
}

fn search_path_allowed(policy: &dyn ToolPolicy, path: &std::path::Path) -> bool {
    // A search grant cannot undo an explicit read Deny from central/child policy.
    !policy.search_path_denied(&path.to_string_lossy())
}

fn pagination(offset: usize, limit: usize, returned: usize, truncated: bool) -> serde_json::Value {
    serde_json::json!({
        "offset": offset,
        "limit": limit,
        "returned": returned,
        "truncated": truncated,
        "next_offset": truncated.then_some(offset + returned),
    })
}

fn tool_patch(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    tool_patch_impl(ctx, call).0
}

pub(crate) fn tool_patch_typed(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
) -> (String, Option<oc_core::patch::PatchEffects>) {
    if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return ("error: cancelled".into(), None);
    }
    if let Err(e) = ctx.policy.check_call(call) {
        return (format!("error: {e}"), None);
    }
    tool_patch_impl(ctx, call)
}

fn tool_patch_impl(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
) -> (String, Option<oc_core::patch::PatchEffects>) {
    let patch = call
        .arguments
        .as_object()
        .filter(|args| args.len() == 1)
        .and_then(|args| args.get("patchText"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if patch.is_empty() {
        return (
            "error: invalid arguments for apply_patch: expected only nonempty patchText"
                .to_string(),
            None,
        );
    }
    let Some(roots) = ctx.roots.as_ref() else {
        return ("error: tool apply_patch failed: no roots".to_string(), None);
    };
    let bridge = PolicyBridge(ctx.policy, &call.name);
    let (result, effects) = crate::patch::apply_patch_with_approved_preimage(
        &roots.project,
        &roots.data,
        patch,
        &bridge,
        ctx.policy.approved_patch_preimage(),
    );
    (patch_outcome(result), Some(effects))
}

pub(crate) fn tool_mutation_typed(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
) -> (String, Option<oc_core::patch::PatchEffects>) {
    if ctx.cancel.load(std::sync::atomic::Ordering::Acquire) {
        return ("error: cancelled".into(), None);
    }
    if let Err(error) = ctx.policy.check_call(call) {
        return (format!("error: {error}"), None);
    }
    let input = match crate::patch::mutation::Input::parse(&call.name, &call.arguments) {
        Ok(input) => input,
        Err(error) => return (format!("error: {error}"), None),
    };
    let Some(roots) = &ctx.roots else {
        return ("error: mutation roots missing".into(), None);
    };
    let (result, effects) = crate::patch::mutation::execute(
        &roots.project,
        &roots.data,
        &input,
        &PolicyBridge(ctx.policy, &call.name),
        ctx.policy.approved_patch_preimage(),
    );
    (
        match result {
            Ok(output) => output.to_string(),
            Err(error) => format!("error: {error}"),
        },
        Some(effects),
    )
}

pub(crate) fn patch_outcome(result: Result<Vec<FileResult>, ApplyFailure>) -> String {
    match result {
        Ok(files) => files
            .iter()
            .map(patch_file_result)
            .collect::<Vec<_>>()
            .join("\n"),
        Err(failure) => {
            // The runtime classifies failures by this prefix, including when
            // earlier operations committed successfully.
            let mut text = format!(
                "error: partial op {} ({}): {}",
                failure.failed_op, failure.failed_path, failure.error
            );
            for done in &failure.done {
                text.push_str(&format!("\ndone {}", patch_file_result(done)));
            }
            text
        }
    }
}

fn patch_file_result(file: &FileResult) -> String {
    let target = file
        .new_path
        .as_ref()
        .map(|path| format!(" -> {path}"))
        .unwrap_or_default();
    format!(
        "{} {}{} (hash_before={}, hash_after={})",
        file.op,
        file.path,
        target,
        file.hash_before.as_deref().unwrap_or("-"),
        file.hash_after.as_deref().unwrap_or("-"),
    )
}

/// Preserve supervised process semantics independently of formatted output.
pub(crate) async fn execute_shell_typed(
    ctx: &ToolContext<'_>,
    call: &ToolCall,
) -> (&'static str, String) {
    if let Err(error) = ctx.policy.check_call(call) {
        return ("failed", format!("error: {error}"));
    }
    let shell_call::ShellInvocation {
        argv,
        cwd,
        timeout,
        background,
    } = match shell_call::invocation(call, ctx.parent_env) {
        Ok(invocation) => invocation,
        Err(error) => return ("failed", format!("error: {error}")),
    };
    if background {
        return (
            "failed",
            "error: background shell requires application ownership".into(),
        );
    }
    let limits = ShellLimits {
        timeout,
        kill_grace: Duration::from_millis(500),
        retain_cap: crate::shell::RETAIN_CAP_BYTES,
    };
    let shell = ctx.shell.clone();
    let env = ctx.parent_env.clone();
    let cancel = std::sync::Arc::new(AtomicBool::new(
        ctx.cancel.load(std::sync::atomic::Ordering::Acquire),
    ));
    struct CancelOnDrop(std::sync::Arc<AtomicBool>);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Release);
        }
    }
    let _cleanup = CancelOnDrop(cancel.clone());
    let owned_cancel = cancel.clone();
    let approved_cwd = ctx.policy.approved_shell_cwd();
    let mut task = tokio::task::spawn_blocking(move || {
        shell.execute_pinned(
            &env,
            &argv,
            &cwd,
            None,
            limits,
            &owned_cancel,
            approved_cwd.as_deref(),
        )
    });
    let result = loop {
        tokio::select! {
            result = &mut task => break result,
            () = tokio::time::sleep(Duration::from_millis(5)) => {
                if ctx.cancel.load(std::sync::atomic::Ordering::Acquire) { cancel.store(true,std::sync::atomic::Ordering::Release); }
            }
        }
    };
    match result {
        Ok(Ok(outcome)) => {
            let state = if outcome.cancelled {
                "cancelled"
            } else if outcome.timed_out {
                "timed_out"
            } else if outcome.code == Some(0) {
                "completed"
            } else {
                "failed"
            };
            let mut text = String::new();
            match outcome.code {
                Some(code) => text.push_str(&format!("exit {code}\n")),
                None => text.push_str("exit signal\n"),
            }
            text.push_str(&String::from_utf8_lossy(&outcome.stdout));
            if !outcome.stderr.is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&String::from_utf8_lossy(&outcome.stderr));
            }
            if outcome.stdout_truncated || outcome.stderr_truncated {
                text.push_str("\n[truncated]");
            }
            if outcome.timed_out {
                text.push_str("\n[timeout]");
            }
            if outcome.cancelled {
                text.push_str("\n[cancelled]");
            }
            (state, text)
        }
        Ok(Err(e)) => (
            if matches!(e, crate::shell::ShellError::Reap) {
                "unknown"
            } else {
                "failed"
            },
            format!("error: {e}"),
        ),
        Err(_) => ("unknown", "error: shell worker failed".into()),
    }
}

async fn tool_webfetch(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let input = match crate::webfetch::invocation(&call.arguments) {
        Ok(input) => input,
        Err(error) => return format!("error: {error}"),
    };
    let opts = crate::webfetch::FetchOptions {
        timeout: input.timeout,
        connect_timeout: Duration::from_secs(10),
        max_redirects: crate::webfetch::MAX_REDIRECTS,
        body_cap: crate::webfetch::BODY_CAP_BYTES,
        allow_loopback: ctx.webfetch_allow_private,
    };
    let started = std::time::Instant::now();
    match crate::webfetch::fetch_formatted(input.url, opts, input.format, ctx.cancel).await {
        Ok(mut result) => {
            // Only small, individually bounded metadata is serialized. Content
            // is appended directly; JSON escaping cannot amplify a 1MiB body.
            let mut metadata = serde_json::json!({"url":result.url, "final_url":result.final_url,
                "status":result.status, "content_type":result.content_type,
                "format":input.format.as_str(), "timeout_seconds":input.timeout.as_secs_f64(),
                "truncated":result.truncated});
            let header_bytes = metadata.to_string().len() + 2;
            let room = crate::webfetch::OUTPUT_CAP_BYTES.saturating_sub(header_bytes);
            if result.text.len() > room {
                let mut end = room;
                while !result.text.is_char_boundary(end) {
                    end -= 1;
                }
                result.text.truncate(end);
                metadata["truncated"] = serde_json::Value::Bool(true);
            }
            let output = format!("{metadata}\n\n{}", result.text);
            if ctx.cancel.load(std::sync::atomic::Ordering::Acquire) {
                "error: cancelled".into()
            } else if started.elapsed() >= input.timeout {
                "error: deadline exceeded".into()
            } else {
                output
            }
        }
        Err(e) => format!("error: {e}"),
    }
}

pub(crate) fn preflight_skill(snapshot: &SkillSnapshot, id: &str) -> Result<(), String> {
    if snapshot.entries.contains_key(id) {
        Ok(())
    } else {
        Err(snapshot
            .errors
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("unknown skill {id} (not in pinned snapshot)")))
    }
}

fn tool_skill(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let id = call
        .arguments
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if id.is_empty() {
        return "error: invalid arguments for skill: missing id".to_string();
    }
    if let Err(e) = ctx.policy.check_call(call) {
        return format!("error: {e}");
    }
    match ctx.snapshot.entries.get(id) {
        Some(entry) => entry.body.clone(),
        None => match ctx.snapshot.errors.get(id) {
            Some(error) => format!("error: {error}"),
            None => format!("error: unknown skill {id} (not in pinned snapshot)"),
        },
    }
}

/// Foreground `subagent` call: run one child to completion in the parent turn.
///
/// Result shape mirrors upstream: completed children are wrapped as
/// `<subagent sessionID="…" state="completed">`; request failures surface as
/// their upstream message; an in-flight cancellation is `error: cancelled`.
async fn tool_subagent(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let Some(runner) = ctx.subagent else {
        return "error: subagent tool is unavailable in this session".to_string();
    };
    let request = match subagent_request(call) {
        Ok(request) => request,
        Err(error) => return format!("error: {error}"),
    };
    match runner.spawn(request).await {
        Ok(SubagentOutcome::Completed { session_id, text }) => {
            format!(
                "<subagent sessionID=\"{session_id}\" state=\"completed\">\n{text}\n</subagent>"
            )
        }
        Ok(SubagentOutcome::Failed {
            session_id: Some(session_id),
            reason,
        }) => format!("error: subagent failed (sessionID: {session_id}): {reason}"),
        Ok(SubagentOutcome::Failed {
            session_id: None,
            reason,
        }) => format!("error: {reason}"),
        Ok(SubagentOutcome::Cancelled { .. }) => "error: cancelled".to_string(),
        Err(error) => format!("error: {error}"),
    }
}

pub(crate) fn preflight_subagent(ctx: &ToolContext<'_>, call: &ToolCall) -> Result<(), ToolError> {
    let runner = ctx.subagent.ok_or_else(|| ToolError::Failed {
        tool: "subagent".into(),
        reason: "subagent tool is unavailable in this session".into(),
    })?;
    runner.preflight(&subagent_request(call)?)
}

fn subagent_request(call: &ToolCall) -> Result<SubagentRequest, ToolError> {
    if call
        .arguments
        .get("background")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        return Err(ToolError::Failed {
            tool: "subagent".into(),
            reason: "background subagents are not supported yet; no child session was created"
                .into(),
        });
    }
    let string = |key: &str| {
        call.arguments
            .get(key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string()
    };
    Ok(SubagentRequest {
        agent: string("agent"),
        description: string("description"),
        prompt: string("prompt"),
        model: call
            .arguments
            .get("model")
            .and_then(|value| value.as_str())
            .map(str::to_string),
        session_id: call
            .arguments
            .get("sessionID")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    })
}

/// Durable turn log: opaque provider items + usage with a replay boundary.
///
/// Serialized as JSON into the turn row (`turns.result`). Opaque replay stays
/// bound to its producing model/provider; model changes project ordinary groups without
/// leaking foreign opaque state or mutating this journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnLog {
    /// Fixed immutable original prefix; replay reads only the hot journal.
    pub(crate) raw_prefix: Option<turn_history::RawPrefix>,
    /// Selected task/checkpoint facts, never re-appended as original RAW.
    pub(crate) working: Option<serde_json::Value>,
    /// Only retained input origins/occurrences; never a lifetime call-id map.
    pub(crate) input_origins: Vec<Option<usize>>,
    pub(crate) call_occurrences: std::collections::BTreeMap<usize, u64>,
    /// Physical attempts and input provenance within this existing journal.
    pub requests: Vec<oc_core::queries::RequestIdentity>,
    pub(crate) instruction_references: Vec<crate::instructions::Reference>,
    /// Assistant-span facts, absent in legacy journals.
    pub spans: Vec<oc_core::queries::AssistantSpan>,
    /// Safe pinned labels and measured footer metadata.
    pub display: serde_json::Value,
    /// Ordered presentation references and public reasoning absent from wire history.
    pub display_parts: Vec<serde_json::Value>,
    /// Turn id this log belongs to.
    pub turn_id: String,
    /// Model id at generation time.
    pub model: String,
    /// Provider id at generation time.
    pub provider: String,
    /// Verbatim opaque payloads in arrival order.
    pub opaque: Vec<serde_json::Value>,
    /// Terminal usage when reported.
    pub usage: Option<(u64, u64)>,
    /// Accepted raw-history anchor for this turn's wire input.
    pub user_message: Option<String>,
    /// Completed Responses items and durable tool results, never UI text parsing.
    pub input: Vec<crate::provider::InputItem>,
    /// Automatic history notices represented in this turn's captured input.
    pub shell_notice_messages: Vec<String>,
    /// Primary-agent behavior digest pinned for this turn.
    pub agent_digest: Option<String>,
}

mod mcp_log;
mod model_history;
pub(crate) mod output;
pub(crate) mod turn_history;

impl TurnLog {
    /// Start an empty log for a turn.
    pub fn new(turn_id: &str, model: &str, provider: &str) -> Self {
        Self {
            raw_prefix: None,
            working: None,
            input_origins: Vec::new(),
            call_occurrences: std::collections::BTreeMap::new(),
            requests: Vec::new(),
            instruction_references: Vec::new(),
            spans: Vec::new(),
            display: serde_json::json!({}),
            display_parts: Vec::new(),
            turn_id: turn_id.to_string(),
            model: model.to_string(),
            provider: provider.to_string(),
            opaque: Vec::new(),
            usage: None,
            user_message: None,
            input: Vec::new(),
            shell_notice_messages: Vec::new(),
            agent_digest: None,
        }
    }

    /// Ingest one stream item (opaque + usage accumulate; rest ignored).
    pub fn ingest(&mut self, item: &StreamItem) {
        match item {
            StreamItem::OpaqueItem { payload, .. } => self.opaque.push(payload.clone()),
            StreamItem::Usage {
                input_tokens,
                output_tokens,
            } => {
                if self.usage.is_none() {
                    self.usage = Some((*input_tokens, *output_tokens));
                }
            }
            _ => {}
        }
    }

    /// Serialize for the turn row.
    pub fn to_json(&self) -> serde_json::Value {
        let (input, native_mcp, native_read) = self.encode_mcp_input();
        let mut value = serde_json::json!({
            "requests": self.requests,
            "instruction_references": self.instruction_references,
            "display": self.display,
            "spans": self.spans,
            "display_parts": self.display_parts,
            "turn_id": self.turn_id,
            "model": self.model,
            "provider": self.provider,
            "opaque": self.opaque,
            "usage": self.usage.map(|(i, o)| serde_json::json!([i, o])),
            "user_message": self.user_message,
            "input": input,
            "shell_notice_messages": self.shell_notice_messages,
            "agent_digest": self.agent_digest,
        });
        if let Some(prefix) = &self.raw_prefix {
            value["raw_prefix"] = serde_json::to_value(prefix).expect("prefix serialization");
        }
        if let Some(working) = &self.working {
            value["working"] = working.clone();
        }
        if !self.input_origins.is_empty() {
            value["input_origins"] = serde_json::to_value(&self.input_origins).expect("origins");
        }
        if !self.call_occurrences.is_empty() {
            value["call_occurrences"] =
                serde_json::to_value(&self.call_occurrences).expect("occurrences");
        }
        if !native_mcp.is_empty() {
            value["native_mcp_results"] = native_mcp.into();
        }
        if !native_read.is_empty() {
            value["native_read_results"] = native_read.into();
        }
        value
    }

    /// Deserialize from the turn row.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        let raw_prefix = turn_history::RawPrefix::parse(value.get("raw_prefix"))?;
        let working = value.get("working").filter(|w| !w.is_null()).cloned();
        if let Some(working) = &working {
            if working.get("working").is_some() || working.get("raw_prefix").is_some() {
                return Err("nested hot working checkpoint".into());
            }
            let selected = Self::from_json(working)?;
            if value["turn_id"] != selected.turn_id {
                return Err("foreign hot working checkpoint".into());
            }
        }
        let log = Self {
            raw_prefix,
            working,
            input_origins: value
                .get("input_origins")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid input origins")?
                .unwrap_or_default(),
            call_occurrences: value
                .get("call_occurrences")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid call occurrences")?
                .unwrap_or_default(),
            requests: value
                .get("requests")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid request receipts")?
                .unwrap_or_default(),
            instruction_references: value
                .get("instruction_references")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid instruction references")?
                .unwrap_or_default(),
            spans: value
                .get("spans")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid spans")?
                .unwrap_or_default(),
            display: value
                .get("display")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
            display_parts: value
                .get("display_parts")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            user_message: value
                .get("user_message")
                .and_then(|v| v.as_str())
                .map(str::to_owned),
            input: Self::decode_mcp_input(value)?,
            shell_notice_messages: value
                .get("shell_notice_messages")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|_| "invalid shell notice references")?
                .unwrap_or_default(),
            agent_digest: value
                .get("agent_digest")
                .and_then(|value| value.as_str())
                .map(str::to_owned),
            turn_id: value
                .get("turn_id")
                .and_then(|v| v.as_str())
                .ok_or("missing turn_id")?
                .to_string(),
            model: value
                .get("model")
                .and_then(|v| v.as_str())
                .ok_or("missing model")?
                .to_string(),
            provider: value
                .get("provider")
                .and_then(|v| v.as_str())
                .ok_or("missing provider")?
                .to_string(),
            opaque: value
                .get("opaque")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
            usage: value
                .get("usage")
                .and_then(|v| v.as_array())
                .and_then(|a| Some((a.first()?.as_u64()?, a.get(1)?.as_u64()?))),
        };
        log.validate_history_coordinates()?;
        Ok(log)
    }

    /// Replay opaque payloads at the continuation boundary.
    ///
    /// Same `(model, provider)` replays verbatim; any switch drops alien
    /// state with a diagnostic instead of reusing it.
    pub fn replay_for(
        &self,
        model: &str,
        provider: &str,
    ) -> Result<Vec<serde_json::Value>, String> {
        if self.model == model && self.provider == provider {
            Ok(self.opaque.clone())
        } else {
            Err(format!(
                "alien turn state (was {}:{}, now {model}:{provider}); dropped",
                self.model, self.provider
            ))
        }
    }

    /// Persist the log JSON into the turn row.
    pub fn save(&self, db: &Db, status: &str) -> Result<(), String> {
        db.finish_turn(&self.turn_id, status, Some(&self.to_json().to_string()))
            .map_err(|e| format!("storage: {e}"))
    }

    /// UI projection: model text, tool activity and usage pass through;
    /// opaque payloads and reasoning deltas never reach UI/logs.
    pub fn ui_projection(items: &[StreamItem]) -> Vec<StreamItem> {
        items
            .iter()
            .filter(|item| {
                !matches!(
                    item,
                    StreamItem::OpaqueItem { .. } | StreamItem::ReasoningDelta(_)
                )
            })
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    #[path = "file_mutations.rs"]
    mod file_mutations;
    #[path = "shell.rs"]
    mod foreground;
    #[path = "search.rs"]
    mod search;
    #[path = "webfetch.rs"]
    mod webfetch;
    use super::{
        AllowAllPolicy, Assembled, BatchError, DenyListPolicy, MODEL_TOOL_NAMES, SkillSnapshot,
        ToolCall, ToolContext, ToolRoots, TurnLog, assemble_calls, execute_batch, to_input_items,
    };
    use crate::files::Files;
    use crate::provider::{ResponsesConfig, StreamItem, stream_generation};
    use crate::shell::Shell;
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    static NO_CANCEL: AtomicBool = AtomicBool::new(false);

    struct Env {
        _tmp: tempfile::TempDir,
        project: std::path::PathBuf,
        data: std::path::PathBuf,
        files: Files,
        shell: Shell,
        env: BTreeMap<String, String>,
        snapshot: SkillSnapshot,
    }

    fn setup() -> Env {
        let tmp = tempfile::tempdir().expect("temp");
        let project = tmp.path().join("project");
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&data).expect("data");
        std::fs::write(project.join("note.txt"), "file-bytes\n").expect("seed");
        let files = Files::new(&project, &data).expect("files");
        let shell = Shell::new(&project).expect("shell");
        let (snapshot, _) = SkillSnapshot::build(&[(
            "demo".to_string(),
            "---\nname: demo\ndescription: bounded demo\n---\n# body\n".to_string(),
        )]);
        Env {
            _tmp: tmp,
            project,
            data,
            files,
            shell,
            env: BTreeMap::new(),
            snapshot,
        }
    }

    fn ctx<'a>(
        env: &'a Env,
        policy: &'a dyn super::ToolPolicy,
        allow_private: bool,
    ) -> ToolContext<'a> {
        ToolContext {
            files: &env.files,
            shell: &env.shell,
            parent_env: &env.env,
            webfetch_auth: None,
            webfetch_allow_private: allow_private,
            policy,
            subagent: None,
            snapshot: &env.snapshot,
            cancel: &NO_CANCEL,
            roots: Some(ToolRoots {
                project: env.project.clone(),
                data: env.data.clone(),
            }),
        }
    }

    fn started(id: &str, name: &str) -> StreamItem {
        StreamItem::ToolCallStarted {
            item_id: id.to_string(),
            call_id: id.to_string(),
            name: name.to_string(),
        }
    }

    fn args(id: &str, delta: &str) -> StreamItem {
        StreamItem::ArgDelta {
            item_id: id.to_string(),
            delta: delta.to_string(),
        }
    }

    #[test]
    fn tool10_registry_has_working_mutation_families() {
        assert_eq!(
            MODEL_TOOL_NAMES,
            &[
                "read",
                "glob",
                "grep",
                "apply_patch",
                "edit",
                "write",
                "shell",
                "webfetch",
                "skill",
                "question",
                "compress",
                "opencode_models",
                "opencode_session_rename",
                "opencode_session_move"
            ]
        );
        assert!(MODEL_TOOL_NAMES.contains(&"write"));
        assert!(MODEL_TOOL_NAMES.contains(&"edit"));
    }

    #[test]
    fn tool10_duplicate_ids_refuse_batch() {
        let items = vec![
            started("c1", "read"),
            args("c1", "{\"path\":\"note.txt\"}"),
            started("c1", "read"),
            args("c1", "{\"path\":\"note.txt\"}"),
        ];
        let err = assemble_calls(&items).expect_err("duplicate");
        assert_eq!(
            err,
            BatchError::DuplicateId {
                id: "c1".to_string()
            }
        );
    }

    #[tokio::test]
    async fn tool10_unknown_invalid_match_ids() {
        let env = setup();
        let policy = AllowAllPolicy;
        let context = ctx(&env, &policy, false);
        let items = vec![
            started("c1", "read"),
            args("c1", "{\"path\":\"note.txt\"}"),
            started("c2", "teleport"),
            args("c2", "{}"),
            started("c3", "read"),
            args("c3", "not json"),
        ];
        let units = assemble_calls(&items).expect("assemble");
        assert_eq!(units.len(), 3);
        let outputs = execute_batch(&context, units).await;
        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs[0].call_id, "c1");
        assert!(
            outputs[0].output.contains("file-bytes"),
            "got: {}",
            outputs[0].output
        );
        assert_eq!(outputs[1].call_id, "c2");
        assert!(
            outputs[1].output.contains("unknown tool"),
            "got: {}",
            outputs[1].output
        );
        assert_eq!(outputs[2].call_id, "c3");
        assert!(
            outputs[2].output.contains("invalid JSON"),
            "got: {}",
            outputs[2].output
        );
        // Results render as next-response input items keyed to call ids.
        let input = to_input_items(&outputs);
        assert_eq!(input[0]["call_id"], "c1");
        assert_eq!(input[0]["type"], "function_call_output");
    }

    #[tokio::test]
    async fn tool01_glob_grep_execute_structured_sorted_pages() {
        let env = setup();
        for (path, body) in [
            ("search/zeta.rs", "aud_runtime_needle zeta\n"),
            ("search/alpha.rs", "first\naud_runtime_needle alpha\n"),
            ("search/nested/beta.rs", "aud_runtime_needle beta\n"),
        ] {
            let path = env.project.join(path);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
            std::fs::write(path, body).expect("search fixture");
        }
        let policy = AllowAllPolicy;
        let context = ctx(&env, &policy, false);
        let outputs = execute_batch(
            &context,
            vec![
                Assembled::Call(ToolCall {
                    id: "glob-1".to_string(),
                    name: "glob".to_string(),
                    arguments: serde_json::json!({
                        "pattern": "search/**/*.rs",
                        "offset": 0,
                        "limit": 2,
                    }),
                }),
                Assembled::Call(ToolCall {
                    id: "grep-1".to_string(),
                    name: "grep".to_string(),
                    arguments: serde_json::json!({
                        "pattern": "aud_runtime_needle",
                        "literal": true,
                        "offset": 0,
                        "limit": 10,
                    }),
                }),
                Assembled::Call(ToolCall {
                    id: "glob-invalid".to_string(),
                    name: "glob".to_string(),
                    arguments: serde_json::json!({"pattern": "**/*", "extra": true}),
                }),
            ],
        )
        .await;

        let glob: serde_json::Value =
            serde_json::from_str(&outputs[0].output).expect("structured glob output");
        assert_eq!(
            glob["items"],
            serde_json::json!(["search/alpha.rs", "search/nested/beta.rs"])
        );
        assert_eq!(glob["pagination"]["offset"], 0);
        assert_eq!(glob["pagination"]["limit"], 2);
        assert_eq!(glob["pagination"]["returned"], 2);
        assert_eq!(glob["pagination"]["truncated"], true);
        assert_eq!(glob["pagination"]["next_offset"], 2);

        let grep: serde_json::Value =
            serde_json::from_str(&outputs[1].output).expect("structured grep output");
        assert_eq!(grep["matches"].as_array().expect("matches").len(), 3);
        assert_eq!(grep["matches"][0]["path"], "search/alpha.rs");
        assert_eq!(grep["matches"][0]["line"], 2);
        assert_eq!(grep["matches"][1]["path"], "search/nested/beta.rs");
        assert_eq!(grep["matches"][2]["path"], "search/zeta.rs");
        assert_eq!(grep["pagination"]["returned"], 3);
        assert_eq!(grep["pagination"]["truncated"], false);
        assert_eq!(grep["pagination"]["next_offset"], serde_json::Value::Null);
        assert!(
            outputs[2]
                .output
                .starts_with("error: invalid arguments for glob")
        );
    }

    #[test]
    fn tool01_glob_grep_validate_strict_bounded_arguments() {
        let valid = |name: &str, arguments: serde_json::Value| ToolCall {
            id: name.to_string(),
            name: name.to_string(),
            arguments,
        };
        assert!(
            super::validate_call(&valid("glob", serde_json::json!({"pattern": "**/*.rs"}))).is_ok()
        );
        assert!(
            super::validate_call(&valid(
                "grep",
                serde_json::json!({"pattern": "needle", "literal": true})
            ))
            .is_ok()
        );
        for call in [
            valid("glob", serde_json::json!({"pattern": "*", "extra": true})),
            valid("glob", serde_json::json!({"pattern": "*", "offset": -1})),
            valid(
                "glob",
                serde_json::json!({"pattern": "*", "offset": 1_000_001}),
            ),
            valid("glob", serde_json::json!({"pattern": "*", "limit": 0})),
            valid("grep", serde_json::json!({"pattern": "x", "limit": 1_001})),
            valid(
                "grep",
                serde_json::json!({"pattern": "x", "literal": "true"}),
            ),
        ] {
            assert!(super::validate_call(&call).is_err(), "accepted {call:?}");
        }
    }

    #[tokio::test]
    async fn tool01_glob_grep_permission_denial_precedes_dispatch() {
        let env = setup();
        let policy = DenyListPolicy {
            denied: vec!["glob".to_string(), "grep".to_string()],
        };
        let context = ctx(&env, &policy, false);
        let outputs = execute_batch(
            &context,
            vec![
                Assembled::Call(ToolCall {
                    id: "glob-denied".to_string(),
                    name: "glob".to_string(),
                    arguments: serde_json::json!({"pattern": "**/*", "extra": true}),
                }),
                Assembled::Call(ToolCall {
                    id: "grep-denied".to_string(),
                    name: "grep".to_string(),
                    arguments: serde_json::json!({
                        "pattern": "file-bytes",
                        "literal": "not-a-boolean",
                    }),
                }),
            ],
        )
        .await;
        assert_eq!(outputs[0].output, "error: denied glob");
        assert_eq!(outputs[1].output, "error: denied grep");
    }

    #[tokio::test]
    async fn aud03_patch_text_schema_and_tool_route() {
        let definition = crate::runtime::builtin_tool_defs()
            .into_iter()
            .find(|tool| tool.name == "apply_patch")
            .expect("patch definition");
        assert_eq!(
            definition.parameters,
            serde_json::json!({
                "type": "object",
                "properties": {"patchText": {"type": "string"}},
                "required": ["patchText"],
                "additionalProperties": false,
            })
        );
        let env = setup();
        let policy = AllowAllPolicy;
        let context = ctx(&env, &policy, false);
        let patch = "*** Begin Patch\n*** Add File: canonical.txt\n+hello\n*** End Patch\n";
        for alias in ["patch", "text"] {
            let output = execute_batch(
                &context,
                vec![Assembled::Call(ToolCall {
                    id: alias.to_string(),
                    name: "apply_patch".to_string(),
                    arguments: serde_json::json!({(alias): patch}),
                })],
            )
            .await;
            assert!(output[0].output.starts_with("error: invalid arguments"));
            assert!(!env.project.join("canonical.txt").exists());
        }
        let output = execute_batch(
            &context,
            vec![Assembled::Call(ToolCall {
                id: "canonical".to_string(),
                name: "apply_patch".to_string(),
                arguments: serde_json::json!({"patchText": patch}),
            })],
        )
        .await;
        assert!(
            output[0].output.starts_with("add canonical.txt"),
            "{}",
            output[0].output
        );
        assert_eq!(
            std::fs::read(env.project.join("canonical.txt")).expect("created"),
            b"hello\n"
        );
    }

    #[test]
    fn aud05_partial_patch_output_keeps_all_commits_and_full_hashes() {
        use crate::patch::{ApplyFailure, FileResult, PatchError};

        let before = "a".repeat(64);
        let after = "b".repeat(64);
        let done = vec![
            FileResult {
                path: "added.txt".to_string(),
                new_path: None,
                op: "add",
                hash_before: None,
                hash_after: Some(after.clone()),
            },
            FileResult {
                path: "old.txt".to_string(),
                new_path: Some("renamed.txt".to_string()),
                op: "update",
                hash_before: Some(before.clone()),
                hash_after: Some(after.clone()),
            },
            FileResult {
                path: "move-failed.txt".to_string(),
                new_path: None,
                op: "update",
                hash_before: Some(before.clone()),
                hash_after: Some(after.clone()),
            },
        ];
        let output = super::patch_outcome(Err(ApplyFailure {
            done,
            failed_op: 2,
            failed_path: "move-failed.txt".to_string(),
            error: PatchError::Io {
                path: "target.txt".to_string(),
            },
        }));
        assert_eq!(
            output,
            format!(
                "error: partial op 2 (move-failed.txt): io error at target.txt\n\
             done add added.txt (hash_before=-, hash_after={after})\n\
             done update old.txt -> renamed.txt (hash_before={before}, hash_after={after})\n\
             done update move-failed.txt (hash_before={before}, hash_after={after})"
            )
        );
    }

    #[tokio::test]
    async fn tool_patch_bash_skill_paths() {
        let env = setup();
        let policy = AllowAllPolicy;
        let context = ctx(&env, &policy, false);
        let units = vec![
            Assembled::Call(ToolCall {
                id: "p1".to_string(),
                name: "apply_patch".to_string(),
                arguments: serde_json::json!({"patchText": "*** Begin Patch\n*** Add File: made.txt\n+made\n*** End Patch\n"}),
            }),
            Assembled::Call(ToolCall {
                id: "b1".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"argv": ["echo", "hi"]}),
            }),
            Assembled::Call(ToolCall {
                id: "s1".to_string(),
                name: "skill".to_string(),
                arguments: serde_json::json!({"id": "demo"}),
            }),
        ];
        let outputs = execute_batch(&context, units).await;
        assert!(
            outputs[0].output.contains("add made.txt"),
            "got: {}",
            outputs[0].output
        );
        assert!(
            outputs[1].output.contains("exit 0") && outputs[1].output.contains("hi"),
            "got: {}",
            outputs[1].output
        );
        assert!(
            outputs[2].output.contains("# body"),
            "got: {}",
            outputs[2].output
        );
        // Denied tools fail visibly without side effects.
        let deny = DenyListPolicy {
            denied: vec!["bash".to_string()],
        };
        let context = ctx(&env, &deny, false);
        let outputs = execute_batch(
            &context,
            vec![Assembled::Call(ToolCall {
                id: "b9".to_string(),
                name: "bash".to_string(),
                arguments: serde_json::json!({"argv": ["touch", "denied-marker"]}),
            })],
        )
        .await;
        assert!(
            outputs[0].output.contains("denied"),
            "got: {}",
            outputs[0].output
        );
        assert!(!env.project.join("denied-marker").exists());
    }

    #[test]
    fn tool11_projection_and_snapshot_failures() {
        let (snapshot, warnings) = SkillSnapshot::build(&[
            (
                "ok".to_string(),
                "---\nname: ok\ndescription: fine\n---\nbody\n".to_string(),
            ),
            (
                "big".to_string(),
                format!(
                    "---\nname: big\ndescription: d\n---\n{}",
                    "x".repeat(super::SKILL_BODY_CAP + 1)
                ),
            ),
            (
                "bad".to_string(),
                "---\nname: bad\nname: duplicate\n---\nbody\n".to_string(),
            ),
            // Upstream parity: no frontmatter still loads with the path id.
            ("bare".to_string(), "no frontmatter".to_string()),
        ]);
        assert_eq!(warnings.len(), 2);
        let projection = snapshot.projection();
        // The no-description skill loads but stays out of auto-invoke guidance.
        assert_eq!(projection.len(), 1);
        let json = serde_json::to_value(&projection).expect("json");
        assert!(json.to_string().contains("\"ok\""));
        assert!(!json.to_string().contains("\"bare\""));
        assert!(!json.to_string().contains("body"));
    }

    #[tokio::test]
    async fn tool11_stale_and_durable_graph() {
        let env = setup();
        let policy = AllowAllPolicy;
        // Stale: call valid at assembly, snapshot without the skill at run.
        let (empty, _) = SkillSnapshot::build(&[]);
        // NOTE: ctx() borrows env; rebuild manually for the swapped snapshot.
        let context = ToolContext {
            files: &env.files,
            shell: &env.shell,
            parent_env: &env.env,
            webfetch_auth: None,
            webfetch_allow_private: false,
            policy: &policy,
            subagent: None,
            snapshot: &empty,
            cancel: &NO_CANCEL,
            roots: Some(ToolRoots {
                project: env.project.clone(),
                data: env.data.clone(),
            }),
        };
        let outputs = execute_batch(
            &context,
            vec![Assembled::Call(ToolCall {
                id: "s9".to_string(),
                name: "skill".to_string(),
                arguments: serde_json::json!({"id": "demo"}),
            })],
        )
        .await;
        assert!(
            outputs[0].output.contains("unknown skill"),
            "got: {}",
            outputs[0].output
        );
        // Durable graph: outputs persist through own storage and read back.
        let context = ctx(&env, &policy, false);
        let outputs = execute_batch(
            &context,
            vec![Assembled::Call(ToolCall {
                id: "c1".to_string(),
                name: "read".to_string(),
                arguments: serde_json::json!({"path": "note.txt"}),
            })],
        )
        .await;
        let db = crate::storage::Db::open(&env.data).expect("db");
        db.create_session("s-t").expect("session");
        let logged = serde_json::to_string(&to_input_items(&outputs)).expect("json");
        db.append_message("s-t", "tool", &logged).expect("log");
        let history = db.read_history("s-t").expect("history");
        assert_eq!(history.len(), 1);
        assert!(history[0].1.contains("\"call_id\":\"c1\"") || history[0].1.contains("c1"));
    }

    /// Minimal SSE responder keyed off the request body for PROV03.
    struct RoundServer {
        base: String,
        handle: tokio::task::JoinHandle<()>,
    }

    impl RoundServer {
        async fn spawn() -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let base = format!("http://{}", listener.local_addr().expect("addr"));
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
                        let mut head = Vec::new();
                        let mut buf = [0u8; 4096];
                        loop {
                            match sock.read(&mut buf).await {
                                Ok(0) => return,
                                Ok(n) => {
                                    head.extend_from_slice(&buf[..n]);
                                    if head.windows(4).any(|w| w == b"\r\n\r\n") {
                                        break;
                                    }
                                }
                                Err(_) => return,
                            }
                        }
                        let text = String::from_utf8_lossy(&head).into_owned();
                        let mut content_len = 0usize;
                        for line in text.lines().skip(1) {
                            if line.is_empty() {
                                break;
                            }
                            if let Some((k, v)) = line.split_once(':')
                                && k.trim().eq_ignore_ascii_case("content-length")
                            {
                                content_len = v.trim().parse().unwrap_or(0);
                            }
                        }
                        let mut body = vec![0u8; content_len];
                        let mut read = 0;
                        if let Some(pos) = head.windows(4).position(|w| w == b"\r\n\r\n") {
                            let have = &head[pos + 4..];
                            let take = have.len().min(content_len);
                            body[..take].copy_from_slice(&have[..take]);
                            read = take;
                        }
                        while read < content_len {
                            match sock.read(&mut body[read..]).await {
                                Ok(0) => break,
                                Ok(n) => read += n,
                                Err(_) => break,
                            }
                        }
                        let request: serde_json::Value =
                            serde_json::from_slice(&body).expect("request");
                        let continuation = request["input"]
                            .as_array()
                            .expect("input")
                            .iter()
                            .any(|item| item["type"] == "function_call_output");
                        let payload = if continuation {
                            assert_eq!(request["input"][2]["call_id"], "c1");
                            assert_eq!(request["input"][3]["call_id"], "c2");
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":6}}}\n\n"
                        } else {
                            concat!(
                                "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"fc1\",\"call_id\":\"c1\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"\"}}\n\n",
                                "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc1\",\"delta\":\"{\\\"path\\\":\\\"note.txt\\\"}\"}\n\n",
                                "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"fc1\",\"call_id\":\"c1\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"note.txt\\\"}\",\"status\":\"completed\"}}\n\n",
                                "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"fc2\",\"call_id\":\"c2\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"\"}}\n\n",
                                "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc2\",\"delta\":\"{\\\"limit\\\":1}\"}\n\n",
                                "data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"id\":\"fc2\",\"call_id\":\"c2\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"{\\\"limit\\\":1}\",\"status\":\"completed\"}}\n\n",
                                "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":2}}}\n\n",
                            )
                        };
                        let head_out = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            payload.len()
                        );
                        let _ = sock.write_all(head_out.as_bytes()).await;
                        let _ = sock.write_all(payload.as_bytes()).await;
                    });
                }
            });
            Self { base, handle }
        }

        fn shutdown(self) {
            self.handle.abort();
        }
    }

    fn provider_config(base: &str) -> ResponsesConfig {
        ResponsesConfig {
            headers: BTreeMap::new(),
            set_cache_key: true,
            base_url: base.to_string(),
            api_key: "k".to_string(),
            timeout: Some(false),
            chunk_timeout_ms: super::super::provider::CHUNK_TIMEOUT_MS,
            connect_timeout: Duration::from_secs(5),
            allow_private: true,
        }
    }

    #[tokio::test]
    async fn prov03_complete_roundtrip_ordered_ids() {
        let env = setup();
        let policy = AllowAllPolicy;
        let server = RoundServer::spawn().await;
        // Turn 1: model emits two tool calls in order.
        let first = stream_generation(
            &provider_config(&server.base),
            "m",
            None,
            "read the note",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .expect("first");
        let units = assemble_calls(&first.items).expect("assemble");
        assert_eq!(units.len(), 2);
        let context = ctx(&env, &policy, false);
        let outputs = execute_batch(&context, units).await;
        assert_eq!(outputs[0].call_id, "c1");
        assert_eq!(outputs[1].call_id, "c2");
        assert!(
            outputs[0].output.contains("file-bytes"),
            "got: {}",
            outputs[0].output
        );
        // c2 lacks a path: visible per-call failure, batch still ordered.
        assert!(
            outputs[1].output.contains("missing path"),
            "got: {}",
            outputs[1].output
        );
        // Turn 2: outputs travel as function_call_output; model finishes.
        let mut input: Vec<crate::provider::InputItem> = first
            .output
            .into_iter()
            .map(crate::provider::InputItem::ProviderOutput)
            .collect();
        input.extend(outputs.into_iter().map(|output| {
            crate::provider::InputItem::FunctionCallOutput {
                call_id: output.call_id,
                output: output.output,
            }
        }));
        let second = crate::provider::stream_input_observed(
            &provider_config(&server.base),
            "m",
            None,
            &input,
            &[],
            100,
            &NO_CANCEL,
            &mut |_| {},
        )
        .await
        .expect("second");
        assert_eq!(second.text, "done");
        assert_eq!(second.usage, Some((5, 6)));
        server.shutdown();
    }

    #[tokio::test]
    async fn prov04_opaque_boundary_persist_replay() {
        let env = setup();
        let items = vec![
            StreamItem::TextDelta("hi".to_string()),
            StreamItem::ReasoningDelta("quiet thought".to_string()),
            StreamItem::OpaqueItem {
                item_id: "rs_1".to_string(),
                payload: serde_json::json!({"type": "reasoning", "id": "rs_1", "encrypted": "zz"}),
            },
            StreamItem::Usage {
                input_tokens: 7,
                output_tokens: 8,
            },
        ];
        // UI projection never carries opaque or reasoning content.
        let shown = TurnLog::ui_projection(&items);
        assert_eq!(shown.len(), 2);
        let flat = format!("{shown:?}");
        assert!(!flat.contains("quiet thought"));
        assert!(!flat.contains("encrypted"));
        // Persist through the turn row and read back.
        let mut log = TurnLog::new("t-1", "m-a", "ludka2");
        for item in &items {
            log.ingest(item);
        }
        let db = crate::storage::Db::open(&env.data).expect("db");
        db.create_session("s-o").expect("session");
        db.begin_turn("t-1", "s-o", "prompt").expect("begin");
        log.save(&db, "completed").expect("save");
        let (status, result) = db.turn_result("t-1").expect("read");
        assert_eq!(status, "completed");
        let back =
            TurnLog::from_json(&serde_json::from_str(&result.expect("result")).expect("json"))
                .expect("parse");
        assert_eq!(back, log);
        // Same pair replays verbatim; any switch drops alien state loudly.
        let replayed = back.replay_for("m-a", "ludka2").expect("replay");
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0]["encrypted"], "zz");
        let err = back.replay_for("m-b", "ludka2").expect_err("alien model");
        assert!(err.contains("alien"));
        let err = back.replay_for("m-a", "other").expect_err("alien provider");
        assert!(err.contains("alien"));
    }
}
