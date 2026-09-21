//! Core context plan for T17 (DCP01).
//!
//! Immutable raw transcript plus a separately computed outbound projection:
//! validated id ranges resolve against stable message ids, summaries replace
//! covered spans in the projection only, and a checksum pins the raw history
//! so any accidental mutation is detectable. Token counts are rough local
//! estimates for planning (the provider bills); projection copies only
//! role/text/summary fields, never config or credential objects.

use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::session::{Message, MessageId};
/// Rough token estimate: 4 chars per token, minimum 1 per message.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64 / 4).max(1)
}

/// Stable checksum over the raw transcript (id + role + text order).
pub fn raw_checksum(history: &[Message]) -> String {
    let mut hasher = Sha256::new();
    for message in history {
        hasher.update(message.id.0.as_bytes());
        hasher.update([role_byte(&message.role)]);
        hasher.update(message.text.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn role_byte(role: &crate::session::Role) -> u8 {
    match role {
        crate::session::Role::User => b'U',
        crate::session::Role::Assistant => b'A',
    }
}

/// Plan errors: unknown ids, inverted/empty ranges, overlaps.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// Referenced message id does not exist.
    #[error("unknown message {id}")]
    UnknownMessage {
        /// Missing id.
        id: String,
    },
    /// Range is empty or inverted.
    #[error("invalid range {start}..{end}")]
    InvalidRange {
        /// Range start id.
        start: String,
        /// Range end id.
        end: String,
    },
    /// Ranges overlap or touch (kept disjoint for stable accounting).
    #[error("overlapping ranges")]
    Overlap,
}

/// One resolved id range: inclusive indices into the history slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRange {
    /// Inclusive start index.
    pub start: usize,
    /// Inclusive end index.
    pub end: usize,
    /// Start message id (for durable references).
    pub start_id: MessageId,
    /// End message id (for durable references).
    pub end_id: MessageId,
}

/// Resolve `(start_id, end_id)` pairs against stable history ids.
///
/// Ranges must be non-empty, ordered, pairwise disjoint, and fully inside
/// the transcript. Returned in ascending order.
pub fn plan_ranges(
    history: &[Message],
    specs: &[(String, String)],
) -> Result<Vec<ResolvedRange>, PlanError> {
    let mut ranges = Vec::with_capacity(specs.len());
    for (start, end) in specs {
        let start_idx = history
            .iter()
            .position(|m| m.id.0 == *start)
            .ok_or_else(|| PlanError::UnknownMessage { id: start.clone() })?;
        let end_idx = history
            .iter()
            .position(|m| m.id.0 == *end)
            .ok_or_else(|| PlanError::UnknownMessage { id: end.clone() })?;
        if start_idx > end_idx {
            return Err(PlanError::InvalidRange {
                start: start.clone(),
                end: end.clone(),
            });
        }
        ranges.push(ResolvedRange {
            start: start_idx,
            end: end_idx,
            start_id: MessageId(start.clone()),
            end_id: MessageId(end.clone()),
        });
    }
    ranges.sort_by_key(|r| (r.start, r.end));
    for pair in ranges.windows(2) {
        if pair[1].start <= pair[0].end {
            return Err(PlanError::Overlap);
        }
    }
    Ok(ranges)
}

/// One outbound projection block: retained message or replacement summary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ProjectedBlock {
    /// Retained transcript message (reference to history content).
    Retained {
        /// Message id.
        id: MessageId,
        /// Role label (`user`/`assistant`).
        role: &'static str,
        /// Message text.
        text: String,
    },
    /// Replacement summary for a compressed range.
    Summary {
        /// Range topic.
        topic: String,
        /// Model-authored summary.
        summary: String,
        /// Covered message count.
        messages: usize,
        /// Rough saved tokens vs. covered raw text.
        saved_tokens: u64,
    },
}

/// Outbound projection: summaries plus retained messages, with the raw
/// checksum it was computed from. The input slice is never mutated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Projection {
    /// Ordered outbound blocks.
    pub blocks: Vec<ProjectedBlock>,
    /// Checksum of the raw transcript at projection time.
    pub raw_checksum: String,
    /// Estimated outbound tokens.
    pub estimated_tokens: u64,
}

/// Project summaries over a transcript: covered spans collapse into summary
/// blocks; everything else (including pruned-away prefixes, handled by
/// skipping them) passes through. Prune drops a leading prefix of the
/// transcript from the outbound view; raw history keeps it.
pub fn project(
    history: &[Message],
    summaries: &[(ResolvedRange, String, String)],
    prune_before: Option<usize>,
) -> Projection {
    let skip = prune_before.unwrap_or(0);
    let mut blocks = Vec::new();
    let mut estimated_tokens = 0u64;
    let mut covered_until = skip.saturating_sub(1);
    // Summaries arrive ascending (plan_ranges order); enforce defensively.
    let mut ordered: Vec<&(ResolvedRange, String, String)> = summaries.iter().collect();
    ordered.sort_by_key(|(range, _, _)| (range.start, range.end));
    for (range, topic, summary) in ordered {
        for message in history.iter().take(range.start).skip(covered_until + 1) {
            push_retained(&mut blocks, &mut estimated_tokens, message);
        }
        let end = range.end.min(history.len().saturating_sub(1));
        let covered: Vec<&Message> = if range.start < history.len() && range.start <= end {
            history[range.start..=end].iter().collect()
        } else {
            Vec::new()
        };
        let raw_tokens: u64 = covered.iter().map(|m| estimate_tokens(&m.text)).sum();
        let summary_tokens = estimate_tokens(topic) + estimate_tokens(summary);
        estimated_tokens += summary_tokens;
        blocks.push(ProjectedBlock::Summary {
            topic: topic.clone(),
            summary: summary.clone(),
            messages: covered.len(),
            saved_tokens: raw_tokens.saturating_sub(summary_tokens),
        });
        covered_until = range.end;
    }
    for message in history.iter().skip(covered_until + 1) {
        push_retained(&mut blocks, &mut estimated_tokens, message);
    }
    Projection {
        blocks,
        raw_checksum: raw_checksum(history),
        estimated_tokens,
    }
}

fn push_retained(blocks: &mut Vec<ProjectedBlock>, tokens: &mut u64, message: &Message) {
    *tokens += estimate_tokens(&message.text);
    blocks.push(ProjectedBlock::Retained {
        id: message.id.clone(),
        role: match message.role {
            crate::session::Role::User => "user",
            crate::session::Role::Assistant => "assistant",
        },
        text: message.text.clone(),
    });
}

/// Total rough saved tokens across summary blocks (effectiveness signal).
pub fn total_saved(projection: &Projection) -> u64 {
    projection
        .blocks
        .iter()
        .map(|block| match block {
            ProjectedBlock::Summary { saved_tokens, .. } => *saved_tokens,
            ProjectedBlock::Retained { .. } => 0,
        })
        .sum()
}

/// Serialized outbound size in bytes (effectiveness signal).
pub fn serialized_size(projection: &Projection) -> usize {
    serde_json::to_string(projection)
        .map(|s| s.len())
        .unwrap_or(usize::MAX)
}

/// Protection specification for range planning.
///
/// Until tool call/result parts land in the message model, protection
/// applies to user messages, `<protect>` tag spans and file-glob mentions;
/// tool-name/file-path part hooks (`isToolNameProtected`,
/// `isFilePathProtected`, `getFilePathsFromParameters` upstream) attach at
/// the turn-loop layer (documented deferral, exact names referenced).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProtectedSpec {
    /// Keep user messages verbatim (appended to summaries, never dropped).
    pub protect_user_messages: bool,
    /// Honor `<protect>` tag spans.
    pub protect_tags: bool,
    /// File glob patterns matched against whitespace-delimited text tokens.
    pub file_globs: Vec<String>,
    /// Stable message IDs retained verbatim by runtime turn protection.
    pub protected_message_ids: std::collections::BTreeSet<String>,
}

/// True when a message carries protected content under `spec`.
pub fn message_protected(spec: &ProtectedSpec, message: &Message) -> bool {
    if spec.protected_message_ids.contains(&message.id.0) {
        return true;
    }
    if spec.protect_user_messages && matches!(message.role, crate::session::Role::User) {
        return true;
    }
    if spec.protect_tags && extract_protect_tags(&message.text).next().is_some() {
        return true;
    }
    if !spec.file_globs.is_empty() {
        for token in message.text.split_whitespace() {
            if spec
                .file_globs
                .iter()
                .any(|pattern| glob_match(pattern, token))
            {
                return true;
            }
        }
    }
    false
}

/// Iterate `<protect>…</protect>` spans (case-insensitive, non-greedy).
pub fn extract_protect_tags(text: &str) -> impl Iterator<Item = &str> {
    let lower = text.to_lowercase();
    let mut spans = Vec::new();
    let mut cursor = 0;
    while let Some(open) = lower[cursor..].find("<protect>") {
        let start = cursor + open + "<protect>".len();
        let Some(close) = lower[start..].find("</protect>") else {
            break;
        };
        let body = text[start..start + close].trim();
        if !body.is_empty() {
            spans.push(body);
        }
        cursor = start + close + "</protect>".len();
    }
    spans.into_iter()
}

/// Minimal glob matcher (`*`/`?` segments; `**` crosses slashes).
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

/// Plan errors extended with unfinished-tail protection.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProtectedPlanError {
    /// Underlying range plan failure.
    #[error("invalid range plan")]
    Invalid,
    /// Range covers the live tail message (unfinished batch lives there
    /// until the turn loop exposes batch state; narrow safe rule).
    #[error("range covers the unfinished tail")]
    UnfinishedTail,
}

/// Plan ranges with tail protection: disjoint ordered spans that never cover
/// the live tail message.
///
/// Protected content never blocks planning (upstream appends it verbatim via
/// [`covered_protected`]); only the unfinished tail is refused.
pub fn plan_ranges_protected(
    history: &[Message],
    specs: &[(String, String)],
) -> Result<Vec<ResolvedRange>, ProtectedPlanError> {
    let ranges = plan_ranges(history, specs).map_err(|_| ProtectedPlanError::Invalid)?;
    if let Some(last) = history.last() {
        let tail = last.id.0.clone();
        for range in &ranges {
            let covers_tail = history
                .iter()
                .position(|m| m.id.0 == tail)
                .is_some_and(|tail_idx| range.start <= tail_idx && tail_idx <= range.end);
            if covers_tail {
                return Err(ProtectedPlanError::UnfinishedTail);
            }
        }
    }
    Ok(ranges)
}

/// Protected messages covered by planned ranges (verbatim-appended at
/// compress time, never dropped).
pub fn covered_protected<'a>(
    history: &'a [Message],
    ranges: &[ResolvedRange],
    spec: &ProtectedSpec,
) -> Vec<&'a Message> {
    let mut out = Vec::new();
    for range in ranges {
        for message in history.iter().take(range.end + 1).skip(range.start) {
            if message_protected(spec, message) {
                out.push(message);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{estimate_tokens, plan_ranges, project, raw_checksum};
    use crate::session::{Message, MessageId, Role};

    fn history() -> Vec<Message> {
        vec![
            Message {
                id: MessageId("m0001".to_string()),
                role: Role::User,
                text: "first".to_string(),
            },
            Message {
                id: MessageId("m0002".to_string()),
                role: Role::Assistant,
                text: "second".to_string(),
            },
            Message {
                id: MessageId("m0003".to_string()),
                role: Role::User,
                text: "third".to_string(),
            },
        ]
    }

    #[test]
    fn checksum_stable_and_sensitive() {
        let history = history();
        let before = raw_checksum(&history);
        assert_eq!(raw_checksum(&history), before);
        let mut changed = history.clone();
        changed[1].text.push('!');
        assert_ne!(raw_checksum(&changed), before);
    }

    #[test]
    fn ranges_validate_existence_order_disjointness() {
        let history = history();
        assert!(plan_ranges(&history, &[("m0009".to_string(), "m0001".to_string())]).is_err());
        assert!(plan_ranges(&history, &[("m0002".to_string(), "m0001".to_string())]).is_err());
        let ok =
            plan_ranges(&history, &[("m0001".to_string(), "m0002".to_string())]).expect("range");
        assert_eq!((ok[0].start, ok[0].end), (0, 1));
        assert!(
            plan_ranges(
                &history,
                &[
                    ("m0001".to_string(), "m0002".to_string()),
                    ("m0002".to_string(), "m0003".to_string())
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn projection_replaces_without_mutating_raw() {
        let history = history();
        let before = raw_checksum(&history);
        let ranges =
            plan_ranges(&history, &[("m0001".to_string(), "m0002".to_string())]).expect("plan");
        let projection = project(
            &history,
            &[(
                ranges[0].clone(),
                "topic".to_string(),
                "summary text".to_string(),
            )],
            None,
        );
        assert_eq!(projection.blocks.len(), 2);
        assert_eq!(raw_checksum(&history), before);
        assert_eq!(estimate_tokens("abcd"), 1);
    }
}
