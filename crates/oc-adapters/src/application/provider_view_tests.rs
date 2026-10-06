use super::*;
use oc_core::queries::{AccountAction, AccountAuthSource, KeyInput};

#[tokio::test]
async fn go05_provider_views_keep_same_slash_id_and_scoped_auth_without_selection_effects() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let config = serde_json::json!({
        "model":"alpha/same/slash",
        "providers":{
            "alpha":{"package":"@ai-sdk/openai","settings":{"baseURL":"https://example.com/alpha","apiKey":"ALPHA_PRIVATE_CANARY"},"models":{"same/slash":{"name":"Alpha","limit":{"context":10000,"output":1024}}}},
            "beta":{"package":"@ai-sdk/anthropic","settings":{"baseURL":"https://example.com/beta"},"models":{"same/slash":{"name":"Beta","settings":{"thinking":{"type":"adaptive"}},"limit":{"context":20000,"output":2048}}}},
            "foreign":{"package":"unknown-package","settings":{"baseURL":"https://example.com/foreign","apiKey":"{file:must-not-read}"},"models":{"other":{"name":"Foreign"}}}
        }
    });
    let file = project.join("opencode.json");
    std::fs::write(&file, config.to_string()).unwrap();
    {
        let db = Db::open(&data).unwrap();
        db.set_pref(crate::models_dev::CACHE_KEY, &serde_json::json!({
            "source":crate::models_dev::SOURCE,"fetched_at_ms":composition::go_catalog::now_ms(),
            "record":{"id":"opencode-go","npm":"@ai-sdk/openai-compatible","models":{
                "same/slash":{"id":"same/slash","name":"Go public","tool_call":true,"limit":{"context":30000,"output":2048}}
            }}
        }).to_string()).unwrap();
    }
    let env = BTreeMap::from([("OPENCODE_API_KEY".into(), "GO_ENV_PRIVATE_CANARY".into())]);
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let alpha = app.catalog().await.unwrap();
    assert_eq!(alpha.selected_model().unwrap().provider, "alpha");
    let beta = app.provider_catalog("beta".into()).await.unwrap();
    assert_eq!(beta.provider, "beta");
    assert!(beta.selected_model().is_none());
    assert_eq!(beta.models[0].id, "same/slash");
    assert_eq!(beta.models[0].display_name, "Beta");
    assert_eq!(beta.models[0].context, 20000);
    assert_eq!(
        app.provider_accounts("beta".into(), None)
            .await
            .unwrap()
            .effective,
        AccountAuthSource::Missing
    );
    let account = app
        .provider_accounts(
            "beta".into(),
            Some(AccountAction::AddKey {
                label: "Beta account".into(),
                key: KeyInput::new("BETA_PRIVATE_CANARY".into()),
            }),
        )
        .await
        .unwrap();
    assert_eq!(account.effective, AccountAuthSource::Stored);
    assert!(!format!("{account:?} {beta:?}").contains("PRIVATE_CANARY"));
    let go = app.provider_catalog("opencode-go".into()).await.unwrap();
    assert_eq!(go.models[0].display_name, "Go public");
    assert_eq!(go.models[0].id, beta.models[0].id);
    assert!(go.selected_model().is_none());
    assert!(app.provider_catalog("foreign".into()).await.is_err());
    assert_eq!(
        app.catalog().await.unwrap().selected_model(),
        alpha.selected_model()
    );
    let mut edited = config.clone();
    edited["providers"]["beta"]["models"]["same/slash"]["name"] = "Unadmitted edit".into();
    std::fs::write(&file, edited.to_string()).unwrap();
    assert_eq!(
        app.provider_catalog("beta".into()).await.unwrap().models[0].display_name,
        "Beta"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = Db::open(&data).unwrap();
    assert!(db.list_sessions().unwrap().is_empty());
    let mut c = composition::load_local_with_env(&project, env.clone())
        .await
        .unwrap();
    c.resolve_credentials(&db).unwrap();
    c.attach_public_catalog(&db).await;
    let beta_view = &c.provider_views["beta"];
    assert_eq!(beta_view.provider.api_key, "BETA_PRIVATE_CANARY");
    assert_eq!(
        beta_view.provider.wire.protocol,
        crate::provider::protocol::Protocol::Messages
    );
    assert_eq!(c.provider.api_key, "ALPHA_PRIVATE_CANARY");
    assert_eq!(
        c.provider_views["opencode-go"].provider.api_key,
        "GO_ENV_PRIVATE_CANARY"
    );
    drop(c);
    drop(db);
    std::fs::write(&file, config.to_string()).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    assert_eq!(
        app.provider_accounts("beta".into(), None).await.unwrap(),
        account
    );
    assert_eq!(
        app.catalog().await.unwrap().selected_model(),
        alpha.selected_model()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
