use super::*;

#[test]
fn r6_disabled_then_readmitted_profile_gets_new_registration_order() {
    let mut defs = LoadedDefs::default();
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{"first":{"system":"first"},"second":{"system":"second"}}}),
        "global",
    );
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{"first":{"disabled":true}}}),
        "project",
    );
    assert!(!defs.agents.contains_key("first"));
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agents":{"first":{"system":"readmitted"}}}),
        "later",
    );
    assert_eq!(defs.order, ["agent.second@global", "agent.first@later"]);
}

#[test]
fn r6_recursive_source_order_nested_ids_and_flat_compatibility() {
    let temp = tempfile::tempdir().unwrap();
    for (file, text) in [
        (
            "agent/team/reviewer.md",
            "---\nmodel: fixture/org/model#fast\npermission:\n  read: deny\n---\nold",
        ),
        (
            "agents/team/reviewer.md",
            "---\ndescription: later\n---\nnew",
        ),
        ("agent/a.md", "A"),
        ("agents/z.md", "Z"),
        ("mode/compat.md", "---\nmode: subagent\n---\ncompat"),
        ("modes/compat.md", "---\nhidden: true\n---\ncompat later"),
        ("mode/nested/ignored.md", "ignored"),
    ] {
        let path = temp.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let defs = load_definitions(&[DefRoot {
        dir: temp.path().into(),
        origin: "global".into(),
    }]);
    assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
    assert_eq!(
        defs.order,
        [
            "agent.a@global",
            "agent.team/reviewer@global",
            "agent.z@global",
            "agent.compat@global"
        ]
    );
    let profile = &defs.agents["team/reviewer"];
    assert_eq!(profile.model.as_deref(), Some("fixture/org/model"));
    assert_eq!(profile.variant.as_deref(), Some("fast"));
    assert_eq!(profile.description, "later");
    assert_eq!(profile.body, "new");
    assert_eq!(profile.permissions["read"], Permission::Deny);
    assert_eq!(defs.agents["compat"].mode.as_deref(), Some("primary"));
    assert!(defs.agents["compat"].hidden);
    assert!(!defs.agents.contains_key("nested/ignored"));
}

#[test]
fn r6_model_selection_precedence_and_whole_selection_replacement() {
    for (model, separate, id, variant) in [
        (
            serde_json::json!("fixture/org/model#fast"),
            "slow",
            "fixture/org/model",
            Some("fast"),
        ),
        (
            serde_json::json!({"providerID":"fixture","model":"org/model","variant":"fast"}),
            "slow",
            "fixture/org/model",
            Some("fast"),
        ),
        (
            serde_json::json!({"providerID":"fixture","modelID":"org/model","variant":"fast"}),
            "slow",
            "fixture/org/model",
            Some("fast"),
        ),
        (
            serde_json::json!({"providerID":"fixture","model":"org/model"}),
            "slow",
            "fixture/org/model",
            None,
        ),
        (
            serde_json::json!("fixture/org/model"),
            "slow",
            "fixture/org/model",
            Some("slow"),
        ),
    ] {
        let mut defs = LoadedDefs::default();
        merge_config_definitions(
            &mut defs,
            &serde_json::json!({"agents":{"team/reviewer":{"model":model,"variant":separate,"system":"own"}}}),
            "global",
        );
        assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
        assert_eq!(defs.agents["team/reviewer"].model.as_deref(), Some(id));
        assert_eq!(defs.agents["team/reviewer"].variant.as_deref(), variant);
        merge_config_definitions(
            &mut defs,
            &serde_json::json!({"agent":{"team/reviewer":{"model":"fixture/replacement"}}}),
            "project",
        );
        assert_eq!(defs.agents["team/reviewer"].variant, None);
        assert_eq!(defs.agents["team/reviewer"].body, "own");
    }
}

#[test]
fn r6_markdown_native_structured_variant_and_nested_path_safety() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("agents/nested")).unwrap();
    std::fs::write(temp.path().join("agents/nested/own.md"), "---\nmodel:\n  providerID: fixture\n  model: org/model\n  variant: fast\nvariant: slow\n---\n  own body  \n").unwrap();
    std::fs::write(outside.path().join("escape.md"), "EXTERNAL_BODY").unwrap();
    std::os::unix::fs::symlink(outside.path(), temp.path().join("agents/escape")).unwrap();
    std::os::unix::fs::symlink(
        temp.path().join("agents"),
        temp.path().join("agents/nested/loop"),
    )
    .unwrap();
    let defs = load_definitions(&[DefRoot {
        dir: temp.path().into(),
        origin: "project".into(),
    }]);
    assert_eq!(defs.agents.len(), 1);
    assert_eq!(defs.agents["nested/own"].variant.as_deref(), Some("fast"));
    assert_eq!(defs.agents["nested/own"].body, "own body");
    assert!(
        defs.diagnostics
            .iter()
            .any(|d| d.failure.code == oc_core::queries::ServiceCode::TrustRefused)
    );
    assert!(
        !defs
            .agents
            .values()
            .any(|d| d.body.contains("EXTERNAL_BODY"))
    );
}
