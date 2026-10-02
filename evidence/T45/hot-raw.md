# T45 R9 — closed current-task raw/hot boundary

## Coordinator qualification — 2026-10-03

The independently reviewed closed-current-task atomic is verified. It is not
whole T45, whole-past DCP11 renewal, or a whole-product readiness claim.
The qualified source is execution base `83054c997e5f9c2f62815f685299bc44127da06e`
plus the reviewed implementation; HEAD `888b411118f876266b2a5e139465fe2cec2bc061`
adds documentation-only owner commits. No production edit followed qualification.

Coordinator checks all exited0: eight `storage::turn_history::tests::` cases,
workspace fmt, strict locked workspace/all-target Clippy, normal locked build,
47 Python tests, documentation/progress structural checks and reviewed diff check.
The current complete workspace gate remains1461/0/10 in
`hot-raw-logs/review-final-workspace.log.gz`; the existing ten opt-in ignores
were not changed. The independent tests specifically exercise corrupt RAW-page
refusal, bounded adjacent descriptors, atomic failure, and authorized family
deletion without losing surviving Fork RAW. The withdrawn partial-span finding
did not alter the causal closed-group boundary.

After the last normal build, both normal ELFs independently passed root8,
root32, child32, same-task A→B switch/reopen, before-COMMIT failure, and
after-COMMIT crash/recovery: twelve workloads, all owned processes and HTTP
workers joined before exact temporary cleanup. Root8 and root32 both retained
checkpoint/wire peaks33755/31922 bytes and a maximum HOT transfer of one row /
22765 bytes; implicit whole-RAW reads were zero. Child32 peaks33912/32062 bytes,
one HOT row /22897 bytes, likewise zero implicit RAW pages. RAW hashes remained
stable as segments grew2→8. Actual dispatches matched physical POSTs; each
effect occurred once. Both crash cases reopened with zero provider replay.
Whole-process memory and SQLite/process-I/O scopes remain the explicitly
qualified measurements below, not an isolated allocator or every-SQL-read claim.

Normal ELF hashes were identical before and after all direct proofs:
`debug=64dadf94105ab2c47b13c82c5c30c1ce6194e2c64a05a9349c3238bee54c9cde`,
`release=6be48e36d079ece0b6a7b4e6f3ec4b6fd27bff7574e21e14bf722113a6e37746`.
No live/paid call, exhausted-ledger reset, inherited config inspection, or
T44 resume occurred. Whole-past replacement/defaults/controls and the remaining
profile/child T45 contracts continue separately. The phase records below are
preserved with their original source associations and limitations.

## Current result — parent-review repairs implemented and requalified

Sole ownership resumed at HEAD `888b411118f876266b2a5e139465fe2cec2bc061`, preserving
all prior dirty source and owner documentation. Current HEAD remains that value;
Rust execution base is `83054c997e5f9c2f62815f685299bc44127da06e` plus this owner's
dirty implementation. The intervening owner commits affect documentation only.
The 1459/0/10 qualification and d6/7af ELF hashes below are explicitly historical.

Two independent falsifiable RED owner tests reproduced the introduced regressions
before production fixes (`review-red-page`, exit101; `review-red-delete`, exit101):
after TEST-only removal of the immutable UPDATE trigger, `{}` was returned as a
successful RAW page; an actual settled segmented root failed public family deletion
with SQLite FK error787. Cause: page admission checked bytes but not canonical
ledger integrity; segments referenced turns without cascade and their DELETE
trigger prohibited the sole explicit archive-removal transaction.

Frozen minimal repair: one admitted page, its own fixed base/end descriptors and
at most two indexed adjacent descriptors, checked canonical TurnLog round trip,
ownership/counts/coordinates and chain linkage before return. Store the fixed base
descriptor alongside the existing fixed end descriptor in unreleased migration8
so successor linkage is checked without transferring/parsing its payload. No
checksum/signature/manifest framework, archive scan or normal recovery reload.
Fork reuses the same page validation within its existing admitted transaction.
CASCADE follows the parent's turn deletion; the immutable segment DELETE trigger
still aborts while its owning turn exists. The existing explicit family transaction
and busy/location guards retain their authority, forks are not descendants, and
deferred COMMIT failure restores the whole family and RAW. Actual SQLite behavior,
not an assumed trigger ordering, is exercised by the public deletion test.

`review-green-owners` passed8/0/0 after the initial repair, including corruption,
public family deletion, active child refusal, direct segment UPDATE/DELETE refusal,
deferred-FK COMMIT rollback and surviving fork RAW. The final eight owner tests
and full gates below pass after stronger adjacent-link checks. The withdrawn
partial-span finding requires no
boundary change: settled failed historical spans followed by successful continuation
remain valid closed RAW; open/in-flight/unknown groups are not sealed.

### Current repair contract and falsifiable receipts

- `raw_turn_segment` reads a consistent SQLite snapshot. Payload transfer is
  admitted before receipt by the caller's byte budget and the unchanged 16 MiB
  host bound. Its own fixed base/end metadata and at most one predecessor and one
  successor descriptor use indexed lookup and the existing 16384-byte descriptor
  budget. It never transfers neighboring payloads or loads the whole archive.
- Before returning bytes it checks JSON and version/schema, owning turn,
  indexed endpoints, all seven original array counts, monotonic coordinates,
  sequential closed call/result balance, completed spans, local media/instruction/
  display indices, original request/span receipts, tool-operation ownership,
  adjacent ordinal/base/end linkage and last-page agreement with bounded HOT
  prefix metadata. Invalid data produces a typed error without returning RAW.
  A missing page inside a committed prefix is an integrity error; a deleted family
  or genuinely absent page is absent. Fork uses this same validator inside its
  existing explicit prefix admission. Ordinary retry/restore/wire readers still
  use only admitted HOT and the indexed last descriptor, never old RAW payloads.
- `review-last-owner-tests` passes eight scenarios. The new corruption scenario
  rejects `{}`, bad JSON, foreign ownership, unsupported version, wrong counts,
  original receipt and display coordinates, mismatched indexed endpoints,
  payload/base/end mismatch, inconsistent neighbor linkage and a missing committed
  page. Valid pages are byte-exact; an over-budget page is refused. A valid Fork
  succeeds, corrupted RAW is rejected by that same Fork path, and ordinary HOT
  restore remains successful despite corruption of an older RAW body.
- The new public deletion scenario verifies actual settled root and child segments.
  Existing wrong-location/non-root/active-child denials remain. Direct segment
  UPDATE/DELETE are refused. An injected deferred-FK failure at COMMIT rolls back
  the public family transaction and preserves exact root/child RAW. Successful
  `delete_root_family` removes only the requested root family; the non-descendant
  fork and sibling survive, fork RAW bytes remain exact, and known fork effects
  remain committed. No effect rollback or replay is introduced.
- Schema 8 is still unreleased: only its existing definition was adjusted, adding
  one fixed `base_descriptor` column and the scoped cascade/immutability ordering.
  No new migration framework, authority, store, hash/signature manifest, global
  DB function/lock or widened public deletion API was added.

### Current gates after the last source edit

Every command below exits **0**, jobs3 / test threads1 / offline / locked,
owned TMPDIR and unchanged 1798-second operational watchdog. Separate precompile
and full execution were retained. All failed RED/experimental logs remain lossless.

| Gate / lossless log stem | Exit | Result / seconds |
| --- | --- | --- |
| `review-last-owner-tests` | 0 | eight RAW/HOT owner scenarios, 12.494 |
| `review-final-fmt` | 0 | workspace format check, 2.354 |
| `review-final-clippy` | 0 | workspace all-targets strict `-D warnings`, 9.289 |
| `review-final-precompile` | 0 | workspace locked offline `test --no-run`, 71.276 |
| `review-final-workspace` | 0 | **42 summaries; 1461 passed / 0 failed / 10 ignored**, 961.102 |
| `review-final-debug` | 0 | normal workspace debug build, 8.418 |
| `review-final-release` | 0 | normal workspace release build, 146.860 |
| `review-final-debug-help`, `review-final-release-help` | 0 / 0 | normal ELF help, 0.009 / 0.016 |
| `review-final-hot-debug`, `review-final-hot-release` | 0 / 0 | actual root8/root32/child32, 76.493 / 66.859 |
| `review-final-switch-debug`, `review-final-switch-release` | 0 / 0 | actual switch/restart/precommit fault/postcommit crash, 16.680 / 13.609 |
| `review-final-t50-tool`, `review-final-t50-retry`, `review-final-t50-compact` | 0 / 0 / 0 | nearest issuing-batch/retry/compaction, 2.724 / 4.192 / 2.406 |
| `review-final-t50-stream-release` | 0 | normal release model-switch stream, 1.184 |
| `review-final-cold-chain`, `review-final-cold-fault` | 0 / 0 | actual ColdArtifact/effect ownership, 5.355 / 2.078 |
| `review-final-mutation-patch`, `review-final-mutation-write` | 0 / 0 | actual release patch/debug write, 0.809 / 1.504 |
| `review-final-round-debug`, `review-final-round-release` | 0 / 0 | root17/child17/partial retry/cancel, 43.938 / 38.021 |
| `review-final-temp-cleanup` | 0 | three exact joined full-test owner directories, 0.038 |
| `review-final-python` | 0 | Python **47/47**, 11.093 |
| `review-final-docs`, `review-final-progress`, `review-final-size` | 0 / 0 / 0 | structural checks after current report, 0.114 / 0.092 / 1.315 |
| `review-final-evidence-validation` | 0 | AST/quota/42-summary totals/current ELF hashes/exact ownership paths, 0.565 |

Current normal ELF SHA-256, unchanged before/after the current native workloads:

- Debug: `64dadf94105ab2c47b13c82c5c30c1ce6194e2c64a05a9349c3238bee54c9cde`.
- Release: `6be48e36d079ece0b6a7b4e6f3ec4b6fd27bff7574e21e14bf722113a6e37746`.

### Current native boundedness measurements

The final normal binaries again execute real root8/root32 and foreground child32
with one committed effect, 8192-byte outputs, reused call IDs, early retry,
late429, a settled failed partial span followed by new continuation, and admitted
counted native checkpoint retry. Actual physical POST counts equal durable lanes.
Old RAW segment byte hashes remain unchanged at later actual provider requests.
Both normal binaries also pass A→B busy switching during an A-issued summary,
B's next safe schema/budget, restart with zero POSTs, before-COMMIT fault keeping
complete old HOT/no segment and post-COMMIT crash keeping RAW/no effect replay.

| Current root measurement | 8 groups | 32 groups |
| --- | ---: | ---: |
| Peak active checkpoint, both builds | **33755 B** | **33755 B** |
| Peak actual main wire JSON, both builds | **31922 B** | **31922 B** |
| Maximum HOT transfer delta, both builds | **1 row / 22765 B** | **1 row / 22765 B** |
| Implicit whole RAW pages, both builds | **0 rows / 0 B** | **0 rows / 0 B** |
| Maximum tail input / receipts / spans, both builds | 8 / 6 / 5 | 8 / 6 / 5 |
| Latest actual wire JSON, both builds | 6295 B | 6295 B |
| Final HOT, debug / release | 2790 / 2790 B | 2796 / 2795 B |
| Original RAW+tail, debug / release | 84853 / 84853 B | 322722 / 322721 B |
| Immutable segments, both builds | 2 | 8 |
| Physical main / compaction / title, both builds | 12 / 3 / 1 | 36 / 9 / 1 |
| Observed predispatch checkpoint-byte sum, both builds | 144247 B | 554705 B |
| Peak VmRSS / VmHWM, debug | 51507200 / 51769344 B | 52494336 / 53075968 B |
| Peak VmRSS / VmHWM, release | 20430848 / 20582400 B | 21483520 / 22192128 B |
| Last DB bytes, debug / release | 700416 / 700416 B | 1433600 / 1421312 B |
| Last WAL bytes / 4096-page frames, debug | 4152992 / 1008 | 4152992 / 1008 |
| Last WAL bytes / frames, release | 4140632 / 1005 | 4144752 / 1006 |
| Process read/write bytes, debug | 0 / 6184960 B | 8192 / 18173952 B |
| Process read/write bytes, release | 0 / 6184960 B | 4096 / 18030592 B |
| Process write bytes / original RAW byte, debug | 72.890 | 56.315 |
| Process write bytes / original RAW byte, release | 72.890 | 55.871 |

Current root RAW hashes, debug8/debug32/release8/release32 respectively:

```text
8d83da8228a7212c19d02a7e4dc7bc08c3a6af899d8c98ffcde51c0b10bafc14
5d949c4268e6c871570ab7fd511ec63dd62d4617b3648ae7147603a4d8ac0a01
750397204cd06747b49f2df702d9b9334a27918c863c14a2b79091a9aef074f7
63b280e3d1c57b6b232988517eb329d28e42ca2a744cb7f04a60d90fcac14c5d
```

The child32 has peak checkpoint/wire 33912/32062 B, final HOT2845 B, original
323629 B, eight segments, maximum HOT transfer1/22897 B, implicit RAW0 and lanes
main2/child36/compact9/title1. Debug/release peak VmHWM is 56442880/22388736 B,
last DB1454080 B, WAL4161232/4165352 B (1010/1011 occupied frames), and process
write18538496/18501632 B. Child RAW hashes debug/release are
`5fab42fbcd0bc54ddc4a985c374cc0c40ce2bec52c6bd00d0f37b3f38533f3be` /
`f008384281db4ee99f83e7b976c18ee37a64f2b4349fe22e9516bde8b28419cc`.

Counters and measurement scope are unchanged and explicit: whole-process physical
VmRSS/VmHWM are not isolated Rust allocator retention; process I/O includes SQLite,
events and shell, not SQLite-only amplification; WAL frames are occupancy/reuse,
not cumulative writes; checkpoint sums are observed predispatch boundaries, not
every callback serialization. Six read counters measure HOT strings, successful
explicit RAW pages and bounded UI span/part strings, not all scalar/preview/Fork
I/O. Current cumulative root8/root32 counters are `[3,25313,0,0,280,116436]` /
`[9,32975,0,0,3352,1336189]`. UI maximum transfer fills from75/30971 to197/77871
rows/bytes within the independent 192-span/240-part query bounds, not an execution
stop. Those bounds are also tested after301 segments. No token/result/byte/resource
cap or successful-step lifetime quota was increased.

### Current source paths and ownership receipt

The two repairs add changes only to the existing RAW owner/tests and Fork budget.
The full uncommitted seam read-set/source diff is:

```text
crates/oc-adapters/src/runtime/context.rs
crates/oc-adapters/src/runtime/turn.rs
crates/oc-adapters/src/runtime_compaction.rs
crates/oc-adapters/src/storage.rs
crates/oc-adapters/src/storage/tests.rs
crates/oc-adapters/src/storage_compaction.rs
crates/oc-adapters/src/storage_dcp_view.rs
crates/oc-adapters/src/storage_fork.rs
crates/oc-adapters/src/storage_grants.rs
crates/oc-adapters/src/storage_turn_history.rs
crates/oc-adapters/src/storage_turn_history/tests.rs
crates/oc-adapters/src/tools.rs
crates/oc-adapters/src/tools/model_history.rs
crates/oc-adapters/src/tools/turn_history.rs
docs/CODE_MAP.md
```

All current coupled-proof fixture owners are joined and removed. Exact names below
are relative to `/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`;
the final three full-test directories were removed only after exact PID1284987
exited. No foreign prune or Cargo tree copy was used. The final release validation
checks these exact paths, evidence quota, workspace aggregate and current hashes.

```text
t54-retry-9ickd6_a
t54-retry-bia43xs1
t54-retry-0h3m9wy6
t54-retry-ocp2rga2
t54-retry-9f8oks41
t54-retry-sr1c4tv7
t50-background-owned-6127bfrh
t50-background-owned-uz5iqvzi
t50-background-owned-s57qh4pv
t50-background-owned-eai7nu1w
t50-background-owned-rn_ev3vq
t50-background-owned-4i1h89cm
oc-tui-workspace-primary-1284987
oc-tui-workspace-stale-1284987
oc-tui-workspace-vanished-1284987
```

Current source qualification does not finish whole T45/READY/Long Horizon. Separate
whole-past DCP11 renewal/defaults/child/profile controls and the other independent
T45 slices remain as recorded below. T54/T55 historical completion, paused T44 and
the exhausted24G/15C/1MCP paid ledger are preserved. This owner changes no
progress/spec/GOAL/staging/commits, current authoring HOME/auth, `.local/live.env`
or foreign `.opencode/`.

`review-final-evidence-validation` verified 42 summaries/1461/0/10, eight owner
tests, Python47, both retained RED counterexamples, current normal ELF hashes and
absence of all15 exact final owned paths. Evidence occupied416327 B at that check;
the completed log and final receipt remain below420 KiB, within1 MiB. Every failed
log is retained. Git diff whitespace validation passes; no Rust file exceeds the
5000-line soft guide. Current source remains uncommitted and source gates were not
invalidated by the final evidence-only edits.

**Current release:** sole mutation/Cargo/offline fixture ownership returns to the
parent. All processes, listeners, HTTP handlers, PTY readers and watchdogs are
joined, with no retained owned temporary directory. The earlier release below
belongs to the historical1459 phase.

## Historical qualification — initial 1459 phase before parent-review repair

The canonical closed RAW/HOT seam is implemented and qualified on dirty source
based on `83054c997e5f9c2f62815f685299bc44127da06e`, active T45/0004. The frozen
design and original RED record below remain historical. This result covers the
closed ongoing-task native checkpoint boundary; it does not finish whole T45,
whole DCP11/A10/Long Horizon, DCP defaults/profile controls or paused T44 visuals.

At final handoff Git HEAD is `888b411118f876266b2a5e139465fe2cec2bc061`.
External commits `464a6c38e` and `888b41111` amend T44 documentation/planning only;
they change no Rust, Cargo or runtime fixture source. Their changes are preserved.
The qualified Rust content is the execution base above plus this owner's dirty
source; no source changed after the final qualification builds.

### Final source gates (after the last production edit)

All commands used the existing offline single-Cargo owner: jobs 3, test threads 1,
locked dependencies, the owned `oc-test-bench-20260924` TMPDIR and the unchanged
1798-second operational watchdog. No product timeout, byte, token or result cap
was raised. Full precompilation and workspace execution were separate commands.

| Gate / lossless log stem | Exit | Result / seconds |
| --- | --- | --- |
| `closed-final-fmt-02` | 0 | workspace format check, 2.661 |
| `closed-final-clippy-02` | 0 | workspace locked offline all-targets `-D warnings`, 19.259 |
| `closed-final-precompile` | 0 | workspace locked offline `test --no-run`, 65.089 |
| `closed-final-workspace` | 0 | **42 summaries; 1459 passed / 0 failed / 10 ignored**, 1000.774 |
| `closed-final-debug` | 0 | normal workspace debug build, 8.423 |
| `closed-final-release` | 0 | normal workspace release build, 150.125 |
| `closed-final-debug-help`, `closed-final-release-help` | 0 / 0 | normal ELF help |
| `closed-final-python` | 0 | Python **47/47**, 12.005 |
| `closed-final-docs`, `closed-final-progress`, `closed-final-size` | 0 / 0 / 0 | structural checks after final report, 0.181 / 0.106 / 1.576 |
| `closed-final-temp-cleanup` | 0 | exact final joined test-owner cleanup, 0.064 |

Normal ELF hashes, checked unchanged before/after the actual native workloads:

- Debug: `d6e3c275081accd2c6bad26968ce94b3564db72a2c8f09a83cd229eec1fdd27e`.
- Release: `7af7a7874e470130759d57f34aafc3bced0f9fdcf5e9ec37ae837ba2f8e930b6`.

### Final physical current-task measurements

`closed-final-hot-debug` / `closed-final-hot-release` both exit 0, after the final
builds. Each runs growing root 8/32 groups and an actual foreground child with 32
groups. All use the same admitted low-context model budget, 8192-byte settled
shell results, one committed effect, an early retry, a later429, a partial-output
failed span plus new continuation and admitted counted checkpoint retries. The
child owns its own RAW/HOT while the parent invocation remains open.

| Root measurement | 8 groups | 32 groups |
| --- | ---: | ---: |
| Peak active checkpoint serialization, both builds | **33755 B** | **33755 B** |
| Peak actual main request JSON, both builds | **31922 B** | **31922 B** |
| Final selected HOT checkpoint, both builds | 2790 B | 2796 B |
| Last actual main request JSON, both builds | 6295 B | 6295 B |
| Maximum resident tail input / receipts / spans, both builds | 8 / 6 / 5 | 8 / 6 / 5 |
| Maximum observed HOT transfer per continuation, both builds | 1 row / **22765 B** | 1 row / **22765 B** |
| Implicit whole RAW-page transfer, both builds | **0 rows / 0 B** | **0 rows / 0 B** |
| Original RAW+tail bytes, both builds | 84853 B | 322722 B |
| Immutable closed segments, both builds | 2 | 8 |
| Actual main / compaction / title POSTs, both builds | 12 / 3 / 1 | 36 / 9 / 1 |
| Observed predispatch checkpoint-byte sum, both builds | 144247 B | 554705 B |
| Peak process VmHWM, debug | 50679808 B | 53063680 B |
| Peak process VmHWM, release | 20324352 B | 21860352 B |
| Last DB bytes, debug | 700416 B | 1433600 B |
| Last DB bytes, release | 700416 B | 1421312 B |
| Last WAL occupancy / 4096-page frame count, debug | 4169472 B / 1012 | 4169472 B / 1012 |
| Last WAL occupancy / frame count, release | 4169472 B / 1012 | 4144752 B / 1006 |
| Aggregate process `write_bytes`, debug | 6250496 B | 18198528 B |
| Aggregate process `write_bytes`, release | 6217728 B | 18092032 B |
| Process write bytes / original RAW byte, debug | 73.663 | 56.391 |
| Process write bytes / original RAW byte, release | 73.276 | 56.061 |

Each old segment's bytes were hashed again at later real main requests and never
changed. Ordered original results, sequential reused-ID occurrences 0..N-1,
completed tool states and the single effect were verified by explicit final RAW
queries. Durable dispatch-lane counts equal actual sockets in every case. Debug
root RAW hashes are `07dddc2df114b7967d71919d34c1e511258e7a80d309817b1ebaa2ac258b4ee9`
and `a1cc2f12903644c0a5f0c2c309f80590a15ef0ae2f76446f68a3eba500c84dda`;
release root hashes are
`8aaea575329973988de477e18c290e752ff963c6c144b013ac9513a5d469aae5`
and `2f37aaea356324ab64da8f7bd3dadea403f8eddfc9f28299c9ec9ac120634ee1`.

The 32-group child has peak checkpoint/wire 33912/32062 B, final HOT 2845 B,
eight immutable segments, 323629 B original RAW+tail, maximum HOT transfer
1 row/22897 B and zero implicit RAW pages. Actual lanes are main2/child36/
compaction9/title1. Peak VmHWM debug/release is 53751808/22331392 B. Child hashes
and all `/proc/io` counters are retained in the complete lossless logs.

Measurement scope is explicit: VmHWM/VmRSS are physical **whole binary process**
memory, not an isolated Rust allocator instrument. Process I/O includes SQLite,
events and shell supervision, not SQLite-only amplification. WAL frames describe
current occupancy/reuse, not cumulative writes. Checkpoint sums are observed
predispatch checkpoints, not every callback serialization. The fixed six counters
measure transferred HOT journal strings, explicit RAW-page strings and bounded UI
span/part strings; they do not count every SQLite scalar/preview/fork read. Latest
UI serving is independently bounded to 192 spans/240 parts; its cumulative totals
grow with service, and its window fills to those existing query bounds. The owner
test with 301 segments verifies those limits do not stop execution and every part
retains its actual owning span. Disk RAW growth is expected, not HOT retention.

### Atomicity, consumer and adjacent proofs

- Six new owner tests cover failure before append, after append before HOT update,
  deferred-FK failure at COMMIT, unchanged old HOT, immutable original arrays and
  references, post-commit reopen, unknown/open refusal, corrupt descriptors,
  bounded oversized-legacy refusal, actual MCP12 rich-media round trip/model-neutral
  selection, stable occurrences, exact-prefix fork rebasing, Undo/Redo, unchanged
  source RAW and latest presentation after 301 closed segments. Existing Read/MCP,
  ambient-instruction, compaction, retry, fork and resource/security tests pass in
  the final full workspace; production API was not expanded merely for tests.
- `closed-final-switch-debug` / `closed-final-switch-release` exit 0: actual busy
  picker commits A→B during an A-issued current summary; that summary retains A's
  snapshot and the next safe primary request uses B's schema/budget. Two segments
  contain original A/B receipts, the file effect remains once, restart dispatches
  zero cached requests, and a new explicit turn uses bounded selected HOT (28969 B
  request) with unchanged RAW hashes. Actual lanes main10/compaction2/title1.
- Both native switch runs also inject a before-segment-COMMIT SQLite fault and
  crash the process after committed segments. Before failure: no segment, old
  complete call/outcome journal preserved, failed turn, effect once. After crash:
  immutable segments preserved, unknown open turn, no recovery POST or tool replay.
  Unsettled external-effect quarantine also passes the unchanged actual-binary
  AUD06 kill/unknown recovery gate and the MCP intent/outcome fault gate.
- After the last builds, `closed-final-t50-{tool,retry,compact}` and
  `closed-final-t50-stream-release` exit 0. Issuing batches, busy retry, summary
  routing, committed choice/restart and model-compatible opaque projection remain
  qualified. `closed-final-mutation-{patch,write}` and
  `closed-final-cold-{chain,fault}` exit 0: actual mutations, registered distant
  ColdArtifact read, producer ownership/resource semantics and no effect replay.
- `closed-final-round-debug` / `closed-final-round-release` exit 0 for root17,
  child17, partial-throttle attribution and cancellation. Numeric round removal
  remains qualified; successful work acquires no replacement lifetime quota.

Failed experimental logs and the first 1457/2/10 full run remain lossless. The two
observed regressions were repaired at their actual owners: typed call provenance
is inserted atomically with intent rather than causing an early outcome UPDATE,
and pending-call coordinates use the same filtered predicate as replay input.
Final diff review also retired the all-session DCP mark loader from production;
both old-session and current-turn compaction use indexed admitted-window lookup.
The former loader exists only for existing unit assertions, not runtime execution.

### Ownership and remaining scope

All Cargo children, watchdogs, normal binary subprocesses, PTY readers, HTTP server
and handler threads are joined. Each fixture prints its exact acquired path and
joined removal. Three known `tui_workspace.rs::test_db` directories per full run
are cleaned only after that exact test PID exits; cleanup logs enumerate the exact
owned paths, without foreign pruning. Final release/check evidence records total
new bytes below 1 MiB and current binary hashes. No temporary owner is retained.

`closed-final-evidence-validation` exits 0: AST validation of all four new Python
fixtures/runners, exact 42-summary/1459/0/10 aggregation, Python47, both unchanged
normal ELF hashes, and evidence quota. At validation time new evidence occupied
320204 B; the completed validation log and this final receipt remain below 322 KiB
and below the 1 MiB limit. All failed logs are retained.

The final coupled-proof roots and final full-test leftovers below are removed.
Every name is relative to the exact owned parent
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`:

```text
t54-retry-lti_diy8
t54-retry-6c663ous
t54-retry-amjba1_9
t54-retry-xvm4k7l1
t54-retry-q434rnwr
t54-retry-zgzrh71n
t50-background-owned-5syvq_y3
t50-background-owned-bq93d9_n
t50-background-owned-39q2s_xv
t50-background-owned-xvdnpy6_
t50-background-owned-r7c5156p
t50-background-owned-vktxqeqg
oc-tui-workspace-primary-1218640
oc-tui-workspace-stale-1218640
oc-tui-workspace-vanished-1218640
```

**Release:** sole mutation, Cargo and fixture ownership returns to the parent;
there is no retained process, listener, watchdog, PTY reader or temporary directory
owned by this execution. This is an uncommitted-source handoff.

This owner made no commit/stage/progress/GOAL/spec changes. Foreign `.opencode/`, current
authoring HOME/auth and `.local/live.env` were untouched. No paid/live invocation;
the exhausted T27 allowance remains 24G/15C/1MCP. T54/T55 historical completion is
preserved. Remaining separate R9/DCP11 work is whole-past DCP block/checkpoint
renewal through range compression, global renewable forget/default/child controls
and full policy/config/profile acceptance. Current groups use the existing admitted
native compaction policy; explicit tool/path/tag protections and live shell control
groups remain structural, while the current task is irreducible. A genuinely
irreducible open/protected pack can still fail existing resource admission honestly.
ConfigSource, built-ins/MCP grants, GO03/T56 and paused T44 paired pixel gates are
independent. This report does not mark whole T45/READY/Long Horizon PASS.

## Historical implementation checkpoints

### Historical prototype qualification (after the original design/RED stage)

Production owners now implement schema 8, current closed-group native compaction,
atomic canonical delta/HOT installation, original receipts/occurrence coordinates,
selected native media, bounded retry/wire reads, indexed latest presentation and
explicit admitted prefix fork copying. Prior session checkpoint/past context is
included in admitted current consolidation and retired in that same transaction;
previous selected summaries are replaced, never chained into RAW.

The normal debug prototype `5d72c489a3edc839a17ea0e7b18ac63b3de03c4e39a69498ded11aff6f0361f1`
passed growing root 8/32 and child 32 groups (`hot-debug-experiment-02`, exit 0,
77.381s). Root peak checkpoint 33,755 bytes and peak wire 31,922 bytes were equal
for 8/32; final checkpoints 2,790/2,796 bytes. Physical POST counts exactly matched
durable dispatch lanes, reused call occurrences were 0..N-1, old segment hashes
remained unchanged, the effect occurred once, and partial-output failure had a
durably closed failed span/distinct continuation. Explicit RAW-page counters were
zero on ordinary execution; presentation counters are a separately bounded UI
window and cumulative served totals, not execution limits or implicit full pages.
The busy native picker/current compaction/reopen fixture also passed
(`hot-switch-debug-experiment-02`, exit 0, 7.798s): A-issued summary remains A,
next primary uses B tools/budget, RAW survives restart and the file effect is not
replayed. These are interim builds, not final qualification.

Strict workspace all-targets Clippy and separate offline precompilation passed.
The first full workspace run (`final-workspace`, 985.351s, exit 101) completed all
42 summaries but exposed two real regressions: a call-metadata UPDATE triggered
the injected MCP outcome fault at intent creation, and pending-call replay
filtering retained an invalid occurrence coordinate. Metadata now belongs to the
original intent/refusal INSERT, and projected occurrence coordinates use the same
answered-call predicate as input. Neither failing test nor its budget changed.
All failed logs remain lossless. This historical checkpoint preceded the completed
repair and full requalification recorded above.

The additive schema-8 owner is now implemented in `storage_turn_history.rs`:
immutable `(turn_id,ordinal)` deltas, fixed seven-counter descriptor, indexed
original endpoints, checkpoint CAS and one transaction before resident eviction.
`tools/turn_history.rs` keeps selected HOT separate from new original input.
Nullable indexed tool-operation call metadata records canonical input coordinate
and occurrence through the intent/refusal transaction, without parsing op names.

`boundary-tests-04` passed the two owner scenarios (52.931s, command exit 0):
rollback before append / after append before HOT update, immutable RAW, bounded
explicit page admission, post-commit reopen, preserved original receipt/span/
instruction/notice/opaque references, open/unknown rejection and corrupt version/
descriptor errors. Earlier compile experiments `boundary-tests-01..03` exit 101
are retained losslessly. These two tests do not yet qualify rich media, repeated
occurrences, latest-window readers, fork rebasing or the coupled runtime seam.

Current-turn compaction now shares the existing counted/admitted summary/retry/
correction/cancellation policy. The initial routing experiment failed one of 15
existing compaction scenarios (`current-compile-02`, 14/1); it compacted current
work while leaving eligible past history on wire. Routing now services eligible
past history first; `compact-regression-03` passes the unchanged regression
(63.448s, exit 0). Runtime captures closed counters after the complete settled
batch; notices appended later stay in the fresh tail. HOT report text/calls clear
only after successful durable replacement. Full qualification remains pending.

All new command output remains lossless gzip in `hot-raw-logs/`; joined command
owners report an empty new-TMPDIR delta. No paid/live invocation or source gate
PASS is inferred from these incremental tests.

Source base: `83054c997e5f9c2f62815f685299bc44127da06e`.
Active task/checkpoint: T45/0004. This is separate from the qualified numeric
round-removal slice. No Long Horizon, A10, DCP11 or whole-T45 PASS is claimed.

## Required outcomes (frozen before production edits)

- One ongoing root/child turn may have indefinitely many **closed** Responses
  groups, subject to the existing finite resource/security/failure guards.
- RAW preserves ordered original Responses input, all opaque items, request
  receipts, recorded spans and display ownership, native Read/MCP attachments,
  instruction event/index references and shell-notice references. Tool operations
  alone are not an adequate raw journal.
- Ordinary provider/retry/model-switch/DCP/compact/reopen reads and checkpoints
  depend on HOT, not on the size of the forgotten prefix. Explicit, bounded raw
  history/Undo/Redo/Fork remain possible. No implicit archive retrieval.
- Eviction occurs only after a single SQLite commit installs both immutable RAW
  delta and a validated scalar descriptor plus self-contained HOT. Any failure
  before commit leaves the previous resident hot state usable. A committed effect
  is never replayed; unknown effects remain quarantined.
- The protected current task/pack and explicitly selected facts survive. A new
  working checkpoint replaces its predecessor; terminal historical task packs
  are not permanently pinned. Model-generated consolidation uses the existing
  admitted, counted, cancellable, finite-retry/correction compaction policy.
- Equal HOT states over increasing closed OLD prefixes have bounded retained/peak
  memory, serialized checkpoint bytes and per-step DB/WAL work. Disk RAW grows.
  Existing byte/token/result/buffer/file caps are not increased. An irreducible
  open group may fail honestly; no age/step/lifetime-work stop is introduced.

## Necessity and selected safe hypothesis

`TurnLog.to_json` is the only lossless current canonical journal;
`checkpoint_turn` replaces it. Conversation object versions do not version this
field. `tool_operations` cannot reconstruct response ordering, opaque items,
receipts, media attachments or provenance. Therefore moving forgotten data out
of resident input without another durable representation loses user RAW.
Existing session checkpoints cannot represent this boundary: they only advance
whole-message sequence boundaries and protect the started user message.

The smallest candidate is **one immutable closed-segment table in the existing
Db**, plus a fixed versioned scalar prefix descriptor in the existing hot turn
row. Migration 8 is justified only by this missing immutable canonical prefix;
it must not introduce a general archive service, a second store or a growing
reference vector. This is the selected hypothesis, not a schema-free claim or
an implementation PASS.

Each segment stores a bounded canonical delta, its original scalar coordinate
base/end, and its closed response/span boundary. Indexed `(turn_id, ordinal)`
and original-coordinate range queries support explicit consumers. Segment JSON
preserves native attachment coordinates and request/instruction/display indexes
in the segment's original coordinate domain. RAW delta contains new original
items only, never repeated copies of previous chosen working checkpoints.
HOT's chosen working input is separate from unsealed original input; previous
summary and task copies must not become a recurring raw prefix. Per-field
coordinate offsets remain fixed-size counters, not lists of archived ids.

No array can silently retain lifetime growth: input, requests, spans, opaque,
instruction references, notice references, display parts, accumulated assistant
text and returned call records all need an explicit owner/representation.
Original raw coordinates and bounded hot projection coordinates are distinct;
model-compatible projection must use each retained item's issuing receipt.
Selecting facts must not turn a hot copy into a new original occurrence.

## Closed boundary and transaction/crash proof obligations

Seal only after a provider response has settled and **all** its call outcomes
are durable, with no open stream, question/Ask, child or in-flight batch. A
pre-output retry reuses its existing span. A partial-output failure settles a
failed span and starts a distinct continuation; it does not pretend completion
of unresolved calls. Unknown effects cannot produce an evictable boundary.

At a safe boundary, current T50 selection supplies the next coherent budget,
tool family and hot projection; the already issued request/tools retain their
captured view. Compaction consolidates only current HOT + latest chosen facts,
not an archive and not a default permanently protected historical suffix.

Transaction order: compare the expected old prefix/hot revision and started
status; validate the closed original delta against durable outcomes; insert the
immutable segment; update scalar descriptor and bounded hot result/checkpoint;
commit. Only then swap resident state. Test rollback before insertion, between
insertion/update and before commit, and reopening after successful commit.
Recovery validates descriptor/version/last segment metadata through indexed
scalar queries, never a full archive scan. Corrupt descriptors fail explicitly.
Legacy unsegmented journals have a bounded fallback and are not rewritten in
committed history. Fork copies/rebases only the explicitly requested actual
prefix, applying its existing row/byte admission before loading source payloads.

## Concrete owner seams / tradeoffs

- `crates/oc-adapters/src/tools.rs` and `tools/{model_history,mcp_log}.rs`:
  raw/hot representation, native attachments and issuing-model projection.
- `runtime/turn.rs`: closed boundary, partial retry restore, latest T50 selection,
  all lifetime accumulators and atomic resident replacement.
- `runtime_compaction.rs`: existing admitted summary policy; current HOT must
  become eligible before admission refuses the original request. Present code
  first calls active projection and protects the started turn, so simply
  attaching an archive does not fix the compaction escape.
- `storage.rs`, `storage_{instructions,compaction,dcp_view,conversation,fork}.rs`:
  atomic durable hot/RAW, indexed metadata, bounded projection/explicit history,
  original instruction references and requested-scope fork rebasing.
- `storage.rs` presentation must query the latest 192 spans / 240 parts and
  their owning references; those limits are query windows, never execution caps.

Tradeoff: schema 8 and coordinate-aware explicit readers are necessary if this
hypothesis succeeds. Merely trimming `TurnLog.input`, archiving tool outputs,
keeping full receipts/spans resident, appending a growing summary chain, or
reloading full RAW before every compact/retry would be incorrect.

### Selected persisted shape / remaining closure decisions

The necessary payload owner remains `Db`; this is a proposed **additive schema
8**, not an applied migration. The concrete minimal shape to validate is:

```text
turn_raw_segments
  turn_id, ordinal                     PRIMARY KEY(turn_id, ordinal)
  input_end, request_end, span_end, opaque_end, part_end,
  reference_end, notice_end             fixed scalar original-array counters
  payload                              canonical closed delta + fixed bases

indexes: (turn_id, input_end), (turn_id, span_end), (turn_id, part_end)
turns.result.raw_prefix
  version, ordinal, the same seven end counters, delivered_notice_seq
```

Segment payload preserves original canonical data, not a summary or copied hot
prefix. Its fixed bases give every item/reference its original coordinate.
Self-contained hot input needs a bounded mapping from retained hot coordinates
to original coordinates and owning receipts. Summary items are explicitly
checkpoint-owned, not invented original occurrences. The mapping contains only
currently selected items, never one descriptor per historical segment. Unknown
descriptor versions, impossible counters, wrong turn ownership, or disagreement
with the last indexed segment are explicit errors. Normal restore checks indexed
last-row scalar metadata, not a scan of old payloads. Immutability/contiguous
append constraints must be enforced by the transaction and storage owner.

**A segment table alone is insufficient for reused call IDs.**
`runtime/context.rs:212–255` derives turn-local occurrence by enumerating the
entire original log. Eviction resets that count; carrying a lifetime map of IDs
would be another unbounded hot owner. The existing tool operation row is the
smallest available durable call owner. It needs typed `provider_call_id`,
`call_occurrence`, and `original_input_index` metadata with an index on
`(turn_id, provider_call_id, call_occurrence DESC)`. Assign through the existing
intent/refusal transaction using the canonical call coordinate; do not infer
from the human-readable operation ID or tool arguments. Latest-occurrence lookup
is one indexed scalar row, not `COUNT` over old raw calls. An assembler failure
without an original canonical function call has no fabricated call coordinate.
New nullable fields do not require rewriting settled historical source logs.
The bounded hot occurrence mapping supplies stable DCP coverage identity while
window-local marks are remapped only at the same committed context revision.

`storage_dcp_view.rs:49–85` is another concrete consumer dependency: its
`presentation_wire_logs` SQL whitelist currently omits `requests`, while
`tools/model_history.rs` uses those receipts' `input_start` to attribute input
to the issuing model. A new prefix descriptor alone cannot repair that
projection. The bounded selected wire log needs retained receipts and a matching
original-to-hot input-coordinate projection after SQL filtering. Raw receipt
coordinates must not be rewritten to match a new prefix-local enumeration.
Native attachment and instruction-reference SQL remapping must use the same
selected-item domain. Otherwise restarting/model-switching a segmented turn
can lose correct issuing-model attribution even though its original raw bytes
were archived. This is a source-derived dependency, not a qualified T50
regression or a reason to reopen its historical baseline.

**Closed counters must be captured at the outcome owner, not inferred at the
next loop head.** `run_turn_admitted` appends new shell notices before compact
selection. All previous spans being completed does not make that new notice a
closed response group. Capture the seven closed array ends after the complete
generation/batch outcome commit. Keep later notices, continuation input, an
open span and any in-flight question/child/batch in the unchanged fresh tail.
`shell_notice_seq` is already scalar; persist it, and exclude consumed current
notices using indexed message/notice ownership instead of retaining every old
notice ID in the hot vector. Latest ambient instruction facts stay owned by
`storage_instructions`' bounded current state; old references remain RAW.

The compaction entry must accept this bounded eligible current prefix and the
previous chosen working checkpoint before refusing the larger ordinary wire.
Its existing counted auxiliary/native request, finite retries, cancellation,
section validation and one admitted correction remain the policy owner. A
model-neutral textual checkpoint is necessary when selection can change A→B;
alien encrypted native state is not a replacement for selected public facts.
No free-text parser can decide permission/protection or fabricate state. Current
effective protected groups and required active resources have to be selected
structurally and carried whole, with their issuing receipts and native media.
If those groups themselves exceed admission, failure is honest.

`text` and `calls` in `runtime/turn.rs` are separate lifetime accumulators, in
addition to the seven journal arrays. Archiving the log while leaving those
vectors intact would fail retained-memory acceptance. After committed replacement
the report/presentation contract needs a bounded current tail and explicit raw
history for previous output, rather than silently promising all past calls as a
resident `TurnReport` vector. This consumer decision has not been implemented.

Presentation uses the latest 192 spans / 240 parts, ordered back into chronology,
with each served part's exact owning span metadata (not ordinal inference).
Indexed scalar part/span endpoints locate the explicit old window. Message and
native attachment lookup resolves the segment's original coordinate domain;
it does not use a local index from an unrelated hot input vector. Existing
physical dispatch counts, patch-effect/question budgets and omission flags stay.

Fork's existing source budget (`storage_fork.rs:89–147`) presently counts and
loads all source turns/tools before filtering the requested boundary. Adding
archived segments to that query would be both eager and incorrectly scoped.
First resolve the actual requested prefix through `turn_acceptances` and the
conversation view, admit only its rows/bytes, then copy/rebase its hot and raw
rows under the same transaction. Turn/tool/message/instruction references must
refer to copied actual-prefix objects. Undo/Redo continues to select whole turns;
the immutable segments follow the selected turn visibility, not a new timeline.

These dependencies are part of the same boundary. The selected shape is not
yet an executable implementation/proof: there are no schema-8 production edits,
no descriptor parser, raw/hot transaction, bounded explicit consumer, or current
compact path installed. Treating the initial segment-table suggestion as ready
to trim vectors would violate the owner contract.

## Frozen offline workload / qualification status

Use an owned loopback service and the real normal debug/release ELF in an owned
HOME/TMPDIR. Baseline first: repeated closed groups with a committed effect,
equal newest working facts over increasing OLD groups, raw/checkpoint byte
growth, process VmRSS/VmHWM and `/proc/io`, DB/WAL bytes/frames and physical POST
counts. A low-context current-only run must expose the existing compact escape.
This baseline is RED reproduction, not boundedness qualification.

After implementation, equal committed HOT across growing RAW must additionally
exercise admitted forget/compact, finite retry/partial continuation, A→B switch,
restart/no replay, explicit bounded RAW/Undo/Redo/Fork, fault points before/after
segment+hot commit, known/unknown effects, media/instruction/notice/span identity.
Record raw byte/hash integrity, loaded rows/bytes, peak retained RAM, checkpoint
serialized bytes and DB/WAL/I/O amplification. Preserve root/child and nearest
T50/T54/T55/resource/security contracts. Full workspace/build/Python/docs gates
are mandatory after production edits; no gates have been rerun here yet.

## Executed RED — normal debug ELF

`red-debug-03` exits 0 because it successfully **detects RED**, not because the
runtime is bounded. Physical POST counters equal durable dispatch counts. One
committed shell effect remains exactly once in every case. No compact request
or snapshot occurs in the first current-only task even with automatic compact
enabled and a deliberately smaller fixture context. It fails honestly after
13 settled groups. Product limits have not been changed.

| Measurement | 8 old closed groups | 32 old closed groups |
| --- | ---: | ---: |
| Main/title physical POSTs | 9/1 | 33/1 |
| Final raw/checkpoint bytes | 79,560 | 312,165 |
| Last pre-dispatch serialized checkpoint bytes | 79,301 | 311,903 |
| Last wire JSON bytes | 73,653 | 276,402 |
| Hot input items / requests / spans | 17 / 9 / 9 | 65 / 33 / 33 |
| Sum checkpoint sizes at observed POST boundaries | 365,169 | 5,175,849 |
| Peak observed process VmHWM bytes | 51,539,968 | 61,149,184 |
| Last process VmRSS bytes | 51,425,280 | 59,342,848 |
| Last process `wchar` | 5,692,297 | 39,834,223 |
| Last process `write_bytes` | 6,029,312 | 37,015,552 |
| Last DB bytes | 700,416 | 1,757,184 |
| Last WAL bytes / frames | 4,132,392 / 1,003 | 4,391,952 / 1,066 |

The requested newest fact and per-result payload are identical (8,192 public
stdout bytes). The baseline cannot construct equal HOT states: the old groups
remain in both resident/provider input and the serialized checkpoint. This is
the specific disproof, not a small/large bounded-state PASS. VmHWM measures whole
binary process high-water memory; it is not an isolated retained-Rust-owner
instrument. `/proc/io` is process aggregate, not SQLite-only I/O. WAL frame counts
are current physical file occupancy, not cumulative write counts (checkpointing
reuses the WAL). Loaded row/byte instrumentation is not implemented.

Lossless logs: `evidence/T45/hot-raw-logs/red-debug-01.log.gz` (fixture assertion
on shell envelope; killed/joined, no new temp paths), `red-debug-02.log.gz`
(fixture SQL assumed scalar state instead of existing snapshot JSON; joined and
exact temp removed), and successful reproduction `red-debug-03.log.gz`. Failed
logs are retained. The successful reproduction records each raw byte hash and
all exact owned temporary paths, their joined cleanup, exit statuses and
physical dispatch measurements.

Normal debug ELF before/after SHA256:
`b1e5ee0e763a3645cc0866b56e7bfcf6d21d4b4662a6c9852d2d34ad71ef6024`.
No build has run during this design/RED stage.

Release reproduction `red-release-01` also exits 0 detecting the same RED:
8/32 groups produce 79,560/312,165 raw bytes, 79,301/311,903 last checkpoint
bytes, 73,653/276,402 wire bytes and 365,169/5,175,849 observed checkpoint-byte
sums. Peak observed process VmHWM is 20,811,776/26,947,584 bytes; last VmRSS is
20,811,776/25,464,832. Process writes are 6,029,312/36,917,248 bytes. The
low-context current-only run again settles 13 groups, exits 1, preserves the
single effect, and dispatches no compaction request/snapshot. Its raw bytes
are 126,629. The complete log records raw hashes, physical counts and exact
owned temporary paths and cleanup.

Normal release ELF before/after SHA256:
`09a8d31fd9558d0fab05825c7e80112f4dbba648bd7362506edcb45dbc6ba526`.

### Historical gate state — before production implementation

The new fixture has successful RED-reproduction command exits 0/0 (debug/release)
and retained failed-fixture exits 1/1. This is not a product-gate PASS. One Cargo
`fmt --all -- --check` owner has completed and joined with exit 0; no compilation
owner is retained. There is no new debug/release build, workspace precompile,
42-target workspace test run, strict Clippy or implementation qualification has
been performed for this seam. The source base's historical full workspace
1453 passed / 0 failed / 10 ignored across 42 summaries remains historical; it
cannot qualify a seam that does not exist. Equal committed small/large HOT,
RAW immutability across eviction, retry/switch/restart/Undo/Redo/Fork, fault-point
and loaded-row/byte measurements are all **NOT_RUN** for an implementation.

The six successful-run temporary roots and `red-debug-02`'s root are printed
in the logs and were removed only after the binary process and every HTTP
handler/server owner joined. `red-debug-01` used the same exact-owner cleanup
path, but its temporary basename was not printed before its fixture failure;
the wrapper observed zero remaining new TMPDIR paths. No active process,
HTTP listener, watchdog or temporary-directory owner is retained.

Exact printed fixture roots, all removed after joins, under
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`:

- `t54-retry-m04ibxwh` — failed fixture SQL experiment, `red-debug-02`.
- `t54-retry-p70n_qvz`, `t54-retry-ixdmry50`, `t54-retry-782yfve8` — debug
  8-group, 32-group and compact-refusal cases.
- `t54-retry-2dcvw87a`, `t54-retry-tfjerzz9`, `t54-retry-bzo6ozwv` — release
  8-group, 32-group and compact-refusal cases.

The first failed experiment's unprinted basename is an evidence gap, not an
unresolved retained owner; its exact-owner cleanup assertion completed and the
wrapper's post-join TMPDIR delta is empty. No foreign path was removed.

Artifact checks completed: workspace fmt exit 0; Python 47/47 exit 0; docs
structure exit 0; progress structure exit 0; code-size inventory exit 0
(348 files / 213,389 physical lines / 8,527,327 UTF-8 bytes, advisory);
Git tracked diff whitespace check exit 0; new-file AST/whitespace/evidence-byte
validation exit 0. The byte validator covers the three new files and this
phase's lossless logs only, with a 1 MiB ceiling; total is below 64 KiB.
Python tests' ephemeral `tmp_hd75lor` / `tmptyywh782` were visible to parallel
read-only checks; the joined Python owner's final TMPDIR delta is empty. Those
checks do not justify pruning any foreign TMPDIR path.

Source HEAD/base remains `83054c997e5f9c2f62815f685299bc44127da06e`.
Tracked source/config/spec/progress/GOAL/CodeMap diff is empty. This phase's dirty
state is only `evidence/T45/hot-raw.md`, `native_hot_raw_red.py`, `check_hot_raw.py`
and `hot-raw-logs/`; the pre-existing foreign untracked `.opencode/` is untouched.
No stage, commit, build, paid/live call or exhausted-ledger mutation occurred.
The normal debug/release hash assertions passed before/after both reproductions.

### Historical remaining engineering / acceptance — before implementation

1. Validate and install the minimal additive representation, exact original/hot
   coordinate domains, typed call identity, bounded legacy/error behavior and
   atomic closed-delta + hot CAS. Test rollback and post-commit reopen at owners.
2. Wire closed-boundary capture, admitted current-prefix replacement, every
   lifetime accumulator, bounded retry/restore/model-switch/DCP marks, latest
   ambient instructions and shell-notice cursor. Keep issuing views, unknown
   effects and all open/protected groups intact. Implement compact escape without
   requiring full oversized `before` admission or reading archived payloads.
3. Update latest-window presentation/explicit raw pages and requested-prefix
   fork rebasing; preserve conversation Undo/Redo and actual-source ownership.
4. Build and prove real normal-ELF equal committed HOT over growing old current
   groups, with admitted forget/compact, retries/partial continuation, A→B,
   restart/no replay, explicit RAW/Undo/Redo/Fork, media/notices/instructions and
   before/after atomic fault cases. Add actual loaded-row/byte measurements and
   retained/peak/checkpoint/DB/WAL amplification, not a schema-only simulation.
5. After the last production edit run separate offline jobs3/threads1 precompile
   and full locked workspace `--no-fail-fast` (1798-second outer watchdog,
   Bash1800000), strict locked workspace all-target Clippy `-D warnings`, fmt,
   normal debug/release build/help, nearest T50/retry/currentMutation/ColdArtifact
   risk regressions, Python47/docs/progress/diff/size and final ELF workload.
   Preserve actual 42 target summaries/exits; historical sums are not new gates.

DCP11 whole-past-block renewal/defaults/child controls/profiles may remain a
later separate slice where uncoupled. ConfigSource/profile registration, built-in
MCP grants, host-context/GO03/T56 and paused T44 pixels are independent and have
not been implemented here. T54/T55 historical completion is not reopened.

Historical design-stage status: selected storage hypothesis frozen; executable current-turn
growth/compact RED reproduced. Coordinate/consumer closure remains pending.
At that historical point production changes were none and the current-task seam
was NOT IMPLEMENTED / NOT QUALIFIED. The final implemented qualification and
remaining separate scope are recorded at the beginning of this report.
