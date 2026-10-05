//! Immutable operation metadata, separate from deployment/auth compatibility.
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{ProviderError, protocol::Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestContext {
    project: String,
    session: String,
    parent: Option<String>,
    cache: String,
}

impl RequestContext {
    pub(crate) fn capture(
        db: &crate::storage::Db,
        project: &Path,
        session: &str,
    ) -> Result<Self, crate::runtime::RuntimeError> {
        Ok(Self {
            project: crate::approval::project_identity(project).map_err(|_| {
                crate::runtime::RuntimeError::InvalidArgs(
                    "request project identity unavailable".into(),
                )
            })?,
            session: session.to_owned(),
            parent: db.session_meta(session)?.parent_id,
            cache: format!(
                "{:x}",
                Sha256::digest(db.cache_lineage(session)?.as_bytes())
            ),
        })
    }

    pub(super) fn go_headers(
        &self,
        headers: &mut reqwest::header::HeaderMap,
    ) -> Result<(), ProviderError> {
        use reqwest::header::HeaderValue;
        for (name, value) in [
            ("user-agent", crate::USER_AGENT),
            ("x-opencode-client", "oc"),
            ("x-opencode-project", self.project.as_str()),
            ("x-opencode-session", self.session.as_str()),
            ("x-session-affinity", self.session.as_str()),
            ("x-session-id", self.session.as_str()),
        ] {
            headers.insert(
                name,
                HeaderValue::from_str(value).map_err(|_| ProviderError::InvalidConfig)?,
            );
        }
        headers.remove("x-parent-session-id");
        if let Some(parent) = &self.parent {
            headers.insert(
                "x-parent-session-id",
                HeaderValue::from_str(parent).map_err(|_| ProviderError::InvalidConfig)?,
            );
        }
        Ok(())
    }

    pub(super) fn cache_body(
        &self,
        protocol: Protocol,
        body: &mut serde_json::Value,
        chat_supported: bool,
    ) {
        match protocol {
            Protocol::Responses => body["prompt_cache_key"] = self.cache.clone().into(),
            Protocol::Chat if chat_supported => {
                body["prompt_cache_key"] = self.cache.clone().into()
            }
            Protocol::Chat => {}
            Protocol::Messages => {
                // OC2 priority: tools, initial system, then message parts. Only
                // the terminal part of each useful prefix needs a breakpoint.
                let mut remaining = 4;
                for field in ["tools", "system"] {
                    if let Some(last) = body[field].as_array_mut().and_then(|v| v.last_mut()) {
                        last["cache_control"] = serde_json::json!({"type":"ephemeral"});
                        remaining -= 1;
                    }
                }
                if let Some(messages) = body["messages"].as_array_mut() {
                    for message in messages.iter_mut().rev() {
                        if remaining == 0 {
                            break;
                        }
                        if let Some(last) = message["content"].as_array_mut().and_then(|v| {
                            v.iter_mut().rev().find(|p| {
                                p["type"] != "thinking" && p["type"] != "redacted_thinking"
                            })
                        }) {
                            last["cache_control"] = serde_json::json!({"type":"ephemeral"});
                            remaining -= 1;
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
