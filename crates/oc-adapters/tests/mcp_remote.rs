//! T20 (MCP01–03): remote `codex_web` against a fake streamable-HTTP MCP
//! server — exact URL, 2025-11-25, bearer, pagination, errors, cancel.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::mcp_remote::{
    CLIENT_TIMEOUT, CatalogError, CodexWebClient, CodexWebConfig, McpError, map_registry,
    merge_registries, search_args,
};
use serde_json::{Value, json};

type Log = Arc<Mutex<Vec<Record>>>;
type CatalogVersion = Arc<std::sync::atomic::AtomicUsize>;

#[tokio::test]
async fn dns_and_connect_failures_are_distinct_from_private_host() {
    use oc_adapters::webfetch::{FetchError, check_host};
    // Interior NUL fails locally at resolver input, without external DNS traffic.
    assert_eq!(
        check_host("invalid\0host", 443, false).await,
        Err(FetchError::Dns)
    );
    assert_eq!(
        check_host("127.0.0.1", 443, false).await,
        Err(FetchError::PrivateHost)
    );
    // Hold a bound, non-listening socket so another process cannot take the port.
    let socket = tokio::net::TcpSocket::new_v4().unwrap();
    socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let config = CodexWebConfig {
        url: format!("http://{}/mcp", socket.local_addr().unwrap()),
        bearer: "fixture".into(),
        custom_headers: Default::default(),
        timeout: Duration::from_secs(2),
        allow_private: true,
    };
    assert!(matches!(
        CodexWebClient::connect(&config).await,
        Err(McpError::Connect)
    ));
}

#[tokio::test]
async fn protocol_mismatch_observes_stalled_session_cleanup() {
    use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let config = CodexWebConfig {
        url: format!("http://{}/mcp", listener.local_addr().unwrap()),
        bearer: "fixture".into(),
        custom_headers: Default::default(),
        timeout: Duration::from_secs(5),
        allow_private: true,
    };
    let server = async {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = tokio::io::BufReader::new(socket);
            let mut first = String::new();
            if socket.read_line(&mut first).await.unwrap() == 0 {
                continue; // optional GET may be cancelled before writing headers
            }
            let mut len = 0;
            loop {
                let mut line = String::new();
                socket.read_line(&mut line).await.unwrap();
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = value.trim().parse::<usize>().unwrap();
                }
            }
            let mut body = vec![0; len];
            socket.read_exact(&mut body).await.unwrap();
            if first.starts_with("DELETE ") {
                // Never reply. Cancellation must close this request before
                // connect returns a cleanup failure, rather than mismatch alone.
                assert_eq!(socket.read(&mut [0; 1]).await.unwrap(), 0);
                return;
            }
            let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let response = if body["method"] == "initialize" {
                let payload = json!({"jsonrpc":"2.0", "id":body["id"], "result": {
                    "protocolVersion":"2025-06-18", "capabilities":{}, "serverInfo":{"name":"fixture","version":"1"}
                }}).to_string();
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\nmcp-session-id: fixture-session\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{payload}",
                    payload.len()
                )
            } else {
                let status = if first.starts_with("GET ") {
                    "405 Method Not Allowed"
                } else {
                    "202 Accepted"
                };
                format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
            };
            socket
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
        }
    };
    let (result, ()) = tokio::time::timeout(Duration::from_secs(8), async {
        tokio::join!(CodexWebClient::connect(&config), server)
    })
    .await
    .unwrap();
    assert!(
        matches!(result, Err(McpError::CleanupFailed)),
        "cleanup error must not be discarded"
    );
}

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
    Tools65,
    Pages17,
    DuplicateTool,
    OversizedSchema,
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
                if self.mode == Mode::Tools65 {
                    let tools = (0..65)
                        .map(|index| tool(&format!("tool-{index}"), "tool"))
                        .collect::<Vec<_>>();
                    return reply(json!({"tools": tools}));
                }
                if self.mode == Mode::Pages17 {
                    let page = cursor
                        .and_then(|value| value.strip_prefix('p'))
                        .and_then(|value| value.parse::<usize>().ok())
                        .unwrap_or(1);
                    let next = (page < 17).then(|| format!("p{}", page + 1));
                    return reply(json!({
                        "tools": [tool(&format!("tool-{page}"), "tool")],
                        "nextCursor": next,
                    }));
                }
                if self.mode == Mode::DuplicateTool {
                    return reply(json!({
                        "tools": [tool("duplicate", "first"), tool("duplicate", "second")],
                    }));
                }
                if self.mode == Mode::OversizedSchema {
                    return reply(json!({
                        "tools": [{
                            "name": "oversized",
                            "description": "too large",
                            "inputSchema": {
                                "type": "object",
                                "description": "x".repeat(33 * 1024),
                            },
                        }],
                    }));
                }
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
                if query == "image" {
                    return reply(json!({
                        "content": [{
                            "type": "image",
                            "data": "AA==",
                            "mimeType": "image/png",
                        }],
                        "isError": false,
                    }));
                }
                if query == "structured" {
                    return reply(json!({
                        "content": [{"type": "text", "text": "text"}],
                        "structuredContent": {"answer": 42},
                        "isError": false,
                    }));
                }
                if query == "empty" {
                    return reply(json!({"content": [], "isError": false}));
                }
                if query == "input-required" {
                    return reply(json!({
                        "resultType": "input_required",
                        "requestState": "opaque",
                    }));
                }
                if query == "task" {
                    return reply(json!({
                        "resultType": "task",
                        "taskId": "task-1",
                        "status": "working",
                        "createdAt": "2026-09-21T00:00:00Z",
                        "lastUpdatedAt": "2026-09-21T00:00:00Z",
                        "ttlMs": null,
                    }));
                }
                if query == "transport" {
                    return (200, b"not json".to_vec(), Some("application/json"));
                }
                if query == "oversized-text" {
                    return reply(json!({
                        "content": [{"type": "text", "text": "x".repeat(1024 * 1024 + 1)}],
                        "isError": false,
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
        custom_headers: Default::default(),
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
    client.close().await.expect("close");
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
        assert_eq!(
            record.headers.get("user-agent").map(String::as_str),
            Some(oc_adapters::USER_AGENT),
            "remote MCP requests carry a User-Agent"
        );
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
    assert_eq!(error, McpError::ProtocolMismatch);
}

#[tokio::test]
async fn generic_remote_uses_sdk_protocol_negotiation() {
    let (url, _) = Fake::start(Mode::VersionMismatch, Duration::ZERO);
    let client = CodexWebClient::connect_remote(&config_for(&url))
        .await
        .expect("generic supported version");
    assert!(
        !client
            .list_tools(&AtomicBool::new(false))
            .await
            .unwrap()
            .is_empty()
    );
    client.close().await.unwrap();
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
    let entries = map_registry("codex", triples).expect("map complete catalog");
    let namespaced: Vec<&str> = entries
        .iter()
        .map(|entry| entry.namespaced.as_str())
        .collect();
    assert_eq!(
        namespaced,
        ["codex__search", "codex__fetch", "codex__summarize"]
    );
    client.close().await.expect("close");
}

#[tokio::test]
async fn catalog_tool_and_page_limits_are_errors_without_partial_success() {
    let cancel = AtomicBool::new(false);

    let (url, _) = Fake::start(Mode::Tools65, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect 65 tools");
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(McpError::CatalogLimited)
    );
    client.close().await.expect("close 65 tools");

    let (url, records) = Fake::start(Mode::Pages17, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect 17 pages");
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(McpError::CatalogLimited)
    );
    let list_requests = records_of(&records)
        .iter()
        .filter(|record| record.body.contains("tools/list"))
        .count();
    assert_eq!(list_requests, 16, "cursor after page 16 must fail");
    client.close().await.expect("close 17 pages");
}

#[tokio::test]
async fn invalid_catalogs_are_typed_attach_errors() {
    let cancel = AtomicBool::new(false);
    let (url, _) = Fake::start(Mode::DuplicateTool, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect duplicate catalog");
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(McpError::Catalog(CatalogError::DuplicateTool(
            "duplicate".to_string()
        )))
    );
    client.close().await.expect("close duplicate catalog");

    let (url, _) = Fake::start(Mode::OversizedSchema, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect oversized catalog");
    assert_eq!(
        client.list_tools(&cancel).await,
        Err(McpError::Catalog(CatalogError::SchemaTooLarge(
            "oversized".to_string()
        )))
    );
    client.close().await.expect("close oversized catalog");
}

#[tokio::test]
async fn search_returns_text_and_surfaces_is_error() {
    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);
    let text = client
        .search("rust", Some("short"), &cancel)
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
    let call_body: Value = serde_json::from_str(&call.body).expect("call json");
    assert_eq!(
        call_body["params"]["arguments"],
        json!({"query": "rust", "response_length": "short"}),
    );
    assert!(
        call_body["params"]["arguments"].get("limit").is_none(),
        "the server schema has no limit argument",
    );
    let error = client
        .search("boom", None, &cancel)
        .await
        .expect_err("isError must fail");
    assert_eq!(error, McpError::ToolFailed);
    client.close().await.expect("close");
}

#[tokio::test]
async fn result_modalities_arguments_and_transport_have_distinct_errors() {
    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let client = CodexWebClient::connect(&config_for(&url))
        .await
        .expect("connect");
    let cancel = AtomicBool::new(false);

    let calls_before = records_of(&records)
        .iter()
        .filter(|record| record.body.contains("tools/call"))
        .count();
    assert_eq!(
        client
            .call_tool("search", json!(["not", "an", "object"]), &cancel)
            .await,
        Err(McpError::InvalidArguments)
    );
    let calls_after = records_of(&records)
        .iter()
        .filter(|record| record.body.contains("tools/call"))
        .count();
    assert_eq!(calls_after, calls_before, "bad arguments fail before I/O");

    for query in ["image", "structured", "input-required", "task"] {
        assert_eq!(
            client.search(query, None, &cancel).await,
            Err(McpError::UnsupportedResult),
            "query {query}"
        );
    }
    assert_eq!(
        client.search("empty", None, &cancel).await,
        Err(McpError::BadResult)
    );
    assert_eq!(
        client.search("transport", None, &cancel).await,
        Err(McpError::Transport)
    );
    assert_eq!(
        client.search("oversized-text", None, &cancel).await,
        Err(McpError::BadResult)
    );
    client.close().await.expect("close");
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
            custom_headers: Default::default(),
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
        custom_headers: Default::default(),
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
    let duplicate = map_registry(
        "codex",
        vec![
            ("a".to_string(), None, json!({"type": "object"})),
            ("a".to_string(), None, json!({"type": "object"})),
        ],
    )
    .expect_err("duplicates fail the entire catalog");
    assert_eq!(
        duplicate,
        McpError::Catalog(CatalogError::DuplicateTool("a".to_string()))
    );
    let oversized = map_registry("codex", vec![("big".to_string(), None, big)])
        .expect_err("oversized schema fails the entire catalog");
    assert_eq!(
        oversized,
        McpError::Catalog(CatalogError::SchemaTooLarge("big".to_string()))
    );
    let too_many = (0..65)
        .map(|index| (format!("tool-{index}"), None, json!({"type": "object"})))
        .collect();
    assert_eq!(
        map_registry("codex", too_many),
        Err(McpError::CatalogLimited)
    );
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
    ])
    .expect("bounded merged registry");
    assert_eq!(merged[0].namespaced, "codex__search");
    assert_eq!(merged[1].namespaced, "codex__search__2");
    assert_eq!(
        (merged[0].server.as_str(), merged[0].tool.as_str()),
        ("codex", "search")
    );
    assert_eq!(
        (merged[1].server.as_str(), merged[1].tool.as_str()),
        ("other", "search")
    );
    let encoded = map_registry(
        "unsafe server",
        vec![("tool/with spaces".into(), None, json!({"type": "object"}))],
    )
    .expect("unsafe identity gets provider-safe wire name");
    assert!(encoded[0].namespaced.len() <= 64);
    assert!(
        encoded[0]
            .namespaced
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    );
    assert_eq!(encoded[0].server, "unsafe server");
    assert_eq!(encoded[0].tool, "tool/with spaces");
    let many = (0..3)
        .map(|server| {
            (0..64)
                .map(|tool| oc_adapters::mcp_remote::RegistryEntry {
                    namespaced: format!("s{server}__t{tool}"),
                    server: format!("s{server}"),
                    tool: format!("t{tool}"),
                    description: None,
                    input_schema: json!({"type": "object"}),
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(merge_registries(many), Err(McpError::CatalogLimited));
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
    assert_eq!(
        search_args("q", Some("long")),
        json!({"query": "q", "response_length": "long"})
    );
    assert!(search_args("q", Some("long")).get("limit").is_none());
}

#[tokio::test]
async fn entry_authorization_and_custom_headers_reach_the_strict_endpoint() {
    use oc_adapters::config::McpEntry;

    let (url, records) = Fake::start(Mode::Ok, Duration::ZERO);
    let entry: McpEntry = serde_json::from_value(json!({
        "type": "remote",
        "url": url,
        "enabled": true,
        "oauth": false,
        "headers": {
            "Authorization": "Bearer entry-key",
            "X-Workspace": "workspace-secret",
            "Content-Type": "text/plain",
            "Accept": "text/plain",
        },
        "timeout": 5_000,
    }))
    .expect("JSONC-compatible MCP entry");
    let mut config = CodexWebConfig::from_entry(&entry).expect("normalize entry");
    config.allow_private = true;
    let debug = format!("{config:?}");
    assert!(!debug.contains("entry-key"));
    assert!(!debug.contains("workspace-secret"));

    let client = CodexWebClient::connect(&config).await.expect("connect");
    let cancel = AtomicBool::new(false);
    client.list_tools(&cancel).await.expect("list");
    assert_eq!(
        client.search("headers", Some("medium"), &cancel).await,
        Ok("result for headers".to_string())
    );
    client.close().await.expect("close");

    let seen = records_of(&records);
    assert!(!seen.is_empty());
    for record in seen {
        assert_eq!(record.path, "/v1/mcp", "exact URL path");
        assert_eq!(record.method, "POST", "no OAuth or GET probe");
        assert_eq!(
            record.headers.get("authorization").map(String::as_str),
            Some("Bearer entry-key")
        );
        assert_eq!(
            record.headers.get("x-workspace").map(String::as_str),
            Some("workspace-secret")
        );
        assert_eq!(
            record.headers.get("content-type").map(String::as_str),
            Some("application/json"),
            "rmcp controls the body media type"
        );
        let accept = record.headers.get("accept").cloned().unwrap_or_default();
        assert!(accept.contains("application/json"));
        assert!(accept.contains("text/event-stream"));
    }
}

#[test]
fn header_names_are_case_insensitive_and_conflicts_are_explicit() {
    use oc_adapters::config::McpEntry;
    use std::collections::BTreeMap;

    let base = |headers| McpEntry {
        kind: "remote".to_string(),
        url: Some("https://mcp.example.com/v1/mcp".to_string()),
        enabled: true,
        oauth: false,
        headers,
        command: Vec::new(),
        timeout: None,
        codemode: None,
    };
    for spelling in ["authorization", "Authorization", "aUtHoRiZaTiOn"] {
        let headers = [(spelling.to_string(), "Bearer key".to_string())]
            .into_iter()
            .collect();
        assert_eq!(
            CodexWebConfig::from_entry(&base(headers))
                .expect("authorization spelling")
                .bearer,
            "key"
        );
    }

    let same_values = [
        ("Authorization".to_string(), "Bearer key".to_string()),
        ("authorization".to_string(), "Bearer key".to_string()),
    ]
    .into_iter()
    .collect();
    CodexWebConfig::from_entry(&base(same_values)).expect("equivalent duplicates collapse");

    let conflicting_values: BTreeMap<_, _> = [
        ("Authorization".to_string(), "Bearer first".to_string()),
        ("authorization".to_string(), "Bearer second".to_string()),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        CodexWebConfig::from_entry(&base(conflicting_values)),
        Err(McpError::ConflictingHeader("authorization".to_string()))
    );
}

#[test]
fn entry_mapping_refuses_oauth_and_keeps_exact_url() {
    use oc_adapters::config::McpEntry;
    use std::collections::BTreeMap;

    let mut headers = BTreeMap::new();
    headers.insert("Authorization".to_string(), "Bearer live-key".to_string());
    headers.insert("X-Tenant".to_string(), "tenant-secret".to_string());
    headers.insert("Content-Type".to_string(), "text/plain".to_string());
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
    assert_eq!(
        config
            .custom_headers
            .get("x-tenant")
            .and_then(|value| value.to_str().ok()),
        Some("tenant-secret")
    );
    assert!(
        config.custom_headers.get("content-type").is_none(),
        "native content controls win"
    );
    assert!(
        !format!("{config:?}").contains("live-key"),
        "bearer redacted"
    );
    assert!(
        !format!("{config:?}").contains("tenant-secret"),
        "custom header values redacted"
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
        custom_headers: Default::default(),
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
        .search("oc smoke probe", Some("short"), &cancel)
        .await
        .expect("live search");
    assert!(!text.trim().is_empty(), "live search must return text");
}
