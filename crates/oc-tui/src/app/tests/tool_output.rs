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

#[test]
fn generic_continuation_indents_inherit_canvas_not_muted_labels() {
    let card = crate::history::card_from_row(&operation());
    let crate::tools::ToolRender::Inline(inline) = &card.render else {
        panic!("fixture must use Generic presentation");
    };
    let theme = crate::theme::Theme::dark();
    let padding = " ".repeat(crate::messages::MESSAGE_PADDING + crate::tools::TOOL_ICON_WIDTH);
    for width in [32, 120] {
        let rows = crate::tools::generic_block_expanded(inline, &card, theme, width, true);
        let continuation = rows
            .iter()
            .filter(|row| {
                row.spans().len() >= 2
                    && row.spans()[0].content() == padding
                    && !row.spans()[1].content().is_empty()
                    && row.spans()[1].content().trim().is_empty()
            })
            .collect::<Vec<_>>();
        assert!(!continuation.is_empty(), "hard and soft wrapped details");
        for row in continuation {
            assert_eq!(row.spans()[1].style(), ratatui::style::Style::default());
        }
        for label in ["nested: ", "output: "] {
            let span = rows
                .iter()
                .flat_map(crate::styled::Line::spans)
                .find(|span| span.content() == label)
                .unwrap();
            assert_eq!(span.style().fg, Some(theme.text_muted()));
        }
    }
}

#[tokio::test]
async fn in_flight_checkpoint_does_not_publish_unsettled_step_usage() {
    let mut state = fresh_state("settled-usage").await;
    let turn = WorkerTurnId("usage-turn".into());
    state.active_turn = Some(turn.clone());
    state.apply_tool_started(&turn, "op", "execute", r#"{"code":"return 1;"}"#);
    let mut projection = HistoryTurn {
        id: turn.0.clone(),
        status: "started".into(),
        revision: 1,
        context_usage: Some((1234, 64)),
        context_usage_limit: Some(65536),
        ..Default::default()
    };
    state.apply_presentation(&turn, &projection);
    assert_eq!(
        state.context_usage(),
        None,
        "checkpoint precedes tool settlement"
    );

    let mut current = msg(2, Role::Assistant, "");
    current.turn = Some(projection.clone());
    state.attach_page(&page(vec![current.clone()], 1, false, false));
    assert_eq!(
        state.context_usage(),
        None,
        "retaining an in-flight checkpoint does not settle its usage"
    );
    let mut previous = msg(1, Role::Assistant, "previous step");
    previous.turn = Some(HistoryTurn {
        id: "previous-turn".into(),
        status: "completed".into(),
        context_usage: Some((500, 10)),
        context_usage_limit: Some(65536),
        ..Default::default()
    });
    state.attach_page(&page(vec![previous, current], 2, false, false));
    assert_eq!(
        state.context_usage(),
        Some((510, Some(65536))),
        "history fallback retains the last settled measurement"
    );

    state.apply_context_usage(&turn, 1234, 64, 65536);
    projection.revision = 2;
    projection.context_usage = Some((2200, 80));
    state.apply_presentation(&turn, &projection);
    assert_eq!(
        state.context_usage(),
        Some((1298, Some(65536))),
        "keep the previous settled step"
    );

    projection.revision = 3;
    projection.status = "completed".into();
    state.apply_presentation(&turn, &projection);
    assert_eq!(
        state.context_usage(),
        Some((2280, Some(65536))),
        "terminal projection recovers measured usage"
    );
}

#[tokio::test]
async fn execute_progress_replaces_only_the_current_running_operation() {
    let mut state = fresh_state("execute-progress").await;
    let turn = WorkerTurnId("execute-turn".into());
    state.active_turn = Some(turn.clone());
    state.input = "preserved draft".into();
    state.editor.cursor = 4;
    let mut facts = Presentation::new("", 0, false);
    facts.label_only = true;
    facts.execute = Some(oc_core::tool_output::Execute {
        code: Some("return 1;".into()),
        ..Default::default()
    });
    state.apply_tool_progress(&turn, "op", Box::new(facts.clone()));
    assert!(state.transcript_rows().iter().all(|row| row.tool.is_none()));
    state.apply_tool_started(&turn, "op", "execute", r#"{"code":"return 1;"}"#);
    state.apply_tool_progress(&turn, "op", Box::new(facts.clone()));
    let current = || {
        state
            .transcript_rows()
            .into_iter()
            .find_map(|row| row.tool)
            .unwrap()
    };
    assert_eq!(current().output_presentation.as_deref(), Some(&facts));
    let mut terminal = facts.clone();
    terminal.label_only = false;
    terminal.body = "1".into();
    terminal.body_bytes = 1;
    terminal.execute.as_mut().unwrap().elapsed_ms = Some(5);
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "op",
        "execute",
        "completed",
        "1",
        1,
        false,
        None,
        None,
        None,
        Some(Box::new(terminal.clone())),
    );
    state.apply_tool_progress(&turn, "op", Box::new(facts));
    let rows = state.transcript_rows();
    let cards = rows
        .iter()
        .filter_map(|row| row.tool.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].state, "completed");
    assert_eq!(cards[0].output_presentation.as_deref(), Some(&terminal));
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"preserved draft".to_owned(), 4)
    );
}

#[tokio::test]
async fn generic_expansion_keeps_sticky_follow_or_the_detached_top_row() {
    for (scroll, newer) in [(0, false), (3, false), (0, true)] {
        let mut state = fresh_state("expansion-follow").await;
        let original = operation();
        let mut answer = msg(2, Role::Assistant, "");
        answer.turn = Some(HistoryTurn {
            id: "expansion-turn".into(),
            status: "completed".into(),
            parts: vec![
                TranscriptPart::Tool(original.clone()),
                TranscriptPart::Text("LATEST RESULT".into()),
            ],
            ..Default::default()
        });
        let prefix = (0..30)
            .map(|i| format!("PREFIX-{i}\n\n"))
            .collect::<String>();
        state.attach_page(&page(
            vec![msg(1, Role::Assistant, &prefix), answer],
            if newer { 100 } else { 2 },
            false,
            newer,
        ));
        state.input = "kept draft".into();
        state.editor.cursor = 4;
        state.scroll = scroll;
        let frame = Rect::new(0, 0, 120, 20);
        let area = crate::shell::transcript_area(&state, frame);
        let (before, total, shown) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        state.observe_transcript_viewport(area.width, frame.width, area.height, total, shown);
        assert_eq!(shown, scroll);

        state.toggle_tool_expansion_at(&original.op, frame);
        let (after, expanded_total, shown) =
            state.visible_transcript_at_viewport(area.width, frame.width, area.height);
        assert!(expanded_total > total);
        if scroll == 0 && !newer {
            assert_eq!(shown, 0, "expanding a tool must not detach sticky follow");
            assert!(
                after
                    .iter()
                    .any(|line| line.plain_text().contains("LATEST RESULT"))
            );
        } else {
            assert_eq!(
                after.first(),
                before.first(),
                "detached reader keeps its top row"
            );
            assert!(shown > 0);
        }
        assert_eq!(
            (&state.input, state.editor.cursor),
            (&"kept draft".to_owned(), 4)
        );
    }
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
    assert!(
        state
            .live_parts
            .iter()
            .any(|part| matches!(part, LivePart::Tool { card, .. } if card.preview_limited()))
    );
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
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), x, y), frame);
    assert!(state.exploration_down.is_some());
    let taller = Rect::new(0, 0, frame.width, frame.height + 1);
    state.resize_mouse_position(taller);
    assert!(state.exploration_down.is_none());
    let (_, next_x, next_y) = paint(&mut state, taller);
    assert_eq!((next_x, next_y), (x, y));
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), x, y), taller);
    assert!(
        state.exploration_expanded.is_empty(),
        "resize cannot authorize a stale tool press"
    );
    state.resize_mouse_position(frame);
    paint(&mut state, frame);
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
        assert!(card.preview_limited());
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
        state.window.rows().is_empty(),
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
            state.window.rows().iter().all(|row| {
                row.meta.as_ref().is_none_or(|meta| !meta.preview_limited)
                    && row.tool.as_ref().is_none_or(|card| !card.preview_limited())
            }),
            "complete body must not inherit RAW guidance loss through {role:?} anchor"
        );
    }
}
