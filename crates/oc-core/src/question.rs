//! Typed question exchange. Permission consent is deliberately a separate protocol.
use crate::{approval::ApprovalBinding, core_app::CoreEvent};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{broadcast, oneshot};

pub const QUESTIONS_CAP: usize = 16;
pub const OPTIONS_CAP: usize = 16;
pub const PAYLOAD_CAP: usize = 64 * 1024;
pub const RESULT_BYTES_CAP: usize = 2 * PAYLOAD_CAP + 64;
pub const ANSWER_BYTES_CAP: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionPrompt {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    #[serde(default)]
    pub multiple: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionInput {
    pub questions: Vec<QuestionPrompt>,
}
impl QuestionInput {
    pub fn parse(value: &serde_json::Value) -> Result<Self, &'static str> {
        if value.to_string().len() > PAYLOAD_CAP {
            return Err("question payload too large");
        }
        let input: Self =
            serde_json::from_value(value.clone()).map_err(|_| "invalid question schema")?;
        input.validate()?;
        Ok(input)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.questions.is_empty() || self.questions.len() > QUESTIONS_CAP {
            return Err("expected 1..16 questions");
        }
        if serde_json::to_vec(self).map_or(true, |v| v.len() > PAYLOAD_CAP) {
            return Err("question payload too large");
        }
        for q in &self.questions {
            if q.question.trim().is_empty()
                || q.question.len() > 4096
                || q.header.trim().is_empty()
                || q.header.chars().count() > 30
                || q.options.len() > OPTIONS_CAP
                || q.options.iter().any(|o| {
                    o.label.trim().is_empty() || o.label.len() > 256 || o.description.len() > 4096
                })
                || q.options
                    .iter()
                    .enumerate()
                    .any(|(i, o)| q.options[..i].iter().any(|p| p.label == o.label))
            {
                return Err("invalid question fields or limits");
            }
        }
        Ok(())
    }
    pub fn validate_answers(&self, answers: &[Vec<String>]) -> Result<(), QuestionReplyError> {
        if answers.len() != self.questions.len()
            || serde_json::to_vec(answers).map_or(true, |v| v.len() > PAYLOAD_CAP)
            || self.questions.iter().zip(answers).any(|(q, a)| {
                a.len() > if q.multiple { OPTIONS_CAP + 1 } else { 1 }
                    || a.iter().any(|s| s.is_empty() || s.len() > ANSWER_BYTES_CAP)
                    || a.iter().enumerate().any(|(i, s)| a[..i].contains(s))
            })
        {
            return Err(QuestionReplyError::InvalidAnswers);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionRequest {
    pub id: u64,
    pub binding: ApprovalBinding,
    pub input: QuestionInput,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionDecision {
    Answers(Vec<Vec<String>>),
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionReply {
    pub id: u64,
    pub binding: ApprovalBinding,
    pub decision: QuestionDecision,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionReplyError {
    Stale,
    BindingMismatch,
    InvalidAnswers,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionWaitError {
    NoConsumer,
    InvalidInput,
    Capacity,
    OwnerClosed,
}
impl std::fmt::Display for QuestionReplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stale => "question request is stale or already settled",
            Self::BindingMismatch => "question reply binding mismatch",
            Self::InvalidAnswers => "invalid question answer count or size",
            Self::Unavailable => "question owner unavailable",
        })
    }
}
impl std::error::Error for QuestionReplyError {}

/// Durable answered presentation; decoded only by the tool-result owner, never a form source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionResult {
    pub questions: Vec<QuestionPrompt>,
    pub answers: Vec<Vec<String>>,
}
impl QuestionResult {
    pub fn from_output(name: &str, state: &str, output: Option<&str>) -> Option<Self> {
        if name != "question" || state != "completed" {
            return None;
        }
        let output = output?;
        if output.len() > RESULT_BYTES_CAP {
            return None;
        }
        let result: Self = serde_json::from_str(output).ok()?;
        let input = QuestionInput {
            questions: result.questions.clone(),
        };
        input.validate().ok()?;
        input.validate_answers(&result.answers).ok()?;
        Some(result)
    }
}

struct Pending {
    request: QuestionRequest,
    reply: oneshot::Sender<QuestionDecision>,
}
#[derive(Default)]
struct State {
    sequence: u64,
    consumer: bool,
    pending: BTreeMap<u64, Pending>,
}
#[derive(Default)]
pub struct QuestionQueue(Mutex<State>);
impl QuestionQueue {
    pub fn register_consumer(&self) {
        self.0.lock().expect("question lock").consumer = true;
    }
    pub fn pending(&self) -> Vec<QuestionRequest> {
        self.0
            .lock()
            .expect("question lock")
            .pending
            .values()
            .map(|p| p.request.clone())
            .collect()
    }
    pub fn resolve(
        &self,
        reply: QuestionReply,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<(), QuestionReplyError> {
        let mut state = self.0.lock().expect("question lock");
        let pending = state
            .pending
            .get(&reply.id)
            .ok_or(QuestionReplyError::Stale)?;
        if pending.request.binding != reply.binding {
            return Err(QuestionReplyError::BindingMismatch);
        }
        if pending.reply.is_closed() {
            state.pending.remove(&reply.id);
            return Err(QuestionReplyError::Stale);
        }
        if let QuestionDecision::Answers(answers) = &reply.decision {
            pending.request.input.validate_answers(answers)?;
        }
        let pending = state.pending.remove(&reply.id).expect("checked question");
        let _ = events.send(CoreEvent::QuestionResolved {
            request: pending.request,
            decision: reply.decision.clone(),
        });
        pending
            .reply
            .send(reply.decision)
            .map_err(|_| QuestionReplyError::Stale)
    }
    pub fn cancel(&self, session: Option<&str>, events: &broadcast::Sender<CoreEvent>) {
        for request in self
            .pending()
            .into_iter()
            .filter(|r| session.is_none_or(|s| s == r.binding.session))
        {
            let _ = self.resolve(
                QuestionReply {
                    id: request.id,
                    binding: request.binding,
                    decision: QuestionDecision::Cancelled,
                },
                events,
            );
        }
    }
    pub async fn wait(
        self: &Arc<Self>,
        mut request: QuestionRequest,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<QuestionDecision, QuestionWaitError> {
        request
            .input
            .validate()
            .map_err(|_| QuestionWaitError::InvalidInput)?;
        if [
            &request.binding.session,
            &request.binding.turn,
            &request.binding.call,
            &request.binding.operation,
            &request.binding.input_digest,
            &request.binding.location,
        ]
        .iter()
        .map(|s| s.len())
        .sum::<usize>()
            + request.binding.agent.as_ref().map_or(0, String::len)
            + request.binding.agent_digest.as_ref().map_or(0, String::len)
            > 16 * 1024
        {
            return Err(QuestionWaitError::InvalidInput);
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut state = self.0.lock().expect("question lock");
            if !state.consumer {
                return Err(QuestionWaitError::NoConsumer);
            }
            if state.pending.len() >= 64 {
                return Err(QuestionWaitError::Capacity);
            }
            state.sequence = state
                .sequence
                .checked_add(1)
                .ok_or(QuestionWaitError::Capacity)?;
            request.id = state.sequence;
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
        let _ = events.send(CoreEvent::QuestionAsked(request));
        let result = rx.await.map_err(|_| QuestionWaitError::OwnerClosed);
        drop(guard);
        result
    }
}
struct WaitGuard {
    queue: Arc<QuestionQueue>,
    id: u64,
    events: broadcast::Sender<CoreEvent>,
}
impl Drop for WaitGuard {
    fn drop(&mut self) {
        if let Some(pending) = self
            .queue
            .0
            .lock()
            .expect("question lock")
            .pending
            .remove(&self.id)
        {
            let _ = self.events.send(CoreEvent::QuestionResolved {
                request: pending.request,
                decision: QuestionDecision::Cancelled,
            });
        }
    }
}

#[cfg(test)]
mod tests;
