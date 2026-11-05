use super::*;

/// Run each mode in a fresh test process to keep VmHWM independent of compiler
/// and earlier cases. The peer streams its result instead of allocating a list.
#[tokio::test]
#[ignore = "opt-in isolated MCP RSS measurement; W11_MCP_PROBE=baseline|large|many"]
async fn w11_mcp_expansion_rss_probe() {
    use crate::mcp_stdio::{StdioClient, StdioConfig};
    use std::{sync::atomic::AtomicBool, time::Duration};
    let mode = std::env::var("W11_MCP_PROBE").unwrap_or_else(|_| "many".into());
    assert!(["baseline", "large", "many"].contains(&mode.as_str()));
    let script = r#"
import json, sys
mode = sys.argv[1]
block = '{"type":"image","mimeType":"image/png","data":"AA=="}'
for line in sys.stdin:
    m = json.loads(line)
    if 'id' not in m: continue
    if m['method'] == 'initialize':
        result = {'protocolVersion':'2024-11-05','capabilities':{'tools':{}},'serverInfo':{'name':'rss-fixture','version':'1'}}
    elif m['method'] == 'tools/list':
        result = {'tools':[{'name':'probe','inputSchema':{'type':'object'}}]}
    elif m['method'] == 'tools/call':
        sys.stdout.write('{"jsonrpc":"2.0","id":'+str(m['id'])+',"result":{"content":[')
        if mode == 'many':
            for i in range(300000): sys.stdout.write((',' if i else '')+block)
        elif mode == 'large':
            sys.stdout.write('{"type":"image","mimeType":"image/png","data":"')
            remaining = ((len(block)+1)*300000-64)//4*4
            while remaining:
                n = min(remaining, 65536)
                sys.stdout.write('A'*n)
                remaining -= n
            sys.stdout.write('"}')
        else: sys.stdout.write(block)
        sys.stdout.write(']}}\n')
        sys.stdout.flush()
        continue
    else: continue
    print(json.dumps({'jsonrpc':'2.0','id':m['id'],'result':result}), flush=True)
"#;
    let client = StdioClient::launch(&StdioConfig {
        server_id: "rss".into(),
        argv: vec![
            "/usr/bin/python3".into(),
            "-I".into(),
            "-c".into(),
            script.into(),
            mode.clone(),
        ],
        cwd: None,
        extra_env: vec![],
        secrets: vec![],
        enabled: true,
        timeout: Duration::from_secs(20),
        startup_timeout: None,
        catalog_timeout: None,
    })
    .await
    .unwrap();
    let result = client
        .call_tool_rich("probe", json!({}), &AtomicBool::new(false))
        .await;
    let hwm = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("VmHWM:"))
            .unwrap()
            .to_owned()
    };
    eprintln!("W11 mode={mode} post_call {}", hwm());
    match result {
        Ok(output) => {
            let raw_bytes = serde_json::to_vec(output.facts()).unwrap().len();
            let wire_bytes = serde_json::to_vec(&output).unwrap().len();
            eprintln!(
                "W11 mode={mode} accepted raw_bytes={raw_bytes} parts={} wire_bytes={wire_bytes} post_serialize {}",
                output.facts()["content"].as_array().unwrap().len(),
                hwm()
            );
        }
        Err(error) => eprintln!("W11 mode={mode} rejected={error} {}", hwm()),
    }
    client.shutdown().await.unwrap();
}

fn fixture() -> Value {
    serde_json::from_str::<Value>(include_str!("../../../../../fixtures/mcp12-media.json")).unwrap()
        ["result"]
        .clone()
}

#[test]
fn w11_mcp_expanded_content_budget_applies_before_projection_and_on_restore() {
    let count = MEDIA_BYTES_CAP / std::mem::size_of::<rmcp::model::ContentBlock>() + 1;
    let original =
        json!({"content":vec![json!({"type":"image","mimeType":"image/png","data":"AA=="});count]});
    assert!(original.to_string().len() < MEDIA_BYTES_CAP);
    let result = project_rich(serde_json::from_value(original.clone()).unwrap());
    assert!(matches!(
        result,
        Err(ResultError::TooLarge {
            limit: MEDIA_BYTES_CAP,
            ..
        })
    ));
    assert!(matches!(
        McpToolOutput::from_stored(original),
        Err(ResultError::TooLarge {
            limit: MEDIA_BYTES_CAP,
            ..
        })
    ));
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
fn mcp12_malformed_content_and_aggregate_cap_reject_the_whole_result() {
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
    assert!(matches!(project_rich(serde_json::from_value(json!({"content":[{"type":"text","text":"x".repeat(MEDIA_BYTES_CAP)},{"type":"image","mimeType":"image/png","data":"AA=="}]})).unwrap()), Err(ResultError::TooLarge { limit: MEDIA_BYTES_CAP, .. })));
}

#[test]
fn w8_mcp_media_above_one_mib_and_sixty_four_parts_reconstructs_in_order() {
    let original = json!({"content":(0..65).map(|i| json!({"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(vec![i as u8; 16 * 1024])})).collect::<Vec<_>>()});
    assert!(original.to_string().len() > 1024 * 1024);
    let output = project_rich(serde_json::from_value(original.clone()).unwrap()).unwrap();
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire.as_array().unwrap().len(), 65);
    for (index, item) in wire.as_array().unwrap().iter().enumerate() {
        assert_eq!(
            item["image_url"],
            format!(
                "data:image/png;base64,{}",
                original["content"][index]["data"].as_str().unwrap()
            )
        );
    }
    let restored = McpToolOutput::from_stored(output.facts().clone()).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), wire);
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
