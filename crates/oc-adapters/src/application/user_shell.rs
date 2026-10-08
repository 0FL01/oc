//! One actor-owned admission future keeps Ask replies reachable. Execution and
//! durable completion remain with the existing shared Shell supervisor.
use super::*;
use oc_core::core_app::UserShellSelection;

#[allow(clippy::too_many_arguments)]
pub(super) async fn run(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &mut Effective,
    registry: &mut WorkspaceRegistry,
    sessions: &mut BTreeMap<String, String>,
    home_choices: &mut BTreeMap<String, Effective>,
    location_epoch: &Arc<AtomicU64>,
    suggestion_queue: &Arc<Mutex<SuggestionQueue>>,
    title_work: &Mutex<AutomaticTitles>,
    terminals: &terminals::Owner,
    authentication: &authentication::Owner,
    events: &broadcast::Sender<CoreEvent>,
    inbox: &mut mpsc::Receiver<InboxMsg>,
    pending_inputs: &mut std::collections::VecDeque<InboxMsg>,
    title_rx: &mut mpsc::Receiver<AutomaticTitleResult>,
    message: InboxMsg,
) -> Result<bool, oc_core::queries::ServiceDiagnostic> {
    let InboxMsg::UserShell {
        session,
        command,
        selection: choice,
        cancel,
        ack,
    } = message
    else {
        unreachable!("only structured user Shell requests enter this owner");
    };
    let fresh = matches!(choice, UserShellSelection::Fresh(_));
    let prepared = (|| -> Result<_, CoreError> {
        if cancel.load(Ordering::Acquire) {
            return Err(app_error("user shell admission cancelled"));
        }
        let (selected, selection) = match choice {
            UserShellSelection::Fresh(choice) => {
                match db.session_meta(&session.0) {
                    Err(StorageError::SessionNotFound) => {}
                    Ok(_) => return Err(CoreError::SessionAlreadyExists),
                    Err(error) => return Err(query_storage_error(db, error)),
                }
                selection::fresh_captured(
                    db,
                    composition,
                    effective,
                    &session.0,
                    choice,
                    home_choices,
                    location_epoch.load(Ordering::SeqCst),
                )?
            }
            UserShellSelection::Existing(commit) => {
                runtime.open_session(&session.0).map_err(runtime_error)?;
                let current = selection::for_turn(db, composition, effective, &session.0)?;
                let selected = match commit {
                    Some(commit) => selection::command_commit(
                        db,
                        composition,
                        &current,
                        &session.0,
                        location_epoch.load(Ordering::SeqCst),
                        &commit,
                    )?,
                    None => current,
                };
                let record = selection::command_record(db, composition, &session.0, &selected)?;
                (selected, record)
            }
        };
        selected.admit_selection(composition)?;
        publish_workspace(runtime, composition, &selected).map_err(app_error)?;
        Ok((selected, selection))
    })();
    let (selected, record) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ack.send(Err(error));
            return Ok(false);
        }
    };
    let Some(catalog) = composition.catalog_for(&selected.provider_id) else {
        let _ = ack.send(Err(app_error("selected provider unavailable")));
        return Ok(false);
    };
    let mut ack = Some(ack);
    let mut admitted = false;
    let mut shutdown = false;
    let mut fatal = None;
    let result = {
        let operation = runtime.run_user_shell(
            crate::runtime::UserShellParams {
                session: &session.0,
                command: &command,
                catalog,
                model: &selected.model_id,
                variant: selected.variant.as_deref(),
                fresh,
                selection: Some((&record.0, &record.1)),
                cancel: &cancel,
            },
            |operation| {
                admitted = true;
                if let Some(ack) = ack.take() {
                    let _ = ack.send(Ok(operation.to_owned()));
                }
            },
        );
        tokio::pin!(operation);
        loop {
            tokio::select! {
                result = &mut operation => break result,
                () = runtime.shell_jobs.changed() => {
                    runtime.shell_jobs.deliver(events).map_err(|error| storage_diagnostic(db.root(), &error))?;
                }
                Some(result) = title_rx.recv(), if !shutdown => {
                    commit_automatic_title(db, events, title_work, result);
                }
                error = runtime.wait_mcp_failure(), if !shutdown => {
                    fatal = Some(runtime_issue(runtime.location(), &["mcp"], &error).diagnostic);
                    shutdown = true;
                    cancel.store(true, Ordering::Release);
                    runtime.cancel_pending_approvals();
                }
                message = inbox.recv(), if !shutdown => match message {
                    None | Some(InboxMsg::Shutdown) => {
                        shutdown = true;
                        cancel.store(true, Ordering::Release);
                        runtime.cancel_pending_approvals();
                    }
                    Some(InboxMsg::Cancel { session: target, ack }) if target == session => {
                        cancel.store(true, Ordering::Release);
                        runtime.cancel_pending_approvals();
                        runtime.shell_jobs.cancel_session(&target.0);
                        runtime.child_jobs.cancel_session(&target.0);
                        let _ = ack.send(Ok(()));
                    }
                    Some(message @ (InboxMsg::SwitchLocation { .. }
                        | InboxMsg::SwitchLocationHome { .. }
                        | InboxMsg::ReloadLocation { .. }
                        | InboxMsg::ChangeConversation { .. })) => {
                        cancel.store(true, Ordering::Release);
                        runtime.cancel_pending_approvals();
                        // Stop reading until the captured admission settles;
                        // exactly one transition is retained, ahead of new input.
                        pending_inputs.push_front(message);
                        let _ = (&mut operation).await;
                        break Err(RuntimeError::Cancelled);
                    }
                    Some(message) => query(
                        db, runtime, composition, effective, registry, sessions,
                        home_choices, location_epoch, suggestion_queue, title_work,
                        terminals, authentication, message,
                    ).await,
                },
            }
        }
    };
    if admitted {
        sessions.insert(runtime.location().to_owned(), session.0.clone());
        let _ = events.send(CoreEvent::ShellChanged { session });
    }
    if let Some(ack) = ack {
        let error = result.err().unwrap_or(RuntimeError::Storage);
        let _ = ack.send(Err(runtime_error(error)));
    }
    if let Some(fatal) = fatal {
        return Err(fatal);
    }
    Ok(shutdown)
}
