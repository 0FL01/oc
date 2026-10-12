use super::*;
use crate::history::card_from_row;
use oc_core::{
    queries::ToolOpView,
    tool_output::{Presentation, Shell},
};

#[test]
fn mcp_label_only_keeps_wire_raw_error_limits_and_legacy_truth() {
    use oc_core::tool_output::McpLabel;
    let facts = Presentation::label(McpLabel {
        text: "vis16_output".into(),
        limited: false,
    });
    let raw = "error: real unknown execution outcome";
    let mut row = operation("mcp_hash_collision_2", raw, Some(facts.clone()));
    row.state = "failed".into();
    let before = row.clone();
    let card = card_from_row(&row);
    assert_eq!(card.name, "mcp_hash_collision_2");
    assert_eq!(card.display_name(), "vis16_output");
    assert_eq!(card.output_preview, raw);
    assert_eq!(row, before);
    assert!(
        presentation_details(&facts).is_empty(),
        "no invented body/capture facts"
    );
    let painted = tool_block(&card, Theme::dark(), 100)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(painted.contains("vis16_output") && !painted.contains("mcp_hash_collision_2"));
    let ToolRender::Inline(inline) = &card.render else {
        panic!("generic")
    };
    let expanded = generic_block_expanded(inline, &card, Theme::dark(), 100, true)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        expanded.contains(raw),
        "ordinary failure expansion retains the real error: {expanded}"
    );
    row.output = Some("real error ".repeat(1000));
    row.output_bytes = row.output.as_ref().unwrap().len() as i64;
    row.output_truncated = true;
    let bounded = card_from_row(&row);
    assert!(bounded.preview_limited() && bounded.output_preview.len() < 600);
    row.output_presentation
        .as_mut()
        .unwrap()
        .mcp_label
        .as_mut()
        .unwrap()
        .text
        .push('\n');
    let invalid = card_from_row(&row);
    assert_eq!(invalid.display_name(), row.name);
    row.output_presentation = None;
    assert_eq!(
        card_from_row(&row).display_name(),
        row.name,
        "legacy is not heuristically decoded"
    );
    row.output_presentation = Some(Box::new(facts));
    row.output_presentation
        .as_mut()
        .unwrap()
        .mcp_label
        .as_mut()
        .unwrap()
        .limited = true;
    assert!(card_from_row(&row).preview_limited());
}

fn operation(name: &str, output: &str, presentation: Option<Presentation>) -> ToolOpView {
    ToolOpView {
        child_job: None,
        output_presentation: presentation.map(Box::new),
        question: None,
        rowid: 1,
        op: "presentation-operation".into(),
        name: name.into(),
        state: "completed".into(),
        input: Some(serde_json::json!({"command":"fixture","query":"a value"}).to_string()),
        output: Some(output.into()),
        output_bytes: output.len() as i64,
        output_truncated: false,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    }
}

#[test]
fn vis37_answered_questions_use_metadata_order_block_spacing_and_semantic_roles() {
    use oc_core::question::{QuestionInput, QuestionResult};
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    let input = QuestionInput::parse(&serde_json::json!({"questions": [
        {"question": "Pick Ω界?", "header": "Pick", "options": [], "multiple": true},
        {"question": "Next?", "header": "Next", "options": []}
    ]}))
    .unwrap();
    let answers = vec![vec!["A".into(), "B".into()], vec![]];
    input.validate_answers(&answers).unwrap();
    let result = QuestionResult {
        questions: input.questions,
        answers,
    };
    let raw = serde_json::to_string(&result).unwrap();
    let mut row = operation("question", &raw, None);
    row.question = Some(result);
    let before = row.clone();
    let mut card = card_from_row(&row);
    let theme = Theme::dark();
    let lines = tool_block(&card, theme, 30);
    assert_eq!(
        lines
            .iter()
            .map(|line| line.plain_text().trim_end().to_string())
            .collect::<Vec<_>>(),
        [
            "┃",
            "┃  # Questions",
            "┃",
            "┃  Pick Ω界?",
            "┃  A, B",
            "┃",
            "┃  Next?",
            "┃  (no answer)",
            "┃"
        ]
    );
    let mut terminal = Terminal::new(TestBackend::new(30, 9)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(styled::Lines::from(lines).into_text()),
                frame.area(),
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for y in [1, 3, 6] {
        assert_eq!(buffer[(3, y)].fg, theme.text_muted());
    }
    for y in [4, 7] {
        assert_eq!(buffer[(3, y)].fg, theme.text());
    }
    for y in 0..9 {
        assert_eq!(buffer[(0, y)].symbol(), "┃");
        assert_eq!(buffer[(0, y)].fg, theme.background());
        assert_eq!(buffer[(29, y)].bg, theme.background_raised());
    }
    assert_eq!(
        row, before,
        "rendering does not change durable result or RAW"
    );

    let result = card.question.as_mut().unwrap();
    result.questions[0].question = format!("{} \u{1b}[31m\nnext", "界".repeat(24));
    result.answers[0] = vec![format!("{}\nanswer \u{7} tail", "Ω".repeat(40))];
    let before = card.question.clone();
    let wrapped = tool_block(&card, theme, 20);
    assert!(wrapped.len() > 9, "long Unicode and literal newlines wrap");
    assert!(
        wrapped.iter().all(|line| {
            line.spans().iter().map(styled::span_width).sum::<usize>() == 20
                && !line.plain_text().chars().any(char::is_control)
                && line.plain_text().starts_with("┃  ")
        }),
        "each safe continuation keeps its border/padding within the width"
    );
    assert_eq!(
        card.question, before,
        "sanitization only changes presentation"
    );
}

#[test]
fn vis37_answered_question_full_and_cached_transcripts_bypass_generic_expansion() {
    use crate::{
        history::HistoryRow,
        messages::{
            MarkdownCache, exploration_header_at_with_range, transcript_with_expansion,
            visible_transcript_expanded,
        },
    };
    use oc_core::question::{QuestionInput, QuestionResult};
    use std::cell::RefCell;
    let input = QuestionInput::parse(&serde_json::json!({"questions": [
        {"question":"Actual question?", "header":"Actual", "options":[]}
    ]}))
    .unwrap();
    let mut operation = operation("question", "model-only question output", None);
    operation.question = Some(QuestionResult {
        questions: input.questions,
        answers: vec![vec!["accepted Ω界".into()]],
    });
    let before = operation.clone();
    let card = card_from_row(&operation);
    let rows = [HistoryRow {
        message_id: None,
        seq: 1,
        role: "tool".into(),
        text: String::new(),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    }];
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    for width in [43, 80] {
        let mut previous = None;
        for expanded in [false, true] {
            let full = transcript_with_expansion(
                &rows,
                theme,
                width,
                width,
                |_| theme.text(),
                Some(&cache),
                &|_| expanded,
            );
            let full = styled::wrap_lines(&full, width as usize);
            assert!(
                full.iter()
                    .any(|line| line.plain_text().trim_end() == "┃  # Questions"),
                "real transcript dispatch must reach the answered renderer"
            );
            let (visible, total) = visible_transcript_expanded(
                &rows,
                theme,
                (width, width),
                (200, 0, None),
                |_| theme.text(),
                &cache,
                &|_| expanded,
            );
            let paint = |lines: Vec<Line>| {
                use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};
                let mut terminal = Terminal::new(TestBackend::new(width, 200)).unwrap();
                terminal
                    .draw(|frame| {
                        frame.render_widget(
                            Paragraph::new(styled::Lines::from(lines).into_text()),
                            frame.area(),
                        )
                    })
                    .unwrap();
                terminal.backend().buffer().clone()
            };
            assert_eq!(paint(visible[1..].to_vec()), paint(full));
            assert_eq!(total, visible.len());
            if let Some(previous) = previous {
                assert_eq!(visible, previous);
            }
            let y = visible
                .iter()
                .position(|line| line.plain_text().contains("# Questions"))
                .unwrap();
            assert!(
                exploration_header_at_with_range(
                    &rows,
                    theme,
                    (width, width),
                    (200, 0, None),
                    |_| theme.text(),
                    &cache,
                    (&|_| expanded, (3, y))
                )
                .is_none(),
                "Question BlockTool has no generic expansion click target"
            );
            previous = Some(visible);
        }
    }
    assert!(!generic_expandable(rows[0].tool.as_ref().unwrap()));
    assert_eq!(operation, before);
}

#[test]
fn vis37_pending_question_uses_validated_input_not_stream_fragments_or_answer_results() {
    use crate::{
        history::HistoryRow,
        messages::{MarkdownCache, transcript_with_expansion, visible_transcript_expanded},
    };
    use std::cell::RefCell;
    let input = serde_json::json!({"questions": [
        {"question":"First actual question?", "header":"First", "options":[]},
        {"question":"Second actual question?", "header":"Second", "options":[], "multiple":true}
    ]})
    .to_string();
    let mut operation = operation("question", "not an answer result", None);
    operation.state = "started".into();
    operation.input = Some(input.clone());
    let before = operation.clone();
    let card = card_from_row(&operation);
    let theme = Theme::dark();
    let lines = tool_block(&card, theme, 80);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].plain_text(), "   → Asked 2 questions");
    assert_eq!(
        lines[0].spans().last().unwrap().style().fg,
        Some(theme.text_muted())
    );
    assert!(card.question.is_none(), "pending is not an invented result");
    assert!(!generic_expandable(&card));
    let rows = [HistoryRow {
        message_id: None,
        seq: 1,
        role: "tool".into(),
        text: String::new(),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    }];
    let cache = RefCell::new(MarkdownCache::default());
    for expanded in [false, true] {
        let full = transcript_with_expansion(
            &rows,
            theme,
            80,
            80,
            |_| theme.text(),
            Some(&cache),
            &|_| expanded,
        );
        assert!(
            full.iter()
                .any(|line| line.plain_text() == "   → Asked 2 questions")
        );
        let (visible, _) = visible_transcript_expanded(
            &rows,
            theme,
            (80, 80),
            (200, 0, None),
            |_| theme.text(),
            &cache,
            &|_| expanded,
        );
        assert_eq!(visible[1..], styled::wrap_lines(&full, 80));
    }
    assert_eq!(operation, before, "RAW and operation binding are unchanged");
    for (state, input) in [
        ("argument_stream", input.as_str()),
        ("started", "{\"questions\":["),
        ("started", "{\"questions\":[{}]}"),
    ] {
        operation.state = state.into();
        operation.input = Some(input.into());
        let card = card_from_row(&operation);
        let lines = tool_block(&card, theme, 80);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plain_text(), "   → Asking questions…");
        assert!(card.question.is_none());
        assert!(!generic_expandable(&card));
    }
    operation.state = "started".into();
    operation.input = Some(
        serde_json::json!({"questions":[
            {"question":"Only?", "header":"Only", "options":[]}
        ]})
        .to_string(),
    );
    assert_eq!(
        tool_block(&card_from_row(&operation), theme, 80)[0].plain_text(),
        "   → Asked 1 question"
    );
    operation.state = "cancelled".into();
    operation.output = Some("The user dismissed this question".into());
    let cancelled = tool_block(&card_from_row(&operation), theme, 80);
    assert!(
        cancelled
            .iter()
            .any(|line| line.plain_text().contains("dismissed"))
    );
    assert!(
        cancelled
            .iter()
            .all(|line| !line.plain_text().contains("# Questions"))
    );
}

#[test]
fn owned_body_excludes_guidance_but_literal_and_legacy_text_stay_data() {
    let body = "actual [Part preview truncated]\n[output preview truncated; full result retained]\n[truncated]";
    let recorded = format!("{body}\n[tool output: generated model guidance]");
    let mut row = operation(
        "mcp_fixture",
        &recorded,
        Some(Presentation::new(body, body.len() as u64, true)),
    );
    row.output_truncated = true; // Raw guidance was clipped, not this complete body.
    let card = card_from_row(&row);
    assert_eq!(card.output_preview, body);
    assert!(!card.preview_limited());
    assert_eq!(
        row.output.as_deref(),
        Some(recorded.as_str()),
        "view construction does not rewrite RAW"
    );
    assert!(
        card.output_presentation
            .as_ref()
            .unwrap()
            .generated_guidance
    );

    let legacy = card_from_row(&operation("mcp_fixture", &recorded, None));
    assert_eq!(
        legacy.output_preview, recorded,
        "no guessed legacy suffix stripping"
    );
    assert!(legacy.output_presentation.is_none());

    row.input = Some(serde_json::json!({"query": "界".repeat(2000)}).to_string());
    let bounded_input = card_from_row(&row);
    assert!(
        bounded_input.preview_limited(),
        "compact status includes input-only loss"
    );
    let ToolRender::Inline(InlineRender::Generic {
        args,
        arguments_limited,
        ..
    }) = &bounded_input.render
    else {
        panic!("generic input")
    };
    assert!(*arguments_limited);
    assert!(
        args.iter()
            .map(|(key, value)| key.len() + value.len())
            .sum::<usize>()
            <= 2048
    );
    assert!(
        !bounded_input
            .output_presentation
            .as_ref()
            .unwrap()
            .body_limited
    );
}

#[test]
fn shell_status_is_owned_and_projection_does_not_synthesize_truncation() {
    let stdout = "exit 0\n[truncated]\n[timeout]\n[cancelled]\n[stderr]\n";
    let mut presentation = Presentation::new("available body", 20_000, true);
    presentation.shell = Some(Shell {
        background: false,
        process_state: None,
        stdout: stdout.into(),
        stderr: "real stderr\n".into(),
        stdout_limited: true,
        stderr_limited: false,
        exit: Some(17),
        signal: None,
        timed_out: false,
        cancelled: false,
    });
    let mut row = operation(
        "shell",
        "exit 17\n[tool output: model-only guidance]",
        Some(presentation),
    );
    row.output_truncated = true;
    let card = card_from_row(&row);
    assert!(card.preview_limited());
    let ToolRender::Shell(shell) = &card.render else {
        panic!("native shell card")
    };
    assert_eq!(shell.stdout, stdout.lines().collect::<Vec<_>>());
    assert_eq!(shell.stderr, ["real stderr"]);
    assert_eq!(shell.exit, Some(17));
    assert!(!shell.timed_out && !shell.cancelled && !shell.signal);
    let text = shell_block_expanded(shell, &card, Theme::dark(), 120, true)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        text.matches("[truncated]").count(),
        1,
        "only the literal process output remains"
    );
    assert!(
        text.contains("[timeout]") && text.contains("[cancelled]") && text.contains("real stderr")
    );
    assert!(text.contains("Command exited with code 17."));
    assert!(!text.contains("model-only guidance") && !text.contains("full result retained"));

    let legacy = card_from_row(&operation(
        "shell",
        "exit 0\n[truncated]\n[timeout]\n",
        None,
    ));
    let ToolRender::Shell(shell) = legacy.render else {
        panic!("legacy shell")
    };
    assert_eq!(shell.stdout, ["[truncated]", "[timeout]"]);
    assert!(!shell.timed_out);
}

#[test]
fn shell_command_character_wrap_keeps_continuation_space_and_owned_body() {
    let command = format!(
        "printf 'VIS-SHELL-LINE-%03d\\n' {}; printf '[Part preview truncated]\\n[output preview truncated; full result retained]\\n[truncated]\\n'",
        (1..=80)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let body = format!(
        "{}[Part preview truncated]\n[output preview truncated; full result retained]\n[truncated]\n",
        (1..=80)
            .map(|n| format!("VIS-SHELL-LINE-{n:03}\n"))
            .collect::<String>()
    );
    let mut facts = Presentation::new(&body, body.len() as u64, false);
    facts.shell = Some(Shell {
        background: false,
        process_state: Some("completed".into()),
        stdout: "separate bounded stream field".into(),
        stderr: String::new(),
        stdout_limited: true,
        stderr_limited: false,
        exit: Some(0),
        signal: None,
        timed_out: false,
        cancelled: false,
    });
    let mut row = operation("shell", "exit 0\nmodel-only preview", Some(facts));
    row.input = Some(serde_json::json!({"command":command}).to_string());
    let before = row.clone();
    let card = card_from_row(&row);
    let saved_card = card.clone();
    let ToolRender::Shell(shell) = &card.render else {
        panic!("owned shell")
    };
    // A 120-column transcript gives this card its actual 116-column block.
    let rows = shell_block_expanded(shell, &card, Theme::dark(), 116, false);
    assert!(
        rows[1].plain_text().trim_end().ends_with(" 30"),
        "{:?}",
        rows.iter()
            .take(3)
            .map(Line::plain_text)
            .collect::<Vec<_>>()
    );
    assert!(rows[2].plain_text().starts_with("┃   31 32 33"));
    for command in &rows[1..=2] {
        assert_eq!(command.spans()[1].content(), "  ");
        assert_eq!(command.spans()[1].style().fg, None);
        assert_eq!(
            command.spans()[1].style().bg,
            Some(Theme::dark().background_raised())
        );
    }
    let collapsed = rows
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(collapsed.contains("(77 earlier lines)"));
    let expanded = shell_block_expanded(shell, &card, Theme::dark(), 116, true)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    for marker in [
        "[Part preview truncated]",
        "[output preview truncated; full result retained]",
        "[truncated]",
    ] {
        assert_eq!(collapsed.matches(marker).count(), 1);
        assert_eq!(
            expanded
                .lines()
                .filter(|line| line.trim_end() == format!("┃  {marker}"))
                .count(),
            1,
            "the complete literal body row survives independently of command wrapping"
        );
    }
    assert!(expanded.contains("VIS-SHELL-LINE-001") && expanded.contains("VIS-SHELL-LINE-080"));
    assert!(!expanded.contains("model-only") && !expanded.contains("separate bounded stream"));
    assert!(!expanded.contains("Command exited with code 0."));
    assert_eq!(row, before);
    assert_eq!(card, saved_card);

    let style = ratatui::style::Style::default().fg(Theme::dark().text());
    let unicode = Line::styled("a界 b😀 c", style);
    let wrapped = styled::wrap_char_line_limited(&unicode, 4, 4);
    assert_eq!(
        wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
        ["a界 ", "b😀 ", "c"]
    );
    assert!(
        wrapped
            .iter()
            .flat_map(Line::spans)
            .all(|span| span.style() == style)
    );
    assert_eq!(styled::wrap_char_line_limited(&unicode, 4, 1), wrapped[..1]);
    assert!(styled::wrap_char_line_limited(&unicode, 4, 0).is_empty());
    assert_eq!(
        styled::wrap_char_line_limited(&Line::plain("界"), 1, 2)[0].plain_text(),
        "…"
    );
}

#[test]
fn generic_denial_and_capture_state_remain_distinct_from_failures_and_view_loss() {
    use oc_core::tool_output::{Capture, CaptureState};
    let mut row = operation("mcp_fixture", "error: denied by policy", None);
    row.state = "denied".into();
    let denied = card_from_row(&row);
    let ToolRender::Inline(inline) = &denied.render else {
        panic!("generic tool")
    };
    let text = generic_block_expanded(inline, &denied, Theme::dark(), 120, true)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("query: a value") && text.contains("error: denied by policy"));
    row.state = "failed".into();
    let failed = card_from_row(&row);
    let text = generic_block_expanded(inline, &failed, Theme::dark(), 120, true)
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("error: denied by policy") && !text.contains("query: a value"));

    for (state, producer_limited, expected) in [
        (CaptureState::Active, false, Some("Capturing output")),
        (CaptureState::Complete, false, None),
        (
            CaptureState::Complete,
            true,
            Some("Producer limited · /cards"),
        ),
        (
            CaptureState::ProducerLimited,
            false,
            Some("Producer limited · /cards"),
        ),
        (
            CaptureState::Expired,
            false,
            Some("Saved output expired · /cards"),
        ),
        (
            CaptureState::ArtifactCap,
            false,
            Some("Capture incomplete · /cards"),
        ),
        (
            CaptureState::Quota,
            false,
            Some("Capture incomplete · /cards"),
        ),
        (CaptureState::Io, false, Some("Capture incomplete · /cards")),
        (
            CaptureState::RegisterFailure,
            false,
            Some("Capture incomplete · /cards"),
        ),
        (
            CaptureState::Interrupted,
            false,
            Some("Capture incomplete · /cards"),
        ),
    ] {
        let mut facts = Presentation::new("available body", 20_000, true);
        facts.producer_limited = producer_limited.then_some(true);
        facts.capture = Some(Capture {
            reference: None,
            state,
            admitted_bytes: 20_000,
            retained_bytes: 100,
            admitted_lines: 100,
            retained_lines: 5,
        });
        let card = card_from_row(&operation("mcp_fixture", "guidance", Some(facts)));
        assert!(
            card.preview_limited(),
            "projection loss remains independent of {state:?}"
        );
        assert_eq!(capture_status(&card), expected);
    }
}

#[test]
fn generic_parameters_body_and_click_extents_match_full_and_indexed_projection() {
    use crate::{
        history::HistoryRow,
        messages::{
            MarkdownCache, exploration_header_at_with_range, transcript_with_expansion,
            visible_transcript_expanded,
        },
    };
    use std::cell::RefCell;
    let body = "kept [Part preview truncated]\n[output preview truncated; full result retained]";
    let mut row = operation(
        "mcp_fixture",
        "model-only guidance",
        Some(Presentation::new(body, body.len() as u64, true)),
    );
    row.input = Some(
        r#"{"query":"a value","limit":3,"enabled":true,"nested":{"key":"detail"},"empty":null}"#
            .into(),
    );
    let card = card_from_row(&row);
    let rows = [HistoryRow {
        message_id: None,
        seq: 1,
        role: "tool".into(),
        text: String::new(),
        agent: None,
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    }];
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    for width in [43, 80, 120] {
        let mut collapsed = Vec::new();
        for expanded in [false, true, false] {
            let full = transcript_with_expansion(
                &rows,
                theme,
                width,
                width,
                |_| theme.text(),
                Some(&cache),
                &|_| expanded,
            );
            let full = crate::styled::wrap_lines(&full, width as usize);
            let (visible, total) = visible_transcript_expanded(
                &rows,
                theme,
                (width, width),
                (200, 0, None),
                |_| theme.text(),
                &cache,
                &|_| expanded,
            );
            assert_eq!(
                &visible[1..],
                full.as_slice(),
                "width {width}, expansion {expanded}"
            );
            assert_eq!(total, visible.len());
            let text = visible
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(!text.contains("model-only guidance"));
            if expanded {
                for wanted in [
                    "query: a value",
                    "nested:",
                    "detail",
                    "empty: null",
                    "output:",
                ] {
                    assert!(
                        text.contains(wanted),
                        "available parameter/output missing {wanted}: {text}"
                    );
                }
                assert!(
                    text.contains("[Part preview truncated]"),
                    "literal body remains: {text}"
                );
            } else {
                assert!(
                    !text.contains("detail")
                        && !text.contains("output:")
                        && !text.contains("[Part preview truncated]")
                );
                if collapsed.is_empty() {
                    collapsed = visible.clone();
                } else {
                    assert_eq!(visible, collapsed);
                }
            }
            let card = rows[0].tool.as_ref().unwrap();
            let ToolRender::Inline(inline) = &card.render else {
                panic!("generic tool")
            };
            let header = generic_block_expanded(inline, card, theme, width, false);
            for (y, line) in visible.iter().enumerate() {
                for x in 0..width as usize {
                    let hit = exploration_header_at_with_range(
                        &rows,
                        theme,
                        (width, width),
                        (200, 0, None),
                        |_| theme.text(),
                        &cache,
                        (&|_| expanded, (x, y)),
                    );
                    let expected = y >= 2 && y < 2 + header.len() && {
                        let text = line.plain_text();
                        let leading = UnicodeWidthStr::width(text.as_str())
                            - UnicodeWidthStr::width(text.trim_start());
                        x >= leading && x < UnicodeWidthStr::width(text.trim_end())
                    };
                    assert_eq!(hit.is_some(), expected, "width {width} / {x},{y}");
                    if let Some((operation, range)) = hit {
                        assert_eq!(operation, "presentation-operation");
                        assert_eq!(range, 2..2 + header.len());
                    }
                }
            }
        }
    }
}
