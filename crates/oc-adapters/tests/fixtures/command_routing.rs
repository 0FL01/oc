//! Application command integration: real wire and pre-effect admission.
use super::*;
use oc_adapters::application::spawn_with_env;
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;
use oc_core::queries::SessionSelectionAction as Action;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Server {
    base: String,
    requests: CapturedRequests,
    stop: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
}
impl Server {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let (stop, mut stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut workers = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopped => break,
                    result = listener.accept() => {
                        let (mut stream, _) = result.unwrap();
                        let captured = captured.clone();
                        workers.spawn(async move {
                            let mut bytes = Vec::new();
                            let end = loop {
                                if let Some(index) = bytes.windows(4).position(|w|w == b"\r\n\r\n") { break index + 4; }
                                let mut chunk = [0;4096];
                                let n = stream.read(&mut chunk).await.unwrap();
                                assert!(n > 0 && bytes.len() < 1048576);
                                bytes.extend_from_slice(&chunk[..n]);
                            };
                            let header = std::str::from_utf8(&bytes[..end]).unwrap();
                            let size: usize = header.lines().find_map(|line| line.split_once(':').filter(|(key,_)|key.eq_ignore_ascii_case("content-length")).map(|(_,value)|value.trim().parse().unwrap())).unwrap();
                            while bytes.len() < end + size {
                                let mut chunk = [0;4096];
                                let n = stream.read(&mut chunk).await.unwrap();
                                assert!(n > 0 && bytes.len() < 1048576);
                                bytes.extend_from_slice(&chunk[..n]);
                            }
                            let request: serde_json::Value = serde_json::from_slice(&bytes[end..end+size]).unwrap();
                            captured.lock().unwrap().push(request);
                            let body = sse_delta("fixture output") + &sse_completed();
                            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                        });
                    }
                }
            }
            while let Some(result) = workers.join_next().await {
                result.unwrap();
            }
        });
        Self {
            base,
            requests,
            stop,
            task,
        }
    }
    async fn close(self) {
        self.stop.send(()).unwrap();
        self.task.await.unwrap();
    }
}

fn settings(base: &str) -> serde_json::Value {
    serde_json::json!({"model":"fixture/m","compaction":{"auto":false},"permission":{"subagent":"allow"},
        "provider":{"fixture":{"options":{"baseURL":base,"apiKey":"dummy"},"models":{
            "m":{"limit":{"context":500000,"output":4096}},
            "profile":{"limit":{"context":500000,"output":4096},"variants":{"slow":{"reasoningEffort":"low"}}},
            "family/override":{"limit":{"context":500000,"output":4096},"variants":{"fast":{"reasoningEffort":"high"}}}
        }}},
        "agent":{"build":{"system":"PRIVATE_PARENT","permission":{"shell":"deny"}},"reviewer":{"mode":"subagent","system":"PRIVATE_REVIEWER","model":"fixture/profile#slow"},"other":{"mode":"primary","system":"PRIVATE_OTHER","model":"fixture/profile#slow"}},
        "command":{
            "inferred":{"template":"EXPANDED $1","agent":"reviewer"},
            "true":{"template":"EXPANDED $1","agent":"other","subagent":true,"model":{"providerID":"fixture","modelID":"family/override#slow","variant":"fast"}},
            "legacy":{"template":"EXPANDED $1","subtask":true},
            "false":{"template":"EXPANDED $1","agent":"reviewer","subagent":false,"subtask":true,"model":"fixture/family/override#fast"},
            "inline":{"template":"EXPANDED $1","agent":"other","model":"fixture/family/override#fast"}
        }
    })
}

#[tokio::test]
async fn command_routing_real_wire_precedence_false_scope_and_unchanged_parent() {
    for (command, background, agent, model, effort, fresh) in [
        ("inferred", true, "reviewer", "profile", "low", false),
        ("true", true, "other", "family/override", "high", false),
        ("legacy", true, "build", "m", "", false),
        ("false", false, "reviewer", "family/override", "high", false),
        ("inline", false, "other", "family/override", "high", false),
        ("inferred", true, "reviewer", "profile", "low", true),
        ("false", false, "reviewer", "family/override", "high", true),
    ] {
        let server = Server::new().await;
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            project.join("opencode.json"),
            settings(&server.base).to_string(),
        )
        .unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
        let mut events = app.subscribe();
        let session = SessionId(format!("command-{command}"));
        if !fresh {
            app.create_session(session.clone()).await.unwrap();
        }
        let before = if fresh {
            app.home_selection(Action::Current).await
        } else {
            app.session_selection(session.clone(), false, Action::Current)
                .await
        }
        .unwrap();
        assert!(
            fresh
                || app
                    .session_selection(session.clone(), false, Action::Agent("reviewer".into()))
                    .await
                    .is_err()
        );
        let invocation = format!("/{command} literal");
        let turn = if fresh {
            app.submit_fresh(session.clone(), invocation.clone(), None)
                .await
        } else {
            app.submit(session.clone(), invocation.clone()).await
        }
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut parent_done = false;
            let mut child_done = !background;
            while !parent_done || !child_done {
                match events.recv().await.unwrap() {
                    CoreEvent::TurnFinished { turn: id, .. } if id == turn => parent_done = true,
                    CoreEvent::ChildNotice(_) => child_done = true,
                    CoreEvent::TurnFailed { error, .. } => panic!("{command}: {error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
        let selected = app
            .session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap();
        if background {
            assert_eq!(selected, before);
        } else {
            assert_eq!(selected.agent_id.as_deref(), Some(agent));
            assert_eq!(selected.model_id, model);
        }
        assert_eq!(
            app.read_history(session.clone())
                .await
                .unwrap()
                .iter()
                .find(|m| m.role == oc_core::session::Role::User)
                .unwrap()
                .text,
            invocation
        );
        let requests = server.requests.lock().unwrap().clone();
        let main = requests
            .iter()
            .filter(|request| {
                request["tools"]
                    .as_array()
                    .is_some_and(|tools| !tools.is_empty())
            })
            .collect::<Vec<_>>();
        assert_eq!(main.len(), 1, "{command}: {requests:?}");
        assert_eq!(main[0]["model"], model);
        if !effort.is_empty() {
            assert_eq!(main[0]["reasoning"]["effort"], effort);
        }
        let wire = main[0].to_string();
        assert!(wire.contains("EXPANDED literal"));
        assert!(wire.contains(match agent {
            "reviewer" => "PRIVATE_REVIEWER",
            "other" => "PRIVATE_OTHER",
            _ => "PRIVATE_PARENT",
        }));
        if agent != "build" {
            assert!(!wire.contains("PRIVATE_PARENT"));
        }
        if command == "false" {
            assert!(
                !main[0]["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == "shell")
            );
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        assert_eq!(
            app.session_selection(session.clone(), false, Action::Current)
                .await
                .unwrap(),
            selected
        );
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        server.close().await;
    }
}

#[tokio::test]
async fn command_routing_implicit_agent_preserves_false_authority_and_infers_child() {
    let server = Server::new().await;
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let data = root.path().join("data");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let mut config = settings(&server.base);
    config["command"]["same"] = serde_json::json!({"template":"SAME $1","subagent":false});
    config["command"]["child"] = serde_json::json!({"template":"CHILD $1"});
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), data.to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let session = SessionId("command-parent-agent".into());
    let (app, guard, _) = spawn_with_env(&project, &data, env.clone()).await.unwrap();
    let mut events = app.subscribe();
    for invocation in ["/false first", "/same second", "/child third"] {
        let turn = if invocation == "/false first" {
            app.submit_fresh(session.clone(), invocation.into(), None)
                .await
        } else {
            app.submit(session.clone(), invocation.into()).await
        }
        .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut done = false;
            let mut notice = invocation != "/child third";
            while !done || !notice {
                match events.recv().await.unwrap() {
                    CoreEvent::TurnFinished { turn: id, .. } if id == turn => done = true,
                    CoreEvent::ChildNotice(_) => notice = true,
                    CoreEvent::TurnFailed { error, .. } => panic!("{error}"),
                    _ => {}
                }
            }
        })
        .await
        .unwrap();
    }
    let selected = app
        .session_selection(session.clone(), false, Action::Current)
        .await
        .unwrap();
    assert_eq!(selected.agent_id.as_deref(), Some("reviewer"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap(),
        selected
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let requests = server.requests.lock().unwrap().clone();
    server.close().await;
    let main = requests
        .iter()
        .filter(|r| r["tools"].as_array().is_some_and(|t| !t.is_empty()))
        .collect::<Vec<_>>();
    assert_eq!(main.len(), 3);
    for request in main {
        assert!(request.to_string().contains("PRIVATE_REVIEWER"));
        assert!(
            !request["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "shell")
        );
    }
    let db = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM child_jobs", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn command_routing_native_admission_refuses_without_turn_child_or_dispatch() {
    for (case, fresh) in [
        "model", "variant", "disabled", "depth", "deny", "ask", "budget",
    ]
    .into_iter()
    .flat_map(|case| [(case, false), (case, true)])
    {
        let server = Server::new().await;
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let data = root.path().join("data");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let mut config = settings(&server.base);
        match case {
            "model" => config["command"]["inferred"]["model"] = "fixture/absent".into(),
            "variant" => config["command"]["inferred"]["model"] = "fixture/profile#absent".into(),
            "disabled" => config["agent"]["reviewer"]["disable"] = true.into(),
            "depth" => config["experimental"]["subagent_depth"] = 0.into(),
            "deny" | "ask" => config["permission"]["subagent"] = case.into(),
            "budget" => {
                config["provider"]["fixture"]["models"]["profile"]["limit"] =
                    serde_json::json!({"context":1,"output":1})
            }
            _ => unreachable!(),
        }
        std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
        let env = BTreeMap::from([
            ("HOME".into(), data.to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]);
        let (app, guard, _) = spawn_with_env(&project, &data, env).await.unwrap();
        let session = SessionId(format!("refuse-{case}"));
        if !fresh {
            app.create_session(session.clone()).await.unwrap();
        }
        let before = if fresh {
            app.home_selection(Action::Current).await
        } else {
            app.session_selection(session.clone(), false, Action::Current)
                .await
        }
        .unwrap();
        assert!(
            if fresh {
                app.submit_fresh(session.clone(), "/inferred literal".into(), None)
                    .await
            } else {
                app.submit(session.clone(), "/inferred literal".into())
                    .await
            }
            .is_err(),
            "{case} accepted"
        );
        assert_eq!(
            if fresh {
                app.home_selection(Action::Current).await
            } else {
                app.session_selection(session.clone(), false, Action::Current)
                    .await
            }
            .unwrap(),
            before
        );
        if !fresh {
            assert!(app.read_history(session.clone()).await.unwrap().is_empty());
        }
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
        let db = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sessions WHERE parent_id IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM tool_operations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(server.requests.lock().unwrap().is_empty());
        drop(db);
        server.close().await;
    }
}
