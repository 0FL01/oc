use super::*;
use oc_core::{
    queries::UserShellResult,
    tool_output::{Presentation, Shell},
};
use std::cell::RefCell;

fn message(state: &str, exit: Option<i32>, stdout: &str) -> HistoryMessage {
    let mut output = Presentation::new(stdout, stdout.len() as u64, false);
    output.shell = Some(Shell {
        stdout: stdout.into(),
        stderr: String::new(),
        stdout_limited: false,
        stderr_limited: false,
        exit,
        signal: None,
        timed_out: state == "timed_out",
        cancelled: state == "cancelled",
    });
    HistoryMessage {
        id: oc_core::session::MessageId("actual-notice".into()),
        seq: 7,
        role: Role::User,
        text: "RAW data-only notice is preserved, not parsed by the UI".into(),
        turn: None,
        model_switch: None,
        user_shell: Some(UserShellResult {
            input: false,
            superseded_input: false,
            operation: "actual-null-turn-operation".into(),
            command: "printf Ω界".into(),
            command_limited: false,
            state: state.into(),
            output: Box::new(output),
            diagnostic: None,
        }),
    }
}

#[test]
fn direct_user_shell_is_a_single_typed_block_without_assistant_graph_or_user_actions() {
    let raw = message(
        "completed",
        Some(0),
        "literal [truncated]\n[timeout]\n[stderr]\nexit 0\n",
    );
    let original = raw.clone();
    let rows = rows_from_page(&raw);
    assert_eq!(raw, original, "UI projection never rewrites RAW DTO text");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].role, "shell");
    assert_eq!(rows[0].message_id.as_deref(), Some(&raw.id));
    assert!(rows[0].meta.is_none() && rows[0].agent.is_none());
    assert!(rows[0].text.is_empty());
    let card = rows[0].tool.as_ref().unwrap();
    assert_eq!(card.op, "actual-null-turn-operation");
    assert!(!card.preview_limited());
    assert!(
        crate::tools::shell_expandable(card, 120),
        "donor Shell ID is expandable even for short output"
    );
    let theme = crate::theme::Theme::dark();
    let block = crate::tools::tool_block(card, theme, 120);
    assert!(
        block.iter().all(|row| row.spans()[1].style().fg.is_none()),
        "standalone box padding inherits foreground, not muted output styling"
    );
    let mut model_card = card.clone();
    let crate::tools::ToolRender::Shell(shell) = &mut model_card.render else {
        panic!("Shell render")
    };
    shell.direct_user = false;
    assert!(
        crate::tools::tool_block(&model_card, theme, 120)
            .iter()
            .all(|row| row.spans()[1].style().fg == Some(theme.text_muted())),
        "model Shell frame styling is unchanged"
    );
    let painted = crate::messages::transcript(&rows, theme, 120, 120, |_| theme.text());
    let mut running = card.clone();
    running.state = "started".into();
    let command = crate::tools::tool_block(&running, theme, 120);
    assert_eq!(command[1].spans()[3].content(), " ");
    assert_eq!(
        command[1].spans()[3].style().fg,
        None,
        "spinner layout gap inherits the standalone block"
    );
    let text = painted
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("$ printf Ω界"));
    assert!(
        text.contains("literal [truncated]")
            && text.contains("[timeout]")
            && text.contains("[stderr]")
            && text.contains("exit 0")
    );
    assert!(!text.contains("Command exited with code 0") && !text.contains("RAW data-only"));
    assert!(!text.chars().any(|c| c == '\x1b'));
    let cache = RefCell::new(crate::messages::MarkdownCache::default());
    let (visible, total, targets) = crate::messages::visible_transcript_user_targets(
        &rows,
        theme,
        (120, 120),
        (40, 0, None),
        |_| theme.text(),
        &cache,
        &|_| false,
    );
    // The indexed viewport has its global leading row and coalesces equal-style
    // spans while wrapping. Compare painted cells, not span segmentation.
    let mut expected = vec![crate::styled::Line::plain("")];
    expected.extend(painted);
    assert_eq!(total, expected.len());
    let paint = |lines| {
        use ratatui::widgets::Widget;
        let area = ratatui::layout::Rect::new(0, 0, 120, 40);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        ratatui::widgets::Paragraph::new(crate::styled::Lines::from(lines).into_text())
            .render(area, &mut buffer);
        buffer
    };
    assert_eq!(paint(visible), paint(expected));
    assert!(
        targets.iter().all(Option::is_none),
        "not a user-message Revert/Fork target"
    );
    let mut ordinary = raw;
    ordinary.user_shell.as_mut().unwrap().superseded_input = true;
    let hidden = rows_from_page(&ordinary);
    assert_eq!(hidden[0].message_id.as_deref(), Some(&ordinary.id));
    assert_eq!(hidden[0].seq, ordinary.seq);
    assert!(crate::messages::transcript(&hidden, theme, 120, 120, |_| theme.text()).is_empty());
    ordinary.user_shell = None;
    assert_eq!(
        rows_from_page(&ordinary)[0].role,
        "user",
        "notice-like prose is not classified"
    );
}

#[test]
fn direct_user_shell_terminal_errors_literal_prose_and_view_limits_remain_truthful() {
    for (state, exit, status) in [
        ("failed", Some(17), "Command exited with code 17"),
        ("cancelled", None, "Command cancelled"),
        ("timed_out", None, "Command timed out"),
        ("unknown", None, "Outcome unknown"),
    ] {
        let mut raw = message(
            state,
            exit,
            "Command exited with code 0.\n\x1b[31mterminal data\x1b[0m",
        );
        raw.user_shell.as_mut().unwrap().diagnostic = Some("owned safe diagnostic".into());
        let rows = rows_from_page(&raw);
        let theme = crate::theme::Theme::dark();
        let text = crate::messages::transcript(&rows, theme, 120, 120, |_| theme.text())
            .iter()
            .map(crate::styled::Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains(status), "{state}: {text}");
        assert!(
            text.contains("Command exited with code 0."),
            "literal output is not a generated success notice"
        );
        assert!(text.contains("owned safe diagnostic"));
        assert!(!text.contains('\x1b'));
    }
    let mut limited = message("completed", Some(0), "available prefix");
    let shell = limited.user_shell.as_mut().unwrap();
    shell.command_limited = true;
    shell.output.body_limited = true;
    shell.output.shell.as_mut().unwrap().stdout_limited = true;
    let rows = rows_from_page(&limited);
    assert!(rows[0].tool.as_ref().unwrap().preview_limited());
    let theme = crate::theme::Theme::dark();
    let text = crate::messages::transcript(&rows, theme, 120, 120, |_| theme.text())
        .iter()
        .map(crate::styled::Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Input preview"));
    assert!(!text.contains("[truncated]") && !text.contains("full result retained"));
    let page = HistoryPage {
        rows: vec![limited; 240],
        total: 240,
        has_older: false,
        has_newer: false,
        parent_id: None,
        title: None,
        reverted: None,
    };
    let mut window = HistoryWindow::new();
    window.reset(&page);
    assert!(window.retained_bytes() <= WINDOW_BYTES && window.len() <= WINDOW_ROWS);
}

#[test]
fn direct_user_shell_separate_pages_and_live_refresh_retire_only_matching_input() {
    let mut input = message("started", None, "");
    input.id.0 = "admission-message".into();
    input.seq = 1;
    input.user_shell.as_mut().unwrap().input = true;
    let result = message("completed", Some(0), "done");
    let page = |rows| HistoryPage {
        rows,
        total: 3,
        ..Default::default()
    };
    let ordinary = HistoryMessage {
        id: oc_core::session::MessageId("ordinary".into()),
        seq: 2,
        role: Role::User,
        text: "ordinary prompt".into(),
        turn: None,
        model_switch: None,
        user_shell: None,
    };
    let mut window = HistoryWindow::new();
    window.reset(&page(vec![input.clone()]));
    window.append_newer(&page(vec![result.clone()]));
    assert_eq!(window.rows()[0].role, "shell_input_delivered");
    assert_eq!(window.rows()[0].message_id.as_deref(), Some(&input.id));
    assert!(window.rows()[0].tool.is_none());
    assert_eq!(window.rows()[1].role, "shell");
    window.reset(&page(vec![input.clone(), ordinary.clone()]));
    window.refresh_completed(&page(vec![ordinary, result.clone()]), true);
    assert_eq!(
        window.rows()[0].role,
        "shell_input_delivered",
        "retained older input is reconciled"
    );
    window.reset(&page(vec![]));
    window.push_user_shell("actual-null-turn-operation".into(), "printf Ω界");
    window.push_synthetic("user", "new live prompt", None, None);
    assert!(window.refresh_user_shell(&page(vec![input, result.clone()])));
    assert_eq!(
        window
            .rows()
            .iter()
            .filter(|row| row.role == "shell")
            .count(),
        1
    );
    assert_eq!(
        window
            .rows()
            .iter()
            .filter(|row| row.role == "shell_input_delivered")
            .count(),
        1
    );
    assert!(
        window
            .rows()
            .iter()
            .any(|row| row.text == "new live prompt")
    );
    assert!(
        !window.refresh_user_shell(&page(vec![result])),
        "repeated receipt is idempotent"
    );
}

#[test]
fn direct_user_shell_detached_result_retires_input_without_changing_durable_anchors() {
    let mut input = message("started", None, "");
    input.id.0 = "retained-admission".into();
    input.seq = 1;
    input.user_shell.as_mut().unwrap().input = true;
    let mut window = HistoryWindow::new();
    window.reset(&HistoryPage {
        rows: vec![input.clone()],
        total: 3,
        has_newer: true,
        ..Default::default()
    });
    assert!(window.refresh_user_shell(&HistoryPage {
        rows: vec![message("completed", Some(0), "done")],
        total: 3,
        ..Default::default()
    }));
    assert_eq!(window.len(), 1);
    assert_eq!(window.rows()[0].role, "shell_input_delivered");
    assert_eq!(window.rows()[0].message_id.as_deref(), Some(&input.id));
    assert_eq!(window.rows()[0].seq, 1);
    assert!(window.has_newer());
    assert!(window.rows()[0].tool.is_none());
    let before = window.rows().to_vec();
    window.push_user_shell("actual-null-turn-operation".into(), "late receipt");
    assert_eq!(
        window.rows(),
        before,
        "a detached result also fences a later receipt"
    );
    assert!(window.has_newer(), "late receipt preserves the paging gap");
}

#[test]
fn direct_user_shell_refresh_never_reinserts_evicted_older_records_or_skips_a_gap() {
    let page = |rows, total| HistoryPage {
        rows,
        total,
        ..Default::default()
    };
    let records = (1..=WINDOW_ROWS + 1)
        .map(|seq| {
            let mut row = message("completed", Some(0), "done");
            row.id.0 = format!("result-{seq}");
            row.seq = seq as i64;
            row.user_shell.as_mut().unwrap().operation = format!("operation-{seq}");
            row
        })
        .collect::<Vec<_>>();
    let mut window = HistoryWindow::new();
    window.reset(&page(records.clone(), records.len()));
    assert_eq!(window.rows()[0].seq, 2);
    assert!(window.has_older());
    assert!(!window.refresh_user_shell(&page(records.clone(), records.len())));
    assert_eq!(
        window.rows()[0].seq,
        2,
        "evicted seq 1 is not appended after the tail"
    );
    window.prepend_older(&page(vec![records[0].clone()], records.len()));
    let ids = window
        .rows()
        .iter()
        .filter_map(|row| row.message_id.as_deref())
        .collect::<Vec<_>>();
    assert!(ids.iter().enumerate().all(|(i, id)| !ids[..i].contains(id)));
    assert!(
        window
            .rows()
            .windows(2)
            .all(|pair| pair[0].seq < pair[1].seq)
    );

    let mut next = message("completed", Some(0), "next");
    next.id.0 = "disjoint-new-result".into();
    next.seq = (WINDOW_ROWS + 10) as i64;
    next.user_shell.as_mut().unwrap().operation = "new-operation".into();
    // Even a tail reader cannot turn an exact/disjoint notification into an
    // allegedly contiguous page. It must use normal newer paging.
    window.reset(&page(vec![records.last().unwrap().clone()], records.len()));
    assert!(window.refresh_user_shell(&page(vec![next], records.len() + 1)));
    assert_eq!(window.len(), 1);
    assert!(window.has_newer());

    window.reset(&page(vec![records[0].clone()], records.len()));
    let before = window.rows().to_vec();
    window.push_user_shell("operation-1".into(), "late receipt");
    assert_eq!(
        window.rows(),
        before,
        "a late receipt cannot revive a completed spinner"
    );
}
