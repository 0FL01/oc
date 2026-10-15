use super::*;

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
