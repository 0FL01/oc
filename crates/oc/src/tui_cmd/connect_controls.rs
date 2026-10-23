//! One mounted connection surface; endpoint IO never blocks terminal input.
use super::*;
use oc_core::queries::{
    AuthMethod, CustomProviderAction, CustomProviderReply, ProviderAccounts, ProviderConnection,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

enum Outcome {
    Providers(Vec<ProviderConnection>),
    Methods(Vec<AuthMethod>),
    Accounts {
        snapshot: ProviderAccounts,
        methods: Result<Vec<AuthMethod>, CoreError>,
        catalog: Box<Result<CatalogSnapshot, CoreError>>,
        canonical: Option<Box<Result<CatalogSnapshot, CoreError>>>,
    },
    Custom {
        reply: CustomProviderReply,
        catalog: Option<Box<Result<CatalogSnapshot, CoreError>>>,
        canonical: Option<Box<Result<CatalogSnapshot, CoreError>>>,
    },
    Closed,
}

enum Kind {
    Providers,
    Methods(String),
    Accounts,
    Custom,
    Close,
}

struct Pending {
    revision: u64,
    kind: Kind,
    task: tokio::task::JoinHandle<Result<Outcome, CoreError>>,
}

struct Probe {
    revision: u64,
    cancel: Arc<AtomicBool>,
    receipt: Option<u64>,
}

#[derive(Default)]
pub(super) struct Controls {
    job: Option<Pending>,
    probe: Option<Probe>,
    cleanup_failed: bool,
}

impl Controls {
    pub(super) fn pending(&self) -> bool {
        self.job.is_some()
    }

    pub(super) fn ready(&self) -> bool {
        self.job.as_ref().is_some_and(|job| job.task.is_finished())
    }

    pub(super) fn start(
        &mut self,
        app: &CoreApp,
        state: &mut TuiState,
        intent: PanelIntent,
    ) -> Result<(), String> {
        let revision = match &intent {
            PanelIntent::LoadProviderConnections { revision }
            | PanelIntent::LoadAuthMethods { revision, .. }
            | PanelIntent::ProviderAccounts { revision, .. }
            | PanelIntent::ProbeCustomProvider { revision, .. }
            | PanelIntent::RetryCustomProvider { revision, .. }
            | PanelIntent::SaveCustomProvider { revision, .. } => *revision,
            _ => return Err("invalid connection operation".into()),
        };
        if state.account_surface_revision() != Some(revision) {
            return Ok(());
        }
        if self.job.is_some() {
            match &intent {
                PanelIntent::LoadProviderConnections { .. } => {
                    state.apply_provider_connections(Err(()))
                }
                PanelIntent::LoadAuthMethods { provider, .. } => {
                    state.apply_auth_methods(provider, revision, Err(()))
                }
                PanelIntent::ProviderAccounts { .. } => {
                    state.apply_provider_accounts(Err(()));
                }
                _ => {
                    state.apply_custom_provider(revision, Err(()));
                }
            }
            return Err("connection operation pending".into());
        }
        let owner = app.clone();
        let session = state.attached_session().cloned();
        let home = state.home;
        let (revision, kind, task) = match intent {
            PanelIntent::LoadProviderConnections { revision } => (
                revision,
                Kind::Providers,
                oc_core::wake::spawn(async move {
                    owner.provider_connections().await.map(Outcome::Providers)
                }),
            ),
            PanelIntent::LoadAuthMethods { provider, revision } => {
                let target = provider.clone();
                (
                    revision,
                    Kind::Methods(provider),
                    oc_core::wake::spawn(async move {
                        owner.auth_methods(target).await.map(Outcome::Methods)
                    }),
                )
            }
            PanelIntent::ProviderAccounts {
                provider,
                revision,
                action,
            } => (
                revision,
                Kind::Accounts,
                oc_core::wake::spawn(async move {
                    let changed = action.is_some();
                    let snapshot = owner.provider_accounts(provider.clone(), action).await?;
                    // Publish the mutation's current facts before its explicit
                    // ACK feedback. A later same-cause catalog must not replace
                    // the final-account disconnect toast before it can paint.
                    let canonical = if changed {
                        Some(Box::new(match session {
                            Some(session) => {
                                owner
                                    .session_selection(session, home, SelectionAction::Current)
                                    .await
                            }
                            None => owner.home_selection(SelectionAction::Current).await,
                        }))
                    } else {
                        None
                    };
                    let methods = if snapshot.accounts.is_empty() {
                        owner.auth_methods(provider.clone()).await
                    } else {
                        Ok(Vec::new())
                    };
                    // Account ACK is already durable. A catalog failure must not
                    // turn it into an ambiguous failed-save acknowledgement.
                    let catalog = Box::new(owner.provider_catalog(provider).await);
                    Ok(Outcome::Accounts {
                        snapshot,
                        methods,
                        catalog,
                        canonical,
                    })
                }),
            ),
            custom @ (PanelIntent::ProbeCustomProvider { .. }
            | PanelIntent::RetryCustomProvider { .. }
            | PanelIntent::SaveCustomProvider { .. }) => {
                let (revision, action) = match custom {
                    PanelIntent::ProbeCustomProvider { revision, input } => {
                        let Some(cancel) = state.custom_provider_cancel(revision) else {
                            return Ok(());
                        };
                        self.probe = Some(Probe {
                            revision,
                            cancel: cancel.clone(),
                            receipt: None,
                        });
                        (revision, CustomProviderAction::Probe { input, cancel })
                    }
                    PanelIntent::RetryCustomProvider { revision, receipt } => {
                        if self.probe.as_ref().is_none_or(|probe| {
                            probe.revision != revision || probe.receipt != Some(receipt)
                        }) {
                            return Err("custom connection form expired".into());
                        }
                        self.probe.as_mut().expect("matching probe").receipt = None;
                        (revision, CustomProviderAction::Retry { revision: receipt })
                    }
                    PanelIntent::SaveCustomProvider {
                        revision,
                        receipt,
                        protocol,
                    } => {
                        if self.probe.as_ref().is_none_or(|probe| {
                            probe.revision != revision || probe.receipt != Some(receipt)
                        }) {
                            return Err("custom connection form expired".into());
                        }
                        (
                            revision,
                            CustomProviderAction::Save {
                                revision: receipt,
                                protocol,
                            },
                        )
                    }
                    _ => unreachable!(),
                };
                (
                    revision,
                    Kind::Custom,
                    oc_core::wake::spawn(async move {
                        let reply = owner.custom_provider(action).await?;
                        let canonical = if matches!(&reply, CustomProviderReply::Saved(_)) {
                            Some(Box::new(match session {
                                Some(session) => {
                                    owner
                                        .session_selection(session, home, SelectionAction::Current)
                                        .await
                                }
                                None => owner.home_selection(SelectionAction::Current).await,
                            }))
                        } else {
                            None
                        };
                        let catalog = if let CustomProviderReply::Saved(connection) = &reply {
                            Some(Box::new(
                                owner.provider_catalog(connection.provider.clone()).await,
                            ))
                        } else {
                            None
                        };
                        Ok(Outcome::Custom {
                            reply,
                            catalog,
                            canonical,
                        })
                    }),
                )
            }
            _ => return Err("invalid connection operation".into()),
        };
        self.job = Some(Pending {
            revision,
            kind,
            task,
        });
        Ok(())
    }

    fn show_catalog(state: &mut TuiState, catalog: Result<CatalogSnapshot, CoreError>) {
        match catalog {
            Ok(snapshot) => state.apply_picker_catalog(snapshot),
            Err(_) => state.push_transient_note(
                "Connected; provider model view unavailable",
                NoteVariant::Error,
            ),
        }
    }

    pub(super) async fn sync(&mut self, app: &CoreApp, state: &mut TuiState) -> bool {
        let mut changed = false;
        if self.ready() {
            let Pending {
                revision,
                kind,
                task,
            } = self.job.take().expect("finished connection job");
            let result = task.await.unwrap_or(Err(CoreError::Shutdown));
            if let Ok(Outcome::Custom { reply, .. }) = &result {
                match reply {
                    CustomProviderReply::Probed {
                        revision: receipt, ..
                    } => {
                        if let Some(probe) = &mut self.probe {
                            probe.receipt = Some(*receipt);
                        }
                    }
                    CustomProviderReply::Saved(_) => self.probe = None,
                    _ => {}
                }
            }
            if matches!(kind, Kind::Close) {
                self.cleanup_failed |= result.is_err();
            } else if state.account_surface_revision() == Some(revision) {
                match result {
                    Ok(Outcome::Providers(providers)) => {
                        state.apply_provider_connections(Ok(providers))
                    }
                    Ok(Outcome::Methods(methods)) => {
                        if let Kind::Methods(provider) = kind {
                            state.apply_auth_methods(&provider, revision, Ok(methods));
                        }
                    }
                    Ok(Outcome::Accounts {
                        snapshot,
                        methods,
                        catalog,
                        canonical,
                    }) => {
                        if let Some(canonical) = canonical
                            && let Ok(canonical) = *canonical
                        {
                            state.apply_catalog_refresh(canonical);
                        }
                        if state.apply_provider_accounts(Ok(snapshot)) {
                            Self::show_catalog(state, *catalog);
                        } else if let Some((provider, current)) = state.account_methods_request() {
                            state.apply_auth_methods(&provider, current, methods.map_err(|_| ()));
                        }
                    }
                    Ok(Outcome::Custom {
                        reply,
                        catalog,
                        canonical,
                    }) => {
                        if let Some(canonical) = canonical
                            && let Ok(canonical) = *canonical
                        {
                            state.apply_catalog_refresh(canonical);
                        }
                        if state.apply_custom_provider(revision, Ok(reply)).is_some()
                            && let Some(catalog) = catalog
                        {
                            Self::show_catalog(state, *catalog);
                        }
                    }
                    Err(_) => match kind {
                        Kind::Providers => state.apply_provider_connections(Err(())),
                        Kind::Accounts => {
                            state.apply_provider_accounts(Err(()));
                        }
                        Kind::Methods(provider) => {
                            state.apply_auth_methods(&provider, revision, Err(()))
                        }
                        Kind::Custom => {
                            state.apply_custom_provider(revision, Err(()));
                        }
                        Kind::Close => unreachable!(),
                    },
                    Ok(Outcome::Closed) => {}
                }
                changed = true;
            }
        }
        if self.job.is_none()
            && self
                .probe
                .as_ref()
                .is_some_and(|probe| state.custom_provider_revision() != Some(probe.revision))
        {
            let probe = self.probe.take().expect("retired probe");
            probe.cancel.store(true, Ordering::SeqCst);
            if let Some(receipt) = probe.receipt {
                let owner = app.clone();
                self.job = Some(Pending {
                    revision: probe.revision,
                    kind: Kind::Close,
                    task: oc_core::wake::spawn(async move {
                        owner
                            .custom_provider(CustomProviderAction::Cancel { revision: receipt })
                            .await?;
                        Ok(Outcome::Closed)
                    }),
                });
            }
        }
        changed
    }

    pub(super) async fn shutdown(&mut self, app: &CoreApp) -> Result<(), String> {
        if let Some(probe) = &self.probe {
            probe.cancel.store(true, Ordering::SeqCst);
        }
        if let Some(Pending { task, .. }) = self.job.take() {
            match task.await.map_err(|_| "connection cleanup failed")? {
                Ok(Outcome::Custom {
                    reply: CustomProviderReply::Probed { revision, .. },
                    ..
                }) => {
                    if let Some(probe) = &mut self.probe {
                        probe.receipt = Some(revision);
                    }
                }
                Ok(Outcome::Custom {
                    reply: CustomProviderReply::Saved(_),
                    ..
                }) => self.probe = None,
                _ => {}
            }
        }
        if let Some(probe) = self.probe.take()
            && let Some(receipt) = probe.receipt
        {
            app.custom_provider(CustomProviderAction::Cancel { revision: receipt })
                .await
                .map_err(|_| "connection cleanup failed")?;
        }
        if self.cleanup_failed {
            Err("connection cleanup failed".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        if let Some(probe) = &self.probe {
            probe.cancel.store(true, Ordering::SeqCst);
        }
        if let Some(job) = &self.job {
            job.task.abort();
        }
        // Normal exit joins every request above; the application additionally
        // owns and joins the read-only endpoint task on all shutdown paths.
    }
}

#[cfg(test)]
mod tests;
