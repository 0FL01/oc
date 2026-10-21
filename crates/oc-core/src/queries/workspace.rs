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
    /// OC2 steer/queue input pending for the selected session.
    #[serde(default)]
    pub inbox: Vec<super::InboxItem>,
    /// Root of the selected route (which may itself be a linked child).
    #[serde(default)]
    pub selected_root: Option<SessionId>,
    /// Durable read watermarks and current family activity for the bounded deck.
    #[serde(default)]
    pub activity: Vec<SessionActivity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SessionIdleOutcome {
    Activity,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionActivity {
    pub session: SessionId,
    pub idle_seq: Option<u64>,
    pub viewed_seq: Option<u64>,
    pub outcome: Option<SessionIdleOutcome>,
    pub busy: bool,
}

impl SessionActivity {
    pub fn unread(&self) -> Option<SessionIdleOutcome> {
        self.idle_seq
            .filter(|idle| self.viewed_seq.is_none_or(|viewed| *idle > viewed))
            .and(self.outcome)
    }
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
