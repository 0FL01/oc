//! Accumulated child jobs and parent events must not make hot child-job
//! operations more expensive. Work is counted in SQLite VM steps, which is
//! deterministic for the same data, so these checks do not depend on timing.
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

thread_local! {
    static STARTS: RefCell<HashMap<String, i32>> = RefCell::new(HashMap::new());
    static STEPS: Cell<i64> = const { Cell::new(0) };
}

fn count(event: rusqlite::trace::TraceEvent<'_>) {
    use rusqlite::{StatementStatus::VmStep, trace::TraceEvent};
    match event {
        // Trigger subprograms report the same statement again; keep its start.
        TraceEvent::Stmt(stmt, _) => STARTS.with_borrow_mut(|starts| {
            starts
                .entry(stmt.sql().into_owned())
                .or_insert_with(|| stmt.get_status(VmStep));
        }),
        TraceEvent::Profile(stmt, _) => {
            if let Some(start) = STARTS.with_borrow_mut(|starts| starts.remove(stmt.sql().as_ref()))
            {
                STEPS.set(STEPS.get() + i64::from(stmt.get_status(VmStep) - start));
            }
        }
        _ => {}
    }
}

fn steps<T>(operation: impl FnOnce() -> T) -> (T, i64) {
    STARTS.with_borrow_mut(HashMap::clear);
    STEPS.set(0);
    let result = operation();
    (result, STEPS.get())
}

struct Population {
    _dir: tempfile::TempDir,
    db: Db,
    notice: oc_core::session::MessageId,
}

fn job(i: usize, background: bool) -> ChildJob {
    ChildJob {
        parent: oc_core::domain::SessionId("parent".into()),
        child: oc_core::domain::SessionId(format!("child-{i}")),
        operation: format!("launch-{i}"),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: "work".into(),
        delivery_id: format!("child-delivery:launch-{i}"),
        state: ChildState::Admitted,
        background,
        turn: None,
        result: None,
        message_id: None,
    }
}

fn admit(db: &Db, i: usize, background: bool) -> ChildJob {
    let job = job(i, background);
    db.create_child_session(
        "parent",
        &job.child.0,
        Some("helper"),
        Some("fixture/child"),
        Some("work"),
    )
    .unwrap();
    let input = serde_json::json!({"agent":"helper","prompt":"task","background":background});
    db.record_tool_intent(
        &job.operation,
        "parent",
        None,
        "subagent",
        &input.to_string(),
    )
    .unwrap();
    let output = serde_json::json!({"status":"running","sessionID":job.child.0,"jobGeneration":job.operation,"deliveryID":job.delivery_id});
    db.record_tool_outcome(&job.operation, "running", Some(&output.to_string()))
        .unwrap();
    db.admit_child_job(&job).unwrap();
    job
}

/// Real launch/terminal/delivery paths for `jobs` children, half of them
/// foreground, plus `20 * jobs` unrelated parent events, then one admitted job.
fn population(jobs: usize) -> Population {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    // Population speed only: the measured statements do not depend on fsync.
    db.conn
        .lock()
        .unwrap()
        .pragma_update(None, "synchronous", "OFF")
        .unwrap();
    db.create_session("parent").unwrap();
    let mut notice = None;
    for i in 0..jobs {
        let background = i % 2 == 1;
        let job = admit(&db, i, background);
        db.finish_child_job(&job.operation, ChildState::Completed, "done")
            .unwrap();
        if background {
            for delivered in db.deliver_child_notices().unwrap() {
                notice = delivered.job.message_id;
            }
        }
    }
    db.conn
        .lock()
        .unwrap()
        .execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<?1)
             INSERT INTO events(session_id,kind,payload)
             SELECT 'parent',CASE i%4 WHEN 0 THEN 'tool_output_presentation' WHEN 1 THEN 'generation_dispatched' WHEN 2 THEN 'shell_terminal' ELSE 'message' END,'{\"operation\":\"x'||i||'\"}' FROM n",
            [jobs as i64 * 20],
        )
        .unwrap();
    admit(&db, jobs, false);
    db.conn.lock().unwrap().trace_v2(
        rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT
            | rusqlite::trace::TraceEventCodes::SQLITE_TRACE_PROFILE,
        Some(count),
    );
    Population {
        _dir: dir,
        db,
        notice: oc_core::session::MessageId(notice.expect("delivered notice")),
    }
}

/// VM steps of each hot operation, checked for the expected result.
fn measure(population: &Population, jobs: usize) -> Vec<(&'static str, i64)> {
    let db = &population.db;
    let (outstanding, outstanding_steps) = steps(|| db.child_job_outstanding().unwrap());
    assert_eq!(outstanding, 1, "only the final admitted job is outstanding");
    let (delivered, deliver_steps) = steps(|| db.deliver_child_notices().unwrap());
    assert!(delivered.is_empty());
    let (cards, cards_steps) = steps(|| db.child_tool_jobs("parent").unwrap());
    assert_eq!(cards.len(), 16);
    assert_eq!(
        cards[0].operation,
        format!("launch-{jobs}"),
        "active job first"
    );
    let (history, history_steps) =
        steps(|| db.child_history("parent", &population.notice).unwrap());
    assert!(matches!(history, Some(ChildHistory::Notice(_))));
    let (converted, convert_steps) =
        steps(|| db.background_child_job(&format!("launch-{jobs}")).unwrap());
    assert!(converted);
    vec![
        ("child_job_outstanding", outstanding_steps),
        ("deliver_child_notices", deliver_steps),
        ("child_tool_jobs", cards_steps),
        ("child_history notice", history_steps),
        ("background_child_job", convert_steps),
    ]
}

#[test]
fn hot_child_job_operations_do_not_grow_with_accumulated_jobs_and_events() {
    let (small, large) = (150, 600);
    let before = measure(&population(small), small);
    let after = measure(&population(large), large);
    for ((name, small_steps), (_, large_steps)) in before.iter().zip(&after) {
        eprintln!("{name}: {small} jobs {small_steps} steps, {large} jobs {large_steps} steps");
    }
    for ((name, small_steps), (_, large_steps)) in before.into_iter().zip(after) {
        // Four times the history may add bounded index depth, not proportional work.
        assert!(
            large_steps <= small_steps + small_steps / 2 + 500,
            "{name}: {small} jobs took {small_steps} VM steps, {large} jobs took {large_steps}"
        );
    }
}

#[test]
fn delivery_mode_migration_backfills_launch_mode_and_conversion_once() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("parent").unwrap();
    // Settled foreground, settled background awaiting notice, a foreground
    // launch converted while running, and a still running foreground launch.
    for (i, background) in [(0, false), (1, true), (2, false), (3, false)] {
        admit(&db, i, background);
    }
    assert!(db.background_child_job("launch-2").unwrap());
    for i in 0..3 {
        db.finish_child_job(&format!("launch-{i}"), ChildState::Completed, "done")
            .unwrap();
    }
    let modes = |db: &Db| -> Vec<(String, i64)> {
        let conn = db.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT operation_id,background FROM child_jobs ORDER BY operation_id")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    let expected = vec![
        ("launch-0".into(), 0),
        ("launch-1".into(), 1),
        ("launch-2".into(), 1),
        ("launch-3".into(), 0),
    ];
    assert_eq!(modes(&db), expected);
    // Pre-migration shape: no typed mode, no delivery indexes, no marker.
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "DROP INDEX child_job_undelivered; DROP INDEX child_job_state;
             DROP INDEX events_subagent_background; DROP INDEX events_subagent_notice; DROP INDEX events_subagent_launch;
             ALTER TABLE child_jobs DROP COLUMN background;
             DELETE FROM schema_migrations WHERE version=16;",
        )
        .unwrap();
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(
        modes(&db),
        expected,
        "backfill uses launch mode or conversion"
    );
    assert_eq!(
        db.child_job_outstanding().unwrap(),
        3,
        "one running, two undelivered"
    );
    let delivered: Vec<_> = db
        .deliver_child_notices()
        .unwrap()
        .into_iter()
        .map(|notice| notice.job.operation)
        .collect();
    assert_eq!(delivered, ["launch-1", "launch-2"]);
    assert_eq!(db.child_job_outstanding().unwrap(), 1);
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    let markers: i64 = db
        .conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM schema_migrations WHERE version=16",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(markers, 1, "reopen does not migrate again");
}
