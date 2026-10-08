use super::*;
use crate::{commands::CommandAction, dialog::DialogSize};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use oc_core::queries::{McpAction, McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};
use ratatui::{Terminal, backend::TestBackend, style::Modifier};

fn inventory() -> McpSnapshot {
    McpSnapshot {
        binding: McpBinding {
            location: "/select-fixture".into(),
            generation: 2,
            instance: 7,
        },
        revision: 1,
        servers: [McpStatus::Connected, McpStatus::Disabled, McpStatus::Failed]
            .into_iter()
            .enumerate()
            .map(|(index, status)| McpServerSnapshot {
                id: format!("opaque-{index}"),
                name: format!("configured-{index}-Ω"),
                configured_enabled: status != McpStatus::Disabled,
                status,
                pending_action: None,
                tools: 0,
                diagnostic: None,
                actions: vec![match status {
                    McpStatus::Connected => McpAction::Disconnect,
                    McpStatus::Failed => McpAction::Retry,
                    _ => McpAction::Connect,
                }],
            })
            .collect(),
    }
}

fn raw(state: &mut TuiState, code: KeyCode, modifiers: KeyModifiers) -> KeyOutcome {
    state
        .terminal_key(KeyEvent::new(code, modifiers))
        .map_or_else(KeyOutcome::default, |action| state.handle_panel_key(action))
}

fn mouse(state: &mut TuiState, area: Rect, kind: MouseEventKind, x: u16, y: u16) -> KeyOutcome {
    state.handle_mouse(
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
        area,
    )
}

#[tokio::test]
async fn select_mcp_footer_focus_styles_controls_and_stale_mouse_share_geometry() {
    let mut state = fresh_state("select-footer").await;
    state.chrome.location = Some("/select-fixture".into());
    state.handle_paste("composer Ω界");
    state.editor.move_to(5, false);
    let mut snapshot = inventory();
    state.apply_mcp_snapshot(snapshot.clone());
    state.run_command(CommandAction::OpenMcps);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    let caret = terminal.backend().cursor_position();
    assert_eq!(state.mcp_footer_action().title, "disconnect");
    raw(&mut state, KeyCode::Tab, KeyModifiers::NONE);
    assert!(state.select.action_focused());
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert_eq!(
        terminal.backend().cursor_position(),
        caret,
        "logical action focus retains Search caret"
    );
    let area = Rect::new(0, 0, 120, 40);
    let action = state.mcp_footer_action();
    let rect =
        state
            .select
            .footer_action_rect(area, DialogSize::Medium, &state.modal_options(), &action);
    let buffer = terminal.backend().buffer();
    let theme = Theme::dark();
    let color = |path: &str| {
        theme
            .color(&format!("@dialog.{path}"))
            .or_else(|| theme.color(path))
            .unwrap()
    };
    assert_eq!(
        buffer[(rect.x, rect.y)].fg,
        color("text.action.primary.$focused")
    );
    assert_eq!(
        buffer[(rect.x, rect.y)].bg,
        color("background.action.primary.$focused")
    );
    assert!(buffer[(rect.x, rect.y)].modifier.contains(Modifier::BOLD));
    assert!(
        !buffer[(rect.x + 11, rect.y)]
            .modifier
            .contains(Modifier::BOLD),
        "shortcut is not title bold"
    );
    assert_eq!(buffer[(34, 15)].fg, color("text.muted"));
    assert_eq!(buffer[(34, 15)].bg, color("background.raised.high"));
    assert!(!buffer[(34, 15)].modifier.contains(Modifier::BOLD));
    assert!(
        buffer[(75, 15)].modifier.contains(Modifier::BOLD),
        "Connected has independent intrinsic bold"
    );
    let Some(PanelIntent::McpControl(control)) =
        raw(&mut state, KeyCode::Enter, KeyModifiers::NONE).intent
    else {
        panic!("focused Enter must control, not select")
    };
    assert_eq!(
        (&control.binding, control.server.as_str(), control.action),
        (&snapshot.binding, "opaque-0", McpAction::Disconnect)
    );
    raw(&mut state, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(!state.select.action_focused());
    raw(&mut state, KeyCode::BackTab, KeyModifiers::SHIFT);
    assert!(state.select.action_focused());
    mouse(&mut state, area, MouseEventKind::Moved, 34, 15);
    assert!(
        !state.select.action_focused(),
        "even same-row hover clears action focus"
    );
    raw(&mut state, KeyCode::Char('p'), KeyModifiers::CONTROL);
    assert_eq!(state.select.cursor, 2);
    assert_eq!(state.mcp_footer_action().title, "retry");
    raw(&mut state, KeyCode::Char('n'), KeyModifiers::CONTROL);
    assert_eq!(state.select.cursor, 0);
    for modifiers in [
        KeyModifiers::CONTROL,
        KeyModifiers::SHIFT,
        KeyModifiers::ALT,
    ] {
        mouse(
            &mut state,
            area,
            MouseEventKind::Down(MouseButton::Left),
            rect.x,
            rect.y,
        );
        assert!(
            state
                .handle_mouse(
                    MouseEvent {
                        kind: MouseEventKind::Up(MouseButton::Left),
                        column: rect.x,
                        row: rect.y,
                        modifiers,
                    },
                    area,
                )
                .intent
                .is_none(),
            "a modified release cannot activate an unmodified footer press"
        );
    }
    mouse(
        &mut state,
        area,
        MouseEventKind::Down(MouseButton::Left),
        rect.x,
        rect.y,
    );
    let Some(PanelIntent::McpControl(clicked)) = mouse(
        &mut state,
        area,
        MouseEventKind::Up(MouseButton::Left),
        rect.x,
        rect.y,
    )
    .intent
    else {
        panic!("footer click")
    };
    assert_eq!(clicked, control);
    for invalidation in 0..3 {
        mouse(
            &mut state,
            area,
            MouseEventKind::Down(MouseButton::Left),
            rect.x,
            rect.y,
        );
        match invalidation {
            0 => {
                mouse(
                    &mut state,
                    area,
                    MouseEventKind::Drag(MouseButton::Left),
                    rect.x,
                    rect.y,
                );
            }
            1 => {
                state.handle_paste("configured");
            }
            _ => {
                snapshot.binding.instance += 1;
                snapshot.revision += 1;
                state.apply_mcp_snapshot(snapshot.clone());
            }
        }
        assert!(
            mouse(
                &mut state,
                area,
                MouseEventKind::Up(MouseButton::Left),
                rect.x,
                rect.y
            )
            .intent
            .is_none()
        );
    }
    mouse(
        &mut state,
        area,
        MouseEventKind::Down(MouseButton::Left),
        rect.x,
        rect.y,
    );
    assert!(
        mouse(
            &mut state,
            Rect::new(0, 0, 80, 24),
            MouseEventKind::Up(MouseButton::Left),
            rect.x,
            rect.y
        )
        .intent
        .is_none()
    );
    state.handle_panel_key(KeyAction::Interrupt);
    assert_eq!(state.select.query, "");
    snapshot.revision += 1;
    snapshot.servers[2].diagnostic = Some(oc_core::queries::ServiceDiagnostic {
        kind: oc_core::queries::ServiceKind::Mcp,
        service: "safe-diagnostic-id".into(),
        source: "safe-source".into(),
        field: vec!["mcp".into()],
        stage: oc_core::queries::ServiceStage::Initialize,
        code: oc_core::queries::ServiceCode::ConnectionFailed,
        action: oc_core::queries::ServiceAction::RetryConnection,
    });
    state.apply_mcp_snapshot(snapshot.clone());
    raw(&mut state, KeyCode::End, KeyModifiers::NONE);
    let narrow_area = Rect::new(0, 0, 54, 24);
    let mut narrow = Terminal::new(TestBackend::new(54, 24)).unwrap();
    narrow
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    let action = state.mcp_footer_action();
    let stacked = state.select.footer_action_rect(
        narrow_area,
        DialogSize::Medium,
        &state.modal_options(),
        &action,
    );
    assert_eq!(
        narrow.backend().buffer()[(stacked.x, stacked.y)].symbol(),
        "r"
    );
    assert_eq!(
        narrow.backend().buffer()[(stacked.x, stacked.y - 1)].symbol(),
        "e",
        "narrow footer places its hint above the actual action hit"
    );
    mouse(
        &mut state,
        narrow_area,
        MouseEventKind::Down(MouseButton::Left),
        stacked.x,
        stacked.y,
    );
    let Some(PanelIntent::McpControl(stacked_control)) = mouse(
        &mut state,
        narrow_area,
        MouseEventKind::Up(MouseButton::Left),
        stacked.x,
        stacked.y,
    )
    .intent
    else {
        panic!("stacked footer uses the painted action row")
    };
    assert_eq!(stacked_control.server, "opaque-2");
    assert_eq!(stacked_control.action, McpAction::Retry);
    raw(&mut state, KeyCode::Home, KeyModifiers::NONE);
    snapshot.revision += 1;
    snapshot.servers[0].pending_action = Some(McpAction::Disconnect);
    snapshot.servers[0].actions.clear();
    state.apply_mcp_snapshot(snapshot.clone());
    assert_eq!(
        state.mcp_footer_action().title,
        "disconnect",
        "loading is not a separate caption"
    );
    assert!(!state.mcp_footer_action().enabled);
    raw(&mut state, KeyCode::Tab, KeyModifiers::NONE);
    assert!(!state.select.action_focused());
    assert!(
        raw(&mut state, KeyCode::Char(' '), KeyModifiers::NONE)
            .intent
            .is_none()
    );
    state.handle_paste("no-such-row");
    assert!(state.modal_options().is_empty());
    assert_eq!(
        state.mcp_footer_action().title,
        "disconnect",
        "no match retains last focused caption"
    );
    assert!(!state.mcp_footer_action().enabled);
    state.handle_panel_key(KeyAction::Interrupt);
    snapshot.revision += 1;
    snapshot.servers.clear();
    state.apply_mcp_snapshot(snapshot);
    assert_eq!(state.mcp_footer_action().title, "connect");
    assert!(!state.mcp_footer_action().enabled);
    state.close_panel();
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"composer Ω界".to_owned(), 5)
    );
}

#[tokio::test]
async fn select_effective_remaps_own_dispatch_hint_chords_and_disable_old_defaults() {
    let mut state = fresh_state("select-remaps").await;
    state.chrome.location = Some("/select-fixture".into());
    state.handle_paste("preserve Ω draft");
    state.editor.move_to(3, false);
    state.apply_mcp_snapshot(inventory());
    state.run_command(CommandAction::OpenMcps);
    let keys = &mut state.chrome.dialog_shortcuts;
    keys.previous = "ctrl+k".into();
    keys.next = "ctrl+j".into();
    keys.page_up = "alt+u".into();
    keys.page_down = "alt+d".into();
    keys.home = "alt+h".into();
    keys.end = "alt+e".into();
    keys.submit = "f3".into();
    keys.mcp_toggle = "f2,ctrl+g t".into();
    assert_eq!(state.mcp_footer_action().shortcut, "f2 ctrl+g t");
    for key in [
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
    ] {
        assert!(
            state.terminal_key(key).is_none(),
            "old defaults disabled: {key:?}"
        );
    }
    raw(&mut state, KeyCode::Char('j'), KeyModifiers::CONTROL);
    assert_eq!(state.select.cursor, 1);
    raw(&mut state, KeyCode::Char('k'), KeyModifiers::CONTROL);
    assert_eq!(state.select.cursor, 0);
    raw(&mut state, KeyCode::Char(' '), KeyModifiers::NONE);
    assert_eq!(
        state.select.query, " ",
        "old toggle is now ordinary Search input"
    );
    state.handle_panel_key(KeyAction::Backspace);
    raw(&mut state, KeyCode::Char('e'), KeyModifiers::ALT);
    assert_eq!(state.select.cursor, 2);
    raw(&mut state, KeyCode::Char('u'), KeyModifiers::ALT);
    assert_eq!(state.select.cursor, 2, "source -10 wraps to the last item");
    raw(&mut state, KeyCode::Char('d'), KeyModifiers::ALT);
    assert_eq!(state.select.cursor, 0, "source +10 wraps to the first item");
    raw(&mut state, KeyCode::Char('h'), KeyModifiers::ALT);
    assert_eq!(state.select.cursor, 0);
    let leader = state
        .terminal_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .unwrap();
    state.handle_key(leader).await;
    assert!(state.leader_pending());
    assert!(matches!(
        raw(&mut state, KeyCode::Char('t'), KeyModifiers::NONE).intent,
        Some(PanelIntent::McpControl(_))
    ));
    assert!(!state.leader_pending());
    raw(&mut state, KeyCode::Tab, KeyModifiers::NONE);
    assert!(matches!(
        raw(&mut state, KeyCode::F(3), KeyModifiers::NONE).intent,
        Some(PanelIntent::McpControl(_))
    ));
    let mut catalog = snapshot();
    catalog.chrome.location = state.chrome.location.clone();
    catalog.chrome.dialog_shortcuts = state.chrome.dialog_shortcuts.clone();
    catalog.chrome.dialog_shortcuts.mcp_toggle.clear();
    state.apply_catalog(catalog);
    assert!(!state.select.action_focused());
    assert_eq!(state.mcp_footer_action().shortcut, "");
    raw(&mut state, KeyCode::Tab, KeyModifiers::NONE);
    assert!(!state.select.action_focused());
    state.handle_panel_key(KeyAction::Interrupt);
    assert_eq!(state.panel(), &TuiPanel::None);
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"preserve Ω draft".to_owned(), 3)
    );
}
