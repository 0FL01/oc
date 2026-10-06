use super::*;

#[test]
fn auth04_subscription_catalog_matches_exact_pinned_numeric_overlay_without_touching_key() {
    let ids = [
        "gpt-5.5",
        "gpt-5.3-codex-spark",
        "gpt-5.5-pro",
        "gpt-5.6",
        "gpt-5.4",
        "gpt-5.4-mini",
        "gpt-5.6-luna",
        "gpt-6",
        "gpt-10.any",
        "gpt-6.",
        "gpt-05.05",
        "gpt-5.9999999999999999999999999999",
        "o3",
        "not-gpt-6",
        "gpt-x",
    ];
    let mut models = ids
        .into_iter()
        .map(|id| {
            (
                id.to_owned(),
                json!({"name":id,
        "cost":{"input":1,"output":2},"limit":{"context":10000,"input":8000,"output":2048}}),
            )
        })
        .collect::<BTreeMap<_, _>>();
    models.insert(
        "alias".into(),
        json!({"modelID":"gpt-6", "limit":{"output":8192}}),
    );
    models.insert(
        "pro-mode".into(),
        json!({"modelID":"gpt-6","body":{"reasoning":{"mode":"pro"}}}),
    );
    let key = models.clone();
    transform(&mut models, false);
    assert_eq!(models, key);
    transform(&mut models, true);
    for id in [
        "gpt-5.5",
        "gpt-5.3-codex-spark",
        "gpt-5.6-luna",
        "gpt-6",
        "gpt-10.any",
        "gpt-6.",
        "gpt-05.05",
        "gpt-5.9999999999999999999999999999",
        "alias",
    ] {
        assert_eq!(models[id]["cost"], json!([]));
        assert_eq!(models[id]["limit"]["context"], 400_000);
        assert_eq!(models[id]["limit"]["input"], 272_000);
        assert_eq!(models[id]["limit"]["output"], key[id]["limit"]["output"]);
    }
    assert_eq!(models.len(), 9);
}

#[tokio::test]
async fn auth04_cached_openai_views_accounts_readonly_listing_and_lookup_share_the_overlay() {
    use crate::{
        auth::{AuthScope, OPENAI_BASE_URL},
        storage::{CredentialMaterial, Db},
    };
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        json!({"model":"openai/gpt-5.5",
        "disabled_providers":["opencode-go"]})
        .to_string(),
    )
    .unwrap();
    struct Public;
    impl crate::discovery::DiscoveryClient for Public {
        async fn get(
            &self,
            url: &str,
            headers: &reqwest::header::HeaderMap,
            _: std::time::Duration,
        ) -> Result<(u16, Vec<u8>), crate::discovery::DiscoveryError> {
            assert_eq!(url, crate::models_dev::SOURCE);
            assert_eq!(headers.len(), 1);
            let models = ["gpt-5.5", "gpt-5.4", "gpt-6", "pro-mode"]
                .into_iter()
                .map(|id| {
                    let mut m = json!({"id":id,"name":id,"tool_call":true,
                    "limit":{"context":10000,"output":2048},"cost":{"input":1,"output":2}});
                    if id == "pro-mode" {
                        m["body"] = json!({"reasoning":{"mode":"pro"}});
                    }
                    (id.to_owned(), m)
                })
                .collect::<BTreeMap<_, _>>();
            Ok((
                200,
                serde_json::to_vec(&json!({"openai":{"id":"openai",
                "npm":"@ai-sdk/openai", "models":models}}))
                .unwrap(),
            ))
        }
    }
    let db = Db::open(&data).unwrap();
    db.public_catalog()
        .refresh_provider(
            &db,
            &Public,
            "openai",
            &BTreeMap::new(),
            crate::composition::go_catalog::now_ms(),
            true,
        )
        .await;
    let scope = AuthScope::admit("openai", OPENAI_BASE_URL).unwrap();
    let key = db
        .add_credential(
            scope.namespace(),
            "Key",
            CredentialMaterial::Key {
                key: "KEY_CANARY".into(),
            },
        )
        .unwrap();
    let subscription = db
        .add_credential(
            scope.namespace(),
            "Subscription",
            CredentialMaterial::OAuth {
                access: "ACCESS_CANARY".into(),
                refresh: Some("REFRESH_CANARY".into()),
                expires_at: Some(i64::MAX),
                method_id: Some("chatgpt-browser".into()),
                metadata: None,
            },
        )
        .unwrap();
    assert!(Db::openai_subscription_read_only(&data));
    let mut c = crate::composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    c.resolve_credentials(&db).await.unwrap();
    c.attach_public_catalog(&db).await;
    assert_eq!(c.catalog.models.len(), 2);
    assert!(c.catalog.models.contains_key("gpt-5.5") && c.catalog.models.contains_key("gpt-6"));
    assert_eq!(c.catalog.models["gpt-5.5"]["cost"], json!([]));
    assert!(c.provider.for_selection("gpt-5.5", None).subscription());
    assert!(!c.attach_public_catalog(&db).await);
    assert!(
        c.generation.providers["openai"]
            .options
            .request_bindings
            .values()
            .all(crate::provider::ResponsesConfig::subscription)
    );
    let public = db.public_catalog();
    let mut read = public.read_provider("openai", &BTreeMap::new()).await;
    transform(&mut read.models, true);
    let output = crate::models::lookup::execute_with_catalogs(
        &c.generation,
        &c.catalog,
        &json!({"provider":"openai", "all":true}),
        &[],
        None,
        Some(&read),
    )
    .unwrap();
    let output: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(output["total"], 2);
    assert_eq!(output["providers"][0]["models"][0]["cost"], json!([]));
    db.activate_credential(scope.namespace(), &key.id).unwrap();
    c.refresh_credentials(&db).await.unwrap();
    assert_eq!(c.catalog.models.len(), 4);
    assert_eq!(c.catalog.models["gpt-5.4"]["limit"]["context"], 10000);
    assert_eq!(c.catalog.models["gpt-5.4"]["cost"]["input"], 1.0);
    assert_eq!(c.provider.base_url, OPENAI_BASE_URL);
    assert!(!c.provider.subscription());
    assert!(!Db::openai_subscription_read_only(&data));
    assert!(db.list_sessions().unwrap().is_empty());
    // An unselected public view is metadata-only: adding the native connect
    // target must not exchange an unrelated near-expiry account on startup.
    db.activate_credential(scope.namespace(), &subscription.id)
        .unwrap();
    let row = db.credential_snapshot(scope.namespace()).unwrap().unwrap();
    assert!(db.begin_credential_refresh(&row).unwrap());
    assert!(
        db.rotate_credential(
            &row,
            CredentialMaterial::OAuth {
                access: "NEAR_EXPIRY_CANARY".into(),
                refresh: Some("REFRESH_CANARY".into()),
                expires_at: Some(crate::composition::go_catalog::now_ms() as i64 / 1000 + 60),
                method_id: Some("chatgpt-browser".into()),
                metadata: None,
            }
        )
        .unwrap()
    );
    std::fs::write(
        project.join("opencode.json"),
        json!({"model":"fixture/m",
        "disabled_providers":["opencode-go"], "provider":{"fixture":{"npm":"@ai-sdk/openai",
            "options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"FIXTURE"},"models":{"m":{}}}}})
        .to_string(),
    )
    .unwrap();
    let mut preview = crate::composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    preview.resolve_credentials(&db).await.unwrap();
    preview.attach_public_catalog(&db).await;
    assert_eq!(preview.catalog_for("openai").unwrap().models.len(), 2);
    assert!(
        !db.credential_snapshot(scope.namespace())
            .unwrap()
            .unwrap()
            .refresh_pending
    );
    assert!(
        crate::models::lookup::wants_openai_public(
            &preview.generation,
            &json!({"provider":"openai"})
        )
        .unwrap()
    );
    db.activate_credential(scope.namespace(), &key.id).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        json!({"model":"openai/gpt-5.5",
        "disabled_providers":["opencode-go"]})
        .to_string(),
    )
    .unwrap();
    drop(preview);
    drop(c);
    drop(db);
    let (app, guard, _) = crate::application::spawn_with_env(&project, &data, BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(
        app.provider_catalog("openai".into())
            .await
            .unwrap()
            .models
            .len(),
        4
    );
    assert!(
        app.provider_connections()
            .await
            .unwrap()
            .iter()
            .any(|p| p.provider == "openai")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
