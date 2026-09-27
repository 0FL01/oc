//! Disposable provider presentation, never an execution or history record.

/// Maximum retained pending calls per provider round.
pub const PENDING_TOOL_MAX: usize = 32;
/// UTF-8 argument prefix budget per pending call.
pub const ARGUMENT_PREVIEW_MAX: usize = 4096;

/// Exact provider identity, scoped to a turn's one-based provider round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStreamIdentity {
    /// One-based provider round (call IDs may repeat across rounds).
    pub round: u32,
    /// Provider output item ID.
    pub item_id: String,
    /// Provider function call ID.
    pub call_id: String,
}

/// Bounded live-only presentation lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStreamEvent {
    /// Replacement snapshot, not a fragment to append or parse as arguments.
    Pending {
        /// Exact provider identity.
        identity: ToolStreamIdentity,
        /// Announced registry name; does not imply availability or permission.
        name: String,
        /// Bounded raw UTF-8 prefix; never interpreted as structured arguments.
        preview: String,
        /// More bytes arrived than the retained prefix budget.
        truncated: bool,
    },
    /// Issued only after the exact canonical call's durable intent exists.
    Linked {
        /// Exact canonical provider identity.
        identity: ToolStreamIdentity,
        /// Actual durable operation ID.
        op: String,
    },
    /// Drop unresolved presentation for a settled or abandoned round.
    Clear {
        /// One-based provider round.
        round: u32,
    },
}
