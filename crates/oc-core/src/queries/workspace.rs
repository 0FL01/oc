//! Bounded shared-workspace projection. This is not raw event history or a
//! provider continuation; ordinary history still uses the existing paged reads.
use super::TabDeckSnapshot;
use crate::{
    core_app::{CoreEvent, WorkerTurnId},
    domain::SessionId,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceSnapshot {
    pub revision: u64,
    pub deck: TabDeckSnapshot,
    pub turns: Vec<LiveTurnSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LiveTurnSnapshot {
    pub session: SessionId,
    pub turn: WorkerTurnId,
    pub origin: Option<u64>,
    /// Materialized/coalesced public parts, bounded like the live viewport.
    pub events: Vec<CoreEvent>,
    pub display_name: Option<String>,
    pub truncated: bool,
}
