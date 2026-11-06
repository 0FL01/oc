//! Native codex_web image-file workflow; generic MCP/media remain unchanged.
use super::*;
use base64::Engine as _;
use serde_json::{Value, json};

const IMAGE_BYTES: usize = 10 * 1024 * 1024;
const INPUT_BYTES: usize = 18 * 1024 * 1024;
const DATA_URL_BYTES: usize = 24 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(in super::super) enum Kind {
    Generate,
    Edit,
}

impl Kind {
    pub(in super::super) fn for_entry(entry: &mcp_remote::RegistryEntry) -> Option<Self> {
        match (entry.server.as_str(), entry.tool.as_str()) {
            ("codex_web", "generate_image") => Some(Self::Generate),
            ("codex_web", "edit_image") => Some(Self::Edit),
            _ => None,
        }
    }

    pub(in super::super) fn schema(self, mut input: Value) -> Value {
        if !input["properties"].is_object() {
            input["properties"] = json!({});
        }
        input["properties"]["output_path"] = json!({
            "type":"string", "minLength":1, "maxLength":4096,
            "description":"Required destination filename, workspace-relative or absolute. Existing entries are never overwritten."
        });
        if matches!(self, Self::Edit) {
            input["properties"]["images"] = json!({
                "type":"array", "minItems":1, "maxItems":5,
                "items":{"type":"string","minLength":1},
                "description":"PNG/JPEG/WebP local paths, public HTTP(S) URLs or data URLs, in order."
            });
        }
        let mut required = input["required"].as_array().cloned().unwrap_or_default();
        if !required.iter().any(|field| field == "output_path") {
            required.push("output_path".into());
        }
        input["required"] = required.into();
        input
    }

    pub(in super::super) fn description(self) -> String {
        let operation = match self {
            Self::Generate => "Generate an image",
            Self::Edit => "Edit 1-5 images from local paths or URLs",
        };
        format!(
            "{operation} and save exact bytes to required output_path without overwriting. Returns {{path,mime_type,bytes}} only after saving. Do not regenerate or switch providers to save an image."
        )
    }
}

pub(in super::super) fn output_schema() -> Value {
    json!({"type":"object","properties":{
        "path":{"type":"string"},"mime_type":{"type":"string"},"bytes":{"type":"integer"}
    },"required":["path","mime_type","bytes"],"additionalProperties":false})
}

struct Input {
    arguments: Value,
    output: String,
    images: Vec<String>,
}

impl Input {
    fn parse(kind: Kind, arguments: &Value) -> Result<Self, String> {
        let mut arguments = arguments
            .as_object()
            .cloned()
            .ok_or("expected image arguments object")?;
        let output = arguments
            .remove("output_path")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or("output_path is required")?;
        if output.trim().is_empty()
            || output.len() > 4096
            || output.chars().any(char::is_control)
            || matches!(output.rsplit('/').next(), None | Some("" | "." | ".."))
        {
            return Err(
                "output_path must specify a destination filename (at most 4096 bytes, no controls)"
                    .into(),
            );
        }
        let images = if matches!(kind, Kind::Edit) {
            arguments
                .get("images")
                .and_then(Value::as_array)
                .filter(|values| (1..=5).contains(&values.len()))
                .ok_or("images must contain 1 to 5 paths or URLs")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_owned)
                        .ok_or_else(|| "images must contain nonempty strings".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            arguments: Value::Object(arguments),
            output,
            images,
        })
    }
}

fn remote_image(input: &str) -> bool {
    input.starts_with("data:")
        || reqwest::Url::parse(input).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

fn image_mime(bytes: &[u8], cancel: &AtomicBool) -> Result<&'static str, String> {
    if bytes.is_empty() || bytes.len() > IMAGE_BYTES {
        return Err("image is empty or exceeds 10 MiB".into());
    }
    let format = image::guess_format(bytes).map_err(|_| "invalid image bytes")?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("images must be PNG, JPEG or WebP".into()),
    };
    crate::files::read::validate_image(bytes, format, cancel).map_err(|e| e.to_string())?;
    Ok(mime)
}

pub(in super::super) struct Prepared {
    pub(in super::super) arguments: Value,
    destination: crate::patch::ImageDestination,
}

impl Prepared {
    pub(in super::super) fn save(
        self,
        output: rmcp::model::CallToolResult,
        cancel: &AtomicBool,
    ) -> Result<crate::mcp_result::McpToolOutput, String> {
        if output.is_error == Some(true) {
            let text = output
                .content
                .iter()
                .filter_map(|part| part.as_text())
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!("server declared image failure: {text}"));
        }
        let mut images = output.content.iter().filter_map(|part| part.as_image());
        let part = images
            .next()
            .ok_or("server did not return exactly one inline image")?;
        if images.next().is_some() {
            return Err("server returned multiple images; no file saved".into());
        }
        if part.data.len() > IMAGE_BYTES.div_ceil(3) * 4 {
            return Err("returned image exceeds 10 MiB".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&part.data)
            .map_err(|_| "invalid returned image base64")?;
        let mime = image_mime(&bytes, cancel)?;
        if part.mime_type != mime {
            return Err("returned image MIME does not match its bytes".into());
        }
        let path = self.destination.save(&bytes, cancel)?;
        let saved = json!({"path":path,"mime_type":mime,"bytes":bytes.len()});
        let result: rmcp::model::CallToolResult = serde_json::from_value(json!({
            "content":[{"type":"text","text":saved.to_string()}],"structuredContent":saved
        }))
        .map_err(|_| "cannot encode saved image result")?;
        crate::mcp_result::project_rich(result)
            .map_err(|_| "cannot project saved image result".into())
    }
}

impl Runtime<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) async fn prepare_mcp_image(
        &self,
        issuing: &ExecutionGeneration,
        session: &str,
        turn: &str,
        op: &str,
        call: &crate::tools::ToolCall,
        ctx: &ToolContext<'_>,
        policy: &RuntimePolicy<'_>,
        attached: &McpGeneration,
        agent: Option<String>,
        agent_digest: Option<String>,
    ) -> Result<Option<Prepared>, String> {
        let Some(kind) = attached
            .entries
            .iter()
            .find(|entry| entry.namespaced == call.name)
            .and_then(Kind::for_entry)
        else {
            return Ok(None);
        };
        // A permit for the outer MCP call is not a permit for a file sub-operation.
        let policy = RuntimePolicy {
            permit: None,
            ..policy.clone()
        };
        let mut input = Input::parse(kind, &call.arguments)?;
        let mut paths = vec![("image_output", input.output.clone())];
        paths.extend(
            input
                .images
                .iter()
                .filter(|path| !remote_image(path))
                .map(|path| ("read", path.clone())),
        );
        let mut files = ctx.files.clone();
        let targets = paths
            .iter()
            .map(|(_, path)| files.concrete_scope(path))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // Every explicit action/boundary Deny precedes approval or file inspection.
        for ((action, path), target) in paths.iter().zip(&targets) {
            if policy.effect(action, &permission_path(policy.root, path)) == Permission::Deny {
                return Err(format!(
                    "denied {}",
                    if *action == "image_output" {
                        "edit"
                    } else {
                        action
                    }
                ));
            }
            if !files.is_internal_read_scope(target) {
                let resource = target.parent().ok_or("invalid image path")?.join("*");
                if policy.effect("external_directory", &resource.to_string_lossy())
                    == Permission::Deny
                {
                    return Err("denied external_directory".into());
                }
            }
        }
        for target in targets
            .iter()
            .filter(|path| !files.is_internal_read_scope(path))
        {
            let boundary = crate::tools::ToolCall {
                id: call.id.clone(),
                name: "external_directory".into(),
                arguments: json!({"directory":target.parent().ok_or("invalid image path")?.join("*")}),
            };
            self.admit_tool(
                issuing,
                session,
                turn,
                op,
                &boundary,
                ctx,
                &policy,
                ctx.cancel,
                agent.clone(),
                agent_digest.clone(),
                false,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
        files.admit_mutation_paths(vec![targets[0].clone()]);
        let mut decoded = 0usize;
        let mut encoded = 0usize;
        for (action, path) in &paths {
            let scoped = if *action == "read"
                && !files
                    .is_internal_read_scope(&files.concrete_scope(path).map_err(|e| e.to_string())?)
            {
                files
                    .pin_external(
                        &files.concrete_scope(path).map_err(|e| e.to_string())?,
                        false,
                    )
                    .map_err(|e| e.to_string())?
            } else {
                files.clone()
            };
            let file_ctx = ToolContext {
                files: &scoped,
                shell: ctx.shell,
                parent_env: ctx.parent_env,
                webfetch_auth: ctx.webfetch_auth.clone(),
                policy: &policy,
                subagent: ctx.subagent,
                snapshot: ctx.snapshot,
                cancel: ctx.cancel,
                roots: ctx.roots.clone(),
            };
            let nested = crate::tools::ToolCall {
                id: call.id.clone(),
                name: (*action).into(),
                arguments: json!({"path":path}),
            };
            let permitted = self
                .admit_tool(
                    issuing,
                    session,
                    turn,
                    op,
                    &nested,
                    &file_ctx,
                    &policy,
                    ctx.cancel,
                    agent.clone(),
                    agent_digest.clone(),
                    false,
                )
                .await
                .map_err(|e| e.to_string())?;
            permitted.check_call(&nested).map_err(|e| e.to_string())?;
        }
        // Permissions are complete. Encode local inputs before the paid remote call.
        for value in &mut input.images {
            if reqwest::Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            {
                continue;
            }
            let (bytes, mime) = if value.starts_with("data:") {
                let (prefix, data) = value.split_once(',').ok_or("invalid image data URL")?;
                if !matches!(
                    prefix,
                    "data:image/png;base64" | "data:image/jpeg;base64" | "data:image/webp;base64"
                ) || data.len() > IMAGE_BYTES.div_ceil(3) * 4
                {
                    return Err("invalid or oversized image data URL".into());
                }
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|_| "invalid image base64")?;
                let mime = image_mime(&bytes, ctx.cancel)?;
                (bytes, mime)
            } else {
                let target = files.concrete_scope(value).map_err(|e| e.to_string())?;
                let scoped = if files.is_internal_read_scope(&target) {
                    files.clone()
                } else {
                    files
                        .pin_external(&target, false)
                        .map_err(|e| e.to_string())?
                };
                match scoped
                    .read_admitted(
                        value,
                        1,
                        1,
                        |p| !policy.search_path_denied(&p.to_string_lossy()),
                        ctx.cancel,
                    )
                    .map_err(|e| e.to_string())?
                {
                    crate::files::read::Content::Image { bytes, mime, .. } => {
                        if bytes.len() > IMAGE_BYTES
                            || !matches!(mime, "image/png" | "image/jpeg" | "image/webp")
                        {
                            return Err(
                                "edit input must be PNG/JPEG/WebP and at most 10 MiB".into()
                            );
                        }
                        (bytes, mime)
                    }
                    _ => return Err("edit input must be a PNG, JPEG or WebP image".into()),
                }
            };
            decoded += bytes.len();
            if decoded > INPUT_BYTES {
                return Err("edit images exceed 18 MiB total".into());
            }
            *value = format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
            encoded += value.len();
            if encoded > DATA_URL_BYTES {
                return Err("edit image data URLs exceed 24 MiB total".into());
            }
        }
        if matches!(kind, Kind::Edit) {
            input.arguments["images"] = json!(input.images);
        }
        let destination = crate::patch::ImageDestination::prepare(&files, &input.output)?;
        Ok(Some(Prepared {
            arguments: input.arguments,
            destination,
        }))
    }
}
