//! Shell supervisor for T10 (TOOL05–TOOL06).
//!
//! One-shot supervised execution, not a persistent shell manager and not a
//! sandbox: the child shares the filesystem, network and uid. Per call the
//! supervisor forks a fresh process group (`setsid`), pins `cwd` inside the
//! trusted root, scrubs the child environment of provider/MCP/runner
//! credentials, drains stdout/stderr concurrently into bounded buffers, and
//! enforces a deadline (`TERM` → grace → `KILL` to the whole group) with
//! verified reap. Timeout kills report `Unknown`, explicit cancellation
//! reports `Cancelled`; neither is ever presented as success.

use std::collections::BTreeMap;
use std::io::Read;
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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
        Ok(Self {
            root: project_root.to_path_buf(),
        })
    }

    /// Execute `argv` (no shell joining: `argv[0]` is the program).
    ///
    /// `parent_env` is the caller-observed environment (production passes
    /// `std::env::vars`); only the minimal base plus scrubbed extras reach
    /// the child. `cancel` is polled alongside the deadline.
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
        let mut child = cmd.spawn().map_err(|_| ShellError::Reap)?;
        let pid = child.id() as i32;

        if !stdin.is_empty() {
            use std::io::Write as _;
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(stdin);
                // Pipe closes on drop; a full pipe cannot block us because
                // the child drains concurrently with our reader threads.
            }
        }

        // Concurrent drains: one thread per pipe so interleaved floods can
        // never deadlock a full pipe buffer while we wait below.
        let mut out_pipe = child.stdout.take();
        let mut err_pipe = child.stderr.take();
        let cap = limits.retain_cap;
        let out_handle = std::thread::spawn(move || read_capped(out_pipe.take(), cap));
        let err_handle = std::thread::spawn(move || read_capped(err_pipe.take(), cap));

        let start = Instant::now();
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
        if outcome_kind != OutcomeKind::Exited {
            // TERM the whole group first (well-behaved children exit here),
            // then KILL after the grace period (TERM-trappers die here).
            // SAFETY: killpg targets our own child group only; negative
            // return values (already dead) are ignored.
            unsafe {
                libc::killpg(pid, libc::SIGTERM);
            }
            let grace_end = Instant::now() + limits.kill_grace;
            loop {
                match child.try_wait().map_err(|_| ShellError::Reap)? {
                    Some(_) => break,
                    None => {
                        if Instant::now() >= grace_end {
                            // SAFETY: same containment as above.
                            unsafe {
                                libc::killpg(pid, libc::SIGKILL);
                            }
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        }
        // Reap (blocks only when the group kill above is still in flight).
        let status = child.wait().map_err(|_| ShellError::Reap)?;
        let (stdout, stdout_truncated) = out_handle.join().map_err(|_| ShellError::Reap)?;
        let (stderr, stderr_truncated) = err_handle.join().map_err(|_| ShellError::Reap)?;
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
        Ok(abs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    Exited,
    TimedOut,
    Cancelled,
}

/// Bounded pipe read: retains `cap` bytes and flags truncation, draining
/// the rest without retaining so the child never blocks on a full pipe.
fn read_capped<T: Read>(pipe: Option<T>, cap: usize) -> (Vec<u8>, bool) {
    let Some(mut pipe) = pipe else {
        return (Vec::new(), false);
    };
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut truncated = false;
    loop {
        match pipe.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() + n > cap {
                    let room = cap.saturating_sub(buf.len());
                    buf.extend_from_slice(&tmp[..room]);
                    truncated = true;
                    // Drain the rest without retaining so the child never
                    // blocks on a full pipe while we wait for its exit.
                    while pipe.read(&mut tmp).map(|n| n > 0).unwrap_or(false) {}
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            Err(_) => break,
        }
    }
    (buf, truncated)
}

/// Minimal child env: fixed base plus scrubbed extras.
///
/// Dropped (case-insensitive substring): `api_key`, `apikey`, `secret`,
/// `token`, `password`, `passwd`, `authorization`, `credential`,
/// `private_key`, `ludka`, `openai`, `anthropic`, `mcp_`, `sentry_dsn`.
/// `PATH`/`LANG` come from the fixed base, never the parent.
pub fn child_env(parent: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    const BASE: [(&str, &str); 2] = [("PATH", "/usr/bin:/bin"), ("LANG", "C.UTF-8")];
    const DROP: [&str; 14] = [
        "api_key",
        "apikey",
        "secret",
        "token",
        "password",
        "passwd",
        "authorization",
        "credential",
        "private_key",
        "ludka",
        "openai",
        "anthropic",
        "mcp_",
        "sentry_dsn",
    ];
    let mut out: BTreeMap<String, String> = BASE
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for (key, value) in parent {
        if key == "PATH" || key == "LANG" {
            continue;
        }
        let lower = key.to_lowercase();
        if DROP.iter().any(|d| lower.contains(d)) {
            continue;
        }
        out.insert(key.clone(), value.clone());
    }
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
            ("PATH", "/evil/bin"),
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
        assert!(text.contains("MYAPP_OK=1"));
        assert!(!text.contains("parent-secret"));
        assert!(text.contains("PATH=/usr/bin:/bin"));
        // Direct unit coverage of the scrub list.
        let scrubbed = child_env(&parent_env());
        assert!(!scrubbed.contains_key("LUDKA_API_KEY"));
        assert!(!scrubbed.contains_key("MCP_TOKEN"));
        assert_eq!(scrubbed.get("MYAPP_OK").map(String::as_str), Some("1"));
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
