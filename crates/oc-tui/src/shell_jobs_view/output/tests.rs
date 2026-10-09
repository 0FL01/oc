use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oc_core::core_app::CoreApp;

fn job() -> ShellJob {
    ShellJob {
        session: SessionId("source".into()),
        shell_id: "original".into(),
        location: "/original".into(),
        generation: 7,
        turn: "issuing-turn".into(),
        model: "fixture".into(),
        provider: "fixture".into(),
        command: "echo bounded output".into(),
        pid: None,
        background: false,
        output: None,
    }
}
fn snapshot(job: ShellJob, display: String, state: &str, exit: Option<i32>) -> ShellSnapshot {
    ShellSnapshot {
        job,
        state: state.into(),
        stdout_cursor: display.len() as u64,
        stderr_cursor: 0,
        truncated: false,
        text: "legacy technical envelope must not appear".into(),
        display,
        display_omitted: false,
        exit,
        signal: None,
    }
}

#[tokio::test]
async fn modal_plain_snapshot_scrolling_final_flush_and_pointer_are_original_owner_scoped() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("source".into()));
    state.restore_prompt("untouched draft界".into());
    let original = job();
    state.apply_shell_jobs(vec![original.clone()]);
    state.handle_key(KeyAction::Shells).await;
    state.handle_key(KeyAction::Enter).await;
    let body = (0..100)
        .map(|i| format!("row{i:03}\r\n"))
        .collect::<String>();
    state.apply_shell_snapshot(snapshot(original.clone(), body.clone(), "running", None));
    for (w, h) in [(80, 24), (120, 40), (160, 48)] {
        let lines = crate::views::render_test(&state, w, h).join("\n");
        assert!(lines.contains("Running") && lines.contains("row099"));
        assert!(!lines.contains("legacy technical") && !lines.contains("generation 7"));
        let paint = state.shells.output_paint.borrow();
        let paint = paint.as_ref().unwrap();
        assert_eq!(paint.area.width, 116.min(w - 2));
        assert_eq!(paint.area.y, (h - paint.area.height) / 2);
        assert_eq!(paint.viewport.height, (h * 3 / 5).saturating_sub(6).max(3));
        assert_eq!(
            paint.max_scroll,
            101 - usize::from(paint.viewport.height),
            "plain output retains its final newline row"
        );
    }
    let area = Rect::new(0, 0, 120, 40);
    crate::views::render_test(&state, 120, 40);
    state.handle_key(KeyAction::Home).await;
    let lines = crate::views::render_test(&state, 120, 40).join("\n");
    assert!(lines.contains("row000") && !lines.contains("row099"));
    state.apply_shell_snapshot(snapshot(
        original.clone(),
        format!("{body}new tail\n"),
        "running",
        None,
    ));
    crate::views::render_test(&state, 120, 40);
    assert_eq!(state.shells.output_scroll.get(), 0);
    state.handle_key(KeyAction::End).await;
    assert!(
        crate::views::render_test(&state, 120, 40)
            .join("\n")
            .contains("new tail")
    );
    state.handle_key(KeyAction::PageUp).await;
    assert!(!state.shells.output_follow.get());
    let before = state.shells.output_scroll.get();
    let viewport = state
        .shells
        .output_paint
        .borrow()
        .as_ref()
        .unwrap()
        .viewport;
    let mouse = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(
        mouse(MouseEventKind::ScrollUp, viewport.x, viewport.y),
        area,
    );
    assert!(state.shells.output_scroll.get() < before);
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
        Some(KeyAction::ComposerNoop)
    );
    assert!(state.handle_paste("must not reach draft").intent.is_none());
    assert_eq!(state.input(), "untouched draft界");
    let close = state.shells.output_paint.borrow().as_ref().unwrap().close;
    state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), close.x, close.y),
        area,
    );
    assert!(state.shell_viewer().is_some(), "bare release cannot close");
    // Interior clicks cannot activate the Shell rows or the editor below it.
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        assert!(
            state
                .handle_mouse(mouse(kind, viewport.x, viewport.y), area)
                .intent
                .is_none()
        );
    }
    state.apply_shell_jobs(Vec::new());
    let mut foreign = original.clone();
    foreign.session = SessionId("other".into());
    state.apply_shell_snapshot(snapshot(
        foreign,
        "foreign data".into(),
        "completed",
        Some(0),
    ));
    assert_eq!(state.shells.output_state, "running");
    state.apply_shell_snapshot(snapshot(
        original.clone(),
        "final flush\n".into(),
        "completed",
        Some(7),
    ));
    assert_eq!(state.shell_viewer(), Some(&original));
    state.handle_key(KeyAction::End).await;
    let lines = crate::views::render_test(&state, 120, 40).join("\n");
    assert!(lines.contains("Exited · code 7") && lines.contains("final flush"));
    state.apply_shell_read_failure(&original);
    assert!(
        crate::views::render_test(&state, 120, 40)
            .join("\n")
            .contains("Unable to read shell output")
    );
    let close = state.shells.output_paint.borrow().as_ref().unwrap().close;
    state.handle_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), close.x, close.y),
        area,
    );
    state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), close.x, close.y),
        Rect::new(0, 0, 80, 24),
    );
    assert!(
        state.shell_viewer().is_some(),
        "stale resized paint has no authority"
    );
    crate::views::render_test(&state, 120, 40);
    let close = state.shells.output_paint.borrow().as_ref().unwrap().close;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(mouse(kind, close.x, close.y), area);
    }
    assert!(state.shell_viewer().is_none() && state.shells_open());
    assert_eq!(state.input(), "untouched draft界");
    assert!(inbox.try_recv().is_err());
}
