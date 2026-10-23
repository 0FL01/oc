//! Actual binary/PTY geometry regression. Independent paired styled captures
//! live in evidence/tui/recovery-v03; these assertions are not parity goldens.
use std::process::Command;

#[test]
fn binary_viewport_resize_draft_sidebar_and_debug() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let config = home.join("config/opencode");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(root.path().join("project")).unwrap();
    std::fs::write(config.join("opencode.json"), serde_json::json!({
        "model":"fixture/geometry", "providers":{"fixture":{
            "package":"@ai-sdk/openai", "name":"Geometry provider",
            "settings":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"test"},
            "models":{"geometry":{"name":"Geometry model","limit":{"context":10000,"output":1000}}}
        }}
    }).to_string()).unwrap();
    let db = oc_adapters::storage::Db::open(&home.join("data/oc")).unwrap();
    db.create_session("long").unwrap();
    db.create_session("wrapped").unwrap();
    db.create_session("empty").unwrap();
    db.create_session("foreign").unwrap();
    db.set_pref(
        &format!("{}foreign", oc_adapters::runtime::SESSION_LOCATION_PREFIX),
        &root.path().join("other-project").to_string_lossy(),
    )
    .unwrap();
    db.create_child_session("long", "child", None, None, Some("Real child title"))
        .unwrap();
    db.create_child_session("long", "ses_v03_child", None, None, Some("Geometry child"))
        .unwrap();
    for session in ["long", "wrapped", "empty", "child", "ses_v03_child"] {
        db.set_pref(
            &format!("{}{session}", oc_adapters::runtime::SESSION_LOCATION_PREFIX),
            &root.path().join("project").to_string_lossy(),
        )
        .unwrap();
    }
    let text = (0..90).map(|i| format!("ROW-{i:03}\n")).collect::<String>();
    let wrapped = format!(
        "FIRST-ANCHOR {} LAST-ANCHOR",
        "wrapped payload ".repeat(800)
    );
    for (session, text) in [
        ("long", text.as_str()),
        ("child", text.as_str()),
        ("wrapped", wrapped.as_str()),
    ] {
        let message = db.append_message(session, "assistant", text).unwrap();
        // Persist measured display metadata on the existing message. Text size
        // alone is deliberately not a source of context accounting in the UI.
        let turn = format!("geometry-{session}");
        db.begin_turn(&turn, session, "seeded history").unwrap();
        db.finish_turn(
            &turn,
            "completed",
            Some(
                &serde_json::json!({
                    "assistant_message": message,
                    "display_parts": [{"message": 0}],
                    "input": [{"content": [{"type": "output_text", "text": text}]}],
                    "display": {"context_usage": [8000, 380], "context_usage_limit": 0}
                })
                .to_string(),
            ),
        )
        .unwrap();
    }
    // This genuinely empty session has no usage/message to invent. Its title
    // is a spatial sidebar witness instead of an invalid Context placeholder.
    rusqlite::Connection::open(home.join("data/oc/oc.sqlite"))
        .unwrap()
        .execute(
            "UPDATE sessions SET title = 'Empty sidebar witness' WHERE id = 'empty'",
            [],
        )
        .unwrap();
    drop(db);
    // Optional qualification uses the same real persisted fixtures and external
    // xterm/Chromium runner. Normal Cargo tests have no Node/browser dependency.
    if let Ok(output) = std::env::var("OC_V03_CAPTURE_OUTPUT") {
        let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (suffix, session) in [("child", "ses_v03_child"), ("query", "foreign")] {
            let mut command = Command::new("node");
            command
                .current_dir(&workspace)
                .args([
                    "scripts/tui_capture/capture.mjs",
                    "--geometry",
                    "true",
                    "--oc",
                    env!("CARGO_BIN_EXE_oc2"),
                    "--build-oc",
                    "true",
                    "--seed-root",
                ])
                .arg(root.path())
                .args(["--session", session, "--output"])
                .arg(format!("{output}-{suffix}"));
            if suffix == "child" {
                command.args(["--reference", "/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode"]);
            }
            let capture = command.output().unwrap();
            println!("{}", String::from_utf8_lossy(&capture.stdout));
            assert_eq!(
                capture.status.code(),
                Some(1),
                "diagnostic comparisons/missing pairs remain nonzero"
            );
            let lock: serde_json::Value = serde_json::from_slice(
                &std::fs::read(format!("{output}-{suffix}/capture.lock.json")).unwrap(),
            )
            .unwrap();
            let attempts = lock["attempts"].as_array().unwrap();
            assert!(
                !attempts.iter().any(|a| a["status"] == "FAILED"),
                "{attempts:?}"
            );
            assert!(
                attempts
                    .iter()
                    .any(|a| a["origin"] == "oc" && a["status"] == "EXECUTED_NATIVE_ROUTE")
            );
        }
    }
    let geometry = include_str!("support/geometry.py");
    let output = Command::new("python3")
        .args(["-c", geometry, env!("CARGO_BIN_EXE_oc2")])
        .arg(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
