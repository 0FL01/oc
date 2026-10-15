use super::*;
use crate::config::{Source, assemble};
use serde_json::{Value, json};

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/v2-config-semantics.json"
    ))
    .unwrap()
}

#[test]
fn cfg17_ordinary_rule_evaluator_asks_without_a_match() {
    for case in fixture()["permission_evaluator_cases"].as_array().unwrap() {
        let sources = [Source {
            path: "/fixture.json".into(),
            trusted: false,
            text: json!({"permissions":case["rules"]}).to_string(),
        }];
        let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
        assert_eq!(
            generation.permission_rules.evaluate(
                &generation.permissions,
                case["action"].as_str().unwrap(),
                case["resource"].as_str().unwrap(),
            ),
            serde_json::from_value::<Permission>(case["expected"].clone()).unwrap(),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn cfg17_shared_document_and_agent_permission_order() {
    let fixture = fixture();
    for case in fixture["permission_cases"].as_array().unwrap() {
        let documents = std::iter::once(fixture["permission_base"].clone())
            .chain(case["documents"].as_array().unwrap().iter().cloned())
            .collect::<Vec<_>>();
        let sources = documents
            .iter()
            .enumerate()
            .map(|(index, document)| Source {
                path: format!("/fixture-{index}.json"),
                trusted: false,
                text: document.to_string(),
            })
            .collect::<Vec<_>>();
        let generation = assemble(&sources, &BTreeMap::new(), None).unwrap();
        let mut defs = crate::defs::LoadedDefs::default();
        for (source, document) in sources.iter().zip(&documents) {
            crate::defs::merge_config_definitions(&mut defs, document, &source.path);
        }
        assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
        let agent = &defs.agents["fixture"];
        let mut rules = generation.permission_rules.clone();
        rules.configure_agent(
            &generation.permissions,
            &agent.permissions,
            &agent.permission_rules,
        );
        assert_eq!(
            rules.evaluate(
                &generation.permissions,
                case["action"].as_str().unwrap(),
                case["resource"].as_str().unwrap(),
            ),
            serde_json::from_value::<Permission>(case["expected"].clone()).unwrap(),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn cfg17_child_overrides_use_own_agent_but_cannot_widen_parent() {
    for (global, parent_allow, child_allow, expected) in [
        ("ask", false, true, Permission::Ask),
        ("deny", false, true, Permission::Deny),
        ("deny", true, false, Permission::Deny),
        ("deny", true, true, Permission::Allow),
    ] {
        let agent = |allow| {
            PermissionRules::from_document(&if allow {
                json!({"permissions":[{"action":"shell","resource":"*","effect":"allow"}]})
            } else {
                json!({})
            })
            .unwrap()
        };
        let source = PermissionRules::from_document(&json!({"permissions":[
            {"action":"shell","resource":"*","effect":global}
        ]}))
        .unwrap();
        let mut parent = source.clone();
        parent.configure_agent(&BTreeMap::new(), &BTreeMap::new(), &agent(parent_allow));
        let mut child = parent.clone();
        child.configure_child(&BTreeMap::new(), &BTreeMap::new(), &agent(child_allow));
        assert_eq!(
            child.evaluate(&BTreeMap::new(), "shell", "echo fixture"),
            expected,
            "global={global} parent_allow={parent_allow} child_allow={child_allow}"
        );
        // Restart/rebinding starts with current documents, then intersects the
        // captured actor before selecting the child's current profile.
        let mut rebound = source;
        rebound.narrow(&BTreeMap::new(), &BTreeMap::new(), &parent);
        rebound.configure_child(&BTreeMap::new(), &BTreeMap::new(), &agent(child_allow));
        assert_eq!(
            rebound.evaluate(&BTreeMap::new(), "shell", "echo fixture"),
            expected,
            "rebound global={global} parent_allow={parent_allow} child_allow={child_allow}"
        );
    }
}

#[test]
fn cfg17_independent_and_builtin_ceilings_survive_configured_allow() {
    let allow = json!({"permissions":[{"action":"*","resource":"*","effect":"allow"}]});
    let mut configured = PermissionRules::from_document(&allow).unwrap();
    configured.extend(
        PermissionRules::from_config(&json!({"permissions":[
            {"action":"shell","resource":"*","effect":"deny"}
        ]}))
        .unwrap(),
    );
    assert_eq!(
        configured.evaluate(&BTreeMap::new(), "shell", "echo fixture"),
        Permission::Deny
    );
    assert_eq!(
        PermissionRules::default().evaluate(&BTreeMap::new(), "shell", "echo fixture"),
        Permission::Deny
    );
    let mut selected = PermissionRules::from_document(&allow).unwrap();
    selected.configure_agent(
        &BTreeMap::new(),
        &BTreeMap::from([("shell".into(), Permission::Deny)]),
        &PermissionRules::from_document(&allow).unwrap(),
    );
    assert_eq!(
        selected.evaluate(&BTreeMap::new(), "shell", "echo fixture"),
        Permission::Deny,
        "ordinary agent documents cannot erase a programmatic agent ceiling"
    );

    let mut defs = crate::defs::LoadedDefs::default();
    crate::defs::register_builtins(&mut defs);
    for id in ["plan", "explore"] {
        crate::defs::merge_config_definitions(
            &mut defs,
            &json!({"agents":{
                (id): {"permissions":allow["permissions"]}
            }}),
            "/fixture.json",
        );
        let agent = &defs.agents[id];
        let mut rules = PermissionRules::from_document(&allow).unwrap();
        rules.configure_agent(
            &BTreeMap::new(),
            &agent.permissions,
            &agent.permission_rules,
        );
        assert_eq!(
            rules.evaluate(&BTreeMap::new(), "apply_patch", "src/lib.rs"),
            Permission::Deny,
            "{id}"
        );
    }
}
