//! Post-publication reload verification off the terminal task.
use super::*;

struct Target {
    identity: (u64, u64),
    session: Option<SessionId>,
    home: bool,
    staged: Option<String>,
}
pub(super) struct Job {
    location: String,
    draft: Option<String>,
    task: tokio::task::JoinHandle<Result<Prepared, String>>,
}
struct Prepared {
    catalogs: Vec<((u64, u64), CatalogSnapshot, Option<CatalogSnapshot>)>,
    mcp: oc_core::queries::McpSnapshot,
    permission: Option<bool>,
}
impl Job {
    pub(super) fn ready(&self) -> bool {
        self.task.is_finished()
    }
}
pub(super) fn start(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    snapshot: ReloadLocationSnapshot,
    draft: Option<String>,
) -> Result<(), String> {
    if state.chrome.location.as_deref() != Some(snapshot.location.as_str())
        || deck.location.as_deref() != Some(snapshot.location.as_str())
    {
        for view in read_jobs::views_mut(state, deck) {
            view.disable_clipboard_until_catalog();
        }
        return Err(
            "Configuration reload incomplete; visible Location changed during refresh".into(),
        );
    }
    let mut targets = Vec::new();
    for view in read_jobs::views_mut(state, deck) {
        view.chrome.session_tps = snapshot.catalog.chrome.session_tps;
        view.chrome.markdown_copy_modifier = snapshot.catalog.chrome.markdown_copy_modifier;
        view.chrome.copy_markdown_shortcut = snapshot.catalog.chrome.copy_markdown_shortcut.clone();
        view.refresh_clipboard_mode(snapshot.catalog.chrome.terminal_copy);
        targets.push(Target {
            identity: view.view_identity(),
            session: view.attached_session().cloned(),
            home: view.home,
            staged: view.staged_agent(),
        });
    }
    let mounted = state.view_identity();
    targets.sort_by_key(|target| (target.session.is_none(), target.identity == mounted));
    let permission = (snapshot.catalog.auto_accept
        != oc_core::queries::AutoAcceptState::Unsupported)
        .then_some(deck.cli_auto || snapshot.catalog.chrome.permissions_auto);
    let location = snapshot.location;
    let captured = location.clone();
    let owner = app.clone();
    deck.reload_verify = Some(Job {
        location,
        draft,
        task: oc_core::wake::spawn(async move {
            if let Some(auto) = permission {
                owner
                    .register_approval_consumer(auto)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            let mut catalogs = Vec::new();
            for target in targets {
                let catalog = if let Some(session) = &target.session {
                    owner
                        .session_selection(session.clone(), target.home, SelectionAction::Current)
                        .await
                } else {
                    owner.home_selection(SelectionAction::Current).await
                }
                .map_err(|_| {
                    "Configuration reload incomplete; selection refresh failed; retry /reload"
                        .to_string()
                })?;
                if catalog.chrome.location.as_deref() != Some(captured.as_str()) {
                    return Err(
                        "Configuration reload incomplete; Location changed during refresh".into(),
                    );
                }
                let preview = if let Some(agent) = target.staged {
                    if let Some(session) = target.session {
                        owner
                            .session_selection(
                                session,
                                target.home,
                                SelectionAction::Preview(agent),
                            )
                            .await
                            .ok()
                    } else {
                        owner
                            .home_selection(SelectionAction::Preview(agent))
                            .await
                            .ok()
                    }
                } else {
                    None
                };
                catalogs.push((target.identity, catalog, preview));
            }
            if owner
                .catalog()
                .await
                .map_err(|e| e.to_string())?
                .chrome
                .location
                .as_deref()
                != Some(captured.as_str())
            {
                return Err(
                    "Configuration reload incomplete; Location changed during refresh".into(),
                );
            }
            let mcp = owner.mcp_status().await.map_err(|_| {
                "Configuration reload incomplete; MCP status verification failed".to_string()
            })?;
            if mcp.binding.location != captured {
                return Err(
                    "Configuration reload incomplete; MCP Location changed during refresh".into(),
                );
            }
            Ok(Prepared {
                catalogs,
                mcp,
                permission,
            })
        }),
    });
    Ok(())
}
pub(super) async fn sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    if !deck.reload_verify.as_ref().is_some_and(Job::ready) {
        return false;
    }
    let Job {
        location,
        draft,
        task,
    } = deck
        .reload_verify
        .take()
        .expect("finished reload verification");
    match task
        .await
        .unwrap_or_else(|_| Err("Configuration reload verification interrupted".into()))
    {
        Ok(prepared) if deck.location.as_deref() == Some(location.as_str()) => {
            if let Some((_, job)) = deck.mention_job.take() {
                job.abort();
            }
            deck.mention_pending = None;
            deck.mention_failed = None;
            for (identity, catalog, preview) in prepared.catalogs {
                if let Some(view) =
                    read_jobs::views_mut(state, deck).find(|view| view.view_identity() == identity)
                {
                    view.refresh_configuration(catalog);
                    if let Some(preview) = preview {
                        let _ = view.apply_agent_preview(preview);
                    }
                }
            }
            deck.permission_auto = prepared.permission.or(deck.permission_auto);
            apply_mcp_to_views(state, deck, prepared.mcp);
            if draft.as_deref() == Some(state.input()) {
                state.accept_intent();
            }
            state.push_transient_note("Configuration reloaded", NoteVariant::Success);
            deck.controls.request();
        }
        Ok(_) => {
            finish_reload_refusal(
                app,
                state,
                deck,
                "Configuration reload incomplete; visible Location changed during refresh".into(),
            )
            .await
        }
        Err(message) => finish_reload_refusal(app, state, deck, message).await,
    }
    true
}
