//! Selection-independent read-view. No application/store mutation or recovery.
use super::*;

/// Complete admitted references plus payload-free catalog diagnostics.
pub struct CatalogListing {
    /// Full exact provider/model references, sorted lexically.
    pub references: Vec<String>,
    /// Safe warnings/errors; never connection values or remote payloads.
    pub diagnostics: Vec<ServiceDiagnostic>,
    /// False when required dynamic metadata could not be obtained.
    pub complete: bool,
}

pub(super) fn provider_filters(
    sources: &[config::Source],
) -> Result<(Option<Vec<String>>, Vec<String>), LoadFailure> {
    let mut enabled = None;
    let mut disabled = Vec::new();
    for source in sources {
        let value =
            config::parse_jsonc(&source.text, &source.path).map_err(|_| document(&source.path))?;
        if let Some(list) = value.get("enabled_providers") {
            enabled = Some(provider_ids(list, "enabled_providers", &source.path)?);
        }
        if let Some(list) = value.get("disabled_providers") {
            disabled = provider_ids(list, "disabled_providers", &source.path)?;
        }
    }
    Ok((enabled, disabled))
}

/// Load metadata before mandatory model selection. Dropping this future cancels
/// its owned read-only GET; no detached catalog/application tasks are created.
pub async fn load_catalog(project: &Path) -> Result<CatalogListing, ServiceDiagnostic> {
    load_catalog_cached(project, None).await
}

/// Load the same public catalog with an optional read-only native cache snapshot.
/// Does not acquire a second data-root owner or create a missing data root.
pub async fn load_catalog_cached(
    project: &Path,
    data: Option<&Path>,
) -> Result<CatalogListing, ServiceDiagnostic> {
    let env = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .collect();
    let public = crate::models_dev::GoCatalog::read_only(data);
    let client =
        discovery::ReqwestDiscoveryClient::new(Duration::from_secs(10)).map_err(
            |_| match failure(
                crate::models_dev::SOURCE,
                &["models"],
                ServiceStage::ModelCatalog,
                ServiceCode::Transport,
            ) {
                LoadFailure::Configuration(diagnostic) => diagnostic,
            },
        )?;
    load_catalog_inner(project, env, Some(&public), &client)
        .await
        .map_err(|failure| match failure {
            LoadFailure::Configuration(diagnostic) => diagnostic,
        })
}

#[cfg(test)]
async fn load_catalog_with_env(
    project: &Path,
    env: BTreeMap<String, String>,
) -> Result<CatalogListing, LoadFailure> {
    let client = discovery::ReqwestDiscoveryClient::new(Duration::from_secs(10)).unwrap();
    load_catalog_inner(project, env, None, &client).await
}

async fn load_catalog_inner(
    project: &Path,
    env: BTreeMap<String, String>,
    public: Option<&crate::models_dev::GoCatalog>,
    client: &impl discovery::DiscoveryClient,
) -> Result<CatalogListing, LoadFailure> {
    let admitted = admit_sources(project, &env)?;
    admit_dcp(&admitted.sources, &admitted.admitted_roots)?;
    let settings = admit_settings(&admitted.sources, &admitted.admitted_roots, &env, false)?;
    let (enabled, disabled) = (settings.enabled, settings.disabled);
    let include =
        |id: &String| !disabled.contains(id) && enabled.as_ref().is_none_or(|ids| ids.contains(id));
    let mut dynamic = if discovery::should_run(&disabled, enabled.as_deref()) {
        HashSet::from([discovery::PROVIDER_ID.to_string()])
    } else {
        HashSet::new()
    };
    if public.is_some() && include(&crate::models_dev::PROVIDER.to_string()) {
        dynamic.insert(crate::models_dev::PROVIDER.into());
    }
    let roots = source_roots(&admitted.admitted_roots, &admitted.source_authority);
    let mut generation =
        config::assemble_catalog_admitted(&admitted.sources, &env, &dynamic, &roots)
            .map_err(LoadFailure::Configuration)?;
    // Foreign protocol metadata is inert, not an admitted native source.
    generation.providers.retain(|id, entry| {
        include(id) && config::package_protocol(id, entry.npm.as_deref()).is_ok()
    });
    // Validate the independently known snapshot before any GET/publication.
    references(&generation)?;
    let mut diagnostics: Vec<_> = settings
        .plugins
        .entries
        .into_iter()
        .filter_map(|entry| entry.diagnostic)
        .collect();
    let mut complete = true;
    if let Some(entry) = generation.providers.get(discovery::PROVIDER_ID) {
        let source = generation
            .provenance
            .get(&format!("provider.{}", discovery::PROVIDER_ID))
            .expect("provider source");
        let ready = entry.options.auth_policy == crate::auth::AuthPolicy::None
            || (entry.options.auth_policy == crate::auth::AuthPolicy::Key
                && !entry.options.api_key.trim().is_empty());
        let mut state = ProviderState::new(discovery::PROVIDER_ID, source, ready, true)
            .with_auth_policy(entry.options.auth_policy);
        if !ready {
            complete = false;
            diagnostics.push(
                state
                    .for_model("", true)
                    .diagnostic
                    .expect("missing key cause"),
            );
        } else {
            // The same native discovery owner controls URL/header admission,
            // retry budgets, strict rows, atomic merge and remote ID retirement.
            let provider = provider::ResponsesConfig {
                base_url: entry.options.base_url.clone(),
                api_key: entry.options.api_key.clone(),
                headers: entry.options.headers.clone(),
                timeout: entry
                    .options
                    .timeout
                    .and_then(config::ProviderTimeout::legacy_flag),
                chunk_timeout_ms: entry
                    .options
                    .chunk_timeout
                    .unwrap_or(provider::CHUNK_TIMEOUT_MS),
                connect_timeout: Duration::from_secs(10),
                allow_private: env.get("OC_TEST_ALLOW_LOOPBACK").map(String::as_str) == Some("1"),
                set_cache_key: entry.options.set_cache_key.unwrap_or(false),
                wire: config::provider_wire(discovery::PROVIDER_ID, entry)
                    .map_err(|_| invalid(source, &["provider", "options"]))?,
            };
            let outcome = Composition::discover_provider(provider, entry.models.clone()).await?;
            state.finish(&outcome);
            complete = outcome.replaced;
            if let Some(diagnostic) = state.for_model("", true).diagnostic {
                diagnostics.push(diagnostic);
            }
            generation
                .providers
                .get_mut(discovery::PROVIDER_ID)
                .expect("catalog source")
                .models = outcome.models;
        }
    }
    if let Some(public) = public
        && let Some(entry) = generation.providers.get_mut(crate::models_dev::PROVIDER)
    {
        let read = public
            .refresh_read_only(client, &entry.models, super::go_catalog::now_ms())
            .await;
        complete &= read.failure.is_none() && read.fetched_at_ms.is_some();
        if let Some(error) = read.failure {
            diagnostics.push(
                match failure(
                    crate::models_dev::SOURCE,
                    &["models"],
                    ServiceStage::ModelCatalog,
                    match error {
                        discovery::DiscoveryFailure::Network => ServiceCode::Transport,
                        _ => ServiceCode::InvalidCatalog,
                    },
                ) {
                    LoadFailure::Configuration(diagnostic) => diagnostic,
                },
            );
        }
        entry.models = read.models;
    }
    Ok(CatalogListing {
        references: references(&generation)?,
        diagnostics,
        complete,
    })
}

fn references(generation: &config::Generation) -> Result<Vec<String>, LoadFailure> {
    let mut references = Vec::new();
    for (provider, entry) in &generation.providers {
        for (id, metadata) in &entry.models {
            let source = generation
                .provenance
                .get(&format!("provider.{provider}"))
                .map(String::as_str)
                .unwrap_or("native catalog");
            let name = metadata
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(id);
            if references.len() >= models::CATALOG_ROWS_CAP
                || provider.len() + id.len() + name.len() > models::CATALOG_LABEL_BYTES_CAP
            {
                return Err(failure(
                    source,
                    &["provider", "models"],
                    ServiceStage::ModelCatalog,
                    ServiceCode::CapacityExceeded,
                ));
            }
            if provider.is_empty()
                || provider.contains('/')
                || id.trim().is_empty()
                || provider.chars().chain(id.chars()).any(char::is_control)
            {
                return Err(failure(
                    source,
                    &["provider", "models"],
                    ServiceStage::ModelCatalog,
                    ServiceCode::InvalidCatalog,
                ));
            }
            references.push(format!("{provider}/{id}"));
        }
    }
    references.sort();
    Ok(references)
}

#[cfg(test)]
mod tests;
