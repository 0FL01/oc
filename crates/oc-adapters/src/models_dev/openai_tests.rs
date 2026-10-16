use super::tests::{PublicFixture, document, model};
use super::*;

#[tokio::test]
async fn auth04_public_slices_share_one_get_and_never_persist_remote_authority() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let mut doc = document();
    doc[OPENAI] = json!({"id":OPENAI,"npm":"@ai-sdk/openai","api":"REMOTE_API_CANARY",
        "env":["REMOTE_KEY_CANARY"],"models":{"gpt-5.5":model("gpt-5.5", "@ai-sdk/openai")}});
    let client = PublicFixture::new(doc);
    let owner = db.public_catalog();
    let local = BTreeMap::new();
    let (go, openai) = tokio::join!(
        owner.refresh(&db, &client, &local, 1000, true),
        owner.refresh_provider(&db, &client, OPENAI, &local, 1000, true)
    );
    assert_eq!(go.models.len(), 4);
    assert_eq!(openai.models.len(), 1);
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    let raw = db.get_pref(CACHE_KEY).unwrap().unwrap();
    for secret in [
        "REMOTE_API_CANARY",
        "REMOTE_KEY_CANARY",
        "REMOTE_SECRET",
        "SECRET_API",
    ] {
        assert!(!raw.contains(secret));
    }
    let reopened = GoCatalog::open(&db)
        .read_provider(OPENAI, &BTreeMap::new())
        .await;
    assert_eq!(reopened.models, openai.models);
    assert_eq!(reopened.fetched_at_ms, Some(1000));
    let mut legacy: Value = serde_json::from_str(&raw).unwrap();
    legacy.as_object_mut().unwrap().remove(OPENAI);
    db.set_pref(CACHE_KEY, &legacy.to_string()).unwrap();
    let old = GoCatalog::open(&db);
    assert_eq!(old.read(&BTreeMap::new()).await.models, go.models);
    assert!(
        old.read_provider(OPENAI, &BTreeMap::new())
            .await
            .models
            .is_empty()
    );
    assert_eq!(
        old.refresh_provider(&db, &client, OPENAI, &BTreeMap::new(), 1001, false)
            .await
            .models,
        openai.models
    );
    assert_eq!(client.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn experimental_modes_expand_like_pinned_models_dev_and_survive_the_cache() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let mut luna = model("gpt-6-luna", "@ai-sdk/openai");
    luna["name"] = json!("GPT-6 Luna");
    luna["experimental"] = json!({"modes": {
        "fast": {"cost": {"input": 0.2, "output": 1}, "provider": {"body": {"service_tier": "priority"}}},
        "ultrafast": {"provider": {"body": {"service_tier": "ultrafast"}}},
        "pro": {"provider": {"body": {"reasoning": {"mode": "pro"}}}},
        "noop": {"cost": {"input": 9}},
        "bad mode": {"provider": {"body": {"service_tier": "priority"}}}
    }});
    let mut doc = document();
    doc[OPENAI] = json!({"id":OPENAI,"npm":"@ai-sdk/openai","models":{"gpt-6-luna":luna}});
    let client = PublicFixture::new(doc);
    let owner = db.public_catalog();
    let read = owner
        .refresh_provider(&db, &client, OPENAI, &BTreeMap::new(), 1000, true)
        .await;
    let ids: Vec<_> = read.models.keys().cloned().collect();
    // `pro` would only touch the adapter-owned `reasoning` field; `noop` and the
    // malformed ID change no request, so neither becomes a silent base copy.
    assert_eq!(
        ids,
        ["gpt-6-luna", "gpt-6-luna-fast", "gpt-6-luna-ultrafast"]
    );
    let fast = &read.models["gpt-6-luna-fast"];
    assert_eq!(fast["name"], "GPT-6 Luna Fast");
    assert_eq!(fast["modelID"], "gpt-6-luna");
    assert_eq!(fast["body"]["service_tier"], "priority");
    assert_eq!(fast["cost"]["input"], 0.2);
    assert_eq!(fast["variants"], read.models["gpt-6-luna"]["variants"]);
    assert_eq!(
        read.models["gpt-6-luna-ultrafast"]["name"],
        "GPT-6 Luna Ultrafast"
    );
    // ChatGPT-plan subscription keeps the eligible fast tiers.
    let mut subscription = read.models.clone();
    crate::composition::openai_catalog::transform(&mut subscription, true);
    assert!(subscription.contains_key("gpt-6-luna-fast"));
    let reopened = GoCatalog::open(&db)
        .read_provider(OPENAI, &BTreeMap::new())
        .await;
    assert_eq!(reopened.models, read.models);
}
