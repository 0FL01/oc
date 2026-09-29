//! Generation-owned MCP request leases, redaction and unknown-effect retirement.
//! Resource supervision/status/control belongs to the cohesive lifecycle slice.

use super::*;

mod lifecycle;
pub(super) use lifecycle::McpOwner;
pub(super) use lifecycle::connection_diagnostic;

enum AttachedServer {
    Remote(CodexWebClient),
    Stdio(StdioClient),
}

/// Immutable request view of the same generation-owned clients. New connections
/// and removals only affect later views; no long-held resource mutex in a turn.
#[derive(Clone)]
pub(super) struct McpGeneration {
    pub(super) publication: u64,
    servers: Vec<AttachedMcp>,
    pub(super) entries: Vec<mcp_remote::RegistryEntry>,
    degraded: Vec<RuntimeError>,
    poisoned: Arc<AtomicBool>,
    remote_unknown: Arc<AtomicBool>,
    cleanup_error: Arc<AtomicBool>,
    wake: Arc<tokio::sync::Notify>,
}

impl McpGeneration {
    pub(super) fn empty(publication: u64, wake: Arc<tokio::sync::Notify>) -> Self {
        Self {
            publication,
            servers: Vec::new(),
            entries: Vec::new(),
            degraded: Vec::new(),
            poisoned: Arc::new(AtomicBool::new(false)),
            remote_unknown: Arc::new(AtomicBool::new(false)),
            cleanup_error: Arc::new(AtomicBool::new(false)),
            wake,
        }
    }
}

impl Drop for McpGeneration {
    fn drop(&mut self) {
        // Wake after releasing the client leases. Waking first can make the
        // parallel owner observe a still-live lease and strand its retirement.
        self.servers.clear();
        self.wake.notify_one();
    }
}

#[derive(Clone)]
struct AttachedMcp {
    server_id: String,
    client: Arc<AttachedServer>,
    registry: Vec<mcp_remote::RegistryEntry>,
}

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

fn record_degradation(degraded: &mut Vec<RuntimeError>, identity: &str, mut error: RuntimeError) {
    if let RuntimeError::McpAttach { server, .. } = &mut error {
        // The admitted registry key is authority; never rehash a presentation
        // string or accept a factory error's claimed identity as that key.
        *server = safe_server_id(identity);
        degraded.retain(|existing| !matches!(existing, RuntimeError::McpAttach { server: known, .. } if known == server));
    }
    degraded.push(error);
}

fn clear_degradation(degraded: &mut Vec<RuntimeError>, server: &str) {
    let server = safe_server_id(server);
    degraded.retain(|existing| !matches!(existing, RuntimeError::McpAttach { server: known, .. } if known == &server));
}

/// Permission-filtered ephemeral initialize guidance; never durable history.
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
            let instructions = match server.client.as_ref() {
                AttachedServer::Remote(client) => client.instructions(),
                AttachedServer::Stdio(client) => client.instructions(),
            }?;
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
    // Shared presentation only: registry/control/wire keys remain owner-local.
    crate::config::mcp::safe_identity(server)
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

impl Runtime<'_> {
    /// Dispatch after the common permission and durable intent path. The lease
    /// and database owner are unchanged; a control cannot replay an unknown call.
    pub(super) async fn execute_mcp(
        &self,
        call: &crate::tools::ToolCall,
        op: &str,
        turn: &str,
        attached: &McpGeneration,
        cancel: &AtomicBool,
        native_result: &mut Option<crate::mcp_result::McpToolOutput>,
    ) -> (&'static str, String) {
        let Some(entry) = attached
            .entries
            .iter()
            .find(|entry| entry.namespaced == call.name)
        else {
            return ("failed", format!("error: unknown mcp tool {}", call.name));
        };
        let Some(server) = attached
            .servers
            .iter()
            .find(|server| server.server_id == entry.server)
        else {
            return (
                "failed",
                format!("error: unknown mcp server {}", entry.server),
            );
        };
        if matches!(server.client.as_ref(), AttachedServer::Remote(_))
            && attached.remote_unknown.load(Ordering::SeqCst)
        {
            return (
                "failed",
                "error: mcp outcome unknown; remote retry unsafe".into(),
            );
        }
        let mut lease = McpCallLease {
            generation: attached,
            db: self.db,
            op,
            turn,
            remote: matches!(server.client.as_ref(), AttachedServer::Remote(_)),
            armed: true,
        };
        let result = match server.client.as_ref() {
            AttachedServer::Remote(client) => client
                .call_tool_rich(&entry.tool, call.arguments.clone(), cancel)
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
                    |output| {
                        let text = output.display().to_owned();
                        if output.needs_native_log() {
                            *native_result = Some(output);
                        }
                        ("completed", text)
                    },
                ),
            AttachedServer::Stdio(client) => client
                .call_tool_rich(&entry.tool, call.arguments.clone(), cancel)
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
                    |output| {
                        let text = output.display().to_owned();
                        if output.needs_native_log() {
                            *native_result = Some(output);
                        }
                        ("completed", text)
                    },
                ),
        };
        lease.armed = false;
        result
    }
}
