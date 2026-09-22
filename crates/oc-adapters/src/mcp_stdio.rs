//! Local MCP stdio child lifecycle for T37 (AUD23/AUD24).
//!
//! Spawn from an exact retained argv over rmcp's child-process transport:
//! JSON-RPC on stdout, bounded redacted stderr, TERM→KILL reap ladder,
//! restart from the same config generation. The child environment is
//! minimal (`env_clear` + PATH/HOME/TMPDIR/locale allowlist) and never shares
//! provider/MCP credentials. Each child owns a dedicated process group, so
//! wrapper descendants are cleaned without signalling the runner group.
//! `enabled: false` spawns nothing and probes nothing: no process, no Node
//! requirement.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
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
/// Maximum retained text from one MCP call.
pub const RESULT_TEXT_BYTES_CAP: usize = 1024 * 1024;

const SERVICE_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const STDERR_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

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
    /// Owned process/task teardown did not complete successfully.
    #[error("stdio cleanup failed")]
    CleanupFailed,
    /// Explicit cancellation closed the request.
    #[error("cancelled")]
    Cancelled,
    /// Tool reported `isError`.
    #[error("tool error")]
    ToolFailed,
    /// tools/list exceeded the tool or page bound; no partial catalog returned.
    #[error("tool catalog limit exceeded")]
    CatalogLimited,
    /// Tool arguments must be a JSON object.
    #[error("tool arguments must be an object")]
    NonObjectArguments,
    /// Result uses a modality this text-only adapter cannot expose.
    #[error("unsupported tool result modality")]
    UnsupportedModality,
    /// Malformed tool result shape.
    #[error("bad tool result")]
    BadResult,
}

/// Exact local-server launch parameters.
#[derive(Clone, PartialEq, Eq)]
pub struct StdioConfig {
    /// Server id for registry namespacing.
    pub server_id: String,
    /// Exact argv, retained verbatim (no shell split, no rewrite).
    pub argv: Vec<String>,
    /// Trusted project cwd; manually constructed configs may omit it.
    pub cwd: Option<PathBuf>,
    /// Minimal child env; only PATH/HOME/TMPDIR/LANG/LC_* are accepted.
    pub extra_env: Vec<(String, String)>,
    /// Secrets redacted from stderr snapshots (values never logged).
    pub secrets: Vec<String>,
    /// Per-operation timeout.
    pub timeout: Duration,
    /// `false` refuses to spawn or probe anything.
    pub enabled: bool,
}

impl std::fmt::Debug for StdioConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioConfig")
            .field("server_id", &redact_text(&self.server_id, &self.secrets))
            .field("argv", &format_args!("<redacted:{}>", self.argv.len()))
            .field("cwd", &self.cwd.as_ref().map(|_| "<trusted>"))
            .field(
                "extra_env",
                &format_args!("<redacted:{}>", self.extra_env.len()),
            )
            .field("secrets", &"<redacted>")
            .field("timeout", &self.timeout)
            .field("enabled", &self.enabled)
            .finish()
    }
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
        let mut names = HashSet::new();
        for (name, value) in &self.extra_env {
            if !is_allowed_env_name(name)
                || is_credential_name(name)
                || !names.insert(name)
                || name.as_bytes().contains(&0)
                || value.as_bytes().contains(&0)
            {
                return Err(StdioError::InvalidConfig);
            }
        }
        if self
            .cwd
            .as_deref()
            .is_some_and(|cwd| cwd.as_os_str().as_bytes().contains(&0))
        {
            return Err(StdioError::InvalidConfig);
        }
        Ok(())
    }

    /// Build from a loaded [`McpEntry`](crate::config::McpEntry) without
    /// spawning. `project_cwd` and `parent_env` must come from the trusted
    /// Location generation. Remote entries, OAuth, and Code Mode are refused.
    pub fn from_entry(
        server_id: &str,
        entry: &crate::config::McpEntry,
        project_cwd: &Path,
        parent_env: &BTreeMap<String, String>,
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
        let secrets: Vec<String> = parent_env
            .iter()
            .filter(|(name, value)| is_credential_name(name) && !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let extra_env = parent_env
            .iter()
            .filter(|(name, value)| {
                is_allowed_env_name(name)
                    && !is_credential_name(name)
                    && !secrets
                        .iter()
                        .any(|secret| !secret.is_empty() && value.contains(secret))
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        let config = Self {
            server_id: server_id.to_string(),
            argv: entry.command.clone(),
            cwd: Some(project_cwd.to_path_buf()),
            extra_env,
            secrets,
            timeout: Duration::from_millis(entry.timeout.unwrap_or(60_000)),
            enabled: true,
        };
        config.validate()?;
        Ok(config)
    }
}

fn is_allowed_env_name(name: &str) -> bool {
    matches!(name, "PATH" | "HOME" | "TMPDIR" | "LANG")
        || name
            .strip_prefix("LC_")
            .is_some_and(|suffix| !suffix.is_empty())
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

struct OwnedProcessGroup {
    pgid: Option<libc::pid_t>,
}

impl OwnedProcessGroup {
    fn new(pid: u32) -> Result<Self, StdioError> {
        let pgid = libc::pid_t::try_from(pid).map_err(|_| StdioError::Spawn)?;
        // PID 0/1 and the runner's group are never owned signal targets.
        // SAFETY: getpgrp reads the caller's process-group id without side effects.
        if pgid <= 1 || pgid == unsafe { libc::getpgrp() } {
            return Err(StdioError::Spawn);
        }
        Ok(Self { pgid: Some(pgid) })
    }

    fn id(&self) -> Option<u32> {
        self.pgid.and_then(|pgid| u32::try_from(pgid).ok())
    }

    fn signal(&self, signal: libc::c_int) -> Result<(), StdioError> {
        let Some(pgid) = self.pgid else {
            return Ok(());
        };
        // SAFETY: pgid is a positive child-created group distinct from the
        // runner group; negation targets only that owned process group.
        let result = unsafe { libc::kill(-pgid, signal) };
        if result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(StdioError::Spawn)
        }
    }

    fn is_alive(&self) -> bool {
        let Some(pgid) = self.pgid else {
            return false;
        };
        // SAFETY: signal 0 only probes the owned negative process-group id.
        let result = unsafe { libc::kill(-pgid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    async fn terminate(&self) -> Result<(), StdioError> {
        self.signal(libc::SIGTERM)?;
        let deadline = tokio::time::Instant::now() + TERM_GRACE;
        while self.is_alive() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        if self.is_alive() {
            self.signal(libc::SIGKILL)?;
        }
        Ok(())
    }

    fn disarm(&mut self) {
        self.pgid = None;
    }
}

impl Drop for OwnedProcessGroup {
    fn drop(&mut self) {
        let _ = self.signal(libc::SIGKILL);
    }
}

/// A spawned child before the MCP handshake: raw lifecycle handle.
pub struct SpawnedChild {
    child: tokio::process::Child,
    process_group: OwnedProcessGroup,
    stderr: Arc<Mutex<StderrLog>>,
    stderr_task: Option<tokio::task::JoinHandle<()>>,
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

    /// Dedicated process-group id, equal to the original child pid.
    pub fn process_group_id(&self) -> Option<u32> {
        self.process_group.id()
    }

    /// Current bounded stderr snapshot with `secrets` redacted.
    pub fn stderr_snapshot(&self, secrets: &[String]) -> StderrSnapshot {
        snapshot(&self.stderr, secrets)
    }

    /// Reap ladder: close stdin, SIGTERM, grace, SIGKILL, wait.
    ///
    /// The owned group stays armed (Drop still kills it) until wait succeeds.
    pub async fn kill_reap(&mut self) -> Result<std::process::ExitStatus, StdioError> {
        drop(self.child.stdin.take());
        let (terminated, status) = tokio::join!(
            self.process_group.terminate(),
            tokio::time::timeout(SERVICE_CLOSE_TIMEOUT, self.child.wait()),
        );
        let stderr = await_stderr(&mut self.stderr_task).await;
        terminated.map_err(|_| StdioError::CleanupFailed)?;
        let status = status
            .map_err(|_| StdioError::CleanupFailed)?
            .map_err(|_| StdioError::CleanupFailed)?;
        stderr?;
        self.process_group.disarm();
        Ok(status)
    }
}

fn snapshot(log: &Arc<Mutex<StderrLog>>, secrets: &[String]) -> StderrSnapshot {
    let guard = log.lock().expect("stderr log");
    let text = String::from_utf8_lossy(&guard.bytes).to_string();
    StderrSnapshot {
        text: redact_text(&text, secrets),
        truncated: guard.truncated,
    }
}

fn redact_text(text: &str, secrets: &[String]) -> String {
    let mut text = text.to_string();
    for secret in secrets {
        if !secret.is_empty() {
            text = text.replace(secret.as_str(), "***");
        }
    }
    text
}

/// Resolve `argv0`: absolute when it names a path, otherwise from the retained
/// allowlisted PATH (with a read-only parent fallback for manual configs).
fn resolve_bin(argv0: &str, child_env: &[(String, String)]) -> Result<PathBuf, StdioError> {
    if argv0.contains('/') {
        let path = PathBuf::from(argv0);
        if path.is_file() {
            return Ok(path);
        }
        return Err(StdioError::Spawn);
    }
    let path_var = child_env
        .iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| OsStr::new(value).to_os_string())
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(argv0);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(StdioError::Spawn)
}

fn configured_command(
    config: &StdioConfig,
) -> Result<(tokio::process::Command, PathBuf), StdioError> {
    config.validate()?;
    let resolved_bin = resolve_bin(&config.argv[0], &config.extra_env)?;
    let mut cmd = tokio::process::Command::new(&resolved_bin);
    cmd.args(&config.argv[1..]);
    cmd.env_clear();
    cmd.envs(config.extra_env.iter().map(|(name, value)| (name, value)));
    cmd.current_dir(config.cwd.as_deref().unwrap_or_else(|| Path::new("/tmp")));
    cmd.kill_on_drop(true);
    // SAFETY: setpgid is async-signal-safe and touches no shared memory. It
    // runs after fork and before exec, making the child leader of a new group.
    unsafe {
        cmd.pre_exec(become_process_group_leader);
    }
    Ok((cmd, resolved_bin))
}

fn become_process_group_leader() -> std::io::Result<()> {
    // SAFETY: setpgid is async-signal-safe; both zero arguments refer to the
    // pre-exec child itself and request a new process group led by that child.
    if unsafe { libc::setpgid(0, 0) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
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
    let (mut cmd, resolved_bin) = configured_command(config)?;
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| StdioError::Spawn)?;
    let process_group = OwnedProcessGroup::new(child.id().ok_or(StdioError::Spawn)?)?;
    let stderr = child.stderr.take();
    let log = Arc::new(Mutex::new(StderrLog::default()));
    let stderr_task = stderr.map(|pipe| {
        let worker = log.clone();
        tokio::spawn(async move {
            drain_capped(pipe, &worker).await;
        })
    });
    Ok(SpawnedChild {
        child,
        process_group,
        stderr: log,
        stderr_task,
        resolved_bin,
        argv_tail: config.argv[1..].to_vec(),
    })
}

async fn await_stderr(task: &mut Option<tokio::task::JoinHandle<()>>) -> Result<(), StdioError> {
    let Some(mut task) = task.take() else {
        return Ok(());
    };
    match tokio::time::timeout(STDERR_DRAIN_TIMEOUT, &mut task).await {
        Ok(result) => result.map_err(|_| StdioError::CleanupFailed),
        Err(_) => {
            task.abort();
            let _ = task.await;
            Err(StdioError::CleanupFailed)
        }
    }
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

#[derive(Clone, Default)]
struct ClientEvents {
    tools_changed: Arc<std::sync::atomic::AtomicBool>,
}

impl rmcp::handler::client::ClientHandler for ClientEvents {
    fn on_tool_list_changed(
        &self,
        _context: rmcp::service::NotificationContext<rmcp::service::RoleClient>,
    ) -> impl std::future::Future<Output = ()> + rmcp::service::MaybeSendFuture + '_ {
        self.tools_changed
            .store(true, std::sync::atomic::Ordering::Release);
        std::future::ready(())
    }
}

type McpPeer = rmcp::service::Peer<rmcp::service::RoleClient>;
type McpRunning = rmcp::service::RunningService<rmcp::service::RoleClient, ClientEvents>;

/// Connected stdio client: one child, one handshake, restartable.
pub struct StdioClient {
    peer: McpPeer,
    running: Option<McpRunning>,
    child: Box<SpawnedChild>,
    config: StdioConfig,
    generation: u64,
    tools_changed: Arc<std::sync::atomic::AtomicBool>,
}

impl StdioClient {
    /// Launch the child and complete the MCP handshake.
    pub async fn launch(config: &StdioConfig) -> Result<Self, StdioError> {
        Self::launch_generation(config, 1).await
    }

    async fn launch_generation(config: &StdioConfig, generation: u64) -> Result<Self, StdioError> {
        Self::launch_cancellable_generation(
            config,
            generation,
            &std::sync::atomic::AtomicBool::new(false),
        )
        .await
    }

    /// Cancel a pre-acceptance handshake, then await owned process/stderr cleanup.
    pub async fn launch_cancellable(
        config: &StdioConfig,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, StdioError> {
        Self::launch_cancellable_generation(config, 1, cancel).await
    }

    async fn launch_cancellable_generation(
        config: &StdioConfig,
        generation: u64,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, StdioError> {
        // Keep the Child here: rmcp 3.4 TokioChildProcess::Drop starts an
        // unjoinable kill task. Give the SDK only its protocol pipes instead.
        let mut child = spawn_child(config)?;
        let transport = rmcp::transport::async_rw::AsyncRwTransport::new(
            child.child.stdout.take().expect("spawn_child stdout pipe"),
            child.child.stdin.take().expect("spawn_child stdin pipe"),
        );
        let events = ClientEvents::default();
        let tools_changed = events.tools_changed.clone();
        let handshake = tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => Err(StdioError::Cancelled),
            result = tokio::time::timeout(config.timeout, rmcp::service::serve_client(events, transport)) => {
                result.map_err(|_| StdioError::Deadline).and_then(|result| result.map_err(|_| StdioError::Transport))
            }
        };
        let running = match handshake {
            Ok(running) => running,
            Err(error) => {
                child.kill_reap().await?;
                return Err(error);
            }
        };
        let peer = running.peer().clone();
        Ok(Self {
            peer,
            child: Box::new(child),
            running: Some(running),
            config: config.clone(),
            generation,
            tools_changed,
        })
    }

    /// Atomically claim a pending tools/list refresh. A notification that
    /// arrives during the relist stays claimed for the next turn.
    pub fn claim_catalog_changed(&self) -> bool {
        self.tools_changed
            .swap(false, std::sync::atomic::Ordering::AcqRel)
    }

    /// Restore a claimed refresh after a failed relist so it is retried.
    pub fn restore_catalog_changed(&self) {
        self.tools_changed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Config generation: increments on every restart from the same argv.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Dedicated process-group id, equal to the original wrapper pid.
    pub fn process_group_id(&self) -> Option<u32> {
        self.child.process_group_id()
    }

    /// Current bounded stderr snapshot with configured secrets redacted.
    pub fn stderr_snapshot(&self) -> StderrSnapshot {
        self.child.stderr_snapshot(&self.config.secrets)
    }

    /// List tools with a bounded cursor loop.
    pub async fn list_tools(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<Vec<crate::mcp_remote::RemoteTool>, StdioError> {
        let tools = tokio::select! {
            biased;
            _ = wait_cancelled(cancel) => return Err(StdioError::Cancelled),
            result = tokio::time::timeout(self.config.timeout, bounded_list(&self.peer)) => {
                result.map_err(|_| StdioError::Deadline)??
            }
        }
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
        let arguments = arguments
            .as_object()
            .cloned()
            .ok_or(StdioError::NonObjectArguments)?;
        let mut params = rmcp::model::CallToolRequestParams::new(tool.to_string());
        params.arguments = Some(arguments);
        let outcome = self
            .run_cancel(cancel, self.peer.call_tool_once(params))
            .await?;
        match outcome {
            rmcp::model::CallToolResponse::Complete(result) => {
                if result.is_error == Some(true) {
                    return Err(StdioError::ToolFailed);
                }
                if result.structured_content.is_some() {
                    return Err(StdioError::UnsupportedModality);
                }
                let mut text = String::new();
                for block in &result.content {
                    let rmcp::model::ContentBlock::Text(t) = block else {
                        return Err(StdioError::UnsupportedModality);
                    };
                    let separator = usize::from(!text.is_empty());
                    if text
                        .len()
                        .saturating_add(separator)
                        .saturating_add(t.text.len())
                        > RESULT_TEXT_BYTES_CAP
                    {
                        return Err(StdioError::BadResult);
                    }
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&t.text);
                }
                if text.is_empty() {
                    return Err(StdioError::BadResult);
                }
                Ok(text)
            }
            _ => Err(StdioError::UnsupportedModality),
        }
    }

    /// Explicitly close rmcp, terminate the owned process group, reap the
    /// wrapper, and finish the bounded stderr drain.
    ///
    /// The owned process group is disarmed only after every teardown step
    /// succeeds; otherwise Drop keeps the SIGKILL fallback armed.
    pub async fn shutdown(mut self) -> Result<(), StdioError> {
        if let Some(running) = self.running.as_ref() {
            running.cancellation_token().cancel();
        }
        let closed = if let Some(mut running) = self.running.take() {
            match running.close_with_timeout(SERVICE_CLOSE_TIMEOUT).await {
                Ok(Some(
                    rmcp::service::QuitReason::Closed | rmcp::service::QuitReason::Cancelled,
                )) => Ok(()),
                Ok(Some(_)) | Err(_) => Err(StdioError::Transport),
                Ok(None) => Err(StdioError::Transport),
            }
        } else {
            Ok(())
        };
        let reaped = self.child.kill_reap().await;
        reaped?;
        closed?;
        Ok(())
    }

    /// Restart from the same retained config: shutdown, respawn, re-handshake.
    pub async fn restart(self) -> Result<Self, StdioError> {
        let generation = self.generation + 1;
        let config = self.config.clone();
        self.shutdown().await?;
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

async fn bounded_list(peer: &McpPeer) -> Result<Vec<rmcp::model::Tool>, StdioError> {
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for page_index in 0..MAX_LIST_PAGES {
        let params = rmcp::model::PaginatedRequestParams::default().with_cursor(cursor);
        let page = peer
            .list_tools(Some(params))
            .await
            .map_err(|_| StdioError::Transport)?;
        if tools.len().saturating_add(page.tools.len()) > TOOLS_CAP {
            return Err(StdioError::CatalogLimited);
        }
        tools.extend(page.tools);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(tools);
        }
        if page_index + 1 == MAX_LIST_PAGES {
            return Err(StdioError::CatalogLimited);
        }
    }
    Err(StdioError::CatalogLimited)
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    #[tokio::test]
    async fn stderr_worker_failure_is_reported_after_child_reap() {
        let config = StdioConfig {
            server_id: "fixture".into(),
            argv: vec!["/bin/cat".into()],
            cwd: None,
            extra_env: Vec::new(),
            secrets: Vec::new(),
            timeout: STDIO_TIMEOUT,
            enabled: true,
        };
        let mut child = spawn_child(&config).unwrap();
        let pid = child.pid().unwrap() as libc::pid_t;
        let old = child.stderr_task.take().unwrap();
        old.abort();
        let _ = old.await;
        child.stderr_task = Some(tokio::spawn(async {
            panic!("injected stderr worker failure")
        }));
        assert_eq!(child.kill_reap().await, Err(StdioError::CleanupFailed));
        // SAFETY: signal zero probes only our fixture's recorded child PID.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
        assert!(
            child.process_group.id().is_some(),
            "cleanup failure keeps fallback armed"
        );
    }
}
