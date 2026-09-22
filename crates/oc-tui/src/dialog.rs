//! Shared native modal surface and searchable, scrolling selection list.
//! Geometry/spacing follows pinned upstream ui/dialog{,-select}.tsx.
use std::cell::Cell;

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
        // Composite over actual rendered RGB cells, including styled blanks.
        for cell in &mut frame.buffer_mut().content {
            cell.fg = backdrop(cell.fg, theme.text());
            cell.bg = backdrop(cell.bg, theme.background());
        }
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default().style(
                Style::default()
                    .fg(slot(theme, "text.base"))
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

#[derive(Clone, Debug)]
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
}

impl SelectList {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn filter(&self, options: Vec<SelectOption>) -> Vec<SelectOption> {
        let needle = self.query.to_lowercase();
        options
            .into_iter()
            .filter(|o| {
                let hay = format!("{} {} {}", o.title, o.category, o.value).to_lowercase();
                needle.split_whitespace().all(|part| hay.contains(part))
            })
            .collect()
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
        let list_height = rows
            .len()
            .max(1)
            .min((frame.area().height / 2).saturating_sub(6).max(1) as usize);
        let area = DialogFrame::rect(
            frame.area(),
            size,
            list_height as u16 + 7 + u16::from(footer.is_some()),
        );
        DialogFrame::paint(frame, area, theme);
        if area.width < 12 || area.height < 5 {
            return;
        }
        let text = slot(theme, "text.base");
        let muted = slot(theme, "text.muted");
        let line = |frame: &mut Frame<'_>, x, y, width, text: &str, style| {
            if y < area.bottom() {
                frame.render_widget(
                    Paragraph::new(text.chars().filter(|c| !c.is_control()).collect::<String>())
                        .style(style),
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
        let height = list_height
            .min(area.height.saturating_sub(7) as usize)
            .max(1);
        let selected_row = rows
            .iter()
            .position(|(index, _)| *index == Some(self.cursor))
            .unwrap_or(0);
        let mut offset = self.offset.get().min(rows.len().saturating_sub(height));
        if selected_row < offset {
            offset = selected_row;
        }
        if selected_row >= offset + height {
            offset = selected_row + 1 - height;
        }
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
                Block::default().style(style),
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
    let title = match state.panel() {
        TuiPanel::None => return,
        TuiPanel::Commands => "Commands",
        TuiPanel::Model => "Select model",
        TuiPanel::Agents => "Select agent",
        TuiPanel::Sessions => "Switch session",
        TuiPanel::Skills => "Skills",
        TuiPanel::Cards => "Tool cards",
        TuiPanel::Help(_) => "Help",
        TuiPanel::Dcp => "DCP context",
    };
    let footer = if *state.panel() == TuiPanel::Model {
        state
            .picker
            .as_ref()
            .filter(|p| !p.variants().is_empty())
            .map(|_| {
                format!(
                    "←/→ variant: {}",
                    state
                        .picker_selection()
                        .and_then(|(_, v)| v)
                        .unwrap_or_else(|| "default".into())
                )
            })
    } else {
        None
    };
    let size = match state.panel() {
        TuiPanel::Cards => DialogSize::Xlarge,
        TuiPanel::Dcp | TuiPanel::Help(_) => DialogSize::Large,
        _ => DialogSize::Medium,
    };
    state.select.render(
        frame,
        title,
        size,
        &state.modal_options(),
        footer.as_deref(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
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
