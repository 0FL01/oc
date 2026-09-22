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
        "model":"fixture/geometry", "provider":{"fixture":{
            "npm":"@ai-sdk/openai", "name":"Geometry provider",
            "options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"test"},
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
    for session in ["long", "child"] {
        db.append_message(session, "assistant", &text).unwrap();
    }
    db.append_message(
        "wrapped",
        "assistant",
        &format!(
            "FIRST-ANCHOR {} LAST-ANCHOR",
            "wrapped payload ".repeat(800)
        ),
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
                    env!("CARGO_BIN_EXE_oc"),
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
    let output = Command::new("python3")
        .args([
            "-c",
            include_str!("support/geometry.py"),
            env!("CARGO_BIN_EXE_oc"),
        ])
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
