//! AUD22-AUD24 regressions through actual `oc` subprocesses.
//!
//! Every fixture has an isolated HOME/XDG/project/data root. Network traffic
//! is limited to loopback fake Responses and streamable-HTTP MCP peers; the
//! same-process ownership check drives two prompts through the real PTY TUI.

#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
#[path = "support/title.rs"]
mod title;
use std::net::{TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const BIN: &str = env!("CARGO_BIN_EXE_oc");
const MODEL: &str = "mcp-application-model";
const TIMEOUT: Duration = Duration::from_secs(12);

/// Startup readiness marker. Iteration 2 of the TUI pixel-parity goal replaced
/// the `oc <status>` history-pane title (upstream has no transcript title,
/// `routes/session/index.tsx:1273-1300`); the always-visible tab-strip title is
/// the upstream fallback for a session without a title
/// (`component/session-tabs.tsx:1561`).
const READY: &str = "Untitled session";
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
struct HttpRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Value,
}

fn read_http(mut socket: TcpStream) -> Option<(TcpStream, HttpRequest)> {
    socket.set_read_timeout(Some(IO_TIMEOUT)).ok()?;
    socket.set_write_timeout(Some(IO_TIMEOUT)).ok()?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let count = socket.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > 64 * 1024 {
            return None;
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = std::str::from_utf8(&bytes[..header_end]).ok()?;
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_string();
    let path = request_line.next()?.to_string();
    let mut headers = HashMap::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':')?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > 2 * 1024 * 1024 {
        return None;
    }
    while bytes.len() < header_end + length {
        let count = socket.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + length]).ok()?
    };
    Some((
        socket,
        HttpRequest {
            method,
            path,
            headers,
            body,
        },
    ))
}

fn write_http(socket: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    write!(
        socket,
        "HTTP/1.1 {status} fixture\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("HTTP response headers");
    socket.write_all(body).expect("HTTP response body");
    socket.flush().expect("HTTP response flush");
}

fn write_json(socket: &mut TcpStream, id: Value, result: Value) {
    let body = serde_json::to_vec(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    }))
    .expect("JSON response");
    write_http(socket, 200, "application/json", &body);
}

#[derive(Debug, Clone)]
struct McpRecord {
    http_method: String,
    path: String,
    authorization: Option<String>,
    protocol_version: Option<String>,
    rpc_method: String,
    arguments: Option<Value>,
}

struct FakeMcp {
    url: String,
    records: Arc<Mutex<Vec<McpRecord>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeMcp {
    fn stalled_initialize() -> (Self, Arc<AtomicBool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/strict/v1/mcp", listener.local_addr().unwrap());
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = records.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let closed = Arc::new(AtomicBool::new(false));
        let observed_close = closed.clone();
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                        continue;
                    }
                    Err(error) => panic!("accept: {error}"),
                };
                let Some((mut socket, request)) = read_http(socket) else {
                    continue;
                };
                captured.lock().unwrap().push(McpRecord {
                    http_method: request.method,
                    path: request.path,
                    authorization: request.headers.get("authorization").cloned(),
                    protocol_version: None,
                    rpc_method: request.body["method"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    arguments: None,
                });
                socket
                    .set_read_timeout(Some(Duration::from_millis(50)))
                    .unwrap();
                // Never release initialize. Observe actual TCP closure on cancel.
                while !stopping.load(Ordering::Relaxed) {
                    match socket.read(&mut [0u8; 1]) {
                        Ok(0) => {
                            observed_close.store(true, Ordering::Relaxed);
                            break;
                        }
                        Err(error)
                            if matches!(
                                error.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => {
                            observed_close.store(true, Ordering::Relaxed);
                            break;
                        }
                        Ok(_) => panic!("unexpected request data after initialize"),
                    }
                }
            }
        });
        (
            Self {
                url,
                records,
                stop,
                thread: Some(thread),
            },
            closed,
        )
    }

    fn start(label: &str, bearer: &str, tools: &[&str]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake MCP listener");
        listener.set_nonblocking(true).expect("nonblocking MCP");
        let address = listener.local_addr().expect("MCP address");
        let records = Arc::new(Mutex::new(Vec::new()));
        let captured = records.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let label = label.to_string();
        let bearer = bearer.to_string();
        let tools = tools
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                        continue;
                    }
                    Err(error) => panic!("fake MCP accept: {error}"),
                };
                let Some((mut socket, request)) = read_http(socket) else {
                    continue;
                };
                let rpc_method = request.body["method"].as_str().unwrap_or("").to_string();
                let tool = request.body["params"]["name"].as_str().map(str::to_string);
                let authorization = request.headers.get("authorization").cloned();
                let protocol_version = request.headers.get("mcp-protocol-version").cloned();
                captured.lock().expect("MCP records").push(McpRecord {
                    http_method: request.method.clone(),
                    path: request.path.clone(),
                    authorization: authorization.clone(),
                    protocol_version,
                    rpc_method: rpc_method.clone(),
                    arguments: request.body.pointer("/params/arguments").cloned(),
                });
                if request.method != "POST"
                    || request.path != "/strict/v1/mcp"
                    || authorization.as_deref() != (!bearer.is_empty()).then_some(bearer.as_str())
                {
                    write_http(
                        &mut socket,
                        401,
                        "text/plain",
                        b"PRIVATE_RESPONSE_BODY_SENTINEL",
                    );
                    continue;
                }
                let id = request.body.get("id").cloned().unwrap_or(Value::Null);
                match rpc_method.as_str() {
                    "initialize" => write_json(
                        &mut socket,
                        id,
                        json!({
                            "protocolVersion": "2025-11-25",
                            "capabilities": {"tools": {}},
                            "serverInfo": {"name": label, "version": "test"},
                        }),
                    ),
                    "notifications/initialized" => {
                        write_http(&mut socket, 202, "application/json", b"")
                    }
                    "tools/list" => write_json(
                        &mut socket,
                        id,
                        json!({"tools": tools.iter().map(|name| json!({
                            "name": name,
                            "description": format!("{label} {name}"),
                            "inputSchema": if name == "search" { json!({
                                "type": "object",
                                "properties": {
                                    "query": {"type": "string"},
                                    "response_length": {"type": "string"},
                                },
                                "required": ["query"],
                                "additionalProperties": false,
                            }) } else { json!({"type": "object", "properties": {}}) },
                        })).collect::<Vec<_>>() }),
                    ),
                    "tools/call" => {
                        let name = tool.as_deref().unwrap_or("");
                        let result = match name {
                            "is_error" => json!({
                                "content": [{"type": "text", "text": "server-declared failure"}],
                                "isError": true,
                            }),
                            "image" => json!({
                                "content": [{
                                    "type": "image",
                                    "data": "aGVsbG8=",
                                    "mimeType": "image/png",
                                }],
                                "isError": false,
                            }),
                            _ => json!({
                                "content": [{
                                    "type": "text",
                                    "text": format!("{label}:{name}"),
                                }],
                                "isError": false,
                            }),
                        };
                        write_json(&mut socket, id, result);
                    }
                    _ => {
                        let body = serde_json::to_vec(&json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {"code": -32601, "message": "unknown"},
                        }))
                        .expect("MCP error JSON");
                        write_http(&mut socket, 200, "application/json", &body);
                    }
                }
            }
        });
        Self {
            url: format!("http://{address}/strict/v1/mcp"),
            records,
            stop,
            thread: Some(thread),
        }
    }

    fn records(&self) -> Vec<McpRecord> {
        self.records.lock().expect("MCP records").clone()
    }
}

impl Drop for FakeMcp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.expect("fake MCP server");
            }
        }
    }
}

#[derive(Clone)]
enum ResponsesScript {
    TextByPrompt,
    ToolBatch {
        calls: Vec<(String, String, Value)>,
        final_text: String,
    },
}

struct FakeResponses {
    base_url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl FakeResponses {
    fn start(script: ResponsesScript) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake Responses listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking Responses");
        let address = listener.local_addr().expect("Responses address");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                let (socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL);
                        continue;
                    }
                    Err(error) => panic!("fake Responses accept: {error}"),
                };
                let Some((mut socket, request)) = read_http(socket) else {
                    continue;
                };
                if request.method != "POST" || request.path != "/proxy/v1/responses" {
                    write_http(&mut socket, 404, "text/plain", b"");
                    continue;
                }
                let index = {
                    let mut guard = captured.lock().expect("Responses requests");
                    let index = guard.len();
                    guard.push(request.body.clone());
                    index
                };
                match &script {
                    _ if title::respond(&mut socket, &request.body) => {}
                    ResponsesScript::TextByPrompt => {
                        let prompt = last_user_text(&request.body).unwrap_or_default();
                        respond_text(&mut socket, &format!("answer:{prompt}"));
                    }
                    ResponsesScript::ToolBatch { calls, final_text } if index == 0 => {
                        respond_tools(&mut socket, calls);
                    }
                    ResponsesScript::ToolBatch { final_text, .. } => {
                        respond_text(&mut socket, final_text);
                    }
                }
            }
        });
        Self {
            base_url: format!("http://{address}/proxy/v1"),
            requests,
            stop,
            thread: Some(thread),
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.requests.lock().expect("Responses requests").clone()
    }

    fn wait_requests(&self, count: usize) -> Vec<Value> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let requests = self.requests();
            if requests.len() >= count {
                return requests;
            }
            assert!(
                Instant::now() < deadline,
                "missing Responses request {count}"
            );
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for FakeResponses {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let result = thread.join();
            if !std::thread::panicking() {
                result.expect("fake Responses server");
            }
        }
    }
}

fn respond_tools(socket: &mut TcpStream, calls: &[(String, String, Value)]) {
    let output = calls
        .iter()
        .map(|(item_id, call_id, arguments)| {
            json!({
                "type": "function_call",
                "id": item_id,
                "call_id": call_id,
                "name": arguments["__wireName"],
                "arguments": arguments["arguments"].to_string(),
                "status": "completed",
            })
        })
        .collect::<Vec<_>>();
    let mut events = Vec::new();
    for item in &output {
        events.push(json!({
            "type": "response.output_item.added",
            "item": {
                "type": "function_call",
                "id": item["id"],
                "call_id": item["call_id"],
                "name": item["name"],
                "arguments": "",
                "status": "in_progress",
            },
        }));
        events.push(json!({
            "type": "response.function_call_arguments.delta",
            "item_id": item["id"],
            "delta": item["arguments"],
        }));
    }
    events.push(json!({
        "type": "response.completed",
        "response": {"status": "completed", "output": output},
    }));
    respond_events(socket, &events);
}

fn respond_text(socket: &mut TcpStream, text: &str) {
    respond_events(
        socket,
        &[
            json!({"type": "response.output_text.delta", "delta": text}),
            json!({
                "type": "response.completed",
                "response": {
                    "status": "completed",
                    "output": [{
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "output_text", "text": text}],
                    }],
                },
            }),
        ],
    );
}

fn respond_events(socket: &mut TcpStream, events: &[Value]) {
    let body = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    write_http(socket, 200, "text/event-stream", body.as_bytes());
}

fn last_user_text(request: &Value) -> Option<String> {
    request["input"]
        .as_array()?
        .iter()
        .rev()
        .find(|item| item["type"] == "message" && item["role"] == "user")?["content"]
        .as_array()?
        .iter()
        .find_map(|part| part["text"].as_str().map(str::to_string))
}

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated MCP application fixture");
        let home = root.path().join("home");
        let project = root.path().join("project");
        fs::create_dir_all(home.join("config/opencode")).expect("config directory");
        fs::create_dir_all(&project).expect("project directory");
        Self {
            _root: root,
            home,
            project,
        }
    }

    fn write_config(&self, provider: &FakeResponses, mcp: Value, permissions: Value) {
        let config = json!({
            "model": format!("fixture/{MODEL}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": provider.base_url, "apiKey": "responses-key"},
                "models": {MODEL: {
                    "name": "MCP application fixture",
                    "limit": {"context": 65536, "output": 4096},
                }},
            }},
            "permissions": permissions,
            "mcp": mcp,
        });
        fs::write(
            self.home.join("config/opencode/opencode.json"),
            config.to_string(),
        )
        .expect("fixture config");
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(&self.project);
        command
    }

    fn spawn_run(&self, label: &str) -> Process {
        let stdout = self.home.join(format!("{label}.stdout"));
        let stderr = self.home.join(format!("{label}.stderr"));
        let child = self
            .command()
            .args(["run", "--session", label, "exercise configured MCP"])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).expect("stdout file"))
            .stderr(fs::File::create(&stderr).expect("stderr file"))
            .spawn()
            .expect("actual oc binary");
        Process {
            child,
            stdout,
            stderr,
        }
    }
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Process {
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll actual binary") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "actual binary timeout: {}",
                self.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }

    fn diagnostics(&self) -> String {
        fs::read_to_string(&self.stderr).unwrap_or_default()
    }

    fn output(&self) -> String {
        fs::read_to_string(&self.stdout).unwrap_or_default()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let deadline = Instant::now() + TIMEOUT;
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

fn remote_entry(server: &FakeMcp, header_name: &str, bearer: &str) -> Value {
    json!({
        "type": "remote",
        "url": server.url,
        "enabled": true,
        "oauth": false,
        "headers": {header_name: bearer},
        "timeout": 3000,
    })
}

#[test]
fn aud22_binary_accepts_user_authorization_spelling_at_strict_mcp() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::start("strict-auth", "Bearer exact-user-token", &["search"]);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"codex_web": remote_entry(
            &mcp,
            "Authorization",
            "Bearer exact-user-token",
        )}),
        json!({}),
    );

    let mut process = fixture.spawn_run("aud22-authorization");
    let status = process.wait();
    assert!(
        status.success(),
        "Authorization config must reach strict MCP; stderr={}",
        process.diagnostics()
    );
    assert_eq!(process.output().trim(), "answer:exercise configured MCP");
    let records = mcp.records();
    assert_eq!(
        records
            .iter()
            .filter(|record| record.rpc_method == "initialize")
            .count(),
        1,
        "one strict initialize: {records:?}"
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record.rpc_method == "tools/list")
            .count(),
        1,
        "one strict list: {records:?}"
    );
    assert!(records.iter().all(|record| {
        record.http_method == "POST"
            && record.path == "/strict/v1/mcp"
            && record.authorization.as_deref() == Some("Bearer exact-user-token")
    }));
    assert!(
        records
            .iter()
            .filter(|record| record.rpc_method != "initialize")
            .all(|record| record.protocol_version.as_deref() == Some("2025-11-25"))
    );
    assert_eq!(responses.requests().len(), 2, "one generation plus title");
    assert_eq!(
        responses
            .requests()
            .iter()
            .filter(|r| title::is_title(r))
            .count(),
        1
    );
}

#[test]
fn aud22_binary_rejects_conflicting_authorization_duplicates_before_network() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::start("must-not-connect", "Bearer lower", &["search"]);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"codex_web": {
            "type": "remote",
            "url": mcp.url,
            "headers": {
                "Authorization": "Bearer upper",
                "authorization": "Bearer lower",
            },
            "enabled": true,
            "oauth": false,
        }}),
        json!({}),
    );

    let mut process = fixture.spawn_run("aud22-conflicting-auth");
    let status = process.wait();
    assert!(
        !status.success(),
        "conflicting credentials must be rejected"
    );
    let diagnostic = process.diagnostics().to_ascii_lowercase();
    assert!(
        diagnostic.contains("authorization") && diagnostic.contains("conflict"),
        "actionable duplicate diagnostic: {diagnostic}"
    );
    assert!(mcp.records().is_empty(), "conflict reached MCP network");
    assert!(
        responses.requests().is_empty(),
        "conflict reached provider network"
    );
}

#[test]
fn aud24_binary_rejects_oversized_catalog_without_partial_provider_tools() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let names = (0..65)
        .map(|index| format!("tool_{index}"))
        .collect::<Vec<_>>();
    let refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    let mcp = FakeMcp::start("oversized", "Bearer catalog-token", &refs);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"oversized": remote_entry(
            &mcp,
            "Authorization",
            "Bearer catalog-token",
        )}),
        json!({}),
    );

    let mut process = fixture.spawn_run("aud24-catalog-limit");
    assert!(!process.wait().success(), "partial catalog was accepted");
    let diagnostic = process.diagnostics().to_ascii_lowercase();
    assert!(
        diagnostic.contains("catalog") && diagnostic.contains("limit"),
        "visible catalog diagnostic: {diagnostic}"
    );
    assert!(
        responses.requests().is_empty(),
        "partial catalog reached model"
    );
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|record| record.rpc_method == "tools/list")
            .count(),
        1
    );
}

#[test]
fn aud24_binary_search_uses_the_advertised_response_length_schema() {
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls: vec![(
            "item-search".into(),
            "call-search".into(),
            json!({
                "__wireName": "codex_web__search",
                "arguments": {"query": "rust mcp", "response_length": "short"},
            }),
        )],
        final_text: "search observed".into(),
    });
    let mcp = FakeMcp::start("search", "Bearer search-token", &["search"]);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"codex_web": remote_entry(
            &mcp,
            "Authorization",
            "Bearer search-token",
        )}),
        json!({"codex_web__search": "allow"}),
    );

    let mut process = fixture.spawn_run("aud24-search-schema");
    assert!(process.wait().success(), "{}", process.diagnostics());
    assert_eq!(process.output().trim(), "search observed");
    let calls = mcp
        .records()
        .into_iter()
        .filter(|record| record.rpc_method == "tools/call")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].arguments,
        Some(json!({"query": "rust mcp", "response_length": "short"}))
    );
    assert!(
        !calls[0]
            .arguments
            .as_ref()
            .unwrap()
            .to_string()
            .contains("limit")
    );
}

#[test]
fn aud24_binary_routes_collision_names_to_exact_original_server_and_tool() {
    let calls = vec![
        (
            "item-a".to_string(),
            "call-a".to_string(),
            json!({"__wireName": "a__b__c", "arguments": {}}),
        ),
        (
            "item-b".to_string(),
            "call-b".to_string(),
            json!({"__wireName": "a__b__c__2", "arguments": {}}),
        ),
    ];
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls,
        final_text: "MCP routing observed".to_string(),
    });
    let fixture = Fixture::new();
    let bin_dir = fixture.home.join("routing-bin");
    fs::create_dir_all(&bin_dir).expect("routing fixture bin");
    let server_a = bin_dir.join("server-a");
    let server_a_log = fixture.home.join("server-a.log");
    write_routing_stdio_server(&server_a, &server_a_log, "server-a", &["b__c"]);
    let server_a_b = bin_dir.join("server-a-b");
    let server_a_b_log = fixture.home.join("server-a-b.log");
    write_routing_stdio_server(&server_a_b, &server_a_b_log, "server-a-b", &["c"]);
    fixture.write_config(
        &responses,
        json!({
            "a": {
                "type": "local",
                "command": [server_a.to_string_lossy()],
                "enabled": true,
                "timeout": 3000,
            },
            "a__b": {
                "type": "local",
                "command": [server_a_b.to_string_lossy()],
                "enabled": true,
                "timeout": 3000,
            },
        }),
        json!({
            "a__b__c": "allow",
            "a__b__c__2": "allow",
        }),
    );

    let mut process = fixture.spawn_run("aud24-routing");
    let status = process.wait();
    assert!(status.success(), "{}", process.diagnostics());
    assert_eq!(process.output().trim(), "MCP routing observed");
    let requests = responses.requests();
    assert_eq!(requests.len(), 3, "tool round, final round, title");
    assert!(title::is_title(&requests[2]));
    assert_eq!(function_output(&requests[1], "call-a"), "server-a:b__c");
    assert_eq!(function_output(&requests[1], "call-b"), "server-a-b:c");
    let advertised = function_tool_names(&requests[0]);
    for expected in ["a__b__c", "a__b__c__2"] {
        assert_eq!(
            advertised
                .iter()
                .filter(|name| name.as_str() == expected)
                .count(),
            1,
            "one collision-safe advertised mapping for {expected}: {advertised:?}"
        );
    }
    assert_eq!(logged_calls(&server_a_log), ["b__c"]);
    assert_eq!(logged_calls(&server_a_b_log), ["c"]);
}

#[test]
fn aud24_binary_surfaces_is_error_and_unsupported_result_modality() {
    let calls = vec![
        (
            "item-error".to_string(),
            "call-error".to_string(),
            json!({"__wireName": "results__is_error", "arguments": {}}),
        ),
        (
            "item-image".to_string(),
            "call-image".to_string(),
            json!({"__wireName": "results__image", "arguments": {}}),
        ),
    ];
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls,
        final_text: "MCP failures observed".to_string(),
    });
    let fixture = Fixture::new();
    let bin_dir = fixture.home.join("result-bin");
    fs::create_dir_all(&bin_dir).expect("result fixture bin");
    let server = bin_dir.join("result-server");
    let server_log = fixture.home.join("result-server.log");
    write_routing_stdio_server(
        &server,
        &server_log,
        "result-server",
        &["is_error", "image"],
    );
    fixture.write_config(
        &responses,
        json!({"results": {
            "type": "local",
            "command": [server.to_string_lossy()],
            "enabled": true,
            "timeout": 3000,
        }}),
        json!({
            "results__is_error": "allow",
            "results__image": "allow",
        }),
    );

    let mut process = fixture.spawn_run("aud24-result-semantics");
    let status = process.wait();
    assert!(status.success(), "{}", process.diagnostics());
    assert_eq!(process.output().trim(), "MCP failures observed");
    let requests = responses.requests();
    assert_eq!(requests.len(), 3, "tool round, final round, title");
    assert!(title::is_title(&requests[2]));
    let declared = function_output(&requests[1], "call-error");
    let unsupported = function_output(&requests[1], "call-image");
    assert!(
        declared.starts_with("error:"),
        "isError was reported as success: {declared:?}"
    );
    assert!(
        unsupported.starts_with("error:"),
        "unsupported image result was reported as success: {unsupported:?}"
    );
    assert_ne!(
        declared, unsupported,
        "server isError and unsupported modality must remain distinguishable"
    );
    assert_eq!(logged_calls(&server_log), ["is_error", "image"]);
}

fn function_tool_names(request: &Value) -> Vec<String> {
    request["tools"]
        .as_array()
        .expect("Responses tools")
        .iter()
        .filter(|tool| tool["type"] == "function")
        .filter_map(|tool| tool["name"].as_str().map(str::to_string))
        .collect()
}

fn function_output(request: &Value, call_id: &str) -> String {
    request["input"]
        .as_array()
        .expect("Responses input")
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .and_then(|item| item["output"].as_str())
        .unwrap_or_else(|| panic!("missing output {call_id}: {request}"))
        .to_string()
}

fn logged_calls(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .expect("routing MCP log")
        .lines()
        .filter_map(|line| line.strip_prefix("call ").map(str::to_string))
        .collect()
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).expect("write executable fixture");
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("fixture permissions");
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

fn write_stdio_server(path: &Path, log: &Path) {
    let log = shell_quote(log);
    write_executable(
        path,
        &format!(
            r#"#!/bin/sh
set -u
printf 'spawn %s\n' "$$" >> {log}
while IFS= read -r line; do
    id=${{line#*\"id\":}}
    id=${{id%%,*}}
    case "$line" in
        *'"method":"initialize"'*)
            printf 'initialize\n' >> {log}
            printf '{{"jsonrpc":"2.0","id":%s,"result":{{"protocolVersion":"2025-11-25","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"counted-stdio","version":"test"}}}}}}\n' "$id"
            ;;
        *'"method":"notifications/initialized"'*)
            printf 'initialized\n' >> {log}
            ;;
        *'"method":"tools/list"'*)
            printf 'list\n' >> {log}
            printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[{{"name":"ping","description":"counted tool","inputSchema":{{"type":"object","properties":{{}}}}}}]}}}}\n' "$id"
            ;;
        *'"method":"tools/call"'*)
            printf 'call\n' >> {log}
            printf '{{"jsonrpc":"2.0","id":%s,"result":{{"content":[{{"type":"text","text":"pong"}}],"isError":false}}}}\n' "$id"
            ;;
    esac
done
"#
        ),
    );
}

fn write_routing_stdio_server(path: &Path, log: &Path, label: &str, tools: &[&str]) {
    let log = shell_quote(log);
    let tools = tools
        .iter()
        .map(|name| {
            format!(
                r#"{{"name":"{name}","description":"{label} {name}","inputSchema":{{"type":"object","properties":{{}}}}}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    write_executable(
        path,
        &format!(
            r#"#!/bin/sh
set -u
printf 'spawn %s\n' "$$" >> {log}
while IFS= read -r line; do
    id=${{line#*\"id\":}}
    id=${{id%%,*}}
    case "$line" in
        *'"method":"initialize"'*)
            printf '{{"jsonrpc":"2.0","id":%s,"result":{{"protocolVersion":"2025-11-25","capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"{label}","version":"test"}}}}}}\n' "$id"
            ;;
        *'"method":"notifications/initialized"'*)
            ;;
        *'"method":"tools/list"'*)
            printf '{{"jsonrpc":"2.0","id":%s,"result":{{"tools":[{tools}]}}}}\n' "$id"
            ;;
        *'"method":"tools/call"'*)
            case "$line" in
                *'"name":"is_error"'*)
                    name='is_error'
                    result='{{"content":[{{"type":"text","text":"server-declared failure"}}],"isError":true}}'
                    ;;
                *'"name":"image"'*)
                    name='image'
                    result='{{"content":[{{"type":"image","data":"aGVsbG8=","mimeType":"image/png"}}],"isError":false}}'
                    ;;
                *'"name":"b__c"'*)
                    name='b__c'
                    result='{{"content":[{{"type":"text","text":"{label}:b__c"}}],"isError":false}}'
                    ;;
                *'"name":"c"'*)
                    name='c'
                    result='{{"content":[{{"type":"text","text":"{label}:c"}}],"isError":false}}'
                    ;;
                *)
                    name='unknown'
                    result='{{"content":[{{"type":"text","text":"unknown"}}],"isError":true}}'
                    ;;
            esac
            printf 'call %s\n' "$name" >> {log}
            printf '{{"jsonrpc":"2.0","id":%s,"result":%s}}\n' "$id" "$result"
            ;;
    esac
done
"#
        ),
    );
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
    // SAFETY: valid out-pointers and a live winsize; success returns two owned fds.
    let result = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            &winsize,
        )
    };
    assert_eq!(result, 0, "openpty");
    // SAFETY: openpty succeeded and ownership of master transfers to this wrapper.
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    // SAFETY: openpty succeeded and ownership of slave transfers to this wrapper.
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    (master, slave)
}

fn dup_fd(fd: &OwnedFd) -> OwnedFd {
    // SAFETY: fd is open; F_DUPFD_CLOEXEC creates a separately owned fd.
    let duplicated = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
    assert!(duplicated >= 0, "dup PTY fd");
    // SAFETY: fcntl returned a new owned descriptor.
    unsafe { OwnedFd::from_raw_fd(duplicated) }
}

struct PtyProcess {
    master: fs::File,
    child: Child,
    output: Arc<Mutex<Vec<u8>>>,
}

impl PtyProcess {
    fn screen(&self) -> Vec<String> {
        // Text-only reconstruction for ASCII fixture assertions. Unlike raw
        // substring matching, this accounts for ratatui's unchanged-cell skips.
        let bytes = self.output.lock().unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let mut chars = text.chars().peekable();
        let mut cells = vec![vec![' '; 160]; 50];
        let (mut row, mut col) = (0usize, 0usize);
        while let Some(ch) = chars.next() {
            match ch {
                '\x1b' if chars.peek() == Some(&'[') => {
                    chars.next();
                    let mut args = String::new();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            let values = args
                                .split(';')
                                .map(|s| s.parse::<usize>().unwrap_or(0))
                                .collect::<Vec<_>>();
                            let first = values[0];
                            match c {
                                'H' | 'f' => {
                                    row = first.saturating_sub(1);
                                    col = values.get(1).copied().unwrap_or(1).saturating_sub(1);
                                }
                                'G' => col = first.saturating_sub(1),
                                'C' => col += first.max(1),
                                'D' => col = col.saturating_sub(first.max(1)),
                                'A' => row = row.saturating_sub(first.max(1)),
                                'B' => row += first.max(1),
                                'J' if first == 2 => cells.iter_mut().for_each(|r| r.fill(' ')),
                                'K' if row < cells.len() => {
                                    let start = if first == 2 { 0 } else { col.min(160) };
                                    cells[row][start..].fill(' ');
                                }
                                _ => {}
                            }
                            break;
                        }
                        args.push(c);
                    }
                }
                '\r' => col = 0,
                '\n' => row += 1,
                c if !c.is_control() => {
                    if row < cells.len() && col < cells[row].len() {
                        cells[row][col] = c;
                    }
                    col += 1;
                }
                _ => {}
            }
        }
        cells.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    fn wait_screen(&self, needle: &str, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let screen = self.screen();
            if screen.iter().any(|row| row.contains(needle)) {
                return;
            }
            assert!(Instant::now() < deadline, "missing {needle:?}: {screen:?}");
            std::thread::sleep(POLL);
        }
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("raw PTY input");
        self.master.flush().expect("raw PTY flush");
    }

    fn resize(&self, cols: u16, rows: u16) {
        let size = libc::winsize {
            ws_col: cols,
            ws_row: rows,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        assert_eq!(
            // SAFETY: the master descriptor and winsize are live.
            unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) },
            0
        );
    }

    fn spawn(fixture: &Fixture, session: &str) -> Self {
        let (master, slave) = openpty_pair(100, 28);
        let mut command = fixture.command();
        command
            .args(["tui", "--session", session])
            .env("TERM", "xterm-256color")
            .stdin(Stdio::from(dup_fd(&slave)))
            .stdout(Stdio::from(dup_fd(&slave)))
            .stderr(Stdio::from(dup_fd(&slave)));
        let child = command.spawn().expect("actual oc TUI");
        drop(slave);
        let master: fs::File = master.into();
        // SAFETY: master is open; fcntl creates a dedicated reader descriptor.
        let reader_fd = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0) };
        assert!(reader_fd >= 0, "dup PTY reader");
        // SAFETY: reader_fd was just created and transfers to File.
        let mut reader = unsafe { fs::File::from_raw_fd(reader_fd) };
        let output = Arc::new(Mutex::new(Vec::new()));
        let captured = output.clone();
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => captured
                        .lock()
                        .expect("PTY output")
                        .extend_from_slice(&chunk[..count]),
                }
            }
        });
        Self {
            master,
            child,
            output,
        }
    }

    fn send_line(&mut self, text: &str) -> usize {
        let offset = self.output.lock().expect("PTY output").len();
        self.master.write_all(text.as_bytes()).expect("PTY input");
        self.master.write_all(b"\r").expect("PTY enter");
        self.master.flush().expect("PTY flush");
        offset
    }

    fn wait_visible(&self, needle: &str) {
        self.wait_visible_after(0, needle);
    }

    fn wait_visible_after(&self, offset: usize, needle: &str) {
        let wanted = normalize(needle.as_bytes());
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let bytes = self.output.lock().expect("PTY output").clone();
            if bytes.len() >= offset && contains(&normalize(&bytes[offset..]), &wanted) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "PTY did not render {needle:?}; tail={:?}",
                String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(1500)..])
            );
            std::thread::sleep(POLL);
        }
    }

    fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("poll TUI") {
                return status;
            }
            assert!(Instant::now() < deadline, "TUI exit timeout");
            std::thread::sleep(POLL);
        }
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn normalize(bytes: &[u8]) -> Vec<u8> {
    let mut visible = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index += 2;
            while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                index += 1;
            }
            index = index.saturating_add(1);
        } else {
            if !bytes[index].is_ascii_whitespace() {
                visible.push(bytes[index]);
            }
            index += 1;
        }
    }
    visible
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack.len() >= needle.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[test]
fn aud23_tui_two_turns_own_one_stdio_child_and_disabled_entry_zero_spawns() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let fixture = Fixture::new();
    let bin_dir = fixture.home.join("fixture-bin");
    fs::create_dir_all(&bin_dir).expect("fixture bin directory");
    let stdio = bin_dir.join("counted-mcp");
    let stdio_log = fixture.home.join("stdio-mcp.log");
    write_stdio_server(&stdio, &stdio_log);
    let trap = bin_dir.join("disabled-mcp-trap");
    let trap_log = fixture.home.join("disabled-mcp-spawned.log");
    write_executable(
        &trap,
        &format!(
            "#!/bin/sh\nprintf 'spawned\\n' >> {}\nexit 97\n",
            shell_quote(&trap_log)
        ),
    );
    fixture.write_config(
        &responses,
        json!({
            "counted": {
                "type": "local",
                "command": [stdio.to_string_lossy()],
                "enabled": true,
                "timeout": 3000,
            },
            "chrome-devtools": {
                "type": "local",
                "command": [trap.to_string_lossy(), "-y", "chrome-devtools-mcp@latest"],
                "enabled": false,
            },
        }),
        json!({}),
    );

    let mut tui = PtyProcess::spawn(&fixture, "aud23-two-turns");
    tui.wait_visible(READY);
    let first = tui.send_line("first MCP ownership turn");
    tui.wait_visible_after(first, "answer:first MCP ownership turn");
    // A text delta is rendered before the worker publishes TurnFinished.
    // Bound the handoff without matching a stale `Idle` cell from ratatui.
    std::thread::sleep(Duration::from_millis(750));
    let second = tui.send_line("second MCP ownership turn");
    tui.wait_visible_after(second, "answer:second MCP ownership turn");
    std::thread::sleep(Duration::from_millis(750));
    tui.send_line("/quit");
    assert!(tui.wait_exit().success(), "clean TUI exit");

    let requests = responses.wait_requests(2);
    assert_eq!(requests.len(), 3, "two provider turns plus one title");
    assert_eq!(requests.iter().filter(|r| title::is_title(r)).count(), 1);
    for request in requests.into_iter().filter(|r| !title::is_title(r)) {
        assert_eq!(
            function_tool_names(&request)
                .iter()
                .filter(|name| name.as_str() == "counted__ping")
                .count(),
            1,
            "same generation catalog on both turns"
        );
    }
    let lifecycle = fs::read_to_string(&stdio_log).expect("stdio lifecycle log");
    assert!(
        !trap_log.exists(),
        "disabled local MCP executed: {}",
        fs::read_to_string(&trap_log).unwrap_or_default()
    );
    for pid in lifecycle.lines().filter_map(|line| {
        line.strip_prefix("spawn ")
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
    }) {
        // SAFETY: signal 0 only probes a child pid recorded by this fixture.
        let probe = unsafe { libc::kill(pid, 0) };
        assert_ne!(probe, 0, "MCP child {pid} survived shutdown");
    }
    assert_eq!(
        lifecycle
            .lines()
            .filter(|line| line.starts_with("spawn "))
            .count(),
        1,
        "one generation-owned child, not one child per turn: {lifecycle}"
    );
    assert_eq!(
        lifecycle
            .lines()
            .filter(|line| *line == "initialize")
            .count(),
        1,
        "one MCP handshake: {lifecycle}"
    );
    assert_eq!(
        lifecycle.lines().filter(|line| *line == "list").count(),
        1,
        "one tools/list for the generation: {lifecycle}"
    );

    // A new application generation with the same entry disabled performs no
    // launch/probe and still runs normally after the prior owner shut down.
    fixture.write_config(
        &responses,
        json!({
            "counted": {
                "type": "local",
                "command": [stdio.to_string_lossy()],
                "enabled": false,
            },
            "chrome-devtools": {
                "type": "local",
                "command": [trap.to_string_lossy(), "-y", "chrome-devtools-mcp@latest"],
                "enabled": false,
            },
        }),
        json!({}),
    );
    let mut disabled = fixture.spawn_run("aud23-disabled-restart");
    assert!(disabled.wait().success(), "{}", disabled.diagnostics());
    assert_eq!(disabled.output().trim(), "answer:exercise configured MCP");
    let requests = responses.wait_requests(5);
    assert_eq!(
        requests.len(),
        5,
        "three main turns plus titles for two sessions"
    );
    assert_eq!(requests.iter().filter(|r| title::is_title(r)).count(), 2);
    assert_eq!(
        fs::read_to_string(&stdio_log)
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("spawn "))
            .count(),
        1,
        "disabled restarted generation spawned a child"
    );
    assert!(!trap_log.exists(), "disabled browser trap executed");
}

#[test]
fn v01_pending_initialize_raw_pty_cancel_edit_duplicate_and_retry() {
    pending_initialize_raw_pty(false);
}

#[test]
fn v01_manual_compress_raw_pty_cancel_shutdown_and_retry() {
    pending_initialize_raw_pty(true);
}

fn pending_initialize_raw_pty(manual_compress: bool) {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let fixture = Fixture::new();
    let server = fixture.home.join("stalled-mcp");
    let log = fixture.home.join("stalled.log");
    let _cleanup_on_assertion_failure = FixtureChildren(log.clone());
    let release = fixture.home.join("release");
    write_executable(
        &server,
        &format!(
            r#"#!/usr/bin/python3
import json, os, sys, time
log = {log:?}
release = {release:?}
with open(log, 'a') as f: f.write('spawn %s\n' % os.getpid())
for line in sys.stdin:
    request = json.loads(line)
    method = request['method']
    with open(log, 'a') as f: f.write(method + '\n')
    if method == 'initialize':
        while not os.path.exists(release): time.sleep(0.01)
        result = {{'protocolVersion':'2025-11-25','capabilities':{{'tools':{{}}}},'serverInfo':{{'name':'stall','version':'1'}}}}
    elif method == 'tools/list': result = {{'tools':[]}}
    else: continue
    print(json.dumps({{'jsonrpc':'2.0','id':request['id'],'result':result}}), flush=True)
"#,
            log = log.to_string_lossy(),
            release = release.to_string_lossy()
        ),
    );
    fixture.write_config(
        &responses,
        json!({"stall": {
            "type":"local", "command":[server], "enabled":true, "timeout":10000
        }}),
        json!({}),
    );
    let mut tui = PtyProcess::spawn(&fixture, "v01-pending");
    tui.wait_visible(READY);
    tui.send_line(if manual_compress {
        "/dcp-compress pending draft"
    } else {
        "pending draft"
    });
    let deadline = Instant::now() + IO_TIMEOUT;
    while !fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("initialize")
    {
        assert!(Instant::now() < deadline, "initialize did not start");
        std::thread::sleep(POLL);
    }
    let lifecycle = fs::read_to_string(&log).unwrap();
    let pid: libc::pid_t = lifecycle
        .lines()
        .next()
        .unwrap()
        .strip_prefix("spawn ")
        .unwrap()
        .parse()
        .unwrap();
    let start = Instant::now();
    if manual_compress {
        tui.wait_screen("dcp |", IO_TIMEOUT);
        tui.raw(b"\x1b"); // close DCP panel using its existing key behavior
        let deadline = start + IO_TIMEOUT;
        while tui.screen().iter().any(|row| row.contains("dcp |")) {
            assert!(
                Instant::now() < deadline,
                "DCP panel did not close before acceptance"
            );
            std::thread::sleep(POLL);
        }
    }
    let resize_offset = tui.output.lock().unwrap().len();
    tui.raw(b"\r"); // duplicate Enter while acceptance is stalled
    tui.resize(120, 40);
    tui.raw(b" edited");
    let deadline = start + IO_TIMEOUT;
    loop {
        let bytes = tui.output.lock().unwrap().clone();
        if contains(&normalize(&bytes), b"edited") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "UI frozen before acceptance: edit/resize not rendered within 2s"
        );
        std::thread::sleep(POLL);
    }
    tui.wait_screen(
        "pending draft edited",
        deadline.saturating_duration_since(Instant::now()),
    );
    assert!(
        tui.output.lock().unwrap()[resize_offset..]
            .windows(5)
            .any(|w| w == b"\x1b[40;"),
        "resize must actually render row 40 before fake release"
    );
    tui.raw(b"\x1b"); // actual Esc, no direct CoreApp cancellation
    loop {
        // SAFETY: signal zero probes only the pid recorded by our fake.
        if unsafe { libc::kill(pid, 0) } != 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pending MCP child not reaped within 2s"
        );
        std::thread::sleep(POLL);
    }
    assert!(!release.exists(), "cancellation must precede release");
    let cancel_elapsed = start.elapsed();
    assert!(
        responses.requests().is_empty(),
        "unaccepted prompt reached provider"
    );
    let db =
        rusqlite::Connection::open(fixture.home.join("data/oc/oc.sqlite")).expect("fixture DB");
    let turns: i64 = db
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0, "cancelled pre-acceptance created a turn");
    let messages: i64 = db
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(messages, 0);
    fs::write(&release, "release").unwrap();
    tui.wait_visible("cancelled");
    let offset = tui.send_line(""); // retry exact retained, edited draft
    tui.wait_visible_after(
        offset,
        if manual_compress {
            "answer:Manual context compression request."
        } else {
            "answer:pending draft edited"
        },
    );
    assert!(
        serde_json::to_string(&responses.requests())
            .unwrap()
            .contains("pending draft edited")
    );
    let deadline = Instant::now() + IO_TIMEOUT;
    loop {
        let completed: i64 = db
            .query_row(
                "SELECT count(*) FROM turns WHERE status='completed'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        if completed == 1 {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(POLL);
    }
    responses.wait_requests(2);
    tui.raw(b"\x03");
    assert!(tui.wait_exit().success());
    assert_eq!(
        responses.requests().len(),
        2,
        "one accepted request plus its title, no duplicate"
    );
    let turns: i64 = db
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 1);
    let messages: i64 = db
        .query_row("SELECT count(*) FROM messages", [], |r| r.get(0))
        .unwrap();
    assert_eq!(messages, 2);
    // Quit during a fresh stalled handshake. Application shutdown must finish
    // cleanup without accepting the prompt or waiting for the fake's release.
    fs::remove_file(&release).unwrap();
    let mut quitting = PtyProcess::spawn(&fixture, "v01-quit-pending");
    quitting.wait_visible(READY);
    quitting.send_line(if manual_compress {
        "/dcp-compress quit before acceptance"
    } else {
        "quit before acceptance"
    });
    let deadline = Instant::now() + IO_TIMEOUT;
    while fs::read_to_string(&log)
        .unwrap()
        .lines()
        .filter(|l| *l == "initialize")
        .count()
        < 3
    {
        assert!(Instant::now() < deadline, "quit handshake did not start");
        std::thread::sleep(POLL);
    }
    quitting.raw(b"\x03");
    while quitting.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "pending shutdown exceeded 2s");
        std::thread::sleep(POLL);
    }
    assert!(quitting.wait_exit().success());
    assert!(!release.exists());
    assert_eq!(responses.requests().len(), 2);
    let turns: i64 = db
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 1, "shutdown did not accept pending input");
    let lifecycle = fs::read_to_string(&log).unwrap();
    assert_eq!(lifecycle.lines().filter(|l| *l == "initialize").count(), 3);
    for pid in lifecycle
        .lines()
        .filter_map(|l| l.strip_prefix("spawn "))
        .map(|p| p.parse::<libc::pid_t>().unwrap())
    {
        assert_ne!(
            // SAFETY: only probes fixture-owned pids.
            unsafe { libc::kill(pid, 0) },
            0,
            "owned child survived shutdown"
        );
    }
    eprintln!(
        "V01 raw (manual_compress={manual_compress}): prompt+CR, CR, resize 120x40, ' edited', ESC; cancel/reap before release within {:?}; retry CR, Ctrl+C",
        cancel_elapsed
    );
}

/// The failing pre-fix test kills oc; clean fixture children on assertions too.
/// On success, application cleanup is asserted before this fallback runs.
struct FixtureChildren(PathBuf);

impl Drop for FixtureChildren {
    fn drop(&mut self) {
        for pid in fs::read_to_string(&self.0)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.strip_prefix("spawn "))
            .filter_map(|p| p.parse::<libc::pid_t>().ok())
            .filter(|p| *p > 1)
        {
            // SAFETY: each negative pid is a group created by this MCP fixture.
            unsafe {
                libc::kill(-pid, libc::SIGKILL);
            }
        }
    }
}

#[test]
fn v01_anonymous_remote_and_codex_required_auth() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::start("anonymous", "", &["search"]);
    let fixture = Fixture::new();
    let entry =
        json!({"type":"remote", "url":mcp.url, "enabled":true, "oauth":false, "timeout":3000});
    fixture.write_config(&responses, json!({"anonymous":entry}), json!({}));
    let mut process = fixture.spawn_run("v01-anonymous");
    assert!(
        process.wait().success(),
        "anonymous remote rejected: {}",
        process.diagnostics()
    );
    assert_eq!(responses.requests().len(), 2);
    assert!(mcp.records().iter().all(|r| r.authorization.is_none()));
    let count = mcp.records().len();
    fixture.write_config(&responses, json!({"codex_web":entry}), json!({}));
    let mut process = fixture.spawn_run("v01-codex-missing-auth");
    assert!(!process.wait().success());
    let diagnostic = process.diagnostics();
    assert!(
        diagnostic.contains("codex_web")
            && diagnostic.contains("config")
            && diagnostic.contains("invalid_config"),
        "{diagnostic}"
    );
    assert_eq!(
        mcp.records().len(),
        count,
        "codex auth requirement checked before network"
    );
    assert_eq!(
        responses.requests().len(),
        2,
        "required failure has no provider fallback"
    );
}

#[test]
fn v01_required_remote_diagnostic_is_staged_and_redacted_in_tui() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let mcp = FakeMcp::start("strict", "Bearer expected", &["search"]);
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"required":remote_entry(&mcp, "Authorization", "Bearer PRIVATE_HEADER_SENTINEL")}),
        json!({}),
    );
    let mut tui = PtyProcess::spawn(&fixture, "v01-required-failure");
    tui.wait_visible(READY);
    tui.send_line("retained prompt");
    tui.wait_screen("mcp required initialize:", IO_TIMEOUT);
    tui.wait_screen("unauthorized (retryable=false)", IO_TIMEOUT);
    tui.wait_screen("retained prompt", IO_TIMEOUT);
    assert!(responses.requests().is_empty(), "required failure bypassed");
    tui.raw(b"\x03");
    assert!(tui.wait_exit().success());
    let bytes = tui.output.lock().unwrap();
    let visible = String::from_utf8_lossy(&bytes);
    for secret in [
        "PRIVATE_HEADER_SENTINEL",
        "PRIVATE_RESPONSE_BODY_SENTINEL",
        &mcp.url,
    ] {
        assert!(!visible.contains(secret), "diagnostic leaked fixture data");
    }
}

#[test]
fn v01_remote_pending_cancel_closes_request_before_fake_release() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let (mcp, closed) = FakeMcp::stalled_initialize();
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"anonymous": {
            "type":"remote", "url":mcp.url, "enabled":true, "oauth":false, "timeout":10000
        }}),
        json!({}),
    );
    let mut tui = PtyProcess::spawn(&fixture, "v01-remote-cancel");
    tui.wait_visible(READY);
    tui.send_line("remote draft");
    let deadline = Instant::now() + IO_TIMEOUT;
    while mcp.records().is_empty() {
        assert!(Instant::now() < deadline, "initialize not received");
        std::thread::sleep(POLL);
    }
    tui.raw(b"\r\x1b");
    tui.wait_screen("cancelled", IO_TIMEOUT);
    tui.wait_screen("remote draft", IO_TIMEOUT);
    while !closed.load(Ordering::Relaxed) {
        assert!(Instant::now() < deadline, "cancel leaked an HTTP request");
        std::thread::sleep(POLL);
    }
    assert_eq!(mcp.records().len(), 1, "no hidden retry");
    assert!(responses.requests().is_empty());
    tui.raw(b"\x03");
    assert!(tui.wait_exit().success());
    let db = rusqlite::Connection::open(fixture.home.join("data/oc/oc.sqlite")).unwrap();
    let turns: i64 = db
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0);
}
