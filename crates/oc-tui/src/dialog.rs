//! Shared native modal surface and searchable, scrolling selection list.
//! Geometry/spacing follows pinned upstream ui/dialog{,-select}.tsx.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Clear, Paragraph},
};

use crate::{
    app::{TuiPanel, TuiState},
    theme::Theme,
};

#[derive(Clone, Copy, Debug)]
pub enum DialogSize {
    Medium,
    Large,
    Xlarge,
}

pub struct DialogFrame;
// OpenTUI writes a truecolor white foreground for the overlay canvas. ANSI
// Color::White resolves to #eeeeec in the shared xterm profile, not #ffffff.
const CANVAS_FOREGROUND: Color = Color::Rgb(255, 255, 255);
impl DialogFrame {
    pub fn rect(area: Rect, size: DialogSize, height: u16) -> Rect {
        let wanted = match size {
            DialogSize::Medium => 60,
            DialogSize::Large => 88,
            DialogSize::Xlarge => 116,
        };
        let width = wanted.min(area.width.saturating_sub(2));
        let top = area.y + area.height / 4;
        Rect::new(
            area.x + (area.width - width) / 2,
            top,
            width,
            height.min(area.bottom() - top),
        )
    }

    fn paint(frame: &mut Frame<'_>, area: Rect, theme: &Theme) {
        // The pinned OpenTUI overlay paints untinted blank glyphs with its white
        // canvas foreground (#fff), not the xterm frontend's default #eee.
        // Visible glyphs retain dimmed RGB and attributes (paired V04 cells).
        for cell in &mut frame.buffer_mut().content {
            if cell.symbol() == " " {
                cell.fg = CANVAS_FOREGROUND;
                cell.modifier = Modifier::empty();
            } else {
                cell.fg = backdrop(cell.fg, theme.text());
            }
            cell.bg = backdrop(cell.bg, theme.background());
        }
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default().style(
                Style::default()
                    .fg(CANVAS_FOREGROUND)
                    .bg(slot(theme, "background.base")),
            ),
            area,
        );
    }
}

pub fn backdrop(color: Color, fallback: Color) -> Color {
    let Color::Rgb(r, g, b) = (if color == Color::Reset {
        fallback
    } else {
        color
    }) else {
        return color;
    };
    let dim = |v: u8| ((u16::from(v) * 105 + 127) / 255) as u8;
    Color::Rgb(dim(r), dim(g), dim(b))
}

fn slot(theme: &Theme, path: &str) -> Color {
    theme
        .color(&format!("@dialog.{path}"))
        .or_else(|| theme.color(path))
        .unwrap_or(theme.text())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub title: String,
    pub category: String,
    pub footer: String,
    pub current: bool,
}

#[derive(Default)]
pub struct SelectList {
    pub query: String,
    pub cursor: usize,
    offset: Cell<usize>,
    follow_cursor: Cell<bool>,
    cache: RefCell<Option<FilterCache>>,
}

/// Hit regions are computed from the same row map and rectangle as painting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogHit {
    Backdrop,
    Surface,
    Search,
    Close,
    Option(usize),
    List,
}

struct Geometry {
    rect: Rect,
    list_y: u16,
    list_height: usize,
    rows: Vec<Option<usize>>,
    offset: usize,
}

struct FilterCache {
    source: Rc<Vec<SelectOption>>,
    prepared: Vec<Vec<crate::fuzzy::Target>>,
    query: String,
    panel: TuiPanel,
    result: Rc<Vec<SelectOption>>,
}

impl SelectList {
    pub fn reset(&mut self) {
        *self = Self::default();
        self.follow_cursor.set(true);
    }
    pub fn filter_for(
        &self,
        options: Rc<Vec<SelectOption>>,
        panel: &TuiPanel,
    ) -> Rc<Vec<SelectOption>> {
        if self.query.is_empty() {
            return options;
        }
        let model = *panel == TuiPanel::Model;
        let commands = *panel == TuiPanel::Commands;
        let query = if model {
            self.query.trim().to_string()
        } else {
            self.query.to_lowercase()
        };
        if model && query.is_empty() {
            return options;
        }
        let mut slot = self.cache.borrow_mut();
        // Exactly one catalog and one query result, replaced on invalidation.
        // Model options are snapshot-owned Rc: repeated frames cost O(1).
        let changed = slot.as_ref().is_none_or(|c| {
            c.panel != *panel || (!Rc::ptr_eq(&c.source, &options) && *c.source != *options)
        });
        if changed {
            let prepared = options
                .iter()
                .map(|o| {
                    let keys = [
                        &*o.title,
                        &*o.category,
                        if commands { &*o.value } else { "" },
                    ];
                    keys[..if model { 2 } else { 3 }]
                        .iter()
                        .map(|s| {
                            if model {
                                crate::fuzzy::Target::membership(s)
                            } else {
                                crate::fuzzy::Target::new(s)
                            }
                        })
                        .collect()
                })
                .collect();
            *slot = Some(FilterCache {
                source: options,
                prepared,
                query: String::new(),
                panel: panel.clone(),
                result: Rc::default(),
            });
        }
        let cache = slot.as_mut().expect("initialized");
        if cache.query == query {
            return cache.result.clone();
        }
        let compiled = crate::fuzzy::Query::new(&query);
        let scores: Vec<_> = cache
            .prepared
            .iter()
            .enumerate()
            .filter_map(|(i, keys)| {
                (if model {
                    compiled.matches(keys).then_some(1.0)
                } else {
                    compiled.score(keys, true)
                })
                .filter(|score| !commands || *score >= 0.7)
                .map(|score| (i, score))
            })
            .collect();
        let order = if model {
            // DialogModel applies its metadata sort *after* fuzzy membership.
            // Keep the already metadata-sorted catalog order, not fuzzy relevance.
            scores.into_iter().map(|(i, _)| i).collect()
        } else {
            crate::fuzzy::rank(scores.into_iter())
        };
        cache.result = Rc::new(order.into_iter().map(|i| cache.source[i].clone()).collect());
        cache.query = query;
        cache.result.clone()
    }
    pub fn move_by(&mut self, delta: isize, count: usize) {
        self.cursor = if count == 0 {
            0
        } else {
            (self.cursor as isize + delta).rem_euclid(count as isize) as usize
        };
    }
    pub fn changed_query(&mut self) {
        self.cursor = 0;
        self.offset.set(0);
        self.follow_cursor.set(true);
    }

    pub fn follow_selection(&self) {
        self.follow_cursor.set(true);
    }

    fn geometry(
        &self,
        area: Rect,
        size: DialogSize,
        options: &[SelectOption],
        footer: bool,
    ) -> Geometry {
        let mut rows = Vec::new();
        let mut category = "";
        for (i, option) in options.iter().enumerate() {
            if self.query.is_empty() && option.category != category {
                if !rows.is_empty() {
                    rows.push(None); // separator
                }
                rows.push(None); // category heading
                category = &option.category;
            }
            rows.push(Some(i));
        }
        let list_height = rows
            .len()
            .max(1)
            .min((area.height / 2).saturating_sub(6).max(1) as usize);
        let rect = DialogFrame::rect(area, size, list_height as u16 + 7 + u16::from(footer));
        Geometry {
            rect,
            list_y: rect.y.saturating_add(5),
            list_height: list_height
                .min(rect.height.saturating_sub(7) as usize)
                .max(1),
            offset: self
                .offset
                .get()
                .min(rows.len().saturating_sub(list_height)),
            rows,
        }
    }

    /// Scroll the option viewport independently of selection, as OpenTUI's scrollbox does.
    pub fn scroll_rows(
        &self,
        delta: isize,
        area: Rect,
        size: DialogSize,
        options: &[SelectOption],
    ) {
        let geo = self.geometry(area, size, options, false);
        let max = geo.rows.len().saturating_sub(geo.list_height);
        self.offset
            .set(geo.offset.saturating_add_signed(delta).min(max));
        self.follow_cursor.set(false);
    }

    pub fn hit(
        &self,
        area: Rect,
        size: DialogSize,
        options: &[SelectOption],
        x: u16,
        y: u16,
    ) -> DialogHit {
        let geo = self.geometry(area, size, options, false);
        let rect = geo.rect;
        if !rect.contains((x, y).into()) {
            return DialogHit::Backdrop;
        }
        if rect.width < 12 || rect.height < 5 {
            return DialogHit::Surface;
        }
        if y == rect.y + 1 && x >= rect.right() - 7 && x < rect.right() - 4 {
            return DialogHit::Close;
        }
        if y == rect.y + 3 && x >= rect.x + 4 && x < rect.right() - 4 {
            return DialogHit::Search;
        }
        if y >= geo.list_y
            && (y - geo.list_y) < geo.list_height as u16
            && x > rect.x
            && x < rect.right() - 1
        {
            return geo
                .rows
                .get(geo.offset + (y - geo.list_y) as usize)
                .and_then(|row| *row)
                .map_or(DialogHit::List, DialogHit::Option);
        }
        DialogHit::Surface
    }

    fn render(
        &self,
        frame: &mut Frame<'_>,
        title: &str,
        size: DialogSize,
        options: &[SelectOption],
        footer: Option<&str>,
    ) {
        let theme = Theme::dark();
        let flat = !self.query.is_empty();
        let mut rows: Vec<(Option<usize>, String)> = Vec::new();
        let mut category = "";
        for (i, option) in options.iter().enumerate() {
            if !flat && option.category != category {
                if !rows.is_empty() {
                    rows.push((None, String::new()));
                }
                rows.push((None, option.category.clone()));
                category = &option.category;
            }
            rows.push((Some(i), String::new()));
        }
        let mut geo = self.geometry(frame.area(), size, options, footer.is_some());
        if self.follow_cursor.get()
            && let Some(row) = geo.rows.iter().position(|item| *item == Some(self.cursor))
        {
            if row < geo.offset {
                geo.offset = row;
            } else if row >= geo.offset + geo.list_height {
                geo.offset = row + 1 - geo.list_height;
            }
        }
        let area = geo.rect;
        DialogFrame::paint(frame, area, theme);
        if area.width < 12 || area.height < 5 {
            return;
        }
        let text = slot(theme, "text.base");
        let muted = slot(theme, "text.muted");
        let line = |frame: &mut Frame<'_>, x, y, width, text: &str, style| {
            if y < area.bottom() {
                frame.render_widget(
                    Paragraph::new(ratatui::text::Line::styled(
                        text.chars().filter(|c| !c.is_control()).collect::<String>(),
                        style,
                    )),
                    Rect::new(x, y, width, 1),
                );
            }
        };
        line(
            frame,
            area.x + 4,
            area.y + 1,
            area.width - 11,
            title,
            Style::default().fg(text).add_modifier(Modifier::BOLD),
        );
        line(
            frame,
            area.right() - 7,
            area.y + 1,
            3,
            "esc",
            Style::default().fg(muted),
        );
        line(
            frame,
            area.x + 4,
            area.y + 3,
            area.width - 8,
            if self.query.is_empty() {
                "Search"
            } else {
                &self.query
            },
            Style::default()
                .fg(if self.query.is_empty() {
                    muted
                } else {
                    slot(theme, "text.formfield.$focused")
                })
                .bg(slot(theme, "background.base")),
        );
        let height = geo.list_height;
        let offset = geo.offset;
        self.offset.set(offset);
        if rows.is_empty() {
            line(
                frame,
                area.x + 4,
                area.y + 5,
                area.width - 8,
                if self.query.is_empty() {
                    "No items available"
                } else {
                    "No results found"
                },
                Style::default().fg(muted),
            );
        }
        for (row, (index, header)) in rows.iter().skip(offset).take(height).enumerate() {
            let y = area.y + 5 + row as u16;
            let Some(index) = index else {
                line(
                    frame,
                    area.x + 4,
                    y,
                    area.width - 8,
                    header,
                    Style::default()
                        .fg(theme.hue("accent", 200).unwrap_or(text))
                        .add_modifier(Modifier::BOLD),
                );
                continue;
            };
            let option = &options[*index];
            let active = *index == self.cursor;
            let fg = if active {
                slot(theme, "text.action.primary.$focused")
            } else if option.current {
                slot(theme, "text.formfield.$selected")
            } else {
                text
            };
            let style = if active {
                Style::default()
                    .fg(fg)
                    .bg(slot(theme, "background.action.primary.$focused"))
            } else {
                Style::default().fg(fg)
            };
            frame.render_widget(
                Block::default().style(
                    Style::default()
                        .fg(CANVAS_FOREGROUND)
                        .bg(style.bg.unwrap_or(slot(theme, "background.base"))),
                ),
                Rect::new(area.x + 1, y, area.width - 2, 1),
            );
            if option.current {
                line(frame, area.x + 2, y, 1, "●", style);
            }
            let right = if flat && !option.category.is_empty() {
                format!(
                    "{}{}{}",
                    option.category,
                    if option.footer.is_empty() { "" } else { " · " },
                    option.footer
                )
            } else {
                option.footer.clone()
            };
            let right_width = ratatui::text::Line::raw(right.as_str())
                .width()
                .min(((area.width - 12) / 2) as usize) as u16;
            line(
                frame,
                area.x + 4,
                y,
                area.width.saturating_sub(9 + right_width),
                &option.title,
                if active {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                },
            );
            line(
                frame,
                area.right() - 4 - right_width,
                y,
                right_width,
                &right,
                if active { style } else { style.fg(muted) },
            );
        }
        if let Some(footer) = footer {
            line(
                frame,
                area.x + 4,
                area.y + 6 + height as u16,
                area.width - 8,
                footer,
                Style::default().fg(muted),
            );
        }
        let cursor = ratatui::text::Line::raw(self.query.as_str())
            .width()
            .min(area.width.saturating_sub(9) as usize) as u16;
        frame.set_cursor_position((area.x + 4 + cursor, area.y + 3));
    }
}

pub fn render(frame: &mut Frame<'_>, state: &TuiState) {
    if state.panel() == &TuiPanel::Cards && state.card_output.is_some() {
        render_card_detail(frame, state);
        return;
    }
    let title = match state.panel() {
        TuiPanel::None => return,
        TuiPanel::Commands => "Commands",
        TuiPanel::Model => "Select model",
        TuiPanel::Variant => "Select variant",
        TuiPanel::Agents => "Select agent",
        TuiPanel::Sessions => "Switch session",
        TuiPanel::Skills => "Skills",
        TuiPanel::Cards => "Tool cards",
        TuiPanel::Help(_) => "Help",
        TuiPanel::Dcp => "DCP context",
    };
    let size = size_for(state.panel());
    state
        .select
        .render(frame, title, size, &state.modal_options(), None);
}

/// The detail viewport has a fixed header and footer and no searchable list.
/// Use the same geometry for wrapping, keyboard navigation and painting.
pub(crate) fn card_geometry(area: Rect) -> (Rect, usize, usize) {
    let rect = DialogFrame::rect(area, DialogSize::Xlarge, (area.height / 2).max(8));
    if rect.width < 12 || rect.height < 7 {
        return (rect, 0, 0);
    }
    (
        rect,
        rect.width.saturating_sub(8) as usize,
        rect.height.saturating_sub(5) as usize,
    )
}

fn render_card_detail(frame: &mut Frame<'_>, state: &TuiState) {
    let (rect, width, height) = card_geometry(frame.area());
    DialogFrame::paint(frame, rect, Theme::dark());
    if width == 0 || height == 0 {
        return;
    }
    let theme = Theme::dark();
    let text = slot(theme, "text.base");
    let muted = slot(theme, "text.muted");
    let lines = crate::views::panel_lines(state);
    let line = |frame: &mut Frame<'_>, y: u16, content: &str, style: Style| {
        frame.render_widget(
            Paragraph::new(ratatui::text::Line::styled(content.to_string(), style)),
            Rect::new(rect.x + 4, y, width as u16, 1),
        );
    };
    line(
        frame,
        rect.y + 1,
        "Tool result",
        Style::default().fg(text).add_modifier(Modifier::BOLD),
    );
    for (index, content) in lines.iter().enumerate() {
        let y = if index == 0 {
            rect.y + 2
        } else if index == lines.len() - 1 {
            rect.bottom() - 2
        } else {
            rect.y + 2 + index as u16
        };
        line(
            frame,
            y,
            content,
            Style::default().fg(if index == 0 || index == lines.len() - 1 {
                muted
            } else {
                text
            }),
        );
    }
    let (start, visible, count) = crate::views::card_window(state);
    state.card_rows_painted(start, (start + visible).min(count));
}

pub fn size_for(panel: &TuiPanel) -> DialogSize {
    match panel {
        TuiPanel::Cards => DialogSize::Xlarge,
        TuiPanel::Dcp | TuiPanel::Help(_) => DialogSize::Large,
        _ => DialogSize::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v04_full_catalog_cpu_and_bounded_cache() {
        use std::time::{Duration, Instant};
        let cold_limit = if cfg!(debug_assertions) {
            Duration::from_secs(2)
        } else {
            Duration::from_millis(500)
        };
        let options = Rc::new(
            (0..10_000)
                .map(|i| SelectOption {
                    value: format!("model-{i:05}"),
                    title: format!("{} abcdefghijklmnopqrstuvwxyz {i:05}", "a".repeat(790)),
                    category: String::new(),
                    footer: String::new(),
                    current: false,
                })
                .collect::<Vec<_>>(),
        );
        assert!(options.iter().map(|o| o.title.len()).sum::<usize>() <= 8 * 1024 * 1024);
        let mut list = SelectList::default();
        let queries = [
            "a".repeat(512),
            format!("{}z", "a".repeat(511)),
            "not_present_Ω".repeat(30),
            (0..128)
                .map(|i| {
                    format!(
                        "a{}{}",
                        (b'a' + (i / 26) as u8) as char,
                        (b'a' + (i % 26) as u8) as char
                    )
                })
                .collect::<Vec<_>>()
                .join(" "),
        ];
        for (i, query) in queries.into_iter().enumerate() {
            list.query = query;
            let start = Instant::now();
            let result = list.filter_for(options.clone(), &TuiPanel::Model);
            let cold = start.elapsed();
            if i < 2 {
                assert_eq!(result.len(), 10_000, "no admissible model dropped");
            }
            let warm = Instant::now();
            for _ in 0..100 {
                assert!(Rc::ptr_eq(
                    &result,
                    &list.filter_for(options.clone(), &TuiPanel::Model)
                ));
            }
            let warm = warm.elapsed();
            eprintln!(
                "V04 catalog=10000 text_bytes={} query_bytes={} cold={cold:?} cached100={warm:?}",
                options.iter().map(|o| o.title.len()).sum::<usize>(),
                list.query.len()
            );
            assert!(cold < cold_limit, "cold matching latency {cold:?}");
            assert!(
                warm < Duration::from_millis(50),
                "unchanged frames recomputed matching"
            );
        }
        assert_eq!(list.cache.borrow().as_ref().unwrap().prepared.len(), 10_000);
        // Every distinct query word matches: exercise the non-short-circuit
        // multiword path, plus Latin decomposition and non-Latin UTF-16 targets.
        for body in [
            "abcdefghijklmnopqrstuvwxyz".repeat(31),
            format!(
                "{}{}",
                "á".repeat(270),
                "abcdefghijklmnopqrstuvwxyz".repeat(10)
            ),
            format!(
                "{}{}",
                "Ω".repeat(270),
                "abcdefghijklmnopqrstuvwxyz".repeat(10)
            ),
        ] {
            let options = Rc::new(
                (0..10_000)
                    .map(|i| SelectOption {
                        value: i.to_string(),
                        title: format!("{body}{i:05}"),
                        category: String::new(),
                        footer: String::new(),
                        current: false,
                    })
                    .collect::<Vec<_>>(),
            );
            let start = Instant::now();
            let result = list.filter_for(options.clone(), &TuiPanel::Model);
            let elapsed = start.elapsed();
            assert_eq!(result.len(), 10_000);
            eprintln!(
                "V04 all-words-match text_bytes={} cold={elapsed:?}",
                options.iter().map(|o| o.title.len()).sum::<usize>()
            );
            assert!(elapsed < cold_limit, "cold matching latency {elapsed:?}");
        }
        list.reset();
        assert!(list.cache.borrow().is_none());
    }
    #[test]
    fn captured_backdrop_blanks_and_modal_padding_keep_default_foreground() {
        use ratatui::{Terminal, backend::TestBackend, text::Line};
        let mut terminal = Terminal::new(TestBackend::new(160, 48)).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new(Line::styled(
                        "A B",
                        Style::default()
                            .fg(Color::Rgb(238, 238, 238))
                            .add_modifier(Modifier::BOLD),
                    )),
                    Rect::new(0, 0, 3, 1),
                );
                SelectList::default().render(
                    frame,
                    "Select variant",
                    DialogSize::Medium,
                    &[SelectOption {
                        value: String::new(),
                        title: "Default".into(),
                        category: String::new(),
                        footer: String::new(),
                        current: true,
                    }],
                    None,
                );
            })
            .unwrap();
        let b = terminal.backend().buffer();
        assert_eq!(b[(0, 0)].fg, Color::Rgb(98, 98, 98));
        assert_eq!(b[(0, 0)].modifier, Modifier::BOLD);
        for (x, y) in [(1, 0), (100, 1), (50, 12), (108, 13), (90, 17)] {
            assert_eq!(b[(x, y)].symbol(), " ");
            assert_eq!(
                b[(x, y)].fg,
                CANVAS_FOREGROUND,
                "blank foreground at {x},{y}"
            );
            assert_eq!(
                b[(x, y)].modifier,
                Modifier::empty(),
                "blank style at {x},{y}"
            );
        }
        // A space that is part of newly drawn modal text keeps its text styling.
        assert_eq!(b[(60, 13)].fg, Theme::dark().text());
        assert_eq!(b[(60, 13)].modifier, Modifier::BOLD);
    }

    #[test]
    fn modal_surface_erases_underlay_symbols() {
        use ratatui::{Terminal, backend::TestBackend};
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let rect = DialogFrame::rect(Rect::new(0, 0, 80, 24), DialogSize::Medium, 10);
        terminal
            .draw(|frame| {
                let rows = vec!["X".repeat(80); 24].join("\n");
                frame.render_widget(Paragraph::new(rows), frame.area());
                DialogFrame::paint(frame, rect, Theme::dark());
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].symbol(), "X");
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                assert_eq!(
                    buffer[(x, y)].symbol(),
                    " ",
                    "underlay glyph leaked through modal at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn constrained_sizes_and_rgb_alpha() {
        for (size, width) in [
            (DialogSize::Medium, 60),
            (DialogSize::Large, 88),
            (DialogSize::Xlarge, 116),
        ] {
            assert_eq!(
                DialogFrame::rect(Rect::new(0, 0, 160, 48), size, 20),
                Rect::new((160 - width) / 2, 12, width, 20)
            );
            let r = DialogFrame::rect(Rect::new(0, 0, 43, 24), size, 40);
            assert_eq!(r, Rect::new(1, 6, 41, 18));
        }
        assert_eq!(
            backdrop(Color::Rgb(255, 128, 10), Color::Black),
            Color::Rgb(105, 53, 4)
        );
    }
}
