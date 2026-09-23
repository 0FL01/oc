# T44 V07 S07 — bounded view resource sample

Code commit `f6734d848913340515d891274f8046cc4418914f`; Linux x86_64,
`rustc 1.93.0`. Sampled debug binary SHA-256
`17f058fe23ac10c5e076b314a2b9149df1188aa69ee6baccb0186854470b5a3d`.
Loopback fake provider, isolated HOME/XDG/project/SQLite and real PTY;
neither real credentials nor external provider used.

## Measurement and reproduced behavior

`s07_pty_equal_view_archive_resource_samples` runs two independent `oc tui`
processes with the same 200-message tail/120×40 viewport and either zero or
3,000 older durable messages. Before sampling, both have the same visible tail;
after wheel scroll away/back, resize 120×40→100×30→120×40, 160-line real
bracketed paste/chip deletion, 145 KiB recorded bash result via `/cards`
(including next page), and Commands dialog search, their visible tails and
post-interaction tails still match. No provider turn is submitted. The real
child is reaped, terminal restored; sampled direct children are zero. The
opt-in `OC_TUI_TEST_METRICS` probe now records count, sum and maximum of the
real `terminal.draw` duration. It is absent unless the test flag is set.

**One post-commit observation**, command `cargo test --locked -p oc --test
pty_t39 s07_pty_equal_view_archive_resource_samples -- --exact --nocapture`
(PASS):

| Older messages | Sampled max RSS / PSS (KiB) | VmHWM (KiB) | CPU ticks after ready | Retained bytes / window rows | Frames / draw sum / draw max |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 26076 / 23782 | 26076 | 49 | 15992 / 150 | 17 / 559.48 ms / 38.39 ms |
| 3000 | 26680 / 24362 | 26680 | 49 | 15992 / 150 | 17 / 585.80 ms / 44.26 ms |

RSS/PSS are samples at workload boundaries, `VmHWM` a kernel high-water;
CPU ticks use `/proc/<pid>/stat` utime+stime between ready and final sample.
Draw includes Ratatui backend write, not only render-model computation.
No performance threshold was invented from one sample.

Standalone `crates/oc/tests/tui_render_alloc.rs` instruments `System` with a
thread-local allocator around **12 warmed synchronous draws** on a reusable
TestBackend per width. Same 200-row active projection with or without 3,000
*metadata-only* older messages produces identical styled cells at 120×40
and 80×40; both projections retain 202 rows / 14,646 bytes. The fixture
includes a 24-row Markdown table, an unclosed Rust fence and a bounded 2 KiB
preview with 128 KiB full-output metadata. Post-commit command
`cargo test --locked -p oc --test tui_render_alloc -- --nocapture` PASS:

| Width | Archive totals | Requested bytes / draw | Peak extra live bytes / draw | Allocation calls / 12 draws |
| --- | --- | ---: | ---: | ---: |
| 120×40 | 200 and 3200 | 30,917,678 each | 220,372 each | 4,300,392 each |
| 80×40 | 200 and 3200 | 30,650,110 each | 220,372 each | 3,399,960 each |

These high gross allocation counts are **observed**, not a memory-leak PASS:
they exclude worker/storage thread allocation and are not comparable with
live process RSS. The metadata-only older archive is not materialized in
this renderer test; real archive loading is tested separately in PTY. The
thread-local peak counts requested bytes above pre-draw baseline, not system
allocator arena memory or cross-thread ownership transfers.

## Related checks, gaps

Current-code `aud31_pty_bounded_backing_state` PASS (3,000 history rows,
bounded window, tool cards/session switch); T40-era `memory_bounds` PASS on
current code (8 vs 3,000 archived pairs; HWM 27,688 / 29,636 KiB, peak
sampled PSS 25,590 / 27,369 KiB), `context_bounds` PASS (8 vs 2,000 pairs,
construction peaks 1,611,973 / 1,097,687 B). Relevant current TUI tests for
wide Markdown, open/incomplete streaming fence and shell output collapse
PASS. Final `cargo fmt --all -- --check`, workspace all-target clippy with
`-D warnings`, `CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked
--workspace --no-fail-fast --quiet`, `cargo build --locked`, docs/progress and
diff checks PASS (zero workspace failures; opt-in live cases ignored).

Early S07 attempts: an assertion requiring >20 *messages* simultaneously at
120×40 failed (8 semantic messages span that frame); removed the invalid
unit-count assumption, preserving the >20 visual-line/large-view requirement.
The first `/cards` assertion wrongly expected the op ID in its list label;
actual label shows the tool and output preview, while the ID is in detail.
Next attempt sent two Esc keys without waiting for the detail→list transition;
now waits for each state and both archive-size runs pass. Neither
failure was suppressed by lowering a resource expectation.

S07 is **partially measured, not closed**: queue occupancy/lag, peak
allocation in the *running* PTY binary, live-part counters and repeated
incomplete-*streaming* fence cost inside that process are not measured;
the renderer fixture has a static unclosed fence and only a metadata-sized
large result. Configured queue cap and bounded part/window tests cannot
replace those measurements. Remaining VIS01–VIS24 still have unequal paired
frames; this report does not establish pixel parity. The old untracked
`.opencode/` was not read or changed.
