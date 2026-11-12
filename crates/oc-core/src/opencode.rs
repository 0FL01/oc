//! OpenCode's public projection and the finite operations used by its TUI plugin.
//!
//! These identities and cursors are not native turns, journal watermarks or
//! permission bindings. The adapter holds no execution/storage authority.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::domain::SessionId;
use crate::queries::{InboxDelivery, InboxItem, ModelRef};
use crate::session::CoreError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub agent: Option<String>,
    pub model: Option<ModelRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub parent: Option<String>,
    pub title: Option<String>,
    pub location: String,
    pub selection: Selection,
    pub created: String,
    pub updated: String,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub models: Vec<Model>,
    pub agents: Vec<crate::queries::AgentEntry>,
    /// Agent defaults retain provider identity even when model IDs collide.
    pub agent_models: BTreeMap<String, ModelRef>,
    pub selection: Selection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub provider: String,
    pub entry: crate::queries::ModelEntry,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Init {
    pub location: String,
    pub session: Option<SessionId>,
    pub catalog: Catalog,
}

/// Content order supplies historical ordinals; tool identity remains `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Part {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Tool {
        id: String,
        name: String,
        input: String,
        output: String,
        status: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,
    pub text: String,
    pub agent: Option<String>,
    pub model: Option<ModelRef>,
    pub parts: Vec<Part>,
    pub status: String,
    pub truncated: bool,
}

/// A server-issued page boundary, not a cursor fabricated from a message ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    /// Bridge-local capture epoch, never a server event/journal sequence.
    pub epoch: u64,
    /// Oldest first, unlike the descending OpenCode response.
    pub messages: Vec<Message>,
    pub older: Option<String>,
    pub newer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permission {
    pub id: String,
    pub session: SessionId,
    pub action: String,
    pub resources: Vec<String>,
    pub save: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Form {
    pub id: String,
    pub session: SessionId,
    pub title: String,
    /// None is an unsupported form, which must remain pending at the backend.
    pub questions: Option<Vec<FormQuestion>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormQuestion {
    pub key: String,
    pub label: String,
    pub multiple: bool,
    pub custom: bool,
    pub options: Vec<FormOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct View {
    pub catalog: Catalog,
    pub session: Session,
    pub page: Page,
    pub running: bool,
    pub inbox: Vec<InboxItem>,
    pub permissions: Vec<Permission>,
    pub forms: Vec<Form>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Once,
    Always,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Answer {
    One(String),
    Many(Vec<String>),
}

/// No arbitrary method name, URL, credential or native execution command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Open {
        session: SessionId,
    },
    Sessions {
        search: String,
    },
    History {
        session: SessionId,
        cursor: Option<String>,
    },
    Create {
        selection: Selection,
    },
    Prompt {
        session: SessionId,
        text: String,
        delivery: InboxDelivery,
    },
    Interrupt {
        session: SessionId,
    },
    CancelInput {
        session: SessionId,
        id: String,
    },
    SelectAgent {
        session: SessionId,
        agent: String,
    },
    SelectModel {
        session: SessionId,
        model: ModelRef,
    },
    ReplyPermission {
        session: SessionId,
        id: String,
        decision: PermissionDecision,
        message: Option<String>,
    },
    ReplyForm {
        session: SessionId,
        id: String,
        answer: BTreeMap<String, Answer>,
    },
    CancelForm {
        session: SessionId,
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reply {
    View {
        view: Box<View>,
    },
    Sessions {
        sessions: Vec<Session>,
    },
    History {
        page: Page,
    },
    Created {
        session: Session,
    },
    /// Durable inbox admission only; not execution/assistant-message acceptance.
    Admitted {
        item: InboxItem,
    },
    Selection {
        selection: Selection,
    },
    Done,
}

/// Text/reasoning ordinals have separate public namespaces. Starts establish
/// transient order; canonical message content retains its own ordered array.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    RequestsChanged,
    Started {
        session: SessionId,
        epoch: u64,
        message: Message,
    },
    Message {
        session: SessionId,
        epoch: u64,
        message: Message,
    },
    PartStarted {
        session: SessionId,
        epoch: u64,
        message: String,
        ordinal: usize,
        reasoning: bool,
    },
    ToolStarted {
        session: SessionId,
        epoch: u64,
        message: String,
        id: String,
        name: String,
    },
    Part {
        session: SessionId,
        epoch: u64,
        message: String,
        ordinal: usize,
        part: Part,
    },
    Delta {
        session: SessionId,
        epoch: u64,
        message: String,
        ordinal: usize,
        reasoning: bool,
        delta: String,
    },
    Running {
        session: SessionId,
        running: bool,
    },
    Refresh {
        session: SessionId,
    },
    /// Fail-stop: no reconnect, resend or implicit execution interruption.
    Unavailable {
        reason: String,
    },
}

pub struct Receipt(pub(crate) oneshot::Receiver<Result<Reply, CoreError>>);

impl Receipt {
    pub fn try_result(&mut self) -> Option<Result<Reply, CoreError>> {
        match crate::wake::try_reply(&mut self.0) {
            Ok(result) => Some(result),
            Err(oneshot::error::TryRecvError::Empty) => None,
            Err(oneshot::error::TryRecvError::Closed) => Some(Err(CoreError::Shutdown)),
        }
    }

    pub async fn wait(self) -> Result<Reply, CoreError> {
        self.0.await.map_err(|_| CoreError::Shutdown)?
    }
}
