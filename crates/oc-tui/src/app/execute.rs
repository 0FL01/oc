//! Execute interaction state stays on the owning TUI view and operation.
use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use std::cell::{Cell, Ref, RefCell};

pub(crate) struct Captured {
    pub facts: oc_core::tool_output::Execute,
    pub running: bool,
    pub failed: bool,
    pub fallback: Option<oc_core::tool_output::ExecuteOutput>,
}

impl Captured {
    fn new(card: &crate::history::ToolCard) -> Option<Self> {
        let mut facts = crate::tools::execute::facts(card)?.clone();
        // Details need code/output, not another copy of child parameter disclosures.
        facts.calls.clear();
        let failed = crate::tools::execute::failed(card);
        let fallback = if facts.output.is_empty() && failed {
            crate::tools::execute::output_text(card).map(|text| {
                oc_core::tool_output::ExecuteOutput {
                    kind: oc_core::tool_output::ExecuteOutputKind::Text,
                    text,
                    limited: card.preview_limited(),
                }
            })
        } else {
            None
        };
        Some(Self {
            facts,
            running: crate::tools::execute::running(card),
            failed,
            fallback,
        })
    }

    pub fn output(&self) -> impl Iterator<Item = &oc_core::tool_output::ExecuteOutput> {
        self.facts.output.iter().chain(self.fallback.iter())
    }
}

pub(crate) struct Detail {
    pub operation: String,
    pub scroll: Cell<usize>,
    pub copied: Option<char>,
    pub copy_pending: Option<char>,
    previous: TuiPanel,
    captured: RefCell<Captured>,
    pressed: Cell<Option<Press>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Press {
    key: char,
    area: Rect,
    target: Rect,
    point: (u16, u16),
}

impl Detail {
    pub(super) fn retained_bytes(&self) -> usize {
        let captured = self.captured.borrow();
        self.operation.len()
            + captured.facts.retained_bytes()
            + captured.fallback.as_ref().map_or(0, |part| part.text.len())
    }
}

impl TuiState {
    /// Refresh only the exact original operation in this view, never a new turn.
    fn apply_execute_presentation(
        &mut self,
        job: &oc_core::queries::ExecuteJob,
        output: &oc_core::tool_output::Presentation,
    ) {
        if self.attached_session() != Some(&job.session) || !output.is_valid() {
            return;
        }
        let mut changed = self.window.refresh_execute(job, output);
        if self
            .active_turn
            .as_ref()
            .is_some_and(|turn| turn.0 == job.turn)
        {
            for part in &mut self.live_parts {
                if let LivePart::Tool { card, .. } = part {
                    changed |= crate::history::edit_card(
                        card,
                        |card| card.name == "execute" && card.op == job.execute_id,
                        |card| card.refresh_execute(output),
                    );
                }
            }
        }
        if changed {
            self.enforce_parts();
        }
    }

    pub fn apply_execute_jobs(&mut self, mut jobs: Vec<oc_core::queries::ExecuteJob>) {
        jobs.truncate(8);
        for job in &jobs {
            if let Some(output) = &job.output {
                self.apply_execute_presentation(job, output);
            }
        }
        self.execute_jobs = jobs;
        self.sync_foreground_hint(Instant::now());
    }

    pub fn apply_execute_snapshot(&mut self, snapshot: &oc_core::queries::ExecuteSnapshot) {
        if let Some(output) = &snapshot.output {
            self.apply_execute_presentation(&snapshot.job, output);
        }
    }

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
                    .find_map(|row| row.tool.as_deref().filter(|card| card.op == operation))
            })
            .filter(|card| crate::tools::execute::facts(card).is_some())
    }

    /// One bounded, view-local capture survives replacement of the resident page.
    /// Current admitted facts always win; missing source never implies completion.
    pub(crate) fn execute_capture(&self) -> Option<(Ref<'_, Captured>, bool)> {
        let detail = self.execute_detail.as_ref()?;
        let current = self.execute_card(&detail.operation).and_then(Captured::new);
        let available = current.is_some();
        if let Some(current) = current {
            *detail.captured.borrow_mut() = current;
        }
        Some((detail.captured.borrow(), available))
    }

    pub(crate) fn cancel_execute_pointer(&self) {
        if let Some(detail) = &self.execute_detail {
            detail.pressed.set(None);
        }
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
            let (rows, _, _) = self.compose_structure(true);
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
            if !self.projection_ready((area.width, frame.width, area.height)) {
                return;
            }
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
        } else {
            let captured = Captured::new(card).expect("validated execute card");
            self.execute_detail = Some(Detail {
                operation: operation.to_owned(),
                scroll: Cell::new(0),
                copied: None,
                copy_pending: None,
                previous: std::mem::replace(&mut self.panel, TuiPanel::Execute),
                captured: RefCell::new(captured),
                pressed: Cell::new(None),
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
            detail.pressed.set(None);
        }
        if let KeyAction::Char(key @ ('c' | 'o')) = action {
            let text = self.execute_capture().and_then(|(captured, _)| {
                if key == 'c' {
                    captured.facts.code.clone()
                } else if captured.output().next().is_none() {
                    None
                } else {
                    Some(
                        captured
                            .output()
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
        let hit = layout
            .actions
            .iter()
            .find_map(|(key, rect)| {
                rect.contains((event.column, event.row).into())
                    .then_some((*key, *rect))
            })
            .or_else(|| {
                (area.contains((event.column, event.row).into())
                    && !layout.rect.contains((event.column, event.row).into()))
                .then_some(('b', layout.rect))
            });
        match event.kind {
            MouseEventKind::ScrollUp => return self.execute_key(KeyAction::Up),
            MouseEventKind::ScrollDown => return self.execute_key(KeyAction::Down),
            MouseEventKind::Down(MouseButton::Left) if event.modifiers.is_empty() => {
                if let Some(detail) = &mut self.execute_detail {
                    detail.pressed.set(hit.map(|(key, target)| Press {
                        key,
                        area,
                        target,
                        point: (event.column, event.row),
                    }));
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let pressed = self
                    .execute_detail
                    .as_mut()
                    .and_then(|detail| detail.pressed.take());
                if event.modifiers.is_empty()
                    && let Some((key, rect)) = hit
                    && pressed
                        == Some(Press {
                            key,
                            area,
                            target: rect,
                            point: (event.column, event.row),
                        })
                {
                    return self.execute_key(if matches!(key, 'e' | 'b') {
                        KeyAction::Cancel
                    } else {
                        KeyAction::Char(key)
                    });
                }
            }
            _ => {
                self.cancel_execute_pointer();
            }
        }
        KeyOutcome::default()
    }
}
