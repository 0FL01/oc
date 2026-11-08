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
fn tool_use_public_actions_mcp_alias_and_each_checked_resource_are_hard_denied() {
    let permissions = BTreeMap::from([("*".into(), Permission::Allow)]);
    let entries = [entry()];
    let mut rules = crate::permissions::PermissionRules::default();
    rules.set_tool_use(vec![
        ("edit:protected".into(), false),
        ("shell:printf protected".into(), false),
        ("subagent:protected".into(), false),
        ("arbitrary_rotate_widget:protected".into(), false),
        ("arbitrary__rotate_widget:wire-only".into(), false),
        ("read:protected".into(), false),
    ]);
    let policy = RuntimePolicy::with_rules(&permissions, &rules).with_mcp(&entries);
    for action in [
        "edit",
        "write",
        "apply_patch",
        "patch",
        "image_output",
        "subagent",
        "task",
        "read",
        "arbitrary__rotate_widget",
    ] {
        assert_eq!(
            policy.effect(action, "protected"),
            Permission::Deny,
            "{action}"
        );
        assert_eq!(
            policy.effect(action, "public"),
            Permission::Allow,
            "{action}"
        );
    }
    assert_eq!(policy.effect("shell", "printf protected"), Permission::Deny);
    assert_eq!(policy.effect("bash", "printf protected"), Permission::Deny);
    assert_eq!(
        policy.effect(&entries[0].namespaced, "wire-only"),
        Permission::Allow
    );
    let call = crate::tools::ToolCall {
        id: "call".into(),
        name: "apply_patch".into(),
        arguments: serde_json::json!({"patchText":"*** Begin Patch\n*** Add File: public\n+safe\n*** Add File: protected\n+denied\n*** End Patch\n"}),
    };
    assert!(matches!(
        policy.check_call(&call),
        Err(ToolError::Denied { .. })
    ));
    assert!(policy.search_path_denied("protected"));
    assert!(!policy.automatic_instruction_source_allowed("protected"));
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
            "permissions": [{"action": action, "resource": "*", "effect": "deny"}]
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
            &serde_json::json!({"permissions": [{"action": "arbitrary_*", "resource": "*", "effect": "deny"}]}),
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
            serde_json::json!({"permissions": [{"action": "arbitrary_rotate_widget", "resource": "*", "effect": "ask"}]}),
            Permission::Ask,
            Permission::Ask,
            true,
        ),
        (
            serde_json::json!({"permissions": [{"action": "arbitrary_*", "resource": "approved", "effect": "allow"}, {"action": "arbitrary_*", "resource": "blocked", "effect": "deny"}]}),
            Permission::Allow,
            Permission::Deny,
            true,
        ),
        (
            serde_json::json!({"permissions": [{"action": "arbitrary_rotate_widget", "resource": "*", "effect": "deny"}]}),
            Permission::Deny,
            Permission::Deny,
            false,
        ),
        (
            serde_json::json!({"permissions": [{"action": "*", "resource": "*", "effect": "deny"}]}),
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
