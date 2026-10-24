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

thread_local! {
    static RENDER_CANCEL: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = const { std::cell::RefCell::new(None) };
}

/// Worker-local cooperative cancellation, not another executor or cache.
pub(crate) fn with_render_cancellation<T>(
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    run: impl FnOnce() -> T,
) -> T {
    struct Restore(Option<std::sync::Arc<std::sync::atomic::AtomicBool>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            RENDER_CANCEL.set(self.0.take());
        }
    }
    let _restore = Restore(RENDER_CANCEL.replace(Some(cancel)));
    run()
}

pub(crate) fn render_cancelled() -> bool {
    RENDER_CANCEL.with_borrow(|flag| {
        flag.as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    })
}

/// Generated cells only. Source text that happens to contain a spinner glyph
/// never receives this tag. Geometry stores tags; paint resolves their clocks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AnimationSource {
    Tool(String),
    Delegation(usize),
    Reasoning,
    Compaction,
}

/// A piece of text with a single style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    content: String,
    style: Style,
    animation: Option<AnimationSource>,
}

impl Span {
    /// Text with an explicit style.
    pub fn styled(content: impl Into<String>, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
            animation: None,
        }
    }

    /// Unstyled text (terminal defaults).
    pub fn plain(content: impl Into<String>) -> Self {
        Self::styled(content, Style::default())
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub(crate) fn with_content(&self, content: String) -> Self {
        Self {
            content,
            style: self.style,
            animation: self.animation.clone(),
        }
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
#[derive(Clone, Debug, Default)]
pub struct Line {
    spans: Vec<Span>,
    style: Style,
    source_range: Option<std::ops::Range<usize>>,
}

impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.spans == other.spans && self.style == other.style
    }
}
impl Eq for Line {}

impl Line {
    pub(crate) fn animate(mut self, byte: usize, source: AnimationSource) -> Self {
        let mut offset = 0;
        let mut spans = Vec::new();
        for span in self.spans {
            let end = offset + span.content.len();
            if byte >= offset && byte < end {
                let at = byte - offset;
                if let Some(glyph) = span.content[at..].graphemes(true).next() {
                    if at > 0 {
                        spans.push(Span::styled(&span.content[..at], span.style));
                    }
                    let mut cell = Span::styled(glyph, span.style);
                    cell.animation = Some(source.clone());
                    spans.push(cell);
                    if at + glyph.len() < span.content.len() {
                        spans.push(Span::styled(&span.content[at + glyph.len()..], span.style));
                    }
                }
            } else {
                spans.push(span);
            }
            offset = end;
        }
        self.spans = spans;
        self
    }

    pub(crate) fn animation_cells(&self) -> Vec<(usize, AnimationSource)> {
        let mut byte = 0;
        self.spans
            .iter()
            .filter_map(|span| {
                let at = byte;
                byte += span.content.len();
                span.animation.clone().map(|source| (at, source))
            })
            .collect()
    }

    /// Preserve hover/selection styling; replace exactly one generated grapheme.
    pub(crate) fn patch_cell(&mut self, byte: usize, glyph: &str) {
        let mut offset = 0;
        for span in &mut self.spans {
            if byte >= offset && byte < offset + span.content.len() {
                let at = byte - offset;
                if let Some(old) = span.content[at..].graphemes(true).next() {
                    let end = at + old.len();
                    span.content.replace_range(at..end, glyph);
                }
                break;
            }
            offset += span.content.len();
        }
    }
    pub fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            style: Style::default(),
            source_range: None,
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

    pub(crate) fn source_range(&self) -> Option<std::ops::Range<usize>> {
        self.source_range.clone()
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
                spans.push((glyph, style, None, 0..0));
                offset += glyph.len();
            }
        }
        Self::new(coalesce(&spans)).with_style(self.style)
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
    wrap_line_with_space_mode(line, width, limit, false, false)
}

/// The number of rows `wrap_line_limited` produces, without building them.
/// Height measurement runs over every retained row; only visible rows need
/// their wrapped spans.
pub fn wrapped_rows(line: &Line, width: usize, limit: usize) -> usize {
    let mut rows = 0;
    visit_wrapped_line(line, width, limit, false, false, |_| rows += 1);
    rows
}

/// OpenTUI character wrap keeps separators at their actual cell boundary.
/// Used by collapsed Shell commands, whose continuation may start with space.
pub(crate) fn wrap_char_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    let max = width.max(1);
    let mut out = Vec::new();
    let mut current = Vec::new();
    let mut used = 0;
    for span in line.spans() {
        for (byte, glyph) in span.content().grapheme_indices(true) {
            if out.len() >= limit {
                return out;
            }
            let cells = UnicodeWidthStr::width(glyph);
            let (glyph, cells) = if cells > max {
                ("…", 1)
            } else {
                (glyph, cells)
            };
            if used + cells > max && !current.is_empty() {
                out.push(Line::new(coalesce(&current)));
                current.clear();
                used = 0;
                if out.len() >= limit {
                    return out;
                }
            }
            current.push((
                glyph,
                span.style(),
                (byte == 0).then(|| span.animation.clone()).flatten(),
                byte..byte + glyph.len(),
            ));
            used += cells;
        }
    }
    if out.len() < limit && (!current.is_empty() || out.is_empty()) {
        out.push(Line::new(coalesce(&current)));
    }
    out
}

/// OpenTUI word boundaries preserve a fitting source separator on the preceding
/// row, so its original style is painted without inventing cells.
pub fn wrap_source_space_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    wrap_line_with_space_mode(line, width, limit, true, true)
}

/// Stream the same source-space rows without retaining a whole long message.
pub(crate) fn visit_source_space_line(line: &Line, width: usize, mut emit: impl FnMut(Line)) {
    visit_wrapped_line(line, width, usize::MAX, true, true, |cells| {
        emit(source_line(cells))
    });
}

/// The same wrapper over a borrowed source, with exact source-byte boundaries.
/// A checkpoint starts at an already chosen visual-row boundary.
pub(crate) fn visit_source_rows(
    text: &str,
    width: usize,
    limit: usize,
    has_word: bool,
    mut emit: impl FnMut(Line),
) {
    let glyphs = text.grapheme_indices(true).map(|(byte, glyph)| {
        let range = byte..byte + glyph.len();
        let glyph = if glyph.chars().any(char::is_control) {
            "�"
        } else {
            glyph
        };
        (glyph, Style::default(), None, range)
    });
    visit_wrapped_glyphs(glyphs, width, limit, true, true, has_word, |cells| {
        emit(source_line(cells))
    });
}

/// Plain permission text uses OpenTUI word boundaries,
/// with a grapheme-safe fallback for a resource longer than the available row.
pub(crate) fn wrap_permission_line_limited(line: &Line, width: usize, limit: usize) -> Vec<Line> {
    wrap_line_with_space_mode(line, width, limit, false, true)
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
    opentui_breaks: bool,
) -> Vec<Line> {
    let mut out = Vec::new();
    visit_wrapped_line(
        line,
        width,
        limit,
        preserve_break_space,
        opentui_breaks,
        |cells| {
            out.push(source_line(cells));
        },
    );
    out
}

fn visit_wrapped_line<'a>(
    line: &'a Line,
    width: usize,
    limit: usize,
    preserve_break_space: bool,
    opentui_breaks: bool,
    emit: impl FnMut(&[WrappedCell<'a>]),
) {
    let mut base = 0;
    let glyphs = line.spans().iter().flat_map(|span| {
        let start = base;
        base += span.content().len();
        span.content()
            .grapheme_indices(true)
            .map(move |(byte, glyph)| {
                (
                    glyph,
                    span.style(),
                    (byte == 0).then(|| span.animation.clone()).flatten(),
                    start + byte..start + byte + glyph.len(),
                )
            })
    });
    visit_wrapped_glyphs(
        glyphs,
        width,
        limit,
        preserve_break_space,
        opentui_breaks,
        false,
        emit,
    );
}

/// One display grapheme borrowed from its source span (no per-glyph copy).
type WrappedCell<'a> = (
    &'a str,
    Style,
    Option<AnimationSource>,
    std::ops::Range<usize>,
);

fn source_line(cells: &[WrappedCell]) -> Line {
    let range = cells
        .first()
        .zip(cells.last())
        .map(|(first, last)| first.3.start..last.3.end);
    let mut line = Line::new(coalesce(cells));
    line.source_range = range;
    line
}

fn visit_wrapped_glyphs<'a>(
    glyphs: impl Iterator<
        Item = (
            &'a str,
            Style,
            Option<AnimationSource>,
            std::ops::Range<usize>,
        ),
    >,
    width: usize,
    limit: usize,
    preserve_break_space: bool,
    opentui_breaks: bool,
    initial_has_word: bool,
    mut emit: impl FnMut(&[WrappedCell<'a>]),
) {
    let is_space = |ch: &str| {
        if opentui_breaks {
            wrap_whitespace(ch)
        } else {
            ch == " "
        }
    };
    let max = width.max(1);
    let mut emitted = 0;
    // One row buffer for the whole line: emitted heads are slices of it.
    let mut current: Vec<WrappedCell<'a>> = Vec::new();
    let mut used = 0usize;
    // Last break and whether its head is logical-leading indentation. That
    // property belongs to the break, not the subsequently collected word.
    let mut break_at: Option<(usize, bool)> = None;
    // Only continuation rows drop their leading spaces; the first row keeps
    // the indentation of the source line.
    let mut continuation = initial_has_word;
    let mut source_has_word = initial_has_word;
    let mut previous_class = 0;
    let mut previous_glyph: Option<&str> = None;
    let mut glyphs = glyphs.peekable();
    let mut visited = 0usize;
    while let Some((glyph, style, animation, source_range)) = glyphs.next() {
        visited += 1;
        if visited.is_multiple_of(256) && render_cancelled() {
            return;
        }
        if emitted >= limit {
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
        let continues_cluster = cells == 0
            && previous_glyph
                .is_some_and(|previous| format!("{previous}{glyph}").graphemes(true).count() == 1);
        previous_glyph = Some(glyph);
        let whitespace = if opentui_breaks {
            wrap_whitespace(glyph)
                && !glyphs.peek().is_some_and(|(next, _, _, _)| {
                    // A styled-span edge is not a grapheme boundary. Keep a
                    // whitespace base when the next span carries its accent.
                    UnicodeWidthStr::width(*next) == 0
                        && !wrap_whitespace(next)
                        && format!("{glyph}{next}").graphemes(true).count() == 1
                })
        } else {
            glyph == " "
        };
        let class = glyph.chars().next().map_or(0, wrap_word_class);
        if opentui_breaks
            && matches!((previous_class, class), (1, 2) | (2, 1))
            && !current.is_empty()
        {
            break_at = Some((current.len(), false));
        }
        while used + cells > max && !current.is_empty() {
            // A separator following an exactly fitting word belongs to the
            // next wrap boundary, not the earlier boundary before that word.
            // Keep the full row and suppress only the overflowing space.
            let boundary = if whitespace {
                break_at = None;
                if !preserve_break_space && (!opentui_breaks || source_has_word) {
                    while current.last().is_some_and(|(ch, _, _, _)| is_space(ch)) {
                        current.pop();
                    }
                }
                None
            } else {
                break_at.take()
            };
            match boundary {
                Some((index, leading_indent)) => {
                    let mut rest = index;
                    if !opentui_breaks || source_has_word {
                        while current.get(rest).is_some_and(|(ch, _, _, _)| is_space(ch)) {
                            rest += 1;
                        }
                    }
                    let mut head = index;
                    if !preserve_break_space && (!opentui_breaks || !leading_indent) {
                        while head > 0 && is_space(current[head - 1].0) {
                            head -= 1;
                        }
                    }
                    emit(&current[..head]);
                    emitted += 1;
                    current.drain(..rest);
                    used = current
                        .iter()
                        .map(|(ch, _, _, _)| UnicodeWidthStr::width(*ch))
                        .sum();
                }
                None => {
                    emit(&current);
                    current.clear();
                    emitted += 1;
                    used = 0;
                }
            }
            continuation = true;
            if emitted >= limit {
                break;
            }
        }
        if emitted >= limit {
            break;
        }
        if whitespace && current.is_empty() && continuation && (!opentui_breaks || source_has_word)
        {
            previous_class = class;
            continue;
        }
        let extend_break = continues_cluster && break_at.is_some_and(|(at, _)| at == current.len());
        let leading_indent = if extend_break {
            break_at.is_some_and(|(_, leading)| leading)
        } else {
            opentui_breaks && whitespace && !source_has_word
        };
        current.push((glyph, style, animation, source_range));
        used += cells;
        source_has_word |= !whitespace;
        // A zero-cell combining suffix in another style span keeps the
        // grapheme's initial script class; span edges are not word edges.
        if !continues_cluster {
            previous_class = class;
        }
        if extend_break || whitespace || (opentui_breaks && wrap_break_after(glyph)) {
            break_at = Some((current.len(), leading_indent));
        }
    }
    if emitted < limit && (!current.is_empty() || emitted == 0) {
        emit(&current);
    }
}

// Pinned OpenTUI 0.5.10 native utf8.zig layout-wrap metadata. Callers have
// already expanded tabs; these are display graphemes, not source-byte offsets.
fn wrap_whitespace(glyph: &str) -> bool {
    // utf8.zig:1716: mixed whitespace/combining clusters are preserved words,
    // not separators that continuation or trailing-space handling may discard.
    glyph.chars().all(wrap_whitespace_char)
}

pub(crate) fn wrap_whitespace_char(ch: char) -> bool {
    matches!(
        ch,
        ' ' | '\t' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200b}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

fn wrap_break_after(glyph: &str) -> bool {
    // Preserved-whitespace clusters still supply a break AFTER their bytes.
    glyph.chars().any(|ch| {
        wrap_whitespace_char(ch)
            || matches!(
                ch,
                '-' | '/'
                    | '\\'
                    | '.'
                    | ','
                    | ';'
                    | ':'
                    | '!'
                    | '?'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '\u{00ad}'
                    | '\u{2010}'
                    | '\u{3001}'
                    | '\u{3002}'
                    | '\u{ff01}'
                    | '\u{ff0c}'
                    | '\u{ff1a}'
                    | '\u{ff1f}'
            )
    })
}

fn wrap_word_class(ch: char) -> u8 {
    if ch.is_ascii_alphanumeric() || ch == '_' {
        1
    } else if matches!(ch, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' |
        '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{2a6df}' |
        '\u{2a700}'..='\u{2b73f}' | '\u{2b740}'..='\u{2b81f}' |
        '\u{2b820}'..='\u{2ceaf}' | '\u{2ceb0}'..='\u{2ebef}' |
        '\u{2ebf0}'..='\u{2ee5d}' | '\u{2f800}'..='\u{2fa1f}' |
        '\u{3040}'..='\u{30ff}' | '\u{31f0}'..='\u{31ff}' |
        '\u{ff66}'..='\u{ff9d}' | '\u{1100}'..='\u{11ff}' |
        '\u{3130}'..='\u{318f}' | '\u{a960}'..='\u{a97f}' |
        '\u{ac00}'..='\u{d7ff}')
    {
        2
    } else {
        0
    }
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
fn coalesce(cells: &[WrappedCell]) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    for (ch, style, animation, _) in cells {
        match spans.last_mut() {
            Some(span)
                if span.style() == *style && span.animation.is_none() && animation.is_none() =>
            {
                span.content.push_str(ch)
            }
            _ => spans.push(Span {
                content: (*ch).to_owned(),
                style: *style,
                animation: animation.clone(),
            }),
        }
    }
    spans
}

#[cfg(test)]
#[path = "styled/wrap_tests.rs"]
mod wrap_tests;

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
            vec!["alpha beta", "gamma"]
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
    fn exact_fit_word_stays_before_an_overflowing_source_separator() {
        let accent = Style::default().fg(Color::Rgb(100, 200, 100));
        let line = Line::new(vec![
            Span::plain("alpha "),
            Span::styled("beta", accent),
            Span::plain("   gamma"),
        ]);
        for wrap in [
            wrap_line_limited,
            wrap_source_space_line_limited,
            wrap_permission_line_limited,
            wrap_code_line_limited,
        ] {
            let rows = wrap(&line, 10, 3);
            assert_eq!(
                rows.iter().map(Line::plain_text).collect::<Vec<_>>(),
                ["alpha beta", "gamma"]
            );
            assert_eq!(rows[0].spans().last().unwrap().style(), accent);
            assert_eq!(wrap(&line, 10, 1), rows[..1]);
            assert_eq!(
                wrap(&Line::plain("ab 界界 tail"), 7, 3)
                    .iter()
                    .map(Line::plain_text)
                    .collect::<Vec<_>>(),
                ["ab 界界", "tail"]
            );
            assert_eq!(
                wrap(&Line::plain("alpha beta   "), 10, 3)
                    .iter()
                    .map(Line::plain_text)
                    .collect::<Vec<_>>(),
                ["alpha beta"],
                "suppressed overflow whitespace must not create an empty final row"
            );
        }
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
