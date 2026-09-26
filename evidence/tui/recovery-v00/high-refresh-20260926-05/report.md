# VIS31/VIS32 source-built recapture — high-refresh-20260926-05

**Observed completion-anchor fix verified on the actual native binary.**
Original and native retain their detached first viewport lines through stream
completion with no intervening keyboard navigation. Sticky-bottom follow is
preserved. This is bounded behavior evidence, not whole-frame parity or
165/250 FPS qualification. All 44 unmasked grid/PNG comparisons remain
DIFFERENT; capture exits **1**, with three native active-scanner stages marked
UNSTABLE_CAPTURE. Both application processes exit naturally with code 0 and
valid provider contracts.

## Source/build association

Capture started `2026-09-26T20:38:26.464Z`. Active task T44.

| Input | Recorded value |
|---|---|
| Native HEAD | `f44842e2d83ced45b2eacf5baa7d98de553548c3` |
| HEAD tree | `11452c754340b8ea0c52a61820bc6eb150ca4098` |
| Dirty tracked diff SHA-256 | `490391b4b6bf0775c111bece473b8a496e80d95d49904402c13a154113c1143f` |
| Source manifest digest | `b4a7f8eb55006e26dce06fcaf7d16b9427eee2b89504cf8c7362e6e8d04586c8` |
| Native executable SHA-256 | `fd6672fb6a9d38aa970f2356db993fcd79df27bfb3f344a99d9530eca4e9cf2e` |
| Original source pin | `2670273ff17da96f85c5826ced57aa1b368754fa` |
| Original executable SHA-256 | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

`--build-oc true` invoked **`cargo build --locked`**, exit **0**; Cargo reports
the dev target finished in 0.17s. Build ran before hashing/executing the native
binary. The source manifest includes tracked and untracked source/test/capture
files. HEAD alone is not the product-source association: the inherited dirty
source includes `refresh_completed_page` and semantic id/part/row anchoring.
Actual command/build output is in `commands.json`; exact runner hashes, fixture
hash, executable paths and build command are in `capture.lock.json`.

Invocation:

```sh
CARGO_BUILD_JOBS=3 TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --bounded-mode wheel --geometry true --sample short --sidebar hide \
  --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/high-refresh-20260926-05
node scripts/tui_capture/summarize_wheel.mjs \
  evidence/tui/recovery-v00/high-refresh-20260926-05
```

No product/test/capture source changes were needed for this recapture. Attempt
04 and its failed-anchor artifacts/report remain intact. This report and
`wheel-summary.json` are new artifacts in the new attempt directory.

## Actual fixture and completion behavior

The same paired local Responses protocol performs two actual authorized
Shell/bash `printf` function calls, validates all returned short/40-line
outputs and call IDs, and expands the long Shell card through actual mouse
input. Single/eight-tick/reversed/32-tick-edge wheels run with a multiline
editor draft, followed by a real scrollable model dialog and two real streamed
120-row responses. The bridge independently paces actual PTY input at requested
165/250 Hz, without waiting for browser sampling or paint completion.

| View | Before completion | After completion | Observation |
|---|---|---|---|
| Original detached | `LIVE-3-000..023` | `LIVE-3-000..023` | Same viewport text; row 119 invisible |
| Native detached | `LIVE-3-000..024` | `LIVE-3-000..024` | Same viewport text; row 119 invisible |
| Original sticky | `LIVE-4-029..059` | `LIVE-4-092..119` | Final row follows bottom |
| Native sticky | `LIVE-4-030..059` | `LIVE-4-092..119` | Final row follows bottom |

Detached stream 3 receives only wheel input before release; it receives **no
keyboard input** between the before/after observations. Cursor remains `(5,34)`
on both. Sticky stream 4 also exercises concurrent UTF-8 typing; its draft cursor
moves as expected. Shell wheels preserve the existing editor cursor; list-modal
wheels route to the list, ending at `ZZ Wheel 04..17` on both, with unchanged
per-side transcript underlay. Normal/settled full frames are still compared
without masks and are different.

The previous defect is no longer reproduced in this scenario. No new product
fix is identified by this recapture. The six-message campaign does not exercise
large-history loading or independently qualify overlapping older-page retention.
The owner reports relevant tests and the raw PTY anchor regression PASS; this
capture independently verifies behavior on the source-built binary rather than
rerunning that already-passing test suite.

## Event-to-painted-terminal latency

Each sample is the first actual PTY output chunk containing the unique injected
UTF-8 glyph minus its monotonic input timestamp. Both sides paint **32/32**
glyphs at each requested rate; no missing sample is replaced with a synthetic
latency. Browser samples are separately labeled coarse observations.

| Requested input rate | Original p50 / p95 / max, ms | Native p50 / p95 / max, ms |
|---|---|---|
| 165 Hz | 89.278 / 147.955 / 153.956 | **28.705 / 38.307 / 39.598** |
| 250 Hz | 180.845 / 224.709 / 227.885 | **24.438 / 35.761 / 37.287** |

Raw input times and outputs are preserved in `protocol.json` and
`output-timeline.jsonl`; per-sample values are in `wheel-checks.json` and the
derived `wheel-summary.json`. These host/load-specific original/native values
are not portable performance thresholds, measured FPS, or physical display
refresh. Native p95 differs from attempt 04; no threshold was raised to hide it.
The actual existing 100ms raw PTY paint deadline and <=1 CPU-tick idle allowance
were not modified.

Native worker queue peak **61**, **0 lagged events**, **265 worker events** and
**415 input events**. Input/worker/paint progress is observed concurrently.

## Wheel timing and settling

These are raw first terminal-write delays from first tick and last terminal
write relative to last tick for otherwise idle Shell wheel scenarios. They are
diagnostic output timing bounds, not per-tick changed-cell latency distributions.
The original applies its default three-row endpoint immediately; native's
16.667ms temporal adaptation is an intentional separate motion contract.

| Scenario | Original first / last-after-final-input, ms | Native, ms |
|---|---|---|
| 165 single up | 6.159 / 19.184 | **27.039 / 51.946** |
| 165 eight-up burst | 6.636 / 15.996 | **21.408 / 48.769** |
| 165 reversed | 2.165 / 20.311 | **20.899 / 32.498** |
| 250 single up | 1.611 / 18.869 | **26.893 / 54.724** |
| 250 eight-up burst | 1.354 / 5.070 | **25.767 / 68.040** |
| 250 reversed | 2.984 / 13.991 | **21.867 / 50.647** |

Native top/bottom-edge last-write bounds: **13.816/34.924ms** at requested
165 Hz, **24.298/40.059ms** at 250 Hz. Original negative edge offsets indicate
settling before the remaining boundary ticks, not negative latency. Single ticks
displace three rows relative to each side's own endpoint; settled Shell endpoints
retain the existing two-row layout difference. No prolonged stale catch-up is
observed in these bounded scenarios. No new settling threshold is asserted.

The active-stream wheel last-write value includes real scanner updates and
cannot be attributed exclusively to scrolling. Active native PNGs for
`wheel-stream-detach`, `stream-detached-before`, and `stream-sticky-before` are
marked UNSTABLE_CAPTURE rather than treated as matched/stable screenshots.
The saved viewport text establishes the anchor observation; those PNGs do not
establish scanner-phase parity.

## Idle windows, scheduler and actual process CPU

Four settled native windows: **1010.287, 1010.522, 1009.368, 1010.891ms**.
Every window records **zero terminal bytes, zero process CPU ticks, zero
main-thread voluntary and involuntary context switches**. Actual `/proc` CPU
resolution is 10ms (`SC_CLK_TCK=100`); zero ticks does not prove sub-tick zero
CPU. Original windows record zero bytes, 0–1 CPU ticks and 14–21 voluntary
main-thread context switches. Context switches do not enumerate all worker
thread wakeups.

Native opt-in `OC_TUI_TEST_METRICS` seals `oc/scheduler.json` on natural exit:

- **286** draw attempts, **212** changed frames, **368** scheduler wakeups.
- Draw total **4207.355ms**, mean **14.711ms**, p50 **13.143ms**, p95
  **24.441ms**, max **38.559ms**; draw durations include terminal writes.
- **171627** write calls, **860** flush calls, **234770** terminal bytes.
- Terminal write time total **47.631ms**, max single write **1.555ms**.
- Worker queue peak **61**, lagged **0**.

The scheduler exports relative time from its first instrumented draw, without
an external monotonic epoch. Exact scheduler attempts/wakeups cannot be aligned
to the external `/proc` sampling windows from these artifacts alone. No
fabricated clock origin or zero-attempt count is inferred from zero bytes.
The owner's passing raw PTY idle test separately checks a startup-relative
no-draw interval. Further exact cross-clock alignment would require additional
measurement scaffolding, beyond this unchanged recapture protocol.

From first expanded-idle sample to final settled-idle end:

| Application | Wall window | Actual process CPU |
|---|---:|---:|
| Original | 47.401s | 1.930s |
| Native | 48.411s | 4.250s |

These windows include active scroll/dialog/stream/screenshot waits and idle,
excluding startup/initial tools; they are not a fixed-work comparative CPU
benchmark. Expanded-history draw costs exceed 6.06ms and 4ms; no actual
165/250Hz paint cadence is claimed from requested input rates or configuration.

## Scope of result and gaps

Source build PASS; actual provider/tool contracts and natural exits PASS;
detached-completion and sticky-follow observations supported; independent paced
input and post-settle idle measurements recorded. The actual old native binary
was not rebuilt; historical old-loop comments are not portable thresholds.

Remaining: all 44 whole-frame differences; active scanner screenshot stability;
exact external idle-to-scheduler clock alignment and all-thread wakeup accounting;
large-history paging qualification; render-cost investigation before any
high-refresh paint claim. Full VIS31/VIS32 closure is not implied by the fixed
completion-anchor slice. Attempt 04 continues to document the real prior defect.
