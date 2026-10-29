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
mod catalog_jobs;
mod child_controls;
mod child_jobs;
mod clipboard;
mod history_jobs;
mod navigation_jobs;
mod picker_jobs;
mod reload_jobs;
mod resource_jobs;
use clipboard::{ClipboardJob, start_clipboard, sync_clipboard};
mod connect_controls;
mod control_jobs;
mod deck_jobs;
mod display_rate;
mod idle_release;
mod inbox_jobs;
mod notice_jobs;
mod read_jobs;
mod shared;
mod tab_activity;
mod terminal_controls;
mod work_controls;

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
    completion_wakeups: u64,
    memory_releases: u64,
    display_hz: Option<u32>,
    input_events: u64,
    worker_events: u64,
    changed_frames: u64,
    /// Frames that repainted only the tab strip, and strip-only attempts
    /// that fell back to a full frame.
    strip_frames: u64,
    strip_refused: u64,
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
/// Live turn events retained while a workspace read is in flight. Overflow
/// drops them and requests another gap read.
const MAX_DEFERRED_EVENTS: usize = 4 * oc_core::session::MAX_QUEUE_ITEMS;
/// Input/worker drain slice per loop turn; fairness, not the paint rate.
const BURST_BUDGET: Duration = Duration::from_millis(2);
/// Background lanes and receipts wake the loop through `oc_core::wake` when
/// they complete. A wake can precede the handle's `is_finished` by a few
/// instructions, so one short recheck follows it; the slow fallback only
/// bounds an unexpected missing wake, it is not a polling cadence.
const COMPLETION_RECHECK: Duration = Duration::from_millis(1);
const COMPLETION_FALLBACK: Duration = Duration::from_millis(100);
const MAX_TABS: usize = 16;
const OWNER_LOST: &str = "Runtime connection lost; reconnecting. In-flight input may already be accepted and was not resent; new input stays a draft.";
const OWNER_RESTORED: &str = "Runtime connection restored; state reloaded. Nothing was resent.";
const MENTION_DEBOUNCE: Duration = Duration::from_millis(90);

/// Launch the interactive TUI; returns process exit code.
pub async fn run_tui(data_dir: &Path, session_opt: Option<String>, auto_once: bool) -> ExitCode {
    install_panic_hook();
    // Keep presentation state out of the shared CLI/native-owner dispatch future.
    match Box::pin(run_inner(data_dir, session_opt, auto_once)).await {
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
            "no TTY for interactive TUI; use `oc2 run \"<prompt>\"` for headless use".to_string(),
        );
    }
    let project = std::env::current_dir().map_err(|e| e.to_string())?;
    oc_adapters::trace::log("tui.begin", "project=configured");
    let session = session_opt
        .map(|raw| SessionId::new(raw).ok_or_else(|| "invalid session id".to_string()))
        .transpose()?;
    let (app, guard, _notices) = match oc_adapters::native::attach(
        data_dir,
        Some(&project),
        oc_adapters::native::Lease::Tui,
    )
    .await
    {
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
            return startup_failure(&mut terminal, StartupFailure::Diagnostic(issue)).map(|_| 1);
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
    reads: read_jobs::Reads,
    notices: notice_jobs::Jobs,
    children: child_jobs::Jobs,
    work: work_controls::Jobs,
    catalogs: catalog_jobs::Jobs,
    provider_feedback: Option<(Option<String>, String)>,
    picker: picker_jobs::Jobs,
    history_reads: history_jobs::Jobs,
    resources: resource_jobs::Jobs,
    controls: control_jobs::Controls,
    terminals: terminal_controls::Jobs,
    workspace_job: Option<shared::Job>,
    /// Applying a live turn event advances the projection floor that an
    /// in-flight workspace read was captured against. A streaming turn would
    /// then make every repair stale and request another gap read forever, so
    /// these wait, already revision-checked, until the read is applied.
    deferred_events: std::collections::VecDeque<(CoreEvent, u64)>,
    workspace_reads: u64,
    navigation_revision: u64,
    pointer_revision: u64,
    interaction_revision: u64,
    selection_dirty: bool,
    selection_blocked: bool,
    closed_owners: std::collections::HashMap<SessionId, Option<CatalogSnapshot>>,
    fresh_membership: std::collections::HashSet<SessionId>,
    deck_job: Option<deck_jobs::Job>,
    deck_read: Option<(
        String,
        tokio::task::JoinHandle<Result<TabDeckSnapshot, CoreError>>,
    )>,
    deck_read_revision: Option<u64>,
    home_job: Option<HomeJob>,
    home_pending: Option<((u64, u64), SelectionAction)>,
    unresolved_homes: std::collections::HashSet<u64>,
    home_commands: std::collections::HashMap<u64, (u64, String)>,
    cancel_jobs: std::collections::HashMap<CancelKey, CancelJob>,
    inbox_jobs: inbox_jobs::Jobs,
    activity: tab_activity::Activity,
    projection_revision: u64,
    shared_refresh: bool,
    shared_gap: bool,
    authentication: auth_controls::Controls,
    authentication_origin: Option<u64>,
    questions: oc_tui::question_view::QuestionView,
    cli_auto: bool,
    permission_auto: Option<bool>,
    sidebar_mcp_open: Option<bool>,
    approvals_checked: Option<(Option<SessionId>, Instant)>,
    compaction_job: Option<CompactionAdmission>,
    compaction_origin: Option<read_jobs::Route>,
    /// Bounded prompt drafts for picker Location round trips. Views and
    /// configuration are rebuilt from the newly accepted owner receipt.
    picker_drafts:
        std::collections::BTreeMap<String, Vec<(Option<SessionId>, oc_tui::ComposerDraft)>>,
    clipboard_job: Option<ClipboardJob>,
    picker_pending_tabs: std::collections::HashSet<SessionId>,
    conversation_job: Option<tokio::task::JoinHandle<Result<ConversationResult, String>>>,
    conversation_origin: Option<(read_jobs::Route, u64)>,
    conversation_recovery: Option<(SessionId, std::time::Instant)>,
    recovery_job: Option<ConversationRefresh>,
    recovery_route: Option<read_jobs::Route>,
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
    reload_verify: Option<reload_jobs::Job>,
    navigation_job: Option<navigation_jobs::Job>,
    scope_uncertain: bool,
    moved_pending: Option<navigation_jobs::Moved>,
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
    authoritative_deck: Option<TabDeckSnapshot>,
    /// A projected or unreadable preference cannot be safely rewritten from
    /// the visible tabs: hidden IDs must survive until a clean reload/repair.
    save_disabled: bool,
    /// A child requested by --session is a standalone history view. Never
    /// submit a turn or promote it into the Location's root-tab preference.
    read_only: bool,
    /// Linked child views are disposable; the real root deck is retained here.
    child_parent: Option<Box<TuiState>>,
    child_parent_read_only: bool,
    child_views: std::collections::HashMap<SessionId, TuiState>,
}

enum ConversationResult {
    Changed(oc_core::queries::ConversationSnapshot, ConversationPrepared),
    Forked(oc_core::queries::ForkSessionSnapshot, ConversationPrepared),
}

type HomeJob = (
    (u64, u64),
    tokio::task::JoinHandle<Result<CatalogSnapshot, CoreError>>,
);
type CancelKey = (SessionId, Option<oc_core::core_app::WorkerTurnId>, bool);
type CancelJob = tokio::task::JoinHandle<Result<(), CoreError>>;

#[cfg(test)]
type CopyTransport = fn(&str) -> Result<(), String>;

type ConversationPrepared = (
    Result<oc_core::queries::HistoryPage, String>,
    Result<CatalogSnapshot, String>,
    Result<Vec<oc_core::compaction::CompactionSnapshot>, String>,
);
type ConversationRefresh = tokio::task::JoinHandle<ConversationPrepared>;

fn conversation_refresh(app: CoreApp, session: SessionId) -> ConversationRefresh {
    oc_core::wake::spawn(async move {
        let page = app
            .transcript_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
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
    _app: &CoreApp,
    state: &mut TuiState,
    refresh: ConversationPrepared,
    _recovery: bool,
) -> bool {
    let (page, catalog, compactions) = refresh;
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
    success
}

impl LoopState {
    fn reload_pending(&self) -> bool {
        self.reload_job.is_some() || self.reload_verify.is_some()
    }
    fn conversation_owns(&self, view: &TuiState) -> bool {
        self.conversation_job.is_some()
            && self
                .conversation_origin
                .as_ref()
                .is_some_and(|(route, _)| route.owns(view))
    }
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
            || self.picker.pending()
            || self.navigation_job.is_some()
            || self.moved_pending.is_some()
            || self.reload_verify.is_some()
            || self.resources.pending()
            || self.history_reads.pending()
            || self.catalogs.pending()
            || self.work.pending()
            || self.children.pending()
            || self.notices.pending()
            || self.reads.pending()
            || self.controls.pending()
            || self.terminals.pending()
            || self.workspace_job.is_some()
            || self.activity.pending()
            || self.deck_job.is_some()
            || self.deck_read.is_some()
            || self.home_job.is_some()
            || self.home_pending.is_some()
            || !self.cancel_jobs.is_empty()
            || self.inbox_jobs.pending()
    }

    fn job_lanes(&self) -> u8 {
        u8::from(self.conversation_job.is_some())
            | (u8::from(self.recovery_job.is_some()) << 1)
            | (u8::from(self.mention_job.is_some()) << 2)
            | (u8::from(self.title_job.is_some()) << 3)
            | (u8::from(self.reload_pending()) << 4)
            | (u8::from(self.compaction_job.is_some()) << 5)
            | (u8::from(self.authentication.pending()) << 6)
            | (u8::from(self.clipboard_job.is_some()) << 7)
    }

    fn ready_job(&self, state: &TuiState) -> bool {
        self.picker.ready()
            || self
                .navigation_job
                .as_ref()
                .is_some_and(navigation_jobs::Job::ready)
            || self
                .reload_verify
                .as_ref()
                .is_some_and(reload_jobs::Job::ready)
            || self.resources.ready()
            || self.history_reads.ready()
            || self.catalogs.ready()
            || self.work.ready()
            || self.children.ready()
            || self.notices.ready()
            || self.reads.ready()
            || self.controls.ready()
            || self.terminals.ready()
            || self.workspace_job.as_ref().is_some_and(shared::Job::ready)
            || self.activity.ready()
            || self.deck_job.as_ref().is_some_and(|job| {
                job.ready()
                    && !read_jobs::views(state, self)
                        .any(|view| job.closing(view) && view.has_pending_transcript_projection())
            })
            || self
                .deck_read
                .as_ref()
                .is_some_and(|(_, job)| job.is_finished())
            || self
                .home_job
                .as_ref()
                .is_some_and(|(_, job)| job.is_finished())
            || self.cancel_jobs.values().any(|job| job.is_finished())
            || self.inbox_jobs.ready()
            || self.authentication.ready()
            || self
                .clipboard_job
                .as_ref()
                .is_some_and(ClipboardJob::is_finished)
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

    fn root_navigation_allowed(&self, state: &TuiState) -> bool {
        let root = self.child_parent.as_deref().unwrap_or(state);
        let read_only = if self.child_parent.is_some() {
            self.child_parent_read_only
        } else {
            self.read_only
        };
        !read_only && root.parent_id.is_none() && root.linked_child().is_none()
    }

    fn departure_ready(&self, state: &TuiState) -> Result<(), String> {
        // Every retained view keeps its receipt. An accepted turn, unlike
        // its admission receipt, remains owned by the native session runner.
        if std::iter::once(state)
            .chain(self.tabs.iter().flatten())
            .chain(self.home.iter())
            .chain(self.child_parent.as_deref())
            .chain(self.child_views.values())
            .any(|view| self.owns_receipt(view))
        {
            return Err("submission or model selection pending; session switch refused".into());
        }
        if self.conversation_job.is_some()
            || self.conversation_recovery.is_some()
            || self.recovery_job.is_some()
            || self.reload_pending()
            || self.navigation_job.is_some()
            || self.compaction_job.is_some()
            || self.authentication.pending()
            || self.authentication.deadline().is_some()
            || state.auth_request().is_some()
        {
            return Err("session transition pending".into());
        }
        Ok(())
    }

    fn target_ready(&self, state: &TuiState, index: usize) -> Result<(), String> {
        let view = if self.active_tab == Some(index) {
            self.child_parent.as_deref().unwrap_or(state)
        } else if index == self.tabs.len() {
            if self.active_tab.is_none() {
                state
            } else {
                self.home.as_ref().ok_or("tab unavailable")?
            }
        } else {
            self.tabs
                .get(index)
                .and_then(Option::as_ref)
                .ok_or("tab unavailable")?
        };
        if self.owns_receipt(view) || view.auth_request().is_some() {
            return Err("submission or model selection pending; target close refused".into());
        }
        Ok(())
    }

    fn owns_receipt(&self, view: &TuiState) -> bool {
        view.has_pending_submission()
            || view.has_pending_model_commit()
            || view.user_shell_admission_session().is_some()
            || self.inbox_jobs.owns(view)
            || self.picker.owns(view)
            || self.conversation_owns(view)
            || self.terminals.owns(view)
            || self.controls.owns(view)
            || self.work.owns(view)
            || self.catalogs.owns(view)
            || self.resources.owns(view)
            || self.authentication_origin == Some(view.view_identity().0)
                && self.authentication.has_surface()
            || self
                .cancel_jobs
                .keys()
                .any(|(session, _, _)| view.tab_route_session() == Some(session))
            || self.compaction_job.is_some()
                && self
                    .compaction_origin
                    .as_ref()
                    .is_some_and(|route| route.owns(view))
    }

    fn snapshot(&self, state: &TuiState) -> TabDeckSnapshot {
        let state = self.child_parent.as_deref().unwrap_or(state);
        let sessions = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, parked)| {
                let view = if self.active_tab == Some(index) {
                    state
                } else {
                    parked.as_ref().expect("parked tab")
                };
                view.attached_session()
                    .filter(|id| !self.closed_owners.contains_key(*id))
                    .cloned()
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
                    .filter_map(|(index, parked)| {
                        let view = if self.active_tab == Some(index) {
                            state
                        } else {
                            parked.as_ref().expect("parked tab")
                        };
                        view.attached_session()
                            .filter(|id| !self.closed_owners.contains_key(*id))
                            .map(|_| view.new_session_tab)
                    })
                    .collect::<Vec<_>>();
                if flags.iter().any(|value| *value) {
                    flags
                } else {
                    Vec::new()
                }
            },
            active: self.active_tab.and_then(|_| {
                state
                    .attached_session()
                    .filter(|id| !self.closed_owners.contains_key(*id))
                    .cloned()
            }),
            rail_width: state.tab_rail_preference(),
        }
    }

    /// Membership belongs to the acknowledged authority, not to whether the
    /// submit reply has reached a provisional view yet. Structural operations
    /// change only their explicit stable target in this captured deck.
    fn structural_snapshot(&self, state: &TuiState) -> TabDeckSnapshot {
        let visible = self.snapshot(state);
        let mut snapshot = self
            .authoritative_deck
            .as_ref()
            .filter(|saved| saved.location == visible.location)
            .cloned()
            .unwrap_or_else(|| visible.clone());
        snapshot.revision = self.revision.clone();
        snapshot.active = visible.active.filter(|id| snapshot.sessions.contains(id));
        snapshot.rail_width = visible.rail_width;
        snapshot
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
            Ok(saved)
                if saved.location == requested.location
                    && saved.revision.is_some()
                    && saved.rail_width == requested.rail_width =>
            {
                self.revision = saved.revision.clone();
                self.authoritative_deck = Some(saved);
                Ok(())
            }
            Ok(_) => Err(CoreError::TabDeckStorage),
            Err(error) => {
                if app.shares_workspace() && error == CoreError::TabDeckConflict {
                    self.shared_refresh = true;
                }
                Err(error)
            }
        }
    }

    fn sync_tabs(&mut self, state: &mut TuiState) {
        let retained: std::collections::HashSet<_> = read_jobs::views(state, self)
            .map(|view| view.view_identity().0)
            .collect();
        self.unresolved_homes.retain(|id| retained.contains(id));
        self.home_commands
            .retain(|home, (source, _)| retained.contains(home) && retained.contains(source));
        if self
            .home_pending
            .as_ref()
            .is_some_and(|(identity, _)| !retained.contains(&identity.0))
        {
            self.home_pending = None;
        }
        self.fresh_membership = read_jobs::views(state, self)
            .filter(|view| view.has_provisional_tab_route())
            .filter_map(|view| view.tab_route_session().cloned())
            .collect();
        if let Some(open) = self.sidebar_mcp_open {
            state.apply_sidebar_mcp_open(open);
        }
        state.set_service_feedback_visible(true);
        for parked in self.tabs.iter_mut().flatten() {
            parked.set_service_feedback_visible(false);
        }
        if let Some(home) = self.home.as_mut() {
            home.set_service_feedback_visible(false);
        }
        let showing_child = self.child_parent.is_some();
        state.set_root_navigation(self.root_navigation_allowed(state));
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
        // Bare Home has no tab. A locally enqueued fresh request reserves its
        // provisional slot before the original receipt can attach the session.
        if !showing_child && self.active_tab.is_none() && tab_route_session(state).is_some() {
            if state.has_provisional_tab_route() {
                self.fresh_membership
                    .insert(tab_route_session(state).expect("promoted route").clone());
            }
            debug_assert!(self.tabs.len() < MAX_TABS);
            state.new_session_tab = true;
            self.active_tab = Some(self.tabs.len());
            self.tabs.push(None);
            self.tab_cards_before.push(self.cards_before);
            self.navigation_revision = self.navigation_revision.wrapping_add(1);
            self.selection_dirty = true;
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
                    unread: view
                        .attached_session()
                        .and_then(|id| self.activity.unread(id)),
                    complete: view
                        .attached_session()
                        .and_then(|id| self.activity.unread(id))
                        == Some(oc_tui::app::TabUnread::Activity),
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
                    busy: view.is_busy()
                        || view
                            .attached_session()
                            .is_some_and(|id| self.activity.busy(id)),
                    attention: if state.tab_attention.contains(&index) {
                        Some(oc_tui::app::TabAttention::Permission)
                    } else {
                        view.questions
                            .active()
                            .map(|_| oc_tui::app::TabAttention::Question)
                    },
                    ..TabPresentation::new(
                        tab_route_session(view)
                            .expect("real or provisional tab has route")
                            .clone(),
                    )
                }
            })
            .collect();
        let active = self.active_tab.unwrap_or(self.tabs.len());
        let can_add =
            self.root_navigation_allowed(state) && (self.home.is_some() || self.can_open_session());
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

    async fn move_tab(
        &mut self,
        app: &CoreApp,
        state: &TuiState,
        session: SessionId,
        index: usize,
    ) -> Result<usize, String> {
        if !self.root_navigation_allowed(state) {
            return Err("child session: read-only history; saved tabs are unchanged".into());
        }
        let before = self.structural_snapshot(state);
        if self.location.is_some() && state.chrome.location != self.location {
            return Err(CoreError::LocationChanged.to_string());
        }
        let from = before
            .sessions
            .iter()
            .position(|id| id == &session)
            .ok_or("tab unavailable")?;
        let index = index.min(before.sessions.len() - 1);
        let mut requested = before.clone();
        let moved = requested.sessions.remove(from);
        requested.sessions.insert(index, moved);
        if !requested.new_session_titles.is_empty() {
            let flag = requested.new_session_titles.remove(from);
            requested.new_session_titles.insert(index, flag);
        }
        requested.active = Some(session);
        if requested != before {
            let order = requested.sessions.clone();
            let session = requested.active.clone().expect("moving real root");
            deck_jobs::start(
                app,
                self,
                requested,
                deck_jobs::Effect::Move {
                    session,
                    order,
                    navigation: self.navigation_revision,
                },
            )?;
        }
        Ok(index)
    }

    fn activate_with_hover(
        &mut self,
        state: &mut TuiState,
        index: usize,
        preserve_pointer: bool,
    ) -> Result<(), String> {
        if !self.root_navigation_allowed(state) {
            return Err("child session: read-only history; saved tabs are unchanged".into());
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        child_controls::return_root(state, self);
        if self.active_tab == Some(index) {
            self.navigation_revision = self.navigation_revision.wrapping_add(1);
            self.selection_dirty = true;
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
        state.resume_retained_view();
        if let Some(old) = self.active_tab {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        } else {
            self.home = Some(previous);
        }
        self.active_tab = Some(index);
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.selection_dirty = true;
        self.cards_before = self.tab_cards_before[index];
        self.dcp_seen = false;
        self.sync_tabs(state);
        Ok(())
    }

    fn open_home(&mut self, state: &mut TuiState, mut next: TuiState) {
        next.inherit_committed_model_choice(state);
        next.approval_roots.clone_from(&state.approval_roots);
        next.sync_clipboard_mode_from(state);
        state.close_panel();
        next.take_tab_clocks_from(state);
        let previous = std::mem::replace(state, next);
        if let Some(old) = self.active_tab.take() {
            self.tabs[old] = Some(previous);
            self.tab_cards_before[old] = self.cards_before;
        }
        // An existing synthetic Home is replaced by a fresh `/new` Home.
        self.home = None;
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.selection_dirty = true;
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
        state.resume_retained_view();
        self.tab_cards_before[old] = self.cards_before;
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.selection_dirty = true;
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
        // OC2: closing a tab never cancels its run; only unresolved receipts
        // (their admission/commit outcome) keep a view open.
        self.target_ready(state, index)?;
        if index == self.tabs.len() {
            if self.tabs.is_empty() || self.active_tab.is_some() && self.home.is_none() {
                return Err("tab unavailable".into());
            }
            let identity = if self.active_tab.is_none() {
                state.view_identity()
            } else {
                self.home.as_ref().expect("parked Home").view_identity()
            };
            let mut candidate = self.structural_snapshot(state);
            if self.active_tab.is_none() {
                candidate.active = self
                    .tabs
                    .last()
                    .and_then(Option::as_ref)
                    .and_then(TuiState::attached_session)
                    .cloned();
            }
            return deck_jobs::start(
                app,
                self,
                candidate,
                deck_jobs::Effect::CloseHome { identity },
            );
        }
        if index >= self.tabs.len() {
            return Err("tab unavailable".into());
        }
        let view = if self.active_tab == Some(index) {
            &*state
        } else {
            self.tabs[index].as_ref().expect("parked target")
        };
        let session = tab_route_session(view).cloned().ok_or("tab unavailable")?;
        if view.attached_session().is_none() {
            // Failed/unknown fresh admission has no durable membership to
            // delete. Dismiss only this local owner, without replay or cancel.
            deck_jobs::close_local(app, state, self, index);
            return Ok(());
        }
        let before = self.structural_snapshot(state);
        let mut candidate = before.clone();
        let durable_index = candidate
            .sessions
            .iter()
            .position(|id| id == &session)
            .ok_or("tab unavailable")?;
        candidate.sessions.remove(durable_index);
        if !candidate.new_session_titles.is_empty() {
            candidate.new_session_titles.remove(durable_index);
            if !candidate.new_session_titles.iter().any(|value| *value) {
                candidate.new_session_titles.clear();
            }
        }
        candidate.active = if self.active_tab == Some(index) {
            candidate
                .sessions
                .get(durable_index.saturating_sub(1))
                .cloned()
        } else {
            candidate.active
        };
        deck_jobs::start_close(app, self, before, candidate, session)
    }

    fn reset_deck(&mut self) {
        self.authoritative_deck = None;
        self.navigation_revision = self.navigation_revision.wrapping_add(1);
        self.child_parent = None;
        self.child_parent_read_only = false;
        self.child_views.clear();
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
        .recovery_job
        .as_ref()
        .is_some_and(|job| job.is_finished())
    {
        let job = deck.recovery_job.take().expect("finished refresh");
        let prepared = job.await.unwrap_or_else(|_| {
            (
                Err("refresh interrupted".into()),
                Err("refresh interrupted".into()),
                Err("refresh interrupted".into()),
            )
        });
        let route = deck.recovery_route.take();
        let current = read_jobs::views_mut(state, deck).any(|view| {
            route
                .as_ref()
                .is_some_and(|route| route.projection_current(view))
        });
        let mut applied = false;
        if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
            route
                .as_ref()
                .is_some_and(|route| route.projection_current(view))
        }) {
            applied = apply_conversation_refresh(app, view, prepared, true).await;
        }
        if !current {
            let mut reads = std::mem::take(&mut deck.reads);
            if let Some(view) = read_jobs::views_mut(state, deck)
                .find(|view| route.as_ref().is_some_and(|route| route.owns(view)))
            {
                reads.request(view, true);
            }
            deck.reads = reads;
            deck.conversation_recovery = None;
        } else if applied {
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
            deck.recovery_route = read_jobs::Route::capture(state);
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
    let origin = deck.conversation_origin.take();
    match job
        .await
        .unwrap_or_else(|_| Err("conversation operation interrupted".into()))
    {
        Ok(ConversationResult::Changed(snapshot, refresh)) => {
            let mut success = false;
            if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
                origin
                    .as_ref()
                    .is_some_and(|(route, _)| route.projection_current(view))
                    && view.attached_session() == Some(&snapshot.session)
            }) {
                view.conversation_applied(&snapshot, &Default::default());
                success = apply_conversation_refresh(app, view, refresh, false).await;
            }
            if !success {
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
            let mut requested = deck.structural_snapshot(state);
            if !requested.sessions.contains(&snapshot.session) {
                requested.sessions.push(snapshot.session.clone());
                if !requested.new_session_titles.is_empty() {
                    requested.new_session_titles.push(false);
                }
            }
            let mut next = TuiState::new(app.clone(), snapshot.session.clone());
            next.restore_prompt_input(oc_core::prompt::PromptInput {
                text: snapshot.prompt,
                images: snapshot.images,
            });
            let refreshed = apply_conversation_refresh(app, &mut next, refresh, false).await;
            if origin
                .as_ref()
                .is_some_and(|(_, navigation)| *navigation != deck.navigation_revision)
            {
                deck.tabs.push(Some(next));
                deck.tab_cards_before.push(None);
                deck.selection_dirty = true;
                deck.sync_tabs(state);
                requested.active = deck.snapshot(state).active;
                if let Err(error) = deck_jobs::start(app, deck, requested, deck_jobs::Effect::Adopt)
                {
                    state.push_note(&error);
                }
                return;
            }
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
            deck.selection_dirty |= deck.snapshot(state) != before;
            requested.active = deck.snapshot(state).active;
            if let Err(error) = deck_jobs::start(app, deck, requested, deck_jobs::Effect::Adopt) {
                state.push_note(&error);
            }
            if !refreshed {
                deck.conversation_recovery = Some((
                    snapshot.session,
                    std::time::Instant::now() + std::time::Duration::from_secs(1),
                ));
            }
            deck.sync_tabs(state);
        }
        Err(error) => {
            if let Some(view) = read_jobs::views_mut(state, deck)
                .find(|view| origin.as_ref().is_some_and(|(route, _)| route.owns(view)))
            {
                view.push_transient_note(&error, NoteVariant::Error);
            }
        }
    }
}

async fn refresh_approvals(app: &CoreApp, state: &mut TuiState) -> Result<(), String> {
    refresh_approvals_in_views(app, state, None).await
}

async fn refresh_approvals_in_views(
    app: &CoreApp,
    state: &mut TuiState,
    unadmitted_shell: Option<&SessionId>,
) -> Result<(), String> {
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
        // A fresh Shell Ask precedes durable session creation. Its existing
        // Home receipt owns visibility; unrelated parked roots cannot use the
        // not-yet-created candidate as a history/ancestry source.
        if Some(&current) == unadmitted_shell {
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
                .transcript_page(current.clone(), None, None, 1)
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
                .transcript_page(current.clone(), None, None, 1)
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
    if app.shares_workspace() {
        loop_state.shared_refresh = true;
        loop_state.shared_gap = true;
        shared::sync(app, &mut state, &mut loop_state).await?;
    }
    loop_state.sidebar_mcp_open = Some(match app.sidebar_mcp_open(None).await {
        Ok(open) => open,
        Err(error) => return startup_failure(&mut terminal, startup_query(error)).map(|_| 1),
    });
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
    // Only the tab strip animated since the last frame: it alone may repaint.
    let mut tabs_dirty = false;
    let mut first_paint = true;
    let mut painted_cursor_color = ratatui::style::Color::Reset;
    let mut pacing = display_rate::Pacing::new(Instant::now());
    // Paint no faster than the display refreshes; detected once, 60 Hz until known.
    let mut frame_interval = display_rate::interval(display_rate::DEFAULT_HZ);
    // A change caused by local input may paint at half that spacing.
    let mut input_dirty = false;
    let mut rate_probe = Some(display_rate::spawn_probe());
    let mut woke_by_completion = false;
    let idle_release = idle_release::IdleRelease::spawn();
    let mut worker_ready = None;
    let mut input_pending = std::collections::VecDeque::new();

    loop {
        // Sample before polling any lane: a completion after this point wakes
        // the wait below immediately instead of being missed.
        let wake_seen = oc_core::wake::generation();
        if rate_probe.as_ref().is_some_and(|probe| probe.is_finished()) {
            let probe = rate_probe.take().expect("finished display probe");
            if let Ok(Some(hz)) = probe.await {
                frame_interval = display_rate::interval(hz);
                state.set_animation_frame(frame_interval);
                if let Some(metrics) = &mut frame_metrics {
                    metrics.display_hz = Some(hz);
                }
            }
        }
        let had_pending = (
            state.has_pending_submission(),
            state.has_pending_model_commit(),
        );
        state.questions.share_drafts(&loop_state.questions);
        let job_lanes = loop_state.job_lanes();
        let job_ready = loop_state.ready_job(&state);
        let (changed, tabs_changed) = state.tick_ui_parts(Instant::now());
        dirty |= changed;
        tabs_dirty |= tabs_changed;
        let active = state.view_identity();
        for view in read_jobs::views(&state, &loop_state) {
            if view.view_identity() != active {
                view.poll_transcript_projection();
            }
        }
        loop_state.retire_parked_auth();
        dirty |= sync_authentication(app, &mut state, &mut loop_state).await;
        // Receipts answer local actions (submit, history, attach): their
        // frame belongs to the input's latency, not to worker throughput.
        let receipts = poll_and_sync(app, &mut state, &mut loop_state).await;
        dirty |= receipts;
        input_dirty |= receipts;
        if app.shares_workspace()
            && loop_state
                .activity
                .needs_workspace(state.attached_session())
        {
            loop_state.shared_refresh = true;
        }
        if app.shares_workspace()
            && (loop_state.shared_refresh || loop_state.workspace_job.is_some())
        {
            dirty |= shared::sync(app, &mut state, &mut loop_state).await?;
        }
        if app.shares_workspace()
            && loop_state
                .activity
                .sync(app, state.attached_session().cloned())
                .await
        {
            loop_state.sync_tabs(&mut state);
            dirty = true;
        }
        dirty |= sync_controller_jobs(app, &mut state, &mut loop_state).await;
        if state.terminals_need_refresh() {
            loop_state.terminals.request(&state);
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
                loop_state.reads.request(&state, false);
            }
            loop_state.controls.request();
            loop_state.approvals_checked =
                Some((state.attached_session().cloned(), Instant::now()));
        }
        dirty |= sync_mention(app, &mut state, &mut loop_state).await;
        finish_conversation(app, &mut state, &mut loop_state).await;
        finish_compaction_admission(&mut state, &mut loop_state).await;
        dirty |= sync_clipboard(&mut state, &mut loop_state).await;
        dirty |= job_ready
            || had_pending
                != (
                    state.has_pending_submission(),
                    state.has_pending_model_commit(),
                );
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
        dirty |= state.tick_delegations(Instant::now());
        dirty |= state.tick_tool_spinners(Instant::now());
        if (dirty || tabs_dirty) && Instant::now() >= pacing.due(input_dirty) {
            let frame_start = Instant::now();
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
                loop_state.terminals.resize(
                    &state,
                    ratatui::layout::Rect::new(0, 0, size.width, size.height),
                );
            }
            let color = cursor_color(&state);
            if color != painted_cursor_color {
                set_cursor_color(terminal.backend_mut(), color)
                    .map_err(|e| format!("cursor color: {e}"))?;
                painted_cursor_color = color;
            }
            // A tab-only change repaints just the strip, outside Ratatui's
            // full-frame diff; anything else (or a refused strip) is a full frame.
            let strip = (!dirty && tabs_dirty && !first_paint)
                .then(|| draw_tab_strip(&mut terminal, &state, frame_start))
                .transpose()
                .map_err(|e| format!("draw tab strip: {e}"))?;
            if strip != Some(true) {
                let painted = terminal
                    .draw(|frame| render_frame(frame, &state))
                    .map_err(|e| format!("draw: {e}"))?;
                if let Some(metrics) = &mut frame_metrics {
                    if metrics.previous.as_ref() != Some(painted.buffer) {
                        metrics.changed_frames += 1;
                    }
                    metrics.previous = Some(painted.buffer.clone());
                }
            }
            if let Some(metrics) = &mut frame_metrics {
                match strip {
                    Some(true) => metrics.strip_frames += 1,
                    Some(false) => metrics.strip_refused += 1,
                    None => {}
                }
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
            tabs_dirty = false;
            pacing.painted(frame_start, frame_interval, input_dirty);
            input_dirty = false;
            if loop_state.reload_pending() {
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
                input_dirty = true;
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
            let deferred = loop_state
                .workspace_job
                .is_none()
                .then(|| loop_state.deferred_events.pop_front())
                .flatten();
            let (event, event_revision) = if let Some((event, revision)) = deferred {
                (event, Some(revision))
            } else {
                let event = match worker_ready
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| try_worker_event(&mut rx, frame_metrics.as_mut()))
                {
                    Ok(event) => event,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                        if app.shares_workspace() {
                            loop_state.shared_refresh = true;
                            loop_state.shared_gap = true;
                            dirty = true;
                        }
                        break;
                    }
                    Err(_) => break,
                };
                let (event, event_revision) = match event {
                    CoreEvent::Projection { revision, event } => {
                        if revision <= loop_state.projection_revision {
                            continue;
                        }
                        if loop_state.projection_revision != 0
                            && revision != loop_state.projection_revision + 1
                        {
                            loop_state.shared_gap = true;
                            loop_state.shared_refresh = true;
                        }
                        loop_state.projection_revision = revision;
                        (*event, Some(revision))
                    }
                    event => (event, None),
                };
                loop_state.activity.observe(&event);
                if let Some(revision) = event_revision
                    && loop_state.workspace_job.is_some()
                    && shared::turn_event(&event)
                {
                    if loop_state.deferred_events.len() < MAX_DEFERRED_EVENTS {
                        loop_state.deferred_events.push_back((event, revision));
                    } else {
                        loop_state.deferred_events.clear();
                        loop_state.shared_gap = true;
                        loop_state.shared_refresh = true;
                    }
                    continue;
                }
                (event, event_revision)
            };
            match &event {
                CoreEvent::SessionViewed { .. } => {
                    loop_state.sync_tabs(&mut state);
                    dirty = true;
                    continue;
                }
                CoreEvent::WorkspaceChanged { location } => {
                    if loop_state.location.as_ref() == Some(location) {
                        loop_state.shared_refresh = true;
                    }
                    continue;
                }
                CoreEvent::ProjectionGap => {
                    loop_state.shared_gap = true;
                    loop_state.shared_refresh = true;
                    continue;
                }
                CoreEvent::SessionModelSelected { .. }
                | CoreEvent::SelectionNoticeCommitted { .. }
                    if app.shares_workspace() =>
                {
                    // The committed choice is common; only an unmatched local
                    // picker draft is private. Reconcile its receipt first.
                    loop_state.shared_refresh = true;
                }
                CoreEvent::OwnerDisconnected => {
                    report_owner(
                        &mut state,
                        &mut loop_state,
                        OWNER_LOST,
                        NoteVariant::Warning,
                    );
                    dirty = true;
                    continue;
                }
                CoreEvent::OwnerReattached => {
                    reattached(app, &mut state, &mut loop_state).await;
                    dirty = true;
                    continue;
                }
                CoreEvent::OwnerUnrecoverable { reason } => {
                    let note = format!(
                        "Runtime connection cannot be restored: {reason}. Restart oc2; input was not resent."
                    );
                    report_owner(&mut state, &mut loop_state, &note, NoteVariant::Error);
                    dirty = true;
                    continue;
                }
                _ => {}
            }
            dirty = true;
            if let Some(metrics) = &mut frame_metrics {
                metrics.worker_events += 1;
            }
            let receipts = poll_and_sync(app, &mut state, &mut loop_state).await;
            dirty |= receipts;
            input_dirty |= receipts;
            if let CoreEvent::Compaction(snapshot) = event {
                apply_compaction_to_view(&mut state, &mut loop_state, snapshot);
            } else if let CoreEvent::McpChanged(snapshot) = event {
                apply_mcp_to_views(&mut state, &mut loop_state, snapshot);
            } else if matches!(event, CoreEvent::ProviderChanged) {
                loop_state.shared_refresh |= app.shares_workspace();
                refresh_provider_views(app, &mut state, &mut loop_state).await;
                loop_state.reads.request(&state, false);
            } else if matches!(
                &event,
                CoreEvent::PermissionAsked(_)
                    | CoreEvent::PermissionResolved { .. }
                    | CoreEvent::QuestionAsked(_)
                    | CoreEvent::QuestionResolved { .. }
            ) {
                // Fresh direct-user Shell Ask exists before a root history does.
                // Control events must reach its receipt owner even on Home.
                loop_state.controls.request();
                loop_state.approvals_checked = None;
            } else if let Some(current) = state
                .attached_session()
                .cloned()
                .or_else(|| child_controls::event_owner(&event).cloned())
            {
                handle_worker_event_at(
                    app,
                    &mut state,
                    &mut loop_state,
                    &current,
                    event,
                    event_revision,
                    false,
                )
                .await?;
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
            loop_state.reads.request(&state, false);
            loop_state.dcp_seen = true;
        } else if *state.panel() != TuiPanel::Dcp {
            loop_state.dcp_seen = false;
        }
        let paint_due = pacing.due(input_dirty);
        if (dirty || tabs_dirty) && Instant::now() >= paint_due {
            continue;
        }
        let mut deadline = state.next_ui_deadline();
        if let Some(at) = loop_state.activity.deadline() {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
        }
        if let Some(at) = loop_state.authentication.deadline() {
            deadline = Some(deadline.map_or(at, |d| d.min(at)));
        }
        if dirty || tabs_dirty {
            deadline = Some(deadline.map_or(paint_due, |at| at.min(paint_due)));
        }
        if loop_state.workspace_job.is_none() && !loop_state.deferred_events.is_empty() {
            deadline = Some(Instant::now());
        }
        if std::iter::once(&state)
            .chain(loop_state.tabs.iter().flatten())
            .chain(loop_state.home.iter())
            .chain(loop_state.child_parent.as_deref())
            .chain(loop_state.child_views.values())
            .any(|view| {
                view.has_pending_submission()
                    || view.has_pending_model_commit()
                    || view.has_pending_prompt_history()
                    || view.user_shell_admission_session().is_some()
                    || view.has_pending_transcript_projection()
            })
            || loop_state.has_jobs()
        {
            let check = Instant::now()
                + if woke_by_completion {
                    COMPLETION_RECHECK
                } else {
                    COMPLETION_FALLBACK
                };
            deadline = Some(deadline.map_or(check, |at| at.min(check)));
        }
        woke_by_completion = false;
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
                idle_release.touch();
            }
            result = rx.recv() => {
                idle_release.touch();
                match result {
                    Ok(event) => worker_ready = Some(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        loop_state.shared_refresh = true;
                        loop_state.shared_gap = true;
                        if let Some(metrics) = &mut frame_metrics {
                            metrics.worker_event_queue_lagged += skipped;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err("application event channel closed".into()),
                }
            }
            () = oc_core::wake::changed(wake_seen) => {
                woke_by_completion = true;
                idle_release.touch();
                if let Some(metrics) = &mut frame_metrics { metrics.completion_wakeups += 1; }
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
    // Stop all views first, then explicitly join without retaining any TUI
    // RefCell borrow across await. Parked roots/Home/children own work too.
    for view in read_jobs::views(&state, &loop_state) {
        view.cancel_transcript_projection();
    }
    for view in read_jobs::views(&state, &loop_state) {
        view.finish_transcript_projection().await;
    }
    if let Some(job) = loop_state.clipboard_job.take() {
        job.cancel().await;
    }
    if let Some((session, job)) = loop_state.title_job.take() {
        if app.shares_workspace() {
            job.abort();
        } else {
            let _ = app.cancel_title(session).await;
            let _ = job.await;
        }
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
        metrics.memory_releases = idle_release.releases();
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
            oc_core::wake::spawn(async move { owner.file_suggestions(query, MENTION_LIMIT).await }),
        ));
    }
    changed
}

fn tab_route_session(view: &TuiState) -> Option<&SessionId> {
    view.tab_route_session()
}

async fn sync_home(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut changed = false;
    if deck
        .home_job
        .as_ref()
        .is_some_and(|(_, job)| job.is_finished())
    {
        let (identity, job) = deck.home_job.take().expect("finished Home catalog");
        let result = job.await.unwrap_or(Err(CoreError::Shutdown));
        let mut resolved = false;
        // Reactivating the same retained Home only changes its read/filesystem
        // generation. It does not withdraw this already-issued selection ACK.
        if let Some(view) = read_jobs::views_mut(state, deck).find(|view| {
            view.view_identity().0 == identity.0 && view.home && view.attached_session().is_none()
        }) {
            match result {
                Ok(catalog) if catalog.chrome.location == view.chrome.location
                    && catalog.chrome.selection_generation >= view.chrome.selection_generation => {
                    view.apply_catalog(catalog);
                    resolved = true;
                }
                _ => view.push_note("Home selection unavailable or changed; input retained. Open New to resolve it again."),
            }
            changed = true;
        }
        if resolved {
            deck.unresolved_homes.remove(&identity.0);
            if let Some((source, command)) = deck.home_commands.remove(&identity.0)
                && let Some(view) = read_jobs::views_mut(state, deck)
                    .find(|view| view.view_identity().0 == source && view.input() == command)
            {
                view.accept_intent();
            }
        }
    }
    if deck.home_job.is_none()
        && deck.navigation_job.is_none()
        && !deck.scope_uncertain
        && let Some((identity, action)) = deck.home_pending.take()
    {
        let owner = app.clone();
        deck.home_job = Some((
            identity,
            oc_core::wake::spawn(async move { owner.home_selection(action).await }),
        ));
    }
    changed
}

async fn sync_authentication(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    if deck.authentication_origin.is_none() {
        if deck.navigation_job.is_some() {
            return false;
        }
        let origin = read_jobs::views(state, deck)
            .find(|view| view.auth_request().is_some())
            .map(|view| view.view_identity().0);
        deck.authentication_origin = origin;
    }
    let Some(origin) = deck.authentication_origin else {
        return false;
    };
    let mut authentication = std::mem::take(&mut deck.authentication);
    let changed = if let Some(view) =
        read_jobs::views_mut(state, deck).find(|view| view.view_identity().0 == origin)
    {
        authentication.sync(app, view).await
    } else {
        false
    };
    if !authentication.has_surface() {
        deck.authentication_origin = None;
    }
    deck.authentication = authentication;
    changed
}

async fn sync_controller_jobs(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut changed = sync_home(app, state, deck).await;
    let finished: Vec<_> = deck
        .cancel_jobs
        .iter()
        .filter(|(_, job)| job.is_finished())
        .map(|(id, _)| id.clone())
        .collect();
    for key in finished {
        let result = deck
            .cancel_jobs
            .remove(&key)
            .expect("finished captured cancel")
            .await
            .unwrap_or(Err(CoreError::Shutdown));
        if let Err(error) = result
            && error != CoreError::TurnNotActive
            && let Some(view) = read_jobs::views_mut(state, deck)
                .find(|view| tab_route_session(view) == Some(&key.0))
        {
            view.push_note(&format!("Cancellation: {error}"));
            changed = true;
        }
    }
    changed |= read_jobs::sync(app, state, deck).await;
    changed |= notice_jobs::sync(app, state, deck).await;
    changed |= child_jobs::sync(app, state, deck).await;
    changed |= work_controls::sync(state, deck).await;
    changed |= catalog_jobs::sync(app, state, deck).await;
    changed |= history_jobs::sync(app, state, deck).await;
    changed |= resource_jobs::sync(state, deck).await;
    changed |= reload_jobs::sync(app, state, deck).await;
    changed |= navigation_jobs::sync(app, state, deck).await;
    changed |= picker_jobs::sync(app, state, deck).await;
    changed |= inbox_jobs::sync(app, state, deck).await;
    changed |= control_jobs::sync(app, state, deck).await;
    changed |= terminal_controls::sync(app, state, deck).await;
    changed |= deck_jobs::sync(app, state, deck).await;
    changed |= finish_provider_feedback(state, deck);
    if changed {
        deck.sync_tabs(state);
    }
    changed
}

async fn poll_and_sync(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) -> bool {
    let mut intents = Vec::new();
    let mut changed = false;
    let slots = deck.tabs.len();
    for view in std::iter::once(&mut *state)
        .chain(deck.tabs.iter_mut().flatten())
        .chain(deck.home.iter_mut())
        .chain(deck.child_parent.as_deref_mut())
        .chain(deck.child_views.values_mut())
    {
        changed |= view.poll_prompt_history();
        let attached = view.attached_session().cloned();
        let pending = (
            view.has_pending_submission(),
            view.has_pending_model_commit(),
            view.has_occupied_submission(),
        );
        if let Some(intent) = view.poll_submission() {
            intents.push((view.view_identity(), intent));
        }
        if attached != view.attached_session().cloned() {
            deck.shared_refresh |= app.shares_workspace();
            deck.selection_dirty = true;
            deck.controls.request();
            deck.reads.request(view, true);
            changed = true;
        }
        changed |= pending
            != (
                view.has_pending_submission(),
                view.has_pending_model_commit(),
                view.has_occupied_submission(),
            );
    }
    // The source can be parked while agent preparation resolves. Execute its
    // one deferred intent against that exact owner, never against current focus.
    for (identity, intent) in intents {
        if state.view_identity() == identity {
            if let Err(error) = Box::pin(apply_intent_with_origin(
                app, state, deck, intent, false, false,
            ))
            .await
            {
                state.push_note(&error);
            }
        } else if let Some(index) = deck.tabs.iter().position(|view| {
            view.as_ref()
                .is_some_and(|view| view.view_identity() == identity)
        }) {
            let mut view = deck.tabs[index].take().expect("retained receipt owner");
            if let Err(error) = Box::pin(apply_intent_with_origin(
                app, &mut view, deck, intent, false, false,
            ))
            .await
            {
                view.push_note(&error);
            }
            deck.tabs[index] = Some(view);
        } else if deck
            .home
            .as_ref()
            .is_some_and(|view| view.view_identity() == identity)
        {
            let mut view = deck.home.take().expect("retained Home receipt owner");
            if let Err(error) = Box::pin(apply_intent_with_origin(
                app, &mut view, deck, intent, false, false,
            ))
            .await
            {
                view.push_note(&error);
            }
            deck.home = Some(view);
        }
    }
    deck.sync_tabs(state);
    changed || slots != deck.tabs.len()
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
    for view in deck
        .tabs
        .iter_mut()
        .flatten()
        .chain(deck.home.iter_mut())
        .chain(deck.child_parent.as_deref_mut())
        .chain(deck.child_views.values_mut())
    {
        view.reconcile_fresh_quit()
            .await
            .map_err(|error| format!("quit cancellation: {error}"))?;
    }
    if app.shares_workspace() {
        if read_jobs::views(state, deck).any(|view| {
            view.attached_session().is_none()
                && (view.has_pending_submission() || view.user_shell_admission_session().is_some())
        }) {
            // A pre-acceptance Ask can hold the native Location deck gate.
            // Its authority-owned admission needs this viewer's lease release;
            // waiting for workspace/selection behind it would deadlock Quit.
            return if deck.home_pending.is_some()
                || deck.home_job.is_some()
                || deck.deck_job.is_some()
                || deck.workspace_job.is_some()
                || deck.shared_refresh
                || deck.selection_dirty
            {
                Err("quit tab selection unavailable while fresh admission awaits acknowledgement; release viewer lease".into())
            } else {
                Ok(())
            };
        }
        // Quit may arrive in the same input batch as New/activation, before the
        // next frame starts its Home query or selection CAS. Settle those known
        // lanes and persist only focus/geometry against authoritative membership.
        // A refused/unknown CAS stays blocked: shutdown must never retry it.
        let mut woke = false;
        loop {
            let seen = oc_core::wake::generation();
            if deck.selection_blocked {
                return Err("quit tab selection unavailable; saved tabs were not replayed".into());
            }
            if deck.navigation_job.is_some() || deck.moved_pending.is_some() || deck.scope_uncertain
            {
                return if deck.home_pending.is_some()
                    || deck.home_job.is_some()
                    || deck.selection_dirty
                {
                    Err(
                        "quit tab selection unavailable while Location navigation is unresolved"
                            .into(),
                    )
                } else {
                    Ok(())
                };
            }
            sync_home(app, state, deck).await;
            if deck.workspace_job.is_some() || deck.shared_refresh {
                shared::sync(app, state, deck).await?;
            }
            deck.sync_tabs(state);
            deck_jobs::sync(app, state, deck).await;
            if deck.selection_blocked {
                return Err("quit tab selection unavailable; saved tabs were not replayed".into());
            }
            if deck.home_job.is_none()
                && deck.home_pending.is_none()
                && deck.deck_job.is_none()
                && deck.workspace_job.is_none()
                && !deck.shared_refresh
            {
                if deck.selection_dirty && !deck.save_disabled {
                    return Err(
                        "quit tab selection unavailable; saved tabs were not replayed".into(),
                    );
                }
                break;
            }
            // Wait for the next lane completion, not a timer; see COMPLETION_RECHECK.
            let wait = if woke {
                COMPLETION_RECHECK
            } else {
                COMPLETION_FALLBACK
            };
            woke = tokio::time::timeout(wait, oc_core::wake::changed(seen))
                .await
                .is_ok();
        }
        return Ok(());
    }
    // `handle_key` can consume an acceptance just before setting Quit, after
    // the frame's final poll_and_sync. Only a newly attached Home needs a
    // save; an already-durable tab never waits or writes again.
    deck.sync_tabs(state);
    let visible = deck.snapshot(state);
    if visible.sessions.iter().any(|id| {
        deck.fresh_membership.contains(id)
            && deck
                .authoritative_deck
                .as_ref()
                .is_none_or(|saved| !saved.sessions.contains(id))
    }) {
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

async fn refresh_provider_views(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    let mut jobs = std::mem::take(&mut deck.catalogs);
    for view in read_jobs::views_mut(state, deck) {
        jobs.request(app, view, catalog_jobs::Request::Provider);
    }
    deck.catalogs = jobs;
    let _ = deck
        .resources
        .request(app, state, resource_jobs::Request::Mcp(None));
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
            details: None,
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
        .transcript_page(session.clone(), None, None, HISTORY_PAGE_LIMIT)
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
                    if restore_error_is_fatal(
                        &error,
                        explicit == Some(&id),
                        active.as_ref() == Some(&id),
                    ) =>
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

fn restore_error_is_fatal(error: &CoreError, explicit: bool, active: bool) -> bool {
    if explicit {
        return true;
    }
    match error {
        CoreError::Diagnostic(diagnostic)
            if !active
                && diagnostic.code == oc_core::queries::ServiceCode::InvalidStoredState
                && diagnostic.field == ["selection"] =>
        {
            false
        }
        CoreError::Diagnostic(_) | CoreError::TabDeckStorage => true,
        _ => false,
    }
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
        || stored.rail_width.is_some_and(|width| {
            !(oc_core::queries::TAB_RAIL_MIN_WIDTH..=oc_core::queries::TAB_RAIL_MAX_WIDTH)
                .contains(&width)
        })
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
    deck.authoritative_deck = Some(stored.clone());
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
    let prefer_home_when_space = explicit.is_none() && !app.shares_workspace();
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
    state
        .apply_tab_rail_width(stored.rail_width)
        .map_err(startup_query)?;
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
        .transcript_page(id.clone(), None, None, HISTORY_PAGE_LIMIT)
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
    if let Ok(rows) = app.execute_jobs(state.session().clone()).await {
        state.apply_execute_jobs(rows);
    }
    refresh_dcp_summaries(app, &mut state).await;
    shared::attach(app, &mut state)
        .await
        .map_err(CoreError::Application)?;
    Ok(state)
}

/// Publish a tab-strip-only change: render the strip into Ratatui's blank
/// current buffer, take its cells back out (leaving the buffer blank for the
/// next full frame) and write them through `FrameBackend::draw_region`.
/// `false` when the strip cannot be drawn alone; the caller draws a full frame.
fn draw_tab_strip<W: Write>(
    terminal: &mut Terminal<FrameBackend<W>>,
    state: &TuiState,
    now: Instant,
) -> std::io::Result<bool> {
    let cursor = terminal.backend().painted_cursor();
    if !oc_tui::shell::render_tab_strip_only(&mut terminal.get_frame(), state, now) {
        return Ok(false);
    }
    let Some(region) = oc_tui::shell::tab_strip_region(state) else {
        return Ok(false);
    };
    let buffer = terminal.current_buffer_mut();
    let region = region.intersection(buffer.area);
    let mut strip = ratatui::buffer::Buffer::empty(region);
    for at in region.positions() {
        strip[at] = std::mem::take(&mut buffer[at]);
    }
    terminal.backend_mut().draw_region(&strip, cursor)
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
    if matches!(&cev, CEvent::Key(_) | CEvent::Mouse(_) | CEvent::Paste(_)) {
        loop_state.interaction_revision = loop_state.interaction_revision.wrapping_add(1);
    }
    loop_state.activity.focus(&cev);
    if let CEvent::Resize(width, height) = cev {
        state.resize_mouse_position(ratatui::layout::Rect::new(0, 0, width, height));
        loop_state
            .terminals
            .resize(state, ratatui::layout::Rect::new(0, 0, width, height));
        return Ok(());
    }
    if let CEvent::Key(key) = &cev
        && key.kind != crossterm::event::KeyEventKind::Release
        && key.code == crossterm::event::KeyCode::Char('d')
        && key.modifiers == crossterm::event::KeyModifiers::CONTROL
        && let Some(target) = loop_state
            .terminals
            .attaching_eof(state, loop_state.interaction_revision)
    {
        let session = target.session.clone();
        terminal_controls::apply(
            app,
            state,
            loop_state,
            session,
            oc_tui::terminal_view::TerminalIntent::Control(
                oc_core::queries::TerminalAction::Input {
                    target,
                    bytes: vec![4],
                },
            ),
        )
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
    if loop_state.reload_pending() {
        match event {
            Some(UiEvent::Key(KeyAction::Interrupt | KeyAction::Quit)) => {
                // Bypass modal/editor focus, but leave owner reload intact for
                // the post-terminal join and orderly application shutdown.
                state.handle_key(KeyAction::Quit).await;
            }
            Some(UiEvent::Key(KeyAction::Enter)) if state.input().trim() == "/quit" => {
                state.handle_key(KeyAction::Quit).await;
            }
            // A reload may run under a turn; its two-Esc interrupt stays live.
            Some(UiEvent::Key(KeyAction::Cancel)) if state.is_busy() => {
                let outcome = state.handle_key(KeyAction::Cancel).await;
                apply_outcome(app, state, loop_state, outcome, false).await;
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
            let composer_submit = submits
                && *state.panel() == TuiPanel::None
                && state.approvals.active().is_none()
                && state.questions.active().is_none();
            if *state.panel() == TuiPanel::Sessions
                && matches!(
                    action,
                    KeyAction::Enter | KeyAction::Rename | KeyAction::DeleteOrQuit
                )
            {
                match loop_state.picker.defer_action(
                    state,
                    loop_state.navigation_revision,
                    loop_state.interaction_revision,
                    false,
                    action.clone(),
                ) {
                    Ok(true) => return Ok(()),
                    Err(error) => {
                        state.push_note(&error);
                        return Ok(());
                    }
                    Ok(false) => {}
                }
            }
            sync_clipboard(state, loop_state).await;
            if action == KeyAction::Paste {
                start_clipboard(state, loop_state);
                return Ok(());
            }
            if submits && loop_state.clipboard_job.is_some() {
                state.push_note("clipboard paste pending; wait or edit to cancel");
                return Ok(());
            }
            if composer_submit
                && dispatch(state.input().trim()).is_none()
                && (loop_state
                    .deck_job
                    .as_ref()
                    .is_some_and(|job| job.closing(state))
                    || loop_state.conversation_owns(state)
                    || loop_state.catalogs.preparing(state)
                    || loop_state.navigation_job.is_some()
                    || loop_state.scope_uncertain)
            {
                state.push_note("tab close pending; input retained until acknowledgement");
                return Ok(());
            }
            if composer_submit
                && loop_state
                    .unresolved_homes
                    .contains(&state.view_identity().0)
                && dispatch(state.input().trim()).is_none()
            {
                state.push_note("Home selection loading; input retained");
                return Ok(());
            }
            if action == KeyAction::Cancel
                && *state.panel() == TuiPanel::None
                && state.input().trim() == "/rename"
                && let Some((session, _)) = &loop_state.title_job
                && state.attached_session() == Some(session)
            {
                loop_state.picker.cancel_title(app, session.clone());
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
                && dispatch(state.input().trim()).is_none_or(|a| {
                    a != CommandAction::Quit
                        && !a.is_terminal()
                        && !(loop_state.root_navigation_allowed(state)
                            && matches!(a, CommandAction::NewSession | CommandAction::OpenSessions))
                })
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
            if text.is_empty() && state.clipboard_target().is_some() {
                start_clipboard(state, loop_state);
                return Ok(());
            }
            let outcome = state.handle_paste(&text);
            apply_outcome(app, state, loop_state, outcome, false).await;
        }
        Some(UiEvent::Mouse(mouse)) => {
            loop_state.pointer_revision = loop_state.pointer_revision.wrapping_add(1);
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
                && !state.sidebar_wheel_hit(area, mouse.column, mouse.row)
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
                    apply_mouse_outcome(app, state, loop_state, outcome, area, mouse).await;
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
    mouse: crossterm::event::MouseEvent,
) {
    let (x, y) = (mouse.column, mouse.row);
    if let Some(note) = outcome.note {
        state.push_note(&note);
    }
    let Some(intent) = outcome.intent else { return };
    // A matched Sessions-option release owns a real pointer even though the
    // dialog deliberately blocks underlying transcript hover. Re-hit it only
    // after the picker owner accepts a same-Location route, never a cached hit.
    let picker_open = *state.panel() == TuiPanel::Sessions
        && matches!(intent, PanelIntent::SwitchSession { .. })
        && area.contains((x, y).into());
    let pointer =
        (state.mouse_position() == Some((x, y, area)) || picker_open).then_some((x, y, area));
    let close = match intent {
        PanelIntent::CloseTab { index }
            if pointer.is_some()
                && matches!(
                    mouse.kind,
                    MouseEventKind::Up(crossterm::event::MouseButton::Left)
                ) =>
        {
            state.mouse_close_snapshot(index)
        }
        _ => None,
    };
    let activate = matches!(
        intent,
        PanelIntent::ActivateTab { .. } | PanelIntent::MoveTab { .. } | PanelIntent::NewSession
    );
    match apply_intent_with_origin(app, state, loop_state, intent, false, pointer.is_some()).await {
        Ok(()) => {
            loop_state.sync_tabs(state);
            if let Some(snapshot) = close {
                if let Some(job) = loop_state.deck_job.as_mut() {
                    job.retain_close_pointer(loop_state.navigation_revision, snapshot);
                } else {
                    state.restore_mouse_close(snapshot);
                }
            } else if (activate || picker_open)
                && let Some(pointer) = pointer
            {
                if picker_open && let Some(job) = loop_state.navigation_job.as_mut() {
                    job.retain_pointer(loop_state.pointer_revision, pointer);
                } else {
                    state.restore_mouse_hover(pointer);
                }
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
    let rename = match &intent {
        PanelIntent::RenameSessionDirect { .. } => Some(true),
        PanelIntent::RenameSession { .. } | PanelIntent::RenameSelectedSession { .. } => {
            Some(false)
        }
        _ => None,
    };
    let keyboard_close_pointer = (matches!(intent, PanelIntent::CloseTab { .. })
        && *state.panel() == TuiPanel::None)
        .then(|| state.mouse_position())
        .flatten();
    // Keep the controller's large intent future out of each nested input poll
    // frame; do not depend on an enlarged executor/test thread stack.
    match Box::pin(apply_intent_with_origin(
        app, state, loop_state, intent, typed_new, false,
    ))
    .await
    {
        Ok(()) => {
            if let Some(pointer) = keyboard_close_pointer {
                state.restore_tab_hover_at(pointer);
            }
        }
        Err(message) => match rename {
            Some(true) => state
                .rename_session_direct_rejected(format!("session rename unavailable: {message}")),
            Some(false) => {
                state.rename_session_rejected(format!("session rename unavailable: {message}"))
            }
            None => state.apply_intent_error(message),
        },
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
    // Intent admission can retain a whole Home view across owner acknowledgements.
    // Keep that bounded frame on the heap, including direct controller callers.
    Box::pin(apply_owned_intent(
        app,
        state,
        loop_state,
        intent,
        typed_new,
        preserve_pointer,
    ))
    .await
}

async fn apply_owned_intent(
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
            | PanelIntent::LoadProviderConnections { .. }
            | PanelIntent::ProviderAccounts { action: None, .. }
    );
    let local_navigation = matches!(
        intent,
        PanelIntent::NewSession | PanelIntent::AddSessionTab | PanelIntent::ActivateTab { .. }
    );
    if loop_state
        .unresolved_homes
        .contains(&state.view_identity().0)
        && matches!(
            &intent,
            PanelIntent::SelectModel { .. }
                | PanelIntent::ChooseModel { .. }
                | PanelIntent::CycleVariant
                | PanelIntent::SelectAgent { .. }
        )
    {
        return Err("Home binding preparing; model and agent drafts unchanged".into());
    }
    let source_mutation = matches!(
        &intent,
        PanelIntent::Inbox { .. }
            | PanelIntent::CompactSession
            | PanelIntent::ChangeConversation { .. }
            | PanelIntent::ForkMessage { .. }
            | PanelIntent::RenameSession { .. }
            | PanelIntent::RenameSessionDirect { .. }
            | PanelIntent::RegenerateTitle
            | PanelIntent::SelectModel { .. }
            | PanelIntent::ChooseModel { .. }
            | PanelIntent::CycleVariant
            | PanelIntent::SelectAgent { .. }
            | PanelIntent::Compress { .. }
            | PanelIntent::Terminal { .. }
            | PanelIntent::SetSidebarMcpOpen { .. }
    );
    if source_mutation && !terminal_control {
        if state
            .tab_route_session()
            .is_some_and(|id| loop_state.closed_owners.contains_key(id))
            || loop_state
                .deck_job
                .as_ref()
                .is_some_and(|job| job.closing(state))
        {
            return Err("tab close pending; draft retained until acknowledgement".into());
        }
        if loop_state.conversation_owns(state) {
            return Err("conversation operation pending".into());
        }
        if loop_state.navigation_job.is_some() || loop_state.scope_uncertain {
            return Err("session navigation pending; draft retained".into());
        }
    }
    if loop_state.reload_pending() && !terminal_control && !auth_control {
        return Err("configuration reload pending".into());
    }
    let root_navigation = matches!(
        intent,
        PanelIntent::NewSession
            | PanelIntent::AddSessionTab
            | PanelIntent::SwitchSession { .. }
            | PanelIntent::ActivateTab { .. }
            | PanelIntent::MoveTab { .. }
    );
    if root_navigation && !local_navigation {
        loop_state.departure_ready(state)?;
    }
    if loop_state.read_only
        && !(root_navigation && loop_state.root_navigation_allowed(state))
        && matches!(
            intent,
            PanelIntent::NewSession
                | PanelIntent::AddSessionTab
                | PanelIntent::SwitchLocation { .. }
                | PanelIntent::SetPermissionMode { .. }
                | PanelIntent::SwitchSession { .. }
                | PanelIntent::CloseTab { .. }
                | PanelIntent::SetTabRailWidth { .. }
                | PanelIntent::ActivateTab { .. }
                | PanelIntent::MoveTab { .. }
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
        PanelIntent::CancelSession {
            session,
            turn,
            cancel_compaction,
        } => {
            let key = (session.clone(), turn.clone(), cancel_compaction);
            if !loop_state.cancel_jobs.contains_key(&key) {
                if loop_state.cancel_jobs.len() >= MAX_TABS + 17 {
                    return Err(
                        "cancellation admissions pending; retry after acknowledgement".into(),
                    );
                }
                let owner = app.clone();
                loop_state.cancel_jobs.insert(
                    key,
                    oc_core::wake::spawn(async move {
                        let compaction = if cancel_compaction {
                            owner.cancel_compaction(session.clone()).await
                        } else {
                            Ok(())
                        };
                        let cancellation = match turn {
                            Some(turn) => owner.cancel_turn(session, turn).await,
                            None => owner.cancel(session).await,
                        };
                        compaction
                            .or_else(|error| {
                                if error == CoreError::TurnNotActive {
                                    Ok(())
                                } else {
                                    Err(error)
                                }
                            })
                            .and(cancellation)
                    }),
                );
            }
        }
        PanelIntent::SetSidebarMcpOpen { open } => {
            loop_state
                .resources
                .request(app, state, resource_jobs::Request::Sidebar(open))?;
        }
        PanelIntent::SetTabRailWidth { width } => {
            if !(oc_core::queries::TAB_RAIL_MIN_WIDTH..=oc_core::queries::TAB_RAIL_MAX_WIDTH)
                .contains(&width)
                || state.chrome.location.as_deref() != loop_state.location.as_deref()
                || loop_state.location.is_none()
            {
                return Err(CoreError::InvalidTabDeck.to_string());
            }
            let mut requested = loop_state.structural_snapshot(state);
            requested.rail_width = Some(width);
            deck_jobs::start(app, loop_state, requested, deck_jobs::Effect::Width(width))?;
        }
        connection @ (PanelIntent::LoadProviderConnections { .. }
        | PanelIntent::LoadAuthMethods { .. }
        | PanelIntent::ProviderAccounts { .. }
        | PanelIntent::ProbeCustomProvider { .. }
        | PanelIntent::RetryCustomProvider { .. }
        | PanelIntent::SaveCustomProvider { .. }) => {
            if loop_state.navigation_job.is_some()
                || loop_state.scope_uncertain
                || loop_state
                    .authentication_origin
                    .is_some_and(|origin| origin != state.view_identity().0)
            {
                return Err(
                    "authorization target pending in another retained view; input retained".into(),
                );
            }
            loop_state.authentication.connect(app, state, connection)?;
            loop_state.authentication_origin = Some(state.view_identity().0);
        }
        PanelIntent::BeginAuthentication(request) => {
            if state.auth_request() == Some(&request) {
                sync_authentication(app, state, loop_state).await;
            }
        }
        PanelIntent::RefreshCustomProvider { binding } => {
            if state.custom_picker_binding() != Some(&binding) {
                return Ok(());
            }
            loop_state
                .catalogs
                .request(app, state, catalog_jobs::Request::Custom(binding));
        }
        PanelIntent::OpenAuthorization => {
            if loop_state.authentication_origin == Some(state.view_identity().0) {
                loop_state.authentication.open(state);
            }
        }
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
            if matches!(action, oc_tui::terminal_view::TerminalIntent::Refresh) {
                if state.attached_session() == Some(&session) {
                    loop_state.terminals.request(state);
                }
                return Ok(());
            }
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
        PanelIntent::LoadChildren => loop_state.reads.request(state, false),
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
            loop_state.work.start(
                app,
                state,
                PanelIntent::BackgroundSession {
                    session,
                    turn,
                    work,
                },
            )?;
        }
        work @ (PanelIntent::BackgroundChild { .. } | PanelIntent::InterruptChild { .. }) => {
            loop_state.work.start(app, state, work)?
        }
        control @ (PanelIntent::SetPermissionMode { .. }
        | PanelIntent::ReplyApproval(_)
        | PanelIntent::ReplyQuestion(_)) => {
            if matches!(&control, PanelIntent::SetPermissionMode { .. })
                && (loop_state.navigation_job.is_some() || loop_state.scope_uncertain)
            {
                return Err("session scope changing; permission mode unchanged".into());
            }
            loop_state
                .controls
                .reply(app, state, control, loop_state.cli_auto)?
        }
        PanelIntent::CompactSession => {
            if loop_state.compaction_job.is_some() {
                return Err("compaction admission pending".into());
            }
            let session = state.attached_session().cloned().ok_or("no session yet")?;
            let owner = app.clone();
            let revision = state.compaction_request_revision();
            loop_state.compaction_origin = read_jobs::Route::capture(state);
            loop_state.compaction_job = Some(oc_core::wake::spawn(async move {
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
            loop_state.conversation_origin = read_jobs::Route::capture(state)
                .map(|route| (route, loop_state.navigation_revision));
            loop_state.conversation_job = Some(oc_core::wake::spawn(async move {
                let snapshot = owner
                    .change_conversation(session.clone(), action)
                    .await
                    .map_err(|e| e.to_string())?;
                let refresh = conversation_refresh(owner, session)
                    .await
                    .map_err(|_| "conversation refresh interrupted".to_string())?;
                Ok(ConversationResult::Changed(snapshot, refresh))
            }));
        }
        PanelIntent::ForkMessage { message } => {
            if loop_state.conversation_recovery.is_some() || loop_state.recovery_job.is_some() {
                return Err("conversation refresh pending".into());
            }
            if !loop_state.can_open_session() {
                return Err("tab limit reached".into());
            }
            let session = require_session(state)?;
            let owner = app.clone();
            loop_state.conversation_origin = read_jobs::Route::capture(state)
                .map(|route| (route, loop_state.navigation_revision));
            loop_state.conversation_job = Some(oc_core::wake::spawn(async move {
                let snapshot = owner
                    .fork_session(session, message)
                    .await
                    .map_err(|e| e.to_string())?;
                let refresh = conversation_refresh(owner, snapshot.session.clone())
                    .await
                    .map_err(|_| "conversation refresh interrupted".to_string())?;
                Ok(ConversationResult::Forked(snapshot, refresh))
            }));
        }
        PanelIntent::CopyMessage { message, seq } => {
            loop_state.history_reads.request(
                app,
                state,
                history_jobs::Request::Copy(message, seq),
            )?;
        }
        PanelIntent::ReloadConfiguration => {
            // OC2 reloads under a running turn: its prepared request keeps the
            // captured generation, the next fresh request uses the new one.
            let slash_draft = (state.input().trim() == "/reload").then(|| state.input().to_owned());
            state.push_transient_note_for(
                "Reloading configuration…",
                NoteVariant::Info,
                std::time::Duration::from_secs(30),
            );
            loop_state.reload_draft = slash_draft;
            let owner = app.clone();
            loop_state.reload_job = Some(oc_core::wake::spawn(async move {
                owner.reload_location().await
            }));
            loop_state.reload_painted = false;
        }
        PanelIntent::LoadCatalog => {
            loop_state.catalogs.request(
                app,
                state,
                catalog_jobs::Request::Selection {
                    action: SelectionAction::Current,
                    preview: false,
                    fallback: None,
                },
            );
        }
        PanelIntent::LoadMcps => {
            loop_state
                .resources
                .request(app, state, resource_jobs::Request::Mcp(None))?;
        }
        PanelIntent::McpControl(control) => {
            loop_state
                .resources
                .request(app, state, resource_jobs::Request::Mcp(Some(control)))?;
        }
        PanelIntent::LoadSessions => {
            loop_state
                .picker
                .request(app, state, picker_jobs::Request::List)?;
        }
        PanelIntent::RenameSelectedSession { id, title } => {
            let target = SessionId::new(id.clone()).ok_or("bad session id")?;
            if loop_state.read_only {
                return Err("child session: read-only history; rename refused".into());
            }
            loop_state.picker.request(
                app,
                state,
                picker_jobs::Request::Rename {
                    target,
                    title,
                    direct: None,
                },
            )?;
        }
        PanelIntent::DeleteSelectedSession { id } => {
            // OC2: the owner interrupts a running session before deleting it.
            if loop_state.read_only {
                state.session_delete_rejected(
                    "conversation operation pending or read-only history; delete refused".into(),
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
                    (tab_route_session(view) == Some(&target)).then_some(index)
                });
            // Prepare the replacement before destructive acceptance. The
            // current view, draft and tab membership survive all refusals.
            if let Some(index) = index {
                loop_state.target_ready(state, index)?;
                deck_jobs::start_delete(app, state, loop_state, target)?;
            } else {
                // Off-deck (including foreign Location) picker deletion owns
                // its own typed receipt, not this Location's membership/token.
                loop_state
                    .picker
                    .request(app, state, picker_jobs::Request::Delete { target })?;
            }
        }
        PanelIntent::LoadSkills => {
            loop_state
                .resources
                .request(app, state, resource_jobs::Request::Skills)?;
        }
        PanelIntent::LoadCards => {
            loop_state.history_reads.request(
                app,
                state,
                history_jobs::Request::Cards(loop_state.cards_before),
            )?;
        }
        PanelIntent::LoadShells => loop_state.reads.request(state, false),
        work @ (PanelIntent::CancelShell { .. } | PanelIntent::BackgroundShell { .. }) => {
            loop_state.work.start(app, state, work)?
        }
        PanelIntent::LoadCardOutput { op, offset } => {
            loop_state.history_reads.request(
                app,
                state,
                history_jobs::Request::Output(op, offset),
            )?;
        }
        PanelIntent::SelectModel { id, recent } => {
            if loop_state.catalogs.preparing(state) {
                return Err("agent selection preparing; model draft unchanged".into());
            }
            state.draft_model(&id)?;
            // Pinned `model.set(..., { recent: true })` ignores storage failures.
            loop_state.catalogs.request(
                app,
                state,
                catalog_jobs::Request::Preference {
                    action: recent.map(oc_core::queries::ModelPreferenceAction::Recent),
                    home: state
                        .attached_session()
                        .is_none()
                        .then(|| state.captured_model_commit())
                        .flatten(),
                },
            );
        }
        PanelIntent::LoadModelDirectory => {
            loop_state
                .catalogs
                .request(app, state, catalog_jobs::Request::Directory);
        }
        PanelIntent::FavoriteModel { model, enabled } => {
            loop_state.catalogs.request(
                app,
                state,
                catalog_jobs::Request::Preference {
                    action: Some(oc_core::queries::ModelPreferenceAction::Favorite {
                        model,
                        enabled,
                    }),
                    home: None,
                },
            );
        }
        PanelIntent::ChooseModel { variant, .. } => {
            if loop_state.catalogs.preparing(state) {
                return Err("agent selection preparing; model draft unchanged".into());
            }
            state.draft_variant(variant.as_deref())?;
            loop_state.catalogs.request(
                app,
                state,
                catalog_jobs::Request::Preference {
                    action: state.drafted_variant(),
                    home: state
                        .attached_session()
                        .is_none()
                        .then(|| state.captured_model_commit())
                        .flatten(),
                },
            );
        }
        PanelIntent::CycleVariant => {
            if loop_state.catalogs.preparing(state) {
                return Err("agent selection preparing; model draft unchanged".into());
            }
            if state.cycle_variant_draft()? {
                loop_state.catalogs.request(
                    app,
                    state,
                    catalog_jobs::Request::Preference {
                        action: state.drafted_variant(),
                        home: state
                            .attached_session()
                            .is_none()
                            .then(|| state.captured_model_commit())
                            .flatten(),
                    },
                );
            }
        }
        PanelIntent::NewSession | PanelIntent::AddSessionTab => {
            if !loop_state.root_navigation_allowed(state) {
                return Err("child session: read-only history; saved tabs are unchanged".into());
            }
            loop_state.sync_tabs(state);
            if let Some(home) = &loop_state.home {
                if typed_new {
                    let home_id = home.view_identity().0;
                    if loop_state.unresolved_homes.contains(&home_id) {
                        loop_state
                            .home_commands
                            .insert(home_id, (state.view_identity().0, state.input().to_owned()));
                    } else {
                        state.accept_intent();
                    }
                }
                child_controls::return_root(state, loop_state);
                loop_state.restore_home(state);
                if loop_state.save_disabled {
                    state.push_note("tab deck could not be saved; review saved tabs");
                }
                return Ok(());
            }
            if loop_state.tabs.len() >= MAX_TABS && tab_route_session(state).is_some() {
                return Err("tab limit reached".into());
            }
            // A shared tab Add navigates to the existing Home selection, just
            // like restoring a parked Home. Merely viewing a blocked saved
            // choice must not re-admit it or overwrite it with this chat's agent.
            // The explicit /new action retains its reset/admission semantics.
            let action = if app.shares_workspace() && intent == PanelIntent::AddSessionTab {
                SelectionAction::Current
            } else {
                SelectionAction::New(
                    loop_state
                        .child_parent
                        .as_deref()
                        .unwrap_or(state)
                        .composer_agent()
                        .map(str::to_string),
                )
            };
            let mut home = TuiState::new_home(app.clone());
            // Paint retained local chrome immediately, but never submit using
            // a guessed Home model/agent. The authoritative catalog is separate.
            home.chrome = loop_state
                .child_parent
                .as_deref()
                .unwrap_or(state)
                .chrome
                .clone();
            home.chrome.location = loop_state.location.clone().or(home.chrome.location);
            loop_state.home_pending = Some((home.view_identity(), action));
            loop_state.unresolved_homes.insert(home.view_identity().0);
            if typed_new {
                loop_state.home_commands.insert(
                    home.view_identity().0,
                    (state.view_identity().0, state.input().to_owned()),
                );
            }
            child_controls::return_root(state, loop_state);
            loop_state.open_home(state, home);
            if loop_state.save_disabled {
                state.push_note("tab deck could not be saved; review saved tabs");
            }
        }
        intent @ (PanelIntent::ActivateTab { .. } | PanelIntent::MoveTab { .. }) => {
            let before = loop_state.snapshot(state);
            let moving = matches!(intent, PanelIntent::MoveTab { .. });
            let index = match intent {
                PanelIntent::ActivateTab { index } => {
                    if preserve_pointer {
                        loop_state
                            .deck_job
                            .as_ref()
                            .and_then(|job| job.pointer_target(index))
                            .and_then(|id| deck_jobs::route_index(state, loop_state, id))
                            .unwrap_or(index)
                    } else {
                        index
                    }
                }
                PanelIntent::MoveTab { session, index } => {
                    loop_state.move_tab(app, state, session, index).await?;
                    return Ok(());
                }
                _ => unreachable!(),
            };
            if preserve_pointer {
                loop_state.activate_with_hover(state, index, true)?;
            } else {
                loop_state.activate(state, index)?;
            }
            loop_state.reads.request(state, false);
            // Parked prompt/editor state survives routing, but pending ownership
            // must be recovered before its first visible frame after activation.
            if state.is_busy()
                || !state.approval_roots.is_empty()
                || state.approvals.active().is_some()
            {
                loop_state.approvals_checked = None;
            }
            if let Some(session) = state.attached_session().cloned()
                && loop_state.picker_pending_tabs.remove(&session)
            {
                if let Some(job) = loop_state.recovery_job.take() {
                    job.abort();
                }
                loop_state.conversation_recovery = Some((session, std::time::Instant::now()));
            }
            if !moving {
                loop_state.selection_dirty |= loop_state.snapshot(state) != before;
            }
            if loop_state.save_disabled {
                state.push_note("tab deck could not be saved; review saved tabs");
            }
        }
        PanelIntent::CloseTab { index } => {
            loop_state.close_tab(app, state, index).await?;
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
            if loop_state.read_only {
                return Err("child session: read-only history; rename refused".into());
            }
            let target = state
                .attached_session()
                .filter(|_| {
                    !state.home
                        && loop_state.active_tab.is_some_and(|index| {
                            index < loop_state.tabs.len() && loop_state.tabs[index].is_none()
                        })
                })
                .cloned()
                .ok_or("no active session; rename refused")?;
            loop_state.picker.request(
                app,
                state,
                picker_jobs::Request::Rename {
                    target,
                    title,
                    direct: Some(direct),
                },
            )?;
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
                        oc_core::wake::spawn(async move { owner.regenerate_title(target).await }),
                    ));
                }
                _ => state
                    .regenerated_title(Err("title generation unavailable for this session".into())),
            }
        }
        PanelIntent::Inbox { action, restore } => {
            loop_state.inbox_jobs.start(app, state, action, restore)?;
        }
        PanelIntent::SelectAgent { id } => {
            // OC2: a session's agent is a composer draft until the next send;
            // the running turn keeps its profile. Home persists its own choice.
            let home = state.home || state.attached_session().is_none();
            loop_state.catalogs.request(
                app,
                state,
                catalog_jobs::Request::Selection {
                    action: if home {
                        SelectionAction::Agent(id)
                    } else {
                        SelectionAction::Preview(id)
                    },
                    preview: !home,
                    fallback: state.captured_model_draft(),
                },
            );
        }
        PanelIntent::SwitchSession { id } => {
            if loop_state.picker.defer_open(
                state,
                loop_state.navigation_revision,
                loop_state.interaction_revision,
                preserve_pointer,
            )? {
                return Ok(());
            }
            let target = SessionId::new(id).ok_or_else(|| "bad session id".to_string())?;
            let index = loop_state
                .tabs
                .iter()
                .enumerate()
                .find_map(|(index, parked)| {
                    let view = if loop_state.active_tab == Some(index) {
                        loop_state.child_parent.as_deref().unwrap_or(state)
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
            navigation_jobs::start(
                app,
                state,
                loop_state,
                navigation_jobs::Request::Open(target),
            )?;
        }
        PanelIntent::SwitchLocation { path } => {
            // OC2: a running root keeps running in its Location. Only an
            // unresolved receipt keeps the current view until it is known.
            loop_state.departure_ready(state)?;
            // Publish a sessionless generation first: even when switching
            // from a real tab, a saved Home route cannot mint a new root.
            navigation_jobs::start(
                app,
                state,
                loop_state,
                navigation_jobs::Request::Location(path),
            )?;
        }
        PanelIntent::LoadOlder => {
            require_session(state)?;
            let before = state
                .history()
                .rows()
                .iter()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(before) = before {
                let part = state.history().older_part_cursor();
                let window = oc_core::queries::HistoryReadWindow {
                    before_seq: part.is_none().then_some(before),
                    after_seq: None,
                    part,
                };
                loop_state.history_reads.request(
                    app,
                    state,
                    history_jobs::Request::Window {
                        window,
                        older: true,
                    },
                )?;
            }
        }
        PanelIntent::LoadNewer => {
            require_session(state)?;
            let after = state
                .history()
                .rows()
                .iter()
                .rev()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(after) = after {
                let part = state.history().newer_part_cursor();
                let window = oc_core::queries::HistoryReadWindow {
                    after_seq: part.is_none().then_some(after),
                    before_seq: None,
                    part,
                };
                loop_state.history_reads.request(
                    app,
                    state,
                    history_jobs::Request::Window {
                        window,
                        older: false,
                    },
                )?;
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

/// A refused atomic reload leaves an owner live. Consume its current provider
/// and MCP facts before explicit feedback, so a queued same-cause provider hint
/// cannot replace the operation outcome before it is painted.
async fn finish_reload_refusal(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    message: String,
) {
    refresh_provider_views(app, state, deck).await;
    deck.provider_feedback = Some((deck.location.clone(), message.clone()));
    state.push_transient_note(&message, NoteVariant::Error);
}

fn finish_provider_feedback(state: &mut TuiState, deck: &mut LoopState) -> bool {
    if !deck.catalogs.pending()
        && !deck.resources.pending()
        && let Some((location, message)) = deck.provider_feedback.take()
        && location == deck.location
    {
        state.push_transient_note(&message, NoteVariant::Error);
        return true;
    }
    false
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
    reload_jobs::start(app, state, deck, snapshot, slash_draft)
}

/// A published Location invalidates every old view, including the parked
/// Home draft. On a broken preference keep the new Location's Home and show
/// a static diagnostic rather than accidentally repainting old-location data.
#[cfg(test)]
async fn adopt_location(
    app: &CoreApp,
    state: &mut TuiState,
    deck: &mut LoopState,
    catalog: CatalogSnapshot,
    location: &str,
) {
    navigation_jobs::start(
        app,
        state,
        deck,
        navigation_jobs::Request::Published {
            catalog: Box::new(catalog),
            location: location.into(),
        },
    )
    .expect("published Location adoption");
}

fn require_session(state: &TuiState) -> Result<SessionId, String> {
    state
        .attached_session()
        .cloned()
        .ok_or_else(|| "no active session; submit a prompt first".to_string())
}

/// Every tab shares the one owner attachment, not only the focused one.
fn report_owner(state: &mut TuiState, deck: &mut LoopState, note: &str, variant: NoteVariant) {
    for view in read_jobs::views_mut(state, deck) {
        view.push_note_variant(note, variant);
    }
}

/// The same handle now serves a fresh attachment: its consumer registrations
/// and live cursor are new, so redo the startup reads without resending input.
async fn reattached(app: &CoreApp, state: &mut TuiState, deck: &mut LoopState) {
    // A re-elected owner restarts live revisions; the old cursor would hide them.
    deck.projection_revision = 0;
    deck.deferred_events.clear();
    deck.shared_gap = true;
    deck.shared_refresh = true;
    let restored = async {
        let catalog = app.catalog().await.map_err(|error| error.to_string())?;
        app.register_approval_consumer(deck.cli_auto || catalog.chrome.permissions_auto)
            .await
            .map_err(|error| error.to_string())?;
        app.register_question_consumer()
            .await
            .map_err(|error| error.to_string())?;
        deck.permission_auto =
            Some(catalog.auto_accept == oc_core::queries::AutoAcceptState::Enabled);
        refresh_approvals(app, state).await
    }
    .await;
    match restored {
        Ok(()) => {
            for view in read_jobs::views_mut(state, deck) {
                view.push_transient_note(OWNER_RESTORED, NoteVariant::Success);
            }
        }
        Err(error) => {
            let note = format!("Runtime reconnected, but reloading failed: {error}");
            report_owner(state, deck, &note, NoteVariant::Error);
        }
    }
    deck.reads.request(state, true);
}

#[cfg(test)]
async fn handle_worker_event(
    app: &CoreApp,
    state: &mut TuiState,
    _loop_state: &mut LoopState,
    session: &SessionId,
    event: CoreEvent,
) -> Result<(), String> {
    handle_worker_event_at(app, state, _loop_state, session, event, None, false).await
}

async fn handle_worker_event_at(
    app: &CoreApp,
    state: &mut TuiState,
    _loop_state: &mut LoopState,
    _session: &SessionId,
    event: CoreEvent,
    revision: Option<u64>,
    replay: bool,
) -> Result<(), String> {
    if replay
        && matches!(
            &event,
            CoreEvent::ShellChanged { .. }
                | CoreEvent::ExecuteChanged { .. }
                | CoreEvent::TerminalChanged { .. }
                | CoreEvent::SessionMoved { .. }
                | CoreEvent::PermissionAsked(_)
                | CoreEvent::PermissionResolved { .. }
                | CoreEvent::QuestionAsked(_)
                | CoreEvent::QuestionResolved { .. }
        )
    {
        return Ok(());
    }
    // Root tabs own their live view even while Home/another tab is selected.
    // Global inventory/Location publications below keep their existing all-view
    // handlers; only exact session-addressed normal events take this route.
    if !matches!(
        &event,
        CoreEvent::ShellChanged { .. }
            | CoreEvent::ExecuteChanged { .. }
            | CoreEvent::TerminalChanged { .. }
            | CoreEvent::SessionMoved { .. }
    ) && let Some(owner) = child_controls::event_owner(&event).cloned()
        && state.attached_session() != Some(&owner)
        && let Some(index) = _loop_state.tabs.iter().position(|view| {
            view.as_ref()
                .is_some_and(|view| view.attached_session() == Some(&owner))
        })
    {
        let mut parked = _loop_state.tabs[index].take().expect("exact parked root");
        let result = Box::pin(handle_worker_event_at(
            app,
            &mut parked,
            _loop_state,
            &owner,
            event,
            revision,
            replay,
        ))
        .await;
        // Restore unique ownership before propagating a bounded read failure.
        // The selected state, active slot, drafts and deck revision are untouched.
        _loop_state.tabs[index] = Some(parked);
        return result;
    }
    if !matches!(event, CoreEvent::SessionMoved { .. })
        && child_controls::route_event(app, state, _loop_state, &event, revision, replay).await?
    {
        return Ok(());
    }
    if let CoreEvent::TerminalChanged { session: owner } = &event {
        let mut jobs = std::mem::take(&mut _loop_state.terminals);
        for view in read_jobs::views_mut(state, _loop_state)
            .filter(|view| view.attached_session() == Some(owner))
        {
            jobs.request(view);
        }
        _loop_state.terminals = jobs;
        return Ok(());
    }
    if let CoreEvent::ShellChanged { session: owner }
    | CoreEvent::ExecuteChanged { session: owner } = &event
    {
        let mut jobs = std::mem::take(&mut _loop_state.reads);
        for view in read_jobs::views_mut(state, _loop_state)
            .filter(|view| view.attached_session() == Some(owner))
        {
            jobs.shells(view);
        }
        _loop_state.reads = jobs;
        return Ok(());
    }
    if let CoreEvent::SessionMoved {
        session: moved,
        location,
        ..
    } = &event
    {
        if let Some(snapshot) = location {
            if !replay {
                navigation_jobs::moved(state, _loop_state, moved, snapshot.clone());
                _loop_state.shared_refresh |= app.shares_workspace();
            }
        } else {
            // An explicit idle target left this scope; retire only its parked
            // view. The caller's Location, draft and all surviving views stay owned.
            if let Some(index) = _loop_state.tabs.iter().position(|view| {
                view.as_ref()
                    .is_some_and(|view| view.attached_session() == Some(moved))
            }) {
                if _loop_state.target_ready(state, index).is_err() {
                    _loop_state.closed_owners.insert(moved.clone(), None);
                } else {
                    _loop_state.tabs.remove(index);
                    _loop_state.tab_cards_before.remove(index);
                    if let Some(active) = _loop_state.active_tab.as_mut()
                        && *active > index
                    {
                        *active -= 1;
                    }
                }
            }
            _loop_state.shared_refresh |= app.shares_workspace();
            _loop_state.selection_blocked = true;
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
        _loop_state.controls.request();
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
        CoreEvent::Projection { .. }
        | CoreEvent::WorkspaceChanged { .. }
        | CoreEvent::SessionViewed { .. }
        | CoreEvent::ProjectionGap
        | CoreEvent::OwnerDisconnected
        | CoreEvent::OwnerReattached
        | CoreEvent::OwnerUnrecoverable { .. } => return Ok(()),
        CoreEvent::ChildNotice(notice) => &notice.job.parent,
        CoreEvent::ShellNotice(notice) => &notice.session,
        CoreEvent::ExecuteNotice(notice) => &notice.session,
        CoreEvent::ShellChanged { session }
        | CoreEvent::ExecuteChanged { session }
        | CoreEvent::ParentContinuationRefused { session, .. }
        | CoreEvent::TerminalChanged { session }
        | CoreEvent::SessionMoved { session, .. }
        | CoreEvent::SessionModelSelected { session, .. }
        | CoreEvent::SelectionNoticeCommitted { session, .. }
        | CoreEvent::InboxChanged { session, .. }
        | CoreEvent::SessionTitleUpdated { session, .. }
        | CoreEvent::RetryScheduled { session, .. }
        | CoreEvent::TurnStarted { session, .. }
        | CoreEvent::TurnPresentation { session, .. }
        | CoreEvent::TextDelta { session, .. }
        | CoreEvent::ReasoningDelta { session, .. }
        | CoreEvent::ReasoningItemEnded { session, .. }
        | CoreEvent::ToolCallStarted { session, .. }
        | CoreEvent::ToolArgumentStream { session, .. }
        | CoreEvent::ToolCallProgress { session, .. }
        | CoreEvent::ToolCallFinished { session, .. }
        | CoreEvent::TurnUsage { session, .. }
        | CoreEvent::TurnContextUsage { session, .. }
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
    let covered = revision.filter(|_| shared::turn_event(&event));
    if covered.is_some_and(|revision| revision <= state.projection_floor()) {
        return Ok(());
    }
    match event {
        CoreEvent::ParentContinuationRefused { diagnostic, .. } => {
            state.push_transient_note(
                &format!("Background continuation refused: {diagnostic}"),
                NoteVariant::Error,
            );
        }
        CoreEvent::ChildNotice(notice) => {
            if !replay {
                _loop_state
                    .notices
                    .request(state, CoreEvent::ChildNotice(notice));
            }
        }
        CoreEvent::ShellChanged { .. } => unreachable!("handled above"),
        CoreEvent::ExecuteChanged { .. } => unreachable!("handled above"),
        CoreEvent::TerminalChanged { .. } => {}
        CoreEvent::SessionModelSelected { session, commit } => {
            state.apply_session_model_selected(&session, &commit)
        }
        CoreEvent::SelectionNoticeCommitted { session, message } => {
            if !replay {
                _loop_state.notices.request(
                    state,
                    CoreEvent::SelectionNoticeCommitted { session, message },
                );
            }
        }
        CoreEvent::InboxChanged {
            session,
            items,
            delivered,
        } => {
            state.apply_inbox(&session, items.clone());
            // A steer joined the running turn: correlate the live prompt, then
            // show its durable user row placed by the turn's notice order.
            if !replay && !delivered.is_empty() {
                _loop_state.notices.request(
                    state,
                    CoreEvent::InboxChanged {
                        session,
                        items,
                        delivered,
                    },
                );
            }
        }
        CoreEvent::ShellNotice(notice) => {
            if !replay {
                _loop_state
                    .notices
                    .request(state, CoreEvent::ShellNotice(notice));
            }
        }
        CoreEvent::ExecuteNotice(notice) => {
            if !replay {
                _loop_state
                    .notices
                    .request(state, CoreEvent::ExecuteNotice(notice));
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
        CoreEvent::Projection { .. }
        | CoreEvent::WorkspaceChanged { .. }
        | CoreEvent::SessionViewed { .. }
        | CoreEvent::ProjectionGap
        | CoreEvent::OwnerDisconnected
        | CoreEvent::OwnerReattached
        | CoreEvent::OwnerUnrecoverable { .. } => unreachable!("handled before session routing"),
        CoreEvent::SessionTitleUpdated { title, .. } => {
            if state.session_title.as_ref() != Some(&title) {
                state.observe_history_commit();
                state.session_title = Some(title);
            }
        }
        CoreEvent::TurnStarted {
            turn, model_switch, ..
        } => {
            if state.linked_child().is_some_and(|job| {
                job.turn.as_deref() == Some(turn.0.as_str())
                    && !matches!(
                        job.state,
                        oc_core::queries::ChildState::Admitted
                            | oc_core::queries::ChildState::Running
                    )
            }) {
                return Ok(());
            }
            if replay
                || state.linked_child().is_some()
                || app.shares_workspace() && !state.has_pending_submission()
            {
                let refresh =
                    !replay && app.shares_workspace() && state.active_turn() != Some(&turn);
                state.begin_linked_turn(turn.clone());
                if refresh {
                    _loop_state.reads.request(state, true);
                }
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
            output_presentation,
            turn,
            op,
            name,
            input,
            dcp_topic,
            ..
        } => {
            state.apply_tool_started_with_output_presentation(
                &turn,
                &op,
                &name,
                &input,
                dcp_topic,
                output_presentation,
            );
            if !replay
                && matches!(name.as_str(), "subagent" | "execute")
                && state.active_turn() == Some(&turn)
            {
                _loop_state.reads.request(state, false);
            }
        }
        CoreEvent::ToolCallProgress {
            turn,
            op,
            output_presentation,
            ..
        } => {
            state.apply_tool_progress(&turn, &op, output_presentation);
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
            if !replay && name == "subagent" && state.active_turn() == Some(&turn) {
                _loop_state.reads.request(state, false);
            }
            if !replay && name == "compress" && state.active_turn() == Some(&turn) {
                _loop_state.reads.request(state, false);
            }
        }
        CoreEvent::TurnContextUsage {
            turn,
            input_tokens,
            output_tokens,
            context_limit,
            ..
        } => state.apply_context_usage(&turn, input_tokens, output_tokens, context_limit),
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
            if current && !replay {
                if compress {
                    _loop_state.reads.compress(state, turn);
                } else {
                    _loop_state.reads.request(state, true);
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
            let current = state.active_turn() == Some(&turn);
            let compress = state.is_compress_turn(&turn);
            state.apply_interrupted(&turn, &partial, duration_ms);
            if compress {
                state.notify_dcp(DcpOutcome::Failed {
                    reason: "compress turn cancelled".to_string(),
                });
            }
            if current && !replay {
                // Replace the accepted prompt's local echo with its durable ID,
                // just as on completion, so message actions work after Esc.
                _loop_state.reads.request(state, true);
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
    if let Some(revision) = covered {
        state.cover_projection(revision, false);
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
    if deck.reload_pending() {
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
    let origin = deck.compaction_origin.take();
    let result = deck
        .compaction_job
        .take()
        .expect("finished compaction admission")
        .await
        .unwrap_or(Err(CoreError::Shutdown));
    let view = read_jobs::views_mut(state, deck)
        .find(|view| origin.as_ref().is_some_and(|route| route.owns(view)));
    if let Some(view) = view {
        match result {
            Ok((snapshot, revision)) => {
                if view
                    .attached_session()
                    .is_some_and(|id| id.0 == snapshot.session)
                {
                    view.compaction_admitted(snapshot, revision);
                }
            }
            Err(error) => {
                view.push_transient_note(&format!("compaction: {error}"), NoteVariant::Error)
            }
        }
    }
    deck.sync_tabs(state);
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

/// Only this exact turn can establish a manual-operation outcome. Successful
/// notices are emitted once by ToolCallFinished from its typed run snapshot.
fn report_compress_outcome(
    state: &mut TuiState,
    turn: &oc_core::core_app::WorkerTurnId,
    page: &oc_core::queries::HistoryPage,
) {
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
        "strip_frames": frames.map_or(0, |frames| frames.strip_frames),
        "strip_refused": frames.map_or(0, |frames| frames.strip_refused),
        "wakeups": frames.map_or(0, |frames| frames.wakeups),
        "completion_wakeups": frames.map_or(0, |frames| frames.completion_wakeups),
        "memory_releases": frames.map_or(0, |frames| frames.memory_releases),
        "display_hz": frames.and_then(|frames| frames.display_hz),
        "input_events": frames.map_or(0, |frames| frames.input_events),
        "worker_events": frames.map_or(0, |frames| frames.worker_events),
        "workspace_reads": deck.workspace_reads,
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
