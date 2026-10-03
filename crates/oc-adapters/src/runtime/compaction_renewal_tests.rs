use super::*;

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
    runtime.dcp_config.write().unwrap().protect_tags = true;
    let mut start = first;
    for cycle in 0..12 {
        let protected = cycle < 11;
        runtime.dcp_config.write().unwrap().protect_tags = protected;
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
    runtime.dcp_config.write().unwrap().protect_user_messages = true;
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
    runtime.dcp_config.write().unwrap().protect_user_messages = false;
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
