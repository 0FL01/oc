//! Real application/SQLite/fake-provider proof, rather than a lifecycle mock.
use super::*;
use oc_core::approval::{ApprovalDecision, ApprovalReply};
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;

async fn finished(events: &mut tokio::sync::broadcast::Receiver<CoreEvent>) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
}

async fn notice(
    events: &mut tokio::sync::broadcast::Receiver<CoreEvent>,
) -> oc_core::queries::ShellNotice {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::ShellNotice(notice) = events.recv().await.unwrap() {
                break notice;
            }
        }
    })
    .await
    .unwrap()
}

fn setup(
    project: &std::path::Path,
    home: &std::path::Path,
    base: String,
) -> BTreeMap<String, String> {
    std::fs::write(project.join("opencode.json"), serde_json::json!({
        "model":"fixture/fixture-model","compaction":{"auto":false},"permission":{"shell":"allow","read":"allow"},
        "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":base,"apiKey":"synthetic"},
            "models":{"fixture-model":{"limit":{"context":65536,"output":2048}}}}}
    }).to_string()).unwrap();
    BTreeMap::from([
        ("HOME".into(), home.to_string_lossy().into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("SHELL".into(), "/bin/bash".into()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ])
}

// This peer holds the *third* provider response until the application has
// committed the busy continuation's shell notice. Scheduling delays cannot
// substitute for that causal barrier.
fn busy_peer(
    script: Vec<String>,
) -> (
    String,
    CapturedRequests,
    Arc<(Mutex<bool>, std::sync::Condvar)>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!(
        "http://127.0.0.1:{}/v1",
        listener.local_addr().unwrap().port()
    );
    let requests: CapturedRequests = Arc::new(Mutex::new(Vec::new()));
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let captured = requests.clone();
    let barrier = release.clone();
    std::thread::spawn(move || {
        for (index, response) in script.into_iter().enumerate() {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            assert!(length <= 1_048_576);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            captured
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            if index == 2 {
                let (ready, _) = barrier
                    .1
                    .wait_timeout_while(
                        barrier.0.lock().unwrap(),
                        Duration::from_secs(10),
                        |ready| !*ready,
                    )
                    .unwrap();
                assert!(*ready, "busy notice did not release fake response");
            }
            write!(reader.get_mut(),"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",response.len(),response).unwrap();
        }
    });
    (base, requests, release)
}

#[tokio::test]
async fn tool13_revert_hides_delivered_notice_without_replay_and_new_notice_is_visible() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let args = |name: &str| serde_json::json!({"command":format!("printf one >> {name}-effect; while [ ! -f {name}-release ]; do sleep .01; done; printf {name}-result"),"background":true});
    let (base, _, requests) = Fake::start_recording(
        vec![
            sse_tool_call("old", "shell", &args("old")) + &sse_completed(),
            sse_completed(),
            sse_tool_call("new", "shell", &args("new")) + &sse_completed(),
            sse_completed(),
            sse_delta("continued") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let env = setup(project.path(), home.path(), base);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env)
            .await
            .unwrap();
    let source = SessionId("source".into());
    app.create_session(source.clone()).await.unwrap();
    app.rename_session(source.clone(), "Source".into())
        .await
        .unwrap();
    let mut events = app.subscribe();
    app.submit(source.clone(), "old launch".into())
        .await
        .unwrap();
    finished(&mut events).await;
    std::fs::write(project.path().join("old-release"), "release").unwrap();
    let old = notice(&mut events).await;
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    let anchor: String = conn.query_row("SELECT user_message FROM turn_acceptances WHERE session_id='source' ORDER BY rowid LIMIT 1",[],|r|r.get(0)).unwrap();
    app.change_conversation(
        source.clone(),
        oc_core::queries::ConversationAction::Revert {
            message: oc_core::session::MessageId(anchor),
        },
    )
    .await
    .unwrap();
    app.submit(source.clone(), "replacement".into())
        .await
        .unwrap();
    finished(&mut events).await;
    let wire = requests.lock().unwrap().clone();
    assert!(
        !wire[2]["input"].to_string().contains(&old.delivery_id),
        "Revert resurrected hidden notice in the first accepted replacement request"
    );
    std::fs::write(project.path().join("new-release"), "release").unwrap();
    let new = notice(&mut events).await;
    app.submit(source, "visible continuation".into())
        .await
        .unwrap();
    finished(&mut events).await;
    let wire = requests.lock().unwrap().clone();
    assert_eq!(wire.len(), 5);
    assert!(!wire[4]["input"].to_string().contains(&old.delivery_id));
    assert_eq!(
        wire[4]["input"]
            .to_string()
            .matches(&new.delivery_id)
            .count(),
        1
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("old-effect")).unwrap(),
        "one"
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("new-effect")).unwrap(),
        "one"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM shell_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn tool13_fork_rebases_busy_notice_reference_once_without_copying_job() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("seed"), "seed").unwrap();
    let args = serde_json::json!({"command":"printf one >> effect; while [ ! -f release ]; do sleep .01; done; printf result", "background":true});
    let (base, requests, release) = busy_peer(vec![
        sse_tool_call("bg", "shell", &args) + &sse_completed(),
        sse_completed(),
        sse_tool_call("read", "read", &serde_json::json!({"path":"seed"})) + &sse_completed(),
        sse_completed(),
        sse_completed(),
        sse_completed(),
        sse_completed(),
    ]);
    let env = setup(project.path(), home.path(), base);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env)
            .await
            .unwrap();
    let source = SessionId("source".into());
    app.create_session(source.clone()).await.unwrap();
    app.rename_session(source.clone(), "Source".into())
        .await
        .unwrap();
    let mut events = app.subscribe();
    app.submit(source.clone(), "launch".into()).await.unwrap();
    finished(&mut events).await;
    app.submit(source.clone(), "busy continuation".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while requests.lock().unwrap().len() < 3 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    std::fs::write(project.path().join("release"), "release").unwrap();
    let completed = notice(&mut events).await;
    *release.0.lock().unwrap() = true;
    release.1.notify_one();
    finished(&mut events).await;
    assert_eq!(
        requests.lock().unwrap()[3]["input"]
            .to_string()
            .matches(&completed.delivery_id)
            .count(),
        1
    );
    app.submit(source.clone(), "boundary".into()).await.unwrap();
    finished(&mut events).await;
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    let boundary: String = conn
        .query_row(
            "SELECT id FROM messages WHERE session_id='source' AND text='boundary'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let fork = app
        .fork_session(source.clone(), oc_core::session::MessageId(boundary))
        .await
        .unwrap();
    assert_eq!(
        requests.lock().unwrap().len(),
        5,
        "fork must not generate a model request"
    );
    app.submit(fork.session.clone(), "fork continue".into())
        .await
        .unwrap();
    finished(&mut events).await;
    assert_eq!(
        requests.lock().unwrap()[5]["input"]
            .to_string()
            .matches(&completed.delivery_id)
            .count(),
        1,
        "copied notice and captured log input duplicated"
    );
    app.submit(source, "original continue".into())
        .await
        .unwrap();
    finished(&mut events).await;
    assert_eq!(
        requests.lock().unwrap()[6]["input"]
            .to_string()
            .matches(&completed.delivery_id)
            .count(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM shell_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM shell_jobs WHERE session_id=?1",
            [fork.session.0],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        std::fs::read_to_string(project.path().join("effect")).unwrap(),
        "one"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn tool13_background_always_reopen_foreign_scope_and_retained_output() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let command = "printf '%s' $$ > leader; while [ ! -f release ]; do sleep .01; done; printf retained-result";
    let args = serde_json::json!({"command":command,"background":true});
    let (base, _requests) = Fake::start(
        vec![
            sse_tool_call("background", "shell", &args) + &sse_completed(),
            sse_delta("calling turn complete") + &sse_completed(),
            sse_tool_call("reopened", "shell", &args) + &sse_completed(),
            sse_delta("saved exact grant") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    std::fs::write(project.path().join("opencode.json"), serde_json::json!({
        "model":"fixture/fixture-model","compaction":{"auto":false},
        "permission":{"bash":"ask"},
        "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":base,"apiKey":"synthetic"},
            "models":{"fixture-model":{"limit":{"context":65536,"output":2048}}}}}
    }).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("SHELL".into(), "/bin/bash".into()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env.clone())
            .await
            .unwrap();
    let source = SessionId("source".into());
    let foreign = SessionId("foreign".into());
    app.create_session(source.clone()).await.unwrap();
    app.rename_session(source.clone(), "Source".into())
        .await
        .unwrap();
    app.create_session(foreign.clone()).await.unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let mut events = app.subscribe();
    app.submit(source.clone(), "launch".into()).await.unwrap();
    let (shell_id, calling_turn) = tokio::time::timeout(Duration::from_secs(10), async {
        let mut shell_id = None;
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(request) => {
                    assert_eq!(request.action, "bash");
                    assert_eq!(request.resources, [command]);
                    assert!(!project.path().join("leader").exists());
                    app.reply_approval(ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: ApprovalDecision::Always,
                    })
                    .await
                    .unwrap();
                }
                CoreEvent::ToolCallFinished { output, .. } => {
                    let value: serde_json::Value = serde_json::from_str(&output).unwrap();
                    assert_eq!(value["status"], "running");
                    shell_id = Some(value["shellID"].as_str().unwrap().to_owned());
                }
                CoreEvent::TurnFinished { session, turn, .. } => {
                    assert_eq!(session, source);
                    break (shell_id.unwrap(), turn);
                }
                CoreEvent::ShellNotice(_) => panic!("blocked command cannot complete"),
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT status FROM turns WHERE id=?1",
            [&calling_turn.0],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "completed"
    );
    assert_eq!(
        conn.query_row(
            "SELECT phase FROM shell_jobs WHERE operation_id=?1",
            [&shell_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "running"
    );
    assert!(
        app.cancel_shell(foreign.clone(), shell_id.clone())
            .await
            .is_err()
    );
    assert!(
        app.shell_output(foreign.clone(), shell_id.clone(), 0, 1024)
            .await
            .is_err()
    );
    assert!(app.delete_session(source.clone()).await.is_err());
    std::fs::write(project.path().join("release"), "release").unwrap();
    let notice = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::ShellNotice(notice) = events.recv().await.unwrap() {
                break notice;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(notice.session, source);
    assert_eq!(notice.state, "completed");
    assert_eq!(notice.shell_id, shell_id);
    assert!(
        app.shell_output(foreign, shell_id.clone(), 0, 1024)
            .await
            .is_err()
    );
    let output = app
        .shell_output(source.clone(), shell_id.clone(), 0, 1024)
        .await
        .unwrap()
        .unwrap();
    assert!(output.text.contains("retained-result"));
    assert!(output.next_offset.is_none());
    let original = app
        .tool_output_page(source.clone(), shell_id, 0, 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&original.text).unwrap()["status"],
        "running"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(conn);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env)
            .await
            .unwrap();
    app.register_approval_consumer(false).await.unwrap();
    let mut events = app.subscribe();
    app.submit(source.clone(), "reopen".into()).await.unwrap();
    let mut notices = 0;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match events.recv().await.unwrap() {
                CoreEvent::PermissionAsked(_) => {
                    panic!("exact saved command authority should survive reopen")
                }
                CoreEvent::ShellNotice(notice) => {
                    assert_eq!(notice.session, source);
                    notices += 1;
                }
                CoreEvent::TurnFinished { .. } => break,
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    // A fast terminal worker can publish after its calling turn. Wait through
    // the same automatic event seam, never by asking the model to poll.
    if notices == 0 {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let CoreEvent::ShellNotice(notice) = events.recv().await.unwrap() {
                    assert_eq!(notice.session, source);
                    break;
                }
            }
        })
        .await
        .unwrap();
    }
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM shell_jobs WHERE phase='terminal' AND message_id IS NOT NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM events WHERE kind='shell_notice'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM permission_grants", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    app.delete_session(source).await.unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM shell_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
