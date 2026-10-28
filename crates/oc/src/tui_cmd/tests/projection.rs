use super::*;
use oc_core::queries::{LiveTurnSnapshot, TabDeckSnapshot, WorkspaceSnapshot};

#[test]
fn parked_projection_close_retains_and_polls_the_owned_join_before_removal() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap()
        .block_on(async {
            let (app, _, _) = CoreApp::channel(8);
            let mut state = TuiState::new(app.clone(), SessionId("active".into()));
            state.restore_prompt("still editable".into());
            let mut deck = LoopState {
                tabs: vec![None, Some(TuiState::new(app, SessionId("parked".into())))],
                active_tab: Some(0),
                ..Default::default()
            };
            // Occupy the sole blocking thread so a real cold projection is
            // deterministically queued. The sender drops before `deck` on an
            // assertion failure, so cleanup cannot deadlock on this fixture.
            let (release, held) = std::sync::mpsc::channel();
            let (entered, started) = tokio::sync::oneshot::channel();
            let blocker = tokio::task::spawn_blocking(move || {
                let _ = entered.send(());
                let _ = held.recv();
            });
            started.await.unwrap();
            let mut page = tps_page();
            page.rows[0].role = Role::User;
            page.rows[0].turn = None;
            page.rows[0].text = "cold source ".repeat(16_384);
            let parked = deck.tabs[1].as_mut().unwrap();
            parked.attach_page(&page);
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 30)).unwrap();
            terminal
                .draw(|frame| oc_tui::shell::render(frame, parked))
                .unwrap();
            assert!(parked.has_pending_transcript_projection());
            assert!(!deck_jobs::projection_drained(&state, &deck, 1));
            assert!(
                deck.tabs[1]
                    .as_ref()
                    .unwrap()
                    .has_pending_transcript_projection()
            );
            assert_eq!(state.input(), "still editable");
            assert!(deck.tabs[1].as_ref().unwrap().next_ui_deadline().is_some());
            release.send(()).unwrap();
            blocker.await.unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                while deck.tabs[1]
                    .as_ref()
                    .unwrap()
                    .has_pending_transcript_projection()
                {
                    for view in read_jobs::views(&state, &deck) {
                        view.poll_transcript_projection();
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert!(deck_jobs::projection_drained(&state, &deck, 1));
            assert!(deck.tabs[1].as_ref().unwrap().next_ui_deadline().is_none());
            assert_eq!(state.input(), "still editable");
            deck.tabs[1].take();
        });
}

fn live(session: &str, turn: &str, text: &str) -> LiveTurnSnapshot {
    let id = SessionId(session.into());
    let turn = WorkerTurnId(turn.into());
    LiveTurnSnapshot {
        session: id.clone(),
        turn: turn.clone(),
        origin: Some(1),
        events: vec![
            CoreEvent::TurnStarted {
                session: id.clone(),
                turn: turn.clone(),
                model_switch: None,
            },
            CoreEvent::TextDelta {
                session: id,
                turn,
                delta: text.into(),
            },
        ],
        display_name: None,
        truncated: false,
    }
}

fn workspace(selected: &str, turns: Vec<LiveTurnSnapshot>) -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        activity: Vec::new(),
        selected_root: None,
        inbox: Vec::new(),
        revision: 10,
        deck: TabDeckSnapshot {
            location: "/fixture".into(),
            sessions: vec![SessionId("root".into())],
            active: Some(SessionId(selected.into())),
            ..Default::default()
        },
        turns,
    }
}

#[tokio::test]
async fn late_attach_keeps_finished_predecessor_and_filters_only_its_destination() {
    let (app, mut inbox, _controls, _, _) = CoreApp::native_channel(8, 1);
    let root = SessionId("root".into());
    let mut state = TuiState::new(app.clone(), root.clone());
    state.chrome.location = Some("/fixture".into());
    state.restore_prompt("private draft".into());
    state.begin_linked_turn(WorkerTurnId("T1".into()));
    state.apply_delta(&WorkerTurnId("T1".into()), "superseded transient");
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::Workspace { selected, ack }) = inbox.recv().await else {
            panic!("target workspace first")
        };
        assert_eq!(selected, Some(SessionId("root".into())));
        ack.send(Ok(workspace(
            "root",
            vec![live("root", "T2", "new prefix")],
        )))
        .unwrap();
        let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
            panic!("canonical state after the snapshot")
        };
        assert_eq!(session.0, "root");
        let mut page = tps_page();
        page.rows[0].text = "completed predecessor".into();
        page.rows[0].turn.as_mut().unwrap().id = "T1".into();
        page.rows[0].turn.as_mut().unwrap().parts = vec![oc_core::queries::TranscriptPart::Text(
            "completed predecessor".into(),
        )];
        ack.send(Ok(page)).unwrap();
        empty_compactions(&mut inbox).await;
        empty_mcp_status(&mut inbox).await;
        empty_shell_inventory(&mut inbox, "root").await;
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("bounded target inventory")
        };
        assert_eq!(session.0, "root");
        ack.send(Ok(Vec::new())).unwrap();
        inbox
    });
    shared::attach(&app, &mut state).await.unwrap();
    let mut inbox = worker.await.unwrap();
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("T2".into())));
    assert_eq!(state.projection_floor(), 10);
    let shown = state.viewport().join("\n");
    assert!(shown.contains("completed predecessor"), "{shown}");
    assert_eq!(shown.matches("new prefix").count(), 1, "{shown}");
    assert!(!shown.contains("superseded transient"));
    let mut other = TuiState::new(app.clone(), SessionId("other".into()));
    other.begin_linked_turn(WorkerTurnId("S".into()));
    let mut deck = LoopState {
        tabs: vec![None, Some(other)],
        tab_cards_before: vec![None, None],
        active_tab: Some(0),
        projection_revision: 4,
        ..Default::default()
    };
    for (revision, event) in [
        (5, live("root", "T1", "old").events.remove(0)),
        (9, live("root", "T2", "new prefix").events.remove(1)),
        (6, live("other", "S", "unrelated suffix").events.remove(1)),
        (11, live("root", "T2", " + tail").events.remove(1)),
    ] {
        handle_worker_event_at(
            &app,
            &mut state,
            &mut deck,
            &root,
            event,
            Some(revision),
            false,
        )
        .await
        .unwrap();
    }
    assert_eq!(state.active_turn(), Some(&WorkerTurnId("T2".into())));
    let shown = state.viewport().join("\n");
    assert_eq!(shown.matches("new prefix").count(), 1, "{shown}");
    assert!(shown.contains("+ tail"), "{shown}");
    assert!(
        deck.tabs[1]
            .as_ref()
            .unwrap()
            .viewport()
            .join("\n")
            .contains("unrelated suffix")
    );
    assert_eq!(state.input(), "private draft");
    assert_eq!(
        deck.projection_revision, 4,
        "a target snapshot is not a consumed stream cursor"
    );
    assert!(
        inbox.try_recv().is_err(),
        "covered lifecycle never rereads or executes work"
    );
}

#[tokio::test]
async fn linked_gap_stages_all_reads_and_repairs_parent_selected_and_retained_child() {
    use oc_core::queries::{ChildJob, ChildState};
    let (app, mut inbox, _controls, _, _) = CoreApp::native_channel(8, 1);
    let job = |id: &str| ChildJob {
        parent: SessionId("root".into()),
        child: SessionId(id.into()),
        operation: format!("launch-{id}"),
        generation: 1,
        location: "/fixture".into(),
        agent: "explore".into(),
        model: "fixture/model".into(),
        description: id.into(),
        delivery_id: format!("delivery-{id}"),
        state: ChildState::Running,
        background: true,
        turn: Some(format!("turn-{id}")),
        result: None,
        message_id: None,
    };
    let a = job("A");
    let mut b = job("B");
    let make = |id: &str, linked: Option<ChildJob>| {
        let mut view = TuiState::new(app.clone(), SessionId(id.into()));
        view.chrome.location = Some("/fixture".into());
        view.restore_prompt(format!("private {id}"));
        if let Some(job) = linked {
            view.attach_linked_child(job);
        }
        view.begin_linked_turn(WorkerTurnId(format!("turn-{id}")));
        view.apply_delta(&WorkerTurnId(format!("turn-{id}")), "old transient");
        view
    };
    let mut state = make("A", Some(a.clone()));
    let parent = make("root", None);
    let retained = make("B", Some(b.clone()));
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        child_views: [(b.child.clone(), retained)].into(),
        read_only: true,
        shared_gap: true,
        shared_refresh: true,
        location: Some("/fixture".into()),
        projection_revision: 2,
        ..Default::default()
    };
    b.state = ChildState::Completed;
    let mut historical_job = b.clone();
    historical_job.location = "/original-source".into();
    let worker = tokio::spawn(async move {
        let mut fail = true;
        let mut reads = 0;
        while let Some(message) = inbox.recv().await {
            reads += 1;
            assert!(reads < 64, "repair remains bounded by existing views");
            match message {
                InboxMsg::Workspace { selected, ack } => {
                    let mut turns = vec![live("root", "turn-root", "root prefix")];
                    if selected.as_ref() == Some(&a.child) {
                        turns.push(live("A", "turn-A", "child prefix"));
                    }
                    ack.send(Ok(workspace("root", turns))).unwrap();
                }
                InboxMsg::ReadChild {
                    session,
                    selected,
                    ack,
                    ..
                } => {
                    assert_eq!(session.0, "root");
                    assert!(selected == a || selected.operation == b.operation);
                    if selected.child == b.child && fail {
                        fail = false;
                        ack.send(Err(CoreError::Application("read failed".into())))
                            .unwrap();
                        continue;
                    }
                    let mut page = oc_core::queries::HistoryPage {
                        child_job: Some(Box::new(if selected.child == b.child {
                            b.clone()
                        } else {
                            a.clone()
                        })),
                        parent_id: Some("root".into()),
                        ..Default::default()
                    };
                    if selected.child == b.child {
                        page.rows = tps_page().rows;
                        page.total = 1;
                        page.rows[0].text = "final B".into();
                        page.rows[0].turn.as_mut().unwrap().id = "turn-B".into();
                        page.rows[0].turn.as_mut().unwrap().parts =
                            vec![oc_core::queries::TranscriptPart::Text("final B".into())];
                    }
                    ack.send(Ok(page)).unwrap();
                }
                InboxMsg::History { session, ack, .. } => {
                    assert_eq!(session.0, "root");
                    ack.send(Ok(Default::default())).unwrap();
                }
                InboxMsg::ChildJobs { session, ack } => {
                    ack.send(Ok(if session.0 == "root" {
                        vec![a.clone(), b.clone()]
                    } else {
                        Vec::new()
                    }))
                    .unwrap();
                }
                InboxMsg::PendingApprovals { ack } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::PendingQuestions { ack } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::CompactionHistory { ack, .. } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::ShellJobs { ack, .. } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::ExecuteJobs { ack, .. } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::McpStatus { ack } => {
                    ack.send(Ok(oc_core::queries::McpSnapshot {
                        binding: oc_core::queries::McpBinding {
                            location: "/fixture".into(),
                            generation: 1,
                            instance: 1,
                        },
                        revision: 1,
                        servers: Vec::new(),
                    }))
                    .unwrap();
                }
                _ => panic!("repair must not save decks, submit or control execution"),
            }
        }
    });
    assert!(!shared::sync(&app, &mut state, &mut deck).await.unwrap());
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.workspace_job.as_ref().is_some_and(shared::Job::ready) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(shared::sync(&app, &mut state, &mut deck).await.unwrap());
    assert!(
        state
            .note()
            .is_some_and(|note| note.contains("read failed"))
    );
    assert!(deck.shared_gap && deck.read_only);
    assert_eq!(state.projection_floor(), 0);
    assert_eq!(deck.child_parent.as_ref().unwrap().projection_floor(), 0);
    assert!(state.viewport().join("\n").contains("old transient"));
    assert!(deck.child_views[&SessionId("B".into())].is_busy());
    // A later refresh retries the read, never a partially applied projection.
    deck.shared_refresh = true;
    settle_workspace(&app, &mut state, &mut deck).await;
    assert!(!deck.shared_gap && !deck.shared_refresh);
    assert!(deck.read_only);
    assert_eq!(state.session().0, "A");
    assert_eq!(state.input(), "private A");
    assert!(state.viewport().join("\n").contains("child prefix"));
    let parent = deck.child_parent.as_ref().unwrap();
    assert_eq!(parent.input(), "private root");
    assert!(parent.viewport().join("\n").contains("root prefix"));
    let retained = &deck.child_views[&SessionId("B".into())];
    assert!(!retained.is_busy());
    assert_eq!(retained.input(), "private B");
    assert!(retained.viewport().join("\n").contains("final B"));
    assert_eq!(state.projection_floor(), 10);
    assert_eq!(parent.projection_floor(), 10);
    assert_eq!(retained.projection_floor(), 10);
    assert_eq!(deck.projection_revision, 2);
    // An original Source remains a valid exact historical link after its root
    // moves. A current-Location snapshot cannot declare that child idle/live.
    let mut historical = TuiState::new(app.clone(), historical_job.child.clone());
    historical.chrome.location = Some(historical_job.location.clone());
    historical.attach_linked_child(historical_job.clone());
    historical.restore_prompt("historical draft".into());
    shared::attach(&app, &mut historical).await.unwrap();
    assert_eq!(historical.projection_floor(), 0);
    assert_eq!(historical.linked_child(), Some(&historical_job));
    let mut isolated = LoopState::default();
    handle_worker_event_at(
        &app,
        &mut historical,
        &mut isolated,
        &historical_job.child,
        live("B", "turn-B", "old prefix").events.remove(0),
        Some(3),
        false,
    )
    .await
    .unwrap();
    assert!(
        !historical.is_busy(),
        "queued start cannot revive a fenced terminal launch"
    );
    assert_eq!(historical.input(), "historical draft");
    worker.abort();
    let _ = worker.await;
}
