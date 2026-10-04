//! Validate the independent ID control lane before legacy protocol/resource
//! fixtures compare conversational wire. The DCP controls native fixture keeps
//! and qualifies the exact, unmodified request including this lane.

pub fn check_context_ids(request: &mut serde_json::Value) {
    const PREFIX: &str = "Stable text-message IDs in order (context selection only): ";
    let input = request["input"].as_array_mut().expect("request input");
    let positions = input
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item["content"][0]["text"]
                .as_str()
                .filter(|text| text.starts_with(PREFIX))
                .map(|_| index)
        })
        .collect::<Vec<_>>();
    assert!(positions.len() <= 1, "ID lane must not accumulate");
    if let Some(index) = positions.first().copied() {
        let item = input.remove(index);
        assert_eq!(item["type"], "message");
        assert_eq!(item["role"], "developer");
        assert_eq!(item["content"].as_array().unwrap().len(), 1);
        let anchors: serde_json::Value = serde_json::from_str(
            item["content"][0]["text"]
                .as_str()
                .unwrap()
                .strip_prefix(PREFIX)
                .unwrap(),
        )
        .unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for anchor in anchors.as_array().unwrap() {
            assert_eq!(
                anchor.as_object().unwrap().len(),
                2,
                "off lane has only IDs and roles"
            );
            let id = anchor["id"].as_str().unwrap();
            assert!(
                id.len() >= 5
                    && matches!(id.as_bytes()[0], b'm' | b'b')
                    && id[1..].bytes().all(|b| b.is_ascii_digit()),
                "stable native ID: {id}"
            );
            assert!(seen.insert(id.to_owned()), "repeated context ID");
            assert!(matches!(
                anchor["role"].as_str(),
                Some("user" | "assistant" | "system")
            ));
        }
    }
}
