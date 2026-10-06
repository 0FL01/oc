//! Actual binary fake authorization/request gate; never enabled in a default build.
#[cfg(feature = "auth-fixture")]
#[test]
fn auth02_auth04_binary_device_browser_and_native_requests() {
    let output = std::process::Command::new("python3")
        .args([
            "-B",
            "-c",
            include_str!("support/auth_oauth.py"),
            env!("CARGO_BIN_EXE_oc"),
        ])
        .env_clear()
        .env("TMPDIR", "/home/opencode/.cache/opencode-tmp/opencode")
        .output()
        .expect("start isolated authorization fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
