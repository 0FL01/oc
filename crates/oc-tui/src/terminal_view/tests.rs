use super::*;
use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::queries::{TerminalCell, TerminalState};

fn entry(id: &str) -> TerminalEntry {
    TerminalEntry {
        target: TerminalRef {
            id: id.into(),
            session: SessionId("source".into()),
            location: "/source".into(),
            generation: 7,
        },
        shell: "/bin/bash".into(),
        cwd: "/source".into(),
        pid: 42,
        title: "bash".into(),
        foreground: Some("sleep".into()),
        state: TerminalState::Running,
        exit: None,
    }
}
fn screen(entry: TerminalEntry) -> TerminalSnapshot {
    TerminalSnapshot {
        entry,
        size: TerminalSize { rows: 2, cols: 4 },
        cells: vec![
            TerminalCell {
                text: "R".into(),
                wide_continuation: false,
                fg: TerminalColor::Indexed(1),
                bg: TerminalColor::Default,
                bold: true,
                dim: false,
                italic: false,
                underline: false,
                inverse: false
            };
            8
        ],
        cursor: (1, 2),
        hide_cursor: false,
        application_cursor: true,
        bracketed_paste: true,
        output_cursor: 12,
        revision: 1,
        ready: true,
        control_strings_dropped: 0,
    }
}
async fn state() -> (TuiState, CoreApp, oc_core::core_app::WorkerGuard) {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    app.create_session(SessionId("source".into()))
        .await
        .unwrap();
    let mut state = TuiState::new(app.clone(), SessionId("source".into()));
    state.chrome.session_terminal = true;
    state.chrome.location = Some("/source".into());
    state.chrome.terminal_generation = 7;
    (state, app, guard)
}
fn attach(state: &mut TuiState, row: TerminalEntry) {
    state.apply_terminal_inventory(
        &row.target.session,
        TerminalInventory {
            enabled: true,
            entries: vec![row.clone()],
            selected: Some(row.target.clone()),
        },
    );
    let snapshot = screen(row);
    state.apply_terminal_snapshot(snapshot.clone());
    assert!(!state.terminal_ready());
    assert!(!state.apply_terminal_replay(TerminalReplay {
        from: 12,
        next: 12,
        bytes: vec![],
        reset: None,
        screen: Box::new(snapshot)
    }));
    assert!(state.terminal_ready());
}

#[tokio::test]
async fn term01_composer_undefined_wrap_mouse_and_close_captured_target() {
    let (mut state, app, guard) = state().await;
    state.apply_terminal_inventory(
        &SessionId("source".into()),
        TerminalInventory {
            enabled: true,
            entries: vec![entry("one"), entry("two")],
            selected: None,
        },
    );
    state.handle_key(KeyAction::TerminalSelect).await;
    assert!(state.terminals_open());
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    state.handle_key(KeyAction::Char('k')).await;
    assert_eq!(state.terminals.cursor, Some(2)); // wraps to New, never first-Up-close
    let out = state.handle_key(KeyAction::Enter).await;
    assert!(!state.terminals_open());
    assert_eq!(
        out.intent,
        Some(PanelIntent::Terminal {
            session: SessionId("source".into()),
            action: TerminalIntent::Create(TerminalAction::Create {
                location: "/source".into(),
                generation: 7,
                size: TerminalSize::default(),
            }),
            close_composer: true
        })
    );
    state.handle_key(KeyAction::TerminalSelect).await;
    state.handle_key(KeyAction::Char('j')).await;
    assert_eq!(state.terminals.cursor, Some(0));
    crate::views::render_test(&state, 80, 24);
    let r = state.terminals.painted.get().unwrap();
    let click = |kind| MouseEvent {
        kind,
        column: r.x + 2,
        row: r.y + 2,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(
        click(MouseEventKind::Down(MouseButton::Left)),
        Rect::new(0, 0, 80, 24),
    );
    let out = state.handle_mouse(
        click(MouseEventKind::Up(MouseButton::Left)),
        Rect::new(0, 0, 80, 24),
    );
    assert_eq!(
        out.intent,
        Some(PanelIntent::Terminal {
            session: SessionId("source".into()),
            action: TerminalIntent::Select(entry("two").target),
            close_composer: true
        })
    );
    assert!(!state.terminals_open());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn term01_raw_priority_controls_leader_and_first_click_restore_draft() {
    let (mut state, app, guard) = state().await;
    state.restore_prompt("draft界 alpha".into());
    state.handle_key(KeyAction::Left).await;
    let cursor = state.prompt_layout(40).1;
    attach(&mut state, entry("one"));
    state.focus_terminal(true);
    state.chrome.terminal_shortcuts.bindings[4] = "ctrl+c".into();
    for (c, expected) in [('c', 3), ('d', 4)] {
        let out = state
            .raw_terminal_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
            .unwrap();
        assert!(
            matches!(out.intent, Some(PanelIntent::Terminal { action: TerminalIntent::Control(TerminalAction::Input { bytes, .. }), .. }) if bytes == [expected])
        );
    }
    assert!(
        state
            .raw_terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
            .is_none()
    );
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
        .unwrap();
    state.handle_key(action).await;
    assert!(
        state
            .raw_terminal_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
            .is_none()
    );
    let action = state
        .terminal_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))
        .unwrap();
    state.handle_key(action).await;
    assert!(!state.terminal_focused());
    assert_eq!(state.input(), "draft界 alpha");
    assert_eq!(state.prompt_layout(40).1, cursor);
    state.focus_terminal(true);
    let frame = Rect::new(0, 0, 80, 24);
    let wheel = MouseEvent {
        kind: MouseEventKind::ScrollUp,
        column: 4,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    assert!(state.terminal_mouse(wheel, frame).is_none());
    assert!(state.terminal_focused());
    let down = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        ..wheel
    };
    assert!(state.terminal_mouse(down, frame).is_some());
    assert!(!state.terminal_focused());
    assert!(
        state
            .terminal_mouse(
                MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    ..wheel
                },
                frame
            )
            .is_some()
    );
    state.chrome.conversation_shortcuts.leader = "ctrl+b".into();
    state.focus_terminal(true);
    assert!(
        state
            .raw_terminal_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL))
            .is_some()
    );
    assert!(
        state
            .raw_terminal_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL))
            .is_none()
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn term01_replay_atomic_checkpoint_theme_cells_resize_and_disconnect() {
    let (mut state, app, guard) = state().await;
    attach(&mut state, entry("one"));
    state.focus_terminal(true);
    let mut snapshot = state.terminal_snapshot().unwrap().clone();
    snapshot.revision = 2;
    snapshot.output_cursor = 70000;
    assert!(state.apply_terminal_replay(TerminalReplay {
        from: 12,
        next: 70000,
        bytes: vec![],
        reset: Some(snapshot.clone()),
        screen: Box::new(snapshot)
    }));
    assert!(state.terminals.gap);
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_pane(f, &state, Rect::new(40, 1, 40, 22), Theme::dark()))
        .unwrap();
    assert_eq!(terminal.backend().buffer()[(41, 2)].symbol(), "R");
    assert_eq!(
        terminal.backend().buffer()[(41, 2)].fg,
        terminal_color(TerminalColor::Indexed(1), false, Theme::dark())
    );
    terminal
        .draw(|f| render_pane(f, &state, Rect::new(40, 1, 40, 22), Theme::light()))
        .unwrap();
    assert_eq!(
        terminal.backend().buffer()[(41, 2)].fg,
        terminal_color(TerminalColor::Indexed(1), false, Theme::light())
    );
    assert_ne!(
        terminal_color(TerminalColor::Indexed(1), false, Theme::light()),
        terminal_color(TerminalColor::Indexed(1), false, Theme::dark())
    );
    assert_eq!(
        state.terminal_size_for_frame(Rect::new(0, 0, 80, 24)),
        TerminalSize { rows: 21, cols: 38 }
    );
    state.chrome.devtools = Some(true);
    assert_eq!(
        state.terminal_size_for_frame(Rect::new(0, 0, 80, 24)).rows,
        20
    );
    state.chrome.vertical_tabs_width = 20;
    assert!(state.terminal_size_for_frame(Rect::new(0, 0, 120, 40)).cols < 58);
    state.apply_terminal_inventory(
        &SessionId("source".into()),
        TerminalInventory {
            enabled: true,
            entries: vec![],
            selected: None,
        },
    );
    assert!(!state.terminal_focused());
    assert!(!state.terminal_ready());
    state.terminal_failure(true);
    assert!(state.chrome.session_terminal);
    assert_eq!(state.note(), Some("Unable to load terminal"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
