use super::*;
use serde_json::json;

fn fixture(config: serde_json::Value) -> (tempfile::TempDir, BTreeMap<String, String>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("opencode.json"), config.to_string()).unwrap();
    (root, BTreeMap::new())
}

#[tokio::test]
async fn catalog_static_is_selection_independent_and_does_not_resolve_connections() {
    let models: BTreeMap<_, _> = (0..25)
        .map(|n| (format!("family-{n:02}"), json!({"family":"same"})))
        .chain([(String::from("route/model"), json!({}))])
        .collect();
    for selection in [
        "missing/unavailable",
        "{file:must-not-read}",
        "{env:OC_MISSING_CATALOG_MODEL}",
    ] {
        let (root, env) = fixture(json!({
            "model":selection, "default_agent":"missing",
            "provider": {
                "z": {"npm":"@ai-sdk/openai", "options":{"apiKey":"{file:do-not-read}","baseURL":"{file:do-not-read}"},"models":models},
                "a": {"models":{"route/model":{}}},
                "foreign": {"npm":"unsupported", "options":{"apiKey":"{file:do-not-read}"},"models":{"hidden":{}}}
            },
            "mcp":{"browser":{"type":"local","enabled":true,"command":["{file:do-not-read}"]}}
        }));
        let listing = load_catalog_with_env(root.path(), env).await.unwrap();
        assert!(listing.complete);
        assert_eq!(listing.references.len(), 27);
        assert_eq!(listing.references.first().unwrap(), "a/route/model");
        assert_eq!(listing.references.last().unwrap(), "z/route/model");
        assert!(listing.references.windows(2).all(|p| p[0] < p[1]));
        let env = BTreeMap::new();
        let admitted = admit_sources(root.path(), &env).unwrap();
        let settings =
            admit_settings(&admitted.sources, &admitted.admitted_roots, &env, false).unwrap();
        assert!(settings.selected.is_none());
        assert!(settings.default_agent.is_none());
        let ordinary = admit_settings(&admitted.sources, &admitted.admitted_roots, &env, true);
        if selection.starts_with("{file:") {
            assert!(
                matches!(ordinary, Err(LoadFailure::Configuration(diagnostic)) if diagnostic.code == ServiceCode::TrustRefused)
            );
        } else {
            let ordinary = ordinary.unwrap();
            assert_eq!(
                ordinary.selected.as_deref(),
                Some(if selection.starts_with("{env:") {
                    ""
                } else {
                    selection
                })
            );
            assert_eq!(ordinary.default_agent.as_deref(), Some("missing"));
        }
    }
}

#[tokio::test]
async fn catalog_empty_disabled_and_missing_dynamic_key_have_distinct_outcomes() {
    for config in [
        json!({}),
        json!({"disabled_providers":["ludka2"],"provider":{"ludka2":{"options":{"apiKey":"{file:never}"},"models":{"hidden":{}}}}}),
    ] {
        let (root, env) = fixture(config);
        let listing = load_catalog_with_env(root.path(), env).await.unwrap();
        assert!(listing.complete);
        assert!(listing.references.is_empty());
    }
    let (root, env) = fixture(json!({"provider":{"ludka2":{"models":{"known":{}}}}}));
    let listing = load_catalog_with_env(root.path(), env).await.unwrap();
    assert!(!listing.complete);
    assert_eq!(listing.references, ["ludka2/known"]);
    assert_eq!(listing.diagnostics[0].code, ServiceCode::MissingCredential);
}

#[tokio::test]
async fn catalog_fatal_policy_metadata_caps_and_controls_publish_no_candidate() {
    for config in [
        json!({"permission":{"read":17}}),
        json!({"model":17}),
        json!({"plugin":17}),
        json!({"provider":{"p":{"models":{"ok":{},"bad\nline":{}}}}}),
        json!({"provider":{"p":{"models":{"ok":{},"oversized":{"name":"x".repeat(12289)}}}}}),
    ] {
        let (root, env) = fixture(config);
        assert!(load_catalog_with_env(root.path(), env).await.is_err());
    }
}

#[tokio::test]
async fn catalog_root_escape_and_dynamic_secret_escape_are_fatal() {
    use std::os::unix::fs::symlink;
    let (root, env) =
        fixture(json!({"provider":{"ludka2":{"options":{"apiKey":"{file:../outside}"}}}}));
    let error = load_catalog_with_env(root.path(), env.clone())
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("trust_refused"));
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join(".opencode")).unwrap();
    assert!(load_catalog_with_env(root.path(), env).await.is_err());
}
