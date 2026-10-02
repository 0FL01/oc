use super::*;

#[test]
fn tool21_invalid_output_limits_fail_before_generation_publication() {
    for raw in [
        "null",
        "[]",
        "0",
        "\"secret-sentinel\"",
        "{\"max_lines\":0}",
        "{\"max_bytes\":65537}",
        "{\"max_bytes\":1.5}",
        "{\"max_lines\":null}",
        "{\"max_lines\":18446744073709551616}",
    ] {
        let source = Source {
            path: "fixture.jsonc".into(),
            trusted: true,
            text: format!("{{\"tool_output\":{raw}}}"),
        };
        assert!(
            assemble(&[source], &BTreeMap::new(), None).is_err(),
            "invalid tool_output accepted: {raw}"
        );
    }
}

#[test]
fn tool21_latest_complete_section_defaults_and_provenance() {
    let source = |path: &str, text: &str| Source {
        path: path.into(),
        trusted: true,
        text: text.into(),
    };
    let first = source("global.jsonc", r#"{"tool_output":{"max_bytes":4096}}"#);
    let second = source("project.jsonc", r#"{"tool_output":{"max_lines":40}}"#);
    let generation = assemble(&[first, second], &BTreeMap::new(), None).unwrap();
    assert_eq!(
        generation.tool_output,
        ToolOutputLimits {
            max_lines: 40,
            max_bytes: 51200
        }
    );
    assert_eq!(generation.provenance["tool_output"], "project.jsonc");
    let generation = assemble(
        &[source("empty.jsonc", r#"{"tool_output":{}}"#)],
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert_eq!(generation.tool_output, ToolOutputLimits::default());
}
