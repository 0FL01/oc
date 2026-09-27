# T44 / VIS35 actual patch capture — 2026-09-27

**DIAGNOSTIC / NOT VIS35 PASS.** The real executors, independent bytes/presence,
and conversation-only replay/restart checks passed in eight completed paired
runs. All 176 comparable full frames differ (352 grid/PNG comparisons).
Native argument-stream running frames were not observed. VIS36 accept/reject,
real-model A09 authorship, and release entry qualification remain open.

## Inputs and source association

- Repository HEAD: `318025abd1d535f8229c9da53b40e375f5610cbf`, with existing
  dirty backend/core/TUI implementation. This worker changed capture tooling,
  documentation and new evidence only; no Rust implementation edits.
- `cargo build --locked` succeeded once directly and in every capture attempt
  through `--build-oc true`. Each attempt preserves the actual command/output,
  source manifest (including untracked Rust modules), tracked dirty-diff hash,
  executable SHA and runner hashes. All ten attempts have identical Rust/Cargo
  input hashes; attempts02–10 use native executable SHA-256
  `7ff96de98e2792f160b179ecee914ca84db8fd661668f981bd9525c2c15f77f5`.
- Original executable SHA-256:
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
  pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`.
  U19: `opencode/packages/core/src/tool/plugin/patch.ts:19–45,85–290,296–310`.
  Attempt10 additionally records this source file's SHA in `capture.lock.json`.
- Original fixture-only context hook re-admits **the existing bundled Core
  `patch` executor** after its model-name filter. It only supplies the U19
  one-required-string admission schema and removes Write/Edit definitions.
  Actual `execute.before`/`execute.after` hooks record calls and genuine
  completed/error results. No tool executor, result, transport, or renderer is
  substituted. Native keeps its ordinary `apply_patch(patchText)` registry.
- Temporary fixture directories are under the owned external tooling
  `t44-reference/runs/`; setup checks root ownership and containment. Permission
  rules deny all other actions/resources, allow a fixed exact fixture path list,
  and keep `denied.txt` denied. Snapshots are disabled. The repository's inherited
  `.opencode/` was never inspected, modified or staged. No live credentials used.

## Immutable attempt history

All directories are `evidence/tui/recovery-v00/apply-patch20260927-NN`.
Every run uses short fixture, sidebar hide, geometry, 40 rows, the explicit
original/native executables above and `--build-oc true`.

| NN | Columns | diff view / wrap | Actual capture result |
|---|---:|---|---|
| 01 | 120 | default | External 120s watchdog interrupted before native launch; exit2. Original executed all10 calls. Regex-only initial running shots can actually be completed: not running qualification. |
| 02 | 120 | default | External 360s watchdog interrupted native restart capture; exit2. Original lifecycle PASS; native executed all10 calls and Redo invariants, but restart not qualified by this attempt. |
| 03 | 120 | default (auto) | Both behavior/replay PASS; exit1: 44 DIFFERENT, 20 missing-running BLOCKED comparisons. |
| 04 | 121 | default (auto) | Same result. |
| 05 | 120 | unified / none | Same result. |
| 06 | 121 | unified / word | Same result. |
| 07 | 160 | split / none | Same result. |
| 08 | 80 | unified / word | Same result; native persisted effects table also captured. |
| 09 | 121 | auto / none | Both behavior/replay PASS; exit1: 44 DIFFERENT, 24 BLOCKED comparisons; current-file unique hover targeting. |
| 10 | 121 | auto / none | Same result, additionally aligned `session.tps=false` on both binaries, sealed U19 source association. |

Failures01/02 and earlier semantic limitations remain intact. The bounded runner
now waits for queued VT writes before terminal reset/relaunch and uses a 5s
write-callback watchdog for this mode. Attempts03–10 completed normally; no
production changes were made to resolve the capture interruptions.

The current hover probe requires a unique visible path belonging to the current
operation and records its actual cell coordinates. Historical multifile-hover
in01–08 targeted the preceding `replace.txt` card: it does **not** qualify hover
of `multi-a.txt`. Attempts09/10 correct this without rewriting prior evidence.
Original error-card paths permit additional error hovers not available in the
native path-less error card; these remain missing-pair BLOCKED comparisons.

## Actual requests and filesystem proof

Newest immutable aggregate: `apply-patch-analysis20260927-03.json`.
Earlier analysis01/02 are retained and cover their then-existing attempts.

- **399 actual local fixture provider requests, 399 completed responses,
  190 emitted tool calls, 0 invalid requests**, including failed captures01/02.
- Each executed side: **21 requests / 21 completions / 10 tool calls**,
  consisting of one title request and twenty patch requests (call + real
  result-bearing continuation per case). Each completed side has ten actual
  model-visible results. No transport adapter was introduced; these measured
  counts are not a general transport-count equivalence claim.
- Original final hooks per side: ten real executions, **7 completed / 3 error**.
  Advertised tool definitions are ordinary function schemas with required
  string `patchText`, not provider-hosted Responses apply_patch. Native does
  not expose Write/Edit as mutation alternatives.
- Create `created.ts` (including a long wrapped/clipped syntax line), empty
  `empty.txt`, two separated hunks at lines8/20 in `update.txt`, delete,
  move `move.txt → moved.txt`, full replace, and multi-file add: exact
  hex-byte/presence checks PASS in all completed pairs. Deleted/source files
  are independently absent and destination bytes match.
- Existing update mode **0640 preserved** in both. Original move destination
  is **0664** under this host's creation policy; native preserves **0751**.
  This is a measured mode difference, not exact filesystem-mode parity PASS.
- Multi-file stale fixture: **both binaries reject in preflight**. `prefix.txt`
  is absent; `stale.txt` remains `actual stale\n`. Native's error says
  `partial op 1`, but this run has zero committed prefix/effects. Single-file
  stale and configured denial also preserve exact sentinel bytes. No claim of
  PTY postcommit success-prefix partial/error, cancellation or unknown outcome.
  The targeted executor tests below cover confirmed partial effects separately.

An early commentary inference of a native committed stale prefix and original
0644 move was incorrect; authoritative independent snapshots establish the
zero-prefix/0664 results above.

## Metadata and replay

- `/new` → saved-session reopen → `/undo` → `/redo` → clean process exit →
  real process restart → saved-session reopen: all completed paired runs
  preserve exact fixture bytes/modes/presence and provider/tool counts.
- Attempts08–10 snapshot native `patch_effects` using SQLite `mode=ro`.
  Nine rows are identical before Undo/Redo and after restart; configured denied
  invocation has no effects row. All retained file diffs use **Minimal**.
  Maximum serialized fixture metadata: **2586 bytes**, within 64KiB cap.
- Native result metadata for the separated update contains exact ranges
  `(old4,count9,new4,count9)` and `(old16,count9,new16,count9)`, additions2 /
  deletions2, and inserted new-line positions8/20. Original's actual metadata
  has the same two content-context hunk ranges. This exercises the corrected
  bounded/context/minimal owner API, not a renderer-constructed diff.
- Reopen is tested for no reexecution and unchanged persisted metadata/files.
  This is not an OS read-syscall audit proving absence of every filesystem read.

## Full styled cells, PNG and cursor results

- No masks, crops, synthesized tool results or injected rendering. xterm reads
  the genuine PTY VT buffer; PNGs cover the whole terminal. Cell comparator
  checks symbols/colors/modifiers/width and cursor, and PNG comparator retains
  all differences. All **22 comparable cursor states per completed pair match**;
  full styled cells and PNGs still differ.
- Eight completed pairs: **352 DIFFERENT comparisons / 168 BLOCKED comparisons**,
  **0 EQUAL**. The BLOCKED entries consist of missing native running counterparts
  and, in09/10, two missing error-hover counterparts on each comparison mode.
- Genuine original argument-stream frames show `⋯ Patching`, captured while
  only its owned child is paused. Native did not show the corresponding running
  patch card during the bounded stream interval. This is not a held filesystem
  executor qualification or a Braille cadence/FPS claim.
- Actionable examples: native gutters omit spaces after `+`/`-`, shift
  line-number/content columns, and differ in styled-cell foreground roles.
  `03/apply-patch-create-completed.grid-diff.json` records **797 different cells**
  with equal cursor. Delete's `# Deleted` / `-2 lines` labels are genuine on both.
- **Auto boundary mismatch:** at terminal121, original remains unified while
  native splits (`04/*/apply-patch-multihunk-completed.*`). U18 original
  `routes/session/index.tsx:1248–1249,3410–3412` uses **contentWidth** via
  `ctx.width > 120`, rather than raw terminal width. Do not infer original
  split behavior from terminal121 alone. Explicit unified/split settings and
  word/none wrapping were exercised independently.
- Attempts03–09 retain native's default TPS text while original had TPS off;
  attempt10 aligns the actual setting and still yields full-frame differences.

## Commands/checks actually executed

Each attempt's `commands.json` contains its exact runner/build/comparator argv,
stdout/stderr and exits. Representative newest invocation:

```sh
node scripts/tui_capture/capture.mjs --apply-patch true \
  --geometry true --sidebar hide --sample short --columns 121 --rows 40 \
  --patch-view auto --patch-wrap none \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/apply-patch20260927-10
```

Exit1: actual behavior/replay PASS on both; visual comparisons differ/block.

Additional checks, all exit0:

```sh
CARGO_BUILD_JOBS=3 cargo build --locked
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-adapters --test patch_effects --test patch_audit
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-adapters --test runtime vis35_serialized_cap_finish_checkpoint_attach_restart_match
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-adapters --test runtime dto_application_events_surface_tool_calls
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-adapters --lib patch::
node scripts/tui_capture/analyze_apply_patch.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/apply-patch-analysis20260927-03.json 10
node --check scripts/tui_capture/capture.mjs
node --check scripts/tui_capture/apply_patch.mjs
node --check scripts/tui_capture/apply_patch_admission.mjs
python3 -m py_compile scripts/tui_capture/bridge.py scripts/tui_capture/apply_patch_fixture.py
node scripts/tui_capture/check_frontend.mjs
node scripts/tui_capture/check_capture_geometry.mjs
git diff --check
```

**34 targeted Rust tests passed:** 10 audit, 4 effects, 2 runtime/replay/cap,
18 patch module tests. Applicable TOOL02–TOOL04 cover Unicode/CRLF/newline,
empty/create/append, edit/delete/move, protected/outside/data-root/symlink,
conflicts and confirmed partial outcomes. No workspace quality/A01, release
build, or live A09 qualification is claimed by this capture-only slice.

## Open gates / next engineering step

VIS35 stays open: fix content-width auto selection, gutter/syntax/theme roles,
error resource presentation and observable native running patch presentation;
then make fresh paired captures. Qualify actual postcommit partial/cancelled/
unknown PTY outcomes and reuse A09 real-model coding harness separately.
VIS36 accept/reject requires its authoritative approval backend and remains
unqualified; configured Deny is not an acceptance/rejection lifecycle card.

All ten attempt directories occupy approximately **305MiB**. New report and
analysis files are immutable additions. No old capture rewritten; no commit,
stage or push performed by this worker. Cargo ownership is released at handoff.
