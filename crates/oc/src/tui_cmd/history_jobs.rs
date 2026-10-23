//! Bounded, cursor-fenced transcript/card reads. Never carry a view into a task.
use super::*;
use oc_core::queries::{HistoryPage, HistoryReadWindow, ToolOpPage, ToolOutputPage};
use std::collections::HashMap;
#[derive(Clone)]
pub(super) enum Request {
    Window {
        window: HistoryReadWindow,
        older: bool,
    },
    Cards(Option<i64>),
    Output(String, usize),
    Copy(oc_core::session::MessageId, i64),
}
enum Prepared {
    Window(HistoryPage, bool),
    Cards(ToolOpPage, Option<i64>),
    Output(String, usize, ToolOutputPage),
    Copy(String),
}
struct Read {
    route: read_jobs::Route,
    request: Request,
    next: Option<Request>,
    job: tokio::task::JoinHandle<Result<Prepared, String>>,
}
#[derive(Default)]
pub(super) struct Jobs {
    reads: HashMap<(u64, u8), Read>,
}
impl Jobs {
    pub(super) fn pending(&self) -> bool {
        !self.reads.is_empty()
    }
    pub(super) fn ready(&self) -> bool {
        self.reads.values().any(|read| read.job.is_finished())
    }
    pub(super) fn request(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        request: Request,
    ) -> Result<(), String> {
        require_session(view)?;
        let route = read_jobs::Route::capture(view).ok_or("no active session")?;
        let kind = match request {
            Request::Window { .. } => 0,
            Request::Cards(_) => 1,
            Request::Output(..) => 2,
            Request::Copy(..) => 3,
        };
        let key = (view.view_identity().0, kind);
        if let Some(read) = self.reads.get_mut(&key) {
            read.next = Some(request);
            return Ok(());
        }
        let owner = app.clone();
        let captured = route.clone();
        let query = request.clone();
        self.reads.insert(
            key,
            Read {
                route,
                request,
                next: None,
                job: tokio::spawn(async move {
                    prepare(&owner, captured, query)
                        .await
                        .map_err(|error| error.to_string())
                }),
            },
        );
        Ok(())
    }
}
async fn prepare(
    app: &CoreApp,
    route: read_jobs::Route,
    request: Request,
) -> Result<Prepared, CoreError> {
    match request {
        Request::Window { window, older } => {
            let page = if let Some(child) = route.child {
                app.read_child_window(child.parent.clone(), child, window)
                    .await?
            } else {
                app.history_window(route.session, window, HISTORY_PAGE_LIMIT)
                    .await?
            };
            Ok(Prepared::Window(page, older))
        }
        Request::Cards(before) => Ok(Prepared::Cards(
            app.tool_ops_page(route.session, before, TOOL_OPS_PAGE_LIMIT)
                .await?,
            before,
        )),
        Request::Output(op, offset) => {
            let page = if let Some(page) = app
                .shell_output(route.session.clone(), op.clone(), offset, 240)
                .await?
            {
                page
            } else {
                app.tool_output_page(route.session, op.clone(), offset, 240)
                    .await?
            };
            Ok(Prepared::Output(op, offset, page))
        }
        Request::Copy(message, seq) => {
            let page = app
                .history_page(route.session, seq.checked_add(1), None, 1)
                .await?;
            let row = page
                .rows
                .into_iter()
                .find(|row| row.id == message && row.role == oc_core::session::Role::User)
                .ok_or(CoreError::SessionNotFound)?;
            Ok(Prepared::Copy(row.text))
        }
    }
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    #[cfg(test)]
    let copy = deck.copy_transport.unwrap_or(crate::clipboard::copy);
    #[cfg(not(test))]
    let copy = crate::clipboard::copy;
    let keys: Vec<_> = deck
        .history_reads
        .reads
        .iter()
        .filter(|(_, read)| read.job.is_finished())
        .map(|(key, _)| *key)
        .collect();
    let mut jobs = std::mem::take(&mut deck.history_reads);
    let mut changed = false;
    for key in keys {
        let read = jobs.reads.remove(&key).expect("finished history read");
        let result = read
            .job
            .await
            .unwrap_or_else(|_| Err("history read interrupted".into()));
        let mut cursor = None;
        let mut refresh = false;
        if let Some(view) =
            read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key.0)
        {
            let current = read.route.matches(view);
            if current {
                match result {
                    Ok(Prepared::Window(page, older)) => {
                        if older {
                            view.prepend_page(&page);
                        } else {
                            view.append_page(&page);
                        }
                        refresh = true;
                    }
                    Ok(Prepared::Cards(page, before)) => {
                        let cards = oc_tui::history::cards_from_rows(&page.rows);
                        if before.is_none() {
                            view.apply_cards(cards, page.has_older);
                        } else {
                            view.prepend_cards(cards, page.has_older);
                        }
                        cursor = Some(page.rows.last().map(|row| row.rowid));
                    }
                    Ok(Prepared::Output(op, offset, page)) => {
                        view.apply_card_output(op, offset, page)
                    }
                    Ok(Prepared::Copy(text)) => {
                        if let Err(error) = view.copy_message_text(text) {
                            view.push_note(&error);
                        }
                        report_copy_request_with(view, copy);
                    }
                    Err(error) => view.push_note(&error),
                }
                changed = true;
            }
            // Reject stale rows, but do not lose the still-mounted Cards read
            // when a projection/acceptance fence advanced while it was held.
            // A newer explicit page request wins over repairing the old one.
            let repair = !current
                && read.route.same_owner(view)
                && *view.panel() == TuiPanel::Cards
                && matches!(read.request, Request::Cards(_));
            if let Some(next) = read.next.or(repair.then_some(read.request)) {
                let _ = jobs.request(app, view, next);
            }
        }
        if let Some(cursor) = cursor {
            if state.view_identity().0 == key.0 {
                deck.cards_before = cursor;
            } else if let Some(index) = deck.tabs.iter().position(|view| {
                view.as_ref()
                    .is_some_and(|view| view.view_identity().0 == key.0)
            }) {
                deck.tab_cards_before[index] = cursor;
            }
        }
        if refresh {
            let mut reads = std::mem::take(&mut deck.reads);
            if let Some(view) =
                read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key.0)
            {
                reads.request(view, false);
            }
            deck.reads = reads;
        }
    }
    deck.history_reads = jobs;
    changed
}
