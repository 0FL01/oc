use super::*;

#[test]
fn tool21_stream_redactor_publishes_benign_live_suffix_and_protects_overlapping_secrets() {
    let mut redactor = StreamRedactor::new(vec![
        "possible-secret".into(),
        "possible-secret-suffix".into(),
    ]);
    assert_eq!(redactor.push("LIVE-FG 二\n", false), "LIVE-FG 二\n");
    assert_eq!(redactor.push("prefix possible-", false), "prefix ");
    assert_eq!(redactor.push("secret", false), "");
    assert_eq!(
        redactor.push("-suffix benign live\n", false),
        "[redacted] benign live\n"
    );
    assert_eq!(redactor.push("possible-", false), "");
    assert_eq!(
        redactor.push("", true),
        "possible-",
        "terminal nonsecret partial remains truthful"
    );
    let mut redactor = StreamRedactor::new(vec!["abc".into(), "bcde".into()]);
    assert_eq!(redactor.push("abcd", false), "");
    assert_eq!(redactor.push("e\n", false), "[redacted]de\n");
    let mut redactor = StreamRedactor::new(vec!["a".repeat(16), format!("{}b", "a".repeat(16))]);
    for _ in 0..100 {
        let published = redactor.push(&"a".repeat(8192), false);
        assert!(!published.contains(&"a".repeat(16)));
        assert!(
            redactor.pending.len() <= 32,
            "repetitive secret prefixes cannot grow carry"
        );
    }
}

#[test]
fn tool21_donor_count_utf8_head_tail_and_tiny_limits() {
    assert_eq!(lines("a\nb\n"), 2);
    assert_eq!(lines(""), 0);
    assert_eq!(lines("a\r\nb"), 2);
    let ordinary = "x".repeat(60000);
    assert_eq!(
        preview(
            &ordinary,
            Limits {
                max_lines: 2000,
                max_bytes: SERVED_CAP
            },
            false
        ),
        (&*ordinary, false)
    );
    let limits = Limits {
        max_lines: 2,
        max_bytes: 6,
    };
    assert_eq!(preview("é\n二\nx", limits, false), ("é\n", true));
    assert_eq!(preview("x\né\n二", limits, true), ("é\n二", true));
    assert_eq!(
        preview(
            "二".repeat(100).as_str(),
            Limits {
                max_lines: 1,
                max_bytes: 1
            },
            false
        )
        .0,
        ""
    );
    let lone = "二".repeat(100);
    assert_eq!(
        preview(
            &lone,
            Limits {
                max_lines: 1,
                max_bytes: 7
            },
            true
        )
        .0,
        "二二"
    );
}

#[test]
fn tool21_mcp_joined_text_metadata_not_bypass_structured_error_facts_survive() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("mcp", "s", None, "mcp__fixture__large", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "mcp",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits {
            max_lines: 2,
            max_bytes: 64,
        },
        secrets: vec!["secret-sentinel".into()],
    };
    let facts = serde_json::json!({"content":[{"type":"text","text":"first secret-sentinel\n"},{"type":"text","text":"x".repeat(400000)}],"isError":false,"structuredContent":{"status":"failed","exit":17,"detail":"y".repeat(3000)},"_meta":{"truncated":false}});
    let mut output = crate::mcp_result::McpToolOutput::from_stored(facts).unwrap();
    assert!(!output.prepare_common(&context).unwrap());
    assert_eq!(output.facts()["isError"], false);
    assert_eq!(output.facts()["structuredContent"]["exit"], 17);
    assert!(output.texts().iter().map(String::len).sum::<usize>() < SERVED_CAP);
    assert!(output.texts().iter().any(|s| s.contains("read(path=")));
    let resource = db.output_for_operation("mcp").unwrap().unwrap();
    let mut lease = db.open_tool_output("s", &resource.path).unwrap();
    let prefix = lease.byte_page(0, 100).unwrap().0;
    assert!(prefix.starts_with("first [redacted]\n"));
    assert!(!prefix.contains("secret-sentinel"));
    assert!(matches!(
        crate::mcp_result::McpToolOutput::from_stored(
            serde_json::json!({"content":[],"isError":true})
        ),
        Err(crate::mcp_result::ResultError::Failed(_))
    ));
}

#[test]
fn tool21_question_control_presentation_survives_common_limits() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("q", "s", None, "question", "{}")
        .unwrap();
    let result = oc_core::question::QuestionResult {
        questions: vec![oc_core::question::QuestionPrompt {
            question: "Choose a direction".into(),
            header: "Decision".into(),
            options: vec![oc_core::question::QuestionOption {
                label: "yes".into(),
                description: "Proceed".into(),
            }],
            multiple: false,
        }],
        answers: vec![vec!["yes".into()]],
    };
    let context = Context {
        db: &db,
        operation: "q",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits {
            max_lines: 1,
            max_bytes: 1,
        },
        secrets: vec![],
    };
    let prepared = context
        .prepare_question(serde_json::to_string(&result).unwrap())
        .unwrap();
    assert!(!prepared.logging_failed);
    let value: serde_json::Value = serde_json::from_str(&prepared.text).unwrap();
    assert_eq!(value["status"], "answered");
    db.record_tool_outcome("q", "completed", Some(&prepared.text))
        .unwrap();
    let view = db.list_tool_ops("s").unwrap();
    assert_eq!(view[0].question.as_ref(), Some(&result));
}

#[test]
fn tool21_structured_only_mcp_text_joins_cold_capture_and_escaped_hot_facts_are_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("mcp", "s", None, "mcp__fixture__large", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "mcp",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits::default(),
        secrets: vec![],
    };
    let facts = serde_json::json!({"content":[{"type":"text","text":"ordinary"}],"isError":false,"structuredContent":{"status":"failed","exit":17,"answer":format!("{}DISTANT_STRUCTURED_SENTINEL","\"".repeat(100000))},"_meta":{"truncated":false}});
    let mut output = crate::mcp_result::McpToolOutput::from_stored(facts).unwrap();
    assert!(!output.prepare_common(&context).unwrap());
    assert_eq!(output.facts()["structuredContent"]["status"], "failed");
    assert_eq!(output.facts()["structuredContent"]["exit"], 17);
    assert!(output.facts().to_string().len() <= SERVED_CAP);
    assert!(output.texts().iter().any(|s| s.contains("read(path=")));
    let resource = db.output_for_operation("mcp").unwrap().unwrap();
    assert!(resource.bytes > 100000);
    let mut lease = db.open_tool_output("s", &resource.path).unwrap();
    assert!(
        lease
            .byte_page((resource.bytes - 30) as usize, 30)
            .unwrap()
            .0
            .contains("DISTANT_STRUCTURED_SENTINEL")
    );
}

#[test]
fn tool21_redaction_small_json_and_escaped_prose_stay_valid_and_served() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("op", "s", None, "write", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "op",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits::default(),
        secrets: vec!["secret-sentinel".into()],
    };
    let small = context.prepare_envelope(
        r#"{"status":"failed","message":"secret-sentinel"}"#.into(),
        false,
        None,
    );
    assert!(!small.text.contains("secret-sentinel"));
    assert!(small.text.contains("[redacted]"));
    let result = context.prepare_envelope(
        serde_json::json!({"status":"failed","exit":17,"message":"\"".repeat(100000)}).to_string(),
        false,
        None,
    );
    assert!(result.text.len() <= SERVED_CAP);
    assert!(!result.logging_failed);
    let value: serde_json::Value = serde_json::from_str(&result.text).unwrap();
    assert_eq!(value["exit"], 17);
    assert!(result.text.contains("read(path="));
}

#[test]
fn tool21_prepared_envelope_retains_controls_no_metadata_bypass() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("op", "s", None, "shell", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "op",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits {
            max_lines: 2,
            max_bytes: 64,
        },
        secrets: vec![],
    };
    let result=context.prepare_envelope(serde_json::json!({"status":"failed","exit":17,"truncated":true,"stdout":"x".repeat(3000),"stderr":"failure"}).to_string(),true,None);
    let value: serde_json::Value = serde_json::from_str(&result.text).unwrap();
    assert_eq!(value["status"], "failed");
    assert_eq!(value["exit"], 17);
    let resource = db.output_for_operation("op").unwrap().unwrap();
    assert_eq!(resource.state, CaptureState::ProducerLimited);
    assert!(result.text.len() < SERVED_CAP);
    let text = db.read_session_tool_output("s", "op", 0, 100).unwrap().0;
    assert!(text.starts_with('x'));
    let text = db.read_session_tool_output("s", "op", 3000, 100).unwrap().0;
    assert!(text.contains("failure"));
}
