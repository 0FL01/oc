//! Local-source graph durability through the shared DCP/fork/compaction owners.
use super::*;

#[test]
fn tool16_stored_read_facts_require_bounded_complete_pixel_validation() {
    use base64::Engine as _;
    let png = |size| {
        let pixels = image::RgbaImage::from_pixel(size, size, image::Rgba([1, 2, 3, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        pixels
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    };
    let paired = |bytes: &[u8], mime: &str| {
        let mut stored = TurnLog::new("stored-turn", "fixture-model", "fixture-provider").to_json();
        let attachment = json!({
        "input": [
            {"type":"function_call","name":"read","call_id":"stored-call","arguments":"{\"path\":\"image.png\"}"},
            {"type":"function_call_output","call_id":"stored-call","output":format!("Read image image.png, {mime}, {} bytes", bytes.len())}
        ],
        "native_read_results":[{"input_index":1,"call_id":"stored-call","result":{
            "source":"local_file","path":"image.png","source_path":"/nonexistent-tool16-source/image.png",
            "mime":mime,"bytes":bytes.len(),"data":base64::engine::general_purpose::STANDARD.encode(bytes)
        }}]
        });
        stored["input"] = attachment["input"].clone();
        stored["native_read_results"] = attachment["native_read_results"].clone();
        stored
    };
    let valid = png(1);
    let original = paired(&valid, "image/png");
    let restored = TurnLog::from_json(&original).unwrap();
    assert_eq!(
        restored.to_json()["native_read_results"],
        original["native_read_results"]
    );
    let wire = serde_json::to_value(&restored.input).unwrap();
    assert_eq!(
        wire[1]["output"][1]["image_url"],
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&valid)
        )
    );
    let mut bad_pixels = b"\x89PNG\r\n\x1a\n".to_vec();
    bad_pixels.extend_from_slice(b"\0\0\0\0IEND\xaeB`\x82");
    let mut animation = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut animation);
        for _ in 0..2 {
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    8,
                    8,
                    image::Rgba([1, 2, 3, 255]),
                )))
                .unwrap();
        }
    }
    animation.truncate(animation.len() - 8);
    animation.push(b';');
    let cases = [
        ("signature only", b"\x89PNG\r\n\x1a\n".to_vec(), "image/png"),
        (
            "missing terminal record",
            valid[..valid.len() - 12].to_vec(),
            "image/png",
        ),
        ("complete envelope, invalid pixels", bad_pixels, "image/png"),
        (
            "decoded allocation over existing cap",
            png(2048),
            "image/png",
        ),
        ("damaged later animation frame", animation, "image/gif"),
    ];
    let accepted: Vec<_> = cases
        .into_iter()
        .filter_map(|(label, bytes, mime)| {
            assert!(bytes.len() <= crate::files::GREP_FILE_BYTES_CAP as usize);
            TurnLog::from_json(&paired(&bytes, mime))
                .is_ok()
                .then_some(label)
        })
        .collect();
    assert!(
        accepted.is_empty(),
        "invalid paired stored images reconstructed: {accepted:?}"
    );
}

fn seed_read(
    rt: &Runtime<'_>,
    db: &Db,
    project: &std::path::Path,
    turn: &str,
    prompt: &str,
) -> (String, String) {
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
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    std::fs::write(project.join("image.png"), bytes.into_inner()).unwrap();
    let call = crate::tools::ToolCall {
        id: turn.into(),
        name: "read".into(),
        arguments: json!({"path":"image.png"}),
    };
    let result = crate::tools::read::execute(
        &rt.files,
        &crate::tools::AllowAllPolicy,
        &AtomicBool::new(false),
        &call,
        true,
    );
    assert_eq!(result.state, "completed");
    let image = result.image.unwrap();
    let mut log = TurnLog::new(turn, "m", "fixture");
    log.user_message = Some(user.clone());
    log.agent_digest = rt.primary_lane(&rt.current.read().unwrap()).agent_digest;
    log.input = vec![
        InputItem::message(InputRole::User, prompt),
        InputItem::ProviderOutput(
            json!({"type":"function_call","call_id":turn,"name":"read","arguments":call.arguments.to_string()}),
        ),
        InputItem::ReadFunctionCallOutput {
            call_id: turn.into(),
            output: image,
        },
        InputItem::ProviderOutput(
            json!({"type":"message","id":format!("answer-{turn}"),"role":"assistant","content":[{"type":"output_text","text":"retained local image"}]}),
        ),
    ];
    db.commit_turn(
        turn,
        "completed",
        Some(&log.to_json().to_string()),
        Some("retained local image"),
    )
    .unwrap();
    let value: Value = serde_json::from_str(&db.turn_result(turn).unwrap().1.unwrap()).unwrap();
    assert!(value.get("native_mcp_results").is_none());
    assert_eq!(
        value["native_read_results"][0]["result"]["source"],
        "local_file"
    );
    assert_eq!(
        value["native_read_results"][0]["result"]["source_path"],
        project.join("image.png").to_str().unwrap()
    );
    let mut bad = value.clone();
    bad["native_read_results"][0]["call_id"] = "wrong-call".into();
    assert!(TurnLog::from_json(&bad).is_err());
    bad = value.clone();
    bad["input"][1]["name"] = "other-tool".into();
    assert!(TurnLog::from_json(&bad).is_err());
    bad = value.clone();
    bad["input"][2]["output"] = "modified presentation".into();
    assert!(TurnLog::from_json(&bad).is_err());
    (user, value["assistant_message"].as_str().unwrap().into())
}

#[test]
fn tool16_local_graph_dcp_fork_restart_preserves_bytes_after_source_change() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.create_session("s").unwrap();
    let (first, last) = seed_read(
        &rt,
        &db,
        project.path(),
        "read-media",
        &"removable local image details ".repeat(1024),
    );
    let (tail_user, _) = seed_read(
        &rt,
        &db,
        project.path(),
        "read-tail",
        "recent image context",
    );
    let original = db.turn_result("read-media").unwrap().1.unwrap();
    let raw = db.read_history_full("s").unwrap();
    let (_, old_bytes) = wire(&rt)
        .iter()
        .find_map(|i| match i {
            InputItem::ReadFunctionCallOutput { call_id, output } if call_id == "read-media" => {
                Some((call_id.clone(), serde_json::to_value(output).unwrap()))
            }
            _ => None,
        })
        .unwrap();
    std::fs::write(project.path().join("image.png"), "changed source").unwrap();
    let fork = db
        .fork_session("s", &tail_user, "work", "fixture", "{}")
        .unwrap()
        .session
        .0;
    let report = rt.run_compress("s", &json!({"topic":"closed local image","content":[{"startId":first,"endId":last,"summary":"Keep the admitted image result; continue."}]}), &ProtectedSpec::default()).unwrap();
    assert_eq!(report.blocks.len(), 1);
    let projected = wire(&rt);
    let actual = projected
        .iter()
        .find_map(|i| match i {
            InputItem::ReadFunctionCallOutput { call_id, output } if call_id == "read-media" => {
                Some(serde_json::to_value(output).unwrap())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(actual, old_bytes);
    let mut hidden = crate::storage::DcpToolProjection::default();
    hidden.hidden.insert(("read-media".into(), 0));
    assert!(!has_media(
        &super::super::context::dcp_continuation(&projected, &[], &hidden),
        "read-media"
    ));
    let fork_rows = db.read_history_full(&fork).unwrap();
    assert!(!fork_rows.is_empty());
    assert_eq!(db.turn_result("read-media").unwrap().1.unwrap(), original);
    assert_eq!(db.read_history_full("s").unwrap(), raw);
    drop(rt);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.open_session("s").unwrap();
    assert!(has_media(&wire(&rt), "read-media"));
    assert_eq!(db.turn_result("read-media").unwrap().1.unwrap(), original);
    let fork_context = rt.active_projection(&fork).unwrap();
    let lane = rt.primary_lane(&rt.current.read().unwrap());
    let fork_wire = rt
        .wire_history(
            &fork,
            &fork_context.projected,
            &fork_context.blocks,
            "m",
            "fixture",
            lane.agent_digest.as_deref(),
            fork_context.after_seq,
        )
        .unwrap();
    assert!(
        fork_wire
            .iter()
            .any(|i| matches!(i, InputItem::ReadFunctionCallOutput { .. }))
    );
}

#[tokio::test]
async fn tool16_compaction_carries_original_local_image_array_and_recent_tail() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.create_session("s").unwrap();
    seed_read(
        &rt,
        &db,
        project.path(),
        "read-older",
        "older image context",
    );
    seed_read(&rt, &db, project.path(), "read-recent", "recent image tail");
    let original = db.turn_result("read-older").unwrap().1.unwrap();
    std::fs::remove_file(project.path().join("image.png")).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        headers: BTreeMap::new(),
        set_cache_key: true,
        wire: Default::default(),
        timeout: Some(false),
        chunk_timeout_ms: 5000,
        connect_timeout: Duration::from_secs(5),
        allow_private: true,
    };
    let peer = tokio::spawn(async move {
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
            assert!(bytes.len() < 1048576);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
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
            assert!(bytes.len() < 1048576);
        }
        let request: Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        assert_eq!(request["tools"], json!([]));
        let input = request["input"].as_array().unwrap();
        let output = input
            .iter()
            .find(|i| i["type"] == "function_call_output" && i["call_id"] == "read-older")
            .unwrap();
        assert_eq!(output["output"][0]["type"], "input_text");
        assert_eq!(output["output"][1]["type"], "input_image");
        assert!(
            input
                .iter()
                .any(|i| i["type"] == "function_call" && i["call_id"] == "read-older")
        );
        assert!(!input.iter().any(|i| i["call_id"] == "read-recent"));
        let body = format!(
            "data: {}\n\n",
            json!({"type":"response.completed","response":{"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"## Work State\nAdmitted image retained."}]}]}})
        );
        socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let catalog = ModelCatalog { provider: "fixture".into(), models: [("m".into(), json!({"limit":{"context":65536,"output":4096},"modalities":{"input":["text","image"]}}))].into() };
    rt.queue_compaction("s", oc_core::compaction::CompactionReason::Manual)
        .unwrap();
    let result = rt
        .deliver_compaction("s", &catalog, "m", None, &provider)
        .await;
    peer.await.unwrap();
    assert!(result.unwrap());
    assert!(!has_media(&wire(&rt), "read-older"));
    assert!(has_media(&wire(&rt), "read-recent"));
    assert_eq!(db.turn_result("read-older").unwrap().1.unwrap(), original);
    drop(rt);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    let rt = runtime(&db, project.path());
    rt.open_session("s").unwrap();
    assert!(has_media(&wire(&rt), "read-recent"));
    assert_eq!(db.turn_result("read-older").unwrap().1.unwrap(), original);
}
