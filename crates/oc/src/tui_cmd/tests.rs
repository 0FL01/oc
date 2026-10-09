//! Shared fixtures for binary routing and accepted lifecycle scenarios.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use oc_core::core_app::InboxMsg;
use oc_core::core_app::WorkerTurnId;
use oc_core::queries::{AutoAcceptState, HistoryMessage, HistoryPage, ToolOpPage, ToolOpView};
use oc_core::session::Role;

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
            id: oc_core::session::MessageId("measured-answer".into()),
            seq: 1,
            role: Role::Assistant,
            text: "cached body".into(),
            model_switch: None,
            child: None,
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
mod routing;
