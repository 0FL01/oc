//! Workspace definitions and instructions loading (T25, CFG06/CFG07).
//!
//! Disk discovery for admitted `.opencode` roots only: skills
//! (`{skill,skills}/<id>/SKILL.md`), agents (`{agent,agents}/<id>.md`),
//! commands (`{command,commands}/<id>.md`). Singular roots load before
//! plural within one source directory; later sources replace duplicates
//! with shadowing provenance. Authoritative inline definitions come from
//! top-level config `agent`/`command` domains through
//! [`merge_config_definitions`], never invented sibling files. Order never
//! depends on filesystem enumeration (entries are sorted). Invalid entries
//! produce path/field/reason diagnostics while valid siblings survive;
//! skill bodies stay out of the model projection (served bounded by the
//! native `skill` tool from the pinned snapshot). Nothing here executes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::{Permission, legacy_key, normalize_permission, parse_skill};
use thiserror::Error;

/// Max admitted definition roots per load.
pub const MAX_DEF_ROOTS: usize = 8;
/// Max definition id length.
pub const MAX_DEF_ID_LEN: usize = 128;
/// Max agent/command body bytes.
pub const MAX_DEF_BODY: usize = 16 * 1024;
/// Max skill file bytes (mirrors `parse_skill`).
pub const MAX_SKILL_FILE: usize = 64 * 1024;
/// Max total definition bytes per load.
pub const MAX_TOTAL_BYTES: usize = 1024 * 1024;
/// Max definitions per kind per load.
pub const MAX_DEFS_PER_KIND: usize = 256;
/// Max single instructions file bytes.
pub const MAX_INSTRUCTIONS_FILE: usize = 64 * 1024;
/// Max total instructions bytes.
pub const MAX_INSTRUCTIONS_TOTAL: usize = 256 * 1024;

/// One admitted `.opencode` source directory in low→high precedence order.
#[derive(Debug, Clone)]
pub struct DefRoot {
    /// Admitted directory (already trust-decided by the caller).
    pub dir: PathBuf,
    /// Provenance label (e.g. `G`, `A`).
    pub origin: String,
}

/// Path/field/reason diagnostic; never carries file bodies or secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// File path.
    pub path: String,
    /// Field or rule.
    pub field: String,
    /// Human reason.
    pub reason: String,
}

/// Loaded skill (body pinned for the snapshot, never projected).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDef {
    /// Directory id.
    pub id: String,
    /// Bounded name.
    pub name: String,
    /// Bounded description.
    pub description: String,
    /// Full `SKILL.md` text for the pinned snapshot.
    pub body: String,
    /// Winning source origin.
    pub origin: String,
}

/// Selectable primary-agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDef {
    /// File id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Optional pinned model id (validated at turn time, not here).
    pub model: Option<String>,
    /// Optional pinned variant.
    pub variant: Option<String>,
    /// Literal primary-agent prompt/body.
    pub body: String,
    /// Agent-specific permission narrowing, normalized to runtime tool names.
    pub permissions: BTreeMap<String, Permission>,
    /// Admitted mode (`primary`) when explicitly configured.
    pub mode: Option<String>,
    /// Winning source origin.
    pub origin: String,
}

/// Non-executable command (literal expansion only, via `expand_command`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDef {
    /// File id.
    pub id: String,
    /// Bounded description.
    pub description: String,
    /// Literal body.
    pub body: String,
    /// Winning source origin.
    pub origin: String,
}

/// Loaded workspace definitions with pinned order and diagnostics.
#[derive(Debug, Clone, Default)]
pub struct LoadedDefs {
    /// Skills by id.
    pub skills: BTreeMap<String, SkillDef>,
    /// Agents by id.
    pub agents: BTreeMap<String, AgentDef>,
    /// Commands by id.
    pub commands: BTreeMap<String, CommandDef>,
    /// First-seen pinned order (`kind.id@origin`).
    pub order: Vec<String>,
    /// Shadowing notes (`kind.id`: old → new origin).
    pub shadowed: Vec<String>,
    /// Per-file diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// Primary-agent selection failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DefError {
    /// Unknown agent id.
    #[error("unknown agent {0}")]
    UnknownAgent(String),
    /// Selected profile invalid.
    #[error("invalid agent {id}: {reason}")]
    InvalidAgent {
        /// Agent id.
        id: String,
        /// Reason.
        reason: String,
    },
}

fn diag(path: &Path, field: &str, reason: &str) -> Diagnostic {
    Diagnostic {
        path: path.to_string_lossy().to_string(),
        field: field.to_string(),
        reason: reason.to_string(),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_DEF_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Read a file with no-follow symlink refusal and containment check.
fn read_plain(path: &Path, expected_dir: &Path, cap: usize) -> Result<String, String> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "unreadable".to_string())?;
    if meta.file_type().is_symlink() {
        return Err("symlink refused".to_string());
    }
    if !meta.is_file() {
        return Err("not a file".to_string());
    }
    if path.parent() != Some(expected_dir) {
        return Err("outside admitted directory".to_string());
    }
    if meta.len() > cap as u64 {
        return Err("file too large".to_string());
    }
    std::fs::read_to_string(path).map_err(|_| "unreadable".to_string())
}

#[derive(Default)]
struct Frontmatter {
    fields: BTreeMap<String, String>,
    permissions: BTreeMap<String, Permission>,
}

/// Split and strictly parse the bounded Markdown YAML subset.
fn split_frontmatter(text: &str) -> Result<(Frontmatter, &str), String> {
    let mut parsed = Frontmatter::default();
    let after = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        Some(after) => after,
        None => return Ok((parsed, text)),
    };
    let mut offset = 0usize;
    let mut lines = 0usize;
    let mut permission_section = false;
    for line in after.split_inclusive('\n') {
        lines += 1;
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let next_offset = offset + line.len();
        if trimmed == "---" {
            return Ok((parsed, &after[next_offset..]));
        }
        offset = next_offset;
        if lines > 64 || line.len() > 1024 {
            return Err("frontmatter too large".to_string());
        }
        if trimmed.trim().is_empty() {
            continue;
        }
        if let Some(nested) = trimmed.strip_prefix("  ") {
            if !permission_section || nested.starts_with(char::is_whitespace) {
                return Err("unsupported nested frontmatter".to_string());
            }
            let (key, value) = nested
                .split_once(':')
                .ok_or_else(|| "malformed permission entry".to_string())?;
            let key = key.trim();
            if !valid_id(key) {
                return Err("invalid permission key".to_string());
            }
            let raw = serde_json::Value::String(value.trim().trim_matches('"').to_string());
            let level = normalize_permission(key, &raw).map_err(|e| e.to_string())?;
            let key = legacy_key(key).to_string();
            parsed
                .permissions
                .entry(key)
                .and_modify(|old| {
                    if permission_rank(level) > permission_rank(*old) {
                        *old = level;
                    }
                })
                .or_insert(level);
            continue;
        }
        if trimmed.starts_with(char::is_whitespace) {
            return Err("unsupported frontmatter indentation".to_string());
        }
        let (key, value) = trimmed
            .split_once(':')
            .ok_or_else(|| "malformed frontmatter field".to_string())?;
        let key = key.trim();
        let value = value.trim();
        permission_section = key == "permission";
        if permission_section {
            if !value.is_empty() {
                return Err("permission must be a mapping".to_string());
            }
            continue;
        }
        if parsed
            .fields
            .insert(key.to_string(), value.trim_matches('"').to_string())
            .is_some()
        {
            return Err(format!("duplicate frontmatter field {key}"));
        }
    }
    Err("missing closing frontmatter ---".to_string())
}

fn permission_rank(level: Permission) -> u8 {
    match level {
        Permission::Allow => 0,
        Permission::Ask => 1,
        Permission::Deny => 2,
    }
}

struct Collector {
    defs: LoadedDefs,
    total_bytes: usize,
}

impl Collector {
    fn note_order(&mut self, kind: &str, id: &str, origin: &str) {
        let prefix = format!("{kind}.{id}@");
        if !self.defs.order.iter().any(|seen| seen.starts_with(&prefix)) {
            self.defs.order.push(format!("{prefix}{origin}"));
        }
    }

    fn put_skill(&mut self, def: SkillDef, origin: &str) {
        if let Some(old) = self.defs.skills.get(&def.id)
            && old.origin != origin
        {
            self.defs
                .shadowed
                .push(format!("skill.{}: {} -> {origin}", def.id, old.origin));
        }
        self.note_order("skill", &def.id, origin);
        self.defs.skills.insert(def.id.clone(), def);
    }

    fn put_agent(&mut self, def: AgentDef, origin: &str) {
        if let Some(old) = self.defs.agents.get(&def.id)
            && old.origin != origin
        {
            self.defs
                .shadowed
                .push(format!("agent.{}: {} -> {origin}", def.id, old.origin));
        }
        self.note_order("agent", &def.id, origin);
        self.defs.agents.insert(def.id.clone(), def);
    }

    fn put_command(&mut self, def: CommandDef, origin: &str) {
        if let Some(old) = self.defs.commands.get(&def.id)
            && old.origin != origin
        {
            self.defs
                .shadowed
                .push(format!("command.{}: {} -> {origin}", def.id, old.origin));
        }
        self.note_order("command", &def.id, origin);
        self.defs.commands.insert(def.id.clone(), def);
    }
}

/// Reserved slash-command ids (with or without the leading slash).
const RESERVED_COMMANDS: &[&str] = &[
    "quit",
    "model",
    "sessions",
    "skills",
    "help",
    "dcp-compress",
];

/// Load definitions from admitted roots in order.
///
/// Later roots replace same-kind/same-id definitions whole; diagnostics
/// never stop sibling loading.
pub fn load_definitions(roots: &[DefRoot]) -> LoadedDefs {
    let mut defs = LoadedDefs::default();
    for root in roots.iter().take(MAX_DEF_ROOTS) {
        merge_definition_root(&mut defs, root);
    }
    if roots.len() > MAX_DEF_ROOTS {
        defs.diagnostics.push(Diagnostic {
            path: String::new(),
            field: "roots".to_string(),
            reason: format!("too many roots (max {MAX_DEF_ROOTS})"),
        });
    }
    defs
}

/// Merge one admitted Markdown definition root at this exact precedence point.
pub fn merge_definition_root(defs: &mut LoadedDefs, root: &DefRoot) {
    let total_bytes = existing_definition_bytes(defs);
    let mut out = Collector {
        defs: std::mem::take(defs),
        total_bytes,
    };
    load_root(&mut out, root);
    *defs = out.defs;
}

fn existing_definition_bytes(defs: &LoadedDefs) -> usize {
    defs.skills
        .values()
        .map(|def| def.body.len())
        .chain(defs.agents.values().map(|def| def.body.len()))
        .chain(defs.commands.values().map(|def| def.body.len()))
        .fold(0usize, usize::saturating_add)
}

fn optional_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| "must be a string".to_string())
        })
        .transpose()
}

fn inline_body(
    object: &serde_json::Map<String, serde_json::Value>,
    first: &str,
    second: &str,
) -> Result<String, String> {
    let first_value = optional_string(object, first)?;
    let second_value = optional_string(object, second)?;
    if first_value.is_some() && second_value.is_some() {
        return Err(format!("{first} and {second} are mutually exclusive"));
    }
    Ok(first_value.or(second_value).unwrap_or_default())
}

fn inline_permissions(
    value: Option<&serde_json::Value>,
) -> Result<BTreeMap<String, Permission>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let object = value
        .as_object()
        .ok_or_else(|| "must be an object".to_string())?;
    let mut permissions = BTreeMap::new();
    for (key, raw) in object {
        if !valid_id(key) {
            return Err(format!("invalid permission key {key}"));
        }
        let level = normalize_permission(key, raw).map_err(|error| error.to_string())?;
        let key = legacy_key(key).to_string();
        permissions
            .entry(key)
            .and_modify(|old| {
                if permission_rank(level) > permission_rank(*old) {
                    *old = level;
                }
            })
            .or_insert(level);
    }
    Ok(permissions)
}

/// Merge authoritative top-level config `agent` and `command` domains.
///
/// Call once per parsed config source in the caller's low-to-high precedence
/// order. Each valid later definition replaces the previous definition whole;
/// an invalid sibling emits a diagnostic and does not disturb valid siblings.
pub fn merge_config_definitions(defs: &mut LoadedDefs, config: &serde_json::Value, source: &str) {
    let source_path = Path::new(source);
    let Some(config) = config.as_object() else {
        defs.diagnostics
            .push(diag(source_path, "config", "must be an object"));
        return;
    };
    let total_bytes = existing_definition_bytes(defs);
    let mut out = Collector {
        defs: std::mem::take(defs),
        total_bytes,
    };
    let root = DefRoot {
        dir: PathBuf::new(),
        origin: source.to_string(),
    };

    if let Some(raw_agents) = config.get("agent") {
        if let Some(agents) = raw_agents.as_object() {
            for (id, raw) in agents {
                if !valid_id(id) {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("agent.{id}"),
                        "invalid id",
                    ));
                    continue;
                }
                let Some(object) = raw.as_object() else {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("agent.{id}"),
                        "must be an object",
                    ));
                    continue;
                };
                let allowed = [
                    "prompt",
                    "body",
                    "description",
                    "model",
                    "variant",
                    "permission",
                    "mode",
                ];
                if let Some(field) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("agent.{id}.{field}"),
                        "unsupported field",
                    ));
                    continue;
                }
                let parsed = (|| {
                    let body = inline_body(object, "prompt", "body")?;
                    let description = optional_string(object, "description")?
                        .or_else(|| {
                            body.lines()
                                .map(str::trim)
                                .find(|line| !line.is_empty())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    let model = optional_string(object, "model")?;
                    let variant = optional_string(object, "variant")?;
                    let mode = primary_mode(optional_string(object, "mode")?)?;
                    let permissions = inline_permissions(object.get("permission"))?;
                    Ok::<_, String>((description, model, variant, body, permissions, mode))
                })();
                match parsed {
                    Ok((description, model, variant, body, permissions, mode)) => insert_agent(
                        &mut out,
                        &root,
                        AgentInput {
                            id: id.clone(),
                            description,
                            model,
                            variant,
                            body,
                            permissions,
                            mode,
                        },
                        source_path,
                    ),
                    Err(reason) => out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("agent.{id}"),
                        &reason,
                    )),
                }
            }
        } else {
            out.defs
                .diagnostics
                .push(diag(source_path, "agent", "must be an object"));
        }
    }

    if let Some(raw_commands) = config.get("command") {
        if let Some(commands) = raw_commands.as_object() {
            for (id, raw) in commands {
                if !valid_id(id) {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("command.{id}"),
                        "invalid id",
                    ));
                    continue;
                }
                let Some(object) = raw.as_object() else {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("command.{id}"),
                        "must be an object",
                    ));
                    continue;
                };
                let allowed = ["template", "body", "description"];
                if let Some(field) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
                    let reason = if matches!(field.as_str(), "agent" | "subtask") {
                        "command execution field unsupported"
                    } else {
                        "unsupported field"
                    };
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("command.{id}.{field}"),
                        reason,
                    ));
                    continue;
                }
                let parsed = (|| {
                    let body = inline_body(object, "template", "body")?;
                    let description = optional_string(object, "description")?
                        .or_else(|| {
                            body.lines()
                                .map(str::trim)
                                .find(|line| !line.is_empty())
                                .map(str::to_string)
                        })
                        .unwrap_or_default();
                    Ok::<_, String>((description, body))
                })();
                match parsed {
                    Ok((description, body)) => {
                        insert_command(&mut out, &root, id.clone(), description, body, source_path)
                    }
                    Err(reason) => out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("command.{id}"),
                        &reason,
                    )),
                }
            }
        } else {
            out.defs
                .diagnostics
                .push(diag(source_path, "command", "must be an object"));
        }
    }
    *defs = out.defs;
}

fn load_root(out: &mut Collector, root: &DefRoot) {
    load_kind(out, root, "skill", &["skill", "skills"]);
    load_kind(out, root, "agent", &["agent", "agents"]);
    load_kind(out, root, "command", &["command", "commands"]);
}

fn load_kind(out: &mut Collector, root: &DefRoot, kind: &str, subs: &[&str]) {
    for sub in subs {
        let dir = root.dir.join(sub);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        for name in names {
            if out.total_bytes > MAX_TOTAL_BYTES {
                out.defs
                    .diagnostics
                    .push(diag(&dir, kind, "total definitions budget exceeded"));
                return;
            }
            load_entry(out, root, kind, &dir, &name);
        }
    }
}

fn insert_skill_text(
    out: &mut Collector,
    root: &DefRoot,
    id: String,
    hint: String,
    text: String,
    path: &Path,
) {
    if out.defs.skills.len() >= MAX_DEFS_PER_KIND {
        out.defs
            .diagnostics
            .push(diag(path, "skill", "too many skills"));
        return;
    }
    if text.len() > MAX_SKILL_FILE {
        out.defs
            .diagnostics
            .push(diag(path, "skill", "skill file too large"));
        return;
    }
    let previous = out.defs.skills.get(&id).map_or(0, |def| def.body.len());
    let next_total = out
        .total_bytes
        .saturating_sub(previous)
        .saturating_add(text.len());
    if next_total > MAX_TOTAL_BYTES {
        out.defs
            .diagnostics
            .push(diag(path, "skill", "total definitions budget exceeded"));
        return;
    }
    match parse_skill(&id, &text) {
        Ok(meta) => {
            out.total_bytes = next_total;
            let description = if meta.description.is_empty() {
                hint
            } else {
                meta.description
            };
            out.put_skill(
                SkillDef {
                    id: id.clone(),
                    name: meta.name,
                    description,
                    body: text,
                    origin: root.origin.clone(),
                },
                &root.origin.clone(),
            );
        }
        Err(e) => out
            .defs
            .diagnostics
            .push(diag(path, "skill", &e.to_string())),
    }
}

struct AgentInput {
    id: String,
    description: String,
    model: Option<String>,
    variant: Option<String>,
    body: String,
    permissions: BTreeMap<String, Permission>,
    mode: Option<String>,
}

fn insert_agent(out: &mut Collector, root: &DefRoot, input: AgentInput, path: &Path) {
    if out.defs.agents.len() >= MAX_DEFS_PER_KIND && !out.defs.agents.contains_key(&input.id) {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "too many agents"));
        return;
    }
    if input.description.len() > 1024 {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "description too large"));
        return;
    }
    if input.body.len() > MAX_DEF_BODY {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "body too large"));
        return;
    }
    let previous = out
        .defs
        .agents
        .get(&input.id)
        .map_or(0, |def| def.body.len());
    let next_total = out
        .total_bytes
        .saturating_sub(previous)
        .saturating_add(input.body.len());
    if next_total > MAX_TOTAL_BYTES {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "total definitions budget exceeded"));
        return;
    }
    out.total_bytes = next_total;
    out.put_agent(
        AgentDef {
            id: input.id.clone(),
            description: input.description,
            model: input.model.filter(|s| !s.is_empty()),
            variant: input.variant.filter(|s| !s.is_empty()),
            body: input.body,
            permissions: input.permissions,
            mode: input.mode,
            origin: root.origin.clone(),
        },
        &root.origin.clone(),
    );
}

fn insert_command(
    out: &mut Collector,
    root: &DefRoot,
    id: String,
    description: String,
    body: String,
    path: &Path,
) {
    if out.defs.commands.len() >= MAX_DEFS_PER_KIND && !out.defs.commands.contains_key(&id) {
        out.defs
            .diagnostics
            .push(diag(path, "command", "too many commands"));
        return;
    }
    let bare = id.trim_start_matches('/');
    if RESERVED_COMMANDS.contains(&bare) {
        out.defs
            .diagnostics
            .push(diag(path, "command", "reserved builtin id"));
        return;
    }
    if body.len() > MAX_DEF_BODY {
        out.defs
            .diagnostics
            .push(diag(path, "command", "body too large"));
        return;
    }
    let previous = out.defs.commands.get(&id).map_or(0, |def| def.body.len());
    let next_total = out
        .total_bytes
        .saturating_sub(previous)
        .saturating_add(body.len());
    if next_total > MAX_TOTAL_BYTES {
        out.defs
            .diagnostics
            .push(diag(path, "command", "total definitions budget exceeded"));
        return;
    }
    out.total_bytes = next_total;
    out.put_command(
        CommandDef {
            id: id.clone(),
            description,
            body,
            origin: root.origin.clone(),
        },
        &root.origin.clone(),
    );
}

fn unsupported_field(fields: &BTreeMap<String, String>, allowed: &[&str]) -> Option<String> {
    fields
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
        .cloned()
}

fn primary_mode(mode: Option<String>) -> Result<Option<String>, String> {
    match mode.as_deref() {
        None | Some("") => Ok(None),
        Some("primary") => Ok(mode),
        Some("subagent" | "all") => Err("subagent mode unsupported".to_string()),
        Some(_) => Err("unknown agent mode".to_string()),
    }
}

fn load_entry(out: &mut Collector, root: &DefRoot, kind: &str, dir: &Path, name: &str) {
    let path = dir.join(name);
    match kind {
        "skill" => {
            let meta = std::fs::symlink_metadata(&path);
            match meta {
                Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
                Ok(_) => {
                    out.defs
                        .diagnostics
                        .push(diag(&path, kind, "flat skill files unsupported"));
                    return;
                }
                Err(_) => {
                    out.defs.diagnostics.push(diag(&path, kind, "unreadable"));
                    return;
                }
            }
            if !valid_id(name) {
                out.defs.diagnostics.push(diag(&path, kind, "invalid id"));
                return;
            }
            let file = path.join("SKILL.md");
            let text = match read_plain(&file, &path, MAX_SKILL_FILE) {
                Ok(text) => text,
                Err(reason) => {
                    out.defs.diagnostics.push(diag(&file, kind, &reason));
                    return;
                }
            };
            insert_skill_text(out, root, name.to_string(), String::new(), text, &file);
        }
        "agent" | "command" => {
            if !(name.ends_with(".md") && name.len() > 3) {
                return; // Ignore foreign files; config domains merge explicitly.
            }
            let id = name.trim_end_matches(".md").to_string();
            if !valid_id(&id) {
                out.defs.diagnostics.push(diag(&path, kind, "invalid id"));
                return;
            }
            let text = match read_plain(&path, dir, MAX_DEF_BODY) {
                Ok(text) => text,
                Err(reason) => {
                    out.defs.diagnostics.push(diag(&path, kind, &reason));
                    return;
                }
            };
            let (frontmatter, body) = match split_frontmatter(&text) {
                Ok(parsed) => parsed,
                Err(reason) => {
                    out.defs
                        .diagnostics
                        .push(diag(&path, "frontmatter", &reason));
                    return;
                }
            };
            if body.len() > MAX_DEF_BODY {
                out.defs
                    .diagnostics
                    .push(diag(&path, kind, "body too large"));
                return;
            }
            if kind == "agent" {
                if let Some(field) = unsupported_field(
                    &frontmatter.fields,
                    &["description", "model", "variant", "mode"],
                ) {
                    out.defs.diagnostics.push(diag(
                        &path,
                        &format!("agent.{field}"),
                        "unsupported field",
                    ));
                    return;
                }
                let mode = match primary_mode(frontmatter.fields.get("mode").cloned()) {
                    Ok(mode) => mode,
                    Err(reason) => {
                        out.defs
                            .diagnostics
                            .push(diag(&path, "agent.mode", &reason));
                        return;
                    }
                };
                let description = frontmatter
                    .fields
                    .get("description")
                    .cloned()
                    .or_else(|| {
                        body.lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let model = frontmatter.fields.get("model").cloned();
                let variant = frontmatter.fields.get("variant").cloned();
                insert_agent(
                    out,
                    root,
                    AgentInput {
                        id,
                        description,
                        model,
                        variant,
                        body: body.to_string(),
                        permissions: frontmatter.permissions,
                        mode,
                    },
                    &path,
                );
            } else {
                if let Some(field) = unsupported_field(&frontmatter.fields, &["description"]) {
                    let reason = if matches!(field.as_str(), "agent" | "subtask") {
                        "command execution field unsupported"
                    } else {
                        "unsupported field"
                    };
                    out.defs
                        .diagnostics
                        .push(diag(&path, &format!("command.{field}"), reason));
                    return;
                }
                if !frontmatter.permissions.is_empty() {
                    out.defs.diagnostics.push(diag(
                        &path,
                        "command.permission",
                        "unsupported field",
                    ));
                    return;
                }
                let description = frontmatter
                    .fields
                    .get("description")
                    .cloned()
                    .or_else(|| {
                        body.lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                insert_command(out, root, id, description, body.to_string(), &path);
            }
        }
        _ => {}
    }
}

/// Select the primary agent: exact id, stable digest for the turn.
///
/// Disappearance or an empty pinned model fails explicitly (no silent
/// fallback); model *presence* in the catalog is validated at turn time.
pub fn select_primary(defs: &LoadedDefs, id: &str) -> Result<String, DefError> {
    let agent = defs
        .agents
        .get(id)
        .ok_or_else(|| DefError::UnknownAgent(id.to_string()))?;
    if let Some(model) = &agent.model
        && model.trim().is_empty()
    {
        return Err(DefError::InvalidAgent {
            id: id.to_string(),
            reason: "empty model".to_string(),
        });
    }
    Ok(agent_digest(agent))
}

/// Stable digest binding the selected profile to the turn.
pub fn agent_digest(agent: &AgentDef) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut hash_bytes = |bytes: &[u8]| {
        for byte in bytes.iter().copied().chain([0]) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for field in [
        agent.id.as_str(),
        agent.description.as_str(),
        agent.model.as_deref().unwrap_or(""),
        agent.variant.as_deref().unwrap_or(""),
        agent.mode.as_deref().unwrap_or(""),
        agent.body.as_str(),
    ] {
        hash_bytes(field.as_bytes());
    }
    for (tool, level) in &agent.permissions {
        hash_bytes(tool.as_bytes());
        hash_bytes(match level {
            Permission::Allow => b"allow",
            Permission::Ask => b"ask",
            Permission::Deny => b"deny",
        });
    }
    let mut out = String::with_capacity(16);
    let _ = write!(out, "{hash:016x}");
    out
}

/// Ordered instructions from admitted `AGENTS.md` files.
///
/// `files` arrive in pinned order (global first, then Location
/// nearest-working-directory-to-root). Each admitted file contributes its
/// body once under a distinct sentinel; repeats canonical-deduplicate;
/// unreadable files give a diagnostic and contribute no stale text.
pub fn load_instructions(files: &[(String, PathBuf)]) -> (String, Vec<Diagnostic>) {
    let mut seen: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut total = 0usize;
    for (display, path) in files {
        if seen.contains(display) {
            continue;
        }
        seen.push(display.clone());
        let text = match std::fs::symlink_metadata(path) {
            Ok(meta) if !meta.file_type().is_symlink() && meta.is_file() => {
                if meta.len() > MAX_INSTRUCTIONS_FILE as u64 {
                    diagnostics.push(Diagnostic {
                        path: display.clone(),
                        field: "instructions".to_string(),
                        reason: "file too large".to_string(),
                    });
                    continue;
                }
                match std::fs::read_to_string(path) {
                    Ok(text) => text,
                    Err(_) => {
                        diagnostics.push(Diagnostic {
                            path: display.clone(),
                            field: "instructions".to_string(),
                            reason: "unreadable".to_string(),
                        });
                        continue;
                    }
                }
            }
            _ => {
                diagnostics.push(Diagnostic {
                    path: display.clone(),
                    field: "instructions".to_string(),
                    reason: "unreadable".to_string(),
                });
                continue;
            }
        };
        if total + text.len() > MAX_INSTRUCTIONS_TOTAL {
            diagnostics.push(Diagnostic {
                path: display.clone(),
                field: "instructions".to_string(),
                reason: "instructions budget exceeded".to_string(),
            });
            continue;
        }
        total += text.len();
        parts.push(format!("<!-- oc-instructions: {display} -->\n{text}"));
    }
    (parts.join("\n"), diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root(dir: &Path, origin: &str) -> DefRoot {
        DefRoot {
            dir: dir.to_path_buf(),
            origin: origin.to_string(),
        }
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }

    #[test]
    fn skills_singular_before_plural_and_valid_siblings_survive() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("skill/a/SKILL.md"),
            "---\nname: a\ndescription: from singular\n---\n# a\n",
        );
        write(
            &opencode.join("skills/b/SKILL.md"),
            "---\nname: b\ndescription: from plural\n---\n# b\n",
        );
        write(&opencode.join("skills/flat.md"), "flat skill file\n");
        write(&opencode.join("skills/broken/SKILL.md"), "no frontmatter\n");
        let loaded = load_definitions(&[root(&opencode, "G")]);
        assert_eq!(loaded.skills.len(), 2);
        assert_eq!(loaded.skills["a"].description, "from singular");
        assert_eq!(loaded.skills["b"].origin, "G");
        assert!(loaded.order.iter().any(|o| o == "skill.a@G"));
        assert!(
            loaded.order.iter().position(|o| o == "skill.a@G")
                < loaded.order.iter().position(|o| o == "skill.b@G")
        );
        assert!(loaded.diagnostics.iter().any(|d| d.reason.contains("flat")));
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.reason.contains("frontmatter") || d.reason.contains("opening"))
        );
    }

    #[test]
    fn later_source_replaces_with_shadow_provenance() {
        let g = tempfile::tempdir().expect("g");
        let p = tempfile::tempdir().expect("p");
        write(
            &g.path().join(".opencode/commands/run.md"),
            "---\ndescription: global run\n---\necho $1\n",
        );
        write(
            &p.path().join(".opencode/commands/run.md"),
            "---\ndescription: local run\n---\necho $1 $2\n",
        );
        let loaded = load_definitions(&[
            root(&g.path().join(".opencode"), "G"),
            root(&p.path().join(".opencode"), "P"),
        ]);
        assert_eq!(loaded.commands["run"].description, "local run");
        assert_eq!(loaded.commands["run"].origin, "P");
        assert_eq!(loaded.shadowed, vec!["command.run: G -> P"]);
    }

    #[test]
    fn unsupported_agent_and_command_modes_fail_explicitly() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("agents/helper.md"),
            "---\ndescription: sub helper\nmode: subagent\n---\nhelp\n",
        );
        write(
            &opencode.join("agents/ok.md"),
            "---\ndescription: ok agent\nmodel: m\nmode: primary\npermission:\n  apply_patch: allow\n  write: deny\n---\nbody\n",
        );
        write(
            &opencode.join("agents/all.md"),
            "---\ndescription: all\nmode: all\n---\nbody\n",
        );
        write(
            &opencode.join("agents/danger.md"),
            "---\ndescription: danger\ntools: true\n---\nbody\n",
        );
        write(&opencode.join("commands/quit.md"), "nope\n");
        write(
            &opencode.join("commands/deploy.md"),
            "Explain `cargo test`, $(literal), and the word subagent.\n```sh\ncargo test\n```\n",
        );
        write(
            &opencode.join("commands/delegate.md"),
            "---\ndescription: no\nsubtask: true\n---\nbody\n",
        );
        write(
            &opencode.join("commands/delegate-agent.md"),
            "---\ndescription: no\nagent: helper\n---\nbody\n",
        );
        write(&opencode.join("commands/ship.md"), "cargo test $1\n");
        let loaded = load_definitions(&[root(&opencode, "P")]);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents["ok"].model.as_deref(), Some("m"));
        assert_eq!(loaded.agents["ok"].mode.as_deref(), Some("primary"));
        assert_eq!(loaded.agents["ok"].body, "body\n");
        assert_eq!(
            loaded.agents["ok"].permissions["apply_patch"],
            Permission::Deny
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.reason.contains("subagent"))
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.reason.contains("reserved"))
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field.contains("tools") && d.reason.contains("unsupported"))
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field.contains("subtask"))
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field == "command.agent")
        );
        assert!(loaded.commands["deploy"].body.contains("`cargo test`"));
        assert!(loaded.commands["deploy"].body.contains("subagent"));
        assert!(loaded.commands.contains_key("ship"));
        let digest = select_primary(&loaded, "ok").expect("select");
        assert_eq!(digest, agent_digest(&loaded.agents["ok"]));
        assert_eq!(
            select_primary(&loaded, "gone"),
            Err(DefError::UnknownAgent("gone".to_string()))
        );
    }

    #[test]
    fn symlinks_are_refused() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &base.path().join("real.md"),
            "---\ndescription: x\n---\nbody\n",
        );
        fs::create_dir_all(opencode.join("commands")).expect("mkdir");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            base.path().join("real.md"),
            opencode.join("commands/link.md"),
        )
        .expect("symlink");
        let loaded = load_definitions(&[root(&opencode, "P")]);
        assert!(loaded.commands.is_empty());
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.reason.contains("symlink"))
        );
    }

    #[test]
    fn config_domains_merge_in_caller_order_without_invented_files() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("commands.json"),
            r#"{"run": {"description": "inline", "body": "echo inline"}}"#,
        );
        let mut loaded = load_definitions(&[root(&opencode, "P")]);
        assert!(loaded.commands.is_empty());

        merge_config_definitions(
            &mut loaded,
            &serde_json::json!({
                "agent": {
                    "review": {
                        "description": "same",
                        "prompt": "global prompt",
                        "model": "old",
                        "permission": {"write": "allow"},
                        "mode": "primary"
                    },
                    "bad": {"prompt": "bad", "hooks": {"x": true}}
                },
                "command": {
                    "run": {"description": "global", "template": "Explain `cargo test` to $1"},
                    "bad": {"template": "delegate", "agent": "review"}
                }
            }),
            "G/opencode.json",
        );
        merge_config_definitions(
            &mut loaded,
            &serde_json::json!({
                "agent": {
                    "review": {
                        "description": "same",
                        "body": "local prompt",
                        "variant": "high",
                        "permission": {"edit": "deny"}
                    }
                },
                "command": {
                    "run": {"body": "literal subagent in a code fence:\n```sh\ncargo test\n```"},
                    "ok": {"template": "valid sibling"}
                }
            }),
            "P/opencode.jsonc",
        );
        let agent = &loaded.agents["review"];
        assert_eq!(agent.origin, "P/opencode.jsonc");
        assert_eq!(agent.body, "local prompt");
        assert_eq!(agent.model, None, "later definitions replace whole");
        assert_eq!(agent.variant.as_deref(), Some("high"));
        assert_eq!(agent.permissions["apply_patch"], Permission::Deny);
        assert_eq!(loaded.commands["run"].origin, "P/opencode.jsonc");
        assert!(loaded.commands["run"].body.contains("```sh"));
        assert!(loaded.commands.contains_key("ok"));
        assert!(!loaded.agents.contains_key("bad"));
        assert!(!loaded.commands.contains_key("bad"));
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field.ends_with("hooks"))
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field.ends_with("agent"))
        );
        assert_eq!(
            loaded.shadowed,
            vec![
                "agent.review: G/opencode.json -> P/opencode.jsonc",
                "command.run: G/opencode.json -> P/opencode.jsonc"
            ]
        );
    }

    #[test]
    fn agent_digest_covers_body_and_permissions() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("agents/review.md"),
            "---\ndescription: review\npermission:\n  apply_patch: deny\n---\npolicy A\n",
        );
        let loaded = load_definitions(&[root(&opencode, "P")]);
        let agent = &loaded.agents["review"];
        let original = agent_digest(agent);
        let mut changed_body = agent.clone();
        changed_body.body = "policy B\n".to_string();
        assert_ne!(original, agent_digest(&changed_body));
        let mut changed_permission = agent.clone();
        changed_permission
            .permissions
            .insert("apply_patch".to_string(), Permission::Ask);
        assert_ne!(original, agent_digest(&changed_permission));
    }

    #[test]
    fn instructions_dedup_sentinel_and_diagnose() {
        let base = tempfile::tempdir().expect("tmp");
        let g = base.path().join("G-AGENTS.md");
        let p = base.path().join("P-AGENTS.md");
        fs::write(&g, "global rules\n").expect("g");
        fs::write(&p, "local rules\n").expect("p");
        let missing = base.path().join("gone.md");
        let (text, diagnostics) = load_instructions(&[
            ("G/AGENTS.md".to_string(), g),
            ("P/AGENTS.md".to_string(), p.clone()),
            ("P/AGENTS.md".to_string(), p),
            ("X/AGENTS.md".to_string(), missing),
        ]);
        assert_eq!(text.matches("oc-instructions: P/AGENTS.md").count(), 1);
        assert!(text.contains("global rules"));
        assert!(text.contains("local rules"));
        assert!(text.find("global rules") < text.find("local rules"));
        assert!(diagnostics.iter().any(|d| d.reason == "unreadable"));
    }
}
