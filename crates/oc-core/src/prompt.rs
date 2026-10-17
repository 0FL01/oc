//! Submitted user input, separate from provider wire and display projections.
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub const IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const IMAGES_TOTAL_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_IMAGES: usize = 1024;

/// An attachment's tracked range in the original UTF-8 draft (no trailing space).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageMark {
    pub start: usize,
    pub end: usize,
    pub ordinal: usize,
}

impl ImageMark {
    pub fn label(&self) -> String {
        format!("[Image {}]", self.ordinal)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct PromptImage {
    pub mark: ImageMark,
    pub png: Arc<[u8]>,
}

impl std::fmt::Debug for PromptImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PromptImage")
            .field("mark", &self.mark)
            .field("bytes", &self.png.len())
            .finish()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptInput {
    pub text: String,
    pub images: Vec<PromptImage>,
}

impl PromptInput {
    pub fn check_budget(&self) -> Result<(), crate::session::CoreError> {
        if self.text.len() > crate::session::MAX_INPUT_BYTES
            || self.images.len() > MAX_IMAGES
            || self
                .images
                .iter()
                .any(|image| image.png.len() > IMAGE_BYTES)
            || self
                .images
                .iter()
                .try_fold(0usize, |n, i| n.checked_add(i.png.len()))
                .is_none_or(|n| n > IMAGES_TOTAL_BYTES)
        {
            return Err(crate::session::CoreError::InputTooLarge);
        }
        Ok(())
    }

    pub fn trimmed(mut self) -> Self {
        let prefix = self.text.len() - self.text.trim_start().len();
        self.text = self.text.trim().to_owned();
        for image in &mut self.images {
            image.mark.start = image.mark.start.saturating_sub(prefix);
            image.mark.end = image.mark.end.saturating_sub(prefix);
        }
        self
    }
}

impl From<String> for PromptInput {
    fn from(text: String) -> Self {
        Self {
            text,
            images: Vec::new(),
        }
    }
}

impl From<&str> for PromptInput {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

impl std::ops::Deref for PromptInput {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

/// Small durable presentation facts; input_index is an original RAW coordinate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserImages {
    pub input_index: usize,
    pub images: Vec<ImageMark>,
}
