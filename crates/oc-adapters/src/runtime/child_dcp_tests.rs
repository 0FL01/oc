use super::*;

#[test]
fn dcp10_direct_child_query_and_api_cannot_use_root_authority() {
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
    runtime.create_session("root").unwrap();
    let mut profile = crate::defs::builtin_build();
    profile.id = "denied-child".into();
    profile.permission_rules = crate::permissions::PermissionRules::from_config(
        &serde_json::json!({"permission":{"compress":"deny"}}),
    )
    .unwrap();
    runtime
        .publish_subagents(Some(SubagentCatalog {
            agents: BTreeMap::from([(
                profile.id.clone(),
                SubagentAgent {
                    id: profile.id,
                    description: String::new(),
                    primary: false,
                    model: None,
                    variant: None,
                    prompt: String::new(),
                    permissions: profile.permissions,
                    permission_rules: profile.permission_rules,
                    hidden: false,
                    digest: None,
                },
            )]),
            depth_limit: 1,
        }))
        .unwrap();
    db.create_child_session("root", "child", Some("denied-child"), None, None)
        .unwrap();
    assert_eq!(
        runtime
            .compression_availability("child")
            .unwrap()
            .ordinary_refusal,
        Some(oc_core::dcp_view::DcpUnavailable::Denied)
    );
    assert_eq!(
        runtime
            .compression_availability("root")
            .unwrap()
            .ordinary_refusal,
        None
    );
    assert!(runtime.run_compress("child", &serde_json::json!({"topic":"bad","content":[{"startId":"alien","endId":"alien","summary":"bad"}]}), &ProtectedSpec::default()).is_err());
    assert_eq!(db.tool_ops_len("child").unwrap(), 0);
}

#[test]
fn dcp10_explore_preview_and_ceiling_follow_real_config_permission_and_consumer() {
    let models = ModelCatalog {
        provider: "fixture".into(),
        models: BTreeMap::from([("own-model".into(), serde_json::json!({}))]),
    };
    let builtin = crate::defs::builtin_agents()
        .into_iter()
        .find(|a| a.id == "explore")
        .unwrap();
    let explore = SubagentAgent {
        id: builtin.id,
        description: builtin.description,
        primary: false,
        model: Some("fixture/own-model".into()),
        variant: None,
        prompt: String::new(),
        permissions: builtin.permissions,
        permission_rules: builtin.permission_rules,
        hidden: false,
        digest: None,
    };
    for (permission, config, consumer, available) in [
        (Permission::Allow, DcpConfig::default(), false, true),
        (
            Permission::Allow,
            DcpConfig {
                allow_subagents: false,
                ..Default::default()
            },
            false,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                enabled: false,
                ..Default::default()
            },
            false,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                compress_enabled: false,
                ..Default::default()
            },
            false,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                manual_mode: true,
                ..Default::default()
            },
            false,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                compress_permission: Some(Permission::Deny),
                ..Default::default()
            },
            true,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                compress_permission: Some(Permission::Ask),
                ..Default::default()
            },
            false,
            false,
        ),
        (
            Permission::Allow,
            DcpConfig {
                compress_permission: Some(Permission::Ask),
                ..Default::default()
            },
            true,
            true,
        ),
        (Permission::Ask, DcpConfig::default(), true, true),
        (Permission::Deny, DcpConfig::default(), true, false),
    ] {
        let permissions = BTreeMap::from([
            ("*".into(), Permission::Allow),
            ("compress".into(), permission),
        ]);
        let mut rules = crate::permissions::PermissionRules::default();
        rules.narrow(
            &permissions,
            &explore.permissions,
            &explore.permission_rules,
        );
        let policy = RuntimePolicy::with_rules(&permissions, &rules);
        for forbidden in [
            "apply_patch",
            "edit",
            "write",
            "shell",
            "question",
            "subagent",
        ] {
            assert!(!policy.tool_visible(forbidden), "{forbidden}");
        }
        let catalog = SubagentCatalog {
            agents: BTreeMap::from([("explore".into(), explore.clone())]),
            depth_limit: 1,
        };
        let parent = RuntimePolicy::new(&permissions);
        let preview = subagent_tool_def(
            &catalog,
            &parent,
            "own-model",
            &models,
            None,
            &config,
            consumer,
        )
        .description;
        let names = preview
            .split("Effective permission preview: ")
            .nth(1)
            .unwrap()
            .split(". Resource/")
            .next()
            .unwrap()
            .split(", ")
            .collect::<Vec<_>>();
        assert_eq!(names.contains(&"compress"), available, "{preview}");
        if available
            && (permission == Permission::Ask
                || config.compress_permission == Some(Permission::Ask))
        {
            assert!(preview.contains("Own-session compression: requires approval"));
        }
        for custom in [
            serde_json::json!({"permission":{"compress":"deny"}}),
            serde_json::json!({"permission":{"*":"deny"}}),
        ] {
            let custom = crate::permissions::PermissionRules::from_config(&custom).unwrap();
            let mut narrowed = rules.clone();
            narrowed.narrow(&permissions, &BTreeMap::new(), &custom);
            assert_eq!(
                RuntimePolicy::with_rules(&permissions, &narrowed).effect("compress", "*"),
                Permission::Deny
            );
        }
    }
}

#[test]
fn dcp10_explore_own_compress_default_grant_fills_only_absent_authority() {
    let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
    let explore = crate::defs::builtin_agents()
        .into_iter()
        .find(|a| a.id == "explore")
        .unwrap();
    let mut rules = crate::permissions::PermissionRules::default();
    rules.narrow(
        &permissions,
        &explore.permissions,
        &explore.permission_rules,
    );
    assert_eq!(
        RuntimePolicy::with_rules(&permissions, &rules).effect("compress", "*"),
        Permission::Allow
    );
    for action in ["compress", "*"] {
        for effect in ["deny", "ask"] {
            let central = crate::permissions::PermissionRules::from_config(
                &serde_json::json!({"permission":{action:effect}}),
            )
            .unwrap();
            let mut narrowed = central;
            narrowed.narrow(
                &permissions,
                &explore.permissions,
                &explore.permission_rules,
            );
            assert_eq!(
                RuntimePolicy::with_rules(&permissions, &narrowed).effect("compress", "*"),
                if effect == "deny" {
                    Permission::Deny
                } else {
                    Permission::Ask
                }
            );
        }
    }
}

#[test]
fn dcp10_approval_selection_ignores_only_allocator_not_coverage_or_summary() {
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
    let mut plans = Vec::new();
    for session in ["first", "sibling"] {
        runtime.create_session(session).unwrap();
        let first = db
            .append_message(session, "user", &"closed ".repeat(100))
            .unwrap();
        let end = db
            .append_message(session, "assistant", &"past ".repeat(100))
            .unwrap();
        db.append_message(session, "user", "current").unwrap();
        let args = serde_json::json!({"topic":"own","content":[{"startId":first,"endId":end,"summary":"kept"}]});
        let (_, ranges) = crate::dcp::validate_range_args(&args).unwrap();
        plans.push((args, ranges));
    }
    let old = runtime
        .prepare_dcp_plan("first", &plans[0].1, &ProtectedSpec::default(), None)
        .unwrap();
    runtime
        .run_compress("sibling", &plans[1].0, &ProtectedSpec::default())
        .unwrap();
    let refreshed = runtime
        .prepare_dcp_plan("first", &plans[0].1, &ProtectedSpec::default(), None)
        .unwrap();
    assert_ne!(old.blocks[0].id, refreshed.blocks[0].id);
    assert!(old.same_approval_selection(&refreshed));
    let mut changed = refreshed.clone();
    changed.blocks[0].summary.push_str(" changed obligation");
    assert!(!old.same_approval_selection(&changed));
    changed = refreshed.clone();
    changed.blocks[0].end_msg = "foreign".into();
    assert!(!old.same_approval_selection(&changed));
    changed = refreshed;
    changed.projection_revision = old.projection_revision.map(|r| r + 1);
    assert!(!old.same_approval_selection(&changed));
}
