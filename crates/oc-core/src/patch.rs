//! Bounded, public metadata of confirmed filesystem effects, never restore images.
use serde::{Deserialize, Serialize};

pub const EFFECT_FILES_CAP: usize = 8;
pub const EFFECT_HUNKS_CAP: usize = 8;
pub const EFFECT_LINES_CAP: usize = 120;
pub const EFFECT_LINE_BYTES_CAP: usize = 1024;
/// Maximum serialized JSON bytes for one settled tool result (including DTO overhead).
pub const EFFECT_PREVIEW_BYTES_CAP: usize = 64 * 1024;
/// Maximum indexed lines in either image; larger images use streaming replacement.
pub const EFFECT_INDEX_LINES_CAP: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffAlgorithm {
    /// Shortest byte-line edit script, with deletion-preferred Myers ties.
    Minimal,
    /// Monotone unique byte-line anchors; counts describe the exact edit script,
    /// not necessarily the shortest edit script for repeated lines.
    UniqueAnchors,
    /// Streaming gap replacement after trimming identical prefix/suffix lines.
    StreamingReplacement,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchEffects {
    pub files: Vec<FileEffect>,
    pub total_files: usize,
    pub additions: usize,
    pub deletions: usize,
    /// Files, hunks, lines or text omitted by the published caps.
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchOperation {
    Create,
    Update,
    Delete,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEffect {
    pub algorithm: DiffAlgorithm,
    pub operation: PatchOperation,
    pub path: String,
    /// Actual committed destination, not the requested destination.
    pub destination: Option<String>,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<PatchHunk>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    /// 1-based first line; an empty range denotes the next insertion position.
    pub start: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchHunk {
    pub old: LineRange,
    pub new: LineRange,
    pub lines: Vec<PatchLine>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineEnding {
    Lf,
    CrLf,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchLine {
    /// True only for a binary delete whose bytes required replacement characters.
    pub lossy: bool,
    pub kind: PatchLineKind,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    /// UTF-8 content without its line terminator, capped at a character boundary.
    pub text: String,
    pub ending: LineEnding,
    pub truncated: bool,
}
