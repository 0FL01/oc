//! T26 (UI01): PTY qualification for the real `oc tui` binary.
//!
//! The binary runs under a real PTY (libc `openpty`), driven through the
//! master side: typed input, pastes, resizes, Ctrl-C, and a panic probe.
//! Assertions inspect actual behavior — rendered bytes, exit codes, and
//! slave termios state — not render snapshots.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oc");
const POLL: Duration = Duration::from_millis(25);
const DEADLINE: Duration = Duration::from_secs(15);
/// Alternate-screen leave sequence: proof the terminal was restored.
const ALT_LEAVE: &[u8] = b"\x1b[?1049l";
const FIXTURE_MODEL: &str = "pty-unseen-model";

/// Real native Responses traffic, isolated from authoring-agent configuration.
/// Echo is scripted by this HTTP peer, never by a product mock provider.
struct Fixture {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    fn new(root: tempfile::TempDir) -> Arc<Self> {
        let home = root.path().join("home");
        let config = home.join("config/opencode");
        std::fs::create_dir_all(&config).expect("isolated config");
        std::fs::create_dir_all(root.path().join("project")).expect("isolated project");
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake endpoint");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("endpoint address");
        let configuration = serde_json::json!({
            "model": format!("fixture/{FIXTURE_MODEL}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                            "apiKey": "{env:OC_FIXTURE_KEY}"},
                "models": {FIXTURE_MODEL: {"name": "PTY fixture", "limit": {
                    "context": 32768, "output": 4096
                }}}
            }}
        });
        std::fs::write(config.join("opencode.json"), configuration.to_string())
            .expect("fixture config");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let server = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(DEADLINE))
                            .expect("read timeout");
                        socket
                            .set_write_timeout(Some(DEADLINE))
                            .expect("write timeout");
                        let body = read_request(&mut socket);
                        assert_eq!(body["model"], FIXTURE_MODEL);
                        assert_eq!(body["stream"], true);
                        let input = body["input"].as_array().expect("typed input");
                        let prompt = input
                            .iter()
                            .rev()
                            .find(|item| item["type"] == "message" && item["role"] == "user")
                            .expect("user message")["content"][0]["text"]
                            .as_str()
                            .expect("last prompt");
                        let answer = match prompt {
                            "CLI durable seed" => "first configured answer".to_string(),
                            "PTY durable followup" => "second configured answer".to_string(),
                            "CLI durable restart" => "third configured answer".to_string(),
                            _ => format!("echo: {prompt}"),
                        };
                        let slow = prompt == "cancel heartbeat probe";
                        captured.lock().expect("requests").push(body);
                        // Preserve the heartbeat regression. Silent-body and
                        // pre-header cancellation are covered by responses.rs.
                        let _ = respond(&mut socket, &answer, slow, &stopping);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
        });
        Arc::new(Self {
            root,
            requests,
            stop,
            server: Some(server),
        })
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("home/data/oc")
    }

    fn command(&self) -> Command {
        let home = self.root.path().join("home");
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OC_FIXTURE_KEY", "fixture-not-a-secret")
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(self.root.path().join("project"));
        command
    }

    fn wait_requests(&self, count: usize) -> Vec<serde_json::Value> {
        let start = Instant::now();
        loop {
            let requests = self.requests.lock().expect("requests").clone();
            if requests.len() >= count {
                return requests;
            }
            assert!(
                start.elapsed() < DEADLINE,
                "missing configured HTTP request {count}"
            );
            std::thread::sleep(POLL);
        }
    }

    fn run(&self, session: &str, prompt: &str) -> std::process::Output {
        let mut child = self
            .command()
            .args(["run", "--session", session, prompt])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("CLI process");
        let start = Instant::now();
        while child.try_wait().expect("CLI wait").is_none() {
            if start.elapsed() > DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("CLI timeout");
            }
            std::thread::sleep(POLL);
        }
        let output = child.wait_with_output().expect("CLI output");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let result = server.join();
            if !std::thread::panicking() {
                result.expect("fake endpoint assertions");
            }
        }
    }
}

fn read_request(socket: &mut TcpStream) -> serde_json::Value {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let header_end = loop {
        let n = socket.read(&mut chunk).expect("HTTP headers");
        assert_ne!(n, 0, "early EOF");
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        assert!(bytes.len() < 65_536, "bounded headers");
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).expect("headers");
    assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-not-a-secret\r\n")
    );
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().expect("length"))
        })
        .expect("content length");
    assert!(length < 262_144, "bounded request");
    while bytes.len() < header_end + length {
        let n = socket.read(&mut chunk).expect("HTTP body");
        assert_ne!(n, 0, "early body EOF");
        bytes.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&bytes[header_end..header_end + length]).expect("request JSON")
}

fn respond(
    socket: &mut TcpStream,
    answer: &str,
    slow: bool,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
    )?;
    if slow {
        socket.write_all(
            b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
        )?;
        socket.flush()?;
    }
    for _ in 0..if slow { 100 } else { 2 } {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        socket.write_all(b": heartbeat\n\n")?;
        socket.flush()?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let delta = serde_json::json!({"type": "response.output_text.delta", "delta": answer});
    let completed = serde_json::json!({"type": "response.completed", "response": {
        "status": "completed", "output": [{"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": answer}]}]
    }});
    write!(socket, "data: {delta}\n\ndata: {completed}\n\n")?;
    socket.flush()
}

fn openpty_pair(cols: u16, rows: u16) -> (OwnedFd, OwnedFd) {
    let winsize = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let mut master = -1;
    let mut slave = -1;
    // SAFETY: out-params are valid mut int pointers; null name/default
    // termios request defaults; winsize points at a live struct. Returns 0
    // with two open fds on success.
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &winsize as *const libc::winsize,
        )
    };
    assert_eq!(rc, 0, "openpty failed");
    // SAFETY: openpty succeeded, so master is an open fd we now own.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    // SAFETY: openpty succeeded, so slave is an open fd we now own.
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    (master, slave)
}

fn dup_fd(fd: &OwnedFd) -> OwnedFd {
    // SAFETY: fd is open; F_DUPFD_CLOEXEC returns a new open fd (>= 0).
    let duped = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    assert!(duped >= 0, "fcntl dup failed");
    // SAFETY: just minted by fcntl, owned from here on.
    unsafe { OwnedFd::from_raw_fd(duped) }
}

/// Live PTY session: `oc tui` attached to the slave, master driven here.
struct PtySession {
    master: std::fs::File,
    child: Child,
    output: Arc<Mutex<Vec<u8>>>,
    fixture: Arc<Fixture>,
    data_dir: PathBuf,
}

impl PtySession {
    fn spawn(cols: u16, rows: u16, term: Option<&str>, with_reader: bool) -> Self {
        let data_dir = tempfile::TempDir::new().expect("tempdir");
        Self::spawn_in(cols, rows, term, with_reader, data_dir, &[], None)
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_in(
        cols: u16,
        rows: u16,
        term: Option<&str>,
        with_reader: bool,
        data_dir: tempfile::TempDir,
        argv_extra: &[&str],
        env_extra: Option<(&str, &str)>,
    ) -> Self {
        Self::spawn_configured(
            cols,
            rows,
            term,
            with_reader,
            Fixture::new(data_dir),
            argv_extra,
            env_extra,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_configured(
        cols: u16,
        rows: u16,
        term: Option<&str>,
        with_reader: bool,
        fixture: Arc<Fixture>,
        argv_extra: &[&str],
        env_extra: Option<(&str, &str)>,
    ) -> Self {
        let (master, slave) = openpty_pair(cols, rows);
        let mut cmd = fixture.command();
        cmd.arg("tui")
            .args(argv_extra)
            .stdin(Stdio::from(dup_fd(&slave)))
            .stdout(Stdio::from(dup_fd(&slave)))
            .stderr(Stdio::from(dup_fd(&slave)));
        match term {
            Some(value) => {
                cmd.env("TERM", value);
            }
            None => {
                cmd.env_remove("TERM");
            }
        }
        if let Some((key, value)) = env_extra {
            cmd.env(key, value);
        }
        let child = cmd.spawn().expect("spawn oc tui");
        drop(slave);
        let master_file: std::fs::File = master.into();
        let output = Arc::new(Mutex::new(Vec::new()));
        if with_reader {
            let out = output.clone();
            // SAFETY: master is open; dup returns a second open fd for the
            // reader thread, closed when its File drops.
            let duped = unsafe { libc::fcntl(master_file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
            assert!(duped >= 0);
            // SAFETY: just minted by fcntl, owned from here on.
            let reader_fd = unsafe { std::fs::File::from(OwnedFd::from_raw_fd(duped)) };
            std::thread::spawn(move || {
                let mut reader = reader_fd;
                let mut chunk = [0u8; 8192];
                loop {
                    match reader.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => out.lock().expect("output").extend_from_slice(&chunk[..n]),
                    }
                }
            });
        }
        PtySession {
            master: master_file,
            child,
            output,
            data_dir: fixture.data_dir(),
            fixture,
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        self.output.lock().expect("output").clone()
    }

    /// Data dir owned by this session (for Db assertions after exit).
    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("pty write");
        self.master.flush().expect("pty flush");
    }

    /// Drain everything currently buffered on the master (non-blocking).
    /// Used when no reader thread runs (slow-consumer stall).
    fn drain(&mut self) -> Vec<u8> {
        // SAFETY: master is an open fd; F_GETFL only reads flags.
        let flags = unsafe { libc::fcntl(self.master.as_raw_fd(), libc::F_GETFL, 0) };
        assert!(flags >= 0);
        // SAFETY: master is an open fd; F_SETFL only flips O_NONBLOCK.
        let rc = unsafe {
            libc::fcntl(
                self.master.as_raw_fd(),
                libc::F_SETFL,
                flags | libc::O_NONBLOCK,
            )
        };
        assert_eq!(rc, 0);
        let mut out = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match self.master.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => out.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        out
    }

    fn wait_for(&self, needle: &[u8], timeout: Duration) -> Vec<u8> {
        let start = Instant::now();
        loop {
            let buf = self.snapshot();
            if contains(&buf, needle) {
                return buf;
            }
            if start.elapsed() > timeout {
                panic!(
                    "timeout waiting for {needle:?}; got {} bytes: {:?}",
                    buf.len(),
                    String::from_utf8_lossy(&buf[buf.len().saturating_sub(2000)..])
                );
            }
            std::thread::sleep(POLL);
        }
    }

    /// Wait until visible text *after* `from` contains `needle`. The offset
    /// defeats stale matches from earlier frames (status words repeat).
    /// Matching is whitespace-insensitive (see [`norm_visible`]).
    fn wait_visible_after(&self, from: usize, needle: &str, timeout: Duration) -> Vec<u8> {
        let want = norm_needle(needle);
        let start = Instant::now();
        loop {
            let buf = self.snapshot();
            if buf.len() >= from && contains(&norm_visible(&buf[from..]), &want) {
                return buf;
            }
            if start.elapsed() > timeout {
                let tail = &buf[from.min(buf.len())..];
                let path = std::env::temp_dir().join("pty-wait-dump.bin");
                std::fs::write(&path, &buf).ok();
                panic!(
                    "timeout waiting for visible-after {needle:?}; {} fresh bytes -> {}; norm head: {:?}",
                    tail.len(),
                    path.display(),
                    String::from_utf8_lossy(&norm_visible(&tail[..tail.len().min(800)]))
                );
            }
            std::thread::sleep(POLL);
        }
    }

    /// Wait until the visible (CSI-stripped) text contains `needle`
    /// (whitespace-insensitive).
    fn wait_visible(&self, needle: &str, timeout: Duration) -> Vec<u8> {
        self.wait_visible_after(0, needle, timeout)
    }

    fn resize(&self, cols: u16, rows: u16) {
        let winsize = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: master is an open PTY fd; TIOCSWINSZ reads the struct.
        let rc = unsafe {
            libc::ioctl(
                self.master.as_raw_fd(),
                libc::TIOCSWINSZ,
                &winsize as *const libc::winsize,
            )
        };
        assert_eq!(rc, 0, "TIOCSWINSZ failed");
    }

    /// Slave termios flags: cooked mode + echo must be back after exit.
    fn slave_lflag(&self) -> libc::c_ulong {
        // SAFETY: zeroed termios is immediately overwritten by tcgetattr.
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: master refers to the PTY pair; tcgetattr fills termios.
        let rc = unsafe { libc::tcgetattr(self.master.as_raw_fd(), &mut termios) };
        assert_eq!(rc, 0, "tcgetattr failed");
        termios.c_lflag as libc::c_ulong
    }

    fn restored(&self) -> bool {
        let lflag = self.slave_lflag();
        lflag & ((libc::ICANON | libc::ECHO) as libc::c_ulong)
            == ((libc::ICANON | libc::ECHO) as libc::c_ulong)
    }

    fn wait_exit(&mut self, timeout: Duration) -> (std::process::ExitStatus, Vec<u8>) {
        let start = Instant::now();
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => {
                    // Let the reader thread observe EOF.
                    std::thread::sleep(Duration::from_millis(200));
                    return (status, self.snapshot());
                }
                None => {
                    if start.elapsed() > timeout {
                        let _ = self.child.kill();
                        panic!("child did not exit in time");
                    }
                    std::thread::sleep(POLL);
                }
            }
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Visible text with CSI escape sequences stripped (ratatui writes titles
/// and widgets in cursor-positioned pieces, so plain phrases are only
/// contiguous after stripping).
fn visible_text(buf: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == 0x1b && i + 1 < buf.len() && buf[i + 1] == b'[' {
            i += 2;
            while i < buf.len() && !(0x40..=0x7e).contains(&buf[i]) {
                i += 1;
            }
            i += 1;
        } else {
            out.push(buf[i]);
            i += 1;
        }
    }
    out
}

/// Visible text with ASCII whitespace removed as well. Frame soup keeps
/// stale space cells and splits phrases across cursor moves, so token
/// matching is whitespace-insensitive; exact content is proven via Db.
fn norm_visible(buf: &[u8]) -> Vec<u8> {
    visible_text(buf)
        .into_iter()
        .filter(|b| !b.is_ascii_whitespace())
        .collect()
}

fn norm_needle(text: &str) -> Vec<u8> {
    text.bytes().filter(|b| !b.is_ascii_whitespace()).collect()
}

fn data_dir_with_history(messages: usize) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let project = dir.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    seed_history(
        &dir.path().join("home/data/oc"),
        &project,
        "s-long",
        messages,
    );
    dir
}

fn seed_history(data_dir: &Path, project: &Path, session: &str, messages: usize) {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    db.create_session(session).expect("session");
    // Runtime sessions are Location-bound; the old mock-only seed lacked this.
    db.set_pref(
        &format!("{}{session}", oc_adapters::runtime::SESSION_LOCATION_PREFIX),
        &project
            .canonicalize()
            .expect("project path")
            .to_string_lossy(),
    )
    .expect("session Location");
    for i in 0..messages {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        db.append_message(session, role, &seed_row(i)).expect("msg");
    }
}

/// Deterministic per-row unique text: after any scroll, newly visible rows
/// differ from previous screen content in nearly every cell, so ratatui's
/// cell diff rewrites them whole and byte needles match. Repetitive rows
/// would diff-skip shared prefixes and only digits would hit the wire.
fn seed_row(i: usize) -> String {
    let mut x = (i as u64)
        .wrapping_mul(0x9E3779B97F4A7C15)
        .wrapping_add(0x12345);
    let mut fill = String::with_capacity(48);
    for _ in 0..48 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        fill.push((b'a' + (x % 26) as u8) as char);
    }
    format!("persisted {i:05} {fill}")
}

/// Minimal screen reconstruction from a PTY byte stream: applies cursor
/// moves, writes, and clears to a grid. Byte-needle matching is fragile
/// against ratatui's cell diff (unchanged cells are skipped on the wire,
/// so scrolled rows arrive fragmented); the grid holds the true final
/// screen state and asserts exact row content with spaces intact.
struct Screen {
    cells: Vec<Vec<char>>,
}

impl Screen {
    fn blank() -> Self {
        Self { cells: Vec::new() }
    }

    fn ensure(&mut self, row: usize, col: usize) {
        while self.cells.len() <= row {
            self.cells.push(Vec::new());
        }
        if self.cells[row].len() <= col {
            self.cells[row].resize(col + 1, ' ');
        }
    }

    fn put(&mut self, row: usize, col: usize, ch: char) {
        if row >= 200 || col >= 400 {
            return;
        }
        self.ensure(row, col);
        self.cells[row][col] = ch;
    }

    fn clear_line_from(&mut self, row: usize, col: usize) {
        if row < self.cells.len() {
            for cell in self.cells[row].iter_mut().skip(col) {
                *cell = ' ';
            }
        }
    }

    fn rows(&self) -> Vec<String> {
        self.cells
            .iter()
            .map(|row| row.iter().collect::<String>().trim_end().to_string())
            .collect()
    }
}

/// Rebuild the final screen grid from raw PTY bytes.
fn render_screen(buf: &[u8]) -> Screen {
    let text = String::from_utf8_lossy(buf);
    let mut screen = Screen::blank();
    let (mut row, mut col) = (0usize, 0usize);
    let mut saved = (0usize, 0usize);
    let bytes = text.as_bytes();
    // Decoded chars with byte lengths for stepping through UTF-8.
    let chars: Vec<(char, usize)> = {
        let mut out = Vec::new();
        let mut j = 0;
        while j < bytes.len() {
            let ch = text[j..].chars().next().unwrap_or('\u{FFFD}');
            out.push((ch, ch.len_utf8()));
            j += ch.len_utf8();
        }
        out
    };
    let mut k = 0;
    while k < chars.len() {
        let (ch, _) = chars[k];
        if ch == '\x1b' {
            // CSI / OSC / single escapes.
            if k + 1 < chars.len() && chars[k + 1].0 == '[' {
                let mut p = k + 2;
                let mut params = String::new();
                while p < chars.len() && !(('@'..='\x7e').contains(&chars[p].0)) {
                    params.push(chars[p].0);
                    p += 1;
                }
                if p >= chars.len() {
                    break;
                }
                let final_ = chars[p].0;
                p += 1;
                let nums: Vec<usize> = params
                    .trim_matches(|c| c == '?' || c == ' ')
                    .split(';')
                    .filter_map(|s| s.parse().ok())
                    .collect();
                let n = nums.first().copied().unwrap_or(1).max(1);
                let m = nums.get(1).copied().unwrap_or(1).max(1);
                match final_ {
                    'H' | 'f' => {
                        row = n.saturating_sub(1);
                        col = m.saturating_sub(1);
                    }
                    'A' => row = row.saturating_sub(n),
                    'B' => row += n,
                    'C' => col += n,
                    'D' => col = col.saturating_sub(n),
                    'K' => {
                        if params.starts_with('2') {
                            if row < screen.cells.len() {
                                for cell in screen.cells[row].iter_mut() {
                                    *cell = ' ';
                                }
                            }
                        } else if params.starts_with('1') {
                            if row < screen.cells.len() {
                                let end = col.min(screen.cells[row].len());
                                for cell in screen.cells[row].iter_mut().take(end) {
                                    *cell = ' ';
                                }
                            }
                        } else {
                            screen.clear_line_from(row, col);
                        }
                    }
                    'J' => {
                        if params.starts_with('2') {
                            screen = Screen::blank();
                        } else {
                            screen.clear_line_from(row, col);
                        }
                    }
                    's' => saved = (row, col),
                    'u' => {
                        (row, col) = saved;
                    }
                    _ => {}
                }
                k = p;
                continue;
            }
            if k + 1 < chars.len() && chars[k + 1].0 == ']' {
                // OSC: skip to BEL or ESC\.
                let mut p = k + 2;
                while p < chars.len() {
                    if chars[p].0 == '\x07' {
                        p += 1;
                        break;
                    }
                    if chars[p].0 == '\x1b' && p + 1 < chars.len() && chars[p + 1].0 == '\\' {
                        p += 2;
                        break;
                    }
                    p += 1;
                }
                k = p;
                continue;
            }
            // Other escapes: \x1b7, \x1b8, \x1b=, \x1b>, \x1b(B — skip 2 chars.
            k += 2;
            continue;
        }
        match ch {
            '\r' => col = 0,
            '\n' => {
                row += 1;
            }
            _ => {
                screen.put(row, col, ch);
                col += 1;
            }
        }
        k += 1;
    }
    screen
}
/// One full-width dash run of a frame border (written contiguously).
fn dash_run(width: usize) -> Vec<u8> {
    "─".repeat(width - 2).into_bytes()
}

/// Wait until the reconstructed screen has a row containing `needle`.
/// Unlike byte needles, this sees diff-skipped cells at their true
/// positions, so scrolled rows assert exactly.
fn wait_screen_row(pty: &PtySession, needle: &str, timeout: Duration) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows.iter().any(|row| row.contains(needle)) {
            return;
        }
        if start.elapsed() > timeout {
            panic!("timeout waiting for screen row {needle:?}; screen: {rows:?}");
        }
        std::thread::sleep(POLL);
    }
}

/// Submit one turn and prove the submit rendered: the `you:` line is pushed
/// whole, so it is stable once drawn. Returns the buffer offset before send.
fn submit_turn(pty: &mut PtySession, text: &str) -> usize {
    let off = pty.snapshot().len();
    pty.send(text.as_bytes());
    pty.send(b"\r");
    pty.wait_visible_after(off, &format!("you: {text}"), DEADLINE);
    off
}

/// Settle the worker: a second single-flight turn is accepted only after the
/// previous one finished, then a pause lets the drain persist it. Without
/// this, quitting could abandon a turn whose finished event is still queued.
fn settle_turn(pty: &mut PtySession) {
    let start = Instant::now();
    let mut attempt = 0;
    loop {
        attempt += 1;
        let off = pty.snapshot().len();
        pty.send(b"zzz\r");
        let inner = Instant::now();
        loop {
            let buf = pty.snapshot();
            let fresh = &buf[off.min(buf.len())..];
            if contains(&norm_visible(fresh), &norm_needle("you: zzz")) {
                std::thread::sleep(Duration::from_secs(1));
                return;
            }
            if inner.elapsed() > Duration::from_secs(2) {
                break; // still busy: resend after the active turn drains
            }
            std::thread::sleep(POLL);
        }
        if start.elapsed() > DEADLINE {
            let buf = pty.snapshot();
            let path = std::env::temp_dir().join("pty-settle-dump.bin");
            std::fs::write(&path, &buf).ok();
            eprintln!("SETTLE DUMP {} bytes -> {}", buf.len(), path.display());
            panic!("worker never settled");
        }
        eprintln!(
            "settle attempt {attempt}: snapshot len {}",
            pty.snapshot().len()
        );
    }
}

/// Spawn with a fixed session id for exact Db assertions after exit.
fn spawn_session(session: &str) -> PtySession {
    PtySession::spawn_in(
        80,
        24,
        Some("xterm-256color"),
        true,
        tempfile::TempDir::new().expect("tempdir"),
        &["--session", session],
        None,
    )
}

/// Quit via `/quit`, expect a clean exit with a restored terminal.
fn quit_clean(pty: &mut PtySession) -> Vec<u8> {
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean quit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "cooked mode + echo restored");
    out
}

/// Exact persisted history for a session (opened only after the child
/// exited and released the data-root lock).
fn persisted(data_dir: &Path, session: &str) -> Vec<(String, String)> {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    db.read_history(session).expect("history")
}

#[test]
fn pty_smoke_type_echo_quit() {
    let mut pty = spawn_session("s-smoke");
    pty.wait_visible("Idle", DEADLINE);
    assert!(contains(&pty.snapshot(), &dash_run(80)), "80-col frame");
    submit_turn(&mut pty, "hi");
    settle_turn(&mut pty);
    let _ = quit_clean(&mut pty);
    let history = persisted(pty.data_dir(), "s-smoke");
    assert!(
        history.contains(&("user".to_string(), "hi".to_string())),
        "user turn persisted: {history:?}"
    );
    assert!(
        history.contains(&("assistant".to_string(), "echo: hi".to_string())),
        "echo turn persisted: {history:?}"
    );
}

#[test]
fn pty_unicode_and_paste_roundtrip() {
    let mut pty = spawn_session("s-uni");
    pty.wait_visible("Idle", DEADLINE);
    // Cyrillic + emoji typed as UTF-8 bytes, then a correction via Backspace.
    let off = pty.snapshot().len();
    pty.send("привет 🌍".as_bytes());
    pty.send("\x7f".as_bytes()); // Backspace drops the emoji (char-pop)
    pty.send("🌎\r".as_bytes());
    pty.wait_visible_after(off, "you: привет 🌎", DEADLINE);
    settle_turn(&mut pty);
    // Single-line paste (1500 chars) arrives as a char stream and fits the cap.
    // Chunked with pauses: crossterm reads at most 1024 bytes per edge and
    // mio is edge-triggered, so one giant write can strand the tail without
    // a fresh edge; paced chunks (like a real terminal paste) each get
    // their own edge and drain fully.
    let paste = "p".repeat(1500);
    let off = pty.snapshot().len();
    for chunk in paste.as_bytes().chunks(400) {
        pty.send(chunk);
        std::thread::sleep(Duration::from_millis(150));
    }
    pty.send(b"\r");
    pty.wait_visible_after(off, &format!("you: {}", &paste[..70]), DEADLINE);
    settle_turn(&mut pty);
    let _ = quit_clean(&mut pty);
    let history = persisted(pty.data_dir(), "s-uni");
    assert!(
        history.contains(&("user".to_string(), "привет 🌎".to_string())),
        "unicode turn exact (char-pop, no mojibake): {history:?}"
    );
    assert!(
        history.contains(&("assistant".to_string(), "echo: привет 🌎".to_string())),
        "unicode echo exact: {history:?}"
    );
    assert!(
        history.contains(&("user".to_string(), paste.clone())),
        "paste landed whole: {} msgs",
        history.len()
    );
    assert!(
        history.contains(&("assistant".to_string(), format!("echo: {paste}"))),
        "paste echo whole"
    );
}

#[test]
fn pty_resize_redraws_full_frame() {
    let mut pty = PtySession::spawn(80, 24, Some("xterm-256color"), true);
    pty.wait_for(&dash_run(80), DEADLINE);
    pty.resize(100, 30);
    pty.wait_for(&dash_run(100), DEADLINE);
    pty.resize(60, 12);
    pty.wait_for(&dash_run(60), DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    assert!(pty.restored(), "terminal restored after resizes");
}

#[test]
fn pty_tiny_screen_survives() {
    let mut pty = PtySession::spawn_in(
        40,
        8,
        Some("xterm-256color"),
        true,
        tempfile::TempDir::new().expect("tempdir"),
        &["--session", "s-tiny"],
        None,
    );
    pty.wait_visible("Idle", DEADLINE);
    submit_turn(&mut pty, "tiny");
    settle_turn(&mut pty);
    pty.send(b"\x03"); // Ctrl-C quits from idle without typing /quit
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "ctrl-c quit on tiny screen");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");
    let history = persisted(pty.data_dir(), "s-tiny");
    assert!(
        history.contains(&("assistant".to_string(), "echo: tiny".to_string())),
        "turn completed on tiny screen: {history:?}"
    );
}

#[test]
fn pty_panic_restores_terminal() {
    let mut pty = PtySession::spawn_in(
        80,
        24,
        Some("xterm-256color"),
        true,
        tempfile::TempDir::new().expect("tempdir"),
        &[],
        Some(("OC_TUI_TEST_PANIC", "1")),
    );
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(!status.success(), "probe panic exits nonzero");
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("panicked"), "panic message visible: {text:?}");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left on panic");
    assert!(pty.restored(), "cooked mode + echo restored on panic");
}

#[test]
fn pty_ssh_like_term_variants() {
    for (term, session) in [(Some("screen-256color"), "s-ssh1"), (None, "s-ssh2")] {
        let mut pty = PtySession::spawn_in(
            80,
            24,
            term,
            true,
            tempfile::TempDir::new().expect("tempdir"),
            &["--session", session],
            None,
        );
        pty.wait_visible("Idle", DEADLINE);
        submit_turn(&mut pty, "ssh");
        settle_turn(&mut pty);
        let _ = quit_clean(&mut pty);
        let history = persisted(pty.data_dir(), session);
        assert!(
            history.contains(&("assistant".to_string(), "echo: ssh".to_string())),
            "term={term:?}: {history:?}"
        );
    }
}

#[test]
fn pty_long_history_starts_and_pages() {
    let dir = data_dir_with_history(3000);
    let mut pty = PtySession::spawn_in(
        80,
        24,
        Some("xterm-256color"),
        true,
        dir,
        &["--session", "s-long"],
        None,
    );
    // Seeded tail renders: the viewport shows the newest persisted rows.
    // Row assertions use the reconstructed screen grid, not byte needles:
    // ratatui's cell diff skips unchanged cells on the wire, so scrolled
    // rows arrive fragmented while the grid holds their true content.
    wait_screen_row(&pty, &format!("user: {}", seed_row(2998)), DEADLINE);
    // Page up through real rendering: older rows scroll into view.
    for _ in 0..5 {
        pty.send(b"\x1b[A"); // Up
        std::thread::sleep(Duration::from_millis(150));
    }
    wait_screen_row(&pty, &format!("assistant: {}", seed_row(2979)), DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    assert!(pty.restored(), "terminal restored");
}

#[test]
fn pty_slow_consumer_stall_then_drain() {
    // Nobody reads while a turn streams and Ctrl-C arrives; the loop must
    // keep processing input and shut down cleanly once drained.
    let mut pty = PtySession::spawn_in(
        80,
        24,
        Some("xterm-256color"),
        false,
        tempfile::TempDir::new().expect("tempdir"),
        &["--session", "s-slow"],
        None,
    );
    std::thread::sleep(Duration::from_secs(1));
    pty.send(b"hello\r");
    std::thread::sleep(Duration::from_secs(2));
    pty.send(b"\x03"); // Ctrl-C while output sits in the PTY buffer
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "quit despite stalled consumer");
    let out = pty.drain();
    // The byte stream interleaves screen regions, so multi-part phrases are
    // proven exactly via Db below; here the restore markers must hold.
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");
    let history = persisted(pty.data_dir(), "s-slow");
    assert!(
        history.contains(&("assistant".to_string(), "echo: hello".to_string())),
        "stalled turn still completed: {history:?}"
    );
}

#[test]
fn tui_without_tty_is_usage_error() {
    // No PTY here: piped stdin must refuse before touching the terminal.
    let fixture = Fixture::new(tempfile::TempDir::new().expect("tempdir"));
    let out = fixture
        .command()
        .arg("tui")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run");
    assert!(!out.status.success(), "no-tty refuses");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no TTY"), "message: {err:?}");
}

#[test]
fn aud02_store01_persist_resume_across_restart() {
    let fixture = Fixture::new(tempfile::tempdir().expect("fixture"));
    let session = "s-aud02";
    let first = fixture.run(session, "CLI durable seed");
    assert_eq!(
        String::from_utf8_lossy(&first.stdout).trim(),
        "first configured answer"
    );
    assert!(
        fixture.data_dir().is_dir(),
        "own default XDG data namespace"
    );

    let mut pty = PtySession::spawn_configured(
        100,
        24,
        Some("xterm-256color"),
        true,
        fixture.clone(),
        &["--session", session],
        None,
    );
    wait_screen_row(&pty, "user: CLI durable seed", DEADLINE);
    wait_screen_row(&pty, "assistant: first configured answer", DEADLINE);
    submit_turn(&mut pty, "PTY durable followup");
    wait_screen_row(&pty, "second configured answer", DEADLINE);
    quit_clean(&mut pty);
    drop(pty);

    let third = fixture.run(session, "CLI durable restart");
    assert_eq!(
        String::from_utf8_lossy(&third.stdout).trim(),
        "third configured answer"
    );
    let requests = fixture.wait_requests(3);
    assert_eq!(requests.len(), 3, "one real request per process turn");
    for (index, expected) in [
        vec!["user: CLI durable seed"],
        vec![
            "user: CLI durable seed",
            "assistant: first configured answer",
            "user: PTY durable followup",
        ],
        vec![
            "user: CLI durable seed",
            "assistant: first configured answer",
            "user: PTY durable followup",
            "assistant: second configured answer",
            "user: CLI durable restart",
        ],
    ]
    .iter()
    .enumerate()
    {
        let input = &requests[index]["input"];
        let typed: Vec<_> = expected
            .iter()
            .map(|message| {
                let (role, text) = message.split_once(": ").expect("role and text");
                let kind = if role == "assistant" {
                    "output_text"
                } else {
                    "input_text"
                };
                serde_json::json!({"type": "message", "role": role,
                "content": [{"type": kind, "text": text}]})
            })
            .collect();
        assert_eq!(
            input,
            &serde_json::json!(typed),
            "durable request history at turn {index}"
        );
    }
    assert_eq!(
        persisted(&fixture.data_dir(), session),
        vec![
            ("user".into(), "CLI durable seed".into()),
            ("assistant".into(), "first configured answer".into()),
            ("user".into(), "PTY durable followup".into()),
            ("assistant".into(), "second configured answer".into()),
            ("user".into(), "CLI durable restart".into()),
            ("assistant".into(), "third configured answer".into()),
        ]
    );
}

#[test]
fn pty_escape_cancels_heartbeat_request() {
    let mut pty = spawn_session("s-cancel");
    pty.wait_visible("Idle", DEADLINE);
    submit_turn(&mut pty, "cancel heartbeat probe");
    pty.fixture.wait_requests(1);
    wait_screen_row(&pty, "ai: partial", DEADLINE);
    pty.send(b"rejected busy input\r");
    wait_screen_row(&pty, "turn busy", DEADLINE);
    // Rejected input stays in the editor; remove it before issuing /quit.
    pty.send(&[127; 19]);
    pty.send(b"\x1b"); // Esc cancels; Ctrl-C always quits in the existing key map.
    wait_screen_row(&pty, "Cancelled", DEADLINE);
    quit_clean(&mut pty);
    assert_eq!(
        persisted(pty.data_dir(), "s-cancel"),
        vec![("user".to_string(), "cancel heartbeat probe".to_string())],
        "cancelled turn preserves input without a completed assistant answer"
    );
}

#[test]
fn ui05_ndjson_stdout_only_and_slow_consumer() {
    let fixture = Fixture::new(tempfile::tempdir().expect("fixture"));
    let mut child = fixture
        .command()
        .args(["run", "--json", "json-probe"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run");
    let stdout = child.stdout.take().expect("stdout");
    use std::io::{BufRead, BufReader};
    let mut lines = Vec::new();
    for line in BufReader::new(stdout).lines() {
        std::thread::sleep(Duration::from_millis(5));
        let value: serde_json::Value = serde_json::from_str(&line.expect("line")).expect("NDJSON");
        assert!(value.get("type").is_some());
        lines.push(value);
    }
    let output = child.wait_with_output().expect("output");
    assert!(output.status.success());
    assert!(!output.stderr.is_empty());
    assert_eq!(lines.last().expect("done")["type"], "done");
    assert_eq!(lines.last().expect("answer")["text"], "echo: json-probe");
}

#[test]
fn ui05_interrupted_exit_is_nonsuccess() {
    let fixture = Fixture::new(tempfile::tempdir().expect("fixture"));
    let mut child = fixture
        .command()
        .args([
            "run",
            "--json",
            "--session",
            "s-int",
            "cancel heartbeat probe",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run");
    fixture.wait_requests(1);
    use std::io::{BufRead, BufReader};
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .expect("first delta before cancel");
    let delta: serde_json::Value = serde_json::from_str(&line).expect("NDJSON delta");
    assert_eq!(delta["type"], "delta");
    assert_eq!(delta["delta"], "partial");
    assert_eq!(
        // SAFETY: signal only the child owned by this test, while it is alive.
        unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) },
        0
    );
    let deadline = Instant::now() + DEADLINE;
    while child.try_wait().expect("wait").is_none() {
        if Instant::now() > deadline {
            child.kill().expect("kill timed-out child");
            let _ = child.wait();
            panic!("cancel timeout");
        }
        std::thread::sleep(POLL);
    }
    let output = child.wait_with_output().expect("output");
    assert_eq!(output.status.code(), Some(130));
    assert_eq!(
        persisted(&fixture.data_dir(), "s-int"),
        vec![("user".into(), "cancel heartbeat probe".into())]
    );
}
