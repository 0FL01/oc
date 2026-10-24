//! Opt-in scroll probe over transcripts dominated by Patch/Edit diff cards
//! (not part of the offline gate).
//!
//! Builds a TUI view whose history holds many confirmed mutation cards with
//! realistic Update hunks (context, paired removals/additions, long Rust
//! lines that wrap in split view), then scrolls through it with mouse-wheel
//! steps and PageUp, timing every frame (projection + render) on a 200x60
//! `TestBackend`. `OC_DIFF_FIXTURE=<json array of PatchEffects>` replaces the
//! synthetic effects with real ones (keep such fixtures out of the repo).
//!
//! ```sh
//! cargo test -p oc2 --test perf_diff_scroll -- --ignored --nocapture probe
//! ```
use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::domain::SessionId;
use oc_core::patch::{
    DiffAlgorithm, FileEffect, LineEnding, LineRange, PatchEffects, PatchHunk, PatchLine,
    PatchLineKind, PatchOperation,
};
use oc_core::queries::{HistoryMessage, HistoryPage, HistoryTurn, ToolOpView, TranscriptPart};
use oc_core::session::Role;
use oc_tui::app::TuiState;
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, Instant};

fn code_line(seed: usize) -> String {
    match seed % 7 {
        0 => {
            format!("    let value_{seed} = state.transcript_rows().last().unwrap().text.as_ptr();")
        }
        1 => format!(
            "    assert_eq!(commit.binding.agent_id.as_deref(), Some(\"agent-{seed}\"), \"profile {seed} keeps its binding across reloads\");"
        ),
        2 => format!("    // Explains why case {seed} keeps the parked view and its cursor."),
        3 => format!("    let mut prepared = stage(&mut state, \"{seed}\").await?;"),
        4 => format!(
            "    state.handle_key(KeyAction::Enter).await; inbox.try_recv().unwrap_or_else(|_| panic!(\"prepare {seed}\"));"
        ),
        5 => "    }".to_string(),
        _ => format!(
            "    fn helper_{seed}(&self, value: u64) -> Result<u64, Error> {{ Ok(value.rotate_left({})) }}",
            seed % 13
        ),
    }
}

fn line(kind: PatchLineKind, old: Option<usize>, new: Option<usize>, text: String) -> PatchLine {
    PatchLine {
        lossy: false,
        kind,
        old_line: old,
        new_line: new,
        text,
        ending: LineEnding::Lf,
        truncated: false,
    }
}

/// One file with `hunks` Update hunks of ~`size` lines each.
fn file(card: usize, index: usize, hunks: usize, size: usize) -> FileEffect {
    let mut out = Vec::new();
    let (mut additions, mut deletions) = (0, 0);
    for hunk in 0..hunks {
        let start = 40 + hunk * 200;
        let (mut old, mut new) = (start, start);
        let mut lines = Vec::new();
        for step in 0..size {
            let seed = card * 1000 + index * 100 + hunk * 10 + step;
            match step % 6 {
                2 => {
                    lines.push(line(
                        PatchLineKind::Removed,
                        Some(old),
                        None,
                        code_line(seed),
                    ));
                    old += 1;
                    deletions += 1;
                }
                3 | 4 => {
                    lines.push(line(
                        PatchLineKind::Added,
                        None,
                        Some(new),
                        code_line(seed + 3),
                    ));
                    new += 1;
                    additions += 1;
                }
                _ => {
                    lines.push(line(
                        PatchLineKind::Context,
                        Some(old),
                        Some(new),
                        code_line(seed),
                    ));
                    old += 1;
                    new += 1;
                }
            }
        }
        out.push(PatchHunk {
            old: LineRange {
                start,
                count: old - start,
            },
            new: LineRange {
                start,
                count: new - start,
            },
            lines,
            truncated: false,
        });
    }
    FileEffect {
        algorithm: DiffAlgorithm::Minimal,
        operation: PatchOperation::Update,
        path: format!("crates/oc-tui/src/app/tests/case_{card}_{index}.rs"),
        destination: None,
        additions,
        deletions,
        hunks: out,
        truncated: false,
    }
}

fn synthetic(cards: usize) -> Vec<PatchEffects> {
    (0..cards)
        .map(|card| {
            let files: Vec<_> = (0..1 + card % 3)
                .map(|index| file(card, index, 3, 40))
                .collect();
            PatchEffects {
                total_files: files.len(),
                additions: files.iter().map(|f| f.additions).sum(),
                deletions: files.iter().map(|f| f.deletions).sum(),
                files,
                truncated: false,
            }
        })
        .collect()
}

fn page(effects: Vec<PatchEffects>) -> HistoryPage {
    let mut rows = Vec::new();
    let mut seq = 0;
    for (index, effects) in effects.into_iter().enumerate() {
        seq += 1;
        rows.push(HistoryMessage {
            images: Vec::new(),
            id: oc_core::session::MessageId(format!("user-{seq}")),
            turn: None,
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
            seq,
            role: Role::User,
            text: format!("Apply change {index} to the selection fixtures"),
        });
        seq += 1;
        let name = if index % 4 == 3 {
            "edit"
        } else {
            "apply_patch"
        };
        rows.push(HistoryMessage {
            images: Vec::new(),
            id: oc_core::session::MessageId(format!("assistant-{seq}")),
            turn: Some(HistoryTurn {
                id: format!("turn-{index}"),
                status: "completed".into(),
                model_label: "fixture".into(),
                parts: vec![
                    TranscriptPart::Text(format!("Updating the fixtures for change {index}.")),
                    TranscriptPart::Tool(ToolOpView {
                        child_job: None,
                        output_presentation: None,
                        question: None,
                        op: format!("op-{index}"),
                        rowid: index as i64 + 1,
                        name: name.into(),
                        state: "completed".into(),
                        input: Some("{}".into()),
                        output: Some("Success. Updated the following files.".into()),
                        output_bytes: 40,
                        output_truncated: false,
                        patch_effects: Some(effects),
                        dcp: None,
                        dcp_topic: None,
                    }),
                    TranscriptPart::Text(format!("Change {index} is applied.")),
                ],
                ..HistoryTurn::default()
            }),
            model_switch: None,
            agent_switch: None,
            child: None,
            shell_notice: None,
            user_shell: None,
            seq,
            role: Role::Assistant,
            text: String::new(),
        });
    }
    let total = rows.len();
    HistoryPage {
        rows,
        total,
        ..HistoryPage::default()
    }
}

fn stats(label: &str, mut samples: Vec<Duration>) {
    if samples.is_empty() {
        return;
    }
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    let over = |ms: u64| {
        samples
            .iter()
            .filter(|d| **d > Duration::from_millis(ms))
            .count()
    };
    eprintln!(
        "DIFF {label}: n={} p50={:.2}ms p95={:.2}ms p99={:.2}ms max={:.2}ms over8ms={} over16ms={}",
        samples.len(),
        at(0.5).as_secs_f64() * 1e3,
        at(0.95).as_secs_f64() * 1e3,
        at(0.99).as_secs_f64() * 1e3,
        at(1.0).as_secs_f64() * 1e3,
        over(8),
        over(16)
    );
}

async fn settle(state: &mut TuiState) {
    let started = Instant::now();
    while state.has_pending_transcript_projection() && started.elapsed() < Duration::from_secs(10) {
        state.poll_transcript_projection();
        tokio::task::yield_now().await;
    }
}

#[derive(Clone, Default)]
struct Sink(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

impl std::io::Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Terminal bytes of the last frames: the plain Crossterm cell diff, and the
/// real `FrameBackend` publication (which scrolls moved rows).
struct Output {
    previous: Option<ratatui::buffer::Buffer>,
    sink: Sink,
    backend: oc_tui::terminal::FrameBackend<Sink>,
    plain: Vec<usize>,
    bytes: Vec<usize>,
    encode: Vec<Duration>,
}

impl Output {
    fn new(width: u16, height: u16) -> Self {
        let sink = Sink::default();
        let backend = oc_tui::terminal::FrameBackend::new(sink.clone());
        backend
            .assume_size(ratatui::layout::Size::new(width, height))
            .unwrap();
        Self {
            previous: None,
            sink,
            backend,
            plain: Vec::new(),
            bytes: Vec::new(),
            encode: Vec::new(),
        }
    }

    /// Publish the cells changed since the last call and record the bytes
    /// and the publication time.
    fn record(&mut self, terminal: &Terminal<TestBackend>) {
        use ratatui::backend::Backend;
        let current = terminal.backend().buffer().clone();
        let empty = ratatui::buffer::Buffer::empty(current.area);
        let previous = self.previous.as_ref().unwrap_or(&empty);
        let changed = previous.diff(&current);
        let mut plain = Vec::new();
        ratatui::backend::CrosstermBackend::new(&mut plain)
            .draw(changed.iter().copied())
            .unwrap();
        self.plain.push(plain.len());
        let started = Instant::now();
        self.backend.begin_frame().unwrap();
        self.backend.draw(changed.into_iter()).unwrap();
        self.backend.finish_frame().unwrap();
        self.encode.push(started.elapsed());
        self.bytes
            .push(std::mem::take(&mut *self.sink.0.borrow_mut()).len());
        self.previous = Some(current);
    }
}

/// One frame like the event loop: projection (off-thread when large), draw.
async fn frame(
    terminal: &mut Terminal<TestBackend>,
    state: &mut TuiState,
    output: &mut Output,
) -> Duration {
    let started = Instant::now();
    terminal
        .draw(|frame| oc_tui::shell::render(frame, state))
        .unwrap();
    if state.has_pending_transcript_projection() {
        settle(state).await;
        terminal
            .draw(|frame| oc_tui::shell::render(frame, state))
            .unwrap();
    }
    let elapsed = started.elapsed();
    output.record(terminal);
    elapsed
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "opt-in diff scroll probe"]
async fn probe() {
    let effects = match std::env::var_os("OC_DIFF_FIXTURE") {
        Some(path) => {
            let fixture: Vec<PatchEffects> =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            let copies: usize = std::env::var("OC_DIFF_COPIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8);
            (0..copies).flat_map(|_| fixture.clone()).collect()
        }
        None => synthetic(
            std::env::var("OC_DIFF_CARDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(40),
        ),
    };
    let lines: usize = effects
        .iter()
        .flat_map(|e| &e.files)
        .flat_map(|f| &f.hunks)
        .map(|h| h.lines.len())
        .sum();
    eprintln!("DIFF cards={} diff_lines={lines}", effects.len());
    let history = page(effects);
    let steps: usize = std::env::var("OC_DIFF_STEPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    for width in [200u16, 110] {
        let mut state = TuiState::new(app.clone(), SessionId::new("s-diff-scroll").unwrap());
        state.attach_page(&history);
        let mut terminal = Terminal::new(TestBackend::new(width, 60)).unwrap();
        let mut output = Output::new(width, 60);
        let first = frame(&mut terminal, &mut state, &mut output).await;
        eprintln!(
            "DIFF width={width} first frame {:.2}ms",
            first.as_secs_f64() * 1e3
        );
        // Mouse wheel: one tick per step, animation run to its end like the
        // display-paced loop (a frame per animation tick).
        let mut wheel = Vec::new();
        let mut now = Instant::now();
        for _ in 0..steps {
            state.wheel_transcript_at(true, 1, now);
            loop {
                now += Duration::from_micros(6060);
                let moving = state.tick_scroll_animation(now);
                wheel.push(frame(&mut terminal, &mut state, &mut output).await);
                if !moving {
                    break;
                }
            }
        }
        stats(&format!("width={width} wheel frame"), wheel);
        for (label, mut bytes) in [
            ("plain diff", std::mem::take(&mut output.plain)),
            ("published", std::mem::take(&mut output.bytes)),
        ] {
            bytes.remove(0);
            let total: usize = bytes.iter().sum();
            bytes.sort_unstable();
            eprintln!(
                "DIFF width={width} wheel {label} bytes p50={} p95={} max={} total={total}",
                bytes[bytes.len() / 2],
                bytes[(bytes.len() - 1) * 95 / 100],
                bytes[bytes.len() - 1]
            );
        }
        let mut encode = std::mem::take(&mut output.encode);
        encode.remove(0);
        stats(&format!("width={width} wheel publish"), encode);
        let mut pages = Vec::new();
        for _ in 0..steps / 5 {
            state.scroll_transcript(false);
            pages.push(frame(&mut terminal, &mut state, &mut output).await);
        }
        stats(&format!("width={width} PageDown frame"), pages);
    }
}
