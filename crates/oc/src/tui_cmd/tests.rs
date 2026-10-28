//! Shared fixtures for binary routing and accepted lifecycle scenarios.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oc_core::core_app::InboxMsg;
use oc_core::core_app::WorkerTurnId;
use oc_core::queries::{AutoAcceptState, HistoryMessage, HistoryPage, ToolOpPage, ToolOpView};
use oc_core::session::Role;

mod navigation;

// Legacy lifecycle fixtures explicitly pump the same production lane after
// initiating an action. Initiation itself must remain nonblocking (covered by
// the held-receipt tests), so `apply_intent` deliberately does not settle jobs.
macro_rules! settle_lane {
    ($name:ident, $module:ident, $field:ident) => {
        pub(super) async fn $name(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    $module::sync(app, state, deck).await;
                    if !deck.$field.pending() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect(concat!(stringify!($module), " receipt"));
        }
    };
}
settle_lane!(settle_history, history_jobs, history_reads);
settle_lane!(settle_catalog, catalog_jobs, catalogs);
settle_lane!(settle_children, child_jobs, children);
settle_lane!(settle_controls, control_jobs, controls);
settle_lane!(settle_picker, picker_jobs, picker);
settle_lane!(settle_inbox, inbox_jobs, inbox_jobs);
settle_lane!(settle_reads, read_jobs, reads);
settle_lane!(settle_notices, notice_jobs, notices);
pub(super) async fn settle_work(_app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.work.pending() {
            work_controls::sync(state, deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured work control receipt");
}

pub(super) async fn empty_view_resources(
    inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>,
    expected: &str,
) {
    let Some(InboxMsg::ChildJobs { session, ack }) = inbox.recv().await else {
        panic!("child inventory")
    };
    assert_eq!(session.0, expected);
    ack.send(Ok(Vec::new())).unwrap();
    empty_compactions(inbox).await;
    empty_shell_inventory(inbox, expected).await;
}

async fn settle_deck(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            deck_jobs::sync(app, state, deck).await;
            if deck.deck_job.is_none() && deck.deck_read.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured deck receipt");
}

async fn settle_navigation(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.navigation_job.is_some() {
            navigation_jobs::sync(app, state, deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured navigation receipt");
}

async fn settle_home(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            sync_home(app, state, deck).await;
            if deck.home_job.is_none() && deck.home_pending.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured Home selection receipt");
}

async fn settle_resources(state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.resources.pending() {
            resource_jobs::sync(state, deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured resource receipt");
}

async fn settle_provider(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.catalogs.pending() || deck.resources.pending() {
            catalog_jobs::sync(app, state, deck).await;
            resource_jobs::sync(state, deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured provider facts");
    finish_provider_feedback(state, deck);
}

pub(super) async fn settle_reload(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.reload_verify.is_some() {
            reload_jobs::sync(app, state, deck).await;
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured reload verification");
}

async fn settle_workspace(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while deck.shared_refresh || deck.workspace_job.is_some() {
            shared::sync(app, state, deck).await.unwrap();
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("captured workspace read");
}

fn catalog() -> CatalogSnapshot {
    CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: AutoAcceptState::Unsupported,
        provider: "fixture".into(),
        models: Vec::new(),
        model_id: "fixture/model".into(),
        variant: None,
        agents: Vec::new(),
        agent_id: None,
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }
}

async fn empty_compactions(inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>) {
    let Some(InboxMsg::CompactionHistory { ack, .. }) = inbox.recv().await else {
        panic!("compaction replay query")
    };
    ack.send(Ok(Vec::new())).unwrap();
}

async fn accept_prompt_input(
    inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>,
    history: &mut Vec<String>,
    expected: &str,
) {
    let Some(InboxMsg::PromptHistory { append, ack }) =
        tokio::time::timeout(Duration::from_secs(3), inbox.recv())
            .await
            .expect("input history admission must precede command dispatch")
    else {
        panic!("accepted input history")
    };
    assert_eq!(append.as_deref(), Some(expected));
    oc_core::queries::append_prompt_history(history, expected).unwrap();
    ack.send(Ok(history.clone())).unwrap();
}

async fn empty_mcp_status(inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>) {
    empty_mcp_status_at(inbox, "/fixture").await;
}

async fn empty_shell_inventory(inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>, expected: &str) {
    let Some(InboxMsg::ShellJobs { session, ack }) = inbox.recv().await else {
        panic!("initial source-owned shell inventory")
    };
    assert_eq!(session.0, expected);
    ack.send(Ok(Vec::new())).unwrap();
    let Some(InboxMsg::ExecuteJobs { session, ack }) = inbox.recv().await else {
        panic!("initial source-owned execute inventory")
    };
    assert_eq!(session.0, expected);
    ack.send(Ok(Vec::new())).unwrap();
}

async fn empty_mcp_status_at(inbox: &mut tokio::sync::mpsc::Receiver<InboxMsg>, location: &str) {
    let Some(InboxMsg::McpStatus { ack }) = inbox.recv().await else {
        panic!("current owned resource status")
    };
    ack.send(Ok(oc_core::queries::McpSnapshot {
        binding: oc_core::queries::McpBinding {
            location: location.into(),
            generation: 1,
            instance: 1,
        },
        revision: 1,
        servers: Vec::new(),
    }))
    .unwrap();
}

fn tps_page() -> oc_core::queries::HistoryPage {
    oc_core::queries::HistoryPage {
        total: 1,
        rows: vec![oc_core::queries::HistoryMessage {
            images: Vec::new(),
            id: oc_core::session::MessageId("measured-answer".into()),
            seq: 1,
            role: Role::Assistant,
            text: "cached body".into(),
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
            turn: Some(oc_core::queries::HistoryTurn {
                id: "measured-turn".into(),
                status: "completed".into(),
                model_label: "Measured model".into(),
                agent: Some("build".into()),
                duration_ms: Some(1500),
                streamed_ms: Some(4000),
                usage: Some((1000, 200)),
                ..Default::default()
            }),
        }],
        ..Default::default()
    }
}

fn append_tab(app: &CoreApp, deck: &mut LoopState, state: &mut TuiState, id: &str) {
    let old = deck.active_tab.expect("active real tab");
    deck.tabs[old] = Some(std::mem::replace(
        state,
        TuiState::new(app.clone(), SessionId::new(id).unwrap()),
    ));
    deck.tab_cards_before[old] = deck.cards_before;
    deck.active_tab = Some(deck.tabs.len());
    deck.tabs.push(None);
    deck.tab_cards_before.push(None);
    deck.cards_before = None;
    deck.sync_tabs(state);
}

mod lifecycle;
mod projection;
mod responsiveness;
mod routing;
