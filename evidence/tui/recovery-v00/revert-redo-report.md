# T44 VIS33 — actual paired Revert / whole-tail Redo

Date: 2026-09-26. HEAD: `566c1a2`; dirty owner implementation captured in each
attempt's source manifest and diff digest. This worker changed capture scripts,
their README and new evidence only. No Rust, `.opencode`, commit or push changes.

## Result

Authoritative complete attempt: [revert-redo-20260926-07](revert-redo-20260926-07/).
Pinned original `opencode v2.0.12` executable is SHA-checked by the runner against
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Native is source-built with `cargo build --locked` exit 0 in every attempt;
`commands.json`, `capture.lock.json` and `source-manifest.json` seal provenance.

| Observation | Original | Native |
|---|---|---|
| Three real completed user submissions | PASS | PASS |
| Identical fixture prompts and response text | PASS | PASS |
| Independent asynchronous title request | 1 completed | 1 completed |
| Total Responses requests/completions | 4 / 4 | 4 / 4 |
| User click → Actions → Revert before turn two | PASS | PASS |
| Two-message card; prompt restored; normal/hover frames | PASS | PASS |
| Drag selection with release on card does not restore | PASS | PASS |
| One card click restores entire tail, three user messages | PASS | PASS |
| Configured ctrl+x r restores entire tail | PASS | PASS |
| Slash `/redo` restores entire tail | PASS | PASS |
| Filtered Commands palette Redo restores entire tail | PASS | PASS |
| Fresh Revert before each independent Redo entry | PASS | PASS |
| New Home → Sessions reopen retains boundary/count | PASS | PASS |
| Clean exit → same-root restart → reopen retains boundary/count | PASS | PASS |
| PageUp/PageDown preserve card/count | PASS | PASS |
| DB read-only owner observations | 15 | 15 |
| Additional provider calls during all actions | 0 | 0 |
| Tool calls / returned tool results | 0 / 0 | 0 / 0 |
| Workspace sentinel and config hashes unchanged | PASS | PASS |
| Both process generations exit naturally with code 0 | PASS | PASS |

29 paired full frames (58 captures), complete styled cells **including cursor**,
PNG, render geometry and VT. All **58** unmasked grid/cursor and PNG comparisons
are **DIFFERENT**. The runner exits **1** for these real differences even though
both behavioral probes PASS. This is not pixel-parity acceptance or T44 closure.

## Exact command / validation

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --revert-redo true --geometry true --sample short --sidebar hide \
  --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v00/revert-redo-20260926-07
```

Command exit 1: original PASS, native PASS, all comparisons DIFFERENT.

`node scripts/tui_capture/check_revert_redo_evidence.mjs
evidence/tui/recovery-v00/revert-redo-20260926-07` exits 0. It validates source
build success, artifact hashes, pairwise exact prompt/response text, request and
completion counts, all 15 boundary observations per side, immutable fixture/config
hashes, two clean process exits per side and the 58 DIFFERENT comparisons.
`node --check` for runner/probe and `python3 -m py_compile` for bridge pass;
`git diff --check` passes. No workspace test suite run by this evidence worker.

## Fixture and owner-state facts

Explicit owner-approved test files live only in the isolated external fixture:
`project/vis33-owner-approved.txt`, each side's
`home/config/opencode/opencode.json` and `home/config/opencode/cli.json`.
The product config explicitly disables `snapshots`; the actual admitted CLI
schema uses `keybinds: {"session.redo":"<leader>r"}` on both binaries.
No renderer substitution, DB writes, masked regions or fake local card count.
No live credentials or paid provider calls: the loopback fake Responses provider
services the real binaries and rejects extra transcript requests/tools.

Actual user prompts are `VIS33 user turn N. No tools.`; actual provider answers
are `VIS33-ANSWER-N: completed.` for N=1..3. The applications independently request
`VIS33 saved tail fixture` as the title. Real application clocks/durations and
asynchronous request ordering are retained; timing is not asserted equal.

SQLite is opened with `mode=ro`. Native observations retain `upper_seq`,
`redo_tip`, archived user count 3 and visible/reverted counts 1/2 or 3/0.
Original observations retain actual `session_v2.revert` JSON and counts from
actual `session_message` user records. Boundary/count 1/2 persists after session
switch, new process restart and pager input; final restoration returns 3/0.
This independently rules out a purely local card counter. Three-turn PageUp/
PageDown coverage does not claim loading a large-history page beyond its limit.

## Preserved attempt ledger

All directories `revert-redo-20260926-01` through `-07` are retained; no prior
evidence was overwritten or deleted.

| Attempt | Exit | Actual result / diagnosed issue |
|---|---:|---|
| 01 | 1 | Original completed three turns; observer used wrong message table. Native home predicate incorrectly required Build. Fixed capture predicates/schema. |
| 02 | 2 | Original card/shortcut/slash worked; runner incorrectly used leader+p for palette. Shell timeout 120s interrupted native; this failure remains retained. |
| 03 | 1 | Native restored all four entry points; session row target included header duplicate. Original palette needed filtering because Redo was below visible list. |
| 04 | 1 | Both behavior PASS, 58 DIFFERENT comparisons; runner teardown forced final generation. Added explicit clean shutdown and workspace/config immutability observations. |
| 05 | 1 | Native complete PASS with clean exits/immutable files. Original `/new` sent Enter before asynchronous autocomplete settled. Added typed-command wait. |
| 06 | 1 | Native complete PASS. Original restored prompt briefly remained while locating next transcript click. Added stable cleared-draft wait and transcript-only target. |
| 07 | 1 | Both complete behavior PASS, clean exits/immutable files; 58 DIFFERENT comparisons. |

## Remaining actual gaps / scope limits

* Plain native profile does not show original `Build` in prompt metadata or
  assistant attribution. Both native protocol and frames show that no explicit
  inline agent profile was selected in this fixture. Investigate default-agent
  owner selection/projection rather than painting a hardcoded label.
* Native assistant attribution includes `tok/s` here while original does not
  (`session.tps:false`); durations differ legitimately. Full frames also retain
  differing session-dialog/palette geometry and real fixture path presentation.
  The saved comparator reports remain authoritative: no visual gate is PASS.
* This probe establishes the admitted default Redo override through `cli.json`.
  Optional custom shortcut and `cli.jsonc` were not separately exercised.
* Large-history page-load count qualification, new-input invalidation,
  executing-turn interrupt ordering and action-failure boundary retention are
  outside this bounded three-turn behavioral capture; no result is inferred.

Cargo ownership is released after this serial source-build capture campaign.
