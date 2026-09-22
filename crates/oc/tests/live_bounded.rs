//! T41 (AUD36/AUD37): bounded live campaign harness on the product binary.
//!
//! Two entry points share one campaign runner:
//!
//! * `live_bounded_dry_run_branches` (offline, part of the normal suite) runs
//!   the campaign twice against loopback peers: once with an MCP server
//!   configured (the step executes) and once without (the step is recorded as
//!   `blocked`, never silently skipped). This is how every harness branch is
//!   exercised before any paid run.
//! * `live_bounded_campaign` (ignored) is fail-closed until an approved durable
//!   campaign-envelope authority can be verified. The external boundary returns
//!   machine-readable BLOCKED before discovery, MCP attachment or generation.
//!
//! Model eligibility comes from the product's effective static/discovered
//! catalog; unknown capacities remain unknown (T47 supplies local admission
//! policy). `OC_TEST_CONFIG` selects a standard product config root. Watchdogs
//! and five steps do not enforce the runbook's request/search/output envelope.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
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
const STEP_NAMES: [&str; STEP_BUDGET] = [
    "coding-fix",
    "mcp",
    "compress",
    "restart",
    "workspace-switch",
];
const SEARCH_SERVER: &str = "codex_web";
const SEARCH_TOOL: &str = "codex_web__search";
const UNVERIFIED_ENVELOPE: &str = "campaign envelope enforcement is unverified: require durable <=24 generation HTTP requests including retries/title/subagents, <=4 short MCP searches, output <=2048 smoke/8192 coding tokens across restarts";
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

    fn remaining(&self) -> Duration {
        CAMPAIGN_TIMEOUT.saturating_sub(self.started.elapsed())
    }

    fn passed(&self) -> bool {
        self.results.len() == STEP_BUDGET
            && self
                .results
                .iter()
                .zip(STEP_NAMES)
                .all(|(result, name)| result.name == name && result.status == "passed")
    }

    fn summary(&self, mode: &str, model: &str, variant: Option<&str>) -> Value {
        json!({
            "harness": "live_bounded",
            "mode": mode,
            "model": model,
            "variant": variant,
            "status": if self.passed() { "passed" } else { "non-success" },
            "live_envelope_verified": false,
            "budget": {"step_budget": STEP_BUDGET,
                       "campaign_timeout_seconds": CAMPAIGN_TIMEOUT.as_secs(),
                       "call_timeout_seconds": CALL_TIMEOUT.as_secs()},
            "steps": self.results.iter().map(|result| json!({
                "name": result.name, "status": result.status,
                "kind": result.kind, "detail": result.detail
            })).chain(STEP_NAMES.iter().skip(self.results.len()).map(|name| json!({
                "name": name, "status": "unexecuted", "kind": "blocked",
                "detail": "campaign did not execute this required step"
            }))).collect::<Vec<_>>(),
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
        "live_envelope_verified": false,
        "reason": reason,
        "steps": STEP_NAMES.iter().map(|name| json!({"name": name, "status": "unexecuted", "reason": reason})).collect::<Vec<_>>(),
        "counts": {"attempted": 0, "passed": 0, "failed": 0, "blocked": 1, "skipped": 0,
                   "unexecuted": STEP_BUDGET},
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

#[derive(serde::Serialize, serde::Deserialize)]
struct LiveConfig {
    variant: Option<String>,
    provider_id: String,
    model_id: String,
    context: Option<u64>,
    output: Option<u64>,
    budget_context: u64,
    budget_output: u64,
    local_fallback: bool,
    mcp_servers: Vec<String>,
}

impl LiveConfig {
    async fn load(model: &str, variant: Option<&str>, project: &Path) -> Result<Self, String> {
        let (provider_id, model_id) = split_model(model)?;
        // No bespoke HTTP discovery or config interpretation. Do not propagate
        // composition diagnostics: they can contain substituted configuration.
        let loaded = oc_adapters::composition::load(project).await.map_err(|_| {
            "effective config/catalog unavailable (credentials, config or discovery)".to_string()
        })?;
        if loaded.catalog.provider != provider_id {
            return Err("effective primary agent selects a different provider".into());
        }
        let selection = oc_adapters::models::select_model(&loaded.catalog, model_id)
            .map_err(|_| "requested model is absent from the effective catalog".to_string())?;
        let selection = oc_adapters::models::select_variant(&selection, variant)
            .map_err(|_| "requested variant is unavailable in the effective catalog".to_string())?;
        validate_smoke_permissions(&loaded)?;
        let limits = |key| {
            selection
                .entry
                .pointer(key)
                .and_then(Value::as_u64)
                .filter(|n| *n > 0)
        };
        let context = limits("/limit/context");
        let output = limits("/limit/output");
        let fallback = loaded.generation.providers[provider_id]
            .options
            .native_fallback_limits;
        let budget = oc_adapters::models::budget(&selection, 0, fallback);
        let mcp_servers = loaded
            .generation
            .mcp
            .iter()
            .filter(|(_, entry)| entry.enabled)
            .map(|(name, _)| name.clone())
            .collect();
        Ok(Self {
            variant: selection.variant.map(|v| v.name),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            context,
            output,
            budget_context: budget.context,
            budget_output: budget.output,
            local_fallback: budget.warning.is_some(),
            mcp_servers,
        })
    }

    fn has_search(&self) -> bool {
        self.mcp_servers.iter().any(|name| name == SEARCH_SERVER)
    }
}

/// Match the actual primary lane's independent central + agent constraints.
/// Inspect permissions only; never add grants or connect a server to discover tools.
fn validate_smoke_permissions(
    loaded: &oc_adapters::composition::Composition,
) -> Result<(), String> {
    use oc_adapters::tools::ToolPolicy as _;

    let generation = &loaded.generation;
    let mut rules = generation.permission_rules.clone();
    if let Some(agent) = loaded
        .default_agent
        .as_ref()
        .and_then(|id| loaded.agents.get(id))
    {
        let mut agent_rules = agent.permission_rules.clone();
        if let Some(home) = loaded.parent_env.get("HOME") {
            agent_rules.expand_home(home);
        }
        rules.narrow(&generation.permissions, &agent.permissions, &agent_rules);
    }
    let policy = oc_adapters::runtime::RuntimePolicy::with_rules(&generation.permissions, &rules)
        .with_root(&loaded.project);
    for (action, resource) in [
        ("read", "src/lib.rs"),
        ("apply_patch", "src/lib.rs"),
        ("bash", "cargo test"),
        ("compress", "*"),
    ] {
        policy.check_resource(action, resource).map_err(|_| {
            format!("required headless permission is not allow: {action} ({resource})")
        })?;
    }
    if generation.mcp.get(SEARCH_SERVER).is_some_and(|entry| entry.enabled)
        // These are the exact wire and upstream action identities for the
        // mandatory server/tool, not the bare server name. No catalog is invented.
        && rules.evaluate_actions(
            &generation.permissions,
            &[SEARCH_TOOL, "codex_web_search"],
            "*",
        ) != oc_adapters::config::Permission::Allow
    {
        return Err(format!(
            "required headless permission is not allow: {SEARCH_TOOL} (*)"
        ));
    }
    Ok(())
}

fn split_model(model: &str) -> Result<(&str, &str), String> {
    model
        .split_once('/')
        .filter(|(provider, id)| !provider.trim().is_empty() && !id.trim().is_empty())
        .ok_or_else(|| "OC_TEST_MODEL must be a nonempty exact provider/model ID".into())
}

fn config_root(path: &Path) -> Result<PathBuf, String> {
    // The product accepts roots, not arbitrary file flags. Preserve relative
    // {file:...} credentials, definitions and DCP files in their admitted root.
    if !matches!(
        path.file_name().and_then(|s| s.to_str()),
        Some("opencode.json" | "opencode.jsonc")
    ) || !path.is_file()
    {
        return Err("OC_TEST_CONFIG must name an existing opencode.json or opencode.jsonc".into());
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(|_| "OC_TEST_CONFIG root cannot be resolved".into())
}

/// Subprocess-only preflight: product composition in the exact child environment
/// without mutating the multithreaded test runner's environment. Never generates
/// or connects MCP. It may run native model discovery after explicit live opt-in.
#[test]
#[ignore = "internal isolated product config/catalog probe"]
fn bounded_catalog_probe() {
    let path = std::env::var_os("OC_BOUNDED_PROBE_REPORT").expect("internal probe report");
    let model = std::env::var("OC_TEST_MODEL").unwrap_or_default();
    let variant = std::env::var("OC_TEST_VARIANT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let runtime = tokio::runtime::Runtime::new().expect("probe runtime");
    let result = runtime.block_on(LiveConfig::load(
        &model,
        variant.as_deref(),
        &std::env::current_dir().expect("project"),
    ));
    let report = match result {
        Ok(config) => {
            let data = std::env::var_os("OC_BOUNDED_DATA").expect("probe data");
            let db = oc_adapters::storage::Db::open(Path::new(&data)).expect("private data root");
            let record = json!({
                "provider": config.provider_id, "id": config.model_id, "variant": config.variant
            });
            db.set_pref(oc_core::queries::PREF_MODEL_SELECTION, &record.to_string())
                .expect("validated product selection");
            json!({"config": config})
        }
        Err(reason) => json!({"blocked": reason}),
    };
    std::fs::write(path, report.to_string()).expect("metadata-only probe report");
}

// ---------------------------------------------------------------------------
// Loopback peers (dry run only).
// ---------------------------------------------------------------------------

struct Peer {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    discoveries: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    fn start() -> Self {
        Self::with_catalog(json!({"data": []}))
    }

    fn with_catalog(catalog: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        listener.set_nonblocking(true).expect("nonblocking");
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let hits = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let discoveries = Arc::new(AtomicUsize::new(0));
        let discovery_count = discoveries.clone();
        let handle = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        socket.set_read_timeout(Some(CALL_TIMEOUT)).ok();
                        let Some((request, body)) = read_request(&mut socket) else {
                            continue;
                        };
                        if request.starts_with("GET ") {
                            discovery_count.fetch_add(1, Ordering::Relaxed);
                            let reply = catalog.to_string();
                            let _ = write!(
                                socket,
                                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                                reply.len()
                            );
                            continue;
                        }
                        captured.lock().expect("requests").push(json!({
                            "model": body["model"], "reasoning": body["reasoning"],
                            "max_output_tokens": body["max_output_tokens"]
                        }));
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
            requests,
            discoveries,
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
                        let Some((_, body)) = read_request(&mut socket) else {
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
                                        "properties": {"query": {"type": "string"},
                                            "response_length": {"type": "string", "enum": ["short", "medium", "long"]}},
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

fn read_request(socket: &mut TcpStream) -> Option<(String, Value)> {
    let mut reader = BufReader::new(socket.try_clone().ok()?);
    let mut request = String::new();
    reader.read_line(&mut request).ok()?;
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
    let body = if content_length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&body).ok()?
    };
    Some((request, body))
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
            ("d-test", "bash", json!({"argv": ["cargo", "test"]})),
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
                json!({"query": "bounded live harness", "response_length": "short"}),
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

/// Only the mandatory codex_web search can satisfy this campaign step.
fn mcp_tool_name(body: &Value) -> Option<String> {
    tool_names(body)
        .into_iter()
        .find(|name| name == SEARCH_TOOL)
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
    config_dir: Option<PathBuf>,
    model: String,
    variant: Option<String>,
}

impl Fixture {
    /// Live fixture: real HOME config, isolated data root and project.
    fn live(model: String, variant: Option<String>, config_dir: Option<PathBuf>) -> Self {
        let root = tempfile::tempdir().expect("root");
        let project = root.path().join("project");
        let fixture = Self {
            data_dir: root.path().join("data/oc"),
            root,
            peer: None,
            mcp: None,
            config_dir,
            model,
            variant,
        };
        fixture.prepare_project(&project);
        fixture
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
                permissions[SEARCH_TOOL] = json!("allow");
                json!({SEARCH_SERVER: {"type": "remote", "url": server.url,
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
            config_dir: Some(config_dir),
            model: "fixture/dry-run-model".into(),
            variant: None,
        }
    }

    fn project(&self) -> PathBuf {
        self.root.path().join("project")
    }

    fn prepare_project(&self, project: &Path) {
        write_project(project);
        // Only test model routing; provider metadata and central permissions
        // still come from the admitted product config root.
        std::fs::write(
            project.join("opencode.json"),
            json!({"model": self.model}).to_string(),
        )
        .expect("fixture model overlay");
    }

    fn environment(&self, command: &mut Command) {
        if self.peer.is_some() {
            let home = self.root.path().join("home");
            // Only build-tool locations pass through. No parent config roots,
            // proxy settings or provider credentials enter offline children.
            command.env_clear();
            for key in ["PATH", "RUSTUP_TOOLCHAIN", "RUSTUP_HOME", "CARGO_HOME"] {
                if let Some(value) = std::env::var_os(key) {
                    command.env(key, value);
                }
            }
            if let Some(parent_home) = std::env::var_os("HOME") {
                for (key, name) in [("RUSTUP_HOME", ".rustup"), ("CARGO_HOME", ".cargo")] {
                    if std::env::var_os(key).is_none() {
                        command.env(key, Path::new(&parent_home).join(name));
                    }
                }
            }
            command
                .env("HOME", &home)
                .env("XDG_CONFIG_HOME", home.join("config"))
                .env("XDG_DATA_HOME", home.join("data"))
                .env("XDG_CACHE_HOME", home.join("cache"))
                .env("XDG_STATE_HOME", home.join("state"))
                .env("OC_TEST_ALLOW_LOOPBACK", "1");
        }
        if let Some(root) = &self.config_dir {
            command.env("OPENCODE_CONFIG_DIR", root);
        }
    }

    fn preflight(&self, remaining: Duration) -> Result<LiveConfig, String> {
        self.verify_external_envelope()?;
        let report_path = self.root.path().join("catalog.json");
        let _ = std::fs::remove_file(&report_path);
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command.args(["--ignored", "--exact", "bounded_catalog_probe"]);
        self.environment(&mut command);
        command
            .current_dir(self.project())
            .env("OC_BOUNDED_PROBE_REPORT", &report_path)
            .env("OC_BOUNDED_DATA", &self.data_dir)
            .env("OC_TEST_MODEL", &self.model)
            .env("OC_TEST_VARIANT", self.variant.as_deref().unwrap_or(""));
        let (ok, detail) = run_bounded(&mut command, remaining);
        if !ok {
            return Err(format!("catalog preflight process failed: {detail}"));
        }
        let report: Value = std::fs::read(&report_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or_else(|| "catalog preflight report unavailable".to_string())?;
        if let Some(reason) = report["blocked"].as_str() {
            return Err(reason.into());
        }
        serde_json::from_value(report["config"].clone())
            .map_err(|_| "catalog preflight report invalid".into())
    }

    fn verify_external_envelope(&self) -> Result<(), String> {
        if self.peer.is_none() {
            // Product retries/rounds and models::budget are request/turn-local.
            // No verifiable runner/proxy campaign quota is integrated here.
            // Only an owned offline peer is exempt; no environment assertion can
            // authorize external work or reset a counter on a fresh process.
            return Err(UNVERIFIED_ENVELOPE.into());
        }
        Ok(())
    }

    /// Run one product-binary step with the same config as preflight.
    fn run(
        &self,
        project: &Path,
        session: &str,
        prompt: &str,
        remaining: Duration,
    ) -> (bool, String) {
        if let Err(reason) = self.verify_external_envelope() {
            return (false, reason);
        }
        let mut command = Command::new(BIN);
        command.args([
            "run",
            "--data-dir",
            &self.data_dir.to_string_lossy(),
            "--session",
            session,
            prompt,
        ]);
        self.environment(&mut command);
        command.current_dir(project);
        run_bounded(&mut command, remaining)
    }
}

/// Drain both pipes while polling, retain byte counts only, and always reap.
/// Nonblocking reads also bound cleanup when a descendant inherits a pipe.
fn run_bounded(command: &mut Command, remaining: Duration) -> (bool, String) {
    let timeout = remaining.min(CALL_TIMEOUT);
    if timeout.is_zero() {
        return (false, "watchdog=true spawned=false".into());
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let started = Instant::now();
    let Ok(mut child) = command.spawn() else {
        return (false, "spawn_failed=true".into());
    };
    let mut stdout = child.stdout.take().expect("stdout pipe");
    let mut stderr = child.stderr.take().expect("stderr pipe");
    let nonblocking = |fd| {
        // SAFETY: fd is owned by a live pipe; this reads only its flags.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        // SAFETY: the same owned pipe stays alive; change only descriptor flags.
        flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0
    };
    let mut io_ok = nonblocking(stdout.as_raw_fd()) && nonblocking(stderr.as_raw_fd());
    let mut stdout_bytes = 0u64;
    let mut stderr_bytes = 0u64;
    let mut watchdog = false;
    let status = loop {
        if !io_ok {
            break None;
        }
        io_ok = drain(&mut stdout, &mut stdout_bytes) && drain(&mut stderr, &mut stderr_bytes);
        if started.elapsed() >= timeout {
            watchdog = true;
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Err(_) => break None,
            Ok(None) => {}
        }
        std::thread::sleep(POLL);
    };
    // SAFETY: spawn created a dedicated group with the child's PID. No parent
    // or unrelated shell belongs to it. Kill descendants even if the leader exited.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let reaped = child.wait().is_ok();
    if io_ok {
        io_ok = drain(&mut stdout, &mut stdout_bytes) && drain(&mut stderr, &mut stderr_bytes);
    }
    let ok = status.is_some_and(|s| s.success()) && io_ok && reaped && !watchdog;
    (
        ok,
        format!(
            "exit_code={:?} watchdog={watchdog} reaped={reaped} io_ok={io_ok} stdout_bytes={stdout_bytes} stderr_bytes={stderr_bytes}",
            status.and_then(|s| s.code())
        ),
    )
}

fn drain(pipe: &mut impl Read, count: &mut u64) -> bool {
    let mut bytes = [0u8; 8192];
    // A continuously writing child cannot starve deadline checks or the other pipe.
    for _ in 0..16 {
        match pipe.read(&mut bytes) {
            Ok(0) => return true,
            Ok(n) => *count = count.saturating_add(n as u64),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return true,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    true
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

fn bounded_campaign(fixture: &Fixture, mcp_declared: bool, mut campaign: Campaign) -> Campaign {
    let project = fixture.project();
    let other = fixture.root.path().join("project-b");
    fixture.prepare_project(&other);
    let session = format!("s-live-{}", std::process::id());

    // 1. Coding fix through the real tool loop. The prompt carries a large
    // context tail so the later compress step has a measurable gain.
    let coding_prompt = format!(
        "Fix the `add` function in src/lib.rs: read it, use apply_patch, then run `cargo test`. {}",
        "context ".repeat(3_000)
    );
    let (ok, watchdog) = fixture.run(&project, &session, &coding_prompt, campaign.remaining());
    let mut tests = Command::new("cargo");
    tests
        .args(["test", "--quiet", "--offline"])
        .current_dir(&project);
    fixture.environment(&mut tests);
    let (tests_ok, tests_detail) = run_bounded(&mut tests, campaign.remaining());
    campaign.record(
        "coding-fix",
        ok && tests_ok,
        "model",
        format!("run_ok={ok} tests_ok={tests_ok} {watchdog} tests=({tests_detail})"),
    );

    // 2. Only the configured mandatory codex_web search qualifies.
    if !mcp_declared {
        campaign.blocked(
            "mcp",
            "codex_web MCP search is not enabled: blocked, never silently skipped",
        );
    } else if campaign.over_budget() {
        campaign.blocked("mcp", "step budget exhausted");
    } else {
        let before = db_completed_mcp(fixture, &session);
        let (ok, watchdog) = fixture.run(
            &project,
            &session,
            "Use the codex_web MCP search tool for `bounded harness`, with response_length short.",
            campaign.remaining(),
        );
        let completed = new_completed_mcp(before, db_completed_mcp(fixture, &session));
        campaign.record(
            "mcp",
            ok && completed > 0,
            "model",
            format!("run_ok={ok} completed_mcp_ops={completed} {watchdog}"),
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
            campaign.remaining(),
        );
        let blocks = db_blocks(fixture, &session);
        campaign.record(
            "compress",
            ok && blocks > 0,
            "model",
            format!("run_ok={ok} blocks={blocks} {watchdog}"),
        );
    }

    // 4. Restart: a new process replays the durable session.
    if campaign.over_budget() {
        campaign.blocked("restart", "step budget exhausted");
    } else {
        let (ok, watchdog) = fixture.run(
            &project,
            &session,
            "Report what we did so far.",
            campaign.remaining(),
        );
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
        let (ok, watchdog) = fixture.run(
            &other,
            &format!("{session}-b"),
            "Report the workspace.",
            campaign.remaining(),
        );
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

fn db_completed_mcp(
    fixture: &Fixture,
    session: &str,
) -> Option<std::collections::BTreeSet<String>> {
    let db = oc_adapters::storage::Db::open(&fixture.data_dir).ok()?;
    // An unrelated MCP server/tool must not qualify the mandatory search step.
    Some(
        db.list_tool_ops(session)
            .ok()?
            .into_iter()
            .filter(|op| op.name == SEARCH_TOOL && op.state == "completed")
            .map(|op| op.op)
            .collect(),
    )
}

fn new_completed_mcp(
    before: Option<std::collections::BTreeSet<String>>,
    after: Option<std::collections::BTreeSet<String>>,
) -> usize {
    match (before, after) {
        (Some(before), Some(after)) => after.difference(&before).count(),
        _ => 0, // Failed verification cannot become a successful MCP step.
    }
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

fn write_summary(campaign: &Campaign, mode: &str, config: &LiveConfig) {
    let mut report = campaign.summary(
        mode,
        &format!("{}/{}", config.provider_id, config.model_id),
        config.variant.as_deref(),
    );
    report["model_limits"] = json!({"context": config.context, "output": config.output});
    report["request_policy"] = json!({"context": config.budget_context, "output": config.budget_output,
                                     "uses_local_fallback": config.local_fallback});
    let text = serde_json::to_string_pretty(&report).expect("summary json");
    println!("{text}");
    if let Ok(path) = std::env::var("OC_LIVE_SUMMARY") {
        std::fs::write(path, &text).expect("summary file");
    }
    require_complete(campaign);
}

fn require_complete(campaign: &Campaign) {
    assert!(
        campaign.passed(),
        "mandatory campaign is incomplete or non-successful"
    );
}

/// Offline branch coverage: with and without a declared MCP server.
#[test]
fn live_bounded_dry_run_branches() {
    let with_mcp = Fixture::dry_run(true);
    let campaign = Campaign::new();
    let config = with_mcp
        .preflight(campaign.remaining())
        .expect("offline preflight");
    let campaign = bounded_campaign(&with_mcp, config.has_search(), campaign);
    let report = campaign.summary("dry-run", "fixture/dry-run-model", None);
    emit_report(&report, "dry-run");
    require_complete(&campaign);
    assert!(
        !with_mcp
            .mcp
            .as_ref()
            .expect("offline MCP")
            .calls()
            .is_empty()
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
    let campaign = Campaign::new();
    let config = without_mcp
        .preflight(campaign.remaining())
        .expect("offline preflight");
    let campaign = bounded_campaign(&without_mcp, config.has_search(), campaign);
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
    assert!(std::panic::catch_unwind(|| require_complete(&campaign)).is_err());
}

/// Reconfigure only an offline fixture, retaining its central tool permissions.
fn select_fixture(
    fixture: &mut Fixture,
    provider: &str,
    id: &str,
    variant: Option<&str>,
    models: Value,
) {
    let path = fixture
        .config_dir
        .as_ref()
        .expect("fixture config")
        .join("opencode.json");
    let mut value: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("fixture config")).expect("json");
    let mut entry = value["provider"]["fixture"].clone();
    entry["models"] = models;
    entry["options"]["baseURL"] = json!(fixture.peer.as_ref().expect("offline peer").url);
    entry["options"]["nativeFallbackLimits"] = json!({"context": 32768, "output": 256});
    value["provider"] = json!({provider: entry});
    // A wrong global default must never override OC_TEST_MODEL routing.
    value["model"] = json!("unselected/not-the-test-model");
    std::fs::write(&path, value.to_string()).expect("fixture config");
    fixture.config_dir = Some(config_root(&path).expect("OC_TEST_CONFIG route"));
    fixture.model = format!("{provider}/{id}");
    fixture.variant = variant.map(str::to_string);
    fixture.prepare_project(&fixture.project());
}

#[test]
fn bounded_exact_slash_model_variant_config_and_unknown_limits() {
    let mut fixture = Fixture::dry_run(false);
    select_fixture(
        &mut fixture,
        "fixture",
        "org/future~model",
        Some("custom"),
        json!({
            "org/future~model": {"variants": {"custom": {"reasoningEffort": "low"}}}
        }),
    );
    let config = fixture.preflight(CALL_TIMEOUT).expect("exact slash model");
    assert_eq!(config.model_id, "org/future~model");
    assert_eq!(config.variant.as_deref(), Some("custom"));
    assert_eq!((config.context, config.output), (None, None));
    assert!(config.local_fallback);
    assert_eq!((config.budget_context, config.budget_output), (32768, 256));
    let (ok, detail) = fixture.run(
        &fixture.project(),
        "s-exact",
        "Confirm selection.",
        CALL_TIMEOUT,
    );
    assert!(ok, "{detail}");
    let peer = fixture.peer.as_ref().unwrap();
    let requests = peer.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request["model"] == "org/future~model"
                && request["reasoning"]["effort"] == "low"
                && request["max_output_tokens"] == 256)
    );
    assert_eq!(peer.discoveries.load(Ordering::Relaxed), 0);
}

#[test]
fn bounded_discovery_selected_campaign_uses_product_catalog() {
    let mut fixture = Fixture::dry_run(true);
    fixture.peer = Some(Peer::with_catalog(json!({"object": "list", "data": [{
        "id": "org/discovered", "context_length": 128000, "max_completion_tokens": 8000
    }]})));
    select_fixture(&mut fixture, "ludka2", "org/discovered", None, json!({}));
    let campaign = Campaign::new();
    let config = fixture
        .preflight(campaign.remaining())
        .expect("native discovery");
    assert_eq!(config.context, Some(128000));
    assert_eq!(config.output, Some(8000));
    assert!(!config.local_fallback);
    // Verification must work without Fixture.mcp, just as it does in live mode.
    let server = fixture.mcp.take().expect("keep loopback server alive");
    let campaign = bounded_campaign(&fixture, config.has_search(), campaign);
    require_complete(&campaign);
    assert_eq!(server.calls().len(), 1);
    let peer = fixture.peer.as_ref().unwrap();
    assert!(peer.discoveries.load(Ordering::Relaxed) >= 6);
    let requests = peer.requests.lock().unwrap();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request["model"] == "org/discovered" && request["reasoning"].is_null())
    );
}

#[test]
fn bounded_missing_catalog_model_variant_and_credentials_block_before_generation() {
    for (model, variant, models) in [
        ("org/missing", None, json!({"other": {}})),
        (
            "org/future",
            Some("unlisted"),
            json!({"org/future": {"variants": {"low": {"reasoningEffort": "low"}}}}),
        ),
        (
            "org/future",
            Some("disabled"),
            json!({"org/future": {"variants": {"disabled": {"disabled": true}}}}),
        ),
        ("org/future", None, json!({})),
    ] {
        let mut fixture = Fixture::dry_run(false);
        select_fixture(&mut fixture, "fixture", model, variant, models);
        assert!(fixture.preflight(CALL_TIMEOUT).is_err());
        assert!(
            fixture
                .peer
                .as_ref()
                .unwrap()
                .requests
                .lock()
                .unwrap()
                .is_empty()
        );
    }
    let mut fixture = Fixture::dry_run(false);
    fixture.peer = Some(Peer::with_catalog(json!({"data": []})));
    select_fixture(&mut fixture, "ludka2", "org/absent", None, json!({}));
    assert!(fixture.preflight(CALL_TIMEOUT).is_err());
    assert!(
        fixture
            .peer
            .as_ref()
            .unwrap()
            .discoveries
            .load(Ordering::Relaxed)
            > 0
    );
    assert!(
        fixture
            .peer
            .as_ref()
            .unwrap()
            .requests
            .lock()
            .unwrap()
            .is_empty()
    );

    let fixture = Fixture::dry_run(false);
    let path = fixture.config_dir.as_ref().unwrap().join("opencode.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["provider"]["fixture"]["options"]["apiKey"] = json!("{env:OC_BOUNDED_MISSING_TEST_KEY}");
    std::fs::write(path, value.to_string()).unwrap();
    assert!(fixture.preflight(CALL_TIMEOUT).is_err());
    assert!(
        fixture
            .peer
            .as_ref()
            .unwrap()
            .requests
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(config_root(&fixture.root.path().join("missing/opencode.json")).is_err());
    assert!(split_model("").is_err());
    assert!(split_model("fixture/").is_err());
}

#[test]
fn bounded_permission_preflight_rejects_before_generation() {
    for case in [
        "missing bash",
        "denied patch",
        "read requires approval",
        "wrong read resource",
        "denied compress",
        "bare MCP server grant",
        "MCP requires approval",
        "primary agent deny",
    ] {
        let fixture = Fixture::dry_run(true);
        let path = fixture.config_dir.as_ref().unwrap().join("opencode.json");
        let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        match case {
            "missing bash" => {
                value["permissions"].as_object_mut().unwrap().remove("bash");
            }
            "denied patch" => value["permissions"]["apply_patch"] = json!("deny"),
            "read requires approval" => value["permissions"]["read"] = json!("ask"),
            "wrong read resource" => value["permissions"]["read"] = json!({"other.rs": "allow"}),
            "denied compress" => value["permissions"]["compress"] = json!("deny"),
            "bare MCP server grant" | "MCP requires approval" => {
                let permissions = value["permissions"].as_object_mut().unwrap();
                permissions.retain(|name, _| !name.contains("__"));
                let (name, effect) = if case == "bare MCP server grant" {
                    ("codex_web", "allow")
                } else {
                    ("codex_web__search", "ask")
                };
                permissions.insert(name.into(), json!(effect));
            }
            "primary agent deny" => {
                value["default_agent"] = json!("restricted");
                value["agent"] = json!({"restricted": {
                    "mode": "primary", "permission": {"bash": "deny"}
                }});
            }
            _ => unreachable!(),
        }
        std::fs::write(path, value.to_string()).unwrap();
        let reason = fixture.preflight(CALL_TIMEOUT).err().expect(case);
        assert!(
            reason.starts_with("required headless permission is not allow:"),
            "{case}: {reason}"
        );
        assert!(
            fixture
                .peer
                .as_ref()
                .unwrap()
                .requests
                .lock()
                .unwrap()
                .is_empty()
        );
        assert!(fixture.mcp.as_ref().unwrap().calls().is_empty());
    }
    // Resource-scoped approvals and the product's exact upstream MCP alias work.
    let fixture = Fixture::dry_run(true);
    let path = fixture.config_dir.as_ref().unwrap().join("opencode.json");
    let mut value: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["permissions"] = json!({
        "read": {"src/lib.rs": "allow"}, "apply_patch": {"src/lib.rs": "allow"},
        "bash": {"cargo test": "allow"}, "compress": "allow", "codex_web_search": "allow"
    });
    value["default_agent"] = json!("restricted");
    value["agent"] = json!({"restricted": {
        "mode": "primary", "permission": {"bash": {"cargo test": "allow"}}
    }});
    std::fs::write(path, value.to_string()).unwrap();
    let campaign = Campaign::new();
    let config = fixture
        .preflight(campaign.remaining())
        .expect("scoped permissions");
    require_complete(&bounded_campaign(&fixture, config.has_search(), campaign));
}

#[test]
fn bounded_external_preflight_requires_verified_campaign_enforcement() {
    let mut fixture = Fixture::dry_run(true);
    // Keep the loopback peer alive, but exercise the external fixture boundary.
    // No real config/credentials or external endpoint is involved in this test.
    let peer = fixture.peer.take().unwrap();
    assert_eq!(
        fixture.preflight(CALL_TIMEOUT).err().as_deref(),
        Some(UNVERIFIED_ENVELOPE)
    );
    let (ok, detail) = fixture.run(
        &fixture.project(),
        "s-blocked",
        "Acknowledge.",
        CALL_TIMEOUT,
    );
    assert!(!ok, "{detail}");
    assert!(detail.contains("campaign envelope enforcement is unverified"));
    assert!(!fixture.root.path().join("catalog.json").exists());
    assert!(peer.requests.lock().unwrap().is_empty());
    assert_eq!(peer.discoveries.load(Ordering::Relaxed), 0);
    assert!(fixture.mcp.as_ref().unwrap().calls().is_empty());
}

#[test]
fn bounded_successful_text_is_not_a_completed_mcp_operation() {
    let fixture = Fixture::dry_run(true);
    fixture.preflight(CALL_TIMEOUT).expect("preflight");
    let session = "s-no-tool";
    let (ok, detail) = fixture.run(
        &fixture.project(),
        session,
        "Acknowledge without calling any tool.",
        CALL_TIMEOUT,
    );
    assert!(ok, "{detail}");
    assert_eq!(db_completed_mcp(&fixture, session).unwrap().len(), 0);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir).unwrap();
    for (id, state) in [
        ("failed", "failed"),
        ("interrupted", "unknown"),
        ("inflight", "started"),
        ("earlier", "completed"),
    ] {
        db.record_tool_intent(id, session, None, SEARCH_TOOL, "{}")
            .unwrap();
        db.record_tool_outcome(id, state, Some("metadata-only fixture"))
            .unwrap();
    }
    drop(db);
    let before = db_completed_mcp(&fixture, session);
    assert_eq!(before.as_ref().unwrap().len(), 1);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir).unwrap();
    for name in ["other_server__search", "codex_web__fetch"] {
        db.record_tool_intent(name, session, None, name, "{}")
            .unwrap();
        db.record_tool_outcome(name, "completed", Some("unrelated MCP operation"))
            .unwrap();
    }
    drop(db);
    let (ok, detail) = fixture.run(
        &fixture.project(),
        session,
        "Again acknowledge without tools.",
        CALL_TIMEOUT,
    );
    assert!(ok, "{detail}");
    assert_eq!(
        new_completed_mcp(before, db_completed_mcp(&fixture, session)),
        0
    );
    assert_eq!(
        new_completed_mcp(None, db_completed_mcp(&fixture, session)),
        0
    );
}

#[test]
fn bounded_summary_requires_every_mandatory_step() {
    let empty = Campaign::new();
    assert_eq!(
        empty.summary("offline", "fixture/model", None)["counts"]["unexecuted"],
        5
    );
    assert!(std::panic::catch_unwind(|| require_complete(&empty)).is_err());
    for state in ["blocked", "failed", "skipped", "unexecuted"] {
        let mut campaign = Campaign::new();
        for name in STEP_NAMES {
            campaign.record(name, true, "fixture", String::new());
        }
        campaign.results[1].status = state;
        assert!(std::panic::catch_unwind(|| require_complete(&campaign)).is_err());
    }
    let mut full = Campaign::new();
    for name in STEP_NAMES {
        full.record(name, true, "fixture", String::new());
    }
    require_complete(&full);
}

#[test]
fn bounded_pipes_redact_output_drain_and_reap_on_timeout() {
    let mut flood = Command::new("sh");
    flood.args(["-c", "i=0; while [ $i -lt 12000 ]; do printf 'private-output-marker-0123456789\\n'; printf 'private-error-marker-0123456789\\n' >&2; i=$((i+1)); done"]);
    let (ok, detail) = run_bounded(&mut flood, Duration::from_secs(10));
    assert!(ok, "{detail}");
    assert!(!detail.contains("private-"));
    assert!(detail.contains("reaped=true"));
    let mut hanging = Command::new("sh");
    hanging.args(["-c", "printf private-output-marker; sleep 10 & wait"]);
    let started = Instant::now();
    let (ok, detail) = run_bounded(&mut hanging, Duration::from_millis(100));
    assert!(!ok);
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(
        detail.contains("watchdog=true") && detail.contains("reaped=true"),
        "{detail}"
    );
    assert!(!detail.contains("private-"));
}

/// External campaign remains fail-closed until its durable envelope is verifiable.
#[test]
#[ignore = "needs verified durable campaign envelope, product credentials and OC_TEST_MODEL"]
fn live_bounded_campaign() {
    let model = std::env::var("OC_TEST_MODEL").unwrap_or_default();
    if let Err(reason) = split_model(&model) {
        blocked_out(&reason);
    }
    let variant = std::env::var("OC_TEST_VARIANT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let config_dir = match std::env::var_os("OC_TEST_CONFIG")
        .map(|path| config_root(Path::new(&path)))
        .transpose()
    {
        Ok(root) => root,
        Err(reason) => blocked_out(&reason),
    };
    let campaign = Campaign::new();
    let fixture = Fixture::live(model, variant, config_dir);
    let config = match fixture.preflight(campaign.remaining()) {
        Ok(config) => config,
        Err(reason) => blocked_out(&reason),
    };
    let campaign = bounded_campaign(&fixture, config.has_search(), campaign);
    write_summary(&campaign, "live", &config);
}
