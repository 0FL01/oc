//! Confirmed mutation transcript. All coordinates come from the mutation owner.
use crate::{
    history::ToolCard,
    styled::{self, Line, RowCopy, Span},
    theme::Theme,
};
use oc_core::{
    patch::{FileEffect, PatchEffects, PatchLine, PatchLineKind, PatchOperation},
    queries::{DiffView, DiffWrap},
};
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Proposed image, rendered independently of committed mutation cards.
pub(crate) fn preview(file: &FileEffect, theme: &Theme, width: u16, split: bool) -> Vec<Line> {
    let width = usize::from(width).max(1);
    let digits = file
        .hunks
        .iter()
        .flat_map(|h| &h.lines)
        .flat_map(|l| [l.old_line, l.new_line])
        .flatten()
        .max()
        .unwrap_or(1)
        .to_string()
        .len();
    let mut out = Vec::new();
    for hunk in &file.hunks {
        let path = file.destination.as_ref().unwrap_or(&file.path);
        let (old, old_error) = syntax_image(&hunk.lines, false, path, theme);
        let (new, new_error) = syntax_image(&hunk.lines, true, path, theme);
        if split && width > 2 * (digits + 4) {
            for (left, right) in pairs(&hunk.lines) {
                let left = side(
                    left.map(|index| &hunk.lines[index]),
                    false,
                    left.map(|index| &old[index]),
                    theme,
                    digits,
                    width.div_ceil(2),
                    DiffWrap::Word,
                );
                let right = side(
                    right.map(|index| &hunk.lines[index]),
                    true,
                    right.map(|index| &new[index]),
                    theme,
                    digits,
                    width / 2,
                    DiffWrap::Word,
                );
                for i in 0..left.len().max(right.len()) {
                    let mut row = left.get(i).cloned().unwrap_or_else(|| {
                        fit(
                            Vec::new(),
                            width.div_ceil(2),
                            Style::default().bg(theme.diff_context_background()),
                        )
                    });
                    row.extend(right.get(i).cloned().unwrap_or_default());
                    out.push(Line::new(row));
                }
            }
        } else {
            for (index, line) in hunk.lines.iter().enumerate() {
                out.extend(
                    side(
                        Some(line),
                        line.kind != PatchLineKind::Removed,
                        Some(if line.kind == PatchLineKind::Removed {
                            &old[index]
                        } else {
                            &new[index]
                        }),
                        theme,
                        digits,
                        width,
                        DiffWrap::Word,
                    )
                    .into_iter()
                    .map(Line::new),
                );
            }
        }
        if old_error.is_some() || new_error.is_some() {
            out.push(Line::styled(
                "[syntax highlighting preview limited]",
                Style::default().fg(theme.text_muted()),
            ));
        }
    }
    if out.is_empty() {
        out.push(Line::plain("No diff provided"));
    }
    out
}

pub(crate) fn render(
    effects: &PatchEffects,
    card: &ToolCard,
    theme: &Theme,
    width: u16,
) -> Vec<Line> {
    let unbounded = width == 0;
    let width = if unbounded {
        effects
            .files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .map(|l| l.text.len() + 32)
            .max()
            .unwrap_or(80)
    } else {
        usize::from(width)
    };
    let inner = width.saturating_sub(4).max(1);
    let base = Style::default()
        .fg(theme.text())
        .bg(theme.background_raised());
    // Empty renderable cells inherit the terminal foreground, not text.base.
    let padding = Style::default().bg(theme.background_raised());
    let muted = base.fg(theme.text_muted());
    let frame = |spans: Vec<Span>| {
        let mut row = vec![
            Span::decor("┃", base.fg(theme.background())),
            Span::decor("  ", padding),
        ];
        if unbounded {
            row.extend(spans);
            Line::new(row)
        } else {
            row.extend(fit(spans, width.saturating_sub(3), padding));
            Line::new(fit(row, width, padding))
        }
    };
    let mut out = Vec::new();
    if card.name == "write" && card.state == "completed" {
        let write = card
            .output_presentation
            .as_ref()
            .filter(|presentation| presentation.is_valid())
            .and_then(|presentation| presentation.write.as_ref());
        out.push(frame(Vec::new()));
        let path = write.map(|write| write.path.as_str()).or_else(|| {
            effects
                .files
                .first()
                .map(|file| file.destination.as_deref().unwrap_or(&file.path))
        });
        out.push(frame(vec![
            Span::styled("# Wrote", muted),
            Span::styled(" ", padding),
            Span::styled(path.unwrap_or("[recorded path unavailable]"), muted),
        ]));
        out.push(frame(Vec::new()));
        if let Some(write) = write {
            let source = write
                .content
                .replace("\r\n", "\n")
                .split('\n')
                .map(crate::syntax::safe_line)
                .collect::<Vec<_>>()
                .join("\n");
            let filetype = crate::syntax::filetype(&write.path);
            let (rows, syntax_error) =
                match crate::syntax::highlight(&source, filetype, theme, base) {
                    Ok(rows) => (rows, false),
                    Err(_) => (
                        source
                            .split('\n')
                            .map(|text| Line::styled(text, base))
                            .collect(),
                        true,
                    ),
                };
            let rows = rows
                .iter()
                .map(crate::syntax::display_line)
                .collect::<Vec<_>>();
            // OpenTUI's minimum three cells already include both padding cells;
            // its digits are based on the target's wrapped virtual line count.
            let mut gutter_width = rows.len().max(1).ilog10() as usize + 3;
            if !unbounded {
                loop {
                    let mut virtual_rows = 0usize;
                    for row in &rows {
                        styled::visit_source_space_line(
                            row,
                            width.saturating_sub(3 + gutter_width).max(1),
                            |_| virtual_rows += 1,
                        );
                    }
                    let measured = virtual_rows.max(1).ilog10() as usize + 3;
                    if measured == gutter_width {
                        break;
                    }
                    gutter_width = measured;
                }
            }
            let number_width = gutter_width - 1;
            let mut displayed = 0usize;
            let mut display_limited = false;
            for (index, row) in rows.iter().enumerate() {
                let remaining = 512usize.saturating_sub(displayed);
                if remaining == 0 {
                    display_limited = true;
                    break;
                }
                let wraps = if unbounded {
                    vec![row.clone()]
                } else {
                    // Continuations retain the measured gutter without inventing
                    // source line numbers; the output remains bounded below.
                    styled::wrap_code_line_limited(
                        row,
                        width.saturating_sub(3 + gutter_width).max(1),
                        remaining + 1,
                    )
                };
                if wraps.len() > remaining {
                    display_limited = true;
                }
                for (wrap_index, row) in wraps.into_iter().take(remaining).enumerate() {
                    // The original draws only the digits with gutter fg. Its
                    // left/right padding inherits the terminal foreground.
                    let mut spans = if wrap_index == 0 {
                        let number = (index + 1).to_string();
                        vec![
                            Span::decor(" ".repeat(number_width - number.len()), padding),
                            Span::decor(number, muted),
                            Span::decor(" ", padding),
                        ]
                    } else {
                        vec![Span::decor(" ".repeat(gutter_width), padding)]
                    };
                    spans.extend_from_slice(row.spans());
                    out.push(frame(spans).with_copy(if wrap_index == 0 {
                        RowCopy::Text
                    } else {
                        RowCopy::Wrap
                    }));
                    displayed += 1;
                }
            }
            if syntax_error {
                out.push(frame(vec![Span::styled(
                    "[syntax highlighting preview limited]",
                    muted,
                )]));
            }
            if write.content_limited || write.path_limited || display_limited {
                out.push(frame(vec![Span::styled(
                    "[Write content preview limited]",
                    muted,
                )]));
            }
        } else {
            // Never fall back to unredacted requested content or reconstruct a
            // complete overwrite from partial diff hunks. The actual bounded
            // effect DTO remains unchanged and its counts stay visible.
            out.push(frame(vec![Span::styled(
                "[recorded Write content preview unavailable]",
                muted,
            )]));
            out.push(frame(vec![Span::styled(
                format!(
                    "{} confirmed files, +{} -{}",
                    effects.total_files, effects.additions, effects.deletions
                ),
                muted,
            )]));
        }
        out.push(frame(Vec::new()));
        return out;
    }
    let failed = card.state != "completed" && card.state != "started";
    if failed {
        out.push(frame(vec![Span::styled(
            match (card.name.as_str(), card.state.as_str()) {
                ("write", "unknown") => "# Write outcome unknown",
                ("edit", "unknown") => "# Edit outcome unknown",
                (_, "unknown") => "# Patch outcome unknown",
                ("write", _) => "# Write failed",
                ("edit", _) => "# Edit failed",
                _ => "# Patch failed",
            },
            base.fg(theme.error()),
        )]));
        if !effects.files.is_empty() {
            out.push(frame(vec![Span::styled(
                "Confirmed effects before failure:",
                muted,
            )]));
        }
    }
    let split = !unbounded
        && match card.diff_settings.view {
            DiffView::Split => true,
            DiffView::Unified => false,
            // U18 session/index.tsx:237,1248-1249,3412: ctx.width is
            // the pane's content width, after root padding and side panes.
            DiffView::Auto => width > 120,
        };
    for (file_index, file) in effects.files.iter().enumerate() {
        if file_index > 0 {
            out.push(Line::plain(""));
        }
        out.push(frame(Vec::new()));
        let label = match (card.name.as_str(), file.operation) {
            ("write", _) => "# Wrote",
            ("edit", _) => "← Edit",
            (_, PatchOperation::Create) => "# Created",
            (_, PatchOperation::Delete) => "# Deleted",
            _ => "← Patched",
        };
        let path = file.destination.as_ref().unwrap_or(&file.path);
        // U18 BlockTool's row has separate label/path children and gap={1}
        // (session/index.tsx:2836-2856); the gap is a canvas cell.
        let mut header = vec![Span::styled(label, muted)];
        let mut trailing_marker = None;
        if file_index == 0 && effects.truncated {
            let heading_width =
                3 + UnicodeWidthStr::width(label) + 1 + UnicodeWidthStr::width(path.as_str());
            let marker = if unbounded || heading_width + " [preview truncated]".len() <= width {
                " [preview truncated]"
            } else {
                " [preview…]"
            };
            if unbounded || heading_width + UnicodeWidthStr::width(marker) <= width {
                header.push(Span::styled(marker, muted));
            } else {
                trailing_marker = Some(marker);
            }
        }
        header.extend([Span::styled(" ", padding), Span::styled(path, muted)]);
        if let Some(marker) = trailing_marker {
            header.push(Span::styled(marker, muted));
        }
        out.push(frame(header));
        // U18 ApplyPatch paints the actual destination in the header only.
        // The source remains in the effect DTO, not a second transcript row.
        if file.operation == PatchOperation::Delete {
            out.push(frame(Vec::new()));
            out.push(frame(vec![Span::styled(
                format!(
                    "-{} line{}",
                    file.deletions,
                    if file.deletions == 1 { "" } else { "s" }
                ),
                base.fg(theme.diff_removed()),
            )]));
        } else {
            if !file.hunks.is_empty() {
                out.push(frame(Vec::new()));
            }
            let digits = file
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .flat_map(|l| [l.old_line, l.new_line])
                .flatten()
                .max()
                .unwrap_or(1)
                .to_string()
                .len();
            for (index, hunk) in file.hunks.iter().enumerate() {
                let (old_syntax, old_error) = syntax_image(&hunk.lines, false, path, theme);
                let (new_syntax, new_error) = syntax_image(&hunk.lines, true, path, theme);
                if index > 0 {
                    out.push(frame(vec![
                        Span::decor(" ", padding),
                        Span::styled(
                            format!(
                                " @@ -{},{} +{},{} @@",
                                hunk.old.start, hunk.old.count, hunk.new.start, hunk.new.count
                            ),
                            base.fg(theme.diff_hunk_header())
                                .bg(theme.diff_context_background()),
                        ),
                    ]));
                }
                if split && inner > 2 * (digits + 4) {
                    // U18's equal flex surfaces round the left half up. Actual
                    // captures16/18 retain the extra left cell at odd widths.
                    let left_width = inner.div_ceil(2);
                    let right_width = inner - left_width;
                    for (left, right) in pairs(&hunk.lines) {
                        let left = side(
                            left.map(|index| &hunk.lines[index]),
                            false,
                            left.map(|index| &old_syntax[index]),
                            theme,
                            digits,
                            left_width,
                            card.diff_settings.wrap,
                        );
                        let right = side(
                            right.map(|index| &hunk.lines[index]),
                            true,
                            right.map(|index| &new_syntax[index]),
                            theme,
                            digits,
                            right_width,
                            card.diff_settings.wrap,
                        );
                        for i in 0..left.len().max(right.len()) {
                            let mut spans = left.get(i).cloned().unwrap_or_else(|| {
                                fit(
                                    Vec::new(),
                                    left_width,
                                    Style::default().bg(theme.diff_context_background()),
                                )
                            });
                            spans.extend(right.get(i).cloned().unwrap_or_else(|| {
                                fit(
                                    Vec::new(),
                                    right_width,
                                    Style::default().bg(theme.diff_context_background()),
                                )
                            }));
                            spans.insert(0, Span::decor(" ", padding));
                            out.push(frame(spans).with_copy(RowCopy::Text));
                        }
                    }
                } else {
                    for (index, line) in hunk.lines.iter().enumerate() {
                        let new = line.kind != PatchLineKind::Removed;
                        for (spans_index, mut spans) in side(
                            Some(line),
                            new,
                            Some(if new {
                                &new_syntax[index]
                            } else {
                                &old_syntax[index]
                            }),
                            theme,
                            digits,
                            inner,
                            card.diff_settings.wrap,
                        )
                        .into_iter()
                        .enumerate()
                        {
                            spans.insert(0, Span::decor(" ", padding));
                            out.push(frame(spans).with_copy(if spans_index == 0 {
                                RowCopy::Text
                            } else {
                                RowCopy::Wrap
                            }));
                        }
                    }
                }
                if old_error.is_some() || new_error.is_some() {
                    out.push(frame(vec![Span::styled(
                        "[syntax highlighting preview limited]",
                        muted,
                    )]));
                }
            }
        }
        out.push(frame(Vec::new()));
    }
    if let Some(capture) = card
        .output_presentation
        .as_ref()
        .filter(|presentation| presentation.is_valid())
        .and_then(|presentation| presentation.capture.as_ref())
        .filter(|capture| {
            effects.total_files > 0
                && (capture.state != oc_core::tool_output::CaptureState::Complete
                    || (effects.truncated && capture.reference.is_none()))
        })
    {
        let reference = capture.reference.as_ref().map_or_else(
            || "no usable path".into(),
            |path| {
                format!(
                    "read(path={})",
                    serde_json::to_string(path).expect("path JSON")
                )
            },
        );
        let spans = vec![Span::styled(
            format!("Recorded capture: {:?}; {reference}", capture.state),
            muted,
        )];
        if unbounded {
            out.push(frame(spans));
        } else {
            out.extend(
                wrap_text(spans, width.saturating_sub(3).max(1), DiffWrap::Word)
                    .into_iter()
                    .map(frame),
            );
        }
    }
    if effects.files.is_empty() && !failed {
        out.push(frame(vec![Span::styled(
            "No confirmed file changes",
            muted,
        )]));
    }
    if failed {
        for line in crate::styled::display_output(&card.output_preview).lines() {
            out.push(frame(vec![Span::styled(
                line.to_owned(),
                base.fg(theme.error()),
            )]));
        }
    }
    out
}

/// Align each removed/added run, preserving both independently numbered sides.
fn pairs(lines: &[PatchLine]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind == PatchLineKind::Context {
            out.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let start = i;
        while i < lines.len() && lines[i].kind != PatchLineKind::Context {
            i += 1;
        }
        let removed: Vec<_> = (start..i)
            .filter(|&index| lines[index].kind == PatchLineKind::Removed)
            .collect();
        let added: Vec<_> = (start..i)
            .filter(|&index| lines[index].kind == PatchLineKind::Added)
            .collect();
        for n in 0..removed.len().max(added.len()) {
            out.push((removed.get(n).copied(), added.get(n).copied()));
        }
    }
    out
}

/// Parse each bounded old/new hunk image once. Added text is never used as old
/// parser context (or vice versa); unavailable file bytes are not fabricated.
fn syntax_image(
    lines: &[PatchLine],
    new: bool,
    path: &str,
    theme: &Theme,
) -> (Vec<Line>, Option<crate::syntax::Error>) {
    let included: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            line.kind == PatchLineKind::Context
                || line.kind
                    == if new {
                        PatchLineKind::Added
                    } else {
                        PatchLineKind::Removed
                    }
        })
        .map(|(index, line)| (index, crate::syntax::safe_line(&line.text)))
        .collect();
    let source = included
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let base = Style::default().fg(theme.text());
    let (highlighted, error) =
        match crate::syntax::highlight(&source, crate::syntax::filetype(path), theme, base) {
            Ok(rows) => (rows, None),
            Err(error) => (
                included
                    .iter()
                    .map(|(_, text)| Line::styled(text, base))
                    .collect(),
                Some(error),
            ),
        };
    let mut rows = vec![Line::plain(""); lines.len()];
    for ((index, _), highlighted) in included.into_iter().zip(highlighted) {
        rows[index] = crate::syntax::display_line(&highlighted);
    }
    (rows, error)
}

fn side(
    line: Option<&PatchLine>,
    new: bool,
    highlighted: Option<&Line>,
    theme: &Theme,
    digits: usize,
    width: usize,
    wrap: DiffWrap,
) -> Vec<Vec<Span>> {
    let context = Style::default().bg(theme.diff_context_background());
    let Some(line) = line else {
        return vec![fit(Vec::new(), width, context)];
    };
    let (sign, bg, sign_color, gutter_bg) = match line.kind {
        PatchLineKind::Added => (
            "+",
            theme.diff_added_background(),
            theme.diff_highlight_added(),
            theme
                .color("diff.lineNumber.background.added")
                .unwrap_or(theme.diff_added_background()),
        ),
        PatchLineKind::Removed => (
            "-",
            theme.diff_removed_background(),
            theme.diff_highlight_removed(),
            theme
                .color("diff.lineNumber.background.removed")
                .unwrap_or(theme.diff_removed_background()),
        ),
        PatchLineKind::Context => (
            " ",
            theme.diff_context_background(),
            theme.text(),
            theme.diff_context_background(),
        ),
    };
    let base = context.fg(theme.text()).bg(bg);
    let fill = Style::default().bg(bg);
    let gutter = Style::default().fg(theme.diff_line_number()).bg(gutter_bg);
    let gutter_fill = Style::default().bg(gutter_bg);
    let number = if new { line.new_line } else { line.old_line };
    let gutter_width = digits + 4;
    let text_width = width.saturating_sub(gutter_width).max(1);
    let mut text = highlighted.map_or_else(Vec::new, |row| {
        row.spans()
            .iter()
            .map(|span| Span::styled(span.content(), base.patch(span.style())))
            .collect()
    });
    if line.truncated {
        text.push(Span::styled("… [truncated]", base));
    }
    if line.lossy {
        text.push(Span::styled(" [lossy byte preview]", base));
    }
    let rows = wrap_text(text, text_width, wrap);
    let mut out = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        let number = if i == 0 {
            number.map_or(String::new(), |n| n.to_string())
        } else {
            String::new()
        };
        let mut spans = vec![
            Span::decor(
                " ".repeat(1 + digits.saturating_sub(number.len())),
                gutter_fill,
            ),
            Span::decor(number, gutter),
            Span::decor(
                format!(" {}", if i == 0 { sign } else { " " }),
                if i == 0 && line.kind != PatchLineKind::Context {
                    gutter_fill.fg(sign_color)
                } else {
                    gutter_fill
                },
            ),
            Span::decor(" ", gutter_fill),
        ];
        spans.extend(fit(row, text_width, fill));
        out.push(fit(spans, width, fill));
    }
    out
}

fn wrap_text(spans: Vec<Span>, width: usize, wrap: DiffWrap) -> Vec<Vec<Span>> {
    if wrap == DiffWrap::Word {
        return styled::wrap_code_line_limited(&Line::new(spans), width, usize::MAX)
            .into_iter()
            .map(|l| l.spans().to_vec())
            .collect();
    }
    vec![spans]
}

fn fit(spans: Vec<Span>, width: usize, base: Style) -> Vec<Span> {
    let mut out = Vec::new();
    let mut used = 0;
    'spans: for span in spans {
        // Printable ASCII is one single-cell grapheme per byte.
        let content = span.content();
        if content.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            let take = content.len().min(width - used);
            if take > 0 {
                out.push(span.with_content(content[..take].to_owned()));
            }
            used += take;
            if take < content.len() {
                break;
            }
            continue;
        }
        let mut text = String::new();
        let safe: String = span
            .content()
            .chars()
            .map(|c| if c.is_control() { '�' } else { c })
            .collect();
        for glyph in safe.graphemes(true) {
            let cells = UnicodeWidthStr::width(glyph);
            if used + cells > width {
                if !text.is_empty() {
                    out.push(span.with_content(text));
                }
                break 'spans;
            }
            text.push_str(glyph);
            used += cells;
        }
        if !text.is_empty() {
            out.push(span.with_content(text));
        }
    }
    if used < width {
        out.push(Span::decor(" ".repeat(width - used), base));
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::history::{HistoryRow, card_from_row};
    use oc_core::{
        patch::{DiffAlgorithm, LineEnding, LineRange, PatchHunk},
        queries::{DiffSettings, ToolOpView},
    };

    pub(crate) fn effects() -> PatchEffects {
        PatchEffects {
            total_files: 1,
            additions: 1,
            deletions: 1,
            truncated: false,
            files: vec![FileEffect {
                algorithm: DiffAlgorithm::UniqueAnchors,
                operation: PatchOperation::Update,
                path: "a.txt".into(),
                destination: None,
                additions: 1,
                deletions: 1,
                truncated: false,
                hunks: vec![PatchHunk {
                    old: LineRange {
                        start: 99,
                        count: 2,
                    },
                    new: LineRange {
                        start: 100,
                        count: 2,
                    },
                    truncated: false,
                    lines: vec![
                        patch_line(PatchLineKind::Context, Some(99), Some(100), "context"),
                        patch_line(PatchLineKind::Removed, Some(100), None, "old"),
                        patch_line(PatchLineKind::Added, None, Some(101), "let value = 42;"),
                    ],
                }],
            }],
        }
    }
    fn patch_line(
        kind: PatchLineKind,
        old_line: Option<usize>,
        new_line: Option<usize>,
        text: &str,
    ) -> PatchLine {
        PatchLine {
            kind,
            old_line,
            new_line,
            text: text.into(),
            ending: LineEnding::Lf,
            truncated: false,
            lossy: false,
        }
    }
    pub(super) fn row(effects: Option<PatchEffects>, state: &str) -> ToolOpView {
        ToolOpView { child_job: None, output_presentation: None, question: None, rowid: 1, op: "actual".into(), name: "apply_patch".into(), state: state.into(),
            input: Some(r#"{"patchText":"*** Begin Patch\n*** Add File: fake.txt\n+invented\n*** End Patch"}"#.into()),
            output: Some("done add fake.txt (hash_before=-, hash_after=abc)".into()), output_bytes: 52, output_truncated: false, patch_effects: effects, dcp: None, dcp_topic: None }
    }
    fn text(lines: &[Line]) -> String {
        lines
            .iter()
            .map(Line::plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The printable-ASCII fast path cuts and pads exactly like the grapheme
    /// walk, across span boundaries, control bytes and wide glyphs.
    #[test]
    fn fit_ascii_fast_path_matches_grapheme_walk() {
        fn reference(spans: Vec<Span>, width: usize, base: Style) -> Vec<Span> {
            let mut out = Vec::new();
            let mut used = 0;
            'spans: for span in spans {
                let mut text = String::new();
                let safe: String = span
                    .content()
                    .chars()
                    .map(|c| if c.is_control() { '�' } else { c })
                    .collect();
                for glyph in safe.graphemes(true) {
                    let cells = UnicodeWidthStr::width(glyph);
                    if used + cells > width {
                        if !text.is_empty() {
                            out.push(Span::styled(text, span.style()));
                        }
                        break 'spans;
                    }
                    text.push_str(glyph);
                    used += cells;
                }
                if !text.is_empty() {
                    out.push(Span::styled(text, span.style()));
                }
            }
            if used < width {
                out.push(Span::decor(" ".repeat(width - used), base));
            }
            out
        }
        let red = Style::default().fg(ratatui::style::Color::Red);
        let base = Style::default().bg(ratatui::style::Color::Blue);
        let cases: Vec<Vec<&str>> = vec![
            vec![],
            vec![""],
            vec!["let value = 42;"],
            vec!["fn ", "", "main", "() {", " }"],
            vec!["tab\there", "cr\r\nlf", "\u{7f}del"],
            vec!["ascii ", "широкий 漢字", " tail"],
            vec!["e\u{301}combining", "🙂 emoji", "~end"],
        ];
        for case in cases {
            for width in 0..48 {
                let spans: Vec<_> = case
                    .iter()
                    .enumerate()
                    .map(|(i, text)| Span::styled(*text, if i % 2 == 0 { red } else { base }))
                    .collect();
                assert_eq!(
                    fit(spans.clone(), width, base),
                    reference(spans, width, base),
                    "{case:?} at {width}"
                );
            }
        }
    }

    /// OpenTUI selects only the code renderables of a diff: gutters, line
    /// numbers, signs, frame and fill are neither highlighted nor copied.
    #[test]
    fn diff_selection_copies_and_highlights_only_code_cells() {
        let theme = Theme::dark();
        let mut effect = effects();
        effect.files[0].hunks[0].lines.insert(
            1,
            patch_line(PatchLineKind::Context, Some(100), Some(101), ""),
        );
        let card = card_from_row(&row(Some(effect), "completed"));
        let copy = |lines: &[Line]| {
            styled::copy_text(
                usize::MAX,
                lines.iter().map(|line| (line, 0, line.plain_text().len())),
            )
            .unwrap()
            .unwrap()
        };
        let unified = render(card.patch_effects.as_ref().unwrap(), &card, theme, 120);
        assert_eq!(
            copy(&unified),
            "← Patched a.txt\ncontext\n\nold\nlet value = 42;"
        );
        let split = render(card.patch_effects.as_ref().unwrap(), &card, theme, 121);
        // Both code renderables of a row join by x, like Selection.getSelectedText.
        assert_eq!(
            copy(&split),
            "← Patched a.txt\ncontextcontext\n\noldlet value = 42;"
        );
        let row = split
            .iter()
            .find(|line| line.plain_text().contains("old"))
            .unwrap();
        let marked = row.highlight(0, row.plain_text().len(), theme.text(), theme.background());
        let cells = |line: &Line| {
            line.spans()
                .iter()
                .flat_map(|span| {
                    span.content()
                        .chars()
                        .map(move |ch| (ch, span.style(), span.is_decor()))
                })
                .collect::<Vec<_>>()
        };
        for (before, after) in cells(row).into_iter().zip(cells(&marked)) {
            assert_eq!(before.2, after.2);
            if before.2 {
                assert_eq!(before, after, "decor cells keep their paint");
            }
        }
        let highlighted: String = marked
            .spans()
            .iter()
            .filter(|span| span.style().bg == Some(theme.text()))
            .map(Span::content)
            .collect();
        assert_eq!(highlighted, "oldlet value = 42;");
    }

    #[test]
    fn vis35_actual_gutters_split_boundary_styles_and_syntax() {
        let theme = Theme::dark();
        let mut effect = effects();
        effect.files[0].path = "a.rs".into();
        let card = card_from_row(&row(Some(effect), "completed"));
        let unified = render(card.patch_effects.as_ref().unwrap(), &card, theme, 120);
        let split = render(card.patch_effects.as_ref().unwrap(), &card, theme, 121);
        assert!(text(&unified).contains("100   context"));
        assert!(text(&unified).contains("100 - old"));
        assert!(text(&unified).contains("101 + let value = 42;"));
        assert!(text(&split).contains(" 99   context"));
        let change = split
            .iter()
            .find(|r| r.plain_text().contains("- old"))
            .unwrap();
        assert!(change.plain_text().contains("101 + let value = 42;"));
        assert!(change.spans().iter().any(|s| s.content() == " +"
            && s.style().fg == Some(theme.diff_highlight_added())
            && s.style().bg == theme.color("diff.lineNumber.background.added")));
        assert!(change.spans().iter().any(|s| s.content().contains("101")
            && s.style().bg == theme.color("diff.lineNumber.background.added")));
        assert!(change.spans().iter().any(|s| s.content().contains("let")
            && s.style().fg == Some(theme.syntax(crate::theme::SyntaxToken::Keyword))
            && s.style().bg == Some(theme.diff_added_background())));
        assert!(!text(&unified).contains("fake.txt"));
        assert!(!text(&unified).contains("invented"));
        let unbounded = render(card.patch_effects.as_ref().unwrap(), &card, theme, 0);
        assert!(text(&unbounded).contains("← Patched a.rs"));
        assert!(text(&unbounded).contains("101 + let value = 42;"));
        assert!(
            unified
                .iter()
                .all(|l| l.spans().iter().map(styled::span_width).sum::<usize>() == 120)
        );
        let mut explicit = card.clone();
        explicit.diff_settings.view = DiffView::Split;
        assert_eq!(
            render(
                explicit.patch_effects.as_ref().unwrap(),
                &explicit,
                theme,
                121
            ),
            split
        );
        explicit.diff_settings.view = DiffView::Unified;
        assert_eq!(
            render(
                explicit.patch_effects.as_ref().unwrap(),
                &explicit,
                theme,
                120
            ),
            unified
        );
        let lines = vec![
            patch_line(PatchLineKind::Context, Some(1), Some(1), "all:"),
            patch_line(PatchLineKind::Removed, Some(2), None, "\tfalse"),
            patch_line(PatchLineKind::Added, None, Some(2), "\techo café"),
        ];
        let (old, old_error) = syntax_image(&lines, false, "Makefile", theme);
        let (new, new_error) = syntax_image(&lines, true, "Makefile", theme);
        assert!(old_error.is_none() && new_error.is_none());
        assert_eq!(old[1].plain_text(), "  false");
        assert_eq!(new[2].plain_text(), "  echo café");
        assert_eq!(old[2].plain_text(), "");
        assert_eq!(new[1].plain_text(), "");
    }

    #[test]
    fn vis35_auto_uses_root_content_width_and_capture_gutter_roles() {
        use crate::layout;
        use ratatui::layout::Rect;
        let theme = Theme::dark();
        let mut effects = effects();
        effects.files[0].hunks[0].lines = vec![
            patch_line(PatchLineKind::Removed, Some(8), None, "line-08"),
            patch_line(PatchLineKind::Added, None, Some(8), "LINE-EIGHT"),
            patch_line(PatchLineKind::Context, Some(24), Some(24), "line-24"),
        ];
        let card = card_from_row(&row(Some(effects), "completed"));
        // U18 ctx.width = session pane width - 4, not terminal.width.
        // Exercise the real root geometry (including a vertical tab rail).
        for (terminal, rail) in [(120, 0), (121, 0), (124, 0), (125, 0), (130, 5), (160, 0)] {
            let root = layout::configured_shell_regions(Rect::new(0, 0, terminal, 40), false, rail);
            let content = layout::session_regions(root.session, 0).transcript;
            assert_eq!(content.width, root.session.width - 4);
            let rows = [HistoryRow {
                position: None,
                seq: 1,
                message_id: None,
                role: "tool".into(),
                text: String::new().into(),
                agent: None,
                agent_color_index: None,
                pending_delivery: None,
                chips: Vec::new(),
                reasoning: None,
                child_notice: None,
                shell_notice: None,
                meta: None,
                tool: Some(std::sync::Arc::new(card.clone())),
            }];
            let full = crate::messages::transcript(&rows, theme, content.width, terminal, |_| {
                theme.text()
            });
            let change = full
                .iter()
                .find(|r| r.plain_text().contains("- line-08"))
                .unwrap();
            assert_eq!(
                change.plain_text().contains("+ LINE-EIGHT"),
                content.width > 120,
                "AUTO terminal={terminal}, rail={rail}, content={content:?}"
            );
        }
        // Independently measured U18 cells: recovery-v00 attempts04/06
        // unified at terminal121, attempt07 split at terminal160.
        for (terminal, view) in [
            (121, DiffView::Auto),
            (160, DiffView::Split),
            (121, DiffView::Split),
        ] {
            let content = layout::session_regions(Rect::new(0, 0, terminal, 40), 0).transcript;
            let mut card = card.clone();
            card.diff_settings.view = view;
            let shown = render(
                card.patch_effects.as_ref().unwrap(),
                &card,
                theme,
                content.width,
            );
            let change = shown
                .iter()
                .find(|r| r.plain_text().contains("- line-08"))
                .unwrap();
            let cells: Vec<_> = change
                .spans()
                .iter()
                .flat_map(|s| s.content().chars().map(move |c| (c, s.style())))
                .collect();
            let gutter_bg = theme.color("diff.lineNumber.background.removed").unwrap();
            assert_eq!(cells[4], (' ', Style::default().bg(gutter_bg)));
            assert_eq!(cells[5], (' ', Style::default().bg(gutter_bg)));
            assert_eq!(
                cells[6],
                (
                    '8',
                    Style::default().fg(theme.diff_line_number()).bg(gutter_bg)
                )
            );
            assert_eq!(
                cells[7],
                (
                    ' ',
                    Style::default()
                        .fg(theme.diff_highlight_removed())
                        .bg(gutter_bg)
                )
            );
            assert_eq!(
                cells[8],
                (
                    '-',
                    Style::default()
                        .fg(theme.diff_highlight_removed())
                        .bg(gutter_bg)
                )
            );
            assert_eq!(cells[9], (' ', Style::default().bg(gutter_bg)));
            assert_eq!(
                cells[10],
                (
                    'l',
                    Style::default()
                        .fg(theme.text())
                        .bg(theme.diff_removed_background())
                )
            );
            assert_eq!(
                cells[17],
                (' ', Style::default().bg(theme.diff_removed_background()))
            );
            if view == DiffView::Split {
                let right = 4 + usize::from(content.width - 4).div_ceil(2);
                assert_eq!(cells[right + 2].0, '8');
                assert_eq!(cells[right + 4].0, '+');
                assert_eq!(cells[right + 6].0, 'L');
                assert_eq!(
                    cells[right - 1].1.bg,
                    Some(theme.diff_removed_background()),
                    "no invented separator between split surfaces"
                );
                if terminal == 160 {
                    assert_eq!(usize::from(content.x) + right + 2, 84);
                }
            }
        }
    }

    #[test]
    fn vis35_fresh_capture_odd_split_and_header_gap_full_indexed() {
        use crate::layout;
        use ratatui::layout::Rect;
        let theme = Theme::dark();
        let mut effects = effects();
        effects.files[0].path = "update.txt".into();
        effects.files[0].hunks[0].lines = vec![
            patch_line(PatchLineKind::Removed, Some(1), None, "old move"),
            patch_line(PatchLineKind::Added, None, Some(1), "new move"),
        ];
        let card = card_from_row(&row(Some(effects.clone()), "completed"));
        let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
        // Actual U18 right-side number coordinates: capture16 x64,
        // capture18 x66, capture15 x83 (one-digit move lines).
        for (terminal, view, right_number) in [
            (121, DiffView::Split, 64),
            (125, DiffView::Auto, 66),
            (160, DiffView::Split, 83),
        ] {
            let content = layout::session_regions(Rect::new(0, 0, terminal, 40), 0).transcript;
            let mut card = card.clone();
            card.diff_settings.view = view;
            let rows = [HistoryRow {
                position: None,
                seq: 1,
                message_id: None,
                role: "tool".into(),
                text: String::new().into(),
                agent: None,
                agent_color_index: None,
                pending_delivery: None,
                chips: Vec::new(),
                reasoning: None,
                child_notice: None,
                shell_notice: None,
                meta: None,
                tool: Some(std::sync::Arc::new(card)),
            }];
            let mut full = vec![Line::plain("")];
            full.extend(styled::wrap_lines(
                &crate::messages::transcript(&rows, theme, content.width, terminal, |_| {
                    theme.text()
                }),
                usize::from(content.width),
            ));
            let change = full
                .iter()
                .find(|r| r.plain_text().contains("- old move"))
                .unwrap();
            let cells: Vec<_> = change
                .spans()
                .iter()
                .flat_map(|s| s.content().chars().map(move |c| (c, s.style())))
                .collect();
            let x = right_number - usize::from(content.x);
            assert_eq!(
                cells[x],
                (
                    '1',
                    Style::default()
                        .fg(theme.diff_line_number())
                        .bg(theme.color("diff.lineNumber.background.added").unwrap())
                )
            );
            assert_eq!(cells[x + 2].0, '+');
            assert_eq!(cells[x + 4].0, 'n');
            for scroll in [0, 2, 5] {
                let (visible, total) = crate::messages::visible_transcript(
                    &rows,
                    theme,
                    content.width,
                    terminal,
                    (5, scroll, None),
                    |_| theme.text(),
                    &cache,
                );
                assert_eq!(total, full.len());
                let end = total.saturating_sub(scroll.min(total.saturating_sub(5)));
                assert_eq!(visible, full[end.saturating_sub(5)..end]);
            }
        }
        // Captures11/13: only the label/path layout gap differs, at global
        // x14. Preserve the internal label space and every path glyph's style.
        for operation in [
            PatchOperation::Update,
            PatchOperation::Delete,
            PatchOperation::Create,
        ] {
            effects.files[0].operation = operation;
            effects.files[0].path = "漢🙂.txt".into();
            let card = card_from_row(&row(Some(effects.clone()), "completed"));
            let rows = [HistoryRow {
                position: None,
                seq: 1,
                message_id: None,
                role: "tool".into(),
                text: String::new().into(),
                agent: None,
                agent_color_index: None,
                pending_delivery: None,
                chips: Vec::new(),
                reasoning: None,
                child_notice: None,
                shell_notice: None,
                meta: None,
                tool: Some(std::sync::Arc::new(card)),
            }];
            for terminal in [120, 121] {
                let content = layout::session_regions(Rect::new(0, 0, terminal, 40), 0).transcript;
                let mut full = vec![Line::plain("")];
                full.extend(crate::messages::transcript(
                    &rows,
                    theme,
                    content.width,
                    terminal,
                    |_| theme.text(),
                ));
                let header = full
                    .iter()
                    .find(|r| r.plain_text().contains("漢🙂.txt"))
                    .unwrap();
                let spans = header.spans();
                assert_eq!(spans[2].style().fg, Some(theme.text_muted()));
                assert_eq!(spans[3].content(), " ");
                assert_eq!(
                    spans[3].style(),
                    Style::default().bg(theme.background_raised())
                );
                assert_eq!(spans[4].content(), "漢🙂.txt");
                assert_eq!(spans[4].style(), spans[2].style());
                assert_eq!(
                    usize::from(content.x)
                        + spans[..3].iter().map(styled::span_width).sum::<usize>(),
                    14
                );
                let mut wrapped = vec![Line::plain("")];
                wrapped.extend(styled::wrap_lines(&full[1..], usize::from(content.width)));
                let (visible, total) = crate::messages::visible_transcript(
                    &rows,
                    theme,
                    content.width,
                    terminal,
                    (40, 0, None),
                    |_| theme.text(),
                    &cache,
                );
                assert_eq!(total, wrapped.len());
                assert_eq!(visible, wrapped);
            }
        }
    }

    #[test]
    fn vis35_multi_operation_move_delete_empty_truncation_and_failure() {
        let mut effects = effects();
        let mut created = effects.files[0].clone();
        created.operation = PatchOperation::Create;
        created.path = "empty.txt".into();
        created.hunks.clear();
        created.additions = 0;
        created.deletions = 0;
        let mut moved = effects.files[0].clone();
        moved.operation = PatchOperation::Move;
        moved.path = "source.rs".into();
        moved.destination = Some("destination.rs".into());
        let mut deleted = created.clone();
        deleted.operation = PatchOperation::Delete;
        deleted.path = "deleted.txt".into();
        deleted.deletions = 1;
        effects.files.extend([created, moved, deleted]);
        effects.total_files = 5;
        effects.truncated = true;
        effects.files[0].truncated = true;
        let second = effects.files[0].hunks[0].clone();
        effects.files[0].hunks.push(second);
        effects.files[0].hunks[1].truncated = true;
        effects.files[0].hunks[0].lines[2].truncated = true;
        effects.files[0].hunks[0].lines[2].lossy = true;
        let card = card_from_row(&row(Some(effects.clone()), "failed"));
        let shown = text(&render(&effects, &card, Theme::dark(), 100));
        for needle in [
            "# Patch failed",
            "Confirmed effects",
            "# Created empty.txt",
            "← Patched destination.rs",
            "# Deleted deleted.txt",
            "-1 line",
            "@@ -99,2 +100,2 @@",
            "[preview truncated]",
            "lossy byte preview",
            "… [truncated]",
        ] {
            assert!(shown.contains(needle), "missing {needle}: {shown}");
        }
        assert!(!shown.contains("[diff preview truncated;"));
        assert!(
            !shown.contains("hunk preview truncated") && !shown.contains("file preview truncated")
        );
        assert!(!shown.contains("source.rs → destination.rs"));
        assert_eq!(effects.total_files, 5);
        assert_eq!(effects.files[2].path, "source.rs");
        let mut literal_effects = effects.clone();
        literal_effects.files[0].hunks[0].lines[2].text =
            "[diff preview truncated; literal code]".into();
        let mut literal = row(Some(literal_effects), "failed");
        let error = "Recorded capture: Complete; literal error";
        literal.output_presentation = Some(Box::new(oc_core::tool_output::Presentation::new(
            error,
            error.len() as u64,
            false,
        )));
        let shown = text(&crate::tools::tool_block(
            &card_from_row(&literal),
            Theme::dark(),
            120,
        ));
        assert!(shown.contains("[diff preview truncated; literal code]"));
        assert!(shown.contains(error) && shown.contains("# Patch failed"));
        for state in ["unknown", "denied", "cancelled"] {
            let card = card_from_row(&row(Some(PatchEffects::default()), state));
            let shown = text(&crate::tools::tool_block(&card, Theme::dark(), 80));
            assert!(!shown.contains("# Created"));
            assert!(!shown.contains("invented"));
            assert!(shown.contains(if state == "unknown" {
                "outcome unknown"
            } else {
                "Patch failed"
            }));
        }
        let legacy = card_from_row(&row(None, "completed"));
        let shown = text(&crate::tools::tool_block(&legacy, Theme::dark(), 100));
        assert!(shown.contains("Request preview (not confirmed)"));
        assert!(!shown.contains("# Created"));
    }

    #[test]
    fn truncated_diff_footer_is_hidden_without_losing_capture_facts() {
        use oc_core::tool_output::{Capture, CaptureState, Presentation};
        let mut effects = effects();
        effects.truncated = true;
        effects.total_files = 2;
        effects.additions = 2;
        let mut presentation = Presentation::new("done", 4, true);
        presentation.capture = Some(Capture {
            reference: Some("/data/tool-output/tool_diff.txt".into()),
            state: CaptureState::Complete,
            admitted_bytes: 1800,
            retained_bytes: 1800,
            admitted_lines: 1,
            retained_lines: 1,
        });
        for (name, state) in [
            ("edit", "completed"),
            ("apply_patch", "completed"),
            ("write", "failed"),
        ] {
            let mut operation = row(Some(effects.clone()), state);
            operation.name = name.into();
            let mut facts = presentation.clone();
            if state == "failed" {
                facts.body = "write failed after commit".into();
                facts.body_bytes = facts.body.len() as u64;
            }
            operation.output_presentation = Some(Box::new(facts.clone()));
            let replay: ToolOpView =
                serde_json::from_str(&serde_json::to_string(&operation).unwrap()).unwrap();
            let card = card_from_row(&operation);
            let replay_card = card_from_row(&replay);
            assert_eq!(card, replay_card);
            assert_eq!(card.patch_effects.as_ref(), Some(&effects));
            assert_eq!(card.output_presentation.as_deref(), Some(&facts));
            let narrow = text(&crate::tools::tool_block(&card, Theme::dark(), 30));
            assert!(narrow.contains("[preview…]") && narrow.contains("a.txt"));
            let narrower = text(&crate::tools::tool_block(&card, Theme::dark(), 18));
            assert!(narrower.contains("a.txt"));
            for width in [120, 121] {
                let shown = text(&crate::tools::tool_block(&card, Theme::dark(), width));
                assert_eq!(
                    shown,
                    text(&crate::tools::tool_block(
                        &replay_card,
                        Theme::dark(),
                        width
                    ))
                );
                assert_eq!(shown.matches("[preview truncated]").count(), 1);
                assert!(
                    !shown.contains("[diff preview truncated;")
                        && !shown.contains("hunk preview truncated")
                        && !shown.contains("file preview truncated")
                );
                assert!(
                    shown.contains("a.txt")
                        && shown.contains("old")
                        && shown.contains("let value = 42;")
                        && !shown.contains("… [truncated]")
                );
                assert!(
                    !shown.contains("Recorded capture: Complete;")
                        && !shown.contains("/data/tool-output/tool_diff.txt")
                );
                if state == "failed" {
                    assert!(shown.contains("# Write failed"));
                    assert!(shown.contains("write failed after commit"));
                }
                let mut no_path = card.clone();
                no_path
                    .output_presentation
                    .as_mut()
                    .unwrap()
                    .capture
                    .as_mut()
                    .unwrap()
                    .reference = None;
                let shown = text(&render(&effects, &no_path, Theme::dark(), width));
                assert!(shown.contains("Recorded capture: Complete; no usable path"));
                let mut full = effects.clone();
                full.truncated = false;
                full.total_files = 1;
                full.additions = 1;
                full.files[0].truncated = false;
                full.files[0].hunks[0].truncated = false;
                full.files[0].hunks[0].lines[2].truncated = false;
                let shown = text(&render(&full, &card, Theme::dark(), width));
                assert!(
                    !shown.contains("preview truncated") && !shown.contains("Recorded capture:")
                );
                let mut failed = card.clone();
                failed.state = "failed".into();
                let capture = failed
                    .output_presentation
                    .as_mut()
                    .unwrap()
                    .capture
                    .as_mut()
                    .unwrap();
                capture.state = CaptureState::Quota;
                capture.retained_bytes = 128;
                let shown = text(&render(&full, &failed, Theme::dark(), width));
                assert!(
                    shown.contains("Recorded capture: Quota;")
                        && !shown.contains("preview truncated")
                );
                let shown = text(&render(
                    &PatchEffects::default(),
                    &failed,
                    Theme::dark(),
                    width,
                ));
                assert!(!shown.contains("Recorded capture:"));
            }
        }
    }

    #[test]
    fn vis36_rejected_patch_labels_requested_target_without_inventing_effects() {
        let mut operation = row(Some(PatchEffects::default()), "denied");
        operation.input = Some(serde_json::json!({"patchText": "*** Begin Patch\n*** Update File: approval.txt\n@@\n-old\n+new\n*** End Patch"}).to_string());
        operation.output = Some(
            serde_json::json!({"status": "permission_rejected", "feedback": null}).to_string(),
        );
        let card = card_from_row(&operation);
        assert_eq!(card.files, ["approval.txt"]);
        assert!(card.patch_effects.as_ref().unwrap().files.is_empty());
        let rows = [HistoryRow {
            position: None,
            seq: 1,
            message_id: None,
            role: "tool".into(),
            text: String::new().into(),
            agent: None,
            agent_color_index: None,
            pending_delivery: None,
            chips: Vec::new(),
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool: Some(std::sync::Arc::new(card)),
        }];
        let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
        for terminal in [79, 80, 120, 121] {
            let width = terminal - 4;
            let full = crate::messages::transcript(&rows, Theme::dark(), width, terminal, |_| {
                Theme::dark().text()
            });
            let shown = text(&full);
            assert!(shown.contains("# Patch failed approval.txt"), "{shown}");
            assert!(
                shown.contains("The user declined this tool call"),
                "{shown}"
            );
            for false_effect in ["← Patched", "Confirmed effects", "- old", "+ new"] {
                assert!(!shown.contains(false_effect), "{shown}");
            }
            let header = full
                .iter()
                .position(|line| line.plain_text().contains("# Patch failed"))
                .unwrap();
            let body = full
                .iter()
                .position(|line| line.plain_text().contains("The user declined"))
                .unwrap();
            assert_eq!(body, header + 2);
            for _ in 0..2 {
                let (visible, total) = crate::messages::visible_transcript(
                    &rows,
                    Theme::dark(),
                    width,
                    terminal,
                    (40, 0, None),
                    |_| Theme::dark().text(),
                    &cache,
                );
                let mut expected = vec![Line::plain("")];
                expected.extend(styled::wrap_lines(&full, usize::from(width)));
                assert_eq!(total, expected.len());
                assert_eq!(visible.len(), expected.len());
                for (actual, expected) in visible.iter().zip(&expected) {
                    assert_eq!(actual.spans(), expected.spans());
                }
            }
        }
    }

    #[test]
    fn vis35_configured_wrap_retains_gutters_and_cell_bounds() {
        let theme = Theme::dark();
        let mut effects = effects();
        effects.files[0].hunks[0].lines[2].text = "alpha beta 漢字 gamma delta".into();
        let card = card_from_row(&row(Some(effects.clone()), "completed"));
        for width in [1, 4, 9, 18, 31, 80, 120, 121] {
            for view in [DiffView::Auto, DiffView::Unified, DiffView::Split] {
                for wrap in [DiffWrap::None, DiffWrap::Word] {
                    let mut card = card.clone();
                    card.diff_settings = DiffSettings { view, wrap };
                    let shown = render(&effects, &card, theme, width);
                    assert!(
                        shown.iter().all(|r| r
                            .spans()
                            .iter()
                            .map(styled::span_width)
                            .sum::<usize>()
                            == usize::from(width))
                    );
                    assert!(shown.len() < 100);
                }
            }
        }
        let mut card = card.clone();
        card.diff_settings.view = DiffView::Unified;
        card.diff_settings.wrap = DiffWrap::None;
        let none = render(&effects, &card, theme, 18);
        card.diff_settings.wrap = DiffWrap::Word;
        let word_rows = render(&effects, &card, theme, 18);
        assert!(word_rows.len() > none.len());
        assert_eq!(
            text(&word_rows).matches("101 +").count(),
            1,
            "continuations must not invent coordinates"
        );
        effects.files[0].path = "path\u{1b}[31m.txt".into();
        effects.files[0].hunks[0].lines[2].text = "\u{1b}[2J\t\r unsafe".into();
        let safe = render(&effects, &card, theme, 18);
        assert!(!text(&safe).chars().any(|c| c.is_control() && c != '\n'));
        assert!(
            safe.iter()
                .all(|r| r.spans().iter().map(styled::span_width).sum::<usize>() == 18)
        );
    }

    #[test]
    fn vis35_full_indexed_replay_and_hover_keep_actual_effects() {
        let card = card_from_row(&row(Some(effects()), "completed"));
        let history = HistoryRow {
            position: None,
            seq: 1,
            message_id: None,
            role: "tool".into(),
            text: String::new().into(),
            agent: None,
            agent_color_index: None,
            pending_delivery: None,
            chips: Vec::new(),
            reasoning: None,
            child_notice: None,
            shell_notice: None,
            meta: None,
            tool: Some(std::sync::Arc::new(card)),
        };
        let theme = Theme::dark();
        let cache = std::cell::RefCell::new(crate::messages::MarkdownCache::default());
        for (width, terminal) in [(18, 120), (80, 121)] {
            let rows = [history.clone()];
            let mut full = vec![Line::plain("")];
            full.extend(styled::wrap_lines(
                &crate::messages::transcript(&rows, theme, width, terminal, |_| theme.text()),
                usize::from(width),
            ));
            for scroll in [0, 2, 5] {
                let (visible, total) = crate::messages::visible_transcript(
                    &rows,
                    theme,
                    width,
                    terminal,
                    (5, scroll, None),
                    |_| theme.text(),
                    &cache,
                );
                assert_eq!(total, full.len());
                let end = total.saturating_sub(scroll.min(total.saturating_sub(5)));
                let start = end.saturating_sub(5);
                assert_eq!(visible, full[start..end]);
            }
            let diff = full
                .iter()
                .find(|r| r.plain_text().contains("- old"))
                .unwrap();
            let hovered = crate::messages::hover_tool_content(diff, theme);
            assert_eq!(
                hovered, *diff,
                "non-expandable diffs retain semantic backgrounds"
            );
        }
        let encoded = serde_json::to_string(&effects()).unwrap();
        let replay: PatchEffects = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            card_from_row(&row(Some(replay), "completed")),
            *history.tool.unwrap()
        );
    }
}

#[cfg(test)]
#[path = "patch_view/write_tests.rs"]
mod write_tests;
