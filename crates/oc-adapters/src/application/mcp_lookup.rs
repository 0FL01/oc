//! Read-only root-session admission for explicit caller-only MCP lookups.
use super::*;
use oc_core::queries::{McpLookup, McpLookupError, McpLookupOp};

pub(super) fn admit(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
    query: &McpLookup,
) -> Result<(), McpLookupError> {
    let server = runtime.mcp_lookup_server(query)?;
    crate::mcp_lookup::validate_operation(&query.operation)?;
    runtime
        .open_session(&query.session.0)
        .map_err(|error| match error {
            RuntimeError::SessionNotFound | RuntimeError::LocationMismatch { .. } => {
                McpLookupError::SessionMismatch
            }
            _ => McpLookupError::NativeFailure,
        })?;
    let meta = db
        .session_meta(&query.session.0)
        .map_err(|_| McpLookupError::NativeFailure)?;
    if meta.parent_id.is_some() {
        return Err(McpLookupError::ChildSessionUnsupported);
    }
    let selected = selection::for_turn(db, composition, effective, &query.session.0)
        .map_err(|_| McpLookupError::NativeFailure)?;
    let agent = selected
        .agent_id
        .as_ref()
        .and_then(|id| composition.agents.get(id))
        .ok_or(McpLookupError::NativeFailure)?;
    let base = &composition.generation.permissions;
    let mut rules = composition.generation.permission_rules.clone();
    rules.narrow(base, &agent.permissions, &agent.permission_rules);
    let policy = crate::runtime::RuntimePolicy::with_rules(base, &rules);
    let identity = serde_json::to_string(&[
        server.as_str(),
        query.operation.method(),
        query.operation.target(),
    ])
    .map_err(|_| McpLookupError::InvalidArguments)?;
    check(policy.effect("mcp_lookup", &identity))?;
    if !query.operation.is_catalog() {
        let resource = match &query.operation {
            McpLookupOp::ReadResource { uri } => uri.as_str(),
            _ => identity.as_str(),
        };
        check(policy.effect("read", resource))?;
    }
    Ok(())
}
fn check(effect: crate::config::Permission) -> Result<(), McpLookupError> {
    match effect {
        crate::config::Permission::Allow => Ok(()),
        crate::config::Permission::Ask => Err(McpLookupError::ApprovalRequired),
        crate::config::Permission::Deny => Err(McpLookupError::PermissionDenied),
    }
}
