use super::*;
use oc_core::{
    core_app::CoreApp,
    domain::SessionId,
    queries::{ChildJob, ChildState, ShellJob},
};

fn shell(id: &str, session: &str) -> ShellJob {
    ShellJob {
        session: SessionId(session.into()),
        shell_id: id.into(),
        location: "/original".into(),
        generation: 7,
        turn: "issued".into(),
        model: "fixture".into(),
        provider: "fixture".into(),
        command: format!("echo {id}"),
        pid: Some(1),
        background: false,
        output: None,
    }
}

#[tokio::test]
async fn one_composer_wraps_scoped_tabs_and_preserves_effect_identity_and_prompt() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("source".into()));
    state.restore_prompt("draft界 remains".into());
    let cursor = state.prompt_layout(40).1;
    state.chrome.session_terminal = true;
    state.apply_shell_jobs(
        (0..8)
            .map(|i| shell(&format!("job{i}"), "source"))
            .chain([shell("foreign", "other")])
            .collect(),
    );
    assert_eq!(
        state.handle_key(KeyAction::Children).await.intent,
        Some(PanelIntent::LoadChildren)
    );
    assert!(state.children_open() && !state.shells_open() && !state.terminals_open());
    assert_eq!(
        state.handle_key(KeyAction::Right).await.intent,
        Some(PanelIntent::LoadShells)
    );
    assert!(state.shells_open() && !state.children_open());
    for _ in 0..6 {
        state.handle_key(KeyAction::Down).await;
    }
    crate::views::render_test(&state, 80, 24);
    {
        let paint = state.composer.painted.borrow();
        let paint = paint.as_ref().unwrap();
        assert_eq!(paint.rows.len(), 5);
        assert!(
            paint
                .rows
                .iter()
                .any(|(_, item)| *item == Item::Shell("job6".into()))
        );
        assert!(
            !paint
                .rows
                .iter()
                .any(|(_, item)| *item == Item::Shell("foreign".into()))
        );
    }
    let intent = state.handle_key(KeyAction::DeleteOrQuit).await.intent;
    assert_eq!(
        intent,
        Some(PanelIntent::CancelShell {
            session: SessionId("source".into()),
            shell_id: "job6".into()
        })
    );
    state.handle_key(KeyAction::Right).await;
    assert!(state.terminals_open());
    assert!(state.handle_key(KeyAction::Enter).await.intent.is_none());
    state.handle_key(KeyAction::Up).await;
    assert!(
        state.terminals_open(),
        "first Up wraps through New instead of closing"
    );
    state.handle_key(KeyAction::Right).await;
    assert!(state.children_open());
    state.handle_key(KeyAction::CtrlA).await;
    assert!(
        crate::views::render_test(&state, 80, 24)
            .iter()
            .any(|line| line.contains("No inactive subagents"))
    );
    state.handle_key(KeyAction::Interrupt).await;
    assert!(!state.composer_open());
    assert_eq!(state.input(), "draft界 remains");
    assert_eq!(state.prompt_layout(40).1, cursor);
    assert!(inbox.try_recv().is_err());
    state.chrome.session_terminal = false;
    state.handle_key(KeyAction::Children).await;
    state.handle_key(KeyAction::Left).await;
    assert!(
        state.shells_open(),
        "disabled terminal capability is not an empty process list"
    );
    state.handle_key(KeyAction::Right).await;
    assert!(state.children_open());
}

#[tokio::test]
async fn composer_painted_pointer_and_remapped_keys_do_not_leak_to_editor_or_stale_jobs() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("source".into()));
    state.restore_prompt("unchanged".into());
    state.apply_shell_jobs(vec![shell("first", "source"), shell("second", "source")]);
    state.handle_key(KeyAction::Shells).await;
    state.chrome.composer_shortcuts.bindings[7] = "f3".into();
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(KeyAction::ComposerNoop)
    );
    assert_eq!(
        state.terminal_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
        Some(KeyAction::DeleteOrQuit)
    );
    assert_eq!(
        state.terminal_key(KeyEvent::new_with_kind(
            KeyCode::F(3),
            KeyModifiers::NONE,
            KeyEventKind::Repeat
        )),
        Some(KeyAction::ComposerNoop)
    );
    let area = Rect::new(0, 0, 80, 24);
    crate::views::render_test(&state, 80, 24);
    let row = state.composer.painted.borrow().as_ref().unwrap().rows[1].0;
    let mouse = |kind| MouseEvent {
        kind,
        column: row.x + 1,
        row: row.y,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Moved), area);
    assert_eq!(
        state.handle_key(KeyAction::DeleteOrQuit).await.intent,
        Some(PanelIntent::CancelShell {
            session: SessionId("source".into()),
            shell_id: "second".into()
        })
    );
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent
            .is_none()
    );
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    state.apply_shell_jobs(vec![shell("first", "source")]);
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent
            .is_none()
    );
    assert!(
        state.shell_viewer().is_none(),
        "the disappeared row cannot retarget an output viewer"
    );
    crate::views::render_test(&state, 80, 24);
    let close = state.composer.painted.borrow().as_ref().unwrap().close;
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(
            MouseEvent {
                kind,
                column: close.x,
                row: close.y,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
    }
    assert!(!state.composer_open());
    assert_eq!(state.input(), "unchanged");
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn linked_child_selects_its_live_owner_after_async_inventory_and_closes_to_parent() {
    let (app, mut inbox, _) = CoreApp::channel(8);
    let mut state = TuiState::new(app, SessionId("current-child".into()));
    state.restore_prompt("parent draft stays owned".into());
    let current = ChildJob {
        parent: SessionId("parent".into()),
        child: SessionId("current-child".into()),
        operation: "current-operation".into(),
        generation: 7,
        location: "/original".into(),
        agent: "helper".into(),
        model: "fixture".into(),
        description: "Current child".into(),
        delivery_id: "current-delivery".into(),
        state: ChildState::Running,
        background: false,
        turn: Some("current-turn".into()),
        result: None,
        message_id: None,
    };
    state.attach_linked_child(current.clone());
    assert!(state.children_open());
    let sibling = ChildJob {
        child: SessionId("sibling".into()),
        operation: "sibling-operation".into(),
        description: "Earlier sibling".into(),
        ..current.clone()
    };
    state.apply_child_jobs(vec![sibling, current.clone()]);
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::OpenChild {
            selected: current.clone()
        })
    );
    assert_eq!(
        state.handle_key(KeyAction::DeleteOrQuit).await.intent,
        Some(PanelIntent::InterruptChild {
            selected: current.clone()
        })
    );
    state.handle_key(KeyAction::Right).await;
    assert!(state.shells_open());
    state.refresh_linked_child(current.clone());
    assert!(
        state.shells_open(),
        "owner refresh must not navigate a live child"
    );
    state.handle_key(KeyAction::Left).await;
    let completed = ChildJob {
        state: ChildState::Completed,
        result: Some("Actual owner completion".into()),
        ..current
    };
    state.apply_child_jobs(vec![completed.clone()]);
    assert_eq!(state.linked_child(), Some(&completed));
    assert!(
        state
            .handle_key(KeyAction::DeleteOrQuit)
            .await
            .intent
            .is_none()
    );
    state.handle_key(KeyAction::CtrlA).await;
    assert_eq!(
        state.handle_key(KeyAction::Enter).await.intent,
        Some(PanelIntent::OpenChild {
            selected: completed
        })
    );
    assert_eq!(
        state.handle_key(KeyAction::Cancel).await.intent,
        Some(PanelIntent::ReturnParent)
    );
    assert!(!state.children_open());
    assert_eq!(state.input(), "parent draft stays owned");
    assert!(inbox.try_recv().is_err());
}
