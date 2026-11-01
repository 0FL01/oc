//! Validate the independent ID control lane before legacy protocol/resource
//! fixtures compare conversational wire. The DCP controls native fixture keeps
//! and qualifies the exact, unmodified request including this lane.

pub fn check_context_ids(request: &mut serde_json::Value) {
    check_execution_context(request);
    // Validate chronological date notices before comparing legacy conversation-only wire.
    request["input"].as_array_mut().unwrap().retain(|item| {
        if item["role"] != "developer" {
            return true;
        }
        let Some(date) = item["content"][0]["text"].as_str().and_then(|text| {
            text.strip_prefix("Today's date: ")
                .or_else(|| text.strip_prefix("Today's date is now: "))
        }) else {
            return true;
        };
        assert_eq!(item["type"], "message");
        assert_eq!(item["content"][0]["type"], "input_text");
        let (ymd, weekday) = date.split_once(" (").expect("date and weekday");
        let bytes = ymd.as_bytes();
        assert_eq!(bytes.len(), 10);
        assert!(bytes.iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        }));
        assert!(matches!(
            weekday,
            "Sunday)"
                | "Monday)"
                | "Tuesday)"
                | "Wednesday)"
                | "Thursday)"
                | "Friday)"
                | "Saturday)"
        ));
        false
    });
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

/// Validate volatile execution metadata before legacy conversation-only equality.
/// R7's native fixture retains exact complete root/child/title/Location requests.
fn check_execution_context(request: &mut serde_json::Value) {
    const OPEN: &str = "<oc-execution-environment>\n";
    const CLOSE: &str = "\n</oc-execution-environment>";
    let input = request["input"].as_array_mut().expect("request input");
    let environments = input
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            item["content"][0]["text"]
                .as_str()
                .filter(|t| t.contains(OPEN))
                .map(|_| i)
        })
        .collect::<Vec<_>>();
    assert!(
        environments.len() <= 1,
        "environment lane must not accumulate"
    );
    if let Some(index) = environments.first().copied() {
        let item = input.remove(index);
        assert_eq!(item["type"], "message");
        assert_eq!(item["role"], "developer");
        assert_eq!(item["content"].as_array().unwrap().len(), 1);
        let text = item["content"][0]["text"].as_str().unwrap();
        assert_eq!(text.matches(OPEN).count(), 1);
        assert_eq!(text.matches(CLOSE).count(), 1);
        let json = text
            .split_once(OPEN)
            .unwrap()
            .1
            .strip_suffix(CLOSE)
            .unwrap();
        assert!(
            !json
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>' | '&'))
        );
        let facts: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(facts["processPointerBits"], usize::BITS);
        assert_eq!(facts["processArchitecture"], std::env::consts::ARCH);
        for field in ["workingDirectory", "workspaceRoot"] {
            assert!(std::path::Path::new(facts[field].as_str().unwrap()).is_absolute());
        }
        assert!(facts["effectiveUid"].is_u64() && facts["effectiveGid"].is_u64());
        assert!(facts.get("dateUtc").is_none());
    }
    let bases = input
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            item["content"][0]["text"]
                .as_str()
                .filter(|t| t.starts_with("You are OpenCode, a native coding assistant."))
                .map(|_| i)
        })
        .collect::<Vec<_>>();
    assert!(bases.len() <= 1, "base lane must not accumulate");
    if let Some(index) = bases.first().copied() {
        let item = input.remove(index);
        assert_eq!(item["role"], "developer");
        assert!(
            item["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("effective permissions")
        );
    }
}
