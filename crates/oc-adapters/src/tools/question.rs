//! Typed question answers carrying pasted images; the images are user input
//! for this tool result only, never text, MCP or local-file origins.
use base64::Engine as _;
use oc_core::{
    prompt::{IMAGE_BYTES, IMAGES_TOTAL_BYTES},
    question::{IMAGES_CAP, QuestionImage},
};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};
use std::sync::atomic::AtomicBool;

/// Serialization lowers to Responses content; the turn owner persists the
/// image facts beside the textual result before continuation.
#[derive(Clone, PartialEq, Eq)]
pub struct QuestionToolOutput {
    facts: Value,
    display: String,
}
impl std::fmt::Debug for QuestionToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuestionToolOutput").finish_non_exhaustive()
    }
}
impl Serialize for QuestionToolOutput {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut content = vec![crate::provider::InputContent::InputText {
            text: self.display.clone(),
        }];
        for image in self.images() {
            content.push(crate::provider::InputContent::InputText {
                text: format!(
                    "Question {} answer [Image {}]:",
                    image["question"].as_u64().expect("validated question") + 1,
                    image["ordinal"].as_u64().expect("validated ordinal"),
                ),
            });
            content.push(crate::provider::InputContent::InputImage {
                image_url: format!(
                    "data:image/png;base64,{}",
                    image["data"].as_str().expect("validated data")
                ),
                detail: None,
            });
        }
        content.serialize(serializer)
    }
}
impl QuestionToolOutput {
    /// Verify every pasted PNG before it can reach a provider request.
    pub(crate) fn new(
        display: String,
        images: &[QuestionImage],
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let images: Vec<_> = images
            .iter()
            .map(|image| {
                json!({
                    "question": image.question,
                    "answer": image.answer,
                    "ordinal": image.mark.ordinal,
                    "mime": "image/png",
                    "bytes": image.png.len(),
                    "data": base64::engine::general_purpose::STANDARD.encode(&image.png),
                })
            })
            .collect();
        Self::verify(json!({ "images": images }), display, cancel)
    }
    pub(crate) fn from_stored(facts: Value, display: String) -> Result<Self, String> {
        // Stored bytes independently satisfy the same bounded validation.
        Self::verify(facts, display, &AtomicBool::new(false))
    }
    fn verify(facts: Value, display: String, cancel: &AtomicBool) -> Result<Self, String> {
        let invalid = || "invalid native question facts".to_owned();
        let object = facts.as_object().ok_or_else(invalid)?;
        let images = object
            .get("images")
            .and_then(Value::as_array)
            .filter(|images| object.len() == 1 && (1..=IMAGES_CAP).contains(&images.len()))
            .ok_or_else(invalid)?;
        let mut total = 0usize;
        for (index, image) in images.iter().enumerate() {
            let fields = image.as_object().ok_or_else(invalid)?;
            if fields.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "question" | "answer" | "ordinal" | "mime" | "bytes" | "data"
                )
            }) || ["question", "answer", "ordinal", "bytes"]
                .iter()
                .any(|key| fields.get(*key).and_then(Value::as_u64).is_none())
                || image["ordinal"] == 0
                || image["mime"] != "image/png"
                || images[..index].iter().any(|p| {
                    (&p["question"], &p["answer"], &p["ordinal"])
                        == (&image["question"], &image["answer"], &image["ordinal"])
                })
            {
                return Err(invalid());
            }
            let data = image["data"].as_str().ok_or_else(invalid)?;
            if data.len() > IMAGE_BYTES.div_ceil(3) * 4 {
                return Err(invalid());
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| invalid())?;
            total = total.saturating_add(bytes.len());
            if bytes.is_empty()
                || bytes.len() > IMAGE_BYTES
                || total > IMAGES_TOTAL_BYTES
                || image["bytes"].as_u64() != Some(bytes.len() as u64)
            {
                return Err(invalid());
            }
            crate::attachments::validate_png(&bytes, cancel)
                .map_err(|_| "pasted question image failed verification")?;
        }
        Ok(Self { facts, display })
    }
    fn images(&self) -> &[Value] {
        self.facts["images"].as_array().expect("validated images")
    }
    pub(crate) fn display(&self) -> &str {
        &self.display
    }
    pub(crate) fn facts(&self) -> &Value {
        &self.facts
    }
    /// Data URL bytes that the provider bills as media, not prompt text.
    pub(crate) fn media(&self) -> (usize, usize) {
        let images = self.images();
        let urls = images
            .iter()
            .map(|i| "data:image/png;base64,".len() + i["data"].as_str().map_or(0, str::len))
            .sum();
        (images.len(), urls)
    }
}

#[cfg(test)]
mod tests;
