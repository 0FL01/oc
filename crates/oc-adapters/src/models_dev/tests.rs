use super::*;
use crate::discovery::{DiscoveryClient, DiscoveryError, DiscoveryFailure};
use std::sync::{Arc, Mutex, atomic::AtomicUsize};

struct PublicFixture {
    body: Mutex<Vec<u8>>,
    calls: AtomicUsize,
}
impl PublicFixture {
    fn new(body: Value) -> Self {
        Self {
            body: Mutex::new(serde_json::to_vec(&body).unwrap()),
            calls: AtomicUsize::new(0),
        }
    }
    fn replace(&self, body: Value) {
        *self.body.lock().unwrap() = serde_json::to_vec(&body).unwrap();
    }
}
impl DiscoveryClient for PublicFixture {
    async fn get(
        &self,
        url: &str,
        headers: &reqwest::header::HeaderMap,
        timeout: Duration,
    ) -> Result<(u16, Vec<u8>), DiscoveryError> {
        assert_eq!(url, SOURCE);
        assert_eq!(headers.len(), 1);
        assert_eq!(headers[reqwest::header::ACCEPT], "application/json");
        assert_eq!(timeout, DEADLINE);
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(30)).await;
        Ok((200, self.body.lock().unwrap().clone()))
    }
}
fn model(id: &str, package: &str) -> Value {
    json!({"id":id,"name":id,"provider":{"npm":package,"api":"http://169.254.169.254/SECRET_API"},
        "limit":{"context":200000,"output":8192},"tool_call":true,
        "modalities":{"input":["text","image","pdf"],"output":["text"]},
        "cost":{"input":1.0,"output":2.0,"cache_read":0.1},
        "reasoning_options":[{"type":"effort","values":[null,"null","none","custom-future-effort"]}],
        "headers":{"Authorization":"REMOTE_SECRET"},"env":["REMOTE_SECRET"],
        "settings":{"apiKey":"REMOTE_SECRET"}})
}
fn document() -> Value {
    let mut messages = model("messages", "@ai-sdk/anthropic");
    messages["reasoning_options"] =
        json!([{"type":"toggle"},{"type":"budget_tokens","min":1024,"max":4096}]);
    let mut deprecated = model("old", "@ai-sdk/openai");
    deprecated["status"] = json!("deprecated");
    let mut chat = model("chat", "@ai-sdk/openai-compatible");
    chat["interleaved"] = json!({"field":"reasoning_content"});
    json!({"opencode-go":{"id":"opencode-go","npm":"@ai-sdk/openai","env":["REMOTE_SECRET"],
        "api":"http://169.254.169.254/SECRET_API","models":{
        "chat":chat,"responses":model("responses","@ai-sdk/openai"),"messages":messages,
        "unknown":model("unknown","@unsupported/SDK"),"old":deprecated}},
        "foreign":{"models":{"huge":{"apiKey":"REMOTE_SECRET"}}}})
}

#[tokio::test]
async fn go02_public_cache_retirement_current_overrides_and_safe_persistence() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let client = PublicFixture::new(document());
    let owner = GoCatalog::open(&db);
    assert!(owner.read(&BTreeMap::new()).await.models.is_empty());
    let locals = serde_json::from_value(json!({"chat":{"name":"Local","limit":{"context":600000},
        "variants":[{"id":"none","settings":{"reasoningEffort":"LOCAL_NONE"}}]},
        "old":{"name":"cannot revive deprecated"},"local-only":{"name":"cannot revive removed"}}))
    .unwrap();
    let first = owner.refresh(&db, &client, &locals, 1000, false).await;
    assert!(first.changed);
    assert!(first.failure.is_none());
    assert_eq!(first.models.len(), 4);
    assert_eq!(first.models["chat"]["name"], "Local");
    assert_eq!(
        first.models["chat"]["limit"],
        json!({"context":600000,"output":8192})
    );
    assert_eq!(
        first.models["chat"]["compatibility"]["reasoningField"],
        "reasoning_content"
    );
    assert_eq!(
        first.models["chat"]["variants"][0]["settings"]["reasoningEffort"],
        "LOCAL_NONE"
    );
    assert_eq!(
        first.models["chat"]["variants"][1]["id"],
        "custom-future-effort"
    );
    assert!(
        first.models["unknown"]["variants"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(first.models["unknown"]["package"], "@unsupported/SDK");
    assert!(
        first.models["messages"]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["id"] == "max" && v["settings"]["thinking"]["budget_tokens"] == 4096)
    );
    let raw = db.get_pref(CACHE_KEY).unwrap().unwrap();
    for secret in [
        "REMOTE_SECRET",
        "SECRET_API",
        "apiKey",
        "headers",
        "foreign",
    ] {
        assert!(!raw.contains(secret), "{secret}");
    }
    assert!(
        !raw.contains("Local"),
        "persist remote base, not merged local snapshot"
    );
    let reopen = GoCatalog::open(&db);
    let changed_locals = serde_json::from_value(json!({"chat":{"name":"Current local"}})).unwrap();
    let cached = reopen.read(&changed_locals).await;
    assert_eq!(cached.models["chat"]["name"], "Current local");
    assert_eq!(cached.models["chat"]["limit"]["context"], 200000);
    assert_eq!(cached.fetched_at_ms, Some(1000));
    assert!(!format!("{cached:?}").contains("Current local"));
    assert!(
        !reopen
            .refresh(&db, &client, &changed_locals, 1001, false)
            .await
            .changed
    );
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    assert!(
        !reopen
            .refresh(&db, &client, &changed_locals, 1002, true)
            .await
            .changed
    );
    client.replace(json!({}));
    let failed = reopen
        .refresh(&db, &client, &changed_locals, 1003, true)
        .await;
    assert_eq!(failed.failure, Some(DiscoveryFailure::InvalidResponse));
    assert_eq!(failed.models, cached.models);
    assert_eq!(failed.fetched_at_ms, Some(1002));
    client.replace(json!({"opencode-go":{"id":"opencode-go","npm":"@ai-sdk/openai","models":{}}}));
    let retired = reopen.refresh(&db, &client, &locals, 1004, true).await;
    assert!(retired.changed && retired.failure.is_none() && retired.models.is_empty());
    assert!(GoCatalog::open(&db).read(&locals).await.models.is_empty());
}

#[tokio::test]
async fn go02_public_refresh_is_single_flight_cancellable_and_source_qualified() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(tmp.path()).unwrap());
    let client = Arc::new(PublicFixture::new(document()));
    let owner = Arc::new(GoCatalog::open(&db));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let (db, client, owner) = (db.clone(), client.clone(), owner.clone());
        tasks.push(tokio::spawn(async move {
            owner
                .refresh(&db, client.as_ref(), &BTreeMap::new(), 1000, true)
                .await
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().models.len(), 4);
    }
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
    let (task_db, task_client, task_owner) = (db.clone(), client.clone(), owner.clone());
    let task = tokio::spawn(async move {
        task_owner
            .refresh(&task_db, task_client.as_ref(), &BTreeMap::new(), 1001, true)
            .await
    });
    while client.calls.load(Ordering::SeqCst) == 1 {
        tokio::task::yield_now().await;
    }
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(owner.read(&BTreeMap::new()).await.fetched_at_ms, Some(1000));
    assert!(
        !owner
            .refresh(&db, client.as_ref(), &BTreeMap::new(), 1002, true)
            .await
            .changed
    );
    assert_eq!(client.calls.load(Ordering::SeqCst), 3);
    let mut raw: Value = serde_json::from_str(&db.get_pref(CACHE_KEY).unwrap().unwrap()).unwrap();
    raw["source"] = json!("https://models.opencode.ai/api.json");
    db.set_pref(CACHE_KEY, &raw.to_string()).unwrap();
    assert!(
        GoCatalog::open(&db)
            .read(&BTreeMap::new())
            .await
            .models
            .is_empty()
    );
    db.set_pref(CACHE_KEY, &"x".repeat(discovery::DISCOVERY_BODY_CAP + 1))
        .unwrap();
    assert!(
        GoCatalog::open(&db)
            .read(&BTreeMap::new())
            .await
            .models
            .is_empty()
    );
}

#[tokio::test]
async fn go02_last_good_reads_do_not_wait_for_the_inflight_public_get() {
    struct Held {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }
    impl DiscoveryClient for Held {
        async fn get(
            &self,
            _: &str,
            _: &reqwest::header::HeaderMap,
            _: Duration,
        ) -> Result<(u16, Vec<u8>), DiscoveryError> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok((200, serde_json::to_vec(&document()).unwrap()))
        }
    }
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let owner = db.public_catalog();
    owner
        .refresh(
            &db,
            &PublicFixture::new(document()),
            &BTreeMap::new(),
            1,
            true,
        )
        .await;
    let held = Arc::new(Held {
        entered: Default::default(),
        release: Default::default(),
    });
    let job = tokio::spawn({
        let owner = owner.clone();
        let db = db.shared_handle();
        let held = held.clone();
        async move {
            owner
                .refresh(&db, held.as_ref(), &BTreeMap::new(), 2, true)
                .await
        }
    });
    held.entered.notified().await;
    let read = tokio::time::timeout(Duration::from_millis(100), owner.read(&BTreeMap::new()))
        .await
        .unwrap();
    assert_eq!(read.fetched_at_ms, Some(1));
    assert_eq!(read.models.len(), 4);
    held.release.notify_one();
    assert_eq!(job.await.unwrap().fetched_at_ms, Some(2));
}

#[tokio::test]
async fn go02_malformed_public_rows_fail_atomically_without_inferred_controls() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Db::open(tmp.path()).unwrap();
    let client = PublicFixture::new(document());
    let owner = GoCatalog::open(&db);
    owner
        .refresh(&db, &client, &BTreeMap::new(), 1, false)
        .await;
    for malformed in [
        json!({"id":"different"}),
        json!({"limit":{"context":0,"output":10}}),
        json!({"reasoning_options":[{"type":"effort"}]}),
        json!({"interleaved":{"field":""}}),
        json!({"reasoning_options":[{"type":"budget_tokens","min":4096,"max":10}]}),
        json!({"tool_call":"yes"}),
    ] {
        let mut document = document();
        document[PROVIDER]["models"]["chat"]
            .as_object_mut()
            .unwrap()
            .extend(malformed.as_object().unwrap().clone());
        client.replace(document);
        let result = owner.refresh(&db, &client, &BTreeMap::new(), 2, true).await;
        assert_eq!(result.failure, Some(DiscoveryFailure::InvalidResponse));
        assert_eq!(result.models.len(), 4);
        assert_eq!(result.fetched_at_ms, Some(1));
    }
    let mut document = document();
    let chat = document[PROVIDER]["models"]["chat"]
        .as_object_mut()
        .unwrap();
    chat.remove("reasoning_options");
    chat.insert("reasoning".into(), json!(true));
    chat.insert("interleaved".into(), json!(true));
    client.replace(document);
    let result = owner
        .refresh(&db, &client, &BTreeMap::new(), TTL_MS + 1, false)
        .await;
    assert!(result.failure.is_none());
    assert!(
        result.models["chat"]["variants"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(result.models["chat"].get("compatibility").is_none());
}

#[tokio::test]
async fn go02_failed_single_flight_reports_failure_to_all_waiters_and_preserves_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(tmp.path()).unwrap());
    let owner = Arc::new(GoCatalog::open(&db));
    let client = Arc::new(PublicFixture::new(document()));
    owner
        .refresh(&db, client.as_ref(), &BTreeMap::new(), 1, false)
        .await;
    client.replace(json!({"opencode-go":null}));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let (db, client, owner) = (db.clone(), client.clone(), owner.clone());
        tasks.push(tokio::spawn(async move {
            owner
                .refresh(&db, client.as_ref(), &BTreeMap::new(), 2, true)
                .await
        }));
    }
    for task in tasks {
        let result = task.await.unwrap();
        assert_eq!(result.failure, Some(DiscoveryFailure::InvalidResponse));
        assert_eq!(result.fetched_at_ms, Some(1));
        assert_eq!(result.models.len(), 4);
    }
    assert_eq!(client.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        GoCatalog::open(&db)
            .read(&BTreeMap::new())
            .await
            .fetched_at_ms,
        Some(1)
    );
    *client.body.lock().unwrap() = vec![b' '; discovery::DISCOVERY_BODY_CAP + 1];
    assert_eq!(
        owner
            .refresh(&db, client.as_ref(), &BTreeMap::new(), 3, true)
            .await
            .failure,
        Some(DiscoveryFailure::InvalidResponse)
    );
    client.replace(document());
    let mut both = document();
    both[PROVIDER]["models"]["messages"]["reasoning_options"] = json!([
        {"type":"budget_tokens","max":4096},{"type":"effort","values":["low","high"]},{"type":"toggle"}]);
    client.replace(both);
    let result = owner
        .refresh(&db, client.as_ref(), &BTreeMap::new(), 4, true)
        .await;
    assert_eq!(
        result.models["messages"]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["none", "low", "high"]
    );
    assert!(
        result.models["messages"]["variants"]
            .as_array()
            .unwrap()
            .iter()
            .skip(1)
            .all(|v| v["settings"]["thinking"]["type"] == "adaptive")
    );
}
