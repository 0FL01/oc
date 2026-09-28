//! Generation-owned MCP attach, calls, close/degradation and unknown-effect retirement.

use super::*;

/// Attached MCP server: remote or stdio behind one call shape.
enum AttachedServer {
    Remote(CodexWebClient),
    Stdio(StdioClient),
}

/// One Location/config generation owns connected clients and its exact dispatch map.
///
/// `degraded` records enabled servers that could not be attached (or whose
/// catalog refresh failed) as sanitized `McpAttach` notices. They publish no
/// tools and never abort the turn: upstream opencode v2.0.12 keeps a per-server
/// `failed` status and continues the session.
pub(super) struct McpGeneration {
    pub(super) publication: u64,
    pub(super) servers: Vec<AttachedMcp>,
    pub(super) entries: Vec<mcp_remote::RegistryEntry>,
    pub(super) degraded: Vec<RuntimeError>,
    /// An in-flight call has no proven result: this entire generation is retired.
    pub(super) poisoned: AtomicBool,
    /// A remote owner cannot prove its server stopped merely by closing HTTP.
    pub(super) remote_unknown: AtomicBool,
    /// The request-specific cancellation notification could not be confirmed.
    pub(super) cleanup_error: AtomicBool,
}

/// If an owning turn future is dropped in the middle of an MCP call, the
/// request result is unknown. A later turn must retire the old generation.
struct McpCallLease<'a> {
    generation: &'a McpGeneration,
    db: &'a Db,
    op: &'a str,
    turn: &'a str,
    remote: bool,
    armed: bool,
}

impl Drop for McpCallLease<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.generation.poisoned.store(true, Ordering::SeqCst);
            if self.remote {
                self.generation.remote_unknown.store(true, Ordering::SeqCst);
            }
            if self
                .db
                .mark_dropped_mcp_call_unknown(self.op, self.turn)
                .is_err()
            {
                self.generation.cleanup_error.store(true, Ordering::SeqCst);
            }
        }
    }
}

impl McpGeneration {
    /// Sanitized per-server degradation notices (server id, stage, safe code).
    pub(super) fn warnings(&self) -> Vec<String> {
        self.degraded.iter().map(ToString::to_string).collect()
    }

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
                Ok(registry) => {
                    replacements[index] = Some(registry);
                    clear_degradation(&mut self.degraded, &server.server_id);
                }
                Err(error) => {
                    // Upstream ignores a failed relist: the previous catalog
                    // stays published and the server is reported as degraded.
                    self.restore_dirty(&claims);
                    record_degradation(&mut self.degraded, error);
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
pub(super) async fn close_generation(generation: McpGeneration) -> Result<(), RuntimeError> {
    match tokio::spawn(generation.close()).await {
        Ok(result) => result,
        Err(_) => Err(RuntimeError::McpShutdown),
    }
}

/// Record one server's degradation, replacing any earlier notice for it.
///
/// Only sanitized `McpAttach` notices are recorded; any other error stays fatal
/// at the call site.
fn record_degradation(degraded: &mut Vec<RuntimeError>, error: RuntimeError) {
    if let RuntimeError::McpAttach { server, .. } = &error {
        clear_degradation(degraded, server);
    }
    degraded.push(error);
}

/// Drop the recorded degradation of a server that is healthy again.
fn clear_degradation(degraded: &mut Vec<RuntimeError>, server: &str) {
    degraded.retain(|existing| match existing {
        RuntimeError::McpAttach { server: known, .. } => known != server,
        _ => true,
    });
}

/// Server id + client + per-server base registry.
pub(super) struct AttachedMcp {
    server_id: String,
    client: AttachedServer,
    registry: Vec<mcp_remote::RegistryEntry>,
}

/// Initialize guidance belongs to the ephemeral request, never TurnLog/history.
/// A server must own at least one attached tool permitted in this exact lane.
pub(super) fn mcp_instruction_input(
    attached: &McpGeneration,
    policy: &RuntimePolicy<'_>,
) -> Vec<InputItem> {
    attached
        .servers
        .iter()
        .filter_map(|server| {
            let tools: Vec<&str> = attached
                .entries
                .iter()
                .filter(|entry| {
                    entry.server == server.server_id && policy.check(&entry.namespaced).is_ok()
                })
                .map(|entry| entry.namespaced.as_str())
                .collect();
            if tools.is_empty() {
                return None;
            }
            let instructions = match &server.client {
                AttachedServer::Remote(client) => client.instructions(),
                AttachedServer::Stdio(client) => client.instructions(),
            };
            let instructions = instructions?;
            Some(InputItem::message(
                InputRole::Developer,
                format!(
                    "MCP server guidance for permitted tools [{}]:\n{instructions}",
                    tools.join(", ")
                ),
            ))
        })
        .collect()
}

fn mcp_redactions(config: &Generation, parent_env: &BTreeMap<String, String>) -> Vec<String> {
    let mut secrets: Vec<String> = parent_env
        .iter()
        .filter(|(name, _)| {
            let name = name.to_ascii_uppercase();
            [
                "KEY",
                "TOKEN",
                "SECRET",
                "PASSWORD",
                "CREDENTIAL",
                "AUTH",
                "COOKIE",
                "BEARER",
            ]
            .iter()
            .any(|part| name.contains(part))
        })
        .map(|(_, value)| value.clone())
        .collect();
    for provider in config.providers.values() {
        secrets.extend([
            provider.options.api_key.clone(),
            provider.options.base_url.clone(),
        ]);
        secrets.extend(provider.options.headers.values().cloned());
    }
    for entry in config.mcp.values() {
        secrets.extend(entry.environment.values().cloned());
        secrets.extend(entry.blocked_inherited_values.iter().cloned());
        secrets.extend(entry.url.iter().cloned());
        secrets.extend(entry.headers.values().cloned());
        // A header may be reflected as its token without the Bearer prefix.
        secrets.extend(
            entry
                .headers
                .values()
                .filter_map(|v| v.split_once(' ').map(|(_, token)| token.to_string())),
        );
    }
    secrets
}

fn remote_mcp_failure(error: McpError, cancel: &AtomicBool) -> (&'static str, String) {
    if cancel.load(Ordering::Relaxed) || error == McpError::Cancelled {
        return ("cancelled", "error: cancelled".into());
    }
    let message = match error {
        McpError::ToolFailedDetail(detail) => return ("failed", format!("error: mcp {detail}")),
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
        StdioError::ToolFailedDetail(detail) => return ("failed", format!("error: mcp {detail}")),
        StdioError::ToolFailed => "error: mcp tool reported failure",
        StdioError::UnsupportedModality | StdioError::BadResult => "error: unsupported mcp result",
        StdioError::NonObjectArguments => "error: invalid mcp arguments",
        _ => "error: mcp transport failure",
    };
    ("failed", message.into())
}

fn remote_attach_error(server: &str, error: McpError) -> RuntimeError {
    remote_attach_error_at(server, "tools-list", error)
}

fn stdio_attach_error(server: &str, error: StdioError) -> RuntimeError {
    stdio_attach_error_at(server, "tools-list", error)
}

fn safe_server_id(server: &str) -> String {
    server
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn remote_attach_error_at(server: &str, stage: &'static str, error: McpError) -> RuntimeError {
    let (stage, safe_code, retryable) = match error {
        McpError::Cancelled => return RuntimeError::Cancelled,
        McpError::InvalidConfig => ("config", "invalid_config", false),
        McpError::InvalidHeader(_) => ("config", "invalid_header", false),
        McpError::ConflictingHeader(name) if name == "authorization" => {
            ("config", "authorization_header_conflict", false)
        }
        McpError::ConflictingHeader(_) => ("config", "header_conflict", false),
        McpError::PrivateHost => ("DNS", "private_host", false),
        McpError::Dns => ("DNS", "resolution_failed", true),
        McpError::Connect => ("connect", "connection_failed", true),
        McpError::CleanupFailed => ("cleanup", "cleanup_failed", false),
        McpError::Unauthorized => (stage, "unauthorized", false),
        McpError::Forbidden => (stage, "forbidden", false),
        McpError::Deadline => (stage, "deadline", true),
        McpError::Transport => (stage, "transport", true),
        McpError::ProtocolMismatch => (stage, "protocol_mismatch", false),
        McpError::CatalogLimited => (stage, "catalog_limit", false),
        McpError::Catalog(_) => (stage, "invalid_catalog", false),
        McpError::ToolFailed | McpError::ToolFailedDetail(_) => (stage, "tool_failed", false),
        McpError::InvalidArguments => (stage, "invalid_arguments", false),
        McpError::BadResult => (stage, "bad_result", false),
        McpError::UnsupportedResult => (stage, "unsupported_result", false),
    };
    RuntimeError::McpAttach {
        server: safe_server_id(server),
        stage,
        safe_code,
        retryable,
    }
}

fn stdio_attach_error_at(server: &str, stage: &'static str, error: StdioError) -> RuntimeError {
    let (stage, safe_code, retryable) = match error {
        StdioError::Cancelled => return RuntimeError::Cancelled,
        StdioError::InvalidConfig => ("config", "invalid_config", false),
        StdioError::Disabled => ("config", "disabled", false),
        StdioError::Spawn => ("spawn", "spawn_failed", false),
        StdioError::Deadline => (stage, "deadline", true),
        StdioError::Transport => (stage, "transport", true),
        StdioError::ProtocolMismatch => (stage, "protocol_mismatch", false),
        StdioError::CleanupFailed => ("cleanup", "cleanup_failed", false),
        StdioError::CatalogLimited => (stage, "catalog_limit", false),
        StdioError::ToolFailed | StdioError::ToolFailedDetail(_) => (stage, "tool_failed", false),
        StdioError::NonObjectArguments => (stage, "invalid_arguments", false),
        StdioError::UnsupportedModality => (stage, "unsupported_result", false),
        StdioError::BadResult => (stage, "bad_result", false),
    };
    RuntimeError::McpAttach {
        server: safe_server_id(server),
        stage,
        safe_code,
        retryable,
    }
}

impl<'a> Runtime<'a> {
    /// Dispatch only after the common permission and durable intent path.
    pub(super) async fn execute_mcp(
        &self,
        call: &crate::tools::ToolCall,
        op: &str,
        turn: &str,
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
        let mut lease = McpCallLease {
            generation: attached,
            db: self.db,
            op,
            turn,
            remote: matches!(&server.client, AttachedServer::Remote(_)),
            armed: true,
        };
        let result = match &server.client {
            AttachedServer::Remote(client) => client
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| {
                        if matches!(
                            error,
                            McpError::Cancelled
                                | McpError::Deadline
                                | McpError::Transport
                                | McpError::CleanupFailed
                                | McpError::UnsupportedResult
                                | McpError::BadResult
                        ) || cancel.load(Ordering::Relaxed)
                        {
                            attached.poisoned.store(true, Ordering::SeqCst);
                            attached.remote_unknown.store(true, Ordering::SeqCst);
                            if error == McpError::CleanupFailed {
                                attached.cleanup_error.store(true, Ordering::SeqCst);
                            }
                            return (
                                "unknown",
                                "error: mcp outcome unknown; remote retry unsafe".into(),
                            );
                        }
                        remote_mcp_failure(error, cancel)
                    },
                    |text| ("completed", text),
                ),
            AttachedServer::Stdio(child) => child
                .call_tool(&entry.tool, call.arguments.clone(), cancel)
                .await
                .map_or_else(
                    |error| {
                        if matches!(
                            error,
                            StdioError::Cancelled
                                | StdioError::Deadline
                                | StdioError::Transport
                                | StdioError::CleanupFailed
                                | StdioError::UnsupportedModality
                                | StdioError::BadResult
                        ) || cancel.load(Ordering::Relaxed)
                        {
                            attached.poisoned.store(true, Ordering::SeqCst);
                            if error == StdioError::CleanupFailed {
                                attached.cleanup_error.store(true, Ordering::SeqCst);
                            }
                            return (
                                "unknown",
                                "error: mcp outcome unknown; local owner stopping".into(),
                            );
                        }
                        stdio_mcp_failure(error, cancel)
                    },
                    |text| ("completed", text),
                ),
        };
        lease.armed = false;
        result
    }

    pub(super) async fn retire_poisoned(
        &self,
        slot: &mut Option<McpGeneration>,
    ) -> Result<(), RuntimeError> {
        if !slot
            .as_ref()
            .is_some_and(|g| g.poisoned.load(Ordering::SeqCst))
        {
            return Ok(());
        }
        if slot
            .as_ref()
            .is_some_and(|g| g.remote_unknown.load(Ordering::SeqCst))
        {
            self.mcp_unsafe_retry.store(true, Ordering::SeqCst);
        }
        let cleanup_error = slot
            .as_ref()
            .is_some_and(|g| g.cleanup_error.load(Ordering::SeqCst));
        if cleanup_error {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
        }
        if let Some(generation) = slot.take()
            && let Err(error) = close_generation(generation).await
        {
            self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
            return Err(error);
        }
        if cleanup_error {
            Err(RuntimeError::McpShutdown)
        } else {
            Ok(())
        }
    }

    /// Reuse one connected MCP registry for the whole Location/config generation.
    pub(super) async fn ensure_mcp_generation<'m>(
        &self,
        slot: &'m mut Option<McpGeneration>,
        published: &PublishedGeneration,
        cancel: &AtomicBool,
    ) -> Result<&'m McpGeneration, RuntimeError> {
        self.retire_poisoned(slot).await?;
        if self.mcp_cleanup_failed.load(Ordering::SeqCst) {
            return Err(RuntimeError::McpShutdown);
        }
        if self.mcp_unsafe_retry.load(Ordering::SeqCst)
            && published
                .config
                .mcp
                .values()
                .any(|entry| entry.enabled && entry.kind == "remote")
        {
            return Err(RuntimeError::McpAttach {
                server: "generation".into(),
                stage: "call",
                safe_code: "unsafe_retry",
                retryable: false,
            });
        }
        let reusable = slot
            .as_ref()
            .is_some_and(|generation| generation.publication == published.id);
        if !reusable {
            if let Some(old) = slot.take()
                && let Err(error) = close_generation(old).await
            {
                self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                return Err(error);
            }
            *slot = Some(match self.attach_mcp(published, cancel).await {
                Ok(generation) => generation,
                Err(error) => {
                    if error == RuntimeError::McpShutdown {
                        self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                    }
                    return Err(error);
                }
            });
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
        let mut degraded = Vec::new();
        let redactions = mcp_redactions(&published.config, &self.parent_env);
        let mut ids: Vec<&String> = published.config.mcp.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &published.config.mcp[id];
            if let Some(failure) = &entry.failure {
                record_degradation(
                    &mut degraded,
                    RuntimeError::McpAttach {
                        server: failure.service.clone(),
                        stage: failure.stage.as_str(),
                        safe_code: failure.code.as_str(),
                        retryable: false,
                    },
                );
                continue;
            }
            if !entry.enabled {
                continue;
            }
            let result = if entry.kind == "remote" {
                // codex_web has an explicit exact-version/static-bearer contract;
                // other configured remote servers use ordinary SDK negotiation.
                let codex_web = id == "codex_web";
                let config = if codex_web {
                    mcp_remote::CodexWebConfig::from_entry(entry)
                } else {
                    mcp_remote::CodexWebConfig::from_remote_entry(entry)
                }
                .map(|mut config| {
                    config.allow_private = self
                        .parent_env
                        .get("OC_TEST_ALLOW_LOOPBACK")
                        .is_some_and(|value| value == "1");
                    config
                })
                .map_err(|error| remote_attach_error_at(id, "config", error));
                match config {
                    Ok(config) => {
                        let client = CodexWebClient::connect_redacted(
                            &config,
                            codex_web,
                            cancel,
                            &redactions,
                        )
                        .await
                        .map_err(|error| remote_attach_error_at(id, "initialize", error));
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
                        .map_err(|error| stdio_attach_error_at(id, "config", error));
                match config {
                    Ok(config) => {
                        let client = StdioClient::launch_redacted(&config, cancel, &redactions)
                            .await
                            .map_err(|error| stdio_attach_error_at(id, "initialize", error));
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
                Err(RuntimeError::McpAttach {
                    server: safe_server_id(id),
                    stage: "config",
                    safe_code: "unsupported_transport",
                    retryable: false,
                })
            };
            match result {
                Ok((server, registry)) => {
                    attached.push(server);
                    registries.push(registry);
                }
                // Per-server attach failure degrades that server only: upstream
                // opencode v2.0.12 marks the server failed and keeps the turn.
                Err(error @ RuntimeError::McpAttach { .. }) => {
                    if matches!(
                        &error,
                        RuntimeError::McpAttach {
                            stage: "cleanup",
                            ..
                        }
                    ) {
                        self.mcp_cleanup_failed.store(true, Ordering::SeqCst);
                        let cleanup = close_generation(McpGeneration {
                            publication: published.id,
                            servers: attached,
                            entries: Vec::new(),
                            degraded: Vec::new(),
                            poisoned: AtomicBool::new(false),
                            remote_unknown: AtomicBool::new(false),
                            cleanup_error: AtomicBool::new(false),
                        })
                        .await;
                        cleanup?;
                        return Err(RuntimeError::McpShutdown);
                    }
                    record_degradation(&mut degraded, error);
                }
                Err(error) => {
                    let cleanup = close_generation(McpGeneration {
                        publication: published.id,
                        servers: attached,
                        entries: Vec::new(),
                        degraded: Vec::new(),
                        poisoned: AtomicBool::new(false),
                        remote_unknown: AtomicBool::new(false),
                        cleanup_error: AtomicBool::new(false),
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
                degraded,
                poisoned: AtomicBool::new(false),
                remote_unknown: AtomicBool::new(false),
                cleanup_error: AtomicBool::new(false),
            }),
            Err(error) => {
                let cleanup = close_generation(McpGeneration {
                    publication: published.id,
                    servers: attached,
                    entries: Vec::new(),
                    degraded: Vec::new(),
                    poisoned: AtomicBool::new(false),
                    remote_unknown: AtomicBool::new(false),
                    cleanup_error: AtomicBool::new(false),
                })
                .await;
                cleanup?;
                Err(remote_attach_error("generation", error))
            }
        }
    }
}
