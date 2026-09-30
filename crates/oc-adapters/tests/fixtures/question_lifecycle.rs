use super::*;
use oc_core::{core_app::CoreEvent, domain::SessionId, question::*};

struct Peer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    requests: CapturedRequests,
}
impl Peer {
    fn start(script: Vec<String>, data: std::path::PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let thread = std::thread::spawn(move || {
            let mut script = VecDeque::from(script);
            while !stopping.load(Ordering::Acquire) {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("peer accept: {e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((key, value)) = line.split_once(':')
                        && key.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                assert!(length <= 1024 * 1024);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                if request["max_output_tokens"] == 256
                    && request["tools"].as_array().is_none_or(Vec::is_empty)
                {
                    let payload = sse_delta("Fixture title") + &sse_completed();
                    reader.get_mut().write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", payload.len(), payload).as_bytes()).unwrap();
                    continue;
                }
                if captured.lock().unwrap().len() == 1 {
                    // The provider must never observe an answer ahead of its durable outcome.
                    let db = rusqlite::Connection::open(data.join("oc.sqlite")).unwrap();
                    let (state, output): (String, String) = db
                        .query_row(
                            "SELECT state,output FROM tool_operations WHERE name='question'",
                            [],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .unwrap();
                    assert_eq!(state, "completed");
                    assert!(
                        request["input"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .any(|i| i["type"] == "function_call_output" && i["output"] == output)
                    );
                }
                captured.lock().unwrap().push(request);
                let payload = script
                    .pop_front()
                    .expect("unexpected provider continuation");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                reader.get_mut().write_all(response.as_bytes()).unwrap();
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
            requests,
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let result = self.thread.take().unwrap().join();
        if !std::thread::panicking() {
            result.unwrap();
        }
    }
}
fn questions() -> serde_json::Value {
    serde_json::json!({"questions":[{"question":"Choice?","header":"Choice","options":[{"label":"One","description":"first"}]}]})
}

#[tokio::test]
async fn question_real_core_binding_commit_and_restart_no_reask() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let peer = Peer::start(
        vec![
            sse_tool_call("ask", "question", &questions()) + &sse_completed(),
            sse_delta("done") + &sse_completed(),
        ],
        data.path().to_owned(),
    );
    let config = serde_json::json!({"model":"fixture/main","compaction":{"auto":false},"agent":{"title":{"disable":true}}, "provider":{"fixture":{"npm":"@ai-sdk/openai","options":{"baseURL":peer.url,"apiKey":"synthetic"},"models":{"main":{}}}}});
    std::fs::write(project.path().join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().display().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env.clone())
            .await
            .unwrap();
    let session = SessionId("question-core".into());
    app.create_session(session.clone()).await.unwrap();
    app.register_question_consumer().await.unwrap();
    let mut events = app.subscribe();
    app.submit(session.clone(), "ask".into()).await.unwrap();
    let request = tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let CoreEvent::QuestionAsked(r) = events.recv().await.unwrap() {
                break r;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.pending_questions().await.unwrap(),
        vec![request.clone()]
    );
    let reply = QuestionReply {
        id: request.id,
        binding: request.binding.clone(),
        decision: QuestionDecision::Answers(vec![vec!["One".into()]]),
    };
    for binding in [
        oc_core::approval::ApprovalBinding {
            session: "foreign".into(),
            ..reply.binding.clone()
        },
        oc_core::approval::ApprovalBinding {
            generation: reply.binding.generation + 1,
            ..reply.binding.clone()
        },
        oc_core::approval::ApprovalBinding {
            operation: "foreign".into(),
            ..reply.binding.clone()
        },
    ] {
        assert_eq!(
            app.reply_question(QuestionReply {
                binding,
                ..reply.clone()
            })
            .await,
            Err(QuestionReplyError::BindingMismatch)
        );
    }
    assert_eq!(
        app.reply_question(QuestionReply {
            decision: QuestionDecision::Answers(vec![]),
            ..reply.clone()
        })
        .await,
        Err(QuestionReplyError::InvalidAnswers)
    );
    assert_eq!(app.pending_questions().await.unwrap(), vec![request]);
    app.reply_question(reply.clone()).await.unwrap();
    assert_eq!(
        app.reply_question(reply.clone()).await,
        Err(QuestionReplyError::Stale)
    );
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let page = app
        .history_page(session.clone(), None, None, 8)
        .await
        .unwrap();
    assert_eq!(
        page.rows
            .iter()
            .filter_map(|r| r.turn.as_ref())
            .flat_map(|t| &t.parts)
            .filter_map(|p| if let oc_core::queries::TranscriptPart::Tool(t) = p {
                t.question.as_ref()
            } else {
                None
            })
            .next()
            .unwrap()
            .answers,
        vec![vec!["One"]]
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    drop(app);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(project.path(), data.path(), env)
            .await
            .unwrap();
    assert!(app.pending_questions().await.unwrap().is_empty());
    assert_eq!(
        app.reply_question(reply).await,
        Err(QuestionReplyError::Stale)
    );
    let reopened = app.history_page(session, None, None, 8).await.unwrap();
    assert_eq!(page.rows, reopened.rows);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert_eq!(peer.requests.lock().unwrap().len(), 2);
    drop(peer);
}
