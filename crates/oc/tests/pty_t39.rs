//! T39 (F14): PTY qualification for panels, terminal recovery and bounded
//! backing state against the real `oc tui` binary.
//!
//! The binary runs under a real PTY with an isolated HOME and a scripted
//! native Responses peer. Assertions inspect real behaviour: rendered bytes,
//! provider request bodies, exit codes, slave termios state, durable rows and
//! the opt-in view-metrics probe — never render snapshots alone.

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
                "t39agent": {"description": "T39 fixture agent", "model": ALT_MODEL,
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

/// Submit one prompt and prove it rendered. Row assertions use the
/// reconstructed screen grid: ratatui's cell diff skips unchanged cells on
/// the wire, so raw byte needles are unreliable for new rows.
fn submit(pty: &mut PtySession, text: &str) -> usize {
    let off = pty.snapshot().len();
    pty.send(text.as_bytes());
    pty.send(b"\r");
    wait_screen_row(pty, &format!("you: {text}"), DEADLINE);
    off
}

fn persisted(data_dir: &Path, session: &str) -> Vec<(String, String)> {
    let db = oc_adapters::storage::Db::open(data_dir).expect("db");
    db.read_history(session).expect("history")
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
fn aud29_pty_panels_change_runtime_state() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "s-aud29-b", 2);
    let mut pty = PtySession::spawn(fixture.clone(), "s-aud29", None);
    pty.wait_visible(READY, DEADLINE);

    // Model picker: the effective model changes for the next turn.
    pty.send(b"/model\r");
    wait_screen_row(&pty, "model |", DEADLINE);
    wait_screen_row(&pty, ALT_MODEL, DEADLINE);
    pty.send(b"\x1b[A"); // Up: cursor moves to the alphabetically first id
    pty.send(b"\x1b[C"); // Right: cycle to the declared variant
    wait_screen_row(&pty, "variant: fast", DEADLINE);
    pty.send(b"\r");
    pty.wait_visible("model: alt-model", DEADLINE);

    let off = submit(&mut pty, "hello model");
    pty.wait_visible_after(off, "echo: hello model", DEADLINE);

    // Agent picker: prompt and pinned model come from the agent definition.
    pty.send(b"/agents\r");
    wait_screen_row(&pty, "agents |", DEADLINE);
    wait_screen_row(&pty, "t39agent", DEADLINE);
    pty.send(b"\r");
    pty.wait_visible("agent: t39agent", DEADLINE);

    let off = submit(&mut pty, "hello agent");
    pty.wait_visible_after(off, "echo: hello agent", DEADLINE);

    // Skill catalog: real cards from the runtime (bodies stay behind).
    pty.send(b"/skills\r");
    wait_screen_row(&pty, "skills |", DEADLINE);
    wait_screen_row(&pty, "t39skill", DEADLINE);
    pty.send(b"\x1b"); // Esc closes
    std::thread::sleep(Duration::from_millis(200));

    // Custom command: the template is expanded by the application.
    let off = submit(&mut pty, "/t39cmd hello");
    pty.wait_visible_after(off, "echo: custom command payload for hello", DEADLINE);

    // Manual DCP compress: the model calls the compress tool, the runtime
    // stores a block and reports real saved tokens.
    let off = pty.snapshot().len();
    pty.send(b"/dcp-compress early span\r");
    pty.wait_visible_after(off, "dcp: compressed, saved", DEADLINE);
    pty.send(b"\x1b"); // Esc closes the DCP panel
    std::thread::sleep(Duration::from_millis(200));

    // Session switch: the attached session (and its history) really changes.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "sessions |", DEADLINE);
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
    pty.wait_visible_after(off, "you: привет 🌍", DEADLINE);
    pty.wait_visible_after(off, "echo: привет 🌍", DEADLINE);

    // Resize while a stream is running: the frame follows the new size and
    // the turn still completes.
    let off = submit(&mut pty, "slow stream");
    std::thread::sleep(Duration::from_millis(300));
    pty.resize(100, 30);
    // A session switch during a stream is explicitly refused, never silent.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "sessions |", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "turn active; session switch refused", DEADLINE);
    pty.send(b"\x1b"); // Esc closes the panel
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
        pty.send(b"\x1b[A");
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
    wait_screen_row(&pty, "sessions |", DEADLINE);
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
