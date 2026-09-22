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
        r#"{"permissions":{"read":{"z/*":"allow","*":"deny","safe/*":"allow","safe/private/*":"deny"},"bash":{"git status":"allow"}}}"#,
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
        r#"{"permission":{"read":{"safe/*":"allow"},"edit":{"safe/*":"allow"},"bash":{"git status":"allow"},"task":{"review":"allow"},"mcp_*":{"*":"allow"}}}"#,
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
        r#"{"tools":{"bash":false},"permissions":[{"action":"*","resource":"*","effect":"deny"},{"action":"read","resource":"safe/*","effect":"allow"},{"action":"bash","resource":"*","effect":"allow"}]}"#,
    );
    let agent = config(r#"{"permission":{"*":"allow"}}"#);
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
fn source_boundaries_and_empty_central_authority_cannot_be_widened() {
    let sources = [
        Source {
            path: "global".into(),
            text: r#"{"permission":{"read":{"*":"deny","safe/*":"allow"}}}"#.into(),
            trusted: true,
        },
        Source {
            path: "local".into(),
            text: r#"{"permission":{"read":"allow"}}"#.into(),
            trusted: true,
        },
    ];
    let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
    let policy = RuntimePolicy::with_rules(&generation.permissions, &generation.permission_rules);
    assert_eq!(policy.effect("read", "private"), Permission::Deny);
    assert_eq!(policy.effect("read", "safe/file"), Permission::Allow);
    let mut empty = config("{}");
    let agent = config(r#"{"permission":"allow"}"#);
    empty.permission_rules.narrow(
        &empty.permissions,
        &agent.permissions,
        &agent.permission_rules,
    );
    assert_eq!(
        RuntimePolicy::with_rules(&empty.permissions, &empty.permission_rules)
            .effect("bash", "anything"),
        Permission::Deny
    );
}

#[test]
fn mcp_uses_registered_upstream_identity_and_star_resource() {
    let generation = config(
        r#"{"permission":{"test_search":{"*":"allow"},"test_write":{"some-query":"allow"}}}"#,
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
        r#"{"permission":{"read":{"$HOME/private/*":"deny","~/safe/*":"allow"},"bash":{"$HOME/private/*":"deny","git status *":"allow"}}}"#,
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
        r#"{"permission":{"read":{"*":"allow","secret*":"deny"},"edit":{"*":"allow","secret*":"deny"}}}"#,
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
        &serde_json::json!({"agent":{
            "hidden":{"hidden":true,"permission":"allow"},
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
        &serde_json::json!({"agent":{"hidden":{"disable":true}}}),
        "later.json",
    );
    assert!(!defs.agents.contains_key("hidden"));

    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("agents")).unwrap();
    std::fs::write(temp.path().join("agents/review.md"), "---\nhidden: true\npermission:\n  read:\n    z/*: allow\n    '*': deny\n    safe/*: allow\ntools:\n  bash: false\n---\nReview files.\n").unwrap();
    let loaded = load_definitions(&[DefRoot {
        dir: temp.path().to_path_buf(),
        origin: "test".into(),
    }]);
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let agent = &loaded.agents["review"];
    assert!(agent.hidden);
    let mut central = config(r#"{"permission":"allow"}"#);
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
