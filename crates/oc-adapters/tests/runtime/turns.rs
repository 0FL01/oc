//! Provider turns, reasoning/usage, title and fresh-session acceptance.

use super::*;

#[tokio::test]
async fn fresh_turn_commits_root_binding_selection_before_ack_and_streams_normally() {
    let mut permissions = allow_all();
    permissions.insert("read".into(), Permission::Deny);
    let (harness, generation) = make_harness(permissions);
    let runtime = runtime_of(&harness, generation, Vec::new());
    let selection_key = "tui.selection.session:[\"/project\",\"test\",\"fresh\"]";
    let selection = r#"{"agent":null,"models":{"":{"id":"m","variant":null}},"epoch":0}"#;
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_tool_call("denied-read", "read", &serde_json::json!({"path":"secret"}))
                + &sse_completed(),
            sse_delta("answer") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut acknowledgements = Vec::new();
    let mut deltas = Vec::new();
    let callback_order = Arc::new(Mutex::new(Vec::new()));
    let accepted_order = callback_order.clone();
    let tool_order = callback_order.clone();
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "fresh",
                "expanded prompt",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            Some((selection_key, selection)),
            |turn| {
                let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
                let (status, prompt): (String, String) = conn
                    .query_row(
                        "SELECT status, prompt FROM turns WHERE id = ?1",
                        [turn],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(
                    (status.as_str(), prompt.as_str()),
                    ("started", "expanded prompt")
                );
                assert_eq!(
                    harness.db.get_pref(selection_key).unwrap().as_deref(),
                    Some(selection)
                );
                runtime.open_session("fresh").unwrap();
                assert_eq!(
                    harness.db.read_history("fresh").unwrap(),
                    [("user".into(), "expanded prompt".into())]
                );
                accepted_order.lock().unwrap().push("accepted");
                acknowledgements.push(turn.to_string());
            },
            |turn, text| deltas.push((turn.to_string(), text.to_string())),
            |_, _| {},
            |_, event| {
                if matches!(event, ToolCallEvent::ArgumentStream(_)) {
                    return;
                }
                tool_order.lock().unwrap().push(match event {
                    ToolCallEvent::Started { .. } => "started",
                    ToolCallEvent::Finished { .. } => "finished",
                    ToolCallEvent::ArgumentStream(_) => "argument_stream",
                });
            },
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "answer");
    assert_eq!(
        acknowledgements.as_slice(),
        std::slice::from_ref(&report.turn_id)
    );
    assert_eq!(deltas, [(report.turn_id.clone(), "answer".into())]);
    assert_eq!(report.calls[0].output, "error: denied read");
    assert_eq!(*callback_order.lock().unwrap(), ["accepted", "finished"]);
    assert_eq!(*hits.lock().unwrap(), 2);
    assert_eq!(requests.lock().unwrap()[0]["model"], "m");
    assert_eq!(
        function_output(&requests.lock().unwrap()[1], "denied-read"),
        Some("error: denied read")
    );
    assert_eq!(harness.db.session_meta("fresh").unwrap().parent_id, None);
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 2);
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let events: Vec<String> = conn
        .prepare("SELECT kind FROM events WHERE session_id='fresh' ORDER BY rowid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        events,
        [
            "session_created",
            "turn_started",
            "accepted_model",
            "message",
            "message",
            "turn_finished"
        ]
    );

    // The old path can continue the root, while fresh-only admission cannot
    // claim it or rewrite its selection/history.
    let duplicate = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "fresh",
                "duplicate",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            None,
            |_| panic!("duplicate accepted"),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert!(matches!(duplicate, Err(RuntimeError::InvalidArgs(_))));
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 2);
    assert_eq!(
        runtime
            .run_turn(params(
                "fresh",
                "next",
                &harness,
                provider_of(&base),
                &NO_CANCEL
            ))
            .await
            .unwrap()
            .status,
        TurnStatus::Completed
    );
    assert_eq!(harness.db.read_history("fresh").unwrap().len(), 4);
}

#[tokio::test]
async fn cancelled_fresh_turn_keeps_the_durable_acceptance_receipt() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let cancelled = AtomicBool::new(true);
    let (base, hits) = Fake::start(
        vec![sse_delta("unexpected") + &sse_completed()],
        Duration::ZERO,
    );
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "cancelled-fresh",
                "input",
                &harness,
                provider_of(&base),
                &cancelled,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| panic!("cancelled turn streamed text"),
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    assert_eq!(report.status, TurnStatus::Cancelled);
    assert_eq!(*hits.lock().unwrap(), 0);
    runtime.open_session("cancelled-fresh").unwrap();
    assert_eq!(
        harness.db.read_history("cancelled-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
}

#[tokio::test]
async fn post_accept_failure_still_delivers_fresh_root_and_turn_receipt() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_checkpoint BEFORE UPDATE ON turns
         WHEN NEW.session_id = 'checkpoint-fresh'
         BEGIN SELECT RAISE(ABORT, 'injected checkpoint failure'); END;",
    )
    .unwrap();
    let (base, hits) = Fake::start(vec![sse_delta("never") + &sse_completed()], Duration::ZERO);
    let mut accepted = None;
    let result = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "checkpoint-fresh",
                "input",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert_eq!(result.unwrap_err(), RuntimeError::Storage);
    let turn = accepted.expect("committed root and turn must have a receipt");
    runtime.open_session("checkpoint-fresh").unwrap();
    let state: String = conn
        .query_row("SELECT status FROM turns WHERE id = ?1", [turn], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(state, "started");
    assert_eq!(
        harness.db.read_history("checkpoint-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
    assert_eq!(*hits.lock().unwrap(), 0);
}

#[tokio::test]
async fn fresh_provider_failure_after_accept_retains_root_and_reports_failed_turn() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params(
                "provider-fresh",
                "input",
                &harness,
                provider_of("http://127.0.0.1:9/v1"),
                &NO_CANCEL,
            ),
            None,
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Failed);
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    runtime.open_session("provider-fresh").unwrap();
    assert_eq!(
        harness.db.read_history("provider-fresh").unwrap(),
        [("user".into(), "input".into())]
    );
}

#[tokio::test]
async fn rejected_fresh_turn_leaves_no_root_or_selection_and_can_retry() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    let key = "tui.selection.session:[\"/project\",\"test\",\"retry\"]";
    let wrong = "tui.selection.session:[\"/project\",\"test\",\"other\"]";
    let (base, hits) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let conn = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    let runtime_ref = &runtime;
    let rejected = |params, selection| async move {
        runtime_ref
            .run_fresh_turn_with_tool_events(
                params,
                selection,
                |_| panic!("rejected fresh turn acknowledged"),
                |_, _| {},
                |_, _| {},
                |_, _| {},
            )
            .await
    };
    let mut invalid_model = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    invalid_model.model_id = "absent".into();
    assert!(matches!(
        rejected(invalid_model, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    let mut invalid_variant = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    invalid_variant.variant = Some("absent".into());
    assert!(matches!(
        rejected(invalid_variant, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    let mut tiny_catalog = harness.catalog.clone();
    tiny_catalog.models.insert(
        "m".into(),
        serde_json::json!({"limit":{"context":1,"output":1}}),
    );
    let mut over_budget = params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL);
    over_budget.catalog = &tiny_catalog;
    assert!(matches!(
        rejected(over_budget, Some((key, "choice"))).await,
        Err(RuntimeError::InvalidArgs(_))
    ));
    assert_eq!(
        rejected(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((wrong, "choice"))
        )
        .await
        .unwrap_err(),
        RuntimeError::Storage
    );

    conn.execute_batch("CREATE TRIGGER fail_fresh_input BEFORE INSERT ON messages WHEN NEW.session_id = 'retry' BEGIN SELECT RAISE(ABORT, 'injected input failure'); END;").unwrap();
    assert_eq!(
        rejected(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((key, "choice"))
        )
        .await
        .unwrap_err(),
        RuntimeError::Storage
    );
    for table in ["sessions", "turns", "messages", "events"] {
        let query = format!("SELECT COUNT(*) FROM {table} WHERE session_id = 'retry'");
        let query = if table == "sessions" {
            "SELECT COUNT(*) FROM sessions WHERE id = 'retry'"
        } else {
            &query
        };
        let count: i64 = conn.query_row(query, [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "{table} left after refusal");
    }
    assert_eq!(harness.db.get_pref(key).unwrap(), None);
    assert_eq!(
        harness
            .db
            .get_pref(&format!("{SESSION_LOCATION_PREFIX}retry"))
            .unwrap(),
        None
    );
    assert_eq!(*hits.lock().unwrap(), 0);

    conn.execute_batch("DROP TRIGGER fail_fresh_input").unwrap();
    let mut accepted = None;
    let report = runtime
        .run_fresh_turn_with_tool_events(
            params("retry", "prompt", &harness, provider_of(&base), &NO_CANCEL),
            Some((key, "choice")),
            |turn| accepted = Some(turn.to_string()),
            |_, _| {},
            |_, _| {},
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(accepted.as_deref(), Some(report.turn_id.as_str()));
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(harness.db.get_pref(key).unwrap().as_deref(), Some("choice"));
    assert_eq!(*hits.lock().unwrap(), 1);
}

#[test]
fn root_location_creation_rolls_back_on_pref_failure_and_retries() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    let conn = rusqlite::Connection::open(harness._data.path().join("oc.sqlite")).unwrap();
    let key = format!("{SESSION_LOCATION_PREFIX}atomic-root");
    conn.execute_batch(
        "CREATE TRIGGER fail_location_binding BEFORE INSERT ON prefs
         WHEN NEW.key = 'tui.session_location.atomic-root'
         BEGIN SELECT RAISE(ABORT, 'injected preference failure'); END;",
    )
    .unwrap();

    assert!(matches!(
        runtime.create_session("atomic-root"),
        Err(RuntimeError::Storage)
    ));
    let count =
        |sql: &str, value: &str| -> i64 { conn.query_row(sql, [value], |row| row.get(0)).unwrap() };
    let sessions = "SELECT count(*) FROM sessions WHERE id = ?1";
    let events = "SELECT count(*) FROM events WHERE session_id = ?1 AND kind = 'session_created'";
    let prefs = "SELECT count(*) FROM prefs WHERE key = ?1";
    assert_eq!(
        count(sessions, "atomic-root"),
        0,
        "failed binding stranded a root"
    );
    assert_eq!(
        count(events, "atomic-root"),
        0,
        "failed binding stranded an event"
    );
    assert_eq!(count(prefs, &key), 0);

    conn.execute_batch("DROP TRIGGER fail_location_binding;")
        .unwrap();
    runtime.create_session("atomic-root").expect("retry");
    runtime
        .create_session("atomic-root")
        .expect("same Location idempotent");
    runtime.open_session("atomic-root").expect("bound root");
    assert_eq!(count(sessions, "atomic-root"), 1);
    assert_eq!(count(events, "atomic-root"), 1);
    assert_eq!(count(prefs, &key), 1);

    let project = harness._project.path();
    let other = Runtime::new(
        &harness.db,
        "elsewhere",
        generation,
        ProtectedGlobs { patterns: vec![] },
        oc_adapters::files::Files::new(project, harness._data.path()).unwrap(),
        oc_adapters::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .unwrap();
    assert_eq!(
        other.create_session("atomic-root"),
        Err(RuntimeError::LocationMismatch {
            session: "atomic-root".into(),
            location: "work".into(),
        })
    );
    assert_eq!(count(sessions, "atomic-root"), 1);
    assert_eq!(count(events, "atomic-root"), 1);
    assert_eq!(count(prefs, &key), 1);

    harness.db.create_session("standalone").unwrap();
    assert_eq!(
        runtime.create_session("standalone"),
        Err(RuntimeError::Storage)
    );
    assert_eq!(count(sessions, "standalone"), 1);
    assert_eq!(count(events, "standalone"), 1);
    assert_eq!(
        count(prefs, &format!("{SESSION_LOCATION_PREFIX}standalone")),
        0
    );
}

#[tokio::test]
async fn accepted_model_switch_is_public_only_and_does_not_add_provider_requests() {
    let (mut harness, generation) = make_harness(allow_all());
    harness.catalog.models.get_mut("m").unwrap()["variants"] = serde_json::json!({"default": {}});
    harness.catalog.models.insert(
        "next".into(),
        serde_json::json!({
            "limit": {"context": 1_000_000, "output": 100_000},
            "variants": {"high": {"reasoningEffort": "high"}}
        }),
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let (base, hits, requests) =
        Fake::start_recording(vec![sse_delta("answer") + &sse_completed()], Duration::ZERO);
    let first = params("s", "first", &harness, provider_of(&base), &NO_CANCEL);
    assert_eq!(
        runtime.run_turn(first).await.unwrap().status,
        TurnStatus::Completed
    );
    let mut same = params(
        "s",
        "still default",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    same.variant = Some("default".into());
    assert_eq!(
        runtime.run_turn(same).await.unwrap().status,
        TurnStatus::Completed
    );
    let mut next = params("s", "second", &harness, provider_of(&base), &NO_CANCEL);
    next.model_id = "next".into();
    next.variant = Some("high".into());
    assert_eq!(
        runtime.run_turn(next).await.unwrap().status,
        TurnStatus::Completed
    );
    assert_eq!(*hits.lock().unwrap(), 3);
    let wire = requests.lock().unwrap();
    assert_eq!(wire.len(), 3);
    assert_eq!(wire[2]["model"], "next");
    let outbound = wire[2].to_string();
    assert!(outbound.contains("first"));
    assert!(!outbound.contains("model_switch"));
    assert!(!outbound.contains("Switched model"));
    assert!(!outbound.contains("Switched variant"));
    drop(wire);
    let page = harness.db.read_history_page("s", 10, None).unwrap();
    assert_eq!(
        page.iter()
            .filter(|(_, role, _)| role == "model_switch")
            .count(),
        1
    );
    assert_eq!(
        page.iter()
            .map(|(_, role, _)| role.as_str())
            .collect::<Vec<_>>(),
        [
            "assistant",
            "user",
            "model_switch",
            "assistant",
            "user",
            "assistant",
            "user"
        ]
    );
    assert_eq!(harness.db.read_history_full("s").unwrap().len(), 6);
    assert_eq!(
        harness
            .db
            .active_history("s", 0, 100_000)
            .unwrap()
            .rows
            .len(),
        6
    );
}

#[tokio::test]
async fn aud07_rejected_input_has_no_turn_or_event() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_input BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'injected input failure'); END;").unwrap();
    let result = runtime
        .run_turn_with_events(
            params(
                "s",
                "input",
                &harness,
                provider_of("http://127.0.0.1:9"),
                &NO_CANCEL,
            ),
            |_| panic!("rejected input acknowledged"),
            |_, _| {},
            |_, _| {},
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    let turns: i64 = sql
        .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0, "unaccepted input left a started turn");
    let events: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind != 'session_created'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(events, 0);
    assert!(harness.db.read_history("s").unwrap().is_empty());
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert!(reopened.read_history("s").unwrap().is_empty());
}

#[tokio::test]
async fn aud07_terminal_failure_does_not_commit_assistant() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").unwrap();
    let sql = rusqlite::Connection::open(harness.db.root().join("oc.sqlite")).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_terminal BEFORE UPDATE ON turns WHEN NEW.status = 'completed' BEGIN SELECT RAISE(ABORT, 'injected terminal failure'); END;").unwrap();
    let (base, _) = Fake::start(
        vec![sse_delta("must not commit") + &sse_completed()],
        Duration::ZERO,
    );
    let result = runtime
        .run_turn(params(
            "s",
            "input",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await;
    assert_eq!(
        result.unwrap_err(),
        oc_adapters::runtime::RuntimeError::Storage
    );
    assert_eq!(
        harness.db.read_history("s").unwrap(),
        [("user".to_string(), "input".to_string())]
    );
    let terminal: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind = 'turn_finished'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(terminal, 0);
    let (status, checkpoint): (String, Option<String>) = sql
        .query_row("SELECT status, result FROM turns", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(status, "started");
    assert!(
        checkpoint.is_some(),
        "generation checkpoint must succeed before the terminal commit fails"
    );
    drop(runtime);
    drop(harness.db);
    let reopened = Db::open(harness._data.path()).unwrap();
    assert_eq!(
        reopened.read_history("s").unwrap(),
        [("user".to_string(), "input".to_string())]
    );
}

#[tokio::test]
async fn text_turn_completes_and_drains() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let (base, _) = Fake::start(vec![sse_delta("hi") + &sse_completed()], Duration::ZERO);

    let report = runtime
        .run_turn(params(
            "s",
            "hello",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "hi");
    assert_eq!(report.rounds, 1);
    assert_eq!(report.usage, Some((10, 5)));

    let history = harness.db.read_history("s").expect("history");
    assert_eq!(
        history,
        [
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "hi".to_string())
        ]
    );
    let (status, _) = harness.db.turn_result(&report.turn_id).expect("turn row");
    assert_eq!(status, "completed");

    // No retained per-turn state: a second turn runs cleanly.
    let report2 = runtime
        .run_turn(params(
            "s",
            "again",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn2");
    assert_eq!(report2.status, TurnStatus::Completed);
}

#[tokio::test]
async fn aud11_text_without_successful_terminal_never_completes() {
    for (terminal, expected, stored) in [
        ("", TurnStatus::Incomplete, "incomplete"),
        (
            "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"server_error\"}}}\n\n",
            TurnStatus::Failed,
            "failed",
        ),
        (
            "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"}}}\n\n",
            TurnStatus::Incomplete,
            "incomplete",
        ),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("s").unwrap();
        let (base, hits) = Fake::start(vec![sse_delta("partial") + terminal], Duration::ZERO);
        let mut observed = String::new();
        let report = runtime
            .run_turn_with_events(
                params("s", "hello", &harness, provider_of(&base), &NO_CANCEL),
                |_| {},
                |_, delta| observed.push_str(delta),
                |_, _| {},
            )
            .await
            .unwrap();
        assert_eq!(
            observed, "partial",
            "fixture must deliver a valid text delta"
        );
        assert_eq!(report.status, expected);
        assert!(report.calls.is_empty());
        assert_eq!(harness.db.turn_result(&report.turn_id).unwrap().0, stored);
        assert_eq!(
            harness.db.read_history("s").unwrap(),
            [("user".to_string(), "hello".to_string())],
            "partial assistant must not be committed as a completed answer"
        );
        assert_eq!(*hits.lock().unwrap(), 1, "no hidden generation retry");
    }
}

#[tokio::test]
async fn aud11_round_exhaustion_retains_output_without_replaying_effect_after_restart() {
    let (mut harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").unwrap();
    let tool = sse_tool_call(
        "call_effect",
        "bash",
        &serde_json::json!({"argv": ["/bin/sh", "-c", "printf 'once\\n' >> effects; printf durable-output"]}),
    );
    let (base, hits, requests) = Fake::start_recording(
        vec![
            tool + &sse_completed(),
            sse_delta("resumed") + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut turn = params(
        "s",
        "record effect",
        &harness,
        provider_of(&base),
        &NO_CANCEL,
    );
    turn.max_rounds = 1;
    let report = runtime.run_turn(turn).await.unwrap();
    assert_eq!(report.status, TurnStatus::Incomplete);
    assert_eq!(report.rounds, 1);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 1, "round budget must stop requests");
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("effects")).unwrap(),
        "once\n"
    );
    let (status, raw) = harness.db.turn_result(&report.turn_id).unwrap();
    assert_eq!(status, "incomplete");
    let journal: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
    let input = journal["input"].as_array().unwrap();
    let output = input
        .iter()
        .find(|item| item["type"] == "function_call_output")
        .unwrap()
        .clone();
    assert_eq!(output["call_id"], "call_effect");
    assert!(
        output["output"]
            .as_str()
            .unwrap()
            .contains("durable-output")
    );
    assert_eq!(
        input
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .count(),
        1
    );
    assert!(input.iter().any(|item| item["type"] == "function_call"
        && item["id"] == "fc_call_effect"
        && item["call_id"] == "call_effect"));

    drop(runtime);
    drop(harness.db);
    harness.db = Db::open(harness._data.path()).unwrap();
    assert_eq!(
        harness.db.turn_result(&report.turn_id).unwrap().0,
        "incomplete"
    );
    let reopened: serde_json::Value =
        serde_json::from_str(&harness.db.turn_result(&report.turn_id).unwrap().1.unwrap()).unwrap();
    assert_eq!(
        reopened, journal,
        "recovery must preserve the durable wire journal"
    );
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.open_session("s").unwrap();
    let resumed = runtime
        .run_turn(params(
            "s",
            "continue",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(resumed.status, TurnStatus::Completed);
    assert!(resumed.calls.is_empty());
    assert_eq!(
        std::fs::read_to_string(harness._project.path().join("effects")).unwrap(),
        "once\n"
    );
    let ops = harness.db.list_tool_ops("s").unwrap();
    assert_eq!(
        ops.len(),
        1,
        "restart must not execute the prior effect again"
    );
    assert_eq!(ops[0].state, "completed");
    assert_eq!(*hits.lock().unwrap(), 2);
    let requests = requests.lock().unwrap();
    let continuation = requests[1]["input"].as_array().unwrap();
    assert_eq!(
        continuation.iter().filter(|item| **item == output).count(),
        1
    );
    assert!(
        continuation
            .iter()
            .any(|item| item["type"] == "function_call"
                && item["id"] == "fc_call_effect"
                && item["call_id"] == "call_effect")
    );
}

#[tokio::test]
async fn cancel_drains_to_records() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let base = Fake::start_stalled(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(30),
    );
    let cancel = AtomicBool::new(false);
    let provider = provider_of(&base);
    let (_, report) = tokio::join!(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel.store(true, Ordering::Relaxed);
        },
        runtime.run_turn(params("s", "slow", &harness, provider, &cancel))
    );
    let report = report.expect("cancelled turn");
    assert_eq!(report.status, TurnStatus::Cancelled);
    let history = harness.db.read_history("s").expect("history");
    assert_eq!(history.len(), 1, "user kept, no partial assistant");
    let (status, _) = harness.db.turn_result(&report.turn_id).expect("turn row");
    assert_eq!(status, "cancelled");
}

#[test]
fn location_binding_holds() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation.clone(), Vec::new());
    runtime.create_session("s").expect("create");
    runtime.open_session("s").expect("same location opens");
    let other = runtime_of(&harness, generation, Vec::new());
    // Same Location id ("work") reopens; a foreign one must fail.
    assert!(other.open_session("s").is_ok());
    assert!(other.open_session("ghost").is_err());
}

#[tokio::test]
async fn reload_applies_new_policy_and_guards_active_turn() {
    let (harness, generation) = make_harness(BTreeMap::new());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    // Default-deny: unlisted tools never run.
    let tool = sse_tool_call("i1", "read", &serde_json::json!({"path": "note.txt"}));
    let (base, _) = Fake::start(vec![tool + &sse_completed()], Duration::ZERO);
    std::fs::write(harness._project.path().join("note.txt"), "file-bytes").expect("seed");
    let report = runtime
        .run_turn(params(
            "s",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert!(
        report.calls[0].output.contains("denied"),
        "{}",
        report.calls[0].output
    );

    // Reload between turns publishes id 2 with read allowed.
    let id = runtime
        .reload(Generation {
            compaction: Default::default(),
            config_diagnostics: Vec::new(),
            animations: None,
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: [("read".to_string(), Permission::Allow)]
                .into_iter()
                .collect(),
            permission_rules: Default::default(),
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        })
        .await
        .expect("reload");
    assert_eq!(id, 2);
    let report = runtime
        .run_turn(params(
            "s",
            "read",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn2");
    assert_eq!(report.calls[0].state, "completed");

    // Reload during an active turn is refused (slow stream within chunk timeout).
    let (slow_base, _) = Fake::start(
        vec![sse_delta("slow") + &sse_completed()],
        Duration::from_secs(2),
    );
    let (reload_result, turn_result) = tokio::join!(
        async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            runtime
                .reload(Generation {
                    compaction: Default::default(),
                    config_diagnostics: Vec::new(),
                    animations: None,
                    providers: BTreeMap::new(),
                    mcp: BTreeMap::new(),
                    permissions: BTreeMap::new(),
                    permission_rules: Default::default(),
                    provenance: BTreeMap::new(),
                    warnings: Vec::new(),
                })
                .await
        },
        runtime.run_turn(params(
            "s",
            "slow",
            &harness,
            provider_of(&slow_base),
            &NO_CANCEL
        ))
    );
    assert_eq!(
        reload_result.expect_err("reload during turn"),
        oc_adapters::runtime::RuntimeError::TurnActive
    );
    assert_eq!(
        turn_result.expect("slow turn").status,
        TurnStatus::Completed
    );
}

#[test]
fn command_expansion_is_single_bounded_pass() {
    let expanded = expand_command(
        "summarize $1 ($ARGUMENTS)",
        &["a".to_string(), "b".to_string()],
    )
    .expect("expand");
    assert_eq!(expanded, "summarize a (a b)");
    assert!(expand_command(&"x".repeat(COMMAND_BYTES_CAP + 1), &[]).is_err());
    // Upstream has no command size limit; a realistic 41 KiB command (owner
    // config shape) must expand instead of failing on a serving cap.
    let large = "x".repeat(41_000);
    assert_eq!(
        expand_command(&large, &[])
            .expect("large command expands")
            .len(),
        large.len()
    );
    assert!(
        expand_command("ok $9", &["only".to_string()])
            .expect("partial")
            .contains("$9")
    );
}

#[tokio::test]
async fn command_invocation_is_durable() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s").expect("create");
    let (base, _) = Fake::start(vec![sse_delta("ok") + &sse_completed()], Duration::ZERO);
    let expanded = expand_command("do $1", &["thing".to_string()]).expect("expand");
    let mut turn_params = params("s", &expanded, &harness, provider_of(&base), &NO_CANCEL);
    turn_params.invocation = Some("/cmd thing".to_string());
    runtime.run_turn(turn_params).await.expect("turn");
    let history = harness.db.read_history("s").expect("history");
    assert_eq!(history[0], ("user".to_string(), "/cmd thing".to_string()));
}

/// DTO extension (iteration 3a): the runtime forwards provider reasoning
/// deltas and reports usage plus the provider-active streamed window, so the
/// TUI can render the reasoning block and the footer's `tok/s`.
#[tokio::test]
async fn dto_reasoning_deltas_and_usage_reach_the_event_callbacks() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("s-dto").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("**Inspecting**\n\n")
                + &sse_reasoning("body")
                + &sse_delta("answer")
                + &sse_completed_usage(42, 7),
        ],
        Duration::from_millis(20),
    );
    let mut accepted = Vec::new();
    let mut reasoning = String::new();
    let mut text = String::new();
    let report = runtime
        .run_turn_with_events(
            params("s-dto", "hello", &harness, provider_of(&base), &NO_CANCEL),
            |turn| accepted.push(turn.to_string()),
            |_, delta| text.push_str(delta),
            |_, delta| reasoning.push_str(delta),
        )
        .await
        .expect("turn");
    assert_eq!(accepted.len(), 1, "one durable acceptance");
    assert_eq!(reasoning, "**Inspecting**\n\nbody");
    assert_eq!(text, "answer");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.usage, Some((42, 7)), "provider-reported usage");
    assert!(
        report.streamed_ms >= 20,
        "provider-active time must be measured: {}ms",
        report.streamed_ms
    );
    assert!(
        report.duration_ms >= 20,
        "turn wall time must be measured: {}ms",
        report.duration_ms
    );
    // Reasoning is never persisted as an assistant message.
    assert_eq!(
        harness.db.read_history("s-dto").unwrap(),
        [
            ("user".to_string(), "hello".to_string()),
            ("assistant".to_string(), "answer".to_string())
        ]
    );
}

#[tokio::test]
async fn two_reasoning_output_items_keep_public_parts_and_opaque_continuation_separate() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("reasoning-items").unwrap();
    let items: Vec<_> = [("rs_1", "encrypted-first"), ("rs_2", "encrypted-second")]
        .into_iter()
        .map(|(id, encrypted_content)| {
            serde_json::json!({
                "type":"reasoning", "id":id, "encrypted_content":encrypted_content,
                "summary":[], "status":"completed"
            })
        })
        .collect();
    let message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_1",
        "status":"completed", "content":[{"type":"output_text", "text":"between"}]});
    let final_message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_2",
        "status":"completed", "content":[{"type":"output_text", "text":"final"}]});
    let (base, hits, requests) = Fake::start_recording(
        vec![
            sse_reasoning("Inspecting")
                + &sse_reasoning_done("rs_1", "encrypted-first")
                + &sse_delta("between")
                + &sse_message_done_by_id(&message)
                + &sse_reasoning("Verifying")
                + &sse_reasoning_done("rs_2", "encrypted-second")
                + &sse_message_done(3, &final_message)
                + &sse_completed_output(vec![
                    items[0].clone(),
                    message,
                    items[1].clone(),
                    final_message,
                ]),
            sse_delta("next") + &sse_completed(),
        ],
        Duration::from_millis(20),
    );
    let public = Arc::new(Mutex::new(Vec::new()));
    let deltas = public.clone();
    let ends = public.clone();
    let texts = public.clone();
    let report = runtime
        .run_turn_with_reasoning_items(
            params(
                "reasoning-items",
                "first",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            move |_, delta| texts.lock().unwrap().push(format!("text:{delta}")),
            move |_, delta| deltas.lock().unwrap().push(delta.to_owned()),
            move |_| ends.lock().unwrap().push("<ended>".into()),
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(
        *public.lock().unwrap(),
        [
            "Inspecting",
            "<ended>",
            "text:between",
            "Verifying",
            "<ended>"
        ]
    );
    assert_eq!(report.text, "betweenfinal");
    assert_eq!(report.usage, Some((10, 5)));
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"][0]["reasoning"], "Inspecting");
    assert_eq!(
        stored["display_parts"][1]["message"], 2,
        "text slot matched by item id without an output index"
    );
    assert_eq!(stored["display_parts"][2]["reasoning"], "Verifying");
    assert_eq!(stored["display_parts"][3]["message"], 4);
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 4);
    assert!(
        report.streamed_ms >= 20,
        "fake delayed the whole generation"
    );
    for part in [&stored["display_parts"][0], &stored["display_parts"][2]] {
        assert!(
            part["duration_ms"].as_u64().unwrap() < report.streamed_ms,
            "each item starts with its own public delta, not the generation request"
        );
    }
    assert!(!stored["display_parts"].to_string().contains("encrypted-"));
    assert!(!stored["display_parts"].to_string().contains("pending_text"));
    assert_eq!(stored["input"][2]["content"][0]["text"], "between");
    assert_eq!(stored["input"][4]["content"][0]["text"], "final");
    assert_eq!(stored["input"][1]["encrypted_content"], "encrypted-first");
    assert_eq!(stored["input"][3]["encrypted_content"], "encrypted-second");
    let next = runtime
        .run_turn(params(
            "reasoning-items",
            "second",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(next.status, TurnStatus::Completed);
    assert_eq!(*hits.lock().unwrap(), 2);
    let requests = requests.lock().unwrap();
    let replay = requests[1]["input"].as_array().unwrap();
    for item in &items {
        assert!(
            replay.contains(item),
            "opaque continuation must survive replay"
        );
    }
    assert!(
        replay
            .iter()
            .any(|item| item["id"] == "msg_1" && item["content"][0]["text"] == "between")
    );
    assert!(
        replay
            .iter()
            .any(|item| item["id"] == "msg_2" && item["content"][0]["text"] == "final")
    );
}

#[tokio::test]
async fn only_done_reasoning_items_have_duration_on_failed_cancelled_or_incomplete_streams() {
    let failed = "data: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\"}}\n\n";
    let incomplete =
        "data: {\"type\":\"response.incomplete\",\"response\":{\"status\":\"incomplete\"}}\n\n";
    for (terminal, expected, stored_status, cancel_after_second) in [
        (failed, TurnStatus::Failed, "failed", false),
        (incomplete, TurnStatus::Incomplete, "incomplete", false),
        ("", TurnStatus::Incomplete, "incomplete", false),
        ("", TurnStatus::Cancelled, "cancelled", true),
    ] {
        let (harness, generation) = make_harness(allow_all());
        let runtime = runtime_of(&harness, generation, Vec::new());
        runtime.create_session("unfinished-reasoning").unwrap();
        let (base, hits) = Fake::start(
            vec![
                sse_reasoning("First")
                    + &sse_reasoning_done("rs_1", "private-first")
                    + &sse_reasoning("Second")
                    + &sse_reasoning(" half")
                    + terminal,
            ],
            Duration::ZERO,
        );
        let cancel = AtomicBool::new(false);
        let mut observed = String::new();
        let mut ended = 0;
        let report = runtime
            .run_turn_with_reasoning_items(
                params(
                    "unfinished-reasoning",
                    "keep the user turn",
                    &harness,
                    provider_of(&base),
                    &cancel,
                ),
                |_| {},
                |_, _| {},
                |_, delta| {
                    observed.push_str(delta);
                    if cancel_after_second && delta == " half" {
                        cancel.store(true, Ordering::Relaxed);
                    }
                },
                |_| ended += 1,
                |_, _| {},
            )
            .await
            .unwrap();
        assert_eq!(observed, "FirstSecond half", "fixture must open r2");
        assert_eq!(ended, 1, "only r1 sent output_item.done");
        assert_eq!(report.status, expected);
        assert!(report.calls.is_empty());
        assert!(
            harness
                .db
                .list_tool_ops("unfinished-reasoning")
                .unwrap()
                .is_empty()
        );
        assert_eq!(*hits.lock().unwrap(), 1, "no retry or tool generation");
        let (status, stored) = harness.db.turn_result(&report.turn_id).unwrap();
        assert_eq!(status, stored_status);
        let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
        let parts = stored["display_parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2, "no synthesized assistant or tool part");
        assert_eq!(parts[0]["reasoning"], "First");
        assert!(parts[0]["duration_ms"].as_u64().is_some());
        assert_eq!(parts[1]["reasoning"], "Second half");
        assert!(parts[1].get("duration_ms").is_none(), "r2 never ended");
        assert!(!stored.to_string().contains("private-first"));
        assert!(!stored.to_string().contains("pending_text"));
        assert_eq!(
            harness.db.read_history("unfinished-reasoning").unwrap(),
            [("user".into(), "keep the user turn".into())]
        );
    }
}

#[tokio::test]
async fn successful_generation_without_reasoning_item_done_leaves_open_part_untimed() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("open-reasoning-success").unwrap();
    let first = serde_json::json!({"type":"reasoning", "id":"rs_1",
        "encrypted_content":"private-first", "summary":[], "status":"completed"});
    let message = serde_json::json!({"type":"message", "role":"assistant", "id":"msg_1",
        "status":"completed", "content":[{"type":"output_text", "text":"answer"}]});
    let (base, hits) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("rs_1", "private-first")
                + &sse_reasoning("Second")
                + &sse_completed_output(vec![first.clone(), message]),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "open-reasoning-success",
            "question",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "answer");
    assert!(report.calls.is_empty());
    assert!(
        harness
            .db
            .list_tool_ops("open-reasoning-success")
            .unwrap()
            .is_empty()
    );
    assert_eq!(*hits.lock().unwrap(), 1);
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    let parts = stored["display_parts"].as_array().unwrap();
    assert_eq!(parts[0]["reasoning"], "First");
    assert!(parts[0]["duration_ms"].as_u64().is_some());
    assert_eq!(parts[1]["reasoning"], "Second");
    assert!(parts[1].get("duration_ms").is_none());
    assert_eq!(parts[2]["message"], 2);
    assert_eq!(parts.len(), 3);
    assert_eq!(
        stored["input"][1], first,
        "only completed opaque item is replayable"
    );
    assert!(
        !stored["display_parts"]
            .to_string()
            .contains("private-first")
    );
    assert_eq!(
        harness.db.read_history("open-reasoning-success").unwrap(),
        [
            ("user".into(), "question".into()),
            ("assistant".into(), "answer".into()),
        ]
    );
}

#[tokio::test]
async fn legacy_reasoning_done_without_item_id_keeps_one_public_part() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("legacy-reasoning").unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("", "private")
                + &sse_reasoning("Second")
                + &sse_delta("answer")
                + &sse_completed(),
        ],
        Duration::ZERO,
    );
    let mut ended = 0;
    let report = runtime
        .run_turn_with_reasoning_items(
            params(
                "legacy-reasoning",
                "hi",
                &harness,
                provider_of(&base),
                &NO_CANCEL,
            ),
            |_| {},
            |_, _| {},
            |_, _| {},
            |_| ended += 1,
            |_, _| {},
        )
        .await
        .unwrap();
    assert_eq!(ended, 0);
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"][0]["reasoning"], "FirstSecond");
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn completed_message_without_delta_or_item_events_stays_between_reasoning_items() {
    let (harness, generation) = make_harness(allow_all());
    let runtime = runtime_of(&harness, generation, Vec::new());
    runtime.create_session("canonical-order").unwrap();
    let first = serde_json::json!({"type":"reasoning","id":"r1","status":"completed","encrypted_content":"private-1"});
    let second = serde_json::json!({"type":"reasoning","id":"r2","status":"completed","encrypted_content":"private-2"});
    let middle = serde_json::json!({"type":"message","id":"m1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"middle"}]});
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("First")
                + &sse_reasoning_done("r1", "private-1")
                + &sse_reasoning("Second")
                + &sse_reasoning_done("r2", "private-2")
                + &sse_completed_output(vec![first, middle, second]),
        ],
        Duration::ZERO,
    );
    let report = runtime
        .run_turn(params(
            "canonical-order",
            "hi",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .unwrap();
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.text, "middle");
    let (_, stored) = harness.db.turn_result(&report.turn_id).unwrap();
    let stored: serde_json::Value = serde_json::from_str(&stored.unwrap()).unwrap();
    assert_eq!(stored["display_parts"].as_array().unwrap().len(), 3);
    assert_eq!(stored["display_parts"][0]["reasoning"], "First");
    assert_eq!(stored["display_parts"][1]["message"], 2);
    assert_eq!(stored["display_parts"][2]["reasoning"], "Second");
    assert!(!stored["display_parts"].to_string().contains("private-"));
}

#[tokio::test]
async fn accepted_prompt_titles_while_main_is_held_and_survives_main_cancel() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, main_release, title_release, _) = split_title_server();
    let (_project, _data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("concurrent-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request["model"] == "title-model"
                && request["input"].to_string().contains("TITLE_AGENT_ONLY")
                && request["input"].to_string().contains("first prompt"))
    );
    title_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let CoreEvent::SessionTitleUpdated {
                session: owner,
                title,
            } = events.recv().await.unwrap()
            {
                assert_eq!(owner, session);
                assert_eq!(title, "Real title");
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.history_page(session.clone(), None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Real title")
    );
    assert!(
        app.submit(session.clone(), "cannot submit while main held".into())
            .await
            .is_err()
    );
    app.cancel(session.clone()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                CoreEvent::TurnInterrupted { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    main_release.store(true, Ordering::Relaxed);
    app.submit(session.clone(), "followup".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.history_page(session, None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Real title")
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["model"] == "title-model")
            .count(),
        1,
        "no second title request on completion or subsequent prompt"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn in_flight_title_is_independent_of_cancel_but_manual_rename_wins() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, _main_release, title_release, _) = split_title_server();
    let (_project, _data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("manual-title-race").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    app.cancel(session.clone()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                CoreEvent::TurnInterrupted { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    app.rename_session(session.clone(), "manual".into())
        .await
        .unwrap();
    app.rename_session(session.clone(), "intermediate".into())
        .await
        .unwrap();
    app.rename_session(session.clone(), "manual".into())
        .await
        .unwrap();
    title_release.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        app.history_page(session, None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("manual")
    );
    assert!(!matches!(
        events.try_recv(),
        Ok(CoreEvent::SessionTitleUpdated { .. })
    ));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn cancelling_automatic_title_does_not_interrupt_main_and_allows_retry() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, main_release, title_release, _) = split_title_server();
    let (_project, _data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("cancel-auto-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    app.cancel_title(session.clone()).await.unwrap();
    title_release.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        app.history_page(session.clone(), None, None, 10)
            .await
            .unwrap()
            .title,
        None
    );
    assert!(
        app.submit(session.clone(), "main still held".into())
            .await
            .is_err()
    );
    main_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.history_page(session.clone(), None, None, 10)
            .await
            .unwrap()
            .title,
        None,
        "no second post-completion title request"
    );
    app.submit(session.clone(), "retry title".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                CoreEvent::SessionTitleUpdated { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(
        app.history_page(session, None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Real title")
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["model"] == "title-model")
            .count(),
        2
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn rejected_reload_keeps_accepted_title_running_on_current_location() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::session::{CoreError, LocationSwitchFailure};

    let (url, requests, _main_release, title_release, title_disconnected) = split_title_server();
    let (project, _data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("rejected-reload-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    app.cancel(session.clone()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                CoreEvent::TurnInterrupted { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();

    // This rebuild fails before publication; the accepted prompt still belongs
    // to the original Location, even though worker() has returned to its owner.
    std::fs::write(project.path().join("opencode.json"), "{invalid json").unwrap();
    assert!(matches!(
        app.reload_location().await,
        Err(CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            ..
        })
    ));
    assert_eq!(
        app.history_page(session.clone(), None, None, 10)
            .await
            .unwrap()
            .title,
        None
    );
    assert!(
        !title_disconnected.load(Ordering::Relaxed),
        "rejected reload must not abort the accepted title"
    );

    title_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if let CoreEvent::SessionTitleUpdated {
                session: owner,
                title,
            } = events.recv().await.unwrap()
            {
                assert_eq!(owner, session);
                assert_eq!(title, "Real title");
                break;
            }
        }
    })
    .await
    .expect("title result must commit after rejected reload");
    assert_eq!(
        app.history_page(session, None, None, 10)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Real title")
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request["model"] == "title-model")
            .count(),
        1
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn title_task_from_retired_location_generation_cannot_commit() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, _main_release, title_release, _) = split_title_server();
    let (_project, data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("retired-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    app.cancel(session.clone()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                events.recv().await.unwrap(),
                CoreEvent::TurnInterrupted { .. }
            ) {
                break;
            }
        }
    })
    .await
    .unwrap();
    app.reload_location().await.unwrap();
    title_release.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='retired-title'",
            [],
            |row| row.get::<_, Option<String>>(0)
        )
        .unwrap(),
        None
    );
    assert_eq!(
        app.history_page(session, None, None, 10)
            .await
            .unwrap()
            .title,
        None
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn completed_turn_shutdown_drains_title_and_persists_before_join() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, main_release, title_release, _) = split_title_server();
    let (_project, data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("graceful-shutdown-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    main_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .expect("main completion must not wait for the title");
    assert_eq!(
        app.history_page(session.clone(), None, None, 10)
            .await
            .unwrap()
            .title,
        None
    );

    let shutdown = tokio::spawn({
        let app = app.clone();
        async move {
            app.shutdown().await.unwrap();
            guard.join().await.unwrap();
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !shutdown.is_finished(),
        "normal shutdown must drain the pending title"
    );
    title_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), shutdown)
        .await
        .expect("shutdown bound")
        .unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='graceful-shutdown-title'",
            [],
            |row| row.get::<_, Option<String>>(0)
        )
        .unwrap()
        .as_deref(),
        Some("Real title")
    );
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["model"] == "title-model")
            .count(),
        1,
        "drain cannot issue a replacement title request"
    );
}

#[tokio::test]
async fn shutdown_aborts_unfinished_automatic_title() {
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    let (url, requests, main_release, title_release, title_disconnected) = split_title_server();
    let (_project, data, _home, app, guard) = title_fixture(&url).await;
    let session = SessionId::new("shutdown-title").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(session.clone(), "first prompt".into(), None)
        .await
        .unwrap();
    wait_for_title_requests(&requests, 2).await;
    main_release.store(true, Ordering::Relaxed);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(events.recv().await.unwrap(), CoreEvent::TurnFinished { .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    let start = std::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(4), async {
        app.shutdown().await.unwrap();
        guard.join().await.unwrap();
    })
    .await
    .expect("held title must not stall shutdown");
    assert!(
        start.elapsed() >= Duration::from_millis(500),
        "title got no grace"
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        while !title_disconnected.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("title provider connection must close before worker join returns");
    title_release.store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='shutdown-title'",
            [],
            |row| row.get::<_, Option<String>>(0)
        )
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn bare_title_regeneration_uses_title_agent_and_preserves_concurrent_manual_rename() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    use oc_core::session::CoreError;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, hits, requests) = Fake::start_recording(
        vec![
            sse_delta("late title") + &sse_completed(),
            sse_delta("Fresh title") + &sse_completed(),
        ],
        Duration::from_millis(800),
    );
    let config = serde_json::json!({
        "model": "fixture/main",
        "provider": {"fixture": {"npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
            "models": {"main": {}, "title-model": {}}}},
        "agent": {"title": {"mode": "subagent", "model": "fixture/title-model", "prompt": "TITLE_AGENT_ONLY"}}
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("title-root").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let mut events = app.subscribe();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    assert!(
        app.regenerate_title(session.clone()).await.is_err(),
        "empty history rejected"
    );
    conn.execute("INSERT INTO sessions(id,created_at,parent_id,title) VALUES ('child','test','title-root','child')", []).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','title-root',1,'user',?1)",
        [&format!("FIRST_SENTINEL {}", "x".repeat(20000))]).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('recent','title-root',2,'assistant','RECENT_SENTINEL')", []).unwrap();
    assert_eq!(
        app.regenerate_title(SessionId::new("child").unwrap()).await,
        Err(CoreError::SessionNotFound)
    );
    assert_eq!(
        *hits.lock().unwrap(),
        0,
        "invalid requests cannot reach the provider"
    );
    let owner = app.clone();
    let target = session.clone();
    let first = tokio::spawn(async move { owner.regenerate_title(target).await });
    tokio::time::timeout(Duration::from_secs(3), async {
        while requests.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(200),
            app.history_page(session.clone(), None, None, 4)
        )
        .await
        .unwrap()
        .is_ok(),
        "snapshots must not block on provider"
    );
    assert_eq!(
        app.regenerate_title(session.clone()).await,
        Err(CoreError::TurnBusy)
    );
    app.rename_session(session.clone(), "manual wins".into())
        .await
        .unwrap();
    assert!(first.await.unwrap().is_err());
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='title-root'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "manual wins"
    );
    let result = app.regenerate_title(session.clone()).await.unwrap();
    assert_eq!(result, "Fresh title");
    assert_eq!(
        app.history_page(session.clone(), None, None, 5)
            .await
            .unwrap()
            .title
            .as_deref(),
        Some("Fresh title")
    );
    let captured = requests.lock().unwrap().clone();
    assert_eq!(captured.len(), 2);
    for request in captured.iter() {
        assert_eq!(request["model"], "title-model");
        assert!(
            request.get("tools").is_none()
                || request["tools"].as_array().is_some_and(Vec::is_empty)
        );
        let encoded = request["input"].to_string();
        assert!(encoded.contains("TITLE_AGENT_ONLY"));
        assert!(encoded.contains("FIRST_SENTINEL"));
        assert!(encoded.len() < 13000, "bounded title input");
    }
    assert!(captured[1]["input"].to_string().contains("RECENT_SENTINEL"));
    assert_eq!(
        captured[1]["input"]
            .to_string()
            .matches("FIRST_SENTINEL")
            .count(),
        1,
        "original request is not repeated in recent history"
    );
    assert_eq!(*hits.lock().unwrap(), 2);
    let turns: i64 = conn
        .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
        .unwrap();
    assert_eq!(turns, 0, "regeneration is not a billed conversational turn");
    assert!(
        matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ),
        "title requests must not emit turn or usage events"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn bare_title_cancel_does_not_persist_or_emit_turn() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let url = Fake::start_stalled(
        vec![sse_delta("too late") + &sse_completed()],
        Duration::from_secs(2),
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model": "fixture/main", "provider": {"fixture": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": url, "apiKey": "dummy"}, "models": {"main": {}}}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let id = SessionId::new("cancel-title").unwrap();
    app.create_session(id.clone()).await.unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','cancel-title',1,'user','please title')", []).unwrap();
    let owner = app.clone();
    let target = id.clone();
    let pending = tokio::spawn(async move { owner.regenerate_title(target).await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    app.cancel_title(id.clone()).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_secs(3), pending)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    assert_eq!(
        app.history_page(id, None, None, 4).await.unwrap().title,
        None
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn bare_title_invalid_configured_model_is_rejected_before_provider_io() {
    use oc_adapters::application;
    use oc_core::domain::SessionId;
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, hits) = Fake::start(
        vec![sse_delta("must not arrive") + &sse_completed()],
        Duration::ZERO,
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model": "fixture/main", "provider": {"fixture": {"npm": "@ai-sdk/openai",
            "options": {"baseURL": url, "apiKey": "dummy"}, "models": {"main": {}}}},
            "agent": {"title": {"mode": "subagent", "model": "fixture/retired", "prompt": "title"}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("invalid-title-model").unwrap();
    app.create_session(session.clone()).await.unwrap();
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    conn.execute("INSERT INTO messages(id,session_id,seq,role,text) VALUES ('first','invalid-title-model',1,'user','request')", []).unwrap();
    assert!(app.regenerate_title(session.clone()).await.is_err());
    assert_eq!(*hits.lock().unwrap(), 0);
    assert_eq!(
        app.history_page(session, None, None, 2)
            .await
            .unwrap()
            .title,
        None
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn dto_application_events_surface_reasoning_and_usage() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;

    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let home = tempfile::tempdir().expect("home");
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("**Planning**\n\n") + &sse_delta("visible") + &sse_completed_usage(9, 4),
        ],
        Duration::from_millis(20),
    );
    let config = serde_json::json!({
        "model": "fixture/fixture-model",
        "provider": {"fixture": {
            "npm": "@ai-sdk/openai",
            "options": {"baseURL": base, "apiKey": "test-key"},
            "models": {"fixture-model": {
                "name": "DTO fixture",
                "limit": {"context": 65536, "output": 4096},
            }},
        }},
        "permissions": {},
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).expect("config");
    let env: BTreeMap<String, String> = [
        ("HOME", home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK", "1".to_string()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value))
    .collect();
    let (app, guard, _diagnostics) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .expect("application");
    let session = SessionId::new("s-app-dto").expect("session id");
    app.create_session(session.clone()).await.expect("create");
    let mut rx = app.subscribe();
    app.submit(session.clone(), "hello".to_string())
        .await
        .expect("submit");

    let mut reasoning = String::new();
    let mut usage = None;
    let (text, duration_ms) = loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel");
        match event {
            CoreEvent::ReasoningDelta { delta, .. } => reasoning.push_str(&delta),
            CoreEvent::TurnUsage {
                input_tokens,
                output_tokens,
                streamed_ms,
                ..
            } => usage = Some((input_tokens, output_tokens, streamed_ms)),
            CoreEvent::TurnFinished {
                text, duration_ms, ..
            } => break (text, duration_ms),
            CoreEvent::TurnFailed { error, .. } => panic!("unexpected failure: {error}"),
            CoreEvent::McpChanged(snapshot) => assert!(snapshot.servers.is_empty()),
            CoreEvent::ProviderChanged => panic!("unexpected native discovery in static fixture"),
            CoreEvent::TurnStarted { .. }
            | CoreEvent::SessionTitleUpdated { .. }
            | CoreEvent::TurnPresentation { .. }
            | CoreEvent::ReasoningItemEnded { .. }
            | CoreEvent::TextDelta { .. }
            | CoreEvent::ToolCallStarted { .. }
            | CoreEvent::ToolArgumentStream { .. }
            | CoreEvent::ToolCallFinished { .. }
            | CoreEvent::TurnInterrupted { .. }
            | CoreEvent::Compaction(_)
            | CoreEvent::PermissionAsked(_)
            | CoreEvent::PermissionResolved { .. } => {}
        }
    };
    assert_eq!(reasoning, "**Planning**\n\n", "reasoning delta surfaces");
    assert_eq!(
        usage.map(|(input, output, _)| (input, output)),
        Some((9, 4)),
        "provider usage surfaces"
    );
    assert!(
        usage.is_some_and(|(_, _, streamed_ms)| streamed_ms >= 20),
        "provider-active time is measured"
    );
    assert_eq!(text, "visible");
    assert!(duration_ms >= 20, "turn duration is measured");
    app.shutdown().await.expect("shutdown");
    guard.join().await.expect("join");
}

#[tokio::test]
async fn anonymous_text_after_second_reasoning_keeps_unstreamed_first_message_on_restart() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::TranscriptPart;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (base, _) = Fake::start(
        vec![
            sse_reasoning("Inspecting")
                + &sse_reasoning_done("rs_1", "private-1")
                + &sse_reasoning("Verifying")
                + &sse_reasoning_done("rs_2", "private-2")
                + &sse_delta("final")
                + &sse_completed_output(vec![
                    serde_json::json!({"type":"reasoning","id":"rs_1","encrypted_content":"private-1","status":"completed"}),
                    serde_json::json!({"type":"message","role":"assistant","id":"m1","status":"completed","content":[{"type":"output_text","text":"between"}]}),
                    serde_json::json!({"type":"reasoning","id":"rs_2","encrypted_content":"private-2","status":"completed"}),
                    serde_json::json!({"type":"message","role":"assistant","id":"m2","status":"completed","content":[{"type":"output_text","text":"final"}]}),
                ]),
        ],
        Duration::ZERO,
    );
    std::fs::write(
        project.path().join("opencode.json"),
        serde_json::json!({
            "model":"fixture/m", "provider":{"fixture":{"npm":"@ai-sdk/openai",
            "options":{"baseURL":base,"apiKey":"test-key"}, "models":{"m":{}}}}
        })
        .to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().to_string()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let session = SessionId::new("two-items-app").unwrap();
    app.create_session(session.clone()).await.unwrap();
    // This fake scripts the reasoning reply by connection order. Keep the
    // fixture focused on replay by pre-titling the root before acceptance.
    app.rename_session(session.clone(), "Reasoning replay".into())
        .await
        .unwrap();
    let mut rx = app.subscribe();
    let turn = app.submit(session.clone(), "first".into()).await.unwrap();
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let finished = matches!(&event, CoreEvent::TurnFinished { turn: id, .. } if id == &turn);
        match event {
            CoreEvent::ReasoningDelta {
                turn: id, delta, ..
            } if id == turn => events.push(delta),
            CoreEvent::TextDelta {
                turn: id, delta, ..
            } if id == turn => events.push(format!("text:{delta}")),
            CoreEvent::ReasoningItemEnded { turn: id, .. } if id == turn => {
                events.push("<ended>".into())
            }
            CoreEvent::TurnFailed { error, .. } => panic!("turn failed: {error}"),
            _ => {}
        }
        if finished {
            break;
        }
    }
    assert_eq!(
        events,
        [
            "Inspecting",
            "<ended>",
            "Verifying",
            "<ended>",
            "text:final"
        ]
    );
    let page = app
        .history_page(session.clone(), None, None, 10)
        .await
        .unwrap();
    let assistant = page
        .rows
        .iter()
        .find(|message| message.turn.as_ref().is_some_and(|t| t.id == turn.0))
        .unwrap();
    let parts = &assistant.turn.as_ref().unwrap().parts;
    assert!(
        matches!(&parts[..], [TranscriptPart::Reasoning { text: first, .. }, TranscriptPart::Text(between), TranscriptPart::Reasoning { text: second, .. }, TranscriptPart::Text(answer)]
        if first == "Inspecting" && between == "between" && second == "Verifying" && answer == "final")
    );
    assert_eq!(parts.len(), 4);
    assert_eq!(assistant.text, "betweenfinal");
    assert!(!format!("{page:?}").contains("private-"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let (app, guard, _) = application::spawn_with_env(
        project.path(),
        data.path(),
        BTreeMap::from([
            ("HOME".into(), home.path().to_string_lossy().to_string()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let replay = app.history_page(session, None, None, 10).await.unwrap();
    assert_eq!(
        replay
            .rows
            .iter()
            .find(|row| row.turn.as_ref().is_some_and(|t| t.id == turn.0))
            .unwrap()
            .turn
            .as_ref()
            .unwrap()
            .parts,
        *parts
    );
    assert!(!format!("{replay:?}").contains("private-"));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

/// End to end through the real application worker: `application::spawn_with_env`
/// broadcasts the tool-call events next to the text/turn events, so the TUI
/// transcript can render a patch card from live state.
#[tokio::test]
async fn dto_application_events_surface_tool_calls() {
    check_application_patch_replay("hello", 1).await;
}

#[tokio::test]
async fn application_fresh_turn_validates_and_atomically_pins_home_choice() {
    use oc_adapters::application;
    use oc_core::core_app::{CoreEvent, FreshSelection};
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction;
    use oc_core::session::CoreError;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(100),
    );
    let config = serde_json::json!({
        "model":"fixture/main", "default_agent":"build",
        "provider":{"fixture":{
            "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
            "models":{
                "main":{"limit":{"context":65536,"output":4096},
                    "variants":{"low":{"reasoningEffort":"low"}}},
                "other":{"limit":{"context":65536,"output":4096},
                    "variants":{"deep":{"reasoningEffort":"high"}}}
            }
        }},
        "agent":{
            "build":{"mode":"primary","prompt":"BUILD_PRIMARY"},
            "review":{"mode":"primary","prompt":"REVIEW_PRIMARY"},
            "helper":{"mode":"subagent","prompt":"HELPER_CHILD"}
        }
    });
    std::fs::write(project.path().join("opencode.json"), config.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let sid = |name| SessionId::new(name).unwrap();
    let chosen = |agent: &str, model: &str, variant: Option<&str>| FreshSelection {
        agent_id: Some(agent.into()),
        model_id: model.into(),
        variant: variant.map(str::to_string),
    };
    app.select_model("main".into(), Some("low".into()))
        .await
        .unwrap();
    for (id, text, choice) in [
        ("empty", " \n ", None),
        ("bad-agent", "prompt", Some(chosen("helper", "main", None))),
        (
            "bad-variant",
            "prompt",
            Some(chosen("review", "main", Some("missing"))),
        ),
        (
            "bad-model",
            "prompt",
            Some(chosen("review", "missing", None)),
        ),
    ] {
        assert!(
            app.submit_fresh(sid(id), text.into(), choice)
                .await
                .is_err(),
            "{id} must be refused"
        );
        assert!(app.read_history(sid(id)).await.is_err());
    }
    assert!(app.list_sessions().await.unwrap().is_empty());
    assert!(
        requests.lock().unwrap().is_empty(),
        "refusal never calls provider"
    );

    let mut events = app.subscribe();
    let implicit = sid("fresh-implicit");
    app.submit_fresh(implicit.clone(), "first".into(), None)
        .await
        .expect("durable first turn");
    assert_eq!(
        app.submit_fresh(sid("fresh-busy"), "busy".into(), None)
            .await,
        Err(CoreError::TurnBusy)
    );
    assert!(app.read_history(sid("fresh-busy")).await.is_err());
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == implicit => break,
            CoreEvent::TurnFailed { error, .. } => panic!("implicit turn failed: {error}"),
            _ => {}
        }
    }
    assert_eq!(
        app.session_selection(implicit.clone(), false, SessionSelectionAction::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("low")
    );

    let explicit = sid("fresh-explicit");
    app.submit_fresh(
        explicit.clone(),
        "second".into(),
        Some(chosen("review", "other", Some("deep"))),
    )
    .await
    .expect("explicit first turn");
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == explicit => break,
            CoreEvent::TurnFailed { error, .. } => panic!("explicit turn failed: {error}"),
            _ => {}
        }
    }
    let actual = app
        .session_selection(explicit.clone(), false, SessionSelectionAction::Current)
        .await
        .unwrap();
    assert_eq!(actual.agent_id.as_deref(), Some("review"));
    assert_eq!(actual.model_id, "other");
    assert_eq!(actual.variant.as_deref(), Some("deep"));
    assert_eq!(
        app.read_history(explicit.clone()).await.unwrap()[0].text,
        "second"
    );
    assert_eq!(
        app.read_history(implicit.clone()).await.unwrap()[0].text,
        "first"
    );
    assert_eq!(
        app.submit_fresh(explicit.clone(), "again".into(), None)
            .await,
        Err(CoreError::SessionAlreadyExists)
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let db = Db::open(data.path()).unwrap();
    let location = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for id in ["fresh-implicit", "fresh-explicit"] {
        assert_eq!(
            db.get_pref(&format!("{SESSION_LOCATION_PREFIX}{id}"))
                .unwrap()
                .as_deref(),
            Some(location.as_str())
        );
    }
    let key = |id: &str| {
        format!(
            "tui.selection.session:{}",
            serde_json::json!([location, "fixture", id])
        )
    };
    let implicit_record: serde_json::Value = serde_json::from_str(
        &db.get_pref(&key("fresh-implicit"))
            .unwrap()
            .expect("implicit Home choice pinned with first turn"),
    )
    .unwrap();
    assert_eq!(implicit_record["agent"], "build");
    assert_eq!(implicit_record["models"]["build"]["id"], "main");
    assert_eq!(implicit_record["models"]["build"]["variant"], "low");
    let record: serde_json::Value = serde_json::from_str(
        &db.get_pref(&key("fresh-explicit"))
            .unwrap()
            .expect("atomic selection"),
    )
    .unwrap();
    assert_eq!(record["agent"], "review");
    assert_eq!(record["models"]["review"]["id"], "other");
    assert_eq!(record["models"]["review"]["variant"], "deep");
    assert_eq!(record["epoch"], 1);
    let captured = requests.lock().unwrap();
    // The title provider may add a request after each turn; locate the turn
    // requests by their user input instead of relying on title call ordering.
    assert!(captured.iter().any(|r| r["model"] == "main"
        && r["input"].to_string().contains("first")
        && r["input"].to_string().contains("BUILD_PRIMARY")));
    assert!(captured.iter().any(|r| r["model"] == "other"
        && r["input"].to_string().contains("second")
        && r["input"].to_string().contains("REVIEW_PRIMARY")));
}

#[tokio::test]
async fn application_home_actions_are_sessionless_and_fresh_turn_pins_current_location_choice() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;
    use oc_core::session::CoreError;

    let project = tempfile::tempdir().unwrap();
    let other_location = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let user_home = tempfile::tempdir().unwrap();
    let (base, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(200),
    );
    let config = |model: &str| {
        serde_json::json!({
            "model":format!("fixture/{model}"), "default_agent":"build",
            "provider":{"fixture":{
                "npm":"@ai-sdk/openai", "options":{"baseURL":base,"apiKey":"fixture-key"},
                "models":{
                    "main":{"limit":{"context":65536,"output":4096},
                        "variants":{"low":{"reasoningEffort":"low"}}},
                    "other":{"limit":{"context":65536,"output":4096},
                        "variants":{"deep":{"reasoningEffort":"high"}}}
                }
            }},
            "agent":{
                "build":{"mode":"primary","prompt":"BUILD_PRIMARY"},
                "review":{"mode":"primary","prompt":"REVIEW_PRIMARY"},
                "helper":{"mode":"subagent"}
            }
        })
    };
    std::fs::write(
        project.path().join("opencode.json"),
        config("main").to_string(),
    )
    .unwrap();
    std::fs::write(
        other_location.path().join("opencode.json"),
        config("other").to_string(),
    )
    .unwrap();
    let env = BTreeMap::from([
        (
            "HOME".into(),
            user_home.path().to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let sid = SessionId::new("home-chosen").unwrap();
    let location = project
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let other = other_location
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let current = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(current.chrome.location.as_deref(), Some(location.as_str()));
    assert_eq!(current.model_id, "main");
    assert_eq!(current.agent_id.as_deref(), Some("build"));

    let selected = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(
        (selected.model_id.as_str(), selected.variant.as_deref()),
        ("other", None)
    );
    assert_eq!(
        app.home_selection(Action::Variant(Some("deep".into())))
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("deep")
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!(
        (review.model_id.as_str(), review.agent_id.as_deref()),
        ("main", Some("review"))
    );
    let review = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(review.variant.as_deref(), Some("deep"));
    let review = app.home_selection(Action::Variant(None)).await.unwrap();
    assert_eq!(review.variant, None);
    let build = app
        .home_selection(Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(
        (build.model_id.as_str(), build.variant.as_deref()),
        ("other", Some("deep"))
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!((review.model_id.as_str(), review.variant), ("other", None));
    assert_eq!(
        app.home_selection(Action::New(Some("review".into())))
            .await
            .unwrap()
            .model_id,
        "other"
    );
    let reset = app.home_selection(Action::New(None)).await.unwrap();
    assert_eq!(
        (
            reset.agent_id.as_deref(),
            reset.model_id.as_str(),
            reset.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    let before = app.home_selection(Action::Current).await.unwrap();
    for rejected in [
        Action::Model("retired".into()),
        Action::Variant(Some("missing".into())),
        Action::Agent("helper".into()),
    ] {
        assert!(app.home_selection(rejected).await.is_err());
        assert_eq!(app.home_selection(Action::Current).await.unwrap(), before);
    }
    assert!(
        app.list_sessions().await.unwrap().is_empty(),
        "Home-only actions must not create a root"
    );
    let conn = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    let session_prefs: i64 = conn
        .query_row(
            "SELECT count(*) FROM prefs WHERE key LIKE 'tui.selection.session:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(session_prefs, 0, "Home has no phantom session preference");
    assert_eq!(
        app.select_model("main".into(), Some("low".into()))
            .await
            .unwrap()
            .model_id,
        "main"
    );

    let mut events = app.subscribe();
    app.submit_fresh(sid.clone(), "chosen Home".into(), None)
        .await
        .expect("durable first turn");
    assert_eq!(
        app.home_selection(Action::Variant(None)).await,
        Err(CoreError::TurnBusy)
    );
    assert_eq!(
        app.home_selection(Action::Current).await,
        Err(CoreError::TurnBusy)
    );
    let key = format!(
        "tui.selection.session:{}",
        serde_json::json!([location, "fixture", "home-chosen"])
    );
    let pinned: serde_json::Value = serde_json::from_str(
        &conn
            .query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(pinned["agent"], "build");
    assert_eq!(pinned["models"]["build"]["id"], "other");
    assert_eq!(pinned["models"]["build"]["variant"], "deep");
    assert_eq!(pinned["epoch"], 1);
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("Home turn failed: {error}"),
            _ => {}
        }
    }
    let actual = app
        .session_selection(sid.clone(), false, Action::Current)
        .await
        .unwrap();
    assert_eq!(
        (actual.model_id.as_str(), actual.variant.as_deref()),
        ("other", Some("deep"))
    );
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other"
                && r["input"].to_string().contains("chosen Home")
                && r["input"].to_string().contains("BUILD_PRIMARY"))
    );

    let switched = app.switch_location(other.clone()).await.unwrap();
    assert_eq!(switched.location, other);
    let baseline = app.list_sessions().await.unwrap().len(); // legacy switch creates its own session
    let second = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (second.model_id.as_str(), second.agent_id.as_deref()),
        ("main", Some("build"))
    );
    let second = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(
        (second.model_id.as_str(), second.variant.as_deref()),
        ("other", None)
    );
    assert_eq!(app.list_sessions().await.unwrap().len(), baseline);
    let home_again = app.switch_location(location.clone()).await.unwrap();
    assert_eq!(home_again.location, location);
    let restored = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            restored.agent_id.as_deref(),
            restored.model_id.as_str(),
            restored.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    assert_eq!(
        app.session_selection(sid, false, Action::Current)
            .await
            .unwrap()
            .model_id,
        "other"
    );
    // A catalog refresh can retire an earlier Home id. Keep the exact visible
    // choice, refuse a re-selection/first turn, and leave all existing rows and
    // preferences untouched until an explicit admitted replacement is chosen.
    let mut reduced = config("main");
    reduced["provider"]["fixture"]["models"]
        .as_object_mut()
        .unwrap()
        .remove("other");
    std::fs::write(project.path().join("opencode.json"), reduced.to_string()).unwrap();
    app.switch_location(other).await.unwrap();
    app.switch_location(location).await.unwrap();
    let retired = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(retired.model_id, "other");
    assert!(!retired.models.iter().any(|model| model.id == "other"));
    let rows_before = app.list_sessions().await.unwrap();
    let pref_before: String = conn
        .query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(
        app.home_selection(Action::Model("other".into()))
            .await
            .is_err()
    );
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), retired);
    assert!(
        app.submit_fresh(
            SessionId::new("retired-home").unwrap(),
            "refused".into(),
            None
        )
        .await
        .is_err()
    );
    assert_eq!(app.list_sessions().await.unwrap(), rows_before);
    assert_eq!(
        conn.query_row("SELECT value FROM prefs WHERE key = ?1", [&key], |r| r
            .get::<_, String>(
            0
        ))
        .unwrap(),
        pref_before
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_location_switch_restores_choices_without_roots_and_first_turn_binds_target() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;
    use oc_core::session::{CoreError, LocationSwitchFailure};

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let bad = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(40),
    );
    let config = |default: &str| {
        serde_json::json!({
            "model": format!("fixture/{default}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {"main": {"limit": {"context": 65536, "output": 4096}},
                           "other": {"limit": {"context": 65536, "output": 4096}}}
            }}
        })
    };
    std::fs::write(a.path().join("opencode.json"), config("main").to_string()).unwrap();
    std::fs::write(b.path().join("opencode.json"), config("other").to_string()).unwrap();
    std::fs::write(
        bad.path().join("opencode.json"),
        config("missing").to_string(),
    )
    .unwrap();
    let (app, guard, _) = application::spawn_with_env(
        a.path(),
        data.path(),
        BTreeMap::from([
            ("HOME".into(), home.path().to_string_lossy().into_owned()),
            ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
        ]),
    )
    .await
    .unwrap();
    let a_path = a
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let b_path = b
        .path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let original = app
        .home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    assert_eq!(original.model_id, "other");
    let error = app
        .switch_location_home(bad.path().display().to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            ..
        }
    ));
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), original);
    assert!(app.list_sessions().await.unwrap().is_empty());

    let target = app.switch_location_home(b_path.clone()).await.unwrap();
    assert_eq!(target.location, b_path);
    assert_eq!(target.catalog.model_id, "other");
    assert_eq!(
        target.catalog.chrome.location.as_deref(),
        Some(b_path.as_str())
    );
    assert!(app.list_sessions().await.unwrap().is_empty());
    let target = app.switch_location_home(a_path.clone()).await.unwrap();
    assert_eq!(target.catalog.model_id, "other");
    assert_eq!(
        target.catalog.chrome.location.as_deref(),
        Some(a_path.as_str())
    );
    app.home_selection(Action::Model("main".into()))
        .await
        .unwrap();
    let target = app.switch_location_home(b_path.clone()).await.unwrap();
    assert_eq!(
        target.catalog.model_id, "other",
        "B keeps its own Home choice"
    );
    assert!(app.list_sessions().await.unwrap().is_empty());

    let mut events = app.subscribe();
    let b_id = SessionId::new("fresh-in-b").unwrap();
    app.submit_fresh(b_id.clone(), "first B".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == b_id => break,
            CoreEvent::TurnFailed { error, .. } => panic!("B: {error}"),
            _ => {}
        }
    }
    assert_eq!(app.list_sessions().await.unwrap(), vec![b_id.clone()]);
    assert_eq!(
        app.switch_location_home(a_path.clone())
            .await
            .unwrap()
            .catalog
            .model_id,
        "main"
    );
    let a_id = SessionId::new("fresh-in-a").unwrap();
    app.submit_fresh(a_id.clone(), "first A".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == a_id => break,
            CoreEvent::TurnFailed { error, .. } => panic!("A: {error}"),
            _ => {}
        }
    }
    assert_eq!(app.list_sessions().await.unwrap().len(), 2);
    let db = rusqlite::Connection::open(data.path().join("oc.sqlite")).unwrap();
    for (id, path) in [(&a_id, &a_path), (&b_id, &b_path)] {
        let bound: String = db
            .query_row(
                "SELECT value FROM prefs WHERE key = ?1",
                [format!(
                    "{}{}",
                    oc_adapters::runtime::SESSION_LOCATION_PREFIX,
                    id.0
                )],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bound, *path);
    }
    {
        let captured = requests.lock().unwrap();
        assert!(
            captured
                .iter()
                .any(|r| r["model"] == "other" && r["input"].to_string().contains("first B"))
        );
        assert!(
            captured
                .iter()
                .any(|r| r["model"] == "main" && r["input"].to_string().contains("first A"))
        );
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_current_reloads_config_after_location_roundtrip_without_explicit_choice() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;

    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(30),
    );
    let config = |model: &str| {
        serde_json::json!({
            "model": format!("fixture/{model}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {"main": {}, "other": {}}
            }}
        })
    };
    std::fs::write(a.path().join("opencode.json"), config("main").to_string()).unwrap();
    std::fs::write(b.path().join("opencode.json"), config("main").to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(a.path(), data.path(), env)
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "main"
    );
    app.switch_location_home(b.path().display().to_string())
        .await
        .unwrap();
    std::fs::write(a.path().join("opencode.json"), config("other").to_string()).unwrap();
    let reloaded = app
        .switch_location_home(a.path().display().to_string())
        .await
        .unwrap();
    assert_eq!(reloaded.catalog.model_id, "other");
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "other"
    );
    let mut events = app.subscribe();
    let sid = SessionId::new("config-reloaded").unwrap();
    app.submit_fresh(sid.clone(), "reloaded turn".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("reloaded turn: {error}"),
            _ => {}
        }
    }
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other" && r["input"].to_string().contains("reloaded turn"))
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[tokio::test]
async fn home_draft_hydrates_after_restart_and_retirement_requires_explicit_replacement() {
    use oc_adapters::application;
    use oc_core::core_app::CoreEvent;
    use oc_core::domain::SessionId;
    use oc_core::queries::SessionSelectionAction as Action;

    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let (url, _, requests) = Fake::start_recording(
        vec![sse_delta("answer") + &sse_completed()],
        Duration::from_millis(30),
    );
    let config = |retired: bool| {
        let mut config = serde_json::json!({
            "model": "fixture/main", "default_agent": "build",
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai", "options": {"baseURL": url, "apiKey": "dummy"},
                "models": {
                    "main": {"variants": {"low": {"reasoningEffort": "low"}}},
                    "other": {"variants": {"deep": {"reasoningEffort": "high"}}}
                }
            }},
            "agent": {
                "build": {"mode": "primary", "prompt": "BUILD_PRIMARY"},
                "review": {"mode": "primary", "prompt": "REVIEW_PRIMARY"}
            }
        });
        if retired {
            config["provider"]["fixture"]["models"]
                .as_object_mut()
                .unwrap()
                .remove("other");
        }
        config
    };
    let path = project.path().join("opencode.json");
    std::fs::write(&path, config(false).to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.path().to_string_lossy().into_owned()),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    assert_eq!(
        app.home_selection(Action::Current).await.unwrap().model_id,
        "main"
    );
    app.home_selection(Action::Model("other".into()))
        .await
        .unwrap();
    app.home_selection(Action::Variant(Some("deep".into())))
        .await
        .unwrap();
    app.home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    app.home_selection(Action::Variant(Some("low".into())))
        .await
        .unwrap();
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env.clone())
        .await
        .unwrap();
    let current = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (
            current.agent_id.as_deref(),
            current.model_id.as_str(),
            current.variant.as_deref()
        ),
        (Some("build"), "other", Some("deep"))
    );
    let sid = SessionId::new("restored-home-draft").unwrap();
    let mut events = app.subscribe();
    app.submit_fresh(sid.clone(), "restored turn".into(), None)
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            CoreEvent::TurnFinished { session, .. } if session == sid => break,
            CoreEvent::TurnFailed { error, .. } => panic!("restored turn: {error}"),
            _ => {}
        }
    }
    assert!(
        requests
            .lock()
            .unwrap()
            .iter()
            .any(|r| r["model"] == "other"
                && r["input"].to_string().contains("restored turn")
                && r["input"].to_string().contains("BUILD_PRIMARY"))
    );
    assert_eq!(
        app.session_selection(sid, false, Action::Current)
            .await
            .unwrap()
            .variant
            .as_deref(),
        Some("deep")
    );
    let review = app
        .home_selection(Action::Agent("review".into()))
        .await
        .unwrap();
    assert_eq!(
        (
            review.agent_id.as_deref(),
            review.model_id.as_str(),
            review.variant.as_deref()
        ),
        (Some("review"), "main", Some("low"))
    );
    let build = app
        .home_selection(Action::Agent("build".into()))
        .await
        .unwrap();
    assert_eq!(
        (build.model_id.as_str(), build.variant.as_deref()),
        ("other", Some("deep"))
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();

    std::fs::write(&path, config(true).to_string()).unwrap();
    let (app, guard, _) = application::spawn_with_env(project.path(), data.path(), env)
        .await
        .unwrap();
    let retired = app.home_selection(Action::Current).await.unwrap();
    assert_eq!(
        (retired.model_id.as_str(), retired.variant.as_deref()),
        ("other", Some("deep"))
    );
    assert!(!retired.models.iter().any(|m| m.id == "other"));
    assert!(
        app.submit_fresh(
            SessionId::new("retired-draft").unwrap(),
            "refuse".into(),
            None
        )
        .await
        .is_err()
    );
    assert!(
        app.read_history(SessionId::new("retired-draft").unwrap())
            .await
            .is_err()
    );
    assert_eq!(app.home_selection(Action::Current).await.unwrap(), retired);
    assert_eq!(
        app.home_selection(Action::Model("main".into()))
            .await
            .unwrap()
            .model_id,
        "main"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
