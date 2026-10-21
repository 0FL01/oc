use super::*;

#[tokio::test]
async fn idle_shell_report_wakes_once_with_current_parent_selection() {
    episode(Case::Idle).await;
}

#[tokio::test]
async fn converted_shell_keeps_original_parent_wake() {
    episode(Case::Converted).await;
}

#[tokio::test]
async fn idle_session_cancel_suppresses_completed_shell_handoff() {
    episode(Case::SessionCancel).await;
}

#[tokio::test]
async fn origin_release_suppresses_completed_shell_handoff() {
    episode(Case::OriginCancel).await;
}

#[tokio::test]
async fn busy_parent_consumes_shell_report_without_an_extra_root() {
    episode(Case::Busy).await;
}

#[tokio::test]
async fn active_undo_holds_shell_report_until_redo_without_replaying_shell() {
    episode(Case::UndoRedo).await;
}

#[tokio::test]
async fn undo_new_prompt_discards_already_queued_shell_report() {
    episode(Case::UndoCommit).await;
}

#[tokio::test]
async fn undo_new_prompt_keeps_shell_that_completes_after_branch_commit() {
    episode(Case::UndoLate).await;
}

#[tokio::test]
async fn worker_shutdown_still_suppresses_detached_shell_handoff() {
    episode(Case::WorkerShutdown).await;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Case {
    Idle,
    Converted,
    SessionCancel,
    OriginCancel,
    Busy,
    UndoRedo,
    UndoCommit,
    UndoLate,
    WorkerShutdown,
}

async fn episode(case: Case) {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("report"), "SHELL_REPORT_SENTINEL").unwrap();
    std::fs::write(project.join("note"), "independent work").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut source = config(&format!("http://{}/v1", listener.local_addr().unwrap()));
    source["agents"]["alpha"] = json!({"system":"ORIGINAL_PARENT_PROFILE"});
    source["agents"]["beta"] =
        json!({"system":"CURRENT_PARENT_PROFILE","model":"fixture/beta-model"});
    source["default_agent"] = json!("alpha");
    source["providers"]["fixture"]["models"]["beta-model"] =
        json!({"limit":{"context":100000,"output":2048}});
    source["permissions"] = json!([
        {"action":"shell","resource":"*","effect":"allow"},
        {"action":"read","resource":"*","effect":"allow"}
    ]);
    std::fs::write(project.join("opencode.json"), source.to_string()).unwrap();
    let continue_busy = Arc::new(tokio::sync::Notify::new());
    let posts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (entered, mut requests) = mpsc::channel(4);
    let peer = {
        let continue_busy = continue_busy.clone();
        let posts = posts.clone();
        tokio::spawn(async move {
            let mut held = Vec::new();
            for index in 0..4 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let body = request(&mut socket).await;
                posts.fetch_add(1, Ordering::AcqRel);
                entered.send(body).await.unwrap();
                if index == 1 && matches!(case, Case::UndoRedo | Case::WorkerShutdown) {
                    held.push(socket);
                    continue;
                }
                let output = if index == 0 {
                    json!([{"type":"function_call","id":"shell-item","call_id":"shell-launch","name":"shell","arguments":json!({
                        "command":"touch entered; while [ ! -f release ]; do sleep .01; done; printf x >> effects; cat report",
                        "background":case != Case::Converted
                    }).to_string()}])
                } else if index == 1 && case == Case::Busy {
                    continue_busy.notified().await;
                    json!([{"type":"function_call","id":"read-item","call_id":"read-call","name":"read","arguments":"{\"path\":\"note\"}"}])
                } else {
                    json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"PARENT_SETTLED"}]}])
                };
                respond(&mut socket, output).await;
            }
        })
    };
    let owner = Owner::new(
        Db::open(&root.path().join("data")).unwrap(),
        env(root.path()),
    )
    .unwrap();
    let location = owner.location(&project, false).await.unwrap();
    let session = SessionId("parent".into());
    location.app.create_session(session.clone()).await.unwrap();
    location
        .app
        .rename_session(session.clone(), "Named parent".into())
        .await
        .unwrap();
    let origin = Arc::new(Origin::new(Kind::Run, false).at(location.clone()));
    let running = owner
        .begin(
            location.clone(),
            session.0.clone(),
            origin.clone(),
            0,
            false,
            None,
        )
        .unwrap();
    let mut events = owner.events.subscribe();
    let first = running
        .app
        .submit(session.clone(), "ORIGINAL_PARENT_TASK".into())
        .await
        .unwrap();
    let completed = tokio::time::timeout(Duration::from_secs(5), async {
        while !project.join("entered").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let job = owner.resources.shell.running("parent").unwrap().remove(0);
        if case == Case::Converted {
            assert!(!job.background);
            assert!(
                owner
                    .resources
                    .shell
                    .background("parent", &job.shell_id)
                    .unwrap()
            );
        }
        let mut seen = vec![
            requests.recv().await.unwrap(),
            requests.recv().await.unwrap(),
        ];
        let mut automatic = None;
        if case == Case::UndoRedo {
            let undone = running
                .app
                .change_conversation(session.clone(), ConversationAction::Undo)
                .await
                .unwrap();
            assert!(undone.can_redo);
            assert_eq!(undone.draft.as_deref(), Some("ORIGINAL_PARENT_TASK"));
            assert!(origin.foreground.load(Ordering::Acquire));
        } else if case == Case::WorkerShutdown {
            running.app.shutdown().await.unwrap();
        }
        if case != Case::Busy {
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                if match event {
                    CoreEvent::TurnInterrupted { turn, .. }
                        if matches!(case, Case::UndoRedo | Case::WorkerShutdown) =>
                    {
                        turn == first
                    }
                    CoreEvent::TurnFinished { turn, .. }
                        if !matches!(case, Case::UndoRedo | Case::WorkerShutdown) =>
                    {
                        turn == first
                    }
                    _ => false,
                } {
                    break;
                }
            }
            owner.next_root().await.unwrap();
            assert!(
                owner.root("parent").is_none(),
                "must be idle before shell completes"
            );
            assert_eq!(
                owner.resources.shell.running("parent").unwrap()[0].shell_id,
                job.shell_id
            );
            if matches!(case, Case::UndoCommit | Case::UndoLate) {
                let blocked = owner.reserve_mutation("parent").unwrap();
                location
                    .app
                    .change_conversation(session.clone(), ConversationAction::Undo)
                    .await
                    .unwrap();
                drop(blocked);
            }
            location
                .app
                .session_selection(
                    session.clone(),
                    false,
                    SessionSelectionAction::Agent("beta".into()),
                )
                .await
                .unwrap();
        } else {
            assert!(owner.root("parent").is_some());
        }
        let mut blocked = (case != Case::Busy).then(|| owner.reserve_mutation("parent").unwrap());
        if case == Case::UndoLate {
            drop(blocked.take());
            seen.push(
                replace_parent(&owner, &location, &session, &mut events, &mut requests).await,
            );
            blocked = Some(owner.reserve_mutation("parent").unwrap());
        }
        std::fs::write(project.join("release"), "").unwrap();
        let notice = loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if let CoreEvent::ShellNotice(notice) = event {
                break notice;
            }
        };
        assert_eq!(notice.shell_id, job.shell_id);
        if case == Case::Busy {
            continue_busy.notify_one();
        } else if matches!(case, Case::UndoRedo | Case::UndoCommit) {
            assert!(owner.resources.shell.parent_batches().unwrap().is_empty());
            drop(blocked.take());
            owner.reconcile_parent_notices().unwrap();
            assert!(
                owner.root("parent").is_none(),
                "staged Undo must not start or refuse a continuation"
            );
            assert_eq!(posts.load(Ordering::Acquire), 2);
            if case == Case::UndoRedo {
                blocked = Some(owner.reserve_mutation("parent").unwrap());
                let redo = location
                    .app
                    .change_conversation(session.clone(), ConversationAction::Redo)
                    .await
                    .unwrap();
                assert!(!redo.can_redo && redo.reverted.is_none());
            } else {
                seen.push(
                    replace_parent(&owner, &location, &session, &mut events, &mut requests).await,
                );
                owner.resources.shell.parent_batches().unwrap();
            }
        } else if case == Case::WorkerShutdown {
            assert!(owner.resources.shell.parent_batches().unwrap().is_empty());
        } else {
            // A reaped handoff, not a running process, must keep finite-run
            // settlement open until admission or explicit suppression.
            while owner.resources.shell.parent_batches().unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            let settle = origin.settle();
            tokio::pin!(settle);
            assert!(futures_util::poll!(&mut settle).is_pending());
            match case {
                Case::SessionCancel => owner.cancel_session(&location, "parent", None).unwrap(),
                Case::OriginCancel => owner.cancel_origin(&origin),
                _ => {}
            }
        }
        drop(blocked);
        let suppressed = matches!(
            case,
            Case::SessionCancel | Case::OriginCancel | Case::UndoCommit | Case::WorkerShutdown
        );
        if !suppressed {
            seen.push(requests.recv().await.unwrap());
            loop {
                let event = match events.recv().await.unwrap() {
                    CoreEvent::Projection { event, .. } => *event,
                    event => event,
                };
                if let CoreEvent::TurnFinished { turn, .. } = event
                    && (case == Case::Busy || turn != first)
                {
                    automatic = Some(turn);
                    break;
                }
            }
            owner.next_root().await.unwrap();
        }
        origin.settle().await;
        assert!(owner.root("parent").is_none());
        (seen, notice, automatic)
    })
    .await;
    owner.shutdown().await.unwrap();
    peer.abort();
    let _ = peer.await;
    let (seen, notice, automatic) = completed.unwrap_or_else(|error| {
        panic!("{case:?}: {error}; posts={}", posts.load(Ordering::Acquire))
    });
    while let Ok(event) = events.try_recv() {
        let event = match event {
            CoreEvent::Projection { event, .. } => *event,
            event => event,
        };
        assert!(!matches!(
            event,
            CoreEvent::ParentContinuationRefused { .. }
        ));
    }
    let suppressed = matches!(
        case,
        Case::SessionCancel | Case::OriginCancel | Case::UndoCommit | Case::WorkerShutdown
    );
    assert_eq!(
        posts.load(Ordering::Acquire),
        match case {
            Case::UndoCommit => 3,
            Case::UndoLate => 4,
            _ if suppressed => 2,
            _ => 3,
        }
    );
    assert_eq!(
        std::fs::read(project.join("effects")).unwrap(),
        b"x",
        "no shell replay"
    );
    assert_eq!(
        owner
            .db
            .parent_notice_assigned("parent", &notice.message_id)
            .unwrap(),
        !suppressed
    );
    let history = owner.db.read_history_full("parent").unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|(_, role, text)| role == "user" && text.is_empty())
            .count(),
        usize::from(!suppressed && case != Case::Busy)
    );
    assert!(
        owner
            .db
            .change_conversation(
                "parent",
                oc_core::queries::ConversationAction::Revert {
                    message: oc_core::session::MessageId(notice.message_id.clone())
                }
            )
            .is_err(),
        "native data is not a human prompt boundary"
    );
    if suppressed {
        if case == Case::UndoCommit {
            let input = seen.last().unwrap()["input"].to_string();
            assert!(input.contains("REPLACEMENT_TASK"));
            assert!(
                !input.contains("SHELL_REPORT_SENTINEL") && !input.contains("ORIGINAL_PARENT_TASK")
            );
        }
        return;
    }
    let request = seen.last().unwrap();
    let input = request["input"].to_string();
    assert_eq!(input.matches("SHELL_REPORT_SENTINEL").count(), 1, "{input}");
    assert!(input.contains(if case == Case::UndoLate {
        "REPLACEMENT_TASK"
    } else {
        "ORIGINAL_PARENT_TASK"
    }));
    if case == Case::UndoLate {
        assert!(!input.contains("ORIGINAL_PARENT_TASK"));
    }
    if case != Case::Busy {
        assert_eq!(request["model"], "beta-model");
        assert!(input.contains("CURRENT_PARENT_PROFILE"));
        let (_, result) = owner.db.turn_result(&automatic.unwrap().0).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(saved["native_input"]["kind"], "shell_notices");
        assert_eq!(
            saved["native_input"]["messages"],
            json!([notice.message_id])
        );
    }
}
