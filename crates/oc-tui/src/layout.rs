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

/// The horizontal strip's painted rectangles. Indices refer to the caller's
/// ordered tabs; overflow markers and the optional add control are not tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSlot {
    pub index: usize,
    pub rect: Rect,
}

/// Horizontal close overlay at upstream `right={1}`. Do not offer a glyph
/// where the prefix/title/right padding cannot all fit in the painted tab.
pub(crate) fn tab_close_cell(rect: Rect) -> Option<u16> {
    (rect.height > 0 && rect.width >= 5).then(|| rect.right() - 2)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HorizontalTabStrip {
    pub start: usize,
    pub before: usize,
    pub after: usize,
    pub before_marker: Option<Rect>,
    pub tabs: Vec<TabSlot>,
    pub after_marker: Option<Rect>,
    pub add: Option<Rect>,
}

impl HorizontalTabStrip {
    /// Only a cell inside a visible, painted tab is selectable.
    pub fn hit_test(&self, x: u16, y: u16) -> Option<usize> {
        self.tabs
            .iter()
            .find(|tab| tab.rect.width > 0 && tab.rect.contains((x, y).into()))
            .map(|tab| tab.index)
    }
}

/// Pinned `session-tabs.tsx:237-274,1446-1467`: on a mouse close, keep
/// surviving visible widths and stretch the adjacent tab until its close cell
/// occupies the actual release column. The hold is owned by the view and is
/// released independently of the ordinary adaptive solver.
pub fn held_after_close(
    old: &HorizontalTabStrip,
    area: Rect,
    closed: usize,
    count: usize,
    target: usize,
    pointer_x: u16,
    can_add: bool,
) -> Option<HorizontalTabStrip> {
    if target >= count || area.height == 0 || pointer_x < area.x || pointer_x >= area.right() {
        return None;
    }
    let remap = |index: usize| (index != closed).then(|| index - usize::from(index > closed));
    let mut visible: Vec<(usize, u16)> = old
        .tabs
        .iter()
        .filter_map(|tab| remap(tab.index).map(|index| (index, tab.rect.width)))
        .collect();
    if !visible.iter().any(|(index, _)| *index == target) {
        visible.push((target, 1));
        visible.sort_by_key(|(index, _)| *index);
    }
    let start = visible.first()?.0;
    if visible
        .iter()
        .enumerate()
        .any(|(offset, (index, _))| *index != start + offset)
    {
        return None;
    }
    let before = start;
    let after = count.checked_sub(start + visible.len())?;
    let marker_width = |hidden: usize| (hidden > 0).then(|| hidden.to_string().len() as u16 + 2);
    let leading = marker_width(before).unwrap_or(0);
    let preceding: u32 = visible
        .iter()
        .take_while(|(index, _)| *index != target)
        .map(|(_, width)| u32::from(*width))
        .sum();
    let target_start = u32::from(area.x) + u32::from(leading) + preceding;
    let width = u32::from(pointer_x)
        .checked_add(2)?
        .checked_sub(target_start)?;
    if width < 5 || width > u32::from(u16::MAX) {
        return None;
    }
    visible.iter_mut().find(|(index, _)| *index == target)?.1 = width as u16;
    let mut x = area.x;
    let mut take = |width: u16| {
        let width = width.min(area.right().saturating_sub(x));
        let rect = Rect::new(x, area.y, width, area.height.min(1));
        x = x.saturating_add(width);
        rect
    };
    let before_marker = marker_width(before).map(&mut take);
    let tabs: Vec<_> = visible
        .into_iter()
        .map(|(index, width)| TabSlot {
            index,
            rect: take(width),
        })
        .collect();
    let after_marker = marker_width(after).map(&mut take);
    let add = can_add.then(|| take(3));
    if tabs
        .iter()
        .find(|tab| tab.index == target)
        .and_then(|tab| tab_close_cell(tab.rect))
        != Some(pointer_x)
    {
        return None;
    }
    Some(HorizontalTabStrip {
        start,
        before,
        after,
        before_marker,
        tabs,
        after_marker,
        add,
    })
}

/// `context/session-tabs-model.ts:33-37,175-257` adaptive window and widths;
/// `component/session-tabs.tsx:1316-1338,1739-1772` reserves the three-cell
/// add affordance only when its action is available. Rectangles are clipped
/// after solving, including at sizes too small for an upstream minimum tab or
/// a complete overflow marker. This does not create a session or an add action.
pub fn horizontal_tab_strip(
    area: Rect,
    count: usize,
    active: Option<usize>,
    previous_start: usize,
    can_add: bool,
) -> HorizontalTabStrip {
    let active = active.filter(|index| *index < count);
    let available = area.width.saturating_sub(if can_add { 3 } else { 0 }) as usize;
    let fit = |width: usize| -> usize {
        if count == 0 {
            return 0;
        }
        let slots = if active.is_some() {
            1 + width.saturating_sub(SESSION_TAB_WIDTH as usize) / SESSION_TAB_MIN_WIDTH as usize
        } else {
            width / SESSION_TAB_MIN_WIDTH as usize
        };
        slots.max(1).min(count)
    };
    let marker_width = |hidden: usize| -> usize {
        if hidden == 0 {
            0
        } else {
            hidden.to_string().len() + 2
        }
    };
    let mut visible = fit(available);
    let mut start = 0;
    if visible > 0 {
        let mut candidate = previous_start;
        // Same bounded solve as upstream; marker digits change the fit at
        // window boundaries, so the current start is reused on each pass.
        for attempt in 0..=3 {
            let bounded = candidate.min(count - visible);
            start = match active {
                Some(index) if index < bounded => index,
                Some(index) if index >= bounded + visible => index - visible + 1,
                _ => bounded,
            };
            let after = count - start - visible;
            let next = fit(available.saturating_sub(marker_width(start) + marker_width(after)));
            if next == visible || attempt == 3 {
                break;
            }
            visible = next;
            candidate = start;
        }
    }
    let before = start;
    let after = count - start - visible;
    let content = available
        .saturating_sub(marker_width(before) + marker_width(after))
        .max(1);
    let total = if content >= SESSION_TAB_WIDTH as usize * visible {
        content.min(SESSION_TAB_MAX_WIDTH as usize * visible)
    } else {
        content
    };
    let widths: Vec<usize> = if visible == 0 {
        Vec::new()
    } else if content >= SESSION_TAB_WIDTH as usize * visible || active.is_none() {
        (0..visible)
            .map(|index| total / visible + usize::from(index < total % visible))
            .collect()
    } else {
        let inactive = if visible == 1 {
            0
        } else {
            ((total.saturating_sub(SESSION_TAB_WIDTH as usize)) / (visible - 1))
                .clamp(SESSION_TAB_MIN_WIDTH as usize, SESSION_TAB_WIDTH as usize)
        };
        let selected = if visible == 1 {
            total
        } else {
            total.saturating_sub(inactive * (visible - 1))
        };
        (start..start + visible)
            .map(|index| {
                if Some(index) == active {
                    selected
                } else {
                    inactive
                }
            })
            .collect()
    };

    let mut x = area.x;
    let end = area.right();
    let mut take = |requested: usize| {
        let width = requested.min(end.saturating_sub(x) as usize) as u16;
        let rect = Rect::new(x, area.y, width, area.height.min(1));
        x = x.saturating_add(width);
        rect
    };
    let before_marker = (before > 0).then(|| take(marker_width(before)));
    let tabs = widths
        .into_iter()
        .enumerate()
        .map(|(offset, width)| TabSlot {
            index: start + offset,
            rect: take(width),
        })
        .collect();
    let after_marker = (after > 0).then(|| take(marker_width(after)));
    let add = can_add.then(|| take(3));
    HorizontalTabStrip {
        start,
        before,
        after,
        before_marker,
        tabs,
        after_marker,
        add,
    }
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

    #[test]
    fn horizontal_tabs_one_and_adaptive_multi() {
        for width in [80, 120] {
            let strip = horizontal_tab_strip(Rect::new(4, 2, width, 1), 1, Some(0), 0, false);
            assert_eq!(strip.tabs[0].index, 0);
            assert_eq!(strip.tabs[0].rect, Rect::new(4, 2, 32, 1));
            assert_eq!((strip.before, strip.after), (0, 0));
            assert_eq!(strip.hit_test(4, 2), Some(0));
            assert_eq!(strip.hit_test(36, 2), None);
            assert_eq!(strip.hit_test(4, 3), None);
            assert!(strip.add.is_none());
        }
        let two = horizontal_tab_strip(Rect::new(0, 0, 43, 1), 2, Some(0), 0, false);
        assert_eq!(
            two.tabs.iter().map(|t| t.rect.width).collect::<Vec<_>>(),
            [22, 21]
        );
        let tight = horizontal_tab_strip(Rect::new(0, 0, 32, 1), 2, Some(1), 0, false);
        assert_eq!(
            tight.tabs.iter().map(|t| t.rect.width).collect::<Vec<_>>(),
            [10, 22]
        );
        let three = horizontal_tab_strip(Rect::new(0, 0, 44, 1), 3, Some(1), 0, false);
        assert_eq!((three.before, three.after), (0, 0));
        assert_eq!(
            three.tabs.iter().map(|t| t.rect.width).collect::<Vec<_>>(),
            [11, 22, 11]
        );
        assert_eq!(three.after_marker, None);
        let window = horizontal_tab_strip(Rect::new(0, 0, 31, 1), 3, Some(1), 0, false);
        assert_eq!((window.start, window.before, window.after), (1, 1, 1));
        assert_eq!(window.before_marker, Some(Rect::new(0, 0, 3, 1)));
        assert_eq!(
            window.tabs[0],
            TabSlot {
                index: 1,
                rect: Rect::new(3, 0, 25, 1)
            }
        );
        assert_eq!(window.after_marker, Some(Rect::new(28, 0, 3, 1)));
    }

    #[test]
    fn closing_hovered_home_holds_close_column_then_clips_at_resize() {
        let area = Rect::new(0, 0, 120, 1);
        let old = horizontal_tab_strip(area, 2, Some(1), 0, false);
        let close = old.tabs[1].rect.right() - 2;
        let held = held_after_close(&old, area, 1, 1, 0, close, true).unwrap();
        assert_eq!(held.tabs[0].rect, Rect::new(0, 0, 64, 1));
        assert_eq!(held.add, Some(Rect::new(64, 0, 3, 1)));
        assert_eq!(held.hit_test(close, 0), Some(0));
        assert_eq!(
            held_after_close(&old, Rect::new(0, 0, 40, 1), 1, 1, 0, close, true),
            None
        );

        // Overflow: if the next sibling was hidden, bring it into the
        // visible window, retaining the before marker and the pointer cell.
        let narrow = Rect::new(0, 0, 31, 1);
        let window = horizontal_tab_strip(narrow, 3, Some(1), 0, false);
        let close = window.tabs[0].rect.right() - 2;
        let next = held_after_close(&window, narrow, 1, 2, 1, close, false).unwrap();
        assert_eq!((next.before, next.after), (1, 0));
        assert_eq!(next.tabs[0].index, 1);
        assert_eq!(next.hit_test(close, 0), Some(1));
        assert_eq!(tab_close_cell(next.tabs[0].rect), Some(close));
    }

    #[test]
    fn horizontal_tabs_keep_active_visible_and_respect_previous_start() {
        let area = Rect::new(5, 3, 44, 1);
        for (active, previous, start) in [(0, 19, 0), (19, 0, 17), (10, 9, 9)] {
            let strip = horizontal_tab_strip(area, 20, Some(active), previous, false);
            assert_eq!(strip.start, start, "{active} {previous}");
            assert!(
                strip
                    .tabs
                    .iter()
                    .any(|t| t.index == active && t.rect.width > 0)
            );
            assert_eq!(strip.before, start);
            assert_eq!(strip.after, 20 - start - strip.tabs.len());
        }
        let no_active = horizontal_tab_strip(area, 20, None, 8, false);
        assert_eq!(no_active.start, 8);
    }

    #[test]
    fn horizontal_tabs_marker_digits_and_add_capability() {
        let strip = horizontal_tab_strip(Rect::new(0, 0, 44, 1), 100, Some(15), 15, false);
        assert_eq!(strip.before, 15);
        assert_eq!(strip.before_marker.unwrap().width, 4); // ‹15 plus gap
        assert!(strip.after >= 10);
        assert_eq!(
            strip.after_marker.unwrap().width,
            strip.after.to_string().len() as u16 + 2
        );
        assert!(strip.add.is_none());
        let three_digits = horizontal_tab_strip(Rect::new(0, 0, 44, 1), 200, Some(150), 150, false);
        assert!(three_digits.before >= 100);
        assert_eq!(three_digits.before_marker.unwrap().width, 5);
        let trailing = horizontal_tab_strip(Rect::new(0, 0, 44, 1), 200, Some(0), 0, false);
        assert!(trailing.after >= 100);
        assert_eq!(trailing.after_marker.unwrap().width, 5);
        let allowed = horizontal_tab_strip(Rect::new(0, 0, 80, 1), 1, Some(0), 0, true);
        assert_eq!(allowed.tabs[0].rect.width, 32);
        assert_eq!(allowed.add, Some(Rect::new(32, 0, 3, 1)));
        assert_eq!(allowed.hit_test(32, 0), None);
        let too_narrow = horizontal_tab_strip(Rect::new(5, 3, 1, 1), 2, Some(1), 0, false);
        assert_eq!(too_narrow.before_marker, Some(Rect::new(5, 3, 1, 1)));
        assert_eq!(too_narrow.tabs[0].rect.width, 0);
        assert_eq!(too_narrow.hit_test(5, 3), None);
        assert_eq!(too_narrow.hit_test(6, 3), None);
    }

    #[test]
    fn horizontal_tabs_clip_without_overlap_or_unpainted_hits() {
        for count in [0_usize, 1, 2, 3, 20, 120] {
            for width in [0, 1, 7, 8, 21, 22, 31, 32, 43, 44, 119, 120, 121] {
                for active in [None, Some(0), Some(count / 2), count.checked_sub(1)] {
                    for add in [false, true] {
                        let area = Rect::new(3, 5, width, 1);
                        let strip = horizontal_tab_strip(area, count, active, count / 2, add);
                        let mut painted = Vec::new();
                        painted.extend(strip.before_marker);
                        painted.extend(strip.tabs.iter().map(|tab| tab.rect));
                        painted.extend(strip.after_marker);
                        painted.extend(strip.add);
                        let mut right = area.x;
                        for rect in painted {
                            assert_eq!(rect.y, area.y);
                            assert_eq!(rect.height, 1);
                            assert!(rect.x >= right && rect.right() <= area.right(), "{strip:?}");
                            right = rect.right();
                        }
                        for x in area.x..=area.right() {
                            let hit = strip.hit_test(x, area.y);
                            assert_eq!(
                                hit,
                                strip
                                    .tabs
                                    .iter()
                                    .find(|tab| tab.rect.contains((x, area.y).into()))
                                    .map(|tab| tab.index),
                                "{strip:?} x={x}"
                            );
                        }
                        assert_eq!(strip.hit_test(area.x, area.y + 1), None);
                        if !add {
                            assert!(strip.add.is_none());
                        }
                    }
                }
            }
        }
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
