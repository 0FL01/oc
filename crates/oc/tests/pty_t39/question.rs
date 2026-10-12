//! TOOL15/VIS37: real question consumer, provider continuation and recovery.
use super::*;
use oc_core::question::{QuestionInput, QuestionResult};

pub(super) const PROMPT: &str = "VIS37 real question consumer";
pub(super) const DONE: &str = "VIS37-ANSWER-ACCEPTED";
const SESSION: &str = "vis37-question-consumer";
const DRAFT: &str = "ordinary draft Ω界";
const CUSTOM: &str = "custom Ω界";

pub(super) fn input() -> QuestionInput {
    QuestionInput::parse(&serde_json::json!({"questions": [
        {"question":"Pick languages?", "header":"Languages", "multiple":true,
         "options":[{"label":"Rust", "description":"Native owner"},
                    {"label":"Python", "description":"Fixture peer"}]},
        {"question":"Where Ω界?", "header":"Platform",
         "options":[{"label":"Linux", "description":"Current host"},
                    {"label":"Other", "description":"Explicit selection"}]}
    ]}))
    .unwrap()
}

fn spawn_auto(fixture: Arc<Fixture>) -> PtySession {
    let (master, slave) = openpty_pair(120, 40);
    let mut command = fixture.command();
    command
        .args(["tui", "--auto", "--session", SESSION])
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(dup_fd(&slave)))
        .stdout(Stdio::from(dup_fd(&slave)))
        .stderr(Stdio::from(dup_fd(&slave)));
    terminal::controlling_terminal(&mut command);
    let child = command.spawn().unwrap();
    drop(slave);
    PtySession::finish_spawn(master, child, fixture)
}

fn raw_rows(fixture: &Fixture) -> Vec<(String, i64, String, String)> {
    let sql = rusqlite::Connection::open_with_flags(
        fixture.data_dir().join("oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    sql.prepare("SELECT id,seq,role,text FROM messages WHERE session_id=?1 ORDER BY seq LIMIT 8")
        .unwrap()
        .query_map([SESSION], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn vis37_actual_question_auto_still_waits_for_owned_reply_and_durable_restart() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, SESSION, 0);
    let mut pty = spawn_auto(fixture.clone());
    pty.wait_visible(READY, DEADLINE);
    pty.send(format!("{PROMPT}\r").as_bytes());
    let first = fixture.wait_requests(1);
    assert_eq!(last_user_text(&first[0]).as_deref(), Some(PROMPT));
    assert!(
        first[0]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "question")
    );
    pty.wait_visible(&user_needle(PROMPT), DEADLINE);
    pty.send(format!("\x1b[200~{DRAFT}\x1b[201~").as_bytes());
    pty.wait_visible(DRAFT, DEADLINE);
    // A modal can already be focused when the provider asks. Its visible
    // controls must not edit, answer or cancel the question behind it.
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    fixture.s07_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&pty, "Pick languages?", DEADLINE);
    pty.send(b"\x1b[200~vis37-no-such-model\x1b[201~\r");
    wait_screen_row(&pty, "vis37-no-such-model", DEADLINE);
    assert_eq!(
        fixture.wait_requests(1),
        first,
        "modal Enter is not a question reply"
    );
    mouse_click(&mut pty, 1, 1);
    dismissed(&pty, "Select model");
    wait_screen_row(&pty, "Pick languages?", DEADLINE);
    assert_eq!(
        fixture.wait_requests(1),
        first,
        "--auto is not a question answer"
    );
    pty.send(b" \x1b[B \t");
    // This text-screen observer is not a styled Unicode-cell oracle. The
    // second field's ASCII question prefix proves navigation; exact Unicode
    // and ordered answers are checked in the actual continuation below.
    wait_screen_row(&pty, "Where ", DEADLINE);
    pty.send(b"3");
    wait_screen_row(&pty, "esc close", DEADLINE);
    pty.send(format!("\x1b[200~{CUSTOM}\x1b[201~").as_bytes());
    wait_screen_row(&pty, CUSTOM, DEADLINE);
    assert_eq!(fixture.wait_requests(1), first, "editing is not submission");
    pty.send(b"\r");
    wait_screen_row(&pty, "enter submit", DEADLINE);
    assert_eq!(fixture.wait_requests(1), first, "review is not submission");
    pty.send(b"\r");
    let requests = fixture.wait_requests(2);
    let outputs: Vec<_> = requests[1]["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .collect();
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0]["call_id"], "call_t39");
    let result: QuestionResult =
        serde_json::from_str(outputs[0]["output"].as_str().unwrap()).unwrap();
    assert_eq!(result.questions, input().questions);
    assert_eq!(result.answers, [vec!["Rust", "Python"], vec![CUSTOM]]);
    pty.wait_visible(DONE, DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "# Questions", DEADLINE);
    wait_screen_row(&pty, DRAFT, DEADLINE);
    pty.send(b"\x03\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let before = raw_rows(&fixture);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let operations = db.list_tool_ops(SESSION).unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].name, "question");
    assert_eq!(operations[0].state, "completed");
    assert_eq!(operations[0].question.as_ref(), Some(&result));
    drop(db);
    let mut restarted = spawn_auto(fixture.clone());
    wait_screen_row(&restarted, "# Questions", DEADLINE);
    restarted.wait_visible(DONE, DEADLINE);
    wait_idle(&restarted);
    assert_eq!(
        fixture.wait_requests(2),
        requests,
        "completed question must not be re-asked"
    );
    restarted.send(b"\x03");
    assert!(restarted.wait_exit(DEADLINE).0.success() && restarted.restored());
    assert_eq!(
        raw_rows(&fixture),
        before,
        "restart does not rewrite RAW ids/sequence/text"
    );
    assert_eq!(fixture.wait_requests(2), requests);
}

#[test]
fn vis37_actual_question_dismiss_is_cancelled_not_answered_or_replayed() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, SESSION, 0);
    fixture.s07_continue.store(true, Ordering::Relaxed);
    let mut pty = spawn_auto(fixture.clone());
    pty.wait_visible(READY, DEADLINE);
    pty.send(format!("{PROMPT}\r").as_bytes());
    wait_screen_row(&pty, "Pick languages?", DEADLINE);
    let requests = fixture.wait_requests(1);
    pty.send(b"\x1b");
    wait_idle(&pty);
    assert_eq!(fixture.wait_requests(1), requests);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let before = raw_rows(&fixture);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let operations = db.list_tool_ops(SESSION).unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].state, "cancelled");
    assert_eq!(
        operations[0].output.as_deref(),
        Some("error: question dismissed")
    );
    assert!(operations[0].question.is_none());
    drop(db);
    let mut restarted = spawn_auto(fixture.clone());
    restarted.wait_visible("Fixture session title", DEADLINE);
    wait_idle(&restarted);
    assert_eq!(fixture.wait_requests(1), requests);
    assert!(
        !render_screen(&restarted.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("# Questions"))
    );
    restarted.send(b"\x03");
    assert!(restarted.wait_exit(DEADLINE).0.success() && restarted.restored());
    assert_eq!(raw_rows(&fixture), before);
    assert_eq!(fixture.wait_requests(1), requests);
}

#[test]
fn vis37_actual_headless_auto_surfaces_question_required_without_answer() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, SESSION, 0);
    fixture.s07_continue.store(true, Ordering::Relaxed);
    let mut command = fixture.command();
    command
        .args(["run", PROMPT, "--auto", "--session", SESSION])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    let began = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if began.elapsed() >= DEADLINE {
            let _ = child.kill();
            let _ = child.wait();
            panic!("headless QuestionRequired did not terminate");
        }
        std::thread::sleep(POLL);
    }
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("question_required (retryable=false)"),
        "headless stderr: {stderr}"
    );
    assert!(
        stderr.contains(
            "question: use the interactive TUI to answer; --auto only accepts permissions"
        )
    );
    let requests = fixture.wait_requests(1);
    assert_eq!(requests.len(), 1);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let operations = db.list_tool_ops(SESSION).unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].state, "failed");
    assert!(operations[0].question.is_none());
    assert!(
        operations[0]
            .output
            .as_deref()
            .unwrap()
            .contains("question required:")
    );
}
