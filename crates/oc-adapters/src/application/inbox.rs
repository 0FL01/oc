//! Rich pending admission and command preparation remain in the config actor.
use super::*;
use crate::storage::inbox::{CommandCapture, InboxPayload, PendingInput};

pub(super) fn payload(
    composition: &Composition,
    generation: u64,
    prompt: oc_core::prompt::PromptInput,
) -> Result<InboxPayload, CoreError> {
    prompt.check_budget()?;
    if prompt.trim().is_empty() {
        return Err(app_error("empty prompt"));
    }
    if !prompt.images.is_empty() && prompt.trim_start().starts_with('/') {
        return Err(app_error("commands do not accept image attachments"));
    }
    let (text, invocation) = resolve_submission(composition, prompt.text).map_err(app_error)?;
    let command = invocation.and_then(|original| {
        commands::definition(composition, Some(&original)).map(|def| CommandCapture {
            id: def.id.clone(),
            generation,
            digest: Some(command_digest(composition)),
            original,
        })
    });
    let prompt = oc_core::prompt::PromptInput {
        text,
        images: prompt.images,
    };
    crate::attachments::prepare_prompt(&prompt, &AtomicBool::new(false)).map_err(app_error)?;
    Ok(InboxPayload { prompt, command })
}

pub(super) fn prepare(
    db: &Db,
    runtime: &Runtime<'_>,
    composition: &Composition,
    effective: &Effective,
    session: &str,
    pending: PendingInput,
) -> Result<crate::runtime::inbox::PreparedInbox, CoreError> {
    let parent = selection::for_turn(db, composition, effective, session)?;
    let (selected, command) = route(
        db,
        composition,
        runtime.generation_id(),
        session,
        &parent,
        &pending,
    )?;
    selected.admit_selection(composition)?;
    let choice = oc_core::queries::ModelRef {
        provider: selected.provider_id.clone(),
        id: selected.model_id.clone(),
        variant: selected.variant.clone(),
    };
    runtime
        .capture_pending(
            pending,
            workspace_publication(composition, &selected),
            choice,
            command,
            selected,
        )
        .map_err(runtime_error)
}

pub(super) fn route(
    db: &Db,
    composition: &Composition,
    generation: u64,
    session: &str,
    parent: &Effective,
    pending: &PendingInput,
) -> Result<(Effective, Option<crate::runtime::NativeCommand>), CoreError> {
    let Some(capture) = &pending.payload.command else {
        return Ok((parent.clone(), None));
    };
    if capture.generation != generation
        || capture.digest.as_deref() != Some(command_digest(composition).as_str())
    {
        return Err(app_error(
            "queued command source changed; restore and resend the command",
        ));
    }
    let def = composition
        .command_defs
        .get(&capture.id)
        .ok_or_else(|| app_error("queued command unavailable"))?;
    let (selected, mut command) = commands::prepare(composition, parent, def)?;
    if command.child.is_none() {
        command.selection = Some(selection::command_record(
            db,
            composition,
            session,
            &selected,
        )?);
    }
    Ok((selected, Some(command)))
}

pub(super) struct AcceptanceAck<'a, 'db> {
    pub sender: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    pub runtime: &'a Runtime<'db>,
    pub session: &'a str,
    pub pending: Option<String>,
}

impl AcceptanceAck<'_, '_> {
    pub fn send(
        self,
        mut result: Result<WorkerTurnId, CoreError>,
    ) -> Result<(), Result<WorkerTurnId, CoreError>> {
        if let (Some(id), Err(error)) = (&self.pending, &result)
            && let Err(storage) = self.runtime.fail_pending(self.session, id, error)
        {
            result = Err(runtime_error(storage));
        }
        self.sender.send(result)
    }
}
