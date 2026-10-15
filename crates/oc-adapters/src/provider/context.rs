//! Immutable operation metadata, separate from deployment/auth compatibility.
use std::path::Path;

use sha2::{Digest, Sha256};

use super::{CacheMode, ProviderError, protocol::Protocol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestContext {
    project: String,
    session: String,
    parent: Option<String>,
    cache: String,
    operation: Option<String>,
}

impl RequestContext {
    pub(super) fn session(&self) -> &str {
        &self.session
    }
    pub(crate) fn for_operation(mut self, operation: &str) -> Self {
        self.operation = Some(operation.into());
        self
    }
    pub(super) fn operation(&self) -> Option<&str> {
        self.operation.as_deref()
    }
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
            operation: None,
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

    pub(super) fn openai_headers(
        &self,
        headers: &mut reqwest::header::HeaderMap,
    ) -> Result<(), ProviderError> {
        headers.insert(
            "session-id",
            reqwest::header::HeaderValue::from_str(&self.session)
                .map_err(|_| ProviderError::InvalidConfig)?,
        );
        Ok(())
    }

    pub(super) fn cache_body(
        context: Option<&Self>,
        protocol: Protocol,
        body: &mut serde_json::Value,
        supported: Option<bool>,
        cache: CacheMode,
    ) {
        match protocol {
            Protocol::Responses | Protocol::Chat => {
                if cache.sends_prompt_cache_key(protocol, supported)
                    && let Some(context) = context
                {
                    body["prompt_cache_key"] = context.cache.clone().into();
                }
            }
            Protocol::Messages if cache == CacheMode::Auto => {
                // V2 Auto: final tool, first/last distinct system boundaries,
                // then one conversation tail. At most four markers; independent
                // of the OpenAI-compatible cache-key capability and identity.
                if let Some(last) = body
                    .get_mut("tools")
                    .and_then(serde_json::Value::as_array_mut)
                    .and_then(|v| v.last_mut())
                {
                    last["cache_control"] = serde_json::json!({"type":"ephemeral"});
                }
                if let Some(system) = body
                    .get_mut("system")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    let length = system.len();
                    for (index, part) in system.iter_mut().enumerate() {
                        if index == 0 || index + 1 == length {
                            part["cache_control"] = serde_json::json!({"type":"ephemeral"});
                        }
                    }
                }
                if let Some(parts) = body
                    .get_mut("messages")
                    .and_then(serde_json::Value::as_array_mut)
                    .and_then(|v| v.last_mut())
                    .and_then(|v| {
                        v.get_mut("content")
                            .and_then(serde_json::Value::as_array_mut)
                    })
                    && let Some(index) = parts
                        .iter()
                        .rposition(|p| p["type"] == "text")
                        .or_else(|| parts.len().checked_sub(1))
                {
                    let part = &mut parts[index];
                    if part["type"] != "thinking" && part["type"] != "redacted_thinking" {
                        part["cache_control"] = serde_json::json!({"type":"ephemeral"});
                    }
                }
            }
            Protocol::Messages => {}
        }
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
