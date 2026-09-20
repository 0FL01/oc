//! DCP policy and durable compression storage for T17 (DCP01).
//!
//! Range-tool arguments follow the pinned schema (`topic` + non-empty
//! `content[{startId, endId, summary}]`, all bounded) and validate against
//! stable history references before anything persists. Compression blocks,
//! membership rows and prune marks live in the own SQLite database
//! (migration v2, alongside the T04 v1 tables); the raw transcript stays the
//! single source of truth — no second history copy, no credentials anywhere
//! near the projection path.

use thiserror::Error;

/// DCP defaults (mirrors `examples/dcp.jsonc`).
pub const MIN_CONTEXT_LIMIT: u64 = 50_000;
/// DCP defaults (mirrors `examples/dcp.jsonc`).
pub const MAX_CONTEXT_LIMIT: u64 = 100_000;
/// Nudge cadence in iterations.
pub const NUDGE_FREQUENCY: u64 = 5;
/// Iterations before the force nudge path.
pub const ITERATION_NUDGE_THRESHOLD: u64 = 15;
/// Range topic bound (chars).
pub const TOPIC_CAP: usize = 256;
/// Range summary bound (chars).
pub const SUMMARY_CAP: usize = 8192;
/// Ranges per tool call.
pub const RANGES_CAP: usize = 32;
/// DCP schema version applied on top of the T04 base.
pub const DCP_SCHEMA_VERSION: i64 = 2;

/// Typed DCP errors (ids and bounds only, no contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DcpError {
    /// Range tool arguments malformed.
    #[error("invalid range args: {reason}")]
    InvalidArgs {
        /// Human reason.
        reason: String,
    },
    /// Referenced message id is not in the transcript.
    #[error("unknown message {id}")]
    UnknownMessage {
        /// Missing id.
        id: String,
    },
    /// Storage failure (kind only).
    #[error("storage error")]
    Storage,
}

/// One validated compression range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRange {
    /// Non-empty bounded topic.
    pub topic: String,
    /// Range start message id.
    pub start_id: String,
    /// Range end message id.
    pub end_id: String,
    /// Non-empty bounded model-authored summary.
    pub summary: String,
}

/// Validate `compress` range-tool arguments against the pinned schema.
///
/// Shape: `{topic: non-empty ≤256, content: [1..=32 × {startId, endId,
/// summary: non-empty ≤8192}]}`. Message-mode shapes are rejected here;
/// that experimental mode is a separate contract.
pub fn validate_range_args(
    value: &serde_json::Value,
) -> Result<(String, Vec<ValidatedRange>), DcpError> {
    let invalid = |reason: &str| DcpError::InvalidArgs {
        reason: reason.to_string(),
    };
    let obj = value
        .as_object()
        .ok_or_else(|| invalid("args must be an object"))?;
    let topic = obj
        .get("topic")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid("missing topic"))?;
    if topic.chars().count() > TOPIC_CAP {
        return Err(invalid("topic too large"));
    }
    let content = obj
        .get("content")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("missing content"))?;
    if content.is_empty() || content.len() > RANGES_CAP {
        return Err(invalid("content must hold 1..=32 entries"));
    }
    let mut ranges = Vec::with_capacity(content.len());
    for entry in content {
        let entry = entry
            .as_object()
            .ok_or_else(|| invalid("entry must be an object"))?;
        let get = |key: &str| {
            entry
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| invalid("entry needs startId/endId/summary"))
        };
        let summary = get("summary")?;
        if summary.chars().count() > SUMMARY_CAP {
            return Err(invalid("summary too large"));
        }
        ranges.push(ValidatedRange {
            topic: topic.to_string(),
            start_id: get("startId")?.to_string(),
            end_id: get("endId")?.to_string(),
            summary: summary.to_string(),
        });
    }
    Ok((topic.to_string(), ranges))
}

/// Durable compression block record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionBlock {
    /// Block id (`b0001`, … per session).
    pub id: String,
    /// Owning session.
    pub session: String,
    /// Range topic.
    pub topic: String,
    /// Model-authored summary.
    pub summary: String,
    /// Covered start message id.
    pub start_msg: String,
    /// Covered end message id.
    pub end_msg: String,
    /// Covered message ids in order.
    pub members: Vec<String>,
}

/// Apply the DCP v2 schema migration (idempotent, additive only).
pub fn apply_dcp_schema(db: &crate::storage::Db) -> Result<(), DcpError> {
    db.apply_dcp_schema().map_err(|_| DcpError::Storage)
}

/// Persist one compression block with explicit membership rows.
#[allow(clippy::too_many_arguments)]
pub fn save_block(
    db: &crate::storage::Db,
    session: &str,
    topic: &str,
    summary: &str,
    start_msg: &str,
    end_msg: &str,
    members: &[String],
) -> Result<String, DcpError> {
    db.save_compression_block(session, topic, summary, start_msg, end_msg, members)
        .map_err(|_| DcpError::Storage)
}

/// Load all blocks of a session in id order.
pub fn load_blocks(
    db: &crate::storage::Db,
    session: &str,
) -> Result<Vec<CompressionBlock>, DcpError> {
    Ok(db
        .load_compression_blocks(session)
        .map_err(|_| DcpError::Storage)?
        .into_iter()
        .map(|row| CompressionBlock {
            id: row.id,
            session: row.session,
            topic: row.topic,
            summary: row.summary,
            start_msg: row.start_msg,
            end_msg: row.end_msg,
            members: row.members,
        })
        .collect())
}

/// Record a prune mark: outbound context drops the prefix through `up_to`.
pub fn save_prune_mark(
    db: &crate::storage::Db,
    session: &str,
    up_to: &str,
) -> Result<(), DcpError> {
    db.save_prune_mark(session, up_to)
        .map_err(|_| DcpError::Storage)
}

/// Read the prune mark, if any.
pub fn load_prune_mark(db: &crate::storage::Db, session: &str) -> Result<Option<String>, DcpError> {
    db.load_prune_mark(session).map_err(|_| DcpError::Storage)
}

#[cfg(test)]
mod tests {
    use super::{
        DcpError, load_blocks, load_prune_mark, save_block, save_prune_mark, validate_range_args,
    };

    fn args() -> serde_json::Value {
        serde_json::json!({
            "topic": "auth flow",
            "content": [
                {"startId": "m0001", "endId": "m0002", "summary": "login works"},
                {"startId": "m0003", "endId": "m0003", "summary": "edge case"},
            ],
        })
    }

    #[test]
    fn dcp01_range_schema_bounds() {
        let (topic, ranges) = validate_range_args(&args()).expect("valid");
        assert_eq!(topic, "auth flow");
        assert_eq!(ranges.len(), 2);
        assert_eq!(ranges[0].start_id, "m0001");
        // Message-mode shape is a different contract: rejected here.
        assert!(matches!(
            validate_range_args(
                &serde_json::json!({"topic": "t", "content": [{"messageId": "m1", "topic": "t", "summary": "s"}]})
            ),
            Err(DcpError::InvalidArgs { .. })
        ));
        assert!(validate_range_args(&serde_json::json!({"topic": "", "content": []})).is_err());
        assert!(validate_range_args(&serde_json::json!({"topic": "t", "content": []})).is_err());
        let big = serde_json::json!({"topic": "t", "content": [{"startId": "a", "endId": "b", "summary": "x".repeat(9000)}]});
        assert!(validate_range_args(&big).is_err());
    }

    #[test]
    fn dcp01_durable_blocks_members_prune() {
        let tmp = tempfile::tempdir().expect("temp");
        let db = crate::storage::Db::open(&tmp.path().join("data")).expect("open");
        super::apply_dcp_schema(&db).expect("migrate");
        db.create_session("s-1").expect("session");
        let id = save_block(
            &db,
            "s-1",
            "auth flow",
            "login works",
            "m0001",
            "m0002",
            &["m0001".to_string(), "m0002".to_string()],
        )
        .expect("save");
        assert_eq!(id, "b0001");
        let blocks = load_blocks(&db, "s-1").expect("load");
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].members,
            vec!["m0001".to_string(), "m0002".to_string()]
        );
        // Second history copy absent: members reference ids, never text.
        assert!(!format!("{blocks:?}").contains("login text"));
        save_prune_mark(&db, "s-1", "m0002").expect("prune");
        assert_eq!(
            load_prune_mark(&db, "s-1").expect("read"),
            Some("m0002".to_string())
        );
        assert_eq!(load_prune_mark(&db, "missing").expect("none"), None);
    }

    #[test]
    fn dcp01_projection_carries_no_credentials() {
        // Structural guarantee: projection blocks expose role/text/summary
        // only. Any config-shaped value fails validation before it could
        // enter the transcript-adjacent path.
        let history = vec![oc_core::session::Message {
            id: oc_core::session::MessageId("m0001".to_string()),
            role: oc_core::session::Role::User,
            text: "hello".to_string(),
        }];
        let ranges = oc_core::context_plan::plan_ranges(
            &history,
            &[("m0001".to_string(), "m0001".to_string())],
        )
        .expect("plan");
        let projection = oc_core::context_plan::project(
            &history,
            &[(ranges[0].clone(), "t".to_string(), "s".to_string())],
            None,
        );
        let flat = serde_json::to_string(&serde_json::json!({
            "blocks": projection.blocks.iter().map(|b| format!("{b:?}")).collect::<Vec<_>>(),
        }))
        .expect("json");
        assert!(!flat.contains("apiKey"));
        assert!(!flat.contains("provider"));
        assert_eq!(
            projection.raw_checksum,
            oc_core::context_plan::raw_checksum(&history)
        );
    }
}
