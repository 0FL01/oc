use super::*;
use oc_core::{
    core_app::{CoreApp, MockProvider},
    domain::SessionId,
};

fn job(id: &str, source: &str) -> ShellJob {
    ShellJob {
        session: SessionId(source.into()),
        shell_id: id.into(),
        location: "/original".into(),
        generation: 42,
        turn: "issuing-turn".into(),
        model: "gpt-original".into(),
        provider: "fixture".into(),
        command: id.into(),
        pid: Some(123),
        background: false,
        output: None,
    }
}

#[tokio::test]
async fn live_user_shell_output_updates_only_its_known_card_and_survives_attachment_races() {
    use oc_core::{
        queries::{HistoryMessage, HistoryPage, UserShellResult},
        session::{MessageId, Role},
        tool_output::{Presentation, Shell},
    };
    let session = SessionId("parent".into());
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, session.clone());
    state.restore_prompt("ordinary newer draft".into());
    let mut output = Presentation::new(
        "stdout literal [stderr]\nstderr literal [stdout]\n",
        48,
        false,
    );
    output.shell = Some(Shell {
        stdout: "stdout literal [stderr]\n".into(),
        stderr: "stderr literal [stdout]\n".into(),
        stdout_limited: false,
        stderr_limited: false,
        exit: None,
        signal: None,
        timed_out: false,
        cancelled: false,
    });
    let mut live = job("actual-user-op", "parent");
    live.turn.clear();
    live.output = Some(Box::new(output.clone()));
    // Inventory can arrive before the durable page or local admission receipt.
    state.apply_shell_jobs(vec![live.clone()]);
    assert!(
        state.transcript_rows().is_empty(),
        "inventory never invents a history row"
    );
    let page = HistoryPage {
        rows: vec![HistoryMessage {
            id: MessageId("actual-input".into()),
            seq: 2,
            role: Role::User,
            text: "unchanged RAW admission".into(),
            turn: None,
            model_switch: None,
            child: None,
            user_shell: Some(UserShellResult {
                input: true,
                superseded_input: false,
                operation: live.shell_id.clone(),
                command: "captured original command".into(),
                command_limited: false,
                state: "started".into(),
                output: Box::new(Presentation::new("", 0, false)),
                diagnostic: None,
            }),
        }],
        total: 1,
        ..Default::default()
    };
    state.attach_page(&page);
    let rows = state.transcript_rows();
    let row = &rows[0];
    assert_eq!(row.message_id.as_deref(), Some(&page.rows[0].id));
    assert_eq!(row.tool.as_ref().unwrap().state, "started");
    assert_eq!(
        row.tool.as_ref().unwrap().output_presentation.as_deref(),
        Some(&output)
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>();
    assert!(text.contains("stdout literal [stderr]") && text.contains("stderr literal [stdout]"));
    assert!(!text.contains("unchanged RAW") && !text.contains("Command exited"));
    let baseline = state.transcript_rows();
    state.apply_shell_jobs(vec![live.clone()]);
    assert_eq!(
        state.transcript_rows(),
        baseline,
        "equal publication is idempotent"
    );
    for (source, id, turn) in [
        ("child", "actual-user-op", ""),
        ("parent", "other-op", ""),
        ("parent", "actual-user-op", "model-turn"),
    ] {
        let mut foreign = live.clone();
        foreign.session.0 = source.into();
        foreign.shell_id = id.into();
        foreign.turn = turn.into();
        foreign.output.as_mut().unwrap().body = "foreign text".into();
        state.apply_shell_jobs(vec![foreign]);
        assert_eq!(state.transcript_rows(), baseline);
    }
    let mut completed = page.clone();
    let result = completed.rows[0].user_shell.as_mut().unwrap();
    result.input = false;
    result.state = "completed".into();
    *result.output = Presentation::new("final output", 12, false);
    state.refresh_user_shell_page(&completed);
    let settled = state.transcript_rows();
    state.apply_shell_jobs(vec![live]);
    assert_eq!(
        state.transcript_rows(),
        settled,
        "late inventory cannot revive a terminal result"
    );
    assert_eq!(state.input(), "ordinary newer draft");
    assert!(
        inbox.try_recv().is_err(),
        "the projection does not send another query or command"
    );
}

#[tokio::test]
async fn live_shell_footer_is_source_scoped_and_owns_only_its_painted_pointer_target() {
    use ratatui::{Terminal, backend::TestBackend};
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("parent".into()));
    state.chrome.location = Some("/shell-in-path/not-a-footer-target".into());
    state.apply_shell_jobs(vec![
        job("child", "child"),
        job("first", "parent"),
        job("second", "parent"),
    ]);
    assert_eq!(state.running_shell_count(), 2);
    assert!(!state.shells_open());
    for (width, height) in [(80, 24), (120, 40)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let (area, [hit, _]) = state.shells.footer_hit.get().unwrap();
        let text: String = (hit.x..hit.right())
            .map(|x| terminal.backend().buffer()[(x, hit.y)].symbol())
            .collect();
        assert_eq!(text, "↓ 2 shells");
        let mouse = |kind, column, row| MouseEvent {
            kind,
            column,
            row,
            modifiers: crossterm::event::KeyModifiers::NONE,
        };
        // Bare release and a drag from another cell are not activations.
        assert!(
            state
                .handle_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
                    area
                )
                .intent
                .is_none()
        );
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), 2, hit.y),
            area,
        );
        assert!(
            state
                .handle_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
                    area
                )
                .intent
                .is_none()
        );
        state.handle_mouse(mouse(MouseEventKind::Moved, hit.x + 2, hit.y), area);
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(hit.x + 2, hit.y)].fg,
            crate::theme::Theme::dark().text()
        );
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), hit.x, hit.y),
            area,
        );
        let opened = state.handle_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
            area,
        );
        assert_eq!(opened.intent, Some(PanelIntent::LoadChildren));
        assert!(state.children_open());
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert!(state.shells.footer_hit.get().is_none());
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), hit.x, hit.y),
            area,
        );
        state.handle_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
            area,
        );
        assert!(
            state.children_open(),
            "the covered footer cannot close its composer"
        );
        state.handle_key(KeyAction::Cancel).await;
        state.handle_key(KeyAction::Children).await;
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert!(state.shells.footer_hit.get().is_none());
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), hit.x, hit.y),
            area,
        );
        state.handle_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
            area,
        );
        assert!(state.children_open() && !state.shells_open());
        state.handle_key(KeyAction::Cancel).await;
        state.push_note(&"A long toast covers the footer surface. ".repeat(100));
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let toast = crate::shell::toast_rect(&state, area).unwrap();
        assert!(toast.contains((hit.x, hit.y).into()));
        assert!(state.shells.footer_hit.get().is_none());
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), hit.x, hit.y),
            area,
        );
        state.handle_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
            area,
        );
        assert!(
            !state.shells_open(),
            "toast-covered cells never activate Shell"
        );
        state.chrome.command_palette_shortcut = Some(String::new());
        state.chrome.child_first_shortcut = Some("ctrl+alt+shift+f12 ctrl+alt+shift+f11".into());
        state.push_note(&format!("{} ", "a".repeat(26)).repeat(100));
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        let toast = crate::shell::toast_rect(&state, area).unwrap();
        let (_, fragments) = state.shells.footer_hit.get().unwrap();
        let visible = fragments.iter().find(|rect| !rect.is_empty()).unwrap();
        assert!(!toast.contains((visible.x, visible.y).into()));
        state.handle_mouse(
            mouse(
                MouseEventKind::Down(MouseButton::Left),
                visible.x,
                visible.y,
            ),
            area,
        );
        assert_eq!(
            state
                .handle_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), visible.x, visible.y),
                    area,
                )
                .intent,
            Some(PanelIntent::LoadChildren),
            "a visible fragment beside the toast remains actionable"
        );
        state.handle_key(KeyAction::Cancel).await;
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(mouse(kind, toast.right() - 4, toast.y + 1), area);
        }
        assert!(state.note().is_none());
        state.chrome.command_palette_shortcut = None;
        state.chrome.child_first_shortcut = None;
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), hit.x, hit.y),
            area,
        );
        state.resize_mouse_position(area);
        terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
        assert!(
            state
                .handle_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), hit.x, hit.y),
                    area
                )
                .intent
                .is_none()
        );
    }
    let mut narrow = Terminal::new(TestBackend::new(8, 24)).unwrap();
    narrow.draw(|f| crate::shell::render(f, &state)).unwrap();
    let (area, [hit, _]) = state.shells.footer_hit.get().unwrap();
    assert!(hit.width > 0 && hit.width < "↓ 2 shells".chars().count() as u16);
    let mouse = |kind| MouseEvent {
        kind,
        column: hit.x,
        row: hit.y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    assert_eq!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent,
        Some(PanelIntent::LoadChildren),
        "the actually painted clipped target is still actionable"
    );
    state.handle_key(KeyAction::Cancel).await;
    state.apply_shell_jobs(vec![job("child", "child")]);
    assert_eq!(state.running_shell_count(), 0);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    assert!(state.shells.footer_hit.get().is_none());
    let mut home = TuiState::new_home(app.clone());
    home.apply_shell_jobs(vec![job("first", "parent")]);
    assert_eq!(home.running_shell_count(), 0);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn live_shell_shortcut_yields_to_prompt_history_and_respects_effective_remaps_and_modal_focus()
 {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    app.prompt_history(Some("accepted older prompt".into()))
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("parent".into()));
    state.apply_shell_jobs(vec![job("first", "parent")]);
    state.restore_prompt("unfinished draft".into());
    state.handle_key(KeyAction::Home).await;
    state.handle_key(KeyAction::Up).await;
    assert_eq!(state.input(), "accepted older prompt");
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    let action = state.terminal_key(down).unwrap();
    assert_eq!(action, KeyAction::PromptHistoryNextOrShells);
    assert!(state.handle_key(action).await.intent.is_none());
    assert_eq!(
        state.input(),
        "accepted older prompt",
        "the raw caret boundary still wins first"
    );
    let action = state.terminal_key(down).unwrap();
    assert!(state.handle_key(action).await.intent.is_none());
    assert_eq!(state.input(), "unfinished draft");
    assert!(!state.shells_open());
    let action = state.terminal_key(down).unwrap();
    assert_eq!(
        state.handle_key(action).await.intent,
        Some(PanelIntent::LoadChildren)
    );
    state.handle_key(KeyAction::Cancel).await;
    let mut repeated = down;
    repeated.kind = crossterm::event::KeyEventKind::Repeat;
    let action = state.terminal_key(repeated).unwrap();
    assert_eq!(action, KeyAction::PromptHistoryNext);
    assert!(state.handle_key(action).await.intent.is_none());
    assert!(
        !state.shells_open(),
        "a held history key does not toggle the live panel"
    );
    state.chrome.child_first_shortcut = Some("f2,ctrl+x s".into());
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(action, KeyAction::Children);
    state.handle_key(action).await;
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE))
        .unwrap();
    state.handle_key(action).await;
    assert!(
        !state.shells_open(),
        "the advertised custom action also closes its panel"
    );
    let prefix = state
        .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .unwrap();
    state.handle_key(prefix).await;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
        Some(KeyAction::Children)
    );
    state.chrome.child_first_shortcut = Some(String::new());
    assert_eq!(state.terminal_key(down), Some(KeyAction::PromptHistoryNext));
    state.chrome.child_first_shortcut = Some("ctrl+p,f2".into());
    assert!(!state.live_shell_shortcut_available("ctrl+p"));
    assert!(!state.live_shell_shortcut_available("ctrl+x"));
    assert!(!state.live_shell_shortcut_available("ctrl+p s"));
    assert!(state.live_shell_shortcut_available("ctrl+x s"));
    assert!(state.live_shell_shortcut_available("f2"));
    state.chrome.child_first_shortcut = Some("up,f2".into());
    assert!(!state.live_shell_shortcut_available("up"));
    assert!(state.live_shell_shortcut_available("f2"));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| crate::shell::render(f, &state)).unwrap();
    let (_, [hit, _]) = state.shells.footer_hit.get().unwrap();
    let hint: String = (hit.x..hit.right())
        .map(|x| terminal.backend().buffer()[(x, hit.y)].symbol())
        .collect();
    assert_eq!(
        hint, "f2 1 shell",
        "the displayed alias is genuinely usable"
    );
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        Some(KeyAction::PromptHistoryPrevious)
    );
    state.chrome.prompt_history_shortcuts.next = "down f3".into();
    assert!(!state.live_shell_shortcut_available("down"));
    assert!(!state.live_shell_shortcut_available("down f3 f4"));
    assert!(state.live_shell_shortcut_available("down f2"));
    state.chrome.prompt_history_shortcuts.next = "down".into();
    state.chrome.child_first_shortcut = Some("ctrl+x,ctrl+p s,f2".into());
    let prefix = state
        .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(prefix, KeyAction::Leader);
    state.handle_key(prefix).await;
    let model = state
        .terminal_key(KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(model, KeyAction::SequenceKey("m".into(), Some('m')));
    state.handle_key(model).await;
    assert_eq!(state.panel(), &crate::app::TuiPanel::Model);
    state.handle_key(KeyAction::Cancel).await;
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        Some(KeyAction::Commands)
    );
    state.chrome.child_first_shortcut = None;
    state.restore_prompt("/".into());
    let options = state.slash_options().unwrap().len();
    assert!(options > 1);
    let before = state.slash_selected(options);
    assert_eq!(state.terminal_key(down), Some(KeyAction::Down));
    assert!(state.handle_key(KeyAction::Down).await.intent.is_none());
    assert_ne!(state.slash_selected(options), before);
    assert!(!state.shells_open());
    state.chrome.location = Some("/original".into());
    state.restore_prompt("@".into());
    assert!(state.mention_request().is_some());
    assert_eq!(state.terminal_key(down), Some(KeyAction::Down));
    assert!(state.handle_key(KeyAction::Down).await.intent.is_none());
    assert!(!state.shells_open());
    state.restore_prompt("unfinished draft".into());
    state.chrome.child_first_shortcut = Some("ctrl+p,f2".into());
    state.handle_key(KeyAction::Commands).await;
    assert_ne!(
        state.terminal_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)),
        Some(KeyAction::Shells)
    );
    assert!(!state.shells_open());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn tool13_open_viewer_and_kill_keep_original_child_identity_after_list_removal() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("child".into()));
    let child = job("selected", "child");
    state.restore_prompt("draft remains ordinary editor state".into());
    state.apply_shell_jobs(vec![child.clone(), job("sibling", "parent")]);
    state.handle_key(KeyAction::Shells).await;
    state.handle_key(KeyAction::Enter).await;
    state.apply_shell_jobs(vec![job("sibling", "parent")]);
    state.apply_shell_output(
        &child,
        ToolOutputPage {
            text: "status: completed\nfinal-child".into(),
            total_bytes: 35,
            next_offset: None,
        },
    );
    let control = state.handle_key(KeyAction::DeleteOrQuit).await;
    let viewer = state.shell_viewer().cloned();
    let output = state.shells.output.as_ref().unwrap().text.clone();
    let draft = state.input().to_owned();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        matches!(control.intent, Some(PanelIntent::CancelShell { session, shell_id }) if session.0 == "child" && shell_id == "selected")
    );
    assert_eq!(viewer, Some(child));
    assert!(output.contains("final-child"));
    assert_eq!(draft, "draft remains ordinary editor state");
}

#[tokio::test]
async fn tool13_same_id_location_adoption_keeps_only_original_shell_capture() {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("parent".into()))
        .await
        .unwrap();
    let mut original = TuiState::new(app.clone(), SessionId("child".into()));
    let child = job("selected", "child");
    original.apply_shell_jobs(vec![child.clone(), job("old-live", "parent")]);
    original.handle_key(KeyAction::Shells).await;
    original.handle_key(KeyAction::Enter).await;
    original.restore_prompt("old-location editor".into());
    let mut moved = TuiState::new(app.clone(), SessionId("child".into()));
    moved.chrome.location = Some("/destination".into());
    moved.apply_shell_jobs(Vec::new());
    moved.inherit_shell_view(&mut original);
    assert_eq!(
        moved.running_shell_count(),
        0,
        "a freshly loaded inventory wins over the inherited viewer's stale list"
    );
    let key = moved.terminal_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('b'),
        crossterm::event::KeyModifiers::CONTROL,
    ));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(moved.shell_viewer(), Some(&child));
    assert_eq!(moved.chrome.location.as_deref(), Some("/destination"));
    assert!(moved.input().is_empty());
    assert_eq!(key, Some(KeyAction::ShellBackground));
    assert!(!original.shells_open());
}
