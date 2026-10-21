//! Bounded tool-result presentation facts, independent of provider/RAW text.
//! Missing facts mean unknown provenance, not a complete capture.

use serde::{Deserialize, Serialize};

/// Existing operation-preview byte ceiling, shared by live and durable views.
pub const PREVIEW_BYTES: usize = 2_048;
/// Existing serialized envelope ceiling for previews, references and fields.
pub const RECORD_BYTES: usize = PREVIEW_BYTES * 12 + 8192;

pub const EXECUTE_CALLS_CAP: usize = 16;
pub const EXECUTE_CODE_BYTES: usize = PREVIEW_BYTES * 2;
pub const EXECUTE_INPUT_BYTES: usize = 512;
pub const EXECUTE_TOOL_BYTES: usize = 512;

/// Observed host-call state under one execute operation, not another operation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ExecuteCallStatus {
    Running,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteCall {
    /// Captured catalog path (including literal segments, without `tools`), or fetch.
    pub tool: String,
    pub tool_limited: bool,
    pub status: ExecuteCallStatus,
    /// Structured redacted preview. Omitted fields are disclosed by input_limited.
    pub input: Option<serde_json::Value>,
    pub input_limited: bool,
}

/// Section provenance comes from the interpreter outcome, never marker matching.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteOutputKind {
    Text,
    Json,
    Warnings,
    Logs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecuteOutput {
    pub kind: ExecuteOutputKind,
    pub text: String,
    pub limited: bool,
}

/// Bounded immutable display facts, also used as disposable live replacements.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Execute {
    /// None while receiving/unavailable; Some("") is a captured empty program.
    pub code: Option<String>,
    pub code_limited: bool,
    pub calls: Vec<ExecuteCall>,
    pub calls_limited: bool,
    pub error: bool,
    pub elapsed_ms: Option<u64>,
    pub output: Vec<ExecuteOutput>,
}

impl Execute {
    pub fn retained_bytes(&self) -> usize {
        self.code.as_ref().map_or(0, String::len)
            + self
                .calls
                .iter()
                .map(|call| {
                    call.tool.len()
                        + call
                            .input
                            .as_ref()
                            .map_or(0, |input| input.to_string().len())
                })
                .sum::<usize>()
            + self
                .output
                .iter()
                .map(|section| section.text.len())
                .sum::<usize>()
    }

    pub fn is_valid(&self) -> bool {
        let mut previous = 0;
        self.code
            .as_ref()
            .is_none_or(|code| code.len() <= EXECUTE_CODE_BYTES)
            && self.calls.len() <= EXECUTE_CALLS_CAP
            && self.calls.iter().all(|call| {
                !call.tool.is_empty()
                    && call.tool.len() <= EXECUTE_TOOL_BYTES
                    && !call.tool.chars().any(char::is_control)
                    && call
                        .input
                        .as_ref()
                        .is_none_or(|input| input.to_string().len() <= EXECUTE_INPUT_BYTES)
            })
            && self.output.len() <= 3
            && self
                .output
                .iter()
                .map(|section| section.text.len())
                .sum::<usize>()
                <= PREVIEW_BYTES
            && self.output.iter().all(|section| {
                let order = match section.kind {
                    ExecuteOutputKind::Text | ExecuteOutputKind::Json => 1,
                    ExecuteOutputKind::Warnings => 2,
                    ExecuteOutputKind::Logs => 3,
                };
                let valid = order > previous;
                previous = order;
                valid
            })
    }
}

/// State of the existing owned text capture. Serialization retains its original
/// descriptor representation; this is not a second capture lifecycle.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CaptureState {
    Active,
    Complete,
    ProducerLimited,
    ArtifactCap,
    Quota,
    Io,
    RegisterFailure,
    Interrupted,
    Expired,
}

/// Metadata of the existing operation-owned output reference, never its payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capture {
    /// A recorded reference, not a grant or promise of current file readability.
    /// Durable projection clears it for expired/missing/foreign descriptors.
    pub reference: Option<String>,
    pub state: CaptureState,
    pub admitted_bytes: u64,
    pub retained_bytes: u64,
    pub admitted_lines: u64,
    pub retained_lines: u64,
}

/// Native shell facts before model-facing preparation changes its text envelope.
/// Stream previews remain bounded; these facts do not promise full recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shell {
    /// Actual foreground-await conversion, not the originally requested mode.
    #[serde(default)]
    pub background: bool,
    /// Frozen supervisor state. None while running or for legacy stream facts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_state: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_limited: bool,
    pub stderr_limited: bool,
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// Safe captured requested postimage of a successful native Write. This is a
/// display preview, not a read-back, restore image or file access capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Write {
    pub path: String,
    pub path_limited: bool,
    pub content: String,
    /// Full redacted requested-content extent, not bytes written to the file.
    pub content_bytes: u64,
    pub content_limited: bool,
}

/// Redacted display name derived from an exact captured MCP registry entry.
/// Never an invocation identifier, registry lookup key or permission resource.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpLabel {
    pub text: String,
    pub limited: bool,
}

impl McpLabel {
    pub fn is_valid(&self) -> bool {
        !self.text.trim().is_empty()
            && self.text.len() <= 512
            && !self.text.chars().any(char::is_control)
    }
}

/// Newly committed automatic instruction loads, never their source bodies.
/// Paths are safe ordered unique project-relative (or absolute) display labels,
/// not grants or filesystem lookup keys.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstructionLoads {
    /// A bounded subset; omitted labels are disclosed by `total`.
    pub paths: Vec<String>,
    pub total: usize,
}

impl InstructionLoads {
    pub fn retained_bytes(&self) -> usize {
        self.paths.iter().map(String::len).sum()
    }

    pub fn is_valid(&self) -> bool {
        self.total > 0
            && self.total <= 128
            && self.total >= self.paths.len()
            && self.paths.len() <= 128
            && self.retained_bytes() <= PREVIEW_BYTES
            && self.paths.iter().enumerate().all(|(index, path)| {
                !path.trim().is_empty()
                    && !path.chars().any(char::is_control)
                    && !self.paths[..index].contains(path)
            })
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

/// One prepared result's safe body and independent provenance. The body is an
/// available preview, not the output-reference notice appended for the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Presentation {
    /// Intent/argument metadata has no result body. Never hide a later error or
    /// unknown outcome behind the empty body of this earlier snapshot.
    #[serde(default, skip_serializing_if = "is_false")]
    pub label_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_label: Option<McpLabel>,
    pub body: String,
    pub body_bytes: u64,
    pub body_limited: bool,
    pub generated_guidance: bool,
    /// None for a producer whose loss/pagination facts are unavailable.
    pub producer_limited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture: Option<Capture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<Shell>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<Write>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execute: Option<Execute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<InstructionLoads>,
}

impl Presentation {
    /// Retained text/reference bytes, including independent stream previews.
    pub fn retained_bytes(&self) -> usize {
        self.body.len()
            + self.mcp_label.as_ref().map_or(0, |label| label.text.len())
            + self.capture.as_ref().map_or(0, |capture| {
                capture.reference.as_ref().map_or(0, String::len)
            })
            + self.shell.as_ref().map_or(0, |shell| {
                shell.stdout.len()
                    + shell.stderr.len()
                    + shell.process_state.as_ref().map_or(0, String::len)
            })
            + self
                .write
                .as_ref()
                .map_or(0, |write| write.path.len() + write.content.len())
            + self.execute.as_ref().map_or(0, Execute::retained_bytes)
            + self
                .instructions
                .as_ref()
                .map_or(0, InstructionLoads::retained_bytes)
    }

    /// Retain the already admitted/redacted body at the existing preview limit.
    pub fn new(body: &str, body_bytes: u64, generated_guidance: bool) -> Self {
        let end = body.floor_char_boundary(body.len().min(PREVIEW_BYTES));
        Self {
            label_only: false,
            mcp_label: None,
            body: body[..end].to_owned(),
            body_bytes,
            body_limited: end < body.len() || body_bytes > body.len() as u64,
            generated_guidance,
            producer_limited: None,
            capture: None,
            shell: None,
            write: None,
            execute: None,
            instructions: None,
        }
    }

    pub fn label(label: McpLabel) -> Self {
        let mut presentation = Self::new("", 0, false);
        presentation.label_only = true;
        presentation.mcp_label = Some(label);
        presentation
    }

    /// Optional durable facts are trusted only within the same bounded shape as
    /// their producer. Invalid/unknown records fall back to unknown provenance.
    pub fn is_valid(&self) -> bool {
        self.body.len() <= PREVIEW_BYTES
            && self
                .instructions
                .as_ref()
                .is_none_or(InstructionLoads::is_valid)
            && self.execute.as_ref().is_none_or(|execute| {
                execute.is_valid()
                    && self.mcp_label.is_none()
                    && self.shell.is_none()
                    && self.write.is_none()
                    && (!self.label_only
                        || (execute.elapsed_ms.is_none()
                            && !execute.error
                            && execute.output.is_empty()))
            })
            && self.mcp_label.as_ref().is_none_or(McpLabel::is_valid)
            && (self.mcp_label.is_none() || (self.shell.is_none() && self.write.is_none()))
            && (!self.label_only
                || ((self.mcp_label.is_some() || self.execute.is_some())
                    && self.body.is_empty()
                    && self.body_bytes == 0
                    && !self.body_limited
                    && !self.generated_guidance
                    && self.producer_limited.is_none()
                    && self.capture.is_none()
                    && self.instructions.is_none()))
            && self.body_bytes >= self.body.len() as u64
            && self.capture.as_ref().is_none_or(|capture| {
                capture
                    .reference
                    .as_ref()
                    .is_none_or(|path| path.len() <= 4096 && !path.chars().any(char::is_control))
                    && capture.retained_bytes <= capture.admitted_bytes
                    && capture.retained_lines <= capture.admitted_lines
            })
            && self.shell.as_ref().is_none_or(|shell| {
                shell.stdout.len() + shell.stderr.len() <= PREVIEW_BYTES
                    && shell.process_state.as_deref().is_none_or(|state| {
                        matches!(
                            state,
                            "completed" | "failed" | "cancelled" | "timed_out" | "unknown"
                        )
                    })
            })
            && (self.shell.is_none() || self.write.is_none())
            && self.write.as_ref().is_none_or(|write| {
                !write.path.is_empty()
                    && write.path.len() <= crate::patch::EFFECT_LINE_BYTES_CAP
                    && !write.path.chars().any(char::is_control)
                    && write.content.len() <= PREVIEW_BYTES
                    && write.content_bytes >= write.content.len() as u64
                    && (write.content_limited || write.content_bytes == write.content.len() as u64)
                    && write.content.bytes().filter(|&b| b == b'\n').count()
                        < crate::patch::EFFECT_LINES_CAP
            })
    }
}

#[cfg(test)]
mod tests;
