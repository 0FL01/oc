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

#[cfg(test)]
#[path = "dcp/renewal_tests.rs"]
mod renewal_tests;

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
    /// Referenced compression block is not in the existing or candidate set.
    #[error("unknown block {id}")]
    UnknownBlock {
        /// Missing block id.
        id: String,
    },
    /// A candidate member is already covered by an effective block.
    #[error("message already compressed {id}")]
    ExistingOverlap {
        /// Conflicting message id.
        id: String,
    },
    /// Protected payload too large to preserve verbatim: compression is
    /// visibly impossible, never silently lossy.
    #[error("compression impossible: {reason}")]
    Impossible {
        /// Human reason.
        reason: String,
    },
    /// Nested block reference cycle.
    #[error("block reference cycle at {id}")]
    Cycle {
        /// Block id closing the cycle.
        id: String,
    },
    /// Nested expansion over depth/byte limits.
    #[error("nested expansion too large: {reason}")]
    NestedTooLarge {
        /// Human reason.
        reason: String,
    },
    /// The measured serialized projection did not shrink.
    #[error("compression has no gain: {before_bytes} -> {after_bytes} bytes")]
    NoGain {
        /// Existing projected serialized size.
        before_bytes: usize,
        /// Candidate projected serialized size.
        after_bytes: usize,
    },
    /// The durable compression snapshot changed before commit.
    #[error("compression state changed")]
    Conflict,
    /// Patch touches protected paths.
    #[error("protected patch paths")]
    PatchProtected {
        /// Violating paths.
        paths: Vec<String>,
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

/// Bounded canonical arguments for operation-backed pending presentation.
pub(crate) fn presentation_topic(name: &str, input: Option<&str>) -> Option<String> {
    if name != "compress" {
        return None;
    }
    let input = input.filter(|input| input.len() <= crate::provider::ARGUMENT_BYTE_CAP)?;
    let args = serde_json::from_str::<serde_json::Value>(input).ok()?;
    validate_range_args(&args).ok().map(|(topic, _)| topic)
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
    /// Native standalone working selection; absent on legacy authored graphs.
    pub hot: Option<serde_json::Value>,
}

/// Pure, fully validated compression candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionPlan {
    pub(crate) projection_revision: Option<i64>,
    /// Bounded graph captured with the addressed planning snapshot.
    pub(crate) context_blocks: Vec<CompressionBlock>,
    /// Independently measured content estimates and coverage candidates.
    pub(crate) measurement: DcpMeasurement,
    /// Candidate blocks in transcript order with their final durable ids.
    pub blocks: Vec<CompressionBlock>,
    /// Existing projection serialized as the provider-facing row shape.
    pub before_bytes: usize,
    /// Candidate projection serialized as the provider-facing row shape.
    pub after_bytes: usize,
    /// Independently rounded public-content estimate, not serialized-byte delta.
    pub saved_tokens: u64,
    /// Older blocks whose active memberships are subsumed by this plan.
    pub consumed_blocks: Vec<String>,
    first_block_number: u64,
}

impl CompressionPlan {
    /// Approval binds the addressed session selection, not a reservation of the
    /// shared durable ID allocator. Another session may allocate IDs while the
    /// consumer waits; all coverage, revision, graph and protected payload facts
    /// must still match. Gain is measured again against the adopted fresh plan.
    pub(crate) fn same_approval_selection(&self, other: &Self) -> bool {
        let normalize = |plan: &Self| {
            let mut plan = plan.clone();
            plan.first_block_number = 0;
            plan.before_bytes = 0;
            plan.after_bytes = 0;
            plan.saved_tokens = 0;
            for block in &mut plan.blocks {
                block.id.clear();
            }
            plan
        };
        normalize(self) == normalize(other)
    }
}

/// Successful durable compression result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressionReport {
    /// Frozen operation-backed presentation published only after commit.
    pub snapshot: oc_core::dcp_view::DcpRunSnapshot,
    /// Blocks committed by this call.
    pub blocks: Vec<CompressionBlock>,
    /// Existing projected serialized bytes.
    pub before_bytes: usize,
    /// Committed projected serialized bytes.
    pub after_bytes: usize,
    /// Independently rounded public-content estimate, not serialized-byte delta.
    pub saved_tokens: u64,
}

/// Prepared content accounting. Coverage is deduplicated again transactionally.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DcpMeasurement {
    pub unavailable_coverage: bool,
    pub prunes: u64,
    pub removed: u64,
    pub summary: u64,
    pub net_saved: u64,
    pub active_summary: u64,
    pub messages: Vec<String>,
    /// Durable turn + turn-local call occurrence, independent of window ranks.
    pub calls: Vec<crate::storage::DcpCallIdentity>,
    pub retire_keys: Vec<crate::storage::DcpCallKey>,
    pub replacement_projection: Option<crate::storage::DcpToolProjection>,
}

/// Optional existing tool operation and turn journal finalized with the blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionCommitMetadata<'a> {
    /// Existing model tool operation id.
    pub operation_id: &'a str,
    /// Terminal operation state.
    pub operation_state: &'a str,
    /// Structured operation output.
    pub operation_output: &'a str,
    /// Existing owning turn id for model dispatch; absent for manual dispatch.
    pub turn_id: Option<&'a str>,
    /// Updated replayable turn log, present exactly with `turn_id`.
    pub turn_log: Option<&'a str>,
    /// Preference updates committed at the same durability boundary.
    pub preference_updates: &'a [(String, String)],
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
    db.load_compression_blocks(session)
        .map_err(|_| DcpError::Storage)?
        .into_iter()
        .map(block_from_row)
        .collect()
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

/// Protected payload bound: a covered protected message larger than this
/// makes compression visibly impossible instead of silently lossy.
pub const PROTECTED_BYTES_CAP: usize = 65536;
/// Nested expansion depth cap (cycle/size guards fire first).
pub const NESTED_DEPTH_CAP: usize = 8;
/// Nested expansion byte cap.
pub const NESTED_BYTES_CAP: usize = 65536;

/// Build a complete compression candidate without storage writes.
///
/// Input ranges may be out of transcript order: each summary remains attached
/// to its own resolved anchors. Existing and candidate nested references are
/// expanded strictly before actual before/after row serialization is measured.
#[allow(clippy::too_many_arguments)]
pub fn plan_compression(
    session: &str,
    history: &[oc_core::session::Message],
    validated: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
    existing: &[CompressionBlock],
    prune_up_to: Option<&str>,
    next_block_number: u64,
) -> Result<CompressionPlan, DcpError> {
    plan_compression_with_gain(
        session,
        history,
        validated,
        spec,
        existing,
        prune_up_to,
        next_block_number,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn plan_compression_with_gain(
    session: &str,
    history: &[oc_core::session::Message],
    validated: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
    existing: &[CompressionBlock],
    prune_up_to: Option<&str>,
    next_block_number: u64,
    require_row_gain: bool,
) -> Result<CompressionPlan, DcpError> {
    use oc_core::context_plan::{PlanError, message_protected, plan_ranges};

    if validated.is_empty() || validated.len() > RANGES_CAP {
        return Err(DcpError::InvalidArgs {
            reason: "content must hold 1..=32 entries".to_string(),
        });
    }
    let existing_by_id = existing
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut consumed = std::collections::BTreeSet::new();
    let mut resolved = Vec::with_capacity(validated.len());
    for range in validated {
        if range.topic.trim().is_empty()
            || range.topic.chars().count() > TOPIC_CAP
            || range.summary.trim().is_empty()
            || range.summary.chars().count() > SUMMARY_CAP
        {
            return Err(DcpError::InvalidArgs {
                reason: "range topic/summary is empty or too large".to_string(),
            });
        }
        if range.summary.contains(PROTECTED_USER_HEADING)
            || range.summary.contains(PROTECTED_CONTENT_HEADING)
        {
            return Err(DcpError::InvalidArgs {
                reason: "summary contains a reserved protection heading".to_string(),
            });
        }
        let start = if let Some(block) = existing_by_id.get(range.start_id.as_str()) {
            consumed.insert(block.id.clone());
            block.start_msg.clone()
        } else {
            range.start_id.clone()
        };
        let end = if let Some(block) = existing_by_id.get(range.end_id.as_str()) {
            consumed.insert(block.id.clone());
            block.end_msg.clone()
        } else {
            range.end_id.clone()
        };
        for placeholder in parse_block_placeholders(&range.summary) {
            if existing_by_id.contains_key(placeholder.block_id.as_str()) {
                consumed.insert(placeholder.block_id);
            }
        }
        let one = plan_ranges(history, &[(start, end)]).map_err(|error| match error {
            PlanError::UnknownMessage { id } => DcpError::UnknownMessage { id },
            other => DcpError::InvalidArgs {
                reason: other.to_string(),
            },
        })?;
        resolved.push((range, one.into_iter().next().expect("one planned range")));
    }
    resolved.sort_by_key(|(_, range)| (range.start, range.end));
    for pair in resolved.windows(2) {
        if pair[1].1.start <= pair[0].1.end {
            return Err(DcpError::InvalidArgs {
                reason: "overlapping ranges".to_string(),
            });
        }
    }

    let positions = history
        .iter()
        .enumerate()
        .map(|(index, message)| (message.id.0.as_str(), index))
        .collect::<std::collections::BTreeMap<_, _>>();
    if !require_row_gain {
        for block in existing {
            if let (Some(start), Some(end)) = (
                positions.get(block.start_msg.as_str()),
                positions.get(block.end_msg.as_str()),
            ) && resolved
                .iter()
                .any(|(_, range)| range.start <= *start && *end <= range.end)
            {
                consumed.insert(block.id.clone());
            }
        }
    }
    for id in &consumed {
        let block = existing_by_id
            .get(id.as_str())
            .expect("known consumed block");
        let covered = resolved.iter().any(|(_, range)| {
            block.members.iter().all(|member| {
                positions
                    .get(member.as_str())
                    .is_some_and(|position| range.start <= *position && *position <= range.end)
            })
        });
        if !covered {
            return Err(DcpError::InvalidArgs {
                reason: format!("referenced block {id} is not fully covered"),
            });
        }
    }
    if let Some(tail) = history.len().checked_sub(1)
        && resolved
            .iter()
            .any(|(_, range)| range.start <= tail && tail <= range.end)
    {
        return Err(DcpError::InvalidArgs {
            reason: "range covers the unfinished tail".to_string(),
        });
    }

    let existing_members: std::collections::HashSet<&str> = existing
        .iter()
        .filter(|block| !consumed.contains(&block.id))
        .flat_map(|block| block.members.iter().map(String::as_str))
        .collect();
    let mut blocks = Vec::with_capacity(resolved.len());
    for (offset, (range, resolved)) in resolved.into_iter().enumerate() {
        let members: Vec<String> = history
            [resolved.start..=resolved.end.min(history.len().saturating_sub(1))]
            .iter()
            .map(|message| message.id.0.clone())
            .collect();
        if let Some(id) = members
            .iter()
            .find(|id| existing_members.contains(id.as_str()))
        {
            return Err(DcpError::ExistingOverlap { id: id.clone() });
        }

        let mut user_texts = Vec::new();
        let mut other_texts = Vec::new();
        for message in &history[resolved.start..=resolved.end] {
            if message.text.is_empty() || !message_protected(spec, message) {
                continue;
            }
            if message.text.len() > PROTECTED_BYTES_CAP {
                return Err(DcpError::Impossible {
                    reason: "protected content exceeds verbatim budget".to_string(),
                });
            }
            if spec.protect_user_messages && matches!(message.role, oc_core::session::Role::User) {
                user_texts.push(message.text.clone());
                if spec.protect_tags {
                    other_texts.extend(
                        oc_core::context_plan::extract_protect_tags(&message.text)
                            .map(String::from),
                    );
                }
            } else {
                // Tag- and file-glob-protected messages are retained in full,
                // not reduced to only the matching token/span.
                other_texts.push(message.text.clone());
            }
        }
        let summary = append_protected_tags(
            &append_protected_user_messages(&range.summary, &user_texts),
            &other_texts,
        );
        let number = next_block_number
            .checked_add(offset as u64)
            .ok_or_else(|| DcpError::Impossible {
                reason: "block id space exhausted".to_string(),
            })?;
        let id = format!("b{number:04}");
        if existing.iter().any(|block| block.id == id) {
            return Err(DcpError::Conflict);
        }
        blocks.push(CompressionBlock {
            id,
            session: session.to_string(),
            topic: range.topic.clone(),
            summary,
            start_msg: members.first().cloned().expect("planned range has a start"),
            end_msg: members.last().cloned().expect("planned range has an end"),
            members,
            hot: None,
        });
    }

    let history_rows = history
        .iter()
        .map(|message| {
            (
                message.id.0.clone(),
                match message.role {
                    oc_core::session::Role::User => "user".to_string(),
                    oc_core::session::Role::Assistant => "assistant".to_string(),
                },
                message.text.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut candidate = if require_row_gain {
        existing.to_vec()
    } else {
        prepared_existing(existing)?
    };
    for block in &mut candidate {
        if consumed.contains(&block.id) {
            block.members.clear();
        }
    }
    candidate.extend(blocks.iter().cloned());
    validate_block_graph(&candidate)?;
    let before = project_rows_checked(&history_rows, existing, prune_up_to)?;
    let after = project_rows_checked(&history_rows, &candidate, prune_up_to)?;
    let before_bytes = serialized_rows_len(&before)?;
    let after_bytes = serialized_rows_len(&after)?;
    if require_row_gain && after_bytes >= before_bytes {
        return Err(DcpError::NoGain {
            before_bytes,
            after_bytes,
        });
    }
    let estimate = oc_core::dcp_view::estimate_content;
    let before_tokens: u64 = before.iter().map(|(_, _, text)| estimate(text)).sum();
    let after_tokens: u64 = after.iter().map(|(_, _, text)| estimate(text)).sum();
    let changed = before
        .iter()
        .filter(|row| !after.contains(row))
        .collect::<Vec<_>>();
    let saved_tokens = before_tokens.saturating_sub(after_tokens);
    let measurement = DcpMeasurement {
        unavailable_coverage: false,
        prunes: 0,
        removed: changed
            .iter()
            .filter(|(id, _, _)| !existing.iter().any(|b| &b.id == id))
            .map(|(_, _, text)| estimate(text))
            .sum(),
        summary: after
            .iter()
            .filter(|(id, _, _)| blocks.iter().any(|block| &block.id == id))
            .map(|(id, _, text)| {
                estimate(
                    text.strip_prefix(&format!("[compressed {id}] "))
                        .unwrap_or(text),
                )
            })
            .sum(),
        net_saved: saved_tokens,
        active_summary: after
            .iter()
            .filter(|(id, _, _)| candidate.iter().any(|block| &block.id == id))
            .map(|(id, _, text)| {
                estimate(
                    text.strip_prefix(&format!("[compressed {id}] "))
                        .unwrap_or(text),
                )
            })
            .sum(),
        messages: changed
            .iter()
            .filter(|(id, _, _)| history.iter().any(|m| &m.id.0 == id))
            .map(|(id, _, _)| id.clone())
            .collect(),
        calls: Vec::new(),
        retire_keys: Vec::new(),
        replacement_projection: None,
    };
    Ok(CompressionPlan {
        projection_revision: None,
        context_blocks: existing.to_vec(),
        measurement,
        blocks,
        before_bytes,
        after_bytes,
        saved_tokens,
        consumed_blocks: consumed.into_iter().collect(),
        first_block_number: next_block_number,
    })
}

/// Plan and atomically commit every range/member and optional tool journal.
pub fn compress_ranges_atomic(
    db: &crate::storage::Db,
    session: &str,
    history: &[oc_core::session::Message],
    validated: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
    commit: Option<&CompressionCommitMetadata<'_>>,
) -> Result<CompressionReport, DcpError> {
    let plan = prepare_compression(db, session, history, validated, spec, true)?;
    commit_compression(db, session, plan, commit)
}

/// Prepare against one consistent storage snapshot without writing.
pub(crate) fn prepare_compression(
    db: &crate::storage::Db,
    session: &str,
    history: &[oc_core::session::Message],
    validated: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
    require_row_gain: bool,
) -> Result<CompressionPlan, DcpError> {
    let (rows, prune, next) = db.compression_snapshot(session).map_err(map_storage)?;
    let existing = rows
        .into_iter()
        .map(block_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    plan_compression_with_gain(
        session,
        history,
        validated,
        spec,
        &existing,
        prune.as_deref(),
        next,
        require_row_gain,
    )
}

/// Runtime planner over a bounded SQL snapshot. Covered message IDs have no
/// raw payload; inherited protections come from their durable summaries.
#[allow(clippy::too_many_arguments)]
pub(crate) fn plan_active_compression(
    session: &str,
    history: &[oc_core::session::Message],
    ranges: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
    existing: &[CompressionBlock],
    prune: Option<&str>,
    next: u64,
) -> Result<CompressionPlan, DcpError> {
    let mut plan =
        plan_compression_with_gain(session, history, ranges, spec, existing, prune, next, false)?;
    let mut graph = prepared_existing(existing)?;
    graph.extend(plan.blocks.iter().cloned());
    validate_block_graph(&graph)?;
    let by_id = graph.into_iter().map(|b| (b.id.clone(), b)).collect();
    for block in &mut plan.blocks {
        let mut facts = Vec::new();
        let mut legacy = Vec::new();
        for message in history.iter().filter(|m| block.members.contains(&m.id.0)) {
            if !message.text.is_empty() && oc_core::context_plan::message_protected(spec, message) {
                facts.push(serde_json::json!({"id":message.id.0,"role":if message.role==oc_core::session::Role::User {"user"} else {"assistant"},"text":message.text}));
            }
        }
        let mut summary = expand_block(&by_id, &block.id, 0, &mut Vec::new())?;
        for old in existing.iter().filter(|old| {
            plan.consumed_blocks.contains(&old.id)
                && old.members.iter().all(|id| block.members.contains(id))
        }) {
            if let Some(previous) = old.hot.as_ref().and_then(|h| h["protected"].as_array()) {
                for fact in previous {
                    let message = oc_core::session::Message {
                        id: oc_core::session::MessageId(
                            fact["id"].as_str().ok_or(DcpError::Storage)?.into(),
                        ),
                        role: match fact["role"].as_str() {
                            Some("user") => oc_core::session::Role::User,
                            Some("assistant") => oc_core::session::Role::Assistant,
                            _ => return Err(DcpError::Storage),
                        },
                        text: fact["text"].as_str().ok_or(DcpError::Storage)?.into(),
                    };
                    if oc_core::context_plan::message_protected(spec, &message)
                        && !facts.contains(fact)
                    {
                        summary.push_str(if message.role == oc_core::session::Role::User {
                            PROTECTED_USER_HEADING
                        } else {
                            PROTECTED_CONTENT_HEADING
                        });
                        summary.push_str(&message.text);
                        facts.push(fact.clone());
                    }
                }
            }
            for fact in select_legacy_protection(&legacy_protection(old)?, spec)? {
                if !legacy.contains(&fact) {
                    append_legacy_protection(&mut summary, std::slice::from_ref(&fact))?;
                    legacy.push(fact);
                }
            }
        }
        if summary.len() > NESTED_BYTES_CAP {
            return Err(DcpError::Impossible {
                reason: "selected protection exceeds summary budget".into(),
            });
        }
        block.summary = summary;
        block.hot = Some(
            serde_json::json!({"version":1,"active":true,"standalone":true,"protected":facts,"legacy_protected":legacy,"logs":[]}),
        );
    }
    Ok(plan)
}

/// Atomically publish a previously prepared plan and optional model-tool journal.
pub(crate) fn commit_compression(
    db: &crate::storage::Db,
    session: &str,
    plan: CompressionPlan,
    commit: Option<&CompressionCommitMetadata<'_>>,
) -> Result<CompressionReport, DcpError> {
    commit_compression_with_projection(db, session, plan, commit, &[], &[])
}

pub(crate) fn commit_compression_with_projection(
    db: &crate::storage::Db,
    session: &str,
    plan: CompressionPlan,
    commit: Option<&CompressionCommitMetadata<'_>>,
    hidden_calls: &[crate::storage::DcpCallKey],
    purged_calls: &[crate::storage::DcpCallKey],
) -> Result<CompressionReport, DcpError> {
    let (revision, prune, next) = db
        .compression_commit_snapshot(session)
        .map_err(map_storage)?;
    if next != plan.first_block_number {
        return Err(DcpError::Conflict);
    }
    let rows = plan
        .blocks
        .iter()
        .map(|block| crate::storage::CompressionBlockRow {
            id: block.id.clone(),
            session: block.session.clone(),
            topic: block.topic.clone(),
            summary: block.summary.clone(),
            start_msg: block.start_msg.clone(),
            end_msg: block.end_msg.clone(),
            members: block.members.clone(),
            hot: block.hot.as_ref().map(serde_json::Value::to_string),
        })
        .collect::<Vec<_>>();
    let tool = commit.map(|metadata| crate::storage::ToolOutcomeLogCommit {
        operation_id: metadata.operation_id,
        operation_state: metadata.operation_state,
        operation_output: metadata.operation_output,
        turn_id: metadata.turn_id,
        turn_log: metadata.turn_log,
        preference_updates: metadata.preference_updates,
    });
    let snapshot = db
        .commit_compression_plan(crate::storage::CompressionPlanCommit {
            session,
            blocks: &rows,
            consumed_blocks: &plan.consumed_blocks,
            expected_next: plan.first_block_number,
            expected_revision: plan.projection_revision.unwrap_or(revision),
            expected_prune: prune.as_deref(),
            hidden_calls,
            purged_calls,
            tool: tool.as_ref(),
            measurement: &plan.measurement,
        })
        .map_err(map_storage)?;
    Ok(CompressionReport {
        snapshot,
        blocks: plan.blocks,
        before_bytes: plan.before_bytes,
        after_bytes: plan.after_bytes,
        saved_tokens: plan.saved_tokens,
    })
}

/// Compatibility wrapper returning only committed block ids.
pub fn compress_ranges(
    db: &crate::storage::Db,
    session: &str,
    history: &[oc_core::session::Message],
    validated: &[ValidatedRange],
    spec: &oc_core::context_plan::ProtectedSpec,
) -> Result<Vec<String>, DcpError> {
    Ok(
        compress_ranges_atomic(db, session, history, validated, spec, None)?
            .blocks
            .into_iter()
            .map(|block| block.id)
            .collect(),
    )
}

fn block_from_row(row: crate::storage::CompressionBlockRow) -> Result<CompressionBlock, DcpError> {
    Ok(CompressionBlock {
        id: row.id,
        session: row.session,
        topic: row.topic,
        summary: row.summary,
        start_msg: row.start_msg,
        end_msg: row.end_msg,
        members: row.members,
        hot: row
            .hot
            .map(|raw| serde_json::from_str(&raw).map_err(|_| DcpError::Storage))
            .transpose()?,
    })
}

fn map_storage(error: crate::storage::StorageError) -> DcpError {
    if matches!(error, crate::storage::StorageError::CompressionConflict) {
        DcpError::Conflict
    } else {
        DcpError::Storage
    }
}

/// Append covered user messages verbatim (upstream-exact heading).
pub fn append_protected_user_messages(summary: &str, user_texts: &[String]) -> String {
    if user_texts.is_empty() {
        return summary.to_string();
    }
    let body: String = user_texts.iter().map(|text| format!("\n{text}")).collect();
    format!("{summary}{PROTECTED_USER_HEADING}{body}")
}

/// Append covered `<protect>` extracts verbatim (upstream-exact heading).
pub fn append_protected_tags(summary: &str, tag_texts: &[String]) -> String {
    if tag_texts.is_empty() {
        return summary.to_string();
    }
    let body: String = tag_texts.iter().map(|text| format!("\n{text}")).collect();
    format!("{summary}{PROTECTED_CONTENT_HEADING}{body}")
}

pub(crate) const PROTECTED_USER_HEADING: &str =
    "\n\nThe following user messages were sent in this conversation verbatim:";
pub(crate) const PROTECTED_CONTENT_HEADING: &str =
    "\n\nThe following protected prompt information was included in this conversation verbatim:";

/// Derived legacy working facts have no recoverable per-message identity. Their
/// runtime-authored headings are not fabricated original message references.
pub(crate) fn legacy_protection(
    block: &CompressionBlock,
) -> Result<Vec<serde_json::Value>, DcpError> {
    if let Some(value) = block.hot.as_ref().and_then(|h| h.get("legacy_protected")) {
        return value.as_array().cloned().ok_or(DcpError::Storage);
    }
    if block.hot.as_ref().is_some_and(|h| h["standalone"] == true) {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for (heading, role, other) in [
        (PROTECTED_USER_HEADING, "user", PROTECTED_CONTENT_HEADING),
        (PROTECTED_CONTENT_HEADING, "content", PROTECTED_USER_HEADING),
    ] {
        if let Some(start) = block.summary.find(heading) {
            let body = &block.summary[start + heading.len()..];
            let text = body.split(other).next().unwrap_or(body);
            if !text.is_empty() {
                result.push(serde_json::json!({"role":role,"text":text,"source_start":block.start_msg,"source_end":block.end_msg}));
            }
        }
    }
    Ok(result)
}

pub(crate) fn select_legacy_protection(
    facts: &[serde_json::Value],
    spec: &oc_core::context_plan::ProtectedSpec,
) -> Result<Vec<serde_json::Value>, DcpError> {
    let mut text_policy = spec.clone();
    text_policy.protected_message_ids.clear();
    let mut selected = Vec::new();
    for fact in facts {
        let role = match fact["role"].as_str() {
            Some("user") => oc_core::session::Role::User,
            Some("content") => oc_core::session::Role::Assistant,
            _ => return Err(DcpError::Storage),
        };
        let text = fact["text"].as_str().ok_or(DcpError::Storage)?;
        let message = oc_core::session::Message {
            id: oc_core::session::MessageId(String::new()),
            role,
            text: text.into(),
        };
        let start = fact["source_start"].as_str().ok_or(DcpError::Storage)?;
        let end = fact["source_end"].as_str().ok_or(DcpError::Storage)?;
        let seq = |id: &str| id.strip_prefix('m').and_then(|n| n.parse::<u64>().ok());
        let current_source = spec.protected_message_ids.iter().any(|id| {
            id == start || id == end || matches!((seq(start), seq(end), seq(id)), (Some(a),Some(b),Some(n)) if a<=n && n<=b)
        });
        if text_policy.protect_user_messages
            || current_source
            || oc_core::context_plan::message_protected(&text_policy, &message)
        {
            selected.push(fact.clone());
        }
    }
    Ok(selected)
}

pub(crate) fn append_legacy_protection(
    summary: &mut String,
    facts: &[serde_json::Value],
) -> Result<(), DcpError> {
    for fact in facts {
        let heading = match fact["role"].as_str() {
            Some("user") => PROTECTED_USER_HEADING,
            Some("content") => PROTECTED_CONTENT_HEADING,
            _ => return Err(DcpError::Storage),
        };
        let text = fact["text"].as_str().ok_or(DcpError::Storage)?;
        if !summary.contains(&format!("{heading}{text}")) {
            summary.push_str(heading);
            summary.push_str(text);
        }
    }
    Ok(())
}

/// Parsed block placeholder (`(bN)` or `{block_N}`) inside a summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPlaceholder {
    /// Raw matched text.
    pub raw: String,
    /// Numeric block id.
    pub block_id: String,
    /// Byte offset of the match.
    pub start: usize,
}

/// Parse `(bN)` / `{block_N}` references out of a summary.
pub fn parse_block_placeholders(summary: &str) -> Vec<BlockPlaceholder> {
    /// Match one placeholder at `i`: (prefix, open_len, closer).
    fn at(
        summary: &str,
        i: usize,
        prefix: &str,
        closer: char,
    ) -> Option<(BlockPlaceholder, usize)> {
        let rest = summary.get(i..)?.strip_prefix(prefix)?;
        let end = rest.find(closer)?;
        let inner = &rest[..end];
        if inner.is_empty() || !inner.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((
            BlockPlaceholder {
                raw: summary[i..i + prefix.len() + end + 1].to_string(),
                block_id: format!("b{inner:0>4}"),
                start: i,
            },
            i + prefix.len() + end + 1,
        ))
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < summary.len() {
        if let Some((placeholder, next)) =
            at(summary, i, "(b", ')').or_else(|| at(summary, i, "{block_", '}'))
        {
            out.push(placeholder);
            i = next;
        } else {
            i += 1;
        }
    }
    out
}

/// Expand a block's nested references with cycle/depth/byte limits.
///
/// Summaries link older blocks; expansion materializes full text for
/// effectiveness accounting. A complex nested block is never replaced by a
/// lossy stub: over-limit expansion is a visible error, not a silent cut.
pub fn expand_block(
    blocks: &std::collections::BTreeMap<String, CompressionBlock>,
    id: &str,
    depth: usize,
    seen: &mut Vec<String>,
) -> Result<String, DcpError> {
    if seen.contains(&id.to_string()) {
        return Err(DcpError::Cycle { id: id.to_string() });
    }
    if depth > NESTED_DEPTH_CAP {
        return Err(DcpError::NestedTooLarge {
            reason: "depth cap".to_string(),
        });
    }
    let block = blocks
        .get(id)
        .ok_or_else(|| DcpError::UnknownBlock { id: id.to_string() })?;
    if block.hot.as_ref().is_some_and(|h| h["standalone"] == true) {
        if block.summary.len() > NESTED_BYTES_CAP {
            return Err(DcpError::NestedTooLarge {
                reason: "byte cap".into(),
            });
        }
        return Ok(block.summary.clone());
    }
    if block.summary.len() > NESTED_BYTES_CAP {
        return Err(DcpError::NestedTooLarge {
            reason: "byte cap".to_string(),
        });
    }
    seen.push(id.to_string());
    let mut text = block.summary.clone();
    for placeholder in parse_block_placeholders(authored_summary(&block.summary)) {
        let nested = expand_block(blocks, &placeholder.block_id, depth + 1, seen)?;
        text = text.replacen(placeholder.raw.as_str(), nested.as_str(), 1);
        if text.len() > NESTED_BYTES_CAP {
            return Err(DcpError::NestedTooLarge {
                reason: "byte cap".to_string(),
            });
        }
    }
    seen.pop();
    Ok(text)
}

pub(crate) fn authored_summary(summary: &str) -> &str {
    let protected_start = [PROTECTED_USER_HEADING, PROTECTED_CONTENT_HEADING]
        .into_iter()
        .filter_map(|heading| summary.find(heading))
        .min()
        .unwrap_or(summary.len());
    &summary[..protected_start]
}

fn prepared_existing(blocks: &[CompressionBlock]) -> Result<Vec<CompressionBlock>, DcpError> {
    let by_id = blocks.iter().cloned().map(|b| (b.id.clone(), b)).collect();
    blocks
        .iter()
        .map(|block| {
            let mut prepared = block.clone();
            prepared.summary = expand_block(&by_id, &block.id, 0, &mut Vec::new())?;
            prepared.hot = Some(serde_json::json!({"standalone":true}));
            Ok(prepared)
        })
        .collect()
}

fn validate_block_graph(blocks: &[CompressionBlock]) -> Result<(), DcpError> {
    let by_id = blocks
        .iter()
        .map(|block| (block.id.clone(), block.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for id in by_id.keys() {
        expand_block(&by_id, id, 0, &mut Vec::new())?;
    }
    Ok(())
}

fn serialized_rows_len(rows: &[(String, String, String)]) -> Result<usize, DcpError> {
    serde_json::to_vec(rows)
        .map(|bytes| bytes.len())
        .map_err(|_| DcpError::Impossible {
            reason: "projection serialization failed".to_string(),
        })
}

/// Project full history through compression blocks and the prune mark.
///
/// Covered member messages collapse into one
/// `[compressed {id}] {summary}` system entry at the position of the
/// first covered message; uncovered messages pass through verbatim in
/// order. Summaries expand nested placeholders under cycle/depth/byte
/// guards. A prune mark drops the prefix
/// through the marked message id; an unknown mark id is ignored, never
/// applied blindly. History rows are never mutated; stale member ids
/// (compacted elsewhere) are skipped, never fatal.
pub fn project_history(
    history: &[(String, String, String)],
    blocks: &[CompressionBlock],
    prune_up_to: Option<&str>,
) -> Vec<(String, String)> {
    project_rows(history, blocks, prune_up_to)
        .into_iter()
        .map(|(_, role, text)| (role, text))
        .collect()
}

/// Projection retaining anchors for Responses wire journals.
pub(crate) fn project_rows(
    history: &[(String, String, String)],
    blocks: &[CompressionBlock],
    prune_up_to: Option<&str>,
) -> Vec<(String, String, String)> {
    // Compatibility projection for previously persisted invalid blocks. New
    // writes always use the strict planner below and can never reach fallback.
    project_rows_with_mode(history, blocks, prune_up_to, false)
        .expect("compatibility projection cannot fail")
}

fn project_rows_checked(
    history: &[(String, String, String)],
    blocks: &[CompressionBlock],
    prune_up_to: Option<&str>,
) -> Result<Vec<(String, String, String)>, DcpError> {
    project_rows_with_mode(history, blocks, prune_up_to, true)
}

fn project_rows_with_mode(
    history: &[(String, String, String)],
    blocks: &[CompressionBlock],
    prune_up_to: Option<&str>,
    strict: bool,
) -> Result<Vec<(String, String, String)>, DcpError> {
    let by_id: std::collections::BTreeMap<String, CompressionBlock> = blocks
        .iter()
        .map(|block| (block.id.clone(), block.clone()))
        .collect();
    let mut member_of: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for block in blocks {
        for member in &block.members {
            member_of
                .entry(member.as_str())
                .or_insert(block.id.as_str());
        }
    }
    let mut start = 0usize;
    if let Some(mark) = prune_up_to
        && let Some(pos) = history.iter().position(|(id, _, _)| id == mark)
    {
        start = pos + 1;
    }
    let mut out = Vec::new();
    let mut emitted: Vec<String> = Vec::new();
    for (id, role, text) in &history[start..] {
        match member_of.get(id.as_str()) {
            Some(block_id) if !emitted.contains(&block_id.to_string()) => {
                emitted.push(block_id.to_string());
                let summary = by_id
                    .get(*block_id)
                    .map(
                        |block| match expand_block(&by_id, block_id, 0, &mut Vec::new()) {
                            Ok(summary) => Ok(summary),
                            Err(error) if strict => Err(error),
                            Err(_) => Ok(block.summary.clone()),
                        },
                    )
                    .transpose()?
                    .unwrap_or_default();
                out.push((
                    block_id.to_string(),
                    "system".to_string(),
                    format!("[compressed {block_id}] {summary}"),
                ));
            }
            Some(_) => {}
            None => out.push((id.clone(), role.clone(), text.clone())),
        }
    }
    Ok(out)
}

/// Lowest member seq per block, ascending: summary placement input for
/// [`project_active_rows`] when blocks are not persisted yet.
pub fn member_positions(
    blocks: &[CompressionBlock],
    seqs: &std::collections::BTreeMap<String, i64>,
) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for block in blocks {
        let lowest = block
            .members
            .iter()
            .filter_map(|member| seqs.get(member).copied())
            .min();
        if let Some(seq) = lowest {
            out.push((block.id.clone(), seq));
        }
    }
    out.sort_by_key(|(_, seq)| *seq);
    out
}

/// Project an already-bounded active history with block summaries.
///
/// `active` holds only rows no block covers (in seq order, prune already
/// applied); `positions` gives each block's lowest in-window member seq
/// ([`crate::storage::Db::block_positions`]). The result is identical to
/// [`project_rows`] over the full history: each block summary is emitted at
/// the position of its first in-window member, and covered rows are never
/// materialised. Blocks without an in-window member emit nothing, matching
/// the prune behaviour of the full projection.
pub fn project_active_rows(
    active: &[(String, String, String)],
    blocks: &[CompressionBlock],
    positions: &[(String, i64)],
) -> Result<Vec<(String, String, String)>, DcpError> {
    let by_id: std::collections::BTreeMap<String, CompressionBlock> = blocks
        .iter()
        .map(|block| (block.id.clone(), block.clone()))
        .collect();
    // Position lookup by block id, ordered by first in-window member.
    let mut queue: Vec<(&str, i64)> = positions
        .iter()
        .map(|(id, seq)| (id.as_str(), *seq))
        .collect();
    queue.sort_by_key(|(_, seq)| *seq);
    // Covered rows are dropped here: their summary is placed by position.
    // A covered row whose block has no position is kept (fail open, never
    // drop a fact because placement metadata was missing).
    let placed: std::collections::BTreeSet<&str> = queue.iter().map(|(id, _)| *id).collect();
    let mut member_of: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for block in blocks {
        for member in &block.members {
            member_of
                .entry(member.as_str())
                .or_insert(block.id.as_str());
        }
    }
    let mut out = Vec::with_capacity(active.len() + queue.len());
    let mut next = 0usize;
    for (id, role, text) in active {
        if let Some(block_id) = member_of.get(id.as_str())
            && placed.contains(*block_id)
        {
            continue;
        }
        let seq = id
            .strip_prefix('m')
            .and_then(|rest| rest.parse::<i64>().ok())
            .unwrap_or(i64::MAX);
        while next < queue.len() && queue[next].1 < seq {
            let block_id = queue[next].0;
            let summary = match by_id.get(block_id) {
                Some(block) => expand_block(&by_id, block_id, 0, &mut Vec::new())
                    .unwrap_or_else(|_| block.summary.clone()),
                None => String::new(),
            };
            out.push((
                block_id.to_string(),
                "system".to_string(),
                format!("[compressed {block_id}] {summary}"),
            ));
            next += 1;
        }
        out.push((id.clone(), role.clone(), text.clone()));
    }
    while next < queue.len() {
        let block_id = queue[next].0;
        let summary = match by_id.get(block_id) {
            Some(block) => expand_block(&by_id, block_id, 0, &mut Vec::new())
                .unwrap_or_else(|_| block.summary.clone()),
            None => String::new(),
        };
        out.push((
            block_id.to_string(),
            "system".to_string(),
            format!("[compressed {block_id}] {summary}"),
        ));
        next += 1;
    }
    Ok(out)
}

/// Patch protection: every affected path is checked against protected globs.
///
/// `apply_patch` counts as a protected mutation equivalent: any violation
/// fails visibly before execution (the executor additionally enforces its
/// own policy). Returns the affected paths when clean.
pub fn check_patch_protected(
    patch_text: &str,
    protected_globs: &[String],
) -> Result<Vec<String>, DcpError> {
    let paths = crate::patch::affected_paths(patch_text).map_err(|e| DcpError::InvalidArgs {
        reason: e.to_string(),
    })?;
    let mut violations = Vec::new();
    for path in &paths {
        if protected_globs
            .iter()
            .any(|pattern| glob_match(pattern, path))
        {
            violations.push(path.clone());
        }
    }
    if violations.is_empty() {
        Ok(paths)
    } else {
        Err(DcpError::PatchProtected { paths: violations })
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    fn segment(pat: &[u8], text: &[u8]) -> bool {
        let (mut p, mut t) = (pat, text);
        let mut star: Option<&[u8]> = None;
        let mut mark: &[u8] = b"";
        loop {
            match (p.first(), t.first()) {
                (Some(b'*'), _) => {
                    star = Some(&p[1..]);
                    mark = t;
                    p = &p[1..];
                }
                (Some(b'?'), Some(_)) => {
                    p = &p[1..];
                    t = &t[1..];
                }
                (Some(a), Some(b)) if a == b => {
                    p = &p[1..];
                    t = &t[1..];
                }
                _ => {
                    if let Some(rest) = star {
                        if mark.is_empty() {
                            return false;
                        }
                        mark = &mark[1..];
                        t = mark;
                        p = rest;
                    } else {
                        return p.is_empty() && t.is_empty();
                    }
                }
            }
            if p.is_empty() && t.is_empty() {
                return true;
            }
            if p.is_empty() && star.is_none() {
                return false;
            }
        }
    }

    fn segments(pat: &[&str], path: &[&str]) -> bool {
        if pat.is_empty() {
            return path.is_empty();
        }
        if pat[0] == "**" {
            return (0..=path.len()).any(|i| segments(&pat[1..], &path[i..]));
        }
        if path.is_empty() {
            return false;
        }
        segment(pat[0].as_bytes(), path[0].as_bytes()) && segments(&pat[1..], &path[1..])
    }

    segments(
        &pattern.split('/').collect::<Vec<_>>(),
        &text.split('/').collect::<Vec<_>>(),
    )
}

/// Match a model tool argument path against DCP context-protection globs.
pub(crate) fn path_is_protected(patterns: &[String], path: &str) -> bool {
    patterns.iter().any(|pattern| glob_match(pattern, path))
}

/// Single-shot compress outcome: either a smaller projection or a visible
/// no-gain result that keeps the original. Callers must not loop on no-gain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompressOutcome {
    /// Projection shrank; use it.
    Compressed {
        /// Saved rough tokens.
        saved_tokens: u64,
    },
    /// No gain: original projection stands, try another selection instead.
    NoGain {
        /// Human reason.
        reason: String,
    },
}

/// Decide the outcome from measured savings (bounded, never a loop).
pub fn decide_outcome(saved_tokens: u64) -> CompressOutcome {
    if saved_tokens > 0 {
        CompressOutcome::Compressed { saved_tokens }
    } else {
        CompressOutcome::NoGain {
            reason: "summary does not shrink the projection".to_string(),
        }
    }
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
        let escaped_topic = "quoted \"topic\" with \\ path and 🌍";
        let mut canonical = args();
        canonical["topic"] = escaped_topic.into();
        let raw = canonical.to_string();
        assert_eq!(
            super::presentation_topic("compress", Some(&raw)).as_deref(),
            Some(escaped_topic)
        );
        assert_eq!(super::presentation_topic("bash", Some(&raw)), None);
        assert_eq!(
            super::presentation_topic("compress", Some("{\"topic\":\"unfinished")),
            None
        );
        assert_eq!(
            super::presentation_topic("compress", Some("{\"topic\":\"alone\"}")),
            None
        );
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

    use oc_core::context_plan::ProtectedSpec;
    use oc_core::session::{Message, MessageId, Role};
    use std::collections::BTreeMap;

    fn message(id: &str, role: Role, text: &str) -> Message {
        Message {
            id: MessageId(id.to_string()),
            role,
            text: text.to_string(),
        }
    }

    fn history5() -> Vec<Message> {
        vec![
            message("m0001", Role::User, "first question"),
            message("m0002", Role::Assistant, "first answer"),
            message(
                "m0003",
                Role::User,
                "second <protect>secret-token</protect> question",
            ),
            message("m0004", Role::Assistant, "second answer"),
            message("m0005", Role::User, "live tail"),
        ]
    }

    fn validated(topic: &str, start: &str, end: &str, summary: &str) -> super::ValidatedRange {
        super::ValidatedRange {
            topic: topic.to_string(),
            start_id: start.to_string(),
            end_id: end.to_string(),
            summary: summary.to_string(),
        }
    }

    #[test]
    fn dcp02_ids_stable_across_restart_and_stale_rejected() {
        let tmp = tempfile::tempdir().expect("temp");
        let root = tmp.path().join("data");
        let history = history5();
        let before = oc_core::context_plan::raw_checksum(&history);
        let spec = ProtectedSpec::default();
        let ranges = vec![validated("t", "m0001", "m0002", "early work")];
        let ids = {
            let db = crate::storage::Db::open(&root).expect("open");
            super::apply_dcp_schema(&db).expect("migrate");
            db.create_session("s-1").expect("session");
            super::compress_ranges(&db, "s-1", &history, &ranges, &spec).expect("compress")
        };
        assert_eq!(ids, vec!["b0001".to_string()]);
        // Restart: same dir reopens identical blocks; ids still resolve.
        {
            let db = crate::storage::Db::open(&root).expect("reopen");
            let blocks = super::load_blocks(&db, "s-1").expect("load");
            assert_eq!(blocks.len(), 1);
            assert_eq!(blocks[0].id, "b0001");
            assert_eq!(
                blocks[0].members,
                vec!["m0001".to_string(), "m0002".to_string()]
            );
            let planned = oc_core::context_plan::plan_ranges(
                &history,
                &[("m0001".to_string(), "m0002".to_string())],
            )
            .expect("still resolves");
            assert_eq!((planned[0].start, planned[0].end), (0, 1));
        }
        assert_eq!(oc_core::context_plan::raw_checksum(&history), before);
        // Stale / cross-session ids (valid elsewhere, unknown here).
        let other = vec![message("m0001", Role::User, "other session")];
        assert!(
            super::compress_ranges(
                &crate::storage::Db::open(&tmp.path().join("d2")).expect("db"),
                "s-2",
                &other,
                &[validated("t", "m0002", "m0002", "stale")],
                &spec,
            )
            .is_err()
        );
        // Unfinished tail coverage refused.
        let db = crate::storage::Db::open(&tmp.path().join("d3")).expect("db");
        super::apply_dcp_schema(&db).expect("migrate");
        db.create_session("s-3").expect("session");
        assert!(matches!(
            super::compress_ranges(
                &db,
                "s-3",
                &history,
                &[validated("t", "m0004", "m0005", "tail")],
                &spec
            ),
            Err(super::DcpError::InvalidArgs { .. })
        ));
    }

    /// DCP 3.2.0 `tests/v2-ids.test.ts` (d637981, AGPL-3.0-or-later) range/restart
    /// cases beyond 9999, translated to native `m{seq}`/`b{id}` IDs without a
    /// compact-ID migration.
    #[test]
    fn dcp320_ids_beyond_9999_keep_block_order_nesting_and_restart() {
        let tmp = tempfile::tempdir().expect("temp");
        let root = tmp.path().join("data");
        let history = vec![
            message("m9998", Role::User, "old question"),
            message("m9999", Role::Assistant, "old answer"),
            message("m10000", Role::User, "later question"),
            message("m10001", Role::Assistant, "later answer"),
            message("m10002", Role::User, "newest question"),
            message("m10003", Role::Assistant, "newest answer"),
            message("m10004", Role::User, "live tail"),
        ];
        let spec = ProtectedSpec::default();
        let db = crate::storage::Db::open(&root).expect("open");
        super::apply_dcp_schema(&db).expect("migrate");
        db.create_session("s").expect("session");
        rusqlite::Connection::open(root.join("oc.sqlite"))
            .expect("sqlite")
            .execute(
                "INSERT OR REPLACE INTO compression_identity(singleton,high_water) VALUES(1,9998)",
                [],
            )
            .expect("high water");
        let first = super::compress_ranges(
            &db,
            "s",
            &history,
            &[validated("a", "m9998", "m9999", "OLD_SUMMARY")],
            &spec,
        )
        .expect("first");
        let second = super::compress_ranges(
            &db,
            "s",
            &history,
            &[validated("b", "m10000", "m10001", "LATER_SUMMARY")],
            &spec,
        )
        .expect("second");
        assert_eq!(
            (first, second),
            (vec!["b9999".to_string()], vec!["b10000".to_string()])
        );
        let rows = |db: &crate::storage::Db| {
            let blocks = super::load_blocks(db, "s").expect("load");
            let positions = blocks
                .iter()
                .filter(|block| block.hot.as_ref().is_none_or(|hot| hot["active"] != false))
                .filter_map(|block| {
                    block.members.first().map(|member| {
                        (
                            block.id.clone(),
                            member[1..].parse::<i64>().expect("numeric seq"),
                        )
                    })
                })
                .collect::<Vec<_>>();
            let active = history
                .iter()
                .map(|m| (m.id.0.clone(), format!("{:?}", m.role), m.text.clone()))
                .collect::<Vec<_>>();
            super::project_active_rows(&active, &blocks, &positions)
                .expect("project")
                .into_iter()
                .map(|(id, _, text)| (id, text))
                .collect::<Vec<_>>()
        };
        let projected = rows(&db);
        let ids: Vec<_> = projected.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["b9999", "b10000", "m10002", "m10003", "m10004"]);
        assert!(projected[0].1.contains("OLD_SUMMARY") && projected[1].1.contains("LATER_SUMMARY"));
        // Restart: reopened IDs resolve and project identically past the boundary.
        drop(db);
        let db = crate::storage::Db::open(&root).expect("reopen");
        assert_eq!(rows(&db), projected);
        let planned = oc_core::context_plan::plan_ranges(
            &history,
            &[("m10000".to_string(), "m10003".to_string())],
        )
        .expect("numeric ids beyond 9999 resolve by position");
        assert_eq!((planned[0].start, planned[0].end), (2, 5));
        // Nested compression across the b9999→b10000 boundary owns every member.
        assert!(matches!(
            super::compress_ranges(
                &db,
                "s",
                &history,
                &[validated("x", "m9998", "m10003", "raw overlap")],
                &spec
            ),
            Err(super::DcpError::ExistingOverlap { .. })
        ));
        let existing = super::load_blocks(&db, "s").expect("load");
        let range = validated("c", "b9999", "m10003", "NESTED (b9999) (b10000) SUMMARY");
        let plan =
            super::plan_active_compression("s", &history, &[range], &spec, &existing, None, 10001)
                .expect("nested plan across the boundary");
        assert_eq!(plan.blocks.len(), 1);
        assert_eq!(plan.blocks[0].id, "b10001");
        let mut consumed = plan.consumed_blocks.clone();
        consumed.sort();
        assert_eq!(consumed, ["b10000", "b9999"]);
        assert!(
            plan.blocks[0].summary.contains("OLD_SUMMARY")
                && plan.blocks[0].summary.contains("LATER_SUMMARY")
        );
        // Text order puts b10000 and b10001 before b9999. As in the runtime
        // candidate graph, consumed blocks keep no members, so ownership does
        // not depend on the textual ID order across the boundary.
        let mut graph = existing.clone();
        for block in &mut graph {
            assert!(plan.consumed_blocks.contains(&block.id));
            block.members.clear();
        }
        graph.extend(plan.blocks.iter().cloned());
        graph.sort_by(|a, b| a.id.cmp(&b.id));
        let active = history
            .iter()
            .map(|m| (m.id.0.clone(), format!("{:?}", m.role), m.text.clone()))
            .collect::<Vec<_>>();
        let projected =
            super::project_active_rows(&active, &graph, &[("b10001".to_string(), 9998)])
                .expect("project nested");
        let ids: Vec<_> = projected.iter().map(|(id, ..)| id.as_str()).collect();
        assert_eq!(ids, ["b10001", "m10004"], "{projected:?}");
        drop(db);
    }

    #[test]
    fn projection_collapses_prunes_and_expands() {
        let block = |id: &str, summary: &str, members: &[&str]| super::CompressionBlock {
            hot: None,
            id: id.to_string(),
            session: "s".to_string(),
            topic: "t".to_string(),
            summary: summary.to_string(),
            start_msg: members.first().unwrap_or(&"").to_string(),
            end_msg: members.last().unwrap_or(&"").to_string(),
            members: members.iter().map(|m| m.to_string()).collect(),
        };
        let history = vec![
            ("m0001".to_string(), "user".to_string(), "first".to_string()),
            (
                "m0002".to_string(),
                "assistant".to_string(),
                "second".to_string(),
            ),
            ("m0003".to_string(), "user".to_string(), "third".to_string()),
            (
                "m0004".to_string(),
                "assistant".to_string(),
                "fourth".to_string(),
            ),
        ];
        // Collapse keeps position of first covered message; stale ids skip.
        let blocks = vec![
            block("b0001", "early work", &["m0001", "m0002", "gone"]),
            block("b0002", "wraps (b1)", &["m0003"]),
        ];
        let projected = super::project_history(&history, &blocks, None);
        assert_eq!(projected.len(), 3);
        assert_eq!(projected[0].0, "system");
        assert!(projected[0].1.contains("[compressed b0001]"));
        assert!(projected[0].1.contains("early work"));
        assert!(projected[1].1.contains("[compressed b0002]"));
        assert!(
            projected[1].1.contains("early work"),
            "nested placeholder expands"
        );
        assert_eq!(
            projected[2],
            ("assistant".to_string(), "fourth".to_string())
        );
        // Prune mark drops the prefix; unknown mark is ignored.
        let pruned = super::project_history(&history, &blocks, Some("m0002"));
        assert_eq!(pruned.len(), 2);
        assert!(pruned[0].1.contains("[compressed b0002]"));
        let ignored = super::project_history(&history, &blocks, Some("nope"));
        assert_eq!(ignored.len(), 3);
        // Empty blocks pass through verbatim.
        let plain = super::project_history(&history, &[], None);
        assert_eq!(plain.len(), 4);
    }

    #[test]
    fn dcp03_nested_placeholders_cycles_and_verbatim() {
        // Placeholder forms from the upstream fixture contract.
        let found = super::parse_block_placeholders("see (b2) and {block_12} done");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].block_id, "b0002");
        assert_eq!(found[1].block_id, "b0012");
        // Nested chain expands; cycle is a visible error, never a stub.
        let mut blocks = BTreeMap::new();
        let block = |id: &str, summary: &str| super::CompressionBlock {
            hot: None,
            id: id.to_string(),
            session: "s".to_string(),
            topic: "t".to_string(),
            summary: summary.to_string(),
            start_msg: "m0001".to_string(),
            end_msg: "m0001".to_string(),
            members: vec!["m0001".to_string()],
        };
        blocks.insert("b0001".to_string(), block("b0001", "base facts"));
        blocks.insert("b0002".to_string(), block("b0002", "wraps (b1) plus"));
        let expanded = super::expand_block(&blocks, "b0002", 0, &mut Vec::new()).expect("expand");
        assert!(expanded.contains("base facts"));
        blocks.insert("b0003".to_string(), block("b0003", "loop (b4)"));
        blocks.insert("b0004".to_string(), block("b0004", "loop (b3)"));
        assert!(matches!(
            super::expand_block(&blocks, "b0003", 0, &mut Vec::new()),
            Err(super::DcpError::Cycle { .. })
        ));
        // Protected user text + protect tags appended verbatim upstream-style.
        let mut history = history5();
        history[1].text = "verbose unprotected answer ".repeat(32);
        let tmp = tempfile::tempdir().expect("temp");
        let db = crate::storage::Db::open(&tmp.path().join("data")).expect("db");
        super::apply_dcp_schema(&db).expect("migrate");
        db.create_session("s-p").expect("session");
        let spec = ProtectedSpec {
            protect_user_messages: true,
            protect_tags: true,
            file_globs: vec![],
            ..ProtectedSpec::default()
        };
        super::compress_ranges(
            &db,
            "s-p",
            &history,
            &[validated("t", "m0001", "m0003", "work done")],
            &spec,
        )
        .expect("compress");
        let blocks = super::load_blocks(&db, "s-p").expect("load");
        let summary = &blocks[0].summary;
        assert!(
            summary
                .contains("The following user messages were sent in this conversation verbatim:")
        );
        assert!(summary.contains("first question"));
        assert!(summary.contains(
            "The following protected prompt information was included in this conversation verbatim:"
        ));
        assert!(summary.contains("secret-token"));
    }

    #[test]
    fn dcp04_patch_affected_paths_and_protection() {
        // Every affected path parses out: op paths plus rename targets.
        let patch = concat!(
            "*** Begin Patch\n",
            "*** Add File: new.txt\n+content\n",
            "*** Update File: old.txt\n",
            "*** Move to: renamed.txt\n",
            "@@\n-a\n+b\n",
            "*** Delete File: gone.txt\n",
            "*** End Patch\n",
        );
        assert_eq!(
            crate::patch::affected_paths(patch).expect("paths"),
            vec![
                "gone.txt".to_string(),
                "new.txt".to_string(),
                "old.txt".to_string(),
                "renamed.txt".to_string()
            ]
        );
        // Protected globs fail visibly with the violating paths listed.
        let err =
            super::check_patch_protected(patch, &["*.txt".to_string()]).expect_err("protected");
        match err {
            super::DcpError::PatchProtected { paths } => assert_eq!(paths.len(), 4),
            other => panic!("wrong error: {other:?}"),
        }
        assert!(super::check_patch_protected(patch, &["*.md".to_string()]).is_ok());
        // Oversized protected content makes compression visibly impossible.
        let big = "x".repeat(super::PROTECTED_BYTES_CAP + 1);
        let history = vec![
            message("m0001", Role::User, &big),
            message("m0002", Role::Assistant, "ok"),
            message("m0003", Role::User, "tail"),
        ];
        let tmp = tempfile::tempdir().expect("temp");
        let db = crate::storage::Db::open(&tmp.path().join("data")).expect("db");
        super::apply_dcp_schema(&db).expect("migrate");
        db.create_session("s").expect("session");
        let spec = ProtectedSpec {
            protect_user_messages: true,
            protect_tags: false,
            file_globs: vec![],
            ..ProtectedSpec::default()
        };
        assert!(matches!(
            super::compress_ranges(
                &db,
                "s",
                &history,
                &[validated("t", "m0001", "m0002", "s")],
                &spec
            ),
            Err(super::DcpError::Impossible { .. })
        ));
    }

    #[test]
    fn dcp09_effectiveness_and_no_gain_bound() {
        use oc_core::context_plan::{plan_ranges, project, serialized_size, total_saved};
        // Synthetic large closed span: 200 verbose messages + live tail.
        let mut history: Vec<Message> = (0..200)
            .map(|i| {
                let role = if i % 2 == 0 {
                    Role::User
                } else {
                    Role::Assistant
                };
                message(
                    &format!("m{i:04}"),
                    role,
                    &format!("verbose payload line {i} with required fact ALPHA-{i}"),
                )
            })
            .collect();
        history.push(message("m0200", Role::User, "live tail"));
        let full = project(&history, &[], None);
        let full_size = serialized_size(&full);
        let ranges =
            plan_ranges(&history, &[("m0000".to_string(), "m0199".to_string())]).expect("plan");
        let factful: Vec<(oc_core::context_plan::ResolvedRange, String, String)> = vec![(
            ranges[0].clone(),
            "bulk".to_string(),
            "ALPHA facts 0..199 preserved in compressed span".to_string(),
        )];
        let small = project(&history, &factful, None);
        assert!(serialized_size(&small) < full_size);
        assert!(total_saved(&small) > 0);
        let flat = serde_json::to_string(&small).expect("json");
        assert!(flat.contains("ALPHA facts"));
        assert_eq!(small.raw_checksum, full.raw_checksum);
        // No gain: a summary as large as the raw span keeps the original.
        let tiny = vec![
            message("m0000", Role::User, "hi"),
            message("m0001", Role::Assistant, "yo"),
            message("m0002", Role::User, "tail"),
        ];
        let ranges =
            plan_ranges(&tiny, &[("m0000".to_string(), "m0001".to_string())]).expect("plan");
        let huge_summary = "s".repeat(200);
        let candidate = project(
            &tiny,
            &[(ranges[0].clone(), "t".to_string(), huge_summary)],
            None,
        );
        let outcome = super::decide_outcome(total_saved(&candidate));
        assert!(matches!(outcome, super::CompressOutcome::NoGain { .. }));
        // Original stands: single-shot guard, no loop.
        let original = project(&tiny, &[], None);
        assert!(serialized_size(&original) < serialized_size(&candidate));
    }
}
