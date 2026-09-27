//! Invocation-bound permission exchange. Waiting is transient, never execution intent.
use crate::core_app::CoreEvent;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, oneshot};

/// Owner-prepared display data; consumers never inspect the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalPreview {
    Resource {
        values: Vec<String>,
    },
    Shell {
        command: String,
        cwd: String,
    },
    Search {
        pattern: String,
        cwd: String,
        include: Option<String>,
    },
    Patch {
        files: Vec<crate::patch::FileEffect>,
        total_files: usize,
        truncated: bool,
    },
}

/// Exact identity echoed by a reply, including immutable invocation digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalBinding {
    pub session: String,
    pub turn: String,
    pub call: String,
    pub operation: String,
    pub input_digest: String,
    pub location: String,
    pub generation: u64,
    pub agent: Option<String>,
    pub agent_digest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub id: u64,
    pub binding: ApprovalBinding,
    pub project: String,
    pub action: String,
    pub resources: Vec<String>,
    pub save_patterns: Vec<String>,
    pub preview: ApprovalPreview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Once,
    Always,
    Reject { feedback: Option<String> },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalReply {
    pub id: u64,
    pub binding: ApprovalBinding,
    pub decision: ApprovalDecision,
}

struct Pending {
    request: ApprovalRequest,
    reply: oneshot::Sender<ApprovalDecision>,
}
#[derive(Default)]
struct State {
    sequence: u64,
    consumer: bool,
    auto_once: bool,
    pending: BTreeMap<u64, Pending>,
}

/// One owner queue, bounded independently of provider streams/history.
#[derive(Default)]
pub struct ApprovalQueue(Mutex<State>);

impl ApprovalQueue {
    pub fn register_consumer(&self, auto_once: bool) {
        let mut state = self.0.lock().expect("approval lock");
        state.consumer = true;
        state.auto_once = auto_once;
    }
    pub fn pending(&self) -> Vec<ApprovalRequest> {
        self.0
            .lock()
            .expect("approval lock")
            .pending
            .values()
            .map(|p| p.request.clone())
            .collect()
    }
    /// Called only by the owner; persist Always before invoking this method.
    pub fn resolve(
        &self,
        reply: ApprovalReply,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<(), &'static str> {
        self.resolve_with(reply, events, |_| Ok(()))
    }
    /// Validation and storage happen under one short owner decision, never a wait.
    pub fn resolve_with(
        &self,
        reply: ApprovalReply,
        events: &broadcast::Sender<CoreEvent>,
        persist: impl FnOnce(&ApprovalRequest) -> Result<(), &'static str>,
    ) -> Result<(), &'static str> {
        let mut state = self.0.lock().expect("approval lock");
        let pending = state.pending.get(&reply.id).ok_or("stale approval reply")?;
        if pending.request.binding != reply.binding {
            return Err("approval binding mismatch");
        }
        if pending.reply.is_closed() {
            state.pending.remove(&reply.id);
            return Err("cancelled approval");
        }
        persist(&pending.request)?;
        let pending = state.pending.remove(&reply.id).expect("checked pending");
        let _ = events.send(CoreEvent::PermissionResolved {
            request: pending.request,
            decision: reply.decision.clone(),
        });
        pending
            .reply
            .send(reply.decision)
            .map_err(|_| "cancelled approval")
    }
    pub fn cancel(&self, session: Option<&str>, events: &broadcast::Sender<CoreEvent>) {
        let requests = self.pending();
        for request in requests
            .into_iter()
            .filter(|r| session.is_none_or(|s| s == r.binding.session))
        {
            let _ = self.resolve(
                ApprovalReply {
                    id: request.id,
                    binding: request.binding,
                    decision: ApprovalDecision::Cancelled,
                },
                events,
            );
        }
    }
    pub async fn wait(
        self: &Arc<Self>,
        mut request: ApprovalRequest,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<ApprovalDecision, &'static str> {
        let strings = request.resources.iter().chain(&request.save_patterns);
        let preview_bytes = match &request.preview {
            ApprovalPreview::Resource { values } => values.iter().map(String::len).sum(),
            ApprovalPreview::Shell { command, cwd } => command.len() + cwd.len(),
            ApprovalPreview::Search {
                pattern,
                cwd,
                include,
            } => pattern.len() + cwd.len() + include.as_ref().map_or(0, String::len),
            ApprovalPreview::Patch { files, .. } => {
                serde_json::to_vec(files).map_or(usize::MAX, |bytes| bytes.len())
            }
        };
        if request.resources.len() > 256
            || request.save_patterns.len() > 256
            || strings.map(String::len).sum::<usize>() > 128 * 1024
            || preview_bytes > crate::patch::EFFECT_PREVIEW_BYTES_CAP
            || [
                &request.binding.session,
                &request.binding.turn,
                &request.binding.call,
                &request.binding.operation,
                &request.binding.input_digest,
                &request.binding.location,
                &request.project,
                &request.action,
            ]
            .iter()
            .map(|s| s.len())
            .sum::<usize>()
                > 16 * 1024
        {
            return Err("approval request too large");
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.0.lock().expect("approval lock");
            if !state.consumer {
                return Err("approval required: no consumer");
            }
            if state.pending.len() >= 64 {
                return Err("approval queue full");
            }
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or("approval identity exhausted")?;
            request.id = state.sequence;
            if state.auto_once {
                let _ = events.send(CoreEvent::PermissionAsked(request.clone()));
                let _ = events.send(CoreEvent::PermissionResolved {
                    request,
                    decision: ApprovalDecision::Once,
                });
                return Ok(ApprovalDecision::Once);
            }
            state.pending.insert(
                request.id,
                Pending {
                    request: request.clone(),
                    reply: tx,
                },
            );
        }
        let guard = WaitGuard {
            queue: self.clone(),
            id: request.id,
            events: events.clone(),
        };
        let _ = events.send(CoreEvent::PermissionAsked(request));
        let result = rx.await.map_err(|_| "approval owner closed");
        drop(guard);
        result
    }
}

struct WaitGuard {
    queue: Arc<ApprovalQueue>,
    id: u64,
    events: broadcast::Sender<CoreEvent>,
}
impl Drop for WaitGuard {
    fn drop(&mut self) {
        if let Some(pending) = self
            .queue
            .0
            .lock()
            .expect("approval lock")
            .pending
            .remove(&self.id)
        {
            let _ = self.events.send(CoreEvent::PermissionResolved {
                request: pending.request,
                decision: ApprovalDecision::Cancelled,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> ApprovalRequest {
        ApprovalRequest {
            id: 0,
            binding: ApprovalBinding {
                session: "s".into(),
                turn: "t".into(),
                call: "c".into(),
                operation: "op".into(),
                input_digest: "digest".into(),
                location: "location".into(),
                generation: 1,
                agent: None,
                agent_digest: None,
            },
            project: "p".into(),
            action: "read".into(),
            resources: vec!["file".into()],
            save_patterns: vec!["file".into()],
            preview: ApprovalPreview::Resource {
                values: vec!["file".into()],
            },
        }
    }
    #[tokio::test]
    async fn dropped_waiter_removes_authoritative_pending_and_emits_cancelled() {
        let queue = Arc::new(ApprovalQueue::default());
        queue.register_consumer(false);
        let (tx, mut rx) = broadcast::channel(8);
        let task = tokio::spawn({
            let queue = queue.clone();
            let tx = tx.clone();
            async move { queue.wait(request(), &tx).await }
        });
        let CoreEvent::PermissionAsked(request) = rx.recv().await.unwrap() else {
            panic!("asked");
        };
        assert_eq!(queue.pending(), vec![request.clone()]);
        task.abort();
        let _ = task.await;
        assert!(queue.pending().is_empty());
        assert!(matches!(
            rx.recv().await.unwrap(),
            CoreEvent::PermissionResolved {
                decision: ApprovalDecision::Cancelled,
                ..
            }
        ));
        assert!(
            queue
                .resolve(
                    ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: ApprovalDecision::Always
                    },
                    &tx
                )
                .is_err()
        );
    }
    #[tokio::test]
    async fn auto_once_requires_explicit_consumer_and_never_saves() {
        let queue = Arc::new(ApprovalQueue::default());
        let (tx, mut rx) = broadcast::channel(8);
        assert_eq!(
            queue.wait(request(), &tx).await,
            Err("approval required: no consumer")
        );
        queue.register_consumer(true);
        assert_eq!(queue.wait(request(), &tx).await, Ok(ApprovalDecision::Once));
        assert!(queue.pending().is_empty());
        assert!(matches!(
            rx.recv().await.unwrap(),
            CoreEvent::PermissionAsked(_)
        ));
        assert!(matches!(
            rx.recv().await.unwrap(),
            CoreEvent::PermissionResolved {
                decision: ApprovalDecision::Once,
                ..
            }
        ));
    }
}
