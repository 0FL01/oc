//! Headless consumer of the same native application used by the TUI.
//! Session transitions and persistence belong to the application, not this UI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;

const PERMISSION_REJECTED_FEEDBACK: &str = "This non-interactive run cannot ask the user for permission, so the request was rejected. Continue without this action.";

/// Own durable XDG namespace; never opens the upstream OpenCode database.
pub fn default_data_dir() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        let root = PathBuf::from(root);
        if root.is_absolute() {
            return Ok(root.join("oc"));
        }
    }
    std::env::var_os("HOME")
        .filter(|s| !s.is_empty())
        .map(|root| PathBuf::from(root).join(".local/share/oc"))
        .ok_or_else(|| {
            "HOME or absolute XDG_DATA_HOME required; alternatively use --data-dir".to_string()
        })
}

/// Run one real configured turn; diagnostics never enter stdout.
#[allow(clippy::too_many_arguments)]
pub async fn run_once_to_writers(
    prompt: Vec<String>,
    session_opt: Option<String>,
    continue_session: bool,
    model: Option<String>,
    files: Vec<PathBuf>,
    agent: Option<String>,
    json: bool,
    auto_once: bool,
    data_dir: &Path,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    oc_adapters::trace::log(
        "headless.begin",
        &format!("data_dir={}", data_dir.display()),
    );
    let result = async {
        use std::io::{IsTerminal as _, Read as _};
        let mut piped = String::new();
        if !std::io::stdin().is_terminal() {
            std::io::stdin().take((oc_core::session::MAX_INPUT_BYTES + 1) as u64)
                .read_to_string(&mut piped).map_err(|_| "cannot read UTF-8 stdin".to_string())?;
        }
        let prompt = merge_input(&prompt, &piped)?;
        let project = std::env::current_dir().map_err(|e| e.to_string())?;
        let data_dir = std::path::absolute(data_dir).map_err(|e| e.to_string())?;
        let prompt = oc_adapters::attachments::attach_files(prompt.into(), &files, &project, &data_dir)
            .map_err(|error| error.to_string())?;
        if prompt.trim().is_empty() {
            return Err("empty prompt".to_string());
        }
        let (app, guard, _notices) =
            match oc_adapters::native::attach(&data_dir, Some(&project), oc_adapters::native::Lease::Run).await {
                Ok(spawned) => {
                    oc_adapters::trace::log("spawn.ok", "");
                    spawned
                }
                Err(issue) => {
                    oc_adapters::trace::log("spawn.fail", &issue.to_string());
                    return Err(issue.to_string());
                }
            };
        let outcome = async {
            let session = if let Some(session) = session_opt {
                SessionId::new(session).ok_or_else(|| "invalid session id".to_string())?
            } else if continue_session {
                app.session_list("".into(), false).await.map_err(|error| error.to_string())?
                    .into_iter().next().map(|entry| entry.id)
                    .unwrap_or_else(|| SessionId(format!("s-{}", nanos())))
            } else { SessionId(format!("s-{}", nanos())) };
            let mut default = if let Some(agent) = &agent {
                // Validate explicit addressing before creating a session or an
                // approval consumer. Hidden primary/all profiles remain valid.
                app.home_selection(oc_core::queries::SessionSelectionAction::Agent(agent.clone())).await.map_err(|error| error.to_string())?
            } else { app.catalog().await.map_err(|error| error.to_string())? };
            if let Some(model) = &model {
                default = app.home_selection(model_action(&default, model)?)
                    .await.map_err(|error| error.to_string())?;
            }
            // An unavailable default must not block an admitted stored choice.
            // Ready defaults retain create_session's existing Location gate;
            // runtime admission still checks the exact scoped request choice.
            let default_unavailable = default.chrome.selection.is_some()
                || default.chrome.provider.as_ref().is_some_and(|provider| {
                    provider.status != oc_core::queries::ProviderStatus::Ready
                });
            let catalog = if default_unavailable
                && app
                    .probe_session(session.clone())
                    .await
                    .map_err(|error| error.to_string())?
                    == oc_core::queries::SessionProbe::Root
            {
                app.session_selection(
                    session.clone(),
                    false,
                    oc_core::queries::SessionSelectionAction::Current,
                )
                .await
            } else {
                Ok(default)
            }
            .map_err(|error| error.to_string())?;
            // These are already bounded, source-qualified public projections.
            // Never ship or parse the composition owner's raw diagnostic prose.
            for diagnostic in &catalog.chrome.service_diagnostics {
                writeln!(err, "warning: {diagnostic}").map_err(|error| error.to_string())?;
            }
            for plugin in &catalog.chrome.plugins.entries {
                writeln!(err, "warning: {plugin}").map_err(|error| error.to_string())?;
            }
            if catalog.chrome.service_diagnostics_omitted > 0 {
                writeln!(err, "warning: diagnostic inventory: {} additional notes omitted from presentation", catalog.chrome.service_diagnostics_omitted).map_err(|error| error.to_string())?;
            }
            if let Some(issue) = &catalog.chrome.selection {
                return Err(issue.diagnostic.to_string());
            }
            if let Some(provider) = catalog.chrome.provider {
                if provider.status != oc_core::queries::ProviderStatus::Ready {
                    return Err(provider
                        .diagnostic
                        .expect("unavailable provider has a cause")
                        .to_string());
                }
                if let Some(diagnostic) = provider.diagnostic {
                    writeln!(err, "warning: {diagnostic}").map_err(|error| error.to_string())?;
                }
            }
            app.register_approval_consumer(auto_once)
                .await
                .map_err(|e| e.to_string())?;
            app.create_session(session.clone())
                .await
                .map_err(|e| e.to_string())?;
            let action = agent.map_or(oc_core::queries::SessionSelectionAction::Current, oc_core::queries::SessionSelectionAction::Agent);
            let selected = app.session_selection(session.clone(), false, action).await.map_err(|error| error.to_string())?;
            if let Some(model) = &model {
                app.session_selection(session.clone(), false, model_action(&selected, model)?)
                    .await.map_err(|error| error.to_string())?;
            }
            let mut rx = app.subscribe();
            // Keep the listener registered across acknowledgement and event
            // delivery; recreating ctrl_c futures can lose SIGINT in that gap.
            let mut interrupts=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).map_err(|e|e.to_string())?;
            let submit = async {
                app.request_submit_input_selected(session.clone(), prompt, None)?.wait().await
            };
            tokio::pin!(submit);
            let turn = tokio::select! {
                result = &mut submit => result.map_err(|e| e.to_string())?,
                signal = interrupts.recv() => {
                    signal.ok_or("interrupt signal stream closed")?;
                    app.cancel(session.clone()).await.map_err(|e| e.to_string())?;
                    let _ = submit.await;
                    return Ok(130);
                }
            };
            writeln!(err, "session {}", session.0).map_err(|e| e.to_string())?;
            loop {
                let event = tokio::select! {
                    event = rx.recv() => event.map_err(|e| e.to_string())?,
                    signal = interrupts.recv() => {
                        signal.ok_or("interrupt signal stream closed")?;
                        app.cancel(session.clone()).await.map_err(|e| e.to_string())?;
                        continue;
                    }
                };
                match event {
                    CoreEvent::OwnerDisconnected => return Err("owner disconnected; operation outcome unknown; reopen history before retrying".into()),
                    CoreEvent::PermissionAsked(request) if !auto_once => {
                        reject_permission_ask(&app, &session, request).await;
                    }
                    CoreEvent::RetryScheduled { turn: id, span, retry, .. } if id == turn => {
                        if json {
                            writeln!(err, "{}", serde_json::json!({"type":"retry", "span":span,"attempt":retry.attempt,"at":retry.at,"error":retry.safe_error}))
                        } else {
                            writeln!(err, "retry attempt {} at {}: {}",retry.attempt,retry.at,retry.safe_error)
                        }.map_err(|e|e.to_string())?;
                    }
                    CoreEvent::TurnStarted { .. } | CoreEvent::SessionTitleUpdated { .. } => {}
                    CoreEvent::TextDelta {
                        turn: id, delta, ..
                    } if id == turn => {
                        if json {
                            writeln!(
                                out,
                                "{}",
                                serde_json::json!({"type":"delta", "delta":delta})
                            )
                        } else {
                            write!(out, "{delta}")
                        }
                        .map_err(|e| e.to_string())?;
                        out.flush().map_err(|e| e.to_string())?;
                    }
                    CoreEvent::TurnFinished {
                        turn: id,
                        text,
                        warnings,
                        ..
                    } if id == turn => {
                        for warning in &warnings {
                            writeln!(err, "warning: {warning}").map_err(|e| e.to_string())?;
                        }
                        if json {
                            writeln!(out, "{}", serde_json::json!({"type":"done", "text":text}))
                        } else {
                            writeln!(out)
                        }
                        .map_err(|e| e.to_string())?;
                        return Ok(0);
                    }
                    CoreEvent::TurnInterrupted { turn: id, .. } if id == turn => {
                        return Ok(130);
                    }
                    CoreEvent::TurnFailed {
                        turn: id,
                        error,
                        warnings,
                        ..
                    } if id == turn => {
                        for warning in &warnings {
                            writeln!(err, "warning: {warning}").map_err(|e| e.to_string())?;
                        }
                        return Err(error.to_string());
                    }
                    _ => {}
                }
            }
        }
        .await;
        let shutdown = app.shutdown().await;
        let joined = guard
            .join_diagnostic()
            .await
            .map_err(|e| format!("application worker: {e}"));
        match outcome {
            Err(error) => Err(error),
            Ok(code) => { joined?; shutdown.map_err(|error| error.to_string())?; Ok(code) }
        }
    }
    .await;
    let (code, exit) = match result {
        Ok(0) => (0, ExitCode::SUCCESS),
        Ok(code) => (code, ExitCode::from(code)),
        Err(message) => {
            let _ = writeln!(err, "error: {message}");
            (1, ExitCode::from(1))
        }
    };
    oc_adapters::trace::log("headless.exit", &format!("code={code}"));
    exit
}

fn merge_input(words: &[String], piped: &str) -> Result<String, String> {
    // Preserve the existing literal single-prompt contract; the shell has
    // already removed argument quoting. Separate words share the same path.
    let mut text = words.join(" ");
    if !piped.is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(piped);
    }
    if text.len() > oc_core::session::MAX_INPUT_BYTES {
        return Err("prompt too large".into());
    }
    Ok(text)
}

fn model_action(
    catalog: &oc_core::queries::CatalogSnapshot,
    raw: &str,
) -> Result<oc_core::queries::SessionSelectionAction, String> {
    let (provider, rest) = raw
        .split_once('/')
        .ok_or("model reference requires provider/model")?;
    let (id, variant) = rest
        .split_once('#')
        .map_or((rest, None), |(id, variant)| (id, Some(variant)));
    if provider.is_empty()
        || provider.contains('#')
        || id.is_empty()
        || variant.is_some_and(|value| value.is_empty() || value.contains('#'))
    {
        return Err("invalid model reference".into());
    }
    Ok(oc_core::queries::SessionSelectionAction::Commit(
        oc_core::queries::ModelCommit {
            attachment: 0,
            caller: 0,
            draft_revision: 0,
            binding: oc_core::queries::SelectionBinding {
                location: catalog.chrome.location.clone(),
                generation: catalog.chrome.selection_generation,
                provider: provider.into(),
                agent_id: catalog.agent_id.clone(),
            },
            model_id: id.into(),
            variant: variant.map(str::to_owned),
        },
    ))
}

async fn reject_permission_ask(
    app: &oc_core::core_app::CoreApp,
    root: &SessionId,
    request: oc_core::approval::ApprovalRequest,
) {
    let mut session = SessionId(request.binding.session.clone());
    while &session != root {
        let Ok(page) = app.transcript_page(session, None, None, 1).await else {
            return;
        };
        let Some(parent) = page.parent_id else {
            return;
        };
        session = SessionId(parent);
    }
    // Like the TS consumer, a request settled while ancestry was read is harmless.
    let _ = app
        .reply_approval(oc_core::approval::ApprovalReply {
            id: request.id,
            binding: request.binding,
            decision: oc_core::approval::ApprovalDecision::Reject {
                feedback: Some(PERMISSION_REJECTED_FEEDBACK.into()),
            },
        })
        .await;
}

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// List sessions from the data dir; stdout carries ids only.
pub async fn list_to_writers(
    data_dir: &Path,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    let result = async {
        let (app, guard, _) =
            oc_adapters::native::attach(data_dir, None, oc_adapters::native::Lease::Sessions)
                .await
                .map_err(|error| error.to_string())?;
        let rows = app.list_sessions().await.map_err(|error| error.to_string());
        let _ = app.shutdown().await;
        guard
            .join_diagnostic()
            .await
            .map_err(|error| error.to_string())?;
        rows
    }
    .await;
    match result {
        Ok(ids) => {
            for id in ids {
                if let Err(error) = writeln!(out, "{}", id.0) {
                    let _ = writeln!(err, "error: output: {error}");
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = writeln!(err, "error: {}", e);
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn w10_run_words_and_stdin_keep_literal_input_and_order() {
        assert_eq!(
            super::merge_input(&["two".into(), "words".into()], "piped\n").unwrap(),
            "two words\npiped\n"
        );
        assert_eq!(
            super::merge_input(&["say \"hello\"".into()], "").unwrap(),
            "say \"hello\""
        );
        assert_eq!(super::merge_input(&[], "stdin only").unwrap(), "stdin only");
    }
}
