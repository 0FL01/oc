//! Upstream v2.0.12 shell: root regions, session area, prompt box, status
//! row, prompt footer and devtools bar.
//!
//! Geometry uses actual frame dimensions and bounded transcript rows. Safe DTOs
//! supply the title, Location, parent relationship, model/agent and measured usage.
//! Native debug chrome follows the override/build channel and labels the in-process runtime: upstream
//! server/Theme/Tools/Experiments actions are not advertised as working controls.
//!
//! Transient notes render as the upstream toast (`ui/toast.tsx:48-50`). Shared
//! modal dialogs composite last, without reserving any transcript/prompt rows.

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
pub const AGENTS_HINT: (&str, &str) = (crate::commands::AGENTS_BINDING, "agents");
/// Prompt footer shortcuts (`feature-plugins/prompt/footer.tsx:89-104`).
pub const COMMANDS_HINT: (&str, &str) = (crate::commands::COMMANDS_BINDING, "commands");
/// Toast max width (`ui/toast.tsx:50`: `min(60, width - 6)`).
pub const TOAST_MAX_WIDTH: u16 = 60;
/// Toast right margin (`ui/toast.tsx:49`: `right={2}`).
pub const TOAST_RIGHT_MARGIN: u16 = 2;

/// Safe startup capability states. Never carry raw configuration/provider errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupFailure {
    /// Native in-process configuration/runtime construction failed.
    Preflight,
    /// The native application's session/history/catalog query failed.
    Query,
}

/// Distinct native error route; upstream service attach is not a native capability.
pub fn render_startup_failure(frame: &mut Frame<'_>, failure: StartupFailure) {
    let theme = Theme::dark();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background())),
        area,
    );
    let (reason, action) = match failure {
        StartupFailure::Preflight => (
            "Configuration / runtime initialization failed",
            "Check opencode.json/jsonc, cli.json/jsonc and data-directory access, then restart.",
        ),
        StartupFailure::Query => (
            "Session / catalog query failed",
            "Check the session's Location and data-directory access, then restart.",
        ),
    };
    let text = vec![
        Line::from("Native startup error").style(Style::default().add_modifier(Modifier::BOLD)),
        Line::from(""),
        Line::from(reason),
        Line::from(action),
        Line::from(""),
        Line::from("Native runtime is in-process; service attach is unsupported."),
        Line::from("Raw configuration and provider details are not displayed."),
        Line::from(""),
        Line::from("esc / q / ctrl+c  exit"),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(theme.text()).bg(theme.background()))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area.inner(ratatui::layout::Margin::new(2, 2)),
    );
}

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
    let mut regions = layout::configured_shell_regions(
        area,
        state.chrome.devtools_visible(),
        state.chrome.vertical_tabs_width,
    );
    if state.home {
        regions.session = Rect {
            height: area.height.saturating_sub(regions.devtools.height),
            ..area
        };
    } else {
        render_tabs(frame, theme, regions.tabs, state.session_title.as_deref());
    }
    let sidebar = !state.home
        && state.parent_id.is_none()
        && !state.chrome.sidebar_hidden
        && layout::sidebar_auto(regions.session.width);
    let main = if sidebar {
        let main = Rect {
            width: regions
                .session
                .width
                .saturating_sub(layout::SESSION_SIDEBAR_WIDTH),
            ..regions.session
        };
        render_sidebar(
            frame,
            state,
            theme,
            Rect::new(
                main.right(),
                main.y,
                layout::SESSION_SIDEBAR_WIDTH,
                main.height,
            ),
        );
        main
    } else {
        regions.session
    };
    render_session(frame, state, theme, main);
    render_devtools(frame, theme, regions.devtools);
    render_toast(frame, state, theme, area);
    crate::dialog::render(frame, state);
}

/// Single active tab in the horizontal strip
/// (`component/session-tabs.tsx:1506-1508`, `context/session-tabs-model.ts:33-35`).
fn tab_line(theme: &Theme, available: u16, title: Option<&str>) -> Line<'static> {
    let title = title.unwrap_or(UNTITLED_SESSION);
    let tab_bg = theme.decrease(theme.background_panel());
    let tab_width = layout::single_tab_width(available);
    // Indicator cell: `numberWidth + 1` with the label right-aligned and one
    // padding cell (`component/session-tabs.tsx:1671-1682`); selected number
    // color is `tint(text.base, tabBackground, 0.25)` (`:1606-1618`).
    let number = tint(theme.text(), tab_bg, 0.25);
    let mut spans = vec![
        Span::styled(" 1 ", Style::default().fg(number).bg(tab_bg)),
        Span::styled(
            title.to_string(),
            Style::default().fg(theme.text()).bg(tab_bg),
        ),
    ];
    let used = 3 + title.chars().count() as u16;
    if tab_width > used {
        spans.push(Span::styled(
            " ".repeat((tab_width - used) as usize),
            Style::default().bg(tab_bg),
        ));
    }
    Line::from(spans)
}

fn render_tabs(frame: &mut Frame<'_>, theme: &Theme, area: Rect, title: Option<&str>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    if area.height > 1 {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.background_panel())),
            area,
        );
    }
    let strip = Rect {
        width: layout::single_tab_width(area.width),
        ..area
    };
    frame.render_widget(Paragraph::new(tab_line(theme, area.width, title)), strip);
}

fn render_session(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    if state.home {
        render_home(frame, state, theme, area);
        return;
    }
    let input = prompt_lines(state, area.width);
    let input_height = (input.len() as u16)
        .min((frame.area().height / 3).max(6))
        .max(1);
    let regions = layout::dynamic_session_regions(area, 0, input_height + 3);
    render_transcript(frame, state, regions.transcript, frame.area().width);
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

fn prompt_lines(state: &TuiState, width: u16) -> Vec<crate::styled::Line> {
    let pad = layout::session_padding(width);
    let text_width = width.saturating_sub(4 * pad + 1).max(1);
    state
        .prompt_layout(text_width as usize)
        .0
        .into_iter()
        .map(|row| {
            crate::styled::Line::new(
                row.spans
                    .into_iter()
                    .map(|(text, selected)| {
                        crate::styled::Span::styled(
                            text,
                            if selected {
                                Style::default().add_modifier(Modifier::REVERSED)
                            } else {
                                Style::default()
                            },
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

fn render_sidebar(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background_panel())),
        area,
    );
    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );
    let title = wrap_text(
        state.session_title.as_deref().unwrap_or(UNTITLED_SESSION),
        inner.width.saturating_sub(2) as usize,
    );
    let mut lines: Vec<Line<'static>> = title
        .into_iter()
        .map(|t| {
            Line::styled(
                t,
                Style::default()
                    .fg(theme.text())
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    lines.push(Line::default());
    lines.push(Line::styled(
        "Context",
        Style::default()
            .fg(theme.text())
            .add_modifier(Modifier::BOLD),
    ));
    match state.context_usage() {
        Some((tokens, limit)) => {
            lines.push(Line::from(format!("{} tokens", thousands(tokens))));
            lines.push(Line::from(limit.map_or_else(
                || "Limit unknown".into(),
                |l| {
                    format!(
                        "{}% used",
                        (tokens as f64 / l as f64 * 100.0).round() as u64
                    )
                },
            )));
        }
        None => lines.push(Line::from("Usage unknown")),
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(theme.text_muted())),
        inner,
    );
    if inner.height > 0 {
        frame.render_widget(
            Paragraph::new(compact_path(
                state
                    .chrome
                    .location
                    .as_deref()
                    .unwrap_or("Location unknown"),
                inner.width as usize,
            ))
            .style(Style::default().fg(theme.text_muted())),
            Rect {
                y: inner.bottom() - 1,
                height: 1,
                ..inner
            },
        );
    }
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    s.chars()
        .enumerate()
        .fold(String::new(), |mut out, (i, c)| {
            if i > 0 && (s.len() - i).is_multiple_of(3) {
                out.push(',');
            }
            out.push(c);
            out
        })
}

fn render_home(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    let width = area
        .width
        .saturating_sub(2 * layout::session_padding(area.width))
        .min(75);
    let x = area.x + area.width.saturating_sub(width).div_ceil(2);
    let input_height = (prompt_lines(state, width + 2 * layout::session_padding(area.width)).len()
        as u16)
        .min((frame.area().height / 3).max(6))
        .max(1);
    let h = input_height + 3;
    let logo = home_logo(theme, area.width, area.height);
    let logo_height = logo.len() as u16;
    // Home's two equal flex spacers surround a 3-row top spacer, logo,
    // 2-row gap and the prompt/footer; home footer keeps its final 2 rows.
    let y = area.y + area.height.saturating_sub(h + logo_height + 9) / 2 + 3;
    let logo_width = logo.iter().map(Line::width).max().unwrap_or(0) as u16;
    frame.render_widget(
        Paragraph::new(logo),
        Rect::new(
            area.x + area.width.saturating_sub(logo_width).div_ceil(2),
            y,
            logo_width.min(area.width),
            logo_height.min(area.bottom().saturating_sub(y)),
        ),
    );
    let prompt_y = (y + logo_height + 2).min(area.bottom());
    let body = Rect::new(
        x,
        prompt_y,
        width,
        h.min(area.bottom().saturating_sub(prompt_y)),
    );
    let underline = Rect::new(
        x,
        body.bottom(),
        width,
        u16::from(body.bottom() < area.bottom()),
    );
    render_prompt(
        frame,
        state,
        theme,
        body,
        underline,
        width + 2 * layout::session_padding(area.width),
    );
    let footer = Rect::new(
        x,
        underline.bottom(),
        width,
        u16::from(underline.bottom() < area.bottom()),
    );
    render_footer(frame, state, theme, footer, area.width);
    if area.height >= 2 {
        frame.render_widget(
            Paragraph::new(env!("CARGO_PKG_VERSION"))
                .alignment(Alignment::Right)
                .style(Style::default().fg(theme.text_muted())),
            Rect::new(area.x, area.bottom() - 2, area.width.saturating_sub(2), 1),
        );
    }
}

/// Upstream logo.ts/component/logo.tsx glyphs; theme-derived shadow cells.
fn home_logo(theme: &Theme, width: u16, height: u16) -> Vec<Line<'static>> {
    if height < 12 {
        return Vec::new();
    }
    let left = [
        "                   ",
        "█▀▀█ █▀▀█ █▀▀█ █▀▀▄",
        "█__█ █__█ █^^^ █__█",
        "▀▀▀▀ █▀▀▀ ▀▀▀▀ ▀~~▀",
    ];
    let right = [
        "             ▄     ",
        "█▀▀▀ █▀▀█ █▀▀█ █▀▀█",
        "█___ █__█ █__█ █^^^",
        "▀▀▀▀ ▀▀▀▀ ▀▀▀▀ ▀▀▀▀",
    ];
    let part = |s: &str, fg, bold| {
        let shadow = tint(theme.background(), fg, 0.25);
        s.chars()
            .map(|c| {
                let mut style = Style::default().fg(fg);
                if bold {
                    style = style.add_modifier(Modifier::BOLD);
                }
                let glyph = match c {
                    '_' => {
                        style = style.bg(shadow);
                        ' '
                    }
                    '^' => {
                        style = style.bg(shadow);
                        '▀'
                    }
                    '~' => {
                        style = style.fg(shadow);
                        '▀'
                    }
                    c => c,
                };
                Span::styled(glyph.to_string(), style)
            })
            .collect::<Vec<_>>()
    };
    if width < 22 {
        return ["█▀▀█", "█__█", "▀▀▀▀"]
            .iter()
            .map(|s| Line::from(part(s, theme.text(), true)))
            .collect();
    }
    if width < 44 {
        return left[1..]
            .iter()
            .map(|s| Line::from(part(s, theme.text_muted(), false)))
            .chain(
                right
                    .iter()
                    .map(|s| Line::from(part(s, theme.text(), true))),
            )
            .collect();
    }
    left.iter()
        .zip(right)
        .map(|(l, r)| {
            let mut spans = part(l, theme.text_muted(), false);
            spans.push(Span::raw(" "));
            spans.extend(part(r, theme.text(), true));
            Line::from(spans)
        })
        .collect()
}

fn take_cells(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|c| {
            used += crate::styled::char_width(*c);
            used <= width
        })
        .collect()
}

/// Keep the basename and as much of the trailing Location as fits.
fn compact_path(path: &str, width: usize) -> String {
    if text_width(path) <= width {
        return path.to_string();
    }
    let prefix = if path.starts_with('/') {
        "/…/"
    } else {
        "…/"
    };
    let mut segments = path.split('/').filter(|s| !s.is_empty()).rev();
    let Some(base) = segments.next() else {
        return take_cells(path, width);
    };
    let available = width.saturating_sub(text_width(prefix));
    if text_width(base) > available {
        return take_cells(
            &format!("{prefix}{}…", take_cells(base, available.saturating_sub(1))),
            width,
        );
    }
    let mut tail = base.to_string();
    for segment in segments {
        let remaining = width.saturating_sub(text_width(prefix) + text_width(&tail) + 1);
        if text_width(segment) > remaining {
            if remaining > 1 {
                tail = format!("{}…/{tail}", take_cells(segment, remaining - 1));
            }
            break;
        }
        tail = format!("{segment}/{tail}");
    }
    take_cells(&format!("{prefix}{tail}"), width)
}

/// Sticky scroll: long history pins to the newest row; short history starts
/// at the top (independently captured in recovery-v03/v03-attempt-04).
/// (`routes/session/index.tsx:1299-1300` `stickyScroll stickyStart="bottom"`).
/// Rows come from the upstream message renderer ([`crate::messages`]) and are
/// wrapped to the content width before the sticky slice.
fn render_transcript(frame: &mut Frame<'_>, state: &TuiState, area: Rect, terminal_width: u16) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let rows = area.height as usize;
    let lines = state.rendered_transcript(area.width, terminal_width);
    let total = lines.len();
    state.observe_viewport(area.height, total);
    let max_scroll = total.saturating_sub(rows);
    let scroll = state.scroll().min(max_scroll);
    let end = total - scroll;
    let start = end.saturating_sub(rows);
    let visible = &lines[start..end];
    let text = crate::styled::Lines::from(visible.to_vec()).into_text();
    frame.render_widget(Paragraph::new(text), area);
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
    (state.display_scroll() > 0).then(|| {
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
    let border_style = Style::default().fg(state
        .active_agent()
        .map_or(theme.border(), |a| state.agent_color(Some(a))));
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
            let (input_rows, caret) = state.prompt_layout(text_width as usize);
            let input_lines: Vec<_> = input_rows
                .into_iter()
                .map(|row| {
                    crate::styled::Line::new(
                        row.spans
                            .into_iter()
                            .map(|(text, selected)| {
                                crate::styled::Span::styled(
                                    text,
                                    if selected {
                                        Style::default().add_modifier(Modifier::REVERSED)
                                    } else {
                                        Style::default()
                                    },
                                )
                            })
                            .collect(),
                    )
                })
                .collect();
            let visible = body.height.saturating_sub(3) as usize;
            let start = caret
                .0
                .saturating_sub(visible.saturating_sub(1))
                .min(input_lines.len().saturating_sub(visible));
            frame.render_widget(
                Paragraph::new(
                    crate::styled::Lines::from(input_lines[start..].to_vec()).into_text(),
                )
                .style(Style::default().fg(theme.text()).bg(prompt_bg)),
                Rect {
                    height: body.height.saturating_sub(3),
                    ..row(1)
                },
            );
            if visible > 0 && text_width > 0 {
                frame.set_cursor_position((
                    text_x + (caret.1 as u16).min(text_width - 1),
                    body.y + 1 + caret.0.saturating_sub(start) as u16,
                ));
            }
        }
        if body.height > 3
            && let Some(line) = metadata_line(state, theme, terminal_width)
        {
            frame.render_widget(
                Paragraph::new(line).style(Style::default().fg(theme.text())),
                row(body.height - 1),
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
        spans.push(Span::styled(
            agent.to_string(),
            Style::default().fg(state.agent_color(Some(agent))),
        ));
    }
    if layout::shows_agent_metadata(width)
        && state.auto_accept == oc_core::queries::AutoAcceptState::Enabled
    {
        if !spans.is_empty() {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled("auto", muted));
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
    let muted = Style::default().fg(theme.text_muted());
    let mut hints = Vec::new();
    let commands_visible;
    if let Some((tokens, limit)) = state.context_usage() {
        let percent = limit
            .map(|l| format!(" ({}%)", (tokens as f64 / l as f64 * 100.0).round() as u64))
            .unwrap_or_default();
        let tokens = if tokens >= 1000 {
            format!("{:.1}K", tokens as f64 / 1000.0)
        } else {
            tokens.to_string()
        };
        let usage = format!("{tokens}{percent}");
        // PromptFooter's usage-aware layout reserves space for Location.
        let available = terminal_width.saturating_sub(8) as usize;
        let available = available.saturating_sub(28.min(available / 2));
        if text_width(&usage) <= available {
            hints.push(Span::styled(usage.clone(), muted));
        }
        commands_visible =
            text_width(&usage) + 4 + text_width(COMMANDS_HINT.0) + text_width(COMMANDS_HINT.1)
                <= available;
    } else {
        commands_visible = layout::shows_prompt_hints(terminal_width);
        if commands_visible {
            hints.push(Span::styled(
                format!("{} ", AGENTS_HINT.0),
                Style::default().fg(theme.text()),
            ));
            hints.push(Span::styled(AGENTS_HINT.1, muted));
        }
    }
    if commands_visible {
        hints.extend([
            Span::raw("  "),
            Span::styled(
                format!("{} ", COMMANDS_HINT.0),
                Style::default().fg(theme.text()),
            ),
            Span::styled(COMMANDS_HINT.1, muted),
        ]);
    }
    let hints = Line::from(hints);
    let hints_visible = !hints.spans.is_empty();
    let left_width =
        (layout_width as usize).saturating_sub(hints.width() + usize::from(hints_visible) * 2);
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
    } else if let Some(location) = &state.chrome.location {
        spans.push(Span::styled(compact_path(location, left_width), muted));
    }
    let mut line = Line::from(spans);
    // The hints breakpoint reads the terminal width, the alignment the row
    // width (`feature-plugins/prompt/footer.tsx:53`).
    if !hints_visible {
        return line;
    }
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
        // Informational native diagnostics, not inert upstream action labels.
        Span::styled("  Native runtime ", muted),
        // `statusIcon(runtimeStatus(no samples))` is the normal `○` (`:294-340,581-586`).
        Span::styled(" ○ UI ", muted),
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
            turn: None,
            seq,
            role,
            text: text.to_string(),
        }
    }

    fn page(rows: Vec<HistoryMessage>) -> HistoryPage {
        let total = rows.len();
        HistoryPage {
            parent_id: None,
            title: None,
            rows,
            total,
            has_older: false,
            has_newer: false,
        }
    }

    fn catalog() -> CatalogSnapshot {
        CatalogSnapshot {
            chrome: oc_core::queries::TuiChrome {
                devtools: Some(true),
                ..Default::default()
            },
            auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
            provider: "ludka2".to_string(),
            models: vec![ModelEntry {
                display_name: String::new(),
                provider_name: String::new(),
                price: None,
                id: "a".to_string(),
                variants: vec![VariantEntry {
                    name: "low".to_string(),
                    disabled: false,
                    reasoning_effort: Some("low".to_string()),
                }],
                context: 1000,
                context_known: true,
                output_known: true,
                output: 100,
            }],
            model_id: "a".to_string(),
            // Geometry goldens include a variant label only because it is selected.
            variant: Some("low".to_string()),
            agents: vec![AgentEntry {
                color_index: 0,
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

    #[tokio::test]
    async fn v03_tall_viewport_and_short_top_placement() {
        let mut state = golden_state().await;
        let short = screen(&state, 120, 80);
        assert!(
            short[3].contains("hello"),
            "short history starts below one-row top padding"
        );
        let text = (0..70).map(|i| format!("ROW-{i:03}\n")).collect::<String>();
        state.attach_page(&page(vec![msg(1, Role::Assistant, &text)]));
        let tall = screen(&state, 120, 80);
        assert!(tall.iter().filter(|r| r.contains("ROW-")).count() > 35);
        let wide = screen(&state, 160, 48);
        assert!(wide.iter().any(|r| r.contains("Context")), "actual sidebar");
    }

    #[tokio::test]
    async fn v03_paste_keeps_full_draft_behind_compact_prompt() {
        let mut state = golden_state().await;
        state.handle_paste("draft-one\ndraft-two\ndraft-three");
        let frame = screen(&state, 80, 24).join("\n");
        assert_eq!(state.input(), "draft-one\ndraft-two\ndraft-three");
        assert!(frame.contains("[Pasted ~3 lines]"));
        assert!(!frame.contains("draft-one"));
        assert!(!frame.contains("draft-two"));
        assert!(!frame.contains("draft-three"));
    }

    #[tokio::test]
    async fn v03_rendered_row_scroll_and_chrome_conditions() {
        let mut state = golden_state().await;
        let long = format!(
            "FIRST-ANCHOR {} LAST-ANCHOR",
            "wrapped payload ".repeat(800)
        );
        state.attach_page(&page(vec![msg(1, Role::Assistant, &long)]));
        assert!(!screen(&state, 80, 40).join("\n").contains("FIRST-ANCHOR"));
        for _ in 0..300 {
            state.scroll_transcript(true);
        }
        assert!(screen(&state, 80, 40).join("\n").contains("FIRST-ANCHOR"));
        let detached = state.scroll();
        state.handle_paste("draft-one\ndraft-two");
        for width in [80, 120, 160, 43, 44, 119, 120, 121, 160] {
            let frame = screen(&state, width, 48).join("\n");
            assert_eq!(frame.contains("Context"), width > 120, "{width}");
            assert_eq!(state.scroll(), detached);
            assert!(frame.contains("draft-two"));
        }
        let displayed = state.display_scroll();
        assert!(
            state.scroll() > displayed,
            "resize clamps the retained request"
        );
        state.scroll_transcript(false);
        assert_eq!(state.scroll(), displayed - 1);
        state.chrome.sidebar_hidden = true;
        assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
        state.chrome.sidebar_hidden = false;
        state.parent_id = Some("parent".into());
        assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
        state.parent_id = None;
        state.chrome.vertical_tabs_width = 42;
        assert!(!screen(&state, 160, 48).join("\n").contains("Context"));
        assert!(screen(&state, 163, 48).join("\n").contains("Context"));
        state.chrome.devtools = Some(false);
        let hidden = screen(&state, 120, 40);
        assert!(!hidden.join("\n").contains("Native runtime"));
        assert!(hidden[38].contains("ctrl+p commands"));
        state.chrome.devtools = Some(true);
        let shown = screen(&state, 120, 40);
        assert!(shown[39].contains("Native runtime"));
        assert!(shown[37].contains("ctrl+p commands"));
    }

    #[tokio::test]
    async fn v03_sidebar_uses_dto_title_usage_and_styled_boundary() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut state = golden_state().await;
        state.chrome.devtools = Some(false);
        state.chrome.location = Some("/workspace/real-project".into());
        state.session_title = Some("Actual title".into());
        state.begin_compress_turn(oc_core::core_app::WorkerTurnId("usage".into()));
        state.close_panel(); // inspect the base chrome, without the modal backdrop
        state.apply_usage(
            &oc_core::core_app::WorkerTurnId("usage".into()),
            300,
            20,
            100,
        );
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
        terminal.draw(|f| render(f, &state)).unwrap();
        let b = terminal.backend().buffer();
        assert_eq!(b[(117, 10)].bg, Theme::dark().background());
        assert_eq!(b[(118, 10)].bg, Theme::dark().background_panel());
        let text = screen(&state, 160, 48).join("\n");
        for value in [
            "Actual title",
            "320 tokens",
            "32% used",
            "/workspace/real-project",
        ] {
            assert!(text.contains(value), "{value}");
        }
        for width in [43, 44] {
            let footer = footer_line(&state, Theme::dark(), width - 4, width).to_string();
            assert!(footer.contains("320 (32%)"));
            assert!(!footer.contains("ctrl+p"));
        }
        state.parent_id = Some("parent".into());
        terminal.draw(|f| render(f, &state)).unwrap();
        assert_eq!(
            terminal.backend().buffer()[(118, 10)].bg,
            Theme::dark().background()
        );
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
    async fn v02_auto_marker_is_session_capability_not_agent_rules() {
        use oc_core::queries::AutoAcceptState;
        let mut state = golden_state().await;
        for (mode, visible) in [
            (AutoAcceptState::Unsupported, false),
            (AutoAcceptState::Disabled, false),
            (AutoAcceptState::Enabled, true),
        ] {
            let mut snapshot = catalog();
            snapshot.auto_accept = mode;
            snapshot.agents[0].color_index = 3;
            state.apply_catalog(snapshot);
            let line = super::metadata_line(&state, crate::theme::Theme::dark(), 120).unwrap();
            assert_eq!(line.to_string().contains("auto"), visible);
            assert_eq!(
                line.spans[0].style.fg,
                Some(crate::theme::Theme::dark().categorical_agents()[3])
            );
        }
        state.reset_workspace();
        assert_eq!(state.auto_accept, AutoAcceptState::Unsupported);
    }

    #[tokio::test]
    async fn no_selected_variant_has_no_metadata_label_or_overlay() {
        let mut state = golden_state().await;
        let mut snapshot = catalog();
        snapshot.variant = None;
        state.apply_catalog(snapshot);
        for width in [44, 80, 120, 160] {
            let metadata = metadata_line(&state, Theme::dark(), width).unwrap();
            assert_eq!(metadata.to_string(), "x · a ludka2");
        }
        assert!(
            state
                .picker
                .as_ref()
                .unwrap()
                .selection()
                .unwrap()
                .variant
                .is_none()
        );
    }

    #[tokio::test]
    async fn golden_screen_80x24() {
        let state = golden_state().await;
        let hints = "shift+tab agents  ctrl+p commands";
        let mut expected = vec![String::new(); 24];
        // Tabs rail: single active tab, upstream number cell + title.
        expected[0] = " 1 Untitled session".to_string();
        // Transcript: sticky bottom, content padding 2. Iteration 3a renders
        // the upstream message presentation: the user block carries the `┃`
        // border with 1/2 padding (`routes/session/index.tsx:2298-2335`), the
        // assistant text sits at paddingLeft=3 and each upstream row has
        // `marginTop=1` (`routes/session/index.tsx:1435`).
        expected[2] = "  ┃".to_string();
        expected[3] = "  ┃  hello".to_string();
        expected[4] = "  ┃".to_string();
        expected[5] = String::new();
        expected[6] = "     hi there".to_string();
        // Status row is empty while pinned; prompt box rows follow.
        expected[16] = "  ┃".to_string();
        expected[17] = "  ┃".to_string();
        expected[18] = "  ┃".to_string();
        expected[19] = "  ┃  x · a ludka2 · low".to_string();
        expected[20] = format!("  ╹{}", "▀".repeat(75));
        expected[21] = format!("  {}", right_aligned(hints, 76));
        expected[22] = String::new();
        // Devtools bar (local channel default), height 1.
        expected[23] = "  Native runtime  ○ UI".to_string();
        assert_eq!(screen(&state, 80, 24), expected);
    }

    #[tokio::test]
    async fn golden_screen_120x40() {
        let state = golden_state().await;
        let hints = "shift+tab agents  ctrl+p commands";
        let mut expected = vec![String::new(); 40];
        expected[0] = " 1 Untitled session".to_string();
        // Same message presentation as 80x24, just taller.
        expected[2] = "  ┃".to_string();
        expected[3] = "  ┃  hello".to_string();
        expected[4] = "  ┃".to_string();
        expected[5] = String::new();
        expected[6] = "     hi there".to_string();
        expected[32] = "  ┃".to_string();
        expected[33] = "  ┃".to_string();
        expected[34] = "  ┃".to_string();
        expected[35] = "  ┃  x · a ludka2 · low".to_string();
        expected[36] = format!("  ╹{}", "▀".repeat(115));
        expected[37] = format!("  {}", right_aligned(hints, 116));
        expected[39] = "  Native runtime  ○ UI".to_string();
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

    async fn submit_and_reconcile(state: &mut TuiState) {
        state.handle_key(KeyAction::Enter).await;
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while state.active_turn().is_none() {
                tokio::task::yield_now().await;
                state.poll_submission();
            }
        })
        .await
        .expect("accepted turn");
    }

    #[tokio::test]
    async fn message_stream_sticks_to_the_bottom() {
        let mut state = golden_state().await;
        let rows: Vec<HistoryMessage> = (0..40)
            .map(|i| msg(i, Role::User, &format!("line {i}")))
            .collect();
        state.attach_page(&page(rows));
        assert_eq!(state.scroll(), 0);

        // Pinned: the newest user block ends on the transcript's last row.
        let frame = screen(&state, 80, 24);
        assert_eq!(frame[13], "  ┃  line 39", "{frame:?}");
        assert_eq!(frame[14], "  ┃", "{frame:?}");
        assert!(!frame.join("\n").contains("Jump to latest"));

        // New rows keep the bottom pinned while at the bottom.
        for c in "go".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        submit_and_reconcile(&mut state).await;
        let turn = state.active_turn().expect("turn").clone();
        state.apply_finished(&turn, "fresh line", 0);
        assert_eq!(state.scroll(), 0);
        let frame = screen(&state, 80, 24);
        assert!(frame.join("\n").contains("fresh line"), "{frame:?}");

        // Scrolling up detaches: the newest row leaves the viewport and the
        // upstream jump affordance appears in the status row.
        for _ in 0..5 {
            state.scroll_transcript(true);
        }
        assert_eq!(state.scroll(), 5);
        let frame = screen(&state, 80, 24);
        assert!(!frame.join("\n").contains("fresh line"), "{frame:?}");
        assert!(frame[15].contains("Jump to latest ↓"), "{frame:?}");

        // Scrolling back to the bottom re-pins the stream.
        while state.scroll() > 0 {
            state.scroll_transcript(false);
        }
        let frame = screen(&state, 80, 24);
        assert!(frame.join("\n").contains("fresh line"), "{frame:?}");
        assert!(!frame[15].contains("Jump to latest"));
    }

    /// Iteration 3a: a live turn renders through the upstream message
    /// presentation — collapsed reasoning, assistant markdown at paddingLeft=3
    /// and the `agent · model · dur · tok/s` footer with provider usage.
    #[tokio::test]
    async fn golden_live_turn_with_reasoning_and_footer() {
        let mut state = golden_state().await;
        for c in "hi".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        submit_and_reconcile(&mut state).await;
        let turn = state.active_turn().expect("turn").clone();
        state.apply_reasoning_delta(&turn, "**Reading the code**\n\nbody");
        state.apply_delta(&turn, "All done.");
        // Running state: the static spinner fallback with the summary title.
        let running = screen(&state, 80, 24);
        assert!(
            running
                .iter()
                .any(|row| row.contains("⋯ Thinking: Reading the code")),
            "{running:?}"
        );
        state.apply_usage(&turn, 100, 200, 4000);
        state.apply_finished(&turn, "All done.", 1500);

        let frame = screen(&state, 80, 24);
        // Two committed rows, then the live turn: user block, assistant block
        // with the reasoning header, markdown body and footer.
        assert_eq!(frame[1], "  ┃", "{frame:?}");
        assert_eq!(frame[2], "  ┃  hello", "{frame:?}");
        assert_eq!(frame[3], "  ┃", "{frame:?}");
        assert_eq!(frame[4], "", "{frame:?}");
        assert_eq!(frame[5], "     hi there", "{frame:?}");
        assert_eq!(frame[6], "  ┃", "{frame:?}");
        assert_eq!(frame[7], "  ┃  hi", "{frame:?}");
        assert_eq!(frame[8], "  ┃", "{frame:?}");
        // Reasoning duration is measured by the view (wall clock), so only the
        // stable prefix is asserted; the turn duration comes from the event.
        assert!(
            frame[10].starts_with("     + Thought: Reading the code"),
            "{frame:?}"
        );
        assert_eq!(frame[12], "     All done.", "{frame:?}");
        assert_eq!(frame[14], "     X · a · 1.5s · 50.0 tok/s", "{frame:?}");
    }

    /// Interrupted turns keep the partial answer and mark the footer
    /// (`routes/session/index.tsx:1977-1980`).
    #[tokio::test]
    async fn golden_interrupted_turn_footer() {
        let mut state = golden_state().await;
        for c in "go".chars() {
            state.handle_key(KeyAction::Char(c)).await;
        }
        submit_and_reconcile(&mut state).await;
        let turn = state.active_turn().expect("turn").clone();
        state.apply_delta(&turn, "partial answer");
        state.apply_interrupted(&turn, "partial answer", 1500);

        let frame = screen(&state, 80, 24);
        assert!(frame.join("\n").contains("partial answer"), "{frame:?}");
        assert_eq!(
            state
                .viewport()
                .iter()
                .filter(|line| line.contains("interrupted"))
                .count(),
            1,
            "{:?}",
            state.viewport()
        );
        assert!(frame[13] == "     X · a · 1.5s · interrupted", "{frame:?}");
    }

    #[tokio::test]
    async fn footer_and_toast_use_theme_colors() {
        use ratatui::{Terminal, backend::TestBackend};

        let theme = Theme::dark();
        let mut state = golden_state().await;
        let turn = oc_core::core_app::WorkerTurnId("t-title".to_string());
        state.begin_compress_turn(turn.clone());
        state.push_note("something happened");
        state.close_panel(); // the toast/footer underlay is qualified independently

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
        state.apply_finished(&turn, "done", 0);
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
