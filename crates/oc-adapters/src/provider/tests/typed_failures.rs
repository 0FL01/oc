use super::*;

#[tokio::test]
async fn ret01_coalesced_frames_yield_to_scoped_consumer_without_lag() {
    // Match the core owner's existing bounded broadcast, without raising it.
    let (sender, mut receiver) = tokio::sync::broadcast::channel(256);
    let consumer = tokio::spawn(async move {
        for _ in 0..4096 {
            receiver.recv().await?;
        }
        Ok::<_, tokio::sync::broadcast::error::RecvError>(4096)
    });
    let mut wire = sse_delta("burst").repeat(4096);
    wire.extend(sse_completed(10, 20));
    let server = TestServer::spawn(Arc::new(move |_| Action {
        status: "200 OK",
        headers: vec![("Content-Length", wire.len().to_string())],
        chunks: vec![(wire.clone(), 0)],
        abort_after: None,
    }))
    .await;
    let generation = crate::provider::stream_generation_observed(
        &test_config(&server.base),
        "m",
        None,
        "input",
        &[],
        &NO_CANCEL,
        None,
        &mut |item| {
            if matches!(item, StreamItem::TextDelta(_)) {
                let _ = sender.send(());
            }
        },
    )
    .await
    .unwrap();
    drop(sender);
    assert_eq!(generation.text.len(), 20480);
    assert_eq!(consumer.await.unwrap().unwrap(), 4096);
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
}
use crate::provider::{
    Delivery, FailureKind, FinishReason, Operation, PhysicalFailure, RetryHeaders, TransportKind,
};

fn facts(error: ProviderError) -> PhysicalFailure {
    match error {
        ProviderError::Request(failure) => *failure,
        other => panic!("expected physical facts, got {other:?}"),
    }
}

async fn http(
    status: &'static str,
    body: serde_json::Value,
    headers: Vec<(&'static str, String)>,
) -> PhysicalFailure {
    let server = TestServer::spawn(Arc::new(move |_| Action {
        status,
        headers: headers.clone(),
        chunks: vec![(body.to_string().into_bytes(), 0)],
        abort_after: None,
    }))
    .await;
    let error = stream_generation(
        &test_config(&server.base),
        "unknown-model",
        None,
        "prompt",
        &[],
        &NO_CANCEL,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    facts(error)
}

#[tokio::test]
async fn ret01_http_statuses_precedence_safe_facts() {
    use FailureKind::*;
    for (status, number, code, kind) in [
        ("429 Throttle", 429, "rate_limit_exceeded", RateLimit),
        ("429 Quota", 429, "insufficient_quota", Quota),
        ("402 Payment", 402, "", Quota),
        ("408 Timeout", 408, "", ProviderInternal),
        ("409 Conflict", 409, "", ProviderInternal),
        (
            "500 Server",
            500,
            "context_length_exceeded",
            ProviderInternal,
        ),
        ("503 Server", 503, "server_error", ProviderInternal),
        ("400 Invalid", 400, "server_error", InvalidRequest),
        ("401 Auth", 401, "", Authentication),
        ("403 Auth", 403, "", Authentication),
        ("413 Payload", 413, "", PayloadTooLarge),
        ("422 Invalid", 422, "", InvalidRequest),
        (
            "400 Context",
            400,
            "model_context_window_exceeded",
            ContextOverflow,
        ),
        ("413 Context", 413, "request_too_large", ContextOverflow),
        ("401 Quota", 401, "billing_error", Quota),
        ("402 Policy", 402, "content_filter", ContentPolicy),
        (
            "429 Policy",
            429,
            "image_content_policy_violation",
            ContentPolicy,
        ),
        ("429 Auth", 429, "permission_error", Authentication),
    ] {
        let outcome = http(
            status,
            serde_json::json!({"error":{"code":code},"raw":"body-canary https://secret.invalid"}),
            vec![],
        )
        .await;
        assert_eq!(outcome.kind, kind, "{status}/{code}");
        assert_eq!(outcome.http_status, Some(number));
        assert_eq!(outcome.event_status, None);
        assert_eq!(outcome.delivery, Delivery::Rejected);
        assert_eq!(outcome.operation, Operation::Request);
        assert_eq!(
            outcome.retry_eligible(),
            matches!(kind, RateLimit | ProviderInternal)
        );
        assert!(!format!("{outcome:?} {outcome}").contains("canary"));
        assert!(!format!("{outcome:?} {outcome}").contains("secret.invalid"));
    }
    for code in [
        "usage_not_included",
        "GoUsageLimitError",
        "FreeUsageLimitError",
        "CreditLimitExceeded",
    ] {
        assert_eq!(
            http(
                "429 Quota",
                serde_json::json!({"error":{"type":code}}),
                vec![]
            )
            .await
            .kind,
            Quota
        );
    }
}

#[tokio::test]
async fn ret01_observed_override_and_delay_only_http_rate_internal() {
    for (value, expected) in [
        ("true", Some(true)),
        ("false", Some(false)),
        ("True", None),
        ("FALSE", None),
        (" true ", Some(true)),
        ("true, false", None),
    ] {
        // HTTP parser trims field whitespace; facts use its observed exact value.
        let error = http(
            "402 Payment",
            serde_json::json!({}),
            vec![
                ("x-should-retry", value.into()),
                ("retry-after", "90".into()),
                ("Authorization", "body-canary".into()),
            ],
        )
        .await;
        assert_eq!(error.headers.should_retry, expected);
        assert_eq!(error.retry_eligible(), expected.unwrap_or(false));
        assert_eq!(error.headers.retry_after_ms, None);
        assert!(!format!("{error:?}").contains("body-canary"));
    }
    for (ms, after, expected) in [
        ("5.2", "99", 6),
        ("0", "99", 0),
        ("-1", "2", 2000),
        ("NaN", "3", 3000),
        ("inf", "3", 3000),
        ("9999999999", "1", 900000),
    ] {
        let error = http(
            "429 Throttle",
            serde_json::json!({}),
            vec![("retry-after-ms", ms.into()), ("Retry-After", after.into())],
        )
        .await;
        assert_eq!(error.headers.retry_after_ms, Some(expected));
    }
    for date in [
        "Sun, 06 Nov 2094 08:49:37 GMT",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 2094",
    ] {
        let expected = if date.contains("-94") { 0 } else { 900000 };
        assert_eq!(
            http(
                "503 Server",
                serde_json::json!({}),
                vec![("Retry-After", date.into())]
            )
            .await
            .headers
            .retry_after_ms,
            Some(expected)
        );
    }
    for value in [
        "-2",
        "NaN",
        "inf",
        "not a date",
        "Sun, 32 Nov 2094 08:49:37 GMT",
    ] {
        assert_eq!(
            http(
                "503 Server",
                serde_json::json!({}),
                vec![("Retry-After", value.into())]
            )
            .await
            .headers
            .retry_after_ms,
            None
        );
    }
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "x-should-retry",
        reqwest::header::HeaderValue::from_str(&"t".repeat(129)).unwrap(),
    );
    headers.insert(
        "retry-after-ms",
        reqwest::header::HeaderValue::from_str(&"1".repeat(129)).unwrap(),
    );
    assert_eq!(
        RetryHeaders::observed(&headers, true),
        RetryHeaders::default()
    );
}

#[tokio::test]
async fn ret01_sse_event_status_independent_of_http_and_no_delay() {
    use FailureKind::*;
    for (event, kind, status) in [
        (
            serde_json::json!({"type":"response.failed","response":{"error":{"code":"insufficient_quota"}}}),
            Quota,
            None,
        ),
        (
            serde_json::json!({"type":"error","status_code":429,"error":{"type":"rate_limit_exceeded"}}),
            RateLimit,
            Some(429),
        ),
        (
            serde_json::json!({"type":"response.failed","status":503,"response":{"error":{"code":"context_length_exceeded"}}}),
            ProviderInternal,
            Some(503),
        ),
        (
            serde_json::json!({"type":"response.failed","response":{"error":{"code":"context_length_exceeded"}}}),
            ContextOverflow,
            None,
        ),
        (
            serde_json::json!({"type":"error","error":{"innererror":{"code":"ResponsibleAIPolicyViolation"},"type":"authentication_error"}}),
            ContentPolicy,
            None,
        ),
        (serde_json::json!({"type":"error"}), ProviderInternal, None),
        (
            serde_json::json!({"type":"response.failed","response":{"error":{"message":"UNRECOGNIZED-CANARY"}}}),
            UnknownProvider,
            None,
        ),
    ] {
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "200 OK",
            headers: vec![
                ("retry-after", "600".into()),
                ("retry-after-ms", "90000".into()),
                ("x-should-retry", "false".into()),
            ],
            chunks: vec![(super::event(event.clone()), 0)],
            abort_after: None,
        }))
        .await;
        let error = facts(
            stream_generation(
                &test_config(&server.base),
                "m",
                None,
                "x",
                &[],
                &NO_CANCEL,
                None,
            )
            .await
            .unwrap_err(),
        );
        assert_eq!(error.kind, kind);
        assert_eq!(error.http_status, Some(200));
        assert_eq!(error.event_status, status);
        assert_eq!(
            error.headers,
            RetryHeaders {
                should_retry: Some(false),
                retry_after_ms: None
            }
        );
        assert!(!error.retry_eligible());
        assert!(!error.output_committed);
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        assert!(!format!("{error:?} {error}").contains("CANARY"));
        server.shutdown();
    }
}

#[tokio::test]
async fn ret01_incomplete_length_filter_eof_and_tools() {
    for (reason, expected) in [
        ("max_output_tokens", None),
        ("content_filter", Some(FailureKind::ContentPolicy)),
        ("unknown", Some(FailureKind::IncompleteStream)),
    ] {
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status:"200 OK",headers:vec![],chunks:vec![(sse_delta("partial"),0),(event(serde_json::json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":reason},"usage":{"input_tokens":2,"output_tokens":3}}})),0)],abort_after:None,
        })).await;
        let result = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await;
        match expected {
            None => {
                let g = result.unwrap();
                assert_eq!(g.finish, FinishReason::Length);
                assert_eq!(g.text, "partial");
                assert_eq!(g.usage, Some((2, 3)));
                assert_eq!(g.compaction_usage.unwrap().output_tokens, 3);
            }
            Some(kind) => {
                let error = facts(result.unwrap_err());
                assert_eq!(error.kind, kind);
                assert!(error.output_committed);
            }
        }
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }
    for finish in [None, Some("max_output_tokens")] {
        let server = TestServer::spawn(Arc::new(move |_| {
            let mut chunks = vec![(event(serde_json::json!({"type":"response.output_item.added","item":{"id":"fc","type":"function_call","call_id":"c","name":"shell"}})),0),(event(serde_json::json!({"type":"response.function_call_arguments.delta","item_id":"fc","delta":"{"})),0)];
            if let Some(reason) = finish {chunks.push((event(serde_json::json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":reason}}})),0));}
            Action {status:"200 OK",headers:vec![("x-should-retry","false".into())],chunks,abort_after:None}
        })).await;
        let error = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .unwrap_err();
        if finish.is_some() {
            assert!(matches!(
                error,
                ProviderError::OutputStructure {
                    stage: crate::provider::OutputStage::Completion,
                    code: crate::provider::OutputCode::MissingDone,
                }
            ));
            assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
            server.shutdown();
            continue;
        }
        let error = facts(error);
        assert_eq!(error.kind, FailureKind::IncompleteStream);
        assert_eq!(error.delivery, Delivery::Accepted);
        assert!(error.output_committed);
        assert!(
            error.retry_eligible(),
            "continuation facts, no dispatch here"
        );
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }
    let call = serde_json::json!({"type":"function_call","id":"fc","call_id":"c","name":"shell","arguments":"{}","status":"completed"});
    let expected = call.clone();
    let server = TestServer::spawn(Arc::new(move |_| Action {
        status: "200 OK", headers: vec![],
        chunks: vec![(event(serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call})),0), (event(serde_json::json!({"type":"response.incomplete","response":{"output":[call],"incomplete_details":{"reason":"max_output_tokens"}}})),0)],
        abort_after: None,
    })).await;
    let generation = stream_generation(
        &test_config(&server.base),
        "m",
        None,
        "x",
        &[],
        &NO_CANCEL,
        None,
    )
    .await
    .unwrap();
    assert_eq!(generation.finish, FinishReason::Length);
    assert_eq!(generation.output, vec![expected]);
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
}

#[tokio::test]
async fn ret01_local_parser_and_validation_never_provider_override() {
    for wire in [b"data: invalid\n\n".to_vec(), b"data: \xff\n\n".to_vec()] {
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "200 OK",
            headers: vec![("x-should-retry", "true".into())],
            chunks: vec![(wire.clone(), 0)],
            abort_after: None,
        }))
        .await;
        let error = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            ProviderError::OutputStructure { .. } | ProviderError::InvalidUtf8
        ));
        assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
        server.shutdown();
    }
    let mut config = test_config("http://127.0.0.1:9");
    config.headers.insert("host".into(), "host-canary".into());
    assert_eq!(
        stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None).await,
        Err(ProviderError::InvalidConfig)
    );
}

async fn length_output(
    prefix: Vec<serde_json::Value>,
    item: serde_json::Value,
) -> Result<super::super::Generation, ProviderError> {
    let server = TestServer::spawn(Arc::new(move |_| {
        let mut chunks: Vec<_> = prefix.iter().map(|value| (event(value.clone()), 0)).collect();
        chunks.push((event(serde_json::json!({"type":"response.incomplete","response":{"output":[item],"incomplete_details":{"reason":"max_output_tokens"}}})),0));
        Action {status:"200 OK",headers:vec![],chunks,abort_after:None}
    })).await;
    let result = stream_generation(
        &test_config(&server.base),
        "m",
        None,
        "x",
        &[],
        &NO_CANCEL,
        None,
    )
    .await;
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    result
}

#[tokio::test]
async fn ret01_review_length_call_requires_prior_matching_done() {
    let call = serde_json::json!({"type":"function_call","id":"fc","call_id":"c","name":"shell","arguments":"{}","status":"completed"});
    assert!(
        length_output(vec![], call.clone()).await.is_err(),
        "terminal-only call must not recover on response.incomplete"
    );
    let done = serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call});
    let result = length_output(vec![done.clone()], call.clone())
        .await
        .unwrap();
    assert_eq!(result.finish, FinishReason::Length);
    assert_eq!(result.output, vec![call.clone()]);
    let pending = serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"other","call_id":"other","name":"shell"}});
    assert!(
        length_output(vec![done.clone(), pending], call.clone())
            .await
            .is_err(),
        "completed call must not silently filter an unresolved announced call"
    );
    let mut changed = call.clone();
    changed["arguments"] = serde_json::json!("{\"changed\":true}");
    assert!(
        length_output(vec![done], changed).await.is_err(),
        "length must not rewrite an emitted completed call's body"
    );
    let announced = serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"fc","call_id":"c","name":"shell"}});
    assert!(
        length_output(vec![announced], call.clone()).await.is_err(),
        "announced call without done remains incomplete"
    );
    let announced = serde_json::json!({"type":"response.output_item.added","item":{"type":"function_call","id":"fc","call_id":"c","name":"shell"}});
    let done = serde_json::json!({"type":"response.output_item.done","output_index":0,"item":call});
    let message = serde_json::json!({"type":"message","role":"assistant","status":"in_progress","content":[{"type":"output_text","text":"partial"}]});
    let result = length_output(vec![announced, done], message.clone())
        .await
        .unwrap();
    assert_eq!(
        result.output,
        vec![call.clone(), message],
        "length preserves an already emitted real call even when terminal output omits it"
    );
    let mut no_id = call;
    no_id.as_object_mut().unwrap().remove("id");
    let done = serde_json::json!({"type":"response.output_item.done","item":no_id});
    assert!(
        length_output(vec![done], no_id).await.is_err(),
        "length cannot match anonymous completion evidence"
    );
}

#[tokio::test]
async fn ret01_review_length_only_partial_assistant_messages_escape_status_validation() {
    for kind in ["compaction", "reasoning", "future_opaque"] {
        for status in ["failed", "in_progress"] {
            let item = serde_json::json!({"type":kind,"id":"opaque","status":status});
            assert!(
                length_output(vec![], item.clone()).await.is_err(),
                "length must retain opaque {kind}/{status} validation"
            );
            let done = serde_json::json!({"type":"response.output_item.done","item":item});
            assert!(
                length_output(vec![done], item).await.is_err(),
                "done cannot bypass ordinary validation"
            );
        }
    }
    for status in ["in_progress", "incomplete"] {
        let item = serde_json::json!({"type":"message","role":"assistant","id":"m","status":status,"content":[{"type":"output_text","text":"partial"}]});
        let result = length_output(vec![], item.clone()).await.unwrap();
        assert_eq!(result.finish, FinishReason::Length);
        assert_eq!(result.output, vec![item]);
    }
    for role in ["assistant", "user"] {
        let item = serde_json::json!({"type":"message","role":role,"status":"failed"});
        assert!(length_output(vec![], item).await.is_err());
    }
    let item = serde_json::json!({"type":"message","role":"user","status":"in_progress"});
    assert!(length_output(vec![], item).await.is_err());
}

#[tokio::test]
async fn ret01_source_patterns_are_bounded_error_fields_not_generated_text() {
    use FailureKind::*;
    for (message, status, expected) in [
        (
            "exceeds the model's maximum context length of 12,345 tokens",
            None,
            ContextOverflow,
        ),
        (
            "exceeds maximum context length (8192)",
            None,
            ContextOverflow,
        ),
        ("exceeds maximum context length", None, UnknownProvider),
        (
            "maximum context length is 12,345 tokens",
            None,
            UnknownProvider,
        ),
        ("Throttling error: too many tokens", None, UnknownProvider),
        ("rate limit: too many tokens", None, RateLimit),
        ("Gateway: [GoUsageLimitError] account caps", None, Quota),
        ("Gateway: [permission_error] auth", None, Authentication),
        ("please retry the request", None, ProviderInternal),
        ("notservererror", None, UnknownProvider),
        ("server_error", Some(400), InvalidRequest),
        ("too many tokens", Some(503), ProviderInternal),
        ("content policy rejection", Some(503), ProviderInternal),
        ("content policy rejection", Some(400), ContentPolicy),
        ("400 status code (no body)", None, ContextOverflow),
    ] {
        let error = super::super::failure::classified(
            Some(&serde_json::json!({"error":{"message":message}})),
            status,
            false,
        );
        assert_eq!(error.kind, expected, "{message}");
        assert!(error.message.is_none());
    }
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "200 OK",
        headers: vec![],
        chunks: vec![
            (
                sse_delta("insufficient_quota content_filter too many tokens try again"),
                0,
            ),
            (sse_completed(1, 1), 0),
        ],
        abort_after: None,
    }))
    .await;
    let generation = stream_generation(
        &test_config(&server.base),
        "m",
        None,
        "x",
        &[],
        &NO_CANCEL,
        None,
    )
    .await
    .unwrap();
    assert_eq!(generation.finish, FinishReason::Stop);
    assert!(generation.text.contains("insufficient_quota"));
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
}

#[tokio::test]
async fn ret01_bounded_diagnostic_body_and_held_error_body_cancel() {
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "429 Throttle",
        headers: vec![],
        chunks: vec![(vec![b'x'; 16 * 1024 + 1], 0)],
        abort_after: None,
    }))
    .await;
    let error = facts(
        stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await
        .unwrap_err(),
    );
    assert_eq!(error.kind, FailureKind::RateLimit);
    assert!(error.message.is_none());
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "401 Auth",
        headers: vec![],
        chunks: vec![(b"{}".to_vec(), 5000)],
        abort_after: None,
    }))
    .await;
    let cancel = AtomicBool::new(false);
    let config = test_config(&server.base);
    let request = stream_generation(&config, "m", None, "x", &[], &cancel, None);
    let signal = async {
        tokio::time::sleep(Duration::from_millis(40)).await;
        cancel.store(true, Ordering::Relaxed);
    };
    let (result, ()) = tokio::join!(request, signal);
    assert_eq!(result, Err(ProviderError::Cancelled));
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    let headers = reqwest::header::HeaderMap::from_iter([(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static("Bearer auth-canary"),
    )]);
    for message in [
        "bad model auth-canary",
        "see https://private.invalid",
        "environment env-canary",
    ] {
        let safe = super::super::safe_diagnostic(message, &headers).unwrap();
        assert!(!safe.contains("canary"));
        assert!(!safe.contains("private.invalid"));
    }
}

#[tokio::test]
async fn ret01_read_transport_retains_status_override_and_continuation_facts() {
    let server = TestServer::spawn(Arc::new(|_| Action {
        status: "200 OK",
        headers: vec![("x-should-retry", "false".into())],
        chunks: vec![(sse_delta("committed"), 0), (sse_completed(1, 1), 5000)],
        abort_after: None,
    }))
    .await;
    let mut config = test_config(&server.base);
    config.chunk_timeout_ms = 40;
    let error = facts(
        stream_generation(&config, "m", None, "x", &[], &NO_CANCEL, None)
            .await
            .unwrap_err(),
    );
    assert_eq!(error.kind, FailureKind::Transport);
    assert_eq!(error.transport, Some(TransportKind::IdleTimeout));
    assert_eq!(error.http_status, Some(200));
    assert_eq!(error.delivery, Delivery::Accepted);
    assert_eq!(error.operation, Operation::Read);
    assert!(error.output_committed && error.retry_eligible());
    assert_eq!(server.attempts.load(Ordering::SeqCst), 1);
    server.shutdown();
    let mut error = PhysicalFailure::transport(Operation::Request, Delivery::Rejected, None);
    assert!(!error.retry_eligible());
    error.headers.should_retry = Some(true);
    assert!(error.retry_eligible());
    error.headers = RetryHeaders::default();
    error.delivery = Delivery::Accepted;
    assert!(!error.retry_eligible());
}

#[tokio::test]
async fn ret01_one_physical_attempt_throttle_and_quota() {
    for code in ["rate_limit_exceeded", "insufficient_quota"] {
        let server = TestServer::spawn(Arc::new(move |_| Action {
            status: "429 Too Many Requests",
            headers: vec![],
            chunks: vec![(
                serde_json::json!({"error":{"code":code}})
                    .to_string()
                    .into_bytes(),
                0,
            )],
            abort_after: None,
        }))
        .await;
        let result = stream_generation(
            &test_config(&server.base),
            "m",
            None,
            "x",
            &[],
            &NO_CANCEL,
            None,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(
            server.attempts.load(Ordering::SeqCst),
            1,
            "one POST for {code}"
        );
        if code == "insufficient_quota" {
            assert!(result.unwrap_err().to_string().contains("quota"));
        }
        server.shutdown();
    }
}
