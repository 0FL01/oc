//! T42: actual-binary PTY qualification for the compatibility fixes —
//! bare `oc` launches the TUI, a real Location switch inside one running
//! application lifecycle, and the bounded `apply_patch` diff card.
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
const MODEL: &str = "t42-model";
/// Global instruction marker: must survive every Location switch.
const GLOBAL_RULE: &str = "T42_GLOBAL_RULE_7a1";
/// Project-local instruction markers: exactly one may be live at a time.
const A_RULE: &str = "T42_A_RULE_3b8";
const B_RULE: &str = "T42_B_RULE_9c4";
/// MCP tool exposed by project B only.
const B_MCP_TOOL: &str = "t42mcp__search";
const B_MCP_TEXT: &str = "t42:search";
const PATCH: &str = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n*** End Patch";

/// Scripted native Responses peer plus isolated HOME/config.
struct Fixture {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
    mcp: Mcp,
}

impl Fixture {
    fn new() -> Arc<Self> {
        let root = tempfile::TempDir::new().expect("tempdir");
        let home = root.path().join("home");
        let config = home.join("config/opencode");
        std::fs::create_dir_all(&config).expect("isolated config");
        let project_a = root.path().join("proj-alpha");
        let project_b = root.path().join("proj-beta");
        std::fs::create_dir_all(&project_a).expect("project a");
        std::fs::create_dir_all(&project_b).expect("project b");
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake endpoint");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("endpoint address");
        let mcp = Mcp::start();
        // Global layer: provider/model plus the global instruction marker.
        std::fs::write(
            config.join("opencode.json"),
            serde_json::json!({
                "model": format!("fixture/{MODEL}"),
                "provider": {"fixture": {
                    "npm": "@ai-sdk/openai",
                    "options": {"baseURL": format!("http://{addr}/proxy/v1"),
                                "apiKey": "{env:OC_FIXTURE_KEY}"},
                    "models": {
                        MODEL: {"name": "T42 model", "limit": {"context": 32768, "output": 4096}}
                    }
                }},
                "permissions": {"apply_patch": "allow"},
                "dcp": {"enabled": false}
            })
            .to_string(),
        )
        .expect("global config");
        std::fs::write(config.join("AGENTS.md"), format!("{GLOBAL_RULE}\n"))
            .expect("global instructions");
        // Project A: own command, agent and skill.
        std::fs::write(
            project_a.join("opencode.json"),
            serde_json::json!({
                "agent": {"aagent": {"description": "T42 A agent", "model": MODEL,
                                     "mode": "primary", "prompt": "T42 A agent prompt."}},
                "command": {"acmd": {"description": "A command",
                                     "template": "A command payload for $1"}}
            })
            .to_string(),
        )
        .expect("project a config");
        std::fs::write(project_a.join("AGENTS.md"), format!("{A_RULE}\n")).expect("a instructions");
        std::fs::create_dir_all(project_a.join("src")).expect("a src");
        std::fs::write(
            project_a.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n",
        )
        .expect("a lib");
        write_skill(&project_a, "askill", "T42 A skill");
        // Project B: own command, skill and MCP server.
        std::fs::write(
            project_b.join("opencode.json"),
            serde_json::json!({
                "command": {"bcmd": {"description": "B command",
                                     "template": "B command payload for $1"}},
                "mcp": {"t42mcp": {"type": "remote", "url": mcp.url,
                                   "enabled": true, "oauth": false,
                                   "headers": {"Authorization": "Bearer t42-not-a-secret"},
                                   "timeout": 3000}}
            })
            .to_string(),
        )
        .expect("project b config");
        std::fs::write(project_b.join("AGENTS.md"), format!("{B_RULE}\n")).expect("b instructions");
        write_skill(&project_b, "bskill", "T42 B skill");
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
            mcp,
        })
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("home/data/oc")
    }

    fn command_in(&self, project: &Path) -> Command {
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
            .current_dir(project);
        command
    }

    fn project_a(&self) -> PathBuf {
        self.root.path().join("proj-alpha")
    }

    fn project_b(&self) -> PathBuf {
        self.root.path().join("proj-beta")
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
    Patch(String),
}

fn script(body: &serde_json::Value) -> Script {
    if has_function_call_output(body) {
        return Script::Text("answer:patched".to_string());
    }
    let prompt = last_user_text(body).unwrap_or_default();
    if prompt.starts_with("patch ") {
        let arguments = serde_json::json!({"patchText": PATCH}).to_string();
        return Script::Patch(arguments);
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
        Script::Patch(arguments) => finish_call(socket, "apply_patch", arguments),
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
    fn spawn(fixture: Arc<Fixture>, project: &Path, args: &[&str], metrics: Option<&Path>) -> Self {
        let (master, slave) = openpty_pair(120, 24);
        let mut cmd = fixture.command_in(project);
        cmd.args(args)
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
                     requests={} last_prompt={:?}; screen={:?}",
                    tail.len(),
                    String::from_utf8_lossy(&norm_visible(&tail[..tail.len().min(600)])),
                    requests.len(),
                    last,
                    render_screen(&buf).rows()
                );
            }
            std::thread::sleep(POLL);
        }
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

fn norm_needle(text: &str) -> Vec<u8> {
    text.bytes().filter(|b| !b.is_ascii_whitespace()).collect()
}

/// Needle for the first rendered row of an upstream message block (short
/// word prefix that always fits the first wrapped row).
fn message_needle(prefix: &str, text: &str) -> String {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return prefix.to_string();
    };
    let first: String = first.chars().take(24).collect();
    let used = first.chars().count();
    let mut needle = prefix.to_string();
    needle.push_str(&first);
    if used < 24
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
    let off = pty.snapshot().len();
    pty.send(text.as_bytes());
    pty.send(b"\r");
    wait_screen_row(pty, &message_needle("┃  ", text), DEADLINE);
    off
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
/// Minimal streamable-HTTP MCP server exposing one tool (project B only).
struct Mcp {
    url: String,
    calls: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Mcp {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/mcp",
            listener.local_addr().expect("addr").port()
        );
        listener.set_nonblocking(true).expect("nonblocking");
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_out = calls.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(DEADLINE))
                            .expect("read timeout");
                        let Some(body) = read_http(&mut socket) else {
                            continue;
                        };
                        let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);
                        match body["method"].as_str().unwrap_or_default() {
                            "initialize" => write_json(
                                &mut socket,
                                id,
                                serde_json::json!({"protocolVersion": "2025-11-25",
                                       "capabilities": {"tools": {}},
                                       "serverInfo": {"name": "t42", "version": "1"}}),
                            ),
                            "notifications/initialized" => {
                                let _ = socket.write_all(
                                    b"HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                                );
                            }
                            "tools/list" => write_json(
                                &mut socket,
                                id,
                                serde_json::json!({"tools": [{
                                    "name": "search",
                                    "description": "t42 search",
                                    "inputSchema": {"type": "object",
                                        "properties": {"query": {"type": "string"}},
                                        "required": ["query"]}}]}),
                            ),
                            "tools/call" => {
                                calls_out.lock().expect("mcp calls").push(
                                    body.pointer("/params/arguments")
                                        .cloned()
                                        .unwrap_or(serde_json::Value::Null),
                                );
                                write_json(
                                    &mut socket,
                                    id,
                                    serde_json::json!({"content": [{"type": "text", "text": B_MCP_TEXT}],
                                           "isError": false}),
                                );
                            }
                            _ => write_json(
                                &mut socket,
                                id,
                                serde_json::json!({"error": {"code": -32601, "message": "unknown"}}),
                            ),
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => panic!("mcp accept: {error}"),
                }
            }
        });
        Self {
            url,
            calls,
            stop,
            handle: Some(handle),
        }
    }
}

impl Mcp {
    fn calls(&self) -> Vec<serde_json::Value> {
        self.calls.lock().expect("mcp calls").clone()
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let result = handle.join();
            if !std::thread::panicking() {
                result.expect("mcp thread");
            }
        }
    }
}

fn read_http(socket: &mut TcpStream) -> Option<serde_json::Value> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let header_end = loop {
        let n = socket.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if bytes.len() >= 65_536 {
            return None;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq("content-length")
                .then(|| value.trim().parse().ok())?
        })
        .unwrap_or(0);
    while bytes.len() < header_end + length {
        let n = socket.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&bytes[header_end..header_end + length]).ok()
}

fn write_json(socket: &mut TcpStream, id: serde_json::Value, result: serde_json::Value) {
    let body =
        serde_json::to_vec(&serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}))
            .expect("mcp json");
    let _ = socket.write_all(
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    let _ = socket.write_all(&body);
}

fn write_skill(project: &Path, id: &str, name: &str) {
    let dir = project.join(".opencode/skill").join(id);
    std::fs::create_dir_all(&dir).expect("skill dir");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {name} card\n---\nbody bytes\n"),
    )
    .expect("skill file");
}

fn request_text(body: &serde_json::Value) -> String {
    body["input"].to_string()
}

fn tool_names(body: &serde_json::Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Inspect the isolated SQLite journal only after the PTY releases its lock.
fn journal_counts(fixture: &Fixture) -> (i64, i64, i64, i64, i64) {
    let db = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).expect("journal");
    let count = |table: &str| -> i64 {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("journal count")
    };
    let bound: i64 = db
        .query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE ?1",
            [format!(
                "{}%",
                oc_adapters::runtime::SESSION_LOCATION_PREFIX
            )],
            |row| row.get(0),
        )
        .expect("Location bindings");
    (
        count("sessions"),
        count("events"),
        count("turns"),
        count("messages"),
        bound,
    )
}

fn deck_key(project: &Path) -> String {
    format!(
        "tui.selection.tab_deck:{}",
        serde_json::json!([project.canonicalize().unwrap().to_string_lossy()])
    )
}

fn deck_record(fixture: &Fixture, project: &Path) -> Option<String> {
    oc_adapters::storage::Db::open(&fixture.data_dir())
        .unwrap()
        .get_pref(&deck_key(project))
        .unwrap()
}

/// Read the preference while the TUI owns the data root, without attempting
/// to acquire the application's exclusive Db lock.
fn live_deck_record(fixture: &Fixture, project: &Path) -> String {
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.query_row(
        "SELECT value FROM prefs WHERE key = ?1",
        [deck_key(project)],
        |row| row.get(0),
    )
    .unwrap()
}

fn session_selection_key(project: &Path, session: &str) -> String {
    format!(
        "tui.selection.session:{}",
        serde_json::json!([
            project.canonicalize().unwrap().to_string_lossy(),
            "fixture",
            session
        ])
    )
}

fn metrics(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn quit(pty: &mut PtySession) {
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
}

fn wait_idle(pty: &PtySession) {
    let start = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("esc interrupt") || row.contains("submission pending"))
    {
        assert!(start.elapsed() < DEADLINE, "turn remained active");
        std::thread::sleep(POLL);
    }
    // The finished answer may precede the application event that replaces
    // the streaming view with the durable page.
    std::thread::sleep(Duration::from_millis(150));
}

fn stored_title(fixture: &Fixture, session: &str) -> Option<String> {
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.query_row(
        "SELECT title FROM sessions WHERE id = ?1",
        [session],
        |row| row.get(0),
    )
    .unwrap()
}

fn wait_stored_title(fixture: &Fixture, session: &str, expected: Option<&str>) {
    let start = Instant::now();
    loop {
        if stored_title(fixture, session).as_deref() == expected {
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "session {session} did not persist title {expected:?}"
        );
        std::thread::sleep(POLL);
    }
}

fn wait_prompt_cleared(pty: &PtySession, slash_text: &str) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if !rows.iter().any(|row| row.contains(slash_text)) {
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "direct slash input remained after owner ACK: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
}

fn wait_dialog_closed(pty: &PtySession, title: &str) {
    let start = Instant::now();
    loop {
        if !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains(title))
        {
            return;
        }
        assert!(start.elapsed() < DEADLINE, "dialog still open: {title}");
        std::thread::sleep(POLL);
    }
}

fn assert_rename_input(pty: &PtySession, text: &str) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows
            .iter()
            .position(|row| row.contains("Rename session"))
            .and_then(|header| rows.get(header + 2))
            .is_some_and(|row| row.contains(text))
        {
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "rename modal field must contain {text:?}; screen: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn direct_rename_trims_unicode_and_persists_without_second_enter_or_overwriting_generated_title() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-rename"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "give me a generated title");
    wait_stored_title(&fixture, "direct-rename", Some("Fixture session title"));
    wait_idle(&pty);
    let deck = live_deck_record(&fixture, &project);
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    let slash = "/rename  Новое 🦊  ";
    pty.send(slash.as_bytes());
    wait_screen_row(&pty, "/rename  Новое 🦊", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "direct-rename").as_deref(),
        Some("Fixture session title")
    );
    pty.send(b"\r"); // exactly one Enter: the owner ACK must clear the command
    wait_stored_title(&fixture, "direct-rename", Some("Новое 🦊"));
    wait_prompt_cleared(&pty, "/rename");
    let rows = render_screen(&pty.snapshot()).rows();
    assert!(rows.iter().any(|row| row.contains("Новое 🦊")), "{rows:?}");
    assert!(
        !rows.iter().any(|row| row.contains("Rename session")),
        "no modal: {rows:?}"
    );
    assert_eq!(live_deck_record(&fixture, &project), deck);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
    let after = journal_counts(&fixture);
    assert_eq!(
        (after.0, after.2, after.3, after.4),
        (counts.0, counts.2, counts.3, counts.4),
        "direct rename cannot create a turn or a root"
    );
    assert_eq!(after.1, counts.1 + 1, "one owner title event");
    quit(&mut pty);

    let mut restart = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-rename"],
        None,
    );
    wait_screen_row(&restart, "Новое 🦊", DEADLINE);
    assert!(
        !render_screen(&restart.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("Rename session"))
    );
    quit(&mut restart);
    assert_eq!(
        stored_title(&fixture, "direct-rename").as_deref(),
        Some("Новое 🦊")
    );
    assert_eq!(journal_counts(&fixture), after);
}

#[test]
fn direct_rename_invalid_titles_keep_slash_draft_and_existing_title() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-invalid"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "give me a generated title");
    wait_stored_title(&fixture, "direct-invalid", Some("Fixture session title"));
    wait_idle(&pty);
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    let too_long = format!("/rename {}🦊", "x".repeat(256));
    pty.send(format!("\x1b[200~{too_long}\x1b[201~").as_bytes());
    pty.send(b"\r");
    wait_screen_row(&pty, "session title must be 1", DEADLINE);
    wait_screen_row(&pty, "[Pasted ~1 lines]", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "direct-invalid").as_deref(),
        Some("Fixture session title")
    );
    assert_eq!(journal_counts(&fixture), counts);
    pty.send(b"\x1b");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));

    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-invalid"],
        None,
    );
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    let invisible = "/rename safe\u{200b}title";
    pty.send(invisible.as_bytes());
    wait_screen_row(&pty, "/rename safe", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "session title must be 1", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("/rename"),
        "invisible draft must remain: {:?}",
        render_screen(&pty.snapshot()).rows()
    );
    assert_eq!(
        stored_title(&fixture, "direct-invalid").as_deref(),
        Some("Fixture session title")
    );
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("Rename session"))
    );
    pty.send(b"\x1b");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
}

#[test]
fn direct_rename_owner_storage_refusal_keeps_slash_input_for_single_enter_retry() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-refuse"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_direct_rename BEFORE UPDATE OF title ON sessions BEGIN SELECT RAISE(ABORT, 'test direct refusal'); END;").unwrap();
    let deck = live_deck_record(&fixture, &project);
    let counts = journal_counts(&fixture);
    let slash = "/rename Retry 🦊";
    pty.send(slash.as_bytes());
    wait_screen_row(&pty, slash, DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "session rename unavailable", DEADLINE);
    wait_screen_row(&pty, slash, DEADLINE);
    assert_eq!(stored_title(&fixture, "direct-refuse"), None);
    assert_eq!(live_deck_record(&fixture, &project), deck);
    assert_eq!(journal_counts(&fixture), counts);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("test direct refusal")
    );
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("Rename session"))
    );

    conn.execute_batch("DROP TRIGGER refuse_direct_rename")
        .unwrap();
    pty.send(b"\r");
    wait_stored_title(&fixture, "direct-refuse", Some("Retry 🦊"));
    wait_prompt_cleared(&pty, slash);
    assert_eq!(live_deck_record(&fixture, &project), deck);
    assert!(fixture.requests.lock().unwrap().is_empty());
    quit(&mut pty);
    let after = journal_counts(&fixture);
    assert_eq!(after.1, counts.1 + 1);
    let mut restart = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "direct-refuse"],
        None,
    );
    wait_screen_row(&restart, "Retry 🦊", DEADLINE);
    quit(&mut restart);
    assert_eq!(journal_counts(&fixture), after);
}

#[test]
fn rename_shortcut_palette_unicode_trim_and_restart_use_owner_title() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "rename-root"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "give me a generated title");
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    wait_idle(&pty);
    assert_eq!(
        stored_title(&fixture, "rename-root").as_deref(),
        Some("Fixture session title")
    );
    let deck = live_deck_record(&fixture, &project);
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    pty.send(b"prompt draft kept");
    wait_screen_row(&pty, "prompt draft kept", DEADLINE);
    pty.send(b"\x12"); // real Ctrl+R preloads the owner-generated title
    wait_screen_row(&pty, "Rename session", DEADLINE);
    assert_rename_input(&pty, "Fixture session title");
    // Escape cancels only the modal, without persisting or eating the draft.
    pty.send(b"\x1b");
    wait_dialog_closed(&pty, "Rename session");
    wait_screen_row(&pty, "prompt draft kept", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "rename-root").as_deref(),
        Some("Fixture session title")
    );

    pty.send(b"\x10"); // Ctrl+P, searchable real command
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"Rename session");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    assert_rename_input(&pty, "Fixture session title");
    pty.send(b"\x1b");
    wait_dialog_closed(&pty, "Rename session");
    wait_screen_row(&pty, "prompt draft kept", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "rename-root").as_deref(),
        Some("Fixture session title")
    );
    pty.send(b"\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    assert_rename_input(&pty, "Fixture session title");
    pty.send(&vec![0x7f; "Fixture session title".len()]);
    pty.send("\x1b[200~  Новое 🦊  \x1b[201~".as_bytes());
    wait_screen_row(&pty, "Новое 🦊", DEADLINE);
    assert_rename_input(&pty, "Новое 🦊");
    pty.send(b"\r");
    wait_dialog_closed(&pty, "Rename session");
    wait_screen_row(&pty, "Новое 🦊", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "rename-root").as_deref(),
        Some("Новое 🦊")
    );
    assert_eq!(live_deck_record(&fixture, &project), deck);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("prompt draft kept")
    );
    pty.send(b"\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    assert_rename_input(&pty, "Новое 🦊");
    pty.send(b"\x1b");
    wait_dialog_closed(&pty, "Rename session");
    pty.send(&[0x7f; 100]);
    quit(&mut pty);
    let after = journal_counts(&fixture);
    assert_eq!(
        (after.0, after.2, after.3, after.4),
        (counts.0, counts.2, counts.3, counts.4),
        "rename creates no turn or root"
    );
    assert_eq!(after.1, counts.1 + 1, "one durable title update event");
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);

    let mut restart = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "rename-root"],
        None,
    );
    wait_screen_row(&restart, "Новое 🦊", DEADLINE);
    restart.send(b"\x12");
    wait_screen_row(&restart, "Rename session", DEADLINE);
    assert_rename_input(&restart, "Новое 🦊");
    restart.send(b"\x1b");
    wait_dialog_closed(&restart, "Rename session");
    quit(&mut restart);
    assert_eq!(
        stored_title(&fixture, "rename-root").as_deref(),
        Some("Новое 🦊")
    );
    assert_eq!(journal_counts(&fixture), after);
}

#[test]
fn rename_storage_refusal_keeps_modal_value_draft_and_old_tab_until_retry() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "rename-refuse"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"draft sentinel");
    wait_screen_row(&pty, "draft sentinel", DEADLINE);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    conn.execute_batch("CREATE TRIGGER refuse_rename BEFORE UPDATE OF title ON sessions BEGIN SELECT RAISE(ABORT, 'test refusal'); END;").unwrap();
    let deck = live_deck_record(&fixture, &project);
    let counts = journal_counts(&fixture);
    pty.send(b"\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(&vec![0x7f; READY.len()]);
    pty.send(b"Retry title");
    wait_screen_row(&pty, "Retry title", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "session rename unavailable", DEADLINE);
    assert_rename_input(&pty, "Retry title");
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("test refusal")
    );
    assert_eq!(stored_title(&fixture, "rename-refuse"), None);
    assert_eq!(live_deck_record(&fixture, &project), deck);
    assert_eq!(journal_counts(&fixture), counts);
    // The original prompt is still parked behind the modal. A second Enter
    // after the storage fault is removed succeeds without retyping.
    conn.execute_batch("DROP TRIGGER refuse_rename").unwrap();
    pty.send(b"\r");
    wait_dialog_closed(&pty, "Rename session");
    wait_screen_row(&pty, "draft sentinel", DEADLINE);
    assert_eq!(
        stored_title(&fixture, "rename-refuse").as_deref(),
        Some("Retry title")
    );
    pty.send(&[0x7f; 100]);
    quit(&mut pty);
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[test]
fn rename_refuses_home_child_foreign_and_busy_without_changing_titles() {
    let fixture = Fixture::new();
    let alpha = fixture.project_a();
    let beta = fixture.project_b();
    let mut home = PtySession::spawn(fixture.clone(), &alpha, &[], None);
    wait_screen_row(&home, "Ask anything", DEADLINE);
    home.send(b"\x12");
    wait_screen_row(&home, "no session yet", DEADLINE);
    assert!(
        !render_screen(&home.snapshot())
            .rows()
            .join("\n")
            .contains("Rename session")
    );
    assert_eq!(journal_counts(&fixture).0, 0);
    quit(&mut home);

    let mut root = PtySession::spawn(
        fixture.clone(),
        &alpha,
        &["tui", "--session", "rename-parent"],
        None,
    );
    root.wait_visible(READY, DEADLINE);
    submit(&mut root, "slow stream");
    fixture.wait_requests(1);
    root.send(b"unsent root draft");
    wait_screen_row(&root, "unsent root draft", DEADLINE);
    root.send(b"\x12");
    wait_screen_row(&root, "tab busy; action unavailable", DEADLINE);
    assert!(
        !render_screen(&root.snapshot())
            .rows()
            .join("\n")
            .contains("Rename session")
    );
    wait_screen_row(&root, "answer:slow stream", DEADLINE);
    wait_idle(&root);
    assert!(
        render_screen(&root.snapshot())
            .rows()
            .join("\n")
            .contains("unsent root draft")
    );
    assert_eq!(
        stored_title(&fixture, "rename-parent").as_deref(),
        Some("Fixture session title")
    );
    root.send(&[0x7f; 100]);
    quit(&mut root);

    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.create_child_session(
        "rename-parent",
        "rename-child",
        None,
        None,
        Some("Child original"),
    )
    .unwrap();
    db.set_pref(
        &format!(
            "{}rename-child",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ),
        &alpha.canonicalize().unwrap().to_string_lossy(),
    )
    .unwrap();
    drop(db);
    let mut child = PtySession::spawn(
        fixture.clone(),
        &alpha,
        &["tui", "--session", "rename-child"],
        None,
    );
    wait_screen_row(&child, "Child original", DEADLINE);
    child.send(b"\x12");
    wait_screen_row(&child, "child session is read-only", DEADLINE);
    assert!(
        !render_screen(&child.snapshot())
            .rows()
            .join("\n")
            .contains("Rename session")
    );
    quit(&mut child);
    assert_eq!(
        stored_title(&fixture, "rename-child").as_deref(),
        Some("Child original")
    );

    let mut foreign = PtySession::spawn(
        fixture.clone(),
        &beta,
        &["tui", "--session", "rename-parent"],
        None,
    );
    wait_screen_row(&foreign, "startup", DEADLINE);
    foreign.send(b"\x12");
    assert!(
        !render_screen(&foreign.snapshot())
            .rows()
            .join("\n")
            .contains("Rename session")
    );
    foreign.send(b"q");
    assert!(!foreign.wait_exit(DEADLINE).0.success());
    assert_eq!(
        stored_title(&fixture, "rename-parent").as_deref(),
        Some("Fixture session title")
    );
    assert_eq!(
        stored_title(&fixture, "rename-child").as_deref(),
        Some("Child original")
    );
}

#[test]
fn immediate_ctrl_c_after_first_home_submit_restores_committed_root_on_restart() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("quit-first-turn-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), &project, &[], None);
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    // One PTY write keeps Quit adjacent to the first Enter. The channel unit
    // test controls the exact receipt order; here the real owner must persist
    // the accepted route before restoring the terminal and shutting down.
    pty.send(b"quit first turn\r\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let saved: serde_json::Value =
        serde_json::from_str(&deck_record(&fixture, &project).expect("accepted root deck"))
            .unwrap();
    let root = saved["active"].as_str().expect("active new root");
    assert_eq!(saved["sessions"], serde_json::json!([root]));
    assert_eq!(journal_counts(&fixture).0, 1);
    let requests = fixture.requests.lock().unwrap().len();

    let mut reopened = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&reopened, "Ask anything", DEADLINE);
    assert!(
        !render_screen(&reopened.snapshot())
            .rows()
            .join("\n")
            .contains("quit first turn")
    );
    click(&mut reopened, 10, 1);
    wait_screen_row(&reopened, "quit first turn", DEADLINE);
    quit(&mut reopened);
    assert_eq!(metrics(&path)["session"], root);
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!([root]));
    assert_eq!(journal_counts(&fixture).0, 1);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

/// One-based xterm SGR coordinates; a complete press/release must reach the
/// binary across ordinary redraws before the release is interpreted.
fn click(pty: &mut PtySession, x: u16, y: u16) {
    pty.send(format!("\x1b[<0;{x};{y}M").as_bytes());
    std::thread::sleep(Duration::from_millis(120));
    pty.send(format!("\x1b[<0;{x};{y}m").as_bytes());
}

#[test]
fn corrupt_parked_selection_keeps_good_tabs_and_original_preference() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("filtered-deck-metrics.json");
    let mut seed = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "good-root"],
        None,
    );
    seed.wait_visible(READY, DEADLINE);
    submit(&mut seed, "good retained prompt");
    wait_screen_row(&seed, "echo: good retained prompt", DEADLINE);
    wait_idle(&seed);
    seed.send(b"/new\r");
    wait_screen_row(&seed, "New session", DEADLINE);
    submit(&mut seed, "parked prompt");
    wait_screen_row(&seed, "echo: parked prompt", DEADLINE);
    wait_idle(&seed);
    click(&mut seed, 10, 1);
    wait_screen_row(&seed, "good retained prompt", DEADLINE);
    quit(&mut seed);
    let counts = journal_counts(&fixture);
    assert_eq!(counts.0, 2);
    let ids = oc_adapters::storage::Db::open(&fixture.data_dir())
        .unwrap()
        .list_sessions()
        .unwrap();
    let parked = ids.iter().find(|id| *id != "good-root").unwrap();
    let saved = deck_record(&fixture, &project).unwrap();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.set_pref(&session_selection_key(&project, parked), "not-json")
        .unwrap();
    drop(db);

    let mut restored = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&restored, "saved tabs partially unavailable", DEADLINE);
    wait_screen_row(&restored, "Ask anything", DEADLINE);
    click(&mut restored, 10, 1);
    wait_screen_row(&restored, "good retained prompt", DEADLINE);
    restored.send(b"/new\r");
    wait_screen_row(&restored, "tab deck could not be saved", DEADLINE);
    click(&mut restored, 10, 1);
    wait_screen_row(&restored, "good retained prompt", DEADLINE);
    quit(&mut restored);
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!(["good-root"]));
    assert_eq!(metrics(&path)["session"], "good-root");
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(saved.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);

    let mut roaming = PtySession::spawn(fixture.clone(), &fixture.project_b(), &[], Some(&path));
    wait_screen_row(&roaming, "Ask anything", DEADLINE);
    roaming.send(format!("/location {}\r", project.display()).as_bytes());
    wait_screen_row(&roaming, "Location tabs unavailable", DEADLINE);
    wait_screen_row(&roaming, "good retained prompt", DEADLINE);
    roaming.send(b"/new\r");
    wait_screen_row(&roaming, "tab deck could not be saved", DEADLINE);
    quit(&mut roaming);
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!(["good-root"]));
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(saved.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);
}

#[test]
fn invalid_absent_explicit_root_is_rejected_without_creating_a_row() {
    for id in [" root", "root ", "bad\troot", &"x".repeat(129)] {
        let fixture = Fixture::new();
        let project = fixture.project_a();
        let mut pty = PtySession::spawn(fixture.clone(), &project, &["tui", "--session", id], None);
        let (status, output) = pty.wait_exit(DEADLINE);
        assert!(!status.success() && pty.restored());
        assert!(contains(&output, b"invalid --session id"));
        assert_eq!(journal_counts(&fixture).0, 0);
        assert!(deck_record(&fixture, &project).is_none());
    }
}

#[test]
fn explicit_child_is_standalone_read_only_and_preserves_saved_home() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("explicit-child-metrics.json");
    let mut seed = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "parent"],
        None,
    );
    seed.wait_visible(READY, DEADLINE);
    quit(&mut seed);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.create_child_session("parent", "child-view", None, None, Some("Child view"))
        .unwrap();
    db.set_pref(
        &format!(
            "{}child-view",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ),
        &project.to_string_lossy(),
    )
    .unwrap();
    db.append_message("child-view", "assistant", "child history sentinel")
        .unwrap();
    drop(db);
    let mut home = PtySession::spawn(fixture.clone(), &project, &[], None);
    home.wait_visible(READY, DEADLINE);
    click(&mut home, 10, 1);
    home.send(b"/new\r");
    wait_screen_row(&home, "New session", DEADLINE);
    quit(&mut home);
    let saved = deck_record(&fixture, &project).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&saved).unwrap()["active"].is_null());
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    let mut child = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "child-view"],
        Some(&path),
    );
    wait_screen_row(&child, "child history sentinel", DEADLINE);
    wait_screen_row(&child, "read-only history", DEADLINE);
    child.send(b"should not submit\r");
    wait_screen_row(&child, "read-only history", DEADLINE);
    child.send(b"\x03");
    let (status, output) = child.wait_exit(DEADLINE);
    assert!(status.success() && child.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(metrics(&path)["session"], "child-view");
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(saved.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn explicit_foreign_and_unbound_rows_are_refused_without_claiming_or_listing() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    for i in 0..1500 {
        db.create_session(&format!("closed-{i:04}")).unwrap();
    }
    for id in ["foreign-root", "unbound-root"] {
        db.create_session(id).unwrap();
    }
    db.set_pref(
        &format!(
            "{}foreign-root",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ),
        &fixture.project_b().to_string_lossy(),
    )
    .unwrap();
    drop(db);
    for id in ["foreign-root", "unbound-root"] {
        let mut pty = PtySession::spawn(fixture.clone(), &project, &["tui", "--session", id], None);
        wait_screen_row(&pty, "startup", DEADLINE);
        pty.send(b"q");
        let (status, _) = pty.wait_exit(DEADLINE);
        assert!(!status.success() && pty.restored());
        assert!(deck_record(&fixture, &project).is_none());
    }
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert!(
        db.get_pref(&format!(
            "{}unbound-root",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ))
        .unwrap()
        .is_none()
    );
    assert_eq!(journal_counts(&fixture).0, 1502);
    drop(db);
    // Only the requested ID is probed even with a large closed archive.
    let mut fresh = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "new-root"],
        None,
    );
    fresh.wait_visible(READY, DEADLINE);
    quit(&mut fresh);
    assert_eq!(journal_counts(&fixture).0, 1503);
    let saved: serde_json::Value =
        serde_json::from_str(&deck_record(&fixture, &project).unwrap()).unwrap();
    assert_eq!(saved["active"], "new-root");
    assert_eq!(saved["sessions"], serde_json::json!(["new-root"]));
}

#[test]
fn persisted_deck_restores_order_home_close_and_explicit_session_without_new_work() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("restart-deck-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "first-deck-root"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "first deck prompt");
    wait_screen_row(&pty, "echo: first deck prompt", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/new\r");
    wait_screen_row(&pty, "New session", DEADLINE);
    submit(&mut pty, "second deck prompt");
    wait_screen_row(&pty, "echo: second deck prompt", DEADLINE);
    wait_idle(&pty);
    fixture.wait_requests(2);
    // Clicking the first tab changes the persisted active route without a turn.
    click(&mut pty, 10, 1);
    wait_screen_row(&pty, "first deck prompt", DEADLINE);
    quit(&mut pty);
    let counts = journal_counts(&fixture);
    assert_eq!((counts.0, counts.2), (2, 2));
    let ids = oc_adapters::storage::Db::open(&fixture.data_dir())
        .unwrap()
        .list_sessions()
        .unwrap();
    let second = ids
        .iter()
        .find(|id| *id != "first-deck-root")
        .unwrap()
        .clone();
    let saved_raw = deck_record(&fixture, &project).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&saved_raw).unwrap();
    assert_eq!(
        saved["sessions"],
        serde_json::json!(["first-deck-root", second])
    );
    assert_eq!(saved["active"], "first-deck-root");
    let requests = fixture.requests.lock().unwrap().len();

    let mut restart = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&restart, "Ask anything", DEADLINE);
    assert!(
        !render_screen(&restart.snapshot())
            .rows()
            .join("\n")
            .contains("first deck prompt")
    );
    assert_eq!(live_deck_record(&fixture, &project), saved_raw);
    assert_eq!(
        journal_counts(&fixture),
        counts,
        "Home did not create a root"
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
    click(&mut restart, 45, 1);
    wait_screen_row(&restart, "second deck prompt", DEADLINE);
    assert!(
        !render_screen(&restart.snapshot())
            .rows()
            .join("\n")
            .contains("first deck prompt")
    );
    click(&mut restart, 10, 1);
    wait_screen_row(&restart, "first deck prompt", DEADLINE);
    assert!(
        !render_screen(&restart.snapshot())
            .rows()
            .join("\n")
            .contains("second deck prompt")
    );
    assert!(render_screen(&restart.snapshot()).rows()[0].contains("Fixture session title"));
    let bad = fixture.root.path().join("invalid-deck-location");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("opencode.json"), r#"{"model":"fixture/unknown"}"#).unwrap();
    restart.send(format!("/location {}\r", bad.display()).as_bytes());
    wait_screen_row(&restart, "Location configuration failed", DEADLINE);
    assert!(
        render_screen(&restart.snapshot())
            .rows()
            .join("\n")
            .contains("first deck prompt")
    );
    restart.send(&[0x7f; 512]);
    quit(&mut restart);
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(saved_raw.as_str())
    );
    let restored = metrics(&path);
    assert_eq!(
        restored["tab_ids"],
        serde_json::json!(["first-deck-root", second])
    );
    assert_eq!(restored["session"], "first-deck-root");
    assert_eq!(restored["active_tab"], 0);
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        db.read_history("first-deck-root").unwrap()[0].1,
        "first deck prompt"
    );
    assert_eq!(db.read_history(&second).unwrap()[0].1, "second deck prompt");
    drop(db);

    let mut home = PtySession::spawn(fixture.clone(), &project, &[], None);
    wait_screen_row(&home, "Ask anything", DEADLINE);
    click(&mut home, 10, 1);
    wait_screen_row(&home, "first deck prompt", DEADLINE);
    home.send(b"/new\r");
    wait_screen_row(&home, "New session", DEADLINE);
    quit(&mut home);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&deck_record(&fixture, &project).unwrap())
            .unwrap()["active"],
        serde_json::Value::Null
    );
    let mut home_restart = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&home_restart, "New session", DEADLINE);
    quit(&mut home_restart);
    assert_eq!(
        metrics(&path)["tab_ids"],
        serde_json::json!(["first-deck-root", second])
    );
    assert!(metrics(&path)["session"].is_null());
    assert_eq!(metrics(&path)["tab_count"], 2);

    // Explicit --session wins over saved Home, without creating an existing root.
    let mut explicit = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", &second],
        Some(&path),
    );
    wait_screen_row(&explicit, "second deck prompt", DEADLINE);
    quit(&mut explicit);
    assert_eq!(
        metrics(&path)["tab_ids"],
        serde_json::json!(["first-deck-root", second])
    );
    assert_eq!(metrics(&path)["session"], second);

    let mut close = PtySession::spawn(fixture.clone(), &project, &[], None);
    wait_screen_row(&close, "Ask anything", DEADLINE);
    click(&mut close, 45, 1);
    wait_screen_row(&close, "second deck prompt", DEADLINE);
    close.send(b"\x1b[<35;63;1M");
    wait_screen_row(&close, "✕", DEADLINE);
    click(&mut close, 63, 1);
    wait_screen_row(&close, "first deck prompt", DEADLINE);
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    let closed_raw: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key = ?1",
            [deck_key(&project)],
            |row| row.get(0),
        )
        .unwrap();
    let closed: serde_json::Value = serde_json::from_str(&closed_raw).unwrap();
    assert_eq!(closed["sessions"], serde_json::json!(["first-deck-root"]));
    assert_eq!(closed["active"], "first-deck-root");
    drop(conn);
    quit(&mut close);
    assert_eq!(
        journal_counts(&fixture),
        counts,
        "closed root remains durable"
    );
    let mut after_close = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&after_close, "Ask anything", DEADLINE);
    click(&mut after_close, 10, 1);
    wait_screen_row(&after_close, "first deck prompt", DEADLINE);
    quit(&mut after_close);
    assert_eq!(
        metrics(&path)["tab_ids"],
        serde_json::json!(["first-deck-root"])
    );
    assert_eq!(
        journal_counts(&fixture),
        counts,
        "closing only changes the deck"
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);

    // Explicitly naming a closed root appends it to the loaded deck without
    // re-creating the session or sending another turn.
    let mut reopen = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", &second],
        Some(&path),
    );
    wait_screen_row(&reopen, "second deck prompt", DEADLINE);
    quit(&mut reopen);
    assert_eq!(
        metrics(&path)["tab_ids"],
        serde_json::json!(["first-deck-root", second])
    );
    assert_eq!(metrics(&path)["session"], second);
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn full_sixteen_real_tabs_restart_keeps_selected_route_and_all_ids() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("full-deck-metrics.json");
    let mut seed = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "capacity-00"],
        None,
    );
    seed.wait_visible(READY, DEADLINE);
    quit(&mut seed);
    let ids: Vec<_> = (0..16).map(|i| format!("capacity-{i:02}")).collect();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    for id in ids.iter().skip(1) {
        db.create_session(id).unwrap();
        db.set_pref(
            &format!("{}{}", oc_adapters::runtime::SESSION_LOCATION_PREFIX, id),
            &project.canonicalize().unwrap().to_string_lossy(),
        )
        .unwrap();
    }
    let full =
        serde_json::json!({"version": 1, "sessions": ids, "active": "capacity-12"}).to_string();
    db.set_pref(&deck_key(&project), &full).unwrap();
    drop(db);
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();
    let mut restart = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&restart, "tab limit reached", DEADLINE);
    assert!(render_screen(&restart.snapshot()).rows()[0].contains("Untitled session"));
    quit(&mut restart);
    let result = metrics(&path);
    assert_eq!(result["session"], "capacity-12");
    assert_eq!(result["active_tab"], 12);
    assert_eq!(result["tab_count"], 16);
    assert_eq!(result["tab_ids"], serde_json::json!(ids));
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(full.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn full_saved_deck_with_unreadable_parked_tab_opens_home_without_rewriting_preference() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("projected-full-deck-metrics.json");
    let mut seed = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "capacity-00"],
        None,
    );
    seed.wait_visible(READY, DEADLINE);
    quit(&mut seed);
    let ids: Vec<_> = (0..16).map(|i| format!("capacity-{i:02}")).collect();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    for id in ids.iter().skip(1) {
        db.create_session(id).unwrap();
        db.set_pref(
            &format!("{}{}", oc_adapters::runtime::SESSION_LOCATION_PREFIX, id),
            &project.canonicalize().unwrap().to_string_lossy(),
        )
        .unwrap();
    }
    let full =
        serde_json::json!({"version": 1, "sessions": ids, "active": "capacity-12"}).to_string();
    db.set_pref(&deck_key(&project), &full).unwrap();
    db.set_pref(&session_selection_key(&project, "capacity-07"), "not-json")
        .unwrap();
    drop(db);
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    let mut restart = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&restart, "saved tabs partially unavailable", DEADLINE);
    wait_screen_row(&restart, "Ask anything", DEADLINE);
    assert!(
        !render_screen(&restart.snapshot())
            .rows()
            .join("\n")
            .contains("tab limit reached")
    );
    assert_eq!(live_deck_record(&fixture, &project), full);
    click(&mut restart, 10, 1);
    wait_screen_row(&restart, "tab deck could not be saved", DEADLINE);
    restart.send(b"/new\r");
    wait_screen_row(&restart, "Ask anything", DEADLINE);
    assert_eq!(live_deck_record(&fixture, &project), full);
    quit(&mut restart);
    let result = metrics(&path);
    assert!(result["session"].is_null());
    assert!(result["active_tab"].is_null());
    assert_eq!(result["tab_count"], 15);
    assert_eq!(
        result["tab_ids"],
        serde_json::json!(
            ids.into_iter()
                .filter(|id| id != "capacity-07")
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(full.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn concurrent_sqlite_deck_edit_cannot_be_overwritten_by_restored_tui() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("cas-deck-metrics.json");
    let mut seed = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "cas-root"],
        None,
    );
    seed.wait_visible(READY, DEADLINE);
    quit(&mut seed);
    let original = deck_record(&fixture, &project).expect("initial explicit route saved");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&original).unwrap()["active"],
        "cas-root"
    );
    let counts = journal_counts(&fixture);
    let requests = fixture.requests.lock().unwrap().len();

    let mut restored = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    restored.wait_visible(READY, DEADLINE);
    click(&mut restored, 10, 1);
    wait_screen_row(&restored, READY, DEADLINE);
    // A second SQLite client edits exactly the preference the running TUI
    // loaded. The owner must reject the TUI's now-stale expected revision.
    let replacement = serde_json::json!({
        "version": 1,
        "sessions": ["cas-root"],
        "active": null
    })
    .to_string();
    let conn = rusqlite::Connection::open(fixture.data_dir().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.execute(
            "UPDATE prefs SET value = ?1 WHERE key = ?2",
            rusqlite::params![replacement, deck_key(&project)]
        )
        .unwrap(),
        1
    );
    drop(conn);
    restored.send(b"/new\r");
    wait_screen_row(&restored, "tab deck could not be saved", DEADLINE);
    wait_screen_row(&restored, "New session", DEADLINE);
    // The stale owner token must also block closing the parked real tab.
    // The close hit-test is the first tab's hover glyph at column 31.
    restored.send(b"\x1b[<35;31;1M");
    wait_screen_row(&restored, "✕", DEADLINE);
    click(&mut restored, 31, 1);
    wait_screen_row(
        &restored,
        "tab close refused; saved tabs unavailable",
        DEADLINE,
    );
    quit(&mut restored);
    assert!(
        metrics(&path)["session"].is_null(),
        "live route was retained"
    );
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!(["cas-root"]));
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(replacement.as_str())
    );

    let mut fresh = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&fresh, "New session", DEADLINE);
    quit(&mut fresh);
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!(["cas-root"]));
    assert!(metrics(&path)["session"].is_null());
    assert_eq!(
        deck_record(&fixture, &project).as_deref(),
        Some(replacement.as_str())
    );
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn deck_location_isolation_and_corrupt_preference_are_safe_on_restart() {
    let fixture = Fixture::new();
    let alpha = fixture.project_a();
    let beta = fixture.project_b();
    let path = fixture.root.path().join("locations-deck-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &alpha,
        &["tui", "--session", "alpha-deck"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "alpha location prompt");
    wait_screen_row(&pty, "echo: alpha location prompt", DEADLINE);
    wait_idle(&pty);
    pty.send(format!("/location {}\r", beta.display()).as_bytes());
    wait_screen_row(&pty, "proj-beta", DEADLINE);
    assert_eq!(
        journal_counts(&fixture).0,
        1,
        "switch to Home makes no root"
    );
    submit(&mut pty, "beta location prompt");
    wait_screen_row(&pty, "echo: beta location prompt", DEADLINE);
    wait_idle(&pty);
    pty.send(format!("/location {}\r", alpha.display()).as_bytes());
    wait_screen_row(&pty, "alpha location prompt", DEADLINE);
    quit(&mut pty);
    let counts = journal_counts(&fixture);
    assert_eq!((counts.0, counts.2), (2, 2));
    let requests = fixture.requests.lock().unwrap().len();

    let mut beta_restart = PtySession::spawn(fixture.clone(), &beta, &[], Some(&path));
    wait_screen_row(&beta_restart, "Ask anything", DEADLINE);
    click(&mut beta_restart, 10, 1);
    wait_screen_row(&beta_restart, "beta location prompt", DEADLINE);
    assert!(
        !render_screen(&beta_restart.snapshot())
            .rows()
            .join("\n")
            .contains("alpha location prompt")
    );
    quit(&mut beta_restart);
    assert_eq!(metrics(&path)["tab_count"], 1);
    assert_ne!(deck_record(&fixture, &alpha), deck_record(&fixture, &beta));
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);

    // A saved Home on B remains selected even when the switch originates
    // from A's real tab; B's parked tab must remain available in order.
    let mut roaming = PtySession::spawn(fixture.clone(), &beta, &[], None);
    wait_screen_row(&roaming, "Ask anything", DEADLINE);
    click(&mut roaming, 10, 1);
    wait_screen_row(&roaming, "beta location prompt", DEADLINE);
    roaming.send(b"/new\r");
    wait_screen_row(&roaming, "New session", DEADLINE);
    roaming.send(format!("/location {}\r", alpha.display()).as_bytes());
    wait_screen_row(&roaming, "alpha location prompt", DEADLINE);
    roaming.send(format!("/location {}\r", beta.display()).as_bytes());
    wait_screen_row(&roaming, "New session", DEADLINE);
    quit(&mut roaming);
    let mut beta_home = PtySession::spawn(fixture.clone(), &beta, &[], Some(&path));
    wait_screen_row(&beta_home, "New session", DEADLINE);
    quit(&mut beta_home);
    assert!(metrics(&path)["session"].is_null());
    assert_eq!(metrics(&path)["tab_count"], 1);
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);

    let corrupt = r#"{"version":55,"sessions":["alpha-deck"],"active":"alpha-deck"}"#;
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.set_pref(&deck_key(&alpha), corrupt).unwrap();
    drop(db);
    let mut broken = PtySession::spawn(fixture.clone(), &alpha, &[], Some(&path));
    wait_screen_row(&broken, "saved tab deck unavailable", DEADLINE);
    assert!(
        !render_screen(&broken.snapshot())
            .rows()
            .join("\n")
            .contains("alpha location prompt")
    );
    quit(&mut broken);
    assert_eq!(metrics(&path)["tab_count"], 0);
    assert!(metrics(&path)["session"].is_null());
    assert_eq!(deck_record(&fixture, &alpha).as_deref(), Some(corrupt));
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests);
}

#[test]
fn retained_tab_mouse_add_restores_draft_and_home_submit_binds_one_new_root() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("deck-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "deck-original"],
        Some(&metrics),
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "first tab");
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"saved draft");
    wait_screen_row(&pty, "saved draft", DEADLINE);

    // A single 32-cell tab followed by the three-cell ` + ` control.
    click(&mut pty, 34, 1);
    wait_screen_row(&pty, "New session", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1, "mouse add is sessionless");
    pty.send(b"home draft");
    wait_screen_row(&pty, "home draft", DEADLINE);
    // Return to the parked original without losing its editor or history.
    click(&mut pty, 10, 1);
    wait_screen_row(&pty, "saved draft", DEADLINE);
    wait_screen_row(&pty, "first tab", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1);

    // /new reopens the parked synthetic Home and its independent draft.
    pty.send(&[0x7f; 64]);
    pty.send(b"/new\r");
    wait_screen_row(&pty, "home draft", DEADLINE);
    pty.send(b"\r");
    fixture.wait_requests(2);
    wait_screen_row(&pty, "echo: home draft", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("esc interrupt") || row.contains("submission pending"))
    {
        assert!(started.elapsed() < DEADLINE, "Home turn stayed busy");
        std::thread::sleep(POLL);
    }
    // The Sessions dialog must reuse the parked original rather than append
    // another view for the same durable ID.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "deck-original", DEADLINE);
    // The list starts at the first row; the bullet marks the active session,
    // not the keyboard cursor.
    pty.send(b"\r");
    wait_screen_row(&pty, "first tab", DEADLINE);
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("Switch session"))
    {
        assert!(started.elapsed() < DEADLINE, "Sessions dialog stayed open");
        std::thread::sleep(POLL);
    }
    pty.send(&[0x7f; 64]);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(journal_counts(&fixture).0, 2, "one accepted Home root");
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
    assert_eq!(metrics["session"], "deck-original");
    assert_eq!(metrics["tab_count"], 2, "no duplicate Sessions tab");
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(db.read_history("deck-original").unwrap()[0].1, "first tab");
    let new = db
        .list_sessions()
        .unwrap()
        .into_iter()
        .find(|id| id != "deck-original")
        .unwrap();
    assert_eq!(db.read_history(&new).unwrap()[0].1, "home draft");
}

#[test]
fn hovered_close_reopens_durable_session_without_creating_a_root() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("close-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "close-reopen"],
        Some(&metrics),
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "persisted before close");
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1);

    // The 32-cell first tab paints its hovered close at x=31 (one-based).
    // A real SGR move is required before a release in that cell means close.
    pty.send(b"\x1b[<35;31;1M");
    wait_screen_row(&pty, "✕", DEADLINE);
    click(&mut pty, 31, 1);
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1, "close is process-local");

    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "close-reopen", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "persisted before close", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, ALT_LEAVE));
    assert_eq!(journal_counts(&fixture).0, 1);
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
    assert_eq!(metrics["session"], "close-reopen");
    assert_eq!(metrics["tab_count"], 1);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        db.read_history("close-reopen").unwrap()[0].1,
        "persisted before close"
    );
}

#[test]
fn keyboard_and_palette_close_tab_keep_the_root_durable_and_reopenable() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let path = fixture.root.path().join("keyboard-close-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    submit(&mut pty, "keyboard close history");
    fixture.wait_requests(1);
    wait_screen_row(&pty, "echo: keyboard close history", DEADLINE);
    wait_idle(&pty);
    let initial: serde_json::Value =
        serde_json::from_str(&live_deck_record(&fixture, &project)).unwrap();
    let root = initial["active"]
        .as_str()
        .expect("accepted root")
        .to_string();
    assert_eq!(initial["sessions"], serde_json::json!([root]));
    let counts = journal_counts(&fixture);
    assert_eq!((counts.0, counts.2, counts.3), (1, 1, 2));

    // Ctrl+X W is the actual two-key PTY chord, not a direct owner intent.
    pty.send(b"\x18");
    pty.send(b"w");
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    let closed: serde_json::Value =
        serde_json::from_str(&live_deck_record(&fixture, &project)).unwrap();
    assert_eq!(closed["sessions"], serde_json::json!([]));
    assert!(closed["active"].is_null());
    assert_eq!(
        journal_counts(&fixture),
        counts,
        "close only changes the deck"
    );
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("keyboard close history")
    );

    pty.send(b"/sessions\r");
    wait_screen_row(&pty, &root, DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "keyboard close history", DEADLINE);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&live_deck_record(&fixture, &project)).unwrap()["active"],
        root
    );

    // The searchable command palette invokes the same close on the reopened
    // root. Enter must select the visible result rather than submit a turn.
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"Close tab");
    wait_screen_row(&pty, "Close tab", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Ask anything", DEADLINE);
    let closed_again: serde_json::Value =
        serde_json::from_str(&live_deck_record(&fixture, &project)).unwrap();
    assert_eq!(closed_again["sessions"], serde_json::json!([]));
    assert!(closed_again["active"].is_null());
    assert_eq!(journal_counts(&fixture), counts);
    quit(&mut pty);
    assert!(metrics(&path)["session"].is_null());
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!([]));
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(db.list_sessions().unwrap(), vec![root.clone()]);
    assert_eq!(
        db.read_history(&root).unwrap()[0].1,
        "keyboard close history"
    );
    drop(db);

    let mut restored = PtySession::spawn(fixture.clone(), &project, &[], Some(&path));
    wait_screen_row(&restored, "Ask anything", DEADLINE);
    restored.send(b"/sessions\r");
    wait_screen_row(&restored, &root, DEADLINE);
    restored.send(b"\r");
    wait_screen_row(&restored, "keyboard close history", DEADLINE);
    quit(&mut restored);
    assert_eq!(metrics(&path)["session"], root);
    assert_eq!(metrics(&path)["tab_ids"], serde_json::json!([root]));
    assert_eq!(journal_counts(&fixture), counts);
    assert_eq!(fixture.wait_requests(1).len(), 1, "reopen sends no turn");
}

#[test]
fn keyboard_close_refuses_busy_turn_without_losing_draft() {
    let fixture = Fixture::new();
    let project = fixture.project_a();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &project,
        &["tui", "--session", "busy-close-root"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "slow stream");
    fixture.wait_requests(1);
    pty.send(b"unsent draft");
    wait_screen_row(&pty, "unsent draft", DEADLINE);
    let before = live_deck_record(&fixture, &project);
    pty.send(b"\x18w");
    wait_screen_row(&pty, "tab busy; action unavailable", DEADLINE);
    wait_screen_row(&pty, "unsent draft", DEADLINE);
    assert_eq!(live_deck_record(&fixture, &project), before);
    wait_screen_row(&pty, "answer:slow stream", DEADLINE);
    wait_idle(&pty);
    assert_eq!(live_deck_record(&fixture, &project), before);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .join("\n")
            .contains("unsent draft")
    );
    pty.send(&[0x7f; 64]);
    quit(&mut pty);
    assert_eq!(journal_counts(&fixture).0, 1);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        db.read_history("busy-close-root").unwrap(),
        vec![
            ("user".into(), "slow stream".into()),
            ("assistant".into(), "answer:slow stream".into()),
        ]
    );
    assert_eq!(fixture.wait_requests(1).len(), 1);
}

#[test]
fn retained_deck_refuses_add_during_turn_and_keeps_tabs_on_failed_location() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "deck-busy"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "slow stream");
    fixture.wait_requests(1);
    click(&mut pty, 34, 1);
    pty.send(b"/new\r");
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1);
    assert!(
        !render_screen(&pty.snapshot()).rows()[0].contains("New session"),
        "a busy turn cannot leave its tab"
    );
    pty.wait_visible("answer:slow stream", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    // The refused command remains editable; clear it before the next action.
    pty.send(&[0x7f; 64]);
    pty.send(b"draft after turn");
    wait_screen_row(&pty, "draft after turn", DEADLINE);
    let bad = fixture.root.path().join("bad-deck-location");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(bad.join("opencode.json"), r#"{"model":"fixture/unknown"}"#).unwrap();
    pty.send(&[0x7f; 64]);
    pty.send(format!("/location {}\r", bad.display()).as_bytes());
    wait_screen_row(&pty, "Location configuration failed", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1);
    pty.send(&[0x7f; 512]);
    click(&mut pty, 34, 1);
    wait_screen_row(&pty, "New session", DEADLINE);
    assert_eq!(journal_counts(&fixture).0, 1);
    click(&mut pty, 10, 1);
    wait_screen_row(&pty, "slow stream", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(journal_counts(&fixture).0, 1);
}

#[test]
fn bare_home_abandon_and_new_do_not_create_a_root() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("home-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], Some(&metrics));
    pty.wait_visible("█▀▀█", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot()).rows()[0].contains(READY),
        "bare Home has no session tab"
    );
    pty.send(b"/models\r");
    wait_screen_row(&pty, "T42 model", DEADLINE);
    pty.send(b"\x1b");
    std::thread::sleep(Duration::from_millis(150));
    pty.send(b"/new\r");
    pty.wait_visible("█▀▀█", DEADLINE);
    // Blank submissions are refused, and Esc with an unsubmitted draft
    // abandons Home without accepting a first turn.
    pty.send(b" \r");
    pty.send(b"draft never submitted\x1b");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "Home exits cleanly");
    assert!(contains(&output, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).expect("Home metrics")).expect("json");
    assert!(metrics["session"].is_null(), "Home has no fabricated ID");
    assert_eq!(journal_counts(&fixture), (0, 0, 0, 0, 0));
    assert!(fixture.requests.lock().expect("requests").is_empty());
}

#[test]
fn home_location_refusal_and_a_b_a_b_remain_sessionless_until_first_submit() {
    let fixture = Fixture::new();
    let bad = fixture.root.path().join("bad-location");
    std::fs::create_dir(&bad).unwrap();
    std::fs::write(
        bad.join("opencode.json"),
        r#"{"model":"fixture/LEAKME-MODEL"}"#,
    )
    .unwrap();
    let metrics = fixture.root.path().join("location-home-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], Some(&metrics));
    pty.wait_visible("█▀▀█", DEADLINE);
    pty.send(format!("/location {}\r", bad.display()).as_bytes());
    wait_screen_row(&pty, "Location configuration failed", DEADLINE);
    let screen = render_screen(&pty.snapshot()).rows().join("\n");
    assert!(screen.contains("/location"), "refusal preserves the draft");
    assert!(!screen.contains("LEAKME-MODEL"), "no config text in status");
    // The failed switch did not replace the Location or its Home selection.
    assert!(
        screen.contains("proj-alpha"),
        "original Location remains visible"
    );
    assert!(
        screen.contains("T42 model"),
        "original Home choice remains visible"
    );
    pty.send(&vec![0x7f; 512]);
    for project in [
        fixture.project_b(),
        fixture.project_a(),
        fixture.project_b(),
    ] {
        pty.send(format!("/location {}\r", project.display()).as_bytes());
        let name = project.file_name().unwrap().to_str().unwrap();
        wait_screen_row(&pty, name, DEADLINE);
        // A successful Home switch still has no candidate root, even if B
        // configured additional skills/MCP resources.
        assert_eq!(journal_counts(&fixture), (0, 0, 0, 0, 0));
    }
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"first on B\r");
    let main = fixture.wait_requests(1);
    assert_eq!(last_user_text(&main[0]).as_deref(), Some("first on B"));
    let (roots, _, turns, messages, bindings) = journal_counts(&fixture);
    assert_eq!((roots, turns, bindings), (1, 1, 1));
    assert!((1..=2).contains(&messages), "user durable before reply");
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    assert!(contains(&output, ALT_LEAVE));
    assert!(pty.restored());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let id = db.list_sessions().unwrap().remove(0);
    assert_eq!(
        db.get_pref(&format!(
            "{}{}",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX,
            id
        ))
        .unwrap(),
        Some(
            fixture
                .project_b()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).unwrap()).unwrap();
    assert!(
        metrics["session"].as_str().is_some(),
        "first accepted turn binds the root"
    );
}

#[test]
fn bare_first_accepted_prompt_creates_exactly_one_bound_root_and_turn() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    pty.wait_visible("█▀▀█", DEADLINE);
    assert!(!render_screen(&pty.snapshot()).rows()[0].contains(READY));
    let off = submit(&mut pty, "slow stream");
    wait_screen_row(&pty, READY, DEADLINE);
    let accepted = fixture.wait_requests(1);
    assert_eq!(last_user_text(&accepted[0]).as_deref(), Some("slow stream"));
    // The scripted peer is still sending heartbeats: acceptance has committed
    // one user message atomically with the new root, turn and Location binding.
    let (roots, events, turns, messages, bindings) = journal_counts(&fixture);
    assert_eq!((roots, turns, messages, bindings), (1, 1, 1, 1));
    assert!(events >= 2, "root and turn journaled at acceptance");
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    // Synchronize on the accepted turn's durable page rather than the first
    // streaming delta before quitting.
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    let (roots, events, turns, messages, bindings) = journal_counts(&fixture);
    assert_eq!((roots, turns, messages, bindings), (1, 1, 2, 1));
    assert!(events >= 2, "root and turn journaled");
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).expect("db");
    let session = db.list_sessions().expect("root").remove(0);
    assert_eq!(
        db.read_history(&session).expect("history"),
        vec![
            ("user".into(), "slow stream".into()),
            ("assistant".into(), "answer:slow stream".into()),
        ]
    );
    assert_eq!(
        db.get_pref(&format!(
            "{}{session}",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ))
        .expect("binding"),
        Some(
            fixture
                .project_a()
                .canonicalize()
                .expect("Location")
                .to_string_lossy()
                .into_owned()
        )
    );
    let requests: Vec<_> = fixture
        .requests
        .lock()
        .expect("requests")
        .iter()
        .filter(|body| !title::is_title(body))
        .cloned()
        .collect();
    assert_eq!(requests.len(), 1, "exactly one Responses turn");
    assert_eq!(last_user_text(&requests[0]).as_deref(), Some("slow stream"));
}

#[test]
fn explicit_session_still_attaches_and_creates_before_first_prompt() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("attached-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "explicit-home-test"],
        Some(&metrics),
    );
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    assert_eq!(journal_counts(&fixture), (1, 1, 0, 0, 1));
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).expect("metrics")).expect("json");
    assert_eq!(metrics["session"], "explicit-home-test");
    assert!(fixture.requests.lock().expect("requests").is_empty());
}

#[test]
fn new_from_attached_returns_home_without_an_extra_root() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("new-home-metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "prior-root"],
        Some(&metrics),
    );
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/new\r");
    pty.wait_visible("█▀▀█", DEADLINE);
    let path = fixture.project_b().canonicalize().expect("target Location");
    pty.send(format!("/location {}\r", path.display()).as_bytes());
    wait_screen_row(&pty, "proj-beta", DEADLINE);
    assert_eq!(
        journal_counts(&fixture),
        (1, 1, 0, 0, 1),
        "Home switch creates no B root"
    );
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics).expect("metrics")).expect("json");
    assert!(metrics["session"].is_null());
    assert_eq!(journal_counts(&fixture), (1, 1, 0, 0, 1));
    assert!(fixture.requests.lock().expect("requests").is_empty());
}

/// AUD38: bare `oc` launches the local TUI on a terminal; without a
/// terminal it is an actionable error, never a hidden headless run.
#[test]
fn aud38_bare_oc_launches_the_local_tui() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], None);
    // Bare launch is Home; a session tab exists only after submitting.
    pty.wait_visible("█▀▀█", DEADLINE);
    pty.wait_visible("T42 model fixture", DEADLINE);
    let off = submit(&mut pty, "hello bare");
    pty.wait_visible_after(off, "echo: hello bare", DEADLINE);
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "bare `oc` TUI exits cleanly");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");

    let requests = fixture.wait_requests(1);
    let body = request_text(&requests[0]);
    assert!(body.contains(GLOBAL_RULE), "global AGENTS rule in request");
    assert!(body.contains(A_RULE), "Location AGENTS rule in request");

    // Redirected bare `oc`: clear error pointing at `oc run`.
    let output = fixture
        .command_in(&fixture.project_a())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn redirected oc")
        .wait_with_output()
        .expect("redirected oc");
    assert_eq!(output.status.code(), Some(2), "usage exit code");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("oc run") && stderr.contains("interactive terminal"),
        "actionable headless hint: {stderr}"
    );
}

/// AUD38: `apply_patch` cards carry a bounded diff, not the patch bytes.
#[test]
fn aud38_apply_patch_card_shows_bounded_diff() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "s-t42-patch"],
        None,
    );
    pty.wait_visible(READY, DEADLINE);
    let off = submit(&mut pty, "patch the file");
    pty.wait_visible_after(off, "answer:patched", DEADLINE);
    pty.send(b"/cards\r");
    wait_screen_row(&pty, "src/lib.rs +1 -1 (1h)", DEADLINE);
    let screen = render_screen(&pty.snapshot());
    assert!(
        screen
            .rows()
            .iter()
            .any(|row| row.contains("diff 1f +1 -1")),
        "bounded diff totals rendered: {:?}",
        screen.rows()
    );
    assert!(
        !screen
            .rows()
            .iter()
            .any(|row| row.contains("*** Begin Patch")),
        "the patch bytes are never copied into the card"
    );
    pty.send(b"\x1b");
    std::thread::sleep(Duration::from_millis(200));
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success());
    assert!(
        fixture.mcp.calls().is_empty(),
        "project A never reaches project B's MCP server"
    );

    // The patch really applied to project A.
    let applied = std::fs::read_to_string(fixture.project_a().join("src/lib.rs")).expect("lib");
    assert!(applied.contains("a + b"), "patch applied: {applied}");
}

/// AUD38: one running lifecycle switches Location A -> B -> A: the target
/// generation is built before publication, the old project state (config,
/// AGENTS, skills, commands, MCP) disappears, global state survives, the
/// session stays Location-bound and an active turn refuses the switch.
#[test]
fn aud38_location_switch_is_one_lifecycle() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("metrics.json");
    let mut pty = PtySession::spawn(
        fixture.clone(),
        &fixture.project_a(),
        &["tui", "--session", "s-t42-a"],
        Some(&metrics),
    );
    pty.wait_visible(READY, DEADLINE);

    // Usage hint for the new command.
    pty.send(b"/location\r");
    pty.wait_visible("usage: /location", DEADLINE);

    let off = submit(&mut pty, "alpha one");
    pty.wait_visible_after(off, "echo: alpha one", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    let requests = fixture.wait_requests(1);
    let first = request_text(&requests[0]);
    assert!(first.contains(A_RULE) && first.contains(GLOBAL_RULE));
    assert!(!first.contains(B_RULE));
    assert!(!tool_names(&requests[0]).contains(&B_MCP_TOOL.to_string()));

    // A switch while a turn streams is refused, not raced.
    let off = submit(&mut pty, "slow stream");
    // The prompt is visible before async acceptance. Synchronize on the real
    // provider request so this remains a streaming-switch test, not an edit
    // of the draft awaiting acceptance (which intentionally preserves edits).
    fixture.wait_requests(2);
    // Row assertions use the reconstructed screen grid: ratatui's cell diff
    // can skip cells whose content coincides with the previous frame, which
    // fragments raw byte needles (see `wait_screen_row`).
    wait_screen_row(&pty, &message_needle("┃  ", "slow stream"), DEADLINE);
    let beta = fixture.project_b().canonicalize().expect("b path");
    pty.send(format!("/location {}\r", beta.display()).as_bytes());
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    // The refused command keeps the typed input (nothing is lost): clear it
    // before retrying with the same target.
    pty.send(&[0x7f; 120]);
    std::thread::sleep(Duration::from_millis(200));

    // Switch to B: its rule, command, skill and MCP server are live.
    pty.send(format!("/location {}\r", beta.display()).as_bytes());
    // The success note renders as the upstream toast; its long path may wrap
    // mid-word (`ui/toast.tsx:75` word wrap), so sync on the note's first
    // line instead of a byte needle inside the path. The B_RULE assertions
    // below prove the target Location really switched.
    wait_screen_row(&pty, "location:", DEADLINE);
    let off = submit(&mut pty, "beta one");
    pty.wait_visible_after(off, "echo: beta one", DEADLINE);
    let requests = fixture.wait_requests(3);
    let beta_request = requests
        .iter()
        .rev()
        .find(|body| request_text(body).contains(B_RULE))
        .expect("beta request");
    let beta_text = request_text(beta_request);
    assert!(
        beta_text.contains(B_RULE) && beta_text.contains(GLOBAL_RULE),
        "beta instructions + global rule: {beta_text}"
    );
    assert!(!beta_text.contains(A_RULE), "alpha instructions gone");
    assert!(
        tool_names(beta_request).contains(&B_MCP_TOOL.to_string()),
        "beta MCP tool published: {:?}",
        tool_names(beta_request)
    );
    pty.send(b"/skills\r");
    wait_screen_row(&pty, "bskill", DEADLINE);
    pty.send(b"\x1b");
    std::thread::sleep(Duration::from_millis(200));

    // Back to A: its state returns, beta state is gone, same A session.
    let alpha = fixture.project_a().canonicalize().expect("a path");
    pty.send(format!("/location {}\r", alpha.display()).as_bytes());
    // Switching back reopens the recorded A session: its history returns to
    // the screen grid (the beta session's rows are replaced).
    wait_screen_row(&pty, &message_needle("┃  ", "alpha one"), DEADLINE);
    let off = submit(&mut pty, "alpha two");
    pty.wait_visible_after(off, "echo: alpha two", DEADLINE);
    let requests = fixture.wait_requests(4);
    let alpha_request = requests
        .iter()
        .rev()
        .find(|body| request_text(body).contains("alpha two"))
        .expect("alpha request");
    let alpha_text = request_text(alpha_request);
    assert!(alpha_text.contains(A_RULE) && alpha_text.contains(GLOBAL_RULE));
    assert!(!alpha_text.contains(B_RULE), "beta instructions gone");
    assert!(
        !tool_names(alpha_request).contains(&B_MCP_TOOL.to_string()),
        "beta MCP tool gone: {:?}",
        tool_names(alpha_request)
    );
    pty.send(b"/skills\r");
    wait_screen_row(&pty, "askill", DEADLINE);
    pty.send(b"\x1b");
    std::thread::sleep(Duration::from_millis(200));
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean exit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");

    // The view metrics prove the reopened A session is the original one.
    let metrics: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&metrics).expect("metrics")).expect("json");
    assert_eq!(
        metrics["session"], "s-t42-a",
        "switching back reopens the recorded Location session"
    );

    // Sessions stay Location-bound; the beta turn landed in a beta session.
    let db = oc_adapters::storage::Db::open(pty.data_dir()).expect("db");
    let sessions = db.list_sessions().expect("sessions");
    assert_eq!(sessions.len(), 2, "one session per Location: {sessions:?}");
    let location_of = |id: &str| {
        db.get_pref(&format!(
            "{}{id}",
            oc_adapters::runtime::SESSION_LOCATION_PREFIX
        ))
        .expect("pref")
    };
    assert_eq!(
        location_of("s-t42-a").as_deref(),
        Some(alpha.to_string_lossy().as_ref())
    );
    let beta_session = sessions
        .iter()
        .find(|id| *id != "s-t42-a")
        .expect("beta session")
        .clone();
    assert_eq!(
        location_of(&beta_session).as_deref(),
        Some(beta.to_string_lossy().as_ref())
    );
    let beta_rows = oc_adapters::storage::Db::read_history(&db, &beta_session).expect("beta rows");
    assert!(
        beta_rows
            .iter()
            .any(|(role, text)| role == "user" && text == "beta one"),
        "beta turn persisted in the beta session: {beta_rows:?}"
    );
    let alpha_rows = oc_adapters::storage::Db::read_history(&db, "s-t42-a").expect("alpha rows");
    assert!(
        alpha_rows
            .iter()
            .any(|(role, text)| role == "user" && text == "alpha two"),
        "returned turn persisted in the alpha session: {alpha_rows:?}"
    );
    assert!(
        !alpha_rows.iter().any(|(_, text)| text.contains("beta one")),
        "beta work never landed in the alpha session"
    );
}
