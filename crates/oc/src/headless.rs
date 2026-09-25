//! Headless consumer of the same native application used by the TUI.
//! Session transitions and persistence belong to the application, not this UI.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use oc_adapters::storage::Db;
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;

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
pub async fn run_once_to_writers(
    prompt: String,
    session_opt: Option<String>,
    json: bool,
    data_dir: &Path,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    oc_adapters::trace::log(
        "headless.begin",
        &format!("data_dir={}", data_dir.display()),
    );
    let result = async {
        if prompt.trim().is_empty() {
            return Err("empty prompt".to_string());
        }
        let project = std::env::current_dir().map_err(|e| e.to_string())?;
        let session = SessionId::new(session_opt.unwrap_or_else(|| format!("s-{}", nanos())))
            .ok_or_else(|| "invalid session id".to_string())?;
        let (app, guard, diagnostics) =
            match oc_adapters::application::spawn(&project, data_dir).await {
                Ok(spawned) => {
                    oc_adapters::trace::log("spawn.ok", "");
                    spawned
                }
                Err(message) => {
                    // The detailed text already goes to stderr; the trace file
                    // records only its size so a malformed typed config value
                    // cannot be duplicated into a second surface.
                    oc_adapters::trace::log("spawn.fail", &format!("detail_len={}", message.len()));
                    return Err(message);
                }
            };
        for diagnostic in diagnostics {
            writeln!(err, "warning: {diagnostic}").map_err(|e| e.to_string())?;
        }
        let outcome = async {
            app.create_session(session.clone())
                .await
                .map_err(|e| e.to_string())?;
            let mut rx = app.subscribe();
            let submit = app.submit(session.clone(), prompt);
            tokio::pin!(submit);
            let turn = tokio::select! {
                result = &mut submit => result.map_err(|e| e.to_string())?,
                signal = tokio::signal::ctrl_c() => {
                    signal.map_err(|e| e.to_string())?;
                    app.cancel(session.clone()).await.map_err(|e| e.to_string())?;
                    let _ = submit.await;
                    return Ok(130);
                }
            };
            writeln!(err, "session {}", session.0).map_err(|e| e.to_string())?;
            loop {
                let event = tokio::select! {
                    event = rx.recv() => event.map_err(|e| e.to_string())?,
                    signal = tokio::signal::ctrl_c() => {
                        signal.map_err(|e| e.to_string())?;
                        app.cancel(session.clone()).await.map_err(|e| e.to_string())?;
                        continue;
                    }
                };
                match event {
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
        let _ = app.shutdown().await;
        guard
            .join()
            .await
            .map_err(|e| format!("application worker: {e}"))?;
        outcome
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

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

/// List sessions from the data dir; stdout carries ids only.
pub fn list_to_writers(
    data_dir: &Path,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    match Db::open(data_dir).and_then(|db| db.list_sessions()) {
        Ok(ids) => {
            for id in ids {
                if let Err(error) = writeln!(out, "{id}") {
                    let _ = writeln!(err, "error: output: {error}");
                    return ExitCode::from(1);
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            let _ = writeln!(err, "error: storage: {e}");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::list_to_writers;
    use std::process::ExitCode;

    #[test]
    fn sessions_list_stdout_ids_only() {
        let tmp = tempfile::tempdir().expect("temp");
        let data = tmp.path().join("data");
        let db = oc_adapters::storage::Db::open(&data).expect("open");
        db.create_session("s-a").expect("a");
        db.create_session("s-b").expect("b");
        drop(db);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = list_to_writers(&data, &mut out, &mut err);
        assert_eq!(code, ExitCode::SUCCESS);
        let text = String::from_utf8(out).expect("out");
        assert!(text.contains("s-a") && text.contains("s-b"));
        assert!(err.is_empty());
    }
}
