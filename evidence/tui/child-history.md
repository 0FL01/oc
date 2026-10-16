# T44 typed child task and completion link — qualified slice

Date: 2026-10-09. Whole R1–R6/VIS01–VIS45/VIS39/T44 remains ACTIVE/NOT_PASS.
This is a display/interaction qualification, not full pixel parity or task finish.

## Owners and preserved contracts

- `HistoryMessage.child` carries an original structured task or an exact delivered
  child notice. Existing application History and fenced ReadChild queries populate
  it; provider history, accepted RAW, tool-call/result graph and execution stay
  unchanged. No new table, store, query variant, timer or executor was added.
- `storage_children.rs` links task display through the actual child turn,
  acceptance, parent Subagent operation, structured arguments and child identity.
  It does not strip host/context-pack delimiters from free text. The display reads
  at most65536 bytes of the original prompt and preserves complete UTF-8; a limited
  display says `Task preview`. Literal delimiter-like task text remains data.
- Notice classification requires the exact message ID, parent/child relationship,
  Subagent operation, terminal state, bounded decoded identity and delivery event.
  Current-branch visibility is checked independently. A copied JSON-looking user
  message, foreign/corrupt identity or missing event does not become a notice.
  Result prose is unnecessary for the captured navigation fence and is omitted.
- History rows retain actual message ID/sequence and bounded notice identity.
  Notices are one clipped, sanitized line, not User blocks, assistant/tool parts
  or Revert/Fork targets. Typed terminal state supplies feedback; heading hover
  changes only non-error/non-cancelled foreground, and description stays muted.
  `finished` is a technical terminal label, not inferred semantic task success.
- The shared indexed transcript path supplies paint height, hit and hover. Only a
  matching unmodified Left Down/Up, current frame/paint generation and captured
  job can emit existing OpenChild. Bare release, drag/selection, key interruption,
  resize, removed notice and higher-priority surfaces cannot activate old content.
  The existing runtime validates the current operation/Location/generation fence.
- Busy Root and ScriptDriver notice routing uses the existing exact-message query
  and a bounded recent page when needed. It correlates the acknowledged current
  user by exact turn identity, without copying executing assistant parts. Selective
  replacement preserves draft/live overlay and chronological paging/gap guards.
- Undo/Revert/Redo excludes only positively qualified native notices from ordinary
  prompt selection/counting. Real saved context and entire immutable RAW/delivery
  remain; unlinked legacy User rows still refuse unsupported Undo.

## Current source and executable association

All five built capture locks were independently read and agree:

| Fact | Value |
| --- | --- |
| Source base | `526e4a63ab82e496daf039efe36f1eea7ba9a057` |
| Base tree | `6717bd11ec958abd2b7df878b104869d14e60487` |
| Captured dirty diff | `405c8eeb599b9bfc5ed0e94cb2435ab43b54b44aa322c234c8b3eb38d4fad684` |
| Source manifest | `be85cab40a30b36500488149c8c01a20cc0ab2f508747d76f7897dc11a896446` |
| Actual debug executable | `5d92158afcdf972d6460526994891519dc8b0f3d6140372ca4a7790994bec0d7` |
| Build command | `cargo build --locked` |
| Original | pinned `2670273ff17da96f85c5826ced57aa1b368754fa`, v2.0.12 |
| Combined runner | `c3f875be039a4300af3c9c6e28be56c85e9ca4a44831a4ebd3d3b13c14e24d07` |
| Combined fixture | `541e8239a7707905179ef5349e1a99a52ead77f016abe11c3c685d1332f28fcc` |

These immutable locks precede this factual documentation/checkpoint. They are not
rewritten to a later documentation commit. Git determines reviewed delivery status.

## Actual executable proof

Qualified artifacts, all under `evidence/tui/`:

- `child-history-002/`:80×24, both20 actual stages,40 strict DIFFERENT comparisons.
- `child-history-003/`:120×40, both20 stages,40 strict DIFFERENT comparisons.
- `child-history-004/`:160×48, both20 stages,40 strict DIFFERENT comparisons.
- `child-history-tool-regression-001/`: ordinary120×40 model-tool/MCP regression.
- `child-history-temporal-001/`:80×24 default/WebGL/collapsed/supported control.
- `child-history-temporal-matched-001/`: independent full-raster temporal audit.

The real combined profile adds two stages to the historical18-stage flow: paired
click on the painted completion caption reopens the original completed child, then
Escape restores the same unfinished parent draft. Both sides have three actual
function calls (one delegation, two Shells), two starts, one completion and one PTY
effect; native six HTTP requests, original eight. Native read-only SQLite witnesses
retain exact root/child/operation/Location/generation, identical same-process
conversion, final flush, selected second-job cancellation, undefined terminal Enter
no-op and same terminal hide/show. The native `typed_child_notice_link` witness is
true; original RAW is not decoded. Reopen/click does not dispatch or replay work.

Final native/original120 parent-restored and160 notice-child PNGs were physically
viewed. Technical background JSON no longer appears as a User block; heading and
muted description are separate, and original requested task replaces native host/
quoted-pack display. Full frames nevertheless differ: original child User text also
has its own human host prefix; profile colors, model-Shell content/status/badges,
parent background caption, extra original continuation responses, timing/underlays
and root Add behavior remain visible. These differences are not masked or waived.

Only the combined profile's stable stills use the existing owned process-group
pause/drain ACK and finally-resume ACK; no animation/phase-alignment claim follows.
All full styled cells, cursor and opaque PNGs remain. Capture exit1 reports strict
remaining differences, not failure of the independently checked effects/links.

Ordinary regression: both27 stages, six requests/four tool effects/three MCP calls,
17-byte model Shell effect, switch/reopen/restart without replay; four EQUAL and50
DIFFERENT comparisons, plus four authorized native-only capture-detail comparisons.
Default temporal control: both six owners, zero phantom states, caret/draft restored
and no replay. Independent audit qualifies144 opaque PNGs/72 matched actual phases,
17694720 pixels per side;160 DIFFERENT comparisons, pixel NOT_PASS. All cycles are
zero, appropriate for DEFAULT, not evidence of blink cadence. Original command
counts `[0,12721,0,0,25,0]`; native `[0,9371,0,0,1382,0]`. No forced refresh.

## Current gates and measurements

Commands used `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2` and owned
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`; Cargo runs were serial.

- Workspace fmt and strict locked all-target Clippy `-D warnings`: PASS.
- `cargo test --locked --workspace --no-fail-fast`:1790 passed,0 failed,
 11 unchanged opt-in ignores,46 results. Six new substantive cases cover exact
 task/notice/RAW/recovery/bounds, Undo/Revert/Redo, indexed cells/typed interaction
 and busy-router identity/live overlay. Actual ReadChild integration also verifies
 original task versus accepted RAW and exact typed notice without retry replay.
- Debug locked/ordinary builds, release build and both actual ELF help: PASS.
 The combined command budget expired during release compilation after the tests
 and debug builds passed; the unfinished same-source release build was resumed
 separately and completed. The expired invocation is not claimed as completed.
- Actual release startup19, GET-only discovery8 and TERM01: PASS; terminal restored,
 no unauthorized provider calls. TERM01 retains raw controls/VT/resize/source/focus,
 hide/last/shutdown/crash/no-replay. RSS17340→20536KiB, threads8→10/fds24→26 for two
 PTYs; pane sizes23×38,47×78,48×57, idle0 ticks/300ms.
- Python47 tests, actual DOM/WebGL RGB/attributes/Unicode/DSR and pending-wrap
 observer, Node/Python syntax, docs/progress structure: PASS.
- VIS31 idle1.004485403s/CPU0.165/250Hz p50/p95/max3.851/8.338/8.457ms and
 6.120/10.436/11.233ms. Provider burst23.269/50.832/50.887ms and
 13.066/23.813/27.605ms; queue peaks87/64, no lag, settled live0.
- Equal-view archive0→3000:RSS69152→68332/PSS66937→66121KiB; retained22896 bytes/
 152 rows/cache7517 unchanged, queues7/7/no lag, children0, frames90/79,
 elapsed2645/2785ms. AUD32 archive8→3000 peakRSS57548→60236KiB(+2688),
 peakPSS54917→57657/endPSS48917→52661; children0/threads8, DB942080→152944640.
 Original100ms pacing and64MiB archive-growth guards were unchanged and passed.
- Advisory changed-code footprint33 files/55094 physical lines. Application facade
 5433(+8) wires existing query facets; coupled owner and next query/transition seam
 remain documented. No cosmetic split or public test-only hook.

## Review and next contract

All substantive source/tests, constructor-only additions and actual artifacts were
reviewed; independent focused review found no confirmed regression. Diagnostic
`child-history-001/` preserves the observed pre-fix description-color mismatch and
is not delivered as current qualification. No env/user config/GOAL/acceptance/RAW
rewrite, new dependency, paid campaign or unknown replay was introduced.

Next: consume existing model-Shell live facts, parent Subagent/background captions,
profile/header differences and grammar/assets, then every remaining frozen outcome
and final current-source R6/V09. This slice does not finish VIS39 or T44.
