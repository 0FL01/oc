//! Actual-binary Codex image saving uses the same direct/Code Mode file workflow.
use super::*;

fn image_bytes(edited: bool) -> Vec<u8> {
    // Valid 1x1 RGB PNG, encoded with stdlib zlib and PNG CRCs.
    let mut bytes = vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2,
        0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 120, 156, 99, 248, 223, 193, 0, 0,
        4, 17, 1, 136, 113, 160, 56, 171, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let end = bytes.split_off(bytes.len() - 12);
    // A valid ancillary chunk crosses the generic 1MiB inline-media cap without huge pixels.
    let mut chunk = b"tEXtnative-fixture\0".to_vec();
    chunk.extend(std::iter::repeat_n(
        if edited { b'E' } else { b'G' },
        1024 * 1024,
    ));
    let crc = !chunk.iter().fold(!0u32, |crc, byte| {
        (0..8).fold(crc ^ u32::from(*byte), |crc, _| {
            (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1))
        })
    });
    bytes.extend_from_slice(&((chunk.len() - 4) as u32).to_be_bytes());
    bytes.extend(chunk);
    bytes.extend_from_slice(&crc.to_be_bytes());
    bytes.extend(end);
    bytes
}

fn data_url(edited: bool) -> String {
    oc_adapters::attachments::encode_segments(&[oc_adapters::attachments::verify_attachment(
        "image/png",
        &image_bytes(edited),
    )
    .unwrap()])
    .unwrap()[0]["image_url"]
        .as_str()
        .unwrap()
        .to_owned()
}

pub(super) fn result(tool: &str) -> Value {
    json!({"content":[
        {"type":"text","text":"Codex image fixture metadata"},
        {"type":"image","mimeType":"image/png", "data":data_url(tool == "edit_image").strip_prefix("data:image/png;base64,").unwrap()}
    ],"isError":false})
}

#[test]
fn native_image_files_direct_generate_then_edit_exact_workspace_bytes() {
    chain(false);
}

#[test]
fn native_image_files_code_mode_generate_then_edit_exact_workspace_bytes() {
    chain(true);
}

fn chain(code_mode: bool) {
    let fixture = Fixture::new();
    let original = fixture.project.join("pictures/generated.png");
    let edited = fixture.project.join("pictures/edited.png");
    let calls = if code_mode {
        vec![(
            "image-files".into(),
            "image-files".into(),
            json!({
                "__wireName":"execute","arguments":{"code":format!(
                    "const original = await tools.codex_web.generate_image({{prompt:'A fox',output_path:'pictures/generated.png'}}); const edited = await tools.codex_web.edit_image({{prompt:'Make it blue',images:[original.path],output_path:{}}}); return {{original,edited}};",
                    serde_json::to_string(&edited).unwrap()
                )}
            }),
        )]
    } else {
        vec![
            (
                "generate".into(),
                "generate".into(),
                json!({"__wireName":"codex_web__generate_image","arguments":{
                    "prompt":"A fox","output_path":"pictures/generated.png"
                }}),
            ),
            (
                "edit".into(),
                "edit".into(),
                json!({"__wireName":"codex_web__edit_image","arguments":{
                    "prompt":"Make it blue","images":[original],"output_path":edited
                }}),
            ),
        ]
    };
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls,
        final_text: "image-file-chain-complete".into(),
    });
    let mcp = FakeMcp::start_with_protocol(
        "image-files",
        "Bearer image-file-fixture",
        &["generate_image", "edit_image"],
        true,
    );
    let mut entry = remote_entry(&mcp, "Authorization", "Bearer image-file-fixture");
    entry["protocol"] = "2026-07-28".into();
    entry["codemode"] = code_mode.into();
    fixture.write_config(
        &responses,
        json!({"servers":{"codex_web":entry}}),
        json!([
            {"action":"*","resource":"*","effect":"allow"}
        ]),
    );
    let mut tui = PtyProcess::spawn(&fixture, "image-file-chain");
    wait_initial_mcp(&mut tui, "codex_web");
    tui.send_line("generate and edit the workspace image");
    tui.wait_screen("image-file-chain-complete", TIMEOUT);
    assert!(
        tui.clear_draft_then_exit(Instant::now() + TIMEOUT)
            .success()
    );
    assert_eq!(fs::read(&original).unwrap(), image_bytes(false));
    assert_eq!(fs::read(&edited).unwrap(), image_bytes(true));
    let calls = mcp
        .records()
        .into_iter()
        .filter(|record| record.rpc_method == "tools/call")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2, "never regenerate to save an image");
    for call in &calls {
        assert!(
            call.arguments
                .as_ref()
                .unwrap()
                .get("output_path")
                .is_none()
        );
    }
    assert_eq!(calls[0].arguments.as_ref().unwrap()["prompt"], "A fox");
    let expected_input = data_url(false);
    assert!(
        calls[1].arguments.as_ref().unwrap()["images"][0] == expected_input,
        "edit must receive the exact saved original bytes"
    );
    let wire = serde_json::to_string(&responses.requests()).unwrap();
    assert!(
        wire.contains("mime_type") && wire.contains("generated.png") && wire.contains("edited.png")
    );
    assert!(
        !wire.contains("data:image/png;base64,") && !wire.contains("Codex image fixture metadata"),
        "provider must receive compact saved-file facts, not inline image data"
    );
    if !code_mode {
        let requests = responses.requests();
        let schema = requests
            .iter()
            .flat_map(|request| request["tools"].as_array().unwrap())
            .find(|tool| tool["name"] == "codex_web__generate_image")
            .unwrap();
        assert!(
            schema["parameters"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == "output_path")
        );
    }
}

#[test]
fn native_image_files_refuse_denied_and_occupied_paths_before_remote_effects() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.project.join("pictures")).unwrap();
    fs::write(
        fixture.project.join("pictures/occupied.png"),
        b"keep-original",
    )
    .unwrap();
    fs::write(fixture.project.join("private.png"), image_bytes(false)).unwrap();
    std::os::unix::fs::symlink("missing.png", fixture.project.join("pictures/dangling.png"))
        .unwrap();
    let calls = vec![
        (
            "denied-output".into(),
            "denied-output".into(),
            json!({"__wireName":"codex_web__generate_image","arguments":{"prompt":"A fox","output_path":"pictures/denied.png"}}),
        ),
        (
            "denied-input".into(),
            "denied-input".into(),
            json!({"__wireName":"codex_web__edit_image","arguments":{"prompt":"Edit","images":["private.png"],"output_path":"pictures/edited.png"}}),
        ),
        (
            "occupied".into(),
            "occupied".into(),
            json!({"__wireName":"codex_web__generate_image","arguments":{"prompt":"A fox","output_path":"pictures/occupied.png"}}),
        ),
        (
            "dangling".into(),
            "dangling".into(),
            json!({"__wireName":"codex_web__generate_image","arguments":{"prompt":"A fox","output_path":"pictures/dangling.png"}}),
        ),
        (
            "boundary".into(),
            "boundary".into(),
            json!({"__wireName":"codex_web__generate_image","arguments":{"prompt":"A fox","output_path":fixture._root.path().join("external.png")}}),
        ),
    ];
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls,
        final_text: "image-refusals-complete".into(),
    });
    let mcp = FakeMcp::start_with_protocol(
        "image-files",
        "Bearer image-file-fixture",
        &["generate_image", "edit_image"],
        true,
    );
    let mut entry = remote_entry(&mcp, "Authorization", "Bearer image-file-fixture");
    entry["protocol"] = "2026-07-28".into();
    fixture.write_config(
        &responses,
        json!({"servers":{"codex_web":entry}}),
        json!([
            {"action":"*","resource":"*","effect":"allow"},
            {"action":"edit","resource":"pictures/denied.png","effect":"deny"},
            {"action":"read","resource":"private.png","effect":"deny"},
            {"action":"external_directory","resource":"*","effect":"deny"}
        ]),
    );
    let mut tui = PtyProcess::spawn(&fixture, "image-refusals");
    wait_initial_mcp(&mut tui, "codex_web");
    tui.send_line("refuse unsafe image paths");
    tui.wait_screen("image-refusals-complete", TIMEOUT);
    assert!(
        tui.clear_draft_then_exit(Instant::now() + TIMEOUT)
            .success()
    );
    assert!(
        !mcp.records()
            .iter()
            .any(|record| record.rpc_method == "tools/call")
    );
    assert_eq!(
        fs::read(fixture.project.join("pictures/occupied.png")).unwrap(),
        b"keep-original"
    );
    assert!(
        fs::symlink_metadata(fixture.project.join("pictures/dangling.png"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!fixture.project.join("pictures/denied.png").exists());
    assert!(!fixture.project.join("pictures/edited.png").exists());
    assert!(!fixture._root.path().join("external.png").exists());
    let requests = responses.requests();
    let errors = requests
        .iter()
        .flat_map(|request| request["input"].as_array().unwrap())
        .filter(|item| item["type"] == "function_call_output")
        .map(|item| item["output"].to_string())
        .collect::<Vec<_>>()
        .join("\n");
    for error in [
        "denied edit",
        "denied read",
        "denied external_directory",
        "output_path already exists",
    ] {
        assert!(errors.contains(error), "missing refusal: {error}");
    }
}
