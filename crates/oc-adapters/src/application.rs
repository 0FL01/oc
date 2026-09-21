//! Single native application owner behind the core command/event interface.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use oc_core::core_app::{CoreApp, CoreEvent, InboxMsg, WorkerGuard, WorkerTurnId};
use oc_core::domain::SessionId;
use oc_core::session::{CoreError, MAX_QUEUE_ITEMS, Message, MessageId, Role};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::composition::{self, Composition};
use crate::runtime::{Runtime, RuntimeError, TurnParams, TurnStatus};
use crate::storage::Db;

/// Compose and start one application. Both frontends use this entry point.
pub async fn spawn(
    project: &Path,
    data: &Path,
) -> Result<(CoreApp, WorkerGuard, Vec<String>), String> {
    let composition = composition::load(project).await?;
    let diagnostics = composition.diagnostics.clone();
    let db = Db::open(data).map_err(|e| format!("storage: {e}"))?;
    db.recover_interrupted_tools()
        .map_err(|e| format!("recovery: {e}"))?;
    let files = crate::files::Files::new(&composition.project, db.root())
        .map_err(|e| format!("files: {e}"))?;
    let shell =
        crate::shell::Shell::new(&composition.project).map_err(|e| format!("shell: {e}"))?;
    let (app, inbox, events) = CoreApp::channel(MAX_QUEUE_ITEMS);
    let (ready, ready_rx) = oneshot::channel();
    let handle = tokio::spawn(start_worker(
        db,
        composition,
        files,
        shell,
        inbox,
        events,
        ready,
    ));
    let guard = WorkerGuard::from_task(handle);
    match ready_rx.await {
        Ok(Ok(())) => Ok((app, guard, diagnostics)),
        result => {
            let _ = guard.join().await;
            Err(match result {
                Ok(Err(error)) => error,
                _ => "application worker closed".to_string(),
            })
        }
    }
}

/// Own the whole application task: build the runtime, publish the workspace
/// exactly once, then run the command loop and close owned MCP resources.
#[allow(clippy::too_many_arguments)]
async fn start_worker(
    db: Db,
    composition: Composition,
    files: crate::files::Files,
    shell: crate::shell::Shell,
    inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
    ready: oneshot::Sender<Result<(), String>>,
) -> Result<(), String> {
    let runtime = Runtime::new(
        &db,
        &composition.project.to_string_lossy(),
        composition.generation.clone(),
        crate::patch::ProtectedGlobs {
            patterns: Vec::new(),
        },
        files,
        shell,
        composition.parent_env.clone(),
        crate::tools::ToolRoots {
            project: composition.project.clone(),
            data: db.root().to_path_buf(),
        },
        None,
        false,
        composition.dcp_config.clone(),
    );
    let runtime = match runtime {
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return Ok(());
        }
        Ok(runtime) => runtime,
    };
    if let Err(error) = runtime.publish_dcp_protection(composition.dcp_protected.clone()) {
        let _ = ready.send(Err(error.to_string()));
        return Ok(());
    }
    if let Err(error) = runtime.publish_workspace(
        composition.agent_prompt.as_deref(),
        &composition.instructions,
        composition.skills.clone(),
        composition.skill_errors.clone(),
        composition.agent_digest.clone(),
    ) {
        let _ = ready.send(Err(error.to_string()));
        return Ok(());
    }
    if ready.send(Ok(())).is_err() {
        return Ok(());
    }
    worker(&runtime, &db, &composition, inbox, events).await
}

fn app_error(error: impl std::fmt::Display) -> CoreError {
    CoreError::Application(error.to_string())
}

fn query(db: &Db, runtime: &Runtime<'_>, message: InboxMsg) {
    match message {
        InboxMsg::Create { id, ack } => {
            let _ = ack.send(runtime.create_session(&id.0).map_err(app_error));
        }
        InboxMsg::List { ack } => {
            let _ = ack.send(
                db.list_sessions()
                    .map(|ids| ids.into_iter().map(SessionId).collect())
                    .map_err(app_error),
            );
        }
        InboxMsg::Read { session, ack } => {
            let result = runtime
                .open_session(&session.0)
                .map_err(app_error)
                .and_then(|()| {
                    db.read_history_full(&session.0)
                        .map_err(app_error)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|(id, role, text)| Message {
                                    id: MessageId(id),
                                    role: if role == "user" {
                                        Role::User
                                    } else {
                                        Role::Assistant
                                    },
                                    text,
                                })
                                .collect()
                        })
                });
            let _ = ack.send(result);
        }
        InboxMsg::Cancel { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnNotActive));
        }
        InboxMsg::Submit { ack, .. } => {
            let _ = ack.send(Err(CoreError::TurnBusy));
        }
        InboxMsg::Shutdown => {}
    }
}

async fn worker(
    runtime: &Runtime<'_>,
    db: &Db,
    composition: &Composition,
    mut inbox: mpsc::Receiver<InboxMsg>,
    events: broadcast::Sender<CoreEvent>,
) -> Result<(), String> {
    while let Some(message) = inbox.recv().await {
        match message {
            InboxMsg::Shutdown => break,
            InboxMsg::Submit { session, text, ack } => {
                if text.trim().is_empty() {
                    let _ = ack.send(Err(app_error("empty prompt")));
                    continue;
                }
                let (prompt, invocation) = match resolve_submission(composition, text) {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        let _ = ack.send(Err(app_error(error)));
                        continue;
                    }
                };
                let cancel = AtomicBool::new(false);
                let max_output = composition.catalog.models[&composition.model_id]
                    .pointer("/limit/output")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let params = TurnParams {
                    session: session.0.clone(),
                    prompt,
                    invocation,
                    catalog: &composition.catalog,
                    model_id: composition.model_id.clone(),
                    variant: composition.variant.clone(),
                    max_output,
                    provider: composition.provider.clone(),
                    cancel: &cancel,
                    max_rounds: crate::runtime::MAX_ROUNDS,
                };
                let mut ack = Some(ack);
                let mut turn = None;
                let mut shutdown = false;
                let result;
                {
                    let operation = runtime.run_turn_with_events(
                        params,
                        |id| {
                            let id = WorkerTurnId(id.to_string());
                            turn = Some(id.clone());
                            let _ = events.send(CoreEvent::TurnStarted {
                                session: session.clone(),
                                turn: id.clone(),
                            });
                            if let Some(ack) = ack.take() {
                                let _ = ack.send(Ok(id));
                            }
                        },
                        |id, delta| {
                            let _ = events.send(CoreEvent::TextDelta {
                                session: session.clone(),
                                turn: WorkerTurnId(id.to_string()),
                                delta: delta.to_string(),
                            });
                        },
                    );
                    tokio::pin!(operation);
                    result = loop {
                        tokio::select! {
                            result = &mut operation => break result,
                            command = inbox.recv(), if !shutdown => match command {
                                None | Some(InboxMsg::Shutdown) => {
                                    shutdown = true;
                                    cancel.store(true, Ordering::Relaxed);
                                }
                                Some(InboxMsg::Cancel { session: target, ack }) if target == session => {
                                    cancel.store(true, Ordering::Relaxed);
                                    let _ = ack.send(Ok(()));
                                }
                                Some(command) => query(db, runtime, command),
                            }
                        }
                    };
                }
                if let Some(ack) = ack {
                    let error = result.err().unwrap_or(RuntimeError::Storage);
                    let _ = ack.send(Err(app_error(error)));
                } else if let Some(turn) = turn {
                    let event = match result {
                        Err(RuntimeError::Cancelled) => CoreEvent::TurnInterrupted {
                            session: session.clone(),
                            turn,
                            partial: String::new(),
                        },
                        Ok(report) if report.status == TurnStatus::Completed => {
                            CoreEvent::TurnFinished {
                                session: session.clone(),
                                turn,
                                text: report.text,
                            }
                        }
                        Ok(report) if report.status == TurnStatus::Cancelled => {
                            CoreEvent::TurnInterrupted {
                                session: session.clone(),
                                turn,
                                partial: report.text,
                            }
                        }
                        Ok(report) if report.status == TurnStatus::Incomplete => {
                            CoreEvent::TurnFailed {
                                session: session.clone(),
                                turn,
                                error: app_error(report.diagnostic.as_deref().unwrap_or(
                                    "turn incomplete: response ended early or round limit reached",
                                )),
                            }
                        }
                        Ok(report) => CoreEvent::TurnFailed {
                            session: session.clone(),
                            turn,
                            error: app_error(
                                report.diagnostic.as_deref().unwrap_or("provider error"),
                            ),
                        },
                        Err(error) => CoreEvent::TurnFailed {
                            session: session.clone(),
                            turn,
                            error: app_error(error),
                        },
                    };
                    let _ = events.send(event);
                }
                if shutdown {
                    break;
                }
            }
            message => query(db, runtime, message),
        }
    }
    runtime
        .shutdown_mcp()
        .await
        .map_err(|error| error.to_string())
}

fn resolve_submission(
    composition: &Composition,
    text: String,
) -> Result<(String, Option<String>), RuntimeError> {
    let Some(command) = text.strip_prefix('/') else {
        return Ok((text, None));
    };
    let split = command.find(char::is_whitespace).unwrap_or(command.len());
    let id = &command[..split];
    let Some(template) = composition.commands.get(id) else {
        return Ok((text, None));
    };
    let remainder = command[split..].trim();
    let args = remainder
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let expanded = crate::runtime::expand_command(template, &args)?;
    Ok((expanded, Some(text)))
}
