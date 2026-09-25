//! T41 (AUD35): one golden workflow through the actual binary.
//!
//! The campaign runs the real `oc run` process four times against one
//! isolated HOME/data root and a *strict* scripted Responses peer: every
//! request must match the next expected step (roles, tool definitions, call
//! ids, prior call/output pairs, DCP anchors), and an unexpected request is
//! recorded as a violation instead of being answered with success.
//!
//! Paths covered: effective config (global + project A rules) -> Responses
//! tool loop (apply_patch + bash, two rounds) -> MCP tool call through a
//! configured remote server -> model-driven compress -> process restart on
//! the same session -> configured workspace switch to project B, including
//! the cross-location refusal of A's session.
//!
//! Independent checks: the fixture's own `cargo test` run from this process,
//! durable rows (turns, tool operations, compression blocks), MCP records,
//! exit codes and the request history.

use std::collections::VecDeque;
#[path = "support/title.rs"]
mod title;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_oc");
const MODEL: &str = "golden-model";
const SESSION_A: &str = "s-golden-a";
const SESSION_B: &str = "s-golden-b";
const POLL: Duration = Duration::from_millis(10);
const DEADLINE: Duration = Duration::from_secs(60);
const A_RULE: &str = "AUD35_PROJECT_A_RULE_9f21";
const B_RULE: &str = "AUD35_PROJECT_B_RULE_4c7e";
const MCP_TEXT: &str = "golden:search";
const PATCH: &str = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n*** End Patch";

// ---------------------------------------------------------------------------
// Strict scripted Responses peer.
// ---------------------------------------------------------------------------

type Check = Box<dyn Fn(&Value) -> Result<(), String> + Send>;

struct Step {
    label: String,
    check: Check,
    reply: String,
}

struct Peer {
    url: String,
    steps: Arc<Mutex<VecDeque<Step>>>,
    seen: Arc<Mutex<Vec<Value>>>,
    violations: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start(script: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/proxy/v1",
            listener.local_addr().expect("addr").port()
        );
        listener.set_nonblocking(true).expect("nonblocking");
        let steps = Arc::new(Mutex::new(VecDeque::from(script)));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let violations = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let steps_out = steps.clone();
        let seen_out = seen.clone();
        let violations_out = violations.clone();
        let stopping = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket
                            .set_read_timeout(Some(DEADLINE))
                            .expect("read timeout");
                        let Some(body) = read_request(&mut socket) else {
                            continue;
                        };
                        seen_out.lock().expect("seen").push(body.clone());
                        if is_genuine_title(&body) && title::respond(&mut socket, &body) {
                            continue;
                        }
                        let step = steps_out.lock().expect("steps").pop_front();
                        let Some(step) = step else {
                            violations_out.lock().expect("violations").push(format!(
                                "unexpected request {}: {}",
                                seen_out.lock().expect("seen").len(),
                                summarize(&body)
                            ));
                            let _ = socket.write_all(
                                b"HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                            );
                            continue;
                        };
                        if let Err(reason) = (step.check)(&body) {
                            violations_out
                                .lock()
                                .expect("violations")
                                .push(format!("step {}: {reason}", step.label));
                        }
                        let _ = socket.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                step.reply.len(),
                                step.reply
                            )
                            .as_bytes(),
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
        });
        Self {
            url,
            steps,
            seen,
            violations,
            stop,
            handle: Some(handle),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.seen.lock().expect("seen").clone()
    }

    fn violations(&self) -> Vec<String> {
        self.violations.lock().expect("violations").clone()
    }

    fn pending_steps(&self) -> usize {
        self.steps.lock().expect("steps").len()
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let result = handle.join();
            if !std::thread::panicking() {
                result.expect("peer thread");
            }
        }
    }
}

fn read_request(socket: &mut TcpStream) -> Option<Value> {
    let mut reader = BufReader::new(socket.try_clone().ok()?);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            content_length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

fn summarize(body: &Value) -> String {
    let items = body["input"].as_array().map(Vec::len).unwrap_or(0);
    format!("input items {items}")
}

fn is_genuine_title(body: &Value) -> bool {
    body["model"] == MODEL
        && body["tools"] == json!([])
        && body["max_output_tokens"] == 256
        && body["input"][0]
            == json!({
                "type":"message", "role":"developer", "content":[{"type":"input_text",
                "text":"Generate a short session title from the user's request. Output only the title, in at most 100 characters."}]
            })
        && body["input"].as_array().is_some_and(|items| {
            items.len() == 2
                && items[1]["role"] == "user"
                && items[1]["content"][0]["text"].as_str().is_some_and(|text| {
                    text.starts_with("fix the add function ") || text == "report the workspace"
                })
        })
}

fn sse_text(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\ndata: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5}}}}}}\n\n",
        Value::String(text.to_string())
    )
}

fn sse_tool_calls(calls: &[(&str, &str, Value)]) -> String {
    let mut out = String::new();
    for (call_id, name, args) in calls {
        let item_id = format!("fc_{call_id}");
        out.push_str(&format!(
            "data: {}\n\n",
            json!({"type": "response.output_item.added", "item": {
                "type": "function_call", "id": item_id, "call_id": call_id,
                "name": name, "arguments": "", "status": "in_progress"}})
        ));
        out.push_str(&format!(
            "data: {}\n\n",
            json!({"type": "response.function_call_arguments.delta",
                "item_id": item_id, "delta": args.to_string()})
        ));
        out.push_str(&format!(
            "data: {}\n\n",
            json!({"type": "response.output_item.done", "item": {
                "type": "function_call", "id": item_id, "call_id": call_id,
                "name": name, "arguments": args.to_string(), "status": "completed"}})
        ));
    }
    out.push_str("data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n");
    out
}

// ---------------------------------------------------------------------------
// Request inspection helpers.
// ---------------------------------------------------------------------------

fn texts(body: &Value) -> Vec<String> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.pointer("/content/0/text")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn tool_names(body: &Value) -> Vec<String> {
    body["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn function_calls(body: &Value) -> Vec<(String, String)> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item["type"] == "function_call")
                .filter_map(|item| {
                    Some((
                        item["call_id"].as_str()?.to_string(),
                        item["name"].as_str().unwrap_or_default().to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn function_output(body: &Value, call_id: &str) -> Option<String> {
    body["input"].as_array()?.iter().find_map(|item| {
        (item["type"] == "function_call_output" && item["call_id"] == call_id)
            .then(|| item["output"].as_str().map(str::to_owned))
            .flatten()
    })
}

fn dcp_anchors(body: &Value) -> Option<Vec<Value>> {
    let text = texts(body)
        .into_iter()
        .find(|text| text.starts_with("DCP context anchors"))?;
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    serde_json::from_str(&text[start..=end]).ok()
}

fn last_user_text(body: &Value) -> Option<String> {
    body["input"].as_array()?.iter().rev().find_map(|item| {
        (item["role"] == "user")
            .then(|| item.pointer("/content/0/text").and_then(|v| v.as_str()))
            .flatten()
            .map(str::to_owned)
    })
}

// ---------------------------------------------------------------------------
// Minimal streamable-HTTP MCP server.
// ---------------------------------------------------------------------------

struct Mcp {
    url: String,
    calls: Arc<Mutex<Vec<Value>>>,
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
                        let Some(body) = read_request(&mut socket) else {
                            continue;
                        };
                        let id = body.get("id").cloned().unwrap_or(Value::Null);
                        match body["method"].as_str().unwrap_or_default() {
                            "initialize" => write_json(
                                &mut socket,
                                id,
                                json!({"protocolVersion": "2025-11-25",
                                       "capabilities": {"tools": {}},
                                       "serverInfo": {"name": "golden", "version": "1"}}),
                            ),
                            "notifications/initialized" => {
                                let _ = socket.write_all(
                                    b"HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                                );
                            }
                            "tools/list" => write_json(
                                &mut socket,
                                id,
                                json!({"tools": [{
                                    "name": "search",
                                    "description": "golden search",
                                    "inputSchema": {"type": "object",
                                        "properties": {"query": {"type": "string"}},
                                        "required": ["query"]}}]}),
                            ),
                            "tools/call" => {
                                calls_out.lock().expect("mcp calls").push(
                                    body.pointer("/params/arguments")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                );
                                write_json(
                                    &mut socket,
                                    id,
                                    json!({"content": [{"type": "text", "text": MCP_TEXT}],
                                           "isError": false}),
                                );
                            }
                            _ => write_json(
                                &mut socket,
                                id,
                                json!({"error": {"code": -32601, "message": "unknown"}}),
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

    fn calls(&self) -> Vec<Value> {
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

fn write_json(socket: &mut TcpStream, id: Value, result: Value) {
    let body = serde_json::to_vec(&json!({"jsonrpc": "2.0", "id": id, "result": result}))
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

// ---------------------------------------------------------------------------
// Fixture: isolated HOME, project A (buggy crate), project B.
// ---------------------------------------------------------------------------

struct Fixture {
    root: tempfile::TempDir,
    peer: Peer,
    mcp: Mcp,
}

impl Fixture {
    fn new(script: Vec<Step>) -> Self {
        let peer = Peer::start(script);
        let mcp = Mcp::start();
        let root = tempfile::tempdir().expect("root");
        let config = root.path().join("home/config/opencode");
        std::fs::create_dir_all(&config).expect("config");
        std::fs::write(
            config.join("opencode.json"),
            json!({
                "model": format!("fixture/{MODEL}"),
                "permissions": {
                    "read": "allow", "apply_patch": "allow", "bash": "allow",
                    "webfetch": "allow", "skill": "allow", "compress": "allow",
                    "codex_web__search": "allow"
                },
                "dcp": {"enabled": true},
                "provider": {"fixture": {
                    "npm": "@ai-sdk/openai",
                    "options": {"baseURL": peer.url, "apiKey": "golden-key",
                                "timeout": false, "setCacheKey": false},
                    "models": {MODEL: {"name": "Golden fixture",
                                      "limit": {"context": 200_000, "output": 8_000}}}
                }},
                "mcp": {"codex_web": {"type": "remote", "url": mcp.url,
                                      "enabled": true, "oauth": false,
                                      "headers": {"Authorization": "Bearer golden-token"},
                                      "timeout": 3000}}
            })
            .to_string(),
        )
        .expect("config");

        // Project A: a small crate with one broken function and its test.
        let project_a = root.path().join("project-a");
        std::fs::create_dir_all(project_a.join("src")).expect("project a src");
        std::fs::write(
            project_a.join("Cargo.toml"),
            "[package]\nname = \"golden-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\n",
        )
        .expect("cargo toml");
        std::fs::write(
            project_a.join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n",
        )
        .expect("lib");
        std::fs::create_dir_all(project_a.join("tests")).expect("tests");
        std::fs::write(
            project_a.join("tests/add.rs"),
            "use golden_fixture::add;\n\n#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n",
        )
        .expect("test");
        std::fs::write(
            project_a.join("opencode.jsonc"),
            "{\n  // Project A local configuration.\n}\n",
        )
        .expect("project a config");
        std::fs::write(project_a.join("AGENTS.md"), format!("{A_RULE}\n")).expect("a rule");

        // Project B: same HOME config, different location rules.
        let project_b = root.path().join("project-b");
        std::fs::create_dir_all(&project_b).expect("project b");
        std::fs::write(project_b.join("opencode.json"), "{}").expect("project b config");
        std::fs::write(project_b.join("AGENTS.md"), format!("{B_RULE}\n")).expect("b rule");

        Self { root, peer, mcp }
    }

    fn project_a(&self) -> PathBuf {
        self.root.path().join("project-a")
    }

    fn project_b(&self) -> PathBuf {
        self.root.path().join("project-b")
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path().join("home/data/oc")
    }

    fn spawn(&self, project: &Path, session: &str, prompt: &str, label: &str) -> Child {
        let home = self.root.path().join("home");
        let stderr = home.join(format!("{label}.stderr"));
        // Production parity: the binary receives the real observed toolchain
        // environment, exactly like an interactive shell would.
        let parent: Vec<(String, String)> = std::env::vars().collect();
        Command::new(BIN)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("XDG_CACHE_HOME", home.join("cache"))
            .env("XDG_STATE_HOME", home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .envs(parent)
            .current_dir(project)
            .args(["run", "--session", session, prompt])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&stderr).expect("stderr"))
            .spawn()
            .expect("actual binary")
    }

    fn run(&self, project: &Path, session: &str, prompt: &str, label: &str) -> Run {
        let mut child = self.spawn(project, session, prompt, label);
        let start = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            assert!(start.elapsed() < DEADLINE, "{label} timed out");
            std::thread::sleep(POLL);
        };
        let mut stderr = String::new();
        if let Ok(mut file) =
            std::fs::File::open(self.root.path().join(format!("home/{label}.stderr")))
        {
            let _ = file.read_to_string(&mut stderr);
        }
        Run { status, stderr }
    }
}

struct Run {
    status: std::process::ExitStatus,
    stderr: String,
}

// ---------------------------------------------------------------------------
// The campaign.
// ---------------------------------------------------------------------------

fn step(
    label: &str,
    check: impl Fn(&Value) -> Result<(), String> + Send + 'static,
    reply: String,
) -> Step {
    Step {
        label: label.to_string(),
        check: Box::new(check),
        reply,
    }
}

#[test]
fn aud35_binary_golden_workflow() {
    let mut summary: Vec<(String, String, String)> = Vec::new();
    let script = vec![
        step(
            "turn1-round1",
            |body| {
                let prompt = last_user_text(body).unwrap_or_default();
                if !prompt.contains("fix the add function") {
                    return Err(format!("unexpected prompt {prompt:?}"));
                }
                let joined = texts(body).join("\n");
                if !joined.contains(A_RULE) {
                    return Err("project A rule missing from the input".to_string());
                }
                let tools = tool_names(body);
                for expected in ["apply_patch", "bash", "read", "codex_web__search"] {
                    if !tools.iter().any(|name| name == expected) {
                        return Err(format!("tool {expected} missing from {tools:?}"));
                    }
                }
                Ok(())
            },
            sse_tool_calls(&[
                ("g-patch", "apply_patch", json!({"patchText": PATCH})),
                (
                    "g-test",
                    "bash",
                    json!({"argv": ["cargo", "test", "--quiet"]}),
                ),
            ]),
        ),
        step(
            "turn1-round2",
            |body| {
                let calls = function_calls(body);
                if calls
                    != vec![
                        ("g-patch".to_string(), "apply_patch".to_string()),
                        ("g-test".to_string(), "bash".to_string()),
                    ]
                {
                    return Err(format!("unexpected call pairs {calls:?}"));
                }
                let patch = function_output(body, "g-patch").unwrap_or_default();
                if patch.is_empty() || patch.contains("error") {
                    return Err(format!("patch output {patch:?}"));
                }
                let test = function_output(body, "g-test").unwrap_or_default();
                if !test.contains("test result: ok") {
                    return Err(format!("cargo test output missing: {test:?}"));
                }
                Ok(())
            },
            sse_text("fixed and tested"),
        ),
        step(
            "turn2-round1",
            |body| {
                let joined = texts(body).join("\n");
                if !joined.contains("fix the add function") || !joined.contains("fixed and tested")
                {
                    return Err("restart must replay the prior turn".to_string());
                }
                if !tool_names(body)
                    .iter()
                    .any(|name| name == "codex_web__search")
                {
                    return Err("MCP tool missing after restart".to_string());
                }
                Ok(())
            },
            sse_tool_calls(&[(
                "g-mcp",
                "codex_web__search",
                json!({"query": "rust", "response_length": "short"}),
            )]),
        ),
        step(
            "turn2-round2",
            |body| {
                let output = function_output(body, "g-mcp").unwrap_or_default();
                if !output.contains(MCP_TEXT) {
                    return Err(format!("MCP result missing: {output:?}"));
                }
                Ok(())
            },
            sse_text("searched"),
        ),
    ];
    let fixture = Fixture::new(script);

    // 1. Coding turn through the real binary. The prompt is deliberately
    // large so the later compress step has a measurable gain.
    let coding_prompt = format!("fix the add function {}", "context ".repeat(6_000));
    let run = fixture.run(&fixture.project_a(), SESSION_A, &coding_prompt, "golden-1");
    summary.push((
        "turn1".to_string(),
        if run.status.success() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        run.stderr.trim().to_string(),
    ));

    // Independent verification of the file/tests outcome.
    let tests = Command::new("cargo")
        .args(["test", "--quiet"])
        .current_dir(fixture.project_a())
        .env("CARGO_TARGET_DIR", fixture.project_a().join("target"))
        .output()
        .expect("independent cargo test");
    let tests_ok = tests.status.success();
    summary.push((
        "independent-cargo-test".to_string(),
        if tests_ok { "passed" } else { "failed" }.to_string(),
        String::from_utf8_lossy(&tests.stderr).trim().to_string(),
    ));

    // 2. Restart on the same session: MCP call.
    let run = fixture.run(
        &fixture.project_a(),
        SESSION_A,
        "search the web for rust",
        "golden-2",
    );
    summary.push((
        "turn2-mcp".to_string(),
        if run.status.success() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        run.stderr.trim().to_string(),
    ));

    // 3. Model-driven compress through the DCP anchor lane.
    let anchors = fixture
        .peer
        .requests()
        .last()
        .and_then(dcp_anchors)
        .unwrap_or_default();
    let mut script = Vec::new();
    let first = anchors
        .iter()
        .find(|anchor| anchor["closed"] == true)
        .and_then(|anchor| anchor["id"].as_str())
        .unwrap_or("m0001")
        .to_string();
    let second = anchors
        .iter()
        .filter(|anchor| anchor["closed"] == true)
        .nth(1)
        .and_then(|anchor| anchor["id"].as_str())
        .unwrap_or("m0002")
        .to_string();
    script.push(step(
        "turn3-round1-compress",
        move |body| {
            if dcp_anchors(body).is_none() {
                return Err("DCP anchor lane missing".to_string());
            }
            Ok(())
        },
        sse_tool_calls(&[(
            "g-compress",
            "compress",
            json!({"topic": "golden compression",
                   "content": [{"startId": first, "endId": second,
                                "summary": "golden summary"}]}),
        )]),
    ));
    script.push(step(
        "turn3-round2-compress",
        |body| {
            let output = function_output(body, "g-compress").unwrap_or_default();
            if !output.contains("\"status\":\"compressed\"") {
                return Err(format!("compress output {output:?}"));
            }
            Ok(())
        },
        sse_text("compressed"),
    ));
    fixture.peer.steps.lock().expect("steps").extend(script);
    let run = fixture.run(
        &fixture.project_a(),
        SESSION_A,
        "compress the old turns",
        "golden-3",
    );
    summary.push((
        "turn3-compress".to_string(),
        if run.status.success() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        run.stderr.trim().to_string(),
    ));

    // 4. Workspace switch: project B has its own rules and session.
    fixture.peer.steps.lock().expect("steps").push_back(step(
        "turn4-workspace-b",
        |body| {
            let joined = texts(body).join("\n");
            if !joined.contains(B_RULE) {
                return Err("project B rule missing".to_string());
            }
            if joined.contains(A_RULE) {
                return Err("project A rule leaked into project B".to_string());
            }
            if joined.contains("fix the add function") {
                return Err("project A history leaked into project B".to_string());
            }
            Ok(())
        },
        sse_text("b ready"),
    ));
    let run = fixture.run(
        &fixture.project_b(),
        SESSION_B,
        "report the workspace",
        "golden-4",
    );
    summary.push((
        "turn4-workspace-b".to_string(),
        if run.status.success() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        run.stderr.trim().to_string(),
    ));

    // 5. Cross-location refusal: A's session from project B.
    let run = fixture.run(
        &fixture.project_b(),
        SESSION_A,
        "wrong location",
        "golden-5",
    );
    let refusal = !run.status.success() && run.stderr.contains("belongs to location");
    summary.push((
        "cross-location".to_string(),
        if refusal { "passed" } else { "failed" }.to_string(),
        run.stderr.trim().to_string(),
    ));

    // ---- independent observations -----------------------------------------
    let mcp_calls = fixture.mcp.calls();
    summary.push((
        "mcp-call".to_string(),
        if mcp_calls
            .iter()
            .any(|args| args["query"].as_str() == Some("rust"))
        {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        format!("{mcp_calls:?}"),
    ));

    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).expect("db");
    let history_a = db.read_history(SESSION_A).expect("history a");
    let ops = db.list_tool_ops(SESSION_A).expect("ops");
    let blocks = oc_adapters::dcp::load_blocks(&db, SESSION_A).expect("blocks");
    let history_b = db.read_history(SESSION_B).expect("history b");
    summary.push((
        "durable-rows".to_string(),
        if history_a.len() >= 4
            && ops
                .iter()
                .any(|op| op.op.ends_with("-g-test") && op.state == "completed")
            && !blocks.is_empty()
            && !history_b.is_empty()
        {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        format!(
            "history_a={} ops={} blocks={} history_b={}",
            history_a.len(),
            ops.len(),
            blocks.len(),
            history_b.len()
        ),
    ));

    // Project B request must carry B's rule and no A history.
    assert_eq!(
        fixture
            .peer
            .requests()
            .iter()
            .filter(|r| is_genuine_title(r))
            .count(),
        2,
        "one real title request per new session"
    );
    let b_requests = fixture
        .peer
        .requests()
        .into_iter()
        .filter(|body| !is_genuine_title(body))
        .filter(|body| last_user_text(body).as_deref() == Some("report the workspace"))
        .collect::<Vec<_>>();
    let b_ok = b_requests.len() == 1
        && texts(&b_requests[0]).join("\n").contains(B_RULE)
        && !texts(&b_requests[0]).join("\n").contains(A_RULE)
        && !texts(&b_requests[0])
            .join("\n")
            .contains("fix the add function");
    summary.push((
        "workspace-isolation".to_string(),
        if b_ok { "passed" } else { "failed" }.to_string(),
        format!("requests={}", b_requests.len()),
    ));

    let violations = fixture.peer.violations();
    summary.push((
        "strict-peer".to_string(),
        if violations.is_empty() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        violations.join("; "),
    ));
    summary.push((
        "unexecuted-steps".to_string(),
        if fixture.peer.pending_steps() == 0 {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        format!("{} scripted steps never ran", fixture.peer.pending_steps()),
    ));

    // ---- machine-readable summary -----------------------------------------
    let report = json!({
        "harness": "golden_binary",
        "model": format!("fixture/{MODEL}"),
        "steps": summary.iter().map(|(name, status, detail)| json!({
            "name": name, "status": status, "detail": detail
        })).collect::<Vec<_>>(),
        "counts": {
            "attempted": summary.len(),
            "passed": summary.iter().filter(|(_, s, _)| s == "passed").count(),
            "failed": summary.iter().filter(|(_, s, _)| s == "failed").count(),
            "blocked": 0,
            "skipped": 0,
        }
    });
    let text = serde_json::to_string_pretty(&report).expect("summary json");
    println!("{text}");
    if let Ok(path) = std::env::var("OC_GOLDEN_SUMMARY") {
        std::fs::write(path, &text).expect("summary file");
    }
    assert_eq!(
        report["counts"]["failed"], 0,
        "golden workflow failures: {text}"
    );
}
