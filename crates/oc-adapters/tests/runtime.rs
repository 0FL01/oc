//! T24 (STORE05/TOOL09/DCP08): runtime turn loop against a fake Responses
//! server — completion/drain, unified permission path, MCP fail-fast,
//! Location binding, reload, compress, commands, AUD11/AUD12 outcomes.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::config::{Generation, McpEntry, Permission};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{
    COMMAND_BYTES_CAP, Runtime, RuntimeError, SESSION_LOCATION_PREFIX, ToolCallEvent, TurnParams,
    TurnStatus, expand_command,
};
use oc_adapters::storage::Db;
use oc_core::context_plan::ProtectedSpec;
#[path = "fixtures/approval_lifecycle.rs"]
mod approval_lifecycle;
#[path = "fixtures/background_lifecycle.rs"]
mod background_lifecycle;
#[path = "runtime/model_session_tools.rs"]
mod model_session_tools;
#[path = "fixtures/question_lifecycle.rs"]
mod question_lifecycle;

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n".to_string()
}

fn sse_completed_usage(input_tokens: u64, output_tokens: u64) -> String {
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"usage\":{{\"input_tokens\":{input_tokens},\"output_tokens\":{output_tokens}}}}}}}\n\n"
    )
}

fn sse_completed_without_usage() -> &'static str {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}\n\n"
}

/// Reasoning summary delta (`response.reasoning_summary_text.delta`).
fn sse_reasoning(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.reasoning_summary_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_reasoning_done(id: &str, secret: &str) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "item":{
            "type":"reasoning", "id":id, "encrypted_content":secret, "summary":[], "status":"completed"
        }})
    )
}

fn sse_completed_output(output: Vec<serde_json::Value>) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed", "response":{
            "status":"completed", "output":output, "usage":{"input_tokens":10,"output_tokens":5}
        }})
    )
}

fn sse_message_done(index: u64, message: &serde_json::Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "output_index":index, "item":message})
    )
}

fn sse_message_done_by_id(message: &serde_json::Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.output_item.done", "item":message})
    )
}

fn sse_tool_call(call_id: &str, name: &str, args: &serde_json::Value) -> String {
    let item_id = format!("fc_{call_id}");
    let added = serde_json::json!({"type": "response.output_item.added", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": "", "status": "in_progress"
    }});
    let delta = serde_json::json!({"type": "response.function_call_arguments.delta",
        "item_id": item_id, "delta": args.to_string()});
    let done = serde_json::json!({"type": "response.output_item.done", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": args.to_string(), "status": "completed"
    }});
    format!("data: {added}\n\ndata: {delta}\n\ndata: {done}\n\n")
}

/// Scripted fakes: serve queued SSE bodies in order, then repeat the last.
struct Fake;

type CapturedRequests = Arc<Mutex<Vec<serde_json::Value>>>;

impl Fake {
    /// Stall the body `stall` after sending headers immediately: models a
    /// hung stream where cancel must land without waiting for bytes.
    fn start_stalled(script: Vec<String>, stall: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = queue.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(_) => return,
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
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    let payload = {
                        let mut queue = queue.lock().expect("queue");
                        if queue.len() > 1 {
                            queue.pop_front().expect("script")
                        } else {
                            queue.front().cloned().unwrap_or_default()
                        }
                    };
                    let stream = reader.get_mut();
                    let _ = stream.write_all(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n"
                            .as_bytes(),
                    );
                    let _ = stream.flush();
                    // Drip heartbeats so a cancelled stream observes the flag
                    // promptly instead of sitting inside one chunk wait.
                    let start = std::time::Instant::now();
                    while start.elapsed() < stall {
                        let _ = stream.write_all(b": hb\n\n");
                        let _ = stream.flush();
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    let _ = stream.write_all(payload.as_bytes());
                });
            }
        });
        base
    }

    fn start(script: Vec<String>, delay: Duration) -> (String, Arc<Mutex<usize>>) {
        let (base, hits, _) = Self::start_recording(script, delay);
        (base, hits)
    }

    fn start_recording(
        script: Vec<String>,
        delay: Duration,
    ) -> (String, Arc<Mutex<usize>>, CapturedRequests) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let hits = Arc::new(Mutex::new(0usize));
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let worker_queue = queue.clone();
        let worker_hits = hits.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let worker_requests = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = worker_queue.clone();
                let hits = worker_hits.clone();
                let requests = worker_requests.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(_) => return,
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
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    requests
                        .lock()
                        .expect("requests")
                        .push(serde_json::from_slice(&body).expect("JSON request"));
                    std::thread::sleep(delay);
                    let payload = {
                        let mut queue = queue.lock().expect("queue");
                        *hits.lock().expect("hits") += 1;
                        if queue.len() > 1 {
                            queue.pop_front().expect("script")
                        } else {
                            queue.front().cloned().unwrap_or_default()
                        }
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    let _ = reader.get_mut().write_all(response.as_bytes());
                });
            }
        });
        let _ = &queue;
        (base, hits, requests)
    }
}

/// Title and main requests have independently controlled responses, regardless
/// of connection order. A main stream stays open after headers until released.
fn split_title_server() -> (
    String,
    CapturedRequests,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://127.0.0.1:{}/v1",
        listener.local_addr().unwrap().port()
    );
    let requests: CapturedRequests = Arc::new(Mutex::new(Vec::new()));
    let main_release = Arc::new(AtomicBool::new(false));
    let title_release = Arc::new(AtomicBool::new(false));
    let title_disconnected = Arc::new(AtomicBool::new(false));
    let requests_worker = requests.clone();
    let main_worker = main_release.clone();
    let title_worker = title_release.clone();
    let disconnected_worker = title_disconnected.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().filter_map(Result::ok) {
            let requests = requests_worker.clone();
            let main_release = main_worker.clone();
            let title_release = title_worker.clone();
            let title_disconnected = disconnected_worker.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        len = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0u8; len];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let title = request["model"] == "title-model";
                requests.lock().unwrap().push(request);
                let stream = reader.get_mut();
                let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n");
                let _ = stream.flush();
                let gate = if title { title_release } else { main_release };
                while !gate.load(Ordering::Relaxed) {
                    if stream.write_all(b": hb\n\n").is_err() {
                        if title {
                            title_disconnected.store(true, Ordering::Relaxed);
                        }
                        return;
                    }
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_millis(20));
                }
                let response =
                    sse_delta(if title { "Real title" } else { "main answer" }) + &sse_completed();
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            });
        }
    });
    (
        url,
        requests,
        main_release,
        title_release,
        title_disconnected,
    )
}

async fn wait_for_title_requests(requests: &CapturedRequests, expected: usize) {
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if requests.lock().unwrap().len() >= expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("provider requests");
}

struct Harness {
    _project: tempfile::TempDir,
    _data: tempfile::TempDir,
    db: Db,
    catalog: ModelCatalog,
}

fn allow_all() -> BTreeMap<String, Permission> {
    [
        "read",
        "apply_patch",
        "bash",
        "webfetch",
        "skill",
        "compress",
    ]
    .into_iter()
    .map(|name| (name.to_string(), Permission::Allow))
    .collect()
}

fn make_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let catalog = ModelCatalog {
        provider: "test".to_string(),
        models: [(
            "m".to_string(),
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        )]
        .into_iter()
        .collect(),
    };
    let generation = Generation {
        compaction: Default::default(),
        config_diagnostics: Vec::new(),
        animations: None,
        providers: BTreeMap::new(),
        mcp: BTreeMap::new(),
        permissions,
        permission_rules: Default::default(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    (
        Harness {
            _project: project,
            _data: data,
            db,
            catalog,
        },
        generation,
    )
}

fn runtime_of<'a>(
    harness: &'a Harness,
    generation: Generation,
    protected: Vec<String>,
) -> Runtime<'a> {
    runtime_with_dcp(harness, generation, protected, DcpConfig::default())
}

fn runtime_with_dcp<'a>(
    harness: &'a Harness,
    generation: Generation,
    protected: Vec<String>,
    dcp_config: DcpConfig,
) -> Runtime<'a> {
    let project = harness._project.path();
    let files = oc_adapters::files::Files::new(project, harness._data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(project).expect("shell");
    Runtime::new(
        &harness.db,
        "work",
        generation,
        ProtectedGlobs {
            patterns: protected,
        },
        files,
        shell,
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        dcp_config,
    )
    .expect("runtime")
}

fn provider_of(base: &str) -> ResponsesConfig {
    ResponsesConfig {
        headers: BTreeMap::new(),
        set_cache_key: true,
        base_url: base.to_string(),
        api_key: "test-key".to_string(),
        timeout: Some(false),
        chunk_timeout_ms: 5_000,
        connect_timeout: Duration::from_secs(5),
        allow_private: true,
    }
}

async fn wait_initial_mcp(runtime: &Runtime<'_>) {
    runtime.start_mcp().unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while runtime
        .mcp_status()
        .servers
        .iter()
        .any(|row| row.status == oc_core::queries::McpStatus::Pending)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "initial MCP not settled: {:?}",
            runtime.mcp_status()
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn params<'c>(
    session: &str,
    prompt: &str,
    harness: &'c Harness,
    provider: ResponsesConfig,
    cancel: &'c AtomicBool,
) -> TurnParams<'c> {
    TurnParams {
        session: session.to_string(),
        prompt: prompt.to_string(),
        invocation: None,
        catalog: &harness.catalog,
        model_id: "m".to_string(),
        variant: None,
        max_output: 1_000,
        provider,
        cancel,
        max_rounds: 4,
    }
}

static NO_CANCEL: AtomicBool = AtomicBool::new(false);

async fn dropped_remote_call_cannot_retry_after(shutdown: bool) {
    use std::net::TcpStream;
    use std::sync::atomic::AtomicUsize;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_out = calls.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let server = std::thread::spawn(move || {
        let mut stalled = Vec::<TcpStream>::new();
        while !stopping.load(Ordering::Relaxed) {
            let (socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("fake MCP accept: {error}"),
            };
            let mut reader = BufReader::new(socket);
            reader
                .get_mut()
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let id = request
                .get("id")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let response = match request["method"].as_str().unwrap_or_default() {
                "initialize" => Some(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{
                    "protocolVersion":"2025-11-25", "capabilities":{"tools":{}},
                    "serverInfo":{"name":"fake","version":"1"}}})),
                "tools/list" => Some(serde_json::json!({"jsonrpc":"2.0","id":id,"result":{
                    "tools":[{"name":"ping","inputSchema":{"type":"object","properties":{}}}]}})),
                "tools/call" => {
                    calls_out.fetch_add(1, Ordering::SeqCst);
                    stalled.push(reader.into_inner()); // server-side effect stays active
                    continue;
                }
                _ => None,
            };
            let socket = reader.get_mut();
            if let Some(body) = response {
                let body = body.to_string();
                let _ = write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            } else {
                let _ = socket.write_all(
                    b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        }
        drop(stalled);
    });

    let mut permissions = allow_all();
    permissions.insert("stall__ping".into(), Permission::Allow);
    let (harness, mut generation) = make_harness(permissions);
    generation.mcp.insert(
        "stall".into(),
        McpEntry {
            kind: "remote".into(),
            url: Some(url),
            enabled: true,
            oauth: false,
            headers: BTreeMap::new(),
            command: Vec::new(),
            timeout: Some(10_000),
            codemode: None,
            ..Default::default()
        },
    );
    let project = harness._project.path();
    let runtime = Runtime::new(
        &harness.db,
        "work",
        generation.clone(),
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        oc_adapters::files::Files::new(project, harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(project).unwrap(),
        BTreeMap::from([("OC_TEST_ALLOW_LOOPBACK".into(), "1".into())]),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    runtime.create_session("dropped").unwrap();
    wait_initial_mcp(&runtime).await;
    let tool = sse_tool_call("drop", "stall__ping", &serde_json::json!({}));
    let (base, _, requests) = Fake::start_recording(vec![tool + &sse_completed()], Duration::ZERO);
    let mut turn = Box::pin(runtime.run_turn(params(
        "dropped",
        "first",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    )));
    tokio::time::timeout(Duration::from_secs(8), async {
        while calls.load(Ordering::SeqCst) == 0 {
            tokio::select! {
                result = &mut turn => panic!("turn finished before MCP stall: {result:?}"),
                _ = tokio::time::sleep(Duration::from_millis(10)) => {},
            }
        }
    })
    .await
    .expect("MCP call not sent");
    drop(turn); // neither cancel flag nor turn callback: the real owner future is dropped
    if shutdown {
        runtime
            .shutdown_mcp()
            .await
            .expect("shutdown after caller drop");
    } else {
        runtime
            .reload(generation.clone())
            .await
            .expect("reload after caller drop");
        runtime
            .reload(generation)
            .await
            .expect("second reload cannot clear quarantine");
    }
    runtime.create_session("retry").unwrap();
    let error = runtime
        .run_turn(params(
            "retry",
            "explicit retry",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(
            error,
            oc_adapters::runtime::RuntimeError::McpAttach {
                safe_code: "unsafe_retry",
                retryable: false,
                ..
            }
        ),
        "unsafe retry admitted: {error:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "overlapping server-side effects"
    );
    assert_eq!(requests.lock().unwrap().len(), 1, "retry reached provider");
    let ops = harness.db.list_tool_ops("dropped").unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].state, "unknown",
        "dropped call must be durable unknown"
    );
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let turn_status: String = sql
        .query_row(
            "SELECT status FROM turns WHERE session_id='dropped'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(turn_status, "unknown");
    assert!(runtime.shutdown_mcp().await.is_ok());
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
}

fn dcp_nudge_count(request: &serde_json::Value) -> usize {
    request["input"]
        .to_string()
        .matches("exceeds soft limit")
        .count()
}

fn function_call<'a>(
    request: &'a serde_json::Value,
    call_id: &str,
) -> Option<&'a serde_json::Value> {
    request["input"]
        .as_array()?
        .iter()
        .find(|item| item["type"] == "function_call" && item["call_id"].as_str() == Some(call_id))
}

fn function_output<'a>(request: &'a serde_json::Value, call_id: &str) -> Option<&'a str> {
    request["input"].as_array()?.iter().find_map(|item| {
        (item["type"] == "function_call_output" && item["call_id"].as_str() == Some(call_id))
            .then(|| item["output"].as_str())
            .flatten()
    })
}

fn function_item_count(request: &serde_json::Value, kind: &str, call_id: &str) -> usize {
    request["input"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["type"] == kind && item["call_id"].as_str() == Some(call_id))
        .count()
}

/// End to end through the real application worker: `application::spawn_with_env`
/// (project-local config, no process env mutation) broadcasts the new
/// `ReasoningDelta` and `TurnUsage` events next to `TurnFinished`.
async fn title_fixture(
    url: &str,
) -> (
    tempfile::TempDir,
    tempfile::TempDir,
    tempfile::TempDir,
    oc_core::core_app::CoreApp,
    oc_core::core_app::WorkerGuard,
) {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("opencode.json"), serde_json::json!({
        "model": "fixture/main",
        "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
            "models": {"main": {}, "title-model": {}}}},
        "agent": {"title": {"mode": "subagent", "model": "fixture/title-model", "prompt": "TITLE_AGENT_ONLY"}}
    }).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env)
            .await
            .unwrap();
    (project, data, home, app, guard)
}

async fn check_application_patch_replay(line: &str, count: usize) {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let home = tempfile::tempdir().expect("home");
    let patch = format!(
        "*** Begin Patch\n*** Add File: added.txt\n{}*** End Patch",
        format!("+{line}\n").repeat(count)
    );
    let (base, _) = Fake::start(
        vec![
            sse_tool_call(
                "call-patch",
                "apply_patch",
                &serde_json::json!({"patchText": patch}),
            ) + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        Duration::from_millis(5),
    );
    let config = serde_json::json!({
        "model": "fixture/fixture-model",
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": base, "apiKey": "test-key"},
            "models": {"fixture-model": {
                "name": "DTO fixture",
                "limit": {"context": 65536, "output": 4096},
            }},
        }},
        "permissions": {"apply_patch": "allow"},
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).expect("config");
    let env: BTreeMap<String, String> = [
        ("HOME", home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK", "1".to_string()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    let (app, guard, _diagnostics) =
        application::spawn_with_env(project.path(), data.path(), env.clone())
            .await
            .expect("application");
    let session = SessionId::new("s-app-tools").expect("session id");
    app.create_session(session.clone()).await.expect("create");
    app.rename_session(session.clone(), "Tool event fixture".into())
        .await
        .unwrap();
    let mut rx = app.subscribe();
    app.submit(session.clone(), "patch it".to_string())
        .await
        .expect("submit");

    let mut started = None;
    let mut finished = None;
    let mut checkpoints = Vec::new();
    let mut argument_events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel");
        match event {
            CoreEvent::RetryScheduled { .. } => {}
            CoreEvent::ToolCallStarted {
                op, name, input, ..
            } => started = Some((op, name, input)),
            CoreEvent::ToolCallFinished {
                op,
                name,
                state,
                output,
                patch_effects,
                ..
            } => {
                let effects = patch_effects.expect("public confirmed effects");
                assert_eq!(effects.files[0].path, "added.txt");
                assert_eq!(effects.files[0].hunks[0].lines[0].new_line, Some(1));
                assert!(
                    serde_json::to_vec(&effects).unwrap().len()
                        <= oc_core::patch::EFFECT_PREVIEW_BYTES_CAP
                );
                if count == 120 {
                    assert!(effects.truncated);
                    assert!(!effects.files[0].hunks.is_empty());
                    assert_eq!(effects.files[0].hunks[0].lines.len(), 120);
                    assert_eq!(effects.files[0].hunks[0].new.count, 120);
                }
                finished = Some((op, name, state, output, effects));
            }
            CoreEvent::TurnFinished { text, .. } => {
                assert_eq!(text, "done");
                break;
            }
            CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            CoreEvent::McpChanged(snapshot) => assert!(snapshot.servers.is_empty()),
            CoreEvent::ProviderChanged => panic!("unexpected native discovery in static fixture"),
            CoreEvent::ShellNotice(_) => panic!("unexpected shell in patch-only fixture"),
            CoreEvent::TurnPresentation { projection, .. } => checkpoints.push(projection),
            CoreEvent::TurnStarted { .. }
            | CoreEvent::SessionTitleUpdated { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ReasoningDelta { .. }
            | CoreEvent::ReasoningItemEnded { .. }
            | CoreEvent::TurnUsage { .. }
            | CoreEvent::TurnInterrupted { .. }
            | CoreEvent::Compaction(_) => {}
            CoreEvent::ToolArgumentStream { event, .. } => argument_events.push(event),
            CoreEvent::PermissionAsked(_) | CoreEvent::PermissionResolved { .. } => {}
            CoreEvent::QuestionAsked(_) | CoreEvent::QuestionResolved { .. } => {}
        }
    }
    let (started_op, started_name, started_input) = started.expect("tool call started event");
    assert!(
        matches!(argument_events.first(), Some(oc_core::tool_stream::ToolStreamEvent::Pending { identity, name, preview, .. }) if identity.round == 1 && identity.call_id == "call-patch" && name == "apply_patch" && preview.is_empty())
    );
    assert!(argument_events.iter().any(|event| matches!(event, oc_core::tool_stream::ToolStreamEvent::Linked { identity, op } if identity.call_id == "call-patch" && op == &started_op)));
    assert!(
        checkpoints
            .iter()
            .any(|p| p.part_states.iter().any(|s| s.status == "started"))
    );
    let page = app
        .history_page(session.clone(), None, None, 100)
        .await
        .unwrap();
    let replay = page.rows.iter().find_map(|r| r.turn.as_ref()).unwrap();
    let effects = replay
        .parts
        .iter()
        .find_map(|part| match part {
            oc_core::queries::TranscriptPart::Tool(tool) => tool.patch_effects.as_ref(),
            _ => None,
        })
        .expect("history confirmed effects");
    assert_eq!(effects.files[0].additions, count);
    assert!(line.starts_with(&effects.files[0].hunks[0].lines[0].text));
    assert_eq!(
        checkpoints.last(),
        Some(replay),
        "live checkpoint and replay share exact identities, order, statuses and content"
    );
    let (finished_op, finished_name, finished_state, finished_output, finished_effects) =
        finished.expect("tool call finished event");
    assert_eq!(effects, &finished_effects);
    if count == 120 {
        let tool = replay
            .parts
            .iter()
            .find_map(|p| match p {
                oc_core::queries::TranscriptPart::Tool(tool) => Some(tool),
                _ => None,
            })
            .unwrap();
        assert!(
            tool.input.is_none(),
            "large structured input is omitted after reserving canonical effects"
        );
        assert!(replay.part_states.iter().any(|state| state.input_omitted));
    }
    let served: usize = replay
        .parts
        .iter()
        .map(|part| match part {
            oc_core::queries::TranscriptPart::Tool(tool) => {
                tool.input.as_ref().map_or(0, String::len)
                    + tool.output.as_ref().map_or(0, String::len)
                    + tool
                        .patch_effects
                        .as_ref()
                        .map_or(0, |e| serde_json::to_vec(e).unwrap().len())
            }
            oc_core::queries::TranscriptPart::Text(text)
            | oc_core::queries::TranscriptPart::Reasoning { text, .. } => text.len(),
        })
        .sum();
    assert!(
        served <= oc_core::patch::EFFECT_PREVIEW_BYTES_CAP,
        "joint serving bytes {served}"
    );
    assert_eq!(started_name, "apply_patch");
    assert_eq!(finished_name, "apply_patch");
    assert_eq!(
        started_op, finished_op,
        "both events share the operation id"
    );
    assert_eq!(finished_state, "completed");
    assert!(
        started_input.contains("*** Add File: added.txt"),
        "the event input carries the patch text the card renders: {started_input}"
    );
    assert!(finished_output.contains("added.txt"), "{finished_output}");
    assert_eq!(
        std::fs::read_to_string(project.path().join("added.txt")).unwrap(),
        format!("{line}\n").repeat(count)
    );
    app.shutdown().await.expect("shutdown");
    guard.join().await.expect("join");
    std::fs::remove_file(project.path().join("added.txt")).unwrap();
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let page = app.history_page(session, None, None, 100).await.unwrap();
    let reopened = page.rows.iter().find_map(|r| r.turn.as_ref()).unwrap();
    assert_eq!(
        reopened, replay,
        "restart reads identical persisted DTO without file bytes"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[path = "runtime/context.rs"]
mod context;
#[path = "runtime/mcp_lifecycle.rs"]
mod mcp_lifecycle;
#[path = "runtime/provider_readiness.rs"]
mod provider_readiness;
#[path = "runtime/reconciliation.rs"]
mod reconciliation;
#[path = "runtime/tool_lifecycle.rs"]
mod tool_lifecycle;
#[path = "runtime/turns.rs"]
mod turns;
