//! Upstream v2.0.12 message rendering: user blocks with chips, assistant
//! markdown, collapsed reasoning and the assistant footer.
//!
//! Geometry and colors cite the upstream sources at tag `v2.0.12`
//! (`packages/tui/src/**`). Markdown structure uses pulldown-cmark events;
//! syntax tokens still cover a documented subset of upstream grammars.
//! Renderer regressions and transcript fixtures live in `messages/tests.rs`.
//!
//! Renderers:
//!
//! - user message: left `┃` border in the agent color, raised background,
//!   `padding 1/2`, then the skill/file chip rows
//!   (`routes/session/index.tsx:2273-2398`);
//! - assistant text: `paddingLeft=3` markdown (`message-parts.tsx:147-174`);
//! - reasoning: default hide-mode `SessionReasoningGroupView` has an inline
//!   header and, when opened, a separately bordered body (`routes/session/
//!   index.tsx:1742-1858`); show mode uses `ReasoningPart` instead
//!   (`message-parts.tsx:20-96`);
//! - assistant footer: `Agent · model · duration · N tok/s · interrupted`
//!   (`routes/session/index.tsx:1934-1985`).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::VecDeque,
    hash::{Hash, Hasher},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

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
// One indexed frame can include every history row, every retained live part,
// and the still-open live row. A smaller LRU thrashes during the count pass.
const MAX_REASONING_HEIGHTS: usize = crate::history::WINDOW_ROWS + crate::app::LIVE_PARTS_MAX + 1;

#[cfg(test)]
thread_local! {
    static INDEXED_TRAVERSALS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static REASONING_BODY_RENDERS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn indexed_traversals() -> usize {
    INDEXED_TRAVERSALS.get()
}

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
    bytes: usize,
}

struct ReasoningHeight {
    part: (i64, usize),
    revision: u64,
    width: u16,
    height: usize,
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
    index_bytes: usize,
    reasoning_heights: VecDeque<ReasoningHeight>,
    footer_sources: VecDeque<(u64, usize)>,
    #[cfg(test)]
    parses: usize,
    #[cfg(test)]
    parsed_bytes: usize,
}

impl MarkdownCache {
    fn footer_source<'a>(&mut self, text: &'a str) -> &'a str {
        if !text.ends_with('\n') {
            return text;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hasher);
        let revision = hasher.finish();
        if let Some((_, end)) = self.footer_sources.iter().find(|(key, _)| *key == revision) {
            return &text[..*end];
        }
        let source = paragraph_before_footer(text);
        self.footer_sources.push_back((revision, source.len()));
        if self.footer_sources.len() > MAX_REASONING_HEIGHTS {
            self.footer_sources.pop_front();
        }
        source
    }
    fn reasoning_height(
        &mut self,
        part: (i64, usize),
        reasoning: &ReasoningBlock,
        theme: &Theme,
        width: u16,
    ) -> usize {
        let content = reasoning_content(reasoning);
        let mut end = content.len().min(LIVE_MARKDOWN_BYTES);
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content[..end].hash(&mut hasher);
        content.len().hash(&mut hasher);
        let revision = hasher.finish();
        if let Some(position) = self.reasoning_heights.iter().position(|entry| {
            entry.part == part && entry.revision == revision && entry.width == width
        }) {
            let entry = self
                .reasoning_heights
                .remove(position)
                .expect("height index");
            let height = entry.height;
            self.reasoning_heights.push_back(entry);
            return height;
        }
        // Measure with the same bounded Markdown renderer as the painted body.
        // Estimates drift for tables, fences and wide glyphs when hidden groups
        // never enter the viewport; retain only the exact height, not the lines.
        let height = reasoning_group_body(reasoning, theme, width).len();
        self.set_reasoning_height(part, revision, width, height);
        height
    }

    fn set_reasoning_height(
        &mut self,
        part: (i64, usize),
        revision: u64,
        width: u16,
        height: usize,
    ) {
        self.reasoning_heights.retain(|entry| entry.part != part);
        self.reasoning_heights.push_back(ReasoningHeight {
            part,
            revision,
            width,
            height,
        });
        if self.reasoning_heights.len() > MAX_REASONING_HEIGHTS {
            self.reasoning_heights.pop_front();
        }
    }

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
        self.indexes.retain(|p| {
            if p.part == part {
                self.index_bytes -= p.bytes;
                false
            } else {
                true
            }
        });
        let bytes = Self::pages_bytes(&pages);
        self.index_bytes += bytes;
        self.indexes.push_back(IndexedPart {
            part,
            revision,
            width,
            pages: pages.clone(),
            bytes,
        });
        // The index is bounded by the loaded history window, with a fixed
        // ceiling for pathological numbers of independently paged parts.
        while self.indexes.len() > 32 || self.index_bytes() > MAX_INDEX_BYTES {
            if let Some(index) = self.indexes.pop_front() {
                self.index_bytes -= index.bytes;
            }
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
        self.index_bytes
    }

    fn pages_bytes(pages: &[SourcePage]) -> usize {
        pages
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
        self.bytes
            + self.index_bytes()
            + self.reasoning_heights.capacity() * std::mem::size_of::<ReasoningHeight>()
            + self.footer_sources.capacity() * std::mem::size_of::<(u64, usize)>()
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
    /// Hide mode offers the per-part +/- header; show mode is always open.
    pub(crate) toggleable: bool,
    /// Stable UI-only part address; absent on ad-hoc presentation fixtures.
    pub(crate) identity: Option<ReasoningIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ReasoningIdentity {
    Durable(i64, usize),
    Live(u64, usize),
}

/// Assistant footer data, projected from durable turns or live events.
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
    /// Latest provider generation's reported input/output context measurement.
    /// Independent of complete turn usage used for tok/s.
    pub context_usage: Option<(u64, u64)>,
    /// Provider-active streaming time in milliseconds, when reported.
    pub streamed_ms: Option<u64>,
    /// Current owner presentation setting; not durable turn/provider metadata.
    /// Pinned config/index.tsx:309 defaults absence to true.
    pub session_tps: Option<bool>,
    /// Upstream `error.message === "Step interrupted"`.
    pub interrupted: bool,
    /// Exact terminal state, when supplied by the owning application.
    pub status: Option<String>,
    /// Agent categorical slot pinned at generation time.
    pub agent_color_index: Option<usize>,
    /// Current bounded projection omitted bytes/parts. This is viewing status,
    /// not response text or a claim that producer output is recoverable.
    pub preview_limited: bool,
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
    transcript_with_expansion(
        rows,
        theme,
        width,
        terminal_width,
        agent_color,
        cache,
        &|_| false,
    )
}

pub(crate) fn transcript_with_expansion(
    rows: &[HistoryRow],
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
    expanded: &impl Fn(&str) -> bool,
) -> Vec<Line> {
    let mut out = Vec::new();
    let mut grouped_until = 0;
    for (index, row) in rows.iter().enumerate() {
        if index < grouped_until {
            continue;
        }
        if let Some(group) = reasoning_group(rows, index) {
            grouped_until = index + group.members.len();
            visit_reasoning_group(&group, theme, width, index, cache, |_, render| {
                out.extend(render())
            });
            continue;
        }
        if let Some(group) = exploration_entry(rows, index, theme) {
            if group.is_empty() {
                continue;
            }
            let op = &row.tool.as_ref().expect("group has tool").op;
            out.extend(group);
            if expanded(op) {
                for member in rows[index..]
                    .iter()
                    .take_while(|row| exploration_kind(row).is_some())
                {
                    out.push(exploration_member(member, theme));
                }
            }
        } else if let Some(lines) = expandable_tool_entry(
            row,
            theme,
            width,
            expanded,
            shell_leading_margin(rows, index),
        ) {
            out.extend(lines);
        } else {
            out.extend(render_row(
                row,
                (index, footer_follows(rows, index)),
                theme,
                width,
                terminal_width,
                &agent_color,
                cache,
            ));
        }
    }
    out
}

fn shell_leading_margin(rows: &[HistoryRow], index: usize) -> bool {
    // A direct command is the first visible session row; consumed RAW admission
    // anchors do not create a second leading spacer ahead of its BlockTool.
    !matches!(rows[index].role.as_str(), "shell" | "shell_input")
        || rows[..index]
            .iter()
            .any(|row| row.role != "shell_input_delivered")
}

fn expandable_tool_entry(
    row: &HistoryRow,
    theme: &Theme,
    width: u16,
    expanded: &dyn Fn(&str) -> bool,
    leading: bool,
) -> Option<Vec<Line>> {
    let card = row
        .tool
        .as_ref()
        .filter(|_| matches!(row.role.as_str(), "tool" | "shell" | "shell_input"))?;
    let rendered = match &card.render {
        crate::tools::ToolRender::Shell(shell) => {
            crate::tools::shell_block_expanded(shell, card, theme, width, expanded(&card.op))
        }
        crate::tools::ToolRender::Subagent(subagent) => {
            crate::tools::subagent_block_expanded(subagent, card, theme, width, expanded(&card.op))
        }
        crate::tools::ToolRender::Inline(inline @ crate::tools::InlineRender::Generic { .. })
            if !matches!(
                card.state.as_str(),
                "argument_stream" | "permission_pending"
            ) =>
        {
            crate::tools::generic_block_expanded(inline, card, theme, width, expanded(&card.op))
        }
        _ => return None,
    };
    let mut lines = if leading {
        vec![Line::plain("")]
    } else {
        Vec::new()
    };
    lines.extend(rendered);
    Some(lines.into_iter().map(sanitize_line).collect())
}

/// BlockTool hover darkens the whole raised box; exploration hover changes text only.
/// Call only after an eligible operation hit from exploration_header_at.
pub(crate) fn tool_hover_range(rows: &[Line], theme: &Theme, row: usize) -> std::ops::Range<usize> {
    let block = |line: &Line| line.style().bg == Some(theme.background_raised());
    let mut start = row;
    let mut end = (row + 1).min(rows.len());
    if rows.get(row).is_some_and(block) {
        while start > 0 && block(&rows[start - 1]) {
            start -= 1;
        }
        while end < rows.len() && block(&rows[end]) {
            end += 1;
        }
    } else {
        // Wrapped exploration headers have a spacer above and either a spacer
        // or an icon-bearing member below. The hit already excludes members.
        while start > 0 && !rows[start - 1].plain_text().trim().is_empty() {
            start -= 1;
        }
        while end < rows.len() {
            let text = rows[end].plain_text();
            if text.trim().is_empty() || text.trim_start().starts_with(['→', '⋯']) {
                break;
            }
            end += 1;
        }
    }
    start..end
}

pub(crate) fn hover_tool_content(line: &Line, theme: &Theme) -> Line {
    let bg = theme.background_raised();
    if line.style().bg == Some(bg) {
        let hover = theme.decrease(bg);
        return Line::new(
            line.spans()
                .iter()
                .map(|span| Span::styled(span.content(), span.style().bg(hover)))
                .collect(),
        )
        .with_style(line.style().bg(hover));
    }
    Line::new(
        line.spans()
            .iter()
            .map(|span| {
                Span::styled(
                    span.content(),
                    if span.style().fg == Some(theme.text_muted()) {
                        span.style().fg(theme.text())
                    } else {
                        span.style()
                    },
                )
            })
            .collect(),
    )
    .with_style(line.style())
}

/// The upstream grouping path is ["reasoning"] only for adjacent part entries.
/// An assistant footer, text, tool or user row terminates the group, even if
/// it shares the same turn. Show mode uses the per-part ReasoningPart fallback.
pub(crate) fn reasoning_group_member(row: &HistoryRow) -> bool {
    row.role == "assistant"
        && row.text.is_empty()
        && row.meta.is_none()
        && row.tool.is_none()
        && row.reasoning.as_ref().is_some_and(|r| r.toggleable)
}

pub(crate) fn visible_reasoning(row: &HistoryRow) -> bool {
    row.reasoning
        .as_ref()
        .is_some_and(|r| !reasoning_content(r).is_empty())
}

struct ReasoningGroup<'a> {
    members: &'a [HistoryRow],
    steps: usize,
    title: String,
    duration_ms: Option<u64>,
    running: bool,
}

fn reasoning_group(rows: &[HistoryRow], index: usize) -> Option<ReasoningGroup<'_>> {
    if !reasoning_group_member(rows.get(index)?)
        || index > 0 && reasoning_group_member(&rows[index - 1])
    {
        return None;
    }
    let count = rows[index..]
        .iter()
        .take_while(|row| reasoning_group_member(row))
        .count();
    let members = &rows[index..index + count];
    let steps = members.iter().filter(|row| visible_reasoning(row)).count();
    if steps == 0 || count < 2 {
        return None;
    }
    let mut duration_ms = None::<u64>;
    let mut title = String::new();
    let mut running = false;
    let closed_by_next = rows.get(index + count).is_some_and(|next| {
        next.meta
            .as_ref()
            .is_none_or(|meta| meta.status.as_deref() != Some("started"))
    });
    for row in members {
        let reasoning = row.reasoning.as_ref().expect("group member");
        // An unresolved/redacted part is absent from the painted refs but is
        // still a child of the upstream group for completion purposes.
        running |= reasoning.running;
        if !visible_reasoning(row) {
            continue;
        }
        if let Some(ms) = reasoning.duration_ms.filter(|ms| *ms > 0) {
            duration_ms = Some(duration_ms.unwrap_or(0).saturating_add(ms));
        }
        let content = reasoning_content(reasoning);
        let next = reasoning_title(&content);
        if !next.is_empty() {
            title = next.to_string();
        } else if !reasoning.running || closed_by_next {
            // Upstream latest memo clears a previous title when an untitled
            // last visible part (or its message) completes.
            title.clear();
        }
    }
    // A following text/tool/footer closes the group even if an individual
    // reasoning part never received its own completion event.
    running &= !closed_by_next;
    Some(ReasoningGroup {
        members,
        steps,
        title,
        duration_ms,
        running,
    })
}

fn visit_reasoning_group(
    group: &ReasoningGroup<'_>,
    theme: &Theme,
    width: u16,
    first_index: usize,
    cache: Option<&RefCell<MarkdownCache>>,
    mut emit: impl FnMut(usize, &mut dyn FnMut() -> Vec<Line>),
) {
    let lines = vec![Line::plain(""), reasoning_group_header(group, theme, width)];
    emit(2, &mut || lines.clone());
    if group.first().expanded {
        for (ordinal, row) in group.members.iter().enumerate() {
            if !visible_reasoning(row) {
                continue;
            }
            let part = row.reasoning.as_ref().expect("group member");
            let key = (row.seq, first_index + ordinal);
            let height = if let Some(cache) = cache {
                cache.borrow_mut().reasoning_height(key, part, theme, width)
            } else {
                0
            };
            let mut render = || reasoning_group_body(part, theme, width);
            if cache.is_some() {
                emit(height, &mut render);
            } else {
                let body = render();
                emit(body.len(), &mut || body.clone());
            }
        }
    }
}

fn reasoning_group_body(part: &ReasoningBlock, theme: &Theme, width: u16) -> Vec<Line> {
    let mut expanded = part.clone();
    expanded.expanded = true;
    reasoning_lines(&expanded, theme, width)
        .into_iter()
        .skip(2)
        .collect()
}

impl ReasoningGroup<'_> {
    fn first(&self) -> &ReasoningBlock {
        self.members
            .iter()
            .find(|row| visible_reasoning(row))
            .and_then(|row| row.reasoning.as_ref())
            .expect("group has visible reasoning")
    }
}

fn reasoning_group_header(group: &ReasoningGroup<'_>, theme: &Theme, width: u16) -> Line {
    let mut header = group.first().clone();
    header.text = format!("**{}**", group.title);
    header.duration_ms = group.duration_ms;
    header.running = group.running;
    if group.running {
        return clipped_reasoning_header(&header, theme, width);
    }
    let base = if header.expanded || group.title.is_empty() {
        "Thought".to_string()
    } else {
        format!("Thought: {}", group.title)
    };
    let mut label = base;
    if group.steps > 1 {
        label.push_str(&format!(" · {} steps", group.steps));
    }
    if let Some(duration) = group.duration_ms.map(Locale::duration) {
        label.push_str(&format!(" · {duration}"));
    }
    let mut spans = reasoning_line(&header, theme).spans().to_vec();
    let style = spans.last().expect("header label").style();
    *spans.last_mut().expect("header label") = Span::styled(label, style);
    clipped_line(sanitize_line(Line::new(spans)), width)
}

/// Upstream groups adjacent read/glob/grep parts in their first-seen order
/// (`grouping/session.ts:67-71`, `index.tsx:1865-1929`). Only running or
/// confirmed, untruncated results can be collapsed; uncertain outcomes never
/// enter a group and their detail rows remain visible.
fn exploration_kind(row: &HistoryRow) -> Option<(&'static str, bool)> {
    let card = row.tool.as_ref()?;
    if row.role != "tool"
        || !matches!(card.state.as_str(), "completed" | "started" | "running")
        || card.preview_limited()
    {
        return None;
    }
    let name = match &card.render {
        crate::tools::ToolRender::Inline(crate::tools::InlineRender::Read { .. }) => Some("read"),
        crate::tools::ToolRender::Inline(
            crate::tools::InlineRender::Glob { .. } | crate::tools::InlineRender::Grep { .. },
        ) => Some("search"),
        _ => None,
    }?;
    Some((name, card.state == "completed"))
}

fn exploration_member(row: &HistoryRow, theme: &Theme) -> Line {
    sanitize_line(crate::tools::exploration_member(
        row.tool.as_ref().expect("group has tool"),
        theme,
    ))
}

fn exploration_entry(rows: &[HistoryRow], index: usize, theme: &Theme) -> Option<Vec<Line>> {
    exploration_kind(rows.get(index)?)?;
    if index > 0 && exploration_kind(&rows[index - 1]).is_some() {
        return Some(Vec::new());
    }
    let mut counts = Vec::<(&str, usize)>::new();
    let mut completed = true;
    for row in &rows[index..] {
        let Some((name, done)) = exploration_kind(row) else {
            break;
        };
        completed &= done;
        if let Some((_, count)) = counts.iter_mut().find(|(known, _)| *known == name) {
            *count += 1;
        } else {
            counts.push((name, 1));
        }
    }
    let label = counts
        .into_iter()
        .map(|(name, count)| {
            format!(
                "{count} {name}{}",
                if count == 1 {
                    ""
                } else if name == "search" {
                    "es"
                } else {
                    "s"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let muted = Style::default().fg(theme.text_muted());
    Some(vec![
        Line::plain(""),
        Line::new(vec![
            Span::plain(" ".repeat(MESSAGE_PADDING)),
            Span::styled(if completed { "→" } else { "⋯" }, muted),
            Span::plain(" "),
            Span::styled(
                format!(
                    "{} — {label}",
                    if completed { "Explored" } else { "Exploring" }
                ),
                muted,
            ),
        ]),
    ])
}

fn render_row(
    row: &HistoryRow,
    identity: (usize, bool),
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let (index, footer_after) = identity;
    let lines = match row.role.as_str() {
        "shell_input_delivered" => Vec::new(),
        "child_notice" => child_notice_block(row, index, theme, width),
        "shell_notice" => shell_notice_block(row, index, theme, width),
        "compaction" | "compaction_failed" | "compaction_queued" => {
            crate::compaction::block(row, theme, width)
        }
        "reverted" => reverted_block(&row.text, row.agent.as_deref().unwrap_or(""), theme, width),
        "user" => {
            let mut lines = Vec::new();
            if index > 0 {
                // SessionRowView marginTop=1 (index.tsx:1433-1435),
                // separate from the user block's own paddingTop=1.
                lines.push(Line::plain(""));
            }
            lines.extend(user_block(row, theme, width, agent_color));
            lines
        }
        "assistant" => assistant_block(
            row,
            (index, footer_after),
            theme,
            width,
            terminal_width,
            agent_color,
            cache,
        ),
        "tool" | "shell" | "shell_input" => {
            if let Some(card) = &row.tool {
                let block = crate::tools::tool_block(card, theme, width);
                let mut lines = Vec::new();
                if !block.is_empty() {
                    lines.push(Line::plain(""));
                    lines.extend(block);
                }
                lines
            } else {
                notice_block(row)
            }
        }
        // SessionRowView supplies one top margin; the notice itself has
        // paddingLeft=3 and muted text (session/index.tsx:1433, 1999-2013).
        "model_switch" => vec![
            Line::plain(""),
            Line::new(vec![
                Span::plain(" ".repeat(MESSAGE_PADDING)),
                Span::styled(row.text.clone(), Style::default().fg(theme.text_muted())),
            ]),
        ],
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
    identity: (usize, bool, bool),
    theme: &Theme,
    widths: (u16, u16),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    mut emit: impl FnMut(Vec<Line>),
) {
    let (index, live, footer_after) = identity;
    let (width, terminal_width) = widths;
    if row.role == "reverted" {
        let mut lines = reverted_block(&row.text, row.agent.as_deref().unwrap_or(""), theme, width);
        emit(vec![lines.remove(0)]);
        emit(lines);
        return;
    }
    if row.role == "user" && width > 0 {
        if index > 0 {
            // Keep the indexed seek height and materialized row in sync.
            emit(vec![Line::plain("")]);
        }
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
            let body = Style::default().fg(theme.text()).bg(bg);
            for raw in row.text.split('\n') {
                let line = Line::new(vec![Span::styled(safe_text(raw), body)]);
                // Byte chunks reset word wrapping in the middle of a physical
                // line. Stream complete visual rows instead: identical to U34
                // and the full wrapper, retaining only one width-bounded row.
                styled::visit_source_space_line(&line, inner, |wrapped| {
                    let mut spans = vec![
                        Span::styled("┃", border),
                        Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
                    ];
                    spans.extend(wrapped.spans().iter().cloned());
                    emit(vec![user_row(&spans, bg, width as usize)]);
                });
            }
        }
        if !row.chips.is_empty() {
            let mut chip_lines = vec![user_row(&[Span::styled("┃", border)], bg, width as usize)];
            let inner = (width as usize).saturating_sub(1 + USER_PADDING).max(1);
            for chips in chip_rows(&row.chips, theme, inner) {
                let mut spans = vec![
                    Span::styled("┃", border),
                    Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
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
            (index, live, footer_after),
            theme,
            (width, terminal_width),
            agent_color,
            cache,
            |_, _, render| emit(render()),
        );
        return;
    }
    emit(render_row(
        row,
        (index, footer_after),
        theme,
        width,
        terminal_width,
        agent_color,
        Some(cache),
    ));
}

fn visit_assistant_indexed(
    row: &HistoryRow,
    identity: (usize, bool, bool),
    theme: &Theme,
    widths: (u16, u16),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    mut emit: impl FnMut(usize, bool, &mut dyn FnMut() -> Vec<Line>),
) {
    let (index, live, footer_after) = identity;
    let (width, terminal_width) = widths;
    let text = if row.meta.is_some() || footer_after {
        cache.borrow_mut().footer_source(&row.text)
    } else {
        &row.text
    };
    if let Some(reasoning) = &row.reasoning {
        let lines = reasoning_lines(reasoning, theme, width);
        emit(lines.len(), false, &mut || lines.clone());
    }
    if !text.trim().is_empty() {
        emit(1, false, &mut || vec![Line::plain("")]);
        if live && text.len() > LIVE_MARKDOWN_BYTES {
            let mut end = LIVE_MARKDOWN_BYTES;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            let preview = &text[..end];
            // The live preview stays bounded; a completed part is indexed in
            // full and can be scrolled, including the region past this limit.
            let height = estimated_lines(preview, width) + 1;
            emit(height, false, &mut || {
                let mut lines =
                    cache
                        .borrow_mut()
                        .render((row.seq, index, 0), preview, theme, width);
                lines.push(Line::styled("   … [Live Markdown preview limited; full response available in history after completion]", Style::default().fg(theme.text_muted())));
                lines
            });
        } else {
            let pages = cache.borrow_mut().pages((row.seq, index), text, width);
            for (number, page) in pages.into_iter().enumerate() {
                let height = page.height;
                emit(height, false, &mut || {
                    let mut source = String::new();
                    if let Some((start, end)) = page.table_header {
                        source.push_str(&text[start..end]);
                    }
                    if let Some(open) = &page.fence {
                        source.push_str(open);
                        source.push('\n');
                    }
                    let chunk = &text[page.start..page.end];
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
        emit(2, true, &mut || {
            vec![Line::plain(""), sanitize_line(footer.clone())]
        });
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
    let widths: Vec<_> = (0..count)
        .map(|i| {
            rows.iter()
                .filter_map(|r| r.get(i))
                .map(|c| styled::span_width(&Span::plain(c)))
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();
    fit_table_widths(widths, available)
}

/// TextPart selects OpenTUI's grid/full table with one cell of padding on
/// either side. Its TextTable expands spare space equally across columns
/// (`@opentui/core@0.5.10 renderables/TextTable.ts:expandColumnWidths`). Work
/// in content cells, excluding borders and two padding cells per column.
fn fit_table_widths(mut widths: Vec<usize>, available: usize) -> Vec<usize> {
    let count = widths.len();
    if count == 0 || available == usize::MAX {
        return widths;
    }
    let usable = available.saturating_sub(count * 3 + 1).max(count);
    let natural = widths.iter().sum::<usize>();
    if natural <= usable {
        let extra = usable - natural;
        for (i, width) in widths.iter_mut().enumerate() {
            *width += extra / count + usize::from(i < extra % count);
        }
        return widths;
    }

    // Keep the existing narrow-table policy: shorter columns reach their
    // intrinsic width before a long cell consumes the rest. The proportional
    // OpenTUI shrinker would change long-cell paging and label visibility.
    let intrinsic = widths.clone();
    widths.fill(1);
    for _ in 0..usable.saturating_sub(count).min(available) {
        let col = (0..count)
            .filter(|&i| widths[i] < intrinsic[i])
            .min_by_key(|&i| widths[i])
            .unwrap_or(count - 1);
        widths[col] += 1;
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
#[cfg(test)]
pub(crate) fn visible_transcript(
    rows: &[HistoryRow],
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
) -> (Vec<Line>, usize) {
    let (lines, total, _, _) = visible_transcript_indexed(
        rows,
        theme,
        (width, terminal_width),
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded: &|_| false,
            point: None,
            retries: 2,
        },
    );
    (lines, total)
}

/// Map a visible exploration header or expandable Shell box to its operation.
/// Uses the same index, wrapping and sticky scroll slice as the painted frame;
/// only viewport rows are materialized, even for long histories.
#[cfg(test)]
pub(crate) fn exploration_header_at(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    hit: (&impl Fn(&str) -> bool, (usize, usize)),
) -> Option<String> {
    exploration_header_at_with_range(rows, theme, widths, viewport, agent_color, cache, hit)
        .map(|(operation, _)| operation)
}

/// Owner-derived visible hover extent, separate from untrusted parameter/body text.
pub(crate) fn exploration_header_at_with_range(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    hit: (&impl Fn(&str) -> bool, (usize, usize)),
) -> Option<(String, std::ops::Range<usize>)> {
    visible_transcript_indexed(
        rows,
        theme,
        widths,
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded: hit.0,
            point: Some(hit.1),
            retries: 2,
        },
    )
    .2
    .and_then(|hit| match hit {
        TranscriptHit::Exploration(op, range) => Some((op, range)),
        TranscriptHit::Reasoning(_) | TranscriptHit::ChildNotice(_, _, _) => None,
    })
}

pub(crate) fn reasoning_header_at(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    hit: (&impl Fn(&str) -> bool, (usize, usize)),
) -> Option<ReasoningIdentity> {
    visible_transcript_indexed(
        rows,
        theme,
        widths,
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded: hit.0,
            point: Some(hit.1),
            retries: 2,
        },
    )
    .2
    .and_then(|hit| match hit {
        TranscriptHit::Reasoning(id) => Some(id),
        TranscriptHit::Exploration(_, _) | TranscriptHit::ChildNotice(_, _, _) => None,
    })
}

pub(crate) fn captured_child_at_with_range(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    hit: (&impl Fn(&str) -> bool, (usize, usize)),
) -> Option<(CapturedChildTarget, std::ops::Range<usize>, bool)> {
    visible_transcript_indexed(
        rows,
        theme,
        widths,
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded: hit.0,
            point: Some(hit.1),
            retries: 2,
        },
    )
    .2
    .and_then(|hit| match hit {
        TranscriptHit::ChildNotice(job, range, tool) => Some((job, range, tool)),
        _ => None,
    })
}

pub(crate) fn hover_child_notice(
    line: &Line,
    theme: &Theme,
    job: &oc_core::queries::ChildJob,
) -> Line {
    let width = UnicodeWidthStr::width(line.plain_text().as_str()).min(u16::MAX as usize) as u16;
    child_notice_line(job, theme, width, true)
        .unwrap_or_else(|| line.clone())
        .with_style(line.style())
}

pub(crate) fn hover_subagent_content(line: &Line, theme: &Theme) -> Line {
    Line::new(
        line.spans()
            .iter()
            .map(|span| {
                Span::styled(
                    span.content(),
                    if span.style().fg == Some(theme.text_muted())
                        && span.style().bg != Some(theme.decrease(theme.background()))
                    {
                        span.style().fg(theme.text())
                    } else {
                        span.style()
                    },
                )
            })
            .collect(),
    )
    .with_style(line.style())
}

#[cfg(test)]
pub(crate) fn visible_transcript_expanded(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    expanded: &impl Fn(&str) -> bool,
) -> (Vec<Line>, usize) {
    let (lines, total, _, _) = visible_transcript_indexed(
        rows,
        theme,
        widths,
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded,
            point: None,
            retries: 2,
        },
    );
    (lines, total)
}

/// A row identity produced while visiting the actual visible, wrapped user
/// block. The ordinal distinguishes synthetic rows (which share i64::MAX);
/// Owner actions must use `message_id`; sequence/ordinal only identify presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UserMessageTarget {
    pub message_id: Option<std::sync::Arc<oc_core::session::MessageId>>,
    pub seq: i64,
    pub ordinal: usize,
    pub reverted: bool,
}

/// Conversation-only RevertMessage, snapshots disabled (index.tsx:2172–2250).
fn reverted_block(count: &str, shortcut: &str, theme: &Theme, width: u16) -> Vec<Line> {
    let bg = theme.background_raised();
    let border = Style::default().fg(bg).bg(bg);
    let muted = Style::default().fg(theme.text_muted()).bg(bg);
    let mut lines = vec![Line::plain("")];
    let inner = (width as usize).saturating_sub(3).max(1);
    let body = [
        Line::plain(""),
        Line::styled(
            format!(
                "{count} message{} reverted",
                if count == "1" { "" } else { "s" }
            ),
            muted,
        ),
        Line::new(vec![
            Span::styled(shortcut, Style::default().fg(theme.text()).bg(bg)),
            Span::styled(" or /redo to restore", muted),
        ]),
        Line::plain(""),
    ];
    for line in body {
        for wrapped in styled::wrap_line_limited(&line, inner, MAX_MARKDOWN_ROWS) {
            let mut spans = vec![Span::styled("┃", border), Span::styled("  ", muted)];
            spans.extend(wrapped.spans().iter().cloned());
            lines.push(user_row(&spans, bg, width as usize));
        }
    }
    lines
}

/// Only the inner box receives the hover fill; the agent-colored left border
/// and chip-specific surfaces remain as painted by upstream.
pub(crate) fn hover_user_content(line: &Line, theme: &Theme) -> Line {
    hover_message_content(line, theme, theme.user_message_background())
}

pub(crate) fn hover_reverted_content(line: &Line, theme: &Theme) -> Line {
    hover_message_content(line, theme, theme.background_raised())
}

fn hover_message_content(line: &Line, theme: &Theme, base: Color) -> Line {
    let hover = theme.decrease(base);
    let spans = line
        .spans()
        .iter()
        .enumerate()
        .map(|(index, span)| {
            let style = span.style();
            Span::styled(
                span.content().to_owned(),
                if !(index == 0 && span.content() == "┃") && style.bg == Some(base) {
                    style.bg(hover)
                } else {
                    style
                },
            )
        })
        .collect();
    Line::new(spans).with_style(line.style().bg(hover))
}

pub(crate) fn visible_transcript_user_targets(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    expanded: &impl Fn(&str) -> bool,
) -> (Vec<Line>, usize, Vec<Option<UserMessageTarget>>) {
    let (lines, total, _, targets) = visible_transcript_indexed(
        rows,
        theme,
        widths,
        viewport,
        &agent_color,
        cache,
        ExplorationOptions {
            expanded,
            point: None,
            retries: 2,
        },
    );
    (lines, total, targets)
}

#[derive(Clone, Copy)]
struct ExplorationOptions<'a> {
    expanded: &'a dyn Fn(&str) -> bool,
    point: Option<(usize, usize)>,
    retries: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CapturedChildTarget {
    Navigate(Box<oc_core::queries::ChildJob>),
    ErrorDetails(String),
}

enum TranscriptHit {
    Exploration(String, std::ops::Range<usize>),
    Reasoning(ReasoningIdentity),
    ChildNotice(CapturedChildTarget, std::ops::Range<usize>, bool),
}

fn visible_transcript_indexed(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    options: ExplorationOptions<'_>,
) -> (
    Vec<Line>,
    usize,
    Option<TranscriptHit>,
    Vec<Option<UserMessageTarget>>,
) {
    #[cfg(test)]
    INDEXED_TRAVERSALS.set(INDEXED_TRAVERSALS.get() + 1);
    let (width, terminal_width) = widths;
    let (height, scroll, live_row) = viewport;
    let total = transcript_part_positions(
        rows,
        theme,
        (width, terminal_width, live_row),
        agent_color,
        cache,
        options.expanded,
        |_, _, _| {},
    );
    let end = total.saturating_sub(scroll.min(total.saturating_sub(height)));
    let start = end.saturating_sub(height);
    let mut visible = Vec::with_capacity(height);
    let mut user_targets = vec![None; height];
    if start == 0 {
        visible.push(Line::plain(""));
    }
    let mut position = 1;
    let mut height_changed = false;
    let mut hit = None;
    let mut grouped_until = 0;
    for (index, row) in rows.iter().enumerate() {
        if position >= end {
            break;
        }
        if index < grouped_until {
            continue;
        }
        if let Some(group) = reasoning_group(rows, index) {
            grouped_until = index + group.members.len();
            if let Some((x, y)) = options.point
                && let Some(id) = group.first().identity
                && position + 1 == start + y
                && position + 1 < end
            {
                let header = reasoning_group_header(&group, theme, width);
                let text = header.plain_text();
                if x >= MESSAGE_PADDING && x < UnicodeWidthStr::width(text.trim_end()) {
                    hit = Some(TranscriptHit::Reasoning(id));
                }
            }
            visit_reasoning_group(
                &group,
                theme,
                width,
                index,
                Some(cache),
                |height, render| {
                    if position < end && position + height > start {
                        let lines = render();
                        height_changed |= lines.len() != height;
                        add_visible_lines(
                            lines,
                            Some(width),
                            (start, end),
                            &mut position,
                            &mut visible,
                        );
                    } else {
                        position += height;
                    }
                },
            );
            continue;
        }
        if let Some(group) = exploration_entry(rows, index, theme) {
            let is_expanded = !group.is_empty()
                && (options.expanded)(&row.tool.as_ref().expect("group has tool").op);
            if !group.is_empty()
                && let Some((x, y)) = options.point
            {
                // The first line is vertical spacing. Only a wrapped header
                // text cell can toggle; neither spacing nor the row tail can.
                let header_start = position
                    + styled::wrap_line_limited(&group[0], width as usize, MAX_MARKDOWN_ROWS).len();
                for (offset, line) in
                    styled::wrap_line_limited(&group[1], width as usize, MAX_MARKDOWN_ROWS)
                        .iter()
                        .enumerate()
                {
                    if header_start + offset == start + y && header_start + offset < end {
                        let text = line.plain_text();
                        let leading = UnicodeWidthStr::width(text.as_str())
                            - UnicodeWidthStr::width(text.trim_start());
                        let last = UnicodeWidthStr::width(text.trim_end());
                        if x >= leading && x < last {
                            hit = row.tool.as_ref().map(|card| {
                                TranscriptHit::Exploration(
                                    card.op.clone(),
                                    header_start.saturating_sub(start)
                                        ..(header_start
                                            + styled::wrap_line_limited(
                                                &group[1],
                                                width as usize,
                                                MAX_MARKDOWN_ROWS,
                                            )
                                            .len())
                                        .min(end)
                                        .saturating_sub(start),
                                )
                            });
                        }
                    }
                }
            }
            add_visible_lines(
                group,
                Some(width),
                (start, end),
                &mut position,
                &mut visible,
            );
            if is_expanded {
                for member in rows[index..]
                    .iter()
                    .take_while(|row| exploration_kind(row).is_some())
                {
                    add_visible_lines(
                        vec![exploration_member(member, theme)],
                        Some(width),
                        (start, end),
                        &mut position,
                        &mut visible,
                    );
                }
            }
        } else if let Some(lines) = expandable_tool_entry(
            row,
            theme,
            width,
            options.expanded,
            shell_leading_margin(rows, index),
        ) {
            let margin = usize::from(shell_leading_margin(rows, index));
            if let Some((x, y)) = options.point
                && let Some(card) = &row.tool
                && x < width as usize
                && start + y >= position + margin
                && start + y < position + lines.len()
                && start + y < end
            {
                let relative = start + y - position;
                let extent = match &card.render {
                    crate::tools::ToolRender::Subagent(subagent) => {
                        let header = crate::tools::subagent_block_expanded(
                            subagent, card, theme, width, false,
                        );
                        let eligible = header.get(relative - margin).is_some_and(|line| {
                            let text = line.plain_text();
                            let leading = UnicodeWidthStr::width(text.as_str())
                                - UnicodeWidthStr::width(text.trim_start());
                            x >= leading && x < UnicodeWidthStr::width(text.trim_end())
                        });
                        if eligible {
                            let target = if crate::tools::subagent_failed(card) {
                                Some(CapturedChildTarget::ErrorDetails(card.op.clone()))
                            } else {
                                card.child_job.clone().map(CapturedChildTarget::Navigate)
                            };
                            if let Some(target) = target {
                                hit = Some(TranscriptHit::ChildNotice(
                                    target,
                                    (position + margin).saturating_sub(start)
                                        ..(position + margin + header.len())
                                            .min(end)
                                            .saturating_sub(start),
                                    true,
                                ));
                            }
                        }
                        None
                    }
                    crate::tools::ToolRender::Shell(_) => {
                        crate::tools::shell_expandable(card, width).then_some(lines.len() - margin)
                    }
                    crate::tools::ToolRender::Inline(
                        inline @ crate::tools::InlineRender::Generic { .. },
                    ) if crate::tools::generic_expandable(card) => {
                        let header =
                            crate::tools::generic_block_expanded(inline, card, theme, width, false);
                        header
                            .get(relative - 1)
                            .is_some_and(|line| {
                                let text = line.plain_text();
                                let leading = UnicodeWidthStr::width(text.as_str())
                                    - UnicodeWidthStr::width(text.trim_start());
                                x >= leading && x < UnicodeWidthStr::width(text.trim_end())
                            })
                            .then_some(header.len())
                    }
                    _ => None,
                };
                if let Some(extent) = extent {
                    hit = Some(TranscriptHit::Exploration(
                        card.op.clone(),
                        (position + margin).saturating_sub(start)
                            ..(position + margin + extent).min(end).saturating_sub(start),
                    ));
                }
            }
            add_visible_lines(
                lines,
                Some(width),
                (start, end),
                &mut position,
                &mut visible,
            );
        } else if row.role == "child_notice" {
            let lines = render_row(
                row,
                (index, false),
                theme,
                width,
                terminal_width,
                &agent_color,
                Some(cache),
            );
            let header = position + usize::from(index > 0);
            if let Some((x, y)) = options.point
                && let Some(job) = &row.child_notice
                && header == start + y
                && header < end
                && let Some(line) = lines.last()
                && x >= MESSAGE_PADDING
                && x < UnicodeWidthStr::width(line.plain_text().trim_end())
            {
                hit = Some(TranscriptHit::ChildNotice(
                    CapturedChildTarget::Navigate(job.clone()),
                    header - start..header - start + 1,
                    false,
                ));
            }
            add_visible_lines(
                lines,
                Some(width),
                (start, end),
                &mut position,
                &mut visible,
            );
        } else if row.role == "assistant" && width > 0 {
            if let Some((x, y)) = options.point
                && let Some(reasoning) = row
                    .reasoning
                    .as_ref()
                    .filter(|r| r.toggleable && !reasoning_content(r).is_empty())
                && let Some(id) = reasoning.identity
            {
                let header_start = position + 1; // reasoning's leading spacer
                if header_start == start + y && header_start < end {
                    let header = clipped_reasoning_header(reasoning, theme, width);
                    let text = header.plain_text();
                    // Group header is flush with InlineToolRow's paddingLeft=3
                    // even when its separately bordered body is expanded.
                    let leading = MESSAGE_PADDING;
                    let last = UnicodeWidthStr::width(text.trim_end());
                    if x >= leading && x < last {
                        hit = Some(TranscriptHit::Reasoning(id));
                    }
                }
            }
            visit_assistant_indexed(
                row,
                (index, live_row == Some(index), footer_follows(rows, index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |height, footer, render| {
                    if position < end && position + height > start {
                        let lines = render();
                        height_changed |= lines.len() != height;
                        // Upstream keeps the footer on one row even when it
                        // exceeds the padded content box. The painter clips
                        // that row at the session column edge.
                        add_visible_lines(
                            lines,
                            (!footer).then_some(width),
                            (start, end),
                            &mut position,
                            &mut visible,
                        );
                    } else {
                        position += height;
                    }
                },
            );
        } else {
            let mut margin = (row.role == "user" && index > 0) || row.role == "reverted";
            visit_row_blocks(
                row,
                (index, false, footer_follows(rows, index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |lines| {
                    let first = visible.len();
                    add_visible_lines(
                        lines,
                        Some(width),
                        (start, end),
                        &mut position,
                        &mut visible,
                    );
                    if (row.role == "user" || row.role == "reverted") && !margin {
                        for slot in user_targets.iter_mut().take(visible.len()).skip(first) {
                            *slot = Some(UserMessageTarget {
                                message_id: row.message_id.clone(),
                                seq: row.seq,
                                ordinal: index,
                                reverted: row.role == "reverted",
                            });
                        }
                    }
                    margin = false;
                },
            );
        }
    }
    if height_changed && options.retries > 0 {
        return visible_transcript_indexed(
            rows,
            theme,
            widths,
            viewport,
            agent_color,
            cache,
            ExplorationOptions {
                retries: options.retries - 1,
                ..options
            },
        );
    }
    user_targets.resize(visible.len(), None);
    (visible, total, hit, user_targets)
}

/// Count the same cached/grouped parts used by the painter, without materializing
/// offscreen Markdown. Positions identify a part independently of preceding rows.
pub(crate) fn transcript_part_positions(
    rows: &[HistoryRow],
    theme: &Theme,
    geometry: (u16, u16, Option<usize>),
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    expanded: &dyn Fn(&str) -> bool,
    mut part: impl FnMut(usize, usize, usize),
) -> usize {
    let (width, terminal_width, live_row) = geometry;
    let mut total = 1usize;
    let mut grouped_until = 0;
    for (index, row) in rows.iter().enumerate() {
        if index < grouped_until {
            continue;
        }
        let start = total;
        if let Some(group) = reasoning_group(rows, index) {
            grouped_until = index + group.members.len();
            visit_reasoning_group(&group, theme, width, index, Some(cache), |height, _| {
                total += height
            });
            part(index, start, total);
            continue;
        }
        if let Some(group) = exploration_entry(rows, index, theme) {
            let is_expanded =
                !group.is_empty() && expanded(&row.tool.as_ref().expect("group has tool").op);
            for line in group {
                total += styled::wrap_line_limited(&line, width as usize, MAX_MARKDOWN_ROWS).len();
            }
            if is_expanded {
                for member in rows[index..]
                    .iter()
                    .take_while(|row| exploration_kind(row).is_some())
                {
                    total += styled::wrap_line_limited(
                        &exploration_member(member, theme),
                        width as usize,
                        MAX_MARKDOWN_ROWS,
                    )
                    .len();
                }
            }
        } else if let Some(lines) = expandable_tool_entry(
            row,
            theme,
            width,
            expanded,
            shell_leading_margin(rows, index),
        ) {
            for line in lines {
                total += styled::wrap_line_limited(&line, width as usize, MAX_MARKDOWN_ROWS).len();
            }
        } else if row.role == "assistant" && width > 0 {
            visit_assistant_indexed(
                row,
                (index, live_row == Some(index), footer_follows(rows, index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |height, _, _| total += height,
            );
        } else {
            visit_row_blocks(
                row,
                (index, false, footer_follows(rows, index)),
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
        part(index, start, total);
    }
    total
}

fn add_visible_lines(
    lines: Vec<Line>,
    width: Option<u16>,
    viewport: (usize, usize),
    position: &mut usize,
    visible: &mut Vec<Line>,
) {
    let (start, end) = viewport;
    for line in lines {
        let wrapped = if let Some(width) = width {
            styled::wrap_line_limited(&line, width as usize, MAX_MARKDOWN_ROWS)
                .into_iter()
                .map(|wrapped| wrapped.with_style(line.style()))
                .collect()
        } else {
            vec![line]
        };
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
    user_block_parts(
        &row.text,
        &row.chips,
        theme,
        width,
        user_agent_color(row, theme, agent_color),
    )
}

/// The real UserMessage text wrapper, also used by metadata-backed DCP cards.
/// It does not create a message, hit target or persistent user prompt.
pub(crate) fn transcript_text_block(
    text: &str,
    theme: &Theme,
    width: u16,
    color: Color,
) -> Vec<Line> {
    user_block_parts(text, &[], theme, width, color)
}

fn user_block_parts(
    text: &str,
    chips: &[Chip],
    theme: &Theme,
    width: u16,
    color: Color,
) -> Vec<Line> {
    let bg = theme.user_message_background();
    let border = Style::default().fg(color).bg(bg);
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
    if !text.is_empty() {
        for raw in text.split('\n') {
            let line = Line::new(vec![Span::styled(safe_text(raw), body)]);
            // U34's text child paints a fitting source separator at a word break;
            // the surrounding box padding keeps its own foreground.
            for wrapped in styled::wrap_source_space_line_limited(&line, inner.max(1), usize::MAX) {
                let mut spans = vec![
                    Span::styled("┃", border),
                    Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
                ];
                spans.extend(wrapped.spans().iter().cloned());
                out.push(user_row(&spans, bg, width));
            }
        }
    }
    if !chips.is_empty() {
        // Chips row: `paddingTop={1}`, `gap={1}`, `flexWrap="wrap"`.
        out.push(user_row(&[Span::styled("┃", border)], bg, width));
        for chips in chip_rows(chips, theme, inner) {
            let mut spans = vec![
                Span::styled("┃", border),
                Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
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

/// The upstream user box owns the leading padding; the text child alone uses
/// `theme.text.base` (`routes/session/index.tsx:2339-2345`).
fn user_padding(bg: Color) -> Style {
    Style::default().fg(Color::Rgb(255, 255, 255)).bg(bg)
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
    identity: (usize, bool),
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let (index, footer_after) = identity;
    let text = if row.meta.is_some() || footer_after {
        match cache {
            Some(cache) => cache.borrow_mut().footer_source(&row.text),
            None => paragraph_before_footer(&row.text),
        }
    } else {
        &row.text
    };
    let mut out = Vec::new();
    if let Some(reasoning) = &row.reasoning {
        out.extend(reasoning_lines(reasoning, theme, width));
    }
    if !text.trim().is_empty() {
        out.push(Line::plain(""));
        out.extend(match cache {
            Some(cache) => cache
                .borrow_mut()
                .render((row.seq, index, 0), text, theme, width),
            None => markdown_block(text, theme, width),
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

fn footer_follows(rows: &[HistoryRow], index: usize) -> bool {
    let row = &rows[index];
    rows.get(index + 1).is_some_and(|next| {
        next.role == "assistant"
            && next.seq == row.seq
            && next.message_id == row.message_id
            && next.text.is_empty()
            && next.reasoning.is_none()
            && next.meta.is_some()
    })
}

/// Only normalize the structural LF of a completed terminal paragraph directly
/// before its footer. TextPart uses content.trim() (message-parts.tsx:163), and
/// SessionRowView supplies the footer's marginTop=1 (index.tsx:1435). Internal
/// paragraph separators, explicit trailing blanks, headings, fences and lists
/// retain their existing source. Both cache/index and full render use this same
/// source slice, so their revisions and measured heights cannot disagree.
fn paragraph_before_footer(text: &str) -> &str {
    if !text.ends_with('\n') || text.ends_with("\n\n") {
        return text;
    }
    let mut depth = 0usize;
    let mut paragraph = false;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    for event in Parser::new_ext(text, options) {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(end) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    paragraph = end == TagEnd::Paragraph;
                }
            }
            _ => {}
        }
    }
    if paragraph {
        &text[..text.len() - 1]
    } else {
        text
    }
}

/// Reasoning header: hide mode is `SessionReasoningGroupView`
/// (`routes/session/index.tsx:1785-1815`), show mode is `ReasoningPart`
/// (`message-parts.tsx:49-65,98-145`).
///
/// - running: static spinner fallback `⋯ Thinking` / `⋯ Thinking: <title>`;
/// - completed hide: `+ Thought: <title> · <duration>` when closed,
///   `- Thought · <duration>` when opened; show mode omits the icon.
fn reasoning_line(reasoning: &ReasoningBlock, theme: &Theme) -> Line {
    let content = reasoning_content(reasoning);
    let title = if !reasoning.toggleable || (reasoning.expanded && !reasoning.running) {
        ""
    } else {
        reasoning_title(&content)
    };
    let mut spans = vec![Span::plain(" ".repeat(MESSAGE_PADDING))];
    if !reasoning.toggleable {
        spans.push(Span::styled(
            "┃",
            Style::default().fg(theme.decrease(theme.background())),
        ));
        spans.push(Span::plain(" "));
    }
    let fg = if reasoning.running {
        if reasoning.toggleable {
            theme.text()
        } else {
            theme.fade(theme.warning(), 0.6)
        }
    } else if reasoning.toggleable == reasoning.expanded {
        theme.warning()
    } else if reasoning.toggleable {
        collapsed_thought_color(theme)
    } else {
        theme.fade(theme.warning(), 0.6)
    };
    let style = Style::default().fg(fg);
    if reasoning.running {
        let mut text = String::from("⋯ Thinking");
        if !title.is_empty() {
            text.push_str(": ");
            text.push_str(title);
        }
        spans.push(Span::styled(text, style));
        return Line::new(spans);
    }
    if reasoning.toggleable {
        spans.push(Span::styled(
            if reasoning.expanded { "-" } else { "+" },
            style,
        ));
        spans.push(Span::plain(" ".repeat(INLINE_ICON_WIDTH - 1)));
    }
    let mut text = String::from("Thought");
    let duration = reasoning
        .duration_ms
        .filter(|ms| *ms > 0)
        .map(Locale::duration);
    if !title.is_empty() || (duration.is_some() && !reasoning.toggleable) {
        text.push_str(": ");
    }
    if !title.is_empty() {
        text.push_str(title);
    }
    if let Some(duration) = duration {
        if reasoning.toggleable || !title.is_empty() {
            text.push_str(" · ");
        }
        text.push_str(&duration);
    }
    spans.push(Span::styled(text, style));
    Line::new(spans)
}

/// OpenTUI composites the header's RGBA(0.6) against the root background at
/// full precision. Theme::fade quantizes alpha to a byte first, which loses a
/// channel on this particular header in the pinned dark palette.
fn collapsed_thought_color(theme: &Theme) -> Color {
    let (Color::Rgb(r, g, b), Color::Rgb(br, bg, bb)) = (theme.warning(), theme.background())
    else {
        return theme.warning();
    };
    let tint = |source: u8, background: u8| {
        (f32::from(source) * 0.6 + f32::from(background) * 0.4).round() as u8
    };
    Color::Rgb(tint(r, br), tint(g, bg), tint(b, bb))
}

/// Verify the already-painted, completed hide-mode header and its clickable
/// cell. Match the generated span structure and styles, not untrusted text
/// that happens to spell "+ Thought" in Markdown or a tool result.
pub(crate) fn collapsed_thought_header(line: &Line, theme: &Theme, x: usize) -> bool {
    let spans = line.spans();
    let faded = collapsed_thought_color(theme);
    if line.style() != Style::default() || spans.len() != 4 {
        return false;
    }
    let label = spans[3].content();
    let text = line.plain_text();
    spans[0].content() == " ".repeat(MESSAGE_PADDING)
        && spans[0].style() == Style::default()
        && spans[1].content() == "+"
        && spans[1].style() == Style::default().fg(faded)
        && spans[2].content() == " ".repeat(INLINE_ICON_WIDTH - 1)
        && spans[2].style() == Style::default()
        && spans[3].style() == Style::default().fg(faded)
        && !label.is_empty()
        && (label.starts_with("Thought") || "Thought".starts_with(label))
        && x >= MESSAGE_PADDING
        && x < UnicodeWidthStr::width(text.trim_end())
}

/// Recolor only the header's warning spans; keep icon gap, text and clipping.
pub(crate) fn hover_collapsed_thought(line: &Line, theme: &Theme) -> Line {
    let faded = collapsed_thought_color(theme);
    Line::new(
        line.spans()
            .iter()
            .map(|span| {
                let style = if span.style().fg == Some(faded) {
                    span.style().fg(theme.warning())
                } else {
                    span.style()
                };
                Span::styled(span.content(), style)
            })
            .collect(),
    )
    .with_style(line.style())
}

fn reasoning_lines(reasoning: &ReasoningBlock, theme: &Theme, width: u16) -> Vec<Line> {
    // The owner exposes individual parts, not the pinned group's aggregate
    // refs/completion; one native row represents one part, never fake steps.
    let content = reasoning_content(reasoning);
    if content.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        Line::plain(""),
        clipped_reasoning_header(reasoning, theme, width),
    ];
    if reasoning.expanded {
        #[cfg(test)]
        REASONING_BODY_RENDERS.set(REASONING_BODY_RENDERS.get() + 1);
        lines.push(Line::plain(""));
        let mut end = content.len().min(LIVE_MARKDOWN_BYTES);
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        // Grouped hide: paddingLeft=3, border and paddingLeft=1
        // (`index.tsx:1815-1851`). Show: ReasoningPart also has a border
        // and paddingLeft=1 (`message-parts.tsx:68-85`).
        let inset = MESSAGE_PADDING + 2;
        let inner = width.saturating_sub(inset as u16).max(1);
        let mut body = markdown_at_width(&content[..end], theme, inner as usize);
        // A single public reasoning part is a bounded view of the owner's
        // durable text. Do not turn expansion into an unbounded render.
        let omitted = end < content.len() || body.len() > MAX_MARKDOWN_ROWS;
        if body.len() > MAX_MARKDOWN_ROWS {
            body.truncate(MAX_MARKDOWN_ROWS);
        }
        if omitted {
            body.push(Line::styled(
                "… [reasoning preview limited]",
                Style::default().fg(theme.text_muted()),
            ));
        }
        let border = Style::default().fg(theme.decrease(if reasoning.toggleable {
            theme.background_raised()
        } else {
            theme.background()
        }));
        for line in body {
            let mut spans = vec![
                Span::plain(" ".repeat(MESSAGE_PADDING)),
                Span::styled("┃", border),
                Span::plain(" ".repeat(inset - MESSAGE_PADDING - 1)),
            ];
            // thinkingSyntax replaces every Markdown token foreground with
            // text.muted; retain the native parser's layout/modifiers.
            spans.extend(
                line.spans()
                    .iter()
                    .map(|span| Span::styled(span.content(), span.style().fg(theme.text_muted()))),
            );
            lines.push(clipped_line(sanitize_line(Line::new(spans)), width));
        }
    }
    lines
}

fn reasoning_content(reasoning: &ReasoningBlock) -> Cow<'_, str> {
    if reasoning.text.contains("[REDACTED]") {
        Cow::Owned(reasoning.text.replace("[REDACTED]", "").trim().to_string())
    } else {
        Cow::Borrowed(reasoning.text.trim())
    }
}

/// OpenTUI's completed header uses `wrapMode="none"`: retain only complete
/// graphemes that fit the painted row, preserving span styles and cell widths.
fn clipped_reasoning_header(reasoning: &ReasoningBlock, theme: &Theme, width: u16) -> Line {
    clipped_line(sanitize_line(reasoning_line(reasoning, theme)), width)
}

fn shell_notice_block(row: &HistoryRow, index: usize, theme: &Theme, width: u16) -> Vec<Line> {
    let Some(notice) = &row.shell_notice else {
        return Vec::new();
    };
    let (heading, color) = match notice.state.as_str() {
        "completed" => ("↳ Shell finished", theme.info()),
        "failed" => ("! Shell failed", theme.error()),
        "cancelled" => ("! Shell cancelled", theme.warning()),
        "timed_out" => ("! Shell timed out", theme.warning()),
        "unknown" => ("! Shell unknown", theme.info()),
        _ => return Vec::new(),
    };
    let mut command = String::with_capacity(notice.command.len());
    for word in notice.command.split_whitespace() {
        if !command.is_empty() {
            command.push(' ');
        }
        command.push_str(word);
    }
    let line = clipped_line(
        sanitize_line(Line::new(vec![
            Span::plain(" ".repeat(MESSAGE_PADDING)),
            Span::styled(heading, Style::default().fg(color)),
            Span::styled(
                format!(" · {command}"),
                Style::default().fg(theme.text_muted()),
            ),
        ])),
        width,
    );
    if index > 0 {
        vec![Line::plain(""), line]
    } else {
        vec![line]
    }
}

fn child_notice_block(row: &HistoryRow, index: usize, theme: &Theme, width: u16) -> Vec<Line> {
    let Some(line) = row
        .child_notice
        .as_deref()
        .and_then(|job| child_notice_line(job, theme, width, false))
    else {
        return Vec::new();
    };
    if index > 0 {
        vec![Line::plain(""), line]
    } else {
        vec![line]
    }
}

fn child_notice_line(
    job: &oc_core::queries::ChildJob,
    theme: &Theme,
    width: u16,
    hover: bool,
) -> Option<Line> {
    let actor = Locale::titlecase(if job.agent.is_empty() {
        "Subagent"
    } else {
        &job.agent
    });
    let (label, color) = match job.state {
        oc_core::queries::ChildState::Completed => (format!("↳ {actor} finished"), theme.info()),
        oc_core::queries::ChildState::Error => (format!("! {actor} failed"), theme.error()),
        oc_core::queries::ChildState::Cancelled => {
            (format!("! {actor} cancelled"), theme.warning())
        }
        oc_core::queries::ChildState::Unknown => (format!("! {actor} unknown"), theme.info()),
        oc_core::queries::ChildState::Admitted | oc_core::queries::ChildState::Running => {
            return None;
        }
    };
    let color = if hover
        && !matches!(
            job.state,
            oc_core::queries::ChildState::Error | oc_core::queries::ChildState::Cancelled
        ) {
        theme.text()
    } else {
        color
    };
    Some(clipped_line(
        sanitize_line(Line::new(vec![
            Span::plain("   "),
            Span::styled(label, Style::default().fg(color)),
            Span::styled(
                format!(" · {}", job.description),
                Style::default().fg(theme.text_muted()),
            ),
        ])),
        width,
    ))
}

fn clipped_line(header: Line, width: u16) -> Line {
    if width == 0 {
        return header;
    }
    let mut remaining = width as usize;
    let mut spans = Vec::new();
    for span in header.spans() {
        let mut clipped = String::new();
        for glyph in span.content().graphemes(true) {
            let cells = UnicodeWidthStr::width(glyph);
            if cells > remaining {
                spans.push(Span::styled(clipped, span.style()));
                return Line::new(spans);
            }
            clipped.push_str(glyph);
            remaining -= cells;
        }
        spans.push(Span::styled(clipped, span.style()));
    }
    Line::new(spans)
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
pub(crate) fn markdown_block(text: &str, theme: &Theme, width: u16) -> Vec<Line> {
    markdown_block_with_widths(text, theme, width, None)
}

fn markdown_block_with_widths(
    text: &str,
    theme: &Theme,
    width: u16,
    columns: Option<&[usize]>,
) -> Vec<Line> {
    markdown_block_with_spacing(text, theme, width, columns, false)
}

/// CompactionMessage's streaming top-level Markdown renderer has a blank row
/// after headings (pinned session/index.tsx:2133–2144; paired manual 12 rows18–28).
/// Keep this mode local to compaction; ordinary message pagination is unchanged.
pub(crate) fn compaction_markdown_block(text: &str, theme: &Theme, width: u16) -> Vec<Line> {
    markdown_block_with_spacing(text, theme, width, None, true)
}

fn markdown_block_with_spacing(
    text: &str,
    theme: &Theme,
    width: u16,
    columns: Option<&[usize]>,
    compaction_style: bool,
) -> Vec<Line> {
    let inner = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(MESSAGE_PADDING).max(1)
    };
    let pad = Span::plain(" ".repeat(MESSAGE_PADDING));
    let mut out = Vec::new();
    for line in markdown_at_width_with_spacing(text, theme, inner, columns, compaction_style) {
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
    // Pinned session/index.tsx:1974–1975 gates only the throughput field.
    if meta.session_tps.unwrap_or(true)
        && let Some(tps) = tokens_per_second(meta)
    {
        push_field(Span::styled(format!("{tps:.1} tok/s"), muted), &mut spans);
    }
    if meta.interrupted {
        // The application records `cancelled` as the durable outcome, while
        // the pinned TUI labels an interrupted assistant row `interrupted`.
        push_field(Span::styled("interrupted", muted), &mut spans);
    } else if let Some(status) = meta
        .status
        .as_deref()
        .filter(|s| *s != "completed" && *s != "started")
    {
        push_field(Span::styled(status, muted), &mut spans);
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
    let wrapped = styled::wrap_source_space_line_limited(&line, width, limit);
    if !list_item || wrapped.len() <= 1 {
        return wrapped;
    }
    let mut rows = Vec::with_capacity(wrapped.len());
    for (index, line) in wrapped.into_iter().enumerate() {
        if index == 0 {
            rows.push(line);
        } else {
            for continuation in styled::wrap_source_space_line_limited(
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
    markdown_at_width_with_spacing(text, theme, width, columns, false)
}

fn markdown_at_width_with_spacing(
    text: &str,
    theme: &Theme,
    width: usize,
    columns: Option<&[usize]>,
    compaction_style: bool,
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
    let mut after_heading = false;
    let mut heading_font = Modifier::empty();
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
            if after_heading && out.last().is_some_and(|line| !line.plain_text().is_empty()) {
                out.push(Line::plain(""));
            }
            after_heading = false;
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
                                // Streaming Markdown resolves both marker forms
                                // through markup.list (theme/v1.ts:362–367).
                                (
                                    marker,
                                    if compaction_style {
                                        MarkdownToken::ListItem
                                    } else {
                                        MarkdownToken::ListEnumeration
                                    },
                                )
                            }
                            None => ("- ".into(), MarkdownToken::ListItem),
                        };
                        spans.push(Span::styled(
                            marker,
                            Style::default().fg(theme.markdown(token)),
                        ));
                    }
                    Tag::Heading { level, .. } => {
                        stack.push(MarkdownToken::Heading);
                        // Pinned theme/v1.ts:297–346; scoped to the streaming
                        // compaction renderer rather than unrelated messages.
                        if compaction_style {
                            heading_font = Modifier::BOLD;
                            if *level == pulldown_cmark::HeadingLevel::H1 {
                                heading_font |= Modifier::UNDERLINED;
                            }
                        }
                    }
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
                                    out.extend(styled::wrap_code_line_limited(
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
                            after_heading = compaction_style && depth == 1;
                            heading_font = Modifier::empty();
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
                    let mut style = Style::default()
                        .fg(theme.markdown(token))
                        .add_modifier(heading_font);
                    if stack.contains(&MarkdownToken::Strong) {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(safe_text(&value), style));
                }
            }
            Event::Code(value) => spans.push(Span::styled(
                safe_text(&value),
                Style::default()
                    .fg(theme.markdown(MarkdownToken::Code))
                    .add_modifier(heading_font),
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
    let widths = (0..count)
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
    let mut widths = fit_table_widths(widths, available);
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

pub(crate) fn highlight_patch(line: &str, path: &str, theme: &Theme, base: Style) -> Vec<Span> {
    let extension = path.rsplit('.').next();
    highlight(line, Language::from_info(extension), theme, base)
        .into_iter()
        .map(|span| Span::styled(span.content(), base.patch(span.style())))
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
mod tests;
