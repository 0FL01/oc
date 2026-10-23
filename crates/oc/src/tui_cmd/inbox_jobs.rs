//! Captured inbox admissions and two-phase withdrawal; no unknown-effect replay.
use super::*;
use oc_core::queries::{InboxAction, InboxSnapshot};

enum Phase {
    Deliver,
    Peek(InboxAction),
    Remove(oc_core::prompt::PromptInput),
    Restore(Box<InboxSnapshot>, oc_core::prompt::PromptInput),
}
struct Admission {
    session: SessionId,
    route: read_jobs::Route,
    phase: Phase,
    job: Option<tokio::task::JoinHandle<Result<InboxSnapshot, CoreError>>>,
}
#[derive(Default)]
pub(super) struct Jobs {
    admissions: std::collections::HashMap<u64, Admission>,
}
impl Jobs {
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.admissions.contains_key(&view.view_identity().0)
    }
    pub(super) fn pending(&self) -> bool {
        !self.admissions.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.admissions
            .values()
            .any(|admission| admission.job.as_ref().is_some_and(|job| job.is_finished()))
    }
    pub(super) fn start(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        action: InboxAction,
        restore: bool,
    ) -> Result<(), String> {
        let identity = view.view_identity();
        if self.admissions.contains_key(&identity.0) {
            return Err("inbox admission pending; original request retained".into());
        }
        if self.admissions.len() >= MAX_TABS + 17 {
            return Err("inbox admissions pending".into());
        }
        let session = require_session(view)?;
        let route = read_jobs::Route::capture(view).ok_or("no session yet")?;
        let (request, phase) = if restore {
            let InboxAction::Remove { id } = &action else {
                return Err("invalid inbox restore".into());
            };
            (InboxAction::Read { id: id.clone() }, Phase::Peek(action))
        } else {
            (action, Phase::Deliver)
        };
        let owner = app.clone();
        let target = session.clone();
        self.admissions.insert(
            identity.0,
            Admission {
                session,
                route,
                phase,
                job: Some(oc_core::wake::spawn(async move {
                    owner.inbox(target, request).await
                })),
            },
        );
        Ok(())
    }
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let keys: Vec<_> = deck
        .inbox_jobs
        .admissions
        .iter()
        .filter(|(_, admission)| admission.job.as_ref().is_none_or(|job| job.is_finished()))
        .map(|(key, _)| *key)
        .collect();
    let mut changed = false;
    for key in keys {
        let mut admission = deck
            .inbox_jobs
            .admissions
            .remove(&key)
            .expect("ready inbox admission");
        let Some(view) =
            read_jobs::views_mut(state, deck).find(|view| admission.route.same_owner(view))
        else {
            // Destructive guards retain this source until its effect is settled.
            deck.inbox_jobs.admissions.insert(key, admission);
            continue;
        };
        if let Some(job) = admission.job.take() {
            let result = job.await.unwrap_or(Err(CoreError::Shutdown));
            changed = true;
            match result {
                Err(error) => {
                    view.push_note(&format!(
                        "Inbox request failed or outcome unknown; nothing resent: {error}"
                    ));
                    continue;
                }
                Ok(snapshot) => match admission.phase {
                    Phase::Deliver => {
                        view.applied_inbox(&admission.session, snapshot, false);
                        continue;
                    }
                    Phase::Peek(action) => {
                        let Some(prompt) = snapshot.prompt else {
                            view.push_note("input already delivered");
                            continue;
                        };
                        if let Err(error) = view.prepare_inbox_restore(prompt.clone()) {
                            view.push_note(&error);
                            continue;
                        }
                        let owner = app.clone();
                        let session = admission.session.clone();
                        admission.phase = Phase::Remove(prompt);
                        admission.job = Some(oc_core::wake::spawn(async move {
                            owner.inbox(session, action).await
                        }));
                    }
                    Phase::Remove(prompt) => {
                        admission.phase = Phase::Restore(Box::new(snapshot), prompt)
                    }
                    Phase::Restore(..) => unreachable!("restoration has no pending owner call"),
                },
            }
        }
        if let Phase::Restore(mut snapshot, prompt) = admission.phase {
            match view.prepare_inbox_restore(prompt.clone()) {
                Ok(merged) => {
                    snapshot.prompt = Some(merged);
                    view.applied_inbox(&admission.session, *snapshot, true);
                    continue;
                }
                Err(error) => {
                    view.push_note(&format!(
                        "Withdrawn input retained; make editor space to restore: {error}"
                    ));
                    admission.phase = Phase::Restore(snapshot, prompt);
                }
            }
        }
        deck.inbox_jobs.admissions.insert(key, admission);
    }
    if changed {
        deck.shared_refresh |= app.shares_workspace();
    }
    changed
}
