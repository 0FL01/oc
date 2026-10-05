use super::*;
use crate::{
    auth::{AuthScope, GO_BASE_URL},
    storage::{CredentialMaterial, Db},
};
use oc_core::{
    domain::SessionId,
    queries::{ProviderStatus, ServiceCode, SessionProbe, SessionSelectionAction as Action},
};
use serde_json::json;

struct PublicFixture(Value);
impl discovery::DiscoveryClient for PublicFixture {
    async fn get(
        &self,
        url: &str,
        headers: &reqwest::header::HeaderMap,
        _: Duration,
    ) -> Result<(u16, Vec<u8>), discovery::DiscoveryError> {
        assert_eq!(url, crate::models_dev::SOURCE);
        assert!(!headers.contains_key("authorization") && !headers.contains_key("x-api-key"));
        Ok((200, serde_json::to_vec(&self.0).unwrap()))
    }
}

fn fixture() -> PublicFixture {
    let mut models = serde_json::Map::new();
    for (id, package) in [
        ("chat", "@ai-sdk/openai-compatible"),
        ("responses", "@ai-sdk/openai"),
        ("messages", "@ai-sdk/anthropic"),
        ("unknown", "@unsupported/sdk"),
    ] {
        models.insert(id.into(), json!({"id":id,"name":id,"provider":{"npm":package,"api":"http://169.254.169.254/REMOTE_SECRET"},
            "tool_call":true,"limit":{"context":100000,"output":4096},
            "interleaved":{"field":"reasoning_content"},
            "reasoning_options":[{"type":"effort","values":["low","high"]}]}));
    }
    PublicFixture(
        json!({"opencode-go":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":models}}),
    )
}

#[tokio::test]
async fn go02_cached_go_startup_is_credential_independent_and_aliases_are_not_fallbacks() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        json!({"model":"opencode-go/chat"}).to_string(),
    )
    .unwrap();
    {
        let db = Db::open(&data).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &db.public_catalog(),
            &db.shared_handle().public_catalog()
        ));
        db.public_catalog()
            .refresh(&db, &fixture(), &BTreeMap::new(), now_ms(), true)
            .await;
    }
    let (app, guard, _) = crate::application::spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let snapshot = app.catalog().await.unwrap();
    assert_eq!(snapshot.models.len(), 4);
    assert_eq!(
        snapshot.chrome.provider.as_ref().unwrap().catalog_status,
        ProviderStatus::Ready
    );
    assert_eq!(
        snapshot
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
    let session = SessionId::new("missing-go-key").unwrap();
    assert!(
        app.submit_fresh(session.clone(), "must not accept".into(), None)
            .await
            .is_err()
    );
    assert_eq!(
        app.probe_session(session).await.unwrap(),
        SessionProbe::Absent
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.add_credential(
            AuthScope::admit(PROVIDER, GO_BASE_URL).unwrap().namespace(),
            "synthetic",
            CredentialMaterial::Key {
                key: "GO_KEY_CANARY".into(),
            },
        )
        .unwrap();
    }
    let (app, guard, _) = crate::application::spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.catalog().await.unwrap().chrome.provider.unwrap().status,
        ProviderStatus::Ready
    );
    let unknown = app
        .home_selection(Action::Model("unknown".into()))
        .await
        .unwrap();
    assert_eq!(
        unknown.chrome.provider.unwrap().diagnostic.unwrap().code,
        ServiceCode::UnsupportedCapability
    );
    let session = SessionId::new("unknown-go-alias").unwrap();
    assert!(
        app.submit_fresh(session.clone(), "must not accept".into(), None)
            .await
            .is_err()
    );
    assert_eq!(
        app.probe_session(session).await.unwrap(),
        SessionProbe::Absent
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn go02_public_models_capture_independent_native_wires_and_retire_bindings() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"), json!({"model":"opencode-go/messages#high", "providers":{"opencode-go":{"models":{"messages":{"name":"Local name"}}}}}).to_string()).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    db.public_catalog()
        .refresh(&db, &fixture(), &BTreeMap::new(), now_ms(), true)
        .await;
    db.add_credential(
        AuthScope::admit(PROVIDER, GO_BASE_URL).unwrap().namespace(),
        "synthetic",
        CredentialMaterial::Key {
            key: "GO_KEY_CANARY".into(),
        },
    )
    .unwrap();
    let mut composition = load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    composition.resolve_credentials(&db).unwrap();
    composition.attach_public_catalog(&db).await;
    assert_eq!(composition.catalog.models["messages"]["name"], "Local name");
    for (model, protocol) in [
        ("chat", provider::protocol::Protocol::Chat),
        ("responses", provider::protocol::Protocol::Responses),
        ("messages", provider::protocol::Protocol::Messages),
    ] {
        let request = composition.provider.for_selection(model, Some("high"));
        assert_eq!(request.wire.protocol, protocol);
        assert_eq!(request.wire.api_model.as_deref(), Some(model));
        assert_eq!(request.base_url, GO_BASE_URL);
        assert_eq!(request.api_key, "GO_KEY_CANARY");
        let headers = provider::request_headers(request).unwrap();
        assert_eq!(headers["authorization"], "Bearer GO_KEY_CANARY");
        assert!(!headers.contains_key("x-api-key"));
        let mut body = json!({});
        request.wire.settings.apply(protocol, &mut body, None);
        if protocol == provider::protocol::Protocol::Messages {
            assert_eq!(body["thinking"]["type"], "adaptive");
        }
    }
    let chat = composition.provider.for_selection("chat", Some("high"));
    assert_eq!(
        chat.wire.chat["chat"].reasoning_field.as_deref(),
        Some("reasoning_content")
    );
    assert!(
        composition
            .provider
            .for_selection("unknown", None)
            .wire
            .unsupported
    );
    // A public 401 with last-good metadata is not paid-connection auth rejection.
    let mut old = db.public_catalog().read(&BTreeMap::new()).await;
    old.failure = Some(discovery::DiscoveryFailure::Unauthorized);
    composition.accept_provider_catalog(outcome(old));
    assert_eq!(
        composition
            .selected_provider_readiness("messages", Some("high"))
            .status,
        ProviderStatus::Ready
    );
    db.public_catalog().refresh(&db, &PublicFixture(json!({"opencode-go":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{}}})), &BTreeMap::new(), now_ms(), true).await;
    composition.attach_public_catalog(&db).await;
    assert!(composition.catalog.models.is_empty());
    assert!(composition.provider.wire.requests.is_empty());
    assert_eq!(
        composition
            .selected_provider_readiness("messages", Some("high"))
            .status,
        ProviderStatus::Unavailable
    );
}
