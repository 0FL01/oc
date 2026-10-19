//! Bounded, live Execute detail; sections retain their producer's typed origin.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::TuiState,
    dialog::{DialogFrame, DialogSize},
    styled::{self, Line, Span},
    theme::Theme,
};
use oc_core::tool_output::ExecuteOutputKind;

fn action_text(key: char, copied: Option<char>) -> &'static str {
    if copied == Some(key) {
        return "✓ copied";
    }
    match key {
        'c' => "c copy code",
        'o' => "o copy output",
        'e' => "esc back",
        _ => "↑/↓ scroll",
    }
}

pub(crate) struct Layout {
    pub rect: Rect,
    pub body: Rect,
    pub rows: Vec<Line>,
    pub actions: Vec<(char, Rect)>,
    status: String,
    failed: bool,
}

fn code_rows(source: &str, language: &str, theme: &Theme, base: Style) -> Vec<Line> {
    crate::syntax::highlight(source, Some(language), theme, base)
        .unwrap_or_else(|_| {
            source
                .split('\n')
                .map(|text| Line::styled(text, base))
                .collect()
        })
        .into_iter()
        .map(|line| {
            let safe = Line::new(
                line.spans()
                    .iter()
                    .map(|span| {
                        Span::styled(crate::syntax::safe_line(span.content()), span.style())
                    })
                    .collect(),
            );
            crate::syntax::display_line(&safe)
        })
        .collect()
}

pub(crate) fn layout(state: &TuiState, area: Rect) -> Layout {
    let theme = Theme::dark();
    let base = Style::default().fg(theme.text());
    let muted = Style::default().fg(theme.text_muted());
    let label = muted.add_modifier(Modifier::BOLD);
    let geometry = DialogFrame::rect(area, DialogSize::Xlarge, 0);
    let width = geometry.width.saturating_sub(4) as usize;
    let captured = state.execute_capture();
    let capture = captured.as_ref().map(|(capture, _)| &**capture);
    let facts = capture.map(|capture| &capture.facts);
    let running = capture.is_some_and(|capture| capture.running);
    let failed = capture.is_some_and(|capture| capture.failed);
    let status = if captured.as_ref().is_none_or(|(_, available)| !available) {
        "Source unavailable".to_owned()
    } else if running {
        if facts.is_none_or(|facts| facts.code.is_none()) {
            "Receiving code…"
        } else {
            "Running"
        }
        .to_owned()
    } else {
        let mut status = if failed { "Failed" } else { "Completed" }.to_owned();
        if let Some(ms) = facts.and_then(|facts| facts.elapsed_ms) {
            status.push_str(&format!(" · {}", crate::messages::Locale::duration(ms)));
        }
        status
    };
    let code = facts.and_then(|facts| facts.code.as_deref());
    let output = capture.and_then(|capture| capture.output().next());
    let json = output
        .filter(|part| part.kind == ExecuteOutputKind::Json)
        .map(|part| part.text.as_str());
    let count = code
        .map_or(0, |text| text.split('\n').count())
        .max(json.map_or(0, |text| text.split('\n').count()));
    let gutter = count.max(1).to_string().len();
    let inner = width.saturating_sub(gutter + 1).max(1);
    let mut rows = vec![Line::styled("Code", label)];
    let numbered = |text: &str, language: &str, rows: &mut Vec<Line>| {
        for (index, line) in code_rows(text, language, theme, base)
            .into_iter()
            .enumerate()
        {
            for (physical, line) in styled::wrap_char_line_limited(&line, inner, usize::MAX)
                .into_iter()
                .enumerate()
            {
                let mut spans = vec![
                    Span::styled(
                        if physical == 0 {
                            format!("{:<gutter$}", index + 1)
                        } else {
                            " ".repeat(gutter)
                        },
                        muted,
                    ),
                    Span::plain(" "),
                ];
                spans.extend_from_slice(line.spans());
                rows.push(Line::new(spans));
            }
        }
    };
    if let Some(code) = code {
        numbered(code, "typescript", &mut rows);
    } else {
        rows.push(Line::styled("Waiting for code…", muted));
    }
    if facts.is_some_and(|facts| facts.code_limited) {
        rows.push(Line::styled("… code preview limited", muted));
    }
    rows.extend([Line::plain(""), Line::styled("Output", label)]);
    if let Some(json) = json {
        numbered(json, "json", &mut rows);
    }
    let color = if failed { theme.error() } else { theme.text() };
    if let Some(capture) = capture {
        for (index, part) in capture.output().enumerate() {
            if index == 0 && json.is_some() {
                continue;
            }
            if index > 0 && !(index == 1 && json.is_some()) {
                rows.push(Line::plain(""));
            }
            for text in part.text.split('\n') {
                let line = Line::styled(crate::syntax::safe_line(text), Style::default().fg(color));
                for line in styled::wrap_line_limited(
                    &line,
                    if json.is_some() { inner } else { width.max(1) },
                    usize::MAX,
                ) {
                    if json.is_some() {
                        let mut spans = vec![Span::plain(" ".repeat(gutter + 1))];
                        spans.extend_from_slice(line.spans());
                        rows.push(Line::new(spans));
                    } else {
                        rows.push(line);
                    }
                }
            }
        }
        if capture.output().any(|part| part.limited) {
            rows.push(Line::styled("… output preview limited", muted));
        }
        if capture.output().next().is_none() {
            rows.push(Line::styled(
                if running {
                    "Waiting for output…"
                } else {
                    "No output"
                },
                muted,
            ));
        }
    } else {
        rows.push(Line::styled("Captured output unavailable", muted));
    }
    let wanted = rows
        .len()
        .min(((area.height as usize * 7 / 10).saturating_sub(6)).max(3));
    let mut rect = DialogFrame::rect(
        area,
        DialogSize::Xlarge,
        (wanted + 6).min(u16::MAX as usize) as u16,
    );
    rect.y = area.y + area.height.saturating_sub(rect.height).div_ceil(2);
    let body = Rect::new(
        rect.x + rect.width.min(2),
        rect.y + rect.height.min(3),
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(6),
    );
    let mut actions = vec![(
        'e',
        Rect::new(
            body.right().saturating_sub(3).max(body.x),
            rect.y + rect.height.min(1),
            body.width.min(3),
            1.min(rect.height),
        ),
    )];
    let mut x = body.x;
    let copied = state
        .execute_detail
        .as_ref()
        .and_then(|detail| detail.copied);
    for key in ['s', 'c', 'o', 'e'] {
        let text = action_text(key, copied);
        let length =
            UnicodeWidthStr::width(text).min(body.right().saturating_sub(x) as usize) as u16;
        actions.push((
            key,
            Rect::new(
                x,
                rect.bottom().saturating_sub(2).max(rect.y),
                length,
                1.min(rect.height),
            ),
        ));
        x = x.saturating_add(length + 3).min(body.right());
    }
    Layout {
        rect,
        body,
        rows,
        actions,
        status,
        failed,
    }
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &TuiState) {
    let layout = layout(state, frame.area());
    let theme = Theme::dark();
    DialogFrame::paint(frame, layout.rect, theme);
    if layout.body.width == 0 || layout.rect.height < 3 {
        return;
    }
    let muted = Style::default().fg(theme.text_muted());
    let heading = Rect::new(
        layout.body.x,
        layout.rect.y + 1,
        layout.body.width.saturating_sub(5),
        1,
    );
    frame.render_widget(
        Paragraph::new("execute").style(
            Style::default()
                .fg(theme.text())
                .add_modifier(Modifier::BOLD),
        ),
        Rect::new(heading.x, heading.y, heading.width.min(7), 1),
    );
    let status_width =
        UnicodeWidthStr::width(layout.status.as_str()).min(heading.width as usize) as u16;
    frame.render_widget(
        Paragraph::new(layout.status.as_str()).style(Style::default().fg(if layout.failed {
            theme.error()
        } else {
            theme.text_muted()
        })),
        Rect::new(
            heading.right().saturating_sub(status_width),
            heading.y,
            status_width,
            1,
        ),
    );
    let scroll = state
        .execute_detail
        .as_ref()
        .map_or(0, |detail| detail.scroll.get());
    let start = scroll.min(
        layout
            .rows
            .len()
            .saturating_sub(layout.body.height as usize),
    );
    // Persist the actually painted viewport, so a later narrower frame cannot
    // resurrect an offset already retired by a resize or live-content change.
    if let Some(detail) = &state.execute_detail {
        detail.scroll.set(start);
    }
    for (index, row) in layout
        .rows
        .iter()
        .skip(start)
        .take(layout.body.height as usize)
        .enumerate()
    {
        frame.render_widget(
            Paragraph::new(row.clone().into_ratatui()),
            Rect::new(
                layout.body.x,
                layout.body.y + index as u16,
                layout.body.width,
                1,
            ),
        );
    }
    for (index, (key, rect)) in layout.actions.iter().enumerate() {
        let copied = state
            .execute_detail
            .as_ref()
            .and_then(|detail| detail.copied);
        let text = if index == 0 {
            "esc"
        } else {
            action_text(*key, copied)
        };
        let line = if index > 0 && matches!(key, 'c' | 'o') {
            if copied == Some(*key) {
                Line::styled(
                    text,
                    Style::default()
                        .fg(theme.success())
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Line::new(vec![
                    Span::styled(
                        key.to_string(),
                        Style::default()
                            .fg(theme.text())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&text[1..], muted),
                ])
            }
        } else {
            Line::styled(text, muted)
        };
        frame.render_widget(Paragraph::new(line.into_ratatui()), *rect);
    }
}
