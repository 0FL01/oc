//! MCP/skill/sidebar resource ports; typed bounded receipts, never a task closure.
use super::*;
use std::collections::HashMap;
pub(super) enum Request {
    Mcp(Option<oc_core::queries::McpControl>),
    Skills,
    Sidebar(bool),
}
enum Data {
    Mcp(oc_core::queries::McpSnapshot),
    Skills(Vec<oc_core::queries::SkillCard>),
    Sidebar(bool),
}
struct Read {
    app: CoreApp,
    identity: (u64, u64),
    location: Option<String>,
    generation: u64,
    effect: bool,
    rerun: bool,
    job: tokio::task::JoinHandle<Result<Data, CoreError>>,
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
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.reads
            .iter()
            .any(|((id, _), read)| *id == view.view_identity().0 && read.effect)
    }
    pub(super) fn request(
        &mut self,
        app: &CoreApp,
        view: &TuiState,
        request: Request,
    ) -> Result<(), String> {
        let kind = match request {
            Request::Mcp(None) => 0,
            Request::Skills => 1,
            Request::Sidebar(_) => 2,
            Request::Mcp(Some(_)) => 3,
        };
        let key = (view.view_identity().0, kind);
        let effect = matches!(&request, Request::Mcp(Some(_)) | Request::Sidebar(_));
        if let Some(read) = self.reads.get_mut(&key) {
            if !effect {
                read.rerun = true;
                return Ok(());
            }
            return Err("resource request pending; original request retained".into());
        }
        let owner = app.clone();
        self.reads.insert(
            key,
            Read {
                app: app.clone(),
                identity: view.view_identity(),
                location: view.chrome.location.clone(),
                generation: view.chrome.selection_generation,
                effect,
                rerun: false,
                job: oc_core::wake::spawn(async move {
                    match request {
                        Request::Mcp(Some(control)) => {
                            owner.mcp_control(control).await.map(Data::Mcp)
                        }
                        Request::Mcp(None) => owner.mcp_status().await.map(Data::Mcp),
                        Request::Skills => owner.skills().await.map(Data::Skills),
                        Request::Sidebar(open) => {
                            owner.sidebar_mcp_open(Some(open)).await.map(Data::Sidebar)
                        }
                    }
                }),
            },
        );
        Ok(())
    }
}
pub(super) async fn sync(state: &mut TuiState, deck: &mut LoopState) -> bool {
    let finished: Vec<_> = deck
        .resources
        .reads
        .iter()
        .filter(|(_, read)| read.job.is_finished())
        .map(|(key, _)| *key)
        .collect();
    let mut changed = false;
    for key in finished {
        let read = deck
            .resources
            .reads
            .remove(&key)
            .expect("finished resource read");
        let result = read.job.await.unwrap_or(Err(CoreError::Shutdown));
        let applicable = read_jobs::views_mut(state, deck).any(|view| {
            view.view_identity().0 == read.identity.0
                && (read.effect || view.view_identity() == read.identity)
                && view.chrome.location == read.location
                && (read.effect || view.chrome.selection_generation == read.generation)
        });
        if applicable {
            match result {
                Ok(Data::Mcp(snapshot)) => apply_mcp_to_views(state, deck, snapshot),
                Ok(Data::Sidebar(open)) => {
                    deck.sidebar_mcp_open = Some(open);
                    for view in read_jobs::views_mut(state, deck) {
                        view.apply_sidebar_mcp_open(open);
                    }
                }
                result => {
                    if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
                        view.view_identity().0 == read.identity.0
                            && (read.effect || view.view_identity() == read.identity)
                    }) {
                        match result {
                            Ok(Data::Skills(cards)) => view.apply_skills(cards),
                            Err(error) => view.push_note(&error.to_string()),
                            _ => unreachable!(),
                        }
                    }
                }
            }
            changed = true;
        }
        if read.rerun || !applicable && !read.effect {
            let mut jobs = std::mem::take(&mut deck.resources);
            if let Some(view) =
                read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key.0)
            {
                let request = if key.1 == 1 {
                    Request::Skills
                } else {
                    Request::Mcp(None)
                };
                let _ = jobs.request(&read.app, view, request);
            }
            deck.resources = jobs;
        }
    }
    changed
}
