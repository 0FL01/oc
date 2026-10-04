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
