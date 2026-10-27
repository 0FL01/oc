use super::*;

#[test]
fn confirmed_diff_capture_keeps_utf8_and_omitted_tail_for_patch_and_edit() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_session("s").unwrap();
    let files = crate::files::Files::new(project.path(), data.path()).unwrap();
    let content = format!(
        "{} secret-sentinel\nmultiline-first\nmultiline-last\n{}DIFF-TAIL\n",
        "界".repeat(600),
        "line\n".repeat(125)
    );
    for tool in ["apply_patch", "edit"] {
        db.record_tool_intent(tool, "s", None, tool, "{}").unwrap();
        let context = Context {
            db: &db,
            operation: tool,
            session: "s",
            location: "/p",
            generation: 1,
            source: "fixture",
            limits: Limits::default(),
            secrets: vec![
                "secret-sentinel".into(),
                "multiline-first\nmultiline-last".into(),
            ],
        };
        let mut capture = DiffCapture::new(&context);
        let (output, effects) = if tool == "apply_patch" {
            let patch = format!(
                "*** Begin Patch\n*** Add File: patch.txt\n+{}*** End Patch",
                content.replace('\n', "\n+").trim_end_matches('+')
            );
            let (result, effects) = crate::patch::apply_patch_with_approved_preimage(
                &files,
                &patch,
                &crate::patch::AllowAll,
                None,
                &mut |text| capture.append(text),
            );
            (crate::tools::patch_outcome(result), effects)
        } else {
            std::fs::write(project.path().join("edit.txt"), "before\n").unwrap();
            let args =
                serde_json::json!({"path":"edit.txt","oldString":"before\n","newString":content});
            let input = crate::patch::mutation::Input::parse(tool, &args).unwrap();
            let (result, effects) = crate::patch::mutation::execute(
                &files,
                &input,
                &crate::patch::AllowAll,
                None,
                &mut |text| capture.append(text),
            );
            (result.unwrap().to_string(), effects)
        };
        assert!(effects.truncated && effects.files[0].hunks[0].lines[0].text.len() <= 1024);
        let prepared = capture.finish(output);
        assert!(!prepared.logging_failed, "{}", prepared.text);
        let facts = prepared.presentation.unwrap();
        assert!(facts.is_valid());
        let captured = facts.capture.unwrap();
        assert_eq!(captured.state, CaptureState::Complete);
        let path = captured.reference.unwrap();
        assert!(prepared.text.contains(&path));
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(&format!("+{} [redacted]\n", "界".repeat(600))));
        assert!(saved.ends_with("+DIFF-TAIL\n") && !saved.contains("secret-sentinel"));
        assert!(!saved.contains("multiline-first") && !saved.contains("multiline-last"));
        assert_eq!(captured.retained_bytes, saved.len() as u64);
        assert_eq!(captured.retained_bytes, captured.admitted_bytes);
        let call = crate::tools::ToolCall {
            id: "continuation".into(),
            name: "read".into(),
            arguments: serde_json::json!({"path":path,"offset":125,"limit":10}),
        };
        let (page, resource) = db
            .artifact_call(
                "s",
                &call,
                Limits::default(),
                &std::sync::atomic::AtomicBool::new(false),
            )
            .unwrap();
        assert!(page.contains("+DIFF-TAIL"));
        assert_eq!(resource.state, CaptureState::Complete);
        let call = crate::tools::ToolCall {
            name: "grep".into(),
            arguments: serde_json::json!({"path":path,"pattern":"DIFF-TAIL","literal":true}),
            ..call
        };
        assert!(
            db.artifact_call(
                "s",
                &call,
                Limits::default(),
                &std::sync::atomic::AtomicBool::new(false)
            )
            .unwrap()
            .0
            .contains("DIFF-TAIL")
        );
    }
}

#[test]
fn confirmed_diff_capture_reports_quota_without_repeating_the_mutation() {
    let project = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let db = Db::open_with_quota(data.path(), 128).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("patch", "s", None, "apply_patch", "{}")
        .unwrap();
    let files = crate::files::Files::new(project.path(), data.path()).unwrap();
    let context = Context {
        db: &db,
        operation: "patch",
        session: "s",
        location: "/p",
        generation: 1,
        source: "fixture",
        limits: Limits::default(),
        secrets: vec![],
    };
    let mut capture = DiffCapture::new(&context);
    let content = "x".repeat(1000);
    let patch = format!("*** Begin Patch\n*** Add File: actual\n+{content}\n*** End Patch");
    let (result, effects) = crate::patch::apply_patch_with_approved_preimage(
        &files,
        &patch,
        &crate::patch::AllowAll,
        None,
        &mut |text| capture.append(text),
    );
    assert!(result.is_ok() && !effects.truncated);
    let prepared = capture.finish(crate::tools::patch_outcome(result));
    assert!(prepared.logging_failed);
    let captured = prepared.presentation.unwrap().capture.unwrap();
    assert_eq!(captured.state, CaptureState::Quota);
    assert_eq!(captured.retained_bytes, 128);
    assert!(captured.admitted_bytes > captured.retained_bytes);
    assert!(prepared.text.contains("capture Quota") && !prepared.text.contains("capture Complete"));
    assert_eq!(
        std::fs::read_to_string(project.path().join("actual")).unwrap(),
        format!("{content}\n")
    );
    assert_eq!(
        std::fs::read_dir(data.path().join("tool-output"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn confirmed_diff_capture_failure_cannot_recapture_status_as_a_complete_diff() {
    let data = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("patch", "s", None, "apply_patch", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "patch",
        session: "s",
        location: "/p",
        generation: 1,
        source: "fixture",
        limits: Limits {
            max_bytes: 8,
            max_lines: 1,
        },
        secrets: vec![],
    };
    let mut capture = DiffCapture::new(&context);
    // A transient artifact-directory identity failure clears before status
    // preparation. That must not start a second capture for the status text.
    std::fs::rename(data.path().join("tool-output"), data.path().join("held")).unwrap();
    capture.append(&"x".repeat(SERVED_CAP));
    std::fs::rename(data.path().join("held"), data.path().join("tool-output")).unwrap();
    let prepared = capture.finish("actual successful mutation status".into());
    assert!(prepared.logging_failed);
    let captured = prepared.presentation.unwrap().capture.unwrap();
    assert_eq!(captured.state, CaptureState::Io);
    assert!(captured.reference.is_none());
    assert!(
        prepared.text.contains("no usable path") && !prepared.text.contains("capture Complete")
    );
    assert!(db.output_for_operation("patch").unwrap().is_none());
    assert_eq!(
        std::fs::read_dir(data.path().join("tool-output"))
            .unwrap()
            .count(),
        0
    );
    // Failure at finish, after a durable extent exists, must publish the same
    // interrupted descriptor as replay rather than zero/no-reference facts.
    db.record_tool_intent("finish", "s", None, "apply_patch", "{}")
        .unwrap();
    let context = Context {
        operation: "finish",
        ..context
    };
    let mut capture = DiffCapture::new(&context);
    capture.append(&"y".repeat(SERVED_CAP));
    std::fs::rename(data.path().join("tool-output"), data.path().join("held")).unwrap();
    let prepared = capture.finish("actual successful mutation status".into());
    std::fs::rename(data.path().join("held"), data.path().join("tool-output")).unwrap();
    assert!(prepared.logging_failed);
    let captured = prepared.presentation.unwrap().capture.unwrap();
    assert_eq!(captured.state, CaptureState::Interrupted);
    assert_eq!(
        captured,
        db.output_for_operation("finish")
            .unwrap()
            .unwrap()
            .presentation_capture()
    );
    assert_eq!(
        std::fs::read(captured.reference.unwrap()).unwrap(),
        vec![b'y'; SERVED_CAP]
    );
}

#[test]
fn structured_clipping_never_reassigns_match_text_and_keeps_diagnostic_facts() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    db.record_tool_intent("grep", "s", None, "grep", "{}")
        .unwrap();
    let context = Context {
        db: &db,
        operation: "grep",
        session: "s",
        location: "/p",
        generation: 1,
        source: "fixture",
        limits: Limits {
            max_bytes: 8,
            max_lines: 2,
        },
        secrets: vec!["secret-crossing-boundary".into()],
    };
    let result = context.prepare_envelope(serde_json::json!({"matches":[{"path":"a","line":1,"text":"AAAA"},{"path":"b","line":2,"text":"BBBB"}]}).to_string(), false, None);
    let value: serde_json::Value = serde_json::from_str(&result.text).unwrap();
    assert_eq!(value["matches"][0]["text"], "AAAA");
    assert_eq!(value["matches"][1]["text"], "BBBB");
    assert_eq!(value["matches"][1]["path"], "b");
    let mut tiny = context.clone();
    tiny.limits.max_bytes = 1;
    db.record_tool_intent("diag", "s", None, "grep", "{}")
        .unwrap();
    tiny.operation = "diag";
    let diagnostic = crate::tools::diagnostic::Diagnostic::new(
        "grep",
        "budget_exhausted",
        "Scan stopped secret-crossing-boundary",
    )
    .detail(serde_json::json!({"budget":"scan_bytes","bytes":16777216,"limit":16777216}))
    .hint("Narrow path or include");
    let result = tiny.prepare_envelope(diagnostic.output("failed"), false, None);
    assert!(!result.logging_failed);
    assert!(!result.text.contains("secret-crossing"));
    let value: serde_json::Value = serde_json::from_str(&result.text).unwrap();
    assert_eq!(value["error"]["code"], "budget_exhausted");
    assert_eq!(value["error"]["detail"]["bytes"], 16777216);
    assert_eq!(value["error"]["message"], "Scan stopped [redacted]");
}

#[test]
fn write_preview_redacts_complete_content_before_byte_and_row_limits() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(dir.path()).unwrap();
    db.create_session("s").unwrap();
    let context = Context {
        db: &db,
        operation: "write-op",
        session: "s",
        location: "/p",
        generation: 9,
        source: "fixture",
        limits: Limits::default(),
        secrets: vec!["secret-sentinel-crossing-the-preview-boundary".into()],
    };
    for content in [
        "".to_owned(),
        "café\n世界\n".into(),
        format!("{}{}", "x".repeat(2044), context.secrets[0]),
    ] {
        let original = content.clone();
        let mut presentation = oc_core::tool_output::Presentation::new("actual result", 13, false);
        let original_body = presentation.body.clone();
        context.attach_write(&mut presentation, "owned.rs", &content);
        let write = presentation.write.as_ref().unwrap();
        assert!(presentation.is_valid());
        assert_eq!(presentation.body, original_body);
        assert_eq!(content, original);
        assert!(write.content.len() <= oc_core::tool_output::PREVIEW_BYTES);
        assert!(!write.content.contains("secret-sentinel"));
        if content.len() < 2048 {
            assert_eq!(write.content, content);
            assert!(!write.content_limited);
        } else {
            assert!(write.content_limited);
            assert!(write.content.ends_with("[red"));
        }
        assert!(
            serde_json::to_vec(&presentation).unwrap().len() <= oc_core::tool_output::RECORD_BYTES
        );
    }
    let mut limited = context.clone();
    limited.limits = Limits {
        max_bytes: 2048,
        max_lines: 2,
    };
    let mut presentation = oc_core::tool_output::Presentation::new("done", 4, false);
    limited.attach_write(&mut presentation, "café\nfile.rs", "first\nsecond\nthird");
    let write = presentation.write.as_ref().unwrap();
    assert_eq!(write.path, "café file.rs");
    assert_eq!(write.content, "first\nsecond");
    assert!(write.content_limited);
    assert_eq!(write.content_bytes, 18);
    assert!(presentation.retained_bytes() >= write.content.len() + write.path.len());
    let mut invalid = presentation.clone();
    invalid.write.as_mut().unwrap().path.push('\n');
    assert!(!invalid.is_valid());
    invalid = presentation.clone();
    invalid.write.as_mut().unwrap().content = "x\n".repeat(120);
    assert!(!invalid.is_valid());
    let mut legacy: serde_json::Value = serde_json::to_value(&presentation).unwrap();
    legacy.as_object_mut().unwrap().remove("write");
    assert!(
        serde_json::from_value::<oc_core::tool_output::Presentation>(legacy)
            .unwrap()
            .write
            .is_none()
    );
    assert!(db.output_for_operation("write-op").unwrap().is_none());
}

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
    let facts = serde_json::json!({"content":[{"type":"text","text":"first secret-sentinel\n[Part preview truncated]\n"},{"type":"text","text":"x".repeat(400000)}],"isError":false,"structuredContent":{"status":"failed","exit":17,"detail":"y".repeat(3000)},"_meta":{"truncated":false}});
    let mut output = crate::mcp_result::McpToolOutput::from_stored(facts).unwrap();
    assert!(!output.prepare_common(&context).unwrap());
    assert_eq!(output.facts()["isError"], false);
    assert_eq!(output.facts()["structuredContent"]["exit"], 17);
    assert!(output.texts().iter().map(String::len).sum::<usize>() < SERVED_CAP);
    assert!(output.texts().iter().any(|s| s.contains("read(path=")));
    let presentation = output.presentation().unwrap();
    assert!(presentation.generated_guidance);
    assert!(presentation.body_limited && presentation.is_valid());
    assert_eq!(
        presentation.body,
        "first [redacted]\n[Part preview truncated]\n"
    );
    assert_eq!(
        presentation.capture.as_ref().unwrap().state,
        CaptureState::Complete
    );
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
    let presentation = result.presentation.as_ref().unwrap();
    assert!(presentation.generated_guidance && presentation.is_valid());
    assert_eq!(presentation.producer_limited, Some(true));
    let body: serde_json::Value = serde_json::from_str(&presentation.body).unwrap();
    assert_eq!(body["status"], "failed");
    assert_eq!(body["exit"], 17);
    assert!(body["truncated"].as_bool().unwrap());
    let resource = db.output_for_operation("op").unwrap().unwrap();
    assert_eq!(resource.state, CaptureState::ProducerLimited);
    assert!(result.text.len() < SERVED_CAP);
    let text = db.read_session_tool_output("s", "op", 0, 100).unwrap().0;
    assert!(text.starts_with('x'));
    let text = db.read_session_tool_output("s", "op", 3000, 100).unwrap().0;
    assert!(text.contains("failure"));
}
