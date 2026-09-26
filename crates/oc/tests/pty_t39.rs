//! T39 (F14): PTY qualification for panels, terminal recovery and bounded
//! backing state against the real `oc tui` binary.
//!
//! The binary runs under a real PTY with an isolated HOME and a scripted
//! native Responses peer. Assertions inspect real behaviour: rendered bytes,
//! provider request bodies, exit codes, slave termios state, durable rows and
//! the opt-in view-metrics probe — never render snapshots alone.

use std::io::{Read, Write};
#[path = "support/terminal.rs"]
mod terminal;
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
const S07_PROMPT: &str = "s07 progressive markdown";
const S07_NOTE: &str = "S07_NOTE_READ_CONFIRMED\n";
const S07_REASONING: &str = "S07 public reasoning before the read.";
const S07_ANSWER: &str = "```rust\nfn s07_probe() {\n    let S07_STREAM_FRAGMENT_42 = 42;\n    let S07_STREAM_DONE_43 = S07_STREAM_FRAGMENT_42 + 1;\n}\n```";
const REASONING_CLICK_PROMPT: &str = "pty reasoning header click";
const REASONING_CLICK_BODY: &str = "PTY_REASONING_CLICK_BODY_7819";
const REASONING_CLICK_ANSWER: &str = "PTY_REASONING_CLICK_FINAL_3826";
const REASONING_STEPS_PROMPT: &str = "pty adjacent reasoning steps";
const REASONING_STEPS_FIRST: &str = "**Inspecting**\n\nPTY_STEPS_FIRST_BODY_6148";
const REASONING_STEPS_SECOND: &str = "**Verifying**\n\nPTY_STEPS_SECOND_BODY_7349";
const REASONING_STEPS_ANSWER: &str = "PTY_STEPS_FINAL_1625";
const REASONING_STEPS_OPAQUE: &str = "PTY_STEPS_ENCRYPTED_NEVER_RENDER_8972";

/// Scripted native Responses peer plus isolated HOME/config.
struct Fixture {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    hold_title: Arc<AtomicBool>,
    title_closed: Arc<AtomicBool>,
    s07_continue: Arc<AtomicBool>,
    vis28_continue: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
    held_titles: Arc<Mutex<Vec<std::thread::JoinHandle<()>>>>,
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
        let hold_title = Arc::new(AtomicBool::new(false));
        let hold = hold_title.clone();
        let title_closed = Arc::new(AtomicBool::new(false));
        let closed = title_closed.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let s07_continue = Arc::new(AtomicBool::new(false));
        let continue_stream = s07_continue.clone();
        let vis28_continue = Arc::new(AtomicBool::new(false));
        let release_scanner = vis28_continue.clone();
        let held_titles = Arc::new(Mutex::new(Vec::new()));
        let title_threads = held_titles.clone();
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
                        let Some(body) = read_request(&mut socket) else {
                            continue;
                        };
                        captured.lock().expect("requests").push(body.clone());
                        if hold.load(Ordering::Relaxed) && title::is_title(&body) {
                            // The title request can arrive before the main request.
                            // Hold only this connection, not the listener or main stream.
                            let closed = closed.clone();
                            title_threads
                                .lock()
                                .unwrap()
                                .push(std::thread::spawn(move || {
                                    let mut byte = [0];
                                    assert_eq!(
                                        socket.read(&mut byte).expect("title disconnect"),
                                        0
                                    );
                                    closed.store(true, Ordering::Relaxed);
                                }));
                            continue;
                        }
                        if title::respond(&mut socket, &body) {
                            continue;
                        }
                        let scripted = script(&body);
                        let _ = respond(
                            &mut socket,
                            &scripted,
                            &stopping,
                            &continue_stream,
                            &release_scanner,
                        );
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
            hold_title,
            title_closed,
            s07_continue,
            vis28_continue,
            stop,
            server: Some(server),
            held_titles,
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
        for thread in self.held_titles.lock().unwrap().drain(..) {
            if !std::thread::panicking() {
                thread.join().expect("held title connection");
            }
        }
    }
}

/// One scripted peer response, including bounded multi-round tool fixtures.
enum Script {
    Text(String),
    HighRateBurst,
    WheelHeldRows,
    Slow(String),
    Vis28Held,
    S07Read,
    S07Markdown,
    ReasoningClick,
    ReasoningSteps,
    Compress(String),
}

fn script(body: &serde_json::Value) -> Script {
    if has_function_call_output(body) {
        if last_user_text(body).as_deref() == Some(S07_PROMPT) {
            return Script::S07Markdown;
        }
        return Script::Text("answer:compressed".to_string());
    }
    let prompt = last_user_text(body).unwrap_or_default();
    if prompt == "vis31 provider burst" {
        return Script::HighRateBurst;
    }
    if prompt == "vis32 held rows" {
        return Script::WheelHeldRows;
    }
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
    if prompt == "vis28 held stream" {
        return Script::Vis28Held;
    }
    if prompt == S07_PROMPT {
        return Script::S07Read;
    }
    if prompt == REASONING_CLICK_PROMPT {
        return Script::ReasoningClick;
    }
    if prompt == REASONING_STEPS_PROMPT {
        return Script::ReasoningSteps;
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

fn read_request(socket: &mut TcpStream) -> Option<serde_json::Value> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    let header_end = loop {
        let n = socket.read(&mut chunk).expect("HTTP headers");
        if n == 0 {
            // A title task can be cancelled before it finishes sending HTTP.
            return None;
        }
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
    assert!(length < 262_144, "bounded request: {length} bytes");
    while bytes.len() < header_end + length {
        let n = socket.read(&mut chunk).expect("HTTP body");
        if n == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    Some(serde_json::from_slice(&bytes[header_end..header_end + length]).expect("request JSON"))
}

fn respond(
    socket: &mut TcpStream,
    script: &Script,
    stop: &AtomicBool,
    s07_continue: &AtomicBool,
    vis28_continue: &AtomicBool,
) -> std::io::Result<()> {
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
    if let Script::Vis28Held = script {
        // Heartbeats keep the real Responses stream open across a complete
        // scanner cycle. Only the test (or Esc disconnect) releases it.
        while !vis28_continue.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
            socket.write_all(b": heartbeat\n\n")?;
            socket.flush()?;
            std::thread::sleep(Duration::from_millis(50));
        }
        return if stop.load(Ordering::Relaxed) {
            Ok(())
        } else {
            finish_text(socket, "answer:vis28 completed")
        };
    }
    match script {
        Script::Text(answer) => finish_text(socket, answer),
        Script::WheelHeldRows => {
            let mut answer = String::from("```text\n");
            let prefix = serde_json::json!({"type":"response.output_text.delta", "delta":answer});
            write!(socket, "data: {prefix}\n\n")?;
            for index in 0..120 {
                if index == 60 {
                    while !s07_continue.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                }
                let text = format!("VIS32_ROW_{index:03}\n");
                answer.push_str(&text);
                let delta = serde_json::json!({"type":"response.output_text.delta", "delta":text});
                write!(socket, "data: {delta}\n\n")?;
                socket.flush()?;
                if index >= 60 {
                    std::thread::sleep(Duration::from_millis(4));
                }
            }
            answer.push_str("```");
            let suffix = serde_json::json!({"type":"response.output_text.delta", "delta":"```"});
            write!(socket, "data: {suffix}\n\n")?;
            finish_completed(socket, &answer)
        }
        Script::HighRateBurst => {
            let mut answer = String::new();
            // Multiple worker deltas per requested input period. Real SSE
            // updates must progress concurrently with keyboard paints.
            for index in 0..1024 {
                let text = if index == 1023 {
                    "VIS31_PROVIDER_BURST_DONE\n".to_string()
                } else {
                    format!("burst-{index:04}\n")
                };
                answer.push_str(&text);
                let delta = serde_json::json!({"type":"response.output_text.delta", "delta":text});
                write!(socket, "data: {delta}\n\n")?;
                socket.flush()?;
                if index % 8 == 7 {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
            finish_completed(socket, &answer)
        }
        Script::Compress(arguments) => finish_call(socket, "compress", arguments),
        Script::S07Read => {
            let reasoning = serde_json::json!({"type": "response.reasoning_summary_text.delta",
                "delta": S07_REASONING});
            write!(socket, "data: {reasoning}\n\n")?;
            socket.flush()?;
            finish_call(socket, "read", r#"{"path":"s07-note.txt"}"#)
        }
        Script::S07Markdown => {
            // Flush an open fence over several real SSE events. The test must
            // observe its unique partial code row before releasing completion.
            for chunk in [
                "```rust\n",
                "fn s07_probe() {\n",
                "    let S07_STREAM_FRAGMENT_42 = 42;\n",
            ] {
                let delta =
                    serde_json::json!({"type": "response.output_text.delta", "delta": chunk});
                write!(socket, "data: {delta}\n\n")?;
                socket.flush()?;
                std::thread::sleep(Duration::from_millis(60));
            }
            let began = Instant::now();
            while !s07_continue.load(Ordering::Relaxed) {
                if stop.load(Ordering::Relaxed) || began.elapsed() >= DEADLINE {
                    return Ok(());
                }
                std::thread::sleep(POLL);
            }
            for chunk in [
                "    let S07_STREAM_DONE_43 = S07_STREAM_FRAGMENT_42 + 1;\n",
                "}\n",
                "```",
            ] {
                let delta =
                    serde_json::json!({"type": "response.output_text.delta", "delta": chunk});
                write!(socket, "data: {delta}\n\n")?;
                socket.flush()?;
                std::thread::sleep(Duration::from_millis(60));
            }
            finish_completed(socket, S07_ANSWER)
        }
        Script::ReasoningClick => {
            let reasoning = serde_json::json!({
                "type": "response.reasoning_summary_text.delta",
                "delta": format!("**Click plan**\n\n{REASONING_CLICK_BODY}")
            });
            write!(socket, "data: {reasoning}\n\n")?;
            socket.flush()?;
            finish_text(socket, REASONING_CLICK_ANSWER)
        }
        Script::ReasoningSteps => {
            let mut output = Vec::new();
            for (id, summary) in [
                ("rs_pty_first", REASONING_STEPS_FIRST),
                ("rs_pty_second", REASONING_STEPS_SECOND),
            ] {
                let delta = serde_json::json!({
                    "type": "response.reasoning_summary_text.delta", "delta": summary
                });
                let item = serde_json::json!({
                    "type": "reasoning", "id": id, "status": "completed",
                    "encrypted_content": REASONING_STEPS_OPAQUE, "summary": []
                });
                let done = serde_json::json!({"type": "response.output_item.done", "item": item});
                write!(socket, "data: {delta}\n\ndata: {done}\n\n")?;
                socket.flush()?;
                output.push(item);
            }
            let answer = serde_json::json!({
                "type": "message", "id": "msg_pty_steps", "role": "assistant",
                "status": "completed", "content": [{"type": "output_text", "text": REASONING_STEPS_ANSWER}]
            });
            let delta = serde_json::json!({
                "type": "response.output_text.delta", "delta": REASONING_STEPS_ANSWER
            });
            let done = serde_json::json!({
                "type": "response.output_item.done", "output_index": 2, "item": answer
            });
            output.push(answer);
            let completed = serde_json::json!({
                "type": "response.completed", "response": {"status": "completed", "output": output}
            });
            write!(
                socket,
                "data: {delta}\n\ndata: {done}\n\ndata: {completed}\n\n"
            )?;
            socket.flush()
        }
        Script::Slow(_) => unreachable!("handled above"),
        Script::Vis28Held => unreachable!("handled above"),
    }
}

fn finish_text(socket: &mut TcpStream, answer: &str) -> std::io::Result<()> {
    let delta = serde_json::json!({"type": "response.output_text.delta", "delta": answer});
    write!(socket, "data: {delta}\n\n")?;
    finish_completed(socket, answer)
}

fn finish_completed(socket: &mut TcpStream, answer: &str) -> std::io::Result<()> {
    let completed = serde_json::json!({"type": "response.completed", "response": {
        "status": "completed", "output": [{"type": "message", "role": "assistant",
            "content": [{"type": "output_text", "text": answer}]}]
    }});
    write!(socket, "data: {completed}\n\n")?;
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

/// VIS31: actual terminal writes and process CPU in a stable idle window,
/// followed by paced unique glyphs (not a configured FPS inferred as output).
#[test]
fn vis31_demand_driven_idle_and_high_rate_input_paints() {
    let fixture = Fixture::new();
    let metrics_path = fixture.root.path().join("scheduling.json");
    let mut pty = PtySession::spawn(fixture, "scheduling", Some(&metrics_path));
    pty.wait_visible(READY, DEADLINE);
    std::thread::sleep(Duration::from_millis(200));
    let idle_start = Instant::now();
    let bytes_before = pty.snapshot().len();
    let cpu_before = process_cpu_ticks(pty.child.id());
    std::thread::sleep(Duration::from_secs(1));
    let cpu_ticks = process_cpu_ticks(pty.child.id()) - cpu_before;
    assert_eq!(
        pty.snapshot().len(),
        bytes_before,
        "stable idle wrote terminal bytes"
    );
    let idle_end = idle_start.elapsed();
    eprintln!("VIS31 idle window={idle_end:?} CPU ticks={cpu_ticks}");
    // The old-loop baseline used 3–4 CPU ticks and 627–660 terminal bytes
    // per second on this host. Stable idle must have no periodic paint work.
    assert!(cpu_ticks <= 1, "stable idle consumed {cpu_ticks} CPU ticks");
    let mut measured = Vec::new();
    for hz in [165_u32, 250] {
        let phase = Instant::now();
        let mut latencies = Vec::new();
        let offset = pty.snapshot().len();
        let mut writer = pty.master.try_clone().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        // Injection must not wait for rendering: slow paints cannot silently
        // lower the requested event rate and make the latency sample look good.
        let sender = std::thread::spawn(move || {
            for index in 0..32 {
                let due = phase + Duration::from_secs_f64(f64::from(index) / f64::from(hz));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                let glyph = char::from_u32(0x4e00 + index + hz).unwrap().to_string();
                let sent = Instant::now();
                writer.write_all(glyph.as_bytes()).unwrap();
                tx.send((sent, glyph)).unwrap();
            }
        });
        for (sent, glyph) in rx {
            while !contains(&pty.snapshot()[offset..], glyph.as_bytes()) {
                assert!(
                    sent.elapsed() < Duration::from_millis(100),
                    "glyph did not paint promptly"
                );
                std::thread::sleep(Duration::from_micros(100));
            }
            latencies.push(sent.elapsed().as_micros());
        }
        sender.join().unwrap();
        latencies.sort_unstable();
        measured.push((
            hz,
            phase.elapsed().as_micros(),
            latencies[16],
            latencies[30],
            latencies[31],
        ));
        pty.send(b"\x03");
        std::thread::sleep(Duration::from_millis(50));
    }
    pty.resize(120, 40);
    std::thread::sleep(Duration::from_millis(100));
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success());
    assert!(pty.restored());
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    let samples = metrics["frame_samples_ns"].as_array().unwrap();
    // This window starts after startup, before any injected input. A repeated
    // Terminal::draw with an unchanged buffer is still a forbidden idle attempt.
    assert!(
        !samples.iter().any(|sample| {
            let at = sample[0].as_u64().unwrap();
            (250_000_000..1_000_000_000).contains(&at)
        }),
        "idle render attempts: {metrics}"
    );
    eprintln!("VIS31 (hz, window_us, p50_us, p95_us, max_us)={measured:?}; metrics={metrics}");
}

fn process_cpu_ticks(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}

/// Nearest VIS31 fairness risk: independently paced UTF-8 keyboard input
/// while a real Responses peer sends a bounded burst of worker updates.
#[test]
fn vis31_high_rate_input_during_provider_burst() {
    for hz in [165_u32, 250] {
        let fixture = Fixture::new();
        let metrics_path = fixture.root.path().join("burst-scheduling.json");
        let mut pty = PtySession::spawn(fixture, "burst-scheduling", Some(&metrics_path));
        pty.wait_visible(READY, DEADLINE);
        let off = submit(&mut pty, "vis31 provider burst");
        pty.wait_visible_after(off, "burst-", DEADLINE);
        let phase = Instant::now();
        let offset = pty.snapshot().len();
        let mut writer = pty.master.try_clone().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let sender = std::thread::spawn(move || {
            for index in 0..32 {
                let due = phase + Duration::from_secs_f64(f64::from(index) / f64::from(hz));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                let glyph = char::from_u32(0x5000 + index + hz).unwrap().to_string();
                let sent = Instant::now();
                writer.write_all(glyph.as_bytes()).unwrap();
                tx.send((sent, glyph)).unwrap();
            }
        });
        let mut latencies = Vec::new();
        for (sent, glyph) in rx {
            while !contains(&pty.snapshot()[offset..], glyph.as_bytes()) {
                assert!(
                    sent.elapsed() < Duration::from_millis(100),
                    "provider burst starved input paint"
                );
                std::thread::sleep(Duration::from_micros(100));
            }
            latencies.push(sent.elapsed().as_micros());
        }
        sender.join().unwrap();
        pty.wait_visible_after(off, "VIS31_PROVIDER_BURST_DONE", DEADLINE);
        latencies.sort_unstable();
        eprintln!(
            "VIS31 burst hz={hz} window_us={} p50_us={} p95_us={} max_us={}",
            phase.elapsed().as_micros(),
            latencies[16],
            latencies[30],
            latencies[31]
        );
        pty.send(b"\x03");
        std::thread::sleep(Duration::from_millis(250));
        let bytes = pty.snapshot().len();
        let cpu = process_cpu_ticks(pty.child.id());
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(
            pty.snapshot().len(),
            bytes,
            "settled burst wrote idle terminal bytes"
        );
        assert!(process_cpu_ticks(pty.child.id()) - cpu <= 1);
        pty.send(b"\x03");
        assert!(pty.wait_exit(DEADLINE).0.success());
        assert!(pty.restored());
        let metrics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
        assert_eq!(
            metrics["worker_event_queue_lagged"], 0,
            "worker events lost: {metrics}"
        );
        eprintln!("VIS31 burst metrics={metrics}");
    }
}

/// The paired wheel campaign exposed a real TurnFinished refresh regression:
/// completion must preserve the detached painted anchor, not repin the view.
#[test]
fn vis32_detached_stream_anchor_survives_durable_completion_refresh() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "wheel-completion", None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis32 held rows");
    wait_screen_row(&pty, "VIS32_ROW_059", DEADLINE);
    for _ in 0..12 {
        pty.send(b"\x1b[<64;20;8M");
        std::thread::sleep(Duration::from_micros(6060));
    }
    std::thread::sleep(Duration::from_millis(250));
    let markers = |pty: &PtySession| {
        render_screen(&pty.snapshot())
            .rows()
            .into_iter()
            .filter_map(|row| {
                let index = row.find("VIS32_ROW_")?;
                Some(row[index..index + "VIS32_ROW_000".len()].to_string())
            })
            .collect::<Vec<_>>()
    };
    let before = markers(&pty);
    assert!(!before.is_empty());
    assert!(!before.iter().any(|row| row == "VIS32_ROW_059"));
    fixture.s07_continue.store(true, Ordering::Relaxed);
    wait_idle(&pty);
    let after = markers(&pty);
    assert_eq!(
        after.first(),
        before.first(),
        "durable completion moved detached viewport"
    );
    assert!(
        after
            .iter()
            .zip(&before)
            .all(|(after, before)| after == before)
    );
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

impl PtySession {
    fn spawn(fixture: Arc<Fixture>, session: &str, metrics: Option<&Path>) -> Self {
        Self::spawn_sized(fixture, session, metrics, 80, 24)
    }

    fn spawn_sized(
        fixture: Arc<Fixture>,
        session: &str,
        metrics: Option<&Path>,
        cols: u16,
        rows: u16,
    ) -> Self {
        let (master, slave) = openpty_pair(cols, rows);
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
        terminal::controlling_terminal(&mut cmd);
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

/// Wait until the rendered draft marker has disappeared (cell-diff aware).
fn wait_screen_absent(pty: &PtySession, needle: &str) {
    let start = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if !rows.iter().any(|row| row.contains(needle)) {
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "draft was not cleared: {rows:?}"
        );
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
    let mut idle_since = None;
    loop {
        let busy = render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("esc interrupt") || row.contains("submission pending"));
        if busy {
            idle_since = None;
        } else {
            // Cell diffs can expose echo text before the busy/footer update.
            // Observe idle across a complete redraw interval, not one stale
            // grid sample, before dispatching an idle-only navigation action.
            let since = idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= Duration::from_millis(100) {
                return;
            }
        }
        assert!(start.elapsed() < DEADLINE, "turn did not become idle");
        std::thread::sleep(POLL);
    }
}

/// Reconstruct just the scanner's painted foreground, including ratatui
/// cell-diff updates (unchanged cells keep their previous SGR color).
fn vis28_painted_color(output: &[u8], target: (usize, usize)) -> Option<(u8, u8, u8)> {
    let text = String::from_utf8_lossy(output);
    let mut chars = text.chars().peekable();
    let (mut row, mut col) = (0usize, 0usize);
    let mut saved = (0usize, 0usize);
    let mut fg = None;
    let mut painted = None;
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let final_ = loop {
                        match chars.next() {
                            Some(c) if ('@'..='~').contains(&c) => break c,
                            Some(c) => params.push(c),
                            None => return painted,
                        }
                    };
                    let nums: Vec<usize> = params
                        .trim_start_matches('?')
                        .split(';')
                        .filter_map(|part| part.parse().ok())
                        .collect();
                    let n = nums.first().copied().unwrap_or(1).max(1);
                    match final_ {
                        'H' | 'f' => {
                            row = n - 1;
                            col = nums.get(1).copied().unwrap_or(1).max(1) - 1;
                        }
                        'A' => row = row.saturating_sub(n),
                        'B' => row += n,
                        'C' => col += n,
                        'D' => col = col.saturating_sub(n),
                        's' => saved = (row, col),
                        'u' => (row, col) = saved,
                        'J' if params.starts_with('2') => painted = None,
                        'K' if row == target.0 && col <= target.1 => painted = None,
                        'm' => {
                            if nums.is_empty() || nums.contains(&0) || nums.contains(&39) {
                                fg = None;
                            }
                            for rgb in nums.windows(5) {
                                if rgb[0..2] == [38, 2] {
                                    fg = Some((rgb[2] as u8, rgb[3] as u8, rgb[4] as u8));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next() == Some('\\')) {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else {
            match ch {
                '\r' => col = 0,
                '\n' => row += 1,
                _ => {
                    if (row, col) == target {
                        painted = fg;
                    }
                    col += 1;
                }
            }
        }
    }
    painted
}

type Vis28Paint = (String, Option<(u8, u8, u8)>);

fn vis28_scanner(pty: &PtySession, animated: bool) -> Option<Vis28Paint> {
    let output = pty.snapshot();
    let rows = render_screen(&output).rows();
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("esc interrupt"))?;
    let hint = row[..row.find("esc interrupt")?].chars().count();
    let width = if animated { 8 } else { 3 };
    let x = hint.checked_sub(width + 1)?;
    let glyphs: String = row.chars().skip(x).take(width).collect();
    assert!(
        y > 0 && !rows[y - 1].trim().is_empty(),
        "scanner must sit below painted prompt metadata: {rows:?}"
    );
    if animated {
        assert_eq!(glyphs.chars().count(), 8, "eight painted cells: {row:?}");
        assert!(
            glyphs.chars().all(|c| c == '■' || c == '⬝'),
            "scanner glyphs: {row:?}"
        );
    }
    Some((glyphs, vis28_painted_color(&output, (y, x))))
}

fn vis28_wait_gone(pty: &PtySession) {
    let start = Instant::now();
    while render_screen(&pty.snapshot()).rows().iter().any(|row| {
        row.contains("esc interrupt")
            || row.contains("[⋯]")
            || row.contains('■')
            || row.contains('⬝')
    }) {
        assert!(start.elapsed() < DEADLINE, "running footer did not clear");
        std::thread::sleep(POLL);
    }
}

#[test]
fn vis28_real_pty_scanner_cycles_then_esc_cancels_and_static_fallback_completes() {
    let fixture = Fixture::new();
    let session = "vis28-animated";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis28 held stream");
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some("vis28 held stream")
    );
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    let initial_rows = render_screen(&pty.snapshot()).rows();
    let footer_y = initial_rows
        .iter()
        .position(|row| row.contains("esc interrupt"))
        .expect("painted running footer");
    let metadata = initial_rows[footer_y - 1].clone();
    assert!(
        !metadata.trim().is_empty(),
        "painted prompt metadata: {initial_rows:?}"
    );
    let mut samples = Vec::new();
    let began = Instant::now();
    // 54 * 40 ms nominal cycle; multiple cycles and a 6s cap tolerate PTY
    // reader/scheduler jitter without requiring exact frame timestamps.
    while began.elapsed() < Duration::from_secs(6) {
        if let Some(sample) = vis28_scanner(&pty, true) {
            samples.push(sample);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(
        render_screen(&pty.snapshot()).rows().get(footer_y - 1) == Some(&metadata),
        "agent/model metadata shifted during animation"
    );
    assert!(
        !fixture.vis28_continue.load(Ordering::Relaxed),
        "stream was released early"
    );
    let frames: Vec<_> = samples.iter().map(|(glyphs, _)| glyphs.as_str()).collect();
    // Require movement within each sweep, rather than exact intermediate
    // frame IDs: a 40ms PTY sample may legitimately skip a renderer tick.
    let mut phase = 0;
    let (mut forward, mut reverse) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    for frame in &frames {
        let prefix = frame.chars().take_while(|ch| *ch == '■').count();
        let suffix = frame.chars().rev().take_while(|ch| *ch == '■').count();
        match phase {
            0 if (1..=6).contains(&prefix) => {
                forward.insert(prefix);
            }
            0 if *frame == "⬝⬝⬝⬝⬝⬝⬝⬝" => {
                if forward.len() >= 3 && forward.iter().any(|n| *n >= 4) {
                    phase = 1; // end hold after observable forward motion
                } else {
                    forward.clear();
                }
            }
            1 if (2..=6).contains(&suffix) => {
                reverse.insert(suffix);
            }
            1 if *frame == "⬝⬝⬝⬝⬝⬝⬝⬝" && reverse.len() >= 3 && reverse.iter().any(|n| *n >= 4) =>
            {
                phase = 2; // start hold after observable reverse motion
                break;
            }
            _ => {}
        }
    }
    assert!(
        phase == 2,
        "missing observed forward/end-hold/reverse/start-hold cycle; distinct frames: {:?}",
        frames
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );
    let fading = samples.windows(2).any(|pair| {
        pair[0].0 == "⬝⬝⬝⬝⬝⬝⬝⬝"
            && pair[1].0 == pair[0].0
            && pair[0].1.is_some()
            && pair[1].1.is_some()
            && pair[0].1 != pair[1].1
    });
    assert!(
        fading,
        "stationary hold must visibly fade in painted SGR colors"
    );
    pty.send(b"\x1b");
    wait_screen_row(&pty, "esc again to interrupt", DEADLINE);
    assert!(
        !fixture.vis28_continue.load(Ordering::Relaxed),
        "first Esc cannot release the held request"
    );
    pty.send(b"\x1b");
    vis28_wait_gone(&pty);
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(pty.data_dir(), session),
        [("user".into(), "vis28 held stream".into())],
        "cancelled turn must remain durable without fabricated answer"
    );

    // A fresh process reads the project's animations:false setting.
    std::fs::write(
        fixture.root.path().join("project/opencode.json"),
        r#"{"animations":false}"#,
    )
    .expect("static config");
    fixture.vis28_continue.store(false, Ordering::Relaxed);
    let session = "vis28-static";
    let mut static_pty = PtySession::spawn(fixture.clone(), session, None);
    static_pty.wait_visible(READY, DEADLINE);
    submit(&mut static_pty, "vis28 held stream");
    fixture.wait_requests(2);
    wait_screen_row(&static_pty, "esc interrupt", DEADLINE);
    let began = Instant::now();
    while began.elapsed() < Duration::from_millis(350) {
        let (glyphs, _) = vis28_scanner(&static_pty, false).expect("running static footer");
        assert_eq!(glyphs, "[⋯]", "animation-disabled fixture");
        assert!(
            !render_screen(&static_pty.snapshot())
                .rows()
                .iter()
                .any(|row| row.contains('■') || row.contains('⬝'))
        );
        std::thread::sleep(Duration::from_millis(40));
    }
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&static_pty, "answer:vis28 completed", DEADLINE);
    vis28_wait_gone(&static_pty);
    static_pty.send(b"/quit\r");
    let (status, output) = static_pty.wait_exit(DEADLINE);
    assert!(status.success() && static_pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(static_pty.data_dir(), session),
        [
            ("user".into(), "vis28 held stream".into()),
            ("assistant".into(), "answer:vis28 completed".into())
        ]
    );
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

/// Native SGR left release with no preceding press.
fn mouse_up_only(pty: &mut PtySession, x: u16, y: u16) {
    pty.send(format!("\x1b[<0;{x};{y}m").as_bytes());
}

/// Decode the terminal clipboard destination from *complete* OSC 52 frames.
/// Never include a frame or its decoded contents in assertion diagnostics.
fn osc52_clipboard(output: &[u8]) -> Vec<Vec<u8>> {
    let mut copies = Vec::new();
    let prefix = b"\x1b]52;c;";
    let mut cursor = 0;
    while cursor + prefix.len() <= output.len() {
        if &output[cursor..cursor + prefix.len()] != prefix {
            cursor += 1;
            continue;
        }
        let start = cursor + prefix.len();
        let Some(end) = output[start..].iter().position(|byte| *byte == b'\x07') else {
            break; // an in-flight frame is not a successful clipboard write
        };
        let encoded = &output[start..start + end];
        assert!(
            !encoded.is_empty() && encoded.len().is_multiple_of(4),
            "invalid OSC 52 length"
        );
        let mut decoded = Vec::new();
        for (index, quartet) in encoded.chunks_exact(4).enumerate() {
            let value = |byte| match byte {
                b'A'..=b'Z' => Some(byte - b'A'),
                b'a'..=b'z' => Some(byte - b'a' + 26),
                b'0'..=b'9' => Some(byte - b'0' + 52),
                b'+' => Some(62),
                b'/' => Some(63),
                _ => None,
            };
            let a = value(quartet[0]).expect("invalid OSC 52 alphabet");
            let b = value(quartet[1]).expect("invalid OSC 52 alphabet");
            decoded.push(a << 2 | b >> 4);
            if quartet[2] != b'=' {
                let c = value(quartet[2]).expect("invalid OSC 52 alphabet");
                decoded.push(b << 4 | c >> 2);
                if quartet[3] != b'=' {
                    let d = value(quartet[3]).expect("invalid OSC 52 alphabet");
                    decoded.push(c << 6 | d);
                }
            } else {
                assert_eq!(quartet[3], b'=', "invalid OSC 52 padding");
            }
            if quartet.contains(&b'=') {
                assert_eq!(index + 1, encoded.len() / 4, "early OSC 52 padding");
            }
        }
        copies.push(decoded);
        cursor = start + end + 1;
    }
    copies
}

fn wait_clipboard(pty: &PtySession, count: usize, expected: &[u8]) {
    let start = Instant::now();
    loop {
        let copies = osc52_clipboard(&pty.snapshot());
        if copies.len() >= count {
            assert_eq!(copies.len(), count, "unexpected OSC 52 clipboard writes");
            assert!(
                copies[count - 1] == expected,
                "OSC 52 clipboard destination mismatch"
            );
            return;
        }
        assert!(
            start.elapsed() < DEADLINE,
            "missing complete OSC 52 clipboard write"
        );
        std::thread::sleep(POLL);
    }
}

/// The transcript source is an ordinary ASCII provider answer, at a painted
/// screen coordinate rather than a hard-coded layout position.
fn vis27_answer_cell(pty: &PtySession) -> (u16, u16) {
    let rows = render_screen(&pty.snapshot()).rows();
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains("echo: amber cobalt zircon"))
        .unwrap_or_else(|| panic!("VIS27 answer missing from painted transcript: {rows:?}"));
    let x = row.find("cobalt").expect("VIS27 painted word");
    (u16::try_from(x + 1).unwrap(), u16::try_from(y + 1).unwrap())
}

fn vis27_mouse(pty: &mut PtySession, button: u8, x: u16, y: u16, suffix: char) {
    pty.send(format!("\x1b[<{button};{x};{y}{suffix}").as_bytes());
}

/// Screen reconstructs glyphs without styles. Require the word's painted
/// bytes to carry the paired source selection colors (or reverse-video), not
/// merely appear in an OSC clipboard frame or a success toast.
fn vis27_highlighted_word(output: &[u8], word: &[u8]) -> bool {
    let (mut cursor, mut reverse) = (0, false);
    let (mut fg, mut bg) = (None, None);
    let mut selected = Vec::new();
    while cursor < output.len() {
        if output[cursor..].starts_with(b"\x1b[") {
            let start = cursor + 2;
            let Some(end) = output[start..]
                .iter()
                .position(|byte| (0x40..=0x7e).contains(byte))
                .map(|n| start + n)
            else {
                break;
            };
            if output[end] == b'm' {
                let codes: Vec<_> = output[start..end]
                    .split(|byte| *byte == b';')
                    .map(|code| {
                        if code.is_empty() {
                            Some(0)
                        } else {
                            std::str::from_utf8(code).ok()?.parse::<u16>().ok()
                        }
                    })
                    .collect();
                let mut i = 0;
                while i < codes.len() {
                    match codes[i] {
                        Some(0) => {
                            reverse = false;
                            fg = None;
                            bg = None;
                        }
                        Some(7) => reverse = true,
                        Some(27) => reverse = false,
                        Some(39) => fg = None,
                        Some(49) => bg = None,
                        Some(38 | 48) => {
                            let foreground = codes[i] == Some(38);
                            let color = if codes.get(i + 1) == Some(&Some(2)) {
                                let rgb = codes
                                    .get(i + 2..i + 5)
                                    .and_then(|parts| Some((parts[0]?, parts[1]?, parts[2]?)));
                                i += 4;
                                rgb
                            } else if codes.get(i + 1) == Some(&Some(5)) {
                                i += 2;
                                None
                            } else {
                                None
                            };
                            if foreground { fg = color } else { bg = color }
                        }
                        Some(30..=37 | 90..=97) => fg = None,
                        Some(40..=47 | 100..=107) => bg = None,
                        _ => {}
                    }
                    i += 1;
                }
                if !reverse && (fg != Some((10, 10, 10)) || bg != Some((238, 238, 238))) {
                    selected.clear();
                }
            } else {
                selected.clear(); // do not join words painted at unrelated cursor positions
            }
            cursor = end + 1;
        } else if output[cursor..].starts_with(b"\x1b]") {
            let Some(end) = output[cursor + 2..]
                .iter()
                .position(|byte| *byte == b'\x07')
            else {
                break;
            };
            cursor += 2 + end + 1;
            selected.clear();
        } else if output[cursor].is_ascii_graphic() || output[cursor] == b' ' {
            if !reverse && (fg != Some((10, 10, 10)) || bg != Some((238, 238, 238))) {
                selected.clear();
                cursor += 1;
                continue;
            }
            selected.push(output[cursor]);
            if selected.windows(word.len()).any(|part| part == word) {
                return true;
            }
            cursor += 1;
        } else {
            selected.clear();
            cursor += 1;
        }
    }
    false
}

fn vis27_wait_highlight(pty: &PtySession, word: &[u8]) {
    let start = Instant::now();
    while !vis27_highlighted_word(&pty.snapshot(), word) {
        assert!(
            start.elapsed() < DEADLINE,
            "selected word was not painted with selection styling"
        );
        std::thread::sleep(POLL);
    }
}

#[test]
fn vis27_real_pty_osc52_select_and_manual_clipboard_modes() {
    const PROMPT: &str = "amber cobalt zircon";
    // The source line selection trims leading transcript padding.
    const LINE: &[u8] = b"echo: amber cobalt zircon";
    let fixture = Fixture::new();
    let session = "vis27-select";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    assert!(
        contains(&pty.snapshot(), b"\x1b[?1006h"),
        "SGR mouse capture"
    );
    submit(&mut pty, PROMPT);
    fixture.wait_requests(1);
    wait_screen_row(&pty, "echo: amber cobalt zircon", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    wait_idle(&pty);
    let requests_before = fixture.requests.lock().unwrap().clone();
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| title::is_title(r))
            .count(),
        1
    );
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    let (x, y) = vis27_answer_cell(&pty);
    assert!(
        osc52_clipboard(&pty.snapshot()).is_empty(),
        "no clipboard write before selection"
    );

    // SGR reports one-based coordinates. First down/up is a caret, never a copy.
    vis27_mouse(&mut pty, 0, x, y, 'M');
    vis27_mouse(&mut pty, 0, x, y, 'm');
    std::thread::sleep(Duration::from_millis(75)); // dispatch before the second click, within 500ms
    assert!(
        osc52_clipboard(&pty.snapshot()).is_empty(),
        "first click copied"
    );
    assert!(
        !contains(&pty.snapshot(), b"Copied to clipboard"),
        "first click claimed success"
    );
    // The two subsequent no-motion releases exercise the multi-click clock.
    let before_word_copy = pty.snapshot().len();
    mouse_click(&mut pty, x, y);
    wait_clipboard(&pty, 1, b"cobalt");
    pty.wait_visible_after(before_word_copy, "Copied to clipboard", DEADLINE);
    assert!(
        vis27_highlighted_word(&pty.snapshot()[before_word_copy..], b"cobalt"),
        "word highlight missing after copy"
    );
    mouse_click(&mut pty, x, y);
    wait_clipboard(&pty, 2, LINE);
    assert_eq!(
        vis27_answer_cell(&pty),
        (x, y),
        "selection changed the painted transcript"
    );

    // A fresh drag starts elsewhere and selects just the interior of the line.
    let start_x = x - 6; // 'amber' begins six cells before 'cobalt'
    vis27_mouse(&mut pty, 0, start_x, y, 'M');
    vis27_mouse(&mut pty, 32, x + 6, y, 'M');
    vis27_mouse(&mut pty, 0, x + 6, y, 'm');
    wait_clipboard(&pty, 3, b"amber cobalt");
    assert!(
        *fixture.requests.lock().unwrap() == requests_before,
        "selection sent provider traffic"
    );
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(
        status.success() && pty.restored(),
        "select-mode terminal recovery"
    );
    assert!(contains(&output, ALT_LEAVE) && contains(&output, b"\x1b[?1006l"));
    assert_eq!(osc52_clipboard(&output).len(), 3);
    assert_eq!(
        persisted(pty.data_dir(), session),
        [
            ("user".into(), PROMPT.into()),
            ("assistant".into(), "echo: amber cobalt zircon".into())
        ],
        "selection changed the two-message history"
    );

    // Write an admitted project config before a *new* binary launch. The
    // fixture global config and data root stay isolated throughout.
    std::fs::write(
        fixture.root.path().join("project/opencode.json"),
        r#"{"terminal":{"copy":"manual"}}"#,
    )
    .expect("manual project config");
    let session = "vis27-manual";
    let mut manual = PtySession::spawn(fixture.clone(), session, None);
    manual.wait_visible(READY, DEADLINE);
    submit(&mut manual, PROMPT);
    // wait_requests counts main turns, not ancillary title requests.
    fixture.wait_requests(2);
    wait_screen_row(&manual, "echo: amber cobalt zircon", DEADLINE);
    let started = Instant::now();
    loop {
        if fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| title::is_title(request))
            .count()
            == 2
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "second root title request missing"
        );
        std::thread::sleep(POLL);
    }
    wait_screen_row(&manual, "Fixture session title", DEADLINE);
    wait_idle(&manual);
    let requests_before = fixture.requests.lock().unwrap().clone();
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| title::is_title(r))
            .count(),
        2
    );
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        2
    );
    let (x, y) = vis27_answer_cell(&manual);
    vis27_mouse(&mut manual, 0, x, y, 'M');
    vis27_mouse(&mut manual, 32, x + 6, y, 'M');
    vis27_mouse(&mut manual, 0, x + 6, y, 'm');
    // Synchronize with the completed drag via its painted highlight redraw;
    // no auto-copy and no success toast may precede a manual right press.
    vis27_wait_highlight(&manual, b"cobalt");
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        osc52_clipboard(&manual.snapshot()).is_empty(),
        "manual drag copied automatically"
    );
    assert!(
        !contains(&manual.snapshot(), b"Copied to clipboard"),
        "manual drag claimed success"
    );
    let before_manual_copy = manual.snapshot().len();
    vis27_mouse(&mut manual, 2, x, y, 'M');
    wait_clipboard(&manual, 1, b"cobalt");
    manual.wait_visible_after(before_manual_copy, "Copied to clipboard", DEADLINE);
    // Right-down leaves the selected cells untouched in the ratatui diff;
    // their prior selection-style paint remains in the PTY's screen buffer.
    assert!(
        vis27_highlighted_word(&manual.snapshot(), b"cobalt"),
        "manual highlight lost after copy"
    );
    assert_eq!(vis27_answer_cell(&manual), (x, y));
    assert!(
        *fixture.requests.lock().unwrap() == requests_before,
        "manual selection sent provider traffic"
    );
    manual.send(b"/quit\r");
    let (status, output) = manual.wait_exit(DEADLINE);
    assert!(
        status.success() && manual.restored(),
        "manual-mode terminal recovery"
    );
    assert!(contains(&output, ALT_LEAVE) && contains(&output, b"\x1b[?1006l"));
    assert_eq!(osc52_clipboard(&output).len(), 1);
    assert_eq!(
        persisted(manual.data_dir(), session),
        [
            ("user".into(), PROMPT.into()),
            ("assistant".into(), "echo: amber cobalt zircon".into())
        ],
        "manual selection changed the two-message history"
    );
}

/// Locate the completed reasoning control on the *painted* PTY grid.
fn reasoning_click_header(pty: &PtySession, prefix: &str) -> (u16, u16) {
    let rows = render_screen(&pty.snapshot()).rows();
    let (y, row) = rows
        .iter()
        .enumerate()
        .find(|(_, row)| row.contains(prefix))
        .unwrap_or_else(|| panic!("missing painted {prefix:?} header: {rows:?}"));
    let x = row.find(prefix).expect("header position");
    (u16::try_from(x + 1).unwrap(), u16::try_from(y + 1).unwrap())
}

#[test]
fn v06_real_pty_completed_reasoning_header_click_expands_and_collapses() {
    let fixture = Fixture::new();
    let session = "v06-reasoning-click";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, REASONING_CLICK_PROMPT);
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some(REASONING_CLICK_PROMPT)
    );
    wait_screen_row(&pty, REASONING_CLICK_ANSWER, DEADLINE);
    wait_screen_row(&pty, "+ Thought: Click plan", DEADLINE);
    wait_idle(&pty);
    let before = render_screen(&pty.snapshot()).rows();
    assert!(
        !before.iter().any(|row| row.contains(REASONING_CLICK_BODY)),
        "collapsed reasoning body must not be painted: {before:?}"
    );
    // Wait for the ancillary title before recording the complete provider
    // request baseline, so header clicks must be entirely local.
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    let requests_before = fixture.requests.lock().unwrap().len();

    let (x, y) = reasoning_click_header(&pty, "+ Thought: Click plan");
    mouse_click(&mut pty, x, y); // real SGR left press + release
    wait_screen_row(&pty, REASONING_CLICK_BODY, DEADLINE);
    wait_screen_row(&pty, "- Thought", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains(REASONING_CLICK_ANSWER)),
        "expanding reasoning retains the answer"
    );

    let (x, y) = reasoning_click_header(&pty, "- Thought");
    // A rapid second click on the same glyph is a word-selection gesture in
    // select-copy mode, not another single-click header activation. Let the
    // multi-click window close before exercising a second ordinary click.
    std::thread::sleep(Duration::from_millis(510));
    mouse_click(&mut pty, x, y);
    let started = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows.iter().any(|row| row.contains("+ Thought: Click plan"))
            && !rows.iter().any(|row| row.contains(REASONING_CLICK_BODY))
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "reasoning did not collapse: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
    // The same completed header also responds to a standalone native SGR UP.
    // Resolve each target from the current painted grid after the prior toggle:
    // expanded/collapsed layouts need not share a fixed terminal coordinate.
    let (x, y) = reasoning_click_header(&pty, "+ Thought: Click plan");
    mouse_up_only(&mut pty, x, y);
    wait_screen_row(&pty, REASONING_CLICK_BODY, DEADLINE);
    wait_screen_row(&pty, "- Thought", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains(REASONING_CLICK_ANSWER)),
        "UP-only expansion retains the answer"
    );
    let (x, y) = reasoning_click_header(&pty, "- Thought");
    mouse_up_only(&mut pty, x, y);
    let started = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows.iter().any(|row| row.contains("+ Thought: Click plan"))
            && !rows.iter().any(|row| row.contains(REASONING_CLICK_BODY))
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "UP-only reasoning did not collapse: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    assert_eq!(
        persisted(pty.data_dir(), session),
        vec![
            ("user".to_string(), REASONING_CLICK_PROMPT.to_string()),
            ("assistant".to_string(), REASONING_CLICK_ANSWER.to_string())
        ],
        "reasoning toggles leave the original durable answer unchanged"
    );
}

/// VIS15: the *real binary* must group adjacent durable reasoning items, not
/// merely render one item twice or reconstruct a group from a screenshot.
fn assert_reasoning_steps_collapsed(pty: &PtySession) {
    wait_screen_row(pty, "+ Thought: Verifying · 2 steps", DEADLINE);
    wait_screen_absent(pty, "PTY_STEPS_FIRST_BODY_6148");
    wait_screen_absent(pty, "PTY_STEPS_SECOND_BODY_7349");
    let rows = render_screen(&pty.snapshot()).rows();
    assert_eq!(
        rows.iter().filter(|row| row.contains("+ Thought")).count(),
        1,
        "exactly one collapsed reasoning header: {rows:?}"
    );
    assert!(rows.iter().any(|row| row.contains(REASONING_STEPS_ANSWER)));
    assert!(!contains(
        &pty.snapshot(),
        REASONING_STEPS_OPAQUE.as_bytes()
    ));
}

fn assert_reasoning_steps_expanded(pty: &PtySession) {
    wait_screen_row(pty, "PTY_STEPS_SECOND_BODY_7349", DEADLINE);
    let rows = render_screen(&pty.snapshot()).rows();
    let first = rows
        .iter()
        .position(|row| row.contains("PTY_STEPS_FIRST_BODY_6148"))
        .unwrap_or_else(|| panic!("first public body missing: {rows:?}"));
    let second = rows
        .iter()
        .position(|row| row.contains("PTY_STEPS_SECOND_BODY_7349"))
        .unwrap();
    assert!(first < second, "public reasoning item order: {rows:?}");
    assert!(rows.iter().any(|row| row.contains("- Thought · 2 steps")));
    assert!(rows.iter().any(|row| row.contains(REASONING_STEPS_ANSWER)));
    assert!(!contains(
        &pty.snapshot(),
        REASONING_STEPS_OPAQUE.as_bytes()
    ));
}

fn toggle_reasoning_steps(pty: &mut PtySession, expanded: bool) {
    let header = if expanded {
        "+ Thought: Verifying · 2 steps"
    } else {
        "- Thought · 2 steps"
    };
    let (x, y) = reasoning_click_header(pty, header);
    mouse_click(pty, x, y);
    if expanded {
        assert_reasoning_steps_expanded(pty);
    } else {
        assert_reasoning_steps_collapsed(pty);
    }
}

#[test]
fn vis15_real_pty_adjacent_reasoning_parts_group_and_replay_after_restart() {
    let fixture = Fixture::new();
    let session = "vis15-adjacent-reasoning";
    let mut pty = PtySession::spawn_sized(fixture.clone(), session, None, 120, 40);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, REASONING_STEPS_PROMPT);
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some(REASONING_STEPS_PROMPT)
    );
    wait_screen_row(&pty, REASONING_STEPS_ANSWER, DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    wait_idle(&pty);
    assert_reasoning_steps_collapsed(&pty);
    let requests_before = fixture.requests.lock().unwrap().len();
    toggle_reasoning_steps(&mut pty, true);
    // Select-copy mode treats a quick second click as a multi-click selection.
    std::thread::sleep(Duration::from_millis(510));
    toggle_reasoning_steps(&mut pty, false);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);

    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert!(!contains(&output, REASONING_STEPS_OPAQUE.as_bytes()));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);

    // Assert the committed journal, not just the answer projection or screen.
    let db = oc_adapters::storage::Db::open(pty.data_dir()).expect("durable db");
    assert_eq!(
        db.read_history(session).unwrap(),
        [
            ("user".into(), REASONING_STEPS_PROMPT.into()),
            ("assistant".into(), REASONING_STEPS_ANSWER.into())
        ]
    );
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let turns: i64 = conn
        .query_row(
            "SELECT count(*) FROM turns WHERE session_id=?1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(turns, 1, "one accepted turn across both launches");
    let (turn_status, journal): (String, String) = conn
        .query_row(
            "SELECT status, result FROM turns WHERE session_id=?1",
            [session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("exactly one durable turn");
    assert_eq!(turn_status, "completed");
    let journal: serde_json::Value = serde_json::from_str(&journal).unwrap();
    let parts = journal["display_parts"].as_array().expect("public parts");
    assert_eq!(parts.len(), 3, "two distinct reasoning parts and answer");
    assert_eq!(parts[0]["reasoning"], REASONING_STEPS_FIRST);
    assert_eq!(parts[1]["reasoning"], REASONING_STEPS_SECOND);
    assert!(parts[2]["message"].is_number(), "answer is a message part");
    assert!(
        !parts
            .iter()
            .any(|part| part.to_string().contains(REASONING_STEPS_OPAQUE)),
        "encrypted continuation must not enter public parts"
    );
    let input = journal["input"].as_array().expect("durable wire items");
    let answer_index = parts[2]["message"].as_u64().unwrap() as usize;
    assert_eq!(
        input[answer_index]["content"][0]["text"],
        REASONING_STEPS_ANSWER
    );
    let encrypted: Vec<_> = input
        .iter()
        .filter(|item| item["encrypted_content"] == REASONING_STEPS_OPAQUE)
        .collect();
    assert_eq!(encrypted.len(), 2, "separate opaque reasoning items");
    assert_eq!(encrypted[0]["id"], "rs_pty_first");
    assert_eq!(encrypted[1]["id"], "rs_pty_second");
    drop(conn);
    drop(db);

    let mut reopened = PtySession::spawn_sized(fixture.clone(), session, None, 120, 40);
    wait_screen_row(&reopened, REASONING_STEPS_ANSWER, DEADLINE);
    assert_reasoning_steps_collapsed(&reopened);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    toggle_reasoning_steps(&mut reopened, true);
    std::thread::sleep(Duration::from_millis(510));
    toggle_reasoning_steps(&mut reopened, false);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    reopened.send(b"/quit\r");
    let (status, output) = reopened.wait_exit(DEADLINE);
    assert!(status.success() && reopened.restored() && contains(&output, ALT_LEAVE));
    assert!(!contains(&output, REASONING_STEPS_OPAQUE.as_bytes()));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
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

/// Text cells precede the final cursor-position bytes in a ratatui frame. A
/// relative cursor assertion must not derive its baseline from a partial write.
fn settled_cursor(pty: &PtySession) -> (usize, usize) {
    let start = Instant::now();
    let mut previous = pty.snapshot();
    loop {
        std::thread::sleep(POLL);
        let current = pty.snapshot();
        if current.len() == previous.len() {
            return render_screen(&current).cursor;
        }
        assert!(start.elapsed() < DEADLINE, "cursor baseline did not settle");
        previous = current;
    }
}

/// V07 S03: terminal bytes, focused dialogs, editor and actual Responses wire.
#[test]
fn v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v07-keys", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"v07-");
    wait_screen_row(&pty, "v07-", DEADLINE);

    pty.send(b"\x10"); // raw Ctrl+P: opens the palette, never inserts 'p'
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b[112;5:3u"); // CSI-u release of Ctrl+P
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "Esc only closes modal"
    );
    wait_screen_row(&pty, "v07-", DEADLINE);

    pty.send(b"\x1b[122;3u"); // CSI-u Alt+z is not plain 'z'
    pty.send(b"\x1bz"); // legacy ESC-prefix Alt+z is not plain 'z'
    pty.send(b"\x1b[120;1:3u"); // release of 'x' must not insert text
    pty.send(b"draft");
    wait_screen_row(&pty, "v07-draft", DEADLINE);
    pty.send(b"\x1b[13;2u"); // Shift+Enter inserts a newline, not a turn
    pty.send(b"\x1b[13;1:3u"); // release of Enter must not submit
    pty.send(b"line-two");
    wait_screen_row(&pty, "line-two", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());

    pty.send(b"\x10"); // dialog owns focus, editor draft remains unchanged
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"v07-no-match");
    wait_screen_row(&pty, "No results found", DEADLINE);
    pty.send(b"\x03"); // Ctrl+C clears the focused dialog search
    dismissed(&pty, "No results found");
    wait_screen_row(&pty, "Commands", DEADLINE);
    assert!(pty.child.try_wait().unwrap().is_none());
    pty.send(b"\x03"); // empty dialog search: Ctrl+C closes dialog
    dismissed(&pty, "Commands");
    assert!(pty.child.try_wait().unwrap().is_none());
    wait_screen_row(&pty, "v07-draft", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());

    pty.send(b"\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("v07-draft\nline-two")
    );
    wait_idle(&pty);
    pty.send(b"\x1b[13;1:3u"); // release after a completed turn stays inert
    pty.send(b"unsent"); // barrier: input after release has reached the editor
    wait_screen_row(&pty, "unsent", DEADLINE);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "release must not start another provider request"
    );
    pty.send(b"\x03"); // Ctrl+C clears the nonempty focused editor
    wait_screen_absent(&pty, "unsent");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "clear must not submit the discarded draft"
    );
    pty.send(b"\x03"); // empty root exits and restores the terminal
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
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
    assert_eq!(
        persisted(pty.data_dir(), "v07-keys")
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["v07-draft\nline-two"]
    );
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
    let original_cursor = settled_cursor(&pty);
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
    let before_paste = settled_cursor(&pty);
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
    let second_line_cursor = settled_cursor(&pty);
    pty.send(b"\x1b[A\x1b[H"); // navigate the draft without scrolling history
    let start = Instant::now();
    while render_screen(&pty.snapshot()).cursor == second_line_cursor {
        assert!(
            start.elapsed() < DEADLINE,
            "editor did not move inside first line"
        );
        std::thread::sleep(POLL);
    }
    let before = settled_cursor(&pty);
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
    pty.send(b"\x03"); // first Ctrl+C clears the restored unfinished draft
    wait_screen_absent(&pty, "unfinished");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "clear must not submit the restored draft"
    );
    pty.send(b"\x03"); // empty root exits
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
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
    wait_screen_row(&pty, "[Pasted ~1 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03"); // clears the oversized draft, not the process
    wait_screen_absent(&pty, "[Pasted ~1 lines]");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03"); // empty root exits and restores terminal state
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
    assert!(fixture.requests.lock().unwrap().is_empty());
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
    pty.send(b"\x03"); // clears multiline draft after the wheel movement
    wait_screen_absent(&pty, "строка");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert!(fixture.requests.lock().unwrap().is_empty());
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
fn selected_model_is_the_main_wire_model_and_each_turn_keeps_its_footer() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "model-identity", None);
    pty.wait_visible(READY, DEADLINE);

    let off = submit(&mut pty, "first model identity");
    pty.wait_visible_after(off, "echo: first model identity", DEADLINE);
    wait_screen_row(&pty, "T39 model", DEADLINE);
    wait_idle(&pty);

    choose_model(&mut pty, "T39 alt");
    choose_variant(&mut pty, "Default");
    wait_screen_row(&pty, "T39 alt fixture", DEADLINE);
    let off = submit(&mut pty, "second model identity");
    pty.wait_visible_after(off, "echo: second model identity", DEADLINE);
    wait_idle(&pty);
    let rows = render_screen(&pty.snapshot()).rows();
    assert!(
        rows.iter().any(|row| row.contains("T39 model")),
        "old footer: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("T39 alt")),
        "new footer: {rows:?}"
    );

    let requests = fixture.wait_requests(2);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("first model identity")
    );
    assert_eq!(requests[0]["model"], MODEL);
    assert_eq!(
        last_user_text(&requests[1]).as_deref(),
        Some("second model identity")
    );
    assert_eq!(requests[1]["model"], ALT_MODEL);
    let captured = fixture.requests.lock().unwrap();
    let titles: Vec<_> = captured
        .iter()
        .filter(|body| title::is_title(body))
        .collect();
    assert_eq!(
        titles.len(),
        1,
        "untitled session generates one ancillary title"
    );
    assert_eq!(titles[0]["model"], MODEL);
    drop(captured);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());

    let mut pty = PtySession::spawn(fixture.clone(), "model-identity", None);
    pty.wait_visible("echo: second model identity", DEADLINE);
    let rows = render_screen(&pty.snapshot()).rows();
    assert!(
        rows.iter().any(|row| row.contains("T39 model")),
        "old replay: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("T39 alt")),
        "new replay: {rows:?}"
    );
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[tokio::test(flavor = "multi_thread")]
async fn title_cancel_and_watchdog_leave_the_main_turn_completed() {
    let fixture = Fixture::new();
    fixture.hold_title.store(true, Ordering::Relaxed);
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
    let (app, guard, _) = oc_adapters::application::spawn_with_env(
        &fixture.root.path().join("project"),
        &fixture.data_dir(),
        env,
    )
    .await
    .unwrap();
    let session = oc_core::domain::SessionId("title-cancel".into());
    app.create_session(session.clone()).await.unwrap();
    let mut events = app.subscribe();
    let turn = app
        .submit(session.clone(), "title still pending".into())
        .await
        .unwrap();
    let requests = fixture.wait_requests(1);
    assert_eq!(requests[0]["model"], MODEL);
    let start = Instant::now();
    while !fixture.requests.lock().unwrap().iter().any(title::is_title) {
        assert!(start.elapsed() < DEADLINE, "missing title request");
        tokio::time::sleep(POLL).await;
    }
    app.cancel_title(session.clone()).await.unwrap();
    let start = Instant::now();
    while !fixture.title_closed.load(Ordering::Relaxed) {
        assert!(
            start.elapsed() < DEADLINE,
            "cancel did not close ancillary title request"
        );
        tokio::time::sleep(POLL).await;
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("main turn did not finish after title cancellation")
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnInterrupted { turn: id, .. } if id == turn => {
                panic!("cancelled title must not interrupt a committed main turn")
            }
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => panic!("main turn: {error}"),
            _ => {}
        }
    }
    // A different untitled session now exercises the ten-second title
    // watchdog, without turning a completed main answer into an interruption.
    fixture.title_closed.store(false, Ordering::Relaxed);
    let session = oc_core::domain::SessionId("title-timeout".into());
    app.create_session(session.clone()).await.unwrap();
    let turn = app
        .submit(session, "title will timeout".into())
        .await
        .unwrap();
    fixture.wait_requests(2);
    let start = Instant::now();
    while fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| title::is_title(request))
        .count()
        < 2
    {
        assert!(start.elapsed() < DEADLINE, "missing second title request");
        tokio::time::sleep(POLL).await;
    }
    let start = Instant::now();
    while !fixture.title_closed.load(Ordering::Relaxed) {
        assert!(
            start.elapsed() < DEADLINE,
            "title watchdog did not close request"
        );
        tokio::time::sleep(POLL).await;
    }
    assert!(start.elapsed() >= Duration::from_secs(9));
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("committed main did not finish after title watchdog")
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnInterrupted { turn: id, .. } if id == turn => {
                panic!("title watchdog must not interrupt a committed main turn")
            }
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => panic!("main turn: {error}"),
            _ => {}
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        events.try_recv().is_err(),
        "completed main must have one terminal event"
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
        pty.send(b"/rename Retained selection root\r");
        wait_screen_row(&pty, "Retained selection root", DEADLINE);
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
            wait_screen_row(&pty, "Sessions", DEADLINE);
            pty.send(b"Retained selection root\r");
            dismissed(&pty, "Sessions");
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
            .any(|r| r.starts_with("   Untitled session")),
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
    // Selection updates the real prompt metadata; upstream does not emit a
    // selection-time toast or durable switch row before the next submission.
    wait_screen_row(&pty, "Modal 29 fixture · fast", DEADLINE);
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
fn sessions_enter_renamed_foreign_root_preserves_deck_draft_and_uses_target_config() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "local-open-root", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/rename Local return target\r");
    wait_screen_row(&pty, "Local return target", DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let foreign = fixture.root.path().join("other-project");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(
        foreign.join("opencode.json"),
        serde_json::json!({"model":format!("fixture/{ALT_MODEL}")}).to_string(),
    )
    .unwrap();
    conn.execute_batch("INSERT INTO sessions(id,created_at,title) VALUES ('foreign-open-root','1','Foreign open target'); INSERT INTO messages(id,session_id,seq,role,text) VALUES ('foreign-user','foreign-open-root',1,'user','foreign history question'),('foreign-answer','foreign-open-root',2,'assistant','foreign history canary');").unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES ('tui.session_location.foreign-open-root',?1,'1')",[foreign.to_str().unwrap()]).unwrap();
    pty.send(b"local roundtrip draft\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x01\x1b[200~Foreign open target\x1b[201~\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Renamed foreign open target\r");
    dismissed(&pty, "Rename session");
    wait_screen_row(&pty, "local roundtrip draft", DEADLINE);
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Renamed foreign open target\x1b[201~\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "foreign history canary", DEADLINE);
    wait_screen_row(&pty, "other-project", DEADLINE);
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "opening an existing root is provider-free"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let off = submit(&mut pty, "target configuration request");
    pty.wait_visible_after(off, "echo: target configuration request", DEADLINE);
    wait_idle(&pty);
    assert_eq!(fixture.wait_requests(1)[0]["model"], ALT_MODEL);
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Local return target\x1b[201~\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "local roundtrip draft", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let saved: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key=?1",
            [format!(
                "tui.selection.tab_deck:{}",
                serde_json::json!([foreign.to_str().unwrap()])
            )],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap()["sessions"],
        serde_json::json!(["foreign-open-root"])
    );
    pty.send(b"\x03/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_all_projects_selected_actions_keep_current_location_and_draft() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "local-current", None);
    pty.wait_visible(READY, DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let foreign = fixture.root.path().join("foreign-project");
    std::fs::create_dir_all(&foreign).unwrap();
    let foreign = foreign.to_str().unwrap();
    conn.execute_batch("INSERT INTO sessions(id,created_at,title) VALUES ('foreign-listed','1','Foreign selected target'); INSERT INTO sessions(id,created_at,parent_id) VALUES ('foreign-child','1','foreign-listed');").unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES ('tui.session_location.foreign-listed',?1,'1')",[foreign]).unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'1')",rusqlite::params![format!("tui.selection.tab_deck:{}",serde_json::json!([foreign])),serde_json::json!({"version":1,"sessions":["foreign-listed"],"active":"foreign-listed"}).to_string()]).unwrap();
    pty.send(b"local draft preserved\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x01");
    wait_screen_row(&pty, "Foreign selected target", DEADLINE);
    pty.send(b"\x1b[200~Foreign selected target\x1b[201~\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Renamed foreign root\r");
    dismissed(&pty, "Rename session");
    wait_screen_row(&pty, "local draft preserved", DEADLINE);
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='foreign-listed'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Renamed foreign root"
    );
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Renamed foreign root\x1b[201~\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "No sessions found", DEADLINE);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE id IN ('foreign-listed','foreign-child')",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE id='local-current'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let saved: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key=?1",
            [format!(
                "tui.selection.tab_deck:{}",
                serde_json::json!([foreign])
            )],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap()["sessions"],
        serde_json::json!([])
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "local draft preserved", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_bracketed_paste_queries_owner_for_root_outside_first_fifty() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "sessions-paste-current", None);
    pty.wait_visible(READY, DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let location: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key='tui.session_location.sessions-paste-current'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for i in 0..61 {
        let id = format!("reverse-{:02}", 61 - i);
        let title = if i == 0 {
            "Unique oldest pasted target".to_string()
        } else {
            format!("Recent root {i}")
        };
        conn.execute(
            "INSERT INTO sessions(id,created_at,title) VALUES (?1,strftime('%s','now'),?2)",
            rusqlite::params![id, title],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'1')",
            rusqlite::params![format!("tui.session_location.{id}"), location],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES (?1,'session_created','{}')",
            [id],
        )
        .unwrap();
    }
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    wait_screen_absent(&pty, "Unique oldest pasted target");
    pty.send(b"\x1b[200~Unique oldest pasted target\x1b[201~");
    wait_screen_row(&pty, "Unique oldest pasted target", DEADLINE);
    pty.send(b"\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Paste reached selected root\r");
    dismissed(&pty, "Rename session");
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='reverse-61'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Paste reached selected root"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_selected_rename_and_confirmed_delete_reach_real_owner_without_provider_work() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "sessions-actions", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x12"); // pinned Ctrl+R acts on the selected row
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Real selected title\r");
    wait_screen_row(&pty, "Real selected title", DEADLINE);
    dismissed(&pty, "Rename session");
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='sessions-actions'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Real selected title"
    );
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Real selected title", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "No sessions available", DEADLINE);
    pty.send(b"\x04\x12\r"); // empty actions and Enter have no target
    wait_screen_row(&pty, "No sessions available", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
}

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
    wait_screen_row(&pty, "T39 alt fixture · fast", DEADLINE);
    let off = submit(&mut pty, "original session");
    pty.wait_visible_after(off, "echo: original session", DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Original session root\r");
    wait_screen_row(&pty, "Original session root", DEADLINE);
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
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"Original session root\r");
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
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Scoped alpha root\r");
    wait_screen_row(&pty, "Scoped alpha root", DEADLINE);
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    // A is focused and newest after its real title update. B is the only
    // other root and must still receive its genuine automatic first title.
    pty.send(b"\x1b[B\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "T39 model fixture", DEADLINE);
    let off = submit(&mut pty, "B different model");
    pty.wait_visible_after(off, "echo: B different model", DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Scoped beta root\r");
    wait_screen_row(&pty, "Scoped beta root", DEADLINE);
    pty.send(b"/continue\rScoped alpha root\r");
    dismissed(&pty, "Sessions");
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
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    choose_model(&mut pty, "T39 model"); // agent-specific override
    let off = submit(&mut pty, "A second agent override");
    pty.wait_visible_after(off, "echo: A second agent override", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rplain\r");
    wait_screen_row(&pty, "Plain ·", DEADLINE);
    dismissed(&pty, "Select agent");
    let off = submit(&mut pty, "A first agent restored");
    pty.wait_visible_after(off, "echo: A first agent restored", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());

    let mut pty = PtySession::spawn(fixture.clone(), "scope-a", None);
    pty.wait_visible("Scoped alpha root", DEADLINE);
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
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    let off = submit(&mut pty, "A second agent restart");
    pty.wait_visible_after(off, "echo: A second agent restart", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/continue\rScoped beta root\r");
    dismissed(&pty, "Sessions");
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
    wait_screen_row(&pty, "T39 alt fixture · fast", DEADLINE);

    let off = submit(&mut pty, "hello model");
    pty.wait_visible_after(off, "echo: hello model", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);

    // Agent picker: prompt and pinned model come from the agent definition.
    pty.send(b"/agents\r");
    wait_screen_row(&pty, "Select agent", DEADLINE);
    wait_screen_row(&pty, "t39agent", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("agent: t39agent"))
    );

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
    wait_screen_row(&pty, "Sessions", DEADLINE);
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
    // The refused slash suggestion remains visible while its draft is editable;
    // dismiss that overlay before checking that no session dialog opened.
    pty.send(b"\x1b");
    let start = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("/sessions") && r.contains("Switch session"))
    {
        assert!(
            start.elapsed() < DEADLINE,
            "slash overlay was not dismissed"
        );
        std::thread::sleep(POLL);
    }
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Sessions"))
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
    // Persist an actual unique title. The picker orders by real update time,
    // so an ID-based Down shortcut no longer identifies this exact root.
    let conn = rusqlite::Connection::open(data_dir.join("oc.sqlite")).unwrap();
    conn.execute(
        "UPDATE sessions SET title=?1 WHERE id=?2",
        ["Bounded short window zero", "s-other-00"],
    )
    .unwrap();
    drop(conn);
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
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"Bounded short window zero\r");
    dismissed(&pty, "Sessions");
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
        "title search resumed the exact requested root"
    );
}

/// S07: identical current transcript tails under the same PTY workload, with
/// an empty versus a 3000-row older archive. These are process measurements,
/// not estimates derived from the view-model's retained-bytes accounting.
#[test]
fn s07_pty_equal_view_archive_resource_samples() {
    let small = measure_s07(0);
    let large = measure_s07(3000);
    assert_eq!(small.viewport, large.viewport, "active transcript differs");
    assert_eq!(
        small.after_viewport, large.after_viewport,
        "recovered viewport differs"
    );
    assert_eq!(small.request, large.request, "Responses requests differ");
    assert_eq!(small.requests.len(), 2, "small: two provider responses");
    assert_eq!(large.requests.len(), 2, "large: two provider responses");
    assert_eq!(
        small.requests, large.requests,
        "full Responses request vectors differ"
    );
    assert_eq!(
        small.request["input"]
            .as_array()
            .expect("Responses input")
            .len(),
        202,
        "200 shared messages, current prompt, and common preamble"
    );
    assert_eq!(
        small.active_view, large.active_view,
        "streamed active view differs"
    );
    assert_eq!(
        small.active_cursor, large.active_cursor,
        "active cursor differs"
    );
    for (label, run) in [("small", &small), ("large", &large)] {
        assert_eq!(
            run.viewport, run.after_viewport,
            "{label}: viewport not restored"
        );
        assert!(
            run.active_view
                .iter()
                .any(|row| row.contains("S07_STREAM_FRAGMENT_42"))
                && run
                    .active_view
                    .iter()
                    .any(|row| row.contains("S07_STREAM_DONE_43")),
            "{label}: incomplete streamed view: {:?}",
            run.active_view
        );
        assert_eq!(last_user_text(&run.request).as_deref(), Some(S07_PROMPT));
        assert_eq!(
            last_user_text(&run.requests[1]).as_deref(),
            Some(S07_PROMPT)
        );
        let outputs = run.requests[1]["input"]
            .as_array()
            .expect("tool output input");
        let read_outputs: Vec<_> = outputs
            .iter()
            .filter(|item| item["type"] == "function_call_output" && item["call_id"] == "call_t39")
            .collect();
        assert_eq!(read_outputs.len(), 1, "{label}: exactly one read result");
        assert!(
            read_outputs[0]["output"]
                .as_str()
                .is_some_and(|text| text == S07_NOTE.trim_end()),
            "{label}: read must return the seeded file: {read_outputs:?}"
        );
        assert_eq!(
            run.tool_ops.len(),
            2,
            "{label}: seeded bash and one real read"
        );
        assert!(
            run.tool_ops
                .iter()
                .any(|(name, state, output)| name == "read"
                    && state == "completed"
                    && output.contains(S07_NOTE.trim_end())),
            "{label}: real read outcome absent: {:?}",
            run.tool_ops
        );
        assert!(
            run.tool_ops
                .iter()
                .all(|(name, _, _)| name == "read" || name == "bash"),
            "{label}: unexpected tool execution: {:?}",
            run.tool_ops
        );
        assert_eq!(run.request["stream"], true);
        assert_eq!(run.request["model"], MODEL);
        let metrics = &run.metrics;
        let rows = metrics["window_rows"].as_u64().expect("window rows");
        let bytes = metrics["retained_bytes"].as_u64().expect("retained bytes");
        assert!(
            rows <= oc_tui::history::WINDOW_ROWS as u64,
            "{label}: {rows}"
        );
        assert!(
            bytes <= (oc_tui::history::WINDOW_BYTES + oc_tui::app::MAX_INPUT_BYTES) as u64,
            "{label}: {bytes}"
        );
        assert_eq!(metrics["window_total"].as_u64(), Some(202 + run.archive));
        let frames = metrics["frame_count"].as_u64().expect("frames");
        let sum = metrics["frame_sum_ns"].as_u64().expect("draw sum");
        let max = metrics["frame_max_ns"].as_u64().expect("draw max");
        let queue_peak = metrics["worker_event_queue_peak"]
            .as_u64()
            .expect("worker event queue peak");
        let queue_lagged = metrics["worker_event_queue_lagged"]
            .as_u64()
            .expect("overwritten worker events");
        let live_text_current = metrics["live_text_bytes_current"]
            .as_u64()
            .expect("live text current");
        let live_text_peak = metrics["live_text_bytes_peak"]
            .as_u64()
            .expect("live text peak");
        let live_reasoning_current = metrics["live_reasoning_bytes_current"]
            .as_u64()
            .expect("live reasoning current");
        let live_reasoning_peak = metrics["live_reasoning_bytes_peak"]
            .as_u64()
            .expect("live reasoning peak");
        let live_parts_current = metrics["live_part_count_current"]
            .as_u64()
            .expect("live parts current");
        let live_parts_peak = metrics["live_part_count_peak"]
            .as_u64()
            .expect("live parts peak");
        let cache_current = metrics["markdown_cache_retained_bytes_current"]
            .as_u64()
            .expect("Markdown cache current");
        let cache_peak = metrics["markdown_cache_retained_bytes_peak"]
            .as_u64()
            .expect("Markdown cache peak");
        assert!(live_text_peak > 0, "{label}: streamed text never sampled");
        assert_eq!(live_text_current, 0, "{label}: live text after exit");
        assert_eq!(
            live_reasoning_current, 0,
            "{label}: live reasoning after exit"
        );
        assert_eq!(live_parts_current, 0, "{label}: live parts after exit");
        assert!(live_reasoning_peak > 0, "{label}: reasoning never sampled");
        assert!(
            live_parts_peak > 0,
            "{label}: frozen reasoning/tool parts never sampled"
        );
        // Each retained route has a 512 KiB styled-block cache and a 2 MiB
        // source-page index; account for the active state plus parked tabs.
        let route_count = metrics["tab_count"].as_u64().expect("tab count") + 1;
        let cache_bound = route_count * (512 * 1024 + 2 * 1024 * 1024);
        assert!(
            cache_current <= cache_peak && cache_peak <= cache_bound,
            "{label}: Markdown cache current={cache_current} peak={cache_peak} bound={cache_bound}"
        );
        assert!(frames > 0 && sum >= max && max > 0);
        println!(
            "S07 {label}: archive={} rss_kb={} pss_kb={} hwm_kb={} cpu_ticks={} child_max={} retained_bytes={} window_rows={} frames={} draw_sum_ns={} draw_max_ns={} worker_event_queue_peak={} worker_event_queue_lagged={} live_text_bytes_current={} live_text_bytes_peak={} live_reasoning_bytes_current={} live_reasoning_bytes_peak={} live_part_count_current={} live_part_count_peak={} markdown_cache_retained_bytes_current={} markdown_cache_retained_bytes_peak={} markdown_cache_bound={} elapsed_ms={}",
            run.archive,
            run.peak_rss_kb,
            run.peak_pss_kb,
            run.peak_hwm_kb,
            run.cpu_ticks,
            run.max_children,
            bytes,
            rows,
            frames,
            sum,
            max,
            queue_peak,
            queue_lagged,
            live_text_current,
            live_text_peak,
            live_reasoning_current,
            live_reasoning_peak,
            live_parts_current,
            live_parts_peak,
            cache_current,
            cache_peak,
            cache_bound,
            run.elapsed.as_millis()
        );
    }
}

struct S07Run {
    archive: u64,
    viewport: Vec<String>,
    after_viewport: Vec<String>,
    active_view: Vec<String>,
    active_cursor: (usize, usize),
    request: serde_json::Value,
    requests: Vec<serde_json::Value>,
    tool_ops: Vec<(String, String, String)>,
    peak_rss_kb: u64,
    peak_pss_kb: u64,
    peak_hwm_kb: u64,
    cpu_ticks: u64,
    max_children: usize,
    elapsed: Duration,
    metrics: serde_json::Value,
}

#[derive(Clone, Copy)]
struct ProcSample {
    rss_kb: u64,
    pss_kb: u64,
    hwm_kb: u64,
    cpu_ticks: u64,
    children: usize,
}

fn s07_proc_sample(pid: u32) -> ProcSample {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).expect("child status");
    let kb = |name: &str| -> u64 {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .expect("status field")
            .split_whitespace()
            .next()
            .expect("kilobytes")
            .parse()
            .expect("numeric kilobytes")
    };
    let rollup =
        std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).expect("child smaps_rollup");
    let pss_kb = rollup
        .lines()
        .find_map(|line| line.strip_prefix("Pss:"))
        .expect("Pss")
        .split_whitespace()
        .next()
        .expect("Pss kB")
        .parse()
        .expect("numeric Pss");
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).expect("child stat");
    // comm is parenthesized and may contain spaces; fields after it start at
    // field 3 (state). utime/stime are fields 14/15.
    let (_, fields) = stat.rsplit_once(") ").expect("stat comm");
    let fields: Vec<&str> = fields.split_whitespace().collect();
    let cpu_ticks =
        fields[11].parse::<u64>().expect("utime") + fields[12].parse::<u64>().expect("stime");
    let children =
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).expect("child list");
    ProcSample {
        rss_kb: kb("VmRSS:"),
        hwm_kb: kb("VmHWM:"),
        pss_kb,
        cpu_ticks,
        children: children.split_whitespace().count(),
    }
}

fn s07_visible_tail(pty: &PtySession) -> Vec<String> {
    render_screen(&pty.snapshot())
        .rows()
        .into_iter()
        .filter(|row| row.contains("shared tail"))
        .collect()
}

fn s07_stable_tail(pty: &PtySession, minimum: usize) -> Vec<String> {
    let began = Instant::now();
    let mut previous = Vec::new();
    loop {
        let visible = s07_visible_tail(pty);
        if visible.len() >= minimum && visible == previous {
            return visible;
        }
        assert!(
            began.elapsed() < DEADLINE,
            "current tail did not settle at {minimum} visible rows: {visible:?}"
        );
        previous = visible;
        std::thread::sleep(POLL);
    }
}

fn s07_active_view(pty: &PtySession) -> Vec<String> {
    render_screen(&pty.snapshot())
        .rows()
        .into_iter()
        .filter(|row| {
            row.contains("shared tail")
                || row.contains(S07_PROMPT)
                || row.contains("rust")
                || row.contains("s07_probe")
                || row.contains("S07_STREAM_")
                || row.trim() == "}"
        })
        .collect()
}

fn s07_stable_active_view(pty: &PtySession) -> Vec<String> {
    let began = Instant::now();
    let mut previous = Vec::new();
    loop {
        let visible = s07_active_view(pty);
        if visible.iter().any(|row| row.contains("S07_STREAM_DONE_43")) && visible == previous {
            return visible;
        }
        assert!(
            began.elapsed() < DEADLINE,
            "streamed active view did not settle: {visible:?}"
        );
        previous = visible;
        std::thread::sleep(POLL);
    }
}

fn measure_s07(archive: usize) -> S07Run {
    let fixture = Fixture::new();
    // The 200-message legacy tail costs more than the default fixture model's
    // input budget. Admit this one S07 turn without changing its history size.
    let config = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).expect("S07 config")).expect("config JSON");
    settings["provider"]["fixture"]["models"][MODEL]["limit"]["context"] =
        serde_json::json!(262_144);
    settings["permissions"]["read"] = serde_json::json!("allow");
    // The two provider requests have identical active context. DCP's anchors
    // contain durable message ids, which legitimately differ across archives.
    settings["dcp"]["enabled"] = serde_json::json!(false);
    std::fs::write(&config, settings.to_string()).expect("S07 model context");
    let data_dir = fixture.data_dir();
    let project = fixture.root.path().join("project");
    let note = project.join("s07-note.txt");
    std::fs::write(&note, S07_NOTE).expect("seed S07 read file");
    let session = "s-s07";
    seed_session(&data_dir, &project, session, archive);
    let db = oc_adapters::storage::Db::open(&data_dir).expect("db");
    db.apply_dcp_schema().expect("DCP schema for prune mark");
    let archived = db.read_history_full(session).expect("durable archive");
    assert_eq!(archived.len(), archive);
    let archive_mark = archived.last().map(|(id, role, text)| {
        assert_eq!(role, "assistant");
        assert_eq!(text, &format!("seeded row {:05} payload", archive - 1));
        db.save_prune_mark(session, id).expect("prune old prefix");
        assert_eq!(
            db.prune_bound(session).expect("validated prune bound"),
            Some((id.clone(), archive as i64))
        );
        id.clone()
    });
    assert_eq!(archive_mark.is_some(), archive != 0);
    if archive_mark.is_none() {
        assert_eq!(db.prune_bound(session).expect("empty prune bound"), None);
    }
    for i in 0..200 {
        let role = if (archive + i).is_multiple_of(2) {
            "user"
        } else {
            "assistant"
        };
        db.append_message(session, role, &format!("shared tail {i:02} payload"))
            .expect("shared tail");
    }
    let large_output = "s07-result-line-0123456789\n".repeat(5_000);
    db.record_tool_intent(
        "s07-large-output",
        session,
        None,
        "bash",
        r#"{"argv":["/bin/true"]}"#,
    )
    .expect("large tool intent");
    db.record_tool_outcome("s07-large-output", "completed", Some(&large_output))
        .expect("large tool result");
    drop(db);
    let path = fixture.root.path().join("s07-metrics.json");
    let mut pty = PtySession::spawn(fixture.clone(), session, Some(&path));
    pty.wait_visible(READY, DEADLINE);
    let off = pty.snapshot().len();
    pty.resize(120, 40);
    pty.wait_visible_after(off, "shared tail", DEADLINE);
    wait_screen_row(&pty, "shared tail 199", DEADLINE);
    // A substring may appear in an intermediate resize frame before the
    // transcript fills the viewport. Sample only after the fixed 120x40 tail
    // (eight visible messages) has settled on both independent processes.
    let viewport = s07_stable_tail(&pty, 8);
    let pid = pty.child.id();
    let start = Instant::now();
    let baseline = s07_proc_sample(pid);
    let mut peak_rss_kb = baseline.rss_kb;
    let mut peak_pss_kb = baseline.pss_kb;
    let mut peak_hwm_kb = baseline.hwm_kb;
    let mut max_children = baseline.children;
    let mut sample = || {
        let now = s07_proc_sample(pid);
        peak_rss_kb = peak_rss_kb.max(now.rss_kb);
        peak_pss_kb = peak_pss_kb.max(now.pss_kb);
        peak_hwm_kb = peak_hwm_kb.max(now.hwm_kb);
        max_children = max_children.max(now.children);
        now
    };
    // Scroll away and repin, resize twice, then open and search a dialog.
    // The same events and geometry are used in both independent processes.
    pty.send(b"\x1b[<64;1;1M");
    wait_screen_row(&pty, "Jump to latest", DEADLINE);
    sample();
    pty.send(b"\x1b[<65;1;1M");
    let began = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("Jump to latest"))
    {
        assert!(began.elapsed() < DEADLINE, "scroll did not repin");
        std::thread::sleep(POLL);
    }
    sample();
    let off = pty.snapshot().len();
    pty.resize(100, 30);
    pty.wait_visible_after(off, "shared tail", DEADLINE);
    sample();
    let off = pty.snapshot().len();
    pty.resize(120, 40);
    pty.wait_visible_after(off, "shared tail", DEADLINE);
    sample();
    // A single real bracketed paste is an editor action, not 160 Enter
    // submissions. Delete its compact chip before restoring the same view.
    let paste = (0..160)
        .map(|i| format!("multiline-{i:03}-resource-probe"))
        .collect::<Vec<_>>()
        .join("\n");
    pty.send(format!("\x1b[200~{paste}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~160 lines]", DEADLINE);
    sample();
    pty.send(b"\x7f");
    let began = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("[Pasted ~160 lines]"))
    {
        assert!(began.elapsed() < DEADLINE, "paste chip was not deleted");
        std::thread::sleep(POLL);
    }
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"/cards\r");
    wait_screen_row(&pty, "cards | newest first", DEADLINE);
    wait_screen_row(&pty, "bash completed", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "operation s07-large-output", DEADLINE);
    wait_screen_row(&pty, "enter next page", DEADLINE);
    sample();
    pty.send(b"\r");
    wait_screen_row(&pty, "bytes", DEADLINE);
    sample();
    pty.send(b"\x1b");
    wait_screen_row(&pty, "cards | newest first", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "cards | newest first");
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"s07-no-match");
    wait_screen_row(&pty, "No results found", DEADLINE);
    sample();
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    let after_viewport = s07_stable_tail(&pty, viewport.len());
    // Submit the same real Responses turn in both processes. The fake peer
    // holds completion until the open fenced code is visibly rendered here.
    let stream_from = submit(&mut pty, S07_PROMPT);
    let requests = fixture.wait_requests(2);
    assert_eq!(
        requests.len(),
        2,
        "read must finish before streamed response"
    );
    let request = requests[0].clone();
    assert_eq!(last_user_text(&request).as_deref(), Some(S07_PROMPT));
    assert_eq!(request["stream"], true);
    assert!(has_function_call_output(&requests[1]));
    pty.wait_visible_after(stream_from, "S07_STREAM_FRAGMENT_42", DEADLINE);
    wait_screen_row(&pty, "S07_STREAM_FRAGMENT_42", DEADLINE);
    let partial = render_screen(&pty.snapshot()).rows();
    assert!(
        partial.iter().any(|row| row.contains("esc interrupt")),
        "the fenced fragment must render while the turn is active: {partial:?}"
    );
    assert!(
        !partial.iter().any(|row| row.contains("S07_STREAM_DONE_43"))
            && !fixture.s07_continue.load(Ordering::Relaxed),
        "completion must still be held at the partial frame"
    );
    sample();
    fixture.s07_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&pty, "S07_STREAM_DONE_43", DEADLINE);
    wait_idle(&pty);
    let active_view = s07_stable_active_view(&pty);
    let active_cursor = render_screen(&pty.snapshot()).cursor;
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("esc interrupt") || row.contains("submission pending")),
        "stream must finish idle"
    );
    let end = sample();
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    let elapsed = start.elapsed();
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        fixture
            .requests
            .lock()
            .expect("requests")
            .iter()
            .filter(|request| !title::is_title(request))
            .count(),
        2,
        "exactly two S07 provider requests after shutdown"
    );
    assert_eq!(
        std::fs::read_to_string(&note).expect("read file intact"),
        S07_NOTE
    );
    let history = persisted(pty.data_dir(), session);
    assert_eq!(history.len(), archive + 202, "archive remains durable");
    if archive != 0 {
        assert_eq!(
            history.first(),
            Some(&("user".into(), "seeded row 00000 payload".into()))
        );
        assert_eq!(
            history[archive - 1],
            (
                "assistant".into(),
                format!("seeded row {:05} payload", archive - 1)
            )
        );
    }
    let post_turn_db = oc_adapters::storage::Db::open(pty.data_dir()).expect("post-turn db");
    let post_turn_full = post_turn_db
        .read_history_full(session)
        .expect("post-turn full history");
    assert_eq!(post_turn_full.len(), archive + 202);
    assert_eq!(
        &post_turn_full[..archive],
        archived.as_slice(),
        "all older archive ids, roles and texts remain durable"
    );
    assert_eq!(
        post_turn_db
            .prune_bound(session)
            .expect("durable prune bound"),
        archive_mark.map(|id| (id, archive as i64))
    );
    drop(post_turn_db);
    assert_eq!(
        history.last(),
        Some(&("assistant".to_string(), S07_ANSWER.to_string()))
    );
    assert_eq!(
        history[history.len() - 2],
        ("user".to_string(), S07_PROMPT.to_string())
    );
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "child not reaped"
    );
    assert_eq!(max_children, 0, "unexpected child processes");
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("metrics file")).expect("metrics JSON");
    let tool_db = oc_adapters::storage::Db::open(pty.data_dir()).expect("tool db");
    let tool_ops = tool_db
        .list_tool_ops(session)
        .expect("tool operations")
        .into_iter()
        .map(|op| (op.name, op.state, op.output.unwrap_or_default()))
        .collect();
    S07Run {
        archive: archive as u64,
        viewport,
        after_viewport,
        active_view,
        active_cursor,
        request,
        requests,
        tool_ops,
        peak_rss_kb,
        peak_pss_kb,
        peak_hwm_kb,
        cpu_ticks: end.cpu_ticks.saturating_sub(baseline.cpu_ticks),
        max_children,
        elapsed,
        metrics,
    }
}
