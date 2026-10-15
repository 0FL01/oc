use super::*;

#[tokio::test]
async fn hot02_reload_verdict_distinguishes_mandatory_inputs_from_optional_inventory() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path();
    let path = project.join("opencode.json");
    let original = serde_json::json!({
        "model":"fixture/m", "providers":{"fixture":{"settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}},
        "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}]}
    });
    std::fs::write(&path, original.to_string()).unwrap();
    let definitions = project.join(".opencode");
    for (subdir, name, bytes) in [
        (
            "skills",
            "broken.md",
            "---\ndescription: false\n---\nPRIVATE_CANARY",
        ),
        (
            "commands",
            "broken.md",
            "---\nsubagent: []\n---\nPRIVATE_CANARY",
        ),
        (
            "agents",
            "broken.md",
            "---\npermissions: []\n---\nPRIVATE_CANARY",
        ),
    ] {
        let parent = definitions.join(subdir);
        std::fs::create_dir_all(&parent).unwrap();
        let file = parent.join(name);
        std::fs::write(&file, bytes).unwrap();
        // The existing startup fault-isolation contract is not changed by reload.
        assert!(load_local_with_env(project, BTreeMap::new()).await.is_ok());
        let error = load_reload_local_with_env(project, BTreeMap::new())
            .await
            .err()
            .unwrap();
        let LoadFailure::Configuration(diagnostic) = &error;
        assert_eq!(diagnostic.kind, ServiceKind::Definition);
        assert!(!error.to_string().contains("PRIVATE_CANARY"));
        std::fs::remove_file(file).unwrap();
    }
    std::fs::write(project.join("AGENTS.md"), [0xff]).unwrap();
    assert!(load_local_with_env(project, BTreeMap::new()).await.is_ok());
    assert!(
        load_reload_local_with_env(project, BTreeMap::new())
            .await
            .is_err()
    );
    std::fs::remove_file(project.join("AGENTS.md")).unwrap();
    let mut invalid = original.clone();
    invalid["compaction"] = serde_json::json!({"auto":"PRIVATE_CANARY"});
    std::fs::write(&path, invalid.to_string()).unwrap();
    assert!(load_local_with_env(project, BTreeMap::new()).await.is_ok());
    let LoadFailure::Configuration(diagnostic) =
        load_reload_local_with_env(project, BTreeMap::new())
            .await
            .err()
            .unwrap();
    assert_eq!(diagnostic.code, ServiceCode::InvalidConfig);
    assert!(diagnostic.field.iter().any(|field| field == "compaction"));
    assert!(!diagnostic.to_string().contains("PRIVATE_CANARY"));

    // A disabled template remains inert; unsupported optional integrations remain
    // failed inventory, not malformed mandatory native policy/definitions.
    let mut optional = original;
    optional["plugins"] = serde_json::json!(["unavailable-native-plugin"]);
    optional["mcp"] = serde_json::json!({"servers":{
        "disabled":{"type":"local","disabled":true,"command":["{file:NEVER_READ}"]},
        "unsupported":{"type":"remote","url":"https://example.invalid/mcp","oauth":{"client_id":"x"}}
    }});
    std::fs::write(&path, optional.to_string()).unwrap();
    let accepted = load_reload_local_with_env(project, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        accepted.generation.mcp["disabled"].command,
        ["{file:NEVER_READ}"]
    );
    assert!(accepted.generation.mcp["unsupported"].failure.is_some());
    assert_eq!(
        accepted.tui_chrome.plugins.entries[0].status,
        PluginStatus::Failed
    );
}

#[tokio::test]
async fn dcp12_native_protection_source_arrays_replace_then_shared_scopes_union() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"fixture/m", "providers":{"fixture":{"settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}},
        "dcp":{"commands":{"protectedTools":["read"]},"compress":{"protectedTools":["read"]},
          "strategies":{"deduplication":{"protectedTools":["read"]},"purgeErrors":{"protectedTools":["read"]}}}
    }).to_string()).unwrap();
    std::fs::write(project.join("dcp.jsonc"), serde_json::json!({
        "compress":{"protectedTools":["skill"]},
        "strategies":{"deduplication":{"protectedTools":["skill"]},"purgeErrors":{"protectedTools":["question"]}}
    }).to_string()).unwrap();
    let loaded = load_with_env(
        &project,
        BTreeMap::from([(
            "HOME".into(),
            root.path().join("home").to_string_lossy().into_owned(),
        )]),
    )
    .await
    .unwrap();
    // Donor mergeLayer unions source arrays and keeps command/compress scopes
    // separate. Native documented replacement and shared list stay frozen;
    // all fixtures name actual registered native tools, not fictitious donors.
    assert_eq!(loaded.dcp_config.protected_tools, ["read", "skill"]);
    assert_eq!(loaded.dcp_config.dedup_protected_tools, ["skill"]);
    assert_eq!(loaded.dcp_config.purge_protected_tools, ["question"]);
}

#[tokio::test]
async fn dcp12_nested_diagnostics_keep_source_and_field_but_not_unknown_payload() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"),r#"{"model":"fixture/m","providers":{"fixture":{"settings":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}}}"#).unwrap();
    let sidecar = project.join("dcp.jsonc");
    let source = crate::config::mcp::safe_source_id(&sidecar.to_string_lossy());
    std::fs::write(
        &sidecar,
        r#"{"compress":{"enabled":false,"secret-name-owner-path":"SECRET_PAYLOAD"}}"#,
    )
    .unwrap();
    let loaded = load_with_env(&project, BTreeMap::new()).await.unwrap();
    assert!(!loaded.dcp_config.compress_enabled);
    let warning = loaded
        .diagnostics
        .iter()
        .find(|d| d.contains("compress.entry"))
        .unwrap();
    assert!(warning.contains(&source));
    assert!(!warning.contains("SECRET_PAYLOAD") && !warning.contains("secret-name-owner-path"));
    std::fs::write(&sidecar, r#"{"compress":{"enabled":"SECRET_PAYLOAD"}}"#).unwrap();
    let error = load_with_env(&project, BTreeMap::new())
        .await
        .err()
        .unwrap();
    assert!(
        error.contains(&source) && error.contains("compress.enabled"),
        "{error}"
    );
    assert!(!error.contains("SECRET_PAYLOAD"));
}
