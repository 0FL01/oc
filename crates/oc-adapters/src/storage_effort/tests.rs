use super::*;

#[test]
fn prm01_idle_effort_boundary_skips_nonprojected_model_switch_notice() {
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_session("root").unwrap();
    db.commit_session_model_choice(
        &[],
        "root",
        &serde_json::json!({"effort_update":{"effort":"low","previous":null}}).to_string(),
    )
    .unwrap();
    let notice = db
        .append_message("root", "model_switch", "metadata only")
        .unwrap();
    assert!(
        db.effort_facts("root", 0).unwrap()[0]
            .before_message
            .is_none()
    );
    let user = db
        .append_message("root", "user", "next actual prompt")
        .unwrap();
    let facts = db.effort_facts("root", 0).unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].before_message.as_deref(), Some(user.as_str()));
    assert_ne!(facts[0].before_message.as_deref(), Some(notice.as_str()));
    assert!(
        matches!(&facts[0].item, InputItem::EffortUpdate { effort: Some(effort), previous: None, .. } if effort == "low")
    );
    let original = db.read_history_full("root").unwrap();
    drop(db);
    let db = Db::open(data.path()).unwrap();
    assert_eq!(
        db.effort_facts("root", 0).unwrap()[0]
            .before_message
            .as_deref(),
        Some(user.as_str())
    );
    assert_eq!(db.read_history_full("root").unwrap(), original);
}
