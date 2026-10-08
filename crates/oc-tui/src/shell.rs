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
//! Composition and golden-frame regressions live in `shell/tests.rs`.

use oc_core::queries::TabIndicators;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::app::{
    NoteVariant, TAB_SPINNER_FRAMES, TabAttention, TabPulseFrame, TuiState, TuiStatus,
    tab_glow_intensity,
};
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
/// Promoted sessionless Home slot (`context/session-tabs-model.ts:8`).
pub(crate) const NEW_SESSION_TAB_TITLE: &str = "New session";
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
/// End-of-title fade in upstream `component/session-tabs.tsx` (resting marquee).
const TAB_TITLE_FADE_WIDTH: usize = 4;

/// Safe startup capability states. Never carry raw configuration/provider errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupFailure {
    /// Native application preflight stage/category (no raw error text).
    Preflight(oc_adapters::application::SpawnFailure),
    /// Actual native preflight, sharing the common safe cause projection.
    Diagnostic(oc_adapters::application::SpawnDiagnostic),
    /// The native application's session/history/catalog query failed.
    Query,
    QueryDiagnostic(oc_core::queries::ServiceDiagnostic),
}

/// Distinct native error route; upstream service attach is not a native capability.
pub fn render_startup_failure(frame: &mut Frame<'_>, failure: StartupFailure) {
    let theme = Theme::dark();
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background())),
        area,
    );
    let diagnostic = match &failure {
        StartupFailure::Diagnostic(issue) => Some(issue.diagnostic.clone()),
        StartupFailure::QueryDiagnostic(diagnostic) => Some(diagnostic.clone()),
        _ => None,
    };
    let category = match failure {
        StartupFailure::Diagnostic(issue) => StartupFailure::Preflight(issue.category),
        StartupFailure::QueryDiagnostic(_) => StartupFailure::Query,
        other => other,
    };
    let (reason, action) = match category {
        StartupFailure::Preflight(category) => match category {
            oc_adapters::application::SpawnFailure::Configuration => (
                "Configuration load failed",
                "Check opencode.json/jsonc, cli.json/jsonc and selected model, then restart.",
            ),
            oc_adapters::application::SpawnFailure::MissingCredential => (
                "Selected provider credential missing",
                "Set a nonempty API key; if configured via {env:...}, export that variable in the launching shell.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryUnauthorized => (
                "Model discovery authentication rejected (401)",
                "Check the configured credential and catalog route; this does not test Responses access.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryForbidden => (
                "Model discovery access forbidden (403)",
                "Check catalog-listing permission or proxy policy; this does not test Responses access.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryHttp => (
                "Model discovery HTTP failure",
                "Check the configured catalog endpoint and provider service, then retry.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryNetwork => (
                "Model discovery network failure",
                "Check connectivity to the catalog endpoint and retry after the timeout.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryInvalidResponse => (
                "Model discovery invalid or empty catalog",
                "Check the provider's catalog response format and available models, then retry.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryInvalidConfig => (
                "Model discovery configuration invalid",
                "Check the configured provider URL, credential and headers, then retry.",
            ),
            oc_adapters::application::SpawnFailure::DiscoveryCancelled => (
                "Model discovery cancelled",
                "Retry catalog loading before selecting a model.",
            ),
            oc_adapters::application::SpawnFailure::SelectedModelAbsent => (
                "Selected model absent from catalog",
                "Discovery succeeded. Choose a model returned for this key or update the selected model.",
            ),
            oc_adapters::application::SpawnFailure::DataRootBusy => (
                "Data root busy",
                "Close the other oc process using this data directory, then retry.",
            ),
            oc_adapters::application::SpawnFailure::UnsafeDataRoot => (
                "Unsafe data root",
                "Choose a private, owned data directory without symlinks, then retry.",
            ),
            oc_adapters::application::SpawnFailure::DataRootUnavailable => (
                "Data root unavailable",
                "Check data-directory access and the --data-dir setting, then retry.",
            ),
            oc_adapters::application::SpawnFailure::Storage => (
                "Storage or saved selection failed",
                "Check the data directory and saved model selection, then retry.",
            ),
            oc_adapters::application::SpawnFailure::Recovery => (
                "Storage recovery failed",
                "Check data-directory access and available disk space, then retry.",
            ),
            oc_adapters::application::SpawnFailure::Runtime => (
                "Native runtime initialization failed",
                "Check the configured Location and native runtime settings, then retry.",
            ),
        },
        StartupFailure::Query => (
            "Session / catalog query failed",
            "Check the session's Location and data-directory access, then restart.",
        ),
        StartupFailure::Diagnostic(_) | StartupFailure::QueryDiagnostic(_) => {
            unreachable!("diagnostic category projected above")
        }
    };
    let mut text = vec![
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
    if let Some(diagnostic) = diagnostic {
        text.insert(
            4,
            Line::from(format!(
                "Stage: {}; code: {}",
                diagnostic.stage.as_str(),
                diagnostic.code.as_str()
            )),
        );
        text.insert(5, Line::from(format!("Source: {}", diagnostic.source)));
        text.insert(
            6,
            Line::from(format!("Field: {}", diagnostic.field.join("."))),
        );
        text.insert(7, Line::from(diagnostic.to_string()));
    }
    frame.render_widget(
        Paragraph::new(text)
            .style(Style::default().fg(theme.text()).bg(theme.background()))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        area.inner(ratatui::layout::Margin::new(2, 2)),
    );
}

/// Establish the truecolor base canvas before drawing any session contents.
pub fn render_background(frame: &mut Frame<'_>) {
    let theme = Theme::dark();
    let area = frame.area();
    // Upstream paints the whole frame with `background.base` (`app.tsx:1310-1314`).
    // Its blank canvas cells carry truecolor white, not the terminal's default
    // foreground (which resolves differently under the shared PTY profile).
    frame.render_widget(
        Block::default().style(
            Style::default()
                .fg(ratatui::style::Color::Rgb(255, 255, 255))
                .bg(theme.background()),
        ),
        area,
    );
}

/// Render one whole frame: root background, tab strip, session area,
/// devtools bar, then the toast overlay.
pub fn render(frame: &mut Frame<'_>, state: &TuiState) {
    state.clear_prompt_paint();
    let theme = Theme::dark();
    let area = frame.area();
    render_background(frame);
    let regions = shell_regions(state, area);
    state.prepare_tabs(area, std::time::Instant::now());
    if let Some(strip) = tab_strip(state, area) {
        render_deck_tabs(frame, state, theme, regions.tabs, &strip);
    }
    let (route, terminal_pane) = crate::terminal_view::split(state, regions.session);
    let main = session_main(state, route);
    if main.width < route.width {
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
    }
    render_session(frame, state, theme, main);
    crate::approval_view::render(frame, state, main);
    crate::question_view::render(frame, state, main);
    crate::shell_jobs_view::render(frame, state, main);
    crate::child_view::render(frame, state, main);
    crate::terminal_view::render_composer(frame, state, main, theme);
    crate::terminal_view::render_pane(frame, state, terminal_pane, theme);
    render_devtools(frame, theme, regions.devtools);
    render_toast(frame, state, theme, area);
    crate::dialog::render(frame, state);
}

/// The same Home row allocation is used by drawing and tab hit-testing.
fn shell_regions(state: &TuiState, area: Rect) -> layout::ShellRegions {
    let mut regions = layout::configured_shell_regions(
        area,
        state.chrome.devtools_visible(),
        state.chrome.vertical_tabs_width,
    );
    if state.home && state.tab_presentation().0.is_empty() {
        regions.session = Rect {
            height: area.height.saturating_sub(regions.devtools.height),
            ..area
        };
    }
    regions
}

/// Shared painted rectangles for the strip and nonmodal mouse routing.
pub fn tab_strip(state: &TuiState, area: Rect) -> Option<layout::HorizontalTabStrip> {
    let (tabs, active, can_add) = state.tab_presentation();
    if state.home && tabs.is_empty() {
        return None;
    }
    if let Some(hold) = &state.close_hold
        && hold.area == area
        && hold.until > std::time::Instant::now()
    {
        return Some(hold.strip.clone());
    }
    let region = shell_regions(state, area).tabs;
    if region.width == 0 || region.height == 0 {
        return None;
    }
    if layout::vertical_tabs_width(area.width, state.chrome.vertical_tabs_width) > 0 {
        return Some(layout::vertical_tab_strip(
            region,
            if tabs.is_empty() {
                1
            } else {
                tabs.len() + usize::from(state.home)
            },
            state.tab_scroll.get(),
            !tabs.is_empty() && !state.home && can_add,
        ));
    }
    Some(layout::horizontal_tab_strip(
        region,
        if tabs.is_empty() {
            1
        } else {
            tabs.len() + usize::from(state.home)
        },
        Some(if state.home {
            tabs.len()
        } else if tabs.is_empty() {
            0
        } else {
            active
        }),
        0,
        !tabs.is_empty() && !state.home && can_add,
    ))
}

pub(crate) fn tab_region(state: &TuiState, area: Rect) -> Rect {
    shell_regions(state, area).tabs
}

fn session_main(state: &TuiState, area: Rect) -> Rect {
    let sidebar = !state.home
        && state.parent_id.is_none()
        && !state.chrome.sidebar_hidden
        && layout::sidebar_auto(area.width);
    Rect {
        width: area.width.saturating_sub(if sidebar {
            layout::SESSION_SIDEBAR_WIDTH
        } else {
            0
        }),
        ..area
    }
}

pub(crate) fn prompt_main(state: &TuiState, frame: Rect) -> Rect {
    session_main(
        state,
        crate::terminal_view::split(state, shell_regions(state, frame).session).0,
    )
}

fn session_regions(state: &TuiState, area: Rect, terminal_height: u16) -> layout::SessionRegions {
    if state.approvals.active().is_some() {
        let mut regions = layout::dynamic_session_regions(area, 0, 0);
        regions.transcript.height =
            regions
                .content
                .height
                .saturating_sub(crate::approval_view::inline_height(
                    state,
                    regions.content.width,
                    state.detail_area().width,
                ));
        return regions;
    }
    let input = prompt_lines(state, area.width);
    let jobs_height = crate::shell_jobs_view::height(state)
        .max(crate::child_view::height(state))
        .max(crate::terminal_view::height(state));
    if jobs_height > 0 {
        let mut regions = layout::dynamic_session_regions(area, 0, 0);
        regions.transcript.height = regions.content.height.saturating_sub(jobs_height);
        return regions;
    }
    if state.questions.active().is_some() {
        let mut regions = layout::dynamic_session_regions(area, 0, 0);
        regions.transcript.height =
            regions
                .content
                .height
                .saturating_sub(crate::question_view::height(
                    state,
                    area.width.saturating_sub(2),
                ));
        return regions;
    }
    let input_height = (input.len() as u16)
        .min((terminal_height / 3).max(6))
        .max(1);
    layout::dynamic_session_regions(area, 0, input_height + 3)
}

/// Exactly the rectangle used by `render_transcript`, including rail, sidebar,
/// prompt-height and content padding at this frame size.
pub(crate) fn transcript_area(state: &TuiState, area: Rect) -> Rect {
    if state.home {
        return Rect::default();
    }
    let shell = shell_regions(state, area);
    session_regions(
        state,
        session_main(state, crate::terminal_view::split(state, shell.session).0),
        area.height,
    )
    .transcript
}

/// Single active tab in the horizontal strip
/// (`component/session-tabs.tsx:1506-1508`, `context/session-tabs-model.ts:33-35`).
#[cfg(test)]
fn tab_line(
    theme: &Theme,
    tab_width: u16,
    title: Option<&str>,
    indicators: TabIndicators,
    busy: bool,
) -> Line<'static> {
    deck_tab_line(
        theme,
        tab_width,
        title,
        indicators,
        busy,
        0,
        true,
        false,
        false,
        (false, false),
        None,
        (0, 0.0, 0),
        TabPulseFrame::default(),
        false,
        false,
        2,
    )
}

#[allow(clippy::too_many_arguments)]
fn deck_tab_line(
    theme: &Theme,
    tab_width: u16,
    title: Option<&str>,
    indicators: TabIndicators,
    busy: bool,
    index: usize,
    selected: bool,
    home_slot: bool,
    hovered: bool,
    close: (bool, bool),
    attention: Option<TabAttention>,
    animation: (usize, f32, usize),
    pulse: TabPulseFrame,
    vertical: bool,
    compact: bool,
    number_width: usize,
) -> Line<'static> {
    let title = if home_slot {
        NEW_SESSION_TAB_TITLE
    } else {
        title.unwrap_or(UNTITLED_SESSION)
    };
    let tab_bg = tab_background(theme, selected, hovered, vertical, compact);
    let title_width =
        layout::tab_title_width(tab_width, number_width, vertical, compact, hovered).unwrap_or(0);
    let foreground = if hovered || selected {
        theme.text()
    } else {
        theme.text_muted()
    };
    let overflow = UnicodeWidthStr::width(title) > title_width;
    let visible = marquee_parts(title, title_width, animation.0);
    let used = visible
        .iter()
        .map(|(value, _)| UnicodeWidthStr::width(*value))
        .sum::<usize>();
    // Indicator cell: `numberWidth + 1` with the label right-aligned and one
    // padding cell (`component/session-tabs.tsx:1671-1682`). Status has no
    // idle label; a busy tab uses the first dot-spinner frame even without
    // animations (`TabIndicator`, `spinner-frames.ts`).
    let number = if attention.is_some() {
        theme.hue("accent", 200).unwrap_or(theme.primary())
    } else if busy {
        theme.hue("interactive", 200).unwrap_or(theme.primary())
    } else if hovered && !selected {
        foreground
    } else if selected {
        tint(theme.text(), tab_bg, 0.25)
    } else {
        tint(
            theme
                .color("text.formfield.base")
                .unwrap_or(theme.text_muted()),
            if vertical {
                theme.background_panel()
            } else {
                theme.background()
            },
            0.55,
        )
    };
    let number = if attention.is_none() && !busy {
        tint(
            number,
            theme.hue("accent", 200).unwrap_or(theme.primary()),
            if vertical {
                pulse.number_glow
            } else {
                f32::from(u8::from(pulse.complete))
            },
        )
    } else {
        number
    };
    // U48 onLevel reports the sweep under cell 1, quantized to 1/32. U47
    // caps the number toward white at .15 horizontally / .35 vertically;
    // its independent 700 ms ignition can lift it as far as .85.
    let pulse_width = if vertical {
        tab_width.min(10)
    } else {
        tab_width
    };
    let sweep_level = (pulse.sweep(1.0, pulse_width) * 32.0).round() / 32.0;
    let number = tint(
        number,
        theme.text(),
        pulse
            .whitecap
            .max(if vertical { 0.35 } else { 0.15 } * sweep_level),
    );
    // U47's mounted spinner does not inherit the fallback text attributes;
    // its box padding inherits text.base, while fallback blanks retain canvas white.
    let animated_indicator =
        busy && attention.is_none() && pulse.animations && indicators == TabIndicators::Status;
    let blank = Style::default()
        .fg(if animated_indicator {
            theme.text()
        } else {
            Color::Rgb(255, 255, 255)
        })
        .bg(tab_bg);
    let label = match indicators {
        _ if home_slot => Some("+".to_string()),
        TabIndicators::Numbers => Some((index + 1).to_string()),
        TabIndicators::Status if attention == Some(TabAttention::Permission) => {
            Some("!".to_string())
        }
        TabIndicators::Status if attention == Some(TabAttention::Question) => Some("?".to_string()),
        TabIndicators::Status if busy => Some(TAB_SPINNER_FRAMES[animation.2 % 10].to_string()),
        TabIndicators::Status if compact => Some(
            title
                .trim_start()
                .graphemes(true)
                .next()
                .unwrap_or("U")
                .to_string(),
        ),
        TabIndicators::Status => None,
    };
    let mut spans = Vec::new();
    if let Some(label) = label {
        let color = if compact && selected {
            theme.text()
        } else if compact && indicators == TabIndicators::Status {
            if attention.is_some() {
                theme.hue("accent", 200).unwrap_or(theme.primary())
            } else if busy {
                theme.hue("interactive", 200).unwrap_or(theme.primary())
            } else {
                foreground
            }
        } else {
            number
        };
        let label_width = UnicodeWidthStr::width(label.as_str());
        let leading = if compact {
            (tab_width as usize).saturating_sub(label_width) / 2
        } else {
            number_width.saturating_sub(label_width)
        };
        spans.push(Span::styled(" ".repeat(leading), blank));
        let mut style = Style::default().fg(color).bg(tab_bg);
        if selected && !animated_indicator {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(label, style));
    } else {
        spans.push(Span::styled(" ".repeat(number_width), blank));
    }
    spans.push(Span::styled(" ", blank));
    if compact {
        return Line::from(spans);
    }
    for (index, (grapheme, separator)) in visible.iter().enumerate() {
        // U56 fades by visible grapheme ordinal, not display-cell position.
        let fade = |position: isize| {
            if position <= 0 {
                0.0
            } else {
                0.2 + 0.72 * (position - 1) as f32 / (TAB_TITLE_FADE_WIDTH - 1) as f32
            }
        };
        let opacity = if overflow && title_width > TAB_TITLE_FADE_WIDTH {
            (fade(TAB_TITLE_FADE_WIDTH as isize - index as isize) * animation.1).max(fade(
                index as isize - (visible.len() as isize - TAB_TITLE_FADE_WIDTH as isize) + 1,
            ))
        } else {
            0.0
        };
        let glow = tint(
            tab_bg,
            theme.hue("accent", 200).unwrap_or(theme.primary()),
            if vertical { 0.45 } else { 1.0 } * pulse.dim,
        );
        let title_glow = if vertical {
            pulse.title_glow
        } else {
            f32::from(u8::from(pulse.glows))
        };
        let foreground = tint(
            foreground,
            glow,
            0.12 * title_glow
                * tab_glow_intensity(
                    (number_width + 1 + index) as f32,
                    12.0_f32.min(f32::from(tab_width.saturating_sub(2).max(1))),
                ),
        );
        let faded = tint(foreground, tab_bg, opacity);
        let mut style = Style::default()
            .fg(if *separator {
                tint(faded, tab_bg, 0.55)
            } else {
                faded
            })
            .bg(tab_bg);
        if selected {
            style = style.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled((*grapheme).to_owned(), style));
    }
    let pad = (tab_width as usize).saturating_sub(number_width + 1 + used);
    if hovered && close.0 {
        if pad > 2 {
            spans.push(Span::styled(
                " ".repeat(pad - 2),
                Style::default().bg(tab_bg),
            ));
        }
        spans.push(Span::styled(
            "✕",
            Style::default()
                .fg(if close.1 {
                    theme.text()
                } else if vertical {
                    theme.text_muted()
                } else {
                    tint(theme.text_muted(), theme.text(), 0.6)
                })
                .bg(tab_bg),
        ));
        spans.push(Span::styled(" ", Style::default().bg(tab_bg)));
    } else if pad > 0 {
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(tab_bg)));
    }
    Line::from(spans)
}

fn tab_background(
    theme: &Theme,
    selected: bool,
    hovered: bool,
    vertical: bool,
    compact: bool,
) -> Color {
    if vertical {
        if selected && !compact {
            theme
                .color("background.action.primary.$selected")
                .unwrap_or(theme.background_panel())
        } else if hovered || selected {
            theme
                .color("background.raised.high")
                .unwrap_or(theme.background_panel())
        } else {
            theme.background_panel()
        }
    } else if selected {
        theme.decrease(theme.background_panel())
    } else if hovered {
        theme
            .color("background.action.primary.$hovered")
            .unwrap_or(theme.background())
    } else {
        theme.background()
    }
}

/// U57 display-cell cursor skips a complete wide grapheme when the cursor is
/// inside it. Only the generated gap dot carries separator tint metadata.
fn marquee_parts(title: &str, width: usize, offset: usize) -> Vec<(&str, bool)> {
    if width == 0 {
        return Vec::new();
    }
    let scrolling = offset > 0 && UnicodeWidthStr::width(title) > width;
    let mut parts: Vec<_> = title.graphemes(true).map(|value| (value, false)).collect();
    if scrolling {
        parts.extend([(" ", false), ("·", true), (" ", false)]);
    }
    let cursor = if scrolling {
        offset % (UnicodeWidthStr::width(title) + 3)
    } else {
        0
    };
    let mut traversed = 0;
    let mut used = 0;
    let mut visible = Vec::new();
    for (value, separator) in parts.iter().chain(parts.iter()).copied() {
        let cells = UnicodeWidthStr::width(value);
        if traversed < cursor {
            traversed += cells;
            continue;
        }
        if used + cells > width {
            break;
        }
        visible.push((value, separator));
        used += cells;
        if !scrolling && visible.len() == parts.len() {
            break;
        }
    }
    visible
}

fn render_deck_tabs(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &Theme,
    area: Rect,
    strip: &layout::HorizontalTabStrip,
) {
    if area.height > 1 {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.background_panel())),
            area,
        );
    }
    let (tabs, active, _) = state.tab_presentation();
    for marker in [strip.before_marker, strip.after_marker]
        .into_iter()
        .flatten()
    {
        let hidden = if Some(marker) == strip.before_marker {
            strip.before
        } else {
            strip.after
        };
        let text = if Some(marker) == strip.before_marker {
            format!("‹{hidden}")
        } else {
            format!(" {hidden}›")
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(theme.text_muted())),
            marker,
        );
    }
    for tab in &strip.tabs {
        if tab.rect.width == 0 {
            continue;
        }
        let home_slot = state.home && tab.index == tabs.len();
        let presentation = tabs.get(tab.index);
        let selected = if state.home {
            home_slot
        } else {
            tab.index == active
        };
        let hovered = state.hovered_tab(frame.area()) == Some(tab.index);
        let close = state.tab_close_cell(frame.area(), tab.index, tab.rect);
        let attention = state.tab_attention_for(tab.index);
        frame.render_widget(
            Paragraph::new(deck_tab_line(
                theme,
                tab.rect.width,
                Some(state.tab_title(tab.index)),
                state.chrome.tab_indicators,
                state.tab_busy(tab.index),
                tab.index,
                selected,
                home_slot || presentation.is_some_and(|p| p.home),
                hovered,
                (
                    close.is_some(),
                    close.is_some()
                        && state.mouse_position().is_some_and(|(x, y, area)| {
                            area == frame.area()
                                && y == tab.rect.y
                                && Some(x) == layout::tab_close_cell(tab.rect)
                        }),
                ),
                attention,
                state.tab_animation(tab.index),
                state.tab_pulse(tab.index),
                strip.vertical,
                strip.compact,
                state.tab_number_width(),
            ))
            .style(Style::default().bg(tab_background(
                theme,
                selected,
                hovered,
                strip.vertical,
                strip.compact,
            ))),
            tab.rect,
        );
        if strip.vertical && !strip.compact && tab.rect.height > 1 {
            let detail = presentation
                .and_then(|tab| tab.detail.as_deref())
                .unwrap_or("");
            let width = layout::tab_title_width(
                tab.rect.width,
                state.tab_number_width(),
                true,
                false,
                hovered,
            )
            .unwrap_or(0);
            let parts = marquee_parts(detail, width, 0);
            let bg = tab_background(theme, selected, hovered, true, false);
            let fg = tint(theme.text_muted(), bg, 0.35);
            let overflow = UnicodeWidthStr::width(detail) > width && width > TAB_TITLE_FADE_WIDTH;
            let mut spans = vec![Span::styled(
                " ".repeat(state.tab_number_width() + 1),
                Style::default().bg(bg),
            )];
            spans.extend(parts.iter().enumerate().map(|(index, (part, _))| {
                let end =
                    index as isize - (parts.len() as isize - TAB_TITLE_FADE_WIDTH as isize) + 1;
                let fade = if overflow && end > 0 {
                    0.2 + 0.72 * (end - 1) as f32 / (TAB_TITLE_FADE_WIDTH - 1) as f32
                } else {
                    0.0
                };
                Span::styled(
                    (*part).to_owned(),
                    Style::default().fg(tint(fg, bg, fade)).bg(bg),
                )
            }));
            frame.render_widget(
                Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
                Rect::new(tab.rect.x, tab.rect.y + 1, tab.rect.width, 1),
            );
        }
        if !strip.compact {
            paint_tab_pulse(
                frame,
                tab.rect,
                theme,
                state.tab_pulse(tab.index),
                strip.vertical,
            );
        }
    }
    if let Some(add) = strip.add.filter(|rect| strip.vertical || rect.width == 3) {
        // session-tabs.tsx:1744-1752: the entire " + " control, including
        // padding, shares the hovered text and action background tokens.
        let hovered = state.tab_add_hovered(frame.area(), add);
        frame.render_widget(
            Paragraph::new(if strip.vertical && !strip.compact {
                " + New session"
            } else {
                " + "
            })
            .alignment(if strip.compact {
                Alignment::Center
            } else {
                Alignment::Left
            })
            .style(
                Style::default()
                    .fg(if hovered {
                        theme.text()
                    } else {
                        theme.text_muted()
                    })
                    .bg(if hovered {
                        theme
                            .color("background.action.primary.$hovered")
                            .unwrap_or(theme.background())
                    } else {
                        theme.background()
                    }),
            ),
            add,
        );
    }
}

/// U48 mixes glow → sweep → edge flash → completion in floating point. Round
/// only the final truecolor cell, not each intermediate blend stage.
fn blend_tab_pulse_color(background: Color, layers: [(Color, f32, f32); 4]) -> Color {
    let Color::Rgb(r, g, b) = background else {
        return background;
    };
    let mut channels = [f32::from(r), f32::from(g), f32::from(b)];
    let base = channels;
    for (color, stop, opacity) in layers {
        let Color::Rgb(r, g, b) = color else { continue };
        for ((channel, base), overlay) in channels.iter_mut().zip(base).zip([r, g, b]) {
            let overlay = base + (f32::from(overlay) - base) * stop;
            *channel += (overlay - *channel) * opacity;
        }
    }
    Color::Rgb(
        channels[0].round() as u8,
        channels[1].round() as u8,
        channels[2].round() as u8,
    )
}

fn paint_tab_pulse(
    frame: &mut Frame<'_>,
    rect: Rect,
    theme: &Theme,
    pulse: TabPulseFrame,
    vertical: bool,
) {
    let width = if vertical {
        rect.width.min(10)
    } else {
        rect.width
    };
    for dy in 0..if vertical { rect.height.min(2) } else { 1 } {
        for dx in 0..width {
            let cell = &mut frame.buffer_mut()[(rect.x + dx, rect.y + dy)];
            let background = cell.bg;
            let accent = theme.hue("accent", 200).unwrap_or(theme.primary());
            let (running, flash_stop, glow) = if !vertical {
                (0.45, 0.65, pulse.dim)
            } else if dy == 0 {
                (0.25, 0.7, 0.45 * pulse.dim)
            } else {
                (0.13, 0.42, 0.25 * pulse.dim)
            };
            let flash_opacity = pulse.flash
                * if vertical {
                    tab_glow_intensity(
                        f32::from(dx),
                        8.0_f32.min(f32::from(width.saturating_sub(2).max(1))),
                    )
                } else {
                    1.0
                };
            cell.bg = blend_tab_pulse_color(
                background,
                [
                    (
                        accent,
                        glow,
                        pulse.glow_at(f32::from(dx), width, if dy == 0 { 12.0 } else { 10.0 }),
                    ),
                    (
                        theme.text(),
                        running,
                        0.14 * pulse.sweep(f32::from(dx), width),
                    ),
                    (theme.text(), flash_stop, flash_opacity),
                    (accent, if vertical { glow } else { 1.0 }, pulse.completion),
                ],
            );
        }
    }
}

#[cfg(test)]
fn render_single_tab(
    frame: &mut Frame<'_>,
    theme: &Theme,
    area: Rect,
    strip: &layout::HorizontalTabStrip,
    title: Option<&str>,
    indicators: TabIndicators,
    busy: bool,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    if area.height > 1 {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme.background_panel())),
            area,
        );
    }
    if let Some(tab) = strip.tabs.first().filter(|tab| tab.rect.width > 0) {
        frame.render_widget(
            Paragraph::new(tab_line(theme, tab.rect.width, title, indicators, busy)),
            tab.rect,
        );
    }
}

#[cfg(test)]
fn render_tabs(
    frame: &mut Frame<'_>,
    theme: &Theme,
    area: Rect,
    title: Option<&str>,
    indicators: TabIndicators,
    busy: bool,
) {
    let strip = layout::horizontal_tab_strip(area, 1, Some(0), 0, false);
    render_single_tab(frame, theme, area, &strip, title, indicators, busy);
}

fn render_session(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    if state.home {
        render_home(frame, state, theme, area);
        return;
    }
    let regions = session_regions(state, area, frame.area().height);
    render_transcript(frame, state, regions.transcript, frame.area().width);
    if state.approvals.active().is_some() || state.questions.active().is_some() {
        return;
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
    render_slash(frame, state, theme, regions.prompt);
    render_mentions(frame, state, theme, regions.prompt);
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
                    .map(|(text, selected, _)| {
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
    let title = wrap_sidebar_title(
        state.session_title.as_deref().unwrap_or(UNTITLED_SESSION),
        inner.width.saturating_sub(2) as usize,
    );
    let title_style = Style::default()
        .fg(theme.text())
        .add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line<'static>> = title
        .into_iter()
        .map(|(t, wrapped_at_space)| {
            let mut line = Line::styled(t, title_style);
            if wrapped_at_space {
                // The title_shimmer renders one bold text node: the original
                // separator before the next word remains on the preceding row.
                line.spans.push(Span::styled(" ", title_style));
            }
            line
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
            lines.push(Line::styled(
                format!("{} tokens", thousands(tokens)),
                Style::default().fg(theme.text_muted()),
            ));
            lines.push(Line::styled(
                limit.map_or_else(
                    || "Limit unknown".into(),
                    |l| {
                        format!(
                            "{}% used",
                            (tokens as f64 / l as f64 * 100.0).round() as u64
                        )
                    },
                ),
                Style::default().fg(theme.text_muted()),
            ));
        }
        None => lines.push(Line::styled(
            "Usage unknown",
            Style::default().fg(theme.text_muted()),
        )),
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if inner.height > 0 {
        frame.render_widget(
            Paragraph::new(Line::styled(
                compact_path(
                    state
                        .chrome
                        .location
                        .as_deref()
                        .unwrap_or("Location unknown"),
                    inner.width as usize,
                ),
                Style::default().fg(theme.text_muted()),
            )),
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
    // Home's two flex spacers surround the top spacer, logo, prompt and
    // footer. The pinned footer mounts at 44 columns, but its version content
    // starts at 64; with no other footer items this leaves one less occupied
    // row at 44..63 (home.tsx and feature-plugins/home/footer.tsx).
    let empty_footer =
        area.height >= 16 && (44..64).contains(&area.width) && state.mcp_status_counts().is_none();
    // The upstream 3-row spacer may flex-shrink to zero before the logo when
    // the prompt, underline and footer consume the entire short viewport.
    let top_spacer = 3.min(area.height.saturating_sub(h + logo_height + 4));
    let y = area.y
        + area
            .height
            .saturating_sub(h + logo_height + 9 - u16::from(empty_footer))
            / 2
        + top_spacer;
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
    render_slash(frame, state, theme, body);
    render_mentions(frame, state, theme, body);
    if let Some((rect, line)) = home_mcp_slot(state, theme, area) {
        frame.render_widget(
            Paragraph::new(line).style(
                Style::default()
                    .fg(Color::Rgb(255, 255, 255))
                    .bg(theme.background()),
            ),
            rect,
        );
    }
    // The pinned Home footer exists at 44x12 but its version slot is shown
    // only at widths >= 64 (`homeFooterVisibility`). Keep the real package
    // version at wider sizes instead of substituting the reference identity.
    if area.height >= 12 && area.width >= 64 {
        let version = env!("CARGO_PKG_VERSION");
        let row_width = area.width.saturating_sub(2);
        let version_width = version.len() as u16;
        frame.render_widget(
            Paragraph::new(version).style(Style::default().fg(theme.text_muted())),
            Rect::new(
                area.x + row_width.saturating_sub(version_width),
                home_footer_y(area),
                row_width.min(version_width),
                1,
            ),
        );
    }
}

/// Pinned Home footer's live MCP item; command visibility follows viewport width.
fn home_mcp_slot(state: &TuiState, theme: &Theme, area: Rect) -> Option<(Rect, Line<'static>)> {
    if !state.home || area.height < 12 || area.width < 44 {
        return None;
    }
    let (connected, failed) = state.mcp_status_counts()?;
    let (label, color) = if failed > 0 {
        (format!("{failed} MCP failed"), theme.error())
    } else {
        (
            format!("{connected} MCP"),
            if connected > 0 {
                theme.success()
            } else {
                theme.text_muted()
            },
        )
    };
    let mut spans = vec![
        Span::styled("⊙ ", Style::default().fg(color)),
        Span::styled(label, Style::default().fg(theme.text())),
    ];
    if area.width >= 64 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            "/mcps",
            Style::default().fg(theme.text_muted()),
        ));
    }
    let line = Line::from(spans);
    let rect = Rect::new(area.x + 2, home_footer_y(area), line.width() as u16, 1);
    Some((rect, line))
}

fn home_footer_y(area: Rect) -> u16 {
    // Pinned footer drops its top/bottom padding below 16 terminal rows.
    area.bottom() - if area.height < 16 { 1 } else { 2 }
}

/// The exact drawn item, not the whole footer row, owns MCP modal activation.
pub(crate) fn home_mcp_rect(state: &TuiState, frame: Rect) -> Option<Rect> {
    home_mcp_slot(state, Theme::dark(), prompt_main(state, frame)).map(|(rect, _)| rect)
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
    let (lines, total, scroll, targets) =
        state.visible_transcript_at_viewport_with_targets(area.width, terminal_width, area.height);
    state.observe_transcript_viewport(area.width, terminal_width, area.height, total, scroll);
    let lines =
        state.paint_transcript_at(area, &lines, total, scroll, Some(frame.area()), &targets);
    let text = crate::styled::Lines::from(lines).into_text();
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

/// `autocomplete.tsx:822-825,851-952`: up to ten rows above the prompt,
/// split side borders, raised surface and the focused primary-action row.
fn render_slash(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, body: Rect) {
    let Some(options) = state.slash_options() else {
        return;
    };
    let height = (options.len().clamp(1, 10) as u16).min(body.y.saturating_sub(frame.area().y));
    let area = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
    if area.width < 3 || area.height == 0 {
        return;
    }
    let bg = theme.background_raised_high();
    // Background styles do not replace symbols already painted by the Home logo.
    frame.render_widget(Clear, area);
    frame.render_widget(Block::default().style(Style::default().bg(bg)), area);
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_set(border::Set {
                vertical_left: "┃",
                vertical_right: "┃",
                ..border::PLAIN
            })
            .border_style(Style::default().fg(theme.border())),
        area,
    );
    let selected = state.slash_selected(options.len());
    let offset = selected.saturating_sub(area.height as usize - 1);
    for (row, option) in options
        .iter()
        .skip(offset)
        .take(area.height as usize)
        .enumerate()
    {
        let focused = row + offset == selected;
        let style = if focused {
            Style::default()
                .fg(theme
                    .color("text.action.primary.$focused")
                    .unwrap_or(theme.text()))
                .bg(theme
                    .color("background.action.primary.$focused")
                    .unwrap_or(bg))
        } else {
            Style::default().fg(theme.text()).bg(bg)
        };
        let rect = Rect::new(area.x + 1, area.y + row as u16, area.width - 2, 1);
        frame.render_widget(Block::default().style(style), rect);
        let label = format!(
            " /{}{}",
            option.name,
            " ".repeat(
                option
                    .display_width
                    .saturating_sub(1 + UnicodeWidthStr::width(option.name.as_str()))
            )
        );
        let label = clip_placeholder(&label, rect.width as usize);
        let remaining = (rect.width as usize).saturating_sub(UnicodeWidthStr::width(label));
        let description = format!(" {}", option.description);
        let description = clip_placeholder(&description, remaining);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(label, style),
                Span::styled(
                    description,
                    if focused {
                        style
                    } else {
                        style.fg(theme.text_muted())
                    },
                ),
            ]))
            .style(style),
            rect,
        );
    }
    if options.is_empty() {
        frame.render_widget(
            Paragraph::new(" No matching commands")
                .style(Style::default().fg(theme.text_muted()).bg(bg)),
            Rect::new(area.x + 1, area.y, area.width - 2, 1),
        );
    }
}

/// Bounded, owner-provided Location-relative paths above either composer.
fn render_mentions(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, body: Rect) {
    let Some(options) = state.mention_options() else {
        return;
    };
    let height = (options.paths.len().clamp(1, crate::app::MENTION_LIMIT) as u16)
        .min(body.y.saturating_sub(frame.area().y));
    let area = Rect::new(body.x, body.y.saturating_sub(height), body.width, height);
    if area.width < 3 || area.height == 0 {
        return;
    }
    let bg = theme.background_raised_high();
    // SplitBorder paints on the root surface, around the raised scrollbox.
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background())),
        area,
    );
    frame.render_widget(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT)
            .border_set(border::Set {
                vertical_left: "┃",
                vertical_right: "┃",
                ..border::PLAIN
            })
            .border_style(Style::default().fg(theme.border()).bg(theme.background())),
        area,
    );
    let selected = state.mention_selected(options.paths.len());
    let offset = selected.saturating_sub(area.height as usize - 1);
    for (row, path) in options
        .paths
        .iter()
        .skip(offset)
        .take(area.height as usize)
        .enumerate()
    {
        let (fg, fill) = if row + offset == selected {
            (
                theme
                    .color("text.action.primary.$focused")
                    .unwrap_or(theme.text()),
                theme
                    .color("background.action.primary.$focused")
                    .unwrap_or(bg),
            )
        } else {
            (theme.text(), bg)
        };
        let rect = Rect::new(area.x + 1, area.y + row as u16, area.width - 2, 1);
        // OpenTUI box padding and flexGrow inherit the terminal's default
        // foreground, while the text child alone gets the focused text color.
        frame.render_widget(Block::default().style(Style::default().bg(fill)), rect);
        let content = clip_placeholder(path, rect.width.saturating_sub(1) as usize);
        frame.render_widget(
            Paragraph::new(content).style(Style::default().fg(fg).bg(fill)),
            Rect::new(
                rect.x + 1,
                rect.y,
                UnicodeWidthStr::width(content) as u16,
                1,
            ),
        );
    }
    if options.paths.is_empty() {
        frame.render_widget(
            Paragraph::new(" No matching files")
                .style(Style::default().fg(theme.text_muted()).bg(bg)),
            Rect::new(area.x + 1, area.y, area.width - 2, 1),
        );
    }
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
    let border_style = Style::default().fg(if state.leader_pending() {
        theme.border()
    } else {
        state
            .active_agent()
            .map_or(theme.border(), |a| state.agent_color(Some(a)))
    });
    let input_fg = if state.leader_pending() {
        theme.text_muted()
    } else {
        theme.text()
    };
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
            let visible = body.height.saturating_sub(3) as usize;
            let start = caret
                .0
                .saturating_sub(visible.saturating_sub(1))
                .min(input_rows.len().saturating_sub(visible));
            state.observe_prompt_paint(
                frame.area(),
                Rect {
                    height: body.height.saturating_sub(3),
                    ..row(1)
                },
                start,
                &input_rows,
            );
            let input_lines: Vec<_> = input_rows
                .into_iter()
                .map(|row| {
                    crate::styled::Line::new(
                        row.spans
                            .into_iter()
                            .zip(row.chip_spans)
                            .map(|((text, selected, mentioned), chip)| {
                                crate::styled::Span::styled(
                                    text,
                                    if chip {
                                        let style = Style::default()
                                            .fg(theme.background())
                                            .bg(theme.warning())
                                            .add_modifier(Modifier::BOLD);
                                        if selected {
                                            style.add_modifier(Modifier::REVERSED)
                                        } else {
                                            style
                                        }
                                    } else if selected {
                                        Style::default()
                                            .fg(input_fg)
                                            .add_modifier(Modifier::REVERSED)
                                    } else if mentioned {
                                        // `generateSyntax`: extmark.file uses
                                        // text.feedback.warning.base + bold.
                                        Style::default()
                                            .fg(theme.warning())
                                            .add_modifier(Modifier::BOLD)
                                    } else {
                                        Style::default().fg(input_fg)
                                    },
                                )
                            })
                            .collect(),
                    )
                })
                .collect();
            frame.render_widget(
                Paragraph::new(
                    crate::styled::Lines::from(input_lines[start..].to_vec()).into_text(),
                )
                .style(Style::default().bg(prompt_bg)),
                Rect {
                    height: body.height.saturating_sub(3),
                    ..row(1)
                },
            );
            if state.home && state.input().is_empty() && visible > 0 && text_width > 0 {
                // `routes/home.tsx:19-23` + `component/prompt/index.tsx:1583-1595`.
                let hint = format!("Ask anything… \"{}\"", state.home_example);
                let hint = clip_placeholder(&hint, text_width as usize);
                let hint_rect = Rect {
                    width: UnicodeWidthStr::width(hint) as u16,
                    ..row(1)
                };
                frame.render_widget(
                    Block::default()
                        .style(Style::default().fg(Color::Rgb(255, 255, 255)).bg(prompt_bg)),
                    row(1),
                );
                frame.render_widget(
                    Paragraph::new(hint)
                        .style(Style::default().fg(theme.text_muted()).bg(prompt_bg)),
                    hint_rect,
                );
            }
            if visible > 0
                && text_width > 0
                && !state.terminal_focused()
                && *state.panel() == crate::app::TuiPanel::None
            {
                frame.set_cursor_position((
                    text_x + (caret.1 as u16).min(text_width - 1),
                    body.y + 1 + caret.0.saturating_sub(start) as u16,
                ));
            }
        }
        if body.height > 3 {
            let metadata = row(body.height - 1);
            let upstream_limit =
                terminal_width.saturating_sub(if terminal_width < 44 { 9 } else { 13 });
            if let Some(line) = metadata_line(
                state,
                theme,
                metadata.width.min(upstream_limit),
                terminal_width,
            ) {
                let text_width = (line.width() as u16).min(metadata.width);
                frame.render_widget(
                    Block::default()
                        .style(Style::default().fg(Color::Rgb(255, 255, 255)).bg(prompt_bg)),
                    metadata,
                );
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().bg(prompt_bg)),
                    Rect {
                        width: text_width,
                        ..metadata
                    },
                );
            }
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

/// A clipped placeholder must never leave a partial extended grapheme at the edge.
fn clip_placeholder(text: &str, width: usize) -> &str {
    let mut used = 0;
    let mut end = 0;
    for (offset, grapheme) in text.grapheme_indices(true) {
        let cells = UnicodeWidthStr::width(grapheme);
        if used + cells > width {
            break;
        }
        used += cells;
        end = offset + grapheme.len();
    }
    &text[..end]
}

/// Prompt metadata candidates (`component/prompt/metadata.tsx:101-146`) fit
/// the painted text rectangle; the 44-column breakpoint uses terminal width.
fn metadata_line(
    state: &TuiState,
    theme: &Theme,
    width: u16,
    terminal_width: u16,
) -> Option<Line<'static>> {
    let agent = layout::shows_agent_metadata(terminal_width)
        .then(|| state.active_agent())
        .flatten();
    let agent_label = agent.map(crate::messages::Locale::titlecase);
    let display_agent = agent_label.as_deref();
    let model = state.active_model_label();
    let provider = layout::shows_agent_metadata(terminal_width)
        .then(|| state.active_provider())
        .flatten()
        .unwrap_or_default();
    let variant = model.as_ref().and_then(|(_, variant)| variant.clone());
    if agent.is_none() && model.is_none() {
        return None;
    }
    let auto = layout::shows_agent_metadata(terminal_width)
        && state.auto_accept == oc_core::queries::AutoAcceptState::Enabled;
    let mut model_label = model
        .as_ref()
        .map_or("", |(name, _)| name.as_str())
        .to_string();
    let short_provider = provider.rsplit(" / ").next().unwrap_or(provider);
    let fits = |auto, provider: &str| {
        let mut parts = Vec::new();
        if let Some(agent) = display_agent.filter(|agent| !agent.is_empty()) {
            parts.push(agent);
        }
        if auto {
            parts.push("auto");
        }
        if !model_label.is_empty() {
            if agent.is_some() {
                parts.push("·");
            }
            parts.push(model_label.as_str());
        }
        if !provider.is_empty() {
            parts.push(provider);
        }
        if let Some(variant) = variant.as_deref().filter(|variant| !variant.is_empty()) {
            parts.extend(["·", variant]);
        }
        UnicodeWidthStr::width(parts.join(" ").as_str()) <= width as usize
    };
    let (show_auto, provider) = [
        (auto, provider),
        (false, provider),
        (false, short_provider),
        (false, ""),
    ]
    .into_iter()
    .find(|(auto, provider)| fits(*auto, provider))
    .unwrap_or_else(|| {
        // `Locale.truncateWidth`, including the upstream minimum of nine
        // cells and trimming whitespace just before the ellipsis.
        let prefix = display_agent.map_or(0, |agent| UnicodeWidthStr::width(agent) + 3);
        let suffix = variant
            .as_ref()
            .map_or(0, |variant| UnicodeWidthStr::width(variant.as_str()) + 3);
        let available = (width as usize).saturating_sub(prefix + suffix).max(9);
        if UnicodeWidthStr::width(model_label.as_str()) > available {
            let mut used = 0;
            let mut end = 0;
            for (offset, grapheme) in model_label.grapheme_indices(true) {
                let cells = UnicodeWidthStr::width(grapheme);
                if used + cells > available - 1 {
                    break;
                }
                used += cells;
                end = offset + grapheme.len();
            }
            model_label = format!("{}…", model_label[..end].trim_end());
        }
        (false, "")
    });
    let muted = Style::default().fg(theme.text_muted());
    let text = Style::default().fg(if state.leader_pending() {
        theme.text_muted()
    } else {
        theme.text()
    });
    let gap = Style::default().fg(Color::Rgb(255, 255, 255));
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(agent) = agent {
        spans.push(Span::styled(
            agent_label.clone().unwrap_or_default(),
            Style::default().fg(if state.leader_pending() {
                theme.border()
            } else {
                state.agent_color(Some(agent))
            }),
        ));
    }
    if show_auto {
        if !spans.is_empty() {
            spans.push(Span::styled(" ", gap));
        }
        spans.push(Span::styled("auto", muted));
    }
    if model.is_some() {
        if agent.is_some() {
            spans.push(Span::styled(" ", gap));
            spans.push(Span::styled("·", muted));
            spans.push(Span::styled(" ", gap));
        }
        spans.push(Span::styled(model_label, text));
    }
    if !provider.is_empty() {
        spans.push(Span::styled(" ", gap));
        spans.push(Span::styled(provider.to_string(), muted));
    }
    if let Some(variant) = variant {
        spans.push(Span::styled(" ", gap));
        spans.push(Span::styled("·", muted));
        spans.push(Span::styled(" ", gap));
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

/// Keep the shrinkable left footer slot within its cell budget without
/// splitting a styled span's last visible grapheme.
fn clip_footer_spans(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    let mut remaining = width;
    let mut clipped = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        if span.width() <= remaining {
            remaining -= span.width();
            clipped.push(span);
            continue;
        }
        let mut end = 0;
        for (start, grapheme) in span.content.grapheme_indices(true) {
            let cells = UnicodeWidthStr::width(grapheme);
            if cells > remaining {
                break;
            }
            remaining -= cells;
            end = start + grapheme.len();
        }
        if end > 0 {
            clipped.push(Span::styled(span.content[..end].to_string(), span.style));
        }
        break;
    }
    clipped
}

fn footer_line(
    state: &TuiState,
    theme: &Theme,
    layout_width: u16,
    terminal_width: u16,
) -> Line<'static> {
    let muted = Style::default().fg(theme.text_muted());
    let mut hints = Vec::new();
    let commands_binding = state
        .chrome
        .command_palette_shortcut
        .as_deref()
        .unwrap_or(COMMANDS_HINT.0)
        .split(',')
        .next()
        .unwrap_or_default()
        .trim();
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
            text_width(&usage) + 4 + text_width(commands_binding) + text_width(COMMANDS_HINT.1)
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
    if commands_visible && !commands_binding.is_empty() {
        hints.extend([
            Span::raw("  "),
            Span::styled(
                format!("{commands_binding} "),
                Style::default().fg(theme.text()),
            ),
            Span::styled(COMMANDS_HINT.1, muted),
        ]);
    }
    let service_start = hints.len();
    let issues = state.service_issue_count();
    if state.preview_limited() {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        hints.push(Span::styled("Preview limited", muted));
    }
    if issues > 0 {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        hints.push(Span::styled(
            format!("{issues} issue{}", if issues == 1 { "" } else { "s" }),
            Style::default().fg(theme.warning()),
        ));
    }
    let pending = state.service_pending_count();
    if pending > 0 {
        if !hints.is_empty() {
            hints.push(Span::raw("  "));
        }
        hints.push(Span::styled(format!("{pending} pending"), muted));
    }
    let mut hints = Line::from(hints);
    let hints_visible = !hints.spans.is_empty();
    let mut left_width =
        (layout_width as usize).saturating_sub(hints.width() + usize::from(hints_visible) * 2);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if let Some(notice) = state.retry_notice() {
        spans.push(Span::styled(notice, Style::default().fg(theme.warning())));
    } else if state.status() == &TuiStatus::Streaming {
        spans.push(Span::raw(" ")); // prompt/index.tsx:1887 marginLeft=1
        if state.chrome.animations == Some(false) {
            spans.push(Span::styled("[⋯]", muted));
        } else {
            let color = state
                .active_agent()
                .map_or(theme.border(), |agent| state.agent_color(Some(agent)));
            spans.extend(crate::scanner::spans(
                state.scanner_frame(),
                color,
                theme.background(),
            ));
        }
        spans.push(Span::raw(" ")); // running row gap=1
        let (key, label) = ESC_INTERRUPT;
        if state.interrupt_armed() {
            // The pinned prompt uses warning for both spans while armed; its
            // 220ms flash is a separate time-dependent color transition.
            spans.push(Span::styled(key, Style::default().fg(theme.warning())));
            spans.push(Span::styled(
                format!("again to {label}"),
                Style::default().fg(theme.warning()),
            ));
        } else {
            spans.push(Span::styled(key, Style::default().fg(theme.text())));
            spans.push(Span::styled(label, Style::default().fg(theme.text_muted())));
        }
    } else if let Some(notice) = state.dcp.notice() {
        spans.push(Span::styled(
            notice.to_string(),
            Style::default().fg(theme.info()),
        ));
    } else if let Some(location) = &state.chrome.location {
        spans.push(Span::styled(compact_path(location, left_width), muted));
    }
    if hints.spans.len() > service_start
        && (state.status() == &TuiStatus::Streaming || state.retry_notice().is_some())
        && spans.iter().map(Span::width).sum::<usize>() + hints.width() + 2 > layout_width as usize
    {
        // Ancillary navigation/usage hints may yield to current service status,
        // but status must never hide the foreground interrupt/retry feedback.
        let mut service_hints = hints.spans.split_off(service_start);
        if service_start > 0 {
            service_hints.remove(0); // the owned separator from preceding hints
        }
        hints = Line::from(service_hints);
        left_width = (layout_width as usize).saturating_sub(hints.width() + 2);
    }
    if state.status() == &TuiStatus::Streaming {
        // In prompt/index.tsx the running slot has flexShrink and minWidth=0;
        // the shortcuts keep their width when the row is narrow.
        spans = clip_footer_spans(spans, left_width);
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
/// absolute top-right, content-sized up to `min(60,width-6)`, side borders
/// in the variant color, raised-high interior with 2/2/1/1 padding.
/// The titleless row adds two cells before the muted close glyph (`:71-79`).
pub(crate) fn toast_rect(state: &TuiState, area: Rect) -> Option<Rect> {
    let message = state.note()?;
    let max_width = TOAST_MAX_WIDTH.min(area.width.saturating_sub(6));
    if max_width < 11 || area.height < 4 {
        return None;
    }
    // Side borders + 2/2 padding + 2-cell gap + one close glyph.
    let max_text_width = max_width - 9;
    let lines = wrap_text(message, max_text_width as usize);
    let content_width = lines.iter().map(|line| text_width(line)).max().unwrap_or(0);
    // A wide grapheme can exceed a one-cell wrap budget on tiny terminals.
    let width = (content_width as u16).saturating_add(9).min(max_width);
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(TOAST_RIGHT_MARGIN + width));
    let y = area.y.saturating_add(1);
    let height = (lines.len() as u16)
        .saturating_add(2)
        .min(area.height.saturating_sub(1));
    Some(Rect::new(x, y, width, height))
}

fn render_toast(frame: &mut Frame<'_>, state: &TuiState, theme: &Theme, area: Rect) {
    let Some(rect) = toast_rect(state, area) else {
        // At tiny widths there is no room for both text and close affordance.
        if let Some(message) = state.note()
            && area.width > 0
            && area.height > 0
        {
            frame.render_widget(
                Paragraph::new(clip_placeholder(message, area.width as usize))
                    .style(Style::default().fg(theme.warning()).bg(theme.background())),
                Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
        }
        return;
    };
    let message = state.note().expect("toast rect requires note");
    let color = match state.note_variant().expect("toast rect requires variant") {
        NoteVariant::Info => theme.info(),
        NoteVariant::Success => theme.success(),
        NoteVariant::Warning => theme.warning(),
        NoteVariant::Error => theme.error(),
    };
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .padding(Padding::new(2, 2, 1, 1))
        .border_set(border::Set {
            vertical_left: "┃",
            vertical_right: "┃",
            ..border::PLAIN
        })
        // Preserve the background under each border, but not underlay text
        // modifiers (a long bold sidebar title can lie beneath this toast).
        .border_style(Style::default().fg(color).remove_modifier(Modifier::all()));
    let text_width = rect.width.saturating_sub(9);
    let wrapped: Vec<Line<'static>> = wrap_text(message, text_width as usize)
        .into_iter()
        .map(Line::from)
        .collect();
    frame.render_widget(block, rect);
    // Interior surface (inside the side borders, padding included), upstream
    // `background.raised.high` (`ui/toast.tsx:64-70`).
    let interior = Rect::new(
        rect.x.saturating_add(1),
        rect.y,
        rect.width.saturating_sub(2),
        rect.height,
    );
    frame.render_widget(Clear, interior);
    // OpenTUI blank canvas cells use truecolor white, including toast padding.
    frame.render_widget(
        Block::default().style(
            Style::default()
                .fg(Color::Rgb(255, 255, 255))
                .bg(theme.background_raised_high()),
        ),
        interior,
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
    frame.render_widget(
        Paragraph::new("x").style(Style::default().fg(theme.text_muted())),
        Rect::new(rect.right().saturating_sub(4), rect.y + 1, 1, 1),
    );
}

/// Greedy word wrap at `width` display cells, upstream toast `wrapMode="word"`
/// (`ui/toast.tsx:75`). Words longer than the width are split so no bounded
/// notice is silently dropped; returns at least one (possibly empty) line.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    wrap_text_with_breaks(text, width)
        .into_iter()
        .map(|(line, _)| line)
        .collect()
}

/// Whether the next row starts after a source-space rather than a hard word split.
fn wrap_text_with_breaks(text: &str, width: usize) -> Vec<(String, bool)> {
    if width == 0 {
        return vec![(String::new(), false)];
    }
    let mut lines: Vec<(String, bool)> = Vec::new();
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
            lines.push((std::mem::take(&mut current), true));
        }
        let mut chunk = String::new();
        let mut chunk_width = 0usize;
        for ch in word.chars() {
            let char_width = text_width(&ch.to_string());
            if chunk_width + char_width > width && !chunk.is_empty() {
                lines.push((std::mem::take(&mut chunk), false));
                chunk_width = 0;
            }
            chunk.push(ch);
            chunk_width += char_width;
        }
        current = chunk;
        current_width = chunk_width;
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push((current, false));
    }
    lines
}

/// Sidebar titles also wrap after a hyphen. Only whitespace wraps carry a
/// styled separator cell into the preceding row.
fn wrap_sidebar_title(text: &str, width: usize) -> Vec<(String, bool)> {
    if width == 0 {
        return vec![(String::new(), false)];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    for word in text.split(' ') {
        let mut pieces = Vec::new();
        let mut start = 0;
        for (offset, grapheme) in word.grapheme_indices(true) {
            if grapheme == "-" {
                let end = offset + grapheme.len();
                pieces.push(&word[start..end]);
                start = end;
            }
        }
        if start < word.len() || pieces.is_empty() {
            pieces.push(&word[start..]);
        }
        for (index, piece) in pieces.into_iter().enumerate() {
            let separated_by_space = index == 0 && !current.is_empty();
            let separator = usize::from(separated_by_space);
            let piece_width = text_width(piece);
            if current_width + separator + piece_width <= width {
                if separated_by_space {
                    current.push(' ');
                    current_width += 1;
                }
                current.push_str(piece);
                current_width += piece_width;
                continue;
            }
            if !current.is_empty() {
                lines.push((std::mem::take(&mut current), separated_by_space));
                current_width = 0;
            }
            for grapheme in piece.graphemes(true) {
                let grapheme_width = text_width(grapheme);
                if current_width + grapheme_width > width && !current.is_empty() {
                    lines.push((std::mem::take(&mut current), false));
                    current_width = 0;
                }
                current.push_str(grapheme);
                current_width += grapheme_width;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push((current, false));
    }
    lines
}

fn text_width(text: &str) -> usize {
    Line::from(text).width()
}

#[cfg(test)]
mod tests;
