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

#[test]
fn bare_home_abandon_and_new_do_not_create_a_root() {
    let fixture = Fixture::new();
    let metrics = fixture.root.path().join("home-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), &fixture.project_a(), &[], Some(&metrics));
    pty.wait_visible("█▀▀█", DEADLINE);
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
    let off = submit(&mut pty, "slow stream");
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
