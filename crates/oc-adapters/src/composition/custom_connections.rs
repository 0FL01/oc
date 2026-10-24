//! Attach user-created, scoped connections after file admission, before credentials.
use super::*;
use crate::storage::provider_connections::{CatalogState, CustomConnection};
use oc_core::queries::NativeProtocol;

#[cfg(test)]
mod tests;

pub(crate) const SOURCE: &str = "native custom connection";

pub(super) fn reserved(sources: &[config::Source]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for source in sources {
        if let Ok(value) = config::parse_jsonc(&source.text, &source.path) {
            for root in ["provider", "providers"] {
                if let Some(map) = value.get(root).and_then(serde_json::Value::as_object) {
                    ids.extend(map.keys().cloned());
                }
            }
        }
    }
    ids
}

impl Composition {
    pub(crate) fn custom_refresh_pending(&mut self, id: &str) {
        if !self.custom_revisions.contains_key(id) {
            return;
        }
        // A pending read does not clear a known auth rejection for this binding.
        if self.catalog.provider == id {
            self.provider_state.catalog_status = oc_core::queries::ProviderStatus::Pending;
        } else if let Some(view) = self.provider_views.get_mut(id) {
            view.state.catalog_status = oc_core::queries::ProviderStatus::Pending;
        }
    }

    pub(crate) fn custom_creation_admitted(&self) -> bool {
        catalog::provider_filters(&self.mcp_activation.sources)
            .is_ok_and(|policy| policy.allows_unlisted())
    }

    pub(crate) fn custom_admitted(&self, id: &str) -> bool {
        let sources = &self.mcp_activation.sources;
        let Ok(policy) = catalog::provider_filters(sources) else {
            return false;
        };
        !reserved(sources).contains(id) && policy.allows(id)
    }

    pub(crate) fn attach_custom_connections(
        &mut self,
        db: &crate::storage::Db,
    ) -> Result<(), LoadFailure> {
        let connections = db.custom_providers().map_err(|_| {
            failure(
                SOURCE,
                &["connections"],
                ServiceStage::Admission,
                ServiceCode::StorageUnavailable,
            )
        })?;
        for row in connections {
            if self.custom_admitted(&row.id) {
                self.upsert_custom_connection(row)?;
            }
        }
        Ok(())
    }

    pub(crate) fn upsert_custom_connection(
        &mut self,
        row: CustomConnection,
    ) -> Result<(), LoadFailure> {
        let id = &row.id;
        if !self.custom_admitted(id) {
            return Err(invalid(SOURCE, &["provider"]));
        }
        let endpoint = crate::endpoint::EndpointBinding::admit(&row.base_url, true, SOURCE)
            .map_err(|_| invalid(SOURCE, &["endpoint"]))?;
        let scope = crate::auth::AuthScope::custom(id, &row.base_url)
            .map_err(|_| invalid(SOURCE, &["auth"]))?;
        let entry = config::ProviderEntry {
            npm: Some(
                match row.protocol {
                    NativeProtocol::Responses => "@ai-sdk/openai",
                    NativeProtocol::Chat => "@ai-sdk/openai-compatible",
                    NativeProtocol::Messages => "@ai-sdk/anthropic",
                }
                .into(),
            ),
            name: Some(row.name),
            env: Vec::new(),
            compatibility: Default::default(),
            models: row.models.clone(),
            options: config::ProviderOptions {
                base_url: row.base_url.clone(),
                auth_policy: row.auth,
                endpoint_source: Some(SOURCE.into()),
                endpoint_trusted: true,
                ..Default::default()
            },
        };
        let mut wire =
            config::provider_wire(id, &entry).map_err(|_| invalid(SOURCE, &["protocol"]))?;
        wire.endpoint = Some(endpoint);
        wire.custom_scope = Some(scope);
        let provider = provider::ResponsesConfig {
            base_url: row.base_url,
            api_key: String::new(),
            headers: BTreeMap::new(),
            timeout: None,
            header_timeout_ms: Some(crate::provider::HEADER_TIMEOUT_MS),
            chunk_timeout_ms: Some(provider::CHUNK_TIMEOUT_MS),
            connect_timeout: Duration::from_secs(10),
            allow_private: false,
            cache: Default::default(),
            wire,
        };
        let mut state =
            ProviderState::new(id, SOURCE, provider.auth_ready(), false).with_auth_policy(row.auth);
        if row.state == CatalogState::Missing {
            state.public_pending();
        } else {
            state.finish(&discovery::DiscoveryOutcome {
                models: BTreeMap::new(),
                warnings: Vec::new(),
                attempts: 0,
                replaced: row.state == CatalogState::Ready
                    && row.catalog_revision == Some(row.revision),
                failure: match row.state {
                    CatalogState::Unauthorized => Some(discovery::DiscoveryFailure::Unauthorized),
                    CatalogState::Forbidden => Some(discovery::DiscoveryFailure::Forbidden),
                    CatalogState::Ready if row.catalog_revision == Some(row.revision) => None,
                    CatalogState::Failed | CatalogState::Ready => {
                        Some(discovery::DiscoveryFailure::Network)
                    }
                    CatalogState::Missing => None,
                },
            });
        }
        let catalog = models::ModelCatalog {
            provider: id.clone(),
            models: row.models.into(),
        };
        self.custom_revisions.insert(id.clone(), row.revision);
        self.generation.providers.insert(id.clone(), entry.clone());
        self.generation
            .provenance
            .insert(format!("provider.{id}"), SOURCE.into());
        if self.catalog.provider == *id {
            self.catalog = catalog;
            self.provider = provider;
            self.provider_state = state;
        } else {
            self.provider_views.insert(
                id.clone(),
                provider_views::ProviderView {
                    entry,
                    catalog,
                    provider,
                    state,
                },
            );
        }
        Ok(())
    }
}
