use super::*;
use std::os::unix::fs::symlink;

#[tokio::test]
async fn dcp_custom_prompts_precedence_fallback_and_captured_inputs() {
    let dir = tempfile::tempdir().unwrap();
    let global = dir.path().join("global");
    let project = dir.path().join("project");
    let local = project.join(".opencode/dcp-prompts/overrides");
    let overrides = global.join("dcp-prompts/overrides");
    std::fs::create_dir_all(&overrides).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(
        global.join("dcp.jsonc"),
        r#"{"experimental":{"customPrompts":true}}"#,
    )
    .unwrap();
    std::fs::write(overrides.join("system.md"), "GLOBAL_SYSTEM").unwrap();
    let env = BTreeMap::from([(
        "OPENCODE_CONFIG_DIR".into(),
        global.to_string_lossy().into_owned(),
    )]);
    let first = load_local_with_env(&project, env.clone()).await.unwrap();
    assert!(first.dcp_config.custom_prompts);
    assert_eq!(
        first.dcp_config.prompt_overrides.system.as_deref(),
        Some("<dcp-system-reminder>\nGLOBAL_SYSTEM\n</dcp-system-reminder>")
    );
    for root in [&global, &project.join(".opencode")] {
        assert!(
            first
                .watch_inputs
                .targets
                .iter()
                .any(|target| { target.path == root.join("dcp-prompts/overrides/system.md") })
        );
    }
    assert!(
        !project.join(".opencode").exists(),
        "admission wrote directories"
    );
    assert!(!global.join("dcp-prompts/defaults").exists());

    std::fs::create_dir_all(&local).unwrap();
    std::fs::write(local.join("system.md"), "LOCAL_SYSTEM").unwrap();
    std::fs::write(local.join("compress-range.md"), "LOCAL_TOOL").unwrap();
    let second = load_local_with_env(&project, env.clone()).await.unwrap();
    assert!(
        second
            .dcp_config
            .prompt_overrides
            .system
            .as_deref()
            .unwrap()
            .contains("LOCAL_SYSTEM")
    );
    assert_eq!(
        second.dcp_config.prompt_overrides.compress_range.as_deref(),
        Some("LOCAL_TOOL")
    );
    assert!(
        first
            .dcp_config
            .prompt_overrides
            .system
            .as_deref()
            .unwrap()
            .contains("GLOBAL_SYSTEM")
    );
    assert!(first.local_inputs != second.local_inputs);

    // An empty normalized override falls back with a source-qualified note.
    std::fs::write(local.join("system.md"), "<!-- no body -->").unwrap();
    let fallback = load_local_with_env(&project, env.clone()).await.unwrap();
    assert_eq!(
        fallback.dcp_config.prompt_overrides.system,
        first.dcp_config.prompt_overrides.system
    );
    assert!(fallback.tui_chrome.service_diagnostics.iter().any(|note| {
        note.code == ServiceCode::IgnoredSetting
            && note.source
                == config::mcp::safe_source_id(&local.join("system.md").to_string_lossy())
            && note.field == ["dcp", "prompts", "system.md"]
    }));
    // Deleting the override/default does not revive a previously captured body.
    std::fs::remove_file(local.join("system.md")).unwrap();
    let quiet = load_local_with_env(&project, env.clone()).await.unwrap();
    assert_eq!(fallback.dcp_config, quiet.dcp_config);
    assert!(
        fallback.local_inputs != quiet.local_inputs,
        "warning-only changes must publish instead of becoming reload no-ops"
    );
    std::fs::remove_file(overrides.join("system.md")).unwrap();
    let default = load_local_with_env(&project, env).await.unwrap();
    assert!(default.dcp_config.prompt_overrides.system.is_none());
    assert!(
        second
            .dcp_config
            .prompt_overrides
            .system
            .as_deref()
            .unwrap()
            .contains("LOCAL_SYSTEM")
    );
}

#[tokio::test]
async fn dcp_custom_prompts_refuse_unsafe_files_but_disabled_files_are_inert() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let overrides = project.join(".opencode/dcp-prompts/overrides");
    std::fs::create_dir_all(&overrides).unwrap();
    let config = project.join("dcp.jsonc");
    std::fs::write(&config, r#"{"experimental":{"customPrompts":true}}"#).unwrap();
    let path = overrides.join("system.md");
    let outside = dir.path().join("outside.md");
    std::fs::write(&outside, "NEVER_READ_EXTERNAL_PROMPT").unwrap();
    for (kind, expected) in [
        ("symlink", ServiceCode::TrustRefused),
        ("fifo", ServiceCode::InvalidDocument),
        ("utf8", ServiceCode::InvalidDocument),
        ("oversize", ServiceCode::CapacityExceeded),
    ] {
        match kind {
            "symlink" => symlink(&outside, &path).unwrap(),
            "fifo" => {
                let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
                // SAFETY: name is NUL-terminated and names an isolated fixture FIFO.
                assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            }
            "utf8" => std::fs::write(&path, [0xff]).unwrap(),
            "oversize" => std::fs::File::create(&path)
                .unwrap()
                .set_len(NATIVE_CONFIG_CAP as u64 + 1)
                .unwrap(),
            _ => unreachable!(),
        }
        let error = load_local_with_env(&project, BTreeMap::new())
            .await
            .err()
            .unwrap();
        let LoadFailure::Configuration(note) = error;
        assert_eq!(note.code, expected);
        assert_eq!(
            note.source,
            super::config::mcp::safe_source_id(&path.to_string_lossy())
        );
        assert_eq!(note.field, ["dcp", "prompts", "system.md"]);
        assert!(!note.to_string().contains("NEVER_READ_EXTERNAL_PROMPT"));
        std::fs::remove_file(&path).unwrap();
    }
    symlink(dir.path().join("missing-external"), &path).unwrap();
    std::fs::write(&config, r#"{"experimental":{"customPrompts":false}}"#).unwrap();
    let disabled = load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(disabled.dcp_config.prompt_overrides, Default::default());
    assert!(
        !disabled
            .watch_inputs
            .targets
            .iter()
            .any(|target| target.path == path)
    );
}
