//! Background execution must overlap an active parent's physical request.
use super::foreground_children::{Gate, Peer, outputs, received, wait_for};
use super::*;

#[tokio::test]
async fn background_real_profile_ask_rechecks_other_owned_child_effect() {
    super::foreground_children::shared_child_preimage(true).await;
}
use oc_core::core_app::CoreEvent;
use tempfile::TempDir;

async fn app_fixture(
    peer: &Peer,
    permissions: serde_json::Value,
) -> (
    TempDir,
    oc_core::core_app::CoreApp,
    oc_core::core_app::WorkerGuard,
) {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("source");
    std::fs::create_dir(&project).unwrap();
    let config = serde_json::json!({
        "model":"fixture/m", "compaction":{"auto":false},"permission":permissions,
        "agent":{"title":{"disable":true},"helper":{"mode":"subagent","model":"fixture/agent-model","system":"OWN_BACKGROUND_PROFILE"}},
        "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":peer.base,"apiKey":"synthetic"},"models":{"m":{"limit":{"context":500000,"output":4096}},"agent-model":{"limit":{"context":500000,"output":4096}}}}}
    });
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("SHELL".into(), "/bin/bash".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &root.path().join("data"), env)
            .await
            .unwrap();
    app.create_session(oc_core::domain::SessionId("parent".into()))
        .await
        .unwrap();
    (root, app, guard)
}

async fn jobs_when(
    app: &oc_core::core_app::CoreApp,
    condition: impl Fn(&[oc_core::queries::ChildJob]) -> bool,
) -> Vec<oc_core::queries::ChildJob> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let jobs = app
                .child_jobs(oc_core::domain::SessionId("parent".into()))
                .await
                .unwrap();
            if condition(&jobs) {
                return jobs;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}

async fn parent_finished(
    events: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
    selected: &oc_core::core_app::WorkerTurnId,
) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events.recv().await.unwrap();
            if matches!(event,CoreEvent::TurnFinished { turn,.. } if &turn == selected) {
                break;
            }
        }
    })
    .await
    .expect("parent must finish independently within fixture watchdog");
}

#[tokio::test]
async fn exact_historical_child_read_survives_seventeen_new_generations_without_replay() {
    use oc_core::{domain::SessionId, queries::ChildState};
    let parent_steps = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let child_steps = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let roots = parent_steps.clone();
    let children = child_steps.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let peer = Peer::start(move |request| {
        if request["max_output_tokens"] == 256
            && request["tools"].as_array().is_none_or(Vec::is_empty)
        {
            return sse_delta("Synthetic title") + &sse_completed();
        }
        captured.lock().unwrap().push(request.clone());
        if request["model"] == "m" {
            let step = roots.fetch_add(1, Ordering::SeqCst);
            if step.is_multiple_of(2) {
                subagent_call(
                    &format!("historical-{}", step / 2),
                    serde_json::json!({"agent":"helper","description":format!("generation {}",step/2),"prompt":"OWN_HISTORICAL_CHILD_TASK"}),
                ) + &sse_completed()
            } else {
                sse_delta("Settled parent generation") + &sse_completed()
            }
        } else {
            assert_eq!(request["model"], "agent-model");
            let step = children.fetch_add(1, Ordering::SeqCst);
            sse_delta(&format!("actual child result {step}")) + &sse_completed()
        }
    });
    let (_root, app, guard) =
        app_fixture(&peer, serde_json::json!({"*":"deny","subagent":"allow"})).await;
    let parent = SessionId("parent".into());
    let mut events = app.subscribe();
    let mut oldest = None;
    for index in 0..17 {
        let turn = app
            .submit(parent.clone(), format!("generation {index}"))
            .await
            .unwrap();
        parent_finished(&mut events, &turn).await;
        let jobs = app.child_jobs(parent.clone()).await.unwrap();
        if jobs.is_empty() {
            let operations = app.tool_ops_page(parent.clone(), None, 1).await.unwrap();
            panic!("real admission must produce an owned job: {operations:?}");
        }
        assert!(jobs.iter().all(|job| job.state == ChildState::Completed));
        if index == 0 {
            oldest = Some(jobs[0].clone());
        }
    }
    let oldest = oldest.unwrap();
    let jobs = app.child_jobs(parent.clone()).await.unwrap();
    assert_eq!(jobs.len(), 16);
    assert!(jobs.iter().all(|job| job.operation != oldest.operation));
    let parent_before = app
        .history_page(parent.clone(), None, None, 100)
        .await
        .unwrap();
    let page = app
        .read_child(parent.clone(), oldest.clone())
        .await
        .unwrap();
    assert_eq!(page.parent_id.as_deref(), Some("parent"));
    let mut owner_facts = oldest.clone();
    owner_facts.result = None;
    assert_eq!(page.child_job.as_deref(), Some(&owner_facts));
    let mut stale_running = oldest.clone();
    stale_running.state = ChildState::Running;
    stale_running.message_id = None;
    assert_eq!(
        app.read_child(parent.clone(), stale_running).await.unwrap(),
        page,
        "read owner supplies current original-launch facts, not the old capture's phase"
    );
    assert!(
        page.rows
            .iter()
            .any(|row| row.text == "actual child result 0")
    );
    for field in ["generation", "location", "delivery", "child", "operation"] {
        let mut foreign = oldest.clone();
        match field {
            "generation" => foreign.generation += 1,
            "location" => foreign.location.push_str("/foreign"),
            "delivery" => foreign.delivery_id = "foreign-delivery".into(),
            "child" => foreign.child = jobs[0].child.clone(),
            _ => foreign.operation = "foreign-operation".into(),
        }
        assert!(
            app.read_child(parent.clone(), foreign).await.is_err(),
            "{field} fence"
        );
    }
    assert_eq!(app.read_child(parent.clone(), oldest).await.unwrap(), page);
    assert_eq!(
        app.history_page(parent, None, None, 100).await.unwrap(),
        parent_before,
        "read-only navigation cannot rewrite RAW, projection or revision"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(parent_steps.load(Ordering::SeqCst), 34);
    assert_eq!(child_steps.load(Ordering::SeqCst), 17);
    assert_eq!(
        requests.lock().unwrap().len(),
        51,
        "reads cannot generate work or replay effects"
    );
}

#[tokio::test]
async fn foreground_child_result_survives_an_identity_too_large_for_ui_projection() {
    let roots = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let children = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let root_calls = roots.clone();
    let child_calls = children.clone();
    let description = "x".repeat(40 * 1024);
    let recorded_description = description.clone();
    let peer = Peer::start(move |request| {
        if request["max_output_tokens"] == 256
            && request["tools"].as_array().is_none_or(Vec::is_empty)
        {
            return sse_delta("Synthetic title") + &sse_completed();
        }
        if request["model"] == "m" {
            let step = root_calls.fetch_add(1, Ordering::SeqCst);
            if step == 0 {
                subagent_call(
                    "large-description",
                    serde_json::json!({"agent":"helper","description":description,"prompt":"OWN_LARGE_DESCRIPTION_TASK"}),
                ) + &sse_completed()
            } else {
                assert_eq!(step, 1);
                let results = outputs(&request);
                assert_eq!(results.len(), 1);
                assert!(
                    results[0].1.contains("ACTUAL_SUCCESSFUL_CHILD"),
                    "actual foreground completion: {results:?}"
                );
                assert!(!results[0].1.contains("child state unavailable"));
                sse_delta("Parent received actual child completion") + &sse_completed()
            }
        } else {
            assert_eq!(request["model"], "agent-model");
            assert_eq!(child_calls.fetch_add(1, Ordering::SeqCst), 0);
            sse_delta("ACTUAL_SUCCESSFUL_CHILD") + &sse_completed()
        }
    });
    let (root, app, guard) =
        app_fixture(&peer, serde_json::json!({"*":"deny","subagent":"allow"})).await;
    let parent = oc_core::domain::SessionId("parent".into());
    let mut events = app.subscribe();
    let turn = app
        .submit(parent.clone(), "run the accepted task".into())
        .await
        .unwrap();
    parent_finished(&mut events, &turn).await;
    assert!(app.child_jobs(parent.clone()).await.unwrap().is_empty());
    let operations = app.tool_ops_page(parent.clone(), None, 1).await.unwrap();
    assert_eq!(operations.rows.len(), 1);
    assert_eq!(operations.rows[0].state, "completed");
    assert!(operations.rows[0].child_job.is_none());
    let selected = {
        let conn = rusqlite::Connection::open_with_flags(
            root.path().join("data/oc.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let (identity, state): (String, String) = conn
            .query_row(
                "SELECT identity,state FROM child_jobs WHERE operation_id=?1",
                [&operations.rows[0].op],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let mut job: oc_core::queries::ChildJob = serde_json::from_str(&identity).unwrap();
        job.state = serde_json::from_value(serde_json::Value::String(state)).unwrap();
        job
    };
    let page = app.read_child(parent, selected).await.unwrap();
    assert!(
        page.child_job.is_none(),
        "display budget is not read authorization"
    );
    assert!(page.title.as_ref().unwrap().len() <= oc_core::tool_output::PREVIEW_BYTES);
    assert!(
        page.rows
            .iter()
            .any(|row| row.text == "ACTUAL_SUCCESSFUL_CHILD")
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = oc_adapters::storage::Db::open(&root.path().join("data")).unwrap();
    let operations = db.list_tool_ops("parent").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(operations[0].input.as_deref().unwrap()).unwrap()
            ["description"],
        recorded_description
    );
    assert_eq!(roots.load(Ordering::SeqCst), 2);
    assert_eq!(children.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn core_child_terminal_reason_and_final_span_reopen_without_retry_replay() {
    use oc_core::domain::SessionId;
    use oc_core::queries::ChildState;
    use oc_core::session::CoreError;

    let child_steps = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let steps = child_steps.clone();
    let terminal_gate = Arc::new(Gate::default());
    let held_terminal = terminal_gate.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let peer = Peer::start(move |request| {
        if request["max_output_tokens"] == 256
            && request["tools"].as_array().is_none_or(Vec::is_empty)
        {
            return sse_delta("Synthetic title") + &sse_completed();
        }
        captured.lock().unwrap().push(request.clone());
        if request["model"] == "m" {
            if outputs(&request).is_empty() {
                return subagent_call(
                    "restricted-child",
                    serde_json::json!({"agent":"helper","description":"Restricted final","prompt":"OWN_CHILD_TASK","background":true}),
                ) + &sse_completed();
            }
            return sse_delta("Independent parent final") + &sse_completed();
        }
        assert_eq!(request["model"], "agent-model");
        match steps.fetch_add(1, Ordering::SeqCst) {
            0 => {
                sse_tool_call(
                    "settled-read",
                    "read",
                    &serde_json::json!({"path":"seed.txt"}),
                ) + &sse_completed()
            }
            attempt @ (1 | 2) => {
                assert_eq!(outputs(&request).len(), 1);
                assert!(outputs(&request)[0].1.contains("ONE_CHILD_READ"));
                let (code, message) = if attempt == 1 {
                    ("server_error", "Synthetic earlier retryable rejection")
                } else {
                    held_terminal.wait();
                    (
                        "cyber_policy",
                        "This content was flagged for possible cybersecurity risk. Review https://platform.openai.com/settings/organization/status-and-access before retrying.",
                    )
                };
                let event = serde_json::json!({"type":"response.failed","response":{"status":"failed","error":{"code":code,"message":message}}});
                let failure = format!("data: {event}\n\n");
                if attempt == 1 {
                    // A real committed partial-output retry owns a historical
                    // failed span; pre-output transparent retry reuses its span.
                    sse_delta("EARLIER_CHILD_PARTIAL") + &failure
                } else {
                    failure
                }
            }
            other => panic!("terminal child dispatched again: {other}"),
        }
    });
    let (root, app, guard) = app_fixture(
        &peer,
        serde_json::json!({"subagent":"allow","read":"allow"}),
    )
    .await;
    std::fs::write(root.path().join("source/seed.txt"), "ONE_CHILD_READ\n").unwrap();
    let parent = SessionId("parent".into());
    let mut events = app.subscribe();
    let parent_turn = app.submit(parent.clone(), "delegate".into()).await.unwrap();
    parent_finished(&mut events, &parent_turn).await;
    // Do not infer parent independence from the retry delay, or discard an early
    // child terminal event while waiting for the parent under scheduler load.
    terminal_gate.release();
    let jobs = jobs_when(&app, |jobs| jobs.len() == 1).await;
    let admitted = jobs[0].clone();
    let reason = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::TurnFailed {
                session,
                error: CoreError::Application(reason),
                ..
            } = events.recv().await.unwrap()
                && session == admitted.child
            {
                break reason;
            }
        }
    })
    .await
    .unwrap();
    // This is the RET01 owner-sanitized final reason, not successful prose or
    // the earlier retry's diagnosis. No independent classifier is introduced.
    assert!(reason.contains("cybersecurity risk"), "{reason}");
    assert!(reason.contains("https://platform.openai.com/settings/organization/status-and-access"));
    assert!(!reason.contains("Synthetic earlier"));
    let terminal = jobs_when(&app, |jobs| {
        jobs.len() == 1 && jobs[0].state == ChildState::Error && jobs[0].message_id.is_some()
    })
    .await
    .remove(0);
    assert_eq!(terminal.operation, admitted.operation);
    assert_eq!(terminal.delivery_id, admitted.delivery_id);
    assert_eq!(terminal.generation, admitted.generation);
    assert_eq!(terminal.result.as_deref(), Some(reason.as_str()));
    let page = app
        .read_child(parent.clone(), terminal.clone())
        .await
        .unwrap();
    let task = page
        .rows
        .iter()
        .find(|row| row.role == oc_core::session::Role::User)
        .unwrap();
    assert!(
        matches!(&task.child, Some(oc_core::queries::ChildHistory::Task { text, limited: false }) if text == "OWN_CHILD_TASK")
    );
    assert_ne!(
        task.text, "OWN_CHILD_TASK",
        "accepted RAW retains its native host fields"
    );
    let notice = app
        .history_message(
            parent.clone(),
            oc_core::session::MessageId(terminal.message_id.clone().unwrap()),
        )
        .await
        .unwrap();
    assert!(
        matches!(&notice.rows[0].child, Some(oc_core::queries::ChildHistory::Notice(job))
        if job.operation == terminal.operation && job.child == terminal.child
            && job.state == ChildState::Error && job.result.is_none())
    );
    let turn = page.rows.iter().find_map(|row| row.turn.as_ref()).unwrap();
    assert_eq!(turn.status, "failed");
    // The bounded metadata window combines the settled RAW read step with the
    // historical partial-output retry and the actual final attempt. No whole
    // archive reload or historical deadline-driven dispatch is needed.
    assert_eq!(turn.spans.len(), 3);
    assert_eq!(turn.spans[0].status, "completed");
    assert_eq!(turn.spans[1].status, "failed");
    assert!(turn.spans[1].retry.is_some());
    assert!(turn.spans[1].completed.is_some());
    let final_span = &turn.spans[2];
    assert_ne!(turn.spans[1].id, final_span.id);
    assert_eq!(final_span.status, "failed");
    assert!(final_span.completed.is_some());
    assert!(final_span.retry.is_none());
    assert_eq!(final_span.error.as_deref(), Some(reason.as_str()));
    assert_eq!(requests.lock().unwrap().len(), 5);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let environment = BTreeMap::from([
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("SHELL".into(), "/bin/bash".into()),
    ]);
    let (reopened, guard, _) = oc_adapters::application::spawn_with_env(
        &root.path().join("source"),
        &root.path().join("data"),
        environment,
    )
    .await
    .unwrap();
    assert_eq!(
        reopened
            .child_jobs(parent.clone())
            .await
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&terminal)
    );
    assert_eq!(
        reopened.read_child(parent, terminal.clone()).await.unwrap(),
        page
    );
    reopened.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 5, "reopen generated work");
    assert_eq!(child_steps.load(Ordering::SeqCst), 3);
    let db = Db::open(&root.path().join("data")).unwrap();
    let reads = db.list_tool_ops(&terminal.child.0).unwrap();
    assert_eq!(reads.len(), 1);
    assert_eq!(reads[0].name, "read");
    assert_eq!(reads[0].state, "completed");
}

#[tokio::test]
async fn core_jobs_busy_continuation_selected_fences_and_source_survive_location_switch() {
    use oc_core::queries::ChildState;
    let parent_steps = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let steps = parent_steps.clone();
    let child_one = Arc::new(Gate::default());
    let child_two = Arc::new(Gate::default());
    let held_one = child_one.clone();
    let held_two = child_two.clone();
    let selected = Arc::new(Mutex::new(String::new()));
    let target = selected.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let peer = Peer::start(move |request| {
        if request["max_output_tokens"] == 256
            && request["tools"].as_array().is_none_or(Vec::is_empty)
        {
            return sse_delta("Synthetic title") + &sse_completed();
        }
        captured.lock().unwrap().push(request.clone());
        if request["model"] == "m" {
            match steps.fetch_add(1, Ordering::SeqCst) {
                0 => {
                    subagent_call(
                        "one",
                        serde_json::json!({"agent":"helper","description":"A","prompt":"FIRST_TASK","background":true}),
                    ) + &subagent_call(
                        "two",
                        serde_json::json!({"agent":"helper","description":"B","prompt":"SECOND_TASK","background":true}),
                    ) + &sse_completed()
                }
                1 => {
                    assert_eq!(outputs(&request).len(), 2);
                    *target.lock().unwrap() =
                        serde_json::from_str::<serde_json::Value>(&outputs(&request)[0].1).unwrap()
                            ["sessionID"]
                            .as_str()
                            .unwrap()
                            .into();
                    sse_delta("parent first finished") + &sse_completed()
                }
                2 => {
                    subagent_call(
                        "busy",
                        serde_json::json!({"agent":"helper","description":"Busy continuation","prompt":"NO_HIDDEN_FORK","background":true,"sessionID":target.lock().unwrap().clone()}),
                    ) + &sse_completed()
                }
                3 => {
                    assert!(outputs(&request).last().unwrap().1.contains("child busy"));
                    sse_delta("parent second finished") + &sse_completed()
                }
                other => panic!("idle work generated parent request {other}"),
            }
        } else if outputs(&request).is_empty() {
            if request.to_string().contains("FIRST_TASK") {
                held_one.wait();
                sse_delta("late cancelled output") + &sse_completed()
            } else {
                held_two.wait();
                sse_tool_call(
                    "source-effect",
                    "bash",
                    &serde_json::json!({"argv":["/bin/sh","-c","printf source > own-effect"]}),
                ) + &sse_completed()
            }
        } else {
            sse_delta("error: successful prose") + &sse_completed()
        }
    });
    let (root, app, guard) = app_fixture(
        &peer,
        serde_json::json!({"subagent":"allow","shell":"allow","read":"allow","edit":"allow"}),
    )
    .await;
    let parent = oc_core::domain::SessionId("parent".into());
    let mut events = app.subscribe();
    let first_turn = app
        .submit(parent.clone(), "PARENT_PRIVATE".into())
        .await
        .unwrap();
    parent_finished(&mut events, &first_turn).await;
    let jobs = jobs_when(&app, |jobs| {
        jobs.len() == 2 && jobs.iter().all(|job| job.state == ChildState::Running)
    })
    .await;
    assert_eq!(requests.lock().unwrap().len(), 4);
    let second_turn = app
        .submit(parent.clone(), "continue busy".into())
        .await
        .unwrap();
    parent_finished(&mut events, &second_turn).await;
    assert_eq!(requests.lock().unwrap().len(), 6);
    assert_eq!(jobs_when(&app, |jobs| jobs.len() == 2).await.len(), 2);
    let active = jobs
        .iter()
        .find(|job| job.child.0 == *selected.lock().unwrap())
        .unwrap()
        .clone();
    for field in 0..5 {
        let mut stale = active.clone();
        match field {
            0 => stale.generation += 1,
            1 => stale.child.0.push_str("foreign"),
            2 => stale.operation.push_str("stale"),
            3 => stale.location.push_str("foreign"),
            _ => stale.delivery_id.push_str("stale"),
        }
        assert!(app.read_child(parent.clone(), stale.clone()).await.is_err());
        assert!(
            app.background_child(parent.clone(), stale.clone())
                .await
                .is_err()
        );
        assert!(app.interrupt_child(parent.clone(), stale).await.is_err());
    }
    assert!(
        app.interrupt_child(oc_core::domain::SessionId("foreign".into()), active.clone())
            .await
            .is_err()
    );
    let destination = root.path().join("destination");
    std::fs::create_dir(&destination).unwrap();
    std::fs::copy(
        root.path().join("source/opencode.json"),
        destination.join("opencode.json"),
    )
    .unwrap();
    app.switch_location_home(destination.to_string_lossy().into())
        .await
        .unwrap();
    assert_eq!(app.child_jobs(parent.clone()).await.unwrap().len(), 2);
    let page = app
        .read_child(parent.clone(), active.clone())
        .await
        .unwrap();
    assert_eq!(page.parent_id.as_deref(), Some(parent.0.as_str()));
    assert_eq!(
        page.rows
            .iter()
            .filter(|r| r.role == oc_core::session::Role::User)
            .count(),
        1
    );
    assert!(
        app.background_child(parent.clone(), active.clone())
            .await
            .is_ok()
    );
    app.interrupt_child(parent.clone(), active).await.unwrap();
    child_one.release();
    child_two.release();
    let terminal = jobs_when(&app, |jobs| {
        jobs.len() == 2 && jobs.iter().all(|job| job.message_id.is_some())
    })
    .await;
    assert!(
        terminal
            .iter()
            .any(|job| job.state == ChildState::Cancelled)
    );
    assert!(terminal.iter().any(|job| job.state == ChildState::Completed
        && job.result.as_deref() == Some("error: successful prose")));
    assert_eq!(
        std::fs::read(root.path().join("source/own-effect")).unwrap(),
        b"source"
    );
    assert!(!destination.join("own-effect").exists());
    assert_eq!(requests.lock().unwrap().len(), 7);
    for request in requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| request["model"] != "m")
    {
        assert!(request.to_string().contains("OWN_BACKGROUND_PROFILE"));
        assert!(!request.to_string().contains("PARENT_PRIVATE"));
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn background_child_posts_effects_and_completes_before_parent_response() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    let mut helper = agent("helper", false, Some("test/agent-model"));
    helper.prompt = "BACKGROUND_OWN_SYSTEM".into();
    runtime
        .publish_subagents(Some(catalog(2, vec![helper])))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let child_gate = Arc::new(Gate::default());
    let parent_gate = Arc::new(Gate::default());
    let held_child = child_gate.clone();
    let held_parent = parent_gate.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let peer = Peer::start(move |request| {
        if request["model"] == "m" {
            if outputs(&request).is_empty() {
                subagent_call(
                    "bg-launch",
                    serde_json::json!({"agent":"helper","description":"Independent work","prompt":"BACKGROUND_TASK","background":true}),
                ) + &sse_completed()
            } else if outputs(&request).len() == 1 {
                tx.send(("parent-active".into(), request)).unwrap();
                held_parent.wait();
                sse_tool_call(
                    "parent-effect",
                    "bash",
                    &serde_json::json!({"argv":["/bin/sh","-c","printf p >> parent-effect"]}),
                ) + &sse_completed()
            } else {
                tx.send(("parent-notice".into(), request)).unwrap();
                sse_delta("parent final") + &sse_completed()
            }
        } else if outputs(&request).is_empty() {
            tx.send(("child-post".into(), request)).unwrap();
            held_child.wait();
            sse_tool_call(
                "child-effect",
                "bash",
                &serde_json::json!({"argv":["/bin/sh","-c","printf c >> child-effect"]}),
            ) + &sse_completed()
        } else {
            tx.send(("child-followup".into(), request)).unwrap();
            sse_delta("error: this is successful child prose") + &sse_completed()
        }
    });
    let running = runtime.run_turn(params(
        "parent",
        "PARENT_PRIVATE",
        &harness,
        provider_of(&peer.base),
        &NO_CANCEL,
    ));
    let driver = async {
        let arrivals = [received(&mut rx).await, received(&mut rx).await];
        // Independent HTTP handlers may enter in either order. Both physical
        // POSTs must be at their barriers while the parent is still active.
        let first = arrivals
            .iter()
            .find(|(kind, _)| kind == "child-post")
            .unwrap();
        assert!(!first.1.to_string().contains("PARENT_PRIVATE"));
        assert!(first.1.to_string().contains("BACKGROUND_OWN_SYSTEM"));
        let active = arrivals
            .iter()
            .find(|(kind, _)| kind == "parent-active")
            .unwrap();
        assert!(outputs(&active.1)[0].1.contains("running"));
        assert!(runtime.turn_active());
        let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        assert_eq!(sql.query_row("SELECT count(*) FROM child_jobs j JOIN turns t ON t.id=j.child_turn AND t.session_id=j.child_id WHERE j.state='running' AND t.status='started'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        child_gate.release();
        assert_eq!(received(&mut rx).await.0, "child-followup");
        wait_for(|| {
            harness
                .db
                .children_of("parent")
                .unwrap()
                .iter()
                .any(|child| {
                    messages(&harness.db, child).iter().any(|(role, text)| {
                        role == "assistant" && text == "error: this is successful child prose"
                    })
                })
        })
        .await;
        assert!(runtime.turn_active());
        assert_eq!(
            std::fs::read(harness._project.path().join("child-effect")).unwrap(),
            b"c"
        );
        assert!(!harness._project.path().join("parent-effect").exists());
        parent_gate.release();
        let notice = received(&mut rx).await;
        assert_eq!(notice.0, "parent-notice");
        assert!(notice.1.to_string().contains("subagent"));
        assert!(
            notice
                .1
                .to_string()
                .contains("error: this is successful child prose")
        );
        assert_eq!(outputs(&notice.1).len(), 2);
    };
    let (result, ()) = tokio::join!(running, driver);
    assert_eq!(result.unwrap().status, TurnStatus::Completed);
    assert_eq!(
        std::fs::read(harness._project.path().join("parent-effect")).unwrap(),
        b"p"
    );
    assert_eq!(harness.db.children_of("parent").unwrap().len(), 1);
    assert_eq!(harness.db.list_tool_ops("parent").unwrap().len(), 2);
    assert_eq!(
        messages(&harness.db, "parent")
            .iter()
            .filter(|(_, text)| text.contains("Automatic background subagent result"))
            .count(),
        1
    );
}

#[tokio::test]
async fn background_admission_caps_before_child_prompt_or_effect() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            2,
            vec![agent("helper", false, Some("test/agent-model"))],
        )))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let children = Arc::new(Gate::default());
    let parent = Arc::new(Gate::default());
    let held_children = children.clone();
    let held_parent = parent.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let peer = Peer::start(move |request| {
        if request["model"] == "m" && outputs(&request).is_empty() {
            (0..6).map(|id|subagent_call(&format!("job-{id}"),serde_json::json!({"agent":"helper","description":"Bounded child","prompt":format!("TASK-{id}"),"background":true}))).collect::<String>()+&sse_completed()
        } else if request["model"] == "m" {
            tx.send(("parent".into(), request)).unwrap();
            held_parent.wait();
            sse_delta("parent done") + &sse_completed()
        } else if outputs(&request).is_empty() {
            tx.send(("child".into(), request)).unwrap();
            held_children.wait();
            sse_tool_call(
                "only-admitted-effect",
                "bash",
                &serde_json::json!({"argv":["/bin/sh","-c","printf x >> bounded-effect"]}),
            ) + &sse_completed()
        } else {
            sse_delta("child done") + &sse_completed()
        }
    });
    let running = runtime.run_turn(params(
        "parent",
        "caps",
        &harness,
        provider_of(&peer.base),
        &NO_CANCEL,
    ));
    let driver = async {
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(received(&mut rx).await);
        }
        assert_eq!(seen.iter().filter(|(kind, _)| kind == "child").count(), 4);
        let wire = &seen.iter().find(|(kind, _)| kind == "parent").unwrap().1;
        assert_eq!(
            outputs(wire)
                .iter()
                .filter(|(_, text)| text.contains("capacity exhausted"))
                .count(),
            2
        );
        assert_eq!(harness.db.children_of("parent").unwrap().len(), 4);
        assert!(!harness._project.path().join("bounded-effect").exists());
        children.release();
        wait_for(|| {
            harness
                .db
                .children_of("parent")
                .unwrap()
                .iter()
                .all(|child| {
                    messages(&harness.db, child)
                        .iter()
                        .any(|(role, text)| role == "assistant" && text == "child done")
                })
        })
        .await;
        assert!(runtime.turn_active());
        assert_eq!(
            std::fs::read(harness._project.path().join("bounded-effect")).unwrap(),
            b"xxxx"
        );
        parent.release();
    };
    let (result, ()) = tokio::join!(running, driver);
    assert_eq!(
        result
            .unwrap()
            .calls
            .iter()
            .filter(|call| call.state == "running")
            .count(),
        4
    );
    runtime.shutdown_mcp().await.unwrap();
}

#[tokio::test]
async fn completed_background_child_continues_own_history_model_and_generation() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(
            2,
            vec![agent("helper", false, Some("test/agent-model"))],
        )))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let target = Arc::new(Mutex::new(String::new()));
    let selected = target.clone();
    let child_step = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let steps = child_step.clone();
    let root_step = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let roots = root_step.clone();
    let gates = [Arc::new(Gate::default()), Arc::new(Gate::default())];
    let held = gates.clone();
    let peer = Peer::start(move |request| {
        if request["model"] == "m" {
            match roots.fetch_add(1, Ordering::SeqCst) {
                0 => {
                    subagent_call(
                        "fresh",
                        serde_json::json!({"agent":"helper","description":"Own first","prompt":"FIRST_CHILD_TASK","background":true}),
                    ) + &sse_completed()
                }
                1 => {
                    *selected.lock().unwrap() =
                        serde_json::from_str::<serde_json::Value>(&outputs(&request)[0].1).unwrap()
                            ["sessionID"]
                            .as_str()
                            .unwrap()
                            .into();
                    held[0].wait();
                    sse_tool_call(
                        "parent-one",
                        "bash",
                        &serde_json::json!({"argv":["/bin/sh","-c","printf p >> parent-history-effect"]}),
                    ) + &sse_completed()
                }
                2 | 5 => sse_delta("parent done") + &sse_completed(),
                3 => {
                    subagent_call(
                        "continue",
                        serde_json::json!({"agent":"helper","description":"Own second","prompt":"CONTINUATION_CHILD_TASK","background":true,"sessionID":selected.lock().unwrap().clone()}),
                    ) + &sse_completed()
                }
                4 => {
                    held[1].wait();
                    sse_tool_call(
                        "parent-two",
                        "bash",
                        &serde_json::json!({"argv":["/bin/sh","-c","printf p >> parent-history-effect"]}),
                    ) + &sse_completed()
                }
                other => panic!("duplicate parent request {other}"),
            }
        } else {
            assert!(!request.to_string().contains("PARENT_PRIVATE"));
            match steps.fetch_add(1, Ordering::SeqCst) {
                0 => {
                    sse_tool_call(
                        "own-effect-one",
                        "bash",
                        &serde_json::json!({"argv":["/bin/sh","-c","printf 1 >> own-history-effect"]}),
                    ) + &sse_completed()
                }
                1 => sse_delta("own first answer") + &sse_completed(),
                2 => {
                    assert!(request.to_string().contains("own first answer"));
                    assert!(request.to_string().contains("FIRST_CHILD_TASK"));
                    assert!(request.to_string().contains("CONTINUATION_CHILD_TASK"));
                    sse_tool_call(
                        "own-effect-two",
                        "bash",
                        &serde_json::json!({"argv":["/bin/sh","-c","printf 2 >> own-history-effect"]}),
                    ) + &sse_completed()
                }
                3 => sse_delta("own second answer") + &sse_completed(),
                other => panic!("hidden child fork/request {other}"),
            }
        }
    });
    for (index, expected) in ["own first answer", "own second answer"]
        .into_iter()
        .enumerate()
    {
        let running = runtime.run_turn(params(
            "parent",
            "PARENT_PRIVATE",
            &harness,
            provider_of(&peer.base),
            &NO_CANCEL,
        ));
        let driver = async {
            wait_for(|| {
                !target.lock().unwrap().is_empty()
                    && messages(&harness.db, &target.lock().unwrap())
                        .iter()
                        .any(|(role, text)| role == "assistant" && text == expected)
            })
            .await;
            let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
            wait_for(|| {
                sql.query_row(
                    "SELECT count(*) FROM child_jobs WHERE state='completed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap()
                    == (index + 1) as i64
            })
            .await;
            gates[index].release();
        };
        let (result, ()) = tokio::join!(running, driver);
        assert_eq!(result.unwrap().status, TurnStatus::Completed);
    }
    assert_eq!(harness.db.children_of("parent").unwrap().len(), 1);
    assert_eq!(child_step.load(Ordering::SeqCst), 4);
    assert_eq!(root_step.load(Ordering::SeqCst), 6);
    assert_eq!(
        std::fs::read(harness._project.path().join("own-history-effect")).unwrap(),
        b"12"
    );
    let launches = harness
        .db
        .list_tool_ops("parent")
        .unwrap()
        .into_iter()
        .filter(|operation| operation.name == "subagent")
        .collect::<Vec<_>>();
    assert_eq!(launches.len(), 2);
    assert!(
        launches
            .iter()
            .all(|operation| operation.state == "running"),
        "{launches:#?}"
    );
    assert_ne!(launches[0].op, launches[1].op);
    assert_eq!(
        messages(&harness.db, "parent")
            .iter()
            .filter(|(_, text)| text.contains("Automatic background subagent result"))
            .count(),
        2
    );
    runtime.shutdown_mcp().await.unwrap();
}
