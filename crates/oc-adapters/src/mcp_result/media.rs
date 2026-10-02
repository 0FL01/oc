//! Actual MCP content → native-owned facts → OpenResponses tool-output content.
//! Donor 2670273ff17da96f85c5826ced57aa1b368754fa, open-responses.ts:507–537.
use super::{ResultError, failure_detail, redact, redact_json};
use base64::Engine as _;
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

const PARTS_CAP: usize = 64;
const IDENTITY_CAP: usize = 8 * 1024;
const PRESENTATION_CAP: usize = 8 * 1024;

/// Validated, redacted native tool facts. Debug never prints media or metadata.
/// Serialization is Responses output only; durable facts belong to TurnLog.
#[derive(Clone, PartialEq, Eq)]
pub struct McpToolOutput {
    facts: Value,
    display: String,
    media: bool,
    texts: Vec<String>,
}

impl std::fmt::Debug for McpToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpToolOutput")
            .field("blocks", &self.facts["content"].as_array().map(Vec::len))
            .field("media", &self.media)
            .finish()
    }
}

impl Serialize for McpToolOutput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.media {
            self.wire_parts().serialize(serializer)
        } else {
            self.display.serialize(serializer)
        }
    }
}

impl McpToolOutput {
    /// Safe existing tool-card/history presentation, without base64 payloads.
    pub fn display(&self) -> &str {
        &self.display
    }

    pub(crate) fn facts(&self) -> &Value {
        &self.facts
    }
    pub(crate) fn texts(&self) -> &[String] {
        &self.texts
    }
    pub(crate) fn has_media(&self) -> bool {
        self.media
    }

    pub(crate) fn prepare_common(
        &mut self,
        context: &crate::tools::output::Context<'_>,
    ) -> Result<bool, ResultError> {
        let texts = self.facts["content"]
            .as_array()
            .expect("validated content")
            .iter()
            .filter_map(native_text);
        let count = texts.clone().count();
        let bytes = texts.clone().map(str::len).sum::<usize>() + count.saturating_sub(1);
        let newlines = texts
            .clone()
            .map(|text| text.bytes().filter(|b| *b == b'\n').count())
            .sum::<usize>()
            + count.saturating_sub(1);
        let terminal = texts
            .clone()
            .next_back()
            .is_some_and(|text| text.ends_with('\n') || (text.is_empty() && count > 1));
        let mut lines = newlines + usize::from(bytes > 0 && !terminal);
        let mut bytes = bytes;
        for key in ["structuredContent", "_meta"] {
            if let Some(value) = self.facts.get(key) {
                data_text(value, None, &mut |text| {
                    bytes += text.len() + 1;
                    lines += crate::tools::output::lines(text).max(1) as usize;
                });
            }
        }
        if bytes <= context.limits.bytes()
            && lines <= context.limits.max_lines
            && self.display.len() <= crate::tools::output::SERVED_CAP
            && text_facts_bytes(&self.facts) <= crate::tools::output::SERVED_CAP
        {
            return Ok(false);
        }
        self.texts = Vec::new();
        self.display = String::new();
        let mut joined = String::new();
        let mut first = true;
        for part in self.facts["content"]
            .as_array_mut()
            .expect("validated content")
        {
            let value = if part["type"] == "text" {
                Some(&mut part["text"])
            } else if part["type"] == "resource" && part["resource"]["text"].is_string() {
                Some(&mut part["resource"]["text"])
            } else {
                None
            };
            if let Some(value) = value {
                let Value::String(text) = value.take() else {
                    return Err(ResultError::BadResult);
                };
                *value = String::new().into();
                if first {
                    joined = text;
                    first = false;
                } else {
                    joined.push('\n');
                    joined.push_str(&text);
                }
            }
        }
        for (key, budget) in [("structuredContent", 8192), ("_meta", 512)] {
            if let Some(value) = self.facts.get_mut(key) {
                take_data_text(value, None, &mut joined, &mut { budget });
            }
        }
        // One cold joined text includes structured free text. Independent native
        // facts and the publication notice have reserved served-text space.
        let mut common = context.clone();
        common.limits.max_bytes = common.limits.max_bytes.min(44_544);
        let prepared = common.prepare(joined, false, false, None);
        let mut preview = Some(prepared.text);
        for part in self.facts["content"]
            .as_array_mut()
            .expect("validated content")
        {
            match part["type"].as_str() {
                Some("text") => part["text"] = preview.take().unwrap_or_default().into(),
                Some("resource") if part["resource"]["text"].is_string() => {
                    part["resource"]["text"] = preview.take().unwrap_or_default().into()
                }
                _ => {}
            }
        }
        if let Some(text) = preview {
            self.facts["content"]
                .as_array_mut()
                .expect("validated content")
                .push(json!({"type":"text","text":text}));
        }
        if text_facts_bytes(&self.facts) > crate::tools::output::SERVED_CAP {
            let content = self.facts["content"].as_array().expect("validated content");
            let first = content.iter().position(|part| {
                part["type"] == "text"
                    || (part["type"] == "resource" && part["resource"]["text"].is_string())
            });
            if let Some(index) = first {
                let resource = self.facts["content"][index]["type"] == "resource";
                let value = if resource {
                    &mut self.facts["content"][index]["resource"]["text"]
                } else {
                    &mut self.facts["content"][index]["text"]
                };
                let Value::String(text) = value.take() else {
                    return Err(ResultError::BadResult);
                };
                *value = Value::String(String::new());
                let room =
                    crate::tools::output::SERVED_CAP.saturating_sub(text_facts_bytes(&self.facts));
                let bounded = crate::tools::output::json_preview(text, room, false);
                if resource {
                    self.facts["content"][index]["resource"]["text"] = bounded.into();
                } else {
                    self.facts["content"][index]["text"] = bounded.into();
                }
            }
        }
        let facts = std::mem::take(&mut self.facts);
        *self = make(facts, (&[], &[]))?;
        let served = self.texts.iter().map(String::len).sum::<usize>();
        if served > crate::tools::output::SERVED_CAP
            || text_facts_bytes(&self.facts) > crate::tools::output::SERVED_CAP
        {
            return Err(ResultError::BadResult);
        }
        Ok(prepared.logging_failed)
    }

    pub(crate) fn estimated_tokens(&self) -> u64 {
        let text: u64 = self
            .texts
            .iter()
            .map(|s| (s.encode_utf16().count() as u64 + 2) / 4)
            .sum();
        self.facts["content"]
            .as_array()
            .expect("validated content")
            .iter()
            .fold(text, |n, part| {
                let (mime, bytes) = match part["type"].as_str() {
                    Some("image" | "audio") => (part["mimeType"].as_str(), part["data"].as_str()),
                    Some("resource") if part["resource"]["blob"].is_string() => (
                        part["resource"]["mimeType"].as_str(),
                        part["resource"]["blob"].as_str(),
                    ),
                    _ => return n,
                };
                n.saturating_add(1_500).saturating_add(
                    if mime.is_some_and(|m| m.starts_with("image/")) {
                        0
                    } else {
                        bytes.map_or(0, |b| b.len() as u64 / 4)
                    },
                )
            })
    }

    pub(crate) fn needs_native_log(&self) -> bool {
        self.media
            || self.facts.get("_meta").is_some()
            || self.facts["content"].as_array().is_some_and(|parts| {
                parts.iter().any(|p| {
                    p.get("_meta").is_some()
                        || p.get("annotations").is_some()
                        || p["resource"].get("_meta").is_some()
                })
            })
    }

    /// Only the native turn-log attachment decoder calls this. Arbitrary provider
    /// items and ordinary string outputs never enter this path.
    pub(crate) fn from_stored(facts: Value) -> Result<Self, ResultError> {
        make(facts, (&[], &[]))
    }

    fn wire_parts(&self) -> Vec<Value> {
        let mut output = Vec::new();
        for part in self.facts["content"].as_array().expect("validated content") {
            match part["type"].as_str().expect("validated type") {
                "text" => output.push(json!({"type":"input_text","text":part["text"]})),
                "image" | "audio" => output.push(media_part(part["mimeType"].as_str().expect("validated MIME"), part["data"].as_str().expect("validated data"))),
                "resource" if part["resource"]["blob"].is_string() => {
                    let resource = &part["resource"];
                    output.push(media_part(resource["mimeType"].as_str().unwrap_or("application/octet-stream"), resource["blob"].as_str().expect("validated blob")));
                }
                "resource" => output.push(json!({"type":"input_text","text":json!({"resource":part["resource"]}).to_string()})),
                "resource_link" => output.push(json!({"type":"input_text","text":part.to_string()})),
                _ => unreachable!("validated native content"),
            }
        }
        if let Some(structured) = self.facts.get("structuredContent") {
            output.push(json!({"type":"input_text","text":json!({"structuredContent":structured}).to_string()}));
        }
        output
    }
}

fn native_text(part: &Value) -> Option<&str> {
    match part["type"].as_str() {
        Some("text") => part["text"].as_str(),
        Some("resource") => part["resource"]["text"].as_str(),
        _ => None,
    }
}

fn control_text(key: Option<&str>) -> bool {
    matches!(
        key,
        Some(
            "status"
                | "state"
                | "id"
                | "name"
                | "uri"
                | "mimeType"
                | "code"
                | "call_id"
                | "operation_id"
                | "session_id"
                | "turn_id"
        )
    )
}

fn data_text(value: &Value, key: Option<&str>, visit: &mut impl FnMut(&str)) {
    match value {
        Value::String(text) if !control_text(key) => visit(text),
        Value::Array(values) => {
            for value in values {
                data_text(value, None, visit)
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                data_text(value, Some(key), visit)
            }
        }
        _ => {}
    }
}

fn take_data_text(
    value: &mut Value,
    key: Option<&str>,
    joined: &mut String,
    remaining: &mut usize,
) {
    match value {
        Value::String(text) if !control_text(key) => {
            let mut source = std::mem::take(text);
            let cap = 2048.min(*remaining);
            if source.len() <= cap {
                *text = source.clone();
            } else if cap >= 48 {
                let end = source.floor_char_boundary(cap - 48);
                *text = format!(
                    "{} [text clipped; see registered tool text]",
                    &source[..end]
                );
            }
            *remaining = remaining.saturating_sub(text.len());
            if joined.is_empty() {
                *joined = source;
            } else if source.len() > joined.len() {
                source.insert(0, '\n');
                source.insert_str(0, joined);
                *joined = source;
            } else {
                joined.push('\n');
                joined.push_str(&source);
            }
        }
        Value::Array(values) => {
            for value in values {
                take_data_text(value, None, joined, remaining);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                take_data_text(value, Some(key), joined, remaining);
            }
        }
        _ => {}
    }
}

fn text_facts_bytes(facts: &Value) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, facts).expect("validated native facts");
    let media = facts["content"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|part| match part["type"].as_str() {
            Some("image" | "audio") => part["data"].as_str().map_or(0, str::len),
            Some("resource") if !part["resource"]["text"].is_string() => {
                part["resource"]["blob"].as_str().map_or(0, str::len)
            }
            _ => 0,
        })
        .sum::<usize>();
    count.0.saturating_sub(media)
}

fn media_part(mime: &str, data: &str) -> Value {
    let data_url = format!("data:{mime};base64,{data}");
    if mime.starts_with("image/") {
        json!({"type":"input_image","image_url":data_url})
    } else {
        json!({"type":"input_file","filename":if mime == "application/pdf" {"document.pdf"} else {"file"},"file_data":data_url})
    }
}

pub(crate) fn project_rich(
    result: rmcp::model::CallToolResult,
    secrets: &[String],
    identity_secrets: &[String],
) -> Result<McpToolOutput, ResultError> {
    make(
        serde_json::to_value(result).map_err(|_| ResultError::BadResult)?,
        (secrets, identity_secrets),
    )
}

fn bound(value: &impl Serialize) -> Result<(), ResultError> {
    if serde_json::to_vec(value)
        .map_err(|_| ResultError::BadResult)?
        .len()
        > crate::mcp_remote::RESULT_TEXT_BYTES_CAP
    {
        Err(ResultError::BadResult)
    } else {
        Ok(())
    }
}

fn exact(value: &str, secrets: &[String]) -> Result<(), ResultError> {
    if value.is_empty()
        || value.len() > IDENTITY_CAP
        || value.chars().any(char::is_control)
        || secrets.iter().any(|s| !s.is_empty() && value.contains(s))
    {
        return Err(ResultError::BadResult);
    }
    Ok(())
}

fn mime(value: &str, secrets: &[String]) -> Result<(), ResultError> {
    exact(value, secrets)?;
    let Some((kind, subtype)) = value.split_once('/') else {
        return Err(ResultError::BadResult);
    };
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    if !token(kind) || !token(subtype) {
        return Err(ResultError::BadResult);
    }
    Ok(())
}

fn binary(data: &str, secrets: &[String]) -> Result<(), ResultError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| ResultError::BadResult)?;
    // Compare actual decoded bytes, as on the existing lookup port. Short
    // ordinary environment values can coincide with base64 alphabet characters
    // without being present in the binary resource itself.
    if secrets
        .iter()
        .any(|s| !s.is_empty() && bytes.windows(s.len()).any(|b| b == s.as_bytes()))
    {
        return Err(ResultError::BadResult);
    }
    Ok(())
}

fn make(
    mut facts: Value,
    (secrets, identity_secrets): (&[String], &[String]),
) -> Result<McpToolOutput, ResultError> {
    bound(&facts)?;
    if facts["isError"] == true {
        return Err(ResultError::Failed(failure_detail(&facts)));
    }
    // Validate the SDK shape again on durable decode; no untyped modern content
    // may become a successful native result at the restart boundary.
    serde_json::from_value::<rmcp::model::CallToolResult>(facts.clone())
        .map_err(|_| ResultError::BadResult)?;
    let mut media = false;
    let parts = facts["content"]
        .as_array_mut()
        .ok_or(ResultError::BadResult)?;
    if parts.len() > PARTS_CAP {
        return Err(ResultError::BadResult);
    }
    for part in parts {
        match part["type"].as_str().ok_or(ResultError::BadResult)? {
            "text" => {
                let text = part["text"].as_str().ok_or(ResultError::BadResult)?;
                part["text"] = redact(text, secrets).into();
            }
            "image" | "audio" => {
                let kind = part["type"].as_str().expect("matched type");
                let m = part["mimeType"].as_str().ok_or(ResultError::BadResult)?;
                mime(m, identity_secrets)?;
                if !m.starts_with(&format!("{kind}/")) {
                    return Err(ResultError::BadResult);
                }
                binary(
                    part["data"].as_str().ok_or(ResultError::BadResult)?,
                    secrets,
                )?;
                media = true;
            }
            "resource" => {
                let resource = &mut part["resource"];
                exact(
                    resource["uri"].as_str().ok_or(ResultError::BadResult)?,
                    identity_secrets,
                )?;
                if let Some(m) = resource.get("mimeType") {
                    mime(m.as_str().ok_or(ResultError::BadResult)?, identity_secrets)?;
                }
                if let Some(blob) = resource.get("blob") {
                    binary(blob.as_str().ok_or(ResultError::BadResult)?, secrets)?;
                    if resource.get("text").is_some() {
                        return Err(ResultError::BadResult);
                    }
                    media = true;
                } else {
                    resource["text"] = redact(
                        resource["text"].as_str().ok_or(ResultError::BadResult)?,
                        secrets,
                    )
                    .into();
                }
                if let Some(meta) = resource.get_mut("_meta") {
                    redact_json(meta, secrets);
                }
            }
            "resource_link" => {
                exact(
                    part["uri"].as_str().ok_or(ResultError::BadResult)?,
                    identity_secrets,
                )?;
                exact(
                    part["name"].as_str().ok_or(ResultError::BadResult)?,
                    identity_secrets,
                )?;
                if let Some(m) = part.get("mimeType") {
                    mime(m.as_str().ok_or(ResultError::BadResult)?, identity_secrets)?;
                }
                for field in ["description", "title"] {
                    if let Some(text) = part.get_mut(field) {
                        *text =
                            redact(text.as_str().ok_or(ResultError::BadResult)?, secrets).into();
                    }
                }
            }
            _ => return Err(ResultError::Unsupported),
        }
        if let Some(meta) = part.get_mut("_meta") {
            redact_json(meta, secrets);
        }
        // Structural audience/priority are protocol facts, not redacted prose.
        if let Some(annotations) = part.get("annotations")
            && let Some(timestamp) = annotations.get("lastModified")
        {
            // Timestamp is a protocol fact, not prose: do not manufacture an
            // invalid synthetic date by replacing part of its exact value.
            exact(
                timestamp.as_str().ok_or(ResultError::BadResult)?,
                identity_secrets,
            )?;
        }
    }
    for field in ["structuredContent", "_meta"] {
        if let Some(value) = facts.get_mut(field) {
            redact_json(value, secrets);
        }
    }
    let display = if media {
        let mut prose = Vec::new();
        for part in facts["content"].as_array().expect("validated content") {
            prose.push(match part["type"].as_str().expect("validated type") {
                "text" => part["text"].as_str().expect("validated text").to_owned(),
                "image" | "audio" => format!(
                    "[MCP media: {}]",
                    part["mimeType"].as_str().expect("validated MIME")
                ),
                "resource" if part["resource"]["blob"].is_string() => {
                    "[MCP embedded binary resource]".into()
                }
                "resource" => json!({"resource":part["resource"]}).to_string(),
                _ => part.to_string(),
            });
        }
        if let Some(structured) = facts.get("structuredContent") {
            prose.push(json!({"structuredContent":structured}).to_string());
        }
        let mut text = prose.join("\n");
        if text.len() > PRESENTATION_CAP {
            let marker = "\n[MCP result presentation truncated; native content retained]";
            text.truncate(text.floor_char_boundary(PRESENTATION_CAP - marker.len()));
            text.push_str(marker);
        }
        text
    } else {
        super::project(
            serde_json::from_value(facts.clone()).map_err(|_| ResultError::BadResult)?,
            &[],
        )?
    };
    let mut output = McpToolOutput {
        facts,
        display,
        media,
        texts: Vec::new(),
    };
    output.texts = if media {
        output
            .wire_parts()
            .into_iter()
            .filter_map(|p| p["text"].as_str().map(str::to_owned))
            .collect()
    } else {
        vec![output.display.clone()]
    };
    bound(&output.facts)?;
    bound(&output)?;
    if output.needs_native_log() {
        bound(&(output.facts(), output.display()))?;
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
