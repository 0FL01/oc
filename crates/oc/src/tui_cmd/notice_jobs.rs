//! Exact durable notice reads, separate from the worker presentation path.
use super::*;
use oc_core::queries::HistoryPage;
use std::collections::HashMap;

struct Prepared {
    event: CoreEvent,
    exact: Option<HistoryPage>,
    recent: HistoryPage,
    children: Option<Vec<oc_core::queries::ChildJob>>,
    shell: Option<oc_core::queries::ShellSnapshot>,
    execute: Option<oc_core::queries::ExecuteSnapshot>,
}
struct Read {
    route: read_jobs::Route,
    pending: HashMap<String, (CoreEvent, bool, bool)>,
    running: Vec<(String, CoreEvent)>,
    job: Option<tokio::task::JoinHandle<Result<Vec<Prepared>, CoreError>>>,
    repair: bool,
}
#[derive(Default)]
pub(super) struct Jobs {
    reads: HashMap<u64, Read>,
}
impl Jobs {
    pub(super) fn pending(&self) -> bool {
        !self.reads.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.reads
            .values()
            .any(|read| read.job.as_ref().is_none_or(|job| job.is_finished()))
    }
    pub(super) fn request(&mut self, view: &TuiState, mut event: CoreEvent) {
        let Some(route) = read_jobs::Route::capture(view) else {
            return;
        };
        if let CoreEvent::InboxChanged { delivered, .. } = &mut event {
            let items = std::mem::take(delivered);
            for item in items {
                let mut single = event.clone();
                if let CoreEvent::InboxChanged { delivered, .. } = &mut single {
                    delivered.push(item.clone());
                }
                self.insert(view, route.clone(), item.message.0.clone(), single);
            }
            return;
        }
        let key = match &event {
            CoreEvent::ChildNotice(notice) => notice.job.message_id.clone(),
            CoreEvent::ShellNotice(notice) => Some(notice.message_id.clone()),
            CoreEvent::ExecuteNotice(notice) => Some(notice.message_id.clone()),
            CoreEvent::SelectionNoticeCommitted { message, .. } => Some(message.0.clone()),
            _ => None,
        };
        let Some(key) = key else { return };
        self.insert(view, route, key, event);
    }
    fn insert(&mut self, view: &TuiState, route: read_jobs::Route, key: String, event: CoreEvent) {
        let read = self
            .reads
            .entry(view.view_identity().0)
            .or_insert_with(|| Read {
                route,
                pending: HashMap::new(),
                running: Vec::new(),
                job: None,
                repair: false,
            });
        queue(read, view, key, event);
    }
}
fn queue(read: &mut Read, view: &TuiState, key: String, event: CoreEvent) {
    if read.pending.len() < 64 || read.pending.contains_key(&key) {
        let shell_card = matches!(&event, CoreEvent::ShellNotice(notice) if view.has_model_shell_card(&notice.shell_id));
        let exact_user_shell = view.is_busy()
            && matches!(&event, CoreEvent::ShellNotice(notice) if notice.user_requested);
        read.pending
            .insert(key, (event, shell_card, exact_user_shell));
    } else {
        read.repair = true;
    }
    // Overflow is repaired by the canonical read lane, never fabricated.
}
async fn prepare(
    app: &CoreApp,
    route: read_jobs::Route,
    events: Vec<(CoreEvent, bool, bool)>,
) -> Result<Vec<Prepared>, CoreError> {
    let mut prepared = Vec::new();
    let linked_page = route.validate_child(app).await?;
    for (event, shell_card, exact_user_shell) in events {
        let message = match &event {
            CoreEvent::ChildNotice(notice) => notice
                .job
                .message_id
                .clone()
                .map(oc_core::session::MessageId),
            CoreEvent::ShellNotice(notice) => {
                Some(oc_core::session::MessageId(notice.message_id.clone()))
            }
            CoreEvent::ExecuteNotice(notice) => {
                Some(oc_core::session::MessageId(notice.message_id.clone()))
            }
            CoreEvent::SelectionNoticeCommitted { message, .. } => Some(message.clone()),
            CoreEvent::InboxChanged { delivered, .. } => {
                delivered.first().map(|item| item.message.clone())
            }
            _ => None,
        };
        let exact = if let Some(message) = message {
            Some(app.history_message(route.session.clone(), message).await?)
        } else {
            None
        };
        let execute = if let CoreEvent::ExecuteNotice(notice) = &event {
            app.execute_snapshot(notice.session.clone(), notice.execute_id.clone())
                .await
                .ok()
        } else {
            None
        };
        let qualified = exact.as_ref().is_some_and(|page| page.rows.iter().any(|row| match &event {
            CoreEvent::ChildNotice(notice) => matches!(&row.child, Some(oc_core::queries::ChildHistory::Notice(job)) if child_controls::same_launch(job, &notice.job) && job.message_id == notice.job.message_id && job.message_id.as_deref() == Some(row.id.0.as_str())),
            CoreEvent::ShellNotice(notice) => row.id.0 == notice.message_id && if notice.user_requested { row.user_shell.as_ref().is_some_and(|result| result.operation == notice.shell_id) } else { row.shell_notice.as_ref().is_some_and(|metadata| metadata.operation == notice.shell_id) },
            CoreEvent::ExecuteNotice(notice) => row.id.0 == notice.message_id && row.role == oc_core::session::Role::User && execute.as_ref().is_some_and(|snapshot| snapshot.job.session == notice.session && snapshot.job.execute_id == notice.execute_id && snapshot.delivery_id == notice.delivery_id && snapshot.message_id.as_deref() == Some(notice.message_id.as_str())),
            CoreEvent::SelectionNoticeCommitted { message, .. } => row.id == *message,
            CoreEvent::InboxChanged { delivered, .. } => delivered.iter().any(|item| item.message == row.id),
            _ => false,
        }));
        if !qualified {
            continue;
        }
        let children = if matches!(&event, CoreEvent::ChildNotice(_)) {
            Some(app.child_jobs(route.session.clone()).await?)
        } else {
            None
        };
        let shell = if let CoreEvent::ShellNotice(notice) = &event
            && !notice.user_requested
            && shell_card
        {
            app.shell_snapshot(notice.session.clone(), notice.shell_id.clone())
                .await
                .ok()
        } else {
            None
        };
        let recent = if exact_user_shell {
            exact.clone().unwrap_or_default()
        } else if let Some(page) = &linked_page {
            page.clone()
        } else {
            app.transcript_page(route.session.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await?
        };
        prepared.push(Prepared {
            event,
            exact,
            recent,
            children,
            shell,
            execute,
        });
    }
    Ok(prepared)
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut jobs = std::mem::take(&mut deck.notices);
    let mut changed = false;
    let mut repairs = Vec::new();
    let keys: Vec<_> = jobs.reads.keys().copied().collect();
    for key in keys {
        let mut read = jobs.reads.remove(&key).expect("notice read");
        if read.job.as_ref().is_some_and(|job| !job.is_finished()) {
            jobs.reads.insert(key, read);
            continue;
        }
        let Some(view) =
            read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key)
        else {
            continue;
        };
        if let Some(job) = read.job.take() {
            let result = job.await.unwrap_or(Err(CoreError::Shutdown));
            if read.route.matches(view) {
                match result {
                    Ok(prepared) => {
                        for item in prepared {
                            apply(view, item);
                        }
                    }
                    Err(error) => {
                        view.push_note(&format!("Notice history refresh failed: {error}"));
                        read.repair = true;
                    }
                }
                changed = true;
            } else {
                read.repair = true;
                if read.route.owns(view) {
                    for (key, event) in std::mem::take(&mut read.running) {
                        if !read.pending.contains_key(&key) {
                            queue(&mut read, view, key, event);
                        }
                    }
                }
            }
            // A changed turn/floor is repaired canonically; do not insert an
            // old read's rows into the new live turn.
        }
        if read.repair {
            repairs.push(key);
            read.repair = false;
        }
        if !read.pending.is_empty()
            && let Some(route) = read_jobs::Route::capture(view)
        {
            let pending: Vec<_> = read.pending.drain().collect();
            read.running = pending
                .iter()
                .map(|(key, (event, _, _))| (key.clone(), event.clone()))
                .collect();
            let events = pending.into_iter().map(|(_, event)| event).collect();
            let owner = app.clone();
            let captured = route.clone();
            read.route = route;
            read.job = Some(oc_core::wake::spawn(async move {
                prepare(&owner, captured, events).await
            }));
            jobs.reads.insert(key, read);
        }
    }
    deck.notices = jobs;
    let mut reads = std::mem::take(&mut deck.reads);
    for view in
        read_jobs::views_mut(state, deck).filter(|view| repairs.contains(&view.view_identity().0))
    {
        reads.request(view, true);
    }
    deck.reads = reads;
    changed
}
fn apply(view: &mut TuiState, prepared: Prepared) {
    let Prepared {
        event,
        exact,
        recent,
        children,
        shell,
        execute,
    } = prepared;
    if let Some(children) = children {
        view.apply_child_jobs(children);
    }
    match event {
        CoreEvent::ChildNotice(notice) => {
            let Some(exact) = exact else { return };
            let Some(receipt) = exact.rows.iter().find_map(|row| {
                let Some(oc_core::queries::ChildHistory::Notice(job)) = &row.child else {
                    return None;
                };
                (job.parent == notice.job.parent
                    && job.child == notice.job.child
                    && job.operation == notice.job.operation
                    && job.generation == notice.job.generation
                    && job.location == notice.job.location
                    && job.delivery_id == notice.job.delivery_id
                    && job.message_id == notice.job.message_id
                    && job.message_id.as_deref() == Some(row.id.0.as_str()))
                .then(|| job.as_ref().clone())
            }) else {
                return;
            };
            view.observe_history_commit();
            if view.is_busy() {
                view.refresh_child_notice_page(&recent);
                view.refresh_child_notice_page(&exact);
            } else {
                view.refresh_completed_page(&recent);
            }
            view.apply_child_notice(&receipt);
        }
        CoreEvent::SelectionNoticeCommitted { message, .. } => {
            if let Some(exact) = exact
                && exact.rows.iter().any(|row| row.id == message)
            {
                view.observe_history_commit();
                view.refresh_child_notice_page(&recent);
                view.refresh_child_notice_page(&exact);
            }
        }
        CoreEvent::InboxChanged { delivered, .. } => {
            view.observe_history_commit();
            view.refresh_child_notice_page(&recent);
            let exact = exact.as_ref().unwrap_or(&recent);
            view.insert_steered_rows(
                exact,
                &delivered
                    .into_iter()
                    .map(|item| item.message)
                    .collect::<Vec<_>>(),
            );
        }
        CoreEvent::ShellNotice(notice) => {
            if let Some(exact) = exact {
                let valid = exact.rows.iter().any(|row| {
                    row.id.0 == notice.message_id
                        && if notice.user_requested {
                            row.user_shell.is_some()
                        } else {
                            row.shell_notice
                                .as_ref()
                                .is_some_and(|metadata| metadata.operation == notice.shell_id)
                        }
                });
                if !valid {
                    return;
                }
                view.observe_history_commit();
                if let Some(shell) = shell {
                    view.apply_model_shell_snapshot(&shell);
                }
                if view.is_busy() {
                    if notice.user_requested {
                        view.refresh_user_shell_page(&exact);
                    } else {
                        view.refresh_child_notice_page(&recent);
                        view.refresh_child_notice_page(&exact);
                    }
                } else {
                    view.refresh_completed_page(&recent);
                }
            }
        }
        CoreEvent::ExecuteNotice(_) => {
            if let (Some(exact), Some(snapshot)) = (exact, execute) {
                view.observe_history_commit();
                if view.is_busy() {
                    view.refresh_child_notice_page(&recent);
                    view.refresh_child_notice_page(&exact);
                } else {
                    view.refresh_completed_page(&recent);
                }
                view.apply_execute_snapshot(&snapshot);
            }
        }
        _ => {}
    }
}
