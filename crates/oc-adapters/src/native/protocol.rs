use super::{
    Lease,
    endpoint::Registration,
    wire::{Event, Request},
};
use crate::application::SpawnDiagnostic;
use oc_core::{domain::SessionId, queries::StartupNotice, session::CoreError};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Startup {
    pub version: u32,
    pub lease: Lease,
    pub project: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) enum Bootstrap {
    Ready(Registration),
    Busy,
    Failed(SpawnDiagnostic),
}

#[derive(Serialize, Deserialize)]
pub(super) struct Hello {
    pub version: u32,
    pub instance: String,
    pub lease: Lease,
    pub project: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Welcome {
    pub version: u32,
    pub instance: String,
    pub attachment: u64,
    pub location_epoch: u64,
    pub notices: Vec<StartupNotice>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub(super) enum Expected {
    Pending(u64),
    Origin(u64),
}

#[derive(Serialize, Deserialize)]
pub(super) enum Client {
    Request {
        id: u64,
        location_epoch: u64,
        expected: Option<Expected>,
        request: Box<Request>,
    },
    /// Separate ordering lane: cancellation may overtake an unsent submission.
    Control {
        id: u64,
        location_epoch: u64,
        expected: Option<Expected>,
        session: SessionId,
        turn: Option<oc_core::core_app::WorkerTurnId>,
    },
    CancelRequest {
        id: u64,
    },
    Release,
}
#[derive(Serialize, Deserialize)]
pub(super) enum Server {
    Reply {
        id: u64,
        location_epoch: u64,
        result: Result<serde_json::Value, CoreError>,
    },
    Bound {
        id: u64,
        session: SessionId,
        origin: u64,
    },
    Event {
        revision: u64,
        origin: Option<u64>,
        event: Box<Event>,
    },
    Released {
        closing: bool,
    },
}
