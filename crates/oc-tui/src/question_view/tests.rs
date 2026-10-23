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
    view.key(KeyAction::Enter);
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
fn answered_question_metadata_is_charged_without_turn_preview_loss() {
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
                images: Vec::new(),
                id: MessageId(format!("message-{seq}")),
                seq,
                role: Role::Assistant,
                text: String::new(),
                model_switch: None,
                agent_switch: None,
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
    assert_eq!(
        window.rows().len(),
        40,
        "all question parts and footers fit"
    );
    assert!(!window.has_older());
    assert!(
        window.retained_bytes() >= 20 * 4 * 4096 * 2,
        "question and answer metadata must both be charged, not ignored"
    );
    assert!(window.retained_bytes() <= WINDOW_BYTES);
}

#[test]
fn question_heading_uses_captured_author_and_profile_color() {
    use oc_core::{core_app::CoreApp, domain::SessionId, queries::*};
    use ratatui::{Terminal, backend::TestBackend};

    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, SessionId("s".into()));
    state.apply_catalog(CatalogSnapshot {
        chrome: TuiChrome {
            agent_colors: [
                ("build".into(), "#5c9cf5".into()),
                ("explore".into(), "#12ab34".into()),
            ]
            .into(),
            ..Default::default()
        },
        auto_accept: Default::default(),
        provider: String::new(),
        models: vec![],
        model_id: String::new(),
        variant: None,
        agents: vec![AgentEntry {
            id: "build".into(),
            color_index: 0,
            description: String::new(),
            model: None,
            variant: None,
        }],
        agent_id: Some("build".into()),
        commands: vec![],
        command_descriptions: Default::default(),
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    for (session, agent, name, color) in [
        ("child", "explore", "Explore", Color::Rgb(0x12, 0xab, 0x34)),
        ("s", "build", "Build", Color::Rgb(0x5c, 0x9c, 0xf5)),
    ] {
        let mut r = request(false);
        r.binding.session = session.into();
        r.binding.agent = Some(agent.into());
        state
            .questions
            .reconcile(std::slice::from_ref(&r), vec![r.clone()]);
        terminal
            .draw(|frame| {
                crate::shell::render_background(frame);
                render(frame, &state, Rect::new(0, 1, 80, 23));
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let title = (0..24)
            .find(|y| buffer[(5, *y)].symbol() == "Q")
            .expect("question heading");
        let row: String = (0..80).map(|x| buffer[(x, title)].symbol()).collect();
        assert_eq!(row.trim(), format!("┃  Questions · {name}"));
        assert_eq!(buffer[(5, title)].fg, Theme::dark().text_muted());
        assert_eq!(buffer[(15, title)].fg, Theme::dark().text_muted());
        for x in 17..17 + name.len() as u16 {
            assert_eq!(buffer[(x, title)].fg, color);
        }
        assert_eq!(state.active_agent(), Some("build"));
        let Some(PanelIntent::ReplyQuestion(reply)) = state.questions.key(KeyAction::Enter).intent
        else {
            panic!("question answer");
        };
        assert_eq!(reply.binding, r.binding);
    }
}

#[tokio::test]
async fn vis37_pending_panel_uses_pinned_roles_tabs_description_rows_and_custom_caret() {
    use oc_core::{
        core_app::{CoreApp, MockProvider},
        domain::SessionId,
    };
    use ratatui::{
        Terminal,
        backend::{Backend, TestBackend},
    };
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("s".into())).await.unwrap();
    let mut r = request(true);
    r.input = oc_core::question::QuestionInput::parse(&serde_json::json!({"questions":[
        {"header":"Languages and native runtimes", "question":"Pick languages?", "multiple":true,
         "options":[{"label":"Rust","description":"Native owner and a bounded ordered answer."},
                    {"label":"Python","description":"Fixture peer, not a production runtime dependency."}]},
        {"header":"Platform and execution targets", "question":"Where Ω界? Choose an explicit host for the current workspace and retain this long question in metadata order.", "options":[
            {"label":"Linux","description":"Current host."}, {"label":"Other","description":"Explicit selection."}]}
    ]})).unwrap();
    let theme = Theme::dark();
    for (width, height) in [(80, 24), (120, 40), (160, 48)] {
        let mut state = TuiState::new(app.clone(), SessionId("s".into()));
        state.handle_paste("ordinary composer draft Ω界");
        let input = state.input().to_string();
        state
            .questions
            .reconcile(std::slice::from_ref(&r), vec![r.clone()]);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let area = Rect::new(0, 1, width, height - 1);
        terminal
            .draw(|frame| {
                crate::shell::render_background(frame);
                render(frame, &state, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let title = height - 15;
        assert_eq!(row(title).trim(), "┃  Questions");
        assert_eq!(
            row(title + 2).trim(),
            "┃  Languages and native ru…  Platform and execution…  Submit"
        );
        assert_eq!(row(title + 4).trim(), "┃  Pick languages?");
        assert_eq!(row(title + 6).trim(), "┃  1. [ ] Rust");
        assert_eq!(
            row(title + 7).trim(),
            "┃         Native owner and a bounded ordered answer."
        );
        assert_eq!(
            row(height - 3).trim(),
            "┃  ⇆ tab  ↑↓ select  enter toggle  esc dismiss"
        );
        assert_eq!(
            buffer[(2, title)].fg,
            theme.color("background.action.primary.$focused").unwrap()
        );
        assert_eq!(buffer[(5, title)].fg, theme.text_muted());
        assert_eq!(
            buffer[(5, title + 2)].modifier,
            ratatui::style::Modifier::BOLD
        );
        assert_eq!(
            buffer[(5, title + 6)].fg,
            theme.color("text.formfield.$focused").unwrap()
        );
        assert_eq!(buffer[(12, title + 7)].fg, theme.text_muted());
        assert_eq!(
            buffer[(7, title + 6)].fg,
            ratatui::style::Color::Rgb(255, 255, 255)
        );
        assert_eq!(
            buffer[(7, title + 6)].bg,
            theme
                .color("background.formfield.$focused")
                .unwrap_or(theme.color("background.formfield.base").unwrap())
        );
        assert_eq!(buffer[(width - 2, title)].bg, theme.background());
        state.questions.key(KeyAction::Char(' '));
        state.questions.key(KeyAction::Down);
        state.questions.key(KeyAction::Char(' '));
        state.questions.key(KeyAction::Tab);
        state.questions.key(KeyAction::Char('3'));
        state.questions.paste("custom Ω界");
        terminal
            .draw(|frame| {
                crate::shell::render_background(frame);
                render(frame, &state, area);
            })
            .unwrap();
        let cursor = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!((cursor.x, cursor.y), (18, height - 5));
        let buffer = terminal.backend().buffer();
        let row = |y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        if width == 80 {
            assert_eq!(row(height - 16).trim(), "┃  Questions");
            assert_eq!(
                row(height - 12).trim(),
                // TestBackend contains a physical continuation cell for 界;
                // the immutable question itself has no inserted space.
                "┃  Where Ω界 ? Choose an explicit host for the current workspace and"
            );
            assert_eq!(buffer[(12, height - 12)].symbol(), "界");
            assert_eq!(buffer[(13, height - 12)].symbol(), " ");
            assert_eq!(
                row(height - 11).trim(),
                "┃  retain this long question in metadata order."
            );
            assert_eq!(buffer[(5, height - 11)].fg, theme.text());
        } else {
            assert_eq!(row(height - 15).trim(), "┃  Questions");
        }
        assert_eq!(
            terminal.backend().buffer()[(20, height - 5)].bg,
            theme
                .color("background.formfield.$focused")
                .unwrap_or(theme.color("background.formfield.base").unwrap())
        );
        assert!(state.questions.key(KeyAction::Enter).intent.is_none());
        terminal
            .draw(|frame| {
                crate::shell::render_background(frame);
                render(frame, &state, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let review = (0..width)
            .map(|x| buffer[(x, height - 6)].symbol())
            .collect::<String>();
        assert_eq!(
            review.trim(),
            "┃  Languages and native runtimes: Rust, Python"
        );
        assert_eq!(state.input(), input);
        assert_eq!(state.questions.active(), Some(&r));
        let drafts = state.questions.drafts.lock().unwrap();
        assert_eq!(
            drafts[0].answers,
            vec![vec!["Rust", "Python"], vec!["custom Ω界"]]
        );
        assert!(!drafts[0].submitting);
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[test]
fn vis37_custom_tab_and_backtab_commit_without_replacing_caret_keys_or_submitting() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let mut r = request(true);
    let mut second = r.input.questions[0].clone();
    second.multiple = false;
    r.input.questions.push(second);
    r.input.validate().unwrap();
    let source = r.clone();
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    let tab = view
        .terminal_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    let backtab = view
        .terminal_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT))
        .unwrap();
    assert_eq!(backtab, KeyAction::DialogActionPrevious);
    view.key(KeyAction::Char(' '));
    view.key(KeyAction::Char('3'));
    view.paste("custom Ω界");
    let left = view
        .terminal_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    assert!(view.key(left).intent.is_none());
    assert_eq!(view.drafts.lock().unwrap()[0].tab, 0);
    assert!(view.key(KeyAction::Enter).intent.is_none());
    assert_eq!(
        view.drafts.lock().unwrap()[0].tab,
        0,
        "multi custom Enter commits, not advances"
    );
    view.key(KeyAction::Char(' '));
    assert_eq!(
        view.drafts.lock().unwrap()[0].answers[0],
        vec!["A"],
        "picked custom is a real multiselect toggle"
    );
    view.key(KeyAction::Char(' '));
    assert!(view.key(tab.clone()).intent.is_none());
    {
        let drafts = view.drafts.lock().unwrap();
        let d = &drafts[0];
        assert_eq!(d.tab, 1);
        assert!(!d.editing && !d.submitting);
        assert_eq!(d.answers[0], vec!["A", "custom Ω界"]);
    }
    view.key(KeyAction::Char('3'));
    assert!(view.key(tab.clone()).intent.is_none());
    assert_eq!(
        view.drafts.lock().unwrap()[0].tab,
        2,
        "question fields are optional: empty custom Tab clears and goes to review without submit"
    );
    assert!(view.key(backtab.clone()).intent.is_none());
    assert_eq!(view.drafts.lock().unwrap()[0].tab, 1);
    assert!(view.key(backtab.clone()).intent.is_none());
    assert_eq!(
        view.drafts.lock().unwrap()[0].tab,
        0,
        "backward navigation remains local to the owned form"
    );
    assert!(view.key(tab.clone()).intent.is_none());
    view.key(KeyAction::Char('3'));
    view.paste("second");
    assert!(view.key(tab.clone()).intent.is_none());
    assert_eq!(view.drafts.lock().unwrap()[0].tab, 2);
    assert!(view.key(backtab).intent.is_none());
    assert_eq!(view.drafts.lock().unwrap()[0].tab, 1);
    assert!(view.key(tab.clone()).intent.is_none());
    let Some(PanelIntent::ReplyQuestion(reply)) = view.key(KeyAction::Enter).intent else {
        panic!("only explicit review Enter submits");
    };
    assert_eq!(reply.binding, source.binding);
    assert_eq!(
        reply.decision,
        QuestionDecision::Answers(vec![
            vec!["A".into(), "custom Ω界".into()],
            vec!["second".into()]
        ])
    );
    assert!(view.key(KeyAction::Enter).intent.is_none());
    assert_eq!(view.active(), Some(&source));

    let single = request(false);
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&single), vec![single.clone()]);
    view.key(KeyAction::Char('3'));
    view.paste("only");
    assert!(view.key(tab).intent.is_none());
    let drafts = view.drafts.lock().unwrap();
    assert_eq!(drafts[0].tab, 0);
    assert_eq!(drafts[0].answers, vec![vec!["only"]]);
    assert!(!drafts[0].editing && !drafts[0].submitting);
    drop(drafts);

    let multiple = request(true);
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&multiple), vec![multiple.clone()]);
    view.key(KeyAction::Char('3'));
    assert!(view.key(KeyAction::Enter).intent.is_none());
    let drafts = view.drafts.lock().unwrap();
    assert_eq!(drafts[0].tab, 0);
    assert!(drafts[0].answered[0] && drafts[0].answers[0].is_empty());
    assert!(!drafts[0].editing && !drafts[0].submitting);
}

#[test]
fn vis37_custom_typing_paste_and_close_keep_owned_answers_without_implicit_submit() {
    let r = request(true);
    let mut view = QuestionView::default();
    view.reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    view.key(KeyAction::Down);
    view.key(KeyAction::Down);
    assert!(view.key(KeyAction::Char('Ω')).intent.is_none());
    {
        let drafts = view.drafts.lock().unwrap();
        assert!(drafts[0].editing);
        assert_eq!(drafts[0].custom[0], "Ω");
        assert_eq!(drafts[0].answers[0], vec!["Ω"]);
    }
    view.key(KeyAction::Cancel);
    assert_eq!(view.drafts.lock().unwrap()[0].answers[0], vec!["Ω"]);
    view.key(KeyAction::Up);
    view.paste("界\r\nsecond line");
    {
        let drafts = view.drafts.lock().unwrap();
        assert!(drafts[0].editing && !drafts[0].submitting);
        assert_eq!(drafts[0].selected[0], 2);
        assert_eq!(drafts[0].custom[0], "Ω界\nsecond line");
        assert_eq!(drafts[0].answers[0], vec!["Ω界\nsecond line"]);
    }
    view.key(KeyAction::Cancel);
    {
        let drafts = view.drafts.lock().unwrap();
        let rows = layout(&drafts[0], 40, 48, Color::Reset)
            .rows
            .into_iter()
            .map(|row| {
                row.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        assert!(rows.iter().any(|row| row.contains("Ω界")));
        assert!(rows.iter().any(|row| row.trim() == "second line"));
        assert!(
            !rows.iter().any(|row| row.contains("Ω界 second")),
            "closed custom text keeps source line breaks"
        );
    }
    view.key(KeyAction::Char(' ')); // selected custom toggles off first
    view.key(KeyAction::Char(' ')); // then reopens the same custom editor
    view.key(KeyAction::Interrupt);
    assert!(view.drafts.lock().unwrap()[0].answers[0].is_empty());
    assert!(view.key(KeyAction::Up).intent.is_none());
    let drafts = view.drafts.lock().unwrap();
    assert!(!drafts[0].editing && !drafts[0].submitting);
    assert_eq!(drafts[0].selected[0], 1);
    assert_eq!(drafts[0].request, r);
}

#[tokio::test]
async fn vis37_question_pointer_uses_owned_paint_and_never_submits_on_edit_drag_or_stale_release() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use oc_core::{
        core_app::{CoreApp, MockProvider},
        domain::SessionId,
    };
    use ratatui::{Terminal, backend::TestBackend};
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("s".into())).await.unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("s".into()));
    state.handle_paste("ordinary composer Ω界");
    let input = state.input().to_string();
    let r = request(true);
    state
        .questions
        .reconcile(std::slice::from_ref(&r), vec![r.clone()]);
    let frame = Rect::new(0, 0, 120, 40);
    let mut loop_owner = QuestionView::default();
    loop_owner.share_drafts(&state.questions);
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    let paint = |terminal: &mut Terminal<TestBackend>, state: &TuiState| {
        terminal
            .draw(|f| {
                crate::shell::render_background(f);
                render(f, state, Rect::new(0, 1, 120, 39));
            })
            .unwrap();
    };
    let target = |state: &TuiState, action| {
        state.questions.drafts.lock().unwrap()[0]
            .painted
            .as_ref()
            .unwrap()
            .targets
            .iter()
            .find(|(_, control)| *control == action)
            .unwrap()
            .0
    };
    let event = |rect: Rect, kind| MouseEvent {
        kind,
        column: rect.x,
        row: rect.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    paint(&mut terminal, &state);
    let a = target(&state, Control::Option(0));
    assert!(
        state
            .handle_mouse(event(a, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    assert!(state.questions.drafts.lock().unwrap()[0].answers[0].is_empty());
    state.handle_mouse(event(a, MouseEventKind::Down(MouseButton::Left)), frame);
    // Production shares this same process-memory owner at every frontend-loop
    // iteration, including the iteration between real mouse down and up.
    state.questions.share_drafts(&loop_owner);
    assert!(
        state
            .handle_mouse(event(a, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].answers[0],
        vec!["A"]
    );
    paint(&mut terminal, &state);
    let b = target(&state, Control::Option(1));
    state.handle_mouse(event(b, MouseEventKind::Down(MouseButton::Left)), frame);
    state.handle_mouse(event(b, MouseEventKind::Drag(MouseButton::Left)), frame);
    state.handle_mouse(event(b, MouseEventKind::Up(MouseButton::Left)), frame);
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].answers[0],
        vec!["A"]
    );
    state.handle_mouse(event(b, MouseEventKind::Down(MouseButton::Left)), frame);
    state.handle_mouse(
        event(b, MouseEventKind::Up(MouseButton::Left)),
        Rect::new(0, 0, 80, 24),
    );
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].answers[0],
        vec!["A"]
    );
    paint(&mut terminal, &state);
    state.handle_mouse(event(b, MouseEventKind::Down(MouseButton::Left)), frame);
    // The visible modal/permission owner retires the question paint and
    // gesture. Repainting the form cannot revive that old release.
    state.questions.cancel_pointer();
    assert!(state.questions.drafts.lock().unwrap()[0].painted.is_none());
    paint(&mut terminal, &state);
    assert!(
        state
            .handle_mouse(event(b, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].answers[0],
        vec!["A"]
    );
    let custom = target(&state, Control::Option(2));
    state.handle_mouse(
        event(custom, MouseEventKind::Down(MouseButton::Left)),
        frame,
    );
    assert!(
        state
            .handle_mouse(event(custom, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    state.questions.paste("custom Ω界");
    paint(&mut terminal, &state);
    let editor = state.questions.drafts.lock().unwrap()[0]
        .painted
        .as_ref()
        .unwrap()
        .input
        .as_ref()
        .unwrap()
        .0;
    let inside_wide = Rect::new(editor.x + 9, editor.y, 1, 1);
    state.handle_mouse(
        event(inside_wide, MouseEventKind::Down(MouseButton::Left)),
        frame,
    );
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].editor.cursor,
        "custom Ω".len()
    );
    assert!(
        state
            .handle_key(KeyAction::Char('Q'))
            .await
            .intent
            .is_none()
    );
    assert_eq!(
        state.questions.drafts.lock().unwrap()[0].custom[0],
        "custom ΩQ界"
    );
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    paint(&mut terminal, &state);
    let tab = target(&state, Control::Tab(1));
    state.handle_mouse(event(tab, MouseEventKind::Down(MouseButton::Left)), frame);
    assert!(
        state
            .handle_mouse(event(tab, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    paint(&mut terminal, &state);
    let submit = target(&state, Control::Submit);
    state.handle_mouse(
        event(submit, MouseEventKind::Down(MouseButton::Left)),
        frame,
    );
    let Some(PanelIntent::ReplyQuestion(reply)) = state
        .handle_mouse(event(submit, MouseEventKind::Up(MouseButton::Left)), frame)
        .intent
    else {
        panic!("explicit painted submit");
    };
    assert_eq!(reply.binding, r.binding);
    assert_eq!(
        reply.decision,
        QuestionDecision::Answers(vec![vec!["A".into(), "custom ΩQ界".into()]])
    );
    assert!(
        state
            .handle_mouse(event(submit, MouseEventKind::Up(MouseButton::Left)), frame)
            .intent
            .is_none()
    );
    assert_eq!(state.input(), input);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[test]
fn vis37_sixteen_fields_keep_fallback_actions_and_explicit_empty_review_without_losing_metadata() {
    let mut r = request(true);
    r.input.questions[0].options[0].description = "long description ".repeat(180);
    r.input.questions = vec![r.input.questions[0].clone(); 16];
    r.input.validate().unwrap();
    let original = r.input.clone();
    let mut d = Draft::new(r);
    let form = layout(&d, 52, 24, Color::Reset);
    let text = |line: &Line<'_>| {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    };
    assert!(
        form.rows
            .iter()
            .any(|r| text(r) == "Field 1 of 16   · 0/16 completed")
    );
    assert_eq!(
        text(&form.rows[form.rows.len() - 2]),
        "⇆ tab  ↑↓ select  enter toggle  esc dismiss"
    );
    assert!(
        form.rows.len() <= 22,
        "body clipping must retain the live actions"
    );
    assert!(d.select().intent.is_none());
    assert!(d.select().intent.is_none());
    assert!(d.answers[0].is_empty() && d.answered[0]);
    d.selected[0] = d.request.input.questions[0].options.len();
    d.select();
    d.custom[0] = "custom Ω界".into();
    assert!(d.commit_custom());
    assert_eq!(d.answers[0], vec!["custom Ω界"]);
    d.answers[0].clear();
    d.tab = 16;
    let review = layout(&d, 52, 24, Color::Reset);
    assert!(review.rows.iter().any(|r| text(r) == "Pick: (none)"));
    assert!(
        review
            .rows
            .iter()
            .any(|r| text(r) == "Pick: (not answered)")
    );
    assert_eq!(
        text(&review.rows[review.rows.len() - 2]),
        "⇆ tab  enter submit  esc dismiss"
    );
    assert_eq!(d.request.input, original);
    assert!(!d.submitting);
    assert!(label("Ω界e\u{301}👨‍👩‍👧‍👦abcdefghijklmno", 24).ends_with('…'));
}
