//! Application-owned session terminals. No OS descriptors or signal capabilities.
use crate::domain::SessionId;
use serde::{Deserialize, Serialize};

pub const TERMINAL_INPUT_BYTES: usize = 8192;
pub const TERMINAL_MAX_ROWS: u16 = 120;
pub const TERMINAL_MAX_COLS: u16 = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}
impl TerminalSize {
    pub fn valid(self) -> bool {
        (1..=TERMINAL_MAX_ROWS).contains(&self.rows) && (1..=TERMINAL_MAX_COLS).contains(&self.cols)
    }
}
impl Default for TerminalSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

/// Immutable execution identity; controls must match every field, not just a PID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRef {
    pub id: String,
    pub session: SessionId,
    pub location: String,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalState {
    Running,
    Exited,
    Removed,
    Interrupted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalEntry {
    pub target: TerminalRef,
    pub shell: String,
    pub cwd: String,
    pub pid: i32,
    pub title: String,
    pub foreground: Option<String>,
    pub state: TerminalState,
    pub exit: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInventory {
    pub enabled: bool,
    pub entries: Vec<TerminalEntry>,
    pub selected: Option<TerminalRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalCell {
    pub text: String,
    pub wide_continuation: bool,
    pub fg: TerminalColor,
    pub bg: TerminalColor,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

/// A single atomic, emulator-authored screen and actual-output checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSnapshot {
    pub entry: TerminalEntry,
    pub size: TerminalSize,
    pub cells: Vec<TerminalCell>,
    pub cursor: (u16, u16),
    pub hide_cursor: bool,
    pub application_cursor: bool,
    pub bracketed_paste: bool,
    pub output_cursor: u64,
    pub revision: u64,
    pub ready: bool,
    pub control_strings_dropped: u64,
}

/// Actual byte replay, never escape output to the host terminal. A gap supplies
/// an atomic current screen instead of pretending discarded bytes were received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalReplay {
    pub from: u64,
    pub next: u64,
    pub bytes: Vec<u8>,
    pub reset: Option<TerminalSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalAction {
    List,
    Create {
        location: String,
        generation: u64,
        size: TerminalSize,
    },
    Select(Option<TerminalRef>),
    Input {
        target: TerminalRef,
        bytes: Vec<u8>,
    },
    Resize {
        target: TerminalRef,
        size: TerminalSize,
    },
    Snapshot(TerminalRef),
    Replay {
        target: TerminalRef,
        cursor: u64,
    },
    Remove(TerminalRef),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalReceipt {
    Inventory(TerminalInventory),
    Created(TerminalEntry),
    Snapshot(TerminalSnapshot),
    Replay(TerminalReplay),
    Applied,
}
