use super::*;

#[test]
fn vis12_shared_input_history_bounds_filters_dedups_and_commits_with_acceptance() {
    let root = std::env::temp_dir().join(format!(
        "oc-prompt-history-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = Db::open(&root).unwrap();
    let mut legacy: Vec<serde_json::Value> = (0..65)
        .map(|i| serde_json::json!(format!("prompt {i}")))
        .collect();
    legacy.extend([
        serde_json::json!(null),
        serde_json::json!({"text":"legacy"}),
        serde_json::json!(""),
        serde_json::json!(["nested"]),
        serde_json::json!("prompt 64"),
    ]);
    db.set_pref(KEY, &serde_json::to_string(&legacy).unwrap())
        .unwrap();
    let history = db.prompt_history(None).unwrap();
    assert_eq!(history.len(), 50);
    assert_eq!(history.first().unwrap(), "prompt 15");
    assert_eq!(history.last().unwrap(), "prompt 64");
    assert_eq!(db.prompt_history(Some("prompt 64")).unwrap(), history);
    db.prompt_history(Some("other Ω界")).unwrap();
    let repeated = db.prompt_history(Some("prompt 64")).unwrap();
    assert_eq!(&repeated[48..], &["other Ω界", "prompt 64"]);
    assert!(
        db.prompt_history(Some(&"x".repeat(MAX_INPUT_BYTES + 1)))
            .is_err()
    );
    assert_eq!(db.prompt_history(None).unwrap(), repeated);
    drop(db);
    let db = Db::open(&root).unwrap();
    assert_eq!(db.prompt_history(None).unwrap(), repeated);
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "model".into(),
        variant: None,
    };
    db.create_bound_session_and_accept_turn_with_reminder(
        "accepted",
        "/project",
        "turn",
        "user Ω界",
        "user Ω界",
        None,
        &model,
        None,
        Some("user Ω界"),
    )
    .unwrap();
    assert_eq!(db.prompt_history(None).unwrap().last().unwrap(), "user Ω界");
    assert_eq!(
        db.read_history_page("accepted", 10, None).unwrap()[0].1,
        "user"
    );
    let before = db.prompt_history(None).unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_prompt_history BEFORE UPDATE ON prefs WHEN NEW.key='tui.prompt_history.v1' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(
        db.create_bound_session_and_accept_turn_with_reminder(
            "refused",
            "/project",
            "rejected",
            "not accepted",
            "not accepted",
            None,
            &model,
            None,
            Some("not accepted")
        )
        .is_err()
    );
    assert!(matches!(
        db.session_meta("refused"),
        Err(StorageError::SessionNotFound)
    ));
    assert_eq!(db.prompt_history(None).unwrap(), before);
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_prompt_history")
        .unwrap();
    db.set_pref(KEY, "broken legacy JSON").unwrap();
    assert!(db.prompt_history(None).unwrap().is_empty());
    assert_eq!(db.get_pref(KEY).unwrap().as_deref(), Some("[]"));
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn vis12_user_shell_admission_is_atomic_without_model_turn_and_recovers_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let db = Db::open(&temp.path().join("data")).unwrap();
    let mut p = crate::shell::jobs::Provenance {
        version: 1,
        session: "user-shell".into(),
        turn: String::new(),
        operation: "user-op".into(),
        location: "/project".into(),
        generation: 7,
        output_limits: Default::default(),
        output_source: "defaults".into(),
        agent: None,
        agent_digest: None,
        model: "captured-model".into(),
        provider: "fixture".into(),
        command: "printf user-effect".into(),
        cwd: "/project".into(),
        selected_shell: "/bin/sh".into(),
    };
    let admission = db.admit_user_shell_job(&p, true, None).unwrap();
    assert_eq!(admission.provenance().operation, "user-op");
    let input_message = db.read_history_page_typed(&p.session, 10, None).unwrap()[0]
        .id
        .clone();
    let input = db
        .user_shell_result(&p.session, &input_message)
        .unwrap()
        .unwrap();
    assert_eq!(input.state, "started");
    assert!(input.input);
    assert!(!input.superseded_input);
    assert_eq!(input.command, p.command);
    assert_eq!(db.prompt_history(None).unwrap(), [p.command.clone()]);
    assert_eq!(
        db.shell_job_phase(&p.session, &p.operation).unwrap(),
        "admitted"
    );
    let conn = db.conn.lock().unwrap();
    let turns: i64 = conn
        .query_row(
            "SELECT count(*) FROM turns WHERE session_id=?1",
            [&p.session],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(turns, 0);
    let model_turn: Option<String> = conn
        .query_row(
            "SELECT turn_id FROM tool_operations WHERE id=?1",
            [&p.operation],
            |r| r.get(0),
        )
        .unwrap();
    assert!(model_turn.is_none());
    drop(conn);
    drop(admission); // No launch token is recreated by recovery.
    let mut result = crate::shell::jobs::Outcome::unknown("fixture terminal result");
    result.state = "completed".into();
    result.exit = Some(0);
    result.stdout = "literal [truncated] body".into();
    result.output_presentation = Some(Box::new(oc_core::tool_output::Presentation::new(
        &result.stdout,
        result.stdout.len() as u64,
        false,
    )));
    db.finish_shell_job(&p.operation, &result).unwrap();
    let conn = db.conn.lock().unwrap();
    let state: String = conn
        .query_row(
            "SELECT state FROM tool_operations WHERE id=?1",
            [&p.operation],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "completed");
    drop(conn);
    let notices = db.deliver_shell_notices().unwrap();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].user_requested);
    let message = oc_core::session::MessageId(notices[0].message_id.clone());
    let projected = db.user_shell_result(&p.session, &message).unwrap().unwrap();
    assert_eq!(projected.command, p.command);
    assert_eq!(projected.operation, p.operation);
    assert_eq!(projected.state, "completed");
    assert!(!projected.superseded_input);
    assert!(!projected.input);
    assert!(
        db.user_shell_result(&p.session, &input_message)
            .unwrap()
            .unwrap()
            .superseded_input
    );
    assert_eq!(
        projected.output.shell.as_ref().unwrap().stdout,
        result.stdout
    );
    assert_eq!(projected.output.shell.as_ref().unwrap().exit, Some(0));
    assert!(!projected.command_limited && projected.output.is_valid());
    assert!(
        db.user_shell_result("foreign-session", &message)
            .unwrap()
            .is_none()
    );
    let raw = db.conversation_history_full(&p.session).unwrap();
    assert_eq!(raw.last().unwrap().2, notices[0].text);
    let spoof = db
        .append_message(&p.session, "user", &notices[0].text)
        .unwrap();
    assert!(
        db.user_shell_result(&p.session, &oc_core::session::MessageId(spoof))
            .unwrap()
            .is_none()
    );
    db.conn.lock().unwrap().execute("UPDATE events SET kind='not_user_shell' WHERE kind='user_shell_admitted' AND payload=?1", [&p.operation]).unwrap();
    assert!(
        db.user_shell_result(&p.session, &message)
            .unwrap()
            .is_none(),
        "NULL turn and exact message linkage alone do not classify origin"
    );
    db.conn.lock().unwrap().execute("UPDATE events SET kind='user_shell_admitted' WHERE kind='not_user_shell' AND payload=?1", [&p.operation]).unwrap();
    assert!(db.deliver_shell_notices().unwrap().is_empty());
    assert_eq!(db.prompt_history(None).unwrap(), [p.command.clone()]);
    for _ in 0..105 {
        db.append_message(&p.session, "assistant", "ordinary later history")
            .unwrap();
    }
    assert!(
        db.read_history_page_typed(&p.session, 100, None)
            .unwrap()
            .iter()
            .all(|row| row.id != message)
    );
    let exact = db
        .read_history_message_typed(&p.session, &message)
        .unwrap()
        .unwrap();
    assert_eq!(exact.id, message);
    assert_eq!(exact.text, notices[0].text);
    db.create_session("foreign-session").unwrap();
    assert!(
        db.read_history_message_typed("foreign-session", &message)
            .unwrap()
            .is_none()
    );
    p.session = "interrupted-shell".into();
    p.operation = "interrupted-op".into();
    p.command = "printf never-replay".into();
    drop(db.admit_user_shell_job(&p, true, None).unwrap());
    db.recover_shell_jobs().unwrap();
    assert_eq!(
        db.shell_job_outcome(&p.session, &p.operation)
            .unwrap()
            .state,
        "unknown"
    );
    let before = db.prompt_history(None).unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_user_history BEFORE UPDATE ON prefs WHEN NEW.key='tui.prompt_history.v1' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    p.session = "refused-shell".into();
    p.operation = "refused-op".into();
    p.command = "printf refused".into();
    assert!(db.admit_user_shell_job(&p, true, None).is_err());
    assert!(matches!(
        db.session_meta(&p.session),
        Err(StorageError::SessionNotFound)
    ));
    assert!(db.shell_job_phase(&p.session, &p.operation).is_err());
    assert_eq!(db.prompt_history(None).unwrap(), before);
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_user_history")
        .unwrap();
    p.session = "user-shell".into();
    p.location = "/foreign".into();
    assert!(db.admit_user_shell_job(&p, false, None).is_err());
    assert_eq!(db.prompt_history(None).unwrap(), before);
    drop(db);
    let db = Db::open(&temp.path().join("data")).unwrap();
    assert_eq!(
        db.user_shell_result("user-shell", &message)
            .unwrap()
            .unwrap(),
        projected
    );
    assert!(
        db.deliver_shell_notices()
            .unwrap()
            .iter()
            .all(|notice| notice.shell_id != "user-op")
    );
}

#[test]
fn direct_user_shell_projection_bounds_streams_without_reinterpreting_prepared_envelope() {
    let root = tempfile::tempdir().unwrap();
    let db = Db::open(root.path()).unwrap();
    let p = crate::shell::jobs::Provenance {
        version: 1,
        session: "bounded-shell".into(),
        turn: String::new(),
        operation: "bounded-op".into(),
        location: "/project".into(),
        generation: 1,
        output_limits: Default::default(),
        output_source: "defaults".into(),
        agent: None,
        agent_digest: None,
        model: "fixture".into(),
        provider: "fixture".into(),
        command: "Ω界".repeat(1000),
        cwd: "/project".into(),
        selected_shell: "/bin/sh".into(),
    };
    drop(db.admit_user_shell_job(&p, true, None).unwrap());
    let mut result = crate::shell::jobs::Outcome::unknown("safe Ω界 diagnostic");
    result.state = "failed".into();
    result.output_prepared = true;
    result.stdout = "model-only output-reference envelope, not stdout".into();
    result.stdout_recent = Some("literal [truncated] Ω界\n".repeat(1000));
    result.stderr_recent = Some("literal [timeout] Ω界\n".repeat(1000));
    db.finish_shell_job(&p.operation, &result).unwrap();
    let notice = db.deliver_shell_notices().unwrap().remove(0);
    let projected = db
        .user_shell_result(&p.session, &oc_core::session::MessageId(notice.message_id))
        .unwrap()
        .unwrap();
    assert!(
        projected.command_limited && projected.command.len() <= oc_core::tool_output::PREVIEW_BYTES
    );
    assert!(p.command.starts_with(&projected.command));
    assert!(projected.output.is_valid());
    let streams = projected.output.shell.unwrap();
    assert!(!streams.stdout.contains("model-only"));
    assert!(streams.stdout.contains("literal [truncated]"));
    assert!(streams.stderr.contains("literal [timeout]"));
    assert!(streams.stdout_limited && streams.stderr_limited);
    assert!(streams.stdout.len() + streams.stderr.len() <= oc_core::tool_output::PREVIEW_BYTES);
    assert!(!streams.timed_out && !streams.cancelled);
}
