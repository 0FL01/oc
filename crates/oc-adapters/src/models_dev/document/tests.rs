use super::*;
use serde_json::{Value, json};

fn decode(parts: &[&[u8]]) -> Result<BTreeMap<String, Value>, DiscoveryError> {
    let mut chunks = Chunks::new();
    for part in parts {
        chunks.push(part)?;
    }
    chunks.finish().map(|document| document.0)
}

#[test]
fn member_framing_matches_serde_at_every_split_and_byte_boundary() {
    let cases = [
        "{}".to_owned(),
        " \n{\"foreign\":null,\"openai\":{\"models\":{}},\"opencode-go\":true}\t".into(),
        r#"{"opencode-go":null,"openai":{"k":1,"k":2},"opencode-go":{"id":"latest"}}"#.into(),
        r#"{"foreign":[null,true,false,-1,2,1.5,{"k":"},]:{\"\\\n"}],"open\u0061i":{"id":"UTF-8 λ🙂"}}"#.into(),
        "".into(), "[]".into(), "null".into(),
        r#"{"openai":{}} trailing"#.into(),
        r#"{"openai":{},}"#.into(),
        r#"{"openai":{},"foreign":[1,]}"#.into(),
        r#"{"openai":{},"foreign":{"x":}}"#.into(),
        r#"{"foreign":"\q"}"#.into(),
        r#"{"foreign":1e9999}"#.into(),
        r#"{"foreign":[}] }"#.into(),
        r#"{"foreign":}"#.into(),
        r#"{"foreign" "x"}"#.into(),
        r#"{"foreign":"unterminated}"#.into(),
        r#"{"foreign":true"#.into(),
        format!("{{\"foreign\":{}null{}}}", "[".repeat(126), "]".repeat(126)),
        format!("{{\"foreign\":{}null{}}}", "[".repeat(127), "]".repeat(127)),
        format!("{{\"foreign\":{}null{}}}", "[".repeat(128), "]".repeat(128)),
    ];
    for case in cases {
        let body = case.as_bytes();
        let expected = serde_json::from_slice::<PublicDocument>(body)
            .map(|document| document.0)
            .map_err(|_| DiscoveryError::InvalidResponse);
        for split in 0..=body.len() {
            assert_eq!(
                decode(&[&body[..split], &body[split..]]),
                expected,
                "split {split}: {case}"
            );
        }
        let bytes: Vec<_> = body.chunks(1).collect();
        assert_eq!(decode(&bytes), expected, "byte chunks: {case}");
    }
    for body in [b"{\"foreign\":\"\xff\"}".as_slice(), b"{\"\xff\":0}"] {
        assert_eq!(
            decode(&body.chunks(1).collect::<Vec<_>>()),
            Err(DiscoveryError::InvalidResponse)
        );
    }
}

#[test]
fn public_wire_retention_is_one_member_and_total_byte_cap_is_unchanged() {
    let value = json!({"models":[{"unknown": "x".repeat(32 * 1024)}]}).to_string();
    let fields = (0..100)
        .map(|index| format!("\"foreign-{index}\":{value}"))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!("{{{fields},\"openai\":{{\"id\":\"openai\"}}}}");
    assert!(body.len() > 3 * 1024 * 1024 && body.len() < DISCOVERY_BODY_CAP);
    let mut chunks = Chunks::new();
    for chunk in body.as_bytes().chunks(8192) {
        chunks.push(chunk).unwrap();
        assert!(chunks.member.capacity() <= 256 * 1024);
    }
    assert_eq!(
        chunks.finish().unwrap().0,
        BTreeMap::from([("openai".into(), json!({"id":"openai"}))])
    );

    let mut chunks = Chunks::new();
    chunks.push(b"{}").unwrap();
    let spaces = vec![b' '; 64 * 1024];
    for _ in 0..127 {
        chunks.push(&spaces).unwrap();
    }
    chunks.push(&spaces[..spaces.len() - 2]).unwrap();
    assert_eq!(chunks.bytes, DISCOVERY_BODY_CAP);
    assert_eq!(chunks.push(b" "), Err(DiscoveryError::InvalidResponse));
}
