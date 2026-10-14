use super::*;
use oc_core::queries::{CustomProviderAuth, KeyInput, NativeProtocol};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn configless(root: &Path) -> std::path::PathBuf {
    let project = root.join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        r#"{"disabled_providers":["opencode-go","openai"]}"#,
    )
    .unwrap();
    project
}

async fn response(stream: &mut tokio::net::TcpStream, body: serde_json::Value) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    while !bytes.windows(4).any(|p| p == b"\r\n\r\n") {
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    let headers = String::from_utf8(bytes).unwrap();
    let body = body.to_string();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    headers
}

#[tokio::test]
async fn conn01_actor_probe_save_ack_single_use_restart_and_no_selection_or_epoch_change() {
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/literal", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let headers = response(&mut stream, serde_json::json!({"object":"list","data":[{"id":"exact/slash", "opencode":{"name":"Exact supplied name","variants":{"deep":{"reasoningEffort":"high"}}}}]})).await;
        assert!(headers.starts_with("GET /literal/models "));
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer key_actor_canary")
        );
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let before = app.catalog().await.unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        app.custom_provider(CustomProviderAction::Probe {
            input: CustomProviderInput {
                name: "New connection".into(),
                base_url: base,
                auth: CustomProviderAuth::Key(KeyInput::new("KEY_ACTOR_CANARY".into())),
            },
            cancel: Arc::new(AtomicBool::new(false)),
        }),
    )
    .await
    .unwrap()
    .unwrap();
    let CustomProviderReply::Probed {
        revision,
        model_count,
        suggested,
        diagnostic,
    } = result
    else {
        panic!("expected probe receipt");
    };
    assert_eq!(model_count, 1);
    assert_eq!(suggested, None);
    assert_eq!(diagnostic, None);
    let conn = rusqlite::Connection::open_with_flags(
        data.join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    for table in [
        "custom_provider_connections",
        "credential_accounts",
        "sessions",
        "turns",
        "tool_operations",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
    let reply = app
        .custom_provider(CustomProviderAction::Save {
            revision,
            protocol: NativeProtocol::Responses,
        })
        .await
        .unwrap();
    let CustomProviderReply::Saved(connection) = reply else {
        panic!("expected save ACK");
    };
    assert!(
        app.custom_provider(CustomProviderAction::Save {
            revision,
            protocol: NativeProtocol::Chat
        })
        .await
        .is_err()
    );
    let after = app.catalog().await.unwrap();
    assert_eq!(
        after.chrome.selection_generation,
        before.chrome.selection_generation
    );
    assert_eq!(
        after.chrome.terminal_generation,
        before.chrome.terminal_generation
    );
    assert_eq!(after.selected_model(), before.selected_model());
    assert_eq!(after.provider, before.provider);
    let catalog = app
        .provider_catalog(connection.provider.clone())
        .await
        .unwrap();
    assert_eq!(catalog.models.len(), 1);
    assert_eq!(catalog.models[0].id, "exact/slash");
    assert_eq!(catalog.models[0].display_name, "Exact supplied name");
    let accounts = app
        .provider_accounts(connection.provider.clone(), None)
        .await
        .unwrap();
    assert_eq!(accounts.accounts.len(), 1);
    assert_eq!(accounts.accounts[0].label, "default");
    assert!(!format!("{accounts:?} {catalog:?}").contains("CANARY"));
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(conn);
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let restored = app
        .provider_catalog(connection.provider.clone())
        .await
        .unwrap();
    assert_eq!(restored.models[0].id, "exact/slash");
    assert_eq!(
        app.provider_accounts(connection.provider, None)
            .await
            .unwrap()
            .accounts[0]
            .label,
        "default"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn conn05_cancel_held_probe_is_joined_and_late_save_cannot_create_rows() {
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (entered, seen) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        entered.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while stream.read(&mut bytes).await.unwrap() != 0 {}
        })
        .await
        .unwrap();
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let app = Arc::new(app);
    let cancel = Arc::new(AtomicBool::new(false));
    let task = {
        let app = app.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move {
            app.custom_provider(CustomProviderAction::Probe {
                input: CustomProviderInput {
                    name: "Cancelled".into(),
                    base_url: base,
                    auth: CustomProviderAuth::Anonymous,
                },
                cancel,
            })
            .await
        })
    };
    tokio::time::timeout(Duration::from_secs(3), seen)
        .await
        .unwrap()
        .unwrap();
    assert!(app.catalog().await.is_ok()); // HTTP does not occupy the actor
    cancel.store(true, Ordering::SeqCst);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert!(db.custom_providers().unwrap().is_empty());
}
