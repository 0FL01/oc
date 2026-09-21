//! Shared config/catalog snapshot for the CLI and TUI application worker.
//!
//! The supplied project is the admitted Location boundary. This baseline
//! composes existing adapters; broader config/Location support belongs to T35.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::{config, discovery, models, provider};

/// Fully built application configuration. Contains credentials and must not be logged.
pub struct Composition {
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

async fn load_with_env(
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
    let mut roots: Vec<PathBuf> = global.into_iter().collect();
    roots.push(project.clone());
    roots.push(project.join(".opencode"));
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for root in &roots {
        for name in ["opencode.json", "opencode.jsonc"] {
            let path = root.join(name);
            let text = match std::fs::read_to_string(&path) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(format!("cannot read config {}: {e}", path.display())),
            };
            let canonical = path
                .canonicalize()
                .map_err(|e| format!("cannot resolve config {}: {e}", path.display()))?;
            if seen.insert(canonical.clone()) {
                sources.push(config::Source {
                    path: canonical.to_string_lossy().into_owned(),
                    text,
                    trusted: false,
                });
            }
        }
    }
    if sources.is_empty() {
        return Err("no opencode.json/jsonc found; configure a provider and top-level model (provider/model-id) in the project or XDG opencode config directory".to_string());
    }

    let mut selected = None;
    let mut enabled = None;
    let mut disabled = Vec::new();
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
                let admitted = roots.iter().any(|root| {
                    let root = root.canonicalize().unwrap_or_else(|_| root.clone());
                    config::classify_plugin(&identity, &root.to_string_lossy()).is_ok()
                });
                if !admitted {
                    return Err(format!("{}: unsupported plugin {identity}", source.path));
                }
            }
        }
    }
    let selected = selected.ok_or_else(|| {
        "model required: set top-level model to provider/model-id in opencode.json/jsonc"
            .to_string()
    })?;
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
    let entry = generation.providers.get(provider_id).ok_or_else(|| {
        format!("selected provider {provider_id} is not configured; add provider.{provider_id}")
    })?;
    let provider = provider::ResponsesConfig {
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
            &BTreeMap::new(),
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
    Ok(Composition {
        generation,
        catalog,
        model_id: model_id.to_string(),
        provider,
        project,
        parent_env,
    })
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
}
