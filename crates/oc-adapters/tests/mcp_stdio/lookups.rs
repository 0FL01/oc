use super::*;
use oc_core::queries::{
    McpLookupData, McpLookupError, McpLookupOp, McpPromptContent, McpPromptRole,
    McpResourceContents,
};

#[tokio::test]
async fn mcp11_prompts_catalog_is_bounded_before_publication_and_owned_child_is_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("events");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mcp11-lookups.py");
    let mut config = fake_config(vec![]);
    config.argv = vec![
        "/usr/bin/python3".into(),
        script.to_string_lossy().into(),
        "oversized".into(),
        report.to_string_lossy().into(),
        "unused-gate".into(),
    ];
    let client = StdioClient::launch(&config).await.unwrap();
    let pid = client.process_group_id().unwrap() as libc::pid_t;
    assert_eq!(
        client
            .lookup(&McpLookupOp::ListPrompts, &AtomicBool::new(false))
            .await,
        Err(McpLookupError::CatalogLimit)
    );
    client.shutdown().await.unwrap();
    // SAFETY: signal zero probes only this fixture's owned child.
    assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    let events = fs::read_to_string(report).unwrap();
    assert_eq!(
        events
            .lines()
            .filter(|line| line.contains("prompts/list"))
            .count(),
        1
    );
    assert!(
        !events.contains("prompts/get"),
        "catalog metadata never loads a prompt body"
    );
}

fn lookup_config(dir: &Path, mode: &str) -> (StdioConfig, PathBuf) {
    let report = dir.join("events");
    let mut config = fake_config(vec![("LOOKUP_CANARY", "LOOKUP_SECRET_CANARY")]);
    config.argv = vec![
        "/usr/bin/python3".into(),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/mcp11-lookups.py")
            .display()
            .to_string(),
        mode.into(),
        report.display().to_string(),
        "unused-gate".into(),
    ];
    (config, report)
}

#[tokio::test]
async fn mcp11_sensitive_caller_identities_are_rejected_before_rpc_not_echoed_in_replies() {
    let dir = tempfile::tempdir().unwrap();
    let (config, report) = lookup_config(dir.path(), "basic");
    let client = StdioClient::launch(&config).await.unwrap();
    let cancel = AtomicBool::new(false);
    // The mode is an ordinary argv word, not a protected identity value.
    assert!(matches!(
        client.lookup(&McpLookupOp::GetPrompt {
            name: "basic".into(), arguments: BTreeMap::new()
        }, &cancel).await.unwrap(),
        McpLookupData::Prompt { name, .. } if name == "basic"
    ));
    for operation in [
        McpLookupOp::GetPrompt {
            name: "LOOKUP_SECRET_CANARY".into(),
            arguments: BTreeMap::new(),
        },
        McpLookupOp::GetPrompt {
            name: "outline".into(),
            arguments: BTreeMap::from([("LOOKUP_SECRET_CANARY".into(), "synthetic".into())]),
        },
        McpLookupOp::ReadResource {
            uri: "fixture://LOOKUP_SECRET_CANARY".into(),
        },
    ] {
        assert_eq!(
            client.lookup(&operation, &cancel).await,
            Err(McpLookupError::SensitiveIdentity)
        );
    }
    client.shutdown().await.unwrap();
    let events = fs::read_to_string(report).unwrap();
    assert_eq!(
        events
            .lines()
            .filter(|line| line.contains("prompts/get"))
            .count(),
        1
    );
    assert!(!events.contains("resources/read"));
    assert_eq!(
        events
            .lines()
            .filter(|line| line.contains("closed"))
            .count(),
        1
    );
}

#[tokio::test]
async fn mcp11_stdio_exact_catalogs_prompt_order_and_typed_binary_are_caller_only() {
    let dir = tempfile::tempdir().unwrap();
    let (config, report) = lookup_config(dir.path(), "basic");
    let client = StdioClient::launch(&config).await.unwrap();
    let cancel = AtomicBool::new(false);
    let McpLookupData::Prompts { available, items } = client
        .lookup(&McpLookupOp::ListPrompts, &cancel)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(available);
    assert_eq!(
        items.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["outline", "translate"]
    );
    assert_eq!(
        items[0]
            .arguments
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["topic", "style"]
    );
    assert_eq!(items[0].arguments[0].required, Some(true));
    let before = fs::read_to_string(&report).unwrap();
    assert!(!before.contains("prompts/get") && !before.contains("resources/read"));
    let data = client
        .lookup(
            &McpLookupOp::GetPrompt {
                name: "outline".into(),
                arguments: BTreeMap::from([("topic".into(), "synthetic".into())]),
            },
            &cancel,
        )
        .await
        .unwrap();
    let McpLookupData::Prompt { messages, .. } = data else {
        panic!()
    };
    assert_eq!(
        messages.iter().map(|m| m.role).collect::<Vec<_>>(),
        [McpPromptRole::User, McpPromptRole::Assistant]
    );
    assert!(
        matches!(&messages[0].content, McpPromptContent::Text { text } if text == "Lookup user text")
    );
    let McpLookupData::Resources { items, .. } = client
        .lookup(&McpLookupOp::ListResources, &cancel)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(items[0].uri, "fixture://text");
    let McpLookupData::ResourceTemplates { items, .. } = client
        .lookup(&McpLookupOp::ListResourceTemplates, &cancel)
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(items[0].uri_template, "fixture://{key}");
    let data = client
        .lookup(
            &McpLookupOp::ReadResource {
                uri: "file:///definitely-missing-fixture".into(),
            },
            &cancel,
        )
        .await
        .unwrap();
    assert!(
        !serde_json::to_string(&data)
            .unwrap()
            .contains("LOOKUP_SECRET_CANARY")
    );
    let McpLookupData::Resource { uri, contents } = data else {
        panic!()
    };
    assert_eq!(uri, "file:///definitely-missing-fixture");
    assert!(
        matches!(&contents[0], McpResourceContents::Text { uri, text, mime_type } if uri == "file:///definitely-missing-fixture" && text.ends_with("[redacted]") && mime_type.as_deref() == Some("text/plain"))
    );
    assert!(
        matches!(&contents[1], McpResourceContents::Blob { blob, mime_type, .. } if blob == "AAEC/w==" && mime_type.as_deref() == Some("application/octet-stream"))
    );
    client.shutdown().await.unwrap();
    let events = fs::read_to_string(report).unwrap();
    assert_eq!(events.lines().filter(|l| l.contains("spawn")).count(), 1);
    assert_eq!(
        events
            .lines()
            .filter(|l| l.contains("arguments_exact"))
            .count(),
        1
    );
}

#[tokio::test]
async fn mcp11_stdio_missing_capability_and_malicious_catalogs_are_bounded_non_success() {
    for (mode, expected, count) in [
        ("pages", McpLookupError::CatalogLimit, 16),
        ("loop", McpLookupError::CatalogLimit, 2),
        ("duplicate", McpLookupError::InvalidData, 1),
        ("metadata-secret", McpLookupError::SensitiveIdentity, 1),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (config, report) = lookup_config(dir.path(), mode);
        let client = StdioClient::launch(&config).await.unwrap();
        assert_eq!(
            client
                .lookup(&McpLookupOp::ListPrompts, &AtomicBool::new(false))
                .await,
            Err(expected)
        );
        client.shutdown().await.unwrap();
        assert_eq!(
            fs::read_to_string(report)
                .unwrap()
                .lines()
                .filter(|l| l.contains("prompts/list"))
                .count(),
            count
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let (config, report) = lookup_config(dir.path(), "absent");
    let client = StdioClient::launch(&config).await.unwrap();
    assert!(
        matches!(client.lookup(&McpLookupOp::ListPrompts, &AtomicBool::new(false)).await.unwrap(), McpLookupData::Prompts { available: false, items } if items.is_empty())
    );
    assert_eq!(
        client
            .lookup(
                &McpLookupOp::ReadResource {
                    uri: "fixture://text".into()
                },
                &AtomicBool::new(false)
            )
            .await,
        Err(McpLookupError::UnsupportedCapability)
    );
    client.shutdown().await.unwrap();
    assert!(
        !fs::read_to_string(report)
            .unwrap()
            .contains("resources/read")
    );
}

#[tokio::test]
async fn mcp11_stdio_body_limit_sensitive_blob_and_modern_interaction_never_partial_success() {
    for (mode, expected) in [
        ("body-limit", McpLookupError::BodyLimit),
        ("binary-secret", McpLookupError::SensitiveBinary),
        ("input-required", McpLookupError::UnsupportedResult),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (config, report) = lookup_config(dir.path(), mode);
        let client = StdioClient::launch(&config).await.unwrap();
        assert_eq!(
            client
                .lookup(
                    &McpLookupOp::ReadResource {
                        uri: "fixture://text".into()
                    },
                    &AtomicBool::new(false)
                )
                .await,
            Err(expected)
        );
        client.shutdown().await.unwrap();
        assert_eq!(
            fs::read_to_string(report)
                .unwrap()
                .lines()
                .filter(|l| l.contains("resources/read"))
                .count(),
            1,
            "no modern round or retry"
        );
    }
}
