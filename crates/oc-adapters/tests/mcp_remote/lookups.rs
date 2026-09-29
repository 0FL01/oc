use super::*;
use oc_core::queries::{
    McpLookupData, McpLookupError, McpLookupOp, McpPromptRole, McpResourceContents,
};
use std::collections::BTreeMap;

pub(super) fn response(request: &Value, version: usize) -> Option<Value> {
    let params = &request["params"];
    Some(match request["method"].as_str()? {
        "prompts/list" if version == 1 => {
            json!({"prompts":(0..65).map(|i| json!({"name":format!("p{i}")})).collect::<Vec<_>>()})
        }
        "prompts/list" if params["cursor"].is_string() => json!({"prompts":[{"name":"second"}]}),
        "prompts/list" => {
            json!({"prompts":[{"name":"first","description":"test-key HEADER-CANARY"}],"nextCursor":"second"})
        }
        "resources/list" => {
            json!({"resources":[{"uri":"fixture://text","name":"text","mimeType":"text/plain"}]})
        }
        "resources/templates/list" => {
            json!({"resourceTemplates":[{"uriTemplate":"fixture://{name}","name":"template"}]})
        }
        "prompts/get" => {
            json!({"messages":[{"role":"user","content":{"type":"text","text":"First"}},
            {"role":"assistant","content":{"type":"text","text":"Second"}}]})
        }
        "resources/read" => {
            json!({"contents":[{"uri":params["uri"],"text":"test-key HEADER-CANARY","mimeType":"text/plain"},
            {"uri":"fixture://blob","blob":"AAEC/w==","mimeType":"application/octet-stream"}]})
        }
        _ => return None,
    })
}

fn config(url: String) -> CodexWebConfig {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-canary", "HEADER-CANARY".parse().unwrap());
    CodexWebConfig {
        url,
        bearer: "test-key".into(),
        custom_headers: headers,
        timeout: Duration::from_secs(2),
        startup_timeout: None,
        catalog_timeout: None,
        allow_private: true,
    }
}

#[tokio::test]
async fn mcp11_http_exact_lookup_methods_arguments_blobs_and_redaction_share_one_initialize() {
    let ((url, log), _) = Fake::start_full(Mode::Lookups, Duration::from_millis(600), false);
    let client = CodexWebClient::connect(&config(url)).await.unwrap();
    let cancel = AtomicBool::new(false);
    let data = client
        .lookup(&McpLookupOp::ListPrompts, &cancel)
        .await
        .unwrap();
    let McpLookupData::Prompts { items, .. } = &data else {
        panic!()
    };
    assert_eq!(
        items.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert!(!serde_json::to_string(&data).unwrap().contains("test-key"));
    assert!(
        !serde_json::to_string(&data)
            .unwrap()
            .contains("HEADER-CANARY")
    );
    client
        .lookup(&McpLookupOp::ListResources, &cancel)
        .await
        .unwrap();
    client
        .lookup(&McpLookupOp::ListResourceTemplates, &cancel)
        .await
        .unwrap();
    assert!(
        !log.lock()
            .unwrap()
            .iter()
            .any(|r| r.body.contains("resources/read") || r.body.contains("prompts/get"))
    );
    let data = client
        .lookup(
            &McpLookupOp::GetPrompt {
                name: "first".into(),
                arguments: BTreeMap::from([("topic".into(), "synthetic".into())]),
            },
            &cancel,
        )
        .await
        .unwrap();
    let McpLookupData::Prompt { messages, .. } = data else {
        panic!()
    };
    assert_eq!(
        messages.iter().map(|m| m.role).collect::<Vec<_>>(),
        [McpPromptRole::User, McpPromptRole::Assistant]
    );
    let data = client
        .lookup(
            &McpLookupOp::ReadResource {
                uri: "https://example.invalid/not-fetched".into(),
            },
            &cancel,
        )
        .await
        .unwrap();
    let McpLookupData::Resource { contents, .. } = &data else {
        panic!()
    };
    assert!(matches!(&contents[1], McpResourceContents::Blob { blob, .. } if blob == "AAEC/w=="));
    assert!(!serde_json::to_string(&data).unwrap().contains("test-key"));
    client.close().await.unwrap();
    let records = log.lock().unwrap();
    let requests = records
        .iter()
        .filter_map(|r| serde_json::from_str::<Value>(&r.body).ok())
        .collect::<Vec<_>>();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "initialize")
            .count(),
        1
    );
    let get = requests
        .iter()
        .find(|r| r["method"] == "prompts/get")
        .unwrap();
    assert_eq!(get["params"]["name"], "first");
    assert_eq!(get["params"]["arguments"], json!({"topic":"synthetic"}));
    assert!(
        get["params"]
            .as_object()
            .unwrap()
            .keys()
            .all(|key| matches!(key.as_str(), "name" | "arguments" | "_meta")),
        "only legacy arguments and SDK metadata"
    );
    assert!(records.iter().all(|r| r.path == "/v1/mcp"));
}

#[tokio::test]
async fn mcp11_http_catalog_cap_and_execution_deadline_use_request_id_cancellation() {
    let ((url, log), version) = Fake::start_full(Mode::Lookups, Duration::from_millis(600), false);
    let mut config = config(url);
    config.timeout = Duration::from_millis(100);
    config.startup_timeout = Some(Duration::from_secs(2));
    config.catalog_timeout = Some(Duration::from_secs(2));
    let client = CodexWebClient::connect(&config).await.unwrap();
    version.store(1, Ordering::SeqCst);
    assert_eq!(
        client
            .lookup(&McpLookupOp::ListPrompts, &AtomicBool::new(false))
            .await,
        Err(McpLookupError::CatalogLimit)
    );
    assert_eq!(
        client
            .lookup(
                &McpLookupOp::ReadResource {
                    uri: "fixture://held".into()
                },
                &AtomicBool::new(false)
            )
            .await,
        Err(McpLookupError::Deadline)
    );
    client.close().await.unwrap();
    let requests = log
        .lock()
        .unwrap()
        .iter()
        .filter_map(|r| serde_json::from_str::<Value>(&r.body).ok())
        .collect::<Vec<_>>();
    let read = requests
        .iter()
        .find(|r| r["method"] == "resources/read")
        .unwrap();
    assert!(requests.iter().any(
        |r| r["method"] == "notifications/cancelled" && r["params"]["requestId"] == read["id"]
    ));
    assert_eq!(
        requests
            .iter()
            .filter(|r| r["method"] == "resources/read")
            .count(),
        1
    );
}
