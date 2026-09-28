use super::*;

#[tokio::test]
async fn close_tab_chord_uses_the_presented_active_slot_without_mutating_the_deck() {
    let mut state = fresh_state("close-chord").await;
    let tabs = vec![
        TabPresentation {
            title: Some("Old".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("old"))
        },
        TabPresentation {
            title: Some("Current".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("current"))
        },
    ];
    state.set_tab_strip(tabs.clone(), 1, true);
    type_text(&mut state, "draft").await;
    assert_eq!(
        state.handle_key(KeyAction::Leader).await,
        KeyOutcome::default()
    );
    assert_eq!(
        state.handle_key(KeyAction::Char('w')).await,
        KeyOutcome {
            intent: Some(PanelIntent::CloseTab { index: 1 }),
            ..KeyOutcome::default()
        }
    );
    assert_eq!(state.tab_presentation().0, tabs);
    assert_eq!(state.input(), "draft");
    assert_eq!(state.panel(), &TuiPanel::None);
}

#[tokio::test]
async fn vis39_tab_only_ticks_reuse_one_warm_viewport_without_copying_retained_rows() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut state = fresh_state("tab-projection-cost").await;
    let body = "durable offscreen text line\n".repeat(240);
    let mut messages: Vec<_> = (1..=32)
        .map(|seq| msg(seq, Role::Assistant, &body))
        .collect();
    let mut last = msg(33, Role::Assistant, "visible durable tail");
    last.turn = Some(oc_core::queries::HistoryTurn {
        model_label: "measured".into(),
        usage: Some((100, 50)),
        streamed_ms: Some(1000),
        ..Default::default()
    });
    messages.push(last);
    state.attach_page(&page(messages, 33, false, false));
    assert!(state.window.retained_bytes() > 150_000);
    let tabs = vec![
        TabPresentation::new(sid("current")),
        TabPresentation {
            busy: true,
            ..TabPresentation::new(sid("running-other"))
        },
    ];
    state.set_tab_strip(tabs, 0, false);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    let copies = state.transcript_row_copies.get();
    let builds = state.visible_projection_builds.get();
    let retained = state.visible_projection.borrow();
    let view = retained.as_ref().unwrap();
    assert!(view.total > 1000 && view.lines.len() <= 24);
    assert!(
        view.lines
            .iter()
            .any(|line| line.plain_text().contains("50.0 tok/s"))
    );
    let expected = view.lines.clone();
    drop(retained);
    let at = Instant::now();
    for frame in 1..=60 {
        assert!(state.tick_tabs(at + Duration::from_millis(frame * 80)));
        terminal
            .draw(|frame| crate::shell::render(frame, &state))
            .unwrap();
    }
    assert_eq!(
        state.transcript_row_copies.get(),
        copies,
        "tab clocks cannot deep-copy offscreen durable rows"
    );
    assert_eq!(
        state.visible_projection_builds.get(),
        builds,
        "tab clocks cannot re-index an unchanged viewport"
    );
    assert_eq!(
        state.visible_projection.borrow().as_ref().unwrap().lines,
        expected
    );

    state.chrome.session_tps = Some(false);
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert!(
        !state
            .visible_projection
            .borrow()
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .any(|line| line.plain_text().contains("tok/s"))
    );
    assert_eq!(state.visible_projection_builds.get(), builds + 1);
    let turn = WorkerTurnId("new-content".into());
    state.begin_compress_turn(turn.clone());
    state.apply_delta(&turn, "fresh live text");
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert!(
        state
            .visible_projection
            .borrow()
            .as_ref()
            .unwrap()
            .lines
            .iter()
            .any(|line| line.plain_text().contains("fresh live text"))
    );
    terminal.resize(Rect::new(0, 0, 120, 40)).unwrap();
    terminal
        .draw(|frame| crate::shell::render(frame, &state))
        .unwrap();
    assert!(
        state
            .visible_projection
            .borrow()
            .as_ref()
            .unwrap()
            .lines
            .len()
            <= 40
    );
}

#[tokio::test]
async fn vis41_activation_and_height_resize_preserve_same_id_including_completed_cycle() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut previous = TuiState::new(app.clone(), sid("first"));
    previous.chrome.animations = Some(false);
    previous.chrome.location = Some("/real-project".into());
    let tabs: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|name| TabPresentation {
            title: Some("opencode native long session title".into()),
            ..TabPresentation::new(sid(name))
        })
        .collect();
    let at = Instant::now();
    let area = Rect::new(0, 0, 80, 24);
    previous.set_tab_strip_at(tabs.clone(), 0, false, at);
    tab_pointer_at(&mut previous, area, 1, at);
    previous.tick_tabs(at + Duration::from_millis(680));
    let sample = previous.tab_animation(1);
    assert_eq!(sample.0, 2);
    let deadline = previous.next_tab_deadline();
    previous.close_panel(); // no modal was mounted, hence no source leave
    assert_eq!(previous.tab_animation(1), sample);
    let pointer = previous.mouse_position().unwrap();
    let mut next = TuiState::new(app, sid("second"));
    next.chrome = previous.chrome.clone();
    next.take_tab_clocks_from(&mut previous);
    next.set_tab_strip_at(tabs, 1, false, at + Duration::from_millis(700));
    next.restore_mouse_hover(pointer);
    assert_eq!(next.tab_animation(1), sample);
    assert_eq!(next.next_tab_deadline(), deadline);
    assert_eq!(previous.next_tab_deadline(), None);
    let taller = Rect::new(0, 0, 80, 40);
    next.resize_mouse_position(taller);
    assert_eq!(next.hovered_tab(taller), Some(1));
    assert_eq!(next.tab_animation(1), sample);
    assert_eq!(next.next_tab_deadline(), deadline);
    next.tick_tabs(at + Duration::from_secs(5));
    assert_eq!(next.tab_animation(1).0, 0);
    assert!(next.tab_view.borrow().marquee.as_ref().unwrap().done);
    assert_eq!(next.next_tab_deadline(), None);
    next.close_panel();
    next.restore_mouse_hover(next.mouse_position().unwrap());
    next.resize_mouse_position(area);
    assert!(next.tab_view.borrow().marquee.as_ref().unwrap().done);
    assert_eq!(
        next.next_tab_deadline(),
        None,
        "same-ID activation/height resize cannot start a second cycle"
    );

    // A real visibility/layout change removes the hovered root, cancelling all marquee clocks.
    next.set_tab_strip_at(
        (0..10)
            .map(|index| TabPresentation {
                title: Some("a long retained tab title".into()),
                ..TabPresentation::new(sid(&format!("root-{index}")))
            })
            .collect(),
        0,
        false,
        at + Duration::from_secs(6),
    );
    tab_pointer_at(&mut next, area, 1, at + Duration::from_secs(6));
    let narrow = Rect::new(0, 0, 8, 24);
    assert!(
        !crate::shell::tab_strip(&next, narrow)
            .unwrap()
            .tabs
            .iter()
            .any(|tab| tab.index == 1)
    );
    next.resize_mouse_position(narrow);
    assert!(next.tab_view.borrow().marquee.is_none());
    assert_eq!(next.next_tab_deadline(), None);
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn vis39_tab_spinner_mount_clocks_survive_updates_and_attention_supersedes_running() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("first"));
    let area = Rect::new(0, 0, 80, 24);
    let now = Instant::now();
    let mut first = TabPresentation::new(sid("first"));
    first.title = Some("same title".into());
    first.busy = true;
    let mut second = TabPresentation::new(sid("second"));
    second.title = first.title.clone();
    state.set_tab_strip_at(vec![first.clone(), second.clone()], 0, false, now);
    state.prepare_tabs(area, now);
    assert_eq!(state.next_ui_deadline(), Some(now + super::TAB_FADE_FRAME));
    second.busy = true;
    state.set_tab_strip_at(
        vec![first.clone(), second.clone()],
        0,
        false,
        now + Duration::from_millis(30),
    );
    assert!(state.tick_ui(now + TAB_STEP + Duration::from_micros(500)));
    assert_eq!((state.tab_animation(0).2, state.tab_animation(1).2), (1, 0));
    // Reordering equal titles uses session IDs; neither mounted phase resets.
    state.set_tab_strip_at(
        vec![second.clone(), first.clone()],
        1,
        false,
        now + Duration::from_millis(90),
    );
    assert!(state.tick_ui(now + Duration::from_millis(110)));
    assert_eq!((state.tab_animation(0).2, state.tab_animation(1).2), (1, 1));
    assert_eq!(
        state
            .tab_view
            .borrow()
            .motions
            .iter()
            .find(|motion| motion.id == super::TabIdentity::Session(sid("first")))
            .unwrap()
            .at
            .map(|at| at + TAB_STEP),
        Some(now + TAB_STEP * 2),
        "late fractional ticks cannot drift the mount clock"
    );
    for frame in 2..10 {
        state.tick_ui(now + TAB_STEP * frame);
        assert_eq!(state.tab_animation(1).2, frame as usize);
    }
    first.attention = Some(super::TabAttention::Question);
    second.busy = false;
    state.tab_attention.insert(1); // real permission-root projection wins question
    state.set_tab_strip_at(
        vec![second, first.clone()],
        1,
        false,
        now + Duration::from_secs(1),
    );
    assert_eq!(
        state.tab_attention_for(1),
        Some(super::TabAttention::Permission)
    );
    assert!(
        state.next_ui_deadline().is_some(),
        "release/ignition are finite visible phases"
    );
    state.tick_ui(now + Duration::from_millis(1900));
    assert_eq!(
        state.next_ui_deadline(),
        None,
        "static attention does not poll"
    );
    state.tab_attention.clear();
    assert_eq!(
        state.tab_attention_for(1),
        Some(super::TabAttention::Question)
    );
    first.attention = None;
    state.chrome.animations = Some(false);
    state.set_tab_strip_at(vec![first.clone()], 0, false, now + Duration::from_secs(2));
    assert_eq!(state.tab_animation(0).2, 0);
    assert_eq!(state.next_ui_deadline(), None);
    state.chrome.animations = Some(true);
    state.chrome.tab_indicators = oc_core::queries::TabIndicators::Numbers;
    state.prepare_tabs(area, now + Duration::from_secs(3));
    assert!(
        state.next_ui_deadline().is_some(),
        "numbers hide the spinner, not the running sweep"
    );
    assert!(
        state
            .tab_view
            .borrow()
            .motions
            .iter()
            .all(|motion| motion.at.is_none())
    );
    assert!(
        inbox.try_recv().is_err(),
        "presentation never submits or selects"
    );
}

#[test]
fn vis39_u48_running_release_completion_and_flash_use_source_clocks() {
    use super::{TabPulse, TabPulseTarget};
    let at = Instant::now();
    let mut target = TabPulseTarget {
        animations: true,
        ..Default::default()
    };
    let mut pulse = TabPulse::new(target, at);
    assert_eq!(pulse.deadline(), None);
    target.runs = true;
    pulse.sync(target, at);
    assert_eq!(pulse.frame().whitecap, 0.85);
    pulse.tick(at + Duration::from_millis(80));
    assert!((pulse.frame().flash - 0.1).abs() < 0.00001);
    pulse.tick(at + Duration::from_millis(225));
    assert!((pulse.frame().running - 0.5).abs() < 0.00001);
    pulse.tick(at + Duration::from_millis(350));
    assert!((pulse.frame().whitecap - 0.425).abs() < 0.00001);
    pulse.tick(at + Duration::from_millis(450));
    assert_eq!(pulse.frame().running, 1.0);
    pulse.tick(at + Duration::from_millis(1400));
    let halfway = pulse.frame();
    assert_eq!(halfway.sweep_clock, 1.4);
    // The second front only enters after the first half-cycle. Both travel
    // with coast(), not linear interpolation or a globally shared phase.
    assert!((halfway.sweep(22.5, 32) - 1.0).abs() < 0.00001);
    assert_eq!(halfway.sweep(0.0, 32), 0.0);
    pulse.tick(at + Duration::from_millis(2100));
    assert!(
        pulse.frame().sweep(1.0, 32) > 0.0,
        "the half-cycle-offset second front is present"
    );

    let off = at + Duration::from_millis(2100);
    target.runs = false;
    pulse.sync(target, off);
    pulse.tick(off + Duration::from_millis(250));
    assert!((pulse.frame().running - 0.5).abs() < 0.00001);
    assert!(
        (pulse.frame().sweep_clock - 2.35).abs() < 0.00001,
        "sweep coasts through release"
    );
    assert_eq!(
        pulse.frame().completion,
        0.0,
        "idle/cancel is not unread activity"
    );
    target.complete = true; // Actual U47 source input, distinct from idle.
    let completed = off + Duration::from_millis(250);
    pulse.sync(target, completed);
    for (milliseconds, expected) in [(72, 0.09), (144, 0.18), (672, 0.09), (1200, 0.0)] {
        pulse.tick(completed + Duration::from_millis(milliseconds));
        assert!(
            (pulse.frame().completion - expected).abs() < 0.00001,
            "{milliseconds}"
        );
    }
    assert_eq!(pulse.deadline(), None);
    assert!(!pulse.tick(completed + Duration::from_secs(60)));

    // No delayed activity flash once the release window has ended.
    target.complete = false;
    target.runs = true;
    pulse.sync(target, completed + Duration::from_secs(61));
    target.runs = false;
    pulse.sync(target, completed + Duration::from_secs(62));
    pulse.tick(completed + Duration::from_secs(63));
    target.complete = true;
    pulse.sync(target, completed + Duration::from_secs(64));
    assert_eq!(pulse.frame().completion, 0.0);
    assert_eq!(pulse.deadline(), None);
}

#[test]
fn vis39_u48_glow_ignition_diffusion_and_off_mode_settle() {
    use super::{TabPulse, TabPulseTarget};
    let at = Instant::now();
    let mut target = TabPulseTarget {
        animations: true,
        vertical: true,
        ..Default::default()
    };
    let mut pulse = TabPulse::new(target, at);
    target.glows = true;
    target.dimmed = true;
    pulse.sync(target, at);
    pulse.tick(at + Duration::from_millis(100));
    assert!((pulse.frame().dim - 0.85).abs() < 0.00001);
    pulse.tick(at + Duration::from_millis(180));
    assert_eq!(pulse.frame().glow, 1.5);
    target.glows = false;
    target.dimmed = false;
    let released = at + Duration::from_millis(180);
    pulse.sync(target, released);
    pulse.tick(released + Duration::from_millis(100));
    assert!(
        (pulse.frame().glow - 0.75).abs() < 0.00001,
        "release captures ignition's 1.5 level"
    );
    pulse.tick(released + Duration::from_millis(108));
    assert!((pulse.frame().swell - 1.875).abs() < 0.00001);
    pulse.tick(released + Duration::from_millis(200));
    assert_eq!(pulse.frame().glow, 0.0, "resting residue drains in 200 ms");
    pulse.tick(released + Duration::from_millis(450));
    assert!(
        pulse.frame().glow_at(13.0, 32, 12.0) > 0.0,
        "release diffuses beyond the resting tail"
    );
    pulse.tick(released + Duration::from_millis(900));
    assert_eq!(pulse.deadline(), None);
    assert_eq!(pulse.frame().release, None);
    assert_eq!(pulse.frame().title_glow, 0.0);
    assert_eq!(pulse.frame().number_glow, 0.0);
    assert!(!pulse.tick(released + Duration::from_secs(60)));

    target.glows = true;
    pulse.sync(target, released + Duration::from_secs(61));
    pulse.tick(released + Duration::from_millis(61600));
    assert_eq!(pulse.frame().glow, 1.0);
    assert_eq!(pulse.deadline(), None, "sustain is motionless");
    target.animations = false;
    target.runs = true;
    pulse.sync(target, released + Duration::from_secs(62));
    assert_eq!(pulse.frame().running, 0.0);
    assert_eq!(pulse.frame().glow, 1.0);
    assert_eq!(pulse.frame().whitecap, 0.0);
    assert_eq!(pulse.deadline(), None);
    target.glows = false;
    pulse.sync(target, released + Duration::from_secs(63));
    assert_eq!(pulse.frame().glow, 0.0);
    assert_eq!(pulse.frame().release, None);
}

#[test]
fn vis39_u48_retrigger_keeps_flash_and_reenable_keeps_sweep_position() {
    use super::{TabPulse, TabPulseTarget};
    let at = Instant::now();
    let mut target = TabPulseTarget {
        animations: true,
        ..Default::default()
    };
    let mut pulse = TabPulse::new(target, at);
    target.runs = true;
    pulse.sync(target, at);
    pulse.tick(at + Duration::from_millis(100));
    let release_level = pulse.frame().running;
    target.runs = false;
    pulse.sync(target, at + Duration::from_millis(100));
    assert_eq!(
        pulse.flash,
        Some(Duration::from_millis(100)),
        "edge start does not restart"
    );
    pulse.tick(at + Duration::from_millis(350));
    assert!((pulse.frame().running - release_level * 0.5).abs() < 0.00001);
    target.prompt = 1;
    pulse.sync(target, at + Duration::from_millis(350));
    pulse.tick(at + Duration::from_millis(430));
    assert!(
        (pulse.frame().flash - 0.2).abs() < 0.00001,
        "prompt restarts at double scale"
    );
    target.runs = true;
    pulse.sync(target, at + Duration::from_millis(430));
    assert_eq!(pulse.frame().sweep_clock, 0.0, "note-on resets the sweep");
    assert_eq!(pulse.flash, Some(Duration::from_millis(80)));
    pulse.tick(at + Duration::from_millis(930));
    let position = pulse.frame().sweep_clock;
    target.animations = false;
    pulse.sync(target, at + Duration::from_millis(930));
    assert_eq!(pulse.deadline(), None);
    target.animations = true;
    pulse.sync(target, at + Duration::from_millis(2000));
    assert_eq!(pulse.frame().sweep_clock, position);
    pulse.tick(at + Duration::from_millis(2225));
    assert!((pulse.frame().running - 0.5).abs() < 0.00001);
    assert!((pulse.frame().sweep_clock - position - 0.225).abs() < 0.00001);
}

#[tokio::test]
async fn vis39_visible_deck_disposes_hidden_phases_without_fabricating_completion() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("current"));
    let at = Instant::now();
    let area = Rect::new(0, 0, 80, 24);
    let mut running = TabPresentation::new(sid("running"));
    running.busy = true;
    let current = TabPresentation::new(sid("current"));
    state.set_tab_strip_at(vec![running.clone(), current.clone()], 1, false, at);
    state.prepare_tabs(area, at);
    state.tick_ui(at + Duration::from_millis(450));
    assert_eq!(state.tab_pulse(0).running, 1.0);
    state.prepare_tabs(Rect::new(0, 0, 8, 24), at + Duration::from_millis(500));
    assert_eq!(state.tab_view.borrow().motions.len(), 1);
    assert_eq!(state.next_ui_deadline(), None, "hidden tabs own no clocks");
    let remounted = at + Duration::from_secs(1);
    state.prepare_tabs(area, remounted);
    assert_eq!(state.tab_animation(0).2, 0);
    assert_eq!(state.tab_pulse(0).running, 0.0);
    assert_eq!(
        state.tab_pulse(0).flash,
        0.0,
        "mount does not ignite an edge flash"
    );
    running.complete = true;
    running.attention = Some(super::TabAttention::Permission);
    state.set_tab_strip_at(vec![running.clone(), current.clone()], 1, false, remounted);
    assert!(!state.tab_pulse(0).complete);
    assert_eq!(state.tab_pulse(0).completion, 0.0);
    state.tick_ui(remounted + Duration::from_millis(900));
    assert_eq!(state.next_ui_deadline(), None);
    running.busy = false;
    state.set_tab_strip_at(
        vec![running, current],
        1,
        false,
        remounted + Duration::from_secs(1),
    );
    assert!(
        state.tab_pulse(0).complete,
        "the actual activity input remains independent of attention"
    );
    assert_eq!(
        state.tab_pulse(0).completion,
        0.0,
        "an expired release cannot manufacture a delayed completion pulse"
    );
    assert_eq!(
        state.tab_attention_for(0),
        Some(super::TabAttention::Permission)
    );
    state.reset_workspace();
    assert!(state.tab_view.borrow().motions.is_empty());
    assert_eq!(state.next_ui_deadline(), None);
    assert!(
        inbox.try_recv().is_err(),
        "all phases are presentation-only"
    );
}

#[tokio::test]
async fn vis41_one_cycle_off_mode_repeated_pointer_and_source_metadata_rules() {
    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("first"));
    state.chrome.animations = Some(false);
    let area = Rect::new(0, 0, 11, 24); // resting width8, hover width6
    let now = Instant::now();
    let mut tab = TabPresentation::new(sid("first"));
    tab.title = Some("opencode".into());
    state.set_tab_strip_at(vec![tab.clone()], 0, false, now);
    tab_pointer_at(&mut state, area, 0, now);
    assert_eq!(state.next_ui_deadline(), Some(now + TAB_MARQUEE_DELAY));
    tab_pointer_at(&mut state, area, 0, now + Duration::from_millis(500));
    assert!(!state.tick_ui(now + Duration::from_millis(599)));
    assert!(state.tick_ui(now + TAB_MARQUEE_DELAY));
    assert_eq!(state.tab_animation(0), (1, 1.0, 0));
    // U56 captures cycle width on enter. Current title and geometry stay live
    // without restarting the clock during owner updates or paints.
    tab.title = Some("a different long title".into());
    state.set_tab_strip_at(vec![tab], 0, false, now + Duration::from_millis(640));
    state.prepare_tabs(area, now + Duration::from_millis(650));
    assert_eq!(
        state.next_ui_deadline(),
        Some(now + Duration::from_millis(680))
    );
    assert!(state.tick_ui(now + Duration::from_millis(680)));
    assert_eq!(state.tab_animation(0).0, 2);
    assert!(state.tick_ui(now + Duration::from_millis(1400)));
    assert_eq!(state.tab_animation(0), (0, 0.0, 0));
    tab_pointer_at(&mut state, area, 0, now + Duration::from_secs(3));
    assert_eq!(
        state.next_ui_deadline(),
        None,
        "completed hover cannot re-enter"
    );
    assert!(!state.tick_ui(now + Duration::from_secs(60)));
    let mut short = TabPresentation::new(sid("second"));
    short.title = Some("short".into());
    state.set_tab_strip_at(vec![short], 0, false, now + Duration::from_secs(61));
    assert_eq!(
        state.hovered_tab(area),
        None,
        "identity replacement cannot inherit hover"
    );
    tab_pointer_at(&mut state, area, 0, now + Duration::from_secs(62));
    assert!(state.tab_view.borrow().marquee.is_none());
    assert!(inbox.try_recv().is_err());
}

#[tokio::test]
async fn vis41_leading_smoothstep_and_deferred_leave_settle_without_periodic_wakes() {
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new(app, sid("first"));
    let area = Rect::new(0, 0, 11, 24);
    let now = Instant::now();
    let mut tab = TabPresentation::new(sid("first"));
    tab.title = Some("opencode".into());
    state.set_tab_strip_at(vec![tab.clone()], 0, false, now);
    tab_pointer_at(&mut state, area, 0, now);
    for (ms, leading) in [
        (600, 0.0),
        (725, 0.5),
        (850, 1.0),
        (1400, 1.0),
        (1525, 0.5),
        (1650, 0.0),
    ] {
        state.tick_ui(now + Duration::from_millis(ms));
        assert!((state.tab_animation(0).1 - leading).abs() < 0.0001, "{ms}");
    }
    assert_eq!(state.next_ui_deadline(), None);
    state.enter_tab_at(area, 10, 1, now + Duration::from_secs(2));
    // A close-cell/nested control enter in the same event burst cancels leave.
    tab_pointer_at(&mut state, area, 0, now + Duration::from_secs(2));
    assert_eq!(state.next_ui_deadline(), None);
    state.enter_tab_at(area, 10, 1, now + Duration::from_secs(3));
    assert!(state.tick_ui(now + Duration::from_secs(3)));
    assert!(state.tab_view.borrow().marquee.is_none());
    tab_pointer_at(&mut state, area, 0, now + Duration::from_secs(4));
    assert_eq!(
        state.next_ui_deadline(),
        Some(now + Duration::from_millis(4600))
    );
    state.run_command(crate::commands::CommandAction::OpenCommands);
    assert_eq!(state.next_ui_deadline(), None);
    state.close_panel();
    tab_pointer_at(&mut state, area, 0, now + Duration::from_secs(5));
    let mut hidden = TabPresentation::new(sid("other"));
    hidden.title = tab.title.clone();
    state.set_tab_strip_at(vec![tab, hidden], 1, false, now + Duration::from_secs(5));
    state.prepare_tabs(Rect::new(0, 0, 8, 24), now + Duration::from_secs(5));
    assert!(state.tab_view.borrow().marquee.is_none());
    state.reset_workspace();
    assert_eq!(state.next_ui_deadline(), None);
    assert!(state.tab_view.borrow().motions.is_empty());
}

#[tokio::test]
async fn close_tab_palette_selection_preserves_modal_and_draft_until_owner_accepts() {
    let mut state = fresh_state("close-palette").await;
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Only tab".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("only"))
        }],
        0,
        true,
    );
    type_text(&mut state, "unsent draft").await;
    state.handle_key(KeyAction::Commands).await;
    state.handle_paste("Close tab");
    let options = state.modal_options();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].value, "session.tab.close");
    assert_eq!(options[0].footer, "ctrl+x w");
    assert_eq!(
        state.handle_panel_key(KeyAction::Enter),
        KeyOutcome {
            intent: Some(PanelIntent::CloseTab { index: 0 }),
            ..KeyOutcome::default()
        }
    );
    assert_eq!(state.panel(), &TuiPanel::Commands);
    assert_eq!(state.select.query, "Close tab");
    assert_eq!(state.input(), "unsent draft");
    assert_eq!(state.tab_presentation().0.len(), 1);
}

#[tokio::test]
async fn close_tab_home_last_and_busy_availability() {
    use crate::commands::CommandAction;
    use oc_core::core_app::InboxMsg;

    let (app, _, _) = CoreApp::channel(4);
    let mut home = TuiState::new_home(app);
    assert_eq!(
        home.command_unavailable(&CommandAction::CloseTab),
        Some("no tab to close")
    );
    home.handle_key(KeyAction::Commands).await;
    home.handle_paste("Close tab");
    assert!(home.modal_options().is_empty());
    home.handle_panel_key(KeyAction::Cancel);
    home.handle_key(KeyAction::Leader).await;
    let bare = home.handle_key(KeyAction::Char('w')).await;
    assert_eq!(bare.intent, None);
    assert_eq!(bare.note.as_deref(), Some("no tab to close"));
    home.set_tab_strip(
        vec![TabPresentation {
            title: Some("Old".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("old"))
        }],
        0,
        false,
    );
    assert_eq!(home.command_unavailable(&CommandAction::CloseTab), None);
    assert_eq!(home.handle_key(KeyAction::Leader).await.intent, None);
    assert_eq!(
        home.handle_key(KeyAction::Char('w')).await.intent,
        Some(PanelIntent::CloseTab { index: 1 })
    );
    assert!(home.home);
    assert_eq!(home.tab_presentation().0.len(), 1);

    let (app, mut inbox, _) = CoreApp::channel(4);
    let mut last = TuiState::new(app, sid("close-last"));
    assert_eq!(
        last.command_unavailable(&CommandAction::CloseTab),
        Some("no tab to close")
    );
    last.set_tab_strip(
        vec![TabPresentation {
            title: Some("Last".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("last"))
        }],
        0,
        false,
    );
    assert_eq!(
        last.run_command(CommandAction::CloseTab).intent,
        Some(PanelIntent::CloseTab { index: 0 })
    );
    assert_eq!(last.tab_presentation().0.len(), 1);

    last.set_tab_strip(
        vec![TabPresentation {
            title: Some("Busy tab".into()),
            home: false,
            busy: true,
            ..TabPresentation::new(sid("busy"))
        }],
        0,
        false,
    );
    assert_eq!(
        last.command_unavailable(&CommandAction::CloseTab),
        Some("tab busy; action unavailable")
    );
    assert_eq!(last.run_command(CommandAction::CloseTab).intent, None);
    last.set_tab_strip(
        vec![TabPresentation {
            title: Some("Last".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("last"))
        }],
        0,
        false,
    );

    type_text(&mut last, "pending draft").await;
    last.handle_key(KeyAction::Enter).await;
    let Some(InboxMsg::Submit { ack, .. }) = inbox.recv().await else {
        panic!("submit")
    };
    last.handle_key(KeyAction::Commands).await;
    last.handle_paste("Close tab");
    assert!(last.modal_options()[0].footer.contains("turn active"));
    let refused = last.handle_panel_key(KeyAction::Enter);
    assert_eq!(refused.intent, None);
    assert_eq!(
        refused.note.as_deref(),
        Some("turn active; action unavailable")
    );
    assert_eq!(last.panel(), &TuiPanel::Commands);
    assert_eq!(last.input(), "pending draft");
    last.handle_panel_key(KeyAction::Cancel);
    assert_eq!(last.run_command(CommandAction::CloseTab).intent, None);
    assert!(inbox.try_recv().is_err());
    drop(ack);

    let (app, _, _) = CoreApp::channel(4);
    let mut bare = TuiState::new_home(app);
    type_text(&mut bare, "/close-tab").await;
    let refused = bare.handle_key(KeyAction::Enter).await;
    assert_eq!(refused.intent, None);
    assert_eq!(refused.note.as_deref(), Some("no tab to close"));
    assert_eq!(bare.input(), "/close-tab");
}

#[tokio::test]
async fn retained_tab_mouse_routes_only_matching_painted_left_clicks() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;

    let mut state = fresh_state("tab-mouse").await;
    let area = Rect::new(0, 0, 31, 24);
    let event = |kind, x, y, modifiers| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers,
    };
    let left = MouseEventKind::Down(MouseButton::Left);
    let up = MouseEventKind::Up(MouseButton::Left);
    let plain = KeyModifiers::NONE;
    state.set_tab_strip(
        (0..12)
            .map(|i| TabPresentation {
                title: Some(format!("Tab {i}")),
                home: false,
                busy: i == 3,
                ..TabPresentation::new(sid(&format!("tab-{i}")))
            })
            .collect(),
        10,
        true,
    );
    assert_eq!(state.tab_presentation().0.len(), 12);
    let strip = crate::shell::tab_strip(&state, area).unwrap();
    let active = strip.tabs.iter().find(|tab| tab.index == 10).unwrap().rect;
    let add = strip.add.unwrap();
    let click = |state: &mut TuiState, x, y| {
        state.handle_mouse(event(left, x, y, plain), area);
        state.handle_mouse(event(up, x, y, plain), area).intent
    };
    assert_eq!(
        click(&mut state, active.x, 0),
        Some(PanelIntent::ActivateTab { index: 10 })
    );
    for rect in [strip.before_marker.unwrap(), strip.after_marker.unwrap()] {
        state.handle_mouse(event(MouseEventKind::Moved, active.x, 0, plain), area);
        state.handle_mouse(event(MouseEventKind::Moved, rect.x, 0, plain), area);
        assert_eq!(state.hovered_tab(area), None);
        assert_eq!(click(&mut state, rect.x, 0), None);
    }
    assert_eq!(click(&mut state, area.right() - 1, 1), None);
    assert_eq!(click(&mut state, add.x, 0), Some(PanelIntent::NewSession));
    assert_eq!(
        click(&mut state, add.x + 1, 0),
        Some(PanelIntent::NewSession)
    );
    assert_eq!(
        click(&mut state, add.x + 2, 0),
        Some(PanelIntent::NewSession)
    );
    state.handle_mouse(event(left, active.x, 0, plain), area);
    assert_eq!(
        state.handle_mouse(event(up, add.x, 0, plain), area).intent,
        None
    );
    state.handle_mouse(event(left, active.x, 0, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, KeyModifiers::SHIFT), area)
            .intent,
        None
    );
    state.handle_mouse(event(left, active.x, 0, KeyModifiers::CONTROL), area);
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(left, active.x, 0, plain), area);
    state.handle_mouse(
        event(
            MouseEventKind::Drag(MouseButton::Left),
            active.x + 1,
            0,
            plain,
        ),
        area,
    );
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(left, active.x, 0, plain), area);
    state.handle_mouse(
        event(MouseEventKind::Down(MouseButton::Right), active.x, 0, plain),
        area,
    );
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(left, active.x, 0, plain), area);
    state.run_command(crate::commands::CommandAction::OpenCommands);
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, plain), area)
            .intent,
        None
    );
    state.close_panel();
    assert_eq!(
        state
            .handle_mouse(event(up, active.x, 0, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(left, add.x, 0, plain), area);
    state.reset_workspace();
    assert_eq!(
        state.handle_mouse(event(up, add.x, 0, plain), area).intent,
        None
    );
    assert!(state.tab_presentation().0.is_empty());
}

#[tokio::test]
async fn home_slot_is_not_an_activation_and_busy_tabs_remain_selectable() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let (app, _inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let area = Rect::new(0, 0, 80, 24);
    let mouse = |kind, x| MouseEvent {
        kind,
        column: x,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    let down = MouseEventKind::Down(MouseButton::Left);
    let up = MouseEventKind::Up(MouseButton::Left);
    assert!(crate::shell::tab_strip(&state, area).is_none());
    assert_eq!(state.handle_mouse(mouse(down, 1), area).intent, None);
    assert_eq!(state.handle_mouse(mouse(up, 1), area).intent, None);
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Busy".into()),
            home: false,
            busy: true,
            ..TabPresentation::new(sid("busy"))
        }],
        0,
        true,
    );
    let strip = crate::shell::tab_strip(&state, area).unwrap();
    assert_eq!(strip.tabs.len(), 2);
    assert!(strip.add.is_none());
    let real = strip.tabs[0].rect;
    let home = strip.tabs[1].rect;
    state.handle_mouse(mouse(down, home.x + 1), area);
    assert_eq!(state.handle_mouse(mouse(up, home.x + 1), area).intent, None);
    state.handle_mouse(mouse(down, real.x + 1), area);
    assert_eq!(
        state.handle_mouse(mouse(up, real.x + 2), area).intent,
        Some(PanelIntent::ActivateTab { index: 0 })
    );
    state.handle_mouse(mouse(down, real.x + 1), area);
    state.set_tab_strip(
        vec![TabPresentation {
            title: None,
            home: false,
            busy: false,
            ..TabPresentation::new(sid("idle"))
        }],
        0,
        false,
    );
    assert_eq!(state.handle_mouse(mouse(up, real.x + 1), area).intent, None);
}

#[tokio::test]
async fn hovered_close_requires_painted_cell_matching_press_and_no_drag() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let (app, _inbox, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Long real tab title".into()),
            home: false,
            busy: false,
            ..TabPresentation::new(sid("long"))
        }],
        0,
        false,
    );
    let area = Rect::new(0, 0, 80, 24);
    let strip = crate::shell::tab_strip(&state, area).unwrap();
    let real = strip.tabs[0].rect;
    let home = strip.tabs[1].rect;
    let event = |kind, x, modifiers| MouseEvent {
        kind,
        column: x,
        row: 0,
        modifiers,
    };
    let plain = KeyModifiers::NONE;
    let down = MouseEventKind::Down(MouseButton::Left);
    let up = MouseEventKind::Up(MouseButton::Left);
    let real_close = real.right() - 2;
    let home_close = home.right() - 2;
    assert_eq!(
        state
            .handle_mouse(event(down, real_close, plain), area)
            .intent,
        None
    );
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        Some(PanelIntent::ActivateTab { index: 0 })
    );

    state.handle_mouse(event(MouseEventKind::Moved, real.x + 3, plain), area);
    assert_eq!(state.hovered_tab(area), Some(0));
    state.handle_mouse(event(down, real_close, plain), area);
    assert_eq!(state.hovered_tab(area), Some(0));
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        Some(PanelIntent::CloseTab { index: 0 })
    );

    state.handle_mouse(event(down, real_close, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, real_close - 1, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(down, real_close - 1, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(down, real_close, plain), area);
    state.handle_mouse(
        event(MouseEventKind::Drag(MouseButton::Left), real_close, plain),
        area,
    );
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(down, real_close, KeyModifiers::SHIFT), area);
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(down, real_close, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, KeyModifiers::SHIFT), area)
            .intent,
        None
    );
    state.handle_mouse(
        event(MouseEventKind::Down(MouseButton::Right), real_close, plain),
        area,
    );
    assert_eq!(
        state
            .handle_mouse(event(up, real_close, plain), area)
            .intent,
        None
    );

    state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
    state.handle_mouse(event(down, home_close, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, home_close, plain), area)
            .intent,
        Some(PanelIntent::CloseTab { index: 1 })
    );
    state.handle_mouse(event(MouseEventKind::Moved, home.right(), plain), area);
    assert_eq!(state.hovered_tab(area), None);
    state.handle_mouse(event(down, home_close, plain), area);
    assert_eq!(
        state
            .handle_mouse(event(up, home_close, plain), area)
            .intent,
        None
    );
    state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
    state.handle_mouse(event(down, home_close, plain), area);
    state.run_command(crate::commands::CommandAction::OpenCommands);
    assert_eq!(state.hovered_tab(area), None);
    assert_eq!(
        state
            .handle_mouse(event(up, home_close, plain), area)
            .intent,
        None
    );
    state.close_panel();
    assert_eq!(state.hovered_tab(area), None);
    state.handle_mouse(event(MouseEventKind::Moved, home_close, plain), area);
    state.set_tab_strip(vec![], 0, false);
    assert_eq!(state.hovered_tab(area), None);
}

#[tokio::test]
async fn post_close_hold_is_released_when_a_modal_opens() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let (app, _, _) = CoreApp::channel(4);
    let mut state = TuiState::new_home(app);
    let real = TabPresentation {
        title: Some("survivor".into()),
        home: false,
        busy: false,
        ..TabPresentation::new(sid("survivor"))
    };
    state.set_tab_strip(vec![real.clone()], 0, false);
    let area = Rect::new(0, 0, 120, 40);
    let before = crate::shell::tab_strip(&state, area).unwrap();
    let x = before.tabs[1].rect.right() - 2;
    let mouse = |kind| MouseEvent {
        kind,
        column: x,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    state.handle_mouse(mouse(MouseEventKind::Moved), area);
    state.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left)), area);
    assert_eq!(
        state
            .handle_mouse(mouse(MouseEventKind::Up(MouseButton::Left)), area)
            .intent,
        Some(PanelIntent::CloseTab { index: 1 })
    );
    let snapshot = state.mouse_close_snapshot(1).unwrap();
    state.home = false;
    state.set_tab_strip(vec![real], 0, true);
    state.restore_mouse_close(snapshot);
    assert_eq!(
        crate::shell::tab_strip(&state, area).unwrap().tabs[0]
            .rect
            .width,
        64
    );
    state.run_command(crate::commands::CommandAction::OpenCommands);
    assert_eq!(
        crate::shell::tab_strip(&state, area).unwrap().tabs[0]
            .rect
            .width,
        32
    );
    state.close_panel();
    assert_eq!(state.mouse_position(), None);
    assert_eq!(
        state.tab_close_cell(
            area,
            0,
            crate::shell::tab_strip(&state, area).unwrap().tabs[0].rect
        ),
        None
    );
    // Wheel input below the tab strip also moves the pointer and ends
    // the temporary close hold without disabling transcript scrolling.
    state.restore_mouse_hover((x, 0, area));
    state.close_hold = Some(TabCloseHold {
        area,
        strip: crate::shell::tab_strip(&state, area).unwrap(),
        until: Instant::now() + std::time::Duration::from_secs(5),
    });
    let mut wheel = mouse(MouseEventKind::ScrollDown);
    wheel.row = 12;
    state.handle_mouse(wheel, area);
    assert!(state.close_hold.is_none());
    assert_eq!(state.hovered_tab(area), None);
}

#[tokio::test]
async fn busy_and_clipped_tabs_never_emit_close() {
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    use ratatui::layout::Rect;
    let mut state = fresh_state("close-busy").await;
    let wide = Rect::new(0, 0, 80, 24);
    let mouse = |kind, x| MouseEvent {
        kind,
        column: x,
        row: 0,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let down = MouseEventKind::Down(MouseButton::Left);
    let up = MouseEventKind::Up(MouseButton::Left);
    let legacy = crate::shell::tab_strip(&state, wide).unwrap().tabs[0].rect;
    state.handle_mouse(mouse(MouseEventKind::Moved, legacy.right() - 2), wide);
    assert_eq!(state.tab_close_cell(wide, 0, legacy), None);
    state.handle_mouse(mouse(down, legacy.right() - 2), wide);
    assert_eq!(
        state
            .handle_mouse(mouse(up, legacy.right() - 2), wide)
            .intent,
        None
    );
    state.set_tab_strip(
        vec![TabPresentation {
            title: Some("Busy".into()),
            home: false,
            busy: true,
            ..TabPresentation::new(sid("busy"))
        }],
        0,
        false,
    );
    let narrow = Rect::new(0, 0, 4, 24);
    let x = crate::shell::tab_strip(&state, wide).unwrap().tabs[0]
        .rect
        .right()
        - 2;
    state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
    state.handle_mouse(mouse(down, x), wide);
    assert_eq!(
        state.handle_mouse(mouse(up, x), wide).intent,
        Some(PanelIntent::ActivateTab { index: 0 })
    );
    state.set_tab_strip(
        vec![TabPresentation {
            title: None,
            home: false,
            busy: false,
            ..TabPresentation::new(sid("idle"))
        }],
        0,
        false,
    );
    state.active_turn = Some(WorkerTurnId("busy-close".into()));
    state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
    assert_eq!(
        state.tab_close_cell(
            wide,
            0,
            crate::shell::tab_strip(&state, wide).unwrap().tabs[0].rect
        ),
        None
    );
    state.handle_mouse(mouse(down, x), wide);
    assert_eq!(
        state.handle_mouse(mouse(up, x), wide).intent,
        Some(PanelIntent::ActivateTab { index: 0 })
    );
    state.active_turn = None;
    let small = crate::shell::tab_strip(&state, narrow).unwrap().tabs[0].rect;
    assert_eq!(small.width, 4);
    state.handle_mouse(mouse(MouseEventKind::Moved, small.right() - 2), narrow);
    assert_eq!(state.tab_close_cell(narrow, 0, small), None);
    state.handle_mouse(mouse(down, small.right() - 2), narrow);
    assert_eq!(
        state
            .handle_mouse(mouse(up, small.right() - 2), narrow)
            .intent,
        Some(PanelIntent::ActivateTab { index: 0 })
    );
    state.handle_mouse(mouse(MouseEventKind::Moved, x), wide);
    assert_eq!(state.hovered_tab(wide), Some(0));
    assert_eq!(
        state.hovered_tab(narrow),
        None,
        "resize invalidates painted hover"
    );
    assert_eq!(
        state.hovered_tab(wide),
        None,
        "old geometry must not resurrect on grow"
    );
}
