//! CFG09: real application-owner queries, atomic reload and durable reopen.
use super::*;
use oc_core::queries::{
    NativePlugin, PluginStatus, ServiceAction, ServiceCode, ServiceKind, ServiceStage,
};
use serde_json::json;

#[tokio::test]
async fn cfg09_plugin_requests_are_safe_independent_and_generation_pinned() {
    let root = tempfile::tempdir().unwrap();
    let global = root.path().join("global");
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    let source = global.join("opencode.json");
    let canary = "CFG09_CONFIG_ENV_AUTH_CANARY_328a";
    let unsafe_identity =
        format!("https://user:{canary}@example.invalid/private/{canary}.js?token={canary}\x1b[31m");
    let mut config = json!({
        "model": "fixture/m", "provider": {"fixture": {
            "options": {"baseURL": "https://example.invalid/v1", "apiKey": "{env:CFG09_KEY}"},
            "models": {"m": {}}
        }},
        "plugin": [
            "@tarquinen/opencode-dcp", "@tarquinen/opencode-dcp",
            "@tarquinen/opencode-dcp@latest", "@tarquinen/opencode-dcp@3.1.15",
            global.join("plugin/openproxy-models.js"),
            project.join("plugins/openproxy-models.js"),
            "@prevalentware/opencode-goal-plugin@0.1.49",
            "@tarquinen/opencode-dcp@3.1.16",
            project.join("plugins/openproxy-models.js.bak"),
            unsafe_identity, canary
        ]
    });
    std::fs::write(&source, config.to_string()).unwrap();
    std::fs::write(project.join("AGENTS.md"), "CFG09_HEALTHY_INSTRUCTIONS").unwrap();
    let env = BTreeMap::from([
        (
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        ),
        ("CFG09_KEY".into(), canary.into()),
    ]);
    let composition = composition::load_with_env(&project, env.clone())
        .await
        .unwrap();
    assert_eq!(
        composition.native_modules,
        ["dcp".into(), "discovery".into()].into()
    );
    assert!(
        composition
            .instructions
            .contains("CFG09_HEALTHY_INSTRUCTIONS")
    );
    let (app, guard, diagnostics) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let initial = app.catalog().await.unwrap();
    let inventory = &initial.chrome.plugins;
    assert_eq!(
        inventory.active_modules,
        [NativePlugin::Dcp, NativePlugin::OpenProxyModels]
    );
    assert_eq!(inventory.entries.len(), 11);
    assert_eq!(inventory.omitted, 0);
    for entry in &inventory.entries[..6] {
        assert_eq!(entry.status, PluginStatus::Active);
        assert!(entry.current.is_some());
        assert!(entry.diagnostic.is_none());
    }
    assert_eq!(
        inventory.entries[0].requested,
        inventory.entries[1].requested
    );
    assert_ne!(
        inventory.entries[0].requested,
        inventory.entries[2].requested
    );
    assert_eq!(inventory.entries[0].current, inventory.entries[2].current);
    assert_eq!(inventory.entries[4].current, inventory.entries[5].current);
    let ignored = &inventory.entries[6];
    assert_eq!(ignored.status, PluginStatus::Ignored);
    assert!(ignored.current.is_none() && ignored.module.is_none());
    for (index, entry) in inventory.entries.iter().enumerate().skip(7) {
        assert_eq!(entry.status, PluginStatus::Failed);
        assert!(entry.current.is_none() && entry.module.is_none());
        let diagnostic = entry.diagnostic.as_ref().unwrap();
        assert_eq!(diagnostic.kind, ServiceKind::Plugin);
        assert_eq!(diagnostic.code, ServiceCode::UnsupportedPlugin);
        assert_eq!(diagnostic.stage, ServiceStage::Capability);
        assert_eq!(diagnostic.action, ServiceAction::ReviewConfiguration);
        assert_eq!(diagnostic.service, entry.requested);
        assert_eq!(diagnostic.field, ["plugin".into(), index.to_string()]);
        assert!(diagnostic.source.starts_with("source-"));
        assert!(diagnostic.source.ends_with("/opencode.json"));
    }
    let safe = format!(
        "{inventory:?} {diagnostics:?} {:?}",
        initial.chrome.service_diagnostics
    );
    assert!(!safe.contains(canary) && !safe.contains('\x1b') && !safe.contains("https://user:"));
    assert!(!safe.contains(&global.to_string_lossy().to_string()));
    let session = SessionId::new("cfg09-durable").unwrap();
    app.create_session(session.clone()).await.unwrap();
    assert!(
        app.history_page(session.clone(), None, None, 20)
            .await
            .unwrap()
            .rows
            .is_empty()
    );

    // A failed revision does not inherit an earlier healthy alias's active state.
    config["plugin"] = json!(["@tarquinen/opencode-dcp@3.1.16"]);
    std::fs::write(&source, config.to_string()).unwrap();
    let failed_request = app.reload_location().await.unwrap();
    assert!(
        failed_request
            .catalog
            .chrome
            .plugins
            .active_modules
            .is_empty()
    );
    assert_eq!(
        failed_request.catalog.chrome.plugins.entries[0].status,
        PluginStatus::Failed
    );
    assert!(
        failed_request.catalog.chrome.plugins.entries[0]
            .current
            .is_none()
    );
    assert_eq!(
        initial.chrome.plugins.entries[0].status,
        PluginStatus::Active
    );
    assert_eq!(failed_request.catalog.model_id, initial.model_id);

    // Invalid mandatory policy/keybindings cannot publish even healthy aliases.
    config["plugin"] = json!(["@tarquinen/opencode-dcp"]);
    config["permissions"] = json!({"read":"not-a-policy"});
    std::fs::write(&source, config.to_string()).unwrap();
    assert!(app.reload_location().await.is_err());
    assert_eq!(app.catalog().await.unwrap(), failed_request.catalog);
    config.as_object_mut().unwrap().remove("permissions");
    config["keybinds"] = json!({"session.undo":12});
    std::fs::write(&source, config.to_string()).unwrap();
    assert!(app.reload_location().await.is_err());
    assert_eq!(app.catalog().await.unwrap(), failed_request.catalog);
    config.as_object_mut().unwrap().remove("keybinds");
    std::fs::write(&source, config.to_string()).unwrap();
    let healthy = app.reload_location().await.unwrap();
    assert!(healthy.generation > failed_request.generation);
    assert_eq!(
        healthy.catalog.chrome.plugins.entries[0],
        initial.chrome.plugins.entries[0]
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (reopened, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    assert_eq!(
        reopened.catalog().await.unwrap().chrome.plugins,
        healthy.catalog.chrome.plugins
    );
    assert!(
        reopened
            .history_page(session, None, None, 20)
            .await
            .unwrap()
            .rows
            .is_empty()
    );
    reopened.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
