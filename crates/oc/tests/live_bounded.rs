//! T41 (AUD36/AUD37): bounded live campaign harness on the product binary.
//!
//! Two entry points share one campaign runner:
//!
//! * `live_bounded_dry_run_branches` (offline, part of the normal suite) runs
//!   the campaign twice against loopback peers: once with an MCP server
//!   configured (the step executes) and once without (the step is recorded as
//!   `blocked`, never silently skipped). This is how every harness branch is
//!   exercised before any paid run.
//! * `live_bounded_campaign` (ignored, credentials-gated) runs the same
//!   campaign against the configured provider. Explicitly invoking it without
//!   credentials is a machine-readable BLOCKED non-success: one JSON object is
//!   printed and the test fails, so no aggregate can turn a skip or an early
//!   return into a live PASS.
//!
//! Model, variant and declared limits come from the real configuration file
//! (`OC_TEST_CONFIG` or `~/.config/opencode/opencode.json[c]`), never from an
//! invented `context = 1_000_000`; the campaign is bounded by a per-call
//! watchdog, a whole-campaign watchdog and a hard step budget.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_oc");
const POLL: Duration = Duration::from_millis(10);
/// Per-process watchdog.
const CALL_TIMEOUT: Duration = Duration::from_secs(300);
/// Whole-campaign watchdog: fixed before the run, never extended afterwards.
const CAMPAIGN_TIMEOUT: Duration = Duration::from_secs(900);
/// Hard step budget: coding, mcp, compress, restart, switch.
const STEP_BUDGET: usize = 5;
const PATCH: &str = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n*** End Patch";

// ---------------------------------------------------------------------------
// Campaign bookkeeping.
// ---------------------------------------------------------------------------

struct StepResult {
    name: &'static str,
    status: &'static str,
    kind: &'static str,
    detail: String,
}

struct Campaign {
    results: Vec<StepResult>,
    started: Instant,
}

impl Campaign {
    fn new() -> Self {
        Self {
            results: Vec::new(),
            started: Instant::now(),
        }
    }

    fn record(&mut self, name: &'static str, ok: bool, kind: &'static str, detail: String) {
        self.results.push(StepResult {
            name,
            status: if ok { "passed" } else { "failed" },
            kind,
            detail,
        });
    }

    fn blocked(&mut self, name: &'static str, reason: &str) {
        self.results.push(StepResult {
            name,
            status: "blocked",
            kind: "blocked",
            detail: reason.to_string(),
        });
    }

    fn over_budget(&self) -> bool {
        self.results.len() >= STEP_BUDGET || self.started.elapsed() > CAMPAIGN_TIMEOUT
    }

    fn summary(&self, mode: &str, model: &str, variant: Option<&str>) -> Value {
        json!({
            "harness": "live_bounded",
            "mode": mode,
            "model": model,
            "variant": variant,
            "budget": {"step_budget": STEP_BUDGET,
                       "campaign_timeout_seconds": CAMPAIGN_TIMEOUT.as_secs(),
                       "call_timeout_seconds": CALL_TIMEOUT.as_secs()},
            "steps": self.results.iter().map(|result| json!({
                "name": result.name, "status": result.status,
                "kind": result.kind, "detail": result.detail
            })).collect::<Vec<_>>(),
            "counts": {
                "attempted": self.results.len(),
                "passed": self.results.iter().filter(|r| r.status == "passed").count(),
                "failed": self.results.iter().filter(|r| r.status == "failed").count(),
                "blocked": self.results.iter().filter(|r| r.status == "blocked").count(),
                "skipped": self.results.iter().filter(|r| r.status == "skipped").count(),
                "unexecuted": STEP_BUDGET.saturating_sub(self.results.len()),
            }
        })
    }
}

/// Machine-readable blocked report: non-success, never a silent pass.
fn blocked_out(reason: &str) -> ! {
    let report = json!({
        "harness": "live_bounded",
        "status": "blocked",
        "reason": reason,
        "counts": {"attempted": 0, "passed": 0, "failed": 0, "blocked": 1, "skipped": 0},
    });
    println!("{}", serde_json::to_string_pretty(&report).expect("json"));
    if let Ok(path) = std::env::var("OC_LIVE_SUMMARY") {
        let _ = std::fs::write(
            path,
            serde_json::to_string_pretty(&report).unwrap_or_default(),
        );
    }
    panic!("live harness BLOCKED: {reason}");
}

// ---------------------------------------------------------------------------
// Real configuration: model eligibility and limits come from here.
// ---------------------------------------------------------------------------

struct LiveConfig {
    variant: Option<String>,
    provider_id: String,
    model_id: String,
    context: u64,
    output: u64,
    mcp_servers: Vec<String>,
}

impl LiveConfig {
    fn load() -> Result<Self, String> {
        let model = std::env::var("OC_TEST_MODEL").unwrap_or_default();
        if model.trim().is_empty() {
            return Err("OC_TEST_MODEL is not set".to_string());
        }
        let variant = std::env::var("OC_TEST_VARIANT")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let path = config_path()?;
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let value: Value = serde_json::from_str(&raw)
            .map_err(|error| format!("{} is not JSON: {error}", path.display()))?;
        let (provider_id, model_id) = model
            .split_once('/')
            .map(|(provider, model)| (provider.to_string(), model.to_string()))
            .ok_or_else(|| format!("OC_TEST_MODEL must be provider/model, got {model:?}"))?;
        let entry = value
            .pointer(&format!("/provider/{provider_id}/models/{model_id}"))
            .ok_or_else(|| format!("model {model} is not declared in {}", path.display()))?;
        let context = entry
            .pointer("/limit/context")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("model {model} declares no limit.context"))?;
        let output = entry
            .pointer("/limit/output")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("model {model} declares no limit.output"))?;
        let mcp_servers = value
            .get("mcp")
            .and_then(Value::as_object)
            .map(|servers| {
                servers
                    .iter()
                    .filter(|(_, entry)| entry["enabled"] != false)
                    .map(|(name, _)| name.clone())
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            variant,
            provider_id,
            model_id,
            context,
            output,
            mcp_servers,
        })
    }
}

fn config_path() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("OC_TEST_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    for candidate in ["opencode.json", "opencode.jsonc"] {
        let path = Path::new(&home).join(".config/opencode").join(candidate);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("no opencode.json/.jsonc under ~/.config/opencode".to_string())
}

// ---------------------------------------------------------------------------
// Loopback peers (dry run only).
// ---------------------------------------------------------------------------

struct Peer {
    url: String,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_read_timeout(Some(CALL_TIMEOUT)).ok();
                        let Some(body) = read_request(&mut socket) else {
                            continue;
                        };
                        let index = hits.fetch_add(1, Ordering::Relaxed);
                        let reply = dry_run_reply(index, &body);
                        let _ = socket.write_all(
                            format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                                reply.len(),
                                reply
                            )
                            .as_bytes(),
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                    }
                    Err(error) => panic!("peer accept: {error}"),
                }
            }
        });
        Self {
            url,
            stop,
            handle: Some(handle),
        }
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

/// Minimal streamable-HTTP MCP server for the dry run.
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
                        socket.set_read_timeout(Some(CALL_TIMEOUT)).ok();
                        let Some(body) = read_request(&mut socket) else {
                            continue;
                        };
                        let id = body.get("id").cloned().unwrap_or(Value::Null);
                        match body["method"].as_str().unwrap_or_default() {
                            "initialize" => write_mcp(
                                &mut socket,
                                id,
                                json!({"protocolVersion": "2025-11-25",
                                       "capabilities": {"tools": {}},
                                       "serverInfo": {"name": "dry-run", "version": "1"}}),
                            ),
                            "notifications/initialized" => {
                                let _ = socket.write_all(
                                    b"HTTP/1.1 202 Accepted\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                                );
                            }
                            "tools/list" => write_mcp(
                                &mut socket,
                                id,
                                json!({"tools": [{"name": "search",
                                    "description": "dry-run search",
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
                                write_mcp(
                                    &mut socket,
                                    id,
                                    json!({"content": [{"type": "text", "text": "dry-run:search"}],
                                           "isError": false}),
                                );
                            }
                            _ => write_mcp(
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

fn write_mcp(socket: &mut TcpStream, id: Value, result: Value) {
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

fn sse_text(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\ndata: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":5}}}}}}\n\n",
        Value::String(text.to_string())
    )
}

fn sse_calls(calls: &[(&str, &str, Value)]) -> String {
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

/// Deterministic dry-run script, keyed on the prompt and the request state
/// (never on a global counter, so a blocked step cannot shift the script).
fn dry_run_reply(_index: usize, body: &Value) -> String {
    let prompt = last_user_text(body).unwrap_or_default();
    let answered = body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item["type"] == "function_call_output")
                .count()
        })
        .unwrap_or(0);
    if prompt.contains("Fix the `add` function") {
        if answered >= 2 {
            return sse_text("dry-run fix complete");
        }
        return sse_calls(&[
            ("d-patch", "apply_patch", json!({"patchText": PATCH})),
            (
                "d-test",
                "bash",
                json!({"argv": ["cargo", "test", "--quiet"]}),
            ),
        ]);
    }
    if prompt.contains("MCP search") {
        if has_output(body, "d-mcp") {
            return sse_text("dry-run search complete");
        }
        return match mcp_tool_name(body) {
            Some(tool) => sse_calls(&[(
                "d-mcp",
                tool.as_str(),
                json!({"query": "bounded live harness"}),
            )]),
            None => sse_text("no mcp configured"),
        };
    }
    if prompt.contains("Compress the closed") {
        if has_output(body, "d-compress") {
            return sse_text("dry-run compression complete");
        }
        let anchors = dcp_anchors(body).unwrap_or_default();
        let closed = anchors
            .iter()
            .filter(|anchor| anchor["closed"] == true)
            .filter_map(|anchor| anchor["id"].as_str())
            .collect::<Vec<_>>();
        let (start, end) = match (closed.first(), closed.get(1)) {
            (Some(start), Some(end)) => (*start, *end),
            // A single closed anchor is still a valid one-message range.
            (Some(only), None) => (*only, *only),
            _ => return sse_text("nothing to compress"),
        };
        return sse_calls(&[(
            "d-compress",
            "compress",
            json!({"topic": "bounded live harness",
                   "content": [{"startId": start, "endId": end,
                                "summary": "dry-run summary"}]}),
        )]);
    }
    sse_text("dry-run acknowledged")
}

/// True when the request already carries the tool result for `call_id`.
fn has_output(body: &Value, call_id: &str) -> bool {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        })
        .unwrap_or(false)
}

fn last_user_text(body: &Value) -> Option<String> {
    body["input"].as_array()?.iter().rev().find_map(|item| {
        (item["role"] == "user")
            .then(|| item.pointer("/content/0/text").and_then(|v| v.as_str()))
            .flatten()
            .map(str::to_owned)
    })
}

fn texts(body: &Value) -> Vec<String> {
    body["input"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.pointer("/content/0/text")
                        .and_then(Value::as_str)
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

/// Namespaced name of the MCP tool offered in a request, if any.
fn mcp_tool_name(body: &Value) -> Option<String> {
    tool_names(body)
        .into_iter()
        .find(|name| name.contains("__"))
}

fn dcp_anchors(body: &Value) -> Option<Vec<Value>> {
    let text = texts(body)
        .into_iter()
        .find(|text| text.starts_with("DCP context anchors"))?;
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    serde_json::from_str(&text[start..=end]).ok()
}

// ---------------------------------------------------------------------------
// Fixture: isolated HOME + project; dry run adds loopback peers.
// ---------------------------------------------------------------------------

struct Fixture {
    root: tempfile::TempDir,
    peer: Option<Peer>,
    mcp: Option<Mcp>,
    data_dir: PathBuf,
}

impl Fixture {
    /// Live fixture: real HOME config, isolated data root and project.
    fn live() -> Self {
        let root = tempfile::tempdir().expect("root");
        let project = root.path().join("project");
        write_project(&project);
        Self {
            data_dir: root.path().join("data/oc"),
            root,
            peer: None,
            mcp: None,
        }
    }

    /// Dry-run fixture: loopback Responses peer, optional loopback MCP server.
    fn dry_run(with_mcp: bool) -> Self {
        let mcp = with_mcp.then(Mcp::start);
        let peer = Peer::start();
        let root = tempfile::tempdir().expect("root");
        let home = root.path().join("home");
        let config_dir = home.join("config/opencode");
        std::fs::create_dir_all(&config_dir).expect("config dir");
        let mut permissions = json!({
            "read": "allow", "apply_patch": "allow", "bash": "allow",
            "compress": "allow", "webfetch": "allow", "skill": "allow"
        });
        let mcp_section = match &mcp {
            Some(server) => {
                permissions["dry_run_mcp__search"] = json!("allow");
                json!({"dry_run_mcp": {"type": "remote", "url": server.url,
                                       "enabled": true, "oauth": false,
                                       "headers": {"Authorization": "Bearer dry-run"},
                                       "timeout": 3000}})
            }
            None => json!({}),
        };
        std::fs::write(
            config_dir.join("opencode.json"),
            json!({
                "model": "fixture/dry-run-model",
                "permissions": permissions,
                "dcp": {"enabled": true},
                "provider": {"fixture": {
                    "npm": "@ai-sdk/openai",
                    "options": {"baseURL": peer.url, "apiKey": "dry-run-key",
                                "timeout": false, "setCacheKey": false},
                    "models": {"dry-run-model": {"name": "Dry run",
                        "limit": {"context": 128_000, "output": 8_000}}}
                }},
                "mcp": mcp_section
            })
            .to_string(),
        )
        .expect("config");
        let project = root.path().join("project");
        write_project(&project);
        Self {
            data_dir: home.join("data/oc"),
            root,
            peer: Some(peer),
            mcp,
        }
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }

    /// Run one product-binary step in `project`.
    fn run(&self, project: &Path, session: &str, prompt: &str) -> (bool, String) {
        let mut command = Command::new(BIN);
        command.args([
            "run",
            "--data-dir",
            &self.data_dir.to_string_lossy(),
            "--session",
            session,
            prompt,
        ]);
        command.stdin(Stdio::null());
        if self.peer.is_some() {
            let home = self.root.path().join("home");
            let parent: Vec<(String, String)> = std::env::vars().collect();
            command
                .env_clear()
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", home.join("config"))
                .env("XDG_DATA_HOME", home.join("data"))
                .env("XDG_CACHE_HOME", home.join("cache"))
                .env("XDG_STATE_HOME", home.join("state"))
                .env("OC_TEST_ALLOW_LOOPBACK", "1")
                .envs(parent);
        }
        command.current_dir(project);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let started = Instant::now();
        let mut child = command.spawn().expect("product binary");
        let status = loop {
            if let Some(status) = child.try_wait().expect("wait") {
                break status;
            }
            if started.elapsed() > CALL_TIMEOUT {
                let _ = child.kill();
                return (false, format!("watchdog: call exceeded {CALL_TIMEOUT:?}"));
            }
            std::thread::sleep(POLL);
        };
        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut pipe) = child.stdout.take() {
            let _ = pipe.read_to_string(&mut stdout);
        }
        if let Some(mut pipe) = child.stderr.take() {
            let _ = pipe.read_to_string(&mut stderr);
        }
        let detail = format!(
            "stdout={} stderr={}",
            stdout.trim().chars().take(200).collect::<String>(),
            stderr.trim().chars().take(200).collect::<String>()
        );
        (status.success(), detail)
    }

    fn mcp_calls(&self) -> Vec<Value> {
        self.mcp.as_ref().map(Mcp::calls).unwrap_or_default()
    }
}

fn write_project(project: &Path) {
    std::fs::create_dir_all(project.join("src")).expect("src");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname = \"live-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n\n[dependencies]\n",
    )
    .expect("toml");
    std::fs::write(
        project.join("src/lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n",
    )
    .expect("lib");
    std::fs::create_dir_all(project.join("tests")).expect("tests");
    std::fs::write(
        project.join("tests/add.rs"),
        "use live_fixture::add;\n\n#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n",
    )
    .expect("test");
}

// ---------------------------------------------------------------------------
// The bounded campaign (shared by dry run and live).
// ---------------------------------------------------------------------------

fn bounded_campaign(fixture: &Fixture, mcp_declared: bool, mcp_tool: Option<String>) -> Campaign {
    let mut campaign = Campaign::new();
    let project = fixture.project();
    let other = fixture.root.path().join("project-b");
    write_project(&other);
    let session = format!("s-live-{}", std::process::id());
    let _ = mcp_tool;

    // 1. Coding fix through the real tool loop. The prompt carries a large
    // context tail so the later compress step has a measurable gain.
    let coding_prompt = format!(
        "Fix the `add` function in src/lib.rs so its test passes; run `cargo test`. {}",
        "context ".repeat(3_000)
    );
    let (ok, watchdog) = fixture.run(&project, &session, &coding_prompt);
    let tests_ok = Command::new("cargo")
        .args(["test", "--quiet"])
        .current_dir(&project)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    campaign.record(
        "coding-fix",
        ok && tests_ok,
        "model",
        format!("run_ok={ok} tests_ok={tests_ok} {watchdog}"),
    );

    // 2. MCP: compulsory when the configuration declares a server.
    if !mcp_declared {
        campaign.blocked(
            "mcp",
            "no MCP server is declared in the configuration: blocked, never silently skipped",
        );
    } else if campaign.over_budget() {
        campaign.blocked("mcp", "step budget exhausted");
    } else {
        let (ok, watchdog) = fixture.run(
            &project,
            &session,
            "Use the configured MCP search tool for `bounded harness`.",
        );
        let called = !fixture.mcp_calls().is_empty();
        campaign.record(
            "mcp",
            ok && called,
            "model",
            format!("run_ok={ok} mcp_calls={called} {watchdog}"),
        );
    }

    // 3. Model-driven compress.
    if campaign.over_budget() {
        campaign.blocked("compress", "step budget exhausted");
    } else {
        let (ok, watchdog) = fixture.run(
            &project,
            &session,
            "Compress the closed early turns with the compress tool.",
        );
        let blocks = db_blocks(fixture, &session);
        let ops = db_op_states(fixture, &session);
        campaign.record(
            "compress",
            ok && blocks > 0,
            "model",
            format!("run_ok={ok} blocks={blocks} ops={ops} {watchdog}"),
        );
    }

    // 4. Restart: a new process replays the durable session.
    if campaign.over_budget() {
        campaign.blocked("restart", "step budget exhausted");
    } else {
        let (ok, watchdog) = fixture.run(&project, &session, "Report what we did so far.");
        let rows = db_history_len(fixture, &session);
        campaign.record(
            "restart",
            ok && rows >= 4,
            "model",
            format!("run_ok={ok} rows={rows} {watchdog}"),
        );
    }

    // 5. Workspace switch: same data root, another location.
    if campaign.over_budget() {
        campaign.blocked("workspace-switch", "step budget exhausted");
    } else {
        let (ok, watchdog) = fixture.run(&other, &format!("{session}-b"), "Report the workspace.");
        campaign.record(
            "workspace-switch",
            ok,
            "model",
            format!("run_ok={ok} {watchdog}"),
        );
    }

    campaign
}

fn db_blocks(fixture: &Fixture, session: &str) -> usize {
    let Ok(db) = oc_adapters::storage::Db::open(&fixture.data_dir) else {
        return 0;
    };
    oc_adapters::dcp::load_blocks(&db, session)
        .map(|blocks| blocks.len())
        .unwrap_or(0)
}

fn db_op_states(fixture: &Fixture, session: &str) -> String {
    let Ok(db) = oc_adapters::storage::Db::open(&fixture.data_dir) else {
        return "db-unavailable".to_string();
    };
    db.list_tool_ops(session)
        .map(|ops| {
            ops.iter()
                .map(|op| format!("{}={}", op.name, op.state))
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_else(|_| "ops-error".to_string())
}

fn db_history_len(fixture: &Fixture, session: &str) -> usize {
    let Ok(db) = oc_adapters::storage::Db::open(&fixture.data_dir) else {
        return 0;
    };
    db.read_history(session).map(|rows| rows.len()).unwrap_or(0)
}

/// Print one report and append it to `OC_LIVE_SUMMARY` when set.
fn emit_report(report: &Value, label: &str) {
    let text = serde_json::to_string_pretty(report).expect("summary json");
    println!("[{label}]\n{text}");
    if let Ok(path) = std::env::var("OC_LIVE_SUMMARY") {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let entry = format!("[{label}]\n{text}\n");
        let _ = std::fs::write(&path, format!("{existing}{entry}"));
    }
}

fn write_summary(campaign: &Campaign, mode: &str, model: &str, variant: Option<&str>) {
    let report = campaign.summary(mode, model, variant);
    let text = serde_json::to_string_pretty(&report).expect("summary json");
    println!("{text}");
    if let Ok(path) = std::env::var("OC_LIVE_SUMMARY") {
        std::fs::write(path, &text).expect("summary file");
    }
    assert_eq!(
        report["counts"]["failed"], 0,
        "live campaign failures: {text}"
    );
}

/// Offline branch coverage: with and without a declared MCP server.
#[test]
fn live_bounded_dry_run_branches() {
    let with_mcp = Fixture::dry_run(true);
    let campaign = bounded_campaign(&with_mcp, true, None);
    let report = campaign.summary("dry-run", "fixture/dry-run-model", None);
    emit_report(&report, "dry-run");
    assert_eq!(
        report["counts"]["failed"], 0,
        "dry run with MCP failed: {report}"
    );
    assert!(
        report["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .any(|step| step["name"] == "mcp" && step["status"] == "passed"),
        "the MCP branch must execute offline: {report}"
    );

    let without_mcp = Fixture::dry_run(false);
    let campaign = bounded_campaign(&without_mcp, false, None);
    let report = campaign.summary("dry-run-no-mcp", "fixture/dry-run-model", None);
    emit_report(&report, "dry-run-no-mcp");
    assert_eq!(
        report["counts"]["failed"], 0,
        "dry run without MCP failed: {report}"
    );
    let blocked = report["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .filter(|step| step["status"] == "blocked")
        .count();
    assert!(
        blocked == 1,
        "a missing MCP server must be an explicit blocked step: {report}"
    );
}

/// Bounded live campaign. Explicit invocation without credentials is a
/// machine-readable BLOCKED non-success, never a silent pass.
#[test]
#[ignore = "needs live OpenProxy credentials and OC_TEST_MODEL"]
fn live_bounded_campaign() {
    let config = match LiveConfig::load() {
        Ok(config) => config,
        Err(reason) => blocked_out(&reason),
    };
    // Declared limits are asserted, not invented: a campaign step that needs
    // more than the declared output budget would be a harness error.
    assert!(config.context > 0 && config.output > 0);
    let fixture = Fixture::live();
    let campaign = bounded_campaign(&fixture, !config.mcp_servers.is_empty(), None);
    write_summary(
        &campaign,
        "live",
        &format!("{}/{}", config.provider_id, config.model_id),
        config.variant.as_deref(),
    );
}
