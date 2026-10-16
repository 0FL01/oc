use super::*;
use oc_core::queries::ModelPreferenceModel;

fn write_config(project: &std::path::Path) {
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model": "alpha/a1",
        "providers": {
            "alpha": {"name": "Alpha AI", "package": "@ai-sdk/openai",
                "settings": {"baseURL": "http://127.0.0.1:9/alpha", "apiKey": "ALPHA_KEY"},
                "models": {"a1": {"name": "A One", "cost": {"input": 0, "output": 1},
                                  "limit": {"context": 1000, "output": 100}}}},
            "beta": {"package": "@ai-sdk/openai",
                "settings": {"baseURL": "http://127.0.0.1:9/beta", "apiKey": "BETA_KEY"},
                "models": {"a1": {"name": "Beta One", "limit": {"context": 1000, "output": 100}}}}
        },
        "permissions": [{"action": "*", "resource": "*", "effect": "allow"}],
        "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}]}
    }).to_string()).unwrap();
}

#[test]
fn release_dates_normalize_to_unix_ms() {
    let ms = |spec| released(&spec);
    assert_eq!(
        ms(serde_json::json!({"release_date": "1970-01-02"})),
        Some(86_400_000)
    );
    assert_eq!(
        ms(serde_json::json!({"release_date": "2025-03"})),
        Some(1_740_787_200_000)
    );
    assert_eq!(ms(serde_json::json!({"time": {"released": 5}})), Some(5));
    assert_eq!(ms(serde_json::json!({"release_date": "soon"})), None);
    assert_eq!(ms(serde_json::json!({})), None);
}

#[tokio::test]
async fn all_loaded_provider_views_and_preferences_survive_reopen() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    write_config(&project);
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let catalogs = app.model_catalogs().await.unwrap();
    assert!(catalogs.connected);
    let providers: Vec<_> = catalogs
        .catalogs
        .iter()
        .map(|c| c.provider.as_str())
        .collect();
    assert_eq!(providers, ["alpha", "beta"], "selected provider first");
    let alpha = &catalogs.catalogs[0].models[0];
    assert_eq!(
        (alpha.provider_name.as_str(), alpha.input_free),
        ("Alpha AI", true)
    );
    assert_eq!(catalogs.catalogs[0].model_id, "a1");
    // Other providers are browse views, never a manufactured selection.
    assert!(catalogs.catalogs[1].model_id.is_empty());
    assert_eq!(catalogs.catalogs[1].models[0].provider_name, "beta");
    assert!(!catalogs.catalogs[1].models[0].input_free);

    let model = |provider: &str| ModelPreferenceModel {
        provider: provider.into(),
        model: "a1".into(),
    };
    assert_eq!(
        app.model_preference(None).await.unwrap(),
        Default::default()
    );
    app.model_preference(Some(ModelPreferenceAction::Recent(model("alpha"))))
        .await
        .unwrap();
    app.model_preference(Some(ModelPreferenceAction::Recent(model("beta"))))
        .await
        .unwrap();
    let stored = app
        .model_preference(Some(ModelPreferenceAction::Favorite {
            model: model("beta"),
            enabled: true,
        }))
        .await
        .unwrap();
    assert_eq!(stored.recent, vec![model("beta"), model("alpha")]);
    assert_eq!(stored.favorite, vec![model("beta")]);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(app.model_preference(None).await.unwrap(), stored);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
