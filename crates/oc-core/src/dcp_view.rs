//! Durable DCP presentation, independent of provider output and UI widgets.
//!
//! `ToolOpView::dcp` and `CoreEvent::ToolCallFinished::dcp` carry the same frozen
//! successful-commit snapshot. `None` means non-compression or unavailable legacy
//! metadata; consumers must not reconstruct success metrics from output text.
//! Current panel accounting follows the restored conversation revision, whereas
//! historical headers and bars are never recomputed. Estimates are not billing.
//! Pending `ToolOpView::dcp_topic` / `CoreEvent::ToolCallStarted::dcp_topic`
//! is owner-derived from validated arguments/plans. Raw argument-stream UI must
//! wait for this optional metadata rather than parsing a topic from JSON text.
use serde::{Deserialize, Serialize};

/// No tokenizer is installed: JS-compatible Math.round(UTF-16 length / 4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DcpEstimateMethod {
    #[default]
    Utf16RoundQuarterFallback,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DcpNotificationMode {
    Off,
    Minimal,
    #[default]
    Detailed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DcpNotificationChannel {
    #[default]
    Chat,
    Toast,
}

/// Independent presentation controls, projected by the composition owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcpDisplayConfig {
    pub notification: DcpNotificationMode,
    pub channel: DcpNotificationChannel,
    pub show_compression: bool,
}

/// Measured accounting in this session's current conversation revision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcpAccounting {
    pub gross_removed: u64,
    pub active_summary: u64,
    pub net_saved: u64,
    pub compressions: u64,
    pub prunes: u64,
    pub method: DcpEstimateMethod,
    /// False if legacy projection changes precede the measured accounting.
    pub complete: bool,
}

/// One successful operation, even when it contains multiple ranges. Compact:
/// no message archive, covered-ID list or full summaries are embedded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DcpRunSnapshot {
    pub session: String,
    pub operation_id: String,
    /// Session-local durable high-water ordinal, independent of block numbering.
    pub ordinal: u64,
    pub topic: String,
    pub block_ids: Vec<String>,
    pub removed: u64,
    pub summary: u64,
    /// Actual projected text/input/output decrease; generated anchor-lane
    /// instructions and the new compress result are outside this measurement.
    pub net_saved: u64,
    pub method: DcpEstimateMethod,
    pub new_messages: u64,
    pub new_tools: u64,
    pub cumulative: DcpAccounting,
    /// Exactly 50 categorical cells, without border glyphs. Frozen at commit.
    pub bar: String,
}

/// A bounded window of one real stored range summary. Request the next offset
/// and then the next block index in `DcpRunSnapshot::block_ids`. Multi-range
/// headings are presentation; topic and block identity are supplied separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcpSummaryPage {
    pub block_id: String,
    pub topic: String,
    pub text: String,
    pub total_bytes: i64,
    pub next_offset: Option<i64>,
}

/// Estimate each text/input/output content separately, never JSON envelope bytes.
pub fn estimate_content(text: &str) -> u64 {
    (text.encode_utf16().count() as u64).saturating_add(2) / 4
}
