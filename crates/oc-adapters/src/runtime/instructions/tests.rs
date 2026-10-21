//! Reuses the real compaction fake-wire owner and saved conversation contexts.
use super::*;

#[tokio::test]
async fn cfg17_saved_allow_cannot_bypass_final_configured_deny() {
    for denied in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join("blocked.txt"),
            "SAVED_GRANT_READ_CANARY",
        )
        .unwrap();
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        let sources = [
            crate::config::Source {
                path: "/first.json".into(), trusted: false,
                text: serde_json::json!({"permissions":[{"action":"read","resource":"*","effect":"ask"}]}).to_string(),
            },
            crate::config::Source {
                path: "/last.json".into(), trusted: false,
                text: if denied {
                    serde_json::json!({"permissions":[{"action":"read","resource":"blocked.txt","effect":"deny"}]})
                } else { serde_json::json!({}) }.to_string(),
            },
        ];
        let mut generation = crate::config::assemble(&sources, &BTreeMap::new(), None).unwrap();
        generation.compaction.auto = false;
        let runtime = runtime(&db, project.path(), generation);
        let patterns =
            crate::approval::grant_resources(project.path(), "read", &["blocked.txt".into()]);
        let identity = crate::approval::project_identity(project.path()).unwrap();
        db.save_permission_grants(&identity, "read", &patterns)
            .unwrap();
        assert!(
            db.permission_grant_matches(&identity, "read", &patterns[0])
                .unwrap()
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = config(&listener);
        let peer = tokio::spawn(async move {
            let (mut socket, request) = read_request(&listener).await;
            assert!(
                request["tools"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|tool| tool["name"] == "read")
            );
            let item = serde_json::json!({"type":"function_call","id":"saved-read","call_id":"saved-read","name":"read","arguments":"{\"path\":\"blocked.txt\",\"limit\":2}","status":"completed"});
            let script = [serde_json::json!({"type":"response.output_item.added","item":item}),
                serde_json::json!({"type":"response.output_item.done","item":item}),
                serde_json::json!({"type":"response.completed","response":{"status":"completed","output":[item]}})]
                .iter().map(|event|format!("data: {event}\n\n")).collect::<String>();
            respond(&mut socket, &script).await;
            let (mut socket, request) = read_request(&listener).await;
            let output = request["input"]
                .as_array()
                .unwrap()
                .iter()
                .find(|item| item["type"] == "function_call_output")
                .unwrap()["output"]
                .as_str()
                .unwrap();
            assert_eq!(output.contains("SAVED_GRANT_READ_CANARY"), !denied);
            if denied {
                let diagnostic: serde_json::Value = serde_json::from_str(output).unwrap();
                assert_eq!(diagnostic["status"], "failed");
                assert_eq!(diagnostic["error"]["tool"], "read");
                assert_eq!(diagnostic["error"]["code"], "permission_denied");
            }
            respond(&mut socket, &sse("done")).await;
        });
        let catalog = catalog();
        let cancel = AtomicBool::new(false);
        let report = tokio::time::timeout(
            Duration::from_secs(4),
            runtime.run_turn(TurnParams {
                session: "s".into(),
                prompt: "read".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider,
                cancel: &cancel,
            }),
        )
        .await
        .unwrap()
        .unwrap();
        peer.await.unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        assert_eq!(
            db.list_tool_ops("s").unwrap()[0].state,
            if denied { "failed" } else { "completed" }
        );
        assert!(runtime.pending_approvals().is_empty());
        assert_eq!(
            std::fs::read_to_string(project.path().join("blocked.txt")).unwrap(),
            "SAVED_GRANT_READ_CANARY"
        );
    }
}

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
            generation.permission_rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({"permissions": [{"action": "read", "resource": "*", "effect": "ask"}, {"action": "read", "resource": "nested/file.txt", "effect": "ask"}, {"action": "read", "resource": "nested/AGENTS.md", "effect": source_effect}]})).unwrap();
            generation.compaction.auto = false;
            let runtime = runtime(&db, project.path(), generation);
            runtime.update_execution(|view| {
                view.workspace.instruction_roots = vec![crate::instructions::Root {
                    path: project.path().into(),
                    dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
                    origin: crate::instructions::Origin::Project,
                    baseline: None,
                }];
            });
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
            let mut finished = Vec::new();
            let turn = runtime.run_turn_with_tool_events(
                TurnParams {
                    session: "s".into(),
                    prompt: "read".into(),
                    invocation: None,
                    catalog: &catalog,
                    model_id: "m".into(),
                    variant: None,
                    max_output: 1024,
                    provider,
                    cancel: &cancel,
                },
                |_| {},
                |_, _| {},
                |_, _| {},
                |_, event| {
                    if let ToolCallEvent::Finished {
                        op,
                        output_presentation,
                        ..
                    } = event
                    {
                        finished.push((op.clone(), output_presentation.clone()));
                    }
                },
            );
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
            assert_eq!(finished.len(), 1);
            let presentation = finished[0].1.as_ref().unwrap();
            assert_eq!(
                presentation.instructions,
                (source_effect == "allow").then(|| oc_core::tool_output::InstructionLoads {
                    paths: vec!["nested/AGENTS.md".into()],
                    total: 1
                })
            );
            let conn = rusqlite::Connection::open_with_flags(
                data.path().join("oc.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            assert_eq!(
                Db::tool_presentation_in(&conn, "s", &finished[0].0).unwrap(),
                finished[0].1
            );
            assert_eq!(
                db.instruction_view("s").unwrap().1.len(),
                usize::from(source_effect == "allow")
            );
            assert!(runtime.pending_approvals().is_empty());
            runtime.shutdown_mcp().await.unwrap();
        }
    }
}

#[tokio::test]
async fn prm01_nested_repository_and_worktree_reads_publish_only_new_committed_receipts() {
    let data = tempfile::tempdir().unwrap();
    let checkout = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let location = checkout.path().join("location");
    std::fs::create_dir_all(location.join(".local/opencode")).unwrap();
    std::fs::create_dir(checkout.path().join("sibling")).unwrap();
    // A linked-worktree marker belongs to this checkout, not its gitdir target.
    std::fs::write(checkout.path().join(".git"), "gitdir: /not-this-checkout\n").unwrap();
    std::fs::write(location.join(".local/opencode/.git"), "nested repository\n").unwrap();
    std::fs::write(location.join(".local/opencode/file.txt"), "nested read\n").unwrap();
    std::fs::write(
        location.join(".local/opencode/AGENTS.md"),
        "NESTED_INSTRUCTION_BODY\n",
    )
    .unwrap();
    std::fs::write(checkout.path().join("sibling/file.txt"), "sibling read\n").unwrap();
    std::fs::write(
        checkout.path().join("sibling/AGENTS.md"),
        "SIBLING_INSTRUCTION_BODY\n",
    )
    .unwrap();
    std::fs::write(
        checkout.path().join("AGENTS.md"),
        "ANCESTOR_MUST_NOT_AUTO_LOAD\n",
    )
    .unwrap();
    std::fs::write(external.path().join("file.txt"), "external approved read\n").unwrap();
    std::fs::write(
        external.path().join("AGENTS.md"),
        "EXTERNAL_MUST_NOT_AUTO_LOAD\n",
    )
    .unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("read".into(), Permission::Allow);
    generation
        .permissions
        .insert("external_directory".into(), Permission::Allow);
    generation.compaction.auto = false;
    let runtime = runtime(&db, &location, generation);
    let (path, dir) = crate::files::Files::enclosing_worktree(&location).unwrap();
    assert_eq!(path, checkout.path());
    runtime.update_execution(|view| {
        view.workspace.instruction_roots = vec![
            crate::instructions::Root {
                path: location.clone(),
                dir: Arc::new(std::fs::File::open(&location).unwrap()),
                origin: crate::instructions::Origin::Project,
                baseline: None,
            },
            crate::instructions::Root {
                path,
                dir,
                origin: crate::instructions::Origin::Project,
                baseline: None,
            },
        ];
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let external_path = external
        .path()
        .join("file.txt")
        .to_string_lossy()
        .into_owned();
    let peer = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        assert!(
            !request["input"]
                .to_string()
                .contains("NESTED_INSTRUCTION_BODY")
        );
        let items = [
            ("nested", ".local/opencode/file.txt"),
            ("repeat", ".local/opencode/file.txt"),
            ("direct", ".local/opencode/AGENTS.md"),
            ("sibling", "../sibling/file.txt"),
            ("external", external_path.as_str()),
        ].map(|(id, path)| serde_json::json!({"type":"function_call","id":id,"call_id":id,"name":"read","arguments":serde_json::json!({"path":path,"limit":2}).to_string(),"status":"completed"}));
        let mut script = String::new();
        for item in &items {
            for kind in ["response.output_item.added", "response.output_item.done"] {
                script.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::json!({"type":kind,"item":item})
                ));
            }
        }
        script.push_str(&format!("data: {}\n\n", serde_json::json!({"type":"response.completed","response":{"status":"completed","output":items}})));
        respond(&mut socket, &script).await;
        let (mut socket, request) = read_request(&listener).await;
        let input = request["input"].as_array().unwrap();
        for (body, call) in [
            ("NESTED_INSTRUCTION_BODY", "nested"),
            ("SIBLING_INSTRUCTION_BODY", "sibling"),
        ] {
            let instructions = input
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item["role"] == "developer" && item["content"].to_string().contains(body)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(instructions.len(), 1);
            let output = input
                .iter()
                .position(|item| item["type"] == "function_call_output" && item["call_id"] == call)
                .unwrap();
            assert!(
                instructions[0] > output,
                "instructions must follow their successful read output"
            );
        }
        assert!(!input.iter().any(|item| {
            item["role"] == "developer"
                && (item["content"]
                    .to_string()
                    .contains("EXTERNAL_MUST_NOT_AUTO_LOAD")
                    || item["content"]
                        .to_string()
                        .contains("ANCESTOR_MUST_NOT_AUTO_LOAD"))
        }));
        assert!(input.iter().any(|item| {
            item["call_id"] == "external"
                && item["output"]
                    .as_str()
                    .is_some_and(|output| output.contains("external approved read"))
        }));
        respond(&mut socket, &sse("done")).await;
    });
    let catalog = catalog();
    let cancel = AtomicBool::new(false);
    let mut finished = Vec::new();
    let report = tokio::time::timeout(
        Duration::from_secs(4),
        runtime.run_turn_with_tool_events(
            TurnParams {
                session: "s".into(),
                prompt: "read".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1024,
                provider,
                cancel: &cancel,
            },
            |_| {},
            |_, _| {},
            |_, _| {},
            |_, event| {
                if let ToolCallEvent::Finished {
                    op,
                    state,
                    output_presentation,
                    ..
                } = event
                {
                    assert_eq!(state, "completed");
                    let conn = rusqlite::Connection::open_with_flags(
                        data.path().join("oc.sqlite"),
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
                    )
                    .unwrap();
                    assert_eq!(
                        Db::tool_presentation_in(&conn, "s", op).unwrap(),
                        *output_presentation
                    );
                    finished.push(output_presentation.clone().unwrap());
                }
            },
        ),
    )
    .await
    .unwrap()
    .unwrap();
    peer.await.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(finished.len(), 5);
    assert_eq!(
        finished[0].instructions,
        Some(oc_core::tool_output::InstructionLoads {
            paths: vec![".local/opencode/AGENTS.md".into()],
            total: 1
        })
    );
    assert!(finished[1].instructions.is_none());
    assert!(finished[2].instructions.is_none());
    assert_eq!(
        finished[3].instructions,
        Some(oc_core::tool_output::InstructionLoads {
            paths: vec!["sibling/AGENTS.md".into()],
            total: 1
        })
    );
    assert!(finished[4].instructions.is_none());
    let facts = db.instruction_view("s").unwrap().1;
    assert_eq!(facts.len(), 2);
    assert!(
        facts
            .iter()
            .all(|fact| fact.source.origin == crate::instructions::Origin::Nested)
    );
    assert!(runtime.pending_approvals().is_empty());
    runtime.shutdown_mcp().await.unwrap();
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
        .instruction_sources(
            &runtime.current.read().unwrap(),
            "s",
            &policy,
            &AtomicBool::new(false),
        )
        .unwrap();
    runtime
        .db
        .checkpoint_instructions(
            turn,
            &mut log,
            revision,
            &sources,
            usize::from(revision != 0),
            &[],
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
    runtime.update_execution(|view| {
        view.workspace.instruction_roots = vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: Some(Arc::new(crate::instructions::Source::baseline(
                &source,
                project.path(),
                crate::instructions::Origin::Project,
                "OLD_INSTRUCTION\n",
            ))),
        }];
    });
    let first = seed_instructions(&runtime, "one", &"closed work ".repeat(1000));
    let old = db.instruction_view("s").unwrap().1[0].clone();
    let original = db.turn_result("one").unwrap();
    std::fs::write(&source, "LATEST_INSTRUCTION\n").unwrap();
    runtime.update_execution(|view| {
        view.workspace.instruction_roots = vec![crate::instructions::Root {
            path: project.path().into(),
            dir: Arc::new(std::fs::File::open(project.path()).unwrap()),
            origin: crate::instructions::Origin::Project,
            baseline: Some(Arc::new(crate::instructions::Source::baseline(
                &source,
                project.path(),
                crate::instructions::Origin::Project,
                "LATEST_INSTRUCTION\n",
            ))),
        }];
    });
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
