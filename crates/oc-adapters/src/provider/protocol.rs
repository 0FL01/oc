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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Protocol {
    #[default]
    Responses,
    Chat,
    Messages,
}

/// Chronological "reasoning effort changed here" marker; `None` is the model
/// default. `position` is the index in the lowered input before which it applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EffortMarker {
    pub position: usize,
    pub effort: Option<String>,
    pub previous: Option<String>,
}

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

/// Explicit metadata only: no model-name inference for either native extension.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Chronology {
    pub effort: bool,
    pub system: bool,
}

pub(crate) fn lower_effort(
    protocol: Protocol,
    input: &[InputItem],
    current: Option<&str>,
    supported: bool,
) -> (Vec<InputItem>, Option<String>) {
    let mut plain = Vec::with_capacity(input.len());
    let mut markers = Vec::new();
    for item in input {
        if let InputItem::EffortUpdate {
            effort, previous, ..
        } = item
        {
            markers.push(EffortMarker {
                position: plain.len(),
                effort: effort.clone(),
                previous: previous.clone(),
            });
        } else {
            plain.push(item.clone());
        }
    }
    let supported = supported && protocol != Protocol::Chat;
    let (mut lowered, top) = lower_responses_effort(plain, &markers, current, supported);
    if protocol == Protocol::Messages {
        for item in &mut lowered {
            if let InputItem::ProviderOutput(value) = item
                && value["type"] == "configuration_update"
            {
                *value = serde_json::json!({"type":"messages_effort_update", "effort":value["reasoning"]["effort"]});
            }
        }
    }
    (lowered, top)
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
