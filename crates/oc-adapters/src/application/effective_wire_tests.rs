//! Actual owner path: catalog identity stays local while each lane captures its wire/auth.
use super::*;
use crate::{auth::AuthScope, storage::CredentialMaterial};
use oc_core::queries::{ProviderStatus, ServiceCode, SessionSelectionAction as Action};
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

#[tokio::test]
async fn go03_model_variant_and_title_bindings_use_api_ids_without_foreign_credentials() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let wire_base = format!("{base}/model-prefix");
    let title_base = format!("{base}/title-prefix");
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let config = json!({"model":"fixture/catalog#fast", "agent":{"title":{"model":"fixture/title"}},
    "providers":{"fixture":{
        "package":"@ai-sdk/openai", "settings":{"baseURL":"https://example.com/parent"},
        "headers":{"X-Overlay":"provider"}, "body":{"nested":{"provider":1}},
        "models":{
            "catalog":{"modelID":"API_MESSAGES_ID", "package":"@ai-sdk/anthropic",
                "settings":{"baseURL":wire_base,"thinking":{"type":"adaptive"}},
                "headers":{"x-overlay":"model"}, "body":{"nested":{"model":2}},
                "limit":{"context":32768,"output":4096},
                "variants":[{"id":"fast","settings":{"reasoningEffort":"high"},
                    "headers":{"X-OVERLAY":"variant"},"body":{"nested":{"variant":3}}},
                    {"id":"blocked","settings":{"authPolicy":"oauth"}}]},
            "title":{"modelID":"API_TITLE_ID", "package":"@ai-sdk/openai-compatible",
                "settings":{"baseURL":title_base,"apiKey":"TITLE_KEY_CANARY"},
                "limit":{"context":32768,"output":4096}},
            "unbound":{"settings":{"baseURL":format!("{base}/unbound")},
                "limit":{"context":32768,"output":4096}}
        }
    }}});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        for (url, key) in [
            ("https://example.com/parent", "PARENT_KEY_CANARY"),
            (wire_base.as_str(), "MODEL_KEY_CANARY"),
        ] {
            let scope = AuthScope::admit("fixture", url).unwrap();
            db.add_credential(
                scope.namespace(),
                "synthetic",
                CredentialMaterial::Key { key: key.into() },
            )
            .unwrap();
        }
    }
    let peer = tokio::spawn(async move {
        for title in [false, true] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let (head, length) = loop {
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0 && bytes.len() < 1_048_576);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(head) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                    let length = String::from_utf8_lossy(&bytes[..head])
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|n| n.parse::<usize>().ok())
                        })
                        .unwrap();
                    break (head + 4, length);
                }
            };
            while bytes.len() < head + length {
                let n = socket.read(&mut chunk).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            let headers = String::from_utf8_lossy(&bytes[..head]).to_ascii_lowercase();
            let body: serde_json::Value =
                serde_json::from_slice(&bytes[head..head + length]).unwrap();
            assert!(!headers.contains("parent_key_canary") && !headers.contains("go_env_canary"));
            let response = if title {
                assert!(headers.starts_with("post /title-prefix/chat/completions "));
                assert!(headers.contains("authorization: bearer title_key_canary"));
                assert!(!headers.contains("model_key_canary"));
                assert_eq!(body["model"], "API_TITLE_ID");
                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    json!({"choices":[{"index":0,"delta":{"content":"Generated title"},"finish_reason":"stop"}]})
                )
            } else {
                assert!(headers.starts_with("post /model-prefix/messages "));
                assert!(headers.contains("x-api-key: model_key_canary"));
                assert!(!headers.contains("title_key_canary"));
                assert!(headers.contains("x-overlay: variant"));
                assert_eq!(body["model"], "API_MESSAGES_ID");
                assert_eq!(body["thinking"], json!({"type":"adaptive"}));
                assert_eq!(body["output_config"]["effort"], "high");
                assert_eq!(body["nested"], json!({"provider":1,"model":2,"variant":3}));
                [json!({"type":"message_start","message":{"role":"assistant","usage":{"input_tokens":10}}}),
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"MODEL_BINDING_OK"}}),
                    json!({"type":"content_block_stop","index":0}),
                    json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}}),
                    json!({"type":"message_stop"})].iter().map(|v|format!("data: {v}\n\n")).collect()
            };
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).as_bytes()).await.unwrap();
        }
    });
    let (app, guard, _) = spawn_with_env(
        &project,
        &data,
        BTreeMap::from([("OPENCODE_API_KEY".into(), "GO_ENV_CANARY".into())]),
    )
    .await
    .unwrap();
    let initial = app.catalog().await.unwrap();
    assert_eq!(initial.model_id, "catalog");
    assert_eq!(initial.variant.as_deref(), Some("fast"));
    assert_eq!(
        initial.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Ready
    );
    for (model, variant, code) in [
        ("unbound", None, ServiceCode::MissingCredential),
        (
            "catalog",
            Some("blocked"),
            ServiceCode::UnsupportedCapability,
        ),
    ] {
        app.home_selection(Action::Model(model.into()))
            .await
            .unwrap();
        let snapshot = app
            .home_selection(Action::Variant(variant.map(str::to_owned)))
            .await
            .unwrap();
        assert_eq!(
            snapshot.chrome.provider.unwrap().diagnostic.unwrap().code,
            code
        );
        let fresh = SessionId::new(format!("refused-{model}")).unwrap();
        assert!(
            matches!(app.submit_fresh(fresh.clone(),"must not accept".into(),None).await,Err(CoreError::ProviderUnavailable(ref d)) if d.code==code)
        );
        assert_eq!(
            app.probe_session(fresh).await.unwrap(),
            oc_core::queries::SessionProbe::Absent
        );
    }
    app.home_selection(Action::Model("catalog".into()))
        .await
        .unwrap();
    app.home_selection(Action::Variant(Some("fast".into())))
        .await
        .unwrap();
    let session = SessionId::new("effective-binding").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Fixed title".into())
        .await
        .unwrap();
    app.submit(session.clone(), "test model binding".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app
                .history_page(session.clone(), None, None, 20)
                .await
                .unwrap()
                .rows
                .iter()
                .any(|r| {
                    r.role == oc_core::session::Role::Assistant
                        && r.text.contains("MODEL_BINDING_OK")
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    app.regenerate_title(session).await.unwrap();
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
