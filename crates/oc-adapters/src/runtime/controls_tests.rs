use super::*;

#[test]
fn dcp12_direct_guards_have_zero_intents_and_reenable_keeps_projection_permission() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let runtime = Runtime::new(
        &db,
        "work",
        Generation {
            permissions: BTreeMap::from([("compress".into(), Permission::Allow)]),
            ..Default::default()
        },
        ProtectedGlobs { patterns: vec![] },
        crate::files::Files::new(project.path(), data.path()).unwrap(),
        crate::shell::Shell::new(project.path()).unwrap(),
        BTreeMap::new(),
        ToolRoots {
            project: project.path().into(),
            data: data.path().into(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    runtime.create_session("s").unwrap();
    let first = db
        .append_message("s", "user", &"old user ".repeat(100))
        .unwrap();
    let end = db
        .append_message("s", "assistant", &"closed ".repeat(100))
        .unwrap();
    db.append_message("s", "user", "current").unwrap();
    let args = serde_json::json!({"topic":"guards","content":[{"startId":first,"endId":end,"summary":"short kept"}]});
    let raw = db.read_history_full("s").unwrap();
    for config in [
        DcpConfig {
            enabled: false,
            ..Default::default()
        },
        DcpConfig {
            compress_enabled: false,
            ..Default::default()
        },
        DcpConfig {
            manual_mode: true,
            ..Default::default()
        },
        DcpConfig {
            compress_permission: Some(Permission::Deny),
            ..Default::default()
        },
        DcpConfig {
            compress_permission: Some(Permission::Ask),
            ..Default::default()
        },
    ] {
        runtime.reload_dcp(config).unwrap();
        assert!(
            runtime
                .run_compress("s", &args, &ProtectedSpec::default())
                .is_err()
        );
        assert_eq!(db.tool_ops_len("s").unwrap(), 0);
        assert_eq!(db.dcp_block_count("s").unwrap(), 0);
        assert_eq!(db.read_history_full("s").unwrap(), raw);
    }
    runtime
        .reload_dcp(DcpConfig {
            commands_enabled: false,
            ..Default::default()
        })
        .unwrap();
    runtime
        .run_compress("s", &args, &ProtectedSpec::default())
        .unwrap();
    let hot = runtime.active_projection("s").unwrap().projected;
    runtime
        .reload_dcp(DcpConfig {
            compress_enabled: false,
            compress_permission: Some(Permission::Ask),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(runtime.active_projection("s").unwrap().projected, hot);
    runtime
        .reload_dcp(DcpConfig {
            compress_permission: Some(Permission::Ask),
            ..Default::default()
        })
        .unwrap();
    assert!(
        runtime
            .run_compress("s", &args, &ProtectedSpec::default())
            .is_err()
    );
    assert_eq!(runtime.active_projection("s").unwrap().projected, hot);
    assert_eq!(db.tool_ops_len("s").unwrap(), 1);
    assert_eq!(db.read_history_full("s").unwrap(), raw);
    runtime
        .reload_dcp(DcpConfig {
            allow_subagents: false,
            ..Default::default()
        })
        .unwrap();
    db.create_child_session("s", "child", None, None, None)
        .unwrap();
    let child = runtime.compression_availability("child").unwrap();
    assert_eq!(
        child.ordinary_refusal,
        // An unprofiled legacy child has no authority; Deny also wins opt-out.
        Some(oc_core::dcp_view::DcpUnavailable::Denied)
    );
    assert_eq!(child.manual_refusal, child.ordinary_refusal);
    assert!(runtime.admit_manual_compression("child").is_err());
    assert!(
        runtime
            .run_compress("child", &args, &ProtectedSpec::default())
            .is_err()
    );
    assert_eq!(db.tool_ops_len("child").unwrap(), 0);
    assert_eq!(db.dcp_block_count("child").unwrap(), 0);
}

#[test]
fn dcp12_native_protection_defaults_are_frozen_against_registered_tools() {
    let config = DcpConfig::default();
    assert!(
        config.protected_tools.is_empty()
            && config.dedup_protected_tools.is_empty()
            && config.purge_protected_tools.is_empty()
    );
    assert!(!config.protect_tags && !config.protect_user_messages && !config.turn_protection);
    let registered = builtin_tool_defs()
        .into_iter()
        .map(|t| t.name)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(registered.contains("skill"));
    assert!(
        !registered.contains("subagent"),
        "subagent is registered from the admitted profile catalog, not this static table"
    );
    for unsupported in ["task", "todowrite", "todoread", "execute"] {
        assert!(!registered.contains(unsupported));
    }
    // Declared native difference: command/compress shared protections default
    // empty; no fictitious donor tool defaults or silent scope/merge change.
    let (configured,_)=crate::dcp_auto::load_config(&serde_json::json!({"commands":{"protectedTools":["read"]},"compress":{"protectedTools":["skill"]}})).unwrap();
    assert_eq!(configured.protected_tools, ["read", "skill"]);
}

#[test]
fn dcp_custom_prompts_follow_tool_visibility_and_keep_native_schema_and_guidance() {
    let config = DcpConfig {
        custom_prompts: true,
        prompt_overrides: crate::dcp_auto::DcpPromptOverrides {
            system: Some("CUSTOM_SYSTEM".into()),
            compress_range: Some("CUSTOM_RANGE".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let original = builtin_tool_defs();
    let mut tools = original.clone();
    // Without overrides the built-in DCP 3.2.0 bodies apply, as in the donor.
    let Some(InputItem::Message { content, .. }) =
        dcp_prompt_guidance(&mut tools, &DcpConfig::default())
    else {
        panic!("default DCP system guidance");
    };
    assert!(
        serde_json::to_string(&content)
            .unwrap()
            .contains("THE PHILOSOPHY OF COMPRESS")
    );
    let described = tools
        .iter()
        .find(|tool| tool.name == COMPRESS_TOOL)
        .unwrap();
    assert!(
        described
            .description
            .starts_with("Collapse a range in the conversation")
    );
    assert!(
        described.description.ends_with(
            &*original
                .iter()
                .find(|tool| tool.name == COMPRESS_TOOL)
                .unwrap()
                .description
        )
    );
    let mut tools = original.clone();
    assert_eq!(
        dcp_prompt_guidance(&mut tools, &config),
        Some(InputItem::message(InputRole::Developer, "CUSTOM_SYSTEM"))
    );
    let tool = tools
        .iter()
        .find(|tool| tool.name == COMPRESS_TOOL)
        .unwrap();
    let baseline = original
        .iter()
        .find(|tool| tool.name == COMPRESS_TOOL)
        .unwrap();
    assert_eq!(tool.parameters, baseline.parameters);
    assert_eq!(
        tool.description,
        format!("CUSTOM_RANGE\n\n{}", baseline.description)
    );
    tools.retain(|tool| tool.name != COMPRESS_TOOL);
    assert!(dcp_prompt_guidance(&mut tools, &config).is_none());
}
