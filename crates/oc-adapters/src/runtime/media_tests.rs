//! Directed persisted-media projection/compaction risks; actual RPC → provider
//! bridge is separately exercised by native mcp_application/media.rs.
use super::*;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "read_tests.rs"]
mod read;

fn runtime<'a>(db: &'a Db, project: &std::path::Path) -> Runtime<'a> {
    let mut generation = Generation {
        permissions: [
            ("read".into(), Permission::Allow),
            ("compress".into(), Permission::Allow),
        ]
        .into(),
        ..Default::default()
    };
    generation.compaction.keep_tokens = 0;
    Runtime::new(
        db,
        "work",
        generation,
        ProtectedGlobs { patterns: vec![] },
        crate::files::Files::new(project, db.root()).unwrap(),
        crate::shell::Shell::new(project).unwrap(),
        BTreeMap::new(),
        ToolRoots {
            project: project.into(),
            data: db.root().into(),
        },
        None,
        false,
        DcpConfig {
            turn_protection: false,
            protected_tools: vec![],
            ..Default::default()
        },
    )
    .unwrap()
}

fn seed(runtime: &Runtime<'_>, db: &Db, turn: &str, prompt: &str) -> (String, String) {
    let user = db
        .accept_turn(
            turn,
            "s",
            prompt,
            prompt,
            &oc_core::queries::ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
        )
        .unwrap()
        .user_message;
    let facts =
        serde_json::from_str::<Value>(include_str!("../../../../fixtures/mcp12-media.json"))
            .unwrap()["result"]
            .clone();
    let secrets = ["mcp-media-private-canary".into()];
    let result =
        crate::mcp_result::project_rich(serde_json::from_value(facts).unwrap(), &secrets, &secrets)
            .unwrap();
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(user.clone());
    log.agent_digest = runtime
        .primary_lane(&runtime.current.read().unwrap())
        .agent_digest;
    log.input = vec![
        InputItem::message(InputRole::User, prompt),
        InputItem::ProviderOutput(
            json!({"type":"function_call","call_id":turn,"name":"fixture__media","arguments":"{}"}),
        ),
        InputItem::McpFunctionCallOutput {
            call_id: turn.into(),
            output: result,
        },
        InputItem::ProviderOutput(
            json!({"type":"message","id":format!("answer-{turn}"),"role":"assistant","content":[{"type":"output_text","text":"retained answer"}]}),
        ),
    ];
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some("retained answer"),
    )
    .unwrap();
    let value: Value = serde_json::from_str(&db.turn_result(turn).unwrap().1.unwrap()).unwrap();
    (user, value["assistant_message"].as_str().unwrap().into())
}

fn wire(runtime: &Runtime<'_>) -> Vec<InputItem> {
    let context = runtime.active_projection("s").unwrap();
    let lane = runtime.primary_lane(&runtime.current.read().unwrap());
    runtime
        .wire_history(
            "s",
            &context.projected,
            &context.blocks,
            "m",
            "fixture",
            lane.agent_digest.as_deref(),
            context.after_seq,
        )
        .unwrap()
}

fn has_media(input: &[InputItem], call_id: &str) -> bool {
    serde_json::to_value(input)
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .any(|i| {
            i["type"] == "function_call_output"
                && i["call_id"] == call_id
                && i["output"][1]["image_url"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("data:image/png;base64,iVBOR"))
        })
}

#[test]
fn mcp12_dcp_sql_projection_reindexes_native_facts_and_reopen_keeps_raw_history() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.create_session("s").unwrap();
    let (first, last) = seed(&rt, &db, "media", &"removable user details ".repeat(1024));
    seed(&rt, &db, "tail", "unfinished context tail");
    let original = db.turn_result("media").unwrap().1.unwrap();
    let raw_history = db.read_history_full("s").unwrap();
    assert!(has_media(&wire(&rt), "media"));
    let report = rt.run_compress("s",&json!({"topic":"closed media context","content":[{"startId":first,"endId":last,"summary":"Preserve the completed media operation and continue."}]}),&ProtectedSpec::default()).unwrap();
    assert_eq!(report.blocks.len(), 1);
    let projected = wire(&rt);
    assert!(
        has_media(&projected, "media"),
        "covered ordinary user input must not shift the native result onto a different item"
    );
    assert_eq!(
        projected
            .iter()
            .filter(
                |i| matches!(i,InputItem::McpFunctionCallOutput{call_id,..} if call_id=="media")
            )
            .count(),
        1
    );
    let mut projection = crate::storage::DcpToolProjection::default();
    projection.hidden.insert(("media".into(), 0));
    let hidden = super::context::dcp_continuation(&projected, &[], &projection);
    assert!(!has_media(&hidden, "media"));
    assert!(!hidden.iter().any(|i| matches!(i,InputItem::ProviderOutput(v) if v["type"]=="function_call"&&v["call_id"]=="media")));
    assert_eq!(db.turn_result("media").unwrap().1.unwrap(), original);
    assert_eq!(db.read_history_full("s").unwrap(), raw_history);
    drop(rt);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.open_session("s").unwrap();
    assert!(has_media(&wire(&rt), "media"));
    assert_eq!(db.turn_result("media").unwrap().1.unwrap(), original);
}

#[tokio::test]
async fn mcp12_compaction_receives_native_media_and_recent_checkpoint_tail_reopens_without_replay()
{
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.create_session("s").unwrap();
    seed(&rt, &db, "older-media", "older completed media exchange");
    seed(&rt, &db, "recent-media", "recent completed media exchange");
    let original = db.turn_result("older-media").unwrap().1.unwrap();
    let raw_history = db.read_history_full("s").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "fixture-key".into(),
        headers: BTreeMap::new(),
        set_cache_key: true,
        wire: Default::default(),
        timeout: Some(false),
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(5),
        allow_private: true,
    };
    let task = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = Vec::new();
        let mut buf = [0u8; 4096];
        let end = loop {
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            assert!(bytes.len() < 2 * 1024 * 1024);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let length: usize = std::str::from_utf8(&bytes[..end])
            .unwrap()
            .lines()
            .find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map(|(_, v)| v.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < end + length {
            let n = socket.read(&mut buf).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            assert!(bytes.len() < 2 * 1024 * 1024);
        }
        let request: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        assert_eq!(request["tools"], json!([]));
        let input = request["input"].as_array().unwrap();
        let output = input
            .iter()
            .find(|i| i["type"] == "function_call_output" && i["call_id"] == "older-media")
            .unwrap();
        assert_eq!(output["output"][1]["type"], "input_image");
        assert_eq!(output["output"][2]["type"], "input_file");
        assert!(
            input
                .iter()
                .any(|i| i["type"] == "function_call" && i["call_id"] == output["call_id"])
        );
        assert!(!input.iter().any(|i| i["call_id"] == "recent-media"));
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Work State\nCompleted media operation; continue from recent context."}]}]}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: [("m".into(), json!({"limit":{"context":65536,"output":4096}}))].into(),
    };
    rt.queue_compaction("s", oc_core::compaction::CompactionReason::Manual)
        .unwrap();
    assert!(
        rt.deliver_compaction("s", &catalog, "m", None, &provider)
            .await
            .unwrap()
    );
    task.await.unwrap();
    assert!(has_media(&wire(&rt), "recent-media"));
    assert!(!has_media(&wire(&rt), "older-media"));
    assert_eq!(db.turn_result("older-media").unwrap().1.unwrap(), original);
    assert_eq!(db.read_history_full("s").unwrap(), raw_history);
    drop(rt);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.open_session("s").unwrap();
    assert!(has_media(&wire(&rt), "recent-media"));
    assert!(db.session_checkpoint("s").unwrap().is_some());
    assert_eq!(db.turn_result("older-media").unwrap().1.unwrap(), original);
}
