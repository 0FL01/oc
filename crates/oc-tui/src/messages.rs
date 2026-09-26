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
    #[cfg(test)]
    parses: usize,
    #[cfg(test)]
    parsed_bytes: usize,
}

impl MarkdownCache {
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
        } else {
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
    }
    out
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
        || card.output_truncated
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
    index: usize,
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
    cache: Option<&RefCell<MarkdownCache>>,
) -> Vec<Line> {
    let lines = match row.role.as_str() {
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
            source_chunks(&row.text, (inner * 128).min(4096), |chunk, _| {
                let mut lines = Vec::new();
                let body = Style::default().fg(theme.text()).bg(bg);
                for raw in chunk.strip_suffix('\n').unwrap_or(chunk).split('\n') {
                    let line = Line::new(vec![Span::styled(safe_text(raw), body)]);
                    for wrapped in styled::wrap_line_limited(&line, inner, MAX_MARKDOWN_ROWS) {
                        let mut spans = vec![
                            Span::styled("┃", border),
                            Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
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
                        Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
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
            (index, live),
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
    mut emit: impl FnMut(usize, bool, &mut dyn FnMut() -> Vec<Line>),
) {
    let (index, live) = identity;
    let (width, terminal_width) = widths;
    if let Some(reasoning) = &row.reasoning {
        let lines = reasoning_lines(reasoning, theme, width);
        emit(lines.len(), false, &mut || lines.clone());
    }
    if !row.text.trim().is_empty() {
        emit(1, false, &mut || vec![Line::plain("")]);
        if live && row.text.len() > LIVE_MARKDOWN_BYTES {
            let mut end = LIVE_MARKDOWN_BYTES;
            while !row.text.is_char_boundary(end) {
                end -= 1;
            }
            let preview = &row.text[..end];
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
            let pages = cache.borrow_mut().pages((row.seq, index), &row.text, width);
            for (number, page) in pages.into_iter().enumerate() {
                let height = page.height;
                emit(height, false, &mut || {
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
        emit(2, true, &mut || {
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

/// Map one visible text cell to the first operation of its exploration group.
/// Uses the same index, wrapping and sticky scroll slice as the painted frame;
/// only viewport rows are materialized, even for long histories.
pub(crate) fn exploration_header_at(
    rows: &[HistoryRow],
    theme: &Theme,
    widths: (u16, u16),
    viewport: (usize, usize, Option<usize>),
    agent_color: impl Fn(Option<&str>) -> Color,
    cache: &RefCell<MarkdownCache>,
    hit: (&impl Fn(&str) -> bool, (usize, usize)),
) -> Option<String> {
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
        TranscriptHit::Exploration(op) => Some(op),
        TranscriptHit::Reasoning(_) => None,
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
        TranscriptHit::Exploration(_) => None,
    })
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
}

/// Only the inner box receives the hover fill; the agent-colored left border
/// and chip-specific surfaces remain as painted by upstream.
pub(crate) fn hover_user_content(line: &Line, theme: &Theme) -> Line {
    let base = theme.user_message_background();
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

enum TranscriptHit {
    Exploration(String),
    Reasoning(ReasoningIdentity),
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
    let mut total = 1usize;
    let mut grouped_until = 0;
    for (index, row) in rows.iter().enumerate() {
        if index < grouped_until {
            continue;
        }
        if let Some(group) = reasoning_group(rows, index) {
            grouped_until = index + group.members.len();
            visit_reasoning_group(&group, theme, width, index, Some(cache), |height, _| {
                total += height
            });
            continue;
        }
        if let Some(group) = exploration_entry(rows, index, theme) {
            let is_expanded = !group.is_empty()
                && (options.expanded)(&row.tool.as_ref().expect("group has tool").op);
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
        } else if row.role == "assistant" && width > 0 {
            visit_assistant_indexed(
                row,
                (index, live_row == Some(index)),
                theme,
                (width, terminal_width),
                &agent_color,
                cache,
                |height, _, _| total += height,
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
                            hit = row
                                .tool
                                .as_ref()
                                .map(|card| TranscriptHit::Exploration(card.op.clone()));
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
                (index, live_row == Some(index)),
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
            let mut margin = row.role == "user" && index > 0;
            visit_row_blocks(
                row,
                (index, false),
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
                    if row.role == "user" && !margin {
                        for slot in user_targets.iter_mut().take(visible.len()).skip(first) {
                            *slot = Some(UserMessageTarget {
                                message_id: row.message_id.clone(),
                                seq: row.seq,
                                ordinal: index,
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
                    Span::styled(" ".repeat(USER_PADDING), user_padding(bg)),
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
                    let mut style = Style::default().fg(theme.markdown(token));
                    if stack.contains(&MarkdownToken::Strong) {
                        style = style.add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(safe_text(&value), style));
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
            rowid: 1,
            op: name.to_string(),
            name: name.to_string(),
            state: state.to_string(),
            input: Some(input.to_string()),
            output: Some("fixture result".to_string()),
            output_bytes: 14,
            output_truncated: truncated,
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
                assert!(text.contains("[output preview truncated; full result retained]"));
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
        assert!(text.contains("[output preview truncated; full result retained]"));
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
            rowid: 1,
            op: "read".into(),
            name: "read".into(),
            state: "error".into(),
            input: Some(serde_json::json!({"path": "+ Thought"}).to_string()),
            output: Some("+ Thought".into()),
            output_bytes: 9,
            output_truncated: false,
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
        let fixture = include_str!("../../../tui-recovery/fixtures/transcript.md");
        let table = &fixture
            [fixture.find("| Инструмент").unwrap()..fixture.find("Через Code Mode").unwrap()];
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

    #[test]
    fn fixture_prose_wrap_paints_source_separator_in_full_and_indexed_rows() {
        let fixture = include_str!("../../../tui-recovery/fixtures/transcript.md");
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
}
