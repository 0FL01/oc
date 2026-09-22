//! Upstream v2.0.12 layout geometry.
//!
//! Every constant cites the upstream source at tag `v2.0.12`; the surrounding
//! cell arithmetic is hand-computed (not delegated to the flex solver) so the
//! golden snapshots in `shell` pin exact rectangles.
//!
//! Root column (`packages/tui/src/app.tsx:1310-1376`): the main row grows,
//! the optional devtools bar keeps its fixed row, and overlays sit on top.
//! The configured vertical rail consumes width before sidebar auto-visibility.
//! Sidebar separation is its raised surface, not an invented border column;
//! upstream only allocates a resize handle for terminal/panel right panes.
//!
//! Inside the session route (`packages/tui/src/routes/session/index.tsx:1273-1419`):
//! content padding, scrollbox, height-1 status row, then the bottom stack
//! (prompt). The port keeps its eight-row picker panes as an inline block
//! between the transcript and the status row (see `shell`) until iteration 4
//! replaces them with upstream dialogs.

use ratatui::layout::Rect;

// ---- root column ----------------------------------------------------------

/// Horizontal tab strip height; a vertical rail replaces it only for the
/// vertical tab layout (`component/session-tabs.tsx:1506-1508` `height={1}`).
pub const TABS_RAIL_HEIGHT: u16 = 1;
/// Devtools bar height (`component/devtools-bar.tsx:232` `height={1}`).
pub const DEVTOOLS_BAR_HEIGHT: u16 = 1;

// ---- session pane ---------------------------------------------------------

/// Session status row height (`routes/session/index.tsx:1331`).
pub const STATUS_ROW_HEIGHT: u16 = 1;
/// Prompt box height: `paddingTop=1` + 1 textarea row + 1 metadata gap row +
/// 1 metadata row (`component/prompt/index.tsx:1650-1671,1856-1865`).
pub const PROMPT_BOX_HEIGHT: u16 = 4;
/// The `╹`/`▀` underline row below the prompt box
/// (`component/prompt/index.tsx:1846-1870`).
pub const PROMPT_UNDERLINE_HEIGHT: u16 = 1;
/// Session content bottom padding (`routes/session/index.tsx:1276`
/// `paddingBottom={1}`).
pub const SESSION_BOTTOM_PADDING: u16 = 1;

// ---- breakpoints ----------------------------------------------------------

/// Below 44 columns the session/route padding drops from 2 to 1 and the
/// prompt metadata row and footer hints disappear
/// (`routes/session/index.tsx:1277-1278`, `component/prompt/index.tsx:1660-1661`,
/// `component/prompt/metadata.tsx:110-111`, `feature-plugins/prompt/footer.tsx:53`).
pub const NARROW_WIDTH: u16 = 44;
/// Sidebar auto-shows when `width - tabsWidth > 120`
/// (`component/session-frame.tsx:106`). Call with the width after the tabs rail.
pub const SIDEBAR_AUTO_WIDTH: u16 = 120;
/// Dialog panel widths and the select-dialog footer breakpoint are recorded
/// for iteration 4; they are deliberately not implemented here
/// (`ui/dialog.tsx:14-18`, `ui/dialog-select.tsx:816`).
pub const DIALOG_WIDTH_MEDIUM: u16 = 60;
pub const DIALOG_WIDTH_LARGE: u16 = 88;
pub const DIALOG_WIDTH_XLARGE: u16 = 116;
pub const DIALOG_FOOTER_COLUMN_BREAKPOINT: u16 = 60;

/// Session sidebar/tab widths (`ui/layout.ts:1-7`).
pub const SESSION_SIDEBAR_WIDTH: u16 = 42;
pub const SESSION_TABS_COMPACT_WIDTH: u16 = 5;
pub const SESSION_TABS_COMPACT_BREAKPOINT: u16 = 12;
pub const SESSION_SIDEBAR_MAX_WIDTH: u16 = 72;
pub const SESSION_CONTENT_MIN_WIDTH: u16 = 44;
pub const SESSION_CONTENT_PREFERRED_WIDTH: u16 = 64;

/// Horizontal tab metrics (`context/session-tabs-model.ts:33-35`).
pub const SESSION_TAB_WIDTH: u16 = 22;
pub const SESSION_TAB_MAX_WIDTH: u16 = 32;
pub const SESSION_TAB_MIN_WIDTH: u16 = 8;

// ---- breakpoint predicates ------------------------------------------------

/// Session/route horizontal padding: `width < 44 ? 1 : 2`
/// (`routes/session/index.tsx:1277-1278`).
pub fn session_padding(width: u16) -> u16 {
    if width < NARROW_WIDTH { 1 } else { 2 }
}

/// Agent and provider are dropped from the prompt metadata row below 44
/// columns (`component/prompt/metadata.tsx:110-111`).
pub fn shows_agent_metadata(width: u16) -> bool {
    width >= NARROW_WIDTH
}

/// Prompt footer shortcuts show from 44 columns (`feature-plugins/prompt/footer.tsx:53`).
pub fn shows_prompt_hints(width: u16) -> bool {
    width >= NARROW_WIDTH
}

/// Sidebar auto-visibility predicate (`component/session-frame.tsx:106`).
pub fn sidebar_auto(width: u16) -> bool {
    // Callers subtract the actual vertical rail before evaluating this predicate.
    width > SIDEBAR_AUTO_WIDTH
}

/// One active tab takes the whole strip up to the upstream maximum width
/// (`adaptiveSessionTabLayout` with a single tab, `session-tabs-model.ts:175-220`).
pub fn single_tab_width(available: u16) -> u16 {
    available.clamp(1, SESSION_TAB_MAX_WIDTH)
}

// ---- rectangles -----------------------------------------------------------

/// Root column regions; `session` is the whole main column below the tab
/// strip (the port has no right pane, so no resize-handle column is carved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellRegions {
    /// Height-1 horizontal tab strip.
    pub tabs: Rect,
    /// Main column: the session route.
    pub session: Rect,
    /// Height-1 devtools bar (local channel default, `app.tsx:472`).
    pub devtools: Rect,
}

/// Fixed root regions for one frame area.
pub fn shell_regions(area: Rect) -> ShellRegions {
    configured_shell_regions(area, true, 0)
}

pub fn configured_shell_regions(area: Rect, devtools: bool, vertical_tabs: u16) -> ShellRegions {
    let main_height = area.height.saturating_sub(u16::from(devtools));
    if vertical_tabs > 0 {
        let width = vertical_tabs.min(area.width.saturating_sub(SESSION_CONTENT_MIN_WIDTH));
        return ShellRegions {
            tabs: Rect::new(area.x, area.y, width, main_height),
            session: Rect::new(area.x + width, area.y, area.width - width, main_height),
            devtools: Rect::new(
                area.x,
                area.y + main_height,
                area.width,
                area.height - main_height,
            ),
        };
    }
    let tabs_height = TABS_RAIL_HEIGHT.min(main_height);
    let tabs = Rect::new(area.x, area.y, area.width, tabs_height);
    let session = Rect::new(
        area.x,
        area.y.saturating_add(tabs_height),
        area.width,
        main_height.saturating_sub(tabs_height),
    );
    let devtools = Rect::new(
        area.x,
        area.y.saturating_add(main_height),
        area.width,
        area.height.saturating_sub(main_height),
    );
    ShellRegions {
        tabs,
        session,
        devtools,
    }
}

/// Session regions top-to-bottom: transcript, optional inline panel (port
/// extension), height-1 status row, prompt box, prompt footer. The content
/// box bottom padding stays outside every region (`index.tsx:1276`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionRegions {
    /// Padded content box (left/right padding applied, bottom padding removed).
    pub content: Rect,
    /// Sticky-bottom transcript.
    pub transcript: Rect,
    /// Inline picker/dcp pane; zero height when no panel is open.
    pub panel: Rect,
    /// Height-1 right-aligned status row.
    pub status: Rect,
    /// Prompt box (left border + padding + textarea + metadata) without the
    /// underline row.
    pub prompt: Rect,
    /// The `╹`/`▀` underline row.
    pub underline: Rect,
    /// Prompt footer row (interrupt status / DCP notice / hints).
    pub footer: Rect,
}

/// Content box with the upstream `1|2` horizontal padding and 1-row bottom
/// padding (`routes/session/index.tsx:1273-1279`).
pub fn content_box(area: Rect) -> Rect {
    let pad = session_padding(area.width);
    Rect {
        x: area.x.saturating_add(pad),
        y: area.y,
        width: area.width.saturating_sub(pad.saturating_mul(2)),
        height: area.height.saturating_sub(SESSION_BOTTOM_PADDING),
    }
}

/// Split one session route column into its fixed bottom stack plus the
/// flexible transcript. Allocations are made top-to-bottom and clamped to
/// the remaining height; on a terminal too short for the whole stack the
/// transcript keeps one row and the bottom stack is clipped, so the message
/// stream stays observable (port decision below the upstream 24-row target).
pub fn session_regions(area: Rect, panel_height: u16) -> SessionRegions {
    dynamic_session_regions(area, panel_height, PROMPT_BOX_HEIGHT)
}

pub fn dynamic_session_regions(
    area: Rect,
    panel_height: u16,
    prompt_height: u16,
) -> SessionRegions {
    let content = content_box(area);
    let fixed = panel_height
        .saturating_add(STATUS_ROW_HEIGHT)
        .saturating_add(prompt_height)
        .saturating_add(PROMPT_UNDERLINE_HEIGHT)
        .saturating_add(1);
    let transcript_height = if content.height == 0 {
        0
    } else {
        content.height.saturating_sub(fixed).max(1)
    };
    let mut y = content.y;
    let mut take = |height: u16| -> Rect {
        let height = height.min(content.bottom().saturating_sub(y));
        let rect = Rect::new(content.x, y, content.width, height);
        y = y.saturating_add(height);
        rect
    };
    SessionRegions {
        content,
        transcript: take(transcript_height),
        panel: take(panel_height),
        status: take(STATUS_ROW_HEIGHT),
        prompt: take(prompt_height),
        underline: take(PROMPT_UNDERLINE_HEIGHT),
        footer: take(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_regions_keep_the_upstream_rows() {
        let area = Rect::new(0, 0, 80, 24);
        let shell = shell_regions(area);
        assert_eq!(shell.tabs, Rect::new(0, 0, 80, 1));
        assert_eq!(shell.session, Rect::new(0, 1, 80, 22));
        assert_eq!(shell.devtools, Rect::new(0, 23, 80, 1));

        let tall = shell_regions(Rect::new(0, 0, 120, 40));
        assert_eq!(tall.tabs, Rect::new(0, 0, 120, 1));
        assert_eq!(tall.session, Rect::new(0, 1, 120, 38));
        assert_eq!(tall.devtools, Rect::new(0, 39, 120, 1));
    }

    #[test]
    fn session_regions_place_the_bottom_stack() {
        // 80x24: 22 session rows, content 2..77, bottom padding on row 22.
        let area = Rect::new(0, 1, 80, 22);
        let s = session_regions(area, 0);
        assert_eq!(s.content, Rect::new(2, 1, 76, 21));
        assert_eq!(s.transcript, Rect::new(2, 1, 76, 14));
        assert_eq!(s.panel, Rect::new(2, 15, 76, 0));
        assert_eq!(s.status, Rect::new(2, 15, 76, 1));
        assert_eq!(s.prompt, Rect::new(2, 16, 76, 4));
        assert_eq!(s.underline, Rect::new(2, 20, 76, 1));
        assert_eq!(s.footer, Rect::new(2, 21, 76, 1));

        // 120x40: 38 session rows, content 2..119.
        let s = session_regions(Rect::new(0, 1, 120, 38), 0);
        assert_eq!(s.transcript, Rect::new(2, 1, 116, 30));
        assert_eq!(s.status, Rect::new(2, 31, 116, 1));
        assert_eq!(s.prompt, Rect::new(2, 32, 116, 4));
        assert_eq!(s.underline, Rect::new(2, 36, 116, 1));
        assert_eq!(s.footer, Rect::new(2, 37, 116, 1));
    }

    #[test]
    fn panel_height_shrinks_only_the_transcript() {
        let area = Rect::new(0, 1, 80, 22);
        let s = session_regions(area, 10);
        assert_eq!(s.transcript, Rect::new(2, 1, 76, 4));
        assert_eq!(s.panel, Rect::new(2, 5, 76, 10));
        assert_eq!(s.status, Rect::new(2, 15, 76, 1));
        assert_eq!(s.prompt, Rect::new(2, 16, 76, 4));
        assert_eq!(s.underline, Rect::new(2, 20, 76, 1));
        assert_eq!(s.footer, Rect::new(2, 21, 76, 1));
    }

    #[test]
    fn tiny_terminals_clamp_instead_of_overflowing() {
        // Too short for the bottom stack: the transcript keeps one row.
        let s = session_regions(Rect::new(0, 1, 20, 3), 10);
        assert_eq!(s.transcript.height, 1);
        assert!(s.footer.bottom() <= 4, "{s:?}");
        // A one-row terminal is the devtools bar alone; the main row is 0.
        let shell = shell_regions(Rect::new(0, 0, 10, 1));
        assert_eq!(shell.tabs.height, 0);
        assert_eq!(shell.session.height, 0);
        assert_eq!(shell.devtools, Rect::new(0, 0, 10, 1));
    }

    /// 44 and 120 are the switches the shell implements today; 60/80 only
    /// affect surfaces that are still out of scope (dialogs, home footer).
    #[test]
    fn breakpoints_switch_at_upstream_widths() {
        assert_eq!(session_padding(43), 1);
        assert_eq!(session_padding(44), 2);
        assert!(!shows_agent_metadata(43));
        assert!(shows_agent_metadata(44));
        assert!(!shows_prompt_hints(43));
        assert!(shows_prompt_hints(44));
        for width in [44, 60, 80, 120] {
            assert_eq!(session_padding(width), 2, "{width}");
            assert!(shows_agent_metadata(width), "{width}");
            assert!(shows_prompt_hints(width), "{width}");
        }
        // `width - tabs > 120`: 120 stays narrow, 121 auto-shows the sidebar.
        assert!(!sidebar_auto(120));
        assert!(sidebar_auto(121));
        assert_eq!(DIALOG_WIDTH_MEDIUM, 60);
        assert_eq!(DIALOG_WIDTH_LARGE, 88);
        assert_eq!(DIALOG_WIDTH_XLARGE, 116);
        assert_eq!(DIALOG_FOOTER_COLUMN_BREAKPOINT, 60);
    }

    #[test]
    fn single_tab_is_capped_at_the_upstream_maximum() {
        assert_eq!(single_tab_width(80), SESSION_TAB_MAX_WIDTH);
        assert_eq!(
            single_tab_width(SESSION_TAB_MAX_WIDTH),
            SESSION_TAB_MAX_WIDTH
        );
        assert_eq!(single_tab_width(20), 20);
        assert_eq!(single_tab_width(0), 1);
    }

    /// Resizing is a pure re-layout: the same area always yields the same
    /// regions, so shrink -> grow reproduces the wide geometry exactly.
    #[test]
    fn re_layout_is_deterministic() {
        let narrow = session_regions(Rect::new(0, 1, 43, 22), 0);
        let wide = session_regions(Rect::new(0, 1, 120, 38), 0);
        assert_eq!(narrow, session_regions(Rect::new(0, 1, 43, 22), 0));
        assert_eq!(wide, session_regions(Rect::new(0, 1, 120, 38), 0));
        assert_eq!(session_padding(43), 1);
        assert_eq!(session_padding(120), 2);
        assert_eq!(session_padding(43), 1);
    }
}
