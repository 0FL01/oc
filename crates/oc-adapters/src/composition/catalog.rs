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

/// Canonical `experimental.policies` `provider.use` rules (donor `plugin/policy.ts`):
/// documents in reverse precedence (user-global last), the last matching wildcard
/// wins and `deny` removes the provider. No rule means allowed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProviderPolicy {
    rules: Vec<(String, bool)>,
}

impl ProviderPolicy {
    pub(crate) fn allows(&self, id: &str) -> bool {
        self.rules
            .iter()
            .rev()
            .find(|(pattern, _)| crate::permissions::wildcard(id, pattern))
            .is_none_or(|(_, allow)| *allow)
    }

    /// An allowed id named exactly by an `allow` rule (explicit opt-in).
    pub(crate) fn names(&self, id: &str) -> bool {
        self.allows(id)
            && self
                .rules
                .iter()
                .any(|(pattern, allow)| *allow && pattern == id)
    }

    /// A not-yet-configured id (custom connection creation) is admitted unless the
    /// winning catch-all rule denies it.
    pub(crate) fn allows_unlisted(&self) -> bool {
        self.rules
            .iter()
            .rev()
            .find(|(pattern, _)| pattern == "*")
            .is_none_or(|(_, allow)| *allow)
    }
}

pub(crate) fn provider_filters(sources: &[config::Source]) -> Result<ProviderPolicy, LoadFailure> {
    let mut policy = ProviderPolicy::default();
    for source in sources.iter().rev() {
        let value =
            config::parse_jsonc(&source.text, &source.path).map_err(|_| document(&source.path))?;
        let Some(rules) = value.pointer("/experimental/policies") else {
            continue;
        };
        let field = ["experimental", "policies"];
        let rules = rules
            .as_array()
            .ok_or_else(|| invalid(&source.path, &field))?;
        for rule in rules {
            let text = |key: &str| rule.get(key).and_then(serde_json::Value::as_str);
            let allow = match text("effect") {
                Some("allow") => true,
                Some("deny") => false,
                _ => return Err(invalid(&source.path, &field)),
            };
            let resource = text("resource").ok_or_else(|| invalid(&source.path, &field))?;
            match text("action") {
                Some("provider.use") => policy.rules.push((resource.to_string(), allow)),
                // Ignoring a permission deny would widen authority: refuse instead.
                Some("permission") => {
                    return Err(failure(
                        &source.path,
                        &field,
                        ServiceStage::Capability,
                        ServiceCode::UnsupportedCapability,
                    ));
                }
                _ => return Err(invalid(&source.path, &field)),
            }
        }
    }
    Ok(policy)
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
    let subscription = data.is_some_and(crate::storage::Db::openai_subscription_read_only);
    load_catalog_inner(project, env, Some(&public), &client, subscription, data)
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
    load_catalog_inner(project, env, None, &client, false, None).await
}

async fn load_catalog_inner(
    project: &Path,
    env: BTreeMap<String, String>,
    public: Option<&crate::models_dev::GoCatalog>,
    client: &impl discovery::DiscoveryClient,
    subscription: bool,
    data: Option<&Path>,
) -> Result<CatalogListing, LoadFailure> {
    let admitted = admit_sources(project, &env)?;
    admit_dcp(
        &admitted.sources,
        &admitted.admitted_roots,
        &admitted.source_authority,
    )?;
    let settings = admit_settings(&admitted.sources, &admitted.admitted_roots, &env, false)?;
    let policy = settings.providers;
    let include = |id: &String| policy.allows(id);
    let mut dynamic = if policy.allows(discovery::PROVIDER_ID) {
        HashSet::from([discovery::PROVIDER_ID.to_string()])
    } else {
        HashSet::new()
    };
    if public.is_some() && include(&crate::models_dev::PROVIDER.to_string()) {
        dynamic.insert(crate::models_dev::PROVIDER.into());
    }
    let openai_requested = policy.names(crate::models_dev::OPENAI)
        || admitted.sources.iter().any(|s| {
            config::parse_jsonc(&s.text, &s.path).ok().is_some_and(|v| {
                v.pointer("/providers/openai").is_some()
                    || v.get("model")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|s| {
                            models::parse_reference(s)
                                .is_ok_and(|(p, _, _)| p == crate::models_dev::OPENAI)
                        })
            })
        });
    if public.is_some()
        && include(&crate::models_dev::OPENAI.to_string())
        && (include(&crate::models_dev::PROVIDER.to_string()) || openai_requested)
    {
        dynamic.insert(crate::models_dev::OPENAI.into());
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
                header_timeout_ms: entry.options.header_timeout.map_or(
                    Some(provider::HEADER_TIMEOUT_MS),
                    config::ProviderTimeout::milliseconds,
                ),
                chunk_timeout_ms: entry.options.chunk_timeout.map_or(
                    Some(provider::CHUNK_TIMEOUT_MS),
                    config::ProviderTimeout::milliseconds,
                ),
                connect_timeout: Duration::from_secs(10),
                allow_private: env.get("OC_TEST_ALLOW_LOOPBACK").map(String::as_str) == Some("1"),
                cache: Default::default(),
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
    for id in [crate::models_dev::PROVIDER, crate::models_dev::OPENAI] {
        if let Some(public) = public
            && let Some(entry) = generation.providers.get_mut(id)
            && (id == crate::models_dev::PROVIDER
                || crate::auth::AuthScope::admit(id, &entry.options.base_url)
                    .is_ok_and(|s| s.is_openai()))
        {
            if id == crate::models_dev::OPENAI
                && entry.models.is_empty()
                && generation
                    .provenance
                    .get(&format!("provider.{id}"))
                    .map(String::as_str)
                    == Some("native OpenAI preset")
                && !openai_requested
                && public
                    .read_provider(id, &entry.models)
                    .await
                    .fetched_at_ms
                    .is_none()
            {
                continue;
            }
            let read = public
                .refresh_provider_read_only(client, id, &entry.models, super::go_catalog::now_ms())
                .await;
            if id == crate::models_dev::OPENAI
                && read.fetched_at_ms.is_none()
                && !openai_requested
                && entry.models.is_empty()
            {
                continue;
            }
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
            if id == crate::models_dev::OPENAI {
                super::openai_catalog::transform(&mut entry.models, subscription);
            }
        }
    }
    if let Some(data) = data {
        let saved = crate::storage::Db::saved_custom_providers(data).map_err(|_| {
            failure(
                super::custom_connections::SOURCE,
                &["connections"],
                ServiceStage::ModelCatalog,
                ServiceCode::StorageUnavailable,
            )
        })?;
        let reserved = super::custom_connections::reserved(&admitted.sources);
        for row in saved {
            if !include(&row.id) || reserved.contains(&row.id) {
                continue;
            }
            use crate::storage::provider_connections::CatalogState;
            if row.state != CatalogState::Ready || row.catalog_revision != Some(row.revision) {
                complete = false;
                diagnostics.push(
                    match failure(
                        super::custom_connections::SOURCE,
                        &["provider", "models"],
                        ServiceStage::ModelCatalog,
                        match row.state {
                            CatalogState::Unauthorized => ServiceCode::Unauthorized,
                            CatalogState::Forbidden => ServiceCode::Forbidden,
                            _ => ServiceCode::InvalidCatalog,
                        },
                    ) {
                        LoadFailure::Configuration(d) => d,
                    },
                );
            }
            generation.provenance.insert(
                format!("provider.{}", row.id),
                super::custom_connections::SOURCE.into(),
            );
            generation.providers.insert(
                row.id,
                config::ProviderEntry {
                    npm: None,
                    name: Some(row.name),
                    env: Vec::new(),
                    compatibility: Default::default(),
                    options: Default::default(),
                    models: row.models,
                },
            );
        }
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
