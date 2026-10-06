//! TERM01 real rebuilt binary, not a projected TUI/owner mock. The same isolated
//! fixture can also be invoked against target/release/oc before release closure.
#[test]
fn term01_actual_binary_session_terminals() {
    let output = std::process::Command::new("python3")
        .args([
            "-c",
            include_str!("support/terminals.py"),
            env!("CARGO_BIN_EXE_oc"),
        ])
        .env_clear()
        .output()
        .expect("isolated native terminal fixture");
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
}
