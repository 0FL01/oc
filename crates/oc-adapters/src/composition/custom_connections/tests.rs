use super::*;
use oc_core::queries::{CustomProviderAuth, CustomProviderInput, KeyInput};

#[tokio::test]
async fn conn01_attach_before_credentials_and_raw_config_collision_filters() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let data = root.path().join("data");
    let db = crate::storage::Db::open(&data).unwrap();
    let input = CustomProviderInput {
        name: "Saved service".into(),
        base_url: "http://192.168.1.2/api".into(),
        auth: CustomProviderAuth::Key(KeyInput::new("CONNECTION_SCOPE_CANARY".into())),
    };
    let connection = db
        .create_custom_provider(&input, NativeProtocol::Chat, |_| true)
        .unwrap();
    let env = BTreeMap::from([(
        "HOME".into(),
        root.path().join("home").to_string_lossy().into_owned(),
    )]);
    let mut c = load_local_with_env(&project, env.clone()).await.unwrap();
    let generation = c.catalog.provider.clone();
    c.attach_custom_connections(&db).unwrap();
    c.resolve_credentials(&db).await.unwrap();
    assert_eq!(c.catalog.provider, generation);
    let provider = c.provider_for(&connection.id).unwrap();
    assert_eq!(provider.api_key, "CONNECTION_SCOPE_CANARY");
    assert_eq!(provider.wire.protocol, provider::protocol::Protocol::Chat);
    assert!(provider.wire.custom_scope.is_some());
    assert!(!provider.allow_private); // explicit endpoint binding grants local/LAN, not a test switch
    assert_eq!(
        crate::auth::AuthScope::for_binding(
            &connection.id,
            &provider.base_url,
            provider.wire.custom_scope.as_ref()
        )
        .unwrap()
        .namespace(),
        provider.wire.custom_scope.as_ref().unwrap().namespace()
    );

    std::fs::write(project.join("opencode.json"), serde_json::json!({"disabled_providers":[connection.id], "providers":{connection.id.clone():{"npm":"@ai-sdk/openai-compatible", "options":{"baseURL":input.base_url}, "models":{}}}}).to_string()).unwrap();
    let mut c = load_local_with_env(&project, env.clone()).await.unwrap();
    assert!(reserved(&c.mcp_activation.sources).contains(&connection.id));
    c.attach_custom_connections(&db).unwrap();
    assert!(!c.custom_revisions.contains_key(&connection.id));
    assert!(c.provider_for(&connection.id).is_none());
    std::fs::write(project.join("opencode.json"), "{\"enabled_providers\":[]}").unwrap();
    let mut c = load_local_with_env(&project, env).await.unwrap();
    c.attach_custom_connections(&db).unwrap();
    assert!(c.custom_revisions.is_empty());
    assert!(!c.custom_admitted("custom-new"));
}
