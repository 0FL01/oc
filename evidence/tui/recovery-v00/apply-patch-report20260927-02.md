# VIS35 fresh source-built patch campaign — 2026-09-27

**Result: behavioral/pending/filesystem/replay checks PASS on seven fresh pairs;
whole-frame VIS35 parity NOT PASS.** This report covers attempts11–18. Attempts01–10,
the prior report, and every earlier JSON analysis remain immutable.

## Source association and scope

- HEAD: `318025abd1d535f8229c9da53b40e375f5610cbf`, with parent-owned dirty Rust source.
- Each attempt invoked `cargo build --locked`, exit0, before launching the actual
  debug executable. All seven completed pairs use native SHA-256
  `25c7e1e0d7e577ff573323fd56fffc566b560e6ee3311a7d83c37a38d2a5e73a`.
- Rust/Cargo input digest for attempts11–18, including untracked Rust modules:
  `a16718e1e4cb77bebb2ea170fcd491a73712a7c1bbe47ddf200ebef21b06defd`.
  This intentionally differs from the prior source group. Each attempt retains
  `source-manifest.json`, binary/runner/config hashes and build output in
  `commands.json` / `capture.lock.json`. Attempt14 built successfully but never
  launched native, so has no native executable capture identity.
- Reference executable: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
  SHA-256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
  Donor pin: `2670273ff17da96f85c5826ced57aa1b368754fa`. Actual U19 module:
  `opencode/packages/core/src/tool/plugin/patch.ts`, SHA-256
  `66f0357e77d8e3d3edc4c63cb0aefac050b39b82ad08e8896bf50354d0e8d490`.
- Edits in this continuation: capture tooling and new evidence only. Rust,
  acceptance files and inherited `.opencode/` were not edited. Inherited
  `.opencode/` was never inspected or staged. No commit/push/staging.

## Actual commands and immutable outcomes

Common capture command (each invocation and exact argv retained in its attempt):

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns WIDTH --rows 40 \
  --apply-patch true [OPTIONS] \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/apply-patch20260927-NN
```

| Attempt | WIDTH / OPTIONS | Both lifecycle probes | Runner exit | EQUAL | DIFFERENT | BLOCKED |
|---|---|---|---:|---:|---:|---:|
| 11 | 120, default auto/word | PASS | 1 | 4 | 60 | 4 |
| 12 | 121, default auto/word | PASS | 1 | 0 | 64 | 4 |
| 13 | 121, `--patch-view unified --patch-wrap none` | PASS | 1 | 2 | 62 | 4 |
| 14 | 160, `--patch-view split --patch-wrap none` | FAILED original; native NOT_RUN | 2 | — | — | 1 |
| 15 | 160, `--patch-view split --patch-wrap none` | PASS | 1 | 0 | 64 | 4 |
| 16 | 121, `--patch-view split --patch-wrap none` | PASS | 1 | 0 | 64 | 4 |
| 17 | 124, default auto/word | PASS | 1 | 0 | 64 | 4 |
| 18 | 125, default auto/word | PASS | 1 | 0 | 64 | 4 |

Attempt14 also has one FAILED status. The outer 600-second watchdog wrapped
attempts12–14 together, interrupted14 during original replay, and closed the
browser (`page.evaluate: Target page, context or browser has been closed`).
Original had completed21 requests/10 tools, but pending snapshots caught six
operations whose effects had already occurred. Those frames do not qualify
pending. No native side or lifecycle PASS is inferred. No replay of its unknown
interruption was attempted;15 uses a new isolated directory and separate bounded
invocation. A process listing afterward showed no leftover capture child.

The real SSE hold was3 seconds in11–14,8 seconds in15–18. The newer probe rejects
an already-finished argument stream before pausing. Old observations are retained.

Seven completed pairs:224 full-frame pairs,448 grid/PNG comparisons:6 EQUAL,
442 DIFFERENT,28 BLOCKED. Every one of224 paired cursor states is equal.
Equal PNGs:11 multihunk completed/hover and delete completed/hover;13 multihunk
completed/hover. No grid EQUAL. No masks, crops, renderer injection or fake results.

## Real pipeline, pending and durable reconciliation

The HTTP Responses fixture emits only real ordinary-function calls. Native
advertises unchanged `apply_patch({patchText:string})`; donor advertises actual
`patch({patchText:string})` via the owner-approved fixture context admission hook.
The hook preserves the pinned original executor and records actual before/after
events/results. `edit`/`write` are not admitted. All-other actions/resources deny;
only exact fixed safe fixture paths allow, with `denied.txt` still denied.

`protocol.json` retains actual incoming schemas, inputs, outputs and emitted
argument deltas with item/call identities and monotonic times. The analyzer
reconstructs each call's exact argument JSON from the two emitted deltas.
This is an actual provider → adapter → owner → TUI pipeline; no capture-side
CoreEvent or tool-result insertion. Direct typed Pending/Linked event assertions
come from the separate runtime test below, not from invented PTY telemetry.

For all70 native calls in the seven completed pairs:

- Exactly one observed `Patch · arguments streaming · no effects yet` label.
- Independent pending snapshot matches previous file bytes, modes and presence;
  durable tool operations and effect rows are unchanged. Raw patch text is not
  painted as a confirmed diff. Pending presentation is not an execution outcome.
- Completion removes the pending label, with exactly one durable operation for
  the real call identity. Ten durable display links are present, all ten unique.
- Reopen, conversation-only Undo/Redo and process restart preserve file evidence
  and request/tool counts; final durable operation/effect rows remain identical.
  Restart shows zero pending labels. No tool is re-executed by playback.

All70 original pending snapshots also retain unchanged file bytes/modes. Its
different `Patching` presentation is compared unmasked, rather than renamed to
the native label. Stable snapshots pause only the owned application's process
group; this is an argument-stream observation, not a held filesystem executor.
Samples and durable links do not prove every intervening transient render.

Completed-side exact counts:21 requests,21 completions,10 tools,10 actual
model-facing results,0 invalid requests (1 title request +20 patch requests).
Seven pairs:294 requests/completions,140 calls/results. Including14:315 requests,
315 completions,150 calls. All01–18:714 requests/completions,340 calls,0 invalid.
No transport adapter was added; these observations do not claim general request
count equivalence. Original real executor outcomes per completed side:
7 completed +3 error (multi-file stale preflight, stale single, denial).

## Independent filesystem and metadata proof

Ten operations exercised per side: add, empty create, update away from file start
with two hunks, delete, move, full replacement, multi-file add, stale multi-file
preflight, stale single and configured denial. Each snapshot stores presence,
full hex bytes, SHA-256 and mode independently of rendered diff and result text.

- `created.ts`: exact two lines including the long fixture string;
  `empty.txt`: present with zero bytes.
- `update.txt`: all24 lines checked, edits at8 and20; mode0640 preserved.
- `delete.txt` and source `move.txt`: absent. `moved.txt`: exact `new move\n`.
- **Mode difference retained:** native destination0751 (source mode preserved),
  original destination0664. Security behavior was not weakened to obtain parity.
- `replace.txt`: exact `replacement\n`; multi-a/multi-b: exact alpha/beta lines.
- `prefix.txt`: absent after stale multi-file preflight; stale bytes and denied
  sentinel untouched. This is zero committed prefix, not postcommit partial.

Native owner `patch_effects` retains9 rows; denied has no effect row. All actual
file algorithms are `Minimal`; maximum serialized metadata2586 bytes (64KiB cap).
Multi-hunk actual ranges old/new `(4,9)` and `(16,9)` with additions/deletions2/2,
actual inserted line positions8/20. Metadata and durable operation rows compare
identically before replay and after restart. File/effect/diff DTOs are read from
the owner's corrected API; no patch-text-derived replacement result is installed.

## Remaining observable gaps

- Actual auto boundary now agrees:124 terminal columns gives unified;125 gives
  split. Move diff rows in17/18 visibly prove the switch in both applications.
- Explicit split121 and auto125 retain an odd-width right-side gutter shift:
  native right-hand `1 + new move` starts one column earlier. Split160 aligns
  that row. Multihunk completed grid differences:11/13 one fg cell;15/17 two
  cells;16/18 203 cells. These are full-frame figures, not masked region PASS.
- Multihunk/delete equal PNGs still have styled-cell foreground mismatch on a
  space in the user text. Equal pixels do not qualify styled-cell equality.
- Move completion remains a large full-frame mismatch (including transcript
  placement); native `moved.txt` appears twice in visible text, so the unique-file
  hover probe refuses ambiguity. Denial has no native file-card target. Those
  two missing counterparts cause4 blocked comparisons per pair. Other successful
  current-file hover coordinates are retained; no arbitrary click substitutes.
- Whole-frame home/running/completed/replay differences, actual elapsed fields
  and intentionally different pending labels remain exposed.
- VIS36 accept/reject remains unqualified; configured deny is a real failure,
  not a synthetic permission card. PTY postcommit partial/cancelled/unknown,
  executor-held running, A09 live-model authorship and release entry remain open.
- Unchanged snapshots prove filesystem/no-reexecution invariants, not an OS
  read-syscall audit or physical refresh-rate measurement.

## Targeted gates executed after captures

All commands below exit0,22 tests total. Environment:
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2`.

```sh
cargo test --locked -p oc --test pty_t42 aud38_apply_patch_card_shows_bounded_diff
cargo test --locked -p oc-adapters --test patch_effects --test patch_audit
cargo test --locked -p oc-adapters --test runtime vis35_serialized_cap_finish_checkpoint_attach_restart_match
cargo test --locked -p oc-tui --lib argument_stream_exact_identity_caps_reconcile_and_terminal_cleanup
cargo test --locked -p oc-tui --lib patch_view::tests::vis35
```

The updated audit-card expectation passes with typed confirmed counts. Runtime
gate verifies real Pending/Linked identities and durable finish/checkpoint/history
association. TUI test covers exact-identity reconciliation, bounded32 pending
calls/4096-byte preview and terminal cleanup. Existing backend tests qualify real
postcommit filesystem failure and unapplied tail independently; no PTY effect
was injected to manufacture that state. Security/audit tests remain enabled.

Capture/probe/analyzer `node --check`, fixture `py_compile`, actual xterm frontend
qualification, capture geometry check and `git diff --check` all PASS. No whole
workspace A01 or final acceptance claim.

Latest immutable analysis (exit0):

```sh
node scripts/tui_capture/analyze_apply_patch.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/apply-patch-analysis20260927-06.json 18
```

Analyses04/05 are retained intermediate reports;06 adds exact delta reconstruction
and unique durable display-link assertions. Detailed rows/diff fields, source
groups, complete counts and filesystem proofs remain in06 and the per-attempt
artifacts. Next unused capture name:19. Cargo ownership released at handoff.
