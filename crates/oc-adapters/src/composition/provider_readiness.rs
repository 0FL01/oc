//! One admitted provider's transient catalog/request facts, never a client registry.
use super::*;
use oc_core::queries::{ProviderReadiness, ProviderStatus};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub(crate) struct ProviderState {
    service: String,
    source: String,
    credential: bool,
    unsupported_oauth: bool,
    unsupported_wire: bool,
    pub(crate) catalog_status: ProviderStatus,
    diagnostic: Option<ServiceDiagnostic>,
    /// Auth rejection observed for the current admitted binding, not a new candidate key.
    auth_failure: Option<ServiceDiagnostic>,
}

impl ProviderState {
    pub(super) fn public_pending(&mut self) {
        self.catalog_status = ProviderStatus::Pending;
        self.diagnostic = None;
        self.auth_failure = None;
    }

    pub(super) fn finish_public(&mut self, outcome: &discovery::DiscoveryOutcome) {
        self.finish(outcome);
        // A credential-free public source's HTTP status is never an auth fact
        // about the separately captured paid provider connection.
        self.auth_failure = None;
        if outcome.failure.is_some() {
            self.catalog_status = ProviderStatus::Failed;
        }
    }

    pub(crate) fn set_unsupported(&mut self) {
        self.credential = false;
        self.unsupported_wire = true;
    }
    pub(crate) fn new(provider: &str, source: &str, credential: bool, dynamic: bool) -> Self {
        let mut state = Self {
            service: format!("provider-{:x}", Sha256::digest(provider.as_bytes())),
            source: config::mcp::safe_source_id(source),
            credential,
            unsupported_oauth: false,
            unsupported_wire: false,
            catalog_status: if dynamic && credential {
                ProviderStatus::Pending
            } else {
                ProviderStatus::Ready
            },
            diagnostic: None,
            auth_failure: None,
        };
        if !credential {
            state.catalog_status = ProviderStatus::Unavailable;
            state.diagnostic = Some(state.diagnostic(
                "apiKey",
                ServiceStage::Config,
                ServiceCode::MissingCredential,
                ServiceAction::ReviewConfiguration,
            ));
        }
        state
    }

    pub(crate) fn with_auth_policy(mut self, policy: crate::auth::AuthPolicy) -> Self {
        self.unsupported_oauth = policy == crate::auth::AuthPolicy::OAuth;
        self
    }

    pub(crate) fn set_auth(&mut self, ready: bool, policy: crate::auth::AuthPolicy) {
        self.credential = ready;
        self.unsupported_oauth = policy == crate::auth::AuthPolicy::OAuth;
        if self.catalog_status == ProviderStatus::Unavailable
            && self
                .diagnostic
                .as_ref()
                .is_some_and(|d| d.code == ServiceCode::MissingCredential)
        {
            self.catalog_status = ProviderStatus::Ready;
            self.diagnostic = None;
        }
    }

    fn diagnostic(
        &self,
        field: &str,
        stage: ServiceStage,
        code: ServiceCode,
        action: ServiceAction,
    ) -> ServiceDiagnostic {
        ServiceDiagnostic {
            kind: ServiceKind::Provider,
            service: self.service.clone(),
            source: self.source.clone(),
            field: if field == "models" {
                vec!["provider".into(), self.service.clone(), "models".into()]
            } else {
                vec![
                    "provider".into(),
                    self.service.clone(),
                    "options".into(),
                    field.into(),
                ]
            },
            stage,
            code,
            action,
        }
    }

    pub(crate) fn finish(&mut self, outcome: &discovery::DiscoveryOutcome) {
        use discovery::DiscoveryFailure as Failure;
        self.catalog_status = if outcome.replaced {
            ProviderStatus::Ready
        } else {
            ProviderStatus::Failed
        };
        self.diagnostic = outcome.failure.map(|failure| {
            let (code, action) = match failure {
                Failure::Unauthorized => (
                    ServiceCode::Unauthorized,
                    ServiceAction::ReviewConfiguration,
                ),
                Failure::Forbidden => (ServiceCode::Forbidden, ServiceAction::ReviewConfiguration),
                Failure::Http => (ServiceCode::HttpFailure, ServiceAction::RefreshCatalog),
                Failure::Network => (ServiceCode::ConnectionFailed, ServiceAction::RefreshCatalog),
                Failure::InvalidResponse | Failure::EmptyResponse => {
                    (ServiceCode::InvalidCatalog, ServiceAction::RefreshCatalog)
                }
                Failure::InvalidConfig => (
                    ServiceCode::InvalidConfig,
                    ServiceAction::ReviewConfiguration,
                ),
                Failure::Cancelled => (ServiceCode::Cancelled, ServiceAction::WaitForProvider),
            };
            self.diagnostic("models", ServiceStage::ModelCatalog, code, action)
        });
        if outcome.replaced {
            self.auth_failure = None;
        } else if matches!(
            outcome.failure,
            Some(Failure::Unauthorized | Failure::Forbidden)
        ) {
            self.auth_failure = self.diagnostic.clone();
        }
    }

    pub(crate) fn for_model(&self, model: &str, present: bool) -> ProviderReadiness {
        let status = if !self.credential {
            ProviderStatus::Unavailable
        } else if self.auth_failure.is_some() {
            ProviderStatus::Failed
        } else if self.catalog_status == ProviderStatus::Pending {
            // The first native attempt also supplies auth facts. A configured
            // row cannot race it and accept a request before a cold 401/403.
            ProviderStatus::Pending
        } else if present {
            ProviderStatus::Ready
        } else {
            match self.catalog_status {
                ProviderStatus::Pending => ProviderStatus::Pending,
                ProviderStatus::Failed => ProviderStatus::Failed,
                _ => ProviderStatus::Unavailable,
            }
        };
        let diagnostic = if !self.credential {
            Some(self.diagnostic(
                if self.unsupported_wire {
                    "package"
                } else if self.unsupported_oauth {
                    "authPolicy"
                } else {
                    "apiKey"
                },
                ServiceStage::Config,
                if self.unsupported_oauth || self.unsupported_wire {
                    ServiceCode::UnsupportedCapability
                } else {
                    ServiceCode::MissingCredential
                },
                ServiceAction::ReviewConfiguration,
            ))
        } else if self.auth_failure.is_some() {
            self.auth_failure.clone()
        } else if self.diagnostic.is_some() {
            self.diagnostic.clone()
        } else if status == ProviderStatus::Pending {
            Some(self.diagnostic(
                "models",
                ServiceStage::ModelCatalog,
                ServiceCode::ProviderPending,
                ServiceAction::WaitForProvider,
            ))
        } else if !present {
            Some(self.diagnostic(
                "models",
                ServiceStage::Admission,
                ServiceCode::ModelUnavailable,
                ServiceAction::SelectModel,
            ))
        } else {
            None
        };
        ProviderReadiness {
            service: self.service.clone(),
            model: format!("model-{:x}", Sha256::digest(model.as_bytes())),
            status,
            catalog_status: self.catalog_status,
            diagnostic,
        }
    }

    pub(crate) fn retain_failed_attempt(&mut self, attempted: &Self, same_binding: bool) {
        self.catalog_status = attempted.catalog_status;
        self.diagnostic = attempted.diagnostic.clone();
        if same_binding && attempted.auth_failure.is_some() {
            self.auth_failure = attempted.auth_failure.clone();
        }
    }

    pub(crate) fn admit(&self, model: &str, present: bool) -> Result<(), ServiceDiagnostic> {
        let readiness = self.for_model(model, present);
        if readiness.status == ProviderStatus::Ready {
            Ok(())
        } else {
            Err(readiness
                .diagnostic
                .expect("unavailable provider has a safe cause"))
        }
    }
}

impl Composition {
    pub(crate) fn selected_provider_readiness(
        &self,
        model: &str,
        variant: Option<&str>,
    ) -> ProviderReadiness {
        let provider = self.provider.for_selection(model, variant);
        let mut state = self.provider_state.clone();
        state.set_auth(provider.auth_ready(), provider.wire.auth_policy);
        if provider.wire.unsupported {
            state.set_unsupported();
        }
        state.for_model(model, self.catalog.models.contains_key(model))
    }
    /// Resolve once per admitted generation, before any discovery or runtime publication.
    /// Prepared requests retain this immutable binding across later account mutations.
    pub(crate) fn resolve_credentials(
        &mut self,
        db: &crate::storage::Db,
    ) -> Result<(), LoadFailure> {
        let id = &self.catalog.provider;
        let source = self
            .generation
            .provenance
            .get(&format!("provider.{id}"))
            .map(String::as_str)
            .unwrap_or("native config");
        // Each template still contains configured inputs, not the already-resolved
        // parent key. An endpoint-changing model must never inherit parent auth.
        for request in self.provider.wire.requests.values_mut() {
            let scope = crate::auth::AuthScope::admit(id, &request.base_url)
                .map_err(|_| invalid(source, &["provider", "models", "settings", "baseURL"]))?;
            let competing = request.headers.keys().any(|name| {
                name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
            });
            scope
                .resolve(
                    db,
                    request.wire.auth_policy,
                    Some(&request.api_key),
                    competing,
                    || self.parent_env.get("OPENCODE_API_KEY").cloned(),
                )
                .map_err(|error| {
                    failure(
                        source,
                        &["provider", "models", "settings", "authPolicy"],
                        ServiceStage::Config,
                        if matches!(error, crate::auth::AuthError::Storage(_)) {
                            ServiceCode::StorageUnavailable
                        } else {
                            ServiceCode::InvalidConfig
                        },
                    )
                })?
                .apply_to(request);
        }
        if let Some(entry) = self.generation.providers.get_mut(id) {
            // Redaction-only captured values; MCP credential inheritance does not
            // inspect this map, and public config Debug never renders its contents.
            entry.options.request_bindings = self.provider.wire.requests.clone();
        }
        let scope = crate::auth::AuthScope::admit(id, &self.provider.base_url)
            .map_err(|_| invalid(source, &["provider", "options", "baseURL"]))?;
        let competing = self.provider.headers.keys().any(|name| {
            name.eq_ignore_ascii_case("authorization") || name.eq_ignore_ascii_case("x-api-key")
        });
        let auth = scope
            .resolve(
                db,
                self.provider.wire.auth_policy,
                Some(&self.provider.api_key),
                competing,
                || self.parent_env.get("OPENCODE_API_KEY").cloned(),
            )
            .map_err(|error| {
                failure(
                    source,
                    &["provider", "options", "authPolicy"],
                    ServiceStage::Config,
                    if matches!(error, crate::auth::AuthError::Storage(_)) {
                        ServiceCode::StorageUnavailable
                    } else {
                        ServiceCode::InvalidConfig
                    },
                )
            })?;
        auth.apply_to(&mut self.provider);
        if let Some(entry) = self.generation.providers.get_mut(id) {
            entry.options.redaction_material = std::iter::once(self.provider.api_key.clone())
                .filter(|value| !value.is_empty())
                .collect();
        }
        self.provider_state = ProviderState::new(
            id,
            source,
            self.provider.auth_ready(),
            id == discovery::PROVIDER_ID,
        )
        .with_auth_policy(self.provider.wire.auth_policy);
        self.tui_chrome.provider =
            Some(self.selected_provider_readiness(&self.model_id, self.variant.as_deref()));
        Ok(())
    }

    /// Only the original bounded discovery loop performs retries/negotiation.
    pub(crate) async fn refresh_provider(&mut self) -> Result<(), LoadFailure> {
        if self.catalog.provider == crate::models_dev::PROVIDER {
            return Ok(());
        }
        if self.provider_state.catalog_status != ProviderStatus::Pending {
            return Ok(());
        }
        let outcome =
            Self::discover_provider(self.provider.clone(), self.catalog.models.clone()).await?;
        self.accept_provider_catalog(outcome);
        Ok(())
    }

    pub(crate) async fn discover_provider(
        provider: provider::ResponsesConfig,
        models: BTreeMap<String, serde_json::Value>,
    ) -> Result<discovery::DiscoveryOutcome, LoadFailure> {
        let client = match discovery::ReqwestDiscoveryClient::captured(&provider) {
            Ok(client) => client,
            Err(error) => {
                return Ok(discovery::DiscoveryOutcome {
                    models,
                    warnings: Vec::new(),
                    attempts: 0,
                    replaced: false,
                    failure: Some(discovery::DiscoveryFailure::from(&error)),
                });
            }
        };
        Ok(discovery::refresh_provider(&client, &provider, &models, &AtomicBool::new(false)).await)
    }

    pub(crate) fn accept_provider_catalog(&mut self, outcome: discovery::DiscoveryOutcome) {
        if outcome.replaced {
            trace::log(
                "discovery.ok",
                &format!(
                    "models={} selected_present={}",
                    outcome.models.len(),
                    outcome.models.contains_key(&self.model_id)
                ),
            );
        } else if let Some(failure) = outcome.failure {
            trace::log("discovery.fail", &format!("class={failure:?}"));
        }
        if self.catalog.provider == crate::models_dev::PROVIDER {
            self.provider_state.finish_public(&outcome);
        } else {
            self.provider_state.finish(&outcome);
        }
        self.catalog.models = outcome.models;
        if self.catalog.provider == crate::models_dev::PROVIDER {
            self.capture_public_bindings();
        }
        self.generation.warnings.extend(outcome.warnings);
        self.tui_chrome.provider =
            Some(self.selected_provider_readiness(&self.model_id, self.variant.as_deref()));
    }
}
