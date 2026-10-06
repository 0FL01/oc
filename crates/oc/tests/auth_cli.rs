//! Current native ELF consumer, isolated pipes and real controlling-terminal input.
#[test]
fn auth05_actual_binary_cli_metadata_masking_and_owned_callback() {
    let output = std::process::Command::new("python3")
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", "/home/opencode/.cache/opencode-tmp/opencode")
        .arg("-B")
        .arg("-c")
        .arg(include_str!("support/auth_cli.py"))
        .arg(env!("CARGO_BIN_EXE_oc"))
        .output()
        .expect("isolated auth fixture");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
