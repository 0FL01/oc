//! Shared bounded fixtures for the input, transcript, tab and lifecycle suites.

use super::{
    ClipboardMode, HOME_EXAMPLES, KeyOutcome, LIVE_PARTS_MAX, LivePart, MAX_INPUT_BYTES,
    MAX_SELECTION_BYTES, MAX_SESSION_TITLE_BYTES, NoteVariant, PaintedTranscript, PanelIntent,
    PumpOutcome, ScriptDriver, TabPresentation, TextPoint, Theme, TranscriptSelection, TuiPanel,
    TuiState, TuiStatus, VIEWPORT_LINES,
};
use crate::events::KeyAction;
use crate::history::{WINDOW_BYTES, WINDOW_ROWS};
use oc_core::core_app::{CoreApp, CoreEvent, MockProvider, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_core::queries::{
    AgentEntry, CatalogSnapshot, HistoryMessage, HistoryPage, ModelEntry, SkillCard, VariantEntry,
};
use oc_core::session::{CoreError, Role};
use ratatui::layout::Rect;
use std::time::{Duration, Instant};

pub(super) fn sid(raw: &str) -> SessionId {
    SessionId::new(raw).expect("id")
}

pub(super) fn msg(seq: i64, role: Role, text: &str) -> HistoryMessage {
    HistoryMessage {
        id: oc_core::session::MessageId(format!("fixture-{seq}")),
        turn: None,
        model_switch: None,
        seq,
        role,
        text: text.to_string(),
    }
}

pub(super) fn page(
    rows: Vec<HistoryMessage>,
    total: usize,
    older: bool,
    newer: bool,
) -> HistoryPage {
    HistoryPage {
        parent_id: None,
        title: None,
        reverted: None,
        rows,
        total,
        has_older: older,
        has_newer: newer,
    }
}

pub(super) async fn fresh_state(name: &str) -> TuiState {
    let (app, guard) = CoreApp::spawn(MockProvider::echo());
    std::mem::forget(guard);
    app.create_session(sid(name)).await.expect("create");
    TuiState::new(app, sid(name))
}

fn file_result(
    location: &str,
    generation: u64,
    paths: &[&str],
) -> oc_core::queries::FileSuggestionsSnapshot {
    oc_core::queries::FileSuggestionsSnapshot {
        location: location.into(),
        generation,
        paths: paths.iter().map(|path| (*path).into()).collect(),
        truncated: false,
    }
}

pub(super) fn tab_pointer_at(state: &mut TuiState, area: Rect, index: usize, now: Instant) {
    let rect = crate::shell::tab_strip(state, area)
        .unwrap()
        .tabs
        .iter()
        .find(|tab| tab.index == index)
        .unwrap()
        .rect;
    state.last_mouse = Some((rect.x + 3, rect.y, area));
    state.enter_tab_at(area, rect.x + 3, rect.y, now);
}

async fn submit_echo(
    state: &mut TuiState,
    events: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
    text: &str,
) {
    state.input = text.to_string();
    state.handle_key(KeyAction::Enter).await;
    let start = Instant::now();
    let turn = loop {
        state.poll_submission();
        if let Some(turn) = &state.active_turn {
            break turn.clone();
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "submit was not accepted"
        );
        tokio::task::yield_now().await;
    };
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("echo turn timed out")
            .expect("event channel closed");
        match event {
            CoreEvent::TurnPresentation {
                turn: event_turn,
                projection,
                ..
            } if event_turn == turn => state.apply_presentation(&turn, &projection),
            CoreEvent::TurnFinished {
                turn: event_turn,
                text,
                duration_ms,
                ..
            } if event_turn == turn => {
                state.apply_finished(&turn, &text, duration_ms);
                break;
            }
            CoreEvent::TurnFailed {
                turn: event_turn,
                error,
                ..
            } if event_turn == turn => panic!("echo turn failed: {error}"),
            _ => {}
        }
    }
}

fn user_border_colors(state: &TuiState) -> Vec<ratatui::style::Color> {
    state
        .transcript_lines(100, 120)
        .iter()
        .filter_map(|line| {
            line.spans()
                .iter()
                .find(|span| span.content() == "┃")
                .and_then(|span| span.style().fg)
        })
        .collect()
}

pub(super) async fn type_text(state: &mut TuiState, text: &str) {
    for c in text.chars() {
        state.handle_key(KeyAction::Char(c)).await;
    }
}

async fn await_submission(state: &mut TuiState) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while state.pending.is_some() {
            tokio::task::yield_now().await;
            state.poll_submission();
        }
    })
    .await
    .expect("submission completed");
}

fn snapshot() -> CatalogSnapshot {
    CatalogSnapshot {
        chrome: Default::default(),
        auto_accept: oc_core::queries::AutoAcceptState::Unsupported,
        provider: "ludka2".to_string(),
        models: vec![
            ModelEntry {
                display_name: String::new(),
                provider_name: String::new(),
                price: None,
                id: "a".to_string(),
                variants: Vec::new(),
                context: 1000,
                context_known: true,
                output_known: true,
                output: 100,
            },
            ModelEntry {
                display_name: String::new(),
                provider_name: String::new(),
                price: None,
                id: "b".to_string(),
                variants: vec![VariantEntry {
                    name: "low".to_string(),
                    disabled: false,
                    reasoning_effort: Some("low".to_string()),
                }],
                context: 1000,
                context_known: true,
                output_known: true,
                output: 100,
            },
        ],
        model_id: "a".to_string(),
        variant: None,
        agents: vec![
            AgentEntry {
                id: "x".to_string(),
                color_index: 0,
                description: "first profile".to_string(),
                model: None,
                variant: None,
            },
            AgentEntry {
                id: "y".to_string(),
                color_index: 1,
                description: "second profile".to_string(),
                model: Some("b".to_string()),
                variant: None,
            },
        ],
        agent_id: Some("x".to_string()),
        commands: Vec::new(),
        command_descriptions: Default::default(),
    }
}

fn selection_mouse(
    kind: crossterm::event::MouseEventKind,
    x: u16,
    y: u16,
) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

fn paint_selection_fixture(state: &mut TuiState, frame: Rect, needle: &str) -> (u16, u16) {
    use unicode_width::UnicodeWidthStr;
    let rect = crate::shell::transcript_area(state, frame);
    let (rows, total, scroll) =
        state.visible_transcript_at_viewport(rect.width, frame.width, rect.height);
    state.observe_transcript_viewport(rect.width, frame.width, rect.height, total, scroll);
    let (row, byte) = rows
        .iter()
        .enumerate()
        .find_map(|(row, line)| line.plain_text().find(needle).map(|byte| (row, byte)))
        .expect("painted fixture text");
    let column = UnicodeWidthStr::width(&rows[row].plain_text()[..byte]);
    state.paint_transcript(rect, &rows, total, scroll);
    (rect.x + column as u16, rect.y + row as u16)
}

mod dcp_controls;
mod input;
mod lifecycle;
mod mcp;
mod model_selection;
mod retry;
mod services;
mod tool_output;
mod transcript;

#[tokio::test]
async fn r6_explicit_profile_color_wins_over_categorical_slot() {
    let mut state = fresh_state("profile-color").await;
    let mut catalog = snapshot();
    catalog
        .chrome
        .agent_colors
        .insert("y".into(), "#12ab34".into());
    catalog
        .chrome
        .agent_colors
        .insert("child".into(), "#0000ff".into());
    state.apply_catalog(catalog);
    let colors = crate::theme::Theme::dark().categorical_agents();
    assert_eq!(
        state.agent_color(Some("y")),
        ratatui::style::Color::Rgb(0x12, 0xab, 0x34)
    );
    assert_eq!(
        state.agent_color(Some("child")),
        ratatui::style::Color::Rgb(0, 0, 0xff),
        "subagent-only profiles resolve by id as well"
    );
    assert_eq!(state.agent_color(Some("x")), colors[0]);
}
