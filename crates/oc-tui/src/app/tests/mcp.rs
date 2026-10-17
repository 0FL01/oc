use super::*;
use crate::commands::CommandAction;
use oc_core::queries::{McpAction, McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};
use ratatui::{Terminal, backend::TestBackend, style::Modifier, text::Line};

fn sidebar_inventory() -> McpSnapshot {
    use oc_core::queries::{
        ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
    };
    McpSnapshot {
        binding: McpBinding {
            location: "/sidebar-input".into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: [
            McpStatus::Connected,
            McpStatus::Failed,
            McpStatus::NeedsAuth,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, status)| {
            let id = format!("sidebar-opaque-{index}");
            McpServerSnapshot {
                id: id.clone(),
                name: "duplicate display name".into(),
                configured_enabled: true,
                status,
                pending_action: None,
                tools: 0,
                actions: Vec::new(),
                diagnostic: (status != McpStatus::Connected).then(|| ServiceDiagnostic {
                    kind: ServiceKind::Mcp,
                    service: id,
                    source: "source-safe/config".into(),
                    field: vec!["mcp".into()],
                    stage: ServiceStage::Initialize,
                    code: ServiceCode::ConnectionFailed,
                    action: ServiceAction::RetryConnection,
                    details: None,
                }),
            }
        })
        .collect(),
    }
}

#[tokio::test]
async fn sidebar_mcp_mouse_opens_the_opaque_id_and_retains_the_load_intent() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = fresh_state("sidebar-id").await;
    state.chrome.location = Some("/sidebar-input".into());
    state.set_service_feedback_visible(false);
    let mut inventory = sidebar_inventory();
    state.apply_mcp_snapshot(inventory.clone());
    state.handle_paste("preserved Ω界 sidebar draft");
    state.editor.move_to(5, false);
    let caret = state.editor.cursor;
    let area = ratatui::layout::Rect::new(0, 0, 160, 32);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    for (id, details) in [
        ("sidebar-opaque-1", true),
        ("sidebar-opaque-2", false),
        ("sidebar-opaque-0", false),
    ] {
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        let layout = crate::shell::sidebar_layout(&state, area).unwrap();
        let rect = layout
            .rows
            .iter()
            .find(|(_, index)| state.sidebar_mcp_servers()[*index].id == id)
            .unwrap()
            .0;
        let mouse = |kind| MouseEvent {
            kind,
            column: rect.x + 2,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        };
        assert!(
            state
                .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area)
                .intent
                .is_none()
        );
        assert_eq!(
            state
                .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
                .intent,
            Some(PanelIntent::LoadMcps)
        );
        assert_eq!(state.mcp_focused.as_deref(), Some(id));
        assert_eq!(state.mcp_detail_server().is_some(), details);
        if details {
            assert_eq!(state.mcp_detail_server().unwrap().id, id);
            inventory.revision += 1;
            inventory.servers.swap(0, 1);
            state.apply_mcp_snapshot(inventory.clone());
            assert_eq!(state.mcp_detail_server().unwrap().id, id);
            assert_eq!(state.modal_options()[state.select.cursor].value, id);
        }
        assert_eq!(state.input(), "preserved Ω界 sidebar draft");
        assert_eq!(state.editor.cursor, caret);
        assert!(!state.has_pending_submission());
        state.close_panel();
    }
}

#[tokio::test]
async fn sidebar_mcp_pointer_invalidation_never_retargets_a_release() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let mut state = fresh_state("sidebar-gestures").await;
    state.chrome.location = Some("/sidebar-input".into());
    state.set_service_feedback_visible(false);
    let area = ratatui::layout::Rect::new(0, 0, 160, 32);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    for (revision, action) in [
        "wheel", "drag", "modified", "resize", "key", "paste", "modal", "park", "accepted",
        "rejected",
    ]
    .into_iter()
    .enumerate()
    {
        let mut snapshot = sidebar_inventory();
        snapshot.revision = revision as u64 * 2 + 1;
        state.apply_mcp_snapshot(snapshot.clone());
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        let rect = crate::shell::sidebar_layout(&state, area).unwrap().rows[0].0;
        let mouse = |kind, modifiers| MouseEvent {
            kind,
            column: rect.x + 2,
            row: rect.y,
            modifiers,
        };
        state.handle_mouse(
            mouse(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
            area,
        );
        match action {
            "wheel" => {
                state.handle_mouse(mouse(MouseEventKind::ScrollDown, KeyModifiers::NONE), area);
            }
            "drag" => {
                state.handle_mouse(
                    mouse(MouseEventKind::Drag(MouseButton::Left), KeyModifiers::NONE),
                    area,
                );
            }
            "modified" => {
                state.handle_mouse(
                    mouse(MouseEventKind::Up(MouseButton::Left), KeyModifiers::CONTROL),
                    area,
                );
            }
            "resize" => {
                state.resize_mouse_position(ratatui::layout::Rect::new(0, 0, 120, 24));
                state.resize_mouse_position(area);
            }
            "modal" => {
                state.run_command(CommandAction::OpenMcps);
                state.close_panel();
            }
            "park" => state.clear_mouse_position(),
            "key" => {
                state.handle_key(KeyAction::Char('x')).await;
            }
            "paste" => {
                state.handle_paste("x");
            }
            "accepted" => {
                snapshot.revision += 1;
                snapshot.servers.swap(0, 1);
                state.apply_mcp_snapshot(snapshot);
                // An inventory that has not painted yet cannot own a fresh press either.
                state.handle_mouse(
                    mouse(MouseEventKind::Down(MouseButton::Left), KeyModifiers::NONE),
                    area,
                );
            }
            "rejected" => {
                snapshot.binding.location = "/foreign".into();
                state.apply_mcp_snapshot(snapshot);
            }
            _ => unreachable!(),
        }
        let outcome = state.handle_mouse(
            mouse(MouseEventKind::Up(MouseButton::Left), KeyModifiers::NONE),
            area,
        );
        if action == "rejected" {
            assert_eq!(outcome.intent, Some(PanelIntent::LoadMcps));
            assert_eq!(state.mcp_focused.as_deref(), Some("sidebar-opaque-0"));
            state.close_panel();
        } else {
            assert!(outcome.intent.is_none(), "{action}");
            assert_eq!(*state.panel(), TuiPanel::None, "{action}");
        }
    }
}

#[tokio::test]
async fn mcp_error_details_are_read_only_preserve_list_and_own_copy_investigation() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use oc_core::queries::{
        ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage,
    };
    use ratatui::layout::Rect;

    let mut state = fresh_state("mcp-error-details").await;
    state.chrome.location = Some("/mcp-details".into());
    state.handle_paste("preserve Ω界 composer");
    state.editor.move_to(5, false);
    let diagnostic = ServiceDiagnostic {
        kind: ServiceKind::Mcp,
        service: "server-opaque-diagnostic".into(),
        source: "source-safe/config".into(),
        field: vec!["mcp".into(), "configured-Ω".into()],
        stage: ServiceStage::Initialize,
        code: ServiceCode::ConnectionFailed,
        action: ServiceAction::RetryConnection,
        details: None,
    };
    let mut snapshot = McpSnapshot {
        binding: McpBinding {
            location: "/mcp-details".into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: vec![McpServerSnapshot {
            id: "opaque-action-id".into(),
            name: "configured-Ω".into(),
            configured_enabled: true,
            status: McpStatus::Failed,
            pending_action: None,
            tools: 0,
            diagnostic: Some(diagnostic.clone()),
            actions: vec![McpAction::Retry],
        }],
    };
    state.apply_mcp_snapshot(snapshot.clone());
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    let composer_caret = terminal.backend().cursor_position();
    state.run_command(CommandAction::OpenMcps);
    state.handle_paste("configured");
    let original_query = state.select.query.clone();
    let original_selection = state.select.cursor;
    assert!(state.mcp_enter().intent.is_none());
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert!(!terminal.backend().cursor_visible());
    assert_eq!(
        crate::views::cursor_color(&state),
        ratatui::style::Color::Reset
    );
    let painted = crate::views::render_test(&state, 120, 40).join("\n");
    for text in [
        "MCP server: configured-Ω",
        "i investigate",
        "c copy details",
        "↑/↓ scroll",
    ] {
        assert!(painted.contains(text), "missing {text}: {painted}");
    }
    assert!(!painted.contains("Search"));
    assert!(
        state
            .handle_panel_key(KeyAction::Char('x'))
            .intent
            .is_none()
    );
    state.handle_paste("must not leak into the list/composer");
    assert_eq!(state.select.query, original_query);
    assert_eq!(state.input(), "preserve Ω界 composer");
    assert!(state.mcp_toggle().intent.is_none());

    state.handle_panel_key(KeyAction::Char('c'));
    assert!(
        !state.mcp_detail_copied(),
        "request is not transport success"
    );
    assert_eq!(
        state.take_copy_request(),
        Some(format!("MCP server: configured-Ω\nError: {diagnostic}"))
    );
    state.report_clipboard_result(Err("clipboard unavailable".into()));
    assert!(!state.mcp_detail_copied());
    state.handle_panel_key(KeyAction::Char('c'));
    assert!(state.take_copy_request().is_some());
    state.report_clipboard_result(Ok(()));
    assert!(state.mcp_detail_copied());
    assert!(
        crate::views::render_test(&state, 120, 40)
            .join("\n")
            .contains("✓ copied")
    );

    // A newly protected name cannot retain stale copied feedback/payload.
    state.handle_panel_key(KeyAction::Char('c'));
    snapshot.revision += 1;
    snapshot.servers[0].name = "server-masked".into();
    state.apply_mcp_snapshot(snapshot.clone());
    assert!(state.take_copy_request().is_none());
    assert!(!state.mcp_detail_copied());
    assert!(state.mcp_detail_server().is_some());
    state.handle_panel_key(KeyAction::Cancel);
    assert_eq!(
        (&state.select.query, state.select.cursor),
        (&original_query, original_selection)
    );
    state.close_panel();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert!(terminal.backend().cursor_visible());
    assert_eq!(terminal.backend().cursor_position(), composer_caret);
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"preserve Ω界 composer".to_string(), 5)
    );

    // Long owner diagnostics scroll within the same bounded surface after
    // resize; controls and the originating list do not become searchable rows.
    snapshot.revision += 1;
    snapshot.servers[0].diagnostic.as_mut().unwrap().field = vec!["safe-field-Ω".repeat(140)];
    state.apply_mcp_snapshot(snapshot.clone());
    state.run_command(CommandAction::OpenMcps);
    state.mcp_enter();
    crate::views::render_test(&state, 80, 24);
    let area = Rect::new(0, 0, 80, 24);
    let layout = crate::dialog::mcp_detail_layout(&state, area);
    assert!(layout.count > layout.body.height as usize);
    state.handle_panel_key(KeyAction::End);
    assert_eq!(
        state.mcp_detail_scroll(),
        layout.count - layout.body.height as usize
    );
    state.handle_panel_key(KeyAction::PageUp);
    assert_eq!(
        state.mcp_detail_scroll(),
        (layout.count - layout.body.height as usize).saturating_sub(20)
    );
    state.handle_panel_key(KeyAction::Home);
    let mouse = |kind, x, y| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    state.handle_mouse(
        mouse(MouseEventKind::ScrollDown, layout.body.x, layout.body.y),
        area,
    );
    assert_eq!(state.mcp_detail_scroll(), 3);
    state.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            layout.copy.x,
            layout.copy.y,
        ),
        area,
    );
    state.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            layout.copy.x,
            layout.copy.y,
        ),
        Rect::new(0, 0, 120, 40),
    );
    assert!(
        state.take_copy_request().is_none(),
        "resize invalidates action press"
    );
    crate::views::render_test(&state, 80, 24);
    state.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            layout.copy.x,
            layout.copy.y,
        ),
        area,
    );
    state.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            layout.copy.x,
            layout.copy.y,
        ),
        area,
    );
    assert!(
        state.take_copy_request().is_some(),
        "real copy hit uses rendered geometry"
    );
    state.report_clipboard_result(Ok(()));
    let layout = crate::dialog::mcp_detail_layout(&state, area);
    state.handle_mouse(
        mouse(
            MouseEventKind::Down(MouseButton::Left),
            layout.investigate.x,
            layout.investigate.y,
        ),
        area,
    );
    let outcome = state.handle_mouse(
        mouse(
            MouseEventKind::Up(MouseButton::Left),
            layout.investigate.x,
            layout.investigate.y,
        ),
        area,
    );
    assert!(outcome.intent.is_none());
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(
        state
            .input()
            .starts_with("MCP server: server-masked\nInvestigate this admitted failure")
    );
    assert!(state.input().len() <= MAX_INPUT_BYTES);
    assert!(!state.has_pending_submission());
    assert_eq!(state.status(), &TuiStatus::Idle);

    state.run_command(CommandAction::OpenMcps);
    state.mcp_enter();
    snapshot.binding.instance += 1;
    snapshot.revision += 1;
    state.apply_mcp_snapshot(snapshot);
    assert!(
        state.mcp_detail.is_none(),
        "replacement owner retires stale detail"
    );
}

#[tokio::test]
async fn mcp_status_uses_typed_tone_loading_and_intrinsic_bold_with_selected_override() {
    let mut state = fresh_state("mcp-status-style").await;
    state.chrome.location = Some("/mcp-style".into());
    state.handle_paste("preserved Ω界 draft");
    let cases = [
        (
            McpStatus::Connected,
            None,
            "Connected ✓",
            "text.feedback.success.base",
            true,
        ),
        (McpStatus::Disabled, None, "Disabled ○", "text.muted", false),
        (
            McpStatus::Failed,
            None,
            "Failed !",
            "text.feedback.error.base",
            false,
        ),
        (
            McpStatus::NeedsAuth,
            None,
            "Sign in required →",
            "text.feedback.warning.base",
            false,
        ),
        (
            McpStatus::Pending,
            None,
            "Connecting …",
            "text.muted",
            false,
        ),
        (
            McpStatus::Connected,
            Some(McpAction::Disconnect),
            "Connecting …",
            "text.muted",
            false,
        ),
        (
            McpStatus::Failed,
            Some(McpAction::Retry),
            "Connecting …",
            "text.muted",
            false,
        ),
    ];
    state.apply_mcp_snapshot(McpSnapshot {
        binding: McpBinding {
            location: "/mcp-style".into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: cases
            .iter()
            .enumerate()
            .map(
                |(index, &(status, pending_action, _, _, _))| McpServerSnapshot {
                    id: index.to_string(),
                    name: format!("fixture-{index}"),
                    configured_enabled: true,
                    status,
                    pending_action,
                    tools: 0,
                    diagnostic: None,
                    actions: Vec::new(),
                },
            )
            .collect(),
    });
    state.panel = TuiPanel::Mcps;
    let theme = crate::theme::Theme::dark();
    let color = |path: &str| {
        theme
            .color(&format!("@dialog.{path}"))
            .or_else(|| theme.color(path))
            .unwrap()
    };
    let options = state.modal_options();
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(options[index].footer, case.2);
    }
    for selected in [0, 1] {
        state.select.cursor = selected;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
        let buffer = terminal.backend().buffer();
        for (index, &(_, _, label, tone, bold)) in cases.iter().enumerate() {
            let y = 15 + index as u16;
            let width = Line::raw(label).width() as u16;
            let x = 86 - width;
            assert_eq!(
                (x..86).map(|x| buffer[(x, y)].symbol()).collect::<String>(),
                label
            );
            let active = index == selected;
            for x in x..86 {
                let cell = &buffer[(x, y)];
                assert_eq!(
                    cell.fg,
                    color(if active {
                        "text.action.primary.$focused"
                    } else {
                        tone
                    }),
                    "{label} foreground"
                );
                assert_eq!(
                    cell.bg,
                    color(if active {
                        "background.action.primary.$focused"
                    } else {
                        "background.base"
                    }),
                    "{label} background"
                );
                assert_eq!(
                    cell.modifier.contains(Modifier::BOLD),
                    bold,
                    "{label} intrinsic bold"
                );
            }
            assert_eq!(
                buffer[(34, y)].modifier.contains(Modifier::BOLD),
                active,
                "title bold remains selection-owned"
            );
        }
    }
    // Safe details and unrelated Select consumers must not inherit MCP tones.
    state.select.cursor = 0;
    state.mcp_enter();
    assert!(
        state.mcp_detail.is_none(),
        "healthy Enter is not a detail/action"
    );
    state.panel = TuiPanel::Model;
    assert!(state.mcp_list_servers().is_none());
    assert_eq!(state.input(), "preserved Ω界 draft");
}
