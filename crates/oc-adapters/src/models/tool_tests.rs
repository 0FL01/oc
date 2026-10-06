#[test]
fn tool18_direct_strict_catalog() {
    let defs = crate::runtime::builtin_tool_defs();
    for (name, required) in [
        ("opencode_models", vec![]),
        ("opencode_session_rename", vec!["title"]),
    ] {
        let def = defs
            .iter()
            .find(|d| d.name == name)
            .expect("direct R7 tool");
        assert_eq!(def.parameters["additionalProperties"], false);
        assert_eq!(def.parameters["required"], serde_json::json!(required));
    }
    assert!(defs.iter().any(|d| d.name == "opencode_session_move"));
}

#[test]
fn tool19_direct_strict_move_catalog() {
    let defs = crate::runtime::builtin_tool_defs();
    let def = defs
        .iter()
        .find(|d| d.name == "opencode_session_move")
        .expect("direct R8 move");
    assert_eq!(def.parameters["additionalProperties"], false);
    assert_eq!(def.parameters["required"], serde_json::json!(["directory"]));
}

#[test]
fn tool18_lookup_group_family_page_merge_and_redaction() {
    use super::*;
    use serde_json::json;
    let mut generation = crate::config::Generation {
        tool_output: Default::default(),
        providers: BTreeMap::new(),
        public_go_enabled: false,
        public_openai_enabled: false,
        mcp: BTreeMap::new(),
        permissions: BTreeMap::new(),
        permission_rules: Default::default(),
        provenance: BTreeMap::new(),
        warnings: vec![],
        compaction: Default::default(),
        config_diagnostics: vec![],
        animations: None,
    };
    let entry = |name: &str, models: serde_json::Value| crate::config::ProviderEntry {
        name: Some(name.into()),
        env: vec![],
        npm: None,
        options: Default::default(),
        models: serde_json::from_value(models).unwrap(),
    };
    generation.providers.insert(
        "alpha".into(),
        entry(
            "Alpha Display",
            json!({"other":{"name":"other","family":"f","released":90}}),
        ),
    );
    generation
        .providers
        .insert("own".into(), entry("Own Display", json!({"retired":{}})));
    let selected = ModelCatalog {provider:"own".into(),models:serde_json::from_value(json!({
        "older":{"name":"Cool old","family":"f","released":10},
        "new":{"name":"Cool new","family":"f","released":20,"cost":{"input":2,"output":3,"secret":"CANARY"},"variants":{"z-low":{"reasoningEffort":"low"},"a-high":{"reasoningEffort":"high"},"custom":{}}},
        "unknown":{"name":"CANARY"}, "unknown2":{}
    })).unwrap()};
    let run = |args| -> serde_json::Value {
        serde_json::from_str(
            &lookup::execute(&generation, &selected, &args, &["CANARY".into()]).unwrap(),
        )
        .unwrap()
    };
    let first = run(json!({"limit":1}));
    assert_eq!(first["total"], 4);
    assert_eq!(first["nextOffset"], 1);
    let model = &first["providers"][0]["models"][0];
    assert_eq!(model["id"], "own/new");
    assert_eq!(model["variants"], json!(["z-low", "a-high", "custom"]));
    assert_eq!(model["cost"], json!({"input":2,"output":3}));
    let all = run(json!({"all":true}));
    assert_eq!(all["total"], 5);
    assert_eq!(all["providers"][0]["models"][1]["id"], "own/older");
    let unknown = &first;
    assert!(!unknown.to_string().contains("CANARY"));
    let last = run(json!({"offset":3}));
    assert_eq!(last["providers"][0]["id"], "alpha");
    assert!(last["nextOffset"].is_null());
    let filtered = run(json!({"query":"OWN/old COOL","provider":"own DISPLAY"}));
    assert_eq!(filtered["total"], 1); // filter precedes newest-family reduction
    assert_eq!(filtered["providers"][0]["models"][0]["id"], "own/older");
    let unknowns = run(json!({"query":"unknown"}));
    for m in unknowns["providers"][0]["models"].as_array().unwrap() {
        for key in ["released", "family", "cost", "status"] {
            assert!(m[key].is_null());
        }
    }
    assert!(!run(json!({})).to_string().contains("CANARY"));
    assert_eq!(run(json!({"offset":100}))["providers"], json!([]));
    assert!(generation.providers["own"].models.contains_key("retired"));
    assert!(!selected.models.contains_key("retired"));
}

#[test]
fn tool18_invalid_input_and_bounded_page() {
    use serde_json::json;
    for args in [
        json!(null),
        json!([]),
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"limit":1.5}),
        json!({"offset":-1}),
        json!({"all":"true"}),
        json!({"query":null}),
        json!({"provider":1}),
        json!({"future":true}),
    ] {
        assert!(super::lookup::parse(&args).is_err(), "{args}");
    }
    for args in [
        json!({"title":" "}),
        json!({"title":"line\nbreak"}),
        json!({"title":"ok","sessionID":null}),
        json!({"title":"ok","sessionID":""}),
        json!({"title":"ok","extra":true}),
    ] {
        assert!(crate::tools::rename_input(&args).is_err(), "{args}");
    }
    assert_eq!(
        crate::tools::rename_input(&json!({"title":"  Café 🦀  "})).unwrap(),
        ("Café 🦀", None)
    );
}

#[test]
fn go02_native_lookup_merges_public_go_before_static_or_selected_rows() {
    use serde_json::{Value, json};
    let mut generation = crate::config::Generation {
        public_go_enabled: true,
        ..Default::default()
    };
    generation.providers.insert(
        crate::models_dev::PROVIDER.into(),
        crate::config::ProviderEntry {
            models: serde_json::from_value(json!({"local-only":{"name":"must not resurrect"}}))
                .unwrap(),
            name: None,
            npm: None,
            env: Vec::new(),
            options: Default::default(),
        },
    );
    let selected = super::ModelCatalog {
        provider: "own".into(),
        models: serde_json::from_value(json!({"selected":{"name":"Selected"}})).unwrap(),
    };
    let mut public = crate::models_dev::CatalogRead {
        models: serde_json::from_value(json!({"remote/only":{"name":"Public CANARY",
            "variants":[{"id":"high","settings":{"reasoningEffort":"high","apiKey":"CANARY"}}],
            "headers":{"Authorization":"CANARY"},"api":"http://never"}}))
        .unwrap(),
        fetched_at_ms: Some(1),
        changed: false,
        failure: None,
    };
    assert!(super::lookup::wants_public(&generation, &json!({})).unwrap());
    assert!(!super::lookup::wants_public(&generation, &json!({"provider":"own"})).unwrap());
    let output = super::lookup::execute_with_public(
        &generation,
        &selected,
        &json!({}),
        &["CANARY".into()],
        Some(&public),
    )
    .unwrap();
    assert!(
        !output.contains("CANARY")
            && !output.contains("Authorization")
            && !output.contains("apiKey")
    );
    assert!(!output.contains("local-only"));
    let result: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(result["providers"][0]["id"], "own");
    assert_eq!(
        result["providers"][1]["models"][0]["id"],
        "opencode-go/remote/only"
    );
    assert_eq!(
        result["providers"][1]["models"][0]["variants"],
        json!(["high"])
    );
    assert_eq!(result["publicCatalog"]["status"], "ready");
    public.models.clear();
    public.failure = Some(crate::discovery::DiscoveryFailure::Network);
    let output: Value = serde_json::from_str(
        &super::lookup::execute_with_public(
            &generation,
            &selected,
            &json!({"provider":"OpenCode Go"}),
            &[],
            Some(&public),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(output["total"], 0);
    assert_eq!(output["publicCatalog"]["status"], "failed");
    generation.public_go_enabled = false;
    assert!(!super::lookup::wants_public(&generation, &json!({})).unwrap());
}
