//! SKILL01: the bundled native manual goes through the real binary/policy/output owner.
use super::*;

const ID: &str = "customize-opencode";
const BODY: &str = include_str!("../../../oc-adapters/assets/skills/customize-opencode/SKILL.md");

fn assert_sources_unchanged(fixture: &Fixture, config: &Path, original: &[u8]) {
    assert_eq!(fs::read(config).unwrap(), original);
    let mut entries = fs::read_dir(&fixture.global)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries, vec![std::ffi::OsString::from("opencode.json")]);
    assert!(!fixture.project_a.join(".opencode").exists());
    for name in [
        "opencode.json",
        "opencode.jsonc",
        "cli.json",
        "cli.jsonc",
        "oc-rs.toml",
        "dcp.json",
        "dcp.jsonc",
        "skills",
        "skill",
    ] {
        assert!(
            !fixture.project_a.join(name).exists(),
            "created source {name}"
        );
    }
    fixture.assert_no_loader_execution();
}

#[test]
fn skill01_binary_embeds_the_full_manual_and_preserves_stricter_user_limits() {
    let fixture = Fixture::new();
    for strict in [false, true] {
        let mut additions =
            json!({"permissions":[{"action": "skill", "resource": "*", "effect": "allow"}]});
        if strict {
            additions["tool_output"] = json!({"max_bytes":128,"max_lines":4});
        }
        let config = fixture.write_base_config(additions);
        let original = fs::read(&config).unwrap();
        let label = if strict {
            "skill01-strict"
        } else {
            "skill01-default"
        };
        let mut process = fixture.spawn(
            &fixture.project_a,
            &format!("s-{label}"),
            "Load the native configuration manual",
            label,
        );
        let (mut socket, first) = fixture.accept(&mut process);
        assert_eq!(count_tool(&first, "skill"), 1);
        let visible = fixed_text(&first);
        assert!(visible.contains(ID), "bundled metadata absent");
        assert!(visible.contains("Configure the native Rust oc itself"));
        assert!(
            !visible.contains("# Configure native oc"),
            "body leaked before loading"
        );
        assert!(!visible.contains(BODY.lines().last().unwrap()));
        assert_sources_unchanged(&fixture, &config, &original);
        respond_tool(&mut socket, "skill", json!({"id":ID}), label);
        let (mut socket, next) = fixture.accept(&mut process);
        let output = function_output(&next, label);
        if strict {
            assert!(
                output.starts_with("---\nname: customize-opencode\n"),
                "{output}"
            );
            assert!(!output.contains("## Before finishing"));
            assert!(output.len() < BODY.len());
            assert!(
                output.contains("[tool output:"),
                "bounded capture notice missing: {output}"
            );
        } else {
            assert!(
                output.contains(BODY),
                "default output did not return the matching complete bundled body: {output}"
            );
            assert!(output.contains(BODY.lines().last().unwrap()));
        }
        respond_text(&mut socket, "native manual checked");
        fixture.respond_title(&mut process);
        assert!(process.wait().success(), "{}", process.diagnostics());
        fixture.assert_no_request();
        assert_sources_unchanged(&fixture, &config, &original);
    }
}

#[test]
fn skill01_binary_per_id_deny_removes_metadata_and_rejects_a_forced_call() {
    let fixture = Fixture::new();
    let config =
        fixture.write_base_config(json!({"permissions":[{"action":"skill","resource":"*","effect":"allow"},{"action":"skill","resource":ID,"effect":"deny"}]}));
    let original = fs::read(&config).unwrap();
    let mut process = fixture.spawn(
        &fixture.project_a,
        "s-skill01-deny",
        "Do not load unavailable skills",
        "skill01-deny",
    );
    let (mut socket, first) = fixture.accept(&mut process);
    assert!(
        !fixed_text(&first).contains(ID),
        "denied metadata advertised"
    );
    assert!(!first.to_string().contains("# Configure native oc"));
    respond_tool(&mut socket, "skill", json!({"id":ID}), "skill01-forced");
    let (mut socket, next) = fixture.accept(&mut process);
    let output = function_output(&next, "skill01-forced");
    assert!(
        output.starts_with("error:"),
        "denied call was not an error: {output}"
    );
    assert!(!output.contains("# Configure native oc"));
    assert!(!output.contains(BODY.lines().last().unwrap()));
    respond_text(&mut socket, "skill denied");
    fixture.respond_title(&mut process);
    assert!(process.wait().success(), "{}", process.diagnostics());
    fixture.assert_no_request();
    assert_sources_unchanged(&fixture, &config, &original);
}

#[test]
fn skill01_binary_fresh_headless_ask_stops_before_execution_intent() {
    let fixture = Fixture::new();
    let config = fixture.write_base_config(json!({"permissions":[{"action":"skill","resource":"*","effect":"allow"},{"action":"skill","resource":ID,"effect":"ask"}]}));
    let original = fs::read(&config).unwrap();
    let session = "s-skill01-ask";
    let mut process = fixture.spawn(
        &fixture.project_a,
        session,
        "Load the native configuration manual with approval",
        "skill01-ask",
    );
    let (mut socket, first) = fixture.accept(&mut process);
    assert!(fixed_text(&first).contains(ID));
    respond_tool(&mut socket, "skill", json!({"id":ID}), "skill01-ask");
    drop(socket);
    // The ancillary title is independent; no second main generation is permitted.
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match fixture.listener.accept() {
            Ok((socket, _)) => {
                if let Some((mut socket, request)) = read_request(socket) {
                    assert!(
                        is_title(&request),
                        "headless Ask continued to the model: {request}"
                    );
                    process.titles += 1;
                    assert_eq!(process.titles, 1);
                    respond_text(&mut socket, "Configuration approval");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("accept title: {error}"),
        }
        if let Some(status) = process.child.try_wait().unwrap() {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "headless Ask did not terminate: {}",
            process.diagnostics()
        );
        std::thread::sleep(POLL);
    };
    assert!(!status.success());
    assert!(
        process.diagnostics().contains("approval_required"),
        "{}",
        process.diagnostics()
    );
    assert!(!process.output().contains("# Configure native oc"));
    fixture.assert_no_request();
    let db = rusqlite::Connection::open_with_flags(
        fixture.home.join("data/oc/oc.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let intents: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM tool_operations WHERE session_id=?1 AND name='skill'",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        intents, 0,
        "headless Ask crossed the execution-intent boundary"
    );
    assert_sources_unchanged(&fixture, &config, &original);
}
