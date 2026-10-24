//! Opt-in long-session performance probe (not part of the offline gate).
//!
//! `generate` drives the real owner through a scripted Responses peer until
//! one session holds thousands of messages and tool calls (read, grep, shell,
//! file edits). `measure` reopens that data root and reports owner page
//! latency plus TUI scroll frame cost, including older-page loads.
//!
//! ```sh
//! OC_PERF_DATA=target/perf-probe/long OC_PERF_TURNS=300 \
//!   cargo test -p oc2 --test perf_long_session -- --ignored --nocapture generate
//! OC_PERF_DATA=target/perf-probe/long \
//!   cargo test -p oc2 --test perf_long_session -- --ignored --nocapture measure
//! ```
use oc_core::core_app::CoreEvent;
use oc_core::domain::SessionId;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SESSION: &str = "perf-long";
const ROUNDS: usize = 4;
const FRAME_165HZ: Duration = Duration::from_micros(6_060);

fn data_root() -> PathBuf {
    let raw = std::env::var_os("OC_PERF_DATA").expect("OC_PERF_DATA names the probe data root");
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        // The owner refuses non-canonical data roots.
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap()
            .join(path)
    }
}

fn rust_file(seed: usize) -> String {
    (0..36)
        .map(|line| match line % 6 {
            0 => format!("/// Item {seed}.{line}: synthetic documentation line."),
            1 => format!("pub fn item_{seed}_{line}(value: u64) -> u64 {{"),
            2 => format!(
                "    let scaled = value.wrapping_mul({}) ^ {line};",
                seed + 3
            ),
            3 => "    if scaled % 7 == 0 { return scaled / 7; }".to_string(),
            4 => format!("    scaled.rotate_left({})", line % 13),
            _ => "}".to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn final_answer(turn: usize) -> String {
    format!(
        "## Turn {turn} summary\n\nUpdated **{ROUNDS}** generated modules and checked the \
         `item_*` helpers.\n\n- read the seeded sources\n- searched for `pub fn`\n- added \
         new files\n\n```rust\npub fn checksum_{turn}(values: &[u64]) -> u64 {{\n    \
         values.iter().fold(0, |acc, v| acc ^ v.rotate_left(3))\n}}\n```\n\n{}\n",
        "The generated code keeps every helper total and side-effect free. ".repeat(6)
    )
}

fn call(id: &str, name: &str, args: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"type":"function_call","id":format!("fc_{id}"),"call_id":id,
        "name":name,"arguments":args.to_string(),"status":"completed"})
}

/// One scripted reply per request: `ROUNDS` tool rounds, then a final answer.
fn reply(request: &serde_json::Value, index: usize) -> String {
    let turn = index / (ROUNDS + 1);
    let round = index % (ROUNDS + 1);
    let tools: Vec<&str> = request["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let output = if round < ROUNDS {
        let id = |kind: &str| format!("t{turn}r{round}{kind}");
        let mut calls = vec![
            call(
                &id("read"),
                "read",
                serde_json::json!({"path": format!("src/f{}.rs", (turn + round) % 20)}),
            ),
            call(
                &id("grep"),
                "grep",
                serde_json::json!({"pattern": format!("item_{}_", (turn + round) % 20), "path": "src"}),
            ),
        ];
        let path = format!("gen/t{turn}_r{round}.rs");
        let body = rust_file(turn * ROUNDS + round);
        calls.push(if tools.contains(&"apply_patch") {
            let added: String = body.lines().map(|line| format!("+{line}\n")).collect();
            call(
                &id("patch"),
                "apply_patch",
                serde_json::json!({"patchText": format!("*** Begin Patch\n*** Add File: {path}\n{added}*** End Patch")}),
            )
        } else {
            call(
                &id("write"),
                "write",
                serde_json::json!({"filePath": path, "content": body}),
            )
        });
        if round == 0 {
            calls.push(call(
                &id("shell"),
                "shell",
                serde_json::json!({"command": format!("printf 'check %s\\n' {turn} a b c")}),
            ));
        }
        calls
    } else {
        let text = final_answer(turn);
        vec![
            serde_json::json!({"type":"message","role":"assistant","id":format!("m{turn}"),
            "status":"completed","content":[{"type":"output_text","text":text}]}),
        ]
    };
    let mut sse = String::new();
    if round == ROUNDS {
        let text = output[0]["content"][0]["text"].as_str().unwrap_or_default();
        for chunk in text.as_bytes().chunks(64) {
            let piece = String::from_utf8_lossy(chunk);
            sse.push_str(&format!(
                "data: {}\n\n",
                serde_json::json!({"type":"response.output_text.delta","delta":piece})
            ));
        }
    }
    for item in &output {
        sse.push_str(&format!(
            "data: {}\n\n",
            serde_json::json!({"type":"response.output_item.done","item":item})
        ));
    }
    sse.push_str(&format!(
        "data: {}\n\n",
        serde_json::json!({"type":"response.completed","response":{"status":"completed",
            "output":output,"usage":{"input_tokens":1000,"output_tokens":100}}})
    ));
    sse
}

fn serve(first_turn: usize) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!(
        "http://127.0.0.1:{}/v1",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        for (index, stream) in listener.incoming().filter_map(Result::ok).enumerate() {
            let mut reader = BufReader::new(stream);
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if line.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = line.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let sse = reply(&request, first_turn * (ROUNDS + 1) + index);
            let _ = write!(
                reader.get_mut(),
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{sse}",
                sse.len()
            );
        }
    });
    base
}

fn project_config(project: &Path, base: &str) {
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::create_dir_all(project.join("gen")).unwrap();
    for index in 0..20 {
        std::fs::write(project.join(format!("src/f{index}.rs")), rust_file(index)).unwrap();
    }
    // Resumed turns write fresh files; drop the earlier generation's outputs.
    for entry in std::fs::read_dir(project.join("gen")).unwrap().flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
    let config = serde_json::json!({
        "model": "fixture/m",
        "providers": {"fixture": {"package": "@ai-sdk/openai",
            "settings": {"baseURL": base, "apiKey": "test-key"},
            "models": {"m": {"limit": {"context": 100_000_000, "output": 100_000}}}}},
        "permissions": [{"action": "*", "resource": "*", "effect": "allow"}]
    });
    std::fs::write(project.join("opencode.json"), config.to_string()).unwrap();
}

fn env(root: &Path) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "HOME".into(),
            root.join("home").to_string_lossy().into_owned(),
        ),
        ("OC_TEST_ALLOW_LOOPBACK".into(), "1".into()),
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opt-in long-session fixture generator"]
async fn generate() {
    let root = data_root();
    // OC_PERF_RESUME continues an existing synthetic session (profiling a turn).
    let resume = std::env::var_os("OC_PERF_RESUME").is_some();
    assert!(
        resume || !root.exists(),
        "refusing to reuse an existing probe root"
    );
    let turns: usize = std::env::var("OC_PERF_TURNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(300);
    let project = root.join("project");
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("data")).unwrap();
    for private in [root.clone(), root.join("data")] {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(private, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let first = if resume {
        std::env::var("OC_PERF_FIRST_TURN")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0)
    } else {
        0
    };
    project_config(&project, &serve(first));
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &root.join("data"), env(&root))
            .await
            .unwrap();
    let session = SessionId::new(SESSION).unwrap();
    if !resume {
        app.create_session(session.clone()).await.unwrap();
        app.rename_session(session.clone(), "Synthetic long session".into())
            .await
            .unwrap();
    }
    let mut events = app.subscribe();
    let started = Instant::now();
    for turn in first..first + turns {
        let prompt = format!(
            "Turn {turn}: extend the generated modules, re-read `src/f{}.rs` and keep \
             helpers total.\n\nConstraints:\n- no panics\n- no allocation in hot loops",
            turn % 20
        );
        app.submit(session.clone(), prompt).await.unwrap();
        loop {
            match tokio::time::timeout(Duration::from_secs(60), events.recv())
                .await
                .expect("turn progress")
            {
                Ok(CoreEvent::TurnFinished { .. }) => break,
                Ok(CoreEvent::TurnFailed { error, .. }) => panic!("turn {turn} failed: {error}"),
                _ => {}
            }
        }
        if turn % 25 == 0 {
            eprintln!("PERF generated turn {turn} after {:?}", started.elapsed());
        }
    }
    eprintln!("PERF generated {turns} turns in {:?}", started.elapsed());
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

fn stats(label: &str, mut samples: Vec<Duration>) {
    if samples.is_empty() {
        return;
    }
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q) as usize];
    let over = samples.iter().filter(|d| **d > FRAME_165HZ).count();
    eprintln!(
        "PERF {label}: n={} p50={:?} p95={:?} max={:?} over6ms={over}",
        samples.len(),
        at(0.5),
        at(0.95),
        samples[samples.len() - 1]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opt-in long-session measurement"]
async fn measure() {
    let root = data_root();
    let project = root.join("project");
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &root.join("data"), env(&root))
            .await
            .unwrap();
    let session = SessionId::new(SESSION).unwrap();
    let mut opens = Vec::new();
    let mut page = None;
    for _ in 0..3 {
        let started = Instant::now();
        page = Some(
            app.transcript_page(
                session.clone(),
                None,
                None,
                oc_adapters::application::HISTORY_PAGE_LIMIT,
            )
            .await
            .unwrap(),
        );
        opens.push(started.elapsed());
    }
    let page = page.unwrap();
    eprintln!(
        "PERF latest page rows={} total={}",
        page.rows.len(),
        page.total
    );
    stats("owner latest page", opens);

    let mut state = oc_tui::app::TuiState::new(app.clone(), session.clone());
    state.attach_page(&page);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 55)).unwrap();
    // Wait for an off-thread projection like the event loop does. Yield each
    // turn as the real loop does, so Tokio's cooperative budget is restored;
    // a stuck flight is reported, never waited on forever.
    let mut stalls = 0usize;
    async fn settle(state: &mut oc_tui::app::TuiState, stalls: &mut usize) {
        let started = Instant::now();
        while state.has_pending_transcript_projection() {
            state.poll_transcript_projection();
            if started.elapsed() > Duration::from_secs(10) {
                *stalls += 1;
                eprintln!("PERF stall: projection pending after 10s");
                break;
            }
            tokio::task::yield_now().await;
        }
    }
    let first = Instant::now();
    terminal
        .draw(|frame| oc_tui::shell::render(frame, &state))
        .unwrap();
    settle(&mut state, &mut stalls).await;
    terminal
        .draw(|frame| oc_tui::shell::render(frame, &state))
        .unwrap();
    eprintln!("PERF first transcript paint {:?}", first.elapsed());

    let steps: usize = std::env::var("OC_PERF_SCROLL")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1500);
    let mut frames = Vec::new();
    let mut pages = Vec::new();
    for _ in 0..steps {
        let started = Instant::now();
        let outcome = state.scroll_transcript(true);
        if matches!(outcome.intent, Some(oc_tui::app::PanelIntent::LoadOlder)) {
            let before = state
                .history()
                .rows()
                .iter()
                .find(|row| row.seq != i64::MAX)
                .map(|row| row.seq);
            if let Some(before) = before {
                let part = state.history().older_part_cursor();
                let window = oc_core::queries::HistoryReadWindow {
                    before_seq: part.is_none().then_some(before),
                    after_seq: None,
                    part,
                };
                let load = Instant::now();
                let older = app
                    .history_window(
                        session.clone(),
                        window,
                        oc_adapters::application::HISTORY_PAGE_LIMIT,
                    )
                    .await
                    .unwrap();
                let read = load.elapsed();
                state.prepend_page(&older);
                pages.push(load.elapsed());
                eprintln!(
                    "PERF older page {} rows in {:?} (owner read {read:?}) at step {}",
                    older.rows.len(),
                    load.elapsed(),
                    frames.len()
                );
            }
        }
        state.tick_ui(Instant::now());
        terminal
            .draw(|frame| oc_tui::shell::render(frame, &state))
            .unwrap();
        settle(&mut state, &mut stalls).await;
        terminal
            .draw(|frame| oc_tui::shell::render(frame, &state))
            .unwrap();
        frames.push(started.elapsed());
        if frames.len() % 250 == 0 {
            eprintln!("PERF progress {} steps", frames.len());
        }
    }
    eprintln!("PERF projection stalls >10s: {stalls}");
    eprintln!(
        "PERF scrolled {steps} rows; window rows={} retained={}",
        state.history().len(),
        state.retained_bytes()
    );
    stats("scroll frame (incl. paging)", frames);
    stats("owner older page", pages);
    drop(state);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
