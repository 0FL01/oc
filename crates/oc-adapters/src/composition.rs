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

use crate::{admitted_fs, config, dcp_auto, defs, discovery, models, provider, trace};
use oc_core::queries::{
    NativePlugin, PluginEntry, PluginInventory, PluginStatus, ServiceAction, ServiceCode,
    ServiceDiagnostic, ServiceKind, ServiceStage, StartupNotice,
};
mod catalog;

#[cfg(test)]
mod controls_tests;
pub(crate) mod go_catalog;
mod provider_readiness;
pub use catalog::{CatalogListing, load_catalog};
pub(crate) use provider_readiness::ProviderState;

/// Fully built application configuration. Contains credentials and must not be logged.
pub struct Composition {
    /// Consumer state projected by the application owner, independent of selection.
    pub(crate) approval_consumer_mode: std::sync::atomic::AtomicU8,
    pub(crate) question_consumer: AtomicBool,
    pub(crate) permission_preference: AtomicBool,
    /// Owner-selected CLI configuration file for permission mode persistence.
    pub permission_mode_source: PathBuf,
    permission_mode_root: AdmittedRoot,
    /// Frozen source authority for explicitly activating disabled MCP templates.
    pub(crate) mcp_activation: std::sync::Arc<McpActivation>,
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
    pub(crate) provider_state: ProviderState,
    pub(crate) go_catalog: Option<std::sync::Arc<crate::models_dev::GoCatalog>>,
    /// Canonical admitted project boundary.
    pub project: PathBuf,
    /// Environment snapshot for substitutions and child processes.
    pub parent_env: BTreeMap<String, String>,
    /// Ordered global + Location instructions for a fixed request lane.
    pub instructions: String,
    pub(crate) instruction_roots: Vec<crate::instructions::Root>,
    /// Selected primary-agent prompt, if configured.
    pub agent_prompt: Option<String>,
    /// Digest of the selected primary profile.
    pub agent_digest: Option<String>,
    /// Selected primary-agent variant.
    pub variant: Option<String>,
    /// All admitted agent profiles for this generation (primary and subagent).
    pub agents: BTreeMap<String, defs::AgentDef>,
    /// First-registration order; overrides keep their original position.
    pub(crate) agent_order: Vec<String>,
    /// Explicitly configured default agent id, if any.
    pub default_agent: Option<String>,
    /// Maximum subagent nesting depth (`experimental.subagent_depth`, default 1).
    pub subagent_depth: u32,
    /// Pinned skill source bytes, loaded once for the application generation.
    pub skills: Vec<(String, String)>,
    /// Invalid skill ids and precise generation diagnostics.
    pub skill_errors: BTreeMap<String, String>,
    /// Literal admitted command templates, including the bundled review fallback.
    pub commands: BTreeMap<String, String>,
    /// Full admitted routing metadata, pinned to this generation.
    pub command_defs: BTreeMap<String, defs::CommandDef>,
    /// Current admitted command descriptions, including the bundled fallback.
    pub command_descriptions: BTreeMap<String, String>,
    /// The review entry is the bundled fallback, not an admitted workspace definition.
    pub builtin_review: bool,
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

/// Payload-free admission cause, shared by the loader and frontend consumers.
#[derive(Debug)]
pub(crate) enum LoadFailure {
    Configuration(ServiceDiagnostic),
}

impl LoadFailure {
    fn into_detail(self) -> String {
        match self {
            Self::Configuration(diagnostic) => diagnostic.to_string(),
        }
    }
}

impl std::fmt::Display for LoadFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(diagnostic) => diagnostic.fmt(f),
        }
    }
}

fn failure(source: &str, field: &[&str], stage: ServiceStage, code: ServiceCode) -> LoadFailure {
    LoadFailure::Configuration(config::diagnostic::failure(
        source,
        field,
        stage,
        code,
        if code == ServiceCode::CapacityExceeded {
            ServiceAction::ReduceCapacity
        } else {
            ServiceAction::ReviewConfiguration
        },
    ))
}

fn invalid(source: &str, field: &[&str]) -> LoadFailure {
    failure(
        source,
        field,
        ServiceStage::Config,
        ServiceCode::InvalidConfig,
    )
}

fn document(source: &str) -> LoadFailure {
    failure(
        source,
        &["document"],
        ServiceStage::Config,
        ServiceCode::InvalidDocument,
    )
}

fn config_error(source: &str, error: &config::ConfigError) -> LoadFailure {
    LoadFailure::Configuration(config::diagnostic::from_error(source, error))
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
    load_with_env_diagnostic(project, parent_env)
        .await
        .map_err(LoadFailure::into_detail)
}

pub(crate) async fn load_with_env_diagnostic(
    project: &Path,
    parent_env: BTreeMap<String, String>,
) -> Result<Composition, LoadFailure> {
    let result = async {
        let mut composition = load_stages(project, parent_env).await?;
        composition.refresh_provider().await?;
        Ok(composition)
    }
    .await;
    match &result {
        Ok(_) => trace::log("load.ok", ""),
        Err(failure) => trace::log(
            "load.fail",
            &format!("category={}", failure_category(failure)),
        ),
    }
    result
}

/// Admit the complete local generation before any optional discovery work.
pub(crate) async fn load_local_with_env(
    project: &Path,
    parent_env: BTreeMap<String, String>,
) -> Result<Composition, LoadFailure> {
    load_stages(project, parent_env).await
}

fn failure_category(failure: &LoadFailure) -> String {
    match failure {
        LoadFailure::Configuration(_) => "Configuration".to_string(),
    }
}

fn config_class(error: &config::ConfigError) -> &'static str {
    match error {
        config::ConfigError::Invalid { .. } => "Invalid",
        config::ConfigError::UnsupportedCapability { .. } => "UnsupportedCapability",
        config::ConfigError::UnsupportedPlugin { .. } => "UnsupportedPlugin",
        config::ConfigError::Untrusted { .. } => "Untrusted",
        config::ConfigError::MissingCredential { .. } => "MissingCredential",
    }
}

struct AdmittedSources {
    project: PathBuf,
    global: Option<PathBuf>,
    roots: Vec<PathBuf>,
    admitted_roots: Vec<Option<AdmittedRoot>>,
    sources: Vec<config::Source>,
    source_authority: BTreeMap<String, (usize, PathBuf)>,
}

fn source_roots<'a>(
    roots: &'a [Option<AdmittedRoot>],
    authority: &BTreeMap<String, (usize, PathBuf)>,
) -> BTreeMap<String, (&'a File, PathBuf)> {
    authority
        .iter()
        .map(|(source, (index, directory))| {
            (
                source.clone(),
                (
                    &roots[*index].as_ref().expect("admitted source root").dir,
                    directory.clone(),
                ),
            )
        })
        .collect()
}

fn admit_sources(
    project: &Path,
    parent_env: &BTreeMap<String, String>,
) -> Result<AdmittedSources, LoadFailure> {
    let project = project.canonicalize().map_err(|_| {
        failure(
            &project.to_string_lossy(),
            &["location"],
            ServiceStage::Admission,
            ServiceCode::SourceUnavailable,
        )
    })?;
    if !project.is_dir() {
        return Err(invalid(&project.to_string_lossy(), &["location"]));
    }
    trace::log(
        "config.project",
        &format!(
            "source={}",
            config::mcp::safe_source_id(&project.to_string_lossy())
        ),
    );
    for name in ["HOME", "XDG_CONFIG_HOME", "OPENCODE_CONFIG_DIR"] {
        trace::log(
            "env.selector",
            &trace::env_fact(name, parent_env.get(name).map(String::as_str)),
        );
    }
    let nonempty_env = |key: &str| parent_env.get(key).filter(|v| !v.is_empty());
    let global = nonempty_env("OPENCODE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| nonempty_env("XDG_CONFIG_HOME").map(|p| Path::new(p).join("opencode")))
        .or_else(|| nonempty_env("HOME").map(|p| Path::new(p).join(".config/opencode")));
    match &global {
        Some(root) => trace::log(
            "config.global",
            &format!(
                "source={}",
                config::mcp::safe_source_id(&root.to_string_lossy())
            ),
        ),
        None => trace::log("config.global", "root=none"),
    }
    trace::log(
        "config.local",
        &format!(
            "source={}",
            config::mcp::safe_source_id(&project.join(".opencode").to_string_lossy())
        ),
    );
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
    let mut source_authority = BTreeMap::new();
    let mut seen = HashSet::new();
    for (index, (root, admitted)) in roots.iter().zip(&admitted_roots).enumerate() {
        let Some(admitted) = admitted else { continue };
        for name in ["opencode.json", "opencode.jsonc"] {
            let path = root.join(name);
            let (canonical, text) = match read_source_config(admitted, &path) {
                Ok(Some(source)) => source,
                Ok(None) => {
                    trace::log(
                        "source.missing",
                        &format!(
                            "source={}",
                            config::mcp::safe_source_id(&path.to_string_lossy())
                        ),
                    );
                    continue;
                }
                Err(error) => {
                    trace::log("source.parse_fail", &error.to_string());
                    return Err(error);
                }
            };
            if seen.insert(canonical.clone()) {
                let directory = canonical
                    .strip_prefix(&admitted.path)
                    .expect("admitted source")
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .to_path_buf();
                let source_path = canonical.to_string_lossy().into_owned();
                trace::log(
                    "source",
                    &format!(
                        "source={} bytes={}",
                        config::mcp::safe_source_id(&source_path),
                        text.len()
                    ),
                );
                sources.push(config::Source {
                    path: source_path,
                    text,
                    trusted: true,
                });
                source_authority
                    .insert(canonical.to_string_lossy().into_owned(), (index, directory));
            }
        }
    }
    Ok(AdmittedSources {
        project,
        global,
        roots,
        admitted_roots,
        sources,
        source_authority,
    })
}

struct DcpAdmission {
    config: dcp_auto::DcpConfig,
    warnings: Vec<String>,
    sources: Vec<String>,
}

fn admit_dcp(
    sources: &[config::Source],
    admitted_roots: &[Option<AdmittedRoot>],
) -> Result<DcpAdmission, LoadFailure> {
    // DCP config is native data, never executable plugin code. Inline `dcp`
    // fragments follow ordinary config precedence; standalone files then layer
    // at the same admitted roots (JSON before JSONC).
    let mut dcp_fragment = serde_json::json!({});
    // Every admitted source that contributed a DCP fragment: an unsupported
    // option must name the file the owner has to edit, not just the field.
    let mut dcp_sources: Vec<String> = Vec::new();
    for admitted in admitted_roots {
        let Some(admitted) = admitted else { continue };
        let root = &admitted.path;
        for source in sources {
            if Path::new(&source.path).parent() != Some(root.as_path()) {
                continue;
            }
            let value = match config::parse_jsonc(&source.text, &source.path) {
                Ok(value) => value,
                Err(error) => {
                    trace::log(
                        "source.parse_fail",
                        &format!(
                            "source={} class={}",
                            config::mcp::safe_source_id(&source.path),
                            config_class(&error)
                        ),
                    );
                    return Err(document(&source.path));
                }
            };
            if let Some(fragment) = value.get("dcp") {
                if merge_json_object(&mut dcp_fragment, fragment).is_err() {
                    trace::log("dcp.fail", &invalid(&source.path, &["dcp"]).to_string());
                    return Err(invalid(&source.path, &["dcp"]));
                }
                dcp_sources.push(source.path.clone());
            }
        }
        for name in ["dcp.json", "dcp.jsonc"] {
            let path = root.join(name);
            let text = match read_native_config(admitted, name) {
                Ok(Some(text)) => text,
                Ok(None) => continue,
                Err(error) => {
                    trace::log("dcp.fail", &error.to_string());
                    return Err(error);
                }
            };
            let value = match config::parse_jsonc(&text, &path.to_string_lossy()) {
                Ok(value) => value,
                Err(error) => {
                    trace::log(
                        "dcp.fail",
                        &format!(
                            "category={} source={}",
                            config_class(&error),
                            config::mcp::safe_source_id(&path.to_string_lossy())
                        ),
                    );
                    return Err(document(&path.to_string_lossy()));
                }
            };
            if merge_json_object(&mut dcp_fragment, &value).is_err() {
                trace::log(
                    "dcp.fail",
                    &invalid(&path.to_string_lossy(), &["dcp"]).to_string(),
                );
                return Err(invalid(&path.to_string_lossy(), &["dcp"]));
            }
            dcp_sources.push(path.display().to_string());
        }
    }
    let (dcp_config, dcp_warnings) = match dcp_auto::load_config(&dcp_fragment) {
        Ok(loaded) => loaded,
        Err(error) => {
            // A merged-field error has no typed leaf provenance. Qualify the
            // contributing source set rather than attributing it to the last file.
            let source = if dcp_sources.is_empty() {
                "native dcp".into()
            } else {
                dcp_sources.join("\0")
            };
            let mut diagnostic = config::diagnostic::failure(
                &source,
                &["dcp"],
                ServiceStage::Config,
                ServiceCode::InvalidConfig,
                ServiceAction::ReviewConfiguration,
            );
            match error {
                dcp_auto::DcpAutoError::InvalidConfig { reason } => {
                    if reason == "compress.enabled must be boolean" {
                        diagnostic.field = vec!["dcp".into(), "compress".into(), "enabled".into()];
                    }
                }
                dcp_auto::DcpAutoError::UnsupportedOption { .. } => {
                    diagnostic.stage = ServiceStage::Capability;
                    diagnostic.code = ServiceCode::UnsupportedCapability;
                }
                dcp_auto::DcpAutoError::UnsupportedPlugin { .. } => {
                    diagnostic.field = vec!["plugin".into()];
                    diagnostic.stage = ServiceStage::Capability;
                    diagnostic.code = ServiceCode::UnsupportedPlugin;
                }
            }
            let error = LoadFailure::Configuration(diagnostic);
            trace::log("dcp.fail", &error.to_string());
            return Err(error);
        }
    };
    Ok(DcpAdmission {
        config: dcp_config,
        warnings: dcp_warnings,
        sources: dcp_sources,
    })
}

struct LocalSettings {
    selected: Option<String>,
    selected_source: String,
    default_agent: Option<String>,
    subagent_depth: u32,
    enabled: Option<Vec<String>>,
    disabled: Vec<String>,
    native_modules: BTreeSet<String>,
    plugins: PluginInventory,
    plugin_issues: bool,
    conversation_keybinds: config::ConversationKeybinds,
}

fn admit_settings(
    sources: &[config::Source],
    admitted_roots: &[Option<AdmittedRoot>],
    parent_env: &BTreeMap<String, String>,
    resolve_selection: bool,
) -> Result<LocalSettings, LoadFailure> {
    let mut selected = None;
    let mut selected_source = sources
        .last()
        .map(|source| source.path.clone())
        .unwrap_or_else(|| "native config".into());
    let mut default_agent = None;
    let mut subagent_depth: u32 = 1;
    let (enabled, disabled) = catalog::provider_filters(sources)?;
    let mut native_modules = BTreeSet::new();
    let mut plugins = PluginInventory::default();
    let mut plugin_issues = false;
    let mut conversation_keybinds = config::ConversationKeybinds::default();
    for source in sources {
        let value = match config::parse_jsonc(&source.text, &source.path) {
            Ok(value) => value,
            Err(error) => {
                trace::log(
                    "source.parse_fail",
                    &format!(
                        "source={} class={}",
                        config::mcp::safe_source_id(&source.path),
                        config_class(&error)
                    ),
                );
                return Err(document(&source.path));
            }
        };
        conversation_keybinds
            .merge(&value)
            .map_err(|error| config_error(&source.path, &error))?;
        if let Some(model) = value.get("model") {
            let model = model
                .as_str()
                .ok_or_else(|| invalid(&source.path, &["model"]))?;
            // Catalog admission validates the known field's shape, but does
            // not resolve unrelated selection templates or choose a model.
            if resolve_selection {
                selected = Some(
                    config::substitute(model, &source.path, false, parent_env)
                        .map_err(|error| config_error(&source.path, &error))?,
                );
                selected_source = source.path.clone();
            }
        }
        if let Some(agent) = value.get("default_agent") {
            let agent = agent
                .as_str()
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| invalid(&source.path, &["default_agent"]))?
                .to_string();
            if resolve_selection {
                default_agent = Some(agent);
            }
        }
        if let Some(experimental) = value.get("experimental") {
            let object = experimental
                .as_object()
                .ok_or_else(|| invalid(&source.path, &["experimental"]))?;
            if let Some(depth) = object.get("subagent_depth") {
                let depth = depth
                    .as_u64()
                    .filter(|depth| *depth <= u64::from(u32::MAX))
                    .ok_or_else(|| invalid(&source.path, &["experimental", "subagent_depth"]))?;
                subagent_depth = depth as u32;
            }
        }
        if let Some(identities) = value.get("plugin") {
            let identities = provider_ids(identities, "plugin", &source.path)?;
            for (index, identity) in identities.into_iter().enumerate() {
                // Exact compiled aliases only. No plugin is opened or executed.
                let module = admitted_roots.iter().flatten().find_map(|root| {
                    config::classify_plugin(&identity, &root.path.to_string_lossy()).ok()
                });
                let (native, current, status) = match module {
                    Some("dcp") => (
                        Some(NativePlugin::Dcp),
                        Some(format!("native-dcp-{}", dcp_auto::DCP_MODULE_REVISION)),
                        PluginStatus::Active,
                    ),
                    Some("discovery") => (
                        Some(NativePlugin::OpenProxyModels),
                        Some("native-openproxy-models".to_string()),
                        PluginStatus::Active,
                    ),
                    Some("ignored-authoring-goal") => (None, None, PluginStatus::Ignored),
                    _ => (None, None, PluginStatus::Failed),
                };
                plugin_issues |= status != PluginStatus::Active;
                if let Some(native) = native {
                    native_modules.insert(module.expect("classified native module").to_string());
                    if !plugins.active_modules.contains(&native) {
                        plugins.active_modules.push(native);
                    }
                }
                let requested = config::safe_plugin_id(&identity);
                let source_id = config::mcp::safe_source_id(&source.path);
                let field = vec!["plugin".into(), index.to_string()];
                let diagnostic = (status == PluginStatus::Failed).then(|| ServiceDiagnostic {
                    kind: ServiceKind::Plugin,
                    service: requested.clone(),
                    source: source_id.clone(),
                    field: field.clone(),
                    stage: ServiceStage::Capability,
                    code: ServiceCode::UnsupportedPlugin,
                    action: ServiceAction::ReviewConfiguration,
                });
                if let Some(diagnostic) = &diagnostic {
                    trace::log("plugin.fail", &diagnostic.to_string());
                }
                // Presentation window only: later aliases still participate in
                // admission and activate their one compiled native capability.
                if plugins.entries.len() < 64 {
                    plugins.entries.push(PluginEntry {
                        requested,
                        current,
                        module: native,
                        status,
                        source: source_id,
                        field,
                        diagnostic,
                    });
                } else {
                    plugins.omitted += 1;
                    plugins.omitted_failed += usize::from(status == PluginStatus::Failed);
                }
            }
        }
    }

    Ok(LocalSettings {
        selected,
        selected_source,
        default_agent,
        subagent_depth,
        enabled,
        disabled,
        native_modules,
        plugins,
        plugin_issues,
        conversation_keybinds,
    })
}

async fn load_stages(
    project: &Path,
    parent_env: BTreeMap<String, String>,
) -> Result<Composition, LoadFailure> {
    let AdmittedSources {
        project,
        global,
        roots,
        admitted_roots,
        sources,
        source_authority,
    } = admit_sources(project, &parent_env)?;
    let source_roots = source_roots(&admitted_roots, &source_authority);
    if sources.is_empty() {
        return Err(failure(
            &project.to_string_lossy(),
            &["document"],
            ServiceStage::Config,
            ServiceCode::MissingConfiguration,
        ));
    }
    let DcpAdmission {
        config: dcp_config,
        warnings: dcp_warnings,
        sources: dcp_sources,
    } = admit_dcp(&sources, &admitted_roots)?;
    let dcp_protected = oc_core::context_plan::ProtectedSpec {
        protect_user_messages: dcp_config.protect_user_messages,
        protect_tags: dcp_config.protect_tags,
        file_globs: dcp_config.protected_file_patterns.clone(),
        protected_message_ids: BTreeSet::new(),
    };
    let LocalSettings {
        selected,
        selected_source,
        mut default_agent,
        subagent_depth,
        enabled,
        disabled,
        native_modules,
        plugins,
        plugin_issues,
        mut conversation_keybinds,
    } = admit_settings(&sources, &admitted_roots, &parent_env, true)?;
    match &selected {
        Some(_) => trace::log("selected", "configured=true"),
        None => trace::log("selected", "model=none"),
    }
    if default_agent.is_some() {
        trace::log("selected.agent", "override=true");
    }

    // Definitions are merged at their exact source precedence points: config
    // inline domains first, then Markdown from the same admitted root.
    let mut loaded_defs = defs::LoadedDefs::default();
    // Pinned builtin profiles are registered before configured transforms.
    // A real definition participates in selection, policy and durable turn
    // metadata; configured overrides/disable retain their normal precedence.
    defs::register_builtins(&mut loaded_defs);
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

    let mut instruction_roots = Vec::new();
    let mut instruction_files = Vec::new();
    for (index, origin) in [
        (0, crate::instructions::Origin::Global),
        (
            usize::from(global.is_some()),
            crate::instructions::Origin::Project,
        ),
    ] {
        if origin == crate::instructions::Origin::Global && global.is_none() {
            continue;
        }
        if let Some(root) = admitted_roots.get(index).and_then(Option::as_ref) {
            // Initial instructions are trusted config-generation inputs, not a
            // model read grant. Read once with the config/profile admission;
            // later requests must not mix in files from a failed reload.
            let file = root.path.join("AGENTS.md");
            let baseline = if let Some(admitted) = admit_instruction(&file, &root.path)? {
                let text = read_instruction(root, &admitted);
                let baseline = text.as_ref().ok().map(|text| {
                    std::sync::Arc::new(crate::instructions::Source::baseline(
                        &admitted,
                        &root.path,
                        origin.clone(),
                        text,
                    ))
                });
                instruction_files.push((file.to_string_lossy().into_owned(), text));
                baseline
            } else {
                None
            };
            instruction_roots.push(crate::instructions::Root {
                path: root.path.clone(),
                dir: std::sync::Arc::new(
                    root.dir
                        .try_clone()
                        .map_err(|_| invalid("instructions", &["root"]))?,
                ),
                origin,
                baseline,
            });
        }
    }
    let (instructions, instruction_diagnostics) = defs::load_instruction_texts(&instruction_files);

    // Resource truncation cannot establish absence or a complete profile's
    // overrides/policy. Refuse admission before any default fallback/selection.
    if let Some(diagnostic) = loaded_defs
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.failure.code == ServiceCode::CapacityExceeded)
    {
        return Err(LoadFailure::Configuration(diagnostic.failure.clone()));
    }

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
        trace::log("defs.fail", &diagnostic.failure.to_string());
        return Err(LoadFailure::Configuration(diagnostic.failure.clone()));
    }
    let agent_order: Vec<String> = loaded_defs
        .order
        .iter()
        .filter_map(|entry| {
            entry
                .strip_prefix("agent.")
                .and_then(|entry| entry.split_once('@'))
                .map(|(id, _)| id.to_string())
        })
        .filter(|id| loaded_defs.agents.contains_key(id))
        .collect();
    // A malformed/security-refused selected definition is not an absent default.
    let agent_diagnostic = |id: &str| {
        loaded_defs.diagnostics.iter().find(|diagnostic| {
            diagnostic.field == format!("agent.{id}")
                || Path::new(&diagnostic.path)
                    .file_stem()
                    .is_some_and(|stem| stem == id)
                || ["agent", "agents", "mode", "modes"].iter().any(|root| {
                    Path::new(&diagnostic.path).ends_with(Path::new(root).join(format!("{id}.md")))
                })
        })
    };
    if let Some(id) = default_agent.as_deref()
        && !loaded_defs.agents.contains_key(id)
    {
        if let Some(diagnostic) = agent_diagnostic(id) {
            return Err(LoadFailure::Configuration(diagnostic.failure.clone()));
        }
        if loaded_defs
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.failure.code == ServiceCode::TrustRefused)
        {
            return Err(invalid(&selected_source, &["default_agent"]));
        }
    }
    if default_agent
        .as_deref()
        .and_then(|id| loaded_defs.agents.get(id))
        .is_none_or(|agent| !agent.primary_visible())
    {
        default_agent = loaded_defs
            .agents
            .get("build")
            .filter(|agent| agent.primary_visible())
            .or_else(|| {
                agent_order
                    .iter()
                    .filter_map(|id| loaded_defs.agents.get(id))
                    .find(|agent| agent.primary_visible())
            })
            .map(|agent| agent.id.clone());
    }
    if default_agent.as_deref() == Some("build")
        && loaded_defs
            .agents
            .get("build")
            .is_some_and(|agent| agent.origin == "builtin")
        && let Some(diagnostic) = loaded_defs
            .diagnostics
            .iter()
            .find(|d| d.field == "agent.build" || d.field.starts_with("agent.build."))
    {
        return Err(LoadFailure::Configuration(diagnostic.failure.clone()));
    }
    let selected_agent = match default_agent.as_deref() {
        Some(id) => match loaded_defs.agents.get(id) {
            Some(agent) if !agent.primary_capable() => {
                trace::log("defs.fail", "category=Agent code=invalid_definition");
                return Err(invalid(&agent.origin, &["default_agent", "mode"]));
            }
            Some(agent) => Some(agent.clone()),
            None => {
                let diagnostic = agent_diagnostic(id);
                trace::log("defs.fail", "category=Agent code=invalid_definition");
                return Err(match diagnostic {
                    Some(diagnostic) => LoadFailure::Configuration(diagnostic.failure.clone()),
                    None => invalid(&selected_source, &["default_agent"]),
                });
            }
        },
        None => return Err(invalid(&selected_source, &["default_agent"])),
    };

    let selected = match selected_agent
        .as_ref()
        .and_then(|agent| agent.model.clone())
        .or(selected)
    {
        Some(selected) => selected,
        None => {
            return Err(failure(
                &selected_source,
                &["model"],
                ServiceStage::Config,
                ServiceCode::MissingConfiguration,
            ));
        }
    };
    let (provider_id, model_id, embedded_variant) =
        models::parse_reference(&selected).map_err(|_| invalid(&selected_source, &["model"]))?;
    let selected_variant = embedded_variant.map(str::to_string).or_else(|| {
        selected_agent
            .as_ref()
            .and_then(|agent| agent.variant.clone())
    });
    if disabled.iter().any(|id| id == provider_id)
        || enabled
            .as_ref()
            .is_some_and(|ids| !ids.iter().any(|id| id == provider_id))
    {
        return Err(invalid(&selected_source, &["enabled_providers"]));
    }
    let selected_providers = HashSet::from([provider_id.to_string()]);
    trace::log("provider.selected", "configured=true");
    let (mut generation, terminal_copy) = config::assemble_admitted_with_terminal_copy(
        &sources,
        &parent_env,
        Some(&selected_providers),
        &source_roots,
    )
    .map_err(LoadFailure::Configuration)?;
    generation.providers.retain(|id, _| {
        !disabled.contains(id) && enabled.as_ref().is_none_or(|ids| ids.contains(id))
    });
    admit_local_mcp(
        &mut generation,
        &project,
        global.as_deref(),
        &parent_env,
        &sources,
    )?;
    // Keep central authority independent of the startup primary selection.
    for tool in [
        "opencode_models",
        "opencode_session_rename",
        "opencode_session_move",
    ] {
        generation
            .permission_rules
            .module_permission(tool, config::Permission::Allow);
        generation
            .permissions
            .entry(tool.into())
            .or_insert(config::Permission::Allow);
    }
    generation
        .permission_rules
        .module_permission("question", config::Permission::Allow);
    generation
        .permissions
        .entry("question".into())
        .or_insert(config::Permission::Allow);
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
    let entry = generation
        .providers
        .get(provider_id)
        .ok_or_else(|| invalid(&selected_source, &["model", "provider"]))?;
    let provider_source = generation
        .provenance
        .get(&format!("provider.{provider_id}"))
        .expect("provider provenance");
    let provider = provider::ResponsesConfig {
        headers: entry.options.headers.clone(),
        set_cache_key: entry.options.set_cache_key.unwrap_or(false),
        wire: config::provider_wire(provider_id, entry)
            .map_err(|_| invalid(&selected_source, &["model", "provider"]))?,
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
    let url = reqwest::Url::parse(&provider.base_url).map_err(|_| {
        invalid(
            provider_source,
            &["provider", "entry", "options", "baseURL"],
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            provider_source,
            &["provider", "entry", "options", "baseURL"],
        ));
    }
    // Mandatory transport shape/security remains admission, even with no key.
    let mut header_validation = provider.clone();
    if header_validation.wire.auth_policy == crate::auth::AuthPolicy::OAuth {
        header_validation.wire.auth_policy = crate::auth::AuthPolicy::Key;
    }
    provider::request_headers(&header_validation).map_err(|_| {
        failure(
            provider_source,
            &["provider", "entry", "options", "headers"],
            ServiceStage::Config,
            ServiceCode::InvalidHeader,
        )
    })?;
    trace::log(
        "provider.base_url",
        &format!("scheme={} configured=true", url.scheme()),
    );
    trace::log(
        "provider.headers",
        &format!("configured={}", entry.options.headers.len()),
    );
    let catalog = models::ModelCatalog {
        provider: provider_id.to_string(),
        models: entry.models.clone(),
    };
    if provider_id != discovery::PROVIDER_ID && provider_id != crate::models_dev::PROVIDER {
        models::select_model(&catalog, model_id).map_err(|_| {
            failure(
                &selected_source,
                &["model"],
                ServiceStage::Admission,
                ServiceCode::ModelUnavailable,
            )
        })?;
    }
    let provider_state = ProviderState::new(
        provider_id,
        generation
            .provenance
            .get(&format!("provider.{provider_id}"))
            .map(String::as_str)
            .unwrap_or("native config"),
        provider.auth_ready(),
        provider_id == discovery::PROVIDER_ID
            && discovery::should_run(&disabled, enabled.as_deref()),
    );
    let provider_state = provider_state.with_auth_policy(provider.wire.auth_policy);
    let skills = loaded_defs
        .skills
        .values()
        .map(|skill| (skill.id.clone(), skill.body.clone()))
        .collect();
    let mut commands: BTreeMap<String, String> = loaded_defs
        .commands
        .values()
        .map(|command| (command.id.clone(), command.body.clone()))
        .collect();
    let mut command_descriptions: BTreeMap<String, String> = loaded_defs
        .commands
        .values()
        .map(|command| (command.id.clone(), command.description.clone()))
        .collect();
    let builtin_review = !commands.contains_key("review");
    if builtin_review {
        commands.insert(
            "review".into(),
            include_str!("../assets/upstream/v2/review.txt").into(),
        );
        command_descriptions.insert(
            "review".into(),
            "review changes [commit|branch|pr], defaults to uncommitted".into(),
        );
    }
    let mut startup_notices = Vec::new();
    if !generation.config_diagnostics.is_empty() {
        startup_notices.push(StartupNotice::CompactionConfig);
    }
    if generation.mcp.values().any(|entry| entry.failure.is_some()) {
        startup_notices.push(StartupNotice::McpConfig);
    }
    if !loaded_defs.diagnostics.is_empty() {
        startup_notices.push(StartupNotice::Definitions);
    }
    if plugin_issues {
        startup_notices.push(StartupNotice::Plugin);
    }
    if !dcp_warnings.is_empty() {
        startup_notices.push(StartupNotice::Dcp);
    }
    if !instruction_diagnostics.is_empty() {
        startup_notices.push(StartupNotice::Instructions);
    }
    // This owner returns only ignored optional-setting notes. No warning prose
    // is classified or retained in the public diagnostic projection.
    let dcp_source = if dcp_sources.is_empty() {
        "native dcp".into()
    } else {
        dcp_sources.join("\0")
    };
    let dcp_notes: Vec<_> = dcp_warnings
        .iter()
        .take(64)
        .map(|warning| {
            config::diagnostic::failure(
                &dcp_source,
                if warning == "unknown dcp key: compress.entry" {
                    &["dcp", "compress", "entry"]
                } else {
                    &["dcp"]
                },
                ServiceStage::Config,
                ServiceCode::IgnoredSetting,
                ServiceAction::ReviewConfiguration,
            )
        })
        .collect();
    let config_notes: Vec<_> = generation
        .config_diagnostics
        .iter()
        .take(64)
        .map(|note| {
            let code = if note.kind == oc_core::queries::ConfigDiagnosticKind::Invalid {
                ServiceCode::InvalidConfig
            } else {
                ServiceCode::IgnoredSetting
            };
            let mut diagnostic = config::diagnostic::failure(
                "native compaction",
                &[],
                ServiceStage::Config,
                code,
                ServiceAction::ReviewConfiguration,
            );
            diagnostic.source = note.source.clone();
            diagnostic.field = note.field.clone();
            diagnostic
        })
        .collect();
    let mut diagnostics: Vec<String> = loaded_defs
        .diagnostics
        .iter()
        .take(64)
        .map(|diagnostic| diagnostic.failure.to_string())
        .collect();
    diagnostics.extend(plugins.entries.iter().map(ToString::to_string));
    if plugins.omitted > 0 {
        diagnostics.push(format!(
            "plugin inventory: {} additional requests omitted from presentation ({} failed/unsupported_plugin); active modules: {:?}",
            plugins.omitted, plugins.omitted_failed, plugins.active_modules
        ));
    }
    diagnostics.extend(
        generation
            .mcp
            .values()
            .filter_map(|entry| entry.failure.as_ref())
            .take(64)
            .map(ToString::to_string),
    );
    diagnostics.extend(
        config_notes
            .iter()
            .zip(&generation.config_diagnostics)
            .map(|(diagnostic, note)| format!("{diagnostic}; {}", note.message())),
    );
    diagnostics.extend(dcp_notes.iter().map(ToString::to_string));
    diagnostics.extend(
        instruction_diagnostics
            .iter()
            .take(64)
            .map(|diagnostic| diagnostic.failure.to_string()),
    );
    let service_diagnostics_omitted = loaded_defs.diagnostics.len().saturating_sub(64)
        + instruction_diagnostics.len().saturating_sub(64)
        + generation.config_diagnostics.len().saturating_sub(64)
        + dcp_warnings.len().saturating_sub(64)
        + generation
            .mcp
            .values()
            .filter(|entry| entry.failure.is_some())
            .count()
            .saturating_sub(64);
    if service_diagnostics_omitted > 0 {
        diagnostics.push(format!("diagnostic inventory: {service_diagnostics_omitted} additional failures/notes omitted from presentation"));
    }
    let mut skill_errors = BTreeMap::new();
    for diagnostic in &loaded_defs.diagnostics {
        if diagnostic.field == "skill"
            && let Some(id) = skill_diagnostic_id(&diagnostic.path)
        {
            skill_errors.insert(id.to_string(), diagnostic.failure.to_string());
        }
    }
    // A missing global directory is not an admitted write boundary. Fall back
    // to the already admitted Location instead of creating external ancestors.
    let default_root = if global.is_some() && admitted_roots[0].is_some() {
        0
    } else {
        usize::from(global.is_some())
    };
    let admitted = admitted_roots[default_root].as_ref().ok_or_else(|| {
        failure(
            &project.to_string_lossy(),
            &["location"],
            ServiceStage::Admission,
            ServiceCode::SourceUnavailable,
        )
    })?;
    let mut permission_mode_source = roots[default_root].join("cli.json");
    let mut permission_mode_root = AdmittedRoot {
        path: admitted.path.clone(),
        dir: admitted.dir.try_clone().map_err(|_| {
            failure(
                &admitted.path.to_string_lossy(),
                &["root"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            )
        })?,
    };
    let mut tui_chrome = oc_core::queries::TuiChrome {
        dcp: oc_core::dcp_view::DcpDisplayConfig {
            notification: dcp_config.prune_notification,
            channel: if dcp_config.prune_notification_type == "toast" {
                oc_core::dcp_view::DcpNotificationChannel::Toast
            } else {
                oc_core::dcp_view::DcpNotificationChannel::Chat
            },
            show_compression: dcp_config.show_compression,
            commands_enabled: dcp_config.commands_enabled,
        },
        config_diagnostics: generation
            .config_diagnostics
            .iter()
            .take(64)
            .cloned()
            .collect(),
        provider: Some(provider_state.for_model(model_id, catalog.models.contains_key(model_id))),
        service_diagnostics: generation
            .mcp
            .values()
            .filter_map(|entry| entry.failure.clone())
            .take(64)
            .chain(
                plugins
                    .entries
                    .iter()
                    .filter_map(|entry| entry.diagnostic.clone()),
            )
            .chain(
                loaded_defs
                    .diagnostics
                    .iter()
                    .take(64)
                    .map(|diagnostic| diagnostic.failure.clone()),
            )
            .chain(
                instruction_diagnostics
                    .iter()
                    .take(64)
                    .map(|diagnostic| diagnostic.failure.clone()),
            )
            .chain(dcp_notes)
            .chain(config_notes)
            .collect(),
        service_diagnostics_omitted,
        plugins,
        location: Some(project.to_string_lossy().into_owned()),
        terminal_copy,
        animations: generation.animations,
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
                .map_err(|_| document(&root.join(name).to_string_lossy()))?;
            conversation_keybinds
                .merge(&value)
                .map_err(|error| config_error(&root.join(name).to_string_lossy(), &error))?;
            if let Some(v) = value.pointer("/debug/devtools") {
                tui_chrome.devtools = Some(v.as_bool().ok_or_else(|| {
                    invalid(&root.join(name).to_string_lossy(), &["debug", "devtools"])
                })?);
            }
            if let Some(v) = value.pointer("/session/sidebar") {
                tui_chrome.sidebar_hidden = match v.as_str() {
                    Some("auto") => false,
                    Some("hide") => true,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["session", "sidebar"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/session/tps") {
                tui_chrome.session_tps = Some(v.as_bool().ok_or_else(|| {
                    invalid(&root.join(name).to_string_lossy(), &["session", "tps"])
                })?);
            }
            if let Some(v) = value.pointer("/session/permissions") {
                permission_mode_source = root.join(name);
                permission_mode_root = AdmittedRoot {
                    path: admitted.path.clone(),
                    dir: admitted.dir.try_clone().map_err(|_| {
                        failure(
                            &root.join(name).to_string_lossy(),
                            &["root"],
                            ServiceStage::Admission,
                            ServiceCode::SourceUnavailable,
                        )
                    })?,
                };
                tui_chrome.permissions_auto = match v.as_str() {
                    Some("prompt") => false,
                    Some("autoaccept") => true,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["session", "permissions"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/diffs/view") {
                tui_chrome.diffs.view = match v.as_str() {
                    Some("auto") => oc_core::queries::DiffView::Auto,
                    Some("unified") => oc_core::queries::DiffView::Unified,
                    Some("split") => oc_core::queries::DiffView::Split,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["diffs", "view"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/diffs/wrap") {
                tui_chrome.diffs.wrap = match v.as_str() {
                    Some("word") => oc_core::queries::DiffWrap::Word,
                    Some("none") => oc_core::queries::DiffWrap::None,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["diffs", "wrap"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/tabs/layout") {
                tui_chrome.vertical_tabs_width = match v.as_str() {
                    Some("horizontal") => 0,
                    Some("vertical") => 42,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["tabs", "layout"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/tabs/scope") {
                tui_chrome.sessions_all_projects = match v.as_str() {
                    Some("global") => true,
                    Some("cwd") => false,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["tabs", "scope"],
                        ));
                    }
                };
            }
            if let Some(v) = value.pointer("/tabs/indicators") {
                tui_chrome.tab_indicators = match v.as_str() {
                    Some("status") => oc_core::queries::TabIndicators::Status,
                    Some("numbers") => oc_core::queries::TabIndicators::Numbers,
                    _ => {
                        return Err(invalid(
                            &root.join(name).to_string_lossy(),
                            &["tabs", "indicators"],
                        ));
                    }
                };
            }
        }
    }
    tui_chrome.permission_shortcuts = conversation_keybinds.permission_shortcuts();
    tui_chrome.leader_timeout_ms = conversation_keybinds.leader_timeout_ms();
    tui_chrome.command_palette_shortcut = Some(conversation_keybinds.command_palette_shortcut());
    tui_chrome.conversation_shortcuts = conversation_keybinds.resolve();
    Ok(Composition {
        mcp_activation: std::sync::Arc::new(McpActivation {
            sources: sources.clone(),
            global: global.as_ref().and_then(|root| root.canonicalize().ok()),
            roots: source_roots
                .iter()
                .map(|(source, (root, directory))| {
                    root.try_clone()
                        .map(|root| (source.clone(), (root, directory.clone())))
                })
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    failure(
                        &project.to_string_lossy(),
                        &["mcp", "source"],
                        ServiceStage::Admission,
                        ServiceCode::SourceUnavailable,
                    )
                })?,
        }),
        approval_consumer_mode: std::sync::atomic::AtomicU8::new(0),
        question_consumer: AtomicBool::new(false),
        permission_preference: AtomicBool::new(tui_chrome.permissions_auto),
        permission_mode_source,
        permission_mode_root,
        tui_chrome,
        generation,
        catalog,
        model_id: model_id.to_string(),
        provider,
        provider_state,
        go_catalog: None,
        project: project.clone(),
        parent_env,
        instructions,
        instruction_roots,
        agent_prompt: selected_agent.as_ref().map(|agent| agent.body.clone()),
        agent_digest: selected_agent.as_ref().map(defs::agent_digest),
        variant: selected_variant,
        agent_order,
        agents: loaded_defs.agents,
        default_agent,
        subagent_depth,
        skills,
        skill_errors,
        commands,
        command_defs: loaded_defs.commands,
        command_descriptions,
        builtin_review,
        native_modules,
        diagnostics,
        startup_notices,
        dcp_config,
        dcp_protected,
    })
}

/// Admit process resources and credential authority before inheriting any env.
/// A Location source does not gain global credential authority through precedence.
fn admit_local_mcp(
    generation: &mut config::Generation,
    project: &Path,
    global: Option<&Path>,
    env: &BTreeMap<String, String>,
    sources: &[config::Source],
) -> Result<(), LoadFailure> {
    let global = global.and_then(|root| root.canonicalize().ok());
    let global_source = |source: &str| {
        global
            .as_deref()
            .is_some_and(|root| Path::new(source).parent() == Some(root))
    };
    let mut global_credentials = Vec::new();
    for source in sources.iter().filter(|source| global_source(&source.path)) {
        global_credentials.extend(
            config::mcp::source_credential_values(source, env)
                .map_err(|error| config_error(&source.path, &error))?,
        );
    }
    for (id, provider) in &generation.providers {
        if generation
            .provenance
            .get(&format!("provider.{id}"))
            .is_some_and(|source| global_source(source))
        {
            global_credentials.push(provider.options.api_key.clone());
            global_credentials.extend(provider.options.headers.values().cloned());
        }
    }
    for (id, entry) in &generation.mcp {
        if entry.enabled
            && entry.failure.is_none()
            && generation
                .provenance
                .get(&format!("mcp.{id}"))
                .is_some_and(|source| global_source(source))
        {
            global_credentials.extend(entry.headers.values().cloned());
            global_credentials.extend(
                entry
                    .headers
                    .values()
                    .filter_map(|v| v.strip_prefix("Bearer ").map(str::to_string)),
            );
            global_credentials.extend(entry.environment.values().cloned());
        }
    }
    for (id, entry) in &mut generation.mcp {
        if entry.kind != "local" || !entry.enabled || entry.failure.is_some() {
            continue;
        }
        let source = &generation.provenance[&format!("mcp.{id}")];
        entry.inherit_credentials = global_source(source);
        if !entry.inherit_credentials {
            entry.blocked_inherited_values = global_credentials.clone();
            entry.blocked_inherited_values.extend(
                env.iter()
                    .filter(|(name, _)| crate::mcp_stdio::is_credential_name(name))
                    .map(|(_, value)| value.clone()),
            );
            if entry
                .command
                .iter()
                .chain(entry.environment.values())
                .chain(entry.cwd.iter())
                .any(|value| {
                    entry
                        .blocked_inherited_values
                        .iter()
                        .any(|secret| !secret.is_empty() && value.contains(secret))
                })
            {
                return Err(failure(
                    source,
                    &["mcp", "entry", "command"],
                    ServiceStage::Admission,
                    ServiceCode::TrustRefused,
                ));
            }
        }
        let configured = entry.cwd.as_deref().unwrap_or(".");
        // No implicit escape or fallback. External cwd requires the same explicit
        // external_directory authority as other resources, not merely a global source.
        let candidate = project.join(configured);
        let cwd = match candidate.canonicalize() {
            Ok(path) if path.is_dir() => path,
            _ => {
                let mut issue = config::mcp::failure(
                    id,
                    source,
                    "cwd",
                    oc_core::queries::ServiceCode::InvalidCwd,
                );
                issue.stage = oc_core::queries::ServiceStage::Admission;
                entry.failure = Some(issue);
                continue;
            }
        };
        if !cwd.starts_with(project) {
            let effect = generation.permission_rules.evaluate_actions(
                &generation.permissions,
                &["external_directory"],
                &cwd.to_string_lossy(),
            );
            if effect != config::Permission::Allow {
                return Err(failure(
                    source,
                    &["mcp", "entry", "cwd"],
                    ServiceStage::Admission,
                    ServiceCode::TrustRefused,
                ));
            }
        }
        entry.cwd = Some(cwd.to_string_lossy().into_owned());
        entry.resource_admitted = true;
    }
    Ok(())
}

/// Root descriptors and original bytes, not a fresh config read on an action.
pub(crate) struct McpActivation {
    sources: Vec<config::Source>,
    global: Option<PathBuf>,
    roots: BTreeMap<String, (File, PathBuf)>,
}

impl McpActivation {
    pub(crate) fn activate(
        &self,
        generation: &config::Generation,
        project: &Path,
        env: &BTreeMap<String, String>,
        id: &str,
    ) -> Result<config::McpEntry, String> {
        let mut generation = generation.clone();
        let entry = generation.mcp.get_mut(id).ok_or("unknown MCP server")?;
        // Malformed/unsupported input never becomes executable by toggling it.
        if entry.failure.is_some() {
            return Ok(entry.clone());
        }
        if !entry.enabled {
            let source = generation
                .provenance
                .get(&format!("mcp.{id}"))
                .ok_or("MCP source authority unavailable")?;
            let trusted = self.sources.iter().any(|s| &s.path == source && s.trusted);
            config::activate_mcp_entry(id, source, trusted, entry, env, &|path, source| {
                let (root, directory) =
                    self.roots
                        .get(source)
                        .ok_or_else(|| config::ConfigError::Untrusted {
                            origin: source.into(),
                            reason: "MCP source authority unavailable".into(),
                        })?;
                config::read_trusted_file_rooted(path, source, root, directory)
            })
            .map_err(|_| "MCP activation admission refused")?;
        }
        // Revalidate resources/credential domains on every new connection,
        // including retries; the immutable source/provenance is unchanged.
        admit_local_mcp(
            &mut generation,
            project,
            self.global.as_deref(),
            env,
            &self.sources,
        )
        .map_err(|error| error.to_string())?;
        Ok(generation.mcp.remove(id).expect("activated entry"))
    }
}

#[cfg(test)]
#[path = "composition/mcp_tests.rs"]
mod mcp_tests;
#[cfg(test)]
#[path = "composition/plugin_tests.rs"]
mod plugin_tests;

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

fn admit_root(
    root: &Path,
    project: &Path,
    local: bool,
) -> Result<Option<AdmittedRoot>, LoadFailure> {
    let canonical = match root.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                &root.to_string_lossy(),
                &["root"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            ));
        }
    };
    if local && !canonical.starts_with(project) {
        return Err(failure(
            &root.to_string_lossy(),
            &["root"],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        ));
    }
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&canonical)
        .map_err(|_| {
            failure(
                &root.to_string_lossy(),
                &["root"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            )
        })?;
    // If an ancestor was swapped during admission, the opened directory, not
    // the earlier pathname resolution, decides whether it is still admitted.
    let opened =
        std::fs::canonicalize(format!("/proc/self/fd/{}", dir.as_raw_fd())).map_err(|_| {
            failure(
                &root.to_string_lossy(),
                &["root"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            )
        })?;
    if opened != canonical || (local && !opened.starts_with(project)) {
        return Err(failure(
            &root.to_string_lossy(),
            &["root"],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        ));
    }
    Ok(Some(AdmittedRoot {
        path: canonical,
        dir,
    }))
}

fn read_bounded(mut file: File, cap: usize) -> Result<String, ServiceCode> {
    let metadata = file
        .metadata()
        .map_err(|_| ServiceCode::SourceUnavailable)?;
    if !metadata.is_file() {
        return Err(ServiceCode::InvalidDocument);
    }
    if metadata.len() > cap as u64 {
        return Err(ServiceCode::CapacityExceeded);
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ServiceCode::SourceUnavailable)?;
    if bytes.len() > cap {
        return Err(ServiceCode::CapacityExceeded);
    }
    String::from_utf8(bytes).map_err(|_| ServiceCode::InvalidDocument)
}

fn read_source_config(
    root: &AdmittedRoot,
    path: &Path,
) -> Result<Option<(PathBuf, String)>, LoadFailure> {
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                &path.to_string_lossy(),
                &["document"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            ));
        }
    };
    let relative = canonical.strip_prefix(&root.path).map_err(|_| {
        failure(
            &path.to_string_lossy(),
            &["document"],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        )
    })?;
    let file = admitted_fs::open_beneath(&root.dir, relative, libc::O_RDONLY).map_err(|_| {
        failure(
            &path.to_string_lossy(),
            &["document"],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        )
    })?;
    let text = read_bounded(file, SOURCE_CONFIG_CAP).map_err(|code| {
        failure(
            &path.to_string_lossy(),
            &["document"],
            ServiceStage::Config,
            code,
        )
    })?;
    Ok(Some((canonical, text)))
}

fn read_native_config(root: &AdmittedRoot, name: &str) -> Result<Option<String>, LoadFailure> {
    let file = match admitted_fs::open_beneath(&root.dir, Path::new(name), libc::O_RDONLY) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                &root.path.join(name).to_string_lossy(),
                &["document"],
                ServiceStage::Admission,
                ServiceCode::TrustRefused,
            ));
        }
    };
    read_bounded(file, NATIVE_CONFIG_CAP)
        .map(Some)
        .map_err(|code| {
            failure(
                &root.path.join(name).to_string_lossy(),
                &["document"],
                ServiceStage::Config,
                code,
            )
        })
}

/// Persist one supported Settings value through the same admitted config boundary.
pub(crate) fn save_permission_mode(
    composition: &Composition,
    auto_once: bool,
) -> Result<(), String> {
    use std::io::Write as _;
    let path = &composition.permission_mode_source;
    let parent = path.parent().ok_or("missing configuration parent")?;
    let root = &composition.permission_mode_root;
    let opened = std::fs::canonicalize(format!("/proc/self/fd/{}", root.dir.as_raw_fd()))
        .map_err(|e| e.to_string())?;
    if opened != root.path || parent.canonicalize().map_err(|e| e.to_string())? != root.path {
        return Err("admitted configuration directory changed; reload before saving".into());
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid configuration name")?;
    let original = read_native_config(root, name).map_err(|error| error.to_string())?;
    let bytes = crate::cli_permissions::update(original.as_deref().unwrap_or("{}\n"), auto_once)?
        .into_bytes();
    if bytes.len() > NATIVE_CONFIG_CAP {
        return Err("CLI settings exceed config budget".into());
    }
    let pinned = PathBuf::from(format!("/proc/self/fd/{}", root.dir.as_raw_fd()));
    let temporary = pinned.join(format!(
        ".cli-permissions-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        if read_native_config(root, name).map_err(|error| error.to_string())? != original {
            return Err("CLI settings changed while saving; retry".into());
        }
        std::fs::rename(&temporary, pinned.join(name)).map_err(|e| e.to_string())?;
        root.dir.sync_all().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

fn read_instruction(root: &AdmittedRoot, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(&root.path)
        .map_err(|_| "outside admitted root".to_string())?;
    let mut file = admitted_fs::open_beneath_no_symlinks(&root.dir, relative, libc::O_RDONLY)
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
fn admit_instruction(file: &Path, root: &Path) -> Result<Option<PathBuf>, LoadFailure> {
    let canonical_root = match root.canonicalize() {
        Ok(canonical) => canonical,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                &root.to_string_lossy(),
                &["instructions"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            ));
        }
    };
    let canonical = match file.canonicalize() {
        Ok(canonical) => canonical,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(failure(
                &file.to_string_lossy(),
                &["instructions"],
                ServiceStage::Admission,
                ServiceCode::SourceUnavailable,
            ));
        }
    };
    if !canonical.starts_with(&canonical_root) {
        return Err(failure(
            &file.to_string_lossy(),
            &["instructions"],
            ServiceStage::Admission,
            ServiceCode::TrustRefused,
        ));
    }
    Ok(Some(canonical))
}

fn merge_config_sources(
    definitions: &mut defs::LoadedDefs,
    sources: &[config::Source],
    parent: &Path,
) -> Result<(), LoadFailure> {
    for source in sources {
        if Path::new(&source.path).parent() == Some(parent) {
            let value = config::parse_jsonc(&source.text, &source.path)
                .map_err(|_| document(&source.path))?;
            defs::merge_config_definitions(definitions, &value, &source.path);
        }
    }
    Ok(())
}

fn provider_ids(
    value: &serde_json::Value,
    field: &str,
    source: &str,
) -> Result<Vec<String>, LoadFailure> {
    serde_json::from_value(value.clone()).map_err(|_| invalid(source, &[field]))
}

#[cfg(test)]
mod tests {
    use super::load_with_env;
    use oc_core::queries::TabIndicators;
    use std::collections::BTreeMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn bundled_review_is_a_fallback_to_admitted_workspace_definition() {
        let dir = tempfile::tempdir().expect("fixture");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).expect("project");
        let config = serde_json::json!({
            "model": "fixture/main",
            "provider": {"fixture": {"options": {
                "baseURL": "https://example.invalid/v1", "apiKey": "fixture-key"
            }, "models": {"main": {}}}}
        });
        std::fs::write(project.join("opencode.json"), config.to_string()).expect("config");
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            dir.path().join("config").to_string_lossy().into_owned(),
        )]);
        let fallback = load_with_env(&project, env.clone())
            .await
            .expect("fallback");
        assert_eq!(fallback.project, project.canonicalize().expect("canonical"));
        assert!(fallback.builtin_review);
        assert_eq!(fallback.commands.len(), 1);
        assert_eq!(
            fallback.commands["review"],
            include_str!("../assets/upstream/v2/review.txt")
        );
        assert_eq!(fallback.command_descriptions.len(), 1);
        assert_eq!(
            fallback.command_descriptions["review"],
            "review changes [commit|branch|pr], defaults to uncommitted"
        );

        let mut override_config = config;
        override_config["command"] = serde_json::json!({"review": {
            "template": "workspace $1 / $ARGUMENTS",
            "description": "workspace review"
        }});
        std::fs::write(project.join("opencode.json"), override_config.to_string())
            .expect("override");
        let overridden = load_with_env(&project, env)
            .await
            .expect("workspace override");
        assert!(!overridden.builtin_review);
        assert_eq!(overridden.commands["review"], "workspace $1 / $ARGUMENTS");
        assert_eq!(
            overridden.command_descriptions["review"],
            "workspace review"
        );
    }

    #[tokio::test]
    async fn tab_indicators_default_ordered_overrides_and_invalid_value() {
        let dir = tempfile::tempdir().expect("fixture");
        let global = dir.path().join("config/opencode");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&global).expect("global");
        std::fs::create_dir_all(project.join(".opencode")).expect("local");
        std::fs::write(project.join("opencode.json"), r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{}}}}}"#).expect("config");
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            dir.path().join("config").to_string_lossy().into_owned(),
        )]);
        let load = || load_with_env(&project, env.clone());
        assert_eq!(
            load().await.expect("default").tui_chrome.tab_indicators,
            TabIndicators::Status
        );

        std::fs::write(
            global.join("cli.json"),
            r#"{"tabs":{"indicators":"numbers"}}"#,
        )
        .expect("global cli");
        assert_eq!(
            load()
                .await
                .expect("global numbers")
                .tui_chrome
                .tab_indicators,
            TabIndicators::Numbers
        );
        std::fs::write(
            project.join(".opencode/cli.jsonc"),
            "{ // local override\n \"tabs\": {\"indicators\": \"status\"},}",
        )
        .expect("local cli");
        assert_eq!(
            load()
                .await
                .expect("local status")
                .tui_chrome
                .tab_indicators,
            TabIndicators::Status
        );
        for invalid in ["\"dots\"", "42", "null"] {
            std::fs::write(
                project.join(".opencode/cli.jsonc"),
                format!("{{\"tabs\":{{\"indicators\":{invalid}}}}}"),
            )
            .expect("invalid cli");
            let error = load().await.map(|_| ()).expect_err("invalid indicators");
            // A path-derived opaque source hash can legitimately contain `42`;
            // require the entire safe diagnostic shape, with no raw value slot.
            assert_eq!(
                error,
                format!(
                    "configuration native config: invalid_config (retryable=false); {} tabs.indicators: review configuration",
                    crate::config::mcp::safe_source_id(
                        &project.join(".opencode/cli.jsonc").to_string_lossy()
                    )
                )
            );
        }
    }

    #[tokio::test]
    async fn builtin_build_selection_and_admitted_session_tps() {
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("config/opencode");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        let mut config = serde_json::json!({"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{},"other":{}}}}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            dir.path().join("config").to_string_lossy().into_owned(),
        )]);
        let load = || load_with_env(&project, env.clone());
        let base = load().await.unwrap();
        assert_eq!(base.default_agent.as_deref(), Some("build"));
        assert_eq!(base.agents["build"], crate::defs::builtin_build());
        assert!(base.tui_chrome.session_tps.unwrap_or(true));
        std::fs::write(global.join("cli.json"), "{\"session\":{\"tps\":false}}").unwrap();
        assert_eq!(load().await.unwrap().tui_chrome.session_tps, Some(false));
        std::fs::write(
            project.join("cli.jsonc"),
            "{ // override\n \"session\":{\"tps\":true}}",
        )
        .unwrap();
        assert_eq!(load().await.unwrap().tui_chrome.session_tps, Some(true));
        for invalid in ["null", "42", "\"false\""] {
            std::fs::write(
                project.join("cli.jsonc"),
                format!("{{\"session\":{{\"tps\":{invalid}}}}}"),
            )
            .unwrap();
            assert!(
                load()
                    .await
                    .err()
                    .unwrap()
                    .contains("session.tps: review configuration")
            );
        }
        std::fs::remove_file(project.join("cli.jsonc")).unwrap();
        config["agent"] = serde_json::json!({"build":{"prompt":"configured Build","model":"fixture/other","permission":{"bash":"deny"}}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let configured = load().await.unwrap();
        assert_eq!(configured.default_agent.as_deref(), Some("build"));
        assert_eq!(configured.model_id, "other");
        assert_eq!(configured.agent_prompt.as_deref(), Some("configured Build"));
        assert!(!configured.agents["build"].subagent_capable());
        assert_eq!(
            configured.agents["build"].permissions["bash"],
            crate::config::Permission::Deny
        );
        config["agent"] = serde_json::json!({"build":{"disabled":true},"review":{"mode":"primary","prompt":"review"}});
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let disabled = load().await.unwrap();
        assert!(!disabled.agents.contains_key("build"));
        assert_eq!(disabled.default_agent.as_deref(), Some("plan"));
        config["default_agent"] = serde_json::Value::Null;
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        assert!(
            load()
                .await
                .err()
                .unwrap()
                .contains("default_agent: review configuration")
        );
    }

    #[tokio::test]
    async fn vis35_admitted_diff_settings_defaults_override_and_validation() {
        use oc_core::queries::{DiffSettings, DiffView, DiffWrap};
        let dir = tempfile::tempdir().unwrap();
        let global = dir.path().join("config/opencode");
        let project = dir.path().join("project");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("opencode.json"), r#"{"model":"fixture/main","provider":{"fixture":{"options":{"baseURL":"https://example.invalid/v1","apiKey":"k"},"models":{"main":{}}}}}"#).unwrap();
        let env = BTreeMap::from([(
            "XDG_CONFIG_HOME".into(),
            dir.path().join("config").to_string_lossy().into_owned(),
        )]);
        let load = || load_with_env(&project, env.clone());
        assert_eq!(
            load().await.unwrap().tui_chrome.diffs,
            DiffSettings::default()
        );
        std::fs::write(
            global.join("cli.json"),
            r#"{"diffs":{"view":"split","wrap":"none"}}"#,
        )
        .unwrap();
        assert_eq!(
            load().await.unwrap().tui_chrome.diffs,
            DiffSettings {
                view: DiffView::Split,
                wrap: DiffWrap::None
            }
        );
        std::fs::write(
            project.join("cli.jsonc"),
            "{ // project\n \"diffs\":{\"view\":\"unified\",\"wrap\":\"word\"}}",
        )
        .unwrap();
        assert_eq!(
            load().await.unwrap().tui_chrome.diffs,
            DiffSettings {
                view: DiffView::Unified,
                wrap: DiffWrap::Word
            }
        );
        for field in ["view", "wrap"] {
            std::fs::write(
                project.join("cli.jsonc"),
                format!("{{\"diffs\":{{\"{field}\":\"invalid\"}}}}"),
            )
            .unwrap();
            assert!(
                load()
                    .await
                    .err()
                    .unwrap()
                    .contains(&format!("diffs.{field}: review configuration"))
            );
        }
    }

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
            error.contains("trust_refused")
                && error.contains(&crate::config::mcp::safe_source_id(
                    &project.join(".opencode").to_string_lossy()
                )),
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

    #[test]
    fn v07c_source_substitution_after_ancestor_swap_stays_inside_admitted_root() {
        use super::{admit_root, read_source_config};
        use crate::config;
        use std::collections::{BTreeMap, HashSet};
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("project");
        let external = tmp.path().join("external");
        std::fs::create_dir_all(project.join("branch/source")).unwrap();
        std::fs::create_dir_all(external.join("source")).unwrap();
        std::fs::write(project.join("branch/source/key"), "local-fixture-key").unwrap();
        std::fs::write(external.join("source/key"), "EXTERNAL_V07C_SECRET_734a").unwrap();
        let config_path = project.join("branch/source/opencode.json");
        std::fs::write(
            &config_path,
            r#"{"provider":{"fixture":{"options":{"apiKey":"{file:key}"}}}}"#,
        )
        .unwrap();
        let root = admit_root(&project, &project, false).unwrap().unwrap();
        let (canonical, text) = read_source_config(&root, &config_path).unwrap().unwrap();
        let sources = [config::Source {
            path: canonical.to_string_lossy().into_owned(),
            text,
            trusted: true,
        }];
        // Deterministic: config bytes were already admitted, but their source
        // ancestor is now an outside symlink before provider substitution.
        std::fs::rename(project.join("branch"), project.join("branch-old")).unwrap();
        std::os::unix::fs::symlink(&external, project.join("branch")).unwrap();
        let roots = BTreeMap::from([(
            sources[0].path.clone(),
            (&root.dir, std::path::PathBuf::from("branch/source")),
        )]);
        let generation = config::assemble_admitted_with_terminal_copy(
            &sources,
            &BTreeMap::new(),
            Some(&HashSet::from(["fixture".into()])),
            &roots,
        );
        let error = generation.expect_err("replaced ancestor must fail closed");
        assert_eq!(error.code, oc_core::queries::ServiceCode::TrustRefused);
        assert_eq!(error.stage, oc_core::queries::ServiceStage::Admission);
        assert!(!error.to_string().contains("EXTERNAL_V07C_SECRET_734a"));
        // The same root capability can still serve a valid nested reference.
        std::fs::remove_file(project.join("branch")).unwrap();
        std::fs::rename(project.join("branch-old"), project.join("branch")).unwrap();
        let generation =
            config::assemble_admitted_with_terminal_copy(&sources, &BTreeMap::new(), None, &roots)
                .unwrap()
                .0;
        assert_eq!(
            generation.providers["fixture"].options.api_key,
            "local-fixture-key"
        );
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
        let unavailable = load_with_env(&project, env)
            .await
            .expect("local generation");
        assert!(unavailable.provider.api_key.is_empty());
        assert_eq!(unavailable.model_id, "org/new");
        let readiness = unavailable.tui_chrome.provider.unwrap();
        assert_eq!(
            readiness.status,
            oc_core::queries::ProviderStatus::Unavailable
        );
        assert_eq!(
            readiness.diagnostic.unwrap().code,
            oc_core::queries::ServiceCode::MissingCredential
        );
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
        assert!(
            error.contains("trust_refused")
                && error.contains(&crate::config::mcp::safe_source_id(
                    &project.join("opencode.json").to_string_lossy()
                )),
            "{error}"
        );
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
        assert!(
            error.contains("trust_refused")
                && error.contains(&crate::config::mcp::safe_source_id(
                    &project.join(".opencode").to_string_lossy()
                )),
            "{error}"
        );
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
            error.contains("unsupported_capability")
                && error.contains(&crate::config::mcp::safe_source_id(
                    &project.join("dcp.jsonc").to_string_lossy()
                )),
            "the diagnostic must name the source file: {error}"
        );

        // Supported child compression flag has no obsolete ignored warning.
        std::fs::write(
            project.join("dcp.jsonc"),
            r#"{"experimental": {"allowSubAgents": true}}"#,
        )
        .expect("dcp config");
        let loaded = load_with_env(&project, BTreeMap::new())
            .await
            .expect("allowSubAgents must not block");
        assert!(loaded.dcp_config.allow_subagents);
        assert!(
            !loaded
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("ignored_setting")
                    && diagnostic.contains("dcp")),
            "obsolete ignored option warning: {:?}",
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
        assert!(error.contains("missing_configuration") && error.contains("document"));
        std::fs::write(dir.path().join("opencode.json"), "{}").expect("config");
        let error = load_with_env(dir.path(), BTreeMap::new())
            .await
            .map(|_| ())
            .expect_err("missing model");
        assert!(error.contains("missing_configuration") && error.contains("model"));
    }

    /// Subagent S3: every admitted agent stays in the catalog, the depth knob
    /// comes from `experimental.subagent_depth`, and a subagent-only configured
    /// `default_agent` falls back without promoting the child to primary.
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
        assert_eq!(
            ids,
            ["boss", "build", "explore", "general", "helper", "plan"]
        );
        assert_eq!(loaded.default_agent.as_deref(), Some("build"));
        assert!(!loaded.agents["build"].subagent_capable());
        assert!(!loaded.agents["boss"].subagent_capable());
        assert!(!loaded.agents["general"].primary_capable());
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
        let loaded = load_with_env(dir.path(), BTreeMap::new())
            .await
            .expect("subagent-only configured default uses pinned fallback");
        assert_eq!(loaded.default_agent.as_deref(), Some("build"));
        assert!(!loaded.agents["helper"].primary_capable());
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
