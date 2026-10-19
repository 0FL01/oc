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
    assert_eq!(
        expanded.iter().map(|row| &row.line).collect::<Vec<_>>(),
        collapsed.iter().map(|row| &row.line).collect::<Vec<_>>(),
        "parent opening never expands the static transcript failure preview"
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
    assert!(initial.rows.len() > initial.body.height as usize);
    assert!(initial.rows.iter().all(|row| {
        row.spans()
            .iter()
            .map(crate::styled::span_width)
            .sum::<usize>()
            <= initial.body.width as usize
    }));
    assert_eq!(
        initial.rect.y,
        frame.height.saturating_sub(initial.rect.height).div_ceil(2)
    );
    state.execute_key(KeyAction::Right);
    state.execute_key(KeyAction::Left);
    assert_eq!(state.execute_detail.as_ref().unwrap().scroll.get(), 0);
    assert_eq!(
        crate::execute_view::layout(&state, frame).rows,
        initial.rows
    );
    state.execute_key(KeyAction::Down);
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
    let mut previous = state.execute_detail.as_ref().unwrap().scroll.get();
    for (width, height) in [(80, 24), (160, 48), (120, 40)] {
        let layout = crate::execute_view::layout(&state, Rect::new(0, 0, width, height));
        let expected = previous.min(
            layout
                .rows
                .len()
                .saturating_sub(layout.body.height as usize),
        );
        let mut resized =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        resized
            .draw(|frame| crate::views::render_frame(frame, &state))
            .unwrap();
        assert_eq!(
            state.execute_detail.as_ref().unwrap().scroll.get(),
            expected,
            "an actually painted viewport retires offsets, and re-narrowing cannot resurrect them"
        );
        previous = expected;
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
    assert_eq!(
        layout.rows[logs - 1].plain_text().trim(),
        "1  {\"value\":2}"
    );
    assert_eq!(layout.rows[logs + 1].plain_text().trim(), "actual log");
}

#[tokio::test]
async fn vis46_every_failed_parent_opens_truthful_details_and_copies_safe_fallback() {
    for outcome in ["failed", "denied", "cancelled", "unknown", "no_gain"] {
        let (mut state, turn) = live().await;
        let mut presentation = Presentation::new("safe captured failure", 21, false);
        presentation.execute = Some(facts());
        state.apply_tool_finished_with_output_presentation(
            &turn,
            "execute-ui-op",
            "execute",
            outcome,
            "safe captured failure",
            21,
            false,
            None,
            None,
            None,
            Some(Box::new(presentation)),
        );
        let frame = Rect::new(0, 0, 120, 40);
        let (rows, area) = paint(&mut state, frame);
        let y = area.y
            + rows
                .iter()
                .position(|row| row.plain_text().trim_end().ends_with("execute"))
                .unwrap() as u16;
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            assert!(
                state
                    .handle_mouse(mouse(kind, area.x + 5, y), frame)
                    .intent
                    .is_none()
            );
        }
        assert_eq!(state.panel, TuiPanel::Execute, "{outcome}");
        let rendered = crate::views::render_test(&state, 120, 40).join("\n");
        assert!(rendered.contains("Failed"), "{outcome}: {rendered}");
        let layout = crate::execute_view::layout(&state, frame);
        assert!(
            layout
                .rows
                .iter()
                .any(|row| row.plain_text().contains("safe captured failure"))
        );
        state.execute_key(KeyAction::Char('o'));
        assert_eq!(
            state.take_copy_request().as_deref(),
            Some("safe captured failure")
        );
        state.execute_key(KeyAction::Cancel);
        assert_eq!(state.panel, TuiPanel::None);
        assert!(!state.exploration_expanded.contains("execute-ui-op"));
        assert_eq!(
            (&state.input, state.editor.cursor),
            (&"exact unsent draft".to_owned(), 5)
        );
    }
}

#[tokio::test]
async fn vis46_character_wrap_preserves_source_whitespace_unicode_and_original_json() {
    let (mut state, turn) = live().await;
    let code = format!(
        "  const value = {:?};  \n\treturn value;\n\n{}",
        "界🦊e\u{301} separators ".repeat(8),
        (0..10)
            .map(|n| format!("// line {n}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let json = format!("{{\"value\":{:?},\"n\":2}}", "界 long value ".repeat(8));
    let mut final_facts = facts();
    final_facts.code = Some(code.clone());
    final_facts.code_limited = true;
    final_facts.output = vec![ExecuteOutput {
        kind: ExecuteOutputKind::Json,
        text: json.clone(),
        limited: true,
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
    state.activate_execute("execute-ui-op", None, Rect::new(0, 0, 80, 24));
    for width in [25, 80, 160] {
        let layout = crate::execute_view::layout(&state, Rect::new(0, 0, width, 40));
        let output = layout
            .rows
            .iter()
            .position(|row| row.plain_text() == "Output")
            .unwrap();
        let reconstruct = |rows: &[crate::styled::Line]| {
            let mut logical = Vec::<String>::new();
            for row in rows {
                assert!(
                    row.spans()
                        .iter()
                        .map(crate::styled::span_width)
                        .sum::<usize>()
                        <= layout.body.width as usize
                );
                if !row.spans()[0].content().trim().is_empty() {
                    assert_eq!(
                        row.spans()[0].content().trim().parse::<usize>().unwrap(),
                        logical.len() + 1
                    );
                    logical.push(String::new());
                }
                assert_eq!(
                    row.spans()[0].content().len(),
                    2,
                    "shared logical-line gutter"
                );
                assert_eq!(row.spans()[1].style(), ratatui::style::Style::default());
                for span in &row.spans()[2..] {
                    logical.last_mut().unwrap().push_str(span.content());
                }
            }
            logical
        };
        assert_eq!(
            reconstruct(&layout.rows[1..output - 2]),
            code.replace('\t', "  ")
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            reconstruct(&layout.rows[output + 1..layout.rows.len() - 1]),
            std::slice::from_ref(&json)
        );
        assert_eq!(
            layout.rows[output - 2].plain_text(),
            "… code preview limited"
        );
        assert_eq!(
            layout.rows.last().unwrap().plain_text(),
            "… output preview limited"
        );
    }
    state.execute_key(KeyAction::Char('c'));
    assert_eq!(state.take_copy_request().as_deref(), Some(code.as_str()));
    state.execute_key(KeyAction::Char('o'));
    assert_eq!(state.take_copy_request().as_deref(), Some(json.as_str()));
}

#[tokio::test]
async fn vis46_fetch_and_search_disclosures_are_independent_of_each_other_and_the_parent() {
    let (mut state, turn) = live().await;
    let mut progress = Presentation::new("", 0, false);
    progress.label_only = true;
    let mut current = facts();
    current.calls = vec![
        ExecuteCall {
            tool: "fetch".into(),
            status: ExecuteCallStatus::Completed,
            input: Some(serde_json::json!({"method":"GET","url":"https://example.test/data"})),
            tool_limited: false,
            input_limited: false,
        },
        ExecuteCall {
            tool: "search".into(),
            status: ExecuteCallStatus::Completed,
            input: Some(serde_json::json!({"query":"needle","namespace":"fixture","limit":12})),
            tool_limited: false,
            input_limited: false,
        },
    ];
    progress.execute = Some(current);
    state.apply_tool_progress(&turn, "execute-ui-op", Box::new(progress));
    let frame = Rect::new(0, 0, 120, 40);
    let (rows, _) = paint(&mut state, frame);
    for name in ["fetch", "search"] {
        let line = rows
            .iter()
            .find(|row| row.plain_text().contains(&format!("› {name} [")))
            .unwrap();
        assert!(
            line.spans()
                .iter()
                .filter(|span| !span.content().trim().is_empty())
                .all(|span| span.style().fg == Some(crate::theme::Theme::dark().text_muted()))
        );
    }
    for name in ["fetch", "search", "search"] {
        let (rows, area) = paint(&mut state, frame);
        let y = area.y
            + rows
                .iter()
                .position(|row| row.plain_text().contains(&format!("› {name}")))
                .unwrap() as u16;
        for kind in [
            MouseEventKind::Down(MouseButton::Left),
            MouseEventKind::Up(MouseButton::Left),
        ] {
            state.handle_mouse(mouse(kind, area.x + 5, y), frame);
        }
        assert_eq!(state.panel, TuiPanel::None);
    }
    assert!(state.execute_calls_expanded["execute-ui-op"].contains(&0));
    assert!(!state.execute_calls_expanded["execute-ui-op"].contains(&1));
    let (rows, _) = paint(&mut state, frame);
    assert!(
        rows.iter()
            .any(|row| row.plain_text().contains("method: GET"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.plain_text().contains("namespace: fixture"))
    );
    state.activate_execute("execute-ui-op", None, frame);
    state.execute_key(KeyAction::Cancel);
    assert!(state.execute_calls_expanded["execute-ui-op"].contains(&0));
}

#[tokio::test]
async fn vis46_details_survive_source_loss_only_in_the_owning_view() {
    let (mut state, turn) = live().await;
    let frame = Rect::new(0, 0, 120, 40);
    state.activate_execute("execute-ui-op", None, frame);
    let mut latest = facts();
    latest.code = Some("return 'latest admitted code';".into());
    let mut progress = Presentation::new("", 0, false);
    progress.label_only = true;
    progress.execute = Some(latest);
    state.apply_tool_progress(&turn, "execute-ui-op", Box::new(progress));
    // Actual resident-part eviction, not a history fetch or a pinned old window.
    state
        .live_parts
        .extend((0..LIVE_PARTS_MAX).map(|_| LivePart::Text("later".into())));
    state.enforce_parts();
    assert!(state.execute_card("execute-ui-op").is_none());
    let mut refreshed = fresh_state("execute-ui").await;
    refreshed.preserve_shared_composer(state);
    let rendered = crate::views::render_test(&refreshed, 120, 40).join("\n");
    assert!(rendered.contains("Source unavailable"));
    assert!(!rendered.contains("Completed"));
    assert!(rendered.contains("latest admitted code"));
    refreshed.execute_key(KeyAction::Char('c'));
    assert_eq!(
        refreshed.take_copy_request().as_deref(),
        Some("return 'latest admitted code';")
    );
    refreshed.active_turn = Some(turn.clone());
    let mut presentation = Presentation::new("terminal", 8, false);
    let mut final_facts = facts();
    final_facts.code = Some("return 'new canonical code';".into());
    presentation.execute = Some(final_facts);
    refreshed.apply_tool_finished_with_output_presentation(
        &turn,
        "execute-ui-op",
        "execute",
        "completed",
        "terminal",
        8,
        false,
        None,
        None,
        None,
        Some(Box::new(presentation)),
    );
    let rendered = crate::views::render_test(&refreshed, 120, 40).join("\n");
    assert!(rendered.contains("Completed"));
    assert!(!rendered.contains("Source unavailable"));
    assert!(rendered.contains("new canonical code"));
    let expected = {
        let captured = refreshed.execute_capture().unwrap().0;
        assert!(
            captured.facts.calls.is_empty(),
            "no duplicate child-parameter cache"
        );
        "execute-ui-op".len() + captured.facts.retained_bytes()
    };
    assert_eq!(
        refreshed.execute_detail.as_ref().unwrap().retained_bytes(),
        expected
    );
    assert!(refreshed.retained_bytes() >= expected);
    let mut other = fresh_state("different-session").await;
    other.preserve_shared_composer(refreshed);
    assert!(other.execute_detail.is_none());
    assert_eq!(other.panel, TuiPanel::None);
}

#[tokio::test]
async fn vis46_approval_is_visible_and_retires_execute_presses_without_losing_details() {
    use crossterm::event::{KeyCode, KeyEvent};
    use oc_core::approval::{ApprovalBinding, ApprovalPreview, ApprovalRequest};
    let (mut state, _) = live().await;
    let frame = Rect::new(0, 0, 120, 40);
    state.activate_execute("execute-ui-op", None, frame);
    let copy = crate::execute_view::layout(&state, frame)
        .actions
        .into_iter()
        .find(|(key, _)| *key == 'c')
        .unwrap()
        .1;
    state.handle_mouse(
        mouse(MouseEventKind::Down(MouseButton::Left), copy.x, copy.y),
        frame,
    );
    state.approvals.reconcile(vec![(
        ApprovalRequest {
            id: 7,
            binding: ApprovalBinding {
                session: "execute-ui".into(),
                turn: "execute-ui-turn".into(),
                call: "nested-call".into(),
                operation: "execute-ui-op".into(),
                input_digest: "digest".into(),
                location: "location".into(),
                generation: 1,
                agent: None,
                agent_digest: None,
            },
            project: "project".into(),
            action: "fixture.nested".into(),
            resources: vec!["*".into()],
            save_patterns: vec![],
            preview: ApprovalPreview::Tool,
        },
        false,
    )]);
    for fullscreen in [false, true] {
        state.approvals.fullscreen = fullscreen;
        let rendered = crate::views::render_test(&state, 120, 40).join("\n");
        assert!(rendered.contains("Tool: fixture.nested"), "{rendered}");
        assert!(
            !rendered.contains("copy code"),
            "approval must not be obscured"
        );
    }
    let key = state
        .terminal_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(matches!(
        state.handle_key(key).await.intent,
        Some(PanelIntent::ReplyApproval(_))
    ));
    assert_eq!(state.panel, TuiPanel::Execute);
    state.approvals.reconcile(vec![]);
    state.handle_mouse(
        mouse(MouseEventKind::Up(MouseButton::Left), copy.x, copy.y),
        frame,
    );
    assert!(
        state.take_copy_request().is_none(),
        "approval transfer retires old Execute presses"
    );
    let rendered = crate::views::render_test(&state, 120, 40).join("\n");
    assert!(rendered.contains("copy code"));
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"exact unsent draft".to_owned(), 5)
    );
}

#[tokio::test]
async fn vis46_read_only_paste_and_captured_backdrop_preserve_the_previous_panel() {
    let (mut state, _) = live().await;
    let frame = Rect::new(0, 0, 120, 40);
    state.panel = TuiPanel::Cards;
    state.select.query = "hidden prior search".into();
    state.activate_execute("execute-ui-op", None, frame);
    assert!(state.handle_paste("must not leak").intent.is_none());
    assert_eq!(state.select.query, "hidden prior search");
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"exact unsent draft".to_owned(), 5)
    );
    assert!(
        !crate::execute_view::layout(&state, frame)
            .rect
            .contains((0, 0).into())
    );
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0), frame);
    state.handle_mouse(mouse(MouseEventKind::Drag(MouseButton::Left), 1, 0), frame);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 0, 0), frame);
    assert_eq!(state.panel, TuiPanel::Execute);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0), frame);
    state.handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left), 0, 1), frame);
    assert_eq!(state.panel, TuiPanel::Execute);
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        state.handle_mouse(mouse(kind, 0, 0), frame);
    }
    assert_eq!(state.panel, TuiPanel::Cards);
    assert!(state.execute_detail.is_none());
    assert_eq!(
        (&state.input, state.editor.cursor),
        (&"exact unsent draft".to_owned(), 5)
    );
    state.activate_execute("execute-ui-op", None, frame);
    let action = state
        .terminal_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('c'),
            KeyModifiers::CONTROL,
        ))
        .unwrap();
    assert!(state.handle_key(action).await.intent.is_none());
    assert_eq!(state.panel, TuiPanel::Cards);
}
