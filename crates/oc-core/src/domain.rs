//! Minimal domain ids for T01 compile smoke.
//! Full session/message/tool/model/context/error types arrive in M1.

use serde::{Deserialize, Serialize};

/// Opaque session identifier; never parsed as path or URL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    /// Create a session id from a non-empty trimmed value.
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let value = raw.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionId;

    #[test]
    fn rejects_empty_session_id() {
        assert!(SessionId::new("   ").is_none());
        assert!(SessionId::new("s-1").is_some());
    }

    #[test]
    fn serde_roundtrip() {
        let id = SessionId::new("s-1").expect("valid");
        let json = serde_json::to_string(&id).expect("serialize");
        let back: SessionId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }
}
