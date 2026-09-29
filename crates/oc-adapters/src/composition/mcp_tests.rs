//! MCP09 resource/credential admission remains a fatal trust boundary.
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
    fs::write(global.join("opencode.json"),json!({"model":"fixture/main","provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"https://example.invalid/v1","apiKey":"{env:BENIGN_ALIAS}"},"models":{"main":{}}}}}).to_string()).unwrap();
    (temp, project, global, env)
}

#[tokio::test]
async fn mcp09_cwd_invalid_is_typed_but_escape_is_fatal_and_no_fallback() {
    let (_root, project, global, env) = fixture();
    fs::write(
        global.join("opencode.jsonc"),
        json!({"mcp":{"local":{"type":"local","command":["/bin/true"],"cwd":"missing"}}})
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
        json!({"mcp":{"local":{"type":"local","command":["/bin/true"],"cwd":"escaped"}}})
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
    // The existing explicit resource policy admits the canonical external path;
    // it does not grant global credential inheritance to the project command.
    fs::write(project.join("opencode.json"),json!({"permissions":{"external_directory":"allow"},"mcp":{"local":{"type":"local","command":["/bin/true"],"cwd":global}}}).to_string()).unwrap();
    let composition = load_with_env(&project, env.clone()).await.unwrap();
    let entry = &composition.generation.mcp["local"];
    assert!(entry.resource_admitted);
    assert!(!entry.inherit_credentials);
    let config = crate::mcp_stdio::StdioConfig::from_entry("local", entry, &project, &env).unwrap();
    assert_eq!(config.cwd, Some(global));
    assert!(
        config
            .extra_env
            .iter()
            .all(|(name, _)| name != "BENIGN_ALIAS")
    );
}

#[tokio::test]
async fn mcp09_project_overlay_cannot_acquire_higher_trust_credential() {
    let (_root, project, global, env) = fixture();
    fs::write(project.join("opencode.json"),json!({"mcp":{"local":{"type":"local","command":["/bin/true"],"environment":{"BENIGN_NAME":"{env:BENIGN_ALIAS}"}}}}).to_string()).unwrap();
    let error = load_with_env(&project, env.clone()).await.err().unwrap();
    assert!(error.contains("trust_refused"));
    assert!(!error.contains("mcp09-higher-domain-canary"));
    // A product-process credential does not become a project credential merely
    // because no global provider/server happened to reference it. The overlay
    // cannot reintroduce a value that inheritance has explicitly withheld.
    let mut orphan_env = env.clone();
    orphan_env.insert(
        "ORPHAN_PASSWORD".into(),
        "mcp09-orphan-secret-canary".into(),
    );
    fs::write(project.join("opencode.json"),json!({"mcp":{"local":{"type":"local","command":["/bin/true"],"environment":{"BENIGN_NAME":"{env:ORPHAN_PASSWORD}"}}}}).to_string()).unwrap();
    let error = load_with_env(&project, orphan_env).await.err().unwrap();
    assert!(error.contains("trust_refused"));
    assert!(!error.contains("mcp09-orphan-secret-canary"));
    fs::write(project.join("opencode.json"),json!({"mcp":{"local":{"type":"local","command":["/bin/true"],"environment":{"LOCAL_TOKEN":"mcp09-explicit-local-canary"}}}}).to_string()).unwrap();
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
            .all(|(_, value)| !value.contains("mcp09-higher-domain-canary"))
    );
    fs::write(project.join("opencode.json"),json!({
        "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":"https://example.invalid/v1","apiKey":"local-key"},"models":{"main":{}}}},
        "mcp":{"local":{"type":"local","command":["/bin/true"]}}
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
            .all(|(_, value)| !value.contains("mcp09-higher-domain-canary")),
        "whole-entry replacement cannot erase the higher source's credential domain"
    );
    let nested = global.join("lower-location");
    fs::create_dir(&nested).unwrap();
    fs::write(
        nested.join("opencode.json"),
        json!({"mcp":{"local":{"type":"local","command":["/bin/true"]}}}).to_string(),
    )
    .unwrap();
    let composition = load_with_env(&nested, env.clone()).await.unwrap();
    let entry = &composition.generation.mcp["local"];
    assert!(
        !entry.inherit_credentials,
        "global path prefix is not global source authority"
    );
    let config = crate::mcp_stdio::StdioConfig::from_entry("local", entry, &nested, &env).unwrap();
    assert!(
        config
            .extra_env
            .iter()
            .all(|(_, value)| !value.contains("mcp09-higher-domain-canary"))
    );
}
