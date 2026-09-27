# VIS35 corrected-source captures19–22 — 2026-09-27

**All four paired lifecycle/filesystem/pending probes PASS. Whole VIS35 parity
remains NOT PASS.** Six individual full styled-grid frames and the corresponding
six PNGs are exactly equal. All older captures and reports01/02 remain immutable.

## Source association

Each pair ran its own `cargo build --locked` (exit0) before executing the actual
native binary. HEAD remains `318025abd1d535f8229c9da53b40e375f5610cbf`, plus the
parent-owned dirty source. The corrected native binary is SHA-256:

`4a0fe8fe67f22cbe1b645a1748e5f1a2d7f7fc417c5c558cb58a9bc8227a2701`.

All four attempts have identical Rust/Cargo input digest:

`124d691e9df7edb84a78b60e3d383a39bf33dba7ef2bdc9602e2f0f35bc48acc`.

All four full source manifests, including untracked source modules and capture
tooling as executed, have canonical SHA-256:

`33f3a20c91535cf122136862e161faae47f9bd2316b5ae583d0c5a9275cecd3c`.

These are exact SHA-256 source seals, not cryptographic signing-key signatures.
The final analyzer independently rehashes current Rust/Cargo inputs against the
latest manifest: PASS. Later analyzer/README edits do not rewrite captured input
manifests. Per-attempt `commands.json` retains actual argv/build output;
`capture.lock.json` retains binary, producer, config, fixture and frontend hashes.

Reference binary SHA-256 remains
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`, donor pin
`2670273ff17da96f85c5826ced57aa1b368754fa`. The fixture admission hook calls the
existing bundled U19 patch executor, never a substitute execution/result.
Native ordinary `apply_patch` schema/arguments remain unchanged.

## Executed capture commands and exact comparisons

Each command used these common arguments:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --rows 40 --apply-patch true \
  --columns WIDTH [OPTIONS] \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/apply-patch20260927-NN
```

| Attempt | WIDTH / OPTIONS | Grid EQUAL / DIFFERENT / BLOCKED | PNG EQUAL / DIFFERENT / BLOCKED | Cursor |
|---|---|---|---|---|
| 19 | 120 default auto/word | 2 /31 /1 | 2 /31 /1 | 33/33 equal |
| 20 | 121 `--patch-view split --patch-wrap none` | 0 /33 /1 | 0 /33 /1 | 33/33 equal |
| 21 | 125 default auto/word | 2 /31 /1 | 2 /31 /1 | 33/33 equal |
| 22 | 160 `--patch-view split --patch-wrap none` | 2 /31 /1 | 2 /31 /1 | 33/33 equal |

All runner exits1 because remaining full-frame differences are real. No blocker
or failed lifecycle in19–22. Overall132 full-frame pairs: grid6 EQUAL/126 DIFFERENT,
PNG6 EQUAL/126 DIFFERENT,8 blocked comparisons; all132 paired cursor states equal.
No masks, crops, clock freezing, pending-label renaming or synthesized effects.

Exact full-grid **and** PNG equality in:

- 19: multihunk completed and hover.
- 21: replacement completed and hover.
- 22: multifile completed and hover.

These are actual frame observations; equal independent elapsed digits on one
capture do not establish deterministic clock/timing equality across runs.

### Corrected width/gutter/header observations

The native move source-arrow transcript row is gone; destination header remains
and actual source/destination metadata remains in the durable DTO. The same real
`moved.txt` target is now unique on both sides. Hover coordinates (zero-based)
match exactly:19 `(15,23)`,20–22 `(15,24)`. Real SGR hover inputs and full resulting
grids/PNGs were captured. There is no ambiguous-target fallback or fake click.

Remaining completed grid differences, before any masking:

| Attempt | Multihunk | Delete | Move | Replacement | Multifile |
|---|---:|---:|---:|---:|---:|
| 19 | 0 | 1 | 2 | 1 | 1 |
| 20 | 1 | 2 | 2 | 1 | 1 |
| 21 | 1 | 1 | 1 | 0 | 1 |
| 22 | 1 | 2 | 3 | 2 | 0 |

Inspected mismatches are symbols in actual elapsed-time footer rows:19 delete
and20 multihunk differ only at `(38,31)`;22 move differs at `(38,3)`, `(38,17)` and
`(38,31)`. Full row context is retained in analysis08. The prior odd-width gutter
and move transcript displacement are no longer present in these inspected
frames. This diagnostic does not relabel DIFFERENT as EQUAL.

Only denial hover lacks a native counterpart: configured denial is not a file
effect card. It remains one blocked grid +one blocked PNG per pair. Successful
current-file hover is now captured for all seven mutation cases, including move.

## Real calls, pending and replay proof

Every completed side has21 actual provider requests,21 completions,10 ordinary
function calls and10 actual model-facing results, zero invalid requests. Four
pairs total168 requests/completions,80 calls/results. All attempts01–22 aggregate
882 requests/completions,420 calls, zero invalid; old interrupted attempts remain
counted and explicitly unqualified. No transport adaptation or extra replay
provider request was introduced.

Original executor per side records7 completed and3 error calls. Cases are actual
add/empty-create/two-hunk update/delete/move/full-replacement/multifile-add/stale
multifile/stale-single/configured-denial. Real emitted SSE argument deltas
reconstruct exactly the calls' JSON arguments and retain provider item/call IDs.

All40 native pending captures show exactly one explicit
`Patch · arguments streaming · no effects yet`; original keeps its actual
different running presentation. Pending snapshots retain all prior file bytes,
modes and presence, and unchanged durable operations/effect rows. Completion
removes pending and links one actual durable operation per call. Each native
session has10 unique durable tool display links; no extra link is created by
finish or restart. Observations are sampled real PTY states, not qualification
of every intervening transient frame or an executor-held state.

Independent hex-byte/hash/mode/presence checks pass for every operation. Empty
creation is present/zero bytes; update24 lines edits8/20 preserve0640; delete and
move source disappear; moved destination exact `new move\n`. Replacement and
both added multifile contents match. Stale/denied sentinels remain untouched;
stale multi-file preflight commits no prefix (`prefix.txt` absent).

**Mode gap retained:** native moved destination0751, donor0664. Source mode/security
semantics were not changed to obtain visual equality. Durable `PatchEffects`
retains the source path and destination,9 rows (none for denied), actual `Minimal`
algorithms, maximum serialized2586 bytes, multihunk old/new ranges `(4,9)` /
`(16,9)` and inserted lines8/20. No rendered/request preview is used as filesystem
proof or installed as a fake result.

Reopen, conversation-only Undo/Redo and actual clean process restart preserve
all file bytes/modes, tool/provider counts and final operation/effect records.
**Playback produces zero re-execution and zero extra requests.** Restart shows
zero pending labels. Read-only SQLite observations are evidence reads, not
transcript/effect writes or a claimed OS read-syscall audit.

## Final targeted gates

Executed after all four pairs, every command exit0:

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-tui --lib patch_view::tests::vis35
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc-tui --lib argument_stream_exact_identity_caps_reconcile_and_terminal_cleanup
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --locked -p oc --test pty_t42 aud38_apply_patch_card_shows_bounded_diff
CARGO_BUILD_JOBS=3 cargo build --locked
```

Six VIS35 patch-view tests PASS, including new
`vis35_fresh_capture_odd_split_and_header_gap_full_indexed`; one bounded pending
identity/reconcile/cleanup test PASS; one patched real audit-card PTY test PASS.
Eight targeted tests total; final build PASS. Analyzer syntax and
`git diff --check` PASS. No claim of whole-workspace A01/final acceptance.

Latest immutable analyzer, exit0:

```sh
node scripts/tui_capture/analyze_apply_patch.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/apply-patch-analysis20260927-08.json 22
```

Analysis07 retained as intermediate;08 adds exact differing-row context and
current Rust source/manifest verification. Old01–18 captures/reports unchanged.
Only analyzer/README and new tooling evidence changed in this continuation.
No Rust/acceptance edit, commit, staging or push. Inherited `.opencode/` never
inspected or modified. Next unused capture23. Cargo ownership released.

## Open gates

Whole-frame home/running/error/replay and independent elapsed-time differences
remain; distinct native pending label is intentional and unmasked. PTY
postcommit partial/cancelled/unknown, executor-held running, A09 live-model
authorship and permission approval accept/reject remain **unqualified**. Real
configured denial is not approval qualification. This report records observed
behavior and exact comparator outcomes, not a VIS35/VIS36 blanket PASS.
