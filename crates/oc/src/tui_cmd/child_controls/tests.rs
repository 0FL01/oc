use super::*;
use oc_core::core_app::InboxMsg;
use oc_core::queries::{ChildState, HistoryPage, ShellNotice};

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
    parent.chrome.location = Some("/B".into());
    parent.restore_prompt("root draft α".into());
    let mut child = TuiState::new(app.clone(), job.child.clone());
    child.attach_linked_child(job.clone());
    let mut deck = LoopState {
        child_parent: Some(Box::new(parent)),
        read_only: true,
        ..Default::default()
    };
    let exact = job.clone();
    let worker = tokio::spawn(async move {
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
