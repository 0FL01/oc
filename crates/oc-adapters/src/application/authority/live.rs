//! One bounded materialized live projection, captured at emission rather than
//! by replaying an unbounded log or depending on a broadcast receiver's speed.
use oc_core::{
    core_app::{CoreEvent, WorkerTurnId},
    domain::SessionId,
    queries::{LiveTurnSnapshot, TabDeckSnapshot, WorkspaceSnapshot},
};
use std::{collections::BTreeMap, sync::Mutex};
use tokio::sync::broadcast;

const TEXT_BYTES: usize = 256 * 1024;
// Retain a canonical whole-part page alongside its bounded live deltas. Larger
// single parts are still read through canonical history when attaching.
const TURN_BYTES: usize = 2 * crate::storage::TRANSCRIPT_PAGE_BYTES;
const PARTS: usize = 256; // 64 rendered parts plus start/end and pending tool metadata.

#[derive(Clone)]
pub(crate) struct Publication {
    pub(crate) revision: u64,
    pub(crate) origin: Option<u64>,
    pub(crate) event: CoreEvent,
}

#[cfg(test)]
mod tests;
struct Turn {
    location: String,
    view: LiveTurnSnapshot,
    bytes: usize,
}
#[derive(Default)]
struct State {
    revision: u64,
    turns: BTreeMap<String, Turn>,
}
pub(crate) struct Live {
    state: Mutex<State>,
    events: broadcast::Sender<Publication>,
}
impl Default for Live {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            events: broadcast::channel(256).0,
        }
    }
}

pub(crate) fn identity(event: &CoreEvent) -> Option<(&SessionId, &WorkerTurnId)> {
    match event {
        CoreEvent::TurnStarted { session, turn, .. }
        | CoreEvent::TextDelta { session, turn, .. }
        | CoreEvent::ReasoningDelta { session, turn, .. }
        | CoreEvent::ReasoningItemEnded { session, turn }
        | CoreEvent::TurnPresentation { session, turn, .. }
        | CoreEvent::TurnContextUsage { session, turn, .. }
        | CoreEvent::TurnUsage { session, turn, .. }
        | CoreEvent::TurnFinished { session, turn, .. }
        | CoreEvent::TurnInterrupted { session, turn, .. }
        | CoreEvent::TurnFailed { session, turn, .. }
        | CoreEvent::ToolArgumentStream { session, turn, .. }
        | CoreEvent::ToolCallStarted { session, turn, .. }
        | CoreEvent::ToolCallProgress { session, turn, .. }
        | CoreEvent::ToolCallFinished { session, turn, .. }
        | CoreEvent::RetryScheduled { session, turn, .. } => Some((session, turn)),
        _ => None,
    }
}
fn bytes(event: &CoreEvent) -> usize {
    if let CoreEvent::TextDelta { delta, .. } | CoreEvent::ReasoningDelta { delta, .. } = event {
        return delta.len() + 128;
    }
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    let _ = serde_json::to_writer(&mut count, event);
    count.0
}
fn prefix(text: &str, cap: usize) -> &str {
    let mut length = text.len().min(cap);
    while !text.is_char_boundary(length) {
        length -= 1;
    }
    &text[..length]
}
fn same_slot(a: &CoreEvent, b: &CoreEvent) -> bool {
    match (a, b) {
        (CoreEvent::TurnPresentation { .. }, CoreEvent::TurnPresentation { .. })
        | (CoreEvent::TurnUsage { .. }, CoreEvent::TurnUsage { .. })
        | (CoreEvent::TurnContextUsage { .. }, CoreEvent::TurnContextUsage { .. }) => true,
        (CoreEvent::RetryScheduled { span: a, .. }, CoreEvent::RetryScheduled { span: b, .. }) => {
            a == b
        }
        (CoreEvent::ToolCallProgress { op: a, .. }, CoreEvent::ToolCallProgress { op: b, .. }) => {
            a == b
        }
        (
            CoreEvent::ToolArgumentStream { event: a, .. },
            CoreEvent::ToolArgumentStream { event: b, .. },
        ) => {
            use oc_core::tool_stream::ToolStreamEvent;
            matches!((a,b), (ToolStreamEvent::Pending { identity: a, .. }, ToolStreamEvent::Pending { identity: b, .. }) if a == b)
        }
        _ => false,
    }
}
impl Live {
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Publication> {
        self.events.subscribe()
    }
    pub(crate) fn emit(&self, location: &str, origin: Option<u64>, event: CoreEvent) {
        let mut state = self.state.lock().expect("live projection");
        state.revision = state
            .revision
            .checked_add(1)
            .expect("live revision exhausted");
        if let Some((session, turn)) = identity(&event) {
            if let CoreEvent::TurnStarted { model_switch, .. } = &event {
                state.turns.insert(
                    session.0.clone(),
                    Turn {
                        location: location.into(),
                        bytes: bytes(&event),
                        view: LiveTurnSnapshot {
                            session: session.clone(),
                            turn: turn.clone(),
                            origin,
                            events: vec![event.clone()],
                            display_name: model_switch
                                .as_ref()
                                .and_then(|notice| notice.display_name.clone()),
                            truncated: false,
                        },
                    },
                );
            } else if matches!(
                event,
                CoreEvent::TurnFinished { .. }
                    | CoreEvent::TurnInterrupted { .. }
                    | CoreEvent::TurnFailed { .. }
            ) {
                if state
                    .turns
                    .get(&session.0)
                    .is_some_and(|active| active.view.turn == *turn)
                {
                    state.turns.remove(&session.0);
                }
            } else if let Some(active) = state
                .turns
                .get_mut(&session.0)
                .filter(|active| active.view.turn == *turn)
            {
                active.view.events.retain(|previous| {
                    if same_slot(previous, &event) {
                        active.bytes = active.bytes.saturating_sub(bytes(previous));
                        false
                    } else {
                        true
                    }
                });
                let append = match (active.view.events.last_mut(), &event) {
                    (
                        Some(CoreEvent::TextDelta { delta: text, .. }),
                        CoreEvent::TextDelta { delta, .. },
                    )
                    | (
                        Some(CoreEvent::ReasoningDelta { delta: text, .. }),
                        CoreEvent::ReasoningDelta { delta, .. },
                    ) => {
                        let part = prefix(delta, TEXT_BYTES.saturating_sub(text.len()));
                        text.push_str(part);
                        active.bytes += part.len();
                        active.view.truncated |= part.len() != delta.len();
                        false
                    }
                    _ => true,
                };
                if append {
                    let mut part = event.clone();
                    if let CoreEvent::TextDelta { delta, .. }
                    | CoreEvent::ReasoningDelta { delta, .. } = &mut part
                        && delta.len() > TEXT_BYTES
                    {
                        delta.truncate(prefix(delta, TEXT_BYTES).len());
                        active.view.truncated = true;
                    }
                    active.bytes += bytes(&part);
                    active.view.events.push(part);
                }
                while active.view.events.len() > 1
                    && (active.bytes > TURN_BYTES || active.view.events.len() > PARTS)
                {
                    active.bytes = active
                        .bytes
                        .saturating_sub(bytes(&active.view.events.remove(1)));
                    active.view.truncated = true;
                }
            }
        }
        let publication = Publication {
            revision: state.revision,
            origin,
            event,
        };
        let _ = self.events.send(publication);
    }
    pub(crate) fn snapshot(
        &self,
        deck: TabDeckSnapshot,
        selected: Option<&SessionId>,
    ) -> WorkspaceSnapshot {
        let state = self.state.lock().expect("live projection");
        let turns = state
            .turns
            .values()
            .filter(|active| {
                active.location == deck.location
                    && (deck.sessions.contains(&active.view.session)
                        || selected == Some(&active.view.session))
            })
            .map(|active| active.view.clone())
            .collect();
        WorkspaceSnapshot {
            inbox: Vec::new(),
            revision: state.revision,
            deck,
            turns,
        }
    }
}
