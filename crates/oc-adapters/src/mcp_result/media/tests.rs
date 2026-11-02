use super::*;

fn fixture() -> Value {
    serde_json::from_str::<Value>(include_str!("../../../../../fixtures/mcp12-media.json")).unwrap()
        ["result"]
        .clone()
}

#[test]
fn mcp12_native_media_lowering_is_ordered_and_legacy_text_api_is_compatible() {
    let original = fixture();
    let output = project_rich(serde_json::from_value(original.clone()).unwrap()).unwrap();
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire.as_array().unwrap().len(), 7);
    assert_eq!(
        wire[1]["image_url"],
        format!(
            "data:image/png;base64,{}",
            original["content"][1]["data"].as_str().unwrap()
        )
    );
    assert_eq!(
        wire[2]["file_data"],
        format!(
            "data:audio/wav;base64,{}",
            original["content"][2]["data"].as_str().unwrap()
        )
    );
    assert_eq!(wire[3]["filename"], "file");
    assert_eq!(output.facts(), &original);
    assert!(
        output
            .facts()
            .to_string()
            .contains("mcp-media-private-canary")
    );
    assert!(!format!("{output:?}").contains("iVBOR"));
    assert!(!output.display().contains("iVBOR"));
    assert!(matches!(
        super::super::project(serde_json::from_value(original.clone()).unwrap()),
        Err(ResultError::Unsupported)
    ));
    for (index, kind) in [(1, "input_image"), (2, "input_file"), (3, "input_file")] {
        let value = json!({"content":[original["content"][index].clone()]});
        let output = project_rich(serde_json::from_value(value).unwrap()).unwrap();
        let wire = serde_json::to_value(output).unwrap();
        assert_eq!(wire.as_array().unwrap().len(), 1);
        assert_eq!(wire[0]["type"], kind);
    }
    let legacy = json!({"content":[{"type":"text","text":"1 true test-key answer text"}],"structuredContent":{"token":"test-key"}});
    let plain = super::super::project(serde_json::from_value(legacy.clone()).unwrap()).unwrap();
    let rich = project_rich(serde_json::from_value(legacy).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(rich).unwrap(), plain);
}

#[test]
fn mcp12_malformed_content_and_parts_cap_reject_the_whole_result() {
    for block in [
        json!({"type":"image","mimeType":"image/png","data":"not-base64!"}),
        json!({"type":"image","mimeType":"image/invalid mime","data":"AA=="}),
        json!({"type":"image","mimeType":"audio/wav","data":"AA=="}),
    ] {
        let result =
            json!({"content":[{"type":"text","text":"must not publish partial text"},block]});
        assert!(matches!(
            project_rich(serde_json::from_value(result).unwrap()),
            Err(ResultError::BadResult)
        ));
    }
    assert!(matches!(
        McpToolOutput::from_stored(
            json!({"content":[{"type":"resource","resource":{"uri":"fixture://binary","mimeType":"application/octet-stream","blob":"AA==","text":"conflicting body"}}]})
        ),
        Err(ResultError::BadResult)
    ));
    assert!(matches!(project_rich(serde_json::from_value(json!({"content":vec![json!({"type":"image","mimeType":"image/png","data":"AA=="});65]})).unwrap()), Err(ResultError::BadResult)));
    assert!(matches!(project_rich(serde_json::from_value(json!({"content":[{"type":"text","text":"x".repeat(MEDIA_BYTES_CAP)},{"type":"image","mimeType":"image/png","data":"AA=="}]})).unwrap()), Err(ResultError::TooLarge { limit: MEDIA_BYTES_CAP, .. })));
}

#[test]
fn mcp_png_containing_ordinary_environment_byte_is_accepted() {
    let mut png = base64::engine::general_purpose::STANDARD
        .decode(fixture()["content"][1]["data"].as_str().unwrap())
        .unwrap();
    // PNG decoders permit trailing bytes; MCP validates the base64 and MIME.
    png.extend_from_slice(b"1 true test-key");
    let data = base64::engine::general_purpose::STANDARD.encode(&png);
    let output = project_rich(
        serde_json::from_value(
            json!({"content":[{"type":"image","mimeType":"image/png","data":data}]}),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(output.facts()["content"][0]["data"], data);
}
