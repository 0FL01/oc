//! Reuses the real compaction fake-wire owner and saved conversation contexts.
use super::*;

#[tokio::test]
async fn prm01_once_and_always_file_approvals_never_promote_source_ask() {
    use oc_core::approval::{ApprovalDecision, ApprovalReply};
    for decision in [ApprovalDecision::Once, ApprovalDecision::Always] {
        for source_effect in ["ask", "allow"] {
            let data = tempfile::tempdir().unwrap();
            let project = tempfile::tempdir().unwrap();
            std::fs::create_dir(project.path().join("nested")).unwrap();
            std::fs::write(project.path().join("nested/file.txt"), "approved read\n").unwrap();
            std::fs::write(
                project.path().join("nested/AGENTS.md"),
                "SOURCE_FOR_APPROVED_READ\n",
            )
            .unwrap();
            let db = Db::open(data.path()).unwrap();
            db.create_bound_session("s", "work").unwrap();
            let mut generation = Generation::default();
            generation
                .permissions
                .insert("read".into(), Permission::Ask);
            generation.permission_rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({"permission":{"read":{"*":"ask","nested/file.txt":"ask","nested/AGENTS.md":source_effect}}})).unwrap();
            generation.compaction.auto = false;
            let runtime = runtime(&db, project.path(), generation);
            runtime
                .publish_instruction_roots(vec![crate::instructions::Root {
                    path: project.path().into(),
                    dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
                    origin: crate::instructions::Origin::Project,
                    baseline: None,
                }])
                .unwrap();
            let (events, _receiver) = tokio::sync::broadcast::channel(16);
            runtime.set_approval_events(&events);
            runtime.register_approval_consumer(false);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let provider = config(&listener);
            let peer = tokio::spawn(async move {
                let (mut socket, _) = read_request(&listener).await;
                let item = serde_json::json!({"type":"function_call","id":"read-approved","call_id":"read-approved","name":"read","arguments":"{\"path\":\"nested/file.txt\",\"limit\":2}","status":"completed"});
                let script = [serde_json::json!({"type":"response.output_item.added","item":item}),
                    serde_json::json!({"type":"response.output_item.done","item":item}),
                    serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item]}})]
                    .iter().map(|event|format!("data: {event}\n\n")).collect::<String>();
                respond(&mut socket, &script).await;
                let (mut socket, request) = read_request(&listener).await;
                assert_eq!(
                    request["input"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|item| item["role"] == "developer"
                            && item["content"]
                                .to_string()
                                .contains("SOURCE_FOR_APPROVED_READ"))
                        .count(),
                    usize::from(source_effect == "allow")
                );
                assert!(request["input"].as_array().unwrap().iter().any(|item| {
                    item["type"] == "function_call_output"
                        && item["output"]
                            .as_str()
                            .is_some_and(|s| s.contains("approved read"))
                }));
                respond(&mut socket, &sse("done")).await;
            });
            let catalog = catalog();
            let cancel = AtomicBool::new(false);
            let turn = runtime.run_turn(TurnParams {
                session: "s".into(),
                prompt: "read".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider,
                cancel: &cancel,
            });
            let approve = async {
                let request = tokio::time::timeout(Duration::from_secs(4), async {
                    loop {
                        if let Some(request) = runtime.pending_approvals().into_iter().next() {
                            break request;
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .unwrap();
                assert_eq!(request.resources, vec!["nested/file.txt"]);
                runtime
                    .reply_approval(ApprovalReply {
                        id: request.id,
                        binding: request.binding,
                        decision: decision.clone(),
                    })
                    .unwrap();
            };
            let (report, ()) = tokio::join!(turn, approve);
            peer.await.unwrap();
            assert_eq!(report.unwrap().status, TurnStatus::Completed);
            assert_eq!(db.list_tool_ops("s").unwrap()[0].state, "completed");
            assert_eq!(
                db.instruction_view("s").unwrap().1.len(),
                usize::from(source_effect == "allow")
            );
            assert!(runtime.pending_approvals().is_empty());
            runtime.shutdown_mcp().await.unwrap();
        }
    }
}

fn seed_instructions(runtime: &Runtime<'_>, turn: &str, text: &str) -> String {
    let accepted = runtime
        .db
        .accept_turn(
            turn,
            "s",
            text,
            text,
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap();
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(accepted.user_message.clone());
    log.input.push(InputItem::message(InputRole::User, text));
    let lane = runtime.primary_lane(&runtime.current.read().unwrap());
    let policy = RuntimePolicy::with_rules(&lane.permissions, &lane.permission_rules)
        .with_root(&runtime.roots.project);
    let (revision, sources) = runtime
        .instruction_sources("s", &policy, &AtomicBool::new(false))
        .unwrap();
    runtime
        .db
        .checkpoint_instructions(
            turn,
            &mut log,
            revision,
            &sources,
            usize::from(revision != 0),
            None,
        )
        .unwrap();
    log.input
        .push(InputItem::message(InputRole::Assistant, "answer"));
    runtime
        .db
        .commit_turn(
            turn,
            "completed",
            Some(&log.to_json().to_string()),
            Some("answer"),
        )
        .unwrap();
    accepted.user_message
}

fn assert_latest(
    runtime: &Runtime<'_>,
    session: &str,
    latest: &crate::instructions::Fact,
    old: &crate::instructions::Fact,
) {
    let active = runtime.active_projection(session).unwrap();
    let history = runtime
        .wire_history(
            session,
            &active.projected,
            &active.blocks,
            "m",
            "fixture",
            None,
            active.after_seq,
        )
        .unwrap();
    let mut input = Vec::new();
    crate::instructions::reconcile(
        &mut input,
        &history,
        &runtime.db.instruction_view(session).unwrap().1,
    );
    input.extend(history);
    assert_eq!(
        input.iter().filter(|item| **item == latest.input()).count(),
        1
    );
    assert!(!input.contains(&old.input()));
}

#[tokio::test]
async fn prm01_latest_sources_survive_real_dcp_compaction_revert_fork_and_reopen() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("AGENTS.md");
    std::fs::write(&source, "OLD_INSTRUCTION\n").unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.apply_dcp_schema().unwrap();
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("read".into(), Permission::Allow);
    generation
        .permissions
        .insert("compress".into(), Permission::Allow);
    generation.compaction.keep_tokens = 0;
    let runtime = runtime(&db, project.path(), generation.clone());
    runtime
        .publish_instruction_roots(vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: Some(Arc::new(crate::instructions::Source::baseline(
                &source,
                project.path(),
                crate::instructions::Origin::Project,
                "OLD_INSTRUCTION\n",
            ))),
        }])
        .unwrap();
    let first = seed_instructions(&runtime, "one", &"closed work ".repeat(1000));
    let old = db.instruction_view("s").unwrap().1[0].clone();
    let original = db.turn_result("one").unwrap();
    std::fs::write(&source, "LATEST_INSTRUCTION\n").unwrap();
    runtime
        .publish_instruction_roots(vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: Some(Arc::new(crate::instructions::Source::baseline(
                &source,
                project.path(),
                crate::instructions::Origin::Project,
                "LATEST_INSTRUCTION\n",
            ))),
        }])
        .unwrap();
    let second = seed_instructions(&runtime, "two", "second");
    let latest = db.instruction_view("s").unwrap().1[0].clone();
    assert_eq!(latest.change, "changed");
    seed_instructions(&runtime, "three", "tail");
    assert_latest(&runtime, "s", &latest, &old);
    let fork = db
        .fork_session("s", &second, "work", "fixture", "{}")
        .unwrap();
    assert_eq!(
        db.instruction_view(&fork.session.0).unwrap().1,
        vec![latest.clone()]
    );
    assert_latest(&runtime, &fork.session.0, &latest, &old);
    let first_answer = db.read_history_full("s").unwrap()[1].0.clone();
    runtime
        .run_compress(
            "s",
            &serde_json::json!({"topic":"closed work","content":[{
        "startId":first,"endId":first_answer,"summary":"Work completed."}]}),
            &ProtectedSpec::default(),
        )
        .unwrap();
    let saved_blocks = db.load_compression_blocks("s").unwrap();
    assert!(!saved_blocks.is_empty());
    assert_latest(&runtime, "s", &latest, &old);
    db.change_conversation(
        "s",
        ConversationAction::Revert {
            message: oc_core::session::MessageId(second),
        },
    )
    .unwrap();
    // Revert restores its exact saved DCP context, not external source bodies.
    assert!(db.load_compression_blocks("s").unwrap().is_empty());
    assert_latest(&runtime, "s", &latest, &old);
    db.change_conversation("s", ConversationAction::Redo)
        .unwrap();
    assert_eq!(db.load_compression_blocks("s").unwrap(), saved_blocks);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let task = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        assert_eq!(request["tools"], serde_json::json!([]));
        let rendered = request["input"].to_string();
        assert!(!rendered.contains("OLD_INSTRUCTION"));
        assert!(
            !rendered.contains("LATEST_INSTRUCTION"),
            "instruction data entered summarizer history"
        );
        respond(&mut socket, &sse("## Objective\nContinue the work.")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        runtime
            .deliver_compaction("s", &catalog(), "m", None, &provider)
            .await
            .unwrap()
    );
    task.await.unwrap();
    assert_latest(&runtime, "s", &latest, &old);
    assert_eq!(db.turn_result("one").unwrap(), original);
    let operations = db.list_tool_ops("s").unwrap();
    runtime.shutdown_mcp().await.unwrap();
    drop(runtime);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    let runtime = super::runtime(&db, project.path(), generation);
    assert_latest(&runtime, "s", &latest, &old);
    assert_eq!(db.list_tool_ops("s").unwrap(), operations);
    assert_eq!(db.turn_result("one").unwrap(), original);
    runtime.shutdown_mcp().await.unwrap();
}
