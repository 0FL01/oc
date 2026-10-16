use super::*;
use std::sync::{Mutex, atomic::AtomicU64};

#[derive(Default)]
struct ClockFake(AtomicU64);
impl Clock for ClockFake {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
    async fn sleep_ms(&self, ms: u64) {
        self.0.fetch_add(ms, Ordering::Relaxed);
    }
}
struct Client {
    replies: Mutex<std::collections::VecDeque<(u16, Vec<u8>)>>,
    requests: Mutex<Vec<(String, HeaderMap)>>,
}
impl Client {
    fn new(replies: Vec<(u16, Value)>) -> Self {
        Self {
            replies: Mutex::new(
                replies
                    .into_iter()
                    .map(|(status, value)| (status, serde_json::to_vec(&value).unwrap()))
                    .collect(),
            ),
            requests: Mutex::new(Vec::new()),
        }
    }
}
impl DiscoveryClient for Client {
    async fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        _: Duration,
    ) -> Result<(u16, Vec<u8>), DiscoveryError> {
        self.requests
            .lock()
            .unwrap()
            .push((url.into(), headers.clone()));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(DiscoveryError::Network)
    }
}

struct TimedClient<'a> {
    clock: &'a ClockFake,
    inner: Client,
    elapsed: Mutex<std::collections::VecDeque<u64>>,
    timeouts: Mutex<Vec<Duration>>,
}
impl DiscoveryClient for TimedClient<'_> {
    async fn get(
        &self,
        url: &str,
        headers: &HeaderMap,
        timeout: Duration,
    ) -> Result<(u16, Vec<u8>), DiscoveryError> {
        self.timeouts.lock().unwrap().push(timeout);
        let elapsed = self.elapsed.lock().unwrap().pop_front().unwrap();
        assert!(u128::from(elapsed) <= timeout.as_millis());
        self.clock.0.fetch_add(elapsed, Ordering::Relaxed);
        self.inner.get(url, headers, timeout).await
    }
}

async fn run(client: &Client, key: Option<&str>) -> Result<CustomCatalog, DiscoveryError> {
    discover(
        &ClockFake::default(),
        client,
        "https://custom.invalid/exact/prefix",
        key,
        None,
        &AtomicBool::new(false),
    )
    .await
}

#[tokio::test]
async fn conn02_openai_ambiguous_id_only_rich_null_false_empty_and_safe_snapshot() {
    let client = Client::new(vec![(
        200,
        json!({"object":"list","data":[
            {"id":"bare/id"},
            {"id":"rich","opencode":{"name":"Supplied name","reasoning":false,"tool_call":true,"modalities":{"input":[],"output":["text"]},"variants":{},"limit":{"context":null,"output":2048},"options":{"apiKey":"evil"},"npm":"evil","headers":{"x":"evil"}}},
            {"id":"nullable","context_length":null,"opencode":{"reasoning":null,"modalities":null,"variants":null}}
        ]}),
    )]);
    let result = run(&client, None).await.unwrap();
    assert_eq!(result.suggested, None);
    assert_eq!(result.models.len(), 3);
    assert!(result.models["bare/id"].get("modalities").is_none());
    assert_eq!(result.models["rich"]["modalities"]["input"], json!([]));
    assert_eq!(result.models["rich"]["reasoning"], false);
    assert!(result.models["nullable"].get("variants").is_none());
    assert!(
        crate::models::available_variants(&result.models["rich"])
            .next()
            .is_none()
    );
    assert!(
        !serde_json::to_string(&result.models)
            .unwrap()
            .contains("evil")
    );
    assert_eq!(sanitize_snapshot(&result.models).unwrap(), result.models);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests[0].0, "https://custom.invalid/exact/prefix/models");
    assert!(!requests[0].1.contains_key(AUTHORIZATION));
    assert!(!requests[0].1.contains_key("x-api-key"));
}

#[tokio::test]
async fn conn02_anthropic_pages_auth_candidates_partial_capabilities_exact_efforts() {
    let client = Client::new(vec![
        (401, json!({"error":"ignored"})),
        (
            200,
            json!({"data":[{"id":"model-a","display_name":"Actual A","type":"model","max_input_tokens":12345,"max_tokens":2048,"capabilities":{"image_input":{"supported":false},"pdf_input":{"supported":true},"thinking":{"supported":true},"effort":{"supported":true,"low":{"supported":true},"medium":{"supported":false},"high":null,"max":{"supported":true}}}}],"has_more":true,"last_id":"model-a"}),
        ),
        (
            200,
            json!({"data":[{"id":"model-b","capabilities":null,"max_tokens":null}],"has_more":false,"last_id":"model-b"}),
        ),
    ]);
    let result = run(&client, Some("KEY_CUSTOM_CANARY")).await.unwrap();
    assert_eq!(result.suggested, Some(NativeProtocol::Messages));
    assert_eq!(
        result.models["model-a"]["partial_capabilities"]["image_input"],
        false
    );
    assert_eq!(
        result.models["model-a"]["partial_capabilities"]["pdf_input"],
        true
    );
    assert!(result.models["model-a"].get("modalities").is_none());
    assert_eq!(
        crate::models::available_variants(&result.models["model-a"])
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        vec!["low", "max"]
    );
    assert_eq!(sanitize_snapshot(&result.models).unwrap(), result.models);
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].1.contains_key(AUTHORIZATION));
    assert!(!requests[1].1.contains_key(AUTHORIZATION));
    assert_eq!(requests[1].1["x-api-key"], "KEY_CUSTOM_CANARY");
    assert_eq!(requests[1].1["anthropic-version"], "2023-06-01");
    assert!(requests[2].0.ends_with("?limit=1000&after_id=model-a"));
}

#[tokio::test]
async fn conn02_empty_complete_partial_duplicate_progress_and_no_page_count_assumption() {
    let empty = run(
        &Client::new(vec![(200, json!({"object":"list","data":[]}))]),
        None,
    )
    .await
    .unwrap();
    assert!(empty.models.is_empty());
    for response in [
        json!({"object":"list","data":[{"id":"a"}],"has_more":true,"last_id":"a"}),
        json!({"object":"list","data":[],"has_more":"false"}),
        json!({"data":[{"id":"a"}],"has_more":false,"last_id":"wrong"}),
        json!({"data":[],"has_more":false,"last_id":"a"}),
        json!({"data":[{"id":"a"}],"has_more":true,"last_id":"wrong"}),
        json!({"data":[],"has_more":true,"last_id":"a"}),
        json!({"object":"list","data":[{"id":"same"},{"id":"same"}]}),
        json!({"object":"list","data":[{"id":"valid"},{"id":"bad","opencode":{"reasoning":"yes"}}]}),
    ] {
        assert!(matches!(
            run(&Client::new(vec![(200, response)]), None).await,
            Err(DiscoveryError::InvalidResponse)
        ));
    }
    let pages = (0..12).map(|n| (200, json!({"data":[{"id":format!("page-{n}")}],"has_more":n<11,"last_id":format!("page-{n}")}))).collect();
    let client = Client::new(pages);
    assert_eq!(run(&client, None).await.unwrap().models.len(), 12);
    assert_eq!(client.requests.lock().unwrap().len(), 12);
    let partial = Client::new(vec![
        (
            200,
            json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a"}),
        ),
        (400, json!({})),
    ]);
    assert!(run(&partial, None).await.is_err());
}

#[tokio::test]
async fn conn02_retry_clock_cancel_total_bytes_rows_and_secret_refusal() {
    let clock = ClockFake::default();
    let client = Client::new(vec![
        (503, json!({})),
        (429, json!({})),
        (200, json!({"object":"list","data":[]})),
    ]);
    discover(
        &clock,
        &client,
        "https://custom.invalid",
        None,
        None,
        &AtomicBool::new(false),
    )
    .await
    .unwrap();
    assert_eq!(clock.now_ms(), 1000);
    assert!(matches!(
        discover(
            &clock,
            &client,
            "https://custom.invalid",
            None,
            None,
            &AtomicBool::new(true)
        )
        .await,
        Err(DiscoveryError::Cancelled)
    ));
    let rows: Vec<_> = (0..=discovery::DISCOVERY_ROWS_CAP)
        .map(|i| json!({"id":format!("id-{i}")}))
        .collect();
    assert!(
        run(
            &Client::new(vec![(200, json!({"object":"list","data":rows}))]),
            None
        )
        .await
        .is_err()
    );
    let large = "a".repeat(discovery::DISCOVERY_BODY_CAP / 2 + 1024);
    let client = Client::new(vec![
        (
            200,
            json!({"data":[{"id":"a","ignored":large}],"has_more":true,"last_id":"a"}),
        ),
        (
            200,
            json!({"data":[{"id":"b","ignored":large}],"has_more":false}),
        ),
    ]);
    assert!(matches!(
        run(&client, None).await,
        Err(DiscoveryError::InvalidResponse)
    ));
    assert!(
        run(
            &Client::new(vec![(
                200,
                json!({"object":"list","data":[{"id":"KEY_CUSTOM_CANARY"}]})
            )]),
            Some("KEY_CUSTOM_CANARY")
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn conn02_one_total_deadline_covers_pages_auth_candidates_and_retry_delays() {
    for first in [
        (
            200,
            json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a"}),
        ),
        (401, json!({})),
    ] {
        let clock = ClockFake::default();
        let client = TimedClient {
            clock: &clock,
            inner: Client::new(vec![
                first,
                (503, json!({})),
                (
                    200,
                    json!({"data":[{"id":"b"}],"has_more":false,"last_id":"b"}),
                ),
            ]),
            elapsed: Mutex::new([14_000, 14_000, 1_750].into()),
            timeouts: Mutex::new(Vec::new()),
        };
        assert!(matches!(
            discover(
                &clock,
                &client,
                "https://custom.invalid/literal",
                Some("KEY_DEADLINE_CANARY"),
                None,
                &AtomicBool::new(false)
            )
            .await,
            Err(DiscoveryError::Network)
        ));
        assert_eq!(clock.now_ms(), discovery::TOTAL_TIMEOUT_MS);
        assert_eq!(client.inner.requests.lock().unwrap().len(), 3);
        assert_eq!(
            *client.timeouts.lock().unwrap(),
            [15_000, 15_000, 1_750].map(Duration::from_millis)
        );
    }
    let clock = ClockFake::default();
    let client = TimedClient {
        clock: &clock,
        inner: Client::new(vec![
            (
                200,
                json!({"data":[{"id":"a"}],"has_more":true,"last_id":"a"}),
            ),
            (
                200,
                json!({"data":[{"id":"b"}],"has_more":false,"last_id":"b"}),
            ),
        ]),
        elapsed: Mutex::new([15_000, 14_999].into()),
        timeouts: Mutex::new(Vec::new()),
    };
    assert_eq!(
        discover(
            &clock,
            &client,
            "https://custom.invalid/literal",
            None,
            Some(NativeProtocol::Messages),
            &AtomicBool::new(false)
        )
        .await
        .unwrap()
        .models
        .len(),
        2
    );
    assert_eq!(clock.now_ms(), discovery::TOTAL_TIMEOUT_MS - 1);
}
