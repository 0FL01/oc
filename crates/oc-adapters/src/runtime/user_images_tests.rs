//! Clipboard input admission, original media ownership and RAW/HOT restoration.
use super::tests::binding_replay_tests::{owner, request, respond};
use super::*;
use crate::provider::protocol::Protocol;
use oc_core::prompt::{ImageMark, PromptImage, PromptInput};
use serde_json::{Value, json};

fn prompt() -> PromptInput {
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    PromptInput {
        text: "Ω [Image 1]".into(),
        images: vec![PromptImage {
            mark: ImageMark {
                start: 3,
                end: 12,
                ordinal: 1,
            },
            png: bytes.into_inner().into(),
        }],
    }
}

#[tokio::test]
async fn clipboard_images_atomic_admission_wire_retirement_reopen_revert_and_fork() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let provider = ResponsesConfig {
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        api_key: "synthetic".into(),
        timeout: None,
        header_timeout_ms: Some(crate::provider::HEADER_TIMEOUT_MS),
        chunk_timeout_ms: Some(3000),
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        cache: crate::provider::CacheMode::None,
        wire: Default::default(),
    };
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: crate::models::Models::from([(
            "m".into(),
            json!({"modalities":{"input":["text","image"]},"limit":{"context":500000,"output":2048}}),
        )]),
    };
    let original = prompt();
    let cancel = AtomicBool::new(false);
    let db = Db::open(data.path()).unwrap();
    let rt = owner(&db, project.path());
    let mut image_turn = String::new();
    for fresh in [true, false] {
        let params = TurnParams {
            session: "s".into(),
            prompt: original.clone(),
            invocation: None,
            catalog: &catalog,
            model_id: "m".into(),
            variant: None,
            max_output: 2048,
            provider: provider.clone(),
            cancel: &cancel,
        };
        let accepted = |turn: &str| {
            // This callback is before instructions/provider preparation. A new
            // connection must already see the complete image-bearing input.
            let reopened = rusqlite::Connection::open_with_flags(
                data.path().join("oc.sqlite"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .unwrap();
            let raw: String = reopened
                .query_row("SELECT result FROM turns WHERE id=?1", [turn], |r| r.get(0))
                .unwrap();
            let value: Value = serde_json::from_str(&raw).unwrap();
            let log = TurnLog::from_json(&value).unwrap();
            assert!(log.requests.is_empty());
            assert!(value["display"]["user_images"].is_object());
            let anchor = log.user_message.clone().unwrap();
            assert_eq!(
                db.user_images("s", &anchor, &original.text).unwrap(),
                vec![original.images[0].mark.clone()]
            );
            let metadata = crate::attachments::image_metadata(&original.text, &value["display"])
                .unwrap()
                .unwrap();
            assert_eq!(
                crate::attachments::restore_prompt(
                    original.text.clone(),
                    &metadata,
                    &log.input[metadata.input_index]
                )
                .unwrap(),
                original
            );
            image_turn = turn.into();
        };
        let run = async {
            if fresh {
                rt.run_fresh_turn_with_tool_events(
                    params,
                    None,
                    accepted,
                    |_, _| {},
                    |_, _| {},
                    |_, _| {},
                )
                .await
            } else {
                rt.run_turn_with_tool_events(params, accepted, |_, _| {}, |_, _| {}, |_, _| {})
                    .await
            }
        };
        let peer = async {
            let (mut socket, wire) = request(&listener).await;
            let images = wire["input"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|item| item["content"].as_array().into_iter().flatten())
                .filter(|part| part["type"] == "input_image")
                .count();
            assert_eq!(images, if fresh { 1 } else { 2 });
            respond(&mut socket, Protocol::Responses, false).await;
        };
        let (result, ()) = tokio::join!(run, peer);
        result.unwrap();
    }
    // Exercise the existing RAW/HOT transaction on an admitted, still-active
    // closed input graph; completed runtime turns cannot be retired again.
    let raw = db.turn_result(&image_turn).unwrap().1.unwrap();
    let closed = TurnLog::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();
    let mut log = TurnLog::new("retired-image", "m", "fixture");
    log.binding = closed.binding.clone();
    log.protocol = closed.protocol;
    log.input
        .push(crate::attachments::prepare_prompt(&original, &cancel).unwrap());
    log.display = json!({"user_images":{"input_index":0,"images":[original.images[0].mark]}});
    let user = db
        .accept_turn_with_selection(
            &log.turn_id.clone(),
            "s",
            &original.text,
            &original.text,
            &oc_core::queries::ModelRef {
                provider: "fixture".into(),
                id: "m".into(),
                variant: None,
            },
            None,
            None,
            Some(&original.text),
            Some(&mut log),
        )
        .unwrap()
        .user_message;
    // Checkpoint a closed response using the preceding real wire receipt/output,
    // with a distinct span identity for this storage-owned lifecycle fixture.
    let receipt = oc_core::queries::RequestIdentity {
        span: "retired-span".into(),
        ..closed.requests[0].clone()
    };
    assert_eq!(log.input.len(), receipt.input_start);
    log.requests.push(receipt.clone());
    log.spans.push(oc_core::queries::AssistantSpan {
        id: receipt.span.clone(),
        request: Some(receipt),
        ..closed.spans[0].clone()
    });
    log.input.extend(closed.input.into_iter().skip(1));
    db.checkpoint_turn(&log.turn_id, &log.to_json().to_string())
        .unwrap();
    let counts = log.closed_counts();
    let working = log
        .renewed_task_checkpoint(
            "Continue without the original image.",
            counts,
            counts[0],
            |_| false,
        )
        .unwrap();
    let (segment, hot) = log.prepare_closed_segment(counts, working, 0).unwrap();
    db.commit_closed_turn_segment(&log, &segment, &hot, None)
        .unwrap();
    db.commit_turn(
        &log.turn_id,
        "completed",
        Some(&hot.to_json().to_string()),
        Some("image checkpoint done"),
    )
    .unwrap();
    assert!(!hot.input_for_bound("m", "fixture", hot.binding.as_ref(), None).iter().any(|item|
        matches!(item, InputItem::Message { content, .. } if content.iter().any(|p| matches!(p, crate::provider::InputContent::InputImage { .. })))));
    drop(rt);
    drop(db);
    let db = Db::open(data.path()).unwrap();
    assert_eq!(
        db.user_images("s", &user, &original.text).unwrap(),
        vec![original.images[0].mark.clone()]
    );
    let fork = db
        .fork_session("s", &user, "work", "fixture", "{}")
        .unwrap();
    assert_eq!(fork.prompt, original.text);
    assert_eq!(fork.images, original.images);
    let reverted = db
        .change_conversation(
            "s",
            oc_core::queries::ConversationAction::Revert {
                message: oc_core::session::MessageId(user),
            },
        )
        .unwrap();
    assert_eq!(reverted.draft.as_deref(), Some(original.text.as_str()));
    assert_eq!(reverted.images, original.images);
}

#[tokio::test]
async fn clipboard_images_refuse_invalid_media_and_unknown_capability_before_fresh_root() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let db = Db::open(data.path()).unwrap();
    let rt = owner(&db, project.path());
    let catalog = ModelCatalog {
        provider: "fixture".into(),
        models: crate::models::Models::from([(
            "m".into(),
            json!({"limit":{"context":500000,"output":2048}}),
        )]),
    };
    let cancel = AtomicBool::new(false);
    // No listener: admission must not dispatch any generation request.
    let provider = ResponsesConfig {
        base_url: "http://127.0.0.1:1/v1".into(),
        api_key: "synthetic".into(),
        timeout: None,
        header_timeout_ms: None,
        chunk_timeout_ms: None,
        connect_timeout: Duration::from_secs(1),
        allow_private: true,
        headers: BTreeMap::new(),
        cache: crate::provider::CacheMode::None,
        wire: Default::default(),
    };
    let mut invalid = prompt();
    invalid.images[0].png = b"\x89PNG\r\n\x1a\n".as_slice().into();
    for input in [invalid, prompt()] {
        let result = rt
            .run_fresh_turn_with_tool_events(
                TurnParams {
                    session: "refused".into(),
                    prompt: input,
                    invocation: None,
                    catalog: &catalog,
                    model_id: "m".into(),
                    variant: None,
                    max_output: 2048,
                    provider: provider.clone(),
                    cancel: &cancel,
                },
                None,
                |_| panic!("must not accept"),
                |_, _| {},
                |_, _| {},
                |_, _| {},
            )
            .await;
        assert!(
            matches!(result, Err(RuntimeError::InvalidArgs(_))),
            "{result:?}"
        );
        assert!(matches!(
            db.session_meta("refused"),
            Err(StorageError::SessionNotFound)
        ));
    }
    let mut invalid = prompt();
    invalid.images[0].mark.start = 1; // splits Ω
    assert!(crate::attachments::prepare_prompt(&invalid, &cancel).is_err());
    let mut item = crate::attachments::prepare_prompt(&prompt(), &cancel).unwrap();
    if let InputItem::Message { content, .. } = &mut item
        && let crate::provider::InputContent::InputImage { image_url, .. } = &mut content[1]
    {
        *image_url = format!("data:image/png;base64,{}", "A".repeat(64 * 1024));
    }
    let wire = serde_json::to_string(&item).unwrap();
    assert!((1500..1600).contains(&estimate_input_tokens_from_bytes(
        wire.len() as u64,
        std::iter::once(&item)
    )));
}
