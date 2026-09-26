# VIS31/VIS32 — actual paired high-refresh/wheel campaign, 2026-09-26

## Current source-built correction

The historical attempt-04 diagnosis below is superseded **only for the observed
detached-completion defect** by [`high-refresh-20260926-05/report.md`](high-refresh-20260926-05/report.md).
Same-session completion now anchors by durable message/derived part/row, retains
overlapping older pages and expanded parts, and leaves sticky-bottom follow and
explicit session/Location/conversation reset separate. Both new raw PTY regressions
and the full final serial workspace gate pass: `tool_0df7a262c0017rL2kcrdyCQPZi`.
The source-built repeat preserves detached first rows on original/native through
completion without keyboard navigation and follows the final row at bottom.
Four native settled idle windows have zero terminal bytes, CPU ticks and main-thread
context switches. Native input-paint p95 is 38.307/35.761ms at requested 165/250Hz,
with no lost glyphs or lagged worker events. Expanded-history draw p95 is 24.441ms;
these are not measured 165/250FPS. All 44 unmasked comparisons remain DIFFERENT,
three active PNGs are unstable, and large-history paging and all-thread wakeup
qualification remain open. Old failed captures and their reports are retained.

## Historical pre-fix handoff

**Result: diagnostic measurements completed; VIS31/VIS32 not qualified.**
Native loses its detached viewport on durable completion. Normal/settled
styled cells and PNGs remain different from original. Active draw cost in this
scenario exceeds both requested frame budgets. No product Rust changes were
made by this capture/test slice.

## Provenance and immutable attempts

The actual checkout at start was `a0ddb7dd0ca25789f08e19718aa51fa085e1169f`,
not the assignment's earlier `0a9d0657`. Inherited VIS31/32 product edits and
terminal-test scaffolding were dirty. Each attempt seals actual executable
SHA-256, HEAD/tree, dirty diff digest and tracked/untracked source manifest.
The existing native binary was supplied by the parent; this runner did not
invoke Cargo and does not independently attest build-to-source association.
Original executable is the pinned source-built v2.0.12 binary, SHA-256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

All attempts remain intact under `evidence/tui/recovery-v00/`:

| Attempt | Actual outcome |
|---|---|
| `high-refresh-20260926-01` | Original completed; external 120s runner timeout interrupted native. Coarse frontend samples are not usable as low-latency measurements. |
| `high-refresh-20260926-02` | Both campaigns completed, comparator exit 1. Moved paced injection/raw timestamps to bridge; stream anchor scenario still mixed typing with detached streaming; original glyph sampling ended too early and omitted late paints. Kept incomplete distributions, no substituted p95. Forced capture teardown did not seal native scheduler metrics. |
| `high-refresh-20260926-03` | Both exposed detached completion behavior. Harness erroneously sent Ctrl-C with empty draft after stream 3, exiting before stream 4; preserved as failed attempt. |
| `high-refresh-20260926-04` | Both complete with natural code-0 exits, verified provider contracts and sealed native scheduler metrics; all 44 full-grid/PNG comparisons DIFFERENT. Three active native capture stages UNSTABLE_CAPTURE due real scanner frames. |

The requested new `...-01` path was created rather than overwritten; later
attempts have independent paths. `...-04/wheel-summary.json` and the subsequently
added `wheel-details.json` are derived from its preserved raw artifacts.
Runner hashes/source manifest identify the capture-time scripts; subsequent
documentation, summary enrichment and raw PTY regression test additions are
not retroactively represented as inputs of that attempt.

## Scenario and measurement method

120x40, xterm.js frontend, identical native/original test-only config semantics.
Select actual `fixture-shell` profile. Responses sends two actual function
calls (`shell`/`bash` according to advertised schema) executing short and
40-line `printf`; subsequent requests validate every returned line and prior
call ID. No fake tool-result corpus, raw fabricated baseline or masks.
Expand long Shell output via real SGR click, resize to show its full output,
restore 40 rows and keep a multiline `FOCUS-A\nFOCUS-B` draft during wheels.

At each requested 165/250 Hz: single up, eight up ticks, three up/three down,
32 top-edge and 32 bottom-edge ticks. Twelve detached-live up ticks and 64
repin ticks cover streaming. List-modal wheels use the actual configured
catalog extended by 32 test-only entries. Both modal outputs scroll to
`ZZ Wheel 04..17`; transcript underlays and editor cursors remain unchanged
within each side across modal wheels. Regular Shell wheel endpoints preserve
the editor cursor. The captured full grids retain all existing differences.

The PTY bridge independently injects paced inputs and timestamps actual writes
and reads with `time.monotonic_ns()`. Streaming/typing latency is first actual
PTY output containing the unique injected UTF-8 glyph minus input timestamp;
32 observations per rate, zero missing samples in attempt 04. It is not FPS,
browser canvas latency or physical terminal refresh. Browser full-grid samples
cost hundreds of ms and are labeled coarse observations only. Wheel first/last
PTY writes bound the output timeline for otherwise idle scrolling; active
scanner/provider writes cannot be attributed exclusively to wheel settling.

## Actual results — attempt 04

### Pinned scroll-source inspection

Original `opencode/packages/tui/src/util/scroll.ts` returns
`CustomSpeedScroll(3)` by default; `MacOSScrollAccel` is selected only when
`config.scroll.acceleration` is explicitly enabled. Local pinned donor
`package.json` admits `@opentui/core` **0.5.10**. Its npm metadata identifies
gitHead `f6673a04ccb671b9207da358c57152bfd27c781f`; inspected exact sources:

- [scroll-acceleration.ts](https://github.com/anomalyco/opentui/blob/f6673a04ccb671b9207da358c57152bfd27c781f/packages/core/src/lib/scroll-acceleration.ts):
  three-interval velocity history, 150ms streak reset, <6ms events return base
  multiplier 1 without adding to velocity history, exponential multiplier
  capped at 6. This is displacement acceleration, not an independent momentum
  animation. Returning 1 does not discard that directional wheel event.
- [ScrollBox.ts](https://github.com/anomalyco/opentui/blob/f6673a04ccb671b9207da358c57152bfd27c781f/packages/core/src/renderables/ScrollBox.ts):
  `onMouseEvent` immediately adds base delta times multiplier to an accumulator,
  truncates whole rows and changes `scrollTop`; wheel motion does not start
  an independent live animation. Drag auto-scroll is a separate path.

The campaign keeps default acceleration configuration. Native's
`WHEEL_PRESENTATION = Duration::from_nanos(16_666_667)` is an explicit temporal
adaptation toward the default displacement endpoint. No optional MacOS
acceleration or momentum parity is claimed. SGR directional wheel bytes
contain no pixel displacement.

### UTF-8 input-to-painted-terminal output during stream updates

| Requested rate | Original p50 / p95 / max ms | Native p50 / p95 / max ms |
|---|---|---|
| 165 Hz | 156.108 / 229.543 / 235.861 | 13.513 / 18.141 / 18.366 |
| 250 Hz | 353.192 / 408.415 / 412.863 | 17.000 / 23.442 / 29.782 |

Both actually painted 32/32 glyphs at both rates. These values are this paired
campaign under its browser/IPC load, not a portable acceptance threshold or
proof that original has the same performance on a user's physical terminal.
The native provider delivered 265 worker events, queue peak 61, zero lagged
events; input progressed concurrently. Exact inputs and chunk timelines are
preserved. A separate raw PTY test now stresses 1024 SSE deltas while injecting
32 unique glyphs at each rate without browser overhead.

### Idle and CPU

Four native stable windows of 1010.13–1010.47 ms each: **0 terminal bytes,
0 process CPU ticks, 0 main-thread voluntary/involuntary context switches**.
`SC_CLK_TCK=100`, so CPU resolution is 10ms; zero ticks is not sub-tick proof of
zero CPU. Original windows: zero terminal bytes, 0–2 CPU ticks (0–20ms),
10–11 voluntary main-thread context switches. This records the actual original
baseline rather than imposing native no-wakeup semantics on original.

Between first expanded-idle sample and final settled-idle end:
original 47.461s wall / 2.110s process CPU; native 49.028s wall / 4.000s process
CPU. The windows include active scrolling, dialogs, streams, screenshot waits
and idle, excluding startup/initial tools. They are not equal fixed-work CPU
benchmarks and do not establish overall active CPU superiority.

### Native scheduler probe

306 draw attempts, 216 changed frames, 391 scheduler wakeups, 416 input events.
Total draw time 3956.665ms; mean **12.930ms**, p50 **11.251ms**, p95
**25.657ms**, max **40.184ms**. Terminal write time total **56.617ms**, max
single write **5.229ms**; 167141 write calls, 920 flush calls, 229601 bytes.
Draw durations include writing. These measurements do not support achieving
6.06ms/4ms actual paint cadence for this expanded/streaming history scenario.
The configured scheduler budget is not measured frame rate.

The scheduler uses a relative first-draw origin, while `/proc` windows have
external monotonic timestamps. No exact cross-clock idle frame/wakeup mapping
is claimed. The existing raw PTY scheduler test independently checks zero idle
draw attempts in a startup-relative stable interval. All-thread wakeups were
not sampled; `/proc/status` context-switch observations refer to the main thread.

### Wheel timeline and settled endpoints

| Scenario | Original first write / last write after last input, ms | Native, ms |
|---|---|---|
| 165 single up | 3.981 / 21.070 | 32.064 / 79.302 |
| 165 eight-up burst | 1.646 / 9.242 | 32.577 / 101.871 |
| 165 reversed | 1.968 / 22.401 | 34.875 / 77.744 |
| 250 single up | 1.578 / 17.620 | 27.659 / 50.764 |
| 250 eight-up burst | 1.477 / 6.500 | 21.381 / 61.807 |
| 250 reversed | 1.701 / 16.995 | 19.299 / 44.324 |

Boundary ticks can continue after original's final changed output; negative
last-write offsets in raw evidence mean settled-before-last-input, not negative
latency. Native edge last writes are 17.2–37.2ms after last input, followed by
zero-byte/zero-main-thread-switch settled idle windows. No long stale catch-up
was observed in these bounded sequences. Complete displacement/convergence at
all possible boundaries is not inferred from these scenarios.

For single-up endpoints original begins `SHELL-LINE-15`, native
`SHELL-LINE-13`; bottom-edge original `18`, native `16`. The two-row difference
already exists in Shell card layout (original's successful exit text persists
in this pinned binary). Both single ticks shift three rows relative to their
own bottom endpoint. Top-edge reaches the actual beginning on both. Full cells
and PNGs remain different; endpoint marker agreement after repin does not
replace whole-grid comparison.

## Confirmed product defect and next engineering step

Pure stream 3 intentionally sends **no keyboard input** after detached scroll.
Original before/after remains `LIVE-3-000..023`, last row 119 invisible.
Native before `LIVE-3-000..024`, after **`LIVE-3-091..119`**, editor cursor
unchanged `(5,34)`: completion repins to bottom. This reproduces independently
of the earlier combined typing probe. Sticky stream 4 follows row 119 on both.

Likely boundary: `crates/oc/src/tui_cmd.rs` TurnFinished calls
`state.attach_page(&page)` after `apply_finished`; `crates/oc-tui/src/app.rs`
`TuiState::attach_page` clears viewport, assigns `scroll=0`, and drops wheel
motion. Preserve the settled detached message/row anchor across same-session
durable completion refresh while retaining bottom follow for already pinned
views. Do not change new-session/session-switch attachment semantics globally.
Also profile visible-history rendering before claiming 165/250Hz actual paints.

## Tests/checks and remaining gaps

Added in `crates/oc/tests/pty_t39.rs`:

- `vis31_high_rate_input_during_provider_burst`: real 1024-event SSE peer,
  independently injected UTF-8 glyphs, existing 100ms paint deadline,
  post-burst zero terminal bytes, existing <=1 CPU-tick idle allowance,
  zero lagged worker events and terminal restoration.
- `vis32_detached_stream_anchor_survives_durable_completion_refresh`: held
  120-row real Responses stream and actual wheel; asserts first visible row
  anchor and shared visible markers survive durable completion. Expected to expose the captured
  current product regression until the parent fixes it.

Rust tests **NOT RUN here**: parent has exclusive Cargo ownership. Parent must
run targeted `cargo test --locked -p oc --test pty_t39 vis31_ -- --nocapture`
and the new `vis32_detached_stream_anchor_survives_durable_completion_refresh`
test after applying the product fix. `rustfmt --edition 2024`, Node syntax,
Python compilation and `git diff --check` were run successfully before final
handoff; compilation/execution of added Rust tests is not represented as PASS.

No retained old native pre-scheduler binary was supplied or rebuilt here.
Inherited scheduler-test comments cite old-loop 3–4 ticks/627–660 bytes per
second; treat that as a historical host-specific measurement, **not a portable
new threshold**. Existing numeric deadlines/CPU assertions were preserved.
No latency/CPU threshold was relaxed or invented from configuration.

Remaining qualification: detached completion fix and retest; exact idle probe
alignment/worker-thread wakeup measurement if required; source-built native
association by parent; original active PNG scanner phase stability; large-history
paging under a detached live anchor; exact whole-cell/PNG parity. These bounded
six-message windows do not qualify history paging. VIS31/VIS32 remain open.
