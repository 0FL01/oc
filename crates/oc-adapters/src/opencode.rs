//! Private inherited-descriptor bridge to the already-connected OpenCode plugin.
//! No service discovery, native application, database or provider is opened here.

use std::collections::HashMap;
use std::os::fd::FromRawFd;

use oc_core::core_app::{CoreApp, CoreEvent, InboxMsg, NativeControl, WorkerGuard};
use oc_core::domain::SessionId;
use oc_core::opencode::{Event, Init, Reply, Request, Selection};
use oc_core::queries::{self, CatalogSnapshot, SessionSelectionAction, TabDeckSnapshot};
use oc_core::session::CoreError;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

const VERSION: u32 = 1;
const MAX_FRAME: usize = 4 * 1024 * 1024;
const CAPACITY: usize = 64;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HostFrame {
    Activate {
        version: u32,
        init: Init,
    },
    Response {
        id: u64,
        reply: Option<Reply>,
        error: Option<String>,
        unknown: bool,
    },
    Event {
        event: Event,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChildFrame {
    Hello { version: u32 },
    Request { id: u64, request: Request },
    Return { session: Option<SessionId> },
}

pub struct Bridge {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    pub init: Init,
}

impl Bridge {
    /// Only descriptors 3/4 are touched before a compatible Activate frame.
    pub async fn handshake() -> Result<Self, String> {
        for descriptor in [3, 4] {
            let mut stat = std::mem::MaybeUninit::uninit();
            // SAFETY: fstat writes a stat only on success and does not read tty data.
            if unsafe { libc::fstat(descriptor, stat.as_mut_ptr()) } != 0 {
                return Err("OpenCode bridge descriptor unavailable".into());
            }
            // SAFETY: the successful fstat initialized this value.
            if unsafe { stat.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFSOCK {
                return Err("OpenCode bridge requires inherited sockets".into());
            }
        }
        // SAFETY: the private CLI role exclusively owns these two descriptors.
        // The parent passes connected sockets, not paths/authentication material.
        let input = unsafe { std::os::unix::net::UnixStream::from_raw_fd(3) };
        // SAFETY: descriptor 4 is distinct from descriptor 3 and is owned once.
        let output = unsafe { std::os::unix::net::UnixStream::from_raw_fd(4) };
        input.set_nonblocking(true).map_err(|_| "bridge input")?;
        output.set_nonblocking(true).map_err(|_| "bridge output")?;
        let mut reader = BufReader::new(UnixStream::from_std(input).map_err(|_| "bridge input")?);
        let mut writer = UnixStream::from_std(output).map_err(|_| "bridge output")?;
        write_frame(&mut writer, &ChildFrame::Hello { version: VERSION }).await?;
        let mut buffer = Vec::new();
        let frame = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            read_frame(&mut reader, &mut buffer),
        )
        .await
        .map_err(|_| "bridge activation timeout")??;
        let HostFrame::Activate {
            version: VERSION,
            init,
        } = frame
        else {
            return Err("incompatible OpenCode bridge".into());
        };
        if init.location.is_empty() {
            return Err("OpenCode location unavailable".into());
        }
        Ok(Self {
            reader,
            writer,
            init,
        })
    }

    pub fn start(self) -> (CoreApp, WorkerGuard) {
        let (app, inbox, control, events) = CoreApp::opencode_channel(CAPACITY, self.init.clone());
        let task = tokio::spawn(async move {
            let result = run(self, inbox, control, &events).await;
            if let Err(reason) = result {
                let _ = events.send(CoreEvent::OpenCode(Box::new(Event::Unavailable { reason })));
            }
            Ok(())
        });
        (app, WorkerGuard::from_diagnostic_task(task))
    }
}

async fn read_frame(
    reader: &mut BufReader<UnixStream>,
    bytes: &mut Vec<u8>,
) -> Result<HostFrame, String> {
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| "OpenCode bridge read failed")?;
        if available.is_empty() {
            return Err("OpenCode bridge disconnected; no requests were resent".into());
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |at| at + 1);
        if bytes.len() + count > MAX_FRAME {
            return Err("OpenCode bridge frame limit exceeded".into());
        }
        let complete = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            let frame =
                serde_json::from_slice(bytes).map_err(|_| "Invalid OpenCode bridge frame".into());
            bytes.clear();
            return frame;
        }
    }
}

async fn write_frame(writer: &mut UnixStream, frame: &ChildFrame) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(frame).map_err(|_| "OpenCode bridge encoding failed")?;
    if bytes.len() >= MAX_FRAME {
        return Err("OpenCode bridge frame limit exceeded".into());
    }
    bytes.push(b'\n');
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| "OpenCode bridge write failed".into())
}

struct Projection {
    init: Init,
    deck: TabDeckSnapshot,
    history: Vec<String>,
    sidebar: bool,
    deck_revision: u64,
}

impl Projection {
    fn catalog(&self, provider: Option<&str>, selection: &Selection) -> CatalogSnapshot {
        let provider = provider
            .map(str::to_owned)
            .or_else(|| selection.model.as_ref().map(|model| model.provider.clone()))
            .unwrap_or_default();
        CatalogSnapshot {
            chrome: queries::TuiChrome {
                location: Some(self.init.location.clone()),
                ..Default::default()
            },
            auto_accept: queries::AutoAcceptState::Unsupported,
            models: self
                .init
                .catalog
                .models
                .iter()
                .filter(|model| model.provider == provider)
                .map(|model| model.entry.clone())
                .collect(),
            model_id: selection
                .model
                .as_ref()
                .map(|model| model.id.clone())
                .unwrap_or_default(),
            variant: selection
                .model
                .as_ref()
                .and_then(|model| model.variant.clone())
                .filter(|variant| variant != "default"),
            provider,
            agents: self.init.catalog.agents.clone(),
            agent_id: selection.agent.clone(),
            commands: Vec::new(),
            command_descriptions: Default::default(),
        }
    }

    fn home(&mut self, action: SessionSelectionAction) -> Result<CatalogSnapshot, CoreError> {
        let mut selection = self.init.catalog.selection.clone();
        match action {
            SessionSelectionAction::Current => {}
            SessionSelectionAction::New(agent) => {
                if agent.is_some() {
                    selection.agent = agent;
                }
            }
            SessionSelectionAction::Agent(agent) | SessionSelectionAction::Preview(agent) => {
                let profile = self
                    .init
                    .catalog
                    .agents
                    .iter()
                    .find(|entry| entry.id == agent)
                    .ok_or(CoreError::SessionNotFound)?;
                selection.agent = Some(agent);
                if let Some(model) = self.init.catalog.agent_models.get(&profile.id) {
                    selection.model = Some(model.clone());
                }
            }
            SessionSelectionAction::Commit(commit) => {
                selection.model = Some(queries::ModelRef {
                    provider: commit.binding.provider,
                    id: commit.model_id,
                    variant: commit.variant,
                });
            }
            SessionSelectionAction::Variant(variant) => {
                let model = selection.model.as_mut().ok_or_else(unsupported)?;
                model.variant = variant;
            }
            _ => return Err(unsupported()),
        }
        Ok(self.catalog(None, &selection))
    }
}

async fn run(
    bridge: Bridge,
    mut inbox: mpsc::Receiver<InboxMsg>,
    mut control: mpsc::Receiver<NativeControl>,
    events: &tokio::sync::broadcast::Sender<CoreEvent>,
) -> Result<(), String> {
    let Bridge {
        mut reader,
        mut writer,
        init,
    } = bridge;
    let mut projection = Projection {
        deck: TabDeckSnapshot {
            revision: Some("0".into()),
            location: init.location.clone(),
            ..Default::default()
        },
        init,
        history: Vec::new(),
        sidebar: false,
        deck_revision: 0,
    };
    let (outbound, mut output) = mpsc::channel(CAPACITY);
    let mut write = tokio::spawn(async move {
        while let Some(frame) = output.recv().await {
            write_frame(&mut writer, &frame).await?;
        }
        Ok::<(), String>(())
    });
    let mut pending: HashMap<u64, oneshot::Sender<Result<Reply, CoreError>>> = HashMap::new();
    let mut buffer = Vec::new();
    let mut writer_joined = false;
    let mut sequence = 0_u64;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| "bridge signal registration failed")?;
    let result = loop {
        tokio::select! {
            biased;
            _ = term.recv() => break Err("OpenCode host is shutting down".into()),
            message = control.recv() => match message {
                Some(NativeControl::Shutdown) | None => break Ok(()),
                Some(NativeControl::Cancel { session, ack, .. }) => {
                    if pending.len() >= CAPACITY {
                        let _ = ack.send(Err(CoreError::QueueFull));
                        break Err("OpenCode bridge pending-request limit exceeded".into());
                    }
                    // Execution interruption is explicit; viewer shutdown never takes this branch.
                    let (reply, result) = oneshot::channel();
                    sequence += 1;
                    pending.insert(sequence, reply);
                    if outbound.try_send(ChildFrame::Request { id: sequence, request: Request::Interrupt { session } }).is_err() { break Err("OpenCode bridge queue overflow".into()); }
                    tokio::spawn(async move {
                        let result = match result.await { Ok(Ok(Reply::Done)) => Ok(()), Ok(Err(error)) => Err(error), _ => Err(CoreError::Shutdown) };
                        let _ = ack.send(result);
                    });
                }
            },
            frame = read_frame(&mut reader, &mut buffer) => match frame {
                Ok(HostFrame::Response { id, reply, error, unknown }) => {
                    let Some(ack) = pending.remove(&id) else { break Err("Unexpected OpenCode response identity".into()); };
                    if unknown { let _ = ack.send(Err(CoreError::Shutdown)); break Err("OpenCode request outcome unknown; no automatic resend".into()); }
                    let result = match (reply, error) { (Some(reply), None) => Ok(reply), (None, Some(error)) => Err(CoreError::Application(error)), _ => { break Err("Invalid OpenCode response".into()); } };
                    let _ = ack.send(result);
                }
                Ok(HostFrame::Event { event: Event::Unavailable { reason } }) => break Err(reason),
                Ok(HostFrame::Event { event }) => { let _ = events.send(CoreEvent::OpenCode(Box::new(event))); }
                Ok(HostFrame::Activate { .. }) => break Err("Repeated OpenCode activation".into()),
                Err(error) => break Err(error),
            },
            message = inbox.recv() => match message {
                Some(InboxMsg::OpenCode { request, ack }) => {
                    if pending.len() >= CAPACITY { break Err("OpenCode bridge pending-request limit exceeded".into()); }
                    sequence += 1;
                    pending.insert(sequence, ack);
                    if outbound.try_send(ChildFrame::Request { id: sequence, request }).is_err() { break Err("OpenCode bridge queue overflow".into()); }
                }
                Some(InboxMsg::Catalog { ack }) => { let _ = ack.send(Ok(projection.catalog(None, &projection.init.catalog.selection))); }
                Some(InboxMsg::ProviderCatalog { provider, ack }) => { let _ = ack.send(Ok(projection.catalog(Some(&provider), &projection.init.catalog.selection))); }
                Some(InboxMsg::ModelCatalogs { ack }) => {
                    let mut providers: Vec<_> = projection.init.catalog.models.iter().map(|model| model.provider.clone()).collect();
                    providers.sort(); providers.dedup();
                    let catalogs = providers.iter().map(|provider| projection.catalog(Some(provider), &projection.init.catalog.selection)).collect();
                    // Model metadata does not establish provider credential usability.
                    let _ = ack.send(Ok(queries::ModelCatalogs { connected: false, catalogs }));
                }
                Some(InboxMsg::HomeSelection { action, ack }) => { let _ = ack.send(projection.home(action)); }
                Some(InboxMsg::TabDeck { ack }) => { let _ = ack.send(Ok(projection.deck.clone())); }
                Some(InboxMsg::SaveTabDeck { mut deck, ack }) => {
                    if deck.revision != projection.deck.revision {
                        let _ = ack.send(Err(CoreError::TabDeckConflict));
                        continue;
                    }
                    projection.deck_revision += 1;
                    deck.revision = Some(projection.deck_revision.to_string());
                    projection.deck = deck;
                    let _ = ack.send(Ok(projection.deck.clone()));
                }
                Some(InboxMsg::PromptHistory { append, ack }) => {
                    let result = append.as_deref().map_or(Ok(()), |text| queries::append_prompt_history(&mut projection.history, text));
                    let _ = ack.send(result.map(|()| projection.history.clone()));
                }
                Some(InboxMsg::SidebarMcpOpen { open, ack }) => { if let Some(open) = open { projection.sidebar = open; } let _ = ack.send(Ok(projection.sidebar)); }
                Some(InboxMsg::RegisterQuestionConsumer { ack }) => { let _ = ack.send(Ok(())); }
                Some(InboxMsg::RegisterApprovalConsumer { ack, .. }) => { let _ = ack.send(Err(unsupported())); }
                Some(InboxMsg::Shutdown) | None => break Ok(()),
                Some(other) => refuse(other),
            },
            _ = &mut write => {
                writer_joined = true;
                break Err("OpenCode bridge writer stopped".into());
            }
        }
    };
    let _ = outbound.try_send(ChildFrame::Return {
        session: projection.deck.active,
    });
    drop(outbound);
    // Closing this descriptor unblocks the parent; a lost peer must not strand
    // runtime shutdown behind a blocking file-reader thread.
    drop(reader);
    if !writer_joined
        && tokio::time::timeout(std::time::Duration::from_secs(1), &mut write)
            .await
            .is_err()
    {
        write.abort();
        let _ = write.await;
    }
    result
}

fn unsupported() -> CoreError {
    CoreError::Application(
        "Not available in the OpenCode bridge; return to stock TUI for this action".into(),
    )
}

fn refuse(message: InboxMsg) {
    macro_rules! refuse {
        ($($name:ident),* $(,)?) => {
            match message {
                $(InboxMsg::$name { ack, .. } => { let _ = ack.send(Err(unsupported())); },)*
                InboxMsg::McpLookup { ack, .. } => { let _ = ack.send(Err(queries::McpLookupError::Unavailable)); }
                InboxMsg::ReplyQuestion { ack, .. } => { let _ = ack.send(Err(oc_core::question::QuestionReplyError::Unavailable)); }
                InboxMsg::Shutdown => {}
            }
        };
    }
    refuse!(
        OpenCode,
        SessionViewed,
        Workspace,
        UserShell,
        PromptHistory,
        AuthMethods,
        Authenticate,
        Terminal,
        ProviderConnections,
        ModelCatalogs,
        ModelPreference,
        SidebarMcpOpen,
        CustomProvider,
        ProviderAccounts,
        McpStatus,
        McpControl,
        PendingApprovals,
        PendingQuestions,
        RegisterQuestionConsumer,
        ReplyApproval,
        RegisterApprovalConsumer,
        CompactSession,
        CancelCompaction,
        CompactionHistory,
        ChangeConversation,
        ForkSession,
        Create,
        RenameSession,
        DeleteSession,
        PickerSessionAction,
        OpenPickerSession,
        SessionPickerContext,
        RegenerateTitle,
        CancelTitle,
        Submit,
        Enqueue,
        SubmitInbox,
        Inbox,
        CommandRecovery,
        SubmitFresh,
        Cancel,
        List,
        SessionList,
        ProbeSession,
        TabDeck,
        SaveTabDeck,
        Read,
        History,
        ToolOps,
        ToolOutput,
        ShellOutput,
        CancelShell,
        ShellJobs,
        ExecuteJobs,
        ChildJobs,
        InterruptChild,
        BackgroundChild,
        ReadChild,
        ShellSnapshot,
        BackgroundShell,
        ExecuteSnapshot,
        BackgroundExecute,
        DcpSummary,
        Catalog,
        ProviderCatalog,
        SessionSelection,
        HomeSelection,
        Skills,
        FileSuggestions,
        SelectModel,
        SelectAgent,
        SwitchLocation,
        SwitchLocationHome,
        ReloadLocation,
        Dcp,
        Compress
    );
}

#[cfg(test)]
mod tests;
