use super::*;

const LAUNCH_OUTPUT: &str = "{\"status\":\"running\",\"sessionID\":\"child\",\"jobGeneration\":\"launch\",\"deliveryID\":\"child-delivery:launch\"}";

#[test]
fn child_tool_projection_requires_exact_launch_and_keeps_each_continuation_generation() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let job = admitted(&db);
    let original = db.list_tool_ops("parent").unwrap();
    assert_eq!(
        db.child_tool_job("parent", "launch").unwrap(),
        Some(job.clone())
    );
    assert!(db.child_tool_job("child", "launch").unwrap().is_none());
    assert!(db.child_tool_job("foreign", "launch").unwrap().is_none());
    assert!(db.child_tool_job("parent", "copied").unwrap().is_none());
    let (identity, launch) = {
        let conn = db.conn.lock().unwrap();
        (
            conn.query_row(
                "SELECT identity FROM child_jobs WHERE operation_id='launch'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            conn.query_row(
                "SELECT payload FROM events WHERE kind='subagent_launch'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        )
    };
    for field in [
        "parent",
        "child",
        "operation",
        "delivery_id",
        "location",
        "generation",
    ] {
        let mut invalid: serde_json::Value = serde_json::from_str(&identity).unwrap();
        invalid[field] = if field == "generation" {
            serde_json::json!(8)
        } else {
            serde_json::json!("foreign")
        };
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
                [invalid.to_string()],
            )
            .unwrap();
        assert!(
            db.child_tool_job("parent", "launch").unwrap().is_none(),
            "{field}"
        );
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
            [&identity],
        )
        .unwrap();
    for generation in [
        serde_json::json!(true),
        serde_json::json!(7.0),
        serde_json::json!(8),
    ] {
        let mut invalid: serde_json::Value = serde_json::from_str(&launch).unwrap();
        invalid["generation"] = generation;
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET payload=?1 WHERE kind='subagent_launch'",
                [invalid.to_string()],
            )
            .unwrap();
        assert!(db.child_tool_job("parent", "launch").unwrap().is_none());
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE events SET payload='broken' WHERE kind='subagent_launch'",
            [],
        )
        .unwrap();
    assert!(db.child_tool_job("parent", "launch").unwrap().is_none());
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE events SET payload=?1 WHERE kind='subagent_launch'",
            [&launch],
        )
        .unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE events SET kind='not-launch' WHERE kind='subagent_launch'",
            [],
        )
        .unwrap();
    assert!(db.child_tool_job("parent", "launch").unwrap().is_none());
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE events SET kind='subagent_launch' WHERE kind='not-launch'",
            [],
        )
        .unwrap();
    // A large but admitted identity has a bounded display caption, not copied
    // child output. Oversized identities are honestly unavailable altogether.
    let mut caption = job.clone();
    caption.description = "Ω界".repeat(2000);
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
            [serde_json::to_string(&caption).unwrap()],
        )
        .unwrap();
    let bounded = db.child_tool_job("parent", "launch").unwrap().unwrap();
    assert!(bounded.description.len() <= TOOL_OP_PREVIEW_BYTES);
    assert!(bounded.description.ends_with('…'));
    assert!(bounded.retained_bytes() <= oc_core::tool_output::RECORD_BYTES);
    caption.description = "x".repeat(oc_core::tool_output::RECORD_BYTES);
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
            [serde_json::to_string(&caption).unwrap()],
        )
        .unwrap();
    assert!(db.child_tool_job("parent", "launch").unwrap().is_none());
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
            [&identity],
        )
        .unwrap();
    db.finish_child_job(
        "launch",
        ChildState::Completed,
        "literal cancelled error must not classify state",
    )
    .unwrap();
    let completed = db.child_tool_job("parent", "launch").unwrap().unwrap();
    assert_eq!(completed.state, ChildState::Completed);
    assert!(completed.background && completed.result.is_none());
    let continued = ChildJob {
        operation: "continue".into(),
        generation: 8,
        delivery_id: "child-delivery:continue".into(),
        ..job
    };
    db.record_tool_intent(
        "continue",
        "parent",
        None,
        "subagent",
        r#"{"sessionID":"child","description":"again"}"#,
    )
    .unwrap();
    db.admit_child_job(&continued).unwrap();
    assert_eq!(
        db.child_tool_job("parent", "continue").unwrap(),
        Some(continued)
    );
    assert_eq!(
        db.child_tool_job("parent", "launch").unwrap(),
        Some(completed)
    );
    assert_eq!(
        db.list_tool_ops("parent")
            .unwrap()
            .into_iter()
            .find(|row| row.op == "launch")
            .unwrap(),
        original[0]
    );
}

#[test]
fn conversion_terminal_race_has_one_mode_event_one_notice_and_immutable_launch() {
    for convert_first in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let mut job = admitted(&db);
        job.background = false;
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
                [serde_json::to_string(&job).unwrap()],
            )
            .unwrap();
        let before = db.list_tool_ops("parent").unwrap();
        let captured = db.child_tool_job("parent", "launch").unwrap().unwrap();
        assert!(!captured.background);
        let launch = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT identity FROM child_jobs WHERE operation_id='launch'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        if convert_first {
            assert!(db.background_child_job("launch").unwrap());
            assert!(db.background_child_job("launch").unwrap());
        }
        db.finish_child_job(
            "launch",
            ChildState::Completed,
            "error: typed successful prose",
        )
        .unwrap();
        assert!(!db.background_child_job("launch").unwrap());
        db.finish_child_job(
            "launch",
            ChildState::Cancelled,
            "duplicate must not replace completion",
        )
        .unwrap();
        let current = db.child_jobs("parent").unwrap().remove(0);
        assert_eq!(current.state, ChildState::Completed);
        assert_eq!(current.background, convert_first);
        let projected = db.child_tool_job("parent", "launch").unwrap().unwrap();
        assert_eq!(projected.state, ChildState::Completed);
        assert_eq!(projected.background, convert_first);
        assert_eq!(projected.parent, captured.parent);
        assert_eq!(projected.child, captured.child);
        assert_eq!(projected.operation, captured.operation);
        assert_eq!(projected.location, captured.location);
        assert_eq!(projected.generation, captured.generation);
        assert!(projected.result.is_none());
        assert_eq!(
            current.result.as_deref(),
            Some("error: typed successful prose")
        );
        assert_eq!(
            db.child_job_outstanding().unwrap(),
            i64::from(convert_first)
        );
        assert_eq!(
            db.deliver_child_notices().unwrap().len(),
            usize::from(convert_first)
        );
        assert!(db.deliver_child_notices().unwrap().is_empty());
        assert_eq!(db.list_tool_ops("parent").unwrap(), before);
        let conn = db.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT identity FROM child_jobs WHERE operation_id='launch'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            launch
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM events WHERE kind='subagent_background'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            i64::from(convert_first)
        );
    }
}

#[test]
fn actual_concurrent_conversion_and_terminal_share_the_existing_transaction_owner() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let mut job = admitted(&db);
    job.background = false;
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
            [serde_json::to_string(&job).unwrap()],
        )
        .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let accepted = std::thread::scope(|scope| {
        let owner = db.shared_handle();
        let start = barrier.clone();
        let control = scope.spawn(move || {
            start.wait();
            owner.background_child_job("launch").unwrap()
        });
        let owner = db.shared_handle();
        let start = barrier.clone();
        let terminal = scope.spawn(move || {
            start.wait();
            owner
                .finish_child_job("launch", ChildState::Completed, "once")
                .unwrap();
        });
        terminal.join().unwrap();
        control.join().unwrap()
    });
    assert!(!db.background_child_job("launch").unwrap());
    assert_eq!(db.child_jobs("parent").unwrap()[0].background, accepted);
    assert_eq!(
        db.child_jobs("parent").unwrap()[0].state,
        ChildState::Completed
    );
    assert_eq!(
        db.deliver_child_notices().unwrap().len(),
        usize::from(accepted)
    );
    assert!(db.deliver_child_notices().unwrap().is_empty());
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM events WHERE kind='subagent_background'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        i64::from(accepted)
    );
}

fn admitted(db: &Db) -> ChildJob {
    admitted_input(db, "{\"background\":true}")
}

fn admitted_input(db: &Db, input: &str) -> ChildJob {
    admitted_description(db, input, "work")
}

fn admitted_description(db: &Db, input: &str, description: &str) -> ChildJob {
    db.create_session("parent").unwrap();
    db.create_child_session(
        "parent",
        "child",
        Some("helper"),
        Some("fixture/child"),
        Some("work"),
    )
    .unwrap();
    db.record_tool_intent("launch", "parent", None, "subagent", input)
        .unwrap();
    db.record_tool_outcome("launch", "running", Some(LAUNCH_OUTPUT))
        .unwrap();
    let job = ChildJob {
        parent: oc_core::domain::SessionId("parent".into()),
        child: oc_core::domain::SessionId("child".into()),
        operation: "launch".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: description.into(),
        delivery_id: "child-delivery:launch".into(),
        state: ChildState::Admitted,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    };
    db.admit_child_job(&job).unwrap();
    job
}

#[test]
fn bounded_child_cards_do_not_truncate_execution_or_recovery_identity() {
    for bytes in [9 * 1024, 40 * 1024] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let description = "x".repeat(bytes);
        let input = serde_json::json!({"background":true,"description":description}).to_string();
        let accepted = admitted_description(&db, &input, &description);
        assert_eq!(db.child_jobs("parent").unwrap(), vec![accepted.clone()]);
        assert_eq!(
            db.child_job("parent", &accepted.operation).unwrap(),
            Some(accepted.clone())
        );
        assert_eq!(
            db.child_recovery_candidates().unwrap(),
            vec![accepted.clone()]
        );
        let cards = db.child_tool_jobs("parent").unwrap();
        if bytes < oc_core::tool_output::RECORD_BYTES {
            assert_eq!(cards.len(), 1);
            assert!(cards[0].description.len() <= TOOL_OP_PREVIEW_BYTES);
            assert!(cards[0].description.ends_with('…'));
        } else {
            assert!(
                cards.is_empty(),
                "oversized display identity is honestly unavailable"
            );
        }
        db.finish_child_job(
            &accepted.operation,
            ChildState::Completed,
            "actual successful result",
        )
        .unwrap();
        let actual = db.child_jobs("parent").unwrap().pop().unwrap();
        assert_eq!(actual.description, description);
        assert_eq!(
            db.child_job("parent", &accepted.operation)
                .unwrap()
                .unwrap(),
            actual
        );
        assert_eq!(actual.state, ChildState::Completed);
        assert_eq!(actual.result.as_deref(), Some("actual successful result"));
        assert!(db.child_recovery_candidates().unwrap().is_empty());
        assert_eq!(
            db.list_tool_ops("parent").unwrap()[0].input.as_deref(),
            Some(input.as_str())
        );
    }
}

#[test]
fn child_history_projects_exact_task_and_delivery_without_changing_raw_or_replaying() {
    use oc_core::{
        queries::{ChildHistory, ModelRef},
        session::MessageId,
    };
    for task in [
        "literal [parent_context_pack] Ω界\nactual task".into(),
        "Ω界".repeat(32768),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        admitted_input(
            &db,
            &serde_json::json!({"agent":"helper", "prompt":task, "background":true}).to_string(),
        );
        let transformed =
            format!("native host fields\n[parent_context_pack]\nquoted parent data\n{task}");
        let accepted = db
            .accept_turn(
                "child-turn",
                "child",
                &transformed,
                &transformed,
                &ModelRef {
                    provider: "fixture".into(),
                    id: "child".into(),
                    variant: None,
                },
            )
            .unwrap();
        db.start_child_job("launch", "child-turn").unwrap();
        let id = MessageId(accepted.user_message);
        let expected = ChildHistory::Task {
            text: task[..task.floor_char_boundary(task.len().min(65536))].into(),
            limited: task.len() > 65536,
        };
        assert_eq!(
            db.child_history("child", &id).unwrap(),
            Some(expected.clone())
        );
        assert_eq!(db.child_history("parent", &id).unwrap(), None);
        let raw = db.read_history_full("child").unwrap();
        assert!(raw.iter().any(|(_, _, text)| text == &transformed));
        db.finish_child_job(
            "launch",
            ChildState::Completed,
            "error: successful literal prose",
        )
        .unwrap();
        let notice = db.deliver_child_notices().unwrap().remove(0);
        let notice_id = MessageId(notice.job.message_id.clone().unwrap());
        let projected = db.child_history("parent", &notice_id).unwrap().unwrap();
        let ChildHistory::Notice(job) = &projected else {
            panic!("typed native notice");
        };
        assert_eq!(job.state, ChildState::Completed);
        assert_eq!(job.child.0, "child");
        assert_eq!(job.message_id.as_deref(), Some(notice_id.0.as_str()));
        assert!(job.result.is_none());
        let spoof = db.append_message("parent", "user", &notice.text).unwrap();
        assert_eq!(db.child_history("parent", &MessageId(spoof)).unwrap(), None);
        assert_eq!(db.child_history("child", &notice_id).unwrap(), None);
        let original = db
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT identity FROM child_jobs WHERE operation_id='launch'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        let mut corrupt: serde_json::Value = serde_json::from_str(&original).unwrap();
        corrupt["parent"] = "foreign".into();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
                [corrupt.to_string()],
            )
            .unwrap();
        assert_eq!(db.child_history("parent", &notice_id).unwrap(), None);
        assert_eq!(db.child_history("child", &id).unwrap(), None);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE child_jobs SET identity=?1 WHERE operation_id='launch'",
                [original],
            )
            .unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET kind='not-a-notice' WHERE kind='subagent_notice'",
                [],
            )
            .unwrap();
        assert_eq!(db.child_history("parent", &notice_id).unwrap(), None);
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE events SET kind='subagent_notice' WHERE kind='not-a-notice'",
                [],
            )
            .unwrap();
        let parent_raw = db.read_history_full("parent").unwrap();
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.child_history("child", &id).unwrap(), Some(expected));
        assert_eq!(
            db.child_history("parent", &notice_id).unwrap(),
            Some(projected)
        );
        assert_eq!(db.read_history_full("child").unwrap(), raw);
        assert_eq!(db.read_history_full("parent").unwrap(), parent_raw);
        assert!(db.deliver_child_notices().unwrap().is_empty());
    }
}

#[test]
fn provider_only_interruption_is_left_for_owned_resume_without_new_input() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    admitted(&db);
    db.begin_turn("child-turn", "child", "own task").unwrap();
    db.start_child_job("launch", "child-turn").unwrap();
    let log = crate::tools::TurnLog::new("child-turn", "child", "fixture");
    db.checkpoint_turn("child-turn", &log.to_json().to_string())
        .unwrap();
    db.recover_child_jobs().unwrap();
    assert_eq!(
        db.child_jobs("parent").unwrap()[0].state,
        ChildState::Running
    );
    assert!(db.deliver_child_notices().unwrap().is_empty());
}

fn resumable(db: &Db) -> (ChildJob, String) {
    let mut job = admitted(db);
    db.set_pref("tui.session_location.child", "/original")
        .unwrap();
    let accepted = db
        .accept_turn(
            "child-turn",
            "child",
            "own task",
            "own task",
            &oc_core::queries::ModelRef {
                provider: "fixture".into(),
                id: "child".into(),
                variant: None,
            },
        )
        .unwrap();
    db.start_child_job("launch", "child-turn").unwrap();
    let mut log = crate::tools::TurnLog::new("child-turn", "child", "fixture");
    log.user_message = Some(accepted.user_message);
    log.input.push(crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        "own task",
    ));
    let raw = log.to_json().to_string();
    db.checkpoint_turn("child-turn", &raw).unwrap();
    job.turn = Some("child-turn".into());
    job.state = ChildState::Running;
    (job, raw)
}

#[test]
fn recovery_claim_crash_boundaries_are_atomic_bounded_and_not_task_admission() {
    for after_dispatch in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let (job, raw) = resumable(&db);
        if after_dispatch {
            db.generation_dispatch("child", "child-turn", "main")
                .unwrap();
        }
        let count = db.read_history_page("child", 100, None).unwrap().len();
        // Failed COMMIT cannot spend an attempt or alter task/turn state.
        db.conn.lock().unwrap().execute_batch("CREATE TABLE claim_fault(value TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER reject_claim AFTER INSERT ON events WHEN NEW.kind='subagent_resume_claim' BEGIN INSERT INTO claim_fault VALUES('missing'); END").unwrap();
        assert!(db.claim_child_resume(&job, &raw).is_err());
        assert_eq!(
            db.conn
                .lock()
                .unwrap()
                .query_row(
                    "SELECT count(*) FROM events WHERE kind='subagent_resume_claim'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
        db.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_claim; DROP TABLE claim_fault")
            .unwrap();
        db.claim_child_resume(&job, &raw).unwrap();
        // Death after claim/before provider: next boot counts a second attempt,
        // retaining the exact same task. A successful step is never this budget.
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        db.recover_child_jobs().unwrap();
        db.recover_interrupted_tools().unwrap();
        assert_eq!(db.turn_result("child-turn").unwrap().0, "started");
        for _ in 1..10 {
            db.claim_child_resume(&job, &raw).unwrap();
        }
        assert!(db.claim_child_resume(&job, &raw).is_err());
        assert_eq!(
            db.read_history_page("child", 100, None).unwrap().len(),
            count
        );
        assert_eq!(
            db.turn_result("child-turn").unwrap().1.as_deref(),
            Some(raw.as_str())
        );
        db.finish_child_job("launch", ChildState::Unknown, "explicit recovery required")
            .unwrap();
        db.recover_child_jobs().unwrap();
        assert!(db.claim_child_resume(&job, &raw).is_err());
        assert_eq!(db.deliver_child_notices().unwrap().len(), 1);
        assert!(db.deliver_child_notices().unwrap().is_empty());
    }
}

#[test]
fn typed_effect_uncertainty_and_unanswered_calls_never_claim() {
    for name in [
        "apply_patch",
        "write",
        "edit",
        "shell",
        "bash",
        "mcp_fixture_effect",
    ] {
        for state in ["started", "unknown", "completed"] {
            let dir = tempfile::tempdir().unwrap();
            let db = Db::open(dir.path()).unwrap();
            let (job, raw) = resumable(&db);
            db.record_tool_intent("effect", "child", Some("child-turn"), name, "{}")
                .unwrap();
            if state != "started" {
                db.record_tool_outcome("effect", state, Some("exact captured effect output"))
                    .unwrap();
            }
            db.recover_child_jobs().unwrap();
            assert_eq!(
                db.child_jobs("parent").unwrap()[0].state,
                ChildState::Unknown
            );
            assert!(db.claim_child_resume(&job, &raw).is_err());
            assert_eq!(db.list_tool_ops("child").unwrap().len(), 1);
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let (job, raw) = resumable(&db);
    let mut log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
    log.input.push(crate::provider::InputItem::ProviderOutput(serde_json::json!({"type":"function_call","name":"read","call_id":"unanswered","arguments":"{}"})));
    let raw = log.to_json().to_string();
    db.checkpoint_turn("child-turn", &raw).unwrap();
    db.recover_child_jobs().unwrap();
    assert!(db.claim_child_resume(&job, &raw).is_err());
    assert_eq!(
        db.child_jobs("parent").unwrap()[0].state,
        ChildState::Unknown
    );
}

#[test]
fn completed_task_renewal_is_resumable_but_other_compress_outcomes_are_not() {
    for (output, state, resumable_expected) in [
        (
            r#"{"status":"task_renewal_accepted","appliesFrom":"next request"}"#,
            "completed",
            true,
        ),
        (
            r#"{"status":"compressed","blocks":["b0001"]}"#,
            "completed",
            false,
        ),
        (r#"{"status":"task_renewal_accepted"}"#, "started", false),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        let (job, raw) = resumable(&db);
        db.record_tool_intent("renew", "child", Some("child-turn"), "compress", "{}")
            .unwrap();
        if state != "started" {
            db.record_tool_outcome("renew", state, Some(output))
                .unwrap();
        }
        db.recover_child_jobs().unwrap();
        let expected = if resumable_expected {
            ChildState::Running
        } else {
            ChildState::Unknown
        };
        assert_eq!(
            db.child_jobs("parent").unwrap()[0].state,
            expected,
            "{output} {state}"
        );
        assert_eq!(
            db.claim_child_resume(&job, &raw).is_ok(),
            resumable_expected,
            "{output}"
        );
    }
}

#[test]
fn claim_refuses_stale_source_and_preserves_latest_hot_not_raw() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let (job, raw) = resumable(&db);
    let mut log = crate::tools::TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
    log.input[0] = crate::provider::InputItem::message(
        crate::provider::InputRole::User,
        "latest selected HOT",
    );
    let hot = log.to_json().to_string();
    db.checkpoint_turn("child-turn", &hot).unwrap();
    assert!(db.claim_child_resume(&job, &raw).is_err());
    for field in 0..5 {
        let mut stale = job.clone();
        match field {
            0 => stale.parent.0.push('x'),
            1 => stale.child.0.push('x'),
            2 => stale.delivery_id.push('x'),
            3 => stale.model.push('x'),
            _ => stale.location.push('x'),
        }
        assert!(db.claim_child_resume(&stale, &hot).is_err());
    }
    db.claim_child_resume(&job, &hot).unwrap();
    assert_eq!(
        db.turn_result("child-turn").unwrap().1.as_deref(),
        Some(hot.as_str())
    );
    assert_eq!(
        db.read_history_page("child", 100, None).unwrap()[0].2,
        "own task"
    );
}

fn fault(db: &Db, boundary: &str, commit: bool) {
    let conn = db.conn.lock().unwrap();
    if commit {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS child_fault(value TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED)").unwrap();
        conn.execute_batch(&format!("CREATE TRIGGER reject_child AFTER UPDATE OF {boundary} ON child_jobs BEGIN INSERT INTO child_fault VALUES('missing-session'); END")).unwrap();
    } else {
        conn.execute_batch(&format!("CREATE TRIGGER reject_child BEFORE UPDATE OF {boundary} ON child_jobs BEGIN SELECT RAISE(FAIL,'child boundary fault'); END")).unwrap();
    }
}

fn remove_fault(db: &Db) {
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_child")
        .unwrap();
}

#[test]
fn native_versioned_child_schema_upgrades_existing_facts_and_rolls_back_failed_marker() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    admitted(&db);
    let facts = db.child_jobs("parent").unwrap();
    let version: i64 = db.conn.lock().unwrap().query_row(
        "SELECT count(*) FROM schema_migrations WHERE version=10 AND applied_at='t45-child-jobs'", [], |row| row.get(0)).unwrap();
    assert_eq!(
        version, 1,
        "child job schema has no native migration identity"
    );
    db.conn
        .lock()
        .unwrap()
        .execute("DELETE FROM schema_migrations WHERE version=10", [])
        .unwrap();
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(db.child_jobs("parent").unwrap(), facts);
    let conn = db.conn.lock().unwrap();
    conn.execute("DELETE FROM schema_migrations WHERE version=10", [])
        .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_child_version BEFORE INSERT ON schema_migrations WHEN NEW.version=10 BEGIN SELECT RAISE(FAIL,'owned migration fault'); END").unwrap();
    assert!(Db::child_jobs_schema(&conn).is_err());
    assert!(
        conn.is_autocommit(),
        "failed migration left transaction open"
    );
    let versions: i64 = conn
        .query_row(
            "SELECT count(*) FROM schema_migrations WHERE version=10",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(versions, 0);
    conn.execute_batch("DROP TRIGGER reject_child_version")
        .unwrap();
    Db::child_jobs_schema(&conn).unwrap();
    Db::child_jobs_schema(&conn).unwrap();
    drop(conn);
    assert_eq!(db.child_jobs("parent").unwrap(), facts);
}

#[test]
fn fresh_child_admission_commit_fault_rolls_back_session_location_event_and_job() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    let original = admitted(&db);
    let mut fresh = original.clone();
    fresh.child.0 = "rejected-child".into();
    fresh.operation = "rejected-launch".into();
    fresh.delivery_id = "rejected-notice".into();
    db.record_tool_intent(&fresh.operation, "parent", None, "subagent", "{}")
        .unwrap();
    let sessions = db.list_sessions().unwrap();
    let conn = db.conn.lock().unwrap();
    conn.execute_batch("CREATE TABLE fresh_fault(value TEXT REFERENCES sessions(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER reject_fresh AFTER INSERT ON child_jobs BEGIN INSERT INTO fresh_fault VALUES('missing-session'); END").unwrap();
    drop(conn);
    assert!(db.admit_fresh_child_job(&fresh).is_err());
    assert_eq!(
        db.list_sessions().unwrap(),
        sessions,
        "failed COMMIT left a fresh child"
    );
    assert!(
        db.get_pref(&format!(
            "{}{}",
            crate::runtime::SESSION_LOCATION_PREFIX,
            fresh.child.0
        ))
        .unwrap()
        .is_none()
    );
    let events: i64 = db.conn.lock().unwrap().query_row("SELECT count(*) FROM events WHERE session_id=?1 OR (kind='subagent_launch' AND json_extract(payload,'$.childID')=?1)", [&fresh.child.0], |row| row.get(0)).unwrap();
    assert_eq!(events, 0);
    assert_eq!(db.child_jobs("parent").unwrap(), [original]);
}

#[test]
fn terminal_and_delivery_statement_and_commit_faults_rollback_then_reopen_dedups() {
    for commit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        admitted(&db);
        fault(&db, "state", commit);
        assert!(
            db.finish_child_job("launch", ChildState::Completed, "error: successful prose")
                .is_err()
        );
        assert_eq!(
            db.child_jobs("parent").unwrap()[0].state,
            ChildState::Admitted
        );
        remove_fault(&db);
        db.finish_child_job("launch", ChildState::Completed, "error: successful prose")
            .unwrap();
        // A lost acknowledgement AFTER terminal COMMIT preserves the result.
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        db.recover_child_jobs().unwrap();
        fault(&db, "message_id", commit);
        assert!(db.deliver_child_notices().is_err());
        assert!(db.child_jobs("parent").unwrap()[0].message_id.is_none());
        assert!(
            db.read_history_page("parent", 100, None)
                .unwrap()
                .is_empty()
        );
        remove_fault(&db);
        let notices = db.deliver_child_notices().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].job.state, ChildState::Completed);
        assert!(notices[0].text.contains("error: successful prose"));
        assert_eq!(
            db.list_tool_ops("parent").unwrap()[0].output.as_deref(),
            Some(LAUNCH_OUTPUT)
        );
        // A lost delivery acknowledgement AFTER COMMIT cannot redeliver.
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        db.recover_child_jobs().unwrap();
        assert!(db.deliver_child_notices().unwrap().is_empty());
        assert_eq!(db.read_history_page("parent", 100, None).unwrap().len(), 1);
    }
}

#[test]
fn missing_terminal_receipt_recovers_committed_assistant_but_unfinished_effect_is_unknown() {
    for completed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        admitted(&db);
        db.begin_turn("child-turn", "child", "own task").unwrap();
        db.start_child_job("launch", "child-turn").unwrap();
        if completed {
            let log = crate::tools::TurnLog::new("child-turn", "child", "fixture");
            db.commit_turn(
                "child-turn",
                "completed",
                Some(&log.to_json().to_string()),
                Some("exact committed child output"),
            )
            .unwrap();
        } else {
            db.record_tool_intent(
                "unknown-effect",
                "child",
                Some("child-turn"),
                "bash",
                "effect",
            )
            .unwrap();
        }
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        db.recover_child_jobs().unwrap();
        let job = db.child_jobs("parent").unwrap().remove(0);
        assert_eq!(
            job.state,
            if completed {
                ChildState::Completed
            } else {
                ChildState::Unknown
            }
        );
        if completed {
            assert_eq!(job.result.as_deref(), Some("exact committed child output"));
        } else {
            assert!(job.result.unwrap().contains("explicit recovery required"));
        }
        assert_eq!(db.deliver_child_notices().unwrap().len(), 1);
        assert!(db.deliver_child_notices().unwrap().is_empty());
        assert_eq!(
            db.list_tool_ops("child").unwrap().len(),
            usize::from(!completed)
        );
        assert_eq!(db.list_tool_ops("parent").unwrap()[0].state, "running");
    }
}

#[test]
fn recursive_fork_preserves_launch_and_notice_without_child_authority_or_foreign_origin() {
    use crate::provider::{InputItem, InputRole};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    admitted(&db);
    Db::insert_location_binding(&db.conn.lock().unwrap(), "parent", "/original").unwrap();
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "child".into(),
        variant: None,
    };
    let accepted = db
        .accept_turn("parent-turn", "parent", "delegate", "delegate", &model)
        .unwrap();
    let output = LAUNCH_OUTPUT.to_owned();
    let mut log = crate::tools::TurnLog::new("parent-turn", "child", "fixture");
    log.user_message = Some(accepted.user_message);
    log.input = vec![
        InputItem::message(InputRole::User, "delegate"),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","call_id":"bg","name":"subagent","arguments":"{}"}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "bg".into(),
            output: output.clone(),
        },
    ];
    // Associate the already committed immutable launch with its accepted turn.
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE tool_operations SET turn_id='parent-turn' WHERE id='launch'",
            [],
        )
        .unwrap();
    db.finish_child_job("launch", ChildState::Completed, "bounded result")
        .unwrap();
    let notice = db.deliver_child_notices().unwrap().remove(0);
    log.shell_notice_messages
        .push(notice.job.message_id.unwrap());
    log.input
        .push(InputItem::message(InputRole::User, &notice.text));
    log.input
        .push(InputItem::message(InputRole::Assistant, "answer"));
    log.display_parts = vec![
        serde_json::json!({"tool":"launch"}),
        serde_json::json!({"message":4}),
    ];
    db.commit_turn(
        "parent-turn",
        "completed",
        Some(&log.to_json().to_string()),
        Some("answer"),
    )
    .unwrap();
    let boundary = db.append_message("parent", "user", "boundary").unwrap();
    let projection = db
        .turn_presentation("parent", "parent-turn")
        .unwrap()
        .unwrap();
    let oc_core::queries::TranscriptPart::Tool(tool) = &projection.parts[0] else {
        panic!("real recorded tool")
    };
    assert_eq!(
        tool.child_job.as_deref(),
        db.child_tool_job("parent", "launch").unwrap().as_ref()
    );
    assert_eq!(tool.state, "running");
    assert_eq!(tool.output.as_deref(), Some(LAUNCH_OUTPUT));
    let raw = db.read_history_full("parent").unwrap();
    let fork = db
        .fork_session("parent", &boundary, "/original", "fixture", "{}")
        .unwrap();
    assert!(db.child_jobs(&fork.session.0).unwrap().is_empty());
    let copied_operation = db.list_tool_ops(&fork.session.0).unwrap()[0].op.clone();
    assert!(
        db.child_tool_job(&fork.session.0, &copied_operation)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.list_tool_ops(&fork.session.0).unwrap()[0].state,
        "running"
    );
    let copied = db.read_history_full(&fork.session.0).unwrap();
    assert_eq!(
        copied
            .iter()
            .filter(|(_, _, text)| text.contains("Automatic background subagent result"))
            .count(),
        1
    );
    let nested_boundary = db
        .append_message(&fork.session.0, "user", "nested boundary")
        .unwrap();
    let nested = db
        .fork_session(
            &fork.session.0,
            &nested_boundary,
            "/original",
            "fixture",
            "{}",
        )
        .unwrap();
    assert!(db.child_jobs(&nested.session.0).unwrap().is_empty());
    let copied_operation = db.list_tool_ops(&nested.session.0).unwrap()[0].op.clone();
    assert!(
        db.child_tool_job(&nested.session.0, &copied_operation)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.list_tool_ops(&nested.session.0).unwrap()[0].state,
        "running"
    );
    db.change_conversation(
        &nested.session.0,
        oc_core::queries::ConversationAction::Revert {
            message: oc_core::session::MessageId(
                db.read_history_full(&nested.session.0).unwrap()[0]
                    .0
                    .clone(),
            ),
        },
    )
    .unwrap();
    assert!(
        db.conversation_history_full(&nested.session.0)
            .unwrap()
            .is_empty()
    );
    assert_eq!(db.read_history_full("parent").unwrap(), raw);
    db.conn.lock().unwrap().execute("UPDATE events SET payload=json_set(payload,'$.location','foreign') WHERE session_id=?1 AND kind='subagent_launch'",[&fork.session.0]).unwrap();
    assert!(
        db.fork_session(
            &fork.session.0,
            &nested_boundary,
            "/original",
            "fixture",
            "{}"
        )
        .is_err()
    );
    assert_eq!(db.read_history_full("parent").unwrap(), raw);
}
