use super::*;
use oc_core::{core_app::CoreEvent, domain::SessionId};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn w3_headless_shared_owner_rejects_grandchild_and_leaves_foreign_ask() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let home = root.path().join("home");
    let data = root.path().join("data");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    std::fs::write(config.join("opencode.json"), serde_json::json!({
        "model":"fixture/m",
        "providers":{"fixture":{"package":"@ai-sdk/openai","settings":{"baseURL":base,"apiKey":"fixture"},"models":{"m":{"limit":{"context":65536,"output":4096}}}}},
        "experimental":{"subagent_depth":2},
        "agents":{"title":{"disabled":true},"helper":{"mode":"subagent","system":"Delegate to worker."},"worker":{"mode":"subagent","system":"Ask shell."}},
        "permissions":[{"action":"shell","resource":"*","effect":"ask"},{"action":"subagent","resource":"*","effect":"allow"}]
    }).to_string()).unwrap();
    let mut process = Process(Some(
        Command::new(env!("CARGO_BIN_EXE_oc2"))
            .current_dir(&project)
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("PATH", "/usr/bin:/bin")
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .arg("--data-dir")
            .arg(&data)
            .args(["run", "delegate twice", "--session", "owned-root"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    ));
    // Holding the first response keeps this Run alive while another native view asks.
    let (mut root_request, _) = accept(&listener).await;
    let (foreign, guard, _) =
        oc_adapters::native::attach(&data, Some(&project), oc_adapters::native::Lease::Tui)
            .await
            .unwrap();
    foreign.register_approval_consumer(false).await.unwrap();
    let session = SessionId("foreign-root".into());
    foreign.create_session(session.clone()).await.unwrap();
    let mut events = foreign.subscribe();
    foreign
        .submit(session.clone(), "foreign shell".into())
        .await
        .unwrap();
    let (mut request, _) = accept(&listener).await;
    respond(
        &mut request,
        &tool(
            "foreign",
            "bash",
            serde_json::json!({"argv":["/usr/bin/touch","foreign-marker"]}),
        ),
    );
    let foreign_ask = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = match events.recv().await.unwrap() {
                CoreEvent::Projection { event, .. } => *event,
                event => event,
            };
            if let CoreEvent::PermissionAsked(request) = event {
                break request;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(foreign_ask.binding.session, session.0);
    let delegate = |id, agent| {
        tool(
            id,
            "subagent",
            serde_json::json!({"agent":agent,"description":"Owned descendant","prompt":"Delegate or ask shell"}),
        )
    };
    respond(&mut root_request, &delegate("child", "helper"));
    let (mut request, _) = accept(&listener).await;
    respond(&mut request, &delegate("grandchild", "worker"));
    let (mut request, _) = accept(&listener).await;
    respond(
        &mut request,
        &tool(
            "owned",
            "bash",
            serde_json::json!({"argv":["/usr/bin/touch","owned-marker"]}),
        ),
    );
    let (mut request, continuation) = accept(&listener).await;
    let outcome = continuation["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == "owned")
        .unwrap();
    assert!(outcome["output"].as_str().unwrap().contains("This non-interactive run cannot ask the user for permission, so the request was rejected. Continue without this action."));
    respond(&mut request, &finished());
    for _ in 0..2 {
        let (mut request, _) = accept(&listener).await;
        respond(&mut request, &finished());
    }
    tokio::time::timeout(Duration::from_secs(10), async {
        while process.0.as_mut().unwrap().try_wait().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let output = process.0.take().unwrap().wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        foreign.pending_approvals().await.unwrap(),
        vec![foreign_ask]
    );
    assert!(!project.join("owned-marker").exists());
    assert!(!project.join("foreign-marker").exists());
    foreign.cancel(session).await.unwrap();
    foreign.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM permission_grants", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

async fn accept(listener: &tokio::net::TcpListener) -> (std::net::TcpStream, serde_json::Value) {
    loop {
        let (stream, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut stream = stream.into_std().unwrap();
        stream.set_nonblocking(false).unwrap();
        let request = read_request(&mut stream);
        if request["max_output_tokens"] == 256
            && request["tools"]
                .as_array()
                .is_none_or(|tools| tools.is_empty())
        {
            respond(&mut stream, &finished());
            continue;
        }
        return (stream, request);
    }
}

fn respond(stream: &mut std::net::TcpStream, body: &str) {
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
}

struct Process(Option<std::process::Child>);
impl Drop for Process {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
