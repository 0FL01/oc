use super::*;
use crate::{config::Permission, permissions::PermissionRules};
use std::collections::BTreeMap;

#[test]
fn plan_ordered_state_dedup_and_independent_native_projection() {
    let text = enter(Some("/fixture/home"));
    assert!(text.contains("unless the user explicitly asks"));
    assert!(text.contains("or ask a subagent"));
    assert_eq!(switched(Some("plan"), Some("plan"), None), None);
    assert_eq!(
        switched(None, Some("plan"), Some("/fixture/home")),
        Some(text.clone())
    );
    let mut history = vec![
        InputItem::message(InputRole::System, "CUSTOM_SYSTEM"),
        InputItem::message(InputRole::User, &text),
    ];
    assert_eq!(
        missing(&history, Some("plan"), Some("/fixture/home")),
        Some(text.clone())
    );
    history.push(InputItem::message(InputRole::System, &text));
    assert_eq!(missing(&history, Some("plan"), Some("/fixture/home")), None);
    assert_eq!(
        missing(&history, Some("build"), Some("/fixture/home")),
        Some(LEAVE.into())
    );
    history.push(InputItem::message(InputRole::System, LEAVE));
    assert_eq!(
        missing(&history, Some("build"), Some("/fixture/home")),
        None
    );
    assert_eq!(
        missing(&history, Some("plan"), Some("/fixture/home")),
        Some(text)
    );
}

#[test]
fn plan_baseline_grants_never_erase_explicit_central_profile_or_parent_rules() {
    let path = "/fixture/home/.opencode/plan/work.md";
    let mut plan = PermissionRules::plan_defaults();
    plan.expand_home("/fixture/home");
    for central in [
        serde_json::json!({}),
        serde_json::json!({"permission":{"*":"allow"}}),
    ] {
        let base = BTreeMap::new();
        let mut rules = PermissionRules::from_config(&central).unwrap();
        rules.narrow(&base, &BTreeMap::new(), &plan);
        for action in ["apply_patch", "edit", "write"] {
            assert_eq!(rules.evaluate(&base, action, path), Permission::Allow);
            assert_eq!(
                rules.evaluate(&base, action, "/fixture/home/other"),
                Permission::Deny
            );
            assert!(rules.actions_visible(&base, &[action], false));
        }
        assert_eq!(
            rules.evaluate(
                &base,
                "external_directory",
                "/fixture/home/.opencode/plan/*"
            ),
            Permission::Allow
        );
        if central == serde_json::json!({}) {
            assert_eq!(
                rules.evaluate(&base, "external_directory", "/fixture/home/unrelated/*"),
                Permission::Deny
            );
            assert_eq!(
                rules.evaluate(&base, "glob", "/fixture/home/*"),
                Permission::Deny
            );
        }
        for action in ["*", "edit", "apply_patch", "write", "external_directory"] {
            for effect in ["deny", "ask"] {
                let ceiling = PermissionRules::from_config(
                    &serde_json::json!({"permission":{action:{"*":effect}}}),
                )
                .unwrap();
                let mut narrowed = rules.clone();
                narrowed.extend(ceiling);
                let target_action = if action == "external_directory" {
                    action
                } else {
                    "write"
                };
                assert_eq!(
                    narrowed.evaluate(&base, target_action, path),
                    if effect == "deny" {
                        Permission::Deny
                    } else {
                        Permission::Ask
                    }
                );
            }
        }
    }
}

#[test]
fn plan_creation_and_selection_commit_with_immutable_reminder_rows() {
    let temp = tempfile::tempdir().unwrap();
    let db = crate::storage::Db::open(&temp.path().join("data")).unwrap();
    let text = enter(Some("/fixture/home"));
    db.create_bound_session_with_reminder("s", "/location", Some(&text))
        .unwrap();
    let original = db.read_history_full("s").unwrap();
    db.create_bound_session_with_reminder("s", "/location", Some(&text))
        .unwrap();
    assert_eq!(db.read_history_full("s").unwrap(), original);
    db.commit_session_agent_choice(&[("choice".into(), "build".into())], "s", Some(LEAVE))
        .unwrap();
    let raw = db.read_history_full("s").unwrap();
    assert_eq!(&raw[..1], original.as_slice());
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[1].2, LEAVE);
    assert!(!temp.path().join("home/.opencode/plan").exists());
}

#[test]
fn plan_location_contains_home_without_widening_ordinary_relative_paths() {
    let mut rules = PermissionRules::plan_defaults();
    rules.expand_home("/fixture/project/home");
    rules.bind_plan_project(std::path::Path::new("/fixture/project"));
    let base = BTreeMap::new();
    assert_eq!(
        rules.evaluate(&base, "write", "home/.opencode/plan/requested.md"),
        Permission::Allow
    );
    assert_eq!(
        rules.evaluate(&base, "write", "home/file"),
        Permission::Deny
    );
    assert_eq!(rules.evaluate(&base, "write", "file"), Permission::Deny);
    let mut missing_home = PermissionRules::plan_defaults();
    missing_home.bind_plan_project(std::path::Path::new("/fixture/project"));
    let centrally_allowed = BTreeMap::from([("apply_patch".into(), Permission::Allow)]);
    assert_eq!(
        missing_home.evaluate(&centrally_allowed, "write", "~/.opencode/plan/requested.md"),
        Permission::Deny,
        "unexpanded HOME is not a Location-relative Plan grant"
    );
    let rooted_glob = PermissionRules::from_config(&serde_json::json!({"permission":[
        {"action":"edit","resource":"*","effect":"allow"},
        {"action":"edit","resource":"/fixture/*/home/.opencode/plan/*","effect":"deny"}
    ]}))
    .unwrap();
    let mut captured = rooted_glob;
    captured.narrow(&base, &base, &rules);
    captured.bind_plan_project(std::path::Path::new("/fixture/project"));
    let policy = crate::runtime::RuntimePolicy::with_rules(&base, &captured)
        .with_root(std::path::Path::new("/fixture/project"));
    assert_eq!(
        policy.effect("write", "home/.opencode/plan/requested.md"),
        Permission::Deny,
        "rooted glob remains authoritative for relative native resources"
    );
    for effect in ["deny", "ask"] {
        let mut central = PermissionRules::from_config(&serde_json::json!({"permission":[
            {"action":"edit","resource":"*","effect":"allow"},
            {"action":"edit","resource":"/fixture/project/home/.opencode/plan/*","effect":effect}
        ]}))
        .unwrap();
        central.narrow(&base, &base, &rules);
        central.bind_plan_project(std::path::Path::new("/fixture/project"));
        let captured = central.clone();
        central.bind_plan_project(std::path::Path::new("/fixture/project"));
        assert_eq!(
            central, captured,
            "equivalent rules are not appended repeatedly"
        );
        let expected = if effect == "deny" {
            Permission::Deny
        } else {
            Permission::Ask
        };
        for action in ["apply_patch", "edit", "write"] {
            assert_eq!(
                central.evaluate(&base, action, "home/.opencode/plan/requested.md"),
                expected,
                "absolute {effect} survives Location-relative resource for {action}"
            );
        }
    }
}
