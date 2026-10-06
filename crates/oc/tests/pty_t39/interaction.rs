//! Actual-binary editor, picker, transcript, mouse and clipboard interactions.

use super::*;

/// VAR01: the same effective order reaches the real picker and Ctrl+T,
/// without a request until explicit submission. Both session and Home paths.
#[test]
fn var01_full_canonical_cycle_picker_agreement_and_exact_wire() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["provider"]["fixture"]["options"]["setCacheKey"] = true.into();
    config["provider"]["fixture"]["options"]["nativeFallbackLimits"] =
        serde_json::json!({"context":32768,"output":128});
    config["provider"]["fixture"]["models"][MODEL]["variants"] = serde_json::json!({
        "max":{"reasoningEffort":"max"}, "xhigh":{"reasoningEffort":"xhigh"},
        "zeta":{"reasoningEffort":" deep "}, "fast":{"reasoningEffort":"low"},
        "high":{"reasoningEffort":"high"}, "alpha":{"reasoningEffort":"custom"},
        "medium":{"reasoningEffort":"medium"}, "default":{"reasoningEffort":"none"},
        "low":{}, "minimal":{"reasoningEffort":"minimal"},
        "disabled":{"reasoningEffort":"medium","disabled":true}, "none":{"reasoningEffort":"none"}
    });
    std::fs::write(&path, config.to_string()).unwrap();
    let mut pty = PtySession::spawn_sized(fixture.clone(), "var01-cycle", None, 100, 40);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/rename VAR01 retained root\r");
    wait_screen_row(&pty, "VAR01 retained root", DEADLINE);
    let named = [
        "none", "minimal", "fast", "low", "medium", "high", "xhigh", "max", "zeta", "alpha",
    ];
    for home in [false, true] {
        if home {
            pty.send(b"\x18n"); // park the session draft via the actual New chord
            wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE);
        }
        let draft = if home {
            "var01 home draft"
        } else {
            "var01 session draft"
        };
        pty.send(draft.as_bytes());
        wait_screen_row(&pty, draft, DEADLINE);
        for (index, name) in named.iter().enumerate() {
            pty.send(b"\x14"); // actual Ctrl+T
            wait_screen_row(&pty, &format!("T39 model fixture · {name}"), DEADLINE);
            wait_screen_row(&pty, draft, DEADLINE);
            // Independent keyboard indexing proves the picker's order agrees,
            // including aliases, custom order and reserved/disabled omissions.
            pty.send(b"\x10Switch model variant\r");
            wait_screen_row(&pty, "Select variant", DEADLINE);
            wait_screen_row(&pty, &format!("● {name}"), DEADLINE);
            pty.send(b"\x1b[H");
            for _ in 0..=index {
                pty.send(b"\x1b[B");
            }
            pty.send(b"\r");
            dismissed(&pty, "Select variant");
            wait_screen_row(&pty, &format!("T39 model fixture · {name}"), DEADLINE);
            wait_screen_row(&pty, draft, DEADLINE);
            assert!(
                fixture.requests.lock().unwrap().is_empty(),
                "cycle/picker submitted"
            );
        }
        pty.send(b"\x14");
        pty.send(b"\x10Switch model variant\r");
        wait_screen_row(&pty, "Select variant", DEADLINE);
        wait_screen_row(&pty, "● Default", DEADLINE);
        pty.send(b"\x1b");
        dismissed(&pty, "Select variant");
        wait_screen_row(&pty, draft, DEADLINE);
        assert!(fixture.requests.lock().unwrap().is_empty());
    }
    // Return to the existing root (its choice was Default), retaining the
    // independently persisted session and Home selection identities.
    pty.send(b"\x03");
    wait_screen_absent(&pty, "var01 home draft");
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"VAR01 retained root\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "var01 session draft", DEADLINE);
    pty.send(b"\x14\x14\x14"); // none → minimal → exact alias fast/low
    wait_screen_row(&pty, "T39 model fixture · fast", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "echo: var01 session draft", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["reasoning"]["effort"], "low");
    wait_idle(&pty);
    pty.send(b"\x14");
    wait_screen_row(&pty, "T39 model fixture · low", DEADLINE);
    submit(&mut pty, "rank does not synthesize effort");
    wait_screen_row(&pty, "echo: rank does not synthesize effort", DEADLINE);
    assert!(fixture.wait_requests(2)[1].get("reasoning").is_none());
    wait_idle(&pty);
    pty.send(b"/variants\r");
    choose_variant(&mut pty, "Default");
    submit(&mut pty, "default has no variant overlay");
    wait_screen_row(&pty, "echo: default has no variant overlay", DEADLINE);
    assert!(fixture.wait_requests(3)[2].get("reasoning").is_none());
    wait_idle(&pty);
    pty.send(b"\x14");
    wait_screen_row(&pty, "T39 model fixture · none", DEADLINE);
    submit(&mut pty, "named none has exact effort");
    wait_screen_row(&pty, "echo: named none has exact effort", DEADLINE);
    assert_eq!(fixture.wait_requests(4)[3]["reasoning"]["effort"], "none");
    wait_idle(&pty);
    pty.send(b"/variants\r");
    choose_variant(&mut pty, "zeta");
    submit(&mut pty, "custom effort is not repaired");
    wait_screen_row(&pty, "echo: custom effort is not repaired", DEADLINE);
    assert_eq!(fixture.wait_requests(5)[4]["reasoning"]["effort"], " deep ");
    wait_idle(&pty);
    pty.send(b"/variants\r");
    choose_variant(&mut pty, "none");
    submit(&mut pty, "vis28 held stream");
    fixture.wait_requests(6);
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    pty.send(b"busy preserved draft\x14");
    wait_screen_row(&pty, "busy preserved draft", DEADLINE);
    wait_screen_row(&pty, "T39 model fixture · minimal", DEADLINE);
    assert_eq!(
        fixture.wait_requests(6).len(),
        6,
        "busy variant draft dispatches nothing"
    );
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&pty, "answer:vis28 completed", DEADLINE);
    wait_idle(&pty);
    pty.send(b"\x03");
    wait_screen_absent(&pty, "busy preserved draft");
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let requests = fixture.wait_requests(6);
    assert_eq!(requests.len(), 6, "busy shortcut must not submit");
    for request in &requests {
        assert_eq!(request["model"], MODEL);
        assert_eq!(
            request["max_output_tokens"], 128,
            "configured base budget survives every overlay"
        );
        assert!(
            request["prompt_cache_key"].is_string(),
            "configured base cache option survives Default"
        );
    }
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, "var01-cycle", DEFAULT_AGENT),
        serde_json::json!({"provider":"fixture","id":MODEL,"variant":"none"})
    );
}

#[test]
fn var01_empty_disabled_noop_and_readonly_shortcut_preserve_draft_and_selection() {
    for all_disabled in [false, true] {
        let fixture = Fixture::new();
        let path = fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json");
        if all_disabled {
            let mut config: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            config["provider"]["fixture"]["models"][MODEL]["variants"] = serde_json::json!({
                "max":{"disabled":true}, "low":{"disabled":true}, "default":{}
            });
            std::fs::write(&path, config.to_string()).unwrap();
        }
        let mut pty = PtySession::spawn(fixture.clone(), "var01-guards", None);
        pty.wait_visible(READY, DEADLINE);
        choose_model(&mut pty, "T39 model"); // identical local choice remains a no-op
        pty.send(b"noop draft\x14\x10"); // palette supplies a processed-key barrier
        wait_screen_row(&pty, "Commands", DEADLINE);
        pty.send(b"\x1b");
        dismissed(&pty, "Commands");
        wait_screen_row(&pty, "noop draft", DEADLINE);
        wait_screen_row(&pty, "T39 model fixture", DEADLINE);
        pty.send(b"\x03");
        wait_screen_absent(&pty, "noop draft");
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            saved_selection_record(&db, &fixture, "var01-guards", "fixture"),
            None
        );
        assert!(db.read_history("var01-guards").unwrap().is_empty());
        db.create_child_session(
            "var01-guards",
            "var01-readonly",
            None,
            None,
            Some("Read-only child"),
        )
        .unwrap();
        db.set_pref(
            &format!(
                "{}var01-readonly",
                oc_adapters::runtime::SESSION_LOCATION_PREFIX
            ),
            &fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
        )
        .unwrap();
        db.append_message("var01-readonly", "assistant", "VAR01 readonly sentinel")
            .unwrap();
        drop(db);
        // Give the child a genuinely cycleable catalog: a missing guard would
        // select low, so no-op fixtures cannot hide a read-only regression.
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["provider"]["fixture"]["models"][MODEL]["variants"] =
            serde_json::json!({"low":{"reasoningEffort":"low"}});
        std::fs::write(&path, config.to_string()).unwrap();
        let mut child = PtySession::spawn(fixture.clone(), "var01-readonly", None);
        wait_screen_row(&child, "VAR01 readonly", DEADLINE); // startup notice overlays the suffix
        child.send(b"readonly draft\x14\x10");
        wait_screen_row(&child, "Commands", DEADLINE);
        child.send(b"\x1b");
        dismissed(&child, "Commands");
        wait_screen_row(&child, "read-only history", DEADLINE);
        wait_screen_row(&child, "readonly draft", DEADLINE);
        assert!(
            !render_screen(&child.snapshot())
                .rows()
                .iter()
                .any(|r| r.contains("fixture · low"))
        );
        child.send(b"\x03");
        wait_screen_absent(&child, "readonly draft");
        child.send(b"\x03");
        assert!(child.wait_exit(DEADLINE).0.success() && child.restored());
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "no-op/read-only shortcut submitted"
        );
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            db.read_history("var01-readonly").unwrap(),
            [("assistant".into(), "VAR01 readonly sentinel".into())]
        );
        assert_eq!(
            saved_selection_record(&db, &fixture, "var01-guards", "fixture"),
            None
        );
    }
}

/// VAR01: real discovery/local merge, refresh, reopen and OS-process restart
/// keep the exact alias. Removed/disabled Home choices remain actionable and
/// refuse submission until the explicit stale Ctrl+T → Default recovery.
#[test]
fn var01_discovery_refresh_reopen_restart_and_retired_identity() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let provider = config["provider"]
        .as_object_mut()
        .unwrap()
        .remove("fixture")
        .unwrap();
    config["provider"]["ludka2"] = provider;
    config["model"] = format!("ludka2/{MODEL}").into();
    config["agent"].as_object_mut().unwrap().remove("t39agent");
    config["provider"]["ludka2"]["models"] = serde_json::json!({MODEL:{"name":"Canonical probe 0", "variants":{
        "fast":{"reasoningEffort":"low"}, "low":{"reasoningEffort":"custom-low"}, "minimal":{}
    }}});
    std::fs::write(&path, config.to_string()).unwrap();
    let publish = |variants: serde_json::Value| {
        *fixture.models.lock().unwrap() = serde_json::json!({"object":"list", "data":[{
            "id":MODEL,"opencode":{"variants":variants}
        }]});
    };
    publish(serde_json::json!({
        "max":{"reasoningEffort":"max"}, "xhigh":{"reasoningEffort":"xhigh"},
        "zeta":{"reasoningEffort":"deep"}, "fast":{"reasoningEffort":"high"},
        "alpha":{"reasoningEffort":"deep"}, "low":{"reasoningEffort":"low"}
    }));
    let mut pty = PtySession::spawn_sized(fixture.clone(), "var01-refresh", None, 100, 40);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/rename VAR01 identity root\r");
    wait_screen_row(&pty, "VAR01 identity root", DEADLINE);
    pty.send(b"identity draft\x14\x14"); // minimal then local-overridden fast/low
    wait_screen_row(&pty, "Canonical probe 0 ludka2 · fast", DEADLINE);
    let mut reordered = serde_json::json!({
        "alpha":{"reasoningEffort":"deep"}, "quick":{"reasoningEffort":"low"},
        "fast":{"reasoningEffort":"high"}, "max":{"reasoningEffort":"max"},
        "xhigh":{"reasoningEffort":"xhigh"}, "zeta":{"reasoningEffort":"deep"},
        "none":{"reasoningEffort":"none"}, "low":{"reasoningEffort":"low"}
    });
    publish(reordered.clone());
    config["provider"]["ludka2"]["models"][MODEL]["name"] = "Canonical probe 1".into();
    std::fs::write(&path, config.to_string()).unwrap();
    pty.send(b"\x10Reload configuration\r");
    wait_screen_row(&pty, "Canonical probe 1 ludka2 · fast", DEADLINE);
    wait_screen_row(&pty, "identity draft", DEADLINE);
    pty.send(b"\x10Switch model variant\r");
    wait_screen_row(&pty, "● fast", DEADLINE);
    // Fast moved from index 2 to index 4: reopening focuses identity, not index.
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "Canonical probe 1 ludka2 · fast", DEADLINE);
    pty.send(b"\x03");
    wait_screen_absent(&pty, "identity draft");
    pty.send(b"\r"); // explicit empty-composer commit before restart
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let before = {
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        saved_provider_selection(&db, &fixture, "var01-refresh", DEFAULT_AGENT, "ludka2")
    };
    assert_eq!(
        before,
        serde_json::json!({"provider":"ludka2","id":MODEL,"variant":"fast"})
    );
    let mut pty = PtySession::spawn_sized(fixture.clone(), "var01-refresh", None, 100, 40);
    wait_screen_row(&pty, "Canonical probe 1 ludka2 · fast", DEADLINE);
    pty.send(b"/new\r");
    wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE);
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"VAR01 identity root\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "Canonical probe 1 ludka2 · fast", DEADLINE);
    pty.send(b"/variants\r");
    choose_variant(&mut pty, "Default");
    // Keep the saved session valid while explicitly retiring the independent
    // Home draft through successful refresh; retained-tab reload guards remain.
    pty.send(b"\r"); // commit the existing session; Home keeps its own local draft
    pty.send(b"/new\r");
    wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE);
    for (phase, disabled) in [(2, false), (3, true)] {
        pty.send(b"/variants\r");
        choose_variant(&mut pty, "fast");
        config["provider"]["ludka2"]["models"][MODEL]["variants"]
            .as_object_mut()
            .unwrap()
            .remove("fast");
        reordered.as_object_mut().unwrap().remove("fast");
        if disabled {
            reordered["fast"] = serde_json::json!({"disabled":true});
        }
        publish(reordered.clone());
        config["provider"]["ludka2"]["models"][MODEL]["name"] =
            format!("Canonical probe {phase}").into();
        std::fs::write(&path, config.to_string()).unwrap();
        pty.send(b"retired draft\x10Reload configuration\r");
        wait_screen_row(&pty, "variant-", DEADLINE);
        // The opaque retired overlay takes footer width. Verify the refreshed
        // canonical model name in the actual picker, not a clipped footer.
        pty.send(b"\x10Switch model\r");
        wait_screen_row(&pty, "Select model", DEADLINE);
        wait_screen_row(&pty, &format!("Canonical probe {phase}"), DEADLINE);
        pty.send(b"\x1b");
        dismissed(&pty, "Select model");
        wait_idle(&pty);
        wait_screen_row(&pty, "retired draft", DEADLINE);
        pty.send(b"\x10Switch model variant\r");
        wait_screen_row(&pty, "Select variant", DEADLINE);
        wait_screen_row(&pty, "variant-", DEADLINE);
        assert!(
            !render_screen(&pty.snapshot())
                .rows()
                .iter()
                .any(|r| r.contains("● Default"))
        );
        pty.send(b"\x1b");
        dismissed(&pty, "Select variant");
        pty.send(b"\r");
        // The exact retired variant stays in the owner selection/preference;
        // the footer and refusal expose only its opaque identity and safe cause.
        wait_screen_row(&pty, "variant_unavailable (retryable=false)", DEADLINE);
        wait_screen_row(&pty, "config variant: select an", DEADLINE);
        wait_screen_row(&pty, "model variant or Default", DEADLINE);
        wait_screen_row(&pty, "retired draft", DEADLINE);
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "retired Home choice reached Responses"
        );
        pty.send(b"\x14"); // explicit existing stale → Default rule
        pty.send(b"\x10Switch model variant\r");
        wait_screen_row(&pty, "● Default", DEADLINE);
        pty.send(b"\x1b");
        dismissed(&pty, "Select variant");
        wait_screen_row(&pty, "retired draft", DEADLINE);
        pty.send(b"\x03");
        wait_screen_absent(&pty, "retired draft");
        if !disabled {
            reordered["fast"] = serde_json::json!({"reasoningEffort":"low"});
            publish(reordered.clone());
            config["provider"]["ludka2"]["models"][MODEL]["name"] = "Canonical restored".into();
            std::fs::write(&path, config.to_string()).unwrap();
            pty.send(b"/reload\r");
            wait_screen_row(&pty, "Canonical restored ludka2", DEADLINE);
            pty.send(b"/variants\r");
            wait_screen_row(&pty, "fast", DEADLINE);
            pty.send(b"\x1b");
            dismissed(&pty, "Select variant");
        }
    }
    submit(&mut pty, "accepted after explicit stale recovery");
    wait_screen_row(
        &pty,
        "echo: accepted after explicit stale recovery",
        DEADLINE,
    );
    let request = fixture.wait_requests(1);
    assert_eq!(request[0]["model"], MODEL);
    assert!(request[0].get("reasoning").is_none());
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_provider_selection(&db, &fixture, "var01-refresh", DEFAULT_AGENT, "ludka2"),
        serde_json::json!({"provider":"ludka2","id":MODEL,"variant":null})
    );
    assert!(
        db.read_history("var01-refresh").unwrap().is_empty(),
        "navigation/rejected submit did not accept history"
    );
}

#[test]
fn vis27_real_pty_osc52_select_and_manual_clipboard_modes() {
    const PROMPT: &str = "amber cobalt zircon";
    // The source line selection trims leading transcript padding.
    const LINE: &[u8] = b"echo: amber cobalt zircon";
    let fixture = Fixture::new();
    let session = "vis27-select";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    assert!(
        contains(&pty.snapshot(), b"\x1b[?1006h"),
        "SGR mouse capture"
    );
    submit(&mut pty, PROMPT);
    fixture.wait_requests(1);
    wait_screen_row(&pty, "echo: amber cobalt zircon", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    wait_idle(&pty);
    let requests_before = fixture.requests.lock().unwrap().clone();
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| title::is_title(r))
            .count(),
        1
    );
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    let (x, y) = vis27_answer_cell(&pty);
    assert!(
        osc52_clipboard(&pty.snapshot()).is_empty(),
        "no clipboard write before selection"
    );

    // SGR reports one-based coordinates. First down/up is a caret, never a copy.
    vis27_mouse(&mut pty, 0, x, y, 'M');
    vis27_mouse(&mut pty, 0, x, y, 'm');
    std::thread::sleep(Duration::from_millis(75)); // dispatch before the second click, within 500ms
    assert!(
        osc52_clipboard(&pty.snapshot()).is_empty(),
        "first click copied"
    );
    assert!(
        !contains(&pty.snapshot(), b"Copied to clipboard"),
        "first click claimed success"
    );
    // The two subsequent no-motion releases exercise the multi-click clock.
    let before_word_copy = pty.snapshot().len();
    mouse_click(&mut pty, x, y);
    wait_clipboard(&pty, 1, b"cobalt");
    pty.wait_visible_after(before_word_copy, "Copied to clipboard", DEADLINE);
    assert!(
        vis27_highlighted_word(&pty.snapshot()[before_word_copy..], b"cobalt"),
        "word highlight missing after copy"
    );
    mouse_click(&mut pty, x, y);
    wait_clipboard(&pty, 2, LINE);
    assert_eq!(
        vis27_answer_cell(&pty),
        (x, y),
        "selection changed the painted transcript"
    );

    // A fresh drag starts elsewhere and selects just the interior of the line.
    let start_x = x - 6; // 'amber' begins six cells before 'cobalt'
    vis27_mouse(&mut pty, 0, start_x, y, 'M');
    vis27_mouse(&mut pty, 32, x + 6, y, 'M');
    vis27_mouse(&mut pty, 0, x + 6, y, 'm');
    wait_clipboard(&pty, 3, b"amber cobalt");
    assert!(
        *fixture.requests.lock().unwrap() == requests_before,
        "selection sent provider traffic"
    );
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(
        status.success() && pty.restored(),
        "select-mode terminal recovery"
    );
    assert!(contains(&output, ALT_LEAVE) && contains(&output, b"\x1b[?1006l"));
    assert_eq!(osc52_clipboard(&output).len(), 3);
    assert_eq!(
        persisted(pty.data_dir(), session),
        [
            ("user".into(), PROMPT.into()),
            ("assistant".into(), "echo: amber cobalt zircon".into())
        ],
        "selection changed the two-message history"
    );

    // Write an admitted project config before a *new* binary launch. The
    // fixture global config and data root stay isolated throughout.
    std::fs::write(
        fixture.root.path().join("project/opencode.json"),
        r#"{"terminal":{"copy":"manual"}}"#,
    )
    .expect("manual project config");
    let session = "vis27-manual";
    let mut manual = PtySession::spawn(fixture.clone(), session, None);
    manual.wait_visible(READY, DEADLINE);
    submit(&mut manual, PROMPT);
    // wait_requests counts main turns, not ancillary title requests.
    fixture.wait_requests(2);
    wait_screen_row(&manual, "echo: amber cobalt zircon", DEADLINE);
    let started = Instant::now();
    loop {
        if fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| title::is_title(request))
            .count()
            == 2
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "second root title request missing"
        );
        std::thread::sleep(POLL);
    }
    wait_screen_row(&manual, "Fixture session title", DEADLINE);
    wait_idle(&manual);
    let requests_before = fixture.requests.lock().unwrap().clone();
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| title::is_title(r))
            .count(),
        2
    );
    assert_eq!(
        requests_before
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        2
    );
    let (x, y) = vis27_answer_cell(&manual);
    vis27_mouse(&mut manual, 0, x, y, 'M');
    vis27_mouse(&mut manual, 32, x + 6, y, 'M');
    vis27_mouse(&mut manual, 0, x + 6, y, 'm');
    // Synchronize with the completed drag via its painted highlight redraw;
    // no auto-copy and no success toast may precede a manual right press.
    vis27_wait_highlight(&manual, b"cobalt");
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        osc52_clipboard(&manual.snapshot()).is_empty(),
        "manual drag copied automatically"
    );
    assert!(
        !contains(&manual.snapshot(), b"Copied to clipboard"),
        "manual drag claimed success"
    );
    let before_manual_copy = manual.snapshot().len();
    vis27_mouse(&mut manual, 2, x, y, 'M');
    wait_clipboard(&manual, 1, b"cobalt");
    manual.wait_visible_after(before_manual_copy, "Copied to clipboard", DEADLINE);
    // Right-down leaves the selected cells untouched in the ratatui diff;
    // their prior selection-style paint remains in the PTY's screen buffer.
    assert!(
        vis27_highlighted_word(&manual.snapshot(), b"cobalt"),
        "manual highlight lost after copy"
    );
    assert_eq!(vis27_answer_cell(&manual), (x, y));
    assert!(
        *fixture.requests.lock().unwrap() == requests_before,
        "manual selection sent provider traffic"
    );
    manual.send(b"/quit\r");
    let (status, output) = manual.wait_exit(DEADLINE);
    assert!(
        status.success() && manual.restored(),
        "manual-mode terminal recovery"
    );
    assert!(contains(&output, ALT_LEAVE) && contains(&output, b"\x1b[?1006l"));
    assert_eq!(osc52_clipboard(&output).len(), 1);
    assert_eq!(
        persisted(manual.data_dir(), session),
        [
            ("user".into(), PROMPT.into()),
            ("assistant".into(), "echo: amber cobalt zircon".into())
        ],
        "manual selection changed the two-message history"
    );
}

#[test]
fn v06_real_pty_completed_reasoning_header_click_expands_and_collapses() {
    let fixture = Fixture::new();
    let session = "v06-reasoning-click";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, REASONING_CLICK_PROMPT);
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some(REASONING_CLICK_PROMPT)
    );
    wait_screen_row(&pty, REASONING_CLICK_ANSWER, DEADLINE);
    wait_screen_row(&pty, "+ Thought: Click plan", DEADLINE);
    wait_idle(&pty);
    let before = render_screen(&pty.snapshot()).rows();
    assert!(
        !before.iter().any(|row| row.contains(REASONING_CLICK_BODY)),
        "collapsed reasoning body must not be painted: {before:?}"
    );
    // Wait for the ancillary title before recording the complete provider
    // request baseline, so header clicks must be entirely local.
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    let requests_before = fixture.requests.lock().unwrap().len();

    let (x, y) = reasoning_click_header(&pty, "+ Thought: Click plan");
    mouse_click(&mut pty, x, y); // real SGR left press + release
    wait_screen_row(&pty, REASONING_CLICK_BODY, DEADLINE);
    wait_screen_row(&pty, "- Thought", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains(REASONING_CLICK_ANSWER)),
        "expanding reasoning retains the answer"
    );

    let (x, y) = reasoning_click_header(&pty, "- Thought");
    // A rapid second click on the same glyph is a word-selection gesture in
    // select-copy mode, not another single-click header activation. Let the
    // multi-click window close before exercising a second ordinary click.
    std::thread::sleep(Duration::from_millis(510));
    mouse_click(&mut pty, x, y);
    let started = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows.iter().any(|row| row.contains("+ Thought: Click plan"))
            && !rows.iter().any(|row| row.contains(REASONING_CLICK_BODY))
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "reasoning did not collapse: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
    // The same completed header also responds to a standalone native SGR UP.
    // Resolve each target from the current painted grid after the prior toggle:
    // expanded/collapsed layouts need not share a fixed terminal coordinate.
    let (x, y) = reasoning_click_header(&pty, "+ Thought: Click plan");
    mouse_up_only(&mut pty, x, y);
    wait_screen_row(&pty, REASONING_CLICK_BODY, DEADLINE);
    wait_screen_row(&pty, "- Thought", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains(REASONING_CLICK_ANSWER)),
        "UP-only expansion retains the answer"
    );
    let (x, y) = reasoning_click_header(&pty, "- Thought");
    mouse_up_only(&mut pty, x, y);
    let started = Instant::now();
    loop {
        let rows = render_screen(&pty.snapshot()).rows();
        if rows.iter().any(|row| row.contains("+ Thought: Click plan"))
            && !rows.iter().any(|row| row.contains(REASONING_CLICK_BODY))
        {
            break;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "UP-only reasoning did not collapse: {rows:?}"
        );
        std::thread::sleep(POLL);
    }
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    assert_eq!(
        persisted(pty.data_dir(), session),
        vec![
            ("user".to_string(), REASONING_CLICK_PROMPT.to_string()),
            ("assistant".to_string(), REASONING_CLICK_ANSWER.to_string())
        ],
        "reasoning toggles leave the original durable answer unchanged"
    );
}

#[test]
fn vis15_real_pty_adjacent_reasoning_parts_group_and_replay_after_restart() {
    let fixture = Fixture::new();
    let session = "vis15-adjacent-reasoning";
    let mut pty = PtySession::spawn_sized(fixture.clone(), session, None, 120, 40);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, REASONING_STEPS_PROMPT);
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some(REASONING_STEPS_PROMPT)
    );
    wait_screen_row(&pty, REASONING_STEPS_ANSWER, DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    wait_idle(&pty);
    assert_reasoning_steps_collapsed(&pty);
    let requests_before = fixture.requests.lock().unwrap().len();
    toggle_reasoning_steps(&mut pty, true);
    // Select-copy mode treats a quick second click as a multi-click selection.
    std::thread::sleep(Duration::from_millis(510));
    toggle_reasoning_steps(&mut pty, false);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);

    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert!(!contains(&output, REASONING_STEPS_OPAQUE.as_bytes()));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);

    // Assert the committed journal, not just the answer projection or screen.
    let db = oc_adapters::storage::Db::open(pty.data_dir()).expect("durable db");
    assert_eq!(
        db.read_history(session).unwrap(),
        [
            ("user".into(), REASONING_STEPS_PROMPT.into()),
            ("assistant".into(), REASONING_STEPS_ANSWER.into())
        ]
    );
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let turns: i64 = conn
        .query_row(
            "SELECT count(*) FROM turns WHERE session_id=?1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(turns, 1, "one accepted turn across both launches");
    let (turn_status, journal): (String, String) = conn
        .query_row(
            "SELECT status, result FROM turns WHERE session_id=?1",
            [session],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("exactly one durable turn");
    assert_eq!(turn_status, "completed");
    let journal: serde_json::Value = serde_json::from_str(&journal).unwrap();
    let parts = journal["display_parts"].as_array().expect("public parts");
    assert_eq!(parts.len(), 3, "two distinct reasoning parts and answer");
    assert_eq!(parts[0]["reasoning"], REASONING_STEPS_FIRST);
    assert_eq!(parts[1]["reasoning"], REASONING_STEPS_SECOND);
    assert!(parts[2]["message"].is_number(), "answer is a message part");
    assert!(
        !parts
            .iter()
            .any(|part| part.to_string().contains(REASONING_STEPS_OPAQUE)),
        "encrypted continuation must not enter public parts"
    );
    let input = journal["input"].as_array().expect("durable wire items");
    let answer_index = parts[2]["message"].as_u64().unwrap() as usize;
    assert_eq!(
        input[answer_index]["content"][0]["text"],
        REASONING_STEPS_ANSWER
    );
    let encrypted: Vec<_> = input
        .iter()
        .filter(|item| item["encrypted_content"] == REASONING_STEPS_OPAQUE)
        .collect();
    assert_eq!(encrypted.len(), 2, "separate opaque reasoning items");
    assert_eq!(encrypted[0]["id"], "rs_pty_first");
    assert_eq!(encrypted[1]["id"], "rs_pty_second");
    drop(conn);
    drop(db);

    let mut reopened = PtySession::spawn_sized(fixture.clone(), session, None, 120, 40);
    wait_screen_row(&reopened, REASONING_STEPS_ANSWER, DEADLINE);
    assert_reasoning_steps_collapsed(&reopened);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    toggle_reasoning_steps(&mut reopened, true);
    std::thread::sleep(Duration::from_millis(510));
    toggle_reasoning_steps(&mut reopened, false);
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
    reopened.send(b"/quit\r");
    let (status, output) = reopened.wait_exit(DEADLINE);
    assert!(status.success() && reopened.restored() && contains(&output, ALT_LEAVE));
    assert!(!contains(&output, REASONING_STEPS_OPAQUE.as_bytes()));
    assert_eq!(fixture.requests.lock().unwrap().len(), requests_before);
}

/// V07 S03: terminal bytes, focused dialogs, editor and actual Responses wire.
#[test]
fn v07_raw_modifiers_release_and_modal_interrupt_keep_exact_draft() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v07-keys", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"v07-");
    wait_screen_row(&pty, "v07-", DEADLINE);

    pty.send(b"\x10"); // raw Ctrl+P: opens the palette, never inserts 'p'
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b[112;5:3u"); // CSI-u release of Ctrl+P
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "Esc only closes modal"
    );
    wait_screen_row(&pty, "v07-", DEADLINE);

    pty.send(b"\x1b[122;3u"); // CSI-u Alt+z is not plain 'z'
    pty.send(b"\x1bz"); // legacy ESC-prefix Alt+z is not plain 'z'
    pty.send(b"\x1b[120;1:3u"); // release of 'x' must not insert text
    pty.send(b"draft");
    wait_screen_row(&pty, "v07-draft", DEADLINE);
    pty.send(b"\x1b[13;2u"); // Shift+Enter inserts a newline, not a turn
    pty.send(b"\x1b[13;1:3u"); // release of Enter must not submit
    pty.send(b"line-two");
    wait_screen_row(&pty, "line-two", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());

    pty.send(b"\x10"); // dialog owns focus, editor draft remains unchanged
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"v07-no-match");
    wait_screen_row(&pty, "No results found", DEADLINE);
    pty.send(b"\x03"); // Ctrl+C clears the focused dialog search
    dismissed(&pty, "No results found");
    wait_screen_row(&pty, "Commands", DEADLINE);
    assert!(pty.child.try_wait().unwrap().is_none());
    pty.send(b"\x03"); // empty dialog search: Ctrl+C closes dialog
    dismissed(&pty, "Commands");
    assert!(pty.child.try_wait().unwrap().is_none());
    wait_screen_row(&pty, "v07-draft", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());

    pty.send(b"\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("v07-draft\nline-two")
    );
    wait_idle(&pty);
    pty.send(b"\x1b[13;1:3u"); // release after a completed turn stays inert
    pty.send(b"unsent"); // barrier: input after release has reached the editor
    wait_screen_row(&pty, "unsent", DEADLINE);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "release must not start another provider request"
    );
    pty.send(b"\x03"); // Ctrl+C clears the nonempty focused editor
    wait_screen_absent(&pty, "unsent");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "clear must not submit the discarded draft"
    );
    pty.send(b"\x03"); // empty root exits and restores the terminal
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    assert_eq!(
        persisted(pty.data_dir(), "v07-keys")
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["v07-draft\nline-two"]
    );
}

#[test]
fn v05_raw_unicode_multiline_focus_and_one_durable_submit() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-unicode", None);
    pty.wait_visible(READY, DEADLINE);
    // UTF-8 combining character, a multi-codepoint ZWJ grapheme and Cyrillic.
    pty.send("привет е\u{301}🧑‍💻 мир".as_bytes());
    wait_screen_row(&pty, "мир", DEADLINE);
    // Left over " мир", backspace removes the whole emoji, then reinsert it.
    let original_cursor = settled_cursor(&pty);
    pty.send(b"\x1b[D\x1b[D\x1b[D\x1b[D");
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 4));
    pty.send(b"\x7f");
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 6));
    pty.send("🧑‍💻".as_bytes());
    wait_cursor(&pty, (original_cursor.0, original_cursor.1 - 4));
    // Insert a genuine multiline break with raw Ctrl+J in the middle.
    pty.send(b"\x0a");
    pty.send("вторая".as_bytes());
    wait_screen_row(&pty, "вторая", DEADLINE);
    pty.send(b"\x1b[13;2u"); // Shift+Enter in terminals supporting CSI-u
    pty.send("третья".as_bytes());
    wait_screen_row(&pty, "третья", DEADLINE);
    let before_paste = settled_cursor(&pty);
    pty.send("\x1b[200~ из пасты\x1b[201~".as_bytes());
    wait_screen_row(&pty, "из пасты", DEADLINE);
    pty.send(b"\x1b[45;5u"); // CSI-u Ctrl+- undoes the whole paste
    wait_cursor(&pty, before_paste);
    let selected_from = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[1;2D"); // select the last grapheme
    wait_cursor(&pty, (selected_from.0, selected_from.1 - 1));
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, (selected_from.0, selected_from.1 - 1));
    pty.send(b"\x7f"); // delete retained selection, then restore it
    pty.send("я".as_bytes());
    wait_screen_row(&pty, "третья", DEADLINE);
    let second_line_cursor = settled_cursor(&pty);
    pty.send(b"\x1b[A\x1b[H"); // navigate the draft without scrolling history
    let start = Instant::now();
    while render_screen(&pty.snapshot()).cursor == second_line_cursor {
        assert!(
            start.elapsed() < DEADLINE,
            "editor did not move inside first line"
        );
        std::thread::sleep(POLL);
    }
    let before = settled_cursor(&pty);
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, before);
    pty.send(b"\x1b[13;1:3u"); // release of Enter must not submit
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r"); // a second Enter while pending/streaming cannot duplicate
    let requests = fixture.wait_requests(1);
    let prompt = last_user_text(&requests[0]).expect("provider prompt");
    assert!(
        prompt.contains("привет е\u{301}"),
        "combining grapheme: {prompt:?}"
    );
    assert!(prompt.contains("🧑‍💻"), "ZWJ grapheme: {prompt:?}");
    assert!(prompt.contains("вторая"), "multiline prompt: {prompt:?}");
    assert!(
        prompt.contains("вторая\nтретья"),
        "CSI-u Shift+Enter: {prompt:?}"
    );
    assert!(
        !prompt.contains("из пасты"),
        "paste undo must be atomic: {prompt:?}"
    );
    assert!(
        prompt.contains('\n'),
        "newline on provider wire: {prompt:?}"
    );
    wait_screen_row(&pty, "echo:", DEADLINE);
    wait_idle(&pty);
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    // Up at the boundary recalls the last durable prompt; Down restores the
    // unfinished draft. Neither navigation path submits a second turn.
    pty.send(b"unfinished");
    wait_screen_row(&pty, "unfinished", DEADLINE);
    pty.send(b"\x1b[H\x1b[A"); // first visual row start, then history boundary
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("unfinished"))
    {
        assert!(started.elapsed() < DEADLINE, "Up did not recall history");
        std::thread::sleep(POLL);
    }
    // Recalled history starts at offset zero. Traverse its three visual rows,
    // reach raw EOF, then a further Down restores the unfinished draft.
    pty.send(b"\x1b[B\x1b[B\x1b[B\x1b[B");
    wait_screen_row(&pty, "unfinished", DEADLINE);
    pty.send(b"\x03"); // first Ctrl+C clears the restored unfinished draft
    wait_screen_absent(&pty, "unfinished");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1,
        "clear must not submit the restored draft"
    );
    pty.send(b"\x03"); // empty root exits
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
    assert_eq!(
        fixture
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| !title::is_title(r))
            .count(),
        1
    );
    let rows = persisted(pty.data_dir(), "v05-unicode");
    assert_eq!(
        rows.iter().filter(|(role, _)| role == "user").count(),
        1,
        "{rows:?}"
    );
    assert_eq!(
        rows.iter().find(|(role, _)| role == "user").unwrap().1,
        prompt
    );
}

#[test]
fn v05_raw_capped_bracket_paste_is_visible_and_not_submitted() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-paste", None);
    pty.wait_visible(READY, DEADLINE);
    let size = oc_core::session::MAX_INPUT_BYTES;
    // One bracketed-paste event exceeding the application budget. No Enter.
    pty.send(b"\x1b[200~");
    pty.send(&vec![b'x'; size + 13]);
    pty.send(b"\x1b[201~");
    pty.wait_visible("paste truncated: 13 bytes dropped", DEADLINE);
    wait_screen_row(&pty, "[Pasted ~1 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03"); // clears the oversized draft, not the process
    wait_screen_absent(&pty, "[Pasted ~1 lines]");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03"); // empty root exits and restores terminal state
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&out, ALT_LEAVE));
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert!(persisted(pty.data_dir(), "v05-paste").is_empty());
}

#[test]
fn v05_raw_paste_chip_keeps_original_on_wire_and_in_history() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-paste-chip", None);
    pty.wait_visible(READY, DEADLINE);
    let pasted = "е\u{301}🧑‍💻\nвторая\nтретья";
    pty.send(b"prefix ");
    pty.send(format!("\x1b[200~{pasted}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("вторая"))
    );
    let end = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, end);
    pty.send(b"\x1b[1;2D"); // select the chip
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    wait_cursor(&pty, (end.0, end.1 - "[Pasted ~3 lines] ".len()));
    pty.send(b"\x7f"); // saved selection deletes the entire chip
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("[Pasted ~3 lines]"))
    {
        assert!(started.elapsed() < DEADLINE, "chip not deleted");
        std::thread::sleep(POLL);
    }
    pty.send(b"\x1b[45;5u"); // undo restores the original bytes AND the chip
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    pty.send(b"\x1b[D"); // move over the entire chip
    pty.send(b"X"); // insertion before chip shifts its mapped byte range
    wait_screen_row(&pty, "X[Pasted ~3 lines]", DEADLINE);
    let long = "z".repeat(151);
    pty.send(format!("\x1b[200~{long}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~1 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r");
    let requests = fixture.wait_requests(1);
    let expected = format!("prefix X{long}{pasted}");
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some(expected.as_str())
    );
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let rows = persisted(pty.data_dir(), "v05-paste-chip");
    assert_eq!(rows.iter().filter(|(role, _)| role == "user").count(), 1);
    assert_eq!(
        rows.iter().find(|(role, _)| role == "user").unwrap().1,
        expected
    );
}

#[test]
fn v05_review_raw_chip_trim_matches_visible_draft_and_wire() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-chip-trim", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x1b[200~a\nb\nc\n\x1b[201~");
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("paste chip trimmed")),
        "trimming hidden terminal paste whitespace must be observable"
    );
    pty.send(b"Z\r\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(last_user_text(&requests[0]).as_deref(), Some("a\nb\ncZ"));
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(pty.data_dir(), "v05-chip-trim")
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        vec!["a\nb\ncZ"]
    );
}

#[test]
fn v05_review_raw_history_edit_down_restores_chip_and_wire() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-history-chip", 1);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-history-chip", None);
    pty.wait_visible(READY, DEADLINE);
    let draft = "draft\nline\nchip";
    pty.send(format!("\x1b[200~{draft}\x1b[201~").as_bytes());
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    pty.send(b"\x1b[H\x1b[A"); // Up recalls only at the draft's start
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("[Pasted ~3 lines]"))
    {
        assert!(
            started.elapsed() < DEADLINE,
            "Up did not recall stored prompt"
        );
        std::thread::sleep(POLL);
    }
    pty.send(b" edited\x1b[F\x1b[B");
    wait_screen_row(&pty, "[Pasted ~3 lines]", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\r\r");
    let requests = fixture.wait_requests(1);
    assert_eq!(last_user_text(&requests[0]).as_deref(), Some(draft));
    wait_idle(&pty);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let persisted = persisted(pty.data_dir(), "v05-history-chip");
    assert_eq!(
        persisted.iter().filter(|(role, _)| role == "user").count(),
        2
    );
    assert!(
        persisted
            .iter()
            .any(|(role, text)| role == "user" && text == "seeded row 00000 payload")
    );
    assert!(
        !persisted
            .iter()
            .any(|(role, text)| role == "user" && text.contains("edited"))
    );
    assert_eq!(
        persisted
            .iter()
            .rfind(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str()),
        Some(draft)
    );
}

#[test]
fn v05_review_empty_editor_up_uses_history_on_wire() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-empty-up", 1);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-empty-up", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x1b[A\r"); // empty editor: Up recalls, Enter submits it
    let requests = fixture.wait_requests(1);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("seeded row 00000 payload")
    );
    wait_idle(&pty);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(
        persisted(pty.data_dir(), "v05-empty-up")
            .iter()
            .filter(|(role, _)| role == "user")
            .count(),
        2
    );
}

#[test]
fn v05_review_wheel_scroll_does_not_move_multiline_editor_caret() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "v05-wheel", 70);
    let mut pty = PtySession::spawn(fixture.clone(), "v05-wheel", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send("\x1b[200~draft 🧑‍💻\nстрока\x1b[201~".as_bytes());
    wait_screen_row(&pty, "строка", DEADLINE);
    let caret = render_screen(&pty.snapshot()).cursor;
    pty.send(b"\x1b[<64;1;1M"); // real xterm SGR wheel, not a keyboard Up
    wait_screen_row(&pty, "Jump to latest", DEADLINE);
    wait_cursor(&pty, caret);
    pty.send(b"\x1b[<65;1;1M");
    let started = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|row| row.contains("Jump to latest"))
    {
        assert!(
            started.elapsed() < DEADLINE,
            "wheel down did not repin transcript"
        );
        std::thread::sleep(POLL);
    }
    wait_cursor(&pty, caret);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03"); // clears multiline draft after the wheel movement
    wait_screen_absent(&pty, "строка");
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "clear must not exit"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert!(fixture.requests.lock().unwrap().is_empty());
    assert_eq!(persisted(pty.data_dir(), "v05-wheel").len(), 70);
}

#[test]
fn v05_review_shift_home_end_select_buffer_on_provider_wire() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v05-buffer-edges", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send("\x1b[200~один\nдва\x1b[201~".as_bytes());
    wait_screen_row(&pty, "два", DEADLINE);
    pty.send(b"\x1b[1;2H"); // Shift+Home selects from end to buffer start
    pty.send("новый".as_bytes());
    pty.send(b"\r");
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some("новый")
    );
    wait_idle(&pty);
    pty.send("\x1b[200~первый\nвторой\x1b[201~".as_bytes());
    wait_screen_row(&pty, "второй", DEADLINE);
    pty.send(b"\x1b[A\x1b[F"); // end of the first logical line
    pty.send(b"\x1b[1;2F"); // Shift+End selects through buffer end
    pty.send(b"X\r");
    let requests = fixture.wait_requests(2);
    assert_eq!(last_user_text(&requests[1]).as_deref(), Some("первыйX"));
    wait_idle(&pty);
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    assert_eq!(
        persisted(pty.data_dir(), "v05-buffer-edges")
            .iter()
            .filter(|(role, _)| role == "user")
            .count(),
        2
    );
}

#[test]
fn v04_raw_sgr_mouse_backdrop_search_variant_and_actual_model() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-mouse", None);
    pty.wait_visible(READY, DEADLINE);
    assert!(
        contains(&pty.snapshot(), b"\x1b[?1006h"),
        "SGR capture enabled"
    );
    pty.send(b"draft-mouse");
    wait_screen_row(&pty, "draft-mouse", DEADLINE);
    let editor_cursor = render_screen(&pty.snapshot()).cursor;

    // Open the model selector, then click the backdrop. The same release must
    // not hit the underlying editor, and Esc after closing must not be needed.
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_cursor(&pty, (9, 14));
    mouse_click(&mut pty, 1, 1);
    dismissed(&pty, "Select model");
    wait_cursor(&pty, editor_cursor);
    assert!(pty.child.try_wait().unwrap().is_none());

    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", DEADLINE);
    mouse_click(&mut pty, 1, 1);
    dismissed(&pty, "Commands");
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    // 80x24: modal x=10..70, top=6; Search y=9, first filtered option y=11.
    mouse_click(&mut pty, 20, 10);
    pty.send(b"T39 alt");
    wait_cursor(&pty, (9, 21));
    assert!(render_screen(&pty.snapshot()).rows()[9].contains("T39 alt"));
    mouse_click(&mut pty, 20, 12);
    wait_screen_row(&pty, "Select variant", DEADLINE);
    // Upstream replaces Model with Variant after the accepted model change.
    pty.send(b"\x1b");
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-mouse", DEADLINE);
    wait_cursor(&pty, editor_cursor);
    assert!(pty.child.try_wait().unwrap().is_none());
    pty.send(b"\x10Switch model variant\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    // No category heading: Default at y=11 and fast at y=12.
    mouse_click(&mut pty, 20, 13);
    dismissed(&pty, "Select variant");
    pty.send(b"\r");
    pty.wait_visible("echo: draft-mouse", DEADLINE);
    let request = fixture.wait_requests(1);
    assert_eq!(request[0]["model"], ALT_MODEL);
    assert_eq!(request[0]["reasoning"]["effort"], "high");
    assert_eq!(last_user_text(&request[0]).as_deref(), Some("draft-mouse"));
    wait_idle(&pty);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
    assert!(contains(&output, b"\x1b[?1006l"), "SGR capture restored");
    let db = oc_adapters::storage::Db::open(pty.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, "v04-mouse", DEFAULT_AGENT)["variant"],
        "fast"
    );
    assert_eq!(
        db.read_history("v04-mouse")
            .unwrap()
            .iter()
            .filter(|(role, text)| role == "user" && text == "draft-mouse")
            .count(),
        1
    );
}

#[test]
fn selected_model_is_the_main_wire_model_and_each_turn_keeps_its_footer() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "model-identity", None);
    pty.wait_visible(READY, DEADLINE);

    let off = submit(&mut pty, "first model identity");
    pty.wait_visible_after(off, "echo: first model identity", DEADLINE);
    wait_screen_row(&pty, "T39 model", DEADLINE);
    wait_idle(&pty);

    choose_model(&mut pty, "T39 alt");
    choose_variant(&mut pty, "Default");
    wait_screen_row(&pty, "T39 alt fixture", DEADLINE);
    let off = submit(&mut pty, "second model identity");
    pty.wait_visible_after(off, "echo: second model identity", DEADLINE);
    wait_idle(&pty);
    let rows = render_screen(&pty.snapshot()).rows();
    assert!(
        rows.iter().any(|row| row.contains("T39 model")),
        "old footer: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("T39 alt")),
        "new footer: {rows:?}"
    );

    let requests = fixture.wait_requests(2);
    assert_eq!(
        last_user_text(&requests[0]).as_deref(),
        Some("first model identity")
    );
    assert_eq!(requests[0]["model"], MODEL);
    assert_eq!(
        last_user_text(&requests[1]).as_deref(),
        Some("second model identity")
    );
    assert_eq!(requests[1]["model"], ALT_MODEL);
    let captured = fixture.requests.lock().unwrap();
    let titles: Vec<_> = captured
        .iter()
        .filter(|body| title::is_title(body))
        .collect();
    assert_eq!(
        titles.len(),
        1,
        "untitled session generates one ancillary title"
    );
    assert_eq!(titles[0]["model"], MODEL);
    drop(captured);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());

    let mut pty = PtySession::spawn(fixture.clone(), "model-identity", None);
    pty.wait_visible("echo: second model identity", DEADLINE);
    let rows = render_screen(&pty.snapshot()).rows();
    assert!(
        rows.iter().any(|row| row.contains("T39 model")),
        "old replay: {rows:?}"
    );
    assert!(
        rows.iter().any(|row| row.contains("T39 alt")),
        "new replay: {rows:?}"
    );
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[test]
fn v04_raw_mouse_wheel_and_hover_select_beyond_visible_rows() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for i in 0..18 {
        config["provider"]["fixture"]["models"][format!("mouse-{i:02}")] = serde_json::json!({
            "name": format!("Mouse {i:02}"), "limit": {"context":32768,"output":4096}
        });
    }
    std::fs::write(&path, config.to_string()).unwrap();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-wheel", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "Mouse 00", DEADLINE);
    // Mouse scroll preserves the selection; row 11 moves from mouse-00 to
    // mouse-12 after four 3-row wheels, without scrolling the underlay.
    for _ in 0..4 {
        pty.send(b"\x1b[<65;20;12M");
    }
    wait_screen_row(&pty, "Mouse 12", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Mouse 00"))
    );
    pty.send(b"\x1b[<35;20;12M"); // SGR pointer movement, first scrolled row
    pty.send(b"\r"); // focused row, no extra pointer release or double activation
    dismissed(&pty, "Select model");
    let off = submit(&mut pty, "wheel hover chosen");
    pty.wait_visible_after(off, "echo: wheel hover chosen", DEADLINE);
    assert_eq!(fixture.wait_requests(1)[0]["model"], "mouse-12");
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[test]
fn v04_retired_model_and_variant_remain_visible_until_explicit_remediation() {
    for retired_model in [true, false] {
        let fixture = Fixture::new();
        let path = fixture
            .root
            .path()
            .join("home/config/opencode/opencode.json");
        let mut config: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["none"] =
            serde_json::json!({"reasoningEffort":"low"});
        std::fs::write(&path, config.to_string()).unwrap();
        let mut pty = PtySession::spawn(fixture.clone(), "retired", None);
        pty.wait_visible(READY, DEADLINE);
        choose_model(&mut pty, "T39 alt");
        choose_variant(&mut pty, "fast");
        pty.send(b"\r"); // picker alone no longer persists the selected choice
        pty.send(b"/rename Retained selection root\r");
        wait_screen_row(&pty, "Retained selection root", DEADLINE);
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let before = {
            let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
            saved_selection(&db, &fixture, "retired", DEFAULT_AGENT)
        };
        assert_eq!(
            before,
            serde_json::json!({"provider":"fixture","id":ALT_MODEL,"variant":"fast"})
        );
        if retired_model {
            config["provider"]["fixture"]["models"]
                .as_object_mut()
                .unwrap()
                .remove(ALT_MODEL);
        } else {
            config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["fast"]["disabled"] =
                true.into();
        }
        std::fs::write(&path, config.to_string()).unwrap();
        let mut pty = PtySession::spawn(
            fixture.clone(),
            if retired_model { "retired" } else { "healthy" },
            None,
        );
        if !retired_model {
            pty.wait_visible(READY, DEADLINE);
            pty.send(b"/continue\r");
            wait_screen_row(&pty, "Sessions", DEADLINE);
            pty.send(b"Retained selection root\r");
            dismissed(&pty, "Sessions");
        }
        wait_screen_row(&pty, "unavailable", DEADLINE);
        if !retired_model {
            wait_screen_row(&pty, "variant-", DEADLINE);
            pty.send(b"/variants\r");
            wait_screen_row(&pty, "Select variant", DEADLINE);
            wait_screen_row(&pty, "variant-", DEADLINE);
            assert!(
                !render_screen(&pty.snapshot())
                    .rows()
                    .iter()
                    .any(|row| row.contains("● Default")),
                "retired fast must not show Default as selected"
            );
            pty.send(b"\x1b");
            dismissed(&pty, "Select variant");
        }
        assert!(fixture.requests.lock().unwrap().is_empty());
        pty.send(b"blocked draft\r");
        wait_screen_row(
            &pty,
            if retired_model {
                "model_unavailable"
            } else {
                "variant_unavailable"
            },
            DEADLINE,
        );
        wait_screen_row(&pty, "blocked draft", DEADLINE);
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "retired selection never submits"
        );
        pty.send(&vec![0x7f; "blocked draft".len()]);
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            saved_selection(&db, &fixture, "retired", DEFAULT_AGENT),
            before,
            "read and refused submit do not rewrite prefs"
        );
        assert!(db.read_history("retired").unwrap().is_empty());
        drop(db);
        let mut pty = PtySession::spawn(fixture.clone(), "retired", None);
        wait_screen_row(&pty, "unavailable", DEADLINE);
        if retired_model {
            choose_model(&mut pty, "T39 model");
        } else {
            pty.send(b"/variants\r");
            choose_variant(&mut pty, "Default");
        }
        let off = submit(&mut pty, "accepted after replacement");
        pty.wait_visible_after(off, "echo: accepted after replacement", DEADLINE);
        let request = fixture.wait_requests(1);
        assert_eq!(
            request[0]["model"],
            if retired_model { MODEL } else { ALT_MODEL }
        );
        assert!(
            request[0]["reasoning"]["effort"].is_null(),
            "explicit replacement cleared retired overlay"
        );
        pty.send(b"/quit\r");
        assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        assert_eq!(
            saved_selection(&db, &fixture, "retired", DEFAULT_AGENT),
            serde_json::json!({"provider":"fixture","id":if retired_model {MODEL} else {ALT_MODEL},"variant":null})
        );
    }
}

/// AUD29: panels reach the runtime and really change the next request.
#[test]
fn v04_raw_dialogs_preserve_draft_and_select_normal_provider_model_variant() {
    let dismissed = |pty: &PtySession, title: &str| {
        let start = Instant::now();
        while render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains(title))
        {
            assert!(start.elapsed() < DEADLINE, "modal did not dismiss: {title}");
            std::thread::sleep(POLL);
        }
    };
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for i in 0..30 {
        config["provider"]["fixture"]["models"][format!("modal-{i:02}")] = serde_json::json!({
            "name":format!("Modal {i:02}"), "limit":{"context":32768,"output":4096},
            // Named `none` is an explicit overlay, NOT the absence of a selection.
            "variants":{"none":{"reasoningEffort":"low"},"fast":{"reasoningEffort":"high"}}
        });
    }
    std::fs::write(&path, config.to_string()).unwrap();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"draft-kept");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\x10"); // actual Ctrl+P, never a direct TuiState call
    wait_screen_row(&pty, "Commands", DEADLINE);
    wait_screen_row(&pty, "Search", DEADLINE);
    pty.send(b"Switch model");
    wait_screen_row(&pty, "Switch model", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"missing-no-results");
    wait_screen_row(&pty, "No results found", DEADLINE);
    pty.send(b"\r"); // zero-result Enter has no application effect
    pty.send(b"\x03"); // clear query only
    wait_screen_row(&pty, "Modal 00", DEADLINE);
    pty.send(b"Modal");
    pty.send(b"\x1b[F"); // beyond the viewport, not just the first eight
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Modal 00"))
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    assert!(
        pty.child.try_wait().unwrap().is_none(),
        "Esc dismisses, not quits"
    );
    let dismissed_screen = render_screen(&pty.snapshot()).rows();
    assert!(
        dismissed_screen
            .iter()
            .any(|r| r.starts_with("   Untitled session")),
        "Esc preserves attached tab"
    );
    assert!(
        dismissed_screen
            .iter()
            .any(|r| r.contains("T39 model fixture")),
        "Esc preserves the effective model"
    );
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "browsing does not submit"
    );
    pty.send(b"\x18m"); // actual Ctrl+X, m
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"Modal 29");
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● Default", DEADLINE);
    // Model is already applied; Escape from its separate variant dialog leaves
    // the chosen model with no overlay, preserving the original prompt draft.
    pty.send(b"\x1b");
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\x10Switch model variant\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    // Selection updates the real prompt metadata; upstream does not emit a
    // selection-time toast or durable switch row before the next submission.
    wait_screen_row(&pty, "Modal 29 fixture · fast", DEADLINE);
    dismissed(&pty, "Select variant");
    wait_screen_row(&pty, "draft-kept", DEADLINE);
    pty.send(b"\r");
    pty.wait_visible("echo: draft-kept", DEADLINE);
    let first = fixture.wait_requests(1);
    assert_eq!(first[0]["model"], "modal-29");
    assert_eq!(first[0]["reasoning"]["effort"], "high");
    assert_eq!(last_user_text(&first[0]).as_deref(), Some("draft-kept"));
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/model\r"); // slash and raw keys converge on the same selector
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"Modal 29");
    wait_screen_row(&pty, "Modal 29", DEADLINE);
    // Existing valid fast is retained; the pinned model flow closes directly.
    pty.send(b"\r");
    dismissed(&pty, "Select model");
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● fast", DEADLINE);
    pty.send(b"none\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "named none variant");
    pty.wait_visible_after(off, "echo: named none variant", DEADLINE);
    assert_eq!(fixture.wait_requests(2)[1]["reasoning"]["effort"], "low");
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    {
        let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
        let saved = saved_selection(&db, &fixture, "v04-modal", DEFAULT_AGENT);
        assert_eq!(saved["id"], "modal-29");
        assert_eq!(
            saved["variant"], "none",
            "named none is persisted, not null"
        );
    }
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible("Fixture session title", DEADLINE);
    pty.send(b"/thinking\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● none", DEADLINE);
    // Focus-current means Enter re-applies none, not the first Default row.
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "none after restart");
    pty.wait_visible_after(off, "echo: none after restart", DEADLINE);
    assert_eq!(fixture.wait_requests(3)[2]["reasoning"]["effort"], "low");
    wait_idle(&pty);
    pty.send(b"/effort\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"Default\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "default variant");
    pty.wait_visible_after(off, "echo: default variant", DEADLINE);
    let requests = fixture.wait_requests(4);
    assert_eq!(requests[3]["model"], "modal-29");
    assert!(
        requests[3]["reasoning"]["effort"].is_null(),
        "explicit default must clear the previous variant: effort={} prompt={:?}",
        requests[3]["reasoning"]["effort"],
        last_user_text(&requests[3])
    );
    let off = submit(&mut pty, "slow stream");
    fixture.wait_requests(5);
    let started = Instant::now();
    pty.send(b"\x10");
    wait_screen_row(&pty, "Commands", Duration::from_millis(900));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "overlay responsive while provider pending"
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    pty.send(b"\x03");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let selection = saved_selection(&db, &fixture, "v04-modal", DEFAULT_AGENT);
    assert_eq!(selection["id"], "modal-29");
    assert_eq!(selection["variant"], serde_json::Value::Null);
    assert!(
        db.read_history("v04-modal")
            .unwrap()
            .iter()
            .any(|(role, text)| role == "user" && text == "draft-kept")
    );
    drop(db);
    let mut pty = PtySession::spawn(fixture.clone(), "v04-modal", None);
    pty.wait_visible("Fixture session title", DEADLINE);
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    wait_screen_row(&pty, "● Default", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "default after restart");
    pty.wait_visible_after(off, "echo: default after restart", DEADLINE);
    assert!(fixture.wait_requests(6)[5]["reasoning"]["effort"].is_null());
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
}

/// All new-session entry points call the real app operation; busy variants of
/// those same routes keep the current session, draft and provider turn intact.
#[test]
fn sessions_enter_renamed_foreign_root_preserves_deck_draft_and_uses_target_config() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "local-open-root", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/rename Local return target\r");
    wait_screen_row(&pty, "Local return target", DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let foreign = fixture.root.path().join("other-project");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(
        foreign.join("opencode.json"),
        serde_json::json!({"model":format!("fixture/{ALT_MODEL}")}).to_string(),
    )
    .unwrap();
    conn.execute_batch("INSERT INTO sessions(id,created_at,title) VALUES ('foreign-open-root','1','Foreign open target'); INSERT INTO messages(id,session_id,seq,role,text) VALUES ('foreign-user','foreign-open-root',1,'user','foreign history question'),('foreign-answer','foreign-open-root',2,'assistant','foreign history canary');").unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES ('tui.session_location.foreign-open-root',?1,'1')",[foreign.to_str().unwrap()]).unwrap();
    pty.send(b"local roundtrip draft\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x01\x1b[200~Foreign open target\x1b[201~\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Renamed foreign open target\r");
    dismissed(&pty, "Rename session");
    wait_screen_row(&pty, "local roundtrip draft", DEADLINE);
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Renamed foreign open target\x1b[201~\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "foreign history canary", DEADLINE);
    wait_screen_row(&pty, "other-project", DEADLINE);
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "opening an existing root is provider-free"
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let off = submit(&mut pty, "target configuration request");
    pty.wait_visible_after(off, "echo: target configuration request", DEADLINE);
    wait_idle(&pty);
    assert_eq!(fixture.wait_requests(1)[0]["model"], ALT_MODEL);
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Local return target\x1b[201~\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "local roundtrip draft", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let saved: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key=?1",
            [format!(
                "tui.selection.tab_deck:{}",
                serde_json::json!([foreign.to_str().unwrap()])
            )],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap()["sessions"],
        serde_json::json!(["foreign-open-root"])
    );
    pty.send(b"\x03/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_all_projects_selected_actions_keep_current_location_and_draft() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "local-current", None);
    pty.wait_visible(READY, DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let foreign = fixture.root.path().join("foreign-project");
    std::fs::create_dir_all(&foreign).unwrap();
    let foreign = foreign.to_str().unwrap();
    conn.execute_batch("INSERT INTO sessions(id,created_at,title) VALUES ('foreign-listed','1','Foreign selected target'); INSERT INTO sessions(id,created_at,parent_id) VALUES ('foreign-child','1','foreign-listed');").unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES ('tui.session_location.foreign-listed',?1,'1')",[foreign]).unwrap();
    conn.execute("INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'1')",rusqlite::params![format!("tui.selection.tab_deck:{}",serde_json::json!([foreign])),serde_json::json!({"version":1,"sessions":["foreign-listed"],"active":"foreign-listed"}).to_string()]).unwrap();
    pty.send(b"local draft preserved\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x01");
    wait_screen_row(&pty, "Foreign selected target", DEADLINE);
    pty.send(b"\x1b[200~Foreign selected target\x1b[201~\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Renamed foreign root\r");
    dismissed(&pty, "Rename session");
    wait_screen_row(&pty, "local draft preserved", DEADLINE);
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='foreign-listed'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Renamed foreign root"
    );
    pty.send(b"\x18l");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[200~Renamed foreign root\x1b[201~\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "No sessions found", DEADLINE);
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE id IN ('foreign-listed','foreign-child')",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM sessions WHERE id='local-current'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let saved: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key=?1",
            [format!(
                "tui.selection.tab_deck:{}",
                serde_json::json!([foreign])
            )],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&saved).unwrap()["sessions"],
        serde_json::json!([])
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "local draft preserved", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"\x03/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_bracketed_paste_queries_owner_for_root_outside_first_fifty() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "sessions-paste-current", None);
    pty.wait_visible(READY, DEADLINE);
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    let location: String = conn
        .query_row(
            "SELECT value FROM prefs WHERE key='tui.session_location.sessions-paste-current'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    for i in 0..61 {
        let id = format!("reverse-{:02}", 61 - i);
        let title = if i == 0 {
            "Unique oldest pasted target".to_string()
        } else {
            format!("Recent root {i}")
        };
        conn.execute(
            "INSERT INTO sessions(id,created_at,title) VALUES (?1,strftime('%s','now'),?2)",
            rusqlite::params![id, title],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prefs(key,value,updated_at) VALUES (?1,?2,'1')",
            rusqlite::params![format!("tui.session_location.{id}"), location],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events(session_id,kind,payload) VALUES (?1,'session_created','{}')",
            [id],
        )
        .unwrap();
    }
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    wait_screen_absent(&pty, "Unique oldest pasted target");
    pty.send(b"\x1b[200~Unique oldest pasted target\x1b[201~");
    wait_screen_row(&pty, "Unique oldest pasted target", DEADLINE);
    pty.send(b"\x12");
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Paste reached selected root\r");
    dismissed(&pty, "Rename session");
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='reverse-61'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Paste reached selected root"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored());
}

#[test]
fn sessions_selected_rename_and_confirmed_delete_reach_real_owner_without_provider_work() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "sessions-actions", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x12"); // pinned Ctrl+R acts on the selected row
    wait_screen_row(&pty, "Rename session", DEADLINE);
    pty.send(b"\x03Real selected title\r");
    wait_screen_row(&pty, "Real selected title", DEADLINE);
    dismissed(&pty, "Rename session");
    let conn = rusqlite::Connection::open(pty.data_dir().join("oc.sqlite")).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT title FROM sessions WHERE id='sessions-actions'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "Real selected title"
    );
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Real selected title", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "Press ctrl+d again to confirm", DEADLINE);
    pty.send(b"\x04");
    wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE);
    assert_eq!(
        conn.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "No sessions available", DEADLINE);
    pty.send(b"\x04\x12\r"); // empty actions and Enter have no target
    wait_screen_row(&pty, "No sessions available", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Sessions");
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
}

#[test]
fn v04_new_session_aliases_and_disabled_actions_have_real_effects_only_when_idle() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "v04-new", None);
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"\x10variant");
    wait_screen_row(&pty, "No results found", DEADLINE);
    assert!(
        fixture.requests.lock().unwrap().is_empty(),
        "no-variant command is absent from palette"
    );
    pty.send(b"\x1b");
    dismissed(&pty, "Commands");
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "No variants available", DEADLINE);
    assert!(fixture.requests.lock().unwrap().is_empty());
    pty.send(&[0x7f; 9]); // edit the unavailable /variants draft via real Backspace
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"T39 alt\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    wait_screen_row(&pty, "T39 alt fixture · fast", DEADLINE);
    let off = submit(&mut pty, "original session");
    pty.wait_visible_after(off, "echo: original session", DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Original session root\r");
    wait_screen_row(&pty, "Original session root", DEADLINE);
    let routes: &[&[u8]] = &[b"/new\r", b"/clear\r", b"\x18n", b"\x10New session\r"];
    for (i, route) in routes.iter().enumerate() {
        wait_idle(&pty);
        pty.send(route);
        wait_screen_row(&pty, "█▀▀█ █▀▀█", DEADLINE); // pinned New session returns Home
        assert!(
            !render_screen(&pty.snapshot())
                .rows()
                .iter()
                .any(|row| row.contains("echo:")),
            "new session has no old transcript"
        );
        if i == 1 {
            choose_model(&mut pty, "T39 alt");
            // Home model draft is separate from any session and restores the
            // existing per-model fast preference without reopening variants.
            assert!(
                !render_screen(&pty.snapshot())
                    .rows()
                    .iter()
                    .any(|r| r.contains("Select variant"))
            );
        }
        let text = format!("new route {i}");
        let off = submit(&mut pty, &text);
        pty.wait_visible_after(off, &format!("echo: {text}"), DEADLINE);
    }
    wait_idle(&pty);
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"Original session root\r");
    wait_screen_row(&pty, "echo: original session", DEADLINE);
    let off = submit(&mut pty, "slow stream");
    fixture.wait_requests(6);
    // Slash, chord and palette routes all consult the same availability policy.
    let mut draft_len = 0;
    for command in ["/new", "/clear", "/agents", "/continue"] {
        pty.send(&vec![0x7f; draft_len]);
        pty.send(command.as_bytes());
        wait_screen_row(&pty, command, DEADLINE);
        pty.send(b"\r");
        wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
        draft_len = command.len();
    }
    pty.send(&vec![0x7f; draft_len]);
    for (command, panel) in [
        ("/model", "Select model"),
        ("/variants", "Select variant"),
        ("/thinking", "Select variant"),
        ("/effort", "Select variant"),
    ] {
        pty.send(format!("{command}\r").as_bytes());
        wait_screen_row(&pty, panel, DEADLINE);
        pty.send(b"\x1b");
        dismissed(&pty, panel);
        assert_eq!(
            fixture.wait_requests(6).len(),
            6,
            "busy selector opening is local"
        );
    }
    pty.send(b"\x18m");
    wait_screen_row(&pty, "Select model", DEADLINE);
    pty.send(b"\x1b");
    dismissed(&pty, "Select model");
    for route in [b"\x18n".as_slice(), b"\x18a", b"\x18l"] {
        pty.send(route);
        wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    }
    pty.send(b"\x10New session");
    wait_screen_row(&pty, "Commands", DEADLINE);
    pty.send(b"\r");
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    assert!(
        render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Commands")),
        "disabled palette action stays open"
    );
    pty.send(b"\x1b");
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);
    wait_idle(&pty);
    let off = submit(&mut pty, "continued same session");
    pty.wait_visible_after(off, "echo: continued same session", DEADLINE);
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success() && contains(&out, ALT_LEAVE) && pty.restored());
    let requests = fixture.wait_requests(7);
    assert_eq!(
        requests.len(),
        7,
        "navigation and unavailable actions never submit"
    );
    for (i, request) in requests.iter().enumerate() {
        if [0, 2, 5, 6].contains(&i) {
            assert_eq!(request["model"], ALT_MODEL);
            assert_eq!(
                request["reasoning"]["effort"], "high",
                "original session restored"
            );
        } else {
            assert_eq!(
                request["model"], MODEL,
                "new Home uses its own draft/configured fallback"
            );
            assert!(request["reasoning"]["effort"].is_null());
        }
    }
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let saved = saved_selection(&db, &fixture, "v04-new", DEFAULT_AGENT);
    let draft_key = format!(
        "tui.selection.draft:{}",
        serde_json::json!([
            fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
            "fixture",
            DEFAULT_AGENT
        ])
    );
    assert!(
        db.get_pref(&draft_key).unwrap().is_none(),
        "local Home picker choice is captured into its accepted session, never a persisted uncommitted draft"
    );
    assert_eq!(saved["id"], ALT_MODEL);
    assert_eq!(
        saved["variant"], "fast",
        "disabled actions did not mutate stored selection"
    );
    let sessions = db.list_sessions().unwrap();
    assert_eq!(
        sessions.len(),
        5,
        "exactly four new sessions, none created while busy"
    );
    for i in 0..4 {
        let text = format!("new route {i}");
        let owners: Vec<_> = sessions
            .iter()
            .filter(|id| {
                db.read_history(id)
                    .unwrap()
                    .iter()
                    .any(|(role, t)| role == "user" && t == &text)
            })
            .collect();
        assert_eq!(owners.len(), 1);
        assert_ne!(owners[0], "v04-new");
        assert_eq!(
            db.read_history(owners[0]).unwrap().len(),
            2,
            "new session contains only its own turn"
        );
    }
    let original = db.read_history("v04-new").unwrap();
    assert_eq!(
        original
            .iter()
            .filter(|(role, _)| role == "user")
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        ["original session", "slow stream", "continued same session"]
    );
}

/// AUD29 continues to qualify native actions through their modal surfaces.
#[test]
fn aud29_pty_panels_change_runtime_state() {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    seed_session(&fixture.data_dir(), &project, "s-aud29-b", 2);
    let mut pty = PtySession::spawn(fixture.clone(), "s-aud29", None);
    pty.wait_visible(READY, DEADLINE);

    // Model picker: the effective model changes for the next turn.
    pty.send(b"/model\r");
    wait_screen_row(&pty, "Select model", DEADLINE);
    wait_screen_row(&pty, "T39 alt", DEADLINE);
    pty.send(b"T39 alt"); // Search selects the same genuine catalog entry.
    pty.send(b"\r");
    wait_screen_row(&pty, "Select variant", DEADLINE);
    pty.send(b"fast\r");
    wait_screen_row(&pty, "T39 alt fixture · fast", DEADLINE);

    let off = submit(&mut pty, "hello model");
    pty.wait_visible_after(off, "echo: hello model", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);

    // Agent picker: prompt and pinned model come from the agent definition.
    pty.send(b"/agents\r");
    wait_screen_row(&pty, "Select agent", DEADLINE);
    wait_screen_row(&pty, "t39agent", DEADLINE);
    // Built-in primaries are real picker rows; select the intended definition
    // by its ID rather than depending on the previous one-row inventory.
    pty.send(b"t39agent\r");
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|row| row.contains("agent: t39agent"))
    );

    let off = submit(&mut pty, "hello agent");
    pty.wait_visible_after(off, "echo: hello agent", DEADLINE);

    // Skill catalog: real cards from the runtime (bodies stay behind).
    pty.send(b"/skills\r");
    wait_screen_row(&pty, "Skills", DEADLINE);
    wait_screen_row(&pty, "T39 skill", DEADLINE);
    pty.send(b"\x1b"); // Esc closes
    std::thread::sleep(Duration::from_millis(200));

    // Custom command: the template is expanded by the application.
    let off = submit(&mut pty, "/t39cmd hello");
    pty.wait_visible_after(off, "echo: custom command payload for hello", DEADLINE);
    wait_idle(&pty); // echo may precede the terminal completion/receipt

    // Manual DCP compress: the model calls the compress tool, the runtime
    // stores a block and projects the same committed run into its chat card.
    pty.send(b"/dcp-compress early span\r");
    wait_screen_row(&pty, "compressions 1", DEADLINE);
    pty.send(b"\x1b"); // Esc closes the DCP panel
    wait_screen_row(&pty, "Compression #1", DEADLINE);
    wait_idle(&pty);

    // Session switch: the attached session (and its history) really changes.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"\x1b[B"); // Down: cursor moves off the first id
    pty.send(b"\r");
    pty.wait_visible(READY, DEADLINE);
    let off = submit(&mut pty, "hello switch");
    pty.wait_visible_after(off, "echo: hello switch", DEADLINE);
    pty.send(b"/quit\r");
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean exit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored");

    let requests = fixture.wait_requests(5);
    let model_of = |index: usize| requests[index]["model"].as_str().unwrap_or_default();
    assert_eq!(model_of(0), ALT_MODEL, "picker changed the request model");
    assert_eq!(
        requests[0]["reasoning"]["effort"], "high",
        "variant reached the request body"
    );
    assert_eq!(model_of(1), ALT_MODEL, "agent pinned model kept");
    assert_eq!(
        requests[1]["reasoning"]["effort"], "high",
        "full agent model#variant reached the normal request"
    );
    let agent_request = requests[1]["input"].to_string();
    assert!(
        agent_request.contains(AGENT_PROMPT),
        "agent prompt reached the provider request"
    );
    let command_prompt = last_user_text(&requests[2]).expect("command prompt");
    assert_eq!(
        command_prompt, "custom command payload for hello",
        "custom command template expanded"
    );
    assert!(
        requests.len() >= 5,
        "compress ran as a tool round: {} requests",
        requests.len()
    );

    let db = oc_adapters::storage::Db::open(pty.data_dir()).expect("db");
    let blocks = oc_adapters::dcp::load_blocks(&db, "s-aud29").expect("blocks");
    assert_eq!(blocks.len(), 1, "one stored compression block");
    let switched = oc_adapters::storage::Db::read_history(&db, "s-aud29-b").expect("switched");
    assert!(
        switched
            .iter()
            .any(|(role, text)| role == "user" && text == "hello switch"),
        "turn landed in the switched session: {switched:?}"
    );
    let original = oc_adapters::storage::Db::read_history(&db, "s-aud29").expect("original");
    assert!(
        !original.iter().any(|(_, text)| text == "hello switch"),
        "turn did not stay in the original session"
    );
    let cards = oc_adapters::storage::Db::list_tool_ops(&db, "s-aud29").expect("ops");
    assert!(
        cards
            .iter()
            .any(|row| row.name == "compress" && row.state == "completed"),
        "compress tool op recorded: {cards:?}"
    );
}

/// AUD30: paste, unicode, resize, Ctrl-C and a failing output handle.
#[test]
fn aud30_pty_paste_resize_error_recovery() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "s-aud30", None);
    pty.wait_visible(READY, DEADLINE);

    // Bracketed paste arrives as one event with unicode intact.
    let off = pty.snapshot().len();
    pty.send("\x1b[200~привет 🌍\x1b[201~".as_bytes());
    pty.send(b"\r");
    pty.wait_visible_after(off, "┃привет🌍", DEADLINE);
    pty.wait_visible_after(off, "echo: привет 🌍", DEADLINE);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);

    // Resize while a stream is running: the frame follows the new size and
    // the turn still completes.
    let off = submit(&mut pty, "slow stream");
    std::thread::sleep(Duration::from_millis(300));
    pty.resize(100, 30);
    // A session switch during a stream is explicitly refused, never silent.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "turn active; action unavailable", DEADLINE);
    // The refused slash suggestion remains visible while its draft is editable;
    // dismiss that overlay before checking that no session dialog opened.
    pty.send(b"\x1b");
    let start = Instant::now();
    while render_screen(&pty.snapshot())
        .rows()
        .iter()
        .any(|r| r.contains("/sessions") && r.contains("Switch session"))
    {
        assert!(
            start.elapsed() < DEADLINE,
            "slash overlay was not dismissed"
        );
        std::thread::sleep(POLL);
    }
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Sessions"))
    );
    pty.send(&[0x7f; 9]); // refused /sessions leaves the draft available for editing
    pty.wait_visible_after(off, "answer:slow stream", DEADLINE);

    pty.send(b"\x03"); // Ctrl-C quits from idle
    let (status, out) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "ctrl-c quit");
    assert!(contains(&out, ALT_LEAVE), "alternate screen left");
    assert!(pty.restored(), "terminal restored after paste/resize");

    assert_eq!(
        persisted(pty.data_dir(), "s-aud30")
            .iter()
            .map(|(role, text)| (role.as_str(), text.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("user", "привет 🌍"),
            ("assistant", "echo: привет 🌍"),
            ("user", "slow stream"),
            ("assistant", "answer:slow stream"),
        ],
        "no extra persisted user messages"
    );

    // A renderer that cannot write its output handle exits visibly and
    // restores the terminal instead of hanging or leaving raw mode.
    let broken = Fixture::new();
    let mut bad = PtySession::spawn_bad_stdout(broken.clone(), "s-aud30-bad");
    let (status, out) = bad.wait_exit(DEADLINE);
    assert!(!status.success(), "broken output handle exits nonzero");
    let visible = visible_text(&out);
    let text = String::from_utf8_lossy(&visible);
    assert!(
        text.contains("error: draw:") || text.contains("error:"),
        "visible error: {text:?}"
    );
    assert!(bad.restored(), "terminal restored after draw failure");
    let history = persisted(bad.data_dir(), "s-aud30-bad");
    assert!(history.is_empty(), "no user message persisted: {history:?}");
}
