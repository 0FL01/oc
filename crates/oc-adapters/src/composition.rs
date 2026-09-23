//! Shared config/catalog snapshot for the CLI and TUI application worker.
//!
//! The supplied project is the admitted Location boundary. This baseline
//! composes existing adapters; broader config/Location support belongs to T35.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Read as _;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::{admitted_fs, config, dcp_auto, defs, discovery, models, provider};
use oc_core::queries::StartupNotice;

/// Fully built application configuration. Contains credentials and must not be logged.
pub struct Composition {
    /// Safe, generation-pinned CLI presentation settings.
    pub tui_chrome: oc_core::queries::TuiChrome,
    /// Immutable effective config for this application instance.
    pub generation: config::Generation,
    /// Selected provider's effective static/discovered models.
    pub catalog: models::ModelCatalog,
    /// Exact model id, without the provider prefix (remaining slashes preserved).
    pub model_id: String,
    /// Native Responses connection configuration.
    pub provider: provider::ResponsesConfig,
    /// Canonical admitted project boundary.
    pub project: PathBuf,
    /// Environment snapshot for substitutions and child processes.
    pub parent_env: BTreeMap<String, String>,
    /// Ordered global + Location instructions for a fixed request lane.
    pub instructions: String,
    /// Selected primary-agent prompt, if configured.
    pub agent_prompt: Option<String>,
    /// Digest of the selected primary profile.
    pub agent_digest: Option<String>,
    /// Selected primary-agent variant.
    pub variant: Option<String>,
    /// All admitted agent profiles for this generation (primary and subagent).
    pub agents: BTreeMap<String, defs::AgentDef>,
    /// Explicitly configured default agent id, if any.
    pub default_agent: Option<String>,
    /// Maximum subagent nesting depth (`experimental.subagent_depth`, default 1).
    pub subagent_depth: u32,
    /// Pinned skill source bytes, loaded once for the application generation.
    pub skills: Vec<(String, String)>,
    /// Invalid skill ids and precise generation diagnostics.
    pub skill_errors: BTreeMap<String, String>,
    /// Literal custom command templates.
    pub commands: BTreeMap<String, String>,
    /// Exact compiled native modules activated by plugin markers.
    pub native_modules: BTreeSet<String>,
    /// Non-fatal definition diagnostics for frontend display.
    pub diagnostics: Vec<String>,
    /// Source-based warning categories for interactive surfaces.
    pub startup_notices: Vec<StartupNotice>,
    /// Effective native DCP policy loaded with this application generation.
    pub dcp_config: dcp_auto::DcpConfig,
    /// Context-preservation policy; independent of filesystem permissions.
    pub dcp_protected: oc_core::context_plan::ProtectedSpec,
}

/// Load ordered user config and resolve an explicitly selected model.
///
/// Missing config, credentials or model selection is an actionable error;
/// there is no mock or automatic model fallback. File substitutions remain
/// untrusted until an explicit source-trust interface is available.
pub async fn load(project: &Path) -> Result<Composition, String> {
    let env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    load_with_env(project, env).await
}

pub(crate) async fn load_with_env(
    project: &Path,
    parent_env: BTreeMap<String, String>,
) -> Result<Composition, String> {
    let project = project
        .canonicalize()
        .map_err(|e| format!("cannot open project {}: {e}", project.display()))?;
    if !project.is_dir() {
        return Err(format!("project {} must be a directory", project.display()));
    }
    let nonempty_env = |key: &str| parent_env.get(key).filter(|v| !v.is_empty());
    let global = nonempty_env("OPENCODE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| nonempty_env("XDG_CONFIG_HOME").map(|p| Path::new(p).join("opencode")))
        .or_else(|| nonempty_env("HOME").map(|p| Path::new(p).join(".config/opencode")));
    // CONFIG.md / config-roots.order.json: JSON before JSONC in each root,
    // one global layer, then direct Location config, then .opencode config.
    let mut roots: Vec<PathBuf> = global.clone().into_iter().collect();
    roots.push(project.clone());
    roots.push(project.join(".opencode"));
    // Admit *all* roots before opening any config. A discovered local root
    // cannot become an external trust boundary, even when its contents are
    // otherwise valid. Pin directory descriptors for subsequent file opens.
    let admitted_roots: Vec<Option<AdmittedRoot>> = roots
        .iter()
        .map(|root| admit_root(root, &project, root == roots.last().expect("local root")))
        .collect::<Result<_, _>>()?;
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for (root, admitted) in roots.iter().zip(&admitted_roots) {
        let Some(admitted) = admitted else { continue };
        for name in ["opencode.json", "opencode.jsonc"] {
            let path = root.join(name);
            let Some((canonical, text)) = read_source_config(admitted, &path)? else {
                continue;
            };
            if seen.insert(canonical.clone()) {
                sources.push(config::Source {
                    path: canonical.to_string_lossy().into_owned(),
                    text,
                    trusted: true,
                });
            }
        }
    }
    if sources.is_empty() {
        return Err("no opencode.json/jsonc found; configure a provider and top-level model (provider/model-id) in the project or XDG opencode config directory".to_string());
    }

    // DCP config is native data, never executable plugin code. Inline `dcp`
    // fragments follow ordinary config precedence; standalone files then layer
    // at the same admitted roots (JSON before JSONC).
    let mut dcp_fragment = serde_json::json!({});
    // Every admitted source that contributed a DCP fragment: an unsupported
    // option must name the file the owner has to edit, not just the field.
    let mut dcp_sources: Vec<String> = Vec::new();
    for admitted in &admitted_roots {
        let Some(admitted) = admitted else { continue };
        let root = &admitted.path;
        for source in &sources {
            if Path::new(&source.path).parent() != Some(root.as_path()) {
                continue;
            }
            let value =
                config::parse_jsonc(&source.text, &source.path).map_err(|e| e.to_string())?;
            if let Some(fragment) = value.get("dcp") {
                merge_json_object(&mut dcp_fragment, fragment)
                    .map_err(|reason| format!("{}: invalid dcp config: {reason}", source.path))?;
                dcp_sources.push(source.path.clone());
            }
        }
        for name in ["dcp.json", "dcp.jsonc"] {
            let path = root.join(name);
            let text = match read_native_config(admitted, name) {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot read dcp config {}: {error}",
                        path.display()
                    ));
                }
            };
            let value = config::parse_jsonc(&text, &path.to_string_lossy())
                .map_err(|error| error.to_string())?;
            merge_json_object(&mut dcp_fragment, &value)
                .map_err(|reason| format!("{}: invalid dcp config: {reason}", path.display()))?;
            dcp_sources.push(path.display().to_string());
        }
    }
    let (dcp_config, dcp_warnings) = dcp_auto::load_config(&dcp_fragment).map_err(|error| {
        if dcp_sources.is_empty() {
            error.to_string()
        } else {
            format!("{error} (dcp config sources: {})", dcp_sources.join(", "))
        }
    })?;
    let dcp_protected = oc_core::context_plan::ProtectedSpec {
        protect_user_messages: dcp_config.protect_user_messages,
        protect_tags: dcp_config.protect_tags,
        file_globs: dcp_config.protected_file_patterns.clone(),
        protected_message_ids: BTreeSet::new(),
    };

    let mut selected = None;
    let mut default_agent = None;
    let mut subagent_depth: u32 = 1;
    let mut enabled = None;
    let mut disabled = Vec::new();
    let mut native_modules = BTreeSet::new();
    let mut plugin_diagnostics = Vec::new();
    for source in &sources {
        let value = config::parse_jsonc(&source.text, &source.path).map_err(|e| e.to_string())?;
        if let Some(model) = value.get("model") {
            let model = model.as_str().ok_or_else(|| {
                format!("{}: model must be a provider/model-id string", source.path)
            })?;
            selected = Some(
                config::substitute(model, &source.path, false, &parent_env)
                    .map_err(|e| e.to_string())?,
            );
        }
        if let Some(agent) = value.get("default_agent") {
            default_agent = Some(
                agent
                    .as_str()
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| {
                        format!("{}: default_agent must be a nonempty string", source.path)
                    })?
                    .to_string(),
            );
        }
        if let Some(experimental) = value.get("experimental") {
            let object = experimental
                .as_object()
                .ok_or_else(|| format!("{}: experimental must be an object", source.path))?;
            if let Some(depth) = object.get("subagent_depth") {
                let depth = depth
                    .as_u64()
                    .filter(|depth| *depth <= u64::from(u32::MAX))
                    .ok_or_else(|| {
                        format!(
                            "{}: experimental.subagent_depth must be a non-negative integer",
                            source.path
                        )
                    })?;
                subagent_depth = depth as u32;
            }
        }
        if let Some(list) = value.get("enabled_providers") {
            enabled = Some(provider_ids(list, "enabled_providers", &source.path)?);
        }
        if let Some(list) = value.get("disabled_providers") {
            disabled = provider_ids(list, "disabled_providers", &source.path)?;
        }
        if let Some(plugins) = value.get("plugin") {
            let plugins = provider_ids(plugins, "plugin", &source.path)?;
            for identity in plugins {
                // Exact compiled aliases only. No plugin is opened or executed.
                let module = roots.iter().find_map(|root| {
                    let root = root.canonicalize().unwrap_or_else(|_| root.clone());
                    config::classify_plugin(&identity, &root.to_string_lossy()).ok()
                });
                let module = module.ok_or_else(|| {
                    format!(
                        "{}: UnsupportedPlugin: unsupported plugin {identity}",
                        source.path
                    )
                })?;
                if module == "ignored-authoring-goal" {
                    plugin_diagnostics.push(format!(
                        "{}: authoring-only plugin {identity} ignored; no package code was loaded",
                        source.path
                    ));
                }
                native_modules.insert(module.to_string());
            }
        }
    }

    // Definitions are merged at their exact source precedence points: config
    // inline domains first, then Markdown from the same admitted root.
    let mut loaded_defs = defs::LoadedDefs::default();
    if let Some(global) = global.as_ref() {
        let global = admitted_roots[0]
            .as_ref()
            .map_or_else(|| global.clone(), |root| root.path.clone());
        merge_config_sources(&mut loaded_defs, &sources, &global)?;
        if let Some(admitted) = admitted_roots[0].as_ref() {
            defs::merge_definition_root_admitted(
                &mut loaded_defs,
                &defs::DefRoot {
                    dir: global.clone(),
                    origin: global.to_string_lossy().into_owned(),
                },
                &admitted.dir,
            );
        }
    }
    merge_config_sources(&mut loaded_defs, &sources, &project)?;
    // The `.opencode` definition root is admitted only inside the Location
    // root; a symlinked root resolving outside fails closed.
    let local_defs = admitted_roots
        .last()
        .and_then(Option::as_ref)
        .map_or_else(|| project.join(".opencode"), |root| root.path.clone());
    merge_config_sources(&mut loaded_defs, &sources, &local_defs)?;
    if let Some(admitted) = admitted_roots.last().and_then(Option::as_ref) {
        defs::merge_definition_root_admitted(
            &mut loaded_defs,
            &defs::DefRoot {
                dir: local_defs.clone(),
                origin: local_defs.to_string_lossy().into_owned(),
            },
            &admitted.dir,
        );
    }

    let mut instruction_files = Vec::new();
    if let Some(global) = global.as_ref()
        && let Some(root) = admitted_roots[0].as_ref()
    {
        let file = global.join("AGENTS.md");
        if let Some(admitted) = admit_instruction(&file, global)? {
            instruction_files.push((
                file.to_string_lossy().into_owned(),
                read_instruction(root, &admitted),
            ));
        }
    }
    let local_agents = project.join("AGENTS.md");
    if let Some(root) = admitted_roots
        .get(usize::from(global.is_some()))
        .and_then(Option::as_ref)
        && let Some(admitted) = admit_instruction(&local_agents, &project)?
    {
        instruction_files.push((
            local_agents.to_string_lossy().into_owned(),
            read_instruction(root, &admitted),
        ));
    }
    let (instructions, instruction_diagnostics) = defs::load_instruction_texts(&instruction_files);

    // Title is now selected automatically. An explicitly malformed profile
    // cannot be mistaken for absence and replaced with the built-in policy.
    if !loaded_defs.agents.contains_key("title")
        && let Some(diagnostic) = loaded_defs.diagnostics.iter().find(|diagnostic| {
            diagnostic.field == "agent.title"
                || Path::new(&diagnostic.path)
                    .file_stem()
                    .is_some_and(|stem| stem == "title")
        })
    {
        return Err(format!(
            "title agent is invalid: {}: {}",
            diagnostic.path, diagnostic.reason
        ));
    }
    let selected_agent = match default_agent.as_deref() {
        Some(id) => match loaded_defs.agents.get(id) {
            Some(agent) if !agent.primary_capable() => {
                return Err(format!(
                    "selected agent {id} is subagent-only and cannot be a primary agent"
                ));
            }
            Some(agent) => Some(agent.clone()),
            None => {
                let diagnostic = loaded_defs.diagnostics.iter().find(|diagnostic| {
                    diagnostic.field == format!("agent.{id}")
                        || Path::new(&diagnostic.path)
                            .file_stem()
                            .is_some_and(|stem| stem == id)
                });
                return Err(match diagnostic {
                    Some(diagnostic) => format!(
                        "selected agent {id} is invalid: {}: {}",
                        diagnostic.path, diagnostic.reason
                    ),
                    None => format!("unknown selected agent {id}"),
                });
            }
        },
        None => None,
    };

    let selected = match selected {
        Some(selected) => selected,
        None => {
            // Actionable, bounded: name the models the admitted config
            // declares so the owner can copy one into the top-level `model`.
            let mut candidates: Vec<String> = Vec::new();
            for source in &sources {
                let Ok(value) = config::parse_jsonc(&source.text, &source.path) else {
                    continue;
                };
                let Some(providers) = value.get("provider").and_then(|v| v.as_object()) else {
                    continue;
                };
                for (provider_id, provider) in providers {
                    let Some(models) = provider.get("models").and_then(|v| v.as_object()) else {
                        continue;
                    };
                    for model_id in models.keys() {
                        candidates.push(format!("{provider_id}/{model_id}"));
                    }
                }
            }
            candidates.sort();
            candidates.dedup();
            let total = candidates.len();
            candidates.truncate(12);
            let listed = if candidates.is_empty() {
                "no models are declared in the admitted config; add a provider with a models map"
                    .to_string()
            } else {
                let suffix = if total > candidates.len() {
                    format!(" (and {} more)", total - candidates.len())
                } else {
                    String::new()
                };
                format!("configured models: {}{suffix}", candidates.join(", "))
            };
            return Err(format!(
                "model required: set top-level `model` to provider/model-id in the Location \
                 or global opencode.json/jsonc; {listed}"
            ));
        }
    };
    let selected = selected_agent
        .as_ref()
        .and_then(|agent| agent.model.clone())
        .unwrap_or(selected);
    let (provider_id, model_id) = selected
        .split_once('/')
        .filter(|(p, m)| !p.trim().is_empty() && !m.trim().is_empty())
        .ok_or_else(|| {
            "model must be provider/model-id; set an explicit configured model".to_string()
        })?;
    if disabled.iter().any(|id| id == provider_id)
        || enabled
            .as_ref()
            .is_some_and(|ids| !ids.iter().any(|id| id == provider_id))
    {
        return Err(format!(
            "selected provider {provider_id} is disabled by provider selection"
        ));
    }
    let selected_providers = HashSet::from([provider_id.to_string()]);
    let mut generation = config::assemble(&sources, &parent_env, Some(&selected_providers))
        .map_err(|e| e.to_string())?;
    // Keep central authority independent of the startup primary selection.
    // The effective primary's constraints are snapshotted with its workspace.
    if let Some(level) = dcp_config.compress_permission {
        generation
            .permission_rules
            .module_permission("compress", level);
        generation
            .permissions
            .entry("compress".to_string())
            .and_modify(|current| {
                if permission_rank(level) > permission_rank(*current) {
                    *current = level;
                }
            })
            .or_insert(level);
        generation.provenance.insert(
            "permissions.compress".to_string(),
            "native dcp config".to_string(),
        );
    }
    let entry = generation.providers.get(provider_id).ok_or_else(|| {
        format!("selected provider {provider_id} is not configured; add provider.{provider_id}")
    })?;
    let provider = provider::ResponsesConfig {
        headers: entry.options.headers.clone(),
        set_cache_key: entry.options.set_cache_key.unwrap_or(false),
        base_url: entry.options.base_url.clone(),
        api_key: entry.options.api_key.clone(),
        timeout: entry.options.timeout,
        chunk_timeout_ms: entry
            .options
            .chunk_timeout
            .unwrap_or(provider::CHUNK_TIMEOUT_MS),
        connect_timeout: Duration::from_secs(10),
        allow_private: parent_env.get("OC_TEST_ALLOW_LOOPBACK").map(String::as_str) == Some("1"),
    };
    let url = reqwest::Url::parse(&provider.base_url)
        .map_err(|_| format!("provider.{provider_id}.options.baseURL must be an HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "provider.{provider_id}.options.baseURL must be an HTTP(S) prefix without credentials, query or fragment"
        ));
    }
    let mut catalog = models::ModelCatalog {
        provider: provider_id.to_string(),
        models: entry.models.clone(),
    };
    // The existing native daily-direct profile enables discovery for this
    // provider; an admitted JS alias denotes the same compiled module.
    if provider_id == discovery::PROVIDER_ID && discovery::should_run(&disabled, enabled.as_deref())
    {
        let client = discovery::ReqwestDiscoveryClient::new(provider.connect_timeout)
            .map_err(|e| format!("model discovery: {e}"))?;
        let outcome = discovery::refresh(
            &discovery::RealClock,
            &client,
            &provider.base_url,
            &provider.api_key,
            &provider.headers,
            &catalog.models,
            &AtomicBool::new(false),
        )
        .await;
        catalog.models = outcome.models;
        generation.warnings.extend(outcome.warnings);
    }
    models::select_model(&catalog, model_id).map_err(|e| {
        let warnings = generation.warnings.join(" ");
        format!(
            "{e}; configure provider.{provider_id}.models or check native discovery. {warnings}"
        )
    })?;
    let skills = loaded_defs
        .skills
        .values()
        .map(|skill| (skill.id.clone(), skill.body.clone()))
        .collect();
    let commands = loaded_defs
        .commands
        .values()
        .map(|command| (command.id.clone(), command.body.clone()))
        .collect();
    let mut startup_notices = Vec::new();
    if !loaded_defs.diagnostics.is_empty() {
        startup_notices.push(StartupNotice::Definitions);
    }
    if !plugin_diagnostics.is_empty() {
        startup_notices.push(StartupNotice::Plugin);
    }
    if !dcp_warnings.is_empty() {
        startup_notices.push(StartupNotice::Dcp);
    }
    if !instruction_diagnostics.is_empty() {
        startup_notices.push(StartupNotice::Instructions);
    }
    let mut diagnostics: Vec<String> = loaded_defs
        .diagnostics
        .iter()
        .map(|diagnostic| {
            format!(
                "{}: {}: {}",
                diagnostic.path, diagnostic.field, diagnostic.reason
            )
        })
        .collect();
    diagnostics.extend(plugin_diagnostics);
    diagnostics.extend(
        dcp_warnings
            .into_iter()
            .map(|warning| format!("dcp: {warning}")),
    );
    diagnostics.extend(instruction_diagnostics.iter().map(|diagnostic| {
        format!(
            "{}: {}: {}",
            diagnostic.path, diagnostic.field, diagnostic.reason
        )
    }));
    let mut skill_errors = BTreeMap::new();
    for diagnostic in &loaded_defs.diagnostics {
        if diagnostic.field == "skill"
            && let Some(id) = skill_diagnostic_id(&diagnostic.path)
        {
            skill_errors.insert(
                id.to_string(),
                format!(
                    "malformed skill {id}: {}: {}",
                    diagnostic.path, diagnostic.reason
                ),
            );
        }
    }
    let mut tui_chrome = oc_core::queries::TuiChrome {
        location: Some(project.to_string_lossy().into_owned()),
        build_channel: if cfg!(debug_assertions) {
            oc_core::queries::TuiBuildChannel::Local
        } else {
            oc_core::queries::TuiBuildChannel::Packaged
        },
        ..Default::default()
    };
    // Use the existing admitted-root reader, not arbitrary TUI-side filesystem access.
    for (root, admitted) in roots.iter().zip(&admitted_roots) {
        let Some(admitted) = admitted else { continue };
        for name in ["cli.json", "cli.jsonc"] {
            let Some(text) = read_native_config(admitted, name)? else {
                continue;
            };
            let value = config::parse_jsonc(&text, &root.join(name).to_string_lossy())
                .map_err(|e| e.to_string())?;
            if let Some(v) = value.pointer("/debug/devtools") {
                tui_chrome.devtools = Some(v.as_bool().ok_or("debug.devtools must be boolean")?);
            }
            if let Some(v) = value.pointer("/session/sidebar") {
                tui_chrome.sidebar_hidden = match v.as_str() {
                    Some("auto") => false,
                    Some("hide") => true,
                    _ => return Err("session.sidebar must be auto or hide".into()),
                };
            }
            if let Some(v) = value.pointer("/tabs/layout") {
                tui_chrome.vertical_tabs_width = match v.as_str() {
                    Some("horizontal") => 0,
                    Some("vertical") => 42,
                    _ => return Err("tabs.layout must be horizontal or vertical".into()),
                };
            }
        }
    }
    Ok(Composition {
        tui_chrome,
        generation,
        catalog,
        model_id: model_id.to_string(),
        provider,
        project,
        parent_env,
        instructions,
        agent_prompt: selected_agent.as_ref().map(|agent| agent.body.clone()),
        agent_digest: selected_agent.as_ref().map(defs::agent_digest),
        variant: selected_agent.and_then(|agent| agent.variant),
        agents: loaded_defs.agents,
        default_agent,
        subagent_depth,
        skills,
        skill_errors,
        commands,
        native_modules,
        diagnostics,
        startup_notices,
        dcp_config,
        dcp_protected,
    })
}

fn merge_json_object(
    target: &mut serde_json::Value,
    layer: &serde_json::Value,
) -> Result<(), &'static str> {
    let target = target.as_object_mut().ok_or("base must be an object")?;
    let layer = layer.as_object().ok_or("fragment must be an object")?;
    for (key, value) in layer {
        if value.is_object() && target.get(key).is_some_and(serde_json::Value::is_object) {
            merge_json_object(target.get_mut(key).expect("existing key"), value)?;
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
    Ok(())
}

// JSONC parsing makes several working copies (char vectors, normalized text,
// JSON values). 16 MiB accepts ordinary multi-MiB user configs while bounding
// startup allocation; small native DCP/CLI configs retain their existing cap.
const SOURCE_CONFIG_CAP: usize = 16 * 1024 * 1024;
const NATIVE_CONFIG_CAP: usize = 1024 * 1024;

struct AdmittedRoot {
    path: PathBuf,
    dir: File,
}

fn admit_root(root: &Path, project: &Path, local: bool) -> Result<Option<AdmittedRoot>, String> {
    let canonical = match root.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot resolve config root {}: {e}",
                root.display()
            ));
        }
    };
    if local && !canonical.starts_with(project) {
        return Err(format!(
            "refusing {}: resolves outside the Location root {} ({})",
            root.display(),
            project.display(),
            canonical.display()
        ));
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&canonical)
        .map_err(|e| format!("cannot open config root {}: {e}", root.display()))?;
    // If an ancestor was swapped during admission, the opened directory, not
    // the earlier pathname resolution, decides whether it is still admitted.
    let opened = std::fs::canonicalize(format!("/proc/self/fd/{}", dir.as_raw_fd()))
        .map_err(|e| format!("cannot verify config root {}: {e}", root.display()))?;
    if opened != canonical || (local && !opened.starts_with(project)) {
        return Err(format!(
            "refusing config root {}: changed during admission",
            root.display()
        ));
    }
    Ok(Some(AdmittedRoot {
        path: canonical,
        dir,
    }))
}

fn read_bounded(mut file: File, cap: usize) -> Result<String, String> {
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("not a regular file".to_string());
    }
    if metadata.len() > cap as u64 {
        return Err(format!("exceeds {} MiB config budget", cap / 1024 / 1024));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > cap {
        return Err(format!("exceeds {} MiB config budget", cap / 1024 / 1024));
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())
}

fn read_source_config(
    root: &AdmittedRoot,
    path: &Path,
) -> Result<Option<(PathBuf, String)>, String> {
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot resolve config {}: {e}", path.display())),
    };
    let relative = canonical.strip_prefix(&root.path).map_err(|_| {
        format!(
            "refusing config {}: resolves outside its admitted root {} ({})",
            path.display(),
            root.path.display(),
            canonical.display()
        )
    })?;
    let file = admitted_fs::open_beneath(&root.dir, relative, libc::O_RDONLY)
        .map_err(|e| format!("cannot open admitted config {}: {e}", path.display()))?;
    let text = read_bounded(file, SOURCE_CONFIG_CAP)
        .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
    Ok(Some((canonical, text)))
}

fn read_native_config(root: &AdmittedRoot, name: &str) -> Result<Option<String>, String> {
    let file = match admitted_fs::open_beneath(&root.dir, Path::new(name), libc::O_RDONLY) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    read_bounded(file, NATIVE_CONFIG_CAP).map(Some)
}

fn read_instruction(root: &AdmittedRoot, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(&root.path)
        .map_err(|_| "outside admitted root".to_string())?;
    let mut file = admitted_fs::open_beneath(&root.dir, relative, libc::O_RDONLY)
        .map_err(|_| "unreadable".to_string())?;
    let meta = file.metadata().map_err(|_| "unreadable".to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    if meta.len() > defs::MAX_INSTRUCTIONS_FILE as u64 {
        return Err("file too large".to_string());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(defs::MAX_INSTRUCTIONS_FILE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable".to_string())?;
    if bytes.len() > defs::MAX_INSTRUCTIONS_FILE {
        return Err("file too large".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string())
}

fn permission_rank(level: config::Permission) -> u8 {
    match level {
        config::Permission::Allow => 0,
        config::Permission::Ask => 1,
        config::Permission::Deny => 2,
    }
}

/// Skill id from a diagnostic path: `<id>/SKILL.md` or flat `<id>.md`.
fn skill_diagnostic_id(path: &str) -> Option<&str> {
    let path = Path::new(path);
    match path.file_name().and_then(|name| name.to_str()) {
        Some("SKILL.md") => path.parent()?.file_name()?.to_str(),
        _ => path.file_stem()?.to_str(),
    }
}

/// Admit an instruction file only when it stays inside its admitted root.
///
/// Returns the canonical path when the file exists inside `root`, `None`
/// when it is absent, and fails closed when a symlink resolves outside.
fn admit_instruction(file: &Path, root: &Path) -> Result<Option<PathBuf>, String> {
    let canonical_root = match root.canonicalize() {
        Ok(canonical) => canonical,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot resolve config root {}: {e}",
                root.display()
            ));
        }
    };
    let canonical = match file.canonicalize() {
        Ok(canonical) => canonical,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "cannot resolve instructions {}: {e}",
                file.display()
            ));
        }
    };
    if !canonical.starts_with(&canonical_root) {
        return Err(format!(
            "refusing instructions {}: resolves outside its admitted root {} ({})",
            file.display(),
            root.display(),
            canonical.display()
        ));
    }
    Ok(Some(canonical))
}

fn merge_config_sources(
    definitions: &mut defs::LoadedDefs,
    sources: &[config::Source],
    parent: &Path,
) -> Result<(), String> {
    for source in sources {
        if Path::new(&source.path).parent() == Some(parent) {
            let value = config::parse_jsonc(&source.text, &source.path)
                .map_err(|error| error.to_string())?;
            defs::merge_config_definitions(definitions, &value, &source.path);
        }
    }
    Ok(())
}

fn provider_ids(
    value: &serde_json::Value,
    field: &str,
    source: &str,
) -> Result<Vec<String>, String> {
    serde_json::from_value(value.clone())
        .map_err(|_| format!("{source}: {field} must be an array of strings"))
}

#[cfg(test)]
mod tests {
    use super::load_with_env;
    use std::collections::BTreeMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn v07a_external_discovered_root_is_refused_before_config_or_substitution() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("local-secret-fixture"), "external-key").expect("secret");
        std::fs::write(
            outside.join("opencode.json"),
            r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"{file:local-secret-fixture}"},"models":{"main":{}}}},"mcp":{"trap":{"type":"local","command":["never-run"]}},"command":{"trap":"never-run"}}"#,
        )
        .expect("outside config");
        std::os::unix::fs::symlink(&outside, project.join(".opencode")).expect("symlink");
        let error = load_with_env(&project, BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("discovered root cannot trust outside config");
        assert!(
            error.contains(".opencode") && error.contains("outside"),
            "{error}"
        );
        assert!(!error.contains("external-key"), "{error}");
    }

    #[tokio::test]
    async fn v07a_explicit_external_global_and_nested_local_jsonc_symlink_work() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        let global = dir.path().join("external-global");
        std::fs::create_dir_all(project.join("nested")).expect("nested");
        std::fs::create_dir_all(&global).expect("global");
        std::fs::write(global.join("key"), "global-key").expect("key");
        std::fs::write(global.join("opencode.json"), r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"{file:key}"},"models":{"main":{}}}}}"#).expect("global config");
        std::fs::write(
            project.join("nested/local.jsonc"),
            "{ // local override\n \"model\": \"fixture/alternate\",}\n",
        )
        .expect("local config");
        // An in-root symlink for the discovered root is allowed as well.
        std::os::unix::fs::symlink(project.join("nested"), project.join(".opencode"))
            .expect("root link");
        std::os::unix::fs::symlink(
            project.join("nested/local.jsonc"),
            project.join("nested/opencode.jsonc"),
        )
        .expect("config link");
        std::fs::write(global.join("opencode.jsonc"), r#"{// global JSONC overrides the preceding JSON
            "provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"{file:key}"},"models":{"main":{},"alternate":{}}}},}"#).expect("global jsonc");
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        )]);
        let loaded = load_with_env(&project, env)
            .await
            .expect("admitted composition");
        assert_eq!(loaded.model_id, "alternate");
        assert_eq!(loaded.provider.api_key, "global-key");
    }

    #[tokio::test]
    async fn v07a_large_normal_config_is_not_limited_to_one_mib() {
        let dir = tempfile::tempdir().expect("fixture");
        let mut config = r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{}}}},"unused":""#.to_string();
        config.push_str(&"x".repeat(1024 * 1024 + 16));
        config.push_str("\"}");
        std::fs::write(dir.path().join("opencode.json"), config).expect("large config");
        assert_eq!(
            load_with_env(dir.path(), BTreeMap::new())
                .await
                .expect("large regular config")
                .model_id,
            "main"
        );
    }

    #[tokio::test]
    async fn v07a_definitions_symlink_inside_root_and_explicit_external_global_stay_valid() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        let global = dir.path().join("external-global");
        std::fs::create_dir_all(project.join(".opencode/nested/skills")).expect("local skills");
        std::fs::create_dir_all(global.join("nested/agents")).expect("global agents");
        std::fs::write(global.join("opencode.json"), r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{}}}}}"#).expect("config");
        std::fs::write(
            project.join(".opencode/nested/skills/local.md"),
            "---\ndescription: local-in-root\n---\nlocal body",
        )
        .expect("skill");
        std::fs::write(
            global.join("nested/agents/global.md"),
            "---\ndescription: global-external\n---\nglobal agent",
        )
        .expect("agent");
        std::os::unix::fs::symlink(
            project.join(".opencode/nested/skills"),
            project.join(".opencode/skills"),
        )
        .expect("local link");
        std::os::unix::fs::symlink(global.join("nested/agents"), global.join("agents"))
            .expect("global link");
        let env = BTreeMap::from([(
            "OPENCODE_CONFIG_DIR".into(),
            global.to_string_lossy().into_owned(),
        )]);
        let loaded = load_with_env(&project, env).await.expect("composition");
        assert!(
            loaded
                .skills
                .iter()
                .any(|(id, body)| id == "local" && body.contains("local body"))
        );
        assert_eq!(loaded.agents["global"].body, "global agent");
        assert_eq!(loaded.agents["global"].origin, global.to_string_lossy());
    }

    #[tokio::test]
    async fn v07a_racing_instruction_symlink_never_reads_external_text() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(project.join("opencode.json"), r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{}}}}}"#).expect("config");
        std::fs::write(project.join("inside.md"), "V07A_SAFE_INSTRUCTIONS_1033").expect("inside");
        let outside = dir.path().join("outside.md");
        std::fs::write(&outside, "V07A_EXTERNAL_INSTRUCTIONS_9b22").expect("outside");
        let link = project.join("AGENTS.md");
        std::os::unix::fs::symlink(project.join("inside.md"), &link).expect("initial link");
        assert!(
            load_with_env(&project, BTreeMap::new())
                .await
                .expect("in-root instruction")
                .instructions
                .contains("V07A_SAFE_INSTRUCTIONS_1033")
        );
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker_project = project.clone();
        let worker_outside = outside.clone();
        let worker = std::thread::spawn(move || {
            let mut external = true;
            while !worker_stop.load(Ordering::Relaxed) {
                let replacement = worker_project.join("next-agents.md");
                let target = if external {
                    worker_outside.clone()
                } else {
                    worker_project.join("inside.md")
                };
                std::os::unix::fs::symlink(target, &replacement).expect("replacement");
                std::fs::rename(&replacement, worker_project.join("AGENTS.md"))
                    .expect("atomic swap");
                external = !external;
                std::thread::yield_now();
            }
        });
        let mut escaped = false;
        for _ in 0..80 {
            if let Ok(loaded) = load_with_env(&project, BTreeMap::new()).await {
                escaped |= loaded
                    .instructions
                    .contains("V07A_EXTERNAL_INSTRUCTIONS_9b22");
            }
        }
        stop.store(true, Ordering::Relaxed);
        worker.join().expect("swapper");
        assert!(
            !escaped,
            "outside instructions reached a composition generation"
        );
    }

    #[tokio::test]
    async fn ordered_sources_select_exact_model_and_selected_credentials() {
        let dir = tempfile::tempdir().expect("fixture");
        let global = dir.path().join("config/opencode");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&global).expect("global");
        std::fs::create_dir_all(project.join(".opencode")).expect("project");
        std::fs::write(
            global.join("opencode.json"),
            r#"{
            "model": "unused/old", "provider": {
                "unused": {"options": {"apiKey": "{env:ABSENT}"}},
                "fixture": {"options": {
                    "baseURL": "https://example.invalid/proxy/v1",
                    "apiKey": "{env:FIXTURE_KEY}", "chunkTimeout": 1234
                }, "models": {"org/new": {"limit": {"context": 1000, "output": 100}}}}
            }
        }"#,
        )
        .expect("config");
        std::fs::write(
            project.join("opencode.json"),
            r#"{"model":"fixture/missing"}"#,
        )
        .expect("project config");
        std::fs::write(
            project.join(".opencode/opencode.json"),
            r#"{"model":"fixture/also-missing"}"#,
        )
        .expect("local json");
        std::fs::write(
            project.join(".opencode/opencode.jsonc"),
            r#"{
            // JSONC wins inside the final source root.
            "model": "fixture/org/new",
        }"#,
        )
        .expect("local jsonc");
        let env = BTreeMap::from([
            (
                "XDG_CONFIG_HOME".to_string(),
                dir.path().join("config").to_string_lossy().into_owned(),
            ),
            ("FIXTURE_KEY".to_string(), "fixture-key".to_string()),
        ]);
        let loaded = load_with_env(&project, env.clone())
            .await
            .expect("composition");
        assert_eq!(loaded.model_id, "org/new");
        assert_eq!(loaded.catalog.provider, "fixture");
        assert_eq!(loaded.provider.api_key, "fixture-key");
        assert_eq!(loaded.provider.chunk_timeout_ms, 1234);
        assert!(!loaded.provider.allow_private);
        assert_eq!(loaded.generation.providers.len(), 1);
        assert_eq!(
            loaded.generation.provenance["provider.fixture"],
            global.join("opencode.json").to_string_lossy()
        );
        let mut env = env;
        env.insert("OC_TEST_ALLOW_LOOPBACK".to_string(), "1".to_string());
        assert!(
            load_with_env(&project, env.clone())
                .await
                .expect("opt-in")
                .provider
                .allow_private
        );
        env.remove("FIXTURE_KEY");
        let error = load_with_env(&project, env)
            .await
            .map(|_| ())
            .expect_err("missing key");
        assert!(error.contains("missing credential"));
    }

    /// A config file that resolves outside its admitted root is not a
    /// trusted source: `{file:}` must never be read relative to an outside
    /// directory (fail closed, not a silent canonicalize-and-trust).
    #[tokio::test]
    async fn symlinked_config_outside_the_root_is_refused() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("secret.txt"), "outside-secret-value").expect("secret");
        std::fs::write(
            outside.join("config.json"),
            r#"{"model":"fixture/org/new","provider":{"fixture":{"options":{
                "baseURL":"https://example.invalid/proxy/v1",
                "apiKey":"{file:secret.txt}"
            },"models":{"org/new":{}}}}}"#,
        )
        .expect("outside config");
        std::os::unix::fs::symlink(outside.join("config.json"), project.join("opencode.json"))
            .expect("symlink");
        let error = load_with_env(&project, BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("symlink escape must fail closed");
        assert!(error.contains("outside"), "{error}");
    }

    /// The admitted `.opencode` root must stay inside the Location root:
    /// a symlinked root cannot pull in outside definitions.
    #[tokio::test]
    async fn symlinked_local_root_outside_the_project_is_refused() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        let outside = dir.path().join("outside/.opencode");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::create_dir_all(outside.join("command")).expect("outside root");
        std::fs::write(
            project.join("opencode.json"),
            r#"{"model":"fixture/org/new","provider":{"fixture":{"options":{
                "baseURL":"https://example.invalid/proxy/v1","apiKey":"k"
            },"models":{"org/new":{}}}}}"#,
        )
        .expect("config");
        std::fs::write(
            outside.join("command/escape.md"),
            "---\ndescription: outside command\n---\noutside payload\n",
        )
        .expect("outside command");
        std::os::unix::fs::symlink(&outside, project.join(".opencode")).expect("symlink");
        let error = load_with_env(&project, BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("symlinked local root must fail closed");
        assert!(error.contains("outside"), "{error}");
    }

    /// Containment, not a blanket symlink ban: a config symlinked inside its
    /// own admitted root stays trusted and keeps `{file:}` working.
    #[tokio::test]
    async fn in_root_symlinked_config_stays_admitted() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(project.join("key.txt"), "in-root-key").expect("key");
        std::fs::write(
            project.join("real.json"),
            r#"{"model":"fixture/org/new","provider":{"fixture":{"options":{
                "baseURL":"https://example.invalid/proxy/v1",
                "apiKey":"{file:key.txt}"
            },"models":{"org/new":{}}}}}"#,
        )
        .expect("real config");
        std::os::unix::fs::symlink(project.join("real.json"), project.join("opencode.json"))
            .expect("symlink");
        let loaded = load_with_env(&project, BTreeMap::new())
            .await
            .expect("in-root symlink is admitted");
        assert_eq!(loaded.provider.api_key, "in-root-key");
    }

    /// An unsupported DCP option names the file the owner must edit.
    #[tokio::test]
    async fn unsupported_dcp_option_names_its_source() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        std::fs::write(
            project.join("opencode.json"),
            r#"{"model":"fixture/org/new","provider":{"fixture":{"options":{
                "baseURL":"https://example.invalid/proxy/v1","apiKey":"k"
            },"models":{"org/new":{}}}}}"#,
        )
        .expect("config");
        std::fs::write(
            project.join("dcp.jsonc"),
            r#"{"experimental": {"customPrompts": true}}"#,
        )
        .expect("dcp config");
        let error = load_with_env(&project, BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("unsupported option must fail closed");
        assert!(
            error.contains("customPrompts") && error.contains("dcp.jsonc"),
            "the diagnostic must name the source file: {error}"
        );

        // Subagents are a future feature: `allowSubAgents` is tolerated with a
        // visible warning instead of blocking the application.
        std::fs::write(
            project.join("dcp.jsonc"),
            r#"{"experimental": {"allowSubAgents": true}}"#,
        )
        .expect("dcp config");
        let loaded = load_with_env(&project, BTreeMap::new())
            .await
            .expect("allowSubAgents must not block");
        assert!(
            loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("allowSubAgents")),
            "the ignored option is reported: {:?}",
            loaded.diagnostics
        );
    }

    #[tokio::test]
    async fn missing_config_and_missing_model_are_errors() {
        let dir = tempfile::tempdir().expect("fixture");
        let error = load_with_env(dir.path(), BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("missing config");
        assert!(error.contains("no opencode.json/jsonc"));
        std::fs::write(dir.path().join("opencode.json"), "{}").expect("config");
        let error = load_with_env(dir.path(), BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("missing model");
        assert!(error.contains("model required"));
    }

    /// Subagent S3: every admitted agent stays in the catalog, the depth knob
    /// comes from `experimental.subagent_depth`, and a subagent-only
    /// `default_agent` fails closed instead of becoming a primary.
    #[tokio::test]
    async fn subagent_catalog_depth_and_subagent_only_default_agent() {
        let dir = tempfile::tempdir().expect("fixture");
        let config = dir.path().join("opencode.json");
        let base = r#"{
            "model": "fixture/main",
            "provider": {"fixture": {"options": {
                "baseURL": "https://example.invalid/v1", "apiKey": "k"
            }, "models": {"main": {}}}},
            "agent": {
                "boss": {"prompt": "lead", "mode": "primary"},
                "helper": {"prompt": "help", "mode": "subagent", "description": "Helper"},
                "general": {"prompt": "any"}
            }
        }"#;
        std::fs::write(&config, base).expect("config");
        let loaded = load_with_env(dir.path(), BTreeMap::new())
            .await
            .expect("composition");
        assert_eq!(loaded.subagent_depth, 1);
        let ids: Vec<&str> = loaded.agents.keys().map(String::as_str).collect();
        assert_eq!(ids, ["boss", "general", "helper"]);
        assert!(!loaded.agents["boss"].subagent_capable());
        assert!(loaded.agents["general"].primary_capable());
        assert!(loaded.agents["general"].subagent_capable());
        assert!(!loaded.agents["helper"].primary_capable());

        std::fs::write(
            &config,
            base.replace(
                "\"agent\": {",
                "\"experimental\": {\"subagent_depth\": 2}, \"agent\": {",
            ),
        )
        .expect("config");
        let loaded = load_with_env(dir.path(), BTreeMap::new())
            .await
            .expect("composition");
        assert_eq!(loaded.subagent_depth, 2);

        std::fs::write(
            &config,
            r#"{"model":"fixture/main","provider":{"fixture":{"options":{
                "baseURL":"https://example.invalid/v1","apiKey":"k"
            },"models":{"main":{}}}},"experimental":{"subagent_depth":"two"}}"#,
        )
        .expect("config");
        let error = load_with_env(dir.path(), BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("depth shape");
        assert!(error.contains("experimental.subagent_depth"), "{error}");

        std::fs::write(
            &config,
            base.replace(
                "\"agent\": {",
                "\"default_agent\": \"helper\", \"agent\": {",
            ),
        )
        .expect("config");
        let error = load_with_env(dir.path(), BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("subagent-only default agent");
        assert!(error.contains("helper"), "{error}");
        assert!(error.contains("subagent-only"), "{error}");
    }

    #[tokio::test]
    async fn aud18_static_ludka_context_above_discovery_cap_is_unchanged() {
        let dir = tempfile::tempdir().expect("fixture");
        std::fs::write(
            dir.path().join("opencode.json"),
            r#"{
                "model": "ludka/org/static",
                "provider": {"ludka": {
                    "options": {
                        "baseURL": "https://example.invalid/v1",
                        "apiKey": "fixture-key"
                    },
                    "models": {"org/static": {
                        "name": "Static",
                        "limit": {"context": 700000, "output": 32000}
                    }}
                }}
            }"#,
        )
        .expect("config");
        let loaded = load_with_env(dir.path(), BTreeMap::new())
            .await
            .expect("static composition");
        assert_eq!(
            loaded.catalog.models["org/static"]["limit"]["context"],
            700_000
        );
    }

    #[tokio::test]
    async fn aud18_composition_sends_configured_discovery_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).await.expect("read");
                assert_ne!(read, 0, "request ended before headers");
                request.extend_from_slice(&chunk[..read]);
            }
            let body = serde_json::to_vec(&serde_json::json!({
                "object": "list",
                "data": [{
                    "id": "org/dynamic",
                    "context_length": 1000,
                    "max_completion_tokens": 100,
                }],
            }))
            .expect("body");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("response");
            stream.write_all(&body).await.expect("body");
            String::from_utf8(request).expect("request text")
        });

        let dir = tempfile::tempdir().expect("fixture");
        std::fs::write(
            dir.path().join("opencode.json"),
            format!(
                r#"{{
                    "model": "ludka2/org/dynamic",
                    "provider": {{"ludka2": {{"options": {{
                        "baseURL": "http://{address}/v1",
                        "apiKey": "fresh-key",
                        "headers": {{
                            "authorization": "Bearer stale-lower",
                            "AUTHORIZATION": "Bearer stale-upper",
                            "accept": "text/plain",
                            "AcCePt": "application/xml",
                            "x-configured": "preserved"
                        }}
                    }}}}}}
                }}"#
            ),
        )
        .expect("config");
        let loaded = load_with_env(dir.path(), BTreeMap::new())
            .await
            .expect("composition");
        assert!(loaded.catalog.models.contains_key("org/dynamic"));

        let request = server.await.expect("server");
        assert!(request.starts_with("GET /v1/models HTTP/1.1\r\n"));
        let headers: Vec<_> = request
            .lines()
            .skip(1)
            .take_while(|line| !line.is_empty())
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name, value.trim()))
            .collect();
        let authorization: Vec<_> = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .collect();
        let accept: Vec<_> = headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("accept"))
            .collect();
        assert_eq!(authorization, vec![&("authorization", "Bearer fresh-key")]);
        assert_eq!(accept, vec![&("accept", "application/json")]);
        assert!(headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("x-configured") && *value == "preserved"
        }));
    }
}
