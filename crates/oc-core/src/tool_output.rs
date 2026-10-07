//! Bounded tool-result presentation facts, independent of provider/RAW text.
//! Missing facts mean unknown provenance, not a complete capture.

use serde::{Deserialize, Serialize};

/// Existing operation-preview byte ceiling, shared by live and durable views.
pub const PREVIEW_BYTES: usize = 2_048;
/// Two bounded text previews at worst-case JSON escaping, plus reference/fields.
pub const RECORD_BYTES: usize = PREVIEW_BYTES * 12 + 8192;

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
    pub stdout: String,
    pub stderr: String,
    pub stdout_limited: bool,
    pub stderr_limited: bool,
    pub exit: Option<i32>,
    pub signal: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
}

/// One prepared result's safe body and independent provenance. The body is an
/// available preview, not the output-reference notice appended for the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Presentation {
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
}

impl Presentation {
    /// Retained text/reference bytes, including independent stream previews.
    pub fn retained_bytes(&self) -> usize {
        self.body.len()
            + self.capture.as_ref().map_or(0, |capture| {
                capture.reference.as_ref().map_or(0, String::len)
            })
            + self
                .shell
                .as_ref()
                .map_or(0, |shell| shell.stdout.len() + shell.stderr.len())
    }

    /// Retain the already admitted/redacted body at the existing preview limit.
    pub fn new(body: &str, body_bytes: u64, generated_guidance: bool) -> Self {
        let end = body.floor_char_boundary(body.len().min(PREVIEW_BYTES));
        Self {
            body: body[..end].to_owned(),
            body_bytes,
            body_limited: end < body.len() || body_bytes > body.len() as u64,
            generated_guidance,
            producer_limited: None,
            capture: None,
            shell: None,
        }
    }

    /// Optional durable facts are trusted only within the same bounded shape as
    /// their producer. Invalid/unknown records fall back to unknown provenance.
    pub fn is_valid(&self) -> bool {
        self.body.len() <= PREVIEW_BYTES
            && self.body_bytes >= self.body.len() as u64
            && self.capture.as_ref().is_none_or(|capture| {
                capture
                    .reference
                    .as_ref()
                    .is_none_or(|path| path.len() <= 4096 && !path.chars().any(char::is_control))
                    && capture.retained_bytes <= capture.admitted_bytes
                    && capture.retained_lines <= capture.admitted_lines
            })
            && self
                .shell
                .as_ref()
                .is_none_or(|shell| shell.stdout.len() + shell.stderr.len() <= PREVIEW_BYTES)
    }
}
