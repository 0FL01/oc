use std::process::Command;

const CANARY: &str = "CFG10-PRIVATE-SOURCE-SECRET";

#[test]
fn cfg10_native_malformed_document_has_safe_cause_and_empty_json_stdout() {
    let root = tempfile::Builder::new()
        .prefix("oc-cfg10-")
        .tempdir_in("/home/opencode/.cache/opencode-tmp/opencode")
        .unwrap();
    let project = root.path().join(format!("{CANARY}\u{1b}[31m"));
    let home = root.path().join("home");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(home.join("config/opencode")).unwrap();
    std::fs::write(
        project.join("opencode.jsonc"),
        format!("{{\"apiKey\":\"{CANARY}\", \"permission\": {{ INVALID }}"),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_oc"))
        .current_dir(&project)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .args(["run", "--json", "must not be accepted"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    for expected in [
        "source-",
        "document",
        "config",
        "invalid_document",
        "retryable=false",
        "review configuration",
    ] {
        assert!(stderr.contains(expected), "missing {expected}: {stderr}");
    }
    assert!(!stderr.contains(CANARY));
    assert!(!stderr.contains(&root.path().to_string_lossy().to_string()));
    assert!(!stderr.contains('\u{1b}'));
    assert!(!home.join("data/oc").exists());
}
