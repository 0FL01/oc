//! Explicit caller-only MCP facts. These do not grant policy or enter history.
use super::{McpBinding, SessionId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq)]
pub enum McpLookupOp {
    ListPrompts,
    ListResources,
    ListResourceTemplates,
    GetPrompt {
        name: String,
        arguments: BTreeMap<String, String>,
    },
    ReadResource {
        uri: String,
    },
}
impl McpLookupOp {
    pub fn method(&self) -> &'static str {
        match self {
            Self::ListPrompts => "prompts/list",
            Self::ListResources => "resources/list",
            Self::ListResourceTemplates => "resources/templates/list",
            Self::GetPrompt { .. } => "prompts/get",
            Self::ReadResource { .. } => "resources/read",
        }
    }
    pub fn is_catalog(&self) -> bool {
        matches!(
            self,
            Self::ListPrompts | Self::ListResources | Self::ListResourceTemplates
        )
    }
    pub fn target(&self) -> &str {
        match self {
            Self::GetPrompt { name, .. } => name,
            Self::ReadResource { uri } => uri,
            _ => "*",
        }
    }
}
impl std::fmt::Debug for McpLookupOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpLookupOp")
            .field("method", &self.method())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct McpLookup {
    pub binding: McpBinding,
    pub server: String,
    pub session: SessionId,
    pub operation: McpLookupOp,
    /// False returns an existing healthy metadata snapshot; true explicitly relists.
    pub refresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpLookupError {
    Unavailable,
    StaleBinding,
    SessionMismatch,
    ChildSessionUnsupported,
    PermissionDenied,
    ApprovalRequired,
    InvalidArguments,
    InvalidData,
    UnsupportedCapability,
    UnsupportedResult,
    CatalogLimit,
    BodyLimit,
    RemoteFailure,
    Cancelled,
    Deadline,
    Transport,
    CleanupFailed,
    UnsafeRetry,
    NativeFailure,
    SensitiveBinary,
    /// A known protected value cannot be returned as an exact protocol identity.
    SensitiveIdentity,
}
impl std::fmt::Display for McpLookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP lookup {:?}", self)
    }
}
impl std::error::Error for McpLookupError {}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpPromptArgument {
    pub name: String,
    pub description: Option<String>,
    pub required: Option<bool>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub arguments: Vec<McpPromptArgument>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceInfo {
    pub name: String,
    pub uri: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResourceTemplate {
    pub name: String,
    pub uri_template: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum McpResourceContents {
    Text {
        uri: String,
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
        text: String,
    },
    Blob {
        uri: String,
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
        blob: String,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpPromptRole {
    User,
    Assistant,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpPromptContent {
    Text {
        text: String,
    },
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    Audio {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
    Resource {
        resource: McpResourceContents,
    },
    ResourceLink {
        uri: String,
        name: String,
        title: Option<String>,
        description: Option<String>,
        #[serde(rename = "mimeType")]
        mime_type: Option<String>,
    },
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpPromptMessage {
    pub role: McpPromptRole,
    pub content: McpPromptContent,
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpLookupData {
    Prompts {
        available: bool,
        items: Vec<McpPromptInfo>,
    },
    Resources {
        available: bool,
        items: Vec<McpResourceInfo>,
    },
    ResourceTemplates {
        available: bool,
        items: Vec<McpResourceTemplate>,
    },
    Prompt {
        name: String,
        arguments: BTreeMap<String, String>,
        description: Option<String>,
        messages: Vec<McpPromptMessage>,
    },
    Resource {
        uri: String,
        contents: Vec<McpResourceContents>,
    },
}
impl std::fmt::Debug for McpLookupData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Prompts { .. } => "prompts",
            Self::Resources { .. } => "resources",
            Self::ResourceTemplates { .. } => "templates",
            Self::Prompt { .. } => "prompt",
            Self::Resource { .. } => "resource",
        };
        f.debug_struct("McpLookupData")
            .field("kind", &kind)
            .finish_non_exhaustive()
    }
}
impl McpLookupData {
    pub fn catalog_entries(&self) -> usize {
        match self {
            Self::Prompts { items, .. } => items.len(),
            Self::Resources { items, .. } => items.len(),
            Self::ResourceTemplates { items, .. } => items.len(),
            _ => 0,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLookupReply {
    pub binding: McpBinding,
    pub server: String,
    pub source: String,
    pub cached: bool,
    pub data: McpLookupData,
}
