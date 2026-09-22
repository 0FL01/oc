//! Upstream v2.0.12 shell: root regions, session area, prompt box, status
//! row, prompt footer and devtools bar.
//!
//! Geometry is frozen in [`crate::layout`]; this module only turns state into
//! widgets. Slots whose data is not in our DTOs are rendered empty, never
//! invented:
//!
//! - the tab title uses the upstream fallback `Untitled session`
//!   (`component/session-tabs.tsx:1561`): session titles are not in our DTOs;
//! - the prompt left border keeps `border.base`; upstream tints it with the
//!   active agent color (`component/prompt/index.tsx:1576`), which our DTOs
//!   do not carry;
//! - the prompt footer left slot shows the interrupt hint while a turn
//!   streams, otherwise the DCP notice, otherwise nothing: the current
//!   Location label upstream renders there (`component/prompt/index.tsx:1920-1935`)
//!   is not in the view state;
//! - the status row shows `Jump to latest ↓` when the stream is detached
//!   (`routes/session/index.tsx:1331-1350`); `Loading session history…` needs
//!   a paging-in-flight flag the view does not have;
//! - the prompt metadata row shows agent/model/provider/variant from the
//!   catalog snapshot; the `auto` permission marker and the agent color are
//!   not in our DTOs;
//! - devtools items show the upstream labels (`component/devtools-bar.tsx:245-445`);
//!   the Server connection icon has no client-connection state (in-process
//!   runtime) and keeps its cell empty, while the UI runtime icon is honest
//!   for "no samples yet" (`runtimeStatus([]) === "normal"`).
//!
//! Transient notes render as the upstream toast (`ui/toast.tsx:48-50`), the
//! only overlay this iteration adds; picker panes stay the inline port block
//! until iteration 4 replaces them with upstream dialogs.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph},
};

use crate::app::{TuiState, TuiStatus};
use crate::layout;
use crate::theme::{Theme, tint};
use crate::views::panel_lines;

/// Prompt left border: upstream `SplitBorder` vertical `┃`
/// (`component/prompt/index.tsx:1653-1656`).
const PROMPT_BORDER: border::Set<'static> = border::Set {
    vertical_left: "┃",
    ..border::PLAIN
};

/// Upstream fallback when a session has no title (`component/session-tabs.tsx:1561`).
pub const UNTITLED_SESSION: &str = "Untitled session";
/// Jump-to-bottom affordance (`routes/session/index.tsx:1346`).
pub const JUMP_TO_LATEST: &str = "Jump to latest ↓";
/// Interrupt hint while a turn streams (`component/prompt/index.tsx:139-142`).
pub const ESC_INTERRUPT: (&str, &str) = ("esc ", "interrupt");
/// Prompt footer shortcuts (`feature-plugins/prompt/footer.tsx:89-104`).
pub const AGENTS_HINT: (&str, &str) = ("shift+tab ", "agents");
/// Prompt footer shortcuts (`feature-plugins/prompt/footer.tsx:89-104`).
pub const COMMANDS_HINT: (&str, &str) = ("ctrl+p ", "commands");
/// Toast max width (`ui/toast.tsx:50`: `min(60, width - 6)`).
pub const TOAST_MAX_WIDTH: u16 = 60;
/// Toast right margin (`ui/toast.tsx:49`: `right={2}`).
pub const TOAST_RIGHT_MARGIN: u16 = 2;

/// Render one whole frame: root background, tab strip, session area,
/// devtools bar, then the toast overlay.
pub fn render(frame: &mut Frame<'_>, state: &TuiState) {
    let theme = Theme::dark();
    let area = frame.area();
    // Upstream paints the whole frame with `background.base` (`app.tsx:1310-1314`).
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background())),
        area,
    );
    let regions = layout::shell_regions(area);
    render_tabs(frame, theme, regions.tabs);
    render_session(frame, state, theme, regions.session);
    render_devtools(frame, theme, regions.devtools);
    render_toast(frame, state, theme, area);
}

/// Single active tab in the horizontal strip
/// (`component/session-tabs.tsx:1506-1508`, `context/session-tabs-model.ts:33-35`).
fn tab_line(theme: &Theme, available: u16) -> Line<'static> {
    let tab_bg = theme.decrease(theme.background_panel());
    let tab_width = layout::single_tab_width(available);
    // Indicator cell: `numberWidth + 1` with the label right-aligned and one
    // padding cell (`component/session-tabs.tsx:1671-1682`); selected number
    // color is `tint(text.base, tabBackground, 0.25)` (`:1606-1618`).
    let number = tint(theme.text(), tab_bg, 0.25);
    let mut spans = vec![
        Span::styled(" 1 ", Style::default().fg(number).bg(tab_bg)),
        Span::styled(
            UNTITLED_SESSION,
            Style::default().fg(theme.text()).bg(tab_bg),
        ),
    ];
    let used = 3 + UNTITLED_SESSION.chars().count() as u16;
    if tab_width > used {
        spans.push(Span::styled(
            " ".repeat((tab_width - used) as usize),
            Style::default().bg(tab_bg),
        ));
    }
    Line::from(spans)
}

fn render_tabs(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let strip = Rect {
        width: layout::single_tab_width(area.width),
        ..area
    };
    frame.render_widget(Paragraph::new(tab_line(theme, area.width)), strip);
}

fn render_session(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    let panel = panel_lines(state);
    let panel_height = if panel.is_empty() {
        0
    } else {
        ((panel.len() + 2) as u16).min(12)
    };
    let regions = layout::session_regions(area, panel_height);
    render_transcript(frame, state, regions.transcript);
    if regions.panel.height > 0 {
        // Port extension until iteration 4: the picker panes stay an inline
        // bordered block between the transcript and the status row.
        let widget = Paragraph::new(crate::styled::Lines::from(panel).into_text()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_active()))
                .title(Line::styled(
                    "panel",
                    Style::default().fg(theme.text_muted()),
                )),
        );
        frame.render_widget(widget, regions.panel);
    }
    render_status(frame, state, theme, regions.status);
    render_prompt(
        frame,
        state,
        theme,
        regions.prompt,
        regions.underline,
        area.width,
    );
    render_footer(frame, state, theme, regions.footer, area.width);
}

/// Sticky-bottom transcript: newest row on the transcript's last row, and
/// new rows keep it pinned while the stream is at the bottom
/// (`routes/session/index.tsx:1299-1300` `stickyScroll stickyStart="bottom"`).
fn render_transcript(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let visible = state.viewport();
    let rows = area.height as usize;
    let skip = visible.len().saturating_sub(rows);
    let pad = rows.saturating_sub(visible.len());
    let target = Rect {
        y: area.y.saturating_add(pad as u16),
        height: area.height.saturating_sub(pad as u16),
        ..area
    };
    frame.render_widget(
        Paragraph::new(crate::styled::Lines::from(visible).into_text()).scroll((skip as u16, 0)),
        target,
    );
}

/// Height-1 right-aligned status row (`routes/session/index.tsx:1331-1350`).
fn render_status(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    let Some(line) = status_line(state, theme) else {
        return;
    };
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Right), area);
}

fn status_line(state: &TuiState, theme: &Theme) -> Option<Line<'static>> {
    // `text.action.secondary.base` (`routes/session/index.tsx:1344-1348`).
    (state.scroll() > 0).then(|| {
        Line::styled(
            JUMP_TO_LATEST,
            Style::default().fg(theme.action_secondary()),
        )
    })
}

/// Prompt box: left `┃` border, interior on `decrease(background.raised.base)`,
/// `paddingTop=1`, textarea row, metadata rows, then the `╹`/`▀` underline
/// (`component/prompt/index.tsx:1650-1671,1856-1870`).
fn render_prompt(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &Theme,
    body: Rect,
    underline: Rect,
    terminal_width: u16,
) {
    let prompt_bg = theme.decrease(theme.background_panel());
    let border_style = Style::default().fg(theme.border());
    if body.height > 0 && body.width > 0 {
        frame.render_widget(
            Block::default()
                .borders(Borders::LEFT)
                .border_set(PROMPT_BORDER)
                .border_style(border_style),
            body,
        );
        let interior = Rect {
            x: body.x.saturating_add(1),
            width: body.width.saturating_sub(1),
            ..body
        };
        frame.render_widget(
            Block::default().style(Style::default().bg(prompt_bg)),
            interior,
        );
        let pad = layout::session_padding(terminal_width);
        let text_x = interior.x.saturating_add(pad);
        let text_width = interior.width.saturating_sub(pad.saturating_mul(2));
        let row = |index: u16| Rect {
            x: text_x,
            y: body.y.saturating_add(index),
            width: text_width,
            height: 1,
        };
        if body.height > 1 {
            frame.render_widget(
                Paragraph::new(state.input().to_string())
                    .style(Style::default().fg(theme.text()).bg(prompt_bg)),
                row(1),
            );
        }
        if body.height > 3
            && let Some(line) = metadata_line(state, theme, terminal_width)
        {
            frame.render_widget(
                Paragraph::new(line).style(Style::default().fg(theme.text())),
                row(3),
            );
        }
    }
    if underline.height > 0 && underline.width > 0 {
        let mut spans = vec![Span::styled("╹", border_style)];
        if underline.width > 1 {
            spans.push(Span::styled(
                "▀".repeat(underline.width.saturating_sub(1) as usize),
                Style::default().fg(prompt_bg),
            ));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)), underline);
    }
}

/// Prompt metadata row (`component/prompt/metadata.tsx:53-99`): `agent · model
/// provider · variant`; below 44 columns the agent and provider are dropped.
fn metadata_line(state: &TuiState, theme: &Theme, width: u16) -> Option<Line<'static>> {
    let agent = layout::shows_agent_metadata(width)
        .then(|| state.active_agent())
        .flatten();
    let model = state.active_model_label();
    let provider = layout::shows_agent_metadata(width)
        .then(|| state.active_provider())
        .flatten();
    let variant = model.as_ref().and_then(|(_, variant)| variant.clone());
    if agent.is_none() && model.is_none() {
        return None;
    }
    let muted = Style::default().fg(theme.text_muted());
    let text = Style::default().fg(theme.text());
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(agent) = agent {
        spans.push(Span::styled(agent.to_string(), text));
    }
    if let Some((id, _)) = &model {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", muted));
            spans.push(Span::styled("·", muted));
            spans.push(Span::styled(" ", muted));
        }
        spans.push(Span::styled(id.clone(), text));
    }
    if let Some(provider) = provider {
        spans.push(Span::styled(" ", muted));
        spans.push(Span::styled(provider.to_string(), muted));
    }
    if let Some(variant) = variant {
        spans.push(Span::styled(" ", muted));
        spans.push(Span::styled("·", muted));
        spans.push(Span::styled(" ", muted));
        spans.push(Span::styled(
            variant,
            Style::default()
                .fg(theme.warning())
                .add_modifier(Modifier::BOLD),
        ));
    }
    Some(Line::from(spans))
}

/// Prompt footer row: left slot then right-aligned shortcuts
/// (`component/prompt/index.tsx:1886-1973`, `feature-plugins/prompt/footer.tsx:53-104`).
fn render_footer(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &Theme,
    area: Rect,
    terminal_width: u16,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(footer_line(state, theme, area.width, terminal_width)),
        area,
    );
}

fn footer_line(
    state: &TuiState,
    theme: &Theme,
    layout_width: u16,
    terminal_width: u16,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    if state.status() == &TuiStatus::Streaming {
        let (key, label) = ESC_INTERRUPT;
        spans.push(Span::styled(key, Style::default().fg(theme.text())));
        spans.push(Span::styled(label, Style::default().fg(theme.text_muted())));
    } else if let Some(notice) = state.dcp.notice() {
        spans.push(Span::styled(
            notice.to_string(),
            Style::default().fg(theme.info()),
        ));
    }
    let mut line = Line::from(spans);
    // The hints breakpoint reads the terminal width, the alignment the row
    // width (`feature-plugins/prompt/footer.tsx:53`).
    if !layout::shows_prompt_hints(terminal_width) {
        return line;
    }
    let hints = Line::from(vec![
        Span::styled(AGENTS_HINT.0, Style::default().fg(theme.text())),
        Span::styled(AGENTS_HINT.1, Style::default().fg(theme.text_muted())),
        Span::styled("  ", Style::default()),
        Span::styled(COMMANDS_HINT.0, Style::default().fg(theme.text())),
        Span::styled(COMMANDS_HINT.1, Style::default().fg(theme.text_muted())),
    ]);
    let gap = layout_width
        .saturating_sub(line.width() as u16)
        .saturating_sub(hints.width() as u16);
    if gap > 0 {
        line.spans.push(Span::raw(" ".repeat(gap as usize)));
    }
    line.spans.extend(hints.spans);
    line
}

/// Height-1 devtools bar (`component/devtools-bar.tsx:232-445`), background
/// `decrease(background.base)`.
fn render_devtools(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let bar_bg = theme.decrease(theme.background());
    let muted = Style::default().fg(theme.text_muted());
    let line = Line::from(vec![
        // `Server` keeps the empty connection-icon cell: there is no client
        // connection to indicate in-process (`devtools-bar.tsx:245-293`).
        Span::styled("  Server ", muted),
        // `statusIcon(runtimeStatus(no samples))` is the normal `○` (`:294-340,581-586`).
        Span::styled(" ○ UI ", muted),
        Span::styled(" Theme ", muted),
        Span::styled(" Tools ", muted),
        Span::styled(" Experiments ", muted),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(bar_bg)),
        area,
    );
}

/// Upstream toast surface for transient notes (`ui/toast.tsx:48-90`):
/// absolute top-right, `maxWidth=min(60,width-6)`, side borders in the
/// variant color, raised-high interior with 2/2/1/1 padding.
fn render_toast(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    let Some(message) = state.note() else {
        return;
    };
    let width = TOAST_MAX_WIDTH.min(area.width.saturating_sub(6));
    if width < 7 || area.height < 4 {
        return;
    }
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(TOAST_RIGHT_MARGIN + width));
    let y = area.y.saturating_add(1);
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(Style::default().fg(theme.warning()));
    let text_width = width.saturating_sub(6);
    let wrapped: Vec<Line<'static>> = wrap_text(message, text_width as usize)
        .into_iter()
        .map(Line::from)
        .collect();
    let height = (wrapped.len() as u16)
        .saturating_add(2)
        .min(area.height.saturating_sub(1));
    let rect = Rect::new(x, y, width, height);
    frame.render_widget(block, rect);
    // Interior surface (inside the side borders, padding included), upstream
    // `background.raised.high` (`ui/toast.tsx:64-70`).
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background_raised_high())),
        Rect::new(
            rect.x.saturating_add(1),
            rect.y,
            rect.width.saturating_sub(2),
            rect.height,
        ),
    );
    frame.render_widget(
        Paragraph::new(wrapped).style(Style::default().fg(theme.text())),
        Rect::new(
            rect.x.saturating_add(3),
            rect.y.saturating_add(1),
            text_width,
            rect.height.saturating_sub(2),
        ),
    );
}

/// Greedy word wrap at `width` display cells, upstream toast `wrapMode="word"`
/// (`ui/toast.tsx:75`). Words longer than the width are split so no bounded
/// notice is silently dropped; returns at least one (possibly empty) line.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for word in text.split(' ') {
        let separator = usize::from(!current.is_empty());
        let word_width = text_width(word);
        if current_width + separator + word_width <= width {
            if separator == 1 {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        let mut chunk = String::new();
        let mut chunk_width = 0usize;
        for ch in word.chars() {
            let char_width = text_width(&ch.to_string());
            if chunk_width + char_width > width && !chunk.is_empty() {
                lines.push(std::mem::take(&mut chunk));
                chunk_width = 0;
            }
            chunk.push(ch);
            chunk_width += char_width;
        }
        current = chunk;
        current_width = chunk_width;
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn text_width(text: &str) -> usize {
    Line::from(text).width()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::KeyAction;
    use oc_core::core_app::{CoreApp, MockProvider};
    use oc_core::domain::SessionId;
    use oc_core::queries::{
        AgentEntry, CatalogSnapshot, HistoryMessage, HistoryPage, ModelEntry, VariantEntry,
    };
    use oc_core::session::Role;

    fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
        HistoryMessage {
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
        let total = rows.len();
        HistoryPage {
            rows,
            total,
            has_older: false,
            has_newer: false,
        }
    }

    fn catalog() -> CatalogSnapshot {
        CatalogSnapshot {
            provider: "ludka2".to_string(),
            models: vec![ModelEntry {
                id: "a".to_string(),
                variants: vec![VariantEntry {
                    name: "low".to_string(),
                    disabled: false,
                    reasoning_effort: Some("low".to_string()),
                }],
                context: 1000,
                output: 100,
            }],
            model_id: "a".to_string(),
            variant: None,
            agents: vec![AgentEntry {
                id: "x".to_string(),
                description: "first profile".to_string(),
                model: Some("a".to_string()),
                variant: None,
            }],
            agent_id: Some("x".to_string()),
            commands: Vec::new(),
        }
    }

    async fn golden_state() -> TuiState {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        std::mem::forget(guard);
        let id = SessionId::new("s-golden").expect("id");
        app.create_session(id.clone()).await.expect("create");
        let mut state = TuiState::new(app, id);
        state.apply_catalog(catalog());
        state.attach_page(&page(vec![
            msg(1, Role::User, "hello"),
            msg(2, Role::Assistant, "hi there"),
        ]));
        state
    }

    /// The buffer rows with trailing spaces removed, so snapshots stay small
    /// enough to review while leading padding is still asserted.
    fn screen(state: &TuiState, width: u16, height: u16) -> Vec<String> {
        crate::views::render_test(state, width, height)
            .into_iter()
            .map(|row| row.trim_end().to_string())
            .collect()
    }

    fn right_aligned(text: &str, width: usize) -> String {
        format!("{}{text}", " ".repeat(width.saturating_sub(text.len())))
    }

    #[tokio::test]
    async fn golden_screen_80x24() {
        let state = golden_state().await;
        let hints = "shift+tab agents  ctrl+p commands";
        let mut expected = vec![String::new(); 24];
        // Tabs rail: single active tab, upstream number cell + title.
        expected[0] = " 1 Untitled session".to_string();
        // Transcript: sticky bottom, content padding 2.
        expected[13] = "  user: hello".to_string();
        expected[14] = "  assistant: hi there".to_string();
        // Status row is empty while pinned; prompt box rows follow.
        expected[16] = "  ┃".to_string();
        expected[17] = "  ┃".to_string();
        expected[18] = "  ┃".to_string();
        expected[19] = "  ┃  x · a ludka2 · low".to_string();
        expected[20] = format!("  ╹{}", "▀".repeat(75));
        expected[21] = format!("  {}", right_aligned(hints, 76));
        expected[22] = String::new();
        // Devtools bar (local channel default), height 1.
        expected[23] = "  Server  ○ UI  Theme  Tools  Experiments".to_string();
        assert_eq!(screen(&state, 80, 24), expected);
    }

    #[tokio::test]
    async fn golden_screen_120x40() {
        let state = golden_state().await;
        let hints = "shift+tab agents  ctrl+p commands";
        let mut expected = vec![String::new(); 40];
        expected[0] = " 1 Untitled session".to_string();
        expected[29] = "  user: hello".to_string();
        expected[30] = "  assistant: hi there".to_string();
        expected[32] = "  ┃".to_string();
        expected[33] = "  ┃".to_string();
        expected[34] = "  ┃".to_string();
        expected[35] = "  ┃  x · a ludka2 · low".to_string();
        expected[36] = format!("  ╹{}", "▀".repeat(115));
        expected[37] = format!("  {}", right_aligned(hints, 116));
        expected[39] = "  Server  ○ UI  Theme  Tools  Experiments".to_string();
        assert_eq!(screen(&state, 120, 40), expected);
    }

    /// 44 is the only session-route switch this iteration renders (narrow
    /// padding, metadata slots, footer hints); 60/80 keep that chrome. The
    /// sidebar predicate switches above 120 (`component/session-frame.tsx:106`).
    #[tokio::test]
    async fn resize_switches_layout_at_upstream_breakpoints() {
        let state = golden_state().await;
        for width in [44, 60, 80, 120] {
            let frame = screen(&state, width, 24).join("\n");
            assert!(frame.contains("x · a ludka2 · low"), "{width}:\n{frame}");
            assert!(frame.contains("ctrl+p commands"), "{width}:\n{frame}");
        }
        assert!(layout::sidebar_auto(121));
        assert!(!layout::sidebar_auto(120));

        // Below 44 the padding drops to one cell and the agent/provider slots
        // and hints disappear (`routes/session/index.tsx:1277`,
        // `component/prompt/metadata.tsx:110-111`, `feature-plugins/prompt/footer.tsx:53`).
        let narrow = screen(&state, 43, 24);
        assert!(narrow[16].starts_with(" ┃"), "{:?}", narrow[16]);
        assert!(narrow[19].contains("┃ a"), "{:?}", narrow[19]);
        assert!(!narrow[19].contains("x ·"), "{:?}", narrow[19]);
        assert!(!narrow[19].contains("ludka2"), "{:?}", narrow[19]);
        assert!(!narrow.join("\n").contains("ctrl+p commands"));

        // Shrink -> grow is a pure re-layout: identical frames at the same size.
        let wide = screen(&state, 120, 40);
        let _ = screen(&state, 43, 24);
        assert_eq!(screen(&state, 120, 40), wide);
        assert_eq!(screen(&state, 43, 24), narrow);
    }

    #[tokio::test]
    async fn message_stream_sticks_to_the_bottom() {
        let mut state = golden_state().await;
        let rows: Vec<HistoryMessage> = (0..40)
            .map(|i| msg(i, Role::User, &format!("line {i}")))
            .collect();
        state.attach_page(&page(rows));
        assert_eq!(state.scroll(), 0);

        // Pinned: the newest row is the transcript's last row.
        let frame = screen(&state, 80, 24);
        assert_eq!(frame[13], "  user: line 38");
        assert_eq!(frame[14], "  user: line 39");
        assert!(!frame.join("\n").contains("Jump to latest"));

        // New rows keep the bottom pinned while at the bottom.
        for c in "go".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        state.handle_key(KeyAction::Enter).await;
        let turn = state.active_turn().expect("turn").clone();
        state.apply_finished(&turn, "fresh line");
        assert_eq!(state.scroll(), 0);
        let frame = screen(&state, 80, 24);
        assert_eq!(frame[14], "  ai: fresh line", "{frame:?}");

        // Scrolling up detaches: the newest row leaves the viewport and the
        // upstream jump affordance appears in the status row.
        for _ in 0..5 {
            state.handle_key(KeyAction::Up).await;
        }
        assert_eq!(state.scroll(), 5);
        let frame = screen(&state, 80, 24);
        assert!(!frame[14].contains("fresh line"), "{frame:?}");
        assert!(frame[15].contains("Jump to latest ↓"), "{frame:?}");

        // Scrolling back to the bottom re-pins the stream.
        while state.scroll() > 0 {
            state.handle_key(KeyAction::Down).await;
        }
        let frame = screen(&state, 80, 24);
        assert_eq!(frame[14], "  ai: fresh line", "{frame:?}");
        assert!(!frame[15].contains("Jump to latest"));
    }

    #[tokio::test]
    async fn footer_and_toast_use_theme_colors() {
        use ratatui::{Terminal, backend::TestBackend};

        let theme = Theme::dark();
        let mut state = golden_state().await;
        let turn = oc_core::core_app::WorkerTurnId("t-title".to_string());
        state.begin_compress_turn(turn.clone());
        state.push_note("something happened");

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| crate::views::render_frame(frame, &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();

        // Toast: side border in the warning variant color, raised-high
        // interior, note text on the first content row (padding 2/1).
        assert_eq!(buffer[(18, 1)].fg, theme.warning());
        assert_eq!(buffer[(21, 2)].bg, theme.background_raised_high());
        assert_eq!(buffer[(21, 2)].fg, theme.text());
        // Footer: `esc interrupt` while streaming (`esc` base, word muted).
        assert_eq!(buffer[(2, 21)].fg, theme.text());
        assert_eq!(buffer[(6, 21)].fg, theme.text_muted());

        // The DCP notice replaces the interrupt hint once the turn is idle.
        state.apply_finished(&turn, "done");
        state.notify_dcp(crate::dcp_panel::DcpOutcome::Failed {
            reason: "span open".to_string(),
        });
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("backend");
        terminal
            .draw(|frame| crate::views::render_frame(frame, &state))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(2, 21)].fg, theme.info());
    }

    #[test]
    fn toast_wrap_is_word_aware_and_lossless() {
        assert_eq!(wrap_text("short note", 20), vec!["short note"]);
        assert_eq!(
            wrap_text("alpha beta gamma delta", 12),
            vec!["alpha beta", "gamma delta"]
        );
        // A word longer than the line splits instead of disappearing.
        assert_eq!(wrap_text("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        // Cyrillic is one cell wide, emoji are two.
        assert_eq!(wrap_text("привет мир", 9), vec!["привет", "мир"]);
        assert_eq!(wrap_text("🌍🌍", 3), vec!["🌍", "🌍"]);
    }

    #[tokio::test]
    async fn prompt_box_glyphs_follow_the_padding_breakpoint() {
        let mut state = golden_state().await;
        let wide = screen(&state, 44, 24);
        assert!(wide[16].starts_with("  ┃"));
        assert!(wide[20].starts_with("  ╹▀"));
        // Input renders inside the box, after the inner 2-cell padding.
        for c in "hi".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        let wide = screen(&state, 44, 24);
        assert_eq!(wide[17], "  ┃  hi");

        let narrow = screen(&state, 43, 24);
        assert!(narrow[16].starts_with(" ┃"));
        assert!(narrow[20].starts_with(" ╹▀"));
        assert_eq!(narrow[17], " ┃ hi");
    }
}
