use super::*;

#[tokio::test]
async fn dcp12_native_protection_source_arrays_replace_then_shared_scopes_union() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"fixture/m", "provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}},
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
    std::fs::write(project.join("opencode.json"),r#"{"model":"fixture/m","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"synthetic"},"models":{"m":{}}}}}"#).unwrap();
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
