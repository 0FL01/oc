//! Independent regression specifications for the pinned OC audit.
//!
//! These tests were NOT compiled or run by the reviewer. They are intended
//! to fail against the audited implementation and pass after the repairs.
//! All filesystem mutations are confined to temporary test directories.
#![cfg(target_os = "linux")]

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use oc_adapters::config::{Permission, Source, assemble};
use oc_adapters::defs::{DefRoot, agent_digest, load_definitions};
use oc_adapters::discovery::{model_config, pretty_model_name};
use oc_adapters::mcp_remote::CodexWebConfig;
use oc_adapters::patch::{AllowAll, apply_patch};
use oc_adapters::storage::Db;
use oc_adapters::webfetch::html_to_text;
use sha2::{Digest as _, Sha256};

fn roots() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary workspace");
    let project = temp.path().join("project");
    let data = temp.path().join("data");
    fs::create_dir_all(&project).expect("project directory");
    fs::create_dir_all(&data).expect("data directory");
    (temp, project, data)
}

fn patch(project: &Path, data: &Path, body: &str) {
    apply_patch(project, data, body, &AllowAll).expect("valid patch");
}

#[test]
fn aud25_html_preserves_unicode_without_panicking() {
    let actual = html_to_text("<p>Привет 🦀</p>");
    assert!(actual.contains("Привет 🦀"), "Unicode text was corrupted");
}

#[test]
fn aud03_add_file_decodes_the_patch_plus_prefix() {
    let (_temp, project, data) = roots();
    patch(
        &project,
        &data,
        "*** Begin Patch\n*** Add File: added.txt\n+hello\n*** End Patch\n",
    );
    assert_eq!(fs::read(project.join("added.txt")).unwrap(), b"hello\n");
}

#[test]
fn aud04_predictable_temp_symlink_must_not_modify_outside_file() {
    let (temp, project, data) = roots();
    let outside = temp.path().join("outside-sentinel");
    fs::write(&outside, b"DO NOT TOUCH\n").unwrap();
    let old_temp_name = project.join(format!("new.txt.tmp-{}", std::process::id()));
    symlink(&outside, &old_temp_name).unwrap();
    let result = apply_patch(
        &project,
        &data,
        "*** Begin Patch\n*** Add File: new.txt\n+safe\n*** End Patch\n",
        &AllowAll,
    );
    assert_eq!(
        fs::read(&outside).unwrap(),
        b"DO NOT TOUCH\n",
        "patch followed a pre-existing temporary-file symlink",
    );
    // Either refuse safely or succeed with an independent regular file.
    if result.is_ok() {
        let metadata = fs::symlink_metadata(project.join("new.txt")).unwrap();
        assert!(metadata.is_file());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(fs::read(project.join("new.txt")).unwrap(), b"safe\n");
    }
}

#[test]
fn aud08_preexisting_orphan_blob_cannot_yield_an_unreadable_digest() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(temp.path()).unwrap();
    let bytes = b"durable blob payload";
    let digest = format!("{:x}", Sha256::digest(bytes));
    // Model a crash after file rename but before the blobs table insert.
    fs::write(temp.path().join("blobs").join(&digest), bytes).unwrap();
    match db.write_blob(bytes) {
        Ok(returned) => {
            assert_eq!(returned, digest);
            assert_eq!(db.read_blob(&returned).unwrap(), bytes);
        }
        // A typed refusal is preferable to a successful unreadable handle.
        Err(_) => {}
    }
}

#[test]
fn aud22_mcp_accepts_authorization_spelling_from_user_config() {
    let entry: oc_adapters::config::McpEntry = serde_json::from_value(serde_json::json!({
        "type": "remote",
        "url": "https://example.invalid/v1/mcp",
        "enabled": true,
        "oauth": false,
        "headers": {"Authorization": "Bearer fixture-not-a-secret"},
        "timeout": 60000
    }))
    .unwrap();
    // from_entry is a pure parse/validation operation: no HTTP is sent.
    let config = CodexWebConfig::from_entry(&entry).expect("original header spelling");
    assert_eq!(config.bearer, "fixture-not-a-secret");
}

#[test]
fn aud16_legacy_write_deny_narrows_apply_patch_allow() {
    let sources = [Source {
        path: "/fixture/opencode.jsonc".to_string(),
        text: r#"{"permissions":{"apply_patch":"allow","write":"deny"}}"#.to_string(),
        trusted: true,
    }];
    let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
    assert_eq!(generation.permissions.get("apply_patch"), Some(&Permission::Deny));
}

fn write_agent(root: &Path, body: &str) {
    fs::create_dir_all(root.join("agents")).unwrap();
    fs::write(
        root.join("agents/review.md"),
        format!("---\ndescription: unchanged description\nmode: primary\n---\n{body}\n"),
    )
    .unwrap();
}

#[test]
fn aud15_agent_body_changes_behavioral_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let root = DefRoot {
        dir: temp.path().to_path_buf(),
        origin: "fixture".to_string(),
    };
    write_agent(temp.path(), "Use policy A.");
    let first = load_definitions(std::slice::from_ref(&root));
    let first_digest = agent_digest(first.agents.get("review").unwrap());
    write_agent(temp.path(), "Use policy B.");
    let second = load_definitions(std::slice::from_ref(&root));
    let second_digest = agent_digest(second.agents.get("review").unwrap());
    assert_ne!(first_digest, second_digest, "agent prompt body was discarded");
}

#[test]
fn aud15_normal_markdown_is_not_shell_interpolation() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("commands")).unwrap();
    fs::write(
        temp.path().join("commands/check.md"),
        "---\ndescription: Review tests\n---\nExplain `cargo test` without running it.\n",
    )
    .unwrap();
    let defs = load_definitions(&[DefRoot {
        dir: temp.path().to_path_buf(),
        origin: "fixture".to_string(),
    }]);
    let command = defs.commands.get("check").expect("ordinary Markdown command");
    assert!(command.body.contains("`cargo test`"));
}

#[test]
fn aud18_discovery_rejects_the_first_unsafe_js_integer() {
    let row = serde_json::json!({
        "id": "fixture-model",
        "context_length": 9_007_199_254_740_992_u64,
        "max_completion_tokens": 16
    });
    assert!(model_config(&row).is_err(), "2^53 is not Number.isSafeInteger");
}

#[test]
fn aud18_model_name_follows_first_slash_user_oracle() {
    assert_eq!(pretty_model_name("vendor/route/model-x"), "Route/model X");
}
