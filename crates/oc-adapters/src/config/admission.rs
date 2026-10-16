//! Fail-closed OpenCode 2 key allowlist (T66, D28).
//!
//! Only keys of the canonical OpenCode 2 schema are admitted: config `Info` of
//! `packages/schema` v2.0.12 (plus the T65-oracle provider `compatibility`), the
//! cli.json `Config.Info` of `packages/tui` and its keybind names, and declared
//! native extensions. Any other key is reported and the configuration refused; it
//! is never migrated, ignored or mapped to a replacement. Open records (`settings`,
//! `headers`, `body`, `env`, `environment`, plugin `options`) and value shapes are
//! validated by their owners. Paths are schema-only: map keys become `<id>`, array
//! items `[]`, unsafe key spellings `<key>`, and no values are printed.
use oc_core::queries::{ServiceAction, ServiceCode, ServiceDiagnostic, ServiceKind, ServiceStage};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    pub(crate) path: String,
}

/// `Info` keys plus native extensions `dcp`, `animations` and `terminal`.
const ROOT: &[&str] = &[
    "$schema",
    "shell",
    "model",
    "default_agent",
    "update",
    "share",
    "enterprise",
    "username",
    "permissions",
    "agents",
    "snapshots",
    "watcher",
    "formatter",
    "lsp",
    "media",
    "tool_output",
    "mcp",
    "compaction",
    "skills",
    "commands",
    "instructions",
    "references",
    "websearch",
    "plugins",
    "worktree",
    "warming",
    "providers",
    "experimental",
    "dcp",
    "animations",
    "terminal",
];

const EXPERIMENTAL: &[&str] = &["portable_shell_scanner", "subagent_depth", "policies"];
const COMPACTION: &[&str] = &["auto", "keep", "buffer"];

const PROVIDER: &[&str] = &[
    "canonical",
    "name",
    "env",
    "package",
    "compatibility",
    "settings",
    "headers",
    "body",
    "models",
];

const MODEL: &[&str] = &[
    "modelID",
    "family",
    "name",
    "compatibility",
    "package",
    "settings",
    "headers",
    "body",
    "capabilities",
    "variants",
    "cost",
    "disabled",
    "limit",
];

/// Native extension: `disabled` hides a configured or discovered variant.
const VARIANT: &[&str] = &["id", "settings", "headers", "body", "disabled"];
const CAPABILITIES: &[&str] = &["tools", "input", "output"];
const LIMIT: &[&str] = &["context", "input", "output"];

const MCP: &[&str] = &["timeout", "servers"];
const TIMEOUT: &[&str] = &["startup", "catalog", "execution"];
/// Local and remote server fields. A field of the other transport is the MCP
/// owner's isolated per-server failure, not a configuration-wide refusal.
const SERVER: &[&str] = &[
    "type",
    "command",
    "cwd",
    "environment",
    "url",
    "headers",
    "oauth",
    "disabled",
    "codemode",
    "timeout",
    "protocol",
];
const OAUTH: &[&str] = &[
    "client_id",
    "client_secret",
    "scope",
    "callback_port",
    "redirect_uri",
    "auth_server_metadata_url",
];

/// `ConfigAgent.Info`; Markdown frontmatter additionally selects a `variant`.
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
const REQUEST: &[&str] = &["headers", "body"];
const COMMAND: &[&str] = &[
    "template",
    "description",
    "agent",
    "model",
    "subagent",
    "subtask",
];
const PLUGIN: &[&str] = &["package", "options"];

const CLI: &[&str] = &[
    "$schema",
    "theme",
    "keybinds",
    "plugins",
    "leader",
    "scroll",
    "attention",
    "diffs",
    "terminal",
    "prompt",
    "session",
    "tabs",
    "mini",
    "debug",
    "experimental",
    "animations",
    "mouse",
    "cursor",
];

/// Closed cli.json sections; `experimental` is an open boolean record.
const CLI_SECTIONS: &[(&str, &[&str])] = &[
    ("theme", &["name", "mode"]),
    ("leader", &["timeout"]),
    ("scroll", &["speed", "acceleration"]),
    (
        "attention",
        &["notifications", "sound", "volume", "sound_pack", "sounds"],
    ),
    ("diffs", &["source", "wrap", "tree", "single", "view"]),
    ("terminal", &["title", "copy"]),
    ("prompt", &["editor", "paste", "image_preview"]),
    (
        "session",
        &[
            "sidebar",
            "scrollbar",
            "thinking",
            "grouping",
            "image_preview",
            "tps",
            "markdown",
            "new_location",
            "permissions",
        ],
    ),
    ("tabs", &["enabled", "scope", "layout", "indicators"]),
    (
        "mini",
        &[
            "thinking",
            "tools",
            "shell_output",
            "turn_summary",
            "footer",
            "splash",
            "work_spinner",
            "mono",
            "replay",
            "replay_limit",
        ],
    ),
    ("debug", &["devtools", "timing", "turn_tokens"]),
    ("cursor", &["style", "blinking"]),
];

/// Donor `TuiKeybind.Definitions` names (`packages/tui/src/config/keybind.ts`).
const KEYBINDS: &[&str] = &[
    "agent.cycle",
    "agent.cycle.reverse",
    "agent.list",
    "app.clear",
    "app.console",
    "app.debug",
    "app.exit",
    "app.scrap",
    "app.toggle.animations",
    "app.toggle.diffwrap",
    "app.toggle.file_context",
    "app.toggle.paste_summary",
    "command.palette.show",
    "composer.shell.down",
    "composer.shell.kill",
    "composer.shell.select",
    "composer.shell.up",
    "composer.subagent.down",
    "composer.subagent.interrupt",
    "composer.subagent.select",
    "composer.subagent.up",
    "composer.terminal.down",
    "composer.terminal.select",
    "composer.terminal.up",
    "dialog.integration.delete",
    "dialog.integration.rename",
    "dialog.mcp.toggle",
    "dialog.move_session.delete",
    "dialog.move_session.move",
    "dialog.move_session.new",
    "dialog.move_session.refresh",
    "dialog.plugins.check",
    "dialog.plugins.error",
    "dialog.plugins.install",
    "dialog.plugins.update",
    "dialog.prompt.submit",
    "dialog.select.end",
    "dialog.select.home",
    "dialog.select.next",
    "dialog.select.page_down",
    "dialog.select.page_up",
    "dialog.select.prev",
    "dialog.select.submit",
    "dialog.worktree.generate",
    "diff.close",
    "diff.collapse",
    "diff.down",
    "diff.expand",
    "diff.expand_all",
    "diff.first",
    "diff.half_page.down",
    "diff.half_page.up",
    "diff.help",
    "diff.last",
    "diff.mark_reviewed",
    "diff.next_file",
    "diff.next_hunk",
    "diff.open",
    "diff.page.down",
    "diff.page.up",
    "diff.previous_file",
    "diff.previous_hunk",
    "diff.single_patch",
    "diff.switch_focus",
    "diff.switch_source",
    "diff.toggle",
    "diff.toggle_file_tree",
    "diff.toggle_view",
    "diff.up",
    "docs.open",
    "help.show",
    "input.backspace",
    "input.buffer.end",
    "input.buffer.home",
    "input.delete",
    "input.delete.line",
    "input.delete.to.line.end",
    "input.delete.to.line.start",
    "input.delete.word.backward",
    "input.delete.word.forward",
    "input.line.end",
    "input.line.home",
    "input.move.down",
    "input.move.left",
    "input.move.right",
    "input.move.up",
    "input.newline",
    "input.redo",
    "input.select.all",
    "input.select.buffer.end",
    "input.select.buffer.home",
    "input.select.down",
    "input.select.left",
    "input.select.line.end",
    "input.select.line.home",
    "input.select.right",
    "input.select.up",
    "input.select.visual.line.end",
    "input.select.visual.line.home",
    "input.select.word.backward",
    "input.select.word.forward",
    "input.submit",
    "input.undo",
    "input.visual.line.end",
    "input.visual.line.home",
    "input.word.backward",
    "input.word.forward",
    "leader",
    "location.reload",
    "mcp.list",
    "messages.copy",
    "model.cycle_favorite",
    "model.cycle_favorite_reverse",
    "model.cycle_recent",
    "model.cycle_recent_reverse",
    "model.dialog.favorite",
    "model.dialog.provider",
    "model.list",
    "open.menu",
    "opencode.debug",
    "opencode.settings",
    "opencode.status",
    "pane.focus.left",
    "pane.focus.right",
    "permission.prompt.fullscreen",
    "plugins.install",
    "plugins.list",
    "plugins.toggle",
    "prompt.autocomplete.complete",
    "prompt.autocomplete.hide",
    "prompt.autocomplete.next",
    "prompt.autocomplete.prev",
    "prompt.autocomplete.select",
    "prompt.clear",
    "prompt.editor",
    "prompt.editor_context.clear",
    "prompt.history.next",
    "prompt.history.previous",
    "prompt.images.view",
    "prompt.paste",
    "prompt.queue",
    "prompt.skills",
    "prompt.stash",
    "prompt.stash.list",
    "prompt.stash.pop",
    "prompt.submit",
    "provider.connect",
    "queued_prompt.delete",
    "server.pair",
    "service.restart",
    "session.aside",
    "session.background",
    "session.cd",
    "session.child.first",
    "session.child.next",
    "session.child.previous",
    "session.compact",
    "session.copy",
    "session.copy.id",
    "session.delete",
    "session.export",
    "session.first",
    "session.fork",
    "session.half.page.down",
    "session.half.page.up",
    "session.interrupt",
    "session.last",
    "session.line.down",
    "session.line.up",
    "session.list",
    "session.message.next",
    "session.message.previous",
    "session.message.user.next",
    "session.message.user.previous",
    "session.messages_last_user",
    "session.move",
    "session.new",
    "session.page.down",
    "session.page.up",
    "session.parent",
    "session.pin.toggle",
    "session.queued_prompts",
    "session.quick_switch.1",
    "session.quick_switch.2",
    "session.quick_switch.3",
    "session.quick_switch.4",
    "session.quick_switch.5",
    "session.quick_switch.6",
    "session.quick_switch.7",
    "session.quick_switch.8",
    "session.quick_switch.9",
    "session.redo",
    "session.rename",
    "session.share",
    "session.sidebar.toggle",
    "session.tab.close",
    "session.tab.history.back",
    "session.tab.history.forward",
    "session.tab.next",
    "session.tab.next_unread",
    "session.tab.previous",
    "session.tab.previous_unread",
    "session.tab.reopen",
    "session.tab.select.1",
    "session.tab.select.10",
    "session.tab.select.2",
    "session.tab.select.3",
    "session.tab.select.4",
    "session.tab.select.5",
    "session.tab.select.6",
    "session.tab.select.7",
    "session.tab.select.8",
    "session.tab.select.9",
    "session.timeline",
    "session.toggle.exploration_grouping",
    "session.toggle.scrollbar",
    "session.toggle.thinking",
    "session.undo",
    "session.unshare",
    "stash.delete",
    "terminal.close",
    "terminal.select",
    "terminal.suspend",
    "terminal.title.toggle",
    "terminal.toggle",
    "theme.mode.lock",
    "theme.switch",
    "theme.switch_mode",
    "variant.cycle",
    "variant.list",
];

/// Reports keys of `value` outside `allowed`; non-objects belong to their owner.
fn check<'a>(
    out: &mut Vec<Finding>,
    value: Option<&'a Value>,
    prefix: &str,
    allowed: &[&str],
) -> Option<&'a Map<String, Value>> {
    let object = value?.as_object()?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            out.push(Finding {
                path: format!("{prefix}{}", safe_key(key)),
            });
        }
    }
    Some(object)
}

fn entries(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(Map::values)
}

fn items(value: Option<&Value>) -> impl Iterator<Item = &Value> {
    value.and_then(Value::as_array).into_iter().flatten()
}

/// One parsed `opencode.json(c)` document.
pub(crate) fn scan_document(document: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(root) = check(&mut out, Some(document), "", ROOT) else {
        return out;
    };
    check(
        &mut out,
        root.get("experimental"),
        "experimental.",
        EXPERIMENTAL,
    );
    if let Some(compaction) = check(&mut out, root.get("compaction"), "compaction.", COMPACTION) {
        check(
            &mut out,
            compaction.get("keep"),
            "compaction.keep.",
            &["tokens"],
        );
    }
    for provider in entries(root.get("providers")) {
        let Some(provider) = check(&mut out, Some(provider), "providers.<id>.", PROVIDER) else {
            continue;
        };
        for model in entries(provider.get("models")) {
            let prefix = "providers.<id>.models.<id>.";
            let Some(model) = check(&mut out, Some(model), prefix, MODEL) else {
                continue;
            };
            check(
                &mut out,
                model.get("capabilities"),
                &format!("{prefix}capabilities."),
                CAPABILITIES,
            );
            check(
                &mut out,
                model.get("limit"),
                &format!("{prefix}limit."),
                LIMIT,
            );
            for variant in items(model.get("variants")) {
                check(
                    &mut out,
                    Some(variant),
                    &format!("{prefix}variants[]."),
                    VARIANT,
                );
            }
        }
    }
    if let Some(mcp) = check(&mut out, root.get("mcp"), "mcp.", MCP) {
        check(&mut out, mcp.get("timeout"), "mcp.timeout.", TIMEOUT);
        for server in entries(mcp.get("servers")) {
            scan_server(&mut out, server);
        }
    }
    for agent in entries(root.get("agents")) {
        scan_agent(&mut out, "agents.<id>.", agent, false);
    }
    for command in entries(root.get("commands")) {
        check(&mut out, Some(command), "commands.<id>.", COMMAND);
    }
    for plugin in items(root.get("plugins")) {
        check(&mut out, Some(plugin), "plugins[].", PLUGIN);
    }
    dedup(out)
}

fn scan_server(out: &mut Vec<Finding>, server: &Value) {
    let prefix = "mcp.servers.<id>.";
    let Some(server) = check(out, Some(server), prefix, SERVER) else {
        return;
    };
    check(
        out,
        server.get("timeout"),
        &format!("{prefix}timeout."),
        TIMEOUT,
    );
    check(out, server.get("oauth"), &format!("{prefix}oauth."), OAUTH);
}

fn scan_agent(out: &mut Vec<Finding>, prefix: &str, agent: &Value, markdown: bool) {
    let markdown_keys;
    let allowed = if markdown {
        markdown_keys = [AGENT, &["variant"]].concat();
        &markdown_keys
    } else {
        AGENT
    };
    if let Some(agent) = check(out, Some(agent), prefix, allowed) {
        check(
            out,
            agent.get("request"),
            &format!("{prefix}request."),
            REQUEST,
        );
    }
}

/// Markdown agent frontmatter (`agent/`, `agents/`, `mode/`, `modes/`). An
/// excluded agent override would leave a wider built-in profile, so unknown keys
/// refuse the configuration. Markdown commands keep their isolated per-file
/// `unsupported field` diagnostic: they cannot widen authority.
pub(crate) fn scan_agent_frontmatter(fields: &Map<String, Value>) -> Vec<Finding> {
    let mut out = Vec::new();
    scan_agent(&mut out, "", &Value::Object(fields.clone()), true);
    dedup(out)
}

/// One parsed `cli.json(c)` document.
pub(crate) fn scan_cli(document: &Value) -> Vec<Finding> {
    let mut out = Vec::new();
    let Some(root) = check(&mut out, Some(document), "", CLI) else {
        return out;
    };
    for (section, allowed) in CLI_SECTIONS {
        check(
            &mut out,
            root.get(*section),
            &format!("{section}."),
            allowed,
        );
    }
    check(&mut out, root.get("keybinds"), "keybinds.", KEYBINDS);
    for plugin in items(root.get("plugins")) {
        check(&mut out, Some(plugin), "plugins[].", PLUGIN);
    }
    dedup(out)
}

/// Schema-like spellings are echoed so the owner can find the key; anything
/// else (arbitrary text, control characters, long keys) is not reflected.
fn safe_key(key: &str) -> &str {
    let safe = !key.is_empty()
        && key.len() <= 64
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-$".contains(&byte));
    if safe { key } else { "<key>" }
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
            let line = format!("{scope}: {}", finding.path);
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
            field: vec!["unknown".into()],
            stage: ServiceStage::Config,
            code: ServiceCode::UnknownConfigKey,
            action: ServiceAction::RemoveUnknownConfigKeys,
            details: Some(Box::new(details)),
        }
    }
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
