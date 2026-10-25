//! Source-derived normalization cases: DCP 3.2.0 `tests/prompts.test.ts` and
//! `lib/prompts/store.ts`, d637981555a18c3992472268a0657a948925d5fa,
//! AGPL-3.0-or-later; license path is recorded in the parent module.
use super::*;

#[test]
fn dcp_custom_prompts_normalize_reminders_without_corrupting_plain_mentions() {
    let plain = "Keep literal <dcp-system-reminder> mentions in prose.";
    for raw in [
        plain.to_string(),
        format!("\u{feff}<!-- private comment -->\r\n// legacy comment //\r\n{plain}\r\n"),
        format!("<DCP-SYSTEM-REMINDER priority=\"high\">\r\n{plain}\r\n</DCP-SYSTEM-REMINDER>"),
        format!("<manual>not editable</manual>{plain}<subagent>not editable</subagent>"),
    ] {
        assert_eq!(
            normalize(&raw, "system.md").as_deref(),
            Some(format!("<dcp-system-reminder>\n{plain}\n</dcp-system-reminder>").as_str())
        );
    }
    for invalid in [
        " \n<!-- empty -->\n",
        "<dcp-system-reminder>missing close",
        "missing open</dcp-system-reminder>",
        "<dcp-system-reminder></dcp-system-reminder>",
    ] {
        assert!(normalize(invalid, "system.md").is_none());
    }
}

#[test]
fn dcp_custom_prompts_leave_tool_prose_unwrapped_and_debug_redacted() {
    let body = "Use <dcp-message-id>m0007</dcp-message-id>; retain <manual>literal</manual>.";
    let prompt = normalize(body, "compress-range.md").unwrap();
    assert_eq!(prompt.as_ref(), body);
    let captured = DcpPromptOverrides {
        compress_range: Some(prompt),
        ..Default::default()
    };
    assert!(!format!("{captured:?}").contains(body));
}
