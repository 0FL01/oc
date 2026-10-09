# T44 combined composer, Shell output dialog and real terminal — qualified slice

Whole T44 and whole VIS39 remain **ACTIVE / NOT_PASS**. This receipt qualifies
the shared consumer and independently checked effects, not full pixel parity,
all syntax assets, or the remaining frozen R1–R6/VIS01–VIS45 outcomes.

## Existing owners and preserved contracts

- One `oc-tui/src/composer.rs` visibility/input/paint owner supplies Subagents,
  Shell and, only when resolved `session.terminal` permits it, Terminals. Existing
  ChildView, ShellView and TerminalView retain inventories, selections, captured
  operation/session/Location/generation and effects. No new inventory, executor,
  store, polling timer or dynamic command registry was introduced.
- The common raised surface has a five-row viewport, semantic focused/selected
  colors, tab-specific effective hints, Left/Right wrapping, and paired painted
  row/esc pointer targets. Tab labels are not buttons. Esc/Ctrl+C closes without
  killing; Ctrl+D affects only the selected running child/Shell. Remapped and
  disabled bindings, leader prefixes, destructive Repeat, stale releases and
  approval/question priority are tested through real UI consumers.
- Shell inventory is exact-source-session, not the whole family or immutable tool
  results. Foreground/background conversion keeps the same admitted execution.
  Linked-child fact refresh no longer reopens Subagents over a chosen Shell tab
  or viewer. The retained root deck/title is projected onto a child without
  mutating the parent draft/deck or allowing a new editable root from that route.
- `shell_jobs_view/output.rs` is a child of the existing ShellView, not a new
  resource owner. The centered read-only dialog retains the opened job after list
  removal, reads the existing bounded ShellSnapshot, and shows typed status/exit,
  plain recent output, omission/error feedback, scrolling and follow-tail. Final
  flush and a real trailing newline are preserved. Closing retires scoped reads;
  mouse/modal input cannot activate the underlying prompt, tabs or PTY.
- Existing Capture now retains a 64-KiB ordered admitted display tail and frozen
  Outcome facts for the dialog. The standalone preview remains 2048 bytes, with
  bounded independent stream previews. The additional hot tail is approximately
  62 KiB per existing maximum-eight active jobs; original capture/resource limits
  are unchanged. Publication order is not claimed to be OS write order.
- A stateful `anstyle-parse` 1.0.0 parser discards split CSI/OSC/DCS and normalized
  UTF-8 C1 controls before the existing incremental secret admission. Its `core`
  feature gives an inline 1024-byte OSC buffer. Cargo.lock changes only two edges
  to already locked packages (`anstyle-parse`→`arrayvec`, adapters→parser); no
  package/version download was needed. No escape payload is replayed to the host.
- TerminalView still owns the real vetted VT emulator, terminal reference, replay
  and focus. The right pane is raised, borderless, full-height with horizontal
  inset one. Raw Ctrl+C/D stays raw; higher forms own keys and pointer input.
  Explicit focus-right does not close/navigate a child. `terminal.close` hides,
  not kills; raw EOF and the existing explicit Core remove/reap are separate.
  A real replay gap uses one existing transient warning without refreshing its
  deadline on subsequent resets. Native PTYs live with the OC process, unlike the
  donor service; this does not waive geometry or safety.

RAW messages, provider/tool graph, permission/trust narrowing, original execution
provenance, child/root drafts, descriptor/capture/queue bounds and no unknown-effect
replay remain intact. No user/authoring configuration, env, credentials, GOAL,
acceptance or AGENTS was edited; all profile configuration is isolated fixture data.

## Actual paired proof and association

Five built captures independently recorded the same source/ELF association:

| Fact | Value |
| --- | --- |
| Base HEAD | `3c24f6510b00ceecd8f8fb408bcf55a65150b093` |
| Base tree | `424663652d49d32d9ddf359f78dd706be41410ce` |
| Captured diff SHA256 | `492ed6b8e88e28d94669d7b81bd4ab997b8bf779489c593940a8c76e510b519a` |
| Source manifest SHA256 | `2dedf803ad6af725f4c551bb3b81fea5ed4632a1c8aa27b765f8ed5879364e52` |
| Actual debug ELF SHA256 | `ab23c4a445013977ef67a04746f76f8cddb117f256ea179885b585a4845c9eb5` |
| Capture build | `cargo build --locked` through `--build-oc true` |
| Pinned original | v2.0.12, `2670273ff17da96f85c5826ced57aa1b368754fa` |

Final immutable directories:

- `combined-composer-008/` — 80×24;
- `combined-composer-009/` — 120×40;
- `combined-composer-010/` — 160×48;
- `combined-composer-tool-regression-002/` — 120×40;
- `combined-composer-temporal-001/` — 80×24;
- `combined-composer-temporal-matched-001/` — independent opaque-raster audit.

Each combined run captured eighteen actual stages per side: real delegation,
parent draft, linked child, shared tabs, running output/Home/Page/follow/final
flush, second-job kill, parent restoration, terminal undefined selection/create,
raw ANSI input/effect and hide/show. Both sides qualify behavior. Each has **36
DIFFERENT** strict full-grid/PNG comparisons, all valid and stable, not pixel PASS.

Each origin has three actual function calls (one subagent, two Shell), two Shell
starts, one completion and one actual PTY effect. Native has six HTTP requests;
original has eight. Native read-only SQLite witnesses verify the exact root/child
relationship, source/Location/generation/nonempty turn, identical stored process
identity before/after foreground conversion, first-job completed exit0/final
flush, second-job cancelled rather than timeout, undefined terminal Enter no-op,
and the same persisted terminal through hide/show. Original RAW is not decoded;
its actual external effects and controlled wire graph are checked independently.

The profile uses a finite fixture-only filesystem release gate and deny-all plus
exact helper/Shell resources. Input scope differs genuinely: the original opens
the busy parent picker through its painted status and backgrounds from base
scope; native routes captured controls through its owner. Only combined stable
stills use the existing owned process-group pause/drain ACK, actual post-ACK frame
hash and finally-resume ACK. This is not animation/cadence or phase alignment.
Normal and temporal regressions are not paused. No masks, crops, tolerance,
version/example substitution, source goldens or forced frontend refresh are used.

The observer now exports the physical pending-wrap cursor used by both real
cached DOM/WebGL renderers, retaining raw logical `x == cols` in render geometry.
Actual browser checks retain all previous RGB/styled-blank/attributes/Unicode/DSR
tests and prove raw8→painted7/export7 with all eight cells, hidden/show/newline and
out-of-range9 refusal. The strict comparator and bounds are unchanged.

Normal regression: both 27 stages, six requests/four effects/three MCP calls,
17-byte model Shell effect and no switch/reopen/restart replay; **5 EQUAL / 49
DIFFERENT**, plus four native-only authorized resource-detail stages. Default
WebGL/collapsed/supported temporal control qualifies six owners on both sides,
zero cycles appropriate to default (not blink proof), no phantom/lost caret/draft
or replay. All160 comparisons differ. Independent audit:144 opaque PNGs,72 phase
pairs,17,694,720 pixels per side; `QUALIFIED_CURSOR_BEHAVIOR_ONLY`, pixel NOT_PASS.

## Current checks

All Cargo checks used jobs3/test threads2 and the approved owned TMPDIR, serially.

- fmt and strict locked workspace/all-target Clippy `-D warnings`: PASS.
- `cargo test --locked --workspace --no-fail-fast`: **1784 passed / 0 failed /
  11 unchanged opt-in ignores**,46 results. New coverage includes shared composer,
  asynchronous current-child selection, original-owner dialog/scroll/pointer,
  PTY permission/question precedence, held-process final flush and bounded ANSI.
- Locked debug, ordinary debug and locked release builds; both actual ELF help:
  PASS. A disappeared release artifact was physically confirmed and safely rebuilt
  from unchanged source; no source/external-account workaround was used.
- Actual release startup19, GET-only discovery8 and actual release TERM01: PASS,
  including terminal restore/no Responses, raw Ctrl+C/D, host escape suppression,
  resize, captured child/PTY identity, hide-only close, shutdown/crash/no replay.
- Python47, Node/Python syntax, actual DOM/WebGL browser observer, source/docs/
  progress checks and advisory code-size review: PASS.35 code files; existing
  5425-line application facade is unchanged and its documented seam retained.
- Exact current resource guards: idle1.000588663s/CPU0;165/250-Hz p50/p95/max
  2.972/5.464/5.962ms and8.529/16.461/20.698ms; provider burst
  37.956/67.456/73.360ms and22.160/37.103/37.631ms, below unchanged100ms.
  Queue peaks140/93, no lag, settled live0. Equal-view archive0→3000 retains
  22,896 bytes/152 rows/cache7517 unchanged; RSS68,640→67,432/PSS66,437→65,172KiB,
  no children. AUD32 archive8→3000 peak RSS56,344→62,992KiB (+6648), threads8,
  children0; unchanged64-MiB guard PASS.

Earlier attempts are not qualified artifacts: combined001–007 exposed actual
scope/refresh/counter/cursor/viewport issues; normal001 exceeded the external
300-second runner budget, while new002 completed with600 seconds without changing
an application deadline. Two earlier unchanged burst checks failed the100ms guard;
a diagnostic-message-only experiment passed, did not establish their cause, and
was completely removed. Final four original serial resource checks pass with no
guard/test/baseline change. This is not a claim of host-load invariance or a proven
performance fix. All diagnostic captures remain immutable and are not delivered.

## Still mandatory

Actual native child model-Shell cards lack the donor's live partial body/background
hint; child task display exposes the native context-pack wrapper; parent automatic
background completion is still a technical RAW USER block rather than the typed
Helper-finished notice. Root Add/captions/background badges/underlay timing,
grammar/assets and all remaining frozen outcomes require current implementation
and paired qualification. No whole VIS39 or T44 finish is justified by this slice.
Next: typed projections from the existing child/job/message facts without RAW
rewrite, provider graph fabrication, authority relaxation or replay.
