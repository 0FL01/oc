use super::*;

fn fixture() -> Value {
    serde_json::from_str::<Value>(include_str!("../../../../../fixtures/mcp12-media.json")).unwrap()
        ["result"]
        .clone()
}

#[test]
fn mcp12_native_media_lowering_is_ordered_and_legacy_text_api_is_compatible() {
    let original = fixture();
    let secrets = ["mcp-media-private-canary".into()];
    let output = project_rich(
        serde_json::from_value(original.clone()).unwrap(),
        &secrets,
        &secrets,
    )
    .unwrap();
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
    assert_eq!(
        output.facts()["content"][3]["resource"]["uri"],
        "fixture://binary"
    );
    assert_eq!(
        output.facts()["content"][0]["annotations"],
        original["content"][0]["annotations"]
    );
    assert_eq!(output.facts()["_meta"]["provenance"], "fixture-result");
    assert!(
        !output
            .facts()
            .to_string()
            .contains("mcp-media-private-canary")
    );
    assert!(!format!("{output:?}").contains("iVBOR"));
    assert!(!output.display().contains("iVBOR"));
    assert!(matches!(
        super::super::project(serde_json::from_value(original.clone()).unwrap(), &secrets),
        Err(ResultError::Unsupported)
    ));
    for (index, kind) in [(1, "input_image"), (2, "input_file"), (3, "input_file")] {
        let value = json!({"content":[original["content"][index].clone()]});
        let output = project_rich(serde_json::from_value(value).unwrap(), &[], &[]).unwrap();
        let wire = serde_json::to_value(output).unwrap();
        assert_eq!(wire.as_array().unwrap().len(), 1);
        assert_eq!(wire[0]["type"], kind);
    }
    let legacy =
        json!({"content":[{"type":"text","text":"answer text"}],"structuredContent":{"answer":42}});
    let secrets = ["text".into()];
    let plain =
        super::super::project(serde_json::from_value(legacy.clone()).unwrap(), &secrets).unwrap();
    let rich = project_rich(serde_json::from_value(legacy).unwrap(), &secrets, &[]).unwrap();
    assert_eq!(serde_json::to_value(rich).unwrap(), plain);
}

#[test]
fn mcp12_malformed_sensitive_binary_identity_and_caps_reject_the_whole_result() {
    let secret = "mcp-media-private-canary".to_string();
    let encoded = base64::engine::general_purpose::STANDARD.encode(secret.as_bytes());
    let timestamp = json!({"content":[{"type":"image","mimeType":"image/png","data":"AA==","annotations":{"lastModified":"2026-09-29T12:34:56Z"}}]});
    assert!(matches!(
        project_rich(
            serde_json::from_value(timestamp).unwrap(),
            &["12:34".into()],
            &["12:34".into()]
        ),
        Err(ResultError::BadResult)
    ));
    let cases = [
        json!({"type":"image","mimeType":"image/png","data":"not-base64!"}),
        json!({"type":"audio","mimeType":"audio/wav","data":encoded}),
        json!({"type":"resource","resource":{"uri":format!("fixture://{secret}"),"blob":"AA=="}}),
        json!({"type":"image","mimeType":format!("image/{secret}"),"data":"AA=="}),
        json!({"type":"image","mimeType":"audio/wav","data":"AA=="}),
    ];
    for block in cases {
        let result =
            json!({"content":[{"type":"text","text":"must not publish partial text"},block]});
        assert!(matches!(
            project_rich(
                serde_json::from_value(result).unwrap(),
                std::slice::from_ref(&secret),
                std::slice::from_ref(&secret)
            ),
            Err(ResultError::BadResult)
        ));
    }
    // The SDK's typed ResourceContents chooses a branch at decode. Durable
    // native facts are still validated directly, so corrupt attachments cannot
    // introduce a second, conflicting body after restart.
    assert!(matches!(
        McpToolOutput::from_stored(
            json!({"content":[{"type":"resource","resource":{"uri":"fixture://binary","mimeType":"application/octet-stream","blob":"AA==","text":"conflicting body"}}]})
        ),
        Err(ResultError::BadResult)
    ));
    for result in [
        json!({"content":vec![json!({"type":"image","mimeType":"image/png","data":"AA=="});65]}),
        json!({"content":[{"type":"text","text":"x".repeat(crate::mcp_remote::RESULT_TEXT_BYTES_CAP)},{"type":"image","mimeType":"image/png","data":"AA=="}]}),
    ] {
        assert!(matches!(
            project_rich(serde_json::from_value(result).unwrap(), &[], &[]),
            Err(ResultError::BadResult)
        ));
    }
    // Product test flag "1" can appear in the base64 alphabet without existing
    // in decoded image bytes; preserve the actual data rather than mutate it.
    let mut image = fixture()["content"][1].clone();
    image.as_object_mut().unwrap().remove("_meta");
    assert!(
        project_rich(
            serde_json::from_value(json!({"content":[image]})).unwrap(),
            &["1".into()],
            &["1".into()]
        )
        .is_ok()
    );
}
