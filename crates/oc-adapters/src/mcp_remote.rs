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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::header::{
    ACCEPT, AUTHORIZATION, CONNECTION, CONTENT_LENGTH, CONTENT_TYPE, HOST, HeaderMap, HeaderName,
    HeaderValue, TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use sha2::{Digest as _, Sha256};
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
/// Maximum retained text from one MCP call.
pub const RESULT_TEXT_BYTES_CAP: usize = 1024 * 1024;
/// Provider-compatible function-name ceiling.
pub const WIRE_NAME_BYTES_CAP: usize = 64;
/// Maximum retained server/tool identifier bytes.
pub const ORIGINAL_NAME_BYTES_CAP: usize = 256;
/// Maximum model-visible description per MCP tool.
pub const DESCRIPTION_BYTES_CAP: usize = 8 * 1024;
/// Maximum MCP tools across one published application generation.
pub const GENERATION_TOOLS_CAP: usize = 128;
/// Aggregate schema/description metadata ceiling for one generation.
pub const GENERATION_CATALOG_BYTES_CAP: usize = 1024 * 1024;
/// Maximum time spent waiting for explicit client shutdown.
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

/// Why an advertised catalog cannot be attached.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// A server advertised the same wire tool name more than once.
    #[error("duplicate tool name: {0}")]
    DuplicateTool(String),
    /// A server advertised a schema beyond [`SCHEMA_BYTES_CAP`].
    #[error("tool schema exceeds size limit: {0}")]
    SchemaTooLarge(String),
    /// Tool/server identity is empty or exceeds the retained-name bound.
    #[error("invalid tool identity")]
    InvalidIdentity,
    /// A model-visible description exceeds its byte ceiling.
    #[error("tool description exceeds size limit: {0}")]
    DescriptionTooLarge(String),
}

/// Typed remote-MCP errors (no secrets, no payload contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum McpError {
    /// Misconfiguration (bad URL, missing bearer, OAuth requested).
    #[error("invalid MCP config")]
    InvalidConfig,
    /// A configured HTTP header name or value is invalid.
    #[error("invalid MCP header: {0}")]
    InvalidHeader(String),
    /// Case variants supplied different values for the same HTTP header.
    #[error("conflicting MCP header values: {0}")]
    ConflictingHeader(String),
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
    /// Host resolution failed before initialize (no hostname exposed).
    #[error("DNS resolution failed")]
    Dns,
    /// Typed HTTP client connection failure, without endpoint details.
    #[error("connection failed")]
    Connect,
    /// An owned service or HTTP worker did not confirm clean teardown.
    #[error("MCP cleanup failed")]
    CleanupFailed,
    /// Negotiated protocol does not meet the explicitly selected profile.
    #[error("protocol mismatch")]
    ProtocolMismatch,
    /// Explicit cancellation closed the request.
    #[error("cancelled")]
    Cancelled,
    /// Tool reported `isError`.
    #[error("tool error")]
    ToolFailed,
    /// Tool failure with a fixed, payload-free diagnostic category.
    #[error("tool error: {0}")]
    ToolFailedDetail(crate::mcp_result::FailureDetail),
    /// Tool arguments were not a JSON object.
    #[error("invalid tool arguments")]
    InvalidArguments,
    /// Malformed tool result shape.
    #[error("bad tool result")]
    BadResult,
    /// The result used a modality this text-only adapter does not support.
    #[error("unsupported tool result")]
    UnsupportedResult,
    /// Catalog pagination or tool count exceeded a hard bound.
    #[error("MCP catalog exceeds limit")]
    CatalogLimited,
    /// The catalog is complete but cannot be attached safely.
    #[error("invalid MCP catalog: {0}")]
    Catalog(#[from] CatalogError),
}

/// Exact `codex_web` connection parameters.
///
/// `Debug` redacts the bearer and every custom-header value.
#[derive(Clone, PartialEq, Eq)]
pub struct CodexWebConfig {
    /// Exact server URL (path preserved verbatim, e.g. `…/v1/mcp`).
    pub url: String,
    /// Static bearer token (never logged).
    pub bearer: String,
    /// Typed, normalized safe custom headers (values never logged).
    pub custom_headers: HeaderMap,
    /// Per-operation timeout.
    pub timeout: Duration,
    /// Test-only private-network exception.
    pub allow_private: bool,
}

impl std::fmt::Debug for CodexWebConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexWebConfig")
            .field("url", &"<configured endpoint>")
            .field("bearer", &"<redacted>")
            .field("custom_headers", &RedactedHeaders(&self.custom_headers))
            .field("timeout", &self.timeout)
            .field("allow_private", &self.allow_private)
            .finish()
    }
}

impl CodexWebConfig {
    /// Validate without touching the network (OAuth has no path here).
    pub fn validate(&self) -> Result<(), McpError> {
        self.validate_profile(true)
    }

    fn validate_profile(&self, require_bearer: bool) -> Result<(), McpError> {
        if self.url.trim().is_empty() || (require_bearer && self.bearer.trim().is_empty()) {
            return Err(McpError::InvalidConfig);
        }
        let url = reqwest::Url::parse(&self.url).map_err(|_| McpError::InvalidConfig)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(McpError::InvalidConfig);
        }
        if let Some(name) = self
            .custom_headers
            .keys()
            .find(|name| is_native_header(name))
        {
            return Err(McpError::InvalidHeader(name.as_str().to_string()));
        }
        Ok(())
    }

    /// Build from a loaded [`McpEntry`](crate::config::McpEntry) without
    /// touching the network. OAuth entries are refused: this profile has no
    /// OAuth path, so a tokened entry never reaches the wire.
    pub fn from_entry(entry: &crate::config::McpEntry) -> Result<Self, McpError> {
        Self::from_entry_profile(entry, true)
    }

    /// Generic remote entry: disabling OAuth does not require static bearer
    /// authentication. An absent header stays absent on the wire.
    pub fn from_remote_entry(entry: &crate::config::McpEntry) -> Result<Self, McpError> {
        Self::from_entry_profile(entry, false)
    }

    fn from_entry_profile(
        entry: &crate::config::McpEntry,
        require_bearer: bool,
    ) -> Result<Self, McpError> {
        if entry.kind != "remote" || !entry.enabled || entry.oauth {
            return Err(McpError::InvalidConfig);
        }
        let url = entry.url.clone().ok_or(McpError::InvalidConfig)?;
        let mut headers = normalize_headers(&entry.headers)?;
        let authorization = headers.remove(AUTHORIZATION);
        let raw = authorization
            .as_ref()
            .map(|authorization| {
                authorization
                    .to_str()
                    .map_err(|_| McpError::InvalidHeader(AUTHORIZATION.as_str().to_string()))
            })
            .transpose()?
            .unwrap_or_default()
            .trim();
        let mut fields = raw.split_ascii_whitespace();
        let first = fields.next().unwrap_or_default();
        let bearer = match (fields.next(), fields.next()) {
            (Some(token), None) if first.eq_ignore_ascii_case("bearer") => token,
            (None, None) => first,
            _ => return Err(McpError::InvalidConfig),
        }
        .to_string();
        if authorization.is_some() && bearer.is_empty() {
            return Err(McpError::InvalidConfig);
        }
        let custom_headers = headers
            .iter()
            .filter(|(name, _)| !is_native_header(name))
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let config = Self {
            url,
            bearer,
            custom_headers,
            timeout: Duration::from_millis(entry.timeout.unwrap_or(60_000)),
            allow_private: false,
        };
        config.validate_profile(require_bearer)?;
        Ok(config)
    }
}

struct RedactedHeaders<'a>(&'a HeaderMap);

impl std::fmt::Debug for RedactedHeaders<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut map = f.debug_map();
        for name in self.0.keys() {
            map.entry(&name.as_str(), &"<redacted>");
        }
        map.finish()
    }
}

fn normalize_headers(
    raw_headers: &std::collections::BTreeMap<String, String>,
) -> Result<HeaderMap, McpError> {
    let mut headers = HeaderMap::new();
    for (raw_name, raw_value) in raw_headers {
        let name = HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|_| McpError::InvalidHeader(raw_name.clone()))?;
        let mut value = HeaderValue::from_str(raw_value)
            .map_err(|_| McpError::InvalidHeader(name.as_str().to_string()))?;
        value.set_sensitive(true);
        if let Some(existing) = headers.get(&name) {
            if existing != value {
                return Err(McpError::ConflictingHeader(name.as_str().to_string()));
            }
            continue;
        }
        headers.insert(name, value);
    }
    Ok(headers)
}

fn is_native_header(name: &HeaderName) -> bool {
    name == AUTHORIZATION
        || name == ACCEPT
        || name == CONTENT_TYPE
        || name == CONTENT_LENGTH
        || name == HOST
        || name == CONNECTION
        || name == TRANSFER_ENCODING
        || name == TE
        || name == TRAILER
        || name == UPGRADE
        || name.as_str().eq_ignore_ascii_case("mcp-session-id")
        || name.as_str().eq_ignore_ascii_case("mcp-protocol-version")
        || name.as_str().eq_ignore_ascii_case("last-event-id")
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
/// Same-server duplicates, oversized schemas, and excessive tool counts fail
/// the whole catalog so attach cannot publish a partial registry.
pub fn map_registry(
    server_id: &str,
    tools: Vec<(String, Option<String>, serde_json::Value)>,
) -> Result<Vec<RegistryEntry>, McpError> {
    if tools.len() > TOOLS_CAP {
        return Err(McpError::CatalogLimited);
    }
    let mut entries = Vec::with_capacity(tools.len());
    let mut seen = HashSet::new();
    for (tool, description, schema) in tools {
        if server_id.is_empty()
            || server_id.len() > ORIGINAL_NAME_BYTES_CAP
            || tool.is_empty()
            || tool.len() > ORIGINAL_NAME_BYTES_CAP
        {
            return Err(CatalogError::InvalidIdentity.into());
        }
        if description
            .as_ref()
            .is_some_and(|description| description.len() > DESCRIPTION_BYTES_CAP)
        {
            return Err(CatalogError::DescriptionTooLarge(tool).into());
        }
        let namespaced = provider_wire_name(server_id, &tool);
        validate_catalog_tool(&mut seen, &tool, &schema)?;
        entries.push(RegistryEntry {
            namespaced,
            server: server_id.to_string(),
            tool,
            description,
            input_schema: schema,
        });
    }
    Ok(entries)
}

/// Merge registries across servers with `__2`-style collision safety.
pub fn merge_registries(
    registries: Vec<Vec<RegistryEntry>>,
) -> Result<Vec<RegistryEntry>, McpError> {
    let mut out = Vec::new();
    let mut taken = std::collections::HashSet::new();
    let mut metadata_bytes = 0usize;
    for entries in registries {
        for mut entry in entries {
            if out.len() >= GENERATION_TOOLS_CAP {
                return Err(McpError::CatalogLimited);
            }
            metadata_bytes = metadata_bytes
                .saturating_add(
                    serde_json::to_vec(&entry.input_schema).map_or(usize::MAX, |v| v.len()),
                )
                .saturating_add(entry.description.as_ref().map_or(0, String::len));
            if metadata_bytes > GENERATION_CATALOG_BYTES_CAP {
                return Err(McpError::CatalogLimited);
            }
            let base = entry.namespaced.clone();
            let mut candidate = base.clone();
            let mut suffix = 2u32;
            while !taken.insert(candidate.clone()) {
                let ending = format!("__{suffix}");
                let keep = WIRE_NAME_BYTES_CAP.saturating_sub(ending.len());
                candidate = format!("{}{}", &base[..base.len().min(keep)], ending);
                suffix += 1;
            }
            entry.namespaced = candidate;
            out.push(entry);
        }
    }
    Ok(out)
}

fn provider_wire_name(server: &str, tool: &str) -> String {
    let original = format!("{server}__{tool}");
    if original.len() <= WIRE_NAME_BYTES_CAP
        && original
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return original;
    }
    let digest = format!("{:x}", Sha256::digest(original.as_bytes()));
    let suffix = format!("__{}", &digest[..12]);
    let keep = WIRE_NAME_BYTES_CAP - suffix.len();
    let mut prefix = original
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-') {
                byte
            } else {
                b'_'
            }
        })
        .take(keep)
        .collect::<Vec<_>>();
    if prefix.is_empty() {
        prefix.extend_from_slice(b"mcp");
    }
    format!(
        "{}{}",
        String::from_utf8(prefix).expect("ASCII wire name"),
        suffix
    )
}

/// Search arguments for the `codex_web` search tool.
pub fn search_args(query: &str, response_length: Option<&str>) -> serde_json::Value {
    let mut args = serde_json::Map::new();
    args.insert(
        "query".to_string(),
        serde_json::Value::String(query.to_string()),
    );
    if let Some(response_length) = response_length {
        args.insert(
            "response_length".to_string(),
            serde_json::Value::from(response_length),
        );
    }
    serde_json::Value::Object(args)
}

#[derive(Clone, Default)]
struct ClientEvents {
    tools_changed: Arc<AtomicBool>,
}

impl rmcp::handler::client::ClientHandler for ClientEvents {
    fn on_tool_list_changed(
        &self,
        _context: rmcp::service::NotificationContext<rmcp::service::RoleClient>,
    ) -> impl std::future::Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        self.tools_changed.store(true, Ordering::Release);
        std::future::ready(())
    }
}

type McpPeer = rmcp::service::Peer<rmcp::service::RoleClient>;
type McpRunning = rmcp::service::RunningService<rmcp::service::RoleClient, ClientEvents>;

#[path = "mcp_http_lifecycle.rs"]
mod lifecycle;

/// Connected remote client with explicit strict `codex_web` or generic profile.
pub struct CodexWebClient {
    peer: McpPeer,
    running: McpRunning,
    timeout: Duration,
    tools_changed: Arc<AtomicBool>,
    cleanup: lifecycle::Cleanup,
    secrets: Vec<String>,
    instructions: Option<String>,
}

impl CodexWebClient {
    /// Connect: pre-dial SSRF guard, exact-URL handshake, 2025-11-25 required.
    pub async fn connect(config: &CodexWebConfig) -> Result<Self, McpError> {
        Self::connect_cancellable(config, true, &AtomicBool::new(false)).await
    }

    /// Generic streamable HTTP: use SDK protocol negotiation and optional
    /// configured bearer. No OAuth or URL rewriting is introduced.
    pub async fn connect_remote(config: &CodexWebConfig) -> Result<Self, McpError> {
        Self::connect_cancellable(config, false, &AtomicBool::new(false)).await
    }

    /// Cancellable attach with bounded HTTP-worker ownership cleanup before
    /// returning a rejection. `codex_web` selects its explicit strict profile.
    pub async fn connect_cancellable(
        config: &CodexWebConfig,
        codex_web: bool,
        cancel: &AtomicBool,
    ) -> Result<Self, McpError> {
        Self::connect_redacted(config, codex_web, cancel, &[]).await
    }

    pub(crate) async fn connect_redacted(
        config: &CodexWebConfig,
        codex_web: bool,
        cancel: &AtomicBool,
        redactions: &[String],
    ) -> Result<Self, McpError> {
        config.validate_profile(codex_web)?;
        let (host, port) = split_host_port(&config.url)?;
        tokio::select! {
            biased;
            () = crate::provider::wait_cancel(cancel) => return Err(McpError::Cancelled),
            result = crate::webfetch::check_host(&host, port, config.allow_private) => result,
        }
        .map_err(|e| match e {
            crate::webfetch::FetchError::PrivateHost => McpError::PrivateHost,
            _ => McpError::Dns,
        })?;

        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent(crate::USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| McpError::Transport)?;
        let custom_headers = config
            .custom_headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<HashMap<_, _>>();
        let mut transport_config =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                config.url.as_str(),
            )
            .custom_headers(custom_headers)
            .control_request_timeout(Duration::from_secs(5))
            .max_sse_event_size(crate::webfetch::BODY_CAP_BYTES)
            .reinit_on_expired_session(false)
            .max_concurrent_requests(4);
        if !config.bearer.is_empty() {
            transport_config = transport_config.auth_header(config.bearer.as_str());
        }
        let (http, mut cleanup) = lifecycle::Http::new(http);
        let transport =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransport::with_client(
                http,
                transport_config,
            );
        let events = ClientEvents::default();
        let tools_changed = events.tools_changed.clone();
        let handshake = tokio::select! {
            biased;
            () = crate::provider::wait_cancel(cancel) => Err(McpError::Cancelled),
            result = tokio::time::timeout(config.timeout, rmcp::service::serve_client(events, transport)) => {
                result.map_err(|_| McpError::Deadline).and_then(|result| result.map_err(|e| classify_handshake(&e)))
            }
        };
        let mut running = match handshake {
            Ok(running) => running,
            Err(error) => {
                cleanup.close().await?;
                return Err(error);
            }
        };
        // The negotiated version must be exactly 2025-11-25 for codex_web.
        let version = running
            .peer()
            .peer_info()
            .map(|info| info.protocol_version.to_string())
            .unwrap_or_default();
        if codex_web && version != MCP_VERSION {
            let closed = close_running(&mut running).await;
            let cleaned = cleanup.close().await;
            cleaned?;
            closed.map_err(|_| McpError::CleanupFailed)?;
            return Err(McpError::ProtocolMismatch);
        }
        let peer = running.peer().clone();
        let mut secrets = vec![config.url.clone(), config.bearer.clone()];
        secrets.extend_from_slice(redactions);
        secrets.extend(
            config
                .custom_headers
                .values()
                .filter_map(|v| v.to_str().ok())
                .map(str::to_string),
        );
        let instructions = peer.peer_info().and_then(|info| {
            crate::mcp_result::instructions(info.instructions.as_deref(), &secrets)
        });
        Ok(Self {
            peer,
            running,
            timeout: config.timeout,
            tools_changed,
            cleanup,
            secrets,
            instructions,
        })
    }

    /// Bounded, configured-value-redacted server guidance from initialize.
    pub fn instructions(&self) -> Option<&str> {
        self.instructions.as_deref()
    }

    /// Atomically claim a pending tools/list refresh. A notification that
    /// arrives during the relist stays claimed for the next turn.
    pub fn claim_catalog_changed(&self) -> bool {
        self.tools_changed.swap(false, Ordering::AcqRel)
    }

    /// Restore a claimed refresh after a failed relist so it is retried.
    pub fn restore_catalog_changed(&self) {
        self.tools_changed.store(true, Ordering::Release);
    }

    /// Close the rmcp service and wait for transport cleanup for a bounded time.
    ///
    /// A timed-out or panicked service task is a cleanup failure, never success.
    pub async fn close(mut self) -> Result<(), McpError> {
        let result = close_running(&mut self.running).await;
        self.cleanup.close().await?;
        result
    }

    /// List tools with a bounded cursor loop (paginated, capped).
    pub async fn list_tools(&self, cancel: &AtomicBool) -> Result<Vec<RemoteTool>, McpError> {
        let tools = self
            .run_cancel(cancel, self.peer.list_all_tools_bounded(), "list")
            .await?;
        let tools = tools
            .into_iter()
            .map(|tool| RemoteTool {
                name: tool.name.to_string(),
                description: tool.description.map(|d| d.to_string()),
                input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
            })
            .collect::<Vec<_>>();
        validate_remote_catalog(&tools)?;
        Ok(tools)
    }

    /// Call the `search` tool; `isError` surfaces as failure, never success.
    pub async fn search(
        &self,
        query: &str,
        response_length: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<String, McpError> {
        let args = search_args(query, response_length);
        self.call_tool("search", args, cancel).await
    }

    /// Call any mapped tool by its exact server-side name.
    pub async fn call_tool(
        &self,
        tool: &str,
        arguments: serde_json::Value,
        cancel: &AtomicBool,
    ) -> Result<String, McpError> {
        let arguments = arguments
            .as_object()
            .cloned()
            .ok_or(McpError::InvalidArguments)?;
        let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
        params.arguments = Some(arguments);
        let request =
            rmcp::model::ClientRequest::CallToolRequest(rmcp::model::CallToolRequest::new(params));
        let deadline = tokio::time::Instant::now() + self.timeout;
        let mut handle = tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => return Err(McpError::Cancelled),
            result = tokio::time::timeout_at(deadline, self.peer.send_cancellable_request(
                request, rmcp::service::PeerRequestOptions::no_options()
            )) => result.map_err(|_| McpError::Deadline)?.map_err(|_| McpError::Transport)?,
        };
        let outcome = tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => None,
            result = tokio::time::timeout_at(deadline, &mut handle.rx) => Some(result),
        };
        let outcome = match outcome {
            None | Some(Err(_)) => {
                let reason = if outcome.is_none() {
                    McpError::Cancelled
                } else {
                    McpError::Deadline
                };
                tokio::time::timeout(CLOSE_TIMEOUT, handle.cancel(Some("request stopped".into())))
                    .await
                    .map_err(|_| McpError::CleanupFailed)?
                    .map_err(|_| McpError::CleanupFailed)?;
                return Err(reason);
            }
            Some(Ok(Ok(Ok(rmcp::model::ServerResult::CallToolResult(result))))) => {
                rmcp::model::CallToolResponse::Complete(result)
            }
            Some(Ok(Ok(Ok(_)))) => return Err(McpError::UnsupportedResult),
            Some(Ok(Ok(Err(error)))) => {
                return Err(crate::mcp_result::rpc_failure(&error)
                    .map_or(McpError::Transport, McpError::ToolFailedDetail));
            }
            Some(Ok(Err(_))) => return Err(McpError::Transport),
        };
        match outcome {
            rmcp::model::CallToolResponse::Complete(result) => {
                crate::mcp_result::project(result, &self.secrets).map_err(|error| match error {
                    crate::mcp_result::ResultError::Failed(Some(detail)) => {
                        McpError::ToolFailedDetail(detail)
                    }
                    crate::mcp_result::ResultError::Failed(None) => McpError::ToolFailed,
                    crate::mcp_result::ResultError::Unsupported => McpError::UnsupportedResult,
                    crate::mcp_result::ResultError::BadResult => McpError::BadResult,
                })
            }
            _ => Err(McpError::UnsupportedResult),
        }
    }

    /// Run a catalog future with timeout and cancellation. Calls use an rmcp
    /// RequestHandle instead, so their request id is explicitly cancelled.
    async fn run_cancel<T>(
        &self,
        cancel: &AtomicBool,
        future: impl std::future::Future<Output = Result<T, McpError>> + Send,
        _op: &str,
    ) -> Result<T, McpError> {
        tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => Err(McpError::Cancelled),
            result = tokio::time::timeout(self.timeout, future) => match result {
                Err(_) => Err(McpError::Deadline),
                Ok(Err(error)) => Err(error),
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
    use rmcp::service::ClientInitializeError;
    use rmcp::transport::streamable_http_client::StreamableHttpError;
    match error {
        ClientInitializeError::NoCompatibleProtocolVersion { .. } => McpError::ProtocolMismatch,
        ClientInitializeError::Cancelled => McpError::Cancelled,
        ClientInitializeError::TransportError { error, .. } => {
            match error
                .error
                .downcast_ref::<StreamableHttpError<reqwest::Error>>()
            {
                Some(StreamableHttpError::AuthRequired(_)) => McpError::Unauthorized,
                Some(StreamableHttpError::InsufficientScope(_)) => McpError::Forbidden,
                // Pinned rmcp loses HTTP status without WWW-Authenticate in
                // this variant. Read only its fixed status prefix, never search
                // or display the arbitrary server body that follows it.
                Some(StreamableHttpError::UnexpectedServerResponse(message))
                    if message.starts_with("HTTP 401 ") =>
                {
                    McpError::Unauthorized
                }
                Some(StreamableHttpError::UnexpectedServerResponse(message))
                    if message.starts_with("HTTP 403 ") =>
                {
                    McpError::Forbidden
                }
                Some(StreamableHttpError::Client(error)) => match error.status() {
                    Some(reqwest::StatusCode::UNAUTHORIZED) => McpError::Unauthorized,
                    Some(reqwest::StatusCode::FORBIDDEN) => McpError::Forbidden,
                    _ if error.is_connect() => McpError::Connect,
                    _ if error.is_timeout() => McpError::Deadline,
                    _ => McpError::Transport,
                },
                _ => McpError::Transport,
            }
        }
        _ => McpError::Transport,
    }
}

async fn close_running(running: &mut McpRunning) -> Result<(), McpError> {
    match running.close_with_timeout(CLOSE_TIMEOUT).await {
        Ok(Some(rmcp::service::QuitReason::Closed | rmcp::service::QuitReason::Cancelled)) => {
            Ok(())
        }
        Ok(Some(_)) | Err(_) => Err(McpError::Transport),
        Ok(None) => Err(McpError::Deadline),
    }
}

fn split_host_port(url: &str) -> Result<(String, u16), McpError> {
    let url = reqwest::Url::parse(url).map_err(|_| McpError::InvalidConfig)?;
    let host = url.host_str().ok_or(McpError::InvalidConfig)?.to_string();
    let port = url.port_or_known_default().ok_or(McpError::InvalidConfig)?;
    Ok((host, port))
}

/// Bounded cursor pagination without the unbounded SDK helper.
trait BoundedList {
    /// List all tools across at most [`MAX_LIST_PAGES`] pages.
    fn list_all_tools_bounded(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<rmcp::model::Tool>, McpError>> + Send;
}

impl BoundedList for McpPeer {
    async fn list_all_tools_bounded(&self) -> Result<Vec<rmcp::model::Tool>, McpError> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        for page_index in 0..MAX_LIST_PAGES {
            let params = rmcp::model::PaginatedRequestParams::default().with_cursor(cursor);
            let page = self
                .list_tools(Some(params))
                .await
                .map_err(|_| McpError::Transport)?;
            if tools.len() + page.tools.len() > TOOLS_CAP {
                return Err(McpError::CatalogLimited);
            }
            tools.extend(page.tools);
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(tools);
            }
            if page_index + 1 == MAX_LIST_PAGES {
                return Err(McpError::CatalogLimited);
            }
        }
        Err(McpError::CatalogLimited)
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

fn validate_remote_catalog(tools: &[RemoteTool]) -> Result<(), McpError> {
    let mut seen = HashSet::new();
    for tool in tools {
        validate_catalog_tool(&mut seen, &tool.name, &tool.input_schema)?;
    }
    Ok(())
}

fn validate_catalog_tool(
    seen: &mut HashSet<String>,
    tool: &str,
    schema: &serde_json::Value,
) -> Result<(), McpError> {
    if !seen.insert(tool.to_string()) {
        return Err(CatalogError::DuplicateTool(tool.to_string()).into());
    }
    if serde_json::to_vec(schema)
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
        > SCHEMA_BYTES_CAP
    {
        return Err(CatalogError::SchemaTooLarge(tool.to_string()).into());
    }
    Ok(())
}
