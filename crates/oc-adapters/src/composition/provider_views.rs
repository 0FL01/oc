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
    async fn resolve(
        &mut self,
        db: &crate::storage::Db,
        env: &BTreeMap<String, String>,
    ) -> Result<(), LoadFailure> {
        let id = &self.catalog.provider;
        for request in self.provider.wire.requests.values_mut() {
            super::provider_readiness::resolve_binding(
                request,
                id,
                db,
                env,
                "native provider view",
                &["provider", "authPolicy"],
                ServiceStage::Admission,
            )
            .await?;
        }
        super::provider_readiness::resolve_binding(
            &mut self.provider,
            id,
            db,
            env,
            "native provider view",
            &["provider", "authPolicy"],
            ServiceStage::Admission,
        )
        .await?;
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
    pub(crate) fn model_reference(
        &self,
        fallback: &str,
        raw: &str,
    ) -> Result<oc_core::queries::ModelRef, &'static str> {
        if self
            .catalog_for(fallback)
            .is_some_and(|catalog| catalog.models.contains_key(raw))
        {
            return Ok(oc_core::queries::ModelRef {
                provider: fallback.into(),
                id: raw.into(),
                variant: None,
            });
        }
        let (provider, id, variant) = models::parse_reference(raw)?;
        if self.catalog_for(provider).is_none() {
            return Err("model provider unavailable");
        }
        Ok(oc_core::queries::ModelRef {
            provider: provider.into(),
            id: id.into(),
            variant: variant.map(str::to_owned),
        })
    }
    /// Finite leaf captures: never nest another provider map inside a leaf.
    pub(crate) fn request_provider(&self, id: &str) -> Option<provider::ResponsesConfig> {
        let leaf = |config: &provider::ResponsesConfig, state: &ProviderState| {
            let mut config = config.clone();
            config.wire.providers.clear();
            config.wire.provider_state = Some(state.clone());
            config
        };
        let mut providers = BTreeMap::new();
        providers.insert(
            self.catalog.provider.clone(),
            provider::CapturedProvider {
                catalog: self.catalog.clone(),
                config: leaf(&self.provider, &self.provider_state),
            },
        );
        for (id, view) in &self.provider_views {
            providers.insert(
                id.clone(),
                provider::CapturedProvider {
                    catalog: view.catalog.clone(),
                    config: leaf(&view.provider, &view.state),
                },
            );
        }
        let mut config = providers.get(id)?.config.clone();
        config.wire.selection_scope = Some(self.catalog.provider.clone());
        config.wire.providers = providers;
        Some(config)
    }

    pub(crate) fn catalog_for(&self, id: &str) -> Option<&models::ModelCatalog> {
        if id == self.catalog.provider {
            Some(&self.catalog)
        } else {
            self.provider_views.get(id).map(|view| &view.catalog)
        }
    }

    pub(crate) fn provider_for(&self, id: &str) -> Option<&provider::ResponsesConfig> {
        if id == self.catalog.provider {
            Some(&self.provider)
        } else {
            self.provider_views.get(id).map(|view| &view.provider)
        }
    }

    pub(crate) fn readiness_for(
        &self,
        id: &str,
        model: &str,
        variant: Option<&str>,
    ) -> oc_core::queries::ProviderReadiness {
        if id == self.catalog.provider {
            self.selected_provider_readiness(model, variant)
        } else if let Some(view) = self.provider_views.get(id) {
            view.readiness(model, variant)
        } else {
            ProviderState::new(id, "saved provider choice", false, false).for_model(model, false)
        }
    }

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

    pub(crate) async fn resolve_provider_views(
        &mut self,
        db: &crate::storage::Db,
    ) -> Result<(), LoadFailure> {
        for view in self.provider_views.values_mut() {
            view.resolve(db, &self.parent_env).await?;
            // Only redaction captures enter the runtime config. Its inert view
            // must never become a configured credential for MCP inheritance.
            let entry = self
                .generation
                .providers
                .entry(view.catalog.provider.clone())
                .or_insert_with(|| config::ProviderEntry {
                    npm: view.entry.npm.clone(),
                    name: view.entry.name.clone(),
                    env: Vec::new(),
                    models: view.entry.models.clone(),
                    options: config::ProviderOptions::default(),
                });
            entry.options.redaction_material = view.entry.options.redaction_material.clone();
            entry.options.request_bindings = view.entry.options.request_bindings.clone();
            entry.options.native_fallback_limits = view.entry.options.native_fallback_limits;
        }
        Ok(())
    }
}
