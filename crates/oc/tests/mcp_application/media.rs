//! T46 media: pinned OpenResponses donor 2670273ff17da96f85c5826ced57aa1b368754fa,
//! open-responses.ts:146–159,203–206,507–537 and provider tests:1618–1705,1768–1816.
use super::*;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=";
const AUDIO: &str = "UklGRiYAAABXQVZFZm10IBAAAAABAAEARKwAAIhYAQACABAAZGF0YQIAAAAAAA==";
const BLOB: &str = "AAEC/w==";

pub(super) fn mixed_result() -> Value {
    serde_json::from_str::<Value>(include_str!("../../../../fixtures/mcp12-media.json")).unwrap()["result"].clone()
}

#[test]
fn mcp12_actual_binary_mixed_media_reaches_next_responses_request_with_exact_call_graph() {
    let responses =
        FakeResponses::start(ResponsesScript::ToolNamed("media__mcp_media_mixed".into()));
    let mcp = FakeMcp::start(
        "media-fixture",
        "Bearer mcp-media-private-canary",
        &["mcp_media_mixed"],
    );
    let fixture = Fixture::new();
    fixture.write_config(
        &responses,
        json!({"media":remote_entry(&mcp,"Authorization","Bearer mcp-media-private-canary")}),
        json!({"media__mcp_media_mixed":"allow"}),
    );
    let bytes = fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap();
    let mut process = fixture.spawn_run("mcp12-mixed");
    assert!(process.wait().success(), "{}", process.diagnostics());
    let requests = responses.requests();
    let next = requests
        .iter()
        .filter(|r| !title::is_title(r))
        .find(|r| {
            r["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["type"] == "function_call_output" && i["output"].is_array())
        })
        .expect("native media result must be an ordered output array in the next request");
    let input = next["input"].as_array().unwrap();
    let output = input
        .iter()
        .find(|i| i["type"] == "function_call_output" && i["output"].is_array())
        .unwrap();
    let call = input
        .iter()
        .find(|i| i["type"] == "function_call" && i["call_id"] == output["call_id"])
        .unwrap();
    assert_eq!(call["name"], "media__mcp_media_mixed");
    let parts = output["output"].as_array().unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|p| p["type"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "input_text",
            "input_image",
            "input_file",
            "input_file",
            "input_text",
            "input_text",
            "input_text"
        ]
    );
    assert_eq!(
        parts[1],
        json!({"type":"input_image","image_url":format!("data:image/png;base64,{PNG}")})
    );
    assert_eq!(
        parts[2],
        json!({"type":"input_file","filename":"file","file_data":format!("data:audio/wav;base64,{AUDIO}")})
    );
    assert_eq!(
        parts[3],
        json!({"type":"input_file","filename":"file","file_data":format!("data:application/octet-stream;base64,{BLOB}")})
    );
    assert!(parts[0]["text"].as_str().unwrap().contains("[redacted]"));
    assert!(
        parts[4]["text"]
            .as_str()
            .unwrap()
            .contains("file:///not-read-by-native")
    );
    assert!(parts[6]["text"].as_str().unwrap().contains("\"answer\":42"));
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|r| r.rpc_method == "initialize")
            .count(),
        1
    );
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|r| r.rpc_method == "tools/call")
            .count(),
        1
    );
    assert_eq!(
        fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap(),
        bytes
    );
    assert!(
        !serde_json::to_string(&requests)
            .unwrap()
            .contains("mcp-media-private-canary")
    );
    assert!(!process.output().contains(PNG));
    assert!(!process.diagnostics().contains(PNG));
}

#[test]
fn mcp12_actual_stdio_restart_replays_durable_media_without_another_tool_call() {
    let responses = FakeResponses::start(ResponsesScript::ToolNamedOnce(
        "media__mcp_media_mixed".into(),
    ));
    let fixture = Fixture::new();
    let report = fixture.project.join("media-counters.jsonl");
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mcp12-media.py")
        .canonicalize()
        .unwrap();
    fixture.write_config(
        &responses,
        json!({"media":{
            "type":"local","command":["/usr/bin/python3",script,report],
            "environment":{"MCP_MEDIA_CANARY":"mcp-media-private-canary"},"timeout":3000
        }}),
        json!({"media__mcp_media_mixed":"allow"}),
    );
    let bytes = fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap();
    let mut first = fixture.spawn_run("mcp12-stdio-restart");
    assert!(first.wait().success(), "{}", first.diagnostics());
    let before = responses.requests().len();
    let mut reopened = fixture.spawn_run("mcp12-stdio-restart");
    assert!(reopened.wait().success(), "{}", reopened.diagnostics());
    let requests = responses.requests();
    assert!(
        requests[before..]
            .iter()
            .filter(|r| !title::is_title(r))
            .any(|r| r["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["type"] == "function_call_output"
                    && i["output"][1]["image_url"] == format!("data:image/png;base64,{PNG}")))
    );
    let counters: Vec<Value> = fs::read_to_string(&report)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        counters
            .iter()
            .filter(|c| c["method"] == "tools/call")
            .count(),
        1,
        "restart must not execute the retained media tool again"
    );
    assert_eq!(
        counters
            .iter()
            .filter(|c| c["method"] == "initialize")
            .count(),
        2
    );
    assert_eq!(
        counters.iter().filter(|c| c["method"] == "closed").count(),
        2
    );
    for event in counters.iter().filter(|c| c["method"] == "initialize") {
        let pid = event["pid"].as_i64().unwrap() as libc::pid_t;
        // SAFETY: probe only the owned fixture PID; signal zero has no effect.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }
    let db = rusqlite::Connection::open(fixture.home.join("data/oc/oc.sqlite")).unwrap();
    let logs = db.prepare("SELECT result FROM conversation_turns WHERE session_id='mcp12-stdio-restart' ORDER BY archive_rowid").unwrap().query_map([], |r| r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    let original: Value = serde_json::from_str(&logs[0]).unwrap();
    assert_eq!(
        original["native_mcp_results"][0]["result"]["content"][1]["data"],
        PNG
    );
    assert_eq!(
        original["native_mcp_results"][0]["result"]["content"][3]["resource"]["uri"],
        "fixture://binary"
    );
    assert_eq!(
        original["native_mcp_results"][0]["result"]["content"][0]["annotations"]["priority"],
        0.5
    );
    assert_eq!(
        original["native_mcp_results"][0]["result"]["_meta"]["provenance"],
        "fixture-result"
    );
    assert!(!logs.join("").contains("mcp-media-private-canary"));
    assert_eq!(
        fs::read(fixture.home.join("config/opencode/opencode.json")).unwrap(),
        bytes
    );
}

#[test]
fn mcp12_media_permissions_and_sensitive_binary_never_publish_partial_success() {
    for (tool, permission, expected_state, expected_calls) in [
        ("mcp_media_mixed", "deny", "denied", 0),
        ("mcp_media_sensitive", "allow", "unknown", 1),
    ] {
        let name = format!("media__{tool}");
        let script = if permission == "deny" {
            // Whole-action Deny now correctly removes its schema. This is an
            // adversarial forbidden call, not a model waiting forever for a
            // catalog entry that policy will never advertise.
            ResponsesScript::ToolNamedForbidden(name.clone())
        } else {
            ResponsesScript::ToolNamed(name.clone())
        };
        let responses = FakeResponses::start(script);
        let mcp = FakeMcp::start(
            "media-sensitive-fixture",
            "Bearer mcp-media-private-canary",
            &[tool],
        );
        let fixture = Fixture::new();
        fixture.write_config(
            &responses,
            json!({"media":remote_entry(&mcp,"Authorization","Bearer mcp-media-private-canary")}),
            json!({name.clone():permission}),
        );
        let mut process = fixture.spawn_run("mcp12-sensitive");
        let success = process.wait().success();
        assert_eq!(success, expected_calls == 0, "{}", process.diagnostics());
        assert_eq!(
            mcp.records()
                .iter()
                .filter(|r| r.rpc_method == "tools/call")
                .count(),
            expected_calls
        );
        assert!(!responses.requests().iter().any(|r| {
            r["input"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["type"] == "function_call_output" && i["output"].is_array())
        }));
        let db = rusqlite::Connection::open(fixture.home.join("data/oc/oc.sqlite")).unwrap();
        let state: String = db
            .query_row(
                "SELECT state FROM tool_operations WHERE name=?1",
                [&name],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(state, expected_state);
        let result:String=db.query_row("SELECT result FROM conversation_turns WHERE session_id='mcp12-sensitive' ORDER BY archive_rowid DESC LIMIT 1",[],|r|r.get(0)).unwrap();
        assert!(!result.contains("mcp-media-private-canary"));
        assert!(!result.contains("bWNwLW1lZGlhLXByaXZhdGUtY2FuYXJ5"));
        assert!(!result.contains("do not publish partial media"));
        assert!(!process.output().contains("mcp-media-private-canary"));
        assert!(!process.diagnostics().contains("mcp-media-private-canary"));
    }
}
