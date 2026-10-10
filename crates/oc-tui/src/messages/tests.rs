use super::*;
use crate::history::card_from_row;
use crate::styled;
use oc_core::queries::ToolOpView;
use ratatui::buffer::Buffer;
use ratatui::{
    Terminal,
    backend::TestBackend,
    widgets::{Block, Paragraph},
};

#[test]
fn retained_cache_index_counter_tracks_replacement_and_eviction() {
    let mut cache = MarkdownCache::default();
    let text = "# one\n".repeat(200);
    cache.pages((1, 0), &text, 60);
    let first = cache.index_bytes();
    assert!(first > 0);
    cache.pages((1, 0), &text, 60);
    assert_eq!(
        cache.index_bytes(),
        first,
        "cache hit must not accumulate bytes"
    );
    cache.pages((1, 0), "# shorter", 60);
    assert!(cache.index_bytes() < first, "revision replaces old index");
    for id in 2..40 {
        cache.pages((id, 0), "# item", 60);
    }
    assert_eq!(cache.indexes.len(), 32);
    let actual: usize = cache
        .indexes
        .iter()
        .map(|index| MarkdownCache::pages_bytes(&index.pages))
        .sum();
    assert_eq!(cache.index_bytes(), actual);
    assert_eq!(cache.retained_bytes(), cache.bytes + actual);
}

fn user(text: &str, chips: Vec<Chip>) -> HistoryRow {
    HistoryRow {
        message_id: None,
        seq: 1,
        role: "user".to_string(),
        text: text.to_string(),
        agent: Some("build".to_string()),
        agent_color_index: None,
        chips,
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: None,
    }
}

#[test]
fn vis10_user_hover_preserves_agent_border_and_chip_surfaces() {
    let theme = Theme::dark();
    let rows = user_block(
        &user(
            "text",
            vec![Chip {
                kind: ChipKind::Skill,
                name: "inspect".into(),
            }],
        ),
        theme,
        50,
        &|_| Color::Rgb(4, 5, 6),
    );
    let bg = theme.user_message_background();
    let hover = theme.decrease(bg);
    for row in rows {
        let changed = hover_user_content(&row, theme);
        assert_eq!(changed.spans()[0].style().fg, row.spans()[0].style().fg);
        assert_eq!(changed.spans()[0].style().bg, Some(bg));
        assert_eq!(changed.style().bg, Some(hover));
        for (before, after) in row.spans().iter().zip(changed.spans()) {
            assert_eq!(
                after.style().bg,
                if before.style().bg == Some(bg) && before.content() != "┃" {
                    Some(hover)
                } else {
                    before.style().bg
                }
            );
        }
    }
}

#[test]
fn vis10_indexed_user_targets_track_wrapped_pages_and_exclude_margins() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut first = user(&"repeat ".repeat(22), vec![]);
    first.seq = 12;
    first.message_id = Some(std::sync::Arc::new(oc_core::session::MessageId(
        "opaque-first".into(),
    )));
    let mut second = user(&"repeat ".repeat(22), vec![]);
    second.seq = 27;
    second.message_id = Some(std::sync::Arc::new(oc_core::session::MessageId(
        "opaque-second".into(),
    )));
    let rows = [first, assistant("gap"), second];
    let render = |scroll| {
        visible_transcript_user_targets(
            &rows,
            theme,
            (17, 40),
            (7, scroll, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        )
    };
    let (_, total, _) = render(0);
    let mut identities = Vec::new();
    for scroll in 0..total {
        let (lines, _, targets) = render(scroll);
        assert_eq!(lines.len(), targets.len());
        for (line, target) in lines.iter().zip(targets) {
            if let Some(target) = &target {
                assert!(matches!(target.seq, 12 | 27));
                assert_eq!(
                    target.message_id.as_ref().unwrap().0,
                    if target.seq == 12 {
                        "opaque-first"
                    } else {
                        "opaque-second"
                    }
                );
                assert_eq!(line.spans()[0].content(), "┃");
                identities.push(target.seq);
            }
            if line.plain_text().contains("gap") {
                assert!(target.is_none());
            }
        }
    }
    assert!(identities.contains(&12) && identities.contains(&27));
}

fn assistant(text: &str) -> HistoryRow {
    HistoryRow {
        message_id: None,
        seq: 2,
        role: "assistant".to_string(),
        text: text.to_string(),
        agent: Some("build".to_string()),
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: None,
    }
}

#[test]
fn adjacent_reasoning_group_matches_full_indexed_and_click_bounds() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let make = |seq, ordinal, text: &str, duration_ms, running, expanded| {
        let mut row = assistant("");
        row.seq = seq;
        row.reasoning = Some(ReasoningBlock {
            text: text.into(),
            duration_ms,
            running,
            expanded,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(seq, ordinal)),
        });
        row
    };
    for (running, expanded) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut rows = vec![
            make(
                42,
                0,
                "**Inspecting**\n\nfirst body",
                Some(5),
                false,
                expanded,
            ),
            make(
                42,
                1,
                "**Verifying**\n\nsecond body",
                Some(7),
                running,
                expanded,
            ),
            assistant("answer"),
        ];
        rows[2].seq = 42;
        for width in [12, 40, 80] {
            let full = transcript(&rows, theme, width, width, |_| Color::Reset);
            let expected = if expanded {
                "- Thought · 2 steps · 12ms"
            } else {
                "+ Thought: Verifying · 2 steps · 12ms"
            };
            if width == 80 {
                assert!(full[1].plain_text().contains(expected));
            }
            let plain: Vec<_> = full.iter().map(Line::plain_text).collect();
            assert_eq!(
                plain
                    .iter()
                    .filter(|s| s.contains("Thou") || s.contains("Think"))
                    .count(),
                1
            );
            if width == 80 {
                assert_eq!(
                    plain.iter().filter(|s| s.contains("first body")).count(),
                    usize::from(expanded)
                );
                assert_eq!(
                    plain.iter().filter(|s| s.contains("second body")).count(),
                    usize::from(expanded)
                );
            }
            let mut indexed_full = vec![Line::plain("")];
            indexed_full.extend(full.iter().cloned());
            for height in [2, 5, indexed_full.len() + 3] {
                for scroll in [0, 2, indexed_full.len() / 2, indexed_full.len()] {
                    let (visible, total) = visible_transcript_expanded(
                        &rows,
                        theme,
                        (width, width),
                        (height, scroll, None),
                        |_| Color::Reset,
                        &cache,
                        &|_| false,
                    );
                    assert_eq!(total, indexed_full.len());
                    let end = total - scroll.min(total.saturating_sub(height));
                    let start = end.saturating_sub(height);
                    assert_eq!(
                        visible.iter().map(Line::plain_text).collect::<Vec<_>>(),
                        indexed_full[start..end]
                            .iter()
                            .map(Line::plain_text)
                            .collect::<Vec<_>>(),
                        "width={width} height={height} scroll={scroll}"
                    );
                }
            }
            let id = Some(ReasoningIdentity::Durable(42, 0));
            assert_eq!(
                reasoning_header_at(
                    &rows,
                    theme,
                    (width, width),
                    (indexed_full.len(), 0, None),
                    |_| Color::Reset,
                    &cache,
                    (&|_| false, (3, 2))
                ),
                id
            );
            assert_eq!(
                reasoning_header_at(
                    &rows,
                    theme,
                    (width, width),
                    (indexed_full.len(), 0, None),
                    |_| Color::Reset,
                    &cache,
                    (&|_| false, (2, 2))
                ),
                None
            );
            if width == 80 {
                assert_eq!(
                    reasoning_header_at(
                        &rows,
                        theme,
                        (width, width),
                        (indexed_full.len(), 0, None),
                        |_| Color::Reset,
                        &cache,
                        (&|_| false, ((width - 1) as usize, 2))
                    ),
                    None
                );
            }
        }
    }
    let mut rows = vec![
        make(42, 0, "**First**\n\nA", Some(u64::MAX), false, false),
        make(42, 1, "**Last**\n\nB", Some(u64::MAX), false, false),
    ];
    let saturated = transcript(&rows, theme, 80, 80, |_| Color::Reset)[1].plain_text();
    assert!(saturated.contains("Thought: Last · 2 steps · "));
    rows[0].reasoning.as_mut().unwrap().duration_ms = None;
    rows[1].reasoning.as_mut().unwrap().duration_ms = None;
    rows[1].reasoning.as_mut().unwrap().text = "untitled body".into();
    assert_eq!(
        transcript(&rows, theme, 80, 80, |_| Color::Reset)[1].plain_text(),
        "   + Thought · 2 steps",
        "the completed untitled last part clears the earlier title"
    );
    for row in &mut rows {
        let reasoning = row.reasoning.as_mut().unwrap();
        reasoning.toggleable = false;
        reasoning.expanded = true;
    }
    let show: Vec<_> = transcript(&rows, theme, 80, 80, |_| Color::Reset)
        .iter()
        .map(Line::plain_text)
        .collect();
    assert_eq!(show.iter().filter(|s| s.contains("┃ Thought")).count(), 2);
    assert!(!show.iter().any(|s| s.contains("steps")));
    for row in &mut rows {
        let reasoning = row.reasoning.as_mut().unwrap();
        reasoning.toggleable = true;
        reasoning.expanded = false;
    }
    rows.insert(1, make(42, 3, "[REDACTED]", None, false, false));
    let bridged = transcript(&rows, theme, 80, 80, |_| Color::Reset);
    assert_eq!(bridged[1].plain_text(), "   + Thought · 2 steps");
    assert_eq!(bridged.len(), 2, "empty middle part adds no spacer or body");
    let (indexed, total) = visible_transcript_expanded(
        &rows,
        theme,
        (80, 80),
        (10, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(total, 3);
    assert_eq!(indexed[2].plain_text(), bridged[1].plain_text());
    assert_eq!(
        reasoning_header_at(
            &rows,
            theme,
            (80, 80),
            (10, 0, None),
            |_| Color::Reset,
            &cache,
            (&|_| false, (4, 2))
        ),
        Some(ReasoningIdentity::Durable(42, 0))
    );
    rows.remove(1);
    for separator in [
        assistant("text"),
        HistoryRow {
            role: "user".into(),
            ..assistant("user")
        },
        HistoryRow {
            meta: Some(AssistantMeta {
                model: Some("model".into()),
                ..Default::default()
            }),
            ..assistant("")
        },
        HistoryRow {
            role: "tool".into(),
            ..assistant("tool")
        },
    ] {
        rows.insert(1, separator);
        let plain: Vec<_> = transcript(&rows, theme, 80, 80, |_| Color::Reset)
            .iter()
            .map(Line::plain_text)
            .collect();
        assert_eq!(plain.iter().filter(|s| s.contains("+ Thought")).count(), 2);
        assert!(!plain.iter().any(|s| s.contains("2 steps")));
        rows.remove(1);
    }
}

#[test]
fn expanded_adjacent_reasoning_caps_each_body_without_losing_the_following_text() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let make = |seq, marker: &str| {
        let mut row = assistant("");
        row.seq = seq;
        row.reasoning = Some(ReasoningBlock {
            text: format!("{marker}\n{}", "word ".repeat(5000)),
            duration_ms: None,
            running: false,
            expanded: true,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(seq, 0)),
        });
        row
    };
    let rows = [
        make(31, "first sentinel"),
        make(32, "second sentinel"),
        assistant("answer sentinel"),
    ];
    let full: Vec<_> = transcript(&rows, theme, 40, 40, |_| Color::Reset)
        .iter()
        .map(Line::plain_text)
        .collect();
    assert_eq!(
        full.iter().filter(|s| s.contains("first sentinel")).count(),
        1
    );
    assert_eq!(
        full.iter()
            .filter(|s| s.contains("second sentinel"))
            .count(),
        1
    );
    assert_eq!(
        full.iter()
            .filter(|s| s.contains("reasoning preview limited"))
            .count(),
        2
    );
    let (tail, total) = visible_transcript_expanded(
        &rows,
        theme,
        (40, 40),
        (5, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(total, full.len() + 1);
    assert!(
        tail.iter()
            .any(|line| line.plain_text().contains("answer sentinel"))
    );
    assert!(cache.borrow().retained_bytes() <= MAX_CACHED_BYTES + MAX_INDEX_BYTES);
}

#[test]
fn hidden_running_reasoning_keeps_adjacent_group_open_without_showing_a_redacted_step() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let make = |text: &str, running| {
        let mut row = assistant("");
        row.reasoning = Some(ReasoningBlock {
            text: text.into(),
            duration_ms: None,
            running,
            expanded: false,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(12, 0)),
        });
        row
    };
    let mut rows = vec![
        make("**Inspecting**\n\nfirst", false),
        make("**Verifying**\n\nsecond", false),
        make("[REDACTED]", true),
    ];
    let header =
        |rows: &[HistoryRow]| transcript(rows, theme, 80, 80, |_| Color::Reset)[1].plain_text();
    assert_eq!(header(&rows), "   ⋯ Thinking: Verifying");
    let (visible, _) = visible_transcript_expanded(
        &rows,
        theme,
        (80, 80),
        (10, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(visible[2].plain_text(), header(&rows));
    let mut started_footer = assistant("");
    started_footer.meta = Some(AssistantMeta {
        status: Some("started".into()),
        ..Default::default()
    });
    rows.push(started_footer);
    assert_eq!(header(&rows), "   ⋯ Thinking: Verifying");
    rows.pop();
    rows[2].reasoning.as_mut().unwrap().running = false;
    assert_eq!(header(&rows), "   + Thought: Verifying · 2 steps");
    rows[2].reasoning.as_mut().unwrap().running = true;
    rows[1].reasoning.as_mut().unwrap().text = "untitled last body".into();
    assert_eq!(header(&rows), "   ⋯ Thinking");
    rows.push(assistant("answer"));
    assert_eq!(header(&rows), "   + Thought · 2 steps");
    rows.remove(1);
    rows.pop();
    assert_eq!(header(&rows), "   ⋯ Thinking: Inspecting");
    rows[1].reasoning.as_mut().unwrap().running = false;
    assert_eq!(header(&rows), "   + Thought: Inspecting");
}

#[test]
fn offscreen_expanded_reasoning_is_measured_once_per_cache_miss() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut rows = Vec::new();
    for group in 0..4 {
        for ordinal in 0..2 {
            let mut row = assistant("");
            row.seq = group;
            row.reasoning = Some(ReasoningBlock {
                text: format!("**Group {group}**\n\n{}", "word ".repeat(1300)),
                duration_ms: Some(10),
                running: false,
                expanded: true,
                toggleable: true,
                identity: Some(ReasoningIdentity::Durable(group, ordinal)),
            });
            rows.push(row);
        }
        rows.push(assistant("answer"));
    }
    rows.push(assistant("trailing answer"));
    REASONING_BODY_RENDERS.set(0);
    let expected = rows.iter().filter(|row| row.reasoning.is_some()).count();
    for _ in 0..3 {
        let (tail, total) = visible_transcript_expanded(
            &rows,
            theme,
            (40, 40),
            (3, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert!(total > 100);
        assert!(tail.iter().any(|line| line.plain_text().contains("answer")));
        assert_eq!(REASONING_BODY_RENDERS.get(), expected);
    }
    let (_, total) = visible_transcript_expanded(
        &rows,
        theme,
        (40, 40),
        (6, usize::MAX, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert!(total > 100);
    assert!(REASONING_BODY_RENDERS.get() > 0);
    let painted = REASONING_BODY_RENDERS.get();
    visible_transcript_expanded(
        &rows,
        theme,
        (40, 40),
        (3, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(REASONING_BODY_RENDERS.get(), painted);
    assert!(cache.borrow().reasoning_heights.len() <= MAX_REASONING_HEIGHTS);
}

#[test]
fn reachable_reasoning_parts_keep_heights_across_frames_and_bounded_tabs() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut rows = Vec::new();
    // Full history + retained live parts + one open row, all in one group.
    for index in 0..MAX_REASONING_HEIGHTS {
        let mut row = assistant("");
        row.seq = index as i64;
        row.reasoning = Some(ReasoningBlock {
            text: format!("**Step {index}**\n\nbody {index}"),
            duration_ms: None,
            running: false,
            expanded: true,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(row.seq, 0)),
        });
        rows.push(row);
    }
    assert!(rows.len() > 240);
    rows.push(assistant("final answer"));
    let mut full = vec![Line::plain("")];
    full.extend(transcript(&rows, theme, 50, 50, |_| Color::Reset));
    let full: Vec<_> = full.iter().map(Line::plain_text).collect();

    REASONING_BODY_RENDERS.set(0);
    for frame in 0..2 {
        let (tail, total) = visible_transcript_expanded(
            &rows,
            theme,
            (50, 50),
            (1, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(total, full.len());
        assert_eq!(
            tail.iter().map(Line::plain_text).collect::<Vec<_>>(),
            full[full.len() - 1..]
        );
        // Scrolling to the group header must preserve both the indexed
        // viewport slice and the click target on every frame.
        let (top, top_total) = visible_transcript_expanded(
            &rows,
            theme,
            (50, 50),
            (3, usize::MAX, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(top_total, full.len());
        assert_eq!(
            top.iter().map(Line::plain_text).collect::<Vec<_>>(),
            full[..3]
        );
        assert_eq!(
            reasoning_header_at(
                &rows,
                theme,
                (50, 50),
                (3, usize::MAX, None),
                |_| Color::Reset,
                &cache,
                (&|_| false, (3, 2))
            ),
            Some(ReasoningIdentity::Durable(0, 0))
        );
        assert_eq!(
            REASONING_BODY_RENDERS.get(),
            MAX_REASONING_HEIGHTS,
            "unchanged frame {frame} must reuse every measured height"
        );
        assert_eq!(
            cache.borrow().reasoning_heights.len(),
            MAX_REASONING_HEIGHTS
        );
    }

    rows[crate::history::WINDOW_ROWS]
        .reasoning
        .as_mut()
        .unwrap()
        .text
        .push('!');
    visible_transcript_expanded(
        &rows,
        theme,
        (50, 50),
        (1, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(REASONING_BODY_RENDERS.get(), MAX_REASONING_HEIGHTS + 1);
    assert_eq!(
        cache.borrow().reasoning_heights.len(),
        MAX_REASONING_HEIGHTS
    );

    // Simulate visiting other tabs and widths in the same cache. Revisions
    // replace their part rather than accumulating alongside old entries.
    for tab in 1..=3 {
        for index in 0..MAX_REASONING_HEIGHTS {
            let mut cache = cache.borrow_mut();
            let part = (tab as i64 * 10_000 + index as i64, index);
            cache.set_reasoning_height(part, 1, 50, 4);
            cache.set_reasoning_height(part, 2, 40, 5);
            assert!(cache.reasoning_heights.len() <= MAX_REASONING_HEIGHTS);
        }
    }
    let cache = cache.borrow();
    assert_eq!(cache.reasoning_heights.len(), MAX_REASONING_HEIGHTS);
    assert_eq!(
        cache.retained_bytes(),
        cache.bytes
            + cache.index_bytes()
            + cache.reasoning_heights.capacity() * std::mem::size_of::<ReasoningHeight>()
    );
}

#[test]
fn offscreen_markdown_group_keeps_exact_slices_and_click_after_table_and_fence() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let make = |seq, ordinal, text: &str| {
        let mut row = assistant("");
        row.seq = seq;
        row.reasoning = Some(ReasoningBlock {
            text: text.into(),
            duration_ms: None,
            running: false,
            expanded: true,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(seq, ordinal)),
        });
        row
    };
    let mut rows = vec![
        make(
            10,
            0,
            "**Table**\n\n| Name | Detail |\n| --- | --- |\n| 東京🧪 | wide Unicode wraps into many cells |\n| alpha | several words across the narrow column |",
        ),
        make(
            10,
            1,
            "**Fence**\n\n```rust\nlet 名前 = \"🦀🦀🦀🦀🦀\";\nprintln!(\"{名前}\");\n```",
        ),
        assistant("between groups"),
        make(20, 0, "**Later**\n\na short step"),
        make(20, 1, "**Done**\n\nlast step"),
        assistant("trailing answer"),
    ];
    // The group state comes from its first part; later parts need not
    // carry the same expanded flag in the durable projection.
    rows[1].reasoning.as_mut().unwrap().expanded = false;
    REASONING_BODY_RENDERS.set(0);
    for width in [18, 27] {
        let mut full = vec![Line::plain("")];
        full.extend(transcript(&rows, theme, width, width, |_| Color::Reset));
        let full: Vec<String> = full.iter().map(Line::plain_text).collect();
        let second_header = full
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains("Thought"))
            .nth(1)
            .expect("later group header")
            .0;
        let height = 2;
        let scroll_to_header = full.len() - (second_header + 1);
        let start = second_header + 1 - height;
        assert!(
            start
                > full
                    .iter()
                    .position(|line| line.contains("between groups"))
                    .unwrap()
        );
        let renders_before = REASONING_BODY_RENDERS.get();
        // First indexed encounter measures even the bodies above this viewport.
        let (visible, total) = visible_transcript_expanded(
            &rows,
            theme,
            (width, width),
            (height, scroll_to_header, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(total, full.len(), "width={width}");
        assert_eq!(
            visible.iter().map(Line::plain_text).collect::<Vec<_>>(),
            full[start..second_header + 1],
            "width={width} header viewport"
        );
        assert_eq!(REASONING_BODY_RENDERS.get() - renders_before, 4);
        for _ in 0..2 {
            assert_eq!(
                reasoning_header_at(
                    &rows,
                    theme,
                    (width, width),
                    (height, scroll_to_header, None),
                    |_| Color::Reset,
                    &cache,
                    (&|_| false, (3, height - 1))
                ),
                Some(ReasoningIdentity::Durable(20, 0))
            );
            assert_eq!(REASONING_BODY_RENDERS.get() - renders_before, 4);
        }
        for (viewport_height, scroll) in [
            (4, 0),
            (5, full.len() / 2),
            (6, full.len()),
            (full.len(), 0),
        ] {
            let (visible, total) = visible_transcript_expanded(
                &rows,
                theme,
                (width, width),
                (viewport_height, scroll, None),
                |_| Color::Reset,
                &cache,
                &|_| false,
            );
            let end = full.len() - scroll.min(full.len().saturating_sub(viewport_height));
            let start = end.saturating_sub(viewport_height);
            assert_eq!(total, full.len(), "width={width} scroll={scroll}");
            assert_eq!(
                visible.iter().map(Line::plain_text).collect::<Vec<_>>(),
                full[start..end],
                "width={width} scroll={scroll}"
            );
        }
    }
    rows[0]
        .reasoning
        .as_mut()
        .unwrap()
        .text
        .push_str("\n\nrevision changed");
    let before = REASONING_BODY_RENDERS.get();
    visible_transcript_expanded(
        &rows,
        theme,
        (27, 27),
        (1, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert_eq!(REASONING_BODY_RENDERS.get() - before, 1);
}

#[test]
fn reasoning_index_hits_only_clipped_painted_cells_at_scroll() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let make = |ordinal, text: &str, running| {
        let mut row = assistant("");
        row.seq = 42;
        row.reasoning = Some(ReasoningBlock {
            text: text.into(),
            duration_ms: None,
            running,
            expanded: false,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(42, ordinal)),
        });
        row
    };
    let rows = vec![
        make(0, "**A long wrapped title**\n\nbody", false),
        assistant("between"),
        make(2, "second", false),
        assistant("between again"),
        make(3, "streaming", true),
    ];
    let hit = |width, height, scroll, x, y| {
        reasoning_header_at(
            &rows,
            theme,
            (width, width),
            (height, scroll, None),
            |_| Color::Reset,
            &cache,
            (&|_| false, (x, y)),
        )
    };
    let (lines, total) = visible_transcript_expanded(
        &rows,
        theme,
        (12, 12),
        (20, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.plain_text().contains("wrapped"))
    );
    assert_eq!(
        lines[2].plain_text(),
        "   + Thought",
        "completed text clips at 12 cells instead of wrapping"
    );
    assert_eq!(total, lines.len());
    assert_eq!(hit(12, 20, 0, 3, 1), None, "spacer");
    assert_eq!(hit(12, 20, 0, 2, 2), None, "padding");
    assert_eq!(
        hit(12, 20, 0, 3, 2),
        Some(ReasoningIdentity::Durable(42, 0))
    );
    assert_eq!(hit(12, 20, 0, 1, 3), None, "no wrapped continuation");
    let second = lines
        .iter()
        .rposition(|line| line.plain_text().contains("Thought"))
        .unwrap();
    assert_eq!(
        hit(12, 20, 0, 4, second),
        Some(ReasoningIdentity::Durable(42, 2))
    );
    let (wide, _) = visible_transcript_expanded(
        &rows,
        theme,
        (40, 40),
        (20, 0, None),
        |_| Color::Reset,
        &cache,
        &|_| false,
    );
    let wide_second = wide
        .iter()
        .rposition(|line| line.plain_text().contains("Thought"))
        .unwrap();
    assert_eq!(hit(40, 20, 0, 39, wide_second), None, "blank tail");
    let running = lines
        .iter()
        .position(|line| line.plain_text().contains('⋯'))
        .unwrap();
    assert_eq!(
        hit(12, 20, 0, 4, running),
        Some(ReasoningIdentity::Durable(42, 3)),
        "running header owns clicks in hide mode"
    );
    assert_eq!(hit(12, 4, 0, 3, 2), None, "off-screen first header");

    let mut show = rows[2].clone();
    show.reasoning.as_mut().unwrap().toggleable = false;
    show.reasoning.as_mut().unwrap().expanded = true;
    assert_eq!(
        clipped_reasoning_header(show.reasoning.as_ref().unwrap(), theme, 12).plain_text(),
        "   ┃ Thought"
    );
    assert_eq!(
        reasoning_header_at(
            &[show],
            theme,
            (12, 12),
            (4, 0, None),
            |_| Color::Reset,
            &cache,
            (&|_| false, (4, 2))
        ),
        None,
        "show header has no toggle"
    );

    let wide = make(4, "**界界**\n\nbody", false);
    let clipped = clipped_reasoning_header(wide.reasoning.as_ref().unwrap(), theme, 17);
    assert_eq!(clipped.plain_text(), "   + Thought: 界");
    assert_eq!(UnicodeWidthStr::width(clipped.plain_text().as_str()), 16);
    assert_eq!(
        reasoning_header_at(
            &[wide],
            theme,
            (17, 17),
            (4, 0, None),
            |_| Color::Reset,
            &cache,
            (&|_| false, (16, 2))
        ),
        None,
        "half a wide glyph is not a hit"
    );
}

fn exploration_tool(name: &str, state: &str, truncated: bool) -> HistoryRow {
    let input = match name {
        "read" => serde_json::json!({"path": "fixture-note.txt"}),
        _ => serde_json::json!({"pattern": "*.rs"}),
    };
    let card = card_from_row(&ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        rowid: 1,
        op: name.to_string(),
        name: name.to_string(),
        state: state.to_string(),
        input: Some(input.to_string()),
        output: Some("fixture result".to_string()),
        output_bytes: 14,
        output_truncated: truncated,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    });
    HistoryRow {
        message_id: None,
        seq: 3,
        role: "tool".to_string(),
        text: String::new(),
        agent: Some("build".to_string()),
        agent_color_index: None,
        chips: Vec::new(),
        reasoning: None,
        child_notice: None,
        shell_notice: None,
        meta: None,
        tool: Some(card),
    }
}

#[test]
fn completed_exploration_is_grouped_in_full_and_visible_transcript() {
    // The pinned original's completed read fixture shows exactly one
    // collapsed `→ Explored — 1 read` row (index.tsx:1865-1929).
    let theme = Theme::dark();
    let rows = [
        exploration_tool("read", "completed", false),
        exploration_tool("glob", "completed", false),
        exploration_tool("grep", "completed", false),
        assistant("After tools"),
        exploration_tool("read", "completed", false),
    ];
    let (full, buffer) = render(&rows, 80, 12);
    assert_eq!(full[1], "   → Explored — 1 read, 2 searches");
    assert_eq!(full[5], "   → Explored — 1 read");
    assert_eq!(buffer[(3, 1)].fg, theme.text_muted());
    assert!(!full.join("\n").contains("Loaded fixture-note.txt"));

    let cache = RefCell::new(MarkdownCache::default());
    let (visible, _) = visible_transcript(
        &rows,
        theme,
        80,
        80,
        (12, 0, None),
        |_| theme.categorical_agents()[0],
        &cache,
    );
    let visible = visible
        .iter()
        .map(|line| {
            line.spans()
                .iter()
                .map(|span| span.content())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert!(
        visible
            .iter()
            .any(|line| line == "   → Explored — 1 read, 2 searches")
    );
    assert!(visible.iter().any(|line| line == "   → Explored — 1 read"));
    assert!(
        !visible
            .iter()
            .any(|line| line.contains("Loaded fixture-note.txt"))
    );
}

#[test]
fn expanded_exploration_keeps_header_and_shows_each_card_in_both_renderers() {
    let theme = Theme::dark();
    let rows = [
        exploration_tool("read", "completed", false),
        exploration_tool("glob", "completed", false),
        exploration_tool("grep", "completed", false),
        assistant("After tools"),
        exploration_tool("read", "failed", false),
        exploration_tool("read", "completed", true),
    ];
    let cache = RefCell::new(MarkdownCache::default());
    for width in [16, 80] {
        let full = transcript_with_expansion(
            &rows,
            theme,
            width,
            width,
            |_| theme.text(),
            Some(&cache),
            &|op| op == "read",
        );
        let (visible, total) = visible_transcript_expanded(
            &rows,
            theme,
            (width, width),
            (100, 0, None),
            |_| theme.text(),
            &cache,
            &|op| op == "read",
        );
        let full = styled::wrap_lines(&full, width as usize)
            .into_iter()
            .map(|line| line.plain_text())
            .collect::<Vec<_>>();
        let visible = visible
            .into_iter()
            .map(|line| line.plain_text())
            .collect::<Vec<_>>();
        assert_eq!(
            visible,
            std::iter::once(String::new())
                .chain(full.iter().cloned())
                .collect::<Vec<_>>()
        );
        assert_eq!(total, visible.len());
        let text = visible.join("\n");
        if width == 80 {
            assert!(text.contains("Explored — 1 read, 2 searches"));
            assert!(text.contains("→ Read fixture-note.txt"));
            assert_eq!(
                text.matches("→ Read fixture-note.txt").count(),
                1,
                "expanded group shows its read once; later failed/truncated rows stay separate"
            );
            assert!(!text.contains("[output preview truncated; full result retained]"));
            assert!(
                rows.iter()
                    .any(|row| row.tool.as_ref().is_some_and(|card| card.preview_limited()))
            );
        }
    }
}

#[test]
fn exploration_does_not_hide_unresolved_failed_or_truncated_results() {
    let rows = [
        exploration_tool("read", "completed", false),
        exploration_tool("read", "unknown", false),
        exploration_tool("read", "failed", false),
        exploration_tool("read", "denied", false),
        exploration_tool("read", "completed", true),
    ];
    let (full, _) = render(&rows, 80, 20);
    let text = full.join("\n");
    assert!(text.contains("→ Explored — 1 read"));
    assert!(text.contains("[outcome unknown]"));
    assert_eq!(
        text.matches("fixture result").count(),
        3,
        "failed, denied and unknown detail rows"
    );
    assert!(text.contains("fixture result"));
    assert!(!text.contains("[output preview truncated; full result retained]"));
    assert!(
        rows.last()
            .unwrap()
            .tool
            .as_ref()
            .unwrap()
            .preview_limited()
    );
}

#[test]
fn exploration_remains_running_until_every_grouped_operation_finishes() {
    let rows = [
        exploration_tool("read", "completed", false),
        exploration_tool("glob", "started", false),
    ];
    let (live, _) = render(&rows, 80, 5);
    assert_eq!(live[1], "   ⋯ Exploring — 1 read, 1 search");
    let (done, _) = render(
        &[
            exploration_tool("read", "completed", false),
            exploration_tool("glob", "completed", false),
        ],
        80,
        5,
    );
    assert_eq!(done[1], "   → Explored — 1 read, 1 search");
}

/// Render the transcript into a `width x height` buffer and return the
/// trimmed row texts plus the raw buffer for color assertions.
fn render(rows: &[HistoryRow], width: u16, height: u16) -> (Vec<String>, Buffer) {
    let theme = Theme::dark();
    let lines = transcript(rows, theme, width, width, |_| theme.categorical_agents()[0]);
    let wrapped = styled::wrap_lines(&lines, width as usize);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("backend");
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(styled::Lines::from(wrapped).into_text()),
                frame.area(),
            );
        })
        .expect("draw");
    let buffer = terminal.backend().buffer().clone();
    let rows_text = (0..height)
        .map(|y| {
            let mut row = String::new();
            for x in 0..width {
                row.push_str(buffer[(x, y)].symbol());
            }
            row.trim_end().to_string()
        })
        .collect();
    (rows_text, buffer)
}

#[test]
fn model_switch_notice_uses_pinned_row_margin_padding_and_muted_text() {
    let theme = Theme::dark();
    let mut notice = assistant("");
    notice.role = "model_switch".into();
    notice.text = "Switched model to Catalog Name".into();
    let (rows, painted) = render(&[notice.clone()], 80, 4);
    assert_eq!(rows[0], "");
    assert_eq!(rows[1], "   Switched model to Catalog Name");
    assert_eq!(painted[(3, 1)].fg, theme.text_muted());

    let cache = RefCell::new(MarkdownCache::default());
    let (indexed, total) = visible_transcript(
        &[notice],
        theme,
        80,
        80,
        (4, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert_eq!(total, 3);
    let indexed_notice = indexed.last().unwrap();
    assert_eq!(indexed_notice.plain_text(), rows[1]);
    assert_eq!(
        indexed_notice.spans()[1].style().fg,
        Some(theme.text_muted())
    );
}

/// Upstream user block (`routes/session/index.tsx:2298-2398`): `┃` border
/// in the agent color on `background.raised.base`, 1/2 padding, then the
/// skill/file chips on the accent and `decrease(raised.base)` surfaces.
#[test]
fn golden_user_message_with_skill_and_file_chips() {
    let row = user(
        "hello world",
        vec![
            Chip {
                kind: ChipKind::Skill,
                name: "review".to_string(),
            },
            Chip {
                kind: ChipKind::File,
                name: "src/main.rs".to_string(),
            },
        ],
    );
    let (rows, buffer) = render(&[row], 60, 8);
    assert_eq!(
        rows,
        vec![
            "┃".to_string(),
            "┃  hello world".to_string(),
            "┃".to_string(),
            "┃   skill  review   file  src/main.rs".to_string(),
            "┃".to_string(),
            String::new(),
            String::new(),
            String::new(),
        ]
    );

    let theme = Theme::dark();
    let agent = theme.categorical_agents()[0];
    // Border carries the agent color and the raised background; the body
    // row's trailing cells keep the raised background too.
    assert_eq!(buffer[(0, 1)].fg, agent);
    assert_eq!(buffer[(0, 1)].bg, theme.user_message_background());
    assert_eq!(buffer[(59, 1)].bg, theme.user_message_background());
    assert_eq!(buffer[(3, 1)].fg, theme.text());
    // Chip label: accent background, raised foreground, bold.
    assert_eq!(buffer[(4, 3)].symbol(), "s");
    assert_eq!(buffer[(4, 3)].bg, theme.accent_chip_background());
    assert_eq!(buffer[(4, 3)].fg, theme.user_message_background());
    assert!(
        buffer[(4, 3)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD)
    );
    // Chip name: `decrease(background.raised.base)` with muted text.
    assert_eq!(buffer[(11, 3)].symbol(), "r");
    assert_eq!(
        buffer[(11, 3)].bg,
        theme.decrease(theme.user_message_background())
    );
    assert_eq!(buffer[(11, 3)].fg, theme.text_muted());
    assert_eq!(buffer[(20, 3)].symbol(), "f");
    // Chips are separate rows, so nothing is invented when absent.
    let (rows, _) = render(&[user("plain", Vec::new())], 60, 4);
    assert_eq!(rows, vec!["┃", "┃  plain", "┃", ""]);
}

#[test]
fn user_message_prefers_the_turns_pinned_agent_color() {
    let mut row = user("sent under another profile", Vec::new());
    row.agent_color_index = Some(1);
    let (_, buffer) = render(&[row], 60, 5);
    assert_eq!(buffer[(0, 1)].fg, Theme::dark().categorical_agents()[1]);
}

#[test]
fn two_turn_footer_to_next_user_has_row_margin_and_inner_padding() {
    let mut first_answer = assistant("first answer");
    first_answer.meta = Some(AssistantMeta {
        model: Some("ludka2/a".into()),
        ..Default::default()
    });
    let mut second_answer = assistant("second answer");
    second_answer.meta = first_answer.meta.clone();
    let rows = [
        user("first question", Vec::new()),
        first_answer,
        user("second question", Vec::new()),
        second_answer,
    ];
    // Pinned 120x40 two-turn capture: answer→footer 1 blank,
    // footer→next block 1 blank, footer→next text 2 blank rows.
    let expected = vec![
        "┃",
        "┃  first question",
        "┃",
        "",
        "   first answer",
        "",
        "   Build · ludka2/a",
        "",
        "┃",
        "┃  second question",
        "┃",
        "",
        "   second answer",
        "",
        "   Build · ludka2/a",
    ];
    let (full, _) = render(&rows, 120, 40);
    assert_eq!(&full[..expected.len()], expected);

    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let (indexed, total) = visible_transcript(
        &rows,
        theme,
        120,
        120,
        (40, 0, None),
        |_| theme.categorical_agents()[0],
        &cache,
    );
    assert_eq!(total, expected.len() + 1, "indexed leading blank");
    assert_eq!(
        indexed[1..]
            .iter()
            .map(|line| line.plain_text().trim_end().to_string())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(indexed[0].plain_text(), "");
}

#[test]
fn two_turn_indexed_windows_keep_boundary_and_wrapped_user_rows() {
    let mut first_answer = assistant("done");
    first_answer.meta = Some(AssistantMeta::default());
    let rows = [
        user("first", Vec::new()),
        first_answer,
        user("abcdefghijklmnopqr", Vec::new()),
        assistant("last"),
    ];
    let width = 12;
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let expected = std::iter::once(String::new())
        .chain(
            styled::wrap_lines(
                &transcript(&rows, theme, width, width, |_| theme.text()),
                width as usize,
            )
            .into_iter()
            .map(|line| line.plain_text().trim_end().to_string()),
        )
        .collect::<Vec<_>>();
    assert_eq!(
        &expected[6..13],
        &["", "   Build", "", "┃", "┃  abcdefghi", "┃  jklmnopqr", "┃"]
    );
    for height in [1, 4, 7] {
        for start in 4..=11 {
            let end = (start + height).min(expected.len());
            let scroll = expected.len() - end;
            let (visible, total) = visible_transcript(
                &rows,
                theme,
                width,
                width,
                (height, scroll, None),
                |_| theme.text(),
                &cache,
            );
            assert_eq!(total, expected.len());
            assert_eq!(
                visible
                    .iter()
                    .map(|line| line.plain_text().trim_end().to_string())
                    .collect::<Vec<_>>(),
                expected[end - height.min(end)..end],
                "height={height} start={start} scroll={scroll}"
            );
        }
        let (bottom, total) = visible_transcript(
            &rows,
            theme,
            width,
            width,
            (height, 0, None),
            |_| theme.text(),
            &cache,
        );
        assert_eq!(total, expected.len());
        assert_eq!(
            bottom
                .iter()
                .map(|line| line.plain_text().trim_end().to_string())
                .collect::<Vec<_>>(),
            expected[expected.len() - height..],
            "sticky bottom at height={height}"
        );
    }
}

/// Assistant markdown at `paddingLeft=3` (`message-parts.tsx:156-171`) with
/// the theme's markdown/syntax tokens; concealed markers (`markdownMode`
/// defaults to `"rendered"`, `routes/session/index.tsx:228`).
#[test]
fn golden_assistant_markdown_with_syntax_colors() {
    let text = "# Heading\n\nIntro `code` here.\n\n- first\n- second\n\n```rust\nlet x = 1; // note\n```\n\n> quoted\n";
    let (rows, buffer) = render(&[assistant(text)], 60, 12);
    assert_eq!(
        rows,
        vec![
            String::new(),
            "   Heading".to_string(),
            String::new(),
            "   Intro code here.".to_string(),
            String::new(),
            "   - first".to_string(),
            "   - second".to_string(),
            String::new(),
            "   let x = 1; // note".to_string(),
            String::new(),
            "   quoted".to_string(),
            String::new(),
        ]
    );

    let theme = Theme::dark();
    // Heading text color (`markdown.heading` = `$hue.accent.200`).
    assert_eq!(buffer[(3, 1)].fg, theme.markdown(MarkdownToken::Heading));
    // Inline code (`markdown.code`).
    assert_eq!(buffer[(9, 3)].symbol(), "c");
    assert_eq!(buffer[(9, 3)].fg, theme.markdown(MarkdownToken::Code));
    // List marker (`markdown.listItem`).
    assert_eq!(buffer[(3, 5)].fg, theme.markdown(MarkdownToken::ListItem));
    // Fenced code: keyword `let`, number `1`, comment `// note`.
    assert_eq!(buffer[(3, 8)].fg, theme.syntax(SyntaxToken::Keyword));
    assert_eq!(buffer[(11, 8)].symbol(), "1");
    assert_eq!(buffer[(11, 8)].fg, theme.syntax(SyntaxToken::Number));
    assert_eq!(buffer[(14, 8)].symbol(), "/");
    assert_eq!(buffer[(14, 8)].fg, theme.syntax(SyntaxToken::Comment));
    // Blockquote (`markdown.blockQuote`).
    assert_eq!(
        buffer[(3, 10)].fg,
        theme.markdown(MarkdownToken::BlockQuote)
    );
    // Body text defaults to `markdown.text`.
    assert_eq!(buffer[(3, 3)].fg, theme.markdown(MarkdownToken::Text));
}

#[test]
fn clipped_thought_hit_requires_generated_styles_and_painted_cells() {
    let theme = Theme::dark();
    let reasoning = ReasoningBlock {
        text: "**界界**\n\nbody".into(),
        duration_ms: None,
        running: false,
        expanded: false,
        toggleable: true,
        identity: None,
    };
    let clipped = clipped_reasoning_header(&reasoning, theme, 11);
    assert_eq!(clipped.plain_text(), "   + Though");
    for x in 0..11 {
        assert_eq!(
            collapsed_thought_header(&clipped, theme, x),
            x >= 3,
            "x={x}"
        );
    }
    assert!(!collapsed_thought_header(&clipped, theme, 11));
    let full = clipped_reasoning_header(&reasoning, theme, 17);
    assert_eq!(full.plain_text(), "   + Thought: 界");
    assert!(
        !collapsed_thought_header(&full, theme, 16),
        "wide glyph tail"
    );

    // Model-controlled text can produce the same bytes, but not the
    // generated icon/label styles. A partial color match is insufficient.
    for text in ["+ Thought", "   + Thought", "`+ Thought`", "+ Though"] {
        for line in markdown_block(text, theme, 11) {
            assert!(!collapsed_thought_header(&line, theme, 5), "{text:?}");
        }
    }
    let mut tool = exploration_tool("read", "error", false);
    tool.tool = Some(card_from_row(&ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        rowid: 1,
        op: "read".into(),
        name: "read".into(),
        state: "error".into(),
        input: Some(serde_json::json!({"path": "+ Thought"}).to_string()),
        output: Some("+ Thought".into()),
        output_bytes: 9,
        output_truncated: false,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    }));
    let tool_lines = transcript(&[tool], theme, 60, 60, |_| theme.text());
    assert!(
        tool_lines
            .iter()
            .any(|line| line.plain_text().contains("+ Thought"))
    );
    for line in tool_lines {
        assert!(!collapsed_thought_header(&line, theme, 5));
    }
    let mut counterfeit = clipped.clone();
    let mut spans = counterfeit.spans().to_vec();
    spans[3] = Span::plain(spans[3].content());
    counterfeit = Line::new(spans);
    assert!(!collapsed_thought_header(&counterfeit, theme, 5));
    let mut spans = clipped.spans().to_vec();
    spans[2] = Span::styled(" ", Style::default().fg(collapsed_thought_color(theme)));
    assert!(!collapsed_thought_header(&Line::new(spans), theme, 5));
}

/// Collapsed reasoning (`routes/session/index.tsx:1765-1815`): a static
/// spinner header while running, `+ Thought: <title> · <duration>` once
/// complete; collapsed warning alpha 0.6, open warning.base.
#[test]
fn golden_reasoning_running_and_completed() {
    let theme = Theme::dark();
    let fading = Color::Rgb(0x97, 0x68, 0x2c);
    assert_eq!(collapsed_thought_color(theme), fading);
    assert_eq!(
        collapsed_thought_color(Theme::light()),
        Color::Rgb(0xe6, 0xba, 0x7d)
    );
    let running = HistoryRow {
        reasoning: Some(ReasoningBlock {
            text: "**Inspecting**\n\nbody".to_string(),
            duration_ms: None,
            running: true,
            expanded: false,
            toggleable: true,
            identity: None,
        }),
        ..assistant("")
    };
    let (rows, buffer) = render(std::slice::from_ref(&running), 60, 3);
    assert_eq!(
        rows,
        vec![
            String::new(),
            "   ⋯ Thinking: Inspecting".to_string(),
            String::new(),
        ]
    );
    assert_eq!(buffer[(3, 1)].symbol(), "⋯");
    assert_eq!(buffer[(3, 1)].fg, theme.text());
    let mut running_open = running.clone();
    running_open.reasoning.as_mut().unwrap().expanded = true;
    let (rows, buffer) = render(&[running_open], 60, 6);
    assert_eq!(rows[1], "   ⋯ Thinking: Inspecting");
    assert_eq!(rows[3], "   ┃ Inspecting");
    assert_eq!(buffer[(3, 1)].fg, theme.text());

    let completed = HistoryRow {
        reasoning: Some(ReasoningBlock {
            text: "**Inspecting**\n\nbody".to_string(),
            duration_ms: Some(1500),
            running: false,
            expanded: false,
            toggleable: true,
            identity: None,
        }),
        ..assistant("")
    };
    let (rows, buffer) = render(std::slice::from_ref(&completed), 60, 3);
    assert_eq!(
        rows,
        vec![
            String::new(),
            "   + Thought: Inspecting · 1.5s".to_string(),
            String::new(),
        ]
    );
    assert_eq!(buffer[(3, 1)].symbol(), "+");
    assert_eq!(buffer[(3, 1)].fg, fading);
    assert_eq!(buffer[(4, 1)].symbol(), " ");
    assert_eq!(buffer[(4, 1)].fg, Color::Reset);
    assert_eq!(buffer[(5, 1)].fg, fading);
    assert!(collapsed_thought_header(
        &transcript(std::slice::from_ref(&completed), theme, 60, 60, |_| theme
            .text())[1],
        theme,
        MESSAGE_PADDING
    ));
    let mut open = completed.clone();
    open.reasoning.as_mut().unwrap().expanded = true;
    let (rows, buffer) = render(&[open.clone()], 60, 6);
    assert_eq!(buffer[(3, 1)].symbol(), "-");
    assert_eq!(buffer[(3, 1)].fg, theme.warning());
    assert_eq!(buffer[(4, 1)].symbol(), " ");
    assert_eq!(buffer[(4, 1)].fg, Color::Reset);
    assert_eq!(buffer[(5, 1)].symbol(), "T");
    assert_eq!(rows[1], "   - Thought · 1.5s");
    assert_eq!(rows[3], "   ┃ Inspecting");
    assert_eq!(buffer[(3, 3)].fg, theme.decrease(theme.background_raised()));
    assert_eq!(buffer[(5, 3)].fg, theme.text_muted());
    assert!(buffer[(5, 3)].modifier.contains(Modifier::BOLD));
    assert!(!buffer[(5, 5)].modifier.contains(Modifier::BOLD));
    open.reasoning.as_mut().unwrap().toggleable = false;
    let (rows, buffer) = render(&[open], 60, 6);
    assert_eq!(rows[1], "   ┃ Thought: 1.5s");
    assert_eq!(buffer[(5, 1)].symbol(), "T");
    assert_eq!(buffer[(5, 1)].fg, theme.fade(theme.warning(), 0.6));
    // Unknown duration renders `Thought` without an invented `0ms`.
    let no_duration = HistoryRow {
        reasoning: Some(ReasoningBlock {
            text: "no title".to_string(),
            duration_ms: None,
            running: false,
            expanded: false,
            toggleable: true,
            identity: None,
        }),
        ..assistant("")
    };
    let (rows, _) = render(&[no_duration], 60, 2);
    assert_eq!(rows, vec![String::new(), "   + Thought".to_string()]);
}

#[test]
fn empty_cleaned_reasoning_has_no_header_spacer_or_index_hit() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    for text in ["  \n\t  ", "  [REDACTED]\n", "[REDACTED] [REDACTED]"] {
        let mut row = assistant("");
        row.reasoning = Some(ReasoningBlock {
            text: text.into(),
            duration_ms: None,
            running: false,
            expanded: true,
            toggleable: true,
            identity: Some(ReasoningIdentity::Durable(2, 0)),
        });
        let (normal, buffer) = render(std::slice::from_ref(&row), 120, 40);
        assert!(normal.iter().all(String::is_empty), "{text:?}: {normal:?}");
        assert_eq!(buffer[(3, 1)].symbol(), " ");
        let (indexed, total) = visible_transcript_expanded(
            std::slice::from_ref(&row),
            theme,
            (120, 120),
            (40, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(total, 1, "no hidden spacer: {text:?}");
        assert_eq!(indexed.len(), 1);
        assert_eq!(
            reasoning_header_at(
                &[row],
                theme,
                (120, 120),
                (40, 0, None),
                |_| Color::Reset,
                &cache,
                (&|_| false, (5, 1)),
            ),
            None
        );
    }
}

#[test]
fn expanded_group_header_and_body_use_separate_geometry_and_index() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut row = assistant("");
    row.reasoning = Some(ReasoningBlock {
        text: "detail".into(),
        duration_ms: None,
        running: false,
        expanded: true,
        toggleable: true,
        identity: Some(ReasoningIdentity::Durable(2, 0)),
    });
    for width in [120, 12, 8] {
        let (rows, buffer) = render(std::slice::from_ref(&row), width, 40);
        assert_eq!(
            rows[1],
            match width {
                8 => "   - Tho",
                _ => "   - Thought",
            }
        );
        assert_eq!(buffer[(3, 1)].symbol(), "-");
        assert_eq!(buffer[(3, 1)].fg, theme.warning());
        assert_eq!(buffer[(4, 1)].symbol(), " ");
        assert_eq!(buffer[(5, 1)].symbol(), "T");
        assert_eq!(buffer[(3, 3)].symbol(), "┃");
        assert_eq!(buffer[(3, 3)].fg, theme.decrease(theme.background_raised()));
        assert_eq!(buffer[(5, 3)].symbol(), "d");
        assert_eq!(buffer[(5, 3)].fg, theme.text_muted());
        assert_eq!(
            rows[3],
            if width == 8 {
                "   ┃ det"
            } else {
                "   ┃ detail"
            }
        );
        let (visible, total) = visible_transcript_expanded(
            std::slice::from_ref(&row),
            theme,
            (width, width),
            (40, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(visible.len(), total);
        assert_eq!(visible[2].plain_text(), rows[1]);
        for x in [2, 3, 4, 7, (width - 1) as usize] {
            let expected = (3..(width as usize).min(12))
                .contains(&x)
                .then_some(ReasoningIdentity::Durable(2, 0));
            assert_eq!(
                reasoning_header_at(
                    std::slice::from_ref(&row),
                    theme,
                    (width, width),
                    (40, 0, None),
                    |_| Color::Reset,
                    &cache,
                    (&|_| false, (x, 2)),
                ),
                expected,
                "width={width} x={x}"
            );
        }
        assert_eq!(
            reasoning_header_at(
                std::slice::from_ref(&row),
                theme,
                (width, width),
                (40, 0, None),
                |_| Color::Reset,
                &cache,
                (&|_| false, (7, 3)),
            ),
            None,
            "body is not clickable"
        );
    }
    row.reasoning.as_mut().unwrap().toggleable = false;
    let (rows, buffer) = render(&[row], 120, 40);
    assert_eq!(rows[1], "   ┃ Thought");
    assert_eq!(rows[3], "   ┃ detail");
    assert_eq!(buffer[(5, 1)].symbol(), "T");
    assert_eq!(buffer[(5, 3)].symbol(), "d");
    assert_eq!(buffer[(3, 3)].fg, theme.decrease(theme.background()));
}

#[test]
fn grouped_hide_duration_title_and_open_body_match_session_group() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut row = assistant("");
    row.reasoning = Some(ReasoningBlock {
        text: "**Inspecting**\n\nPublic summary only.".into(),
        duration_ms: Some(5),
        running: false,
        expanded: false,
        toggleable: true,
        identity: Some(ReasoningIdentity::Durable(2, 0)),
    });
    let (collapsed, buffer) = render(std::slice::from_ref(&row), 120, 40);
    assert_eq!(collapsed[1], "   + Thought: Inspecting · 5ms");
    assert_eq!(buffer[(3, 1)].fg, collapsed_thought_color(theme));
    row.reasoning.as_mut().unwrap().expanded = true;
    let (open, buffer) = render(std::slice::from_ref(&row), 120, 40);
    assert_eq!(open[1], "   - Thought · 5ms");
    assert_eq!(open[3], "   ┃ Inspecting");
    assert!(open.iter().any(|line| line == "   ┃ Public summary only."));
    assert_eq!(buffer[(3, 1)].fg, theme.warning());
    assert_eq!(buffer[(3, 3)].fg, theme.decrease(theme.background_raised()));
    assert_eq!(buffer[(5, 3)].fg, theme.text_muted());
    let (narrow, _) = render(std::slice::from_ref(&row), 12, 40);
    assert_eq!(narrow[1], "   - Thought");
    assert_eq!(narrow[3], "   ┃ Inspect");
    for width in [120, 12] {
        let (visible, _) = visible_transcript_expanded(
            std::slice::from_ref(&row),
            theme,
            (width, width),
            (40, 0, None),
            |_| Color::Reset,
            &cache,
            &|_| false,
        );
        assert_eq!(
            visible[2].plain_text(),
            if width == 12 {
                narrow[1].as_str()
            } else {
                open[1].as_str()
            }
        );
        for x in [2, 3, 4, 5, 11, 12, 18] {
            let header_width = if width == 12 { 12 } else { 18 };
            assert_eq!(
                reasoning_header_at(
                    std::slice::from_ref(&row),
                    theme,
                    (width, width),
                    (40, 0, None),
                    |_| Color::Reset,
                    &cache,
                    (&|_| false, (x, 2)),
                ),
                (3..header_width)
                    .contains(&x)
                    .then_some(ReasoningIdentity::Durable(2, 0)),
                "width={width}, x={x}"
            );
        }
    }
}

/// Assistant footer (`routes/session/index.tsx:1963-1981`): titlecased
/// agent in the agent color, then model, duration, tok/s and the
/// interrupted marker, all muted, separated by ` · `.
#[test]
fn golden_assistant_footer_with_tokens_per_second_and_interrupt() {
    let theme = Theme::dark();
    let meta = AssistantMeta {
        model: Some("ludka2/a".to_string()),
        duration_ms: Some(1500),
        input_tokens: Some(1000),
        output_tokens: Some(200),
        streamed_ms: Some(4000),
        interrupted: false,
        ..AssistantMeta::default()
    };
    let row = HistoryRow {
        meta: Some(meta.clone()),
        ..assistant("done")
    };
    let (rows, buffer) = render(&[row], 80, 4);
    assert_eq!(
        rows,
        vec![
            String::new(),
            "   done".to_string(),
            String::new(),
            "   Build · ludka2/a · 1.5s · 50.0 tok/s".to_string(),
        ]
    );
    assert_eq!(buffer[(3, 3)].fg, theme.categorical_agents()[0]);
    assert_eq!(buffer[(9, 3)].fg, theme.text_muted());

    // Interrupted: muted agent color and the `interrupted` marker; no
    // `tok/s` when the provider reported no usage.
    let interrupted = HistoryRow {
        meta: Some(AssistantMeta {
            model: None,
            duration_ms: None,
            input_tokens: None,
            output_tokens: None,
            streamed_ms: None,
            interrupted: true,
            status: Some("cancelled".into()),
            ..AssistantMeta::default()
        }),
        ..assistant("")
    };
    let (rows, buffer) = render(&[interrupted], 80, 3);
    assert_eq!(
        rows,
        vec![
            String::new(),
            "   Build · interrupted".to_string(),
            String::new(),
        ]
    );
    assert_eq!(buffer[(3, 1)].fg, theme.text_muted());

    // Below 28 columns the model field disappears, in the 28..36 band the
    // duration does too (`routes/session/index.tsx:1964-1969`).
    let narrow = footer_line(Some("build"), &meta, theme, 27, &|_| {
        theme.categorical_agents()[0]
    })
    .expect("footer");
    assert_eq!(narrow.plain_text(), "   Build · 1.5s · 50.0 tok/s");
    let banded = footer_line(Some("build"), &meta, theme, 30, &|_| {
        theme.categorical_agents()[0]
    })
    .expect("footer");
    assert_eq!(banded.plain_text(), "   Build · ludka2/a · 50.0 tok/s");
}

#[test]
fn vis34_archive_paragraph_final_lf_has_one_footer_margin_full_cached_and_indexed() {
    use oc_core::compaction::{
        CompactionAnchor, CompactionReason, CompactionSnapshot, CompactionState, CompactionUsage,
    };
    use ratatui::{layout::Rect, widgets::Widget};
    let paint = |lines: &[Line]| {
        let area = Rect::new(0, 0, 116, lines.len() as u16);
        let mut buffer = Buffer::empty(area);
        Paragraph::new(
            lines
                .iter()
                .cloned()
                .map(Line::into_ratatui)
                .collect::<Vec<_>>(),
        )
        .render(area, &mut buffer);
        buffer
    };
    // Actual provider body shape in compaction_fixture.py:66, paired
    // threshold 21/22: ARCHIVE-3-113..119, one blank, then prior footer.
    let body = format!(
        "VIS34-ANSWER-3:\n{}",
        (0..120)
            .map(|n| format!("ARCHIVE-3-{n:03} requirement detail retained in raw history.\n"))
            .collect::<String>()
    );
    let mut prior = assistant(&body);
    prior.seq = 1;
    prior.meta = Some(AssistantMeta {
        model: Some("MiMo-V2.6-Flash Free".into()),
        duration_ms: Some(28),
        session_tps: Some(false),
        ..Default::default()
    });
    let mut prompt = user("VIS34 next: continue from checkpoint; no tools.", vec![]);
    prompt.seq = 2;
    let checkpoint = crate::compaction::row(&CompactionSnapshot {
        model:None,
            anchor: CompactionAnchor { message: prompt.message_id.as_ref().map(|id| id.0.clone()), ..Default::default() },
            id: "checkpoint".into(), session: "s".into(), reason: CompactionReason::Automatic, state: CompactionState::Completed,
            summary: "## Objective\n- VIS34-CHECKPOINT: preserve R1, R2, R3.\n\n## Work State\n- Three seeded exchanges completed; filesystem unchanged.\n\n## Next Move\n1. Continue the user request without replaying tools.\n".into(),
            usage: Some(CompactionUsage { input_tokens: 1234, output_tokens: 321, cache_read_tokens: 0, cache_write_tokens: 0, reasoning_tokens: 0 }), provider_native: false, error: None,
        }, 0, false);
    let mut next = assistant("VIS34-NEXT-DONE");
    next.seq = 3;
    next.meta = Some(AssistantMeta {
        model: Some("MiMo-V2.6-Flash Free".into()),
        duration_ms: Some(4400),
        session_tps: Some(false),
        ..Default::default()
    });
    let theme = Theme::dark();
    // Both live completion (body+footer) and replay (separate footer row).
    for separated_footer in [false, true] {
        let mut rows = vec![prior.clone()];
        if separated_footer {
            rows[0].meta = None;
            let mut footer = assistant("");
            footer.seq = 1;
            footer.meta = prior.meta.clone();
            rows.push(footer);
        }
        rows.extend([prompt.clone(), checkpoint.clone(), next.clone()]);
        let full = transcript(&rows, theme, 116, 120, |_| theme.categorical_agents()[0]);
        let last_archive = full
            .iter()
            .position(|line| line.plain_text().contains("ARCHIVE-3-119"))
            .unwrap();
        assert_eq!(full[last_archive + 1].plain_text(), "");
        assert_eq!(
            full[last_archive + 2].plain_text(),
            "   Build · MiMo-V2.6-Flash Free · 28ms"
        );
        let cache = RefCell::new(MarkdownCache::default());
        let cached = transcript_with_expansion(
            &rows,
            theme,
            116,
            120,
            |_| theme.categorical_agents()[0],
            Some(&cache),
            &|_| false,
        );
        assert_eq!(cached, full);
        let mut padded = vec![Line::plain("")];
        padded.extend(full);
        let indexed_cache = RefCell::new(MarkdownCache::default());
        for pass in 0..2 {
            let (visible, total) = visible_transcript_expanded(
                &rows,
                theme,
                (116, 120),
                (31, 0, None),
                |_| theme.categorical_agents()[0],
                &indexed_cache,
                &|_| false,
            );
            assert_eq!(total, padded.len(), "pass={pass}");
            assert_eq!(
                paint(&visible),
                paint(&padded[padded.len() - 31..]),
                "all styled cells, pass={pass}"
            );
            assert!(visible[0].plain_text().contains("ARCHIVE-3-113"));
            assert_eq!(visible[7].plain_text(), "");
            assert_eq!(
                visible[8].plain_text(),
                "   Build · MiMo-V2.6-Flash Free · 28ms"
            );
        }
    }
}

#[test]
fn v02_footer_uses_pinned_agent_slot_and_exact_status() {
    for status in ["completed", "failed", "cancelled", "incomplete", "unknown"] {
        let row = HistoryRow {
            meta: Some(AssistantMeta {
                status: Some(status.into()),
                agent_color_index: Some(3),
                ..Default::default()
            }),
            ..assistant("")
        };
        let (rows, buffer) = render(&[row], 80, 3);
        assert!(
            rows.iter().any(|row| if status == "completed" {
                row.trim() == "Build"
            } else {
                row.contains(&format!("Build · {status}"))
            }),
            "{rows:?}"
        );
        assert_eq!(
            buffer[(3, 1)].fg,
            if matches!(status, "failed" | "cancelled" | "unknown") {
                Theme::dark().text_muted()
            } else {
                Theme::dark().categorical_agents()[3]
            },
            "failure mutes the profile; otherwise its generation slot wins over current slot zero"
        );
    }
}

/// Long content wraps instead of clipping (`message-parts.tsx` markdown and
/// the user `<text>` both wrap at the content width).
#[test]
fn golden_long_lines_wrap_without_loss() {
    let long = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let (rows, _) = render(&[user(long, Vec::new())], 40, 10);
    for row in &rows {
        assert!(row.chars().count() <= 40, "{row:?}");
        assert!(!row.contains('…'), "wrapping must not truncate: {row:?}");
    }
    let joined = rows.join(" ").replace('┃', " ");
    for word in long.split(' ') {
        assert!(joined.contains(word), "{word:?} missing from {rows:?}");
    }

    // A single word longer than the line splits instead of disappearing.
    let giant = "x".repeat(90);
    let (rows, _) = render(&[user(&giant, Vec::new())], 40, 10);
    let text: String = rows
        .iter()
        .flat_map(|row| {
            row.trim_start_matches('┃')
                .trim()
                .chars()
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(text, giant);
}

#[test]
fn vis38_off_long_user_wrap_keeps_continuity_across_preview_byte_boundaries() {
    // The off 160x48 actual pair exposed a different last user line after
    // independent 4096-byte chunks restarted word wrapping. U34 wraps one text.
    let text = format!(
        "VIS38 seed C: closed analysis, retain requirement C. {}",
        "Closed analysis 日本語 résumé 👩‍💻 e\u{301}. ".repeat(300)
    );
    let rows = [user(&text, Vec::new())];
    let theme = Theme::dark();
    for width in [76, 116, 156] {
        let full = transcript(&rows, theme, width, width + 4, |_| theme.primary());
        let cache = RefCell::new(MarkdownCache::default());
        let (indexed, total) = visible_transcript(
            &rows,
            theme,
            width,
            width + 4,
            (10_000, 0, None),
            |_| theme.primary(),
            &cache,
        );
        assert_eq!(total, full.len() + 1);
        assert_eq!(
            indexed,
            std::iter::once(Line::plain(""))
                .chain(full)
                .collect::<Vec<_>>(),
            "width={width}"
        );
    }
}

#[test]
fn vis38_summary_wrapper_keeps_source_separator_foreground_at_word_wrap() {
    // Unchanged pinned D05 multi-range payload through original U34 <text>:
    // dcp-pair-check20260928-07.json, (110,3) at 120x40 and (152,12) at 160x48.
    // Remove only the real two-cell transcript margin for this wrapper unit test.
    let source = "→ Compression (~28 tokens): ### VIS38 multi long topic 日本語 résumé 👩‍💻 e\u{301} — preserve closed analysis and requirements across ranges with full real summaries";
    let theme = Theme::dark();
    for (width, separator_x) in [(116, 107), (156, 149)] {
        let lines = transcript_text_block(source, theme, width, theme.categorical_agents()[0]);
        assert_eq!(lines.len(), 4, "two text rows plus vertical padding");
        let mut terminal = Terminal::new(TestBackend::new(width, 4)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Block::default().style(Style::default().fg(Color::Rgb(255, 255, 255))),
                    frame.area(),
                );
                frame.render_widget(
                    Paragraph::new(styled::Lines::from(lines).into_text()),
                    frame.area(),
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(separator_x, 1)].symbol(), " ");
        assert_eq!(buffer[(separator_x, 1)].fg, theme.text(), "width={width}");
        assert_eq!(buffer[(separator_x, 1)].bg, theme.user_message_background());
        assert_eq!(
            buffer[(separator_x + 1, 1)].fg,
            Color::Rgb(255, 255, 255),
            "padding after the real separator must not inherit text foreground"
        );
    }
}

/// Markdown subset: every supported construct, and unsupported markers kept
/// literally instead of being dropped.
#[test]
fn markdown_subset_renders_and_falls_back() {
    let theme = Theme::dark();
    let plain = |line: &Line| line.plain_text();

    let lines = markdown(
        "# Title\n## Sub\nparagraph with `code`, **bold**, *ital*, _em_, and [link](https://x)\n",
        theme,
    );
    assert_eq!(plain(&lines[0]), "Title");
    assert_eq!(plain(&lines[1]), "Sub");
    assert_eq!(
        plain(&lines[2]),
        "paragraph with code, bold, ital, em, and link"
    );
    let styles: Vec<Vec<Color>> = lines
        .iter()
        .map(|line| {
            line.spans()
                .iter()
                .map(|span| span.style().fg.unwrap_or(Color::Reset))
                .collect()
        })
        .collect();
    assert!(styles[0].contains(&theme.markdown(MarkdownToken::Heading)));
    assert!(styles[2].contains(&theme.markdown(MarkdownToken::Code)));
    assert!(styles[2].contains(&theme.markdown(MarkdownToken::Strong)));
    assert!(styles[2].contains(&theme.markdown(MarkdownToken::Emphasis)));
    assert!(styles[2].contains(&theme.markdown(MarkdownToken::LinkText)));

    // Strike is left literal without the extension; image syntax is concealed.
    let lines = markdown("a ~~strike~~ b `unclosed and ![alt](u)", theme);
    assert_eq!(plain(&lines[0]), "a ~~strike~~ b `unclosed and alt");
    // A table is a grid, with concealed Markdown delimiters.
    let lines = markdown("| a | b |\n|---|---|\n| 1 | 2 |", theme);
    assert!(
        lines
            .iter()
            .any(|line| line.plain_text().contains("a") && line.plain_text().contains("│"))
    );
    assert!(
        lines
            .iter()
            .any(|line| line.plain_text().contains("1") && line.plain_text().contains("2"))
    );
    assert!(!lines.iter().any(|line| line.plain_text().contains("---")));
}

#[test]
fn strong_markdown_keeps_nested_tokens_bold_and_their_own_colors() {
    let theme = Theme::dark();
    let (rows, buffer) = render(
        &[assistant(
            "plain **bold [link](https://x) and *inner* `code`** tail",
        )],
        80,
        4,
    );
    assert_eq!(rows[1], "   plain bold link and inner code tail");
    for (x, token) in [
        (9, MarkdownToken::Strong),
        (14, MarkdownToken::LinkText),
        (23, MarkdownToken::Emphasis),
    ] {
        assert!(buffer[(x, 1)].modifier.contains(Modifier::BOLD), "x={x}");
        assert_eq!(buffer[(x, 1)].fg, theme.markdown(token), "x={x}");
    }
    for x in [3, 34] {
        assert!(!buffer[(x, 1)].modifier.contains(Modifier::BOLD), "x={x}");
        assert_eq!(buffer[(x, 1)].fg, theme.markdown(MarkdownToken::Text));
    }
    assert_eq!(buffer[(29, 1)].fg, theme.markdown(MarkdownToken::Code));
    assert!(!buffer[(29, 1)].modifier.contains(Modifier::BOLD));
}

#[test]
fn v06a_table_fixture_escaped_pipes_unicode_and_bounded_wrap() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tui-recovery/fixtures/transcript.md"
    ));
    let theme = Theme::dark();
    let table =
        &fixture[fixture.find("| Инструмент").unwrap()..fixture.find("Через Code Mode").unwrap()];
    for width in [42, 80, 120] {
        let rows = markdown_block(table, theme, width);
        let grid: Vec<_> = rows.iter().map(Line::plain_text).collect();
        assert!(
            grid.iter()
                .any(|r| r.contains("Инструмент") && r.contains('│')),
            "{width}: {grid:?}"
        );
        assert!(
            grid.iter().any(|r| r.contains("subagent")),
            "{width}: {grid:?}"
        );
        assert_eq!(
            grid.iter()
                .filter(|r| r.contains("├") && r.contains("┼"))
                .count(),
            11,
            "header plus 11 tool rows: {grid:?}"
        );
        assert!(
            !grid.iter().any(|r| r.contains("| ---")),
            "{width}: {grid:?}"
        );
        assert!(
            rows.iter()
                .all(|r| r.spans().iter().map(styled::span_width).sum::<usize>() <= width as usize)
        );
    }
    let narrow = markdown_at_width(table, theme, 74);
    assert!(
        narrow
            .iter()
            .any(|r| r.plain_text().contains("│ Инструмент │"))
    );
    let header = narrow
        .iter()
        .find(|r| r.plain_text().contains("Инструмент"))
        .unwrap();
    assert_eq!(
        header.spans()[0].style().fg,
        Some(Color::Rgb(136, 136, 136))
    );
    assert!(
        header
            .spans()
            .iter()
            .any(|span| span.content().contains("Инструмент")
                && span.style().fg == Some(theme.markdown(MarkdownToken::Heading))
                && span.style().add_modifier.contains(Modifier::BOLD))
    );
    let wrapped_row = narrow
        .iter()
        .find(|r| r.plain_text().contains("кодовой"))
        .unwrap();
    assert!(
        wrapped_row
            .spans()
            .windows(2)
            .any(|spans| spans[0].content() == " "
                && spans[0].style().fg == Some(theme.markdown(MarkdownToken::Text))
                && spans[1].style().fg == theme.hue("neutral", 100))
    );
    let wide = markdown_at_width(table, theme, 114);
    assert!(
        wide.iter()
            .any(|r| r.plain_text().contains("│ Инструмент       │"))
    );
    let wide = markdown_at_width(table, theme, 112);
    assert!(
        wide.iter()
            .any(|r| r.plain_text().contains("│ Инструмент      │"))
    );
    let escaped = "| Key | Value |\n| --- | --- |\n| a \\| b | 中文 🧑‍💻 emoji and a very long cell with more words to wrap |";
    let rows = markdown_block(escaped, theme, 24);
    let text: String = rows
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("a | b"), "{text}");
    assert!(text.contains("中文"), "{text}");
    assert!(text.contains("🧑‍💻"), "{text}");
    assert!(
        rows.iter()
            .all(|r| r.spans().iter().map(styled::span_width).sum::<usize>() <= 24)
    );
}

#[test]
fn table_grid_columns_follow_pinned_full_width_sizing() {
    // opencode v2.0.12 TextPart uses OpenTUI 0.5.10's grid/full table
    // (message-parts.tsx:160-170). In the paired 160x48 Reader capture,
    // the fixture grid's left and right edges agree, but the native
    // divider is one cell too far right (x23 instead of x22).
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tui-recovery/fixtures/transcript.md"
    ));
    let table =
        &fixture[fixture.find("| Инструмент").unwrap()..fixture.find("Через Code Mode").unwrap()];
    let theme = Theme::dark();
    for (available, first_column) in [(74, 10), (110, 14), (114, 16)] {
        let rows = markdown_at_width(table, theme, available);
        let border = rows.first().unwrap().plain_text();
        let divider = border.chars().position(|ch| ch == '┬').unwrap();
        assert_eq!(divider, first_column + 3, "available={available}: {border}");
        assert_eq!(UnicodeWidthStr::width(border.as_str()), available);
    }
}

#[test]
fn unicode_table_widths_agree_between_full_and_indexed_pages() {
    let text = "| 名称 | 説明 |\n| --- | --- |\n| 中文🧑‍💻 | 長い説明 with several words |\n| café | 東京に行く とても長い説明 with many more words to wrap |\n";
    let short = "| 中文🧑‍💻 | 東京 |\n| --- | --- |\n| 中文🧑‍💻 | 東京 |";
    let theme = Theme::dark();
    // Natural content widths are 6 and 4 cells. Grid borders and padding
    // cost seven cells; the remaining spare width is divided evenly.
    for (width, divider) in [(80, 42), (120, 62), (160, 82)] {
        let border = markdown_block(short, theme, width)[0].plain_text();
        assert_eq!(border.chars().position(|ch| ch == '┬'), Some(divider));
        assert_eq!(UnicodeWidthStr::width(border.as_str()), width as usize);
        let full = markdown_block(text, theme, width);
        let page = index_source(text, width)
            .into_iter()
            .find_map(|page| page.table_widths)
            .unwrap();
        let indexed = markdown_block_with_widths(text, theme, width, Some(&page));
        let grid = full.iter().map(Line::plain_text).collect::<Vec<_>>();
        assert_eq!(
            full.first().unwrap().plain_text(),
            indexed.first().unwrap().plain_text()
        );
        assert!(grid.iter().any(|line| line.contains("中文🧑‍💻")));
        assert!(grid.iter().any(|line| line.contains("東京に行く")));
        assert!(full.iter().all(|line| {
            line.spans().iter().map(styled::span_width).sum::<usize>() <= width as usize
        }));
        assert!(indexed.iter().all(|line| {
            line.spans().iter().map(styled::span_width).sum::<usize>() <= width as usize
        }));
    }
}

#[test]
fn v06a_cached_parts_invalidate_only_dirty_revision_width_and_theme() {
    let dark = Theme::dark();
    let light = Theme::light();
    let cache = RefCell::new(MarkdownCache::default());
    let mut rows = vec![
        assistant("# Heading\n\n| a | b |\n|---|---|\n| 1 | 2 |"),
        assistant("```rust\nlet x = 1;\n"),
    ];
    rows[0].seq = 1;
    rows[1].seq = i64::MAX;
    let render = |rows: &[HistoryRow], theme, width, height, scroll| {
        visible_transcript(
            rows,
            theme,
            width,
            width,
            (height, scroll, None),
            |_| theme.text(),
            &cache,
        )
    };
    let (first, total) = render(&rows, dark, 40, 4, 0);
    assert_eq!(first.len(), 4);
    assert!(total > 4);
    assert_eq!(cache.borrow().parses, 2);
    let (again, _) = render(&rows, dark, 40, 4, 0);
    assert_eq!(first, again);
    assert_eq!(
        cache.borrow().parses,
        2,
        "completed parts reused across counting and repeated frames"
    );
    let (scrolled, _) = render(&rows, dark, 40, 4, 5);
    assert!(scrolled.len() <= 4);
    assert!(
        cache.borrow().parses <= 4,
        "scroll may parse only newly visible pages"
    );
    rows[1].text.push_str("let y = 2;\n");
    render(&rows, dark, 40, 4, 0);
    assert_eq!(cache.borrow().parses, 4);
    assert_eq!(
        cache.borrow().blocks.len(),
        3,
        "dirty part supersedes its revision"
    );
    render(&rows, dark, 24, 4, 0);
    render(&rows, light, 24, 4, 0);
    assert_eq!(
        cache.borrow().parses,
        6,
        "width/theme select fresh geometry/styles"
    );
    assert!(cache.borrow().bytes <= MAX_CACHED_BYTES);
    assert!(cache.borrow().index_bytes() <= MAX_INDEX_BYTES);
}

#[test]
fn v06a_ansi_and_large_markdown_stay_bounded() {
    let theme = Theme::dark();
    let raw = "# Hi \u{1b}[31mred\u{1b}[0m\n\n| a | b |\n|---|---|\n| \u{1b}]8;;url\u{7}link | `\u{1b}[2J` |\n\n```sh\n\u{1b}[Hsecret\n```";
    let rows = markdown_block(raw, theme, 32);
    let text = rows
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rows.iter()
            .all(|row| !row.plain_text().chars().any(char::is_control)),
        "{text:?}"
    );
    assert!(text.contains("secret"));
    let long = "word\n".repeat(10_000);
    let rows = markdown_block(&long, theme, 32);
    assert!(rows.len() <= MAX_MARKDOWN_ROWS);
    assert!(
        rows.last()
            .unwrap()
            .plain_text()
            .contains("preview limited")
    );
    let gaps = format!("first\n{}last", "\n".repeat(10_000));
    let rows = markdown_block(&gaps, theme, 32);
    assert!(rows.len() <= MAX_MARKDOWN_ROWS);
    assert!(
        rows.last()
            .unwrap()
            .plain_text()
            .contains("preview limited")
    );
}

#[test]
fn v06a_oversize_assistant_keeps_marker_footer_and_scrollable_content() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let mut row = assistant(&format!("start\n{}end", "line\n".repeat(700)));
    row.meta = Some(AssistantMeta {
        model: Some("fixture-model".into()),
        ..Default::default()
    });
    let view = |scroll| {
        visible_transcript(
            std::slice::from_ref(&row),
            theme,
            40,
            40,
            (10, scroll, None),
            |_| theme.text(),
            &cache,
        )
    };
    let (bottom, total) = view(0);
    let text = bottom
        .iter()
        .map(Line::plain_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Build · fixture-model"), "{text}");
    assert!(text.contains("end"), "{text}");
    assert!(total > 512);
    let (top, _) = view(usize::MAX);
    assert!(top.iter().any(|line| line.plain_text().contains("start")));
    assert!(bottom.len() <= 10 && top.len() <= 10);
}

#[test]
fn v06a_oversize_user_is_paged_without_dropping_the_tail() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let row = user(&format!("first\n{}last", "line\n".repeat(800)), Vec::new());
    let view = |scroll| {
        visible_transcript(
            std::slice::from_ref(&row),
            theme,
            32,
            32,
            (10, scroll, None),
            |_| theme.text(),
            &cache,
        )
    };
    let (bottom, total) = view(0);
    assert!(total > 800);
    assert!(bottom.iter().any(|line| line.plain_text().contains("last")));
    let (top, _) = view(usize::MAX);
    assert!(top.iter().any(|line| line.plain_text().contains("first")));
}

#[test]
fn v06a_narrow_table_wide_glyphs_never_displace_grid_borders() {
    let lines = markdown_block("| 中 | 中 |\n| --- | --- |\n| 🧑‍💻 | 文 |", Theme::dark(), 12);
    for line in &lines {
        assert_eq!(
            line.spans().iter().map(styled::span_width).sum::<usize>(),
            12,
            "{:?}",
            line.plain_text()
        );
        assert!(
            line.plain_text().ends_with('┐')
                || line.plain_text().ends_with('┤')
                || line.plain_text().ends_with('┘')
                || line.plain_text().ends_with('│')
        );
    }
    assert!(lines.iter().any(|line| line.plain_text().contains('…')));
}

#[test]
fn v06a_provider_controls_are_inert_outside_markdown() {
    let mut reasoning = assistant("");
    reasoning.reasoning = Some(ReasoningBlock {
        text: "**\u{1b}]52;;AAA\u{7}**\n\nbody".into(),
        duration_ms: None,
        running: true,
        expanded: false,
        toggleable: true,
        identity: None,
    });
    reasoning.meta = Some(AssistantMeta {
        model: Some("\u{1b}[2Jmodel".into()),
        status: Some("\u{1b}[Hfailed".into()),
        ..Default::default()
    });
    let mut user_row = user(
        "\u{1b}]8;;link\u{7}visible",
        vec![Chip {
            kind: ChipKind::File,
            name: "\u{1b}[31mname".into(),
        }],
    );
    user_row.seq = 1;
    for line in transcript(&[user_row, reasoning], Theme::dark(), 80, 80, |_| {
        Color::Reset
    }) {
        assert!(
            !line.plain_text().chars().any(char::is_control),
            "{:?}",
            line.plain_text()
        );
    }
    let operation = oc_core::queries::ToolOpView {
        child_job: None,
        output_presentation: None,
        question: None,
        op: "op".into(),
        rowid: 1,
        name: "bash".into(),
        state: "completed".into(),
        input: Some(serde_json::json!({"command": "\u{1b}]52;;AAA\u{7}"}).to_string()),
        output: Some("\u{1b}[2Jtool output\u{7}".into()),
        output_bytes: 20,
        output_truncated: false,
        patch_effects: None,
        dcp: None,
        dcp_topic: None,
    };
    let mut row = assistant("");
    row.role = "tool".into();
    row.tool = Some(crate::history::card_from_row(&operation));
    for line in transcript(&[row], Theme::dark(), 80, 80, |_| Color::Reset) {
        assert!(
            !line.plain_text().chars().any(char::is_control),
            "{:?}",
            line.plain_text()
        );
    }
}

#[test]
fn v06a_long_open_fence_has_bounded_per_delta_parse_work() {
    let theme = Theme::dark();
    let mut cache = MarkdownCache::default();
    let mut text = String::from("```rust\n");
    for _ in 0..80 {
        text.push_str(&"let a = 1234567890; ".repeat(100));
        text.push('\n');
        let _ = cache.render((2, 0, 0), &text, theme, 40);
    }
    assert!(
        cache.parsed_bytes <= 80 * 16 * 1024,
        "{}",
        cache.parsed_bytes
    );
    assert!(
        cache
            .render((2, 0, 0), &text, theme, 40)
            .iter()
            .any(|line| line.plain_text().contains("preview limited"))
    );
    let cache = RefCell::new(MarkdownCache::default());
    let mut stream = String::from("```rust\n");
    for _ in 0..80 {
        stream.push_str(&"let a = 1234567890; ".repeat(100));
        stream.push('\n');
        let mut row = assistant(&stream);
        row.seq = i64::MAX;
        let (visible, _) = visible_transcript(
            &[row],
            theme,
            40,
            40,
            (10, 0, Some(0)),
            |_| theme.text(),
            &cache,
        );
        assert!(visible.len() <= 10);
    }
    assert!(cache.borrow().parsed_bytes <= 80 * LIVE_MARKDOWN_BYTES);
    assert!(
        cache.borrow().parses < 20,
        "prefix cache must stop decoding once full"
    );
}

#[test]
fn v06a_fenced_code_retains_intentional_blank_lines() {
    let lines = markdown("```text\nfirst\n\nsecond\n\n```", Theme::dark());
    assert_eq!(
        lines.iter().map(Line::plain_text).collect::<Vec<_>>(),
        vec!["first", "", "second", ""]
    );
}

#[test]
fn v06a_paginated_fence_keeps_code_styling_on_later_pages() {
    let theme = Theme::dark();
    let source = format!("```rust\n{}\n```", "let count = 1;\n".repeat(300));
    let row = assistant(&source);
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, total) = visible_transcript(
        &[row],
        theme,
        40,
        40,
        (12, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert!(total > 300);
    let last = bottom
        .iter()
        .find(|line| line.plain_text().contains("let count = 1"))
        .expect("end of fenced block still reachable");
    assert_eq!(
        last.spans()
            .iter()
            .find(|span| span.content() == "let")
            .and_then(|span| span.style().fg),
        Some(theme.syntax(SyntaxToken::Keyword))
    );
}

#[test]
fn v06a_fenced_literal_table_at_page_boundary_remains_code() {
    let theme = Theme::dark();
    let text = format!(
        "```text\n{}| first | second |\n| --- | --- |\n| one | two |\n```",
        "line\n".repeat(127)
    );
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (visible, _) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        60,
        60,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    let plain: Vec<_> = visible.iter().map(Line::plain_text).collect();
    assert!(
        plain.iter().any(|line| line.contains("| --- | --- |")),
        "{plain:?}"
    );
    assert!(
        !plain.iter().any(|line| line.contains('┼')),
        "literal fence cannot become table"
    );
}

#[test]
fn v06a_completed_synthetic_part_without_footer_can_reach_tail() {
    // Frozen text before/after a tool is seq MAX with no per-part footer.
    let mut row = assistant(&format!("{}after-tool-tail", "content row\n".repeat(1800)));
    row.seq = i64::MAX;
    row.meta = None;
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, total) = visible_transcript(
        &[row],
        Theme::dark(),
        40,
        40,
        (10, 0, None),
        |_| Color::Reset,
        &cache,
    );
    assert!(total > 1000);
    assert!(
        bottom
            .iter()
            .any(|line| line.plain_text().contains("after-tool-tail"))
    );
}

#[test]
fn v06a_long_list_item_continuation_is_hanging_indented() {
    let rows = markdown_at_width(
        "- browser (45 tools) — tabs.open/list/focus, navigate, back, forward, reload, stop, preview",
        Theme::dark(),
        34,
    );
    assert!(rows.len() >= 2);
    assert!(rows[0].plain_text().starts_with("- browser"));
    assert!(
        rows.iter()
            .skip(1)
            .all(|r| r.plain_text().starts_with("  "))
    );
    assert!(
        rows.iter()
            .all(|r| r.spans().iter().map(styled::span_width).sum::<usize>() <= 34)
    );
}

#[test]
fn v06a_incomplete_streaming_blocks_reparse_only_the_dirty_part() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let rows = [
        assistant("# Заголовок"),
        assistant("```rust\nlet total = 1;"),
    ];
    let (visible, _) = visible_transcript(
        &rows,
        theme,
        40,
        40,
        (10, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert!(visible.iter().any(|r| r.plain_text().contains("Заголовок")));
    assert!(
        visible
            .iter()
            .any(|r| r.plain_text().contains("let total = 1;"))
    );
    assert!(!visible.iter().any(|r| r.plain_text().contains("```")));
    let mut changed = rows.to_vec();
    changed[1].text.push_str("\nlet next = 2;\n```");
    let (visible, _) = visible_transcript(
        &changed,
        theme,
        40,
        40,
        (10, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert!(
        visible
            .iter()
            .any(|r| r.plain_text().contains("let next = 2;"))
    );
    assert_eq!(
        cache.borrow().parses,
        3,
        "first completed block was not reparsed"
    );
}

#[test]
fn v06a_bounded_history_counts_parts_without_retaining_all_wrapped_rows() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let rows: Vec<_> = (0..240)
        .map(|index| {
            let mut row = assistant(&format!("## Part {index}\n\nSome `code` and text"));
            row.seq = index;
            row
        })
        .collect();
    let (bottom, total) = visible_transcript(
        &rows,
        theme,
        43,
        43,
        (24, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert_eq!(bottom.len(), 24);
    assert!(total > 24);
    assert!(bottom.iter().any(|r| r.plain_text().contains("Part 239")));
    assert!(
        cache.borrow().parses <= 24,
        "only viewport pages may be parsed"
    );
    let first_parses = cache.borrow().parses;
    let (earlier, _) = visible_transcript(
        &rows,
        theme,
        43,
        43,
        (24, 100, None),
        |_| theme.text(),
        &cache,
    );
    assert_eq!(earlier.len(), 24);
    assert!(!earlier.iter().any(|r| r.plain_text().contains("Part 239")));
    assert_eq!(
        first_parses + 12,
        cache.borrow().parses,
        "revisit cached completed blocks without reparsing"
    );
    assert!(cache.borrow().bytes <= MAX_CACHED_BYTES);
    assert!(cache.borrow().index_bytes() <= MAX_INDEX_BYTES);
}

#[test]
fn v06a_long_table_crosses_page_boundaries_without_repeated_header_or_missing_rows() {
    let theme = Theme::dark();
    let mut text = "| Инструмент | Назначение |\n| --- | --- |\n".to_string();
    for n in 0..150 {
        text.push_str(&format!("| item-{n:03} | 中文 \\| 🧑‍💻 `{n}` |\n"));
    }
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (end, total) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        74,
        74,
        (12, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert!(end.iter().any(|l| l.plain_text().contains("item-149")));
    assert!(total > 300);
    let mut actual = Vec::new();
    for scroll in (0..total).rev() {
        let (line, _) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            74,
            74,
            (1, scroll, None),
            |_| theme.text(),
            &cache,
        );
        actual.extend(line);
    }
    let mut expected = vec![Line::plain(""), Line::plain("")];
    expected.extend(markdown_block(&text, theme, 74));
    assert_eq!(actual.len(), expected.len(), "table pagination row count");
    for (n, (got, want)) in actual.iter().zip(&expected).enumerate() {
        if got.plain_text().is_empty() && want.plain_text().is_empty() {
            continue;
        }
        assert_eq!(
            styled::wrap_line(got, 74),
            styled::wrap_line(want, 74),
            "table styled grid differs at row {n}: {:?} vs {:?}",
            got.plain_text(),
            want.plain_text()
        );
    }
}

#[test]
fn v06a_long_table_with_wrapped_cells_stays_scrollable_at_both_widths() {
    let theme = Theme::dark();
    let mut text = String::from("| Key | Value |\n| --- | --- |\n");
    for n in 0..100 {
        text.push_str(&format!(
            "| item-{n:03} | escaped \\| 中文 🧑‍💻 many words wrapped across the cell |\n"
        ));
    }
    let cache = RefCell::new(MarkdownCache::default());
    for width in [42, 74] {
        let row = assistant(&text);
        let (_, total) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            width,
            width,
            (8, 0, None),
            |_| theme.text(),
            &cache,
        );
        let expected = markdown_block(&text, theme, width);
        assert_eq!(
            total,
            expected.len() + 2,
            "{width}: exact scroll count across table pages"
        );
        let (bottom, _) = visible_transcript(
            &[row],
            theme,
            width,
            width,
            (8, 0, None),
            |_| theme.text(),
            &cache,
        );
        assert!(
            bottom
                .iter()
                .any(|line| line.plain_text().contains("item-099")),
            "{width}: tail remains reachable"
        );
        let mut actual = Vec::new();
        let row = assistant(&text);
        for scroll in (0..total).rev() {
            actual.extend(
                visible_transcript(
                    std::slice::from_ref(&row),
                    theme,
                    width,
                    width,
                    (1, scroll, None),
                    |_| theme.text(),
                    &cache,
                )
                .0,
            );
        }
        let mut whole = vec![Line::plain(""), Line::plain("")];
        whole.extend(expected);
        assert_eq!(actual.len(), whole.len());
        for (n, (got, want)) in actual.iter().zip(&whole).enumerate() {
            assert_eq!(
                got.plain_text(),
                want.plain_text(),
                "{width}: content at row {n}"
            );
        }
    }
}

#[test]
fn v06a_table_over_512_rendered_rows_keeps_the_real_tail() {
    let mut text = String::from("| Key | Value |\n| --- | --- |\n");
    for n in 0..700 {
        text.push_str(&format!("| k{n:03} | value-{n:03} |\n"));
    }
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, total) = visible_transcript(
        std::slice::from_ref(&row),
        Theme::dark(),
        74,
        74,
        (10, 0, None),
        |_| Color::Reset,
        &cache,
    );
    assert!(total > 1400);
    assert!(bottom.iter().any(|l| l.plain_text().contains("value-699")));
    assert!(
        !bottom
            .iter()
            .any(|l| l.plain_text().contains("preview limited"))
    );
    assert!(
        cache.borrow().parses <= 2,
        "only the visible table page should parse"
    );
}

#[test]
fn v06a_oversized_grapheme_splitter_makes_progress() {
    let text = format!("a{}\n", "\u{0301}".repeat(5000));
    let pages = index_source(&text, 74);
    assert!(!pages.is_empty());
    assert_eq!(pages.first().unwrap().start, 0);
    assert_eq!(pages.last().unwrap().end, text.len());
    assert!(pages.iter().all(|page| page.start < page.end));
    let row = assistant(&format!("{text}\nafter-long-grapheme"));
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, _) = visible_transcript(
        std::slice::from_ref(&row),
        Theme::dark(),
        74,
        74,
        (8, 0, None),
        |_| Color::Reset,
        &cache,
    );
    assert!(
        bottom
            .iter()
            .any(|line| line.plain_text().contains("after-long-grapheme"))
    );
}

#[test]
fn v06a_large_table_cell_scrolls_as_grid_through_its_real_tail() {
    let text = format!(
        "| Key | Value |\n| --- | --- |\n| k | {}TAILCELL |\n",
        "word ".repeat(4500)
    );
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    for width in [42, 74] {
        let view = |scroll| {
            visible_transcript(
                std::slice::from_ref(&row),
                Theme::dark(),
                width,
                width,
                (10, scroll, None),
                |_| Color::Reset,
                &cache,
            )
        };
        let (bottom, total) = view(0);
        assert!(
            total > if width == 42 { 512 } else { 300 },
            "{width}: complete table spans more than one page"
        );
        assert!(
            bottom
                .iter()
                .any(|line| line.plain_text().contains("TAILCELL")),
            "{width}: lost cell tail"
        );
        assert!(
            bottom.iter().any(|line| line.plain_text().contains('└')),
            "{width}: missing table bottom"
        );
        let (top, _) = view(usize::MAX);
        assert!(
            top.iter()
                .any(|line| line.plain_text().contains("Key") && line.plain_text().contains('│')),
            "{width}: {:?}",
            top.iter().map(Line::plain_text).collect::<Vec<_>>()
        );
        for scroll in [0, total / 2, total.saturating_sub(10)] {
            let (lines, _) = view(scroll);
            assert!(
                !lines
                    .iter()
                    .any(|line| line.plain_text().contains("preview limited")),
                "{width}: cell preview truncated"
            );
            assert!(
                lines
                    .iter()
                    .filter(|line| line.plain_text().contains("word"))
                    .all(|line| line.plain_text().contains('│')),
                "{width}: table cell escaped the grid"
            );
            for line in &lines {
                for span in line
                    .spans()
                    .iter()
                    .filter(|span| span.content().contains("word"))
                {
                    assert_eq!(
                        span.style().fg,
                        Some(Theme::dark().markdown(MarkdownToken::Text)),
                        "{width}: cell styling changed at scroll {scroll}"
                    );
                }
            }
        }
        let mut words = 0;
        let mut keys = 0;
        let mut horizontal_separators = 0;
        for scroll in 0..total {
            let (lines, _) = visible_transcript(
                std::slice::from_ref(&row),
                Theme::dark(),
                width,
                width,
                (1, scroll, None),
                |_| Color::Reset,
                &cache,
            );
            for line in lines {
                let plain = line.plain_text();
                words += plain.matches("word").count();
                keys += plain.matches("│ k ").count();
                horizontal_separators += usize::from(plain.contains('┼'));
            }
        }
        assert_eq!(
            words, 4500,
            "{width}: all cell words survive semantic pages"
        );
        assert_eq!(
            keys, 1,
            "{width}: logical table row label appears exactly once"
        );
        assert_eq!(
            horizontal_separators, 1,
            "{width}: no phantom row separators"
        );
    }
}

#[test]
fn v06a_escaped_pipe_label_does_not_drop_long_cell_continuations() {
    let theme = Theme::dark();
    let text = format!(
        "| Key | Value |\n| --- | --- |\n| a \\| b | {}TAILCELL |\n| next | survives |",
        "word ".repeat(4500)
    );
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    for width in [42, 74] {
        let view = |scroll| {
            visible_transcript(
                std::slice::from_ref(&row),
                theme,
                width,
                width,
                (1, scroll, None),
                |_| theme.text(),
                &cache,
            )
        };
        let (bottom, total) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            width,
            width,
            (8, 0, None),
            |_| theme.text(),
            &cache,
        );
        assert!(
            total > if width == 42 { 512 } else { 300 },
            "{width}: missing table content"
        );
        let bottom: Vec<_> = bottom.iter().map(Line::plain_text).collect();
        assert!(
            bottom
                .iter()
                .any(|line| line.contains("TAILCELL") && line.contains('│')),
            "{width}: {bottom:?}"
        );
        assert!(
            bottom.iter().any(|line| line.contains("next")
                && line.contains("survives")
                && line.contains('│')),
            "{width}: {bottom:?}"
        );
        assert!(
            bottom.iter().any(|line| line.contains('└')),
            "{width}: {bottom:?}"
        );
        let (top, _) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            width,
            width,
            (8, usize::MAX, None),
            |_| theme.text(),
            &cache,
        );
        assert!(
            top.iter()
                .any(|line| line.plain_text().contains("Key") && line.plain_text().contains('│')),
            "{width}: {:?}",
            top.iter().map(Line::plain_text).collect::<Vec<_>>()
        );
        let mut words = 0;
        let mut labels = 0;
        let mut separators = 0;
        for scroll in 0..total {
            let (lines, _) = view(scroll);
            for line in lines {
                let plain = line.plain_text();
                assert!(
                    !plain.contains("preview limited"),
                    "{width}: page previewed"
                );
                if plain.contains("word") {
                    assert!(plain.contains('│'), "{width}: cell escaped grid");
                    assert!(
                        line.spans()
                            .iter()
                            .filter(|span| span.content().contains("word"))
                            .all(
                                |span| span.style().fg == Some(theme.markdown(MarkdownToken::Text))
                            )
                    );
                }
                words += plain.matches("word").count();
                labels += plain.matches("a | b").count();
                separators += usize::from(plain.contains('┼'));
            }
        }
        assert_eq!(words, 4500, "{width}: lost escaped-label cell words");
        assert_eq!(labels, 1, "{width}: duplicated escaped label");
        assert_eq!(
            separators, 2,
            "{width}: missing/duplicate grid row separator"
        );
    }
}

#[test]
fn v06a_blank_table_label_respects_escaped_and_inline_pipes() {
    // The synthetic first cell is empty on later visual slices. A pipe
    // inside either escape or code syntax must never become a delimiter.
    for prefix in ["| a \\| b | ", "| `a | b` | "] {
        let blank = blank_table_label(prefix);
        let real_pipes = blank.bytes().filter(|byte| *byte == b'|').count();
        assert_eq!(real_pipes, 2, "synthetic label added a column: {blank:?}");
        assert_eq!(blank.len(), prefix.len());
    }
}

#[test]
fn v06a_blockquoted_150_row_table_preserves_parser_structure_and_scroll_count() {
    let theme = Theme::dark();
    let mut text = String::from("> | Key | Value |\n> | --- | --- |\n");
    for n in 0..150 {
        text.push_str(&format!("> | k{n:03} | value-{n:03} extra stuff |\n"));
    }
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (end, total) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        74,
        74,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    let expected = markdown_block(&text, theme, 74);
    assert_eq!(
        total,
        expected.len() + 2,
        "quoted table rows cannot be counted as prose"
    );
    assert!(
        end.iter()
            .any(|line| line.plain_text().contains("value-149") && line.plain_text().contains('│'))
    );
    for scroll in [0, total / 2, total.saturating_sub(8)] {
        let (visible, _) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            74,
            74,
            (1, scroll, None),
            |_| theme.text(),
            &cache,
        );
        let expected_row = &expected[total - scroll - 3];
        assert_eq!(
            visible[0].plain_text(),
            expected_row.plain_text(),
            "scroll={scroll}: wrong quoted table content"
        );
        if expected_row.plain_text().contains('│') {
            assert_eq!(
                styled::wrap_line(&visible[0], 74),
                styled::wrap_line(expected_row, 74),
                "scroll={scroll}: wrong table styles"
            );
        }
    }
}

#[test]
fn v06a_blockquoted_long_cell_continues_into_the_next_real_table_row() {
    let text = format!(
        "> | Key | Value |\n> | --- | --- |\n> | k | {}TAILCELL |\n> | after | remains |",
        "word ".repeat(4500)
    );
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, total) = visible_transcript(
        std::slice::from_ref(&row),
        Theme::dark(),
        74,
        74,
        (10, 0, None),
        |_| Color::Reset,
        &cache,
    );
    assert!(total > 300);
    let plain: Vec<_> = bottom.iter().map(Line::plain_text).collect();
    assert!(
        plain
            .iter()
            .any(|r| r.contains("TAILCELL") && r.contains('│')),
        "{plain:?}"
    );
    assert!(
        plain
            .iter()
            .any(|r| r.contains("after") && r.contains("remains") && r.contains('│')),
        "{plain:?}"
    );
    assert!(plain.iter().any(|r| r.contains('└')), "{plain:?}");
    assert!(!plain.iter().any(|r| r.contains("preview limited")));
}

#[test]
fn v06a_table_offscreen_wide_label_does_not_change_earlier_column_width() {
    let theme = Theme::dark();
    let mut text = String::from("| Key | Value |\n| --- | --- |\n");
    for n in 0..140 {
        let key = if n == 139 {
            "very-long-label-at-the-tail".into()
        } else {
            format!("k{n}")
        };
        text.push_str(&format!("| {key} | value-{n} |\n"));
    }
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (_, total) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        74,
        74,
        (4, 0, None),
        |_| theme.text(),
        &cache,
    );
    let expected = markdown_block(&text, theme, 74);
    assert_eq!(total, expected.len() + 2);
    for scroll in [total - 10, total - 140, 0] {
        let (visible, _) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            74,
            74,
            (1, scroll, None),
            |_| theme.text(),
            &cache,
        );
        let want = &expected[total - scroll - 3];
        // Styled comparisons are normalized for same-color span merging.
        assert_eq!(visible[0].plain_text(), want.plain_text());
    }
}

#[test]
fn v06a_fixture_viewport_preserves_interblock_spacing() {
    let text = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tui-recovery/fixtures/transcript.md"
    ));
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let row = assistant(text);
    for width in [74, 114] {
        let (visible, total) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            width,
            width,
            (500, 0, None),
            |_| theme.text(),
            &cache,
        );
        let mut expected = vec![Line::plain(""), Line::plain("")];
        expected.extend(markdown_block(text, theme, width));
        assert_eq!(total, expected.len(), "width={width} scroll count");
        assert_eq!(
            visible.iter().map(Line::plain_text).collect::<Vec<_>>(),
            expected.iter().map(Line::plain_text).collect::<Vec<_>>(),
            "width={width} spacing"
        );
    }
}

#[test]
fn v06a_completed_single_long_line_remains_scrollable_past_decode_limit() {
    let text = format!("{}END-OF-COMPLETED-LINE", "word ".repeat(4500));
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (bottom, total) = visible_transcript(
        std::slice::from_ref(&row),
        Theme::dark(),
        60,
        60,
        (10, 0, None),
        |_| Color::Reset,
        &cache,
    );
    assert!(total > 300);
    assert!(
        bottom
            .iter()
            .any(|l| l.plain_text().contains("END-OF-COMPLETED-LINE")),
        "completed content after 16 KiB must be accessible"
    );
}

#[test]
fn v06a_ordered_list_and_blank_boundary_keep_context_and_row_count() {
    let theme = Theme::dark();
    let text = format!(
        "{}\n\nend",
        (1..=280)
            .map(|n| format!("{n}. entry-{n:03}\n"))
            .collect::<String>()
    );
    let row = assistant(&text);
    let cache = RefCell::new(MarkdownCache::default());
    let (_, total) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        60,
        60,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    let (end, _) = visible_transcript(
        &[row],
        theme,
        60,
        60,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert!(
        end.iter()
            .any(|l| l.plain_text().contains("280. entry-280"))
    );
    assert!(end.iter().any(|l| l.plain_text().contains("end")));
    assert_eq!(total, markdown_block(&text, theme, 60).len() + 2);
    assert!(!end.iter().any(|l| l.plain_text().contains("1. entry-280")));
    let repeated = format!(
        "{}\nend",
        (1..=280)
            .map(|n| format!("1. entry-{n:03}\n"))
            .collect::<String>()
    );
    let repeated_row = assistant(&repeated);
    let (end, total) = visible_transcript(
        std::slice::from_ref(&repeated_row),
        theme,
        60,
        60,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    assert_eq!(total, markdown_block(&repeated, theme, 60).len() + 2);
    assert!(
        end.iter()
            .any(|l| l.plain_text().contains("280. entry-280")),
        "{:?}",
        end.iter().map(Line::plain_text).collect::<Vec<_>>()
    );
}

#[test]
fn v06a_visible_page_only_parse_stays_cached_across_241_and_10000_parts_and_resize() {
    let theme = Theme::dark();
    let cache = RefCell::new(MarkdownCache::default());
    let rows: Vec<_> = (0..10_000)
        .map(|n| {
            let mut row = assistant(&format!("## part-{n:05}"));
            row.seq = n;
            row
        })
        .collect();
    for count in [241, 10_000] {
        let render = |width| {
            visible_transcript(
                &rows[..count],
                theme,
                width,
                width,
                (12, 0, None),
                |_| theme.text(),
                &cache,
            )
        };
        let (lines, total) = render(60);
        assert!(
            lines
                .iter()
                .any(|l| l.plain_text().contains(&format!("part-{:05}", count - 1)))
        );
        assert!(total >= count * 2);
        let first = cache.borrow().parses;
        assert!(first < 32, "first frame parsed {first} invisible parts");
        assert_eq!(lines, render(60).0);
        assert_eq!(
            cache.borrow().parses,
            first,
            "second frame reparsed cached viewport"
        );
        render(40);
        assert!(
            cache.borrow().parses <= first + 16,
            "width invalidation must affect visible parts only"
        );
        assert!(cache.borrow().bytes <= MAX_CACHED_BYTES);
        assert!(cache.borrow().index_bytes() <= MAX_INDEX_BYTES);
    }
}

/// Code blocks: fenced language selection, token colors for the supported
/// subset, and the code-block color for unknown languages.
#[test]
fn markdown_code_blocks_highlight_the_supported_subset() {
    let theme = Theme::dark();
    let base = theme.markdown(MarkdownToken::CodeBlock);

    let lines = markdown(
        "```python\ndef f(x):  # note\n    return \"s\" + 1\n```",
        theme,
    );
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0].plain_text(), "def f(x):  # note");
    let styles: Vec<Color> = lines[0]
        .spans()
        .iter()
        .map(|span| span.style().fg.unwrap_or(Color::Reset))
        .collect();
    assert_eq!(styles[0], theme.syntax(SyntaxToken::Keyword));
    assert!(styles.contains(&theme.syntax(SyntaxToken::Function)));
    assert!(styles.contains(&theme.syntax(SyntaxToken::Comment)));
    assert!(lines[1].spans().iter().any(|span| {
        span.style()
            .fg
            .is_some_and(|fg| fg == theme.syntax(SyntaxToken::String))
    }));
    assert!(lines[1].spans().iter().any(|span| {
        span.style()
            .fg
            .is_some_and(|fg| fg == theme.syntax(SyntaxToken::Number))
    }));

    // Unknown language: one base-styled span, text intact.
    let lines = markdown("```brainfuck\n+++[>+++<-]\n```", theme);
    assert_eq!(lines[0].plain_text(), "+++[>+++<-]");
    assert_eq!(lines[0].spans().len(), 1);
    assert_eq!(lines[0].spans()[0].style().fg, Some(base));

    // An unclosed fence streams as code, never as prose.
    let lines = markdown("```rust\nlet x = 1;", theme);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].plain_text(), "let x = 1;");
    assert_eq!(
        lines[0].spans()[0].style().fg,
        Some(theme.syntax(SyntaxToken::Keyword))
    );

    // Indented text is prose, not a code block (upstream highlights only
    // fenced blocks in this subset).
    let lines = markdown("    indented", theme);
    assert_eq!(lines[0].plain_text(), "indented");
    assert_eq!(
        lines[0].spans()[0].style().fg,
        Some(theme.markdown(MarkdownToken::Text))
    );
}

#[test]
fn fixture_prose_wrap_paints_source_separator_in_full_and_indexed_rows() {
    let fixture = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tui-recovery/fixtures/transcript.md"
    ));
    let source = fixture.lines().last().expect("browser list item");
    assert!(source.contains("`navigate`, `back`"));
    let theme = Theme::dark();
    let row = assistant(source);
    let width = 114;
    let full = transcript(std::slice::from_ref(&row), theme, width, width, |_| {
        theme.text()
    });
    let cache = RefCell::new(MarkdownCache::default());
    let (indexed, total) = visible_transcript(
        std::slice::from_ref(&row),
        theme,
        width,
        width,
        (8, 0, None),
        |_| theme.text(),
        &cache,
    );
    let paint = |lines: Vec<Line>| {
        let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Block::default().style(Style::default().fg(theme.text())),
                    frame.area(),
                );
                frame.render_widget(
                    Paragraph::new(styled::Lines::from(lines).into_text()),
                    frame.area(),
                );
            })
            .unwrap();
        terminal.backend().buffer().clone()
    };
    let full_text = full.iter().map(Line::plain_text).collect::<Vec<_>>();
    let indexed_text = indexed.iter().map(Line::plain_text).collect::<Vec<_>>();
    assert_eq!(indexed_text[1..], full_text, "indexed leading blank");
    assert_eq!(total, indexed.len());
    let y = full_text
        .iter()
        .position(|line| line.ends_with("navigate, "))
        .unwrap_or_else(|| panic!("source delimiter at wrap: {full_text:?}"));
    let x = UnicodeWidthStr::width(full_text[y].as_str()) as u16 - 1;
    assert!(x < width);
    for (buffer, y) in [(paint(full), y as u16), (paint(indexed), y as u16 + 1)] {
        assert_eq!(buffer[(x, y)].symbol(), " ");
        assert_eq!(buffer[(x, y)].fg, theme.markdown(MarkdownToken::Text));
        assert_eq!(buffer[(x + 1, y)].fg, theme.text());
    }
}

#[test]
fn fenced_code_preserves_only_source_separator_on_word_wrap() {
    // In the pinned rows-reflow fixture ROW-000..040 have a source
    // separator before a long x word; ROW-041..089 do not.
    let theme = Theme::dark();
    let paint = |lines: Vec<Line>, width: u16| {
        let wrapped = styled::wrap_lines(&lines, width as usize);
        let count = wrapped.len();
        let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Block::default().style(Style::default().fg(theme.text())),
                    frame.area(),
                );
                frame.render_widget(
                    Paragraph::new(styled::Lines::from(wrapped).into_text()),
                    frame.area(),
                );
            })
            .unwrap();
        (count, terminal.backend().buffer().clone())
    };
    for (width, code, expected, separator) in [
        (
            80,
            format!("ROW-014 {}", "x".repeat(90)),
            vec!["ROW-014 ".to_string(), "x".repeat(77), "x".repeat(13)],
            true,
        ),
        (
            80,
            "ROW-041".to_string(),
            vec!["ROW-041".to_string()],
            false,
        ),
        (
            10,
            "ROW-041".to_string(),
            vec!["ROW-041".to_string()],
            false,
        ),
    ] {
        let row = assistant(&format!("```text\n{code}\n```"));
        let full = transcript(std::slice::from_ref(&row), theme, width, width, |_| {
            theme.text()
        });
        let cache = RefCell::new(MarkdownCache::default());
        let (visible, total) = visible_transcript(
            std::slice::from_ref(&row),
            theme,
            width,
            width,
            (8, 0, None),
            |_| theme.text(),
            &cache,
        );
        for (index, expected_row) in expected.iter().enumerate() {
            let text = format!("   {expected_row}");
            assert_eq!(full[index + 1].plain_text(), text, "width {width}");
            assert_eq!(
                visible[index + 2].plain_text(),
                text,
                "indexed width {width}"
            );
        }
        let (full_count, full_buffer) = paint(full, width);
        let (visible_count, visible_buffer) = paint(visible, width);
        assert_eq!(
            full_count,
            expected.len() + 1,
            "width {width}: no extra code row"
        );
        assert_eq!(
            visible_count,
            expected.len() + 2,
            "width {width}: indexed leading blank + code"
        );
        assert_eq!(total, expected.len() + 2, "width {width}: seek height");
        let x = MESSAGE_PADDING as u16 + 7;
        for (buffer, y) in [(&full_buffer, 1), (&visible_buffer, 2)] {
            assert_eq!(
                buffer[(x - 1, y)].symbol(),
                if separator { "4" } else { "1" },
                "width {width}"
            );
            if separator {
                assert_eq!(buffer[(x, y)].symbol(), " ");
                assert_eq!(
                    buffer[(x, y)].fg,
                    theme.markdown(MarkdownToken::CodeBlock),
                    "width {width}"
                );
                assert_eq!(
                    buffer[(x + 1, y)].fg,
                    theme.text(),
                    "width {width}: no synthetic cell"
                );
            } else if x < width {
                assert_eq!(
                    buffer[(x, y)].fg,
                    theme.text(),
                    "width {width}: short row blank stays default"
                );
            } else {
                assert_eq!(x, width, "width {width}: exact fit");
            }
        }
        for (index, _) in expected.iter().enumerate() {
            for x in 0..width {
                assert_eq!(
                    full_buffer[(x, index as u16 + 1)],
                    visible_buffer[(x, index as u16 + 2)],
                    "width {width}, row {index}, x {x}"
                );
            }
        }
    }

    let prose = assistant("ROW-041");
    let (count, buffer) = paint(transcript(&[prose], theme, 80, 80, |_| theme.text()), 80);
    assert_eq!(count, 2);
    assert_eq!(buffer[(10, 1)].fg, theme.text(), "prose tail stays default");
}

/// `reasoningSummary` (`context/thinking.ts:10-19`) extracts only a leading
/// `**Title**` block; everything else stays title-less.
#[test]
fn reasoning_title_matches_the_upstream_summary_rule() {
    assert_eq!(
        reasoning_title("**Inspecting PR**\n\nbody"),
        "Inspecting PR"
    );
    assert_eq!(reasoning_title("**Only title**"), "Only title");
    assert_eq!(reasoning_title("**Title** rest"), "");
    assert_eq!(reasoning_title("plain text"), "");
    assert_eq!(reasoning_title("**multi\nline**\n\nbody"), "");
    assert_eq!(reasoning_title("**inner*star**\n\nbody"), "");
    assert_eq!(reasoning_title(""), "");
}

/// `Locale.titlecase` and `Locale.duration` (`util/locale.ts:3-5,35-57`).
#[test]
fn locale_helpers_match_upstream_formatting() {
    assert_eq!(Locale::titlecase("build"), "Build");
    assert_eq!(Locale::titlecase("my-agent_2"), "My-Agent_2");
    assert_eq!(Locale::titlecase(""), "");

    assert_eq!(Locale::duration(0), "0ms");
    assert_eq!(Locale::duration(999), "999ms");
    assert_eq!(Locale::duration(1_000), "1.0s");
    assert_eq!(Locale::duration(1_500), "1.5s");
    assert_eq!(Locale::duration(59_999), "60.0s");
    assert_eq!(Locale::duration(60_000), "1m 0s");
    assert_eq!(Locale::duration(61_000), "1m 1s");
    assert_eq!(Locale::duration(3_600_000), "1h 0m");
    assert_eq!(Locale::duration(86_400_000), "1d 0h");
}

/// Missing data omits the field; nothing is printed as a zero.
#[test]
fn footer_omits_unavailable_fields() {
    let theme = Theme::dark();
    let none = footer_line(None, &AssistantMeta::default(), theme, 80, &|_| {
        theme.text()
    });
    assert!(none.is_none());
    let tokens_without_duration = footer_line(
        Some("build"),
        &AssistantMeta {
            output_tokens: Some(10),
            streamed_ms: Some(0),
            ..AssistantMeta::default()
        },
        theme,
        80,
        &|_| theme.text(),
    )
    .expect("agent field");
    assert_eq!(tokens_without_duration.plain_text(), "   Build");
}
