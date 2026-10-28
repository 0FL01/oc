use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dcp_custom_prompts_reach_wire_and_reload_on_the_next_same_task_request() {
    let fixture = Fixture::new();
    let global = fixture.home.join("config/opencode");
    let overrides = global.join("dcp-prompts/overrides");
    fs::create_dir_all(&overrides).unwrap();
    let config_path = global.join("opencode.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["plugins"] = json!(["@tarquinen/opencode-dcp@3.2.0"]);
    fs::write(config_path, config.to_string()).unwrap();
    fs::write(
        global.join("dcp.jsonc"),
        r#"{"experimental":{"customPrompts":true}}"#,
    )
    .unwrap();
    fs::write(overrides.join("system.md"), "CUSTOM_SYSTEM_G1").unwrap();
    fs::write(overrides.join("compress-range.md"), "CUSTOM_RANGE_BODY").unwrap();
    fs::write(
        overrides.join("context-limit-nudge.md"),
        "CUSTOM_CONTEXT_BODY",
    )
    .unwrap();

    let mut process = fixture.spawn("custom-prompts", "Use glob then finish", "custom-prompts");
    let (mut socket, first) = fixture.accept(&mut process);
    assert_dcp_tool_schemas(&first);
    let first_text = message_text(&first);
    assert_eq!(first_text.matches("CUSTOM_SYSTEM_G1").count(), 1);
    assert_eq!(first_text.matches("CUSTOM_CONTEXT_BODY").count(), 1);
    assert!(
        !first_text.contains("MAX CONTEXT LIMIT REACHED"),
        "the override replaces the donor body"
    );
    let original_tool = tool_definition(&first, "compress");
    let description = original_tool["description"].as_str().unwrap();
    assert_eq!(description.matches("CUSTOM_RANGE_BODY").count(), 1);
    assert!(description.contains("NATIVE RULES"));
    assert!(description.contains("renew the whole running task"));

    let (observer, guard, _) = oc_adapters::native::attach(
        &fixture.data(),
        Some(&fixture.project),
        oc_adapters::native::Lease::Run,
    )
    .await
    .unwrap();
    let initial = observer
        .catalog()
        .await
        .unwrap()
        .chrome
        .selection_generation;
    fs::write(overrides.join("atomic-system"), "CUSTOM_SYSTEM_G2").unwrap();
    fs::rename(overrides.join("atomic-system"), overrides.join("system.md")).unwrap();
    // Wait for actual publication, not a guessed watcher/debounce delay. The
    // first provider stream is still open and retains its original snapshot.
    tokio::time::timeout(TIMEOUT, async {
        loop {
            if observer
                .catalog()
                .await
                .unwrap()
                .chrome
                .selection_generation
                > initial
            {
                break;
            }
            tokio::time::sleep(POLL).await;
        }
    })
    .await
    .expect("prompt-only source edit must be observed and published");
    respond_tool(
        &mut socket,
        "glob",
        json!({"pattern":"search/*.rs"}),
        "custom-prompts-glob",
    );
    let (mut socket, second) = fixture.accept(&mut process);
    let second_text = message_text(&second);
    assert_eq!(second_text.matches("CUSTOM_SYSTEM_G2").count(), 1);
    assert!(!second_text.contains("CUSTOM_SYSTEM_G1"));
    assert_eq!(
        second_text.matches("CUSTOM_CONTEXT_BODY").count(),
        1,
        "prompt reload must preserve one active context reminder"
    );
    assert_eq!(
        tool_definition(&second, "compress")["parameters"],
        original_tool["parameters"]
    );
    assert!(!function_output(&second, "custom-prompts-glob", "glob").starts_with("error:"));
    respond_text(&mut socket, "custom prompts complete");
    assert!(
        fixture.wait_with_title(&mut process).success(),
        "{}",
        process.diagnostics()
    );
    observer.shutdown().await.unwrap();
    guard.join().await.unwrap();
}
