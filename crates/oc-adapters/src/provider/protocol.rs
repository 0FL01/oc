//! Finite native wire protocols and their chronological lowering (T53/R3, GO03).
//!
//! The initial privileged prompt is the lane's fixed developer input. A
//! `System`-role history item is a chronological operator update that applies
//! from its position onward; each protocol lowers it without silently passing
//! a raw mid-conversation `system` role: Responses uses `developer`, Chat an
//! escaped `<system-update>` user text in place, Messages its native system
//! update only when explicitly supported, otherwise the same lower-authority
//! fallback. Effort markers are kept in place only for a declared per-message
//! capability; otherwise they are stripped and the captured top-level effort
//! applies. Pinned OC2 reference: `packages/ai/src/effort-updates.ts`,
//! `protocols/shared.ts` (`wrapSystemUpdate`), `protocols/open-responses.ts`.
use std::borrow::Cow;

use super::{InputContent, InputItem, InputRole};

/// The three admitted native wires; dispatch is a plain enum, not a framework.
#[allow(dead_code)] // The Messages adapter lands in a later T53 slice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Protocol {
    #[default]
    Responses,
    Chat,
    Messages,
}

// Declared-capability consumer arrives with T53 compatibility facts (slice 3a).
#[cfg_attr(not(test), allow(dead_code))]
/// Chronological "reasoning effort changed here" marker; `None` is the model
/// default. `position` is the index in the lowered input before which it applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffortMarker {
    pub position: usize,
    pub effort: Option<String>,
    pub previous: Option<String>,
}

// Declared-capability consumer arrives with T53 compatibility facts (slice 3a).
#[cfg_attr(not(test), allow(dead_code))]
/// Responses `configuration_update` default when a marker returns to the model default.
const DEFAULT_EFFORT: &str = "medium";

/// Stable escaped fallback for chronological system text on routes without a
/// native mid-conversation system role. It stays visibly lower authority.
pub(crate) fn wrap_system_update(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<system-update>\n{escaped}\n</system-update>")
}

/// Lower chronological system items for `protocol`. `native_messages_system`
/// is the explicitly declared Messages capability; it is ignored elsewhere.
pub(crate) fn lower_chronological_system(
    protocol: Protocol,
    input: &[InputItem],
    native_messages_system: bool,
) -> Cow<'_, [InputItem]> {
    let chronological = |item: &InputItem| {
        matches!(
            item,
            InputItem::Message {
                role: InputRole::System,
                ..
            }
        )
    };
    if !input.iter().any(chronological) {
        return Cow::Borrowed(input);
    }
    Cow::Owned(
        input
            .iter()
            .map(|item| match item {
                InputItem::Message {
                    role: InputRole::System,
                    content,
                } => match protocol {
                    Protocol::Responses => InputItem::Message {
                        role: InputRole::Developer,
                        content: content.clone(),
                    },
                    Protocol::Messages if native_messages_system => item.clone(),
                    Protocol::Chat | Protocol::Messages => {
                        InputItem::message(InputRole::User, wrap_system_update(&text_of(content)))
                    }
                },
                other => other.clone(),
            })
            .collect(),
    )
}

fn text_of(content: &[InputContent]) -> String {
    content
        .iter()
        .filter_map(|part| match part {
            InputContent::InputText { text } | InputContent::OutputText { text } => {
                Some(text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// Declared-capability consumer arrives with T53 compatibility facts (slice 3a).
#[cfg_attr(not(test), allow(dead_code))]
/// Donor `resolveEffortUpdates`: with no markers, or when the last marker
/// disagrees with the requested effort (reverted/forked history), markers are
/// stripped and the current effort applies; otherwise the top-level effort is
/// frozen at the first marker's `previous` and markers are lowered in place.
pub(crate) fn resolve_effort_updates(
    markers: &[EffortMarker],
    current: Option<&str>,
) -> (bool, Option<String>) {
    match (markers.first(), markers.last()) {
        (Some(first), Some(last)) if last.effort.as_deref() == current => {
            (true, first.previous.clone())
        }
        _ => (false, current.map(str::to_owned)),
    }
}

// Declared-capability consumer arrives with T53 compatibility facts (slice 3a).
#[cfg_attr(not(test), allow(dead_code))]
/// Responses effort lowering. Without the declared capability (or with
/// automatic context management) markers are stripped and the captured
/// current effort applies. With it, consecutive markers coalesce (newest
/// wins) into `configuration_update` items and the top-level effort freezes.
pub(crate) fn lower_responses_effort(
    input: Vec<InputItem>,
    markers: &[EffortMarker],
    current: Option<&str>,
    supported: bool,
) -> (Vec<InputItem>, Option<String>) {
    if !supported {
        return (input, current.map(str::to_owned));
    }
    let (keep, top) = resolve_effort_updates(markers, current);
    if !keep {
        return (input, top);
    }
    let mut lowered = Vec::with_capacity(input.len() + markers.len());
    let mut pending = markers.iter().peekable();
    for index in 0..=input.len() {
        let mut latest = None;
        while let Some(marker) = pending.next_if(|marker| marker.position <= index) {
            latest = Some(marker);
        }
        if let Some(marker) = latest {
            lowered.push(InputItem::ProviderOutput(serde_json::json!({
                "type": "configuration_update",
                "reasoning": {"effort": marker.effort.as_deref().unwrap_or(DEFAULT_EFFORT)},
            })));
        }
        if let Some(item) = input.get(index) {
            lowered.push(item.clone());
        }
    }
    (lowered, top)
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
