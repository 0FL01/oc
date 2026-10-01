use super::*;
use oc_core::queries::{AssistantSpan, HistoryTurn, RetryFact};

#[tokio::test]
async fn ret01_retry_scope_due_semantic_start_and_stale_snapshot() {
    let mut state = fresh_state("retry-view").await;
    let turn = WorkerTurnId("turn".into());
    state.active_turn = Some(turn.clone());
    state.status = TuiStatus::Streaming;
    state.chrome.animations = Some(false);
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let retry = RetryFact {
        attempt: 2,
        at: epoch + 2000,
        safe_error: "provider internal error".into(),
    };
    state.apply_retry(&WorkerTurnId("foreign".into()), "old", &retry);
    assert!(state.retry_notice().is_none());
    state.apply_retry(&turn, "old", &retry);
    assert!(state.retry_notice().unwrap().contains("Retry scheduled"));
    let due = state.retry_due.unwrap();
    assert_eq!(state.next_ui_deadline(), Some(due));
    assert!(state.tick_ui(due));
    assert!(state.retry_notice().unwrap().contains("Retry due"));
    assert!(
        state.active_turn.is_some(),
        "due is presentation, never dispatch"
    );
    state.apply_presentation(
        &turn,
        &HistoryTurn {
            id: turn.0.clone(),
            status: "started".into(),
            revision: 3,
            spans: vec![AssistantSpan {
                request: None,
                id: "old".into(),
                step: 1,
                status: "started".into(),
                started: epoch,
                completed: None,
                retry: None,
                error: None,
                finish: None,
            }],
            ..Default::default()
        },
    );
    state.apply_retry(&turn, "old", &retry);
    assert!(
        state.retry_notice().is_none(),
        "late same-ID retry cannot undo semantic start"
    );
    let rescheduled = RetryFact {
        attempt: 3,
        ..retry.clone()
    };
    state.apply_retry(&turn, "old", &rescheduled);
    state.apply_retry(&turn, "old", &retry);
    assert!(
        state.retry_notice().unwrap().contains("attempt 3"),
        "older reschedule cannot replace newest fact"
    );
    let old = AssistantSpan {
        request: None,
        id: "old".into(),
        step: 1,
        status: "failed".into(),
        started: 0,
        completed: Some(1),
        retry: Some(retry.clone()),
        error: Some("incomplete stream".into()),
        finish: None,
    };
    let new = AssistantSpan {
        request: None,
        id: "new".into(),
        step: 1,
        status: "started".into(),
        started: 2,
        completed: None,
        retry: None,
        error: None,
        finish: None,
    };
    let projection = HistoryTurn {
        id: turn.0.clone(),
        status: "started".into(),
        revision: 5,
        spans: vec![old.clone(), new],
        ..Default::default()
    };
    state.apply_presentation(&turn, &projection);
    assert!(state.retry_notice().is_none());
    state.apply_retry(&turn, "old", &retry);
    assert!(
        state.retry_notice().is_none(),
        "settled historical retry cannot poison new span"
    );
    state.apply_presentation(
        &turn,
        &HistoryTurn {
            revision: 4,
            spans: vec![old],
            ..projection.clone()
        },
    );
    assert!(
        state.retry_notice().is_none(),
        "old snapshot cannot roll back semantic start"
    );
    state.apply_retry(&turn, "new", &retry);
    assert!(state.retry_notice().is_some());
    state.begin_compress_turn(WorkerTurnId("next-operation".into()));
    assert!(
        state.retry_notice().is_none(),
        "new operation must not inherit active wait"
    );
    state.apply_retry(&turn, "new", &retry);
    assert!(state.retry_notice().is_none());
    state.app.shutdown().await.unwrap();
}

#[tokio::test]
async fn ret01_reopen_failed_retry_is_history_not_active_wait() {
    let mut state = fresh_state("reopen-retry").await;
    let mut row = msg(1, Role::User, "input");
    row.turn = Some(HistoryTurn {
        id: "finished".into(),
        status: "completed".into(),
        spans: vec![AssistantSpan {
            request: None,
            id: "failed-span".into(),
            step: 1,
            status: "failed".into(),
            started: 0,
            completed: Some(1),
            retry: Some(RetryFact {
                attempt: 2,
                at: 42,
                safe_error: "rate limit".into(),
            }),
            error: Some("rate limit".into()),
            finish: None,
        }],
        ..Default::default()
    });
    state.attach_page(&page(vec![row], 1, false, false));
    assert!(state.retry_notice().is_none());
    assert!(
        state
            .viewport()
            .iter()
            .any(|line| line.contains("Historical retry"))
    );
    assert!(!state.is_busy());
    state.app.shutdown().await.unwrap();
}
