use super::*;
use serde_json::{Value, json};

#[tokio::test]
async fn tool14_regex_default_and_canonical_scoped_options() {
    let env = setup();
    std::fs::create_dir_all(env.project.join("src/nested")).unwrap();
    std::fs::write(env.project.join("src/a.rs"), "foo12\nf.o\nÉCOLE\n").unwrap();
    std::fs::write(env.project.join("src/nested/b.txt"), "foo34\n").unwrap();
    let policy = AllowAllPolicy;
    let context = ctx(&env, &policy, false);
    for (name, arguments, expected) in [
        (
            "grep",
            json!({"pattern":"^foo[0-9]+$", "path":"src", "include":"*.rs"}),
            json!([{"path":"src/a.rs", "line":1, "text":"foo12"}]),
        ),
        (
            "grep",
            json!({"pattern":"f.o", "literal":true, "path":"src/a.rs"}),
            json!([{"path":"src/a.rs", "line":2, "text":"f.o"}]),
        ),
        (
            "grep",
            json!({"pattern":"école", "caseSensitive":false, "path":"src/a.rs"}),
            json!([{"path":"src/a.rs", "line":3, "text":"ÉCOLE"}]),
        ),
        (
            "glob",
            json!({"pattern":"*.{rs,txt}", "path":"src", "hidden":false}),
            json!(["src/a.rs", "src/nested/b.txt"]),
        ),
    ] {
        let call = ToolCall {
            id: "search".into(),
            name: name.into(),
            arguments,
        };
        let output = execute_batch(&context, vec![Assembled::Call(call)]).await;
        let value: Value = serde_json::from_str(&output[0].output)
            .unwrap_or_else(|_| panic!("{}", output[0].output));
        assert_eq!(
            value[if name == "grep" { "matches" } else { "items" }],
            expected
        );
        assert_eq!(value["pagination"]["truncated"], false);
    }
}
