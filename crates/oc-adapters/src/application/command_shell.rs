//! Busy command preparation is a side future owned through completion by the
//! same actor as the running turn, even if that turn settles before the ACK.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn start_side<'a>(
    db: &Db,
    runtime: &'a Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
    root: &SessionId,
    cancel: &'a AtomicBool,
    epoch: u64,
    message: InboxMsg,
) -> Option<user_shell::SideAdmission<'a>> {
    let InboxMsg::Enqueue {
        session,
        text,
        delivery,
        selection: commit,
        ack,
    } = message
    else {
        unreachable!("enqueue")
    };
    let prepared = (|| -> Result<_, CoreError> {
        if &session != root {
            return Err(CoreError::TurnBusy);
        }
        if !runtime.inbox_running(&session.0) {
            return Err(CoreError::TurnNotActive);
        }
        inbox_session(db, runtime, &session)?;
        let payload = inbox::payload(composition, runtime.generation_id(), text)?;
        let current = selection::for_turn(db, composition, effective, &session.0)?;
        let selected = if let Some(commit) = &commit {
            selection::command_commit(db, composition, &current, &session.0, epoch, commit)?
        } else {
            current
        };
        let record = commit
            .as_ref()
            .map(|_| selection::command_record(db, composition, &session.0, &selected))
            .transpose()?;
        let workspace = commit.as_ref().map(|_| {
            (
                workspace_publication(composition, &selected),
                selected.clone(),
            )
        });
        let capture = if let Some(capture) = &payload.command
            && !crate::runtime::command_shell::expressions(&payload.prompt.text).is_empty()
        {
            let def = composition
                .command_defs
                .get(&capture.id)
                .ok_or_else(|| app_error("command unavailable"))?;
            let (target, command) = commands::prepare(composition, &selected, def)?;
            target.admit_selection(composition)?;
            let (catalog, provider) = target.request(composition)?;
            runtime
                .admit_provider_variant(
                    catalog,
                    &target.model_id,
                    target.variant.as_deref(),
                    &provider,
                )
                .map_err(runtime_error)?;
            db.command_inbox_capacity(&session.0, &payload)
                .map_err(|_| CoreError::QueueFull)?;
            Some(
                runtime
                    .capture_command(workspace_publication(composition, &target), &command)
                    .map_err(runtime_error)?,
            )
        } else {
            None
        };
        Ok((payload, record, workspace, capture))
    })();
    let (mut payload, record, workspace, capture) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = ack.send(Err(error));
            return None;
        }
    };
    let has_command_interpolation = capture.is_some();
    Some(Box::pin(async move {
        let result = async {
            let _resolution = if let Some(capture) = capture {
                let original = payload
                    .command
                    .as_ref()
                    .expect("command capture")
                    .original
                    .clone();
                let (prompt, resolution) = runtime
                    .resolve_command_shell(
                        &session.0,
                        &original,
                        &payload.prompt.text,
                        capture,
                        cancel,
                    )
                    .await
                    .map_err(runtime_error)?;
                payload.prompt.text = prompt;
                payload
                    .command
                    .as_mut()
                    .expect("command capture")
                    .resolution = Some(resolution.id.clone());
                Some(resolution)
            } else {
                None
            };
            if cancel.load(Ordering::Acquire) {
                return Err(app_error("command admission cancelled; nothing resent"));
            }
            let item = runtime.enqueue_pending(
                &session.0,
                delivery,
                &payload,
                record.as_ref().map(|(k, v)| (k.as_str(), v.as_str())),
                workspace,
            )?;
            if let Some(commit) = commit {
                runtime.publish_model_selection(session, commit);
            }
            Ok(item)
        }
        .await;
        let _ = ack.send(result.map_err(|error| if has_command_interpolation {
            app_error(format!("Command input was not accepted: {error}. Effects may have occurred; Settings → Interrupted commands. Nothing resent."))
        } else { error }));
        false
    }))
}
