//! Strict stateless OpenProxy 2026-07-28 wire contract, without live effects.
use super::*;

pub(super) fn route(
    mode: Mode,
    slow: Duration,
    method: &str,
    headers: &HashMap<String, String>,
    body: &str,
) -> (u16, Vec<u8>, Option<&'static str>) {
    let request: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let id = &request["id"];
    let rpc_method = request["method"].as_str().unwrap_or_default();
    let error = |status, code, message| {
        (
            status,
            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
                .to_string()
                .into_bytes(),
            Some("application/json"),
        )
    };
    if method != "POST" {
        return error(405, -32600, "POST required");
    }
    if headers.get("authorization").map(String::as_str) != Some("Bearer test-key") {
        return error(401, -32600, "Unauthorized");
    }
    let params = &request["params"];
    let meta = &params["_meta"];
    if !(id.is_string() || id.is_i64() || id.is_u64())
        || request["jsonrpc"] != "2.0"
        || meta["io.modelcontextprotocol/protocolVersion"] != "2026-07-28"
        || !meta["io.modelcontextprotocol/clientCapabilities"].is_object()
    {
        return error(
            400,
            -32602,
            "Required request metadata is missing or invalid",
        );
    }
    if headers.get("mcp-protocol-version").map(String::as_str) != Some("2026-07-28")
        || headers.get("mcp-method").map(String::as_str) != Some(rpc_method)
        || (rpc_method == "tools/call"
            && headers.get("mcp-name").map(String::as_str) != params["name"].as_str())
        || headers.contains_key("mcp-session-id")
        || headers.contains_key("last-event-id")
    {
        return error(400, -32020, "HeaderMismatch");
    }
    let mut result = match rpc_method {
        "server/discover" => json!({
            "supportedVersions": if mode == Mode::ModernVersionMismatch {
                json!(["2025-11-25"])
            } else {
                json!(["2026-07-28"])
            },
            "capabilities":{"tools":{}}, "ttlMs":0, "cacheScope":"private",
            "_meta":{"io.modelcontextprotocol/serverInfo":{"name":"openproxy-fixture","version":"1"}}
        }),
        "tools/list" => json!({
            "tools":[tool("search", "Search"), tool("generate_image", "Generate"), tool("edit_image", "Edit")],
            "ttlMs":0,"cacheScope":"private"
        }),
        "tools/call" => {
            let query = params["arguments"]["query"].as_str().unwrap_or_default();
            if query == "rpc-error" {
                return error(400, -32602, "Invalid arguments");
            }
            if query == "slow" {
                std::thread::sleep(slow);
            }
            json!({"content":[{"type":"text","text":format!("result for {query}")}], "isError":query == "boom"})
        }
        _ => return error(404, -32601, "Method not found"),
    };
    result["resultType"] = "complete".into();
    (
        200,
        json!({"jsonrpc":"2.0","id":id,"result":result})
            .to_string()
            .into_bytes(),
        Some("application/json"),
    )
}

#[tokio::test]
async fn explicit_discovery_lists_and_calls_with_self_contained_metadata() {
    let (url, records) = Fake::start(Mode::Modern, Duration::ZERO);
    let mut config = config_for(&format!("{url}?exact=fixture"));
    config.protocol = "2026-07-28".into();
    // A generic server uses the same protocol selection, not a codex_web name route.
    let client = CodexWebClient::connect_remote(&config).await.unwrap();
    let cancel = AtomicBool::new(false);
    let tools = client.list_tools(&cancel).await.unwrap();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        ["search", "generate_image", "edit_image"]
    );
    assert_eq!(
        client.search("rust", None, &cancel).await.unwrap(),
        "result for rust"
    );
    assert!(matches!(
        client.search("boom", None, &cancel).await,
        Err(McpError::ToolFailed(None, text)) if text == "result for boom"
    ));
    assert!(matches!(
        client.search("rpc-error", None, &cancel).await,
        Err(McpError::ToolFailed(
            Some(oc_adapters::mcp_result::FailureDetail::InvalidArguments),
            _
        ))
    ));
    client.close().await.unwrap();
    let seen = records_of(&records);
    let methods: Vec<_> = seen
        .iter()
        .map(|record| {
            assert_eq!(record.path, "/v1/mcp?exact=fixture");
            serde_json::from_str::<Value>(&record.body).unwrap()["method"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(
        methods,
        [
            "server/discover",
            "tools/list",
            "tools/call",
            "tools/call",
            "tools/call"
        ]
    );
}

#[tokio::test]
async fn pinned_discovery_rejects_mismatch_without_legacy_fallback() {
    let (url, records) = Fake::start(Mode::ModernVersionMismatch, Duration::ZERO);
    let mut config = config_for(&url);
    config.protocol = "2026-07-28".into();
    assert!(matches!(
        CodexWebClient::connect(&config).await,
        Err(McpError::ProtocolMismatch)
    ));
    let seen = records_of(&records);
    assert_eq!(seen.len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(&seen[0].body).unwrap()["method"],
        "server/discover"
    );
}

#[tokio::test]
async fn modern_cancel_closes_http_without_wire_notification_or_replay() {
    let (url, records) = Fake::start(Mode::Modern, Duration::from_millis(600));
    let mut config = config_for(&url);
    config.protocol = "2026-07-28".into();
    let client = CodexWebClient::connect(&config).await.unwrap();
    let cancel = AtomicBool::new(false);
    let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(client.search("slow", None, &cancel), async {
            while !records_of(&records)
                .iter()
                .any(|record| record.body.contains("\"method\":\"tools/call\""))
            {
                tokio::task::yield_now().await;
            }
            cancel.store(true, Ordering::Relaxed);
        })
    })
    .await
    .unwrap();
    assert_eq!(result, Err(McpError::Cancelled));
    client.close().await.unwrap();
    let methods: Vec<_> = records_of(&records)
        .iter()
        .map(|record| {
            serde_json::from_str::<Value>(&record.body).unwrap()["method"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(methods, ["server/discover", "tools/call"]);
}
