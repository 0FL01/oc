# T44 — real caret frame publication and blink qualification

Date: 2026-10-08. Task remains **ACTIVE**. This is a qualified functional
VIS16/VIS31 slice, **not** full pixel/temporal parity or whole T44/R6/V09 PASS.
Implementation commit: `0becfcb09402e25eea0d3f66361d70c2d322156a`, ordinary
fast-forward PUSHED to verified `github.com/0FL01/oc`, `agent/oc-rust-port`.
Frozen scope: GOAL.md tool-output phantom-caret amendment;
T44_CONTRACT_AMENDMENT.md lines 2007–2142; docs/TEST_PLAN.md VIS16/VIS31.

## Implementation and preserved boundaries

- `oc-tui::terminal::FrameBackend` retains Ratatui's buffers/cursor bookkeeping
  while staging one actual frame. Begin synchronized update and Hide precede
  first background, resize/clear, cursor color and content. Ratatui's internal
  Show-before-MoveTo is deferred: the final active-input MoveTo precedes Show,
  then End synchronized update, one publication and flush.
- Mode 2026 is advisory. Unsupported terminals still receive Hide before paint
  and no Show until the final input position. A frame without an input owner
  has no Show. No terminal capability polling or permanently hidden cursor.
- Pending bytes are consumed before publication; write/flush errors are
  non-success and never replay a failed/partial frame on retry or Drop. Restore
  ends partial synchronized output, resets cursor color, releases mouse/raw/alt
  modes and explicitly shows the terminal cursor, including error/panic paths.
- Composer no longer supplies an underlying caret to a read-only panel. Editable
  Search supplies its own caret; closing it restores the exact composer draft,
  Unicode-aware position and visibility. Existing terminal-pane ownership stays
  separate. No new widget clock, permanent repaint timer, framework, dependency
  fork, configuration writer, store, permissions or capture-cap changes.
- The existing demand-driven scheduler, 2-ms frame/burst budget and bounded event
  admission remain. The extra Vec is only the current terminal frame, released
  after publication; it is not retained transcript history or a second queue.

## Current-source actual paired proofs

Pinned original source: `2670273ff17da96f85c5826ced57aa1b368754fa`.
Original executable SHA-256:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Captured Git base: `0b5a19e59dd2dcb155dbe04767e780830423d3c1` plus the
source-associated dirty implementation, not an inference from that base alone.
All final profiles recorded successful real `cargo build --locked`, identical
Rust inputs and executable SHA-256
`ccbbbbaf967905a9e88a06e7a495e2e760be051871e31e32cdcfc3b7fe7e315a`.
Each immutable capture.lock/source-manifest/commands receipt retains full inputs,
frontend profile and binary association. Later report/document edits do not
change the qualified Rust or loaded observer/probe sources.

Shared real xterm 6.0.0, Unicode11 0.9.0, Chromium 145.0.7632.6,
Playwright 1.58.2, DejaVu Sans Mono 14/DPR1; WebGL addon exactly 0.19.0 is isolated
test tooling, not a native runtime or replacement for the pinned original.
Its actual bundle/external package lock are hashed and the lock is captured.
The original's admitted cursor config explicitly enables/disables blink;
default preserves terminal-default style. Native preserves frontend defaults.

State order below: idle, hover, restored, Search idle, Search hover,
composer restored. Every input state is observed for 5.6 seconds. Hover injection
is independent 20-ms real SGR input; snapshots never gate the input producer.

| Immutable attempt | Geometry/profile | Native full cycles | Original full cycles |
| --- | --- | --- | --- |
| cursor-temporal-attempt-018 | 160×48 default, collapsed, sync supported | 0/0/0/0/0/0 | 0/0/0/0/0/0 |
| cursor-temporal-attempt-019 | 80×24 blink, expanded, sync unsupported | 3/3/4/3/4/3 | 4/4/3/3/3/3 |
| cursor-temporal-attempt-020 | 120×40 blink, expanded, sync supported | 3/4/3/4/3/4 | 3/4/4/4/3/4 |
| cursor-temporal-attempt-021 | 80×24 steady, expanded, sync unsupported | 0/0/0/0/0/0 | 0/0/0/0/0/0 |

Both sides qualify cursor behavior in all four profiles. Native hover parsed
9,153/1,921/2,046/1,893 commands; native Search hover 1,147/837/532/831.
Original Search hover is not claimed to be continuous repaint: its producer
sends continuous moves but its own renderer emits only about 23–25 commands.
All states preserve final active-owner x/y/visibility/shape and have zero visible
unsynchronized command states outside that owner. Genuine blink is measured
from raw fixed-owner raster RGBA, not parser visibility or desired blink-manager
state. Each full cycle/cadence is checked against the same input owner's idle;
maximum sample gap is ≤ idle period/4, not relaxed after a failed attempt.
The actual Unicode draft plus two Left movements survives Search and restoration.

Each side/profile has 72 actual full temporal renderer-canvas PNGs and eight
settled ordinary Chromium PNGs, with full styled cells/cursor/VT/command traces.
Temporal frames are explicitly timestamped/unsettled; the full opaque terminal
canvas is the renderer's actual output, not a generated model-state image or
cropped/composited browser screenshot. The observer samples the fixed input
position independently of intermediate parser coordinates and never changes
commands, cursor shape, CSS animation or terminal rendering.

`check_cursor_temporal.py` audited all **576** full temporal PNGs: whole-viewport
dimensions and opacity, recorded raw fixed-owner pixel equality, actual build,
Rust input identity, owners, cadence/sampling and no replay. Derived immutable
`cursor-temporal-matched-002/report.json` records **275** observed phase-matched
pairs and **550** strict full grid/PNG comparisons: **all DIFFERENT**.
Raw runner comparisons total 614 DIFFERENT + 52 UNMATCHED_TEMPORAL_SAMPLE,
zero EQUAL; runners exit 1. Unequal phase counts are not equality or parity PASS.
No image/cell normalization, mask, threshold or comparator-validation change.
Captured padded text/raw VT naturally retains terminal whitespace; the Git
whitespace gate excludes only final capture `.txt`/`.vt` data, not code/docs/JSON.
Those immutable bytes are not trimmed to manufacture a clean source check.

The real local fake Responses chain has one title plus five main requests,
four ordinary tool effects, three real MCP calls and exactly one Shell counter
line/17 bytes. These counts and hashes, native read-only operation/resource/
presentation observations and retained artifact bytes remain identical after
the temporal interactions. No generation campaign, user configuration, live
credentials or authoring-agent environment is read or changed.

Normal `tool-preview-attempt-013` separately requalifies fourteen paired stages:
live completion, hover, MCP/error/Shell expansion/re-collapse, resize, `/new`,
reopen and clean same-root restart. Both behavior checks pass, six requests/four
effects remain unchanged. Two native-only bounded `/cards` detail/page frames
have no visible caret. Its 28 full paired grid/PNG comparisons are DIFFERENT;
four native-only entries are explicitly not donor parity claims.

## Experiments retained, not promoted to PASS

- 001: old native boundary had 1,517 visible intermediate command positions and
  only one hover blink cycle. Original effective blink was off, not a valid blink
  reference; its admitted fixture configuration was then made explicit.
- 002: safe staged ordering removed transient cursor, but DOM row replacement
  restarted CSS blink during hover on both products. No CSS-phase workaround.
- 003–009: real WebGL cadence identified; blocking screenshots/CDP/RPC observation
  could starve samples/input. In-browser passive full-canvas acquisition replaced
  that approach. Undersampled or failed/draft-timeout attempts remain diagnostic.
- 010–012: corrected current-parser-coordinate sampling to a fixed real input
  raster. Wide default repaint still exposed actual partial-frame flicker;
  advisory synchronized publication removed it in 013 without weakening fallback.
- 013–017: behavior qualification preceded correct live visible/hidden naming;
  old ordinal phase mismatches were never rewritten. A multi-run outer timeout
  in 015 is incomplete, not an external blocker or successful qualification.
  Failed derived matched-001 is preserved; final 018–021 name actual raster phase
  at acquisition and matched-002 retains strict validation.
- First broad gate chain hit the observed 600-second wrapper deadline partway
  through ordinary passing suites. No owned test/cargo process remained; final2
  used an 1,800-second bound and completed, rather than relabeling partial PASS.

## Current-source gates and measured resources

Final2 log: `.local/t44-cursor-gates-final2-20261008.log`.
`cargo fmt --all -- --check`; strict locked workspace/all-targets Clippy;
locked workspace tests: **1,731 passed / 0 failed / 11 unchanged opt-in ignores**,
46 completed records (TUI459, binary97, PTY52, MCP41, adapters670, runtime120,
subagents39 included). Locked/unlocked debug builds, locked release build,
debug/release `--help` and real release startup/discovery scripts all PASS.
Normal/error/panic cleanup, failed stdout, missing/corrupt selection, mandatory
policy/config refusal/retry and discovery failures preserve terminal restoration
and no unauthorized Responses calls. No ignored live campaign is run or reset.

Extra current actual-binary `--nocapture` resource log:
`.local/t44-cursor-resource-gates-20261008.log`. Existing VIS31 idle/fairness,
S07 and AUD32 filters pass without raising any threshold:

- Settled idle: 1.000389 s, zero terminal bytes, zero CPU ticks, no periodic draw.
  165/250-Hz injection p50/p95/max = 2.123/3.271/3.368 ms and
  2.281/6.017/7.428 ms. Real provider burst = 14.337/21.509/23.468 ms and
  13.322/21.171/21.701 ms; each remains below the existing 100-ms admission bound.
- S07 equal active view, 0→3,000 older rows: RSS 68,320→69,364 KiB,
  PSS 66,070→67,078 KiB; retained view22,896 bytes/152 rows on both sides,
  queue peak10/6, lag0, own children0, live text/reasoning/parts settle to0.
  Markdown cache7,517 bytes on both, existing bound5,242,880 bytes.
- AUD32 equal active context, 8→3,000 archived pairs: peak RSS56,956→58,368 KiB,
  PSS54,331→55,699 KiB; DB942,080→152,944,640 bytes, children0/eight threads.
  Peak growth1,412 KiB is below the unchanged64-MiB bound, not retained history.

## Open qualification / delivery

This closes the narrow functional caret-ordering/blink risk with current-source
proof, not full VIS16/VIS31/R6/V09. Full unmasked styling/text/geometry comparisons
remain DIFFERENT and every other frozen R1–R6/VIS01–VIS45 obligation remains.
AUTH06 stays deferred; original24/24 and Go13/24 ledgers and unrelated `.opencode/`
remain untouched. No task finish/acceptance/baseline reset or paid campaign.
Next responsibility: Home's typed live MCP footer slot, then remaining frozen
styled/temporal/interaction outcomes and final current-source gates.
