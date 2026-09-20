//! Local MCP stdio child lifecycle for T21 (MCP04/MCP05).
//!
//! Spawn from an exact retained argv over rmcp's child-process transport:
//! JSON-RPC on stdout, bounded redacted stderr, TERM→KILL reap ladder,
//! restart from the same config generation. The child environment is
//! minimal (`env_clear` + explicit non-credential extras) and never shares
//! provider/MCP credentials or the runner cwd. `enabled: false` spawns
//! nothing and probes nothing: no process, no Node requirement.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;

/// Default stderr kept per child.
pub const STDERR_CAP_BYTES: usize = 8 * 1024;
/// Grace between SIGTERM and SIGKILL.
pub const TERM_GRACE: Duration = Duration::from_millis(500);
/// Default per-operation timeout.
pub const STDIO_TIMEOUT: Duration = Duration::from_secs(60);
/// Bound for tools/list cursor pages.
pub const MAX_LIST_PAGES: usize = 16;
/// Max tools accepted per server.
pub const TOOLS_CAP: usize = 64;

/// Typed stdio errors (no secrets, no child output contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StdioError {
    /// Misconfiguration (bad argv, credential-named extra env, OAuth/Code Mode).
    #[error("invalid stdio config")]
    InvalidConfig,
    /// `enabled: false`: zero spawn, zero probe.
    #[error("server disabled")]
    Disabled,
    /// Spawn or stdio plumbing failed (kind only).
    #[error("spawn failed")]
    Spawn,
    /// Deadline exceeded (handshake or operation).
    #[error("deadline exceeded")]
    Deadline,
    /// MCP transport or protocol failure (kind only).
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

/// Exact local-server launch parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdioConfig {
    /// Server id for registry namespacing.
    pub server_id: String,
    /// Exact argv, retained verbatim (no shell split, no rewrite).
    pub argv: Vec<String>,
    /// Child cwd; defaults to the system temp dir (never the runner cwd).
    pub cwd: Option<PathBuf>,
    /// Extra child env vars; credential names refused.
    pub extra_env: Vec<(String, String)>,
    /// Secrets redacted from stderr snapshots (values never logged).
    pub secrets: Vec<String>,
    /// Per-operation timeout.
    pub timeout: Duration,
    /// `false` refuses to spawn or probe anything.
    pub enabled: bool,
}

impl StdioConfig {
    /// Validate without spawning. `command` keeps its exact shape here.
    pub fn validate(&self) -> Result<(), StdioError> {
        if !self.enabled {
            return Err(StdioError::Disabled);
        }
        if self.server_id.trim().is_empty()
            || self.argv.is_empty()
            || self
                .argv
                .iter()
                .any(|arg| arg.is_empty() || arg.contains('\0'))
        {
            return Err(StdioError::InvalidConfig);
        }
        for (name, _) in &self.extra_env {
            if is_credential_name(name) {
                return Err(StdioError::InvalidConfig);
            }
        }
        Ok(())
    }

    /// Build from a loaded [`McpEntry`](crate::config::McpEntry) without
    /// spawning. Remote entries, OAuth, and Code Mode are refused.
    pub fn from_entry(
        server_id: &str,
        entry: &crate::config::McpEntry,
    ) -> Result<Self, StdioError> {
        if entry.kind != "local"
            || !entry.enabled
            || entry.oauth
            || entry.codemode == Some(true)
            || entry.command.is_empty()
        {
            if entry.kind == "local" && !entry.enabled {
                return Err(StdioError::Disabled);
            }
            return Err(StdioError::InvalidConfig);
        }
        let config = Self {
            server_id: server_id.to_string(),
            argv: entry.command.clone(),
            cwd: None,
            extra_env: Vec::new(),
            secrets: Vec::new(),
            timeout: Duration::from_millis(entry.timeout.unwrap_or(60_000)),
            enabled: true,
        };
        config.validate()?;
        Ok(config)
    }
}

fn is_credential_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "KEY", "TOKEN", "SECRET", "PASSWORD", "AUTH", "COOKIE", "BEARER",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

/// Bounded stderr snapshot with secret redaction applied at read time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StderrSnapshot {
    /// Kept bytes (prefix), secrets replaced with `***`.
    pub text: String,
    /// True when the child wrote past the cap.
    pub truncated: bool,
}

#[derive(Debug, Default)]
struct StderrLog {
    bytes: Vec<u8>,
    truncated: bool,
}

/// A spawned child before the MCP handshake: raw lifecycle handle.
pub struct SpawnedChild {
    child: tokio::process::Child,
    stderr: Arc<Mutex<StderrLog>>,
    /// Resolved executable path actually exec'd.
    pub resolved_bin: PathBuf,
    /// Exact argv tail passed to the child.
    pub argv_tail: Vec<String>,
}

impl SpawnedChild {
    /// Child pid, if still tracked.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    /// Current bounded stderr snapshot with `secrets` redacted.
    pub fn stderr_snapshot(&self, secrets: &[String]) -> StderrSnapshot {
        snapshot(&self.stderr, secrets)
    }

    /// Reap ladder: close stdin, SIGTERM, grace, SIGKILL, wait.
    pub async fn kill_reap(&mut self) -> Result<std::process::ExitStatus, StdioError> {
        drop(self.child.stdin.take());
        if let Some(pid) = self.child.id() {
            // Best effort: the child may already be gone.
            // SAFETY: kill with SIGTERM to our direct child pid only; ESRCH ignored.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            let waited = poll_exit(&mut self.child, TERM_GRACE).await;
            if !waited {
                self.child.kill().await.map_err(|_| StdioError::Spawn)?;
            }
        }
        self.child.wait().await.map_err(|_| StdioError::Spawn)
    }
}

async fn poll_exit(child: &mut tokio::process::Child, grace: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if start.elapsed() >= grace {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn snapshot(log: &Arc<Mutex<StderrLog>>, secrets: &[String]) -> StderrSnapshot {
    let guard = log.lock().expect("stderr log");
    let mut text = String::from_utf8_lossy(&guard.bytes).to_string();
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret.as_str(), "***");
        }
    }
    StderrSnapshot {
        text,
        truncated: guard.truncated,
    }
}

/// Resolve `argv0` without inheriting PATH: absolute when it names a path,
/// otherwise a read-only parent-PATH lookup. The child never sees PATH.
fn resolve_bin(argv0: &str) -> Result<PathBuf, StdioError> {
    if argv0.contains('/') {
        let path = PathBuf::from(argv0);
        if path.is_file() {
            return Ok(path);
        }
        return Err(StdioError::Spawn);
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(argv0);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(StdioError::Spawn)
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt as _;
    let bytes = path.as_os_str().as_bytes();
    let cstr = match std::ffi::CString::new(bytes) {
        Ok(cstr) => cstr,
        Err(_) => return false,
    };
    // Read-only probe: accessibility for execution, nothing inherited.
    // SAFETY: cstr is a valid NUL-terminated path; access takes no ownership.
    unsafe { libc::access(cstr.as_ptr(), libc::X_OK) == 0 }
}

/// Spawn the child with a minimal environment. No handshake happens here.
pub fn spawn_child(config: &StdioConfig) -> Result<SpawnedChild, StdioError> {
    config.validate()?;
    let resolved_bin = resolve_bin(&config.argv[0])?;
    let mut cmd = tokio::process::Command::new(&resolved_bin);
    cmd.args(&config.argv[1..]);
    cmd.env_clear();
    for (name, value) in &config.extra_env {
        cmd.env(name, value);
    }
    match &config.cwd {
        Some(cwd) => {
            cmd.current_dir(cwd);
        }
        None => {
            cmd.current_dir(std::env::temp_dir());
        }
    }
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // New process group is intentionally *not* requested: the reap ladder
    // addresses the direct child, never the runner group.
    let mut child = cmd.spawn().map_err(|_| StdioError::Spawn)?;
    let stderr = child.stderr.take();
    let log = Arc::new(Mutex::new(StderrLog::default()));
    if let Some(pipe) = stderr {
        let worker = log.clone();
        tokio::spawn(async move {
            drain_capped(pipe, &worker).await;
        });
    }
    Ok(SpawnedChild {
        child,
        stderr: log,
        resolved_bin,
        argv_tail: config.argv[1..].to_vec(),
    })
}

async fn drain_capped(mut pipe: tokio::process::ChildStderr, log: &Arc<Mutex<StderrLog>>) {
    use tokio::io::AsyncReadExt as _;
    let mut chunk = [0u8; 4096];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let mut guard = log.lock().expect("stderr log");
                let room = STDERR_CAP_BYTES.saturating_sub(guard.bytes.len());
                if room == 0 {
                    guard.truncated = true;
                    continue;
                }
                let take = n.min(room);
                guard.bytes.extend_from_slice(&chunk[..take]);
                if take < n {
                    guard.truncated = true;
                }
            }
        }
    }
}

type McpPeer = rmcp::service::Peer<rmcp::service::RoleClient>;
type McpRunning = rmcp::service::RunningService<rmcp::service::RoleClient, ()>;

/// Connected stdio client: one child, one handshake, restartable.
pub struct StdioClient {
    peer: McpPeer,
    running: Option<McpRunning>,
    pid: Option<u32>,
    stderr: Arc<Mutex<StderrLog>>,
    config: StdioConfig,
    generation: u64,
}

impl StdioClient {
    /// Launch the child and complete the MCP handshake.
    pub async fn launch(config: &StdioConfig) -> Result<Self, StdioError> {
        Self::launch_generation(config, 1).await
    }

    async fn launch_generation(config: &StdioConfig, generation: u64) -> Result<Self, StdioError> {
        config.validate()?;
        let resolved_bin = resolve_bin(&config.argv[0])?;
        let mut cmd = tokio::process::Command::new(&resolved_bin);
        cmd.args(&config.argv[1..]);
        cmd.env_clear();
        for (name, value) in &config.extra_env {
            cmd.env(name, value);
        }
        match &config.cwd {
            Some(cwd) => {
                cmd.current_dir(cwd);
            }
            None => {
                cmd.current_dir(std::env::temp_dir());
            }
        }
        let (transport, stderr_pipe) =
            rmcp::transport::child_process::TokioChildProcess::builder(cmd)
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|_| StdioError::Spawn)?;
        let pid = transport.id();
        let log = Arc::new(Mutex::new(StderrLog::default()));
        if let Some(pipe) = stderr_pipe {
            let worker = log.clone();
            tokio::spawn(async move {
                drain_capped(pipe, &worker).await;
            });
        }
        let running =
            tokio::time::timeout(config.timeout, rmcp::service::serve_client((), transport))
                .await
                .map_err(|_| StdioError::Deadline)?
                .map_err(|_| StdioError::Transport)?;
        let peer = running.peer().clone();
        Ok(Self {
            peer,
            pid,
            running: Some(running),
            stderr: log,
            config: config.clone(),
            generation,
        })
    }

    /// Config generation: increments on every restart from the same argv.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Current bounded stderr snapshot with configured secrets redacted.
    pub fn stderr_snapshot(&self) -> StderrSnapshot {
        snapshot(&self.stderr, &self.config.secrets)
    }

    /// List tools with a bounded cursor loop.
    pub async fn list_tools(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<crate::mcp_remote::RemoteTool>, StdioError> {
        let tools = self
            .run_cancel(cancel, bounded_list(&self.peer))
            .await?
            .into_iter()
            .map(|tool| crate::mcp_remote::RemoteTool {
                name: tool.name.to_string(),
                description: tool.description.map(|d| d.to_string()),
                input_schema: serde_json::Value::Object(tool.input_schema.as_ref().clone()),
            })
            .collect();
        Ok(tools)
    }

    /// Call a tool by exact server-side name; `isError` is failure.
    pub async fn call_tool(
        &self,
        tool: &str,
        arguments: serde_json::Value,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<String, StdioError> {
        let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
        params.arguments = arguments.as_object().cloned();
        let outcome = self
            .run_cancel(cancel, self.peer.call_tool_once(params))
            .await?;
        match outcome {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(StdioError::ToolFailed);
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
                    return Err(StdioError::BadResult);
                }
                Ok(text)
            }
            _ => Err(StdioError::Transport),
        }
    }

    /// Shutdown ladder: SIGTERM, grace, then transport cancel (kill
    /// fallback + reap). Only the direct child pid is signalled; the
    /// runner group is never touched.
    pub async fn shutdown(mut self) -> Result<(), StdioError> {
        if let Some(pid) = self.pid {
            // SAFETY: kill with SIGTERM to our direct child pid only; ESRCH ignored.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            tokio::time::sleep(TERM_GRACE).await;
        }
        if let Some(running) = self.running.take() {
            // Drop closes the transport: stdin EOF, kill fallback, reap.
            drop(running);
        }
        Ok(())
    }

    /// Restart from the same retained config: shutdown, respawn, re-handshake.
    pub async fn restart(self) -> Result<Self, StdioError> {
        let generation = self.generation + 1;
        let config = self.config.clone();
        let _ = self.shutdown().await;
        Self::launch_generation(&config, generation).await
    }

    async fn run_cancel<T>(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
        future: impl std::future::Future<Output = Result<T, rmcp::service::ServiceError>> + Send,
    ) -> Result<T, StdioError> {
        tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => Err(StdioError::Cancelled),
            result = tokio::time::timeout(self.config.timeout, future) => match result {
                Err(_) => Err(StdioError::Deadline),
                Ok(Err(_)) => Err(StdioError::Transport),
                Ok(Ok(value)) => Ok(value),
            },
        }
    }
}

async fn wait_cancelled(cancel: &std::sync::atomic::AtomicBool) {
    while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn bounded_list(
    peer: &McpPeer,
) -> Result<Vec<rmcp::model::Tool>, rmcp::service::ServiceError> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_LIST_PAGES {
        let params = rmcp::model::PaginatedRequestParams::default().with_cursor(cursor);
        let page = peer.list_tools(Some(params)).await?;
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
