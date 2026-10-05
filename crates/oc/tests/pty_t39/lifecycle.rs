//! Actual-binary scheduling, retention, cancellation and restart/resource lifecycle.

use super::*;

/// VIS31: actual terminal writes and process CPU in a stable idle window,
/// followed by paced unique glyphs (not a configured FPS inferred as output).
#[test]
fn vis31_demand_driven_idle_and_high_rate_input_paints() {
    let fixture = Fixture::new();
    let metrics_path = fixture.root.path().join("scheduling.json");
    let mut pty = PtySession::spawn(fixture, "scheduling", Some(&metrics_path));
    pty.wait_visible(READY, DEADLINE);
    std::thread::sleep(Duration::from_millis(200));
    let idle_start = Instant::now();
    let bytes_before = pty.snapshot().len();
    let cpu_before = process_cpu_ticks(pty.child.id());
    std::thread::sleep(Duration::from_secs(1));
    let cpu_ticks = process_cpu_ticks(pty.child.id()) - cpu_before;
    assert_eq!(
        pty.snapshot().len(),
        bytes_before,
        "stable idle wrote terminal bytes"
    );
    let idle_end = idle_start.elapsed();
    eprintln!("VIS31 idle window={idle_end:?} CPU ticks={cpu_ticks}");
    // The old-loop baseline used 3–4 CPU ticks and 627–660 terminal bytes
    // per second on this host. Stable idle must have no periodic paint work.
    assert!(cpu_ticks <= 1, "stable idle consumed {cpu_ticks} CPU ticks");
    let mut measured = Vec::new();
    for hz in [165_u32, 250] {
        let phase = Instant::now();
        let mut latencies = Vec::new();
        let offset = pty.snapshot().len();
        let mut writer = pty.master.try_clone().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        // Injection must not wait for rendering: slow paints cannot silently
        // lower the requested event rate and make the latency sample look good.
        let sender = std::thread::spawn(move || {
            for index in 0..32 {
                let due = phase + Duration::from_secs_f64(f64::from(index) / f64::from(hz));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                let glyph = char::from_u32(0x4e00 + index + hz).unwrap().to_string();
                let sent = Instant::now();
                writer.write_all(glyph.as_bytes()).unwrap();
                tx.send((sent, glyph)).unwrap();
            }
        });
        for (sent, glyph) in rx {
            while !contains(&pty.snapshot()[offset..], glyph.as_bytes()) {
                assert!(
                    sent.elapsed() < Duration::from_millis(100),
                    "glyph did not paint promptly"
                );
                std::thread::sleep(Duration::from_micros(100));
            }
            latencies.push(sent.elapsed().as_micros());
        }
        sender.join().unwrap();
        latencies.sort_unstable();
        measured.push((
            hz,
            phase.elapsed().as_micros(),
            latencies[16],
            latencies[30],
            latencies[31],
        ));
        pty.send(b"\x03");
        std::thread::sleep(Duration::from_millis(50));
    }
    pty.resize(120, 40);
    std::thread::sleep(Duration::from_millis(100));
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success());
    assert!(pty.restored());
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    let samples = metrics["frame_samples_ns"].as_array().unwrap();
    // This window starts after startup, before any injected input. A repeated
    // Terminal::draw with an unchanged buffer is still a forbidden idle attempt.
    assert!(
        !samples.iter().any(|sample| {
            let at = sample[0].as_u64().unwrap();
            (250_000_000..1_000_000_000).contains(&at)
        }),
        "idle render attempts: {metrics}"
    );
    eprintln!("VIS31 (hz, window_us, p50_us, p95_us, max_us)={measured:?}; metrics={metrics}");
}

/// Nearest VIS31 fairness risk: independently paced UTF-8 keyboard input
/// while a real Responses peer sends a bounded burst of worker updates.
#[test]
fn vis31_high_rate_input_during_provider_burst() {
    for hz in [165_u32, 250] {
        let fixture = Fixture::new();
        let metrics_path = fixture.root.path().join("burst-scheduling.json");
        let mut pty = PtySession::spawn(fixture, "burst-scheduling", Some(&metrics_path));
        pty.wait_visible(READY, DEADLINE);
        let off = submit(&mut pty, "vis31 provider burst");
        pty.wait_visible_after(off, "burst-", DEADLINE);
        let phase = Instant::now();
        let offset = pty.snapshot().len();
        let mut writer = pty.master.try_clone().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let sender = std::thread::spawn(move || {
            for index in 0..32 {
                let due = phase + Duration::from_secs_f64(f64::from(index) / f64::from(hz));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                let glyph = char::from_u32(0x5000 + index + hz).unwrap().to_string();
                let sent = Instant::now();
                writer.write_all(glyph.as_bytes()).unwrap();
                tx.send((sent, glyph)).unwrap();
            }
        });
        let mut latencies = Vec::new();
        for (sent, glyph) in rx {
            while !contains(&pty.snapshot()[offset..], glyph.as_bytes()) {
                assert!(
                    sent.elapsed() < Duration::from_millis(100),
                    "provider burst starved input paint"
                );
                std::thread::sleep(Duration::from_micros(100));
            }
            latencies.push(sent.elapsed().as_micros());
        }
        sender.join().unwrap();
        pty.wait_visible_after(off, "VIS31_PROVIDER_BURST_DONE", DEADLINE);
        latencies.sort_unstable();
        eprintln!(
            "VIS31 burst hz={hz} window_us={} p50_us={} p95_us={} max_us={}",
            phase.elapsed().as_micros(),
            latencies[16],
            latencies[30],
            latencies[31]
        );
        pty.send(b"\x03");
        // U48 now renders a real finite completion pulse after the running
        // indicator releases: 500 ms release + 1,200 ms completion. That is
        // active animation, not settled idle. Keep the idle byte/CPU gate
        // unchanged, but start it after those source-defined clocks drain.
        std::thread::sleep(Duration::from_millis(500 + 1_200));
        let bytes = pty.snapshot().len();
        let cpu = process_cpu_ticks(pty.child.id());
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(
            pty.snapshot().len(),
            bytes,
            "settled burst wrote idle terminal bytes"
        );
        assert!(process_cpu_ticks(pty.child.id()) - cpu <= 1);
        pty.send(b"\x03");
        assert!(pty.wait_exit(DEADLINE).0.success());
        assert!(pty.restored());
        let metrics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
        assert_eq!(
            metrics["worker_event_queue_lagged"], 0,
            "worker events lost: {metrics}"
        );
        eprintln!("VIS31 burst metrics={metrics}");
    }
}

/// The paired wheel campaign exposed a real TurnFinished refresh regression:
/// completion must preserve the detached painted anchor, not repin the view.
#[test]
fn vis32_detached_stream_anchor_survives_durable_completion_refresh() {
    let fixture = Fixture::new();
    let mut pty = PtySession::spawn(fixture.clone(), "wheel-completion", None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis32 held rows");
    wait_screen_row(&pty, "VIS32_ROW_059", DEADLINE);
    for _ in 0..12 {
        pty.send(b"\x1b[<64;20;8M");
        std::thread::sleep(Duration::from_micros(6060));
    }
    std::thread::sleep(Duration::from_millis(250));
    let markers = |pty: &PtySession| {
        render_screen(&pty.snapshot())
            .rows()
            .into_iter()
            .filter_map(|row| {
                let index = row.find("VIS32_ROW_")?;
                Some(row[index..index + "VIS32_ROW_000".len()].to_string())
            })
            .collect::<Vec<_>>()
    };
    let before = markers(&pty);
    assert!(!before.is_empty());
    assert!(!before.iter().any(|row| row == "VIS32_ROW_059"));
    fixture.s07_continue.store(true, Ordering::Relaxed);
    wait_idle(&pty);
    let after = markers(&pty);
    assert_eq!(
        after.first(),
        before.first(),
        "durable completion moved detached viewport"
    );
    assert!(
        after
            .iter()
            .zip(&before)
            .all(|(after, before)| after == before)
    );
    pty.send(b"\x03");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
}

#[test]
fn vis28_real_pty_scanner_cycles_then_esc_cancels_and_static_fallback_completes() {
    let fixture = Fixture::new();
    let session = "vis28-animated";
    let mut pty = PtySession::spawn(fixture.clone(), session, None);
    pty.wait_visible(READY, DEADLINE);
    submit(&mut pty, "vis28 held stream");
    assert_eq!(
        last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
        Some("vis28 held stream")
    );
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    let initial_rows = render_screen(&pty.snapshot()).rows();
    let footer_y = initial_rows
        .iter()
        .position(|row| row.contains("esc interrupt"))
        .expect("painted running footer");
    let metadata = initial_rows[footer_y - 1].clone();
    assert!(
        !metadata.trim().is_empty(),
        "painted prompt metadata: {initial_rows:?}"
    );
    let mut samples = Vec::new();
    let began = Instant::now();
    // 54 * 40 ms nominal cycle; multiple cycles and a 6s cap tolerate PTY
    // reader/scheduler jitter without requiring exact frame timestamps.
    while began.elapsed() < Duration::from_secs(6) {
        if let Some(sample) = vis28_scanner(&pty, true) {
            samples.push(sample);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(
        render_screen(&pty.snapshot()).rows().get(footer_y - 1) == Some(&metadata),
        "agent/model metadata shifted during animation"
    );
    assert!(
        !fixture.vis28_continue.load(Ordering::Relaxed),
        "stream was released early"
    );
    let frames: Vec<_> = samples.iter().map(|(glyphs, _)| glyphs.as_str()).collect();
    // Require movement within each sweep, rather than exact intermediate
    // frame IDs: a 40ms PTY sample may legitimately skip a renderer tick.
    let mut phase = 0;
    let (mut forward, mut reverse) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
    );
    for frame in &frames {
        let prefix = frame.chars().take_while(|ch| *ch == '■').count();
        let suffix = frame.chars().rev().take_while(|ch| *ch == '■').count();
        match phase {
            0 if (1..=6).contains(&prefix) => {
                forward.insert(prefix);
            }
            0 if *frame == "⬝⬝⬝⬝⬝⬝⬝⬝" => {
                if forward.len() >= 3 && forward.iter().any(|n| *n >= 4) {
                    phase = 1; // end hold after observable forward motion
                } else {
                    forward.clear();
                }
            }
            1 if (2..=6).contains(&suffix) => {
                reverse.insert(suffix);
            }
            1 if *frame == "⬝⬝⬝⬝⬝⬝⬝⬝" && reverse.len() >= 3 && reverse.iter().any(|n| *n >= 4) =>
            {
                phase = 2; // start hold after observable reverse motion
                break;
            }
            _ => {}
        }
    }
    assert!(
        phase == 2,
        "missing observed forward/end-hold/reverse/start-hold cycle; distinct frames: {:?}",
        frames
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    );
    let fading = samples.windows(2).any(|pair| {
        pair[0].0 == "⬝⬝⬝⬝⬝⬝⬝⬝"
            && pair[1].0 == pair[0].0
            && pair[0].1.is_some()
            && pair[1].1.is_some()
            && pair[0].1 != pair[1].1
    });
    assert!(
        fading,
        "stationary hold must visibly fade in painted SGR colors"
    );
    pty.send(b"\x1b");
    wait_screen_row(&pty, "esc again to interrupt", DEADLINE);
    assert!(
        !fixture.vis28_continue.load(Ordering::Relaxed),
        "first Esc cannot release the held request"
    );
    pty.send(b"\x1b");
    vis28_wait_gone(&pty);
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    pty.send(b"/quit\r");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(pty.data_dir(), session),
        [("user".into(), "vis28 held stream".into())],
        "cancelled turn must remain durable without fabricated answer"
    );

    // A fresh process reads the project's animations:false setting.
    std::fs::write(
        fixture.root.path().join("project/opencode.json"),
        r#"{"animations":false}"#,
    )
    .expect("static config");
    fixture.vis28_continue.store(false, Ordering::Relaxed);
    let session = "vis28-static";
    let mut static_pty = PtySession::spawn(fixture.clone(), session, None);
    static_pty.wait_visible(READY, DEADLINE);
    submit(&mut static_pty, "vis28 held stream");
    fixture.wait_requests(2);
    wait_screen_row(&static_pty, "esc interrupt", DEADLINE);
    let began = Instant::now();
    while began.elapsed() < Duration::from_millis(350) {
        let (glyphs, _) = vis28_scanner(&static_pty, false).expect("running static footer");
        assert_eq!(glyphs, "[⋯]", "animation-disabled fixture");
        assert!(
            !render_screen(&static_pty.snapshot())
                .rows()
                .iter()
                .any(|row| row.contains('■') || row.contains('⬝'))
        );
        std::thread::sleep(Duration::from_millis(40));
    }
    fixture.vis28_continue.store(true, Ordering::Relaxed);
    wait_screen_row(&static_pty, "answer:vis28 completed", DEADLINE);
    vis28_wait_gone(&static_pty);
    static_pty.send(b"/quit\r");
    let (status, output) = static_pty.wait_exit(DEADLINE);
    assert!(status.success() && static_pty.restored() && contains(&output, ALT_LEAVE));
    assert_eq!(
        persisted(static_pty.data_dir(), session),
        [
            ("user".into(), "vis28 held stream".into()),
            ("assistant".into(), "answer:vis28 completed".into())
        ]
    );
}

/// Own-running deck presentation under a held real Responses request: hover
/// cannot close/submit, off mode still scrolls once, and completion leaves no
/// tab clock writing to an otherwise idle terminal.
#[test]
fn vis39_vis41_real_pty_held_tab_spinner_and_one_cycle_marquee() {
    const LONG_TITLE: &str = "abcdefghijklmnopqrstuvwxyz123456789ABCDEFGHIJKLMNO";
    const DOTS: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let header = |pty: &PtySession| {
        render_screen(&pty.snapshot())
            .cells
            .first()
            .map(|row| row.iter().take(32).copied().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    for animations in [true, false] {
        let fixture = Fixture::new();
        std::fs::write(
            fixture.root.path().join("project/opencode.json"),
            serde_json::json!({"animations": animations}).to_string(),
        )
        .unwrap();
        let session = if animations {
            "vis41-animated"
        } else {
            "vis41-static"
        };
        let metrics_path = fixture.root.path().join("tab-scheduling.json");
        let mut pty = PtySession::spawn(fixture.clone(), session, Some(&metrics_path));
        pty.wait_visible(READY, DEADLINE);
        pty.send(format!("/rename {LONG_TITLE}\r").as_bytes());
        let began = Instant::now();
        while !header(&pty)
            .iter()
            .collect::<String>()
            .contains("abcdefghijklmnop")
        {
            assert!(
                began.elapsed() < DEADLINE,
                "manual title did not reach the real tab"
            );
            std::thread::sleep(POLL);
        }
        assert!(
            fixture.requests.lock().unwrap().is_empty(),
            "native rename has no provider effect"
        );
        submit(&mut pty, "vis28 held stream");
        assert_eq!(
            last_user_text(&fixture.wait_requests(1)[0]).as_deref(),
            Some("vis28 held stream")
        );
        wait_screen_row(&pty, "esc interrupt", DEADLINE);
        vis27_mouse(&mut pty, 35, 4, 1, 'M'); // SGR is one-based
        let began = Instant::now();
        let mut frames = std::collections::BTreeSet::new();
        let mut scrolled = false;
        // U56 capture: title width50 + gap3, first step600, return at4760,
        // leading tween settled at5010. Reader/scheduler jitter gets a margin.
        while began.elapsed() < Duration::from_millis(5500) {
            let row = header(&pty);
            if let Some(&frame) = row.get(1) {
                assert!(
                    DOTS.contains(&frame),
                    "held tab status is not running: {row:?}"
                );
                frames.insert(frame);
            }
            assert!(
                !row.contains(&'✕'),
                "busy hover painted an ineligible close"
            );
            scrolled |= row.get(3).is_some_and(|&ch| ch != 'a');
            // Same-tab motion never restarts the captured 600/80 clock.
            if began.elapsed() < Duration::from_secs(1) {
                vis27_mouse(&mut pty, 35, 5, 1, 'M');
            }
            std::thread::sleep(POLL);
        }
        assert!(scrolled, "animation setting cannot disable hover marquee");
        assert_eq!(
            header(&pty).get(3),
            Some(&'a'),
            "one cycle returns to offset zero"
        );
        if animations {
            assert!(frames.len() >= 5, "no observable dots cycle: {frames:?}");
        } else {
            assert_eq!(frames, ['⠋'].into_iter().collect());
        }
        assert!(!fixture.vis28_continue.load(Ordering::Relaxed));
        assert_eq!(
            fixture.requests.lock().unwrap().len(),
            1,
            "hover never selects/submits/title-refreshes"
        );
        fixture.vis28_continue.store(true, Ordering::Relaxed);
        wait_screen_row(&pty, "answer:vis28 completed", DEADLINE);
        vis28_wait_gone(&pty);
        let after_completion = pty.snapshot().len();
        std::thread::sleep(Duration::from_millis(250));
        if animations {
            assert!(
                pty.snapshot().len() > after_completion,
                "own-running release/edge flash must continue after the real terminal receipt"
            );
        }
        // The current root is viewed, so no synthetic unread/completion pulse:
        // source run release500, whitecap700 and edge flash800 all settle.
        std::thread::sleep(Duration::from_millis(650));
        let mut idle_windows = vec![("completed", tab_idle_window(&pty))];
        vis27_mouse(&mut pty, 35, 70, 3, 'M');
        std::thread::sleep(Duration::from_millis(50));
        vis27_mouse(&mut pty, 35, 4, 1, 'M');
        let entered = Instant::now();
        while header(&pty).get(3) == Some(&'a') {
            assert!(
                entered.elapsed() < DEADLINE,
                "re-entry must start a new source hover cycle"
            );
            std::thread::sleep(POLL);
        }
        vis27_mouse(&mut pty, 35, 70, 3, 'M');
        let left = Instant::now();
        while header(&pty).get(3) != Some(&'a') {
            assert!(
                left.elapsed() < DEADLINE,
                "hover leave did not reset the marquee"
            );
            std::thread::sleep(POLL);
        }
        idle_windows.push(("hover-leave", tab_idle_window(&pty)));
        vis27_mouse(&mut pty, 35, 4, 1, 'M');
        pty.send(b"\x10");
        wait_screen_row(&pty, "Commands", DEADLINE);
        idle_windows.push(("modal", tab_idle_window(&pty)));
        pty.send(b"\x1b");
        std::thread::sleep(Duration::from_millis(100));
        idle_windows.push(("modal-close", tab_idle_window(&pty)));
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        pty.send(b"\x03");
        let (status, output) = pty.wait_exit(DEADLINE);
        assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
        let metrics: serde_json::Value =
            serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
        assert_tab_idle_metrics(&metrics, &idle_windows);
        assert_eq!(
            persisted(pty.data_dir(), session),
            [
                ("user".into(), "vis28 held stream".into()),
                ("assistant".into(), "answer:vis28 completed".into()),
            ]
        );
    }
}

#[test]
fn vis39_vis41_real_pty_cancel_disposes_tab_and_marquee_deadlines() {
    let fixture = Fixture::new();
    let session = "vis39-cancel-clock";
    let metrics_path = fixture.root.path().join("cancel-tab-scheduling.json");
    let mut pty = PtySession::spawn(fixture.clone(), session, Some(&metrics_path));
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/rename abcdefghijklmnopqrstuvwxyz123456789ABCDEFGHIJKLMNO\r");
    wait_screen_row(&pty, "abcdefghijklmnop", DEADLINE);
    submit(&mut pty, "vis28 held stream");
    fixture.wait_requests(1);
    wait_screen_row(&pty, "esc interrupt", DEADLINE);
    vis27_mouse(&mut pty, 35, 4, 1, 'M');
    std::thread::sleep(Duration::from_millis(650));
    pty.send(b"\x1b");
    wait_screen_row(&pty, "esc again to interrupt", DEADLINE);
    pty.send(b"\x1b");
    vis28_wait_gone(&pty);
    vis27_mouse(&mut pty, 35, 70, 3, 'M');
    std::thread::sleep(Duration::from_millis(900));
    let idle = tab_idle_window(&pty);
    assert!(
        !fixture.vis28_continue.load(Ordering::Relaxed),
        "cancel must not release the held provider barrier"
    );
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    assert_tab_idle_metrics(&metrics, &[("cancel-and-leave", idle)]);
    assert_eq!(
        persisted(pty.data_dir(), session),
        [("user".into(), "vis28 held stream".into())]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn title_cancel_and_watchdog_leave_the_main_turn_completed() {
    let fixture = Fixture::new();
    fixture.hold_title.store(true, Ordering::Relaxed);
    let home = fixture.root.path().join("home");
    let env = std::collections::BTreeMap::from([
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home.join("data").to_string_lossy().into_owned(),
        ),
        ("OC_FIXTURE_KEY".to_string(), "fixture-not-a-secret".into()),
        ("OC_TEST_ALLOW_LOOPBACK".to_string(), "1".into()),
    ]);
    let (app, guard, _) = oc_adapters::application::spawn_with_env(
        &fixture.root.path().join("project"),
        &fixture.data_dir(),
        env,
    )
    .await
    .unwrap();
    let session = oc_core::domain::SessionId("title-cancel".into());
    app.create_session(session.clone()).await.unwrap();
    let mut events = app.subscribe();
    let turn = app
        .submit(session.clone(), "title still pending".into())
        .await
        .unwrap();
    let requests = fixture.wait_requests(1);
    assert_eq!(requests[0]["model"], MODEL);
    let start = Instant::now();
    while !fixture.requests.lock().unwrap().iter().any(title::is_title) {
        assert!(start.elapsed() < DEADLINE, "missing title request");
        tokio::time::sleep(POLL).await;
    }
    app.cancel_title(session.clone()).await.unwrap();
    let start = Instant::now();
    while !fixture.title_closed.load(Ordering::Relaxed) {
        assert!(
            start.elapsed() < DEADLINE,
            "cancel did not close ancillary title request"
        );
        tokio::time::sleep(POLL).await;
    }
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("main turn did not finish after title cancellation")
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnInterrupted { turn: id, .. } if id == turn => {
                panic!("cancelled title must not interrupt a committed main turn")
            }
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => panic!("main turn: {error}"),
            _ => {}
        }
    }
    // A different untitled session now exercises the ten-second title
    // watchdog, without turning a completed main answer into an interruption.
    fixture.title_closed.store(false, Ordering::Relaxed);
    let session = oc_core::domain::SessionId("title-timeout".into());
    app.create_session(session.clone()).await.unwrap();
    let turn = app
        .submit(session, "title will timeout".into())
        .await
        .unwrap();
    fixture.wait_requests(2);
    let start = Instant::now();
    while fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|request| title::is_title(request))
        .count()
        < 2
    {
        assert!(start.elapsed() < DEADLINE, "missing second title request");
        tokio::time::sleep(POLL).await;
    }
    let start = Instant::now();
    while !fixture.title_closed.load(Ordering::Relaxed) {
        assert!(
            start.elapsed() < DEADLINE,
            "title watchdog did not close request"
        );
        tokio::time::sleep(POLL).await;
    }
    assert!(start.elapsed() >= Duration::from_secs(9));
    loop {
        match tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("committed main did not finish after title watchdog")
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnInterrupted { turn: id, .. } if id == turn => {
                panic!("title watchdog must not interrupt a committed main turn")
            }
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => panic!("main turn: {error}"),
            _ => {}
        }
    }
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    assert!(
        events.try_recv().is_err(),
        "completed main must have one terminal event"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn v04_legacy_headless_api_supersedes_older_scoped_session_drafts() {
    use oc_core::queries::SessionSelectionAction as Action;
    let fixture = Fixture::new();
    let home = fixture.root.path().join("home");
    let env = std::collections::BTreeMap::from([
        ("HOME".to_string(), home.to_string_lossy().into_owned()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.join("config").to_string_lossy().into_owned(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            home.join("data").to_string_lossy().into_owned(),
        ),
        ("OC_FIXTURE_KEY".to_string(), "fixture-not-a-secret".into()),
        ("OC_TEST_ALLOW_LOOPBACK".to_string(), "1".into()),
    ]);
    let project = fixture.root.path().join("project");
    let session = oc_core::domain::SessionId("scoped-headless".into());
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &fixture.data_dir(), env.clone())
            .await
            .unwrap();
    app.create_session(session.clone()).await.unwrap();
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some(DEFAULT_AGENT),
        "assert the real owner identity used by scoped preference keys"
    );
    app.session_selection(session.clone(), false, Action::Model(ALT_MODEL.into()))
        .await
        .unwrap();
    app.session_selection(session.clone(), false, Action::Variant(Some("fast".into())))
        .await
        .unwrap();
    let home_session = oc_core::domain::SessionId("old-home".into());
    app.create_session(home_session.clone()).await.unwrap();
    app.session_selection(home_session, true, Action::Model(ALT_MODEL.into()))
        .await
        .unwrap();
    let snapshot = app.select_model(MODEL.into(), None).await.unwrap();
    assert_eq!(snapshot.model_id, MODEL);
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .model_id,
        MODEL
    );
    let fresh = oc_core::domain::SessionId("fresh-headless".into());
    app.create_session(fresh.clone()).await.unwrap();
    assert_eq!(
        app.session_selection(fresh, false, Action::Current)
            .await
            .unwrap()
            .model_id,
        MODEL,
        "older Home draft cannot mask explicit headless selection in a fresh session"
    );
    let mut events = app.subscribe();
    let turn = app
        .submit(session.clone(), "legacy model".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy turn failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(1);
    assert_eq!(request[0]["model"], MODEL);
    assert!(request[0]["reasoning"]["effort"].is_null());
    let snapshot = app
        .select_model(ALT_MODEL.into(), Some("fast".into()))
        .await
        .unwrap();
    assert_eq!(snapshot.model_id, ALT_MODEL);
    assert_eq!(snapshot.variant.as_deref(), Some("fast"));
    let turn = app
        .submit(session.clone(), "legacy variant".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy variant failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(2);
    assert_eq!(request[1]["model"], ALT_MODEL);
    assert_eq!(request[1]["reasoning"]["effort"], "high");
    let snapshot = app.select_agent("t39agent".into()).await.unwrap();
    assert_eq!(snapshot.agent_id.as_deref(), Some("t39agent"));
    assert_eq!(
        app.session_selection(session.clone(), false, Action::Current)
            .await
            .unwrap()
            .model_id,
        ALT_MODEL
    );
    let turn = app
        .submit(session.clone(), "legacy agent".into())
        .await
        .unwrap();
    loop {
        match tokio::time::timeout(DEADLINE, events.recv())
            .await
            .unwrap()
            .unwrap()
        {
            oc_core::core_app::CoreEvent::TurnFinished { turn: id, .. } if id == turn => break,
            oc_core::core_app::CoreEvent::TurnFailed { error, .. } => {
                panic!("legacy agent failed: {error}")
            }
            _ => {}
        }
    }
    let request = fixture.wait_requests(3);
    assert_eq!(request[2]["model"], ALT_MODEL);
    assert_eq!(request[2]["reasoning"]["effort"], "high");
    assert!(request[2]["input"].to_string().contains(AGENT_PROMPT));
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, &session.0, DEFAULT_AGENT),
        serde_json::json!({"id":ALT_MODEL,"variant":"fast"}),
        "legacy API does not erase old scoped records"
    );
    drop(db);
    let (app, guard, _) =
        oc_adapters::application::spawn_with_env(&project, &fixture.data_dir(), env)
            .await
            .unwrap();
    assert_eq!(
        app.session_selection(session, false, Action::Current)
            .await
            .unwrap()
            .agent_id
            .as_deref(),
        Some("t39agent"),
        "legacy precedence survives restart"
    );
    app.shutdown().await.unwrap();
    guard.join().await.unwrap();
}

/// AUD29 continues to qualify native actions through their modal surfaces.
#[test]
fn v04_scoped_session_agent_and_model_preferences_survive_restart() {
    let fixture = Fixture::new();
    let path = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["default_agent"] = "plain".into();
    config["agent"]["plain"] =
        serde_json::json!({"mode":"primary","prompt":"Plain fixture agent."});
    config["provider"]["fixture"]["models"][ALT_MODEL]["variants"]["none"] =
        serde_json::json!({"reasoningEffort":"low"});
    std::fs::write(path, config.to_string()).unwrap();
    seed_session(
        &fixture.data_dir(),
        &fixture.root.path().join("project"),
        "scope-b",
        0,
    );
    let mut pty = PtySession::spawn(fixture.clone(), "scope-a", None);
    pty.wait_visible(READY, DEADLINE);
    choose_model(&mut pty, "T39 alt");
    choose_variant(&mut pty, "none");
    let off = submit(&mut pty, "A named none");
    pty.wait_visible_after(off, "echo: A named none", DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Scoped alpha root\r");
    wait_screen_row(&pty, "Scoped alpha root", DEADLINE);
    pty.send(b"/continue\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    // A is focused and newest after its real title update. B is the only
    // other root and must still receive its genuine automatic first title.
    pty.send(b"\x1b[B\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "T39 model fixture", DEADLINE);
    let off = submit(&mut pty, "B different model");
    pty.wait_visible_after(off, "echo: B different model", DEADLINE);
    wait_idle(&pty);
    wait_screen_row(&pty, "Fixture session title", DEADLINE);
    pty.send(b"/rename Scoped beta root\r");
    wait_screen_row(&pty, "Scoped beta root", DEADLINE);
    pty.send(b"/continue\rScoped alpha root\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "echo: A named none", DEADLINE);
    pty.send(b"/variants\r");
    wait_screen_row(&pty, "● none", DEADLINE);
    pty.send(b"\r"); // current focus, not merely a displayed dot
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A restored");
    pty.wait_visible_after(off, "echo: A restored", DEADLINE);
    choose_model(&mut pty, "T39 model");
    choose_model(&mut pty, "T39 alt"); // preferred none restores; no variant dialog
    assert!(
        !render_screen(&pty.snapshot())
            .rows()
            .iter()
            .any(|r| r.contains("Select variant"))
    );
    let off = submit(&mut pty, "A per model preference");
    pty.wait_visible_after(off, "echo: A per model preference", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rt39agent\r");
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    choose_model(&mut pty, "T39 model"); // agent-specific override
    let off = submit(&mut pty, "A second agent override");
    pty.wait_visible_after(off, "echo: A second agent override", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rplain\r");
    wait_screen_row(&pty, "Plain ·", DEADLINE);
    dismissed(&pty, "Select agent");
    let off = submit(&mut pty, "A first agent restored");
    pty.wait_visible_after(off, "echo: A first agent restored", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());

    let mut pty = PtySession::spawn(fixture.clone(), "scope-a", None);
    pty.wait_visible("Scoped alpha root", DEADLINE);
    pty.send(b"/thinking\r");
    wait_screen_row(&pty, "● none", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A restart current");
    pty.wait_visible_after(off, "echo: A restart current", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/effort\r");
    choose_variant(&mut pty, "Default");
    choose_model(&mut pty, "T39 model");
    choose_model(&mut pty, "T39 alt"); // explicit Default erased the named preference
    wait_screen_row(&pty, "● Default", DEADLINE);
    pty.send(b"\r");
    dismissed(&pty, "Select variant");
    let off = submit(&mut pty, "A cleared preference");
    pty.wait_visible_after(off, "echo: A cleared preference", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/agents\rt39agent\r");
    wait_screen_row(&pty, "T39agent ·", DEADLINE);
    dismissed(&pty, "Select agent");
    let off = submit(&mut pty, "A second agent restart");
    pty.wait_visible_after(off, "echo: A second agent restart", DEADLINE);
    wait_idle(&pty);
    pty.send(b"/continue\rScoped beta root\r");
    dismissed(&pty, "Sessions");
    wait_screen_row(&pty, "echo: B different model", DEADLINE);
    let off = submit(&mut pty, "B restart unchanged");
    pty.wait_visible_after(off, "echo: B restart unchanged", DEADLINE);
    pty.send(b"/quit\r");
    assert!(pty.wait_exit(DEADLINE).0.success() && pty.restored());
    let requests = fixture.wait_requests(10);
    assert_eq!(requests.len(), 10);
    for (i, request) in requests.iter().enumerate() {
        let low = [0, 2, 3, 5, 6].contains(&i);
        assert_eq!(
            request["model"],
            if low || i == 7 { ALT_MODEL } else { MODEL },
            "request {i}"
        );
        assert_eq!(
            request["reasoning"]["effort"],
            if low {
                serde_json::json!("low")
            } else {
                serde_json::Value::Null
            },
            "request {i}"
        );
        assert_eq!(
            request["input"].to_string().contains(AGENT_PROMPT),
            [4, 8].contains(&i),
            "rightful agent {i}"
        );
    }
    // Inherited title selection must follow each new session, too.
    let titles: Vec<_> = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|r| title::is_title(r))
        .cloned()
        .collect();
    assert_eq!(titles.len(), 2);
    assert_eq!(titles[0]["model"], ALT_MODEL);
    assert_eq!(titles[0]["reasoning"]["effort"], "low");
    assert_eq!(titles[1]["model"], MODEL);
    assert!(titles[1]["reasoning"]["effort"].is_null());
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(
        saved_selection(&db, &fixture, "scope-a", "plain"),
        serde_json::json!({"id":ALT_MODEL,"variant":null})
    );
    assert_eq!(
        saved_selection(&db, &fixture, "scope-a", "t39agent")["id"],
        MODEL
    );
    let untouched_key = format!(
        "tui.selection.session:{}",
        serde_json::json!([
            fixture
                .root
                .path()
                .join("project")
                .canonicalize()
                .unwrap()
                .to_string_lossy(),
            "fixture",
            "scope-b"
        ])
    );
    assert!(
        db.get_pref(&untouched_key).unwrap().is_none(),
        "reading an untouched session does not create a preference"
    );
    assert_eq!(
        db.get_pref("tui.selection.variant:[\"fixture\",\"alt-model\"]")
            .unwrap()
            .as_deref(),
        Some("null")
    );
    assert!(
        db.get_pref(oc_core::queries::PREF_MODEL_SELECTION)
            .unwrap()
            .is_none(),
        "scoped TUI never overwrites legacy global preference"
    );
}

/// AUD31: long history and session switches keep the view bounded, and tool
/// cards are paged newest-first (never the oldest 200 only).
#[test]
fn aud31_pty_bounded_backing_state() {
    let fixture = Fixture::new();
    let data_dir = fixture.data_dir();
    let project = fixture.root.path().join("project");
    std::fs::create_dir_all(&project).expect("project");
    seed_session(&data_dir, &project, "s-long39", 3000);
    for i in 0..20 {
        seed_session(&data_dir, &project, &format!("s-other-{i:02}"), 60);
    }
    seed_tool_ops(&data_dir, "s-long39", 260);
    // Persist an actual unique title. The picker orders by real update time,
    // so an ID-based Down shortcut no longer identifies this exact root.
    let conn = rusqlite::Connection::open(data_dir.join("oc.sqlite")).unwrap();
    conn.execute(
        "UPDATE sessions SET title=?1 WHERE id=?2",
        ["Bounded short window zero", "s-other-00"],
    )
    .unwrap();
    drop(conn);
    let metrics = fixture.root.path().join("metrics.json");

    let mut pty = PtySession::spawn(fixture.clone(), "s-long39", Some(&metrics));
    pty.wait_visible(READY, DEADLINE);
    // Page up through real rendering: older pages load and older rows evict.
    for _ in 0..40 {
        pty.send(b"\x1b[<64;1;1M"); // SGR wheel, distinct from editor history Up
        std::thread::sleep(Duration::from_millis(60));
    }
    pty.wait_visible("seeded row", DEADLINE);

    // Tool cards: the newest operation is visible, not the 200 oldest.
    pty.send(b"/cards\r");
    wait_screen_row(&pty, "cards | newest first, up pages older", DEADLINE);
    wait_screen_row(&pty, "tool-0259", DEADLINE);
    pty.send(b"\x1b"); // Esc
    std::thread::sleep(Duration::from_millis(200));

    // Switch sessions: the window is replaced, not accumulated.
    pty.send(b"/sessions\r");
    wait_screen_row(&pty, "Sessions", DEADLINE);
    pty.send(b"Bounded short window zero\r");
    dismissed(&pty, "Sessions");
    pty.wait_visible(READY, DEADLINE);
    pty.send(b"/quit\r");
    let (status, _) = pty.wait_exit(DEADLINE);
    assert!(status.success(), "clean exit");

    let raw = std::fs::read_to_string(&metrics).expect("metrics probe written");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("metrics JSON");
    let rows = value["window_rows"].as_u64().expect("window_rows");
    let bytes = value["retained_bytes"].as_u64().expect("retained_bytes");
    let total = value["window_total"].as_u64().expect("window_total");
    assert!(
        rows <= oc_tui::history::WINDOW_ROWS as u64,
        "window rows bounded: {rows}"
    );
    assert!(
        bytes <= (oc_tui::history::WINDOW_BYTES + oc_tui::app::MAX_INPUT_BYTES) as u64,
        "retained bytes bounded: {bytes}"
    );
    assert!(
        total >= 60,
        "switched session reported its own total: {total}"
    );
    assert_eq!(
        value["session"].as_str(),
        Some("s-other-00"),
        "title search resumed the exact requested root"
    );
}

/// S07: identical current transcript tails under the same PTY workload, with
/// an empty versus a 3000-row older archive. These are process measurements,
/// not estimates derived from the view-model's retained-bytes accounting.
#[test]
fn s07_pty_equal_view_archive_resource_samples() {
    let small = measure_s07(0);
    let large = measure_s07(3000);
    assert_eq!(small.viewport, large.viewport, "active transcript differs");
    assert_eq!(
        small.after_viewport, large.after_viewport,
        "recovered viewport differs"
    );
    assert_eq!(small.request, large.request, "Responses requests differ");
    // T45/R10: this fixture permits no `skill` action, so neither the skill
    // tool nor its automatic preview is advertised (deny is not shown as allow).
    assert!(
        small.request["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .all(|tool| tool["name"] != "skill")
    );
    assert!(
        !small.request["input"]
            .to_string()
            .contains("Available native skills")
    );
    assert_eq!(small.requests.len(), 2, "small: two provider responses");
    assert_eq!(large.requests.len(), 2, "large: two provider responses");
    assert_eq!(
        small.requests, large.requests,
        "full Responses request vectors differ"
    );
    assert_eq!(
        small.request["input"]
            .as_array()
            .expect("Responses input")
            .len(),
        201,
        "200 shared messages, current prompt, and common preamble"
    );
    assert_eq!(
        small.active_view, large.active_view,
        "streamed active view differs"
    );
    assert_eq!(
        small.active_cursor, large.active_cursor,
        "active cursor differs"
    );
    for (label, run) in [("small", &small), ("large", &large)] {
        assert_eq!(
            run.viewport, run.after_viewport,
            "{label}: viewport not restored"
        );
        assert!(
            run.active_view
                .iter()
                .any(|row| row.contains("S07_STREAM_FRAGMENT_42"))
                && run
                    .active_view
                    .iter()
                    .any(|row| row.contains("S07_STREAM_DONE_43")),
            "{label}: incomplete streamed view: {:?}",
            run.active_view
        );
        assert_eq!(last_user_text(&run.request).as_deref(), Some(S07_PROMPT));
        assert_eq!(
            last_user_text(&run.requests[1]).as_deref(),
            Some(S07_PROMPT)
        );
        let outputs = run.requests[1]["input"]
            .as_array()
            .expect("tool output input");
        let read_outputs: Vec<_> = outputs
            .iter()
            .filter(|item| item["type"] == "function_call_output" && item["call_id"] == "call_t39")
            .collect();
        assert_eq!(read_outputs.len(), 1, "{label}: exactly one read result");
        assert!(
            read_outputs[0]["output"].as_str().is_some_and(|text| text
                == format!(
                    "Read file s07-note.txt, lines 1-1\n1: {}",
                    S07_NOTE.trim_end()
                )),
            "{label}: read must return the seeded file: {read_outputs:?}"
        );
        assert_eq!(
            run.tool_ops.len(),
            2,
            "{label}: seeded bash and one real read"
        );
        assert!(
            run.tool_ops
                .iter()
                .any(|(name, state, output)| name == "read"
                    && state == "completed"
                    && output.contains(S07_NOTE.trim_end())),
            "{label}: real read outcome absent: {:?}",
            run.tool_ops
        );
        assert!(
            run.tool_ops
                .iter()
                .all(|(name, _, _)| name == "read" || name == "bash"),
            "{label}: unexpected tool execution: {:?}",
            run.tool_ops
        );
        assert_eq!(run.request["stream"], true);
        assert_eq!(run.request["model"], MODEL);
        let metrics = &run.metrics;
        let rows = metrics["window_rows"].as_u64().expect("window rows");
        let bytes = metrics["retained_bytes"].as_u64().expect("retained bytes");
        assert!(
            rows <= oc_tui::history::WINDOW_ROWS as u64,
            "{label}: {rows}"
        );
        assert!(
            bytes <= (oc_tui::history::WINDOW_BYTES + oc_tui::app::MAX_INPUT_BYTES) as u64,
            "{label}: {bytes}"
        );
        assert_eq!(metrics["window_total"].as_u64(), Some(202 + run.archive));
        let frames = metrics["frame_count"].as_u64().expect("frames");
        let sum = metrics["frame_sum_ns"].as_u64().expect("draw sum");
        let max = metrics["frame_max_ns"].as_u64().expect("draw max");
        let queue_peak = metrics["worker_event_queue_peak"]
            .as_u64()
            .expect("worker event queue peak");
        let queue_lagged = metrics["worker_event_queue_lagged"]
            .as_u64()
            .expect("overwritten worker events");
        let live_text_current = metrics["live_text_bytes_current"]
            .as_u64()
            .expect("live text current");
        let live_text_peak = metrics["live_text_bytes_peak"]
            .as_u64()
            .expect("live text peak");
        let live_reasoning_current = metrics["live_reasoning_bytes_current"]
            .as_u64()
            .expect("live reasoning current");
        let live_reasoning_peak = metrics["live_reasoning_bytes_peak"]
            .as_u64()
            .expect("live reasoning peak");
        let live_parts_current = metrics["live_part_count_current"]
            .as_u64()
            .expect("live parts current");
        let live_parts_peak = metrics["live_part_count_peak"]
            .as_u64()
            .expect("live parts peak");
        let cache_current = metrics["markdown_cache_retained_bytes_current"]
            .as_u64()
            .expect("Markdown cache current");
        let cache_peak = metrics["markdown_cache_retained_bytes_peak"]
            .as_u64()
            .expect("Markdown cache peak");
        assert!(live_text_peak > 0, "{label}: streamed text never sampled");
        assert_eq!(live_text_current, 0, "{label}: live text after exit");
        assert_eq!(
            live_reasoning_current, 0,
            "{label}: live reasoning after exit"
        );
        assert_eq!(live_parts_current, 0, "{label}: live parts after exit");
        assert!(live_reasoning_peak > 0, "{label}: reasoning never sampled");
        assert!(
            live_parts_peak > 0,
            "{label}: frozen reasoning/tool parts never sampled"
        );
        // Each retained route has a 512 KiB styled-block cache and a 2 MiB
        // source-page index; account for the active state plus parked tabs.
        let route_count = metrics["tab_count"].as_u64().expect("tab count") + 1;
        let cache_bound = route_count * (512 * 1024 + 2 * 1024 * 1024);
        assert!(
            cache_current <= cache_peak && cache_peak <= cache_bound,
            "{label}: Markdown cache current={cache_current} peak={cache_peak} bound={cache_bound}"
        );
        assert!(frames > 0 && sum >= max && max > 0);
        println!(
            "S07 {label}: archive={} rss_kb={} pss_kb={} hwm_kb={} cpu_ticks={} child_max={} retained_bytes={} window_rows={} frames={} draw_sum_ns={} draw_max_ns={} worker_event_queue_peak={} worker_event_queue_lagged={} live_text_bytes_current={} live_text_bytes_peak={} live_reasoning_bytes_current={} live_reasoning_bytes_peak={} live_part_count_current={} live_part_count_peak={} markdown_cache_retained_bytes_current={} markdown_cache_retained_bytes_peak={} markdown_cache_bound={} elapsed_ms={}",
            run.archive,
            run.peak_rss_kb,
            run.peak_pss_kb,
            run.peak_hwm_kb,
            run.cpu_ticks,
            run.max_children,
            bytes,
            rows,
            frames,
            sum,
            max,
            queue_peak,
            queue_lagged,
            live_text_current,
            live_text_peak,
            live_reasoning_current,
            live_reasoning_peak,
            live_parts_current,
            live_parts_peak,
            cache_current,
            cache_peak,
            cache_bound,
            run.elapsed.as_millis()
        );
    }
}

/// Same shared process/PTY/view probes with a genuinely committed DCP card and
/// bounded saved-summary queries. The archive setup and actual manual commit
/// precede measurement; reopening/display/stats are the measured hot path.
#[test]
fn vis38_pty_dcp_equal_active_archive_resource_samples() {
    let small = measure_vis38_dcp_display(0);
    let large = measure_vis38_dcp_display(3000);
    assert_eq!(
        small.0, large.0,
        "same active compressed contents/accounting"
    );
    assert!(
        large.1.hwm_kb <= small.1.hwm_kb + 64 * 1024,
        "shared AUD32 64MiB equal-active peak delta"
    );
    assert!(
        large.1.pss_kb <= small.1.pss_kb + 64 * 1024,
        "equal-active PSS delta"
    );
    assert_eq!(
        small.1.threads, large.1.threads,
        "archive does not create OS tasks/threads"
    );
}

fn measure_vis38_dcp_display(archive: usize) -> ((u64, u64, u64), ProcSample) {
    let fixture = Fixture::new();
    let project = fixture.root.path().join("project");
    std::fs::write(project.join("dcp.jsonc"), serde_json::json!({
        "enabled":true,"autoUpdate":false,"pruneNotification":"detailed","pruneNotificationType":"chat",
        "compress":{"mode":"range","permission":"allow","showCompression":true,"minContextLimit":1000000,"maxContextLimit":1000000},
        "strategies":{"deduplication":{"enabled":false},"purgeErrors":{"enabled":false}}
    }).to_string()).unwrap();
    let config = fixture
        .root
        .path()
        .join("home/config/opencode/opencode.json");
    let mut settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    settings["provider"]["fixture"]["models"][MODEL]["limit"]["context"] =
        serde_json::json!(262144);
    std::fs::write(config, settings.to_string()).unwrap();
    let session = "s-vis38-resource";
    seed_session(&fixture.data_dir(), &project, session, 0);
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    db.apply_dcp_schema().unwrap();
    let mut end = None;
    for i in 0..archive {
        end = Some(
            db.append_message(
                session,
                if i.is_multiple_of(2) {
                    "user"
                } else {
                    "assistant"
                },
                &format!("archive-{i:04} {}", "x".repeat(16 * 1024)),
            )
            .unwrap(),
        );
        // Real bounded prune commits, not a synthetic mark that bypasses the
        // owner's active-input cap for this deliberately 48MiB raw archive.
        if (i + 1) % 64 == 0 {
            db.save_prune_mark(session, end.as_ref().unwrap()).unwrap();
        }
    }
    if let Some(id) = end {
        db.save_prune_mark(session, &id).unwrap();
    }
    for i in 0..4 {
        db.append_message(
            session,
            if i % 2 == 0 { "user" } else { "assistant" },
            &format!("active-{i} {}", "closed requirement ".repeat(1000)),
        )
        .unwrap();
    }
    drop(db);
    let mut prepare = PtySession::spawn_sized(fixture.clone(), session, None, 120, 40);
    prepare.wait_visible(READY, DEADLINE);
    prepare.send(format!("/dcp-compress {VIS38_RESOURCE_FOCUS}\r").as_bytes());
    fixture.wait_requests(2);
    wait_screen_row(&prepare, "compressions 1", DEADLINE);
    prepare.send(b"\x1b");
    wait_screen_row(&prepare, "answer:compressed", DEADLINE);
    wait_idle(&prepare);
    // This is a resource probe, not a Ctrl+C lifecycle assertion. A second
    // control byte can become SIGINT after the first restores terminal modes.
    // Use the same explicit teardown as neighboring resource probes.
    prepare.send(b"/quit\r");
    let (status, _) = prepare.wait_exit(DEADLINE);
    assert!(
        status.success() && prepare.restored(),
        "prepare exit={status:?}; restored={}",
        prepare.restored()
    );
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    let ops = db.list_tool_ops(session).unwrap();
    assert_eq!(ops.len(), 1);
    let run = db
        .dcp_run(session, &ops[0].op)
        .unwrap()
        .expect("actual committed resource operation");
    // The newly accepted manual turn is unfinished. All four prior legacy
    // anchors are closed and advertised without duplicating the current prompt.
    assert_eq!(
        (run.new_messages, run.block_ids.len(), run.ordinal),
        (4, 1, 1)
    );
    let raw_before = db.read_history_full(session).unwrap();
    drop(db);
    let request_count = fixture.requests.lock().unwrap().len();
    let metrics_path = fixture.root.path().join("vis38-resource-metrics.json");
    let mut pty = PtySession::spawn_sized(fixture.clone(), session, Some(&metrics_path), 120, 40);
    wait_screen_row(&pty, "Compression #1", DEADLINE);
    wait_screen_row(&pty, "summary preview limited", DEADLINE);
    let pid = pty.child.id();
    let mut peak = s07_proc_sample(pid);
    let baseline_cpu = peak.cpu_ticks;
    let began = Instant::now();
    let mut sample = |pty: &PtySession| {
        let now = s07_proc_sample(pty.child.id());
        peak.rss_kb = peak.rss_kb.max(now.rss_kb);
        peak.pss_kb = peak.pss_kb.max(now.pss_kb);
        peak.hwm_kb = peak.hwm_kb.max(now.hwm_kb);
        peak.children = peak.children.max(now.children);
        peak.threads = peak.threads.max(now.threads);
        peak.cpu_ticks = now.cpu_ticks;
    };
    pty.send(b"/dcp\r");
    wait_screen_row(&pty, "compressions 1", DEADLINE);
    sample(&pty);
    pty.send(b"\x1b");
    for (cols, rows) in [(80, 24), (160, 48), (120, 40)] {
        let offset = pty.snapshot().len();
        pty.resize(cols, rows);
        // At 80x24 the long preview pushes its header above the viewport; wait
        // for the real preview tail, not a header that is honestly offscreen.
        pty.wait_visible_after(offset, "summary", DEADLINE);
        wait_screen_row(&pty, "summary preview limited", DEADLINE);
        sample(&pty);
    }
    pty.send(b"\x03");
    std::thread::sleep(POLL);
    pty.send(b"\x03");
    let (status, output) = pty.wait_exit(DEADLINE);
    assert!(status.success() && pty.restored() && contains(&output, ALT_LEAVE));
    assert!(
        !Path::new(&format!("/proc/{pid}")).exists(),
        "measured child and OS tasks reaped"
    );
    assert_eq!(peak.children, 0);
    assert_eq!(
        fixture.requests.lock().unwrap().len(),
        request_count,
        "reopen/summary/stats/resize do not generate"
    );
    let metrics: serde_json::Value =
        serde_json::from_slice(&std::fs::read(metrics_path).unwrap()).unwrap();
    assert!(metrics["window_rows"].as_u64().unwrap() <= oc_tui::history::WINDOW_ROWS as u64);
    assert!(
        metrics["retained_bytes"].as_u64().unwrap()
            <= (oc_tui::history::WINDOW_BYTES + oc_tui::app::MAX_INPUT_BYTES) as u64
    );
    let route_count = metrics["tab_count"].as_u64().unwrap() + 1;
    assert!(
        metrics["markdown_cache_retained_bytes_peak"]
            .as_u64()
            .unwrap()
            <= route_count * (512 * 1024 + 2 * 1024 * 1024)
    );
    assert_eq!(metrics["worker_event_queue_lagged"].as_u64(), Some(0));
    assert!(metrics["worker_event_queue_peak"].as_u64().unwrap() <= 256);
    assert_eq!(metrics["live_text_bytes_current"].as_u64(), Some(0));
    assert_eq!(metrics["live_part_count_current"].as_u64(), Some(0));
    let db = oc_adapters::storage::Db::open(&fixture.data_dir()).unwrap();
    assert_eq!(db.read_history_full(session).unwrap(), raw_before);
    assert_eq!(
        db.dcp_run(session, &run.operation_id).unwrap(),
        Some(run.clone())
    );
    println!(
        "VIS38 S07 DCP archive={archive} archive_source_bytes={} rss_kb={} pss_kb={} hwm_kb={} os_tasks_threads={} children={} cpu_ticks={} elapsed_ms={} retained_bytes={} window_rows={} queue_peak={} queue_lagged={} markdown_cache_peak={} frames={} draw_max_ns={} replay_requests=0 actual_runs=1 summary_page_limit={}",
        archive * 16 * 1024,
        peak.rss_kb,
        peak.pss_kb,
        peak.hwm_kb,
        peak.threads,
        peak.children,
        peak.cpu_ticks.saturating_sub(baseline_cpu),
        began.elapsed().as_millis(),
        metrics["retained_bytes"],
        metrics["window_rows"],
        metrics["worker_event_queue_peak"],
        metrics["worker_event_queue_lagged"],
        metrics["markdown_cache_retained_bytes_peak"],
        metrics["frame_count"],
        metrics["frame_max_ns"],
        oc_tui::dcp_view::SUMMARY_PAGE_BYTES
    );
    (
        (run.removed, run.summary, run.cumulative.active_summary),
        peak,
    )
}
