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

/// Prompt slot shared by admitted overrides and the built-in defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptSlot {
    /// DCP system guidance.
    System,
    /// `compress` tool instructions.
    CompressRange,
    /// Above the maximum threshold.
    ContextLimitNudge,
    /// At a qualifying new user turn.
    TurnNudge,
    /// After a qualifying sequence of assistant iterations.
    IterationNudge,
}

impl PromptSlot {
    fn file(self) -> &'static str {
        match self {
            Self::System => "system.md",
            Self::CompressRange => "compress-range.md",
            Self::ContextLimitNudge => "context-limit-nudge.md",
            Self::TurnNudge => "turn-nudge.md",
            Self::IterationNudge => "iteration-nudge.md",
        }
    }
}

impl DcpPromptOverrides {
    fn get(&self, slot: PromptSlot) -> Option<&Arc<str>> {
        match slot {
            PromptSlot::System => self.system.as_ref(),
            PromptSlot::CompressRange => self.compress_range.as_ref(),
            PromptSlot::ContextLimitNudge => self.context_limit_nudge.as_ref(),
            PromptSlot::TurnNudge => self.turn_nudge.as_ref(),
            PromptSlot::IterationNudge => self.iteration_nudge.as_ref(),
        }
    }

    /// Admitted override, else the built-in DCP 3.2.0 body. Like the donor,
    /// `customPrompts` only gates overrides; the defaults always apply.
    pub fn effective(&self, slot: PromptSlot) -> Arc<str> {
        self.get(slot)
            .cloned()
            .unwrap_or_else(|| default_prompt(slot))
    }
}

/// Built-in bodies from `assets/dcp` (see its PROVENANCE.md), normalized like overrides.
fn default_prompt(slot: PromptSlot) -> Arc<str> {
    static DEFAULTS: LazyLock<[Arc<str>; 5]> = LazyLock::new(|| {
        [
            PromptSlot::System,
            PromptSlot::CompressRange,
            PromptSlot::ContextLimitNudge,
            PromptSlot::TurnNudge,
            PromptSlot::IterationNudge,
        ]
        .map(|slot| {
            let raw = match slot {
                PromptSlot::System => include_str!("../../assets/dcp/system.md"),
                PromptSlot::CompressRange => include_str!("../../assets/dcp/compress-range.md"),
                PromptSlot::ContextLimitNudge => {
                    include_str!("../../assets/dcp/context-limit-nudge.md")
                }
                PromptSlot::TurnNudge => include_str!("../../assets/dcp/turn-nudge.md"),
                PromptSlot::IterationNudge => include_str!("../../assets/dcp/iteration-nudge.md"),
            };
            normalize(raw, slot.file()).expect("built-in DCP prompt")
        })
    });
    let index = match slot {
        PromptSlot::System => 0,
        PromptSlot::CompressRange => 1,
        PromptSlot::ContextLimitNudge => 2,
        PromptSlot::TurnNudge => 3,
        PromptSlot::IterationNudge => 4,
    };
    DEFAULTS[index].clone()
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
