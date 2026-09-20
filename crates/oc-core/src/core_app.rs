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
use crate::session::{CoreError, MAX_INPUT_BYTES, MAX_QUEUE_ITEMS, Message, MessageId, Role};

/// Opaque turn id for the worker (monotonic `t0001`, …).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkerTurnId(pub String);

/// Typed application events (live hints + durable outcomes for T03).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
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
    /// Turn completed; assistant message is now in history.
    TurnFinished {
        /// Session that owns the turn.
        session: SessionId,
        /// Finished turn id.
        turn: WorkerTurnId,
        /// Full assistant text.
        text: String,
    },
    /// Turn was cancelled; partial text was not committed as a message.
    TurnInterrupted {
        /// Session that owns the turn.
        session: SessionId,
        /// Interrupted turn id.
        turn: WorkerTurnId,
        /// Accumulated text at cancel time (not stored).
        partial: String,
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

enum InboxMsg {
    Create {
        id: SessionId,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    Submit {
        session: SessionId,
        text: String,
        ack: oneshot::Sender<Result<WorkerTurnId, CoreError>>,
    },
    Cancel {
        session: SessionId,
        ack: oneshot::Sender<Result<(), CoreError>>,
    },
    List {
        ack: oneshot::Sender<Vec<SessionId>>,
    },
    Read {
        session: SessionId,
        ack: oneshot::Sender<Result<Vec<Message>, CoreError>>,
    },
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
}

/// Cloneable application handle over a bounded worker inbox.
#[derive(Debug, Clone)]
pub struct CoreApp {
    inbox: mpsc::Sender<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
}

/// Worker join guard; await after [`CoreApp::shutdown`].
pub struct WorkerGuard {
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl CoreApp {
    /// Spawn a worker with default bounded capacity.
    pub fn spawn(provider: MockProvider) -> (Self, WorkerGuard) {
        Self::spawn_with_capacity(provider, MAX_QUEUE_ITEMS)
    }

    /// Spawn a worker with explicit inbox capacity (tests use small caps).
    pub fn spawn_with_capacity(provider: MockProvider, capacity: usize) -> (Self, WorkerGuard) {
        let (inbox_tx, inbox_rx) = mpsc::channel(capacity);
        let (event_tx, _) = broadcast::channel(256);
        let app = Self {
            inbox: inbox_tx,
            events: event_tx.clone(),
        };
        let handle = tokio::spawn(worker_loop(provider, inbox_rx, event_tx));
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
        ack_rx.await.map_err(|_| CoreError::Shutdown)
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
    /// Wait for the worker task to finish.
    pub async fn join(mut self) -> Result<(), tokio::task::JoinError> {
        if let Some(handle) = self.handle.take() {
            handle.await?;
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
                        Some(InboxMsg::Cancel { session, ack }) => {
                            if session == turn.session {
                                let partial = turn.accumulated.clone();
                                let done = CoreEvent::TurnInterrupted {
                                    session: turn.session.clone(),
                                    turn: turn.turn.clone(),
                                    partial,
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
                            let _ = ack.send(ids);
                        }
                        Some(InboxMsg::Read { session, ack }) => {
                            let res = sessions
                                .get(&session.0)
                                .map(|s| s.messages.clone())
                                .ok_or(CoreError::SessionNotFound);
                            let _ = ack.send(res);
                        }
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
                    if text.len() > MAX_INPUT_BYTES {
                        let _ = ack.send(Err(CoreError::InputTooLarge));
                        continue;
                    }
                    let Some(sess) = sessions.get_mut(&session.0) else {
                        let _ = ack.send(Err(CoreError::SessionNotFound));
                        continue;
                    };
                    turn_counter += 1;
                    let turn_id = WorkerTurnId(format!("t{:04}", turn_counter));
                    sess.push(Role::User, text.clone());
                    let chunks = provider.plan(&text);
                    active = Some(ActiveTurn {
                        session: session.clone(),
                        turn: turn_id.clone(),
                        chunks,
                        index: 0,
                        accumulated: String::new(),
                    });
                    let _ = events.send(CoreEvent::TurnStarted {
                        session,
                        turn: turn_id.clone(),
                    });
                    let _ = ack.send(Ok(turn_id));
                }
                Some(InboxMsg::Cancel { session: _, ack }) => {
                    let _ = ack.send(Err(CoreError::TurnNotActive));
                }
                Some(InboxMsg::List { ack }) => {
                    let mut ids: Vec<SessionId> = sessions.values().map(|s| s.id.clone()).collect();
                    ids.sort_by(|a, b| a.0.cmp(&b.0));
                    let _ = ack.send(ids);
                }
                Some(InboxMsg::Read { session, ack }) => {
                    let res = sessions
                        .get(&session.0)
                        .map(|s| s.messages.clone())
                        .ok_or(CoreError::SessionNotFound);
                    let _ = ack.send(res);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreApp, CoreEvent, MockProvider};
    use crate::domain::SessionId;
    use crate::session::Role;
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
                CoreEvent::TurnStarted { .. } => {}
            }
        }
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
                CoreEvent::TextDelta { .. } | CoreEvent::TurnStarted { .. } => {}
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
