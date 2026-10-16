//! Disposable read-only modal over the opened job's original capture identity.
use super::*;
use crate::app::TuiPanel;
use crate::dialog::{DialogFrame, DialogSize};
use oc_core::queries::ShellSnapshot;
use ratatui::{
    Frame,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

pub(super) struct Paint {
    frame: Rect,
    area: Rect,
    close: Rect,
    viewport: Rect,
    max_scroll: usize,
    session: SessionId,
    operation: String,
    generation: u64,
}

impl ShellView {
    pub(super) fn output_key(&mut self, key: &KeyAction) -> Option<KeyOutcome> {
        let paint = self.output_paint.borrow();
        let page = paint.as_ref().map_or(3, |p| p.viewport.height.max(1)) as usize;
        let max = paint.as_ref().map_or(0, |p| p.max_scroll);
        let at = self.output_scroll.get();
        let next = match key {
            KeyAction::Up => at.saturating_sub(1),
            KeyAction::Down => at.saturating_add(1).min(max),
            KeyAction::PageUp => at.saturating_sub(page),
            KeyAction::PageDown => at.saturating_add(page).min(max),
            KeyAction::Home => 0,
            KeyAction::End => max,
            _ => return None,
        };
        self.output_follow.set(
            *key == KeyAction::End
                || matches!(key, KeyAction::Down | KeyAction::PageDown) && next == max,
        );
        self.output_scroll.set(next);
        self.output_down = None;
        Some(KeyOutcome::default())
    }
    fn matches_output(&self, job: &ShellJob) -> bool {
        self.viewer.as_ref().is_some_and(|opened| {
            opened.session == job.session
                && opened.shell_id == job.shell_id
                && opened.generation == job.generation
                && opened.location == job.location
        })
    }
}

impl TuiState {
    /// Snapshot reads never adopt another job, even after inventory removal or a
    /// same-ID Location switch. Terminal status is typed data, not output parsing.
    pub fn apply_shell_snapshot(&mut self, snapshot: ShellSnapshot) {
        if !self.shells.matches_output(&snapshot.job) || snapshot.display.len() > 65536 {
            return;
        }
        self.shells.output_state = snapshot.state;
        self.shells.output_exit = snapshot.exit;
        self.shells.output_omitted = snapshot.display_omitted;
        self.shells.output_error = false;
        self.shells.output = Some(ToolOutputPage {
            total_bytes: snapshot.display.len() as i64,
            text: snapshot.display,
            next_offset: None,
        });
        self.shells.output_paint.get_mut().take();
        self.shells.output_down = None;
    }
    pub fn apply_shell_read_failure(&mut self, job: &ShellJob) {
        if self.shells.matches_output(job) {
            self.shells.output_error = true;
            self.shells.output_paint.get_mut().take();
            self.shells.output_down = None;
        }
    }
    pub(crate) fn shell_output_mouse(&mut self, event: MouseEvent, frame: Rect) -> KeyOutcome {
        let hit = {
            let paint = self.shells.output_paint.borrow();
            paint
                .as_ref()
                .filter(|p| {
                    p.frame == frame
                        && self.shells.viewer.as_ref().is_some_and(|j| {
                            j.session == p.session
                                && j.shell_id == p.operation
                                && j.generation == p.generation
                        })
                })
                .map(|p| {
                    (
                        p.area.contains((event.column, event.row).into()),
                        p.close.contains((event.column, event.row).into()),
                        p.viewport.contains((event.column, event.row).into()),
                    )
                })
        };
        let Some((inside, close, viewport)) = hit else {
            self.shells.output_down = None;
            return KeyOutcome::default();
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                self.shells.output_down = (close || !inside).then_some(close);
            }
            MouseEventKind::Up(MouseButton::Left) if event.modifiers.is_empty() => {
                if self.shells.output_down.take() == Some(close) && (close || !inside) {
                    return self.shells.key(KeyAction::Cancel);
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown if viewport => {
                self.shells
                    .output_key(&if event.kind == MouseEventKind::ScrollUp {
                        KeyAction::Up
                    } else {
                        KeyAction::Down
                    });
            }
            MouseEventKind::Down(_) | MouseEventKind::Up(_) | MouseEventKind::Drag(_) => {
                self.shells.output_down = None
            }
            _ => {}
        }
        // The modal owns its entire backdrop too. No pointer leaks to the deck,
        // prompt, live footer, lower composer or a raw PTY beneath it.
        KeyOutcome::default()
    }
}

pub(crate) fn render(frame: &mut Frame<'_>, state: &TuiState, whole: Rect) {
    state.shells.output_paint.borrow_mut().take();
    if *state.panel() != TuiPanel::None
        || state.approvals.active().is_some()
        || state.questions.active().is_some()
    {
        return;
    }
    let Some(job) = state.shells.viewer.as_ref() else {
        return;
    };
    let theme = Theme::dark();
    let base = theme
        .color("@dialog.text.base")
        .unwrap_or_else(|| theme.text());
    let muted = theme
        .color("@dialog.text.muted")
        .unwrap_or_else(|| theme.text_muted());
    let bg = theme
        .color("@dialog.background.base")
        .unwrap_or_else(|| theme.background());
    let width = 116u16.min(whole.width.saturating_sub(2));
    let inner_width = width.saturating_sub(4);
    if inner_width == 0 || whole.height < 3 {
        return;
    }
    let command = job
        .command
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>();
    let command_paragraph = Paragraph::new(command).wrap(Wrap { trim: false });
    let command_rows = command_paragraph.line_count(inner_width).clamp(1, 3) as u16;
    let wanted_output = ((whole.height as u32 * 3 / 5) as u16)
        .saturating_sub(6)
        .max(3);
    let extra =
        u16::from(state.shells.output_omitted) * 2 + u16::from(state.shells.output_error) * 2;
    let height = (wanted_output + command_rows + 7 + extra).min(whole.height);
    let mut area = DialogFrame::rect(whole, DialogSize::Xlarge, height);
    area.y = whole.y + whole.height.saturating_sub(height) / 2;
    area.height = height;
    DialogFrame::paint(frame, area, theme);
    let style = Style::default().fg(base).bg(bg);
    let muted_style = Style::default().fg(muted).bg(bg);
    let content = Rect::new(
        area.x + 2,
        area.y + 1,
        inner_width,
        area.height.saturating_sub(2),
    );
    let status = match state.shells.output_state.as_str() {
        "admitted" | "running" | "started" => "Running".into(),
        "cancelled" => "Killed".into(),
        "timed_out" => "Timed out".into(),
        "unknown" => "Unknown".into(),
        _ => state
            .shells
            .output_exit
            .map_or_else(|| "Exited".into(), |code| format!("Exited · code {code}")),
    };
    let status_width = status.width().min(inner_width as usize) as u16;
    let close = Rect::new(
        content.right().saturating_sub(3).max(content.x),
        content.y,
        3.min(inner_width),
        1,
    );
    let status_x = close.x.saturating_sub(status_width + 2).max(content.x);
    frame.render_widget(
        Paragraph::new("Shell output").style(style.add_modifier(Modifier::BOLD)),
        Rect::new(
            content.x,
            content.y,
            status_x.saturating_sub(content.x + 2),
            1,
        ),
    );
    frame.render_widget(
        Paragraph::new(status).style(muted_style),
        Rect::new(status_x, content.y, status_width, 1),
    );
    frame.render_widget(Paragraph::new("esc").style(muted_style), close);
    frame.render_widget(
        command_paragraph.style(muted_style),
        Rect::new(content.x, content.y + 2, inner_width, command_rows).intersection(area),
    );
    let mut output_y = content.y + 3 + command_rows;
    if state.shells.output_omitted {
        frame.render_widget(
            Paragraph::new("Earlier output omitted · showing recent output").style(muted_style),
            Rect::new(content.x, output_y, inner_width, 1).intersection(area),
        );
        output_y += 2;
    }
    let output_height = wanted_output.min(
        area.bottom()
            .saturating_sub(output_y + 3 + u16::from(state.shells.output_error) * 2),
    );
    let viewport = Rect::new(
        content.x,
        output_y.min(area.bottom()),
        inner_width,
        output_height,
    );
    let text = state
        .shells
        .output
        .as_ref()
        .map(|p| p.text.replace("\r\n", "\n").replace('\r', "\n"));
    let text =
        text.as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(if state.shells.output.is_some() {
                "No captured output. Output redirected to files is not shown here."
            } else {
                "Loading output…"
            });
    // Ratatui Text::from(&str) drops the last empty line; the pinned plain
    // scrollbox retains it. Preserve actual process newlines, including the
    // final blank row, rather than changing the viewport or cropping output.
    let mut body = ratatui::text::Text::from(text);
    if text.ends_with('\n') {
        body.lines.push(Line::default());
    }
    let paragraph = Paragraph::new(body).style(style).wrap(Wrap { trim: false });
    let max_scroll = paragraph
        .line_count(inner_width)
        .saturating_sub(output_height as usize);
    let scroll = if state.shells.output_follow.get() {
        max_scroll
    } else {
        state.shells.output_scroll.get().min(max_scroll)
    };
    state.shells.output_scroll.set(scroll);
    frame.render_widget(
        paragraph.scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        viewport,
    );
    let mut footer_y = viewport.bottom() + 1;
    if state.shells.output_error {
        let error = theme
            .color("@dialog.text.feedback.error.base")
            .unwrap_or_else(|| theme.error());
        frame.render_widget(
            Paragraph::new("Unable to read shell output. Retrying…").style(style.fg(error)),
            Rect::new(content.x, footer_y, inner_width, 1).intersection(area),
        );
        footer_y += 2;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("↑/↓ scroll", muted_style),
            Span::styled("  end follow  esc back", muted_style),
        ])),
        Rect::new(
            content.x,
            footer_y.min(area.bottom()),
            inner_width,
            u16::from(footer_y < area.bottom()),
        ),
    );
    state.shells.output_paint.replace(Some(Paint {
        frame: whole,
        area,
        close,
        viewport,
        max_scroll,
        session: job.session.clone(),
        operation: job.shell_id.clone(),
        generation: job.generation,
    }));
}

#[cfg(test)]
mod tests;
