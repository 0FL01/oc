use super::*;

#[tokio::test]
async fn ui07_completed_catalog_join_failure_is_not_suppressed_on_stop() {
    let task: JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>> =
        tokio::spawn(async { panic!("owned catalog fixture join failure") });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut work = ProviderWork {
        task: Some(task),
        public_task: None,
    };
    let result = work.stop().await;
    let diagnostic = result.unwrap_err();
    assert_eq!(
        diagnostic.code,
        oc_core::queries::ServiceCode::RuntimeFailed
    );
    assert_eq!(diagnostic.stage, oc_core::queries::ServiceStage::Cleanup);
    assert_eq!(
        diagnostic.action,
        oc_core::queries::ServiceAction::RestartApplication
    );
}

#[tokio::test]
async fn go05_independent_public_job_publishes_only_its_view_and_stops_without_late_cache() {
    struct PublicPeer;
    impl crate::discovery::DiscoveryClient for PublicPeer {
        async fn get(
            &self,
            url: &str,
            headers: &reqwest::header::HeaderMap,
            _: std::time::Duration,
        ) -> Result<(u16, Vec<u8>), crate::discovery::DiscoveryError> {
            assert_eq!(url, crate::models_dev::SOURCE);
            assert_eq!(headers.len(), 1);
            assert!(!headers.contains_key("authorization") && !headers.contains_key("x-api-key"));
            Ok((
                200,
                serde_json::to_vec(&serde_json::json!({"opencode-go":{
                    "id":"opencode-go", "npm":"@ai-sdk/openai-compatible", "models":{
                        "public/slash":{"id":"public/slash","name":"Public row","tool_call":true,
                            "limit":{"context":32000,"output":2048}}
                    }
                }}))
                .unwrap(),
            ))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"custom/local", "providers":{"custom":{
            "package":"@ai-sdk/openai", "settings":{"baseURL":"https://example.com/custom", "apiKey":"CUSTOM_CANARY"},
            "models":{"local":{"name":"Local", "limit":{"context":10000,"output":1024}}}
        }}
    }).to_string()).unwrap();
    let db = Db::open(&root.path().join("data")).unwrap();
    let mut c = composition::load_local_with_env(&project, BTreeMap::new())
        .await
        .unwrap();
    c.resolve_credentials(&db).unwrap();
    c.attach_public_catalog(&db).await;
    let original = (
        c.catalog.provider.clone(),
        c.catalog.models.clone(),
        c.provider.clone(),
        c.model_id.clone(),
    );
    let owned = db.shared_handle();
    let public_task = tokio::spawn(async move {
        Ok(composition::go_catalog::outcome(
            owned
                .public_catalog()
                .refresh(&owned, &PublicPeer, &BTreeMap::new(), 1, true)
                .await,
        ))
    });
    let selected = tokio::spawn(std::future::pending());
    let mut work = ProviderWork {
        task: Some(selected),
        public_task: Some(public_task),
    };
    let CatalogOutcome::Public(outcome) =
        tokio::time::timeout(std::time::Duration::from_secs(1), work.wait())
            .await
            .unwrap()
            .unwrap()
    else {
        panic!("public job must not wait for configured discovery")
    };
    assert!(c.accept_public_view(outcome));
    assert_eq!(
        c.provider_views[crate::models_dev::PROVIDER]
            .catalog
            .models
            .len(),
        1
    );
    assert_eq!(
        (
            c.catalog.provider.clone(),
            c.catalog.models.clone(),
            c.provider.clone(),
            c.model_id.clone()
        ),
        original
    );
    assert!(work.pending());
    work.stop().await.unwrap();
    assert!(!work.pending());
    // A fresh cache query does not publish another change (and cannot create an event/query loop).
    let unchanged = db.public_catalog().read(&BTreeMap::new()).await;
    assert!(!c.accept_public_view(composition::go_catalog::outcome(unchanged)));
    let cached = db.get_pref(crate::models_dev::CACHE_KEY).unwrap();
    let notify = Arc::new(tokio::sync::Notify::new());
    let entered = notify.clone();
    let public_task = tokio::spawn(async move {
        entered.notify_one();
        std::future::pending().await
    });
    let mut retired = ProviderWork {
        task: None,
        public_task: Some(public_task),
    };
    assert!(retired.pending() && !retired.selected_pending());
    notify.notified().await;
    retired.stop().await.unwrap();
    assert_eq!(db.get_pref(crate::models_dev::CACHE_KEY).unwrap(), cached);
    assert!(db.list_sessions().unwrap().is_empty());
}
