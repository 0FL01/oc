use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{ChildState, HistoryPage, ShellNotice};

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
            ack,
        }) = inbox.recv().await
        else {
            panic!("exact read despite missing old Running capture");
        };
        assert_eq!(session, selected.parent);
        assert_eq!(requested, selected);
        ack.send(Ok(HistoryPage {
            parent_id: Some(selected.parent.0.clone()),
            child_job: Some(Box::new(frozen)),
            ..Default::default()
        }))
        .unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("current parent inventory");
        };
        assert_eq!(session, selected.parent);
        ack.send(Ok(newer)).unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("existing child inventory");
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
    let worker = tokio::spawn(async move {
        for phase in 0..3 {
            let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
                panic!("one bounded inventory on owned lifecycle phase {phase}")
            };
            assert_eq!(session, expected.parent);
            let mut current = expected.clone();
            if phase == 0 {
                current.state = ChildState::Admitted;
                current.turn = None;
            }
            current.background = phase == 2;
            ack.send(Ok(vec![current])).unwrap();
        }
        let Some(InboxMsg::ReadChild {
            session,
            selected,
            ack,
        }) = inbox.recv().await
        else {
            panic!("actual child terminal refresh")
        };
        assert_eq!(session, expected.parent);
        assert_eq!(selected, expected);
        ack.send(Ok(HistoryPage::default())).unwrap();
        let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
            panic!("terminal child status while composer stays closed")
        };
        assert_eq!(session, expected.parent);
        let mut current = expected;
        current.state = ChildState::Completed;
        current.background = true;
        ack.send(Ok(vec![current])).unwrap();
        assert!(inbox.try_recv().is_err());
    });
    handle_worker_event(
        &app,
        &mut state,
        &mut deck,
        &job.parent,
        CoreEvent::ToolCallStarted {
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
    assert_eq!(
        state.transcript_rows()[1]
            .tool
            .as_ref()
            .unwrap()
            .child_job
            .as_deref(),
        Some(&job)
    );
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
    assert!(state.viewport().join("\n").contains("Background"));
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
    worker.await.unwrap();
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
            QuestionDecision::Answers(vec![vec!["A".into()]])
        );
        ack.send(Err(QuestionReplyError::Unavailable)).unwrap();
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
    assert!(
        !can_add,
        "projection does not widen readonly child controls"
    );
    assert_eq!(deck.snapshot(&child), snapshot_before);
    assert_eq!(deck.child_parent.as_ref().unwrap().input(), "root draft α");
    let exact = job.clone();
    let worker = tokio::spawn(async move {
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
                id: oc_core::session::MessageId("leaf-committed".into()),
                seq: 3,
                role: oc_core::session::Role::User,
                text: "late captured-source RAW result".into(),
                turn: None,
                model_switch: None,
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
        let Some(InboxMsg::ReadChild {
            session,
            selected,
            ack,
        }) = inbox.recv().await
        else {
            panic!(
                "linked notice must read the captured family fence, not current Location history"
            )
        };
        assert_eq!(session, exact.parent);
        assert_eq!(selected, exact);
        ack.send(Ok(HistoryPage::default())).unwrap();
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
    worker.await.unwrap();
    assert_eq!(child.chrome.location.as_deref(), Some("/A"));
    assert!(deck.read_only);
    return_parent(&mut child, &mut deck);
    assert_eq!(child.session(), &job.parent);
    assert_eq!(child.input(), "root draft α");
    assert_eq!(child.chrome.location.as_deref(), Some("/B"));
    assert!(!deck.read_only);
}
