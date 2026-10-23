use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{ChildJob, ChildState};

#[tokio::test]
async fn focused_terminal_eof_does_not_require_output_cursor_attachment() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId("root".into());
    let target = TerminalRef {
        id: "captured".into(),
        session: session.clone(),
        location: "/fixture".into(),
        generation: 7,
    };
    let mut state = TuiState::new(app.clone(), session.clone());
    state.chrome.session_terminal = true;
    state.chrome.location = Some("/fixture".into());
    state.chrome.terminal_generation = 7;
    state.apply_terminal_inventory(
        &session,
        oc_core::queries::TerminalInventory {
            enabled: true,
            entries: Vec::new(),
            selected: Some(target.clone()),
        },
    );
    state.focus_terminal(true);
    assert!(state.terminal_focused());
    assert!(!state.terminal_ready());
    let mut deck = LoopState::default();
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('d'),
            crossterm::event::KeyModifiers::CONTROL,
        )),
    )
    .await
    .unwrap();
    let InboxMsg::Terminal {
        action: TerminalAction::Input {
            target: sent,
            bytes,
        },
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("EOF must reach its selected PTY before output attaches")
    };
    assert_eq!(sent, target);
    assert_eq!(bytes, vec![4]);
    ack.send(Ok(TerminalReceipt::Applied)).unwrap();
    while !deck.terminals.ready() {
        tokio::task::yield_now().await;
    }
    sync(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn selected_terminal_eof_routes_once_before_its_prepared_screen() {
    for old_focused in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let session = SessionId("root".into());
        let target = TerminalRef {
            id: "captured".into(),
            session: session.clone(),
            location: "/fixture".into(),
            generation: 7,
        };
        let mut state = TuiState::new(app.clone(), session.clone());
        state.chrome.session_terminal = true;
        state.chrome.location = Some("/fixture".into());
        state.chrome.terminal_generation = 8;
        assert_ne!(target.generation, state.chrome.terminal_generation);
        if old_focused {
            state.apply_terminal_inventory(
                &session,
                oc_core::queries::TerminalInventory {
                    enabled: true,
                    entries: Vec::new(),
                    selected: Some(TerminalRef {
                        id: "older visible PTY".into(),
                        ..target.clone()
                    }),
                },
            );
            state.focus_terminal(true);
            assert!(state.terminal_focused());
        }
        let mut deck = LoopState::default();
        deck.sync_tabs(&mut state);
        apply(
            &app,
            &mut state,
            &mut deck,
            session.clone(),
            TerminalIntent::Select(target.clone()),
        )
        .await;
        let InboxMsg::Terminal {
            action: TerminalAction::Select(Some(selected)),
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("select")
        };
        assert_eq!(selected, target);
        ack.send(Ok(TerminalReceipt::Applied)).unwrap();
        while !deck.terminals.ready() {
            tokio::task::yield_now().await;
        }
        sync(&app, &mut state, &mut deck).await;
        let InboxMsg::Terminal {
            action: TerminalAction::List,
            ack: list,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("held screen")
        };
        if old_focused {
            let InboxMsg::Terminal {
                action: TerminalAction::Resize { target, .. },
                ack,
                ..
            } = inbox.recv().await.unwrap()
            else {
                panic!("one resize of the existing visible screen")
            };
            assert_eq!(target.id, "older visible PTY");
            ack.send(Ok(TerminalReceipt::Applied)).unwrap();
        }
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('d'),
                crossterm::event::KeyModifiers::CONTROL,
            )),
        )
        .await
        .unwrap();
        let InboxMsg::Terminal {
            session: sent,
            action:
                TerminalAction::Input {
                    target: sent_target,
                    bytes,
                },
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("captured EOF before screen")
        };
        assert_eq!(sent, session);
        assert_eq!(sent_target, target);
        assert_eq!(bytes, vec![4]);
        assert!(state.input().is_empty());
        ack.send(Ok(TerminalReceipt::Applied)).unwrap();
        list.send(Err(CoreError::Shutdown)).unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while deck.terminals.pending() {
                sync(&app, &mut state, &mut deck).await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "EOF cannot be replayed by screen completion"
        );
    }
}

#[tokio::test]
async fn terminal_selection_waits_for_prepared_screen_and_cannot_steal_later_composer_focus() {
    use oc_core::queries::{
        TerminalEntry, TerminalInventory, TerminalReplay, TerminalSize, TerminalSnapshot,
        TerminalState,
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId("root".into());
    let target = TerminalRef {
        id: "original-terminal".into(),
        session: session.clone(),
        location: "/fixture".into(),
        generation: 7,
    };
    let entry = TerminalEntry {
        target: target.clone(),
        shell: "fixture".into(),
        cwd: "/fixture".into(),
        pid: 1234,
        title: "Fixture terminal".into(),
        foreground: None,
        state: TerminalState::Running,
        exit: None,
    };
    let screen = TerminalSnapshot {
        entry: entry.clone(),
        size: TerminalSize::default(),
        cells: Vec::new(),
        cursor: (0, 0),
        hide_cursor: false,
        application_cursor: false,
        bracketed_paste: false,
        output_cursor: 0,
        revision: 1,
        ready: true,
        control_strings_dropped: 0,
    };
    let inventory = TerminalInventory {
        enabled: true,
        entries: vec![entry],
        selected: Some(target.clone()),
    };
    let mut state = TuiState::new(app.clone(), session.clone());
    state.chrome.session_terminal = true;
    state.chrome.location = Some("/fixture".into());
    state.chrome.terminal_generation = 7;
    state.apply_terminal_inventory(&session, inventory.clone());
    state.apply_terminal_snapshot(screen.clone());
    state.focus_terminal(false);
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::Terminal {
            session: session.clone(),
            action: TerminalIntent::Select(target.clone()),
            close_composer: false,
        },
    )
    .await
    .unwrap();
    let InboxMsg::Terminal {
        action: TerminalAction::Select(Some(selected)),
        ack,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("captured selection")
    };
    assert_eq!(selected, target);
    ack.send(Ok(TerminalReceipt::Applied)).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while !deck.terminals.ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    sync(&app, &mut state, &mut deck).await;
    let InboxMsg::Terminal {
        action: TerminalAction::List,
        ack: list,
        ..
    } = inbox.recv().await.unwrap()
    else {
        panic!("prepared inventory")
    };
    assert!(
        !state.terminal_focused(),
        "mutation ACK is not a ready screen"
    );
    tokio::time::timeout(Duration::from_millis(100), async {
        handle_event(&app, &mut state, &mut deck, CEvent::Resize(120, 40))
            .await
            .unwrap();
        handle_event(&app, &mut state, &mut deck, CEvent::Resize(100, 32))
            .await
            .unwrap();
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('Ж'),
                crossterm::event::KeyModifiers::NONE,
            )),
        )
        .await
        .unwrap();
        assert!(
            oc_tui::views::render_test(&state, 100, 32)
                .iter()
                .any(|line| line.contains('Ж'))
        );
    })
    .await
    .unwrap();
    list.send(Ok(TerminalReceipt::Inventory(inventory)))
        .unwrap();
    let final_size = state.terminal_size_for_frame(ratatui::layout::Rect::new(0, 0, 100, 32));
    let expected = target.clone();
    let owner = tokio::spawn(async move {
        let mut replayed = false;
        let mut resized = Vec::new();
        for _ in 0..3 {
            let InboxMsg::Terminal {
                session: requested,
                action,
                ack,
            } = inbox.recv().await.unwrap()
            else {
                panic!("terminal read/resize only")
            };
            assert_eq!(requested, session);
            match action {
                TerminalAction::Replay { target, cursor: 0 } => {
                    assert_eq!(target, expected);
                    assert!(!replayed);
                    replayed = true;
                    ack.send(Ok(TerminalReceipt::Replay(TerminalReplay {
                        from: 0,
                        next: 0,
                        bytes: Vec::new(),
                        reset: None,
                        screen: Box::new(screen.clone()),
                    })))
                    .unwrap();
                }
                TerminalAction::Resize { target, size } => {
                    assert_eq!(target, expected);
                    resized.push(size);
                    ack.send(Ok(TerminalReceipt::Applied)).unwrap();
                }
                _ => panic!("no input dispatch or duplicate terminal effect"),
            }
        }
        assert!(replayed);
        assert_eq!(
            resized.len(),
            2,
            "one already-sent size and one coalesced latest size"
        );
        assert_ne!(resized[0], resized[1]);
        assert_eq!(resized[1], final_size);
        inbox
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.terminals.pending() {
            sync(&app, &mut state, &mut deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(state.input(), "Ж");
    assert!(
        !state.terminal_focused(),
        "late read cannot reclaim focus after new typing"
    );
    let mut inbox = owner.await.unwrap();
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn term01_child_composer_closes_to_parent_before_captured_create_error() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut parent = TuiState::new(app.clone(), SessionId("parent".into()));
    parent.restore_prompt("parked parent draft".into());
    parent.chrome.location = Some("/parent".into());
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    state.chrome.session_terminal = true;
    state.chrome.terminal_generation = 19;
    state.attach_linked_child(ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "child-operation".into(),
        generation: 7,
        location: "/child".into(),
        agent: "build".into(),
        model: "fixture/model".into(),
        description: "child".into(),
        delivery_id: "delivery".into(),
        state: ChildState::Running,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    });
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        read_only: true,
        ..Default::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::ReadChild {
            session,
            selected,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("exact launch before terminal access")
        };
        assert_eq!(session.0, "parent");
        assert_eq!(selected.child.0, "child");
        assert_eq!(selected.operation, "child-operation");
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::Terminal {
            session,
            action,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured terminal create");
        };
        assert_eq!(session, SessionId("child".into()));
        assert!(
            matches!(action,TerminalAction::CreateChild { source,generation:19,.. }
                if source.child.0 == "child" && source.parent.0 == "parent"
                    && source.location == "/child" && source.generation == 7
                    && source.operation == "child-operation")
        );
        ack.send(Err(CoreError::Application(
            "captured admission refused".into(),
        )))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    state.handle_key(KeyAction::TerminalSelect).await;
    state.handle_key(KeyAction::Up).await;
    let captured = state.handle_key(KeyAction::Enter).await.intent.unwrap();
    assert!(!state.terminals_open());
    apply_intent(&app, &mut state, &mut deck, captured)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.terminals.pending() {
            sync(&app, &mut state, &mut deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    worker.await.unwrap();
    assert_eq!(state.session(), &SessionId("parent".into()));
    assert_eq!(state.input(), "parked parent draft");
    assert!(deck.child_views.contains_key(&SessionId("child".into())));
    assert!(!deck.read_only);
    assert_eq!(
        state.note(),
        None,
        "late child error cannot target the mounted parent"
    );
    assert_eq!(
        deck.child_views[&SessionId("child".into())].note(),
        Some("Unable to load terminal")
    );
    assert!(
        deck.child_views[&SessionId("child".into())]
            .chrome
            .session_terminal
    );
}
