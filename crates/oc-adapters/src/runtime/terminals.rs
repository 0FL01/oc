//! PTY creation from the admitted root or retained child execution context.
use super::*;
use oc_core::queries::{
    ChildJob, ChildState, TerminalAction, TerminalEntry, TerminalRef, TerminalSize,
};
use oc_core::{domain::SessionId, session::CoreError};

pub(super) fn same_source(actual: &ChildJob, selected: &ChildJob) -> bool {
    actual.parent == selected.parent
        && actual.child == selected.child
        && actual.operation == selected.operation
        && actual.generation == selected.generation
        && actual.location == selected.location
        && actual.delivery_id == selected.delivery_id
        && actual.agent == selected.agent
        && actual.model == selected.model
}

fn refused() -> CoreError {
    CoreError::Application("terminal source changed".into())
}

impl Runtime<'_> {
    fn spawn_terminal(
        &self,
        owner: &mut crate::terminals::Terminals,
        session: SessionId,
        generation: u64,
        size: TerminalSize,
    ) -> Result<TerminalEntry, CoreError> {
        self.open_session(&session.0)
            .map_err(|_| CoreError::SessionNotFound)?;
        owner.create_admitted(
            TerminalRef {
                id: String::new(),
                session,
                location: self.location.clone(),
                generation,
            },
            &self.parent_env,
            size,
            &self.shell,
        )
    }

    pub(crate) fn create_terminal(
        &self,
        owner: &mut crate::terminals::Terminals,
        session: SessionId,
        action: TerminalAction,
        epoch: u64,
    ) -> Result<TerminalEntry, CoreError> {
        let (selected, generation, size) = match action {
            TerminalAction::Create {
                location,
                generation,
                size,
            } => {
                if location != self.location || generation != epoch {
                    return Err(refused());
                }
                return self.spawn_terminal(owner, session, generation, size);
            }
            TerminalAction::CreateChild {
                source,
                generation,
                size,
            } => (source, generation, size),
            _ => return Err(refused()),
        };
        if session != selected.child {
            return Err(refused());
        }
        // This is the existing child ownership lineage; a caller's path/epoch
        // alone cannot recreate an old source or confer execution authority.
        if let Some(source) = self.child_jobs.terminal_source(&selected) {
            if source.location != selected.location || source.generation_id() != selected.generation
            {
                return Err(refused());
            }
            return source.spawn_terminal(owner, session, selected.generation, size);
        }
        // Settled/reopened children have no retained executable lane. Only the
        // current admitted context of the same Location is usable, with its
        // captured current epoch. Never reconstruct/restart the child runtime.
        if selected.location != self.location || generation != epoch {
            return Err(refused());
        }
        let actual = self
            .db
            .child_jobs(&selected.parent.0)
            .map_err(|_| refused())?;
        if !actual.iter().any(|job| {
            same_source(job, &selected)
                && !matches!(job.state, ChildState::Admitted | ChildState::Running)
        }) {
            return Err(refused());
        }
        self.spawn_terminal(owner, session, generation, size)
    }
}
