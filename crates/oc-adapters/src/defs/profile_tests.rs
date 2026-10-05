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

#[test]
fn r6_request_overlays_merge_by_key_with_legacy_migration_and_explicit_refusals() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("agents/team/tuned.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        "---\nrequest:\n  headers:\n    X-Team: md\n  body:\n    text:\n      verbosity: low\n---\nmd body",
    )
    .unwrap();
    let mut defs = load_definitions(&[DefRoot {
        dir: temp.path().into(),
        origin: "global".into(),
    }]);
    assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({
            "agent":{"legacy":{"options":{"top_p":0.1,"seed":7},"temperature":0.3,"top_p":0.9}},
            "agents":{"team/tuned":{"request":{"headers":{"x-other":"json"},"body":{"seed":1}}}}
        }),
        "project",
    );
    assert!(defs.diagnostics.is_empty(), "{:?}", defs.diagnostics);
    let legacy = &defs.agents["legacy"].request;
    assert!(legacy.headers.is_empty());
    assert_eq!(
        serde_json::Value::Object(legacy.body.clone()),
        serde_json::json!({"top_p":0.9,"seed":7,"temperature":0.3})
    );
    let tuned = &defs.agents["team/tuned"];
    assert_eq!(
        tuned.body, "md body",
        "request-only override keeps the body"
    );
    assert_eq!(
        tuned.request.headers,
        BTreeMap::from([
            ("x-other".to_string(), "json".to_string()),
            ("x-team".to_string(), "md".to_string()),
        ])
    );
    assert_eq!(
        serde_json::Value::Object(tuned.request.body.clone()),
        serde_json::json!({"text":{"verbosity":"low"},"seed":1})
    );
    assert!(
        !format!("{tuned:?}").contains("json\""),
        "header values stay out of Debug"
    );
    let before = agent_digest(tuned);
    let mut changed = tuned.clone();
    changed.request.body.insert("seed".into(), 2.into());
    assert_ne!(before, agent_digest(&changed));

    for (request, reason) in [
        (
            serde_json::json!({"settings":{}}),
            "request.settings: unsupported field",
        ),
        (
            serde_json::json!({"headers":{"Authorization":"x"}}),
            "reserved header",
        ),
        (
            serde_json::json!({"headers":{"bad name":"x"}}),
            "invalid header name",
        ),
        (serde_json::json!({"headers":{"x-n":1}}), "must be a string"),
        (
            serde_json::json!({"body":{"model":"other"}}),
            "request.body.model: reserved",
        ),
        (
            serde_json::json!({"body":{"reasoning":{}}}),
            "reserved request field",
        ),
        (
            serde_json::json!({"body":[]}),
            "request.body must be an object",
        ),
    ] {
        let mut defs = LoadedDefs::default();
        merge_config_definitions(
            &mut defs,
            &serde_json::json!({"agents":{"bad":{"request":request}}}),
            "global",
        );
        assert!(!defs.agents.contains_key("bad"), "{reason}");
        assert!(
            defs.diagnostics.iter().any(|d| d.reason.contains(reason)),
            "{reason}: {:?}",
            defs.diagnostics
        );
    }
    let mut defs = LoadedDefs::default();
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({"agent":{"bad":{"temperature":"hot"}}}),
        "global",
    );
    assert!(!defs.agents.contains_key("bad"));
}

#[test]
fn r6_color_native_pattern_legacy_theme_migration_and_supplied_merge() {
    let temp = tempfile::tempdir().unwrap();
    for (file, text) in [
        ("agents/native.md", "---\ncolor: \"#A1b2C3\"\n---\nn"),
        (
            "agents/legacy.md",
            "---\ncolor: accent\ntemperature: 0.1\n---\nl",
        ),
        ("agents/theme.md", "---\ncolor: accent\n---\nt"),
        ("agents/short.md", "---\ncolor: \"#abc\"\n---\ns"),
    ] {
        let path = temp.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let mut defs = load_definitions(&[DefRoot {
        dir: temp.path().into(),
        origin: "global".into(),
    }]);
    assert_eq!(defs.agents["native"].color.as_deref(), Some("#A1b2C3"));
    assert_eq!(defs.agents["legacy"].color.as_deref(), Some("#aaaaaa"));
    assert_eq!(
        defs.agents["legacy"].request.body["temperature"],
        serde_json::json!(0.1),
        "plain YAML numbers stay numbers"
    );
    for refused in ["theme", "short"] {
        assert!(!defs.agents.contains_key(refused), "{refused}");
    }
    assert_eq!(
        defs.diagnostics
            .iter()
            .filter(|d| d.reason.contains("color must be #RRGGBB"))
            .count(),
        2,
        "{:?}",
        defs.diagnostics
    );
    merge_config_definitions(
        &mut defs,
        &serde_json::json!({
            "agent":{"v1":{"color":"primary"}},
            "agents":{"native":{"description":"kept color"},"bad":{"color":"primary"}}
        }),
        "project",
    );
    assert_eq!(defs.agents["v1"].color.as_deref(), Some("#aaaaaa"));
    assert_eq!(defs.agents["native"].color.as_deref(), Some("#A1b2C3"));
    assert!(!defs.agents.contains_key("bad"));
}

#[test]
fn r6_frontmatter_plain_scalars_follow_yaml_core_numbers() {
    for (text, expected) in [
        ("7", serde_json::json!(7)),
        ("-0.5", serde_json::json!(-0.5)),
        ("1e3", serde_json::json!(1000.0)),
        ("\"7\"", serde_json::json!("7")),
        ("4o", serde_json::json!("4o")),
        ("1.2.3", serde_json::json!("1.2.3")),
        (".", serde_json::json!(".")),
        ("0x1f", serde_json::json!("0x1f")),
    ] {
        let (value, _) =
            crate::config::split_frontmatter_value(&format!("---\nk: {text}\n---\n")).unwrap();
        assert_eq!(value["k"], expected, "{text}");
    }
}
