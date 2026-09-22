//! T39 (F14): PTY qualification for panels, terminal recovery and bounded
//! backing state against the real `oc tui` binary.
//!
//! The binary runs under a real PTY with an isolated HOME and a scripted
//! native Responses peer. Assertions inspect real behaviour: rendered bytes,
//! provider request bodies, exit codes, slave termios state, durable rows and
//! the opt-in view-metrics probe — never render snapshots alone.

use std::io::{Read, Write};
#[path = "support/title.rs"]
mod title;
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_oc");
const POLL: Duration = Duration::from_millis(25);
const DEADLINE: Duration = Duration::from_secs(20);

/// Startup readiness marker. Iteration 2 of the TUI pixel-parity goal replaced
/// the `oc <status>` history-pane title (upstream has no transcript title,
/// `routes/session/index.tsx:1273-1300`); the always-visible tab-strip title is
/// the upstream fallback for a session without a title
/// (`component/session-tabs.tsx:1561`).
const READY: &str = "Untitled session";
/// Alternate-screen leave sequence: proof the terminal was restored.
const ALT_LEAVE: &[u8] = b"\x1b[?1049l";
const MODEL: &str = "pty-model";
const ALT_MODEL: &str = "alt-model";
const AGENT_PROMPT: &str = "You are the T39 fixture agent.";
const COMPRESS_PREFIX: &str = "Manual context compression request";

/// Scripted native Responses peer plus isolated HOME/config.
struct Fixture {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl Fixture {
    fn new() -> Arc<Self> {
        let root = tempfile::TempDir::new().expect("tempdir");
        let home = root.path().join("home");
        let config = home.join("config/opencode");
        std::fs::create_dir_all(&config).expect("isolated config");
        std::fs::create_dir_all(root.path().join("project")).expect("isolated project");
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake endpoint");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("endpoint address");
        let configuration = serde_json::json!({
            "model": format!("fixture/{MODEL}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                            "apiKey": "{env:OC_FIXTURE_KEY}"},
                "models": {
                    MODEL: {"name": "T39 model", "limit": {"context": 32768, "output": 4096}},
                    ALT_MODEL: {"name": "T39 alt", "limit": {"context": 32768, "output": 4096},
                                "variants": {"fast": {"reasoningEffort": "high"}}}
                }
            }},
            "agent": {
                "t39agent": {"description": "T39 fixture agent", "model": format!("fixture/{ALT_MODEL}#fast"),
                             "mode": "primary", "prompt": AGENT_PROMPT}
            },
            "command": {
                "t39cmd": {"description": "T39 custom command",
                           "template": "custom command payload for $1"}
            },
            "permissions": {"compress": "allow"},
            "dcp": {"enabled": true}
        });
        std::fs::write(config.join("opencode.json"), configuration.to_string())
            .expect("fixture config");
        let skill = config.join("skill/t39skill");
        std::fs::create_dir_all(&skill).expect("skill dir");
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: T39 skill\ndescription: T39 skill card\n---\nbody bytes\n",
        )
        .expect("skill file");
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
                        captured.lock().expect("requests").push(body.clone());
                        if title::respond(&mut socket, &body) {
                            continue;
                        }
                        let scripted = script(&body);
                        let _ = respond(&mut socket, &scripted, &stopping);
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
            // Raw capture includes titles; this accessor counts main turns.
            let requests: Vec<_> = self
                .requests
                .lock()
                .expect("requests")
                .iter()
                .filter(|r| !title::is_title(r))
                .cloned()
                .collect();
            if requests.len() >= count {
                return requests;
            }
            assert!(
                start.elapsed() < DEADLINE,
                "missing configured HTTP request {count}; fixture prompts: {:?}",
                requests.iter().map(last_user_text).collect::<Vec<_>>()
            );
            std::thread::sleep(POLL);
        }
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

/// One scripted peer answer: text, or a `compress` function call.
enum Script {
    Text(String),
    Slow(String),
    Compress(String),
}

fn script(body: &serde_json::Value) -> Script {
    if has_function_call_output(body) {
        return Script::Text("answer:compressed".to_string());
    }
    let prompt = last_user_text(body).unwrap_or_default();
    if prompt.starts_with(COMPRESS_PREFIX) {
        let anchors = dcp_anchors(body).expect("DCP anchors in the compress request");
        let first = anchors[0]["id"].as_str().expect("first anchor id");
        let last = anchors
            .iter()
            .rfind(|anchor| anchor["closed"] == serde_json::Value::Bool(true))
            .expect("a closed anchor")["id"]
            .as_str()
            .expect("closed anchor id");
        let arguments = serde_json::json!({
            "topic": "t39 span",
            "content": [{
                "startId": first,
                "endId": last,
                "summary": "Compressed early turns into one durable summary."
            }]
        })
        .to_string();
        return Script::Compress(arguments);
    }
    if prompt == "slow stream" {
        return Script::Slow("answer:slow stream".to_string());
    }
    Script::Text(format!("echo: {prompt}"))
}

fn has_function_call_output(body: &serde_json::Value) -> bool {
    body["input"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| item["type"] == "function_call_output")
    })
}

fn last_user_text(body: &serde_json::Value) -> Option<String> {
    body["input"].as_array()?.iter().rev().find_map(|item| {
        if item["type"] != "message" || item["role"] != "user" {
            return None;
        }
        match &item["content"] {
            serde_json::Value::Array(parts) => parts
                .iter()
                .find_map(|part| part["text"].as_str())
                .map(str::to_string),
            serde_json::Value::String(text) => Some(text.clone()),
            _ => None,
        }
    })
}

fn dcp_anchors(body: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    let items = body["input"].as_array()?;
    for item in items {
        let Some(parts) = item["content"].as_array() else {
            continue;
        };
        for part in parts {
            let Some(text) = part["text"].as_str() else {
                continue;
            };
            if !text.starts_with("DCP context anchors") {
                continue;
            }
            let Some(start) = text.find('[') else {
                continue;
            };
            if let Ok(serde_json::Value::Array(anchors)) =
                serde_json::from_str::<serde_json::Value>(&text[start..])
            {
                return Some(anchors);
            }
        }
    }
    None
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

fn respond(socket: &mut TcpStream, script: &Script, stop: &AtomicBool) -> std::io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n"
    )?;
    if let Script::Slow(answer) = script {
        // Heartbeats only: the turn stays open for a resize without adding
        // text that would land in the persisted answer.
        for _ in 0..40 {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            socket.write_all(b": heartbeat\n\n")?;
            socket.flush()?;
            std::thread::sleep(Duration::from_millis(50));
        }
        return finish_text(socket, answer);
    }
    match script {
        Script::Text(answer) => finish_text(socket, answer),
        Script::Compress(arguments) => finish_call(socket, "compress", arguments),
        Script::Slow(_) => unreachable!("handled above"),
    }
}

fn finish_text(socket: &mut TcpStream, answer: &str) -> std::io::Result<()> {
    let delta = serde_json::json!({"type": "response.output_text.delta", "delta": answer});
    let completed = serde_json::json!({"type": "response.completed", "response": {
        "status": "completed", "output": [{"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": answer}]}]
    }});
    write!(socket, "data: {delta}\n\ndata: {completed}\n\n")?;
    socket.flush()
}

fn finish_call(socket: &mut TcpStream, name: &str, arguments: &str) -> std::io::Result<()> {
    let call = serde_json::json!({"type": "function_call", "id": "fc_t39", "call_id": "call_t39",
        "name": name, "arguments": arguments, "status": "completed"});
    let added = serde_json::json!({"type": "response.output_item.added",
        "item": {"type": "function_call", "id": "fc_t39", "call_id": "call_t39", "name": name}});
    let args_delta = serde_json::json!({"type": "response.function_call_arguments.delta",
        "item_id": "fc_t39", "delta": arguments});
    let done =
        serde_json::json!({"type": "response.output_item.done", "output_index": 0, "item": call});
    let completed = serde_json::json!({"type": "response.completed", "response": {
        "status": "completed", "output": [call]}});
    write!(
        socket,
        "data: {added}\n\ndata: {args_delta}\n\ndata: {done}\n\ndata: {completed}\n\n"
    )?;
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
    // termios request defaults; winsize points at a live struct.
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
    fn spawn(fixture: Arc<Fixture>, session: &str, metrics: Option<&Path>) -> Self {
        let (master, slave) = openpty_pair(80, 24);
        let mut cmd = fixture.command();
        cmd.arg("tui")
            .args(["--session", session])
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(dup_fd(&slave)))
            .stdout(Stdio::from(dup_fd(&slave)))
            .stderr(Stdio::from(dup_fd(&slave)));
        if let Some(path) = metrics {
            cmd.env("OC_TUI_TEST_METRICS", path);
        }
        let child = cmd.spawn().expect("spawn oc tui");
        drop(slave);
        Self::finish_spawn(master, child, fixture)
    }

    /// Spawn with a stdout that cannot be written (closed pipe read end):
    /// the renderer must fail visibly and still restore the terminal.
    fn spawn_bad_stdout(fixture: Arc<Fixture>, session: &str) -> Self {
        let (master, slave) = openpty_pair(80, 24);
        let mut pipe = [0; 2];
        // SAFETY: pipe is a two-element array; pipe(2) fills both fds.
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        // SAFETY: pipe[0]/pipe[1] are open fds from pipe(2).
        let read_end = unsafe { OwnedFd::from_raw_fd(pipe[0]) };
        // SAFETY: see above.
        let write_end = unsafe { OwnedFd::from_raw_fd(pipe[1]) };
        drop(read_end);
        let mut cmd = fixture.command();
        cmd.arg("tui")
            .args(["--session", session])
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(dup_fd(&slave)))
            .stdout(Stdio::from(write_end))
            .stderr(Stdio::from(dup_fd(&slave)));
        let child = cmd.spawn().expect("spawn oc tui");
        drop(slave);
        Self::finish_spawn(master, child, fixture)
    }

    fn finish_spawn(master: OwnedFd, child: Child, fixture: Arc<Fixture>) -> Self {
        let master_file: std::fs::File = master.into();
        let output = Arc::new(Mutex::new(Vec::new()));
        let out = output.clone();
        // SAFETY: master is open; dup returns a second open fd for the reader
        // thread, closed when its File drops.
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
        Self {
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

    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("pty write");
        self.master.flush().expect("pty flush");
    }

    fn wait_visible(&self, needle: &str, timeout: Duration) -> Vec<u8> {
        self.wait_visible_after(0, needle, timeout)
    }

    /// Wait until visible text *after* `from` contains `needle`
    /// (whitespace-insensitive, CSI stripped).
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
                let requests = self.fixture.requests.lock().expect("requests");
                let last = requests.last().and_then(last_user_text).unwrap_or_default();
                panic!(
                    "timeout waiting for visible-after {needle:?}; {} fresh bytes; head: {:?}; \
                     requests={} last_prompt={:?}",
                    tail.len(),
                    String::from_utf8_lossy(&norm_visible(&tail[..tail.len().min(600)])),
                    requests.len(),
                    last
                );
            }
            std::thread::sleep(POLL);
        }
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
    fn restored(&self) -> bool {
        // SAFETY: zeroed termios is immediately overwritten by tcgetattr.
        let mut termios: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: master refers to the PTY pair; tcgetattr fills termios.
        let rc = unsafe { libc::tcgetattr(self.master.as_raw_fd(), &mut termios) };
        assert_eq!(rc, 0, "tcgetattr failed");
        let wanted = (libc::ICANON | libc::ECHO) as libc::c_ulong;
        (termios.c_lflag as libc::c_ulong) & wanted == wanted
    }

    fn wait_exit(&mut self, timeout: Duration) -> (std::process::ExitStatus, Vec<u8>) {
        let start = Instant::now();
        loop {
            match self.child.try_wait().expect("try_wait") {
                Some(status) => {
                    std::thread::sleep(Duration::from_millis(200));
                    return (status, self.snapshot());
                }
                None => {
                    if start.elapsed() > timeout {
                        let buf = self.snapshot();
                        let tail = &buf[buf.len().saturating_sub(1200)..];
                        let _ = self.child.kill();
                        panic!(
                            "child did not exit in time; tail: {:?}",
                            String::from_utf8_lossy(&visible_text(tail))
                        );
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

/// Minimal screen reconstruction from a PTY byte stream: applies cursor
/// moves, writes, and clears to a grid. Byte-needle matching is fragile
/// against ratatui's cell diff (unchanged cells are skipped on the wire), so
/// panel rows are asserted against the true final screen state.
struct Screen {
    cells: Vec<Vec<char>>,
    cursor: (usize, usize),
}

impl Screen {
    fn blank() -> Self {
        Self {
            cells: Vec::new(),
            cursor: (0, 0),
        }
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
    screen.cursor = (row, col);
    screen
}

/// Wait until the reconstructed screen has a row containing `needle`.
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

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// Visible text with CSI escape sequences stripped.
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

/// Visible text with ASCII whitespace removed (ratatui splits phrases across
/// cursor moves, so token matching is whitespace-insensitive).
fn norm_visible(buf: &[u8]) -> Vec<u8> {
    visible_text(buf)
        .into_iter()
        .filter(|b| !b.is_ascii_whitespace())
        .collect()
}

fn norm_needle(text: &str) -> Vec<u8> {
    text.bytes().filter(|b| !b.is_ascii_whitespace()).collect()
}

/// Needle for the first rendered row of an upstream user block (short word
/// prefix that always fits the first wrapped row).
fn user_needle(text: &str) -> String {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return "┃".to_string();
    };
    let first: String = first.chars().take(24).collect();
    let mut needle = format!("┃  {first}");
    if first.chars().count() < 24
        && let Some(second) = words.next()
    {
        needle.push(' ');
        needle.extend(second.chars().take(16));
    }
    needle
}

/// Submit one prompt and prove it rendered. Row assertions use the
/// reconstructed screen grid: ratatui's cell diff skips unchanged cells on
/// the wire, so raw byte needles are unreliable for new rows.
fn submit(pty: &mut PtySession, text: &str) -> usize {
    wait_idle(pty);
    let off = pty.snapshot().len();
    pty.send(text.as_bytes());
    pty.send(b"\r");
    wait_screen_row(pty, &user_needle(text), DEADLINE);
    off
}

/// Echo bytes may be drawn before TurnFinished. Wait for the actual terminal
/// status before starting the next idle action, rather than racing that event.
fn wait_idle(pty: &PtySession) {
    let start = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("esc interrupt") || row.contains("submission pending"))
    {
        assert!(start.elapsed() < DEADLINE, "turn did not become idle");
        std::thread::sleep(POLL);
    }
}

fn persisted(data_dir: &Path, session: &str) -> Vec<(String, String)> {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    db.read_history(session).expect("history")
}

fn saved_selection(
    db: &oc_adapters::storage::Db,
    fixture: &Fixture,
    session: &str,
    agent: &str,
) -> serde_json::Value {
    let key = format!(
        "tui.selection.session:{}",
        serde_json::json!([
            fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
            "fixture",
            session
        ])
    );
    let record: serde_json::Value =
        serde_json::from_str(&db.get_pref(&key).unwrap().unwrap()).unwrap();
    record["models"][agent].clone()
}

fn dismissed(pty: &PtySession, title: &str) {
    let start = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains(title))
    {
        assert!(start.elapsed() < DEADLINE, "modal did not dismiss: {title}");
        std::thread::sleep(POLL);
    }
}

fn choose_model(pty: &mut PtySession, title: &str) {
    wait_idle(pty);
    pty.send(b"/model\r");
    wait_screen_row(pty, "Select model", DEADLINE);
    pty.send(title.as_bytes());
    wait_screen_row(pty, title, DEADLINE);
    pty.send(b"\r");
    dismissed(pty, "Select model");
}

fn choose_variant(pty: &mut PtySession, title: &str) {
    wait_screen_row(pty, "Select variant", DEADLINE);
    pty.send(title.as_bytes());
    pty.send(b"\r");
    dismissed(pty, "Select variant");
}

/// SGR mouse protocol: terminal reports one-based x/y; Crossterm maps to cells.
fn mouse_click(pty: &mut PtySession, x: u16, y: u16) {
    pty.send(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes());
}

fn wait_cursor(pty: &PtySession, wanted: (usize, usize)) {
    let start = Instant::now();
    while render_screen(&pty.snapshot()).cursor != wanted {
        assert!(
            start.elapsed() < DEADLINE,
            "cursor did not reach {wanted:?}"
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn v05_raw_unicode_multiline_focus_and_one_durable_submit() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-unicode", None);
    pty.wait_visible(READY, DEADLINE);
    // UTF-8 combining character, a multi-codepoint ZWJ grapheme and Cyrillic.
    pty.send("привет е\u{301}🧑‍💻 мир".as_bytes());
    wait_screen_row(&pty, "мир", DEADLINE);
    // Left over " мир", backspace removes the whole emoji, then reinsert it.
    let original_cursor = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[D\x1b[D\x1b[D\x1b[D");
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 4));
    pty.send(b"\x7f");
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 6));
    pty.send("🧑‍💻".as_bytes());
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 4));
    // Insert a genuine multiline break with raw Ctrl+J in the middle.
    pty.send(b"\x0a");
    pty.send("вторая".as_bytes());
    wait_screen_row(&pty, "вторая", DEADLINE);
    pty.send(b"\x1b[13;2u"); // Shift+Enter in terminals supporting CSI-u
    pty.send("третья".as_bytes());
    wait_screen_row(&pty, "третья", DEADLINE);
    let before_paste = render_screen(&pty.snapshot()).cursor;
    pty.send("\x1b[200~ из пасты\x1b[201~".as_bytes());
    wait_screen_row(&pty, "из пасты", DEADLINE);
    pty.send(b"\x1b[45;5u"); // CSI-u Ctrl+- undoes the whole paste
    wait_cursor(&pty, before_paste);
    let selected_from = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[1;2D"); // select the last grapheme
    wait_cursor(&pty, (selected_from.0, selected_from.1 - 1));
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, (selected_from.0, selected_from.1 - 1));
    pty.send(b"\x7f"); // delete retained selection, then restore it
    pty.send("я".as_bytes());
    wait_screen_row(&pty, "третья", DEADLINE);
    let second_line_cursor = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[A\x1b[H"); // navigate the draft without scrolling history
    let start = Instant::now();
    while render_screen(&pty.snapshot()).cursor == second_line_cursor {
        assert!(
            start.elapsed() < DEADLINE,
            "editor did not move inside first line"
        );
        std::thread::sleep(POLL);
    }
    let before = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, before);
    pty.send(b"\x1b[13;1:3u"); // release of Enter must not submit
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r"); // a second Enter while pending/streaming cannot duplicate
    let requests = fixture.wait_requests(1);
    let prompt = last_user_text(&requests[0]).expect("provider prompt");
    assert!(
        prompt.contains("привет е\u{301}"),
        "combining grapheme: {prompt:?}"
    );
    assert!(prompt.contains("🧑‍💻"), "ZWJ grapheme: {prompt:?}");
    assert!(prompt.contains("вторая"), "multiline prompt: {prompt:?}");
    assert!(
        prompt.contains("вторая\nтретья"),
        "CSI-u Shift+Enter: {prompt:?}"
    );
    assert!(
        !prompt.contains("из пасты"),
        "paste undo must be atomic: {prompt:?}"
    );
    assert!(
        prompt.contains('\n'),
        "newline on provider wire: {prompt:?}"
    );
    wait_screen_row(&pty, "echo:", DEADLINE);
    wait_idle(&pty);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    // Up at the boundary recalls the last durable prompt; Down restores the
    // unfinished draft. Neither navigation path submits a second turn.
    pty.send(b"unfinished");
    wait_screen_row(&pty, "unfinished", DEADLINE);
    pty.send(b"\x1b[A");
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("unfinished"))
    {
        assert!(started.elapsed() < DEADLINE, "Up did not recall history");
        std::thread::sleep(POLL);
    }
    pty.send(b"\x1b[B");
    wait_screen_row(&pty, "unfinished", DEADLINE);
    pty.send(b"\x03");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
    let rows = persisted(pty.data_dir(), "v05-unicode");
    assert_eq!(
        rows.iter().filter(|(role, _)| role == "user").count(),
        1,
        "{rows:?}"
    );
    assert_eq!(
        rows.iter().find(|(role, _)| role == "user").unwrap().1,
        prompt
    );
}

#[test]
fn v05_raw_capped_bracket_paste_is_visible_and_not_submitted() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-paste", None);
    pty.wait_visible(READY, DEADLINE);
    let size = oc_core::session::MAX_INPUT_BYTES;
    // One bracketed-paste event exceeding the application budget. No Enter.
    pty.send(b"\x1b[200~");
    pty.send(&vec![b'x'; size + 13]);
    pty.send(b"\x1b[201~");
    pty.wait_visible("paste truncated: 13 bytes dropped", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
    assert!(persisted(pty.data_dir(), "v05-paste").is_empty());
}

#[test]
fn v05_raw_paste_chip_keeps_original_on_wire_and_in_history() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-paste-chip", None);
    pty.wait_visible(READY, DEADLINE);
    let pasted = "е\u{301}🧑‍💻\nвторая\nтретья";
    pty.send(b"prefix ");
    pty.send(format!("\x1b[200~{pasted}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("вторая"))
    );
    let end = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, end);
    pty.send(b"\x1b[1;2D"); // select the chip
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, (end.0, end.1 - "[Pasted ~3 lines] ".len()));
    pty.send(b"\x7f"); // saved selection deletes the entire chip
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("[Pasted ~3 lines]"))
    {
        assert!(started.elapsed() < DEADLINE, "chip not deleted");
        std::thread::sleep(POLL);
    }
    pty.send(b"\x1b[45;5u"); // undo restores the original bytes AND the chip
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    pty.send(b"\x1b[D"); // move over the entire chip
    pty.send(b"X"); // insertion before chip shifts its mapped byte range
    wait_screen_row(&pty, "X[Pasted ~3 lines]", DEADLINE);
    let long = "z".repeat(151);
    pty.send(format!("\x1b[200~{long}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~1 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r");
    let requests = fixture.wait_requests(1);
    let expected = format!("prefix X{long}{pasted}");
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some(expected.as_str())
    );
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let rows = persisted(pty.data_dir(), "v05-paste-chip");
    assert_eq!(rows.iter().filter(|(role, _)| role == "user").count(), 1);
    assert_eq!(
        rows.iter().find(|(role, _)| role == "user").unwrap().1,
        expected
    );
}

#[test]
fn v05_review_raw_chip_trim_matches_visible_draft_and_wire() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-chip-trim", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x1b[200~a\nb\nc\n\x1b[201~");
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("paste chip trimmed")),
        "trimming hidden terminal paste whitespace must be observable"
    );
    pty.send(b"Z\r\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(last_user_text(&requests[0]).as_deref(), Some("a\nb\ncZ"));
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(pty.data_dir(), "v05-chip-trim")
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["a\nb\ncZ"]
    );
}

#[test]
fn v05_review_raw_history_edit_down_restores_chip_and_wire() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-history-chip", 1);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-history-chip", None);
    pty.wait_visible(READY, DEADLINE);
    let draft = "draft\nline\nchip";
    pty.send(format!("\x1b[200~{draft}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    pty.send(b"\x1b[A");
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("[Pasted ~3 lines]"))
    {
        assert!(
            started.elapsed() < DEADLINE,
            "Up did not recall stored prompt"
        );
        std::thread::sleep(POLL);
    }
    pty.send(b" edited\x1b[B");
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(last_user_text(&requests[0]).as_deref(), Some(draft));
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let persisted = persisted(pty.data_dir(), "v05-history-chip");
    assert_eq!(
        persisted.iter().filter(|(role, _)| role == "user").count(),
        2
    );
    assert!(
        persisted
            .iter()
            .any(|(role, text)| role == "user" && text == "seeded row 00000 payload")
    );
    assert!(
        !persisted
            .iter()
            .any(|(role, text)| role == "user" && text.contains("edited"))
    );
    assert_eq!(
        persisted
            .iter()
            .rfind(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str()),
        Some(draft)
    );
}

#[test]
fn v05_review_empty_editor_up_uses_history_on_wire() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-empty-up", 1);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-empty-up", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x1b[A\r"); // empty editor: Up recalls, Enter submits it
    let requests = fixture.wait_requests(1);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("seeded row 00000 payload")
    );
    wait_idle(&pty);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(
        persisted(pty.data_dir(), "v05-empty-up")
            .iter()
            .filter(|(role, _)| role == "user")
            .count(),
        2
    );
}

#[test]
fn v05_review_wheel_scroll_does_not_move_multiline_editor_caret() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-wheel", 70);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-wheel", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send("\x1b[200~draft 🧑‍💻\nстрока\x1b[201~".as_bytes());
    wait_screen_row(&pty, "строка", DEADLINE);
    let caret = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[<64;1;1M"); // real xterm SGR wheel, not a keyboard Up
    wait_screen_row(&pty, "Jump to latest", DEADLINE);
    wait_cursor(&pty, caret);
    pty.send(b"\x1b[<65;1;1M");
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("Jump to latest"))
    {
        assert!(
            started.elapsed() < DEADLINE,
            "wheel down did not repin transcript"
        );
        std::thread::sleep(POLL);
    }
    wait_cursor(&pty, caret);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(persisted(pty.data_dir(), "v05-wheel").len(), 70);
}

#[test]
fn v05_review_shift_home_end_select_buffer_on_provider_wire() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-buffer-edges", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send("\x1b[200~один\nдва\x1b[201~".as_bytes());
    wait_screen_row(&pty, "два", DEADLINE);
    pty.send(b"\x1b[1;2H"); // Shift+Home selects from end to buffer start
    pty.send("новый".as_bytes());
    pty.send(b"\r");
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some("новый")
    );
    wait_idle(&pty);
    pty.send("\x1b[200~первый\nвторой\x1b[201~".as_bytes());
    wait_screen_row(&pty, "второй", DEADLINE);
    pty.send(b"\x1b[A\x1b[F"); // end of the first logical line
    pty.send(b"\x1b[1;2F"); // Shift+End selects through buffer end
    pty.send(b"X\r");
    let requests = fixture.wait_requests(2);
    assert_eq!(last_user_text(&requests[1]).as_deref(), Some("первыйX"));
    wait_idle(&pty);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(
        persisted(pty.data_dir(), "v05-buffer-edges")
            .iter()
            .filter(|(role, _)| role == "user")
            .count(),
        2
    );
}

#[test]
fn v04_raw_sgr_mouse_backdrop_search_variant_and_actual_model() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-mouse", None);
    pty.wait_visible(READY, DEADLINE);
    assert!(
        contains(&pty.snapshot(), b"\x1b[?1006h"),
        "SGR capture enabled"
    );
    pty.send(b"draft-mouse");
    wait_screen_row(&pty, "draft-mouse", DEADLINE);
    let editor_cursor = render_screen(&pty.snapshot()).cursor;

    // Open the model selector, then click the backdrop. The same release must
    // not hit the underlying editor, and Esc after closing must not be needed.
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_cursor(&pty, (9, 14));
    mouse_click(&mut pty, 1, 1);
    dismissed(&pty, "Select model");
    wait_cursor(&pty, editor_cursor);
    assert!(pty.child.try_wait().unwrap().is_none());

    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    mouse_click(&mut pty, 1, 1);
    dismissed(&pty, "Commands");
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    // 80x24: modal x=10..70, top=6; Search y=9, first filtered option y=11.
    mouse_click(&mut pty, 20, 10);
    pty.send(b"T39 alt");
    wait_cursor(&pty, (9, 21));
    assert!(render_screen(&pty.snapshot()).rows()[9].contains("T39 alt"));
    mouse_click(&mut pty, 20, 12);
    wait_screen_row(&pty, "Select variant", DEADLINE);
    // Upstream replaces Model with Variant after the accepted model change.
    pty.send(b"\x1b");
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-mouse", DEADLINE);
    wait_cursor(&pty, editor_cursor);
    assert!(pty.child.try_wait().unwrap().is_none());
    pty.send(b"\x10Switch model variant\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    // No category heading: Default at y=11 and fast at y=12.
    mouse_click(&mut pty, 20, 13);
    dismissed(&pty, "Select variant");
    pty.send(b"\r");
    pty.wait_visible("echo: draft-mouse", DEADLINE);
    let request = fixture.wait_requests(1);
    assert_eq!(request[0]["model"], ALT_MODEL);
    assert_eq!(request[0]["reasoning"]["effort"], "high");
    assert_eq!(last_user_text(&request[0]).as_deref(), Some("draft-mouse"));
    wait_idle(&pty);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, b"\x1b[?1006l"), "SGR capture restored");
    let db = oc_adapters::storage::Db::open(pty.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, "v04-mouse", "")["variant"],
        "fast"
    );
    assert_eq!(
        db.read_history("v04-mouse")
            .unwrap()
            .iter()
            .filter(|(role, text)| role == "user" && text == "draft-mouse")
            .count(),
        1
    );
}

#[test]
fn v04_raw_mouse_wheel_and_hover_select_beyond_visible_rows() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for i in 0..18 {
        config["provider"]["fixture"]["models"][format!("mouse-{i:02}")] = serde_json::json!({
            "name": format!("Mouse {i:02}"), "limit": {"context":32768,"output":4096}
        });
    }
    std::fs::write(&path, config.to_string()).unwrap();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-wheel", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "Mouse 00", DEADLINE);
    // Mouse scroll preserves the selection; row 11 moves from mouse-00 to
    // mouse-12 after four 3-row wheels, without scrolling the underlay.
    for _ in 0..4 {
        pty.send(b"\x1b[<65;20;12M");
    }
    wait_screen_row(&pty, "Mouse 12", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Mouse 00"))
    );
    pty.send(b"\x1b[<35;20;12M"); // SGR pointer movement, first scrolled row
    pty.send(b"\r"); // focused row, no extra pointer release or double activation
    dismissed(&pty, "Select model");
    let off = submit(&mut pty, "wheel hover chosen");
    pty.wait_visible_after(off, "echo: wheel hover chosen", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["model"], "mouse-12");
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[test]
fn v04_retired_model_and_variant_remain_visible_until_explicit_remediation() {
    for retired_model in [true, false] {
        let fixture = Fixture::new();
        let path = fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["none"] =
            serde_json::json!({"reasoningEffort":"low"});
        std::fs::write(&path, config.to_string()).unwrap();
        let mut pty = PtySession::spawn(fixture.clone(), "retired", None);
        pty.wait_visible(READY, DEADLINE);
        choose_model(&mut pty, "T39 alt");
        choose_variant(&mut pty, "fast");
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let before = {
            let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
            saved_selection(&db, &fixture, "retired", "")
        };
        assert_eq!(before, serde_json::json!({"id":ALT_MODEL,"variant":"fast"}));
        if retired_model {
            config["provider"]["fixture"]["models"]
                .as_object_mut()
                .unwrap()
                .remove(ALT_MODEL);
        } else {
            config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["fast"]["disabled"] =
                true.into();
        }
        std::fs::write(&path, config.to_string()).unwrap();
        let mut pty = PtySession::spawn(
            fixture.clone(),
            if retired_model { "retired" } else { "healthy" },
            None,
        );
        if !retired_model {
            pty.wait_visible(READY, DEADLINE);
            pty.send(b"/continue\r");
            wait_screen_row(&pty, "Switch session", DEADLINE);
            pty.send(b"retired\r");
            dismissed(&pty, "Switch session");
        }
        wait_screen_row(&pty, "unavailable", DEADLINE);
        if !retired_model {
            wait_screen_row(&pty, "fast (unavailable)", DEADLINE);
            pty.send(b"/variants\r");
            wait_screen_row(&pty, "fast unavailable", DEADLINE);
            assert!(
                !render_screen(&pty.snapshot())
                    .rows()
                    .iter()
                    .any(|row| row.contains("● Default")),
                "retired fast must not show Default as selected"
            );
            pty.send(b"\x1b");
            dismissed(&pty, "Select variant");
        }
        assert!(fixture.requests.lock().unwrap().is_empty());
        pty.send(b"blocked draft\r");
        wait_screen_row(&pty, "select an admitted replacement", DEADLINE);
        wait_screen_row(&pty, "blocked draft", DEADLINE);
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "retired selection never submits"
        );
        pty.send(&vec![0x7f; "blocked draft".len()]);
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            saved_selection(&db, &fixture, "retired", ""),
            before,
            "read and refused submit do not rewrite prefs"
        );
        assert!(db.read_history("retired").unwrap().is_empty());
        drop(db);
        let mut pty = PtySession::spawn(fixture.clone(), "retired", None);
        wait_screen_row(&pty, "unavailable", DEADLINE);
        if retired_model {
            choose_model(&mut pty, "T39 model");
        } else {
            pty.send(b"/variants\r");
            choose_variant(&mut pty, "Default");
        }
        let off = submit(&mut pty, "accepted after replacement");
        pty.wait_visible_after(off, "echo: accepted after replacement", DEADLINE);
        let request = fixture.wait_requests(1);
        assert_eq!(
            request[0]["model"],
            if retired_model { MODEL } else { ALT_MODEL }
        );
        assert!(
            request[0]["reasoning"]["effort"].is_null(),
            "explicit replacement cleared retired overlay"
        );
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            saved_selection(&db, &fixture, "retired", ""),
            serde_json::json!({"id":if retired_model {MODEL} else {ALT_MODEL},"variant":null})
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn v04_legacy_headless_api_supersedes_older_scoped_session_drafts() {
    use oc_core::queries::SessionSelectionAction as Action;
    let fixture = Fixture::new();
    let home = fixture.root.path().join("home");
    let env = std::collections::BTreeMap::from([
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home.join("data").to_string_lossy().into_owned(),
        ),
        ("OC_FIXTURE_KEY".to_string(), "fixture-not-a-secret".into()),
        ("OC_TEST_ALLOW_LOOPBACK".to_string(), "1".into()),
    ]);
    let project = fixture.root.path().join("project");
    let session = oc_core::domain::SessionId("scoped-headless".into());
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &fixture.data_dir(), env.clone())
            .await
            .unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.session_selection(session.clone(), false, Action::Current)
        .await
        .unwrap();
    app.session_selection(session.clone(), false, Action::Model(ALT_MODEL.into()))
        .await
        .unwrap();
    app.session_selection(session.clone(), false, Action::Variant(Some("fast".into())))
        .await
        .unwrap();
    let home_session = oc_core::domain::SessionId("old-home".into());
    app.create_session(home_session.clone()).await.unwrap();
    app.session_selection(home_session, true, Action::Model(ALT_MODEL.into()))
        .await
        .unwrap();
    let snapshot = app.select_model(MODEL.into(), None).await.unwrap();
    assert_eq!(snapshot.model_id, MODEL);
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .model_id,
        MODEL
    );
    let fresh = oc_core::domain::SessionId("fresh-headless".into());
    app.create_session(fresh.clone()).await.unwrap();
    assert_eq!(
        app.session_selection(fresh, false, Action::Current)
            .await
            .unwrap()
            .model_id,
        MODEL,
        "older Home draft cannot mask explicit headless selection in a fresh session"
    );
    let mut events = app.subscribe();
    let turn = app
        .submit(session.clone(), "legacy model".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy turn failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(1);
    assert_eq!(request[0]["model"], MODEL);
    assert!(request[0]["reasoning"]["effort"].is_null());
    let snapshot = app
        .select_model(ALT_MODEL.into(), Some("fast".into()))
        .await
        .unwrap();
    assert_eq!(snapshot.model_id, ALT_MODEL);
    assert_eq!(snapshot.variant.as_deref(), Some("fast"));
    let turn = app
        .submit(session.clone(), "legacy variant".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy variant failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(2);
    assert_eq!(request[1]["model"], ALT_MODEL);
    assert_eq!(request[1]["reasoning"]["effort"], "high");
    let snapshot = app.select_agent("t39agent".into()).await.unwrap();
    assert_eq!(snapshot.agent_id.as_deref(), Some("t39agent"));
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .model_id,
        ALT_MODEL
    );
    let turn = app
        .submit(session.clone(), "legacy agent".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy agent failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(3);
    assert_eq!(request[2]["model"], ALT_MODEL);
    assert_eq!(request[2]["reasoning"]["effort"], "high");
    assert!(request[2]["input"].to_string().contains(AGENT_PROMPT));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, &session.0, ""),
        serde_json::json!({"id":ALT_MODEL,"variant":"fast"}),
        "legacy API does not erase old scoped records"
    );
    drop(db);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &fixture.data_dir(), env)
            .await
            .unwrap();
    assert_eq!(
        app.session_selection(session, false, Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("t39agent"),
        "legacy precedence survives restart"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

fn seed_session(data_dir: &Path, project: &Path, session: &str, messages: usize) {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    db.create_session(session).expect("session");
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
        db.append_message(session, role, &format!("seeded row {i:05} payload"))
            .expect("msg");
    }
}

fn seed_tool_ops(data_dir: &Path, session: &str, ops: usize) {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    for i in 0..ops {
        let op = format!("op-{i:04}");
        db.record_tool_intent(&op, session, None, &format!("tool-{i:04}"), "{}")
            .expect("intent");
        db.record_tool_outcome(&op, "completed", Some(&format!("result-{i:04}")))
            .expect("outcome");
    }
}

/// AUD29: panels reach the runtime and really change the next request.
#[test]
fn v04_raw_dialogs_preserve_draft_and_select_normal_provider_model_variant() {
    let dismissed = |pty: &PtySession, title: &str| {
        let start = Instant::now();
        while render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains(title))
        {
            assert!(start.elapsed() < DEADLINE, "modal did not dismiss: {title}");
            std::thread::sleep(POLL);
        }
    };
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for i in 0..30 {
        config["provider"]["fixture"]["models"][format!("modal-{i:02}")] = serde_json::json!({
            "name":format!("Modal {i:02}"), "limit":{"context":32768,"output":4096},
            // Named `none` is an explicit overlay, NOT the absence of a selection.
            "variants":{"none":{"reasoningEffort":"low"},"fast":{"reasoningEffort":"high"}}
        });
    }
    std::fs::write(&path, config.to_string()).unwrap();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"draft-kept");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\x10"); // actual Ctrl+P, never a direct TuiState call
    wait_screen_row(&pty, "Commands", DEADLINE);
    wait_screen_row(&pty, "Search", DEADLINE);
    pty.send(b"Switch model");
    wait_screen_row(&pty, "Switch model", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"missing-no-results");
    wait_screen_row(&pty, "No results found", DEADLINE);
    pty.send(b"\r"); // zero-result Enter has no application effect
    pty.send(b"\x03"); // clear query only
    wait_screen_row(&pty, "Modal 00", DEADLINE);
    pty.send(b"Modal");
    pty.send(b"\x1b[F"); // beyond the viewport, not just the first eight
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Modal 00"))
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "Esc dismisses, not quits"
    );
    let dismissed_screen = render_screen(&pty.snapshot()).rows();
    assert!(
        dismissed_screen
            .iter()
            .any(|r| r.contains("1 Untitled session")),
        "Esc preserves attached tab"
    );
    assert!(
        dismissed_screen
            .iter()
            .any(|r| r.contains("T39 model fixture")),
        "Esc preserves the effective model"
    );
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "browsing does not submit"
    );
    pty.send(b"\x18m"); // actual Ctrl+X, m
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"Modal 29");
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● Default", DEADLINE);
    // Model is already applied; Escape from its separate variant dialog leaves
    // the chosen model with no overlay, preserving the original prompt draft.
    pty.send(b"\x1b");
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\x10Switch model variant\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    wait_screen_row(&pty, "model: modal-29", DEADLINE);
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\r");
    pty.wait_visible("echo: draft-kept", DEADLINE);
    let first = fixture.wait_requests(1);
    assert_eq!(first[0]["model"], "modal-29");
    assert_eq!(first[0]["reasoning"]["effort"], "high");
    assert_eq!(last_user_text(&first[0]).as_deref(), Some("draft-kept"));
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/model\r"); // slash and raw keys converge on the same selector
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"Modal 29");
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    // Existing valid fast is retained; the pinned model flow closes directly.
    pty.send(b"\r");
    dismissed(&pty, "Select model");
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● fast", DEADLINE);
    pty.send(b"none\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "named none variant");
    pty.wait_visible_after(off, "echo: named none variant", DEADLINE);
    assert_eq!(fixture.wait_requests(2)[1]["reasoning"]["effort"], "low");
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    {
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        let saved = saved_selection(&db, &fixture, "v04-modal", "");
        assert_eq!(saved["id"], "modal-29");
        assert_eq!(
            saved["variant"], "none",
            "named none is persisted, not null"
        );
    }
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible("Fixture session title", DEADLINE);
    pty.send(b"/thinking\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● none", DEADLINE);
    // Focus-current means Enter re-applies none, not the first Default row.
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "none after restart");
    pty.wait_visible_after(off, "echo: none after restart", DEADLINE);
    assert_eq!(fixture.wait_requests(3)[2]["reasoning"]["effort"], "low");
    wait_idle(&pty);
    pty.send(b"/effort\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"Default\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "default variant");
    pty.wait_visible_after(off, "echo: default variant", DEADLINE);
    let requests = fixture.wait_requests(4);
    assert_eq!(requests[3]["model"], "modal-29");
    assert!(
        requests[3]["reasoning"]["effort"].is_null(),
        "explicit default must clear the previous variant: effort={} prompt={:?}",
        requests[3]["reasoning"]["effort"],
        last_user_text(&requests[3])
    );
    let off = submit(&mut pty, "slow stream");
    fixture.wait_requests(5);
    let started = Instant::now();
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", Duration::from_millis(900));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "overlay responsive while provider pending"
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    pty.send(b"\x03");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let selection = saved_selection(&db, &fixture, "v04-modal", "");
    assert_eq!(selection["id"], "modal-29");
    assert_eq!(selection["variant"], serde_json::Value::Null);
    assert!(
        db.read_history("v04-modal")
            .unwrap()
            .iter()
            .any(|(role, text)| role == "user" && text == "draft-kept")
    );
    drop(db);
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible("Fixture session title", DEADLINE);
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● Default", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "default after restart");
    pty.wait_visible_after(off, "echo: default after restart", DEADLINE);
    assert!(fixture.wait_requests(6)[5]["reasoning"]["effort"].is_null());
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
}

/// All new-session entry points call the real app operation; busy variants of
/// those same routes keep the current session, draft and provider turn intact.
#[test]
fn v04_new_session_aliases_and_disabled_actions_have_real_effects_only_when_idle() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-new", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x10variant");
    wait_screen_row(&pty, "No results found", DEADLINE);
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "no-variant command is absent from palette"
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "No variants available", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(&[0x7f; 9]); // edit the unavailable /variants draft via real Backspace
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"T39 alt\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    wait_screen_row(&pty, "model: alt-model", DEADLINE);
    let off = submit(&mut pty, "original session");
    pty.wait_visible_after(off, "echo: original session", DEADLINE);
    let routes: &[&[u8]] = &[b"/new\r", b"/clear\r", b"\x18n", b"\x10New session\r"];
    for (i, route) in routes.iter().enumerate() {
        wait_idle(&pty);
        pty.send(route);
        wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE); // pinned New session returns Home
        assert!(
            !render_screen(&pty.snapshot())
                .rows()
                .iter()
                .any(|row| row.contains("echo:")),
            "new session has no old transcript"
        );
        if i == 1 {
            choose_model(&mut pty, "T39 alt");
            // Home model draft is separate from any session and restores the
            // existing per-model fast preference without reopening variants.
            assert!(
                !render_screen(&pty.snapshot())
                    .rows()
                    .iter()
                    .any(|r| r.contains("Select variant"))
            );
        }
        let text = format!("new route {i}");
        let off = submit(&mut pty, &text);
        pty.wait_visible_after(off, &format!("echo: {text}"), DEADLINE);
    }
    wait_idle(&pty);
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Switch session", DEADLINE);
    pty.send(b"v04-new\r");
    wait_screen_row(&pty, "echo: original session", DEADLINE);
    let off = submit(&mut pty, "slow stream");
    fixture.wait_requests(6);
    // Slash, chord and palette routes all consult the same availability policy.
    let mut draft_len = 0;
    for command in [
        "/new",
        "/clear",
        "/model",
        "/variants",
        "/agents",
        "/continue",
        "/thinking",
        "/effort",
    ] {
        pty.send(&vec![0x7f; draft_len]);
        pty.send(command.as_bytes());
        wait_screen_row(&pty, command, DEADLINE);
        pty.send(b"\r");
        wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
        draft_len = command.len();
    }
    pty.send(&vec![0x7f; draft_len]);
    for route in [b"\x18n".as_slice(), b"\x18m", b"\x18a", b"\x18l"] {
        pty.send(route);
        wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    }
    pty.send(b"\x10New session");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Commands")),
        "disabled palette action stays open"
    );
    pty.send(b"\x1b");
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    wait_idle(&pty);
    let off = submit(&mut pty, "continued same session");
    pty.wait_visible_after(off, "echo: continued same session", DEADLINE);
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    let requests = fixture.wait_requests(7);
    assert_eq!(
        requests.len(),
        7,
        "navigation and unavailable actions never submit"
    );
    for (i, request) in requests.iter().enumerate() {
        if i != 1 {
            assert_eq!(request["model"], ALT_MODEL);
            assert_eq!(
                request["reasoning"]["effort"], "high",
                "original session restored"
            );
        } else {
            assert_eq!(
                request["model"], MODEL,
                "new Home uses its own draft/configured fallback"
            );
            assert!(request["reasoning"]["effort"].is_null());
        }
    }
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let saved = saved_selection(&db, &fixture, "v04-new", "");
    let draft_key = format!(
        "tui.selection.draft:{}",
        serde_json::json!([
            fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
            "fixture",
            ""
        ])
    );
    let draft: serde_json::Value =
        serde_json::from_str(&db.get_pref(&draft_key).unwrap().unwrap()).unwrap();
    assert_eq!(
        draft["id"], ALT_MODEL,
        "subsequent Home routes restore the persisted Location/agent model draft"
    );
    assert_eq!(saved["id"], ALT_MODEL);
    assert_eq!(
        saved["variant"], "fast",
        "disabled actions did not mutate stored selection"
    );
    let sessions = db.list_sessions().unwrap();
    assert_eq!(
        sessions.len(),
        5,
        "exactly four new sessions, none created while busy"
    );
    for i in 0..4 {
        let text = format!("new route {i}");
        let owners: Vec<_> = sessions
            .iter()
            .filter(|id| {
                db.read_history(id)
                    .unwrap()
                    .iter()
                    .any(|(role, t)| role == "user" && t == &text)
            })
            .collect();
        assert_eq!(owners.len(), 1);
        assert_ne!(owners[0], "v04-new");
        assert_eq!(
            db.read_history(owners[0]).unwrap().len(),
            2,
            "new session contains only its own turn"
        );
    }
    let original = db.read_history("v04-new").unwrap();
    assert_eq!(
        original
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        ["original session", "slow stream", "continued same session"]
    );
}

/// AUD29 continues to qualify native actions through their modal surfaces.
#[test]
fn v04_scoped_session_agent_and_model_preferences_survive_restart() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["default_agent"] = "plain".into();
    config["agent"]["plain"] =
        serde_json::json!({"mode":"primary","prompt":"Plain fixture agent."});
    config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["none"] =
        serde_json::json!({"reasoningEffort":"low"});
    std::fs::write(path, config.to_string()).unwrap();
    seed_session(
        &fixture.data_dir(),
        &fixture.root.path().join("project"),
        "scope-b",
        0,
    );
    let mut pty = PtySession::spawn(fixture.clone(), "scope-a", None);
    pty.wait_visible(READY, DEADLINE);
    choose_model(&mut pty, "T39 alt");
    choose_variant(&mut pty, "none");
    let off = submit(&mut pty, "A named none");
    pty.wait_visible_after(off, "echo: A named none", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/continue\rscope-b\r");
    dismissed(&pty, "Switch session");
    wait_screen_row(&pty, "T39 model fixture", DEADLINE);
    let off = submit(&mut pty, "B different model");
    pty.wait_visible_after(off, "echo: B different model", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/continue\rscope-a\r");
    wait_screen_row(&pty, "echo: A named none", DEADLINE);
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "● none", DEADLINE);
    pty.send(b"\r"); // current focus, not merely a displayed dot
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A restored");
    pty.wait_visible_after(off, "echo: A restored", DEADLINE);
    choose_model(&mut pty, "T39 model");
    choose_model(&mut pty, "T39 alt"); // preferred none restores; no variant dialog
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Select variant"))
    );
    let off = submit(&mut pty, "A per model preference");
    pty.wait_visible_after(off, "echo: A per model preference", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rt39agent\r");
    wait_screen_row(&pty, "agent: t39agent", DEADLINE);
    choose_model(&mut pty, "T39 model"); // agent-specific override
    let off = submit(&mut pty, "A second agent override");
    pty.wait_visible_after(off, "echo: A second agent override", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rplain\r");
    wait_screen_row(&pty, "agent: plain", DEADLINE);
    let off = submit(&mut pty, "A first agent restored");
    pty.wait_visible_after(off, "echo: A first agent restored", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());

    let mut pty = PtySession::spawn(fixture.clone(), "scope-a", None);
    pty.wait_visible("Fixture session title", DEADLINE);
    pty.send(b"/thinking\r");
    wait_screen_row(&pty, "● none", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A restart current");
    pty.wait_visible_after(off, "echo: A restart current", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/effort\r");
    choose_variant(&mut pty, "Default");
    choose_model(&mut pty, "T39 model");
    choose_model(&mut pty, "T39 alt"); // explicit Default erased the named preference
    wait_screen_row(&pty, "● Default", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A cleared preference");
    pty.wait_visible_after(off, "echo: A cleared preference", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rt39agent\r");
    wait_screen_row(&pty, "agent: t39agent", DEADLINE);
    let off = submit(&mut pty, "A second agent restart");
    pty.wait_visible_after(off, "echo: A second agent restart", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/continue\rscope-b\r");
    wait_screen_row(&pty, "echo: B different model", DEADLINE);
    let off = submit(&mut pty, "B restart unchanged");
    pty.wait_visible_after(off, "echo: B restart unchanged", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let requests = fixture.wait_requests(10);
    assert_eq!(requests.len(), 10);
    for (i, request) in requests.iter().enumerate() {
        let low = [0, 2, 3, 5, 6].contains(&i);
        assert_eq!(
            request["model"],
            if low || i == 7 { ALT_MODEL } else { MODEL },
            "request {i}"
        );
        assert_eq!(
            request["reasoning"]["effort"],
            if low {
                serde_json::json!("low")
            } else {
                serde_json::Value::Null
            },
            "request {i}"
        );
        assert_eq!(
            request["input"].to_string().contains(AGENT_PROMPT),
            [4, 8].contains(&i),
            "rightful agent {i}"
        );
    }
    // Inherited title selection must follow each new session, too.
    let titles: Vec<_> = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| title::is_title(r))
        .cloned()
        .collect();
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[0]["model"], ALT_MODEL);
    assert_eq!(titles[0]["reasoning"]["effort"], "low");
    assert_eq!(titles[1]["model"], MODEL);
    assert!(titles[1]["reasoning"]["effort"].is_null());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, "scope-a", "plain"),
        serde_json::json!({"id":ALT_MODEL,"variant":null})
    );
    assert_eq!(
        saved_selection(&db, &fixture, "scope-a", "t39agent")["id"],
        MODEL
    );
    let untouched_key = format!(
        "tui.selection.session:{}",
        serde_json::json!([
            fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
            "fixture",
            "scope-b"
        ])
    );
    assert!(
        db.get_pref(&untouched_key).unwrap().is_none(),
        "reading an untouched session does not create a preference"
    );
    assert_eq!(
        db.get_pref("tui.selection.variant:[\"fixture\",\"alt-model\"]")
            .unwrap()
            .as_deref(),
        Some("null")
    );
    assert!(
        db.get_pref(oc_core::queries::PREF_MODEL_SELECTION)
            .unwrap()
            .is_none(),
        "scoped TUI never overwrites legacy global preference"
    );
}

/// AUD29 continues to qualify native actions through their modal surfaces.
#[test]
fn aud29_pty_panels_change_runtime_state() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "s-aud29-b", 2);
    let mut pty = PtySession::spawn(fixture.clone(), "s-aud29", None);
    pty.wait_visible(READY, DEADLINE);

    // Model picker: the effective model changes for the next turn.
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "T39 alt", DEADLINE);
    pty.send(b"T39 alt"); // Search selects the same genuine catalog entry.
    pty.send(b"\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    pty.wait_visible("model: alt-model", DEADLINE);

    let off = submit(&mut pty, "hello model");
    pty.wait_visible_after(off, "echo: hello model", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);

    // Agent picker: prompt and pinned model come from the agent definition.
    pty.send(b"/agents\r");
    wait_screen_row(&pty, "Select agent", DEADLINE);
    wait_screen_row(&pty, "t39agent", DEADLINE);
    pty.send(b"\r");
    pty.wait_visible("agent: t39agent", DEADLINE);

    let off = submit(&mut pty, "hello agent");
    pty.wait_visible_after(off, "echo: hello agent", DEADLINE);

    // Skill catalog: real cards from the runtime (bodies stay behind).
    pty.send(b"/skills\r");
    wait_screen_row(&pty, "Skills", DEADLINE);
    wait_screen_row(&pty, "T39 skill", DEADLINE);
    pty.send(b"\x1b"); // Esc closes
    std::thread::sleep(Duration::from_millis(200));

    // Custom command: the template is expanded by the application.
    let off = submit(&mut pty, "/t39cmd hello");
    pty.wait_visible_after(off, "echo: custom command payload for hello", DEADLINE);
    wait_idle(&pty); // echo may precede the terminal completion/receipt

    // Manual DCP compress: the model calls the compress tool, the runtime
    // stores a block and reports real saved tokens.
    let off = pty.snapshot().len();
    pty.send(b"/dcp-compress early span\r");
    pty.wait_visible_after(off, "dcp: compressed, saved", DEADLINE);
    pty.send(b"\x1b"); // Esc closes the DCP panel
    std::thread::sleep(Duration::from_millis(200));

    // Session switch: the attached session (and its history) really changes.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Switch session", DEADLINE);
    pty.send(b"\x1b[B"); // Down: cursor moves off the first id
    pty.send(b"\r");
    pty.wait_visible(READY, DEADLINE);
    let off = submit(&mut pty, "hello switch");
    pty.wait_visible_after(off, "echo: hello switch", DEADLINE);
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean exit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");

    let requests = fixture.wait_requests(5);
    let model_of = |index: usize| requests[index]["model"].as_str().unwrap_or_default();
    assert_eq!(model_of(0), ALT_MODEL, "picker changed the request model");
    assert_eq!(
        requests[0]["reasoning"]["effort"], "high",
        "variant reached the request body"
    );
    assert_eq!(model_of(1), ALT_MODEL, "agent pinned model kept");
    assert_eq!(
        requests[1]["reasoning"]["effort"], "high",
        "full agent model#variant reached the normal request"
    );
    let agent_request = requests[1]["input"].to_string();
    assert!(
        agent_request.contains(AGENT_PROMPT),
        "agent prompt reached the provider request"
    );
    let command_prompt = last_user_text(&requests[2]).expect("command prompt");
    assert_eq!(
        command_prompt, "custom command payload for hello",
        "custom command template expanded"
    );
    assert!(
        requests.len() >= 5,
        "compress ran as a tool round: {} requests",
        requests.len()
    );

    let db = oc_adapters::storage::Db::open(pty.data_dir()).expect("db");
    let blocks = oc_adapters::dcp::load_blocks(&db, "s-aud29").expect("blocks");
    assert_eq!(blocks.len(), 1, "one stored compression block");
    let switched = oc_adapters::storage::Db::read_history(&db, "s-aud29-b").expect("switched");
    assert!(
        switched
            .iter()
            .any(|(role, text)| role == "user" && text == "hello switch"),
        "turn landed in the switched session: {switched:?}"
    );
    let original = oc_adapters::storage::Db::read_history(&db, "s-aud29").expect("original");
    assert!(
        !original.iter().any(|(_, text)| text == "hello switch"),
        "turn did not stay in the original session"
    );
    let cards = oc_adapters::storage::Db::list_tool_ops(&db, "s-aud29").expect("ops");
    assert!(
        cards
            .iter()
            .any(|row| row.name == "compress" && row.state == "completed"),
        "compress tool op recorded: {cards:?}"
    );
}

/// AUD30: paste, unicode, resize, Ctrl-C and a failing output handle.
#[test]
fn aud30_pty_paste_resize_error_recovery() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "s-aud30", None);
    pty.wait_visible(READY, DEADLINE);

    // Bracketed paste arrives as one event with unicode intact.
    let off = pty.snapshot().len();
    pty.send("\x1b[200~привет 🌍\x1b[201~".as_bytes());
    pty.send(b"\r");
    pty.wait_visible_after(off, "┃привет🌍", DEADLINE);
    pty.wait_visible_after(off, "echo: привет 🌍", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);

    // Resize while a stream is running: the frame follows the new size and
    // the turn still completes.
    let off = submit(&mut pty, "slow stream");
    std::thread::sleep(Duration::from_millis(300));
    pty.resize(100, 30);
    // A session switch during a stream is explicitly refused, never silent.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Switch session"))
    );
    pty.send(&[0x7f; 9]); // refused /sessions leaves the draft available for editing
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);

    pty.send(b"\x03"); // Ctrl-C quits from idle
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "ctrl-c quit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored after paste/resize");

    assert_eq!(
        persisted(pty.data_dir(), "s-aud30")
            .iter()
            .map(|(role, text)| (role.as_str(), text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("user", "привет 🌍"),
            ("assistant", "echo: привет 🌍"),
            ("user", "slow stream"),
            ("assistant", "answer:slow stream"),
        ],
        "no extra persisted user messages"
    );

    // A renderer that cannot write its output handle exits visibly and
    // restores the terminal instead of hanging or leaving raw mode.
    let broken = Fixture::new();
    let mut bad = PtySession::spawn_bad_stdout(broken.clone(), "s-aud30-bad");
    let (status, out) = bad.wait_exit(DEADLINE);
    assert!(!status.success(), "broken output handle exits nonzero");
    let visible = visible_text(&out);
    let text = String::from_utf8_lossy(&visible);
    assert!(
        text.contains("error: draw:") || text.contains("error:"),
        "visible error: {text:?}"
    );
    assert!(bad.restored(), "terminal restored after draw failure");
    let history = persisted(bad.data_dir(), "s-aud30-bad");
    assert!(history.is_empty(), "no user message persisted: {history:?}");
}

/// AUD31: long history and session switches keep the view bounded, and tool
/// cards are paged newest-first (never the oldest 200 only).
#[test]
fn aud31_pty_bounded_backing_state() {
    let fixture = Fixture::new();
    let data_dir = fixture.data_dir();
    let project = fixture.root.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    seed_session(&data_dir, &project, "s-long39", 3000);
    for i in 0..20 {
        seed_session(&data_dir, &project, &format!("s-other-{i:02}"), 60);
    }
    seed_tool_ops(&data_dir, "s-long39", 260);
    let metrics = fixture.root.path().join("metrics.json");

    let mut pty = PtySession::spawn(fixture.clone(), "s-long39", Some(&metrics));
    pty.wait_visible(READY, DEADLINE);
    // Page up through real rendering: older pages load and older rows evict.
    for _ in 0..40 {
        pty.send(b"\x1b[<64;1;1M"); // SGR wheel, distinct from editor history Up
        std::thread::sleep(Duration::from_millis(60));
    }
    pty.wait_visible("seeded row", DEADLINE);

    // Tool cards: the newest operation is visible, not the 200 oldest.
    pty.send(b"/cards\r");
    wait_screen_row(&pty, "cards | newest first, up pages older", DEADLINE);
    wait_screen_row(&pty, "tool-0259", DEADLINE);
    pty.send(b"\x1b"); // Esc
    std::thread::sleep(Duration::from_millis(200));

    // Switch sessions: the window is replaced, not accumulated.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Switch session", DEADLINE);
    pty.send(b"\x1b[B"); // Down: cursor moves off the first id
    pty.send(b"\r");
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean exit");

    let raw = std::fs::read_to_string(&metrics).expect("metrics probe written");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("metrics JSON");
    let rows = value["window_rows"].as_u64().expect("window_rows");
    let bytes = value["retained_bytes"].as_u64().expect("retained_bytes");
    let total = value["window_total"].as_u64().expect("window_total");
    assert!(
        rows <= oc_tui::history::WINDOW_ROWS as u64,
        "window rows bounded: {rows}"
    );
    assert!(
        bytes <= (oc_tui::history::WINDOW_BYTES + oc_tui::app::MAX_INPUT_BYTES) as u64,
        "retained bytes bounded: {bytes}"
    );
    assert!(
        total >= 60,
        "switched session reported its own total: {total}"
    );
    assert_eq!(
        value["session"].as_str(),
        Some("s-other-00"),
        "switch landed on the next session"
    );
}
