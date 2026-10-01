use super::*;

#[tokio::test]
async fn tool20_write_overwrite_preserves_bytes_and_bom_without_read_call() {
    let env = setup();
    std::fs::write(env.project.join("note.txt"), "\u{feff}old\r\n").unwrap();
    let context = ctx(&env, &AllowAllPolicy, false);
    let call = ToolCall {
        id: "overwrite".into(),
        name: "write".into(),
        arguments: serde_json::json!({"path":"note.txt","content":"new\nEOF"}),
    };
    let outputs = execute_batch(&context, vec![Assembled::Call(call)]).await;
    assert_eq!(outputs[0].call_id, "overwrite");
    assert!(
        !outputs[0].output.starts_with("error:"),
        "{}",
        outputs[0].output
    );
    assert_eq!(
        std::fs::read(env.project.join("note.txt")).unwrap(),
        "\u{feff}new\nEOF".as_bytes()
    );
    let result: serde_json::Value = serde_json::from_str(&outputs[0].output).unwrap();
    assert_eq!(result["operation"], "write");
    assert_eq!(result["existed"], true);
}
