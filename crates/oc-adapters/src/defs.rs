//! Workspace definitions and instructions loading (T25, CFG06/CFG07).
//!
//! Disk discovery for admitted `.opencode` roots only: skills
//! (`{skill,skills}/<id>/SKILL.md` and flat `{skill,skills}/<id>.md`),
//! agents (`{agent,agents}/<id>.md`), commands (`{command,commands}/<id>.md`).
//! Singular roots load before plural within one source directory; later
//! sources replace duplicates with shadowing provenance. Authoritative inline
//! definitions come from top-level config `agent`/`command` domains through
//! [`merge_config_definitions`], never invented sibling files. Order never
//! depends on filesystem enumeration (entries are sorted). Invalid entries
//! produce path/field/reason diagnostics while valid siblings survive; a
//! skill directory without `SKILL.md` is silently skipped (upstream parity).
//! Skill bodies stay out of the model projection (served bounded by the
//! native `skill` tool from the pinned snapshot). Nothing here executes.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{self, Read as _};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use crate::admitted_fs;
use crate::config::{
    Permission, legacy_key, normalize_permission, parse_skill, split_frontmatter_value,
};
use thiserror::Error;

/// Max admitted definition roots per load.
pub const MAX_DEF_ROOTS: usize = 8;
/// Max definition id length.
pub const MAX_DEF_ID_LEN: usize = 128;
/// Max total definition bytes per load.
///
/// Deliberate deviation from upstream: opencode v2.0.12 imposes no size limit
/// on agent/command/skill files, bodies, frontmatter or definition counts.
/// This is the single generous resource bound; per-file, per-body and
/// frontmatter caps are intentionally absent.
pub const MAX_TOTAL_BYTES: usize = 4 * 1024 * 1024;
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
    /// Directory id (or flat file stem).
    pub id: String,
    /// Name (`id` when frontmatter omits it).
    pub name: String,
    /// Description (empty when frontmatter omits it).
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
    /// Description.
    pub description: String,
    /// Optional pinned model id (validated at turn time, not here).
    pub model: Option<String>,
    /// Optional pinned variant.
    pub variant: Option<String>,
    /// Literal primary-agent prompt/body.
    pub body: String,
    /// Agent-specific permission narrowing, normalized to runtime tool names.
    pub permissions: BTreeMap<String, Permission>,
    /// Ordered resource-aware narrowing, retained independently of the summary.
    pub permission_rules: crate::permissions::PermissionRules,
    /// Omitted from automatic subagent discovery, still explicitly addressable.
    pub hidden: bool,
    /// Admitted mode (`primary`) when explicitly configured.
    pub mode: Option<String>,
    /// Winning source origin.
    pub origin: String,
}

impl AgentDef {
    /// Whether this profile may be selected as a primary agent.
    ///
    /// An absent mode is `all` (upstream default); only `subagent` is
    /// primary-ineligible.
    pub fn primary_capable(&self) -> bool {
        self.mode.as_deref() != Some("subagent")
    }

    /// Whether this profile may be spawned as a subagent.
    ///
    /// An absent mode is `all` (upstream default); only `primary` is
    /// subagent-ineligible.
    pub fn subagent_capable(&self) -> bool {
        self.mode.as_deref() != Some("primary")
    }
}

/// Non-executable command (literal expansion only, via `expand_command`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandDef {
    /// File id.
    pub id: String,
    /// Description.
    pub description: String,
    /// Literal body.
    pub body: String,
    /// Optional agent this command delegates to (execution is a later slice).
    pub agent: Option<String>,
    /// Optional pinned model, string or object (execution is a later slice).
    pub model: Option<serde_json::Value>,
    /// Explicit subagent flag.
    pub subagent: Option<bool>,
    /// Deprecated `subagent` alias.
    pub subtask: Option<bool>,
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

/// Read only an already-opened, admitted regular file. The same 4 MiB total
/// policy also bounds each transient read, even if the file grows after fstat.
fn read_plain(mut file: File) -> Result<String, String> {
    let meta = file.metadata().map_err(|_| "unreadable".to_string())?;
    if !meta.is_file() {
        return Err("not a file".to_string());
    }
    if meta.len() > MAX_TOTAL_BYTES as u64 {
        return Err("total definitions budget exceeded".to_string());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_TOTAL_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable".to_string())?;
    if bytes.len() > MAX_TOTAL_BYTES {
        return Err("total definitions budget exceeded".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())
}

/// Parsed Markdown frontmatter (scalar and nested values).
#[derive(Default)]
struct Frontmatter {
    fields: BTreeMap<String, serde_json::Value>,
}

/// Split and parse the admitted Markdown YAML subset (comments included).
fn split_frontmatter(text: &str) -> Result<(Frontmatter, &str), String> {
    let (value, body) = split_frontmatter_value(text)?;
    let object = value
        .as_object()
        .ok_or_else(|| "unsupported frontmatter structure".to_string())?;
    let mut parsed = Frontmatter::default();
    for (key, value) in object {
        parsed.fields.insert(key.clone(), value.clone());
    }
    Ok((parsed, body))
}

fn field_string(
    fields: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match fields.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn field_bool(
    fields: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<bool>, String> {
    match fields.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

fn field_model(
    fields: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    match fields.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(value) if value.is_string() || value.is_object() => Ok(Some(value.clone())),
        Some(_) => Err(format!("{key} must be a string or object")),
    }
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
    let canonical = match root.dir.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return,
        Err(_) => {
            defs.diagnostics
                .push(diag(&root.dir, "definitions", "unreadable root"));
            return;
        }
    };
    let dir = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(&canonical)
    {
        Ok(dir) => dir,
        Err(_) => {
            defs.diagnostics
                .push(diag(&root.dir, "definitions", "unreadable root"));
            return;
        }
    };
    if !std::fs::canonicalize(format!("/proc/self/fd/{}", dir.as_raw_fd()))
        .is_ok_and(|opened| opened == canonical)
    {
        defs.diagnostics.push(diag(
            &root.dir,
            "definitions",
            "root changed during admission",
        ));
        return;
    }
    merge_definition_root_admitted(
        defs,
        &DefRoot {
            dir: canonical,
            origin: root.origin.clone(),
        },
        &dir,
    );
}

/// Merge a root previously admitted by composition, using its pinned fd.
pub(crate) fn merge_definition_root_admitted(defs: &mut LoadedDefs, root: &DefRoot, dir: &File) {
    let total_bytes = existing_definition_bytes(defs);
    let mut out = Collector {
        defs: std::mem::take(defs),
        total_bytes,
    };
    load_root(&mut out, root, dir);
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

fn optional_bool(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<bool>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean"))
        })
        .transpose()
}

fn optional_model(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<serde_json::Value>, String> {
    object
        .get(key)
        .map(|value| {
            if value.is_string() || value.is_object() {
                Ok(value.clone())
            } else {
                Err(format!("{key} must be a string or object"))
            }
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

/// Upstream legacy permissions are `Record(String, Rule)`: any action name
/// (including custom/glob keys such as `tavily-local_*`) is accepted, so the
/// key only has to be a YAML-safe scalar.
fn valid_permission_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_DEF_ID_LEN
        && !key.chars().any(|c| c.is_whitespace() || c == ':')
}

/// Normalize a permission mapping (scalar actions or glob→action maps).
fn permission_map(
    value: Option<&serde_json::Value>,
) -> Result<BTreeMap<String, Permission>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    if value.is_null() {
        return Ok(BTreeMap::new());
    }
    if value.is_string() {
        return Ok(BTreeMap::from([(
            "*".into(),
            normalize_permission("*", value).map_err(|error| error.to_string())?,
        )]));
    }
    let object = value
        .as_object()
        .ok_or_else(|| "must be an object".to_string())?;
    let mut permissions = BTreeMap::new();
    for (key, raw) in object {
        if !valid_permission_key(key) {
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

fn metadata_bool(value: Option<&serde_json::Value>) -> Result<bool, String> {
    match value {
        None => Ok(false),
        Some(value) => value.as_bool().ok_or_else(|| "must be boolean".into()),
    }
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
                    "permissions",
                    "tools",
                    "hidden",
                    "disable",
                    "disabled",
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
                    let mode = agent_mode(optional_string(object, "mode")?)?;
                    let permissions = permission_map(object.get("permission"))?;
                    let permission_rules = crate::permissions::PermissionRules::from_config(raw)
                        .map_err(|error| error.to_string())?;
                    let hidden = metadata_bool(raw.get("hidden"))?;
                    let disabled =
                        metadata_bool(raw.get("disable"))? | metadata_bool(raw.get("disabled"))?;
                    Ok::<_, String>((
                        description,
                        model,
                        variant,
                        body,
                        permissions,
                        mode,
                        permission_rules,
                        hidden,
                        disabled,
                    ))
                })();
                match parsed {
                    Ok((
                        description,
                        model,
                        variant,
                        body,
                        permissions,
                        mode,
                        permission_rules,
                        hidden,
                        disabled,
                    )) => insert_agent(
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
                            permission_rules,
                            hidden,
                            disabled,
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
                let allowed = [
                    "template",
                    "body",
                    "description",
                    "agent",
                    "model",
                    "subagent",
                    "subtask",
                ];
                if let Some(field) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
                    out.defs.diagnostics.push(diag(
                        source_path,
                        &format!("command.{id}.{field}"),
                        "unsupported field",
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
                    Ok::<_, String>(CommandInput {
                        id: id.clone(),
                        description,
                        body,
                        agent: optional_string(object, "agent")?,
                        model: optional_model(object, "model")?,
                        subagent: optional_bool(object, "subagent")?,
                        subtask: optional_bool(object, "subtask")?,
                    })
                })();
                match parsed {
                    Ok(input) => insert_command(&mut out, &root, input, source_path),
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

fn open_admitted(root: &DefRoot, dir: &File, candidate: &Path, flags: i32) -> io::Result<File> {
    // Canonicalize only to choose an in-root target. Never read or enumerate
    // it by pathname: the descriptor-relative open is the trust decision.
    let canonical = candidate.canonicalize()?;
    let relative = canonical.strip_prefix(&root.dir).map_err(|_| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "symlink resolves outside admitted root",
        )
    })?;
    admitted_fs::open_beneath(dir, relative, flags)
}

fn load_root(out: &mut Collector, root: &DefRoot, dir: &File) {
    load_kind(out, root, dir, "skill", &["skill", "skills"]);
    load_kind(out, root, dir, "agent", &["agent", "agents"]);
    load_kind(out, root, dir, "command", &["command", "commands"]);
}

fn load_kind(out: &mut Collector, root: &DefRoot, admitted: &File, kind: &str, subs: &[&str]) {
    for sub in subs {
        let dir = root.dir.join(sub);
        let pinned = match open_admitted(root, admitted, &dir, libc::O_RDONLY | libc::O_DIRECTORY) {
            Ok(pinned) => pinned,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                out.defs
                    .diagnostics
                    .push(diag(&dir, kind, &error.to_string()));
                continue;
            }
        };
        // read_dir reopens only this pinned descriptor, never the pathname that
        // may be concurrently replaced with an external symlink.
        let entries = match std::fs::read_dir(format!("/proc/self/fd/{}", pinned.as_raw_fd())) {
            Ok(entries) => entries,
            Err(_) => {
                out.defs
                    .diagnostics
                    .push(diag(&dir, kind, "unreadable directory"));
                continue;
            }
        };
        // 256 definitions per kind are admitted; bound directory enumeration
        // as well, before sorting names or opening any definition bodies.
        let names: Vec<String> = entries
            .take(4097)
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        if names.len() > 4096 {
            out.defs
                .diagnostics
                .push(diag(&dir, kind, "too many directory entries"));
            continue;
        }
        let mut names = names;
        names.sort();
        for name in names {
            if out.total_bytes > MAX_TOTAL_BYTES {
                out.defs
                    .diagnostics
                    .push(diag(&dir, kind, "total definitions budget exceeded"));
                return;
            }
            load_entry(out, root, admitted, kind, &dir, &name);
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
    permission_rules: crate::permissions::PermissionRules,
    hidden: bool,
    disabled: bool,
    mode: Option<String>,
}

fn insert_agent(out: &mut Collector, root: &DefRoot, input: AgentInput, path: &Path) {
    if input.disabled {
        if let Some(previous) = out.defs.agents.remove(&input.id) {
            out.total_bytes = out.total_bytes.saturating_sub(previous.body.len());
        }
        return;
    }
    if out.defs.agents.len() >= MAX_DEFS_PER_KIND && !out.defs.agents.contains_key(&input.id) {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "too many agents"));
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
            permission_rules: input.permission_rules,
            hidden: input.hidden,
            mode: input.mode,
            origin: root.origin.clone(),
        },
        &root.origin.clone(),
    );
}

struct CommandInput {
    id: String,
    description: String,
    body: String,
    agent: Option<String>,
    model: Option<serde_json::Value>,
    subagent: Option<bool>,
    subtask: Option<bool>,
}

fn insert_command(out: &mut Collector, root: &DefRoot, input: CommandInput, path: &Path) {
    if out.defs.commands.len() >= MAX_DEFS_PER_KIND && !out.defs.commands.contains_key(&input.id) {
        out.defs
            .diagnostics
            .push(diag(path, "command", "too many commands"));
        return;
    }
    let bare = input.id.trim_start_matches('/');
    if RESERVED_COMMANDS.contains(&bare) {
        out.defs
            .diagnostics
            .push(diag(path, "command", "reserved builtin id"));
        return;
    }
    let previous = out
        .defs
        .commands
        .get(&input.id)
        .map_or(0, |def| def.body.len());
    let next_total = out
        .total_bytes
        .saturating_sub(previous)
        .saturating_add(input.body.len());
    if next_total > MAX_TOTAL_BYTES {
        out.defs
            .diagnostics
            .push(diag(path, "command", "total definitions budget exceeded"));
        return;
    }
    out.total_bytes = next_total;
    out.put_command(
        CommandDef {
            id: input.id.clone(),
            description: input.description,
            body: input.body,
            agent: input.agent.filter(|s| !s.is_empty()),
            model: input.model,
            subagent: input.subagent,
            subtask: input.subtask,
            origin: root.origin.clone(),
        },
        &root.origin.clone(),
    );
}

fn unsupported_field(
    fields: &BTreeMap<String, serde_json::Value>,
    allowed: &[&str],
) -> Option<String> {
    fields
        .keys()
        .find(|key| !allowed.contains(&key.as_str()))
        .cloned()
}

fn agent_mode(mode: Option<String>) -> Result<Option<String>, String> {
    match mode.as_deref() {
        None | Some("") => Ok(None),
        Some("primary" | "subagent" | "all") => Ok(mode),
        Some(_) => Err("unknown agent mode".to_string()),
    }
}

fn load_entry(
    out: &mut Collector,
    root: &DefRoot,
    admitted: &File,
    kind: &str,
    dir: &Path,
    name: &str,
) {
    let path = dir.join(name);
    match kind {
        "skill" => {
            let entry = match open_admitted(root, admitted, &path, libc::O_RDONLY) {
                Ok(entry) => entry,
                Err(error) => {
                    out.defs
                        .diagnostics
                        .push(diag(&path, kind, &error.to_string()));
                    return;
                }
            };
            let meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(_) => {
                    out.defs.diagnostics.push(diag(&path, kind, "unreadable"));
                    return;
                }
            };
            if meta.is_file() {
                // Flat `skills/<id>.md` is a skill (upstream scans `*.md`).
                if !name.ends_with(".md") {
                    return;
                }
                let id = name.trim_end_matches(".md").to_string();
                if !valid_id(&id) {
                    out.defs.diagnostics.push(diag(&path, kind, "invalid id"));
                    return;
                }
                let text = match read_plain(entry) {
                    Ok(text) => text,
                    Err(reason) => {
                        out.defs.diagnostics.push(diag(&path, kind, &reason));
                        return;
                    }
                };
                insert_skill_text(out, root, id, String::new(), text, &path);
                return;
            }
            if !meta.is_dir() {
                return; // Foreign entry; upstream ignores anything else.
            }
            if !valid_id(name) {
                out.defs.diagnostics.push(diag(&path, kind, "invalid id"));
                return;
            }
            let file = path.join("SKILL.md");
            let admitted_file = match open_admitted(root, admitted, &file, libc::O_RDONLY) {
                // A directory without `SKILL.md` is silently skipped
                // (upstream scans `**/SKILL.md`, not every file).
                Err(error) if error.kind() == io::ErrorKind::NotFound => return,
                Err(error) => {
                    out.defs
                        .diagnostics
                        .push(diag(&file, kind, &error.to_string()));
                    return;
                }
                Ok(file) => file,
            };
            let text = match read_plain(admitted_file) {
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
            let admitted_file = match open_admitted(root, admitted, &path, libc::O_RDONLY) {
                Ok(file) => file,
                Err(error) => {
                    out.defs
                        .diagnostics
                        .push(diag(&path, kind, &error.to_string()));
                    return;
                }
            };
            let text = match read_plain(admitted_file) {
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
            if kind == "agent" {
                if let Some(field) = unsupported_field(
                    &frontmatter.fields,
                    &[
                        "description",
                        "model",
                        "variant",
                        "mode",
                        "permission",
                        "permissions",
                        "tools",
                        "hidden",
                        "disable",
                        "disabled",
                    ],
                ) {
                    out.defs.diagnostics.push(diag(
                        &path,
                        &format!("agent.{field}"),
                        "unsupported field",
                    ));
                    return;
                }
                let parsed = (|| -> Result<AgentInput, String> {
                    Ok(AgentInput {
                        id: id.clone(),
                        description: field_string(&frontmatter.fields, "description")?
                            .or_else(|| {
                                body.lines()
                                    .map(str::trim)
                                    .find(|line| !line.is_empty())
                                    .map(str::to_string)
                            })
                            .unwrap_or_default(),
                        model: field_string(&frontmatter.fields, "model")?,
                        variant: field_string(&frontmatter.fields, "variant")?,
                        body: body.to_string(),
                        permissions: permission_map(frontmatter.fields.get("permission"))?,
                        permission_rules: crate::permissions::PermissionRules::from_config(
                            &serde_json::Value::Object(
                                frontmatter.fields.clone().into_iter().collect(),
                            ),
                        )
                        .map_err(|error| error.to_string())?,
                        hidden: metadata_bool(frontmatter.fields.get("hidden"))?,
                        disabled: metadata_bool(frontmatter.fields.get("disable"))?
                            | metadata_bool(frontmatter.fields.get("disabled"))?,
                        mode: agent_mode(field_string(&frontmatter.fields, "mode")?)?,
                    })
                })();
                match parsed {
                    Ok(input) => insert_agent(out, root, input, &path),
                    Err(reason) => out.defs.diagnostics.push(diag(&path, "agent", &reason)),
                }
            } else {
                if let Some(field) = unsupported_field(
                    &frontmatter.fields,
                    &["description", "agent", "model", "subagent", "subtask"],
                ) {
                    out.defs.diagnostics.push(diag(
                        &path,
                        &format!("command.{field}"),
                        "unsupported field",
                    ));
                    return;
                }
                match frontmatter.fields.get("permission") {
                    None | Some(serde_json::Value::Null) => {}
                    Some(serde_json::Value::Object(map)) if map.is_empty() => {}
                    Some(_) => {
                        out.defs.diagnostics.push(diag(
                            &path,
                            "command.permission",
                            "unsupported field",
                        ));
                        return;
                    }
                }
                let parsed = (|| -> Result<CommandInput, String> {
                    Ok(CommandInput {
                        id: id.clone(),
                        description: field_string(&frontmatter.fields, "description")?
                            .or_else(|| {
                                body.lines()
                                    .map(str::trim)
                                    .find(|line| !line.is_empty())
                                    .map(str::to_string)
                            })
                            .unwrap_or_default(),
                        body: body.to_string(),
                        agent: field_string(&frontmatter.fields, "agent")?,
                        model: field_model(&frontmatter.fields, "model")?,
                        subagent: field_bool(&frontmatter.fields, "subagent")?,
                        subtask: field_bool(&frontmatter.fields, "subtask")?,
                    })
                })();
                match parsed {
                    Ok(input) => insert_command(out, root, input, &path),
                    Err(reason) => out.defs.diagnostics.push(diag(&path, "command", &reason)),
                }
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
    hash_bytes(
        &serde_json::to_vec(&agent.permission_rules).expect("serializable permission rules"),
    );
    hash_bytes(if agent.hidden { b"hidden" } else { b"visible" });
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
    let mut seen = std::collections::HashSet::new();
    let texts: Vec<_> = files
        .iter()
        .filter(|(display, _)| seen.insert(display.clone()))
        .map(|(display, path)| {
            let text = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)
                .map_err(|_| "unreadable".to_string())
                .and_then(|mut file| {
                    let meta = file.metadata().map_err(|_| "unreadable".to_string())?;
                    if !meta.is_file() {
                        return Err("unreadable".to_string());
                    }
                    if meta.len() > MAX_INSTRUCTIONS_FILE as u64 {
                        return Err("file too large".to_string());
                    }
                    let mut bytes = Vec::new();
                    file.by_ref()
                        .take(MAX_INSTRUCTIONS_FILE as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|_| "unreadable".to_string())?;
                    if bytes.len() > MAX_INSTRUCTIONS_FILE {
                        return Err("file too large".to_string());
                    }
                    String::from_utf8(bytes).map_err(|_| "unreadable".to_string())
                });
            (display.clone(), text)
        })
        .collect();
    load_instruction_texts(&texts)
}

/// Assemble already-admitted instruction bytes; no path re-open after trust.
pub(crate) fn load_instruction_texts(
    files: &[(String, Result<String, String>)],
) -> (String, Vec<Diagnostic>) {
    let mut seen: Vec<String> = Vec::new();
    let mut parts: Vec<String> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut total = 0usize;
    for (display, result) in files {
        if seen.contains(display) {
            continue;
        }
        seen.push(display.clone());
        let text = match result {
            Ok(text) => text,
            Err(reason) => {
                diagnostics.push(Diagnostic {
                    path: display.clone(),
                    field: "instructions".to_string(),
                    reason: reason.clone(),
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
        // Upstream parity: flat `*.md` is a skill; a directory without
        // `SKILL.md` is silently skipped; a file without frontmatter loads.
        write(&opencode.join("skills/flat.md"), "flat skill file\n");
        write(
            &opencode.join("skills/no-skill-md/notes.md"),
            "not a skill\n",
        );
        write(&opencode.join("skills/bare/SKILL.md"), "no frontmatter\n");
        let loaded = load_definitions(&[root(&opencode, "G")]);
        assert_eq!(loaded.skills.len(), 4);
        assert_eq!(loaded.skills["a"].description, "from singular");
        assert_eq!(loaded.skills["b"].origin, "G");
        assert_eq!(loaded.skills["flat"].name, "flat");
        assert_eq!(loaded.skills["flat"].description, "");
        assert_eq!(loaded.skills["bare"].name, "bare");
        assert!(loaded.order.iter().any(|o| o == "skill.a@G"));
        assert!(
            loaded.order.iter().position(|o| o == "skill.a@G")
                < loaded.order.iter().position(|o| o == "skill.b@G")
        );
        assert!(
            loaded.diagnostics.is_empty(),
            "owner-parity shapes produce no diagnostics: {:?}",
            loaded.diagnostics
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
    fn agent_modes_and_command_execution_fields_load() {
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
            &opencode.join("agents/broken-mode.md"),
            "---\ndescription: bad mode\nmode: banana\n---\nbody\n",
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
            "---\ndescription: no\nagent: helper\nmodel: arbuz/main\nsubagent: false\n---\nbody\n",
        );
        write(&opencode.join("commands/ship.md"), "cargo test $1\n");
        let loaded = load_definitions(&[root(&opencode, "P")]);
        // Upstream accepts `primary|subagent|all`; only unknown modes fail.
        assert_eq!(loaded.agents.len(), 3);
        assert_eq!(loaded.agents["helper"].mode.as_deref(), Some("subagent"));
        assert_eq!(loaded.agents["all"].mode.as_deref(), Some("all"));
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
                .any(|d| d.reason.contains("unknown agent mode"))
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
                .any(|d| d.path.ends_with("danger.md") && d.reason.contains("invalid config tools"))
        );
        // Execution fields are parsed and stored; the commands stay loadable.
        assert_eq!(loaded.commands["delegate"].subtask, Some(true));
        let delegated = &loaded.commands["delegate-agent"];
        assert_eq!(delegated.agent.as_deref(), Some("helper"));
        assert_eq!(delegated.model, Some(serde_json::json!("arbuz/main")));
        assert_eq!(delegated.subagent, Some(false));
        assert_eq!(delegated.subtask, None);
        assert!(
            !loaded
                .diagnostics
                .iter()
                .any(|d| d.field.starts_with("command.")),
            "command execution fields never diagnose: {:?}",
            loaded.diagnostics
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
    fn commented_frontmatter_and_glob_permissions_follow_upstream() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("agents/explore.md"),
            "---\ndescription: explore\n#model: arbuz/commented\n# model: arbuz/commented\nmodel: arbuz/main\nmode: subagent\n---\nbody\n",
        );
        write(
            &opencode.join("agents/build.md"),
            "---\ndescription: build\npermission:\n  external_directory:\n    \"*\": ask\n    \"~/.cargo/**\": allow\n  edit: allow\n  bash:\n    \"*\": ask\n    \"git status\": allow\n  webfetch: allow\n  some_future_action: allow\n  tavily-local_*: allow\n  read:\n    \"*\": allow\n    \"~/.ssh/*\": deny\n---\nbody\n",
        );
        write(
            &opencode.join("agents/bad-permission.md"),
            "---\ndescription: bad\npermission:\n  bash:\n    \"*\":\n      nested: allow\n---\nbody\n",
        );
        let loaded = load_definitions(&[root(&opencode, "P")]);
        let explore = &loaded.agents["explore"];
        assert_eq!(explore.model.as_deref(), Some("arbuz/main"));
        assert_eq!(explore.mode.as_deref(), Some("subagent"));
        let build = &loaded.agents["build"];
        assert_eq!(
            build.permissions["external_directory"],
            Permission::Ask,
            "glob maps fold to the most restrictive level"
        );
        assert_eq!(build.permissions["apply_patch"], Permission::Allow);
        assert_eq!(build.permissions["bash"], Permission::Ask);
        assert_eq!(build.permissions["webfetch"], Permission::Allow);
        assert_eq!(build.permissions["some_future_action"], Permission::Allow);
        assert_eq!(
            build.permissions["tavily-local_*"],
            Permission::Allow,
            "custom/glob permission keys are accepted like upstream"
        );
        assert_eq!(build.permissions["read"], Permission::Deny);
        assert!(
            !loaded
                .diagnostics
                .iter()
                .any(|d| d.path.ends_with("explore.md") || d.path.ends_with("build.md")),
            "owner shapes produce no diagnostics: {:?}",
            loaded.diagnostics
        );
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.path.ends_with("bad-permission.md")),
            "genuinely unsupported nesting stays a diagnostic"
        );
    }

    #[test]
    fn large_agent_and_command_bodies_load_within_the_global_budget() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("agents/build-work.md"),
            &format!(
                "---\ndescription: big agent\n---\n{}\n",
                "a".repeat(30 * 1024)
            ),
        );
        write(
            &opencode.join("commands/mge.md"),
            &format!(
                "---\ndescription: big command\nagent: build\n---\n{}\n",
                "c".repeat(41 * 1024)
            ),
        );
        let loaded = load_definitions(&[root(&opencode, "P")]);
        assert_eq!(loaded.agents["build-work"].body.len(), 30 * 1024 + 1);
        assert_eq!(loaded.commands["mge"].body.len(), 41 * 1024 + 1);
        assert_eq!(loaded.commands["mge"].agent.as_deref(), Some("build"));
        assert!(
            loaded.diagnostics.is_empty(),
            "no artificial size limits: {:?}",
            loaded.diagnostics
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
    fn v07a_racing_definition_subdirectory_never_reads_outside() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let base = tempfile::tempdir().expect("fixture");
        let local = base.path().join("project/.opencode");
        let inside = local.join("inside-skills");
        let outside = base.path().join("outside-skills");
        write(
            &inside.join("safe.md"),
            "---\ndescription: safe\n---\nsafe\n",
        );
        write(
            &outside.join("outside.md"),
            "---\ndescription: forbidden\n---\nforbidden\n",
        );
        let link = local.join("skills");
        std::os::unix::fs::symlink(&inside, &link).expect("initial link");
        let stop = AtomicBool::new(false);
        let mut escaped = false;
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut external = true;
                while !stop.load(Ordering::Relaxed) {
                    let replacement = local.join("next-skills");
                    std::os::unix::fs::symlink(
                        if external { &outside } else { &inside },
                        &replacement,
                    )
                    .expect("replacement");
                    fs::rename(&replacement, &link).expect("atomic swap");
                    external = !external;
                    std::thread::yield_now();
                }
            });
            for _ in 0..80 {
                let loaded = load_definitions(&[root(&local, "P")]);
                escaped |= loaded.skills.contains_key("outside");
                std::thread::yield_now();
            }
            stop.store(true, Ordering::Relaxed);
            worker.join().expect("swapper");
        });
        assert!(
            !escaped,
            "external definition was loaded during a symlink race"
        );
    }

    #[test]
    fn v07a_definition_fifo_and_oversize_do_not_block_or_exhaust_budget() {
        let base = tempfile::tempdir().expect("fixture");
        let dir = base.path().join(".opencode/skills");
        write(
            &dir.join("safe.md"),
            "---\ndescription: safe\n---\nvalid body",
        );
        write(&dir.join("huge.md"), &"x".repeat(MAX_TOTAL_BYTES + 1));
        let fifo = dir.join("pipe.md");
        let fifo_name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).expect("path");
        // SAFETY: NUL-terminated path inside the isolated test directory.
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        let loaded = load_definitions(&[root(&base.path().join(".opencode"), "P")]);
        assert!(loaded.skills.contains_key("safe"));
        assert!(!loaded.skills.contains_key("huge") && !loaded.skills.contains_key("pipe"));
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.path.ends_with("huge.md") && d.reason.contains("budget"))
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
                    "bad": {"template": "delegate", "hooks": {"x": true}}
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
                    "delegate": {
                        "template": "delegate now",
                        "agent": "review",
                        "model": {"providerID": "p", "modelID": "m"},
                        "subtask": false
                    },
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
        // Inline execution fields are stored, never rejected.
        let delegate = &loaded.commands["delegate"];
        assert_eq!(delegate.agent.as_deref(), Some("review"));
        assert_eq!(
            delegate.model,
            Some(serde_json::json!({"providerID": "p", "modelID": "m"}))
        );
        assert_eq!(delegate.subtask, Some(false));
        assert_eq!(delegate.subagent, None);
        assert!(!loaded.agents.contains_key("bad"));
        assert!(!loaded.commands.contains_key("bad"));
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|d| d.field.ends_with("hooks"))
        );
        assert!(
            !loaded
                .diagnostics
                .iter()
                .any(|d| d.field.contains("delegate")),
            "execution fields do not diagnose: {:?}",
            loaded.diagnostics
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
