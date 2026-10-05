//! UI07: existing application owner remains local while provider admission fails.
use super::*;
use oc_core::queries::{ProviderStatus, ServiceAction, ServiceCode, ServiceKind, ServiceStage};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn go01_auth_policy_conflicts_and_go_override_fail_before_publication() {
    let root = tempfile::tempdir().unwrap();
    for options in [
        json!({"baseURL":"https://example.com/v1", "authPolicy":"none", "apiKey":"{env:MISSING}"}),
        json!({"baseURL":"https://example.com/v1", "authPolicy":"none", "headers":{"Authorization":"secret"}}),
        json!({"baseURL":"https://foreign.invalid/v1", "apiKey":"{env:OPENCODE_API_KEY}"}),
    ] {
        let go = options["baseURL"] == "https://foreign.invalid/v1";
        let id = if go { "opencode-go" } else { "fixture" };
        std::fs::write(
            root.path().join("opencode.json"),
            json!({
                "model":format!("{id}/m"), "provider":{id:{"options":options, "models":{"m":{}}}}
            })
            .to_string(),
        )
        .unwrap();
        let error = composition::load_local_with_env(
            root.path(),
            BTreeMap::from([(
                "OPENCODE_API_KEY".into(),
                "GO01_ENV_SECRET_DO_NOT_SHOW".into(),
            )]),
        )
        .await
        .err()
        .expect("invalid binding refused locally");
        assert!(!format!("{error:?}").contains("GO01_ENV_SECRET_DO_NOT_SHOW"));
    }
    std::fs::write(root.path().join("opencode.json"), json!({
        "model":"fixture/m", "provider":{"fixture":{
            "options":{"baseURL":"https://example.com/v1", "authPolicy":"oauth"},"models":{"m":{}}
        }}
    }).to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(root.path(), &root.path().join("data"), BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.catalog()
            .await
            .unwrap()
            .chrome
            .provider
            .unwrap()
            .diagnostic
            .unwrap()
            .code,
        ServiceCode::UnsupportedCapability
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn go01_application_resolves_scoped_accounts_on_restart_and_reload() {
    use crate::{auth::AuthScope, storage::CredentialMaterial};
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let mut document = json!({"model":"fixture/m", "provider":{"fixture":{
        "options":{"baseURL":"https://example.com/v1"},
        "models":{"m":{"limit":{"context":32768,"output":4096}}}
    }}});
    let path = project.join("opencode.json");
    std::fs::write(&path, document.to_string()).unwrap();
    let namespace = AuthScope::admit("fixture", "https://example.com/v1/").unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.add_credential(
            namespace.namespace(),
            "stored",
            CredentialMaterial::Key {
                key: "GO01_STORED_SECRET_7b2".into(),
            },
        )
        .unwrap();
    }
    for restart in 0..2 {
        let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
            .await
            .unwrap();
        let catalog = app.catalog().await.unwrap();
        assert_eq!(
            catalog.chrome.provider.as_ref().unwrap().status,
            ProviderStatus::Ready
        );
        assert!(!format!("{catalog:?}").contains("GO01_STORED_SECRET_7b2"));
        if restart == 1 {
            document["provider"]["fixture"]["options"]["baseURL"] =
                "https://example.com/new-prefix".into();
            std::fs::write(&path, document.to_string()).unwrap();
            let changed = app.reload_location().await.unwrap();
            assert_eq!(
                changed
                    .catalog
                    .chrome
                    .provider
                    .as_ref()
                    .unwrap()
                    .diagnostic
                    .as_ref()
                    .unwrap()
                    .code,
                ServiceCode::MissingCredential
            );
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    }
    document["provider"]["fixture"]["options"]["baseURL"] = "https://example.com/v1".into();
    std::fs::write(&path, document.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.add_credential(
            namespace.namespace(),
            "OAuth stored",
            CredentialMaterial::OAuth {
                access: "GO01_OAUTH_SECRET_82c".into(),
                refresh: None,
                expires_at: None,
            },
        )
        .unwrap();
    }
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let catalog = app.catalog().await.unwrap();
    let readiness = catalog.chrome.provider.as_ref().unwrap();
    assert_eq!(readiness.status, ProviderStatus::Unavailable);
    assert_eq!(
        readiness.diagnostic.as_ref().unwrap().code,
        ServiceCode::UnsupportedCapability
    );
    let session = SessionId::new("unsupported-oauth").unwrap();
    app.create_session(session.clone()).await.unwrap();
    assert!(
        matches!(app.submit(session.clone(), "do not accept".into()).await,
        Err(CoreError::ProviderUnavailable(ref d)) if d.code == ServiceCode::UnsupportedCapability)
    );
    assert!(
        app.history_page(session, None, None, 20)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn go01_explicit_anonymous_application_sends_no_auth_and_completes() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    std::fs::write(
        project.join("opencode.json"),
        json!({
            "model":"fixture/m", "provider":{"fixture":{
                "options":{"baseURL":format!("http://{address}/v1"), "authPolicy":"none"},
                "models":{"m":{"limit":{"context":32768,"output":4096}}}
            }}
        })
        .to_string(),
    )
    .unwrap();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0; 4096];
            let count = socket.read(&mut chunk).await.unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&chunk[..count]);
            if let Some(offset) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                break offset + 4;
            }
        };
        let headers = String::from_utf8(request[..header_end].to_vec())
            .unwrap()
            .to_lowercase();
        assert!(headers.starts_with("post /v1/responses "));
        assert!(!headers.contains("authorization:"));
        assert!(!headers.contains("x-api-key:"));
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        while request.len() < header_end + length {
            let mut chunk = [0; 4096];
            let count = socket.read(&mut chunk).await.unwrap();
            assert_ne!(count, 0);
            request.extend_from_slice(&chunk[..count]);
        }
        let event = json!({"type":"response.completed","response":{"id":"anon", "status":"completed", "output":[{
            "type":"message","id":"anon-message","role":"assistant","content":[{"type":"output_text","text":"ANONYMOUS_OK"}]
        }]}});
        let body = format!("data: {event}\n\n");
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let (app, guard, _) = spawn_with_env(
        &project,
        &root.path().join("data"),
        BTreeMap::from([("OC_TEST_ALLOW_LOOPBACK".into(), "1".into())]),
    )
    .await
    .unwrap();
    assert_eq!(
        app.catalog()
            .await
            .unwrap()
            .chrome
            .provider
            .as_ref()
            .unwrap()
            .status,
        ProviderStatus::Ready
    );
    let session = SessionId::new("anonymous-session").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Explicit title".into())
        .await
        .unwrap();
    app.submit(session.clone(), "test anonymous".into())
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
                .any(|row| {
                    row.role == oc_core::session::Role::Assistant
                        && row.text.contains("ANONYMOUS_OK")
                })
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    peer.await.unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn ui07_missing_selected_credential_keeps_local_owner_and_refuses_before_acceptance() {
    let root = tempfile::tempdir().unwrap();
    const CANARY: &str = "UI07_AUTH_CONFIG_PATH_SECRET_724f";
    let global = root.path().join(CANARY);
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut config = json!({
        "model": "fixture/m", "provider": {"fixture": {
            "options": {"baseURL": format!("http://{}/v1", listener.local_addr().unwrap()), "apiKey": "{env:UI07_MISSING}", "headers": {"x-ui07": CANARY}},
            "models": {"m": {"name": "Configured model", "limit": {"context": 32768, "output": 4096}}}
        }}
    });
    std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([(
        "OPENCODE_CONFIG_DIR".into(),
        global.to_string_lossy().into_owned(),
    )]);
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    let catalog = app.catalog().await.unwrap();
    assert_eq!(catalog.model_id, "m");
    assert_eq!(catalog.models.len(), 1);
    let readiness = catalog.chrome.provider.as_ref().unwrap();
    assert_eq!(readiness.status, ProviderStatus::Unavailable);
    let diagnostic = readiness.diagnostic.as_ref().unwrap();
    assert_eq!(diagnostic.kind, ServiceKind::Provider);
    assert_eq!(diagnostic.code, ServiceCode::MissingCredential);
    assert_eq!(diagnostic.stage, ServiceStage::Config);
    assert_eq!(diagnostic.action, ServiceAction::ReviewConfiguration);
    assert!(diagnostic.source.starts_with("source-"));
    assert!(!format!("{readiness:?} {diagnostic}").contains(CANARY));
    let session = SessionId::new("ui07-refused").unwrap();
    app.create_session(session.clone()).await.unwrap();
    assert!(
        matches!(app.submit(session.clone(), "must not be accepted".into()).await,
        Err(CoreError::ProviderUnavailable(ref diagnostic)) if diagnostic.code == ServiceCode::MissingCredential)
    );
    assert!(
        app.history_page(session.clone(), None, None, 20)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    let compaction = app.compact_session(session.clone()).await;
    let compaction_history = app.compaction_history(session).await.unwrap();
    config["provider"]["fixture"]["options"]["apiKey"] = CANARY.into();
    std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
    let reloaded = app.reload_location().await.unwrap();
    assert!(reloaded.generation > 0);
    assert_eq!(
        reloaded.catalog.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Ready
    );
    assert_eq!(
        catalog.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Unavailable
    );
    assert!(!format!("{:?}", reloaded.catalog.chrome.provider).contains(CANARY));
    // Empty credentials cannot turn malformed mandatory transport syntax into
    // an optional-service failure or partially publish a new generation.
    config["provider"]["fixture"]["options"]["apiKey"] = "{env:UI07_MISSING}".into();
    config["provider"]["fixture"]["options"]["headers"]["x-ui07"] =
        format!("{CANARY}\r\nnot-a-header").into();
    std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
    assert!(matches!(
        app.reload_location().await,
        Err(CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            ..
        })
    ));
    assert_eq!(app.catalog().await.unwrap(), reloaded.catalog);
    assert_eq!(
        app.file_suggestions("".into(), 1).await.unwrap().generation,
        reloaded.generation
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        matches!(compaction, Err(CoreError::ProviderUnavailable(ref diagnostic)) if diagnostic.code == ServiceCode::MissingCredential),
        "{compaction:?}"
    );
    assert!(compaction_history.is_empty());
    assert!(
        matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}

#[tokio::test]
async fn ui07_pending_profile_selection_is_live_and_successful_catalog_retires_local_override() {
    use oc_core::queries::SessionSelectionAction as Action;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    const DYNAMIC: &str = "ui07-new-runtime-id-73";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (started, seen) = oneshot::channel();
    let (release, hold) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let mut chunk = [0; 1024];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0 && bytes.len() < 65_536);
            bytes.extend_from_slice(&chunk[..count]);
        }
        assert!(bytes.starts_with(b"GET /v1/models HTTP/1.1\r\n"));
        started.send(()).unwrap();
        hold.await.unwrap();
        let body = json!({"object":"list", "data":[{"id":DYNAMIC, "opencode":{
            "name":"UI07 dynamic", "limit":{"context":32768,"output":4096}
        }}]})
        .to_string();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let global = root.path().join("global");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&global).unwrap();
    let config = json!({"model":format!("ludka2/{DYNAMIC}"), "default_agent":"ui07-primary",
        "agent":{"ui07-primary":{"mode":"primary", "model":format!("ludka2/{DYNAMIC}"), "variant":"fast", "prompt":"UI07 profile body"}},
        "provider":{"ludka2":{"options":{"baseURL":format!("http://{addr}/v1"),"apiKey":"fixture"},
            "models":{"retired-local":{"name":"Retired local override","limit":{"context":32768,"output":4096}}}}}});
    std::fs::write(global.join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        (
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _, _) = spawn_inner(&project, &data, env, true)
        .await
        .unwrap_or_else(|issue| panic!("{:?}", issue.category));
    let mut events = app.subscribe();
    seen.await.unwrap();
    let initial = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(initial.model_id, DYNAMIC);
    assert_eq!(initial.agent_id.as_deref(), Some("ui07-primary"));
    assert!(
        initial.chrome.selection.is_none(),
        "a variant on a model pending discovery is not yet a disabled variant"
    );
    assert_eq!(
        initial.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Pending
    );
    assert_eq!(initial.models[0].id, "retired-local");
    assert!(
        matches!(app.reload_location().await, Err(CoreError::ProviderUnavailable(ref diagnostic)) if diagnostic.code == ServiceCode::ProviderPending)
    );
    let selected = app
        .home_selection(Action::Model("retired-local".into()))
        .await
        .unwrap();
    let fresh = SessionId::new("ui07-pending-auth").unwrap();
    let refused = app
        .submit_fresh(fresh.clone(), "no pending generation".into(), None)
        .await;
    assert!(
        matches!(refused,
        Err(CoreError::ProviderUnavailable(ref diagnostic)) if diagnostic.code == ServiceCode::ProviderPending),
        "pending authentication/catalog must precede request acceptance: {refused:?}"
    );
    assert_eq!(
        app.probe_session(fresh).await.unwrap(),
        oc_core::queries::SessionProbe::Absent
    );
    assert_eq!(
        selected.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Pending
    );
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::ProviderChanged) {
                break;
            }
        }
    })
    .await
    .unwrap();
    peer.await.unwrap();
    let completed = app.home_selection(Action::Current).await.unwrap();
    // The owner still holds the exact selected draft; successful discovery
    // retired only the metadata. The public unavailable identity is opaque.
    assert_eq!(selected.model_id, "retired-local");
    assert_eq!(
        completed.model_id,
        selection_identity("model", "retired-local")
    );
    let saved = completed.chrome.selection.as_ref().unwrap();
    assert_eq!(saved.requested, completed.model_id);
    assert_eq!(saved.diagnostic.code, ServiceCode::ModelUnavailable);
    assert_eq!(completed.models.len(), 1);
    assert_eq!(completed.models[0].id, DYNAMIC);
    let readiness = completed.chrome.provider.as_ref().unwrap();
    assert_eq!(readiness.catalog_status, ProviderStatus::Ready);
    assert_eq!(readiness.status, ProviderStatus::Unavailable);
    assert_eq!(
        readiness.diagnostic.as_ref().unwrap().code,
        ServiceCode::ModelUnavailable
    );
    assert_eq!(
        initial.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Pending
    );
    let repaired = app
        .home_selection(Action::Model(DYNAMIC.into()))
        .await
        .unwrap();
    assert_eq!(repaired.model_id, DYNAMIC);
    assert_eq!(
        repaired.chrome.provider.as_ref().unwrap().status,
        ProviderStatus::Ready
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
