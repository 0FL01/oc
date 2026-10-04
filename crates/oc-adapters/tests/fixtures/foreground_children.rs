//! Real provider barriers for the scoped foreground scheduling owner.
use super::*;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Condvar;

type Script = dyn Fn(Value) -> String + Send + Sync;

#[tokio::test]
async fn concurrent_child_effect_invalidates_other_child_approved_preimage() {
    shared_child_preimage(false).await;
}

pub(super) async fn shared_child_preimage(background: bool) {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    let (mut harness, generation) = make_harness(allow_all());
    harness.catalog.models.insert(
        "gpt-guarded".into(),
        harness.catalog.models["agent-model"].clone(),
    );
    std::fs::write(harness._project.path().join("shared"), "before\n").unwrap();
    let runtime = runtime_of(&harness, generation);
    let mut guarded = agent("guarded", false, Some("test/gpt-guarded"));
    guarded
        .permissions
        .insert("apply_patch".into(), Permission::Ask);
    let sibling = agent("sibling", false, Some("test/agent-model"));
    runtime
        .publish_subagents(Some(catalog(2, vec![guarded, sibling])))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let (events, _) = tokio::sync::broadcast::channel(64);
    runtime.set_approval_events(&events);
    runtime.register_approval_consumer(false);
    let gate = Arc::new(Gate::default());
    let held = gate.clone();
    let peer = Peer::start(move |request| {
        if request["model"] == "m" {
            if outputs(&request).is_empty() {
                subagent_call(
                    "guarded-call",
                    serde_json::json!({"agent":"guarded","description":"Guarded","prompt":"GUARDED_TASK","background":background}),
                ) + &subagent_call(
                    "sibling-call",
                    serde_json::json!({"agent":"sibling","description":"Sibling","prompt":"SIBLING_TASK","background":background}),
                ) + &sse_completed()
            } else {
                sse_delta("parent") + &sse_completed()
            }
        } else if !outputs(&request).is_empty() {
            sse_delta("child") + &sse_completed()
        } else if request.to_string().contains("GUARDED_TASK") {
            sse_tool_call(
                "guarded-patch",
                "apply_patch",
                &serde_json::json!({"patchText":"*** Begin Patch\n*** Update File: shared\n@@\n-before\n+after\n*** End Patch"}),
            ) + &sse_completed()
        } else {
            held.wait();
            sse_tool_call(
                "sibling-effect",
                "bash",
                &serde_json::json!({"argv":["/bin/sh","-c","printf 'foreign\\n' >> shared"]}),
            ) + &sse_completed()
        }
    });
    let running = runtime.run_turn(params(
        "parent",
        "delegate",
        &harness,
        provider_of(&peer.base),
        &NO_CANCEL,
    ));
    let driver = async {
        wait_for(|| !runtime.pending_approvals().is_empty()).await;
        let request = runtime.pending_approvals().remove(0);
        assert_eq!(request.binding.agent.as_deref(), Some("guarded"));
        assert!(
            harness
                .db
                .list_tool_ops(&request.binding.session)
                .unwrap()
                .is_empty()
        );
        gate.release();
        wait_for(|| {
            std::fs::read(harness._project.path().join("shared")).unwrap() == b"before\nforeign\n"
        })
        .await;
        runtime
            .reply_approval(ApprovalReply {
                id: request.id,
                binding: request.binding,
                decision: ApprovalDecision::Once,
            })
            .unwrap();
        if background {
            wait_for(|| {
                harness
                    .db
                    .children_of("parent")
                    .unwrap()
                    .iter()
                    .all(|child| {
                        harness.db.list_tool_ops(child).unwrap().len() == 1
                            && messages(&harness.db, child)
                                .iter()
                                .any(|(role, text)| role == "assistant" && text == "child")
                    })
            })
            .await;
        }
    };
    let (result, ()) = tokio::join!(running, driver);
    assert_eq!(result.unwrap().status, TurnStatus::Completed);
    assert_eq!(
        std::fs::read(harness._project.path().join("shared")).unwrap(),
        b"before\nforeign\n"
    );
    let ops: Vec<_> = harness
        .db
        .children_of("parent")
        .unwrap()
        .iter()
        .flat_map(|child| harness.db.list_tool_ops(child).unwrap())
        .collect();
    assert_eq!(ops.len(), 2);
    assert!(
        ops.iter()
            .any(|op| op.name == "bash" && op.state == "completed")
    );
    assert!(
        ops.iter()
            .any(|op| op.name == "apply_patch" && op.state == "failed")
    );
    assert!(runtime.pending_approvals().is_empty());
}

pub(super) struct Peer {
    pub(super) base: String,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Peer {
    pub(super) fn start(script: impl Fn(Value) -> String + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let script: Arc<Script> = Arc::new(script);
        let thread = std::thread::spawn(move || {
            let mut workers = Vec::new();
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let script = script.clone();
                        workers.push(std::thread::spawn(move || serve(stream, &*script)));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
            for worker in workers {
                worker.join().unwrap();
            }
        });
        Self {
            base,
            stop,
            thread: Some(thread),
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

fn serve(mut stream: TcpStream, script: &Script) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let header;
    loop {
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
        assert!(bytes.len() <= 1024 * 1024);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            header = end + 4;
            break;
        }
    }
    let length: usize = String::from_utf8_lossy(&bytes[..header])
        .lines()
        .find_map(|line| {
            line.to_lowercase()
                .strip_prefix("content-length:")
                .map(|size| size.trim().parse().unwrap())
        })
        .unwrap();
    while bytes.len() < header + length {
        let mut buffer = [0; 4096];
        let n = stream.read(&mut buffer).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&buffer[..n]);
    }
    let body = script(serde_json::from_slice(&bytes[header..header + length]).unwrap());
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()); // cancellation closes an owned peer
}

#[derive(Default)]
pub(super) struct Gate(Mutex<bool>, Condvar);
impl Gate {
    pub(super) fn release(&self) {
        *self.0.lock().unwrap() = true;
        self.1.notify_all();
    }
    pub(super) fn wait(&self) {
        let (_guard, timed) = self
            .1
            .wait_timeout_while(self.0.lock().unwrap(), Duration::from_secs(5), |open| {
                !*open
            })
            .unwrap();
        assert!(!timed.timed_out(), "provider barrier watchdog");
    }
}

pub(super) fn outputs(request: &Value) -> Vec<(String, String)> {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["type"] == "function_call_output")
        .map(|item| {
            (
                item["call_id"].as_str().unwrap().into(),
                item["output"].as_str().unwrap().into(),
            )
        })
        .collect()
}

pub(super) async fn received(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
) -> (String, Value) {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap()
}

pub(super) async fn wait_for(mut predicate: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !predicate() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn distinct_children_overlap_reverse_completion_preserves_order_and_one_durable_effect() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    let mut helper = agent("custom", false, Some("test/agent-model"));
    helper.prompt = "OWN_CUSTOM_SYSTEM".into();
    runtime
        .publish_subagents(Some(catalog(2, vec![helper])))
        .unwrap();
    runtime.create_session("parent").unwrap();
    let gates = [Arc::new(Gate::default()), Arc::new(Gate::default())];
    let held = gates.clone();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let captured = requests.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let peer = Peer::start(move |request| {
        captured.lock().unwrap().push(request.clone());
        if request["model"] == "m" {
            if outputs(&request).is_empty() {
                subagent_call(
                    "original-a",
                    serde_json::json!({"agent":"custom","description":"A","prompt":"TASK_A"}),
                ) + &subagent_call(
                    "original-b",
                    serde_json::json!({"agent":"custom","description":"B","prompt":"TASK_B"}),
                ) + &sse_completed()
            } else {
                tx.send(("parent-followup".into(), request)).unwrap();
                sse_delta("parent") + &sse_completed()
            }
        } else {
            let text = request["input"].to_string();
            let index = usize::from(text.contains("TASK_B"));
            if outputs(&request).is_empty() {
                tx.send((format!("child-{index}"), request)).unwrap();
                held[index].wait();
                if index == 0 {
                    sse_tool_call(
                        "one-effect",
                        "bash",
                        &serde_json::json!({"argv":["/bin/sh","-c","printf x >> once"]}),
                    ) + &sse_completed()
                } else {
                    sse_delta("answer-b") + &sse_completed()
                }
            } else {
                assert_eq!(outputs(&request)[0].0, "one-effect");
                sse_delta("answer-a") + &sse_completed()
            }
        }
    });
    let running = runtime.run_turn(params(
        "parent",
        "PARENT_PRIVATE_TRANSCRIPT",
        &harness,
        provider_of(&peer.base),
        &NO_CANCEL,
    ));
    let driver = async {
        let first = received(&mut rx).await;
        let second = received(&mut rx).await;
        assert_ne!(first.0, second.0);
        for (_, request) in [&first, &second] {
            assert_eq!(request["model"], "agent-model");
            assert!(
                request["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["role"] == "developer"
                        && item.to_string().contains("OWN_CUSTOM_SYSTEM"))
            );
            assert!(!request.to_string().contains("PARENT_PRIVATE_TRANSCRIPT"));
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "both dispatch before either release"
        );
        gates[1].release();
        wait_for(|| {
            harness
                .db
                .children_of("parent")
                .unwrap()
                .iter()
                .any(|child| {
                    messages(&harness.db, child)
                        .iter()
                        .any(|(role, text)| role == "assistant" && text == "answer-b")
                })
        })
        .await;
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "parent cannot request while A is held"
        );
        gates[0].release();
        received(&mut rx).await.1
    };
    let (result, next) = tokio::join!(running, driver);
    let report = result.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    let pairs = outputs(&next);
    assert_eq!(
        pairs.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        ["original-a", "original-b"]
    );
    assert!(pairs[0].1.contains("answer-a") && pairs[1].1.contains("answer-b"));
    assert_eq!(
        std::fs::read(harness._project.path().join("once")).unwrap(),
        b"x"
    );
    let children = harness.db.children_of("parent").unwrap();
    assert_eq!(children.len(), 2);
    let ops: Vec<_> = children
        .iter()
        .flat_map(|child| harness.db.list_tool_ops(child).unwrap())
        .collect();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].state, "completed");
    assert_eq!(harness.db.list_tool_ops("parent").unwrap().len(), 2);
}

#[tokio::test]
async fn duplicate_continuations_serialize_without_losing_child_history_or_model() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation);
    runtime
        .publish_subagents(Some(catalog(2, vec![agent("helper", false, None)])))
        .unwrap();
    runtime.create_session("parent").unwrap();
    harness
        .db
        .create_child_session(
            "parent",
            "existing",
            Some("helper"),
            Some("test/agent-model"),
            Some("Existing"),
        )
        .unwrap();
    harness
        .db
        .set_pref(&format!("{SESSION_LOCATION_PREFIX}existing"), "work")
        .unwrap();
    let gate = Arc::new(Gate::default());
    let held = gate.clone();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let peer = Peer::start(move |request| {
        if request["model"] == "m" {
            if outputs(&request).is_empty() {
                subagent_call(
                    "cont-a",
                    serde_json::json!({"agent":"helper","description":"A","prompt":"FIRST_CONT","sessionID":"existing"}),
                ) + &subagent_call(
                    "cont-b",
                    serde_json::json!({"agent":"helper","description":"B","prompt":"SECOND_CONT","sessionID":"existing"}),
                ) + &sse_completed()
            } else {
                sse_delta("parent") + &sse_completed()
            }
        } else {
            let second = request.to_string().contains("SECOND_CONT");
            tx.send((
                if second { "second" } else { "first" }.into(),
                request.clone(),
            ))
            .unwrap();
            if !second {
                held.wait();
            }
            sse_delta(if second {
                "second-answer"
            } else {
                "first-answer"
            }) + &sse_completed()
        }
    });
    let running = runtime.run_turn(params(
        "parent",
        "continue twice",
        &harness,
        provider_of(&peer.base),
        &NO_CANCEL,
    ));
    let driver = async {
        assert_eq!(received(&mut rx).await.0, "first");
        assert!(rx.try_recv().is_err());
        assert_eq!(messages(&harness.db, "existing").len(), 1);
        gate.release();
        let (which, request) = received(&mut rx).await;
        assert_eq!(which, "second");
        assert_eq!(request["model"], "agent-model");
        assert!(request.to_string().contains("first-answer"));
    };
    let (result, ()) = tokio::join!(running, driver);
    assert_eq!(result.unwrap().status, TurnStatus::Completed);
    assert_eq!(messages(&harness.db, "existing").len(), 4);
    assert_eq!(harness.db.children_of("parent").unwrap(), ["existing"]);
}

#[tokio::test]
async fn child_question_dismissal_is_local_and_parent_cancel_joins_both_peers() {
    for parent_cancel in [false, true] {
        let mut permissions = allow_all();
        permissions.insert("question".into(), Permission::Allow);
        let (harness, generation) = make_harness(permissions);
        let runtime = runtime_of(&harness, generation);
        let mut helper = agent("helper", false, Some("test/agent-model"));
        helper
            .permissions
            .insert("question".into(), Permission::Allow);
        runtime
            .publish_subagents(Some(catalog(2, vec![helper])))
            .unwrap();
        runtime.create_session("parent").unwrap();
        let (events, _) = tokio::sync::broadcast::channel(64);
        runtime.set_approval_events(&events);
        runtime.register_question_consumer();
        let cancel = AtomicBool::new(false);
        let gates = [Arc::new(Gate::default()), Arc::new(Gate::default())];
        let held = gates.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let peer = Peer::start(move |request| {
            captured.lock().unwrap().push(request.clone());
            if request["model"] == "m" {
                if outputs(&request).is_empty() {
                    subagent_call(
                        "a",
                        serde_json::json!({"agent":"helper","description":"A","prompt":"TASK_A"}),
                    ) + &subagent_call(
                        "b",
                        serde_json::json!({"agent":"helper","description":"B","prompt":"TASK_B"}),
                    ) + &sse_completed()
                } else {
                    sse_delta("parent survived") + &sse_completed()
                }
            } else {
                let index = usize::from(request.to_string().contains("TASK_B"));
                tx.send((index.to_string(), request)).unwrap();
                held[index].wait();
                if parent_cancel || index == 1 {
                    sse_delta("sibling survived") + &sse_completed()
                } else {
                    sse_tool_call(
                        "dismiss",
                        "question",
                        &serde_json::json!({"questions":[{"question":"Q","header":"H","options":[{"label":"L","description":"D"}]}]}),
                    ) + &sse_completed()
                }
            }
        });
        let running = runtime.run_turn(params(
            "parent",
            "delegate",
            &harness,
            provider_of(&peer.base),
            &cancel,
        ));
        let driver = async {
            received(&mut rx).await;
            received(&mut rx).await;
            if parent_cancel {
                cancel.store(true, Ordering::Release);
            }
            gates[0].release();
            if !parent_cancel {
                wait_for(|| !runtime.pending_questions().is_empty()).await;
                let question = runtime.pending_questions().remove(0);
                runtime
                    .reply_question(oc_core::question::QuestionReply {
                        id: question.id,
                        binding: question.binding,
                        decision: oc_core::question::QuestionDecision::Cancelled,
                    })
                    .unwrap();
                wait_for(|| {
                    harness
                        .db
                        .children_of("parent")
                        .unwrap()
                        .iter()
                        .any(|child| {
                            harness
                                .db
                                .list_tool_ops(child)
                                .unwrap()
                                .iter()
                                .any(|op| op.name == "question" && op.state == "cancelled")
                        })
                })
                .await;
                assert!(
                    !cancel.load(Ordering::Acquire),
                    "child dismissal must not cancel parent/sibling"
                );
                assert_eq!(requests.lock().unwrap().len(), 3);
            }
            gates[1].release();
        };
        let (result, ()) = tokio::join!(running, driver);
        let report = result.unwrap();
        assert_eq!(
            report.status,
            if parent_cancel {
                TurnStatus::Cancelled
            } else {
                TurnStatus::Completed
            }
        );
        assert_eq!(harness.db.children_of("parent").unwrap().len(), 2);
        let ops = harness.db.list_tool_ops("parent").unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops.iter().all(|op| op.state != "started"));
        if parent_cancel {
            assert_eq!(requests.lock().unwrap().len(), 3);
        } else {
            assert!(report.calls[1].output.contains("sibling survived"));
        }
    }
}

#[tokio::test]
async fn batched_pre_effect_guards_and_child_budget_fail_without_sibling_cancellation() {
    for case in ["deny", "ask", "depth", "model", "budget", "unclosed"] {
        let mut permissions = allow_all();
        if case == "deny" {
            permissions.insert("subagent".into(), Permission::Deny);
        }
        if case == "ask" {
            permissions.insert("subagent".into(), Permission::Ask);
        }
        let (mut harness, generation) = make_harness(permissions);
        harness.catalog.models.insert(
            "tiny".into(),
            serde_json::json!({"limit":{"context":40,"output":20}}),
        );
        let runtime = runtime_of(&harness, generation);
        runtime
            .publish_subagents(Some(catalog(
                if case == "depth" { 0 } else { 2 },
                vec![agent("helper", false, Some("test/agent-model"))],
            )))
            .unwrap();
        runtime.create_session("parent").unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let peer = Peer::start(move |request| {
            captured.lock().unwrap().push(request.clone());
            if request["model"] == "m" && outputs(&request).is_empty() {
                let mut args =
                    serde_json::json!({"agent":"helper","description":"A","prompt":"NO_EFFECT"});
                if case == "model" {
                    args["model"] = "test/missing".into();
                }
                if case == "budget" {
                    args["model"] = "test/tiny".into();
                }
                let second = if matches!(case, "model" | "budget") {
                    serde_json::json!({"agent":"helper","description":"B","prompt":"VALID_SIBLING"})
                } else {
                    args.clone()
                };
                let batch = subagent_call("guard-a", args) + &subagent_call("guard-b", second);
                if case == "unclosed" {
                    batch
                } else {
                    batch + &sse_completed()
                }
            } else {
                sse_delta("survived") + &sse_completed()
            }
        });
        let cancel = AtomicBool::new(false);
        let result = runtime
            .run_turn(params(
                "parent",
                "guards",
                &harness,
                provider_of(&peer.base),
                &cancel,
            ))
            .await;
        if case == "ask" {
            assert!(result.is_err(), "{case}");
            assert!(
                harness.db.list_tool_ops("parent").unwrap().is_empty(),
                "{case}: no intent before Ask/closed response"
            );
        } else if case == "unclosed" {
            assert_eq!(result.unwrap().status, TurnStatus::Incomplete);
            assert!(
                harness.db.list_tool_ops("parent").unwrap().is_empty(),
                "open response cannot create intent"
            );
        } else {
            let report = result.unwrap();
            assert_eq!(report.status, TurnStatus::Completed, "{case}");
            assert_eq!(report.calls.len(), 2, "{case}");
            assert!(
                report.calls[0].output.starts_with("error: "),
                "{case}: {:?}",
                report.calls
            );
            if matches!(case, "model" | "budget") {
                assert!(report.calls[1].output.contains("survived"));
                assert!(!cancel.load(Ordering::Acquire));
            }
        }
        let children = harness.db.children_of("parent").unwrap();
        assert_eq!(
            children.len(),
            match case {
                "model" => 1,
                "budget" => 2,
                _ => 0,
            },
            "{case}"
        );
        let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
        let child_turns: i64 = sql
            .query_row(
                "SELECT count(*) FROM turns WHERE session_id!='parent'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            child_turns,
            i64::from(matches!(case, "model" | "budget")),
            "{case}: invalid budget must fail before turn acceptance"
        );
        for child in children {
            assert!(harness.db.list_tool_ops(&child).unwrap().is_empty());
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            match case {
                "ask" => 1,
                "unclosed" => 11,
                "model" | "budget" => 3,
                _ => 2,
            },
            "{case}"
        );
    }
}
