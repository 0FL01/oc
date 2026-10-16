//! Execute interaction state stays on the owning TUI view and operation.
use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};

pub(crate) struct Detail {
    pub operation: String,
    pub scroll: std::cell::Cell<usize>,
    pub pan: std::cell::Cell<usize>,
    pub copied: Option<char>,
    pub copy_pending: Option<char>,
    previous: TuiPanel,
    pressed: Option<(char, Rect, Rect, (u16, u16))>,
}

impl TuiState {
    pub(crate) fn execute_card(&self, operation: &str) -> Option<&crate::history::ToolCard> {
        self.live_parts
            .iter()
            .find_map(|part| match part {
                LivePart::Tool { card, .. } if card.op == operation => Some(card.as_ref()),
                _ => None,
            })
            .or_else(|| {
                self.window
                    .rows()
                    .iter()
                    .find_map(|row| row.tool.as_ref().filter(|card| card.op == operation))
            })
            .filter(|card| crate::tools::execute::facts(card).is_some())
    }

    pub(super) fn activate_execute(&mut self, operation: &str, call: Option<usize>, frame: Rect) {
        let Some(card) = self.execute_card(operation) else {
            return;
        };
        let facts = crate::tools::execute::facts(card).expect("validated execute card");
        if let Some(index) = call {
            if index >= facts.calls.len() {
                return;
            }
            let area = crate::shell::transcript_area(self, frame);
            let (_, before, displayed) =
                self.visible_transcript_at_viewport(area.width, frame.width, area.height);
            let first = before
                .saturating_sub(area.height as usize)
                .saturating_sub(displayed);
            let following = displayed == 0 && !self.window.has_newer();
            let rows = self.transcript_rows();
            self.execute_calls_expanded.retain(|op, _| {
                rows.iter()
                    .any(|row| row.tool.as_ref().is_some_and(|card| &card.op == op))
            });
            let calls = self
                .execute_calls_expanded
                .entry(operation.to_owned())
                .or_default();
            if !calls.insert(index) {
                calls.remove(&index);
            }
            self.invalidate_transcript();
            let (_, after) = self.visible_transcript(area.width, frame.width, area.height);
            self.scroll = if following {
                0
            } else {
                after
                    .saturating_sub(area.height as usize)
                    .saturating_sub(first)
            };
            self.observe_transcript_viewport(
                area.width,
                frame.width,
                area.height,
                after,
                self.scroll,
            );
        } else if facts.error || crate::tools::is_error_state(&card.state) {
            self.toggle_tool_expansion_at(operation, frame);
        } else {
            self.execute_detail = Some(Detail {
                operation: operation.to_owned(),
                scroll: std::cell::Cell::new(0),
                pan: std::cell::Cell::new(0),
                copied: None,
                copy_pending: None,
                previous: std::mem::replace(&mut self.panel, TuiPanel::Execute),
                pressed: None,
            });
            self.mouse_down = None;
        }
    }

    pub(super) fn execute_key(&mut self, action: KeyAction) -> KeyOutcome {
        if matches!(action, KeyAction::Cancel | KeyAction::Interrupt) {
            if let Some(detail) = self.execute_detail.take() {
                self.panel = detail.previous;
            }
            self.mouse_down = None;
            return KeyOutcome::default();
        }
        let layout = crate::execute_view::layout(self, self.detail_area());
        if let Some(detail) = &mut self.execute_detail {
            let end = layout
                .rows
                .len()
                .saturating_sub(layout.body.height as usize);
            let start = detail.scroll.get().min(end);
            detail.scroll.set(match action {
                KeyAction::Up => start.saturating_sub(1),
                KeyAction::Down => (start + 1).min(end),
                KeyAction::PageUp => start.saturating_sub(layout.body.height as usize),
                KeyAction::PageDown => (start + layout.body.height as usize).min(end),
                KeyAction::Home => 0,
                KeyAction::End => end,
                _ => start,
            });
            detail.pan.set(match action {
                KeyAction::Left => detail.pan.get().saturating_sub(8),
                KeyAction::Right => (detail.pan.get() + 8).min(layout.max_pan),
                _ => detail.pan.get().min(layout.max_pan),
            });
            detail.pressed = None;
        }
        if let KeyAction::Char(key @ ('c' | 'o')) = action {
            let text = self
                .execute_detail
                .as_ref()
                .and_then(|detail| self.execute_card(&detail.operation))
                .and_then(crate::tools::execute::facts)
                .and_then(|facts| {
                    if key == 'c' {
                        facts.code.clone()
                    } else if facts.output.is_empty() {
                        None
                    } else {
                        Some(
                            facts
                                .output
                                .iter()
                                .map(|part| part.text.as_str())
                                .collect::<Vec<_>>()
                                .join("\n\n"),
                        )
                    }
                });
            if let Some(text) = text {
                if let Err(error) = self.copy_message_text(text) {
                    self.push_transient_note(&error, NoteVariant::Error);
                } else if let Some(detail) = &mut self.execute_detail {
                    detail.copy_pending = Some(key);
                }
            } else {
                self.push_transient_note("Captured output is not available yet", NoteVariant::Info);
            }
        }
        KeyOutcome::default()
    }

    pub(super) fn execute_mouse(&mut self, event: MouseEvent, area: Rect) -> KeyOutcome {
        let layout = crate::execute_view::layout(self, area);
        let hit = layout.actions.iter().find_map(|(key, rect)| {
            rect.contains((event.column, event.row).into())
                .then_some((*key, *rect))
        });
        match event.kind {
            MouseEventKind::ScrollUp => return self.execute_key(KeyAction::Up),
            MouseEventKind::ScrollDown => return self.execute_key(KeyAction::Down),
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                if let Some(detail) = &mut self.execute_detail {
                    detail.pressed =
                        hit.map(|(key, rect)| (key, area, rect, (event.column, event.row)));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self
                    .execute_detail
                    .as_mut()
                    .and_then(|detail| detail.pressed.take());
                if event.modifiers.is_empty()
                    && let Some((key, rect)) = hit
                    && pressed == Some((key, area, rect, (event.column, event.row)))
                {
                    return self.execute_key(if key == 'e' {
                        KeyAction::Cancel
                    } else {
                        KeyAction::Char(key)
                    });
                }
            }
            MouseEventKind::Moved => {}
            _ => {
                if let Some(detail) = &mut self.execute_detail {
                    detail.pressed = None;
                }
            }
        }
        KeyOutcome::default()
    }
}
