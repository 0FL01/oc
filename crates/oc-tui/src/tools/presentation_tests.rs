use super::*;
use crate::history::card_from_row;
use oc_core::{
    queries::ToolOpView,
    tool_output::{Presentation, Shell},
};

fn operation(name: &str, output: &str, presentation: Option<Presentation>) -> ToolOpView {
    ToolOpView {
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
