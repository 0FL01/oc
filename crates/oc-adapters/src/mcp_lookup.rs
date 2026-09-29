//! Bounded prompt/resource wire facts shared by the existing two MCP clients.
//! No implicit fetch, SDK all-pages cache fallback, modern retry, or projection.
use crate::mcp_remote::{
    CLOSE_TIMEOUT, DESCRIPTION_BYTES_CAP, MAX_LIST_PAGES, ORIGINAL_NAME_BYTES_CAP,
    RESULT_TEXT_BYTES_CAP, SCHEMA_BYTES_CAP, TOOLS_CAP,
};
use base64::Engine as _;
use oc_core::queries::*;
use rmcp::model::{self, ClientRequest, ServerResult};
use std::{collections::HashSet, sync::atomic::AtomicBool, time::Duration};

type Peer = rmcp::service::Peer<rmcp::service::RoleClient>;

pub(crate) fn validate_operation(op: &McpLookupOp) -> Result<(), McpLookupError> {
    if !op.is_catalog()
        && (op.target().is_empty()
            || op.target().contains('\0')
            || op.target().len() > DESCRIPTION_BYTES_CAP)
    {
        return Err(McpLookupError::InvalidArguments);
    }
    if let McpLookupOp::GetPrompt { name, arguments } = op
        && (name.len() > ORIGINAL_NAME_BYTES_CAP
            || arguments.len() > TOOLS_CAP
            || arguments.keys().any(|key| !identity(key))
            || serde_json::to_vec(arguments)
                .map_err(|_| McpLookupError::InvalidArguments)?
                .len()
                > SCHEMA_BYTES_CAP)
    {
        return Err(McpLookupError::InvalidArguments);
    }
    Ok(())
}

pub(crate) async fn lookup(
    peer: &Peer,
    op: &McpLookupOp,
    catalog: Duration,
    execution: Duration,
    // Body redaction includes argv; exact identities use protected config/env
    // values, not ordinary command arguments.
    (secrets, identity_secrets): (&[String], &[String]),
    cancel: &AtomicBool,
    dispatched: Option<&AtomicBool>,
) -> Result<McpLookupData, McpLookupError> {
    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        return Err(McpLookupError::Cancelled);
    }
    validate_operation(op)?;
    // Caller identities are echoed in typed replies too; protecting only
    // server-supplied names/URIs would still publish known protected values.
    if !op.is_catalog() {
        safe_identity(op.target(), identity_secrets)?;
        if let McpLookupOp::GetPrompt { arguments, .. } = op {
            for key in arguments.keys() {
                safe_identity(key, identity_secrets)?;
            }
        }
    }
    let available = peer
        .peer_info()
        .map(|info| match op {
            McpLookupOp::ListPrompts | McpLookupOp::GetPrompt { .. } => {
                info.capabilities.prompts.is_some()
            }
            _ => info.capabilities.resources.is_some(),
        })
        .ok_or(McpLookupError::Unavailable)?;
    if !available {
        return match op {
            McpLookupOp::ListPrompts => Ok(McpLookupData::Prompts {
                available: false,
                items: vec![],
            }),
            McpLookupOp::ListResources => Ok(McpLookupData::Resources {
                available: false,
                items: vec![],
            }),
            McpLookupOp::ListResourceTemplates => Ok(McpLookupData::ResourceTemplates {
                available: false,
                items: vec![],
            }),
            _ => Err(McpLookupError::UnsupportedCapability),
        };
    }
    let deadline = tokio::time::Instant::now() + if op.is_catalog() { catalog } else { execution };
    if !op.is_catalog() {
        let request = match op {
            McpLookupOp::GetPrompt { name, arguments } => {
                let mut params = model::GetPromptRequestParams::new(name.clone());
                params.arguments = Some(
                    arguments
                        .iter()
                        .map(|(key, value)| (key.clone(), serde_json::Value::String(value.clone())))
                        .collect(),
                );
                ClientRequest::GetPromptRequest(model::GetPromptRequest::new(params))
            }
            McpLookupOp::ReadResource { uri } => ClientRequest::ReadResourceRequest(
                model::ReadResourceRequest::new(model::ReadResourceRequestParams::new(uri.clone())),
            ),
            _ => unreachable!(),
        };
        let result = once(peer, request, deadline, cancel, dispatched).await?;
        let value = bounded_value(&result)?;
        let data = match (op, result) {
            (McpLookupOp::GetPrompt { name, arguments }, ServerResult::GetPromptResult(_)) => {
                let mut messages: Vec<McpPromptMessage> =
                    serde_json::from_value(value["messages"].clone())
                        .map_err(|_| McpLookupError::UnsupportedResult)?;
                if messages.len() > TOOLS_CAP {
                    return Err(McpLookupError::BodyLimit);
                }
                for message in &mut messages {
                    redact_content(&mut message.content, (secrets, identity_secrets))?;
                }
                McpLookupData::Prompt {
                    name: name.clone(),
                    arguments: arguments
                        .iter()
                        .map(|(key, value)| {
                            (key.clone(), crate::mcp_result::redact(value, secrets))
                        })
                        .collect(),
                    description: value
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|v| crate::mcp_result::redact(v, secrets)),
                    messages,
                }
            }
            (McpLookupOp::ReadResource { uri }, ServerResult::ReadResourceResult(_)) => {
                let mut contents: Vec<McpResourceContents> =
                    serde_json::from_value(value["contents"].clone())
                        .map_err(|_| McpLookupError::UnsupportedResult)?;
                if contents.len() > TOOLS_CAP {
                    return Err(McpLookupError::BodyLimit);
                }
                for part in &mut contents {
                    redact_resource(part, (secrets, identity_secrets))?;
                }
                McpLookupData::Resource {
                    uri: uri.clone(),
                    contents,
                }
            }
            _ => return Err(McpLookupError::UnsupportedResult),
        };
        bounded_value(&data)?;
        return Ok(data);
    }
    let mut prompts = vec![];
    let mut resources = vec![];
    let mut templates = vec![];
    let mut cursor = None;
    let mut cursors = HashSet::new();
    let mut identities = HashSet::new();
    for _ in 0..MAX_LIST_PAGES {
        let params = Some(model::PaginatedRequestParams::default().with_cursor(cursor));
        let request = match op {
            McpLookupOp::ListPrompts => {
                ClientRequest::ListPromptsRequest(model::ListPromptsRequest {
                    method: Default::default(),
                    params,
                    extensions: Default::default(),
                })
            }
            McpLookupOp::ListResources => {
                ClientRequest::ListResourcesRequest(model::ListResourcesRequest {
                    method: Default::default(),
                    params,
                    extensions: Default::default(),
                })
            }
            McpLookupOp::ListResourceTemplates => {
                ClientRequest::ListResourceTemplatesRequest(model::ListResourceTemplatesRequest {
                    method: Default::default(),
                    params,
                    extensions: Default::default(),
                })
            }
            _ => unreachable!(),
        };
        let result = once(peer, request, deadline, cancel, dispatched).await?;
        let value = bounded_value(&result)?;
        let next = match (&result, op) {
            (ServerResult::ListPromptsResult(result), McpLookupOp::ListPrompts) => {
                result.next_cursor.clone()
            }
            (ServerResult::ListResourcesResult(result), McpLookupOp::ListResources) => {
                result.next_cursor.clone()
            }
            (
                ServerResult::ListResourceTemplatesResult(result),
                McpLookupOp::ListResourceTemplates,
            ) => result.next_cursor.clone(),
            _ => return Err(McpLookupError::UnsupportedResult),
        };
        match op {
            McpLookupOp::ListPrompts => {
                let page: Vec<McpPromptInfo> = serde_json::from_value(value["prompts"].clone())
                    .map_err(|_| McpLookupError::InvalidData)?;
                for mut prompt in page {
                    if !identity(&prompt.name)
                        || prompt.arguments.len() > TOOLS_CAP
                        || !identities.insert(prompt.name.clone())
                    {
                        return Err(McpLookupError::InvalidData);
                    }
                    safe_identity(&prompt.name, identity_secrets)?;
                    redact_metadata(&mut prompt.title, secrets)?;
                    redact_metadata(&mut prompt.description, secrets)?;
                    let mut args = HashSet::new();
                    for arg in &mut prompt.arguments {
                        if !identity(&arg.name) || !args.insert(arg.name.clone()) {
                            return Err(McpLookupError::InvalidData);
                        }
                        safe_identity(&arg.name, identity_secrets)?;
                        redact_metadata(&mut arg.description, secrets)?;
                    }
                    prompts.push(prompt);
                }
            }
            McpLookupOp::ListResources => {
                let page: Vec<McpResourceInfo> = serde_json::from_value(value["resources"].clone())
                    .map_err(|_| McpLookupError::InvalidData)?;
                for mut resource in page {
                    if !identity(&resource.name)
                        || !uri_identity(&resource.uri)
                        || !identities.insert(format!("{:?}", (&resource.name, &resource.uri)))
                    {
                        return Err(McpLookupError::InvalidData);
                    }
                    safe_identity(&resource.name, identity_secrets)?;
                    safe_identity(&resource.uri, identity_secrets)?;
                    if let Some(mime) = &resource.mime_type {
                        safe_identity(mime, identity_secrets)?;
                    }
                    redact_metadata(&mut resource.title, secrets)?;
                    redact_metadata(&mut resource.description, secrets)?;
                    resources.push(resource);
                }
            }
            McpLookupOp::ListResourceTemplates => {
                let page: Vec<McpResourceTemplate> =
                    serde_json::from_value(value["resourceTemplates"].clone())
                        .map_err(|_| McpLookupError::InvalidData)?;
                for mut template in page {
                    if !identity(&template.name)
                        || !uri_identity(&template.uri_template)
                        || !identities
                            .insert(format!("{:?}", (&template.name, &template.uri_template)))
                    {
                        return Err(McpLookupError::InvalidData);
                    }
                    safe_identity(&template.name, identity_secrets)?;
                    safe_identity(&template.uri_template, identity_secrets)?;
                    if let Some(mime) = &template.mime_type {
                        safe_identity(mime, identity_secrets)?;
                    }
                    redact_metadata(&mut template.title, secrets)?;
                    redact_metadata(&mut template.description, secrets)?;
                    templates.push(template);
                }
            }
            _ => unreachable!(),
        }
        if prompts.len() + resources.len() + templates.len() > TOOLS_CAP {
            return Err(McpLookupError::CatalogLimit);
        }
        let bytes = serde_json::to_vec(&prompts)
            .map_err(|_| McpLookupError::InvalidData)?
            .len()
            + serde_json::to_vec(&resources)
                .map_err(|_| McpLookupError::InvalidData)?
                .len()
            + serde_json::to_vec(&templates)
                .map_err(|_| McpLookupError::InvalidData)?
                .len();
        if bytes > RESULT_TEXT_BYTES_CAP {
            return Err(McpLookupError::CatalogLimit);
        }
        match next {
            None => {
                return Ok(match op {
                    McpLookupOp::ListPrompts => McpLookupData::Prompts {
                        available: true,
                        items: prompts,
                    },
                    McpLookupOp::ListResources => McpLookupData::Resources {
                        available: true,
                        items: resources,
                    },
                    _ => McpLookupData::ResourceTemplates {
                        available: true,
                        items: templates,
                    },
                });
            }
            Some(next) => {
                if !identity(&next) || !cursors.insert(next.clone()) {
                    return Err(McpLookupError::CatalogLimit);
                }
                cursor = Some(next);
            }
        }
    }
    Err(McpLookupError::CatalogLimit)
}

fn bounded_value<T: serde::Serialize>(result: &T) -> Result<serde_json::Value, McpLookupError> {
    let bytes = serde_json::to_vec(result).map_err(|_| McpLookupError::InvalidData)?;
    if bytes.len() > RESULT_TEXT_BYTES_CAP {
        return Err(McpLookupError::BodyLimit);
    }
    serde_json::from_slice(&bytes).map_err(|_| McpLookupError::InvalidData)
}
fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= ORIGINAL_NAME_BYTES_CAP && !value.contains('\0')
}
fn uri_identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= DESCRIPTION_BYTES_CAP && !value.contains('\0')
}
fn safe_identity(value: &str, secrets: &[String]) -> Result<(), McpLookupError> {
    if secrets
        .iter()
        .any(|secret| !secret.is_empty() && value.contains(secret))
    {
        Err(McpLookupError::SensitiveIdentity)
    } else {
        Ok(())
    }
}
fn redact_metadata(value: &mut Option<String>, secrets: &[String]) -> Result<(), McpLookupError> {
    if let Some(text) = value {
        if text.len() > DESCRIPTION_BYTES_CAP {
            return Err(McpLookupError::CatalogLimit);
        }
        *text = crate::mcp_result::redact(text, secrets);
    }
    Ok(())
}
fn validate_blob(data: &str, secrets: &[String]) -> Result<(), McpLookupError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| McpLookupError::InvalidData)?;
    if secrets.iter().any(|secret| {
        !secret.is_empty()
            && decoded
                .windows(secret.len())
                .any(|part| part == secret.as_bytes())
    }) {
        return Err(McpLookupError::SensitiveBinary);
    }
    Ok(())
}
fn redact_resource(
    part: &mut McpResourceContents,
    (secrets, identity_secrets): (&[String], &[String]),
) -> Result<(), McpLookupError> {
    let (uri, mime) = match part {
        McpResourceContents::Text { uri, mime_type, .. }
        | McpResourceContents::Blob { uri, mime_type, .. } => (uri, mime_type),
    };
    safe_identity(uri, identity_secrets)?;
    if let Some(mime) = mime {
        safe_identity(mime, identity_secrets)?;
    }
    match part {
        McpResourceContents::Text { uri, text, .. } => {
            if !uri_identity(uri) {
                return Err(McpLookupError::InvalidData);
            }
            *text = crate::mcp_result::redact(text, secrets);
        }
        McpResourceContents::Blob { uri, blob, .. } => {
            if !uri_identity(uri) {
                return Err(McpLookupError::InvalidData);
            }
            validate_blob(blob, secrets)?;
        }
    }
    Ok(())
}
fn redact_content(
    content: &mut McpPromptContent,
    (secrets, identity_secrets): (&[String], &[String]),
) -> Result<(), McpLookupError> {
    match content {
        McpPromptContent::Text { text } => *text = crate::mcp_result::redact(text, secrets),
        McpPromptContent::Resource { resource } => {
            redact_resource(resource, (secrets, identity_secrets))?
        }
        McpPromptContent::Image { data, mime_type }
        | McpPromptContent::Audio { data, mime_type } => {
            safe_identity(mime_type, identity_secrets)?;
            validate_blob(data, secrets)?
        }
        McpPromptContent::ResourceLink {
            uri,
            name,
            title,
            description,
            mime_type,
        } => {
            if !uri_identity(uri) || !identity(name) {
                return Err(McpLookupError::InvalidData);
            }
            safe_identity(uri, identity_secrets)?;
            safe_identity(name, identity_secrets)?;
            if let Some(mime) = mime_type {
                safe_identity(mime, identity_secrets)?;
            }
            redact_metadata(title, secrets)?;
            redact_metadata(description, secrets)?;
        }
    }
    Ok(())
}

async fn once(
    peer: &Peer,
    request: ClientRequest,
    deadline: tokio::time::Instant,
    cancel: &AtomicBool,
    dispatched: Option<&AtomicBool>,
) -> Result<ServerResult, McpLookupError> {
    let mut handle = tokio::select! {
        biased;
        () = crate::provider::wait_cancel(cancel) => return Err(McpLookupError::Cancelled),
        result = tokio::time::timeout_at(deadline, peer.send_cancellable_request(request,
            rmcp::service::PeerRequestOptions::no_options())) => result.map_err(|_| McpLookupError::Deadline)?
                .map_err(|_| McpLookupError::Transport)?,
    };
    if let Some(dispatched) = dispatched {
        dispatched.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    let outcome = tokio::select! {
        biased;
        () = crate::provider::wait_cancel(cancel) => None,
        result = tokio::time::timeout_at(deadline, &mut handle.rx) => Some(result),
    };
    match outcome {
        None | Some(Err(_)) => {
            let reason = if outcome.is_none() {
                McpLookupError::Cancelled
            } else {
                McpLookupError::Deadline
            };
            tokio::time::timeout(CLOSE_TIMEOUT, handle.cancel(Some("request stopped".into())))
                .await
                .map_err(|_| McpLookupError::CleanupFailed)?
                .map_err(|_| McpLookupError::CleanupFailed)?;
            Err(reason)
        }
        Some(Ok(Ok(Ok(result)))) => Ok(result),
        Some(Ok(Ok(Err(rmcp::service::ServiceError::McpError(error))))) => match error.code.0 {
            -32601 => Err(McpLookupError::UnsupportedCapability),
            -32602 => Err(McpLookupError::InvalidArguments),
            _ => Err(McpLookupError::RemoteFailure),
        },
        _ => Err(McpLookupError::Transport),
    }
}
