use super::*;
use crate::tools::shell_call::invocation;

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "foreground".into(),
        name: name.into(),
        arguments,
    }
}

#[test]
fn tool13_source_defaults_zero_and_uncapped_timeout() {
    let env = BTreeMap::new();
    for (name, args, expected) in [
        (
            "shell",
            serde_json::json!({"command":"true"}),
            Duration::from_millis(120000),
        ),
        (
            "shell",
            serde_json::json!({"command":"true","timeout":0}),
            Duration::ZERO,
        ),
        (
            "shell",
            serde_json::json!({"command":"true","timeout":600000}),
            Duration::from_millis(600000),
        ),
        (
            "shell",
            serde_json::json!({"command":"true","timeout":600001}),
            Duration::from_millis(600001),
        ),
        (
            "bash",
            serde_json::json!({"argv":["true"],"timeout_ms":600001}),
            Duration::from_millis(600001),
        ),
        (
            "bash",
            serde_json::json!({"argv":["true"]}),
            Duration::from_millis(30000),
        ),
    ] {
        assert_eq!(
            invocation(&call(name, args), &env).unwrap().timeout,
            expected
        );
    }
    for args in [
        serde_json::json!({"command":"true","timeout":-1}),
        serde_json::json!({"command":"true","timeout":1.5}),
        serde_json::json!({"command":"true","timeout":null}),
        serde_json::json!({"command":"true","workdir":1}),
        serde_json::json!({"command":"true","timeout_ms":1}),
        serde_json::json!({"command":"true","argv":["true"]}),
        serde_json::json!({"command":"true","background":"yes"}),
        serde_json::json!({"command":"\u{0}"}),
    ] {
        assert!(invocation(&call("shell", args), &env).is_err());
    }
    for timeout in [None, Some(0), Some(123)] {
        let mut args = serde_json::json!({"command":"true","background":true});
        if let Some(timeout) = timeout {
            args["timeout"] = timeout.into();
        }
        let admitted = invocation(&call("shell", args), &env).unwrap();
        assert!(admitted.background);
        assert_eq!(
            admitted.timeout,
            Duration::from_millis(timeout.unwrap_or(0))
        );
    }
}

#[tokio::test]
async fn w8_long_argv_reaches_exec_and_os_e2big_is_a_tool_failure() {
    let env = setup();
    let context = ctx(&env, &AllowAllPolicy);
    let accepted = call(
        "bash",
        serde_json::json!({"argv":["/bin/true", "x".repeat(33 * 1024)]}),
    );
    let (state, _) = crate::tools::execute_shell_typed(&context, &accepted).await;
    assert_eq!(state, "completed");
    // SAFETY: sysconf reads this fixture process's page-size limit.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    assert!(page > 0);
    let too_large = call(
        "bash",
        serde_json::json!({"argv":["/bin/true", "x".repeat(32 * page as usize + 1)]}),
    );
    let (state, output) = crate::tools::execute_shell_typed(&context, &too_large).await;
    assert_eq!(state, "failed");
    assert!(
        output.contains("Argument list too long"),
        "actual exec E2BIG must remain a tool error: {output}"
    );
}

#[tokio::test]
async fn tool13_zero_keeps_cancel_group_reap_and_output_bounds() {
    let mut env = setup();
    env.env.insert("SHELL".into(), "/bin/bash".into());
    let cancel = AtomicBool::new(false);
    let mut context = ctx(&env, &AllowAllPolicy);
    context.cancel = &cancel;
    let call = call(
        "shell",
        serde_json::json!({"command":
        "printf '%s' $$ > pid; trap '' TERM; while :; do printf 'bounded-output'; done", "timeout":0}),
    );
    let cancel_after_spawn = async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while !env.project.join("pid").exists() {
            assert!(tokio::time::Instant::now() < deadline);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancel.store(true, std::sync::atomic::Ordering::Release);
    };
    let ((state, output), ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            crate::tools::execute_shell_typed(&context, &call),
            cancel_after_spawn
        )
    })
    .await
    .unwrap();
    assert_eq!(state, "cancelled");
    assert!(output.contains("[cancelled]"));
    assert!(output.len() <= 2 * crate::shell::RETAIN_CAP_BYTES + 128);
    let pid: i32 = std::fs::read_to_string(env.project.join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    // SAFETY: signal zero probes only the recorded fixture child, after reap.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
}

#[test]
fn tool12_scalar_alias_deny_and_resource_spelling_are_distinct() {
    use crate::config::Permission;
    let permissions = BTreeMap::from([
        ("shell".into(), Permission::Allow),
        ("bash".into(), Permission::Deny),
    ]);
    let policy = crate::runtime::RuntimePolicy::new(&permissions);
    for name in ["shell", "bash"] {
        assert_eq!(policy.effect(name, "true"), Permission::Deny);
    }
    let command = call(
        "shell",
        serde_json::json!({"command":"printf '%s' 'a b' && true"}),
    );
    let argv = call(
        "bash",
        serde_json::json!({"argv":["printf", "%s", "a b && true"]}),
    );
    assert_eq!(
        crate::tools::permission_resources(&command).unwrap(),
        ["printf '%s' 'a b'", "true"]
    );
    assert_eq!(
        crate::tools::permission_resources(&argv).unwrap(),
        ["printf %s 'a b && true'"]
    );
}

#[test]
fn w7_opaque_shell_input_is_not_an_authorizable_resource() {
    // TS shell-scan/bash-safety.test.ts: an empty command name cannot authorize
    // the redirect that the shell would perform before failing to execute it.
    let input = call("shell", serde_json::json!({"command":"'' > opaque"}));
    assert!(crate::tools::permission_resources(&input).is_err());
}

#[test]
fn w7_directory_and_declaration_have_no_shell_permission_resource() {
    let permissions = BTreeMap::from([("bash".into(), crate::config::Permission::Deny)]);
    let denied = crate::runtime::RuntimePolicy::new(&permissions);
    for command in ["cd .", "export X=1"] {
        let input = call("shell", serde_json::json!({"command":command}));
        assert!(
            crate::tools::permission_resources(&input)
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            crate::tools::ToolPolicy::check_call(&denied, &input),
            Err(crate::tools::ToolError::Denied { .. })
        ));
    }
}

#[test]
fn tool12_canonical_shell_rules_cover_both_native_names_in_last_match_order() {
    use crate::config::Permission;
    let rules = crate::permissions::PermissionRules::from_config(&serde_json::json!({
        "permissions":[
            {"action":"shell","resource":"*","effect":"deny"},
            {"action":"shell","resource":"printf *","effect":"allow"},
            {"action":"shell","resource":"printf held","effect":"ask"}
        ]
    }))
    .unwrap();
    for name in ["shell", "bash"] {
        assert_eq!(
            rules.evaluate(&BTreeMap::new(), name, "true"),
            Permission::Deny
        );
        assert_eq!(
            rules.evaluate(&BTreeMap::new(), name, "printf other"),
            Permission::Allow
        );
        assert_eq!(
            rules.evaluate(&BTreeMap::new(), name, "printf held"),
            Permission::Ask
        );
    }
}
