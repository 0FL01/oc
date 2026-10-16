//! Execute uses the real transcript hit map and live modal/clipboard owners.
use super::*;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use oc_core::tool_output::{
    Execute, ExecuteCall, ExecuteCallStatus, ExecuteOutput, ExecuteOutputKind, Presentation,
};

fn facts() -> Execute {
    Execute {
        code: Some(
            (0..14)
                .map(|index| format!("const value{index} = {:?};", "long source ".repeat(12)))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        calls: vec![ExecuteCall {
            tool: "fixture.echo".into(),
            tool_limited: false,
            status: ExecuteCallStatus::Completed,
            input: Some(serde_json::json!({"query":"needle","nested":{"safe":"expanded input"}})),
            input_limited: false,
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn vis46_error_preview_limits_logical_text_and_empty_inputs_are_not_actions() {
    let (mut state, turn) = live().await;
    let mut facts = facts();
    facts.error = true;
    facts.elapsed_ms = Some(1);
    facts.calls[0].input = Some(serde_json::json!({}));
    facts.output = vec![
        ExecuteOutput {
            kind: ExecuteOutputKind::Text,
            text: format!("first\n{}\nthird\nfourth\nfifth", "x".repeat(200)),
            limited: false,
        },
        ExecuteOutput {
            kind: ExecuteOutputKind::Logs,
            text: "Logs:\nactual log".into(),
            limited: false,
        },
    ];
    let mut presentation = Presentation::new("error", 5, false);
    presentation.execute = Some(facts);
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "execute-ui-op",
        "execute",
        "failed",
        "error",
        5,
        false,
        None,
        None,
        None,
        Some(Box::new(presentation)),
    );
    let card = state.execute_card("execute-ui-op").unwrap();
    let collapsed = crate::tools::execute::block(card, crate::theme::Theme::dark(), 32, &|_| false);
    assert!(collapsed[1].target.is_none());
    assert!(collapsed[2].line.plain_text().starts_with("   ↳ first"));
    assert!(
        collapsed.len() > 6,
        "four logical lines are wrapped after truncation"
    );
    assert!(
        collapsed
            .iter()
            .any(|row| row.line.plain_text().contains('…'))
    );
    assert!(
        !collapsed
            .iter()
            .any(|row| row.line.plain_text().contains("actual log"))
    );
    let expanded = crate::tools::execute::block(card, crate::theme::Theme::dark(), 32, &|target| {
        matches!(target, crate::messages::ToolExpansion::Operation(_))
    });
    assert!(
        expanded
            .iter()
            .any(|row| row.line.plain_text().contains("actual log"))
    );
    assert!(
        expanded
            .iter()
            .any(|row| row.line.plain_text().contains("fifth"))
    );
}

async fn live() -> (TuiState, WorkerTurnId) {
    let mut state = fresh_state("execute-ui").await;
    let turn = WorkerTurnId("execute-ui-turn".into());
    state.active_turn = Some(turn.clone());
    state.input = "exact unsent draft".into();
    state.editor.cursor = 5;
    state.apply_tool_started(
        &turn,
        "execute-ui-op",
        "execute",
        r#"{"code":"raw unchanged"}"#,
    );
    let mut presentation = Presentation::new("", 0, false);
    presentation.label_only = true;
    presentation.execute = Some(facts());
    state.apply_tool_progress(&turn, "execute-ui-op", Box::new(presentation));
    (state, turn)
}

fn paint(state: &mut TuiState, frame: Rect) -> (Vec<crate::styled::Line>, Rect) {
    let area = crate::shell::transcript_area(state, frame);
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(area.width, frame.width, area.height);
    state.observe_transcript_viewport(area.width, frame.width, area.height, total, scroll);
    (
        state.paint_transcript_at(area, &rows, total, scroll, Some(frame), &[]),
        area,
    )
}

fn mouse(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

#[tokio::test]
async fn vis46_execute_rows_own_child_toggles_and_parent_dialog_without_changing_raw() {
    let (mut state, turn) = live().await;
    let mut presentation = Presentation::new("done", 4, false);
    presentation.execute = Some(facts());
    let operation = oc_core::queries::ToolOpView {
        child_job: None,
        rowid: 1,
        op: "execute-ui-op".into(),
        name: "execute".into(),
        state: "completed".into(),
        input: Some(r#"{"code":"raw unchanged"}"#.into()),
        output: Some("done".into()),
        output_bytes: 4,
        output_truncated: false,
        output_presentation: Some(Box::new(presentation)),
        question: None,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    };
    let mut message = msg(2, Role::Assistant, "");
    message.turn = Some(oc_core::queries::HistoryTurn {
        id: turn.0.clone(),
        status: "completed".into(),
        model_label: "Fixture model".into(),
        parts: vec![oc_core::queries::TranscriptPart::Tool(operation)],
        ..Default::default()
    });
    state.apply_finished(&turn, "", 0);
    state.attach_page(&page(
        vec![msg(1, Role::User, "prompt"), message],
        2,
        false,
        false,
    ));
    let frame = Rect::new(0, 0, 120, 40);
    let (rows, area) = paint(&mut state, frame);
    let child = rows
        .iter()
        .position(|line| line.plain_text().contains("fixture.echo"))
        .unwrap();
    assert!(
        !rows
            .iter()
            .any(|line| line.plain_text().contains("expanded input"))
    );
    let x = area.x + crate::messages::MESSAGE_PADDING as u16 + 2;
    let y = area.y + child as u16;
    state.handle_mouse(mouse(MouseEventKind::Moved, x, y), frame);
    paint(&mut state, frame);
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        assert!(
            state
                .handle_mouse(mouse(kind, x, y), frame)
                .intent
                .is_none()
        );
    }
    assert!(state.execute_calls_expanded["execute-ui-op"].contains(&0));
    let (rows, _) = paint(&mut state, frame);
    assert!(
        rows.iter()
            .any(|line| line.plain_text().contains("expanded input"))
    );
    assert_eq!(
        rows[child]
            .spans()
            .iter()
            .find(|span| span.content() == "fixture.echo")
            .unwrap()
            .style()
            .fg,
        Some(crate::theme::Theme::dark().text()),
        "the current expanded header stays hovered on its first painted frame"
    );
    let parent = rows
        .iter()
        .position(|line| line.plain_text().trim_end().ends_with("execute"))
        .unwrap();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(mouse(kind, x, area.y + parent as u16), frame);
    }
    assert_eq!(state.panel, TuiPanel::Execute);
    assert!(
        state
            .execute_card("execute-ui-op")
            .unwrap()
            .input_preview
            .contains("raw unchanged")
    );
    state.execute_key(KeyAction::Cancel);
    assert_eq!(state.panel, TuiPanel::None);
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"exact unsent draft".to_owned(), 5)
    );
}

#[tokio::test]
async fn vis46_execute_modal_follows_final_facts_and_real_copy_feedback_without_input_leak() {
    let (mut state, turn) = live().await;
    let frame = Rect::new(0, 0, 80, 24);
    state.set_detail_area(frame);
    state.panel = TuiPanel::Cards;
    state.activate_execute("execute-ui-op", None, frame);
    let initial = crate::execute_view::layout(&state, frame);
    assert!(initial.max_pan > 8 && initial.rows.len() > initial.body.height as usize);
    assert_eq!(
        initial.rect.y,
        frame.height.saturating_sub(initial.rect.height).div_ceil(2)
    );
    state.execute_key(KeyAction::Right);
    state.execute_key(KeyAction::Down);
    assert_eq!(state.execute_detail.as_ref().unwrap().pan.get(), 8);
    assert_eq!(state.execute_detail.as_ref().unwrap().scroll.get(), 1);
    state.execute_key(KeyAction::Char('c'));
    assert_eq!(
        state.take_copy_request().as_deref(),
        facts().code.as_deref()
    );
    assert!(state.execute_detail.as_ref().unwrap().copied.is_none());
    state.report_clipboard_result(Err("clipboard rejected".into()));
    assert!(state.execute_detail.as_ref().unwrap().copied.is_none());
    state.execute_key(KeyAction::Char('c'));
    state.take_copy_request().unwrap();
    let copy = crate::execute_view::layout(&state, frame)
        .actions
        .into_iter()
        .find(|(key, _)| *key == 'c')
        .unwrap()
        .1;
    state.execute_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), copy.x, copy.y),
        frame,
    );
    state.report_clipboard_result(Ok(()));
    state.execute_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), copy.x, copy.y),
        frame,
    );
    assert!(
        state.take_copy_request().is_none(),
        "changed footer geometry cannot authorize a stale press"
    );
    assert_eq!(state.execute_detail.as_ref().unwrap().copied, Some('c'));
    state.tick_ui(Instant::now() + Duration::from_secs(3));
    assert_eq!(state.execute_detail.as_ref().unwrap().copied, Some('c'));

    let mut final_facts = facts();
    final_facts.elapsed_ms = Some(1234);
    final_facts.output = vec![ExecuteOutput {
        kind: ExecuteOutputKind::Text,
        text: "literal\n\nWarnings:\nnot a section".into(),
        limited: false,
    }];
    let mut presentation = Presentation::new("result", 6, false);
    presentation.execute = Some(final_facts);
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "execute-ui-op",
        "execute",
        "completed",
        "result",
        6,
        false,
        None,
        None,
        None,
        Some(Box::new(presentation)),
    );
    state.execute_key(KeyAction::Char('o'));
    assert_eq!(
        state.take_copy_request().as_deref(),
        Some("literal\n\nWarnings:\nnot a section")
    );
    state.execute_key(KeyAction::End);
    for (width, height) in [(80, 24), (160, 48), (120, 40)] {
        let mut resized =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        resized
            .draw(|frame| crate::views::render_frame(frame, &state))
            .unwrap();
        if width == 80 {
            assert!(state.execute_detail.as_ref().unwrap().scroll.get() > 0);
        } else {
            assert_eq!(
                state.execute_detail.as_ref().unwrap().scroll.get(),
                0,
                "an actually painted wider viewport must retire the old scroll offset"
            );
        }
    }
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::views::render_frame(frame, &state))
        .unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(rendered.contains("Completed · 1.2s"));
    assert!(!terminal.backend().cursor_visible());
    state.execute_key(KeyAction::Cancel);
    assert_eq!(state.panel, TuiPanel::Cards);
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"exact unsent draft".to_owned(), 5)
    );
}

#[tokio::test]
async fn vis46_receiving_modal_follows_the_typed_canonical_link() {
    use oc_core::tool_stream::{ToolStreamEvent, ToolStreamIdentity};
    let mut state = fresh_state("execute-receiving").await;
    let turn = WorkerTurnId("receiving-turn".into());
    state.active_turn = Some(turn.clone());
    let identity = ToolStreamIdentity {
        round: 1,
        item_id: "provider-item".into(),
        call_id: "provider-call".into(),
    };
    let mut presentation = Presentation::new("", 0, false);
    presentation.label_only = true;
    presentation.execute = Some(Execute::default());
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Pending {
            output_presentation: Some(Box::new(presentation)),
            identity: identity.clone(),
            name: "execute".into(),
            preview: r#"{"code":"unfinished"#.into(),
            truncated: false,
        },
    );
    let op = state
        .live_parts
        .iter()
        .find_map(|part| match part {
            LivePart::Tool { card, .. } => Some(card.op.clone()),
            _ => None,
        })
        .unwrap();
    let frame = Rect::new(0, 0, 120, 40);
    state.activate_execute(&op, None, frame);
    assert_eq!(state.panel, TuiPanel::Execute);
    assert!(
        crate::execute_view::layout(&state, frame)
            .rows
            .iter()
            .any(|row| row.plain_text().contains("Waiting for code"))
    );
    state.apply_tool_argument_stream(
        &turn,
        &ToolStreamEvent::Linked {
            identity,
            op: "accepted-execute".into(),
        },
    );
    assert_eq!(
        state.execute_detail.as_ref().unwrap().operation,
        "accepted-execute"
    );
    let mut presentation = Presentation::new("", 0, false);
    presentation.label_only = true;
    presentation.execute = Some(facts());
    state.apply_tool_progress(&turn, "accepted-execute", Box::new(presentation));
    assert!(
        crate::execute_view::layout(&state, frame)
            .rows
            .iter()
            .any(|row| row.plain_text().contains("const value0"))
    );
    assert_eq!(state.panel, TuiPanel::Execute);
}

#[tokio::test]
async fn vis46_numbered_json_and_typed_rest_share_the_source_layout() {
    let (mut state, turn) = live().await;
    let mut final_facts = facts();
    final_facts.elapsed_ms = Some(10);
    final_facts.output = vec![
        ExecuteOutput {
            kind: ExecuteOutputKind::Json,
            text: "{\"value\":2}".into(),
            limited: false,
        },
        ExecuteOutput {
            kind: ExecuteOutputKind::Logs,
            text: "Logs:\nactual log".into(),
            limited: false,
        },
    ];
    let mut presentation = Presentation::new("result", 6, false);
    presentation.execute = Some(final_facts);
    state.apply_tool_finished_with_output_presentation(
        &turn,
        "execute-ui-op",
        "execute",
        "completed",
        "result",
        6,
        false,
        None,
        None,
        None,
        Some(Box::new(presentation)),
    );
    let frame = Rect::new(0, 0, 160, 48);
    state.activate_execute("execute-ui-op", None, frame);
    let layout = crate::execute_view::layout(&state, frame);
    assert_eq!(layout.rows[1].spans()[1].content(), " ");
    assert_eq!(
        layout.rows[1].spans()[1].style(),
        ratatui::style::Style::default()
    );
    let logs = layout
        .rows
        .iter()
        .position(|row| row.plain_text().trim() == "Logs:")
        .unwrap();
    assert_eq!(layout.rows[logs - 1].plain_text().trim(), "3  }");
    assert_eq!(layout.rows[logs + 1].plain_text().trim(), "actual log");
}
