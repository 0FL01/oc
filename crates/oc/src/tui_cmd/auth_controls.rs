//! Mounted auth-surface work; never blocks the input loop on issuer/browser IO.
use super::*;
use oc_core::queries::{AuthAction, AuthAttempt, AuthAttemptState, ProviderAccounts};
use oc_tui::app::AuthRequest;

enum Outcome {
    View(AuthAttempt),
    Connected {
        id: String,
        accounts: Result<ProviderAccounts, CoreError>,
        catalog: Box<Result<CatalogSnapshot, CoreError>>,
    },
    Closed,
}

struct Active {
    request: AuthRequest,
    id: String,
    pending: bool,
}

#[derive(Default)]
pub(super) struct Controls {
    cleanup_failed: bool,
    cancelling: bool,
    seen: Option<AuthRequest>,
    active: Option<Active>,
    job: Option<(
        AuthRequest,
        tokio::task::JoinHandle<Result<Outcome, CoreError>>,
    )>,
    poll_at: Option<Instant>,
    opener: Option<(
        AuthRequest,
        tokio::task::JoinHandle<Result<(), &'static str>>,
    )>,
}

impl Controls {
    pub(super) fn pending(&self) -> bool {
        self.job.is_some() || self.opener.is_some()
    }
    pub(super) fn ready(&self) -> bool {
        self.job.as_ref().is_some_and(|(_, j)| j.is_finished())
            || self.opener.as_ref().is_some_and(|(_, j)| j.is_finished())
    }
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.poll_at
    }

    fn cancel(&mut self, app: &CoreApp) {
        let Some(active) = &self.active else {
            return;
        };
        let owner = app.clone();
        let request = active.request.clone();
        let id = active.id.clone();
        self.poll_at = None;
        self.cancelling = true;
        self.job = Some((
            request.clone(),
            tokio::spawn(async move {
                owner
                    .authenticate(request.provider, AuthAction::Cancel { attempt: id })
                    .await?;
                Ok(Outcome::Closed)
            }),
        ));
    }

    pub(super) async fn sync(&mut self, app: &CoreApp, state: &mut TuiState) -> bool {
        let mut changed = false;
        if self.opener.as_ref().is_some_and(|(_, j)| j.is_finished()) {
            let (request, job) = self.opener.take().expect("finished auth opener");
            let result = job.await.unwrap_or(Err("browser opening unavailable"));
            self.cleanup_failed |= result == Err("browser cleanup failed");
            if state.auth_request() == Some(&request) {
                if result.is_err() {
                    state.push_transient_note(
                        "Could not open the browser. Copy the URL and continue manually.",
                        NoteVariant::Error,
                    );
                }
                changed = true;
            }
        }
        if self.job.as_ref().is_some_and(|(_, j)| j.is_finished()) {
            let (request, job) = self.job.take().expect("finished auth job");
            match job.await.unwrap_or(Err(CoreError::Shutdown)) {
                Ok(Outcome::View(view)) => {
                    let pending = view.state == AuthAttemptState::Pending;
                    self.active = Some(Active {
                        request: request.clone(),
                        id: view.id.clone(),
                        pending,
                    });
                    self.poll_at = pending.then(|| Instant::now() + Duration::from_millis(500));
                    if state.auth_request() == Some(&request) {
                        if view.state == AuthAttemptState::Complete {
                            if let Some(id) = view.account_id.clone() {
                                let owner = app.clone();
                                let target = request.provider.clone();
                                self.job = Some((
                                    request.clone(),
                                    tokio::spawn(async move {
                                        let accounts =
                                            owner.provider_accounts(target.clone(), None).await;
                                        let catalog = if accounts.as_ref().is_ok_and(|a| {
                                            a.provider == target
                                                && a.accounts.iter().any(|a| a.id == id)
                                        }) {
                                            owner.provider_catalog(target).await
                                        } else {
                                            Err(CoreError::Application(
                                                "authorization acknowledgement unavailable".into(),
                                            ))
                                        };
                                        Ok(Outcome::Connected {
                                            id,
                                            accounts,
                                            catalog: Box::new(catalog),
                                        })
                                    }),
                                ));
                            } else {
                                state.auth_failure(
                                    &request,
                                    "Authorization acknowledgement unavailable",
                                );
                            }
                        }
                        state.apply_auth_attempt(&request, view);
                        changed = true;
                    }
                }
                Ok(Outcome::Connected {
                    id,
                    accounts,
                    catalog,
                }) => {
                    self.active = None;
                    if state.auth_connected(&request, &id, accounts.map_err(|_| ())) {
                        match *catalog {
                            Ok(snapshot) => state.apply_picker_catalog(snapshot),
                            Err(_) => state.push_transient_note(
                                "Connected; provider model view unavailable",
                                NoteVariant::Error,
                            ),
                        }
                        state.push_transient_note("Connected to OpenAI", NoteVariant::Success);
                        changed = true;
                    }
                }
                Ok(Outcome::Closed) => {
                    self.active = None;
                    self.poll_at = None;
                    self.cancelling = false;
                }
                Err(_) => {
                    self.poll_at = None;
                    state.auth_failure(&request, "Authorization unavailable");
                    if self.cancelling {
                        self.cleanup_failed = true;
                        self.cancelling = false;
                        self.active = None;
                    } else if self.active.as_ref().is_some_and(|a| a.pending) {
                        self.cancel(app);
                    }
                    changed = true;
                }
            }
        }
        if self.job.is_some() {
            return changed;
        }
        let desired = state.auth_request().cloned();
        if self
            .active
            .as_ref()
            .is_some_and(|a| desired.as_ref() != Some(&a.request))
        {
            if self.active.as_ref().is_some_and(|a| a.pending) {
                self.cancel(app);
                return changed;
            }
            self.active = None;
            self.poll_at = None;
        }
        if desired != self.seen {
            self.seen = desired.clone();
            if let Some(request) = desired {
                let owner = app.clone();
                self.job = Some((
                    request.clone(),
                    tokio::spawn(async move {
                        owner
                            .authenticate(
                                request.provider,
                                AuthAction::Begin {
                                    method: request.method,
                                    label: request.method.label().into(),
                                },
                            )
                            .await
                            .map(Outcome::View)
                    }),
                ));
                return true;
            }
        }
        if self.poll_at.is_some_and(|at| Instant::now() >= at) {
            self.poll_at = None;
            if let Some(active) = &self.active {
                let owner = app.clone();
                let request = active.request.clone();
                let id = active.id.clone();
                self.job = Some((
                    request.clone(),
                    tokio::spawn(async move {
                        owner
                            .authenticate(request.provider, AuthAction::Status { attempt: id })
                            .await
                            .map(Outcome::View)
                    }),
                ));
            }
        }
        changed
    }

    pub(super) fn open(&mut self, state: &TuiState) {
        if self.opener.is_some() {
            return;
        }
        if let (Some(request), Some(url)) = (state.auth_request(), state.auth_detail(false)) {
            let url = url.to_owned();
            self.opener = Some((
                request.clone(),
                tokio::spawn(async move { crate::auth_cmd::open_authorization(&url).await }),
            ));
        }
    }

    pub(super) async fn shutdown(&mut self, app: &CoreApp) -> Result<(), String> {
        // Do not abort Begin: its owned attempt ID must be acknowledged before cancel.
        if let Some((request, job)) = self.job.take() {
            match job
                .await
                .map_err(|_| "authorization cleanup failed")?
                .map_err(|_| "authorization cleanup failed")?
            {
                Outcome::View(view) if view.state == AuthAttemptState::Pending => {
                    self.active = Some(Active {
                        request,
                        id: view.id,
                        pending: true,
                    });
                }
                Outcome::Closed => self.active = None,
                _ => {}
            }
        }
        if let Some(active) = self.active.take().filter(|a| a.pending) {
            app.authenticate(
                active.request.provider,
                AuthAction::Cancel { attempt: active.id },
            )
            .await
            .map_err(|_| "authorization cleanup failed")?;
        }
        if let Some((_, job)) = self.opener.take() {
            let result = job.await.map_err(|_| "browser cleanup failed")?;
            if result == Err("browser cleanup failed") {
                return Err("browser cleanup failed".into());
            }
        }
        self.poll_at = None;
        if self.cleanup_failed {
            Err("authorization cleanup failed".into())
        } else {
            Ok(())
        }
    }
}

impl Drop for Controls {
    fn drop(&mut self) {
        if let Some((_, job)) = &self.job {
            job.abort();
        }
        if let Some((_, job)) = &self.opener {
            job.abort();
        }
        // The application attempt owner additionally joins every exit/error path.
    }
}

#[cfg(test)]
mod tests;
