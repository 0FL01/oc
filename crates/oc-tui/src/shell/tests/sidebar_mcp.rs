use super::*;
use oc_core::queries::{McpAction, McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};

fn inventory(rows: &[(&str, McpStatus)], revision: u64) -> McpSnapshot {
    McpSnapshot {
        binding: McpBinding {
            location: "/sidebar-mcp".into(),
            generation: 1,
            instance: 1,
        },
        revision,
        servers: rows
            .iter()
            .enumerate()
            .map(|(index, &(name, status))| McpServerSnapshot {
                id: format!("opaque-{index}"),
                name: name.into(),
                configured_enabled: status != McpStatus::Disabled,
                status,
                pending_action: (status == McpStatus::Connected).then_some(McpAction::Disconnect),
                tools: 0,
                diagnostic: None,
                actions: Vec::new(),
            })
            .collect(),
    }
}

#[tokio::test]
async fn sidebar_context_shows_only_measured_rows_and_keeps_mcp_hits_aligned() {
    use oc_core::queries::HistoryTurn;

    let mut state = golden_state().await;
    state.chrome.location = Some("/sidebar-mcp".into());
    let servers = [
        ("chrome-devtools", McpStatus::Disabled),
        ("codex_web", McpStatus::Connected),
        ("crw", McpStatus::Disabled),
    ];
    state.apply_mcp_snapshot(inventory(&servers, 1));
    let area = Rect::new(0, 0, 160, 48);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    for (usage, expected_context) in [
        (None, ""),
        (Some((0, 0, 500_000)), ""),
        (Some((8000, 380, 0)), "Context\n8,380 tokens\n"),
        (
            Some((8000, 380, 500_000)),
            "Context\n8,380 tokens\n2% used\n",
        ),
        (None, ""),
    ] {
        let mut answer = msg(2, Role::Assistant, "answer");
        answer.turn = usage.map(|(input, output, limit)| HistoryTurn {
            context_usage: Some((input, output)),
            context_usage_limit: Some(limit),
            ..Default::default()
        });
        state.attach_page(&page(vec![answer]));
        terminal.draw(|frame| render(frame, &state)).unwrap();
        let layout = sidebar_layout(&state, area).unwrap();
        let buffer = terminal.backend().buffer();
        let row = |y| {
            (layout.body.x..layout.body.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        };
        let header = layout.header.unwrap();
        let context_height = expected_context.lines().count();
        let mcp_start = context_height + usize::from(context_height > 0);
        assert_eq!(header.y, layout.body.y + mcp_start as u16);
        assert_eq!(
            (layout.body.y..header.y)
                .map(row)
                .collect::<Vec<_>>()
                .join("\n"),
            expected_context
        );
        assert_eq!(row(header.y), "▼ MCP");
        assert_eq!(layout.content_height, mcp_start + 1 + servers.len());
        assert_eq!(layout.rows.len(), servers.len());
        for (rect, index) in &layout.rows {
            assert_eq!(rect.y, header.y + 1 + *index as u16);
            assert!(row(rect.y).contains(servers[*index].0));
        }
        assert!(
            (layout.body.y + layout.content_height as u16..layout.body.bottom())
                .all(|y| row(y).is_empty())
        );
    }
}

#[tokio::test]
async fn sidebar_mcp_uses_published_status_order_and_semantic_styles() {
    let mut state = golden_state().await;
    state.chrome.location = Some("/sidebar-mcp".into());
    let rows = [
        ("z-connected", McpStatus::Connected),
        ("b-pending", McpStatus::Pending),
        ("a-disabled", McpStatus::Disabled),
        ("y-failed", McpStatus::Failed),
        ("c-auth", McpStatus::NeedsAuth),
    ];
    state.apply_mcp_snapshot(inventory(&rows, 1));
    let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let buffer = terminal.backend().buffer();
    let text: Vec<String> = (0..48)
        .map(|y| (0..160).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let header = text
        .iter()
        .position(|line| line.contains("▼ MCP"))
        .expect("MCP sidebar heading");
    assert_eq!(
        buffer[(121, header as u16)].fg,
        buffer[(119, header as u16)].fg,
        "the disclosure separator retains the blank canvas foreground"
    );
    let theme = Theme::dark();
    for (index, ((name, _), (label, dot))) in rows
        .iter()
        .zip([
            ("Connected", theme.success()),
            ("Connecting", theme.text_muted()),
            ("Disabled", theme.text_muted()),
            ("Error", theme.error()),
            ("Sign in", theme.warning()),
        ])
        .enumerate()
    {
        let y = (header + index + 1) as u16;
        assert!(text[y as usize].contains(name));
        assert!(text[y as usize].trim_end().ends_with(label));
        assert_eq!(buffer[(120, y)].symbol(), "•");
        assert_eq!(buffer[(120, y)].fg, dot);
        assert_eq!(buffer[(122, y)].fg, theme.text());
        assert!(buffer[(122, y)].modifier.contains(Modifier::BOLD));
        let status_x = 157 - label.len() as u16;
        assert_eq!(
            buffer[(status_x, y)].fg,
            if label == "Error" {
                theme.error()
            } else {
                theme.text_muted()
            }
        );
        assert!(!buffer[(status_x, y)].modifier.contains(Modifier::BOLD));
    }
}

#[tokio::test]
async fn sidebar_mcp_fold_threshold_summary_and_empty_inventory_follow_the_source() {
    let mut state = golden_state().await;
    state.chrome.location = Some("/sidebar-mcp".into());
    let rows = [
        ("chrome-devtools", McpStatus::Disabled),
        ("codex_web", McpStatus::Connected),
        ("crw", McpStatus::Disabled),
    ];
    state.apply_mcp_snapshot(inventory(&rows, 1));
    let expanded = screen(&state, 160, 48);
    assert!(expanded.iter().any(|line| line.contains("▼ MCP")));
    for (name, _) in rows {
        assert!(expanded.iter().any(|line| line.contains(name)));
    }
    state.apply_sidebar_mcp_open(false);
    let folded = screen(&state, 160, 48);
    assert!(folded.iter().any(|line| line.contains("▶ MCP (1 active)")));
    assert!(!folded.iter().any(|line| line.contains("codex_web")));

    state.apply_mcp_snapshot(inventory(
        &[
            ("auth", McpStatus::NeedsAuth),
            ("failed", McpStatus::Failed),
        ],
        2,
    ));
    let two = screen(&state, 160, 48);
    assert!(
        two.iter()
            .any(|line| line.contains("MCP (0 active, 2 errors)"))
    );
    assert!(
        two.iter()
            .any(|line| line.contains("auth") && line.contains("Sign in"))
    );
    assert!(!two.iter().any(|line| line.contains("▶ MCP")));
    state.apply_mcp_snapshot(inventory(
        &[
            ("auth", McpStatus::NeedsAuth),
            ("disabled", McpStatus::Disabled),
            ("pending", McpStatus::Pending),
        ],
        3,
    ));
    let three = screen(&state, 160, 48);
    assert!(
        three
            .iter()
            .any(|line| line.contains("▶ MCP (0 active, 1 error)"))
    );
    assert!(!three.iter().any(|line| line.contains("Sign in")));
    state.apply_mcp_snapshot(inventory(&[], 4));
    let empty = screen(&state, 160, 48);
    assert!(!empty.iter().any(|line| line.contains("MCP")));
    assert!(!empty.iter().any(|line| line.contains("Context")));
    assert_eq!(
        sidebar_layout(&state, Rect::new(0, 0, 160, 48))
            .unwrap()
            .content_height,
        0
    );
    assert!(empty.iter().any(|line| line.contains("/sidebar-mcp")));
}

#[tokio::test]
async fn sidebar_mcp_unicode_scroll_clamping_and_tiny_heights_share_visible_hits() {
    use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
    let mut state = golden_state().await;
    state.chrome.location = Some("/sidebar-mcp".into());
    let name = "界e\u{301}👩‍💻".repeat(20);
    let rows: Vec<_> = (0..30)
        .map(|_| (name.as_str(), McpStatus::Connected))
        .collect();
    state.apply_mcp_snapshot(inventory(&rows, 1));
    let area = Rect::new(0, 0, 160, 18);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| render(frame, &state)).unwrap();
    let layout = sidebar_layout(&state, area).unwrap();
    assert_eq!(layout.body.width, 37);
    assert!(layout.content_height > layout.body.height as usize);
    for (rect, _) in &layout.rows {
        let buffer = terminal.backend().buffer();
        let status_x = rect.right() - 9;
        assert_eq!(buffer[(status_x, rect.y)].symbol(), "C");
        let text: String = (rect.x..rect.right())
            .map(|x| buffer[(x, rect.y)].symbol())
            .collect();
        assert!(text.ends_with("Connected"));
        assert!(text.contains("e\u{301}"));
        assert!(text.contains("👩‍💻"));
    }
    for _ in 0..30 {
        state.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: layout.body.right(),
                row: layout.body.y,
                modifiers: KeyModifiers::NONE,
            },
            area,
        );
    }
    let bottom = sidebar_layout(&state, area).unwrap();
    assert_eq!(
        bottom.offset,
        bottom.content_height - bottom.body.height as usize
    );
    assert_eq!(bottom.rows.last().unwrap().1, 29);
    state.apply_sidebar_mcp_open(false);
    let short = sidebar_layout(&state, area).unwrap();
    assert_eq!(short.offset, 0);
    assert!(short.header.is_some());
    terminal.draw(|frame| render(frame, &state)).unwrap();
    state.apply_sidebar_mcp_open(true);
    assert_eq!(sidebar_layout(&state, area).unwrap().offset, 0);
    state.apply_sidebar_mcp_open(false);
    for height in 0..8 {
        let frame = Rect::new(0, 0, 160, height);
        let layout = sidebar_layout(&state, frame).unwrap();
        assert!(layout.body.bottom() <= frame.bottom());
        assert!(layout.footer.bottom() <= frame.bottom());
        if layout.body.height == 0 {
            assert!(layout.rows.is_empty());
            assert!(layout.header.is_none());
        }
    }
    state.chrome.sidebar_hidden = true;
    assert!(!state.sidebar_wheel_hit(area, 159, 4));
    assert!(sidebar_layout(&state, area).is_none());
}
