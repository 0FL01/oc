//! S07: per-draw, single-thread allocation profile of the bounded TUI view.
//!
//! The 3,000-message archive is represented by HistoryPage metadata, not by
//! materializing older rows. This qualifies the renderer with an identical
//! loaded window; it does not measure storage, history paging or the PTY path.
//! The allocator tracks the test thread only. Cross-thread allocation/free of
//! the same pointer is not attributed accurately; the worker is idle here.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use oc_core::core_app::{CoreApp, MockProvider};
use oc_core::domain::SessionId;
use oc_core::queries::{HistoryMessage, HistoryPage, HistoryTurn, ToolOpView, TranscriptPart};
use oc_core::session::Role;
use oc_tui::app::TuiState;
use oc_tui::history::{WINDOW_BYTES, WINDOW_ROWS};
use ratatui::{Terminal, backend::TestBackend};

#[derive(Clone, Copy, Default)]
struct Counts {
    // All live requested bytes on this thread, including allocations made
    // before the measured draw; the baseline is captured at begin().
    live: usize,
    baseline: usize,
    peak_live: usize,
    allocated: usize,
    calls: usize,
    active: bool,
}

thread_local! {
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        live: 0, baseline: 0, peak_live: 0, allocated: 0, calls: 0, active: false
    }) };
}

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

impl CountingAllocator {
    fn change(old: usize, new: usize, allocation: bool) {
        // try_with also permits late deallocation during thread-local teardown.
        let _ = COUNTS.try_with(|cell| {
            let mut c = cell.get();
            c.live = c.live.saturating_sub(old).saturating_add(new);
            if c.active {
                if allocation {
                    c.allocated = c.allocated.saturating_add(new);
                    c.calls = c.calls.saturating_add(1);
                }
                c.peak_live = c.peak_live.max(c.live);
            }
            cell.set(c);
        });
    }
}

// SAFETY: Each operation delegates to System with exactly the caller's layout
// and pointer; accounting only occurs after a successful allocation. No
// allocator operation allocates or holds a lock in the accounting path.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc passes a valid layout through to System.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            Self::change(0, layout.size(), true);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc passes a valid layout through to System.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            Self::change(0, layout.size(), true);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original live pointer and layout.
        unsafe { System.dealloc(ptr, layout) };
        Self::change(layout.size(), 0, false);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: The caller supplies the original live pointer and layout;
        // System implements the requested new allocation size.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            // A successful realloc counts its new requested size as an
            // allocation, even if System could resize in place.
            Self::change(layout.size(), new_size, true);
        }
        new_ptr
    }
}

struct DrawScope;

impl DrawScope {
    fn begin() -> Self {
        COUNTS.with(|cell| {
            let mut c = cell.get();
            assert!(!c.active, "nested draw measurement");
            c.baseline = c.live;
            c.peak_live = c.live;
            c.allocated = 0;
            c.calls = 0;
            c.active = true;
            cell.set(c);
        });
        Self
    }

    fn finish(self) -> Counts {
        let result = COUNTS.with(|cell| {
            let mut c = cell.get();
            c.active = false;
            cell.set(c);
            c
        });
        std::mem::forget(self);
        result
    }
}

impl Drop for DrawScope {
    fn drop(&mut self) {
        COUNTS.with(|cell| {
            let mut c = cell.get();
            c.active = false;
            cell.set(c);
        });
    }
}

fn page(older: usize) -> HistoryPage {
    let table = (0..24)
        .map(|i| format!("| row {i:02} 界 | `value-{i}` | note {i} |\n"))
        .collect::<String>();
    let markdown = format!(
        "### Wide table\n| col A | col B | col C |\n| --- | --- | --- |\n{table}\n```rust\nfn unfinished() {{\n    let x = 42;\n"
    );
    // The real storage projection serves at most 2,048 output bytes. Model a
    // longer result through its bounded preview plus full-size metadata; no
    // 128 KiB result is materialized by this renderer fixture.
    let tool_preview = "result line: 0123456789 abcdefghijklmnop\n"
        .repeat(100)
        .chars()
        .take(2_048)
        .collect::<String>();
    let rows = (1..=200)
        .map(|seq| {
            let role = if seq % 2 == 0 {
                Role::Assistant
            } else {
                Role::User
            };
            let text = if seq == 200 {
                markdown.clone()
            } else {
                format!("current-{seq:03}: rendered message with Unicode 漢字")
            };
            let turn = (seq == 200).then(|| HistoryTurn {
                id: "render-stress-turn".into(),
                status: "completed".into(),
                model_label: "fixture".into(),
                parts: vec![
                    TranscriptPart::Text(markdown.clone()),
                    TranscriptPart::Tool(ToolOpView {
                        op: "render-stress-op".into(),
                        rowid: 1,
                        name: "bash".into(),
                        state: "completed".into(),
                        input: Some(r#"{"command":"generate output"}"#.into()),
                        output: Some(tool_preview.clone()),
                        output_bytes: 128 * 1024,
                        output_truncated: true,
                    }),
                ],
                ..HistoryTurn::default()
            });
            HistoryMessage {
                seq: i64::from(seq) + older as i64,
                role,
                text,
                turn,
            }
        })
        .collect();
    HistoryPage {
        rows,
        total: 200 + older,
        has_older: older > 0,
        has_newer: false,
        ..HistoryPage::default()
    }
}

fn sample(
    state: &TuiState,
    terminal: &mut Terminal<TestBackend>,
) -> (Vec<Counts>, ratatui::buffer::Buffer) {
    for _ in 0..4 {
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, state))
            .expect("warm-up draw");
    }
    let mut samples = Vec::with_capacity(12);
    for _ in 0..12 {
        let scope = DrawScope::begin();
        terminal
            .draw(|frame| oc_tui::views::render_frame(frame, state))
            .expect("measured draw");
        samples.push(scope.finish());
    }
    (samples, terminal.backend().buffer().clone())
}

fn report(label: &str, samples: &[Counts]) {
    let count = samples.len();
    let total: usize = samples.iter().map(|c| c.allocated).sum();
    let peak = samples.iter().map(|c| c.peak_live).max().unwrap_or(0);
    let extra = samples
        .iter()
        .map(|c| c.peak_live.saturating_sub(c.baseline))
        .max()
        .unwrap_or(0);
    let calls: usize = samples.iter().map(|c| c.calls).sum();
    println!(
        "{label}: draws={count}, total_requested={total} B, mean_requested={} B/draw, peak_thread_live={peak} B, max_peak_above_baseline={extra} B, allocation_calls={calls}",
        total / count
    );
    assert!(calls > 0, "draw should allocate");
    assert!(total > 0, "draw should request memory");
}

#[tokio::test(flavor = "current_thread")]
async fn bounded_renderer_archive_allocation_profile() {
    let (app, _guard) = CoreApp::spawn(MockProvider::echo());
    let id = SessionId::new("s-render-alloc").expect("valid id");
    let mut short = TuiState::new(app.clone(), id.clone());
    let mut large = TuiState::new(app, id);
    short.attach_page(&page(0));
    large.attach_page(&page(3000));
    for (name, state) in [("short", &short), ("large", &large)] {
        let history = state.history();
        println!(
            "{name}: backing_total={}, retained_rows={}, retained_bytes={}, has_older={}",
            history.total(),
            history.len(),
            history.retained_bytes(),
            history.has_older()
        );
        assert!(history.len() <= WINDOW_ROWS);
        assert!(history.retained_bytes() <= WINDOW_BYTES);
    }
    assert_eq!(short.history().len(), large.history().len());
    assert_eq!(
        short.history().retained_bytes(),
        large.history().retained_bytes()
    );
    let mut short_terminal = Terminal::new(TestBackend::new(120, 40)).expect("test backend");
    let mut large_terminal = Terminal::new(TestBackend::new(120, 40)).expect("test backend");
    for (width, height) in [(120, 40), (80, 40)] {
        if width != 120 {
            for terminal in [&mut short_terminal, &mut large_terminal] {
                terminal.backend_mut().resize(width, height);
                terminal
                    .resize(ratatui::layout::Rect::new(0, 0, width, height))
                    .expect("resize terminal");
            }
        }
        let (short_samples, short_frame) = sample(&short, &mut short_terminal);
        let (large_samples, large_frame) = sample(&large, &mut large_terminal);
        assert_eq!(
            short_frame, large_frame,
            "same visible cells at {width}x{height}"
        );
        report(&format!("short {width}x{height}"), &short_samples);
        report(&format!("large {width}x{height}"), &large_samples);
    }
}
