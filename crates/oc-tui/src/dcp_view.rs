//! VIS38 compression presentation. Donor text/summary layout:
//! Opencode-DCP/opencode-dynamic-context-pruning 3.1.15,
//! `11f6517780a502512a3467645074be447cb0369e`, lib/ui/notification.ts
//! and lib/ui/utils.ts (AGPL-3.0-or-later; see repository DCP notices).
//! Native M units and rounded K promotion are owner-approved display extensions.
//! Counters and run identity are supplied by the runtime, never reconstructed here.

use crate::{styled::Line, theme::Theme};
use oc_core::dcp_view::{
    DcpDisplayConfig, DcpNotificationChannel, DcpNotificationMode, DcpRunSnapshot, DcpSummaryPage,
};
use ratatui::style::Color;

/// Per-range previews preserve all 32 possible range headings within 16 KiB.
pub const SUMMARY_PAGE_BYTES: usize = 512;
pub const SUMMARY_RUN_BYTES: usize = 16 * 1024;

/// Disposable presentation attached to the existing ToolCard identity. The
/// snapshot remains the owner's frozen data; summary pages are bounded previews.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcpRender {
    pub snapshot: Option<DcpRunSnapshot>,
    pub config: DcpDisplayConfig,
    pub summaries: Vec<DcpSummaryPage>,
    pub topic: Option<String>,
    pub color_index: Option<usize>,
    pub spinner: Option<String>,
}

impl DcpRender {
    pub(crate) fn apply_summary(&mut self, mut page: DcpSummaryPage) -> bool {
        let Some(run) = &self.snapshot else {
            return false;
        };
        if !run.block_ids.contains(&page.block_id)
            || self
                .summaries
                .iter()
                .any(|old| old.block_id == page.block_id)
        {
            return false;
        }
        let remaining = SUMMARY_RUN_BYTES.saturating_sub(
            self.summaries
                .iter()
                .map(|old| old.text.len())
                .sum::<usize>(),
        );
        let mut end = page.text.len().min(remaining.min(SUMMARY_PAGE_BYTES));
        while !page.text.is_char_boundary(end) {
            end -= 1;
        }
        if end < page.text.len() {
            page.text.truncate(end);
            page.next_offset = Some(end as i64);
        }
        self.summaries.push(page);
        self.summaries.sort_by_key(|page| {
            run.block_ids
                .iter()
                .position(|id| id == &page.block_id)
                .unwrap_or(usize::MAX)
        });
        true
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.topic.as_ref().map_or(0, String::len)
            + self.spinner.as_ref().map_or(0, String::len)
            + self.snapshot.as_ref().map_or(0, |run| {
                run.session.len()
                    + run.operation_id.len()
                    + run.topic.len()
                    + run.bar.len()
                    + run.block_ids.iter().map(String::len).sum::<usize>()
            })
            + self
                .summaries
                .iter()
                .map(|page| page.block_id.len() + page.topic.len() + page.text.len())
                .sum::<usize>()
    }
}

/// Format only a successful, owner-confirmed snapshot. It is never computed
/// from an output envelope, a loaded history length or process-global stats.
pub fn committed_text(
    run: &DcpRunSnapshot,
    config: &DcpDisplayConfig,
    summaries: &[DcpSummaryPage],
) -> String {
    let title = header(run.cumulative.gross_removed, run.cumulative.active_summary);
    if config.notification == DcpNotificationMode::Minimal {
        return format!("{title} — Compression #{}", run.ordinal);
    }
    let mut text = format!(
        "{title}\n\n│{}│\n▣ Compression #{} {}\n→ Topic: {}\n→ Items: {} messages",
        run.bar,
        run.ordinal,
        metrics(run.removed, run.summary),
        run.topic,
        run.new_messages
    );
    if run.new_tools > 0 {
        text.push_str(&format!(" and {} tools", run.new_tools));
    }
    text.push_str(" compressed");
    if config.show_compression {
        text.push_str(&format!(
            "\n→ Compression (~{} tokens): ",
            format_tokens(run.summary)
        ));
        if summaries.is_empty() {
            text.push_str("[summary preview unavailable]");
        } else {
            for (index, page) in summaries.iter().enumerate() {
                if index > 0 {
                    text.push_str("\n\n");
                }
                if run.block_ids.len() > 1 {
                    text.push_str(&format!("### {}\n", page.topic));
                }
                text.push_str(&page.text);
                if page.next_offset.is_some() {
                    text.push_str("\n… [summary preview limited]");
                }
            }
            if summaries.len() < run.block_ids.len() {
                text.push_str("\n… [additional range summaries unavailable]");
            }
        }
    }
    text
}

/// Chat confirmation is hidden for off/toast. Failure diagnostics stay visible
/// independently, including unavailable legacy outcomes and no-gain.
pub fn block(view: &DcpRender, state: &str, reason: &str, theme: &Theme, width: u16) -> Vec<Line> {
    let confirmed = state == "completed" && view.snapshot.is_some();
    if confirmed
        && (view.config.notification == DcpNotificationMode::Off
            || view.config.channel == DcpNotificationChannel::Toast)
    {
        return Vec::new();
    }
    let label = match (state, &view.snapshot) {
        ("completed", Some(run)) => committed_text(run, &view.config, &view.summaries),
        ("started" | "running", _) => {
            let mut label = format!(
                "{} DCP · Compressing…",
                view.spinner.as_deref().unwrap_or(crate::tools::SPINNER)
            );
            if let Some(topic) = &view.topic {
                label.push_str(&format!("\n→ Topic: {topic}"));
            }
            label
        }
        ("completed", None) => "▣ DCP · Compression metadata unavailable (legacy)".into(),
        ("no_gain", _) => "▣ DCP · No projection gain".into(),
        ("denied", _) => "▣ DCP · Compression denied".into(),
        ("cancelled", _) => "▣ DCP · Compression cancelled".into(),
        ("failed", _) => "▣ DCP · Compression failed".into(),
        _ => "▣ DCP · Compression outcome unknown".into(),
    };
    let mut text = label;
    if matches!(
        state,
        "failed" | "denied" | "cancelled" | "unknown" | "no_gain"
    ) && !reason.is_empty()
    {
        text.push('\n');
        text.push_str(reason);
    }
    let colors = theme.categorical_agents();
    let color = if matches!(state, "failed" | "unknown") {
        theme.error()
    } else {
        colors[view.color_index.unwrap_or(0) % colors.len()]
    };
    text_block(&text, theme, width, color)
}

/// Native toast mapping: one bounded transient status notice, no persistent card
/// copy or synthetic history message. Match the donor's first-line header.
pub fn toast_text(run: &DcpRunSnapshot, config: &DcpDisplayConfig) -> Option<String> {
    (config.notification != DcpNotificationMode::Off
        && config.channel == DcpNotificationChannel::Toast)
        .then(|| {
            format!(
                "{} — Compression #{}",
                header(run.cumulative.gross_removed, run.cumulative.active_summary),
                run.ordinal
            )
        })
}

/// Donor metrics omit a zero summary. Removed is always gross, supplied by the
/// accounting owner; this helper never substitutes the net-saved estimate.
pub fn metrics(removed: u64, summary: u64) -> String {
    let mut label = format!("-{} removed", format_tokens(removed));
    if summary > 0 {
        label.push_str(&format!(", +{} summary", format_tokens(summary)));
    }
    label
}

/// Shared cumulative header for card and transient notification labels.
pub fn header(removed: u64, active_summary: u64) -> String {
    format!("▣ DCP | {}", metrics(removed, active_summary))
}

fn text_block(text: &str, theme: &Theme, width: u16, color: Color) -> Vec<Line> {
    crate::messages::transcript_text_block(text, theme, width, color)
}

/// Decimal token label, integer half-up to one decimal, without float conversion
/// or an overflowing multiplication/addition. This is presentation only.
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    let (unit, suffix) = if tokens < 999_950 {
        (1_000, "K")
    } else {
        (1_000_000, "M")
    };
    // Round the remainder separately; even u64::MAX fits after division.
    let tenths = (tokens / unit) * 10 + ((tokens % unit) + unit / 20) / (unit / 10);
    if tenths.is_multiple_of(10) {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    }
}

#[cfg(test)]
pub(crate) fn fixture_run(session: &str, op: &str) -> DcpRunSnapshot {
    use oc_core::dcp_view::{DcpAccounting, DcpEstimateMethod};
    DcpRunSnapshot {
        session: session.into(),
        operation_id: op.into(),
        ordinal: 7,
        topic: "Investigated 中文 👩‍💻 e\u{301}".into(),
        block_ids: vec!["b7".into()],
        removed: 11_900,
        summary: 842,
        net_saved: 11_058,
        method: DcpEstimateMethod::Utf16RoundQuarterFallback,
        new_messages: 5,
        new_tools: 2,
        cumulative: DcpAccounting {
            gross_removed: 21_900,
            active_summary: 1_342,
            net_saved: 20_558,
            compressions: 7,
            prunes: 0,
            complete: true,
            ..Default::default()
        },
        bar: format!("{}{}{}", "░".repeat(10), "⣿".repeat(20), "█".repeat(20)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn token_labels_use_integer_half_up_and_promote_rounded_k() {
        for (raw, expected) in [
            (0, "0"),
            (842, "842"),
            (999, "999"),
            (1_000, "1K"),
            (1_049, "1K"),
            (1_050, "1.1K"),
            (11_900, "11.9K"),
            (999_949, "999.9K"),
            (999_950, "1M"),
            (1_000_000, "1M"),
            (1_049_999, "1M"),
            (1_050_000, "1.1M"),
            (4_218_800, "4.2M"),
            (u64::MAX, "18446744073709.6M"),
        ] {
            assert_eq!(format_tokens(raw), expected, "raw={raw}");
        }
    }

    #[test]
    fn donor_metrics_omit_zero_summary_and_share_number_labels() {
        assert_eq!(header(11_900, 842), "▣ DCP | -11.9K removed, +842 summary");
        assert_eq!(metrics(999_950, 0), "-1M removed");
    }

    #[test]
    fn detailed_fixture_is_one_frozen_run_with_distinct_cumulative_metrics() {
        let run = fixture_run("s", "op");
        assert_eq!(run.bar.chars().count(), 50);
        assert_eq!(
            committed_text(&run, &DcpDisplayConfig::default(), &[]),
            format!(
                "▣ DCP | -21.9K removed, +1.3K summary\n\n│{}│\n▣ Compression #7 -11.9K removed, +842 summary\n→ Topic: Investigated 中文 👩‍💻 e\u{301}\n→ Items: 5 messages and 2 tools compressed",
                run.bar
            )
        );
        let mut zero = run;
        zero.summary = 0;
        zero.new_tools = 0;
        zero.cumulative.active_summary = 0;
        let text = committed_text(&zero, &DcpDisplayConfig::default(), &[]);
        assert!(!text.contains("summary"));
        assert!(text.ends_with("5 messages compressed"));
        assert!(!text.contains("tools"));
    }

    #[test]
    fn modes_channels_and_running_never_fabricate_confirmation() {
        let run = fixture_run("s", "op");
        let mut view = DcpRender {
            snapshot: Some(run.clone()),
            ..Default::default()
        };
        view.config.notification = DcpNotificationMode::Minimal;
        view.config.show_compression = true;
        let minimal = committed_text(&run, &view.config, &[]);
        assert_eq!(
            minimal,
            "▣ DCP | -21.9K removed, +1.3K summary — Compression #7"
        );
        assert!(!minimal.contains('\n'));
        view.config.channel = DcpNotificationChannel::Toast;
        assert!(block(&view, "completed", "", Theme::dark(), 100).is_empty());
        assert_eq!(toast_text(&run, &view.config), Some(minimal));
        view.config.notification = DcpNotificationMode::Off;
        assert!(toast_text(&run, &view.config).is_none());
        assert!(block(&view, "completed", "", Theme::dark(), 100).is_empty());
        for state in ["failed", "denied", "cancelled", "unknown", "no_gain"] {
            let text = block(&view, state, "actual diagnostic", Theme::dark(), 100)
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("actual diagnostic"), "{state}");
            assert!(
                !text.contains("removed") && !text.contains("Compression #"),
                "{state}: {text}"
            );
        }
        view.snapshot = None;
        view.topic = Some("Actual 中文 topic".into());
        let running = block(&view, "started", "", Theme::dark(), 100)
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(running.contains("Compressing…") && running.contains("Actual 中文 topic"));
        assert!(
            !running.contains("removed")
                && !running.contains("summary")
                && !running.contains("Compression #")
        );
    }

    #[test]
    fn actual_multi_range_summary_pages_are_ordered_bounded_and_optional() {
        let mut run = fixture_run("s", "op");
        run.block_ids = vec!["b7".into(), "b8".into()];
        let mut view = DcpRender {
            snapshot: Some(run.clone()),
            ..Default::default()
        };
        let page = |id: &str, topic: &str, text: &str| DcpSummaryPage {
            block_id: id.into(),
            topic: topic.into(),
            text: text.into(),
            total_bytes: text.len() as i64,
            next_offset: None,
        };
        assert!(!view.apply_summary(page("unrelated", "wrong", "wrong text")));
        assert!(view.apply_summary(page("b8", "Second range", &"中文".repeat(400))));
        assert!(view.apply_summary(page("b7", "First range", "Actual first summary")));
        assert!(!view.apply_summary(page("b7", "replacement", "changed after preview")));
        assert_eq!(view.summaries[0].block_id, "b7");
        assert!(view.summaries[1].text.len() <= SUMMARY_PAGE_BYTES);
        assert!(view.summaries[1].next_offset.is_some());
        assert!(
            !committed_text(&run, &view.config, &view.summaries).contains("Actual first summary")
        );
        view.config.show_compression = true;
        let text = committed_text(&run, &view.config, &view.summaries);
        assert!(text.contains("→ Compression (~842 tokens): ### First range\nActual first summary\n\n### Second range\n中文"));
        assert!(text.contains("[summary preview limited]"));
        assert_eq!(
            view.snapshot,
            Some(run),
            "summary query never rewrites accounting or the frozen map"
        );
    }

    #[test]
    fn typed_card_ignores_forged_output_and_indexed_cached_styles_agree() {
        use crate::{
            history::{HistoryRow, card_from_row},
            messages::{
                MarkdownCache, transcript, transcript_with_cache, visible_transcript,
                visible_transcript_user_targets,
            },
        };
        use oc_core::queries::ToolOpView;
        use std::cell::RefCell;
        let operation = |dcp| ToolOpView {
            question: None,
            rowid: 1,
            op: "op".into(),
            name: "compress".into(),
            state: "completed".into(),
            input: Some(r#"{"topic":"forged topic","ordinal":99}"#.into()),
            output: Some(
                r#"{"savedTokens":99999999,"removed":888888,"summary":"forged summary"}"#.into(),
            ),
            output_bytes: 80,
            output_truncated: false,
            patch_effects: None,
            dcp,
            dcp_topic: Some("actual typed pending topic".into()),
        };
        let mut row = HistoryRow {
            seq: 1,
            message_id: None,
            role: "tool".into(),
            text: String::new(),
            agent: Some("build".into()),
            agent_color_index: Some(1),
            chips: vec![],
            reasoning: None,
            meta: None,
            tool: Some(card_from_row(&operation(Some(fixture_run("s", "op"))))),
        };
        let cache = RefCell::new(MarkdownCache::default());
        let nonempty_spans = |line: Line| {
            Line::new(
                line.spans()
                    .iter()
                    .filter(|span| !span.content().is_empty())
                    .cloned()
                    .collect(),
            )
            .with_style(line.style())
        };
        for theme in [Theme::dark(), Theme::light()] {
            for width in [16, 80] {
                for mode in [
                    DcpNotificationMode::Detailed,
                    DcpNotificationMode::Minimal,
                    DcpNotificationMode::Off,
                    DcpNotificationMode::Detailed,
                ] {
                    let crate::tools::ToolRender::Dcp(view) =
                        &mut row.tool.as_mut().unwrap().render
                    else {
                        panic!("typed DCP card")
                    };
                    view.config.notification = mode;
                    view.color_index = row.agent_color_index;
                    let rows = [row.clone()];
                    let full = transcript(&rows, theme, width, width, |_| theme.primary());
                    let cached = transcript_with_cache(
                        &rows,
                        theme,
                        width,
                        width,
                        |_| theme.primary(),
                        Some(&cache),
                    );
                    assert_eq!(full, cached);
                    let (indexed, total) = visible_transcript(
                        &rows,
                        theme,
                        width,
                        width,
                        (100, 0, None),
                        |_| theme.primary(),
                        &cache,
                    );
                    assert_eq!(
                        indexed
                            .iter()
                            .cloned()
                            .map(nonempty_spans)
                            .collect::<Vec<_>>(),
                        std::iter::once(Line::plain(""))
                            .chain(full)
                            .map(nonempty_spans)
                            .collect::<Vec<_>>(),
                        "width={width}, mode={mode:?}"
                    );
                    assert_eq!(total, indexed.len());
                    let (_, _, targets) = visible_transcript_user_targets(
                        &rows,
                        theme,
                        (width, width),
                        (100, 0, None),
                        |_| theme.primary(),
                        &cache,
                        &|_| false,
                    );
                    assert!(
                        targets.iter().all(Option::is_none),
                        "DCP wrapper is not a user prompt or hit target"
                    );
                    let text = indexed
                        .iter()
                        .map(Line::plain_text)
                        .collect::<Vec<_>>()
                        .join("\n");
                    assert!(!text.contains("forged"));
                    if mode == DcpNotificationMode::Off {
                        assert!(!text.contains('┃'));
                    }
                }
            }
        }
        let mut pending = operation(None);
        pending.state = "started".into();
        row.tool = Some(card_from_row(&pending));
        let replayed_pending =
            transcript(&[row.clone()], Theme::dark(), 100, 100, |_| Color::Reset)
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
        assert!(replayed_pending.contains("actual typed pending topic"));
        assert!(!replayed_pending.contains("forged") && !replayed_pending.contains("removed"));
        row.tool = Some(card_from_row(&operation(None)));
        let legacy = transcript(&[row], Theme::dark(), 100, 100, |_| Color::Reset)
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(legacy.contains("metadata unavailable (legacy)"));
        assert!(
            !legacy.contains("forged") && !legacy.contains("removed") && !legacy.contains("99M")
        );
    }

    #[test]
    fn real_user_wrapper_has_padding_raised_surface_and_unicode_wrap() {
        for theme in [Theme::dark(), Theme::light()] {
            let lines = text_block(
                "▣ DCP | -1K removed\n\n→ Topic: 中文 👩‍💻 e\u{301}",
                theme,
                20,
                theme.primary(),
            );
            assert_eq!(
                lines.first().unwrap().plain_text(),
                format!("┃{}", " ".repeat(19))
            );
            assert_eq!(
                lines.last().unwrap().plain_text(),
                format!("┃{}", " ".repeat(19))
            );
            for line in &lines {
                assert_eq!(line.plain_text().width(), 20);
                assert_eq!(line.style().bg, Some(theme.user_message_background()));
                assert_eq!(line.spans()[0].style().fg, Some(theme.primary()));
                for span in line.spans().iter().skip(2) {
                    if !span.content().trim().is_empty() {
                        assert_eq!(span.style().fg, Some(theme.text()));
                    }
                }
            }
            let joined = lines
                .iter()
                .map(Line::plain_text)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(joined.contains("┃  ▣ DCP"));
            assert!(joined.contains("👩‍💻"));
            assert!(joined.contains("e\u{301}"));
            let plain = text_block("a\n\nb", theme, 20, theme.primary());
            assert_eq!(plain.len(), 5, "padding + text + blank + text + padding");
        }
    }
}
