//! Prompt normalization derived from Opencode-DCP 3.2.0, `lib/prompts/store.ts`,
//! revision d637981555a18c3992472268a0657a948925d5fa, AGPL-3.0-or-later.
//! Full license: evidence/tui/recovery-v00/dcp-oracle20260928-native01/sources/LICENSE.
//! Native defaults and admitted roots stay native; no managed files are written.

use std::sync::{Arc, LazyLock};

use regex::Regex;

/// Optional admitted prompt bodies, shared by immutable request publications.
/// None preserves the existing native text (or absent system block).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct DcpPromptOverrides {
    /// Additional DCP system guidance, not the agent's system prompt.
    pub system: Option<Arc<str>>,
    /// Editable range-tool instructions; native schema/anchor guidance is separate.
    pub compress_range: Option<Arc<str>>,
    /// Reminder body when context is above the maximum threshold.
    pub context_limit_nudge: Option<Arc<str>>,
    /// Reminder body at a qualifying new user turn.
    pub turn_nudge: Option<Arc<str>>,
    /// Reminder body after a qualifying sequence of assistant iterations.
    pub iteration_nudge: Option<Arc<str>>,
}

impl std::fmt::Debug for DcpPromptOverrides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Prompt contents must not leak through DcpConfig diagnostics/debug output.
        f.debug_struct("DcpPromptOverrides").finish_non_exhaustive()
    }
}

/// The filename is one of the five fixed names supplied by DCP admission.
pub(crate) fn normalize(raw: &str, name: &str) -> Option<Arc<str>> {
    static COMMENTS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").expect("DCP comment pattern"));
    static LEGACY_COMMENTS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?m)^[ \t]*//.*?//[ \t]*$").expect("DCP legacy comment pattern")
    });
    static CONDITIONALS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?is)<manual>.*?</manual>|<subagent>.*?</subagent>")
            .expect("DCP conditional pattern")
    });
    static OPEN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^<dcp-system-reminder\b[^>]*>\s*").expect("DCP opening pattern")
    });
    static CLOSE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)\s*</dcp-system-reminder>$").expect("DCP closing pattern")
    });

    let normalized = raw
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let normalized = COMMENTS.replace_all(&normalized, "");
    let normalized = LEGACY_COMMENTS.replace_all(&normalized, "");
    let normalized = if name == "system.md" {
        CONDITIONALS.replace_all(&normalized, "")
    } else {
        normalized
    };
    let mut text = normalized.trim();
    if name == "compress-range.md" {
        return (!text.is_empty()).then(|| Arc::from(text));
    }
    match (OPEN.find(text), CLOSE.find(text)) {
        (Some(open), Some(close)) if open.end() <= close.start() => {
            text = text[open.end()..close.start()].trim();
        }
        (None, None) => {}
        _ => return None,
    }
    (!text.is_empty()).then(|| {
        Arc::from(format!(
            "<dcp-system-reminder>\n{text}\n</dcp-system-reminder>"
        ))
    })
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
