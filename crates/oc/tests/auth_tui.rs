//! Rebuilt normal native TUI consumer; no issuer/token or model request fixture.
#[test]
fn auth05_actual_binary_tui_accounts_picker_and_owned_authorization() {
    let output = std::process::Command::new("python3")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", "/home/opencode/.cache/opencode-tmp/opencode")
        .arg("-B")
        .arg("-c")
        .arg(include_str!("support/auth_tui.py"))
        .arg(env!("CARGO_BIN_EXE_oc"))
        .output()
        .expect("isolated TUI auth fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
