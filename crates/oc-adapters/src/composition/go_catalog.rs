//! Public metadata and immutable execution bindings remain separate from auth.
use super::*;
use crate::models_dev::{CatalogRead, PROVIDER};
use serde_json::Value;

pub(crate) fn outcome(read: CatalogRead) -> discovery::DiscoveryOutcome {
    discovery::DiscoveryOutcome {
        models: read.models,
        warnings: Vec::new(),
        attempts: 0,
        replaced: read.fetched_at_ms.is_some(),
        failure: read.failure,
    }
}

#[cfg(test)]
mod tests;

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

impl Composition {
    pub(crate) fn public_provider(&self, id: &str) -> bool {
        if id == PROVIDER {
            return self.generation.public_go_enabled;
        }
        id == crate::models_dev::OPENAI
            && self.generation.public_openai_enabled
            && self
                .provider_views
                .get(id)
                .map(|v| &v.entry)
                .or_else(|| self.generation.providers.get(id))
                .is_some_and(|entry| {
                    crate::auth::AuthScope::admit(id, &entry.options.base_url)
                        .is_ok_and(|s| s.is_openai())
                })
    }

    pub(crate) fn accept_public_view(&mut self, outcome: discovery::DiscoveryOutcome) -> bool {
        self.accept_public_view_for(PROVIDER, outcome)
    }

    pub(crate) fn accept_public_view_for(
        &mut self,
        id: &str,
        mut outcome: discovery::DiscoveryOutcome,
    ) -> bool {
        if !self.public_provider(id) {
            return false;
        }
        if let Some(view) = self.provider_views.get_mut(id) {
            if id == crate::models_dev::OPENAI {
                super::openai_catalog::transform(&mut outcome.models, view.provider.subscription());
            }
            let before = (view.state.catalog_status, view.readiness("", None));
            let models_changed = view.catalog.models != outcome.models;
            view.state.finish_public(&outcome);
            view.catalog.models = outcome.models;
            view.provider.wire.requests =
                public_bindings(&view.entry, &view.catalog, &view.provider);
            view.entry.options.request_bindings = view.provider.wire.requests.clone();
            if let Some(entry) = self.generation.providers.get_mut(id) {
                entry.options.request_bindings = view.provider.wire.requests.clone();
            }
            return models_changed
                || before != (view.state.catalog_status, view.readiness("", None));
        }
        false
    }

    pub(crate) async fn attach_public_catalog(&mut self, db: &crate::storage::Db) -> bool {
        let owner = db.public_catalog();
        let mut changed = false;
        for id in [PROVIDER, crate::models_dev::OPENAI] {
            if !self.public_provider(id) {
                continue;
            }
            let local = if id == self.catalog.provider {
                &self.generation.providers[id].models
            } else {
                &self.provider_views[id].entry.models
            };
            let mut read = owner.read_provider(id, local).await;
            // Explicit configured OpenAI models remain a usable offline base;
            // a validated public retirement still wins over those local rows.
            if id == crate::models_dev::OPENAI && read.fetched_at_ms.is_none() {
                read.models = local.clone();
            }
            if id == self.catalog.provider {
                let previous = (self.catalog.models.clone(), self.provider_state.clone());
                self.go_catalog = Some(owner.clone());
                self.provider_state.public_pending();
                if read.fetched_at_ms.is_none() && read.failure.is_none() && read.models.is_empty()
                {
                    self.catalog.models.clear();
                    self.capture_public_bindings();
                } else {
                    self.accept_provider_catalog(outcome(read));
                }
                changed |= previous != (self.catalog.models.clone(), self.provider_state.clone());
            } else if let Some(view) = self.provider_views.get_mut(id) {
                let previous = (view.catalog.models.clone(), view.state.clone());
                view.state.public_pending();
                if read.fetched_at_ms.is_none() && read.failure.is_none() && read.models.is_empty()
                {
                    view.catalog.models.clear();
                    view.provider.wire.requests.clear();
                } else {
                    self.accept_public_view_for(id, outcome(read));
                }
                let view = &self.provider_views[id];
                changed |= previous != (view.catalog.models.clone(), view.state.clone());
            }
        }
        self.tui_chrome.provider =
            Some(self.selected_provider_readiness(&self.model_id, self.variant.as_deref()));
        changed
    }

    pub(crate) async fn refresh_public_catalog(&mut self, db: &crate::storage::Db, force: bool) {
        let Some(owner) = self.go_catalog.clone() else {
            return;
        };
        let id = self.catalog.provider.clone();
        let client = discovery::ReqwestDiscoveryClient::new(Duration::from_secs(10));
        let Ok(client) = client else {
            let mut read = owner
                .read_provider(&id, &self.generation.providers[&id].models)
                .await;
            read.failure = Some(discovery::DiscoveryFailure::Network);
            self.accept_provider_catalog(outcome(read));
            return;
        };
        let read = owner
            .refresh_provider(
                db,
                &client,
                &id,
                &self.generation.providers[&id].models,
                now_ms(),
                force,
            )
            .await;
        self.accept_provider_catalog(outcome(read));
        self.attach_public_catalog(db).await;
    }

    pub(super) fn capture_public_bindings(&mut self) {
        let Some(entry) = self.generation.providers.get(&self.catalog.provider) else {
            // A retained Go choice may be explicitly disabled in this
            // generation. Another connection's account ACK must not recreate
            // its bindings or assume that the optional entry exists.
            self.provider.wire.requests.clear();
            return;
        };
        self.provider.wire.requests = public_bindings(entry, &self.catalog, &self.provider);
        if let Some(entry) = self.generation.providers.get_mut(&self.catalog.provider) {
            entry.options.request_bindings = self.provider.wire.requests.clone();
        }
    }
}

fn public_bindings(
    entry: &config::ProviderEntry,
    catalog: &models::ModelCatalog,
    provider: &provider::ResponsesConfig,
) -> BTreeMap<(String, Option<String>), provider::ResponsesConfig> {
    let original = entry.options.request_bindings.clone();
    let mut requests = BTreeMap::new();
    for (id, model) in &catalog.models {
        let variants = std::iter::once((None, None)).chain(
            models::ordered_variants(model)
                .into_iter()
                .map(|(id, value)| (Some(id), Some(value))),
        );
        for (variant, value) in variants {
            let key = (id.clone(), variant.map(str::to_owned));
            let exact = original.get(&key);
            let mut request = exact
                .or_else(|| original.get(&(id.clone(), None)))
                .unwrap_or(provider)
                .clone();
            request.wire.requests.clear();
            request.wire.api_model = Some(
                model
                    .get("modelID")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_owned(),
            );
            // This source can choose a finite wire alias, never a route/auth input.
            let package = model.get("package").and_then(Value::as_str);
            let supported = config::package_protocol(&catalog.provider, package)
                .and_then(|protocol| {
                    let settings: BTreeMap<String, Value> = value
                        .and_then(|v| v.get("settings"))
                        .and_then(Value::as_object)
                        .map(|v| v.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();
                    // Exact local variants were already admitted with their own field provenance.
                    if exact.is_none() {
                        request.wire.settings = request
                            .wire
                            .settings
                            .overlay(protocol, &settings)
                            .map_err(|_| config::ConfigError::Invalid {
                                field: "provider.models.settings".into(),
                                reason: "unsupported wire settings".into(),
                            })?;
                    }
                    request.wire.protocol = protocol;
                    if request.subscription() && protocol != provider::protocol::Protocol::Responses
                    {
                        request.wire.unsupported = true;
                    }
                    // Go uses the native Messages x-api-key scheme; custom
                    // authToken/Bearer metadata must not override that policy.
                    if catalog.provider == PROVIDER {
                        request.wire.messages_bearer = false;
                    }
                    {
                        let entry = config::ProviderEntry {
                            npm: package.map(str::to_owned),
                            name: None,
                            env: Vec::new(),
                            options: Default::default(),
                            models: BTreeMap::from([(id.clone(), model.clone())]),
                        };
                        let metadata = config::provider_wire(&catalog.provider, &entry)?;
                        request.wire.chat = metadata.chat;
                        request.wire.chronology = metadata.chronology;
                    }
                    Ok(())
                })
                .is_ok();
            request.wire.unsupported |= !supported;
            requests.insert(key, request);
        }
    }
    requests
}
