//! Code Mode wiring for the `execute` tool.
//!
//! MCP servers default into Code Mode (donor v2.0.12): their tools leave the provider's direct
//! tool list and become `tools.<server>.<tool>` inside `execute`. `codemode: false` keeps a
//! server direct. Every host request a program makes is admitted through the same permission,
//! approval and MCP lease path as a model-issued call, bound to the `execute` operation.

use std::sync::atomic::AtomicUsize;

use serde_json::Value as Json;

use super::mcp::McpGeneration;
use super::*;
use crate::codemode::{
    self, Catalog, CodeModeTool, ExecuteLimits, FetchRequest, FetchResponse, HostRequest,
};

/// Model-facing name of the Code Mode tool.
pub(crate) const EXECUTE_TOOL: &str = codemode::TOOL_NAME;

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
    ) -> (&'static str, String) {
        let Some(code) = call.arguments.get("code").and_then(Json::as_str) else {
            return (
                "failed",
                "error: execute requires a string `code` argument".into(),
            );
        };
        let (_, catalog) = mcp_tool_defs(attached, policy);
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
        let outcome = codemode::run(
            code.to_string(),
            Arc::new(catalog),
            ExecuteLimits::default(),
            cancel,
            |request| async move {
                match request {
                    HostRequest::Tool { path, input, reply } => {
                        let result = self.code_mode_tool(inv, &path, input).await;
                        let _ = reply.send(result);
                    }
                    HostRequest::Fetch { request, reply } => {
                        let result = self.code_mode_fetch(inv, request).await;
                        let _ = reply.send(result);
                    }
                }
            },
        )
        .await;
        if cancel.load(Ordering::Acquire) {
            return ("cancelled", "error: cancelled".into());
        }
        (
            if outcome.is_ok() {
                "completed"
            } else {
                "failed"
            },
            outcome.output(),
        )
    }
}
