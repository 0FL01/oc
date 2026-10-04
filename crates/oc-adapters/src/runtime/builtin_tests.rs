use super::*;

fn entry() -> mcp_remote::RegistryEntry {
    mcp_remote::RegistryEntry {
        server: "arbitrary".into(),
        tool: "rotate_widget".into(),
        namespaced: "arbitrary__rotate_widget".into(),
        description: None,
        input_schema: serde_json::json!({"type":"object"}),
    }
}

#[test]
fn registered_mcp_default_and_alias_visibility_obey_independent_constraints() {
    let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
    let entries = [entry()];
    let policy = RuntimePolicy::new(&permissions).with_mcp(&entries);
    assert_eq!(
        policy.effect(&entries[0].namespaced, "*"),
        Permission::Allow
    );
    assert!(policy.tool_visible(&entries[0].namespaced));
    assert_eq!(policy.effect("arbitrary__unknown", "*"), Permission::Deny);
    assert_eq!(policy.effect("unknown_native", "*"), Permission::Deny);
    for action in [
        "arbitrary__rotate_widget",
        "arbitrary_rotate_widget",
        "arbitrary_*",
    ] {
        let rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({
            "permission": {action: "deny"}
        }))
        .unwrap();
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_mcp(&entries);
        assert_eq!(policy.effect(&entries[0].namespaced, "*"), Permission::Deny);
        assert!(!policy.tool_visible(&entries[0].namespaced));
    }
}

#[test]
fn profiles_and_parent_ceiling_narrow_registered_default_before_preview_or_dispatch() {
    let permissions = BTreeMap::from([("*".into(), Permission::Allow)]);
    let entries = [entry()];
    for agent in crate::defs::builtin_agents() {
        let mut rules = crate::permissions::PermissionRules::default();
        rules.narrow(&permissions, &agent.permissions, &agent.permission_rules);
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_mcp(&entries);
        assert_eq!(
            policy.effect(&entries[0].namespaced, "*"),
            Permission::Allow,
            "{}",
            agent.id
        );
        assert!(policy.tool_visible(&entries[0].namespaced));
        if agent.id == "explore" {
            for tool in ["apply_patch", "edit", "write", "bash"] {
                assert_eq!(
                    policy.effect(tool, "file"),
                    Permission::Deny,
                    "{} {tool}",
                    agent.id
                );
                assert!(!policy.tool_visible(tool));
            }
        }
        if agent.id == "plan" {
            for tool in ["apply_patch", "edit", "write"] {
                assert_eq!(policy.effect(tool, "file"), Permission::Deny);
                assert!(policy.tool_visible(tool), "narrow plan-file schema {tool}");
            }
            assert_eq!(policy.effect("bash", "file"), Permission::Allow);
        }
        if agent.id == "general" || agent.id == "explore" {
            for tool in [
                "question",
                "subagent",
                "opencode_session_rename",
                "opencode_session_move",
            ] {
                assert_eq!(
                    policy.effect(tool, "*"),
                    Permission::Deny,
                    "{} {tool}",
                    agent.id
                );
            }
        }
        let parent = crate::permissions::PermissionRules::from_config(
            &serde_json::json!({"permission":{"arbitrary_*":"deny"}}),
        )
        .unwrap();
        rules.extend(parent);
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_mcp(&entries);
        assert!(!policy.tool_visible(&entries[0].namespaced));
        assert_eq!(policy.effect(&entries[0].namespaced, "*"), Permission::Deny);
    }
    let mut rules = crate::permissions::PermissionRules::default();
    let agents = crate::defs::builtin_agents();
    rules.narrow(
        &permissions,
        &agents[1].permissions,
        &agents[1].permission_rules,
    );
    for child in &agents[2..] {
        let mut child_rules = rules.clone();
        child_rules.narrow(&permissions, &child.permissions, &child.permission_rules);
        let policy = RuntimePolicy::with_rules(&permissions, &child_rules).with_mcp(&entries);
        assert_eq!(policy.effect("write", "file"), Permission::Deny);
        assert_eq!(
            policy.effect(&entries[0].namespaced, "*"),
            Permission::Allow
        );
    }
}

#[test]
fn profile_explicit_resources_ask_and_tools_false_never_receive_builtin_exception() {
    let permissions = BTreeMap::from([("read".into(), Permission::Allow)]);
    let entries = [entry()];
    let explore = crate::defs::builtin_agents()
        .into_iter()
        .find(|agent| agent.id == "explore")
        .unwrap();
    for (config, allowed, blocked, visible) in [
        (
            serde_json::json!({"permission":{"arbitrary_rotate_widget":"ask"}}),
            Permission::Ask,
            Permission::Ask,
            true,
        ),
        (
            serde_json::json!({"permission":{"arbitrary_*":{"approved":"allow","blocked":"deny"}}}),
            Permission::Allow,
            Permission::Deny,
            true,
        ),
        (
            serde_json::json!({"tools":{"arbitrary_rotate_widget":false}}),
            Permission::Deny,
            Permission::Deny,
            false,
        ),
        (
            serde_json::json!({"permission":{"*":"deny"}}),
            Permission::Deny,
            Permission::Deny,
            false,
        ),
    ] {
        let mut rules = crate::permissions::PermissionRules::default();
        let mut profile = explore.permission_rules.clone();
        profile.extend(crate::permissions::PermissionRules::from_config(&config).unwrap());
        rules.narrow(&permissions, &explore.permissions, &profile);
        let policy = RuntimePolicy::with_rules(&permissions, &rules).with_mcp(&entries);
        assert_eq!(policy.effect(&entries[0].namespaced, "approved"), allowed);
        assert_eq!(policy.effect(&entries[0].namespaced, "blocked"), blocked);
        assert_eq!(policy.tool_visible(&entries[0].namespaced), visible);
        if blocked != Permission::Allow {
            assert!(policy.check(&entries[0].namespaced).is_err());
        }
    }
    // Pending/failed/disabled views contain no registry entries. A name alone
    // cannot acquire Explore's approved MCP difference or the default grant.
    let mut rules = crate::permissions::PermissionRules::default();
    rules.narrow(
        &permissions,
        &explore.permissions,
        &explore.permission_rules,
    );
    let policy = RuntimePolicy::with_rules(&permissions, &rules);
    assert_eq!(policy.effect(&entries[0].namespaced, "*"), Permission::Deny);
}

#[test]
fn preview_resolves_child_model_before_selecting_the_captured_file_family() {
    let permissions = BTreeMap::from([("*".into(), Permission::Allow)]);
    let entries = [entry()];
    let policy = RuntimePolicy::new(&permissions).with_mcp(&entries);
    let mut agent = SubagentAgent {
        id: "general".into(),
        description: "General".into(),
        primary: false,
        model: Some("gpt-provider/future-plain".into()),
        variant: None,
        prompt: String::new(),
        permissions: BTreeMap::new(),
        permission_rules: Default::default(),
        hidden: false,
        digest: None,
    };
    let models = ModelCatalog {
        provider: "gpt-provider".into(),
        models: BTreeMap::from([("future-plain".into(), serde_json::json!({}))]),
    };
    let catalog = |agent: SubagentAgent| SubagentCatalog {
        agents: BTreeMap::from([(agent.id.clone(), agent)]),
        depth_limit: 1,
    };
    let preview = subagent_tool_def(
        &catalog(agent.clone()),
        &policy,
        "gpt-future",
        &models,
        None,
    );
    let preview = preview.description;
    let capabilities = preview
        .split("Effective permission preview: ")
        .nth(1)
        .unwrap();
    let capabilities = capabilities
        .split(". Resource/")
        .next()
        .unwrap()
        .split(", ")
        .collect::<Vec<_>>();
    assert!(capabilities.contains(&"write") && capabilities.contains(&"edit"));
    assert!(!capabilities.contains(&"apply_patch"));
    assert!(capabilities.contains(&"arbitrary__rotate_widget"));
    agent.model = Some("gpt-provider/retired".into());
    let preview = subagent_tool_def(&catalog(agent), &policy, "gpt-future", &models, None);
    assert!(preview.description.contains("preview unavailable"));
}
