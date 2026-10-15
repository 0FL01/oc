use super::*;
use oc_core::queries::{CustomProviderAuth, KeyInput, NativeProtocol};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn configless(root: &Path) -> std::path::PathBuf {
    let project = root.join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        r#"{"experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}]}}"#,
    )
    .unwrap();
    project
}

#[tokio::test]
async fn conn05_location_switch_joins_old_saved_refresh_and_attaches_scoped_key_before_selection() {
    use crate::storage::provider_connections::CatalogState;
    use oc_core::queries::{AccountAuthSource, ProviderStatus};
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let target = root.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/switch-prefix", listener.local_addr().unwrap());
    let row = {
        let db = Db::open(&data).unwrap();
        let row = db
            .create_custom_provider(
                &CustomProviderInput {
                    name: "Saved across Locations".into(),
                    base_url: base,
                    auth: CustomProviderAuth::Key(KeyInput::new("LOCATION_KEY_CANARY".into())),
                },
                NativeProtocol::Chat,
                |_| true,
            )
            .unwrap();
        db.save_custom_catalog(
            &row.id,
            row.revision,
            &BTreeMap::from([(
                "saved/exact".into(),
                serde_json::json!({"name":"Saved before refresh"}),
            )]),
            CatalogState::Ready,
        )
        .unwrap();
        row
    };
    std::fs::write(
        target.join("opencode.json"),
        serde_json::json!({
            "model":format!("{}/saved/exact", row.id),
            "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}]}
        })
        .to_string(),
    )
    .unwrap();
    let (arrived, held) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).await.unwrap();
        let headers = String::from_utf8_lossy(&buffer[..count]).to_ascii_lowercase();
        assert!(headers.starts_with("get /switch-prefix/models "));
        assert!(headers.contains("authorization: bearer location_key_canary"));
        arrived.send(()).unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buffer))
                .await
                .unwrap()
                .unwrap(),
            0
        );
        listener
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let initial = app.catalog().await.unwrap();
    app.custom_provider(CustomProviderAction::Refresh {
        provider: row.id.clone(),
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(3), held)
        .await
        .unwrap()
        .unwrap();
    // The saved provider is not the source selection, and its held HTTP does not own the actor.
    assert_eq!(
        app.catalog().await.unwrap().selected_model(),
        initial.selected_model()
    );
    let switched = app
        .switch_location_home(target.to_string_lossy().into_owned())
        .await
        .unwrap();
    let listener = peer.await.unwrap();
    assert_eq!(switched.catalog.provider, row.id);
    assert_eq!(switched.catalog.model_id, "saved/exact");
    assert_eq!(
        switched.catalog.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Ready
    );
    assert_eq!(
        switched
            .catalog
            .chrome
            .provider
            .as_ref()
            .unwrap()
            .catalog_status,
        ProviderStatus::Ready
    );
    assert_eq!(
        switched.catalog.models[0].display_name,
        "Saved before refresh"
    );
    assert_eq!(
        app.provider_accounts(row.id.clone(), None)
            .await
            .unwrap()
            .effective,
        AccountAuthSource::Stored
    );
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = response(&mut stream, serde_json::json!({"object":"list","data":[]})).await;
        assert!(request.starts_with("GET /switch-prefix/models "));
    });
    app.custom_provider(CustomProviderAction::Refresh {
        provider: row.id.clone(),
    })
    .await
    .unwrap();
    let retired = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let snapshot = app.catalog().await.unwrap();
            if snapshot.chrome.provider.as_ref().unwrap().catalog_status == ProviderStatus::Ready {
                break snapshot;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    peer.await.unwrap();
    assert!(retired.models.is_empty());
    assert_eq!(
        retired.model_id,
        selection_identity("model", "saved/exact"),
        "retirement retains the exact choice as its existing safe unavailable identity"
    );
    assert!(retired.chrome.selection.is_some());
    assert_eq!(
        retired.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Unavailable
    );
    let restored = app
        .switch_location_home(project.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert_eq!(restored.catalog.selected_model(), initial.selected_model());
    assert!(
        app.provider_catalog(row.id.clone())
            .await
            .unwrap()
            .models
            .is_empty()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let saved = Db::open(&data)
        .unwrap()
        .custom_providers()
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.catalog_revision, Some(1));
    assert!(saved.models.is_empty());
    assert_eq!(saved.state, CatalogState::Ready);
}

#[tokio::test]
async fn conn05_invalid_replacement_joins_old_probe_without_storage_effects() {
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/held", listener.local_addr().unwrap());
    let (held, accepted) = tokio::sync::oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 4096];
        let read = stream.read(&mut buffer).await.unwrap();
        assert!(read > 0);
        held.send(()).unwrap();
        let read = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buffer))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read, 0, "replacement must abort and join the old GET");
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let old = app.clone();
    let pending = tokio::spawn(async move {
        old.custom_provider(CustomProviderAction::Probe {
            input: CustomProviderInput {
                name: "Old form".into(),
                base_url: base,
                auth: CustomProviderAuth::Key(KeyInput::new("EPHEMERAL_OLD_KEY".into())),
            },
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .await
    });
    tokio::time::timeout(Duration::from_secs(3), accepted)
        .await
        .unwrap()
        .unwrap();
    let refused = app
        .custom_provider(CustomProviderAction::Probe {
            input: CustomProviderInput {
                name: String::new(),
                base_url: "not a URL".into(),
                auth: CustomProviderAuth::Anonymous,
            },
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .await;
    assert!(refused.is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(3), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        Db::open(&data)
            .unwrap()
            .custom_providers()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn conn05_failed_probe_join_retires_captured_input_and_ack() {
    let (ack, reply) = tokio::sync::oneshot::channel();
    let mut work = Work {
        task: Some(tokio::spawn(async { panic!("fixture probe task failure") })),
        capture: Some(Capture::Probe {
            revision: 1,
            input: CustomProviderInput {
                name: "Ephemeral".into(),
                base_url: "http://127.0.0.1/held".into(),
                auth: CustomProviderAuth::Key(KeyInput::new("EPHEMERAL_KEY".into())),
            },
            cancel: Arc::new(AtomicBool::new(false)),
            result: None,
            ack: Some(ack),
        }),
    };
    tokio::time::timeout(Duration::from_secs(1), async {
        while !work.task.as_ref().unwrap().is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(work.stop().await.is_err());
    assert!(!work.pending() && !work.has_probe());
    assert!(reply.await.unwrap().is_err());
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
    assert!(
        app.custom_provider(CustomProviderAction::Save {
            revision: u64::MAX,
            protocol: NativeProtocol::Responses,
        })
        .await
        .is_err()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert!(db.custom_providers().unwrap().is_empty());
}

#[tokio::test]
async fn conn04_retry_reuses_owned_input_and_expires_the_old_form_receipt() {
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/retry-prefix", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = response(
                &mut stream,
                serde_json::json!({"object":"list","data":[{"id":"retry-exact"}]}),
            )
            .await;
            assert!(request.starts_with("GET /retry-prefix/models "));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer retry_key_canary")
            );
        }
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let CustomProviderReply::Probed {
        revision: first, ..
    } = app
        .custom_provider(CustomProviderAction::Probe {
            input: CustomProviderInput {
                name: "Retry".into(),
                base_url: base,
                auth: CustomProviderAuth::Key(KeyInput::new("RETRY_KEY_CANARY".into())),
            },
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .await
        .unwrap()
    else {
        panic!("not probe")
    };
    let CustomProviderReply::Probed {
        revision: second, ..
    } = app
        .custom_provider(CustomProviderAction::Retry { revision: first })
        .await
        .unwrap()
    else {
        panic!("not retry")
    };
    assert_ne!(first, second);
    assert!(
        app.custom_provider(CustomProviderAction::Save {
            revision: first,
            protocol: NativeProtocol::Responses
        })
        .await
        .is_err()
    );
    assert_eq!(
        app.custom_provider(CustomProviderAction::Cancel { revision: first })
            .await
            .unwrap(),
        CustomProviderReply::Cancelled
    );
    let CustomProviderReply::Saved(saved) = app
        .custom_provider(CustomProviderAction::Save {
            revision: second,
            protocol: NativeProtocol::Chat,
        })
        .await
        .unwrap()
    else {
        panic!("not save")
    };
    assert_eq!(saved.source, oc_core::queries::AccountAuthSource::Stored);
    assert_eq!(
        app.provider_catalog(saved.provider.clone())
            .await
            .unwrap()
            .models[0]
            .id,
        "retry-exact"
    );
    assert_eq!(
        app.provider_accounts(saved.provider, None)
            .await
            .unwrap()
            .accounts[0]
            .label,
        "default"
    );
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert_eq!(db.custom_providers().unwrap().len(), 1);
}

#[tokio::test]
async fn conn03_anonymous_catalog_projects_nullable_partial_full_and_effort_metadata() {
    let root = tempfile::tempdir().unwrap();
    let project = configless(root.path());
    let data = root.path().join("data");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/catalog", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let request = response(&mut stream, serde_json::json!({"has_more":false,"data":[
            {"id":"unknown", "capabilities":null,"max_input_tokens":null,"max_tokens":null},
            {"id":"empty", "opencode":{"modalities":{"input":[],"output":[]},"reasoning":false,"tool_call":false,"variants":{}}},
            {"id":"partial", "max_input_tokens":12000,"max_tokens":3000,"capabilities":{
                "image_input":{"supported":false},"pdf_input":{"supported":true},"thinking":{"supported":true},
                "effort":{"supported":true,"low":{"supported":true},"medium":{"supported":false},"max":{"supported":true}}
            }},
            {"id":"full", "opencode":{"modalities":{"input":["text","image"],"output":["text"]},"variants":{"precise":{"reasoningEffort":"supplied-exact"}}},
                "capabilities":{"image_input":{"supported":false}}}
        ]})).await;
        assert!(request.starts_with("GET /catalog/models "));
        let request = request.to_ascii_lowercase();
        assert!(!request.contains("authorization:") && !request.contains("x-api-key:"));
    });
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let CustomProviderReply::Probed {
        revision,
        suggested,
        ..
    } = app
        .custom_provider(CustomProviderAction::Probe {
            input: CustomProviderInput {
                name: "Anonymous metadata".into(),
                base_url: base,
                auth: CustomProviderAuth::Anonymous,
            },
            cancel: Arc::new(AtomicBool::new(false)),
        })
        .await
        .unwrap()
    else {
        panic!("probe receipt")
    };
    assert_eq!(suggested, Some(NativeProtocol::Messages));
    let CustomProviderReply::Saved(connection) = app
        .custom_provider(CustomProviderAction::Save {
            revision,
            protocol: NativeProtocol::Messages,
        })
        .await
        .unwrap()
    else {
        panic!("save receipt")
    };
    let catalog = app
        .provider_catalog(connection.provider.clone())
        .await
        .unwrap();
    let entry = |id| catalog.models.iter().find(|m| m.id == id).unwrap();
    assert!(!entry("unknown").variants_known);
    assert_eq!(entry("unknown").capabilities, Default::default());
    assert!(!entry("unknown").context_known && !entry("unknown").output_known);
    let empty = entry("empty");
    assert!(empty.variants_known && empty.variants.iter().all(|v| v.disabled));
    assert_eq!(empty.capabilities.input, Some(Vec::new()));
    assert_eq!(empty.capabilities.output, Some(Vec::new()));
    assert_eq!(empty.capabilities.reasoning, Some(false));
    assert_eq!(empty.capabilities.tools, Some(false));
    let partial = entry("partial");
    assert_eq!(partial.capabilities.input, None);
    assert_eq!(partial.capabilities.output, None);
    assert_eq!(
        partial.capabilities.partial,
        oc_core::queries::PartialModelCapabilities {
            image_input: Some(false),
            pdf_input: Some(true),
            reasoning: Some(true),
        }
    );
    assert_eq!(
        partial
            .variants
            .iter()
            .filter(|v| !v.disabled)
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["low", "max"]
    );
    assert_eq!(partial.output, 3000);
    assert!(partial.output_known && !partial.context_known);
    assert!(entry("full").capabilities.supports_image());
    assert_eq!(
        entry("full")
            .variants
            .iter()
            .find(|v| v.name == "precise")
            .unwrap()
            .reasoning_effort
            .as_deref(),
        Some("supplied-exact")
    );
    assert!(
        app.provider_accounts(connection.provider, None)
            .await
            .unwrap()
            .accounts
            .is_empty()
    );
    assert!(app.catalog().await.unwrap().selected_model().is_none());
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
