use super::*;
use oc_core::queries::{ChildJob, ChildState};

fn job(parent: &str, child: &str) -> ChildJob {
    ChildJob {
        parent: SessionId(parent.into()),
        child: SessionId(child.into()),
        operation: "navigation-launch".into(),
        generation: 7,
        location: "/fixture".into(),
        agent: "helper".into(),
        model: "fixture/model".into(),
        description: "held child".into(),
        delivery_id: "navigation-delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    }
}

#[tokio::test]
async fn pending_model_commit_refuses_close_and_delete_before_effects() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("model-pending".into()));
    let mut snapshot = catalog();
    snapshot.models.push(oc_core::queries::ModelEntry {
        id: snapshot.model_id.clone(),
        ..Default::default()
    });
    state.apply_catalog(snapshot.clone());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::SessionSelection { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("blank Enter commits selection")
    };
    assert!(state.has_pending_model_commit());
    assert!(!state.is_busy(), "model receipt is distinct from execution");
    let mut deck = LoopState {
        tabs: vec![
            None,
            Some(TuiState::new(app.clone(), SessionId("other".into()))),
        ],
        tab_cards_before: vec![None, None],
        active_tab: Some(0),
        ..Default::default()
    };
    let before = deck.snapshot(&state);
    assert!(deck.close_tab(&app, &mut state, 0).await.is_err());
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::DeleteSelectedSession {
                id: "model-pending".into()
            }
        )
        .await
        .is_err()
    );
    assert_eq!(deck.snapshot(&state), before);
    assert!(state.has_pending_model_commit());
    assert!(
        inbox.try_recv().is_err(),
        "no save/delete before receipt settlement"
    );
    ack.send(Ok(snapshot)).unwrap();
    state.poll_submission();
    assert!(!state.has_pending_model_commit());
    deck.close_tab(&app, &mut state, 0).await.unwrap();
    assert_eq!(state.session().0, "other");
    assert_eq!(deck.tabs.len(), 1);
}

#[tokio::test]
async fn typed_new_consumes_only_child_command_not_retained_root_draft() {
    for parked_home in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut root = TuiState::new(app.clone(), SessionId("root".into()));
        root.restore_prompt("private root draft".into());
        let mut state = TuiState::new(app.clone(), SessionId("child".into()));
        state.attach_linked_child(job("root", "child"));
        state.restore_prompt("/new".into());
        let mut deck = LoopState {
            tabs: vec![None],
            tab_cards_before: vec![None],
            active_tab: Some(0),
            read_only: true,
            child_parent: Some(Box::new(root)),
            home: parked_home.then(|| TuiState::new_home(app.clone())),
            ..Default::default()
        };
        let owner = tokio::spawn(async move {
            if !parked_home {
                let InboxMsg::HomeSelection { ack, .. } = inbox.recv().await.unwrap() else {
                    panic!("Home metadata only")
                };
                ack.send(Ok(catalog())).unwrap();
            }
            inbox
        });
        apply_intent_with_origin(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::NewSession,
            true,
            false,
        )
        .await
        .unwrap();
        inbox = owner.await.unwrap();
        assert!(state.home);
        assert_eq!(deck.tabs[0].as_ref().unwrap().input(), "private root draft");
        assert!(
            deck.child_views[&SessionId("child".into())]
                .input()
                .is_empty()
        );
        assert!(inbox.try_recv().is_err(), "no execution or repeated save");
    }
}

#[tokio::test]
async fn linked_same_root_activation_preserves_root_and_standalone_provenance() {
    for standalone in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut root = TuiState::new(app.clone(), SessionId("root".into()));
        root.restore_prompt("original root draft".into());
        root.begin_linked_turn(WorkerTurnId("root-turn".into()));
        if standalone {
            root.parent_id = Some("older-parent".into());
        }
        let mut state = TuiState::new(app, SessionId("child".into()));
        state.attach_linked_child(job("root", "child"));
        let mut deck = LoopState {
            tabs: vec![None],
            tab_cards_before: vec![None],
            active_tab: Some(0),
            read_only: true,
            child_parent_read_only: standalone,
            child_parent: Some(Box::new(root)),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        assert_eq!(state.root_navigation_allowed(), !standalone);
        assert_eq!(
            state
                .command_unavailable(&CommandAction::OpenSessions)
                .is_none(),
            !standalone
        );
        if standalone {
            assert!(deck.activate(&mut state, 0).is_err());
            child_controls::return_parent(&mut state, &mut deck);
            assert!(
                deck.read_only,
                "grandchild return cannot authorize standalone child"
            );
        } else {
            deck.activate(&mut state, 0).unwrap();
            assert!(!deck.read_only);
            assert!(deck.child_parent.is_none());
            assert!(deck.child_views.contains_key(&SessionId("child".into())));
        }
        assert_eq!(state.session().0, "root");
        assert_eq!(state.active_turn(), Some(&WorkerTurnId("root-turn".into())));
        assert_eq!(state.input(), "original root draft");
        assert!(
            inbox.try_recv().is_err(),
            "navigation has no execution effects"
        );
    }
}

#[tokio::test]
async fn parked_parent_discovers_running_child_before_tool_completion() {
    let (app, mut inbox, _) = CoreApp::channel(16);
    let launch = job("parent-A", "child-A");
    let mut parent = TuiState::new(app.clone(), launch.parent.clone());
    let turn = WorkerTurnId("parent-turn".into());
    parent.begin_linked_turn(turn.clone());
    parent.restore_prompt("parked A draft".into());
    parent.apply_tool_started(
        &turn,
        &launch.operation,
        "subagent",
        r#"{"agent":"helper","description":"held child"}"#,
    );
    let mut state = TuiState::new(app.clone(), SessionId("root-B".into()));
    state.restore_prompt("visible B draft".into());
    let mut deck = LoopState {
        tabs: vec![Some(parent), None],
        tab_cards_before: vec![None, None],
        active_tab: Some(1),
        ..Default::default()
    };
    let expected = launch.clone();
    let owner = tokio::spawn(async move {
        while let Some(message) = inbox.recv().await {
            match message {
                InboxMsg::ChildJobs { session, ack } => {
                    ack.send(Ok(if session == expected.parent {
                        vec![expected.clone()]
                    } else {
                        Vec::new()
                    }))
                    .unwrap();
                }
                InboxMsg::ReadChild { selected, ack, .. } => {
                    assert_eq!(selected, expected);
                    ack.send(Ok(HistoryPage {
                        child_job: Some(Box::new(expected.clone())),
                        ..Default::default()
                    }))
                    .unwrap();
                }
                InboxMsg::Catalog { ack } => {
                    ack.send(Ok(catalog())).unwrap();
                }
                _ => panic!("navigation must not execute"),
            }
        }
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &launch.parent,
        CoreEvent::TurnStarted {
            session: launch.child.clone(),
            turn: WorkerTurnId("child-turn".into()),
            model_switch: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "root-B");
    assert_eq!(state.input(), "visible B draft");
    assert!(state.transcript_rows().is_empty());
    let parent = deck.tabs[0].as_ref().unwrap();
    assert_eq!(parent.input(), "parked A draft");
    assert_eq!(
        parent.transcript_rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_deref(),
        Some(&launch)
    );
    assert!(deck.child_views.contains_key(&launch.child));
    owner.abort();
}
