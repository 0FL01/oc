use super::*;
use crate::provider::InputRole;
use serde_json::json;

fn png() -> std::sync::Arc<[u8]> {
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner().into()
}

fn history() -> Vec<InputItem> {
    let question = crate::tools::question::QuestionToolOutput::new(
        "answers".into(),
        &[oc_core::question::QuestionImage {
            question: 0,
            answer: 0,
            mark: oc_core::prompt::ImageMark {
                start: 0,
                end: 9,
                ordinal: 1,
            },
            png: png(),
        }],
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    vec![
        InputItem::Message {
            role: InputRole::User,
            content: vec![
                InputContent::InputText {
                    text: "see [Image 1]".into(),
                },
                InputContent::InputImage {
                    image_url: "data:image/png;base64,AA==".into(),
                    detail: None,
                },
            ],
        },
        InputItem::QuestionFunctionCallOutput {
            call_id: "ask".into(),
            output: question,
        },
        InputItem::ProviderOutput(
            json!({"type":"function_call_output","call_id":"legacy","output":[
                {"type":"input_text","text":"legacy text"},
                {"type":"input_image","image_url":"data:image/png;base64,AA=="}
            ]}),
        ),
        InputItem::FunctionCallOutput {
            call_id: "plain".into(),
            output: "plain".into(),
        },
    ]
}

fn images(input: &[InputItem]) -> usize {
    serde_json::to_string(input)
        .unwrap()
        .matches("input_image")
        .count()
}

#[test]
fn media_projection_follows_model_vision_and_chat_tool_wire() {
    // Vision model on a wire that carries tool images: unchanged.
    let mut input = history();
    assert_eq!(omitted_images(&input, true, false), 0);
    project_media(&mut input, true, false);
    assert_eq!(input, history());

    // Chat tool messages are text-only even for a vision model; the user image stays.
    let mut input = history();
    assert_eq!(omitted_images(&input, true, true), 2);
    project_media(&mut input, true, true);
    assert_eq!(images(&input), 1);
    assert_eq!(input[0], history()[0]);
    assert_eq!(
        input[1],
        InputItem::FunctionCallOutput {
            call_id: "ask".into(),
            output: format!("answers\n{IMAGE_OMITTED}"),
        }
    );
    assert_eq!(
        input[2],
        InputItem::FunctionCallOutput {
            call_id: "legacy".into(),
            output: format!("legacy text\n{IMAGE_OMITTED}"),
        }
    );

    // No vision: every image becomes a note, the surrounding text is kept.
    let mut input = history();
    assert_eq!(omitted_images(&input, false, false), 3);
    project_media(&mut input, false, false);
    assert_eq!(images(&input), 0);
    assert_eq!(
        input[0],
        InputItem::Message {
            role: InputRole::User,
            content: vec![
                InputContent::InputText {
                    text: "see [Image 1]".into()
                },
                InputContent::InputText {
                    text: IMAGE_OMITTED.into()
                },
            ],
        }
    );
    assert_eq!(input[3], history()[3]);
}

#[test]
fn w3_projected_media_fallback_keeps_full_native_and_opaque_text() {
    use base64::Engine as _;
    let full = "MCP text beyond the card preview ".repeat(350);
    let media = crate::mcp_result::project_rich(serde_json::from_value(json!({"content":[
        {"type":"text","text":full},
        {"type":"image","mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(png())}
    ]})).unwrap()).unwrap();
    assert!(media.display().len() < full.len());
    let mut log = crate::tools::TurnLog::new("prior", "m", "fixture");
    log.input = vec![
        InputItem::ProviderOutput(
            json!({"type":"function_call","id":"native","call_id":"native","name":"fixture__media","arguments":"{}"}),
        ),
        InputItem::McpFunctionCallOutput {
            call_id: "native".into(),
            output: media,
        },
        history()[2].clone(),
    ];
    let raw = log.to_json();
    let source = std::sync::Arc::new(
        crate::storage::WireLogSource::captured(std::sync::Arc::from(raw.to_string())).unwrap(),
    );
    let mut deferred = log.input.clone();
    crate::attachments::defer_images(&mut deferred, source).unwrap();
    for (vision, chat) in [(false, false), (true, true)] {
        let mut input = deferred.clone();
        project_media(&mut input, vision, chat);
        assert_eq!(
            input[1].call_output().unwrap().1,
            format!("{full}\n{IMAGE_OMITTED}")
        );
        assert_eq!(
            input[2].call_output().unwrap().1,
            format!("legacy text\n{IMAGE_OMITTED}")
        );
    }
    assert_eq!(log.to_json(), raw);
}
