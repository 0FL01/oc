use super::*;
use crate::provider::InputItem;
use crate::tools::TurnLog;
use oc_core::prompt::ImageMark;

fn png() -> std::sync::Arc<[u8]> {
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner().into()
}

fn image(png: std::sync::Arc<[u8]>) -> QuestionImage {
    QuestionImage {
        question: 0,
        answer: 0,
        mark: ImageMark {
            start: 4,
            end: 13,
            ordinal: 1,
        },
        png,
    }
}

#[test]
fn pasted_answer_images_lower_to_provider_media_and_survive_the_turn_row() {
    let cancel = AtomicBool::new(false);
    assert!(
        QuestionToolOutput::new(
            "answers".into(),
            &[image(b"not a png".as_slice().into())],
            &cancel
        )
        .is_err(),
        "unverified bytes never reach a provider"
    );
    let output = QuestionToolOutput::new("answers".into(), &[image(png())], &cancel).unwrap();
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire[0], json!({"type":"input_text","text":"answers"}));
    assert_eq!(wire[1]["text"], "Question 1 answer [Image 1]:");
    assert_eq!(wire[2]["type"], "input_image");
    assert!(
        wire[2]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    assert_eq!(output.media().0, 1);

    let mut log = TurnLog::new("turn", "model", "provider");
    log.input.push(InputItem::ProviderOutput(
        json!({"type":"function_call","call_id":"ask","name":"question","arguments":"{}"}),
    ));
    log.input.push(InputItem::QuestionFunctionCallOutput {
        call_id: "ask".into(),
        output,
    });
    let stored = log.to_json();
    assert_eq!(stored["input"][1]["output"], "answers");
    assert!(stored["native_question_results"][0]["result"]["images"][0]["data"].is_string());
    assert_eq!(TurnLog::from_json(&stored).unwrap(), log);

    let mut bad = stored.clone();
    bad["input"][0]["name"] = "read".into();
    assert!(
        TurnLog::from_json(&bad).is_err(),
        "graph names the question tool"
    );
    bad = stored.clone();
    bad["input"][1]["output"] = "other".into();
    assert_eq!(
        TurnLog::from_json(&bad).unwrap().input[1],
        InputItem::QuestionFunctionCallOutput {
            call_id: "ask".into(),
            output: QuestionToolOutput::from_stored(
                stored["native_question_results"][0]["result"].clone(),
                "other".into()
            )
            .unwrap(),
        },
        "the stored text row stays the displayed answer"
    );
    bad = stored.clone();
    bad["native_question_results"][0]["result"]["images"][0]["extra"] = true.into();
    assert!(TurnLog::from_json(&bad).is_err());
    bad = stored.clone();
    bad["native_question_results"][0]["result"]["images"][0]["data"] = "AAAA".into();
    assert!(TurnLog::from_json(&bad).is_err());
}
