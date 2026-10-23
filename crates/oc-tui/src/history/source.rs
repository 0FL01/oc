//! Immutable presentation source. Cloning a render row shares its payload;
//! the monotonic token cannot alias a subsequently allocated source.
use std::{
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

#[derive(Debug, Clone)]
pub struct SourceText(Arc<Source>);

#[derive(Debug)]
struct Source {
    token: u64,
    text: String,
    reasoning: std::sync::OnceLock<(std::ops::Range<usize>, bool)>,
    preview_end: Option<usize>,
}

impl Clone for Source {
    fn clone(&self) -> Self {
        Self {
            token: self.token,
            text: self.text.clone(),
            reasoning: std::sync::OnceLock::new(),
            preview_end: self.preview_end,
        }
    }
}

impl SourceText {
    pub(crate) fn token(&self) -> u64 {
        self.0.token
    }
    pub fn as_str(&self) -> &str {
        self
    }
    pub(crate) fn preview(&self, limit: usize, marker: &str) -> Self {
        if self.len() <= limit {
            return self.clone();
        }
        let mut end = limit;
        while !self.is_char_boundary(end) {
            end -= 1;
        }
        Self(Arc::new(Source {
            token: next_token(),
            text: format!("{}{marker}", &self[..end]),
            reasoning: std::sync::OnceLock::new(),
            preview_end: Some(end),
        }))
    }
    pub(crate) fn completion_prefix(&self, limit: usize) -> &str {
        let mut end = self.0.preview_end.unwrap_or(self.len()).min(limit);
        while !self.is_char_boundary(end) {
            end -= 1;
        }
        &self[..end]
    }
    pub(crate) fn reasoning_preview(&self, mut limit: usize, marker: &str) -> Self {
        // Do not turn a redaction sentinel split by the byte limit into public
        // partial text. Inspect only the bounded boundary, never the full body.
        const REDACTED: &str = "[REDACTED]";
        if self.len() > limit {
            let start = limit.saturating_sub(REDACTED.len() - 1);
            if let Some(offset) = self.as_bytes()[start..limit]
                .iter()
                .rposition(|byte| *byte == b'[')
            {
                let at = start + offset;
                if self.get(at..at + REDACTED.len()) == Some(REDACTED) {
                    limit = at;
                }
            }
        }
        self.preview(limit, marker)
    }
    pub(crate) fn reasoning_content(&self) -> std::borrow::Cow<'_, str> {
        let (range, redacted) = self.reasoning_info();
        if *redacted {
            std::borrow::Cow::Owned(self.replace("[REDACTED]", "").trim().to_owned())
        } else {
            std::borrow::Cow::Borrowed(&self[range.clone()])
        }
    }
    fn reasoning_info(&self) -> &(std::ops::Range<usize>, bool) {
        self.0.reasoning.get_or_init(|| {
            if let Some(end) = self.0.preview_end
                && self[..end].replace("[REDACTED]", "").trim().is_empty()
            {
                // A generated omission label is not a public reasoning step.
                return (0..0, false);
            }
            let trimmed = self.trim();
            let start = self.len() - self.trim_start().len();
            (start..start + trimmed.len(), self.contains("[REDACTED]"))
        })
    }
    pub(crate) fn reasoning_bounds(&self) -> Option<std::ops::Range<usize>> {
        let (bounds, redacted) = self.reasoning_info();
        (!redacted).then(|| bounds.clone())
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    fn edit(&mut self) -> &mut String {
        let source = Arc::make_mut(&mut self.0);
        source.token = next_token();
        source.reasoning.take();
        source.preview_end = None;
        &mut source.text
    }
    pub fn push(&mut self, ch: char) {
        self.edit().push(ch);
    }
    pub fn push_str(&mut self, value: &str) {
        self.edit().push_str(value);
    }
}

fn next_token() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl From<SourceText> for String {
    fn from(source: SourceText) -> Self {
        Arc::try_unwrap(source.0).map_or_else(|source| source.text.clone(), |source| source.text)
    }
}

impl From<String> for SourceText {
    fn from(text: String) -> Self {
        Self(Arc::new(Source {
            token: next_token(),
            text,
            reasoning: std::sync::OnceLock::new(),
            preview_end: None,
        }))
    }
}
impl From<&str> for SourceText {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}
impl Default for SourceText {
    fn default() -> Self {
        String::new().into()
    }
}
impl Deref for SourceText {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0.text
    }
}
impl PartialEq for SourceText {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0) || **self == **other
    }
}
impl Eq for SourceText {}
impl PartialEq<str> for SourceText {
    fn eq(&self, other: &str) -> bool {
        **self == *other
    }
}
impl PartialEq<&str> for SourceText {
    fn eq(&self, other: &&str) -> bool {
        **self == **other
    }
}
impl PartialEq<String> for SourceText {
    fn eq(&self, other: &String) -> bool {
        **self == **other
    }
}
impl std::fmt::Display for SourceText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_bytes_and_edits_fence_cached_source_and_reasoning() {
        let mut source: SourceText = "  thought  ".into();
        let old = source.clone();
        assert_eq!(source.as_ptr(), old.as_ptr());
        assert_eq!(source.reasoning_content(), "thought");
        let cleaned = source.reasoning_content().as_ptr();
        assert_eq!(source.reasoning_content().as_ptr(), cleaned);
        source.push_str(" [REDACTED] next");
        assert_ne!(source.token(), old.token());
        assert_ne!(source.as_ptr(), old.as_ptr());
        assert_eq!(old.reasoning_content(), "thought");
        assert_eq!(source.reasoning_content(), "thought    next");
        assert_ne!(source.token(), SourceText::from(source.as_str()).token());
    }

    #[test]
    fn generated_preview_marker_does_not_make_redacted_or_blank_reasoning_visible() {
        for text in ["[REDACTED]".repeat(4000), " ".repeat(40_000)] {
            let source: SourceText = text.into();
            let preview = source.reasoning_preview(16 * 1024, "\n\n… [reasoning preview limited]");
            assert!(preview.len() < 17 * 1024);
            assert!(preview.reasoning_content().is_empty());
            assert_eq!(preview.reasoning_bounds(), Some(0..0));
        }
    }
}
