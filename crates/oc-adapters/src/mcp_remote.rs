//! Remote MCP `codex_web` client for T20 (MCP01–03).
//!
//! rmcp streamable-HTTP against the exact configured URL (no probing, no
//! rewrite), protocol 2025-11-25, bearer auth, JSON responses, no session
//! dependency. One attempt per request (no auto-reconnect; session recovery
//! disabled), methods on distinct connections with reused headers, 60 s
//! client timeout, optional standalone GET answered 405 without affecting
//! the POST path. Results map into a namespaced collision-safe registry
//! with bounded paginated schemas; `isError` surfaces as failure.
//! OAuth discovery is unsupported and refused before any network use.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use thiserror::Error;

/// Required MCP wire version for `codex_web`.
pub const MCP_VERSION: &str = "2025-11-25";
/// Client timeout per operation.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(60);
/// Max tools/list pages followed (cursor loop bound).
pub const MAX_LIST_PAGES: usize = 16;
/// Max tools accepted per server.
pub const TOOLS_CAP: usize = 64;
/// Max input-schema bytes per tool.
pub const SCHEMA_BYTES_CAP: usize = 32 * 1024;

/// Typed remote-MCP errors (no secrets, no payload contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpError {
    /// Misconfiguration (bad URL, missing bearer, OAuth requested).
    #[error("invalid MCP config")]
    InvalidConfig,
    /// SSRF guard: non-public dial target.
    #[error("private host refused")]
    PrivateHost,
    /// 401 from the server: terminal, never retried.
    #[error("unauthorized")]
    Unauthorized,
    /// 403 from the server.
    #[error("forbidden")]
    Forbidden,
    /// Deadline exceeded (connect or operation).
    #[error("deadline exceeded")]
    Deadline,
    /// Transport or protocol failure (kind only).
    #[error("transport error")]
    Transport,
    /// Explicit cancellation closed the request.
    #[error("cancelled")]
    Cancelled,
    /// Tool reported `isError`.
    #[error("tool error")]
    ToolFailed,
    /// Malformed tool result shape.
    #[error("bad tool result")]
    BadResult,
}

/// Exact `codex_web` connection parameters.
///
/// `Debug` redacts the bearer: connection parameters are safe to log.
#[derive(Clone, PartialEq, Eq)]
pub struct CodexWebConfig {
    /// Exact server URL (path preserved verbatim, e.g. `…/v1/mcp`).
    pub url: String,
    /// Static bearer token (never logged).
    pub bearer: String,
    /// Per-operation timeout.
    pub timeout: Duration,
    /// Test-only private-network exception.
    pub allow_private: bool,
}

impl std::fmt::Debug for CodexWebConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexWebConfig")
            .field("url", &self.url)
            .field("bearer", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("allow_private", &self.allow_private)
            .finish()
    }
}

impl CodexWebConfig {
    /// Validate without touching the network (OAuth has no path here).
    pub fn validate(&self) -> Result<(), McpError> {
        if self.url.trim().is_empty() || self.bearer.trim().is_empty() {
            return Err(McpError::InvalidConfig);
        }
        if !(self.url.starts_with("http://") || self.url.starts_with("https://")) {
            return Err(McpError::InvalidConfig);
        }
        if self.url.contains(' ') || self.url.contains('\0') {
            return Err(McpError::InvalidConfig);
        }
        Ok(())
    }

    /// Build from a loaded [`McpEntry`](crate::config::McpEntry) without
    /// touching the network. OAuth entries are refused: this profile has no
    /// OAuth path, so a tokened entry never reaches the wire.
    pub fn from_entry(entry: &crate::config::McpEntry) -> Result<Self, McpError> {
        if entry.kind != "remote" || !entry.enabled || entry.oauth {
            return Err(McpError::InvalidConfig);
        }
        let url = entry.url.clone().ok_or(McpError::InvalidConfig)?;
        let raw = entry
            .headers
            .get("authorization")
            .cloned()
            .unwrap_or_default();
        let bearer = raw
            .strip_prefix("Bearer ")
            .unwrap_or(raw.as_str())
            .trim()
            .to_string();
        let config = Self {
            url,
            bearer,
            timeout: Duration::from_millis(entry.timeout.unwrap_or(60_000)),
            allow_private: false,
        };
        config.validate()?;
        Ok(config)
    }
}

/// One namespaced registry entry mapping an exact server tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEntry {
    /// Collision-safe model-facing name (`{server}__{tool}`).
    pub namespaced: String,
    /// Owning server id.
    pub server: String,
    /// Exact server-side tool name.
    pub tool: String,
    /// Description, if advertised.
    pub description: Option<String>,
    /// Bounded input schema.
    pub input_schema: serde_json::Value,
}

/// Map server tools into namespaced registry entries (bounded).
///
/// Same-server duplicates and oversized schemas are dropped with their ids
/// recorded in `skipped`, never silently renamed.
pub fn map_registry(
    server_id: &str,
    tools: Vec<(String, Option<String>, serde_json::Value)>,
) -> (Vec<RegistryEntry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut skipped = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (tool, description, schema) in tools.into_iter().take(TOOLS_CAP) {
        let namespaced = format!("{server_id}__{tool}");
        if !seen.insert(namespaced.clone()) {
            skipped.push(tool);
            continue;
        }
        if serde_json::to_string(&schema)
            .map(|s| s.len())
            .unwrap_or(usize::MAX)
            > SCHEMA_BYTES_CAP
        {
            skipped.push(tool);
            continue;
        }
        entries.push(RegistryEntry {
            namespaced,
            server: server_id.to_string(),
            tool,
            description,
            input_schema: schema,
        });
    }
    (entries, skipped)
}

/// Merge registries across servers with `__2`-style collision safety.
pub fn merge_registries(registries: Vec<Vec<RegistryEntry>>) -> Vec<RegistryEntry> {
    let mut out = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for entries in registries {
        for mut entry in entries {
            let base = entry.namespaced.clone();
            let mut candidate = base.clone();
            let mut suffix = 2u32;
            while !taken.insert(candidate.clone()) {
                candidate = format!("{base}__{suffix}");
                suffix += 1;
            }
            entry.namespaced = candidate;
            out.push(entry);
        }
    }
    out
}

/// Search arguments for the `codex_web` search tool.
pub fn search_args(query: &str, limit: Option<u32>) -> serde_json::Value {
    let mut args = serde_json::Map::new();
    args.insert(
        "query".to_string(),
        serde_json::Value::String(query.to_string()),
    );
    if let Some(limit) = limit {
        args.insert("limit".to_string(), serde_json::Value::from(limit));
    }
    serde_json::Value::Object(args)
}

type McpPeer = rmcp::service::Peer<rmcp::service::RoleClient>;
type McpRunning = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

/// Connected `codex_web` client: distinct connections per method, reused
/// headers, no session requirement.
pub struct CodexWebClient {
    peer: McpPeer,
    _running: McpRunning,
    timeout: Duration,
}

impl CodexWebClient {
    /// Connect: pre-dial SSRF guard, exact-URL handshake, 2025-11-25 required.
    pub async fn connect(config: &CodexWebConfig) -> Result<Self, McpError> {
        config.validate()?;
        let (host, port) = split_host_port(&config.url)?;
        crate::webfetch::check_host(&host, port, config.allow_private)
            .await
            .map_err(|e| match e {
                crate::webfetch::FetchError::PrivateHost => McpError::PrivateHost,
                _ => McpError::Transport,
            })?;

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| McpError::Transport)?;
        let transport_config =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                config.url.as_str(),
            )
            .auth_header(config.bearer.as_str())
            .control_request_timeout(Duration::from_secs(5))
            .max_sse_event_size(crate::webfetch::BODY_CAP_BYTES)
            .reinit_on_expired_session(false)
            .max_concurrent_requests(4);
        let transport =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransport::with_client(
                http,
                transport_config,
            );
        let running =
            tokio::time::timeout(config.timeout, rmcp::service::serve_client((), transport))
                .await
                .map_err(|_| McpError::Deadline)?
                .map_err(|e| classify_handshake(&e))?;
        // The negotiated version must be exactly 2025-11-25 for codex_web.
        let version = running
            .peer()
            .peer_info()
            .map(|info| info.protocol_version.to_string())
            .unwrap_or_default();
        if version != MCP_VERSION {
            return Err(McpError::Transport);
        }
        let peer = running.peer().clone();
        Ok(Self {
            peer,
            _running: running,
            timeout: config.timeout,
        })
    }

    /// List tools with a bounded cursor loop (paginated, capped).
    pub async fn list_tools(&self, cancel: &AtomicBool) -> Result<Vec<RemoteTool>, McpError> {
        let tools = self
            .run_cancel(cancel, self.peer.list_all_tools_bounded(), "list")
            .await?;
        Ok(tools
            .into_iter()
            .map(|tool| RemoteTool {
                name: tool.name.to_string(),
                description: tool.description.map(|d| d.to_string()),
                input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
            })
            .collect())
    }

    /// Call the `search` tool; `isError` surfaces as failure, never success.
    pub async fn search(
        &self,
        query: &str,
        limit: Option<u32>,
        cancel: &AtomicBool,
    ) -> Result<String, McpError> {
        let args = search_args(query, limit);
        self.call_tool("search", args, cancel).await
    }

    /// Call any mapped tool by its exact server-side name.
    pub async fn call_tool(
        &self,
        tool: &str,
        arguments: serde_json::Value,
        cancel: &AtomicBool,
    ) -> Result<String, McpError> {
        let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
        params.arguments = arguments.as_object().cloned();
        let outcome = self
            .run_cancel(cancel, self.peer.call_tool_once(params), "call")
            .await?;
        match outcome {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(McpError::ToolFailed);
                }
                let mut text = String::new();
                for block in &result.content {
                    if let rmcp::model::ContentBlock::Text(t) = block {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&t.text);
                    }
                }
                if text.is_empty() && result.content.is_empty() {
                    return Err(McpError::BadResult);
                }
                Ok(text)
            }
            _ => Err(McpError::Transport),
        }
    }

    /// Run a peer future with timeout + explicit cancellation. Dropping the
    /// future on cancel aborts the request; nothing is retried here.
    async fn run_cancel<T>(
        &self,
        cancel: &AtomicBool,
        future: impl std::future::Future<Output = Result<T, rmcp::service::ServiceError>> + Send,
        _op: &str,
    ) -> Result<T, McpError> {
        tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => Err(McpError::Cancelled),
            result = tokio::time::timeout(self.timeout, future) => match result {
                Err(_) => Err(McpError::Deadline),
                Ok(Err(_)) => Err(McpError::Transport),
                Ok(Ok(value)) => Ok(value),
            },
        }
    }
}

async fn wait_cancelled(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn classify_handshake(error: &rmcp::service::ClientInitializeError) -> McpError {
    let text = format!("{error:?}");
    if text.contains("401")
        || text.contains("Unauthorized")
        || text.contains("AuthRequired")
        || text.contains("Auth required")
    {
        McpError::Unauthorized
    } else if text.contains("403")
        || text.contains("Forbidden")
        || text.contains("InsufficientScope")
    {
        McpError::Forbidden
    } else {
        McpError::Transport
    }
}

fn split_host_port(url: &str) -> Result<(String, u16), McpError> {
    let rest = url.split("://").nth(1).ok_or(McpError::InvalidConfig)?;
    let authority = rest.split('/').next().unwrap_or("");
    if authority.is_empty() {
        return Err(McpError::InvalidConfig);
    }
    let default = if url.starts_with("https://") { 443 } else { 80 };
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() => match port.parse::<u16>() {
            Ok(port) => Ok((
                host.trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string(),
                port,
            )),
            Err(_) => Ok((authority.to_string(), default)),
        },
        _ => Ok((authority.to_string(), default)),
    }
}

/// Bounded cursor pagination without the unbounded SDK helper.
trait BoundedList {
    /// List all tools across at most [`MAX_LIST_PAGES`] pages.
    fn list_all_tools_bounded(
        &self,
    ) -> impl std::future::Future<
        Output = Result<Vec<rmcp::model::Tool>, rmcp::service::ServiceError>,
    > + Send;
}

impl BoundedList for McpPeer {
    async fn list_all_tools_bounded(
        &self,
    ) -> Result<Vec<rmcp::model::Tool>, rmcp::service::ServiceError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..MAX_LIST_PAGES {
            let params = rmcp::model::PaginatedRequestParams::default().with_cursor(cursor);
            let page = self.list_tools(Some(params)).await?;
            if tools.len() + page.tools.len() > TOOLS_CAP {
                break;
            }
            tools.extend(page.tools);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }
}

/// One advertised remote tool (bounded schema).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTool {
    /// Exact server-side name.
    pub name: String,
    /// Description, if advertised.
    pub description: Option<String>,
    /// Input schema object.
    pub input_schema: serde_json::Value,
}
