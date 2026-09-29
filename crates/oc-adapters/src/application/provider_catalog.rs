//! The application's single optional cold catalog job. No storage/client registry.
use super::*;
use oc_core::queries::ProviderStatus;

#[cfg(test)]
#[path = "provider_catalog/tests.rs"]
mod tests;

pub(super) struct ProviderWork {
    task: Option<JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>>>,
}

impl ProviderWork {
    pub(super) fn start(composition: &Composition) -> Self {
        let task = if composition.provider_state.catalog_status == ProviderStatus::Pending {
            let provider = composition.provider.clone();
            let models = composition.catalog.models.clone();
            Some(tokio::spawn(Composition::discover_provider(
                provider, models,
            )))
        } else {
            None
        };
        Self { task }
    }

    pub(super) fn pending(&self) -> bool {
        self.task.is_some()
    }

    pub(super) async fn wait(
        &mut self,
    ) -> Result<crate::discovery::DiscoveryOutcome, oc_core::queries::ServiceDiagnostic> {
        let result = self.task.as_mut().expect("pending provider work").await;
        self.task.take();
        result
            .map_err(|_| worker_failure(oc_core::queries::ServiceStage::ModelCatalog))?
            .map_err(|failure| match failure {
                composition::LoadFailure::Configuration(diagnostic) => diagnostic,
            })
    }

    pub(super) async fn stop(&mut self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        if let Some(task) = self.task.take() {
            task.abort();
            match task.await {
                // This owns only a read-only GET. Retired metadata is discarded;
                // the explicit abort's cancellation is successful cleanup.
                Ok(Ok(_)) => {}
                Err(error) if error.is_cancelled() => {}
                Err(_) => return Err(worker_failure(oc_core::queries::ServiceStage::Cleanup)),
                Ok(Err(composition::LoadFailure::Configuration(diagnostic))) => {
                    return Err(diagnostic);
                }
            }
        }
        Ok(())
    }
}

fn worker_failure(stage: oc_core::queries::ServiceStage) -> oc_core::queries::ServiceDiagnostic {
    let mut diagnostic = crate::config::diagnostic::failure(
        "native provider worker",
        &["provider", "models"],
        stage,
        oc_core::queries::ServiceCode::RuntimeFailed,
        oc_core::queries::ServiceAction::RestartApplication,
    );
    diagnostic.kind = oc_core::queries::ServiceKind::Runtime;
    diagnostic
}

impl Drop for ProviderWork {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
