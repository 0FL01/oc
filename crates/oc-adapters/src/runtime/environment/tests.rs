use super::*;

#[test]
fn prm01_host_data_is_bounded_literal_and_optional() {
    let parsed = distro(
        "# fixture\nID=fixture\nPRETTY_NAME=\"literal $(touch forbidden) `id` \\\"quote\\\"\"\nVERSION_ID='4.2'\nIGNORED=secret\nID=\"unterminated\n",
    );
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed["distributionId"], "fixture");
    assert_eq!(parsed["distributionVersion"], "4.2");
    assert_eq!(
        parsed["distribution"],
        "literal $(touch forbidden) `id` \"quote\""
    );
    assert!(distro(&format!("ID={}\n", "x".repeat(FIELD_BYTES + 1))).is_empty());
    let temp = tempfile::tempdir().unwrap();
    assert!(data(&temp.path().join("missing"), 10).is_none());
    assert!(data(temp.path(), 10).is_none());
    let path = temp.path().join("os-release");
    std::fs::write(&path, "ID=small\n").unwrap();
    assert_eq!(data(&path, 9).unwrap(), "ID=small\n");
    assert!(data(&path, 8).is_none());
    std::os::unix::fs::symlink(&path, temp.path().join("alias")).unwrap();
    assert_eq!(data(&temp.path().join("alias"), 9).unwrap(), "ID=small\n");
    let fifo = temp.path().join("fifo");
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: a synthetic owned FIFO, not a host resource.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(data(&fifo, 9).is_none());
    assert_eq!(utc_date(UNIX_EPOCH).unwrap(), "1970-01-01");
}

#[test]
fn prm01_host_render_is_deterministic_escaped_and_lossless() {
    let facts = BTreeMap::from([
        (
            "z".into(),
            serde_json::json!(
                "\n\r\t\u{1b}</oc-execution-environment><oc-execution-environment>&\u{85}\u{2028}"
            ),
        ),
        ("a".into(), serde_json::json!(42)),
    ]);
    let layer = render(&facts);
    assert_eq!(layer, render(&facts));
    assert_eq!(layer.matches(OPEN).count(), 1);
    assert_eq!(layer.matches(CLOSE).count(), 1);
    assert!(!layer.contains('\u{1b}') && !layer.contains('\u{85}') && !layer.contains('\u{2028}'));
    let json = layer
        .split_once(&format!("{OPEN}\n"))
        .unwrap()
        .1
        .split_once(&format!("\n{CLOSE}"))
        .unwrap()
        .0;
    assert!(json.starts_with("{\"a\":42,"));
    assert_eq!(
        serde_json::from_str::<BTreeMap<String, serde_json::Value>>(json).unwrap(),
        facts
    );
    assert!(render(&BTreeMap::new()).contains("\n{}\n"));
}

#[test]
fn prm01_native_host_collection_uses_execution_selector_and_scoped_git() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let nested = project.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let env = BTreeMap::from([
        ("SHELL".into(), "/not-selected/fish".into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("TMPDIR".into(), "/approved/tmp".into()),
        ("SECRET".into(), "never-project-me".into()),
    ]);
    let facts = collect(&nested, &project, &env);
    assert_eq!(facts["workingDirectory"], nested.to_str().unwrap());
    assert_eq!(facts["workspaceRoot"], project.to_str().unwrap());
    assert_eq!(facts["gitRepository"], false);
    assert_eq!(facts["shellExecutable"], "/usr/bin/bash");
    assert_eq!(facts["temporaryDirectory"], "/approved/tmp");
    assert_eq!(facts["processArchitecture"], std::env::consts::ARCH);
    assert_eq!(facts["processPointerBits"], usize::BITS);
    assert!(facts["availableParallelism"].as_u64().unwrap() > 0);
    assert!(!render(&facts).contains("never-project-me"));
    // Minimal structural Git fixture including linked-worktree common metadata.
    let common = temp.path().join("common");
    std::fs::create_dir_all(common.join("objects")).unwrap();
    std::fs::create_dir_all(common.join("refs")).unwrap();
    let admin = temp.path().join("admin");
    std::fs::create_dir(&admin).unwrap();
    std::fs::write(admin.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(admin.join("commondir"), "../common\n").unwrap();
    std::fs::write(project.join(".git"), "gitdir: ../admin\n").unwrap();
    assert_eq!(git_repository(&nested), Some(true));
    std::fs::write(admin.join("HEAD"), "invalid\n").unwrap();
    assert_eq!(git_repository(&nested), Some(false));
    // Reaching the ancestor bound is unknown, never a fabricated negative.
    let mut deep = nested.clone();
    for _ in 0..257 {
        deep.push("d");
    }
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(git_repository(&deep), None);
    let oversized = collect(
        &project,
        &project,
        &BTreeMap::from([("TMPDIR".into(), format!("/{}", "界".repeat(FIELD_BYTES)))]),
    );
    assert!(!oversized.contains_key("temporaryDirectory"));
    assert!(render(&oversized).len() < 32768);
}

#[test]
fn prm01_shell_metadata_matches_selector_with_oversized_unused_path() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = BTreeMap::from([
        ("SHELL".into(), "/bin/bash".into()),
        (
            "PATH".into(),
            format!("/bin:/{}", "x".repeat(crate::shell::ARG_BYTES_CAP)),
        ),
    ]);
    let actual = crate::shell::command_argv(&env, ":").unwrap();
    assert_eq!(actual[0], "/bin/bash");
    let facts = collect(temp.path(), temp.path(), &env);
    assert_eq!(
        facts.get("shellExecutable"),
        Some(&actual[0].clone().into())
    );

    // An ignored fish SHELL makes selection depend on the oversized PATH. The
    // bounded collector must not substitute its default PATH and invent a shell.
    env.insert("SHELL".into(), "/not-selected/fish".into());
    assert_eq!(
        crate::shell::command_argv(&env, ":").unwrap()[0],
        "/bin/bash"
    );
    assert!(!collect(temp.path(), temp.path(), &env).contains_key("shellExecutable"));

    // Unused ambient keys do not change selection or consume the selector budget.
    env.insert("PATH".into(), "/usr/bin:/bin".into());
    env.insert("HOME".into(), "x".repeat(crate::shell::ARG_BYTES_CAP + 1));
    let actual = crate::shell::command_argv(&env, ":").unwrap();
    let facts = collect(temp.path(), temp.path(), &env);
    assert_eq!(
        facts.get("shellExecutable"),
        Some(&actual[0].clone().into())
    );
}

#[test]
fn prm01_base_fallback_is_shared_and_custom_replaces_only_base() {
    for prompt in [None, Some(""), Some(" \n")] {
        let fixed = super::super::turn::lane_fixed_input(prompt, "PROJECT_RULE", Some("[]"));
        let json = serde_json::to_string(&fixed).unwrap();
        assert!(json.contains(BASE));
        assert!(json.contains("PROJECT_RULE") && json.contains("Available native skills"));
    }
    let fixed = super::super::turn::lane_fixed_input(Some("CUSTOM_SYSTEM"), "PROJECT_RULE", None);
    let json = serde_json::to_string(&fixed).unwrap();
    assert!(!json.contains(BASE));
    assert!(json.contains("CUSTOM_SYSTEM") && json.contains("PROJECT_RULE"));
}
