use super::*;

#[tokio::test]
async fn hot02_provider_reuse_uses_original_leaf_inputs_not_selection_or_remote_models() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let env = BTreeMap::from([("HOME".into(), root.path().to_string_lossy().into_owned())]);
    let path = project.join("opencode.json");
    let mut config = serde_json::json!({
        "model":"ludka2/exact",
        "disabled_providers":["opencode-go","openai"],
        "providers":{
            "ludka2":{"npm":"@ai-sdk/openai","options":{
                "baseURL":"http://192.168.1.2/literal","apiKey":"CONFIGURED_KEY"
            },"models":{"exact":{"name":"LOCAL_OVERRIDE"}}},
            "other":{"npm":"@ai-sdk/openai","options":{
                "baseURL":"http://192.168.1.3/other","apiKey":"OTHER_KEY"
            },"models":{"other-model":{}}}
        }
    });
    std::fs::write(&path, config.to_string()).unwrap();
    let mut previous = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    previous.accept_provider_catalog(discovery::DiscoveryOutcome {
        models: BTreeMap::from([(
            "remote-only".into(),
            serde_json::json!({"name":"SUPPLIED_REMOTE"}),
        )]),
        warnings: Vec::new(),
        attempts: 1,
        replaced: true,
        failure: None,
    });
    config["model"] = "other/other-model".into();
    config["agents"] = serde_json::json!({"build":{"prompt":"CHANGED_PROFILE"}});
    std::fs::write(&path, config.to_string()).unwrap();
    let mut next = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    assert!(next.same_provider_inputs(&previous, "ludka2"));
    assert!(next.same_provider_inputs(&previous, "other"));
    assert!(next.local_inputs != previous.local_inputs);
    let retained = next.retain_provider_catalogs(&previous);
    assert_eq!(retained, BTreeSet::from(["ludka2".into(), "other".into()]));
    assert!(
        next.catalog_for("ludka2")
            .unwrap()
            .models
            .contains_key("remote-only")
    );
    assert!(
        !next.generation.providers["ludka2"]
            .models
            .contains_key("remote-only")
    );
    // A stored-account/credential change is not a local source edit. Resolve
    // that binding first, and do not carry its previous catalog/auth facts.
    let mut changed_account = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    changed_account
        .provider_views
        .get_mut("ludka2")
        .unwrap()
        .provider
        .api_key = "DIFFERENT_RESOLVED_ACCOUNT_KEY".into();
    assert!(changed_account.same_provider_inputs(&previous, "ludka2"));
    assert!(
        !changed_account
            .retain_provider_catalogs(&previous)
            .contains("ludka2")
    );
    for (field, value) in [
        ("baseURL", "http://192.168.1.2/different-prefix"),
        ("apiKey", "CHANGED_CONFIGURED_KEY"),
    ] {
        let original = config["providers"]["ludka2"]["options"][field].clone();
        config["providers"]["ludka2"]["options"][field] = value.into();
        std::fs::write(&path, config.to_string()).unwrap();
        let changed = crate::composition::load_reload_local_with_env(&project, env.clone())
            .await
            .unwrap();
        assert!(!changed.same_provider_inputs(&previous, "ludka2"));
        config["providers"]["ludka2"]["options"][field] = original;
    }
    config["providers"]["ludka2"]["models"]["exact"]["name"] = "CHANGED_LOCAL".into();
    std::fs::write(&path, config.to_string()).unwrap();
    let mut changed = crate::composition::load_reload_local_with_env(&project, env)
        .await
        .unwrap();
    assert!(!changed.same_provider_inputs(&previous, "ludka2"));
    assert!(
        !changed
            .retain_provider_catalogs(&previous)
            .contains("ludka2")
    );
    assert!(
        !changed
            .catalog_for("ludka2")
            .unwrap()
            .models
            .contains_key("remote-only")
    );
    assert!(!project.join("oc.sqlite").exists());
}

#[tokio::test]
async fn hot02_local_inputs_keep_original_models_and_resolved_dependencies_not_remote_results() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir_all(project.join(".opencode/skills")).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), root.path().to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".into(),
            root.path().join("config").to_string_lossy().into_owned(),
        ),
    ]);
    let path = project.join("opencode.jsonc");
    let config = serde_json::json!({
        "model":"fixture/exact",
        "disabled_providers":["opencode-go","openai"],
        "providers":{"fixture":{
            "npm":"@ai-sdk/openai", "options":{
                "baseURL":"http://192.168.1.2/literal", "apiKey":"{file:key}"
            }, "models":{"exact":{"name":"LOCAL_OVERRIDE","limit":{"context":32000}}}
        }},
        "mcp":{"disabled":{"type":"local","enabled":false,"command":["{file:NEVER_READ}"]}}
    });
    std::fs::write(&path, config.to_string()).unwrap();
    std::fs::write(project.join("key"), "INPUT_KEY_CANARY").unwrap();
    std::fs::write(project.join("AGENTS.md"), "INPUT_INSTRUCTIONS").unwrap();
    std::fs::write(project.join(".opencode/skills/local.md"), "INPUT_SKILL").unwrap();
    let mut original = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    let admitted = original.reload_inputs().unwrap();
    original
        .catalog
        .models
        .insert("remote".into(), serde_json::json!({"name":"REMOTE_ONLY"}));
    original
        .generation
        .providers
        .get_mut("fixture")
        .unwrap()
        .models
        .insert("remote".into(), serde_json::json!({}));
    // Resolved/remote runtime metadata is never mistaken for original local overrides.
    let fresh = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    assert!(admitted == fresh.reload_inputs().unwrap());
    assert!(!fresh.catalog.models.contains_key("remote"));

    // Formatting/comments are not source identity, and unrelated files stay inert.
    std::fs::write(
        &path,
        format!(
            "// cosmetic comment\n{}\n",
            serde_json::to_string_pretty(&config).unwrap()
        ),
    )
    .unwrap();
    std::fs::write(project.join("NEVER_READ"), "DISABLED_CHANGE").unwrap();
    let fresh = crate::composition::load_reload_local_with_env(&project, env.clone())
        .await
        .unwrap();
    assert!(admitted == fresh.reload_inputs().unwrap());
    assert!(
        !fresh
            .watch_inputs
            .targets
            .iter()
            .any(|target| target.path.ends_with("NEVER_READ"))
    );

    for (file, changed, restored) in [
        ("key", "CHANGED_KEY_CANARY", "INPUT_KEY_CANARY"),
        ("AGENTS.md", "CHANGED_INSTRUCTIONS", "INPUT_INSTRUCTIONS"),
        (".opencode/skills/local.md", "CHANGED_SKILL", "INPUT_SKILL"),
    ] {
        std::fs::write(project.join(file), changed).unwrap();
        let fresh = crate::composition::load_reload_local_with_env(&project, env.clone())
            .await
            .unwrap();
        assert!(admitted != fresh.reload_inputs().unwrap());
        std::fs::write(project.join(file), restored).unwrap();
    }
    let mut changed = config;
    changed["providers"]["fixture"]["models"]["exact"]["limit"]["context"] = 64000.into();
    std::fs::write(&path, changed.to_string()).unwrap();
    let fresh = crate::composition::load_reload_local_with_env(&project, env)
        .await
        .unwrap();
    assert!(admitted != fresh.reload_inputs().unwrap());
    assert!(!project.join("oc.sqlite").exists());
}
