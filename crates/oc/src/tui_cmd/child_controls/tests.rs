use super::super::tests::{
    settle_children, settle_controls, settle_notices, settle_reads, settle_work,
};
use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::core_app::WorkerTurnId;
use oc_core::queries::{ChildState, HistoryPage, ShellNotice};

#[tokio::test]
async fn root_handoff_dispatches_all_original_owners_and_rejects_stale_context() {
    use oc_core::queries::{ExecuteJob, ShellJob};
    use oc_tui::app::ForegroundWork;
    let (app, mut inbox, _) = CoreApp::channel(8);
    let turn = WorkerTurnId("parent-turn".into());
    let child = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("captured-child".into()),
        operation: "child-operation".into(),
        generation: 17,
        location: "/original/child".into(),
        agent: "helper".into(),
        model: "captured/model".into(),
        description: "held child".into(),
        delivery_id: "original-delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    };
    let shell = ShellJob {
        session: child.parent.clone(),
        shell_id: "shell-operation".into(),
        generation: 23,
        location: "/original/shell".into(),
        model: "other/captured-model".into(),
        provider: "captured-provider".into(),
        turn: turn.0.clone(),
        command: "original command".into(),
        pid: Some(1234),
        background: false,
        output: None,
    };
    let execute = ExecuteJob {
        session: child.parent.clone(),
        execute_id: "execute-operation".into(),
        generation: 29,
        location: "/original/execute".into(),
        model: "execute/captured-model".into(),
        provider: "captured-provider".into(),
        turn: turn.0.clone(),
        background: false,
        output: None,
    };
    let mut state = TuiState::new(app.clone(), child.parent.clone());
    state.begin_linked_turn(turn.clone());
    state.restore_prompt("original unsent root draft".into());
    state.apply_child_jobs(vec![child.clone()]);
    state.apply_shell_jobs(vec![shell.clone()]);
    state.apply_execute_jobs(vec![execute.clone()]);
    state.apply_tool_started(
        &turn,
        &child.operation,
        "subagent",
        r#"{"agent":"helper","description":"held child"}"#,
    );
    state.apply_tool_started(
        &turn,
        &shell.shell_id,
        "shell",
        r#"{"command":"original command"}"#,
    );
    state.apply_tool_started(
        &turn,
        &execute.execute_id,
        "execute",
        r#"{"code":"return 1"}"#,
    );
    let parts = state.transcript_rows();
    let action = state
        .terminal_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('b'),
            crossterm::event::KeyModifiers::CONTROL,
        ))
        .unwrap();
    let intent = state.handle_key(action).await.intent.unwrap();
    let mut deck = LoopState::default();
    let expected = child.clone();
    let expected_shell = shell.clone();
    let expected_execute = execute.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::BackgroundChild {
            session,
            selected,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured child")
        };
        assert_eq!(session, expected.parent);
        assert_eq!(selected, expected);
        // A terminal race in one owner must not prevent handoff of the others.
        ack.send(Err(CoreError::Application("already settled".into())))
            .unwrap();
        let Some(InboxMsg::BackgroundShell {
            session,
            shell_id,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured shell")
        };
        assert_eq!(session, expected_shell.session);
        assert_eq!(shell_id, expected_shell.shell_id);
        ack.send(Ok(())).unwrap();
        let Some(InboxMsg::BackgroundExecute {
            session,
            turn,
            execute_id,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured execute")
        };
        assert_eq!(session, expected_execute.session);
        assert_eq!(turn, expected_execute.turn);
        assert_eq!(execute_id, expected_execute.execute_id);
        ack.send(Ok(())).unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("same-session inventory")
        };
        assert_eq!(session, expected.parent);
        ack.send(Ok(vec![])).unwrap();
        let Some(InboxMsg::CompactionHistory { ack, .. }) = inbox.recv().await else {
            panic!("same-session compactions")
        };
        ack.send(Ok(vec![])).unwrap();
        let Some(InboxMsg::ShellJobs { session, ack }) = inbox.recv().await else {
            panic!("same-session inventory")
        };
        assert_eq!(session, expected_shell.session);
        ack.send(Ok(vec![])).unwrap();
        let Some(InboxMsg::ExecuteJobs { session, ack }) = inbox.recv().await else {
            panic!("same-session execute inventory")
        };
        assert_eq!(session, expected_execute.session);
        ack.send(Ok(vec![])).unwrap();
        inbox
    });
    apply_intent(&app, &mut state, &mut deck, intent)
        .await
        .unwrap();
    settle_work(&app, &mut state, &mut deck).await;
    settle_reads(&app, &mut state, &mut deck).await;
    let mut inbox = worker.await.unwrap();
    assert_eq!(state.input(), "original unsent root draft");
    assert_eq!(state.active_turn(), Some(&turn));
    let after = state.transcript_rows();
    assert_eq!(after.len(), parts.len(), "no added model/tool part");
    for (before, after) in parts.iter().zip(&after) {
        let before = before.tool.as_ref().unwrap();
        let after = after.tool.as_ref().unwrap();
        assert_eq!(
            (
                &after.op,
                &after.state,
                &after.output_preview,
                after.output_bytes
            ),
            (
                &before.op,
                &before.state,
                &before.output_preview,
                before.output_bytes
            ),
            "inventory may update presentation, never the logical graph/result"
        );
    }
    for (session, selected_turn) in [
        (SessionId("foreign".into()), turn.clone()),
        (child.parent.clone(), WorkerTurnId("stale".into())),
    ] {
        apply_intent(
            &app,
            &mut state,
            &mut deck,
            PanelIntent::BackgroundSession {
                session,
                turn: selected_turn,
                work: vec![
                    ForegroundWork::Shell(Box::new(shell.clone())),
                    ForegroundWork::Execute(Box::new(execute.clone())),
                ],
            },
        )
        .await
        .unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "stale queued scope cannot query or execute"
        );
    }
}

#[tokio::test]
async fn stale_running_child_capture_outside_inventory_reads_current_original_owner() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let captured = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("oldest".into()),
        operation: "oldest-launch".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/captured".into(),
        description: "oldest child".into(),
        delivery_id: "oldest-delivery".into(),
        state: ChildState::Running,
        background: true,
        turn: Some("oldest-turn".into()),
        result: None,
        message_id: None,
    };
    let mut current = captured.clone();
    current.state = ChildState::Completed;
    current.message_id = Some("actual-delivery-message".into());
    let newer: Vec<_> = (0..16)
        .map(|index| {
            let mut job = current.clone();
            job.operation = format!("newer-{index}");
            job.child = SessionId(format!("newer-child-{index}"));
            job.delivery_id = format!("newer-delivery-{index}");
            job
        })
        .collect();
    let mut state = TuiState::new(app.clone(), captured.parent.clone());
    state.restore_prompt("parked original parent draft α".into());
    let parent_turn = WorkerTurnId("parent-turn".into());
    state.begin_linked_turn(parent_turn.clone());
    state.apply_child_jobs(vec![captured.clone()]);
    state.apply_tool_started(
        &parent_turn,
        &captured.operation,
        "subagent",
        r#"{"agent":"helper","description":"oldest child"}"#,
    );
    state.apply_tool_finished(
        &parent_turn,
        &captured.operation,
        "subagent",
        "completed",
        "immutable launch result",
        23,
        false,
    );
    state.apply_child_jobs(newer.clone());
    assert_eq!(
        state.transcript_rows()[0]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_ref()
            .unwrap()
            .state,
        ChildState::Running
    );
    let mut child = TuiState::new(app.clone(), captured.child.clone());
    child.attach_linked_child(captured.clone());
    child.begin_linked_turn(WorkerTurnId("oldest-turn".into()));
    let mut deck = LoopState::default();
    deck.child_views.insert(captured.child.clone(), child);
    let selected = captured.clone();
    let frozen = current.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("bounded inventory");
        };
        assert_eq!(session, selected.parent);
        ack.send(Ok(newer.clone())).unwrap();
        let Some(InboxMsg::ReadChild {
            session,
            selected: requested,
            window,
            ack,
        }) = inbox.recv().await
        else {
            panic!("exact read despite missing old Running capture");
        };
        assert_eq!(session, selected.parent);
        assert_eq!(requested, selected);
        assert_eq!(window, Some(Default::default()));
        ack.send(Ok(HistoryPage {
            parent_id: Some(selected.parent.0.clone()),
            child_job: Some(Box::new(frozen)),
            ..Default::default()
        }))
        .unwrap();
        let Some(InboxMsg::ReadChild {
            session,
            selected: requested,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("qualified retained resource read")
        };
        assert_eq!(session, selected.parent);
        assert_eq!(requested.child, selected.child);
        ack.send(Ok(HistoryPage::default())).unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("existing child inventory");
        };
        assert_eq!(session, selected.child);
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("current parent inventory")
        };
        assert_eq!(session, selected.parent);
        ack.send(Ok(newer)).unwrap();
        let Some(InboxMsg::CompactionHistory { ack, .. }) = inbox.recv().await else {
            panic!("child compactions")
        };
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::ShellJobs { session, ack }) = inbox.recv().await else {
            panic!("child shells")
        };
        assert_eq!(session, selected.child);
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::ExecuteJobs { session, ack }) = inbox.recv().await else {
            panic!("child executes")
        };
        assert_eq!(session, selected.child);
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::PendingApprovals { ack }) = inbox.recv().await else {
            panic!("existing approval owner");
        };
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::PendingQuestions { ack }) = inbox.recv().await else {
            panic!("existing question owner");
        };
        ack.send(Ok(Vec::new())).unwrap();
        assert!(
            inbox.try_recv().is_err(),
            "no control, provider or extra read query"
        );
    });
    open(&app, &mut state, &mut deck, captured.clone())
        .await
        .unwrap();
    settle_children(&app, &mut state, &mut deck).await;
    settle_reads(&app, &mut state, &mut deck).await;
    settle_controls(&app, &mut state, &mut deck).await;
    worker.await.unwrap();
    assert_eq!(state.session(), &captured.child);
    assert_eq!(state.linked_child(), Some(&current));
    assert!(
        state.active_turn().is_none(),
        "settled owner cannot acquire a ghost live turn"
    );
    assert!(deck.read_only);
    state.apply_child_jobs(vec![captured.clone()]);
    assert_eq!(
        state.linked_child(),
        Some(&current),
        "late same-operation Running inventory cannot revive a frozen terminal owner"
    );
    assert_eq!(
        deck.child_parent.as_ref().unwrap().input(),
        "parked original parent draft α"
    );
    return_parent(&mut state, &mut deck);
    assert_eq!(state.session(), &captured.parent);
    assert_eq!(state.input(), "parked original parent draft α");
    assert_eq!(state.active_turn(), Some(&parent_turn));
}

#[tokio::test]
async fn closed_root_subagent_card_receives_owned_lifecycle_before_parent_completion() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let job = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "actual-launch".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture/captured".into(),
        description: "inspect configs".into(),
        delivery_id: "actual-delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    };
    let mut state = TuiState::new(app.clone(), job.parent.clone());
    state.restore_prompt("root draft α".into());
    let parent_turn = WorkerTurnId("parent-turn".into());
    state.begin_linked_turn(parent_turn.clone());
    state.apply_delta(&parent_turn, "one actual text part");
    let mut child = TuiState::new(app.clone(), job.child.clone());
    child.attach_linked_child(job.clone());
    let mut deck = LoopState::default();
    deck.child_views.insert(job.child.clone(), child);
    let expected = job.clone();
    let phase = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed_phase = phase.clone();
    let worker = tokio::spawn(async move {
        let mut parents = [0; 4];
        let mut children = [0; 4];
        let mut qualified = [0; 4];
        for _ in 0..22 {
            let message = inbox.recv().await.unwrap();
            let phase = observed_phase.load(std::sync::atomic::Ordering::SeqCst);
            match message {
                InboxMsg::ChildJobs { session, ack } => {
                    if session == expected.parent {
                        parents[phase] += 1;
                        let mut current = expected.clone();
                        if phase == 0 {
                            current.state = ChildState::Admitted;
                            current.turn = None;
                        }
                        if phase == 3 {
                            current.state = ChildState::Completed;
                        }
                        current.background = phase >= 2;
                        ack.send(Ok(vec![current])).unwrap();
                    } else {
                        assert_eq!(session, expected.child);
                        children[phase] += 1;
                        ack.send(Ok(Vec::new())).unwrap();
                    }
                }
                InboxMsg::ReadChild {
                    session,
                    selected,
                    window,
                    ack,
                } => {
                    assert_eq!(session, expected.parent);
                    assert_eq!(selected, expected);
                    assert_eq!(window, Some(Default::default()));
                    qualified[phase] += 1;
                    if phase != 3 {
                        ack.send(Ok(HistoryPage::default())).unwrap();
                        continue;
                    }
                    ack.send(Ok(HistoryPage {
                        rows: vec![oc_core::queries::HistoryMessage {
                            id: oc_core::session::MessageId("child-answer".into()),
                            seq: 42,
                            role: oc_core::session::Role::Assistant,
                            text: "aggregate must not be replayed".into(),
                            images: Vec::new(),
                            model_switch: None,
                            agent_switch: None,
                            user_shell: None,
                            child: None,
                            shell_notice: None,
                            turn: Some(oc_core::queries::HistoryTurn {
                                parts: vec![oc_core::queries::TranscriptPart::Text(
                                    "actual completed child".into(),
                                )],
                                window: Some(oc_core::queries::HistoryPartWindow {
                                    start: 0,
                                    end: 1,
                                    total: 1,
                                }),
                                ..Default::default()
                            }),
                        }],
                        total: 1,
                        ..Default::default()
                    }))
                    .unwrap();
                }
                InboxMsg::CompactionHistory { session, ack, .. } => {
                    assert!(session == expected.parent || session == expected.child);
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::ShellJobs { session, ack } => {
                    assert!(session == expected.parent || session == expected.child);
                    ack.send(Ok(Vec::new())).unwrap();
                }
                InboxMsg::ExecuteJobs { session, ack } => {
                    assert!(session == expected.parent || session == expected.child);
                    ack.send(Ok(Vec::new())).unwrap();
                }
                _ => panic!("only qualified child and source resource reads"),
            }
        }
        assert_eq!(parents, [1, 1, 1, 2]);
        assert_eq!(children, [0, 0, 0, 1]);
        assert_eq!(qualified, [0, 0, 0, 1]);
        assert!(inbox.try_recv().is_err());
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &job.parent,
        CoreEvent::ToolCallStarted {
            output_presentation: None,
            session: job.parent.clone(),
            turn: parent_turn.clone(),
            op: job.operation.clone(),
            name: "subagent".into(),
            input: r#"{"agent":"helper","description":"inspect configs"}"#.into(),
            dcp_topic: None,
        },
    )
    .await
    .unwrap();
    settle_reads(&app, &mut state, &mut deck).await;
    assert!(!state.children_open());
    assert_eq!(
        state.transcript_rows()[1]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_ref()
            .unwrap()
            .state,
        ChildState::Admitted
    );
    phase.store(1, std::sync::atomic::Ordering::SeqCst);
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &job.parent,
        CoreEvent::TurnStarted {
            session: job.child.clone(),
            turn: WorkerTurnId("child-turn".into()),
            model_switch: None,
        },
    )
    .await
    .unwrap();
    settle_reads(&app, &mut state, &mut deck).await;
    assert_eq!(
        state.transcript_rows()[1]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_deref(),
        Some(&job)
    );
    phase.store(2, std::sync::atomic::Ordering::SeqCst);
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &job.parent,
        CoreEvent::ToolCallFinished {
            session: job.parent.clone(),
            turn: parent_turn.clone(),
            op: job.operation.clone(),
            name: "subagent".into(),
            state: "completed".into(),
            output: "immutable logical launch result".into(),
            output_bytes: 31,
            output_truncated: false,
            output_presentation: None,
            question: None,
            dcp: None,
            patch_effects: None,
        },
    )
    .await
    .unwrap();
    settle_reads(&app, &mut state, &mut deck).await;
    assert!(state.viewport().join("\n").contains("Background"));
    phase.store(3, std::sync::atomic::Ordering::SeqCst);
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &job.parent,
        CoreEvent::TurnFinished {
            session: job.child.clone(),
            turn: WorkerTurnId("child-turn".into()),
            text: "actual completed child".into(),
            duration_ms: 1,
            warnings: Vec::new(),
            service_warning_range: 0..0,
        },
    )
    .await
    .unwrap();
    settle_reads(&app, &mut state, &mut deck).await;
    worker.await.unwrap();
    let child_rows = deck.child_views.get(&job.child).unwrap().transcript_rows();
    assert!(
        child_rows
            .iter()
            .any(|row| row.text == "actual completed child")
    );
    assert!(
        !child_rows
            .iter()
            .any(|row| row.text.contains("aggregate must not be replayed"))
    );
    let rows = state.transcript_rows();
    assert_eq!(rows.len(), 2, "no extra model parts during child refresh");
    assert_eq!(rows[0].text, "one actual text part");
    let card = rows[1].tool.as_ref().unwrap();
    assert_eq!(card.op, job.operation);
    assert_eq!(card.state, "completed");
    assert_eq!(card.output_preview, "immutable logical launch result");
    assert_eq!(
        card.child_job.as_ref().unwrap().state,
        ChildState::Completed
    );
    assert!(state.viewport().join("\n").contains("✓ Helper Subagent"));
    assert!(state.viewport().join("\n").contains("Background"));
    assert_eq!(state.active_turn(), Some(&parent_turn));
    assert_eq!(state.input(), "root draft α");
    assert!(!state.children_open());
}

#[tokio::test]
async fn readonly_child_enter_preserves_real_question_binding_and_owner_reply() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use oc_core::approval::ApprovalBinding;
    use oc_core::question::{QuestionDecision, QuestionInput, QuestionReplyError, QuestionRequest};
    let (app, mut inbox, _) = CoreApp::channel(8);
    let request = QuestionRequest {
        id: 17,
        binding: ApprovalBinding {
            session: "child".into(),
            turn: "accepted-child-turn".into(),
            call: "real-question".into(),
            operation: "original-operation".into(),
            input_digest: "original-input".into(),
            location: "/A".into(),
            generation: 7,
            agent: Some("maker".into()),
            agent_digest: Some("original-profile".into()),
        },
        input: QuestionInput::parse(&serde_json::json!({"questions":[{
            "question":"Choose?","header":"Owned question",
            "options":[{"label":"A","description":"first"}]
        }]}))
        .unwrap(),
        images: false,
    };
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    state.restore_prompt("unsubmitted readonly draft".into());
    state
        .questions
        .reconcile(std::slice::from_ref(&request), vec![request.clone()]);
    let mut deck = LoopState {
        read_only: true,
        ..Default::default()
    };
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::ReplyQuestion { reply, ack }) = inbox.recv().await else {
            panic!(
                "question form must reach its existing owner, never Submit or child cancellation"
            )
        };
        assert_eq!(reply.id, request.id);
        assert_eq!(reply.binding, request.binding);
        assert_eq!(
            reply.decision,
            QuestionDecision::Answers {
                answers: vec![vec!["A".into()]],
                images: vec![]
            }
        );
        ack.send(Err(QuestionReplyError::Unavailable)).unwrap();
        let Some(InboxMsg::PendingApprovals { ack }) = inbox.recv().await else {
            panic!("control refresh")
        };
        ack.send(Ok(Vec::new())).unwrap();
        let Some(InboxMsg::PendingQuestions { ack }) = inbox.recv().await else {
            panic!("question refresh")
        };
        ack.send(Ok(vec![request])).unwrap();
        let Some(InboxMsg::History {
            session,
            limit,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("single ancestry query")
        };
        assert_eq!(session.0, "child");
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage::default())).unwrap();
        assert!(inbox.try_recv().is_err());
    });
    handle_event(
        &app,
        &mut state,
        &mut deck,
        CEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .await
    .unwrap();
    settle_controls(&app, &mut state, &mut deck).await;
    worker.await.unwrap();
    assert_eq!(state.input(), "unsubmitted readonly draft");
    assert!(state.questions.active().is_some());
    assert!(deck.read_only);
}

#[tokio::test]
async fn terminal_source_notice_reads_captured_child_and_keeps_parked_parent_draft() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let job = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "original-operation".into(),
        generation: 7,
        location: "/A".into(),
        agent: "maker".into(),
        model: "fixture/captured".into(),
        description: "owned child".into(),
        delivery_id: "original-delivery".into(),
        state: ChildState::Completed,
        background: true,
        turn: Some("accepted-child-turn".into()),
        result: Some("terminal".into()),
        message_id: Some("committed".into()),
    };
    let mut parent = TuiState::new(app.clone(), job.parent.clone());
    parent.session_title = Some("Actual parent title".into());
    parent.chrome.location = Some("/B".into());
    parent.restore_prompt("root draft α".into());
    let mut child = TuiState::new(app.clone(), job.child.clone());
    child.attach_linked_child(job.clone());
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        tabs: vec![None],
        tab_cards_before: vec![None],
        active_tab: Some(0),
        read_only: true,
        ..Default::default()
    };
    child.session_title = Some("Distinct child title".into());
    let snapshot_before = deck.snapshot(&child);
    deck.sync_tabs(&mut child);
    let (shown, active, can_add) = child.tab_presentation();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].session, job.parent);
    assert_eq!(shown[0].title.as_deref(), Some("Actual parent title"));
    assert_eq!(active, 0);
    assert!(can_add, "linked read-only view can leave for root Home");
    assert!(
        child
            .command_unavailable(&CommandAction::OpenAgents)
            .is_some(),
        "navigation does not authorize child mutation"
    );
    assert_eq!(deck.snapshot(&child), snapshot_before);
    assert_eq!(deck.child_parent.as_ref().unwrap().input(), "root draft α");
    let exact = job.clone();
    let worker = tokio::spawn(async move {
        let Some(InboxMsg::ReadChild {
            session,
            selected,
            window,
            ack,
        }) = inbox.recv().await
        else {
            panic!("captured launch before notice resources")
        };
        assert_eq!(session, exact.parent);
        assert_eq!(selected, exact);
        assert_eq!(window, Some(Default::default()));
        ack.send(Ok(HistoryPage::default())).unwrap();
        let Some(InboxMsg::History {
            session,
            message,
            limit,
            ack,
            ..
        }) = inbox.recv().await
        else {
            panic!("exact child-source notice qualification before family refresh");
        };
        assert_eq!(session, exact.child);
        assert_eq!(
            message,
            Some(oc_core::session::MessageId("leaf-committed".into()))
        );
        assert_eq!(limit, 1);
        ack.send(Ok(HistoryPage {
            rows: vec![oc_core::queries::HistoryMessage {
                images: Vec::new(),
                id: oc_core::session::MessageId("leaf-committed".into()),
                seq: 3,
                role: oc_core::session::Role::User,
                text: "late captured-source RAW result".into(),
                turn: None,
                model_switch: None,
                agent_switch: None,
                user_shell: None,
                child: None,
                shell_notice: Some(oc_core::queries::ShellHistoryNotice {
                    operation: "original-leaf".into(),
                    state: "completed".into(),
                    command: "captured original command".into(),
                }),
            }],
            total: 3,
            ..Default::default()
        }))
        .unwrap();
        assert!(inbox.try_recv().is_err());
    });
    handle_worker_event(
        &app,
        &mut child,
        &mut deck,
        &job.child,
        CoreEvent::ShellNotice(ShellNotice {
            user_requested: false,
            session: job.child.clone(),
            shell_id: "original-leaf".into(),
            delivery_id: "leaf-once".into(),
            message_id: "leaf-committed".into(),
            state: "completed".into(),
            text: "late captured-source result".into(),
        }),
    )
    .await
    .unwrap();
    settle_notices(&app, &mut child, &mut deck).await;
    worker.await.unwrap();
    assert_eq!(child.chrome.location.as_deref(), Some("/A"));
    assert!(deck.read_only);
    return_parent(&mut child, &mut deck);
    assert_eq!(child.session(), &job.parent);
    assert_eq!(child.input(), "root draft α");
    assert_eq!(child.chrome.location.as_deref(), Some("/B"));
    assert!(!deck.read_only);
}

#[tokio::test]
async fn execute_notice_qualifies_child_source_and_preserves_parked_parent() {
    use oc_core::queries::{ExecuteJob, ExecuteNotice, ExecuteSnapshot, HistoryMessage};
    use oc_core::session::{MessageId, Role};
    use oc_core::tool_output::{Execute, ExecuteOutput, ExecuteOutputKind, Presentation};

    let (app, mut inbox, _) = CoreApp::channel(8);
    let job = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("child".into()),
        operation: "child-operation".into(),
        generation: 7,
        location: "/A".into(),
        agent: "helper".into(),
        model: "captured/model".into(),
        description: "child".into(),
        delivery_id: "child-delivery".into(),
        state: ChildState::Running,
        background: true,
        turn: Some("child-turn".into()),
        result: None,
        message_id: None,
    };
    let turn = WorkerTurnId("child-turn".into());
    let execute = ExecuteJob {
        session: job.child.clone(),
        execute_id: "execute-op".into(),
        location: "/A".into(),
        generation: 7,
        turn: turn.0.clone(),
        model: "captured/model".into(),
        provider: "fixture".into(),
        background: true,
        output: None,
    };
    let mut parent = TuiState::new(app.clone(), job.parent.clone());
    parent.restore_prompt("parked parent draft".into());
    let mut child = TuiState::new(app.clone(), job.child.clone());
    child.attach_linked_child(job.clone());
    child.begin_linked_turn(turn.clone());
    child.restore_prompt("child draft".into());
    child.apply_tool_started(&turn, "execute-op", "execute", r#"{"code":"return 1"}"#);
    let mut running = Presentation::new("running receipt", 15, false);
    running.execute = Some(Execute {
        background: true,
        state: Some("running".into()),
        ..Default::default()
    });
    child.apply_tool_finished_with_output_presentation(
        &turn,
        "execute-op",
        "execute",
        "completed",
        "running receipt",
        15,
        false,
        None,
        None,
        None,
        Some(Box::new(running)),
    );
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        read_only: true,
        ..Default::default()
    };
    for valid in [false, true] {
        let expected = job.clone();
        let operation = execute.clone();
        let owner = tokio::spawn(async move {
            let Some(InboxMsg::ReadChild {
                session,
                selected,
                ack,
                ..
            }) = inbox.recv().await
            else {
                panic!("qualify the retained child")
            };
            assert_eq!(session, expected.parent);
            assert_eq!(selected, expected);
            ack.send(Ok(HistoryPage {
                child_job: Some(Box::new(expected.clone())),
                ..Default::default()
            }))
            .unwrap();
            let Some(InboxMsg::History {
                session,
                message,
                ack,
                ..
            }) = inbox.recv().await
            else {
                panic!("read the exact execute notice")
            };
            assert_eq!(session, expected.child);
            assert_eq!(message, Some(MessageId("message".into())));
            ack.send(Ok(HistoryPage {
                rows: vec![HistoryMessage {
                    id: MessageId("message".into()),
                    seq: 3,
                    role: Role::User,
                    text: "native result".into(),
                    images: Vec::new(),
                    turn: None,
                    model_switch: None,
                    agent_switch: None,
                    user_shell: None,
                    child: None,
                    shell_notice: None,
                }],
                total: 1,
                ..Default::default()
            }))
            .unwrap();
            let Some(InboxMsg::ExecuteSnapshot {
                session,
                execute_id,
                ack,
            }) = inbox.recv().await
            else {
                panic!("read the exact execute owner")
            };
            assert_eq!(session, expected.child);
            assert_eq!(execute_id, "execute-op");
            let mut terminal = Presentation::new("final result", 12, false);
            terminal.execute = Some(Execute {
                background: true,
                state: Some("completed".into()),
                output: vec![ExecuteOutput {
                    kind: ExecuteOutputKind::Text,
                    text: "final result".into(),
                    limited: false,
                }],
                ..Default::default()
            });
            ack.send(Ok(ExecuteSnapshot {
                job: operation,
                state: "completed".into(),
                output: Some(Box::new(terminal)),
                delivery_id: if valid { "delivery" } else { "other-delivery" }.into(),
                message_id: Some("message".into()),
            }))
            .unwrap();
            inbox
        });
        handle_worker_event(
            &app,
            &mut child,
            &mut deck,
            &job.child,
            CoreEvent::ExecuteNotice(ExecuteNotice {
                session: job.child.clone(),
                execute_id: "execute-op".into(),
                delivery_id: "delivery".into(),
                message_id: "message".into(),
                state: "completed".into(),
                text: "native result".into(),
            }),
        )
        .await
        .unwrap();
        settle_notices(&app, &mut child, &mut deck).await;
        inbox = owner.await.unwrap();
        let rows = child.transcript_rows();
        let facts = rows[0]
            .tool
            .as_ref()
            .unwrap()
            .output_presentation
            .as_ref()
            .unwrap()
            .execute
            .as_ref()
            .unwrap();
        assert_eq!(
            facts.state.as_deref(),
            Some(if valid { "completed" } else { "running" })
        );
        if valid {
            assert_eq!(facts.output[0].text, "final result");
        }
        assert_eq!(child.input(), "child draft");
        assert_eq!(
            deck.child_parent.as_ref().unwrap().input(),
            "parked parent draft"
        );
        assert!(inbox.try_recv().is_err());
    }
}
