//! Upstream v2.0.12 message rendering: user blocks with chips, assistant
//! markdown, collapsed reasoning and the assistant footer.
//!
//! Geometry and colors cite the upstream sources at tag `v2.0.12`
//! (`packages/tui/src/**`). Markdown structure uses pulldown-cmark events;
//! syntax tokens still cover a documented subset of upstream grammars.
//!
//! Renderers:
//!
//! - user message: left `┃` border in the agent color, raised background,
//!   `padding 1/2`, then the skill/file chip rows
//!   (`routes/session/index.tsx:2273-2398`);
//! - assistant text: `paddingLeft=3` markdown (`message-parts.tsx:147-174`);
//! - reasoning: `paddingLeft=3`, collapsed upstream default (`thinkingMode`
//!   defaults to `"hide"`, `routes/session/index.tsx:226`): a static spinner
//!   header while running and `+ Thought: … · <duration>` once complete
//!   (`routes/session/index.tsx:1765-1815`, `message-parts.tsx:98-145`);
//! - assistant footer: `Agent · model · duration · N tok/s · interrupted`
//!   (`routes/session/index.tsx:1934-1985`).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use std::{
    cell::RefCell,
    collections::VecDeque,
    hash::{Hash, Hasher},
};
use unicode_segmentation::UnicodeSegmentation;

use crate::history::HistoryRow;
use crate::styled::{self, Line, Span};
use crate::theme::{MarkdownToken, SyntaxToken, Theme, ThemeMode};

/// Assistant, reasoning and footer left padding: `paddingLeft={3}`
/// (`routes/session/message-parts.tsx:51,158`, `routes/session/index.tsx:1940`).
pub const MESSAGE_PADDING: usize = 3;
/// User message inner padding: `paddingLeft={2}`
/// (`routes/session/index.tsx:2330-2335`).
pub const USER_PADDING: usize = 2;
/// The model field shows from 28 terminal columns
/// (`routes/session/index.tsx:1964-1966`).
pub const FOOTER_MODEL_MIN_WIDTH: u16 = 28;
/// The duration field is hidden in the 28..36 column band
/// (`routes/session/index.tsx:1967-1969`).
pub const FOOTER_DURATION_MIN_WIDTH: u16 = 36;
/// `InlineToolRow` icon column width (`message-parts.tsx:18,213-219`).
pub const INLINE_ICON_WIDTH: usize = 2;
const MAX_MARKDOWN_ROWS: usize = 512;
const MAX_CACHED_BYTES: usize = 512 * 1024;
const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;
const LIVE_MARKDOWN_BYTES: usize = 16 * 1024;

struct CachedBlock {
    part: (i64, usize, usize),
    revision: u64,
    width: u16,
    theme: crate::theme::ThemeMode,
    lines: Vec<Line>,
    bytes: usize,
}

// The seek index contains source offsets and row counts, never styled history.
// Only the pages intersecting the viewport are sent to the Markdown parser.
struct IndexedPart {
    part: (i64, usize),
    revision: u64,
    width: u16,
    pages: Vec<SourcePage>,
}

#[derive(Clone)]
struct SourcePage {
    start: usize,
    end: usize,
    table_header: Option<(usize, usize)>,
    table_widths: Option<Vec<usize>>,
    table_segment: Option<TableSegment>,
    continuation: bool,
    last_table_page: bool,
    fence: Option<String>,
    list_number: Option<u64>,
    height: usize,
}

#[derive(Clone)]
struct TableSegment {
    prefix: String,
    suffix: String,
    same_row: bool,
    terminal_newline: bool,
}

struct LongCellSegments {
    prefix: String,
    suffix: String,
    fragments: Vec<(usize, usize)>,
}

/// Per-session completed-part cache, bounded independently of history. The
/// content hash is the part revision; width and theme invalidate geometry.
#[derive(Default)]
pub(crate) struct MarkdownCache {
    blocks: VecDeque<CachedBlock>,
    bytes: usize,
    indexes: VecDeque<IndexedPart>,
    #[cfg(test)]
    parses: usize,
    #[cfg(test)]
    parsed_bytes: usize,
}

impl MarkdownCache {
    fn pages(&mut self, part: (i64, usize), text: &str, width: u16) -> Vec<SourcePage> {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let revision = hasher.finish();
        if let Some(index) = self
            .indexes
            .iter()
            .find(|p| p.part == part && p.revision == revision && p.width == width)
        {
            return index.pages.clone();
        }
        let pages = index_source(text, width);
        self.indexes.retain(|p| p.part != part);
        self.indexes.push_back(IndexedPart {
            part,
            revision,
            width,
            pages: pages.clone(),
        });
        // The index is bounded by the loaded history window, with a fixed
        // ceiling for pathological numbers of independently paged parts.
        while self.indexes.len() > 32 || self.index_bytes() > MAX_INDEX_BYTES {
            self.indexes.pop_front();
        }
        pages
    }

    fn update_page_height(&mut self, part: (i64, usize), width: u16, page: usize, height: usize) {
        if let Some(index) = self
            .indexes
            .iter_mut()
            .find(|p| p.part == part && p.width == width)
            && let Some(entry) = index.pages.get_mut(page)
        {
            entry.height = height;
        }
    }

    fn index_bytes(&self) -> usize {
        self.indexes
            .iter()
            .map(|p| {
                p.pages
                    .iter()
                    .map(|page| {
                        std::mem::size_of::<SourcePage>()
                            + page.fence.as_ref().map_or(0, String::len)
                            + page
                                .table_widths
                                .as_ref()
                                .map_or(0, |cols| cols.len() * std::mem::size_of::<usize>())
                            + page
                                .table_segment
                                .as_ref()
                                .map_or(0, |segment| segment.prefix.len() + segment.suffix.len())
                    })
                    .sum::<usize>()
            })
            .sum()
    }
    fn render(
        &mut self,
        part: (i64, usize, usize),
        text: &str,
        theme: &Theme,
        width: u16,
    ) -> Vec<Line> {
        self.render_with_widths(part, text, theme, width, None)
    }

    fn render_table_page(
        &mut self,
        part: (i64, usize, usize),
        text: &str,
        theme: &Theme,
        width: u16,
        columns: &[usize],
    ) -> Vec<Line> {
        self.render_with_widths(part, text, theme, width, Some(columns))
    }

    fn render_with_widths(
        &mut self,
        part: (i64, usize, usize),
        text: &str,
        theme: &Theme,
        width: u16,
        columns: Option<&[usize]>,
    ) -> Vec<Line> {
        let mut end = text.len().min(LIVE_MARKDOWN_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let omitted = end < text.len();
        let text = &text[..end];
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        columns.hash(&mut hasher);
        let revision = hasher.finish();
        if let Some(position) = self.blocks.iter().position(|b| {
            b.part == part && b.revision == revision && b.width == width && b.theme == theme.mode()
        }) {
            let block = self.blocks.remove(position).expect("cache index");
            let lines = block.lines.clone();
            self.blocks.push_back(block);
            return if omitted {
                with_preview_limit(lines, theme)
            } else {
                lines
            };
        }
        #[cfg(test)]
        {
            self.parses += 1;
            self.parsed_bytes += text.len();
        }
        let lines = markdown_block_with_widths(text, theme, width, columns);
        let bytes: usize = lines
            .iter()
            .flat_map(Line::spans)
            .map(|s| s.content().len())
            .sum();
        if bytes <= MAX_CACHED_BYTES {
            if let Some(position) = self
                .blocks
                .iter()
                .position(|b| b.part == part && b.width == width && b.theme == theme.mode())
            {
                let old = self.blocks.remove(position).expect("cache index");
                self.bytes -= old.bytes;
            }
            while self.bytes.saturating_add(bytes) > MAX_CACHED_BYTES || self.blocks.len() >= 240 {
                if let Some(old) = self.blocks.pop_front() {
                    self.bytes -= old.bytes;
                } else {
                    break;
                }
            }
            self.bytes += bytes;
            self.blocks.push_back(CachedBlock {
                part,
                revision,
                width,
                theme: theme.mode(),
                lines: lines.clone(),
                bytes,
            });
        }
        if omitted {
            with_preview_limit(lines, theme)
        } else {
            lines
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.bytes + self.index_bytes()
    }
}

fn with_preview_limit(mut lines: Vec<Line>, theme: &Theme) -> Vec<Line> {
    lines.push(Line::styled(
        "   … [Markdown preview limited; remaining text available in history]",
        Style::default().fg(theme.text_muted()),
    ));
    lines
}

/// Kind of one user-message chip (`routes/session/index.tsx:2346-2393`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipKind {
    /// Skill chip, label `skill`.
    Skill,
    /// File chip, label `file`.
    File,
    /// Directory chip, label `dir`.
    Dir,
}

impl ChipKind {
    /// Upstream chip label (`:2357`, `:2380-2381`).
    pub const fn label(self) -> &'static str {
        match self {
            ChipKind::Skill => "skill",
            ChipKind::File => "file",
            ChipKind::Dir => "dir",
        }
    }
}

/// One skill/file chip under a user message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// Chip kind; selects the label.
    pub kind: ChipKind,
    /// Displayed name (`skill.name` / `file.name`).
    pub name: String,
}

/// Collapsed reasoning block attached to an assistant row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningBlock {
    /// Raw reasoning text (summary deltas concatenated).
    pub text: String,
    /// Reasoning duration in milliseconds; `None`/`0` hides the field.
    pub duration_ms: Option<u64>,
    /// True while the block still streams (`!completed` upstream).
    pub running: bool,
    /// Session-local presentation mode; history and provider payloads stay unchanged.
    pub expanded: bool,
}

/// Assistant footer data. Committed history rows carry none: storage keeps
/// `(id, session_id, seq, role, text)` only, so the footer is omitted for
/// historical messages instead of printing invented numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssistantMeta {
    /// Model label (`provider/id`; our catalog DTOs carry no display name).
    pub model: Option<String>,
    /// Turn wall time in milliseconds (`turnDuration`).
    pub duration_ms: Option<u64>,
    /// Input tokens of the last provider round, when reported.
    pub input_tokens: Option<u64>,
    /// Output tokens summed over the turn's rounds, when reported.
    pub output_tokens: Option<u64>,
    /// Provider-active streaming time in milliseconds, when reported.
    pub streamed_ms: Option<u64>,
    /// Upstream `error.message === "Step interrupted"`.
    pub interrupted: bool,
    /// Exact terminal state, when supplied by the owning application.
    pub status: Option<String>,
    /// Agent categorical slot pinned at generation time.
    pub agent_color_index: Option<usize>,
}

/// Render the whole transcript, wrapped to the content-box `width`.
///
/// `width == 0` is the unbounded text projection: no wrapping and no
/// background padding (used for scroll metrics and plain-text assertions).
/// `terminal_width` drives the footer breakpoints (upstream reads
/// `ctx.terminal.width`, `routes/session/index.tsx:1964-1976`); `agent_color`
/// resolves a row's agent (or `None`) to the categorical agent color.
pub fn transcript(
    rows: &[HistoryRow],
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: impl Fn(Option<&str>) -> Color,
) -> Vec<Line> {
    transcript_with_cache(rows, theme, width, terminal_width, agent_color, None)
}

pub(crate) fn transcript_with_cache(
    rows: &[HistoryRow],
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let mut out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        out.extend(render_row(
            row,
            index,
            theme,
            width,
            terminal_width,
            &agent_color,
            cache,
        ));
    }
    out
}

fn render_row(
    row: &HistoryRow,
    index: usize,
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let lines = match row.role.as_str() {
        "user" => user_block(row, theme, width, agent_color),
        "assistant" => {
            assistant_block(row, index, theme, width, terminal_width, agent_color, cache)
        }
        "tool" => {
            if let Some(card) = &row.tool {
                let mut lines = vec![Line::plain("")];
                lines.extend(crate::tools::tool_block(card, theme, width));
                lines
            } else {
                notice_block(row)
            }
        }
        _ => notice_block(row),
    };
    // Markdown event text is already cleaned; reasoning titles, user chips,
    // footer metadata and recorded tool output can bypass that parser.
    lines.into_iter().map(sanitize_line).collect()
}

fn sanitize_line(line: Line) -> Line {
    if !line
        .spans()
        .iter()
        .any(|span| span.content().chars().any(char::is_control))
    {
        return line;
    }
    Line::new(
        line.spans()
            .iter()
            .map(|span| Span::styled(safe_text(span.content()), span.style()))
            .collect(),
    )
    .with_style(line.style())
}

/// Visit independently bounded text pages. A row's footer is a separate block;
/// a huge user message never needs to become one Vec of rendered lines.
fn visit_row_blocks(
    row: &HistoryRow,
    identity: (usize, bool),
    theme: &Theme,
    widths: (u16, u16),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    mut emit: impl FnMut(Vec<Line>),
) {
    let (index, live) = identity;
    let (width, terminal_width) = widths;
    if row.role == "user" && width > 0 {
        let bg = theme.user_message_background();
        let color = user_agent_color(row, theme, agent_color);
        let border = Style::default().fg(color).bg(bg);
        emit(vec![user_row(
            &[Span::styled("┃", border)],
            bg,
            width as usize,
        )]);
        if !row.text.is_empty() {
            let inner = (width as usize).saturating_sub(1 + USER_PADDING).max(1);
            source_chunks(&row.text, (inner * 128).min(4096), |chunk, _| {
                let mut lines = Vec::new();
                let body = Style::default().fg(theme.text()).bg(bg);
                for raw in chunk.strip_suffix('\n').unwrap_or(chunk).split('\n') {
                    let line = Line::new(vec![Span::styled(safe_text(raw), body)]);
                    for wrapped in styled::wrap_line_limited(&line, inner, MAX_MARKDOWN_ROWS) {
                        let mut spans = vec![
                            Span::styled("┃", border),
                            Span::styled(" ".repeat(USER_PADDING), body),
                        ];
                        spans.extend(wrapped.spans().iter().cloned());
                        lines.push(user_row(&spans, bg, width as usize));
                    }
                }
                emit(lines);
            });
            if row.text.ends_with('\n') {
                emit(vec![user_row(
                    &[
                        Span::styled("┃", border),
                        Span::styled(
                            " ".repeat(USER_PADDING),
                            Style::default().fg(theme.text()).bg(bg),
                        ),
                    ],
                    bg,
                    width as usize,
                )]);
            }
        }
        if !row.chips.is_empty() {
            let mut chip_lines = vec![user_row(&[Span::styled("┃", border)], bg, width as usize)];
            let inner = (width as usize).saturating_sub(1 + USER_PADDING).max(1);
            for chips in chip_rows(&row.chips, theme, inner) {
                let mut spans = vec![
                    Span::styled("┃", border),
                    Span::styled(
                        " ".repeat(USER_PADDING),
                        Style::default().fg(theme.text()).bg(bg),
                    ),
                ];
                spans.extend(chips);
                chip_lines.push(user_row(&spans, bg, width as usize));
            }
            emit(chip_lines.into_iter().map(sanitize_line).collect());
        }
        emit(vec![user_row(
            &[Span::styled("┃", border)],
            bg,
            width as usize,
        )]);
        return;
    }
    if row.role == "assistant" && width > 0 {
        visit_assistant_indexed(
            row,
            (index, live),
            theme,
            (width, terminal_width),
            agent_color,
            cache,
            |_, render| emit(render()),
        );
        return;
    }
    emit(render_row(
        row,
        index,
        theme,
        width,
        terminal_width,
        agent_color,
        Some(cache),
    ));
}

fn visit_assistant_indexed(
    row: &HistoryRow,
    identity: (usize, bool),
    theme: &Theme,
    widths: (u16, u16),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    mut emit: impl FnMut(usize, &mut dyn FnMut() -> Vec<Line>),
) {
    let (index, live) = identity;
    let (width, terminal_width) = widths;
    if let Some(reasoning) = &row.reasoning {
        let lines = reasoning_lines(reasoning, theme, width);
        emit(lines.len(), &mut || lines.clone());
    }
    if !row.text.trim().is_empty() {
        emit(1, &mut || vec![Line::plain("")]);
        if live && row.text.len() > LIVE_MARKDOWN_BYTES {
            let mut end = LIVE_MARKDOWN_BYTES;
            while !row.text.is_char_boundary(end) {
                end -= 1;
            }
            let preview = &row.text[..end];
            // The live preview stays bounded; a completed part is indexed in
            // full and can be scrolled, including the region past this limit.
            let height = estimated_lines(preview, width) + 1;
            emit(height, &mut || {
                let mut lines =
                    cache
                        .borrow_mut()
                        .render((row.seq, index, 0), preview, theme, width);
                lines.push(Line::styled("   … [Live Markdown preview limited; full response available in history after completion]", Style::default().fg(theme.text_muted())));
                lines
            });
        } else {
            let pages = cache.borrow_mut().pages((row.seq, index), &row.text, width);
            for (number, page) in pages.into_iter().enumerate() {
                let height = page.height;
                emit(height, &mut || {
                    let mut source = String::new();
                    if let Some((start, end)) = page.table_header {
                        source.push_str(&row.text[start..end]);
                    }
                    if let Some(open) = &page.fence {
                        source.push_str(open);
                        source.push('\n');
                    }
                    let chunk = &row.text[page.start..page.end];
                    if let Some(segment) = &page.table_segment {
                        source.push_str(&segment.prefix);
                        source.push_str(chunk);
                        source.push_str(&segment.suffix);
                    } else if let Some(number) = page.list_number {
                        // CommonMark numbers following a page boundary must
                        // continue from the original list, even if all source
                        // markers read `1.`.
                        if let Some((_, rest)) = ordered_marker(chunk.lines().next().unwrap_or(""))
                        {
                            source.push_str(&number.to_string());
                            let marker_len = chunk.lines().next().unwrap_or("").len() - rest.len();
                            source.push(chunk.as_bytes()[marker_len - 1] as char);
                            source.push_str(&chunk[marker_len..]);
                        } else {
                            source.push_str(chunk);
                        }
                    } else {
                        source.push_str(chunk);
                    }
                    if page.table_header.is_some() && !page.last_table_page {
                        // Table pages are stitched at a row separator. A
                        // synthetic header is needed by CommonMark on each
                        // continuation but must not appear in the viewport.
                        if source.ends_with('\n') {
                            source.pop();
                        }
                    } else if !page
                        .table_segment
                        .as_ref()
                        .is_some_and(|s| s.terminal_newline)
                        && page.end < row.text.len()
                        && source.ends_with('\n')
                    {
                        source.pop();
                    }
                    let mut lines = if let Some(columns) = &page.table_widths {
                        // All slices of the same table share widths measured
                        // over its actual source rows, including off-screen
                        // cells that could otherwise change the grid mid-table.
                        let mut cache = cache.borrow_mut();
                        cache.render_table_page(
                            (row.seq, index, number),
                            &source,
                            theme,
                            width,
                            columns,
                        )
                    } else {
                        cache
                            .borrow_mut()
                            .render((row.seq, index, number), &source, theme, width)
                    };
                    if page.table_header.is_some() {
                        if page.continuation {
                            let hidden = if page.table_segment.as_ref().is_some_and(|s| s.same_row)
                            {
                                3
                            } else {
                                2
                            };
                            lines.drain(..hidden.min(lines.len()));
                        }
                        if !page.last_table_page
                            && lines
                                .last()
                                .is_some_and(|line| line.plain_text().contains('└'))
                        {
                            lines.pop();
                        }
                    }
                    cache.borrow_mut().update_page_height(
                        (row.seq, index),
                        width,
                        number,
                        lines.len(),
                    );
                    lines
                });
            }
        }
    }
    if let Some(meta) = &row.meta
        && let Some(footer) = footer_line(
            row.agent.as_deref(),
            meta,
            theme,
            terminal_width,
            agent_color,
        )
    {
        emit(2, &mut || {
            vec![Line::plain(""), sanitize_line(footer.clone())]
        });
    }
}

/// Cut at grapheme boundaries and prefer newlines. No text is discarded; each
/// page has at most 128 source lines and a width-dependent byte budget.
fn source_chunks(text: &str, max_bytes: usize, mut emit: impl FnMut(&str, usize)) {
    let max_bytes = max_bytes.max(1);
    let mut start = 0;
    let mut lines = 0;
    let mut page = 0;
    for (offset, glyph) in text.grapheme_indices(true) {
        if offset > start && (offset + glyph.len() - start > max_bytes || lines >= 128) {
            emit(&text[start..offset], page);
            page += 1;
            start = offset;
            lines = 0;
        }
        if glyph.ends_with('\n') {
            lines += 1;
        }
    }
    if start < text.len() {
        emit(&text[start..], page);
    }
}

/// Track only fence boundaries between source pages. pulldown-cmark still
/// owns parsing each page; this scan does not tokenize inline Markdown.
fn advance_fence(chunk: &str, open: &mut Option<String>, at_line_start: &mut bool) {
    for line in chunk.split_inclusive('\n') {
        if *at_line_start {
            let text = line.trim_end_matches(['\r', '\n']);
            let indentation = text.len() - text.trim_start_matches(' ').len();
            if indentation <= 3 {
                let text = &text[indentation..];
                if let Some(marker) = text.chars().next().filter(|c| *c == '`' || *c == '~') {
                    let count = text.chars().take_while(|c| *c == marker).count();
                    if count >= 3 {
                        match open {
                            Some(previous)
                                if previous.starts_with(marker)
                                    && count
                                        >= previous
                                            .chars()
                                            .take_while(|c| *c == marker)
                                            .count()
                                    && text[count..].trim().is_empty() =>
                            {
                                *open = None
                            }
                            None => *open = Some(text.to_string()),
                            _ => {}
                        }
                    }
                }
            }
        }
        *at_line_start = line.ends_with('\n');
    }
}

fn ordered_marker(line: &str) -> Option<(u64, &str)> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 9 || !matches!(line.as_bytes().get(digits), Some(b'.' | b')')) {
        return None;
    }
    let rest = line.get(digits + 1..)?;
    if !rest.starts_with(' ') {
        return None;
    }
    Some((line[..digits].parse().ok()?, rest))
}

fn estimated_lines(text: &str, width: u16) -> usize {
    // Cell counting is deliberately independent of Markdown parsing; this is
    // the scroll seek estimate for non-visible pages. Long visual lines use
    // the same grapheme width calculator as the renderer.
    let available = (width as usize).saturating_sub(MESSAGE_PADDING).max(1);
    let mut code = false;
    let mut count = 0;
    for line in text.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            code = !code;
            continue;
        }
        let content = if code {
            line
        } else {
            line.trim_start_matches(['#', ' ', '-', '*', '>'])
        };
        count += if line.trim_start().starts_with("- ") {
            wrap_markdown_line(Line::plain(line.trim()), available, MAX_MARKDOWN_ROWS, true).len()
        } else {
            styled::wrap_line_limited(&Line::plain(content), available, MAX_MARKDOWN_ROWS).len()
        };
    }
    count + usize::from(text.ends_with('\n'))
}

fn table_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut code = false;
    let mut chars = line
        .trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => cell.push(chars.next().unwrap_or('\\')),
            '|' if !code => cells.push(std::mem::take(&mut cell).trim().to_string()),
            '`' => code = !code,
            _ => cell.push(c),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

/// The structural seek pass only needs to recognize a table's physical rows;
/// pulldown-cmark remains responsible for the actual blockquote/table syntax.
fn table_source(line: &str) -> (&str, usize) {
    let mut source = line.trim_start_matches(' ');
    let mut depth = 0;
    while let Some(rest) = source.strip_prefix('>') {
        source = rest.strip_prefix(' ').unwrap_or(rest);
        depth += 1;
    }
    (source, depth)
}

fn table_column_widths(rows: &[Vec<String>], available: usize) -> Vec<usize> {
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths: Vec<_> = (0..count)
        .map(|i| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(|c| styled::span_width(&Span::plain(c)))
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();
    let usable = available.saturating_sub(count * 3 + 1).max(count);
    if widths.iter().sum::<usize>() > usable {
        let natural = widths.clone();
        widths.fill(1);
        for _ in 0..usable.saturating_sub(count).min(available) {
            let col = (0..count)
                .filter(|&i| widths[i] < natural[i])
                .min_by_key(|&i| widths[i])
                .unwrap_or(count - 1);
            widths[col] += 1;
        }
    }
    if count == 2 && available >= 24 && available != usize::MAX {
        let label_extra = (available.saturating_sub(74).saturating_add(7) / 8).min(5);
        widths[0] = (widths[0] + label_extra).min(usable.saturating_sub(1));
        widths[1] = usable - widths[0];
    }
    widths
}

/// Preserve a physical table row as one semantic row. For an oversized last
/// cell, only its *content* is paged; the real row prefix/suffix and table
/// header are supplied to pulldown-cmark on every independently bounded page.
fn long_table_cell_segments(raw: &str, offset: usize, budget: usize) -> Option<LongCellSegments> {
    let (source, _) = table_source(raw);
    let before = raw.len() - source.len();
    let mut pipes = Vec::new();
    let mut escaped = false;
    let mut code = false;
    for (at, ch) in source.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '`' => code = !code,
            '|' if !code => pipes.push(at),
            _ => {}
        }
    }
    if pipes.len() < 3 {
        return None;
    }
    let cell_start = before + pipes[pipes.len() - 2] + 1;
    let cell_end = before + pipes[pipes.len() - 1];
    if cell_start >= cell_end {
        return None;
    }
    let prefix = raw[..cell_start].to_owned();
    let suffix = raw[cell_end..].to_owned();
    let mut fragments = Vec::new();
    let mut start = cell_start;
    while start < cell_end {
        let remaining = &raw[start..cell_end];
        if remaining.len() <= budget {
            fragments.push((offset + start, offset + cell_end));
            break;
        }
        let mut boundary = 0;
        let mut word = 0;
        for (at, glyph) in remaining.grapheme_indices(true) {
            if at + glyph.len() > budget {
                break;
            }
            boundary = at + glyph.len();
            if glyph == " " {
                word = boundary;
            }
        }
        let taken = if word > 0 {
            word
        } else if boundary > 0 {
            boundary
        } else {
            remaining.graphemes(true).next().map_or(0, str::len)
        };
        debug_assert!(taken > 0);
        fragments.push((offset + start, offset + start + taken));
        start += taken;
    }
    Some(LongCellSegments {
        prefix,
        suffix,
        fragments,
    })
}

fn blank_table_label(prefix: &str) -> String {
    let (source, _) = table_source(prefix);
    let mut blank = prefix[..prefix.len() - source.len()].to_owned();
    let mut escaped = false;
    let mut code = false;
    for ch in source.chars() {
        // Keep only actual column delimiters. An escaped pipe or a pipe inside
        // inline code is label content: retaining that pipe while blanking its
        // backslash/backticks would manufacture an extra GFM table column.
        if ch == '|' && !escaped && !code {
            blank.push('|');
        } else {
            blank.push_str(&" ".repeat(ch.len_utf8()));
        }
        if escaped {
            escaped = false;
        } else {
            match ch {
                '\\' => escaped = true,
                '`' => code = !code,
                _ => {}
            }
        }
    }
    blank
}

/// Line-only structural seek pass, once per part revision+width. It recognizes
/// table rows and list items as boundaries, never parses inline Markdown or
/// allocates rendered rows. A large table carries its real header/alignment to
/// each page; only its visible slice reaches pulldown-cmark.
fn index_source(text: &str, width: u16) -> Vec<SourcePage> {
    let mut lines = Vec::new();
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        // A table row's pipes/inline Markdown are one semantic source line.
        // Split only after its cell ranges have been identified below.
        if table_source(line).0.starts_with('|') {
            lines.push((start, start + line.len()));
            start += line.len();
            continue;
        }
        let mut offset = 0;
        while line.len() - offset > 4096 {
            let segment = &line[offset..];
            let mut boundary = 0;
            let mut word_boundary = 0;
            for (at, glyph) in segment.grapheme_indices(true) {
                if at + glyph.len() > 4096 {
                    break;
                }
                boundary = at + glyph.len();
                if glyph == " " {
                    word_boundary = boundary;
                }
            }
            // A single extended grapheme can exceed the byte budget (one
            // starter followed by thousands of combining marks). Keep it
            // atomic, even if that one segment is oversized: zero progress
            // would otherwise loop forever on every redraw.
            let taken = if word_boundary > 0 {
                word_boundary
            } else if boundary > 0 {
                boundary
            } else {
                segment.graphemes(true).next().map_or(0, str::len)
            };
            debug_assert!(taken > 0);
            lines.push((start + offset, start + offset + taken));
            offset += taken;
        }
        lines.push((start + offset, start + line.len()));
        start += line.len();
    }
    let mut pages = Vec::new();
    let mut i = 0;
    let mut fence: Option<String> = None;
    let mut at_line_start = true;
    let mut next_list_number: Option<u64> = None;
    while i < lines.len() {
        let (begin, _) = lines[i];
        let line = |n: usize| &text[lines[n].0..lines[n].1];
        let (header, quote_depth) = table_source(line(i));
        if fence.is_none()
            && i + 1 < lines.len()
            && header.starts_with('|')
            && table_source(line(i + 1)).1 == quote_depth
            && table_source(line(i + 1)).0.trim().starts_with('|')
            && table_source(line(i + 1)).0.contains("---")
        {
            let header_end = lines[i + 1].1;
            let mut j = i + 2;
            while j < lines.len()
                && table_source(line(j)).1 == quote_depth
                && table_source(line(j)).0.starts_with('|')
            {
                j += 1;
            }
            let table_rows: Vec<Vec<String>> = std::iter::once(table_cells(header))
                .chain((i + 2..j).map(|n| table_cells(table_source(line(n)).0)))
                .collect();
            let available = (width as usize).saturating_sub(MESSAGE_PADDING).max(1);
            let widths = table_column_widths(&table_rows, available);
            let heights: Vec<usize> = table_rows
                .iter()
                .map(|row| {
                    widths
                        .iter()
                        .enumerate()
                        .map(|(col, &w)| {
                            row.get(col)
                                .map(|c| {
                                    styled::wrap_line_limited(&Line::plain(c), w, MAX_MARKDOWN_ROWS)
                                        .len()
                                })
                                .unwrap_or(1)
                        })
                        .max()
                        .unwrap_or(1)
                })
                .collect();
            let mut cursor = i + 2;
            if cursor == j {
                cursor = j;
            }
            let mut first = true;
            while first || cursor < j {
                if cursor < j && line(cursor).len() > 4096 {
                    let budget = widths
                        .last()
                        .copied()
                        .unwrap_or(1)
                        .saturating_mul(128)
                        .clamp(128, 2048);
                    if let Some(LongCellSegments {
                        prefix,
                        suffix,
                        fragments,
                    }) = long_table_cell_segments(line(cursor), lines[cursor].0, budget)
                    {
                        let blank_prefix = blank_table_label(&prefix);
                        let fragments_len = fragments.len();
                        for (fragment_number, (start, end)) in fragments.into_iter().enumerate() {
                            let same_row = fragment_number != 0;
                            let final_page =
                                cursor + 1 == j && fragment_number + 1 == fragments_len;
                            let terminal_newline = final_page
                                && lines[cursor].1 == text.len()
                                && suffix.ends_with('\n');
                            let content = &text[start..end];
                            let cell_height = styled::wrap_line_limited(
                                &Line::plain(content),
                                *widths.last().unwrap_or(&1),
                                MAX_MARKDOWN_ROWS,
                            )
                            .len();
                            pages.push(SourcePage {
                                start,
                                end,
                                table_header: Some((begin, header_end)),
                                table_widths: Some(widths.clone()),
                                table_segment: Some(TableSegment {
                                    prefix: if same_row {
                                        blank_prefix.clone()
                                    } else {
                                        prefix.clone()
                                    },
                                    suffix: suffix.clone(),
                                    same_row,
                                    terminal_newline,
                                }),
                                continuation: !first,
                                last_table_page: final_page,
                                fence: None,
                                list_number: None,
                                height: (if first { 1 + heights[0] } else { 0 })
                                    + usize::from(!same_row)
                                    + cell_height
                                    + usize::from(final_page)
                                    + usize::from(terminal_newline),
                            });
                            first = false;
                        }
                        cursor += 1;
                        continue;
                    }
                }
                let body_start = cursor;
                let mut used = 0;
                while cursor < j
                    && cursor - body_start < 64
                    && (used + line(cursor).len() <= 4096 || cursor == body_start)
                {
                    used += line(cursor).len();
                    cursor += 1;
                }
                let final_page = cursor == j;
                let n = cursor - body_start;
                let end = if n == 0 {
                    header_end
                } else {
                    lines[cursor - 1].1
                };
                let trailing = usize::from(
                    final_page && end == text.len() && text[begin..end].ends_with('\n'),
                );
                let page_height = if first { 1 + heights[0] } else { 0 }
                    + (body_start..cursor)
                        .map(|row| 1 + heights[row - i - 1])
                        .sum::<usize>()
                    + usize::from(final_page)
                    + trailing;
                pages.push(SourcePage {
                    start: if first { begin } else { lines[body_start].0 },
                    end,
                    table_header: Some((begin, if first { begin } else { header_end })),
                    table_widths: Some(widths.clone()),
                    table_segment: None,
                    continuation: !first,
                    last_table_page: final_page,
                    fence: None,
                    list_number: None,
                    height: page_height,
                });
                first = false;
            }
            i = j;
            next_list_number = None;
            continue;
        }
        let mut j = i;
        let mut used = 0;
        let mut items = 0u64;
        let begins_list = ordered_marker(line(i)).map(|(n, _)| n);
        while j < lines.len() && (j - i) < 128 {
            let len = line(j).len();
            if used + len > 4096 && j > i {
                break;
            }
            // Break a list only between items. Keep blank lines in the page
            // where they occur, rather than synthesizing one at the boundary.
            if j > i
                && begins_list.is_some()
                && ordered_marker(line(j)).is_none()
                && !line(j).trim().is_empty()
            {
                break;
            }
            used += len;
            if ordered_marker(line(j)).is_some() {
                items += 1;
            }
            j += 1;
            if j < lines.len() && line(j - 1).trim().is_empty() && !line(j).trim().is_empty() {
                break;
            }
        }
        let end = lines[j - 1].1;
        let chunk = &text[begin..end];
        let number = begins_list.and(next_list_number).filter(|_| i > 0);
        let mut height = estimated_lines(chunk, width);
        if end < text.len() && chunk.ends_with('\n') {
            height = height.saturating_sub(1);
        }
        pages.push(SourcePage {
            start: begin,
            end,
            table_header: None,
            table_widths: None,
            table_segment: None,
            continuation: false,
            last_table_page: true,
            fence: fence.clone(),
            list_number: number,
            height,
        });
        advance_fence(chunk, &mut fence, &mut at_line_start);
        next_list_number = begins_list.map(|n| number.unwrap_or(n).saturating_add(items));
        i = j;
    }
    pages
}

/// Count bounded blocks first, then materialize only blocks intersecting the
/// viewport. The cache prevents completed Markdown from re-tokenizing.
pub(crate) fn visible_transcript(
    rows: &[HistoryRow],
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
) -> (Vec<Line>, usize) {
    visible_transcript_indexed(
        rows,
        theme,
        (width, terminal_width),
        viewport,
        &agent_color,
        cache,
        2,
    )
}

fn visible_transcript_indexed(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    retries: u8,
) -> (Vec<Line>, usize) {
    let (width, terminal_width) = widths;
    let (height, scroll, live_row) = viewport;
    let mut total = 1usize;
    for (index, row) in rows.iter().enumerate() {
        if row.role == "assistant" && width > 0 {
            visit_assistant_indexed(
                row,
                (index, live_row == Some(index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |height, _| total += height,
            );
        } else {
            visit_row_blocks(
                row,
                (index, false),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |lines| {
                    for line in lines {
                        total +=
                            styled::wrap_line_limited(&line, width as usize, MAX_MARKDOWN_ROWS)
                                .len();
                    }
                },
            );
        }
    }
    let end = total.saturating_sub(scroll.min(total.saturating_sub(height)));
    let start = end.saturating_sub(height);
    let mut visible = Vec::with_capacity(height);
    if start == 0 {
        visible.push(Line::plain(""));
    }
    let mut position = 1;
    let mut height_changed = false;
    for (index, row) in rows.iter().enumerate() {
        if position >= end {
            break;
        }
        if row.role == "assistant" && width > 0 {
            visit_assistant_indexed(
                row,
                (index, live_row == Some(index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |height, render| {
                    if position < end && position + height > start {
                        let lines = render();
                        height_changed |= lines.len() != height;
                        add_visible_lines(lines, width, (start, end), &mut position, &mut visible);
                    } else {
                        position += height;
                    }
                },
            );
        } else {
            visit_row_blocks(
                row,
                (index, false),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |lines| add_visible_lines(lines, width, (start, end), &mut position, &mut visible),
            );
        }
    }
    if height_changed && retries > 0 {
        return visible_transcript_indexed(
            rows,
            theme,
            widths,
            viewport,
            agent_color,
            cache,
            retries - 1,
        );
    }
    (visible, total)
}

fn add_visible_lines(
    lines: Vec<Line>,
    width: u16,
    viewport: (usize, usize),
    position: &mut usize,
    visible: &mut Vec<Line>,
) {
    let (start, end) = viewport;
    for line in lines {
        let wrapped = styled::wrap_line_limited(&line, width as usize, MAX_MARKDOWN_ROWS);
        let next = *position + wrapped.len();
        if *position < end && next > start {
            visible.extend(
                wrapped
                    .into_iter()
                    .skip(start.saturating_sub(*position))
                    .take(end.saturating_sub((*position).max(start))),
            );
        }
        *position = next;
    }
}

/// User message block (`routes/session/index.tsx:2298-2398`): left `┃` border
/// in the agent color, `background.raised.base`, `paddingTop/Bottom=1`,
/// `paddingLeft=2`, then chip rows with `paddingTop=1` and `gap=1`.
fn user_block(
    row: &HistoryRow,
    theme: &Theme,
    width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
) -> Vec<Line> {
    let bg = theme.user_message_background();
    let border = Style::default()
        .fg(user_agent_color(row, theme, agent_color))
        .bg(bg);
    let body = Style::default().fg(theme.text()).bg(bg);
    let width = width as usize;
    let mut out = Vec::new();
    // paddingTop=1: one bordered, background-filled empty row.
    out.push(user_row(&[Span::styled("┃", border)], bg, width));
    let inner = if width == 0 {
        usize::MAX
    } else {
        width.saturating_sub(1 + USER_PADDING)
    };
    if !row.text.is_empty() {
        for raw in row.text.split('\n') {
            let line = Line::new(vec![Span::styled(raw, body)]);
            for wrapped in styled::wrap_line(&line, inner.max(1)) {
                let mut spans = vec![
                    Span::styled("┃", border),
                    Span::styled(" ".repeat(USER_PADDING), body),
                ];
                spans.extend(wrapped.spans().iter().cloned());
                out.push(user_row(&spans, bg, width));
            }
        }
    }
    if !row.chips.is_empty() {
        // Chips row: `paddingTop={1}`, `gap={1}`, `flexWrap="wrap"`.
        out.push(user_row(&[Span::styled("┃", border)], bg, width));
        for chips in chip_rows(&row.chips, theme, inner) {
            let mut spans = vec![
                Span::styled("┃", border),
                Span::styled(" ".repeat(USER_PADDING), body),
            ];
            spans.extend(chips);
            out.push(user_row(&spans, bg, width));
        }
    }
    // paddingBottom=1.
    out.push(user_row(&[Span::styled("┃", border)], bg, width));
    out
}

fn user_agent_color(
    row: &HistoryRow,
    theme: &Theme,
    agent_color: &impl Fn(Option<&str>) -> Color,
) -> Color {
    row.agent_color_index.map_or_else(
        || agent_color(row.agent.as_deref()),
        |index| {
            let colors = theme.categorical_agents();
            colors[index % colors.len()]
        },
    )
}

/// Pad one user-block row to the full content width so the raised background
/// covers the whole row (upstream box background stretches to its parent).
fn user_row(spans: &[Span], bg: Color, width: usize) -> Line {
    let used: usize = spans.iter().map(styled::span_width).sum();
    let mut spans = spans.to_vec();
    if used < width {
        spans.push(Span::styled(
            " ".repeat(width - used),
            Style::default().bg(bg),
        ));
    }
    Line::new(spans).with_style(Style::default().bg(bg))
}

/// One chip: ` skill ` on `hue.accent[light ? 300 : 200]` with
/// `background.raised.base` foreground and bold, then ` name ` on
/// `decrease(background.raised.base)` in `text.muted`
/// (`routes/session/index.tsx:2352-2363`).
fn chip_spans(chip: &Chip, theme: &Theme) -> Vec<Span> {
    let label_bg = theme.accent_chip_background();
    let label_fg = theme.user_message_background();
    let name_bg = theme.decrease(theme.user_message_background());
    vec![
        Span::styled(
            format!(" {} ", chip.kind.label()),
            Style::default()
                .fg(label_fg)
                .bg(label_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {} ", chip.name),
            Style::default().fg(theme.text_muted()).bg(name_bg),
        ),
    ]
}

/// Pack chips into rows of at most `width` cells with a one-cell gap,
/// upstream `flexWrap="wrap"` + `gap={1}` (`routes/session/index.tsx:2348`).
fn chip_rows(chips: &[Chip], theme: &Theme, width: usize) -> Vec<Vec<Span>> {
    let mut rows = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let mut used = 0usize;
    for chip in chips {
        let spans = chip_spans(chip, theme);
        let chip_width: usize = spans.iter().map(styled::span_width).sum();
        let gap = usize::from(!current.is_empty());
        if used + gap + chip_width > width && !current.is_empty() {
            rows.push(std::mem::take(&mut current));
            used = 0;
        }
        if !current.is_empty() {
            current.push(Span::plain(" "));
            used += 1;
        }
        current.extend(spans);
        used += chip_width;
    }
    if !current.is_empty() {
        rows.push(current);
    }
    rows
}

/// Assistant rows: each upstream row (`reasoning`, `part`, `assistant-footer`)
/// has `marginTop=1` (`routes/session/index.tsx:1435`), so every block is
/// preceded by one empty row.
fn assistant_block(
    row: &HistoryRow,
    index: usize,
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let mut out = Vec::new();
    if let Some(reasoning) = &row.reasoning {
        out.extend(reasoning_lines(reasoning, theme, width));
    }
    if !row.text.trim().is_empty() {
        out.push(Line::plain(""));
        out.extend(match cache {
            Some(cache) => cache
                .borrow_mut()
                .render((row.seq, index, 0), &row.text, theme, width),
            None => markdown_block(&row.text, theme, width),
        });
    }
    if let Some(meta) = &row.meta
        && let Some(footer) = footer_line(
            row.agent.as_deref(),
            meta,
            theme,
            terminal_width,
            agent_color,
        )
    {
        out.push(Line::plain(""));
        out.push(footer);
    }
    out
}

/// Collapsed reasoning header (`routes/session/index.tsx:1765-1815`):
///
/// - running: static spinner fallback `⋯ Thinking` / `⋯ Thinking: <title>`
///   (`component/spinner.tsx:29-33`, animations off) in `text.base`
///   (`:1789-1791`);
/// - completed: `+ Thought: <title> · <duration>` in the warning color at
///   alpha 0.6 (`:1793-1800`, `message-parts.tsx:106-114`).
fn reasoning_line(reasoning: &ReasoningBlock, theme: &Theme) -> Line {
    let content = reasoning.text.replace("[REDACTED]", "");
    let title = if reasoning.expanded {
        ""
    } else {
        reasoning_title(&content)
    };
    let mut spans = vec![Span::plain(" ".repeat(MESSAGE_PADDING))];
    if reasoning.running {
        let mut text = String::from("⋯ Thinking");
        if !title.is_empty() {
            text.push_str(": ");
            text.push_str(title);
        }
        spans.push(Span::styled(text, Style::default().fg(theme.text())));
        return Line::new(spans);
    }
    let faded = theme.fade(theme.warning(), 0.6);
    let style = Style::default().fg(faded);
    spans.push(Span::styled(
        format!(
            "{:<width$}",
            if reasoning.expanded { "-" } else { "+" },
            width = INLINE_ICON_WIDTH
        ),
        style,
    ));
    let mut text = String::from("Thought");
    let duration = reasoning
        .duration_ms
        .filter(|ms| *ms > 0)
        .map(Locale::duration);
    if !title.is_empty() || duration.is_some() {
        text.push_str(": ");
    }
    if !title.is_empty() {
        text.push_str(title);
        if duration.is_some() {
            text.push_str(" · ");
        }
    }
    if let Some(duration) = duration {
        text.push_str(&duration);
    }
    spans.push(Span::styled(text, style));
    Line::new(spans)
}

fn reasoning_lines(reasoning: &ReasoningBlock, theme: &Theme, width: u16) -> Vec<Line> {
    let mut lines = vec![
        Line::plain(""),
        sanitize_line(reasoning_line(reasoning, theme)),
    ];
    if reasoning.expanded {
        let content = reasoning.text.replace("[REDACTED]", "");
        if !content.trim().is_empty() {
            lines.push(Line::plain(""));
            let mut end = content.len().min(LIVE_MARKDOWN_BYTES);
            while !content.is_char_boundary(end) {
                end -= 1;
            }
            let mut body = markdown_block(&content[..end], theme, width);
            // A single public reasoning part is a bounded view of the owner's
            // durable text. Do not turn expansion into an unbounded render.
            let omitted = end < content.len() || body.len() > MAX_MARKDOWN_ROWS;
            if body.len() > MAX_MARKDOWN_ROWS {
                body.truncate(MAX_MARKDOWN_ROWS);
            }
            if omitted {
                body.push(Line::styled(
                    "   … [reasoning preview limited]",
                    Style::default().fg(theme.text_muted()),
                ));
            }
            lines.extend(body.into_iter().map(sanitize_line));
        }
    }
    lines
}

/// Upstream `reasoningSummary` (`context/thinking.ts:10-19`): a leading
/// `**Title**` block (followed by a blank line or the end of the text) is the
/// reasoning title; the remainder is the body. Returns `""` for no title.
pub fn reasoning_title(text: &str) -> &str {
    let content = text.trim();
    let Some(rest) = content.strip_prefix("**") else {
        return "";
    };
    let Some(end) = rest.find("**") else {
        return "";
    };
    let title = &rest[..end];
    if title.is_empty() || title.contains('\n') || title.contains('*') {
        return "";
    }
    let after = &rest[end + 2..];
    if after.is_empty() || after.starts_with("\n\n") || after.starts_with("\r\n\r\n") {
        title.trim()
    } else {
        ""
    }
}

/// Assistant text block (`message-parts.tsx:147-174`): markdown rendered at
/// `paddingLeft=3` with `fg = markdown.text`.
fn markdown_block(text: &str, theme: &Theme, width: u16) -> Vec<Line> {
    markdown_block_with_widths(text, theme, width, None)
}

fn markdown_block_with_widths(
    text: &str,
    theme: &Theme,
    width: u16,
    columns: Option<&[usize]>,
) -> Vec<Line> {
    let inner = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(MESSAGE_PADDING).max(1)
    };
    let pad = Span::plain(" ".repeat(MESSAGE_PADDING));
    let mut out = Vec::new();
    for line in markdown_at_width_with_columns(text, theme, inner, columns) {
        let mut spans = vec![pad.clone()];
        spans.extend(line.spans().iter().cloned());
        out.push(Line::new(spans));
    }
    out
}

/// Non-message rows (port notices such as `(cancelled)`/`(error: …)` for
/// turns without a rendered assistant block) keep the historical plain text.
fn notice_block(row: &HistoryRow) -> Vec<Line> {
    if row.role.is_empty() {
        vec![Line::plain(row.text.clone())]
    } else {
        vec![Line::plain(format!("{}: {}", row.role, row.text))]
    }
}

/// Assistant footer (`routes/session/index.tsx:1934-1985`): titlecased agent
/// in the agent color (muted on error/interrupt), then ` · <model>`,
/// ` · <duration>`, ` · <N> tok/s` and ` · interrupted`, all in `text.muted`.
/// Fields with no data are omitted, never faked.
fn footer_line(
    agent: Option<&str>,
    meta: &AssistantMeta,
    theme: &Theme,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
) -> Option<Line> {
    let muted = Style::default().fg(theme.text_muted());
    let mut spans: Vec<Span> = vec![Span::plain(" ".repeat(MESSAGE_PADDING))];
    let push_field = |span: Span, spans: &mut Vec<Span>| {
        if spans.len() > 1 {
            spans.push(Span::styled(" · ", muted));
        }
        spans.push(span);
    };
    if let Some(agent) = agent {
        let color = if meta.interrupted {
            theme.text_muted()
        } else {
            meta.agent_color_index
                .map(|index| {
                    let colors = theme.categorical_agents();
                    colors[index % colors.len()]
                })
                .unwrap_or_else(|| agent_color(Some(agent)))
        };
        push_field(
            Span::styled(Locale::titlecase(agent), Style::default().fg(color)),
            &mut spans,
        );
    }
    if terminal_width >= FOOTER_MODEL_MIN_WIDTH
        && let Some(model) = &meta.model
    {
        push_field(Span::styled(model.clone(), muted), &mut spans);
    }
    if let Some(duration) = meta.duration_ms.filter(|ms| *ms > 0)
        // Upstream hides the duration in the 28..36 column band
        // (`routes/session/index.tsx:1967-1969`).
        && !(FOOTER_MODEL_MIN_WIDTH..FOOTER_DURATION_MIN_WIDTH).contains(&terminal_width)
    {
        push_field(Span::styled(Locale::duration(duration), muted), &mut spans);
    }
    if let Some(tps) = tokens_per_second(meta) {
        push_field(Span::styled(format!("{tps:.1} tok/s"), muted), &mut spans);
    }
    if let Some(status) = meta
        .status
        .as_deref()
        .filter(|s| *s != "completed" && *s != "started")
    {
        push_field(Span::styled(status, muted), &mut spans);
    } else if meta.interrupted {
        push_field(Span::styled("interrupted", muted), &mut spans);
    }
    (spans.len() > 1).then(|| Line::new(spans))
}

/// Upstream `turnTokensPerSecond` (`routes/session/rows.ts:365-386`):
/// `(output + reasoning) tokens / provider-active seconds`, aggregated before
/// dividing. `None` when either side is missing (never `0 tok/s`).
fn tokens_per_second(meta: &AssistantMeta) -> Option<f64> {
    let output = meta.output_tokens? as f64;
    let streamed_ms = meta.streamed_ms?;
    if output <= 0.0 || streamed_ms == 0 {
        return None;
    }
    Some(output / (streamed_ms as f64 / 1_000.0))
}

/// CommonMark + GFM tables, with upstream's concealed markers and grid tables
/// (`message-parts.tsx:159-170`). The public unbounded projection is for
/// inspection; the screen uses `markdown_at_width` with a measured cell width.
pub fn markdown(text: &str, theme: &Theme) -> Vec<Line> {
    markdown_at_width(text, theme, usize::MAX)
}

/// Model text must never emit raw terminal control sequences, even inside a
/// code/table event. Keep printable Unicode and make controls visible as U+FFFD.
fn safe_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '�' } else { c })
        .collect()
}

fn wrap_markdown_line(line: Line, width: usize, limit: usize, list_item: bool) -> Vec<Line> {
    let wrapped = styled::wrap_line_limited(&line, width, limit);
    if !list_item || wrapped.len() <= 1 {
        return wrapped;
    }
    let mut rows = Vec::with_capacity(wrapped.len());
    for (index, line) in wrapped.into_iter().enumerate() {
        if index == 0 {
            rows.push(line);
        } else {
            for continuation in styled::wrap_line_limited(
                &line,
                width.saturating_sub(2),
                limit.saturating_sub(rows.len()),
            ) {
                let mut spans = vec![Span::plain("  ")];
                spans.extend(continuation.spans().iter().cloned());
                rows.push(Line::new(spans));
            }
        }
    }
    rows
}

#[derive(Default)]
struct TableDraft {
    rows: Vec<Vec<Line>>,
    row: Vec<Line>,
    header: bool,
    omitted: bool,
}

fn markdown_at_width(text: &str, theme: &Theme, width: usize) -> Vec<Line> {
    markdown_at_width_with_columns(text, theme, width, None)
}

fn markdown_at_width_with_columns(
    text: &str,
    theme: &Theme,
    width: usize,
    columns: Option<&[usize]>,
) -> Vec<Line> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    let mut out = Vec::new();
    let mut spans = Vec::new();
    let mut stack = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut code: Option<(String, String)> = None;
    let mut table: Option<TableDraft> = None;
    let mut previous_end = 0;
    let mut depth = 0usize;
    let mut truncated = false;
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        if out.len() >= MAX_MARKDOWN_ROWS {
            truncated = true;
            break;
        }
        if depth == 0 && !matches!(event, Event::End(_)) {
            let gap = &text[previous_end..range.start];
            out.extend(
                std::iter::repeat_with(|| Line::plain("")).take(
                    gap.bytes()
                        .filter(|c| *c == b'\n')
                        .count()
                        .saturating_sub(usize::from(
                            previous_end == 0 || !text[..previous_end].ends_with('\n'),
                        ))
                        .min((MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len())),
                ),
            );
        }
        match event {
            Event::Start(tag) => {
                depth += 1;
                match &tag {
                    Tag::Table(_) => table = Some(TableDraft::default()),
                    Tag::TableHead => {
                        if let Some(t) = &mut table {
                            t.header = true;
                        }
                    }
                    Tag::TableRow => {
                        if let Some(t) = &mut table {
                            t.row.clear();
                        }
                    }
                    Tag::TableCell => spans.clear(),
                    Tag::CodeBlock(kind) => {
                        let lang = match kind {
                            CodeBlockKind::Fenced(info) => {
                                info.split_whitespace().next().unwrap_or("").to_string()
                            }
                            CodeBlockKind::Indented => String::new(),
                        };
                        code = Some((lang, String::new()));
                    }
                    Tag::List(start) => lists.push(*start),
                    Tag::Item => {
                        let ordered = lists.last_mut().and_then(Option::as_mut);
                        let (marker, token) = match ordered {
                            Some(n) => {
                                let marker = format!("{n}. ");
                                *n += 1;
                                (marker, MarkdownToken::ListEnumeration)
                            }
                            None => ("- ".into(), MarkdownToken::ListItem),
                        };
                        spans.push(Span::styled(
                            marker,
                            Style::default().fg(theme.markdown(token)),
                        ));
                    }
                    Tag::Heading { .. } => stack.push(MarkdownToken::Heading),
                    Tag::BlockQuote(_) => stack.push(MarkdownToken::BlockQuote),
                    Tag::Strong => stack.push(MarkdownToken::Strong),
                    Tag::Emphasis => stack.push(MarkdownToken::Emphasis),
                    Tag::Link { .. } => stack.push(MarkdownToken::LinkText),
                    Tag::Image { .. } => stack.push(MarkdownToken::ImageText),
                    _ => {}
                }
            }
            Event::End(end) => {
                match end {
                    TagEnd::TableCell => {
                        if let Some(t) = &mut table {
                            t.row.push(Line::new(std::mem::take(&mut spans)));
                        }
                    }
                    TagEnd::TableHead | TagEnd::TableRow => {
                        if let Some(t) = &mut table {
                            if !t.row.is_empty() {
                                if t.rows.len() < MAX_MARKDOWN_ROWS {
                                    t.rows.push(std::mem::take(&mut t.row));
                                } else {
                                    t.omitted = true;
                                    t.row.clear();
                                }
                            }
                            if end == TagEnd::TableHead {
                                t.header = false;
                            }
                        }
                    }
                    TagEnd::Table => {
                        if let Some(mut t) = table.take() {
                            if !t.row.is_empty() {
                                t.rows.push(t.row);
                            }
                            out.extend(
                                render_table(&t.rows, theme, width, columns)
                                    .into_iter()
                                    .take((MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len())),
                            );
                            if t.omitted {
                                out.push(Line::plain("… [Markdown table preview limited]"));
                            }
                        }
                    }
                    TagEnd::CodeBlock => {
                        if let Some((lang, body)) = code.take() {
                            // Only the structural closing newline is removed;
                            // blank lines immediately above the fence are data.
                            let body = body.strip_suffix('\n').unwrap_or(&body);
                            if !body.is_empty() {
                                for line in body.split('\n') {
                                    if out.len() >= MAX_MARKDOWN_ROWS {
                                        truncated = true;
                                        break;
                                    }
                                    let highlighted =
                                        code_lines(Some(&lang), &[safe_text(line)], theme);
                                    out.extend(styled::wrap_line_limited(
                                        &highlighted[0],
                                        width,
                                        (MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len()),
                                    ));
                                }
                            }
                        }
                    }
                    TagEnd::List(_) => {
                        lists.pop();
                    }
                    TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item => {
                        if !spans.is_empty() {
                            out.extend(wrap_markdown_line(
                                Line::new(std::mem::take(&mut spans)),
                                width,
                                (MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len()),
                                !lists.is_empty(),
                            ));
                        }
                        if matches!(end, TagEnd::Heading(_)) {
                            stack.pop();
                        }
                    }
                    TagEnd::Strong
                    | TagEnd::Emphasis
                    | TagEnd::Link
                    | TagEnd::Image
                    | TagEnd::BlockQuote(_) => {
                        stack.pop();
                    }
                    _ => {}
                }
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let trailing = text[range.clone()]
                        .bytes()
                        .rev()
                        .take_while(|c| *c == b'\n')
                        .count();
                    out.extend(
                        std::iter::repeat_with(|| Line::plain("")).take(
                            trailing
                                .saturating_sub(1)
                                .min((MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len())),
                        ),
                    );
                    previous_end = range.end;
                }
            }
            Event::Text(value) => {
                if let Some((_, body)) = &mut code {
                    body.push_str(&value);
                } else {
                    let token = if table.as_ref().is_some_and(|t| t.header) {
                        MarkdownToken::Heading
                    } else {
                        stack.last().copied().unwrap_or(MarkdownToken::Text)
                    };
                    spans.push(Span::styled(
                        safe_text(&value),
                        Style::default().fg(theme.markdown(token)),
                    ));
                }
            }
            Event::Code(value) => spans.push(Span::styled(
                safe_text(&value),
                Style::default().fg(theme.markdown(MarkdownToken::Code)),
            )),
            Event::SoftBreak | Event::HardBreak => {
                if !spans.is_empty() {
                    out.extend(wrap_markdown_line(
                        Line::new(std::mem::take(&mut spans)),
                        width,
                        (MAX_MARKDOWN_ROWS + 1).saturating_sub(out.len()),
                        !lists.is_empty(),
                    ));
                }
            }
            Event::Rule => out.push(Line::styled(
                "───",
                Style::default().fg(theme.markdown(MarkdownToken::HorizontalRule)),
            )),
            Event::Html(value) | Event::InlineHtml(value) => spans.push(Span::styled(
                safe_text(&value),
                Style::default().fg(theme.markdown(MarkdownToken::Text)),
            )),
            _ => {}
        }
    }
    if out.len() > MAX_MARKDOWN_ROWS {
        truncated = true;
    }
    if truncated {
        out.truncate(MAX_MARKDOWN_ROWS - 1);
        out.push(Line::styled(
            "… [Markdown preview limited]",
            Style::default().fg(theme.text_muted()),
        ));
    } else if text.ends_with('\n') {
        out.push(Line::plain(""));
    }
    if out.is_empty() {
        out.push(Line::plain(""));
    }
    out
}

/// Grid style + one-cell horizontal padding from upstream TextPart's
/// `tableOptions={{style:"grid",cellPaddingX:1}}`. Widths use terminal cells.
fn render_table(
    rows: &[Vec<Line>],
    theme: &Theme,
    available: usize,
    columns: Option<&[usize]>,
) -> Vec<Line> {
    let count = rows.iter().map(Vec::len).max().unwrap_or(0);
    if count == 0 {
        return Vec::new();
    }
    if available < count * 4 + 1 {
        return vec![Line::plain("…")];
    }
    // OpenTUI's grid uses neutral #888 borders and #fff padding under the
    // pinned dark profile (paired v2.0.12 styled cells, recovery-v06a).
    // Cell content keeps Markdown token colors, including inline code.
    let border = Style::default().fg(if theme.mode() == ThemeMode::Dark {
        Color::Rgb(136, 136, 136)
    } else {
        theme.border()
    });
    let padding = Style::default().fg(theme.hue("neutral", 100).unwrap_or_else(|| theme.text()));
    let mut widths = (0..count)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.spans().iter().map(styled::span_width).sum::<usize>())
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect::<Vec<_>>();
    let usable = available.saturating_sub(count * 3 + 1).max(count);
    if widths.iter().sum::<usize>() > usable {
        widths.fill(1);
        let extra = usable.saturating_sub(count);
        // Give the shorter label column room before assigning the remaining
        // cells to the long description; never allocate beyond the viewport.
        for _ in 0..extra.min(available) {
            let col = (0..count)
                .filter(|&i| {
                    widths[i]
                        < rows
                            .iter()
                            .filter_map(|row| row.get(i))
                            .map(|cell| cell.spans().iter().map(styled::span_width).sum())
                            .max()
                            .unwrap_or(1)
                })
                .min_by_key(|&i| widths[i])
                .unwrap_or(count - 1);
            widths[col] += 1;
        }
    }
    if count == 2 && available >= 24 && available != usize::MAX {
        // Captured upstream at 120x40: label column reserves five cells past
        // its longest word; the description column fills the content box.
        let label_extra = (available.saturating_sub(74).saturating_add(7) / 8).min(5);
        widths[0] = (widths[0] + label_extra).min(usable.saturating_sub(1));
        widths[1] = usable - widths[0];
    }
    if let Some(columns) = columns.filter(|c| c.len() == count && c.iter().sum::<usize>() <= usable)
    {
        widths.copy_from_slice(columns);
    }
    let separator = |left: &str, middle: &str, right: &str| {
        let mut parts = vec![Span::styled(left, border)];
        for (i, width) in widths.iter().enumerate() {
            if i > 0 {
                parts.push(Span::styled(middle, border));
            }
            parts.push(Span::styled("─".repeat(width + 2), border));
        }
        parts.push(Span::styled(right, border));
        Line::new(parts)
    };
    let mut out = vec![separator("┌", "┬", "┐")];
    for (index, row) in rows.iter().enumerate().take(MAX_MARKDOWN_ROWS) {
        if out.len() >= MAX_MARKDOWN_ROWS {
            break;
        }
        let wrapped: Vec<Vec<Line>> = widths
            .iter()
            .enumerate()
            .map(|(i, &width)| {
                row.get(i)
                    .map(|cell| {
                        styled::wrap_line_limited(cell, width, MAX_MARKDOWN_ROWS - out.len())
                    })
                    .unwrap_or_else(|| vec![Line::plain("")])
            })
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        for y in 0..height {
            if out.len() >= MAX_MARKDOWN_ROWS {
                break;
            }
            let mut parts = vec![Span::styled("│", border)];
            for (i, cell) in wrapped.iter().enumerate() {
                parts.push(Span::styled(" ", padding));
                if let Some(line) = cell.get(y) {
                    parts.extend(line.spans().iter().map(|span| {
                        if index == 0 {
                            Span::styled(span.content(), span.style().add_modifier(Modifier::BOLD))
                        } else {
                            span.clone()
                        }
                    }));
                }
                let mut used = cell.get(y).map_or(0, |line| {
                    line.spans().iter().map(styled::span_width).sum::<usize>()
                });
                // Word-wrap consumes the separating space. OpenTUI retains
                // that cell in the source token color before grid padding.
                if used < widths[i]
                    && let (Some(source), Some(current), Some(next)) =
                        (row.get(i), cell.get(y), cell.get(y + 1))
                    && let (Some(last), Some(first)) = (
                        current
                            .plain_text()
                            .split_whitespace()
                            .last()
                            .map(str::to_owned),
                        next.plain_text()
                            .split_whitespace()
                            .next()
                            .map(str::to_owned),
                    )
                    && source.plain_text().contains(&format!("{last} {first}"))
                {
                    let color = current.spans().last().map_or(Style::default(), Span::style);
                    parts.push(Span::styled(" ", color));
                    used += 1;
                }
                parts.push(Span::styled(
                    " ".repeat(widths[i].saturating_sub(used) + 1),
                    padding,
                ));
                parts.push(Span::styled("│", border));
            }
            out.push(Line::new(parts));
        }
        if index + 1 < rows.len() && out.len() < MAX_MARKDOWN_ROWS {
            out.push(separator("├", "┼", "┤"));
        }
    }
    out.push(separator("└", "┴", "┘"));
    out
}

fn flush(spans: &mut Vec<Span>, plain: &mut String, style: Style) {
    if !plain.is_empty() {
        spans.push(Span::styled(std::mem::take(plain), style));
    }
}

/// Language for the hand-rolled code-block highlighter. Upstream highlights
/// through tree-sitter WASM (`parsers-config.ts`, ~40 languages); this port
/// supports a small documented subset and renders every other language in
/// `markdown.codeBlock` color without pretending to know its grammar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Language {
    Rust,
    Python,
    Shell,
    Json,
    Plain,
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn", "else", "enum",
    "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
    "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true",
    "type", "unsafe", "use", "where", "while",
];
const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "False", "finally", "for", "from", "global", "if", "import", "in", "is",
    "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "self", "True", "try",
    "while", "with", "yield",
];
const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in",
    "local", "readonly", "return", "then", "unset", "while",
];
const JSON_KEYWORDS: &[&str] = &["false", "null", "true"];

impl Language {
    fn from_info(info: Option<&str>) -> Self {
        match info.unwrap_or("").to_ascii_lowercase().as_str() {
            "rust" | "rs" => Language::Rust,
            "python" | "py" => Language::Python,
            "bash" | "sh" | "shell" | "zsh" => Language::Shell,
            "json" | "jsonc" => Language::Json,
            _ => Language::Plain,
        }
    }

    /// Line-comment prefix, when the language has one.
    fn comment(self) -> Option<&'static str> {
        match self {
            Language::Rust => Some("//"),
            Language::Python | Language::Shell => Some("#"),
            Language::Json | Language::Plain => None,
        }
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Language::Rust => RUST_KEYWORDS,
            Language::Python => PYTHON_KEYWORDS,
            Language::Shell => SHELL_KEYWORDS,
            Language::Json => JSON_KEYWORDS,
            Language::Plain => &[],
        }
    }

    fn single_quoted_strings(self) -> bool {
        matches!(self, Language::Shell)
    }
}

/// Code block: base `markdown.codeBlock` plus the subset syntax tokens
/// (`packages/theme/src/tui/syntax.ts:10-85`).
fn code_lines(lang: Option<&str>, body: &[String], theme: &Theme) -> Vec<Line> {
    let language = Language::from_info(lang);
    let base = Style::default().fg(theme.markdown(MarkdownToken::CodeBlock));
    body.iter()
        .map(|line| Line::new(highlight(line, language, theme, base)))
        .collect()
}

/// Tokenize one code line: comments, strings, numbers, keywords, function
/// calls and type-like identifiers. Everything else keeps the code-block
/// color; unknown languages produce one base-styled span.
fn highlight(line: &str, language: Language, theme: &Theme, base: Style) -> Vec<Span> {
    if language == Language::Plain {
        return vec![Span::styled(line, base)];
    }
    let style = |token: SyntaxToken| Style::default().fg(theme.syntax(token));
    let mut spans: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let mut index = 0;
    let chars: Vec<char> = line.chars().collect();
    while index < chars.len() {
        let ch = chars[index];
        if let Some(prefix) = language.comment()
            && line[char_offset(line, index)..].starts_with(prefix)
        {
            flush(&mut spans, &mut plain, base);
            spans.push(Span::styled(
                chars[index..].iter().collect::<String>(),
                style(SyntaxToken::Comment),
            ));
            return spans;
        }
        if ch == '"' || (ch == '\'' && language.single_quoted_strings()) {
            flush(&mut spans, &mut plain, base);
            let mut end = index + 1;
            while end < chars.len() && chars[end] != ch {
                if chars[end] == '\\' {
                    end += 1;
                }
                end += 1;
            }
            let end = end.min(chars.len());
            spans.push(Span::styled(
                chars[index..end].iter().collect::<String>(),
                style(SyntaxToken::String),
            ));
            index = (end + 1).min(chars.len());
            continue;
        }
        if ch.is_ascii_digit() {
            flush(&mut spans, &mut plain, base);
            let mut end = index + 1;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '.' || chars[end] == '_')
            {
                end += 1;
            }
            spans.push(Span::styled(
                chars[index..end].iter().collect::<String>(),
                style(SyntaxToken::Number),
            ));
            index = end;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let mut end = index;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let ident: String = chars[index..end].iter().collect();
            let token = if language.keywords().contains(&ident.as_str()) {
                Some(SyntaxToken::Keyword)
            } else if chars.get(end) == Some(&'(') {
                Some(SyntaxToken::Function)
            } else if ident.chars().next().is_some_and(char::is_uppercase) {
                Some(SyntaxToken::Type)
            } else {
                None
            };
            match token {
                Some(token) => {
                    flush(&mut spans, &mut plain, base);
                    spans.push(Span::styled(ident, style(token)));
                }
                None => plain.push_str(&ident),
            }
            index = end;
            continue;
        }
        plain.push(ch);
        index += 1;
    }
    flush(&mut spans, &mut plain, base);
    spans
}

/// Byte offset of the `index`-th character (ASCII fast path for `starts_with`).
fn char_offset(line: &str, index: usize) -> usize {
    if line.is_ascii() {
        index.min(line.len())
    } else {
        line.char_indices()
            .nth(index)
            .map(|(offset, _)| offset)
            .unwrap_or(line.len())
    }
}

/// Locale helpers (`util/locale.ts:3-5,35-57`), exact upstream formatting.
pub(crate) struct Locale;

impl Locale {
    /// `titlecase`: uppercase the first letter of every word.
    pub fn titlecase(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut boundary = true;
        for ch in input.chars() {
            if boundary && ch.is_ascii_alphanumeric() {
                out.extend(ch.to_uppercase());
            } else {
                out.push(ch);
            }
            boundary = !(ch.is_ascii_alphanumeric() || ch == '_');
        }
        out
    }

    /// `duration`: `Nms` / `N.Ns` / `Nm Ns` / `Nh Nm` / `Nd Nh`.
    pub fn duration(ms: u64) -> String {
        if ms < 1_000 {
            return format!("{ms}ms");
        }
        if ms < 60_000 {
            return format!("{:.1}s", ms as f64 / 1_000.0);
        }
        if ms < 3_600_000 {
            let minutes = ms / 60_000;
            let seconds = (ms % 60_000) / 1_000;
            return format!("{minutes}m {seconds}s");
        }
        if ms < 86_400_000 {
            let hours = ms / 3_600_000;
            let minutes = (ms % 3_600_000) / 60_000;
            return format!("{hours}h {minutes}m");
        }
        let days = ms / 86_400_000;
        let hours = (ms % 86_400_000) / 3_600_000;
        format!("{days}d {hours}h")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::styled;
    use ratatui::buffer::Buffer;
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    fn user(text: &str, chips: Vec<Chip>) -> HistoryRow {
        HistoryRow {
            seq: 1,
            role: "user".to_string(),
            text: text.to_string(),
            agent: Some("build".to_string()),
            agent_color_index: None,
            chips,
            reasoning: None,
            meta: None,
            tool: None,
        }
    }

    fn assistant(text: &str) -> HistoryRow {
        HistoryRow {
            seq: 2,
            role: "assistant".to_string(),
            text: text.to_string(),
            agent: Some("build".to_string()),
            agent_color_index: None,
            chips: Vec::new(),
            reasoning: None,
            meta: None,
            tool: None,
        }
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

    /// Collapsed reasoning (`routes/session/index.tsx:1765-1815`): a static
    /// spinner header while running, `+ Thought: <title> · <duration>` once
    /// complete, in the warning color at alpha 0.6.
    #[test]
    fn golden_reasoning_running_and_completed() {
        let theme = Theme::dark();
        let fading = theme.fade(theme.warning(), 0.6);
        let running = HistoryRow {
            reasoning: Some(ReasoningBlock {
                text: "**Inspecting**\n\nbody".to_string(),
                duration_ms: None,
                running: true,
                expanded: false,
            }),
            ..assistant("")
        };
        let (rows, buffer) = render(&[running], 60, 3);
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

        let completed = HistoryRow {
            reasoning: Some(ReasoningBlock {
                text: "**Inspecting**\n\nbody".to_string(),
                duration_ms: Some(1500),
                running: false,
                expanded: false,
            }),
            ..assistant("")
        };
        let (rows, buffer) = render(&[completed], 60, 3);
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
        assert_eq!(buffer[(5, 1)].fg, fading);
        // Unknown duration renders `Thought` without an invented `0ms`.
        let no_duration = HistoryRow {
            reasoning: Some(ReasoningBlock {
                text: "no title".to_string(),
                duration_ms: None,
                running: false,
                expanded: false,
            }),
            ..assistant("")
        };
        let (rows, _) = render(&[no_duration], 60, 2);
        assert_eq!(rows, vec![String::new(), "   + Thought".to_string()]);
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
    fn v02_footer_uses_pinned_agent_slot_and_exact_status() {
        for status in ["failed", "cancelled", "incomplete", "unknown"] {
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
                rows.iter()
                    .any(|r| r.contains(&format!("Build · {status}"))),
                "{rows:?}"
            );
            assert_eq!(
                buffer[(3, 1)].fg,
                Theme::dark().categorical_agents()[3],
                "historical generation slot wins over render helper's current slot zero"
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
    fn v06a_table_fixture_escaped_pipes_unicode_and_bounded_wrap() {
        let fixture = include_str!("../../../tui-recovery/fixtures/transcript.md");
        let theme = Theme::dark();
        let table = &fixture
            [fixture.find("| Инструмент").unwrap()..fixture.find("Через Code Mode").unwrap()];
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
                    .all(|r| r.spans().iter().map(styled::span_width).sum::<usize>()
                        <= width as usize)
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
                .any(|r| r.plain_text().contains("│ Инструмент      │"))
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
            op: "op".into(),
            rowid: 1,
            name: "bash".into(),
            state: "completed".into(),
            input: Some(serde_json::json!({"command": "\u{1b}]52;;AAA\u{7}"}).to_string()),
            output: Some("\u{1b}[2Jtool output\u{7}".into()),
            output_bytes: 20,
            output_truncated: false,
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
                top.iter().any(
                    |line| line.plain_text().contains("Key") && line.plain_text().contains('│')
                ),
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
                top.iter().any(
                    |line| line.plain_text().contains("Key") && line.plain_text().contains('│')
                ),
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
                                .all(|span| span.style().fg
                                    == Some(theme.markdown(MarkdownToken::Text)))
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
                .any(|line| line.plain_text().contains("value-149")
                    && line.plain_text().contains('│'))
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
        let text = include_str!("../../../tui-recovery/fixtures/transcript.md");
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
}
