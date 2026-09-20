//! DCP nudges and automatic strategies for T19 (DCP05–07).
//!
//! Soft-limit nudge state machine (frequency/iteration/turn resets, no
//! accumulating reminder copies), compress-time dedup/purgeErrors strategies,
//! layered DCP config (defaults < `dcp.jsonc` < per-model overrides, sources
//! read-only), manual-mode gating, stats/notifications, and exact bare/pinned
//! alias binding to one compiled module instance. No generic summarizer and
//! no JS hooks live here.

use std::collections::BTreeMap;

use thiserror::Error;

/// Required-profile defaults (upstream-derived, see `examples/dcp.jsonc`).
pub const DEFAULT_MIN_CONTEXT: u64 = 50_000;
/// Required-profile defaults (upstream-derived).
pub const DEFAULT_MAX_CONTEXT: u64 = 100_000;
/// Reminder cadence in iterations.
pub const DEFAULT_NUDGE_FREQUENCY: u64 = 5;
/// Iterations before the force path.
pub const DEFAULT_ITERATION_THRESHOLD: u64 = 15;
/// Error-input purge age in turns.
pub const DEFAULT_PURGE_AFTER_TURNS: u64 = 4;
/// Large error-input bound (bytes).
pub const LARGE_INPUT_BYTES: usize = 4096;
/// Visible compiled module revision for both admitted aliases.
pub const DCP_MODULE_REVISION: &str = "11f6517780a502512a3467645074be447cb0369e";

/// Typed DCP-auto errors (bounds only, no contents).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DcpAutoError {
    /// Config value malformed.
    #[error("invalid dcp config: {reason}")]
    InvalidConfig {
        /// Human reason.
        reason: String,
    },
    /// Experimental option explicitly unsupported.
    #[error("unsupported dcp option: {reason}")]
    UnsupportedOption {
        /// Human reason.
        reason: String,
    },
    /// Unknown plugin identity (not bare/pinned DCP).
    #[error("unsupported plugin {identity}")]
    UnsupportedPlugin {
        /// Given identity.
        identity: String,
    },
}

/// Nudge force: soft hints vs. hard pre-continue requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeForce {
    /// Advisory hint text.
    Soft,
    /// Compression required before continuing.
    Hard,
}

/// Per-model threshold overrides (keyed at runtime, never hardcoded).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelOverride {
    /// Optional min-context replacement.
    pub min_context: Option<u64>,
    /// Optional max-context replacement.
    pub max_context: Option<u64>,
    /// Optional frequency replacement.
    pub nudge_frequency: Option<u64>,
}

/// Layered DCP configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcpConfig {
    /// Master switch.
    pub enabled: bool,
    /// Soft min-context threshold.
    pub min_context: u64,
    /// Soft max-context threshold.
    pub max_context: u64,
    /// Reminder cadence in iterations.
    pub nudge_frequency: u64,
    /// Iterations before the force path.
    pub iteration_threshold: u64,
    /// Nudge force.
    pub nudge_force: NudgeForce,
    /// Keep a summary buffer alongside compressed spans.
    pub summary_buffer: bool,
    /// Manual mode: no autonomous nudges or tool invocation.
    pub manual_mode: bool,
    /// Dedup strategy enabled.
    pub deduplication: bool,
    /// Purge-errors strategy enabled.
    pub purge_errors: bool,
    /// Error-input purge age in turns.
    pub purge_after_turns: u64,
    /// Protected tool names (exempt from dedup, outputs preserved).
    pub protected_tools: Vec<String>,
    /// Protected file glob patterns.
    pub protected_file_patterns: Vec<String>,
    /// Prune notifications enabled.
    pub prune_notification: bool,
    /// Per-model overrides (runtime keys only).
    pub model_overrides: BTreeMap<String, ModelOverride>,
}

impl Default for DcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_context: DEFAULT_MIN_CONTEXT,
            max_context: DEFAULT_MAX_CONTEXT,
            nudge_frequency: DEFAULT_NUDGE_FREQUENCY,
            iteration_threshold: DEFAULT_ITERATION_THRESHOLD,
            nudge_force: NudgeForce::Soft,
            summary_buffer: true,
            manual_mode: false,
            deduplication: true,
            purge_errors: true,
            purge_after_turns: DEFAULT_PURGE_AFTER_TURNS,
            protected_tools: Vec::new(),
            protected_file_patterns: Vec::new(),
            prune_notification: true,
            model_overrides: BTreeMap::new(),
        }
    }
}

fn positive_u64(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().filter(|v| *v > 0)
}

/// Load layered config: defaults, then the `dcp.jsonc` fragment (already
/// parsed to JSON by the caller; sources stay read-only here).
///
/// Experimental `allowSubAgents`/`customPrompts` are explicitly rejected;
/// unknown top-level keys produce warnings, never silent behavior.
pub fn load_config(fragment: &serde_json::Value) -> Result<(DcpConfig, Vec<String>), DcpAutoError> {
    let invalid = |reason: &str| DcpAutoError::InvalidConfig {
        reason: reason.to_string(),
    };
    let obj = fragment
        .as_object()
        .ok_or_else(|| invalid("config must be an object"))?;
    if let Some(exp) = obj.get("experimental") {
        if exp.get("allowSubAgents") == Some(&serde_json::Value::Bool(true)) {
            return Err(DcpAutoError::UnsupportedOption {
                reason: "allowSubAgents is out of goal scope".to_string(),
            });
        }
        if exp.get("customPrompts") == Some(&serde_json::Value::Bool(true)) {
            return Err(DcpAutoError::UnsupportedOption {
                reason: "customPrompts are deferred".to_string(),
            });
        }
    }
    let mut config = DcpConfig::default();
    let mut warnings = Vec::new();
    let known = [
        "enabled",
        "minContextLimit",
        "maxContextLimit",
        "nudgeFrequency",
        "iterationNudgeThreshold",
        "nudgeForce",
        "summaryBuffer",
        "manualMode",
        "strategies",
        "protectedTools",
        "protectedFilePatterns",
        "pruneNotification",
        "modelOverrides",
        "experimental",
    ];
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            warnings.push(format!("unknown dcp key: {key}"));
        }
    }
    if let Some(value) = obj.get("enabled").and_then(|v| v.as_bool()) {
        config.enabled = value;
    }
    if let Some(value) = obj.get("minContextLimit").and_then(positive_u64) {
        config.min_context = value;
    }
    if let Some(value) = obj.get("maxContextLimit").and_then(positive_u64) {
        config.max_context = value;
    }
    if let Some(value) = obj.get("nudgeFrequency").and_then(positive_u64) {
        config.nudge_frequency = value;
    }
    if let Some(value) = obj.get("iterationNudgeThreshold").and_then(positive_u64) {
        config.iteration_threshold = value;
    }
    match obj.get("nudgeForce").and_then(|v| v.as_str()) {
        Some("hard") => config.nudge_force = NudgeForce::Hard,
        Some("soft") | None => {}
        Some(other) => return Err(invalid(&format!("unknown nudgeForce {other}"))),
    }
    if let Some(value) = obj.get("summaryBuffer").and_then(|v| v.as_bool()) {
        config.summary_buffer = value;
    }
    if let Some(value) = obj.get("manualMode").and_then(|v| v.as_bool()) {
        config.manual_mode = value;
    }
    if let Some(strategies) = obj.get("strategies").and_then(|v| v.as_object()) {
        if let Some(value) = strategies.get("deduplication").and_then(|v| v.as_bool()) {
            config.deduplication = value;
        }
        if let Some(value) = strategies.get("purgeErrors").and_then(|v| v.as_bool()) {
            config.purge_errors = value;
        }
        if let Some(value) = strategies.get("purgeAfterTurns").and_then(positive_u64) {
            config.purge_after_turns = value;
        }
    }
    if let Some(tools) = obj.get("protectedTools").and_then(|v| v.as_array()) {
        config.protected_tools = tools
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(patterns) = obj.get("protectedFilePatterns").and_then(|v| v.as_array()) {
        config.protected_file_patterns = patterns
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
    }
    if let Some(value) = obj.get("pruneNotification").and_then(|v| v.as_bool()) {
        config.prune_notification = value;
    }
    if let Some(overrides) = obj.get("modelOverrides").and_then(|v| v.as_object()) {
        for (model, value) in overrides {
            let mut entry = ModelOverride::default();
            if let Some(v) = value.get("minContextLimit").and_then(positive_u64) {
                entry.min_context = Some(v);
            }
            if let Some(v) = value.get("maxContextLimit").and_then(positive_u64) {
                entry.max_context = Some(v);
            }
            if let Some(v) = value.get("nudgeFrequency").and_then(positive_u64) {
                entry.nudge_frequency = Some(v);
            }
            config.model_overrides.insert(model.clone(), entry);
        }
    }
    if config.min_context > config.max_context {
        return Err(invalid("minContextLimit exceeds maxContextLimit"));
    }
    Ok((config, warnings))
}

/// Effective numeric thresholds for a model (per-model overrides applied).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveThresholds {
    /// Soft min-context threshold.
    pub min_context: u64,
    /// Soft max-context threshold.
    pub max_context: u64,
    /// Reminder cadence.
    pub nudge_frequency: u64,
}

impl DcpConfig {
    /// Resolve thresholds for a model id (never hardcoded: caller supplies).
    pub fn effective(&self, model: &str) -> EffectiveThresholds {
        let mut out = EffectiveThresholds {
            min_context: self.min_context,
            max_context: self.max_context,
            nudge_frequency: self.nudge_frequency,
        };
        if let Some(entry) = self.model_overrides.get(model) {
            if let Some(value) = entry.min_context {
                out.min_context = value;
            }
            if let Some(value) = entry.max_context {
                out.max_context = value;
            }
            if let Some(value) = entry.nudge_frequency {
                out.nudge_frequency = value;
            }
        }
        out
    }
}

/// Nudge counters: iterations, turns since compress, last reminder point.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NudgeState {
    /// Monotonic iteration counter.
    pub iteration: u64,
    /// Turns since the last successful compression.
    pub turns_since_compress: u64,
    /// Iteration of the last emitted reminder, if any.
    pub last_reminder: Option<u64>,
    /// Emitted reminders (for the no-accumulation test hook).
    pub emitted: u64,
}

impl NudgeState {
    /// Advance one turn/iteration.
    pub fn on_turn(&mut self) {
        self.iteration += 1;
        self.turns_since_compress += 1;
    }

    /// Recalculate after a successful compression.
    pub fn on_compress_success(&mut self) {
        self.turns_since_compress = 0;
        self.last_reminder = None;
    }
}

/// Emitted nudge hint (transient: never persisted as a message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nudge {
    /// Advisory or required.
    pub force: NudgeForce,
    /// Short hint text (no transcript contents).
    pub text: String,
}

/// Evaluate the reminder: fires while the estimate exceeds the max threshold
/// and the cadence elapsed; manual mode and disabled configs stay silent.
/// Each emission is transient — projections never accumulate copies.
pub fn evaluate(
    config: &DcpConfig,
    state: &mut NudgeState,
    model: &str,
    estimated_tokens: u64,
) -> Option<Nudge> {
    if !config.enabled || config.manual_mode {
        return None;
    }
    let thresholds = config.effective(model);
    if estimated_tokens <= thresholds.max_context {
        return None;
    }
    let due = state
        .last_reminder
        .is_none_or(|last| state.iteration >= last + thresholds.nudge_frequency);
    if !due {
        return None;
    }
    let force = if state.iteration >= config.iteration_threshold {
        NudgeForce::Hard
    } else {
        config.nudge_force
    };
    state.last_reminder = Some(state.iteration);
    state.emitted += 1;
    Some(Nudge {
        force,
        text: format!(
            "context estimate {estimated_tokens} exceeds soft limit {}; compress a closed span",
            thresholds.max_context
        ),
    })
}

/// Tool-call record for compress-time strategies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRecord {
    /// Tool name.
    pub tool: String,
    /// Canonical argument bytes.
    pub arguments: String,
    /// Input size class driver.
    pub input_bytes: usize,
    /// Completion state.
    pub status: CallStatus,
    /// Kept output text.
    pub output: String,
    /// Turn index (for purge aging).
    pub turn: u64,
}

/// Call completion state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallStatus {
    /// Successful call.
    Completed,
    /// Errored call (outcome text retained even when purged).
    Errored,
}

/// Deduplicate identical calls at compress time, keeping the last needed
/// output per `(tool, arguments)`. Protected tools are exempt: all of their
/// outputs are preserved.
pub fn dedup_calls(records: Vec<ToolRecord>, protected_tools: &[String]) -> Vec<ToolRecord> {
    let mut last_index: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        if protected_tools.iter().any(|name| name == &record.tool) {
            continue;
        }
        last_index.insert((record.tool.clone(), record.arguments.clone()), index);
    }
    records
        .into_iter()
        .enumerate()
        .filter(|(index, record)| {
            protected_tools.iter().any(|name| name == &record.tool)
                || last_index.get(&(record.tool.clone(), record.arguments.clone())) == Some(index)
        })
        .map(|(_, record)| record)
        .collect()
}

/// Purge large errored inputs older than the configured age, keeping error
/// outcomes. Recent, small, or completed calls pass through untouched.
pub fn purge_errors(
    records: Vec<ToolRecord>,
    current_turn: u64,
    after_turns: u64,
) -> Vec<ToolRecord> {
    records
        .into_iter()
        .map(|mut record| {
            if record.status == CallStatus::Errored
                && record.input_bytes > LARGE_INPUT_BYTES
                && current_turn.saturating_sub(record.turn) >= after_turns
            {
                record.input_bytes = 0;
                record.arguments = "[purged large error input]".to_string();
            }
            record
        })
        .collect()
}

/// Compiled module identity: both admitted aliases bind one instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcpModuleId {
    /// Visible resolved revision.
    pub revision: String,
}

/// Resolve a plugin identity to the single compiled DCP module.
pub fn resolve_dcp_module(identity: &str) -> Result<DcpModuleId, DcpAutoError> {
    if identity == "@tarquinen/opencode-dcp" || identity == "@tarquinen/opencode-dcp@3.1.15" {
        return Ok(DcpModuleId {
            revision: DCP_MODULE_REVISION.to_string(),
        });
    }
    Err(DcpAutoError::UnsupportedPlugin {
        identity: identity.to_string(),
    })
}

/// Runtime stats (counts only; debug logs carry metadata, never contents).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DcpStats {
    /// Reminders emitted.
    pub nudges_emitted: u64,
    /// Successful compressions.
    pub compressions: u64,
    /// Prune marks recorded.
    pub prunes: u64,
}

/// Metadata-only debug line for an event (safe under `debug`).
pub fn debug_line(event: &str, stats: &DcpStats) -> String {
    format!(
        "dcp {event} nudges={} compressions={} prunes={}",
        stats.nudges_emitted, stats.compressions, stats.prunes
    )
}

#[cfg(test)]
mod tests {
    use super::{
        CallStatus, DEFAULT_MAX_CONTEXT, DEFAULT_MIN_CONTEXT, DcpAutoError, DcpConfig, DcpStats,
        NudgeForce, NudgeState, ToolRecord, debug_line, dedup_calls, evaluate, load_config,
        purge_errors, resolve_dcp_module,
    };

    #[test]
    fn dcp05_thresholds_frequency_turn_reset_no_accumulation() {
        let config = DcpConfig::default();
        assert_eq!(
            (config.min_context, config.max_context),
            (DEFAULT_MIN_CONTEXT, DEFAULT_MAX_CONTEXT)
        );
        let mut state = NudgeState::default();
        // Below threshold: silent at any iteration.
        for _ in 0..10 {
            state.on_turn();
            assert!(evaluate(&config, &mut state, "m", 10_000).is_none());
        }
        // Over threshold fires once, then stays quiet until the cadence elapses.
        let first = evaluate(&config, &mut state, "m", 200_000).expect("nudge");
        assert_eq!(first.force, NudgeForce::Soft);
        assert!(first.text.contains("200000"));
        assert!(!first.text.contains("transcript"));
        for _ in 0..4 {
            state.on_turn();
            assert!(evaluate(&config, &mut state, "m", 200_000).is_none());
        }
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 200_000).is_some());
        assert_eq!(state.emitted, 2);
        // Successful compression recalculates: immediate re-fire allowed once.
        state.on_compress_success();
        assert!(evaluate(&config, &mut state, "m", 200_000).is_some());
        // Per-model override changes the cadence without code changes.
        let fragment = serde_json::json!({"modelOverrides": {"m": {"nudgeFrequency": 2}}});
        let (config, _) = load_config(&fragment).expect("config");
        let mut state = NudgeState::default();
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 200_000).is_some());
        // Manual mode silences autonomous nudges entirely.
        let fragment = serde_json::json!({"manualMode": true});
        let (config, _) = load_config(&fragment).expect("config");
        let mut state = NudgeState::default();
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 9_999_999).is_none());
    }

    #[test]
    fn dcp06_dedup_purge_recalc_at_compress() {
        let call = |tool: &str, args: &str, status: CallStatus, turn: u64| ToolRecord {
            tool: tool.to_string(),
            arguments: args.to_string(),
            input_bytes: args.len(),
            status,
            output: format!("out-{args}"),
            turn,
        };
        // Identical calls collapse to the last output; protected tools keep all.
        let records = vec![
            call("read", "{\"p\":\"a\"}", CallStatus::Completed, 1),
            call("read", "{\"p\":\"a\"}", CallStatus::Completed, 2),
            call("skill", "{\"id\":\"s\"}", CallStatus::Completed, 2),
            call("skill", "{\"id\":\"s\"}", CallStatus::Completed, 3),
        ];
        let deduped = dedup_calls(records, &["skill".to_string()]);
        assert_eq!(deduped.len(), 3);
        assert!(
            deduped
                .iter()
                .filter(|r| r.tool == "read")
                .all(|r| r.turn == 2)
        );
        // Purge: old large errored inputs drop bulk but keep outcomes.
        let mut old = call("bash", &"x".repeat(5000), CallStatus::Errored, 1);
        old.output = "error: boom".to_string();
        let records = vec![
            old,
            call("bash", &"y".repeat(5000), CallStatus::Errored, 9),
            call("bash", &"z".repeat(5000), CallStatus::Completed, 1),
        ];
        let purged = purge_errors(records, 10, 4);
        assert_eq!(purged[0].arguments, "[purged large error input]");
        assert_eq!(purged[0].output, "error: boom");
        assert!(purged[1].arguments.len() > 100);
        assert!(purged[2].arguments.len() > 100);
    }

    #[test]
    fn dcp07_config_aliases_stats_and_unsupported() {
        // Defaults < user fragment < per-model override.
        let (config, warnings) = load_config(&serde_json::json!({
            "maxContextLimit": 80000,
            "modelOverrides": {"m": {"maxContextLimit": 60000}},
            "mystery": 1,
        }))
        .expect("config");
        assert_eq!(config.max_context, 80000);
        assert_eq!(config.effective("m").max_context, 60000);
        assert_eq!(config.effective("other").max_context, 80000);
        assert_eq!(warnings, vec!["unknown dcp key: mystery".to_string()]);
        assert!(
            load_config(&serde_json::json!({"minContextLimit": 9, "maxContextLimit": 8})).is_err()
        );
        assert!(matches!(
            load_config(&serde_json::json!({"experimental": {"allowSubAgents": true}})),
            Err(DcpAutoError::UnsupportedOption { .. })
        ));
        // Bare and pinned aliases bind one compiled instance, visibly revised.
        let bare = resolve_dcp_module("@tarquinen/opencode-dcp").expect("bare");
        let pinned = resolve_dcp_module("@tarquinen/opencode-dcp@3.1.15").expect("pinned");
        assert_eq!(bare, pinned);
        assert!(!bare.revision.is_empty());
        assert!(resolve_dcp_module("@tarquinen/opencode-dcp@latest").is_err());
        assert!(resolve_dcp_module("https://x.invalid/p.js").is_err());
        // Stats/debug carry counts only.
        let stats = DcpStats {
            nudges_emitted: 2,
            compressions: 1,
            prunes: 0,
        };
        let line = debug_line("prune", &stats);
        assert!(line.contains("nudges=2"));
    }
}
