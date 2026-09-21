//! T25 (E2E01/E2E03/E2E05): offline configured-workspace coding,
//! compression/restart, and A→B switch against a scripted Responses
//! server. No network beyond loopback doubles, no JS runtime, no
//! subprocess except the scripted `cargo test` the model itself runs.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oc_adapters::config::{Generation, Permission, Source, assemble, classify_plugin};
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::defs::{DefRoot, load_definitions, load_instructions, select_primary};
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{Runtime, TurnParams, TurnStatus, expand_command};
use oc_adapters::storage::Db;
use oc_adapters::tools::SkillSnapshot;
use oc_core::context_plan::ProtectedSpec;

fn sse_delta(text: &str) -> String {
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{}}}\n\n",
        serde_json::Value::String(text.to_string())
    )
}

fn sse_completed() -> String {
    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n\n".to_string()
}

fn sse_tool_call(call_id: &str, name: &str, args: &serde_json::Value) -> String {
    let item_id = format!("fc_{call_id}");
    let added = serde_json::json!({"type": "response.output_item.added", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": "", "status": "in_progress"
    }});
    let delta = serde_json::json!({"type": "response.function_call_arguments.delta",
        "item_id": item_id, "delta": args.to_string()});
    let done = serde_json::json!({"type": "response.output_item.done", "item": {
        "type": "function_call", "id": item_id, "call_id": call_id,
        "name": name, "arguments": args.to_string(), "status": "completed"
    }});
    format!("data: {added}\n\ndata: {delta}\n\ndata: {done}\n\n")
}

/// Scripted fake: queued SSE bodies in order (last repeats), every
/// request body recorded for outbound-context measurement.
struct Fake;

impl Fake {
    fn start(script: Vec<String>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!(
            "http://127.0.0.1:{}/v1",
            listener.local_addr().expect("addr").port()
        );
        let queue = Arc::new(Mutex::new(VecDeque::from(script)));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let worker_queue = queue.clone();
        let worker_bodies = bodies.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().filter_map(Result::ok) {
                let queue = worker_queue.clone();
                let bodies = worker_bodies.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut content_length = 0usize;
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(0) => return,
                            Ok(_) => {}
                            Err(_) => return,
                        }
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':')
                            && name.trim().eq_ignore_ascii_case("content-length")
                        {
                            content_length = value.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; content_length];
                    if content_length > 0 {
                        let _ = reader.read_exact(&mut body);
                    }
                    bodies
                        .lock()
                        .expect("bodies")
                        .push(String::from_utf8_lossy(&body).to_string());
                    let payload = {
                        let mut queue = queue.lock().expect("queue");
                        if queue.len() > 1 {
                            queue.pop_front().expect("script")
                        } else {
                            queue.front().cloned().unwrap_or_default()
                        }
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        payload.len(),
                        payload
                    );
                    let _ = reader.get_mut().write_all(response.as_bytes());
                });
            }
        });
        (base, bodies)
    }
}

struct Harness {
    _project: tempfile::TempDir,
    _data: tempfile::TempDir,
    db: Db,
    catalog: ModelCatalog,
}

fn allow_all() -> BTreeMap<String, Permission> {
    [
        "read",
        "apply_patch",
        "bash",
        "webfetch",
        "skill",
        "compress",
    ]
    .into_iter()
    .map(|name| (name.to_string(), Permission::Allow))
    .collect()
}

fn make_harness(permissions: BTreeMap<String, Permission>) -> (Harness, Generation) {
    let project = tempfile::tempdir().expect("project");
    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let catalog = ModelCatalog {
        provider: "test".to_string(),
        models: [(
            "m".to_string(),
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        )]
        .into_iter()
        .collect(),
    };
    let generation = Generation {
        providers: BTreeMap::new(),
        mcp: BTreeMap::new(),
        permissions,
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    (
        Harness {
            _project: project,
            _data: data,
            db,
            catalog,
        },
        generation,
    )
}

fn runtime_of<'a>(harness: &'a Harness, location: &str, generation: Generation) -> Runtime<'a> {
    let project = harness._project.path();
    let files = oc_adapters::files::Files::new(project, harness._data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(project).expect("shell");
    Runtime::new(
        &harness.db,
        location,
        generation,
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        files,
        shell,
        BTreeMap::new(),
        oc_adapters::tools::ToolRoots {
            project: project.to_path_buf(),
            data: harness._data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .expect("runtime")
}

fn provider_of(base: &str) -> ResponsesConfig {
    ResponsesConfig {
        headers: BTreeMap::new(),
        set_cache_key: true,
        base_url: base.to_string(),
        api_key: "test-key".to_string(),
        timeout: Some(false),
        chunk_timeout_ms: 5_000,
        connect_timeout: Duration::from_secs(5),
        allow_private: true,
    }
}

fn params<'c>(
    session: &str,
    prompt: &str,
    harness: &'c Harness,
    provider: ResponsesConfig,
    cancel: &'c AtomicBool,
) -> TurnParams<'c> {
    TurnParams {
        session: session.to_string(),
        prompt: prompt.to_string(),
        invocation: None,
        catalog: &harness.catalog,
        model_id: "m".to_string(),
        variant: None,
        max_output: 1_000,
        provider,
        cancel,
        max_rounds: 6,
    }
}

static NO_CANCEL: AtomicBool = AtomicBool::new(false);

fn src(path: &str, text: &str) -> Source {
    Source {
        path: path.to_string(),
        text: text.to_string(),
        trusted: true,
    }
}

fn write_tree(base: &std::path::Path, files: &[(&str, &str)]) {
    for (rel, text) in files {
        let path = base.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, text).expect("write");
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("readdir") {
        let entry = entry.expect("entry");
        let dest = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).expect("copy");
        }
    }
}

fn snapshot_files(root: &std::path::Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readdir") {
            let entry = entry.expect("entry");
            let path = entry.path();
            if path.file_name().map(|n| n == "target").unwrap_or(false) {
                continue;
            }
            if entry.file_type().expect("type").is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .expect("rel")
                    .to_string_lossy()
                    .to_string();
                out.insert(rel, std::fs::read_to_string(&path).expect("read"));
            }
        }
    }
    out
}

const G_CONFIG: &str = r#"{"provider": {"ludka2": {"npm": "@ai-sdk/openai",
    "options": {"baseURL": "https://g.invalid", "apiKey": "gk",
    "timeout": false, "chunkTimeout": 6000000}}},
    "permissions": {"read": "allow", "apply_patch": "allow", "bash": "allow",
    "webfetch": "allow", "skill": "allow", "compress": "allow"}}"#;

#[tokio::test]
async fn e2e05_configured_workspace_a_to_b() {
    let ws = tempfile::tempdir().expect("ws");
    let g = ws.path().join("G");
    let a = ws.path().join("A");
    let b = ws.path().join("B");
    write_tree(
        &g,
        &[
            ("opencode.json", G_CONFIG),
            ("AGENTS.md", "global rules\n"),
            (
                ".opencode/skills/greet/SKILL.md",
                "---\nname: greet\ndescription: greets on call\n---\nGREET-BODY-MARKER-42: repeat after me\n",
            ),
            (
                ".opencode/agents/main.md",
                "---\ndescription: main agent\nmodel: m\n---\nYou are main.\n",
            ),
            (
                ".opencode/commands/deploy.md",
                "---\ndescription: deploy it\n---\ntouch WOULD-RUN-MARKER $1\n",
            ),
        ],
    );
    write_tree(
        &a,
        &[
            ("opencode.json", r#"{"permissions": {"webfetch": "deny"}}"#),
            ("AGENTS.md", "location A rules\n"),
            (
                ".opencode/commands/test-a.md",
                "---\ndescription: a test\n---\ncargo test $1\n",
            ),
        ],
    );
    write_tree(
        &b,
        &[
            ("opencode.json", "{}"),
            ("AGENTS.md", "location B rules\n"),
            (
                ".opencode/commands/test-b.md",
                "---\ndescription: b test\n---\ncargo test $1\n",
            ),
        ],
    );

    // Ordered instructions: global first, sentinel once each.
    let (instructions, diags) = load_instructions(&[
        ("G/AGENTS.md".to_string(), g.join("AGENTS.md")),
        ("A/AGENTS.md".to_string(), a.join("AGENTS.md")),
    ]);
    assert!(diags.is_empty());
    assert!(instructions.find("global rules") < instructions.find("location A rules"));

    // Generation A: later source wins, permissions most-restrictive.
    let gen_a = assemble(
        &[
            src("G/opencode.json", G_CONFIG),
            src(
                "A/opencode.json",
                r#"{"permissions": {"webfetch": "deny"}}"#,
            ),
        ],
        &BTreeMap::new(),
        None,
    )
    .expect("gen A");
    assert_eq!(gen_a.permissions["webfetch"], Permission::Deny);
    assert_eq!(gen_a.permissions["read"], Permission::Allow);

    let defs_a = load_definitions(&[
        DefRoot {
            dir: g.join(".opencode"),
            origin: "G".to_string(),
        },
        DefRoot {
            dir: a.join(".opencode"),
            origin: "A".to_string(),
        },
    ]);
    assert!(defs_a.diagnostics.is_empty(), "{:?}", defs_a.diagnostics);
    assert!(defs_a.skills.contains_key("greet"));
    assert!(defs_a.agents.contains_key("main"));
    assert!(defs_a.commands.contains_key("deploy"));
    assert!(defs_a.commands.contains_key("test-a"));
    let pos = |id: &str| defs_a.order.iter().position(|o| o == id).expect(id);
    assert!(pos("skill.greet@G") < pos("agent.main@G"));
    assert!(pos("agent.main@G") < pos("command.deploy@G"));
    assert!(pos("command.deploy@G") < pos("command.test-a@A"));

    // Skill body absent from the model projection until the call.
    let (snapshot, warnings) =
        SkillSnapshot::build(&[("greet".to_string(), defs_a.skills["greet"].body.clone())]);
    assert!(warnings.is_empty());
    let projection = serde_json::to_string(&snapshot.projection()).expect("json");
    assert!(projection.contains("greet"));
    assert!(!projection.contains("GREET-BODY-MARKER-42"));

    // Both native aliases classify; lookalikes fail before any execution.
    assert_eq!(
        classify_plugin("@tarquinen/opencode-dcp", "/ws").expect("bare"),
        "dcp"
    );
    assert_eq!(
        classify_plugin("@tarquinen/opencode-dcp@3.1.15", "/ws").expect("pinned"),
        "dcp"
    );
    assert_eq!(
        classify_plugin("/ws/plugin/openproxy-models.js", "/ws").expect("discovery"),
        "discovery"
    );
    for bad in [
        "@tarquinen/opencode-dcp@3.1.16",
        "other/openproxy-models.js",
        "evil.ts",
        "@ai-sdk/openai",
    ] {
        assert!(classify_plugin(bad, "/ws").is_err(), "{bad}");
    }

    // Primary agent digest is stable for the turn.
    let digest = select_primary(&defs_a, "main").expect("select");
    assert_eq!(digest, select_primary(&defs_a, "main").expect("stable"));

    // Scripted turn on A uses the skill; body arrives only as call output.
    let (harness, _) = make_harness(gen_a.permissions.clone());
    let runtime = runtime_of(&harness, "A", gen_a);
    runtime
        .publish_skills(vec![(
            "greet".to_string(),
            defs_a.skills["greet"].body.clone(),
        )])
        .expect("skills");
    runtime.create_session("ea5").expect("session");
    let (base, _) = Fake::start(vec![
        sse_tool_call("c1", "skill", &serde_json::json!({"id": "greet"})) + &sse_completed(),
        sse_delta("done") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(params(
            "ea5",
            "greet via skill",
            &harness,
            provider_of(&base),
            &NO_CANCEL,
        ))
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    assert_eq!(report.calls.len(), 1);
    assert_eq!(report.calls[0].name, "skill");
    assert!(report.calls[0].output.contains("GREET-BODY-MARKER-42"));
    assert!(
        report
            .calls
            .iter()
            .all(|c| c.name != "bash" && c.name != "webfetch")
    );

    // Command bodies are literal text: expanded, never executed.
    assert_eq!(
        expand_command("touch WOULD-RUN-MARKER $1", &["v1".to_string()]).expect("expand"),
        "touch WOULD-RUN-MARKER v1"
    );
    assert!(!harness._project.path().join("WOULD-RUN-MARKER").exists());
    assert!(!ws.path().join("WOULD-RUN-MARKER").exists());

    // Switch to B: global retained, A-local gone, old session unopenable.
    let gen_b = assemble(
        &[
            src("G/opencode.json", G_CONFIG),
            src("B/opencode.json", "{}"),
        ],
        &BTreeMap::new(),
        None,
    )
    .expect("gen B");
    let defs_b = load_definitions(&[
        DefRoot {
            dir: g.join(".opencode"),
            origin: "G".to_string(),
        },
        DefRoot {
            dir: b.join(".opencode"),
            origin: "B".to_string(),
        },
    ]);
    assert!(defs_b.skills.contains_key("greet"), "global retained");
    assert!(!defs_b.commands.contains_key("test-a"), "A-local removed");
    assert!(defs_b.commands.contains_key("test-b"));
    let runtime_b = runtime_of(&harness, "B", gen_b);
    assert!(
        runtime_b.open_session("ea5").is_err(),
        "A session stays bound"
    );
    runtime_b.create_session("eb5").expect("b session");
    let (base_b, _) = Fake::start(vec![sse_delta("b ok") + &sse_completed()]);
    let report_b = runtime_b
        .run_turn(params(
            "eb5",
            "hello B",
            &harness,
            provider_of(&base_b),
            &NO_CANCEL,
        ))
        .await
        .expect("turn B");
    assert_eq!(report_b.status, TurnStatus::Completed);
    assert_eq!(report_b.text, "b ok");
}

/// Absolute toolchain path: the scrubbed child `PATH` (`/usr/bin:/bin`)
/// cannot resolve `cargo`, so the scripted model invokes it by path.
fn cargo_bin() -> String {
    if let Ok(cargo) = std::env::var("CARGO") {
        return cargo;
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("cargo");
        if cand.is_file() {
            return cand.to_string_lossy().to_string();
        }
    }
    panic!("cargo not found in test PATH");
}

/// Absolute `rustc` for the `RUSTC` parent-env extra: the scrubbed child
/// `PATH` cannot resolve it, so the launcher environment provides the
/// toolchain location (production passes its own observed env through
/// the same scrub).
fn rustc_bin() -> String {
    let from_cargo = std::path::Path::new(&cargo_bin())
        .parent()
        .map(|dir| dir.join("rustc"));
    if let Some(cand) = from_cargo
        && cand.is_file()
    {
        return cand.to_string_lossy().to_string();
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("rustc");
        if cand.is_file() {
            return cand.to_string_lossy().to_string();
        }
    }
    panic!("rustc not found in test PATH");
}

#[tokio::test]
async fn e2e01_seeded_coding_fix() {
    let ws = tempfile::tempdir().expect("ws");
    let project = ws.path().join("proj");
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_dir(&here.join("../../fixtures/e2e-coding"), &project);
    let sentinel = ws.path().join("outside.txt");
    std::fs::write(&sentinel, "untouched").expect("sentinel");
    let before = snapshot_files(&project);

    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let catalog = ModelCatalog {
        provider: "test".to_string(),
        models: [(
            "m".to_string(),
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        )]
        .into_iter()
        .collect(),
    };
    let generation = Generation {
        providers: BTreeMap::new(),
        mcp: BTreeMap::new(),
        permissions: allow_all(),
        provenance: BTreeMap::new(),
        warnings: Vec::new(),
    };
    let files = oc_adapters::files::Files::new(&project, data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(&project).expect("shell");
    let parent_env: BTreeMap<String, String> =
        [("RUSTC".to_string(), rustc_bin())].into_iter().collect();
    let runtime = Runtime::new(
        &db,
        "work",
        generation,
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        files,
        shell,
        parent_env,
        oc_adapters::tools::ToolRoots {
            project: project.clone(),
            data: data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .expect("runtime");
    runtime.create_session("e1").expect("session");

    let patch = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n pub fn add(a: i32, b: i32) -> i32 {\n-    a - b\n+    a + b\n }\n*** End Patch\n";
    let (base, _) = Fake::start(vec![
        sse_tool_call("r1", "read", &serde_json::json!({"path": "src/lib.rs"})) + &sse_completed(),
        sse_tool_call(
            "r2",
            "apply_patch",
            &serde_json::json!({"patchText": patch}),
        ) + &sse_completed(),
        sse_tool_call(
            "r3",
            "bash",
            &serde_json::json!({"argv": [cargo_bin(), "test"], "timeout_ms": 120_000}),
        ) + &sse_completed(),
        sse_delta("fixed") + &sse_completed(),
    ]);
    let report = runtime
        .run_turn(TurnParams {
            session: "e1".to_string(),
            prompt: "fix the add bug and run tests".to_string(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".to_string(),
            variant: None,
            max_output: 1_000,
            provider: provider_of(&base),
            cancel: &NO_CANCEL,
            max_rounds: 6,
        })
        .await
        .expect("turn");
    assert_eq!(report.status, TurnStatus::Completed);
    let names: Vec<&str> = report.calls.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["read", "apply_patch", "bash"]);
    assert!(
        report.calls[2].output.contains("test result: ok"),
        "model-observed tests pass: {}",
        report.calls[2].output
    );

    // Independent verification: fixture tests pass, API stable.
    let output = std::process::Command::new(cargo_bin())
        .args(["test"])
        .current_dir(&project)
        .output()
        .expect("cargo test");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("test result: ok"));

    // No edits outside the fixture file (`Cargo.lock` is the runner's
    // own artifact, `target/` is excluded from the snapshot).
    let after = snapshot_files(&project);
    let added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
    assert!(
        added.iter().all(|k| k.as_str() == "Cargo.lock"),
        "added: {added:?}"
    );
    for (rel, text) in &before {
        if rel == "src/lib.rs" {
            continue;
        }
        assert_eq!(after.get(rel), Some(text), "unexpected change in {rel}");
    }
    assert!(after["src/lib.rs"].contains("a + b"));
    assert!(
        after["src/lib.rs"].contains("\"1.0.0\""),
        "protected API intact"
    );
    assert_eq!(
        std::fs::read_to_string(&sentinel).expect("sentinel"),
        "untouched"
    );
}

#[tokio::test]
async fn e2e03_compress_restart_retained_fact() {
    let (harness, generation) = make_harness(allow_all());
    let pad = |tag: &str| format!("filler {tag} {}", "lorem ipsum dolor sit amet ".repeat(40));
    // Turn 1-3 build a long transcript holding one key fact.
    let (base1, bodies) = Fake::start(vec![
        sse_delta(&pad("PAD-AAA")) + &sse_completed(),
        sse_delta(&pad("PAD-BBB")) + &sse_completed(),
        sse_delta("noted WIDGET-COLOR-BLUE among pad").to_string() + &sse_completed(),
    ]);
    {
        let runtime = runtime_of(&harness, "work", generation);
        runtime.create_session("e3").expect("session");
        for (i, prompt) in ["first", "second", "third"].iter().enumerate() {
            let report = runtime
                .run_turn(params(
                    "e3",
                    &format!("{prompt} {}", "please ".repeat(60)),
                    &harness,
                    provider_of(&base1),
                    &NO_CANCEL,
                ))
                .await
                .expect("turn");
            assert_eq!(report.status, TurnStatus::Completed, "turn {i}");
        }
        let sizes: Vec<usize> = bodies
            .lock()
            .expect("bodies")
            .iter()
            .map(String::len)
            .collect();
        assert_eq!(sizes.len(), 3);

        // Model calls compress: same permission path, validated ranges.
        let ids: Vec<(String, String, String)> = harness.db.read_history_full("e3").expect("ids");
        assert!(ids.len() >= 6);
        let args = serde_json::json!({
            "topic": "early work",
            "content": [{
                "startId": ids[0].0, "endId": ids[3].0,
                "summary": "early discussion done; retained fact WIDGET-COLOR-BLUE",
            }],
        });
        let spec = ProtectedSpec {
            protect_user_messages: false,
            protect_tags: false,
            file_globs: Vec::new(),
        };
        let compress = runtime.run_compress("e3", &args, &spec).expect("compress");
        assert!(compress.shrank, "projection must shrink");
        assert!(!compress.blocks.is_empty());
        let first_bytes: usize = bodies.lock().expect("bodies").iter().map(String::len).sum();
        drop(runtime);

        // Restart: same data root, new runtime; session reopens, fact flows.
        let (_, generation2) = make_harness(allow_all());
        let runtime2 = runtime_of(&harness, "work", generation2);
        runtime2.open_session("e3").expect("reopen after restart");
        let patch = "*** Begin Patch\n*** Add File: widget.txt\n+blue\n*** End Patch\n";
        let (base2, bodies2) = Fake::start(vec![
            sse_tool_call(
                "w1",
                "apply_patch",
                &serde_json::json!({"patchText": patch}),
            ) + &sse_completed(),
            sse_delta("repainted") + &sse_completed(),
        ]);
        let report = runtime2
            .run_turn(params(
                "e3",
                "repaint the widget",
                &harness,
                provider_of(&base2),
                &NO_CANCEL,
            ))
            .await
            .expect("next prompt");
        assert_eq!(report.status, TurnStatus::Completed);
        let next_bodies = bodies2.lock().expect("bodies2").clone();
        assert_eq!(next_bodies.len(), 2);
        assert!(
            next_bodies[0].contains("WIDGET-COLOR-BLUE"),
            "retained fact reaches the model"
        );
        assert!(
            !next_bodies[0].contains("PAD-AAA"),
            "covered filler leaves outbound context"
        );
        let next_bytes: usize = next_bodies.iter().map(String::len).sum();
        assert!(
            next_bytes < first_bytes,
            "output context reduced: {next_bytes} < {first_bytes}"
        );
        let widget =
            std::fs::read_to_string(harness._project.path().join("widget.txt")).expect("widget");
        assert!(widget.contains("blue"));
    }
}
