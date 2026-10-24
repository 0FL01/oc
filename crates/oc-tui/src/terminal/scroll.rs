//! Publishing frames that move rows as terminal scrolls.
//!
//! Scrolling the transcript shifts nearly every visible row, and a plain cell
//! diff repaints all of them: tens of KiB of styled cells per frame over a
//! wide diff view, which the terminal must parse and paint at display rate.
//! `Screen` tracks the cells the terminal shows. When the next frame matches
//! a band of them shifted vertically, the band is scrolled in the terminal
//! (DECSTBM + SU/SD) and only the cells that still differ are drawn. That
//! residual diff is exact, so a poor plan costs bytes, never correctness.

use ratatui::{
    buffer::{Buffer, Cell},
    layout::{Rect, Size},
};
use std::hash::{Hash, Hasher};

/// Columns per compared chunk: a side panel that does not scroll spoils only
/// its own chunks, not the whole row.
const CHUNK: usize = 16;
/// Fewer changed rows than this are published as they are.
pub(super) const MIN_ROWS: usize = 3;
/// Chunks a scroll must save to pay for its escape sequences.
const MIN_SAVED: isize = 2;

/// The terminal's cells (Ratatui's previous buffer) with per-chunk hashes.
#[derive(Clone)]
pub(super) struct Screen {
    buffer: Buffer,
    hashes: Vec<u64>,
    chunks: usize,
}

/// Distinct rows of `changes` (a buffer diff, so in row order).
pub(super) fn touched_rows(changes: &[(u16, u16, &Cell)]) -> Vec<usize> {
    let mut rows: Vec<_> = changes.iter().map(|&(_, y, _)| usize::from(y)).collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// Scroll rows `top..=bottom` by `rows`: positive moves content up (SU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Scroll {
    top: u16,
    bottom: u16,
    rows: i32,
}

impl Screen {
    /// A cleared terminal of `size`.
    pub(super) fn blank(size: Size) -> Self {
        let buffer = Buffer::empty(Rect::new(0, 0, size.width, size.height));
        let chunks = usize::from(size.width).div_ceil(CHUNK);
        let mut screen = Self {
            hashes: vec![0; chunks * usize::from(size.height)],
            buffer,
            chunks,
        };
        screen.rehash(0..usize::from(size.height));
        screen
    }

    pub(super) fn size(&self) -> Size {
        Size::new(self.buffer.area.width, self.buffer.area.height)
    }

    pub(super) fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub(super) fn covers(&self, changes: &[(u16, u16, &Cell)]) -> bool {
        let Size { width, height } = self.size();
        changes.iter().all(|&(x, y, _)| x < width && y < height)
    }

    /// Apply covered `changes`, whose distinct rows are `rows`.
    pub(super) fn apply(&mut self, changes: &[(u16, u16, &Cell)], rows: &[usize]) {
        for &(x, y, cell) in changes {
            self.buffer[(x, y)] = cell.clone();
        }
        self.rehash(rows.iter().copied());
    }

    fn rehash(&mut self, rows: impl Iterator<Item = usize>) {
        let width = usize::from(self.buffer.area.width);
        for row in rows {
            let cells = &self.buffer.content[row * width..(row + 1) * width];
            for (chunk, cells) in cells.chunks(CHUNK).enumerate() {
                let mut hasher = Mix::default();
                cells.hash(&mut hasher);
                self.hashes[row * self.chunks + chunk] = hasher.finish();
            }
        }
    }

    fn row(&self, row: usize) -> &[u64] {
        &self.hashes[row * self.chunks..(row + 1) * self.chunks]
    }

    /// The vertical band scroll that leaves the fewest chunks to repaint for
    /// `next`, if it saves enough.
    pub(super) fn plan(&self, next: &Screen) -> Option<Scroll> {
        let height = usize::from(self.buffer.area.height);
        if next.size() != self.size() || height < 2 {
            return None;
        }
        let dirty = |a: &[u64], b: &[u64]| a.iter().zip(b).filter(|(a, b)| a != b).count() as isize;
        let blank = Screen::blank(Size::new(self.buffer.area.width, 1));
        let unshifted: Vec<isize> = (0..height)
            .map(|row| dirty(next.row(row), self.row(row)))
            .collect();
        let mut best: Option<(isize, Scroll)> = None;
        for distance in 1..height {
            for up in [true, false] {
                let (low, high) = if up {
                    (0, height - distance)
                } else {
                    (distance, height)
                };
                // Best contiguous run of rows that gain from taking the row
                // `distance` away (Kadane over per-row savings).
                let (mut run, mut start, mut found) = (0, low, None::<(isize, usize, usize)>);
                for (row, kept) in unshifted.iter().enumerate().take(high).skip(low) {
                    let source = if up { row + distance } else { row - distance };
                    if run <= 0 {
                        run = 0;
                        start = row;
                    }
                    run += kept - dirty(next.row(row), self.row(source));
                    if found.is_none_or(|(saved, _, _)| run > saved) {
                        found = Some((run, start, row));
                    }
                }
                let Some((saved, first, last)) = found else {
                    continue;
                };
                // Rows the scroll vacates are blank before their repaint.
                let vacated = if up {
                    last + 1..last + 1 + distance
                } else {
                    first - distance..first
                };
                let saved = saved
                    + vacated
                        .map(|row| unshifted[row] - dirty(next.row(row), blank.row(0)))
                        .sum::<isize>();
                let (top, bottom) = if up {
                    (first, last + distance)
                } else {
                    (first - distance, last)
                };
                // Terminals ignore DECSTBM unless the band spans two rows.
                if bottom > top && saved >= MIN_SAVED && best.is_none_or(|(best, _)| saved > best) {
                    best = Some((
                        saved,
                        Scroll {
                            top: top as u16,
                            bottom: bottom as u16,
                            rows: if up {
                                distance as i32
                            } else {
                                -(distance as i32)
                            },
                        },
                    ));
                }
            }
        }
        best.map(|(_, scroll)| scroll)
    }

    /// The cells after the terminal performs `scroll`.
    pub(super) fn scrolled(mut self, scroll: Scroll) -> Buffer {
        let width = usize::from(self.buffer.area.width);
        let rows = scroll.rows.unsigned_abs() as usize;
        let band = &mut self.buffer.content
            [usize::from(scroll.top) * width..(usize::from(scroll.bottom) + 1) * width];
        let vacated = if scroll.rows > 0 {
            band.rotate_left(rows * width);
            band.len() - rows * width..band.len()
        } else {
            band.rotate_right(rows * width);
            0..rows * width
        };
        band[vacated].fill(Cell::default());
        self.buffer
    }
}

impl Scroll {
    /// Reset attributes so vacated rows are default blanks, scroll inside the
    /// band's margins, then restore full-screen margins.
    pub(super) fn escape(self) -> String {
        format!(
            "\x1b[0m\x1b[{};{}r\x1b[{}{}\x1b[r",
            self.top + 1,
            self.bottom + 1,
            self.rows.unsigned_abs(),
            if self.rows > 0 { 'S' } else { 'T' }
        )
    }
}

/// Small multiplicative hash: a collision only costs repainted bytes.
#[derive(Default)]
struct Mix(u64);

impl Mix {
    fn add(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for Mix {
    fn write(&mut self, bytes: &[u8]) {
        for word in bytes.chunks(8) {
            let mut padded = [0; 8];
            padded[..word.len()].copy_from_slice(word);
            self.add(u64::from_le_bytes(padded));
        }
    }
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }
    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
