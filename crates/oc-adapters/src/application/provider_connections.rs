//! One requested custom/probe slot within ProviderWork; only the actor commits results.
use super::*;
use crate::custom_discovery::CustomCatalog;
use crate::discovery::{DiscoveryError, DiscoveryFailure};
use crate::storage::provider_connections::CatalogState;
use oc_core::queries::{
    CustomProviderAction, CustomProviderInput, CustomProviderReply, ProviderConnection,
};

#[cfg(test)]
mod tests;

type Reply = oneshot::Sender<Result<CustomProviderReply, CoreError>>;
type ResultCatalog = Result<CustomCatalog, DiscoveryError>;

enum Capture {
    Probe {
        revision: u64,
        input: CustomProviderInput,
        cancel: Arc<AtomicBool>,
        result: Option<ResultCatalog>,
        ack: Option<Reply>,
    },
    Refresh {
        provider: String,
        revision: i64,
    },
}

#[derive(Default)]
pub(super) struct Work {
    task: Option<JoinHandle<ResultCatalog>>,
    capture: Option<Capture>,
}

fn diagnostic(error: &DiscoveryError) -> oc_core::queries::ServiceDiagnostic {
    use oc_core::queries::{ServiceAction, ServiceCode, ServiceStage};
    crate::config::diagnostic::failure(
        composition::custom_connections::SOURCE,
        &["provider", "models"],
        ServiceStage::ModelCatalog,
        match DiscoveryFailure::from(error) {
            DiscoveryFailure::Unauthorized => ServiceCode::Unauthorized,
            DiscoveryFailure::Forbidden => ServiceCode::Forbidden,
            DiscoveryFailure::Cancelled => ServiceCode::Cancelled,
            DiscoveryFailure::InvalidConfig => ServiceCode::InvalidConfig,
            DiscoveryFailure::InvalidResponse | DiscoveryFailure::EmptyResponse => {
                ServiceCode::InvalidCatalog
            }
            DiscoveryFailure::Http => ServiceCode::HttpFailure,
            DiscoveryFailure::Network => ServiceCode::ConnectionFailed,
        },
        ServiceAction::RefreshCatalog,
    )
}

fn catalog_state(result: &ResultCatalog) -> CatalogState {
    match result {
        Ok(_) => CatalogState::Ready,
        Err(DiscoveryError::Http { status: 401 }) => CatalogState::Unauthorized,
        Err(DiscoveryError::Http { status: 403 }) => CatalogState::Forbidden,
        Err(_) => CatalogState::Failed,
    }
}

impl Work {
    pub(super) fn pending(&self) -> bool {
        self.task.is_some()
    }
    pub(super) fn has_probe(&self) -> bool {
        matches!(self.capture, Some(Capture::Probe { .. }))
    }

    pub(super) fn start_refresh(
        &mut self,
        composition: &Composition,
        provider: &str,
    ) -> Result<(), CoreError> {
        if self.pending() {
            return if matches!(&self.capture, Some(Capture::Refresh { provider: current, revision }) if current == provider && composition.custom_revisions.get(provider) == Some(revision))
            {
                Ok(())
            } else {
                Err(CoreError::TurnBusy)
            };
        }
        if self.has_probe() {
            return Err(CoreError::TurnBusy);
        }
        let revision = composition
            .custom_revisions
            .get(provider)
            .copied()
            .ok_or_else(|| app_error("custom connection unavailable"))?;
        let binding = composition
            .provider_for(provider)
            .cloned()
            .ok_or_else(|| app_error("custom connection unavailable"))?;
        self.capture = Some(Capture::Refresh {
            provider: provider.into(),
            revision,
        });
        self.task = Some(tokio::spawn(crate::custom_discovery::refresh_provider(
            binding,
        )));
        Ok(())
    }

    pub(super) async fn wait(
        &mut self,
    ) -> Result<ResultCatalog, oc_core::queries::ServiceDiagnostic> {
        let result = match self.task.as_mut() {
            Some(task) => task.await,
            None => std::future::pending().await,
        };
        self.task.take();
        result.map_err(|_| {
            super::provider_catalog::worker_failure(oc_core::queries::ServiceStage::ModelCatalog)
        })
    }

    pub(super) async fn stop(&mut self) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        let cleanup = if let Some(task) = self.task.take() {
            task.abort();
            if let Err(error) = task.await
                && !error.is_cancelled()
            {
                Err(super::provider_catalog::worker_failure(
                    oc_core::queries::ServiceStage::Cleanup,
                ))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        if let Some(Capture::Probe { ack: Some(ack), .. }) = self.capture.take() {
            let _ = ack.send(Err(app_error("custom connection cancelled")));
        }
        cleanup
    }

    pub(super) async fn apply(
        &mut self,
        db: &Db,
        runtime: &Runtime<'_>,
        composition: &mut Composition,
        action: CustomProviderAction,
        ack: Reply,
        events: &broadcast::Sender<CoreEvent>,
    ) {
        if runtime.turn_active() {
            let _ = ack.send(Err(CoreError::TurnBusy));
            return;
        }
        let action = match action {
            CustomProviderAction::Retry { revision } => {
                let valid = !self.pending()
                    && matches!(&self.capture, Some(Capture::Probe { revision: current, cancel, result: Some(_), .. }) if *current == revision && !cancel.load(Ordering::SeqCst));
                if !valid {
                    let _ = ack.send(Err(app_error("custom form expired")));
                    return;
                }
                let Some(Capture::Probe { input, cancel, .. }) = self.capture.take() else {
                    unreachable!()
                };
                CustomProviderAction::Probe { input, cancel }
            }
            action => action,
        };
        match action {
            CustomProviderAction::Probe { input, cancel } => {
                // Replacing a form retires its old input/GET even when the new
                // candidate is invalid. Never strand an older receipt/key.
                if let Err(d) = self.stop().await {
                    let _ = ack.send(Err(CoreError::Diagnostic(d)));
                    return;
                }
                if !composition.custom_creation_admitted() {
                    let _ = ack.send(Err(app_error(
                        "provider whitelist forbids a new custom connection",
                    )));
                    return;
                }
                if let Err(error) = crate::storage::provider_connections::admit_input(&input) {
                    let _ = ack.send(Err(query_storage_error(db, error)));
                    return;
                }
                static NEXT: AtomicU64 = AtomicU64::new(1);
                let Ok(revision) =
                    NEXT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
                else {
                    let _ = ack.send(Err(app_error("custom form revision exhausted")));
                    return;
                };
                let captured = input.clone();
                let stopped = cancel.clone();
                self.task = Some(tokio::spawn(async move {
                    tokio::select! {
                        result = crate::custom_discovery::probe(&captured, None) => result,
                        () = async { while !stopped.load(Ordering::SeqCst) { tokio::time::sleep(std::time::Duration::from_millis(5)).await; } } => Err(DiscoveryError::Cancelled),
                    }
                }));
                self.capture = Some(Capture::Probe {
                    revision,
                    input,
                    cancel,
                    result: None,
                    ack: Some(ack),
                });
            }
            CustomProviderAction::Cancel { revision } => {
                let matches = matches!(self.capture, Some(Capture::Probe { revision: current, .. }) if current == revision);
                let result = if matches {
                    self.stop()
                        .await
                        .map_err(CoreError::Diagnostic)
                        .map(|_| CustomProviderReply::Cancelled)
                } else {
                    // Closing an already retired form is idempotent, but can
                    // never clear another form's captured input.
                    Ok(CustomProviderReply::Cancelled)
                };
                let _ = ack.send(result);
            }
            CustomProviderAction::Refresh { provider } => {
                let result = if self.has_probe() {
                    Err(CoreError::TurnBusy)
                } else {
                    self.start_refresh(composition, &provider).and_then(|()| {
                        composition.custom_refresh_pending(&provider);
                        runtime
                            .publish_provider_state(
                                composition.provider_state.clone(),
                                composition.request_provider(&composition.catalog.provider),
                            )
                            .map_err(runtime_error)?;
                        let _ = events.send(CoreEvent::ProviderChanged);
                        Ok(CustomProviderReply::Refreshing)
                    })
                };
                let _ = ack.send(result);
            }
            CustomProviderAction::Save { revision, protocol } => {
                if self.pending() {
                    let _ = ack.send(Err(CoreError::TurnBusy));
                    return;
                }
                let valid = matches!(&self.capture, Some(Capture::Probe { revision: current, cancel, result: Some(_), .. }) if *current == revision && !cancel.load(Ordering::SeqCst));
                if !valid {
                    let _ = ack.send(Err(app_error("custom form expired")));
                    return;
                }
                // Consume before effects: repeated Enter cannot create a second connection.
                let Some(Capture::Probe {
                    input,
                    result: Some(result),
                    ..
                }) = self.capture.take()
                else {
                    unreachable!()
                };
                let saved = async {
                    let mut row = db
                        .create_custom_provider(&input, protocol, |id| {
                            composition.custom_admitted(id)
                        })
                        .map_err(|e| query_storage_error(db, e))?;
                    row.state = catalog_state(&result);
                    if let Ok(catalog) = result {
                        row.models = catalog.models;
                        row.catalog_revision = Some(row.revision);
                    }
                    db.save_custom_catalog(&row.id, row.revision, &row.models, row.state)
                        .map_err(|e| query_storage_error(db, e))?;
                    let reply = ProviderConnection {
                        provider: row.id.clone(),
                        name: row.name.clone(),
                        summary: Vec::new(),
                        source: match row.auth {
                            crate::auth::AuthPolicy::Key | crate::auth::AuthPolicy::OAuth => {
                                oc_core::queries::AccountAuthSource::Stored
                            }
                            crate::auth::AuthPolicy::None => {
                                oc_core::queries::AccountAuthSource::Anonymous
                            }
                        },
                    };
                    composition.upsert_custom_connection(row).map_err(
                        |composition::LoadFailure::Configuration(d)| CoreError::Diagnostic(d),
                    )?;
                    composition.refresh_credential_preview(db).await.map_err(
                        |composition::LoadFailure::Configuration(d)| CoreError::Diagnostic(d),
                    )?;
                    runtime
                        .publish_custom_provider(&reply.provider, &composition.generation)
                        .map_err(runtime_error)?;
                    runtime
                        .publish_provider_state(
                            composition.provider_state.clone(),
                            composition.request_provider(&composition.catalog.provider),
                        )
                        .map_err(runtime_error)?;
                    Ok(CustomProviderReply::Saved(reply))
                }
                .await;
                if saved.is_ok() {
                    let _ = events.send(CoreEvent::ProviderChanged);
                }
                let _ = ack.send(saved);
            }
            CustomProviderAction::Retry { .. } => unreachable!("retry captured above"),
        }
    }

    pub(super) async fn finish(
        &mut self,
        db: &Db,
        runtime: &Runtime<'_>,
        composition: &mut Composition,
        result: ResultCatalog,
    ) -> Result<bool, oc_core::queries::ServiceDiagnostic> {
        match self.capture.take() {
            Some(Capture::Probe {
                revision,
                input,
                cancel,
                ack,
                ..
            }) => {
                if cancel.load(Ordering::SeqCst) {
                    if let Some(ack) = ack {
                        let _ = ack.send(Err(app_error("custom connection cancelled")));
                    }
                    return Ok(false);
                }
                let reply = CustomProviderReply::Probed {
                    revision,
                    model_count: result.as_ref().map(|c| c.models.len()).unwrap_or(0),
                    suggested: result.as_ref().ok().and_then(|c| c.suggested),
                    diagnostic: result.as_ref().err().map(diagnostic),
                };
                if ack.is_some_and(|ack| ack.send(Ok(reply)).is_ok()) {
                    self.capture = Some(Capture::Probe {
                        revision,
                        input,
                        cancel,
                        result: Some(result),
                        ack: None,
                    });
                }
                Ok(false)
            }
            Some(Capture::Refresh { provider, revision }) => {
                if composition.custom_revisions.get(&provider) != Some(&revision)
                    || !composition.custom_admitted(&provider)
                {
                    return Ok(false);
                }
                let models = result
                    .as_ref()
                    .map(|c| c.models.clone())
                    .unwrap_or_default();
                if !db
                    .save_custom_catalog(&provider, revision, &models, catalog_state(&result))
                    .map_err(|e| storage_diagnostic(db.root(), &e))?
                {
                    return Ok(false);
                }
                composition
                    .attach_custom_connections(db)
                    .map_err(|composition::LoadFailure::Configuration(d)| d)?;
                composition
                    .refresh_credential_preview(db)
                    .await
                    .map_err(|composition::LoadFailure::Configuration(d)| d)?;
                runtime
                    .publish_custom_provider(&provider, &composition.generation)
                    .map_err(|e| runtime_issue(runtime.location(), &["provider"], &e).diagnostic)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}
