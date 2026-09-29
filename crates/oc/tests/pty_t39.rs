//! T39 (F14): PTY qualification for panels, terminal recovery and bounded
//! backing state against the real `oc tui` binary.
//!
//! The binary runs under a real PTY with an isolated HOME and a scripted
//! native Responses peer. Assertions inspect real behaviour: rendered bytes,
//! provider request bodies, exit codes, slave termios state, durable rows and
//! the opt-in view-metrics probe — never render snapshots alone.

use std::io::{Read, Write};
#[path = "support/screen.rs"]
mod tui_screen;
use tui_screen::render_screen;
#[path = "support/terminal.rs"]
mod terminal;
#[path = "support/title.rs"]
mod title;
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
const VIS38_RESOURCE_FOCUS: &str = "VIS38_RESOURCE";
const VIS38_RESOURCE_SUMMARY: &str =
    "VIS38 bounded real summary 中文: preserve the closed requirements. ";
const REASONING_CLICK_PROMPT: &str = "pty reasoning header click";
const REASONING_CLICK_BODY: &str = "PTY_REASONING_CLICK_BODY_7819";
const REASONING_CLICK_ANSWER: &str = "PTY_REASONING_CLICK_FINAL_3826";
const REASONING_STEPS_PROMPT: &str = "pty adjacent reasoning steps";
const REASONING_STEPS_FIRST: &str = "**Inspecting**\n\nPTY_STEPS_FIRST_BODY_6148";
const REASONING_STEPS_SECOND: &str = "**Verifying**\n\nPTY_STEPS_SECOND_BODY_7349";
const REASONING_STEPS_ANSWER: &str = "PTY_STEPS_FINAL_1625";
const REASONING_STEPS_OPAQUE: &str = "PTY_STEPS_ENCRYPTED_NEVER_RENDER_8972";
// Effective owner primary for fixtures without an explicit default_agent.
const DEFAULT_AGENT: &str = "build";

/// Scripted native Responses peer plus isolated HOME/config.
struct Fixture {
    root: tempfile::TempDir,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    models: Arc<Mutex<serde_json::Value>>,
    discoveries: Arc<AtomicUsize>,
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
        let models = Arc::new(Mutex::new(serde_json::json!({"object":"list", "data":[]})));
        let discovered = models.clone();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let discovery_count = discoveries.clone();
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
                        let Some(body) = read_request(&mut socket, &discovered, &discovery_count)
                        else {
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
            models,
            discoveries,
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
            "topic": if prompt.contains(VIS38_RESOURCE_FOCUS) { "VIS38 resource 中文" } else { "t39 span" },
            "content": [{
                "startId": first,
                "endId": last,
                "summary": if prompt.contains(VIS38_RESOURCE_FOCUS) { VIS38_RESOURCE_SUMMARY.repeat(64) } else { "Compressed early turns into one durable summary.".into() }
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

fn read_request(
    socket: &mut TcpStream,
    models: &Mutex<serde_json::Value>,
    discoveries: &AtomicUsize,
) -> Option<serde_json::Value> {
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
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-not-a-secret\r\n")
    );
    if headers.starts_with("GET /proxy/v1/models HTTP/1.1\r\n") {
        discoveries.fetch_add(1, Ordering::Relaxed);
        let body = models.lock().unwrap().to_string();
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("discovery response");
        socket.flush().expect("discovery flush");
        return None;
    }
    assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
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

fn tab_monotonic_ns() -> u128 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the output timespec is valid; this only reads the host monotonic clock.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    assert_eq!(result, 0);
    time.tv_sec as u128 * 1_000_000_000 + time.tv_nsec as u128
}

fn tab_idle_window(pty: &PtySession) -> (u128, u128) {
    std::thread::sleep(Duration::from_millis(100));
    let start = tab_monotonic_ns();
    let bytes = pty.snapshot().len();
    let cpu = process_cpu_ticks(pty.child.id());
    // Covers the marquee's 600 ms delayed start as well as multiple source frame periods.
    std::thread::sleep(Duration::from_millis(750));
    let end = tab_monotonic_ns();
    assert_eq!(
        pty.snapshot().len(),
        bytes,
        "idle tab scope retained terminal writes"
    );
    assert!(
        process_cpu_ticks(pty.child.id()) - cpu <= 1,
        "idle tab scope consumed CPU"
    );
    (start, end)
}

fn assert_tab_idle_metrics(metrics: &serde_json::Value, windows: &[(&str, (u128, u128))]) {
    for (field, count) in [
        ("frame_monotonic_ns", "frame_count"),
        ("wake_monotonic_ns", "wakeups"),
    ] {
        let samples = metrics[field].as_array().unwrap();
        assert_eq!(
            samples.len() as u64,
            metrics[count].as_u64().unwrap(),
            "bounded probe must cover the whole test window"
        );
        let epoch = metrics["monotonic_epoch_ns"].as_u64().unwrap();
        assert!(samples.iter().all(|at| at.as_u64().unwrap() >= epoch));
        for (label, (start, end)) in windows {
            assert!(
                !samples
                    .iter()
                    .any(|at| (*start..=*end).contains(&u128::from(at.as_u64().unwrap()))),
                "{label}: idle {field} continued inside the real monotonic window"
            );
        }
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
    saved_provider_selection(db, fixture, session, agent, "fixture")
}

fn saved_provider_selection(
    db: &oc_adapters::storage::Db,
    fixture: &Fixture,
    session: &str,
    agent: &str,
    provider: &str,
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
            provider,
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
    threads: u64,
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
        threads: kb("Threads:"),
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

#[path = "pty_t39/interaction.rs"]
mod interaction;
#[path = "pty_t39/lifecycle.rs"]
mod lifecycle;
#[path = "pty_t39/plugin_admission.rs"]
mod plugin_admission;
