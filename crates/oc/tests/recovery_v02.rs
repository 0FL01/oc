//! V02: real binary/config/provider followed by a fresh application owner.
use oc_core::{domain::SessionId, queries::TranscriptPart};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};

#[tokio::test]
async fn binary_restart_projects_real_metadata_and_parts() {
    let first = scenario(
        "Named",
        [true, true],
        Some((0, 0)),
        Some("durable tool evidence"),
        1,
    )
    .await;
    let second = scenario(
        "Changed",
        [false, false],
        None,
        Some("different evidence"),
        1,
    )
    .await;
    assert_ne!(
        first, second,
        "fixture mutation must change the visible frame"
    );
    scenario(
        "Partial",
        [true, false],
        Some((2, 5)),
        Some(&format!("{}NEEDLE-AFTER-PREVIEW", "é".repeat(3000))),
        400,
    )
    .await;
}

async fn scenario(
    label: &str,
    usage_rounds: [bool; 2],
    price: Option<(u64, u64)>,
    probe: Option<&str>,
    patch_repeat: usize,
) -> String {
    let known = usage_rounds.iter().all(|v| *v);
    let model_name = format!("{label} model");
    let provider_name = format!("{label} provider");
    let title = format!("{label} generated title");
    let agent = label.to_lowercase();
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    let home = root.path().join("home");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&config).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let mut configuration = serde_json::json!({
        "model":"fixture/route/model/with/slashes", "default_agent":agent,
        "agent":{agent.clone():{"description":"Review","mode":"primary","prompt":"Review carefully"},
                 "title":{"mode":"subagent","prompt":"Generate a short session title. Output only the title."}},
        "permissions":{"read":"allow","apply_patch":"allow"},
        "provider":{"fixture":{"name":provider_name, "npm":"@ai-sdk/openai",
          "options":{"baseURL":format!("http://{address}/v1"),"apiKey":"fixture-secret"},
           "models":{"route/model/with/slashes":{"name":model_name,"cost":price.map(|(input,output)|serde_json::json!({"input":input,"output":output})),"limit":{"context":32768,"output":4096}}}}}
    });
    if label == "Changed" {
        configuration["agent"]
            .as_object_mut()
            .unwrap()
            .remove("title");
    } else {
        configuration["agent"]["title"]["model"] = "fixture/route/model/with/slashes#brief".into();
        configuration["provider"]["fixture"]["models"]["route/model/with/slashes"]["variants"] =
            serde_json::json!({"brief":{"reasoningEffort":"low"}});
    }
    std::fs::write(config.join("opencode.json"), configuration.to_string()).unwrap();
    if let Some(probe) = probe {
        std::fs::write(project.join("probe.txt"), probe).unwrap();
    }
    let generated_title = title.clone();
    let title_prompt = if label == "Changed" {
        "Generate a short session title from the user's request. Output only the title, in at most 100 characters."
    } else {
        "Generate a short session title. Output only the title."
    };
    let expected_probe = probe.map(str::to_string);
    let server = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let mut main_rounds = 0;
        let mut titles = 0;
        for request_index in 0..3 {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(pair) => break pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "missing request {request_index}"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0; 4096];
            let end = loop {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(p) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break p + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..end]);
            let length: usize = headers
                .lines()
                .find_map(|l| {
                    let (k, v) = l.split_once(':')?;
                    k.eq_ignore_ascii_case("content-length")
                        .then(|| v.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < end + length {
                let n = socket.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            let request: serde_json::Value =
                serde_json::from_slice(&bytes[end..end + length]).unwrap();
            assert_eq!(request["model"], "route/model/with/slashes");
            let title_request = request["tools"] == serde_json::json!([])
                && request["max_output_tokens"] == 256
                && request["input"][0]["role"] == "developer"
                && request["input"][0]["content"][0]["text"] == title_prompt
                && request["input"].as_array().is_some_and(|items| {
                    items.len() == 2
                        && items[1]["role"] == "user"
                        && items[1]["content"][0]["text"] == "Inspect probe.txt"
                });
            if !title_request {
                assert!(main_rounds < 2, "unexpected extra main turn: {request}");
            }
            if !title_request && main_rounds == 1 {
                let input = request["input"].as_array().unwrap();
                for (call, name) in [("c1", "read"), ("c2", "apply_patch")] {
                    let calls: Vec<_> = input
                        .iter()
                        .filter(|item| item["type"] == "function_call" && item["call_id"] == call)
                        .collect();
                    let results: Vec<_> = input
                        .iter()
                        .filter(|item| {
                            item["type"] == "function_call_output" && item["call_id"] == call
                        })
                        .collect();
                    assert_eq!(calls.len(), 1, "exact call graph");
                    assert_eq!(calls[0]["name"], name);
                    assert_eq!(results.len(), 1, "one real result per call");
                    let result = results[0]["output"].as_str().unwrap();
                    if call == "c1" {
                        match &expected_probe {
                            Some(probe) => assert!(result.contains(probe), "{result}"),
                            None => assert!(result.starts_with("error:"), "{result}"),
                        }
                    } else {
                        assert!(
                            result.contains("replay.txt")
                                && result.contains("hash_after=")
                                && !result.starts_with("error:"),
                            "{result}"
                        );
                    }
                }
            }
            let (text, output) = if title_request {
                titles += 1;
                assert_eq!(titles, 1, "one real title request");
                (
                    "",
                    serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":generated_title}]}]),
                )
            } else {
                let round = main_rounds;
                main_rounds += 1;
                match round {
                    0 => (
                        "",
                        serde_json::json!([
                            {"type":"function_call","id":"i1","call_id":"c1","name":"read","arguments":"{\"path\":\"probe.txt\"}"},
                            {"type":"function_call","id":"i2","call_id":"c2","name":"apply_patch","arguments":serde_json::json!({"patchText":format!("*** Begin Patch\n*** Add File: replay.txt\n+{}\n*** End Patch", "payload ".repeat(patch_repeat))}).to_string()}
                        ]),
                    ),
                    1 => (
                        "Final durable answer",
                        serde_json::json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Final durable answer"}]}]),
                    ),
                    _ => unreachable!(),
                }
            };
            let mut sse = String::new();
            if !title_request && main_rounds == 1 {
                sse.push_str("data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"Public thought\"}\n\ndata: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"reasoning\",\"id\":\"r1\",\"encrypted_content\":\"opaque-secret\",\"summary\":[]}}\n\n");
            }
            if !title_request && main_rounds == 2 {
                sse.push_str("data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"Second thought\"}\n\n");
            }
            sse.push_str(&format!(
                "data: {}\n\n",
                serde_json::json!({"type":"response.output_text.delta","delta":text})
            ));
            sse.push_str(&format!("data: {}\n\n",serde_json::json!({"type":"response.completed","response":{"status":"completed","output":output,"usage":if !title_request && usage_rounds[main_rounds - 1] {serde_json::json!({"input_tokens":321,"output_tokens":17})}else{serde_json::Value::Null}}})));
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",sse.len()).unwrap();
            let split = sse.find("\n\n").unwrap() + 2;
            socket.write_all(&sse.as_bytes()[..split]).unwrap();
            socket.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(150));
            socket.write_all(&sse.as_bytes()[split..]).unwrap();
        }
        assert_eq!(
            (main_rounds, titles),
            (2, 1),
            "exact call graph and title cardinality"
        );
    });
    if label == "Changed" {
        binary_tui_restart(&home, &project, "", &title, &model_name, true);
    } else {
        let output = Command::new(env!("CARGO_BIN_EXE_oc"))
            .env_clear()
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .current_dir(&project)
            .args(["run", "Inspect probe.txt"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    server.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("replay.txt")).unwrap(),
        format!("{}\n", "payload ".repeat(patch_repeat))
    );
    let env = BTreeMap::from([
        ("HOME".into(), home.display().to_string()),
        (
            "XDG_CONFIG_HOME".into(),
            home.join("config").display().to_string(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &home.join("data/oc"), env)
            .await
            .unwrap();
    let catalog = app.catalog().await.unwrap();
    assert_eq!(
        catalog.auto_accept,
        oc_core::queries::AutoAcceptState::Unsupported,
        "allow rules do not implement upstream session autoaccept"
    );
    assert_eq!(catalog.models[0].display_name, model_name);
    assert_eq!(catalog.models[0].provider_name, provider_name);
    assert!(catalog.models[0].context_known && catalog.models[0].output_known);
    assert_eq!(
        catalog.models[0].price.as_ref().map(|p| p.is_free()),
        price.map(|(input, output)| input == 0 && output == 0)
    );
    let sessions = app.list_sessions().await.unwrap();
    let page = app
        .history_page(sessions[0].clone(), None, None, 100)
        .await
        .unwrap();
    assert_eq!(page.title.as_deref(), Some(title.as_str()));
    let turn = page.rows.iter().find_map(|row| row.turn.as_ref()).unwrap();
    if label == "Partial" {
        let read = turn
            .parts
            .iter()
            .find_map(|part| match part {
                TranscriptPart::Tool(op) if op.name == "read" => Some(op),
                _ => None,
            })
            .unwrap();
        assert!(
            read.output_truncated
                && !read
                    .output
                    .as_deref()
                    .unwrap_or("")
                    .contains("NEEDLE-AFTER-PREVIEW"),
            "truncated={} bytes={} preview_bytes={}",
            read.output_truncated,
            read.output_bytes,
            read.output.as_ref().map_or(0, String::len)
        );
        let first = app
            .tool_output_page(sessions[0].clone(), read.op.clone(), 0, 1024)
            .await
            .unwrap();
        assert!(
            app.tool_output_page(sessions[0].clone(), read.op.clone(), i64::MAX as usize, 240)
                .await
                .is_err(),
            "max signed offset cannot be incremented by SQLite indexing"
        );
        assert!(
            app.tool_output_page(sessions[0].clone(), read.op.clone(), 1, 4)
                .await
                .is_err(),
            "mid-character offsets must not return zero progress"
        );
        assert!(
            app.tool_output_page(
                sessions[0].clone(),
                read.op.clone(),
                read.output_bytes as usize + 1,
                240
            )
            .await
            .is_err(),
            "past-end offsets must be rejected"
        );
        assert_eq!(first.total_bytes, read.output_bytes);
        assert!(first.text.len() <= 1024);
        let mut rest = first.text;
        let mut next = first.next_offset;
        while let Some(offset) = next {
            let part = app
                .tool_output_page(sessions[0].clone(), read.op.clone(), offset as usize, 1024)
                .await
                .unwrap();
            assert_eq!(part.total_bytes, read.output_bytes);
            rest.push_str(&part.text);
            next = part.next_offset;
        }
        assert!(rest.contains("NEEDLE-AFTER-PREVIEW"));
        assert_eq!(rest.len() as i64, read.output_bytes);
    }
    assert_eq!(turn.agent.as_deref(), Some(agent.as_str()));
    assert_eq!(turn.model_label, model_name);
    assert_eq!(turn.usage, known.then_some((321, 34)));
    assert_eq!(
        turn.part_states
            .iter()
            .map(|p| p.sequence)
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4]
    );
    assert!(
        matches!(&turn.parts[..],[TranscriptPart::Reasoning{text:first,..},TranscriptPart::Tool(read),TranscriptPart::Tool(patch),TranscriptPart::Reasoning{text:second,..},TranscriptPart::Text(answer)] if first=="Public thought" && read.name=="read" && patch.name=="apply_patch" && second=="Second thought" && answer=="Final durable answer")
    );
    assert!(
        turn.parts
            .iter()
            .any(|p| matches!(p,TranscriptPart::Reasoning{text,..} if text=="Public thought"))
    );
    assert!(
        turn.parts.iter().any(
            |p| matches!(p,TranscriptPart::Tool(op) if op.name=="read" && op.state==if probe.is_some(){"completed"}else{"failed"} && op.output.as_ref().is_some_and(|s| !s.is_empty()))
        ),
        "{turn:?}"
    );
    let mut state = oc_tui::app::TuiState::new(app.clone(), SessionId(sessions[0].0.clone()));
    state.apply_catalog(catalog);
    state.attach_page(&page);
    if label == "Partial" {
        let ops = app
            .tool_ops_page(sessions[0].clone(), None, 20)
            .await
            .unwrap();
        state.apply_cards(oc_tui::history::cards_from_rows(&ops.rows), ops.has_older);
        for ch in "/cards".chars() {
            state.handle_key(oc_tui::events::KeyAction::Char(ch)).await;
        }
        state.handle_key(oc_tui::events::KeyAction::Enter).await;
        for _ in 0..ops.rows.iter().position(|row| row.name == "read").unwrap() {
            state.handle_panel_key(oc_tui::events::KeyAction::Down);
        }
        let choice = state.handle_panel_key(oc_tui::events::KeyAction::Enter);
        let Some(oc_tui::app::PanelIntent::LoadCardOutput { op, offset: 0 }) = choice.intent else {
            panic!(
                "cards must route real owner continuation: {:?}",
                choice.intent
            )
        };
        let mut offset = 0;
        let mut visible = String::new();
        for _ in 0..100 {
            let detail = app
                .tool_output_page(sessions[0].clone(), op.clone(), offset, 240)
                .await
                .unwrap();
            let next = detail.next_offset;
            state.apply_card_output(op.clone(), offset, detail);
            // The actual rendered dialog, not an off-screen producer, must
            // expose every owner byte window before the next-page intent.
            let _ = oc_tui::views::render_test(&state, 120, 40);
            let mut lines = oc_tui::views::panel_lines(&state);
            assert!(lines.len() <= 17 && lines[0].contains("operation "));
            visible.push_str(&lines[1..lines.len() - 1].join(""));
            for _ in 0..240 {
                if lines.last().is_some_and(|row| {
                    row.contains("enter next page") || row.contains("end of result")
                }) {
                    break;
                }
                assert!(
                    state
                        .handle_panel_key(oc_tui::events::KeyAction::Enter)
                        .intent
                        .is_none()
                );
                state.handle_panel_key(oc_tui::events::KeyAction::Down);
                let frame = oc_tui::views::render_test(&state, 120, 40).join("\n");
                lines = oc_tui::views::panel_lines(&state);
                let last = &lines[lines.len() - 2];
                assert!(frame.contains(last), "scroll row must actually be painted");
                visible.push_str(last);
            }
            assert!(
                lines.last().is_some_and(
                    |row| row.contains("enter next page") || row.contains("end of result")
                ),
                "all page rows must be reachable"
            );
            let choice = state.handle_panel_key(oc_tui::events::KeyAction::Enter);
            if let Some(next) = next {
                assert_eq!(
                    choice.intent,
                    Some(oc_tui::app::PanelIntent::LoadCardOutput {
                        op: op.clone(),
                        offset: next as usize
                    })
                );
                offset = next as usize;
            } else {
                assert!(choice.intent.is_none());
                break;
            }
        }
        assert!(
            visible.contains("NEEDLE-AFTER-PREVIEW"),
            "the TUI must page to the actual tail"
        );
        state.close_panel();
        // Same Location, different real session: cached A cards must not be
        // visible and the owner API must refuse A's operation from B.
        let local_b = SessionId::new("local-b-v06b").unwrap();
        app.create_session(local_b.clone()).await.unwrap();
        state.set_session(local_b.clone());
        for ch in "/cards".chars() {
            state.handle_key(oc_tui::events::KeyAction::Char(ch)).await;
        }
        let load = state.handle_key(oc_tui::events::KeyAction::Enter).await;
        assert_eq!(load.intent, Some(oc_tui::app::PanelIntent::LoadCards));
        assert!(
            !oc_tui::views::render_test(&state, 120, 40)
                .join("\n")
                .contains("NEEDLE-AFTER-PREVIEW")
        );
        assert!(
            state
                .handle_panel_key(oc_tui::events::KeyAction::Enter)
                .intent
                .is_none()
        );
        assert!(app.tool_output_page(local_b, op, 0, 240).await.is_err());
        state.set_session(sessions[0].clone());
        state.attach_page(&page);
    }
    let frame = oc_tui::views::render_test(&state, 120, 40).join("\n");
    for label in [title.as_str(), model_name.as_str(), provider_name.as_str()] {
        assert!(frame.contains(label), "missing {label}: {frame}");
    }
    // V03 owns the existing fixed-height/wrapped-row scrolling limitation.
    // Compact fixtures qualify visible replay; the large patch additionally
    // qualifies complete structured input and its parsed card across restart.
    if patch_repeat == 1 {
        // The first public reasoning item has output_item.done and a measured
        // duration. The second fixture deliberately has only a delta, so its
        // unfinished item must not acquire a fabricated completion duration.
        let completed = frame
            .lines()
            .filter(|row| {
                row.trim_start()
                    .strip_prefix("+ Thought · ")
                    .and_then(|header| header.split_whitespace().next())
                    .is_some_and(|duration| {
                        duration
                            .strip_suffix("ms")
                            .or_else(|| duration.strip_suffix('s'))
                            .and_then(|value| value.parse::<f64>().ok())
                            .is_some_and(|value| value > 0.0)
                    })
            })
            .count();
        let unfinished = frame
            .lines()
            .filter(|row| row.trim() == "+ Thought")
            .count();
        assert_eq!((completed, unfinished), (1, 1), "{frame}");
        assert!(
            frame.contains("probe.txt") && frame.contains("replay.txt"),
            "{frame}"
        );
    }
    assert!(state.history().rows().iter().any(|r| r.reasoning.is_some()));
    assert!(state.history().rows().iter().any(|r| r.tool.is_some()));
    assert!(
        state
            .history()
            .rows()
            .iter()
            .filter_map(|r| r.tool.as_ref())
            .any(|card| card.name == "apply_patch"
                && card.state == "completed"
                && card.files.iter().any(|f| f == "replay.txt")),
        "restart must retain the actual patch card, even when input exceeds the output-preview budget"
    );
    assert!(state.history().rows().iter().any(|r| {
        r.meta
            .as_ref()
            .is_some_and(|m| m.output_tokens == known.then_some(34))
    }));
    assert!(!format!("{page:?}").contains("fixture-secret"));
    assert!(!format!("{page:?}").contains("opaque-secret"));
    for c in "/model".chars() {
        state.handle_key(oc_tui::events::KeyAction::Char(c)).await;
    }
    state.handle_key(oc_tui::events::KeyAction::Enter).await;
    let picker = oc_tui::views::render_test(&state, 120, 40).join("\n");
    assert!(
        state
            .modal_options()
            .iter()
            .any(|option| option.title == model_name && option.category.is_empty())
            && picker.contains(&model_name)
            && picker.contains(&provider_name),
        "{picker}"
    );
    assert_eq!(picker.contains("Free"), price == Some((0, 0)), "{picker}");

    // Same application owner, real generation replacement A -> B -> A. Global
    // provider/agent stay available; local names/title/history cannot bleed.
    let project_b = root.path().join("project-b");
    std::fs::create_dir(&project_b).unwrap();
    std::fs::write(
        project_b.join("opencode.json"),
        serde_json::json!({
            "provider":{"fixture":{"name":"Local B provider","npm":"@ai-sdk/openai",
                "options":{"baseURL":format!("http://{address}/v1"),"apiKey":"fixture-secret"},
                "models":{"route/model/with/slashes":{"name":"Local B model"}}}}
        })
        .to_string(),
    )
    .unwrap();
    let b = app
        .switch_location(project_b.display().to_string())
        .await
        .unwrap();
    assert_eq!(b.catalog.models[0].display_name, "Local B model");
    assert_eq!(b.catalog.models[0].provider_name, "Local B provider");
    assert!(!b.catalog.models[0].context_known && !b.catalog.models[0].output_known);
    assert_eq!(b.catalog.agent_id.as_deref(), Some(agent.as_str()));
    assert!(
        app.history_page(sessions[0].clone(), None, None, 100)
            .await
            .is_err()
    );
    let b_session = SessionId(b.session.clone());
    if let Some(read) = turn.parts.iter().find_map(|part| match part {
        TranscriptPart::Tool(op) if op.name == "read" => Some(op),
        _ => None,
    }) {
        assert!(
            app.tool_output_page(b_session.clone(), read.op.clone(), 0, 1024)
                .await
                .is_err(),
            "Location B must not read A's tool output"
        );
    }
    let b_page = app
        .history_page(b_session.clone(), None, None, 100)
        .await
        .unwrap();
    state.reset_workspace();
    state.set_session(b_session);
    state.apply_catalog(b.catalog);
    state.attach_page(&b_page);
    let stale_turn = oc_core::core_app::WorkerTurnId(turn.id.clone());
    state.apply_reasoning_delta(&stale_turn, "STALE REASONING");
    state.apply_presentation(&stale_turn, turn);
    assert!(state.live_part_states.is_empty());
    state.apply_tool_started(&stale_turn, "stale-op", "read", "STALE TOOL");
    state.apply_finished(&stale_turn, "STALE ANSWER", 1);
    let b_frame = oc_tui::views::render_test(&state, 120, 40).join("\n");
    assert!(b_frame.contains("Local B model"));
    assert!(!b_frame.contains(&title));
    assert!(!b_frame.contains("STALE"));
    let a = app
        .switch_location(project.display().to_string())
        .await
        .unwrap();
    assert_eq!(a.catalog.models[0].display_name, model_name);
    assert_eq!(a.catalog.models[0].provider_name, provider_name);
    let restored = app
        .history_page(sessions[0].clone(), None, None, 100)
        .await
        .unwrap();
    assert_eq!(
        restored, page,
        "Location transitions must not rewrite durable presentation"
    );
    // A second genuine session in the same Location must not carry A's tool
    // cards; reattaching A reloads the owner's identical semantic projection.
    let other = SessionId::new("s-v06b-other").unwrap();
    app.create_session(other.clone()).await.unwrap();
    state.set_session(other.clone());
    state.attach_page(&app.history_page(other, None, None, 100).await.unwrap());
    assert!(state.history().rows().iter().all(|row| row.tool.is_none()));
    state.set_session(sessions[0].clone());
    state.attach_page(&restored);
    let projected = state
        .history()
        .rows()
        .iter()
        .filter_map(|row| row.tool.as_ref())
        .map(|card| (card.op.clone(), card.state.clone()))
        .collect::<Vec<_>>();
    let expected = page
        .rows
        .iter()
        .filter_map(|row| row.turn.as_ref())
        .flat_map(|turn| turn.parts.iter())
        .filter_map(|part| match part {
            TranscriptPart::Tool(op) => Some((op.op.clone(), op.state.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        projected, expected,
        "page reload and session switch retain durable op identity/order/state"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    // Invalid title configuration must be refused before durable acceptance,
    // including when an existing session already has a generated title.
    let original = configuration.clone();
    configuration["agent"]["title"] =
        serde_json::json!({"mode":"subagent","prompt":"Title","model":"fixture/missing#brief"});
    std::fs::write(config.join("opencode.json"), configuration.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.display().to_string()),
        (
            "XDG_CONFIG_HOME".into(),
            home.join("config").display().to_string(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let (invalid, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &home.join("data/oc"), env)
            .await
            .unwrap();
    let error = invalid
        .submit(
            sessions[0].clone(),
            "Do not accept invalid title profile".into(),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("title agent"), "{error}");
    assert_eq!(
        invalid
            .history_page(sessions[0].clone(), None, None, 100)
            .await
            .unwrap(),
        page
    );
    invalid.shutdown().await.unwrap();
    guard.join().await.unwrap();
    configuration["agent"]["title"] = serde_json::json!({"mode":"invalid-mode","prompt":"Title"});
    std::fs::write(config.join("opencode.json"), configuration.to_string()).unwrap();
    let env = BTreeMap::from([
        ("HOME".into(), home.display().to_string()),
        (
            "XDG_CONFIG_HOME".into(),
            home.join("config").display().to_string(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ]);
    let malformed =
        oc_adapters::application::spawn_with_env(&project, &home.join("data/oc"), env).await;
    assert!(
        malformed.is_err(),
        "malformed explicit title profile must not silently select the built-in default"
    );
    assert!(malformed.err().unwrap().contains("title"));
    std::fs::write(config.join("opencode.json"), original.to_string()).unwrap();
    binary_tui_restart(
        &home,
        &project,
        &sessions[0].0,
        &title,
        &model_name,
        patch_repeat == 1,
    );
    frame
}

fn binary_tui_restart(
    home: &std::path::Path,
    project: &std::path::Path,
    session: &str,
    title: &str,
    model: &str,
    compact: bool,
) {
    use std::os::fd::{AsRawFd, FromRawFd};
    let mut master = -1;
    let mut slave = -1;
    let size = libc::winsize {
        ws_row: 40,
        ws_col: 120,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    assert_eq!(
        // SAFETY: valid output pointers and size, defaults for name/termios.
        unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        },
        0
    );
    // SAFETY: openpty returned owned descriptors.
    let mut master = unsafe { std::fs::File::from_raw_fd(master) };
    // SAFETY: the distinct slave descriptor is owned here.
    let slave = unsafe { std::fs::File::from_raw_fd(slave) };
    // SAFETY: valid descriptor, only setting nonblocking read flags.
    unsafe {
        libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK);
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_oc"));
    command
        .env_clear()
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("OC_TEST_ALLOW_LOOPBACK", "1")
        .env("TERM", "xterm-256color")
        .current_dir(project)
        .arg("tui");
    if !session.is_empty() {
        command.args(["--session", session]);
    }
    let mut child = command
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut bytes = Vec::new();
    let mut buffer = [0; 8192];
    let mut ready = false;
    let mut submitted = !session.is_empty();
    let mut saw_live_reasoning = false;
    while std::time::Instant::now() < deadline {
        if let Ok(n) = master.read(&mut buffer) {
            bytes.extend_from_slice(&buffer[..n]);
        }
        let text = String::from_utf8_lossy(&bytes);
        if !submitted && text.contains(model) {
            master.write_all(b"Inspect probe.txt\r").unwrap();
            submitted = true;
        }
        if session.is_empty() && text.contains("Thinking") && !text.contains("Final durable answer")
        {
            saw_live_reasoning = true;
        }
        if [title, model, "Final durable answer"]
            .iter()
            .all(|s| text.contains(s))
            && (!compact
                || (without_sgr(&text).contains("+ Thought · ")
                    && ["probe.txt", "replay.txt"].iter().all(|s| text.contains(s))))
        {
            ready = true;
            break;
        }
        if child.try_wait().unwrap().is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    master.write_all(b"/quit\r").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
        let _ = master.read(&mut buffer);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if child.try_wait().unwrap().is_none() {
        child.kill().unwrap();
    }
    let status = child.wait().unwrap();
    assert!(ready, "restart frame: {}", String::from_utf8_lossy(&bytes));
    assert!(status.success(), "TUI failed to shut down: {status}");
    if session.is_empty() {
        assert!(
            saw_live_reasoning,
            "must observe streaming before the completed frame: {}",
            String::from_utf8_lossy(&bytes)
        );
    }
}

// Only erase color/style changes: cursor moves remain barriers between
// independently painted cells, so the reasoning icon and label must still
// appear together in the PTY stream.
fn without_sgr(text: &str) -> String {
    let mut parts = text.split('\x1b');
    let mut visible = parts.next().unwrap_or_default().to_string();
    for part in parts {
        if let Some(csi) = part.strip_prefix('[') {
            let after_params = csi.trim_start_matches(|c: char| c.is_ascii_digit() || c == ';');
            if let Some(rest) = after_params.strip_prefix('m') {
                visible.push_str(rest);
                continue;
            }
        }
        visible.push('\x1b');
        visible.push_str(part);
    }
    visible
}
