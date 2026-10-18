use super::*;

async fn roundtrip<T: Serialize + DeserializeOwned>(value: &T) -> T {
    let (mut sender, mut receiver) = tokio::net::UnixStream::pair().unwrap();
    // These are the two process budgets, not a doubled allowance at one endpoint.
    let encoded = Message::encode(value, &Budget::default()).unwrap();
    let write = async {
        super::write(&mut sender, &encoded).await.unwrap();
    };
    let read = async {
        super::read(&mut receiver, &Budget::default())
            .await
            .unwrap()
            .decode::<T>()
            .unwrap()
    };
    let ((), value) = tokio::join!(write, read);
    value
}

#[tokio::test]
async fn srv01_wire_keeps_full_history_and_original_png_budgets() {
    let history = vec![
        "\0".repeat(oc_core::session::MAX_INPUT_BYTES);
        oc_core::queries::MAX_PROMPT_HISTORY_ENTRIES
    ];
    assert_eq!(roundtrip(&history).await, history);
    drop(history);
    let prompt = oc_core::prompt::PromptInput {
        text: "[Image 1] [Image 2]".into(),
        images: vec![
            oc_core::prompt::PromptImage {
                mark: oc_core::prompt::ImageMark {
                    start: 0,
                    end: 9,
                    ordinal: 1,
                },
                png: Arc::from(vec![1_u8; oc_core::prompt::IMAGE_BYTES]),
            },
            oc_core::prompt::PromptImage {
                mark: oc_core::prompt::ImageMark {
                    start: 10,
                    end: 19,
                    ordinal: 2,
                },
                png: Arc::from(vec![2_u8; oc_core::prompt::IMAGE_BYTES]),
            },
        ],
    };
    prompt.check_budget().unwrap();
    assert_eq!(roundtrip(&prompt).await, prompt);

    let (mut sender, mut receiver) = tokio::net::UnixStream::pair().unwrap();
    sender.write_u32(u32::MAX).await.unwrap();
    assert!(
        super::read(&mut receiver, &Budget::default())
            .await
            .is_err(),
        "refuse declared length before body allocation/read"
    );
}
