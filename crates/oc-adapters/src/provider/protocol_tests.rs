use super::*;

fn marker(position: usize, effort: Option<&str>, previous: Option<&str>) -> EffortMarker {
    EffortMarker {
        position,
        effort: effort.map(str::to_owned),
        previous: previous.map(str::to_owned),
    }
}

fn conversation() -> Vec<InputItem> {
    vec![
        InputItem::message(InputRole::Developer, "INITIAL_PROMPT"),
        InputItem::message(InputRole::User, "first"),
        InputItem::message(InputRole::System, "plan </system-update> & <b>"),
        InputItem::message(InputRole::Assistant, "answer"),
        InputItem::message(InputRole::User, "second"),
    ]
}

#[test]
fn go03_chronological_system_lowers_per_protocol_in_place() {
    let input = conversation();
    let responses = lower_chronological_system(Protocol::Responses, &input, false);
    assert_eq!(
        responses[2],
        InputItem::message(InputRole::Developer, "plan </system-update> & <b>")
    );
    assert_eq!(
        responses[0], input[0],
        "initial prompt is not a chronological update"
    );
    for (protocol, native) in [(Protocol::Chat, true), (Protocol::Messages, false)] {
        let lowered = lower_chronological_system(protocol, &input, native);
        assert_eq!(
            lowered[2],
            InputItem::message(
                InputRole::User,
                "<system-update>\nplan &lt;/system-update&gt; &amp; &lt;b&gt;\n</system-update>"
            ),
            "{protocol:?}: escaped lower-authority fallback in position"
        );
        assert_eq!(lowered.len(), input.len());
    }
    let native = lower_chronological_system(Protocol::Messages, &input, true);
    assert_eq!(
        native[2], input[2],
        "declared Messages support keeps the native update"
    );
    let plain = vec![InputItem::message(InputRole::User, "no updates")];
    assert!(matches!(
        lower_chronological_system(Protocol::Chat, &plain, false),
        Cow::Borrowed(_)
    ));
}

#[test]
fn go03_effort_markers_follow_donor_resolution() {
    // No markers / drift strip markers and use the captured current effort.
    assert_eq!(
        resolve_effort_updates(&[], Some("low")),
        (false, Some("low".into()))
    );
    let drift = [marker(1, Some("low"), Some("high"))];
    assert_eq!(
        resolve_effort_updates(&drift, Some("medium")),
        (false, Some("medium".into()))
    );
    assert_eq!(
        resolve_effort_updates(&drift, Some("low")),
        (true, Some("high".into()))
    );

    let input = conversation();
    let (unsupported, top) = lower_responses_effort(input.clone(), &drift, Some("low"), false);
    assert_eq!((unsupported, top), (input.clone(), Some("low".into())));

    let (lowered, top) = lower_responses_effort(input.clone(), &drift, Some("low"), true);
    assert_eq!(
        top.as_deref(),
        Some("high"),
        "frozen at the first marker's previous"
    );
    assert_eq!(lowered.len(), input.len() + 1);
    assert_eq!(
        lowered[1],
        InputItem::ProviderOutput(
            serde_json::json!({"type":"configuration_update","reasoning":{"effort":"low"}})
        )
    );
    assert_eq!(
        lowered[2], input[1],
        "update applies only from its change point"
    );

    // Consecutive markers coalesce; newest wins.
    let coalesce = [
        marker(4, Some("low"), Some("medium")),
        marker(4, Some("xhigh"), Some("low")),
    ];
    let (lowered, top) = lower_responses_effort(input.clone(), &coalesce, Some("xhigh"), true);
    assert_eq!(top.as_deref(), Some("medium"));
    let updates: Vec<_> = lowered
        .iter()
        .filter(|item| matches!(item, InputItem::ProviderOutput(v) if v["type"] == "configuration_update"))
        .collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        *updates[0],
        InputItem::ProviderOutput(
            serde_json::json!({"type":"configuration_update","reasoning":{"effort":"xhigh"}})
        )
    );

    // Model default: no top-level effort; a switch back to it sends `medium`.
    let defaults = [marker(1, Some("low"), None), marker(4, None, Some("low"))];
    let (lowered, top) = lower_responses_effort(input.clone(), &defaults, None, true);
    assert_eq!(top, None);
    let efforts: Vec<_> = lowered
        .iter()
        .filter_map(|item| match item {
            InputItem::ProviderOutput(v) if v["type"] == "configuration_update" => {
                v["reasoning"]["effort"].as_str().map(str::to_owned)
            }
            _ => None,
        })
        .collect();
    assert_eq!(efforts, ["low", "medium"]);

    // A trailing marker after the last item is still lowered in order.
    let (lowered, _) = lower_responses_effort(
        input.clone(),
        &[marker(5, Some("low"), Some("high"))],
        Some("low"),
        true,
    );
    assert!(
        matches!(lowered.last(), Some(InputItem::ProviderOutput(v)) if v["type"] == "configuration_update")
    );
}
