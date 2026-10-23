//! Captured catalog/model preference preparation, applied on the local view task.
use super::*;
use oc_core::queries::{ModelCatalogs, ModelCommit, ModelPreference, ModelPreferenceAction};
use std::collections::HashMap;

#[derive(Clone)]
pub(super) enum Request {
    Selection {
        action: SelectionAction,
        preview: bool,
    },
    Directory,
    Preference {
        action: Option<ModelPreferenceAction>,
        home: Option<ModelCommit>,
    },
    Provider,
    Custom(oc_core::queries::SelectionBinding),
}
impl Request {
    fn effect(&self) -> bool {
        matches!(self, Self::Preference { .. } | Self::Custom(_)) || self.preparing()
    }
    fn preparing(&self) -> bool {
        matches!(self, Self::Selection { action, .. } if !matches!(action, SelectionAction::Current))
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Fence {
    identity: (u64, u64),
    session: Option<SessionId>,
    route: Option<SessionId>,
    location: Option<String>,
    generation: u64,
    choice: Option<ModelCommit>,
    scope: Option<String>,
    agent: u64,
    dialog: Option<(String, usize)>,
}
impl Fence {
    fn owns(&self, view: &TuiState) -> bool {
        self.identity.0 == view.view_identity().0
            && self.route.as_ref() == view.tab_route_session()
            && self.location == view.chrome.location
            && self.generation == view.chrome.selection_generation
    }
}
fn fence(view: &TuiState) -> Fence {
    Fence {
        identity: view.view_identity(),
        session: view.attached_session().cloned(),
        route: view.tab_route_session().cloned(),
        location: view.chrome.location.clone(),
        generation: view.chrome.selection_generation,
        choice: view.captured_model_commit(),
        scope: view.picker_provider_filter().map(str::to_owned),
        agent: view.agent_selection_revision(),
        dialog: view
            .agent_picker_identity()
            .map(|(query, cursor)| (query.to_owned(), cursor)),
    }
}
struct Prepared {
    catalog: Option<CatalogSnapshot>,
    picker: Option<CatalogSnapshot>,
    directory: Option<(ModelCatalogs, ModelPreference)>,
    preference: Option<ModelPreference>,
    preview: bool,
}
struct Read {
    fence: Fence,
    effect: bool,
    preparing: bool,
    preference: bool,
    refresh: bool,
    next: Option<(Fence, Request)>,
    job: tokio::task::JoinHandle<Result<Prepared, CoreError>>,
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
        self.reads.values().any(|read| read.job.is_finished())
    }
    pub(super) fn owns(&self, view: &TuiState) -> bool {
        self.reads.get(&view.view_identity().0).is_some_and(|read| {
            read.effect
                || read
                    .next
                    .as_ref()
                    .is_some_and(|(_, request)| request.effect())
        })
    }
    pub(super) fn preparing(&self, view: &TuiState) -> bool {
        self.reads.get(&view.view_identity().0).is_some_and(|read| {
            read.preparing
                || read
                    .next
                    .as_ref()
                    .is_some_and(|(_, request)| request.preparing())
        })
    }
    pub(super) fn request(&mut self, app: &CoreApp, view: &mut TuiState, request: Request) {
        let key = view.view_identity().0;
        if let Some(read) = self.reads.get_mut(&key) {
            if read
                .next
                .as_ref()
                .is_some_and(|(_, request)| request.effect())
            {
                if request.effect() {
                    view.push_note(
                        "model preference update pending; additional change was not persisted",
                    );
                } else {
                    read.refresh = true;
                }
                return;
            }
            if request.preparing() {
                view.begin_agent_selection();
            }
            read.next = Some((fence(view), request));
            return;
        }
        if request.preparing() {
            view.begin_agent_selection();
        }
        self.launch(app, view, request, fence(view));
    }
    fn launch(&mut self, app: &CoreApp, view: &TuiState, request: Request, fence: Fence) {
        let key = fence.identity.0;
        let effect = request.effect();
        let preparing = request.preparing();
        let preference = matches!(&request, Request::Preference { .. });
        let captured = fence.clone();
        let owner = app.clone();
        let home = view.home;
        let picker = view.picker_provider_filter().map(str::to_owned);
        let directory = view.model_directory_visible();
        let loaded = view.catalog_loaded();
        self.reads.insert(
            key,
            Read {
                fence,
                effect,
                preparing,
                preference,
                refresh: false,
                next: None,
                job: oc_core::wake::spawn(async move {
                    prepare(&owner, captured, home, picker, directory, loaded, request).await
                }),
            },
        );
    }
}
async fn selection(
    app: &CoreApp,
    fence: &Fence,
    home: bool,
    action: SelectionAction,
) -> Result<CatalogSnapshot, CoreError> {
    if let Some(session) = &fence.session {
        app.session_selection(session.clone(), home, action).await
    } else {
        app.home_selection(action).await
    }
}
async fn prepare(
    app: &CoreApp,
    fence: Fence,
    home: bool,
    picker: Option<String>,
    directory: bool,
    loaded: bool,
    request: Request,
) -> Result<Prepared, CoreError> {
    let mut prepared = Prepared {
        catalog: None,
        picker: None,
        directory: None,
        preference: None,
        preview: false,
    };
    match request {
        Request::Selection { action, preview } => {
            prepared.catalog = Some(selection(app, &fence, home, action).await?);
            prepared.preview = preview;
        }
        Request::Directory => {
            if !loaded {
                prepared.catalog =
                    Some(selection(app, &fence, home, SelectionAction::Current).await?);
            }
            prepared.directory = Some((
                app.model_catalogs().await?,
                app.model_preference(None).await.unwrap_or_default(),
            ));
        }
        Request::Preference { action, home } => {
            if let Some(commit) = home {
                let _ = app.home_selection(SelectionAction::Commit(commit)).await;
            }
            if let Some(action) = action {
                prepared.preference = Some(app.model_preference(Some(action)).await?);
            }
        }
        Request::Provider => {
            prepared.catalog = Some(selection(app, &fence, home, SelectionAction::Current).await?);
            if let Some(provider) = picker {
                prepared.picker = app.provider_catalog(provider).await.ok();
            }
            if directory {
                prepared.directory = Some((
                    app.model_catalogs().await?,
                    app.model_preference(None).await.unwrap_or_default(),
                ));
            }
        }
        Request::Custom(binding) => {
            let current = app.catalog().await?;
            if current.chrome.location == binding.location
                && current.chrome.selection_generation == binding.generation
            {
                app.custom_provider(oc_core::queries::CustomProviderAction::Refresh {
                    provider: binding.provider.clone(),
                })
                .await?;
                prepared.picker = Some(app.provider_catalog(binding.provider).await?);
            }
        }
    }
    let generation = prepared
        .catalog
        .as_ref()
        .map_or(fence.generation, |catalog| {
            catalog.chrome.selection_generation
        });
    let catalogs = prepared.catalog.iter().chain(prepared.picker.iter()).chain(
        prepared
            .directory
            .iter()
            .flat_map(|(directory, _)| directory.catalogs.iter()),
    );
    if catalogs.into_iter().any(|catalog| {
        (fence.location.is_some() && catalog.chrome.location != fence.location)
            || catalog.chrome.selection_generation != generation
    }) {
        return Err(CoreError::LocationChanged);
    }
    Ok(prepared)
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let keys: Vec<_> = deck
        .catalogs
        .reads
        .iter()
        .filter(|(_, read)| read.job.is_finished())
        .map(|(key, _)| *key)
        .collect();
    let mut jobs = std::mem::take(&mut deck.catalogs);
    let mut changed = false;
    for key in keys {
        let read = jobs.reads.remove(&key).expect("finished catalog read");
        let result = read.job.await.unwrap_or(Err(CoreError::Shutdown));
        if let Some(view) =
            read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == key)
        {
            if read.effect
                && read.fence.owns(view)
                && fence(view) != read.fence
                && let Err(error) = &result
            {
                view.push_note(&format!("model preference: {error}"));
                changed = true;
            }
            let applicable = if read.preparing {
                read.fence.owns(view) && read.fence.agent == view.agent_selection_revision()
            } else if read.preference {
                read.fence.owns(view)
            } else {
                fence(view) == read.fence
            };
            if applicable {
                match result {
                    Ok(prepared) => {
                        let close_agents = read.preparing
                            && read.fence.dialog.as_ref().is_some_and(|(query, cursor)| {
                                view.agent_picker_identity() == Some((query.as_str(), *cursor))
                            });
                        let mut success = true;
                        if let Some(catalog) = prepared.catalog {
                            if prepared.preview
                                || read.preparing
                                    && (view.attached_session().is_some()
                                        || view.has_occupied_submission())
                            {
                                if let Err(error) = view.apply_agent_preview(catalog) {
                                    view.push_note(&error);
                                    success = false;
                                }
                            } else {
                                view.apply_catalog_refresh(catalog);
                            }
                        }
                        if let Some(catalog) = prepared.picker {
                            view.apply_picker_catalog(catalog);
                        }
                        if let Some((catalogs, preference)) = prepared.directory {
                            view.apply_model_directory(catalogs, preference);
                        }
                        if let Some(preference) = prepared.preference {
                            view.apply_model_preference(preference);
                        }
                        if close_agents && success {
                            view.close_panel();
                        }
                    }
                    Err(error) => view.push_note(&error.to_string()),
                }
                changed = true;
            }
            if let Some((captured, next)) = read.next {
                if !next.effect() {
                    jobs.request(app, view, next);
                } else if captured.owns(view) {
                    jobs.launch(app, view, next, captured);
                } else {
                    view.push_note(
                        "model preference not persisted; original source changed before dispatch",
                    );
                    changed = true;
                }
            }
            if read.refresh {
                if let Some(next) = jobs.reads.get_mut(&key) {
                    next.refresh = true;
                } else {
                    jobs.request(app, view, Request::Provider);
                }
            }
        }
    }
    deck.catalogs = jobs;
    changed
}
