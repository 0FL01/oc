use super::*;
use std::os::unix::process::CommandExt as _;
use std::task::Poll;

#[tokio::test]
async fn hot01_child_recovery_uses_saved_provenance_not_the_current_counter() {
    use crate::provider::protocol::Protocol;
    use crate::runtime::tests::binding_replay_tests::{owner, request, respond};
    use oc_core::domain::SessionId;
    use serde_json::json;
    for refusal in [
        None,
        Some("configuration"),
        Some("credentials"),
        Some("missing-fence"),
        Some("unknown-effect"),
    ] {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = ResponsesConfig {
            base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
            api_key: "synthetic recovery key".into(),
            timeout: None,
            header_timeout_ms: Some(crate::provider::HEADER_TIMEOUT_MS),
            chunk_timeout_ms: Some(3000),
            connect_timeout: std::time::Duration::from_secs(1),
            allow_private: true,
            headers: BTreeMap::new(),
            cache: crate::provider::CacheMode::None,
            wire: Default::default(),
        };
        let catalog = ModelCatalog {
            provider: "fixture".into(),
            models: BTreeMap::from([(
                "m".into(),
                json!({"limit":{"context":500000,"output":1000}}),
            )]),
        };
        let agent = SubagentAgent {
            id: "helper".into(),
            description: "helper".into(),
            primary: false,
            model: None,
            variant: None,
            prompt: "RECOVERY_PROFILE".into(),
            permissions: BTreeMap::new(),
            permission_rules: Default::default(),
            hidden: false,
            digest: Some("helper-profile".into()),
            request: Default::default(),
            color: None,
        };
        {
            let db = Db::open(data.path()).unwrap();
            let runtime = owner(&db, project.path());
            runtime.create_session("parent").unwrap();
            db.accept_turn(
                "parent-turn",
                "parent",
                "delegate",
                "delegate",
                &oc_core::queries::ModelRef {
                    provider: "fixture".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap();
            db.finish_turn("parent-turn", "completed", None).unwrap();
            runtime
                .publish_subagents(Some(SubagentCatalog {
                    agents: BTreeMap::from([("helper".into(), agent.clone())]),
                    depth_limit: 4,
                }))
                .unwrap();
            let identity = ChildJob {
                parent: SessionId("parent".into()),
                child: SessionId("child".into()),
                operation: "resume-launch".into(),
                generation: 77,
                location: "work".into(),
                agent: "helper".into(),
                model: "fixture/m".into(),
                description: "work".into(),
                delivery_id: "child-delivery:resume-launch".into(),
                state: ChildState::Admitted,
                background: true,
                turn: None,
                result: None,
                message_id: None,
            };
            db.record_tool_intent(
                &identity.operation,
                "parent",
                None,
                "subagent",
                "{\"background\":true}",
            )
            .unwrap();
            db.record_tool_outcome(&identity.operation, "completed", Some("child accepted"))
                .unwrap();
            let published = runtime.current.read().unwrap().clone();
            let mut fence = json!({
                "version":1, "identity":identity, "parent_turn":"parent-turn", "lane":runtime.primary_lane(&published),
                "generation_fingerprint":runtime.child_recovery_fingerprint(),
                "launch_source":db.child_launch_fingerprint(&identity.operation).unwrap(),
                "route":crate::compaction::route_identity("fixture", "m", &provider).unwrap(),
            });
            // Legacy fences need not contain launch_sources. The existing exact
            // launch/source/route/turn guards remain authoritative after reopen.
            fence["fingerprint"] = crate::compaction::fingerprint(&fence).into();
            if refusal == Some("missing-fence") {
                db.admit_fresh_child_job(&identity).unwrap();
            } else {
                db.admit_recoverable_child_job(&identity, true, &fence)
                    .unwrap();
            }
            let accepted = db
                .accept_turn(
                    "child-turn",
                    "child",
                    "own task",
                    "own task",
                    &oc_core::queries::ModelRef {
                        provider: "fixture".into(),
                        id: "m".into(),
                        variant: None,
                    },
                )
                .unwrap();
            db.start_child_job(&identity.operation, "child-turn")
                .unwrap();
            let mut log = TurnLog::new("child-turn", "m", "fixture");
            log.user_message = Some(accepted.user_message);
            log.agent_digest = agent.digest.clone();
            log.display = json!({"location":"work","move_epoch":0,"config_generation":77,"owning_operation":identity.operation,"agent":"helper"});
            log.input
                .push(InputItem::message(InputRole::User, "own task"));
            db.checkpoint_turn("child-turn", &log.to_json().to_string())
                .unwrap();
            if refusal == Some("unknown-effect") {
                db.record_tool_intent(
                    "uncertain-effect",
                    "child",
                    Some("child-turn"),
                    "shell",
                    "{\"command\":\"must not replay\"}",
                )
                .unwrap();
            }
            runtime.shutdown_mcp().await.unwrap();
        }
        let db = Db::open(data.path()).unwrap();
        db.recover_child_jobs().unwrap();
        db.recover_interrupted_tools().unwrap();
        let runtime = owner(&db, project.path());
        runtime
            .publish_subagents(Some(SubagentCatalog {
                agents: BTreeMap::from([("helper".into(), agent)]),
                depth_limit: 4,
            }))
            .unwrap();
        assert_ne!(runtime.generation_id(), 77);
        if refusal == Some("configuration") {
            runtime.update_execution(|view| {
                view.generation
                    .config
                    .permissions
                    .insert("read".into(), Permission::Deny);
            });
        }
        let mut restored_provider = provider.clone();
        if refusal == Some("credentials") {
            restored_provider.api_key = "different admitted key".into();
        }
        runtime
            .recover_background_children(&catalog, &restored_provider, false)
            .await
            .unwrap();
        if refusal.is_none() {
            assert_ne!(
                db.child_jobs("parent").unwrap()[0].state,
                ChildState::Unknown
            );
            let (mut socket, input) =
                tokio::time::timeout(std::time::Duration::from_secs(3), request(&listener))
                    .await
                    .unwrap();
            assert!(
                input.to_string().contains("own task")
                    && input.to_string().contains("RECOVERY_PROFILE")
            );
            respond(&mut socket, Protocol::Responses, false).await;
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while db.child_jobs("parent").unwrap()[0].state != ChildState::Completed {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let job = &db.child_jobs("parent").unwrap()[0];
            assert_eq!(job.generation, 77, "launch attribution is immutable");
            let log = TurnLog::from_json(
                &serde_json::from_str(&db.turn_result("child-turn").unwrap().1.unwrap()).unwrap(),
            )
            .unwrap();
            assert_eq!(log.requests.len(), 1);
            assert_eq!(
                log.requests[0].config_generation,
                Some(runtime.generation_id())
            );
            assert_eq!(
                db.read_history_page("child", 100, None)
                    .unwrap()
                    .iter()
                    .filter(|row| row.1 == "user")
                    .count(),
                1
            );
        } else {
            assert_eq!(
                db.child_jobs("parent").unwrap()[0].state,
                ChildState::Unknown
            );
            assert_eq!(db.turn_result("child-turn").unwrap().0, "unknown");
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept())
                .await
                .is_err(),
            "no extra or refused recovery dispatch"
        );
        assert_eq!(
            db.list_tool_ops("child").unwrap().len(),
            usize::from(refusal == Some("unknown-effect"))
        );
        runtime.child_jobs.shutdown().await.unwrap();
        runtime.shutdown_mcp().await.unwrap();
    }
}

#[tokio::test]
async fn term01_terminal_creation_uses_retained_child_source_not_moved_parent() {
    use oc_core::queries::{TerminalAction, TerminalSize};
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).unwrap();
    std::fs::create_dir(&b).unwrap();
    let db = Db::open(&dir.path().join("data")).unwrap();
    let make = |project: &std::path::Path, shell: &str| {
        Runtime::new(
            &db,
            project.to_str().unwrap(),
            Generation::default(),
            ProtectedGlobs { patterns: vec![] },
            crate::files::Files::new(project, db.root()).unwrap(),
            crate::shell::Shell::new(project).unwrap(),
            BTreeMap::from([
                ("SHELL".into(), shell.into()),
                ("HOME".into(), project.display().to_string()),
                ("OC_API_KEY".into(), "SYNTHETIC_NOT_INHERITED".into()),
            ]),
            ToolRoots {
                project: project.into(),
                data: db.root().into(),
            },
            None,
            false,
            DcpConfig::default(),
        )
        .unwrap()
    };
    let original = make(&a, "/bin/bash");
    original.create_session("parent").unwrap();
    let source = Arc::new(original.owned_child_snapshot());
    let mut selected = identity("parent", "child");
    selected.location = a.display().to_string();
    selected.generation = source.generation_id();
    db.record_tool_intent(&selected.operation, "parent", None, "subagent", "{}")
        .unwrap();
    db.admit_fresh_child_job(&selected).unwrap();
    original.child_jobs.work.lock().unwrap().insert(
        selected.operation.clone(),
        Work {
            identity: selected.clone(),
            parent_turn: "fixture".into(),
            runtime: Arc::downgrade(&source),
            mcp: source.mcp_owner(),
            cancel: Arc::new(AtomicBool::new(false)),
            completion: async { true }.boxed().shared(),
            background: Arc::new(AtomicBool::new(true)),
            mode_changed: Arc::new(tokio::sync::Notify::new()),
            foreground_wait: Arc::new(AtomicBool::new(false)),
            parent_rejected: AtomicBool::new(false),
            foreground_result: Arc::new(Mutex::new(None)),
        },
    );
    let mut moved = make(&b, "/bin/sh");
    moved.inherit_application_owners(&original);
    let mut terminals = crate::terminals::Terminals::new(&db).unwrap();
    let request = |job: &ChildJob| TerminalAction::CreateChild {
        source: Box::new(job.clone()),
        generation: 19,
        size: TerminalSize::default(),
    };
    for field in 0..8 {
        let mut stale = selected.clone();
        match field {
            0 => stale.parent.0.push('x'),
            1 => stale.child.0.push('x'),
            2 => stale.operation.push('x'),
            3 => stale.generation += 1,
            4 => stale.location.push('x'),
            5 => stale.delivery_id.push('x'),
            6 => stale.agent.push('x'),
            _ => stale.model.push('x'),
        }
        assert!(
            moved
                .create_terminal(&mut terminals, selected.child.clone(), request(&stale), 19)
                .is_err()
        );
    }
    assert!(
        moved
            .create_terminal(
                &mut terminals,
                selected.parent.clone(),
                request(&selected),
                19
            )
            .is_err()
    );
    assert!(
        terminals
            .inventory(&selected.child)
            .unwrap()
            .entries
            .is_empty()
    );
    let entry = moved
        .create_terminal(
            &mut terminals,
            selected.child.clone(),
            request(&selected),
            19,
        )
        .unwrap();
    assert_eq!(entry.target.session, selected.child);
    assert_eq!(entry.target.location, selected.location);
    assert_eq!(entry.target.generation, selected.generation);
    assert_eq!(entry.cwd, selected.location);
    assert_eq!(entry.shell, "/bin/bash");
    let environment = std::fs::read(format!("/proc/{}/environ", entry.pid)).unwrap();
    assert!(
        !environment
            .windows(b"SYNTHETIC_NOT_INHERITED".len())
            .any(|w| w == b"SYNTHETIC_NOT_INHERITED")
    );
    // Losing the retained child lane never fabricates one at the moved parent.
    drop(source);
    assert!(
        moved
            .create_terminal(
                &mut terminals,
                selected.child.clone(),
                request(&selected),
                19
            )
            .is_err()
    );
    assert_eq!(
        terminals.inventory(&selected.child).unwrap().entries.len(),
        1
    );
    let current = make(&a, "/bin/sh");
    assert!(
        current
            .create_terminal(
                &mut terminals,
                selected.child.clone(),
                request(&selected),
                19
            )
            .is_err()
    );
    db.finish_child_job(
        &selected.operation,
        ChildState::Completed,
        "settled fixture",
    )
    .unwrap();
    let next = current
        .create_terminal(
            &mut terminals,
            selected.child.clone(),
            request(&selected),
            19,
        )
        .unwrap();
    assert_eq!(next.shell, "/bin/sh");
    assert_eq!(next.cwd, selected.location);
    assert_eq!(next.target.generation, 19);
    assert_eq!(
        terminals.inventory(&selected.child).unwrap().entries[0].target,
        entry.target
    );
    terminals.shutdown().unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{}", entry.pid)).exists());
    assert!(!std::path::Path::new(&format!("/proc/{}", next.pid)).exists());
    original.child_jobs.shutdown().await.unwrap();
}

fn identity(parent: &str, child: &str) -> ChildJob {
    ChildJob {
        parent: oc_core::domain::SessionId(parent.into()),
        child: oc_core::domain::SessionId(child.into()),
        operation: format!("launch:{child}"),
        generation: 7,
        location: "/owned-source".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: "owned child".into(),
        delivery_id: format!("notice:{child}"),
        state: ChildState::Admitted,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    }
}

#[tokio::test]
async fn foreground_conversion_releases_only_waiter_and_retains_actual_join_and_fences() {
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_session("parent").unwrap();
    db.begin_turn("parent-turn", "parent", "parent").unwrap();
    let jobs = Jobs::new(&db);
    let mut selected = identity("parent", "held");
    selected.background = false;
    db.record_tool_intent(&selected.operation, "parent", None, "subagent", "{}")
        .unwrap();
    db.admit_fresh_child_job(&selected).unwrap();
    let (release, held) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        held.await.unwrap();
        true
    });
    let completion = async move { task.await.unwrap() }.boxed().shared();
    let cancel = Arc::new(AtomicBool::new(false));
    jobs.work.lock().unwrap().insert(
        selected.operation.clone(),
        Work {
            identity: selected.clone(),
            parent_turn: "parent-turn".into(),
            runtime: std::sync::Weak::new(),
            mcp: Arc::new(mcp::McpOwner::new("/owned-source", 7)),
            cancel: cancel.clone(),
            completion: completion.clone(),
            background: Arc::new(AtomicBool::new(false)),
            mode_changed: Arc::new(tokio::sync::Notify::new()),
            foreground_wait: Arc::new(AtomicBool::new(true)),
            parent_rejected: AtomicBool::new(false),
            foreground_result: Arc::new(Mutex::new(None)),
        },
    );
    let parent_cancel = AtomicBool::new(false);
    let mut waiter = Box::pin(jobs.wait_foreground(&selected, &parent_cancel));
    assert!(futures_util::poll!(&mut waiter).is_pending());
    for field in 0..6 {
        let mut stale = selected.clone();
        match field {
            0 => stale.parent.0.push('x'),
            1 => stale.child.0.push('x'),
            2 => stale.operation.push('x'),
            3 => stale.generation += 1,
            4 => stale.location.push('x'),
            _ => stale.delivery_id.push('x'),
        }
        assert!(!jobs.background("parent", &stale));
        assert!(!jobs.interrupt("parent", &stale));
    }
    assert!(!jobs.background("foreign", &selected));
    assert!(jobs.background("parent", &selected));
    assert!(jobs.background("parent", &selected));
    assert!(
        matches!(waiter.await.unwrap(),SubagentOutcome::Running {session_id,operation,..} if session_id==selected.child.0 && operation==selected.operation)
    );
    assert!(!cancel.load(Ordering::Acquire));
    assert!(completion.clone().now_or_never().is_none());
    let mut successor = Box::pin(jobs.join_child(&selected.child.0, &parent_cancel));
    assert!(
        futures_util::poll!(&mut successor).is_pending(),
        "explicit continuation crossed released waiter instead of actual join"
    );
    assert_eq!(jobs.work.lock().unwrap().len(), 1);
    assert!(jobs.interrupt("parent", &selected));
    assert!(cancel.load(Ordering::Acquire));
    assert!(
        !jobs.take_parent_rejection("parent"),
        "selected cancellation became permission rejection"
    );
    jobs.permission_rejected(&selected.operation);
    assert!(!jobs.take_parent_rejection("foreign"));
    assert!(jobs.take_parent_rejection("parent"));
    assert!(!jobs.take_parent_rejection("parent"));
    db.finish_turn("parent-turn", "completed", Some("parent done"))
        .unwrap();
    db.begin_turn("new-parent-turn", "parent", "unrelated turn")
        .unwrap();
    jobs.permission_rejected(&selected.operation);
    assert!(
        !jobs.take_parent_rejection("parent"),
        "late old-child rejection cancelled a new parent turn"
    );
    db.finish_child_job(&selected.operation, ChildState::Cancelled, "cancelled")
        .unwrap();
    assert!(!jobs.background("parent", &selected));
    assert!(!jobs.interrupt("parent", &selected));
    release.send(()).unwrap();
    assert!(completion.await);
    assert!(successor.await);
    jobs.shutdown().await.unwrap();
    assert!(jobs.work.lock().unwrap().is_empty());
}

#[test]
fn seven_outstanding_two_reserved_launches_reject_without_orphan_or_cleanup() {
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    for parent in ["seed", "left", "right", "unrelated"] {
        db.create_session(parent).unwrap();
    }
    db.set_pref("unrelated-pref", "preserved").unwrap();
    db.append_message("unrelated", "user", "preserved history")
        .unwrap();
    for i in 0..7 {
        let job = identity("seed", &format!("seed-{i}"));
        db.record_tool_intent(&job.operation, "seed", None, "subagent", "{}")
            .unwrap();
        db.admit_fresh_child_job(&job).unwrap();
        db.finish_child_job(
            &job.operation,
            ChildState::Completed,
            "committed undelivered",
        )
        .unwrap();
    }
    let original = db.list_sessions().unwrap();
    let unrelated = db.read_history_full("unrelated").unwrap();
    let jobs = Jobs::new(&db);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let outcomes = std::thread::scope(|scope| {
        let mut launches = Vec::new();
        for parent in ["left", "right"] {
            let jobs = jobs.clone();
            let barrier = barrier.clone();
            let db = db.shared_handle();
            launches.push(scope.spawn(move || {
                let job = identity(parent, &format!("fresh-{parent}"));
                db.record_tool_intent(&job.operation, parent, None, "subagent", "{}")
                    .unwrap();
                let _reservation = jobs.reserve(parent, &job.child.0).unwrap();
                // Both callers have passed the real owner reservation against
                // seven terminal, undelivered facts before either admission.
                barrier.wait();
                let outcome = db.admit_fresh_child_job(&job);
                (job, outcome)
            }));
        }
        launches
            .into_iter()
            .map(|launch| launch.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        outcomes.iter().filter(|(_, result)| result.is_ok()).count(),
        1
    );
    let rejected = &outcomes
        .iter()
        .find(|(_, result)| result.is_err())
        .unwrap()
        .0;
    assert_eq!(db.child_job_outstanding().unwrap(), 8);
    assert_eq!(db.read_history_full("unrelated").unwrap(), unrelated);
    assert_eq!(
        db.get_pref("unrelated-pref").unwrap().as_deref(),
        Some("preserved")
    );
    assert_eq!(
        db.list_sessions().unwrap().len(),
        original.len() + 1,
        "rejected fresh launch left an orphan session"
    );
    assert!(matches!(
        db.session_meta(&rejected.child.0),
        Err(StorageError::SessionNotFound)
    ));
    assert!(
        db.get_pref(&session_location_key(&rejected.child.0))
            .unwrap()
            .is_none()
    );
    assert!(db.child_jobs(&rejected.parent.0).unwrap().is_empty());
    assert!(db.children_of(&rejected.parent.0).unwrap().is_empty());
}

#[tokio::test]
async fn dropped_shutdown_keeps_shared_join_waits_and_reaps_leaf_with_sticky_failure() {
    for good in [true, false] {
        let data = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        let jobs = Jobs::new(&db);
        let (release, held) = tokio::sync::oneshot::channel::<()>();
        let (started, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut command = tokio::process::Command::new("/bin/sh");
            command.args(["-c", "exec sleep 30"]).kill_on_drop(true);
            command.as_std_mut().process_group(0);
            let mut child = command.spawn().unwrap();
            let pid = child.id().unwrap();
            started.send(pid).unwrap();
            let _ = held.await;
            child.start_kill().unwrap();
            child.wait().await.unwrap();
            good
        });
        let pid = ready.await.unwrap();
        let completion = async move { task.await.unwrap_or(false) }.boxed().shared();
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.work.lock().unwrap().insert(
            "launch:held".into(),
            Work {
                identity: identity("parent", "held"),
                parent_turn: "closed-parent-turn".into(),
                runtime: std::sync::Weak::new(),
                mcp: Arc::new(mcp::McpOwner::new("/owned-source", 7)),
                cancel: cancel.clone(),
                completion: completion.clone(),
                background: Arc::new(AtomicBool::new(true)),
                mode_changed: Arc::new(tokio::sync::Notify::new()),
                foreground_wait: Arc::new(AtomicBool::new(false)),
                parent_rejected: AtomicBool::new(false),
                foreground_result: Arc::new(Mutex::new(None)),
            },
        );
        let mut first = Box::pin(jobs.shutdown());
        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(cancel.load(Ordering::Acquire));
        drop(first);
        let retained = jobs.work.lock().unwrap().len() == 1;
        let mut retry = Box::pin(jobs.shutdown());
        let premature = match futures_util::poll!(&mut retry) {
            Poll::Ready(result) => Some(result),
            Poll::Pending => None,
        };
        // Always release/join the exact fixture leaf before any failing assert.
        release.send(()).unwrap();
        assert_eq!(completion.await, good);
        let succeeded = match premature.as_ref() {
            Some(result) => result.is_ok(),
            None => retry.await.is_ok(),
        };
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        assert!(retained, "dropped shutdown discarded authoritative Work");
        assert!(
            premature.is_none(),
            "retry reported drain while owned join was held"
        );
        assert_eq!(succeeded, good);
        assert_eq!(
            jobs.shutdown().await.is_ok(),
            good,
            "join failure was not sticky"
        );
        assert!(jobs.work.lock().unwrap().is_empty());
    }
}

const STDIO: &str = r#"
import json, os, pathlib, sys
pathlib.Path('mcp.pid').write_text(str(os.getpid()))
for line in sys.stdin:
    message = json.loads(line)
    if 'id' not in message: continue
    method = message['method']
    result = {'protocolVersion':'2024-11-05', 'capabilities':{'tools':{}}, 'serverInfo':{'name':'owned-lease','version':'1'}} if method == 'initialize' else {'tools':[]} if method == 'tools/list' else {}
    print(json.dumps({'jsonrpc':'2.0','id':message['id'],'result':result}), flush=True)
"#;

#[tokio::test]
async fn dropped_retirement_reap_retains_mcp_stop_until_real_lease_and_leaf_join() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let jobs = Jobs::new(&db);
    let owner = Arc::new(mcp::McpOwner::new("/owned-source", 7));
    let mut entry: crate::config::McpEntry = serde_json::from_value(serde_json::json!({"type":"local", "command":["/usr/bin/python3","-u","-c", STDIO], "cwd":project.path().to_str().unwrap()})).unwrap();
    entry.resource_admitted = true;
    owner
        .start(
            Generation {
                mcp: BTreeMap::from([("owned-lease".into(), entry)]),
                ..Default::default()
            },
            project.path().into(),
            BTreeMap::new(),
            None,
        )
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while owner.snapshot().servers[0].status != oc_core::queries::McpStatus::Connected {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(project.path().join("mcp.pid"))
        .unwrap()
        .parse()
        .unwrap();
    println!(
        "owned retirement fixture data={} project={} leaf={pid}",
        data.path().display(),
        project.path().display()
    );
    let lease = owner.request_view().unwrap();
    jobs.retired.lock().unwrap().push(owner.clone());
    let mut first = Box::pin(jobs.reap());
    assert!(futures_util::poll!(&mut first).is_pending());
    drop(first);
    let retained = jobs.retired.lock().unwrap().len() == 1;
    let mut retry = Box::pin(jobs.reap());
    let premature = matches!(futures_util::poll!(&mut retry), Poll::Ready(_));
    drop(lease);
    // Fixture retains one Arc only to guarantee cleanup even on RED.
    if !premature {
        tokio::time::timeout(Duration::from_secs(5), retry)
            .await
            .unwrap()
            .unwrap();
    }
    owner.stop().await.unwrap();
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    assert!(retained, "interrupted reap discarded retired source owner");
    assert!(!premature, "second reap forgot the held retirement join");
    assert!(jobs.retired.lock().unwrap().is_empty());
}
