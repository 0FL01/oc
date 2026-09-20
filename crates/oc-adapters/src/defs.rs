//! Workspace definitions and instructions loading (T25, CFG06/CFG07).
//!
//! Disk discovery for admitted `.opencode` roots only: skills
//! (`{skill,skills}/<id>/SKILL.md`), agents (`{agent,agents}/<id>.md`),
//! commands (`{command,commands}/<id>.md`). Singular roots load before
//! plural within one source directory; inline `<kind>.json`/`.jsonc`
//! declarations load before Markdown of the same root; later sources
//! replace duplicates with shadowing provenance. Order never depends on
//! filesystem enumeration (entries are sorted). Invalid files produce
//! path/field/reason diagnostics while valid siblings survive; skill
//! bodies stay out of the model projection (served bounded by the native
//! `skill` tool from the pinned snapshot). Nothing here executes.
//!
//! Assumption: inline JSON lives at `<root>/<kind>.json` (singular kind
//! name, e.g. `.opencode/commands.json`); upstream pins only the
//! before-Markdown order, not the filename.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::config::parse_skill;
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

/// Selectable primary-agent profile (Markdown body is local-only).
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

/// Split optional leading `---` frontmatter; returns (fields, body).
///
/// Unclosed or overlong headers are treated as plain body (no partial
/// field extraction).
fn split_frontmatter(text: &str) -> (BTreeMap<String, String>, &str) {
    let mut fields = BTreeMap::new();
    let after = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        Some(after) => after,
        None => return (fields, text),
    };
    let mut offset = 0usize;
    let mut lines = 0usize;
    for line in after.split_inclusive('\n') {
        lines += 1;
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" {
            offset += line.len();
            return (fields, &after[offset..]);
        }
        if lines > 64 {
            return (BTreeMap::new(), text);
        }
        if line.len() <= 1024
            && let Some((key, value)) = trimmed.split_once(':')
        {
            let key = key.trim();
            if matches!(key, "description" | "model" | "variant" | "mode" | "name") {
                fields.insert(key.to_string(), value.trim().trim_matches('"').to_string());
            }
        }
    }
    (BTreeMap::new(), text)
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
    let mut out = Collector {
        defs: LoadedDefs::default(),
        total_bytes: 0,
    };
    for root in roots.iter().take(MAX_DEF_ROOTS) {
        load_root(&mut out, root);
    }
    if roots.len() > MAX_DEF_ROOTS {
        out.defs.diagnostics.push(Diagnostic {
            path: String::new(),
            field: "roots".to_string(),
            reason: format!("too many roots (max {MAX_DEF_ROOTS})"),
        });
    }
    out.defs
}

fn load_root(out: &mut Collector, root: &DefRoot) {
    load_kind(out, root, "skill", &["skill", "skills"]);
    load_kind(out, root, "agent", &["agent", "agents"]);
    load_kind(out, root, "command", &["command", "commands"]);
}

fn load_kind(out: &mut Collector, root: &DefRoot, kind: &str, subs: &[&str]) {
    // Inline JSON/JSONC declaration before Markdown of this root.
    let inline = root.dir.join(format!("{kind}.json"));
    if inline.is_file() {
        load_inline(out, root, kind, &inline, false);
    }
    let inline_c = root.dir.join(format!("{kind}.jsonc"));
    if inline_c.is_file() {
        load_inline(out, root, kind, &inline_c, true);
    }
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

fn load_inline(out: &mut Collector, root: &DefRoot, kind: &str, path: &Path, jsonc: bool) {
    let text = match read_plain(path, &root.dir, MAX_DEF_BODY) {
        Ok(text) => text,
        Err(reason) => {
            out.defs.diagnostics.push(diag(path, kind, &reason));
            return;
        }
    };
    let clean = if jsonc {
        match crate::config::strip_jsonc(&text) {
            Ok(clean) => clean,
            Err(e) => {
                out.defs.diagnostics.push(diag(path, kind, &e.to_string()));
                return;
            }
        }
    } else {
        text
    };
    let value: BTreeMap<String, serde_json::Value> = match serde_json::from_str(&clean) {
        Ok(map) => map,
        Err(e) => {
            out.defs
                .diagnostics
                .push(diag(path, kind, &format!("json: {e}")));
            return;
        }
    };
    for (id, raw) in value {
        if !valid_id(&id) {
            out.defs.diagnostics.push(diag(path, kind, "invalid id"));
            continue;
        }
        let obj = raw.as_object().cloned().unwrap_or_default();
        let description = obj
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let body = obj
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        match kind {
            "skill" => {
                if body.is_empty() {
                    out.defs
                        .diagnostics
                        .push(diag(path, kind, "inline skill needs a body"));
                    continue;
                }
                insert_skill_text(out, root, id, description, body, path);
            }
            "agent" => {
                let model = obj
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let variant = obj
                    .get("variant")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                insert_agent(out, root, id, description, model, variant, path);
            }
            _ => insert_command(out, root, id, description, body, path),
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
    match parse_skill(&id, &text) {
        Ok(meta) => {
            out.total_bytes += text.len();
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

fn insert_agent(
    out: &mut Collector,
    root: &DefRoot,
    id: String,
    description: String,
    model: Option<String>,
    variant: Option<String>,
    path: &Path,
) {
    if out.defs.agents.len() >= MAX_DEFS_PER_KIND {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "too many agents"));
        return;
    }
    if description.len() > 1024 {
        out.defs
            .diagnostics
            .push(diag(path, "agent", "description too large"));
        return;
    }
    out.put_agent(
        AgentDef {
            id: id.clone(),
            description,
            model: model.filter(|s| !s.is_empty()),
            variant: variant.filter(|s| !s.is_empty()),
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
    if out.defs.commands.len() >= MAX_DEFS_PER_KIND {
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
    let lower = body.to_lowercase();
    if lower.contains("subagent") || lower.contains("subtask") {
        out.defs
            .diagnostics
            .push(diag(path, "command", "subagent behavior unsupported"));
        return;
    }
    if body.contains("$(") || body.contains('`') {
        out.defs
            .diagnostics
            .push(diag(path, "command", "shell interpolation unsupported"));
        return;
    }
    out.total_bytes += body.len();
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

fn load_entry(out: &mut Collector, root: &DefRoot, kind: &str, dir: &Path, name: &str) {
    let path = dir.join(name);
    match kind {
        "skill" => {
            if name == "skill.json" || name == "skill.jsonc" {
                return; // handled as inline declaration
            }
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
                return; // ignore foreign files (inline JSON handled above)
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
            let (fields, body) = split_frontmatter(&text);
            if body.len() > MAX_DEF_BODY {
                out.defs
                    .diagnostics
                    .push(diag(&path, kind, "body too large"));
                return;
            }
            if kind == "agent" {
                if let Some(mode) = fields.get("mode")
                    && (mode == "subagent" || mode == "all")
                {
                    out.defs
                        .diagnostics
                        .push(diag(&path, kind, "subagent mode unsupported"));
                    return;
                }
                let description = fields
                    .get("description")
                    .cloned()
                    .or_else(|| {
                        body.lines()
                            .map(str::trim)
                            .find(|l| !l.is_empty())
                            .map(str::to_string)
                    })
                    .unwrap_or_default();
                let model = fields.get("model").cloned();
                let variant = fields.get("variant").cloned();
                insert_agent(out, root, id, description, model, variant, &path);
            } else {
                let description = fields
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
    for byte in agent
        .id
        .bytes()
        .chain([0])
        .chain(agent.description.bytes())
        .chain([0])
        .chain(agent.model.as_deref().unwrap_or("").bytes())
        .chain([0])
        .chain(agent.variant.as_deref().unwrap_or("").bytes())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
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
            "---\ndescription: ok agent\nmodel: m\n---\nbody\n",
        );
        write(&opencode.join("commands/quit.md"), "nope\n");
        write(&opencode.join("commands/deploy.md"), "run $(evil) now\n");
        write(&opencode.join("commands/ship.md"), "cargo test $1\n");
        let loaded = load_definitions(&[root(&opencode, "P")]);
        assert_eq!(loaded.agents.len(), 1);
        assert_eq!(loaded.agents["ok"].model.as_deref(), Some("m"));
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
                .any(|d| d.reason.contains("interpolation"))
        );
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
    fn inline_json_precedes_markdown_in_same_root() {
        let base = tempfile::tempdir().expect("tmp");
        let opencode = base.path().join(".opencode");
        write(
            &opencode.join("commands.json"),
            r#"{"run": {"description": "inline", "body": "echo inline"}}"#,
        );
        write(
            &opencode.join("commands/run.md"),
            "---\ndescription: markdown\n---\necho md\n",
        );
        let loaded = load_definitions(&[root(&opencode, "P")]);
        assert_eq!(loaded.commands["run"].description, "markdown");
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
