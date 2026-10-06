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
    pub(crate) async fn attach_public_catalog(&mut self, db: &crate::storage::Db) {
        if !self.generation.public_go_enabled {
            return;
        }
        let owner = db.public_catalog();
        if self.catalog.provider != PROVIDER {
            if let Some(view) = self.provider_views.get_mut(PROVIDER) {
                let read = owner.read(&view.entry.models).await;
                view.state.public_pending();
                view.state.finish_public(&outcome(read.clone()));
                view.catalog.models = read.models;
                view.provider.wire.requests =
                    public_bindings(&view.entry, &view.catalog, &view.provider);
            }
            return;
        }
        let local = &self.generation.providers[PROVIDER].models;
        let read = owner.read(local).await;
        self.go_catalog = Some(owner);
        self.provider_state.public_pending();
        if read.fetched_at_ms.is_some() {
            self.accept_provider_catalog(outcome(read));
        } else {
            self.catalog.models.clear();
        }
        self.tui_chrome.provider =
            Some(self.selected_provider_readiness(&self.model_id, self.variant.as_deref()));
    }

    pub(crate) async fn refresh_public_catalog(&mut self, db: &crate::storage::Db, force: bool) {
        let Some(owner) = self.go_catalog.clone() else {
            return;
        };
        let client = discovery::ReqwestDiscoveryClient::new(Duration::from_secs(10));
        let Ok(client) = client else {
            let mut read = owner
                .read(&self.generation.providers[PROVIDER].models)
                .await;
            read.failure = Some(discovery::DiscoveryFailure::Network);
            self.accept_provider_catalog(outcome(read));
            return;
        };
        let read = owner
            .refresh(
                db,
                &client,
                &self.generation.providers[PROVIDER].models,
                now_ms(),
                force,
            )
            .await;
        self.accept_provider_catalog(outcome(read));
    }

    pub(super) fn capture_public_bindings(&mut self) {
        self.provider.wire.requests = public_bindings(
            &self.generation.providers[PROVIDER],
            &self.catalog,
            &self.provider,
        );
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
            let supported = config::package_protocol(PROVIDER, package)
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
                    // Go auth is always Bearer, including the Messages endpoint.
                    request.wire.messages_bearer = true;
                    {
                        let entry = config::ProviderEntry {
                            npm: package.map(str::to_owned),
                            name: None,
                            env: Vec::new(),
                            options: Default::default(),
                            models: BTreeMap::from([(id.clone(), model.clone())]),
                        };
                        let metadata = config::provider_wire(PROVIDER, &entry)?;
                        request.wire.chat = metadata.chat;
                        request.wire.chronology = metadata.chronology;
                    }
                    Ok(())
                })
                .is_ok();
            request.wire.unsupported = !supported;
            requests.insert(key, request);
        }
    }
    requests
}
