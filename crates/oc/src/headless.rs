//! Headless runner for T05: same `CoreApp` + `MockProvider` as local code,
//! plus SQLite persistence and NDJSON discipline.
//!
//! Flow per `oc run`: open `Db` → ensure session in `Db` and `CoreApp` →
//! persist user message on accept → stream `CoreEvent`s → persist assistant
//! on finish. Interrupts persist the user input but never a partial
//! assistant message. Stdout carries only the answer (text or NDJSON);
//! diagnostics go to stderr. Exit codes: 0 success, 1 error, 130 interrupted.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use oc_adapters::storage::Db;
use oc_core::core_app::{CoreApp, CoreEvent, MockProvider};
use oc_core::domain::SessionId;
use oc_core::session::CoreError;

/// Default data dir when `--data-dir` is absent (under system temp + uid).
pub fn default_data_dir() -> PathBuf {
    let uid = current_uid();
    std::env::temp_dir().join(format!("oc-t05-{uid}"))
}

fn current_uid() -> u32 {
    // SAFETY: getuid has no failure mode and touches no Rust memory.
    unsafe { libc::getuid() }
}

/// Run one prompt; stream incrementally to `out`, diagnostics to `err`.
#[allow(clippy::too_many_arguments)]
pub async fn run_once_to_writers(
    prompt: String,
    session_opt: Option<String>,
    json: bool,
    data_dir: &Path,
    provider: MockProvider,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    match run_inner(prompt, session_opt, json, data_dir, provider, out, err).await {
        Ok(code) => code,
        Err(message) => {
            let _ = writeln!(err, "error: {message}");
            ExitCode::from(1)
        }
    }
}

async fn run_inner(
    prompt: String,
    session_opt: Option<String>,
    json: bool,
    data_dir: &Path,
    provider: MockProvider,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> Result<ExitCode, String> {
    if prompt.trim().is_empty() {
        return Err("empty prompt".to_string());
    }
    let db = Db::open(data_dir).map_err(|e| format!("storage: {e}"))?;
    let (app, guard) = CoreApp::spawn(provider);
    let result = drive_turn(prompt, session_opt, json, &db, &app, out, err).await;
    // Clean shutdown without orphan tasks; shutdown errors are diagnostics.
    let _ = app.shutdown().await;
    let _ = guard.join().await;
    result
}

async fn drive_turn(
    prompt: String,
    session_opt: Option<String>,
    json: bool,
    db: &Db,
    app: &CoreApp,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> Result<ExitCode, String> {
    let session_id = match session_opt {
        Some(raw) => SessionId::new(raw).ok_or_else(|| "invalid session id".to_string())?,
        None => SessionId::new(format!("s-{}", nanos())).ok_or("id".to_string())?,
    };
    // Ensure session in durable store first (resume-safe), then in memory.
    match db.create_session(&session_id.0) {
        Ok(()) => {}
        Err(oc_adapters::storage::StorageError::Sqlite(_)) => {
            // Duplicate: session already persisted from a prior run.
        }
        Err(e) => return Err(format!("storage: {e}")),
    }
    if app.create_session(session_id.clone()).await.is_err() {
        return Err("worker unavailable".to_string());
    }
    let _ = writeln!(
        err,
        "session {} prompt {} bytes",
        session_id.0,
        prompt.len()
    );

    let mut rx = app.subscribe();
    let turn = app
        .submit(session_id.clone(), prompt.clone())
        .await
        .map_err(|e| match e {
            CoreError::TurnBusy => "turn busy".to_string(),
            CoreError::InputTooLarge => "input too large".to_string(),
            _ => format!("submit: {e}"),
        })?;
    // Durable user input on accept (survives interrupts/provider errors).
    db.append_message(&session_id.0, "user", &prompt)
        .map_err(|e| format!("storage: {e}"))?;

    let mut finished_text: Option<String> = None;
    let mut interrupted = false;
    loop {
        match tokio::time::timeout(Duration::from_secs(30), rx.recv()).await {
            Err(_) => return Err("stream timeout".to_string()),
            Ok(Err(_)) => return Err("event channel closed".to_string()),
            Ok(Ok(CoreEvent::TurnStarted { .. })) => {}
            Ok(Ok(CoreEvent::TextDelta { delta, .. })) => {
                if json {
                    let line = serde_json::json!({"type": "delta", "delta": delta});
                    let _ = writeln!(out, "{line}");
                } else {
                    let _ = write!(out, "{delta}");
                }
                let _ = out.flush();
            }
            Ok(Ok(CoreEvent::TurnFinished {
                text, turn: done, ..
            })) => {
                if done != turn {
                    continue;
                }
                finished_text = Some(text);
                break;
            }
            Ok(Ok(CoreEvent::TurnInterrupted { turn: done, .. })) => {
                if done != turn {
                    continue;
                }
                interrupted = true;
                break;
            }
        }
    }

    if interrupted {
        let _ = writeln!(err, "interrupted; user input preserved");
        return Ok(ExitCode::from(130));
    }
    let text = finished_text.ok_or_else(|| "missing result".to_string())?;
    db.append_message(&session_id.0, "assistant", &text)
        .map_err(|e| format!("storage: {e}"))?;
    if json {
        let line = serde_json::json!({"type": "done", "text": text});
        let _ = writeln!(out, "{line}");
    } else {
        let _ = writeln!(out);
    }
    let _ = writeln!(err, "done {} bytes", text.len());
    Ok(ExitCode::SUCCESS)
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
    match Db::open(data_dir) {
        Ok(db) => match db.list_sessions() {
            Ok(ids) => {
                for id in ids {
                    let _ = writeln!(out, "{id}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                let _ = writeln!(err, "error: storage: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            let _ = writeln!(err, "error: storage: {e}");
            ExitCode::from(1)
        }
    }
}

/// Test hook: long provider + immediate cancel to exercise UI05 interrupt.
#[cfg(test)]
pub async fn run_cancel_probe_to_writers(
    data_dir: &Path,
    out: &mut dyn std::io::Write,
    err: &mut dyn std::io::Write,
) -> ExitCode {
    let provider = MockProvider::fixed((0..50).map(|i| format!("tok{i} ")).collect(), 10);
    let db = match Db::open(data_dir) {
        Ok(db) => db,
        Err(e) => {
            let _ = writeln!(err, "error: storage: {e}");
            return ExitCode::from(1);
        }
    };
    let (app, guard) = CoreApp::spawn(provider);
    let session = SessionId::new("s-int").expect("id");
    let _ = db.create_session(&session.0);
    let _ = app.create_session(session.clone()).await;
    let mut rx = app.subscribe();
    let turn = match app.submit(session.clone(), "long".to_string()).await {
        Ok(turn) => turn,
        Err(_) => {
            let _ = app.shutdown().await;
            let _ = guard.join().await;
            return ExitCode::from(1);
        }
    };
    let _ = db.append_message(&session.0, "user", "long");
    // Wait for the first delta, then cancel like Ctrl-C.
    loop {
        match rx.recv().await {
            Ok(CoreEvent::TextDelta { turn: t, delta, .. }) if t == turn => {
                let _ = write!(out, "{delta}");
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = app.cancel(session.clone()).await;
    let code = loop {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Ok(CoreEvent::TurnInterrupted { .. })) => {
                let _ = writeln!(err, "interrupted; user input preserved");
                break ExitCode::from(130);
            }
            Ok(Ok(CoreEvent::TurnFinished { .. })) => {
                break ExitCode::from(0);
            }
            Ok(Ok(_)) => {}
            _ => break ExitCode::from(1),
        }
    };
    let _ = app.shutdown().await;
    let _ = guard.join().await;
    code
}

#[cfg(test)]
mod tests {
    use super::{list_to_writers, run_cancel_probe_to_writers, run_once_to_writers};
    use oc_core::core_app::MockProvider;
    use std::process::ExitCode;

    fn code_n(code: ExitCode) -> u8 {
        // ExitCode has no getter; compare against known values.
        if code == ExitCode::SUCCESS {
            0
        } else if code == ExitCode::from(130) {
            130
        } else if code == ExitCode::from(1) {
            1
        } else {
            255
        }
    }

    #[tokio::test]
    async fn store01_persist_resume_across_restart() {
        let tmp = tempfile::tempdir().expect("temp");
        let data = tmp.path().join("data");
        // First run creates the session and persists both messages.
        let mut out1 = Vec::new();
        let mut err1 = Vec::new();
        let code = run_once_to_writers(
            "hello".to_string(),
            Some("s-1".to_string()),
            false,
            &data,
            MockProvider::echo(),
            &mut out1,
            &mut err1,
        )
        .await;
        assert_eq!(code_n(code), 0);
        assert!(
            String::from_utf8(out1)
                .expect("out")
                .contains("echo: hello")
        );
        assert!(!String::from_utf8(err1.clone()).expect("err").is_empty());

        // Restart: same data dir reopens history without reset.
        let mut out2 = Vec::new();
        let mut err2 = Vec::new();
        let code = run_once_to_writers(
            "again".to_string(),
            Some("s-1".to_string()),
            false,
            &data,
            MockProvider::echo(),
            &mut out2,
            &mut err2,
        )
        .await;
        assert_eq!(code_n(code), 0);

        let db = oc_adapters::storage::Db::open(&data).expect("reopen");
        let history = db.read_history("s-1").expect("history");
        assert_eq!(history.len(), 4);
        assert_eq!(history[0], ("user".to_string(), "hello".to_string()));
        assert_eq!(
            history[1],
            ("assistant".to_string(), "echo: hello".to_string())
        );
        assert_eq!(
            history[3],
            ("assistant".to_string(), "echo: again".to_string())
        );
    }

    #[tokio::test]
    async fn ui05_ndjson_stdout_only_and_slow_consumer() {
        let tmp = tempfile::tempdir().expect("temp");
        let data = tmp.path().join("data");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_once_to_writers(
            "json-probe".to_string(),
            Some("s-j".to_string()),
            true,
            &data,
            MockProvider::echo(),
            &mut SlowWriter::new(&mut out, 5),
            &mut err,
        )
        .await;
        assert_eq!(code_n(code), 0);
        // Stdout is pure NDJSON; every line parses.
        let text = String::from_utf8(out).expect("out");
        assert!(!text.is_empty());
        for line in text.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("ndjson line");
            assert!(v.get("type").is_some());
        }
        // Diagnostics never leak to stdout; stderr is non-empty.
        assert!(!String::from_utf8(err).expect("err").is_empty());
    }

    #[tokio::test]
    async fn ui05_interrupted_exit_is_nonsuccess() {
        let tmp = tempfile::tempdir().expect("temp");
        let data = tmp.path().join("data");
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run_cancel_probe_to_writers(&data, &mut out, &mut err).await;
        assert_eq!(code_n(code), 130);
        // User input survives the interrupt; no partial assistant is stored.
        let db = oc_adapters::storage::Db::open(&data).expect("reopen");
        let history = db.read_history("s-int").expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].0, "user");
    }

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
        assert_eq!(code_n(code), 0);
        let text = String::from_utf8(out).expect("out");
        assert!(text.contains("s-a") && text.contains("s-b"));
        assert!(String::from_utf8(err).expect("err").is_empty());
    }

    /// Adapter that sleeps per write to emulate a slow TTY/pipe consumer.
    struct SlowWriter<'a> {
        inner: &'a mut Vec<u8>,
        delay_ms: u64,
    }

    impl<'a> SlowWriter<'a> {
        fn new(inner: &'a mut Vec<u8>, delay_ms: u64) -> Self {
            Self { inner, delay_ms }
        }
    }

    impl std::io::Write for SlowWriter<'_> {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            std::thread::sleep(std::time::Duration::from_millis(self.delay_ms));
            self.inner.write(buf)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.inner.flush()
        }
    }
}
