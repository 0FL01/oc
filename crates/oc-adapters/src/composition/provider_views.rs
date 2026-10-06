//! Executable connection views admitted from the same frozen local sources.
//! This is not a second policy/Location owner or a provider SDK registry.
use super::*;

pub(crate) struct ProviderView {
    pub(crate) entry: config::ProviderEntry,
    pub(crate) catalog: models::ModelCatalog,
    pub(crate) provider: provider::ResponsesConfig,
    pub(crate) state: ProviderState,
}

impl ProviderView {
    fn resolve(
        &mut self,
        db: &crate::storage::Db,
        env: &BTreeMap<String, String>,
    ) -> Result<(), LoadFailure> {
        let id = &self.catalog.provider;
        let resolve = |request: &mut provider::ResponsesConfig| -> Result<(), LoadFailure> {
            if request.base_url.is_empty() {
                request.wire.unsupported = true;
                return Ok(());
            }
            request.restore_auth_input();
            let scope = crate::auth::AuthScope::admit(id, &request.base_url)
                .map_err(|_| invalid("native provider view", &["provider", "baseURL"]))?;
            let competing = request.headers.keys().any(|name| {
                name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
            });
            scope
                .resolve(
                    db,
                    request.wire.auth_policy,
                    Some(&request.api_key),
                    competing,
                    || env.get("OPENCODE_API_KEY").cloned(),
                )
                .map_err(|error| {
                    failure(
                        "native provider view",
                        &["provider", "authPolicy"],
                        ServiceStage::Admission,
                        if matches!(error, crate::auth::AuthError::Storage(_)) {
                            ServiceCode::StorageUnavailable
                        } else {
                            ServiceCode::InvalidConfig
                        },
                    )
                })?
                .apply_to(request);
            Ok(())
        };
        for request in self.provider.wire.requests.values_mut() {
            resolve(request)?;
        }
        resolve(&mut self.provider)?;
        self.entry.options.request_bindings = self.provider.wire.requests.clone();
        self.entry.options.redaction_material = std::iter::once(self.provider.api_key.clone())
            .filter(|key| !key.is_empty())
            .collect();
        self.state
            .set_auth(self.provider.auth_ready(), self.provider.wire.auth_policy);
        Ok(())
    }

    pub(crate) fn readiness(
        &self,
        model: &str,
        variant: Option<&str>,
    ) -> oc_core::queries::ProviderReadiness {
        let provider = self.provider.for_selection(model, variant);
        let mut state = self.state.clone();
        state.set_auth(provider.auth_ready(), provider.wire.auth_policy);
        if provider.wire.unsupported {
            state.set_unsupported();
        }
        state.for_model(model, self.catalog.models.contains_key(model))
    }
}

impl Composition {
    /// Freeze only connection views. Reassembling normalized provider inputs does
    /// not publish policy, activate MCP, read new config files or open another Db.
    pub(crate) fn admit_provider_views(&mut self) {
        let mut ids: BTreeSet<_> = self.generation.providers.keys().cloned().collect();
        if self.generation.public_go_enabled {
            ids.insert(crate::models_dev::PROVIDER.into());
        }
        let roots = self
            .mcp_activation
            .roots
            .iter()
            .map(|(source, (root, directory))| (source.clone(), (root, directory.clone())))
            .collect();
        for id in ids {
            if id == self.catalog.provider {
                continue;
            }
            let admitted = config::assemble_admitted_with_terminal_copy(
                &self.mcp_activation.sources,
                &self.parent_env,
                Some(&HashSet::from([id.clone()])),
                &roots,
            );
            let entry = match admitted {
                Ok((generation, _)) => match generation.providers.get(&id) {
                    Some(entry) => entry.clone(),
                    None => continue,
                },
                Err(diagnostic) => {
                    if self.tui_chrome.service_diagnostics.len() < 64 {
                        self.tui_chrome.service_diagnostics.push(diagnostic);
                    } else {
                        self.tui_chrome.service_diagnostics_omitted += 1;
                    }
                    continue;
                }
            };
            let Ok(wire) = config::provider_wire(&id, &entry) else {
                continue;
            };
            let mut provider = provider::ResponsesConfig {
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
                allow_private: false,
                set_cache_key: entry.options.set_cache_key.unwrap_or(false),
                wire,
            };
            if provider.base_url.is_empty() {
                provider.wire.unsupported = true;
            }
            let mut validation = provider.clone();
            if validation.wire.auth_policy == crate::auth::AuthPolicy::OAuth {
                validation.wire.auth_policy = crate::auth::AuthPolicy::Key;
            }
            if provider::request_headers(&validation).is_err() {
                continue;
            }
            let state = ProviderState::new(
                &id,
                "native provider view",
                provider.auth_ready(),
                id == discovery::PROVIDER_ID,
            )
            .with_auth_policy(provider.wire.auth_policy);
            self.provider_views.insert(
                id.clone(),
                ProviderView {
                    catalog: models::ModelCatalog {
                        provider: id,
                        models: entry.models.clone(),
                    },
                    entry,
                    provider,
                    state,
                },
            );
        }
    }

    pub(crate) fn resolve_provider_views(
        &mut self,
        db: &crate::storage::Db,
    ) -> Result<(), LoadFailure> {
        for view in self.provider_views.values_mut() {
            view.resolve(db, &self.parent_env)?;
        }
        Ok(())
    }
}
