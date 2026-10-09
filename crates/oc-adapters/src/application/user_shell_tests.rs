//! Explicit user commands through the real policy/admission/job owner, no LLM.
use super::*;
use oc_core::{
    approval::{ApprovalDecision, ApprovalReply},
    core_app::UserShellSelection,
};
use serde_json::json;
use std::{fs, path::PathBuf, time::Duration};

struct Fixture {
    _root: tempfile::TempDir,
    project: PathBuf,
    data: PathBuf,
    env: BTreeMap<String, String>,
    listener: std::net::TcpListener,
}

impl Fixture {
    fn new(permission: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let config = root.path().join("home/config/opencode");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&config).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        fs::write(config.join("opencode.json"), json!({
            "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{
                "apiKey":"fixture-key", "baseURL":format!("http://{}/v1", listener.local_addr().unwrap())},
                "models":{"fixture":{"name":"Fixture","limit":{"context":65536,"output":4096}}}}},
            "model":"fixture/fixture", "permission":{"*":"deny","shell":permission}
        }).to_string()).unwrap();
        let env = BTreeMap::from([
            (
                "HOME".into(),
                root.path().join("home").display().to_string(),
            ),
            (
                "XDG_CONFIG_HOME".into(),
                root.path().join("home/config").display().to_string(),
            ),
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("OC_ALLOW_LOOPBACK_TEST_PROVIDER".into(), "1".into()),
        ]);
        Self {
            data: root.path().join("data"),
            _root: root,
            project,
            env,
            listener,
        }
    }

    async fn spawn(&self) -> (CoreApp, WorkerGuard) {
        let (app, guard, _) = tokio::time::timeout(
            Duration::from_secs(5),
            spawn_with_env(&self.project, &self.data, self.env.clone()),
        )
        .await
        .unwrap()
        .unwrap();
        (app, guard)
    }

    fn no_provider(&self) {
        assert_eq!(
            self.listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[tokio::test]
async fn vis12_user_shell_application_admits_history_before_owned_effect_without_model_turn() {
    for permission in ["allow", "deny", "ask"] {
        let fixture = Fixture::new(permission);
        let (app, guard) = fixture.spawn().await;
        if permission == "ask" {
            app.register_approval_consumer(false).await.unwrap();
        }
        let session = SessionId::new("explicit-user-shell").unwrap();
        let command = "printf 'USER-SHELL-OUTPUT Ω界\\n'; printf effect > user-shell.effect";
        let mut receipt = app
            .request_user_shell(
                session.clone(),
                command.into(),
                UserShellSelection::Fresh(None),
            )
            .unwrap();
        if permission == "ask" {
            let approval = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(request) = app.pending_approvals().await.unwrap().into_iter().next()
                    {
                        break request;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            // Canonical shell calls retain the existing bash permission action.
            assert_eq!(approval.action, "bash");
            assert_eq!(approval.binding.call, approval.binding.operation);
            assert!(approval.binding.operation.starts_with("user-shell:"));
            assert_eq!(approval.binding.session, session.0);
            assert!(!fixture.project.join("user-shell.effect").exists());
            assert!(app.prompt_history(None).await.unwrap().is_empty());
            assert!(receipt.try_result().is_none());
            app.reply_approval(ApprovalReply {
                id: approval.id,
                binding: approval.binding,
                decision: ApprovalDecision::Once,
            })
            .await
            .unwrap();
        }
        let accepted = tokio::time::timeout(Duration::from_secs(5), receipt.wait())
            .await
            .unwrap();
        if permission == "deny" {
            assert!(accepted.is_err());
            assert!(app.prompt_history(None).await.unwrap().is_empty());
            assert!(!fixture.project.join("user-shell.effect").exists());
        } else {
            let operation = accepted.unwrap();
            assert_eq!(app.prompt_history(None).await.unwrap(), [command]);
            tokio::time::timeout(Duration::from_secs(5), async {
                while !fixture.project.join("user-shell.effect").exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                fs::read_to_string(fixture.project.join("user-shell.effect")).unwrap(),
                "effect"
            );
            let snapshot = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(snapshot) =
                        app.shell_snapshot(session.clone(), operation.clone()).await
                        && snapshot.state == "completed"
                    {
                        break snapshot;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert!(snapshot.job.turn.is_empty(), "not an accepted LLM turn");
            assert_eq!(snapshot.job.command, command);
            assert_eq!(snapshot.job.model, "fixture");
            assert_eq!(snapshot.job.provider, "fixture");
            assert!(snapshot.text.contains("USER-SHELL-OUTPUT"));
            let page = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let page = app
                        .history_page(session.clone(), None, None, 10)
                        .await
                        .unwrap();
                    if page.rows.len() >= 2
                        && page.rows.last().is_some_and(|row| {
                            row.user_shell.as_ref().is_some_and(|shell| {
                                !shell.superseded_input && shell.state == "completed"
                            })
                        })
                    {
                        break page;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            let row = page.rows.last().unwrap();
            assert!(row.turn.is_none());
            assert_eq!(row.role, Role::User);
            assert!(
                row.text.contains("native durable notice"),
                "RAW data remains intact"
            );
            let projected = row.user_shell.as_ref().unwrap();
            assert_eq!(projected.operation, operation);
            assert_eq!(projected.command, command);
            assert_eq!(
                projected.output.shell.as_ref().unwrap().stdout,
                "USER-SHELL-OUTPUT Ω界\n"
            );
            assert_eq!(projected.output.shell.as_ref().unwrap().exit, Some(0));
            assert!(projected.output.is_valid());
            let exact = app
                .history_message(session.clone(), row.id.clone())
                .await
                .unwrap();
            assert_eq!(exact.rows.as_slice(), std::slice::from_ref(row));
            let missing = app
                .history_message(
                    session.clone(),
                    oc_core::session::MessageId("not-a-message-in-this-session".into()),
                )
                .await
                .unwrap();
            assert!(missing.rows.is_empty());
            assert_eq!(missing.total, page.total);
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        fixture.no_provider();
        let db = Db::open(&fixture.data).unwrap();
        if permission == "deny" {
            assert!(
                db.session_meta(&session.0).is_err(),
                "refused fresh command leaves no root"
            );
        } else {
            assert_eq!(db.prompt_history(None).unwrap(), [command]);
            let operations = db.list_tool_ops(&session.0).unwrap();
            assert_eq!(operations.len(), 1);
            assert!(operations[0].turn.is_none());
            assert_eq!(operations[0].state, "completed");
            assert!(
                db.shell_job_identity(&session.0, &operations[0].op)
                    .unwrap()
                    .turn
                    .is_empty()
            );
        }
    }
}

#[tokio::test]
async fn user_shell_live_typed_streams_are_available_before_terminal_effect_and_never_replayed() {
    let fixture = Fixture::new("allow");
    let (app, guard) = fixture.spawn().await;
    let mut events = app.subscribe();
    let session = SessionId::new("held-user-shell").unwrap();
    let command = "printf 'LIVE-STDOUT [stderr] Ω界\\n'; printf 'LIVE-STDERR [stdout]\\n' >&2; printf 'fixture-'; printf 'key\\n'; while [ ! -f user-shell.release ]; do sleep .01; done; printf effect > user-shell.effect; printf 'TERMINAL\\n'";
    let mut receipt = app
        .request_user_shell(
            session.clone(),
            command.into(),
            UserShellSelection::Fresh(None),
        )
        .unwrap();
    let operation = tokio::time::timeout(Duration::from_secs(5), receipt.wait())
        .await
        .unwrap()
        .unwrap();
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::ShellChanged { session: source } if source == session) {
                let jobs = app.shell_jobs(session.clone()).await.unwrap();
                if let Some(job) = jobs.into_iter().find(|job| job.shell_id == operation)
                    && job.output.as_ref().and_then(|p| p.shell.as_ref()).is_some_and(|s| s.stdout.contains("[redacted]") && s.stderr.contains("LIVE-STDERR"))
                {
                    break job;
                }
            }
        }
    }).await;
    let before_effect = !fixture.project.join("user-shell.effect").exists();
    let history = app
        .history_page(session.clone(), None, None, 10)
        .await
        .unwrap();
    fs::write(fixture.project.join("user-shell.release"), "release").unwrap();
    let terminal = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let page = app
                .history_page(session.clone(), None, None, 10)
                .await
                .unwrap();
            if page
                .rows
                .last()
                .and_then(|r| r.user_shell.as_ref())
                .is_some_and(|s| s.state == "completed")
            {
                break page;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    fixture.no_provider();
    let job = observed.unwrap();
    assert!(before_effect && job.turn.is_empty() && job.session == session);
    let live = job.output.unwrap();
    assert!(live.is_valid() && !live.body_limited);
    let streams = live.shell.as_ref().unwrap();
    assert_eq!(streams.stdout, "LIVE-STDOUT [stderr] Ω界\n[redacted]\n");
    assert_eq!(streams.stderr, "LIVE-STDERR [stdout]\n");
    assert!(
        streams.exit.is_none()
            && streams.signal.is_none()
            && !streams.timed_out
            && !streams.cancelled
    );
    assert!(!live.body.contains("fixture-key") && !live.body.contains("TERMINAL"));
    assert_eq!(history.rows.len(), 1);
    assert!(history.rows[0].turn.is_none());
    assert_eq!(
        history.rows[0].user_shell.as_ref().unwrap().state,
        "started"
    );
    let result = terminal.rows.last().unwrap().user_shell.as_ref().unwrap();
    assert_eq!(result.operation, operation);
    assert_eq!(result.output.shell.as_ref().unwrap().exit, Some(0));
    assert!(result.output.body.contains("TERMINAL") && !result.output.body.contains("fixture-key"));
    assert_eq!(
        fs::read_to_string(fixture.project.join("user-shell.effect")).unwrap(),
        "effect"
    );
    let db = Db::open(&fixture.data).unwrap();
    assert_eq!(db.list_tool_ops(&session.0).unwrap().len(), 1);
    assert!(db.deliver_shell_notices().unwrap().is_empty());
    assert_eq!(
        db.read_history_page_typed(&session.0, 10, None)
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn vis12_user_shell_refusal_cancel_stale_scope_and_history_failure_never_launch() {
    for failure in ["reject", "cancel", "no-consumer", "scope", "history-fault"] {
        let fixture = Fixture::new(if matches!(failure, "scope" | "history-fault") {
            "allow"
        } else {
            "ask"
        });
        if failure == "history-fault" {
            let db = Db::open(&fixture.data).unwrap();
            db.prompt_history(Some("seeded input")).unwrap();
            drop(db);
            let connection = rusqlite::Connection::open(fixture.data.join("oc.sqlite")).unwrap();
            connection.execute_batch("CREATE TRIGGER fail_user_history BEFORE UPDATE ON prefs WHEN NEW.key='tui.prompt_history.v1' BEGIN SELECT RAISE(ABORT,'owned history fault'); END;").unwrap();
        }
        let (app, guard) = fixture.spawn().await;
        if matches!(failure, "reject" | "cancel") {
            app.register_approval_consumer(false).await.unwrap();
        }
        let session = SessionId::new("refused-user-shell").unwrap();
        let choice = if failure == "scope" {
            Some(oc_core::core_app::FreshSelection {
                binding: Some(oc_core::queries::SelectionBinding {
                    location: Some("/different-captured-location".into()),
                    generation: 0,
                    provider: "fixture".into(),
                    agent_id: None,
                }),
                agent_id: None,
                model_id: "fixture".into(),
                variant: None,
            })
        } else {
            None
        };
        let mut receipt = app
            .request_user_shell(
                session.clone(),
                "printf refused > refused.effect".into(),
                UserShellSelection::Fresh(choice),
            )
            .unwrap();
        if matches!(failure, "reject" | "cancel") {
            let approval = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Some(approval) =
                        app.pending_approvals().await.unwrap().into_iter().next()
                    {
                        break approval;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert!(app.prompt_history(None).await.unwrap().is_empty());
            assert!(!fixture.project.join("refused.effect").exists());
            if failure == "reject" {
                app.reply_approval(ApprovalReply {
                    id: approval.id,
                    binding: approval.binding,
                    decision: ApprovalDecision::Reject { feedback: None },
                })
                .await
                .unwrap();
            } else {
                receipt.cancel();
            }
        }
        assert!(
            tokio::time::timeout(Duration::from_secs(5), receipt.wait())
                .await
                .unwrap()
                .is_err(),
            "{failure}"
        );
        assert!(
            !fixture.project.join("refused.effect").exists(),
            "{failure}"
        );
        assert!(
            app.pending_approvals().await.unwrap().is_empty(),
            "{failure}"
        );
        assert_eq!(
            app.prompt_history(None).await.unwrap(),
            if failure == "history-fault" {
                vec!["seeded input".to_owned()]
            } else {
                Vec::new()
            }
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        fixture.no_provider();
        let db = Db::open(&fixture.data).unwrap();
        assert!(
            matches!(
                db.session_meta(&session.0),
                Err(StorageError::SessionNotFound)
            ),
            "{failure}"
        );
    }
}
