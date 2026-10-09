//! Captured tool facts through the existing live/page and mouse owners.
use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use oc_core::{
    queries::{HistoryTurn, PartState, ToolOpView, TranscriptPart},
    tool_output::{Capture, CaptureState, Presentation},
};

fn operation() -> ToolOpView {
    let body = "actual [Part preview truncated]\n[output preview truncated; full result retained]\n[truncated]";
    let mut facts = Presentation::new(body, 70_000, true);
    facts.capture = Some(Capture {
        reference: None,
        state: CaptureState::Interrupted,
        admitted_bytes: 70_000,
        retained_bytes: 22,
        admitted_lines: 2_000,
        retained_lines: 2,
    });
    ToolOpView {
        child_job: None,
        rowid: 1,
        op: "preview-operation".into(),
        name: "mcp_fixture".into(),
        state: "completed".into(),
        input: Some(r#"{"query":"native needle","nested":{"value":"available parameter"}}"#.into()),
        output: Some(format!("{body}\n[tool output: model-only guidance]")),
        output_bytes: 70_050,
        output_truncated: true,
        output_presentation: Some(Box::new(facts)),
        question: None,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    }
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

fn paint(state: &mut TuiState, frame: Rect) -> (Vec<crate::styled::Line>, u16, u16) {
    let area = crate::shell::transcript_area(state, frame);
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
    let painted = state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]);
    let header = rows
        .iter()
        .position(|line| line.plain_text().contains("mcp_fixture"))
        .unwrap();
    (painted, area.x + 6, area.y + header as u16)
}

#[tokio::test]
async fn generic_live_mouse_and_replayed_projection_keep_body_caret_and_capture_truth() {
    let mut state = fresh_state("preview-owner").await;
    let mut guidance_only = operation();
    let body = "complete body";
    guidance_only.output_presentation =
        Some(Box::new(Presentation::new(body, body.len() as u64, true)));
    let detail =
        super::super::transcript::card_row(&crate::history::card_from_row(&guidance_only)).text;
    assert!(detail.contains(body));
    assert!(
        !detail.contains("bytes stored") && !detail.contains("Body preview:"),
        "clipped RAW guidance does not imply clipped presented body"
    );
    state.input = "unchanged draft".into();
    state.editor.cursor = 5;
    let original = operation();
    let turn = WorkerTurnId("preview-turn".into());
    state.active_turn = Some(turn.clone());
    state.apply_tool_started(
        &turn,
        &original.op,
        &original.name,
        original.input.as_deref().unwrap(),
    );
    state.apply_tool_finished_with_output_presentation(
        &turn,
        &original.op,
        &original.name,
        &original.state,
        original.output.as_deref().unwrap(),
        original.output_bytes,
        original.output_truncated,
        None,
        None,
        None,
        original.output_presentation.clone(),
    );
    assert!(state.preview_limited());
    let frame = Rect::new(0, 0, 120, 48);
    let (collapsed, x, y) = paint(&mut state, frame);
    let text = collapsed
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Capture incomplete · /cards"));
    assert!(!text.contains("output:") && !text.contains("[Part preview truncated]"));
    state.handle_mouse(mouse(MouseEventKind::Moved, x, y), frame);
    let (hover, _, _) = paint(&mut state, frame);
    assert_ne!(
        hover, collapsed,
        "actual header hover has source foreground feedback"
    );
    assert!(
        state.exploration_expanded.is_empty(),
        "hover is not expansion"
    );
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"unchanged draft".to_owned(), 5)
    );
    for expanded in [true, false, true] {
        state.click = None;
        state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), frame);
        state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), frame);
        assert_eq!(state.exploration_expanded.contains(&original.op), expanded);
        let (visible, next_x, next_y) = paint(&mut state, frame);
        assert_eq!((next_x, next_y), (x, y), "clicked header remains anchored");
        let text = visible
            .iter()
            .map(crate::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            text.contains("[Part preview truncated]"),
            expanded,
            "literal payload only in available output"
        );
        assert_eq!(text.contains("available parameter"), expanded);
        assert!(!text.contains("model-only guidance"));
    }
    state.last_mouse = None;
    let (expanded, _, _) = paint(&mut state, frame);
    let area = crate::shell::transcript_area(&state, frame);
    let output = expanded
        .iter()
        .position(|line| line.plain_text().contains("output:"))
        .unwrap();
    let output_y = area.y + output as u16;
    state.handle_mouse(mouse(MouseEventKind::Moved, x, output_y), frame);
    let (unhovered, _, _) = paint(&mut state, frame);
    assert_eq!(
        unhovered, expanded,
        "parameter/body text is not a hover action surface"
    );
    assert!(state.exploration_hit(frame, x, output_y).is_none());
    state.click = None;
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), frame);
    state.handle_mouse(
        mouse(MouseEventKind::Drag(MouseButton::Left), x + 3, y),
        frame,
    );
    state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), x + 3, y),
        frame,
    );
    assert!(
        state.exploration_expanded.contains(&original.op),
        "selection owns release"
    );
    state.panel = TuiPanel::Help(None);
    state.click = None;
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), frame);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), frame);
    assert!(
        state.exploration_expanded.contains(&original.op),
        "modal owns input"
    );
    state.close_panel();

    let mut message = msg(1, Role::Assistant, "");
    message.turn = Some(HistoryTurn {
        id: turn.0.clone(),
        status: "completed".into(),
        model_label: "recorded model".into(),
        parts: vec![TranscriptPart::Tool(original.clone())],
        part_states: vec![PartState {
            truncated: true,
            ..Default::default()
        }],
        ..Default::default()
    });
    state.apply_finished(&turn, "", 0);
    let replay = page(vec![message], 1, false, false);
    for width in [120, 80, 120] {
        state.attach_page(&replay);
        assert!(
            state.exploration_expanded.is_empty(),
            "page replacement has no hidden stale expansion"
        );
        assert!(state.preview_limited());
        let (rows, _, _) = paint(&mut state, Rect::new(0, 0, width, 48));
        let text = rows
            .iter()
            .map(crate::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !text.contains("[Part preview truncated]") && !text.contains("model-only guidance")
        );
        let card = state
            .window
            .rows()
            .iter()
            .find_map(|row| row.tool.as_ref())
            .unwrap();
        assert!(
            super::super::transcript::card_row(card)
                .text
                .contains("22 / 70000 bytes")
        );
        assert!(
            super::super::transcript::card_row(card)
                .text
                .contains("unavailable")
        );
        assert_eq!(
            card.output_preview,
            original.output_presentation.as_ref().unwrap().body
        );
    }
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"unchanged draft".to_owned(), 5)
    );
    state.attach_page(&page(vec![], 0, false, false));
    assert!(
        !state.preview_limited(),
        "ordinary empty view does not inherit old limitation"
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 48)).unwrap();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    let composer_caret = terminal.backend().cursor_position();
    assert!(terminal.backend().cursor_visible());
    state.panel = TuiPanel::Cards;
    state.apply_cards(vec![crate::history::card_from_row(&original)], false);
    state.apply_card_output(
        original.op.clone(),
        0,
        oc_core::queries::ToolOutputPage {
            text: original.output.clone().unwrap(),
            total_bytes: original.output.as_ref().unwrap().len() as i64,
            next_offset: None,
        },
    );
    let painted = crate::views::render_test(&state, 120, 48).join("\n");
    for fact in [
        "Body preview:",
        "Capture Interrupted: 22 / 70000 bytes",
        "Recorded reference (not a readability guarantee): unavailable",
        "Available output page (bounded; explicit viewer read):",
        "model-only guidance",
    ] {
        assert!(painted.contains(fact), "detail omitted {fact}: {painted}");
    }
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert!(
        !terminal.backend().cursor_visible(),
        "read-only detail has no input owner"
    );
    assert_eq!(
        state.card_output.as_ref().unwrap().page.text,
        original.output.unwrap()
    );
    state.panel = TuiPanel::Model;
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert!(
        terminal.backend().cursor_visible(),
        "search owns its real caret"
    );
    assert_ne!(terminal.backend().cursor_position(), composer_caret);
    state.close_panel();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    assert_eq!(terminal.backend().cursor_position(), composer_caret);
    assert!(terminal.backend().cursor_visible());
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"unchanged draft".to_owned(), 5)
    );
    for role in [Role::User, Role::Assistant] {
        let mut message = msg(1, role, "accepted prompt");
        message.turn = Some(HistoryTurn {
            id: "guidance-only-turn".into(),
            status: if role == Role::User {
                "interrupted"
            } else {
                "completed"
            }
            .into(),
            parts: vec![TranscriptPart::Tool(guidance_only.clone())],
            part_states: vec![PartState {
                truncated: true,
                ..Default::default()
            }],
            ..Default::default()
        });
        state.attach_page(&page(vec![message], 1, false, false));
        assert!(state.window.rows().iter().any(|row| row.tool.is_some()));
        assert!(
            !state.preview_limited(),
            "complete body must not inherit RAW guidance loss through {role:?} anchor"
        );
    }
}
