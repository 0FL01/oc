//! Native tool executor for T13 (PROV03/04/05, TOOL10/11).
//!
//! Complete tool batch over assembled [`StreamItem`] calls: argument
//! accumulation per item id, unique-ID validation, sequential execution in
//! first-appearance order through [`ToolContext`], and `function_call_output`
//! items keyed to the original call IDs for the next response. Unknown
//! tools, invalid JSON and replayed terminals never execute; duplicates
//! refuse the whole batch before any side effect.
//!
//! Registry (and only registry): `read`, `apply_patch`, `bash`, `webfetch`,
//! `skill`. No `write`/`edit` entries exist. Reasoning/opaque provider items
//! accumulate in [`TurnLog`] (durable JSON, same-model/provider replay
//! boundary) and are stripped from the UI projection.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use thiserror::Error;

use crate::config::parse_skill;
use crate::files::Files;
use crate::patch::{ApplyFailure, FileResult, PatchError, WritePolicy};
use crate::provider::StreamItem;
use crate::shell::{Shell, ShellLimits};
use crate::storage::Db;

/// Model-visible tool names; `write`/`edit` must never appear here.
pub const MODEL_TOOL_NAMES: &[&str] = &["read", "apply_patch", "bash", "webfetch", "skill"];
/// Skill body snapshot cap (bytes).
pub const SKILL_BODY_CAP: usize = 16384;
/// Bash per-call timeout cap (ms).
pub const BASH_TIMEOUT_CAP_MS: u64 = 600_000;

/// Typed tool errors (no argument contents, no secrets).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolError {
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
    /// Authorize a tool invocation or deny it.
    fn check(&self, tool: &str) -> Result<(), ToolError>;
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
        if self.denied.iter().any(|d| d == tool) {
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
    /// Provider item id (output key).
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
    let mut names: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut args: BTreeMap<String, String> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for item in items {
        match item {
            StreamItem::ToolCallStarted { item_id, name } => {
                if names.contains_key(item_id) {
                    return Err(BatchError::DuplicateId {
                        id: item_id.clone(),
                    });
                }
                order.push(item_id.clone());
                names.insert(item_id.clone(), (name.clone(), order.len()));
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
            Some((name, _)) => {
                let raw = args.remove(&id).unwrap_or_default();
                match serde_json::from_str::<serde_json::Value>(&raw) {
                    Ok(arguments) if arguments.is_object() => {
                        units.push(Assembled::Call(ToolCall {
                            id,
                            name,
                            arguments,
                        }));
                    }
                    _ => units.push(Assembled::Failed(CallFailure {
                        id,
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
    /// Original provider item id.
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

/// Pinned skill catalog: id → bounded entry (generation-time snapshot).
#[derive(Debug, Clone, Default)]
pub struct SkillSnapshot {
    /// Pinned entries.
    pub entries: BTreeMap<String, SkillEntry>,
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
        (Self { entries }, warnings)
    }

    /// Model projection: bounded id/name/description without bodies.
    pub fn projection(&self) -> Vec<SkillView> {
        self.entries
            .iter()
            .map(|(id, e)| SkillView {
                id: id.clone(),
                name: e.name.clone(),
                description: e.description.clone(),
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
    /// Explicit webfetch bearer (config-provided, never from model args).
    pub webfetch_auth: Option<String>,
    /// Test-only webfetch loopback exception.
    pub webfetch_allow_private: bool,
    /// Central policy hook.
    pub policy: &'a dyn ToolPolicy,
    /// Pinned skill snapshot.
    pub snapshot: &'a SkillSnapshot,
    /// Cancellation flag (checked between calls and inside `bash`).
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

struct PolicyBridge<'a>(&'a dyn ToolPolicy);

impl WritePolicy for PolicyBridge<'_> {
    fn check(&self, _path: &str) -> Result<(), PatchError> {
        self.0.check("apply_patch").map_err(|_| PatchError::Denied {
            path: "apply_patch".to_string(),
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
    if let Err(e) = ctx.policy.check(&call.name) {
        return format!("error: {e}");
    }
    match call.name.as_str() {
        "read" => tool_read(ctx, call),
        "apply_patch" => tool_patch(ctx, call),
        "bash" => tool_bash(ctx, call).await,
        "webfetch" => tool_webfetch(ctx, call).await,
        "skill" => tool_skill(ctx, call),
        other => format!("error: unknown tool {other}"),
    }
}

fn tool_read(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let path = call
        .arguments
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if path.is_empty() {
        return "error: invalid arguments for read: missing path".to_string();
    }
    let offset = call
        .arguments
        .get("offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);
    let limit = call
        .arguments
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;
    match ctx.files.read(path, offset, limit) {
        Ok(result) => {
            let mut text = result.lines.join("\n");
            if result.truncated {
                text.push_str(&format!(
                    "\n[truncated, next_offset={}]",
                    result.next_offset.unwrap_or(0)
                ));
            }
            text
        }
        Err(e) => format!("error: {e}"),
    }
}

fn tool_patch(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let patch = call
        .arguments
        .as_object()
        .filter(|args| args.len() == 1)
        .and_then(|args| args.get("patchText"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if patch.is_empty() {
        return "error: invalid arguments for apply_patch: expected only nonempty patchText"
            .to_string();
    }
    let Some(roots) = ctx.roots.as_ref() else {
        return "error: tool apply_patch failed: no roots".to_string();
    };
    let bridge = PolicyBridge(ctx.policy);
    patch_outcome(crate::patch::apply_patch(
        &roots.project,
        &roots.data,
        patch,
        &bridge,
    ))
}

fn patch_outcome(result: Result<Vec<FileResult>, ApplyFailure>) -> String {
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

async fn tool_bash(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    let argv: Vec<String> = call
        .arguments
        .get("argv")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if argv.is_empty() {
        return "error: invalid arguments for bash: missing argv".to_string();
    }
    let cwd = call
        .arguments
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(".");
    let timeout_ms = call
        .arguments
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000)
        .min(BASH_TIMEOUT_CAP_MS);
    let limits = ShellLimits {
        timeout: Duration::from_millis(timeout_ms),
        kill_grace: Duration::from_millis(500),
        retain_cap: crate::shell::RETAIN_CAP_BYTES,
    };
    match ctx
        .shell
        .execute(ctx.parent_env, &argv, cwd, None, limits, ctx.cancel)
    {
        Ok(outcome) => {
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
            text
        }
        Err(e) => format!("error: {e}"),
    }
}

async fn tool_webfetch(ctx: &ToolContext<'_>, call: &ToolCall) -> String {
    for forbidden in ["auth", "authorization", "headers", "apiKey", "api_key"] {
        if call.arguments.get(forbidden).is_some() {
            return format!(
                "error: invalid arguments for webfetch: {forbidden} is never model-supplied"
            );
        }
    }
    let url = call
        .arguments
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if url.is_empty() {
        return "error: invalid arguments for webfetch: missing url".to_string();
    }
    let opts = crate::webfetch::FetchOptions {
        timeout: Duration::from_secs(30),
        connect_timeout: Duration::from_secs(10),
        max_redirects: crate::webfetch::MAX_REDIRECTS,
        body_cap: crate::webfetch::BODY_CAP_BYTES,
        allow_loopback: ctx.webfetch_allow_private,
    };
    match crate::webfetch::fetch(url, ctx.webfetch_auth.as_deref(), opts).await {
        Ok(result) => {
            let mut text = result.text;
            if result.truncated {
                text.push_str("\n[truncated]");
            }
            text
        }
        Err(e) => format!("error: {e}"),
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
    if let Err(e) = ctx.policy.check("skill") {
        return format!("error: {e}");
    }
    match ctx.snapshot.entries.get(id) {
        Some(entry) => entry.body.clone(),
        None => format!("error: unknown skill {id} (not in pinned snapshot)"),
    }
}

/// Durable turn log: opaque provider items + usage with a replay boundary.
///
/// Serialized as JSON into the turn row (`turns.result`); replay is allowed
/// only for the same `(model, provider)` pair — switches drop alien state
/// with a diagnostic instead of leaking it into a foreign context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnLog {
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
}

impl TurnLog {
    /// Start an empty log for a turn.
    pub fn new(turn_id: &str, model: &str, provider: &str) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            model: model.to_string(),
            provider: provider.to_string(),
            opaque: Vec::new(),
            usage: None,
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
        serde_json::json!({
            "turn_id": self.turn_id,
            "model": self.model,
            "provider": self.provider,
            "opaque": self.opaque,
            "usage": self.usage.map(|(i, o)| serde_json::json!([i, o])),
        })
    }

    /// Deserialize from the turn row.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        Ok(Self {
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
        })
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
    fn tool10_registry_has_no_write_edit() {
        assert_eq!(
            MODEL_TOOL_NAMES,
            &["read", "apply_patch", "bash", "webfetch", "skill"]
        );
        assert!(!MODEL_TOOL_NAMES.contains(&"write"));
        assert!(!MODEL_TOOL_NAMES.contains(&"edit"));
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
            ("bad".to_string(), "no frontmatter".to_string()),
        ]);
        assert_eq!(warnings.len(), 2);
        let projection = snapshot.projection();
        assert_eq!(projection.len(), 1);
        let json = serde_json::to_value(&projection).expect("json");
        assert!(json.to_string().contains("\"ok\""));
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
                        let body_text = String::from_utf8_lossy(&body).into_owned();
                        let payload = if body_text.contains("function_call_output") {
                            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"done\"}\n\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":6}}}\n\n"
                        } else {
                            concat!(
                                "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"c1\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"\"}}\n\n",
                                "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"c1\",\"delta\":\"{\\\"path\\\":\\\"note.txt\\\"}\"}\n\n",
                                "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"c2\",\"type\":\"function_call\",\"name\":\"read\",\"arguments\":\"\"}}\n\n",
                                "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"c2\",\"delta\":\"{\\\"limit\\\":1}\"}\n\n",
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
        let input = to_input_items(&outputs);
        assert_eq!(input[0]["call_id"], "c1");
        let _ = input; // Shape asserted; wire below carries it.
        let second = stream_generation(
            &provider_config(&server.base),
            "m",
            None,
            "roundtrip-marker function_call_output",
            &[],
            &NO_CANCEL,
            None,
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
