//! Opt-in CPU cost of the frames a running tab's sweep causes (not part of
//! the offline gate).
//!
//! A 200x60 view with a transcript and two running tabs is published
//! through the real `FrameBackend` into memory, either as full frames or as
//! strip-only frames (the UI loop's path for a change that touched only the
//! tab strip: `render_tab_strip_only` + `FrameBackend::draw_region`). Costs are thread CPU time, so a busy
//! host preempting the probe does not inflate them.
//!
//! ```sh
//! cargo test --release -p oc2 --test perf_tab_frames -- --ignored --nocapture probe
//! ```
use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::domain::SessionId;
use oc_core::queries::{HistoryMessage, HistoryPage, HistoryTurn, TranscriptPart};
use oc_core::session::Role;
use oc_tui::app::{TabPresentation, TuiState};
use oc_tui::terminal::FrameBackend;
use ratatui::{Terminal, TerminalOptions, Viewport, layout::Rect};
use std::time::{Duration, Instant};

struct Sink;

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn thread_cpu() -> Duration {
    let mut time = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime writes one valid timespec.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
    Duration::new(time.tv_sec as u64, time.tv_nsec as u32)
}

fn page() -> HistoryPage {
    let mut rows = Vec::new();
    for index in 0..30 {
        let (role, turn) = if index % 2 == 0 {
            (Role::User, None)
        } else {
            (
                Role::Assistant,
                Some(HistoryTurn {
                    id: format!("turn-{index}"),
                    status: "completed".into(),
                    model_label: "fixture".into(),
                    parts: vec![TranscriptPart::Text(format!(
                        "## Step {index}\n\nUpdated **the fixtures** and checked `item_{index}`.\n\n- first point\n- second point with more words to wrap across the transcript width\n\n```rust\nfn step_{index}() -> u64 {{ {index} }}\n```\n"
                    ))],
                    ..HistoryTurn::default()
                }),
            )
        };
        rows.push(HistoryMessage {
            images: Vec::new(),
            id: oc_core::session::MessageId(format!("m-{index}")),
            turn,
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
            seq: index + 1,
            role,
            text: format!("Please continue with step {index} of the fixture update"),
        });
    }
    let total = rows.len();
    HistoryPage {
        rows,
        total,
        ..HistoryPage::default()
    }
}

fn report(label: &str, mut samples: Vec<Duration>) {
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    eprintln!(
        "TABS {label}: n={} cpu p50={:.1}us p90={:.1}us",
        samples.len(),
        at(0.5).as_secs_f64() * 1e6,
        at(0.9).as_secs_f64() * 1e6
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opt-in tab frame cost probe"]
async fn probe() {
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    let first = SessionId::new("s-tabs-first").unwrap();
    let mut state = TuiState::new(app, first.clone());
    state.attach_page(&page());
    let mut running = TabPresentation::new(first);
    running.title = Some("Running session with a long descriptive title".into());
    running.busy = true;
    let mut other = TabPresentation::new(SessionId::new("s-tabs-second").unwrap());
    other.title = Some("Another running session".into());
    other.busy = true;
    state.set_tab_strip(vec![running, other], 0, false);
    let backend = FrameBackend::new(Sink);
    backend
        .assume_size(ratatui::layout::Size::new(200, 60))
        .unwrap();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Fixed(Rect::new(0, 0, 200, 60)),
        },
    )
    .unwrap();
    let frames = 600;
    let mut full = Vec::new();
    let mut strip = Vec::new();
    let start = Instant::now();
    for index in 0..frames * 2 {
        let now = start + Duration::from_micros(6060) * index;
        state.tick_ui_parts(now);
        let began = thread_cpu();
        terminal.backend_mut().begin_frame().unwrap();
        // The UI loop's strip-only path (see tui_cmd::draw_tab_strip).
        let strip_drawn = index >= frames && {
            let cursor = terminal.backend().painted_cursor();
            assert!(oc_tui::shell::render_tab_strip_only(
                &mut terminal.get_frame(),
                &state,
                now
            ));
            let region = oc_tui::shell::tab_strip_region(&state).unwrap();
            let buffer = terminal.current_buffer_mut();
            let mut cells = ratatui::buffer::Buffer::empty(region);
            for at in region.positions() {
                cells[at] = std::mem::take(&mut buffer[at]);
            }
            terminal.backend_mut().draw_region(&cells, cursor).unwrap()
        };
        if !strip_drawn {
            terminal
                .draw(|frame| oc_tui::shell::render_at(frame, &state, now))
                .unwrap();
        }
        terminal.backend_mut().finish_frame().unwrap();
        let cost = thread_cpu() - began;
        if strip_drawn { &mut strip } else { &mut full }.push(cost);
    }
    report("full frame", full);
    report("strip-only frame", strip);
}
