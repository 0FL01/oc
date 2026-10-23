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

/// A fanout clone shares one serialization and its single byte reservation;
/// the budget returns only after the last queued copy is gone.
#[test]
fn shared_message_reserves_its_bytes_once() {
    let budget = Budget::default();
    let full = budget.0.available_permits();
    let message = Message::encode(&"x".repeat(4096), &budget).unwrap();
    let reserved = full - budget.0.available_permits();
    assert!(reserved > 4096);
    let copies: Vec<_> = (0..8).map(|_| message.clone()).collect();
    assert_eq!(full - budget.0.available_permits(), reserved);
    drop(message);
    assert_eq!(full - budget.0.available_permits(), reserved);
    drop(copies);
    assert_eq!(budget.0.available_permits(), full);
}

/// Unflushed framing of several messages through one buffer decodes as the
/// same ordered messages, including one spanning several fragments.
#[tokio::test]
async fn batched_unflushed_messages_keep_order_and_fragments() {
    let (sender, mut receiver) = tokio::net::UnixStream::pair().unwrap();
    let budget = Budget::default();
    let values: Vec<String> = (0..20)
        .map(|n| {
            if n == 7 {
                "y".repeat(3 * FRAME + 5)
            } else {
                format!("m{n}")
            }
        })
        .collect();
    let messages: Vec<_> = values
        .iter()
        .map(|value| Message::encode(value, &budget).unwrap())
        .collect();
    let write = async move {
        let mut writer = tokio::io::BufWriter::with_capacity(FRAME, sender);
        for message in &messages {
            write_unflushed(&mut writer, message).await.unwrap();
        }
        writer.flush().await.unwrap();
    };
    let read = async {
        let mut received = Vec::new();
        for _ in 0..values.len() {
            received.push(
                super::read(&mut receiver, &Budget::default())
                    .await
                    .unwrap()
                    .decode::<String>()
                    .unwrap(),
            );
        }
        received
    };
    let ((), received) = tokio::join!(write, read);
    assert_eq!(received, values);
}
