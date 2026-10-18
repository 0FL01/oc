//! Native same-UID application attachment. This is not a remote/public API.
mod client;
mod endpoint;
mod framing;
mod protocol;
mod secrets;
mod server;
mod wire;

use serde::{Deserialize, Serialize};

/// A finite attachment's lifetime, not a persistent daemon keepalive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lease {
    Tui,
    Run,
    Auth,
    Sessions,
}

pub use client::attach;
pub use server::serve;

fn issue(code: oc_core::queries::ServiceCode) -> crate::application::SpawnDiagnostic {
    use crate::application::{SpawnDiagnostic, SpawnFailure};
    let category = match code {
        oc_core::queries::ServiceCode::DataRootBusy => SpawnFailure::DataRootBusy,
        oc_core::queries::ServiceCode::UnsafeDataRoot => SpawnFailure::UnsafeDataRoot,
        _ => SpawnFailure::Runtime,
    };
    let mut issue = SpawnDiagnostic::new(category, "native owner", &["attachment"], code);
    if code == oc_core::queries::ServiceCode::ProtocolMismatch {
        issue.diagnostic.action = oc_core::queries::ServiceAction::RestartApplication;
    }
    issue
}
