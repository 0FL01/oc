//! Fail-closed OpenCode 1 key detector (T62, D27).
//!
//! Only the canonical OpenCode 2 v2.0.12 schema is admitted. Keys the donor accepts
//! solely through `core/src/v1/**` normalization or the tui.json/kv.json migration are
//! reported with their canonical replacement and never migrated. Paths are schema-only:
//! map keys become `<id>` and no values are printed.
use oc_core::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) path: String,
    pub(crate) replacement: &'static str,
}

const REMOVED: &str = "remove (not in the OpenCode 2 schema)";
const RULES: &str = "permissions: [{action, resource, effect}]";
const POLICIES: &str = "experimental.policies: [{action: \"provider.use\", resource, effect}]";
const TIMEOUT: &str = "timeout: {startup, catalog, execution}";
const VARIANT: &str = "model: \"provider/model#variant\"";

const TOP_LEVEL: &[(&str, &str)] = &[
    ("agent", "agents"),
    ("mode", "agents.<id> with mode: \"primary\""),
    ("provider", "providers"),
    ("tools", RULES),
    ("permission", RULES),
    ("command", "commands"),
    ("plugin", "plugins"),
    ("snapshot", "snapshots"),
    ("autoupdate", "update: \"disable\" | \"notify\" | \"auto\""),
    ("autoshare", "share: \"manual\" | \"auto\" | \"disabled\""),
    ("attachment", "media.image"),
    ("reference", "references"),
    ("small_model", "agents.title.model"),
    ("enabled_providers", POLICIES),
    ("disabled_providers", POLICIES),
    ("subagent_depth", "experimental.subagent_depth"),
    ("keybinds", "cli.json keybinds"),
    ("theme", "cli.json theme"),
    ("tui", "cli.json"),
    ("logLevel", REMOVED),
    ("server", REMOVED),
    ("layout", REMOVED),
];

const EXPERIMENTAL: &[(&str, &str)] = &[
    ("mcp_timeout", "mcp.timeout: {catalog, execution}"),
    ("disable_paste_summary", REMOVED),
    ("batch_tool", REMOVED),
    ("openTelemetry", REMOVED),
    ("primary_tools", REMOVED),
    ("continue_loop_on_deny", REMOVED),
];

const COMPACTION: &[(&str, &str)] = &[
    ("preserve_recent_tokens", "compaction.keep.tokens"),
    ("reserved", "compaction.buffer"),
    ("tail_turns", REMOVED),
    ("prune", REMOVED),
];

const PROVIDER: &[(&str, &str)] = &[
    ("npm", "package"),
    ("api", "settings.baseURL"),
    ("options", "settings / headers / body"),
    ("id", REMOVED),
    ("whitelist", REMOVED),
    ("blacklist", REMOVED),
];

const MODEL: &[(&str, &str)] = &[
    ("id", "modelID"),
    ("tool_call", "capabilities.tools"),
    ("modalities", "capabilities.input / capabilities.output"),
    ("interleaved", "compatibility"),
    ("options", "settings"),
    ("provider", "package / settings"),
    ("status", "disabled"),
    ("release_date", REMOVED),
    ("attachment", REMOVED),
    ("reasoning", REMOVED),
    ("temperature", REMOVED),
    ("experimental", REMOVED),
];

const MCP_OAUTH: &[(&str, &str)] = &[
    ("clientId", "client_id"),
    ("clientSecret", "client_secret"),
    ("redirectUri", "redirect_uri"),
    ("callbackPort", "callback_port"),
];

/// Canonical `ConfigAgent.Info` keys; anything else selects the donor V1 migration.
const AGENT: &[&str] = &[
    "model",
    "request",
    "system",
    "description",
    "mode",
    "hidden",
    "color",
    "steps",
    "disabled",
    "permissions",
];

const AGENT_V1: &[(&str, &str)] = &[
    ("prompt", "system"),
    ("temperature", "request.body"),
    ("top_p", "request.body"),
    ("options", "request.body"),
    ("tools", RULES),
    ("permission", RULES),
    ("disable", "disabled"),
    ("maxSteps", "steps"),
    ("variant", VARIANT),
];

const COMMAND: &[&str] = &[
    "template",
    "description",
    "agent",
    "model",
    "subagent",
    "subtask",
];

const ACTIONS: &[(&str, &str)] = &[
    ("write", "edit"),
    ("patch", "edit"),
    ("apply_patch", "edit"),
    ("bash", "shell"),
    ("task", "subagent"),
];

const CLI: &[(&str, &str)] = &[
    ("plugin", "plugins"),
    ("plugin_enabled", "plugins (\"-<id>\" disables)"),
    ("leader_timeout", "leader.timeout"),
    ("scroll_speed", "scroll.speed"),
    ("scroll_acceleration", "scroll.acceleration"),
    ("diff_style", "diffs.view"),
];

const KEYBINDS: &[(&str, &str)] = &[
    ("session_undo", "keybinds.session.undo"),
    ("session_redo", "keybinds.session.redo"),
    ("command_list", "keybinds.command.palette.show"),
    (
        "permission_prompt_fullscreen",
        "keybinds.permission.prompt.fullscreen",
    ),
    ("app_exit", "keybinds.app.exit"),
    ("session_child_first", "keybinds.session.child.first"),
    ("session_background", "keybinds.session.background"),
    ("history_previous", "keybinds.prompt.history.previous"),
    ("history_next", "keybinds.prompt.history.next"),
];

fn push(out: &mut Vec<Finding>, path: String, replacement: &'static str) {
    out.push(Finding { path, replacement });
}

fn table(
    out: &mut Vec<Finding>,
    object: &Map<String, Value>,
    prefix: &str,
    rules: &[(&str, &'static str)],
) {
    for (key, replacement) in rules {
        if object.contains_key(*key) {
            push(out, format!("{prefix}{key}"), replacement);
        }
    }
}

fn entries<'a>(value: Option<&'a Value>) -> impl Iterator<Item = &'a Map<String, Value>> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|map| map.values().filter_map(Value::as_object))
}

/// One parsed `opencode.json(c)` document.
pub(crate) fn scan_document(document: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(root) = document.as_object() else {
        return out;
    };
    table(&mut out, root, "", TOP_LEVEL);
    if let Some(experimental) = root.get("experimental").and_then(Value::as_object) {
        table(&mut out, experimental, "experimental.", EXPERIMENTAL);
    }
    if let Some(compaction) = root.get("compaction").and_then(Value::as_object) {
        table(&mut out, compaction, "compaction.", COMPACTION);
    }
    if root.get("skills").is_some_and(Value::is_object) {
        push(&mut out, "skills".into(), "skills: [path or URL, ...]");
    }
    for domain in ["provider", "providers"] {
        for provider in entries(root.get(domain)) {
            table(&mut out, provider, &format!("{domain}.<id>."), PROVIDER);
            for model in entries(provider.get("models")) {
                table(
                    &mut out,
                    model,
                    &format!("{domain}.<id>.models.<id>."),
                    MODEL,
                );
                if model.get("variants").is_some_and(Value::is_object) {
                    push(
                        &mut out,
                        format!("{domain}.<id>.models.<id>.variants"),
                        "variants: [{id, settings}]",
                    );
                }
            }
        }
    }
    if let Some(mcp) = root.get("mcp").and_then(Value::as_object) {
        for (key, value) in mcp {
            match key.as_str() {
                "timeout" if !value.is_object() => push(&mut out, "mcp.timeout".into(), TIMEOUT),
                "timeout" => {}
                "servers" => {
                    for server in entries(Some(value)) {
                        scan_mcp(&mut out, "mcp.servers.<id>.", server);
                    }
                }
                _ => {
                    push(&mut out, "mcp.<id>".into(), "mcp.servers.<id>");
                    if let Some(server) = value.as_object() {
                        scan_mcp(&mut out, "mcp.<id>.", server);
                    }
                }
            }
        }
    }
    if let Some(permissions) = root.get("permissions") {
        scan_rules(&mut out, "permissions", permissions);
    }
    for domain in ["agent", "mode", "agents"] {
        for agent in entries(root.get(domain)) {
            scan_agent(&mut out, &format!("{domain}.<id>."), agent, false);
        }
    }
    for domain in ["command", "commands"] {
        for command in entries(root.get(domain)) {
            scan_command(&mut out, &format!("{domain}.<id>."), command);
        }
    }
    dedup(out)
}

fn scan_mcp(out: &mut Vec<Finding>, prefix: &str, server: &Map<String, Value>) {
    if server.contains_key("enabled") {
        push(out, format!("{prefix}enabled"), "disabled (inverted)");
    }
    if server
        .get("timeout")
        .is_some_and(|value| !value.is_object())
    {
        push(out, format!("{prefix}timeout"), TIMEOUT);
    }
    match server.get("oauth") {
        Some(Value::Bool(true)) => push(out, format!("{prefix}oauth"), "oauth: {...} or false"),
        Some(Value::Object(oauth)) => table(out, oauth, &format!("{prefix}oauth."), MCP_OAUTH),
        _ => {}
    }
}

fn scan_rules(out: &mut Vec<Finding>, path: &str, rules: &Value) {
    let Some(rules) = rules.as_array() else {
        push(out, path.into(), RULES);
        return;
    };
    for rule in rules {
        if let Some(action) = rule.get("action").and_then(Value::as_str)
            && let Some((_, canonical)) = ACTIONS.iter().find(|(legacy, _)| *legacy == action)
        {
            push(out, format!("{path}[].action \"{action}\""), canonical);
        }
    }
}

fn scan_agent(out: &mut Vec<Finding>, prefix: &str, agent: &Map<String, Value>, markdown: bool) {
    for key in agent.keys() {
        if AGENT.contains(&key.as_str()) || (markdown && key == "variant") {
            continue;
        }
        let replacement = AGENT_V1
            .iter()
            .find(|(legacy, _)| legacy == key)
            .map_or(REMOVED, |(_, canonical)| *canonical);
        push(out, format!("{prefix}{}", safe_key(key)), replacement);
    }
    if let Some(rules) = agent.get("permissions") {
        scan_rules(out, &format!("{prefix}permissions"), rules);
    }
}

fn scan_command(out: &mut Vec<Finding>, prefix: &str, command: &Map<String, Value>) {
    for key in command.keys() {
        if !COMMAND.contains(&key.as_str()) {
            let replacement = if key == "variant" { VARIANT } else { REMOVED };
            push(out, format!("{prefix}{}", safe_key(key)), replacement);
        }
    }
}

/// Markdown agent frontmatter (`agent/`, `agents/`, `mode/`, `modes/`).
pub(crate) fn scan_agent_frontmatter(fields: &Map<String, Value>) -> Vec<Finding> {
    let mut out = Vec::new();
    scan_agent(&mut out, "", fields, true);
    dedup(out)
}

/// Markdown command frontmatter; the Markdown body is the canonical template.
pub(crate) fn scan_command_frontmatter(fields: &Map<String, Value>) -> Vec<Finding> {
    let mut out = Vec::new();
    scan_command(&mut out, "", fields);
    dedup(out)
}

/// One parsed `cli.json(c)` document.
pub(crate) fn scan_cli(document: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(root) = document.as_object() else {
        return out;
    };
    table(&mut out, root, "", CLI);
    if root.get("theme").is_some_and(Value::is_string) {
        push(&mut out, "theme".into(), "theme: {name}");
    }
    if let Some(terminal) = root.get("terminal").and_then(Value::as_object) {
        table(
            &mut out,
            terminal,
            "terminal.",
            &[("copy_on_select", "terminal.copy: \"select\" | \"manual\"")],
        );
    }
    if let Some(prompt) = root.get("prompt").and_then(Value::as_object) {
        table(
            &mut out,
            prompt,
            "prompt.",
            &[("max_height", REMOVED), ("max_width", REMOVED)],
        );
    }
    if let Some(attention) = root.get("attention").and_then(Value::as_object) {
        table(&mut out, attention, "attention.", &[("enabled", REMOVED)]);
    }
    if let Some(keybinds) = root.get("keybinds").and_then(Value::as_object) {
        table(&mut out, keybinds, "keybinds.", KEYBINDS);
    }
    dedup(out)
}

/// Only known schema names are echoed; arbitrary keys are not reflected.
fn safe_key(key: &str) -> &str {
    let known = AGENT_V1
        .iter()
        .map(|(legacy, _)| *legacy)
        .chain(["name", "body"])
        .any(|legacy| legacy == key);
    if known { key } else { "<key>" }
}

fn dedup(findings: Vec<Finding>) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::with_capacity(findings.len());
    for finding in findings {
        if !out.contains(&finding) {
            out.push(finding);
        }
    }
    out
}

/// Bounded presentation of the complete report.
const MAX_LINES: usize = 64;

/// Collects findings per static scope label (`global opencode.jsonc`,
/// `location .opencode agent definitions`, ...) in admission order.
#[derive(Debug, Default)]
pub(crate) struct Report {
    lines: Vec<(String, usize)>,
}

impl Report {
    pub(crate) fn add(&mut self, scope: &str, findings: &[Finding]) {
        for finding in findings {
            let line = format!("{scope}: {} → {}", finding.path, finding.replacement);
            match self
                .lines
                .iter_mut()
                .find(|(existing, _)| *existing == line)
            {
                Some((_, count)) => *count += 1,
                None => self.lines.push((line, 1)),
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub(crate) fn diagnostic(&self) -> ServiceDiagnostic {
        let mut details: Vec<String> = self
            .lines
            .iter()
            .take(MAX_LINES)
            .map(|(line, count)| match count {
                1 => line.clone(),
                count => format!("{line} (×{count})"),
            })
            .collect();
        if self.lines.len() > MAX_LINES {
            details.push(format!("… {} more", self.lines.len() - MAX_LINES));
        }
        ServiceDiagnostic {
            kind: ServiceKind::Configuration,
            service: "native".into(),
            source: "config".into(),
            field: vec!["legacy".into()],
            stage: ServiceStage::Config,
            code: ServiceCode::LegacyConfig,
            action: ServiceAction::ReplaceLegacyConfig,
            details,
        }
    }
}

#[cfg(test)]
#[path = "legacy_tests.rs"]
mod tests;
