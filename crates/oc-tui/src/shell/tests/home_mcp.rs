use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use oc_core::queries::{McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};

fn inventory(statuses: &[McpStatus], revision: u64) -> McpSnapshot {
    McpSnapshot {
        binding: McpBinding {
            location: "/footer-fixture".into(),
            generation: 1,
            instance: 1,
        },
        revision,
        servers: statuses
            .iter()
            .enumerate()
            .map(|(index, &status)| McpServerSnapshot {
                id: index.to_string(),
                name: format!("footer-{index}"),
                configured_enabled: status != McpStatus::Disabled,
                status,
                pending_action: None,
                tools: 0,
                diagnostic: None,
                actions: Vec::new(),
            })
            .collect(),
    }
}

#[tokio::test]
async fn home_mcp_live_counts_styles_and_breakpoints_use_the_typed_inventory() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    state.chrome.location = Some("/footer-fixture".into());
    let theme = Theme::dark();
    for (revision, statuses, label, mark_color) in [
        (
            1,
            vec![McpStatus::Pending],
            Some("0 MCP"),
            theme.text_muted(),
        ),
        (
            2,
            vec![McpStatus::Connected; 2],
            Some("2 MCP"),
            theme.success(),
        ),
        (
            3,
            vec![McpStatus::Connected, McpStatus::Failed, McpStatus::Failed],
            Some("2 MCP failed"),
            theme.error(),
        ),
        (
            4,
            vec![McpStatus::Disabled, McpStatus::NeedsAuth],
            Some("0 MCP"),
            theme.text_muted(),
        ),
        (5, Vec::new(), None, theme.text_muted()),
    ] {
        state.apply_mcp_snapshot(inventory(&statuses, revision));
        for (width, height) in [
            (43, 24),
            (44, 24),
            (63, 24),
            (64, 11),
            (64, 12),
            (80, 24),
            (120, 40),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render(frame, &state)).unwrap();
            let buffer = terminal.backend().buffer();
            let footer_y = height - if height < 16 { 1 } else { 2 };
            let row = (0..width)
                .map(|x| buffer[(x, footer_y)].symbol())
                .collect::<String>();
            if width < 44 || height < 12 || label.is_none() {
                assert!(!row.contains("MCP"), "{width}x{height}: {row}");
                assert!(home_mcp_rect(&state, Rect::new(0, 0, width, height)).is_none());
                if width == 43 {
                    let rows = screen(&state, width, height);
                    assert_eq!(
                        rows.iter().position(|row| row.contains("█▀▀█ █▀▀█")),
                        Some(6),
                        "An unmounted footer must not reserve inventory space"
                    );
                    assert!(rows[16].contains("Ask anything"));
                }
                continue;
            }
            let text = format!(
                "⊙ {}{}",
                label.unwrap(),
                if width >= 64 { " /mcps" } else { "" }
            );
            assert!(row[2..].starts_with(&text), "{width}x{height}: {row}");
            assert_eq!(buffer[(2, footer_y)].fg, mark_color);
            assert_eq!(buffer[(3, footer_y)].fg, mark_color);
            assert_eq!(buffer[(4, footer_y)].fg, theme.text());
            for x in 0..2 {
                assert_eq!(buffer[(x, footer_y)].fg, Color::Rgb(255, 255, 255));
            }
            let rect = home_mcp_rect(&state, Rect::new(0, 0, width, height)).unwrap();
            assert_eq!(
                rect,
                Rect::new(2, footer_y, UnicodeWidthStr::width(text.as_str()) as u16, 1)
            );
            if width >= 64 {
                let command_x = rect.right() - 5;
                assert_eq!(
                    buffer[(command_x - 1, rect.y)].fg,
                    Color::Rgb(255, 255, 255)
                );
                assert_eq!(buffer[(command_x, rect.y)].fg, theme.text_muted());
            } else {
                assert!(!row.contains("/mcps"));
                assert!(screen(&state, width, height)[13].contains("Ask anything"));
            }
        }
    }
    // A constructor announcement is not a live removal/recovery event.
    state.apply_mcp_snapshot(inventory(&[McpStatus::Connected], 0));
    assert_eq!(state.mcp_status_counts(), None);
    let mut foreign = inventory(&[McpStatus::Failed], 6);
    foreign.binding.location = "/foreign".into();
    state.apply_mcp_snapshot(foreign);
    assert_eq!(state.mcp_status_counts(), None);
}

#[tokio::test]
async fn home_mcp_item_opens_the_existing_modal_without_changing_draft_or_effects() {
    let mut state = golden_state().await;
    state.home = true;
    state.chrome.devtools = Some(false);
    state.chrome.location = Some("/footer-fixture".into());
    state.apply_mcp_snapshot(inventory(&[McpStatus::Connected], 1));
    state.handle_key(KeyAction::Char('Ω')).await;
    let area = Rect::new(0, 0, 120, 40);
    let item = home_mcp_rect(&state, area).unwrap();
    let mouse = |kind, x| MouseEvent {
        kind,
        column: x,
        row: item.y,
        modifiers: KeyModifiers::NONE,
    };
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let caret = terminal.get_cursor_position().unwrap();
    for kind in [
        MouseEventKind::Moved,
        MouseEventKind::Down(MouseButton::Left),
    ] {
        assert!(
            state
                .handle_mouse(mouse(kind, item.x), area)
                .intent
                .is_none()
        );
        assert_eq!(state.panel(), &crate::app::TuiPanel::None);
    }
    let opened = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), item.x), area);
    assert!(matches!(
        opened.intent,
        Some(crate::app::PanelIntent::LoadMcps)
    ));
    assert_eq!(state.panel(), &crate::app::TuiPanel::Mcps);
    assert_eq!(state.input(), "Ω");
    // A modal owns its full paint; the underlying Home shortcut stays inert.
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), item.x), area)
            .intent
            .is_none()
    );
    state.handle_key(KeyAction::Cancel).await;
    terminal.draw(|frame| render(frame, &state)).unwrap();
    assert_eq!(terminal.get_cursor_position().unwrap(), caret);
    assert_eq!(state.input(), "Ω");
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Drag(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        assert!(
            state
                .handle_mouse(mouse(kind, item.x), area)
                .intent
                .is_none()
        );
    }
    assert_eq!(state.panel(), &crate::app::TuiPanel::None);
    assert!(
        state
            .handle_mouse(
                mouse(MouseEventKind::Up(MouseButton::Left), item.right()),
                area
            )
            .intent
            .is_none()
    );
    state.home = false;
    assert!(home_mcp_rect(&state, area).is_none());
}
