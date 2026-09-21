//! Bounded query/action payloads shared by the native worker and any
//! frontend (T39).
//!
//! These are view-model DTOs: counts, ids and bounded previews only. No
//! storage handles, no transcripts beyond the requested page, no secrets.

use crate::session::Role;

/// Prefs key holding the persisted model selection JSON.
pub const PREF_MODEL_SELECTION: &str = "tui.model_selection";
/// Prefs key holding the persisted primary agent JSON.
pub const PREF_PRIMARY_AGENT: &str = "tui.primary_agent";

/// One committed history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    /// Message sequence (ordering key for paging).
    pub seq: i64,
    /// `user` / `assistant`.
    pub role: Role,
    /// Message text.
    pub text: String,
}

/// One contiguous, bounded history page (oldest-first for rendering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPage {
    /// Rows in render order.
    pub rows: Vec<HistoryMessage>,
    /// Total committed messages in the session.
    pub total: usize,
    /// Older rows exist before the first row of this page.
    pub has_older: bool,
    /// Newer rows exist after the last row of this page.
    pub has_newer: bool,
}

/// One selectable model with its bounded variant list and limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
    /// Exact model id (no provider prefix).
    pub id: String,
    /// Declared variants.
    pub variants: Vec<VariantEntry>,
    /// Context limit (0 when undeclared).
    pub context: u64,
    /// Output limit (0 when undeclared).
    pub output: u64,
}

/// One declared model variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEntry {
    /// Variant name.
    pub name: String,
    /// True when the variant is declared but disabled.
    pub disabled: bool,
    /// Declared reasoning effort, if any.
    pub reasoning_effort: Option<String>,
}

/// One selectable agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    /// Profile id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Pinned model id, if any.
    pub model: Option<String>,
    /// Pinned variant, if any.
    pub variant: Option<String>,
}

/// Catalog plus the effective selection for the next turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Selected provider id.
    pub provider: String,
    /// Sorted models.
    pub models: Vec<ModelEntry>,
    /// Effective model id.
    pub model_id: String,
    /// Effective variant.
    pub variant: Option<String>,
    /// Sorted agent profiles.
    pub agents: Vec<AgentEntry>,
    /// Effective primary agent id, if resolved.
    pub agent_id: Option<String>,
    /// Workspace command ids (templates stay in the application).
    pub commands: Vec<String>,
}

/// Skill catalog card: metadata only, never bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCard {
    /// Directory id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
}

/// DCP context/stats snapshot (counts only, no transcript).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcpSnapshot {
    /// Estimated context tokens.
    pub estimated_tokens: u64,
    /// Effective max context tokens.
    pub max_context: u64,
    /// Turns since the last successful compression.
    pub turns_since_compress: u64,
    /// Stored compression blocks for the session.
    pub blocks: usize,
    /// Successful compressions.
    pub compressions: u64,
    /// Emitted nudges.
    pub nudges: u64,
    /// Recorded prune marks.
    pub prunes: u64,
}

/// One tool operation as recorded durably.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOpView {
    /// Operation id.
    pub op: String,
    /// Insertion-order cursor for newest-first paging.
    pub rowid: i64,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `denied` / `cancelled` / `unknown`.
    pub state: String,
    /// Recorded input JSON (bounded preview by the consumer).
    pub input: Option<String>,
    /// Recorded outcome (bounded preview by the consumer).
    pub output: Option<String>,
}

/// One bounded, newest-first tool operation page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOpPage {
    /// Rows newest-first.
    pub rows: Vec<ToolOpView>,
    /// Total recorded operations for the session.
    pub total: usize,
    /// Older operations exist before the last row of this page.
    pub has_older: bool,
}
