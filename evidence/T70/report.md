# T70 — completed canonical transcript

Status: COMPLETE for T70 R1/R2/R3. Implementation `b1c2b529729fdc370c97cf1e225fd1e49ad86219`
is pushed to verified origin/master. This is not whole-product T44/T61 visual acceptance.

## Source and diagnosis

- Base: `a8a3d28c7da8872f3b41c2614bbc627a630c40cb`, clean `master` at start.
- Owner requested removal of the aggregate policy, iterative implementation,
  visual comparison to OC2 TS, commit/push and a debug build.
- `a4bb3612` introduced a shared 64 KiB turn-presentation allowance without
  intra-turn continuation. `72fc7b62` coupled it to the legitimate single-result
  effect guard. Neither provider nor transport required that aggregate policy.
- At the old completion boundary, canonical history replaced richer live cards.
  Earlier content consumed the allowance: late input became `None`, final text
  became empty, and named tools painted empty arguments. The actual old-ELF
  baseline reproduced this loss; the process remained running, not panicking.
- Separate display clips limited public reasoning at runtime and in the expanded
  UI. Both needed removal from the accessible-history path.

## Implementation

- Remove the aggregate allocation, effects reservation and `remaining − 2048`
  argument admission. Whole inputs, ordered text and producer-owned metadata do
  not compete for a disposable turn preview.
- Use the existing storage/application owner with optional canonical part cursors
  on History and launch-fenced ReadChild. Indexed RAW/HOT ranges remain bounded;
  older windows do not reconstruct the entire archive. Whole-part page target:
  8 MiB, with one larger admitted part served alone to guarantee progress.
- Original aggregate message-reader APIs retain their contracts. Canonical
  transcript reads avoid duplicating the aggregate answer alongside its parts.
- TUI history retains 240 rows / 64 MiB and carries durable turn/component/part
  positions. Evicted same-message parts are reachable before moving to another
  message. Completion reanchors accepted-user parts to the assistant message.
- Canonical checkpoints adopt covered live parts with a bounded source-sequence
  index. Composition never guesses identity from repeated prose. Newer canonical
  reads supersede queued older hints; completion still adopts durable history.
- Large expanded public bodies reuse the existing indexed Markdown source pages;
  neither a 16 KiB prefix nor a 512-row snapshot becomes the interactive limit.
- Restore the pinned file-tool grouping, search-root labels, glyphs, muted/error
  colors, contiguous member spacing and real collapsed error interaction.
  Fix captured transcript releases above the nonmodal child composer without
  enabling covered prompt controls or relaxing painted-target fences.
- Canonical JSON encoding has a producer-derived representation envelope distinct
  from raw argument admission (normalizing a parsed number can increase bytes).

No new database/schema/store, production dependency or execution authority. RAW
viewing is read-only; cold tool-output artifacts are not automatically loaded.
The genuine provider argument/generation, per-result effect/output, execution RAW
and native framing guards are retained. Private native protocol is **3** rather
than 2 so an older owner is not silently used. No running owner was restarted,
killed or unlocked.

## Verification performed

Environment for Cargo: normal workspace `target`, approved `TMPDIR`, and
`CARGO_INCREMENTAL=0` after an interrupted initial compilation produced stale
incremental linker symbols. No clean, target override, automatic retries or added
ignored tests. Failed experiments were diagnosed before the next changed attempt.

- Red producer regression at the base: late `read` input `None`, expected its exact
  recorded JSON. Then green with unchanged journal/reopen equality.
- 646 focused tests passed in 8.049 s across adapters, TUI and binary lifecycle /
  exact child-control routes. Includes whole UTF-8 windows in both directions,
  normalized admitted arguments, a 10 MiB text, a >16 MiB public reference,
  root/selected-child snapshot retention and canonical-only final adoption.
- After indexed reasoning integration, all **587 TUI unit tests passed in 6.887 s**.
  Expanded byte/row tails, full/indexed geometry, cached/offscreen work and exact
  painted composer/group/error controls remain covered.
- The 301-segment archive/cursor regression passed in 15.347 s in its focused run.
  No new test exceeded the 30 s runtime budget in these runs.
- Capture JS syntax, Python AST parsing, `git diff --check` and progress structure
  validation passed. Progress validation is not product acceptance.
- Final mandatory `bash scripts/test.sh`: **PASS**, log
  `.local/evidence/T70/offline-gate-04.log`. Fmt, strict workspace/all-target Clippy,
  **2,105/2,105 tests** (12 existing skips), doctests, normal debug build/help/version
  pass. Test execution **371.977 s**; total **689.98 s**. RECON: this was not a warmed
  gate; test compilation took 265 s and Clippy compilation/check 42.61 s. Only the
  already approved UI07 measurement exceeds 30 s: **32.500 s**, within its 40 s
  contract. No new slow-test exception or relaxed threshold.
  Gate 03 had 2,103 passes / one expired-clipboard-deadline failure, 12 existing
  skips, 381.695 s tests / 397.33 s total. An already-expired helper budget raced
  immediate EOF before its timer became ready. Keep the existing deadline/cancel/
  reap assertions and correct deadline admission, not retry the unchanged race.
- Final R1 owner regression reproduces a partial prefix hidden among surrounding
  messages (four rows rather than the contiguous three-row tail). Canonical root/
  child message pages now stop at a partial turn; forward entry serves part zero.
  Original aggregate API and RAW equality remain asserted. This case, UTF-8
  continuation and the unchanged clipboard deadline/cancel/reap test pass 3/3 in
  2.547 s; the deadline fix admits no helper after its shared budget expires.
  The first test-reaching run passed strict Clippy and 2,101 tests; three failures
  identified two fixtures requiring correction: manual native startup used wire 2,
  and a MCP check observed the fixture's pre-exit `closed` record rather than actual
  reap. Wire 3 and bounded actual-reap readiness preserve the admission/shutdown
  assertions; all three focused tests then passed in 2.066 s. No production MCP
  change or increased timeout. Its 681.06 s total included 280 s test compilation
  and 382.123 s test execution; the only >30 s test was the approved UI07 case
  (32.731 s, within its 40 s contract).

## Actual pinned pair

Final-source episode: `.local/evidence/T70/history-canonical-05`.
The existing runner used `--build-oc true`, explicit tools/native/reference paths,
120×40 plus a real 80×24 child resize. Reference: official 2.0.12,
commit `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA256
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
No digest guard, baseline, clock or production timer was changed.

Both origins complete all **10 actual stages**: root live/completed, real error
expanded/recollapsed, expanded reasoning tail, child live/completed/narrow,
original root history reopened and selected child cold-reopened. Each origin has
seven fixture HTTP requests and nine real tools. Navigation/inspection adds no
provider or tool executions. Native root and child each persist five exact public
reasoning hashes / **86,565 bytes**; settled RAW hashes are unchanged after reopen.
`T70-THOUGHT-END-4`, beyond the former per-part preview, is visible in both actual
terminal captures.

Seven comparable stages have **zero differences across all 480 styled cells of
their four complete tool-member rows** (3,360 cells total). Root rows coincide
absolutely; child rows are offset by the pre-existing child-input bootstrap line.
The final-source diagnostic is `.local/evidence/T70/styled-members-05.json`;
the original full frames are unchanged.
Labels, glyphs, colors/modifiers, default spacers, group counts, final-text margin
and actual prompt/composer cursor checks pass. The failed operation remains failed
and reveals its actual recorded error, not rewritten reference prose.

**Full unmasked grid/PNG comparisons remain DIFFERENT (20 comparisons); the runner
therefore returns 1.** Per-origin `PASS_BEHAVIOR_ONLY` is not global pixel PASS.
Real timing/usage/footer values, literal OS error text and the existing child-input
bootstrap difference remain visible in the full artifacts. No crops, masks or
normalization hide them. This receipt qualifies the affected history/tool surface,
not paused whole-product parity. The earlier baseline-02/canonical-01/02 attempts
remain truthful failed diagnostic evidence.

## Delivery

Implementation: **`b1c2b529729fdc370c97cf1e225fd1e49ad86219`**,
`fix(history): preserve canonical transcript across completion`.
Ordinary push to `git@github.com:0FL01/oc.git`, `origin/master`, succeeded on
2026-10-19; a separate keyed ls-remote returned that exact commit. The owner-provided
identity is used only by normal Git SSH authentication, never inspected or printed.
No force push, persistent Git configuration, credentials or owner processes changed.

Post-commit `cargo build -p oc2 --bin oc2 --locked` passes in 0.34 s using the normal
workspace target; `target/debug/oc2 --version` reports `oc2 0.2.0`. The ELF has
debug_info and is not stripped. No owner `oc2-dev` interactive process was launched.
An existing protocol-2 owner requires its normal user-driven lease/quit transition
before a protocol-3 binary can own that data root. There is no storage migration.
