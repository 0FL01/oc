//! Minimal application façade for T01.
//! Full typed commands/queries arrive in M1 (T03).

use crate::domain::SessionId;

/// Bounded application handle placeholder.
///
/// The real handle will be a cloneable typed sender/query façade over a
/// single `SessionWorker`. For T01 this only proves the crate compiles and
/// the public application API is reachable from `oc-tui` and `oc`.
#[derive(Debug, Clone)]
pub struct AppHandle {
    pending_sessions: u64,
}

impl AppHandle {
    /// Create a smoke handle with no sessions.
    pub fn smoke() -> Self {
        Self {
            pending_sessions: 0,
        }
    }

    /// Return the number of locally known sessions (smoke: always zero).
    pub fn pending_sessions(&self) -> u64 {
        self.pending_sessions
    }

    /// Validate a session id through the application layer.
    pub fn describe_session(&self, id: &SessionId) -> String {
        format!("session:{}", id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::AppHandle;
    use crate::domain::SessionId;

    #[test]
    fn smoke_handle_reports_zero() {
        let app = AppHandle::smoke();
        assert_eq!(app.pending_sessions(), 0);
        let id = SessionId::new("s-1").expect("valid");
        assert_eq!(app.describe_session(&id), "session:s-1");
    }
}
