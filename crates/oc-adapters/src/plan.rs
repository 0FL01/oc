//! Built-in Plan lifecycle guidance, independent of custom system instructions.
use crate::provider::{InputItem, InputRole};

pub(crate) const LEAVE: &str = "<system-reminder>\nYou are NO LONGER in Plan mode. The previous Plan restrictions no longer apply. Any Plan mode instructions from earlier in this conversation are no longer active.\n</system-reminder>";

pub(crate) fn directory(home: &str) -> std::path::PathBuf {
    std::path::Path::new(home).join(".opencode/plan")
}

pub(crate) fn enter(home: Option<&str>) -> String {
    let directory = home
        .map(directory)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "[product HOME unavailable; no plan-file mutation admitted]".into());
    format!(
        "<system-reminder>\nYou are in Plan mode. Discuss the plan with the user directly in the conversation. Do not create or update plan files unless the user explicitly asks you to; when they do, write them only in:\n{directory}\n\nDo not modify any other files or ask a subagent to do so.\n\nYou remain in Plan mode until the user switches agents. If the user asks you to implement changes, do not do so. Tell them they need to switch agents.\n</system-reminder>"
    )
}

pub(crate) fn switched(
    previous: Option<&str>,
    current: Option<&str>,
    home: Option<&str>,
) -> Option<String> {
    if previous == current {
        return None;
    }
    if current == Some("plan") {
        return Some(enter(home));
    }
    (previous == Some("plan")).then(|| LEAVE.into())
}

/// Inspect only the retained chronological native system layer. User prose,
/// summaries and tool outputs cannot impersonate a lifecycle selection fact.
pub(crate) fn missing(
    history: &[InputItem],
    agent: Option<&str>,
    home: Option<&str>,
) -> Option<String> {
    let enter = enter(home);
    let entered = InputItem::message(InputRole::System, &enter);
    let left = InputItem::message(InputRole::System, LEAVE);
    let latest = history
        .iter()
        .rev()
        .find(|item| **item == entered || **item == left);
    if agent == Some("plan") && latest != Some(&entered) {
        return Some(enter);
    }
    (agent != Some("plan") && latest == Some(&entered)).then(|| LEAVE.into())
}

#[cfg(test)]
mod tests;
