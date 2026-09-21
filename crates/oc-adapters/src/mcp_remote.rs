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
pub const CLOSE_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// Explicit cancellation closed the request.
    #[error("cancelled")]
    Cancelled,
    /// Tool reported `isError`.
    #[error("tool error")]
    ToolFailed,
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
            .field("url", &self.url)
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
        if self.url.trim().is_empty() || self.bearer.trim().is_empty() {
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
        if entry.kind != "remote" || !entry.enabled || entry.oauth {
            return Err(McpError::InvalidConfig);
        }
        let url = entry.url.clone().ok_or(McpError::InvalidConfig)?;
        let mut headers = normalize_headers(&entry.headers)?;
        let authorization = headers
            .remove(AUTHORIZATION)
            .ok_or(McpError::InvalidConfig)?;
        let raw = authorization
            .to_str()
            .map_err(|_| McpError::InvalidHeader(AUTHORIZATION.as_str().to_string()))?
            .trim();
        let mut fields = raw.split_ascii_whitespace();
        let first = fields.next().unwrap_or_default();
        let bearer = match (fields.next(), fields.next()) {
            (Some(token), None) if first.eq_ignore_ascii_case("bearer") => token,
            (None, None) => first,
            _ => return Err(McpError::InvalidConfig),
        }
        .to_string();
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
        config.validate()?;
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

/// Connected `codex_web` client: distinct connections per method, reused
/// headers, no session requirement.
pub struct CodexWebClient {
    peer: McpPeer,
    running: McpRunning,
    timeout: Duration,
    tools_changed: Arc<AtomicBool>,
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
        let custom_headers = config
            .custom_headers
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<HashMap<_, _>>();
        let transport_config =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(
                config.url.as_str(),
            )
            .auth_header(config.bearer.as_str())
            .custom_headers(custom_headers)
            .control_request_timeout(Duration::from_secs(5))
            .max_sse_event_size(crate::webfetch::BODY_CAP_BYTES)
            .reinit_on_expired_session(false)
            .max_concurrent_requests(4);
        let transport =
            rmcp::transport::streamable_http_client::StreamableHttpClientTransport::with_client(
                http,
                transport_config,
            );
        let events = ClientEvents::default();
        let tools_changed = events.tools_changed.clone();
        let mut running = tokio::time::timeout(
            config.timeout,
            rmcp::service::serve_client(events, transport),
        )
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
            let _ = running.close_with_timeout(CLOSE_TIMEOUT).await;
            return Err(McpError::Transport);
        }
        let peer = running.peer().clone();
        Ok(Self {
            peer,
            running,
            timeout: config.timeout,
            tools_changed,
        })
    }

    /// True after the server requests a tools/list refresh.
    pub fn catalog_changed(&self) -> bool {
        self.tools_changed.load(Ordering::Acquire)
    }

    /// Clear the refresh marker only after a complete validated relist.
    pub fn clear_catalog_changed(&self) {
        self.tools_changed.store(false, Ordering::Release);
    }

    /// Close the rmcp service and wait for transport cleanup for a bounded time.
    pub async fn close(mut self) -> Result<(), McpError> {
        match self.running.close_with_timeout(CLOSE_TIMEOUT).await {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(McpError::Deadline),
            Err(_) => Err(McpError::Transport),
        }
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
        let outcome = self
            .run_cancel(
                cancel,
                async {
                    self.peer
                        .call_tool_once(params)
                        .await
                        .map_err(|_| McpError::Transport)
                },
                "call",
            )
            .await?;
        match outcome {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(McpError::ToolFailed);
                }
                if result.structured_content.is_some() {
                    return Err(McpError::UnsupportedResult);
                }
                if result.content.is_empty() {
                    return Err(McpError::BadResult);
                }
                let mut text = String::new();
                for block in &result.content {
                    match block {
                        rmcp::model::ContentBlock::Text(value) => {
                            if value.text.trim().is_empty() {
                                return Err(McpError::BadResult);
                            }
                            let separator = usize::from(!text.is_empty());
                            if text
                                .len()
                                .saturating_add(separator)
                                .saturating_add(value.text.len())
                                > RESULT_TEXT_BYTES_CAP
                            {
                                return Err(McpError::BadResult);
                            }
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(&value.text);
                        }
                        _ => return Err(McpError::UnsupportedResult),
                    }
                }
                Ok(text)
            }
            _ => Err(McpError::UnsupportedResult),
        }
    }

    /// Run a peer future with timeout + explicit cancellation. Dropping the
    /// future on cancel aborts the request; nothing is retried here.
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
