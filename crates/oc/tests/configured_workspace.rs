//! AUD14-AUD17: configured-workspace regressions through the actual `oc` binary.

#![cfg(target_os = "linux")]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(10);
const MODEL: &str = "aud-configured-workspace-model";
const GLOBAL_RULE: &str = "AUD14_GLOBAL_RULE_750aa4";
const A_RULE: &str = "AUD14_A_RULE_609c89";
const B_RULE: &str = "AUD14_B_RULE_286d1f";
const A_AGENT_BODY: &str = "AUD15_PRIMARY_A_BODY_b62aca";
const B_AGENT_BODY: &str = "AUD15_PRIMARY_B_BODY_f93d2e";
const OLD_SKILL_BODY: &str = "AUD17_PINNED_SKILL_OLD_31b624";
const NEW_SKILL_BODY: &str = "AUD17_MUTATED_SKILL_NEW_a8315b";

struct Fixture {
    _root: tempfile::TempDir,
    home: PathBuf,
    global: PathBuf,
    project_a: PathBuf,
    project_b: PathBuf,
    listener: TcpListener,
    endpoint: String,
    exec_log: PathBuf,
    trap_bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("isolated configured-workspace fixture");
        let home = root.path().join("home");
        let global = home.join("config/opencode");
        let project_a = root.path().join("project-a");
        let project_b = root.path().join("project-b");
        let trap_bin = home.join("trap-bin");
        for dir in [&global, &project_a, &project_b, &trap_bin] {
            fs::create_dir_all(dir).expect("fixture directory");
        }
        let exec_log = home.join("unexpected-exec.log");
        for program in ["cargo", "node", "bun", "npx"] {
            let path = trap_bin.join(program);
            fs::write(
                &path,
                "#!/bin/sh\nprintf '%s\\n' \"$0 $*\" >> \"$OC_EXEC_LOG\"\nexit 97\n",
            )
            .expect("trap executable");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                .expect("trap permissions");
        }
        let listener = TcpListener::bind("127.0.0.1:0").expect("fake Responses endpoint");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let endpoint = format!(
            "http://{}/proxy/v1",
            listener.local_addr().expect("address")
        );
        Self {
            _root: root,
            home,
            global,
            project_a,
            project_b,
            listener,
            endpoint,
            exec_log,
            trap_bin,
        }
    }

    fn base_config(&self) -> Value {
        json!({
            "model": format!("fixture/{MODEL}"),
            "provider": {"fixture": {
                "npm": "@ai-sdk/openai",
                "options": {"baseURL": self.endpoint, "apiKey": "offline-fixture-key"},
                "models": {MODEL: {
                    "name": "Unknown static fixture model",
                    "limit": {"context": 65536, "output": 4096}
                }}
            }}
        })
    }

    fn write_base_config(&self, additions: Value) -> PathBuf {
        let mut config = self.base_config();
        merge_object(&mut config, additions);
        let path = self.global.join("opencode.json");
        write(&path, &config.to_string());
        path
    }

    fn spawn(&self, project: &Path, session: &str, prompt: &str, label: &str) -> Process {
        let stdout = self.home.join(format!("{label}.stdout"));
        let stderr = self.home.join(format!("{label}.stderr"));
        let child = Command::new(env!("CARGO_BIN_EXE_oc"))
            .env_clear()
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join("config"))
            .env("XDG_DATA_HOME", self.home.join("data"))
            .env("XDG_CACHE_HOME", self.home.join("cache"))
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env("OC_TEST_ALLOW_LOOPBACK", "1")
            .env("OC_EXEC_LOG", &self.exec_log)
            .env("PATH", &self.trap_bin)
            .current_dir(project)
            .args(["run", "--session", session, prompt])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).expect("stdout file"))
            .stderr(fs::File::create(&stderr).expect("stderr file"))
            .spawn()
            .expect("actual oc binary");
        Process {
            child,
            stdout,
            stderr,
        }
    }

    fn accept(&self, process: &mut Process) -> (TcpStream, Value) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match self.listener.accept() {
                Ok((socket, _)) => return read_request(socket),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = process.child.try_wait().expect("poll actual binary") {
                        panic!(
                            "actual oc exited before provider request ({status}): {}",
                            process.diagnostics()
                        );
                    }
                    assert!(
                        Instant::now() < deadline,
                        "actual oc never contacted fake Responses endpoint: {}",
                        process.diagnostics()
                    );
                    std::thread::sleep(POLL);
                }
                Err(error) => panic!("accept provider request: {error}"),
            }
        }
    }

    fn assert_no_request(&self) {
        assert_eq!(
            self.listener
                .accept()
                .expect_err("unexpected provider request")
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    fn wait_for_preflight_failure(&self, process: &mut Process) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match self.listener.accept() {
                Ok((socket, _)) => {
                    let (_, request) = read_request(socket);
                    panic!("invalid selected definition reached the provider: {request}");
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => panic!("poll fake endpoint: {error}"),
            }
            if let Some(status) = process.child.try_wait().expect("poll actual binary") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "actual oc did not finish preflight: {}",
                process.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }

    fn assert_no_loader_execution(&self) {
        assert!(
            !self.exec_log.exists(),
            "config/definition loader executed an external program: {}",
            fs::read_to_string(&self.exec_log).unwrap_or_default()
        );
    }
}

struct Process {
    child: Child,
    stdout: PathBuf,
    stderr: PathBuf,
}

impl Process {
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().expect("bounded child wait") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "actual oc timeout: {}",
                self.diagnostics()
            );
            std::thread::sleep(POLL);
        }
    }

    fn diagnostics(&self) -> String {
        fs::read_to_string(&self.stderr).unwrap_or_default()
    }

    fn output(&self) -> String {
        fs::read_to_string(&self.stdout).unwrap_or_default()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let deadline = Instant::now() + TIMEOUT;
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(POLL);
        }
    }
}

#[test]
fn aud14_aud15_aud17_binary_loads_isolated_a_b_workspace_and_pins_skill() {
    let fixture = Fixture::new();
    fixture.write_base_config(json!({
        "plugin": [
            "@tarquinen/opencode-dcp@latest",
            "@tarquinen/opencode-dcp",
            "@tarquinen/opencode-dcp",
            "@prevalentware/opencode-goal-plugin@0.1.49"
        ],
        "permissions": {"skill": "allow", "read": "allow"}
    }));
    write(&fixture.global.join("AGENTS.md"), GLOBAL_RULE);
    write_skill(
        &fixture.global.join("skills/global-guide/SKILL.md"),
        "global-guide",
        "AUD17_GLOBAL_SKILL_METADATA_65e0a2",
        "AUD17_GLOBAL_SKILL_BODY_36ed24",
    );
    write_agent(
        &fixture.global.join("agents/global-unused.md"),
        "Global profile",
        "AUD15_GLOBAL_UNUSED_AGENT_2bb785",
        None,
    );
    write(
        &fixture.global.join("commands/global-unused.md"),
        "---\ndescription: global command\n---\nAUD15_GLOBAL_COMMAND_7e93fb $ARGUMENTS\n",
    );

    write(
        &fixture.project_a.join("opencode.jsonc"),
        "{\n  // The selected profile must contribute its body, not its description.\n  \"default_agent\": \"alpha\",\n}\n",
    );
    write(&fixture.project_a.join("AGENTS.md"), A_RULE);
    let a_skill = fixture.project_a.join(".opencode/skills/snapshot/SKILL.md");
    write_skill(
        &a_skill,
        "snapshot",
        "AUD17_SNAPSHOT_SKILL_METADATA_b976f2",
        OLD_SKILL_BODY,
    );
    write_agent(
        &fixture.project_a.join(".opencode/agents/alpha.md"),
        "Same profile description",
        A_AGENT_BODY,
        None,
    );
    let command_body = concat!(
        "Literal markdown command.\n",
        "`cargo test`\n",
        "```sh\n",
        "cargo test --locked\n",
        "```\n",
        "all=<$ARGUMENTS>\n",
        "one=<$1>\n",
        "two=<$2>\n"
    );
    write(
        &fixture.project_a.join(".opencode/commands/literal.md"),
        &format!("---\ndescription: literal expansion regression\n---\n{command_body}"),
    );

    write(
        &fixture.project_b.join("opencode.json"),
        "{\"default_agent\":\"beta\"}",
    );
    write(&fixture.project_b.join("AGENTS.md"), B_RULE);
    write_skill(
        &fixture.project_b.join(".opencode/skills/b-only/SKILL.md"),
        "b-only",
        "AUD17_B_SKILL_METADATA_27cbc4",
        "AUD17_B_SKILL_BODY_557abd",
    );
    write_agent(
        &fixture.project_b.join(".opencode/agents/beta.md"),
        "Same profile description",
        B_AGENT_BODY,
        None,
    );
    write(
        &fixture.project_b.join(".opencode/commands/b-only.md"),
        "---\ndescription: project B command\n---\nAUD15_B_COMMAND_f72b4b $1\n",
    );

    let invocation = "/literal value$2 second";
    let expected_command = concat!(
        "Literal markdown command.\n",
        "`cargo test`\n",
        "```sh\n",
        "cargo test --locked\n",
        "```\n",
        "all=<value$2 second>\n",
        "one=<value$2>\n",
        "two=<second>\n"
    );
    assert!(
        expected_command.contains("all=<value$2 second>")
            && expected_command.contains("one=<value$2>")
            && expected_command.contains("two=<second>"),
        "test fixture itself must model single-pass expansion"
    );

    let mut a = fixture.spawn(&fixture.project_a, "s-aud14-a", invocation, "project-a");
    let (mut socket, first_a) = fixture.accept(&mut a);
    assert_eq!(first_a["model"], MODEL);
    assert_fixed_marker(&first_a, GLOBAL_RULE);
    assert_fixed_marker(&first_a, A_RULE);
    assert_absent(&first_a, B_RULE);
    assert_fixed_marker(&first_a, A_AGENT_BODY);
    assert_absent(&first_a, B_AGENT_BODY);
    assert_fixed_order(&first_a, &[A_AGENT_BODY, GLOBAL_RULE, A_RULE]);
    assert_eq!(
        last_message_text(&first_a, "user").as_deref(),
        Some(expected_command),
        "custom command is one literal SubmitInput expansion"
    );
    // Successful startup with duplicate exact DCP aliases and loader traps proves
    // native admission/no JS execution. Model-visible compress is qualified by T36.
    assert_eq!(count_tool(&first_a, "skill"), 1);
    let visible = first_a.to_string();
    assert!(visible.contains("AUD17_SNAPSHOT_SKILL_METADATA_b976f2"));
    assert!(visible.contains("AUD17_GLOBAL_SKILL_METADATA_65e0a2"));
    assert!(
        !visible.contains(OLD_SKILL_BODY),
        "skill body leaked before call"
    );
    assert!(!visible.contains(NEW_SKILL_BODY));
    fixture.assert_no_loader_execution();

    write_skill(
        &a_skill,
        "snapshot",
        "AUD17_SNAPSHOT_SKILL_METADATA_b976f2",
        NEW_SKILL_BODY,
    );
    respond_tool(
        &mut socket,
        "skill",
        json!({"id": "snapshot"}),
        "aud17-skill",
    );
    let (mut socket, second_a) = fixture.accept(&mut a);
    let skill_result = function_output(&second_a, "aud17-skill");
    assert!(skill_result.contains(OLD_SKILL_BODY), "{skill_result}");
    assert!(
        !skill_result.contains(NEW_SKILL_BODY),
        "skill was reread after generation publication: {skill_result}"
    );
    assert_fixed_marker(&second_a, GLOBAL_RULE);
    assert_fixed_marker(&second_a, A_RULE);
    respond_text(&mut socket, "project A complete");
    assert!(a.wait().success(), "{}", a.diagnostics());
    assert_eq!(a.output().trim(), "project A complete");
    assert!(
        a.diagnostics().contains("authoring-only plugin")
            && a.diagnostics().contains("no package code was loaded"),
        "{}",
        a.diagnostics()
    );

    let mut b = fixture.spawn(
        &fixture.project_b,
        "s-aud14-b",
        "ordinary project B prompt",
        "project-b",
    );
    let (mut socket, first_b) = fixture.accept(&mut b);
    assert_fixed_marker(&first_b, GLOBAL_RULE);
    assert_fixed_marker(&first_b, B_RULE);
    assert_fixed_marker(&first_b, B_AGENT_BODY);
    assert_absent(&first_b, A_RULE);
    assert_absent(&first_b, A_AGENT_BODY);
    assert_absent(&first_b, "AUD17_SNAPSHOT_SKILL_METADATA_b976f2");
    assert!(
        first_b
            .to_string()
            .contains("AUD17_B_SKILL_METADATA_27cbc4")
    );
    assert_ne!(fixed_text(&first_a), fixed_text(&first_b));
    respond_text(&mut socket, "project B complete");
    assert!(b.wait().success(), "{}", b.diagnostics());

    let mut cross = fixture.spawn(
        &fixture.project_b,
        "s-aud14-a",
        "must not cross locations",
        "cross-location",
    );
    let status = cross.wait();
    assert!(!status.success());
    let diagnostic = cross.diagnostics();
    assert!(diagnostic.contains("s-aud14-a"), "{diagnostic}");
    assert!(diagnostic.contains("belongs to location"), "{diagnostic}");
    assert!(diagnostic.contains(&fixture.project_a.to_string_lossy().to_string()));
    fixture.assert_no_request();
    fixture.assert_no_loader_execution();
}

#[test]
fn aud16_binary_legacy_write_edit_deny_blocks_apply_patch_side_effect() {
    let fixture = Fixture::new();
    fixture.write_base_config(json!({
        "permissions": {
            "apply_patch": "allow",
            "write": "deny",
            "edit": "deny"
        }
    }));
    run_denied_patch(
        &fixture,
        &fixture.project_a,
        "s-aud16-legacy",
        "legacy-denied.txt",
        "legacy",
    );
}

#[test]
fn aud16_binary_selected_agent_can_only_narrow_apply_patch_permission() {
    let fixture = Fixture::new();
    fixture.write_base_config(json!({"permissions": {"apply_patch": "allow"}}));
    write(
        &fixture.project_a.join("opencode.json"),
        "{\"default_agent\":\"narrow\"}",
    );
    write_agent(
        &fixture.project_a.join(".opencode/agents/narrow.md"),
        "Narrowing primary profile",
        "AUD16_NARROW_AGENT_BODY_f58d08",
        Some("deny"),
    );
    run_denied_patch(
        &fixture,
        &fixture.project_a,
        "s-aud16-agent",
        "agent-denied.txt",
        "agent",
    );
}

#[test]
fn aud17_binary_unknown_plugin_is_precise_preflight_error_without_side_effect() {
    let fixture = Fixture::new();
    // If admission accidentally resolves this URL, the same loopback listener
    // records it. A valid provider request would use the distinct /proxy/v1 path.
    let plugin = format!(
        "http://{}/plugins/not-native.js",
        fixture.listener.local_addr().expect("listener address")
    );
    let config = fixture.write_base_config(json!({"plugin": [plugin.clone()]}));
    let before = fs::read(&config).expect("config before");
    let mut process = fixture.spawn(
        &fixture.project_a,
        "s-aud17-unknown-plugin",
        "must fail before provider",
        "unknown-plugin",
    );
    let status = process.wait();
    assert!(!status.success());
    assert!(process.output().is_empty());
    let diagnostic = process.diagnostics();
    assert_eq!(diagnostic.lines().count(), 1, "{diagnostic}");
    assert!(diagnostic.contains("UnsupportedPlugin"), "{diagnostic}");
    assert!(diagnostic.contains(&plugin));
    assert!(diagnostic.contains(&config.to_string_lossy().to_string()));
    assert_eq!(fs::read(&config).expect("config after"), before);
    assert!(!fixture.home.join("data/oc").exists());
    fixture.assert_no_request();
    fixture.assert_no_loader_execution();
}

#[test]
fn aud17_binary_selected_malformed_agent_is_hard_error_not_sibling_fallback() {
    let fixture = Fixture::new();
    fixture.write_base_config(json!({"permissions": {"read": "allow"}}));
    write(
        &fixture.project_a.join("opencode.json"),
        "{\"default_agent\":\"broken\"}",
    );
    write_agent(
        &fixture.project_a.join(".opencode/agents/good.md"),
        "Valid sibling",
        "AUD17_VALID_AGENT_SIBLING_38890d",
        None,
    );
    let broken = fixture.project_a.join(".opencode/agents/broken.md");
    write(
        &broken,
        "---\ndescription: selected malformed agent\nmode: subagent\n---\nMUST_NOT_FALL_BACK\n",
    );
    let mut process = fixture.spawn(
        &fixture.project_a,
        "s-aud17-broken-agent",
        "must not use valid sibling",
        "broken-agent",
    );
    assert!(!fixture.wait_for_preflight_failure(&mut process).success());
    assert!(process.output().is_empty());
    let diagnostic = process.diagnostics();
    assert!(diagnostic.contains("broken"), "{diagnostic}");
    assert!(diagnostic.contains("subagent"), "{diagnostic}");
    assert!(diagnostic.contains(&broken.to_string_lossy().to_string()));
    fixture.assert_no_request();
}

#[test]
fn aud17_binary_malformed_selected_skill_is_visible_and_valid_sibling_survives() {
    let fixture = Fixture::new();
    fixture.write_base_config(json!({"permissions": {"skill": "allow"}}));
    write_skill(
        &fixture.project_a.join(".opencode/skills/good/SKILL.md"),
        "good",
        "AUD17_VALID_SKILL_SIBLING_ee4b1f",
        "AUD17_VALID_SKILL_BODY_45f4e2",
    );
    let malformed = fixture.project_a.join(".opencode/skills/broken/SKILL.md");
    write(&malformed, "missing frontmatter and metadata\n");
    let mut process = fixture.spawn(
        &fixture.project_a,
        "s-aud17-broken-skill",
        "ask for a malformed selected skill",
        "broken-skill",
    );
    let (mut socket, first) = fixture.accept(&mut process);
    let visible = first.to_string();
    assert!(visible.contains("AUD17_VALID_SKILL_SIBLING_ee4b1f"));
    assert!(!visible.contains("AUD17_VALID_SKILL_BODY_45f4e2"));
    respond_tool(
        &mut socket,
        "skill",
        json!({"id": "broken"}),
        "aud17-broken-skill",
    );
    let (mut socket, second) = fixture.accept(&mut process);
    let result = function_output(&second, "aud17-broken-skill");
    assert!(result.contains("broken"), "{result}");
    assert!(
        result.contains("frontmatter") || result.contains("malformed"),
        "selected malformed skill was silently treated as unknown: {result}"
    );
    respond_text(&mut socket, "malformed skill handled");
    assert!(process.wait().success(), "{}", process.diagnostics());
    let diagnostic = process.diagnostics();
    assert!(diagnostic.contains(&malformed.to_string_lossy().to_string()));
    assert!(diagnostic.contains("frontmatter"), "{diagnostic}");
}

fn run_denied_patch(fixture: &Fixture, project: &Path, session: &str, target: &str, label: &str) {
    let mut process = fixture.spawn(project, session, "attempt a denied patch", label);
    let (mut socket, request) = fixture.accept(&mut process);
    assert_eq!(count_tool(&request, "apply_patch"), 1);
    let call_id = format!("aud16-{label}");
    respond_tool(
        &mut socket,
        "apply_patch",
        json!({
            "patchText": format!(
                "*** Begin Patch\n*** Add File: {target}\n+forbidden side effect\n*** End Patch"
            )
        }),
        &call_id,
    );
    let (mut socket, continuation) = fixture.accept(&mut process);
    let outcome = function_output(&continuation, &call_id);
    assert!(
        outcome.to_ascii_lowercase().contains("denied") && outcome.contains("apply_patch"),
        "executor did not return a precise policy denial: {outcome}"
    );
    assert!(
        !project.join(target).exists(),
        "denied apply_patch changed the project"
    );
    respond_text(&mut socket, "denial observed");
    assert!(process.wait().success(), "{}", process.diagnostics());
    assert!(!project.join(target).exists());
}

fn merge_object(target: &mut Value, additions: Value) {
    let target = target.as_object_mut().expect("base object");
    for (key, value) in additions.as_object().expect("additions object") {
        target.insert(key.clone(), value.clone());
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("parent directory")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

fn write_skill(path: &Path, name: &str, description: &str, body: &str) {
    write(
        path,
        &format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n"),
    );
}

fn write_agent(path: &Path, description: &str, body: &str, apply_patch_permission: Option<&str>) {
    let permission = apply_patch_permission.map_or_else(String::new, |level| {
        format!("permission:\n  apply_patch: {level}\n")
    });
    write(
        path,
        &format!("---\ndescription: {description}\n{permission}---\n{body}\n"),
    );
}

fn read_request(mut socket: TcpStream) -> (TcpStream, Value) {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("request read timeout");
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .expect("response write timeout");
    let deadline = Instant::now() + TIMEOUT;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        assert!(Instant::now() < deadline, "HTTP header deadline");
        let read = socket.read(&mut chunk).expect("request headers");
        assert_ne!(read, 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..read]);
        assert!(bytes.len() < 65_536, "bounded request headers");
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end]).expect("HTTP headers");
    assert!(headers.starts_with("POST /proxy/v1/responses HTTP/1.1\r\n"));
    let length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().expect("content length"))
        })
        .expect("content-length");
    assert!(length < 2 * 1024 * 1024, "bounded request body");
    while bytes.len() < header_end + length {
        assert!(Instant::now() < deadline, "HTTP body deadline");
        let read = socket.read(&mut chunk).expect("request body");
        assert_ne!(read, 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = serde_json::from_slice(&bytes[header_end..header_end + length])
        .expect("typed Responses request JSON");
    (socket, body)
}

fn respond_tool(socket: &mut TcpStream, name: &str, arguments: Value, call_id: &str) {
    let item_id = format!("item-{call_id}");
    let arguments = arguments.to_string();
    let output = json!({
        "type": "function_call",
        "id": item_id,
        "call_id": call_id,
        "name": name,
        "arguments": arguments,
        "status": "completed"
    });
    respond_events(
        socket,
        &[
            json!({"type": "response.output_item.added", "item": {
                "type": "function_call", "id": item_id, "call_id": call_id,
                "name": name, "arguments": "", "status": "in_progress"
            }}),
            json!({"type": "response.function_call_arguments.delta",
                "item_id": item_id, "delta": arguments}),
            json!({"type": "response.completed", "response": {
                "status": "completed", "output": [output]
            }}),
        ],
    );
}

fn respond_text(socket: &mut TcpStream, text: &str) {
    respond_events(
        socket,
        &[
            json!({"type": "response.output_text.delta", "delta": text}),
            json!({"type": "response.completed", "response": {
                "status": "completed", "output": [{
                    "type": "message", "role": "assistant", "content": [{
                        "type": "output_text", "text": text
                    }]
                }]
            }}),
        ],
    );
}

fn respond_events(socket: &mut TcpStream, events: &[Value]) {
    let sse: String = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect();
    write!(
        socket,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sse}",
        sse.len()
    )
    .expect("fake Responses reply");
}

fn count_tool(request: &Value, name: &str) -> usize {
    request["tools"]
        .as_array()
        .expect("typed tools")
        .iter()
        .filter(|tool| tool["type"] == "function" && tool["name"] == name)
        .count()
}

fn fixed_text(request: &Value) -> String {
    message_texts(request, &["system", "developer"]).join("\n")
}

fn assert_fixed_marker(request: &Value, marker: &str) {
    let text = fixed_text(request);
    assert_eq!(
        text.match_indices(marker).count(),
        1,
        "fixed marker must occur exactly once in typed system/developer input: {marker}; request={request}"
    );
}

fn assert_fixed_order(request: &Value, markers: &[&str]) {
    let text = fixed_text(request);
    let positions: Vec<_> = markers
        .iter()
        .map(|marker| {
            text.find(marker)
                .unwrap_or_else(|| panic!("missing {marker}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "wrong fixed-lane order for {markers:?}: {text}"
    );
}

fn assert_absent(request: &Value, marker: &str) {
    assert!(
        !request.to_string().contains(marker),
        "unexpected workspace leak {marker}: {request}"
    );
}

fn last_message_text(request: &Value, role: &str) -> Option<String> {
    message_texts(request, &[role]).pop()
}

fn message_texts(request: &Value, roles: &[&str]) -> Vec<String> {
    request["input"]
        .as_array()
        .expect("typed Responses input")
        .iter()
        .filter(|item| {
            item["type"] == "message"
                && item["role"]
                    .as_str()
                    .is_some_and(|role| roles.contains(&role))
        })
        .filter_map(|item| item["content"].as_array())
        .flat_map(|parts| parts.iter())
        .filter_map(|part| part["text"].as_str().map(str::to_string))
        .collect()
}

fn function_output(request: &Value, call_id: &str) -> String {
    request["input"]
        .as_array()
        .expect("typed Responses input")
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .and_then(|item| item["output"].as_str())
        .unwrap_or_else(|| panic!("missing function_call_output for {call_id}: {request}"))
        .to_string()
}
