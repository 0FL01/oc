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

#[tokio::test]
async fn unselected_dynamic_view_discovers_its_catalog_without_selecting_it() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
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
        let body = serde_json::json!({"object":"list", "data":[{"id":"dyn-1", "opencode":{
            "name":"Discovered One", "limit":{"context":32768,"output":4096}
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
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model": "alpha/a1",
        "providers": {
            "alpha": {"package": "@ai-sdk/openai",
                "settings": {"baseURL": "http://127.0.0.1:9/alpha", "apiKey": "ALPHA_KEY"},
                "models": {"a1": {"limit": {"context": 1000, "output": 100}}}},
            "ludka2": {"package": "@ai-sdk/openai", "settings": {"baseURL": format!("http://{addr}/v1"), "apiKey": "fixture"}}
        },
        "permissions": [{"action": "*", "resource": "*", "effect": "allow"}],
        "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}]}
    }).to_string()).unwrap();
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
    tokio::time::timeout(std::time::Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap();
    let ludka2 = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let catalogs = app.model_catalogs().await.unwrap();
            if let Some(view) = catalogs
                .catalogs
                .iter()
                .find(|c| c.provider == "ludka2" && !c.models.is_empty())
            {
                break (catalogs.catalogs[0].provider.clone(), view.clone());
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("unselected ludka2 view must load its catalog");
    assert_eq!(ludka2.0, "alpha", "discovery never changes the selection");
    assert_eq!(ludka2.1.models[0].id, "dyn-1");
    assert!(ludka2.1.model_id.is_empty());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn selection_and_effort_survive_a_restart_with_another_default_provider() {
    use oc_core::queries::{ModelCommit, SelectionBinding, SessionSelectionAction as Action};
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    let config = |model: &str| {
        serde_json::json!({
            "model": model,
            "providers": {
                "alpha": {"package": "@ai-sdk/openai",
                    "settings": {"baseURL": "http://127.0.0.1:9/alpha", "apiKey": "A"},
                    "models": {"a1": {"limit": {"context": 1000, "output": 100}}}},
                "gamma": {"package": "@ai-sdk/openai",
                    "settings": {"baseURL": "http://127.0.0.1:9/gamma", "apiKey": "G"},
                    "models": {"g1": {"limit": {"context": 1000, "output": 100}}}},
                "beta": {"package": "@ai-sdk/openai",
                    "settings": {"baseURL": "http://127.0.0.1:9/beta", "apiKey": "B"},
                    "models": {"b1": {"limit": {"context": 1000, "output": 100},
                        "variants": [{"id": "low", "settings": {"reasoningEffort": "low"}},
                                     {"id": "high", "settings": {"reasoningEffort": "high"}}]}}}
            },
            "permissions": [{"action": "*", "resource": "*", "effect": "allow"}],
            "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}]}
        })
        .to_string()
    };
    std::fs::write(project.join("opencode.json"), config("alpha/a1")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let home = app.home_selection(Action::Current).await.unwrap();
    let commit = |snapshot: &oc_core::queries::CatalogSnapshot| {
        Action::Commit(ModelCommit {
            attachment: 0,
            caller: 1,
            binding: SelectionBinding {
                location: snapshot.chrome.location.clone(),
                generation: snapshot.chrome.selection_generation,
                provider: "beta".into(),
                agent_id: snapshot.agent_id.clone(),
            },
            model_id: "b1".into(),
            variant: Some("high".into()),
            draft_revision: 1,
        })
    };
    app.home_selection(commit(&home)).await.unwrap();
    let session = SessionId::new("persisted").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let current = app
        .session_selection(session.clone(), false, Action::Current)
        .await
        .unwrap();
    let chosen = app
        .session_selection(session.clone(), false, commit(&current))
        .await
        .unwrap();
    assert_eq!(
        (
            chosen.provider.as_str(),
            chosen.model_id.as_str(),
            chosen.variant.as_deref()
        ),
        ("beta", "b1", Some("high"))
    );
    let legacy = SessionId::new("legacy").unwrap();
    app.create_session(legacy.clone()).await.unwrap();
    let remembered = app.model_preference(None).await.unwrap();
    assert!(
        remembered
            .variants
            .iter()
            .any(|(m, v)| m.provider == "beta" && m.model == "b1" && v == "high")
    );
    let location = home.chrome.location.clone().unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    // A pre-change record keyed by the then-default provider stays readable.
    let db = crate::storage::Db::open(&data).unwrap();
    db.set_pref(
        &format!(
            "tui.selection.session:{}",
            serde_json::json!([location, "alpha", "legacy"])
        ),
        &serde_json::json!({"agent": chosen.agent_id, "models": {
            chosen.agent_id.clone().unwrap_or_default():
                {"provider": "beta", "id": "b1", "variant": "low"}}})
        .to_string(),
    )
    .unwrap();
    drop(db);
    // The next start defaults to another provider: choices keep their own.
    std::fs::write(project.join("opencode.json"), config("gamma/g1")).unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let home = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            home.provider.as_str(),
            home.model_id.as_str(),
            home.variant.as_deref()
        ),
        ("beta", "b1", Some("high"))
    );
    let restored = app
        .session_selection(session, false, Action::Current)
        .await
        .unwrap();
    assert_eq!(
        (
            restored.provider.as_str(),
            restored.model_id.as_str(),
            restored.variant.as_deref()
        ),
        ("beta", "b1", Some("high"))
    );
    let legacy = app
        .session_selection(legacy, false, Action::Current)
        .await
        .unwrap();
    assert_eq!(
        (legacy.provider.as_str(), legacy.variant.as_deref()),
        ("beta", Some("low"))
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_pick_and_effort_are_remembered_without_sending() {
    use oc_core::queries::SessionSelectionAction as Action;
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    // No configured `model`: pinned fallbackModel uses the recent choice.
    std::fs::write(
        project.join("opencode.json"),
        serde_json::json!({
            "providers": {
                "beta": {"package": "@ai-sdk/openai",
                    "settings": {"baseURL": "http://127.0.0.1:9/beta", "apiKey": "B"},
                    "models": {"b1": {"limit": {"context": 1000, "output": 100},
                        "variants": [{"id": "low", "settings": {"reasoningEffort": "low"}},
                                     {"id": "high", "settings": {"reasoningEffort": "high"}}]}}}
            },
            "permissions": [{"action": "*", "resource": "*", "effect": "allow"}],
            "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}]}
        })
        .to_string(),
    )
    .unwrap();
    let model = ModelPreferenceModel {
        provider: "beta".into(),
        model: "b1".into(),
    };
    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    app.model_preference(Some(ModelPreferenceAction::Recent(model.clone())))
        .await
        .unwrap();
    let stored = app
        .model_preference(Some(ModelPreferenceAction::Variant {
            model: model.clone(),
            variant: Some("high".into()),
        }))
        .await
        .unwrap();
    assert_eq!(stored.variants, vec![(model.clone(), "high".to_string())]);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    let home = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            home.provider.as_str(),
            home.model_id.as_str(),
            home.variant.as_deref()
        ),
        ("beta", "b1", Some("high"))
    );
    assert_eq!(
        app.model_preference(None).await.unwrap().variants,
        vec![(model, "high".to_string())]
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
