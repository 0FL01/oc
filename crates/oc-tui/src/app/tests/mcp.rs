use super::*;
use oc_core::queries::{McpAction, McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};
use ratatui::{Terminal, backend::TestBackend, style::Modifier, text::Line};

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
    state.mcp_detail = Some("0".into());
    assert!(state.mcp_list_servers().is_none());
    state.mcp_detail = None;
    state.panel = TuiPanel::Model;
    assert!(state.mcp_list_servers().is_none());
    assert_eq!(state.input(), "preserved Ω界 draft");
}
