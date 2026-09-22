//! Upstream v2.0.12 message rendering: user blocks with chips, assistant
//! markdown, collapsed reasoning and the assistant footer.
//!
//! All geometry and colors cite the upstream sources at tag `v2.0.12`
//! (`packages/tui/src/**`). Anything upstream renders that this subset cannot
//! parse keeps its source text instead of being dropped; the deviations are
//! listed in the module docs of the individual renderers and in
//! `evidence/tui/upstream-inventory.md` §7.
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

use ratatui::style::{Color, Modifier, Style};

use crate::history::HistoryRow;
use crate::styled::{self, Line, Span};
use crate::theme::{MarkdownToken, SyntaxToken, Theme};

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
    let mut out = Vec::new();
    for row in rows {
        match row.role.as_str() {
            "user" => out.extend(user_block(row, theme, width, &agent_color)),
            "assistant" => out.extend(assistant_block(
                row,
                theme,
                width,
                terminal_width,
                &agent_color,
            )),
            "tool" => {
                if let Some(card) = &row.tool {
                    // Every upstream row has `marginTop=1`
                    // (`routes/session/index.tsx:1435`).
                    out.push(Line::plain(""));
                    out.extend(crate::tools::tool_block(card, theme, width));
                } else {
                    out.extend(notice_block(row));
                }
            }
            _ => out.extend(notice_block(row)),
        }
    }
    out
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
        .fg(agent_color(row.agent.as_deref()))
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
    theme: &Theme,
    width: u16,
    terminal_width: u16,
    agent_color: &impl Fn(Option<&str>) -> Color,
) -> Vec<Line> {
    let mut out = Vec::new();
    if let Some(reasoning) = &row.reasoning {
        out.push(Line::plain(""));
        out.push(reasoning_line(reasoning, theme));
    }
    if !row.text.trim().is_empty() {
        out.push(Line::plain(""));
        out.extend(markdown_block(&row.text, theme, width));
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
    let title = reasoning_title(&reasoning.text);
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
        format!("{:<width$}", "+", width = INLINE_ICON_WIDTH),
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
    let inner = if width == 0 {
        usize::MAX
    } else {
        (width as usize).saturating_sub(MESSAGE_PADDING).max(1)
    };
    let pad = Span::plain(" ".repeat(MESSAGE_PADDING));
    let mut out = Vec::new();
    for line in styled::wrap_lines(&markdown(text, theme), inner) {
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
            agent_color(Some(agent))
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

/// Markdown subset renderer (`message-parts.tsx:156-171`, default
/// `markdownMode === "rendered"` conceals markers, `routes/session/index.tsx:228`).
///
/// Supported: ATX headings, unordered/ordered list items, blockquotes,
/// fenced code blocks (``` / ~~~ with an info string), horizontal rules,
/// paragraphs, and inline code/bold/italic/links. Everything else keeps its
/// source text (tables render as paragraph rows, an unclosed fence streams as
/// code); nothing is dropped.
pub fn markdown(text: &str, theme: &Theme) -> Vec<Line> {
    let mut out = Vec::new();
    let mut lines = text.split('\n');
    while let Some(raw) = lines.next() {
        let line = raw.trim_end_matches('\r');
        if let Some((fence, lang)) = open_fence(line) {
            let mut body = Vec::new();
            for next in lines.by_ref() {
                let next = next.trim_end_matches('\r');
                if is_fence_close(next, fence) {
                    break;
                }
                body.push(next.to_string());
            }
            out.extend(code_lines(lang, &body, theme));
            continue;
        }
        if line.trim().is_empty() {
            out.push(Line::plain(""));
            continue;
        }
        let trimmed = line.trim_start();
        if let Some(level) = heading_level(trimmed) {
            let content = trimmed[level..].trim_start();
            out.push(Line::new(inline(content, theme, MarkdownToken::Heading)));
            continue;
        }
        if let Some(rest) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            out.push(list_line_with_marker("- ", rest, theme));
            continue;
        }
        if let Some((marker, rest)) = ordered_marker(trimmed) {
            out.push(list_line_with_marker(marker, rest, theme));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            out.push(Line::new(inline(rest, theme, MarkdownToken::BlockQuote)));
            continue;
        }
        if is_horizontal_rule(trimmed) {
            out.push(Line::styled(
                trimmed,
                Style::default().fg(theme.markdown(MarkdownToken::HorizontalRule)),
            ));
            continue;
        }
        out.push(Line::new(inline(trimmed, theme, MarkdownToken::Text)));
    }
    out
}

/// Fence opener: at least three backticks or tildes plus an optional info
/// string; returns the fence char and the (lowercased later) language token.
fn open_fence(line: &str) -> Option<(char, Option<&str>)> {
    let trimmed = line.trim_start();
    let fence = trimmed.chars().next()?;
    if fence != '`' && fence != '~' {
        return None;
    }
    let run = trimmed.chars().take_while(|ch| *ch == fence).count();
    if run < 3 {
        return None;
    }
    let info = trimmed[run..].trim();
    let lang = info
        .split_whitespace()
        .next()
        .filter(|item| !item.is_empty());
    Some((fence, lang))
}

/// Fence closer: same fence char, at least three, nothing else on the row.
fn is_fence_close(line: &str, fence: char) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= 3 && trimmed.chars().all(|ch| ch == fence)
}

/// ATX heading level; returns the byte length of the `#… ` marker.
fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let bytes = hashes;
    let rest = line.get(bytes..)?;
    if rest.starts_with(' ') || rest.is_empty() {
        Some(bytes)
    } else {
        None
    }
}

fn ordered_marker(line: &str) -> Option<(&str, &str)> {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = &line[digits..];
    let punctuation = rest.chars().next()?;
    if punctuation != '.' && punctuation != ')' {
        return None;
    }
    let after = rest[punctuation.len_utf8()..].strip_prefix(' ')?;
    Some((&line[..digits + punctuation.len_utf8() + 1], after))
}

fn is_horizontal_rule(line: &str) -> bool {
    let trimmed = line.trim();
    let marker = trimmed.chars().next().unwrap_or(' ');
    (marker == '-' || marker == '*' || marker == '_')
        && trimmed.chars().count() >= 3
        && trimmed.chars().all(|ch| ch == marker || ch == ' ')
        && trimmed.chars().filter(|ch| *ch == marker).count() >= 3
}

/// List item: the source marker keeps the `listItem`/`listEnumeration` token
/// color (the concealed bullet glyph of `@opentui/core` is not in this
/// repository, so the real source marker is rendered instead of an invented one).
fn list_line_with_marker(marker: &str, rest: &str, theme: &Theme) -> Line {
    let ordered = marker.chars().next().is_some_and(|ch| ch.is_ascii_digit());
    let token = if ordered {
        MarkdownToken::ListEnumeration
    } else {
        MarkdownToken::ListItem
    };
    let mut spans = vec![Span::styled(
        marker,
        Style::default().fg(theme.markdown(token)),
    )];
    spans.extend(inline(rest, theme, MarkdownToken::Text));
    Line::new(spans)
}

/// Inline subset: `` `code` ``, `**strong**`, `*emphasis*` / `_emphasis_`,
/// `[text](url)`. Unsupported markers stay literal.
fn inline(text: &str, theme: &Theme, base: MarkdownToken) -> Vec<Span> {
    let plain_style = Style::default().fg(theme.markdown(base));
    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '`'
            && let Some(end) = find_char(&chars, index + 1, '`')
            && end > index + 1
        {
            flush(&mut spans, &mut plain, plain_style);
            spans.push(Span::styled(
                chars[index + 1..end].iter().collect::<String>(),
                Style::default().fg(theme.markdown(MarkdownToken::Code)),
            ));
            index = end + 1;
            continue;
        }
        if ch == '*' && chars.get(index + 1) == Some(&'*') {
            if let Some(end) = find_double(&chars, index + 2)
                && end > index + 2
            {
                flush(&mut spans, &mut plain, plain_style);
                spans.push(Span::styled(
                    chars[index + 2..end].iter().collect::<String>(),
                    Style::default().fg(theme.markdown(MarkdownToken::Strong)),
                ));
                index = end + 2;
                continue;
            }
        } else if ch == '*' || ch == '_' {
            if let Some(end) = find_char(&chars, index + 1, ch)
                && end > index + 1
            {
                flush(&mut spans, &mut plain, plain_style);
                spans.push(Span::styled(
                    chars[index + 1..end].iter().collect::<String>(),
                    Style::default().fg(theme.markdown(MarkdownToken::Emphasis)),
                ));
                index = end + 1;
                continue;
            }
        } else if ch == '['
            && let Some((label, end)) = parse_link(&chars, index)
        {
            flush(&mut spans, &mut plain, plain_style);
            spans.push(Span::styled(
                label,
                Style::default().fg(theme.markdown(MarkdownToken::LinkText)),
            ));
            index = end;
            continue;
        }
        plain.push(ch);
        index += 1;
    }
    flush(&mut spans, &mut plain, plain_style);
    spans
}

fn flush(spans: &mut Vec<Span>, plain: &mut String, style: Style) {
    if !plain.is_empty() {
        spans.push(Span::styled(std::mem::take(plain), style));
    }
}

fn find_char(chars: &[char], from: usize, needle: char) -> Option<usize> {
    (from..chars.len()).find(|index| chars[*index] == needle)
}

fn find_double(chars: &[char], from: usize) -> Option<usize> {
    (from..chars.len().saturating_sub(1))
        .find(|index| chars[*index] == '*' && chars[*index + 1] == '*')
}

/// `[label](url)` starting at `start`; returns the label and the index just
/// past the closing `)`.
fn parse_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    let label_end = find_char(chars, start + 1, ']')?;
    if label_end == start + 1 || chars.get(label_end + 1) != Some(&'(') {
        return None;
    }
    let url_end = find_char(chars, label_end + 2, ')')?;
    let label: String = chars[start + 1..label_end].iter().collect();
    Some((label, url_end + 1))
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

        // Unsupported inline markers keep their source text.
        let lines = markdown("a ~~strike~~ b `unclosed and ![alt](u)", theme);
        assert_eq!(plain(&lines[0]), "a ~~strike~~ b `unclosed and !alt");
        // A table row stays a paragraph (source text preserved).
        let lines = markdown("| a | b |\n|---|---|\n| 1 | 2 |", theme);
        assert_eq!(plain(&lines[0]), "| a | b |");
        assert_eq!(plain(&lines[1]), "|---|---|");
        assert_eq!(plain(&lines[2]), "| 1 | 2 |");
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
