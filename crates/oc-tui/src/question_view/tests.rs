use super::*;
fn request(multiple: bool) -> QuestionRequest {
    QuestionRequest { id: 1, binding: oc_core::approval::ApprovalBinding { session: "s".into(), turn: "t".into(), call: "c".into(), operation: "o".into(), input_digest: "d".into(), location: "l".into(), generation: 1, agent: None, agent_digest: None }, input: oc_core::question::QuestionInput::parse(&serde_json::json!({"questions":[{"question":"Pick?","header":"Pick","multiple":multiple,"options":[{"label":"A","description":"first"},{"label":"B","description":"second"}]}]})).unwrap() }
}
#[test]
fn multiple_requires_submit_and_error_remount_retains_exact_draft() {
    let r = request(true);
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    assert!(view.key(KeyAction::Down).intent.is_none());
    assert!(view.key(KeyAction::Enter).intent.is_none());
    view.key(KeyAction::Up);
    view.key(KeyAction::Char(' '));
    view.key(KeyAction::Tab);
    let Some(PanelIntent::ReplyQuestion(reply)) = view.key(KeyAction::Enter).intent else {
        panic!("explicit submit");
    };
    assert_eq!(
        reply.decision,
        QuestionDecision::Answers(vec![vec!["A".into(), "B".into()]])
    );
    view.reply_result(&reply, Some("retry".into()));
    let mut remounted = QuestionView::default();
    remounted.share_drafts(&view);
    remounted.reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    let mut view = remounted;
    assert_eq!(view.drafts.lock().unwrap()[0].answers, vec![vec!["A", "B"]]);
    assert_eq!(
        view.drafts.lock().unwrap()[0].error.as_deref(),
        Some("retry")
    );
    let Some(PanelIntent::ReplyQuestion(retry)) = view.key(KeyAction::Enter).intent else {
        panic!("retry");
    };
    assert_eq!(reply, retry);
}
#[test]
fn automatic_custom_single_then_esc_cancels_without_inventing_answer() {
    let r = request(false);
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    view.key(KeyAction::Char('3'));
    view.paste("discarded custom draft");
    assert!(view.key(KeyAction::Interrupt).intent.is_none());
    {
        let drafts = view.drafts.lock().unwrap();
        assert!(drafts[0].custom[0].is_empty());
        assert!(drafts[0].editing);
        assert!(!drafts[0].submitting);
    }
    // Pinned question.toField always supplies options, including []; this is
    // non-textual custom editing, not an arbitrary textual Form field.
    assert!(view.key(KeyAction::Interrupt).intent.is_none());
    assert!(!view.drafts.lock().unwrap()[0].editing);
    view.key(KeyAction::Char('3'));
    view.paste("custom");
    view.key(KeyAction::Cancel);
    assert_eq!(view.drafts.lock().unwrap()[0].custom[0], "custom");
    let Some(PanelIntent::ReplyQuestion(reply)) = view.key(KeyAction::Cancel).intent else {
        panic!("dismiss");
    };
    assert_eq!(reply.decision, QuestionDecision::Cancelled);
    view.reply_result(&reply, Some("retry".into()));
    view.key(KeyAction::Enter);
    let Some(PanelIntent::ReplyQuestion(reply)) = view.key(KeyAction::Enter).intent else {
        panic!("custom answer");
    };
    assert_eq!(
        reply.decision,
        QuestionDecision::Answers(vec![vec!["custom".into()]])
    );
}

#[tokio::test]
async fn composer_chip_cursor_and_permission_priority_are_preserved() {
    use oc_core::{
        approval::*,
        core_app::{CoreApp, MockProvider},
        domain::SessionId,
    };
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("s".into())).await.unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("s".into()));
    state.handle_paste(&"ordinary draft\n".repeat(20));
    state.handle_key(KeyAction::Left).await;
    let input = state.input().to_string();
    let (rows, cursor) = state.prompt_layout(80);
    let chips: Vec<_> = rows
        .iter()
        .map(|r| (r.text.clone(), r.chip_spans.clone()))
        .collect();
    assert!(chips.iter().any(|(_, c)| c.iter().any(|flag| *flag)));
    let r = request(false);
    state
        .questions
        .reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    state.approvals.reconcile(vec![(
        ApprovalRequest {
            id: 9,
            binding: r.binding.clone(),
            project: "p".into(),
            action: "read".into(),
            resources: vec!["file".into()],
            save_patterns: vec![],
            preview: ApprovalPreview::Resource {
                values: vec!["file".into()],
            },
        },
        false,
    )]);
    assert!(matches!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ReplyApproval(_))
    ));
    assert!(state.questions.drafts.lock().unwrap()[0].answers[0].is_empty());
    state.approvals.reconcile(vec![]);
    state.handle_key(KeyAction::Char('3')).await;
    state.handle_paste("answer");
    assert!(matches!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::ReplyQuestion(_))
    ));
    assert_eq!(state.input(), input);
    let (rows, after_cursor) = state.prompt_layout(80);
    assert_eq!(after_cursor, cursor);
    assert_eq!(
        rows.iter()
            .map(|r| (r.text.clone(), r.chip_spans.clone()))
            .collect::<Vec<_>>(),
        chips
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[test]
fn answered_question_metadata_obeys_the_existing_history_byte_budget() {
    use crate::history::{HistoryWindow, WINDOW_BYTES};
    use oc_core::{
        queries::*,
        question::{QuestionInput, QuestionResult},
        session::{MessageId, Role},
    };
    let mut q = request(false).input.questions[0].clone();
    q.question = "q".repeat(4096);
    for option in &mut q.options {
        option.description = "d".repeat(1000);
    }
    let result = QuestionResult {
        questions: vec![q; 4],
        answers: vec![vec!["a".repeat(4096)]; 4],
    };
    QuestionInput {
        questions: result.questions.clone(),
    }
    .validate()
    .unwrap();
    let page = HistoryPage {
        child_job: None,
        parent_id: None,
        title: None,
        reverted: None,
        total: 20,
        has_older: false,
        has_newer: false,
        rows: (0..20)
            .map(|seq| HistoryMessage {
                id: MessageId(format!("message-{seq}")),
                seq,
                role: Role::Assistant,
                text: String::new(),
                model_switch: None,
                child: None,
                shell_notice: None,
                user_shell: None,
                turn: Some(HistoryTurn {
                    id: format!("turn-{seq}"),
                    status: "completed".into(),
                    parts: vec![TranscriptPart::Tool(ToolOpView {
                        child_job: None,
                        output_presentation: None,
                        question: Some(result.clone()),
                        op: format!("op-{seq}"),
                        rowid: seq,
                        name: "question".into(),
                        state: "completed".into(),
                        input: None,
                        output: None,
                        output_bytes: 0,
                        output_truncated: false,
                        patch_effects: None,
                        dcp: None,
                        dcp_topic: None,
                    })],
                    ..Default::default()
                }),
            })
            .collect(),
    };
    let mut window = HistoryWindow::new();
    window.reset(&page);
    assert!(
        window.rows().len() < 20,
        "question metadata must trigger existing byte eviction"
    );
    assert!(window.has_older());
    assert!(window.retained_bytes() <= WINDOW_BYTES);
}
