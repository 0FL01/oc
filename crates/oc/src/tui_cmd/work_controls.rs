//! Captured child/shell background and interruption commands.
use super::*;
use std::collections::HashMap;
struct Mutation {
    route: read_jobs::Route,
    job: tokio::task::JoinHandle<Vec<CoreError>>,
}
#[derive(Default)]
pub(super) struct Jobs {
    effects: HashMap<u64, Mutation>,
}
impl Jobs {
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.effects.contains_key(&view.view_identity().0)
    }
    pub(super) fn pending(&self) -> bool {
        !self.effects.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.effects.values().any(|effect| effect.job.is_finished())
    }
    pub(super) fn start(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        intent: PanelIntent,
    ) -> Result<(), String> {
        let route = read_jobs::Route::capture(view).ok_or("no session yet")?;
        let key = view.view_identity().0;
        if self.effects.contains_key(&key) || self.effects.len() >= MAX_TABS + 17 {
            return Err("work control pending; original request retained".into());
        }
        let owner = app.clone();
        self.effects.insert(
            key,
            Mutation {
                route,
                job: oc_core::wake::spawn(async move {
                    let mut errors = Vec::new();
                    match intent {
                        PanelIntent::BackgroundSession {
                            session,
                            turn,
                            work,
                        } => {
                            for target in work {
                                let result = match target {
                                    oc_tui::app::ForegroundWork::Child(job)
                                        if job.parent == session
                                            && !job.background
                                            && matches!(
                                                job.state,
                                                oc_core::queries::ChildState::Admitted
                                                    | oc_core::queries::ChildState::Running
                                            ) =>
                                    {
                                        owner.background_child(job.parent.clone(), *job).await
                                    }
                                    oc_tui::app::ForegroundWork::Shell(job)
                                        if job.session == session
                                            && job.turn == turn.0
                                            && !job.background =>
                                    {
                                        owner.background_shell(job.session, job.shell_id).await
                                    }
                                    oc_tui::app::ForegroundWork::Execute(job)
                                        if job.session == session
                                            && job.turn == turn.0
                                            && !job.background =>
                                    {
                                        owner
                                            .background_execute(
                                                job.session,
                                                job.turn,
                                                job.execute_id,
                                            )
                                            .await
                                    }
                                    _ => continue,
                                };
                                if let Err(error) = result {
                                    errors.push(error);
                                }
                            }
                        }
                        PanelIntent::BackgroundChild { selected } => {
                            if let Err(error) = owner
                                .background_child(selected.parent.clone(), selected)
                                .await
                            {
                                errors.push(error);
                            }
                        }
                        PanelIntent::InterruptChild { selected } => {
                            if let Err(error) = owner
                                .interrupt_child(selected.parent.clone(), selected)
                                .await
                            {
                                errors.push(error);
                            }
                        }
                        PanelIntent::CancelShell { session, shell_id } => {
                            if let Err(error) = owner.cancel_shell(session, shell_id).await
                                && error != CoreError::TurnNotActive
                            {
                                errors.push(error);
                            }
                        }
                        PanelIntent::BackgroundShell { session, shell_id } => {
                            if let Err(error) = owner.background_shell(session, shell_id).await
                                && error != CoreError::TurnNotActive
                            {
                                errors.push(error);
                            }
                        }
                        _ => unreachable!("work control command"),
                    }
                    errors
                }),
            },
        );
        Ok(())
    }
}
pub(super) async fn sync(state: &mut TuiState, deck: &mut LoopState) -> bool {
    let finished: Vec<_> = deck
        .work
        .effects
        .iter()
        .filter(|(_, effect)| effect.job.is_finished())
        .map(|(key, _)| *key)
        .collect();
    let changed = !finished.is_empty();
    for key in finished {
        let effect = deck
            .work
            .effects
            .remove(&key)
            .expect("finished work control");
        let errors = effect
            .job
            .await
            .unwrap_or_else(|_| vec![CoreError::Shutdown]);
        let mut reads = std::mem::take(&mut deck.reads);
        if let Some(view) = read_jobs::views_mut(state, deck).find(|view| effect.route.owns(view)) {
            for error in errors {
                view.push_note(&error.to_string());
            }
            reads.request(view, false);
        }
        deck.reads = reads;
    }
    changed
}
