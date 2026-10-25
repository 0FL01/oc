use super::*;
use crate::composition::{load_local_with_env, load_reload_local_with_env};
use std::collections::BTreeMap;
use std::os::unix::fs::symlink;

fn has(inputs: &WatchInputs, path: impl Into<PathBuf>, kind: InputKind) -> bool {
    inputs.targets.contains(&Target {
        path: path.into(),
        kind,
    })
}

#[tokio::test]
async fn hot02_watch_inputs_follow_actual_admission_and_keep_missing_roots_inert() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let global = temp.path().join("global");
    std::fs::create_dir(&project).unwrap();
    let mut env = BTreeMap::from([(
        "OPENCODE_CONFIG_DIR".into(),
        global.to_string_lossy().into_owned(),
    )]);
    env.insert("CAPTURED".into(), "SYNTHETIC_CAPTURED_VALUE".into());
    let bare = load_local_with_env(&project, env.clone()).await.unwrap();
    for root in [&global, &project, &project.join(".opencode")] {
        for name in ["opencode.json", "opencode.jsonc", "dcp.json", "dcp.jsonc"] {
            assert!(has(&bare.watch_inputs, root.join(name), InputKind::File));
        }
    }
    assert!(has(
        &bare.watch_inputs,
        global.join("agents"),
        InputKind::AgentTree
    ));
    assert!(has(
        &bare.watch_inputs,
        project.join(".opencode/skills"),
        InputKind::Skills
    ));
    assert!(!has(
        &bare.watch_inputs,
        project.join("agents"),
        InputKind::AgentTree
    ));
    assert!(!has(
        &bare.watch_inputs,
        project.join(".opencode/AGENTS.md"),
        InputKind::File
    ));
    assert!(!global.exists() && !project.join(".opencode").exists());
    assert!(std::fs::read_dir(&project).unwrap().next().is_none());

    // Actual source aliases are admitted once; the watcher must retain both the
    // known lexical names and the canonical targets that supplied the bytes.
    let global_real = temp.path().join("global-real");
    std::fs::create_dir(&global_real).unwrap();
    symlink(&global_real, &global).unwrap();
    std::fs::write(global_real.join("AGENTS.md"), "GLOBAL_INSTRUCTIONS").unwrap();
    let shared = project.join("shared");
    std::fs::create_dir(&shared).unwrap();
    for name in ["selected-key", "other-key", "mcp-key"] {
        std::fs::write(shared.join(name), "SYNTHETIC_KEY").unwrap();
    }
    let original = serde_json::json!({
        "model":"fixture/m", "providers":{
            "fixture":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{file:./selected-key}"},"models":{"m":{}}},
            "other":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{file:other-key}"},"models":{"n":{}}},
            "environment":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{env:CAPTURED}"},"models":{"n":{}}},
            "missing":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{file:later-key}"},"models":{"n":{}}},
            "escape":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{file:../NEVER_FOLLOW}"},"models":{"n":{}}},
            "skipped":{"settings":{"baseURL":"https://example.invalid/v1", "apiKey":"{file:NEVER_READ}"},"models":{}}
        },
        "mcp":{
            "servers": {"enabled": {"type":"local","command":["/bin/true"],"environment":{"TOKEN":"{file:mcp-key}"}}, "disabled": {"type":"local","disabled":true,"command":["{file:NEVER_READ_DISABLED}"]}}
        },
        "experimental": {"policies": [{"action": "provider.use", "resource": "opencode-go", "effect": "deny"}, {"action": "provider.use", "resource": "openai", "effect": "deny"}, {"action": "provider.use", "resource": "skipped", "effect": "deny"}]},
        "dcp": {"experimental": {"customPrompts": true}}
    });
    std::fs::write(shared.join("config.json"), original.to_string()).unwrap();
    symlink(shared.join("config.json"), project.join("opencode.json")).unwrap();
    let native = project.join("native-inputs");
    std::fs::create_dir_all(native.join("agents")).unwrap();
    std::fs::create_dir_all(native.join("shared-agents")).unwrap();
    std::fs::create_dir_all(native.join("skills")).unwrap();
    std::fs::create_dir_all(native.join("shared-skill")).unwrap();
    symlink(&native, project.join(".opencode")).unwrap();
    symlink("../shared-agents", native.join("agents/nested")).unwrap();
    symlink("../shared-skill", native.join("skills/kit")).unwrap();
    std::fs::write(
        native.join("shared-agents/worker.md"),
        "---\nmode: subagent\n---\nWORKER",
    )
    .unwrap();
    // Native sidecars retain their stricter final-component no-follow contract.
    std::fs::write(native.join("dcp.json"), "{}").unwrap();
    std::fs::create_dir_all(native.join("dcp-prompts/overrides")).unwrap();
    std::fs::write(
        native.join("dcp-prompts/overrides/system.md"),
        "NATIVE_DCP_SYSTEM",
    )
    .unwrap();
    std::fs::write(project.join("cli.json"), "{}").unwrap();
    let loaded = load_reload_local_with_env(&project, env).await.unwrap();
    let inputs = &loaded.watch_inputs;
    for file in [
        shared.join("config.json"),
        shared.join("selected-key"),
        shared.join("other-key"),
        shared.join("later-key"),
        shared.join("mcp-key"),
        native.join("dcp.json"),
        global_real.join("AGENTS.md"),
        native.join("dcp-prompts/overrides/system.md"),
        native.join("dcp-prompts/overrides/turn-nudge.md"),
        global_real.join("dcp-prompts/overrides/system.md"),
    ] {
        assert!(
            has(inputs, &file, InputKind::File),
            "missing admitted target {}",
            file.display()
        );
    }
    for never in [
        shared.join("NEVER_READ"),
        shared.join("NEVER_READ_DISABLED"),
        project.join("NEVER_FOLLOW"),
        project.join(".env"),
        shared.join(".env"),
        project.join("cli.json"),
    ] {
        assert!(!has(inputs, &never, InputKind::File));
    }
    assert!(has(
        inputs,
        native.join("shared-agents"),
        InputKind::AgentTree
    ));
    assert!(has(
        inputs,
        native.join("shared-agents/worker.md"),
        InputKind::File
    ));
    assert!(has(
        inputs,
        native.join("shared-skill"),
        InputKind::SkillDirectory
    ));
    assert!(loaded.agents.contains_key("nested/worker"));
    assert_eq!(loaded.provider.api_key, "SYNTHETIC_KEY");
    assert_eq!(
        loaded.provider_for("other").unwrap().api_key,
        "SYNTHETIC_KEY"
    );
    assert_eq!(
        loaded.provider_for("environment").unwrap().api_key,
        "SYNTHETIC_CAPTURED_VALUE"
    );
    assert!(inputs.targets.iter().all(|target| {
        !target
            .path
            .to_string_lossy()
            .contains("SYNTHETIC_CAPTURED_VALUE")
    }));
    assert!(!shared.join("later-key").exists());
    assert!(!native.join("shared-skill/SKILL.md").exists());
}

#[test]
fn hot02_watch_observation_uses_the_open_descriptor_and_bounds_topology() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original");
    let moved = temp.path().join("moved");
    let outside = temp.path().join("outside");
    std::fs::write(&original, "ADMITTED").unwrap();
    std::fs::write(&outside, "NEVER_READ").unwrap();
    let pinned = File::open(&original).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    symlink(&outside, &original).unwrap();
    let mut inputs = WatchInputs::default();
    inputs.opened(&original, &pinned, InputKind::File);
    inputs.validate().unwrap();
    assert!(has(&inputs, original, InputKind::File));
    assert!(has(&inputs, moved, InputKind::File));
    assert!(!has(&inputs, outside, InputKind::File));
    let mut bounded = WatchInputs::default();
    for _ in 0..MAX_TARGETS + 1 {
        bounded.file(Path::new("same"));
    }
    bounded.validate().unwrap();
    assert_eq!(bounded.targets.len(), 1);
    for index in 0..MAX_TARGETS + 1 {
        bounded.file(&PathBuf::from(format!("target-{index}")));
    }
    let LoadFailure::Configuration(error) = bounded.validate().unwrap_err();
    assert_eq!(error.code, ServiceCode::CapacityExceeded);
    assert_eq!(bounded.targets.len(), MAX_TARGETS);
    let mut byte_bound = WatchInputs::default();
    byte_bound.file(&PathBuf::from("PRIVATE_CANARY".repeat(MAX_PATH_BYTES / 10)));
    let error = byte_bound.validate().unwrap_err();
    assert!(!error.to_string().contains("PRIVATE_CANARY"));
    assert!(byte_bound.targets.is_empty());
}
