//! Donor command routing, bound to the application's immutable generation.
use super::*;

pub(super) fn definition<'a>(
    c: &'a Composition,
    invocation: Option<&str>,
) -> Option<&'a crate::defs::CommandDef> {
    let id = invocation?.strip_prefix('/')?.split_whitespace().next()?;
    c.command_defs.get(id)
}

pub(super) fn inline_eligible(c: &Composition, command: Option<&str>, agent: &str) -> bool {
    command
        .and_then(|id| c.command_defs.get(id))
        .is_some_and(|def| {
            def.agent.as_deref().is_none_or(|id| id == agent)
                && def.subagent.or(def.subtask) == Some(false)
        })
}

pub(super) fn restore_inline(
    c: &Composition,
    fallback: &Effective,
    agent: Option<&str>,
    command: &str,
    parents: &[String],
) -> Result<Effective, CoreError> {
    let agent = agent.ok_or_else(|| app_error("command agent unavailable"))?;
    if !inline_eligible(c, Some(command), agent) {
        let mut unavailable = fallback.clone();
        unavailable.retain_unavailable_agent(c, agent);
        return Ok(unavailable);
    }
    let mut selected = fallback.clone();
    selected.set_agent_inner(c, agent, true)?;
    selected.inline_command = Some(command.into());
    selected.command_parents = parents.to_vec();
    Ok(selected)
}

pub(super) fn prepare(
    c: &Composition,
    parent: &Effective,
    def: &crate::defs::CommandDef,
) -> Result<(Effective, crate::runtime::NativeCommand), CoreError> {
    let id = def
        .agent
        .as_deref()
        .or(parent.agent_id.as_deref())
        .ok_or_else(|| app_error("command agent unavailable"))?;
    let agent = c
        .agents
        .get(id)
        .ok_or_else(|| app_error("command agent unavailable"))?;
    let background = def
        .subagent
        .or(def.subtask)
        .unwrap_or(agent.mode.as_deref() == Some("subagent"));
    // Agent first, then the explicit command model, as pinned switchAgent -> switchModel.
    let mut selected = parent.clone();
    selected.set_agent_inner(c, id, true)?;
    if let Some(raw) = def.model.as_ref() {
        let (model, variant) = crate::defs::command_model(Some(raw)).map_err(app_error)?;
        let raw = model
            .as_deref()
            .ok_or_else(|| app_error("command model unavailable"))?;
        let raw = if variant.is_some() {
            raw.split_once('#').map_or(raw, |(id, _)| id)
        } else {
            raw
        };
        let resolved =
            crate::runtime::resolve_subagent_model(&c.catalog, raw).map_err(app_error)?;
        selected.model_id = resolved.id;
        selected.variant = variant.or(resolved.variant);
    }
    selected.inline_command = (!agent.primary_capable() && !background).then(|| def.id.clone());
    if selected.inline_command.is_some() {
        if let Some(parent_agent) = &parent.agent_id
            && parent_agent != id
            && !selected.command_parents.contains(parent_agent)
        {
            selected.command_parents.push(parent_agent.clone());
        }
    } else {
        selected.command_parents.clear();
    }
    if !background {
        selected.admit_selection(c)?;
    }
    // Background is eligible for primary profiles too, but never widens the model-tool catalog.
    let child = background.then(|| crate::runtime::CommandChild {
        agent: id.into(),
        model_id: selected.model_id.clone(),
        variant: selected.variant.clone(),
        description: def.description.clone(),
    });
    let selected = if background { parent.clone() } else { selected };
    Ok((
        selected,
        crate::runtime::NativeCommand {
            id: def.id.clone(),
            child,
            selection: None,
        },
    ))
}

pub(super) fn permission_rules(
    c: &Composition,
    effective: &Effective,
    agent: Option<&crate::defs::AgentDef>,
) -> crate::permissions::PermissionRules {
    let mut rules = agent
        .map(|a| a.permission_rules.clone())
        .unwrap_or_default();
    let permissions = agent.map(|a| a.permissions.clone()).unwrap_or_default();
    for parent in &effective.command_parents {
        if let Some(parent) = c.agents.get(parent) {
            rules.narrow(&permissions, &parent.permissions, &parent.permission_rules);
        }
    }
    rules
}
