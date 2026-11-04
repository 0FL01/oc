//! MCP09 resource admission and captured parent environment inheritance.
use super::*;
use serde_json::json;
use std::fs;

fn fixture() -> (
    tempfile::TempDir,
    PathBuf,
    PathBuf,
    BTreeMap<String, String>,
) {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let global = temp.path().join("global");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&global).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), temp.path().to_string_lossy().into_owned()),
        (
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        ),
        ("BENIGN_ALIAS".into(), "mcp09-higher-domain-canary".into()),
    ]);
    fs::write(global.join("opencode.json"),json!({"model":"fixture/main","providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"baseURL":"https://example.invalid/v1","apiKey":"{env:BENIGN_ALIAS}"},"models":{"main":{}}}}}).to_string()).unwrap();
    (temp, project, global, env)
}

#[tokio::test]
async fn mcp09_cwd_invalid_is_typed_but_escape_is_fatal_and_no_fallback() {
    let (_root, project, global, env) = fixture();
    fs::write(
        global.join("opencode.jsonc"),
        json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"cwd":"missing"}}}})
            .to_string(),
    )
    .unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    let issue = composition.generation.mcp["local"]
        .failure
        .as_ref()
        .unwrap();
    assert_eq!(issue.code, oc_core::queries::ServiceCode::InvalidCwd);
    assert_eq!(issue.stage, oc_core::queries::ServiceStage::Admission);
    assert_eq!(
        composition.tui_chrome.service_diagnostics.as_slice(),
        std::slice::from_ref(issue)
    );
    fs::remove_file(global.join("opencode.jsonc")).unwrap();
    std::os::unix::fs::symlink(&global, project.join("escaped")).unwrap();
    fs::write(
        project.join("opencode.json"),
        json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"cwd":"escaped"}}}})
            .to_string(),
    )
    .unwrap();
    assert!(
        load_with_env(&project, env.clone())
            .await
            .err()
            .unwrap()
            .contains("trust_refused")
    );
    // External cwd still requires explicit resource admission. Environment
    // inheritance is the full captured parent map for either source domain.
    fs::write(project.join("opencode.json"),json!({"permissions":[{"action": "external_directory", "resource": "*", "effect": "allow"}],"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"cwd":global}}}}).to_string()).unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    let entry = &composition.generation.mcp["local"];
    assert!(entry.resource_admitted);
    let config = crate::mcp_stdio::StdioConfig::from_entry("local", entry, &project, &env).unwrap();
    assert_eq!(config.cwd, Some(global));
    assert!(
        config
            .extra_env
            .iter()
            .any(|(name, value)| name == "BENIGN_ALIAS" && value == &env["BENIGN_ALIAS"])
    );
}

#[tokio::test]
async fn mcp09_project_overlay_inherits_captured_credentials() {
    let (_root, project, global, env) = fixture();
    fs::write(project.join("opencode.json"),json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"environment":{"BENIGN_NAME":"{env:BENIGN_ALIAS}"}}}}}).to_string()).unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    assert!(composition.generation.mcp["local"].environment["BENIGN_NAME"] == env["BENIGN_ALIAS"]);
    // Inheritance is independent of whether another source references the value.
    let mut orphan_env = env.clone();
    orphan_env.insert(
        "ORPHAN_PASSWORD".into(),
        "mcp09-orphan-secret-canary".into(),
    );
    fs::write(project.join("opencode.json"),json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"environment":{"BENIGN_NAME":"{env:ORPHAN_PASSWORD}"}}}}}).to_string()).unwrap();
    let composition = load_with_env(&project, orphan_env.clone()).await.unwrap();
    assert!(
        composition.generation.mcp["local"].environment["BENIGN_NAME"]
            == orphan_env["ORPHAN_PASSWORD"]
    );
    fs::write(project.join("opencode.json"),json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"],"environment":{"LOCAL_TOKEN":"mcp09-explicit-local-canary"}}}}}).to_string()).unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    let entry = &composition.generation.mcp["local"];
    let config = crate::mcp_stdio::StdioConfig::from_entry("local", entry, &project, &env).unwrap();
    assert!(
        config
            .extra_env
            .contains(&("LOCAL_TOKEN".into(), "mcp09-explicit-local-canary".into()))
    );
    assert!(
        config
            .extra_env
            .iter()
            .any(|(name, value)| name == "BENIGN_ALIAS" && value == &env["BENIGN_ALIAS"])
    );
    fs::write(project.join("opencode.json"),json!({
        "providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"baseURL":"https://example.invalid/v1","apiKey":"local-key"},"models":{"main":{}}}},
        "mcp":{"servers": {"local": {"type":"local","command":["/bin/true"]}}}
    }).to_string()).unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    assert_eq!(
        composition.generation.providers["fixture"].options.api_key,
        "local-key"
    );
    let config = crate::mcp_stdio::StdioConfig::from_entry(
        "local",
        &composition.generation.mcp["local"],
        &project,
        &env,
    )
    .unwrap();
    assert!(
        config
            .extra_env
            .iter()
            .any(|(name, value)| name == "BENIGN_ALIAS" && value == &env["BENIGN_ALIAS"]),
        "whole-entry replacement must not filter the captured parent map"
    );
    let nested = global.join("lower-location");
    fs::create_dir(&nested).unwrap();
    fs::write(
        nested.join("opencode.json"),
        json!({"mcp":{"servers": {"local": {"type":"local","command":["/bin/true"]}}}}).to_string(),
    )
    .unwrap();
    let composition = load_with_env(&nested, env.clone()).await.unwrap();
    let entry = &composition.generation.mcp["local"];
    assert!(entry.resource_admitted);
    let config = crate::mcp_stdio::StdioConfig::from_entry("local", entry, &nested, &env).unwrap();
    assert!(
        config
            .extra_env
            .iter()
            .any(|(name, value)| name == "BENIGN_ALIAS" && value == &env["BENIGN_ALIAS"])
    );
}
