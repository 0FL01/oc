//! Shell supervisor for T10 (TOOL05–TOOL06).
//!
//! One-shot supervised execution, not a persistent shell manager and not a
//! sandbox: the child shares the filesystem, network and uid. Per call the
//! supervisor forks a fresh session (`setsid`), pins `cwd` inside the trusted
//! root, exposes only an allowlisted non-credential child environment,
//! services stdin/stdout/stderr concurrently into bounded buffers, and
//! enforces one deadline that starts before spawn. Leader exit is not group
//! completion: drains get a bounded window, then the owned session group is
//! TERM→grace→KILLed so a descendant holding a pipe can never hang the call.
//! Timeout kills report `Unknown`, explicit cancellation reports `Cancelled`;
//! neither is ever presented as success.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use thiserror::Error;

/// Retained stdout/stderr bytes per stream (truncation flagged, never grown).
pub const RETAIN_CAP_BYTES: usize = 1024 * 1024;
/// Bounded stdin payload.
pub const STDIN_CAP_BYTES: usize = 65536;
/// Max argv entries (program + args).
pub const ARGV_CAP: usize = 64;
/// Max bytes per argv entry.
pub const ARG_BYTES_CAP: usize = 32768;

/// Typed shell errors (no secrets, no payload contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ShellError {
    /// Spawn refused or failed.
    #[error("spawn refused: {reason}")]
    SpawnRefused {
        /// Human reason.
        reason: String,
    },
    /// `sh -c` script shape rejected (pipes/chains/background).
    #[error("command shape refused: {reason}")]
    ShapeRefused {
        /// Human reason.
        reason: String,
    },
    /// Working directory outside the trusted root.
    #[error("cwd outside trusted root")]
    BadCwd,
    /// Stdin payload exceeds the bound.
    #[error("stdin too large")]
    StdinTooLarge,
    /// Wait/reap failure.
    #[error("reap failed")]
    Reap,
}

/// Terminal outcome of one supervised call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutcome {
    /// Process exit code (`None` = killed by signal).
    pub code: Option<i32>,
    /// Terminating signal (`None` = exited normally).
    pub signal: Option<i32>,
    /// Retained stdout (truncated at [`RETAIN_CAP_BYTES`]).
    pub stdout: Vec<u8>,
    /// Retained stderr (truncated at [`RETAIN_CAP_BYTES`]).
    pub stderr: Vec<u8>,
    /// True when stdout hit the cap.
    pub stdout_truncated: bool,
    /// True when stderr hit the cap.
    pub stderr_truncated: bool,
    /// Killed by deadline: outcome is `Unknown`, never success.
    pub timed_out: bool,
    /// Killed by explicit cancel flag: outcome is `Cancelled`.
    pub cancelled: bool,
    /// Wall-clock elapsed.
    pub elapsed: Duration,
}

/// Per-call limits (tests shrink these; product uses larger values).
#[derive(Debug, Clone, Copy)]
pub struct ShellLimits {
    /// Total deadline for the call.
    pub timeout: Duration,
    /// Grace between `TERM` and `KILL` to the group.
    pub kill_grace: Duration,
    /// Retained bytes per stream.
    pub retain_cap: usize,
}

impl Default for ShellLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(120),
            kill_grace: Duration::from_secs(2),
            retain_cap: RETAIN_CAP_BYTES,
        }
    }
}

/// Supervised shell bound to a trusted root.
#[derive(Debug, Clone)]
pub struct Shell {
    root: PathBuf,
}

impl Shell {
    /// Bind a trusted root (absolute). Data-root exclusion is enforced by
    /// the caller passing the project root, never the data root itself.
    pub fn new(project_root: &Path) -> Result<Self, ShellError> {
        if !project_root.is_absolute() {
            return Err(ShellError::BadCwd);
        }
        // Canonical root: symlink containment compares real paths.
        Ok(Self {
            root: project_root
                .canonicalize()
                .map_err(|_| ShellError::BadCwd)?,
        })
    }

    /// Execute `argv` (no shell joining: `argv[0]` is the program).
    ///
    /// `parent_env` is the caller-observed environment (production passes
    /// `std::env::vars`); only allowlisted non-credential names reach the
    /// child. `cancel` is polled alongside the deadline, which starts before
    /// spawn so a slow spawn or a blocked stdin write cannot extend it.
    pub fn execute(
        &self,
        parent_env: &BTreeMap<String, String>,
        argv: &[String],
        cwd: &str,
        stdin: Option<&[u8]>,
        limits: ShellLimits,
        cancel: &AtomicBool,
    ) -> Result<ShellOutcome, ShellError> {
        validate_argv(argv)?;
        let stdin = stdin.unwrap_or_default();
        if stdin.len() > STDIN_CAP_BYTES {
            return Err(ShellError::StdinTooLarge);
        }
        let cwd_abs = self.resolve_cwd(cwd)?;
        // The single deadline covers spawn, stdin write, execution and the
        // bounded drain/teardown window.
        let start = Instant::now();
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.current_dir(&cwd_abs);
        cmd.stdin(if stdin.is_empty() {
            Stdio::null()
        } else {
            Stdio::piped()
        });
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.env_clear();
        for (key, value) in child_env(parent_env) {
            cmd.env(&key, &value);
        }
        // SAFETY: process_group(0) only calls setsid in the child before
        // exec; it touches no Rust memory and cannot fail the parent.
        // Both unsafe ops below are the single coupled pre-exec setup.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        // New session => process group leader; group kills stay contained.
        // The OS error is kept (kind only, no payload) so a refused exec is
        // diagnosable instead of a bare `Reap`.
        let mut child = cmd.spawn().map_err(|error| ShellError::SpawnRefused {
            reason: format!("exec: {error}"),
        })?;
        let pid = child.id() as i32;

        // stdin runs on its own thread: a child that never reads cannot block
        // the supervisor (the writer is released by pipe close or group kill).
        let stdin_done = if stdin.is_empty() {
            None
        } else {
            child
                .stdin
                .take()
                .map(|pipe| spawn_stdin(pipe, stdin.to_vec()))
        };

        // Concurrent drains: one thread per pipe so interleaved floods can
        // never deadlock a full pipe buffer while we wait below.
        let cap = limits.retain_cap;
        let out = spawn_drain(child.stdout.take(), cap);
        let err = spawn_drain(child.stderr.take(), cap);

        let mut outcome_kind = OutcomeKind::Exited;
        loop {
            if cancel.load(Ordering::Relaxed) {
                outcome_kind = OutcomeKind::Cancelled;
                break;
            }
            match child.try_wait().map_err(|_| ShellError::Reap)? {
                Some(_) => break,
                None => {
                    if start.elapsed() >= limits.timeout {
                        outcome_kind = OutcomeKind::TimedOut;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
        // Deadline/cancel: the whole owned group goes first, so the leader
        // reap below cannot block on a still-running child.
        if outcome_kind != OutcomeKind::Exited {
            kill_group(pid, limits.kill_grace);
        }
        // Leader status (cached by `try_wait` above when it exited).
        let status = child.wait().map_err(|_| ShellError::Reap)?;

        // Leader exit is not group completion: a descendant may still hold
        // the pipes open. A normal exit first gets a bounded drain window,
        // then the owned group is removed so the reader threads can finish.
        if outcome_kind == OutcomeKind::Exited {
            wait_drains(
                &out,
                &err,
                stdin_done.as_ref(),
                Instant::now() + limits.kill_grace,
            );
            if !drains_finished(&out, &err, stdin_done.as_ref()) {
                kill_group(pid, limits.kill_grace);
            }
        }
        // Bounded completion wait: a reader blocked on a pipe held outside
        // our group must never hang the call; partial output is still
        // returned instead of joining a possibly infinite reader thread.
        wait_drains(
            &out,
            &err,
            stdin_done.as_ref(),
            Instant::now() + limits.kill_grace,
        );
        let (stdout, stdout_truncated) = out.take();
        let (stderr, stderr_truncated) = err.take();
        Ok(ShellOutcome {
            code: status.code(),
            signal: status.signal(),
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
            timed_out: outcome_kind == OutcomeKind::TimedOut,
            cancelled: outcome_kind == OutcomeKind::Cancelled,
            elapsed: start.elapsed(),
        })
    }

    /// Single bounded `sh -c` call with shape validation (no pipes, chains,
    /// backgrounding or command substitution outside quotes).
    #[allow(clippy::too_many_arguments)]
    pub fn run_sh(
        &self,
        parent_env: &BTreeMap<String, String>,
        script: &str,
        cwd: &str,
        stdin: Option<&[u8]>,
        limits: ShellLimits,
        cancel: &AtomicBool,
    ) -> Result<ShellOutcome, ShellError> {
        validate_sh_script(script)?;
        self.execute(
            parent_env,
            &["sh".to_string(), "-c".to_string(), script.to_string()],
            cwd,
            stdin,
            limits,
            cancel,
        )
    }

    fn resolve_cwd(&self, cwd: &str) -> Result<PathBuf, ShellError> {
        let mut abs = self.root.clone();
        let path = if cwd.is_empty() {
            Path::new(".")
        } else {
            Path::new(cwd)
        };
        for comp in path.components() {
            match comp {
                Component::Prefix(_) | Component::RootDir => return Err(ShellError::BadCwd),
                Component::ParentDir => {
                    abs.pop();
                }
                Component::CurDir => {}
                Component::Normal(part) => abs.push(part),
            }
        }
        if abs != self.root && !abs.starts_with(&self.root) {
            return Err(ShellError::BadCwd);
        }
        if !abs.is_dir() {
            return Err(ShellError::BadCwd);
        }
        // Lexical containment is not enough: a symlink inside the trusted
        // root can point outside it. The kernel resolves the real path, so
        // the canonical target must stay inside the canonical root.
        let canonical = abs.canonicalize().map_err(|_| ShellError::BadCwd)?;
        if canonical != self.root && !canonical.starts_with(&self.root) {
            return Err(ShellError::BadCwd);
        }
        Ok(canonical)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    Exited,
    TimedOut,
    Cancelled,
}

/// Bounded concurrent pipe drain shared with the supervisor.
///
/// Retains `cap` bytes, keeps draining past the cap while flagging
/// truncation (so a flooding child never blocks on a full pipe), and marks
/// completion under the same lock. The supervisor reads partial output
/// without ever joining the thread unboundedly.
struct Drain {
    state: Arc<Mutex<DrainState>>,
}

#[derive(Default)]
struct DrainState {
    bytes: Vec<u8>,
    truncated: bool,
    done: bool,
}

impl Drain {
    fn finished(&self) -> bool {
        self.state.lock().expect("drain state").done
    }

    fn take(&self) -> (Vec<u8>, bool) {
        let guard = self.state.lock().expect("drain state");
        (guard.bytes.clone(), guard.truncated)
    }
}

fn spawn_drain<R: Read + Send + 'static>(pipe: Option<R>, cap: usize) -> Drain {
    let state = Arc::new(Mutex::new(DrainState::default()));
    let worker = state.clone();
    std::thread::spawn(move || {
        if let Some(mut pipe) = pipe {
            let mut tmp = [0u8; 8192];
            loop {
                match pipe.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = worker.lock().expect("drain state");
                        let room = cap.saturating_sub(guard.bytes.len());
                        if room == 0 {
                            guard.truncated = true;
                        } else {
                            let take = n.min(room);
                            guard.bytes.extend_from_slice(&tmp[..take]);
                            if take < n {
                                guard.truncated = true;
                            }
                        }
                    }
                }
            }
        }
        worker.lock().expect("drain state").done = true;
    });
    Drain { state }
}

/// Write the bounded stdin payload off-thread; returns the completion flag.
fn spawn_stdin(mut pipe: std::process::ChildStdin, data: Vec<u8>) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    std::thread::spawn(move || {
        use std::io::Write as _;
        // A child that never reads blocks this writer, not the supervisor:
        // pipe close (child exit) or group teardown releases it.
        let _ = pipe.write_all(&data);
        drop(pipe);
        flag.store(true, Ordering::Release);
    });
    done
}

fn drains_finished(out: &Drain, err: &Drain, stdin: Option<&Arc<AtomicBool>>) -> bool {
    out.finished() && err.finished() && stdin.is_none_or(|flag| flag.load(Ordering::Acquire))
}

/// Wait for all three stdio streams to finish, never past `deadline`.
fn wait_drains(out: &Drain, err: &Drain, stdin: Option<&Arc<AtomicBool>>, deadline: Instant) {
    while Instant::now() < deadline {
        if drains_finished(out, err, stdin) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// TERM the owned session group, wait up to `grace`, then KILL it.
///
/// Containment: `setsid` in the pre-exec child makes it session and group
/// leader with `pgid == pid`, so `killpg(pid, …)` can only ever reach
/// processes this call started — never the supervisor's own group.
fn kill_group(pid: i32, grace: Duration) {
    if !group_alive(pid) {
        return;
    }
    // SAFETY: signals the child's own session group only; a vanished group
    // (ESRCH) is already handled by the `group_alive` probe above.
    unsafe {
        libc::killpg(pid, libc::SIGTERM);
    }
    let deadline = Instant::now() + grace;
    while group_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if group_alive(pid) {
        // SAFETY: same containment as the TERM above.
        unsafe {
            libc::killpg(pid, libc::SIGKILL);
        }
    }
}

fn group_alive(pid: i32) -> bool {
    // SAFETY: signal 0 only probes the child's own process group.
    unsafe { libc::killpg(pid, 0) == 0 }
}

/// Strict child-environment allowlist: names that cannot carry provider,
/// MCP or runner credentials and that ordinary dev toolchains need.
///
/// This is a positive list, not a "name does not contain a keyword" filter:
/// anything unlisted (including innocuous names) is dropped.
const ENV_ALLOWLIST: [&str; 13] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "TERM",
    "USER",
    "LOGNAME",
    "SHELL",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
];

fn env_allowed(name: &str) -> bool {
    name == "LANG" || name.starts_with("LC_") || ENV_ALLOWLIST.contains(&name)
}

/// Minimal child env: strict name allowlist from the parent plus working
/// defaults.
///
/// Allowlisted names keep their parent value so toolchains resolve normally
/// (`PATH`, `HOME`, `CARGO_HOME`, …); `PATH`/`LANG` fall back to a working
/// default when unset. Every other name — credential-shaped or not — is
/// dropped, because a substring denylist is not a security boundary.
pub fn child_env(parent: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (key, value) in parent {
        if !env_allowed(key) || value.contains('\0') {
            continue;
        }
        out.insert(key.clone(), value.clone());
    }
    out.entry("PATH".to_string())
        .or_insert_with(|| "/usr/bin:/bin".to_string());
    out.entry("LANG".to_string())
        .or_insert_with(|| "C.UTF-8".to_string());
    out
}

fn validate_argv(argv: &[String]) -> Result<(), ShellError> {
    if argv.is_empty() || argv.len() > ARGV_CAP {
        return Err(ShellError::SpawnRefused {
            reason: "argv arity".to_string(),
        });
    }
    if argv
        .iter()
        .any(|a| a.is_empty() || a.len() > ARG_BYTES_CAP || a.contains('\0'))
    {
        return Err(ShellError::SpawnRefused {
            reason: "argv shape".to_string(),
        });
    }
    Ok(())
}

/// Reject multi-command shell shapes outside quotes: `|`, `&&`, `||`, `;`,
/// backticks, `$(...)`, trailing `&` backgrounding.
fn validate_sh_script(script: &str) -> Result<(), ShellError> {
    if script.len() > STDIN_CAP_BYTES {
        return Err(ShellError::ShapeRefused {
            reason: "script too large".to_string(),
        });
    }
    let bytes = script.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == b'\\' && q == b'"' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'\'' | b'"' => {
                quote = Some(c);
                i += 1;
            }
            b'`' => {
                return Err(ShellError::ShapeRefused {
                    reason: "backticks".to_string(),
                });
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'(' => {
                // `$((...))` arithmetic runs no commands: skip in. Other
                // `$(...)` is command substitution: refused.
                if i + 2 < bytes.len() && bytes[i + 2] == b'(' {
                    i += 3;
                    continue;
                }
                return Err(ShellError::ShapeRefused {
                    reason: "command substitution".to_string(),
                });
            }
            b'|' | b';' => {
                return Err(ShellError::ShapeRefused {
                    reason: "pipes/chains".to_string(),
                });
            }
            b'&' if i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                return Err(ShellError::ShapeRefused {
                    reason: "pipes/chains".to_string(),
                });
            }
            b'&' => {
                let fd_redirect = i > 0 && (bytes[i - 1] == b'>' || bytes[i - 1] == b'<');
                if fd_redirect {
                    i += 1;
                    continue;
                }
                return Err(ShellError::ShapeRefused {
                    reason: "pipes/chains".to_string(),
                });
            }
            _ => {
                i += 1;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Shell, ShellError, ShellLimits, child_env};
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    fn setup() -> (tempfile::TempDir, Shell) {
        let tmp = tempfile::tempdir().expect("temp");
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        (tmp, Shell::new(&project).expect("shell"))
    }

    fn parent_env() -> BTreeMap<String, String> {
        [
            ("LUDKA_API_KEY", "parent-secret"),
            ("OPENAI_API_KEY", "parent-secret"),
            ("MCP_TOKEN", "parent-secret"),
            ("MYAPP_OK", "1"),
            ("HOME", "/home/fixture"),
            ("PATH", "/usr/bin:/bin"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    fn limits(timeout_ms: u64) -> ShellLimits {
        ShellLimits {
            timeout: Duration::from_millis(timeout_ms),
            kill_grace: Duration::from_millis(200),
            retain_cap: super::RETAIN_CAP_BYTES,
        }
    }

    static NO_CANCEL: AtomicBool = AtomicBool::new(false);

    #[test]
    fn tool05_interleaved_output_no_deadlock() {
        let (_tmp, shell) = setup();
        let out = shell
            .run_sh(
                &parent_env(),
                "i=0\nwhile [ $i -lt 2000 ]\ndo\necho out-$i\necho err-$i >&2\ni=$((i+1))\ndone",
                ".",
                None,
                limits(30_000),
                &NO_CANCEL,
            )
            .expect("run");
        assert_eq!(out.code, Some(0));
        assert!(!out.timed_out && !out.cancelled);
        let stdout = String::from_utf8(out.stdout).expect("utf8");
        let stderr = String::from_utf8(out.stderr).expect("utf8");
        assert!(stdout.contains("out-1999"));
        assert!(stderr.contains("err-1999"));
        assert!(!out.stdout_truncated && !out.stderr_truncated);
    }

    #[test]
    fn tool05_truncation_exit_and_shape() {
        let (_tmp, shell) = setup();
        // 3 MiB of stdout against a 1 MiB cap: bounded, flagged, exit kept.
        let out = shell
            .execute(
                &parent_env(),
                &[
                    "head".to_string(),
                    "-c".to_string(),
                    "3000000".to_string(),
                    "/dev/zero".to_string(),
                ],
                ".",
                None,
                limits(30_000),
                &NO_CANCEL,
            )
            .expect("run");
        assert_eq!(out.code, Some(0));
        assert_eq!(out.stdout.len(), super::RETAIN_CAP_BYTES);
        assert!(out.stdout_truncated);
        // Exit codes propagate.
        let out = shell
            .run_sh(
                &parent_env(),
                "exit 42",
                ".",
                None,
                limits(10_000),
                &NO_CANCEL,
            )
            .expect("run");
        assert_eq!(out.code, Some(42));
        // Multi-command shapes refused before spawn.
        assert!(matches!(
            shell.run_sh(
                &parent_env(),
                "echo a | cat",
                ".",
                None,
                limits(1000),
                &NO_CANCEL
            ),
            Err(ShellError::ShapeRefused { .. })
        ));
        assert!(matches!(
            shell.run_sh(&parent_env(), "a && b", ".", None, limits(1000), &NO_CANCEL),
            Err(ShellError::ShapeRefused { .. })
        ));
        assert!(matches!(
            shell.run_sh(
                &parent_env(),
                "sleep 1 &",
                ".",
                None,
                limits(1000),
                &NO_CANCEL
            ),
            Err(ShellError::ShapeRefused { .. })
        ));
        // Quoted operators are data, not chains.
        let out = shell
            .run_sh(
                &parent_env(),
                "echo 'a|b'",
                ".",
                None,
                limits(10_000),
                &NO_CANCEL,
            )
            .expect("run");
        assert_eq!(out.stdout, b"a|b\n");
    }

    #[test]
    fn tool05_child_env_excludes_credentials() {
        let (_tmp, shell) = setup();
        let out = shell
            .execute(
                &parent_env(),
                &["env".to_string()],
                ".",
                None,
                limits(10_000),
                &NO_CANCEL,
            )
            .expect("run");
        let text = String::from_utf8(out.stdout).expect("utf8");
        // Allowlisted non-credential names keep their parent value…
        assert!(text.contains("HOME=/home/fixture"));
        assert!(text.contains("PATH=/usr/bin:/bin"));
        assert!(!text.contains("parent-secret"));
        // …everything else is dropped: a name-based denylist is not a
        // boundary, so even an innocuous unlisted name must not survive.
        assert!(!text.contains("MYAPP_OK"), "unlisted env leaked: {text}");
        let scrubbed = child_env(&parent_env());
        assert!(!scrubbed.contains_key("LUDKA_API_KEY"));
        assert!(!scrubbed.contains_key("MCP_TOKEN"));
        assert!(!scrubbed.contains_key("MYAPP_OK"));
        assert_eq!(
            scrubbed.get("HOME").map(String::as_str),
            Some("/home/fixture")
        );
        // Working defaults apply when the parent lacks them.
        let bare = child_env(&BTreeMap::new());
        assert_eq!(bare.get("PATH").map(String::as_str), Some("/usr/bin:/bin"));
        assert_eq!(bare.get("LANG").map(String::as_str), Some("C.UTF-8"));
    }

    #[test]
    fn aud28_symlink_cwd_escape_refused_and_toolchain_resolves() {
        let (tmp, shell) = setup();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).expect("outside");
        std::os::unix::fs::symlink(&outside, tmp.path().join("project/link")).expect("symlink");
        assert!(matches!(
            shell.execute(
                &parent_env(),
                &["true".to_string()],
                "link",
                None,
                limits(5_000),
                &NO_CANCEL
            ),
            Err(ShellError::BadCwd)
        ));
        // A symlink that resolves inside the root stays usable, and the
        // child starts in the canonical directory.
        let real = tmp.path().join("project/real");
        std::fs::create_dir_all(&real).expect("real");
        std::os::unix::fs::symlink(&real, tmp.path().join("project/inner-link")).expect("inner");
        let out = shell
            .execute(
                &parent_env(),
                &["pwd".to_string()],
                "inner-link",
                None,
                limits(5_000),
                &NO_CANCEL,
            )
            .expect("run");
        assert_eq!(out.code, Some(0));
        let printed = String::from_utf8(out.stdout).expect("utf8");
        assert_eq!(
            printed.trim(),
            real.canonicalize().expect("canon").to_string_lossy()
        );

        // AUD28: ordinary dev toolchain binaries resolve through the
        // production child environment, with no test-only absolute hints.
        let real_parent: BTreeMap<String, String> = std::env::vars().collect();
        for (program, needle) in [("rustc", "rustc"), ("cargo", "cargo")] {
            let out = shell
                .execute(
                    &real_parent,
                    &[program.to_string(), "--version".to_string()],
                    ".",
                    None,
                    limits(30_000),
                    &NO_CANCEL,
                )
                .expect("toolchain run");
            assert_eq!(
                out.code,
                Some(0),
                "{program}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(String::from_utf8_lossy(&out.stdout).contains(needle));
        }
    }

    #[test]
    fn tool06_timeout_term_resistant_grandchild() {
        let (_tmp, shell) = setup();
        // Plain sleep exceeds the deadline.
        let start = std::time::Instant::now();
        let out = shell
            .execute(
                &parent_env(),
                &["sleep".to_string(), "30".to_string()],
                ".",
                None,
                limits(500),
                &NO_CANCEL,
            )
            .expect("run");
        assert!(out.timed_out && !out.cancelled);
        assert!(start.elapsed() < Duration::from_secs(10));
        // TERM-trapping child still dies via group KILL inside the bound.
        let start = std::time::Instant::now();
        let out = shell
            .run_sh(
                &parent_env(),
                "trap '' TERM\nsleep 30",
                ".",
                None,
                limits(500),
                &NO_CANCEL,
            )
            .expect("run");
        assert!(out.timed_out);
        assert!(out.code.is_none());
        assert!(start.elapsed() < Duration::from_secs(10));
        // Grandchild in the same group dies with it: a subshell fork (no
        // background operator needed) sleeps while the parent waits.
        let start = std::time::Instant::now();
        let out = shell
            .run_sh(
                &parent_env(),
                "(sleep 30)",
                ".",
                None,
                limits(500),
                &NO_CANCEL,
            )
            .expect("run");
        assert!(out.timed_out && !out.cancelled);
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn tool06_cancel_flag_reports_cancelled() {
        let (_tmp, shell) = setup();
        let cancel = AtomicBool::new(false);
        std::thread::scope(|s| {
            s.spawn(|| {
                std::thread::sleep(Duration::from_millis(200));
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            let start = std::time::Instant::now();
            let out = shell
                .execute(
                    &parent_env(),
                    &["sleep".to_string(), "30".to_string()],
                    ".",
                    None,
                    limits(20_000),
                    &cancel,
                )
                .expect("run");
            assert!(out.cancelled && !out.timed_out);
            assert!(start.elapsed() < Duration::from_secs(10));
        });
    }

    #[test]
    fn tool06_cwd_and_argv_guards() {
        let (_tmp, shell) = setup();
        assert!(matches!(
            shell.execute(
                &parent_env(),
                &["true".to_string()],
                "..",
                None,
                limits(1000),
                &NO_CANCEL
            ),
            Err(ShellError::BadCwd)
        ));
        assert!(matches!(
            shell.execute(&parent_env(), &[], ".", None, limits(1000), &NO_CANCEL),
            Err(ShellError::SpawnRefused { .. })
        ));
    }
}
