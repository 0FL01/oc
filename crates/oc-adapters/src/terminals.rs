//! Application-owned Linux PTYs. One joined, nonblocking drain per terminal;
//! no daemon, model tool, transcript, host escape forwarding or inherited auth.
use crate::shell::jobs::ProcessIdentity;
use crate::shell::{Shell, child_env, command_argv};
use crate::storage::{Db, StorageError};
use oc_core::domain::SessionId;
use oc_core::queries::*;
use oc_core::session::CoreError;
use std::collections::{BTreeMap, VecDeque};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread::JoinHandle;
use std::time::Duration;

pub(crate) const TERMINAL_CAP: usize = 8;
const RING_BYTES: usize = 64 * 1024;
const QUEUE_ITEMS: usize = 32;
const PENDING_INPUT: usize = 64 * 1024;
const SCROLLBACK: usize = 256;

fn refused() -> CoreError {
    CoreError::Application("terminal unavailable or invalid target".into())
}
fn storage(_: StorageError) -> CoreError {
    CoreError::Application("terminal storage unavailable".into())
}

enum Control {
    Input(Vec<u8>),
    Resize(TerminalSize),
    Scroll(i16),
}

struct OwnedPty {
    process: Child,
    reaped: bool,
}
impl Drop for OwnedPty {
    fn drop(&mut self) {
        if !self.reaped {
            stop_session(self.process.id() as i32);
            let _ = self.process.wait();
        }
    }
}

struct OwnedTerminal {
    state: Arc<Mutex<Screen>>,
    input: mpsc::SyncSender<Control>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<Result<(), CoreError>>>,
}
impl OwnedTerminal {
    fn join(&mut self) -> Result<(), CoreError> {
        if let Some(join) = self.join.take() {
            join.join().map_err(|_| refused())??;
        }
        Ok(())
    }
}
impl Drop for OwnedTerminal {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}

/// Narrow adapter owner used by the application supervisor, independent of
/// Location replacement. DTOs carry source identities, never OS handles.
pub struct Terminals {
    db: Db,
    live: BTreeMap<String, OwnedTerminal>,
    events: Option<tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>>,
}
impl Terminals {
    pub fn new(db: &Db) -> Result<Self, CoreError> {
        db.recover_terminals().map_err(storage)?;
        Ok(Self {
            db: db.shared_handle(),
            live: BTreeMap::new(),
            events: None,
        })
    }

    pub(crate) fn set_events(
        &mut self,
        events: &tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>,
    ) {
        self.events = Some(events.clone());
    }

    fn reap(&mut self) -> Result<(), CoreError> {
        let ended: Vec<_> = self
            .live
            .iter()
            .filter(|(_, t)| {
                t.state.lock().expect("terminal mutex").entry.state != TerminalState::Running
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ended {
            if let Some(mut t) = self.live.remove(&id) {
                t.join()?;
            }
        }
        Ok(())
    }

    pub fn inventory(&mut self, session: &SessionId) -> Result<TerminalInventory, CoreError> {
        self.reap()?;
        let selected = self.db.selected_terminal(&session.0).map_err(storage)?;
        let had_selection = selected.is_some();
        let mut entries: Vec<_> = self
            .live
            .values()
            .filter_map(|t| {
                let mut s = t.state.lock().expect("terminal mutex");
                (s.entry.target.session == *session).then(|| {
                    s.entry.foreground = foreground(s.entry.pid);
                    s.entry.clone()
                })
            })
            .collect();
        // IDs are random, inventory order is owner admission order in SQLite.
        let order = entries
            .iter()
            .map(|e| {
                self.db
                    .terminal_order(&e.target.id)
                    .map(|o| (e.target.id.clone(), o))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(storage)?;
        entries.sort_by_key(|e| order[&e.target.id]);
        let selected = entries
            .iter()
            .find(|e| Some(&e.target.id) == selected.as_ref())
            .map(|e| e.target.clone());
        if selected.is_none() && had_selection {
            self.db.select_terminal(&session.0, None).map_err(storage)?;
        }
        Ok(TerminalInventory {
            enabled: cfg!(target_os = "linux"),
            entries,
            selected,
        })
    }

    pub fn create(
        &mut self,
        source: TerminalRef,
        parent: &BTreeMap<String, String>,
        size: TerminalSize,
    ) -> Result<TerminalEntry, CoreError> {
        let shell = Shell::new(Path::new(&source.location)).map_err(|_| refused())?;
        self.create_admitted(source, parent, size, &shell)
    }

    pub(crate) fn create_admitted(
        &mut self,
        source: TerminalRef,
        parent: &BTreeMap<String, String>,
        size: TerminalSize,
        shell: &Shell,
    ) -> Result<TerminalEntry, CoreError> {
        self.reap()?;
        if self.live.len() >= TERMINAL_CAP {
            return Err(CoreError::QueueFull);
        }
        if !size.valid() || source.location.len() > 4096 || source.session.0.len() > 256 {
            return Err(refused());
        }
        // Validate the session before any descriptor or process side effect.
        self.db
            .selected_terminal(&source.session.0)
            .map_err(storage)?;
        let root = Path::new(&source.location);
        if root != shell.root() || root.starts_with(self.db.root()) {
            return Err(refused());
        }
        let argv = command_argv(parent, ":").map_err(|_| refused())?;
        let pinned = shell.pin_cwd(&argv, ".").map_err(|_| refused())?;
        if pinned.path.starts_with(self.db.root()) {
            return Err(refused());
        }
        let (master, slave) = open_pty(size)?;
        let cwd_fd = pinned.descriptor();
        let mut cmd = Command::new(&argv[0]);
        cmd.arg("-i")
            .env_clear()
            .envs(child_env(parent))
            .env("TERM", "xterm-256color");
        cmd.stdin(Stdio::from(slave.try_clone().map_err(|_| refused())?));
        cmd.stdout(Stdio::from(slave.try_clone().map_err(|_| refused())?));
        cmd.stderr(Stdio::from(slave));
        // SAFETY: only async-signal-safe syscalls in the post-fork child. The
        // pinned directory lives through spawn; fd 0 is this PTY's owned slave.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            cmd.pre_exec(move || {
                if libc::fchdir(cwd_fd) != 0
                    || libc::setsid() < 0
                    || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = cmd.spawn().map_err(|_| refused())?;
        let pid = child.id() as i32;
        let Some(identity) = ProcessIdentity::read(pid, self.db.root()) else {
            stop_session(pid);
            let _ = child.wait();
            return Err(refused());
        };
        let mut random = [0u8; 16];
        // SAFETY: writable 16-byte buffer, no uninitialized memory is exposed.
        if unsafe { libc::getrandom(random.as_mut_ptr().cast(), random.len(), 0) }
            != random.len() as isize
        {
            stop_session(pid);
            let _ = child.wait();
            return Err(refused());
        }
        let mut target = source;
        target.id = format!(
            "pty-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let entry = TerminalEntry {
            target,
            shell: argv[0].clone(),
            cwd: pinned.path.to_string_lossy().into(),
            pid,
            title: Path::new(&argv[0])
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            foreground: None,
            state: TerminalState::Running,
            exit: None,
        };
        if let Err(e) = self.db.record_terminal(&entry, &identity) {
            stop_session(pid);
            let _ = child.wait();
            return Err(storage(e));
        }
        let child = OwnedPty {
            process: child,
            reaped: false,
        };
        let state = Arc::new(Mutex::new(Screen::new(entry.clone(), size)));
        let stop = Arc::new(AtomicBool::new(false));
        let (input, rx) = mpsc::sync_channel(QUEUE_ITEMS);
        let s = state.clone();
        let stopping = stop.clone();
        let db = self.db.shared_handle();
        let events = self.events.clone();
        let join = match std::thread::Builder::new()
            .name("oc-pty".into())
            .spawn(move || drain(master, child, s, stopping, rx, db, events))
        {
            Ok(join) => join,
            Err(_) => {
                let mut failed = entry.clone();
                failed.state = TerminalState::Failed;
                self.db.finish_terminal(&failed).map_err(storage)?;
                return Err(refused());
            }
        };
        self.live.insert(
            entry.target.id.clone(),
            OwnedTerminal {
                state,
                input,
                stop,
                join: Some(join),
            },
        );
        Ok(entry)
    }

    fn target(&self, actor: &SessionId, target: &TerminalRef) -> Result<&OwnedTerminal, CoreError> {
        let t = self.live.get(&target.id).ok_or_else(refused)?;
        let state = t.state.lock().expect("terminal mutex");
        if *actor != target.session
            || state.entry.target != *target
            || state.entry.state != TerminalState::Running
        {
            return Err(refused());
        }
        drop(state);
        Ok(t)
    }

    pub fn select(&self, actor: &SessionId, target: Option<&TerminalRef>) -> Result<(), CoreError> {
        if let Some(target) = target {
            self.target(actor, target)?;
        }
        self.db.select_terminal(&actor.0, target).map_err(storage)
    }

    pub fn input(
        &self,
        actor: &SessionId,
        target: &TerminalRef,
        bytes: Vec<u8>,
    ) -> Result<(), CoreError> {
        if bytes.len() > TERMINAL_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let t = self.target(actor, target)?;
        if !t.state.lock().expect("terminal mutex").ready {
            return Err(refused());
        }
        t.input
            .try_send(Control::Input(bytes))
            .map_err(|e| match e {
                mpsc::TrySendError::Full(_) => CoreError::QueueFull,
                _ => refused(),
            })
    }

    pub fn resize(
        &self,
        actor: &SessionId,
        target: &TerminalRef,
        size: TerminalSize,
    ) -> Result<(), CoreError> {
        if !size.valid() {
            return Err(refused());
        }
        self.target(actor, target)?
            .input
            .try_send(Control::Resize(size))
            .map_err(|_| CoreError::QueueFull)
    }

    pub fn snapshot(
        &self,
        actor: &SessionId,
        target: &TerminalRef,
    ) -> Result<TerminalSnapshot, CoreError> {
        Ok(self
            .target(actor, target)?
            .state
            .lock()
            .expect("terminal mutex")
            .snapshot())
    }

    pub fn scroll(
        &self,
        actor: &SessionId,
        target: &TerminalRef,
        lines: i16,
    ) -> Result<(), CoreError> {
        if lines.unsigned_abs() > SCROLLBACK as u16 {
            return Err(refused());
        }
        self.target(actor, target)?
            .input
            .try_send(Control::Scroll(lines))
            .map_err(|_| CoreError::QueueFull)
    }

    pub fn replay(
        &self,
        actor: &SessionId,
        target: &TerminalRef,
        cursor: u64,
    ) -> Result<TerminalReplay, CoreError> {
        let t = self.target(actor, target)?;
        let s = t.state.lock().expect("terminal mutex");
        let start = s.cursor.saturating_sub(s.ring.len() as u64);
        if cursor > s.cursor {
            return Err(refused());
        }
        if cursor < start {
            return Ok(TerminalReplay {
                from: cursor,
                next: s.cursor,
                bytes: Vec::new(),
                reset: Some(s.snapshot()),
                screen: Box::new(s.snapshot()),
            });
        }
        Ok(TerminalReplay {
            from: cursor,
            next: s.cursor,
            bytes: s
                .ring
                .iter()
                .skip((cursor - start) as usize)
                .copied()
                .collect(),
            reset: None,
            screen: Box::new(s.snapshot()),
        })
    }

    pub fn remove(&mut self, actor: &SessionId, target: &TerminalRef) -> Result<(), CoreError> {
        self.target(actor, target)?;
        let mut t = self.live.remove(&target.id).ok_or_else(refused)?;
        t.stop.store(true, Ordering::Release);
        t.join()
    }

    pub fn shutdown(&mut self) -> Result<(), CoreError> {
        for t in self.live.values() {
            t.stop.store(true, Ordering::Release);
        }
        let mut failed = false;
        for (_, mut t) in std::mem::take(&mut self.live) {
            failed |= t.join().is_err();
        }
        if failed { Err(refused()) } else { Ok(()) }
    }
}
impl Drop for Terminals {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn open_pty(size: TerminalSize) -> Result<(File, File), CoreError> {
    // Atomic CLOEXEC avoids leaking the master into another concurrently spawned
    // process; slave is likewise opened CLOEXEC, without becoming our terminal.
    // SAFETY: no pointers; a new owned fd is returned on success.
    let fd = unsafe {
        libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NONBLOCK)
    };
    if fd < 0 {
        return Err(refused());
    }
    // SAFETY: successful open transfers one new descriptor.
    let master = unsafe { File::from_raw_fd(fd) };
    // SAFETY: live master descriptor, grant/unlock have no pointer arguments.
    if unsafe { libc::grantpt(fd) } != 0 {
        return Err(refused());
    }
    // SAFETY: this is the same owned master descriptor.
    if unsafe { libc::unlockpt(fd) } != 0 {
        return Err(refused());
    }
    let mut name = [0i8; 128];
    // SAFETY: correctly sized writable buffer; ptsname_r NUL terminates on success.
    if unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) } != 0 {
        return Err(refused());
    }
    // SAFETY: NUL-terminated path from ptsname_r; return is a new owned descriptor.
    let slave = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if slave < 0 {
        return Err(refused());
    }
    // SAFETY: successful open transfers one new descriptor.
    let slave = unsafe { File::from_raw_fd(slave) };
    set_size(fd, size)?;
    Ok((master, slave))
}

fn set_size(fd: i32, size: TerminalSize) -> Result<(), CoreError> {
    let winsize = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: live owned PTY descriptor and a valid winsize for the ioctl.
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize) } < 0 {
        Err(refused())
    } else {
        Ok(())
    }
}

/// Signal only groups in the still-owned terminal session. Unlike one-shot
/// shell jobs, interactive job control creates additional foreground/bg groups.
pub(crate) fn stop_session(pid: i32) {
    stop_session_if(pid, || true);
}

pub(crate) fn quarantine(identity: &ProcessIdentity, root: &Path) {
    // Unlike an unreaped direct child, a recovered leader can disappear at any
    // point. Revalidate the original starttime/boot/UID/root before each signal.
    stop_session_if(identity.pid, || identity.matches(root));
}

fn stop_session_if(pid: i32, still_owned: impl Fn() -> bool) {
    if !still_owned() {
        return;
    }
    let groups = session_groups(pid);
    for group in groups.iter().copied().filter(|g| *g != pid) {
        if !still_owned() {
            return;
        }
        // SAFETY: groups were resolved under the owned, unreaped session leader.
        unsafe {
            libc::killpg(group, libc::SIGTERM);
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    for group in session_groups(pid).into_iter().filter(|g| *g != pid) {
        if !still_owned() {
            return;
        }
        // SAFETY: still the verified leader's session, no foreign PID signalling.
        unsafe {
            libc::killpg(group, libc::SIGKILL);
        }
    }
    std::thread::sleep(Duration::from_millis(50));
    if !still_owned() {
        return;
    }
    // SAFETY: direct owned session/group leader; called before wait or after
    // ProcessIdentity comparison during recovery, never on an arbitrary saved PID.
    unsafe {
        libc::killpg(pid, libc::SIGTERM);
    }
    std::thread::sleep(Duration::from_millis(50));
    if !still_owned() {
        return;
    }
    // SAFETY: leader is still unreaped by this owner (recovery verified identity).
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
}
fn session_groups(pid: i32) -> Vec<i32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().parse::<i32>().is_err() {
            continue;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            continue;
        };
        let mut f = fields.split_whitespace();
        let _state = f.next();
        let _ppid = f.next();
        let group = f.next().and_then(|s| s.parse::<i32>().ok());
        let session = f.next().and_then(|s| s.parse::<i32>().ok());
        if session == Some(pid)
            && let Some(group) = group
            && group > 1
            && !groups.contains(&group)
        {
            groups.push(group);
        }
    }
    groups
}
fn foreground(pid: i32) -> Option<String> {
    // Linux stat tpgid identifies the actual foreground process group.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let group = stat
        .rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(5)?
        .parse::<i32>()
        .ok()?;
    let name = std::fs::read_to_string(format!("/proc/{group}/comm")).ok()?;
    Some(safe_title(name.as_bytes()))
}
fn safe_title(bytes: &[u8]) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(128)])
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect()
}

#[derive(Default)]
struct Callbacks {
    title: Option<String>,
    replies: Vec<u8>,
}
impl vt100::Callbacks for Callbacks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = Some(safe_title(title));
    }
    fn unhandled_csi(
        &mut self,
        s: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        if i1.is_some() || i2.is_some() || self.replies.len() > 2048 {
            return;
        }
        match (
            c,
            params.first().and_then(|p| p.first()).copied().unwrap_or(0),
        ) {
            ('n', 5) => self.replies.extend_from_slice(b"\x1b[0n"),
            ('n', 6) => {
                let (r, col) = s.cursor_position();
                self.replies
                    .extend_from_slice(format!("\x1b[{};{}R", r + 1, col + 1).as_bytes());
            }
            ('c', 0) => self.replies.extend_from_slice(b"\x1b[?1;2c"),
            _ => (),
        }
    }
}

/// A syntax-state gate, not text matching: string escapes are retained at most
/// 1024 bytes before vte (whose std OSC Vec otherwise has no cap). Overlong
/// sequences are discarded until ST/BEL/CAN/SUB, never exposed as visible text.
#[derive(Default)]
struct EscapeGate {
    bytes: Vec<u8>,
    kind: u8,
    escaped: bool,
    overflow: bool,
    dropped: u64,
}
impl EscapeGate {
    fn process(&mut self, parser: &mut vt100::Parser<Callbacks>, input: &[u8]) {
        for &b in input {
            if self.kind == 0 {
                if b == 0x1b {
                    self.kind = 1;
                    self.bytes.push(b);
                } else {
                    parser.process(&[b]);
                }
                continue;
            }
            if matches!(self.kind, 1 | 2) && b == 0x1b {
                self.bytes.clear();
                self.bytes.push(b);
                self.kind = 1;
                self.overflow = false;
                self.escaped = false;
                continue;
            }
            if self.bytes.len() < 1024 && !self.overflow {
                self.bytes.push(b);
            } else {
                self.overflow = true;
            }
            let done = match self.kind {
                1 => {
                    self.kind = match b {
                        b'[' => 2,
                        b']' => 3,
                        b'P' | b'_' | b'^' | b'X' => 4,
                        0x20..=0x2f => 1,
                        _ => 5,
                    };
                    self.kind == 5
                }
                2 => (0x40..=0x7e).contains(&b),
                3 | 4 => (self.kind == 3 && b == 7) || (self.escaped && b == b'\\'),
                _ => true,
            } || matches!(b, 0x18 | 0x1a);
            self.escaped = b == 0x1b;
            if done {
                if self.overflow || self.kind == 4 {
                    self.dropped += 1;
                } else {
                    parser.process(&self.bytes);
                }
                self.bytes.clear();
                self.kind = 0;
                self.overflow = false;
                self.escaped = false;
            }
        }
    }
}

struct Screen {
    entry: TerminalEntry,
    parser: vt100::Parser<Callbacks>,
    gate: EscapeGate,
    ring: VecDeque<u8>,
    cursor: u64,
    revision: u64,
    ready: bool,
}
impl Screen {
    fn new(entry: TerminalEntry, size: TerminalSize) -> Self {
        Self {
            entry,
            parser: vt100::Parser::new_with_callbacks(
                size.rows,
                size.cols,
                SCROLLBACK,
                Callbacks::default(),
            ),
            gate: EscapeGate::default(),
            ring: VecDeque::new(),
            cursor: 0,
            revision: 0,
            ready: true,
        }
    }
    fn output(&mut self, bytes: &[u8]) {
        self.gate.process(&mut self.parser, bytes);
        if let Some(title) = self.parser.callbacks_mut().title.take() {
            self.entry.title = title;
        }
        self.cursor = self.cursor.saturating_add(bytes.len() as u64);
        self.revision = self.revision.saturating_add(1);
        let excess = self
            .ring
            .len()
            .saturating_add(bytes.len())
            .saturating_sub(RING_BYTES);
        self.ring.drain(..excess.min(self.ring.len()));
        self.ring.extend(
            bytes
                .iter()
                .skip(bytes.len().saturating_sub(RING_BYTES))
                .copied(),
        );
    }
    fn snapshot(&self) -> TerminalSnapshot {
        let s = self.parser.screen();
        let (rows, cols) = s.size();
        let mut cells = Vec::with_capacity(rows as usize * cols as usize);
        let color = |c| match c {
            vt100::Color::Default => TerminalColor::Default,
            vt100::Color::Idx(i) => TerminalColor::Indexed(i),
            vt100::Color::Rgb(r, g, b) => TerminalColor::Rgb(r, g, b),
        };
        for r in 0..rows {
            for c in 0..cols {
                let cell = s.cell(r, c).expect("screen bounds");
                cells.push(TerminalCell {
                    text: cell.contents().into(),
                    wide_continuation: cell.is_wide_continuation(),
                    fg: color(cell.fgcolor()),
                    bg: color(cell.bgcolor()),
                    bold: cell.bold(),
                    dim: cell.dim(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    inverse: cell.inverse(),
                });
            }
        }
        TerminalSnapshot {
            entry: self.entry.clone(),
            size: TerminalSize { rows, cols },
            cells,
            cursor: s.cursor_position(),
            hide_cursor: s.hide_cursor() || s.scrollback() > 0,
            application_cursor: s.application_cursor(),
            bracketed_paste: s.bracketed_paste(),
            output_cursor: self.cursor,
            revision: self.revision,
            ready: self.ready,
            control_strings_dropped: self.gate.dropped,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn drain(
    mut master: File,
    mut child: OwnedPty,
    state: Arc<Mutex<Screen>>,
    stop: Arc<AtomicBool>,
    rx: mpsc::Receiver<Control>,
    db: Db,
    events: Option<tokio::sync::broadcast::Sender<oc_core::core_app::CoreEvent>>,
) -> Result<(), CoreError> {
    let mut pending = VecDeque::<u8>::new();
    let mut failed = false;
    let mut ended = false;
    let mut bytes = [0u8; 4096];
    let mut announced = 0;
    let mut announce_at = std::time::Instant::now();
    while !stop.load(Ordering::Acquire) && !ended {
        if pending.len() < PENDING_INPUT - TERMINAL_INPUT_BYTES {
            for _ in 0..4 {
                match rx.try_recv() {
                    Ok(Control::Input(bytes)) => {
                        let mut s = state.lock().expect("terminal mutex");
                        if s.parser.screen().scrollback() > 0 {
                            s.parser.screen_mut().set_scrollback(0);
                            s.revision += 1;
                        }
                        pending.extend(bytes);
                    }
                    Ok(Control::Scroll(lines)) => {
                        let mut s = state.lock().expect("terminal mutex");
                        let rows = s
                            .parser
                            .screen()
                            .scrollback()
                            .saturating_add_signed(isize::from(lines))
                            .min(SCROLLBACK);
                        s.parser.screen_mut().set_scrollback(rows);
                        s.revision += 1;
                    }
                    Ok(Control::Resize(size)) => {
                        if set_size(master.as_raw_fd(), size).is_err() {
                            failed = true;
                            ended = true;
                            break;
                        }
                        let mut s = state.lock().expect("terminal mutex");
                        s.parser.screen_mut().set_size(size.rows, size.cols);
                        s.revision += 1;
                    }
                    Err(_) => break,
                }
                if pending.len() >= PENDING_INPUT - TERMINAL_INPUT_BYTES {
                    break;
                }
            }
        }
        if !pending.is_empty() {
            match master.write(pending.make_contiguous()) {
                Ok(n) => {
                    pending.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                Err(_) => {
                    failed = true;
                    ended = true;
                }
            }
        }
        for _ in 0..16 {
            match master.read(&mut bytes) {
                Ok(0) => {
                    ended = true;
                    break;
                }
                Ok(n) => {
                    let mut s = state.lock().expect("terminal mutex");
                    s.output(&bytes[..n]);
                    let replies = std::mem::take(&mut s.parser.callbacks_mut().replies);
                    if pending.len() + replies.len() <= PENDING_INPUT {
                        pending.extend(replies);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) if e.raw_os_error() == Some(libc::EIO) => {
                    ended = true;
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    failed = true;
                    ended = true;
                    break;
                }
            }
        }
        if crate::shell::exited_without_reap(child.process.id() as i32).map_err(|_| refused())? {
            ended = true;
        }
        if let Some(events) = &events
            && std::time::Instant::now() >= announce_at
        {
            let s = state.lock().expect("terminal mutex");
            if s.revision != announced {
                announced = s.revision;
                let _ = events.send(oc_core::core_app::CoreEvent::TerminalChanged {
                    session: s.entry.target.session.clone(),
                });
                announce_at = std::time::Instant::now() + Duration::from_millis(33);
            }
        }
        if !ended {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    // The leader has not been reaped: its PID/session cannot be reused between
    // group discovery and signals, including when the shell exited naturally.
    stop_session(child.process.id() as i32);
    let exit = child.process.wait().map_err(|_| refused())?;
    child.reaped = true;
    drop(master);
    let entry = {
        let mut s = state.lock().expect("terminal mutex");
        s.ready = false;
        s.entry.exit = exit.code();
        s.entry.state = if failed {
            TerminalState::Failed
        } else if stop.load(Ordering::Acquire) {
            TerminalState::Removed
        } else {
            TerminalState::Exited
        };
        s.entry.clone()
    };
    let publication = db.finish_terminal(&entry).map_err(storage);
    if let Some(events) = events {
        let _ = events.send(oc_core::core_app::CoreEvent::TerminalChanged {
            session: entry.target.session,
        });
    }
    publication?;
    if failed { Err(refused()) } else { Ok(()) }
}

#[cfg(test)]
mod tests;
