//! Typed local-file read outcomes; local source facts never become MCP origins.
use super::*;
use crate::files::read::Content;
use base64::Engine as _;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

/// Native local image facts. Serialization lowers only to Responses content;
/// the turn owner persists the separate source attachment before continuation.
#[derive(Clone, PartialEq, Eq)]
pub struct ReadToolOutput {
    facts: Value,
    display: String,
}
impl std::fmt::Debug for ReadToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadToolOutput").finish_non_exhaustive()
    }
}
impl Serialize for ReadToolOutput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        vec![
            crate::provider::InputContent::InputText {
                text: self.display.clone(),
            },
            crate::provider::InputContent::InputImage {
                image_url: format!(
                    "data:{};base64,{}",
                    self.facts["mime"].as_str().expect("validated MIME"),
                    self.facts["data"].as_str().expect("validated data")
                ),
                detail: None,
            },
        ]
        .serialize(serializer)
    }
}
impl ReadToolOutput {
    pub(crate) fn display(&self) -> &str {
        &self.display
    }
    pub(crate) fn facts(&self) -> &Value {
        &self.facts
    }
    pub(crate) fn estimated_tokens(&self) -> u64 {
        1500 + (self.display.encode_utf16().count() as u64 + 2) / 4
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.facts.to_string().len()
            + serde_json::to_vec(self).map_or(usize::MAX, |s| s.len())
            + self.display.len()
    }
    pub(crate) fn from_stored(facts: Value) -> Result<Self, String> {
        if facts.as_object().is_none_or(|object| {
            object.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "source" | "path" | "source_path" | "mime" | "bytes" | "data"
                )
            })
        }) {
            return Err("invalid native read facts".into());
        }
        let path = facts["path"].as_str().ok_or("invalid native read path")?;
        let source_path = facts["source_path"]
            .as_str()
            .ok_or("invalid native read source path")?;
        if path.is_empty()
            || path.len() > 4096
            || path.chars().any(char::is_control)
            || source_path.len() > 4096
            || source_path.chars().any(char::is_control)
            || !std::path::Path::new(source_path).is_absolute()
            || facts["source"] != "local_file"
        {
            return Err("invalid native read source".into());
        }
        let data = facts["data"].as_str().ok_or("invalid native image data")?;
        if data.len() > (crate::files::GREP_FILE_BYTES_CAP as usize).div_ceil(3) * 4 {
            return Err("native image exceeds read budget".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| "invalid native image data")?;
        if bytes.len() > crate::files::GREP_FILE_BYTES_CAP as usize {
            return Err("native image exceeds read budget".into());
        }
        let format = image::guess_format(&bytes).map_err(|_| "invalid native image")?;
        let mime = match format {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::Gif => "image/gif",
            image::ImageFormat::WebP => "image/webp",
            _ => return Err("unsupported native image".into()),
        };
        if facts["mime"] != mime || facts["bytes"].as_u64() != Some(bytes.len() as u64) {
            return Err("invalid native image identity".into());
        }
        // Stored attachments retain origin/graph checks, but their bytes must
        // independently satisfy the same bounded pixel/container validation.
        // This token belongs only to in-memory reconstruction, never live IO.
        crate::files::read::validate_image(&bytes, format, &AtomicBool::new(false))
            .map_err(|_| "invalid or over-budget stored read image")?;
        Ok(Self {
            display: format!("Read image {path}, {mime}, {} bytes", bytes.len()),
            facts,
        })
    }
}

pub(crate) struct Outcome {
    pub state: &'static str,
    pub output: String,
    pub image: Option<ReadToolOutput>,
    pub directory: bool,
    pub producer_limited: Option<bool>,
}
impl Outcome {
    pub fn error(state: &'static str, error: impl std::fmt::Display) -> Self {
        Self {
            state,
            output: format!("error: {error}"),
            image: None,
            directory: false,
            producer_limited: None,
        }
    }
}
pub(crate) fn parse(call: &ToolCall) -> Result<(&str, u64, usize), String> {
    parse_with_offset_cap(call, 1_000_000)
}
pub(crate) fn parse_with_offset_cap(
    call: &ToolCall,
    offset_cap: u64,
) -> Result<(&str, u64, usize), String> {
    let args = call
        .arguments
        .as_object()
        .ok_or("expected read arguments object")?;
    if args
        .keys()
        .any(|k| !matches!(k.as_str(), "path" | "offset" | "limit"))
    {
        return Err("unexpected read property".into());
    }
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control))
        .ok_or("read requires a bounded path")?;
    let offset = match args.get("offset") {
        Some(n) => n
            .as_u64()
            .filter(|n| (1..=offset_cap).contains(n))
            .ok_or_else(|| format!("read offset must be 1..{offset_cap}"))?,
        None => 1,
    };
    let limit = match args.get("limit") {
        Some(n) => n
            .as_u64()
            .filter(|n| (1..=crate::files::READ_LINES_CAP as u64).contains(n))
            .ok_or("read limit must be 1..2000")? as usize,
        None => crate::files::READ_LINES_CAP,
    };
    Ok((path, offset, limit))
}

pub(crate) fn execute(
    files: &Files,
    policy: &dyn ToolPolicy,
    cancel: &AtomicBool,
    call: &ToolCall,
    images: bool,
) -> Outcome {
    let (path, offset, limit) = match parse(call) {
        Ok(args) => args,
        Err(e) => return Outcome::error("failed", e),
    };
    if let Err(e) = policy.check_call(call) {
        return Outcome::error("failed", e);
    }
    let result = files.read_admitted(
        path,
        offset,
        limit,
        |p| !policy.search_path_denied(&p.to_string_lossy()),
        cancel,
    );
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Outcome::error("cancelled", "cancelled");
    }
    match result {
        Ok(Content::Image {
            bytes,
            mime,
            source_path,
        }) => {
            if !images {
                return Outcome::error(
                    "failed",
                    "selected model cannot accept image input; choose an image-capable Responses model",
                );
            }
            let Some(source_path) = source_path
                .to_str()
                .filter(|source| source.len() <= 4096 && !source.chars().any(char::is_control))
            else {
                return Outcome::error("failed", "invalid native read source path");
            };
            // Content::Image was decoded by the descriptor-held file owner
            // with this invocation's cancellation token. Do not decode twice
            // or replace that token by entering the stored reconstruction path.
            let image = ReadToolOutput {
                display: format!("Read image {path}, {mime}, {} bytes", bytes.len()),
                facts: json!({"source":"local_file", "path":path, "source_path":source_path, "mime":mime, "bytes":bytes.len(), "data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
            };
            Outcome {
                state: "completed",
                output: image.display.clone(),
                image: Some(image),
                directory: false,
                producer_limited: Some(false),
            }
        }
        Ok(content) => {
            let directory = matches!(content, Content::Directory(_));
            let (Content::Text(page) | Content::Directory(page)) = content else {
                unreachable!()
            };
            let kind = if directory { "directory" } else { "file" };
            let unit = if directory { "entries" } else { "lines" };
            let mut output = if page.lines.is_empty() {
                format!("Read {kind} {path}, 0 {unit}")
            } else {
                format!(
                    "Read {kind} {path}, {unit} {}-{}",
                    page.offset,
                    page.offset + page.lines.len() as u64 - 1
                )
            };
            for (index, line) in page.lines.iter().enumerate() {
                output.push('\n');
                if !directory {
                    output.push_str(&format!("{}: ", page.offset + index as u64));
                }
                output.push_str(line);
            }
            if let Some(next) = page.next_offset {
                output.push_str(&format!(
                    "\n[Output truncated. Continue reading with offset: {next}]"
                ));
            }
            Outcome {
                state: "completed",
                output,
                image: None,
                directory,
                producer_limited: Some(page.next_offset.is_some()),
            }
        }
        Err(e) => Outcome::error("failed", e),
    }
}
