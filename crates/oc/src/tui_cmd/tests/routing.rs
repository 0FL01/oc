use super::*;

#[tokio::test]
async fn sidebar_mcp_preference_ack_preserves_drafts_and_projects_on_activation() {
    use oc_core::queries::{McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let inventory = McpSnapshot {
        binding: McpBinding {
            location: "/fold-location".into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: (0..3)
            .map(|index| McpServerSnapshot {
                id: index.to_string(),
                name: format!("server-{index}"),
                configured_enabled: false,
                status: McpStatus::Disabled,
                pending_action: None,
                tools: 0,
                diagnostic: None,
                actions: Vec::new(),
            })
            .collect(),
    };
    let mut state = TuiState::new(app.clone(), SessionId("sidebar-fold-root".into()));
    state.chrome.location = Some("/fold-location".into());
    state.apply_mcp_snapshot(inventory.clone());
    state.restore_prompt("preserved fold draft Ω界".into());
    let caret = state.prompt_layout(100).1;
    let mut deck = LoopState {
        sidebar_mcp_open: Some(true),
        ..Default::default()
    };
    let paint = |view: &TuiState| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(160, 32)).unwrap();
        terminal
            .draw(|frame| oc_tui::shell::render(frame, view))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    for success in [false, true] {
        let owner = async {
            let Some(InboxMsg::SidebarMcpOpen { open, ack }) = inbox.recv().await else {
                panic!("one explicit fold save")
            };
            assert_eq!(open, Some(false));
            ack.send(if success {
                Ok(false)
            } else {
                Err(CoreError::Application("safe storage failure".into()))
            })
            .unwrap();
        };
        let (result, ()) = tokio::join!(
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::SetSidebarMcpOpen { open: false }
            ),
            owner
        );
        assert_eq!(result.is_ok(), success);
        assert_eq!(deck.sidebar_mcp_open, Some(!success));
        assert!(paint(&state).contains(if success { "▶ MCP" } else { "▼ MCP" }));
        assert_eq!(state.input(), "preserved fold draft Ω界");
        assert_eq!(state.prompt_layout(100).1, caret);
        assert!(!state.has_pending_submission());
        assert!(inbox.try_recv().is_err());
    }
    // No eager fanout is needed: the same confirmed bit is projected on activation,
    // including a child/history view and a freshly constructed view after reload.
    for child in [false, true] {
        let mut activated = TuiState::new(app.clone(), SessionId("sidebar-fold-restored".into()));
        activated.chrome.location = Some("/fold-location".into());
        activated.apply_mcp_snapshot(inventory.clone());
        activated.restore_prompt("parked draft".into());
        deck.child_parent =
            child.then(|| Box::new(TuiState::new(app.clone(), SessionId("parent".into()))));
        deck.sync_tabs(&mut activated);
        assert!(paint(&activated).contains("▶ MCP"));
        assert_eq!(activated.input(), "parked draft");
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn parked_root_events_keep_exact_view_and_restore_it_after_read_failure() {
    use oc_core::tool_output::Presentation;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let root = SessionId("parked-root-owner".into());
    let other = SessionId("unrelated-root".into());
    let turn = WorkerTurnId("accepted-parked-root-turn".into());
    let mut state = TuiState::new(app.clone(), root.clone());
    state.restore_prompt("accepted root prompt".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("one genuine root admission")
    };
    ack.send(Ok(turn.clone())).unwrap();
    state.poll_submission();
    assert_eq!(state.active_turn(), Some(&turn));
    state.restore_prompt("unfinished root draft".into());
    let mut unrelated = TuiState::new(app.clone(), other.clone());
    unrelated.restore_prompt("unrelated draft".into());
    let mut deck = LoopState {
        tabs: vec![None, Some(unrelated)],
        tab_cards_before: vec![None, None],
        active_tab: Some(0),
        ..Default::default()
    };
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    state.restore_prompt("unsent Home draft".into());
    // Home has no session; the event's immutable owner is the routing hint.
    let home_session = root.clone();
    let paint = |view: &TuiState| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| oc_tui::shell::render(frame, view))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    let events = vec![
        CoreEvent::TextDelta {
            session: root.clone(),
            turn: turn.clone(),
            delta: "captured parked ROOT delta".into(),
        },
        CoreEvent::ToolCallStarted {
            output_presentation: None,
            session: root.clone(),
            turn: turn.clone(),
            op: "parked-op".into(),
            name: "fixture_echo".into(),
            input: r#"{"value":"recorded input"}"#.into(),
            dcp_topic: None,
        },
        CoreEvent::ToolCallFinished {
            session: root.clone(),
            turn: turn.clone(),
            op: "parked-op".into(),
            name: "fixture_echo".into(),
            state: "completed".into(),
            output: "RAW result remains a separate fact".into(),
            output_bytes: 34,
            output_truncated: false,
            output_presentation: Some(Box::new(Presentation::new("typed parked body", 17, false))),
            patch_effects: None,
            dcp: None,
            question: None,
        },
        CoreEvent::SessionTitleUpdated {
            session: root.clone(),
            title: "committed parked title".into(),
        },
        CoreEvent::TextDelta {
            session: SessionId("foreign-not-in-this-deck".into()),
            turn: turn.clone(),
            delta: "foreign text must not leak".into(),
        },
    ];
    for event in events {
        handle_worker_event(&app, &mut state, &mut deck, &home_session, event)
            .await
            .unwrap();
    }
    let parked = deck.tabs[0].as_mut().unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| oc_tui::shell::render(frame, parked))
        .unwrap();
    let label = "fixture_echo";
    let position = terminal
        .backend()
        .buffer()
        .content
        .windows(label.len())
        .position(|cells| cells.iter().map(|cell| cell.symbol()).collect::<String>() == label)
        .expect("painted generic header is the real expansion target");
    for kind in [
        crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
    ] {
        parked.handle_mouse(
            crossterm::event::MouseEvent {
                kind,
                column: (position % 120) as u16,
                row: (position / 120) as u16,
                modifiers: crossterm::event::KeyModifiers::NONE,
            },
            ratatui::layout::Rect::new(0, 0, 120, 40),
        );
    }
    let painted = paint(parked);
    assert!(painted.contains("captured parked ROOT delta"));
    assert!(painted.contains("typed parked body"));
    assert!(!painted.contains("foreign text must not leak"));
    assert_eq!(parked.active_turn(), Some(&turn));
    assert_eq!(
        parked.session_title.as_deref(),
        Some("committed parked title")
    );
    assert_eq!(parked.input(), "unfinished root draft");
    assert_eq!(state.input(), "unsent Home draft");
    assert!(!paint(&state).contains("captured parked ROOT delta"));
    assert_eq!(deck.tabs[1].as_ref().unwrap().input(), "unrelated draft");
    assert_eq!(
        deck.tabs[1].as_ref().unwrap().attached_session(),
        Some(&other)
    );
    assert_eq!(deck.active_tab, None);
    assert!(inbox.try_recv().is_err(), "no source-unrelated lookups");

    // A failed bounded history read must not discard the unique parked view.
    let expected = root.clone();
    let owner = tokio::spawn(async move {
        let InboxMsg::History { session, ack, .. } = inbox.recv().await.unwrap() else {
            panic!("completion queries its immutable owner, not Home")
        };
        assert_eq!(session, expected);
        ack.send(Err(CoreError::Shutdown)).unwrap();
        inbox
    });
    assert!(
        handle_worker_event(
            &app,
            &mut state,
            &mut deck,
            &home_session,
            CoreEvent::TurnFinished {
                session: root.clone(),
                turn,
                text: "captured parked ROOT delta".into(),
                duration_ms: 123,
                warnings: Vec::new(),
                service_warning_range: 0..0,
            },
        )
        .await
        .is_err()
    );
    let mut inbox = owner.await.unwrap();
    let parked = deck.tabs[0]
        .as_ref()
        .expect("root survives error propagation");
    assert_eq!(parked.attached_session(), Some(&root));
    assert!(!parked.is_busy());
    assert_eq!(parked.input(), "unfinished root draft");
    assert_eq!(state.input(), "unsent Home draft");
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn busy_root_strip_add_commits_before_parking_and_keeps_cancel_route() {
    for refuse_save in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let root = SessionId("accepted-root-for-add".into());
        let turn = WorkerTurnId("existing-root-turn".into());
        let mut state = TuiState::new(app.clone(), root.clone());
        state.restore_prompt("accepted original prompt".into());
        state.handle_key(KeyAction::Enter).await;
        let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
            panic!("one original admission")
        };
        let mut deck = LoopState {
            tabs: vec![None],
            tab_cards_before: vec![None],
            active_tab: Some(0),
            location: Some("/fixture".into()),
            revision: Some("before-add".into()),
            ..Default::default()
        };
        assert!(!deck.can_park_busy_root(&state), "pending is not accepted");
        ack.send(Ok(turn.clone())).unwrap();
        state.poll_submission();
        state.restore_prompt("unfinished accepted-root draft".into());
        deck.sync_tabs(&mut state);
        assert!(state.tab_presentation().2, "painted Add is owner-enabled");
        assert!(
            apply_intent(&app, &mut state, &mut deck, PanelIntent::NewSession)
                .await
                .is_err(),
            "command New still refuses an ordinary busy turn"
        );
        assert!(inbox.try_recv().is_err());
        let before = deck.snapshot(&state);
        deck.conversation_recovery = Some((root.clone(), Instant::now()));
        assert!(
            apply_intent(&app, &mut state, &mut deck, PanelIntent::AddSessionTab)
                .await
                .is_err()
        );
        deck.conversation_recovery = None;
        assert_eq!(deck.snapshot(&state), before);
        assert!(inbox.try_recv().is_err());

        let expected_root = root.clone();
        let expected_before = before.clone();
        let owner = tokio::spawn(async move {
            let InboxMsg::HomeSelection { action, ack } = inbox.recv().await.unwrap() else {
                panic!("sessionless Home choice, not a new runtime admission")
            };
            assert!(matches!(action, SelectionAction::New(_)));
            let mut catalog = catalog();
            catalog.chrome.location = Some("/fixture".into());
            ack.send(Ok(catalog)).unwrap();
            let InboxMsg::SaveTabDeck {
                deck: snapshot,
                ack,
            } = inbox.recv().await.unwrap()
            else {
                panic!("CAS before replacing the selected root")
            };
            assert_eq!(snapshot.sessions, vec![expected_root]);
            assert_eq!(
                snapshot.new_session_titles,
                expected_before.new_session_titles
            );
            assert_eq!(snapshot.location, expected_before.location);
            assert_eq!(snapshot.revision, expected_before.revision);
            assert_eq!(snapshot.active, None);
            if refuse_save {
                ack.send(Err(CoreError::TabDeckStorage)).unwrap();
            } else {
                ack.send(Ok(TabDeckSnapshot {
                    revision: Some("after-add".into()),
                    ..snapshot
                }))
                .unwrap();
            }
            inbox
        });
        let result = apply_intent(&app, &mut state, &mut deck, PanelIntent::AddSessionTab).await;
        inbox = owner.await.unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "no submit, cancel, replay or extra save"
        );
        if refuse_save {
            assert!(result.is_err());
            assert_eq!(deck.snapshot(&state), before);
            assert_eq!(state.attached_session(), Some(&root));
            assert_eq!(state.active_turn(), Some(&turn));
            assert_eq!(state.input(), "unfinished accepted-root draft");
            assert!(deck.tabs[0].is_none(), "the selected root is still unique");
            continue;
        }
        result.unwrap();
        assert!(state.attached_session().is_none());
        let parked = deck.tabs[0].as_ref().unwrap();
        assert_eq!(parked.attached_session(), Some(&root));
        assert_eq!(parked.active_turn(), Some(&turn));
        assert_eq!(parked.input(), "unfinished accepted-root draft");
        assert_eq!(deck.revision.as_deref(), Some("after-add"));
        assert!(deck.close_tab(&app, &mut state, 0).await.is_err());
        assert!(
            inbox.try_recv().is_err(),
            "busy parked owner cannot be discarded"
        );
        // Home is idle: the root's same mounted state can be selected and stopped.
        deck.activate(&mut state, 0).unwrap();
        assert_eq!(state.attached_session(), Some(&root));
        assert_eq!(state.active_turn(), Some(&turn));
        state.handle_key(KeyAction::Cancel).await;
        assert!(
            inbox.try_recv().is_err(),
            "first Esc keeps the interrupt guard"
        );
        let owner = tokio::spawn(async move {
            let InboxMsg::Cancel { session, ack } = inbox.recv().await.unwrap() else {
                panic!("second Esc addresses the original root")
            };
            assert_eq!(session, root);
            ack.send(Ok(())).unwrap();
            inbox
        });
        state.handle_key(KeyAction::Cancel).await;
        inbox = owner.await.unwrap();
        assert!(inbox.try_recv().is_err());
        assert_eq!(state.input(), "unfinished accepted-root draft");
    }
}

#[tokio::test]
async fn parked_root_controls_keep_bindings_without_selecting_home_or_replying() {
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    use oc_core::question::{QuestionInput, QuestionPrompt, QuestionRequest};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let root = SessionId("control-root".into());
    let binding = ApprovalBinding {
        session: root.0.clone(),
        turn: "accepted-root-turn".into(),
        call: "captured-call".into(),
        operation: "captured-operation".into(),
        input_digest: "immutable-input-digest".into(),
        location: "/fixture".into(),
        generation: 7,
        agent: None,
        agent_digest: None,
    };
    let permission = ApprovalRequest {
        id: 31,
        binding: binding.clone(),
        project: "/fixture".into(),
        action: "read".into(),
        resources: vec!["actual-resource".into()],
        save_patterns: Vec::new(),
        preview: ApprovalPreview::Resource {
            values: vec!["actual-resource".into()],
        },
    };
    let question = QuestionRequest {
        id: 32,
        binding,
        input: QuestionInput {
            questions: vec![QuestionPrompt {
                question: "Captured root question?".into(),
                header: "Root question".into(),
                options: Vec::new(),
                multiple: false,
            }],
        },
    };
    let mut parked = TuiState::new(app.clone(), root.clone());
    parked.restore_prompt("original root draft".into());
    let mut state = TuiState::new_home(app.clone());
    state.restore_prompt("unsent Home draft".into());
    let mut deck = LoopState {
        tabs: vec![Some(parked)],
        tab_cards_before: vec![None],
        ..Default::default()
    };
    let expected_permission = permission.clone();
    let expected_question = question.clone();
    let expected_root = root.clone();
    let owner = tokio::spawn(async move {
        for _ in 0..2 {
            let InboxMsg::PendingApprovals { ack } = inbox.recv().await.unwrap() else {
                panic!("bounded permission snapshot")
            };
            ack.send(Ok(vec![expected_permission.clone()])).unwrap();
            let InboxMsg::History { session, ack, .. } = inbox.recv().await.unwrap() else {
                panic!("permission ancestry")
            };
            assert_eq!(session, expected_root);
            ack.send(Ok(HistoryPage::default())).unwrap();
            let InboxMsg::PendingQuestions { ack } = inbox.recv().await.unwrap() else {
                panic!("bounded question snapshot")
            };
            ack.send(Ok(vec![expected_question.clone()])).unwrap();
            let InboxMsg::History { session, ack, .. } = inbox.recv().await.unwrap() else {
                panic!("question ancestry")
            };
            assert_eq!(session, expected_root);
            ack.send(Ok(HistoryPage::default())).unwrap();
        }
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &root,
        CoreEvent::QuestionAsked(question.clone()),
    )
    .await
    .unwrap();
    let inbox = owner.await.unwrap();
    assert!(
        inbox.is_empty(),
        "projection cannot authorize or answer work"
    );
    assert!(state.attached_session().is_none());
    assert_eq!(state.input(), "unsent Home draft");
    assert!(state.approvals.active().is_none() && state.questions.active().is_none());
    let parked = deck.tabs[0].as_ref().unwrap();
    assert_eq!(parked.attached_session(), Some(&root));
    assert_eq!(parked.input(), "original root draft");
    assert_eq!(parked.approvals.active(), Some(&permission));
    assert_eq!(parked.questions.active(), Some(&question));
    assert_eq!(deck.active_tab, None);
    let busy = parked.is_busy();
    deck.sync_tabs(&mut state);
    assert_eq!(
        state.tab_presentation().0[0].attention,
        Some(oc_tui::app::TabAttention::Permission)
    );
    assert_eq!(state.tab_presentation().0[0].busy, busy);
    state.approval_roots.clear();
    deck.sync_tabs(&mut state);
    assert_eq!(
        state.tab_presentation().0[0].attention,
        Some(oc_tui::app::TabAttention::Question)
    );
    assert_eq!(
        state.tab_presentation().0[0].busy,
        busy,
        "attention never changes the execution state"
    );
    deck.tabs[0]
        .as_mut()
        .unwrap()
        .questions
        .reconcile(&[], vec![]);
    deck.sync_tabs(&mut state);
    assert_eq!(state.tab_presentation().0[0].attention, None);
    deck.tabs[0]
        .as_mut()
        .unwrap()
        .questions
        .reconcile(std::slice::from_ref(&question), vec![question.clone()]);
    deck.activate(&mut state, 0).unwrap();
    let reply = state.questions.key(KeyAction::Cancel).intent.unwrap();
    assert!(
        matches!(reply, PanelIntent::ReplyQuestion(ref reply) if reply.id == question.id && reply.binding == question.binding)
    );
    assert_eq!(state.input(), "original root draft");
}

#[tokio::test]
async fn fresh_shell_permission_stays_with_its_receipt_without_parked_history_lookup() {
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    state.handle_key(KeyAction::Char('!')).await;
    state.restore_prompt("printf explicit command".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::UserShell {
        session,
        command,
        cancel,
        ack,
        ..
    } = inbox.try_recv().unwrap()
    else {
        panic!("actual fresh-shell receipt")
    };
    assert_eq!(command, "printf explicit command");
    assert_eq!(state.user_shell_admission_session(), Some(&session));
    assert!(state.attached_session().is_none());
    let permission = ApprovalRequest {
        id: 41,
        binding: ApprovalBinding {
            session: session.0.clone(),
            turn: "captured-shell-admission".into(),
            call: "captured-shell-call".into(),
            operation: "captured-shell-operation".into(),
            input_digest: "captured-command-digest".into(),
            location: "/fixture".into(),
            generation: 7,
            agent: None,
            agent_digest: None,
        },
        project: "/fixture".into(),
        action: "bash".into(),
        resources: vec![command],
        save_patterns: Vec::new(),
        preview: ApprovalPreview::Shell {
            command: "printf explicit command".into(),
            cwd: "/fixture".into(),
        },
    };
    let root = SessionId("parked-shell-origin".into());
    let mut parked = TuiState::new(app.clone(), root.clone());
    parked.restore_prompt("original root draft".into());
    let mut deck = LoopState {
        tabs: vec![Some(parked)],
        tab_cards_before: vec![None],
        ..Default::default()
    };
    let expected = permission.clone();
    let fresh = session.clone();
    let owner = tokio::spawn(async move {
        for _ in 0..2 {
            let InboxMsg::PendingApprovals { ack } = inbox.recv().await.unwrap() else {
                panic!("current permission snapshot")
            };
            ack.send(Ok(vec![expected.clone()])).unwrap();
            match inbox.recv().await.unwrap() {
                InboxMsg::PendingQuestions { ack } => {
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::History { session, ack, .. } => {
                    assert_eq!(session, fresh, "baseline queried the unadmitted candidate");
                    ack.send(Err(CoreError::Shutdown)).unwrap();
                    return inbox;
                }
                _ => panic!("projection must not admit or answer the shell"),
            }
        }
        inbox
    });
    let result = refresh_control_views(&app, &mut state, &mut deck).await;
    let inbox = owner.await.unwrap();
    assert!(
        result.is_ok(),
        "unadmitted history is not a valid projection source: {result:?}"
    );
    assert!(
        inbox.is_empty(),
        "no Submit, Reply, Cancel or session creation"
    );
    assert!(!cancel.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(state.approvals.active(), Some(&permission));
    assert_eq!(state.user_shell_admission_session(), Some(&session));
    assert!(state.attached_session().is_none() && state.has_pending_submission());
    assert_eq!(state.input(), "printf explicit command");
    let parked = deck.tabs[0].as_ref().unwrap();
    assert_eq!(parked.attached_session(), Some(&root));
    assert_eq!(parked.input(), "original root draft");
    assert!(parked.approvals.active().is_none());
    assert_eq!(deck.active_tab, None);
    drop(ack);
}

#[tokio::test]
async fn model_shell_notice_reads_only_the_known_original_capture_and_keeps_live_parts() {
    use oc_core::{
        queries::{
            HistoryMessage, HistoryPage, ShellHistoryNotice, ShellJob, ShellNotice, ShellSnapshot,
        },
        session::{MessageId, Role},
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId("model-shell-source".into());
    let current = WorkerTurnId("model-shell-turn".into());
    let mut state = TuiState::new(app.clone(), session.clone());
    state.begin_linked_turn(current.clone());
    state.apply_delta(&current, "same assistant part");
    state.apply_tool_started(
        &current,
        "actual-model-operation",
        "shell",
        r#"{"command":"captured original command"}"#,
    );
    state.restore_prompt("unfinished draft".into());
    let source = session.clone();
    let owner = tokio::spawn(async move {
        let InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("exact data-notice qualification before any capture lookup");
        };
        assert_eq!(session, source);
        assert_eq!(message, Some(MessageId("actual-message".into())));
        assert_eq!(limit, 1);
        let row = HistoryMessage {
            images: Vec::new(),
            id: MessageId("actual-message".into()),
            seq: 9,
            role: Role::User,
            text: "technical RAW envelope must not become a toast".into(),
            turn: None,
            model_switch: None,
            user_shell: None,
            child: None,
            shell_notice: Some(ShellHistoryNotice {
                operation: "actual-model-operation".into(),
                state: "completed".into(),
                command: "captured original command".into(),
            }),
        };
        ack.send(Ok(HistoryPage {
            rows: vec![row.clone()],
            total: 1,
            ..Default::default()
        }))
        .unwrap();
        let InboxMsg::ShellSnapshot {
            session,
            shell_id,
            ack,
        } = inbox.recv().await.unwrap()
        else {
            panic!("one original capture query after qualification")
        };
        assert_eq!(session, source);
        assert_eq!(shell_id, "actual-model-operation");
        ack.send(Ok(ShellSnapshot {
            job: ShellJob {
                session,
                shell_id,
                location: "/original-owned-location".into(),
                generation: 17,
                turn: "model-shell-turn".into(),
                model: "fixture/model".into(),
                provider: "fixture".into(),
                command: "captured original command".into(),
                pid: None,
                background: true,
                output: None,
            },
            state: "completed".into(),
            stdout_cursor: 20,
            stderr_cursor: 0,
            truncated: false,
            text: "legacy envelope must not replace process output".into(),
            display: "actual frozen final flush\n".into(),
            display_omitted: false,
            exit: Some(0),
            signal: None,
        }))
        .unwrap();
        let InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("bounded current-turn correlation, not a second model graph");
        };
        assert_eq!(session, source);
        assert!(message.is_none());
        assert_eq!(limit, HISTORY_PAGE_LIMIT);
        ack.send(Ok(HistoryPage {
            rows: vec![row],
            total: 1,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "actual-model-operation".into(),
            delivery_id: "actual-delivery".into(),
            message_id: "actual-message".into(),
            user_requested: false,
            state: "completed".into(),
            text: "existing model notification".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = owner.await.unwrap();
    let rows = state.transcript_rows();
    assert_eq!(
        rows.len(),
        3,
        "one data notice and the same two model parts"
    );
    assert_eq!(rows[0].role, "shell_notice");
    assert_eq!(rows[1].text, "same assistant part");
    let card = rows[2].tool.as_ref().unwrap();
    assert_eq!(card.state, "started", "logical graph state is immutable");
    assert_eq!(card.op, "actual-model-operation");
    assert!(card.output_preview.contains("actual frozen final flush"));
    assert!(!card.output_preview.contains("legacy envelope"));
    assert_eq!(state.active_turn(), Some(&current));
    assert_eq!(state.input(), "unfinished draft");
    assert!(state.note().is_none());
    assert!(inbox.try_recv().is_err());
    // A hidden/unassociated notice gets only an exact current-branch lookup,
    // never a fabricated capture association or a technical RAW toast.
    let hidden = tokio::spawn(async move {
        let InboxMsg::History {
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("even a known card cannot read capture for an unqualified notice");
        };
        assert_eq!(message, Some(MessageId("unrelated-message".into())));
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage::default())).unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "actual-model-operation".into(),
            delivery_id: "unrelated-delivery".into(),
            message_id: "unrelated-message".into(),
            user_requested: false,
            state: "completed".into(),
            text: "unrelated model notification".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = hidden.await.unwrap();
    assert_eq!(state.transcript_rows(), rows);
    assert!(state.note().is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn exact_child_notice_terminalizes_retained_launch_outside_inventory_without_retargeting() {
    use oc_core::queries::{ChildHistory, ChildJob, ChildNotice, ChildState};
    for retained_history in [false, true] {
        for continuation in [false, true] {
            let (app, mut inbox, _) = CoreApp::channel(8);
            let parent = SessionId("notice-card-parent".into());
            let turn = WorkerTurnId("launch-parent-turn".into());
            let mut state = TuiState::new(app.clone(), parent.clone());
            let make_job = |name: &str| ChildJob {
                parent: parent.clone(),
                child: SessionId(format!("child-{name}")),
                operation: name.into(),
                generation: 7,
                location: "/original".into(),
                agent: "explore".into(),
                model: "fixture/child".into(),
                description: format!("Task {name}"),
                delivery_id: format!("delivery-{name}"),
                state: ChildState::Running,
                background: true,
                turn: Some(format!("child-turn-{name}")),
                result: None,
                message_id: None,
            };
            let a = make_job("A");
            let b = make_job("B");
            state.begin_linked_turn(turn.clone());
            state.apply_child_jobs(vec![a.clone(), b.clone()]);
            for job in [&a, &b] {
                let input = serde_json::json!({"agent":"explore", "description":job.description, "prompt":"controlled task", "background":true});
                state.apply_tool_started(&turn, &job.operation, "subagent", &input.to_string());
                state.apply_tool_finished(
                    &turn,
                    &job.operation,
                    "subagent",
                    "running",
                    "owned returned handle",
                    21,
                    false,
                );
            }
            if retained_history {
                state.apply_finished(&turn, "parent finished", 1);
                state.begin_linked_turn(WorkerTurnId("later-parent-turn".into()));
            }
            state.restore_prompt("newer unsent draft".into());
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| oc_tui::shell::render(frame, &state))
                .unwrap();
            let original = state
                .transcript_rows()
                .into_iter()
                .find_map(|row| row.tool.filter(|card| card.op == "B"))
                .unwrap();
            let mut finished = b.clone();
            finished.state = ChildState::Completed;
            finished.message_id = Some("finished-B".into());
            let notice = HistoryMessage {
                id: oc_core::session::MessageId("finished-B".into()),
                seq: 8,
                role: Role::User,
                text: "RAW is not presentation".into(),
                images: vec![],
                turn: None,
                model_switch: None,
                user_shell: None,
                child: Some(ChildHistory::Notice(Box::new(finished.clone()))),
                shell_notice: None,
            };
            let mut inventory = vec![a.clone()];
            if continuation {
                let mut next = b.clone();
                next.operation = "B-continuation".into();
                next.generation += 1;
                next.delivery_id = "continued-delivery".into();
                inventory.push(next);
            }
            while inventory.len() < 16 {
                inventory.push(make_job(&format!("other-{}", inventory.len())));
            }
            assert!(!inventory.iter().any(|job| job.operation == "B"));
            let owner = tokio::spawn(async move {
                for _ in 0..2 {
                    let InboxMsg::History {
                        message,
                        limit,
                        ack,
                        ..
                    } = inbox.recv().await.unwrap()
                    else {
                        panic!("exact notice")
                    };
                    assert_eq!(message, Some(notice.id.clone()));
                    assert_eq!(limit, 1);
                    ack.send(Ok(HistoryPage {
                        rows: vec![notice.clone()],
                        total: 8,
                        ..Default::default()
                    }))
                    .unwrap();
                    let InboxMsg::ChildJobs { ack, .. } = inbox.recv().await.unwrap() else {
                        panic!("bounded current inventory")
                    };
                    ack.send(Ok(inventory.clone())).unwrap();
                    let InboxMsg::History { message, ack, .. } = inbox.recv().await.unwrap() else {
                        panic!("recent history")
                    };
                    assert!(message.is_none());
                    ack.send(Ok(HistoryPage {
                        rows: vec![notice.clone()],
                        total: 8,
                        ..Default::default()
                    }))
                    .unwrap();
                }
                inbox
            });
            for _ in 0..2 {
                handle_worker_event(
                    &app,
                    &mut state,
                    &mut LoopState::default(),
                    &parent,
                    CoreEvent::ChildNotice(ChildNotice {
                        job: finished.clone(),
                        text: "untrusted event prose".into(),
                    }),
                )
                .await
                .unwrap();
                terminal
                    .draw(|frame| oc_tui::shell::render(frame, &state))
                    .unwrap();
                let rows = state.transcript_rows();
                let cards: Vec<_> = rows.iter().filter_map(|row| row.tool.as_ref()).collect();
                assert_eq!(cards.len(), 2);
                let updated = cards.iter().find(|card| card.op == "B").unwrap();
                assert_eq!(updated.child_job.as_deref(), Some(&finished));
                assert_eq!(updated.state, original.state);
                assert_eq!(updated.output_preview, original.output_preview);
                assert!(
                    matches!(&updated.render, oc_tui::tools::ToolRender::Subagent(view) if view.live_running == continuation)
                );
                assert_eq!(
                    cards
                        .iter()
                        .find(|card| card.op == "A")
                        .unwrap()
                        .child_job
                        .as_deref(),
                    Some(&a)
                );
                let shown = state
                    .transcript_lines(120, 120)
                    .iter()
                    .map(oc_tui::styled::Line::plain_text)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    shown.contains(if continuation {
                        "Explore Subagent — Task B"
                    } else {
                        "✓ Explore Subagent — Task B"
                    }),
                    "{shown}"
                );
                assert!(shown.contains("Background"));
                assert_eq!(state.input(), "newer unsent draft");
            }
            assert!(owner.await.unwrap().try_recv().is_err());
        }
    }
}

#[tokio::test]
async fn typed_child_notice_during_parent_stream_keeps_exact_prompt_and_live_parts() {
    use oc_core::{
        queries::{
            ChildHistory, ChildJob, ChildNotice, ChildState, HistoryMessage, HistoryPage,
            HistoryTurn, TranscriptPart,
        },
        session::{MessageId, Role},
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId("busy-child-notice".into());
    let current = WorkerTurnId("current-parent-turn".into());
    let mut state = TuiState::new(app.clone(), session.clone());
    state.restore_prompt("ordinary accepted prompt".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("actual submit");
    };
    ack.send(Ok(current.clone())).unwrap();
    state.poll_submission();
    state.apply_delta(&current, "live assistant only once");
    state.restore_prompt("new unfinished draft".into());
    let job = ChildJob {
        parent: session.clone(),
        child: SessionId("actual-child".into()),
        operation: "child-operation".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/child".into(),
        description: "owned task".into(),
        delivery_id: "delivery".into(),
        state: ChildState::Completed,
        background: true,
        turn: Some("child-turn".into()),
        result: None,
        message_id: Some("actual-notice".into()),
    };
    let notice = HistoryMessage {
        images: Vec::new(),
        id: MessageId("actual-notice".into()),
        seq: 9,
        role: Role::User,
        text: "RAW technical child JSON is not UI prose".into(),
        turn: None,
        model_switch: None,
        user_shell: None,
        child: Some(ChildHistory::Notice(Box::new(job.clone()))),
        shell_notice: None,
    };
    let owner = tokio::spawn(async move {
        let InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("exact notice");
        };
        assert_eq!(session.0, "busy-child-notice");
        assert_eq!(message, Some(notice.id.clone()));
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage {
            rows: vec![notice.clone()],
            total: 120,
            has_newer: true,
            ..Default::default()
        }))
        .unwrap();
        let InboxMsg::History {
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("bounded identity correlation");
        };
        assert!(message.is_none());
        assert_eq!(limit, HISTORY_PAGE_LIMIT);
        ack.send(Ok(HistoryPage {
            rows: vec![
                HistoryMessage {
                    images: Vec::new(),
                    id: MessageId("accepted-prompt".into()),
                    seq: 8,
                    role: Role::User,
                    text: "ordinary accepted prompt".into(),
                    turn: Some(HistoryTurn {
                        id: "current-parent-turn".into(),
                        status: "executing".into(),
                        parts: vec![TranscriptPart::Text(
                            "must not duplicate executing projection".into(),
                        )],
                        ..Default::default()
                    }),
                    model_switch: None,
                    user_shell: None,
                    child: None,
                    shell_notice: None,
                },
                notice,
            ],
            total: 120,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ChildNotice(ChildNotice {
            job,
            text: "RAW technical child JSON is not UI prose".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = owner.await.unwrap();
    let text = state
        .transcript_lines(120, 120)
        .iter()
        .map(oc_tui::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("↳ Helper finished · owned task"));
    assert!(!text.contains("RAW technical") && !text.contains("must not duplicate"));
    assert_eq!(text.matches("live assistant only once").count(), 1);
    assert_eq!(
        state.history().rows()[0].message_id.as_deref(),
        Some(&MessageId("accepted-prompt".into()))
    );
    assert_eq!(state.history().rows()[1].role, "child_notice");
    assert_eq!(state.active_turn(), Some(&current));
    assert_eq!(state.input(), "new unfinished draft");
    assert!(state.note().is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn shell_changed_updates_closed_owner_footer_and_parked_view_without_cross_session_requests()
{
    use oc_core::queries::ShellJob;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let a = SessionId("footer-a".into());
    let b = SessionId("footer-b".into());
    let mut state = TuiState::new(app.clone(), a.clone());
    state.restore_prompt("draft a".into());
    let mut parked = TuiState::new(app.clone(), b.clone());
    parked.restore_prompt("draft b".into());
    let mut deck = LoopState {
        tabs: vec![None, Some(parked)],
        active_tab: Some(0),
        ..Default::default()
    };
    let painted = |view: &TuiState| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| oc_tui::shell::render(f, view)).unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    for (source, live) in [(b.clone(), true), (a.clone(), true), (a.clone(), false)] {
        let wanted = source.clone();
        let owner = tokio::spawn(async move {
            let InboxMsg::ShellJobs { session, ack } = inbox.recv().await.unwrap() else {
                panic!("one source-owned inventory query")
            };
            assert_eq!(session, wanted);
            ack.send(Ok(if live {
                vec![ShellJob {
                    session,
                    shell_id: "existing-live-operation".into(),
                    location: "/fixture".into(),
                    generation: 1,
                    turn: String::new(),
                    model: "fixture/model".into(),
                    provider: "fixture".into(),
                    command: "actual command".into(),
                    pid: Some(123),
                    background: false,
                    output: None,
                }]
            } else {
                Vec::new()
            }))
            .unwrap();
            inbox
        });
        handle_worker_event(
            &app,
            &mut state,
            &mut deck,
            &a,
            CoreEvent::ShellChanged {
                session: source.clone(),
            },
        )
        .await
        .unwrap();
        inbox = owner.await.unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "no unrelated inventory, provider or history query"
        );
        assert_eq!(state.input(), "draft a");
        assert_eq!(deck.tabs[1].as_ref().unwrap().input(), "draft b");
        assert!(!state.shells_open());
        assert!(!deck.tabs[1].as_ref().unwrap().shells_open());
        assert!(painted(deck.tabs[1].as_ref().unwrap()).contains("↓ 1 shell"));
        assert_eq!(painted(&state).contains("↓ 1 shell"), source == a && live);
    }
}

#[tokio::test]
async fn direct_user_shell_notice_updates_typed_block_during_live_model_turn_without_raw_toast() {
    use oc_core::{
        queries::{HistoryMessage, HistoryPage, ShellNotice, UserShellResult},
        session::{MessageId, Role},
        tool_output::{Presentation, Shell},
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId::new("user-shell-with-live-model").unwrap();
    let current = WorkerTurnId("real-live-overlay".into());
    let mut state = TuiState::new(app.clone(), session.clone());
    state.restore_prompt("ordinary accepted prompt".into());
    state.handle_key(KeyAction::Enter).await;
    let InboxMsg::Submit { ack, .. } = inbox.recv().await.unwrap() else {
        panic!("model submit");
    };
    ack.send(Ok(current.clone())).unwrap();
    state.poll_submission();
    state.apply_delta(&current, "live assistant text");
    state.restore_prompt("unfinished newer draft".into());
    let mut output = Presentation::new("user process output", 19, false);
    output.shell = Some(Shell {
        background: false,
        process_state: None,
        stdout: "user process output".into(),
        stderr: String::new(),
        stdout_limited: false,
        stderr_limited: false,
        exit: Some(0),
        signal: None,
        timed_out: false,
        cancelled: false,
    });
    let owner = tokio::spawn(async move {
        let InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("bounded history query");
        };
        assert_eq!(session.0, "user-shell-with-live-model");
        assert_eq!(message, Some(MessageId("actual-shell-notice".into())));
        assert_eq!(
            limit, 1,
            "a delayed notice is not searched in the newest page"
        );
        ack.send(Ok(HistoryPage {
            rows: vec![HistoryMessage {
                images: Vec::new(),
                id: MessageId("actual-shell-notice".into()),
                seq: 7,
                role: Role::User,
                text: "RAW technical notice must not become a toast".into(),
                turn: None,
                model_switch: None,
                child: None,
                shell_notice: None,
                user_shell: Some(UserShellResult {
                    input: false,
                    superseded_input: false,
                    operation: "owned-user-command".into(),
                    command: "printf output".into(),
                    command_limited: false,
                    state: "completed".into(),
                    output: Box::new(output),
                    diagnostic: None,
                }),
            }],
            total: 120,
            has_newer: true,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "owned-user-command".into(),
            delivery_id: "once-only".into(),
            message_id: "actual-shell-notice".into(),
            user_requested: true,
            state: "completed".into(),
            text: "RAW technical notice must not become a toast".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = owner.await.unwrap();
    let text = state
        .transcript_lines(120, 120)
        .iter()
        .map(oc_tui::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("$ printf output") && text.contains("user process output"));
    assert_eq!(text.matches("live assistant text").count(), 1);
    assert!(!text.contains("must not duplicate") && !text.contains("RAW technical"));
    assert!(state.note().is_none());
    assert_eq!(state.active_turn(), Some(&current));
    assert_eq!(state.input(), "unfinished newer draft");

    let hidden_owner = tokio::spawn(async move {
        let InboxMsg::History { message, ack, .. } = inbox.recv().await.unwrap() else {
            panic!("exact current-branch lookup");
        };
        assert_eq!(message, Some(MessageId("hidden-native-result".into())));
        ack.send(Ok(HistoryPage {
            total: 120,
            ..Default::default()
        }))
        .unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "hidden-operation".into(),
            delivery_id: "hidden-delivery".into(),
            message_id: "hidden-native-result".into(),
            user_requested: true,
            state: "completed".into(),
            text: "hidden RAW warning".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = hidden_owner.await.unwrap();
    assert!(
        state.note().is_none(),
        "branch-hidden native result does not become RAW prose"
    );
    assert_eq!(state.input(), "unfinished newer draft");
    let model_owner = tokio::spawn(async move {
        let InboxMsg::History {
            message,
            limit,
            ack,
            ..
        } = inbox.recv().await.unwrap()
        else {
            panic!("exact model-notice identity, not RAW prose");
        };
        assert_eq!(message, Some(MessageId("model-result".into())));
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage::default())).unwrap();
        inbox
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut LoopState::default(),
        &session,
        CoreEvent::ShellNotice(ShellNotice {
            session: session.clone(),
            shell_id: "model-operation".into(),
            delivery_id: "model-delivery".into(),
            message_id: "model-result".into(),
            user_requested: false,
            state: "completed".into(),
            text: "existing model background notice".into(),
        }),
    )
    .await
    .unwrap();
    let mut inbox = model_owner.await.unwrap();
    assert!(
        state.note().is_none(),
        "unassociated model data never becomes RAW prose"
    );
    assert!(
        inbox.try_recv().is_err(),
        "no capture or extra model request for an unassociated notice"
    );
}

#[tokio::test]
async fn vis38_late_dcp_other_session_child_and_old_turn_never_touch_parent_view_or_query() {
    use oc_core::dcp_view::{DcpAccounting, DcpRunSnapshot};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let parent = SessionId::new("dcp-parent").unwrap();
    let current = WorkerTurnId("current-generation".into());
    let mut state = TuiState::new(app.clone(), parent.clone());
    state.restore_prompt("active parent request".into());
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("parent submission")
    };
    ack.send(Ok(current.clone())).unwrap();
    state.poll_submission();
    assert_eq!(state.active_turn(), Some(&current));
    let before = state.transcript_lines(100, 100);
    let mut deck = LoopState::default();
    for (owner, turn) in [
        (SessionId::new("dcp-other").unwrap(), current.clone()),
        (SessionId::new("dcp-child").unwrap(), current.clone()),
        (parent.clone(), WorkerTurnId("obsolete-generation".into())),
    ] {
        let event = CoreEvent::ToolCallFinished {
            output_presentation: None,
            question: None,
            session: owner.clone(),
            turn,
            op: "late-operation".into(),
            name: "compress".into(),
            state: "completed".into(),
            output: "successful-looking untrusted output".into(),
            output_bytes: 44,
            output_truncated: false,
            patch_effects: None,
            dcp: Some(DcpRunSnapshot {
                session: owner.0,
                operation_id: "late-operation".into(),
                ordinal: 42,
                topic: "late child must not affect parent".into(),
                block_ids: vec!["foreign-block".into()],
                removed: 10000,
                summary: 42,
                net_saved: 9958,
                method: Default::default(),
                new_messages: 2,
                new_tools: 1,
                cumulative: DcpAccounting {
                    gross_removed: 10000,
                    compressions: 42,
                    complete: true,
                    ..Default::default()
                },
                bar: "⣿".repeat(50),
            }),
        };
        tokio::time::timeout(
            Duration::from_millis(250),
            handle_worker_event(&app, &mut state, &mut deck, &parent, event),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(state.transcript_lines(100, 100), before);
        assert!(state.note().is_none());
        assert!(
            inbox.try_recv().is_err(),
            "late event must not refresh parent counters or summaries"
        );
    }
}

#[tokio::test]
async fn vis26_key_burst_only_queries_latest_and_route_swap_cancels_old_view() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    state.chrome.location = Some("/A".into());
    let mut deck = LoopState::default();
    for key in ['@', 's'] {
        state.handle_key(KeyAction::Char(key)).await;
        sync_mention(&app, &mut state, &mut deck).await;
    }
    tokio::time::sleep(Duration::from_millis(40)).await;
    state.handle_key(KeyAction::Char('r')).await;
    sync_mention(&app, &mut state, &mut deck).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "intermediate edits must not start owner traversal"
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions {
        query,
        limit,
        ack: old_ack,
    }) = inbox.recv().await
    else {
        panic!("one latest query");
    };
    assert_eq!(query, "sr");
    assert_eq!(limit, MENTION_LIMIT);
    let old_key = deck.mention_job.as_ref().unwrap().0.clone();

    // A second view can have the same draft, Location and local revision.
    let mut next = TuiState::new_home(app.clone());
    next.chrome.location = Some("/A".into());
    next.handle_paste("@sr");
    std::mem::swap(&mut state, &mut next);
    sync_mention(&app, &mut state, &mut deck).await;
    let new_key = state.mention_request().unwrap();
    assert_ne!(old_key.view_id, new_key.view_id);
    assert!(deck.mention_job.is_none());
    assert!(inbox.try_recv().is_err(), "new view also waits for quiet");
    tokio::time::sleep(MENTION_DEBOUNCE).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions { query, ack, .. }) = inbox.recv().await else {
        panic!("new view query");
    };
    assert_eq!(query, "sr");
    // The old owner request may already have started, but its receiver is
    // cancelled; it cannot populate either the old or new view.
    let _ = old_ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 1,
        paths: vec!["old".into()],
        truncated: false,
    }));
    ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 1,
        paths: vec!["fresh".into()],
        truncated: false,
    }))
    .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
        sync_mention(&app, &mut state, &mut deck).await;
        if state.mention_loaded(&new_key) {
            break;
        }
    }
    assert!(state.mention_loaded(&new_key));
    assert!(!next.mention_loaded(&old_key));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(
        inbox.try_recv().is_err(),
        "loaded result is not queried every frame"
    );
}

#[tokio::test]
async fn vis26_parked_tab_refreshes_file_rows_only_on_reactivation() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.chrome.location = Some("/A".into());
    state.handle_paste("@sr");
    let cached = state.mention_request().unwrap();
    assert!(state.apply_file_suggestions(
        cached.clone(),
        FileSuggestionsSnapshot {
            location: "/A".into(),
            generation: 1,
            paths: vec!["src/removed.rs".into()],
            truncated: false,
        }
    ));
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "active result stays cached");
    append_tab(&app, &mut deck, &mut state, "second");
    assert!(deck.tabs[0].as_ref().unwrap().mention_loaded(&cached));
    deck.activate(&mut state, 0).unwrap();
    let refreshed = state.mention_request().unwrap();
    assert_ne!(cached, refreshed);
    assert!(!state.mention_loaded(&refreshed));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "restored tab is debounced");
    tokio::time::sleep(MENTION_DEBOUNCE).await;
    sync_mention(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::FileSuggestions { query, ack, .. }) = inbox.recv().await else {
        panic!("restored tab requests a fresh snapshot");
    };
    assert_eq!(query, "sr");
    ack.send(Ok(FileSuggestionsSnapshot {
        location: "/A".into(),
        generation: 2,
        paths: vec!["src/new.rs".into()],
        truncated: false,
    }))
    .unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
        sync_mention(&app, &mut state, &mut deck).await;
        if state.mention_loaded(&refreshed) {
            break;
        }
    }
    assert!(state.mention_loaded(&refreshed));
    assert!(!state.mention_loaded(&cached));
    sync_mention(&app, &mut state, &mut deck).await;
    assert!(inbox.try_recv().is_err(), "normal sync does not invalidate");
    state.handle_key(KeyAction::Tab).await;
    assert_eq!(state.input(), "@src/new.rs ");
}

#[tokio::test]
async fn accepted_conversation_survives_refresh_failure_and_retries_queries_only() {
    use oc_core::queries::{ConversationAction, ConversationSnapshot, HistoryPage};
    for action in [
        ConversationAction::Undo,
        ConversationAction::Redo,
        ConversationAction::Revert {
            message: oc_core::session::MessageId("user".into()),
        },
    ] {
        for history_failure in [true, false] {
            let (app, mut inbox, _) = CoreApp::channel(8);
            let session = SessionId::new("receipt").unwrap();
            let mut state = TuiState::new(app.clone(), session.clone());
            state.session_title = Some("future title".into());
            state.restore_prompt("draft before ACK".into());
            let mut deck = LoopState::default();
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::ChangeConversation {
                    action: action.clone(),
                },
            )
            .await
            .unwrap();
            let worker = tokio::spawn(async move {
                let Some(InboxMsg::ChangeConversation { ack, session, .. }) = inbox.recv().await
                else {
                    panic!("mutation once")
                };
                ack.send(Ok(ConversationSnapshot {
                    images: Vec::new(),
                    session,
                    draft: Some("owner restored".into()),
                    can_undo: false,
                    can_redo: true,
                    reverted: None,
                }))
                .unwrap();
                let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                    panic!("history")
                };
                ack.send(if history_failure {
                    Err(CoreError::Shutdown)
                } else {
                    Ok(HistoryPage::default())
                })
                .unwrap();
                let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                    panic!("selection")
                };
                ack.send(if history_failure {
                    Ok(catalog())
                } else {
                    Err(CoreError::Shutdown)
                })
                .unwrap();
                empty_compactions(&mut inbox).await;
                inbox
            });
            settle_conversation(&app, &mut state, &mut deck).await;
            let mut inbox = worker.await.unwrap();
            assert_eq!(state.input(), "owner restored");
            assert!(
                state.session_title.is_none(),
                "future projection invalidated"
            );
            assert_eq!(
                state.command_unavailable(&CommandAction::UndoConversation),
                Some("nothing to undo")
            );
            assert!(deck.conversation_recovery.is_some());
            deck.conversation_recovery.as_mut().unwrap().1 = std::time::Instant::now();
            finish_conversation(&app, &mut state, &mut deck).await;
            let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                panic!("refresh only")
            };
            ack.send(Ok(HistoryPage::default())).unwrap();
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("refresh only")
            };
            ack.send(Ok(catalog())).unwrap();
            empty_compactions(&mut inbox).await;
            while !deck.recovery_job.as_ref().unwrap().is_finished() {
                tokio::task::yield_now().await;
            }
            let current_history = tokio::spawn(async move {
                let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
                    panic!("current recovery history")
                };
                ack.send(Ok(HistoryPage::default())).unwrap();
                inbox
            });
            finish_conversation(&app, &mut state, &mut deck).await;
            let mut inbox = current_history.await.unwrap();
            assert!(deck.conversation_recovery.is_none());
            assert_eq!(state.input(), "owner restored");
            assert!(inbox.try_recv().is_err());
        }
    }
}

#[tokio::test]
async fn conversation_recovery_cannot_restore_reverted_page_after_new_admission() {
    use oc_core::queries::{HistoryPage, RevertedConversation};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let session = SessionId::new("recovery-branch").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let staged = HistoryPage {
        reverted: Some(RevertedConversation {
            message: oc_core::session::MessageId("hidden-user".into()),
            user_messages: 2,
        }),
        ..Default::default()
    };
    state.attach_page(&staged);
    let mut deck = LoopState {
        conversation_recovery: Some((session, std::time::Instant::now())),
        ..Default::default()
    };
    finish_conversation(&app, &mut state, &mut deck).await;
    let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
        panic!("staged recovery history")
    };
    ack.send(Ok(staged)).unwrap();
    let Some(InboxMsg::SessionSelection { ack: metadata, .. }) = inbox.recv().await else {
        panic!("hold recovery metadata")
    };
    state.restore_prompt("replacement".into());
    state.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("replacement admission")
    };
    let turn = oc_core::core_app::WorkerTurnId("accepted-replacement".into());
    ack.send(Ok(turn.clone())).unwrap();
    state.poll_submission();
    state.restore_prompt("newer draft".into());
    state.apply_failed(&turn, &CoreError::Application("after admission".into()));
    metadata.send(Ok(catalog())).unwrap();
    empty_compactions(&mut inbox).await;
    while !deck.recovery_job.as_ref().unwrap().is_finished() {
        tokio::task::yield_now().await;
    }
    let current_history = tokio::spawn(async move {
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("current branch history")
        };
        ack.send(Ok(HistoryPage::default())).unwrap();
    });
    finish_conversation(&app, &mut state, &mut deck).await;
    assert!(
        !state
            .transcript_lines(120, 120)
            .iter()
            .any(|l| l.plain_text().contains("messages reverted"))
    );
    assert_eq!(state.input(), "newer draft");
    assert!(deck.conversation_recovery.is_none());
    current_history.await.unwrap();
}

#[tokio::test]
async fn copy_message_queries_exact_owner_row_instead_of_window_preview() {
    use oc_core::queries::{HistoryMessage, HistoryPage};
    use oc_core::session::{MessageId, Role};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId::new("copy-ui").unwrap());
    let text = "full public user text\n".repeat(500);
    let expected = text.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::History {
            before_seq,
            limit,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("owner history")
        };
        assert_eq!(before_seq, Some(43));
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage {
            rows: vec![HistoryMessage {
                images: Vec::new(),
                id: MessageId("opaque-user".into()),
                seq: 42,
                role: Role::User,
                text,
                turn: None,
                model_switch: None,
                child: None,
                shell_notice: None,
                user_shell: None,
            }],
            ..Default::default()
        }))
        .unwrap();
    });
    fn record_copy(text: &str) -> Result<(), String> {
        assert_eq!(text, "full public user text\n".repeat(500));
        Ok(())
    }
    let mut deck = LoopState {
        copy_transport: Some(record_copy),
        ..Default::default()
    };
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CopyMessage {
            message: MessageId("opaque-user".into()),
            seq: 42,
        },
    )
    .await
    .unwrap();
    assert_eq!(expected, "full public user text\n".repeat(500));
    assert!(
        state.take_copy_request().is_none(),
        "transport drained the request"
    );
    assert_eq!(state.note_variant(), Some(NoteVariant::Info));
    worker.await.unwrap();
}

#[tokio::test]
async fn mcp_detail_keyboard_copy_drains_actual_transport_before_outcome() {
    use oc_core::queries::{
        McpBinding, McpServerSnapshot, McpSnapshot, McpStatus, ServiceAction, ServiceCode,
        ServiceDiagnostic, ServiceKind, ServiceStage,
    };

    for succeeds in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut state = TuiState::new(app.clone(), SessionId::new("detail-copy-ui").unwrap());
        let mut chrome = catalog();
        chrome.chrome.location = Some("/fixture".into());
        state.apply_catalog(chrome);
        state.restore_prompt("unfinished Ω draft".into());
        state.apply_mcp_snapshot(McpSnapshot {
            binding: McpBinding {
                location: "/fixture".into(),
                generation: 1,
                instance: 1,
            },
            revision: 1,
            servers: vec![McpServerSnapshot {
                id: "opaque-control".into(),
                name: "safe-label".into(),
                configured_enabled: true,
                status: McpStatus::Failed,
                pending_action: None,
                tools: 0,
                actions: Vec::new(),
                diagnostic: Some(ServiceDiagnostic {
                    kind: ServiceKind::Mcp,
                    service: "opaque-diagnostic".into(),
                    source: "safe-source".into(),
                    field: vec!["mcp".into()],
                    stage: ServiceStage::Initialize,
                    code: ServiceCode::ConnectionFailed,
                    action: ServiceAction::RetryConnection,
                    details: None,
                }),
            }],
        });
        state.restore_prompt("/mcps".into());
        let open = state.handle_key(KeyAction::Enter);
        let history_owner = async {
            let Some(oc_core::core_app::InboxMsg::PromptHistory { append, ack }) =
                inbox.recv().await
            else {
                panic!("expected bounded input-history admission");
            };
            let mut entries = Vec::new();
            oc_core::queries::append_prompt_history(&mut entries, append.as_deref().unwrap())
                .unwrap();
            assert_eq!(entries, ["/mcps"]);
            ack.send(Ok(entries)).unwrap();
        };
        tokio::time::timeout(Duration::from_secs(3), async {
            tokio::join!(open, history_owner);
        })
        .await
        .expect("slash acceptance records before dispatch");
        state.restore_prompt("unfinished Ω draft".into());
        state.handle_key(KeyAction::Enter).await;
        fn success(text: &str) -> Result<(), String> {
            assert!(text.starts_with("MCP server: safe-label\nError: mcp opaque-diagnostic"));
            Ok(())
        }
        fn failure(text: &str) -> Result<(), String> {
            success(text)?;
            Err("transport failed".into())
        }
        let mut deck = LoopState {
            copy_transport: Some(if succeeds { success } else { failure }),
            ..Default::default()
        };
        handle_event(
            &app,
            &mut state,
            &mut deck,
            CEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
        )
        .await
        .unwrap();
        assert!(
            state.take_copy_request().is_none(),
            "key request must reach transport"
        );
        assert_eq!(state.input(), "unfinished Ω draft");
        let detail = oc_tui::views::render_test(&state, 120, 40).join("\n");
        assert_eq!(detail.contains("✓ copied"), succeeds);
        assert_eq!(detail.contains("c copy details"), !succeeds);
        assert!(
            inbox.try_recv().is_err(),
            "copy neither submits nor reconnects"
        );
    }
}

#[tokio::test]
async fn selecting_model_updates_local_draft_without_owner_commit_or_selection_toast() {
    use oc_core::queries::ModelEntry;

    let (app, mut inbox, _) = CoreApp::channel(4);
    let session = SessionId::new("model-selection").unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut selected = catalog();
    selected.models = ["first", "second"]
        .into_iter()
        .map(|id| ModelEntry {
            id: id.into(),
            display_name: id.into(),
            provider_name: "fixture".into(),
            price: None,
            variants: Vec::new(),
            context: 0,
            context_known: false,
            output: 0,
            output_known: false,
            variants_known: true,
            capabilities: Default::default(),
            ..Default::default()
        })
        .collect();
    selected.model_id = "first".into();
    state.apply_catalog(selected.clone());
    assert_eq!(state.active_model_label(), Some(("first".into(), None)));
    assert_eq!(state.note(), None);

    apply_intent(
        &app,
        &mut state,
        &mut LoopState::default(),
        PanelIntent::SelectModel {
            id: "second".into(),
            recent: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(state.active_model_label(), Some(("second".into(), None)));
    assert_eq!(state.note(), None);
    assert!(inbox.try_recv().is_err(), "picker choice is only a draft");
}

#[tokio::test]
async fn home_model_pick_is_remembered_as_the_home_draft_without_sending() {
    use oc_core::queries::ModelEntry;

    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    let mut selected = catalog();
    selected.models = ["first", "second"]
        .into_iter()
        .map(|id| ModelEntry {
            id: id.into(),
            display_name: id.into(),
            variants_known: true,
            ..Default::default()
        })
        .collect();
    selected.model_id = "first".into();
    state.apply_catalog(selected);
    let owner = async {
        let Some(InboxMsg::HomeSelection {
            action: SelectionAction::Commit(commit),
            ack,
        }) = tokio::time::timeout(Duration::from_secs(5), inbox.recv())
            .await
            .ok()
            .flatten()
        else {
            panic!("a Home pick must become the durable Home draft at once");
        };
        let _ = ack.send(Err(CoreError::TurnBusy));
        commit
    };
    let mut deck = LoopState::default();
    let pick = PanelIntent::SelectModel {
        id: "second".into(),
        recent: None,
    };
    let (commit, applied) = tokio::join!(owner, apply_intent(&app, &mut state, &mut deck, pick));
    applied.unwrap();
    assert_eq!((commit.model_id.as_str(), commit.variant), ("second", None));
    assert_eq!(
        state.active_model_label(),
        Some(("second".into(), None)),
        "a refused memory write never undoes the draft"
    );
    assert!(inbox.try_recv().is_err(), "a pick never submits");
}

#[tokio::test]
async fn home_model_pick_outlives_a_restart_over_the_configured_model() {
    use std::collections::BTreeMap;

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(
        project.join("opencode.json"),
        serde_json::json!({
            "model": "fixture/m",
            "providers": {"fixture": {"package": "@ai-sdk/openai",
                "settings": {"baseURL": "http://127.0.0.1:9/v1", "apiKey": "fixture-key"},
                "models": {"m": {}, "n": {"variants": [
                    {"id": "high", "settings": {"reasoningEffort": "high"}}]}}}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([("HOME".into(), root.path().to_string_lossy().to_string())]);
    let data = root.path().join("data");
    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env.clone())
        .await
        .unwrap();
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(app.home_selection(SelectionAction::Current).await.unwrap());
    let mut deck = LoopState::default();
    let pick = PanelIntent::SelectModel {
        id: "n".into(),
        recent: None,
    };
    let choose = PanelIntent::ChooseModel {
        id: "n".into(),
        variant: Some("high".into()),
    };
    for intent in [pick, choose] {
        apply_intent(&app, &mut state, &mut deck, intent)
            .await
            .unwrap();
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env)
        .await
        .unwrap();
    let home = app.home_selection(SelectionAction::Current).await.unwrap();
    assert_eq!(
        (home.model_id.as_str(), home.variant.as_deref()),
        ("n", Some("high")),
        "the manual Home choice and effort survive quitting without a send"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn selecting_agent_updates_owner_and_draft_without_selection_toast() {
    use oc_core::queries::{AgentEntry, ModelEntry};
    for (open_picker, existing_note) in [
        (true, None),
        (true, Some("unrelated warning")),
        (false, None),
    ] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new(app.clone(), SessionId::new("agent-selection").unwrap());
        let mut selected = catalog();
        selected.models.push(ModelEntry {
            id: selected.model_id.clone(),
            display_name: "Fixture".into(),
            provider_name: "fixture".into(),
            price: None,
            variants: Vec::new(),
            context: 0,
            context_known: false,
            output: 0,
            output_known: false,
            variants_known: true,
            capabilities: Default::default(),
            ..Default::default()
        });
        selected.agents = ["build", "build-yolo"]
            .into_iter()
            .enumerate()
            .map(|(color_index, id)| AgentEntry {
                id: id.into(),
                color_index,
                description: String::new(),
                model: None,
                variant: None,
            })
            .collect();
        selected.agent_id = Some("build".into());
        state.apply_catalog(selected.clone());
        state.restore_prompt("preserved draft".into());
        if let Some(note) = existing_note {
            state.push_note(note);
        }
        let mut scroll = None;
        if open_picker {
            state.handle_key(oc_tui::events::KeyAction::Agents).await;
        } else {
            state.attach_page(&oc_core::queries::HistoryPage {
                rows: vec![oc_core::queries::HistoryMessage {
                    images: Vec::new(),
                    id: oc_core::session::MessageId("cycle-history".into()),
                    seq: 1,
                    role: Role::User,
                    text: "retained history row\n".repeat(80),
                    turn: None,
                    model_switch: None,
                    child: None,
                    shell_notice: None,
                    user_shell: None,
                }],
                total: 1,
                ..Default::default()
            });
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| oc_tui::shell::render(frame, &state))
                .unwrap();
            state.scroll_transcript(true);
            state.wheel_transcript_at(true, 1, Instant::now());
            assert!(state.scroll() > 0);
            let deadline = state.next_scroll_animation_deadline().unwrap();
            scroll = Some((state.scroll(), deadline));
        }
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::SessionSelection { action, ack, .. }) = inbox.recv().await else {
                panic!("selection")
            };
            assert_eq!(action, SelectionAction::Agent("build-yolo".into()));
            selected.agent_id = Some("build-yolo".into());
            ack.send(Ok(selected)).unwrap();
            assert!(inbox.try_recv().is_err());
        });
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::SelectAgent {
                id: "build-yolo".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(state.active_agent(), Some("build-yolo"));
        assert_eq!(state.input(), "preserved draft");
        assert_eq!(state.panel(), &oc_tui::app::TuiPanel::None);
        assert_eq!(state.note(), existing_note);
        if let Some((position, deadline)) = scroll {
            assert_eq!(state.scroll(), position);
            assert_eq!(state.next_scroll_animation_deadline(), Some(deadline));
        }
        worker.await.unwrap();
    }
    // Owner failures stay errors and leave the dialog/draft available.
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(catalog());
    state.restore_prompt("retry draft".into());
    state.handle_key(oc_tui::events::KeyAction::Agents).await;
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("Home selection")
        };
        ack.send(Err(CoreError::Application("unknown agent".into())))
            .unwrap();
    });
    let error = apply_intent(
        &app,
        &mut state,
        &mut LoopState::default(),
        PanelIntent::SelectAgent {
            id: "missing".into(),
        },
    )
    .await
    .unwrap_err();
    assert!(error.contains("unknown agent"));
    assert_eq!(state.panel(), &oc_tui::app::TuiPanel::Agents);
    assert_eq!(state.input(), "retry draft");
    worker.await.unwrap();
}

#[tokio::test]
async fn variant_cycle_drafts_catalog_order_and_stale_default_rules_in_home_and_session() {
    use oc_core::queries::{ModelEntry, VariantEntry};
    for home in [true, false] {
        for (current, expected) in [
            (None, Some("zeta")),
            (Some("default"), Some("zeta")),
            (Some("zeta"), Some("alpha")),
            (Some("alpha"), None),
            (Some("retired"), None),
        ] {
            let (app, mut inbox, _) = CoreApp::channel(4);
            let mut state = if home {
                TuiState::new_home(app.clone())
            } else {
                TuiState::new(app.clone(), SessionId::new("cycle-owner").unwrap())
            };
            state.restore_prompt("next accepted prompt".into());
            let mut selected = catalog();
            selected.models = vec![ModelEntry {
                id: selected.model_id.clone(),
                display_name: "Dynamic model".into(),
                provider_name: "fixture".into(),
                price: None,
                variants: [
                    ("default", false),
                    ("zeta", false),
                    ("disabled", true),
                    ("alpha", false),
                ]
                .into_iter()
                .map(|(name, disabled)| VariantEntry {
                    name: name.into(),
                    disabled,
                    reasoning_effort: Some(name.into()),
                })
                .collect(),
                context: 0,
                context_known: false,
                output: 0,
                output_known: false,
                variants_known: true,
                capabilities: Default::default(),
                ..Default::default()
            }];
            selected.variant = current.map(str::to_string);
            state.apply_catalog(selected);
            state.push_note("unrelated warning");
            let intent = state
                .handle_key(oc_tui::events::KeyAction::CycleVariant)
                .await
                .intent
                .unwrap();
            // The cycled effort is remembered at once (pinned `saveVariant`).
            // Home also records the durable Home draft first.
            let owner = async {
                if home {
                    let Some(InboxMsg::HomeSelection {
                        action: SelectionAction::Commit(commit),
                        ack,
                    }) = inbox.recv().await
                    else {
                        panic!("expected an immediate Home draft write");
                    };
                    assert_eq!(commit.variant.as_deref(), expected);
                    let _ = ack.send(Err(CoreError::TurnBusy));
                }
                let Some(InboxMsg::ModelPreference { action, ack }) = inbox.recv().await else {
                    panic!("expected an immediate effort memory write");
                };
                let _ = ack.send(Ok(Default::default()));
                action
            };
            let mut deck = LoopState::default();
            let (remembered, applied) =
                tokio::join!(owner, apply_intent(&app, &mut state, &mut deck, intent));
            applied.unwrap();
            assert!(matches!(
                remembered,
                Some(oc_core::queries::ModelPreferenceAction::Variant { variant, .. })
                    if variant.as_deref() == expected
            ));
            assert_eq!(
                state.active_model_label(),
                Some(("Dynamic model".into(), expected.map(str::to_string)))
            );
            assert_eq!(state.input(), "next accepted prompt");
            assert_eq!(state.panel(), &oc_tui::app::TuiPanel::None);
            assert_eq!(state.note(), Some("unrelated warning"));
            assert!(
                inbox.try_recv().is_err(),
                "cycle must neither commit nor submit"
            );
        }
    }
}

#[tokio::test]
async fn variant_cycle_missing_models_or_named_variants_is_noop_and_child_is_readonly() {
    use oc_core::queries::{ModelEntry, VariantEntry};
    for missing_model in [true, false] {
        let (app, mut inbox, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app.clone());
        let mut selected = catalog();
        if !missing_model {
            selected.models.push(ModelEntry {
                id: selected.model_id.clone(),
                display_name: "No variants".into(),
                provider_name: "fixture".into(),
                price: None,
                variants: vec![
                    VariantEntry {
                        name: "default".into(),
                        disabled: false,
                        reasoning_effort: None,
                    },
                    VariantEntry {
                        name: "disabled".into(),
                        disabled: true,
                        reasoning_effort: None,
                    },
                ],
                context: 0,
                context_known: false,
                output: 0,
                output_known: false,
                variants_known: true,
                capabilities: Default::default(),
                ..Default::default()
            });
        }
        state.apply_catalog(selected);
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::CycleVariant,
        )
        .await
        .unwrap();
        assert!(inbox.try_recv().is_err());
        let mut deck = LoopState {
            read_only: true,
            ..Default::default()
        };
        assert!(
            apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
                .await
                .unwrap_err()
                .contains("read-only")
        );
        assert!(inbox.try_recv().is_err());
    }
}

#[tokio::test]
async fn variant_cycle_persists_and_next_real_request_uses_selected_overlay() {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::fs::write(
        project.join("opencode.json"),
        serde_json::json!({
            "model":"fixture/m", "providers":{"fixture":{"package":"@ai-sdk/openai",
                "settings":{"baseURL":format!("http://{address}/v1"),"apiKey":"fixture-key"},
                "models":{"m":{"variants":[{"id": "custom", "settings": {"reasoningEffort": "high"}}]}}}}
        })
        .to_string(),
    )
    .unwrap();
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut requests = Vec::<serde_json::Value>::new();
        while requests.len() < 2 && std::time::Instant::now() < deadline {
            let mut socket = match listener.accept() {
                Ok((socket, _)) => socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("fake provider: {error}"),
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let end = loop {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(pos) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    break pos + 4;
                }
            };
            let length: usize = String::from_utf8_lossy(&bytes[..end])
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < end + length {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            requests.push(serde_json::from_slice(&bytes[end..end + length]).unwrap());
            let item = serde_json::json!({"type":"message","role":"assistant","id":"a","status":"completed","content":[{"type":"output_text","text":"done"}]});
            let sse = format!(
                "data: {}\n\ndata: {}\n\n",
                serde_json::json!({"type":"response.output_item.done","item":item}),
                serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item],"usage":{"input_tokens":1,"output_tokens":1}}})
            );
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}", sse.len()).unwrap();
        }
        assert_eq!(requests.len(), 2);
        requests
    });
    let env = BTreeMap::from([
        ("HOME".into(), root.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let data = root.path().join("data");
    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env.clone())
        .await
        .unwrap();
    let session = SessionId::new("cycle-wire").unwrap();
    app.create_session(session.clone()).await.unwrap();
    app.rename_session(session.clone(), "Already titled".into())
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    let mut deck = LoopState::default();
    state.apply_catalog(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap(),
    );
    apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
        .await
        .unwrap();
    assert_eq!(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("custom"),
        "a cycled effort is remembered before any send"
    );
    state.handle_key(KeyAction::Enter).await;
    assert_eq!(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("custom")
    );
    state.poll_submission();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = oc_adapters::application::spawn_with_env(&project, &data, env)
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), session.clone());
    state.apply_catalog(
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .unwrap(),
    );
    assert_eq!(
        state.active_model_label().unwrap().1.as_deref(),
        Some("custom")
    );
    let mut events = app.subscribe();
    for (index, prompt) in ["named variant request", "default variant request"]
        .into_iter()
        .enumerate()
    {
        if index == 1 {
            apply_intent(&app, &mut state, &mut deck, PanelIntent::CycleVariant)
                .await
                .unwrap();
        }
        state.handle_paste(prompt);
        state.handle_key(KeyAction::Enter).await;
        loop {
            let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
                .await
                .unwrap()
                .unwrap();
            let finished = matches!(event, CoreEvent::TurnFinished { .. });
            assert!(!matches!(event, CoreEvent::TurnFailed { .. }), "{event:?}");
            state.poll_submission();
            handle_worker_event(&app, &mut state, &mut deck, &session, event)
                .await
                .unwrap();
            if finished {
                break;
            }
        }
    }
    assert_eq!(
        app.session_selection(session, false, SelectionAction::Current)
            .await
            .unwrap()
            .variant,
        None
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let requests = server.join().unwrap();
    assert_eq!(requests[0]["model"], "m");
    assert_eq!(requests[1]["model"], "m");
    assert_eq!(requests[0]["reasoning"]["effort"], "high");
    assert!(requests[1]["reasoning"]["effort"].is_null());
}

#[test]
fn vis27_parked_tabs_and_home_use_current_owner_mode_on_activation() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, _inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    old.chrome.session_tps = Some(true);
    let measured_page = tps_page();
    let mut current = old.clone();
    current.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    current.chrome.session_tps = Some(false);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&measured_page);
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(current);
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    assert!(
        !state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );

    let mut home = TuiState::new_home(app.clone());
    home.apply_catalog(old);
    deck.home = Some(home);
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    // A now-unconfigured owner snapshot resets every restored route.
    state.apply_catalog(catalog());
    deck.activate(&mut state, 1).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::default());
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::default());
    deck.activate(&mut state, 0).unwrap();
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s")),
        "missing setting restores pinned default without losing stats"
    );
}

#[tokio::test]
async fn pending_owner_reload_allows_resize_and_quit_without_dropping_owner_receipt() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new_home(app.clone());
    state.handle_paste("/reload");
    let mut deck = LoopState::default();
    let owner = app.clone();
    deck.reload_job = Some(tokio::spawn(async move { owner.reload_location().await }));
    let Some(InboxMsg::ReloadLocation { ack }) = inbox.recv().await else {
        panic!("owner reload must be in flight")
    };
    let pointer = (2, 3, ratatui::layout::Rect::new(0, 0, 80, 24));
    state.restore_mouse_hover(pointer);
    assert_eq!(state.mouse_position(), Some(pointer));
    handle_event(&app, &mut state, &mut deck, CEvent::Resize(100, 30))
        .await
        .unwrap();
    // Resize preserves an in-frame physical pointer, not any old painted
    // action receipt; the pending reload still owns the unchanged view/draft.
    assert_eq!(
        state.mouse_position(),
        Some((2, 3, ratatui::layout::Rect::new(0, 0, 100, 30)))
    );
    assert_eq!(state.status(), &TuiStatus::Idle);
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
    )
    .await
    .unwrap();
    assert_eq!(state.status(), &TuiStatus::Quit);
    assert_eq!(state.input(), "/reload");
    assert!(!deck.reload_job.as_ref().unwrap().is_finished());
    // The owner can complete its transaction after terminal quit: no
    // dropped receiver or optimistic success/selection replacement.
    ack.send(Err(CoreError::Shutdown)).unwrap();
    assert!(deck.reload_job.take().unwrap().await.unwrap().is_err());
}

#[tokio::test]
async fn refused_reload_consumes_provider_facts_before_feedback_and_late_hint() {
    use oc_core::queries::{
        McpBinding, McpSnapshot, ProviderReadiness, ProviderStatus, ServiceAction, ServiceCode,
        ServiceDiagnostic, ServiceKind, ServiceStage,
    };
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut current = catalog();
    current.chrome.location = Some("/fixture".into());
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(current.clone());
    state.handle_paste("/reload");
    let mut deck = LoopState::default();
    let diagnostic = ServiceDiagnostic {
        kind: ServiceKind::Provider,
        service: "fixture-provider".into(),
        source: "source-fixture/provider".into(),
        field: vec!["models".into()],
        stage: ServiceStage::Query,
        code: ServiceCode::QueryFailed,
        action: ServiceAction::RetryConnection,
        details: None,
    };
    current.chrome.service_diagnostics.push(diagnostic.clone());
    current.chrome.provider = Some(ProviderReadiness {
        service: "fixture-provider".into(),
        model: "opaque-model".into(),
        status: ProviderStatus::Failed,
        catalog_status: ProviderStatus::Failed,
        diagnostic: Some(diagnostic.clone()),
    });
    let mcp = McpSnapshot {
        binding: McpBinding {
            location: "/fixture".into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: Vec::new(),
    };
    let worker = tokio::spawn(async move {
        // Support the old MCP-only finisher too: the feedback assertion below
        // must fail on its ordering, not on a mock query-shape mismatch.
        match inbox.recv().await.unwrap() {
            InboxMsg::HomeSelection { ack, .. } => {
                ack.send(Ok(current.clone())).unwrap();
                let Some(InboxMsg::McpStatus { ack }) = inbox.recv().await else {
                    panic!("current MCP facts")
                };
                ack.send(Ok(mcp.clone())).unwrap();
            }
            InboxMsg::McpStatus { ack } => {
                ack.send(Ok(mcp.clone())).unwrap();
            }
            _ => panic!("current refusal facts"),
        }
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("late provider hint")
        };
        ack.send(Ok(current)).unwrap();
        let Some(InboxMsg::McpStatus { ack }) = inbox.recv().await else {
            panic!("late MCP facts")
        };
        ack.send(Ok(mcp)).unwrap();
    });
    let message = "Configuration reload failed; check native settings";
    finish_reload_refusal(&app, &mut state, &mut deck, message.into()).await;
    assert_eq!(state.note(), Some(message));
    refresh_provider_views(&app, &mut state, &mut deck).await;
    assert_eq!(
        state.note(),
        Some(message),
        "same current provider facts cannot replace the explicit operation outcome before its paint"
    );
    assert_eq!(state.note_variant(), Some(NoteVariant::Error));
    assert!(state.chrome.service_diagnostics.contains(&diagnostic));
    assert_eq!(state.input(), "/reload");
    assert_eq!(state.status(), &TuiStatus::Idle);
    worker.await.unwrap();
}

#[tokio::test]
async fn conn05_provider_publication_refreshes_mounted_parked_and_home_filtered_views() {
    use oc_core::queries::{McpBinding, McpSnapshot};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    let mut browse = old.clone();
    browse.provider = "custom-exact".into();
    browse.chrome.custom_connection = true;
    let mut state = TuiState::new(app.clone(), SessionId("mounted".into()));
    let mut parked = TuiState::new(app.clone(), SessionId("parked".into()));
    let mut home = TuiState::new_home(app.clone());
    for view in [&mut state, &mut parked, &mut home] {
        view.apply_catalog(old.clone());
        view.handle_key(KeyAction::Commands).await;
        view.handle_paste("Switch model");
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            view.handle_key(KeyAction::Enter),
        )
        .await
        .unwrap();
        assert_eq!(view.panel(), &TuiPanel::Model);
        view.apply_picker_catalog(browse.clone());
        assert_eq!(view.picker_provider_filter(), Some("custom-exact"));
        view.restore_prompt("captured draft".into());
    }
    let mut deck = LoopState {
        tabs: vec![Some(parked)],
        home: Some(home),
        ..Default::default()
    };
    let mut fresh = browse;
    fresh.models = vec![oc_core::queries::ModelEntry {
        id: "new/exact-model".into(),
        display_name: "CONN05 supplied fresh metadata".into(),
        ..Default::default()
    }];
    let worker = tokio::spawn(async move {
        for expected in [Some("mounted"), Some("parked"), None] {
            match (inbox.recv().await.unwrap(), expected) {
                (InboxMsg::SessionSelection { session, ack, .. }, Some(expected)) => {
                    assert_eq!(session.0, expected);
                    ack.send(Ok(old.clone())).unwrap();
                }
                (InboxMsg::HomeSelection { ack, .. }, None) => {
                    ack.send(Ok(old.clone())).unwrap();
                }
                _ => panic!("publication must query the original view owner"),
            }
            let InboxMsg::ProviderCatalog { provider, ack } = inbox.recv().await.unwrap() else {
                panic!("filtered provider view")
            };
            assert_eq!(provider, "custom-exact");
            ack.send(Ok(fresh.clone())).unwrap();
        }
        let InboxMsg::McpStatus { ack } = inbox.recv().await.unwrap() else {
            panic!("surviving MCP facts")
        };
        ack.send(Ok(McpSnapshot {
            binding: McpBinding {
                location: "/fixture".into(),
                generation: 1,
                instance: 1,
            },
            revision: 1,
            servers: vec![],
        }))
        .unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "metadata refresh must not issue a selection mutation"
        );
    });
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        refresh_provider_views(&app, &mut state, &mut deck),
    )
    .await
    .unwrap();
    worker.await.unwrap();
    for view in [
        &state,
        deck.tabs[0].as_ref().unwrap(),
        deck.home.as_ref().unwrap(),
    ] {
        assert_eq!(view.input(), "captured draft");
        assert_eq!(view.picker_provider_filter(), Some("custom-exact"));
        assert!(
            oc_tui::views::panel_lines(view)
                .join("\n")
                .contains("CONN05 supplied fresh metadata")
        );
    }
    assert_eq!(state.attached_session().unwrap().0, "mounted");
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().attached_session().unwrap().0,
        "parked"
    );
    assert!(deck.home.as_ref().unwrap().attached_session().is_none());
}

#[tokio::test]
async fn failed_selection_after_owner_reload_keeps_every_view_and_draft() {
    use oc_core::queries::{
        McpBinding, McpServerSnapshot, McpSnapshot, McpStatus, TerminalCopyMode,
    };
    use oc_tui::app::ClipboardMode;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    old.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    old.chrome.session_tps = Some(true);
    old.commands = vec!["old-command".into()];
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(old.clone());
    state.handle_paste("/reload");
    let captured = vis27_begin_prompt_drag(&mut state, "/reload");
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    let mut fresh = old.clone();
    fresh.commands = vec!["fresh-command".into()];
    fresh.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    fresh.chrome.session_tps = Some(false);
    let published = fresh.clone();
    let current_mcp = McpSnapshot {
        binding: McpBinding {
            location: "/fixture".into(),
            generation: 1,
            instance: 1,
        },
        revision: 2,
        servers: vec![McpServerSnapshot {
            id: "coalesced-server".into(),
            name: "safe-service".into(),
            configured_enabled: true,
            status: McpStatus::Failed,
            pending_action: None,
            tools: 0,
            diagnostic: None,
            actions: Vec::new(),
        }],
    };
    let worker_mcp = current_mcp.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("parked session selection")
        };
        ack.send(Ok(fresh)).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("active session selection")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        for _ in 0..2 {
            let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
                panic!("refusal refreshes current active/parked provider facts")
            };
            // A failed read must not replace either captured route or its
            // already-published safety-sensitive settings.
            ack.send(Err(CoreError::Shutdown)).unwrap();
        }
        let Some(InboxMsg::McpStatus { ack }) = inbox.recv().await else {
            panic!("refused reload consumes the surviving owner's current facts")
        };
        ack.send(Ok(worker_mcp)).unwrap();
    });
    let error = finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/fixture".into(),
            generation: 2,
            catalog: published,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        Some("/reload".into()),
    )
    .await
    .unwrap_err();
    assert!(error.contains("selection refresh failed"));
    state.apply_intent_error(error);
    assert!(state.note().unwrap().contains("reload incomplete"));
    assert_eq!(state.input(), "/reload");
    assert!(state.is_workspace_command("/old-command"));
    assert!(!state.is_workspace_command("/fresh-command"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(
        vis27_release_prompt_drag(&mut state, captured),
        None,
        "published Manual cancels the old automatic gesture even after route read failure"
    );
    let captured = vis27_begin_prompt_drag(&mut state, "/reload");
    assert_eq!(vis27_release_prompt_drag(&mut state, captured), None);
    for view in [&state, deck.tabs[0].as_ref().unwrap()] {
        let text = view
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .map(oc_tui::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Build · Measured model · 1.5s"), "{text}");
        assert!(
            !text.contains("tok/s"),
            "published setting survives read failure: {text}"
        );
    }
    assert!(
        deck.tabs[0]
            .as_ref()
            .unwrap()
            .is_workspace_command("/old-command")
    );
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Manual
    );
    deck.reload_job = Some(tokio::spawn(async { Err(CoreError::Shutdown) }));
    apply_mcp_to_views(&mut state, &mut deck, current_mcp);
    state.handle_key(KeyAction::Cancel).await; // dismiss /reload autocomplete, retaining the draft
    state.handle_key(KeyAction::Commands).await;
    assert_eq!(state.panel(), &TuiPanel::Commands);
    state.handle_paste("mcp");
    state.handle_panel_key(KeyAction::Enter);
    assert_eq!(state.panel(), &TuiPanel::Mcps);
    assert!(
        state.modal_options().is_empty(),
        "hint is coalesced while rebuilding"
    );
    let refusal = deck.reload_job.take().unwrap().await.unwrap().unwrap_err();
    finish_reload_refusal(&app, &mut state, &mut deck, reload_error(refusal)).await;
    worker.await.unwrap();
    assert!(
        state
            .note()
            .unwrap()
            .contains("Configuration reload failed")
    );
    for view in [&mut state, deck.tabs[0].as_mut().unwrap()] {
        if view.panel() != &TuiPanel::Mcps {
            view.handle_key(KeyAction::Commands).await;
            view.handle_paste("mcp");
            view.handle_panel_key(KeyAction::Enter);
        }
        assert_eq!(view.panel(), &TuiPanel::Mcps);
        assert!(
            view.modal_options()
                .iter()
                .any(|row| { row.title == "safe-service" && row.footer.contains("Failed") })
        );
    }
    assert!(
        state
            .note()
            .unwrap()
            .contains("Configuration reload failed")
    );
    assert_eq!(state.input(), "/reload");
}

fn vis27_begin_prompt_drag(
    state: &mut TuiState,
    text: &str,
) -> (ratatui::layout::Rect, crossterm::event::MouseEvent) {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    state.restore_prompt(text.into());
    let area = Rect::new(0, 0, 120, 40);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| oc_tui::shell::render(frame, state))
        .unwrap();
    let (x, y) = (0..40)
        .rev()
        .find_map(|y| {
            let row = (0..120)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>();
            row.find(text)
                .map(|at| (row[..at].chars().count() as u16, y))
        })
        .unwrap();
    let event = |kind, x| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(event(MouseEventKind::Down(MouseButton::Left), x), area);
    state.handle_mouse(
        event(
            MouseEventKind::Drag(MouseButton::Left),
            x + text.len() as u16,
        ),
        area,
    );
    (
        area,
        event(MouseEventKind::Up(MouseButton::Left), x + text.len() as u16),
    )
}

fn vis27_release_prompt_drag(
    state: &mut TuiState,
    (area, event): (ratatui::layout::Rect, crossterm::event::MouseEvent),
) -> Option<String> {
    state.handle_mouse(event, area);
    let mut copied = None;
    report_copy_request_with(state, |text| {
        copied = Some(text.to_owned());
        Ok(())
    });
    assert!(
        state.take_copy_request().is_none(),
        "producing view is drained exactly once"
    );
    copied
}

#[tokio::test]
async fn vis27_mismatched_published_reload_disables_copy_until_owner_catalog() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut current = catalog();
    current.chrome.location = Some("/B".into());
    current.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(current.clone());
    let mut deck = LoopState {
        location: Some("/B".into()),
        ..Default::default()
    };
    let error = finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/A".into(),
            generation: 2,
            catalog: current.clone(),
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(error.contains("Location changed"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Disabled);
    let captured = vis27_begin_prompt_drag(&mut state, "copy draft");
    assert_eq!(vis27_release_prompt_drag(&mut state, captured), None);
    assert_eq!(state.input(), "copy draft");
    assert!(inbox.try_recv().is_err());
    state.apply_catalog(current);
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    let captured = vis27_begin_prompt_drag(&mut state, "copy draft");
    assert_eq!(
        vis27_release_prompt_drag(&mut state, captured),
        Some("copy draft".into())
    );
    assert_eq!(state.input(), "copy draft");
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn vis27_reload_projects_owner_mode_to_active_parked_and_home() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut old = catalog();
    old.chrome.location = Some("/fixture".into());
    old.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    old.chrome.session_tps = Some(true);
    let mut state = TuiState::new(app.clone(), SessionId::new("first").unwrap());
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut deck = LoopState {
        location: Some("/fixture".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    append_tab(&app, &mut deck, &mut state, "second");
    state.apply_catalog(old.clone());
    state.attach_page(&tps_page());
    assert!(
        state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let mut home = TuiState::new_home(app.clone());
    home.apply_catalog(old.clone());
    deck.home = Some(home);

    let mut fresh = old.clone();
    fresh.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    fresh.chrome.session_tps = Some(false);
    let returned = fresh.clone();
    let worker = tokio::spawn(async move {
        for expected in ["first", "second"] {
            let Some(InboxMsg::SessionSelection { session, ack, .. }) = inbox.recv().await else {
                panic!("refresh session selection")
            };
            assert_eq!(session.0, expected);
            ack.send(Ok(fresh.clone())).unwrap();
        }
        let Some(InboxMsg::HomeSelection { ack, .. }) = inbox.recv().await else {
            panic!("refresh Home selection")
        };
        ack.send(Ok(fresh.clone())).unwrap();
        let Some(InboxMsg::Catalog { ack }) = inbox.recv().await else {
            panic!("verify Location catalog")
        };
        ack.send(Ok(fresh)).unwrap();
        empty_mcp_status(&mut inbox).await;
        assert!(inbox.try_recv().is_err(), "reload does not save tab deck");
    });
    finish_reload(
        &app,
        &mut state,
        &mut deck,
        ReloadLocationSnapshot {
            location: "/fixture".into(),
            generation: 2,
            catalog: returned,
            diagnostics: Vec::new(),
            notices: Vec::new(),
        },
        None,
    )
    .await
    .unwrap();
    worker.await.unwrap();
    for view in [&state, deck.tabs[0].as_ref().unwrap()] {
        let text = view
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .map(oc_tui::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Build · Measured model · 1.5s"), "{text}");
        assert!(
            !text.contains("tok/s"),
            "fresh owner setting must replace warmed footer: {text}"
        );
        let meta = view
            .history()
            .rows()
            .iter()
            .find_map(|row| row.meta.as_ref())
            .unwrap();
        assert_eq!(meta.streamed_ms, Some(4000));
        assert_eq!(meta.output_tokens, Some(200));
    }
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    let captured = vis27_begin_prompt_drag(&mut state, "copy draft");
    assert_eq!(
        vis27_release_prompt_drag(&mut state, captured),
        Some("copy draft".into())
    );
    assert_eq!(
        deck.tabs[0].as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Select
    );
    assert_eq!(
        deck.home.as_ref().unwrap().clipboard_mode(),
        ClipboardMode::Select
    );
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    let captured = vis27_begin_prompt_drag(&mut state, "copy draft");
    assert_eq!(
        vis27_release_prompt_drag(&mut state, captured),
        Some("copy draft".into())
    );
    assert!(
        !state
            .visible_transcript(80, 80, 40)
            .0
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    assert!(deck.restore_home(&mut state));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Select);
    let captured = vis27_begin_prompt_drag(&mut state, "copy draft");
    assert_eq!(
        vis27_release_prompt_drag(&mut state, captured),
        Some("copy draft".into())
    );
}

#[tokio::test]
async fn vis27_invalid_owner_location_does_not_replace_mode_or_write_tabs() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;

    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut current = catalog();
    current.chrome.location = Some("/A".into());
    current.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    let mut state = TuiState::new_home(app.clone());
    state.apply_catalog(current);
    let mut deck = LoopState {
        location: Some("/A".into()),
        ..Default::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SwitchLocationHome { ack, .. }) = inbox.recv().await else {
            panic!("owner Location switch")
        };
        ack.send(Err(CoreError::Application("invalid configuration".into())))
            .unwrap();
        let Some(InboxMsg::ReloadLocation { ack }) = inbox.recv().await else {
            panic!("owner reload")
        };
        ack.send(Err(CoreError::Application("invalid configuration".into())))
            .unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "failed config must not save tabs"
        );
    });
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchLocation { path: "/B".into() },
        )
        .await
        .is_err()
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ReloadConfiguration,
    )
    .await
    .unwrap();
    assert!(deck.reload_job.take().unwrap().await.unwrap().is_err());
    worker.await.unwrap();
    assert_eq!(state.chrome.location.as_deref(), Some("/A"));
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.location.as_deref(), Some("/A"));
}

#[tokio::test]
async fn published_location_uses_new_owner_binding_and_token_for_next_save() {
    use oc_core::queries::TerminalCopyMode;
    use oc_tui::app::ClipboardMode;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut b_catalog = catalog();
    b_catalog.chrome.terminal_copy = Some(TerminalCopyMode::Manual);
    let b_session_catalog = b_catalog.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::TabDeck { ack }) = inbox.recv().await else {
            panic!("read B preference")
        };
        ack.send(Ok(TabDeckSnapshot {
            location: "/B".into(),
            revision: Some("B-loaded".into()),
            new_session_titles: Vec::new(),
            sessions: vec![SessionId::new("b-root").unwrap()],
            active: None,
            rail_width: Some(5),
        }))
        .unwrap();
        let Some(InboxMsg::History { session, ack, .. }) = inbox.recv().await else {
            panic!("read B root")
        };
        assert_eq!(session.0, "b-root");
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("read B catalog")
        };
        ack.send(Ok(b_session_catalog)).unwrap();
        empty_compactions(&mut inbox).await;
        empty_mcp_status_at(&mut inbox, "/B").await;
        empty_shell_inventory(&mut inbox, "b-root").await;
        let Some(InboxMsg::SaveTabDeck { deck, ack }) = inbox.recv().await else {
            panic!("save B route")
        };
        assert_eq!(deck.location, "/B");
        assert_eq!(deck.revision.as_deref(), Some("B-loaded"));
        assert_eq!(deck.sessions, vec![SessionId::new("b-root").unwrap()]);
        assert_eq!(deck.rail_width, Some(5));
        ack.send(Ok(TabDeckSnapshot {
            revision: Some("B-saved".into()),
            ..deck
        }))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    let mut state = TuiState::new(app.clone(), SessionId::new("a-root").unwrap());
    let mut a_catalog = catalog();
    a_catalog.chrome.terminal_copy = Some(TerminalCopyMode::Select);
    state.apply_catalog(a_catalog);
    let mut deck = LoopState {
        location: Some("/A".into()),
        revision: Some("A-stale".into()),
        tabs: vec![None],
        tab_cards_before: vec![None],
        active_tab: Some(0),
        ..Default::default()
    };
    adopt_location(&app, &mut state, &mut deck, b_catalog, "/B").await;
    assert!(state.attached_session().is_none());
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.location.as_deref(), Some("/B"));
    assert_eq!(deck.revision.as_deref(), Some("B-loaded"));
    assert_eq!(state.tab_rail_preference(), Some(5));
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::ActivateTab { index: 0 },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "b-root");
    assert_eq!(state.clipboard_mode(), ClipboardMode::Manual);
    assert_eq!(deck.revision.as_deref(), Some("B-saved"));
    worker.await.unwrap();
}

#[tokio::test]
async fn close_inactive_tab_reindexes_active_view_and_cursor_without_owner_query() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("a").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("first draft");
    state.attach_page(&HistoryPage {
        rows: vec![HistoryMessage {
            images: Vec::new(),
            seq: 1,
            role: Role::User,
            id: oc_core::session::MessageId("viewport-user".into()),
            text: "first viewport marker".into(),
            turn: None,
            model_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
        }],
        total: 1,
        ..Default::default()
    });
    deck.cards_before = Some(11);
    append_tab(&app, &mut deck, &mut state, "b");
    state.handle_paste("middle draft");
    deck.cards_before = Some(22);
    append_tab(&app, &mut deck, &mut state, "c");
    state.handle_paste("active draft");
    deck.cards_before = Some(33);

    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 1 },
    )
    .await
    .unwrap();
    assert_eq!(deck.active_tab, Some(1));
    assert_eq!(deck.tab_cards_before, [Some(11), None]);
    assert_eq!(deck.cards_before, Some(33));
    assert_eq!(state.input(), "active draft");
    assert_eq!(state.tab_presentation().0.len(), 2);
    assert_eq!(state.tab_presentation().1, 1);
    assert!(inbox.try_recv().is_err(), "close cannot delete a session");
    deck.activate(&mut state, 0).unwrap();
    assert_eq!(state.input(), "first draft");
    assert!(
        state
            .viewport()
            .join("\n")
            .contains("first viewport marker")
    );
    assert_eq!(deck.cards_before, Some(11));
}

#[tokio::test]
async fn rail_release_saves_once_and_only_acknowledged_width_changes_the_deck() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app.clone(), SessionId("rail-root".into()));
    state.chrome.location = Some("/rail-location".into());
    state.chrome.vertical_tabs_width = 42;
    state.apply_tab_rail_width(Some(20)).unwrap();
    state.restore_prompt("kept rail draft Ω界".into());
    state.attach_page(&oc_core::queries::HistoryPage {
        rows: vec![oc_core::queries::HistoryMessage {
            images: Vec::new(),
            id: oc_core::session::MessageId("rail-history".into()),
            seq: 1,
            role: Role::User,
            text: "retained history row\n".repeat(80),
            turn: None,
            model_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
        }],
        total: 1,
        ..Default::default()
    });
    let caret = state.prompt_layout(78).1;
    let mut deck = LoopState {
        location: Some("/rail-location".into()),
        revision: Some("original-revision".into()),
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| oc_tui::shell::render(frame, &state))
        .unwrap();
    for _ in 0..3 {
        assert!(state.scroll_transcript(true).intent.is_none());
    }
    let scroll = state.scroll();
    assert_eq!(scroll, 3);
    let mouse = |kind, column| MouseEvent {
        kind,
        column,
        row: 10,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    for result in [0, 1, 2] {
        let before = deck.snapshot(&state);
        assert!(
            state
                .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 19), area)
                .intent
                .is_none()
        );
        assert!(
            state
                .handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 4), area)
                .intent
                .is_none()
        );
        assert_eq!(
            deck.snapshot(&state),
            before,
            "preview must not enter the durable candidate"
        );
        assert!(
            inbox.try_recv().is_err(),
            "Down/Drag cannot write or submit"
        );
        let outcome = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 4), area);
        assert_eq!(
            outcome.intent,
            Some(PanelIntent::SetTabRailWidth { width: 5 })
        );
        assert_eq!(state.tab_rail_preference(), Some(20));
        let expected = before.clone();
        let owner = async {
            let Some(InboxMsg::SaveTabDeck {
                deck: requested,
                ack,
            }) = inbox.recv().await
            else {
                panic!("one existing checked deck save")
            };
            let mut candidate = expected;
            candidate.rail_width = Some(5);
            assert_eq!(
                requested, candidate,
                "retain every root, Location and CAS token"
            );
            if result == 0 {
                ack.send(Err(CoreError::TabDeckConflict)).unwrap();
            } else {
                let mut saved = requested;
                saved.revision = Some("accepted-revision".into());
                if result == 1 {
                    saved.rail_width = Some(6);
                }
                ack.send(Ok(saved)).unwrap();
            }
        };
        let (finished, ()) = tokio::join!(
            apply_intent(&app, &mut state, &mut deck, outcome.intent.unwrap()),
            owner,
        );
        if result < 2 {
            assert!(finished.is_err());
            assert_eq!(deck.snapshot(&state), before);
        } else {
            finished.unwrap();
            assert_eq!(state.tab_rail_preference(), Some(5));
            assert_eq!(deck.revision.as_deref(), Some("accepted-revision"));
        }
        assert_eq!(state.session().0, "rail-root");
        assert_eq!(state.input(), "kept rail draft Ω界");
        assert_eq!(state.prompt_layout(78).1, caret);
        assert_eq!(state.scroll(), scroll);
        assert!(
            inbox.try_recv().is_err(),
            "no second save, provider request or effect"
        );
    }
    let accepted = deck.snapshot(&state);
    for guard in 0..5 {
        deck.read_only = guard == 0;
        deck.save_disabled = guard == 1;
        state.chrome.location = Some(
            if guard == 2 {
                "/foreign"
            } else {
                "/rail-location"
            }
            .into(),
        );
        let width = if guard == 3 {
            4
        } else if guard == 4 {
            73
        } else {
            20
        };
        assert!(
            apply_intent(
                &app,
                &mut state,
                &mut deck,
                PanelIntent::SetTabRailWidth { width }
            )
            .await
            .is_err()
        );
        assert_eq!(deck.snapshot(&state), accepted);
        assert!(
            inbox.try_recv().is_err(),
            "invalid/read-only saves never reach the owner"
        );
    }
}

#[tokio::test]
async fn close_last_real_tab_queries_current_home_before_discarding_and_reopens() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("durable").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    state.handle_paste("saved draft");
    state.apply_tab_rail_width(Some(5)).unwrap();
    let worker = tokio::spawn(async move {
        for result in [Err(CoreError::Shutdown), Ok(catalog())] {
            let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
                panic!("close must query Home only")
            };
            assert_eq!(action, SelectionAction::Current);
            ack.send(result).unwrap();
        }
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("Sessions reopen must query existing selection")
        };
        assert_eq!(session.0, "durable");
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: String::new(),
            catalog: catalog(),
            page: Default::default(),
            deck: TabDeckSnapshot {
                sessions: vec![session.clone()],
                active: Some(session),
                rail_width: Some(5),
                ..Default::default()
            },
            previous_deck: old_deck,
        }))
        .unwrap();
    });
    let intent = PanelIntent::CloseTab { index: 0 };
    assert!(
        apply_intent(&app, &mut state, &mut deck, intent.clone())
            .await
            .is_err()
    );
    assert_eq!(state.input(), "saved draft");
    assert_eq!(state.session().0, "durable");
    assert_eq!(deck.tabs.len(), 1);
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert!(state.attached_session().is_none());
    assert_eq!(deck.active_tab, None);
    assert!(deck.tabs.is_empty() && deck.tab_cards_before.is_empty());
    assert!(state.tab_presentation().0.is_empty());
    assert_eq!(state.tab_rail_preference(), Some(5));
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession {
            id: "durable".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "durable");
    assert_eq!(state.tab_rail_preference(), Some(5));
    assert_eq!(deck.tabs.len(), 1);
    assert_eq!(deck.active_tab, Some(0));
    assert!(deck.home.is_some(), "reopening parks synthetic Home");
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert!(state.attached_session().is_none());
    assert!(deck.home.is_none() && deck.tabs.is_empty());
    assert_eq!(state.tab_rail_preference(), Some(5));
    worker.await.unwrap();
}

#[tokio::test]
async fn closing_last_real_tab_restores_parked_home_draft_without_query() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("kept").unwrap());
    let mut deck = LoopState::default();
    deck.sync_tabs(&mut state);
    deck.open_home(&mut state, TuiState::new_home(app.clone()));
    state.handle_paste("parked Home draft");
    deck.activate(&mut state, 0).unwrap();
    assert!(deck.home.is_some());
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::CloseTab { index: 0 },
    )
    .await
    .unwrap();
    assert!(state.attached_session().is_none());
    assert_eq!(state.input(), "parked Home draft");
    assert!(deck.tabs.is_empty() && deck.home.is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn sessions_query_uses_owner_metadata_and_scope_without_legacy_id_enumeration() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("root").unwrap());
    let mut deck = LoopState::default();
    state.handle_paste("/sessions");
    let mut input_history = Vec::new();
    tokio::join!(
        state.handle_key(KeyAction::Enter),
        accept_prompt_input(&mut inbox, &mut input_history, "/sessions")
    );
    state.handle_panel_key(KeyAction::Char('N'));
    state.handle_panel_key(KeyAction::CtrlA);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::SessionPickerContext { all_projects, ack }) = inbox.recv().await else {
            panic!("expected picker preferences");
        };
        assert_eq!(all_projects, Some(false));
        ack.send(Ok(oc_core::queries::SessionPickerContext {
            all_projects: false,
            project_name: Some("project".into()),
            canonical: Some("/project".into()),
        }))
        .unwrap();
        let Some(InboxMsg::SessionList {
            search,
            all_projects,
            ack,
        }) = inbox.recv().await
        else {
            panic!("expected bounded metadata query");
        };
        assert_eq!(search, "N");
        assert!(!all_projects);
        ack.send(Ok(vec![oc_core::queries::SessionListEntry {
            id: SessionId("root".into()),
            title: "Named owner title".into(),
            directory: Some("/project".into()),
            created_at: "1".into(),
            updated_at: Some("2".into()),
            date_group: "Today".into(),
            running: false,
            worktree: None,
        }]))
        .unwrap();
    });
    apply_intent(&app, &mut state, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert_eq!(state.modal_options()[0].title, "Named owner title");
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter).intent,
        Some(PanelIntent::SwitchSession { id: "root".into() })
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn picker_open_refusal_keeps_old_view_and_accepted_foreign_receipt_survives_refresh_failure()
{
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("a".into()));
    state.restore_prompt("kept A draft".into());
    let mut deck = LoopState {
        location: Some("/a".into()),
        revision: Some("old".into()),
        ..LoopState::default()
    };
    deck.sync_tabs(&mut state);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker open")
        };
        assert_eq!(session.0, "b");
        assert_eq!(old_deck.location, "/a");
        ack.send(Err(CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            detail: "invalid target".into(),
            diagnostic: None,
        }))
        .unwrap();
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker open")
        };
        let mut target = catalog();
        target.chrome.location = Some("/b".into());
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: "/b".into(),
            catalog: target,
            page: Default::default(),
            deck: TabDeckSnapshot {
                location: "/b".into(),
                revision: Some("accepted-b".into()),
                new_session_titles: vec![true, false],
                sessions: vec![session.clone(), SessionId("parked-b".into())],
                active: Some(session),
                rail_width: Some(5),
            },
            previous_deck: old_deck,
        }))
        .unwrap();
        empty_compactions(&mut inbox).await;
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("parked history")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("parked selection")
        };
        ack.send(Err(CoreError::Shutdown)).unwrap();
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("return route")
        };
        assert_eq!(old_deck.sessions.len(), 2);
        assert_eq!(session.0, "a");
        let mut target = catalog();
        target.chrome.location = Some("/a".into());
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session: session.clone(),
            location: "/a".into(),
            catalog: target,
            page: Default::default(),
            deck: TabDeckSnapshot {
                location: "/a".into(),
                revision: Some("accepted-a".into()),
                new_session_titles: vec![true],
                sessions: vec![session.clone()],
                active: Some(session),
                rail_width: Some(20),
            },
            previous_deck: old_deck,
        }))
        .unwrap();
        empty_compactions(&mut inbox).await;
    });
    let intent = PanelIntent::SwitchSession { id: "b".into() };
    assert!(
        apply_intent(&app, &mut state, &mut deck, intent.clone())
            .await
            .is_err()
    );
    assert_eq!(state.session().0, "a");
    assert_eq!(state.input(), "kept A draft");
    assert_eq!(deck.location.as_deref(), Some("/a"));
    assert_eq!(deck.revision.as_deref(), Some("old"));
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    assert_eq!(state.session().0, "b");
    assert_eq!(deck.location.as_deref(), Some("/b"));
    assert_eq!(deck.revision.as_deref(), Some("accepted-b"));
    assert_eq!(state.tab_rail_preference(), Some(5));
    assert_eq!(deck.tabs.len(), 2);
    assert!(
        deck.picker_pending_tabs
            .contains(&SessionId("parked-b".into()))
    );
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession { id: "a".into() },
    )
    .await
    .unwrap();
    assert_eq!(state.session().0, "a");
    assert_eq!(state.input(), "kept A draft");
    assert_eq!(deck.tabs.len(), 1);
    assert_eq!(state.tab_rail_preference(), Some(20));
    worker.await.unwrap();
}

#[tokio::test]
async fn picker_open_preserves_child_read_only_policy_without_owner_or_deck_changes() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    state.attach_page(&oc_core::queries::HistoryPage {
        parent_id: Some("parent".into()),
        ..Default::default()
    });
    let mut deck = LoopState {
        location: Some("/a".into()),
        read_only: true,
        save_disabled: true,
        ..LoopState::default()
    };
    deck.sync_tabs(&mut state);
    let before = deck.snapshot(&state);
    assert!(
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::SwitchSession {
                id: "parent".into(),
            },
        )
        .await
        .is_err()
    );
    assert_eq!(state.session().0, "child");
    assert!(deck.read_only && deck.save_disabled);
    assert_eq!(deck.snapshot(&state), before);
    assert!(matches!(
        inbox.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn picker_open_existing_tab_keeps_older_history_window_and_draft() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let row = |seq| oc_core::queries::HistoryMessage {
        images: Vec::new(),
        id: oc_core::session::MessageId(format!("m{seq}")),
        seq,
        role: Role::User,
        text: format!("row {seq}"),
        turn: None,
        model_switch: None,
        child: None,
        shell_notice: None,
        user_shell: None,
    };
    let mut state = TuiState::new(app.clone(), SessionId("kept".into()));
    state.attach_page(&oc_core::queries::HistoryPage {
        rows: vec![row(1)],
        total: 200,
        has_newer: true,
        ..Default::default()
    });
    state.restore_prompt("retained draft".into());
    state.chrome.location = Some("/a".into());
    state.apply_tab_rail_width(Some(20)).unwrap();
    let mut deck = LoopState {
        location: Some("/a".into()),
        projection_revision: 7,
        ..Default::default()
    };
    deck.sync_tabs(&mut state);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::OpenPickerSession {
            session,
            old_deck,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("trusted picker route")
        };
        let mut catalog = catalog();
        catalog.chrome.location = Some("/a".into());
        let mut accepted = old_deck.clone();
        accepted.rail_width = Some(5);
        ack.send(Ok(oc_core::queries::SessionPickerOpen {
            session,
            location: "/a".into(),
            catalog: catalog.clone(),
            page: oc_core::queries::HistoryPage {
                rows: vec![row(200)],
                total: 200,
                has_older: true,
                title: Some("real title".into()),
                ..Default::default()
            },
            deck: accepted.clone(),
            previous_deck: old_deck,
        }))
        .unwrap();
        empty_compactions(&mut inbox).await;
        let Some(InboxMsg::Workspace { ack, .. }) = inbox.recv().await else {
            panic!("WorkspaceChanged reconciliation")
        };
        ack.send(Ok(oc_core::queries::WorkspaceSnapshot {
            revision: 10,
            deck: accepted,
            turns: Vec::new(),
        }))
        .unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("retained view refreshes catalog, not transcript")
        };
        ack.send(Ok(catalog)).unwrap();
        let Some(InboxMsg::PendingApprovals { ack }) = inbox.recv().await else {
            panic!("retained approval projection")
        };
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::PendingQuestions { ack }) = inbox.recv().await else {
            panic!("retained question projection")
        };
        ack.send(Ok(Vec::new())).unwrap();
    });
    apply_intent(
        &app,
        &mut state,
        &mut deck,
        PanelIntent::SwitchSession { id: "kept".into() },
    )
    .await
    .unwrap();
    assert_eq!(state.history().rows()[0].seq, 1);
    assert!(state.history().has_newer());
    assert_eq!(state.input(), "retained draft");
    assert_eq!(state.tab_rail_preference(), Some(5));
    assert_eq!(state.session_title.as_deref(), Some("real title"));
    deck.shared_refresh = true;
    assert!(shared::sync(&app, &mut state, &mut deck).await.unwrap());
    assert_eq!(state.history().rows()[0].seq, 1);
    assert!(state.history().has_newer());
    assert_eq!(state.input(), "retained draft");
    assert_eq!(deck.projection_revision, 7);
    worker.await.unwrap();
}

#[tokio::test]
async fn mouse_picker_open_rehits_only_after_same_location_owner_acceptance() {
    for retained in [false, true] {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let target = SessionId("picker-root".into());
        let mut state = if retained {
            TuiState::new(app.clone(), target.clone())
        } else {
            TuiState::new_home(app.clone())
        };
        state.chrome.location = Some("/a".into());
        let mut deck = LoopState {
            location: Some("/a".into()),
            ..Default::default()
        };
        deck.sync_tabs(&mut state);
        state.handle_paste("/sessions");
        let mut inputs = Vec::new();
        tokio::join!(
            state.handle_key(KeyAction::Enter),
            accept_prompt_input(&mut inbox, &mut inputs, "/sessions")
        );
        state.apply_session_entries(vec![oc_core::queries::SessionListEntry {
            id: target.clone(),
            title: "Painted picker root".into(),
            directory: Some("/a".into()),
            created_at: "1".into(),
            updated_at: Some("2".into()),
            date_group: "Today".into(),
            running: false,
            worktree: None,
        }]);
        state.restore_prompt("untouched source draft".into());
        let before = deck.snapshot(&state);
        let owner = target.clone();
        let worker = tokio::spawn(async move {
            for accepted in [false, true] {
                let Some(InboxMsg::OpenPickerSession {
                    session,
                    old_deck,
                    ack,
                    ..
                }) = inbox.recv().await
                else {
                    panic!("only the existing picker owner admits the route")
                };
                assert_eq!(session, owner);
                if !accepted {
                    ack.send(Err(CoreError::Shutdown)).unwrap();
                    continue;
                }
                let mut catalog = catalog();
                catalog.chrome.location = Some("/a".into());
                ack.send(Ok(oc_core::queries::SessionPickerOpen {
                    session: session.clone(),
                    location: "/a".into(),
                    catalog,
                    page: Default::default(),
                    deck: TabDeckSnapshot {
                        location: "/a".into(),
                        sessions: vec![session.clone()],
                        active: Some(session),
                        ..Default::default()
                    },
                    previous_deck: old_deck,
                }))
                .unwrap();
                empty_compactions(&mut inbox).await;
            }
            assert!(inbox.try_recv().is_err(), "hover cannot submit or replay");
        });
        let area = ratatui::layout::Rect::new(0, 0, 120, 40);
        for accepted in [false, true] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal.draw(|frame| render_frame(frame, &state)).unwrap();
            let label = "Painted picker root";
            let position = terminal
                .backend()
                .buffer()
                .content
                .windows(label.len())
                .position(|cells| {
                    cells.iter().map(|cell| cell.symbol()).collect::<String>() == label
                })
                .expect("click the actual painted session option");
            let (x, y) = ((position % 120) as u16, (position / 120) as u16);
            for kind in [
                MouseEventKind::Down(crossterm::event::MouseButton::Left),
                MouseEventKind::Up(crossterm::event::MouseButton::Left),
            ] {
                let outcome = state.handle_mouse(
                    crossterm::event::MouseEvent {
                        kind,
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    },
                    area,
                );
                if matches!(kind, MouseEventKind::Up(_)) {
                    assert_eq!(
                        outcome.intent,
                        Some(PanelIntent::SwitchSession {
                            id: target.0.clone()
                        })
                    );
                }
                apply_mouse_outcome(
                    &app,
                    &mut state,
                    &mut deck,
                    outcome,
                    area,
                    crossterm::event::MouseEvent {
                        kind,
                        column: x,
                        row: y,
                        modifiers: KeyModifiers::NONE,
                    },
                )
                .await;
            }
            if accepted {
                assert_eq!(state.attached_session(), Some(&target));
                assert_eq!(state.mouse_position(), Some((x, y, area)));
                assert_eq!(*state.panel(), TuiPanel::None);
                if retained {
                    assert_eq!(state.input(), "untouched source draft");
                } else {
                    assert_eq!(
                        deck.home.as_ref().unwrap().input(),
                        "untouched source draft"
                    );
                }
            } else {
                assert_eq!(deck.snapshot(&state), before);
                assert_eq!(state.mouse_position(), None);
                assert_eq!(*state.panel(), TuiPanel::Sessions);
                assert_eq!(state.input(), "untouched source draft");
            }
        }
        worker.await.unwrap();
    }
}

#[tokio::test]
async fn sessions_retained_views_read_latest_scope_and_only_ctrl_a_writes() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut a = TuiState::new(app.clone(), SessionId("a".into()));
    let mut b = TuiState::new(app.clone(), SessionId("b".into()));
    let mut deck = LoopState::default();
    let worker = tokio::spawn(async move {
        let mut scope = false;
        let mut input_history = Vec::new();
        for (index, expected) in [None, None, Some(true), None, Some(false), None]
            .into_iter()
            .enumerate()
        {
            if matches!(index, 0 | 1 | 3 | 5) {
                accept_prompt_input(&mut inbox, &mut input_history, "/sessions").await;
            }
            let Some(InboxMsg::SessionPickerContext { all_projects, ack }) = inbox.recv().await
            else {
                panic!("scope")
            };
            assert_eq!(
                all_projects, expected,
                "only the explicit toggle may persist"
            );
            if let Some(value) = all_projects {
                scope = value;
            }
            ack.send(Ok(oc_core::queries::SessionPickerContext {
                all_projects: scope,
                project_name: None,
                canonical: None,
            }))
            .unwrap();
            let Some(InboxMsg::SessionList {
                all_projects, ack, ..
            }) = inbox.recv().await
            else {
                panic!("query")
            };
            assert_eq!(all_projects, scope);
            ack.send(Ok(Vec::new())).unwrap();
        }
    });
    for state in [&mut a, &mut b] {
        state.handle_paste("/sessions");
        state.handle_key(KeyAction::Enter).await;
        apply_intent(&app, state, &mut deck, PanelIntent::LoadSessions)
            .await
            .unwrap();
    }
    a.handle_panel_key(KeyAction::CtrlA);
    apply_intent(&app, &mut a, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    b.close_panel();
    b.handle_paste("/sessions");
    b.handle_key(KeyAction::Enter).await;
    apply_intent(&app, &mut b, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert!(b.sessions_all_projects());
    b.handle_panel_key(KeyAction::CtrlA);
    apply_intent(&app, &mut b, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    a.close_panel();
    a.handle_paste("/sessions");
    a.handle_key(KeyAction::Enter).await;
    apply_intent(&app, &mut a, &mut deck, PanelIntent::LoadSessions)
        .await
        .unwrap();
    assert!(!a.sessions_all_projects());
    worker.await.unwrap();
}

#[tokio::test]
async fn v03_catalog_failure_is_not_an_empty_usable_session() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::Create { ack, .. }) = inbox.recv().await else {
            panic!("create")
        };
        ack.send(Ok(())).unwrap();
        let Some(InboxMsg::History { ack, .. }) = inbox.recv().await else {
            panic!("history")
        };
        ack.send(Ok(Default::default())).unwrap();
        let Some(InboxMsg::SessionSelection { ack, .. }) = inbox.recv().await else {
            panic!("catalog")
        };
        ack.send(Err(oc_core::session::CoreError::Shutdown))
            .unwrap();
    });
    let result = initial_state(&app, Some(SessionId::new("catalog-failure").unwrap())).await;
    assert!(
        matches!(result, Err(StartupFailure::QueryDiagnostic(diagnostic))
        if diagnostic.code == oc_core::queries::ServiceCode::QueryFailed)
    );
    worker.await.unwrap();
}

#[tokio::test]
async fn bare_home_queries_selection_without_creating_or_reading_history() {
    use oc_core::core_app::InboxMsg;
    let (app, mut inbox, _) = CoreApp::channel(4);
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::HomeSelection { action, ack }) = inbox.recv().await else {
            panic!("Home must query selection first")
        };
        assert_eq!(action, SelectionAction::Current);
        ack.send(Err(CoreError::Shutdown)).unwrap();
        assert!(inbox.try_recv().is_err(), "no root or history query");
    });
    assert!(matches!(
        initial_state(&app, None).await,
        Err(StartupFailure::QueryDiagnostic(diagnostic))
            if diagnostic.code == oc_core::queries::ServiceCode::QueryFailed
    ));
    worker.await.unwrap();
}

#[tokio::test]
async fn home_refuses_session_scoped_queries() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app.clone());
    let mut loop_state = LoopState::default();
    for intent in [
        PanelIntent::LoadCards,
        PanelIntent::LoadCardOutput {
            op: "op".into(),
            offset: 0,
        },
        PanelIntent::LoadOlder,
        PanelIntent::LoadNewer,
        PanelIntent::Compress {
            focus: String::new(),
        },
    ] {
        assert!(
            apply_intent(&app, &mut state, &mut loop_state, intent)
                .await
                .unwrap_err()
                .contains("no active session")
        );
        assert!(
            inbox.try_recv().is_err(),
            "no session-scoped query reached worker"
        );
    }
}

#[tokio::test]
async fn chosen_and_cycled_effort_is_remembered_immediately_without_sending() {
    use oc_core::queries::{ModelEntry, ModelPreferenceAction, VariantEntry};
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app.clone(), SessionId::new("effort-memory").unwrap());
    let mut selected = catalog();
    let variant = |name: &str| VariantEntry {
        name: name.into(),
        disabled: false,
        reasoning_effort: Some(name.into()),
    };
    selected.models = vec![ModelEntry {
        id: "first".into(),
        display_name: "first".into(),
        variants: vec![variant("low"), variant("high")],
        variants_known: true,
        ..Default::default()
    }];
    state.apply_catalog(selected);
    state.draft_model("first").unwrap();
    let provider = state.drafted_variant().map(|action| match action {
        ModelPreferenceAction::Variant { model, .. } => model.provider,
        _ => unreachable!(),
    });
    let owner = async {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let Some(InboxMsg::ModelPreference { action, ack }) = inbox.recv().await else {
                panic!("expected an immediate effort memory write");
            };
            seen.push(action);
            let _ = ack.send(Ok(Default::default()));
        }
        seen
    };
    let ui = async {
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::ChooseModel {
                id: "first".into(),
                variant: Some("high".into()),
            },
        )
        .await
        .unwrap();
        apply_intent(
            &app,
            &mut state,
            &mut LoopState::default(),
            PanelIntent::CycleVariant,
        )
        .await
        .unwrap();
    };
    let (seen, ()) = tokio::join!(owner, ui);
    let remembered: Vec<_> = seen
        .into_iter()
        .map(|action| match action {
            Some(ModelPreferenceAction::Variant { model, variant }) => {
                assert_eq!(
                    (Some(model.provider), model.model.as_str()),
                    (provider.clone(), "first")
                );
                variant
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(remembered[0].as_deref(), Some("high"));
    assert_ne!(
        remembered[1], remembered[0],
        "Ctrl+T cycle is remembered too"
    );
}
