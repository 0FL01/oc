use super::*;

#[test]
fn model_shell_frozen_presentation_survives_logical_result_order_reopen_and_fork() {
    use crate::shell::jobs::{Outcome, Provenance};
    use oc_core::queries::TranscriptPart;
    use oc_core::tool_output::{Presentation, Shell};
    for terminal_first in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_bound_session("source", "/p").unwrap();
        let user = db
            .accept_turn(
                "turn",
                "source",
                "unchanged task",
                "unchanged task",
                &oc_core::queries::ModelRef {
                    provider: "fixture".into(),
                    id: "fixture-model".into(),
                    variant: None,
                },
            )
            .unwrap()
            .user_message;
        let input = r#"{"command":"printf original"}"#;
        db.record_turn_tool_intent("op", "source", "turn", "shell", input, "{}")
            .unwrap();
        let provenance = Provenance {
            version: 1,
            session: "source".into(),
            turn: "turn".into(),
            operation: "op".into(),
            location: "/p".into(),
            generation: 9,
            output_limits: Default::default(),
            output_source: "fixture".into(),
            agent: None,
            agent_digest: None,
            model: "fixture-model".into(),
            provider: "fixture".into(),
            command: "printf original".into(),
            cwd: "/p".into(),
            selected_shell: "/bin/sh".into(),
        };
        db.admit_shell_job_mode(&provenance, false).unwrap();
        let body = "literal [stderr]\nfinal frozen flush\n";
        let mut presentation = Presentation::new(body, body.len() as u64, false);
        presentation.shell = Some(Shell {
            background: true,
            process_state: Some("completed".into()),
            stdout: body.into(),
            stderr: String::new(),
            stdout_limited: false,
            stderr_limited: false,
            exit: Some(0),
            signal: None,
            timed_out: false,
            cancelled: false,
        });
        let mut outcome = Outcome::unknown("fixture frozen result");
        outcome.state = "completed".into();
        outcome.exit = Some(0);
        outcome.output_presentation = Some(Box::new(presentation.clone()));
        let handle = r#"{"status":"running","shellID":"op"}"#;
        let mut log = crate::tools::TurnLog::new("turn", "fixture-model", "fixture");
        log.user_message = Some(user);
        log.input = vec![
            crate::provider::InputItem::message(crate::provider::InputRole::User, "unchanged task"),
            crate::provider::InputItem::ProviderOutput(
                serde_json::json!({"type":"function_call","id":"item-op","call_id":"call-op","name":"shell","arguments":input}),
            ),
            crate::provider::InputItem::FunctionCallOutput {
                call_id: "call-op".into(),
                output: handle.into(),
            },
        ];
        log.display_parts = vec![serde_json::json!({"tool":"op"})];
        let log = log.to_json().to_string();
        if terminal_first {
            db.finish_shell_job("op", &outcome).unwrap();
        }
        db.tool_outcome_with_log_and_effects(
            "op",
            "completed",
            handle,
            "turn",
            &log,
            None,
            Some(&Presentation::new(handle, handle.len() as u64, false)),
        )
        .unwrap();
        if !terminal_first {
            db.finish_shell_job("op", &outcome).unwrap();
        }
        let projected = db.turn_presentation("source", "turn").unwrap().unwrap();
        let TranscriptPart::Tool(tool) = &projected.parts[0] else {
            panic!("recorded tool")
        };
        assert_eq!(tool.state, "completed", "logical tool state is unchanged");
        assert_eq!(tool.input.as_deref(), Some(input));
        assert_eq!(
            tool.output.as_deref(),
            Some(handle),
            "RAW tool handle is unchanged"
        );
        assert_eq!(tool.output_presentation.as_deref(), Some(&presentation));
        db.commit_turn(
            "turn",
            "completed",
            Some(&log),
            Some("finished logical await"),
        )
        .unwrap();
        let projected = db.turn_presentation("source", "turn").unwrap().unwrap();
        let raw = db.turn_result("turn").unwrap();
        let mut late = outcome.clone();
        late.output_presentation.as_mut().unwrap().body = "must not replace frozen facts".into();
        db.finish_shell_job("op", &late).unwrap();
        assert_eq!(
            db.turn_presentation("source", "turn").unwrap().unwrap(),
            projected
        );
        // Fork copies display facts, not authority to the original process/file.
        let boundary = db.append_message("source", "user", "next").unwrap();
        let after_boundary = db.turn_presentation("source", "turn").unwrap().unwrap();
        assert_eq!(after_boundary.parts, projected.parts);
        assert_eq!(after_boundary.part_states, projected.part_states);
        assert_eq!(after_boundary.status, projected.status);
        let projected = after_boundary; // Serving revision includes the new user row.
        let fork = db
            .fork_session("source", &boundary, "/p", "fixture", "{}")
            .unwrap();
        assert_eq!(
            db.list_tool_ops(&fork.session.0).unwrap()[0]
                .output_presentation
                .as_deref(),
            Some(&presentation)
        );
        drop(db);
        let db = Db::open(dir.path()).unwrap();
        assert_eq!(db.turn_result("turn").unwrap(), raw);
        assert_eq!(
            db.turn_presentation("source", "turn").unwrap().unwrap(),
            projected
        );
        assert_eq!(
            db.list_tool_ops(&fork.session.0).unwrap()[0]
                .output_presentation
                .as_deref(),
            Some(&presentation)
        );
        // Recovery is unknown, never a logical-handle claim of successful exit.
        let mut pending = provenance;
        pending.operation = "interrupted".into();
        db.record_tool_intent("interrupted", "source", Some("turn"), "shell", input)
            .unwrap();
        db.admit_shell_job_mode(&pending, false).unwrap();
        db.recover_shell_jobs().unwrap();
        let recovered = db
            .list_tool_ops("source")
            .unwrap()
            .into_iter()
            .find(|tool| tool.op == "interrupted")
            .unwrap();
        let facts = recovered
            .output_presentation
            .as_ref()
            .unwrap()
            .shell
            .as_ref()
            .unwrap();
        assert_eq!(facts.process_state.as_deref(), Some("unknown"));
        assert!(facts.exit.is_none() && facts.background);
        assert_eq!(db.turn_result("turn").unwrap(), raw);
        let conn = db.conn.lock().unwrap();
        for version in [serde_json::json!(true), serde_json::json!(1.0)] {
            let mut invalid = serde_json::to_value(&outcome).unwrap();
            invalid["version"] = version;
            let mut invalid_presentation = presentation.clone();
            invalid_presentation.body = "malformed owner must not win".into();
            invalid_presentation.body_bytes = invalid_presentation.body.len() as u64;
            assert!(invalid_presentation.is_valid());
            invalid["output_presentation"] = serde_json::to_value(invalid_presentation).unwrap();
            conn.execute(
                "UPDATE shell_jobs SET outcome=?1 WHERE operation_id='op'",
                [serde_json::to_string(&invalid).unwrap()],
            )
            .unwrap();
            assert_eq!(
                Db::tool_presentation_in(&conn, "source", "op")
                    .unwrap()
                    .as_deref(),
                Some(&presentation),
                "invalid owner falls back to the genuine frozen presentation event"
            );
        }
        conn.execute(
            "UPDATE shell_jobs SET outcome='broken' WHERE operation_id='op'",
            [],
        )
        .unwrap();
        assert_eq!(
            Db::tool_presentation_in(&conn, "source", "op")
                .unwrap()
                .as_deref(),
            Some(&presentation)
        );
        assert!(
            Db::tool_presentation_in(&conn, "foreign", "op")
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn tool21_actual_oversized_question_has_one_cold_payload_and_restart_presentation() {
    use crate::tools::output::{Context, Limits};
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_bound_session("s", "/p").unwrap();
    let model = oc_core::queries::ModelRef {
        provider: "fixture".into(),
        id: "m".into(),
        variant: None,
    };
    let user = db
        .accept_turn("t", "s", "ask", "ask", &model)
        .unwrap()
        .user_message;
    db.create_bound_session("legacy-session", "/p").unwrap();
    db.record_tool_intent("legacy", "legacy-session", None, "fixture_tool", "{}")
        .unwrap();
    let literal = "[tool output: producer literal; not generated guidance]";
    db.record_tool_outcome("legacy", "completed", Some(literal))
        .unwrap();
    let legacy = db.list_tool_ops("legacy-session").unwrap().remove(0);
    assert_eq!(legacy.output.as_deref(), Some(literal));
    assert!(legacy.output_presentation.is_none());
    db.conn.lock().unwrap().execute(
        "INSERT INTO events(session_id,kind,payload) VALUES('legacy-session','tool_output_presentation',?1)",
        [serde_json::json!({"operation":"legacy","presentation":{"unknown_future_shape":true}}).to_string()],
    ).unwrap();
    assert!(
        db.list_tool_ops("legacy-session").unwrap()[0]
            .output_presentation
            .is_none()
    );
    db.record_turn_tool_intent("q", "s", "t", "question", "{}", "{}")
        .unwrap();
    let result = oc_core::question::QuestionResult {
        questions: vec![oc_core::question::QuestionPrompt {
            question: "Choose the actual implementation".into(),
            header: "Decision".into(),
            options: (0..14)
                .map(|i| oc_core::question::QuestionOption {
                    label: format!("choice-{i}"),
                    description: "description ".repeat(330),
                })
                .collect(),
            multiple: false,
        }],
        answers: vec![vec!["exact custom answer".into()]],
    };
    let full = serde_json::to_string(&result).unwrap();
    assert!(full.len() > 51200);
    assert_eq!(
        oc_core::question::QuestionResult::from_output("question", "completed", Some(&full)),
        Some(result.clone())
    );
    let context = Context {
        db: &db,
        operation: "q",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits::default(),
        secrets: vec![],
    };
    let prepared = context.prepare_question(full.clone()).unwrap();
    assert!(!prepared.logging_failed);
    assert!(prepared.text.len() < 51200);
    let presentation = prepared.presentation.as_deref().unwrap();
    assert!(presentation.generated_guidance && presentation.body_limited);
    let before = db.turn_result("t").unwrap();
    let mut invalid = presentation.clone();
    invalid.body = "x".repeat(oc_core::tool_output::PREVIEW_BYTES + 1);
    assert!(
        db.tool_outcome_with_log_and_effects(
            "q",
            "completed",
            "must roll back",
            "t",
            "{}",
            None,
            Some(&invalid),
        )
        .is_err()
    );
    let pending = db.list_tool_ops("s").unwrap().remove(0);
    assert_eq!(pending.state, "started");
    assert!(pending.output.is_none() && pending.output_presentation.is_none());
    assert_eq!(db.turn_result("t").unwrap(), before);
    db.tool_outcome_with_log_and_effects(
        "q",
        "completed",
        &prepared.text,
        "t",
        "{}",
        None,
        Some(presentation),
    )
    .unwrap();
    assert_eq!(
        db.list_tool_ops("s").unwrap()[0]
            .output_presentation
            .as_deref(),
        Some(presentation),
    );
    let presentation_event_bytes: i64 = db.conn.lock().unwrap().query_row(
        "SELECT max(length(CAST(payload AS BLOB))) FROM events WHERE kind='tool_output_presentation'",
        [], |r| r.get(0),
    ).unwrap();
    assert!(presentation_event_bytes < oc_core::tool_output::RECORD_BYTES as i64);
    let resource = db.output_for_operation("q").unwrap().unwrap();
    assert_eq!(resource.bytes as usize, full.len());
    assert_eq!(std::fs::read_to_string(&resource.path).unwrap(), full);
    let event_bytes: i64 = db.conn.lock().unwrap().query_row(
        "SELECT max(length(CAST(payload AS BLOB))) FROM events WHERE kind='tool_output_question'", [], |r|r.get(0)).unwrap();
    assert!(
        event_bytes < 1024,
        "question event duplicates the complete cold payload: {event_bytes}"
    );
    assert_eq!(
        db.list_tool_ops("s").unwrap()[0].question.as_ref(),
        Some(&result)
    );
    let mut log = crate::tools::TurnLog::new("t", "m", "fixture");
    log.user_message = Some(user);
    log.input = vec![
        crate::provider::InputItem::message(crate::provider::InputRole::User, "ask"),
        crate::provider::InputItem::ProviderOutput(
            serde_json::json!({"type":"function_call","id":"item-q","call_id":"call-q","name":"question","arguments":"{}"}),
        ),
        crate::provider::InputItem::FunctionCallOutput {
            call_id: "call-q".into(),
            output: prepared.text.clone(),
        },
    ];
    log.display_parts = vec![serde_json::json!({"tool":"q"})];
    let raw_log = log.to_json().to_string();
    db.commit_turn("t", "completed", Some(&raw_log), Some("answered"))
        .unwrap();
    let raw_log = db.turn_result("t").unwrap().1.unwrap();
    let boundary = db.append_message("s", "user", "next").unwrap();
    let fork = db
        .fork_session("s", &boundary, "/p", "fixture", "{}")
        .unwrap()
        .session
        .0;
    assert_eq!(
        db.list_tool_ops(&fork).unwrap()[0].question.as_ref(),
        Some(&result)
    );
    let fork_fact = db
        .list_tool_ops(&fork)
        .unwrap()
        .remove(0)
        .output_presentation
        .unwrap();
    assert_eq!(fork_fact.body, presentation.body);
    assert!(fork_fact.generated_guidance);
    assert!(fork_fact.capture.as_ref().unwrap().reference.is_none());
    assert!(
        db.open_tool_output(&fork, &resource.path).is_err(),
        "presentation fork is not a model filesystem grant"
    );
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM tool_output_resources", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.turn_result("t").unwrap().1.as_deref(),
        Some(raw_log.as_str())
    );
    drop(db);
    let db = Db::open(dir.path()).unwrap();
    assert_eq!(
        db.list_tool_ops_page("s", 8, None).unwrap()[0]
            .output_presentation
            .as_deref(),
        Some(presentation),
    );
    assert_eq!(
        db.list_tool_ops_page("s", 8, None).unwrap()[0]
            .question
            .as_ref(),
        Some(&result)
    );
    assert_eq!(
        db.list_tool_ops(&fork).unwrap()[0].question.as_ref(),
        Some(&result)
    );
    db.expire_tool_outputs(timestamp() + TTL + 1).unwrap();
    let expired = db
        .list_tool_ops("s")
        .unwrap()
        .remove(0)
        .output_presentation
        .unwrap();
    assert_eq!(expired.body, presentation.body);
    assert_eq!(
        expired.capture.as_ref().unwrap().state,
        CaptureState::Expired
    );
    assert!(expired.capture.as_ref().unwrap().reference.is_none());
    assert!(db.list_tool_ops("s").unwrap()[0].question.is_none());
    assert!(db.list_tool_ops(&fork).unwrap()[0].question.is_none());
    assert_eq!(
        db.turn_result("t").unwrap().1.as_deref(),
        Some(raw_log.as_str())
    );
}

fn fixture(quota: u64) -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(dir.path(), quota).unwrap();
    db.create_session("s").unwrap();
    db.create_session("foreign").unwrap();
    db.record_tool_intent("op", "s", None, "skill", "{}")
        .unwrap();
    (dir, db)
}

#[test]
fn tool21_distant_stream_continuation_lineage_restart_and_no_recursive_copy() {
    let (dir, db) = fixture(CAP);
    let mut writer = db
        .begin_tool_output("op", "s", "/project", 7, "pinned", vec![])
        .unwrap();
    let line = "é ordinary line with padding 0123456789012345678901234567890123456789\n";
    for _ in 0..156 {
        writer.append(&line.repeat(128)).unwrap();
    }
    writer.append(&line.repeat(32)).unwrap();
    for _ in 0..122 {
        writer.append(&"\n".repeat(8192)).unwrap();
    }
    writer.append(&"\n".repeat(577)).unwrap();
    writer.append("DISTANT_SENTINEL 二\nnext\n").unwrap();
    let resource = writer.finish(false).unwrap();
    assert!(resource.bytes > 1024 * 1024);
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let call = crate::tools::ToolCall {
        id: "read-call".into(),
        name: "read".into(),
        arguments: serde_json::json!({"path":resource.path,"offset":1020002,"limit":1}),
    };
    let (text, source) = db
        .artifact_call(
            "s",
            &call,
            crate::config::ToolOutputLimits::default(),
            &cancel,
        )
        .unwrap();
    assert!(text.contains("DISTANT_SENTINEL"));
    assert!(text.contains("next_offset Some(1020003)"));
    assert_eq!(source.bytes, resource.bytes);
    let call = crate::tools::ToolCall {
        id: "grep-call".into(),
        name: "grep".into(),
        arguments: serde_json::json!({"path":resource.path,"pattern":"DISTANT_SENTINEL","literal":true}),
    };
    let (text, _) = db
        .artifact_call(
            "s",
            &call,
            crate::config::ToolOutputLimits::default(),
            &cancel,
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["matches"][0]["line"], 1020002);
    assert_eq!(value["source"]["validated"], true);
    db.create_child_session("s", "child", None, None, None)
        .unwrap();
    assert!(db.open_tool_output("child", &resource.path).is_err());
    let count: i64 = db
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM tool_output_resources", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    db.record_tool_intent("interrupted", "s", None, "skill", "{}")
        .unwrap();
    let mut writer = db
        .begin_tool_output("interrupted", "s", "/project", 8, "pinned", vec![])
        .unwrap();
    writer.append("durable prefix\n").unwrap();
    drop(writer);
    let incomplete = db.output_for_operation("interrupted").unwrap().unwrap();
    drop(db);
    let reopened = Db::open(dir.path()).unwrap();
    assert!(reopened.open_tool_output("s", &resource.path).is_ok());
    assert_eq!(
        reopened
            .open_tool_output("s", &incomplete.path)
            .unwrap()
            .resource
            .state,
        CaptureState::Interrupted
    );
}

#[test]
fn tool21_publication_fault_never_promises_unregistered_renamed_file() {
    let (_dir, db) = fixture(CAP);
    let mut writer = db
        .begin_tool_output("op", "s", "/project", 1, "default", vec![])
        .unwrap();
    writer.append("durable prefix\n").unwrap();
    db.conn.lock().unwrap().execute_batch("CREATE TEMP TRIGGER fail_publication BEFORE UPDATE OF name ON tool_output_resources BEGIN SELECT RAISE(ABORT,'directed publication failure'); END;").unwrap();
    assert!(writer.finish(false).is_err());
    let resource = db.output_for_operation("op").unwrap().unwrap();
    assert!(db.open_tool_output("s", &resource.path).is_err());
    assert_eq!(db.output_disk_bytes().unwrap(), 15);
    assert!(db.write_blob(&vec![0; CAP as usize]).is_err());
}

#[test]
fn tool21_explicit_family_delete_respects_writer_and_orphan_reader_leases() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_bound_session("s", "/project").unwrap();
    db.record_tool_intent("op", "s", None, "skill", "{}")
        .unwrap();
    let mut writer = db
        .begin_tool_output("op", "s", "/project", 1, "source", vec![])
        .unwrap();
    writer.append("prefix\n").unwrap();
    assert!(db.delete_root_family("s", "/project").is_err());
    let resource = writer.finish(false).unwrap();
    let reader = db.open_tool_output("s", &resource.path).unwrap();
    db.delete_root_family("s", "/project").unwrap();
    db.expire_tool_outputs(timestamp() + TTL + 1).unwrap();
    assert!(std::path::Path::new(&resource.path).exists());
    drop(reader);
    db.expire_tool_outputs(timestamp() + TTL + 1).unwrap();
    assert!(!std::path::Path::new(&resource.path).exists());
}

#[test]
fn tool21_concurrent_blob_and_registered_writers_share_exclusive_quota() {
    let (_dir, db) = fixture(8192);
    db.record_tool_intent("op2", "s", None, "skill", "{}")
        .unwrap();
    let mut a = db
        .begin_tool_output("op", "s", "/project", 1, "default", vec![])
        .unwrap();
    let mut b = db
        .begin_tool_output("op2", "s", "/project", 1, "default", vec![])
        .unwrap();
    let blob = db.shared_handle();
    let threads = [
        std::thread::spawn(move || {
            a.append(&"a".repeat(8192)).unwrap();
            a.finish(false).unwrap()
        }),
        std::thread::spawn(move || {
            b.append(&"b".repeat(8192)).unwrap();
            b.finish(false).unwrap()
        }),
    ];
    let blob = std::thread::spawn(move || blob.write_blob(&[0; 8192]));
    let resources: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    let _ = blob.join().unwrap();
    assert_eq!(db.resource_used().unwrap(), 8192);
    assert!(resources.iter().all(|r| r.bytes == 0 || r.bytes == 8192));
    assert!(resources.iter().any(|r| r.state == CaptureState::Quota));
}

#[test]
fn tool21_registered_stream_cap_redaction_leases_and_shared_quota() {
    let (_dir, db) = fixture(CAP + 100);
    let mut writer = db
        .begin_tool_output(
            "op",
            "s",
            "/project",
            1,
            "source",
            vec!["secret-sentinel".into()],
        )
        .unwrap();
    writer.append("hello secret-").unwrap();
    writer.append("sentinel\n").unwrap();
    for _ in 0..2049 {
        writer.append(&"x".repeat(8192)).unwrap();
    }
    let resource = writer.finish(false).unwrap();
    assert_eq!(resource.state, CaptureState::ArtifactCap);
    assert_eq!(resource.bytes, CAP);
    let mut reader = db.open_tool_output("s", &resource.path).unwrap();
    assert!(
        reader
            .byte_page(0, 100)
            .unwrap()
            .0
            .starts_with("hello [redacted]\n")
    );
    assert!(db.open_tool_output("foreign", &resource.path).is_err());
    assert!(db.write_blob(&[0; 101]).is_err());
    assert_eq!(db.expire_tool_outputs(timestamp() + TTL + 1).unwrap(), 0);
    drop(reader);
    assert_eq!(db.expire_tool_outputs(timestamp() + TTL + 1).unwrap(), 1);
    assert!(db.open_tool_output("s", &resource.path).is_err());
}

#[test]
fn tool21_registered_identity_swap_interruption_and_blob_orphan_quota() {
    let (dir, db) = fixture(100);
    fs::write(dir.path().join("blobs/orphan"), [0; 90]).unwrap();
    let mut writer = db
        .begin_tool_output("op", "s", "/project", 1, "default", vec![])
        .unwrap();
    writer.append("abcdefghijk").unwrap();
    let resource = writer.finish(false).unwrap();
    assert_eq!(resource.state, CaptureState::Quota);
    assert_eq!(resource.bytes, 10);
    let replacement = dir.path().join("replacement");
    fs::write(&replacement, b"1234567890").unwrap();
    fs::rename(&replacement, &resource.path).unwrap();
    assert!(db.open_tool_output("s", &resource.path).is_err());
    db.record_tool_intent("op2", "s", None, "skill", "{}")
        .unwrap();
    let writer = db
        .begin_tool_output("op2", "s", "/project", 2, "default", vec![])
        .unwrap();
    drop(writer);
    assert_eq!(
        db.output_for_operation("op2").unwrap().unwrap().state,
        CaptureState::Interrupted
    );
}
