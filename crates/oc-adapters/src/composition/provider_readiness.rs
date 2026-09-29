//! One admitted provider's transient catalog/request facts, never a client registry.
use super::*;
use oc_core::queries::{ProviderReadiness, ProviderStatus};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub(crate) struct ProviderState {
    service: String,
    source: String,
    credential: bool,
    pub(crate) catalog_status: ProviderStatus,
    diagnostic: Option<ServiceDiagnostic>,
    /// Auth rejection observed for the current admitted binding, not a new candidate key.
    auth_failure: Option<ServiceDiagnostic>,
}

impl ProviderState {
    pub(crate) fn new(provider: &str, source: &str, credential: bool, dynamic: bool) -> Self {
        let mut state = Self {
            service: format!("provider-{:x}", Sha256::digest(provider.as_bytes())),
            source: config::mcp::safe_source_id(source),
            credential,
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
                "apiKey",
                ServiceStage::Config,
                ServiceCode::MissingCredential,
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
    /// Only the original bounded discovery loop performs retries/negotiation.
    pub(crate) async fn refresh_provider(&mut self) -> Result<(), LoadFailure> {
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
        let client = match discovery::ReqwestDiscoveryClient::new(provider.connect_timeout) {
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
        Ok(discovery::refresh(
            &discovery::RealClock,
            &client,
            &provider.base_url,
            &provider.api_key,
            &provider.headers,
            &models,
            &AtomicBool::new(false),
        )
        .await)
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
        self.provider_state.finish(&outcome);
        self.catalog.models = outcome.models;
        self.generation.warnings.extend(outcome.warnings);
        self.tui_chrome.provider = Some(self.provider_state.for_model(
            &self.model_id,
            self.catalog.models.contains_key(&self.model_id),
        ));
    }
}
