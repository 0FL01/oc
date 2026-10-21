//! A turn-local handshake with the existing application actor. The actor owns
//! command resolution; runtime owns request/effect admission and atomic delivery.
use super::*;
use crate::storage::inbox::PendingInput;

pub(crate) struct InboxBoundary {
    pub session: String,
    pub pending: PendingInput,
    pub reply: tokio::sync::oneshot::Sender<Result<PreparedInbox, oc_core::session::CoreError>>,
}

pub(crate) struct PreparedInbox {
    pub(super) pending: PendingInput,
    pub(super) published: Arc<ExecutionGeneration>,
    pub(super) choice: oc_core::queries::ModelRef,
    pub(super) command: Option<NativeCommand>,
    profile: Option<crate::application::Effective>,
    selection_before: Option<String>,
}

pub(super) struct InboxDriver {
    pub(super) session: String,
    initial: Option<String>,
    sender: tokio::sync::mpsc::Sender<InboxBoundary>,
}

pub(crate) struct InboxGuard<'a, 'db>(&'a Runtime<'db>);
impl Drop for InboxGuard<'_, '_> {
    fn drop(&mut self) {
        self.0.inbox_driver.lock().expect("inbox driver").take();
    }
}

impl<'db> Runtime<'db> {
    /// All fallible validation and snapshot construction precede the durable
    /// ACK. Publication adoption after the transaction is infallible.
    pub(crate) fn enqueue_pending(
        &self,
        session: &str,
        delivery: oc_core::queries::InboxDelivery,
        payload: &crate::storage::inbox::InboxPayload,
        selection: Option<(&str, &str)>,
        workspace: Option<(WorkspacePublication, crate::application::Effective)>,
    ) -> Result<oc_core::queries::InboxItem, oc_core::session::CoreError> {
        use oc_core::session::CoreError;
        let prepared = workspace
            .map(|(workspace, profile)| {
                self.prepare_workspace(workspace)
                    .map(|workspace| (workspace, profile))
            })
            .transpose()
            .map_err(crate::application::runtime_error)?;
        let _commit = self.publication_commit.lock().expect("publication commit");
        if prepared.is_some()
            && self
                .primary
                .as_ref()
                .is_some_and(|p| p.source_id.load(Ordering::SeqCst) != self.generation_id())
        {
            return Err(CoreError::Application(
                "source changed; refresh before submitting".into(),
            ));
        }
        let (item, items) = self
            .db
            .inbox_add_rich(session, delivery, payload, selection)
            .map_err(|error| match error {
                StorageError::InboxFull => CoreError::QueueFull,
                error => crate::application::runtime_error(error.into()),
            })?;
        if let Some((workspace, profile)) = prepared {
            if let Some(primary) = &self.primary {
                *primary.profile.write().expect("primary profile") = profile;
            }
            Arc::make_mut(&mut self.current.write().expect("generation lock")).workspace =
                workspace;
        }
        self.publish_inbox(session, items, Vec::new());
        Ok(item)
    }

    pub(crate) fn drive_inbox(
        &self,
        session: &str,
        initial: Option<String>,
        sender: tokio::sync::mpsc::Sender<InboxBoundary>,
    ) -> InboxGuard<'_, 'db> {
        *self.inbox_driver.lock().expect("inbox driver") = Some(InboxDriver {
            session: session.into(),
            initial,
            sender,
        });
        InboxGuard(self)
    }

    pub(super) fn inbox_initial(&self) -> Option<String> {
        self.inbox_driver
            .lock()
            .expect("inbox driver")
            .as_ref()
            .and_then(|d| d.initial.clone())
    }

    pub(crate) fn capture_pending(
        &self,
        pending: PendingInput,
        workspace: WorkspacePublication,
        choice: oc_core::queries::ModelRef,
        command: Option<NativeCommand>,
        profile: crate::application::Effective,
    ) -> Result<PreparedInbox, RuntimeError> {
        let workspace = self.prepare_workspace(workspace)?;
        let _commit = self.publication_commit.lock().expect("publication commit");
        let mut published = (**self.current.read().expect("generation lock")).clone();
        if self
            .primary
            .as_ref()
            .is_some_and(|p| p.source_id.load(Ordering::SeqCst) != published.id)
        {
            return Err(RuntimeError::InvalidArgs(
                "source changed; refresh before submitting".into(),
            ));
        }
        if pending.payload.command.as_ref().is_some_and(|c| {
            c.generation != published.id
                || c.digest.is_none()
                || c.digest.as_deref() != workspace.command_digest.as_deref()
        }) {
            return Err(RuntimeError::InvalidArgs(
                "queued command source changed; restore and resend".into(),
            ));
        }
        published.workspace = workspace;
        let profile = command
            .as_ref()
            .is_some_and(|c| c.child.is_none())
            .then_some(profile);
        let selection_before = command
            .as_ref()
            .and_then(|c| c.selection.as_ref())
            .map(|(key, _)| self.db.get_pref(key))
            .transpose()?
            .flatten();
        Ok(PreparedInbox {
            pending,
            published: Arc::new(published),
            choice,
            command,
            profile,
            selection_before,
        })
    }

    pub(super) fn pending_selection<'a>(
        &self,
        prepared: &'a PreparedInbox,
    ) -> Option<(&'a str, &'a str, Option<&'a str>)> {
        prepared
            .command
            .as_ref()?
            .selection
            .as_ref()
            .map(|(key, value)| {
                (
                    key.as_str(),
                    value.as_str(),
                    prepared.selection_before.as_deref(),
                )
            })
    }

    pub(super) fn adopt_pending_profile(
        &self,
        prepared: &PreparedInbox,
    ) -> Result<(), RuntimeError> {
        let Some(profile) = &prepared.profile else {
            return Ok(());
        };
        let Some((key, value)) = prepared.command.as_ref().and_then(|c| c.selection.as_ref())
        else {
            return Ok(());
        };
        let _commit = self.publication_commit.lock().expect("publication commit");
        if self.db.get_pref(key)?.as_ref() != Some(value)
            || self.generation_id() != prepared.published.id
        {
            return Ok(());
        }
        if let Some(primary) = &self.primary {
            *primary.profile.write().expect("primary profile") = profile.clone();
        }
        let mut current = self.current.write().expect("generation lock");
        Arc::make_mut(&mut current).workspace = prepared.published.workspace.clone();
        Ok(())
    }

    pub(crate) fn fail_pending(
        &self,
        session: &str,
        id: &str,
        error: &oc_core::session::CoreError,
    ) -> Result<(), RuntimeError> {
        self.db.inbox_fail(session, id, error)?;
        self.publish_inbox(session, self.db.inbox_pending(session)?, Vec::new());
        Ok(())
    }

    pub(super) async fn pending_steer(
        &self,
        session: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<PreparedInbox>, RuntimeError> {
        if cancel.load(Ordering::Acquire) {
            return Ok(None);
        }
        let Some(pending) = self.db.inbox_next(session, false)? else {
            return Ok(None);
        };
        let id = pending.item.id.clone();
        let sender = self
            .inbox_driver
            .lock()
            .expect("inbox driver")
            .as_ref()
            .map(|d| d.sender.clone());
        let Some(sender) = sender else {
            // Standalone runtime users have no command/config actor.
            if pending.payload.command.is_some() {
                self.fail_pending(
                    session,
                    &id,
                    &oc_core::session::CoreError::Application(
                        "command requires its application owner".into(),
                    ),
                )?;
                return Ok(None);
            }
            let published = self.fresh_request_view()?.0;
            let choice = oc_core::queries::ModelRef {
                provider: String::new(),
                id: String::new(),
                variant: None,
            };
            return Ok(Some(PreparedInbox {
                pending,
                published,
                choice,
                command: None,
                profile: None,
                selection_before: None,
            }));
        };
        let (reply, receipt) = tokio::sync::oneshot::channel();
        let result = async {
            sender
                .send(InboxBoundary {
                    session: session.into(),
                    pending,
                    reply,
                })
                .await
                .map_err(|_| oc_core::session::CoreError::Shutdown)?;
            receipt
                .await
                .map_err(|_| oc_core::session::CoreError::Shutdown)?
        };
        let result = tokio::select! {
            biased;
            () = crate::provider::wait_cancel(cancel) => return Ok(None),
            result = result => result,
        };
        match result {
            Ok(prepared) => Ok(Some(prepared)),
            Err(error) => {
                self.fail_pending(session, &id, &error)?;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oc_core::queries::InboxDelivery;

    #[tokio::test]
    async fn running_boundary_cancellation_preserves_input_without_waiting_for_the_actor() {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        let runtime = crate::runtime::tests::binding_replay_tests::owner(&db, project.path());
        let item = db.inbox_add("s", InboxDelivery::Steer, "retained").unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        let _guard = runtime.drive_inbox("s", None, sender);
        assert!(runtime.inbox_running("s"));
        assert!(!runtime.inbox_running("another-session"));
        let cancel = AtomicBool::new(false);
        let request = runtime.pending_steer("s", &cancel);
        let actor = async {
            let boundary = receiver.recv().await.unwrap();
            cancel.store(true, Ordering::Release);
            // The actor can already be shutting down and never reply. Keep the
            // sender alive until runtime has observed cancellation itself.
            boundary
        };
        let (result, boundary) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(request, actor)
        })
        .await
        .unwrap();
        assert!(result.unwrap().is_none());
        assert_eq!(db.inbox_pending("s").unwrap(), [item]);
        assert!(db.read_history("s").unwrap().is_empty());
        drop(boundary);
    }
}
