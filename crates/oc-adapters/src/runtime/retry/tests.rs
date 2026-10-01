use super::*;
use crate::provider::{
    Delivery, FailureKind, Operation, PhysicalFailure, ProviderError, RetryHeaders,
};

fn failure(kind: FailureKind) -> ProviderError {
    ProviderError::Request(Box::new(PhysicalFailure {
        kind,
        http_status: Some(503),
        event_status: None,
        delivery: Delivery::Rejected,
        operation: Operation::Request,
        transport: None,
        output_committed: false,
        headers: RetryHeaders::default(),
        message: None,
    }))
}

#[test]
fn ret01_policy_shared_ten_gaps_and_minimum() {
    let mut policy = RetryPolicy::default();
    for (n, delay) in [
        2000, 4000, 8000, 10000, 10000, 10000, 10000, 10000, 10000, 10000,
    ]
    .into_iter()
    .enumerate()
    {
        let decision = policy
            .decide_sample(&failure(FailureKind::ProviderInternal), 100, 0.5)
            .unwrap();
        assert_eq!((decision.at, decision.attempt), (100 + delay, n as u32 + 2));
    }
    assert!(
        policy
            .decide_sample(&failure(FailureKind::ProviderInternal), 100, 0.5)
            .is_none()
    );
    let mut error = failure(FailureKind::RateLimit);
    if let ProviderError::Request(f) = &mut error {
        f.headers.retry_after_ms = Some(u64::MAX);
    }
    assert_eq!(
        RetryPolicy::default()
            .decide_sample(&error, 0, 0.0)
            .unwrap()
            .at,
        900000
    );
    assert_eq!(
        RetryPolicy::default()
            .decide_sample(&failure(FailureKind::ProviderInternal), 0, 0.0)
            .unwrap()
            .at,
        1600
    );
    assert_eq!(
        RetryPolicy::default()
            .decide_sample(&failure(FailureKind::ProviderInternal), 0, 0.999999)
            .unwrap()
            .at,
        2400
    );
}

#[test]
fn ret01_policy_typed_terminal_override_partial_and_local_matrix() {
    for kind in [
        FailureKind::Quota,
        FailureKind::Authentication,
        FailureKind::ContentPolicy,
        FailureKind::InvalidRequest,
        FailureKind::PayloadTooLarge,
        FailureKind::ContextOverflow,
    ] {
        let mut error = failure(kind);
        assert!(
            RetryPolicy::default()
                .decide_sample(&error, 0, 0.5)
                .is_none()
        );
        if let ProviderError::Request(f) = &mut error {
            f.headers.should_retry = Some(true);
        }
        assert_eq!(
            RetryPolicy::default()
                .decide_sample(&error, 0, 0.5)
                .is_some(),
            kind != FailureKind::ContextOverflow
        );
    }
    let mut policy = RetryPolicy::default();
    for n in 0..11 {
        let mut error = failure(if n % 2 == 0 {
            FailureKind::ProviderInternal
        } else {
            FailureKind::IncompleteStream
        });
        if let ProviderError::Request(f) = &mut error
            && n % 2 == 1
        {
            f.delivery = Delivery::Accepted;
            f.operation = Operation::Read;
            f.output_committed = true;
            f.headers.should_retry = Some(false);
        }
        assert_eq!(
            policy.decide_sample(&error, 0, 0.5).is_some(),
            n < 10,
            "one allowance across both recovery modes"
        );
    }
    for error in [
        ProviderError::InvalidOutput,
        ProviderError::InvalidConfig,
        ProviderError::PrivateHost,
        ProviderError::ByteLimit("request"),
        ProviderError::DispatchRefused,
        ProviderError::Cancelled,
    ] {
        assert!(
            RetryPolicy::default()
                .decide_sample(&error, 0, 0.5)
                .is_none()
        );
    }
}

#[test]
fn ret01_recovery_preserves_historical_retry_without_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.accept_turn(
        "t",
        "s",
        "input",
        "input",
        &oc_core::queries::ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        },
    )
    .unwrap();
    let mut log = TurnLog::new("t", "m", "fixture");
    let retry = oc_core::queries::RetryFact {
        attempt: 2,
        at: 42,
        safe_error: "provider internal error".into(),
    };
    for (id, completed, status) in [("old", Some(1), "failed"), ("active", None, "started")] {
        log.spans.push(oc_core::queries::AssistantSpan {
            request: None,
            id: id.into(),
            step: 1,
            status: status.into(),
            started: 0,
            completed,
            retry: Some(retry.clone()),
            error: Some("provider internal error".into()),
            finish: None,
        });
    }
    db.checkpoint_retry("s", "t", "active", &log.to_json().to_string(), &retry)
        .unwrap();
    db.recover_interrupted_tools().unwrap();
    let projection = db.turn_presentation("s", "t").unwrap().unwrap();
    assert_eq!(projection.status, "unknown");
    assert_eq!(projection.spans[0], log.spans[0]);
    assert_eq!(projection.spans[1].status, "unknown");
    assert!(projection.spans[1].completed.is_some());
    assert_eq!(projection.spans[1].retry, Some(retry));
    let conn = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM events WHERE kind='generation_dispatched'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn ret01_dispatch_family_is_fixed_across_later_root_turn_and_uses_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("root").unwrap();
    db.create_child_session("root", "child", None, None, None)
        .unwrap();
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    db.accept_turn("old", "root", "old", "old", &model).unwrap();
    db.commit_turn("old", "completed", Some("{}"), None)
        .unwrap();
    db.accept_turn("new", "root", "new", "new", &model).unwrap();
    db.accept_turn("child-turn", "child", "child", "child", &model)
        .unwrap();
    let mut log = TurnLog::new("child-turn", "m", "fixture");
    log.display["owning_operation"] = "old".into();
    db.checkpoint_turn("child-turn", &log.to_json().to_string())
        .unwrap();
    db.generation_dispatch("child", "child-turn", "child")
        .unwrap();
    let conn = rusqlite::Connection::open(db.root().join("oc.sqlite")).unwrap();
    let operation:String=conn.query_row("SELECT json_extract(payload,'$.operation') FROM events WHERE kind='generation_dispatched'",[],|r|r.get(0)).unwrap();
    assert_eq!(operation, "old");
    let plan:String=conn.query_row("EXPLAIN QUERY PLAN SELECT count(*) FROM events WHERE kind='generation_dispatched' AND json_extract(payload,'$.operation')='old'",[],|r|r.get(3)).unwrap();
    assert!(plan.contains("events_generation_operation"), "{plan}");
}
