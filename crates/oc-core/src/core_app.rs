//! Typed vertical slice for T03: commands, queries, session worker,
//! scripted provider, cancellation and bounded channels.
//!
//! One `SessionWorker` owns all sessions and at most one active turn. Long
//! provider streams never block `CancelTurn` or read queries: the worker
//! `select!`s the inbox against the next-chunk timer. No real network or
//! SQLite happens here; persistence/resume arrive in T04/T05.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::{broadcast, mpsc, oneshot};

use crate::domain::SessionId;
use crate::queries::{
    CatalogSnapshot, DcpSnapshot, HistoryPage, HomeLocationSnapshot, LocationSnapshot,
    SessionProbe, SkillCard, TabDeckSnapshot, ToolOpPage,
};
use crate::session::{CoreError, MAX_INPUT_BYTES, MAX_QUEUE_ITEMS, Message, MessageId, Role};

/// Opaque turn id for the worker (monotonic `t0001`, …).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkerTurnId(pub String);

/// Explicit first-turn choice for a new root. `None` at the API boundary uses
/// the application's current Home choice; `Some` pins these exact values to
/// the new session and turn. The owner validates the model, variant and agent
/// against its current Location before creating anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreshSelection {
    /// Selected primary agent, if any.
    pub agent_id: Option<String>,
    /// Exact selected model id.
    pub model_id: String,
    /// Exact variant, or the explicit Default variant.
    pub variant: Option<String>,
}

/// One enqueued submission's acceptance receipt. The application owns the
/// operation even if this receipt is dropped; cancel/shutdown it through CoreApp.
pub struct SubmissionReceipt(oneshot::Receiver<Result<WorkerTurnId, CoreError>>);

impl SubmissionReceipt {
    /// Poll without waiting for network, provider or durable acceptance.
    pub fn try_result(&mut self) -> Option<Result<WorkerTurnId, CoreError>> {
        match self.0.try_recv() {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }

    /// Wait for the owner's acceptance decision on this already-enqueued
    /// request. A closed owner is a failure, never an accepted turn.
    pub async fn wait(&mut self) -> Result<WorkerTurnId, CoreError> {
        (&mut self.0).await.map_err(|_| CoreError::Shutdown)?
    }
}

/// Typed application events (live hints + durable outcomes for T03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    /// Safe durable checkpoint projection; identical identity/order on restart.
    TurnPresentation {
        /// Owning session.
        session: SessionId,
        /// Owning turn.
        turn: WorkerTurnId,
        /// Bounded metadata and public parts, never wire continuation.
        projection: crate::queries::HistoryTurn,
    },
    /// Turn was accepted and streaming started.
    TurnStarted {
        /// Session that owns the turn.
        session: SessionId,
        /// New turn id.
        turn: WorkerTurnId,
    },
    /// Incremental provider delta (not yet a durable message).
    TextDelta {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Chunk text.
        delta: String,
    },
    /// Incremental provider reasoning/thinking delta (not durable; rendered
    /// as the transcript's reasoning block, `session.reasoning.delta` upstream).
    ReasoningDelta {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Reasoning text chunk.
        delta: String,
    },
    /// Terminal provider usage for the turn (only when the provider reported
    /// it; never synthesized).
    TurnUsage {
        /// Session that owns the turn.
        session: SessionId,
        /// Turn the usage belongs to.
        turn: WorkerTurnId,
        /// Input tokens of the last reported provider round.
        input_tokens: u64,
        /// Output tokens summed over the turn's provider rounds.
        output_tokens: u64,
        /// Provider-active streaming time in milliseconds, summed over rounds
        /// (upstream `time.streamed - time.created` per assistant step).
        streamed_ms: u64,
    },
    /// One tool call intent was recorded durably (before the side effect);
    /// the transcript renders a running card. The input is the recorded
    /// arguments JSON, bounded by the tool argument caps (the view bounds
    /// its own previews).
    ToolCallStarted {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// Recorded arguments JSON.
        input: String,
    },
    /// One tool call reached a terminal state (durable outcome recorded);
    /// the transcript updates the card in place.
    ToolCallFinished {
        /// Session that owns the turn.
        session: SessionId,
        /// Active turn id.
        turn: WorkerTurnId,
        /// Durable operation id.
        op: String,
        /// Registry tool name.
        name: String,
        /// `completed` / `failed` / `denied` / `cancelled` / `no_gain`.
        state: String,
        /// Outcome preview (bounded by the producer at the report cap).
        output: String,
        /// Full stored output size in bytes.
        output_bytes: i64,
        /// True when [`CoreEvent::ToolCallFinished::output`] is a preview.
        output_truncated: bool,
    },
    /// Turn completed; assistant message is now in history.
    TurnFinished {
        /// Session that owns the turn.
        session: SessionId,
        /// Finished turn id.
        turn: WorkerTurnId,
        /// Full assistant text.
        text: String,
        /// Turn wall time in milliseconds, accept to commit (upstream
        /// `turnDuration`: user message created → assistant completed).
        duration_ms: u64,
        /// Sanitized non-fatal notices for this turn (e.g. degraded MCP
        /// servers); never secrets, never a reason to hide the answer.
        warnings: Vec<String>,
    },
    /// Turn was cancelled; partial text was not committed as a message.
    TurnInterrupted {
        /// Session that owns the turn.
        session: SessionId,
        /// Interrupted turn id.
        turn: WorkerTurnId,
        /// Accumulated text at cancel time (not stored).
        partial: String,
        /// Turn wall time in milliseconds, accept to interrupt.
        duration_ms: u64,
    },
    /// Accepted turn failed; never present this as a completed answer.
    TurnFailed {
        /// Session that owns the turn.
        session: SessionId,
        /// Failed turn id.
        turn: WorkerTurnId,
        /// Sanitized application error.
        error: CoreError,
        /// Sanitized non-fatal notices for this turn (e.g. degraded MCP
        /// servers); never secrets, never a substitute for the error.
        warnings: Vec<String>,
    },
}

/// Scripted provider for offline tests.
///
/// `echo` mirrors the prompt as two chunks; `fixed` replays the given chunks.
/// No network I/O happens. Delays are cooperative cancellation points.
#[derive(Debug, Clone)]
pub struct MockProvider {
    chunks: Option<Vec<String>>,
    delay_ms: u64,
}

impl MockProvider {
    /// Echo provider: `["echo: ", prompt]` with 5 ms between chunks.
    pub fn echo() -> Self {
        Self {
            chunks: None,
            delay_ms: 5,
        }
    }

    /// Fixed script with explicit per-chunk delay.
    pub fn fixed(chunks: Vec<String>, delay_ms: u64) -> Self {
        Self {
            chunks: Some(chunks),
            delay_ms,
        }
    }

    fn plan(&self, prompt: &str) -> Vec<String> {
        if let Some(chunks) = &self.chunks {
            chunks.clone()
        } else {
            vec!["echo: ".to_string(), prompt.to_string()]
        }
    }
}

/// Commands consumed by the single application owner (native or scripted).
pub enum InboxMsg {
    /// Create or open a session according to the owner's storage contract.
    Create {
        /// Session id.
        id: SessionId,
        /// Acceptance after the operation succeeds.
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// Accept input and start a turn.
    Submit {
        /// Owning session.
        session: SessionId,
        /// Input text.
        text: String,
        /// Durable acceptance or rejection.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Atomically accept the first turn and create its Location-bound root.
    /// A refusal must not create a session; acknowledgement follows durable
    /// acceptance of the root, selection, turn and user message.
    SubmitFresh {
        /// New, unused root session id.
        session: SessionId,
        /// Input text.
        text: String,
        /// Optional explicit Home selection for this root.
        selection: Option<FreshSelection>,
        /// Durable acceptance or rejection.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Cancel the active turn.
    Cancel {
        /// Owning session.
        session: SessionId,
        /// Cancellation acknowledgement.
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    /// List sessions.
    List {
        /// Query result; storage errors are not an empty list.
        ack: oneshot::Sender<Result<Vec<SessionId>, CoreError>>,
    },
    /// Probe one ID through the application owner, including Location binding.
    ProbeSession {
        id: SessionId,
        ack: oneshot::Sender<Result<SessionProbe, CoreError>>,
    },
    /// Restore the current Location's ordered root tabs without opening a root.
    TabDeck {
        ack: oneshot::Sender<Result<TabDeckSnapshot, CoreError>>,
    },
    /// Persist one validated deck in the current Location.
    SaveTabDeck {
        deck: TabDeckSnapshot,
        ack: oneshot::Sender<Result<TabDeckSnapshot, CoreError>>,
    },
    /// Read committed history.
    Read {
        /// Owning session.
        session: SessionId,
        /// Query result.
        ack: oneshot::Sender<Result<Vec<Message>, CoreError>>,
    },
    /// Read one bounded history page (newest-first cursor, `before_seq`).
    History {
        /// Owning session.
        session: SessionId,
        /// Upper bound: rows with a smaller seq (older). `None` = newest page.
        before_seq: Option<i64>,
        /// Lower bound: rows with a larger seq (newer), oldest-first. Exactly
        /// one of `before_seq`/`after_seq` is set by the caller.
        after_seq: Option<i64>,
        /// Requested rows (clamped by the owner).
        limit: usize,
        /// Query result.
        ack: oneshot::Sender<Result<HistoryPage, CoreError>>,
    },
    /// Read one bounded tool-operation page (newest-first cursor).
    ToolOps {
        /// Owning session.
        session: SessionId,
        /// Upper bound: rows recorded before this rowid.
        before_rowid: Option<i64>,
        /// Requested rows (clamped by the owner).
        limit: usize,
        /// Query result.
        ack: oneshot::Sender<Result<ToolOpPage, CoreError>>,
    },
    /// Read a bounded continuation of one operation's result in this session.
    ToolOutput {
        session: SessionId,
        op: String,
        offset: usize,
        limit: usize,
        ack: oneshot::Sender<Result<crate::queries::ToolOutputPage, CoreError>>,
    },
    /// Model catalog plus the effective model/variant/agent selection.
    Catalog {
        /// Query result.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Read/change a session/agent model draft in the existing application owner.
    SessionSelection {
        /// Owning session.
        session: SessionId,
        /// Home selections additionally remember the Location/agent draft.
        home: bool,
        /// Exact action; model selection is distinct from clearing a variant.
        action: crate::queries::SessionSelectionAction,
        /// Actual accepted selection and catalog.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Read/change the sessionless Home selection in the current Location.
    HomeSelection {
        /// Exact selection action; no session id or session preference exists.
        action: crate::queries::SessionSelectionAction,
        /// Actual selected choice and catalog.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Skill catalog cards (metadata only).
    Skills {
        /// Query result.
        ack: oneshot::Sender<Result<Vec<SkillCard>, CoreError>>,
    },
    /// Select the effective model (and optional variant) for later turns.
    SelectModel {
        /// Exact model id.
        id: String,
        /// Optional variant name.
        variant: Option<String>,
        /// Resulting snapshot after acceptance.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Select the effective primary agent for later turns.
    SelectAgent {
        /// Agent profile id.
        id: String,
        /// Resulting snapshot after acceptance.
        ack: oneshot::Sender<Result<CatalogSnapshot, CoreError>>,
    },
    /// Switch the whole application to another Location (project path).
    ///
    /// The target generation is built completely before the switch is
    /// published; the current Location is kept on failure. Refused while a
    /// turn is active.
    SwitchLocation {
        /// Target project path.
        path: String,
        /// Resulting Location/session/catalog snapshot after acceptance.
        ack: oneshot::Sender<Result<LocationSnapshot, CoreError>>,
    },
    /// Publish another Location's complete generation for sessionless Home.
    SwitchLocationHome {
        /// Target project path.
        path: String,
        /// Location/catalog/diagnostics without an attached session.
        ack: oneshot::Sender<Result<HomeLocationSnapshot, CoreError>>,
    },
    /// DCP context/stats snapshot for a session.
    Dcp {
        /// Owning session.
        session: SessionId,
        /// Query result.
        ack: oneshot::Sender<Result<DcpSnapshot, CoreError>>,
    },
    /// Manual DCP compress request (focus instruction, executed by the owner).
    Compress {
        /// Owning session.
        session: SessionId,
        /// Bounded focus instruction.
        focus: String,
        /// Accepted turn id, or a typed refusal.
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    /// Cancel/drain active work and close the owner.
    Shutdown,
}

struct SessionState {
    id: SessionId,
    messages: Vec<Message>,
    next_seq: u64,
}

impl SessionState {
    fn new(id: SessionId) -> Self {
        Self {
            id,
            messages: Vec::new(),
            next_seq: 1,
        }
    }

    fn push(&mut self, role: Role, text: String) -> Message {
        let id = MessageId(format!("m{:04}", self.next_seq));
        self.next_seq += 1;
        let msg = Message { id, role, text };
        self.messages.push(msg.clone());
        msg
    }
}

struct ActiveTurn {
    session: SessionId,
    turn: WorkerTurnId,
    chunks: Vec<String>,
    index: usize,
    accumulated: String,
    /// Wall-clock start, so terminal events report a measured duration.
    started: std::time::Instant,
}

/// Cloneable application handle over a bounded worker inbox.
#[derive(Debug, Clone)]
pub struct CoreApp {
    inbox: mpsc::Sender<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
}

/// Worker join guard; await after [`CoreApp::shutdown`].
///
/// The worker reports a typed failure string (for example an owned MCP
/// resource that could not be closed), so callers can exit non-zero instead
/// of claiming a clean shutdown.
pub struct WorkerGuard {
    handle: Option<tokio::task::JoinHandle<Result<(), String>>>,
}

impl CoreApp {
    /// Build the existing application boundary for an adapter-owned worker.
    pub fn channel(
        capacity: usize,
    ) -> (Self, mpsc::Receiver<InboxMsg>, broadcast::Sender<CoreEvent>) {
        let (inbox, receiver) = mpsc::channel(capacity);
        let (events, _) = broadcast::channel(256);
        (
            Self {
                inbox,
                events: events.clone(),
            },
            receiver,
            events,
        )
    }

    /// Spawn a worker with default bounded capacity.
    pub fn spawn(provider: MockProvider) -> (Self, WorkerGuard) {
        Self::spawn_with_capacity(provider, MAX_QUEUE_ITEMS)
    }

    /// Spawn a worker with explicit inbox capacity (tests use small caps).
    pub fn spawn_with_capacity(provider: MockProvider, capacity: usize) -> (Self, WorkerGuard) {
        let (app, inbox_rx, event_tx) = Self::channel(capacity);
        let handle = tokio::spawn(async move {
            worker_loop(provider, inbox_rx, event_tx).await;
            Ok(())
        });
        (
            app,
            WorkerGuard {
                handle: Some(handle),
            },
        )
    }

    /// Subscribe to live turn events.
    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.events.subscribe()
    }

    /// Create a session; duplicate ids fail.
    pub async fn create_session(&self, id: SessionId) -> Result<(), CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Create { id, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Submit input; fails fast on oversize/unknown/busy without queueing.
    pub async fn submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Submit {
                session,
                text,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Non-waiting submit: `QueueFull` when the bounded inbox is full.
    pub async fn try_submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Submit {
                session,
                text,
                ack: ack_tx,
            })
            .map_err(|_| CoreError::QueueFull)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Enqueue once without awaiting acceptance. A subsequent session cancel is
    /// ordered after this request in the same inbox; no detached task is needed.
    pub fn request_submit(
        &self,
        session: SessionId,
        text: String,
    ) -> Result<SubmissionReceipt, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Submit { session, text, ack })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Enqueue a new root's first turn without waiting for durable acceptance.
    /// Rejection leaves no session; the receipt resolves only from the owner.
    pub fn request_submit_fresh(
        &self,
        session: SessionId,
        text: String,
        selection: Option<FreshSelection>,
    ) -> Result<SubmissionReceipt, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::SubmitFresh {
                session,
                text,
                selection,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Create a new root and submit its first turn, awaiting the owner's
    /// acceptance acknowledgement. A rejected input never creates a root.
    pub async fn submit_fresh(
        &self,
        session: SessionId,
        text: String,
        selection: Option<FreshSelection>,
    ) -> Result<WorkerTurnId, CoreError> {
        if text.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SubmitFresh {
                session,
                text,
                selection,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        receipt.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Cancel the active turn for a session.
    pub async fn cancel(&self, session: SessionId) -> Result<(), CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Cancel {
                session,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// List known sessions.
    pub async fn list_sessions(&self) -> Result<Vec<SessionId>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::List { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Check one Location-bound ID without listing any other sessions.
    pub async fn probe_session(&self, id: SessionId) -> Result<SessionProbe, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ProbeSession { id, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read the current Location's bounded deck (no root or provider work).
    pub async fn tab_deck(&self) -> Result<TabDeckSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::TabDeck { ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Persist an ordered deck for the current Location after owner validation.
    pub async fn save_tab_deck(&self, deck: TabDeckSnapshot) -> Result<TabDeckSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SaveTabDeck { deck, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read committed history (user + finished assistant messages only).
    pub async fn read_history(&self, session: SessionId) -> Result<Vec<Message>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Read {
                session,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read one bounded history page (newest-first cursor).
    pub async fn history_page(
        &self,
        session: SessionId,
        before_seq: Option<i64>,
        after_seq: Option<i64>,
        limit: usize,
    ) -> Result<HistoryPage, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::History {
                session,
                before_seq,
                after_seq,
                limit,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read one bounded tool-operation page (newest-first cursor).
    pub async fn tool_ops_page(
        &self,
        session: SessionId,
        before_rowid: Option<i64>,
        limit: usize,
    ) -> Result<ToolOpPage, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ToolOps {
                session,
                before_rowid,
                limit,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Fetch the durable output beyond a tool-card preview, scoped to its
    /// session. The owner caps each request and returns a byte continuation.
    pub async fn tool_output_page(
        &self,
        session: SessionId,
        op: String,
        offset: usize,
        limit: usize,
    ) -> Result<crate::queries::ToolOutputPage, CoreError> {
        let (ack, rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::ToolOutput {
                session,
                op,
                offset,
                limit,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Model catalog plus the effective model/variant/agent selection.
    pub async fn catalog(&self) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Catalog { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read/change the durable session/agent selection. Home model choices also
    /// remember the Location/agent draft; no transcript is copied or modified.
    pub async fn session_selection(
        &self,
        session: SessionId,
        home: bool,
        action: crate::queries::SessionSelectionAction,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SessionSelection {
                session,
                home,
                action,
                ack,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Read/change the application's Home choice without creating a session.
    pub async fn home_selection(
        &self,
        action: crate::queries::SessionSelectionAction,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::HomeSelection { action, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Switch the running application to another Location (project path).
    pub async fn switch_location(&self, path: String) -> Result<LocationSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SwitchLocation { path, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Switch a sessionless Home composer without minting a root. A failed
    /// target validation leaves the currently published generation untouched.
    pub async fn switch_location_home(
        &self,
        path: String,
    ) -> Result<HomeLocationSnapshot, CoreError> {
        let (ack, result) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SwitchLocationHome { path, ack })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        result.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Skill catalog cards (metadata only).
    pub async fn skills(&self) -> Result<Vec<SkillCard>, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Skills { ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Select the effective model (and optional variant) for later turns.
    pub async fn select_model(
        &self,
        id: String,
        variant: Option<String>,
    ) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SelectModel {
                id,
                variant,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Select the effective primary agent for later turns.
    pub async fn select_agent(&self, id: String) -> Result<CatalogSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::SelectAgent { id, ack: ack_tx })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// DCP context/stats snapshot for a session.
    pub async fn dcp_snapshot(&self, session: SessionId) -> Result<DcpSnapshot, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Dcp {
                session,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Enqueue manual compression without waiting for durable acceptance.
    pub fn request_compress(
        &self,
        session: SessionId,
        focus: String,
    ) -> Result<SubmissionReceipt, CoreError> {
        if focus.len() > MAX_INPUT_BYTES {
            return Err(CoreError::InputTooLarge);
        }
        let (ack, receipt) = oneshot::channel();
        self.inbox
            .try_send(InboxMsg::Compress {
                session,
                focus,
                ack,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => CoreError::QueueFull,
                mpsc::error::TrySendError::Closed(_) => CoreError::Shutdown,
            })?;
        Ok(SubmissionReceipt(receipt))
    }

    /// Manual DCP compress request; waits for the accepted turn id.
    pub async fn compress(
        &self,
        session: SessionId,
        focus: String,
    ) -> Result<WorkerTurnId, CoreError> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.inbox
            .send(InboxMsg::Compress {
                session,
                focus,
                ack: ack_tx,
            })
            .await
            .map_err(|_| CoreError::Shutdown)?;
        ack_rx.await.map_err(|_| CoreError::Shutdown)?
    }

    /// Request clean shutdown; worker finishes without orphan tasks.
    pub async fn shutdown(&self) -> Result<(), CoreError> {
        self.inbox
            .send(InboxMsg::Shutdown)
            .await
            .map_err(|_| CoreError::Shutdown)?;
        Ok(())
    }
}

impl WorkerGuard {
    /// Own the native application task using the same shutdown/join contract.
    pub fn from_task(handle: tokio::task::JoinHandle<Result<(), String>>) -> Self {
        Self {
            handle: Some(handle),
        }
    }

    /// Wait for the worker task to finish and surface its cleanup result.
    pub async fn join(mut self) -> Result<(), String> {
        if let Some(handle) = self.handle.take() {
            handle
                .await
                .map_err(|error| format!("application worker join: {error}"))??;
        }
        Ok(())
    }
}

async fn worker_loop(
    provider: MockProvider,
    mut inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
) {
    let mut sessions: HashMap<String, SessionState> = HashMap::new();
    let mut active: Option<ActiveTurn> = None;
    let mut turn_counter: u64 = 0;

    loop {
        if let Some(turn) = active.as_mut() {
            let delay = Duration::from_millis(provider.delay_ms);
            tokio::select! {
                msg = inbox.recv() => {
                    match msg {
                        None => break,
                        Some(InboxMsg::Shutdown) => {
                            let partial = turn.accumulated.clone();
                            let _ = events.send(CoreEvent::TurnInterrupted {
                                session: turn.session.clone(),
                                turn: turn.turn.clone(),
                                partial,
                                duration_ms: elapsed_ms(turn.started),
                            });
                            break;
                        }
                        Some(InboxMsg::Create { id, ack }) => {
                            use std::collections::hash_map::Entry;
                            let res = match sessions.entry(id.0.clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(SessionState::new(id));
                                    Ok(())
                                }
                                Entry::Occupied(_) => Err(CoreError::SessionAlreadyExists),
                            };
                            let _ = ack.send(res);
                        }
                        Some(InboxMsg::Submit { session: _, text: _, ack }) => {
                            let _ = ack.send(Err(CoreError::TurnBusy));
                        }
                        Some(InboxMsg::SubmitFresh { ack, .. }) => {
                            let _ = ack.send(Err(CoreError::TurnBusy));
                        }
                        Some(InboxMsg::Cancel { session, ack }) => {
                            if session == turn.session {
                                let partial = turn.accumulated.clone();
                                let done = CoreEvent::TurnInterrupted {
                                    session: turn.session.clone(),
                                    turn: turn.turn.clone(),
                                    partial,
                                    duration_ms: elapsed_ms(turn.started),
                                };
                                active = None;
                                let _ = events.send(done);
                                let _ = ack.send(Ok(()));
                            } else {
                                let _ = ack.send(Err(CoreError::TurnNotActive));
                            }
                        }
                        Some(InboxMsg::List { ack }) => {
                            let mut ids: Vec<SessionId> =
                                sessions.values().map(|s| s.id.clone()).collect();
                            ids.sort_by(|a, b| a.0.cmp(&b.0));
                            let _ = ack.send(Ok(ids));
                        }
                        Some(InboxMsg::ProbeSession { id, ack }) => {
                            let kind = sessions.get(&id.0).map_or(SessionProbe::Absent, |_| SessionProbe::Root);
                            let _ = ack.send(Ok(kind));
                        }
                        Some(InboxMsg::Read { session, ack }) => {
                            let res = sessions
                                .get(&session.0)
                                .map(|s| s.messages.clone())
                                .ok_or(CoreError::SessionNotFound);
                            let _ = ack.send(res);
                        }
                        Some(other) => scripted_unsupported(other),
                    }
                }
                _ = tokio::time::sleep(delay) => {
                    if turn.index < turn.chunks.len() {
                        let delta = turn.chunks[turn.index].clone();
                        turn.accumulated.push_str(&delta);
                        turn.index += 1;
                        let _ = events.send(CoreEvent::TextDelta {
                            session: turn.session.clone(),
                            turn: turn.turn.clone(),
                            delta,
                        });
                        if turn.index == turn.chunks.len() {
                            let text = turn.accumulated.clone();
                            if let Some(sess) = sessions.get_mut(&turn.session.0) {
                                sess.push(Role::Assistant, text.clone());
                            }
                            let done = CoreEvent::TurnFinished {
                                session: turn.session.clone(),
                                turn: turn.turn.clone(),
                                text,
                                duration_ms: elapsed_ms(turn.started),
                                warnings: Vec::new(),
                            };
                            active = None;
                            let _ = events.send(done);
                        }
                    }
                }
            }
        } else {
            match inbox.recv().await {
                None => break,
                Some(InboxMsg::Shutdown) => break,
                Some(InboxMsg::Create { id, ack }) => {
                    use std::collections::hash_map::Entry;
                    let res = match sessions.entry(id.0.clone()) {
                        Entry::Vacant(entry) => {
                            entry.insert(SessionState::new(id));
                            Ok(())
                        }
                        Entry::Occupied(_) => Err(CoreError::SessionAlreadyExists),
                    };
                    let _ = ack.send(res);
                }
                Some(InboxMsg::Submit { session, text, ack }) => {
                    scripted_accept_submit(
                        &provider,
                        &events,
                        &mut sessions,
                        &mut active,
                        &mut turn_counter,
                        session,
                        text,
                        false,
                        ack,
                    );
                }
                Some(InboxMsg::SubmitFresh {
                    session,
                    text,
                    selection: _,
                    ack,
                }) => {
                    scripted_accept_submit(
                        &provider,
                        &events,
                        &mut sessions,
                        &mut active,
                        &mut turn_counter,
                        session,
                        text,
                        true,
                        ack,
                    );
                }
                Some(InboxMsg::Cancel { session: _, ack }) => {
                    let _ = ack.send(Err(CoreError::TurnNotActive));
                }
                Some(InboxMsg::List { ack }) => {
                    let mut ids: Vec<SessionId> = sessions.values().map(|s| s.id.clone()).collect();
                    ids.sort_by(|a, b| a.0.cmp(&b.0));
                    let _ = ack.send(Ok(ids));
                }
                Some(InboxMsg::ProbeSession { id, ack }) => {
                    let kind = sessions
                        .get(&id.0)
                        .map_or(SessionProbe::Absent, |_| SessionProbe::Root);
                    let _ = ack.send(Ok(kind));
                }
                Some(InboxMsg::Read { session, ack }) => {
                    let res = sessions
                        .get(&session.0)
                        .map(|s| s.messages.clone())
                        .ok_or(CoreError::SessionNotFound);
                    let _ = ack.send(res);
                }
                Some(other) => scripted_unsupported(other),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scripted_accept_submit(
    provider: &MockProvider,
    events: &broadcast::Sender<CoreEvent>,
    sessions: &mut HashMap<String, SessionState>,
    active: &mut Option<ActiveTurn>,
    turn_counter: &mut u64,
    session: SessionId,
    text: String,
    fresh: bool,
    ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
) {
    if text.len() > MAX_INPUT_BYTES {
        let _ = ack.send(Err(CoreError::InputTooLarge));
        return;
    }
    if fresh {
        if text.trim().is_empty() {
            let _ = ack.send(Err(CoreError::Application("empty prompt".to_string())));
            return;
        }
        if sessions.contains_key(&session.0) {
            let _ = ack.send(Err(CoreError::SessionAlreadyExists));
            return;
        }
        sessions.insert(session.0.clone(), SessionState::new(session.clone()));
    }
    let Some(sess) = sessions.get_mut(&session.0) else {
        let _ = ack.send(Err(CoreError::SessionNotFound));
        return;
    };
    *turn_counter += 1;
    let turn_id = WorkerTurnId(format!("t{:04}", turn_counter));
    sess.push(Role::User, text.clone());
    *active = Some(ActiveTurn {
        session: session.clone(),
        turn: turn_id.clone(),
        chunks: provider.plan(&text),
        index: 0,
        accumulated: String::new(),
        started: std::time::Instant::now(),
    });
    let _ = ack.send(Ok(turn_id.clone()));
    let _ = events.send(CoreEvent::TurnStarted {
        session,
        turn: turn_id,
    });
}

/// Whole milliseconds since `started`, saturating at `u64::MAX`.
fn elapsed_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Scripted worker answer for owner-only queries: typed, never silent.
fn scripted_unsupported(message: InboxMsg) {
    let error = || CoreError::Application("query unsupported by scripted worker".to_string());
    match message {
        InboxMsg::History { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ToolOps { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::ToolOutput { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Catalog { ack }
        | InboxMsg::SessionSelection { ack, .. }
        | InboxMsg::HomeSelection { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Skills { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::TabDeck { ack } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SaveTabDeck { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SelectModel { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SelectAgent { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SwitchLocation { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::SwitchLocationHome { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Dcp { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Compress { ack, .. } => {
            let _ = ack.send(Err(error()));
        }
        InboxMsg::Create { .. }
        | InboxMsg::Submit { .. }
        | InboxMsg::SubmitFresh { .. }
        | InboxMsg::Cancel { .. }
        | InboxMsg::List { .. }
        | InboxMsg::ProbeSession { .. }
        | InboxMsg::Read { .. }
        | InboxMsg::Shutdown => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CoreApp, CoreEvent, FreshSelection, InboxMsg, MockProvider, WorkerGuard, WorkerTurnId,
    };
    use crate::domain::SessionId;
    use crate::session::{CoreError, Role};
    use std::time::Duration;

    fn sid(raw: &str) -> SessionId {
        SessionId::new(raw).expect("valid session id")
    }

    async fn collect_until_finished(
        rx: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
        timeout: Duration,
    ) -> (Vec<String>, String) {
        let mut deltas = Vec::new();
        loop {
            let ev = tokio::time::timeout(timeout, rx.recv())
                .await
                .expect("event timeout")
                .expect("event channel");
            match ev {
                CoreEvent::TextDelta { delta, .. } => deltas.push(delta),
                CoreEvent::TurnFinished { text, .. } => return (deltas, text),
                CoreEvent::TurnInterrupted { partial, .. } => {
                    panic!("unexpected interrupt partial={partial}")
                }
                CoreEvent::TurnStarted { .. }
                | CoreEvent::TurnPresentation { .. }
                | CoreEvent::ReasoningDelta { .. }
                | CoreEvent::TurnUsage { .. }
                | CoreEvent::ToolCallStarted { .. }
                | CoreEvent::ToolCallFinished { .. } => {}
                CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            }
        }
    }

    #[tokio::test]
    async fn worker_guard_surfaces_cleanup_failure() {
        let handle = tokio::spawn(async { Err("mcp shutdown failed".to_string()) });
        let error = WorkerGuard::from_task(handle)
            .join()
            .await
            .expect_err("cleanup failure must not be reported as success");
        assert_eq!(error, "mcp shutdown failed");
        let panicked = tokio::spawn(async { panic!("worker panic probe") });
        let error = WorkerGuard::from_task(panicked)
            .join()
            .await
            .expect_err("a panicked worker is a join failure");
        assert!(error.contains("application worker join"), "{error}");
    }

    #[tokio::test]
    async fn vertical_input_to_response() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let mut rx = app.subscribe();
        app.create_session(sid("s-1")).await.expect("create");
        let turn = app
            .submit(sid("s-1"), "hello".to_string())
            .await
            .expect("submit");
        assert_eq!(turn.0, "t0001");

        let (deltas, text) = collect_until_finished(&mut rx, Duration::from_secs(5)).await;
        assert_eq!(deltas, vec!["echo: ".to_string(), "hello".to_string()]);
        assert_eq!(text, "echo: hello");

        let history = app.read_history(sid("s-1")).await.expect("history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, Role::User);
        assert_eq!(history[0].text, "hello");
        assert_eq!(history[1].role, Role::Assistant);
        assert_eq!(history[1].text, "echo: hello");

        let sessions = app.list_sessions().await.expect("list");
        assert_eq!(sessions, vec![sid("s-1")]);

        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn fresh_receipt_waits_for_owner_ack_and_preserves_explicit_selection() {
        let (app, mut inbox, _) = CoreApp::channel(1);
        let selected = FreshSelection {
            agent_id: Some("build".into()),
            model_id: "model-x".into(),
            variant: None,
        };
        let mut receipt = app
            .request_submit_fresh(sid("fresh"), "prompt".into(), Some(selected.clone()))
            .expect("enqueued");
        assert!(receipt.try_result().is_none(), "enqueue is not acceptance");
        let err = app
            .request_submit_fresh(sid("other"), "prompt".into(), None)
            .err()
            .expect("bounded queue must refuse");
        assert_eq!(err, CoreError::QueueFull);
        let Some(InboxMsg::SubmitFresh {
            session,
            text,
            selection,
            ack,
        }) = inbox.recv().await
        else {
            panic!("expected fresh submit");
        };
        assert_eq!(session, sid("fresh"));
        assert_eq!(text, "prompt");
        assert_eq!(selection, Some(selected));
        assert!(receipt.try_result().is_none());
        ack.send(Ok(WorkerTurnId("durable-turn".into())))
            .expect("ack");
        assert_eq!(
            receipt.try_result(),
            Some(Ok(WorkerTurnId("durable-turn".into())))
        );
        assert_eq!(
            app.request_submit_fresh(
                sid("large"),
                "x".repeat(crate::session::MAX_INPUT_BYTES + 1),
                None
            )
            .err(),
            Some(CoreError::InputTooLarge)
        );
        drop(inbox);
        assert_eq!(
            app.request_submit_fresh(sid("closed"), "prompt".into(), None)
                .err(),
            Some(CoreError::Shutdown)
        );
    }

    #[tokio::test]
    async fn mock_fresh_rejects_without_creating_and_keeps_existing_submit_path() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(vec!["reply".into()], 100));
        assert_eq!(
            app.submit_fresh(sid("fresh"), "  \n ".into(), None).await,
            Err(CoreError::Application("empty prompt".into()))
        );
        assert!(app.list_sessions().await.expect("list").is_empty());
        app.create_session(sid("taken")).await.expect("create");
        assert_eq!(
            app.submit_fresh(sid("taken"), "prompt".into(), None).await,
            Err(CoreError::SessionAlreadyExists)
        );
        assert!(
            app.read_history(sid("taken"))
                .await
                .expect("history")
                .is_empty()
        );
        let turn = app
            .submit_fresh(sid("fresh"), "first".into(), None)
            .await
            .expect("accepted");
        assert_eq!(turn, WorkerTurnId("t0001".into()));
        assert_eq!(
            app.list_sessions().await.expect("list"),
            [sid("fresh"), sid("taken")]
        );
        assert_eq!(
            app.read_history(sid("fresh")).await.expect("history")[0].text,
            "first"
        );
        assert_eq!(
            app.submit_fresh(sid("busy"), "second".into(), None).await,
            Err(CoreError::TurnBusy)
        );
        assert_eq!(
            app.read_history(sid("busy")).await,
            Err(CoreError::SessionNotFound)
        );
        app.cancel(sid("fresh")).await.expect("cancel");
        assert_eq!(
            app.submit_fresh(sid("fresh"), "again".into(), None).await,
            Err(CoreError::SessionAlreadyExists)
        );
        assert_eq!(
            app.submit(sid("fresh"), "continued".into()).await,
            Ok(WorkerTurnId("t0002".into()))
        );
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn cancel_long_stream_interrupts() {
        let chunks = (0..10).map(|i| format!("c{i} ")).collect::<Vec<_>>();
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(chunks, 20));
        let mut rx = app.subscribe();
        app.create_session(sid("s-cancel")).await.expect("create");
        app.submit(sid("s-cancel"), "long".to_string())
            .await
            .expect("submit");

        // First delta must arrive before cancel.
        let first = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("event");
        match first {
            CoreEvent::TurnStarted { .. } => {
                let second = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                    .await
                    .expect("timeout")
                    .expect("event");
                assert!(matches!(second, CoreEvent::TextDelta { .. }));
            }
            CoreEvent::TextDelta { .. } => {}
            other => panic!("unexpected first event: {other:?}"),
        }

        app.cancel(sid("s-cancel")).await.expect("cancel");
        // Next terminal event must be Interrupted, never Finished.
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("timeout")
                .expect("event");
            match ev {
                CoreEvent::TurnInterrupted { partial, .. } => {
                    assert!(partial.contains("c0 "), "partial={partial}");
                    assert!(
                        !partial.contains("c9 "),
                        "partial must be partial: {partial}"
                    );
                    break;
                }
                CoreEvent::TurnFinished { text, .. } => {
                    panic!("cancel must not finish text={text}")
                }
                CoreEvent::TextDelta { .. }
                | CoreEvent::TurnPresentation { .. }
                | CoreEvent::TurnStarted { .. }
                | CoreEvent::ReasoningDelta { .. }
                | CoreEvent::TurnUsage { .. }
                | CoreEvent::ToolCallStarted { .. }
                | CoreEvent::ToolCallFinished { .. } => {}
                CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            }
        }

        // Interrupted assistant text is not committed; only user remains.
        let history = app.read_history(sid("s-cancel")).await.expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].role, Role::User);

        // Worker stays usable after cancel.
        let turn = app
            .submit(sid("s-cancel"), "again".to_string())
            .await
            .expect("resubmit");
        assert_eq!(turn.0, "t0002");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn second_submit_while_active_is_busy() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            vec!["a".to_string(), "b".to_string()],
            50,
        ));
        app.create_session(sid("s-busy")).await.expect("create");
        app.submit(sid("s-busy"), "first".to_string())
            .await
            .expect("first");
        let err = app
            .submit(sid("s-busy"), "second".to_string())
            .await
            .expect_err("busy");
        assert_eq!(err, crate::session::CoreError::TurnBusy);
        app.cancel(sid("s-busy")).await.expect("cancel");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn rejects_unknown_session_and_oversize_and_stale_cancel() {
        let (app, guard) = CoreApp::spawn(MockProvider::echo());
        let err = app
            .submit(sid("missing"), "hi".to_string())
            .await
            .expect_err("missing");
        assert_eq!(err, crate::session::CoreError::SessionNotFound);
        app.create_session(sid("s-err")).await.expect("create");
        let dup = app.create_session(sid("s-err")).await.expect_err("dup");
        assert_eq!(dup, crate::session::CoreError::SessionAlreadyExists);
        let big = "x".repeat(crate::session::MAX_INPUT_BYTES + 1);
        let err = app.submit(sid("s-err"), big).await.expect_err("big");
        assert_eq!(err, crate::session::CoreError::InputTooLarge);
        let err = app.cancel(sid("s-err")).await.expect_err("no turn");
        assert_eq!(err, crate::session::CoreError::TurnNotActive);
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }

    #[tokio::test]
    async fn queries_stay_responsive_during_stream() {
        let (app, guard) = CoreApp::spawn(MockProvider::fixed(
            vec!["x".to_string(), "y".to_string(), "z".to_string()],
            50,
        ));
        app.create_session(sid("s-q")).await.expect("create");
        app.submit(sid("s-q"), "prompt".to_string())
            .await
            .expect("submit");
        // List must answer while streaming (worker select, not blocked).
        let sessions = tokio::time::timeout(Duration::from_secs(5), app.list_sessions())
            .await
            .expect("timeout")
            .expect("list");
        assert_eq!(sessions, vec![sid("s-q")]);
        app.cancel(sid("s-q")).await.expect("cancel");
        app.shutdown().await.expect("shutdown");
        guard.join().await.expect("join");
    }
}
