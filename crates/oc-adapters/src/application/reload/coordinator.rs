//! One Location's joined observer and bounded single-flight candidate builds.
//! Builds own finite inputs/shared DB access, never the runtime/application owner.
use super::*;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::Instant;

type Waiter = oneshot::Sender<Result<ReloadLocationSnapshot, CoreError>>;

pub(in crate::application) enum Ready {
    Build {
        result: Result<Candidate, CoreError>,
        force: bool,
        stamp: crate::runtime::SourceStamp,
        waiters: Vec<Waiter>,
    },
    Follow(Arc<authority::Snapshot>),
}

struct Build {
    task: JoinHandle<Result<Candidate, CoreError>>,
    force: bool,
    stamp: crate::runtime::SourceStamp,
    waiters: Vec<Waiter>,
}

pub(in crate::application) struct Receipt {
    result: Result<ReloadLocationSnapshot, CoreError>,
    waiters: Vec<Waiter>,
    explicit: bool,
}

impl Receipt {
    pub(in crate::application) fn retire_titles(&self) -> bool {
        self.explicit && self.result.is_ok()
    }

    pub(in crate::application) fn reply(self) {
        for waiter in self.waiters {
            let _ = waiter.send(self.result.clone());
        }
    }
}

#[derive(Default)]
pub(in crate::application) struct Coordinator {
    follower: Option<tokio::sync::watch::Receiver<Option<Arc<authority::Snapshot>>>>,
    location: Option<PathBuf>,
    watch: Option<composition::SourceWatch>,
    build: Option<Build>,
    pending: bool,
    force: bool,
    rearm: bool,
    due: Option<Instant>,
    waiters: Vec<Waiter>,
    failure: Option<CoreError>,
}

impl Coordinator {
    pub(in crate::application) fn follow(
        source: tokio::sync::watch::Receiver<Option<Arc<authority::Snapshot>>>,
    ) -> Self {
        let mut coordinator = Self::default();
        coordinator.follower = Some(source);
        coordinator
    }
    /// A new root cannot overtake an already accepted explicit source change.
    pub(in crate::application) fn forced(&self) -> bool {
        self.force || self.build.as_ref().is_some_and(|build| build.force)
    }

    pub(in crate::application) async fn ensure(
        &mut self,
        composition: &Composition,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        if self.follower.is_some() {
            return Ok(());
        }
        if self.location.as_ref() == Some(&composition.project) {
            return Ok(());
        }
        self.shutdown().await?;
        self.watch = Some(composition::SourceWatch::start(&composition.watch_inputs)?);
        self.location = Some(composition.project.clone());
        Ok(())
    }

    pub(in crate::application) fn request(&mut self, ack: Waiter) {
        let active = self.build.as_ref().map_or(0, |build| build.waiters.len());
        if active + self.waiters.len() >= MAX_QUEUE_ITEMS {
            let _ = ack.send(Err(CoreError::TurnBusy));
            return;
        }
        self.waiters.push(ack);
        self.pending = true;
        self.force = true;
        self.due = Some(Instant::now());
    }

    pub(in crate::application) fn start(
        &mut self,
        db: &Db,
        runtime: &Runtime<'_>,
        composition: &Composition,
    ) {
        if self.build.is_some() || !self.pending || self.due.is_some_and(|due| due > Instant::now())
        {
            return;
        }
        let Some(watch) = &self.watch else {
            return;
        };
        let observation = watch.observation();
        let inputs = BuildInputs::capture(composition);
        let topology = composition.watch_inputs.clone();
        let db = db.shared_handle();
        let force = std::mem::take(&mut self.force);
        let rearm = std::mem::take(&mut self.rearm);
        self.pending = false;
        self.due = None;
        self.build = Some(Build {
            force,
            stamp: runtime.source_stamp(),
            waiters: std::mem::take(&mut self.waiters),
            task: tokio::spawn(Box::pin(async move {
                if rearm {
                    observation
                        .reconcile(&topology, true)
                        .await
                        .map_err(CoreError::Diagnostic)?;
                }
                build(&db, inputs, &observation, force).await
            })),
        });
    }

    /// Cancellation-safe: the JoinHandle stays owned until its result is taken.
    pub(in crate::application) async fn wait(
        &mut self,
    ) -> Result<Option<Ready>, oc_core::queries::ServiceDiagnostic> {
        if let Some(follower) = &mut self.follower {
            follower.changed().await.map_err(|_| {
                crate::config::diagnostic::failure(
                    "native source follower",
                    &["owner"],
                    oc_core::queries::ServiceStage::Admission,
                    oc_core::queries::ServiceCode::SourceUnavailable,
                    oc_core::queries::ServiceAction::ReviewConfiguration,
                )
            })?;
            return Ok(follower.borrow_and_update().clone().map(Ready::Follow));
        }
        let Some(watch) = &self.watch else {
            return std::future::pending().await;
        };
        tokio::select! {
            change = watch.changed() => {
                if change.failed { return Err(crate::config::diagnostic::failure("native source observer", &["watch"], oc_core::queries::ServiceStage::Admission, oc_core::queries::ServiceCode::SourceUnavailable, oc_core::queries::ServiceAction::ReviewConfiguration)); }
                self.pending = true;
                self.rearm |= change.rearm;
                // An automatic event cannot weaken an accumulated explicit force.
                if !self.force { self.due = Some(Instant::now() + Duration::from_millis(100)); }
                Ok(None)
            },
            result = async { (&mut self.build.as_mut().expect("guarded build").task).await }, if self.build.is_some() => {
                let build = self.build.take().expect("completed build");
                let result = result.map_err(|_| crate::config::diagnostic::failure("native source build", &["join"], oc_core::queries::ServiceStage::Admission, oc_core::queries::ServiceCode::SourceUnavailable, oc_core::queries::ServiceAction::ReviewConfiguration))?;
                Ok(Some(Ready::Build { result, force: build.force, stamp: build.stamp, waiters: build.waiters }))
            },
            () = async { tokio::time::sleep_until(self.due.expect("guarded deadline")).await }, if self.pending && self.build.is_none() && self.due.is_some() => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::application) async fn finish(
        &mut self,
        ready: Ready,
        db: &Db,
        runtime: &Runtime<'_>,
        composition: &mut Composition,
        effective: &mut Effective,
        registry: &mut WorkspaceRegistry,
        sessions: &BTreeMap<String, String>,
        location_epoch: &AtomicU64,
        provider_work: &mut provider_catalog::ProviderWork,
        events: &broadcast::Sender<CoreEvent>,
        active: Option<(&str, &Effective)>,
    ) -> Result<Option<Receipt>, oc_core::queries::ServiceDiagnostic> {
        let ready = match ready {
            Ready::Follow(snapshot) => {
                let next = snapshot.composition.primary_snapshot().map_err(|_| {
                    runtime_issue(
                        runtime.location(),
                        &["source", "snapshot"],
                        &RuntimeError::Storage,
                    )
                    .diagnostic
                })?;
                next.approval_consumer_mode.store(
                    composition.approval_consumer_mode.load(Ordering::SeqCst),
                    Ordering::SeqCst,
                );
                next.question_consumer.store(
                    composition.question_consumer.load(Ordering::SeqCst),
                    Ordering::SeqCst,
                );
                let accepted = active
                    .map(|(_, selected)| selected.clone())
                    .or_else(|| runtime.primary_profile())
                    .unwrap_or_else(|| effective.clone());
                let selected = if let Some(session) = active
                    .map(|(session, _)| session)
                    .or_else(|| sessions.get(runtime.location()).map(String::as_str))
                {
                    selection::for_reload(db, &next, &snapshot.effective, session, &accepted)
                        .map_err(|_| {
                            runtime_issue(
                                runtime.location(),
                                &["selection"],
                                &RuntimeError::Storage,
                            )
                            .diagnostic
                        })?
                } else {
                    accepted
                };
                *registry = WorkspaceRegistry::bind(
                    snapshot.source_id,
                    runtime.location(),
                    &next.generation,
                    workspace_agents(&next),
                    skill_metas(&next),
                );
                *composition = next;
                *effective = selected;
                runtime.follow_primary_source(snapshot.source_id);
                return Ok(None);
            }
            ready => ready,
        };
        let Ready::Build {
            mut result,
            force,
            stamp,
            waiters,
        } = ready
        else {
            unreachable!("handled follower")
        };
        let settle_interactions =
            force && (active.is_some() || !runtime.primary_source_views().is_empty());
        if !runtime.source_is_current(&stamp) {
            self.pending = true;
            self.due = Some(Instant::now());
            result = Err(superseded());
        }
        if result.is_err() && self.pending {
            self.force |= force;
            self.waiters.extend(waiters);
            return Ok(None);
        }
        let result = match result {
            Ok(candidate) => {
                let observation = self.watch.as_ref().expect("live observation").observation();
                // Keep candidate publication state out of each caller's active
                // operation/select frame; the coordinator remains its sole owner.
                Box::pin(publish(
                    db,
                    runtime,
                    composition,
                    effective,
                    registry,
                    sessions,
                    location_epoch,
                    provider_work,
                    events,
                    &observation,
                    candidate,
                    active,
                ))
                .await
            }
            Err(error) => Err(error),
        };
        if settle_interactions && result.is_ok() {
            runtime.reload_pending_interactions();
        }
        self.failure = result.as_ref().err().cloned();
        Ok(Some(Receipt {
            result,
            waiters,
            explicit: force,
        }))
    }

    pub(in crate::application) async fn shutdown(
        &mut self,
    ) -> Result<(), oc_core::queries::ServiceDiagnostic> {
        let mut failure = None;
        self.follower = None;
        if let Some(build) = self.build.take() {
            build.task.abort();
            let result = build.task.await;
            for ack in build.waiters {
                let _ = ack.send(Err(CoreError::TurnBusy));
            }
            if let Err(error) = result
                && !error.is_cancelled()
            {
                failure = Some(crate::config::diagnostic::failure(
                    "native source build",
                    &["cleanup"],
                    oc_core::queries::ServiceStage::Admission,
                    oc_core::queries::ServiceCode::SourceUnavailable,
                    oc_core::queries::ServiceAction::ReviewConfiguration,
                ));
            }
        }
        for ack in self.waiters.drain(..) {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        self.location = None;
        self.pending = false;
        self.force = false;
        self.rearm = false;
        self.due = None;
        if let Some(watch) = self.watch.take()
            && let Err(error) = watch.shutdown().await
        {
            failure = Some(error);
        }
        failure.map_or(Ok(()), Err)
    }
}

impl Drop for Coordinator {
    fn drop(&mut self) {
        if let Some(build) = &self.build {
            build.task.abort();
        }
        // SourceWatch::Drop is the interruption fallback; normal owner exit joins.
    }
}

#[cfg(test)]
mod tests;
