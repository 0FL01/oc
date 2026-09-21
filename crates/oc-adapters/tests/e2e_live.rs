//! T25/T27 reusable live workflow harness (ignored, env-gated).
//!
//! Mirrors `e2e_offline.rs` against the real OpenProxy: seeded coding
//! turn, session reopen + next command, compress, restart, webfetch and
//! (when configured) `codex_web` search. Requires `LUDKA2_API_URL`,
//! `LUDKA2_API_KEY` and `OC_TEST_MODEL`; without them it records
//! `BUILD_READY_LIVE_BLOCKED` and passes without touching the network —
//! never a false PASS. Optional: `OC_TEST_VARIANT`, `LUDKA2_MCP_URL`
//! (codex_web exact URL, same key as bearer).

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use oc_adapters::config::Permission;
use oc_adapters::dcp_auto::DcpConfig;
use oc_adapters::models::ModelCatalog;
use oc_adapters::patch::ProtectedGlobs;
use oc_adapters::provider::ResponsesConfig;
use oc_adapters::runtime::{Runtime, TurnParams, TurnStatus};
use oc_adapters::storage::Db;
use oc_core::context_plan::ProtectedSpec;

static NO_CANCEL: AtomicBool = AtomicBool::new(false);

/// Absolute toolchain paths: the scrubbed child `PATH` (`/usr/bin:/bin`)
/// cannot resolve `cargo`/`rustc`.
fn cargo_abs() -> String {
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

fn rustc_abs() -> String {
    let sibling = std::path::Path::new(&cargo_abs())
        .parent()
        .map(|dir| dir.join("rustc"));
    if let Some(cand) = sibling
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

fn live_env() -> Option<(ResponsesConfig, String, Option<String>)> {
    let base = std::env::var("LUDKA2_API_URL").ok()?;
    let key = std::env::var("LUDKA2_API_KEY").ok()?;
    let model = std::env::var("OC_TEST_MODEL").ok()?;
    if base.trim().is_empty() || key.trim().is_empty() || model.trim().is_empty() {
        return None;
    }
    let variant = std::env::var("OC_TEST_VARIANT")
        .ok()
        .filter(|v| !v.trim().is_empty());
    Some((
        ResponsesConfig {
            headers: BTreeMap::new(),
            set_cache_key: true,
            base_url: base,
            api_key: key,
            timeout: Some(false),
            chunk_timeout_ms: 300_000,
            connect_timeout: Duration::from_secs(30),
            allow_private: false,
        },
        model,
        variant,
    ))
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

/// Bounded live campaign: coding, reopen/next, compress, webfetch,
/// codex_web. Live model text is never asserted; behavior/files/tests are.
#[tokio::test]
#[ignore = "needs live OpenProxy credentials"]
async fn live_workflow_harness() {
    let Some((provider, model, variant)) = live_env() else {
        eprintln!("BUILD_READY_LIVE_BLOCKED: set LUDKA2_API_URL, LUDKA2_API_KEY, OC_TEST_MODEL");
        return;
    };
    let started = Instant::now();
    let mut steps = 0usize;

    let ws = tempfile::tempdir().expect("ws");
    let project = ws.path().join("proj");
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_dir(&here.join("../../fixtures/e2e-coding"), &project);
    let data = tempfile::tempdir().expect("data");
    let db = Db::open(data.path()).expect("db");
    let catalog = ModelCatalog {
        provider: "ludka2".to_string(),
        models: [(
            model.clone(),
            serde_json::json!({"limit": {"context": 1_000_000, "output": 100_000}}),
        )]
        .into_iter()
        .collect(),
    };
    let permissions: BTreeMap<String, Permission> = [
        "read",
        "apply_patch",
        "bash",
        "webfetch",
        "skill",
        "compress",
    ]
    .into_iter()
    .map(|name| (name.to_string(), Permission::Allow))
    .collect();
    let files = oc_adapters::files::Files::new(&project, data.path()).expect("files");
    let shell = oc_adapters::shell::Shell::new(&project).expect("shell");
    // Toolchain location for the scrubbed child env (same pattern as the
    // offline suite): absolute `cargo` in the prompt, `RUSTC` as a
    // caller-observed parent-env extra.
    let cargo = cargo_abs();
    let parent_env: BTreeMap<String, String> =
        [("RUSTC".to_string(), rustc_abs())].into_iter().collect();
    let runtime = Runtime::new(
        &db,
        "work",
        oc_adapters::config::Generation {
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions: permissions.clone(),
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        },
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
    runtime.create_session("live").expect("session");

    // 1. Coding: drive genuine continuation turns until the seeded bug is
    // fixed (or the budget runs out). Truncated gateway streams fail single
    // turns; every attempt is a new durable turn that sees prior history,
    // never a hidden retry of executed tools.
    let prompt = format!(
        "Fix the `add` function in src/lib.rs so tests pass. \
        First read src/lib.rs. \
        Use apply_patch with a *** Begin Patch / *** Update File: / @@ / +/-lines patch, \
        then run `{cargo} test` with the default working directory (no cwd override). \
        Reply briefly."
    );
    let mut fixed = false;
    for attempt in 0..4 {
        let candidate = runtime
            .run_turn(TurnParams {
                session: "live".to_string(),
                prompt: if attempt == 0 {
                    prompt.clone()
                } else {
                    format!("Continue: {prompt} Previous attempt stalled; re-read src/lib.rs and finish the fix.")
                },
                invocation: None,
                catalog: &catalog,
                model_id: model.clone(),
                variant: variant.clone(),
                max_output: 4_000,
                provider: provider.clone(),
                cancel: &NO_CANCEL,
                max_rounds: 10,
            })
            .await
            .expect("coding turn");
        steps += 1;
        let lib = std::fs::read_to_string(project.join("src/lib.rs")).expect("lib");
        if lib.contains("a + b") {
            fixed = true;
            break;
        }
        eprintln!(
            "coding turn attempt {attempt}: status={:?}, not fixed yet",
            candidate.status
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    assert!(fixed, "seeded bug fixed across continuation turns");
    // Pace live steps: the gateway truncates bursty streams.
    tokio::time::sleep(Duration::from_secs(5)).await;
    let lib = std::fs::read_to_string(project.join("src/lib.rs")).expect("lib");
    let output = std::process::Command::new(cargo_abs())
        .args(["test"])
        .current_dir(&project)
        .output()
        .expect("cargo test");
    assert!(output.status.success(), "fixture tests pass live");
    assert!(lib.contains("\"1.0.0\""), "protected API intact");

    // 2. Reopen + next command in the same session (retried pre-side-effect).
    runtime.open_session("live").expect("reopen");
    let mut next = None;
    for attempt in 0..3 {
        let candidate = runtime
            .run_turn(TurnParams {
                session: "live".to_string(),
                prompt: "Reply with the single word READY.".to_string(),
                invocation: None,
                catalog: &catalog,
                model_id: model.clone(),
                variant: variant.clone(),
                max_output: 100,
                provider: provider.clone(),
                cancel: &NO_CANCEL,
                max_rounds: 2,
            })
            .await
            .expect("next turn");
        if candidate.status == TurnStatus::Completed || !candidate.calls.is_empty() {
            next = Some(candidate);
            break;
        }
        eprintln!("next turn attempt {attempt}: transient failure, retrying");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let next = next.expect("next turn progressed");
    steps += 1;
    assert_eq!(next.status, TurnStatus::Completed);
    tokio::time::sleep(Duration::from_secs(5)).await;

    // 3. Compress, restart over the same root, continue.
    let ids = db.read_history_full("live").expect("ids");
    assert!(ids.len() >= 3);
    // The unfinished tail stays out of the range (product rule).
    let args = serde_json::json!({
        "topic": "live work",
        "content": [{
            "startId": ids[0].0, "endId": ids[ids.len() - 2].0,
            "summary": "fixed add; tests green",
        }],
    });
    let spec = ProtectedSpec {
        protect_user_messages: false,
        protect_tags: false,
        file_globs: Vec::new(),
        ..ProtectedSpec::default()
    };
    let compress = runtime
        .run_compress("live", &args, &spec)
        .expect("compress");
    steps += 1;
    assert!(!compress.blocks.is_empty());
    drop(runtime);
    let files2 = oc_adapters::files::Files::new(&project, data.path()).expect("files2");
    let shell2 = oc_adapters::shell::Shell::new(&project).expect("shell2");
    let runtime2 = Runtime::new(
        &db,
        "work",
        oc_adapters::config::Generation {
            providers: BTreeMap::new(),
            mcp: BTreeMap::new(),
            permissions,
            provenance: BTreeMap::new(),
            warnings: Vec::new(),
        },
        ProtectedGlobs {
            patterns: Vec::new(),
        },
        files2,
        shell2,
        std::env::vars().collect(),
        oc_adapters::tools::ToolRoots {
            project: project.clone(),
            data: data.path().to_path_buf(),
        },
        None,
        false,
        DcpConfig::default(),
    )
    .expect("runtime2");
    runtime2.open_session("live").expect("reopen after restart");
    let mut after = None;
    for attempt in 0..3 {
        let candidate = runtime2
            .run_turn(TurnParams {
                session: "live".to_string(),
                prompt: "Reply with the single word RESUMED.".to_string(),
                invocation: None,
                catalog: &catalog,
                model_id: model.clone(),
                variant: variant.clone(),
                max_output: 100,
                provider: provider.clone(),
                cancel: &NO_CANCEL,
                max_rounds: 2,
            })
            .await
            .expect("resumed turn");
        if candidate.status == TurnStatus::Completed || !candidate.calls.is_empty() {
            after = Some(candidate);
            break;
        }
        eprintln!("resumed turn attempt {attempt}: transient failure, retrying");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let after = after.expect("resumed turn progressed");
    steps += 1;
    assert_eq!(after.status, TurnStatus::Completed);

    // 4. codex_web search when an exact URL is configured (same key).
    if let Ok(url) = std::env::var("LUDKA2_MCP_URL")
        && !url.trim().is_empty()
    {
        let key = std::env::var("LUDKA2_API_KEY").expect("key");
        let config = oc_adapters::mcp_remote::CodexWebConfig {
            url,
            bearer: key,
            custom_headers: reqwest::header::HeaderMap::new(),
            timeout: Duration::from_secs(60),
            allow_private: false,
        };
        let client = oc_adapters::mcp_remote::CodexWebClient::connect(&config)
            .await
            .expect("codex_web connect");
        let text = client
            .search("oc smoke probe", Some("short"), &NO_CANCEL)
            .await
            .expect("codex_web search");
        steps += 1;
        assert!(!text.trim().is_empty());
    } else {
        eprintln!("codex_web skipped: LUDKA2_MCP_URL not set");
    }

    eprintln!(
        "LIVE_WORKFLOW_DONE steps={steps} elapsed_s={} model={model}",
        started.elapsed().as_secs()
    );
}
