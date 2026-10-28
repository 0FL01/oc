//! Code Mode: the model-facing `execute` tool runs JavaScript in a confined interpreter written
//! in Rust. There is no JavaScript engine or host process: the program is parsed and evaluated
//! here, and every external effect goes through host requests (`tools.*` and `fetch`) that the
//! runtime admits like ordinary tool calls.
//!
//! Behaviour follows the donor's `packages/codemode` (OpenCode v2.0.12): the same input schema,
//! catalog/search contract, data boundary and result formatting. Native differences: resource
//! guards are always on (step budget, string/array/promise/log limits), local time is UTC and
//! `String.prototype.normalize` returns text unchanged.

mod ast;
mod builtins;
mod catalog;
mod generator;
mod host;
mod interp;
mod json;
mod lexer;
mod observed;
mod parser;
mod promise;
mod value;

#[cfg(test)]
mod tests;

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use serde_json::{Value as Json, json};
use tokio::sync::{mpsc, oneshot};

pub use catalog::{Catalog, CodeModeTool, schema_to_typescript, tool_expression};
pub use host::{FetchRequest, FetchResponse, HostRequest};
pub(crate) use observed::{ObservedCall, run_observed};

use interp::{Abrupt, Cx, Fatal};
use value::{PropKey, Slot, Value};

/// Model-facing tool name.
pub const TOOL_NAME: &str = "execute";

/// Invariant tool guidance (donor `core/src/codemode/tool.ts`). The changing catalog is appended
/// by [`tool_description`].
pub const DESCRIPTION: &str = "Run JavaScript in a confined Code Mode runtime to script tool calls and HTTP requests and compose their results.\n`fetch` is available for HTTP requests. Imports, direct filesystem access, and timers are unavailable; all other external access goes through `tools`.\nWithin `{ code }`, the only callable tools are those explicitly listed in the Code Mode catalog instructions or returned by the `search` function. Inside `{ code }`, ignore tools shown outside the Code Mode catalog. They are not available in the Code Mode runtime.\nCall tools through `tools` using only exact paths and signatures from the catalog. Do not infer or normalize tool names; preserve bracket notation such as `tools.<namespace>[\"tool-name\"](input)`.\nPrefer an explicit `return`; if omitted, the final top-level expression becomes the result.\nAwait every call whose completion matters; pending calls are interrupted when execution ends. Run independent calls concurrently with `Promise.all`.\nSet `background: true` on execute to run the entire program in the background. Completion is reported automatically; do not poll.";

/// Full tool description: invariant guidance followed by the current catalog.
pub fn tool_description(catalog: &Catalog) -> String {
    format!("{DESCRIPTION}\n\n{}", catalog.render())
}

/// JSON Schema of the `execute` input.
pub fn input_schema() -> Json {
    json!({
        "type": "object",
        "properties": {
            "code": {
                "type": "string",
                "description": "JavaScript program in the Code Mode subset. Top-level await and return are allowed."
            },
            "background": {
                "type": "boolean",
                "default": false,
                "description": "Run the entire program in the background; completion is reported automatically."
            }
        },
        "required": ["code"],
        "additionalProperties": false
    })
}

pub(crate) struct Input<'a> {
    pub(crate) code: &'a str,
    pub(crate) background: bool,
}

pub(crate) fn parse_input(value: &Json) -> Result<Input<'_>, String> {
    let fields = value
        .as_object()
        .ok_or("execute arguments must be an object")?;
    if fields
        .keys()
        .any(|key| key != "code" && key != "background")
    {
        return Err("unknown execute argument".into());
    }
    let code = fields
        .get("code")
        .and_then(Json::as_str)
        .ok_or("execute requires a string `code` argument")?;
    let background = match fields.get("background") {
        None => false,
        Some(Json::Bool(background)) => *background,
        Some(_) => return Err("execute `background` must be a boolean".into()),
    };
    Ok(Input { code, background })
}

const INTERPRETER_STACK_BYTES: usize = 256 << 20;
/// Concurrent host requests served for one program.
const MAX_CONCURRENT_REQUESTS: usize = 8;

/// Resource guards for one execution. Long Horizon keeps these as guards, not round limits.
#[derive(Debug, Clone)]
pub struct ExecuteLimits {
    pub max_steps: u64,
    pub max_tool_calls: Option<u32>,
    pub max_log_bytes: usize,
}

impl Default for ExecuteLimits {
    fn default() -> Self {
        let defaults = interp::Limits::default();
        ExecuteLimits {
            max_steps: defaults.max_steps,
            max_tool_calls: None,
            max_log_bytes: defaults.max_log_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub kind: String,
    pub message: String,
    pub suggestions: Vec<String>,
}

impl Diagnostic {
    fn new(kind: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            kind: kind.into(),
            message: message.into(),
            suggestions: Vec::new(),
        }
    }
}

/// Result of one program, after the value crossed the data boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub value: Result<Json, Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub logs: Vec<String>,
    pub logs_dropped: usize,
    pub tool_calls: u32,
}

/// Formatter-owned sections; literal marker text in a returned value is data.
pub(crate) struct OutputSections {
    pub value: String,
    pub warnings: String,
    pub logs: String,
}

impl OutputSections {
    pub fn output(&self) -> String {
        [&self.value, &self.warnings, &self.logs]
            .into_iter()
            .filter(|part| !part.is_empty())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

impl Outcome {
    fn failure(diagnostic: Diagnostic) -> Outcome {
        Outcome {
            value: Err(diagnostic),
            warnings: Vec::new(),
            logs: Vec::new(),
            logs_dropped: 0,
            tool_calls: 0,
        }
    }

    pub fn is_ok(&self) -> bool {
        self.value.is_ok()
    }

    /// Model-facing text, as the donor's `formatResult`.
    pub fn output(&self) -> String {
        self.output_sections().output()
    }

    pub(crate) fn output_sections(&self) -> OutputSections {
        let main = match &self.value {
            Ok(Json::String(s)) => s.clone(),
            Ok(value) => json::write_json(value, "  "),
            Err(error) => {
                let mut lines = vec![error.message.clone()];
                lines.extend(
                    error
                        .suggestions
                        .iter()
                        .filter(|hint| !error.message.contains(hint.as_str()))
                        .cloned(),
                );
                lines.join("\n").trim().to_string()
            }
        };
        let warnings = if self.value.is_ok() && !self.warnings.is_empty() {
            let lines: Vec<String> = self
                .warnings
                .iter()
                .map(|w| format!("- [{}] {}", w.kind, w.message))
                .collect();
            format!("Warnings:\n{}", lines.join("\n"))
        } else {
            String::new()
        };
        let logs = if !self.logs.is_empty() || self.logs_dropped > 0 {
            let mut logs = self.logs.join("\n");
            if self.logs_dropped > 0 {
                if !logs.is_empty() {
                    logs.push('\n');
                }
                logs.push_str(&format!(
                    "[logs truncated: {} more lines exceeded the log limit]",
                    self.logs_dropped
                ));
            }
            format!("Logs:\n{logs}")
        } else {
            String::new()
        };
        OutputSections {
            value: main,
            warnings,
            logs,
        }
    }
}

fn diagnostic_from(cx: &Cx, abrupt: Abrupt) -> Diagnostic {
    match abrupt {
        Abrupt::Fatal(Fatal::StepLimit) => Diagnostic::new(
            "TimeoutExceeded",
            format!(
                "Execution exceeded the Code Mode step limit of {} steps. Reduce the work done in one program.",
                cx.limits.max_steps
            ),
        ),
        Abrupt::Fatal(Fatal::Cancelled) => {
            Diagnostic::new("ExecutionFailure", "Execution was interrupted.")
        }
        Abrupt::Short | Abrupt::GenReturn(_) => {
            Diagnostic::new("ExecutionFailure", "Unexpected control flow.")
        }
        Abrupt::Throw(value) => thrown_diagnostic(cx, &value),
    }
}

fn thrown_diagnostic(cx: &Cx, value: &Value) -> Diagnostic {
    if let Value::Obj(obj) = value {
        let meta = match obj
            .borrow()
            .props
            .get(&PropKey::Sym(cx.sym_diagnostic.clone()))
            .map(|p| &p.slot)
        {
            Some(Slot::Data(Value::Obj(meta))) => Some(meta.clone()),
            _ => None,
        };
        if let Some(meta) = meta {
            let kind = match meta.own_data("kind") {
                Some(Value::Str(k)) => k.to_string(),
                _ => "ExecutionFailure".into(),
            };
            let message = match obj.own_data("message") {
                Some(Value::Str(m)) => m.to_string(),
                _ => String::new(),
            };
            let suggestions = meta
                .own_data("suggestions")
                .and_then(|s| s.as_obj().and_then(value::Obj::array_items))
                .unwrap_or_default()
                .iter()
                .filter_map(|v| match v {
                    Value::Str(s) => Some(s.to_string()),
                    _ => None,
                })
                .collect();
            return Diagnostic {
                kind,
                message,
                suggestions,
            };
        }
        if obj.is_error() {
            let mut message = cx
                .to_str(value)
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "Error".into());
            if let Some(Value::Num(line)) = obj.own_data("line") {
                message.push_str(&format!(" (line {line})"));
            }
            return Diagnostic::new("ExecutionFailure", message);
        }
    }
    let shown = match value {
        Value::Str(s) => s.to_string(),
        other => json::console_format(cx, other),
    };
    Diagnostic::new("ExecutionFailure", format!("Uncaught: {shown}"))
}

async fn execute_program(
    code: String,
    catalog: Arc<Catalog>,
    limits: ExecuteLimits,
    cancel: Arc<AtomicBool>,
    transport: host::Transport,
) -> Outcome {
    if code.trim().is_empty() {
        return Outcome::failure(Diagnostic::new("ParseError", "Code cannot be empty."));
    }
    let program = match parser::parse_program(&code) {
        Ok(program) => program,
        Err(err) => {
            let kind = if err.unsupported {
                "UnsupportedSyntax"
            } else {
                "ParseError"
            };
            return Outcome::failure(Diagnostic::new(
                kind,
                format!("{} ({}:{})", err.message, err.pos.line, err.pos.column),
            ));
        }
    };
    let interp_limits = interp::Limits {
        max_steps: limits.max_steps,
        max_log_bytes: limits.max_log_bytes,
        ..interp::Limits::default()
    };
    let host = host::Host {
        catalog,
        transport,
        max_tool_calls: limits.max_tool_calls,
        calls: std::cell::Cell::new(0),
    };
    let cx = Cx::new(interp_limits, cancel, host);
    let value = match cx.run_program(&program).await {
        Ok(value) => match json::to_host(&cx, &value).await {
            Ok(json) => Ok(json),
            Err(err) => Err(diagnostic_from(&cx, err)),
        },
        Err(err) => Err(diagnostic_from(&cx, err)),
    };
    let warnings = if value.is_ok() {
        let rejected = std::mem::take(&mut *cx.rejected.borrow_mut());
        rejected
            .iter()
            .filter(|cell| !cell.handled.get())
            .filter_map(|cell| cell.rejection())
            .map(|error| {
                let diagnostic = thrown_diagnostic(&cx, &error);
                Diagnostic {
                    message: format!(
                        "Unhandled rejection from an un-awaited promise: {}",
                        diagnostic.message
                    ),
                    ..diagnostic
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    let logs = std::mem::take(&mut *cx.logs.borrow_mut());
    Outcome {
        value,
        warnings,
        logs: logs.lines,
        logs_dropped: logs.dropped,
        tool_calls: cx.host.calls.get(),
    }
}

/// A running program. Host requests arrive on `requests`; the outcome follows.
pub struct Execution {
    pub requests: mpsc::UnboundedReceiver<HostRequest>,
    owner: ExecutionOwner,
}

/// Thread/outcome/cancellation ownership is shared, not request scheduling.
struct ExecutionOwner {
    outcome: oneshot::Receiver<Outcome>,
    cancel: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Execution {
    pub fn start(code: String, catalog: Arc<Catalog>, limits: ExecuteLimits) -> Execution {
        let (sender, requests) = mpsc::unbounded_channel();
        Execution {
            requests,
            owner: ExecutionOwner::start(
                code,
                catalog,
                limits,
                host::Transport::RequestOnly(sender),
            ),
        }
    }

    /// Asks the interpreter to stop at its next step.
    pub fn cancel(&self) {
        self.owner.cancel();
    }
}

impl ExecutionOwner {
    fn start(
        code: String,
        catalog: Arc<Catalog>,
        limits: ExecuteLimits,
        transport: host::Transport,
    ) -> Self {
        let (done, outcome) = oneshot::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("oc-codemode".into())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn(move || {
                interp::mark_stack_base();
                let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
                    let local = tokio::task::LocalSet::new();
                    let outcome = local.block_on(
                        &runtime,
                        execute_program(code, catalog, limits, flag, transport),
                    );
                    drop(local);
                    Ok::<Outcome, std::io::Error>(outcome)
                }));
                interp::teardown_heap();
                let outcome = match result {
                    Ok(Ok(outcome)) => outcome,
                    Ok(Err(err)) => Outcome::failure(Diagnostic::new(
                        "ExecutionFailure",
                        format!("Code Mode runtime could not start: {err}"),
                    )),
                    Err(_) => Outcome::failure(Diagnostic::new(
                        "ExecutionFailure",
                        "Code Mode interpreter failed unexpectedly.",
                    )),
                };
                let _ = done.send(outcome);
            });
        let thread = spawned.ok();
        Self {
            outcome,
            cancel,
            thread,
        }
    }

    fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    async fn join(&mut self) {
        if let Some(handle) = self.thread.take() {
            let _ = tokio::task::spawn_blocking(move || handle.join()).await;
        }
    }
}

/// Runs a program to completion, serving host requests with `handle`. Each handler must reply
/// on the request's channel; up to eight requests are served concurrently. `cancel` is the
/// turn's cancellation flag.
pub async fn run<H, F>(
    code: String,
    catalog: Arc<Catalog>,
    limits: ExecuteLimits,
    cancel: &AtomicBool,
    mut handle: H,
) -> Outcome
where
    H: FnMut(HostRequest) -> F,
    F: Future<Output = ()>,
{
    let mut execution = Execution::start(code, catalog, limits);
    if execution.owner.thread.is_none() {
        return Outcome::failure(Diagnostic::new(
            "ExecutionFailure",
            "Code Mode runtime could not start a thread.",
        ));
    }
    let mut pending = FuturesUnordered::new();
    let mut queued = std::collections::VecDeque::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let outcome = loop {
        while pending.len() < MAX_CONCURRENT_REQUESTS {
            let Some(request) = queued.pop_front() else {
                break;
            };
            pending.push(handle(request));
        }
        tokio::select! {
            biased;
            outcome = &mut execution.owner.outcome => {
                break outcome.unwrap_or_else(|_| Outcome::failure(Diagnostic::new(
                    "ExecutionFailure",
                    "Code Mode interpreter stopped unexpectedly.",
                )));
            }
            Some(request) = execution.requests.recv() => {
                if !execution.owner.cancel.load(Ordering::Relaxed) {
                    queued.push_back(request);
                }
            }
            Some(()) = pending.next(), if !pending.is_empty() => {}
            _ = tick.tick() => {
                if cancel.load(Ordering::Relaxed) {
                    execution.cancel();
                    // Dropping in-flight requests closes their reply channels.
                    pending.clear();
                    queued.clear();
                }
            }
        }
    };
    drop(pending);
    execution.owner.join().await;
    outcome
}
