//! Mouse ownership for the editor and the two readonly chrome surfaces.
//! The transcript keeps its existing word/line and message-click contracts.
use super::*;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextSource {
    Sidebar,
    Location,
}

#[derive(Clone, PartialEq, Eq)]
struct PaintedText {
    source: TextSource,
    frame: Rect,
    home: bool,
    session: Option<SessionId>,
    location: Option<String>,
    generation: u64,
    inventory: Option<(oc_core::queries::McpBinding, u64)>,
    rows: Vec<(Rect, Line)>,
}

enum Selection {
    Prompt(PaintedPrompt),
    Text {
        paint: PaintedText,
        anchor: TextPoint,
        focus: TextPoint,
    },
}

#[derive(Default)]
pub(super) struct SelectionState {
    selected: Option<Selection>,
    // A cancelled press still owns its eventual release: never let it activate
    // a release-only target in a different surface.
    held: bool,
    dragged: bool,
    transcript: bool,
    invalid: std::cell::Cell<bool>,
    painted: std::cell::RefCell<[Option<PaintedText>; 2]>,
}

fn slot(source: TextSource) -> usize {
    match source {
        TextSource::Sidebar => 0,
        TextSource::Location => 1,
    }
}

fn bounds(a: TextPoint, b: TextPoint) -> (TextPoint, TextPoint) {
    if a <= b { (a, b) } else { (b, a) }
}

fn text_point(paint: &PaintedText, x: u16, y: u16) -> Option<TextPoint> {
    let (first, _) = paint.rows.first()?;
    let (last, line) = paint.rows.last()?;
    if y < first.y {
        return Some(TextPoint { row: 0, byte: 0 });
    }
    if y >= last.bottom() {
        return Some(TextPoint {
            row: paint.rows.len() - 1,
            byte: line.plain_text().len(),
        });
    }
    let row = paint
        .rows
        .iter()
        .position(|(rect, _)| rect.y >= y)
        .or_else(|| paint.rows.len().checked_sub(1))?;
    let (rect, line) = &paint.rows[row];
    Some(TextPoint {
        row,
        byte: line.byte_at_cell(x.saturating_sub(rect.x) as usize),
    })
}

fn text_intervals(
    paint: &PaintedText,
    a: TextPoint,
    b: TextPoint,
) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let (a, b) = bounds(a, b);
    (a.row..=b.row).filter_map(move |row| {
        let (_, line) = paint.rows.get(row)?;
        Some((
            row,
            if row == a.row { a.byte } else { 0 },
            if row == b.row {
                b.byte
            } else {
                line.plain_text().len()
            },
        ))
    })
}

impl TuiState {
    fn text_paint(&self, source: TextSource, frame: Rect, rows: Vec<(Rect, Line)>) -> PaintedText {
        PaintedText {
            source,
            frame,
            rows,
            home: self.home,
            session: self.session.clone(),
            location: self.chrome.location.clone(),
            generation: self.generation,
            inventory: (source == TextSource::Sidebar)
                .then(|| {
                    self.mcp_snapshot
                        .as_ref()
                        .map(|snapshot| (snapshot.binding.clone(), snapshot.revision))
                })
                .flatten(),
        }
    }

    pub(crate) fn clear_text_paint(&self, frame: Rect) {
        let current = match &self.text_selection.selected {
            Some(Selection::Prompt(paint)) => self.prompt_capture_current(paint, frame),
            Some(Selection::Text { paint, .. }) => self.text_source_current(paint, frame),
            None => true,
        };
        if !current {
            self.text_selection.invalid.set(true);
        }
        self.text_selection.painted.replace([None, None]);
    }

    fn text_controls_available(&self) -> bool {
        self.transcript_controls_available() && !self.composer_open()
    }

    fn transcript_controls_available(&self) -> bool {
        self.panel == TuiPanel::None
            && self.approvals.active().is_none()
            && self.questions.active().is_none()
            && self.shell_viewer().is_none()
            && !self.terminal_focused()
            && self.status != TuiStatus::Quit
    }

    fn text_paint_current(&self, paint: &PaintedText, frame: Rect) -> bool {
        !self.text_selection.invalid.get()
            && self.text_selection.painted.borrow()[slot(paint.source)].as_ref() == Some(paint)
            && self.text_source_current(paint, frame)
    }

    fn text_source_current(&self, paint: &PaintedText, frame: Rect) -> bool {
        self.text_controls_available()
            && paint.frame == frame
            && crate::shell::selectable_rows(self, frame, paint.source)
                .is_some_and(|rows| self.text_paint(paint.source, frame, rows) == *paint)
    }

    fn text_occluded(&self, paint: &PaintedText, a: TextPoint, b: TextPoint) -> bool {
        let Some(toast) = crate::shell::text_toast_rect(self, paint.frame) else {
            return false;
        };
        text_intervals(paint, a, b).any(|(row, start, end)| {
            let (rect, line) = &paint.rows[row];
            let text = line.plain_text();
            let x = rect.x + UnicodeWidthStr::width(&text[..start]) as u16;
            let width = UnicodeWidthStr::width(&text[start..end]) as u16;
            !toast
                .intersection(Rect::new(x, rect.y, width, 1))
                .is_empty()
        })
    }

    pub(crate) fn paint_text(
        &self,
        source: TextSource,
        frame: Rect,
        rows: Vec<(Rect, Line)>,
    ) -> Vec<(Rect, Line)> {
        let paint = self.text_paint(source, frame, rows);
        if matches!(&self.text_selection.selected, Some(Selection::Text { paint: old, .. }) if old.source == source && old != &paint)
        {
            self.text_selection.invalid.set(true);
        }
        self.text_selection.painted.borrow_mut()[slot(source)] = Some(paint.clone());
        let mut rows = paint.rows.clone();
        if let Some(Selection::Text {
            paint: selected,
            anchor,
            focus,
        }) = &self.text_selection.selected
            && selected == &paint
            && !self.text_selection.invalid.get()
            && self.text_controls_available()
            && !self.text_occluded(&paint, *anchor, *focus)
        {
            let theme = Theme::dark();
            let background = if source == TextSource::Sidebar {
                theme.background_panel()
            } else {
                theme.background()
            };
            for (row, from, to) in text_intervals(&paint, *anchor, *focus) {
                rows[row].1 = rows[row].1.highlight(from, to, theme.text(), background);
            }
        }
        rows
    }

    pub(super) fn has_new_text_selection(&self) -> bool {
        self.text_selection.selected.is_some()
    }

    pub(super) fn cancel_text_selection(&mut self) {
        self.text_selection.invalid.set(true);
        self.text_selection.selected = None;
        self.pending_copy = None;
        self.cancel_transcript_capture();
    }

    pub(super) fn cancel_transcript_capture(&mut self) {
        if self.text_selection.transcript {
            self.text_selection.transcript = false;
            self.selection_gesture = false;
        }
    }

    pub(super) fn finish_transcript_click(&mut self) {
        self.text_selection.held = false;
        self.cancel_transcript_capture();
    }

    pub(super) fn observe_transcript_gesture(&mut self, event: MouseEvent) {
        if event.kind == MouseEventKind::Down(MouseButton::Left) && self.selection_gesture {
            self.text_selection.held = true;
            self.text_selection.transcript = true;
        }
    }

    pub(super) fn cancel_sidebar_selection(&mut self) {
        if matches!(&self.text_selection.selected, Some(Selection::Text { paint, .. }) if paint.source == TextSource::Sidebar)
        {
            self.cancel_text_selection();
        }
    }

    fn prompt_capture_current(&self, paint: &PaintedPrompt, frame: Rect) -> bool {
        !self.text_selection.invalid.get()
            && self.text_controls_available()
            && self.prompt_mouse_editable()
            && paint.frame == frame
            && paint.main == crate::shell::prompt_main(self, frame)
            && paint.home == self.home
            && paint.session == self.session
            && paint.generation == self.generation
            && paint.revision == self.input_revision
            && paint.input == crate::shell::prompt_text_geometry(self, frame).0
    }

    pub(crate) fn prompt_drag_top(&self) -> Option<usize> {
        match &self.text_selection.selected {
            Some(Selection::Prompt(paint))
                if self.text_selection.held && self.prompt_capture_current(paint, paint.frame) =>
            {
                Some(paint.top)
            }
            _ => None,
        }
    }

    pub(super) fn start_prompt_selection(&mut self) {
        let paint = self.painted_prompt.borrow().clone();
        if let Some(paint) = paint {
            self.text_selection.selected = Some(Selection::Prompt(paint));
            self.text_selection.invalid.set(false);
            self.text_selection.held = true;
            self.text_selection.dragged = false;
        }
    }

    fn prompt_drag_point(&self, paint: &PaintedPrompt, event: MouseEvent) -> Option<usize> {
        if event.row < paint.input.y {
            return paint.rows.iter().flatten().next().map(|&(_, raw)| raw);
        }
        if event.row >= paint.input.bottom() {
            return paint.rows.iter().flatten().last().map(|&(_, raw)| raw);
        }
        let row = event.row.saturating_sub(paint.input.y) as usize;
        let row = row.min(paint.rows.len().checked_sub(1)?);
        let y = paint.input.y + row as u16;
        let x = event
            .column
            .clamp(paint.input.x, paint.input.right().saturating_sub(1));
        if let Some((_, start)) = paint
            .chips
            .iter()
            .find(|(rect, _)| rect.contains((x, y).into()))
        {
            let anchor = self.editor.anchor.unwrap_or(self.editor.cursor);
            return Some(if anchor <= *start {
                self.editor.chip_end(*start)?
            } else {
                *start
            });
        }
        let column = event.column.saturating_sub(paint.input.x) as usize;
        let stops = &paint.rows[row];
        stops
            .iter()
            .rev()
            .find(|&&(cell, _)| cell <= column)
            .or_else(|| stops.first())
            .map(|&(_, offset)| offset)
    }

    fn new_selection_text(&self, frame: Rect) -> Result<Option<String>, &'static str> {
        let too_large = "Selection exceeds clipboard size limit";
        match &self.text_selection.selected {
            Some(Selection::Prompt(paint)) if self.prompt_capture_current(paint, frame) => {
                let Some((a, b)) = self.editor.selected() else {
                    return Ok(None);
                };
                if a == b {
                    return Ok(None);
                }
                if b - a > MAX_SELECTION_BYTES {
                    return Err(too_large);
                }
                // No underlying prompt text is available through an overpaint.
                if crate::shell::text_toast_rect(self, frame)
                    .is_some_and(|toast| !toast.intersection(paint.input).is_empty())
                {
                    return Ok(None);
                }
                Ok(Some(self.input[a..b].to_owned()))
            }
            Some(Selection::Text {
                paint,
                anchor,
                focus,
            }) if self.text_paint_current(paint, frame) => {
                if anchor == focus || self.text_occluded(paint, *anchor, *focus) {
                    return Ok(None);
                }
                let mut text = String::new();
                for (index, (row, a, b)) in text_intervals(paint, *anchor, *focus).enumerate() {
                    let line = paint.rows[row].1.plain_text();
                    if text.len().saturating_add(b - a + usize::from(index > 0))
                        > MAX_SELECTION_BYTES
                    {
                        return Err(too_large);
                    }
                    if index > 0 {
                        text.push('\n');
                    }
                    text.push_str(&line[a..b]);
                }
                Ok((!text.is_empty()).then_some(text))
            }
            _ => Ok(None),
        }
    }

    fn request_new_selection_copy(&mut self, frame: Rect) {
        self.pending_copy = None;
        if self.clipboard_mode == ClipboardMode::Disabled {
            return;
        }
        match self.new_selection_text(frame) {
            Ok(text) => self.pending_copy = text,
            Err(error) => self.push_transient_note(error, NoteVariant::Error),
        }
    }

    /// Runs before destination handlers, but never steals an uncaptured press
    /// from terminal focus, forms, modals or rail resizing.
    pub(super) fn continue_text_mouse(&mut self, event: MouseEvent, frame: Rect) -> bool {
        if event.kind == MouseEventKind::Down(MouseButton::Right)
            && self.clipboard_mode == ClipboardMode::Manual
            && self.text_selection.selected.is_some()
            && event.modifiers.is_empty()
        {
            self.request_new_selection_copy(frame);
            return true;
        }
        if matches!(event.kind, MouseEventKind::Down(_)) {
            if self.has_new_text_selection() {
                self.pending_copy = None;
            }
            self.text_selection.held = false;
            self.text_selection.transcript = false;
            self.text_selection.selected = None;
            self.text_selection.invalid.set(false);
            return false;
        }
        if !self.text_selection.held {
            return false;
        }
        if !matches!(event.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_)) {
            if event.kind != MouseEventKind::Moved {
                self.cancel_text_selection();
            }
            return false;
        }
        if self.text_selection.transcript
            && self.selection_gesture
            && self.transcript_controls_available()
        {
            let dragged = self
                .selection
                .as_ref()
                .is_some_and(|selection| selection.dragging);
            self.handle_transcript_selection(event, frame);
            let invalidated = self.selection.is_none();
            if matches!(event.kind, MouseEventKind::Up(_)) {
                self.text_selection.held = false;
                self.text_selection.transcript = false;
            }
            if matches!(event.kind, MouseEventKind::Drag(_)) || dragged || invalidated {
                if matches!(event.kind, MouseEventKind::Up(_)) {
                    self.reasoning_pointer_down = false;
                }
                self.tab_down = None;
                self.reasoning_down = None;
                self.exploration_down = None;
                self.message_down = None;
                self.reverted_down = None;
                self.child_notice_down = None;
                self.home_mcp_down = None;
                self.cancel_sidebar_press();
                return true;
            }
            return false; // the original no-drag message/header click still owns Up
        }
        let valid = event.modifiers.is_empty()
            && match &self.text_selection.selected {
                Some(Selection::Prompt(paint)) => self.prompt_capture_current(paint, frame),
                Some(Selection::Text { paint, .. }) => self.text_paint_current(paint, frame),
                None => false,
            };
        if !valid
            || matches!(event.kind, MouseEventKind::Drag(button) if button != MouseButton::Left)
        {
            self.cancel_text_selection();
        } else {
            if matches!(event.kind, MouseEventKind::Drag(_)) {
                self.text_selection.dragged = true;
                self.cancel_sidebar_press();
            }
            // A plain sidebar click still belongs to its captured disclosure/row.
            if matches!(event.kind, MouseEventKind::Up(_)) && !self.text_selection.dragged {
                let sidebar_click = matches!(&self.text_selection.selected, Some(Selection::Text { paint, .. }) if paint.source == TextSource::Sidebar && paint.rows.iter().any(|(rect, _)| rect.contains((event.column, event.row).into())));
                self.text_selection.held = false;
                if sidebar_click {
                    return false;
                }
            }
            if self.text_selection.dragged {
                match &self.text_selection.selected {
                    Some(Selection::Prompt(paint)) => {
                        if let Some(offset) = self.prompt_drag_point(paint, event) {
                            self.editor.move_to(offset, true);
                            self.slash_selected = 0;
                            self.mention_selected = 0;
                        }
                    }
                    Some(Selection::Text { paint, .. }) => {
                        let point = text_point(paint, event.column, event.row);
                        if let Some(point) = point
                            && let Some(Selection::Text { focus, .. }) =
                                &mut self.text_selection.selected
                        {
                            *focus = point;
                        }
                    }
                    None => {}
                }
                if matches!(event.kind, MouseEventKind::Up(_))
                    && self.clipboard_mode == ClipboardMode::Select
                {
                    self.request_new_selection_copy(frame);
                }
            }
        }
        if matches!(event.kind, MouseEventKind::Up(_)) {
            self.text_selection.held = false;
            self.reasoning_pointer_down = false;
            self.cancel_sidebar_press();
        }
        true
    }

    pub(super) fn start_readonly_selection(&mut self, event: MouseEvent, frame: Rect) -> bool {
        if event.kind != MouseEventKind::Down(MouseButton::Left)
            || !event.modifiers.is_empty()
            || !self.text_controls_available()
        {
            return false;
        }
        let paint = self
            .text_selection
            .painted
            .borrow()
            .iter()
            .flatten()
            .find(|paint| {
                paint
                    .rows
                    .iter()
                    .any(|(rect, _)| rect.contains((event.column, event.row).into()))
                    && self.text_paint_current(paint, frame)
            })
            .cloned();
        let Some(paint) = paint else { return false };
        let Some(anchor) = text_point(&paint, event.column, event.row) else {
            return false;
        };
        if crate::shell::text_toast_rect(self, frame)
            .is_some_and(|rect| rect.contains((event.column, event.row).into()))
        {
            return false;
        }
        let footer = paint.source == TextSource::Location;
        self.clear_transcript_selection();
        self.text_selection.selected = Some(Selection::Text {
            paint,
            anchor,
            focus: anchor,
        });
        self.text_selection.invalid.set(false);
        self.text_selection.held = true;
        self.text_selection.dragged = false;
        self.tab_down = None;
        self.reasoning_down = None;
        self.exploration_down = None;
        self.home_mcp_down = None;
        footer
    }
}
