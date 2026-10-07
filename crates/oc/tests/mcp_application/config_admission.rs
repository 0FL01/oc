//! MCP09 actual-binary config admission and launch effects, offline only.
use super::*;

#[test]
fn mcp09_disabled_chrome_environment_timeout_opens_first_tui_zero_spawn() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let fixture = Fixture::new();
    let effects = fixture.project.join("browser-effects");
    let bin = fixture.project.join("bin");
    fs::create_dir(&bin).unwrap();
    for name in ["npx", "chrome", "chromium"] {
        let path = bin.join(name);
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf 'spawn\\n' >> {:?}\nexit 99\n", effects),
        )
        .unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fixture.write_config(&responses, json!({"chrome-devtools": {
        "type":"local", "command":[bin.join("npx"),"-y","chrome-devtools-mcp@latest"],
        "enabled":false,"environment":{"npm_config_offline":"true","CREDENTIAL":"{file:missing}"},
        "cwd":"missing-disabled-directory", "timeout":5000
    }}), json!({}));
    let path = fixture.home.join("config/opencode/opencode.json");
    let before = fs::read(&path).unwrap();
    let mut tui = PtyProcess::spawn(&fixture, "mcp09-disabled-chrome");
    tui.wait_visible(READY);
    assert!(
        responses.requests().is_empty(),
        "no generation before a prompt"
    );
    assert!(!effects.exists(), "disabled server had process effects");
    tui.send_line("/quit");
    assert!(tui.wait_exit().success());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!effects.exists(), "disabled shutdown had process effects");
}

#[test]
fn mcp09_withheld_credential_path_cannot_reenter_executable_resolution() {
    let responses = FakeResponses::start(ResponsesScript::TextByPrompt);
    let fixture = Fixture::new();
    let bin = fixture.project.join("credential-path-canary");
    fs::create_dir(&bin).unwrap();
    let counter = fixture.project.join("must-stay-zero");
    let trap = bin.join("mcp09-denied-executable");
    fs::write(
        &trap,
        format!("#!/bin/sh\nprintf 'spawn\\n' >> {:?}\nexit 99\n", counter),
    )
    .unwrap();
    fs::set_permissions(&trap, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.write_config(&responses, json!({}), json!({}));
    let global = fixture.home.join("config/opencode/opencode.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&global).unwrap()).unwrap();
    config["provider"]["fixture"]["options"]["apiKey"] = json!(bin);
    fs::write(&global, config.to_string()).unwrap();
    let project_config = fixture.project.join("opencode.json");
    fs::write(
        &project_config,
        json!({"mcp":{"trap":{"type":"local","command":["mcp09-denied-executable"]}}}).to_string(),
    )
    .unwrap();
    let before = [
        (global.clone(), fs::read(&global).unwrap()),
        (project_config.clone(), fs::read(&project_config).unwrap()),
    ];
    let stdout = fixture.home.join("path.stdout");
    let stderr = fixture.home.join("path.stderr");
    let child = fixture
        .command()
        .env("PATH", &bin)
        .args([
            "run",
            "--session",
            "mcp09-withheld-path",
            "safe despite rejected executable",
        ])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&stdout).unwrap())
        .stderr(fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let mut process = Process {
        child,
        stdout,
        stderr,
    };
    assert!(process.wait().success());
    assert!(process.diagnostics().contains(&format!(
        "mcp {} spawn: spawn_failed",
        diagnostic_name("trap")
    )));
    assert!(
        !counter.exists(),
        "withheld PATH was re-read for executable resolution"
    );
    assert!(!process.diagnostics().contains("credential-path-canary"));
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn mcp09_actual_argv_cwd_inherited_overlay_path_and_credential_domain() {
    for (label, cwd, global) in [
        ("default", None, true),
        ("relative", Some("nested"), true),
        ("absolute", Some("absolute"), false),
    ] {
        let responses = FakeResponses::start(ResponsesScript::ToolNamed("probe__probe".into()));
        let fixture = Fixture::new();
        let nested = fixture.project.join("nested");
        fs::create_dir(&nested).unwrap();
        let bin = fixture.project.join("bin");
        fs::create_dir(&bin).unwrap();
        let server = bin.join("mcp09-server");
        fs::copy(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/mcp09-stdio.py"),
            &server,
        )
        .unwrap();
        fs::set_permissions(&server, fs::Permissions::from_mode(0o700)).unwrap();
        let report = fixture.project.join("effects.json");
        let expected_cwd = if cwd.is_some() {
            &nested
        } else {
            &fixture.project
        };
        let mut entry = json!({
            "type":"local", "command":["mcp09-server",report,expected_cwd,if global {"global"} else {"project"},"steady","two words",""],
            "disabled":false,"codemode":false,
            "environment":{"PATH":bin,"MCP09_OVERLAY":"{env:OVERLAY_SOURCE}"},
            "timeout":{"startup":3000,"catalog":1000,"execution":1000}
        });
        if let Some(cwd) = cwd {
            entry["cwd"] = if cwd == "absolute" {
                json!(nested)
            } else {
                json!(cwd)
            };
        }
        fixture.write_config(
            &responses,
            if global {
                json!({"servers":{"probe":entry.clone()}})
            } else {
                json!({})
            },
            json!({"probe__probe":"allow"}),
        );
        if !global {
            fs::write(
                fixture.project.join("opencode.jsonc"),
                json!({"mcp":{"servers":{"probe":entry}}}).to_string(),
            )
            .unwrap();
        }
        let configs: Vec<_> = [
            fixture.home.join("config/opencode/opencode.json"),
            fixture.project.join("opencode.jsonc"),
        ]
        .into_iter()
        .filter(|p| p.exists())
        .map(|path| {
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
        let stdout = fixture.home.join("effects.stdout");
        let stderr = fixture.home.join("effects.stderr");
        let mut command = fixture.command();
        command
            .env("MCP09_INHERITED", "mcp09-inherited-canary")
            .env("MCP09_OVERLAY", "mcp09-old-overlay-canary")
            .env("OVERLAY_SOURCE", "mcp09-configured-canary")
            .env("GLOBAL_PROVIDER_TOKEN", "mcp09-domain-canary")
            .env("BENIGN_ALIAS", "mcp09-domain-canary")
            .env("PATH", "/not-a-working-path")
            .args(["run", "--session", label, "exercise configured MCP"])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&stdout).unwrap())
            .stderr(fs::File::create(&stderr).unwrap());
        let mut process = Process {
            child: command.spawn().unwrap(),
            stdout,
            stderr,
        };
        assert!(process.wait().success(), "actual admitted MCP failed");
        let counters: Value = serde_json::from_slice(&fs::read(&report).unwrap()).unwrap();
        for field in [
            "argv",
            "cwd",
            "inherited",
            "overlay",
            "path",
            "credentials",
            "alias",
        ] {
            assert_eq!(counters[field], true, "{label}: {field}");
        }
        for field in ["spawn", "initialize", "catalog", "call"] {
            assert_eq!(counters[field], 1, "{label}: {field}");
        }
        let pid = counters["pid"].as_i64().unwrap() as libc::pid_t;
        // SAFETY: signal zero only probes this fixture's recorded child.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "child survived shutdown");
        for (path, before) in configs {
            assert_eq!(fs::read(path).unwrap(), before);
        }
        let db = rusqlite::Connection::open(fixture.home.join("data/oc/oc.sqlite")).unwrap();
        let messages: Vec<String> = db.prepare("SELECT text FROM messages UNION ALL SELECT coalesce(output,'') FROM tool_operations UNION ALL SELECT payload FROM events").unwrap().query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
        let output = format!(
            "{} {} {} {}",
            fs::read_to_string(&process.stdout).unwrap(),
            fs::read_to_string(&process.stderr).unwrap(),
            serde_json::to_string(&responses.requests()).unwrap(),
            messages.join("\n")
        );
        for canary in [
            "mcp09-inherited-canary",
            "mcp09-configured-canary",
            "mcp09-domain-canary",
            "mcp09-old-overlay-canary",
        ] {
            assert!(
                !output.contains(canary),
                "{label}: canary escaped redaction"
            );
        }
        assert!(
            output.contains("probe complete"),
            "actual call/result was missing"
        );
    }
}

#[test]
fn mcp09_failed_inventory_before_prompt_and_healthy_sibling_actual_call() {
    let responses = FakeResponses::start(ResponsesScript::ToolBatch {
        calls: vec![(
            "mcp09-item".into(),
            "mcp09-call".into(),
            json!({"__wireName":"codex_web__search","arguments":{"query":"MCP09 offline"}}),
        )],
        final_text: "MCP roundtrip complete".into(),
    });
    let mcp = FakeMcp::start("mcp09", "Bearer mcp-key", &["search"]);
    let fixture = Fixture::new();
    let trap = fixture.project.join("must-not-spawn");
    let trap_counter = fixture.project.join("broken-effects");
    fs::write(
        &trap,
        format!(
            "#!/bin/sh\nprintf 'spawn\\n' >> {:?}\nexit 99\n",
            trap_counter
        ),
    )
    .unwrap();
    fs::set_permissions(&trap, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.write_config(&responses, json!({"timeout":{"startup":1500},"servers":{
        "codex_web":{"type":"remote","url":mcp.url,"headers":{"Authorization":"Bearer mcp-key"},"oauth":false},
        "malformed":{"type":"local","command":[trap],"environment":{"X":["malformed-value-CANARY"]},"disabled":true},
        "oauth":{"type":"remote","url":mcp.url,"oauth":{"client_secret":"oauth-value-CANARY"}},
        "codemode":{"type":"local","command":[trap],"codemode":true},
        "modern":{"type":"remote","url":mcp.url,"protocol":"auto"}
    }}), json!({"codex_web__search":"allow"}));
    let path = fixture.home.join("config/opencode/opencode.json");
    let before = fs::read(&path).unwrap();
    let mut tui = PtyProcess::spawn(&fixture, "mcp09-inventory");
    tui.wait_visible(READY);
    for (name, code) in [
        ("malformed", "invalid_config"),
        ("oauth", "unsupported_capability"),
        ("modern", "unsupported_protocol"),
    ] {
        inspect_mcp_code(&mut tui, name, code);
    }
    assert!(responses.requests().is_empty());
    assert!(!trap_counter.exists());
    let offset = tui.send_line("healthy with failed inventory");
    tui.wait_visible_after(offset, "MCP roundtrip complete");
    std::thread::sleep(Duration::from_millis(300));
    tui.send_line("/quit");
    assert!(tui.wait_exit().success());
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|r| r.rpc_method == "initialize")
            .count(),
        1
    );
    assert_eq!(
        mcp.records()
            .iter()
            .filter(|r| r.rpc_method == "tools/call")
            .count(),
        1
    );
    let bytes = tui.output.lock().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    for forbidden in ["malformed-value-CANARY", "oauth-value-CANARY", "mcp-key"] {
        assert!(!text.contains(forbidden));
    }
    assert!(!trap_counter.exists());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        responses
            .requests()
            .iter()
            .filter(|r| !title::is_title(r))
            .all(|r| r["tools"].as_array().unwrap().iter().all(|tool| ![
                "malformed__",
                "oauth__",
                "modern__",
                "codemode__"
            ]
            .iter()
            .any(|prefix| tool["name"].as_str().unwrap_or("").starts_with(prefix))))
    );
}
