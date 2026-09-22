//! Actual binary startup routes. The fixture lives outside the workspace and
//! carries only a static, non-network provider; no turn is submitted.
use std::process::Command;

#[tokio::test]
async fn location_switch_carries_safe_category_and_keeps_detailed_api_error() {
    use oc_core::session::{CoreError, LocationSwitchFailure};
    use std::collections::BTreeMap;

    let root = tempfile::Builder::new()
        .prefix("oc-startup-switch-")
        .tempdir_in("/home/opencode/.cache/opencode-tmp/opencode")
        .unwrap();
    let home = root.path().join("home");
    let original = root.path().join("original");
    let bad = root.path().join("bad");
    std::fs::create_dir_all(home.join("config")).unwrap();
    std::fs::create_dir_all(&original).unwrap();
    std::fs::create_dir_all(&bad).unwrap();
    let config = |model: &str| {
        serde_json::json!({
            "model":format!("fixture/{model}"),
            "provider":{"fixture":{
                "npm":"@ai-sdk/openai", "name":"Offline",
                "options":{"baseURL":"http://127.0.0.1:9/v1","apiKey":"dummy"},
                "models":{"offline":{"name":"Offline","limit":{"context":10000,"output":1000}}}
            }}
        })
        .to_string()
    };
    std::fs::write(original.join("opencode.json"), config("offline")).unwrap();
    std::fs::write(bad.join("opencode.json"), config("LEAKME-MODEL-SECRET")).unwrap();
    let env = BTreeMap::from([
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().into_owned(),
        ),
    ]);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&original, &home.join("data/oc"), env)
            .await
            .unwrap();
    let before = app.catalog().await.unwrap();
    let error = app
        .switch_location(bad.to_string_lossy().into_owned())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::LocationSwitch {
            category: LocationSwitchFailure::Configuration,
            ..
        }
    ));
    // Existing API callers retain the detailed explanation, while the TUI
    // matches the category without formatting this error.
    assert!(error.to_string().contains("LEAKME-MODEL-SECRET"));
    assert_eq!(app.catalog().await.unwrap(), before);
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

#[test]
fn isolated_binary_startup_routes() {
    let output = Command::new("python3")
        .args([
            "-c",
            include_str!("support/startup.py"),
            env!("CARGO_BIN_EXE_oc"),
        ])
        .env_clear()
        .output()
        .expect("python PTY harness");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
