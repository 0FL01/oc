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

    pub(super) async fn wait(&mut self) -> Result<crate::discovery::DiscoveryOutcome, String> {
        let result = self.task.as_mut().expect("pending provider work").await;
        self.task.take();
        result
            .map_err(|_| "native catalog worker failed".to_string())?
            .map_err(|failure| match failure {
                composition::LoadFailure::Configuration(detail) => detail,
            })
    }

    pub(super) async fn stop(&mut self) -> Result<(), String> {
        if let Some(task) = self.task.take() {
            task.abort();
            match task.await {
                // This owns only a read-only GET. Retired metadata is discarded;
                // the explicit abort's cancellation is successful cleanup.
                Ok(Ok(_)) => {}
                Err(error) if error.is_cancelled() => {}
                Err(_) => return Err("native catalog worker failed".into()),
                Ok(Err(composition::LoadFailure::Configuration(detail))) => return Err(detail),
            }
        }
        Ok(())
    }
}

impl Drop for ProviderWork {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
