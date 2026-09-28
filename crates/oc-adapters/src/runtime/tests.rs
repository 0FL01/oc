//! Identity and VIS38 owner regressions; existing compaction tests stay separate.

use super::*;

#[cfg(test)]
mod identity_tests {
    #[test]
    fn turn_ids_do_not_repeat_when_clock_repeats_or_rolls_back() {
        let mut ids = std::collections::BTreeSet::new();
        for timestamp in [123, 123, 122, 123] {
            for session in ["a", "b", "a"] {
                assert!(ids.insert(super::next_turn_id(session, timestamp)));
            }
        }
    }
}

#[cfg(test)]
mod vis38_review_tests {
    use super::*;
    use crate::storage::Db;

    fn runtime<'a>(db: &'a Db, project: &std::path::Path, data: &std::path::Path) -> Runtime<'a> {
        let config = Generation {
            permissions: ["read", "compress"]
                .into_iter()
                .map(|s| (s.into(), Permission::Allow))
                .collect(),
            ..Default::default()
        };
        Runtime::new(
            db,
            "work",
            config,
            ProtectedGlobs { patterns: vec![] },
            crate::files::Files::new(project, data).unwrap(),
            crate::shell::Shell::new(project).unwrap(),
            BTreeMap::new(),
            ToolRoots {
                project: project.into(),
                data: data.into(),
            },
            None,
            false,
            DcpConfig {
                turn_protection: false,
                protected_tools: vec![],
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn seed(db: &Db, turn: &str, user: &str, assistant: &str) -> (String, String) {
        let accepted = db
            .accept_turn(
                turn,
                "s",
                user,
                user,
                &oc_core::queries::ModelRef {
                    provider: "test".into(),
                    id: "m".into(),
                    variant: None,
                },
            )
            .unwrap();
        let mut log = TurnLog::new(turn, "m", "test");
        log.user_message = Some(accepted.user_message.clone());
        log.input.push(InputItem::message(InputRole::User, user));
        log.input.push(InputItem::ProviderOutput(serde_json::json!({"type":"reasoning","id":format!("reason-{turn}"),"summary":[{"type":"summary_text","text":"retained public reasoning"}],"encrypted_content":"opaque"})));
        log.input.push(InputItem::ProviderOutput(serde_json::json!({"type":"message","id":format!("answer-{turn}"),"role":"assistant","content":[{"type":"output_text","text":assistant}]})));
        db.commit_turn(
            turn,
            "completed",
            Some(&log.to_json().to_string()),
            Some(assistant),
        )
        .unwrap();
        let (_, raw) = db.turn_result(turn).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        (
            accepted.user_message,
            value["assistant_message"].as_str().unwrap().into(),
        )
    }

    fn args(start: &str, end: &str, summary: &str) -> serde_json::Value {
        serde_json::json!({"topic":"wire review","content":[{"startId":start,"endId":end,"summary":summary}]})
    }

    #[test]
    fn manual_actual_wire_no_gain_and_zero_item_recompression() {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        let runtime = runtime(&db, project.path(), data.path());
        runtime.create_session("s").unwrap();
        let (first, last) = seed(
            &db,
            "seed",
            "u",
            &"canonical assistant retained ".repeat(512),
        );
        seed(&db, "tail", "tail", "done");
        // Raw assistant text would make this look profitable, but the real
        // canonical output survives compression and the summary adds content.
        assert!(
            runtime
                .run_compress(
                    "s",
                    &args(&first, &last, &"summary ".repeat(64)),
                    &ProtectedSpec::default()
                )
                .is_err()
        );
        let failed = db.list_tool_ops("s").unwrap();
        assert_eq!(failed.last().unwrap().state, "no_gain");
        assert!(db.dcp_accounting("s").unwrap().is_none());
        assert!(
            db.dcp_run("s", &failed.last().unwrap().op)
                .unwrap()
                .is_none()
        );
        let (first, last) = seed(
            &db,
            "large",
            &"removable user ".repeat(512),
            "retained answer",
        );
        seed(&db, "later", "open tail", "done");
        let raw = db.read_history_full("s").unwrap();
        let first_report = runtime
            .run_compress(
                "s",
                &args(&first, &last, &"summary details ".repeat(32)),
                &ProtectedSpec::default(),
            )
            .unwrap();
        let second = runtime
            .run_compress(
                "s",
                &args(&first_report.blocks[0], &first_report.blocks[0], "short"),
                &ProtectedSpec::default(),
            )
            .unwrap();
        let run = db
            .list_tool_ops("s")
            .unwrap()
            .into_iter()
            .filter_map(|op| op.dcp)
            .next_back()
            .unwrap();
        assert_eq!((run.new_messages, run.new_tools, run.removed), (0, 0, 0));
        assert!(run.net_saved > 0);
        assert_eq!(run.block_ids, second.blocks);
        let context = runtime.active_projection("s").unwrap();
        let wire = runtime
            .wire_history(
                "s",
                &context.projected,
                &context.blocks,
                "m",
                "test",
                None,
                context.after_seq,
            )
            .unwrap();
        let projection = db.dcp_tool_projection_for_input("s", &wire).unwrap();
        let actual = dcp_continuation(&wire, &[], &projection);
        assert_eq!(
            runtime
                .dcp_projection_estimate(
                    "s",
                    &context.projected,
                    &context.blocks,
                    context.after_seq
                )
                .unwrap(),
            Some(
                dcp_contents(&actual)
                    .iter()
                    .map(|text| oc_core::dcp_view::estimate_content(text))
                    .sum()
            )
        );
        assert!(
            actual
                .iter()
                .any(|i| matches!(i,InputItem::ProviderOutput(v) if v["id"]=="reason-large"))
        );
        assert!(
            actual
                .iter()
                .any(|i| matches!(i,InputItem::ProviderOutput(v) if v["id"]=="answer-large"))
        );
        assert_eq!(db.read_history_full("s").unwrap(), raw);
    }

    #[tokio::test]
    async fn next_request_projects_current_turn_duplicate_read_and_preserves_journal() {
        use std::io::{BufRead, BufReader, Read, Write};
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("note.txt"), "actual read result").unwrap();
        let db = Db::open(data.path()).unwrap();
        let runtime = runtime(&db, project.path(), data.path());
        runtime.create_session("s").unwrap();
        let (first, last) = seed(
            &db,
            "seed",
            &"long user context ".repeat(512),
            "seed completed",
        );
        let call = |id: &str, call_id: &str, name: &str, args: serde_json::Value| serde_json::json!({"type":"function_call","id":id,"call_id":call_id,"name":name,"arguments":args.to_string(),"status":"completed"});
        let read_args = serde_json::json!({"path":"note.txt"});
        let script = vec![
            vec![call("read-1", "read-old", "read", read_args.clone())],
            vec![
                call("read-2", "read-new", "read", read_args),
                call(
                    "compress-1",
                    "compress",
                    "compress",
                    args(&first, &last, "short"),
                ),
            ],
            vec![
                serde_json::json!({"type":"message","id":"finished","role":"assistant","content":[{"type":"output_text","text":"done"}]}),
            ],
        ];
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/v1", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let captured = requests.clone();
        let server = std::thread::spawn(move || {
            for output in script {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                captured
                    .lock()
                    .unwrap()
                    .push(serde_json::from_slice(&body).unwrap());
                let response = format!(
                    "data: {}\n\n",
                    serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output}})
                );
                reader.get_mut().write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{response}").as_bytes()).unwrap();
            }
        });
        let catalog = models::ModelCatalog {
            provider: "test".into(),
            models: [(
                "m".into(),
                serde_json::json!({"limit":{"context":1_000_000,"output":100_000}}),
            )]
            .into_iter()
            .collect(),
        };
        let cancel = AtomicBool::new(false);
        let report = runtime
            .run_turn(TurnParams {
                session: "s".into(),
                prompt: "read twice then compress and continue".into(),
                invocation: None,
                catalog: &catalog,
                model_id: "m".into(),
                variant: None,
                max_output: 1000,
                provider: ResponsesConfig {
                    headers: BTreeMap::new(),
                    set_cache_key: true,
                    base_url: base,
                    api_key: "fixture".into(),
                    timeout: Some(false),
                    chunk_timeout_ms: 5000,
                    connect_timeout: Duration::from_secs(5),
                    allow_private: true,
                },
                cancel: &cancel,
                max_rounds: 4,
            })
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(report.status, TurnStatus::Completed);
        let (_, raw) = db.turn_result(&report.turn_id).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw.unwrap()).unwrap();
        let journal = TurnLog::from_json(&value).unwrap();
        assert_eq!(journal.input.iter().filter(|i|matches!(i,InputItem::ProviderOutput(v) if v["type"]=="function_call"&&v["name"]=="read")).count(),2);
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 3);
        for kind in ["function_call", "function_call_output"] {
            assert_eq!(
                requests[2]["input"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|v| v["type"] == kind
                        && matches!(v["call_id"].as_str(), Some("read-old" | "read-new")))
                    .count(),
                1,
                "{kind} reappeared from raw current-turn journal"
            );
        }
        let op = db
            .list_tool_ops("s")
            .unwrap()
            .into_iter()
            .find(|op| op.name == "compress")
            .unwrap();
        let snapshot = op.dcp.unwrap();
        assert_eq!(snapshot.new_tools, 1);
        let removed_read = journal
            .input
            .iter()
            .find_map(|item| match item {
                InputItem::FunctionCallOutput { call_id, output } if call_id == "read-old" => {
                    Some(output)
                }
                _ => None,
            })
            .unwrap();
        let seed_text = "long user context ".repeat(512);
        let removed = oc_core::dcp_view::estimate_content(&seed_text)
            + oc_core::dcp_view::estimate_content(
                &serde_json::json!({"path":"note.txt"}).to_string(),
            )
            + oc_core::dcp_view::estimate_content(removed_read);
        assert_eq!(snapshot.removed, removed);
        assert_eq!(
            snapshot.net_saved,
            removed
                - oc_core::dcp_view::estimate_content(&format!(
                    "[compressed {}] short",
                    snapshot.block_ids[0]
                ))
        );
        assert!(requests[0]["input"].to_string().contains(&seed_text));
        let identities = dcp_call_identities(&[], Some(&journal)).unwrap();
        assert_eq!(
            identities[&("read-old".into(), 0)],
            (report.turn_id.clone(), "read-old".into(), 0)
        );
        assert!(snapshot.net_saved > 0);
        let (boundary, _) = seed(&db, "boundary", "fork boundary", "done");
        let fork = db
            .fork_session("s", &boundary, "work", "test", "{}")
            .unwrap();
        let root = &fork.session.0;
        assert_eq!(
            db.dcp_accounting(root).unwrap(),
            Some(snapshot.cumulative.clone())
        );
        let context = runtime.active_projection(root).unwrap();
        let published = runtime.current.read().unwrap().clone();
        let lane = runtime.primary_lane(&published);
        let wire = runtime
            .wire_history(
                root,
                &context.projected,
                &context.blocks,
                "m",
                "test",
                lane.agent_digest.as_deref(),
                context.after_seq,
            )
            .unwrap();
        let projection = db.dcp_tool_projection_for_input(root, &wire).unwrap();
        let wire = dcp_continuation(&wire, &[], &projection);
        assert_eq!(wire.iter().filter(|i|matches!(i,InputItem::ProviderOutput(v) if v["type"]=="function_call"&&v["name"]=="read")).count(),1);
    }

    #[test]
    fn recompression_keeps_inherited_verbatim_protection_without_covered_text() {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let db = Db::open(data.path()).unwrap();
        let runtime = runtime(&db, project.path(), data.path());
        runtime.create_session("s").unwrap();
        let protected = "owner verbatim 🌍";
        let first = db.append_message("s", "user", protected).unwrap();
        let last = db
            .append_message("s", "assistant", &"removable plain answer ".repeat(512))
            .unwrap();
        db.append_message("s", "user", "unfinished tail").unwrap();
        let spec = ProtectedSpec {
            protect_user_messages: true,
            ..Default::default()
        };
        let report = runtime
            .run_compress(
                "s",
                &args(&first, &last, &"authored summary ".repeat(32)),
                &spec,
            )
            .unwrap();
        let report = runtime
            .run_compress(
                "s",
                &args(&report.blocks[0], &report.blocks[0], "short"),
                &ProtectedSpec::default(),
            )
            .unwrap();
        let context = runtime.active_projection("s").unwrap();
        let summary = context
            .projected
            .iter()
            .find(|r| r.0 == report.blocks[0])
            .unwrap();
        assert_eq!(summary.2.matches(protected).count(), 1);
        let run = db
            .list_tool_ops("s")
            .unwrap()
            .into_iter()
            .filter_map(|op| op.dcp)
            .next_back()
            .unwrap();
        assert_eq!((run.new_messages, run.removed), (0, 0));
    }
}
