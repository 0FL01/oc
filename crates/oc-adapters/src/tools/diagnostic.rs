//! Bounded native failure facts. Execution state is carried separately, never
//! inferred from these model-visible bytes (which may also be ordinary content).
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Diagnostic {
    pub tool: String,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub detail: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(tool: &str, code: &str, message: impl AsRef<str>) -> Self {
        Self {
            tool: bounded(tool, 128),
            code: bounded(code, 128),
            // Redact the complete admitted message before the publication owner
            // clips it; clipping here could expose a prefix of a known secret.
            message: message.as_ref().to_owned(),
            detail: Value::Null,
            hint: None,
        }
    }

    pub fn detail(mut self, detail: Value) -> Self {
        self.detail = detail;
        self
    }

    pub fn hint(mut self, hint: &str) -> Self {
        self.hint = Some(bounded(hint, 512));
        self
    }

    pub fn output(&self, state: &str) -> String {
        json!({"status":state, "error":self}).to_string()
    }

    pub fn failed(self) -> (&'static str, String) {
        ("failed", self.output("failed"))
    }

    pub fn cancelled(tool: &str) -> (&'static str, String) {
        let diagnostic = Self::new(tool, "cancelled", "Execution cancelled")
            .detail(json!({"by":"unknown", "reason":"cancellation_requested"}));
        ("cancelled", diagnostic.output("cancelled"))
    }
}

fn bounded(text: &str, cap: usize) -> String {
    text[..text.floor_char_boundary(text.len().min(cap))].to_owned()
}

impl super::ToolError {
    pub(crate) fn diagnostic(&self, tool: &str) -> Diagnostic {
        let code = match self {
            Self::Unsupported { .. } => "unsupported",
            Self::UnknownTool { .. } => "unknown_tool",
            Self::InvalidArgs { .. } => "invalid_args",
            Self::Denied { .. } => "permission_denied",
            Self::ApprovalRequired { .. } => "approval_required",
            Self::Failed { .. } => "execution_failed",
            Self::Cancelled => "cancelled",
        };
        Diagnostic::new(tool, code, self.to_string())
    }
}
