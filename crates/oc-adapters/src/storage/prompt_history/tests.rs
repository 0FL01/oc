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
