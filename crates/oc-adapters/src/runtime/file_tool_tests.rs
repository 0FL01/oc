use super::super::*;

#[test]
fn tool20_dcp_protection_inspects_parsed_path_not_replacement_content() {
    let config = DcpConfig {
        protected_file_patterns: vec!["private/**".into()],
        ..Default::default()
    };
    for (name, args, protected) in [
        (
            "write",
            serde_json::json!({"path":"file","content":"private/secret"}),
            false,
        ),
        (
            "edit",
            serde_json::json!({"path":"file","oldString":"private/old","newString":"private/new"}),
            false,
        ),
        (
            "write",
            serde_json::json!({"path":"private/file","content":"safe"}),
            true,
        ),
        (
            "edit",
            serde_json::json!({"path":"private/file","oldString":"x","newString":"y"}),
            true,
        ),
    ] {
        assert_eq!(
            super::dcp_call_has_protected_path(name, &args.to_string(), &config),
            protected
        );
    }
    assert!(!super::dcp_call_has_protected_path(
        "remote__write",
        r#"{"path":"file","content":"safe"}"#,
        &config
    ));
}

#[test]
fn tool12_selected_schema_guidance_case_sensitive_and_policy_narrowing() {
    for (id, patch) in [
        ("unknown", false),
        ("gpt-next", true),
        ("pre-gpt-next-post", true),
        ("gpt-oss", false),
        ("gpt-4.9", false),
        ("GPT-next", false),
        ("gpt-Next", true),
        ("gpt-next-OSS", true),
        ("gpt-next-oss-gpt-4", false),
    ] {
        let mut defs = selected_tool_defs(id);
        assert_eq!(defs.iter().any(|t| t.name == "apply_patch"), patch, "{id}");
        assert_eq!(defs.iter().any(|t| t.name == "write"), !patch, "{id}");
        assert_eq!(defs.iter().any(|t| t.name == "edit"), !patch, "{id}");
        let mut permissions = BTreeMap::new();
        permissions.insert("apply_patch".into(), Permission::Deny);
        let policy = RuntimePolicy::new(&permissions);
        defs.retain(|t| policy.tool_visible(&t.name));
        assert!(
            !defs
                .iter()
                .any(|t| matches!(t.name.as_str(), "apply_patch" | "edit" | "write"))
        );
        assert!(file_tool_guidance(&defs).is_none());
    }
}
