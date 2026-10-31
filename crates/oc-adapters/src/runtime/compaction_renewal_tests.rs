use super::*;

fn native_report_fixture(db: &Db) -> (String, String) {
    db.create_bound_session("s", "work").unwrap();
    db.apply_dcp_schema().unwrap();
    let first = seed(db, "first", "original genuine task");
    db.create_child_session(
        "s",
        "child",
        Some("helper"),
        Some("fixture/m"),
        Some("work"),
    )
    .unwrap();
    db.record_tool_intent("launch", "s", None, "subagent", "{\"background\":true}")
        .unwrap();
    db.admit_child_job(&oc_core::queries::ChildJob {
        parent: oc_core::domain::SessionId("s".into()),
        child: oc_core::domain::SessionId("child".into()),
        operation: "launch".into(),
        generation: 1,
        location: "work".into(),
        agent: "helper".into(),
        model: "fixture/m".into(),
        description: "independent investigation".into(),
        delivery_id: "delivery".into(),
        state: oc_core::queries::ChildState::Admitted,
        background: true,
        turn: None,
        result: None,
        message_id: None,
    })
    .unwrap();
    db.finish_child_job(
        "launch",
        oc_core::queries::ChildState::Completed,
        "<protect>FULL_REPORT</protect>",
    )
    .unwrap();
    let notice = db
        .deliver_child_notices()
        .unwrap()
        .remove(0)
        .job
        .message_id
        .unwrap();
    (first, notice)
}

#[test]
fn protected_checkpoint_report_is_assigned_only_when_emitted_and_never_duplicated() {
    for native_batch in [false, true] {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        let (_, notice) = native_report_fixture(&db);
        let runtime = runtime(&db, project.path(), Generation::default());
        runtime.update_execution(|view| view.dcp_config.protect_user_messages = true);
        let users_only = runtime.current.read().unwrap().dcp_config.clone();
        runtime.update_execution(|view| view.dcp_config.protect_tags = true);
        let captured = runtime.current.read().unwrap().dcp_config.clone();
        runtime.update_execution(|view| {
            view.dcp_config.protect_user_messages = false;
            view.dcp_config.protect_tags = false;
        });
        let before = runtime.active_projection("s").unwrap();
        // A runtime notice is not user input: only its own <protect> keeps it.
        assert!(
            !runtime
                .selected_message_facts("s", &before.projected, &before.blocks, &users_only)
                .unwrap()
                .iter()
                .any(|fact| fact["protected_message"]["id"] == notice)
        );
        let facts = runtime
            .selected_message_facts("s", &before.projected, &before.blocks, &captured)
            .unwrap();
        runtime.update_execution(|view| view.dcp_config = captured);
        assert!(
            facts
                .iter()
                .any(|fact| fact["protected_message"]["id"] == notice)
        );
        let compact = CompactionSnapshot {
            model: None,
            profile: None,
            anchor: oc_core::compaction::CompactionAnchor::default(),
            id: "checkpoint".into(),
            session: "s".into(),
            reason: CompactionReason::Manual,
            state: CompactionState::Completed,
            summary: "CHECKPOINT_SUMMARY".into(),
            usage: None,
            provider_native: false,
            error: None,
        };
        db.save_compaction(&compact).unwrap();
        db.commit_checkpoint_selected(&compact, &notice, None, &BTreeMap::new(), &facts, None)
            .unwrap();
        let selection = db.checkpoint_selection("s").unwrap();
        let raw = db.read_history_full("s").unwrap();
        let model = ModelRef {
            provider: "fixture".into(),
            id: "m".into(),
            variant: None,
        };
        if native_batch {
            let (ids, task) = db
                .parent_notice_input("s", std::slice::from_ref(&notice))
                .unwrap();
            let mut log = TurnLog::new("native", "m", "fixture");
            log.input.push(task);
            let control = db
                .accept_parent_notice_batch("s", "work", &ids, &model, None, &mut log)
                .unwrap()
                .unwrap()
                .user_message;
            let active = runtime.active_projection("s").unwrap();
            let prior = active
                .projected
                .iter()
                .filter(|row| row.0 != control && row.0 != notice)
                .cloned()
                .collect::<Vec<_>>();
            let (mut wire, represented) = runtime
                .wire_history_with_child_notices(
                    "s",
                    &prior,
                    &active.blocks,
                    "m",
                    "fixture",
                    None,
                    active.after_seq,
                    None,
                    |id| ids.iter().any(|notice| notice == id),
                )
                .unwrap();
            assert!(!represented.contains(&notice));
            wire.extend(log.input.clone());
            assert_eq!(
                serde_json::to_string(&wire)
                    .unwrap()
                    .matches("FULL_REPORT")
                    .count(),
                1
            );
            log.input
                .push(InputItem::message(InputRole::Assistant, "automatic answer"));
            db.commit_turn(
                "native",
                "completed",
                Some(&log.to_json().to_string()),
                Some("automatic answer"),
            )
            .unwrap();
            let active = runtime.active_projection("s").unwrap();
            let future = runtime
                .wire_history(
                    "s",
                    &active.projected,
                    &active.blocks,
                    "m",
                    "fixture",
                    None,
                    active.after_seq,
                )
                .unwrap();
            assert_eq!(
                serde_json::to_string(&future)
                    .unwrap()
                    .matches("FULL_REPORT")
                    .count(),
                1
            );
        } else {
            let active = runtime.active_projection("s").unwrap();
            let (wire, represented) = runtime
                .wire_history_with_child_notices(
                    "s",
                    &active.projected,
                    &active.blocks,
                    "m",
                    "fixture",
                    None,
                    active.after_seq,
                    None,
                    |_| false,
                )
                .unwrap();
            assert_eq!(
                serde_json::to_string(&wire)
                    .unwrap()
                    .matches("FULL_REPORT")
                    .count(),
                1
            );
            assert_eq!(represented.as_slice(), std::slice::from_ref(&notice));
            assert!(!db.parent_notice_assigned("s", &notice).unwrap());
            let mut log = TurnLog::new("ordinary", "m", "fixture");
            db.accept_turn_with_selection(
                "ordinary",
                "s",
                "genuine followup",
                "genuine followup",
                &model,
                None,
                None,
                None,
                Some(&mut log),
            )
            .unwrap();
            db.checkpoint_turn_with_child_notices(
                "ordinary",
                "s",
                &log.to_json().to_string(),
                &represented,
            )
            .unwrap();
            assert!(db.parent_notice_assigned("s", &notice).unwrap());
        }
        assert_eq!(db.checkpoint_selection("s").unwrap(), selection);
        assert_eq!(&db.read_history_full("s").unwrap()[..raw.len()], raw);
    }
}

#[test]
fn native_followup_after_old_notice_compression_keeps_its_new_tool_journal() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let (first, notice) = native_report_fixture(&db);
    seed(
        &db,
        "tail",
        "a later genuine task that did not prepare this report",
    );
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("compress".into(), Permission::Allow);
    let runtime = runtime(&db, project.path(), generation);
    runtime
        .run_compress(
            "s",
            &serde_json::json!({"topic":"old work","content":[{
                "startId":first,"endId":notice,"summary":"OLD_STANDALONE_SUMMARY"
            }]}),
            &oc_core::context_plan::ProtectedSpec::default(),
        )
        .unwrap();
    let graph = db.active_compression_graph("s", 0).unwrap();
    assert!(graph[0].hot.as_ref().unwrap()["standalone"] == true);
    let raw_before = db.read_history_full("s").unwrap();
    let (ids, task) = db
        .parent_notice_input("s", std::slice::from_ref(&notice))
        .unwrap();
    let mut native = TurnLog::new("native", "m", "fixture");
    native.input.push(task);
    let control = db
        .accept_parent_notice_batch(
            "s",
            "work",
            &ids,
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
            None,
            &mut native,
        )
        .unwrap()
        .unwrap()
        .user_message;
    assert_ne!(control, notice);
    native.input.extend([
        InputItem::ProviderOutput(serde_json::json!({"type":"function_call","call_id":"new-tool","name":"read","arguments":"{}"})),
        InputItem::FunctionCallOutput { call_id: "new-tool".into(), output: "NEW_NATIVE_TOOL_RESULT".into() },
        InputItem::message(InputRole::Assistant, "automatic answer"),
    ]);
    db.commit_turn(
        "native",
        "completed",
        Some(&native.to_json().to_string()),
        Some("automatic answer"),
    )
    .unwrap();
    let active = runtime.active_projection("s").unwrap();
    let wire = runtime
        .wire_history(
            "s",
            &active.projected,
            &active.blocks,
            "m",
            "fixture",
            None,
            active.after_seq,
        )
        .unwrap();
    let wire = serde_json::to_string(&wire).unwrap();
    assert!(wire.contains("OLD_STANDALONE_SUMMARY"), "{wire}");
    assert!(wire.contains("NEW_NATIVE_TOOL_RESULT"), "{wire}");
    assert_eq!(wire.matches("FULL_REPORT").count(), 1, "{wire}");
    assert_eq!(
        &db.read_history_full("s").unwrap()[..raw_before.len()],
        raw_before
    );
    assert_eq!(
        db.active_compression_graph("s", 0).unwrap()[0].summary,
        graph[0].summary
    );
}

#[test]
fn repeated_standalone_renewal_releases_old_facts_and_preserves_literal_fork() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.apply_dcp_schema().unwrap();
    let control = "CONTROL_OBJECTIVE <protect>literal (b9000)</protect>";
    let first = db
        .accept_turn(
            "first",
            "s",
            control,
            control,
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap()
        .user_message;
    let mut log = TurnLog::new("first", "m", "fixture");
    log.user_message = Some(first.clone());
    let investigation = "OLD_INVESTIGATION ".repeat(300);
    log.input = vec![
        InputItem::message(InputRole::User, control),
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","call_id":"old-projection","name":"compress","arguments":"{\"summary\":\"OLD_SELF_SUMMARY\"}"}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "old-projection".into(),
            output: "closed old projection".into(),
        },
        InputItem::message(InputRole::Assistant, &investigation),
    ];
    db.commit_turn(
        "first",
        "completed",
        Some(&log.to_json().to_string()),
        Some(&investigation),
    )
    .unwrap();
    let mut end = db.read_history_full("s").unwrap().last().unwrap().0.clone();
    seed(
        &db,
        "tail",
        "CONTROL_OBJECTIVE changed requirement, path and next move",
    );
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("compress".into(), Permission::Allow);
    let runtime = runtime(&db, project.path(), generation);
    runtime.update_execution(|view| view.dcp_config.protect_tags = true);
    let mut start = first;
    for cycle in 0..12 {
        let protected = cycle < 11;
        runtime.update_execution(|view| view.dcp_config.protect_tags = protected);
        let report=runtime.run_compress("s",&serde_json::json!({"topic":"renewal","content":[{"startId":start,"endId":end,"summary":format!("CONTROL_OBJECTIVE changed requirement path selected-{cycle} next move")}]}),&oc_core::context_plan::ProtectedSpec{protect_tags:protected,..Default::default()}).unwrap();
        start = report.blocks[0].clone();
        let active = runtime.active_projection("s").unwrap();
        assert_eq!(active.blocks.len(), 1);
        assert!(active.blocks[0].hot.as_ref().unwrap()["standalone"] == true);
        if cycle == 0 {
            let wire = runtime
                .wire_history(
                    "s",
                    &active.projected,
                    &active.blocks,
                    "m",
                    "fixture",
                    None,
                    active.after_seq,
                )
                .unwrap();
            assert!(
                !serde_json::to_string(&wire)
                    .unwrap()
                    .contains("OLD_SELF_SUMMARY"),
                "closed projection-only groups cannot pin obsolete authored summaries"
            );
            assert!(
                db.turn_result("first")
                    .unwrap()
                    .1
                    .unwrap()
                    .contains("OLD_SELF_SUMMARY")
            );
            let cutoff = seed(&db, "fork-boundary", "CONTROL_OBJECTIVE explicit fork cut");
            let fork = db
                .fork_session("s", &cutoff, "work", "fixture", "{}")
                .unwrap();
            let graph = db.active_compression_graph(&fork.session.0, 0).unwrap();
            assert!(graph[0].summary.contains("literal (b9000)"));
        }
        if cycle == 11 {
            let wire = runtime
                .wire_history(
                    "s",
                    &active.projected,
                    &active.blocks,
                    "m",
                    "fixture",
                    None,
                    active.after_seq,
                )
                .unwrap();
            let wire = serde_json::to_string(&wire).unwrap();
            assert!(!wire.contains("literal (b9000)"));
            assert!(!wire.contains("OLD_INVESTIGATION"));
            assert!(wire.contains("selected-11"));
        } else {
            seed(
                &db,
                &format!("fresh-{cycle}"),
                &"fresh closed work ".repeat(300),
            );
            end = db.read_history_full("s").unwrap().last().unwrap().0.clone();
            seed(&db, &format!("tail-{cycle}"), "CONTROL_OBJECTIVE next tail");
        }
    }
    let raw = db.read_history_full("s").unwrap();
    assert!(raw.iter().any(|r| r.2.contains("OLD_INVESTIGATION")));
    assert_eq!(db.dcp_block_count("s").unwrap(), 1);
    drop(runtime);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    assert_eq!(db.dcp_block_count("s").unwrap(), 1);
    assert_eq!(db.read_history_full("s").unwrap(), raw);
}

#[tokio::test]
async fn compact_forget_selection_does_not_reassign_a_reused_call_mark() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    db.apply_dcp_schema().unwrap();
    let mut logs = Vec::new();
    for turn in ["old", "tail"] {
        let prompt = if turn == "old" {
            "old LEGACY_VERBATIM"
        } else {
            turn
        };
        let user = db
            .accept_turn(
                turn,
                "s",
                prompt,
                prompt,
                &ModelRef {
                    provider: "fixture".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap()
            .user_message;
        let mut log = TurnLog::new(turn, "m", "fixture");
        log.user_message = Some(user.clone());
        log.input = vec![
            InputItem::message(InputRole::User, prompt),
            InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","call_id":"reused","name":"bash","arguments":"{}"}),
            ),
            InputItem::FunctionCallOutput {
                call_id: "reused".into(),
                output: format!("{turn} payload"),
            },
        ];
        db.commit_turn(
            turn,
            "completed",
            Some(&log.to_json().to_string()),
            Some("settled"),
        )
        .unwrap();
        logs.push(log);
    }
    let first = logs[0].user_message.as_ref().unwrap();
    let block = db
        .save_compression_block(
            "s",
            "old",
            "chosen facts",
            first,
            first,
            std::slice::from_ref(first),
        )
        .unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("UPDATE compression_blocks SET hot=?2 WHERE id=?1",rusqlite::params![block,serde_json::json!({"version":1,"active":true,"standalone":true,"protected":[],"legacy_protected":[{"role":"user","text":"old LEGACY_VERBATIM","source_start":first,"source_end":first}],"logs":[logs[0].to_json()]}).to_string()]).unwrap();
    conn.execute_batch("INSERT INTO dcp_tool_projection_v2(session_id,call_id,occurrence,action) VALUES('s','reused',0,'hidden'),('s','reused',1,'purged');").unwrap();
    conn.execute_batch("INSERT INTO dcp_tool_projection_v2(session_id,call_id,occurrence,action) VALUES('s','inactive-archive',0,'hidden'); CREATE TRIGGER preserve_inactive_mark BEFORE DELETE ON dcp_tool_projection_v2 WHEN OLD.call_id='inactive-archive' BEGIN SELECT RAISE(ABORT,'unrelated inactive mark touched'); END;").unwrap();
    let mut generation = Generation::default();
    generation.compaction.keep_tokens = 0;
    let runtime = runtime(&db, project.path(), generation);
    runtime.update_execution(|view| view.dcp_config.protect_user_messages = true);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let responder = tokio::spawn(async move {
        let (mut socket, _) = read_request(&listener).await;
        respond(&mut socket, &sse("## Objective\nContinue selected facts")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    let result = runtime
        .deliver_compaction("s", &catalog(), "m", None, &provider)
        .await;
    if matches!(result, Ok(true)) {
        responder.await.unwrap();
    } else {
        responder.abort();
        let _ = responder.await;
    }
    assert!(result.unwrap());
    let projection = db.load_dcp_tool_projection("s").unwrap();
    assert!(
        projection.purged.contains(&("reused".into(), 0)),
        "surviving producer keeps its original decision"
    );
    assert!(
        !projection.hidden.contains(&("reused".into(), 0)),
        "forgotten producer's decision cannot migrate"
    );
    let raw = db.turn_result("old").unwrap();
    let selected = db.checkpoint_selection("s").unwrap().unwrap();
    assert!(
        selected
            .iter()
            .any(|v| v["legacy_protected"][0]["text"] == "old LEGACY_VERBATIM")
    );
    let cutoff = seed(&db, "legacy-fork-boundary", "current task");
    let fork = db
        .fork_session("s", &cutoff, "work", "fixture", "{}")
        .unwrap();
    let selected = db.checkpoint_selection(&fork.session.0).unwrap().unwrap();
    assert_eq!(
        db.get_pref(&format!("dcp.projection_owned.{}", fork.session.0))
            .unwrap()
            .as_deref(),
        Some("true"),
        "fork carries the actual owned projection frame"
    );
    let fact = selected
        .iter()
        .find_map(|v| v["legacy_protected"].as_array())
        .unwrap();
    assert_ne!(fact[0]["source_start"].as_str().unwrap(), first);
    runtime.update_execution(|view| view.dcp_config.protect_user_messages = false);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let responder = tokio::spawn(async move {
        let (mut socket, _) = read_request(&listener).await;
        respond(&mut socket, &sse("## Objective\nNew selected facts")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    let result = runtime
        .deliver_compaction("s", &catalog(), "m", None, &provider)
        .await;
    if matches!(result, Ok(true)) {
        responder.await.unwrap();
    } else {
        responder.abort();
        let _ = responder.await;
    }
    assert!(result.unwrap());
    let active = runtime.active_projection("s").unwrap();
    let wire = runtime
        .wire_history(
            "s",
            &active.projected,
            &active.blocks,
            "m",
            "fixture",
            None,
            active.after_seq,
        )
        .unwrap();
    assert!(
        !serde_json::to_string(&wire)
            .unwrap()
            .contains("LEGACY_VERBATIM")
    );
    assert_eq!(db.turn_result("old").unwrap(), raw);
    let user = db
        .accept_turn(
            "fresh-producer",
            "s",
            "fresh",
            "fresh",
            &ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap()
        .user_message;
    let mut fresh = TurnLog::new("fresh-producer", "m", "fixture");
    fresh.user_message = Some(user);
    fresh.input = vec![
        InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","call_id":"inactive-archive","name":"bash","arguments":"{}"}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "inactive-archive".into(),
            output: "fresh known result".into(),
        },
    ];
    db.commit_turn(
        "fresh-producer",
        "completed",
        Some(&fresh.to_json().to_string()),
        Some("known"),
    )
    .unwrap();
    assert!(
        !db.dcp_tool_projection_for_input("s", &fresh.input)
            .unwrap()
            .hidden
            .contains(&("inactive-archive".into(), 0)),
        "an unused legacy archive slot cannot attach to a fresh producer"
    );
}

#[tokio::test]
async fn compaction_admits_replay_instead_of_archive_and_preserves_protected_prefix() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    let requirement = "<protect>KEEP_EXACT_REQUIREMENT</protect>";
    let model = ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let user = db
        .accept_turn("archived", "s", requirement, requirement, &model)
        .unwrap()
        .user_message;
    let mut log = TurnLog::new("archived", "m", "fixture");
    log.user_message = Some(user);
    log.input = vec![
        InputItem::message(InputRole::User, requirement),
        InputItem::message(InputRole::Assistant, "settled"),
    ];
    log.opaque.push(serde_json::json!({"type":"reasoning","id":"archived-reasoning","summary":[],"encrypted_content":"ARCHIVE_ONLY".repeat(8192)}));
    let raw = log.to_json().to_string();
    db.commit_turn("archived", "completed", Some(&raw), Some("settled"))
        .unwrap();
    let raw = db.turn_result("archived").unwrap().1.unwrap();
    seed(&db, "tail", "CONTROL_OBJECTIVE current task");
    let mut generation = Generation::default();
    generation.compaction.keep_tokens = 0;
    generation.compaction.auto = false;
    let runtime = runtime(&db, project.path(), generation);
    runtime.update_execution(|view| {
        view.dcp_config.protect_tags = true;
        view.dcp_config.protected_tools = vec!["never_called".into()];
    });
    let mut models = catalog();
    models.models.insert(
        "m".into(),
        serde_json::json!({"limit":{"context":16384,"input":12000,"output":1024}}),
    );
    let cutoff = db.compaction_boundary("s", 0).unwrap().unwrap().0;
    assert!(
        db.compaction_content_floor("s", 0, cutoff, 40000).unwrap() > 0,
        "old archive-derived model floor rejects this history"
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let responder = tokio::spawn(async move {
        let (mut socket, summary) = read_request(&listener).await;
        assert!(summary["tools"].as_array().unwrap().is_empty());
        let input = summary["input"].to_string();
        assert!(input.contains("KEEP_EXACT_REQUIREMENT"));
        assert!(!input.contains("ARCHIVE_ONLY"));
        respond(&mut socket, &sse("## Objective\nContinue selected work.")).await;
        let (mut socket, primary) = read_request(&listener).await;
        let input = primary["input"].to_string();
        assert!(
            input.contains("KEEP_EXACT_REQUIREMENT") && input.contains("Continue selected work.")
        );
        assert!(input.contains("CONTROL_OBJECTIVE"));
        assert!(!input.contains("ARCHIVE_ONLY"));
        respond(&mut socket, &sse("continued")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    let compacted = runtime
        .deliver_compaction("s", &models, "m", None, &provider)
        .await;
    if !matches!(compacted, Ok(true)) {
        responder.abort();
        let _ = responder.await;
        panic!("admitted replay must compact: {compacted:?}");
    }
    let report = runtime
        .run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue".into(),
            invocation: None,
            catalog: &models,
            model_id: "m".into(),
            variant: None,
            max_output: 1024,
            provider,
            cancel: &AtomicBool::new(false),
        })
        .await
        .unwrap();
    responder.await.unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        db.turn_result("archived").unwrap().1.as_deref(),
        Some(raw.as_str())
    );
    let selected = db.checkpoint_selection("s").unwrap().unwrap();
    assert!(
        selected
            .iter()
            .any(|fact| fact["protected_message"]["text"] == requirement)
    );
    assert!(db.list_tool_ops("s").unwrap().is_empty());
}

#[tokio::test]
async fn compaction_final_frame_overflow_preserves_protected_history_without_dispatch() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(
        &db,
        "protected",
        &format!("<protect>{}</protect>", "\\\"".repeat(2000)),
    );
    seed(&db, "tail", "current task");
    let before = db.read_history_full("s").unwrap();
    let mut generation = Generation::default();
    generation.compaction.keep_tokens = 0;
    let runtime = runtime(&db, project.path(), generation);
    runtime.update_execution(|view| view.dcp_config.protect_tags = true);
    let mut models = catalog();
    models.models.insert(
        "m".into(),
        serde_json::json!({"limit":{"context":16384,"input":9000,"output":1024}}),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    assert!(
        !runtime
            .deliver_compaction("s", &models, "m", None, &provider)
            .await
            .unwrap()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    assert_eq!(db.read_history_full("s").unwrap(), before);
    assert!(db.session_checkpoint("s").unwrap().is_none());
    let snapshot = db.compaction_history("s").unwrap().remove(0);
    assert_eq!(snapshot.state, CompactionState::Failed);
    assert!(!snapshot.error.unwrap().contains("irreducible"));
}

#[tokio::test]
async fn manual_compact_reachable_after_host_admission_overflow_without_huge_before() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(
        &db,
        "old",
        &"OBSOLETE_HUGE ".repeat(ACTIVE_CONTEXT_BYTES_CAP / 12 + 1),
    );
    seed(&db, "tail", "CONTROL_OBJECTIVE current task");
    let mut generation = Generation::default();
    generation.compaction.keep_tokens = 0;
    let runtime = runtime(&db, project.path(), generation);
    assert!(matches!(
        runtime.active_projection("s"),
        Err(RuntimeError::ContextOverflow { .. })
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let responder = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        assert!(request["input"].to_string().len() < 32_000);
        assert!(!request["input"].to_string().contains("OBSOLETE_HUGE"));
        respond(&mut socket, &sse("## Objective\nContinue selected work.")).await;
    });
    runtime
        .queue_compaction("s", CompactionReason::Manual)
        .unwrap();
    let result = runtime
        .deliver_compaction("s", &catalog(), "m", None, &provider)
        .await;
    if !matches!(result, Ok(true)) {
        responder.abort();
        let _ = responder.await;
    } else {
        responder.await.unwrap();
    }
    assert!(
        result.unwrap(),
        "manual escape must select metadata before overflowing content"
    );
    let active = runtime.active_projection("s").unwrap();
    assert!(
        active
            .projected
            .iter()
            .any(|r| r.2.contains("CONTROL_OBJECTIVE"))
    );
    assert!(
        !active
            .projected
            .iter()
            .any(|r| r.2.contains("OBSOLETE_HUGE"))
    );
}
