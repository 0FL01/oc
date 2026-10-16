//! Application admission for user terminals; existing processes never rebind on
//! Location/config replacement. No turn/tool admission and no provider effects.
use super::*;
use oc_core::queries::{TerminalAction, TerminalReceipt};

pub(super) type Owner = Mutex<Result<crate::terminals::Terminals, CoreError>>;

pub(super) fn shutdown(owner: &Owner) -> Result<(), CoreError> {
    owner
        .lock()
        .expect("terminal owner mutex")
        .as_mut()
        .map_err(|error| error.clone())?
        .shutdown()
}

pub(super) fn action(
    owner: &Owner,
    runtime: &Runtime<'_>,
    epoch: u64,
    session: SessionId,
    action: TerminalAction,
) -> Result<TerminalReceipt, CoreError> {
    let mut guard = owner.lock().expect("terminal owner mutex");
    let owner = guard.as_mut().map_err(|error| error.clone())?;
    match action {
        TerminalAction::List => owner.inventory(&session).map(TerminalReceipt::Inventory),
        action @ (TerminalAction::Create { .. } | TerminalAction::CreateChild { .. }) => runtime
            .create_terminal(owner, session, action, epoch)
            .map(TerminalReceipt::Created),
        TerminalAction::Select(target) => {
            owner.select(&session, target.as_ref())?;
            Ok(TerminalReceipt::Applied)
        }
        TerminalAction::Input { target, bytes } => {
            owner.input(&session, &target, bytes)?;
            Ok(TerminalReceipt::Applied)
        }
        TerminalAction::Resize { target, size } => {
            owner.resize(&session, &target, size)?;
            Ok(TerminalReceipt::Applied)
        }
        TerminalAction::Scroll { target, lines } => {
            owner.scroll(&session, &target, lines)?;
            Ok(TerminalReceipt::Applied)
        }
        TerminalAction::Snapshot(target) => owner
            .snapshot(&session, &target)
            .map(TerminalReceipt::Snapshot),
        TerminalAction::Replay { target, cursor } => owner
            .replay(&session, &target, cursor)
            .map(TerminalReceipt::Replay),
        TerminalAction::Remove(target) => {
            owner.remove(&session, &target)?;
            Ok(TerminalReceipt::Applied)
        }
    }
}
