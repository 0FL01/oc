//! Byte offsets are always extended grapheme boundaries. The text stays in
//! `TuiState` so submission still snapshots the exact application draft.
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub const MAX_PASTE_CHIPS: usize = 1024;
const MAX_FILE_MENTIONS: usize = 1024;

pub struct PasteOutcome {
    pub inserted: usize,
    pub trimmed: usize,
}

pub fn chip_worthy(content: &str) -> Option<usize> {
    let content = content.trim();
    let lines = content.bytes().filter(|&b| b == b'\n').count() + 1;
    // The pinned original uses JavaScript string.length (UTF-16 code units).
    (lines >= 3 || content.encode_utf16().count() > 150).then_some(lines)
}

#[derive(Clone, Debug)]
struct Snapshot {
    text: String,
    cursor: usize,
    anchor: Option<usize>,
    chips: Vec<PasteChip>,
    mentions: Vec<(usize, usize)>,
}

// Positions refer only to the single, real draft string. No pasted content is
// copied into display metadata; losing a chip can never lose submitted text.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PasteChip {
    start: usize,
    end: usize,
    lines: usize,
}

#[derive(Default)]
pub struct Editor {
    pub cursor: usize,
    pub anchor: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    history: Option<(Vec<String>, usize, Snapshot)>,
    chips: Vec<PasteChip>,
    // Styled extmarks over the real draft, excluding the trailing separator.
    mentions: Vec<(usize, usize)>,
    // One accepted prompt in this editor instance; durable history is text-only.
    recent_mention: Option<(String, Vec<(usize, usize)>)>,
}

impl Editor {
    pub fn retained_bytes(&self) -> usize {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|s| {
                s.text.len()
                    + s.chips.len() * std::mem::size_of::<PasteChip>()
                    + s.mentions.len() * std::mem::size_of::<(usize, usize)>()
            })
            .sum::<usize>()
            + self.chips.len() * std::mem::size_of::<PasteChip>()
            + self.mentions.len() * std::mem::size_of::<(usize, usize)>()
            + self.recent_mention.as_ref().map_or(0, |(text, marks)| {
                text.len() + marks.len() * std::mem::size_of::<(usize, usize)>()
            })
            + self.history.as_ref().map_or(0, |(items, _, draft)| {
                draft.text.len()
                    + draft.chips.len() * std::mem::size_of::<PasteChip>()
                    + draft.mentions.len() * std::mem::size_of::<(usize, usize)>()
                    + items.iter().map(String::len).sum::<usize>()
            })
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn clear_submitted_draft(&mut self) {
        let recent = self.recent_mention.take();
        self.clear();
        self.recent_mention = recent;
    }

    pub fn forget_accepted_mentions(&mut self) {
        self.recent_mention = None;
    }

    fn snapshot(&self, text: &str) -> Snapshot {
        Snapshot {
            text: text.into(),
            cursor: self.cursor,
            anchor: self.anchor,
            chips: self.chips.clone(),
            mentions: self.mentions.clone(),
        }
    }

    fn save(&mut self, text: &str) {
        self.undo.push(self.snapshot(text));
        // Bound snapshots by both count and bytes, even for maximum-size pastes.
        while self.undo.len() > 32
            || self
                .undo
                .iter()
                .map(|s| {
                    s.text.len()
                        + s.chips.len() * std::mem::size_of::<PasteChip>()
                        + s.mentions.len() * std::mem::size_of::<(usize, usize)>()
                })
                .sum::<usize>()
                > 2 * 1024 * 1024
        {
            self.undo.remove(0);
        }
        self.redo.clear();
        // Recalled entries are editable copies; preserve the saved unfinished
        // draft and its chip metadata until Down returns to it.
        if self
            .history
            .as_ref()
            .is_some_and(|(items, index, _)| *index == items.len())
        {
            self.history = None;
        }
    }

    fn selection(&self) -> Option<(usize, usize)> {
        self.anchor
            .filter(|a| *a != self.cursor)
            .map(|a| (a.min(self.cursor), a.max(self.cursor)))
    }

    pub fn selected(&self) -> Option<(usize, usize)> {
        self.selection()
    }

    pub fn replace(&mut self, text: &mut String, inserted: &str, limit: usize) -> usize {
        self.insert(text, inserted, limit, false).inserted
    }

    pub fn paste(&mut self, text: &mut String, inserted: &str, limit: usize) -> PasteOutcome {
        self.insert(text, inserted, limit, true)
    }

    fn insert(
        &mut self,
        text: &mut String,
        inserted: &str,
        limit: usize,
        paste: bool,
    ) -> PasteOutcome {
        let (start, end) = self.selection().unwrap_or((self.cursor, self.cursor));
        let room = limit.saturating_sub(text.len() - (end - start));
        let mut kept = 0;
        for grapheme in inserted.graphemes(true) {
            if kept + grapheme.len() > room {
                break;
            }
            kept += grapheme.len();
        }
        // A rejected insertion must never silently erase a selection. In
        // particular, a wider grapheme can fail to fit even after the
        // selected bytes are accounted for in `room`.
        if kept == 0 {
            return PasteOutcome {
                inserted: 0,
                trimmed: 0,
            };
        }
        let clipped = &inserted[..kept];
        // The original creates a chip from normalized.trim(). Apply that rule
        // only to the portion actually fitting the bounded draft.
        let lines = paste.then(|| chip_worthy(clipped)).flatten();
        let content = if lines.is_some() {
            clipped.trim()
        } else {
            clipped
        };
        let trimmed = clipped.len() - content.len();
        let kept = content.len();
        self.save(text);
        self.adjust_chips(start, end, kept);
        self.adjust_mentions(start, end, kept);
        text.replace_range(start..end, content);
        // A cap on visual markers bounds metadata and rendering even for many
        // individually tiny multiline pastes. Text is inserted regardless.
        if let Some(lines) = lines.filter(|_| self.chips.len() < MAX_PASTE_CHIPS) {
            let at = self.chips.partition_point(|chip| chip.start < start);
            self.chips.insert(
                at,
                PasteChip {
                    start,
                    end: start + kept,
                    lines,
                },
            );
        }
        // A join can shift boundaries past the edit (notably regional-
        // indicator pairing). Never retain a range that splits a grapheme.
        self.revalidate_chips(text);
        self.revalidate_mentions(text);
        self.cursor = following_boundary(text, start + kept);
        self.anchor = None;
        PasteOutcome {
            inserted: kept,
            trimmed,
        }
    }

    fn revalidate_chips(&mut self, text: &str) {
        if self.chips.is_empty() {
            return;
        }
        // Sorted, nonoverlapping ranges allow one segmentation walk even if
        // RI pairing shifts boundaries across multiple display chips.
        let mut boundaries = text
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .peekable();
        self.chips.retain(|chip| {
            while boundaries.peek().is_some_and(|&at| at < chip.start) {
                boundaries.next();
            }
            let start_ok = boundaries.peek().is_some_and(|&at| at == chip.start);
            while boundaries.peek().is_some_and(|&at| at < chip.end) {
                boundaries.next();
            }
            start_ok && boundaries.peek().is_some_and(|&at| at == chip.end)
        });
    }

    fn adjust_chips(&mut self, start: usize, end: usize, inserted: usize) {
        self.chips
            .retain(|chip| chip.end <= start || chip.start >= end);
        for chip in &mut self.chips {
            if chip.start >= end {
                chip.start = chip.start - (end - start) + inserted;
                chip.end = chip.end - (end - start) + inserted;
            }
        }
    }

    fn adjust_mentions(&mut self, start: usize, end: usize, inserted: usize) {
        self.mentions.retain(|&(a, b)| b <= start || a >= end);
        for (a, b) in &mut self.mentions {
            // Inserting at the beginning shifts the whole extmark; inserting
            // at its end leaves it in place. Interior edits invalidate it.
            if *a >= end {
                *a = *a - (end - start) + inserted;
                *b = *b - (end - start) + inserted;
            } else if start == end && *a < start && start < *b {
                // A zero-width insertion still edits the marked text.
                *a = usize::MAX;
            }
        }
        self.mentions.retain(|&(a, _)| a != usize::MAX);
    }

    fn revalidate_mentions(&mut self, text: &str) {
        if self.mentions.is_empty() {
            return;
        }
        // Extmarks are sorted and nonoverlapping. Walk segmentation once,
        // without allocating a boundary for every grapheme in a large draft.
        let mut boundaries = text
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .peekable();
        self.mentions.retain(|&(start, end)| {
            while boundaries.peek().is_some_and(|&at| at < start) {
                boundaries.next();
            }
            let start_ok = boundaries.peek().is_some_and(|&at| at == start);
            while boundaries.peek().is_some_and(|&at| at < end) {
                boundaries.next();
            }
            start_ok
                && boundaries.peek().is_some_and(|&at| at == end)
                && text
                    .get(start..end)
                    .is_some_and(|part| part.starts_with('@'))
        });
    }

    /// The selected file is still ordinary submitted text; only its extmark
    /// receives `extmark.file` paint (`autocomplete.tsx:177-190`).
    pub fn mark_file_mention(&mut self, start: usize, end: usize, text: &str) {
        if self.mentions.len() < MAX_FILE_MENTIONS
            && text
                .get(start..end)
                .is_some_and(|part| part.starts_with('@'))
            && !self.mentions.iter().any(|&(a, b)| start < b && end > a)
        {
            self.mentions.push((start, end));
            self.mentions.sort_unstable_by_key(|&(start, _)| start);
            self.revalidate_mentions(text);
        }
    }

    /// Capture only the markers inside the text submitted by the owner.
    pub fn submitted_mentions(&self, draft: &str) -> Vec<(usize, usize)> {
        let trimmed = draft.trim();
        let start = trimmed.as_ptr() as usize - draft.as_ptr() as usize;
        let end = start + trimmed.len();
        self.mentions
            .iter()
            .filter(|&&(a, b)| a >= start && b <= end)
            .map(|&(a, b)| (a - start, b - start))
            .collect()
    }

    /// Retain only the latest accepted prompt for same-instance Up recall.
    pub fn accepted_mentions(&mut self, text: &str, mentions: Vec<(usize, usize)>) {
        self.recent_mention = (!mentions.is_empty()).then(|| (text.to_owned(), mentions));
    }

    pub fn delete(&mut self, text: &mut String, backward: bool, word: bool) -> bool {
        let (start, end) = if let Some(range) = self.selection() {
            range
        } else if backward {
            (
                if word {
                    word_left(text, self.cursor)
                } else {
                    previous(text, self.cursor)
                },
                self.cursor,
            )
        } else {
            (
                self.cursor,
                if word {
                    word_right(text, self.cursor)
                } else {
                    next(text, self.cursor)
                },
            )
        };
        let start = self.snap(start, false);
        let end = self.snap(end, true);
        if start == end {
            return false;
        }
        self.save(text);
        self.adjust_chips(start, end, 0);
        self.adjust_mentions(start, end, 0);
        text.replace_range(start..end, "");
        self.revalidate_chips(text);
        self.revalidate_mentions(text);
        self.cursor = following_boundary(text, start);
        self.anchor = None;
        true
    }

    pub fn move_to(&mut self, offset: usize, select: bool) {
        if select {
            if self.anchor.is_none() {
                self.anchor = Some(self.cursor);
            }
        } else {
            self.anchor = None;
        }
        self.cursor = self.snap(offset, offset >= self.cursor);
    }

    fn snap(&self, pos: usize, right: bool) -> usize {
        self.chips
            .iter()
            .find(|c| c.start < pos && pos < c.end)
            .map_or(pos, |c| if right { c.end } else { c.start })
    }

    pub fn horizontal(&mut self, text: &str, right: bool, word: bool, select: bool) {
        if !select && let Some((start, end)) = self.selection() {
            self.move_to(if right { end } else { start }, false);
            return;
        }
        let pos = if word {
            if right {
                word_right(text, self.cursor)
            } else {
                word_left(text, self.cursor)
            }
        } else if right {
            next(text, self.cursor)
        } else {
            previous(text, self.cursor)
        };
        let pos = if right {
            self.chips
                .iter()
                .find(|c| c.start == self.cursor)
                .map_or(pos, |c| c.end)
        } else {
            self.chips
                .iter()
                .find(|c| c.end == self.cursor)
                .map_or(pos, |c| c.start)
        };
        self.move_to(pos, select);
    }

    pub fn line_edge(&mut self, text: &str, end: bool, select: bool) {
        if !self.chips.is_empty() {
            let (visible, map) = self.project(text);
            let pos = raw_to_visual(self.cursor, &map);
            let target = if end {
                visible[pos..].find('\n').map_or(visible.len(), |i| pos + i)
            } else {
                visible[..pos].rfind('\n').map_or(0, |i| i + 1)
            };
            self.move_to(visual_to_raw(target, &map, end), select);
            return;
        }
        let pos = if end {
            text[self.cursor..]
                .find('\n')
                .map_or(text.len(), |i| self.cursor + i)
        } else {
            text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
        };
        self.move_to(pos, select);
    }

    pub fn vertical(&mut self, text: &str, down: bool, select: bool) -> bool {
        if !self.chips.is_empty() {
            let (visible, map) = self.project(text);
            let cursor = raw_to_visual(self.cursor, &map);
            let Some(pos) = vertical_target(&visible, cursor, down) else {
                return false;
            };
            self.move_to(visual_to_raw(pos, &map, down), select);
            return true;
        }
        let Some(pos) = vertical_target(text, self.cursor, down) else {
            return false;
        };
        self.move_to(pos, select);
        true
    }

    fn project(&self, text: &str) -> (String, Vec<(usize, usize, usize, usize)>) {
        let mut visible = String::new();
        let mut map = Vec::with_capacity(self.chips.len());
        let mut last = 0;
        for chip in &self.chips {
            visible.push_str(&text[last..chip.start]);
            let start = visible.len();
            visible.push_str(&format!("[Pasted ~{} lines] ", chip.lines));
            map.push((chip.start, chip.end, start, visible.len()));
            last = chip.end;
        }
        visible.push_str(&text[last..]);
        (visible, map)
    }

    pub fn layout(&self, text: &str, width: usize) -> (Vec<PromptRow>, (usize, usize)) {
        if self.chips.is_empty() {
            return layout_with_mentions(text, self.cursor, self.selected(), width, &self.mentions);
        }
        let (visible, map) = self.project(text);
        let mentions: Vec<_> = self
            .mentions
            .iter()
            .filter(|&&(a, b)| {
                // An extmark inside a collapsed paste chip is not visible.
                !map.iter().any(|&(start, end, _, _)| a < end && b > start)
            })
            .map(|&(a, b)| (raw_to_visual(a, &map), raw_to_visual(b, &map)))
            .collect();
        layout_with_mentions(
            &visible,
            raw_to_visual(self.cursor, &map),
            self.selected()
                .map(|(a, b)| (raw_to_visual(a, &map), raw_to_visual(b, &map))),
            width,
            &mentions,
        )
    }

    pub fn chip_count(&self) -> usize {
        self.chips.len()
    }

    pub fn undo(&mut self, text: &mut String, redo: bool) -> bool {
        let snapshot = if redo {
            self.redo.pop()
        } else {
            self.undo.pop()
        };
        let Some(snapshot) = snapshot else {
            return false;
        };
        let current = self.snapshot(text);
        if redo {
            self.undo.push(current);
        } else {
            self.redo.push(current);
        }
        *text = snapshot.text;
        self.cursor = snapshot.cursor;
        self.anchor = snapshot.anchor;
        self.chips = snapshot.chips;
        self.mentions = snapshot.mentions;
        if let Some((items, index, draft)) = self.history.as_mut() {
            if text == &draft.text && self.chips == draft.chips {
                *index = items.len();
            } else if let Some(at) = items.iter().position(|item| item == text) {
                *index = at;
            } else if *index == items.len() {
                self.history = None;
            }
        }
        true
    }

    pub fn recall(&mut self, text: &mut String, previous: bool, entries: Vec<String>) -> bool {
        if entries.is_empty() {
            return false;
        }
        if self.history.is_none() {
            if !previous {
                return false;
            }
            let len = entries.len();
            // A first history visit is one undoable navigation from the draft.
            // Later edits of a recalled copy are separately undoable.
            self.save(text);
            self.history = Some((entries, len, self.snapshot(text)));
        }
        let Some((items, index, draft)) = self.history.as_mut() else {
            return false;
        };
        if *index < items.len() && *text != items[*index] {
            if previous {
                // Pinned original refuses to browse older entries after a
                // recalled entry has been edited. Down recovers the native
                // saved unfinished draft instead of discarding it.
                return false;
            }
            *index = items.len();
        } else if previous {
            *index = index.saturating_sub(1);
        } else {
            *index = (*index + 1).min(items.len());
        }
        *text = if *index == items.len() {
            draft.text.clone()
        } else {
            items[*index].clone()
        };
        self.chips = if *index == items.len() {
            draft.chips.clone()
        } else {
            Vec::new()
        };
        self.mentions = if *index == items.len() {
            draft.mentions.clone()
        } else if *index == items.len() - 1 {
            self.recent_mention
                .as_ref()
                .filter(|(recent, _)| recent == text)
                .map_or_else(Vec::new, |(_, marks)| marks.clone())
        } else {
            Vec::new()
        };
        self.cursor = if *index == items.len() {
            draft.cursor
        } else {
            text.len()
        };
        self.anchor = if *index == items.len() {
            draft.anchor
        } else {
            None
        };
        true
    }
}

// Offset maps are ephemeral layout projections; raw text alone is submitted.
fn raw_to_visual(offset: usize, map: &[(usize, usize, usize, usize)]) -> usize {
    let mut raw = 0;
    let mut visual = 0;
    for &(start, end, _, ve) in map {
        if offset <= start {
            return visual + offset - raw;
        }
        if offset <= end {
            return ve;
        }
        raw = end;
        visual = ve;
    }
    visual + offset - raw
}

fn visual_to_raw(offset: usize, map: &[(usize, usize, usize, usize)], right: bool) -> usize {
    let mut raw = 0;
    let mut visual = 0;
    for &(start, end, vs, ve) in map {
        if offset <= vs {
            return raw + offset - visual;
        }
        if offset <= ve {
            return if offset - vs > ve - offset || (offset - vs == ve - offset && right) {
                end
            } else {
                start
            };
        }
        raw = end;
        visual = ve;
    }
    raw + offset - visual
}

fn vertical_target(text: &str, cursor: usize, down: bool) -> Option<usize> {
    let start = text[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let end = text[cursor..].find('\n').map_or(text.len(), |i| cursor + i);
    let column = UnicodeWidthStr::width(&text[start..cursor]);
    let (from, to) = if down {
        if end == text.len() {
            return None;
        }
        let from = end + 1;
        (
            from,
            text[from..].find('\n').map_or(text.len(), |i| from + i),
        )
    } else {
        if start == 0 {
            return None;
        }
        let to = start - 1;
        (text[..to].rfind('\n').map_or(0, |i| i + 1), to)
    };
    let mut pos = from;
    let mut cells = 0;
    for (i, g) in text[from..to].grapheme_indices(true) {
        let width = UnicodeWidthStr::width(g);
        if cells + width > column {
            break;
        }
        cells += width;
        pos = from + i + g.len();
    }
    Some(pos)
}

fn previous(text: &str, pos: usize) -> usize {
    text[..pos]
        .grapheme_indices(true)
        .next_back()
        .map_or(0, |(i, _)| i)
}
fn following_boundary(text: &str, pos: usize) -> usize {
    text.grapheme_indices(true)
        .find(|(offset, _)| *offset >= pos)
        .map_or(text.len(), |(offset, _)| offset)
}
fn next(text: &str, pos: usize) -> usize {
    text[pos..]
        .graphemes(true)
        .next()
        .map_or(text.len(), |g| pos + g.len())
}
fn word_left(text: &str, pos: usize) -> usize {
    let mut at = pos;
    while at > 0 {
        let prev = previous(text, at);
        if !text[prev..at].chars().all(char::is_whitespace) {
            break;
        }
        at = prev;
    }
    while at > 0 {
        let prev = previous(text, at);
        if !text[prev..at].chars().any(char::is_alphanumeric) {
            break;
        }
        at = prev;
    }
    at
}
fn word_right(text: &str, pos: usize) -> usize {
    let mut at = pos;
    while at < text.len() {
        let end = next(text, at);
        if text[at..end].chars().any(char::is_alphanumeric) {
            break;
        }
        at = end;
    }
    while at < text.len() {
        let end = next(text, at);
        if !text[at..end].chars().any(char::is_alphanumeric) {
            break;
        }
        at = end;
    }
    at
}

/// Visible prompt rows and the caret's row/column use the same wrap algorithm.
/// Byte spans are retained for styling a selected grapheme without splitting it.
pub struct PromptRow {
    pub text: String,
    pub spans: Vec<(String, bool, bool)>,
}
#[cfg(test)]
pub fn layout(
    text: &str,
    cursor: usize,
    selection: Option<(usize, usize)>,
    width: usize,
) -> (Vec<PromptRow>, (usize, usize)) {
    layout_with_mentions(text, cursor, selection, width, &[])
}

fn layout_with_mentions(
    text: &str,
    cursor: usize,
    selection: Option<(usize, usize)>,
    width: usize,
    mentions: &[(usize, usize)],
) -> (Vec<PromptRow>, (usize, usize)) {
    let width = width.max(1);
    let mut rows = vec![PromptRow {
        text: String::new(),
        spans: Vec::new(),
    }];
    let mut column = 0;
    let mut caret = (0, 0);
    for (offset, grapheme) in text.grapheme_indices(true) {
        if offset == cursor {
            caret = (rows.len() - 1, column);
        }
        if grapheme == "\n" {
            rows.push(PromptRow {
                text: String::new(),
                spans: Vec::new(),
            });
            column = 0;
            continue;
        }
        let cells = UnicodeWidthStr::width(grapheme);
        if column > 0 && column + cells > width {
            rows.push(PromptRow {
                text: String::new(),
                spans: Vec::new(),
            });
            column = 0;
            if offset == cursor {
                caret = (rows.len() - 1, 0);
            }
        }
        let selected = selection.is_some_and(|(start, end)| offset >= start && offset < end);
        let mentioned = mentions
            .get(mentions.partition_point(|&(_, end)| end <= offset))
            .is_some_and(|&(start, _)| start <= offset);
        let row = rows.last_mut().expect("one row");
        row.text.push_str(grapheme);
        if let Some((last, was_selected, was_mentioned)) = row.spans.last_mut()
            && *was_selected == selected
            && *was_mentioned == mentioned
        {
            last.push_str(grapheme);
        } else {
            row.spans.push((grapheme.to_string(), selected, mentioned));
        }
        column += cells;
    }
    if cursor == text.len() {
        if column >= width && !text.is_empty() {
            rows.push(PromptRow {
                text: String::new(),
                spans: Vec::new(),
            });
            caret = (rows.len() - 1, 0);
        } else {
            caret = (rows.len() - 1, column);
        }
    }
    (rows, caret)
}

#[cfg(test)]
mod tests {
    use super::{Editor, MAX_FILE_MENTIONS, layout};

    #[test]
    fn large_draft_mention_validation_does_not_retain_grapheme_boundaries() {
        let mut editor = Editor::default();
        let mut text = format!("@a {}@b", "x".repeat(1024 * 1024 - 16));
        let last = text.len() - 2;
        editor.mark_file_mention(0, 2, &text);
        editor.mark_file_mention(last, text.len(), &text);
        editor.cursor = 2;
        let limit = text.len() + 1;
        assert_eq!(editor.replace(&mut text, "!", limit), 1);
        assert_eq!(editor.mentions, vec![(0, 2), (last + 1, text.len())]);
        assert!(editor.retained_bytes() <= 2 * text.len() + 4 * 16);
        assert!(editor.delete(&mut text, true, false));
        assert_eq!(editor.mentions, vec![(0, 2), (last, text.len())]);

        let mut editor = Editor::default();
        let mut text = "@🇦 🇧".to_string();
        editor.mark_file_mention(0, "@🇦".len(), &text);
        editor.move_to("@🇦".len(), false);
        assert!(editor.delete(&mut text, false, false));
        assert_eq!(text, "@🇦🇧");
        assert!(editor.mentions.is_empty(), "RI join splits the marked end");

        let mut editor = Editor::default();
        let text = "@".repeat(MAX_FILE_MENTIONS + 1);
        for i in 0..text.len() {
            editor.mark_file_mention(i, i + 1, &text);
        }
        assert_eq!(editor.mentions.len(), MAX_FILE_MENTIONS);
        assert!(editor.retained_bytes() <= MAX_FILE_MENTIONS * 16);
    }

    #[test]
    fn accepted_mention_recall_is_latest_only_and_preserves_edited_draft() {
        let mut editor = Editor::default();
        let mut text = "  @file.rs  ".to_string();
        editor.mark_file_mention(2, 10, &text);
        let marks = editor.submitted_mentions(&text);
        assert_eq!(marks, vec![(0, 8)]);
        editor.accepted_mentions("@file.rs", marks);
        text.clear();
        editor.clear_submitted_draft();
        editor.replace(&mut text, "@draft", 100);
        editor.mark_file_mention(0, text.len(), &text);
        assert!(editor.recall(&mut text, true, vec!["older".into(), "@file.rs".into()]));
        assert_eq!(editor.mentions, vec![(0, 8)]);
        assert!(editor.recall(&mut text, true, vec!["older".into(), "@file.rs".into()]));
        assert!(editor.mentions.is_empty());
        assert!(editor.recall(&mut text, false, vec!["older".into(), "@file.rs".into()]));
        assert_eq!(editor.mentions, vec![(0, 8)]);
        assert!(editor.recall(&mut text, false, vec!["older".into(), "@file.rs".into()]));
        assert_eq!(text, "@draft");
        assert_eq!(editor.mentions, vec![(0, 6)]);
        editor.accepted_mentions("unmarked", Vec::new());
        assert!(editor.recall(&mut text, true, vec!["@file.rs".into(), "unmarked".into()]));
        assert!(editor.recall(&mut text, true, vec!["@file.rs".into(), "unmarked".into()]));
        assert!(editor.mentions.is_empty());
    }

    #[test]
    fn file_extmark_tracks_draft_edits_undo_redo_and_chip_projection() {
        let mut editor = Editor::default();
        let mut text = "@file.rs ".to_string();
        editor.cursor = text.len();
        editor.mark_file_mention(0, "@file.rs".len(), &text);
        assert_eq!(
            editor.layout(&text, 5).0[0].spans[0],
            ("@file".into(), false, true)
        );

        editor.move_to(0, false);
        editor.replace(&mut text, "go ", 200);
        assert_eq!(text, "go @file.rs ");
        assert_eq!(
            editor.layout(&text, 80).0[0].spans[1],
            ("@file.rs".into(), false, true)
        );
        editor.move_to("go @fi".len(), false);
        editor.replace(&mut text, "x", 200);
        assert!(editor.layout(&text, 80).0[0].spans.iter().all(|s| !s.2));
        assert!(editor.undo(&mut text, false));
        assert!(editor.layout(&text, 80).0[0].spans[1].2);
        assert!(editor.undo(&mut text, false));
        assert!(editor.layout(&text, 80).0[0].spans[0].2);
        assert!(editor.undo(&mut text, true));
        assert!(editor.layout(&text, 80).0[0].spans[1].2);

        editor.move_to(text.len(), false);
        editor.paste(&mut text, "a\nb\nc", 200);
        assert!(editor.layout(&text, 80).0[0].spans[1].2);
        editor.move_to(0, false);
        editor.paste(&mut text, "x\ny\nz", 200);
        let rows = editor.layout(&text, 80).0;
        assert!(rows[0].spans.iter().any(|(_, _, mention)| *mention));
        assert_eq!(
            rows[0].text,
            "[Pasted ~3 lines] go @file.rs [Pasted ~3 lines] "
        );
    }

    #[test]
    fn v05_grapheme_selection_undo_and_width() {
        let mut editor = Editor::default();
        let mut text = "е\u{301}🧑‍💻界\nда".to_string();
        editor.cursor = text.len();
        editor.line_edge(&text, false, false);
        assert_eq!(editor.cursor, "е\u{301}🧑‍💻界\n".len());
        editor.horizontal(&text, false, false, false);
        editor.horizontal(&text, false, false, false);
        editor.horizontal(&text, false, false, true);
        assert_eq!(
            editor.selected(),
            Some(("е\u{301}".len(), "е\u{301}🧑‍💻".len()))
        );
        assert!(editor.delete(&mut text, true, false));
        assert_eq!(text, "е\u{301}界\nда");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, "е\u{301}🧑‍💻界\nда");
        editor.cursor = "е\u{301}🧑‍💻界".len();
        let (rows, caret) = layout(&text, editor.cursor, None, 4);
        assert_eq!(
            rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            vec!["е\u{301}🧑‍💻", "界", "да"]
        );
        assert_eq!(caret, (1, 2));
    }

    #[test]
    fn v05_paste_is_one_undo_and_does_not_split_grapheme_at_limit() {
        let mut editor = Editor::default();
        let mut text = "a".to_string();
        editor.cursor = 1;
        assert_eq!(editor.replace(&mut text, "🧑‍💻b", 6), 0);
        assert_eq!(text, "a");
        let (rows, caret) = layout("abcd", 4, None, 4);
        assert_eq!(rows.len(), 2, "full-width line needs a caret row");
        assert_eq!(caret, (1, 0));
        assert_eq!(editor.replace(&mut text, "🧑‍💻b", 64), "🧑‍💻b".len());
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, "a");
    }

    #[test]
    fn v05_chip_is_atomic_and_undo_restores_real_text_and_display() {
        let mut editor = Editor::default();
        let mut text = "до".to_owned();
        editor.cursor = text.len();
        let paste = "е\u{301}🧑‍💻\nвторая\nтретья";
        assert_eq!(editor.paste(&mut text, paste, 1024).inserted, paste.len());
        assert_eq!(text, format!("до{paste}"));
        let (rows, caret) = editor.layout(&text, 80);
        assert_eq!(rows[0].text, "до[Pasted ~3 lines] ");
        assert_eq!(
            caret,
            (
                0,
                unicode_width::UnicodeWidthStr::width("до[Pasted ~3 lines] ")
            )
        );
        editor.horizontal(&text, false, false, true);
        assert_eq!(editor.selected(), Some(("до".len(), text.len())));
        editor.move_to(text.len(), false);
        assert!(editor.delete(&mut text, true, false));
        assert_eq!(text, "до");
        assert_eq!(editor.layout(&text, 80).0[0].text, "до");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, format!("до{paste}"));
        assert_eq!(editor.layout(&text, 80).0[0].text, "до[Pasted ~3 lines] ");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, "до");
        assert!(editor.undo(&mut text, true));
        assert_eq!(text, format!("до{paste}"));
        editor.horizontal(&text, false, false, false);
        assert_eq!(editor.cursor, "до".len());
        assert_eq!(editor.replace(&mut text, "X", 1024), 1);
        assert_eq!(text, format!("доX{paste}"));
        assert_eq!(editor.layout(&text, 80).0[0].text, "доX[Pasted ~3 lines] ");
        editor.move_to(text.len(), false);
        editor.horizontal(&text, false, false, true);
        let replacement = "z".repeat(151);
        assert_eq!(editor.paste(&mut text, &replacement, 1024).inserted, 151);
        assert_eq!(text, format!("доX{replacement}"));
        assert_eq!(editor.layout(&text, 80).0[0].text, "доX[Pasted ~1 lines] ");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, format!("доX{paste}"));
        assert_eq!(editor.layout(&text, 80).0[0].text, "доX[Pasted ~3 lines] ");

        let mut editor = Editor::default();
        let mut text = "top\n".to_owned();
        editor.cursor = text.len();
        editor.paste(&mut text, paste, 1024);
        assert!(editor.vertical(&text, false, false));
        assert_eq!(
            editor.cursor,
            "top".len(),
            "hidden lines are not caret rows"
        );
        editor.line_edge(&text, true, false);
        assert_eq!(editor.cursor, "top".len());
        assert!(editor.vertical(&text, true, false));
        assert_eq!(editor.cursor, "top\n".len(), "caret snaps to chip start");
    }

    #[test]
    fn v05_chip_expands_if_adjacent_edit_joins_graphemes_and_history_restores_it() {
        let mut editor = Editor::default();
        let mut text = String::new();
        let pasted = "a\nb\nc";
        editor.paste(&mut text, pasted, 1024);
        assert_eq!(editor.chip_count(), 1);
        editor.recall(&mut text, true, vec!["old".into()]);
        assert_eq!(text, "old");
        assert_eq!(editor.chip_count(), 0);
        editor.recall(&mut text, false, vec!["old".into()]);
        assert_eq!(text, pasted);
        assert_eq!(editor.layout(&text, 80).0[0].text, "[Pasted ~3 lines] ");
        editor.replace(&mut text, "\u{301}", 1024);
        assert_eq!(text, "a\nb\nc\u{301}");
        assert_eq!(editor.chip_count(), 0, "joined chip edge must expand");
        assert_eq!(editor.layout(&text, 80).0.len(), 3);
        editor.undo(&mut text, false);
        assert_eq!(text, pasted);
        assert_eq!(editor.chip_count(), 1);

        editor.clear();
        text = "e".into();
        editor.cursor = 1;
        editor.paste(&mut text, "\u{301}\nb\nc", 1024);
        assert_eq!(text, "e\u{301}\nb\nc");
        assert_eq!(
            editor.chip_count(),
            0,
            "chip cannot split preceding grapheme"
        );
    }

    #[test]
    fn v05_chip_threshold_matches_original_utf16_length() {
        let mut editor = Editor::default();
        let mut text = String::new();
        editor.paste(&mut text, &"🦊".repeat(75), 4096);
        assert_eq!(editor.chip_count(), 0, "150 UTF-16 units remain expanded");
        editor.clear();
        text.clear();
        editor.paste(&mut text, &"🦊".repeat(76), 4096);
        assert_eq!(editor.chip_count(), 1);
        assert_eq!(editor.layout(&text, 80).0[0].text, "[Pasted ~1 lines] ");
        assert_eq!(text, "🦊".repeat(76));
    }

    #[test]
    fn v05_review_deleting_between_regional_indicators_expands_chip() {
        let mut editor = Editor::default();
        let mut text = "🇦 ".to_string();
        editor.cursor = text.len();
        editor.paste(&mut text, "🇧\nx\ny", 1024);
        assert_eq!(editor.chip_count(), 1);
        editor.move_to("🇦".len(), false);
        assert!(editor.delete(&mut text, false, false));
        assert_eq!(text, "🇦🇧\nx\ny");
        assert_eq!(editor.chip_count(), 0, "chip must not split a joined flag");
        assert_eq!(editor.layout(&text, 80).0[0].text, "🇦🇧");
        assert_eq!(
            editor.cursor,
            "🇦🇧".len(),
            "caret must remain a grapheme boundary"
        );
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, "🇦 🇧\nx\ny");
        assert_eq!(editor.chip_count(), 1);
    }

    #[test]
    fn v05_review_regional_indicator_pairing_revalidates_distant_chips() {
        let mut editor = Editor::default();
        let mut text = "🇦 ".to_string();
        editor.cursor = text.len();
        editor.paste(&mut text, &"🇧".repeat(76), 4096);
        editor.paste(&mut text, &"🇨".repeat(76), 4096);
        assert_eq!(editor.chip_count(), 2);
        editor.move_to("🇦".len(), false);
        assert!(editor.delete(&mut text, false, false));
        assert_eq!(text, format!("🇦{}{}", "🇧".repeat(76), "🇨".repeat(76)));
        assert_eq!(
            editor.chip_count(),
            0,
            "RI pairing shifted beyond deleted byte"
        );
    }

    #[test]
    fn v05_review_history_edit_undo_down_restores_saved_draft_chip() {
        let mut editor = Editor::default();
        let mut text = String::new();
        editor.paste(&mut text, "draft\nline\nchip", 1024);
        let original = text.clone();
        assert!(editor.recall(&mut text, true, vec!["stored prompt".into()]));
        assert_eq!(text, "stored prompt");
        assert_eq!(editor.replace(&mut text, " edited", 1024), " edited".len());
        assert_eq!(text, "stored prompt edited");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, "stored prompt");
        assert!(editor.undo(&mut text, false));
        assert_eq!(text, original, "second Undo reaches saved draft");
        assert_eq!(editor.chip_count(), 1);
        assert!(editor.undo(&mut text, true));
        assert_eq!(text, "stored prompt", "redo restores recalled copy");
        assert!(editor.recall(&mut text, false, vec!["stored prompt".into()]));
        assert_eq!(text, original);
        assert_eq!(editor.layout(&text, 80).0[0].text, "[Pasted ~3 lines] ");
        assert!(editor.recall(&mut text, true, vec!["stored prompt".into()]));
        assert_eq!(editor.replace(&mut text, "!", 1024), 1);
        assert!(editor.recall(&mut text, false, vec!["stored prompt".into()]));
        assert_eq!(
            text, original,
            "Down also recovers draft after recalled edit"
        );
        assert_eq!(editor.chip_count(), 1);
    }
}
