//! Execute rows and their action surfaces share one bounded typed projection.
use super::*;
use crate::messages::ToolExpansion;
use oc_core::tool_output::{Execute, ExecuteCallStatus};
use ratatui::style::Style;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Parent,
    Call(usize),
}

impl Target {
    pub(crate) fn index(self) -> Option<usize> {
        match self {
            Self::Parent => None,
            Self::Call(index) => Some(index),
        }
    }
}

pub(crate) struct Row {
    pub line: Line,
    pub target: Option<Target>,
}

pub(crate) fn facts(card: &ToolCard) -> Option<&Execute> {
    card.output_presentation.as_ref()?.execute.as_ref()
}

pub(crate) fn failed(card: &ToolCard) -> bool {
    facts(card).is_some_and(|facts| facts.error) || is_error_state(&card.state)
}

pub(crate) fn output_text(card: &ToolCard) -> Option<String> {
    let facts = facts(card)?;
    if facts.output.is_empty() {
        (failed(card) && !card.output_preview.is_empty()).then(|| card.output_preview.clone())
    } else {
        Some(
            facts
                .output
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    }
}

pub(crate) fn hover(line: &Line, theme: &Theme) -> Line {
    line.restyle_spans(|_, span| {
        let style = span.style();
        if [
            Some(theme.text_muted()),
            Some(theme.decrease(theme.text_muted())),
        ]
        .contains(&style.fg)
        {
            style.fg(theme.text())
        } else {
            style
        }
    })
}

pub(crate) fn block(
    card: &ToolCard,
    theme: &Theme,
    width: u16,
    expanded: &dyn Fn(ToolExpansion<'_>) -> bool,
) -> Vec<Row> {
    let Some(facts) = facts(card) else {
        return Vec::new();
    };
    let failed = failed(card);
    let muted = Style::default().fg(theme.text_muted());
    let error = Style::default().fg(theme.error());
    let available = if width == 0 {
        usize::MAX
    } else {
        (width as usize)
            .saturating_sub(crate::messages::MESSAGE_PADDING + TOOL_ICON_WIDTH)
            .max(1)
    };
    let mut rows = vec![Row {
        line: Line::new(vec![
            Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
            Span::styled(
                if running(card) {
                    card.spinner()
                } else if failed {
                    "✗"
                } else {
                    "✓"
                },
                if failed { error } else { muted },
            ),
            if running(card) {
                Span::styled(" ", muted)
            } else {
                Span::plain(" ")
            },
            Span::styled("execute", if failed { error } else { muted }),
        ]),
        target: Some(Target::Parent),
    }];
    if tool_spinner_animated(card) {
        rows[0].line = std::mem::take(&mut rows[0].line).animate(
            crate::messages::MESSAGE_PADDING,
            crate::styled::AnimationSource::Tool(card.op.clone()),
        );
    }
    for (index, call) in facts.calls.iter().enumerate() {
        let open = expanded(ToolExpansion::ExecuteCall(&card.op, index));
        let style = if call.status == ExecuteCallStatus::Error {
            error
        } else if open {
            Style::default().fg(theme.decrease(theme.text_muted()))
        } else {
            muted
        };
        let InlineRender::Generic {
            mut args, summary, ..
        } = generic_args(call.input.as_ref())
        else {
            unreachable!()
        };
        let mut label = if open || summary.is_empty() {
            call.tool.clone()
        } else {
            format!("{} {summary}", call.tool)
        };
        if UnicodeWidthStr::width(label.as_str()) > available {
            let mut cells = 0;
            let end = label
                .grapheme_indices(true)
                .find_map(|(byte, glyph)| {
                    cells += UnicodeWidthStr::width(glyph);
                    (cells > available.saturating_sub(1)).then_some(byte)
                })
                .unwrap_or(label.len());
            label.truncate(end);
            label.push('…');
        }
        rows.push(Row {
            line: Line::new(vec![
                Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
                Span::styled(
                    if call.status == ExecuteCallStatus::Error {
                        "✗"
                    } else {
                        "›"
                    },
                    style,
                ),
                Span::plain(" "),
                Span::styled(label, style),
            ]),
            target: (call.input_limited
                || call.input.as_ref().is_some_and(|input| match input {
                    serde_json::Value::Object(fields) => !fields.is_empty(),
                    serde_json::Value::Array(items) => !items.is_empty(),
                    _ => false,
                }))
            .then_some(Target::Call(index)),
        });
        if open {
            if args.is_empty()
                && let Some(input) = &call.input
            {
                args.push((
                    String::new(),
                    serde_json::to_string_pretty(input).expect("JSON input"),
                ));
            }
            for (key, value) in args {
                let prefix = if key.is_empty() {
                    String::new()
                } else {
                    format!("{key}: ")
                };
                let inner = available
                    .saturating_sub(UnicodeWidthStr::width(prefix.as_str()))
                    .max(1);
                let mut first = true;
                for text in styled::display_output(&value).split('\n') {
                    for line in styled::wrap_line_limited(
                        &Line::styled(text, Style::default().fg(theme.text())),
                        inner,
                        512,
                    ) {
                        let mut spans = vec![
                            Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING)),
                            Span::styled(
                                "│",
                                Style::default().fg(theme.decrease(theme.text_muted())),
                            ),
                            Span::plain(" "),
                            if first {
                                Span::styled(prefix.clone(), muted)
                            } else {
                                Span::plain(" ".repeat(UnicodeWidthStr::width(prefix.as_str())))
                            },
                        ];
                        spans.extend_from_slice(line.spans());
                        rows.push(Row {
                            line: Line::new(spans),
                            target: None,
                        });
                        first = false;
                    }
                }
            }
            if call.input_limited {
                rows.push(Row {
                    line: Line::styled("     … input preview limited", muted),
                    target: None,
                });
            }
        }
    }
    if facts.calls_limited {
        rows.push(Row {
            line: Line::styled("     … additional calls not retained", muted),
            target: None,
        });
    }
    if failed {
        let text = output_text(card).unwrap_or_default();
        let text = styled::display_output(text.trim());
        let text = text.trim();
        let max_chars = 4 * if width == 0 {
            usize::MAX / 4
        } else {
            (width as usize).saturating_sub(6).max(20)
        };
        let mut visible = text.lines().take(4).map(str::to_owned).collect::<Vec<_>>();
        if text.lines().count() > 4
            && let Some(last) = visible.last_mut()
        {
            last.push('…');
        }
        let mut text = visible.join("\n");
        if text.chars().count() > max_chars {
            text = text.chars().take(max_chars.saturating_sub(1)).collect();
            text.push('…');
        }
        let mut first = true;
        let preview_width = if width == 0 {
            usize::MAX
        } else {
            (width as usize)
                .saturating_sub(crate::messages::MESSAGE_PADDING)
                .max(1)
        };
        for logical in text.lines() {
            let line = Line::styled(
                format!("{}{logical}", if first { "↳ " } else { "  " }),
                error,
            );
            for line in styled::wrap_line_limited(&line, preview_width, usize::MAX) {
                let mut spans = vec![Span::plain(" ".repeat(crate::messages::MESSAGE_PADDING))];
                spans.extend_from_slice(line.spans());
                rows.push(Row {
                    line: Line::new(spans),
                    target: None,
                });
            }
            first = false;
        }
    }
    rows
}

pub(crate) fn running(card: &ToolCard) -> bool {
    is_running(&card.state)
        || matches!(
            card.state.as_str(),
            "running" | "argument_stream" | "permission_pending"
        )
}
