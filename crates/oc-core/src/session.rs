//! Session domain types for T03 vertical slice.
//!
//! Full message/tool/model/context/error taxonomy arrives incrementally in
//! M1–M4. This keeps the T01 `SessionId` shape and adds the minimal turn and
//! history types needed for input → MockProvider stream → in-memory history.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Opaque turn identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub String);

impl TurnId {
    /// Create a turn id; returns `None` on empty input.
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let value = raw.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }
}

/// Opaque message identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    /// Create a message id; returns `None` on empty input.
    pub fn new(raw: impl Into<String>) -> Option<Self> {
        let value = raw.into();
        if value.trim().is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }
}

/// Message role for the T03 in-memory history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Human input.
    User,
    /// MockProvider output.
    Assistant,
}

/// Single history message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Stable per-session sequence (`m0001`, `m0002`, …).
    pub id: MessageId,
    /// Who produced the message.
    pub role: Role,
    /// Bounded text payload.
    pub text: String,
}

/// Typed core errors for the T03 slice.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// Session id is unknown.
    #[error("session not found")]
    SessionNotFound,
    /// Session id already exists.
    #[error("session already exists")]
    SessionAlreadyExists,
    /// Another turn is already active (single-turn worker).
    #[error("turn busy")]
    TurnBusy,
    /// No active turn for the session.
    #[error("no active turn")]
    TurnNotActive,
    /// Bounded inbox is full; input was not accepted.
    #[error("queue full")]
    QueueFull,
    /// Single input exceeds the T03 smoke cap.
    #[error("input too large")]
    InputTooLarge,
    /// Worker is shut down.
    #[error("shutdown")]
    Shutdown,
    /// Provider failure (scripted only in T03).
    #[error("provider: {0}")]
    Provider(String),
}

/// Initial smoke caps for T03 (see `examples/oc-rs.toml` for product caps).
///
/// Product queue caps are 8 MiB / 256 items; the single-input smoke cap is a
/// temporary 1 MiB bound so `InputTooLarge` is testable without allocating
/// megabytes. Full byte/item admission arrives with storage/limits in M1–M2.
pub const MAX_QUEUE_ITEMS: usize = 256;
/// Product queue byte cap reference (not yet enforced across the whole queue).
pub const MAX_QUEUE_BYTES: usize = 8 * 1024 * 1024;
/// Single-input smoke cap for T03.
pub const MAX_INPUT_BYTES: usize = 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::{CoreError, MessageId, TurnId};

    #[test]
    fn ids_reject_empty() {
        assert!(TurnId::new("  ").is_none());
        assert!(MessageId::new("").is_none());
        assert!(TurnId::new("t-1").is_some());
    }

    #[test]
    fn error_messages_are_stable() {
        assert_eq!(CoreError::TurnBusy.to_string(), "turn busy");
        assert_eq!(CoreError::QueueFull.to_string(), "queue full");
    }
}
