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
    /// Layout chrome (gutter, padding, fill, border): painted, never selected
    /// or copied, like OpenTUI's non-selectable renderables.
    decor: bool,
}

impl Span {
    /// Text with an explicit style.
    pub fn styled(content: impl Into<String>, style: Style) -> Self {
        Self {
            content: content.into(),
            style,
            animation: None,
            decor: false,
        }
    }

    /// Non-selectable layout cells with an explicit style.
    pub fn decor(content: impl Into<String>, style: Style) -> Self {
        Self {
            decor: true,
            ..Self::styled(content, style)
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
            decor: self.decor,
        }
    }

    pub fn style(&self) -> Style {
        self.style
    }

    pub fn is_decor(&self) -> bool {
        self.decor
    }

    /// The same cells, restyled; the selectable/decor role is kept.
    pub(crate) fn restyled(&self, style: Style) -> Self {
        Self {
            style,
            ..self.clone()
        }
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
    copy: RowCopy,
    markdown: Option<Box<MarkdownOrigin>>,
}

/// Raw Markdown provenance of a rendered row: the bytes of the physical source
/// line(s) it was drawn from. The renderer records ranges relative to its own
/// input; only assistant text attaches its owning source, so other Markdown
/// (reasoning, compaction, tool output) never claims a raw excerpt.
#[derive(Clone, Debug)]
pub(crate) struct MarkdownOrigin {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) source: Option<crate::history::SourceText>,
}

/// How a painted row joins a copied selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RowCopy {
    /// A row without selectable non-blank text is layout spacing and is
    /// skipped; any other row is its own copied line.
    #[default]
    Auto,
    /// A real (possibly empty) text line, e.g. a blank code or diff line.
    Text,
    /// A soft-wrap continuation of the previous row's logical line: joined
    /// without a newline, as OpenTUI copies its text buffer.
    Wrap,
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
                        spans.push(span.with_content(span.content[..at].to_owned()));
                    }
                    let mut cell = span.with_content(glyph.to_owned());
                    cell.animation = Some(source.clone());
                    spans.push(cell);
                    if at + glyph.len() < span.content.len() {
                        spans.push(span.with_content(span.content[at + glyph.len()..].to_owned()));
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
            copy: RowCopy::Auto,
            markdown: None,
        }
    }

    pub(crate) fn markdown(&self) -> Option<&MarkdownOrigin> {
        self.markdown.as_deref()
    }

    pub(crate) fn with_markdown(mut self, origin: Option<MarkdownOrigin>) -> Self {
        self.markdown = origin.map(Box::new);
        self
    }

    pub fn copy(&self) -> RowCopy {
        self.copy
    }

    /// The same row with each span restyled; cell roles, animation tags and
    /// the copy mode are kept (hover overlays must not change selection).
    pub(crate) fn restyle_spans(&self, mut style: impl FnMut(usize, &Span) -> Style) -> Self {
        let mut line = self.clone();
        for (index, span) in line.spans.iter_mut().enumerate() {
            span.style = style(index, span);
        }
        line
    }

    pub fn with_copy(mut self, copy: RowCopy) -> Self {
        self.copy = copy;
        self
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
    /// Decor cells (gutter, padding, fill) are never highlighted.
    pub fn highlight(&self, start: usize, end: usize, base_fg: Color, base_bg: Color) -> Self {
        let mut spans = Vec::new();
        let mut offset = 0;
        for span in &self.spans {
            for glyph in span.content.graphemes(true) {
                let selected = !span.decor && offset >= start && offset < end;
                let style = if selected {
                    let source = self.style.patch(span.style);
                    span.style
                        .remove_modifier(Modifier::REVERSED)
                        .fg(source.bg.unwrap_or(base_bg))
                        .bg(source.fg.unwrap_or(base_fg))
                } else {
                    span.style
                };
                spans.push((glyph, style, None, 0..0, span.decor));
                offset += glyph.len();
            }
        }
        let mut line = Self::new(coalesce(&spans)).with_style(self.style);
        line.copy = self.copy;
        line.markdown = self.markdown.clone();
        line
    }

    /// Selectable text in the plain-text byte range `start..end`; decor runs
    /// between two text runs join without a separator (OpenTUI joins the
    /// segments of one row by x).
    pub fn selected_text(&self, start: usize, end: usize) -> String {
        let mut out = String::new();
        let mut offset = 0;
        for span in &self.spans {
            let span_end = offset + span.content.len();
            if !span.decor && span_end > start && offset < end {
                let from = start.saturating_sub(offset);
                let to = (end - offset).min(span.content.len());
                if let Some(text) = span.content.get(from..to) {
                    out.push_str(text);
                }
            }
            offset = span_end;
        }
        out
    }

    /// Plain-text byte range of the selectable cells, if any.
    pub fn selectable_range(&self) -> Option<std::ops::Range<usize>> {
        let mut offset = 0;
        let mut range: Option<std::ops::Range<usize>> = None;
        for span in &self.spans {
            let end = offset + span.content.len();
            if !span.decor && end > offset {
                range = Some(range.map_or(offset..end, |range| range.start..end));
            }
            offset = end;
        }
        range
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

/// Leading default-style blank padding of a painted transcript row (older
/// renderers emit it as ordinary text): never selected or copied.
pub(crate) fn selectable_start(line: &Line) -> usize {
    match line.spans() {
        [padding, content, ..]
            if padding.style() == Style::default()
                && !padding.content().is_empty()
                && padding.content().bytes().all(|byte| byte == b' ')
                && !content.content().is_empty() =>
        {
            padding.content().len()
        }
        _ => 0,
    }
}

/// A painted row that can contribute to a copy: real text rows, or any row
/// whose selectable cells hold non-blank text. Layout spacers never do.
pub(crate) fn copyable_row(line: &Line) -> bool {
    line.copy() != RowCopy::Auto
        || line
            .selectable_range()
            .is_some_and(|range| !line.selected_text(range.start, range.end).trim().is_empty())
}

/// OpenTUI `Selection.getSelectedText`: the selectable segments of one row
/// join by x, rows join by newline, and a soft-wrap continuation joins its
/// logical line. Each item is a row with its selected plain-text byte range.
pub(crate) fn copy_text<'a>(
    limit: usize,
    rows: impl Iterator<Item = (&'a Line, usize, usize)>,
) -> Result<Option<String>, &'static str> {
    const TOO_LARGE: &str = "Selection exceeds clipboard size limit";
    let mut result = String::new();
    let mut started = false;
    let mut previous: Option<usize> = None;
    for (index, (line, begin, finish)) in rows.enumerate() {
        if !copyable_row(line) {
            continue;
        }
        let text = line.plain_text();
        let begin = begin.max(selectable_start(line)).min(text.len());
        let finish = finish.min(text.len());
        if begin > finish || (begin == finish && line.copy() != RowCopy::Text) {
            continue;
        }
        if !text.is_char_boundary(begin) || !text.is_char_boundary(finish) {
            return Ok(None);
        }
        let selected = line.selected_text(begin, finish);
        let joined = line.copy() == RowCopy::Wrap && previous.is_some_and(|row| row + 1 == index);
        let separator = usize::from(started && !joined);
        if result.len().saturating_add(selected.len() + separator) > limit {
            return Err(TOO_LARGE);
        }
        if separator == 1 {
            result.push('\n');
        }
        result.push_str(&selected);
        started = true;
        previous = Some(index);
    }
    Ok(started.then_some(result))
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
                span.decor,
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
    let mut copy = line.copy;
    visit_wrapped_line(line, width, usize::MAX, true, true, |cells| {
        emit(
            source_line(cells)
                .with_copy(copy)
                .with_markdown(line.markdown().cloned()),
        );
        copy = RowCopy::Wrap;
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
        // Same one-cell TAB/control projection as `messages::safe_text`.
        let glyph = if glyph == "\t" {
            " "
        } else if glyph.chars().any(char::is_control) {
            "�"
        } else {
            glyph
        };
        (glyph, Style::default(), None, range, false)
    });
    // A checkpoint inside a logical line, and every later row, continue it.
    let mut continuation = has_word;
    visit_wrapped_glyphs(glyphs, width, limit, true, true, has_word, |cells| {
        let copy = if continuation {
            RowCopy::Wrap
        } else {
            RowCopy::Auto
        };
        continuation = true;
        emit(source_line(cells).with_copy(copy))
    });
}

/// End offsets for bounded pages of a table cell, using the table's own word
/// wrapping. Each cell advances by the same number of visual rows per page.
pub(crate) fn table_cell_page_ends(text: &str, width: usize, page_rows: usize) -> Vec<usize> {
    let glyphs = text.grapheme_indices(true).map(|(byte, glyph)| {
        let range = byte..byte + glyph.len();
        let glyph = if glyph == "\t" {
            " "
        } else if glyph.chars().any(char::is_control) {
            "�"
        } else {
            glyph
        };
        (glyph, Style::default(), None, range, false)
    });
    let mut ends = Vec::new();
    let mut rows = 0usize;
    visit_wrapped_glyphs(glyphs, width, usize::MAX, false, false, false, |cells| {
        rows += 1;
        if rows.is_multiple_of(page_rows)
            && let Some(end) = cells.last().map(|cell| cell.3.end)
            && end < text.len()
            && ends.last() != Some(&end)
        {
            ends.push(end);
        }
    });
    if !text.is_empty() {
        ends.push(text.len());
    }
    ends
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
            // Source-space rows keep every separator, so a continuation is
            // copied back onto its logical line without a newline.
            let copy = if out.is_empty() {
                line.copy
            } else if preserve_break_space {
                RowCopy::Wrap
            } else {
                RowCopy::Auto
            };
            // Every visual row of a logical line keeps its source provenance.
            out.push(
                source_line(cells)
                    .with_copy(copy)
                    .with_markdown(line.markdown().cloned()),
            );
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
                    span.decor,
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
    bool,
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
    glyphs: impl Iterator<Item = WrappedCell<'a>>,
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
    while let Some((glyph, style, animation, source_range, decor)) = glyphs.next() {
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
                && !glyphs.peek().is_some_and(|(next, _, _, _, _)| {
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
                    while current.last().is_some_and(|(ch, _, _, _, _)| is_space(ch)) {
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
                        while current
                            .get(rest)
                            .is_some_and(|(ch, _, _, _, _)| is_space(ch))
                        {
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
                        .map(|(ch, _, _, _, _)| UnicodeWidthStr::width(*ch))
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
        current.push((glyph, style, animation, source_range, decor));
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
    for (ch, style, animation, _, decor) in cells {
        match spans.last_mut() {
            Some(span)
                if span.style() == *style
                    && span.decor == *decor
                    && span.animation.is_none()
                    && animation.is_none() =>
            {
                span.content.push_str(ch)
            }
            _ => spans.push(Span {
                content: (*ch).to_owned(),
                style: *style,
                animation: animation.clone(),
                decor: *decor,
            }),
        }
    }
    spans
}

/// Recorded tool output is a terminal byte stream, not markup. Upstream views
/// apply `stripAnsi` and OpenTUI paints each TAB as two cells; CR overwrites
/// its row, as progress output expects. Remaining controls never reach the
/// terminal. Presentation only: recorded output is never rewritten.
pub(crate) fn display_output(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.chars().any(|c| c.is_control() && c != '\n') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut row = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                out.push('\n');
                row = out.len();
            }
            '\r' => {
                if chars
                    .peek()
                    .is_some_and(|next| !matches!(next, '\n' | '\r'))
                {
                    out.truncate(row);
                }
            }
            '\t' => out.push_str("  "),
            '\u{1b}' => match chars.peek() {
                Some('[') => {
                    chars.next();
                    skip_csi(&mut chars);
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    chars.next();
                    skip_control_string(&mut chars);
                }
                Some('\u{20}'..='\u{2f}') => {
                    while chars
                        .next_if(|c| matches!(c, '\u{20}'..='\u{2f}'))
                        .is_some()
                    {}
                    chars.next_if(|c| matches!(c, '\u{30}'..='\u{7e}'));
                }
                Some('\u{30}'..='\u{7e}') => {
                    chars.next();
                }
                _ => {}
            },
            '\u{9b}' => skip_csi(&mut chars),
            '\u{90}' | '\u{98}' | '\u{9d}' | '\u{9e}' | '\u{9f}' => skip_control_string(&mut chars),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars
        .next_if(|c| matches!(c, '\u{20}'..='\u{3f}'))
        .is_some()
    {}
    chars.next_if(|c| matches!(c, '\u{40}'..='\u{7e}'));
}

/// OSC/DCS/SOS/PM/APC end at BEL or ST; an unterminated one ends at its row.
fn skip_control_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(c) = chars.next_if(|c| *c != '\n') {
        match c {
            '\u{7}' | '\u{9c}' => return,
            '\u{1b}' if chars.next_if_eq(&'\\').is_some() => return,
            _ => {}
        }
    }
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
    fn display_output_strips_terminal_controls() {
        let clean = "plain\nunicode café ⇒";
        assert!(matches!(
            display_output(clean),
            std::borrow::Cow::Borrowed(text) if text == clean
        ));
        for (raw, shown) in [
            (
                "\tlibfoo.so (libc6) => /lib/libfoo.so",
                "  libfoo.so (libc6) => /lib/libfoo.so",
            ),
            ("\x1b[1;31mred\x1b[0m text\x1b[K", "red text"),
            ("\u{9b}32mgreen\u{9b}0m", "green"),
            ("\x1b]0;title\x07after", "after"),
            ("\x1b]8;;https://x\x1b\\link\x1b]8;;\x1b\\", "link"),
            ("\x1b(Bcharset\x1b=", "charset"),
            ("10%\r55%\r100%", "100%"),
            ("crlf\r\nnext\r", "crlf\nnext"),
            ("a\x07b\x08c\0d\x7fe", "abcde"),
            ("\x1b[31\nkept", "\nkept"),
            ("\x1b]unterminated\nkept", "\nkept"),
        ] {
            let shown_now = display_output(raw);
            assert_eq!(shown_now, shown, "{raw:?}");
            assert!(!shown_now.chars().any(|c| c.is_control() && c != '\n'));
        }
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
