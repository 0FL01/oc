//! Styled text primitives for the bounded TUI view model.
//!
//! The historical view contract is `Vec<String>` (`views::panel_lines`,
//! `app::TuiState::viewport`, `picker::ModelPicker::window`). [`Lines`]
//! converts those producers losslessly: `Lines::from(Vec<String>)` yields one
//! unstyled [`Line`] per input string, so existing producers and their tests
//! keep compiling and render byte-identically while views add [`Style`]s
//! where the theme requires them.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Text;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A piece of text with a single style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    content: String,
    style: Style,
}

impl Span {
    /// Text with an explicit style.
    pub fn styled(content: impl Into<String>, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
        }
    }

    /// Unstyled text (terminal defaults).
    pub fn plain(content: impl Into<String>) -> Self {
        Self::styled(content, Style::default())
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn style(&self) -> Style {
        self.style
    }
}

impl From<String> for Span {
    fn from(content: String) -> Self {
        Self::plain(content)
    }
}

impl From<&str> for Span {
    fn from(content: &str) -> Self {
        Self::plain(content)
    }
}

/// One styled row. `style` is patched over every span style by Ratatui.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    spans: Vec<Span>,
    style: Style,
}

impl Line {
    pub fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            style: Style::default(),
        }
    }

    /// A single-span line.
    pub fn styled(content: impl Into<String>, style: Style) -> Self {
        Self::new(vec![Span::styled(content, style)])
    }

    /// A single unstyled span.
    pub fn plain(content: impl Into<String>) -> Self {
        Self::new(vec![Span::plain(content)])
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    pub fn style(&self) -> Style {
        self.style
    }

    /// Patch a row-level style (applied on top of the span styles).
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Concatenated span text, without any style.
    pub fn plain_text(&self) -> String {
        self.spans
            .iter()
            .map(|span| span.content.as_str())
            .collect()
    }

    /// Convert to the Ratatui type for rendering.
    pub fn into_ratatui(self) -> ratatui::text::Line<'static> {
        let spans = self
            .spans
            .into_iter()
            .map(|span| ratatui::text::Span::styled(span.content, span.style))
            .collect::<Vec<_>>();
        ratatui::text::Line::from(spans).style(self.style)
    }

    /// A terminal cell maps to a whole grapheme boundary, never the middle of
    /// a wide emoji/CJK glyph. Empty cells after the text map to its end.
    pub fn byte_at_cell(&self, cell: usize) -> usize {
        let text = self.plain_text();
        let mut column = 0;
        for (byte, glyph) in text.grapheme_indices(true) {
            let next = column + UnicodeWidthStr::width(glyph);
            if cell < next {
                return byte;
            }
            column = next;
        }
        text.len()
    }

    /// Paint selected graphemes with explicit source-cell foreground and
    /// background colors; the terminal must not retain a REVERSED attribute.
    pub fn highlight(&self, start: usize, end: usize, base_fg: Color, base_bg: Color) -> Self {
        let mut spans = Vec::new();
        let mut offset = 0;
        for span in &self.spans {
            for glyph in span.content.graphemes(true) {
                let selected = offset >= start && offset < end;
                let style = if selected {
                    let source = self.style.patch(span.style);
                    span.style
                        .remove_modifier(Modifier::REVERSED)
                        .fg(source.bg.unwrap_or(base_bg))
                        .bg(source.fg.unwrap_or(base_fg))
                } else {
                    span.style
                };
                spans.push((glyph.to_string(), style));
                offset += glyph.len();
            }
        }
        Self::new(coalesce(spans)).with_style(self.style)
    }
}

impl From<String> for Line {
    fn from(content: String) -> Self {
        Self::plain(content)
    }
}

impl From<&str> for Line {
    fn from(content: &str) -> Self {
        Self::plain(content)
    }
}

impl From<Span> for Line {
    fn from(span: Span) -> Self {
        Self::new(vec![span])
    }
}

impl From<Vec<Span>> for Line {
    fn from(spans: Vec<Span>) -> Self {
        Self::new(spans)
    }
}

/// A sequence of styled rows; the drop-in replacement for `Vec<String>`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lines(pub Vec<Line>);

impl Lines {
    pub fn new(lines: Vec<Line>) -> Self {
        Self(lines)
    }

    /// Row texts without styles.
    pub fn plain_text(&self) -> Vec<String> {
        self.0.iter().map(Line::plain_text).collect()
    }

    /// Convert to the Ratatui type for rendering.
    pub fn into_text(self) -> Text<'static> {
        Text::from(
            self.0
                .into_iter()
                .map(Line::into_ratatui)
                .collect::<Vec<_>>(),
        )
    }
}

impl From<Vec<String>> for Lines {
    fn from(lines: Vec<String>) -> Self {
        Self(lines.into_iter().map(Line::plain).collect())
    }
}

impl From<Vec<Line>> for Lines {
    fn from(lines: Vec<Line>) -> Self {
        Self(lines)
    }
}

/// Display cells occupied by one character (wide CJK and emoji are 2 cells).
pub fn char_width(ch: char) -> usize {
    ratatui::text::Line::from(ch.to_string()).width()
}

/// Display cells occupied by a span.
pub fn span_width(span: &Span) -> usize {
    ratatui::text::Line::from(span.content()).width()
}

/// Word-wrap one styled line to at most `width` display cells, keeping the
/// styles. Breaks at spaces; a word longer than the width is split at the
/// cell boundary. Leading spaces of a continuation row are dropped, exactly
/// like a terminal text wrap.
pub fn wrap_line(line: &Line, width: usize) -> Vec<Line> {
    wrap_line_limited(line, width, usize::MAX)
}

/// At most `limit` wrapped rows; used for bounded Markdown previews.
pub fn wrap_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    wrap_line_with_space_mode(line, width, limit, false)
}

/// Preserve a source separator at a word-wrap boundary when it fits on the
/// preceding row, so its original style is painted without inventing cells.
pub fn wrap_source_space_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    wrap_line_with_space_mode(line, width, limit, true)
}

/// Code fences use the same source-space wrap as Markdown text.
pub fn wrap_code_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    wrap_source_space_line_limited(line, width, limit)
}

fn wrap_line_with_space_mode(
    line: &Line,
    width: usize,
    limit: usize,
    preserve_break_space: bool,
) -> Vec<Line> {
    let max = width.max(1);
    let mut out: Vec<Vec<(String, Style)>> = Vec::new();
    let mut current: Vec<(String, Style)> = Vec::new();
    let mut used = 0usize;
    // Index just past the last space run in `current`; a wrap can break there.
    let mut break_at: Option<usize> = None;
    // Only continuation rows drop their leading spaces; the first row keeps
    // the indentation of the source line.
    let mut continuation = false;
    for span in line.spans() {
        for glyph in span.content().graphemes(true) {
            if out.len() >= limit {
                break;
            }
            let cells = UnicodeWidthStr::width(glyph);
            // A grapheme wider than the target cell cannot fit on any row.
            // Show an overflow glyph without displacing the next grid border.
            let (glyph, cells) = if cells > max {
                ("…", 1)
            } else {
                (glyph, cells)
            };
            if used + cells > max && !current.is_empty() {
                match break_at.take() {
                    Some(index) => {
                        let mut rest = current.split_off(index);
                        while rest.first().is_some_and(|(ch, _)| ch == " ") {
                            rest.remove(0);
                        }
                        let mut head = std::mem::take(&mut current);
                        if !preserve_break_space {
                            while head.last().is_some_and(|(ch, _)| ch == " ") {
                                head.pop();
                            }
                        }
                        out.push(head);
                        current = rest;
                        used = current
                            .iter()
                            .map(|(ch, _)| UnicodeWidthStr::width(ch.as_str()))
                            .sum();
                    }
                    None => {
                        out.push(std::mem::take(&mut current));
                        used = 0;
                    }
                }
                continuation = true;
            }
            if glyph == " " && current.is_empty() && continuation {
                continue;
            }
            current.push((glyph.to_string(), span.style()));
            used += cells;
            if glyph == " " {
                break_at = Some(current.len());
            }
        }
    }
    if out.len() < limit {
        out.push(current);
    }
    out.into_iter()
        .map(|cells| Line::new(coalesce(cells)))
        .collect()
}

/// Wrap every line, preserving the row order.
pub fn wrap_lines(lines: &[Line], width: usize) -> Vec<Line> {
    let mut out = Vec::new();
    for line in lines {
        out.extend(wrap_line(line, width));
    }
    out
}

/// Merge consecutive cells with the same style back into spans.
fn coalesce(cells: Vec<(String, Style)>) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (ch, style) in cells {
        match spans.last_mut() {
            Some(span) if span.style() == style => span.content.push_str(&ch),
            _ => spans.push(Span { content: ch, style }),
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    fn render_paragraph(text: Text<'static>, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| frame.render_widget(Paragraph::new(text), frame.area()))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    #[test]
    fn string_conversions_are_unstyled() {
        let span = Span::from("hi");
        assert_eq!(span.content(), "hi");
        assert_eq!(span.style(), Style::default());

        let line = Line::from(String::from("hi"));
        assert_eq!(line.plain_text(), "hi");
        assert_eq!(line.spans().len(), 1);
        assert_eq!(line.spans()[0].style(), Style::default());

        let lines = Lines::from(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(lines.plain_text(), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn styled_spans_keep_their_style() {
        use ratatui::style::Color;

        let line = Line::new(vec![
            Span::styled("oc ", Style::default().fg(Color::Rgb(1, 2, 3))),
            Span::plain("Idle"),
        ]);
        assert_eq!(line.plain_text(), "oc Idle");
        let rendered = line.into_ratatui();
        assert_eq!(rendered.spans[0].style.fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(rendered.spans[1].style.fg, None);
    }

    #[test]
    fn selection_cells_snap_to_wide_graphemes_and_keep_wrapped_source() {
        let line = Line::plain("a中🧑‍💻b");
        let rows = wrap_line(&line, 3);
        assert_eq!(rows[0].plain_text(), "a中");
        assert_eq!(rows[1].plain_text(), "🧑‍💻b");
        assert_eq!(rows[0].byte_at_cell(1), 1);
        assert_eq!(rows[0].byte_at_cell(2), 1);
        assert_eq!(rows[1].byte_at_cell(0), 0);
        assert_eq!(rows[1].byte_at_cell(1), 0);
        assert_eq!(rows[1].byte_at_cell(2), "🧑‍💻".len());
        let marked = rows[1].highlight(
            0,
            "🧑‍💻".len(),
            Color::Rgb(238, 238, 238),
            Color::Rgb(10, 10, 10),
        );
        assert_eq!(marked.plain_text(), rows[1].plain_text());
        assert_eq!(marked.spans()[0].style().fg, Some(Color::Rgb(10, 10, 10)));
        assert_eq!(
            marked.spans()[0].style().bg,
            Some(Color::Rgb(238, 238, 238))
        );
        assert!(
            !marked.spans()[0]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(
            !rows[1].spans()[0]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn selected_cells_use_source_style_and_preserve_unselected_spans() {
        let bg = Color::Rgb(10, 10, 10);
        let text = Color::Rgb(238, 238, 238);
        let accent = Color::Rgb(100, 200, 100);
        let line = Line::new(vec![
            Span::styled(" ab", Style::default().fg(accent)),
            Span::styled("中z", Style::default().fg(text)),
        ])
        .with_style(Style::default().bg(bg));
        let marked = line.highlight(1, " ab中".len(), text, bg);
        let cells = render_paragraph(Lines::from(vec![marked]).into_text(), 10, 1);
        assert_eq!(cells[(0, 0)].fg, accent);
        assert_eq!(cells[(0, 0)].bg, bg);
        for x in 1..3 {
            assert_eq!(cells[(x, 0)].fg, bg);
            assert_eq!(cells[(x, 0)].bg, accent);
            assert!(!cells[(x, 0)].modifier.contains(Modifier::REVERSED));
        }
        assert_eq!(cells[(3, 0)].fg, bg);
        assert_eq!(cells[(3, 0)].bg, text);
        assert_eq!(cells[(5, 0)].fg, text);
        assert_eq!(cells[(5, 0)].bg, bg);
    }

    /// Unstyled `Vec<String>` conversion must render byte-identically to the
    /// historical `join("\n")` paragraph, styles included (all default).
    #[test]
    fn lines_render_byte_identically_to_joined_strings() {
        let rows = vec![
            "user: привет 🌍".to_string(),
            "ai: ok".to_string(),
            String::new(),
            "panel | /model".to_string(),
        ];
        let legacy = render_paragraph(Text::from(rows.join("\n")), 40, 8);
        let styled = render_paragraph(Lines::from(rows).into_text(), 40, 8);
        assert_eq!(legacy, styled);
    }

    #[test]
    fn wrap_line_keeps_styles_and_splits_long_words() {
        use ratatui::style::Color;

        let red = Style::default().fg(Color::Rgb(255, 0, 0));
        let line = Line::new(vec![Span::styled("alpha ", red), Span::plain("beta gamma")]);
        let wrapped = wrap_line(&line, 10);
        assert_eq!(
            wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
            vec!["alpha", "beta gamma"]
        );
        // The first row keeps the red style from the source span.
        assert_eq!(
            wrapped[0].spans()[0].style().fg,
            Some(Color::Rgb(255, 0, 0))
        );
        // Continuation rows drop leading spaces.
        let wrapped = wrap_line(&Line::plain("aaa   bbb"), 4);
        assert_eq!(
            wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
            vec!["aaa", "bbb"]
        );
        // A word longer than the width splits instead of overflowing.
        let wrapped = wrap_line(&Line::plain("abcdefghij"), 4);
        assert_eq!(
            wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
            vec!["abcd", "efgh", "ij"]
        );
        // Leading indentation of the source line is preserved.
        let wrapped = wrap_line(&Line::plain("   indented"), 20);
        assert_eq!(wrapped[0].plain_text(), "   indented");
        // Wide characters count as two cells.
        let wrapped = wrap_line(&Line::plain("🌍🌍🌍"), 4);
        assert_eq!(
            wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
            vec!["🌍🌍", "🌍"]
        );
        let wrapped = wrap_line(&Line::plain("中🧑‍💻文"), 4);
        assert_eq!(
            wrapped.iter().map(Line::plain_text).collect::<Vec<_>>(),
            vec!["中🧑‍💻", "文"]
        );
        assert_eq!(wrap_line_limited(&Line::plain("abcdefghij"), 2, 2).len(), 2);
    }

    #[test]
    fn code_wrap_keeps_source_separator_style_and_row_limit() {
        use ratatui::style::Color;

        let code = Style::default().fg(Color::Rgb(238, 238, 238));
        let line = Line::new(vec![
            Span::styled("ROW-014 ", code),
            Span::plain("x".repeat(90)),
        ]);
        let code_rows = wrap_code_line_limited(&line, 77, 2);
        assert_eq!(code_rows.len(), 2);
        assert_eq!(code_rows[0].plain_text(), "ROW-014 ");
        assert_eq!(code_rows[0].spans()[0].style(), code);
        assert_eq!(code_rows[1].plain_text(), "x".repeat(77));
        assert_eq!(
            wrap_line_limited(&line, 77, 2)[0].plain_text(),
            "ROW-014",
            "default prose wrap still drops break spaces"
        );
    }

    #[test]
    fn source_space_wrap_only_paints_real_fitting_spaces_and_stays_bounded() {
        use ratatui::style::Color;

        let source = Style::default().fg(Color::Rgb(238, 238, 238));
        let line = Line::new(vec![Span::styled("中🧑‍💻 ", source), Span::plain("back")]);
        let rows = wrap_source_space_line_limited(&line, 6, 2);
        assert_eq!(
            rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
            ["中🧑‍💻 ", "back"]
        );
        assert_eq!(rows[0].spans()[0].style(), source);
        assert!(
            rows.iter()
                .all(|row| row.spans().iter().map(span_width).sum::<usize>() <= 6)
        );
        let no_room = wrap_source_space_line_limited(&Line::styled("abc back", source), 3, 3);
        assert_eq!(
            no_room.iter().map(Line::plain_text).collect::<Vec<_>>(),
            ["abc", "bac", "k"]
        );

        // Exact fit, explicit source end and newline boundaries cannot add a
        // painted cell or an extra row. Only a wrap before the next word does.
        for (text, width) in [("abc def", 7), ("abc ", 4), ("abc", 4)] {
            let rows = wrap_source_space_line_limited(&Line::styled(text, source), width, 2);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].plain_text(), text);
        }
        let lines = [Line::styled("abc", source), Line::styled("back", source)];
        assert_eq!(
            lines
                .iter()
                .flat_map(|line| wrap_source_space_line_limited(line, 4, 2))
                .map(|line| line.plain_text())
                .collect::<Vec<_>>(),
            ["abc", "back"]
        );

        let long = Line::styled(format!("abc {}", "x".repeat(20_000)), source);
        let rows = wrap_source_space_line_limited(&long, 4, 2);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].plain_text(), "abc ");
        assert_eq!(rows[1].plain_text(), "xxxx");
    }

    #[test]
    fn conversion_to_ratatui_matches_the_source_text() {
        let lines = Lines::from(vec!["a".to_string(), "b".to_string()]);
        let text = lines.into_text();
        assert_eq!(text.lines.len(), 2);
        assert_eq!(text.lines[0].to_string(), "a");
        assert_eq!(text.lines[1].to_string(), "b");
    }
}
