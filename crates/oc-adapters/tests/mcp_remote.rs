//! T20 (MCP01–03): remote `codex_web` against a fake streamable-HTTP MCP
//! server — exact URL, 2025-11-25, bearer, pagination, errors, cancel.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::mcp_remote::{
    CLIENT_TIMEOUT, CodexWebClient, CodexWebConfig, McpError, map_registry, merge_registries,
    search_args,
};
use serde_json::{Value, json};

type Log = Arc<Mutex<Vec<Record>>>;
type CatalogVersion = Arc<std::sync::atomic::AtomicUsize>;

#[derive(Debug, Clone)]
struct Record {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ok,
    VersionMismatch,
    Always401,
    InvalidJson,
}

#[derive(Clone)]
struct Fake {
    records: Log,
    mode: Mode,
    slow: Duration,
    version: CatalogVersion,
    sse_call: bool,
}

impl Fake {
    fn start(mode: Mode, slow: Duration) -> (String, Log) {
        Self::start_full(mode, slow, false).0
    }

    fn start_full(mode: Mode, slow: Duration, sse_call: bool) -> ((String, Log), CatalogVersion) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake");
        let addr = listener.local_addr().expect("addr");
        let fake = Fake {
            records: Arc::new(Mutex::new(Vec::new())),
            mode,
            slow,
            version: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            sse_call,
        };
        let version = fake.version.clone();
        let records = fake.records.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let worker = fake.clone();
                std::thread::spawn(move || worker.serve(stream));
            }
        });
        (
            (format!("http://127.0.0.1:{}/v1/mcp", addr.port()), records),
            version,
        )
    }

    fn serve(&self, stream: std::net::TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let Some((method, path, headers, body_text)) = Self::read_request(&mut reader) else {
            return;
        };
        self.records.lock().expect("records").push(Record {
            method: method.clone(),
            path: path.clone(),
            headers: headers.clone(),
            body: body_text.clone(),
        });
        let (status, payload, content_type) = self.route(&method, &body_text);
        Self::respond(&mut reader, status, &payload, content_type);
    }

    fn read_request(
        reader: &mut BufReader<std::net::TcpStream>,
    ) -> Option<(String, String, HashMap<String, String>, String)> {
        let mut request_line = String::new();
        let mut headers = HashMap::new();
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return None,
                Ok(_) => {}
                Err(_) => return None,
            }
            let line = line.trim_end().to_string();
            if request_line.is_empty() {
                if line.is_empty() {
                    return None;
                }
                request_line = line;
                continue;
            }
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                let name = name.trim().to_lowercase();
                let value = value.trim().to_string();
                if name == "content-length" {
                    content_length = value.parse().unwrap_or(0);
                }
                headers.insert(name, value);
            }
        }
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();
        let mut body = vec![0u8; content_length];
        if content_length > 0 && reader.read_exact(&mut body).is_err() {
            return None;
        }
        Some((
            method,
            path,
            headers,
            String::from_utf8_lossy(&body).to_string(),
        ))
    }

    fn respond(
        reader: &mut BufReader<std::net::TcpStream>,
        status: u16,
        payload: &[u8],
        content_type: Option<&str>,
    ) {
        let mut response = format!(
            "HTTP/1.1 {status} x\r\ncontent-length: {}\r\nconnection: close\r\n",
            payload.len()
        );
        if let Some(content_type) = content_type {
            response.push_str(&format!("content-type: {content_type}\r\n"));
        }
        response.push_str("\r\n");
        let stream = reader.get_mut();
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.write_all(payload);
        let _ = stream.flush();
    }

    fn route(&self, method: &str, body: &str) -> (u16, Vec<u8>, Option<&'static str>) {
        if self.mode == Mode::Always401 {
            return (401, Vec::new(), None);
        }
        if self.mode == Mode::InvalidJson {
            return (200, b"not json".to_vec(), Some("application/json"));
        }
        if method == "GET" {
            return (405, Vec::new(), None);
        }
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let rpc_method = parsed.get("method").and_then(Value::as_str).unwrap_or("");
        let id = parsed.get("id").cloned().unwrap_or(Value::Null);
        let reply = |result: Value| {
            let payload = json!({"jsonrpc": "2.0", "id": id, "result": result});
            (
                200,
                serde_json::to_vec(&payload).expect("json"),
                Some("application/json"),
            )
        };
        match rpc_method {
            "server/discover" => (404, Vec::new(), None),
            "initialize" => {
                let version = if self.mode == Mode::VersionMismatch {
                    "2025-06-18"
                } else {
                    "2025-11-25"
                };
                reply(json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": true}},
                    "serverInfo": {"name": "fake-codex-web", "version": "test"},
                }))
            }
            "notifications/initialized" => (202, Vec::new(), None),
            "tools/list" => {
                let cursor = parsed
                    .get("params")
                    .and_then(|params| params.get("cursor"))
                    .and_then(Value::as_str);
                let mut first = vec![
                    tool("search", "Search the web"),
                    tool("fetch", "Fetch a URL"),
                ];
                if self.version.load(Ordering::Relaxed) >= 1 {
                    first.push(tool("fresh", "New tool"));
                }
                if cursor.is_none() {
                    reply(json!({
                        "tools": first,
                        "nextCursor": "p2",
                    }))
                } else {
                    reply(json!({"tools": [tool("summarize", "Summarize text")]}))
                }
            }
            "tools/call" => {
                let name = parsed
                    .get("params")
                    .and_then(|params| params.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let query = parsed
                    .get("params")
                    .and_then(|params| params.get("arguments"))
                    .and_then(|args| args.get("query"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if query == "slow" {
                    std::thread::sleep(self.slow);
                }
                if name == "search" && query == "boom" {
                    return reply(json!({
                        "content": [{"type": "text", "text": "nope"}],
                        "isError": true,
                    }));
                }
                let result = json!({
                    "content": [{"type": "text", "text": format!("result for {query}")}],
                    "isError": false,
                });
                if self.sse_call {
                    let frame = format!(
                        "event: message\ndata: {}\n\n",
                        json!({"jsonrpc": "2.0", "id": id, "result": result})
                    );
                    return (200, frame.into_bytes(), Some("text/event-stream"));
                }
                reply(result)
            }
            _ => {
                let payload = json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": {"code": -32601, "message": "unknown"},
                });
                (
                    200,
                    serde_json::to_vec(&payload).expect("json"),
                    Some("application/json"),
                )
            }
        }
    }
}

fn tool(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": {"query": {"type": "string"}},
        },
    })
}

fn config_for(url: &str) -> CodexWebConfig {
    CodexWebConfig {
        url: url.to_string(),
        bearer: "test-key".to_string(),
        timeout: CLIENT_TIMEOUT,
        allow_private: true,
    }
}

fn records_of(records: &Arc<Mutex<Vec<Record>>>) -> Vec<Record> {
    records.lock().expect("records").clone()
}

#[tokio::test]
async fn handshake_uses_exact_url_and_bearer() {
    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    drop(client);
    let seen = records_of(&records);
    assert!(!seen.is_empty(), "handshake must hit the server");
    for record in &seen {
        assert_eq!(record.method, "POST");
        assert_eq!(record.path, "/v1/mcp", "exact configured path, no rewrite");
        assert_eq!(
            record.headers.get("authorization").map(String::as_str),
            Some("Bearer test-key"),
        );
        let accept = record.headers.get("accept").cloned().unwrap_or_default();
        assert!(accept.contains("application/json"), "accept json: {accept}");
        assert!(accept.contains("text/event-stream"), "accept sse: {accept}");
    }
    let methods: Vec<String> = seen
        .iter()
        .filter_map(|record| {
            serde_json::from_str::<Value>(&record.body)
                .ok()
                .and_then(|body| body.get("method")?.as_str().map(str::to_string))
        })
        .collect();
    assert!(
        methods.contains(&"initialize".to_string()),
        "methods: {methods:?}"
    );
    assert_eq!(
        seen.iter().filter(|record| record.method == "GET").count(),
        0,
        "no standalone GET"
    );
}

#[tokio::test]
async fn version_mismatch_is_rejected() {
    let (url, _) = Fake::start(Mode::VersionMismatch, Duration::ZERO);
    let error = match CodexWebClient::connect(&config_for(&url)).await {
        Err(error) => error,
        Ok(_) => panic!("version must fail"),
    };
    assert_eq!(error, McpError::Transport);
}

#[tokio::test]
async fn unauthorized_is_terminal_without_retry() {
    let (url, records) = Fake::start(Mode::Always401, Duration::ZERO);
    let error = match CodexWebClient::connect(&config_for(&url)).await {
        Err(error) => error,
        Ok(_) => panic!("401 must fail"),
    };
    assert_eq!(error, McpError::Unauthorized);
    assert!(
        records_of(&records).len() <= 2,
        "single attempt, no reconnect loop"
    );
}

#[tokio::test]
async fn list_paginates_and_maps_registry() {
    let (url, _) = Fake::start(Mode::Ok, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.expect("list");
    let names: Vec<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    assert_eq!(names, ["search", "fetch", "summarize"], "both pages");
    let triples: Vec<(String, Option<String>, Value)> = tools
        .into_iter()
        .map(|tool| (tool.name, tool.description, tool.input_schema))
        .collect();
    let (entries, skipped) = map_registry("codex", triples);
    assert!(skipped.is_empty());
    let namespaced: Vec<&str> = entries
        .iter()
        .map(|entry| entry.namespaced.as_str())
        .collect();
    assert_eq!(
        namespaced,
        ["codex__search", "codex__fetch", "codex__summarize"]
    );
}

#[tokio::test]
async fn search_returns_text_and_surfaces_is_error() {
    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);
    let text = client
        .search("rust", Some(3), &cancel)
        .await
        .expect("search");
    assert_eq!(text, "result for rust");
    let calls: Vec<Record> = records_of(&records)
        .into_iter()
        .filter(|record| record.body.contains("tools/call"))
        .collect();
    assert_eq!(calls.len(), 1);
    let call = calls.into_iter().next().expect("call");
    assert_eq!(
        call.headers.get("mcp-protocol-version").map(String::as_str),
        Some("2025-11-25"),
        "version header on tool calls",
    );
    let error = client
        .search("boom", None, &cancel)
        .await
        .expect_err("isError must fail");
    assert_eq!(error, McpError::ToolFailed);
}

#[tokio::test]
async fn slow_call_can_be_cancelled() {
    let (url, _) = Fake::start(Mode::Ok, Duration::from_secs(30));
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let flag = AtomicBool::new(false);
    let pending = client.search("slow", None, &flag);
    tokio::pin!(pending);
    tokio::time::sleep(Duration::from_millis(300)).await;
    flag.store(true, Ordering::Relaxed);
    let error = pending.await.expect_err("cancel must win");
    assert_eq!(error, McpError::Cancelled);
}

#[tokio::test]
async fn slow_call_hits_client_deadline() {
    let (url, _) = Fake::start(Mode::Ok, Duration::from_secs(30));
    let mut config = config_for(&url);
    config.timeout = Duration::from_millis(300);
    let client = CodexWebClient::connect(&config).await.expect("connect");
    let cancel = AtomicBool::new(false);
    let error = client
        .search("slow", None, &cancel)
        .await
        .expect_err("deadline must fire");
    assert_eq!(error, McpError::Deadline);
}

#[test]
fn config_validation_needs_no_network() {
    for url in ["", "ftp://x/y", "https://exa mple.com/mcp"] {
        let config = CodexWebConfig {
            url: url.to_string(),
            bearer: "k".to_string(),
            timeout: CLIENT_TIMEOUT,
            allow_private: true,
        };
        assert_eq!(
            config.validate(),
            Err(McpError::InvalidConfig),
            "url {url:?}"
        );
    }
    let config = CodexWebConfig {
        url: "https://example.com/v1/mcp".to_string(),
        bearer: "".to_string(),
        timeout: CLIENT_TIMEOUT,
        allow_private: true,
    };
    assert_eq!(config.validate(), Err(McpError::InvalidConfig));
}

#[tokio::test]
async fn private_host_refused_without_test_flag() {
    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let mut config = config_for(&url);
    config.allow_private = false;
    let error = match CodexWebClient::connect(&config).await {
        Err(error) => error,
        Ok(_) => panic!("loopback must be refused"),
    };
    assert_eq!(error, McpError::PrivateHost);
    assert!(
        records_of(&records).is_empty(),
        "refused before any request"
    );
}

#[test]
fn registry_bounds_and_collisions() {
    let big = Value::String("x".repeat(64 * 1024));
    let (entries, skipped) = map_registry(
        "codex",
        vec![
            ("a".to_string(), None, json!({"type": "object"})),
            ("a".to_string(), None, json!({"type": "object"})),
            ("big".to_string(), None, big),
        ],
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].namespaced, "codex__a");
    assert_eq!(skipped, ["a", "big"]);
    let merged = merge_registries(vec![
        vec![oc_adapters::mcp_remote::RegistryEntry {
            namespaced: "codex__search".to_string(),
            server: "codex".to_string(),
            tool: "search".to_string(),
            description: None,
            input_schema: json!({}),
        }],
        vec![oc_adapters::mcp_remote::RegistryEntry {
            namespaced: "codex__search".to_string(),
            server: "other".to_string(),
            tool: "search".to_string(),
            description: None,
            input_schema: json!({}),
        }],
    ]);
    assert_eq!(merged[0].namespaced, "codex__search");
    assert_eq!(merged[1].namespaced, "codex__search__2");
}

#[tokio::test]
async fn sse_response_path_supported() {
    let ((url, _), _) = Fake::start_full(Mode::Ok, Duration::ZERO, true);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);
    let text = client
        .search("rust", None, &cancel)
        .await
        .expect("sse search");
    assert_eq!(text, "result for rust");
}

#[tokio::test]
async fn list_update_is_visible_on_relist() {
    let ((url, _), version) = Fake::start_full(Mode::Ok, Duration::ZERO, false);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);
    let before = client.list_tools(&cancel).await.expect("list");
    assert_eq!(before.len(), 3);
    version.store(1, Ordering::Relaxed);
    let after = client.list_tools(&cancel).await.expect("relist");
    assert_eq!(after.len(), 4, "catalog update must surface");
    assert!(after.iter().any(|tool| tool.name == "fresh"));
}

#[tokio::test]
async fn invalid_replies_are_transport_errors() {
    let (url, _) = Fake::start(Mode::InvalidJson, Duration::ZERO);
    let error = match CodexWebClient::connect(&config_for(&url)).await {
        Err(error) => error,
        Ok(_) => panic!("garbage handshake must fail"),
    };
    assert_eq!(error, McpError::Transport);
}

#[test]
fn search_args_shape() {
    assert_eq!(search_args("q", None), json!({"query": "q"}));
    assert_eq!(search_args("q", Some(5)), json!({"query": "q", "limit": 5}));
}

#[test]
fn entry_mapping_refuses_oauth_and_keeps_exact_url() {
    use oc_adapters::config::McpEntry;
    use std::collections::BTreeMap;

    let mut headers = BTreeMap::new();
    headers.insert("authorization".to_string(), "Bearer live-key".to_string());
    let entry = McpEntry {
        kind: "remote".to_string(),
        url: Some("https://mcp.example.com/v1/mcp".to_string()),
        enabled: true,
        oauth: false,
        headers,
        command: Vec::new(),
        timeout: Some(5_000),
        codemode: None,
    };
    let config = CodexWebConfig::from_entry(&entry).expect("entry maps");
    assert_eq!(config.url, "https://mcp.example.com/v1/mcp");
    assert_eq!(config.bearer, "live-key");
    assert_eq!(config.timeout, Duration::from_millis(5_000));
    assert!(
        !format!("{config:?}").contains("live-key"),
        "bearer redacted"
    );

    let mut oauth = entry.clone();
    oauth.oauth = true;
    assert_eq!(
        CodexWebConfig::from_entry(&oauth),
        Err(McpError::InvalidConfig)
    );
    let mut local = entry.clone();
    local.kind = "local".to_string();
    assert_eq!(
        CodexWebConfig::from_entry(&local),
        Err(McpError::InvalidConfig)
    );
    let mut disabled = entry.clone();
    disabled.enabled = false;
    assert_eq!(
        CodexWebConfig::from_entry(&disabled),
        Err(McpError::InvalidConfig)
    );
    let mut no_bearer = entry.clone();
    no_bearer.headers.clear();
    assert_eq!(
        CodexWebConfig::from_entry(&no_bearer),
        Err(McpError::InvalidConfig)
    );
}

/// Reusable live harness: real `codex_web` search when credentials exist.
///
/// Without `LUDKA2_MCP_URL`/`LUDKA2_API_KEY` this records
/// `BUILD_READY_LIVE_BLOCKED` and passes without touching the network —
/// never a false PASS.
#[tokio::test]
#[ignore = "needs live codex_web credentials"]
async fn live_search_harness() {
    let (url, key) = match (
        std::env::var("LUDKA2_MCP_URL"),
        std::env::var("LUDKA2_API_KEY"),
    ) {
        (Ok(url), Ok(key)) if !url.trim().is_empty() && !key.trim().is_empty() => (url, key),
        _ => {
            eprintln!("BUILD_READY_LIVE_BLOCKED: set LUDKA2_MCP_URL and LUDKA2_API_KEY");
            return;
        }
    };
    let config = CodexWebConfig {
        url,
        bearer: key,
        timeout: CLIENT_TIMEOUT,
        allow_private: false,
    };
    let client = CodexWebClient::connect(&config)
        .await
        .expect("live connect");
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.expect("live list");
    assert!(!tools.is_empty(), "live catalog must be non-empty");
    let text = client
        .search("oc smoke probe", Some(3), &cancel)
        .await
        .expect("live search");
    assert!(!text.trim().is_empty(), "live search must return text");
}
