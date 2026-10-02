//! Shell supervisor for T10 (TOOL05–TOOL06).
//!
//! One-shot supervised execution, not a persistent shell manager and not a
//! sandbox: the child shares the filesystem, network and uid. Per call the
//! supervisor forks a fresh session (`setsid`), pins `cwd` inside the trusted
//! root, exposes only an allowlisted non-credential child environment,
//! services stdin/stdout/stderr concurrently into bounded buffers, and
//! enforces an optional execution deadline that starts before spawn. Zero
//! disables only that deadline. Leader exit is not group
//! completion: drains get a bounded window, then the owned session group is
//! TERM→grace→KILLed so a descendant holding a pipe can never hang the call.
//! Timeout kills report `Unknown`, explicit cancellation reports `Cancelled`;
//! neither is ever presented as success.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::os::unix::process::{CommandExt as _, ExitStatusExt as _};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) mod jobs;

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
    /// Legacy shell argv shape rejected.
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
    /// The process launched but its durable identity could not be frozen.
    #[error("durable launch admission failed; execution unknown")]
    AdmissionFailed,
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
    /// A drain window required disposing of a producer holding an output pipe.
    pub capture_interrupted: bool,
}

/// Per-call limits (tests shrink these; product uses larger values).
#[derive(Debug, Clone, Copy)]
pub struct ShellLimits {
    /// Execution deadline for the call; zero disables only this deadline.
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

/// Open authority retained from admission through exec. Neither directory is
/// reopened by pathname, including after the tool Started callback.
#[derive(Debug)]
pub struct PinnedCwd {
    root: File,
    cwd: File,
    pub(crate) path: PathBuf,
}
impl PinnedCwd {
    pub(crate) fn identity(&self) -> Result<[u64; 4], ShellError> {
        let root = self.root.metadata().map_err(|_| ShellError::BadCwd)?;
        let cwd = self.cwd.metadata().map_err(|_| ShellError::BadCwd)?;
        Ok([root.dev(), root.ino(), cwd.dev(), cwd.ino()])
    }
}

fn open_directory(path: &Path, mut directory: File) -> Result<File, ShellError> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    use std::os::unix::ffi::OsStrExt;
    for component in path.components() {
        let Component::Normal(component) = component else {
            if component == Component::RootDir {
                continue;
            }
            return Err(ShellError::BadCwd);
        };
        let name = CString::new(component.as_bytes()).map_err(|_| ShellError::BadCwd)?;
        // SAFETY: live parent descriptor and NUL-terminated single component;
        // each ancestor is opened no-follow and retained before the next hop.
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(ShellError::BadCwd);
        }
        // SAFETY: openat returned a new owned descriptor.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
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
    pub(crate) fn preflight(&self, argv: &[String], cwd: &str) -> Result<PathBuf, ShellError> {
        validate_argv(argv)?;
        self.resolve_cwd(cwd)
    }
    pub(crate) fn pin_cwd(&self, argv: &[String], cwd: &str) -> Result<Arc<PinnedCwd>, ShellError> {
        let path = self.preflight(argv, cwd)?;
        let slash = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open("/")
            .map_err(|_| ShellError::BadCwd)?;
        let root = open_directory(&self.root, slash)?;
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| ShellError::BadCwd)?;
        let directory =
            open_directory(relative, root.try_clone().map_err(|_| ShellError::BadCwd)?)?;
        Ok(Arc::new(PinnedCwd {
            root,
            cwd: directory,
            path,
        }))
    }

    /// Execute a validated invocation under the supervisor.
    ///
    /// `parent_env` is the caller-observed environment (production passes
    /// `std::env::vars`); only allowlisted non-credential names reach the
    /// child. `cancel` is polled alongside the optional deadline, which starts before
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
        self.execute_pinned(parent_env, argv, cwd, stdin, limits, cancel, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute_pinned(
        &self,
        parent_env: &BTreeMap<String, String>,
        argv: &[String],
        cwd: &str,
        stdin: Option<&[u8]>,
        limits: ShellLimits,
        cancel: &AtomicBool,
        approved: Option<&PinnedCwd>,
    ) -> Result<ShellOutcome, ShellError> {
        self.execute_pinned_started(
            parent_env,
            argv,
            cwd,
            stdin,
            limits,
            cancel,
            approved,
            false,
            None,
            |_| Ok(()),
        )
    }

    /// The lifecycle owner freezes OS identity before acknowledging launch.
    /// Failure in that durable callback tears down the group before returning.
    #[allow(clippy::too_many_arguments)]
    fn execute_pinned_started(
        &self,
        parent_env: &BTreeMap<String, String>,
        argv: &[String],
        cwd: &str,
        stdin: Option<&[u8]>,
        limits: ShellLimits,
        cancel: &AtomicBool,
        approved: Option<&PinnedCwd>,
        finish_group: bool,
        capture: Option<&jobs::Capture>,
        started: impl FnOnce(i32) -> Result<(), ShellError>,
    ) -> Result<ShellOutcome, ShellError> {
        validate_argv(argv)?;
        let stdin = stdin.unwrap_or_default();
        if stdin.len() > STDIN_CAP_BYTES {
            return Err(ShellError::StdinTooLarge);
        }
        let fallback;
        let pinned = match approved {
            Some(pinned) => pinned,
            None => {
                fallback = self.pin_cwd(argv, cwd)?;
                &fallback
            }
        };
        let cwd_fd = pinned.cwd.as_raw_fd();
        // The optional execution deadline covers spawn, stdin and execution.
        // Drain/teardown windows remain independently bounded when it is zero.
        let start = Instant::now();
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]);
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
            cmd.pre_exec(move || {
                if libc::fchdir(cwd_fd) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                // Directory descriptors are O_CLOEXEC; exec closes both after
                // the last use here. Parent Arc retains them until spawn ends.
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // New session => process group leader; group kills stay contained.
        // The OS error is kept (kind only, no payload) so a refused exec is
        // diagnosable instead of a bare `Reap`.
        let child = cmd.spawn().map_err(|error| ShellError::SpawnRefused {
            reason: format!("exec: {error}"),
        })?;
        let mut child = OwnedChild {
            process: child,
            grace: limits.kill_grace,
            reaped: false,
        };
        let pid = child.process.id() as i32;
        started(pid)?;

        // stdin runs on its own thread: a child that never reads cannot block
        // the supervisor (the writer is released by pipe close or group kill).
        let stdin_done = if stdin.is_empty() {
            None
        } else {
            child
                .process
                .stdin
                .take()
                .map(|pipe| spawn_stdin(pipe, stdin.to_vec()))
        };

        // Concurrent drains: one thread per pipe so interleaved floods can
        // never deadlock a full pipe buffer while we wait below.
        let cap = limits.retain_cap;
        let out = spawn_drain_live(
            child.process.stdout.take(),
            cap,
            capture.map(|c| (c, jobs::Stream::Stdout)),
        );
        let err = spawn_drain_live(
            child.process.stderr.take(),
            cap,
            capture.map(|c| (c, jobs::Stream::Stderr)),
        );

        let mut outcome_kind = OutcomeKind::Exited;
        loop {
            if cancel.load(Ordering::Relaxed) {
                outcome_kind = OutcomeKind::Cancelled;
                break;
            }
            match exited_without_reap(pid)? {
                true => break,
                false => {
                    if !limits.timeout.is_zero() && start.elapsed() >= limits.timeout {
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
        // Leader exit is not group completion: a descendant may still hold
        // the pipes open. A normal exit first gets a bounded drain window,
        // then the owned group is removed so the reader threads can finish.
        let mut capture_interrupted = false;
        if outcome_kind == OutcomeKind::Exited {
            wait_drains(
                &out,
                &err,
                stdin_done.as_ref(),
                Instant::now() + limits.kill_grace,
            );
            if finish_group || !drains_finished(&out, &err, stdin_done.as_ref()) {
                capture_interrupted = !drains_finished(&out, &err, stdin_done.as_ref());
                kill_group(pid, limits.kill_grace);
            }
        }
        // Keep the exited leader unreaped through group disposal. Its PID/PGID
        // cannot be recycled while we still have authority to signal the group.
        let status = child.process.wait().map_err(|_| ShellError::Reap)?;
        child.reaped = true;
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
            capture_interrupted,
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

fn exited_without_reap(pid: i32) -> Result<bool, ShellError> {
    // SAFETY: Linux siginfo_t accepts the all-zero empty WNOHANG result.
    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
    // SAFETY: pid identifies this supervisor's live/unreaped child; info is a
    // writable siginfo_t. WNOWAIT reserves identity until explicit child.wait.
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            pid as u32,
            &mut info,
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result != 0 {
        return Err(ShellError::Reap);
    }
    // SAFETY: successful waitid initialized the SIGCHLD pid member (or zero).
    Ok(unsafe { info.si_pid() } != 0)
}

struct OwnedChild {
    process: std::process::Child,
    grace: Duration,
    reaped: bool,
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let pid = self.process.id() as i32;
        if !self.reaped && exited_without_reap(pid).is_ok() {
            kill_group(pid, self.grace);
            let _ = self.process.wait();
        }
    }
}

/// Donor compatible Linux selection (shell/select.ts): inherited SHELL,
/// excluding fish/nu, then PATH bash, then /bin/sh. No login startup files.
pub(crate) fn command_argv(
    parent: &BTreeMap<String, String>,
    command: &str,
) -> Result<Vec<String>, ShellError> {
    let env = child_env(parent);
    let executable = |candidate: &str| -> Option<PathBuf> {
        if candidate.is_empty() || candidate.contains('\0') {
            return None;
        }
        let usable = |path: &Path| {
            use std::os::unix::fs::PermissionsExt as _;
            path.metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        };
        let path = Path::new(candidate);
        if path.is_absolute() {
            return usable(path).then(|| path.to_path_buf());
        }
        // Relative executable paths must not depend on the author's cwd.
        if path.components().count() != 1 {
            return None;
        }
        std::env::split_paths(std::ffi::OsStr::new(&env["PATH"]))
            .filter(|directory| directory.is_absolute())
            .map(|directory| directory.join(candidate))
            .find(|path| usable(path))
    };
    let selected = parent.get("SHELL").and_then(|candidate| {
        let name = Path::new(candidate)
            .file_name()?
            .to_str()?
            .to_ascii_lowercase();
        (!matches!(name.as_str(), "fish" | "nu"))
            .then(|| executable(candidate))
            .flatten()
    });
    let shell = selected
        .or_else(|| executable("bash"))
        .or_else(|| executable("/bin/sh"))
        .ok_or_else(|| ShellError::SpawnRefused {
            reason: "no executable Linux shell".into(),
        })?;
    let argv = vec![
        shell.to_string_lossy().into_owned(),
        "-c".into(),
        command.into(),
    ];
    validate_argv(&argv)?;
    Ok(argv)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    Exited,
    TimedOut,
    Cancelled,
}

/// Bounded concurrent pipe drain shared with the supervisor.
///
/// Jobs admit bytes to their registered writer before bounded recent retention;
/// low-level one-shot callers retain their bounded prefix. Nonblocking pipe reads
/// honor supervisor stop, and every owned reader joins before capture finalization.
struct Drain {
    state: Arc<Mutex<DrainState>>,
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

#[derive(Default)]
struct DrainState {
    bytes: Vec<u8>,
    total: u64,
    truncated: bool,
    done: bool,
}

impl Drain {
    fn finished(&self) -> bool {
        self.state.lock().expect("drain state").done
    }

    fn take(mut self) -> (Vec<u8>, bool) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        let mut guard = self.state.lock().expect("drain state");
        (std::mem::take(&mut guard.bytes), guard.truncated)
    }
}
impl Drop for Drain {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn spawn_drain_live<R: Read + AsRawFd + Send + 'static>(
    pipe: Option<R>,
    cap: usize,
    live: Option<(&jobs::Capture, jobs::Stream)>,
) -> Drain {
    let state = live.map_or_else(
        || Arc::new(Mutex::new(DrainState::default())),
        |(capture, stream)| match stream {
            jobs::Stream::Stdout => capture.stdout.clone(),
            jobs::Stream::Stderr => capture.stderr.clone(),
        },
    );
    // Borrowed capture lives in the supervisor; the drain holds the same owned
    // Arc via a scoped clone supplied by Jobs, not a second capture lifetime.
    let live = live.map(|(capture, stream)| (capture.shared(), stream));
    let worker = state.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let join = std::thread::spawn(move || {
        let mut complete = true;
        if let Some(mut pipe) = pipe {
            // Nonblocking pipe reads permit bounded teardown even if an escaped
            // descendant holds a descriptor. No detached reader survives return.
            let fd = pipe.as_raw_fd();
            // SAFETY: fd belongs to this drain's pipe and remains live here.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            // SAFETY: modifies only the owned pipe's open-file status flags.
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                complete = false;
            } else {
                let mut tmp = [0u8; 8192];
                loop {
                    if stopping.load(Ordering::Acquire) {
                        complete = false;
                        break;
                    }
                    match pipe.read(&mut tmp) {
                        Ok(0) => break,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(_) => {
                            complete = false;
                            break;
                        }
                        Ok(n) => {
                            if let Some((capture, stream)) = &live {
                                capture.ingest(*stream, &tmp[..n], false, true);
                                continue;
                            }
                            let mut guard = worker.lock().expect("drain state");
                            guard.total = guard.total.saturating_add(n as u64);
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
        }
        if let Some((capture, stream)) = &live {
            capture.ingest(*stream, &[], true, complete);
        } else {
            worker.lock().expect("drain state").done = true;
        }
    });
    Drain {
        state,
        stop,
        join: Some(join),
    }
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

pub(crate) fn validate_argv(argv: &[String]) -> Result<(), ShellError> {
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
