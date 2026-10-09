# T44 model Shell capture and plain notices — qualified slice

Date: 2026-10-09. Whole R1–R6/VIS01–VIS45/VIS39/T44 remains ACTIVE/NOT_PASS.
This qualifies bounded display, ownership and actual interaction, not full pixel
parity, semantic task success or task completion.

## Owners and preserved contracts

- Existing supervised Shell captures now supply bounded `ShellJob.output` for
  model operations as well as direct-user commands. Association requires the
  original session, operation and nonempty issuing turn; current live parts also
  require the present matching worker turn. Inventory cannot create a tool part.
- `tool_output::Shell` carries actual background conversion and optional frozen
  process state, with backward-compatible defaults and validated terminal states.
  Logical model-tool completion is not process exit. Recorded command/input,
  logical tool state, RAW handle, provider results and graph remain unchanged.
- Foreground cards show publication-ordered process data in muted text. Actual
  background cards collapse their body, remain expandable and use the pinned
  `Background` badge styling. Expanded cards expose actual bounded output, not
  generated transport labels; literal `[stderr]`/`[stdout]` payloads stay data.
- Final flush uses an existing captured ShellSnapshot after exact current-branch
  typed-notice qualification. Known terminal facts reject late running inventory.
  No polling, extra execution, new query variant, registry or store was introduced.
- The original worker freezes ordered display body plus separate stdout/stderr,
  exit/signal and actual supervisor state. Existing terminal transactions record
  presentation for model operations without changing their logical result. Cold
  projection prioritizes exact owned frozen facts over late logical-handle metadata;
  fork/reopen preserves display, not process/file authority. Recovery reports
  unknown/interrupted output and never invents successful exit or replays effects.
- Frozen projection and model-notice classification require integer version1,
  valid bounded JSON, exact session/operation/turn and known terminal states.
  Boolean/real JSON versions do not qualify through SQLite numeric coercion.
- `HistoryMessage.shell_notice` is separate display-only structured data. Positive
  classification requires the exact delivered message, Shell operation, captured
  provenance/outcome and delivery event plus current-branch visibility. Copied RAW,
  hidden/foreign/corrupt identity and missing events do not become notices.
- Model Shell notices render as one sanitized clipped line: semantic terminal
  heading plus always-muted recorded command. They are neither User instructions,
  assistant/tool parts nor links. Original RAW/provider notification text remains.
  Undo/Revert/Redo skips only qualified data notices and retains complete RAW and
  delivery; ordinary unlinked legacy User rows still refuse unsupported Undo.
- Root and ScriptDriver consume exact typed facts before any capture lookup. Busy
  selective refresh retains current parts/draft; idle refresh loads the existing
  bounded tail before selective replacement. Direct-user and child ownership remain.
- Existing2KiB presentation/960-byte stream previews,64KiB captured viewer,
  bounded inventory,240-row/256KiB history and live-part budgets remain unchanged.

## Current source and executable association

All eight built capture locks agree; immutable locks precede this factual receipt
and continuation checkpoint. They are not rewritten to a later documentation commit.

| Fact | Value |
| --- | --- |
| Source base | `9a3ca85e5dd9a733b6b1e85575a42c779df53c12` |
| Base tree | `e86d26fc63ecff9d2192713c7a4446630a14bb62` |
| Captured dirty diff | `47bf530fbae8318a5df6443421c63673e54a9db0eb23304fc7572fd7b319a590` |
| Source manifest,527 entries | `9801d5ec364487dd11f49a26359f1f6afa6419151c9dcfe26635b5862f3bb0e2` |
| Actual debug executable | `b9e3b036451b6fe3ed18c5686ec88c6ad8d24c0e28e32319b80789de769ed9b6` |
| Build per capture campaign | one successful recorded `cargo build --locked` |
| Original | v2.0.12, pinned `2670273ff17da96f85c5826ced57aa1b368754fa` |
| Original executable | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

## Actual paired proof

All paths are under `evidence/tui/`; only these final campaigns are qualified:

| Campaign | Geometry | Actual captures | Direct strict grid/PNG comparisons |
| --- | --- | --- | --- |
| `model-shell-023/` |80×24|8 stages/side,16 captures|16 DIFFERENT |
| `model-shell-024/` |120×40|8 stages/side,16 captures|16 DIFFERENT |
| `model-shell-025/` |160×48|8 stages/side,16 captures|16 DIFFERENT |
| `model-shell-child-regression-009/` |80×24|20 stages/side,40 captures|40 DIFFERENT |
| `model-shell-child-regression-010/` |120×40|20 stages/side,40 captures|40 DIFFERENT |
| `model-shell-child-regression-011/` |160×48|20 stages/side,40 captures|40 DIFFERENT |
| `model-shell-tool-regression-006/` |120×40 plus real resizes|27 paired stages plus2 native-only details,56 captures|4 EQUAL,50 DIFFERENT;4 authorized native-only entries |
| `model-shell-temporal-002/` |80×24 default/WebGL|80 captures/side,160 total|160 DIFFERENT |

Root stages are home, foreground, background, background-expanded,
background-collapsed, background-output, final-output and second. Both sides make
exactly two structured Shell calls/four HTTP requests, two starts and one completion,
without children or PTYs. Native original session/operation/provenance and PID stay
the same across foreground admission, real conversion and completed background
exit0/final flush. The selected second stage is captured while running; actual Ctrl-D
then supplies its cancellation witness. Reopen/expand/collapse never replays work.

Input scope is declared, not assumed identical: native captured output-viewer Ctrl-B
versus original focused Root prompt Ctrl-B; native three actual wheel ticks per
viewport step versus original PageUp/PageDown. The wheel burst avoids per-tick
observer IPC consuming the unchanged120-second process timeout. Fixture release is
finite180 seconds; the0.2-second stdout/stderr separation affects only observation
of the fixture's visible tail, not process capture ordering or production limits.
Each finite viewport step must actually move text; final still-shot gates remain
unchanged. Stable combined stills use owned PG pause/drain ACK and finally-resume,
not claimed animation-phase synchronization. Original RAW is not decoded.

Child regressions retain three calls, native six/original eight HTTP requests, one
child and two child-owned Shells, captured completion-notice navigation/restored
parent draft, selected Shell cancellation and one Root PTY write effect. Source,
selection, terminal hide/show and no-replay witnesses remain qualified.

Ordinary regression retains four operations/six HTTP requests/three MCP calls and
one17-byte/one-line Shell effect across reopen/restart. Five presentation events
mean one per MCP operation plus two identical actual foreground Shell presentations
(logical and frozen, completed/backgroundfalse/exit0), not a fifth tool call.
Actual120×120 Shell expansion exposes all80 short native body rows plus chrome;
draft/caret x/shape and bottom-pinned y translation remain checked through resizes.

Default temporal proof has six owners,72 temporal PNGs/side and no phantom command,
sampled phantom caret or replay. All cycles are zero, appropriate for DEFAULT—not
blink-cadence qualification. `model-shell-temporal-matched-002/` independently checks
144 opaque PNGs/17694720 pixels per side and72 unique full-frame pairs,144 strict
grid/PNG comparisons, all DIFFERENT. Original command counts are
`[0,12649,0,0,25,0]`, native `[0,9290,0,0,1362,0]`.

## Complete raster and integrity review

An independent audit found tall historical PNGs clipped to the1100-pixel browser
viewport despite complete cell grids. This was an observer failure, not acceptable
qualification. Capture now requests the same full document-coordinate clip with
`fullPage:true` and rejects any PNG whose dimensions differ from the full clip.
There is no browser/terminal resize, CSS normalization, forced buffer refresh,
mask, tolerance or comparator relaxation. The geometry experiment independently
demonstrates the old1100-height crop and verifies complete80/120-row1280/1920-height
rasters with identical before/after styled grid and geometry.

Final `python3 -B evidence/tui/model-shell-integrity.py` PASS verifies all eight
locks/builds,527 current source entries each, actual binaries,384 complete opaque
captures and1152/1152 cells/PNG/render seals. Every full PNG dimension matches its
recorded clip; ordinary includes14 full1011×1280 and2 full1011×1920 rasters.
There are no missing/unstable/invalid pairs. Direct paired total is382 comparisons:
378 DIFFERENT and4 EQUAL; four native-only entries are not paired comparisons.
The independent audit additionally recomputed the new Root/child/temporal direct
comparisons and matched full-frame temporal pairs.
Byte-exact VT/text exports intentionally retain terminal blanks; staged whitespace
checks cover source, documentation and JSON, without trimming captured frames.

Native/original ordinary expanded full1920-height PNGs, Root80 collapsed/expanded
background and actual exited/final-flush output modal were physically viewed.
Original12-line prepared tail versus native actual bounded80-row body, original
Root Add/hints/context, native preview-limited footer, timing, underlays and colors
remain visible. These are not masked or waived and do not establish full parity.
Historical Root001–022, child001–008, ordinary001–005 and temporal001 remain
immutable diagnostics/noncurrent, especially the cropped ordinary005 images.

## Current gates and measurements

Cargo was serial with `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2` and owned
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`.

- Fmt, strict locked workspace/all-target Clippy `-D warnings` and diff check: PASS.
- `cargo test --locked --workspace --no-fail-fast`:1797 passed,0 failed,
  11 unchanged opt-in ignores,46 results. Seven added substantive cases cover
  live/background association, frozen result ordering/reopen/fork, Root routing,
  exact notice/Undo/Redo/RAW, plain typed rendering, idle ScriptDriver refresh and
  retained orphan-live-part fencing. Existing real held-process/conversion and
  SIGKILL durability cases pin complete typed stream/unknown facts without replay.
- Actual debug locked/ordinary builds, release build and both ELF help: PASS.
- Release startup19 isolated routes,8 GET-only discovery cases and TERM01: PASS,
  no unauthorized provider calls. TERM01 RSS17704→21404KiB, threads8→10,
  fds24→26, two-PTY geometries23×38/47×78/48×57; idle0 ticks/300ms.
- VIS31 idle0 ticks/1s;165Hz p50/p95/max0.738/3.603/5.013ms and250Hz
  0.584/1.802/3.028ms. Provider burst4.648/6.891/8.597ms and5.014/7.503/8.421ms,
  queue peaks23/32, no lag. Original pacing guards remain unchanged.
- S07 equal-view archive0→3000:RSS23572→24468/PSS21345→22163KiB,
  retained22928 bytes/152 rows/cache7517 unchanged, elapsed1526→1518ms.
  AUD32 archive8→3000 HWM19756→21452/PSSpeak17005→18662KiB,
  DB942080→152944640 bytes, children0; original64MiB growth guard passed.
- Python47, actual DOM/WebGL RGB/attributes/Unicode/cursor/DSR/pending-wrap,
  capture geometry/syntax, docs/progress structure and complete integrity audit: PASS.
- Advisory code size retains the coupled application facade5441 and runtime turn
  5227 lines; existing query/transition seams remain the natural next split. No
  cosmetic count reduction or public test-only hook was introduced.

Earlier broad attempts exposed stale query mocks and two temporal failures in
unchanged MCP shutdown/TERM01 fixtures. Exact unchanged targeted checks and the
subsequent complete same-source no-fail-fast gate passed; no timeout, assertion,
baseline or test was disabled to obtain the final gate.

## Review and next contract

All substantive production/tests, constructor-only additions and final artifacts
were reviewed; focused independent code reviews found no confirmed regression.
No env/user config/GOAL/acceptance/Cargo.lock/RAW rewrite, new dependency, paid
campaign, execution owner or unknown replay was introduced.

Next: typed Subagent/background/model/profile and Root caption presentation,
human host prefix, grammar/assets and every remaining frozen mandatory outcome,
then final current-source R6/V09. This slice does not finish VIS39 or T44.
