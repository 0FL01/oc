//! Presentation of owner snapshots; no provider work or persisted UI history.
use oc_core::compaction::{CompactionSnapshot, CompactionState};
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::{
    history::HistoryRow,
    styled::{Line, Span},
    theme::Theme,
};

pub(crate) const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(crate) fn active(snapshot: &CompactionSnapshot) -> bool {
    matches!(
        snapshot.state,
        CompactionState::Queued | CompactionState::Running
    )
}

fn number(value: u64) -> String {
    // Pinned util/locale.ts:19–25 preserves one fraction digit for K/M.
    if value < 1000 {
        return value.to_string();
    }
    let (scale, suffix) = if value < 1_000_000 {
        (1000.0, "K")
    } else {
        (1_000_000.0, "M")
    };
    format!("{:.1}{suffix}", value as f64 / scale)
}

pub(crate) fn row(snapshot: &CompactionSnapshot, frame: usize, animations: bool) -> HistoryRow {
    let mut label = match snapshot.state {
        CompactionState::Queued => "◇ Compaction queued".into(),
        CompactionState::Running => format!(
            "{} Compaction",
            if animations {
                FRAMES[frame % FRAMES.len()]
            } else {
                "⋯"
            }
        ),
        CompactionState::Failed => "✗ Compaction".into(),
        CompactionState::Cancelled => "Compaction · cancelled".into(),
        CompactionState::Completed if snapshot.provider_native => "Provider compaction".into(),
        CompactionState::Completed => "Compaction".into(),
    };
    if !matches!(
        snapshot.state,
        CompactionState::Queued | CompactionState::Running
    ) && let Some(usage) = &snapshot.usage
    {
        let input = usage
            .input_tokens
            .saturating_add(usage.cache_read_tokens)
            .saturating_add(usage.cache_write_tokens);
        let output = usage.output_tokens.saturating_add(usage.reasoning_tokens);
        if input > 0 || output > 0 {
            label.push_str(&format!(" · {} in · {} out", number(input), number(output)));
        }
    }
    HistoryRow {
        message_id: Some(std::sync::Arc::new(oc_core::session::MessageId(
            snapshot.id.clone(),
        ))),
        seq: i64::MAX,
        role: match snapshot.state {
            CompactionState::Failed => "compaction_failed",
            CompactionState::Queued => "compaction_queued",
            _ => "compaction",
        }
        .into(),
        text: match snapshot.state {
            CompactionState::Failed => snapshot.error.clone().unwrap_or_default(),
            CompactionState::Cancelled | CompactionState::Queued => String::new(),
            _ if snapshot.provider_native => String::new(),
            _ => snapshot.summary.clone(),
        },
        agent: Some(label),
        agent_color_index: None,
        chips: vec![],
        reasoning: None,
        meta: None,
        tool: None,
    }
}

pub(crate) fn block(row: &HistoryRow, theme: &Theme, width: u16) -> Vec<Line> {
    // CompactionMessage / CompactionQueued, pinned session/index.tsx:2084–2163.
    let color = if row.role == "compaction_failed" {
        theme.error()
    } else {
        theme.text_muted()
    };
    let style = Style::default().fg(color);
    let label = row.agent.as_deref().unwrap_or("Compaction");
    let available = usize::from(width).saturating_sub(label.width() + 2);
    let border = Style::default().fg(if row.role == "compaction_queued" {
        theme.border()
    } else {
        color
    });
    let mut divider = vec![
        Span::styled("─".repeat(available.div_ceil(2)), border),
        Span::plain(" "),
    ];
    // JSX gap={1} and padding cells inherit canvas foreground; they aren't
    // part of the neighboring text's muted/error foreground.
    let (title, detail) = label
        .split_once(" · ")
        .map_or((label, None), |(title, detail)| (title, Some(detail)));
    if let Some((icon, title)) = title.split_once(' ')
        && (FRAMES.contains(&icon) || matches!(icon, "⋯" | "◇" | "✗"))
    {
        divider.extend([
            Span::styled(icon, style),
            Span::plain(" "),
            Span::styled(title, style),
        ]);
    } else {
        divider.push(Span::styled(title, style));
    }
    if let Some(detail) = detail {
        divider.extend([Span::plain(" "), Span::styled(format!("· {detail}"), style)]);
    }
    divider.extend([
        Span::plain(" "),
        Span::styled("─".repeat(available / 2), border),
    ]);
    let mut lines = vec![Line::plain(""), Line::new(divider)];
    if !row.text.trim().is_empty() {
        lines.push(Line::plain(""));
        lines.extend(crate::messages::compaction_markdown_block(
            row.text.trim(),
            theme,
            width,
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{PanelIntent, TuiState},
        commands::{CommandAction, dispatch, spec},
        events::KeyAction,
    };
    use oc_core::{
        compaction::{CompactionAnchor, CompactionReason, CompactionUsage},
        core_app::{CoreApp, InboxMsg},
        domain::SessionId,
    };
    use std::time::{Duration, Instant};

    fn snapshot(state: CompactionState) -> CompactionSnapshot {
        CompactionSnapshot {
            anchor: CompactionAnchor::default(),
            id: "checkpoint".into(),
            session: "session".into(),
            reason: CompactionReason::Manual,
            state,
            summary: "## Retained\n\n**Actual** streamed summary".into(),
            usage: Some(CompactionUsage {
                input_tokens: 1000,
                output_tokens: 20,
                cache_read_tokens: 200,
                cache_write_tokens: 300,
                reasoning_tokens: 10,
            }),
            provider_native: false,
            error: None,
        }
    }

    #[test]
    fn vis34_summary_geometry_and_styles_match_pinned_completed_cells() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            style::Color,
            widgets::{Paragraph, Widget},
        };
        // Source: pinned session/index.tsx:2105–2144 (top-level streaming
        // Markdown, paddingTop=1, paddingLeft=3). Verify the complete component
        // rectangle using each immutable full-grid capture. This unit contract
        // does not assert whole-frame parity or alter capture comparisons.
        let captures = [
            include_str!(
                "../../../evidence/tui/recovery-v00/compaction20260927-12/upstream/compaction-completed.cells.json"
            ),
            include_str!(
                "../../../evidence/tui/recovery-v00/compaction20260927-13/upstream/compaction-threshold-completed.cells.json"
            ),
            include_str!(
                "../../../evidence/tui/recovery-v00/compaction20260927-14/upstream/compaction-overflow-completed.cells.json"
            ),
        ];
        let mut completed = snapshot(CompactionState::Completed);
        completed.summary = "## Objective\n- VIS34-CHECKPOINT: preserve R1, R2, R3.\n\n## Work State\n- Three seeded exchanges completed; filesystem unchanged.\n\n## Next Move\n1. Continue the user request without replaying tools.\n".into();
        completed.usage = Some(CompactionUsage {
            input_tokens: 1000,
            cache_read_tokens: 234,
            cache_write_tokens: 0,
            output_tokens: 300,
            reasoning_tokens: 21,
        });
        let theme = Theme::dark();
        let lines = block(&row(&completed, 0, false), theme, 116);
        assert_eq!(
            lines.len(),
            14,
            "divider, body padding, and all three heading/list gaps"
        );
        let area = Rect::new(0, 0, 116, lines.len() as u16);
        let mut buffer = Buffer::empty(area);
        buffer.set_style(
            area,
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .bg(theme.background()),
        );
        Paragraph::new(
            lines
                .into_iter()
                .map(Line::into_ratatui)
                .collect::<Vec<_>>(),
        )
        .render(area, &mut buffer);
        let hex = |color| match color {
            Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
            _ => panic!("expected truecolor"),
        };
        for capture in captures {
            let capture: serde_json::Value = serde_json::from_str(capture).unwrap();
            assert_eq!(capture["columns"], 120);
            assert_eq!(capture["rows"], 40);
            for y in 0..area.height {
                for x in 0..area.width {
                    let expected = &capture["cells"][usize::from(y) + 14][usize::from(x) + 2];
                    let actual = &buffer[(x, y)];
                    assert_eq!(
                        actual.symbol(),
                        expected["symbol"].as_str().unwrap(),
                        "{} x={x} y={y}",
                        capture["scenario"]
                    );
                    assert_eq!(
                        hex(actual.fg),
                        expected["fg"].as_str().unwrap(),
                        "fg x={x} y={y}"
                    );
                    assert_eq!(
                        hex(actual.bg),
                        expected["bg"].as_str().unwrap(),
                        "bg x={x} y={y}"
                    );
                    let modifiers = expected["modifiers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str().unwrap())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        actual.modifier.contains(ratatui::style::Modifier::BOLD),
                        modifiers.contains(&"bold"),
                        "bold x={x} y={y}"
                    );
                    assert!((actual.modifier - ratatui::style::Modifier::BOLD).is_empty());
                    assert!(modifiers.iter().all(|modifier| *modifier == "bold"));
                }
            }
        }
    }

    #[test]
    fn vis34_heading_spacing_is_scoped_and_preserves_real_markdown() {
        let theme = Theme::dark();
        let source =
            "## Heading\n- item\n\n## Second\n\nparagraph\n\n```md\n## literal\n- literal\n```";
        let text = |lines: Vec<Line>| {
            lines
                .iter()
                .map(|line| line.plain_text().trim_end().to_string())
                .collect::<Vec<_>>()
        };
        let ordinary = text(crate::messages::markdown_block(source, theme, 80));
        let compaction = text(crate::messages::compaction_markdown_block(
            source, theme, 80,
        ));
        assert_eq!(&ordinary[..2], &["   Heading", "   - item"]);
        assert_eq!(&compaction[..3], &["   Heading", "", "   - item"]);
        assert_eq!(
            compaction
                .iter()
                .filter(|line| line.as_str() == "   Second")
                .count(),
            1
        );
        assert!(
            compaction
                .windows(3)
                .any(|lines| lines == ["   Second", "", "   paragraph"]),
            "existing blank must not double"
        );
        assert!(
            compaction
                .windows(2)
                .any(|lines| lines == ["   ## literal", "   - literal"]),
            "fenced literal is not a heading"
        );
        assert_eq!(
            text(crate::messages::markdown_block(source, theme, 80)),
            ordinary
        );
        let streaming = "## Open heading";
        assert_eq!(
            text(crate::messages::compaction_markdown_block(
                streaming, theme, 80
            )),
            ["   Open heading"],
            "no trailing synthetic gap before a body exists"
        );
    }

    #[test]
    fn vis34_queued_and_failed_dividers_match_pinned_padding_rounding_and_foreground() {
        use ratatui::{
            buffer::Buffer,
            layout::Rect,
            style::Color,
            widgets::{Paragraph, Widget},
        };
        let captures = [
            (
                CompactionState::Queued,
                include_str!(
                    "../../../evidence/tui/recovery-v00/compaction20260927-12/upstream/compaction-queued.cells.json"
                ),
            ),
            (
                CompactionState::Failed,
                include_str!(
                    "../../../evidence/tui/recovery-v00/compaction20260927-12/upstream/compaction-failed.cells.json"
                ),
            ),
        ];
        for (state, capture) in captures {
            let mut owner = snapshot(state);
            owner.usage = None;
            owner.error = Some("owner diagnostic; preserve precisely".into());
            let projected = row(&owner, 0, false);
            if state == CompactionState::Failed {
                assert_eq!(projected.text, owner.error.unwrap());
            }
            let divider = block(&projected, Theme::dark(), 116).remove(1);
            let area = Rect::new(0, 0, 116, 1);
            let mut buffer = Buffer::empty(area);
            buffer.set_style(
                area,
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(Theme::dark().background()),
            );
            Paragraph::new(divider.into_ratatui()).render(area, &mut buffer);
            let capture: serde_json::Value = serde_json::from_str(capture).unwrap();
            for x in 0..116u16 {
                let expected = &capture["cells"][29][usize::from(x) + 2];
                let actual = &buffer[(x, 0)];
                assert_eq!(
                    actual.symbol(),
                    expected["symbol"].as_str().unwrap(),
                    "{state:?} x={x}"
                );
                let Color::Rgb(r, g, b) = actual.fg else {
                    panic!("truecolor")
                };
                assert_eq!(
                    format!("#{r:02x}{g:02x}{b:02x}"),
                    expected["fg"].as_str().unwrap(),
                    "{state:?} fg x={x}"
                );
            }
        }
    }

    #[tokio::test]
    async fn vis34_owner_lifecycle_clock_usage_and_two_escape_cancel() {
        let (app, mut inbox, _) = CoreApp::channel(8);
        let mut view = TuiState::new(app, SessionId("session".into()));
        assert_eq!(dispatch("/compact"), Some(CommandAction::CompactSession));
        assert_eq!(
            spec(&CommandAction::CompactSession).unavailable(true, false),
            None
        );
        assert!(matches!(
            dispatch("/dcp-compress focus"),
            Some(CommandAction::DcpCompress { .. })
        ));
        view.handle_paste("/compact");
        let revision = view.compaction_request_revision();
        assert_eq!(
            view.handle_key(KeyAction::Enter).await.intent,
            Some(PanelIntent::CompactSession)
        );
        assert_eq!(view.input(), "/compact", "keep the draft until acceptance");
        view.apply_compaction(snapshot(CompactionState::Running));
        view.compaction_admitted(snapshot(CompactionState::Queued), revision);
        assert_eq!(view.input(), "");
        let before = view.transcript_lines(80, 80);
        let at = view.next_ui_deadline().unwrap();
        assert!(at <= Instant::now() + Duration::from_millis(80));
        assert!(view.tick_ui(at));
        assert_ne!(before, view.transcript_lines(80, 80));
        assert!(view.is_busy());
        view.handle_key(KeyAction::Cancel).await;
        assert!(inbox.try_recv().is_err(), "first Esc only arms interrupt");
        let worker = tokio::spawn(async move {
            let Some(InboxMsg::CancelCompaction { session, ack }) = inbox.recv().await else {
                panic!("compaction cancellation")
            };
            assert_eq!(session.0, "session");
            ack.send(Ok(())).unwrap();
            assert!(
                inbox.try_recv().is_err(),
                "no turn or extra provider submission"
            );
        });
        view.handle_key(KeyAction::Cancel).await;
        worker.await.unwrap();
        view.apply_compaction(snapshot(CompactionState::Completed));
        let lines = view.transcript_lines(80, 80);
        let text = lines
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Compaction · 1.5K in · 30 out"), "{text}");
        assert!(text.contains("Retained") && text.contains("Actual"));
        assert!(!view.is_busy());
        // The two-Esc guard has been consumed; a completed job schedules nothing.
        assert!(view.next_ui_deadline().is_none());
        assert!(!view.tick_ui(at + Duration::from_secs(10)));
        view.apply_compaction(snapshot(CompactionState::Running));
        assert_eq!(
            view.transcript_lines(80, 80),
            lines,
            "late admission cannot revive completion"
        );
    }

    #[tokio::test]
    async fn vis34_disabled_animation_native_failure_cancel_and_owner_replay() {
        let (app, _, _) = CoreApp::channel(8);
        let mut view = TuiState::new(app, SessionId("session".into()));
        view.chrome.animations = Some(false);
        view.apply_compaction(snapshot(CompactionState::Running));
        let text = |view: &TuiState| {
            view.transcript_lines(80, 80)
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert!(text(&view).contains("⋯ Compaction"));
        assert!(view.next_ui_deadline().is_none());
        let mut native = snapshot(CompactionState::Completed);
        native.provider_native = true;
        view.apply_compaction_history(vec![native]);
        assert!(text(&view).contains("Provider compaction"));
        assert!(!text(&view).contains("Actual"));
        let mut failed = snapshot(CompactionState::Failed);
        failed.error = Some("Owner failure".into());
        view.apply_compaction(failed);
        assert!(text(&view).contains("✗ Compaction") && text(&view).contains("Owner failure"));
        view.apply_compaction(snapshot(CompactionState::Cancelled));
        assert!(text(&view).contains("Compaction · cancelled"));
        assert!(!text(&view).contains("Actual"));
        view.apply_compaction_history(vec![]);
        assert!(
            view.transcript_rows().is_empty(),
            "owner branch projection removes archived future checkpoints"
        );
    }

    #[test]
    fn vis34_causal_tool_message_turn_anchors_and_reopen_order() {
        use oc_core::{
            queries::{HistoryMessage, HistoryPage, HistoryTurn, ToolOpView, TranscriptPart},
            session::{MessageId, Role},
        };
        let (app, _, _) = CoreApp::channel(8);
        let mut view = TuiState::new(app, SessionId("session".into()));
        let page = HistoryPage {
            total: 3,
            rows: vec![
                HistoryMessage {
                    id: MessageId("user".into()),
                    seq: 1,
                    role: Role::User,
                    text: "question".into(),
                    turn: None,
                    model_switch: None,
                },
                HistoryMessage {
                    id: MessageId("answer".into()),
                    seq: 2,
                    role: Role::Assistant,
                    text: "final".into(),
                    model_switch: None,
                    turn: Some(HistoryTurn {
                        id: "turn".into(),
                        status: "completed".into(),
                        parts: vec![
                            TranscriptPart::Tool(ToolOpView {
                                question: None,
                                op: "tool".into(),
                                rowid: 1,
                                name: "read".into(),
                                state: "completed".into(),
                                input: None,
                                output: None,
                                output_bytes: 0,
                                output_truncated: false,
                                patch_effects: None,
                                dcp: None,
                                dcp_topic: None,
                            }),
                            TranscriptPart::Text("final".into()),
                        ],
                        ..Default::default()
                    }),
                },
                HistoryMessage {
                    id: MessageId("next".into()),
                    seq: 3,
                    role: Role::User,
                    text: "continue".into(),
                    turn: None,
                    model_switch: None,
                },
            ],
            ..Default::default()
        };
        view.attach_page(&page);
        let mut completed = snapshot(CompactionState::Completed);
        completed.anchor = CompactionAnchor {
            message: Some("answer".into()),
            turn: Some("turn".into()),
            tool: Some("tool".into()),
        };
        view.apply_compaction_history(vec![completed.clone()]);
        let rows = view.transcript_rows();
        let at = rows.iter().position(|r| r.role == "compaction").unwrap();
        assert!(rows[..at].iter().any(|r| r.text == "final"));
        assert_eq!(
            rows[at + 1].text,
            "continue",
            "after complete answer/footer, before next prompt"
        );
        let original = view.transcript_lines(80, 80);
        view.attach_page(&page);
        view.apply_compaction_history(vec![completed.clone()]);
        assert_eq!(view.transcript_lines(80, 80), original);
        let mut boundary = completed.clone();
        boundary.anchor.message = Some("user".into());
        view.apply_compaction_history(vec![boundary.clone()]);
        let rows = view.transcript_rows();
        let tool = rows
            .iter()
            .position(|r| r.tool.as_ref().is_some_and(|card| card.op == "tool"))
            .unwrap();
        assert_eq!(
            rows[tool + 1].role,
            "compaction",
            "safe tool boundary precedes later continuation, even after replay"
        );
        assert_eq!(rows[tool + 2].text, "final");
        let boundary_lines = view.transcript_lines(80, 80);
        view.attach_page(&page);
        view.apply_compaction_history(vec![boundary]);
        assert_eq!(view.transcript_lines(80, 80), boundary_lines);
        completed.anchor.message = None;
        completed.anchor.tool = None;
        view.apply_compaction_history(vec![completed]);
        assert_eq!(
            view.transcript_lines(80, 80),
            original,
            "turn-only anchor resolves through owner turn projection"
        );
        let older = snapshot(CompactionState::Completed);
        let mut newer = older.clone();
        newer.id = "newer-checkpoint".into();
        view.apply_compaction_history(vec![newer, older]);
        let checkpoints = view
            .transcript_rows()
            .into_iter()
            .filter(|r| r.role == "compaction")
            .map(|r| r.message_id.unwrap().0.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            checkpoints,
            ["checkpoint", "newer-checkpoint"],
            "newest-first query is rendered chronologically"
        );
    }
}
