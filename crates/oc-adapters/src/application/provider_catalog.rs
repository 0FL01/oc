//! Independently owned configured/public catalog jobs. No storage/client registry.
use super::*;
use oc_core::queries::ProviderStatus;

#[cfg(test)]
#[path = "provider_catalog/tests.rs"]
mod tests;

pub(super) struct ProviderWork {
    task: Option<JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>>>,
    public_task:
        Option<JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>>>,
    public_provider: &'static str,
}

pub(super) enum CatalogOutcome {
    Selected(crate::discovery::DiscoveryOutcome),
    Public(crate::discovery::DiscoveryOutcome),
    OpenAi(crate::discovery::DiscoveryOutcome),
}

impl ProviderWork {
    pub(super) fn start(composition: &Composition, db: &Db) -> Self {
        Self::start_requested(composition, db, None)
    }

    pub(super) fn start_requested(
        composition: &Composition,
        db: &Db,
        requested: Option<&str>,
    ) -> Self {
        let task = if composition.go_catalog.is_some() {
            let db = db.shared_handle();
            let id = composition.catalog.provider.clone();
            let models = composition.generation.providers[&id].models.clone();
            Some(tokio::spawn(refresh_public(db, id, models)))
        } else if composition.provider_state.catalog_status == ProviderStatus::Pending {
            let provider = composition.provider.clone();
            let models = composition.catalog.models.clone();
            Some(tokio::spawn(Composition::discover_provider(
                provider, models,
            )))
        } else {
            None
        };
        // Default background work remains Go's existing public GET. OpenAI uses
        // that same cached response, or an explicit catalog request, not a new
        // unrequested network job in unrelated/offline compositions.
        let public_provider = if requested == Some(crate::models_dev::OPENAI) {
            crate::models_dev::OPENAI
        } else {
            crate::models_dev::PROVIDER
        };
        let public_task = (composition.catalog.provider != public_provider
            && composition.public_provider(public_provider))
        .then(|| composition.provider_views.get(public_provider))
        .flatten()
        .map(|view| {
            let db = db.shared_handle();
            let models = view.entry.models.clone();
            tokio::spawn(refresh_public(db, public_provider.into(), models))
        });
        Self {
            task,
            public_task,
            public_provider,
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.task.is_some() || self.public_task.is_some()
    }

    pub(super) fn selected_pending(&self) -> bool {
        self.task.is_some()
    }

    pub(super) async fn wait(
        &mut self,
    ) -> Result<CatalogOutcome, oc_core::queries::ServiceDiagnostic> {
        async fn wait_task(
            task: &mut Option<
                JoinHandle<Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure>>,
            >,
        ) -> Result<crate::discovery::DiscoveryOutcome, oc_core::queries::ServiceDiagnostic>
        {
            let result = match task.as_mut() {
                Some(task) => task.await,
                None => std::future::pending().await,
            };
            task.take();
            result
                .map_err(|_| worker_failure(oc_core::queries::ServiceStage::ModelCatalog))?
                .map_err(|failure| match failure {
                    composition::LoadFailure::Configuration(diagnostic) => diagnostic,
                })
        }
        let openai = self.public_provider == crate::models_dev::OPENAI;
        tokio::select! {
            result = wait_task(&mut self.task) => result.map(CatalogOutcome::Selected),
            result = wait_task(&mut self.public_task) => result.map(|o| if openai { CatalogOutcome::OpenAi(o) } else { CatalogOutcome::Public(o) }),
        }
    }

    pub(super) async fn stop(&mut self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        let tasks = [self.task.take(), self.public_task.take()];
        for task in tasks.iter().flatten() {
            task.abort();
        }
        let mut failure = None;
        for task in tasks.into_iter().flatten() {
            match task.await {
                // This owns only a read-only GET. Retired metadata is discarded;
                // the explicit abort's cancellation is successful cleanup.
                Ok(Ok(_)) => {}
                Err(error) if error.is_cancelled() => {}
                Err(_) => failure = Some(worker_failure(oc_core::queries::ServiceStage::Cleanup)),
                Ok(Err(composition::LoadFailure::Configuration(diagnostic))) => {
                    failure = Some(diagnostic);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}

async fn refresh_public(
    db: Db,
    provider: String,
    models: BTreeMap<String, serde_json::Value>,
) -> Result<crate::discovery::DiscoveryOutcome, composition::LoadFailure> {
    let owner = db.public_catalog();
    let read =
        match crate::discovery::ReqwestDiscoveryClient::new(std::time::Duration::from_secs(10)) {
            Ok(client) => {
                owner
                    .refresh_provider(
                        &db,
                        &client,
                        &provider,
                        &models,
                        composition::go_catalog::now_ms(),
                        false,
                    )
                    .await
            }
            Err(_) => {
                let mut read = owner.read_provider(&provider, &models).await;
                read.failure = Some(crate::discovery::DiscoveryFailure::Network);
                read
            }
        };
    Ok(composition::go_catalog::outcome(read))
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
        if let Some(task) = &self.public_task {
            task.abort();
        }
    }
}
