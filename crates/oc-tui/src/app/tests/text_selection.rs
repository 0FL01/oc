use super::super::text_selection::TextSource;
use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend};

fn mouse(kind: MouseEventKind, point: (u16, u16)) -> MouseEvent {
    MouseEvent {
        kind,
        column: point.0,
        row: point.1,
        modifiers: KeyModifiers::NONE,
    }
}

fn paint(state: &TuiState, area: Rect) {
    Terminal::new(TestBackend::new(area.width, area.height))
        .unwrap()
        .draw(|frame| crate::shell::render(frame, state))
        .unwrap();
}

fn prompt_cell(state: &TuiState, offset: usize) -> (u16, u16) {
    let painted = state.painted_prompt.borrow();
    let p = painted.as_ref().unwrap();
    p.rows
        .iter()
        .enumerate()
        .find_map(|(row, stops)| {
            stops
                .iter()
                .find(|(_, raw)| *raw == offset)
                .map(|(cell, _)| (p.input.x + *cell as u16, p.input.y + row as u16))
        })
        .unwrap()
}

fn drag(state: &mut TuiState, area: Rect, from: (u16, u16), to: (u16, u16)) {
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), from), area)
            .intent
            .is_none()
    );
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), to), area)
            .intent
            .is_none()
    );
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), to), area)
            .intent
            .is_none()
    );
}

fn chrome_state() -> TuiState {
    use oc_core::queries::{McpBinding, McpServerSnapshot, McpSnapshot, McpStatus};
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("copy-chrome"));
    state.chrome.location =
        Some("/private-hidden-source-component/another-very-long-directory/project".into());
    state.session_title = Some("a quite long sidebar title text".into());
    state.set_service_feedback_visible(false);
    state.apply_mcp_snapshot(McpSnapshot {
        binding: McpBinding {
            location: state.chrome.location.clone().unwrap(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: (0..3)
            .map(|n| McpServerSnapshot {
                id: format!("opaque-{n}"),
                name: format!("copy-disabled-{n}"),
                configured_enabled: false,
                status: McpStatus::Disabled,
                pending_action: None,
                tools: 0,
                actions: Vec::new(),
                diagnostic: None,
            })
            .collect(),
    });
    state
}

fn text_cells(
    state: &TuiState,
    area: Rect,
    source: TextSource,
    needle: &str,
) -> ((u16, u16), (u16, u16)) {
    use unicode_width::UnicodeWidthStr;
    crate::shell::selectable_rows(state, area, source)
        .unwrap()
        .iter()
        .find_map(|(rect, line)| {
            let text = line.plain_text();
            text.find(needle).map(|at| {
                let x = rect.x + UnicodeWidthStr::width(&text[..at]) as u16;
                (
                    (x, rect.y),
                    (x + UnicodeWidthStr::width(needle) as u16, rect.y),
                )
            })
        })
        .unwrap()
}

#[tokio::test]
async fn vis27_text_selection_prompt_raw_ranges_and_editing_on_both_routes() {
    let area = Rect::new(0, 0, 120, 40);
    for home in [true, false] {
        for reverse in [false, true] {
            let (app, mut inbox, _) = CoreApp::channel(4);
            let mut state = if home {
                TuiState::new_home(app)
            } else {
                TuiState::new(app, sid("copy-prompt"))
            };
            let text = format!("α界e\u{301}👩\u{200d}💻 {}tail\nsecond", "word ".repeat(15));
            state.restore_prompt(text.clone());
            paint(&state, area);
            let a = prompt_cell(&state, 0);
            let b = prompt_cell(&state, text.len());
            drag(
                &mut state,
                area,
                if reverse { b } else { a },
                if reverse { a } else { b },
            );
            assert_eq!(
                state.take_copy_request(),
                Some(text.clone()),
                "soft wrap must not introduce LF"
            );
            assert_eq!(state.input(), text);
            assert_eq!(state.editor.selected(), Some((0, text.len())));
            state.handle_key(KeyAction::Char('X')).await;
            assert_eq!(state.input(), "X");
            state.handle_key(KeyAction::Undo).await;
            assert_eq!(state.input(), text);
            state.handle_paste("replacement Ω");
            assert_eq!(
                state.input(),
                "replacement Ω",
                "paste replaces the same raw selection"
            );
            state.handle_key(KeyAction::Undo).await;
            assert_eq!(state.input(), text);
            assert!(inbox.try_recv().is_err());
        }
    }
}

#[test]
fn vis27_text_selection_prompt_capture_survives_repaint_and_clamps_visible_rows() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let area = Rect::new(0, 0, 120, 40);
    let text = (0..30)
        .map(|n| format!("line{n:02} 界"))
        .collect::<Vec<_>>()
        .join("\n");
    state.restore_prompt(text.clone());
    paint(&state, area);
    let p = state.painted_prompt.borrow().clone().unwrap();
    let a = p.rows[0][0].1;
    let b = p.rows.last().unwrap().last().unwrap().1;
    assert!(a > 0);
    let from = prompt_cell(&state, a);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), from), area);
    paint(&state, area);
    assert_eq!(state.painted_prompt.borrow().as_ref().unwrap().top, p.top);
    let to = (p.input.right() + 10, p.input.bottom() + 5);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), to), area);
    paint(&state, area);
    assert_eq!(state.painted_prompt.borrow().as_ref().unwrap().top, p.top);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Right), to), area);
    assert_eq!(
        state.take_copy_request(),
        Some(text[a..b].into()),
        "owned release button is immaterial"
    );
}

#[test]
fn vis27_text_selection_chip_endpoint_and_raw_byte_limit() {
    for size in [2000, MAX_SELECTION_BYTES - 4, MAX_SELECTION_BYTES + 1] {
        let (app, _, _) = CoreApp::channel(4);
        let mut state = TuiState::new_home(app);
        let area = Rect::new(0, 0, 12, 40);
        state.handle_paste("pre ");
        state.handle_paste(&"z".repeat(size));
        paint(&state, area);
        let p = state.painted_prompt.borrow().clone().unwrap();
        assert!(
            p.rows.iter().any(Vec::is_empty),
            "wrapped chip has no interior caret stops"
        );
        let from = prompt_cell(&state, 0);
        let chip = p.chips[0].0;
        drag(&mut state, area, from, (chip.x, chip.y));
        if size + 4 > MAX_SELECTION_BYTES {
            assert!(state.take_copy_request().is_none());
            assert_eq!(state.note(), Some("Selection exceeds clipboard size limit"));
        } else {
            assert_eq!(
                state.take_copy_request(),
                Some(format!("pre {}", "z".repeat(size)))
            );
        }
        assert_eq!(
            state.editor.chip_count(),
            1,
            "drag does not expand the chip"
        );
    }
}

#[test]
fn vis27_text_selection_readonly_payload_ownership_and_mcp_click_arbitration() {
    let mut state = chrome_state();
    let area = Rect::new(0, 0, 160, 40);
    state.restore_prompt("preserve draft".into());
    state.editor.move_to(0, true);
    let editor = (state.editor.cursor, state.editor.anchor);
    state.set_clipboard_mode(ClipboardMode::Manual);
    paint(&state, area);
    for (source, needle) in [
        (TextSource::Sidebar, "Context"),
        (TextSource::Sidebar, "copy-disabled-1"),
        (TextSource::Sidebar, "Disabled"),
        (TextSource::Location, "project"),
    ] {
        let (a, b) = text_cells(&state, area, source, needle);
        drag(&mut state, area, a, b);
        assert!(state.take_copy_request().is_none());
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), b), area);
        assert_eq!(state.take_copy_request(), Some(needle.into()));
        assert_eq!((state.editor.cursor, state.editor.anchor), editor);
        assert_eq!(state.input(), "preserve draft");
    }
    let layout = crate::shell::sidebar_layout(&state, area).unwrap();
    let header = layout.header.unwrap();
    let point = (header.x + 2, header.y);
    let down = state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), point), area);
    assert!(down.intent.is_none());
    let up = state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), point), area);
    assert_eq!(
        up.intent,
        Some(PanelIntent::SetSidebarMcpOpen { open: false })
    );
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), point), area);
    state.handle_mouse(
        mouse(
            MouseEventKind::Drag(MouseButton::Left),
            (point.0 + 2, point.1),
        ),
        area,
    );
    assert!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), point), area)
            .intent
            .is_none()
    );
}

#[test]
fn vis27_text_selection_visible_sidebar_projection_and_source_changes() {
    let area = Rect::new(0, 0, 160, 40);
    let mut state = chrome_state();
    paint(&state, area);
    let rows = crate::shell::selectable_rows(&state, area, TextSource::Sidebar).unwrap();
    let from = (rows[0].0.x, rows[0].0.y);
    let (last, line) = rows.last().unwrap();
    let to = (last.right(), last.y);
    let expected = rows
        .iter()
        .map(|(_, line)| line.plain_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !line
            .plain_text()
            .contains("private-hidden-source-component")
    );
    drag(&mut state, area, from, to);
    assert_eq!(state.take_copy_request(), Some(expected));
    for repaint in [false, true] {
        paint(&state, area);
        let (a, b) = text_cells(&state, area, TextSource::Sidebar, "Context");
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
        state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
        state.session_title = Some(format!("changed {repaint}"));
        if repaint {
            paint(&state, area);
        }
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), b), area);
        assert!(state.take_copy_request().is_none());
    }
    paint(&state, area);
    let title = state.session_title.clone();
    let (a, b) = text_cells(&state, area, TextSource::Sidebar, "Context");
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
    state.session_title = Some("intervening title".into());
    paint(&state, area);
    state.session_title = title;
    paint(&state, area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), b), area);
    assert!(
        state.take_copy_request().is_none(),
        "reverting painted content cannot revive an invalidated gesture"
    );
}

#[test]
fn vis27_text_selection_stable_chrome_and_pending_payload_survive_history_refresh() {
    let mut state = chrome_state();
    let area = Rect::new(0, 0, 160, 40);
    paint(&state, area);
    let (a, b) = text_cells(&state, area, TextSource::Sidebar, "Context");
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
    let mut history = page(
        vec![msg(1, Role::Assistant, "unrelated live history")],
        1,
        false,
        false,
    );
    history.title = state.session_title.clone();
    state.attach_page(&history);
    paint(&state, area);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), b), area);
    state.attach_page(&history);
    assert_eq!(
        state.take_copy_request(),
        Some("Context".into()),
        "transcript-only invalidation must not erase another source's request"
    );
}

#[test]
fn vis27_text_selection_cancelled_release_cannot_open_home_mcp() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let chrome = chrome_state();
    state.chrome.location = chrome.chrome.location.clone();
    state.set_service_feedback_visible(false);
    state.apply_mcp_snapshot(chrome.mcp_snapshot.unwrap());
    state.restore_prompt("copy draft".into());
    let area = Rect::new(0, 0, 120, 40);
    paint(&state, area);
    let a = prompt_cell(&state, 0);
    let b = prompt_cell(&state, 4);
    let mcp = crate::shell::home_mcp_rect(&state, area).unwrap();
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
    state.resize_mouse_position(area);
    let outcome = state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), (mcp.x, mcp.y)),
        area,
    );
    assert!(outcome.intent.is_none());
    assert!(state.take_copy_request().is_none());
    assert_eq!(state.panel(), &TuiPanel::None);
    paint(&state, area);
    state.handle_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), (mcp.x, mcp.y)),
        area,
    );
    let click = state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), (mcp.x, mcp.y)),
        area,
    );
    assert_eq!(
        click.intent,
        Some(PanelIntent::LoadMcps),
        "fresh click keeps the original action"
    );
}

#[test]
fn vis27_text_selection_transcript_drag_owns_release_over_sidebar() {
    let mut state = chrome_state();
    let mut history = page(
        vec![msg(1, Role::Assistant, "select cobalt")],
        1,
        false,
        false,
    );
    history.title = state.session_title.clone();
    state.attach_page(&history);
    let area = Rect::new(0, 0, 160, 40);
    paint(&state, area);
    let transcript = state.painted_transcript.borrow().clone().unwrap();
    let (row, at) = transcript
        .rows
        .iter()
        .enumerate()
        .find_map(|(row, line)| line.plain_text().find("select").map(|at| (row, at)))
        .unwrap();
    let a = (
        transcript.area.x + at as u16,
        transcript.area.y + row as u16,
    );
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
    state.handle_mouse(
        mouse(MouseEventKind::Drag(MouseButton::Left), (a.0 + 6, a.1)),
        area,
    );
    let header = crate::shell::sidebar_layout(&state, area)
        .unwrap()
        .header
        .unwrap();
    let up = state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), (header.x, header.y)),
        area,
    );
    assert!(up.intent.is_none());
    assert_eq!(state.take_copy_request(), Some("select".into()));
    assert!(state.sidebar_mcp_open());
}

#[test]
fn vis27_text_selection_hidden_surface_cannot_revive_after_repaint() {
    let area = Rect::new(0, 0, 160, 40);
    for sidebar in [false, true] {
        let mut state = chrome_state();
        state.restore_prompt("copy draft".into());
        paint(&state, area);
        let (a, b) = if sidebar {
            text_cells(&state, area, TextSource::Sidebar, "Context")
        } else {
            (prompt_cell(&state, 0), prompt_cell(&state, 4))
        };
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
        state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
        // Asynchronous forms can mount/unmount without an input event. Painting
        // a covering owner must retire the capture even if the text returns.
        state.panel = TuiPanel::Settings;
        paint(&state, area);
        state.panel = TuiPanel::None;
        paint(&state, area);
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), b), area);
        assert!(
            state.take_copy_request().is_none(),
            "hidden source revived; sidebar={sidebar}"
        );
        assert_eq!(state.input(), "copy draft");
    }
}

#[test]
fn vis27_text_selection_toast_checks_the_whole_range_not_just_endpoints() {
    let mut state = chrome_state();
    let area = Rect::new(0, 0, 160, 40);
    paint(&state, area);
    let rows = crate::shell::selectable_rows(&state, area, TextSource::Sidebar).unwrap();
    let from = (rows[0].0.x, rows[0].0.y);
    let last = rows.last().unwrap().0;
    let to = (last.x + 1, last.y);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), from), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), to), area);
    state.push_note("hi");
    let toast = crate::shell::toast_rect(&state, area).unwrap();
    assert!(!toast.contains(from.into()) && !toast.contains(to.into()));
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), to), area);
    assert!(state.take_copy_request().is_none());
    let (a, b) = text_cells(&state, area, TextSource::Sidebar, "Context");
    drag(&mut state, area, a, b);
    assert_eq!(
        state.take_copy_request(),
        Some("Context".into()),
        "uncovered rows remain selectable"
    );
}

#[tokio::test]
async fn vis27_text_selection_cancelled_release_and_mode_changes_do_not_leak() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let area = Rect::new(0, 0, 120, 40);
    for mode in [ClipboardMode::Manual, ClipboardMode::Disabled] {
        state.restore_prompt("copy draft".into());
        state.set_clipboard_mode(ClipboardMode::Select);
        paint(&state, area);
        let a = prompt_cell(&state, 0);
        let b = prompt_cell(&state, 4);
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
        state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
        state.set_clipboard_mode(mode);
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), (0, 0)), area);
        assert!(state.take_copy_request().is_none());
        paint(&state, area);
        drag(&mut state, area, a, b);
        assert!(state.take_copy_request().is_none());
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Right), b), area);
        assert_eq!(
            state.take_copy_request(),
            (mode == ClipboardMode::Manual).then(|| "copy".into())
        );
    }
    state.set_clipboard_mode(ClipboardMode::Select);
    state.restore_prompt("copy draft".into());
    paint(&state, area);
    let a = prompt_cell(&state, 0);
    let b = prompt_cell(&state, 4);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), a), area);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), b), area);
    state.handle_key(KeyAction::Interrupt).await;
    assert!(
        state.input().is_empty(),
        "Ctrl+C retains existing clear semantics"
    );
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), b), area);
    assert!(state.take_copy_request().is_none());
    assert_eq!(state.panel(), &TuiPanel::None);
    assert!(inbox.try_recv().is_err());
}
