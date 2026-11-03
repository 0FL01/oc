//! Image-only request projection. RAW sources stay immutable and are loaded one
//! bounded journal at a time; deferred locators never serialize as model input.
use crate::{
    provider::{InputContent, InputItem, ProjectedImageKind, ProjectedToolPart},
    storage::{Db, StorageError, WireLogSource},
    tools::TurnLog,
};
use serde_json::{Value, json};
use sha2::Digest as _;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::Arc,
};

const TRIGGER: usize = 25 * 1024 * 1024;
const TARGET: usize = 15 * 1024 * 1024;
const REMOVED: &str = "[This image was removed to reduce the request size and is no longer visible. Do not make claims about its contents from memory. If needed, retrieve it again with an available tool or ask the user to attach it again.]";

/// Opaque, request-owned locator into an already validated journal. It cannot be
/// authored by provider/user JSON and contains no image payload or new grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageSource {
    log: Arc<WireLogSource>,
    digest: [u8; 32],
    bytes: usize,
    json_bytes: u64,
}

fn uri_json_bytes(url: &str) -> u64 {
    struct Count(u64);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len() as u64;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, url).expect("string counter cannot fail");
    count.0 - 2
}

fn digest(url: &str) -> [u8; 32] {
    sha2::Sha256::digest(url.as_bytes()).into()
}

fn media_bytes(url: &str) -> usize {
    url.split_once(";base64,")
        .filter(|(head, _)| head.to_ascii_lowercase().starts_with("data:image/"))
        .map_or(0, |(_, data)| data.len())
}

fn image_field(part: &Value) -> Option<&'static str> {
    match part["type"].as_str() {
        Some("input_image") if part["image_url"].is_string() => Some("image_url"),
        Some("input_file")
            if part["file_data"].as_str().is_some_and(|url| {
                url.get(..11)
                    .is_some_and(|head| head.eq_ignore_ascii_case("data:image/"))
            }) =>
        {
            Some("file_data")
        }
        _ => None,
    }
}

fn tool_parts(item: &InputItem) -> Result<Option<Vec<Value>>, StorageError> {
    match item {
        InputItem::ReadFunctionCallOutput { .. }
        | InputItem::QuestionFunctionCallOutput { .. }
        | InputItem::McpFunctionCallOutput { .. } => {}
        InputItem::ProviderOutput(value)
            if value["type"] == "function_call_output" && value["call_id"].is_string() => {}
        _ => return Ok(None),
    }
    let mut value = serde_json::to_value(item).map_err(|_| StorageError::CompressionConflict)?;
    let Value::Array(parts) = value["output"].take() else {
        return Ok(None);
    };
    Ok(Some(parts))
}

fn normalize_tools(input: &mut [InputItem]) -> Result<(), StorageError> {
    for item in input {
        if let Some(parts) = tool_parts(item)?
            && parts.iter().any(|part| image_field(part).is_some())
        {
            let kind = match item {
                InputItem::ReadFunctionCallOutput { .. } => Some(ProjectedImageKind::Read),
                InputItem::QuestionFunctionCallOutput { .. } => Some(ProjectedImageKind::Question),
                InputItem::McpFunctionCallOutput { .. } => Some(ProjectedImageKind::Mcp),
                _ => None,
            };
            let (call_id, display) = item.call_output().unwrap_or_else(|| {
                let InputItem::ProviderOutput(value) = &*item else {
                    unreachable!()
                };
                (value["call_id"].as_str().expect("output identity"), "")
            });
            let texts = match &*item {
                InputItem::McpFunctionCallOutput { output, .. } => output.texts().to_vec(),
                _ if kind.is_some() => vec![display.to_owned()],
                _ => Vec::new(),
            };
            *item = InputItem::ProjectedFunctionCallOutput {
                call_id: call_id.to_owned(),
                display: display.to_owned(),
                kind,
                texts,
                output: parts.into_iter().map(ProjectedToolPart::Ready).collect(),
            };
        }
    }
    Ok(())
}

/// Called only after the native attachment graph has been validated. Compatible
/// filtering may reorder/omit parts, so locators bind the exact URI digest, not
/// a new positional identity or a marker parsed from untrusted text.
pub(crate) fn defer_images(
    input: &mut [InputItem],
    log: Arc<WireLogSource>,
) -> Result<(), StorageError> {
    normalize_tools(input)?;
    for item in input {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    if let InputContent::InputImage { image_url, detail } = part {
                        *part = InputContent::DeferredImage {
                            source: Arc::new(ImageSource {
                                log: log.clone(),
                                digest: digest(image_url),
                                bytes: media_bytes(image_url),
                                json_bytes: uri_json_bytes(image_url),
                            }),
                            detail: detail.clone(),
                        };
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, .. } => {
                for part in output {
                    if let ProjectedToolPart::Ready(value) = part
                        && let Some(field) = image_field(value)
                    {
                        let url = value[field].take();
                        let url = url.as_str().expect("image URI");
                        let source = Arc::new(ImageSource {
                            log: log.clone(),
                            digest: digest(url),
                            bytes: url.len(),
                            json_bytes: uri_json_bytes(url),
                        });
                        *part = ProjectedToolPart::DeferredImage {
                            source,
                            template: value.take(),
                            field,
                        };
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Size-only view: leave empty URI slots and return their exact escaped byte
/// contribution. DCP prices the original projection without loading its images;
/// metadata admission ignores this contribution. This view is never dispatched.
pub(crate) fn image_measurement(input: &mut [InputItem]) -> Result<u64, StorageError> {
    normalize_tools(input)?;
    let mut bytes = 0u64;
    for item in input {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    match part {
                        InputContent::DeferredImage { source, detail } => {
                            bytes = bytes.saturating_add(source.json_bytes);
                            *part = InputContent::InputImage {
                                image_url: String::new(),
                                detail: detail.clone(),
                            };
                        }
                        InputContent::InputImage { image_url, .. } => {
                            bytes = bytes.saturating_add(uri_json_bytes(image_url));
                            image_url.clear();
                        }
                        _ => {}
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, .. } => {
                for part in output {
                    match part {
                        ProjectedToolPart::DeferredImage {
                            source,
                            template,
                            field,
                        } => {
                            bytes = bytes.saturating_add(source.json_bytes);
                            template[*field] = "".into();
                            *part = ProjectedToolPart::Ready(template.take());
                        }
                        ProjectedToolPart::Ready(value) => {
                            if let Some(field) = image_field(value) {
                                bytes = bytes.saturating_add(uri_json_bytes(
                                    value[field].as_str().expect("image URI"),
                                ));
                                value[field] = "".into();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(bytes)
}

/// TS cache-stable prefix replay, after unsupported-media and DCP filtering.
pub(crate) fn project_images(
    input: &mut [InputItem],
    db: &Db,
    vision: bool,
    chat: bool,
) -> Result<(), StorageError> {
    super::project_media(input, vision, chat);
    normalize_tools(input)?;
    let mut sizes = Vec::new();
    for item in input.iter() {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    match part {
                        InputContent::InputImage { image_url, .. } => {
                            sizes.push(media_bytes(image_url))
                        }
                        InputContent::DeferredImage { source, .. } => sizes.push(source.bytes),
                        _ => {}
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, .. } => {
                for part in output {
                    match part {
                        ProjectedToolPart::DeferredImage { source, .. } => sizes.push(source.bytes),
                        ProjectedToolPart::Ready(value) => {
                            if let Some(field) = image_field(value) {
                                sizes.push(value[field].as_str().expect("image URI").len());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let (mut kept, mut cut) = (0usize, 0usize);
    for size in &sizes {
        kept = kept.saturating_add(*size);
        if kept > TRIGGER {
            while kept > TARGET {
                kept -= sizes[cut];
                cut += 1;
            }
        }
    }
    let mut seen = 0;
    for item in input.iter_mut() {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    if matches!(
                        part,
                        InputContent::InputImage { .. } | InputContent::DeferredImage { .. }
                    ) {
                        if seen < cut {
                            *part = InputContent::InputText {
                                text: REMOVED.into(),
                            };
                        }
                        seen += 1;
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, texts, .. } => {
                for part in output {
                    let image = match part {
                        ProjectedToolPart::DeferredImage { .. } => true,
                        ProjectedToolPart::Ready(value) => image_field(value).is_some(),
                    };
                    if image {
                        if seen < cut {
                            *part = ProjectedToolPart::Ready(
                                json!({"type":"input_text","text":REMOVED}),
                            );
                            texts.push(REMOVED.into());
                        }
                        seen += 1;
                    }
                }
            }
            _ => {}
        }
    }
    resolve(input, db)
}

fn visit_urls(input: &[InputItem], visit: &mut impl FnMut(&str)) -> Result<(), StorageError> {
    for item in input {
        if let InputItem::Message { content, .. } = item {
            for part in content {
                if let InputContent::InputImage { image_url, .. } = part {
                    visit(image_url);
                }
            }
        } else if let Some(parts) = tool_parts(item)? {
            for part in parts {
                if let Some(field) = image_field(&part) {
                    visit(part[field].as_str().expect("image URI"));
                }
            }
        }
    }
    Ok(())
}

fn visit_log_urls(log: &TurnLog, visit: &mut impl FnMut(&str)) -> Result<(), StorageError> {
    // Match input_for_bound's existing handling of a legacy unusable working view.
    if let Some(working) = &log.working
        && let Ok(working) = TurnLog::from_json(working)
    {
        visit_log_urls(&working, visit)?;
    }
    visit_urls(&log.input, visit)
}

fn resolve(input: &mut [InputItem], db: &Db) -> Result<(), StorageError> {
    let mut groups: BTreeMap<usize, (Arc<WireLogSource>, BTreeSet<[u8; 32]>)> = BTreeMap::new();
    let mut add = |source: &Arc<ImageSource>| {
        groups
            .entry(Arc::as_ptr(&source.log) as usize)
            .or_insert_with(|| (source.log.clone(), BTreeSet::new()))
            .1
            .insert(source.digest);
    };
    for item in input.iter() {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    if let InputContent::DeferredImage { source, .. } = part {
                        add(source);
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, .. } => {
                for part in output {
                    if let ProjectedToolPart::DeferredImage { source, .. } = part {
                        add(source);
                    }
                }
            }
            _ => {}
        }
    }
    let mut urls = HashMap::new();
    for (key, (source, wanted)) in groups {
        let raw = source.load(db)?.ok_or(StorageError::CompressionConflict)?;
        let value = serde_json::from_str(&raw).map_err(|_| StorageError::CompressionConflict)?;
        let log = TurnLog::from_json(&value).map_err(|_| StorageError::CompressionConflict)?;
        visit_log_urls(&log, &mut |url| {
            let digest = digest(url);
            if wanted.contains(&digest) {
                urls.insert((key, digest), url.to_owned());
            }
        })?;
    }
    let get = |source: &Arc<ImageSource>| {
        urls.get(&(Arc::as_ptr(&source.log) as usize, source.digest))
            .cloned()
            .ok_or(StorageError::CompressionConflict)
    };
    for item in input {
        match item {
            InputItem::Message { content, .. } => {
                for part in content {
                    if let InputContent::DeferredImage { source, detail } = part {
                        *part = InputContent::InputImage {
                            image_url: get(source)?,
                            detail: detail.clone(),
                        };
                    }
                }
            }
            InputItem::ProjectedFunctionCallOutput { output, .. } => {
                for part in output {
                    if let ProjectedToolPart::DeferredImage {
                        source,
                        template,
                        field,
                    } = part
                    {
                        template[*field] = get(source)?.into();
                        *part = ProjectedToolPart::Ready(template.take());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: usize) -> InputItem {
        InputItem::Message {
            role: crate::provider::InputRole::User,
            content: vec![InputContent::InputImage {
                image_url: format!("data:IMAGE/PNG;base64,{}", "A".repeat(bytes)),
                detail: None,
            }],
        }
    }

    #[test]
    fn w3_image_budget_replays_strict_trigger_and_keeps_cache_stable_prefix() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let mut input = vec![InputItem::Message {
            role: crate::provider::InputRole::User,
            content: vec![InputContent::InputImage {
                image_url: "https://example.test/image".into(),
                detail: None,
            }],
        }];
        input.extend((0..9).map(|_| image(5 * 1024 * 1024)));
        project_images(&mut input, &db, true, false).unwrap();
        // Two trigger crossings: not a single final-total trim to 15 MiB.
        assert!(
            input[..7]
                .iter()
                .all(|item| matches!(item, InputItem::Message { content, .. }
            if matches!(&content[0], InputContent::InputText { text } if text == REMOVED)))
        );
        assert!(
            input[7..]
                .iter()
                .all(|item| matches!(item, InputItem::Message { content, .. }
            if matches!(&content[0], InputContent::InputImage { .. })))
        );
    }

    #[test]
    fn w3_image_budget_counts_tool_uri_and_preserves_other_parts_without_native_upgrade() {
        let root = tempfile::tempdir().unwrap();
        let db = Db::open(root.path()).unwrap();
        let first = format!("data:image/png;base64,{}", "A".repeat(6 * 1024 * 1024));
        let kept = format!("data:IMAGE/PNG;base64,{}", "B".repeat(1024 * 1024));
        let file = json!({"type":"input_file","filename":"report.pdf","file_data":"data:application/pdf;base64,ZmlsZQ=="});
        let structured = json!({"type":"input_text","text":"{\"status\":\"unchanged\"}"});
        let mut input = vec![
            image(5 * 1024 * 1024),
            InputItem::ProviderOutput(json!({
                "type":"function_call_output","call_id":"opaque","output":[
                    {"type":"input_text","text":"original text"},
                    {"type":"input_image","image_url":first}, file.clone(),
                    {"type":"input_file","filename":"image.png","file_data":kept}, structured.clone()
                ]
            })),
            image(6 * 1024 * 1024),
            image(7 * 1024 * 1024),
        ];
        let before = serde_json::to_vec(&input).unwrap().len() as u64;
        let mut measurement = input.clone();
        let images = image_measurement(&mut measurement).unwrap();
        assert_eq!(
            serde_json::to_vec(&measurement).unwrap().len() as u64 + images,
            before
        );
        let escaped_uri = "https://example.test/雪/\"\\\n";
        assert_eq!(
            uri_json_bytes(escaped_uri) + 2,
            serde_json::to_string(escaped_uri).unwrap().len() as u64
        );
        // Base64 alone totals exactly 25 MiB. Tool URI overhead crosses
        // the trigger, removing the oldest message and only the first tool image.
        project_images(&mut input, &db, true, false).unwrap();
        assert!(input[1].call_output().is_none());
        let value = serde_json::to_value(&input[1]).unwrap();
        assert_eq!(value["output"][0]["text"], "original text");
        assert_eq!(
            value["output"][1],
            json!({"type":"input_text","text":REMOVED})
        );
        assert_eq!(value["output"][2], file);
        assert_eq!(
            value["output"][3]["file_data"].as_str(),
            Some(kept.as_str())
        );
        assert_eq!(value["output"][4], structured);
        assert!(matches!(&input[0], InputItem::Message {content,..}
            if matches!(&content[0],InputContent::InputText {text} if text==REMOVED)));
    }
}
