use std::collections::BTreeMap;

use oc_adapters::config::{Permission, Source, assemble};
use oc_adapters::runtime::RuntimePolicy;
use oc_adapters::tools::{ToolCall, ToolPolicy};

fn config(text: &str) -> oc_adapters::config::Generation {
    assemble(
        &[Source {
            path: "test.json".into(),
            text: text.into(),
            trusted: true,
        }],
        &BTreeMap::new(),
        None,
    )
    .unwrap()
}

#[test]
fn resource_maps_are_ordered_and_unmatched_never_allow() {
    let generation = config(
        r#"{"permissions":[{"action":"read","resource":"z/*","effect":"allow"},{"action":"read","resource":"*","effect":"deny"},{"action":"read","resource":"safe/*","effect":"allow"},{"action":"read","resource":"safe/private/*","effect":"deny"},{"action":"shell","resource":"git status","effect":"allow"}]}"#,
    );
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    assert_eq!(policy.effect("read", "z/file"), Permission::Deny);
    assert_eq!(policy.effect("read", "safe/file"), Permission::Allow);
    assert_eq!(policy.effect("read", "safe/private/key"), Permission::Deny);
    assert_eq!(policy.effect("bash", "git status"), Permission::Allow);
    assert_eq!(policy.effect("bash", "rm -rf x"), Permission::Ask);
    assert!(
        policy
            .check_resource("bash", "rm -rf x")
            .unwrap_err()
            .to_string()
            .contains("approval")
    );
}

#[test]
fn actual_call_resources_include_patch_rename_and_mcp_star() {
    let generation = config(
        r#"{"permissions": [{"action": "read", "resource": "safe/*", "effect": "allow"}, {"action": "edit", "resource": "safe/*", "effect": "allow"}, {"action": "shell", "resource": "git status", "effect": "allow"}, {"action": "subagent", "resource": "review", "effect": "allow"}, {"action": "mcp_*", "resource": "*", "effect": "allow"}]}"#,
    );
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    let check = |name: &str, arguments| {
        policy.check_call(&ToolCall {
            id: "c".into(),
            name: name.into(),
            arguments,
        })
    };
    assert!(check("read", serde_json::json!({"path":"safe/file"})).is_ok());
    assert!(check("read", serde_json::json!({"path":"safe/../private"})).is_err());
    assert!(check("bash", serde_json::json!({"argv":["git","status"]})).is_ok());
    assert!(
        check(
            "bash",
            serde_json::json!({"argv":["sh","-c","git status; touch marker"]})
        )
        .is_err()
    );
    assert!(check("subagent", serde_json::json!({"agent":"review"})).is_ok());
    assert!(check("subagent", serde_json::json!({"agent":"build"})).is_err());
    assert!(check("mcp_search", serde_json::json!({"query":"anything"})).is_ok());
    assert!(check("apply_patch", serde_json::json!({"patchText":"*** Begin Patch\n*** Update File: safe/a\n*** Move to: private\n@@\n-old\n+new\n*** End Patch"})).is_err());
}

#[test]
fn native_order_tools_and_agent_constraints_never_widen_central() {
    let mut generation = config(
        r#"{"permissions":[{"action":"*","resource":"*","effect":"deny"},{"action":"read","resource":"safe/*","effect":"allow"},{"action":"shell","resource":"*","effect":"allow"}, {"action": "shell", "resource": "*", "effect": "deny"}]}"#,
    );
    let agent = config(r#"{"permissions": [{"action": "*", "resource": "*", "effect": "allow"}]}"#);
    generation.permission_rules.narrow(
        &generation.permissions,
        &agent.permissions,
        &agent.permission_rules,
    );
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    assert_eq!(policy.effect("read", "safe/a"), Permission::Allow);
    assert_eq!(policy.effect("read", "private"), Permission::Deny);
    assert_eq!(policy.effect("bash", "git status"), Permission::Deny);
    assert_eq!(policy.effect("subagent", "review"), Permission::Deny);
}

#[test]
fn ordered_documents_override_but_independent_narrowing_cannot_grant() {
    let sources = [
        Source {
            path: "global".into(),
            text: r#"{"permissions": [{"action": "read", "resource": "*", "effect": "deny"}, {"action": "read", "resource": "safe/*", "effect": "allow"}]}"#.into(),
            trusted: true,
        },
        Source {
            path: "local".into(),
            text: r#"{"permissions": [{"action": "read", "resource": "*", "effect": "allow"}]}"#.into(),
            trusted: true,
        },
    ];
    let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    assert_eq!(policy.effect("read", "private"), Permission::Allow);
    assert_eq!(policy.effect("read", "safe/file"), Permission::Allow);
    let mut empty = config("{}");
    let agent = config(r#"{"permissions": [{"action": "*", "resource": "*", "effect": "allow"}]}"#);
    empty.permission_rules.narrow(
        &empty.permissions,
        &agent.permissions,
        &agent.permission_rules,
    );
    assert_eq!(
        RuntimePolicy::with_rules(&empty.permissions, &empty.permission_rules)
            .effect("bash", "anything"),
        Permission::Ask
    );
}

#[test]
fn mcp_uses_registered_upstream_identity_and_star_resource() {
    let generation = config(
        r#"{"permissions": [{"action": "test_search", "resource": "*", "effect": "allow"}, {"action": "test_write", "resource": "some-query", "effect": "allow"}]}"#,
    );
    let entries = oc_adapters::mcp_remote::map_registry(
        "test",
        vec![
            ("search".into(), None, serde_json::json!({})),
            ("write".into(), None, serde_json::json!({})),
        ],
    )
    .unwrap();
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules)
        .with_mcp(&entries);
    assert!(policy.check("test__search").is_ok());
    assert!(policy.check("test__write").is_err());
    assert!(policy.check("unknown__search").is_err());
}

#[test]
fn path_home_expansion_does_not_rewrite_shell_text_and_wildcards_match_upstream() {
    let mut generation = config(
        r#"{"permissions": [{"action": "read", "resource": "$HOME/private/*", "effect": "deny"}, {"action": "read", "resource": "~/safe/*", "effect": "allow"}, {"action": "shell", "resource": "$HOME/private/*", "effect": "deny"}, {"action": "shell", "resource": "git status *", "effect": "allow"}]}"#,
    );
    generation.permission_rules.expand_home("/home/fixture");
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    assert_eq!(
        policy.effect("read", "/home/fixture/private/key"),
        Permission::Deny
    );
    assert_eq!(
        policy.effect("read", "/home/fixture/safe/a"),
        Permission::Allow
    );
    assert_eq!(policy.effect("bash", "$HOME/private/run"), Permission::Deny);
    assert_eq!(policy.effect("bash", "git status"), Permission::Allow);
    assert_eq!(
        policy.effect("bash", "git status --short"),
        Permission::Allow
    );
    assert!(oc_adapters::permissions::wildcard("a/b", "a?b"));
    assert!(oc_adapters::permissions::wildcard("a/b/c", "a/*"));
    assert!(!oc_adapters::permissions::wildcard("a", "[a]"));
}

#[test]
fn literal_stars_in_resources_cannot_bypass_wildcard_denials() {
    let generation = config(
        r#"{"permissions": [{"action": "read", "resource": "*", "effect": "allow"}, {"action": "read", "resource": "secret*", "effect": "deny"}, {"action": "edit", "resource": "*", "effect": "allow"}, {"action": "edit", "resource": "secret*", "effect": "deny"}]}"#,
    );
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    for resource in [
        "secret*keys",
        "secret**keys",
        "secret*",
        "secret",
        "secret/keys",
    ] {
        assert!(oc_adapters::permissions::wildcard(resource, "secret*"));
        for action in ["read", "apply_patch"] {
            assert_eq!(
                policy.effect(action, resource),
                Permission::Deny,
                "{action} {resource}"
            );
        }
    }
    assert!(oc_adapters::permissions::wildcard("a*b/c", "a*b*"));
    assert!(!oc_adapters::permissions::wildcard("a*x/c", "a*b*"));
    assert_eq!(policy.effect("read", "public*keys"), Permission::Allow);
}

#[test]
fn metadata_and_markdown_preserve_ordered_narrowing() {
    use oc_adapters::defs::{DefRoot, LoadedDefs, load_definitions, merge_config_definitions};
    let mut defs = LoadedDefs::default();
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{
            "hidden":{"hidden":true, "permissions": [{"action": "*", "resource": "*", "effect": "allow"}]},
            "off":{"disabled":true},
            "invalid":{"hidden":"yes"}
        }}),
        "config.json",
    );
    assert!(defs.agents["hidden"].hidden);
    assert!(!defs.agents.contains_key("off"));
    assert!(!defs.agents.contains_key("invalid"));
    assert_eq!(defs.diagnostics.len(), 1);
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{"hidden":{"disabled":true}}}),
        "later.json",
    );
    assert!(!defs.agents.contains_key("hidden"));

    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("agents")).unwrap();
    std::fs::write(temp.path().join("agents/review.md"), "---\nhidden: true\npermissions:\n  - action: read\n    resource: z/*\n    effect: allow\n  - action: read\n    resource: '*'\n    effect: deny\n  - action: read\n    resource: safe/*\n    effect: allow\n  - action: shell\n    resource: '*'\n    effect: deny\n---\nReview files.\n").unwrap();
    let loaded = load_definitions(&[DefRoot {
        dir: temp.path().to_path_buf(),
        origin: "test".into(),
    }]);
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let agent = &loaded.agents["review"];
    assert!(agent.hidden);
    let mut central =
        config(r#"{"permissions": [{"action": "*", "resource": "*", "effect": "allow"}]}"#);
    central.permission_rules.narrow(
        &central.permissions,
        &agent.permissions,
        &agent.permission_rules,
    );
    let policy = RuntimePolicy::with_rules(&central.permissions, &central.permission_rules);
    assert_eq!(policy.effect("read", "z/file"), Permission::Deny);
    assert_eq!(policy.effect("read", "safe/file"), Permission::Allow);
    assert_eq!(policy.effect("bash", "echo hi"), Permission::Deny);
}
