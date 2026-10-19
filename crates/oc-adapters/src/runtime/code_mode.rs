//! Code Mode wiring for the `execute` tool.
//!
//! MCP servers default into Code Mode (donor v2.0.12): their tools leave the provider's direct
//! tool list and become `tools.<server>.<tool>` inside `execute`. `codemode: false` keeps a
//! server direct. Every host request a program makes is admitted through the same permission,
//! approval and MCP lease path as a model-issued call, bound to the `execute` operation.

use std::sync::atomic::AtomicUsize;

use oc_core::tool_output::{
    EXECUTE_CALLS_CAP, EXECUTE_CODE_BYTES, EXECUTE_INPUT_BYTES, EXECUTE_TOOL_BYTES, Execute,
    ExecuteCall, ExecuteCallStatus, ExecuteOutput, ExecuteOutputKind, PREVIEW_BYTES, Presentation,
};
use serde_json::Value as Json;

use super::mcp::McpGeneration;
use super::*;
use crate::codemode::{
    self, Catalog, CodeModeTool, ExecuteLimits, FetchRequest, FetchResponse, HostRequest,
    ObservedCall,
};

/// Model-facing name of the Code Mode tool.
pub(crate) const EXECUTE_TOOL: &str = codemode::TOOL_NAME;

fn limited_text(text: &str, budget: usize, secrets: &[String]) -> (String, bool) {
    let mut text = text.to_owned();
    crate::tools::output::redact_string(&mut text, secrets);
    let limited = text.len() > budget;
    text.truncate(text.floor_char_boundary(text.len().min(budget)));
    (text, limited)
}

fn input_preview(input: &Json, secrets: &[String]) -> (Json, bool) {
    let mut input = input.clone();
    crate::mcp_result::redact_json(&mut input, secrets);
    if input.to_string().len() <= EXECUTE_INPUT_BYTES {
        return (input, false);
    }
    // Retain structured fields that actually fit, never an invalid JSON prefix
    // or a changed value type. Missing fields are explicitly limited.
    let input = match input {
        Json::Object(fields) => {
            let mut kept = serde_json::Map::new();
            for (key, value) in fields {
                kept.insert(key.clone(), value);
                if serde_json::to_string(&kept).expect("JSON fields").len() > EXECUTE_INPUT_BYTES {
                    kept.remove(&key);
                }
            }
            Json::Object(kept)
        }
        Json::Array(values) => {
            let mut kept = Vec::new();
            for value in values {
                kept.push(value);
                if serde_json::to_string(&kept).expect("JSON values").len() > EXECUTE_INPUT_BYTES {
                    kept.pop();
                    break;
                }
            }
            Json::Array(kept)
        }
        Json::String(mut text) => {
            while serde_json::to_string(&text).expect("JSON string").len() > EXECUTE_INPUT_BYTES {
                text.truncate(text.floor_char_boundary(text.len() / 2));
            }
            Json::String(text)
        }
        input => input,
    };
    (input, true)
}

pub(super) fn intent_presentation(
    op: &str,
    arguments: &Json,
    secrets: &[String],
) -> Option<Box<Presentation>> {
    let mut execute = Execute::default();
    if let Some(code) = arguments.get("code").and_then(Json::as_str) {
        let (code, limited) = limited_text(code, EXECUTE_CODE_BYTES, secrets);
        execute.code = Some(code);
        execute.code_limited = limited;
    }
    progress_presentation(op, execute)
}

fn progress_presentation(op: &str, execute: Execute) -> Option<Box<Presentation>> {
    let mut facts = Presentation::new("", 0, false);
    facts.label_only = true;
    facts.execute = Some(execute);
    Db::bounded_mcp_presentation(op, Some(Box::new(facts)))
}

pub(super) fn refusal_presentation(
    op: &str,
    presentation: Option<Box<Presentation>>,
    output: &str,
    secrets: &[String],
) -> Option<Box<Presentation>> {
    let mut presentation = presentation?;
    let Some(execute) = presentation.execute.as_mut() else {
        return Some(presentation);
    };
    let safe = limited_text(output, usize::MAX, secrets).0;
    let mut result = Presentation::new(&safe, safe.len() as u64, false);
    execute.error = true;
    execute.elapsed_ms = Some(0);
    execute.output = vec![ExecuteOutput {
        kind: ExecuteOutputKind::Text,
        text: result.body.clone(),
        limited: result.body_limited,
    }];
    result.execute = presentation.execute.take();
    Db::bounded_mcp_presentation(op, Some(Box::new(result)))
}

/// The invocation owns bounded facts; watch only coalesces wakeups, not effects.
struct ExecuteProgress<'a> {
    facts: Mutex<Execute>,
    changed: tokio::sync::watch::Sender<()>,
    secrets: &'a [String],
}

impl ExecuteProgress<'_> {
    fn start(&self, tool: &str, input: &Json, producer_limited: bool) -> Option<usize> {
        let mut facts = self.facts.lock().expect("execute progress lock");
        if facts.calls.len() == EXECUTE_CALLS_CAP {
            if !facts.calls_limited {
                facts.calls_limited = true;
                self.changed.send_replace(());
            }
            return None;
        }
        let (tool, tool_limited) = limited_text(tool, EXECUTE_TOOL_BYTES, self.secrets);
        let (input, input_limited) = input_preview(input, self.secrets);
        let index = facts.calls.len();
        facts.calls.push(ExecuteCall {
            tool,
            tool_limited,
            status: ExecuteCallStatus::Running,
            input: Some(input),
            input_limited: producer_limited || input_limited,
        });
        self.changed.send_replace(());
        Some(index)
    }

    fn finish(&self, index: Option<usize>, failed: bool) {
        let Some(index) = index else {
            return;
        };
        self.facts.lock().expect("execute progress lock").calls[index].status = if failed {
            ExecuteCallStatus::Error
        } else {
            ExecuteCallStatus::Completed
        };
        self.changed.send_replace(());
    }

    fn mark_calls_limited(&self) {
        let mut facts = self.facts.lock().expect("execute progress lock");
        if !facts.calls_limited {
            facts.calls_limited = true;
            self.changed.send_replace(());
        }
    }

    fn emit(
        &self,
        op: &str,
        turn: &str,
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) {
        let facts = self.facts.lock().expect("execute progress lock").clone();
        if let Some(output_presentation) = progress_presentation(op, facts) {
            tool_event(
                turn,
                &ToolCallEvent::Progress {
                    op: op.into(),
                    output_presentation,
                },
            );
        }
    }
}

/// Splits permitted MCP tools into direct provider definitions and the Code Mode catalog.
pub(super) fn mcp_tool_defs(
    attached: &McpGeneration,
    policy: &RuntimePolicy<'_>,
) -> (Vec<ToolDef>, Catalog) {
    let mut direct = Vec::new();
    let mut nested = Vec::new();
    for entry in attached
        .entries
        .iter()
        .filter(|entry| policy.tool_visible(&entry.namespaced))
    {
        if attached.code_mode_server(&entry.server) {
            nested.push((
                CodeModeTool {
                    path: entry.namespaced.clone(),
                    description: entry.description.clone().unwrap_or_default(),
                    input_schema: entry.input_schema.clone(),
                    output_schema: None,
                },
                vec![entry.server.clone(), entry.tool.clone()],
            ));
        } else {
            direct.push(ToolDef {
                name: entry.namespaced.clone(),
                description: entry
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("mcp {} tool", entry.tool)),
                parameters: entry.input_schema.clone(),
            });
        }
    }
    (direct, Catalog::with_paths(nested, Vec::new()))
}

/// Request tool definitions for MCP: direct tools plus `execute` when the catalog is non-empty
/// and `execute` itself is not denied.
pub(super) fn mcp_request_tools(
    attached: &McpGeneration,
    policy: &RuntimePolicy<'_>,
) -> Vec<ToolDef> {
    let (mut defs, catalog) = mcp_tool_defs(attached, policy);
    if !catalog.is_empty() && policy.tool_visible(EXECUTE_TOOL) {
        defs.push(ToolDef {
            name: EXECUTE_TOOL.into(),
            description: codemode::tool_description(&catalog),
            parameters: codemode::input_schema(),
        });
    }
    defs
}

fn mcp_value(output: &crate::mcp_result::McpToolOutput) -> Json {
    let facts = output.facts();
    if let Some(structured) = facts.get("structuredContent").filter(|v| !v.is_null()) {
        return structured.clone();
    }
    let text = output.texts().join("\n");
    if text.is_empty() {
        Json::Null
    } else {
        Json::String(text)
    }
}

fn admission_message(path: &str, failure: AdmissionFailure) -> String {
    match failure {
        AdmissionFailure::Required => {
            format!(
                "Permission for '{path}' requires approval, but no approval consumer is attached."
            )
        }
        AdmissionFailure::Denied(_) => format!("Permission denied for '{path}'."),
        AdmissionFailure::Rejected(Some(feedback)) => {
            format!("The user rejected the call to '{path}': {feedback}")
        }
        AdmissionFailure::Rejected(None) => format!("The user rejected the call to '{path}'."),
        AdmissionFailure::Reloaded => format!("The source reloaded before '{path}' was approved."),
        AdmissionFailure::Cancelled => format!("Approval for '{path}' was cancelled."),
        AdmissionFailure::Invalid(reason) => format!("Call to '{path}' was not admitted: {reason}"),
    }
}

/// Immutable context of one `execute` invocation shared by its host requests.
struct Invocation<'a, 'p> {
    issuing: &'a ExecutionGeneration,
    call: &'a crate::tools::ToolCall,
    session: &'a str,
    turn: &'a str,
    op: &'a str,
    ctx: &'a ToolContext<'a>,
    policy: &'a RuntimePolicy<'p>,
    attached: &'a McpGeneration,
    cancel: &'a AtomicBool,
    agent: Option<String>,
    agent_digest: Option<String>,
    sequence: AtomicUsize,
}

impl Invocation<'_, '_> {
    fn nested_call(&self, name: &str, arguments: Json) -> crate::tools::ToolCall {
        let n = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        crate::tools::ToolCall {
            id: format!("{}#{n}", self.call.id),
            name: name.into(),
            arguments,
        }
    }
}

impl Runtime<'_> {
    async fn admit_nested(
        &self,
        inv: &Invocation<'_, '_>,
        nested: &crate::tools::ToolCall,
        path: &str,
    ) -> Result<(), String> {
        let permitted = self
            .admit_tool(
                inv.issuing,
                inv.session,
                inv.turn,
                inv.op,
                nested,
                inv.ctx,
                inv.policy,
                inv.cancel,
                inv.agent.clone(),
                inv.agent_digest.clone(),
                false,
            )
            .await
            .map_err(|failure| admission_message(path, failure))?;
        permitted
            .check_call(nested)
            .map_err(|error| format!("Call to '{path}' was not admitted: {error}"))
    }

    async fn code_mode_tool(
        &self,
        inv: &Invocation<'_, '_>,
        path: &str,
        input: Json,
    ) -> Result<Json, String> {
        let Some(entry) =
            inv.attached.entries.iter().find(|entry| {
                entry.namespaced == path && inv.attached.code_mode_server(&entry.server)
            })
        else {
            return Err(format!("Unknown tool '{path}'."));
        };
        let nested = inv.nested_call(&entry.namespaced, input);
        let display = format!("{}.{}", entry.server, entry.tool);
        self.admit_nested(inv, &nested, &display).await?;
        let mut native = None;
        let (state, text) = self
            .execute_mcp(
                &nested,
                inv.op,
                inv.turn,
                inv.attached,
                inv.cancel,
                &mut native,
            )
            .await;
        if state != "completed" {
            return Err(text.strip_prefix("error: ").unwrap_or(&text).to_string());
        }
        Ok(match native {
            Some(output) => mcp_value(&output),
            None => Json::String(text),
        })
    }

    async fn code_mode_fetch(
        &self,
        inv: &Invocation<'_, '_>,
        request: FetchRequest,
    ) -> Result<FetchResponse, String> {
        let mut url = reqwest::Url::parse(&request.url)
            .map_err(|_| format!("Invalid URL: {}", request.url))?;
        crate::webfetch::validate_url(&mut url).map_err(|error| error.to_string())?;
        crate::webfetch::check_static_host(&url, inv.ctx.webfetch_allow_private)
            .map_err(|error| error.to_string())?;
        // `fetch` is admitted as the `webfetch` action on the requested URL.
        let nested = inv.nested_call(
            "webfetch",
            serde_json::json!({ "url": url.as_str(), "format": "text" }),
        );
        self.admit_nested(inv, &nested, "fetch").await?;
        let options = crate::webfetch::FetchOptions {
            allow_loopback: inv.ctx.webfetch_allow_private,
            ..crate::webfetch::FetchOptions::default()
        };
        let response = crate::webfetch::fetch_raw(
            crate::webfetch::RawRequest {
                url: url.to_string(),
                method: request.method,
                headers: request.headers,
                body: request.body,
            },
            options,
            inv.cancel,
        )
        .await
        .map_err(|error| error.to_string())?;
        Ok(FetchResponse {
            url: response.url,
            status: response.status,
            status_text: response.status_text,
            redirected: response.redirected,
            headers: response.headers,
            body: response.body,
        })
    }

    /// Runs one `execute` call after its own admission and durable intent.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn execute_code_mode(
        &self,
        issuing: &ExecutionGeneration,
        call: &crate::tools::ToolCall,
        session: &str,
        turn: &str,
        op: &str,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        attached: &McpGeneration,
        cancel: &AtomicBool,
        agent: Option<String>,
        agent_digest: Option<String>,
        secrets: &[String],
        tool_event: &mut (dyn FnMut(&str, &ToolCallEvent) + Send),
    ) -> (&'static str, String, Execute) {
        let Some(code) = call.arguments.get("code").and_then(Json::as_str) else {
            return (
                "failed",
                "error: execute requires a string `code` argument".into(),
                Execute {
                    error: true,
                    elapsed_ms: Some(0),
                    ..Default::default()
                },
            );
        };
        let (_, catalog) = mcp_tool_defs(attached, policy);
        let catalog = Arc::new(catalog);
        let (code_preview, code_limited) = limited_text(code, EXECUTE_CODE_BYTES, secrets);
        let (changed, mut changes) = tokio::sync::watch::channel(());
        let progress = ExecuteProgress {
            facts: Mutex::new(Execute {
                code: Some(code_preview),
                code_limited,
                ..Default::default()
            }),
            changed,
            secrets,
        };
        let progress = &progress;
        progress.emit(op, turn, tool_event);
        let inv = Invocation {
            issuing,
            call,
            session,
            turn,
            op,
            ctx,
            policy,
            attached,
            cancel,
            agent,
            agent_digest,
            sequence: AtomicUsize::new(0),
        };
        let inv = &inv;
        let started = std::time::Instant::now();
        let mut execution = Box::pin(codemode::run_observed(
            code.to_string(),
            catalog.clone(),
            ExecuteLimits::default(),
            cancel,
            |observed| {
                // Capture actual invocation order before external admission;
                // completion updates the same slot even when requests settle
                // concurrently. Local search has no external effect to admit.
                match observed {
                    ObservedCall::Tool { path, input } => progress.start(
                        catalog
                            .display_path_for_host(path)
                            .unwrap_or("[unavailable tool]"),
                        input,
                        false,
                    ),
                    ObservedCall::Fetch(request) => progress.start(
                        "fetch",
                        &serde_json::json!({
                            "method": request.method, "url": request.url,
                        }),
                        false,
                    ),
                    ObservedCall::LocalSearch {
                        input,
                        input_limited,
                    } => {
                        let index = progress.start("search", input, input_limited);
                        progress.finish(index, false);
                        index
                    }
                    ObservedCall::CallsLimited => {
                        progress.mark_calls_limited();
                        None
                    }
                }
            },
            |request, index| async move {
                match request {
                    HostRequest::Tool { path, input, reply } => {
                        let result = self.code_mode_tool(inv, &path, input).await;
                        progress.finish(index, result.is_err());
                        let _ = reply.send(result);
                    }
                    HostRequest::Fetch { request, reply } => {
                        let result = self.code_mode_fetch(inv, request).await;
                        progress.finish(index, result.is_err());
                        let _ = reply.send(result);
                    }
                }
            },
        ));
        let outcome = loop {
            tokio::select! {
                outcome = &mut execution => break outcome,
                changed = changes.changed() => {
                    if changed.is_ok() { progress.emit(op, turn, tool_event); }
                }
            }
        };
        let mut facts = progress
            .facts
            .lock()
            .expect("execute progress lock")
            .clone();
        facts.elapsed_ms = Some(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        facts.error = !outcome.is_ok() || cancel.load(Ordering::Acquire);
        let sections = outcome.output_sections();
        let mut remaining = PREVIEW_BYTES;
        for (kind, text) in [
            (ExecuteOutputKind::Text, &sections.value),
            (ExecuteOutputKind::Warnings, &sections.warnings),
            (ExecuteOutputKind::Logs, &sections.logs),
        ] {
            if text.is_empty() {
                continue;
            }
            let kind = if kind == ExecuteOutputKind::Text
                && serde_json::from_str::<Json>(&limited_text(text, usize::MAX, secrets).0)
                    .is_ok_and(|value| value.is_object() || value.is_array())
            {
                ExecuteOutputKind::Json
            } else {
                kind
            };
            let (text, limited) = limited_text(text, remaining, secrets);
            remaining -= text.len();
            facts.output.push(ExecuteOutput {
                kind,
                text,
                limited: limited || (kind == ExecuteOutputKind::Logs && outcome.logs_dropped > 0),
            });
        }
        if cancel.load(Ordering::Acquire) {
            return ("cancelled", "error: cancelled".into(), facts);
        }
        (
            if outcome.is_ok() {
                "completed"
            } else {
                "failed"
            },
            sections.output(),
            facts,
        )
    }
}

#[cfg(test)]
#[path = "code_mode/tests.rs"]
mod tests;
