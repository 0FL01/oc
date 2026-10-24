//! A semantic mirror of InboxMsg. Receipts, cancellation flags and secrets keep
//! their existing owners; no frontend handle or generic transport trait changes.
use oc_core::{
    core_app::{CoreEvent, FreshSelection, InboxMsg, UserShellSelection, WorkerTurnId},
    domain::SessionId,
    queries::*,
    session::{CoreError, Message, MessageId},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::oneshot;

use super::secrets;

pub(super) fn unknown() -> CoreError {
    CoreError::Application(
        "owner connection lost; operation outcome unknown; reload before retrying".into(),
    )
}

fn cancel_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

pub(super) type Reply =
    futures_util::future::BoxFuture<'static, Result<serde_json::Value, CoreError>>;

macro_rules! rpc {
    (
        $( $name:ident { $( $(#[$attr:meta])* $field:ident : $ty:ty ),* $(,)? } -> $out:ty; )*
        @special { $( $special:ident { $( $(#[$sattr:meta])* $sfield:ident : $sty:ty ),* $(,)? }
            -> $sout:ty, $error:ty, $failure:expr; )* }
    ) => {
        #[derive(Serialize, Deserialize)]
        pub(super) enum Request {
            $( $name { $( $(#[$attr])* $field: $ty ),* }, )*
            $( $special { $( $(#[$sattr])* $sfield: $sty ),* }, )*
        }
        enum Ack {
            $( $name(oneshot::Sender<Result<$out, CoreError>>), )*
            $( $special(oneshot::Sender<Result<$sout, $error>>), )*
        }
        impl Pending {
            pub(super) fn from_inbox(message: InboxMsg) -> Option<(Request, Self)> {
                let (request, ack) = match message {
                    $( InboxMsg::$name { $( $field, )* ack } => (Request::$name { $( $field, )* }, Ack::$name(ack)), )*
                    $( InboxMsg::$special { $( $sfield, )* ack } => (Request::$special { $( $sfield, )* }, Ack::$special(ack)), )*
                    InboxMsg::Shutdown => return None,
                };
                let cancel = request.cancellation();
                Some((request, Self { ack, cancel, cancel_sent: false }))
            }
            pub(super) fn finish(self, value: Result<serde_json::Value, CoreError>) {
                match self.ack {
                    $( Ack::$name(ack) => {
                        let result = value.and_then(|value| serde_json::from_value::<Result<$out, CoreError>>(value).map_err(|_| unknown())).and_then(|result| result);
                        let _ = ack.send(result);
                    }, )*
                    $( Ack::$special(ack) => {
                        let result = value.ok().and_then(|value| serde_json::from_value(value).ok()).unwrap_or_else(|| Err($failure));
                        let _ = ack.send(result);
                    }, )*
                }
            }
        }
        impl Request {
            pub(super) fn into_local(self) -> (InboxMsg, Reply) {
                match self {
                    $( Self::$name { $( $field, )* } => {
                        let (ack, reply) = oneshot::channel();
                        (InboxMsg::$name { $( $field, )* ack }, Box::pin(async move {
                            serde_json::to_value(reply.await.map_err(|_| unknown())?).map_err(|_| unknown())
                        }))
                    }, )*
                    $( Self::$special { $( $sfield, )* } => {
                        let (ack, reply) = oneshot::channel();
                        (InboxMsg::$special { $( $sfield, )* ack }, Box::pin(async move {
                            serde_json::to_value(reply.await.map_err(|_| unknown())?).map_err(|_| unknown())
                        }))
                    }, )*
                }
            }
        }
    };
}

rpc! {
    SessionViewed { session: SessionId, idle_seq: u64 } -> SessionActivity;
    Workspace { selected: Option<SessionId> } -> WorkspaceSnapshot;
    UserShell { session: SessionId, command: String, selection: UserShellSelection,
        #[serde(skip, default="cancel_flag")] cancel: Arc<AtomicBool> } -> String;
    PromptHistory { append: Option<String> } -> Vec<String>;
    AuthMethods { provider: String } -> Vec<AuthMethod>;
    Authenticate { provider: String, action: AuthAction } -> AuthAttempt;
    Terminal { session: SessionId, action: TerminalAction } -> TerminalReceipt;
    ProviderConnections {} -> Vec<ProviderConnection>;
    ModelCatalogs {} -> ModelCatalogs;
    ModelPreference { action: Option<ModelPreferenceAction> } -> ModelPreference;
    SidebarMcpOpen { open: Option<bool> } -> bool;
    CustomProvider { #[serde(with="secrets::custom_provider")] action: CustomProviderAction } -> CustomProviderReply;
    ProviderAccounts { provider: String, #[serde(with="secrets::account")] action: Option<AccountAction> } -> ProviderAccounts;
    McpStatus {} -> McpSnapshot;
    McpControl { control: McpControl } -> McpSnapshot;
    PendingApprovals {} -> Vec<oc_core::approval::ApprovalRequest>;
    PendingQuestions {} -> Vec<oc_core::question::QuestionRequest>;
    RegisterQuestionConsumer {} -> ();
    ReplyApproval { reply: oc_core::approval::ApprovalReply } -> ();
    RegisterApprovalConsumer { auto_once: bool, preference: Option<bool> } -> ();
    CompactSession { session: SessionId } -> oc_core::compaction::CompactionSnapshot;
    CancelCompaction { session: SessionId } -> ();
    CompactionHistory { session: SessionId } -> Vec<oc_core::compaction::CompactionSnapshot>;
    ChangeConversation { session: SessionId, action: ConversationAction } -> ConversationSnapshot;
    ForkSession { source: SessionId, before: MessageId } -> ForkSessionSnapshot;
    Create { id: SessionId } -> ();
    RenameSession { session: SessionId, title: String } -> ();
    DeleteSession { session: SessionId } -> TabDeckSnapshot;
    PickerSessionAction { session: SessionId, search: String, all_projects: bool, action: SessionPickerAction } -> SessionPickerResult;
    OpenPickerSession { session: SessionId, search: String, all_projects: bool, old_deck: TabDeckSnapshot } -> SessionPickerOpen;
    SessionPickerContext { all_projects: Option<bool> } -> SessionPickerContext;
    RegenerateTitle { session: SessionId } -> String;
    CancelTitle { session: SessionId } -> ();
    Submit { session: SessionId, text: oc_core::prompt::PromptInput, selection: Option<ModelCommit> } -> WorkerTurnId;
    Enqueue { session: SessionId, text: oc_core::prompt::PromptInput, delivery: InboxDelivery, selection: Option<ModelCommit> } -> InboxItem;
    SubmitInbox { session: SessionId, id: String } -> WorkerTurnId;
    Inbox { session: SessionId, action: InboxAction } -> InboxSnapshot;
    SubmitFresh { session: SessionId, text: oc_core::prompt::PromptInput, selection: Option<FreshSelection> } -> WorkerTurnId;
    Cancel { session: SessionId } -> ();
    List {} -> Vec<SessionId>;
    SessionList { search: String, all_projects: bool } -> Vec<SessionListEntry>;
    ProbeSession { id: SessionId } -> SessionProbe;
    TabDeck {} -> TabDeckSnapshot;
    SaveTabDeck { deck: TabDeckSnapshot } -> TabDeckSnapshot;
    Read { session: SessionId } -> Vec<Message>;
    History { session: SessionId, message: Option<MessageId>, before_seq: Option<i64>, after_seq: Option<i64>, limit: usize, window: Option<HistoryReadWindow> } -> HistoryPage;
    ToolOps { session: SessionId, before_rowid: Option<i64>, limit: usize } -> ToolOpPage;
    ToolOutput { session: SessionId, op: String, offset: usize, limit: usize } -> ToolOutputPage;
    ShellOutput { session: SessionId, shell_id: String, offset: usize, limit: usize } -> Option<ToolOutputPage>;
    CancelShell { session: SessionId, shell_id: String } -> ();
    ShellJobs { session: SessionId } -> Vec<ShellJob>;
    ChildJobs { session: SessionId } -> Vec<ChildJob>;
    InterruptChild { session: SessionId, selected: ChildJob } -> ();
    BackgroundChild { session: SessionId, selected: ChildJob } -> ();
    ReadChild { session: SessionId, selected: ChildJob, window: Option<HistoryReadWindow> } -> HistoryPage;
    ShellSnapshot { session: SessionId, shell_id: String } -> ShellSnapshot;
    BackgroundShell { session: SessionId, shell_id: String } -> ();
    DcpSummary { session: SessionId, op: String, block_index: usize, offset: i64, limit: usize } -> Option<oc_core::dcp_view::DcpSummaryPage>;
    Catalog {} -> CatalogSnapshot;
    ProviderCatalog { provider: String } -> CatalogSnapshot;
    SessionSelection { session: SessionId, home: bool, action: SessionSelectionAction } -> CatalogSnapshot;
    HomeSelection { action: SessionSelectionAction } -> CatalogSnapshot;
    Skills {} -> Vec<SkillCard>;
    FileSuggestions { query: String, limit: usize } -> FileSuggestionsSnapshot;
    SelectModel { id: String, variant: Option<String> } -> CatalogSnapshot;
    SelectAgent { id: String } -> CatalogSnapshot;
    SwitchLocation { path: String } -> LocationSnapshot;
    SwitchLocationHome { path: String } -> HomeLocationSnapshot;
    ReloadLocation {} -> ReloadLocationSnapshot;
    Dcp { session: SessionId } -> DcpSnapshot;
    Compress { session: SessionId, focus: String } -> WorkerTurnId;
    @special {
        McpLookup { query: McpLookup, #[serde(skip, default="cancel_flag")] cancel: Arc<AtomicBool> }
            -> McpLookupReply, McpLookupError, McpLookupError::Transport;
        ReplyQuestion { reply: oc_core::question::QuestionReply }
            -> (), oc_core::question::QuestionReplyError, oc_core::question::QuestionReplyError::OutcomeUnknown;
    }
}

pub(super) struct Pending {
    ack: Ack,
    cancel: Option<Arc<AtomicBool>>,
    cancel_sent: bool,
}

impl Pending {
    pub(super) fn has_cancellation(&self) -> bool {
        self.cancel.is_some() && !self.cancel_sent
    }
    pub(super) fn take_cancellation(&mut self) -> bool {
        if self.has_cancellation()
            && self
                .cancel
                .as_ref()
                .is_some_and(|flag| flag.load(Ordering::Acquire))
        {
            self.cancel_sent = true;
            return true;
        }
        false
    }
}

impl Request {
    pub(super) fn bind_attachment(&mut self, attachment: u64) {
        let commit = match self {
            Self::Submit {
                selection: Some(commit),
                ..
            }
            | Self::Enqueue {
                selection: Some(commit),
                ..
            }
            | Self::UserShell {
                selection: UserShellSelection::Existing(Some(commit)),
                ..
            }
            | Self::SessionSelection {
                action:
                    SessionSelectionAction::Commit(commit) | SessionSelectionAction::AgentCommit(commit),
                ..
            }
            | Self::HomeSelection {
                action: SessionSelectionAction::Commit(commit),
                ..
            } => Some(commit),
            _ => None,
        };
        if let Some(commit) = commit {
            commit.attachment = attachment;
        }
    }

    pub(super) fn cancellation(&self) -> Option<Arc<AtomicBool>> {
        match self {
            Self::UserShell { cancel, .. } | Self::McpLookup { cancel, .. } => Some(cancel.clone()),
            Self::CustomProvider {
                action: CustomProviderAction::Probe { cancel, .. },
            } => Some(cancel.clone()),
            _ => None,
        }
    }
}

/// Private event envelope. Catalog labels are public projection data; storage
/// writes only model references, independently of this live encoding.
#[derive(Serialize, Deserialize)]
pub(super) struct Event {
    pub event: CoreEvent,
    display_name: Option<String>,
}
impl From<CoreEvent> for Event {
    fn from(event: CoreEvent) -> Self {
        let display_name = match &event {
            CoreEvent::TurnStarted {
                model_switch: Some(notice),
                ..
            } => notice.display_name.clone(),
            _ => None,
        };
        Self {
            event,
            display_name,
        }
    }
}
impl Event {
    pub(super) fn into_core(mut self) -> CoreEvent {
        if let CoreEvent::TurnStarted {
            model_switch: Some(notice),
            ..
        } = &mut self.event
        {
            notice.display_name = self.display_name;
        }
        self.event
    }
}
