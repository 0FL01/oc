//! Session checkpoint compaction, independent of DCP range compression.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionReason {
    Manual,
    Automatic,
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
}

/// Public causal placement for live and reopened transcript interleaving.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionAnchor {
    pub message: Option<String>,
    pub turn: Option<String>,
    pub tool: Option<String>,
}

/// Public replayable operation. Opaque provider continuation never crosses this API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionSnapshot {
    /// Producing request scope in the existing JSON record (legacy absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<crate::queries::ModelRef>,
    #[serde(default)]
    pub anchor: CompactionAnchor,
    pub id: String,
    pub session: String,
    pub reason: CompactionReason,
    pub state: CompactionState,
    pub summary: String,
    pub usage: Option<CompactionUsage>,
    pub provider_native: bool,
    pub error: Option<String>,
}
