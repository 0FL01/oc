//! Bounded query/action payloads shared by the native worker and any
//! frontend (T39).
//!
//! These are view-model DTOs: counts, ids and bounded previews only. No
//! storage handles, no transcripts beyond the requested page, no secrets.

use std::collections::BTreeMap;

use crate::domain::SessionId;
use crate::session::Role;

/// One Location-verified session ID, without enumerating the archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionProbe {
    /// No row and no Location binding exist for this ID.
    Absent,
    /// An existing root in the current Location.
    Root,
    /// An existing child in the current Location; never a root tab.
    Child,
}

/// Ordered root tabs in the current Location. `None` leaves Home sessionless,
/// even when other tabs are open; this is not a session creation request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TabDeckSnapshot {
    /// Canonical Location served by the application at load/save time.
    pub location: String,
    /// Opaque token for the exact stored preference, or None if absent.
    /// Projected restores carry a non-writable token; carry it unchanged into
    /// a save so the owner can refuse writes that would erase hidden tabs.
    /// Adopt the returned token only after a save succeeds.
    pub revision: Option<String>,
    /// Existing Location-bound root sessions, in display order.
    pub sessions: Vec<SessionId>,
    /// Selected tab, or sessionless Home.
    pub active: Option<SessionId>,
}

impl TabDeckSnapshot {
    /// The stored preference contained tabs or an active route that could not
    /// be restored exactly. A projected deck is read-only until repaired.
    pub fn projected(&self) -> bool {
        self.revision
            .as_deref()
            .is_some_and(|revision| revision.starts_with("projected:"))
    }

    /// Mark an incomplete restore without losing the original CAS fingerprint.
    /// The revision remains opaque to consumers and cannot be used for a save.
    pub fn mark_projected(&mut self) {
        if !self.projected() {
            self.revision = Some(format!(
                "projected:{}",
                self.revision.as_deref().unwrap_or("")
            ));
        }
    }
}

/// Prefs key holding the persisted model selection JSON.
pub const PREF_MODEL_SELECTION: &str = "tui.model_selection";
/// Prefs key holding the persisted primary agent JSON.
pub const PREF_PRIMARY_AGENT: &str = "tui.primary_agent";

/// Scoped frontend selection action. Selecting a model restores its preference;
/// selecting `Variant(None)` explicitly clears the overlay. Legacy headless
/// `select_model` remains an exact model/variant action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelectionAction {
    /// Read/initialize the session's selection.
    Current,
    /// Select a model, retaining the current choice or its remembered variant.
    Model(String),
    /// Select an exact variant, including explicit Default (`None`).
    Variant(Option<String>),
    /// Select an agent and restore that session/agent's model draft.
    Agent(String),
    /// Initialize a new Home route for the current agent.
    New(Option<String>),
}

/// One committed history row with its durable sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryMessage {
    /// Message sequence (ordering key for paging).
    pub seq: i64,
    /// `user` / `assistant`.
    pub role: Role,
    /// Message text.
    pub text: String,
    /// Safe projection of this row's turn; absent for legacy text-only rows.
    pub turn: Option<HistoryTurn>,
}

/// Safe turn metadata and ordered bounded parts, projected from durable records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryTurn {
    /// Stable journal turn id.
    pub id: String,
    /// Actual terminal/runtime status.
    pub status: String,
    /// Agent selected for this turn, when recorded.
    pub agent: Option<String>,
    /// Categorical presentation slot pinned by the owning generation.
    pub agent_color_index: Option<usize>,
    /// Display name pinned at generation time.
    pub model_label: String,
    /// Last input and summed output token counts; None means unreported.
    pub usage: Option<(u64, u64)>,
    /// Latest provider-reported generation input/output pair, independent of
    /// whether billed usage for every round of this turn is known.
    pub context_usage: Option<(u64, u64)>,
    /// Measured wall time, if recorded.
    pub duration_ms: Option<u64>,
    /// Measured provider-active time, if recorded.
    pub streamed_ms: Option<u64>,
    /// Ordered parts; identity is (turn id, part index), tool ids remain durable.
    pub parts: Vec<TranscriptPart>,
    /// Identity `(id, sequence)`, original order and status for each served part.
    pub part_states: Vec<PartState>,
    /// Parts outside the bounded projection (not deleted from the archive).
    pub omitted_parts: usize,
    /// At least one served field is a preview or was omitted.
    pub truncated: bool,
    /// Older journal has no trustworthy public part ordering/summary records.
    pub legacy_text_only: bool,
}

/// Metadata parallel to `HistoryTurn.parts`, shared by live checkpoint events
/// and replay. Sequence never changes when an earlier part is omitted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartState {
    /// Durable display sequence within its owning turn.
    pub sequence: usize,
    /// Recorded tool outcome, or parent turn state for text/reasoning.
    pub status: String,
    /// This part is a bounded preview.
    pub truncated: bool,
    /// Structured tool input exceeded the serving budget; op id remains usable.
    pub input_omitted: bool,
}

/// Public transcript content only. Never contains opaque provider payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptPart {
    /// Bounded text projected from an existing wire message.
    Text(String),
    /// Public reasoning summary, not encrypted continuation.
    Reasoning {
        /// Bounded public text.
        text: String,
        /// Measured summary window, when known.
        duration_ms: Option<u64>,
    },
    /// Existing operation record, retaining exact outcome and continuation id.
    Tool(ToolOpView),
}

/// One contiguous, bounded history page (oldest-first for rendering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryPage {
    /// Durable hierarchy; child sessions suppress the automatic sidebar.
    pub parent_id: Option<String>,
    /// Existing session metadata; None honestly denotes an untitled session.
    pub title: Option<String>,
    /// Rows in render order.
    pub rows: Vec<HistoryMessage>,
    /// Total committed messages in the session.
    pub total: usize,
    /// Older rows exist before the first row of this page.
    pub has_older: bool,
    /// Newer rows exist after the last row of this page.
    pub has_newer: bool,
}

/// Bounded byte window of an existing durable tool result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutputPage {
    /// Complete UTF-8 characters in this window.
    pub text: String,
    /// Complete stored result size in bytes.
    pub total_bytes: i64,
    /// Next byte boundary, absent when the result is exhausted.
    pub next_offset: Option<i64>,
}

/// One selectable model with its bounded variant list and limits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelEntry {
    /// Exact model id (no provider prefix).
    pub id: String,
    /// Configured/discovered human name; exact id is the fallback.
    pub display_name: String,
    /// Selected provider's configured name, falling back to its id.
    pub provider_name: String,
    /// Explicit input/output tariffs. None is unknown, never free.
    pub price: Option<ModelPrice>,
    /// Declared variants.
    pub variants: Vec<VariantEntry>,
    /// Context limit (0 when undeclared).
    pub context: u64,
    /// Distinguishes an explicit context value from the legacy zero fallback.
    pub context_known: bool,
    /// Output limit (0 when undeclared).
    pub output: u64,
    /// Distinguishes an explicit output value from the legacy zero fallback.
    pub output_known: bool,
}

/// Decimal tariffs per million tokens (strings preserve configured precision).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelPrice {
    /// Input tariff.
    pub input: String,
    /// Output tariff.
    pub output: String,
}

impl ModelPrice {
    /// Both explicitly declared tariffs must be zero.
    pub fn is_free(&self) -> bool {
        self.input.parse::<f64>() == Ok(0.0) && self.output.parse::<f64>() == Ok(0.0)
    }
}

/// One declared model variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantEntry {
    /// Variant name.
    pub name: String,
    /// True when the variant is declared but disabled.
    pub disabled: bool,
    /// Declared reasoning effort, if any.
    pub reasoning_effort: Option<String>,
}

/// One selectable agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    /// Generation categorical slot over all admitted profiles, including children.
    pub color_index: usize,
    /// Profile id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Pinned model id, if any.
    pub model: Option<String>,
    /// Pinned variant, if any.
    pub variant: Option<String>,
}

/// Catalog plus the effective selection for the next turn.
/// Session autoaccept is independent of agent tool allow/deny rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutoAcceptState {
    /// Backend has no interactive permission-request/reply capability.
    #[default]
    Unsupported,
    /// Capability exists, but approvals are prompted.
    Disabled,
    /// Pending permission requests are automatically approved.
    Enabled,
}

/// Catalog plus effective next-turn and session capability state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    /// Safe presentation settings and canonical admitted Location.
    pub chrome: TuiChrome,
    /// Honest autoaccept capability/state, never inferred from permission rules.
    pub auto_accept: AutoAcceptState,
    /// Selected provider id.
    pub provider: String,
    /// Sorted models.
    pub models: Vec<ModelEntry>,
    /// Effective model id.
    pub model_id: String,
    /// Effective variant.
    pub variant: Option<String>,
    /// Sorted agent profiles.
    pub agents: Vec<AgentEntry>,
    /// Effective primary agent id, if resolved.
    pub agent_id: Option<String>,
    /// Admitted command ids (templates stay in the application).
    pub commands: Vec<String>,
    /// Descriptions for the admitted commands in this generation; no templates.
    pub command_descriptions: BTreeMap<String, String>,
}

/// Presentation-only settings; no runtime policy or credentials.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuiChrome {
    /// Canonical application Location, unknown in mock workers.
    pub location: Option<String>,
    /// Explicit debug.devtools override; absence uses the build channel.
    pub devtools: Option<bool>,
    /// Effective native build channel, projected by the composition owner.
    pub build_channel: TuiBuildChannel,
    /// session.sidebar == hide.
    pub sidebar_hidden: bool,
    /// Width occupied by configured vertical tabs (zero for horizontal).
    pub vertical_tabs_width: u16,
    /// Upstream tab indicator presentation; status is the default.
    pub tab_indicators: TabIndicators,
    /// Explicit terminal.copy selection; absence uses the UI's platform default.
    pub terminal_copy: Option<TerminalCopyMode>,
    /// Explicit config.animations; absence enables interface animations.
    pub animations: Option<bool>,
}

/// Mouse text selection/copy behavior from the effective Location config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCopyMode {
    /// Copy selected text on a dragging mouse-up.
    Select,
    /// Copy an existing selection on right mouse-down.
    Manual,
}

/// Selected session tab indicator presentation (upstream `tabs.indicators`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TabIndicators {
    /// Blank while idle; busy state displays the first spinner frame.
    #[default]
    Status,
    /// Always display the selected tab number.
    Numbers,
}

/// Native debug builds map to upstream's local channel; release builds to packaged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TuiBuildChannel {
    /// Locally built debug executable.
    Local,
    /// Packaged/release executable (also the neutral mock default).
    #[default]
    Packaged,
}

impl TuiChrome {
    /// Match upstream `debug.devtools ?? channel == local` without fake controls.
    pub fn devtools_visible(&self) -> bool {
        self.devtools
            .unwrap_or(self.build_channel == TuiBuildChannel::Local)
    }

    /// Match upstream `config.animations ?? true`.
    pub fn animations_enabled(&self) -> bool {
        self.animations.unwrap_or(true)
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;

    #[test]
    fn v03_devtools_override_and_channel_default() {
        for channel in [TuiBuildChannel::Local, TuiBuildChannel::Packaged] {
            let mut chrome = TuiChrome {
                build_channel: channel,
                ..Default::default()
            };
            assert_eq!(chrome.devtools_visible(), channel == TuiBuildChannel::Local);
            chrome.devtools = Some(false);
            assert!(!chrome.devtools_visible());
            chrome.devtools = Some(true);
            assert!(chrome.devtools_visible());
        }
    }
}

/// Result of one successful Location switch inside a running application.
///
/// The target generation was built completely before this value is produced:
/// the frontend adopts the returned session and catalog together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocationSnapshot {
    /// Canonical Location id (project path) now served.
    pub location: String,
    /// Monotonic owner Location epoch; compare with async file suggestions.
    pub generation: u64,
    /// Session bound to that Location (new, or the recorded one on return).
    pub session: String,
    /// Catalog for the new generation.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for existing application API callers. May contain
    /// configured paths/keys; frontends must use `notices` instead.
    pub diagnostics: Vec<String>,
    /// Source-based, allowlisted warnings for interactive presentation.
    pub notices: Vec<StartupNotice>,
}

/// Published Location generation for a sessionless Home composer.
/// No root session is created by this switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeLocationSnapshot {
    /// Canonical Location id now served.
    pub location: String,
    /// Monotonic owner Location epoch; compare with async file suggestions.
    pub generation: u64,
    /// Current Home choice and catalog in the target Location.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for application callers; frontends display `notices`.
    pub diagnostics: Vec<String>,
    /// Allowlisted interactive warnings.
    pub notices: Vec<StartupNotice>,
}

/// Rebuilt configuration for the current canonical Location. Existing roots,
/// selected tab and stored deck are untouched; the catalog is for the next turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadLocationSnapshot {
    /// Canonical Location still served by the application.
    pub location: String,
    /// Monotonic owner epoch, including reloads and A→B→A switches.
    pub generation: u64,
    /// Fresh catalog and effective selection for this generation.
    pub catalog: CatalogSnapshot,
    /// Detailed diagnostics for application callers; UIs display `notices`.
    pub diagnostics: Vec<String>,
    /// Allowlisted interactive warnings.
    pub notices: Vec<StartupNotice>,
}

/// Bounded file candidates from the application owner's current Location.
/// Consumers must compare both `location` and `generation` with their current
/// route before showing an asynchronously delivered response (including A→B→A).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSuggestionsSnapshot {
    /// Canonical Location that owned the file walk.
    pub location: String,
    /// Monotonic owner Location epoch; changes on every successful switch.
    pub generation: u64,
    /// Sorted Location-relative paths; no file contents.
    pub paths: Vec<String>,
    /// More matches exist or the traversal budget was reached.
    pub truncated: bool,
}

/// Source of non-fatal composition/persisted-selection diagnostics. Never
/// contains user input or text from the underlying error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupNotice {
    /// Agent, skill or command definition could not be admitted.
    Definitions,
    /// A plugin marker was ignored without executing it.
    Plugin,
    /// Native DCP settings include ignored or unsupported options.
    Dcp,
    /// Ordered instruction sources produced diagnostics.
    Instructions,
    /// A stored model/agent selection could not be applied.
    SavedSelection,
}

/// Skill catalog card: metadata only, never bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillCard {
    /// Directory id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
}

/// DCP context/stats snapshot (counts only, no transcript).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcpSnapshot {
    /// Estimated context tokens.
    pub estimated_tokens: u64,
    /// Effective max context tokens.
    pub max_context: u64,
    /// Turns since the last successful compression.
    pub turns_since_compress: u64,
    /// Stored compression blocks for the session.
    pub blocks: usize,
    /// Successful compressions.
    pub compressions: u64,
    /// Emitted nudges.
    pub nudges: u64,
    /// Recorded prune marks.
    pub prunes: u64,
}

/// One tool operation as recorded durably.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOpView {
    /// Operation id.
    pub op: String,
    /// Insertion-order cursor for newest-first paging.
    pub rowid: i64,
    /// Tool name.
    pub name: String,
    /// `started` / `completed` / `failed` / `denied` / `cancelled` / `unknown`.
    pub state: String,
    /// Recorded input JSON (bounded preview by the consumer).
    pub input: Option<String>,
    /// Recorded outcome preview (bounded; never the whole result).
    pub output: Option<String>,
    /// Full stored output size in bytes.
    pub output_bytes: i64,
    /// Whether [`ToolOpView::output`] is a truncated preview.
    pub output_truncated: bool,
}

/// One bounded, newest-first tool operation page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolOpPage {
    /// Rows newest-first.
    pub rows: Vec<ToolOpView>,
    /// Total recorded operations for the session.
    pub total: usize,
    /// Older operations exist before the last row of this page.
    pub has_older: bool,
}
