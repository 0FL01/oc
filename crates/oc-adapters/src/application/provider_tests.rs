//! UI07: existing application owner remains local while provider admission fails.
use super::*;
use oc_core::queries::{ProviderStatus, ServiceAction, ServiceCode, ServiceKind, ServiceStage};
use serde_json::json;
use std::time::Duration;

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
        "agent":{"ui07-primary":{"mode":"primary", "model":format!("ludka2/{DYNAMIC}"), "prompt":"UI07 profile body"}},
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
    assert_eq!(completed.model_id, "retired-local");
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
