//! Model-visible DCP affordances: built-in donor prompts, compress IDs,
//! reminder placement and in-task step compression.
use super::*;

fn runtime_with_dcp<'a>(
    db: &'a Db,
    project: &std::path::Path,
    config: Generation,
    dcp: DcpConfig,
) -> Runtime<'a> {
    Runtime::new(
        db,
        "work",
        config,
        ProtectedGlobs { patterns: vec![] },
        crate::files::Files::new(project, db.root()).unwrap(),
        crate::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        ToolRoots {
            project: project.into(),
            data: db.root().into(),
        },
        None,
        false,
        dcp,
    )
    .unwrap()
}

fn texts(request: &serde_json::Value) -> Vec<String> {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|item| item["content"].as_array().into_iter().flatten())
        .filter_map(|part| part["text"].as_str().map(str::to_owned))
        .collect()
}

fn step_markers(request: &serde_json::Value) -> Vec<(usize, String)> {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let text = item["content"].as_array()?.first()?["text"].as_str()?;
            let id = text
                .strip_prefix("<dcp-message-id>tassistant-")?
                .strip_suffix("</dcp-message-id>")?;
            (item["role"] == "developer").then(|| (index, format!("tassistant-{id}")))
        })
        .collect()
}

fn output_index(request: &serde_json::Value, call: &str) -> usize {
    request["input"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["type"] == "function_call_output" && item["call_id"] == call)
        .unwrap_or_else(|| panic!("{call} output"))
}

fn completed(output: serde_json::Value) -> String {
    format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed","response":{
            "status":"completed","output":output,"usage":{"input_tokens":1000,"output_tokens":10}
        }})
    )
}

fn read(call: &str, path: &str) -> serde_json::Value {
    serde_json::json!({"type":"function_call","id":format!("{call}-item"),"call_id":call,"name":"read","arguments":serde_json::json!({"path":path}).to_string()})
}

#[tokio::test]
async fn running_task_steps_compress_in_place_with_visible_ids() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let data = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        for (name, body) in [("a.txt", "A_CONTENT"), ("b.txt", "B_CONTENT"), ("c.txt", "C_CONTENT"), ("d.txt", "D_CONTENT")] {
            std::fs::write(project.path().join(name), body).unwrap();
        }
        let db = Db::open(data.path()).unwrap();
        db.create_bound_session("s", "work").unwrap();
        let mut generation = Generation::default();
        for name in ["read", "compress"] {
            generation.permissions.insert(name.into(), Permission::Allow);
        }
        let runtime = runtime(&db, project.path(), generation);
        let mut models = catalog();
        models.models.insert("m".into(), serde_json::json!({"limit":{"context":64000,"output":8000}}));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let provider = config(&listener);
        let peer = tokio::spawn(async move {
            let mut seen = Vec::new();
            for index in 0..7 {
                let (mut socket, request) = read_request(&listener).await;
                let output = match index {
                    0 => {
                        let all = texts(&request).join("\n");
                        assert!(all.contains("THE PHILOSOPHY OF COMPRESS"), "donor system guidance");
                        let compress = request["tools"].as_array().unwrap().iter().find(|tool| tool["name"] == "compress").unwrap();
                        assert!(compress["description"].as_str().unwrap().starts_with("Collapse a range in the conversation"));
                        let task = request["input"].as_array().unwrap().iter()
                            .find(|item| item["role"] == "user" && item.to_string().contains("TASK_TEXT"))
                            .expect("task message");
                        assert!(task.to_string().contains("<dcp-message-id>"), "the running task carries its ID");
                        serde_json::json!([read("r1", "a.txt")])
                    }
                    1 => serde_json::json!([read("r2", "b.txt")]),
                    2 => serde_json::json!([read("r3", "c.txt")]),
                    3 => {
                        let markers = step_markers(&request);
                        assert_eq!(markers.len(), 3, "one marker per closed step");
                        for (marker, call) in markers.iter().zip(["r1", "r2", "r3"]) {
                            assert_eq!(marker.0, output_index(&request, call) + 1, "marker follows its step results");
                        }
                        serde_json::json!([
                            {"type":"function_call","id":"bad-item","call_id":"bad","name":"compress","arguments":serde_json::json!({
                                "topic":"mixed","content":[{"startId":markers[0].1,"endId":"m0001","summary":"x"}]
                            }).to_string()},
                            {"type":"function_call","id":"steps-item","call_id":"steps","name":"compress","arguments":serde_json::json!({
                                "topic":"read a and b","content":[{"startId":markers[0].1,"endId":markers[1].1,"summary":"STEP_SUMMARY_FACT a and b were read"}]
                            }).to_string()}
                        ])
                    }
                    4 => {
                        let all = request["input"].to_string();
                        assert!(all.contains("STEP_SUMMARY_FACT"), "summary replaces the steps");
                        assert!(!all.contains("A_CONTENT") && !all.contains("B_CONTENT"), "compressed results left HOT");
                        assert!(all.contains("C_CONTENT") && all.contains("TASK_TEXT"), "task and later step stay");
                        assert_eq!(step_markers(&request).len(), 1, "only the remaining step is addressable");
                        serde_json::json!([read("r4", "d.txt")])
                    }
                    5 => {
                        // A second range over the committed HOT keeps the first summary.
                        let markers = step_markers(&request);
                        assert_eq!(markers.len(), 2);
                        serde_json::json!([{"type":"function_call","id":"steps2-item","call_id":"steps2","name":"compress","arguments":serde_json::json!({
                            "topic":"read c and d","content":[{"startId":markers[0].1,"endId":markers[1].1,"summary":"SECOND_SUMMARY_FACT c and d were read"}]
                        }).to_string()}])
                    }
                    _ => {
                        let all = request["input"].to_string();
                        assert!(all.contains("STEP_SUMMARY_FACT") && all.contains("SECOND_SUMMARY_FACT"));
                        assert!(all.find("STEP_SUMMARY_FACT") < all.find("SECOND_SUMMARY_FACT"), "summaries keep chronology");
                        for gone in ["A_CONTENT", "B_CONTENT", "C_CONTENT", "D_CONTENT"] {
                            assert!(!all.contains(gone), "{gone} left HOT");
                        }
                        assert!(all.contains("TASK_TEXT"));
                        assert!(step_markers(&request).is_empty());
                        serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"DONE"}]}])
                    }
                };
                seen.push(request);
                respond(&mut socket, &completed(output)).await;
            }
            seen
        });
        let cancel = AtomicBool::new(false);
        let report = runtime.run_turn(TurnParams {
            session: "s".into(), prompt: "TASK_TEXT read the files".into(), invocation: None,
            catalog: &models, model_id: "m".into(), variant: None, max_output: 8000,
            provider, cancel: &cancel,
        }).await.unwrap();
        peer.await.unwrap();
        assert_eq!(report.status, TurnStatus::Completed, "{:?}", report.diagnostic);
        let operations = db.list_tool_ops("s").unwrap();
        let compress = operations.iter().filter(|op| op.name == "compress").collect::<Vec<_>>();
        assert_eq!(compress.len(), 3);
        assert_eq!(compress[0].state, "failed");
        assert!(compress[0].output.as_deref().unwrap().contains("cannot mix running-task steps"));
        assert_eq!(compress[1].state, "completed");
        assert!(compress[1].output.as_deref().unwrap().contains("\"scope\":\"steps\""));
        assert_eq!(compress[2].state, "completed");
        assert_eq!(operations.iter().filter(|op| op.name == "read").count(), 4, "no read replayed");
    })
    .await
    .expect("step compression finishes within 20 seconds");
}

#[tokio::test]
async fn reminder_uses_donor_body_after_the_newest_context() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "OLD WORK");
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("compress".into(), Permission::Allow);
    let dcp = DcpConfig {
        min_context: 1,
        min_context_percent: None,
        max_context: 2,
        max_context_percent: None,
        ..DcpConfig::default()
    };
    let runtime = runtime_with_dcp(&db, project.path(), generation, dcp);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let peer = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        respond(&mut socket, &sse("done")).await;
        request
    });
    let catalog = catalog();
    let cancel = AtomicBool::new(false);
    runtime
        .run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1024,
            provider,
            cancel: &cancel,
        })
        .await
        .unwrap();
    let request = peer.await.unwrap();
    let input = request["input"].as_array().unwrap();
    let last = input.last().unwrap();
    assert_eq!(
        last["role"], "developer",
        "the reminder follows the newest context"
    );
    let reminder = last["content"][0]["text"].as_str().unwrap();
    assert!(
        reminder.contains("DCP SOFT CONTEXT THRESHOLD PASSED"),
        "{reminder}"
    );
    assert!(reminder.contains("Compressed block context:"));
    assert!(reminder.trim_end().ends_with("</dcp-system-reminder>"));
    assert!(
        !request["input"].to_string().contains("DCP reminder ("),
        "no internal lane label"
    );
    let old = input
        .iter()
        .find(|item| item["role"] == "user" && item.to_string().contains("OLD WORK"))
        .unwrap();
    assert!(
        old.to_string().contains("<dcp-message-id>m"),
        "history user messages carry IDs"
    );
}

#[tokio::test]
async fn denied_compress_sends_no_ids_or_guidance() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    db.create_bound_session("s", "work").unwrap();
    seed(&db, "old", "OLD WORK");
    let mut generation = Generation::default();
    generation
        .permissions
        .insert("compress".into(), Permission::Deny);
    let runtime = runtime(&db, project.path(), generation);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = config(&listener);
    let peer = tokio::spawn(async move {
        let (mut socket, request) = read_request(&listener).await;
        respond(&mut socket, &sse("done")).await;
        request
    });
    let catalog = catalog();
    let cancel = AtomicBool::new(false);
    runtime
        .run_turn(TurnParams {
            session: "s".into(),
            prompt: "continue".into(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 1024,
            provider,
            cancel: &cancel,
        })
        .await
        .unwrap();
    let request = peer.await.unwrap().to_string();
    assert!(!request.contains("<dcp-message-id>"));
    assert!(!request.contains("THE PHILOSOPHY OF COMPRESS"));
}
