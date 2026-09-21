//! DCP nudges and automatic strategies for T19 (DCP05–07).
//!
//! Soft-limit nudge state machine (frequency/iteration/turn resets, no
//! accumulating reminder copies), compress-time dedup/purgeErrors strategies,
//! layered DCP config (defaults < `dcp.jsonc` < per-model overrides, sources
//! read-only), manual-mode gating, stats/notifications, and exact bare/pinned/latest
//! alias binding to one compiled module instance. No generic summarizer and
//! no JS hooks live here.

use std::collections::BTreeMap;

use thiserror::Error;

use crate::config::Permission;

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
    /// Optional min-context percentage in basis points.
    pub min_context_percent: Option<u32>,
    /// Optional max-context replacement.
    pub max_context: Option<u64>,
    /// Optional max-context percentage in basis points.
    pub max_context_percent: Option<u32>,
    /// Optional frequency replacement.
    pub nudge_frequency: Option<u64>,
}

/// Layered DCP configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcpConfig {
    /// Master switch.
    pub enabled: bool,
    /// Compression-specific central permission override.
    pub compress_permission: Option<Permission>,
    /// Soft min-context threshold.
    pub min_context: u64,
    /// Percentage replacement for min-context, in basis points.
    pub min_context_percent: Option<u32>,
    /// Soft max-context threshold.
    pub max_context: u64,
    /// Percentage replacement for max-context, in basis points.
    pub max_context_percent: Option<u32>,
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
    /// Whether automatic projection strategies remain enabled in manual mode.
    pub automatic_strategies: bool,
    /// Dedup strategy enabled.
    pub deduplication: bool,
    /// Purge-errors strategy enabled.
    pub purge_errors: bool,
    /// Error-input purge age in turns.
    pub purge_after_turns: u64,
    /// Protected tool names (exempt from dedup, outputs preserved).
    pub protected_tools: Vec<String>,
    /// Strategy-local deduplication protections.
    pub dedup_protected_tools: Vec<String>,
    /// Strategy-local purge-error protections.
    pub purge_protected_tools: Vec<String>,
    /// Protected file glob patterns.
    pub protected_file_patterns: Vec<String>,
    /// Preserve `<protect>` payloads verbatim.
    pub protect_tags: bool,
    /// Preserve covered user messages verbatim.
    pub protect_user_messages: bool,
    /// Prune notifications enabled.
    pub prune_notification: bool,
    /// Notification channel (`chat` or documented `toast` UI difference).
    pub prune_notification_type: String,
    /// Whether configured slash commands are enabled.
    pub commands_enabled: bool,
    /// Whether recent-turn protection is enabled.
    pub turn_protection: bool,
    /// Number of recent turns protected when enabled.
    pub turn_protection_turns: u64,
    /// Show compression content in a user-facing notice (TUI consumes later).
    pub show_compression: bool,
    /// Safe metadata-only DCP diagnostics.
    pub debug: bool,
    /// Per-model overrides (runtime keys only).
    pub model_overrides: BTreeMap<String, ModelOverride>,
}

impl Default for DcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compress_permission: None,
            min_context: DEFAULT_MIN_CONTEXT,
            min_context_percent: None,
            max_context: DEFAULT_MAX_CONTEXT,
            max_context_percent: None,
            nudge_frequency: DEFAULT_NUDGE_FREQUENCY,
            iteration_threshold: DEFAULT_ITERATION_THRESHOLD,
            nudge_force: NudgeForce::Soft,
            summary_buffer: true,
            manual_mode: false,
            automatic_strategies: true,
            deduplication: true,
            purge_errors: true,
            purge_after_turns: DEFAULT_PURGE_AFTER_TURNS,
            protected_tools: Vec::new(),
            dedup_protected_tools: Vec::new(),
            purge_protected_tools: Vec::new(),
            protected_file_patterns: Vec::new(),
            protect_tags: false,
            protect_user_messages: false,
            prune_notification: true,
            prune_notification_type: "chat".to_string(),
            commands_enabled: true,
            turn_protection: false,
            turn_protection_turns: 4,
            show_compression: false,
            debug: false,
            model_overrides: BTreeMap::new(),
        }
    }
}

fn positive_u64(value: &serde_json::Value) -> Option<u64> {
    value.as_u64().filter(|v| *v > 0)
}

fn percentage_basis_points(value: &str) -> Option<u32> {
    let number = value.strip_suffix('%')?;
    let (whole, fractional) = number.split_once('.').unwrap_or((number, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fractional.len() > 2
        || !fractional.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole = whole.parse::<u32>().ok()?;
    let fraction = match fractional.len() {
        0 => 0,
        1 => fractional.parse::<u32>().ok()?.saturating_mul(10),
        2 => fractional.parse::<u32>().ok()?,
        _ => return None,
    };
    let basis_points = whole.checked_mul(100)?.checked_add(fraction)?;
    (1..=10_000).contains(&basis_points).then_some(basis_points)
}

fn parse_limit(
    value: &serde_json::Value,
    field: &str,
) -> Result<(Option<u64>, Option<u32>), DcpAutoError> {
    if let Some(number) = positive_u64(value) {
        return Ok((Some(number), None));
    }
    if let Some(percent) = value.as_str().and_then(percentage_basis_points) {
        return Ok((None, Some(percent)));
    }
    Err(DcpAutoError::InvalidConfig {
        reason: format!("{field} must be a positive integer or X% (0 < X <= 100)"),
    })
}

/// Load layered config: defaults, then the `dcp.jsonc` fragment (already
/// parsed to JSON by the caller; sources stay read-only here).
///
/// Experimental `allowSubAgents` is accepted with a visible warning until
/// subagent support lands (it only permits subagent summarisation, which this
/// generation never performs); `customPrompts` is still rejected as deferred.
/// Unknown top-level keys produce warnings, never silent behavior.
pub fn load_config(fragment: &serde_json::Value) -> Result<(DcpConfig, Vec<String>), DcpAutoError> {
    let invalid = |reason: &str| DcpAutoError::InvalidConfig {
        reason: reason.to_string(),
    };
    let obj = fragment
        .as_object()
        .ok_or_else(|| invalid("config must be an object"))?;
    let mut config = DcpConfig::default();
    let mut warnings = Vec::new();
    if let Some(exp) = obj.get("experimental") {
        if exp.get("allowSubAgents") == Some(&serde_json::Value::Bool(true)) {
            // Future subagent support will use this flag; today the option is
            // tolerated visibly instead of blocking the whole application.
            warnings.push(
                "dcp experimental.allowSubAgents is enabled; subagents are not \
                 implemented yet, so the option is ignored"
                    .to_string(),
            );
        }
        if exp.get("customPrompts") == Some(&serde_json::Value::Bool(true)) {
            return Err(DcpAutoError::UnsupportedOption {
                reason: "customPrompts are deferred".to_string(),
            });
        }
    }
    let known = [
        "enabled",
        "$schema",
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
        "autoUpdate",
        "debug",
        "pruneNotificationType",
        "commands",
        "turnProtection",
        "compress",
    ];
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            warnings.push(format!("unknown dcp key: {key}"));
        }
    }
    if let Some(value) = obj.get("enabled").and_then(|v| v.as_bool()) {
        config.enabled = value;
    }
    if let Some(value) = obj.get("minContextLimit") {
        let (absolute, percent) = parse_limit(value, "minContextLimit")?;
        if let Some(value) = absolute {
            config.min_context = value;
        }
        config.min_context_percent = percent;
    }
    if let Some(value) = obj.get("maxContextLimit") {
        let (absolute, percent) = parse_limit(value, "maxContextLimit")?;
        if let Some(value) = absolute {
            config.max_context = value;
        }
        config.max_context_percent = percent;
    }
    if let Some(value) = obj.get("nudgeFrequency").and_then(positive_u64) {
        config.nudge_frequency = value;
    }
    if let Some(value) = obj.get("iterationNudgeThreshold").and_then(positive_u64) {
        config.iteration_threshold = value;
    }
    match obj.get("nudgeForce").and_then(|v| v.as_str()) {
        Some("strong") => config.nudge_force = NudgeForce::Hard,
        Some("soft") | None => {}
        Some(other) => return Err(invalid(&format!("unknown nudgeForce {other}"))),
    }
    if let Some(value) = obj.get("summaryBuffer").and_then(|v| v.as_bool()) {
        config.summary_buffer = value;
    }
    if obj.get("autoUpdate") == Some(&serde_json::Value::Bool(true)) {
        return Err(DcpAutoError::UnsupportedOption {
            reason: "autoUpdate is unavailable in the native runtime".to_string(),
        });
    }
    if let Some(value) = obj.get("autoUpdate")
        && !value.is_boolean()
    {
        return Err(invalid("autoUpdate must be boolean"));
    }
    if let Some(value) = obj.get("debug") {
        config.debug = value
            .as_bool()
            .ok_or_else(|| invalid("debug must be boolean"))?;
    }
    match obj.get("manualMode") {
        Some(serde_json::Value::Bool(value)) => {
            config.manual_mode = *value;
            if *value {
                config.automatic_strategies = false;
            }
        }
        Some(value) if value.is_object() => {
            let manual = value.as_object().expect("checked object");
            if let Some(value) = manual.get("enabled").and_then(|v| v.as_bool()) {
                config.manual_mode = value;
            }
            if let Some(value) = manual.get("automaticStrategies").and_then(|v| v.as_bool()) {
                config.automatic_strategies = value;
            }
        }
        Some(_) => return Err(invalid("manualMode must be a bool or object")),
        None => {}
    }
    if let Some(strategies) = obj.get("strategies").and_then(|v| v.as_object()) {
        if let Some(value) = strategy_enabled(strategies.get("deduplication")) {
            config.deduplication = value;
        }
        if let Some(value) = strategy_enabled(strategies.get("purgeErrors")) {
            config.purge_errors = value;
        }
        if let Some(value) = strategies
            .get("purgeErrors")
            .and_then(|v| v.get("turns"))
            .and_then(positive_u64)
            .or_else(|| strategies.get("purgeAfterTurns").and_then(positive_u64))
        {
            config.purge_after_turns = value;
        }
        append_strings(
            &mut config.dedup_protected_tools,
            strategies
                .get("deduplication")
                .and_then(|value| value.get("protectedTools")),
        )?;
        append_strings(
            &mut config.purge_protected_tools,
            strategies
                .get("purgeErrors")
                .and_then(|value| value.get("protectedTools")),
        )?;
    }
    if obj.get("protectedTools").is_some() {
        config.protected_tools.clear();
        append_strings(&mut config.protected_tools, obj.get("protectedTools"))?;
    }
    if obj.get("protectedFilePatterns").is_some() {
        config.protected_file_patterns.clear();
        append_strings(
            &mut config.protected_file_patterns,
            obj.get("protectedFilePatterns"),
        )?;
    }
    if let Some(value) = obj.get("pruneNotification") {
        config.prune_notification = match value {
            serde_json::Value::Bool(value) => *value,
            serde_json::Value::String(mode) if matches!(mode.as_str(), "none" | "off") => false,
            serde_json::Value::String(_) => true,
            _ => return Err(invalid("pruneNotification must be bool or string")),
        };
    }
    if let Some(overrides) = obj.get("modelOverrides").and_then(|v| v.as_object()) {
        for (model, value) in overrides {
            let mut entry = ModelOverride::default();
            if let Some(value) = value.get("minContextLimit") {
                let (absolute, percent) = parse_limit(value, "modelOverrides.minContextLimit")?;
                entry.min_context = absolute;
                entry.min_context_percent = percent;
            }
            if let Some(value) = value.get("maxContextLimit") {
                let (absolute, percent) = parse_limit(value, "modelOverrides.maxContextLimit")?;
                entry.max_context = absolute;
                entry.max_context_percent = percent;
            }
            if let Some(v) = value.get("nudgeFrequency").and_then(positive_u64) {
                entry.nudge_frequency = Some(v);
            }
            config.model_overrides.insert(model.clone(), entry);
        }
    }
    if let Some(compress) = obj.get("compress") {
        let compress = compress
            .as_object()
            .ok_or_else(|| invalid("compress must be an object"))?;
        if compress
            .get("mode")
            .and_then(|v| v.as_str())
            .is_some_and(|mode| mode != "range")
        {
            return Err(DcpAutoError::UnsupportedOption {
                reason: "only range compression mode is supported".to_string(),
            });
        }
        if let Some(permission) = compress.get("permission") {
            config.compress_permission = Some(
                serde_json::from_value(permission.clone())
                    .map_err(|_| invalid("compress.permission must be allow/ask/deny"))?,
            );
        }
        if let Some(value) = compress.get("summaryBuffer").and_then(|v| v.as_bool()) {
            config.summary_buffer = value;
        }
        if let Some(value) = compress.get("showCompression") {
            config.show_compression = value
                .as_bool()
                .ok_or_else(|| invalid("compress.showCompression must be boolean"))?;
        }
        if let Some(value) = compress.get("minContextLimit") {
            let (absolute, percent) = parse_limit(value, "compress.minContextLimit")?;
            if let Some(value) = absolute {
                config.min_context = value;
            }
            config.min_context_percent = percent;
        }
        if let Some(value) = compress.get("maxContextLimit") {
            let (absolute, percent) = parse_limit(value, "compress.maxContextLimit")?;
            if let Some(value) = absolute {
                config.max_context = value;
            }
            config.max_context_percent = percent;
        }
        if let Some(value) = compress.get("nudgeFrequency").and_then(positive_u64) {
            config.nudge_frequency = value;
        }
        if let Some(value) = compress
            .get("iterationNudgeThreshold")
            .and_then(positive_u64)
        {
            config.iteration_threshold = value;
        }
        match compress.get("nudgeForce").and_then(|v| v.as_str()) {
            Some("strong") => config.nudge_force = NudgeForce::Hard,
            Some("soft") | None => {}
            Some(other) => return Err(invalid(&format!("unknown nudgeForce {other}"))),
        }
        if let Some(value) = compress.get("protectTags").and_then(|v| v.as_bool()) {
            config.protect_tags = value;
        }
        if let Some(value) = compress
            .get("protectUserMessages")
            .and_then(|v| v.as_bool())
        {
            config.protect_user_messages = value;
        }
        append_strings(&mut config.protected_tools, compress.get("protectedTools"))?;
        if let Some(overrides) = compress.get("modelOverrides").and_then(|v| v.as_object()) {
            load_model_overrides(&mut config.model_overrides, overrides)?;
        }
        load_limit_map(
            &mut config.model_overrides,
            compress.get("modelMinLimits"),
            true,
        )?;
        load_limit_map(
            &mut config.model_overrides,
            compress.get("modelMaxLimits"),
            false,
        )?;
    }
    if let Some(commands) = obj.get("commands") {
        let commands = commands
            .as_object()
            .ok_or_else(|| invalid("commands must be an object"))?;
        if let Some(value) = commands.get("enabled") {
            config.commands_enabled = value
                .as_bool()
                .ok_or_else(|| invalid("commands.enabled must be boolean"))?;
        }
        append_strings(&mut config.protected_tools, commands.get("protectedTools"))?;
    }
    if let Some(turns) = obj.get("turnProtection") {
        let turns = turns
            .as_object()
            .ok_or_else(|| invalid("turnProtection must be an object"))?;
        if let Some(value) = turns.get("enabled") {
            config.turn_protection = value
                .as_bool()
                .ok_or_else(|| invalid("turnProtection.enabled must be boolean"))?;
        }
        if let Some(value) = turns.get("turns") {
            config.turn_protection_turns = positive_u64(value)
                .ok_or_else(|| invalid("turnProtection.turns must be positive"))?;
        }
    }
    if let Some(value) = obj.get("pruneNotificationType") {
        let value = value
            .as_str()
            .filter(|value| matches!(*value, "chat" | "toast"))
            .ok_or_else(|| invalid("pruneNotificationType must be chat or toast"))?;
        config.prune_notification_type = value.to_string();
    }
    config.protected_tools.sort();
    config.protected_tools.dedup();
    config.dedup_protected_tools.sort();
    config.dedup_protected_tools.dedup();
    config.purge_protected_tools.sort();
    config.purge_protected_tools.dedup();
    for pattern in config
        .protected_tools
        .iter()
        .chain(&config.dedup_protected_tools)
        .chain(&config.purge_protected_tools)
    {
        if pattern.is_empty() || pattern.len() > 256 || pattern.contains('\0') {
            return Err(invalid(
                "protected tool patterns must be 1..=256 bytes without NUL",
            ));
        }
    }
    if config.min_context_percent.is_none()
        && config.max_context_percent.is_none()
        && config.min_context > config.max_context
    {
        return Err(invalid("minContextLimit exceeds maxContextLimit"));
    }
    Ok((config, warnings))
}

fn strategy_enabled(value: Option<&serde_json::Value>) -> Option<bool> {
    match value? {
        serde_json::Value::Bool(value) => Some(*value),
        value => value.get("enabled").and_then(|v| v.as_bool()),
    }
}

fn append_strings(
    output: &mut Vec<String>,
    value: Option<&serde_json::Value>,
) -> Result<(), DcpAutoError> {
    let Some(value) = value else { return Ok(()) };
    let values = value
        .as_array()
        .ok_or_else(|| DcpAutoError::InvalidConfig {
            reason: "protected patterns must be an array".to_string(),
        })?;
    for value in values {
        let value = value.as_str().ok_or_else(|| DcpAutoError::InvalidConfig {
            reason: "protected patterns must contain only strings".to_string(),
        })?;
        output.push(value.to_string());
    }
    Ok(())
}

fn load_model_overrides(
    output: &mut BTreeMap<String, ModelOverride>,
    overrides: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), DcpAutoError> {
    for (model, value) in overrides {
        let mut entry = output.get(model).cloned().unwrap_or_default();
        if let Some(value) = value.get("minContextLimit") {
            let (absolute, percent) = parse_limit(value, "modelOverrides.minContextLimit")?;
            entry.min_context = absolute;
            entry.min_context_percent = percent;
        }
        if let Some(value) = value.get("maxContextLimit") {
            let (absolute, percent) = parse_limit(value, "modelOverrides.maxContextLimit")?;
            entry.max_context = absolute;
            entry.max_context_percent = percent;
        }
        if let Some(value) = value.get("nudgeFrequency").and_then(positive_u64) {
            entry.nudge_frequency = Some(value);
        }
        output.insert(model.clone(), entry);
    }
    Ok(())
}

fn load_limit_map(
    output: &mut BTreeMap<String, ModelOverride>,
    value: Option<&serde_json::Value>,
    minimum: bool,
) -> Result<(), DcpAutoError> {
    let Some(value) = value else { return Ok(()) };
    let values = value
        .as_object()
        .ok_or_else(|| DcpAutoError::InvalidConfig {
            reason: if minimum {
                "compress.modelMinLimits must be an object"
            } else {
                "compress.modelMaxLimits must be an object"
            }
            .to_string(),
        })?;
    for (model, value) in values {
        let (absolute, percent) = parse_limit(
            value,
            if minimum {
                "compress.modelMinLimits value"
            } else {
                "compress.modelMaxLimits value"
            },
        )?;
        let entry = output.entry(model.clone()).or_default();
        if minimum {
            entry.min_context = absolute;
            entry.min_context_percent = percent;
        } else {
            entry.max_context = absolute;
            entry.max_context_percent = percent;
        }
    }
    Ok(())
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
        self.effective_for_context(model, self.max_context.max(self.min_context))
    }

    /// Resolve absolute/percentage thresholds against the selected model context.
    pub fn effective_for_context(&self, model: &str, model_context: u64) -> EffectiveThresholds {
        let resolve = |absolute: u64, percent: Option<u32>| {
            percent.map_or(absolute, |basis_points| {
                u64::try_from(
                    u128::from(model_context).saturating_mul(u128::from(basis_points)) / 10_000,
                )
                .unwrap_or(u64::MAX)
                .max(1)
            })
        };
        let mut out = EffectiveThresholds {
            min_context: resolve(self.min_context, self.min_context_percent),
            max_context: resolve(self.max_context, self.max_context_percent),
            nudge_frequency: self.nudge_frequency,
        };
        if let Some(entry) = self.model_overrides.get(model) {
            if let Some(value) = entry.min_context {
                out.min_context = value;
            } else if entry.min_context_percent.is_some() {
                out.min_context = resolve(self.min_context, entry.min_context_percent);
            }
            if let Some(value) = entry.max_context {
                out.max_context = value;
            } else if entry.max_context_percent.is_some() {
                out.max_context = resolve(self.max_context, entry.max_context_percent);
            }
            if let Some(value) = entry.nudge_frequency {
                out.nudge_frequency = value;
            }
        }
        out
    }
}

/// Nudge counters: iterations, turns since compress, last reminder point.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        self.iteration = 0;
        self.turns_since_compress = 0;
        // Start a real cadence cooldown at the compression iteration instead
        // of immediately emitting another reminder on the next request.
        self.last_reminder = Some(0);
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
    model_context: u64,
    estimated_tokens: u64,
    active_summary_tokens: u64,
) -> Option<Nudge> {
    if !config.enabled || config.manual_mode {
        return None;
    }
    let thresholds = config.effective_for_context(model, model_context);
    if estimated_tokens < thresholds.min_context {
        return None;
    }
    let due = state
        .last_reminder
        .is_none_or(|last| state.iteration >= last + thresholds.nudge_frequency);
    if !due {
        return None;
    }
    let max_estimate = if config.summary_buffer {
        estimated_tokens.saturating_sub(active_summary_tokens)
    } else {
        estimated_tokens
    };
    let over_max = max_estimate > thresholds.max_context;
    let force = if over_max || state.iteration >= config.iteration_threshold {
        NudgeForce::Hard
    } else {
        config.nudge_force
    };
    state.last_reminder = Some(state.iteration);
    state.emitted += 1;
    Some(Nudge {
        force,
        text: if over_max {
            format!(
                "context estimate {estimated_tokens} exceeds soft limit {}; compress a closed span",
                thresholds.max_context
            )
        } else {
            format!(
                "context estimate {estimated_tokens} reached reminder limit {}; consider compressing a closed span",
                thresholds.min_context
            )
        },
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
        if tool_is_protected(protected_tools, &record.tool) {
            continue;
        }
        last_index.insert((record.tool.clone(), record.arguments.clone()), index);
    }
    records
        .into_iter()
        .enumerate()
        .filter(|(index, record)| {
            tool_is_protected(protected_tools, &record.tool)
                || last_index.get(&(record.tool.clone(), record.arguments.clone())) == Some(index)
        })
        .map(|(_, record)| record)
        .collect()
}

/// Match configured tool protections. `*` spans zero or more characters and
/// `?` spans one character; patterns are bounded by config validation.
pub fn tool_is_protected(patterns: &[String], tool: &str) -> bool {
    patterns.iter().any(|pattern| wildcard_match(pattern, tool))
}

fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut previous = vec![false; value.len() + 1];
    previous[0] = true;
    for byte in pattern {
        let mut current = vec![false; value.len() + 1];
        if *byte == b'*' {
            current[0] = previous[0];
        }
        for index in 1..=value.len() {
            current[index] = match *byte {
                b'*' => previous[index] || current[index - 1],
                b'?' => previous[index - 1],
                literal => previous[index - 1] && literal == value[index - 1],
            };
        }
        previous = current;
    }
    previous[value.len()]
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

/// Compiled module identity: all admitted aliases bind one instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DcpModuleId {
    /// Visible resolved revision.
    pub revision: String,
}

/// Resolve a plugin identity to the single compiled DCP module.
pub fn resolve_dcp_module(identity: &str) -> Result<DcpModuleId, DcpAutoError> {
    if matches!(
        identity,
        "@tarquinen/opencode-dcp"
            | "@tarquinen/opencode-dcp@3.1.15"
            | "@tarquinen/opencode-dcp@latest"
    ) {
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
            assert!(evaluate(&config, &mut state, "m", 1_000_000, 10_000, 0).is_none());
        }
        // Over threshold fires once, then stays quiet until the cadence elapses.
        let first = evaluate(&config, &mut state, "m", 1_000_000, 200_000, 0).expect("nudge");
        assert_eq!(first.force, NudgeForce::Hard);
        assert!(first.text.contains("200000"));
        assert!(!first.text.contains("transcript"));
        for _ in 0..4 {
            state.on_turn();
            assert!(evaluate(&config, &mut state, "m", 1_000_000, 200_000, 0).is_none());
        }
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 1_000_000, 200_000, 0).is_some());
        assert_eq!(state.emitted, 2);
        // Successful compression starts a cadence cooldown.
        state.on_compress_success();
        assert_eq!(state.iteration, 0, "compression resets force iteration");
        assert!(evaluate(&config, &mut state, "m", 1_000_000, 200_000, 0).is_none());
        for _ in 0..config.nudge_frequency {
            state.on_turn();
        }
        assert_eq!(
            evaluate(&config, &mut state, "m", 1_000_000, 75_000, 0)
                .expect("cadence resumes after reset")
                .force,
            NudgeForce::Soft,
            "pre-compression hard iteration state must not leak"
        );
        // Per-model override changes the cadence without code changes.
        let fragment = serde_json::json!({"modelOverrides": {"m": {"nudgeFrequency": 2}}});
        let (config, _) = load_config(&fragment).expect("config");
        let mut state = NudgeState::default();
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 1_000_000, 200_000, 0).is_some());
        // Manual mode silences autonomous nudges entirely.
        let fragment = serde_json::json!({"manualMode": true});
        let (config, _) = load_config(&fragment).expect("config");
        let mut state = NudgeState::default();
        state.on_turn();
        assert!(evaluate(&config, &mut state, "m", 1_000_000, 9_999_999, 0).is_none());
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
        // Subagents are a future feature: the flag is accepted visibly.
        let (accepted, warnings) =
            load_config(&serde_json::json!({"experimental": {"allowSubAgents": true}}))
                .expect("allowSubAgents is tolerated until subagents land");
        assert_eq!(accepted.enabled, DcpConfig::default().enabled);
        assert!(
            warnings.iter().any(|w| w.contains("allowSubAgents")),
            "the ignored option is reported: {warnings:?}"
        );
        // Bare, pinned, and the user-required latest alias bind one compiled instance.
        let bare = resolve_dcp_module("@tarquinen/opencode-dcp").expect("bare");
        let pinned = resolve_dcp_module("@tarquinen/opencode-dcp@3.1.15").expect("pinned");
        let latest = resolve_dcp_module("@tarquinen/opencode-dcp@latest").expect("latest");
        assert_eq!(bare, pinned);
        assert_eq!(bare, latest);
        assert!(!bare.revision.is_empty());
        assert!(resolve_dcp_module("@tarquinen/opencode-dcp@3.1.14").is_err());
        assert!(resolve_dcp_module("https://x.invalid/p.js").is_err());
        let (nested, warnings) = load_config(&serde_json::json!({
            "manualMode": {"enabled": true, "automaticStrategies": true},
            "pruneNotification": "detailed",
            "commands": {"enabled": true, "protectedTools": ["apply_patch"]},
            "compress": {
                "mode": "range", "minContextLimit": 7, "maxContextLimit": 70,
                "nudgeFrequency": 3, "protectTags": true,
                "protectUserMessages": true, "protectedTools": ["skill"]
            },
            "strategies": {
                "deduplication": {"enabled": true, "protectedTools": ["bash"]},
                "purgeErrors": {"enabled": true, "turns": 2}
            }
        }))
        .expect("nested required-profile shape");
        assert!(warnings.is_empty());
        assert!(nested.manual_mode && nested.automatic_strategies);
        assert_eq!(
            (
                nested.min_context,
                nested.max_context,
                nested.nudge_frequency
            ),
            (7, 70, 3)
        );
        assert!(nested.protect_tags && nested.protect_user_messages);
        assert_eq!(nested.purge_after_turns, 2);
        assert_eq!(nested.protected_tools, ["apply_patch", "skill"]);
        assert_eq!(nested.dedup_protected_tools, ["bash"]);
        // Stats/debug carry counts only.
        let stats = DcpStats {
            nudges_emitted: 2,
            compressions: 1,
            prunes: 0,
        };
        let line = debug_line("prune", &stats);
        assert!(line.contains("nudges=2"));
    }

    #[test]
    fn aud20_percent_model_limits_strong_nudge_and_wildcard_protection() {
        let (config, warnings) = load_config(&serde_json::json!({
            "$schema": "https://example.invalid/dcp.schema.json",
            "autoUpdate": false,
            "debug": true,
            "pruneNotificationType": "toast",
            "commands": {"enabled": false, "protectedTools": ["mcp_*", "tool_?"]},
            "turnProtection": {"enabled": true, "turns": 6},
            "compress": {
                "minContextLimit": "25%",
                "maxContextLimit": "80.5%",
                "modelMinLimits": {"provider/model": "40%"},
                "modelMaxLimits": {"provider/model": 70000},
                "nudgeForce": "strong",
                "showCompression": true
            }
        }))
        .expect("full required config shape");
        assert!(warnings.is_empty());
        assert!(config.debug && config.turn_protection && config.show_compression);
        assert!(!config.commands_enabled);
        assert_eq!(config.turn_protection_turns, 6);
        assert_eq!(config.prune_notification_type, "toast");
        let generic = config.effective_for_context("other/model", 200_000);
        assert_eq!(
            (generic.min_context, generic.max_context),
            (50_000, 161_000)
        );
        let exact = config.effective_for_context("provider/model", 200_000);
        assert_eq!((exact.min_context, exact.max_context), (80_000, 70_000));
        assert_eq!(config.nudge_force, NudgeForce::Hard);
        assert!(super::tool_is_protected(
            &config.protected_tools,
            "mcp_search"
        ));
        assert!(super::tool_is_protected(&config.protected_tools, "tool_a"));
        assert!(!super::tool_is_protected(
            &config.protected_tools,
            "tool_long"
        ));
        assert!(matches!(
            load_config(&serde_json::json!({"autoUpdate": true})),
            Err(DcpAutoError::UnsupportedOption { .. })
        ));
        assert!(
            load_config(&serde_json::json!({"compress": {"maxContextLimit": "101%"}})).is_err()
        );
        assert!(load_config(&serde_json::json!({"compress": {"nudgeForce": "hard"}})).is_err());

        let mut buffered = DcpConfig {
            min_context: 50,
            max_context: 100,
            nudge_frequency: 1,
            summary_buffer: true,
            ..DcpConfig::default()
        };
        let mut state = NudgeState::default();
        state.on_turn();
        assert_eq!(
            evaluate(&buffered, &mut state, "m", 1_000, 150, 100)
                .unwrap()
                .force,
            NudgeForce::Soft,
            "summary tokens extend max but must not suppress min reminder"
        );
        buffered.summary_buffer = false;
        let mut state = NudgeState::default();
        state.on_turn();
        assert_eq!(
            evaluate(&buffered, &mut state, "m", 1_000, 150, 100)
                .unwrap()
                .force,
            NudgeForce::Hard
        );
    }
}
