//! Real-terminal consumer of the shared native application.
//!
//! The view-model (`oc-tui`) is storage-free: this loop answers its
//! [`PanelIntent`] values through the application API, drains worker events
//! into turn-scoped view state, and owns terminal setup/restore. UI never
//! persists input or outcomes independently of application acceptance.
//! Unit fixtures and routing/lifecycle scenarios live under `tui_cmd/tests`;
//! approval tests retain their existing root module relationship.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CEvent, MouseEventKind};
use futures_util::{Stream, StreamExt};
use ratatui::Terminal;

use oc_adapters::application::{HISTORY_PAGE_LIMIT, TOOL_OPS_PAGE_LIMIT};
use oc_core::core_app::{CoreApp, CoreEvent};
use oc_core::domain::SessionId;
use oc_core::queries::ReloadLocationSnapshot;
use oc_core::queries::{
    CatalogSnapshot, FileSuggestionsSnapshot, SessionSelectionAction as SelectionAction,
};
use oc_core::queries::{SessionProbe, TabDeckSnapshot};
use oc_core::session::{CoreError, LocationSwitchFailure};
use oc_tui::app::{
    KeyOutcome, LiveViewMetrics, MENTION_LIMIT, MentionRequest, NoteVariant, PanelIntent,
    TabPresentation, TuiPanel, TuiState, TuiStatus,
};
use oc_tui::commands::{CommandAction, dispatch};
use oc_tui::dcp_panel::DcpOutcome;
use oc_tui::events::{KeyAction, UiEvent, map_event};
use oc_tui::shell::{StartupFailure, render_background, render_startup_failure};
use oc_tui::terminal::{FrameBackend, enter, install_panic_hook, set_cursor_color};
use oc_tui::views::{cursor_color, render_frame};

mod auth_controls;
mod child_controls;
mod terminal_controls;

/// Qualification probe (T26): when set, panic right after entering the
/// terminal so PTY tests can verify panic-path restoration. Never set in
/// normal use.
const PANIC_PROBE_ENV: &str = "OC_TUI_TEST_PANIC";

/// Qualification probe (T39): when set, write one bounded view-metrics JSON
/// document on exit so PTY tests can assert retained-state bounds. Never set
/// in normal use.
const METRICS_ENV: &str = "OC_TUI_TEST_METRICS";

#[derive(Clone, Copy, Default)]
struct WriteMetrics {
    calls: u64,
    flush_calls: u64,
    bytes: u64,
    sum_ns: u128,
    max_ns: u128,
}

/// Instrument the actual backend writes only when the existing bounded probe
/// is enabled. No terminal content is retained by this instrumentation.
struct TerminalOutput {
    stdout: std::io::Stdout,
    metrics: Option<std::rc::Rc<std::cell::Cell<WriteMetrics>>>,
}

impl Write for TerminalOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let start = self.metrics.as_ref().map(|_| Instant::now());
        let result = self.stdout.write(bytes);
        if let (Some(metrics), Some(start)) = (&self.metrics, start) {
            let mut value = metrics.get();
            let elapsed = start.elapsed().as_nanos();
            value.calls += 1;
            value.bytes += result.as_ref().copied().unwrap_or(0) as u64;
            value.sum_ns += elapsed;
            value.max_ns = value.max_ns.max(elapsed);
            metrics.set(value);
        }
        result
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let start = self.metrics.as_ref().map(|_| Instant::now());
        let result = self.stdout.flush();
        if let (Some(metrics), Some(start)) = (&self.metrics, start) {
            let mut value = metrics.get();
            let elapsed = start.elapsed().as_nanos();
            value.flush_calls += 1;
            value.sum_ns += elapsed;
            value.max_ns = value.max_ns.max(elapsed);
            metrics.set(value);
        }
        result
    }
}

#[derive(Default)]
struct FrameMetrics {
    count: u64,
    sum_ns: u128,
    max_ns: u128,
    worker_event_queue_peak: usize,
    worker_event_queue_lagged: u64,
    live_current: LiveViewMetrics,
    live_peak: LiveViewMetrics,
    started: Option<Instant>,
    frame_samples: Vec<(u128, u128)>,
    monotonic_epoch_ns: Option<u128>,
    frame_monotonic_ns: Vec<u128>,
    wake_monotonic_ns: Vec<u128>,
    wakeups: u64,
    input_events: u64,
    worker_events: u64,
    changed_frames: u64,
    previous: Option<ratatui::buffer::Buffer>,
    writes: WriteMetrics,
}

/// Opt-in PTY metrics share Linux's real monotonic epoch across processes.
/// This observes the scheduler; it never drives or synchronizes animation.
fn monotonic_ns() -> u128 {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes exactly one valid timespec; MONOTONIC has no external effect.
    let result = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut time) };
    assert_eq!(result, 0, "monotonic clock unavailable");
    time.tv_sec as u128 * 1_000_000_000 + time.tv_nsec as u128
}

impl FrameMetrics {
    fn sample_views(&mut self, state: &TuiState, deck: &LoopState) {
        let mut current = state.live_view_metrics();
        for parked in deck.tabs.iter().flatten() {
            current += parked.live_view_metrics();
        }
        if let Some(home) = &deck.home {
            current += home.live_view_metrics();
        }
        self.live_peak.text_bytes = self.live_peak.text_bytes.max(current.text_bytes);
        self.live_peak.reasoning_bytes =
            self.live_peak.reasoning_bytes.max(current.reasoning_bytes);
        self.live_peak.part_count = self.live_peak.part_count.max(current.part_count);
        self.live_peak.markdown_cache_retained_bytes = self
            .live_peak
            .markdown_cache_retained_bytes
            .max(current.markdown_cache_retained_bytes);
        self.live_current = current;
    }
}

/// Max key events drained per frame (paste bursts stay fast; a flooding
/// input still yields to the worker drain below each frame).
const MAX_KEYS_PER_FRAME: usize = 256;
const MAX_WORKER_EVENTS_PER_FRAME: usize = 256;
/// A short coalescing window, independent of animation cadence. Continuous
/// activity cannot postpone its first dirty deadline.
const FRAME_BUDGET: Duration = Duration::from_millis(2);
const BURST_BUDGET: Duration = Duration::from_millis(2);
const MAX_TABS: usize = 16;
const MENTION_DEBOUNCE: Duration = Duration::from_millis(90);

/// Launch the interactive TUI; returns process exit code.
pub async fn run_tui(data_dir: &Path, session_opt: Option<String>, auto_once: bool) -> ExitCode {
    install_panic_hook();
    match run_inner(data_dir, session_opt, auto_once).await {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(1)
        }
    }
}

async fn run_inner(
    data_dir: &Path,
    session_opt: Option<String>,
    auto_once: bool,
) -> Result<ExitCode, String> {
    let outcome = run_stages(data_dir, session_opt, auto_once).await;
    let code = match &outcome {
        Ok(code) => *code,
        Err(_) => 1,
    };
    oc_adapters::trace::log("tui.exit", &format!("code={code}"));
    outcome.map(ExitCode::from)
}

async fn run_stages(
    data_dir: &Path,
    session_opt: Option<String>,
    auto_once: bool,
) -> Result<u8, String> {
    if !at_tty() {
        return Err(
            "no TTY for interactive TUI; use `oc run \"<prompt>\"` for headless use".to_string(),
        );
    }
    let project = std::env::current_dir().map_err(|e| e.to_string())?;
    oc_adapters::trace::log("tui.begin", "project=configured");
    let session = session_opt
        .map(|raw| SessionId::new(raw).ok_or_else(|| "invalid session id".to_string()))
        .transpose()?;
    let (app, guard, _notices) =
        match oc_adapters::application::spawn_with_startup_diagnostic(&project, data_dir).await {
            Ok(runtime) => {
                oc_adapters::trace::log("spawn.ok", "");
                runtime
            }
            Err(issue) => {
                oc_adapters::trace::log("spawn.fail", &issue.to_string());
                eprintln!("error: {issue}");
                let _term = enter()?;
                let mut terminal = Terminal::new(FrameBackend::new(std::io::stdout()))
                    .map_err(|e| format!("terminal: {e}"))?;
                return startup_failure(&mut terminal, StartupFailure::Diagnostic(issue))
                    .map(|_| 1);
            }
        };
    let result = async {
        let catalog = match app.catalog().await {
            Ok(catalog) => catalog,
            Err(error) => return startup_query_frame(error),
        };
        if let Err(error) = app
            .register_approval_consumer(auto_once || catalog.chrome.permissions_auto)
            .await
        {
            return startup_query_frame(error);
        }
        if let Some(id) = session.as_ref().filter(|id| !valid_tab_id(&id.0)) {
            match app.probe_session(id.clone()).await {
                Ok(SessionProbe::Absent) => Err(
                    "invalid --session id: use a trimmed, non-control ID of at most 128 bytes"
                        .to_string(),
                ),
                Err(error) => startup_query_frame(error),
                _ => drive_ui(&app, session, auto_once).await,
            }
        } else {
            drive_ui(&app, session, auto_once).await
        }
    }
    .await;
    let shutdown = app.shutdown().await;
    guard
        .join_diagnostic()
        .await
        .map_err(|e| format!("application worker: {e}"))?;
    shutdown.map_err(|error| error.to_string())?;
    result
}

fn startup_query_frame(error: CoreError) -> Result<u8, String> {
    let failure = startup_query(error);
    if let StartupFailure::QueryDiagnostic(diagnostic) = &failure {
        eprintln!("error: {diagnostic}");
    }
    let _term = enter()?;
    let mut terminal = Terminal::new(FrameBackend::new(std::io::stdout()))
        .map_err(|error| format!("terminal: {error}"))?;
    startup_failure(&mut terminal, failure).map(|_| 1)
}

/// Owner receipt plus the slash-editor revision; palette admission has no revision.
type CompactionAdmission = tokio::task::JoinHandle<
    Result<(oc_core::compaction::CompactionSnapshot, Option<u64>), CoreError>,
>;

/// Loop-local application state that is not part of the view-model.
#[derive(Default)]
struct LoopState {
    authentication: auth_controls::Controls,
    questions: oc_tui::question_view::QuestionView,
    cli_auto: bool,
    permission_auto: Option<bool>,
    approvals_checked: Option<(Option<SessionId>, Instant)>,
    compaction_job: Option<CompactionAdmission>,
    /// Bounded prompt drafts for picker Location round trips. Views and
    /// configuration are rebuilt from the newly accepted owner receipt.
    picker_drafts: std::collections::BTreeMap<String, Vec<(Option<SessionId>, String)>>,
    picker_pending_tabs: std::collections::HashSet<SessionId>,
    conversation_job: Option<tokio::task::JoinHandle<Result<ConversationResult, String>>>,
    conversation_recovery: Option<(SessionId, std::time::Instant)>,
    recovery_job: Option<ConversationRefresh>,
    #[cfg(test)]
    copy_transport: Option<CopyTransport>,
    mention_pending: Option<(MentionRequest, Instant)>,
    mention_job: Option<(
        MentionRequest,
        tokio::task::JoinHandle<Result<FileSuggestionsSnapshot, CoreError>>,
    )>,
    mention_failed: Option<MentionRequest>,
    /// Provider work is awaited separately from the synchronous terminal loop.
    title_job: Option<(
        SessionId,
        tokio::task::JoinHandle<Result<String, CoreError>>,
    )>,
    /// The owner rebuild runs off the terminal loop so the progress notice
    /// paints before the final outcome. Only quit and resize are accepted meanwhile.
    reload_job: Option<tokio::task::JoinHandle<Result<ReloadLocationSnapshot, CoreError>>>,
    reload_draft: Option<String>,
    reload_painted: bool,
    /// Turn started by a manual `/dcp-compress` request.
    /// Cursor for paging older tool cards.
    cards_before: Option<i64>,
    /// DCP snapshot was fetched for the currently open panel.
    dcp_seen: bool,
    /// Ordered real tabs. The selected tab lives in `drive_ui`'s `state`
    /// instead of being duplicated here. A sessionless Home is a synthetic
    /// slot outside this vector until its first turn is accepted.
    tabs: Vec<Option<TuiState>>,
    /// Card cursors follow the same slots as `tabs` (including the active slot).
    tab_cards_before: Vec<Option<i64>>,
    active_tab: Option<usize>,
    home: Option<TuiState>,
    /// Binding and opaque CAS token from the owner. Directly constructed
    /// mock decks have no Location and do not write a preference.
    location: Option<String>,
    revision: Option<String>,
    /// A projected or unreadable preference cannot be safely rewritten from
    /// the visible tabs: hidden IDs must survive until a clean reload/repair.
    save_disabled: bool,
    /// A child requested by --session is a standalone history view. Never
    /// submit a turn or promote it into the Location's root-tab preference.
    read_only: bool,
    /// Linked child views are disposable; the real root deck is retained here.
    child_parent: Option<Box<TuiState>>,
    child_views: std::collections::HashMap<SessionId, TuiState>,
}

enum ConversationResult {
    Changed(oc_core::queries::ConversationSnapshot, ConversationRefresh),
    Forked(oc_core::queries::ForkSessionSnapshot, ConversationRefresh),
}

#[cfg(test)]
type CopyTransport = fn(&str) -> Result<(), String>;

type ConversationRefresh = tokio::task::JoinHandle<(
    Result<oc_core::queries::HistoryPage, String>,
    Result<CatalogSnapshot, String>,
    Result<Vec<oc_core::compaction::CompactionSnapshot>, String>,
)>;

fn conversation_refresh(app: CoreApp, session: SessionId) -> ConversationRefresh {
    tokio::spawn(async move {
        let page = app
            .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
            .await
            .map_err(|e| e.to_string());
        let catalog = app
            .session_selection(session.clone(), false, SelectionAction::Current)
            .await
            .map_err(|e| e.to_string());
        let compactions = app
            .compaction_history(session)
            .await
            .map_err(|e| e.to_string());
        (page, catalog, compactions)
    })
}

async fn apply_conversation_refresh(
    app: &CoreApp,
    state: &mut TuiState,
    refresh: ConversationRefresh,
) -> bool {
    match refresh.await {
        Ok((page, catalog, compactions)) => {
            let success = page.is_ok() && catalog.is_ok() && compactions.is_ok();
            match page {
                Ok(page) => state.attach_page(&page),
                Err(error) => state.push_transient_note(
                    &format!("Conversation saved; history refresh failed: {error}"),
                    NoteVariant::Error,
                ),
            }
            match catalog {
                Ok(catalog) => state.apply_catalog(catalog),
                Err(error) => state.push_transient_note(
                    &format!("Conversation saved; metadata refresh failed: {error}"),
                    NoteVariant::Error,
                ),
            }
            match compactions {
                Ok(snapshots) => state.apply_compaction_history(snapshots),
                Err(error) => state.push_transient_note(
                    &format!("Conversation saved; compaction refresh failed: {error}"),
                    NoteVariant::Error,
                ),
            }
            refresh_dcp_summaries(app, state).await;
            success
        }
        Err(_) => {
            state.push_transient_note(
                "Conversation saved; refresh interrupted",
                NoteVariant::Error,
            );
            false
        }
    }
}

impl LoopState {
    fn retire_parked_auth(&mut self) {
        for view in self
            .tabs
            .iter_mut()
            .flatten()
            .chain(self.home.iter_mut())
            .chain(self.child_views.values_mut())
        {
            view.retire_auth_surface();
        }
        if let Some(parent) = self.child_parent.as_deref_mut() {
            parent.retire_auth_surface();
        }
    }
    fn has_jobs(&self) -> bool {
        self.job_lanes() != 0
    }

    fn job_lanes(&self) -> u8 {
        u8::from(self.conversation_job.is_some())
            | (u8::from(self.recovery_job.is_some()) << 1)
            | (u8::from(self.mention_job.is_some()) << 2)
            | (u8::from(self.title_job.is_some()) << 3)
            | (u8::from(self.reload_job.is_some()) << 4)
            | (u8::from(self.compaction_job.is_some()) << 5)
            | (u8::from(self.authentication.pending()) << 6)
    }

    fn ready_job(&self) -> bool {
        self.authentication.ready()
            || self
                .compaction_job
                .as_ref()
                .is_some_and(|job| job.is_finished())
            || self
                .conversation_job
                .as_ref()
                .is_some_and(|job| job.is_finished())
            || self
                .recovery_job
                .as_ref()
                .is_some_and(|job| job.is_finished())
            || self
                .mention_job
                .as_ref()
                .is_some_and(|(_, job)| job.is_finished())
            || self
                .title_job
                .as_ref()
                .is_some_and(|(_, job)| job.is_finished())
            || self
                .reload_job
                .as_ref()
                .is_some_and(|job| job.is_finished())
    }

    fn can_open_session(&self) -> bool {
        self.tabs.len() < MAX_TABS - usize::from(self.home.is_some() || self.active_tab.is_none())
    }

    fn snapshot(&self, state: &TuiState) -> TabDeckSnapshot {
        let state = self.child_parent.as_deref().unwrap_or(state);
        let sessions = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, parked)| {
                let view = if self.active_tab == Some(index) {
                    state
                } else {
                    parked.as_ref().expect("parked tab")
                };
                view.attached_session()
                    .expect("real tab has session")
                    .clone()
            })
            .collect();
        TabDeckSnapshot {
            location: self.location.clone().unwrap_or_default(),
            revision: self.revision.clone(),
            sessions,
            new_session_titles: {
                let flags = self
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(index, parked)| {
                        if self.active_tab == Some(index) {
                            state.new_session_tab
                        } else {
                            parked.as_ref().expect("parked tab").new_session_tab
                        }
                    })
                    .collect::<Vec<_>>();
                if flags.iter().any(|value| *value) {
                    flags
                } else {
                    Vec::new()
                }
            },
            active: self
                .active_tab
                .and_then(|_| state.attached_session().cloned()),
        }
    }

    async fn save(&mut self, app: &CoreApp, state: &mut TuiState) {
        if self.save_disabled {
            state.push_note("tab deck could not be saved; review saved tabs");
        } else if self.location.as_deref() == Some("") {
            state.push_note("tab deck could not be saved; invalid Location binding");
        } else if self.save_checked(app, state).await.is_err() {
            // The action was accepted locally. A conflict must not replace
            // the token, or retry against an unrelated Location/revision.
            state.push_note(
                "tab deck could not be saved; saved tabs changed or storage unavailable",
            );
        }
    }

    async fn save_checked(&mut self, app: &CoreApp, state: &TuiState) -> Result<(), CoreError> {
        self.save_snapshot_checked(app, self.snapshot(state)).await
    }

    async fn save_snapshot_checked(
        &mut self,
        app: &CoreApp,
        requested: TabDeckSnapshot,
    ) -> Result<(), CoreError> {
        if self.save_disabled {
            return Err(CoreError::StoredTabDeck);
        }
        let Some(location) = self.location.as_ref() else {
            return Ok(());
        };
        if location.is_empty() || requested.location != *location {
            return Err(CoreError::InvalidTabDeck);
        }
        match app.save_tab_deck(requested.clone()).await {
            Ok(saved) if saved.location == requested.location && saved.revision.is_some() => {
                self.revision = saved.revision;
                Ok(())
            }
            Ok(_) => Err(CoreError::TabDeckStorage),
            Err(error) => Err(error),
        }
    }

    async fn save_if_changed(
        &mut self,
        app: &CoreApp,
        state: &mut TuiState,
        before: &TabDeckSnapshot,
    ) {
        if self.snapshot(state) != *before {
            self.save(app, state).await;
        }
    }

    fn sync_tabs(&mut self, state: &mut TuiState) {
        state.set_service_feedback_visible(true);
        for parked in self.tabs.iter_mut().flatten() {
            parked.set_service_feedback_visible(false);
        }
        if let Some(home) = self.home.as_mut() {
            home.set_service_feedback_visible(false);
        }
        let showing_child = self.child_parent.is_some();
        if !showing_child && let Some(auto) = self.permission_auto {
            state.auto_accept = if auto {
                oc_core::queries::AutoAcceptState::Enabled
            } else {
                oc_core::queries::AutoAcceptState::Disabled
            };
        }
        state.tab_attention = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, parked)| {
                let view = if self.active_tab == Some(index) {
                    self.child_parent.as_deref().unwrap_or(state)
                } else {
                    parked.as_ref()?
                };
                view.attached_session()
                    .filter(|s| state.approval_roots.contains(&s.0))
                    .map(|_| index)
            })
            .collect();
        // Bare Home has no tab. Acceptance attaches it to the first real
        // session; subsequent Home submissions append to the existing deck.
        if !showing_child && self.active_tab.is_none() && state.attached_session().is_some() {
            debug_assert!(self.tabs.len() < MAX_TABS);
            state.new_session_tab = true;
            self.active_tab = Some(self.tabs.len());
            self.tabs.push(None);
            self.tab_cards_before.push(self.cards_before);
        }
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(index, parked)| {
                let view = if self.active_tab == Some(index) {
                    self.child_parent.as_deref().unwrap_or(state)
                } else {
                    parked.as_ref().expect("parked tab")
                };
                TabPresentation {
                    title: view
                        .session_title
                        .clone()
                        .or_else(|| view.tab_title_fallback().map(str::to_owned)),
                    detail: view.chrome.location.as_deref().and_then(|location| {
                        std::path::Path::new(location)
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                    }),
                    home: false,
                    busy: view.is_busy(),
                    attention: state
                        .tab_attention
                        .contains(&index)
                        .then_some(oc_tui::app::TabAttention::Permission),
                    ..TabPresentation::new(
                        view.attached_session()
                            .expect("real tab has session")
                            .clone(),
                    )
                }
            })
            .collect();
        let active = self.active_tab.unwrap_or(self.tabs.len());
        let can_add = !showing_child
            && (!state.is_busy() || state.approvals.active().is_some())
            && self.can_open_session();
        let (shown, selected, allowed) = state.tab_presentation();
        // `set_tab_strip` cancels a pending mouse Down: never call it while
        // nothing painted has changed, even across the 50ms redraw loop.
        if shown != tabs || selected != active || allowed != (can_add && !tabs.is_empty()) {
            state.set_tab_strip(tabs, active, can_add);
        }
    }

    fn activate(&mut self, state: &mut TuiState, index: usize) -> Result<(), String> {
        self.activate_with_hover(state, index, false)
    }

    fn activate_with_hover(
        &mut self,
        state: &mut TuiState,
        index: usize,
        preserve_pointer: bool,
    ) -> Result<(), String> {
        if state.is_busy() {
            return Err("turn active; session switch refused".into());
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        if self.active_tab == Some(index) {
            state.close_panel();
            return Ok(());
        }
        if !preserve_pointer {
            state.clear_mouse_position();
        }
        state.close_panel();
        let mut next = self.tabs[index].take().expect("parked tab");
        next.approval_roots.clone_from(&state.approval_roots);
        next.sync_clipboard_mode_from(state);
        next.take_tab_clocks_from(state);
        let previous = std::mem::replace(state, next);
        state.invalidate_file_suggestions();
        if let Some(old) = self.active_tab {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        } else {
            self.home = Some(previous);
        }
        self.active_tab = Some(index);
        self.cards_before = self.tab_cards_before[index];
        self.dcp_seen = false;
        self.sync_tabs(state);
        Ok(())
    }

    fn open_home(&mut self, state: &mut TuiState, mut next: TuiState) {
        next.inherit_committed_model_choice(state);
        next.approval_roots.clone_from(&state.approval_roots);
        state.close_panel();
        next.take_tab_clocks_from(state);
        let previous = std::mem::replace(state, next);
        if let Some(old) = self.active_tab.take() {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        }
        // An existing synthetic Home is replaced by a fresh `/new` Home.
        self.home = None;
        self.cards_before = None;
        self.dcp_seen = false;
        self.sync_tabs(state);
    }

    fn restore_home(&mut self, state: &mut TuiState) -> bool {
        let Some(mut home) = self.home.take() else {
            return false;
        };
        home.approval_roots.clone_from(&state.approval_roots);
        state.close_panel();
        home.sync_clipboard_mode_from(state);
        home.take_tab_clocks_from(state);
        let old = self.active_tab.take().expect("Home parked from a tab");
        self.tabs[old] = Some(std::mem::replace(state, home));
        state.invalidate_file_suggestions();
        self.tab_cards_before[old] = self.cards_before;
        self.cards_before = None;
        self.dcp_seen = false;
        self.sync_tabs(state);
        true
    }

    async fn close_tab(
        &mut self,
        app: &CoreApp,
        state: &mut TuiState,
        index: usize,
    ) -> Result<(), String> {
        if state.is_busy() {
            return Err("turn active; tab close refused".into());
        }
        if index == self.tabs.len() {
            // Home is a synthetic slot, and cannot be closed when it is the
            // only route. When selected, activate the last real view before
            // discarding the parked Home (and its draft).
            if self.active_tab.is_none() && !self.tabs.is_empty() {
                let mut candidate = self.snapshot(state);
                candidate.active = self.tabs.last().and_then(|view| {
                    view.as_ref()
                        .and_then(|view| view.attached_session())
                        .cloned()
                });
                self.save_snapshot_checked(app, candidate)
                    .await
                    .map_err(|_| "tab close refused; saved tabs unavailable".to_string())?;
                self.activate(state, self.tabs.len() - 1)?;
            } else if self.home.is_none() {
                return Err("tab unavailable".into());
            }
            self.home = None;
            self.sync_tabs(state);
            return Ok(());
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        // Fetch a replacement Home while the active tab and its draft still
        // exist. A failed query must not retire the pending-root marker or
        // change the visible route.
        let replacement_home =
            if self.active_tab == Some(index) && self.tabs.len() == 1 && self.home.is_none() {
                let catalog = app
                    .home_selection(SelectionAction::Current)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut home = TuiState::new_home(app.clone());
                home.apply_catalog(catalog);
                Some(home)
            } else {
                None
            };
        // A fresh Home root can be accepted before its first preference save
        // succeeds. Retire the owner's pending-root marker while the full
        // visible deck still contains that root; otherwise closing it locally
        // can make a later restart resurrect an explicitly closed tab.
        // This CAS also refuses stale or unreadable projected preferences.
        self.save_checked(app, state)
            .await
            .map_err(|_| "tab close refused; saved tabs unavailable".to_string())?;
        let mut candidate = self.snapshot(state);
        candidate.sessions.remove(index);
        if !candidate.new_session_titles.is_empty() {
            candidate.new_session_titles.remove(index);
            if !candidate.new_session_titles.iter().any(|value| *value) {
                candidate.new_session_titles.clear();
            }
        }
        candidate.active = if self.active_tab == Some(index) {
            // Immediately previous if available, otherwise the next tab.
            (self.tabs.len() > 1).then(|| candidate.sessions[index.saturating_sub(1)].clone())
        } else {
            candidate.active
        };
        self.save_snapshot_checked(app, candidate)
            .await
            .map_err(|_| "tab close refused; saved tabs unavailable".to_string())?;
        if self.active_tab == Some(index) {
            if self.tabs.len() == 1 {
                if self.home.is_some() {
                    self.restore_home(state);
                    self.tabs.clear();
                    self.tab_cards_before.clear();
                    self.sync_tabs(state);
                    return Ok(());
                }
                *state = replacement_home.expect("Home fetched before close save");
                self.reset_deck();
                self.sync_tabs(state);
                return Ok(());
            }
            // Immediately previous if available, otherwise the next tab.
            let survivor = if index > 0 { index - 1 } else { 1 };
            self.activate(state, survivor)
                .expect("idle surviving tab is available");
        }
        // The former active view is now parked. Removing the parallel cursor
        // at the same index keeps paging state aligned with the real tabs.
        self.tabs.remove(index);
        self.tab_cards_before.remove(index);
        if let Some(active) = self.active_tab.as_mut()
            && *active > index
        {
            *active -= 1;
        }
        self.sync_tabs(state);
        Ok(())
    }

    fn reset_deck(&mut self) {
        self.tabs.clear();
        self.tab_cards_before.clear();
        self.active_tab = None;
        self.home = None;
        self.cards_before = None;
        self.dcp_seen = false;
    }
}

async fn finish_conversation(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    if deck
        .conversation_recovery
        .as_ref()
        .is_some_and(|(session, _)| state.attached_session() != Some(session))
    {
        deck.conversation_recovery = None;
        if let Some(job) = deck.recovery_job.take() {
            job.abort();
        }
    }
    if deck
        .recovery_job
        .as_ref()
        .is_some_and(|job| job.is_finished())
    {
        let job = deck.recovery_job.take().expect("finished refresh");
        if apply_conversation_refresh(app, state, job).await {
            deck.conversation_recovery = None;
        } else if let Some((_, retry)) = &mut deck.conversation_recovery {
            *retry = std::time::Instant::now() + std::time::Duration::from_secs(1);
        }
        deck.sync_tabs(state);
    }
    if deck.conversation_job.is_none()
        && deck.recovery_job.is_none()
        && let Some((session, retry)) = &deck.conversation_recovery
        && *retry <= std::time::Instant::now()
    {
        if state.attached_session() == Some(session) {
            deck.recovery_job = Some(conversation_refresh(app.clone(), session.clone()));
        } else {
            deck.conversation_recovery = None;
        }
    }
    if !deck
        .conversation_job
        .as_ref()
        .is_some_and(|job| job.is_finished())
    {
        return;
    }
    let job = deck
        .conversation_job
        .take()
        .expect("finished conversation job");
    match job
        .await
        .unwrap_or_else(|_| Err("conversation operation interrupted".into()))
    {
        Ok(ConversationResult::Changed(snapshot, refresh)) => {
            if state.attached_session() != Some(&snapshot.session) {
                return;
            }
            state.conversation_applied(&snapshot, &Default::default());
            if !apply_conversation_refresh(app, state, refresh).await {
                deck.conversation_recovery = Some((
                    snapshot.session,
                    std::time::Instant::now() + std::time::Duration::from_secs(1),
                ));
            }
            deck.cards_before = None;
            deck.dcp_seen = false;
            deck.sync_tabs(state);
        }
        Ok(ConversationResult::Forked(snapshot, refresh)) => {
            let before = deck.snapshot(state);
            let mut next = TuiState::new(app.clone(), snapshot.session.clone());
            next.restore_prompt(snapshot.prompt);
            state.close_panel();
            next.take_tab_clocks_from(state);
            if let Some(old) = deck.active_tab.take() {
                deck.tabs[old] = Some(std::mem::replace(state, next));
                deck.tab_cards_before[old] = deck.cards_before;
            } else {
                deck.home = Some(std::mem::replace(state, next));
            }
            deck.active_tab = Some(deck.tabs.len());
            deck.tabs.push(None);
            deck.tab_cards_before.push(None);
            deck.cards_before = None;
            deck.dcp_seen = false;
            deck.sync_tabs(state);
            deck.save_if_changed(app, state, &before).await;
            if !apply_conversation_refresh(app, state, refresh).await {
                deck.conversation_recovery = Some((
                    snapshot.session,
                    std::time::Instant::now() + std::time::Duration::from_secs(1),
                ));
            }
            deck.sync_tabs(state);
        }
        Err(error) => state.push_transient_note(&error, NoteVariant::Error),
    }
}

async fn refresh_approvals(app: &CoreApp, state: &mut TuiState) -> Result<(), String> {
    let pending = app.pending_approvals().await.map_err(|e| e.to_string())?;
    state.project_pending_approvals(&pending);
    let mut visible = Vec::new();
    let mut roots = std::collections::BTreeSet::new();
    let attached = state.attached_session().cloned();
    for request in pending {
        let mut current = SessionId(request.binding.session.clone());
        if state.parent_id.is_none() && state.user_shell_admission_session() == Some(&current) {
            roots.insert(current.0.clone());
            visible.push((request, false));
            continue;
        }
        let mut child = false;
        let mut belongs = false;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            if !seen.insert(current.0.clone()) {
                break;
            }
            let page = app
                .history_page(current.clone(), None, None, 1)
                .await
                .map_err(|e| e.to_string())?;
            if current.0 == request.binding.session {
                child = page.parent_id.is_some();
            }
            if Some(&current) == attached.as_ref() {
                belongs = true;
            }
            let Some(parent) = page.parent_id else {
                roots.insert(current.0);
                break;
            };
            current = SessionId(parent);
        }
        if belongs && state.parent_id.is_none() {
            visible.push((request, child));
        }
    }
    visible.sort_by_key(|(r, _)| {
        (
            Some(&SessionId(r.binding.session.clone())) != attached.as_ref(),
            r.id,
        )
    });
    state.approval_roots = roots;
    state.approvals.reconcile(visible);
    let all = app.pending_questions().await.map_err(|e| e.to_string())?;
    let mut visible = Vec::new();
    for request in &all {
        let mut current = SessionId(request.binding.session.clone());
        let mut belongs = false;
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            if !seen.insert(current.0.clone()) {
                break;
            }
            if Some(&current) == attached.as_ref() {
                belongs = true;
            }
            let page = app
                .history_page(current.clone(), None, None, 1)
                .await
                .map_err(|e| e.to_string())?;
            let Some(parent) = page.parent_id else {
                break;
            };
            current = SessionId(parent);
        }
        if belongs {
            visible.push(request.clone());
        }
    }
    visible.sort_by_key(|r| {
        (
            Some(&SessionId(r.binding.session.clone())) != attached.as_ref(),
            r.id,
        )
    });
    state.questions.reconcile(&all, visible);
    Ok(())
}

async fn drive_ui(app: &CoreApp, session: Option<SessionId>, cli_auto: bool) -> Result<u8, String> {
    let _term = enter()?;
    if std::env::var_os(PANIC_PROBE_ENV).is_some() {
        panic!("{PANIC_PROBE_ENV} probe");
    }
    let output_metrics = std::env::var_os(METRICS_ENV)
        .map(|_| std::rc::Rc::new(std::cell::Cell::new(WriteMetrics::default())));
    let backend = FrameBackend::new(TerminalOutput {
        stdout: std::io::stdout(),
        metrics: output_metrics.clone(),
    });
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;
    // Subscribe before reading any view snapshots: a fast optional catalog
    // completion between those reads must remain observable.
    let mut rx = app.subscribe();
    let (mut state, mut loop_state) = match restore_initial(app, session).await {
        Ok(restored) => restored,
        Err(failure) => return startup_failure(&mut terminal, failure).map(|_| 1),
    };
    loop_state.cli_auto = cli_auto;
    app.register_question_consumer()
        .await
        .map_err(|e| e.to_string())?;
    loop_state.permission_auto = Some(
        app.catalog().await.map_err(|e| e.to_string())?.auto_accept
            == oc_core::queries::AutoAcceptState::Enabled,
    );
    refresh_approvals(app, &mut state).await?;
    loop_state.sync_tabs(&mut state);
    let mut frame_metrics = std::env::var_os(METRICS_ENV).map(|_| FrameMetrics::default());
    // One Crossterm reader owns both terminal input and resize. Mixing its
    // Mio reader with a second stdin readiness poller can lose edge events.
    let mut input = event::EventStream::new();
    let mut dirty = true;
    let mut first_paint = true;
    let mut painted_cursor_color = ratatui::style::Color::Reset;
    let mut paint_at = Instant::now();
    let mut worker_ready = None;
    let mut input_pending = std::collections::VecDeque::new();

    loop {
        let had_pending = state.has_pending_submission();
        state.questions.share_drafts(&loop_state.questions);
        let job_lanes = loop_state.job_lanes();
        let job_ready = loop_state.ready_job();
        dirty |= state.tick_ui(Instant::now());
        loop_state.retire_parked_auth();
        dirty |= loop_state.authentication.sync(app, &mut state).await;
        poll_and_sync(app, &mut state, &mut loop_state).await;
        if state.terminals_need_refresh() {
            terminal_controls::refresh_or_report(app, &mut state).await;
            dirty = true;
        }
        if loop_state
            .approvals_checked
            .as_ref()
            .is_none_or(|(session, at)| {
                session.as_ref() != state.attached_session()
                    || at.elapsed() >= Duration::from_millis(500)
            })
        {
            if state.children_open() || state.linked_child().is_some() {
                if let Err(error) = child_controls::refresh(app, &mut state).await {
                    state.push_note(&error);
                }
                dirty = true;
            }
            let before = state.approvals.active().cloned();
            let question_before = state.questions.active().cloned();
            let roots_before = state.approval_roots.clone();
            if let Err(error) = refresh_approvals(app, &mut state).await {
                state.approvals.error = Some(error);
            }
            dirty |= before.as_ref() != state.approvals.active();
            dirty |= question_before.as_ref() != state.questions.active();
            dirty |= roots_before != state.approval_roots;
            loop_state.sync_tabs(&mut state);
            loop_state.approvals_checked =
                Some((state.attached_session().cloned(), Instant::now()));
        }
        dirty |= sync_mention(app, &mut state, &mut loop_state).await;
        finish_conversation(app, &mut state, &mut loop_state).await;
        finish_compaction_admission(&mut state, &mut loop_state).await;
        dirty |= job_ready || had_pending != state.has_pending_submission();
        if loop_state.reload_job.is_some()
            && loop_state.reload_painted
            && loop_state
                .reload_job
                .as_ref()
                .is_some_and(|job| job.is_finished())
        {
            let job = loop_state.reload_job.take().expect("finished reload job");
            let draft = loop_state.reload_draft.take();
            match job.await.unwrap_or(Err(CoreError::Shutdown)) {
                Ok(snapshot) => {
                    match finish_reload(app, &mut state, &mut loop_state, snapshot, draft).await {
                        Ok(()) => {}
                        Err(message) => {
                            finish_reload_refusal(app, &mut state, &mut loop_state, message).await
                        }
                    }
                }
                Err(error) => {
                    finish_reload_refusal(app, &mut state, &mut loop_state, reload_error(error))
                        .await
                }
            }
            loop_state.reload_painted = false;
            loop_state.sync_tabs(&mut state);
        }
        if loop_state
            .title_job
            .as_ref()
            .is_some_and(|(_, job)| job.is_finished())
        {
            let (session, job) = loop_state.title_job.take().expect("finished title job");
            let result = job.await.unwrap_or(Err(CoreError::Shutdown)).map_err(|_| {
                "title generation unavailable or title changed; retry /rename".to_string()
            });
            if state.attached_session() == Some(&session) {
                state.regenerated_title(result);
            } else if let Some(view) = loop_state
                .tabs
                .iter_mut()
                .flatten()
                .find(|view| view.attached_session() == Some(&session))
            {
                view.regenerated_title(result);
            }
            loop_state.sync_tabs(&mut state);
        }
        // Reconciliation may have just accepted the first running turn. Arm
        // its clock before the first painted frame, rather than one poll late.
        // A job can finish after ready_job was sampled above. Its removal or
        // replacement must still paint the accepted receipt in this iteration.
        dirty |= job_lanes != loop_state.job_lanes();
        dirty |= state.tick_scanner(Instant::now());
        if dirty && Instant::now() >= paint_at {
            let draw_start = frame_metrics
                .as_ref()
                .map(|_| (Instant::now(), monotonic_ns()));
            terminal
                .backend_mut()
                .begin_frame()
                .map_err(|e| format!("begin draw: {e}"))?;
            if first_paint {
                // Establish base cells before the first content diff, just as
                // subsequent frames inherit an already painted root canvas.
                // Include this one-time paint in the same measured UI wake.
                terminal
                    .draw(render_background)
                    .map_err(|e| format!("draw background: {e}"))?;
                first_paint = false;
            }
            if state.terminal_visible().is_some() {
                let size = terminal.size().map_err(|e| format!("terminal size: {e}"))?;
                terminal_controls::resize(
                    app,
                    &mut state,
                    ratatui::layout::Rect::new(0, 0, size.width, size.height),
                )
                .await;
            }
            let color = cursor_color(&state);
            if color != painted_cursor_color {
                set_cursor_color(terminal.backend_mut(), color)
                    .map_err(|e| format!("cursor color: {e}"))?;
                painted_cursor_color = color;
            }
            let painted = terminal
                .draw(|frame| render_frame(frame, &state))
                .map_err(|e| format!("draw: {e}"))?;
            if let Some(metrics) = &mut frame_metrics {
                if metrics.previous.as_ref() != Some(painted.buffer) {
                    metrics.changed_frames += 1;
                }
                metrics.previous = Some(painted.buffer.clone());
            }
            terminal
                .backend_mut()
                .finish_frame()
                .map_err(|e| format!("publish draw: {e}"))?;
            if let (Some(metrics), Some((start, monotonic))) = (&mut frame_metrics, draw_start) {
                let elapsed = start.elapsed().as_nanos();
                metrics.count += 1;
                metrics.sum_ns += elapsed;
                metrics.max_ns = metrics.max_ns.max(elapsed);
                let origin = *metrics.started.get_or_insert(start);
                metrics.monotonic_epoch_ns.get_or_insert(monotonic);
                if metrics.frame_samples.len() < 4096 {
                    metrics.frame_monotonic_ns.push(monotonic);
                    metrics
                        .frame_samples
                        .push((start.duration_since(origin).as_nanos(), elapsed));
                }
                metrics.sample_views(&state, &loop_state);
                metrics.writes = output_metrics.as_ref().expect("metrics enabled").get();
            }
            dirty = false;
            paint_at = Instant::now() + FRAME_BUDGET;
            if loop_state.reload_job.is_some() {
                loop_state.reload_painted = true;
            }
        }
        if *state.status() == TuiStatus::Quit {
            break;
        }
        // Drain bounded ready bursts on both lanes before considering sleep.
        // Count and elapsed-time limits give paints and provider events a turn.
        let burst_start = Instant::now();
        for _ in input_pending.len()..MAX_KEYS_PER_FRAME {
            // Register the actual UI task's waker even for a nonblocking
            // drain. now_or_never() would register a noop waker in Crossterm's
            // blocking reader and strand the following select at stable idle.
            let ready = std::future::poll_fn(|cx| {
                let item = std::pin::Pin::new(&mut input).poll_next(cx);
                std::task::Poll::Ready(match item {
                    std::task::Poll::Ready(item) => Some(item),
                    std::task::Poll::Pending => None,
                })
            })
            .await;
            let cev = match ready {
                Some(Some(Ok(event))) => event,
                Some(Some(Err(error))) => return Err(format!("input: {error}")),
                Some(None) => return Err("terminal input closed".into()),
                None => break,
            };
            if let Some(metrics) = &mut frame_metrics {
                metrics.input_events += 1;
            }
            let coalesce = matches!(&cev, CEvent::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown))
                && input_pending
                    .back()
                    .is_some_and(|(previous, _)| *previous == cev);
            if coalesce {
                input_pending.back_mut().expect("compatible wheel").1 += 1;
            } else {
                input_pending.push_back((cev, 1));
            }
            if burst_start.elapsed() >= BURST_BUDGET {
                break;
            }
        }
        let handling_start = Instant::now();
        for _ in 0..MAX_KEYS_PER_FRAME {
            if let Some((cev, ticks)) = input_pending.pop_front() {
                handle_event_ticks(app, &mut state, &mut loop_state, cev, ticks).await?;
                dirty = true;
                if *state.status() == TuiStatus::Quit || handling_start.elapsed() >= BURST_BUDGET {
                    break;
                }
            } else {
                break;
            }
        }
        // Quit stops input/event processing; the unique pending Home receipt
        // is reconciled below before run_inner shuts down the owner.
        if *state.status() == TuiStatus::Quit {
            break;
        }
        // Worker events, non-blocking drain. Observe occupancy on either side
        // of the existing drain; sampling does not consume an event.
        if let Some(metrics) = frame_metrics.as_mut() {
            metrics.worker_event_queue_peak = metrics.worker_event_queue_peak.max(rx.len());
        }
        let worker_start = Instant::now();
        for _ in 0..MAX_WORKER_EVENTS_PER_FRAME {
            let event = match worker_ready
                .take()
                .map(Ok)
                .unwrap_or_else(|| try_worker_event(&mut rx, frame_metrics.as_mut()))
            {
                Ok(event) => event,
                Err(_) => break,
            };
            dirty = true;
            if let Some(metrics) = &mut frame_metrics {
                metrics.worker_events += 1;
            }
            poll_and_sync(app, &mut state, &mut loop_state).await;
            if let CoreEvent::Compaction(snapshot) = event {
                apply_compaction_to_view(&mut state, &mut loop_state, snapshot);
            } else if let CoreEvent::McpChanged(snapshot) = event {
                apply_mcp_to_views(&mut state, &mut loop_state, snapshot);
            } else if matches!(event, CoreEvent::ProviderChanged) {
                refresh_provider_views(app, &mut state, &mut loop_state).await;
            } else if matches!(
                &event,
                CoreEvent::PermissionAsked(_)
                    | CoreEvent::PermissionResolved { .. }
                    | CoreEvent::QuestionAsked(_)
                    | CoreEvent::QuestionResolved { .. }
            ) {
                // Fresh direct-user Shell Ask exists before a root history does.
                // Control events must reach its receipt owner even on Home.
                refresh_approvals(app, &mut state).await?;
                loop_state.approvals_checked = None;
            } else if let Some(current) = state.attached_session().cloned() {
                handle_worker_event(app, &mut state, &mut loop_state, &current, event).await?;
            }
            loop_state.sync_tabs(&mut state);
            if let Some(metrics) = frame_metrics.as_mut() {
                metrics.worker_event_queue_peak = metrics.worker_event_queue_peak.max(rx.len());
            }
            if worker_start.elapsed() >= BURST_BUDGET {
                break;
            }
        }
        if let Some(metrics) = frame_metrics.as_mut() {
            metrics.worker_event_queue_peak = metrics.worker_event_queue_peak.max(rx.len());
        }
        // Arm the debounce for the final caret/edit after the key burst.
        dirty |= sync_mention(app, &mut state, &mut loop_state).await;
        // The DCP panel shows runtime counters: refresh when it opens.
        if *state.panel() == TuiPanel::Dcp && !loop_state.dcp_seen {
            if let Some(current) = state.attached_session().cloned() {
                refresh_dcp(app, &mut state, &current).await;
            }
            loop_state.dcp_seen = true;
        } else if *state.panel() != TuiPanel::Dcp {
            loop_state.dcp_seen = false;
        }
        if dirty && Instant::now() >= paint_at {
            continue;
        }
        let mut deadline = state.next_ui_deadline();
        if let Some(at) = loop_state.authentication.deadline() {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
        }
        if dirty {
            deadline = Some(deadline.map_or(paint_at, |at| at.min(paint_at)));
        }
        if state.has_pending_submission() || loop_state.has_jobs() {
            let check = Instant::now() + FRAME_BUDGET;
            deadline = Some(deadline.map_or(check, |at| at.min(check)));
        }
        if let Some((_, since)) = &loop_state.mention_pending {
            let at = *since + MENTION_DEBOUNCE;
            if at > Instant::now() {
                deadline = Some(deadline.map_or(at, |d| d.min(at)));
            }
        }
        if loop_state.recovery_job.is_none()
            && loop_state.conversation_job.is_none()
            && let Some((_, at)) = &loop_state.conversation_recovery
        {
            deadline = Some(deadline.map_or(*at, |d| d.min(*at)));
        }
        tokio::select! {
            event = input.next() => {
                let event = event.ok_or("terminal input closed")?.map_err(|e| format!("input: {e}"))?;
                if let Some(metrics) = &mut frame_metrics { metrics.input_events += 1; }
                input_pending.push_back((event, 1));
            }
            result = rx.recv() => {
                match result {
                    Ok(event) => worker_ready = Some(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        if let Some(metrics) = &mut frame_metrics {
                            metrics.worker_event_queue_lagged += skipped;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err("application event channel closed".into()),
                }
            }
            _ = async {
                match deadline {
                    Some(at) => tokio::time::sleep_until(at.into()).await,
                    None => std::future::pending::<()>().await,
                }
            } => {}
        }
        if let Some(metrics) = &mut frame_metrics {
            metrics.wakeups += 1;
            if metrics.wake_monotonic_ns.len() < 4096 {
                metrics.wake_monotonic_ns.push(monotonic_ns());
            }
        }
    }
    loop_state.authentication.shutdown(app).await?;
    if let Some((session, job)) = loop_state.title_job.take() {
        let _ = app.cancel_title(session).await;
        let _ = job.await;
    }
    if let Some((_, job)) = loop_state.mention_job.take() {
        job.abort();
    }
    settle_conversation(app, &mut state, &mut loop_state).await;
    if let Some(job) = loop_state.compaction_job.take() {
        let _ = job.await;
    }
    reconcile_exit(app, &mut state, &mut loop_state).await?;
    if let Some(metrics) = frame_metrics.as_mut() {
        metrics.sample_views(&state, &loop_state);
    }
    write_metrics(&state, &loop_state, frame_metrics.as_ref());
    drop(_term);
    // The terminal is restored promptly even when the owner is still rebuilding.
    // Do not abort the request: the owner's published transaction must finish
    // before run_stages queues shutdown, and a quit never claims reload success.
    if let Some(job) = loop_state.reload_job.take() {
        let _ = job.await;
    }
    Ok(0)
}

async fn settle_conversation(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    while deck
        .conversation_job
        .as_ref()
        .is_some_and(|job| !job.is_finished())
    {
        tokio::task::yield_now().await;
    }
    finish_conversation(app, state, deck).await;
}

/// A lagged receiver stops this frame's drain, as before; record the number
/// of overwritten events (not the time spent waiting) when metrics are on.
fn try_worker_event(
    rx: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
    metrics: Option<&mut FrameMetrics>,
) -> Result<CoreEvent, tokio::sync::broadcast::error::TryRecvError> {
    match rx.try_recv() {
        Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
            if let Some(metrics) = metrics {
                metrics.worker_event_queue_lagged =
                    metrics.worker_event_queue_lagged.saturating_add(skipped);
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped))
        }
        result => result,
    }
}

/// Never await the owner on a key. The quiet period avoids owner-side blocking
/// work for intermediate edits; aborting a JoinHandle alone cannot stop a
/// request that the owner has already started. Only deliver exact-key results.
async fn sync_mention(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut changed = false;
    let current = state.mention_request();
    if deck.mention_pending.as_ref().map(|(key, _)| key) != current.as_ref() {
        deck.mention_pending = current.clone().map(|key| (key, Instant::now()));
    }
    if deck
        .mention_job
        .as_ref()
        .is_some_and(|(key, _)| Some(key) != current.as_ref())
        && let Some((_, job)) = deck.mention_job.take()
    {
        job.abort();
    }
    if deck.mention_failed.as_ref() != current.as_ref() {
        deck.mention_failed = None;
    }
    if deck
        .mention_job
        .as_ref()
        .is_some_and(|(_, job)| job.is_finished())
    {
        let (key, job) = deck.mention_job.take().expect("finished mention job");
        match job.await {
            Ok(Ok(snapshot)) => {
                changed = state.apply_file_suggestions(key.clone(), snapshot);
                if !changed {
                    deck.mention_failed = Some(key);
                }
            }
            _ => deck.mention_failed = Some(key),
        }
    }
    if let Some(key) = current
        && !state.mention_loaded(&key)
        && deck.mention_failed.as_ref() != Some(&key)
        && deck.mention_job.is_none()
        && deck
            .mention_pending
            .as_ref()
            .is_some_and(|(pending, since)| pending == &key && since.elapsed() >= MENTION_DEBOUNCE)
    {
        let owner = app.clone();
        let query = key.query.clone();
        deck.mention_job = Some((
            key,
            tokio::spawn(async move { owner.file_suggestions(query, MENTION_LIMIT).await }),
        ));
    }
    changed
}

async fn poll_and_sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    state.poll_submission();
    // A key can poll the acceptance before this loop does. The deck, rather
    // than the view, records whether that accepted Home was already saved.
    let fresh = deck.active_tab.is_none() && state.attached_session().is_some();
    deck.sync_tabs(state);
    if fresh {
        deck.save(app, state).await;
        if let Err(error) = refresh_shells(app, state).await {
            state.push_note(&error);
        }
    }
}

async fn reconcile_exit(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
) -> Result<(), String> {
    state
        .reconcile_fresh_quit()
        .await
        .map_err(|error| format!("quit cancellation: {error}"))?;
    // `handle_key` can consume an acceptance just before setting Quit, after
    // the frame's final poll_and_sync. Only a newly attached Home needs a
    // save; an already-durable tab never waits or writes again.
    if deck.active_tab.is_none() && state.attached_session().is_some() {
        deck.sync_tabs(state);
        deck.save_checked(app, state)
            .await
            .map_err(|error| format!("quit tab deck: {error}"))?;
    }
    Ok(())
}

fn at_tty() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal()
}

async fn refresh_provider_view(app: &CoreApp, state: &mut TuiState) {
    let picker_provider = state.picker_provider_filter().map(str::to_owned);
    let catalog = if let Some(session) = state.attached_session() {
        app.session_selection(session.clone(), false, SelectionAction::Current)
            .await
    } else {
        app.home_selection(SelectionAction::Current).await
    };
    if let Ok(catalog) = catalog {
        state.apply_catalog(catalog);
        if let Some(provider) = picker_provider
            && let Ok(catalog) = app.provider_catalog(provider).await
        {
            state.apply_picker_catalog(catalog);
        }
    }
}

async fn refresh_provider_views(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    refresh_provider_view(app, state).await;
    for view in deck.tabs.iter_mut().flatten() {
        refresh_provider_view(app, view).await;
    }
    if let Some(home) = deck.home.as_mut() {
        refresh_provider_view(app, home).await;
    }
    if deck.reload_job.is_none()
        && let Ok(snapshot) = app.mcp_status().await
    {
        apply_mcp_to_views(state, deck, snapshot);
    }
}

fn startup_query(error: CoreError) -> StartupFailure {
    match error {
        CoreError::Diagnostic(diagnostic) | CoreError::ProviderUnavailable(diagnostic) => {
            StartupFailure::QueryDiagnostic(diagnostic)
        }
        CoreError::LocationSwitch {
            diagnostic: Some(diagnostic),
            ..
        } => StartupFailure::QueryDiagnostic(diagnostic),
        _ => StartupFailure::QueryDiagnostic(oc_core::queries::ServiceDiagnostic {
            kind: oc_core::queries::ServiceKind::Runtime,
            service: "worker".into(),
            source: "source-native/worker".into(),
            field: vec!["query".into()],
            stage: oc_core::queries::ServiceStage::Query,
            code: oc_core::queries::ServiceCode::QueryFailed,
            action: oc_core::queries::ServiceAction::RestartApplication,
        }),
    }
}

async fn initial_state(
    app: &CoreApp,
    session: Option<SessionId>,
) -> Result<TuiState, StartupFailure> {
    let Some(session) = session else {
        let snapshot = app
            .home_selection(SelectionAction::Current)
            .await
            .map_err(startup_query)?;
        let mut state = TuiState::new_home(app.clone());
        state.apply_catalog(snapshot);
        state.apply_mcp_snapshot(app.mcp_status().await.map_err(startup_query)?);
        return Ok(state);
    };
    app.create_session(session.clone())
        .await
        .map_err(startup_query)?;
    let page = app
        .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await
        .map_err(startup_query)?;
    let mut state = TuiState::new(app.clone(), session);
    state.attach_page(&page);
    // A failed catalog is an initialization error, never a usable empty snapshot.
    let snapshot = app
        .session_selection(state.session().clone(), false, SelectionAction::Current)
        .await
        .map_err(startup_query)?;
    state.apply_catalog(snapshot);
    state.apply_mcp_snapshot(app.mcp_status().await.map_err(startup_query)?);
    state.apply_compaction_history(
        app.compaction_history(state.session().clone())
            .await
            .map_err(startup_query)?,
    );
    refresh_dcp_summaries(app, &mut state).await;
    Ok(state)
}

// Match the owner's tab-deck ID predicate before creating a new explicit
// root. Existing legacy IDs are still readable, but cannot be saved as tabs.
fn valid_tab_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id == id.trim() && !id.chars().any(char::is_control)
}

async fn standalone_explicit(
    app: &CoreApp,
    deck: &mut LoopState,
    id: SessionId,
    child: bool,
) -> Result<TuiState, StartupFailure> {
    let mut state = load_tab(app, id).await.map_err(startup_query)?;
    deck.active_tab = Some(0);
    deck.tabs.push(None);
    deck.tab_cards_before.push(None);
    deck.save_disabled = true;
    deck.read_only = child;
    state.push_note(if child {
        "child session: read-only history; saved tabs are unchanged"
    } else {
        "legacy session id cannot be saved; review saved tabs"
    });
    Ok(state)
}

/// Restore all readable views in preference order. Once one is omitted the
/// resulting route is a projection, never a replacement for the stored list.
async fn restore_views(
    app: &CoreApp,
    deck: &mut LoopState,
    ids: Vec<(SessionId, bool)>,
    active: Option<SessionId>,
    explicit: Option<&SessionId>,
    home: Option<TuiState>,
    prefer_home_when_space: bool,
) -> Result<TuiState, StartupFailure> {
    // Parked view restoration carries large owned states across awaits. Heap
    // pin this bounded future instead of multiplying inline caller poll frames.
    Box::pin(async move {
    let mut views = Vec::with_capacity(ids.len());
    let mut unavailable = Vec::new();
    for (id, new_session_tab) in ids {
        match load_tab(app, id.clone()).await {
            Ok(mut view) => {
                view.new_session_tab = new_session_tab;
                views.push((id, view));
            }
            Err(error)
                if explicit == Some(&id)
                    || (matches!(&error, CoreError::Diagnostic(_) | CoreError::TabDeckStorage)
                        && !(active.as_ref() != Some(&id)
                            && matches!(&error, CoreError::Diagnostic(diagnostic)
                                if diagnostic.code == oc_core::queries::ServiceCode::InvalidStoredState
                                    && diagnostic.field == ["selection"]))) =>
            {
                return Err(startup_query(error));
            }
            Err(error) => {
                // A parked selection preference is optional view metadata.
                // Query/storage failures and the selected root still fail closed.
                if let CoreError::Diagnostic(diagnostic) = error {
                    unavailable.push(diagnostic);
                }
                deck.save_disabled = true;
            }
        }
    }
    let active_index = if prefer_home_when_space && views.len() < MAX_TABS {
        None
    } else {
        active
            .as_ref()
            .and_then(|id| views.iter().position(|(candidate, _)| candidate == id))
    };
    let mut state = if let Some(index) = active_index {
        deck.active_tab = Some(index);
        views.remove(index).1
    } else if views.len() == MAX_TABS {
        deck.active_tab = Some(0);
        views.remove(0).1
    } else if let Some(home) = home {
        home
    } else {
        initial_state(app, None).await?
    };
    deck.tabs = views.into_iter().map(|(_, view)| Some(view)).collect();
    if let Some(index) = deck.active_tab {
        deck.tabs.insert(index, None);
    }
    deck.tab_cards_before.resize(deck.tabs.len(), None);
    if deck.save_disabled {
        state.chrome.service_diagnostics.extend(unavailable);
        state.push_note("saved tabs partially unavailable; review saved tabs");
    }
    Ok(state)
    })
    .await
}

/// Load the complete Location-scoped route before the first frame. Parked
/// views use the same bounded history page and catalog path as an active view.
async fn restore_initial(
    app: &CoreApp,
    explicit: Option<SessionId>,
) -> Result<(TuiState, LoopState), StartupFailure> {
    let mut deck = LoopState::default();
    let stored = match app.tab_deck().await {
        Ok(snapshot) => snapshot,
        Err(error @ (CoreError::Diagnostic(_) | CoreError::TabDeckStorage)) => {
            return Err(startup_query(error));
        }
        Err(_) => {
            // An unreadable/malformed preference is never repaired on read.
            // Obtain the Location from the owner's catalog, but keep an empty
            // expected revision: CAS prevents overwriting the broken record.
            let mut state = if let Some(id) = explicit {
                let kind = app.probe_session(id.clone()).await.map_err(startup_query)?;
                if kind == SessionProbe::Absent {
                    if !valid_tab_id(&id.0) {
                        return Err(StartupFailure::Query);
                    }
                    app.create_session(id.clone())
                        .await
                        .map_err(startup_query)?;
                }
                standalone_explicit(app, &mut deck, id, kind == SessionProbe::Child).await?
            } else {
                initial_state(app, None).await?
            };
            deck.location = Some(
                state
                    .chrome
                    .location
                    .as_ref()
                    .filter(|location| !location.is_empty())
                    .ok_or(StartupFailure::Query)?
                    .clone(),
            );
            deck.save_disabled = true;
            if !deck.read_only {
                state.push_note("saved tab deck unavailable; review saved tabs");
            }
            return Ok((state, deck));
        }
    };
    if stored.location.is_empty()
        || stored.sessions.len() > MAX_TABS
        || (!stored.new_session_titles.is_empty()
            && stored.new_session_titles.len() != stored.sessions.len())
        || (stored.active.is_none() && stored.sessions.len() == MAX_TABS)
        || stored
            .active
            .as_ref()
            .is_some_and(|active| !stored.sessions.contains(active))
    {
        return Err(StartupFailure::Query);
    }
    deck.location = Some(stored.location);
    deck.revision = stored.revision;
    let mut ids = stored.sessions;
    let mut active = stored.active;
    let mut save_explicit = false;
    if let Some(id) = explicit.as_ref() {
        if !ids.contains(id) {
            let kind = app.probe_session(id.clone()).await.map_err(startup_query)?;
            if kind == SessionProbe::Child || (kind == SessionProbe::Root && !valid_tab_id(&id.0)) {
                let state =
                    standalone_explicit(app, &mut deck, id.clone(), kind == SessionProbe::Child)
                        .await?;
                return Ok((state, deck));
            }
            if ids.len() >= MAX_TABS {
                return Err(StartupFailure::Query);
            }
            if kind == SessionProbe::Absent {
                if !valid_tab_id(&id.0) {
                    return Err(StartupFailure::Query);
                }
                app.create_session(id.clone())
                    .await
                    .map_err(startup_query)?;
            }
            ids.push(id.clone());
        }
        save_explicit = active.as_ref() != Some(id);
        active = Some(id.clone());
    }
    // With no explicit route the original TUI starts on Home even when the
    // preference remembers a selected real tab. Keep that preference and all
    // parked views intact; only a user route change writes a new selection.
    // A full usable deck has no slot for synthetic Home, so retain its saved
    // active route. Decide after loading: an unreadable parked tab frees a
    // slot, but must never cause the hidden saved preference to be rewritten.
    let prefer_home_when_space = explicit.is_none();
    let mut state = restore_views(
        app,
        &mut deck,
        ids.into_iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    id,
                    stored
                        .new_session_titles
                        .get(index)
                        .copied()
                        .unwrap_or(false),
                )
            })
            .collect(),
        active,
        explicit.as_ref(),
        None,
        prefer_home_when_space,
    )
    .await?;
    if prefer_home_when_space && deck.tabs.len() == MAX_TABS {
        state.push_note("tab limit reached; Home unavailable until a tab is closed");
    }
    if deck.save_disabled && explicit.as_ref().is_some_and(|id| !valid_tab_id(&id.0)) {
        state.push_note("legacy session id cannot be saved; review saved tabs");
    }
    if save_explicit && !deck.save_disabled {
        deck.save(app, &mut state).await;
    }
    Ok((state, deck))
}

async fn load_tab(app: &CoreApp, id: SessionId) -> Result<TuiState, CoreError> {
    let page = app
        .history_page(id.clone(), None, None, HISTORY_PAGE_LIMIT)
        .await?;
    let catalog = app
        .session_selection(id.clone(), false, SelectionAction::Current)
        .await?;
    let mut state = TuiState::new(app.clone(), id);
    state.attach_page(&page);
    state.apply_compaction_history(app.compaction_history(state.session().clone()).await?);
    state.apply_catalog(catalog);
    state.apply_mcp_snapshot(app.mcp_status().await?);
    if let Ok(rows) = app.shell_jobs(state.session().clone()).await {
        state.apply_shell_jobs(rows);
    }
    refresh_dcp_summaries(app, &mut state).await;
    Ok(state)
}

fn startup_failure<W: Write>(
    terminal: &mut Terminal<FrameBackend<W>>,
    failure: StartupFailure,
) -> Result<ExitCode, String> {
    use crossterm::event::{KeyCode, KeyModifiers};
    loop {
        terminal
            .backend_mut()
            .begin_frame()
            .map_err(|e| format!("begin draw: {e}"))?;
        terminal
            .draw(|frame| render_startup_failure(frame, failure.clone()))
            .map_err(|e| format!("draw: {e}"))?;
        terminal
            .backend_mut()
            .finish_frame()
            .map_err(|e| format!("publish draw: {e}"))?;
        if event::poll(Duration::from_millis(100)).map_err(|e| format!("input: {e}"))?
            && let CEvent::Key(key) = event::read().map_err(|e| format!("input: {e}"))?
            && (matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
                || (key.code == KeyCode::Char('c')
                    && key.modifiers.contains(KeyModifiers::CONTROL)))
        {
            return Ok(ExitCode::from(1));
        }
    }
}

/// True when bare `oc` may launch the interactive TUI: both ends must be a
/// real terminal. `oc tui` keeps the stdin-only gate so an unusable stdout
/// still fails visibly at draw time instead of silently doing nothing.
pub fn interactive_ready() -> bool {
    use std::io::IsTerminal as _;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Handle one Crossterm event: keys drive the open panel or the prompt,
/// bracketed paste is one bounded input event, resize is picked up by the
/// next draw (which re-queries the size).
#[cfg(test)]
async fn handle_event(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    cev: CEvent,
) -> Result<(), String> {
    // Repeated event awaits must not embed a full controller future per event
    // in the test caller's stack frame as captured query metadata grows.
    Box::pin(handle_event_ticks(app, state, loop_state, cev, 1)).await
}

async fn handle_event_ticks(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    cev: CEvent,
    wheel_ticks: usize,
) -> Result<(), String> {
    if let CEvent::Resize(width, height) = cev {
        state.resize_mouse_position(ratatui::layout::Rect::new(0, 0, width, height));
        terminal_controls::resize(app, state, ratatui::layout::Rect::new(0, 0, width, height))
            .await;
        return Ok(());
    }
    let raw = match &cev {
        CEvent::Key(key) => state.raw_terminal_key(*key),
        CEvent::Paste(text) => state.raw_terminal_paste(text),
        _ => None,
    };
    if let Some(outcome) = raw {
        apply_outcome(app, state, loop_state, outcome, false).await;
        return Ok(());
    }
    let event = match cev {
        crossterm::event::Event::Key(key) => state.terminal_key(key).map(UiEvent::Key),
        other => map_event(other),
    };
    if loop_state.reload_job.is_some() || loop_state.conversation_job.is_some() {
        match event {
            Some(UiEvent::Key(KeyAction::Interrupt | KeyAction::Quit)) => {
                // Bypass modal/editor focus, but leave owner reload intact for
                // the post-terminal join and orderly application shutdown.
                state.handle_key(KeyAction::Quit).await;
            }
            Some(UiEvent::Key(KeyAction::Enter)) if state.input().trim() == "/quit" => {
                state.handle_key(KeyAction::Quit).await;
            }
            Some(UiEvent::Resize) => {}
            _ => {}
        }
        return Ok(());
    }
    match event {
        Some(UiEvent::Key(action)) => {
            let submits = action == KeyAction::Enter
                || matches!(&action, KeyAction::SequenceKey(key, _) if key == "enter");
            if action == KeyAction::Cancel
                && *state.panel() == TuiPanel::None
                && state.input().trim() == "/rename"
                && let Some((session, _)) = &loop_state.title_job
                && state.attached_session() == Some(session)
            {
                let _ = app.cancel_title(session.clone()).await;
                return Ok(());
            }
            if loop_state.read_only
                && state.approvals.active().is_none()
                && state.questions.active().is_none()
                && *state.panel() == TuiPanel::None
                && submits
                && !state.children_open()
                && !state.shells_open()
                && !state.terminals_open()
                && dispatch(state.input().trim())
                    .is_none_or(|a| a != CommandAction::Quit && !a.is_terminal())
            {
                state.push_note("child session: read-only history; saved tabs are unchanged");
                return Ok(());
            }
            let typed_new = submits
                && *state.panel() == TuiPanel::None
                && dispatch(state.input().trim()) == Some(CommandAction::NewSession);
            // The view owns the global pending lifecycle before choosing the
            // focused editor/modal consumer, including pending Enter replay.
            let outcome = state.handle_key(action).await;
            report_copy_request(state, loop_state);
            apply_outcome(app, state, loop_state, outcome, typed_new).await;
        }
        Some(UiEvent::Paste(text)) => {
            let outcome = state.handle_paste(&text);
            apply_outcome(app, state, loop_state, outcome, false).await;
        }
        Some(UiEvent::Mouse(mouse)) => {
            let (cols, rows) =
                crossterm::terminal::size().map_err(|e| format!("mouse terminal size: {e}"))?;
            let area = ratatui::layout::Rect::new(0, 0, cols, rows);
            if *state.panel() == TuiPanel::None
                && state.approvals.active().is_none()
                && matches!(
                    mouse.kind,
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
                )
                && !state.tab_wheel_hit(area, mouse.column, mouse.row)
                && !oc_tui::terminal_view::pane_hit(state, area, mouse.column, mouse.row)
            {
                // A wheel event also moves the pointer. Keep transcript
                // scrolling, but invalidate a held tab when it leaves the strip.
                state.handle_mouse(mouse, area);
                report_copy_request(state, loop_state);
                let outcome = state.wheel_transcript_at(
                    mouse.kind == MouseEventKind::ScrollUp,
                    wheel_ticks,
                    Instant::now(),
                );
                apply_outcome(app, state, loop_state, outcome, false).await;
            } else {
                for _ in 0..wheel_ticks {
                    let outcome = state.handle_mouse(mouse, area);
                    report_copy_request(state, loop_state);
                    apply_mouse_outcome(
                        app,
                        state,
                        loop_state,
                        outcome,
                        area,
                        mouse.column,
                        mouse.row,
                    )
                    .await;
                }
            }
        }
        Some(UiEvent::Resize) => {}
        None => report_copy_request(state, loop_state),
    }
    Ok(())
}

fn report_copy_request(state: &mut TuiState, _loop_state: &LoopState) {
    // Apply to the view that produced the request before a key/mouse outcome
    // can swap the active tab. Never log the selected text.
    #[cfg(test)]
    if let Some(copy) = _loop_state.copy_transport {
        report_copy_request_with(state, copy);
        return;
    }
    report_copy_request_with(state, crate::clipboard::copy);
}

fn report_copy_request_with(state: &mut TuiState, copy: impl FnOnce(&str) -> Result<(), String>) {
    if let Some(text) = state.take_copy_request() {
        state.report_clipboard_result(copy(&text));
    }
}

/// Carry a validated real pointer through admitted activation, then reconcile
/// its stable tab identity with the replacement view's actual layout.
async fn apply_mouse_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    outcome: KeyOutcome,
    area: ratatui::layout::Rect,
    x: u16,
    y: u16,
) {
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else { return };
    let pointer = (state.mouse_position() == Some((x, y, area))).then_some((x, y, area));
    let close = match intent {
        PanelIntent::CloseTab { index } if pointer.is_some() => state.mouse_close_snapshot(index),
        _ => None,
    };
    let activate = matches!(
        intent,
        PanelIntent::ActivateTab { .. } | PanelIntent::NewSession
    );
    match apply_intent_with_origin(app, state, loop_state, intent, false, pointer.is_some()).await {
        Ok(()) => {
            loop_state.sync_tabs(state);
            if let Some(snapshot) = close {
                state.restore_mouse_close(snapshot);
            } else if activate && let Some(pointer) = pointer {
                state.restore_mouse_hover(pointer);
            }
        }
        Err(message) => {
            state.apply_intent_error(message);
            loop_state.sync_tabs(state);
        }
    }
}

/// Report a note, then apply the intent (or its typed failure).
async fn apply_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    outcome: KeyOutcome,
    typed_new: bool,
) {
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else {
        return;
    };
    let keyboard_close_pointer = (matches!(intent, PanelIntent::CloseTab { .. })
        && *state.panel() == TuiPanel::None)
        .then(|| state.mouse_position())
        .flatten();
    // Scrolling intents never consume typed input; commands do.
    let consumes = matches!(intent, PanelIntent::SwitchLocation { .. });
    // Keep the controller's large intent future out of each nested input poll
    // frame; do not depend on an enlarged executor/test thread stack.
    match Box::pin(apply_intent_with_origin(
        app, state, loop_state, intent, typed_new, false,
    ))
    .await
    {
        Ok(()) => {
            if consumes {
                state.accept_intent();
            }
            if let Some(pointer) = keyboard_close_pointer {
                state.restore_tab_hover_at(pointer);
            }
        }
        Err(message) => state.apply_intent_error(message),
    }
    loop_state.sync_tabs(state);
}

#[cfg(test)]
async fn apply_intent(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    intent: PanelIntent,
) -> Result<(), String> {
    apply_intent_with_origin(app, state, loop_state, intent, false, false).await
}

async fn apply_intent_with_origin(
    app: &CoreApp,
    state: &mut TuiState,
    loop_state: &mut LoopState,
    intent: PanelIntent,
    typed_new: bool,
    preserve_pointer: bool,
) -> Result<(), String> {
    let terminal_control = matches!(
        intent,
        PanelIntent::Terminal {
            action: oc_tui::terminal_view::TerminalIntent::Control(_),
            ..
        }
    );
    let auth_control = matches!(
        &intent,
        PanelIntent::BeginAuthentication(_)
            | PanelIntent::LoadAuthMethods { .. }
            | PanelIntent::OpenAuthorization
            | PanelIntent::CopyAuthorization
            | PanelIntent::LoadProviderConnections
            | PanelIntent::ProviderAccounts { action: None, .. }
    );
    if loop_state.conversation_job.is_some() && !terminal_control && !auth_control {
        return Err("conversation operation pending".into());
    }
    if loop_state.reload_job.is_some() && !terminal_control && !auth_control {
        return Err("configuration reload pending".into());
    }
    if loop_state.read_only
        && matches!(
            intent,
            PanelIntent::NewSession
                | PanelIntent::SwitchLocation { .. }
                | PanelIntent::SetPermissionMode { .. }
                | PanelIntent::SwitchSession { .. }
                | PanelIntent::CloseTab { .. }
                | PanelIntent::ActivateTab { .. }
                | PanelIntent::SelectModel { .. }
                | PanelIntent::ChooseModel { .. }
                | PanelIntent::CycleVariant
                | PanelIntent::SelectAgent { .. }
                | PanelIntent::Compress { .. }
                | PanelIntent::CompactSession
                | PanelIntent::ReloadConfiguration
                | PanelIntent::ChangeConversation { .. }
                | PanelIntent::ForkMessage { .. }
        )
    {
        return Err("child session: read-only history; saved tabs are unchanged".into());
    }
    match intent {
        PanelIntent::LoadAuthMethods { provider, revision } => {
            let methods = app.auth_methods(provider.clone()).await.map_err(|_| ());
            state.apply_auth_methods(&provider, revision, methods);
        }
        PanelIntent::BeginAuthentication(request) => {
            if state.auth_request() == Some(&request) {
                loop_state.authentication.sync(app, state).await;
            }
        }
        PanelIntent::OpenAuthorization => loop_state.authentication.open(state),
        PanelIntent::CopyAuthorization => {
            if let Some(detail) = state.auth_detail(true) {
                let result = crate::clipboard::copy(detail);
                state.push_transient_note(
                    if result.is_ok() {
                        "Clipboard request sent"
                    } else {
                        "Clipboard unavailable"
                    },
                    NoteVariant::Info,
                );
            }
        }
        PanelIntent::Terminal {
            session,
            action,
            close_composer,
        } => {
            // Source callback closes the lower composer BEFORE dispatch. A child
            // route returns to its parent, but never changes the captured actor.
            if close_composer {
                state.close_terminal_composer();
                if state.linked_child().is_some() {
                    child_controls::return_parent(state, loop_state);
                }
            }
            terminal_controls::apply(app, state, loop_state, session, action).await;
        }
        PanelIntent::LoadProviderConnections => {
            state.apply_provider_connections(app.provider_connections().await.map_err(|_| ()));
        }
        PanelIntent::ProviderAccounts { provider, action } => {
            let result = app.provider_accounts(provider.clone(), action).await;
            if state.apply_provider_accounts(result.map_err(|_| ())) {
                // Storage ACK precedes the picker; no model is committed here.
                let snapshot = app
                    .provider_catalog(provider)
                    .await
                    .map_err(|e| e.to_string())?;
                state.apply_picker_catalog(snapshot);
            }
            if let Some((provider, revision)) = state.account_methods_request() {
                let methods = app.auth_methods(provider.clone()).await.map_err(|_| ());
                state.apply_auth_methods(&provider, revision, methods);
            }
        }
        PanelIntent::LoadChildren => child_controls::refresh(app, state).await?,
        PanelIntent::OpenChild { selected } => {
            child_controls::open(app, state, loop_state, selected).await?
        }
        PanelIntent::ReturnParent => child_controls::return_parent(state, loop_state),
        PanelIntent::BackgroundSession {
            session,
            turn,
            work,
        } => {
            // The view may have switched before an externally queued intent runs.
            // Never rebuild targets from its new route or selected list row.
            if state.attached_session() != Some(&session) || state.active_turn() != Some(&turn) {
                return Ok(());
            }
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
                        app.background_child(job.parent.clone(), *job).await
                    }
                    oc_tui::app::ForegroundWork::Shell(job)
                        if job.session == session && job.turn == turn.0 && !job.background =>
                    {
                        app.background_shell(job.session, job.shell_id).await
                    }
                    _ => continue,
                };
                if let Err(error) = result {
                    state.push_note(&error.to_string());
                }
            }
            child_controls::refresh(app, state).await?;
            if let Ok(rows) = app.shell_jobs(session).await {
                state.apply_shell_jobs(rows);
            }
        }
        PanelIntent::BackgroundChild { selected } => {
            if let Err(error) = app
                .background_child(selected.parent.clone(), selected)
                .await
            {
                state.push_note(&error.to_string());
            }
            child_controls::refresh(app, state).await?;
        }
        PanelIntent::InterruptChild { selected } => {
            if let Err(error) = app.interrupt_child(selected.parent.clone(), selected).await {
                state.push_note(&error.to_string());
            }
            child_controls::refresh(app, state).await?;
        }
        PanelIntent::SetPermissionMode { auto_once } => {
            app.set_permission_mode(auto_once)
                .await
                .map_err(|e| e.to_string())?;
            state.chrome.permissions_auto = auto_once;
            let effective = auto_once || loop_state.cli_auto;
            if loop_state.cli_auto {
                app.register_approval_consumer(true)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            loop_state.permission_auto = Some(effective);
            state.auto_accept = if effective {
                oc_core::queries::AutoAcceptState::Enabled
            } else {
                oc_core::queries::AutoAcceptState::Disabled
            };
            for parked in loop_state.tabs.iter_mut().flatten() {
                parked.auto_accept = state.auto_accept;
                parked.chrome.permissions_auto = auto_once;
            }
            if let Some(home) = loop_state.home.as_mut() {
                home.auto_accept = state.auto_accept;
                home.chrome.permissions_auto = auto_once;
            }
            state.permission_mode_applied();
            refresh_approvals(app, state).await?;
        }
        PanelIntent::ReplyApproval(reply) => match app.reply_approval(reply).await {
            Ok(()) => refresh_approvals(app, state).await?,
            Err(error) => {
                state.approvals.error = Some(error.to_string());
                return Err(error.to_string());
            }
        },
        PanelIntent::ReplyQuestion(reply) => {
            let result = app.reply_question(reply.clone()).await;
            state
                .questions
                .reply_result(&reply, result.as_ref().err().map(ToString::to_string));
            if result.is_ok() {
                refresh_approvals(app, state).await?;
            }
        }
        PanelIntent::CompactSession => {
            if loop_state.compaction_job.is_some() {
                return Err("compaction admission pending".into());
            }
            let session = state.attached_session().cloned().ok_or("no session yet")?;
            let owner = app.clone();
            let revision = state.compaction_request_revision();
            loop_state.compaction_job = Some(tokio::spawn(async move {
                owner
                    .compact_session(session)
                    .await
                    .map(|snapshot| (snapshot, revision))
            }));
        }
        PanelIntent::ChangeConversation { action } => {
            if loop_state.conversation_recovery.is_some() || loop_state.recovery_job.is_some() {
                return Err("conversation refresh pending".into());
            }
            let session = require_session(state)?;
            if *state.status() == TuiStatus::PendingSubmission {
                return Err("submission pending".into());
            }
            let owner = app.clone();
            loop_state.conversation_job = Some(tokio::spawn(async move {
                let snapshot = owner
                    .change_conversation(session.clone(), action)
                    .await
                    .map_err(|e| e.to_string())?;
                let refresh = conversation_refresh(owner, session);
                Ok(ConversationResult::Changed(snapshot, refresh))
            }));
        }
        PanelIntent::ForkMessage { message } => {
            if loop_state.conversation_recovery.is_some() || loop_state.recovery_job.is_some() {
                return Err("conversation refresh pending".into());
            }
            if state.is_busy() {
                return Err("turn active; fork unavailable".into());
            }
            if !loop_state.can_open_session() {
                return Err("tab limit reached".into());
            }
            let session = require_session(state)?;
            let owner = app.clone();
            loop_state.conversation_job = Some(tokio::spawn(async move {
                let snapshot = owner
                    .fork_session(session, message)
                    .await
                    .map_err(|e| e.to_string())?;
                let refresh = conversation_refresh(owner, snapshot.session.clone());
                Ok(ConversationResult::Forked(snapshot, refresh))
            }));
        }
        PanelIntent::CopyMessage { message, seq } => {
            let session = require_session(state)?;
            // Fetch the owner row again: HistoryWindow/rendering can shorten text.
            let page = app
                .history_page(session, seq.checked_add(1), None, 1)
                .await
                .map_err(|e| e.to_string())?;
            let row = page
                .rows
                .into_iter()
                .find(|row| row.id == message && row.role == oc_core::session::Role::User)
                .ok_or("message is no longer active")?;
            state.copy_message_text(row.text)?;
            report_copy_request(state, loop_state);
        }
        PanelIntent::ReloadConfiguration => {
            if state.is_busy()
                || loop_state.tabs.iter().flatten().any(TuiState::is_busy)
                || loop_state.home.as_ref().is_some_and(TuiState::is_busy)
                || loop_state.title_job.is_some()
            {
                return Err("turn active; configuration reload refused".into());
            }
            let slash_draft = (state.input().trim() == "/reload").then(|| state.input().to_owned());
            state.push_transient_note_for(
                "Reloading configuration…",
                NoteVariant::Info,
                std::time::Duration::from_secs(30),
            );
            loop_state.reload_draft = slash_draft;
            let owner = app.clone();
            loop_state.reload_job =
                Some(tokio::spawn(async move { owner.reload_location().await }));
            loop_state.reload_painted = false;
        }
        PanelIntent::LoadCatalog => {
            let snapshot = selection(app, state, SelectionAction::Current).await?;
            state.apply_catalog(snapshot);
        }
        PanelIntent::LoadMcps => {
            state.apply_mcp_snapshot(app.mcp_status().await.map_err(|error| error.to_string())?);
        }
        PanelIntent::McpControl(control) => {
            state.apply_mcp_snapshot(
                app.mcp_control(control)
                    .await
                    .map_err(|error| error.to_string())?,
            );
        }
        PanelIntent::LoadSessions => {
            let context = app
                .session_picker_context(state.session_scope_update())
                .await
                .map_err(|e| e.to_string())?;
            state.apply_session_picker_context(context);
            let entries = app
                .session_list(state.session_search(), state.sessions_all_projects())
                .await
                .map_err(|e| e.to_string())?;
            state.apply_session_entries(entries);
        }
        PanelIntent::RenameSelectedSession { id, title } => {
            let target = SessionId::new(id.clone()).ok_or("bad session id")?;
            let result = if loop_state.read_only || state.is_busy() {
                Err(CoreError::TurnBusy)
            } else {
                app.picker_session_action(
                    target,
                    state.session_search(),
                    state.sessions_all_projects(),
                    oc_core::queries::SessionPickerAction::Rename(title.clone()),
                )
                .await
                .map(|_| ())
            };
            match result {
                Ok(()) => {
                    state.selected_session_renamed(&id, title.clone());
                    for parked in loop_state.tabs.iter_mut().flatten() {
                        parked.selected_session_renamed(&id, title.clone());
                    }
                    loop_state.sync_tabs(state);
                }
                Err(error) => state.rename_session_rejected(error.to_string()),
            }
        }
        PanelIntent::DeleteSelectedSession { id } => {
            if loop_state.read_only || state.is_busy() || loop_state.conversation_job.is_some() {
                state.session_delete_rejected(
                    "turn active or read-only history; delete refused".into(),
                );
                return Ok(());
            }
            let target = SessionId::new(id.clone()).ok_or("bad session id")?;
            let index = loop_state
                .tabs
                .iter()
                .enumerate()
                .find_map(|(index, parked)| {
                    let view = if loop_state.active_tab == Some(index) {
                        &*state
                    } else {
                        parked.as_ref()?
                    };
                    (view.attached_session() == Some(&target)).then_some(index)
                });
            // Prepare the replacement before destructive acceptance. The
            // current view, draft and tab membership survive all refusals.
            let home = if index == loop_state.active_tab
                && index.is_some()
                && loop_state.tabs.len() == 1
                && loop_state.home.is_none()
            {
                let catalog = app
                    .home_selection(SelectionAction::Current)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut home = TuiState::new_home(app.clone());
                home.apply_catalog(catalog);
                Some(home)
            } else {
                None
            };
            if index.is_some() {
                loop_state
                    .save_checked(app, state)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            match app
                .picker_session_action(
                    target.clone(),
                    state.session_search(),
                    state.sessions_all_projects(),
                    oc_core::queries::SessionPickerAction::Delete,
                )
                .await
            {
                Err(error) => state.session_delete_rejected(error.to_string()),
                Ok(oc_core::queries::SessionPickerResult::Renamed) => unreachable!("delete result"),
                Ok(oc_core::queries::SessionPickerResult::Deleted(snapshot)) => {
                    if loop_state.location.as_deref() == Some(snapshot.location.as_str())
                        || index.is_some()
                    {
                        loop_state.revision = snapshot.revision;
                    }
                    if loop_state
                        .conversation_recovery
                        .as_ref()
                        .is_some_and(|(owner, _)| owner == &target)
                    {
                        loop_state.conversation_recovery = None;
                        if let Some(job) = loop_state.recovery_job.take() {
                            job.abort();
                        }
                    }
                    if loop_state
                        .title_job
                        .as_ref()
                        .is_some_and(|(owner, _)| owner == &target)
                        && let Some((_, job)) = loop_state.title_job.take()
                    {
                        job.abort();
                    }
                    if let Some(index) = index {
                        if loop_state.active_tab == Some(index) {
                            if loop_state.tabs.len() == 1 {
                                if !loop_state.restore_home(state) {
                                    *state = home.expect("replacement prepared before deletion");
                                }
                                loop_state.reset_deck();
                            } else {
                                loop_state
                                    .activate(state, if index > 0 { index - 1 } else { 1 })?;
                                loop_state.tabs.remove(index);
                                loop_state.tab_cards_before.remove(index);
                                if let Some(active) = &mut loop_state.active_tab
                                    && *active > index
                                {
                                    *active -= 1;
                                }
                            }
                        } else {
                            loop_state.tabs.remove(index);
                            loop_state.tab_cards_before.remove(index);
                            if let Some(active) = &mut loop_state.active_tab
                                && *active > index
                            {
                                *active -= 1;
                            }
                        }
                        loop_state.sync_tabs(state);
                    }
                    // An unrelated deletion keeps modal search and prompt draft.
                    let entries = app
                        .session_list(state.session_search(), state.sessions_all_projects())
                        .await
                        .map_err(|e| e.to_string())?;
                    state.apply_session_entries(entries);
                }
            }
        }
        PanelIntent::LoadSkills => {
            let cards = app.skills().await.map_err(|e| e.to_string())?;
            state.apply_skills(cards);
        }
        PanelIntent::LoadCards => {
            let session = require_session(state)?;
            let page = app
                .tool_ops_page(session, loop_state.cards_before, TOOL_OPS_PAGE_LIMIT)
                .await
                .map_err(|e| e.to_string())?;
            let cards = oc_tui::history::cards_from_rows(&page.rows);
            if loop_state.cards_before.is_none() {
                state.apply_cards(cards, page.has_older);
            } else {
                state.prepend_cards(cards, page.has_older);
            }
            loop_state.cards_before = page.rows.last().map(|row| row.rowid);
        }
        PanelIntent::LoadShells => refresh_shells(app, state).await?,
        PanelIntent::CancelShell { session, shell_id } => {
            match app.cancel_shell(session, shell_id).await {
                Ok(()) | Err(CoreError::TurnNotActive) => {}
                Err(error) => state.push_note(&error.to_string()),
            }
            refresh_shells(app, state).await?;
        }
        PanelIntent::BackgroundShell { session, shell_id } => {
            match app.background_shell(session, shell_id).await {
                Ok(()) | Err(CoreError::TurnNotActive) => {}
                Err(error) => state.push_note(&error.to_string()),
            }
            refresh_shells(app, state).await?;
        }
        PanelIntent::LoadCardOutput { op, offset } => {
            let session = require_session(state)?;
            let page = match app
                .shell_output(session.clone(), op.clone(), offset, 240)
                .await
                .map_err(|error| error.to_string())?
            {
                Some(page) => page,
                None => app
                    .tool_output_page(session, op.clone(), offset, 240)
                    .await
                    .map_err(|error| error.to_string())?,
            };
            state.apply_card_output(op, offset, page);
        }
        PanelIntent::SelectModel { id } => {
            state.draft_model(&id)?;
        }
        PanelIntent::ChooseModel { variant, .. } => {
            state.draft_variant(variant.as_deref())?;
        }
        PanelIntent::CycleVariant => {
            state.cycle_variant_draft()?;
        }
        PanelIntent::NewSession => {
            if state.is_busy() && state.approvals.active().is_none() {
                return Err("turn active; action unavailable".into());
            }
            let before = loop_state.snapshot(state);
            if loop_state.home.is_some() {
                if typed_new {
                    state.accept_intent();
                }
                loop_state.restore_home(state);
                loop_state.save_if_changed(app, state, &before).await;
                return Ok(());
            }
            if loop_state.tabs.len() >= MAX_TABS && state.attached_session().is_some() {
                return Err("tab limit reached".into());
            }
            let snapshot = app
                .home_selection(SelectionAction::New(
                    state.active_agent().map(str::to_string),
                ))
                .await
                .map_err(|e| e.to_string())?;
            let mut home = TuiState::new_home(app.clone());
            home.apply_catalog(snapshot);
            if typed_new {
                state.accept_intent();
            }
            loop_state.open_home(state, home);
            loop_state.save_if_changed(app, state, &before).await;
        }
        PanelIntent::ActivateTab { index } => {
            let before = loop_state.snapshot(state);
            if preserve_pointer {
                loop_state.activate_with_hover(state, index, true)?;
            } else {
                loop_state.activate(state, index)?;
            }
            refresh_dcp_summaries(app, state).await;
            // Parked prompt/editor state survives routing, but pending ownership
            // must be recovered before its first visible frame after activation.
            if (!state.approval_roots.is_empty() || state.approvals.active().is_some())
                && let Err(error) = refresh_approvals(app, state).await
            {
                state.approvals.error = Some(error);
            }
            if let Some(session) = state.attached_session().cloned()
                && loop_state.picker_pending_tabs.remove(&session)
            {
                if let Some(job) = loop_state.recovery_job.take() {
                    job.abort();
                }
                loop_state.conversation_recovery = Some((session, std::time::Instant::now()));
            }
            loop_state.save_if_changed(app, state, &before).await;
        }
        PanelIntent::CloseTab { index } => {
            let closed = if loop_state.active_tab == Some(index) {
                state.attached_session().cloned()
            } else {
                loop_state
                    .tabs
                    .get(index)
                    .and_then(Option::as_ref)
                    .and_then(TuiState::attached_session)
                    .cloned()
            };
            loop_state.close_tab(app, state, index).await?;
            refresh_dcp_summaries(app, state).await;
            if let Some((owner, _)) = &loop_state.title_job
                && closed.as_ref() == Some(owner)
            {
                let _ = app.cancel_title(owner.clone()).await;
            }
        }
        intent @ (PanelIntent::RenameSession { .. } | PanelIntent::RenameSessionDirect { .. }) => {
            let direct = matches!(intent, PanelIntent::RenameSessionDirect { .. });
            let title = match intent {
                PanelIntent::RenameSession { title }
                | PanelIntent::RenameSessionDirect { title } => title,
                _ => unreachable!(),
            };
            // A selected root in the current deck is the only writable route.
            // The owner validates its Location and persists the title before
            // the dialog or tab presentation is changed.
            let result = async {
                if loop_state.read_only {
                    return Err("child session: read-only history; rename refused");
                }
                let session = state
                    .attached_session()
                    .filter(|_| {
                        !state.home
                            && loop_state.active_tab.is_some_and(|index| {
                                index < loop_state.tabs.len() && loop_state.tabs[index].is_none()
                            })
                    })
                    .cloned()
                    .ok_or("no active session; rename refused")?;
                if state.is_busy() {
                    return Err("turn active; rename refused");
                }
                app.rename_session(session, title.clone())
                    .await
                    .map_err(|error| match error {
                        CoreError::Application(ref reason) if reason == "invalid session title" => {
                            "invalid session title (max 256 bytes; no invisible or control text)"
                        }
                        CoreError::SessionNotFound => "session unavailable for rename",
                        _ => "session rename unavailable; check the data directory",
                    })
            }
            .await;
            match result {
                Ok(()) => {
                    if direct {
                        state.rename_session_direct_applied(title);
                    } else {
                        state.rename_session_applied(title);
                    }
                    loop_state.sync_tabs(state);
                }
                Err(message) => {
                    if direct {
                        state.rename_session_direct_rejected(message.into());
                    } else {
                        state.rename_session_rejected(message.into());
                    }
                }
            }
        }
        PanelIntent::RegenerateTitle => {
            let session = state
                .attached_session()
                .filter(|_| {
                    !state.home
                        && !loop_state.read_only
                        && loop_state.active_tab.is_some_and(|index| {
                            index < loop_state.tabs.len() && loop_state.tabs[index].is_none()
                        })
                })
                .cloned();
            match session {
                Some(session) if !state.is_busy() && loop_state.title_job.is_none() => {
                    let owner = app.clone();
                    let target = session.clone();
                    loop_state.title_job = Some((
                        session,
                        tokio::spawn(async move { owner.regenerate_title(target).await }),
                    ));
                }
                _ => state
                    .regenerated_title(Err("title generation unavailable for this session".into())),
            }
        }
        PanelIntent::SelectAgent { id } => {
            let snapshot = selection(app, state, SelectionAction::Agent(id)).await?;
            state.apply_catalog(snapshot);
            state.close_panel();
        }
        PanelIntent::SwitchSession { id } => {
            // The worker is single-turn: refuse the switch while a turn runs
            // instead of silently losing the active task.
            if state.is_busy() || loop_state.conversation_job.is_some() {
                return Err("turn active; session switch refused".to_string());
            }
            let target = SessionId::new(id).ok_or_else(|| "bad session id".to_string())?;
            let index = loop_state
                .tabs
                .iter()
                .enumerate()
                .find_map(|(index, parked)| {
                    let view = if loop_state.active_tab == Some(index) {
                        &*state
                    } else {
                        parked.as_ref().expect("parked tab")
                    };
                    (view.attached_session() == Some(&target)).then_some(index)
                });
            if index.is_none()
                && !state
                    .session_entries()
                    .iter()
                    .find(|entry| entry.id == target)
                    .is_some_and(|entry| {
                        entry.directory.as_deref() != loop_state.location.as_deref()
                    })
                && !loop_state.can_open_session()
            {
                return Err("tab limit reached".into());
            }
            let receipt = app
                .open_picker_session(
                    target.clone(),
                    state.session_search(),
                    state.sessions_all_projects(),
                    loop_state.snapshot(state),
                )
                .await
                .map_err(|e| e.to_string())?;
            if loop_state.location.as_deref().unwrap_or_default() != receipt.location {
                adopt_picker_open(app, state, loop_state, receipt).await;
                return Ok(());
            }
            loop_state.revision = receipt.deck.revision;
            let pending_refresh = loop_state.picker_pending_tabs.remove(&target)
                || loop_state
                    .conversation_recovery
                    .as_ref()
                    .is_some_and(|(session, _)| session == &target);
            if let Some(index) = index {
                loop_state.activate(state, index)?;
                // A healthy retained tab keeps its loaded older window and
                // scroll state. A changed branch or incomplete view gets the
                // already validated owner page instead.
                let changed = state.history().total() != receipt.page.total
                    || (!state.history().has_newer()
                        && state.history().rows().last().map(|row| row.seq)
                            != receipt.page.rows.last().map(|row| row.seq));
                if pending_refresh || changed {
                    state.attach_page(&receipt.page);
                    if let Some(job) = loop_state.recovery_job.take() {
                        job.abort();
                    }
                    loop_state.conversation_recovery = None;
                } else if let Some(title) = receipt.page.title {
                    state.selected_session_renamed(&target.0, title);
                }
                state.apply_catalog(receipt.catalog);
                refresh_compactions(app, state).await;
                loop_state.sync_tabs(state);
                return Ok(());
            }
            let mut next = TuiState::new(app.clone(), target);
            next.attach_page(&receipt.page);
            next.apply_catalog(receipt.catalog);
            refresh_compactions(app, &mut next).await;
            state.close_panel();
            next.take_tab_clocks_from(state);
            if let Some(old) = loop_state.active_tab.take() {
                loop_state.tabs[old] = Some(std::mem::replace(state, next));
                loop_state.tab_cards_before[old] = loop_state.cards_before;
            } else {
                loop_state.home = Some(std::mem::replace(state, next));
            }
            loop_state.active_tab = Some(loop_state.tabs.len());
            loop_state.tabs.push(None);
            loop_state.tab_cards_before.push(None);
            loop_state.cards_before = None;
            loop_state.dcp_seen = false;
            loop_state.sync_tabs(state);
        }
        PanelIntent::SwitchLocation { path } => {
            // The application refuses a switch during a turn; the view-model
            // keeps the current Location and session until it is published.
            if state.is_busy() {
                return Err("turn active; location switch refused".to_string());
            }
            // Publish a sessionless generation first: even when switching
            // from a real tab, a saved Home route cannot mint a new root.
            let snapshot = app.switch_location_home(path).await.map_err(switch_error)?;
            adopt_location(app, state, loop_state, snapshot.catalog, &snapshot.location).await;
        }
        PanelIntent::LoadOlder => {
            let session = require_session(state)?;
            let before = state
                .history()
                .rows()
                .iter()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(before) = before {
                let page = app
                    .history_page(session, Some(before), None, HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|e| e.to_string())?;
                state.prepend_page(&page);
                refresh_dcp_summaries(app, state).await;
            }
        }
        PanelIntent::LoadNewer => {
            let session = require_session(state)?;
            let after = state
                .history()
                .rows()
                .iter()
                .rev()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(after) = after {
                let page = app
                    .history_page(session, None, Some(after), HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|e| e.to_string())?;
                state.append_page(&page);
                refresh_dcp_summaries(app, state).await;
            }
        }
        PanelIntent::Compress { focus } => {
            require_session(state)?;
            state.request_compress(focus).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn switch_error(error: CoreError) -> String {
    match error {
        CoreError::LocationSwitch { category, diagnostic: Some(diagnostic), .. } => format!("Location {} failed; {diagnostic}", match category {
            LocationSwitchFailure::Configuration => "configuration", LocationSwitchFailure::Storage => "storage", LocationSwitchFailure::Runtime => "runtime",
        }),
        CoreError::LocationSwitch { category, .. } => match category {
            LocationSwitchFailure::Configuration =>
                "Location configuration failed; check the target directory, opencode.json/jsonc and selected model".to_string(),
            LocationSwitchFailure::Storage =>
                "Location storage failed; check the data directory and saved selection".to_string(),
            LocationSwitchFailure::Runtime =>
                "Location runtime failed; check the target's native settings".to_string(),
        },
        CoreError::TurnBusy => "turn active; location switch refused".to_string(),
        _ => "Location switch unavailable; retry after checking the data directory".to_string(),
    }
}

fn reload_error(error: CoreError) -> String {
    match error {
        CoreError::LocationSwitch {
            diagnostic: Some(diagnostic),
            ..
        } => format!("Configuration reload failed; {diagnostic}"),
        CoreError::TurnBusy => "turn active; configuration reload refused".into(),
        CoreError::ProviderUnavailable(diagnostic) => {
            format!("Configuration reload failed; {diagnostic}")
        }
        CoreError::LocationSwitch { category, .. } => match category {
            LocationSwitchFailure::Configuration => {
                "Configuration reload failed; check opencode.json/jsonc and selected model".into()
            }
            LocationSwitchFailure::Storage => {
                "Configuration reload failed; check the data directory and saved selection".into()
            }
            LocationSwitchFailure::Runtime => {
                "Configuration reload failed; check native settings".into()
            }
        },
        _ => "Configuration reload failed; check the current Location".into(),
    }
}

/// Hints were coalesced while the reload slot was held. A refused atomic reload
/// leaves an owner live; consume its current facts before the explicit
/// refusal, which must remain louder than any new background-service alert.
async fn finish_reload_refusal(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    message: String,
) {
    if let Ok(snapshot) = app.mcp_status().await {
        apply_mcp_to_views(state, deck, snapshot);
    }
    state.push_transient_note(&message, NoteVariant::Error);
}

/// Owner success is already published. Refresh every retained route before
/// acknowledging the slash draft; an external shared handle may have switched
/// Locations while the asynchronous rebuild was in flight.
async fn finish_reload(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    snapshot: ReloadLocationSnapshot,
    slash_draft: Option<String>,
) -> Result<(), String> {
    if state.chrome.location.as_deref() != Some(snapshot.location.as_str())
        || deck.location.as_deref() != Some(snapshot.location.as_str())
    {
        state.disable_clipboard_until_catalog();
        for parked in deck.tabs.iter_mut().flatten() {
            parked.disable_clipboard_until_catalog();
        }
        if let Some(home) = deck.home.as_mut() {
            home.disable_clipboard_until_catalog();
        }
        return Err(
            "Configuration reload incomplete; visible Location changed during refresh".into(),
        );
    }
    // The owner has already published the replacement. Unlike the remainder
    if snapshot.catalog.auto_accept != oc_core::queries::AutoAcceptState::Unsupported {
        let permission_auto = deck.cli_auto || snapshot.catalog.chrome.permissions_auto;
        app.register_approval_consumer(permission_auto)
            .await
            .map_err(|e| e.to_string())?;
        deck.permission_auto = Some(permission_auto);
    }
    // of each view's catalog, terminal.copy is safety-sensitive: a failed
    // session/Home refresh must not retain an obsolete automatic-copy mode.
    let copy_mode = snapshot.catalog.chrome.terminal_copy;
    // This setting is already owner-published too. Footer presentation must
    // follow it even if a later route-specific catalog query fails.
    let session_tps = snapshot.catalog.chrome.session_tps;
    state.chrome.session_tps = session_tps;
    state.refresh_clipboard_mode(copy_mode);
    for parked in deck.tabs.iter_mut().flatten() {
        parked.chrome.session_tps = session_tps;
        parked.refresh_clipboard_mode(copy_mode);
    }
    if let Some(home) = deck.home.as_mut() {
        home.chrome.session_tps = session_tps;
        home.refresh_clipboard_mode(copy_mode);
    }
    let mut catalogs = Vec::with_capacity(deck.tabs.len());
    for (index, parked) in deck.tabs.iter().enumerate() {
        let view = if deck.active_tab == Some(index) {
            &*state
        } else {
            parked.as_ref().expect("parked tab")
        };
        let session = view.attached_session().expect("real tab");
        catalogs.push(
            app.session_selection(session.clone(), view.home, SelectionAction::Current)
                .await
                .map_err(|_| "Configuration reload incomplete; session selection refresh failed; retry /reload".to_string())?,
        );
    }
    let home_catalog = if deck.active_tab.is_none() || deck.home.is_some() {
        Some(
            app.home_selection(SelectionAction::Current)
                .await
                .map_err(|_| {
                    "Configuration reload incomplete; Home selection refresh failed; retry /reload"
                        .to_string()
                })?,
        )
    } else {
        None
    };
    if app
        .catalog()
        .await
        .map_err(|_| "Configuration reload incomplete; catalog verification failed".to_string())?
        .chrome
        .location
        .as_deref()
        != Some(snapshot.location.as_str())
        || catalogs
            .iter()
            .any(|catalog| catalog.chrome.location.as_deref() != Some(snapshot.location.as_str()))
        || home_catalog.as_ref().is_some_and(|catalog| {
            catalog.chrome.location.as_deref() != Some(snapshot.location.as_str())
        })
    {
        return Err("Configuration reload incomplete; Location changed during refresh".into());
    }
    if let Some((_, job)) = deck.mention_job.take() {
        job.abort();
    }
    deck.mention_pending = None;
    deck.mention_failed = None;
    let mcp = app.mcp_status().await.map_err(|_| {
        "Configuration reload incomplete; MCP status verification failed".to_string()
    })?;
    if mcp.binding.location != snapshot.location {
        return Err("Configuration reload incomplete; MCP Location changed during refresh".into());
    }
    // Explicit operation feedback survives unchanged causes; a new cause can
    // replace it with its brief service summary, not be hidden by a later toast.
    state.push_transient_note("Configuration reloaded", NoteVariant::Success);
    for (index, catalog) in catalogs.into_iter().enumerate() {
        if deck.active_tab == Some(index) {
            state.refresh_configuration(catalog);
        } else {
            deck.tabs[index]
                .as_mut()
                .expect("parked tab")
                .refresh_configuration(catalog);
        }
    }
    if let Some(catalog) = home_catalog {
        if deck.active_tab.is_none() {
            state.refresh_configuration(catalog);
        } else if let Some(home) = deck.home.as_mut() {
            home.refresh_configuration(catalog);
        }
    }
    apply_mcp_to_views(state, deck, mcp);
    if slash_draft.as_deref() == Some(state.input()) {
        state.accept_intent();
    }
    Ok(())
}

/// Adopt the accepted route before optional reads; keep bounded prompt drafts
/// without carrying old Location views or configuration into the new owner.
async fn adopt_picker_open(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    receipt: oc_core::queries::SessionPickerOpen,
) {
    let drafts = deck
        .picker_drafts
        .remove(&receipt.location)
        .unwrap_or_default();
    if let Some(location) = &deck.location {
        let mut old = vec![(state.attached_session().cloned(), state.input().to_owned())];
        old.extend(
            deck.tabs
                .iter()
                .flatten()
                .map(|view| (view.attached_session().cloned(), view.input().to_owned())),
        );
        if let Some(home) = &deck.home {
            old.push((None, home.input().to_owned()));
        }
        if deck.picker_drafts.len() >= 4
            && let Some(key) = deck.picker_drafts.keys().next().cloned()
        {
            deck.picker_drafts.remove(&key);
        }
        deck.picker_drafts.insert(location.clone(), old);
    }
    if let Some((_, job)) = deck.title_job.take() {
        job.abort();
    }
    if let Some((_, job)) = deck.mention_job.take() {
        job.abort();
    }
    if let Some(job) = deck.recovery_job.take() {
        job.abort();
    }
    deck.conversation_recovery = None;
    deck.mention_pending = None;
    deck.mention_failed = None;
    deck.reset_deck();
    deck.picker_pending_tabs.clear();
    deck.location = Some(receipt.location);
    deck.revision = receipt.deck.revision;
    deck.save_disabled = false;
    deck.read_only = false;
    let mut active = TuiState::new(app.clone(), receipt.session.clone());
    active.attach_page(&receipt.page);
    active.apply_catalog(receipt.catalog.clone());
    refresh_compactions(app, &mut active).await;
    if let Some((_, draft)) = drafts
        .iter()
        .find(|(id, _)| id.as_ref() == Some(&receipt.session))
    {
        active.restore_prompt(draft.clone());
    }
    // Adopt the accepted route before any optional parked-view refresh.
    *state = active;
    for (index, session) in receipt.deck.sessions.into_iter().enumerate() {
        let new_session_tab = receipt
            .deck
            .new_session_titles
            .get(index)
            .copied()
            .unwrap_or(false);
        if session == receipt.session {
            state.new_session_tab = new_session_tab;
            deck.active_tab = Some(deck.tabs.len());
            deck.tabs.push(None);
        } else {
            let mut view = TuiState::new(app.clone(), session.clone());
            view.new_session_tab = new_session_tab;
            view.apply_catalog(receipt.catalog.clone());
            if let Some((_, draft)) = drafts.iter().find(|(id, _)| id.as_ref() == Some(&session)) {
                view.restore_prompt(draft.clone());
            }
            let page = app
                .history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
                .await;
            let catalog = app
                .session_selection(session.clone(), false, SelectionAction::Current)
                .await;
            match (page, catalog) {
                (Ok(page), Ok(catalog)) => {
                    view.attach_page(&page);
                    view.apply_catalog(catalog);
                    refresh_compactions(app, &mut view).await;
                }
                _ => {
                    deck.picker_pending_tabs.insert(session);
                    view.push_note("Session opened; tab refresh pending");
                }
            }
            deck.tabs.push(Some(view));
        }
        deck.tab_cards_before.push(None);
    }
    if deck.tabs.len() < MAX_TABS
        && let Some((_, draft)) = drafts.iter().find(|(id, _)| id.is_none())
    {
        let mut home = TuiState::new_home(app.clone());
        match app.home_selection(SelectionAction::Current).await {
            Ok(catalog) => home.apply_catalog(catalog),
            Err(_) => {
                home.apply_catalog(receipt.catalog);
                home.disable_clipboard_until_catalog();
                home.push_note("Home selection refresh pending");
            }
        }
        home.restore_prompt(draft.clone());
        deck.home = Some(home);
    }
    deck.sync_tabs(state);
}

/// A published Location invalidates every old view, including the parked
/// Home draft. On a broken preference keep the new Location's Home and show
/// a static diagnostic rather than accidentally repainting old-location data.
async fn adopt_location(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    catalog: CatalogSnapshot,
    location: &str,
) {
    deck.reset_deck();
    // Never carry A's binding or token into a successfully published B.
    deck.location = None;
    deck.revision = None;
    deck.save_disabled = false;
    deck.read_only = false;
    let mut previous = std::mem::replace(state, TuiState::new_home(app.clone()));
    state.apply_catalog(catalog);
    match app.tab_deck().await {
        Ok(snapshot)
            if snapshot.location != location
                || snapshot.location.is_empty()
                || snapshot.sessions.len() > MAX_TABS
                || (!snapshot.new_session_titles.is_empty()
                    && snapshot.new_session_titles.len() != snapshot.sessions.len())
                || (snapshot.active.is_none() && snapshot.sessions.len() == MAX_TABS)
                || snapshot
                    .active
                    .as_ref()
                    .is_some_and(|active| !snapshot.sessions.contains(active)) =>
        {
            deck.save_disabled = true;
            state.push_note("Location tabs unavailable; check saved tabs");
        }
        Ok(snapshot) => {
            deck.location = Some(snapshot.location);
            deck.revision = snapshot.revision;
            let home = std::mem::replace(state, TuiState::new_home(app.clone()));
            // `home` carries B's published catalog; no old Location view can
            // be reactivated even if one of B's parked reads fails.
            *state = restore_views(
                app,
                deck,
                snapshot
                    .sessions
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| {
                        (
                            id,
                            snapshot
                                .new_session_titles
                                .get(index)
                                .copied()
                                .unwrap_or(false),
                        )
                    })
                    .collect(),
                snapshot.active,
                None,
                Some(home),
                false,
            )
            .await
            .expect("published Home is already available");
        }
        Err(_) => {
            // With no read token the owner will refuse to overwrite an
            // existing broken preference. A successful switch supplies B.
            deck.location = (!location.is_empty()).then(|| location.to_string());
            deck.save_disabled = true;
            state.push_note("saved tab deck unavailable; review saved tabs");
        }
    }
    // A route change must preserve the fixed diagnostic, including when a
    // loaded active view replaces the Home state.
    if deck.save_disabled {
        state.push_note("Location tabs unavailable; check saved tabs");
    } else {
        state.push_note(&format!("location: {location}"));
    }
    state.inherit_shell_view(&mut previous);
    state.inherit_terminal_view(&mut previous);
    if state.shells_open()
        && let Err(error) = refresh_shells(app, state).await
    {
        state.push_note(&error);
    }
    deck.sync_tabs(state);
}

fn require_session(state: &TuiState) -> Result<SessionId, String> {
    state
        .attached_session()
        .cloned()
        .ok_or_else(|| "no active session; submit a prompt first".to_string())
}

async fn selection(
    app: &CoreApp,
    state: &TuiState,
    action: SelectionAction,
) -> Result<CatalogSnapshot, String> {
    match state.attached_session() {
        Some(session) => app
            .session_selection(session.clone(), state.home, action)
            .await
            .map_err(|e| e.to_string()),
        None => app.home_selection(action).await.map_err(|e| e.to_string()),
    }
}

async fn handle_worker_event(
    app: &CoreApp,
    state: &mut TuiState,
    _loop_state: &mut LoopState,
    session: &SessionId,
    event: CoreEvent,
) -> Result<(), String> {
    if child_controls::route_event(app, state, _loop_state, &event).await? {
        return Ok(());
    }
    if let CoreEvent::TerminalChanged { session: owner } = &event {
        if state.attached_session() == Some(owner) {
            terminal_controls::refresh_or_report(app, state).await;
        }
        for parked in _loop_state
            .tabs
            .iter_mut()
            .flatten()
            .filter(|v| v.attached_session() == Some(owner))
        {
            terminal_controls::refresh_or_report(app, parked).await;
        }
        return Ok(());
    }
    if let CoreEvent::ShellChanged { session: owner } = &event {
        if state.attached_session() == Some(owner) || state.shells_open() {
            refresh_shells(app, state).await?;
        }
        for parked in _loop_state
            .tabs
            .iter_mut()
            .flatten()
            .filter(|view| view.attached_session() == Some(owner) || view.shells_open())
        {
            refresh_shells(app, parked).await?;
        }
        return Ok(());
    }
    if let CoreEvent::SessionMoved {
        session: moved,
        location,
        ..
    } = &event
    {
        if let Some(snapshot) = location {
            if app.catalog().await.is_ok_and(|catalog| {
                catalog.chrome.location.as_deref() == Some(snapshot.location.as_str())
            }) {
                adopt_location(
                    app,
                    state,
                    _loop_state,
                    snapshot.catalog.clone(),
                    &snapshot.location,
                )
                .await;
            }
        } else {
            // An explicit idle target left this scope; retire only its parked
            // view. The caller's Location, draft and all surviving views stay owned.
            if let Some(index) = _loop_state.tabs.iter().position(|view| {
                view.as_ref()
                    .is_some_and(|view| view.attached_session() == Some(moved))
            }) {
                _loop_state.tabs.remove(index);
                _loop_state.tab_cards_before.remove(index);
                if let Some(active) = _loop_state.active_tab.as_mut()
                    && *active > index
                {
                    *active -= 1;
                }
            }
            match app.tab_deck().await {
                Ok(snapshot)
                    if _loop_state.location.as_deref() == Some(snapshot.location.as_str())
                        && _loop_state.snapshot(state).sessions == snapshot.sessions
                        && _loop_state.snapshot(state).new_session_titles
                            == snapshot.new_session_titles
                        && _loop_state.snapshot(state).active == snapshot.active =>
                {
                    _loop_state.revision = snapshot.revision;
                }
                _ => {
                    _loop_state.save_disabled = true;
                    state.push_note("Session moved; saved tabs refresh required");
                }
            }
            _loop_state.sync_tabs(state);
        }
        return Ok(());
    }
    if matches!(
        &event,
        CoreEvent::PermissionAsked(_)
            | CoreEvent::PermissionResolved { .. }
            | CoreEvent::QuestionAsked(_)
            | CoreEvent::QuestionResolved { .. }
    ) {
        refresh_approvals(app, state).await?;
        _loop_state.approvals_checked = None;
        return Ok(());
    }
    if let CoreEvent::Compaction(snapshot) = event {
        apply_compaction_to_view(state, _loop_state, snapshot);
        return Ok(());
    }
    if let CoreEvent::McpChanged(snapshot) = event {
        apply_mcp_to_views(state, _loop_state, snapshot);
        return Ok(());
    }
    let owner = match &event {
        CoreEvent::ChildNotice(notice) => &notice.job.parent,
        CoreEvent::ShellNotice(notice) => &notice.session,
        CoreEvent::ShellChanged { session }
        | CoreEvent::TerminalChanged { session }
        | CoreEvent::SessionMoved { session, .. }
        | CoreEvent::SessionModelSelected { session, .. }
        | CoreEvent::SessionTitleUpdated { session, .. }
        | CoreEvent::RetryScheduled { session, .. }
        | CoreEvent::TurnStarted { session, .. }
        | CoreEvent::TurnPresentation { session, .. }
        | CoreEvent::TextDelta { session, .. }
        | CoreEvent::ReasoningDelta { session, .. }
        | CoreEvent::ReasoningItemEnded { session, .. }
        | CoreEvent::ToolCallStarted { session, .. }
        | CoreEvent::ToolArgumentStream { session, .. }
        | CoreEvent::ToolCallFinished { session, .. }
        | CoreEvent::TurnUsage { session, .. }
        | CoreEvent::TurnFinished { session, .. }
        | CoreEvent::TurnInterrupted { session, .. }
        | CoreEvent::TurnFailed { session, .. } => session,
        CoreEvent::Compaction(_) | CoreEvent::McpChanged(_) | CoreEvent::ProviderChanged => {
            unreachable!("handled above")
        }
        CoreEvent::PermissionAsked(_)
        | CoreEvent::PermissionResolved { .. }
        | CoreEvent::QuestionAsked(_)
        | CoreEvent::QuestionResolved { .. } => {
            unreachable!("handled above")
        }
    };
    if state.attached_session() != Some(owner) {
        return Ok(());
    }
    match event {
        CoreEvent::ChildNotice(notice) => {
            let Some(message) = notice.job.message_id else {
                return Ok(());
            };
            let exact = app
                .history_message(
                    notice.job.parent.clone(),
                    oc_core::session::MessageId(message),
                )
                .await
                .map_err(|error| error.to_string())?;
            if !exact
                .rows
                .iter()
                .any(|row| matches!(row.child, Some(oc_core::queries::ChildHistory::Notice(_))))
            {
                return Ok(());
            }
            if state.has_subagent_cards() {
                child_controls::refresh(app, state).await?;
            }
            if state.is_busy() {
                let recent = app
                    .history_page(notice.job.parent, None, None, HISTORY_PAGE_LIMIT)
                    .await
                    .map_err(|error| error.to_string())?;
                state.refresh_child_notice_page(&recent);
                state.refresh_child_notice_page(&exact);
            } else {
                let page = if let Some(selected) = state.linked_child().cloned() {
                    app.read_child(selected.parent.clone(), selected).await
                } else {
                    app.history_page(notice.job.parent, None, None, HISTORY_PAGE_LIMIT)
                        .await
                }
                .map_err(|error| error.to_string())?;
                state.refresh_completed_page(&page);
            }
        }
        CoreEvent::ShellChanged { .. } => unreachable!("handled above"),
        CoreEvent::TerminalChanged { .. } => {}
        CoreEvent::SessionModelSelected { session, commit } => {
            state.apply_session_model_selected(&session, &commit)
        }
        CoreEvent::ShellNotice(notice) => {
            if notice.user_requested {
                let page = app
                    .history_message(
                        notice.session.clone(),
                        oc_core::session::MessageId(notice.message_id),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if page.rows.iter().any(|row| row.user_shell.is_some()) {
                    if state.is_busy() {
                        state.refresh_user_shell_page(&page);
                    } else {
                        let page = app
                            .history_page(notice.session, None, None, HISTORY_PAGE_LIMIT)
                            .await
                            .map_err(|error| error.to_string())?;
                        state.refresh_completed_page(&page);
                    }
                }
            } else {
                let exact = app
                    .history_message(
                        notice.session.clone(),
                        oc_core::session::MessageId(notice.message_id),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                if !exact.rows.iter().any(|row| {
                    row.shell_notice
                        .as_ref()
                        .is_some_and(|metadata| metadata.operation == notice.shell_id)
                }) {
                    return Ok(());
                }
                if state.has_model_shell_card(&notice.shell_id)
                    && let Ok(snapshot) = app
                        .shell_snapshot(notice.session.clone(), notice.shell_id.clone())
                        .await
                {
                    state.apply_model_shell_snapshot(&snapshot);
                }
                if state.is_busy() {
                    let recent = app
                        .history_page(notice.session, None, None, HISTORY_PAGE_LIMIT)
                        .await
                        .map_err(|error| error.to_string())?;
                    state.refresh_child_notice_page(&recent);
                    state.refresh_child_notice_page(&exact);
                    return Ok(());
                }
                let page = if let Some(selected) = state.linked_child().cloned() {
                    app.read_child(selected.parent.clone(), selected).await
                } else {
                    app.history_page(notice.session, None, None, HISTORY_PAGE_LIMIT)
                        .await
                }
                .map_err(|error| error.to_string())?;
                state.refresh_completed_page(&page);
            }
        }
        CoreEvent::Compaction(_) | CoreEvent::McpChanged(_) | CoreEvent::ProviderChanged => {
            unreachable!("handled above")
        }
        CoreEvent::PermissionAsked(_)
        | CoreEvent::PermissionResolved { .. }
        | CoreEvent::QuestionAsked(_)
        | CoreEvent::QuestionResolved { .. } => {
            unreachable!("handled above")
        }
        CoreEvent::SessionMoved { .. } => unreachable!("handled above"),
        CoreEvent::SessionTitleUpdated { title, .. } => state.session_title = Some(title),
        CoreEvent::TurnStarted {
            turn, model_switch, ..
        } => {
            if state.linked_child().is_some() {
                state.begin_linked_turn(turn.clone());
            }
            if let Some(notice) = model_switch {
                state.apply_model_switch(&turn, &notice);
            }
        }
        CoreEvent::TurnPresentation {
            turn, projection, ..
        } => state.apply_presentation(&turn, &projection),
        CoreEvent::RetryScheduled {
            turn, span, retry, ..
        } => state.apply_retry(&turn, &span, &retry),
        CoreEvent::TextDelta { turn, delta, .. } => state.apply_delta(&turn, &delta),
        CoreEvent::ReasoningDelta { turn, delta, .. } => {
            state.apply_reasoning_delta(&turn, &delta);
        }
        CoreEvent::ReasoningItemEnded { turn, .. } => {
            state.apply_reasoning_item_ended(&turn);
        }
        CoreEvent::ToolArgumentStream { turn, event, .. } => {
            state.apply_tool_argument_stream(&turn, &event)
        }
        CoreEvent::ToolCallStarted {
            turn,
            op,
            name,
            input,
            dcp_topic,
            ..
        } => {
            state.apply_tool_started_with_presentation(&turn, &op, &name, &input, dcp_topic);
            if name == "subagent" && state.active_turn() == Some(&turn) {
                child_controls::refresh(app, state).await?;
            }
        }
        CoreEvent::ToolCallFinished {
            turn,
            op,
            name,
            state: tool_state,
            output,
            output_bytes,
            output_truncated,
            output_presentation,
            patch_effects,
            dcp,
            question,
            ..
        } => {
            state.apply_tool_finished_with_output_presentation(
                &turn,
                &op,
                &name,
                &tool_state,
                &output,
                output_bytes,
                output_truncated,
                patch_effects,
                dcp,
                question,
                output_presentation,
            );
            if name == "subagent" && state.active_turn() == Some(&turn) {
                child_controls::refresh(app, state).await?;
            }
            if name == "compress" && state.active_turn() == Some(&turn) {
                refresh_dcp(app, state, session).await;
                refresh_dcp_summaries(app, state).await;
            }
        }
        CoreEvent::TurnUsage {
            turn,
            input_tokens,
            output_tokens,
            streamed_ms,
            ..
        } => state.apply_usage(&turn, input_tokens, output_tokens, streamed_ms),
        CoreEvent::TurnFinished {
            turn,
            text,
            duration_ms,
            warnings,
            service_warning_range,
            ..
        } => {
            let current = state.active_turn() == Some(&turn);
            let compress = state.is_compress_turn(&turn);
            state.apply_finished(&turn, &text, duration_ms);
            if current {
                let page = if let Some(selected) = state.linked_child().cloned() {
                    app.read_child(selected.parent.clone(), selected).await
                } else {
                    app.history_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
                        .await
                }
                .map_err(|e| e.to_string())?;
                state.refresh_completed_page(&page);
                refresh_dcp_summaries(app, state).await;
                if compress {
                    report_compress_outcome(app, state, session, &turn, &page).await;
                }
            }
            state.apply_turn_warnings(warnings, service_warning_range);
        }
        CoreEvent::TurnInterrupted {
            turn,
            partial,
            duration_ms,
            ..
        } => {
            let compress = state.is_compress_turn(&turn);
            state.apply_interrupted(&turn, &partial, duration_ms);
            if compress {
                state.notify_dcp(DcpOutcome::Failed {
                    reason: "compress turn cancelled".to_string(),
                });
            }
        }
        CoreEvent::TurnFailed {
            turn,
            error,
            warnings,
            service_warning_range,
            ..
        } => {
            let compress = state.is_compress_turn(&turn);
            state.apply_failed(&turn, &error);
            state.apply_turn_warnings(warnings, service_warning_range);
            if compress {
                state.notify_dcp(DcpOutcome::Failed {
                    reason: error.to_string(),
                });
            }
        }
    }
    Ok(())
}

fn apply_mcp_to_views(
    state: &mut TuiState,
    deck: &mut LoopState,
    snapshot: oc_core::queries::McpSnapshot,
) {
    // Retiring the old runtime publishes disabled/pending rows. That is not
    // service recovery. The acknowledged reload reads one current owner
    // snapshot before releasing this captured view; queued old instances then
    // fail the normal binding guards. No second registry/history is needed.
    if deck.reload_job.is_some() {
        return;
    }
    state.apply_mcp_snapshot(snapshot.clone());
    for parked in deck.tabs.iter_mut().flatten() {
        parked.set_service_feedback_visible(false);
        parked.apply_mcp_snapshot(snapshot.clone());
    }
    if let Some(home) = deck.home.as_mut() {
        home.set_service_feedback_visible(false);
        home.apply_mcp_snapshot(snapshot);
    }
}

async fn refresh_shells(app: &CoreApp, state: &mut TuiState) -> Result<(), String> {
    let Some(session) = state.attached_session().cloned() else {
        return Ok(());
    };
    state.apply_shell_jobs(
        app.shell_jobs(session)
            .await
            .map_err(|error| error.to_string())?,
    );
    if let Some(job) = state.shell_viewer().cloned() {
        let snapshot = app
            .shell_snapshot(job.session.clone(), job.shell_id.clone())
            .await;
        match snapshot {
            Ok(snapshot) => state.apply_shell_snapshot(snapshot),
            Err(_) => state.apply_shell_read_failure(&job),
        }
    }
    Ok(())
}

fn apply_compaction_to_view(
    state: &mut TuiState,
    deck: &mut LoopState,
    snapshot: oc_core::compaction::CompactionSnapshot,
) {
    if state
        .attached_session()
        .is_some_and(|id| id.0 == snapshot.session)
    {
        state.apply_compaction(snapshot);
    } else if let Some(view) = deck.tabs.iter_mut().flatten().find(|view| {
        view.attached_session()
            .is_some_and(|id| id.0 == snapshot.session)
    }) {
        view.apply_compaction(snapshot);
    }
}

async fn finish_compaction_admission(state: &mut TuiState, deck: &mut LoopState) {
    if !deck
        .compaction_job
        .as_ref()
        .is_some_and(|job| job.is_finished())
    {
        return;
    }
    match deck
        .compaction_job
        .take()
        .expect("finished compaction admission")
        .await
        .unwrap_or(Err(CoreError::Shutdown))
    {
        Ok((snapshot, revision)) => {
            if state
                .attached_session()
                .is_some_and(|id| id.0 == snapshot.session)
            {
                state.compaction_admitted(snapshot, revision);
            } else if let Some(view) = deck.tabs.iter_mut().flatten().find(|view| {
                view.attached_session()
                    .is_some_and(|id| id.0 == snapshot.session)
            }) {
                view.compaction_admitted(snapshot, revision);
            }
        }
        Err(error) => {
            state.push_transient_note(&format!("compaction: {error}"), NoteVariant::Error)
        }
    }
    deck.sync_tabs(state);
}

async fn refresh_compactions(app: &CoreApp, state: &mut TuiState) {
    let Some(session) = state.attached_session().cloned() else {
        return;
    };
    match app.compaction_history(session).await {
        Ok(snapshots) => state.apply_compaction_history(snapshots),
        Err(error) => state.push_transient_note(
            &format!("Session opened; compaction refresh failed: {error}"),
            NoteVariant::Error,
        ),
    }
    refresh_dcp_summaries(app, state).await;
}

async fn refresh_dcp_summaries(app: &CoreApp, state: &mut TuiState) {
    let Some(session) = state.attached_session().cloned() else {
        return;
    };
    for (op, index) in state.dcp_summary_requests() {
        // This is a bounded preview, not the full transcript or tool output.
        // Missing legacy/query data stays explicitly unavailable on the card.
        if let Ok(Some(page)) = app
            .dcp_summary_page(
                session.clone(),
                op.clone(),
                index,
                0,
                oc_tui::dcp_view::SUMMARY_PAGE_BYTES,
            )
            .await
        {
            state.apply_dcp_summary(&session, &op, page);
        }
    }
}

/// Refresh the DCP snapshot for the attached session.
async fn refresh_dcp(app: &CoreApp, state: &mut TuiState, session: &SessionId) {
    match app.dcp_snapshot(session.clone()).await {
        Ok(snapshot) => state.apply_dcp_snapshot(snapshot),
        Err(error) => state.push_note(&format!("DCP query failed: {error}")),
    }
}

/// Only this exact turn can establish a manual-operation outcome. Successful
/// notices are emitted once by ToolCallFinished from its typed run snapshot.
async fn report_compress_outcome(
    app: &CoreApp,
    state: &mut TuiState,
    session: &SessionId,
    turn: &oc_core::core_app::WorkerTurnId,
    page: &oc_core::queries::HistoryPage,
) {
    refresh_dcp(app, state, session).await;
    let recorded = page
        .rows
        .iter()
        .filter_map(|row| row.turn.as_ref())
        .filter(|owner| owner.id == turn.0)
        .any(|owner| {
            owner.parts.iter().any(|part| {
                matches!(part,
        oc_core::queries::TranscriptPart::Tool(tool) if tool.name == "compress")
            })
        });
    if !recorded {
        state.notify_dcp(DcpOutcome::Failed {
            reason: "no compression recorded in this turn".to_string(),
        });
    }
}

/// Bounded view metrics for PTY qualification (opt-in, never in normal use).
fn write_metrics(state: &TuiState, deck: &LoopState, frames: Option<&FrameMetrics>) {
    let Some(path) = std::env::var_os(METRICS_ENV) else {
        return;
    };
    let metrics = serde_json::json!({
        "session": state.attached_session().map(|session| &session.0),
        "tab_count": state.tab_presentation().0.len(),
        "tab_ids": deck.snapshot(state).sessions.iter().map(|id| id.0.as_str()).collect::<Vec<_>>(),
        "active_tab": deck.active_tab,
        "retained_bytes": state.retained_bytes(),
        "window_rows": state.history().len(),
        "window_total": state.history().total(),
        "panel": format!("{:?}", state.panel()),
        "status": format!("{:?}", state.status()),
        "frame_count": frames.map_or(0, |frames| frames.count),
        "frame_sum_ns": frames.map_or(0, |frames| frames.sum_ns),
        "frame_max_ns": frames.map_or(0, |frames| frames.max_ns),
        "changed_frames": frames.map_or(0, |frames| frames.changed_frames),
        "wakeups": frames.map_or(0, |frames| frames.wakeups),
        "input_events": frames.map_or(0, |frames| frames.input_events),
        "worker_events": frames.map_or(0, |frames| frames.worker_events),
        "terminal_write_calls": frames.map_or(0, |frames| frames.writes.calls),
        "terminal_flush_calls": frames.map_or(0, |frames| frames.writes.flush_calls),
        "terminal_write_bytes": frames.map_or(0, |frames| frames.writes.bytes),
        "terminal_write_sum_ns": frames.map_or(0, |frames| frames.writes.sum_ns),
        "terminal_write_max_ns": frames.map_or(0, |frames| frames.writes.max_ns),
        "frame_samples_ns": frames.map(|frames| &frames.frame_samples),
        "monotonic_epoch_ns": frames.and_then(|frames| frames.monotonic_epoch_ns),
        "frame_monotonic_ns": frames.map(|frames| &frames.frame_monotonic_ns),
        "wake_monotonic_ns": frames.map(|frames| &frames.wake_monotonic_ns),
        "worker_event_queue_peak": frames.map_or(0, |frames| frames.worker_event_queue_peak),
        "worker_event_queue_lagged": frames.map_or(0, |frames| frames.worker_event_queue_lagged),
        "live_text_bytes_current": frames.map_or(0, |frames| frames.live_current.text_bytes),
        "live_text_bytes_peak": frames.map_or(0, |frames| frames.live_peak.text_bytes),
        "live_reasoning_bytes_current": frames.map_or(0, |frames| frames.live_current.reasoning_bytes),
        "live_reasoning_bytes_peak": frames.map_or(0, |frames| frames.live_peak.reasoning_bytes),
        "live_part_count_current": frames.map_or(0, |frames| frames.live_current.part_count),
        "live_part_count_peak": frames.map_or(0, |frames| frames.live_peak.part_count),
        "markdown_cache_retained_bytes_current": frames.map_or(0, |frames| frames.live_current.markdown_cache_retained_bytes),
        "markdown_cache_retained_bytes_peak": frames.map_or(0, |frames| frames.live_peak.markdown_cache_retained_bytes),
    });
    let _ = std::fs::write(path, metrics.to_string());
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "approval_tests.rs"]
mod approval_tests;
