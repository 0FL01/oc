# T50 R9 / captured file-family execution

## Coordinator qualification

Independently reviewed the shared mutation/approval/resource/preimage path,
donor matching/bounds, captured request allowset, DCP parsed-path protection,
persisted effects and native fixture. Two read-only scoped reviews found no
concrete introduced counterexample; these reviews alone are not runtime proof.
Owner documentation commit `3efaf719f3103ec78f4729e867168b297f485558`
contains no Rust/Cargo/fixture change above source BASE below and was preserved.

Fresh independent commands, all exit0, with prescribed offline jobs3/threads1
and owned TMPDIR:
- adapter lib `tool20_`: 4 PASS; lib `tool12_`: 3 PASS;
- existing runtime target `tool20_`: 3 PASS; `tool12_`: 3 PASS;
- workspace fmt and strict locked all-target Clippy; normal `cargo build --locked`;
- after that last build, direct `native_file_mutations.py` against both normal
  ELFs: **22 + 22 PASS**, 46 requests and 31 operation rows per ELF. Real bytes,
  paired results, Ask/preimage/cancel, exclusions, child ceilings, storage failure
  and unknown/reopen boundaries were checked. Owned groups/HTTP workers joined;
- Python47, docs/progress structure and diff checks: PASS.

Independent complete-log inspection found 42 successful target summaries totaling
**1409 passed / 0 failed / 10 unchanged existing ignores** in the full workspace
log below. No Rust input changed after that gate. Current normal fingerprints
below matched before and after both independent native suites; no intervening
Cargo command. Source association remains BASE + reviewed dirty implementation,
not a clean-BASE artifact. No actual live campaign or user configuration accessed.

This closes the frozen executor/captured-family slice, not the next mandatory
same-task busy draft/commit/request-rebuild work or new shell controls. T50 remains
active; T44 remains paused. The frozen evidence and development history follow.

Frozen before RED against source BASE `71fcd0f06bf618f06d4b5972fb312123f24378d2`.
Donor: OC2 `2670273ff17da96f85c5826ced57aa1b368754fa`, core tool/plugin/edit.ts,
write.ts and file-mutation.ts. This atomic slice couples working edit/write with
the selected request's catalog and execution allowset. T50 remains active; T44
visual qualification remains paused. Full busy picker draft/commit and same-task
request-boundary live selection is the NEXT atomic slice, not qualified here.

## Frozen obligations

- Exact case-sensitive selected model.id contains `gpt-`, excludes `oss` and
  `gpt-4`: apply_patch only; otherwise edit/write only. Effective policy narrows.
  Root and own-model child use their own captured selection. Excluded calls get
  durable matched failed results without effects. Native patchText stays unchanged.
- Write creates/overwrites/empties and creates missing parents, preserving supplied
  newlines/EOF and an existing or supplied single UTF-8 BOM. No synthetic patch Add.
- Edit requires an existing text file; empty/identical old, no match and ambiguous
  unique edit fail; replaceAll counts nonoverlapping matches, empty new deletes.
  Exact precedes typography normalization, then trailing-whitespace line matching;
  preserve actual boundaries, CRLF/BOM and native EOF semantics. Strict patch unchanged.
- One shared private patch filesystem/effects owner: prepared bytes/digests,
  directory-relative no-follow, per-path staging/fsync/commit, approved preimage
  identity/bytes/absence recheck. Preview before Ask and recheck after wait/before
  effects. Legacy apply_patch permission identity, grants, Deny/Ask/Allow, protected
  paths/data root and primary/child/Plan ceilings remain effective. No prior read call.
- Exact intent durable before effects; typed confirmed effects after. Partial,
  unknown/storage/cancel boundaries remain truthful and never auto-reexecute.
- Parsed edit/write path is the DCP/mutation resource, never content/old/new text.
  Old raw calls/results, profiles/user text and namespaced MCP suffixes stay intact.
  Current-turn/restart/DCP/compaction catalogs follow committed selection within
  the existing captured-turn boundary; this is not full same-task live-switch parity.
- Bounded saved result-derived metadata feeds a real UI consumer; replay never reads
  present-day files. Full VIS35/36 geometry is separate paused qualification.
- RED targeted mandatory behavior, owner tests, full fmt/strict clippy/workspace
  locked tests, normal debug/release build/help, Python47/docs/progress checks;
  then direct final ELFs with fake-provider file mutation and relevant T55/T54/
  TOOL16/control/historical patch regressions, without paid generation.

## Execution evidence

Qualified atomic backend slice against BASE above + DIRTY worktree, not a new
commit. No stage/commit/push/progress/GOAL/spec/acceptance mutation was performed.
Inherited `.opencode/` was not inspected. No paid generation, actual HOME/config,
live environment or exhausted T27 ledger was read or used.

### Ownership and review

- `patch/mutation.rs` owns native text preparation/matching, not a second mutation
  executor. `patch.rs::commit_prepared` and private `patch/{fs,effects}.rs` retain
  the single no-follow, staging/fsync, namespace-commit and confirmed-effects owner.
  The existing snapshot identity is also included in strict patch approval digests.
- Common `approval.rs`, `tools.rs`, `runtime.rs` and `runtime/turn.rs` own admission,
  exact captured invocation permits, legacy permission/grant identity, durable
  intent/outcome and issuing-request family exclusion. Native home/absolute paths
  are admitted lexically under the project; execution uses that same prepared call,
  while raw provider arguments remain unchanged in the journal.
- `runtime/context.rs` protects the parsed edit/write path only. Existing persisted
  PatchEffects feed `oc-tui/{history,tools,patch_view}.rs`, including distinct confirmed
  Write/Edit labels, without present-day file reads on replay. No schema, dependency,
  crate, public test-only API or extra store was added. CODE_MAP records the seam.
- New substantial tests are private owner modules and the existing runtime Cargo
  target. Ten new Rust scenarios bring the prior 1399 passing count to 1409.
  Existing synthetic patch fixtures use eligible `gpt-` fixture IDs; assertions,
  production IDs, old stored-history fixtures, deadlines and caps were preserved.
  The same necessary fixture-ID adjustment was made in the executable T55 fixture;
  historical evidence/checkpoint reports were not rewritten.
- Reviewed tracked diff, new mutation owner/tests and native helper; `git diff --check`
  passes. No foreign code writer was observed. Advisory inventory: 327 handwritten
  files, 204791 physical lines, no >5000-line warning. Native matching translates
  normalized offsets in place and bounds both search and source line indices.

### Checks and complete current logs

All paths below are under the absolute log root
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924/`.
Cargo used one invocation at a time with jobs3, test threads1, offline=true and that
TMPDIR; each invocation had a 900000ms bound. Logs are small: 36 owned logs total
370512 bytes at final fixture qualification, largest 129072 bytes.

| Check | Result / exit | Full current log filename |
| --- | --- | --- |
| Mandatory RED: overwrite existing BOM file via real write, no preceding read | expected failure: unknown tool write | `t50-r9-red.log` |
| Final nearest owner TOOL20 | 4 PASS, exit0 | `t50-r9-owner-final.log` |
| TOOL20 integration development | 3 PASS, exit0 | `t50-r9-tool20.log` |
| TOOL12 owner/integration | 3 + 3 PASS, exit0 | `t50-r9-tool12.log` |
| `cargo fmt --all --check` | PASS, exit0 | terminal; followed by successful recorded Clippy |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS, exit0 | `t50-r9-clippy.log` |
| `cargo test --workspace --locked --no-run` | PASS, exit0 | `t50-r9-precompile.log` |
| `cargo test --workspace --locked --no-fail-fast` | 1409 PASS / 0 failed / 10 existing ignored, 42 target summaries, exit0 | `t50-r9-workspace.log` |
| Python bounded-live13 + code-size5 + progress15 + check-docs14 | 47 PASS, exit0 | `t50-r9-python.log` |
| docs / progress read-only structure checks | PASS, exit0 each | `t50-r9-docs.log`, `t50-r9-progress.log` |
| code-size advisory against BASE | PASS, exit0 | `t50-r9-size.log` |
| Separate normal debug / release `cargo build --locked` | PASS, exit0 each | `t50-r9-debug-build.log`, `t50-r9-release-build.log` |
| Both direct normal ELF `--help`, isolated HOME | PASS, exit0 each | `t50-r9-debug-help.log`, `t50-r9-release-help.log` |

Python tests used `PYTHONPATH=scripts`, no bytecode retention. Native fixtures used
an isolated environment, synthetic loopback peers and their own HOME/project/data.
All below ran directly after the last NORMAL builds, with no intervening Cargo
builds or Rust changes. T55's fixture-only ID spelling was corrected after builds;
it does not change the ELF inputs.

| Direct normal ELF fixture | Debug / release cases, exit0 each | Full current logs |
| --- | --- | --- |
| `evidence/T50/native_file_mutations.py` | 22 / 22 | `t50-r9-native-debug.log`, `t50-r9-native-release.log` |
| `evidence/T55/native_completed.py` | 7 / 7 | `t50-r9-t55-debug.log`, `t50-r9-t55-release.log` |
| Same with `--reasoning-replay` | 7 / 7 | `t50-r9-t55-reasoning-debug.log`, `t50-r9-t55-reasoning-release.log` |
| `evidence/T54/native_runtime.py` | 9 / 9 | `t50-r9-t54-debug.log`, `t50-r9-t54-release.log` |
| TOOL16 `evidence/T50/native_read.py` | 20 / 20 | `t50-r9-read-debug.log`, `t50-r9-read-release.log` |
| Model/session controls with `--session-move-supported` | 19 / 19 | `t50-r9-control-debug.log`, `t50-r9-control-release.log` |
| Same-ID move directed `--case boundary` | 1 / 1 | `t50-r9-move-debug.log`, `t50-r9-move-release.log` |

Direct totals: 85 scenarios per ELF, 170 PASS overall. New file mutation fixture
has 46 captured provider requests and 31 operation rows per ELF, including failure
states. It checks actual bytes, typed matched wire results, durable graph/effects,
real PTY Ask/changed-preimage/cancel, own-model children, no-effect Deny/exclusion,
storage-intent failure, effect-then-storage failure recovering unknown without
reexecution, and actual reopened TUI Write card after present-day file mutation.
Historical strict patch regressions (including golden coding workflow and unchanged
parser/effects/audit tests) also pass in the workspace; the direct new patch case
passes on both final ELFs. Historical paid A09 is not new current-family evidence.

### Normal ELF association

- Debug `target/debug/oc` SHA256:
  `4ca06e184e393001b761d304bf6d66c6c86c791d3a54de2daeb99ed645e577b7`.
- Release `target/release/oc` SHA256:
  `5df018eb9513a5e8c4bcaf6af5467f48727d50e399918f3234d0fd1f8775a88d`.
- Source provenance is BASE + reviewed DIRTY Rust changes, not BASE-only and not a
  claimed delivery commit. Fingerprints remained identical after direct fixtures.
- Final HEAD inspection observed concurrent owner-only documentation commit
  `3efaf719f3103ec78f4729e867168b297f485558` above BASE. Its 13 committed paths are
  GOAL/test-plan/T44 planning/progress documents only, with no Rust or native fixture
  source changes. Those owner plans were preserved; normal ELF code provenance and
  the complete runtime checks above remain BASE + this reviewed DIRTY code. Final
  read-only docs/progress checks were repeated against the owner-updated HEAD.

### Closed development failures and remaining scope

- RED was the missing executor. Donor matching, shared owner and real executors
  were implemented together with selected-family publication/admission.
- Old synthetic non-GPT patch fixtures correctly failed after the selector; only
  their synthetic eligible IDs changed. A home-path preview/execution discrepancy
  was found by the real ELF case and fixed by executing the admitted normalized call.
- Existing strict provider-input tests exposed an extra empty-family developer item;
  guidance is now absent when no mutation tool is admitted. Original S07 count and
  all assertions remain unchanged. One AUD12 two-second signal timeout and one T55
  five-second PTY-exit timeout passed unchanged on rerun; final workspace and all
  final direct suites pass. No deadline or baseline was weakened.
- Final resource review removed duplicate normalized-match vectors and bounded the
  search line index with the existing cap; meaningful owner regression, strict
  Clippy, full workspace and both normal builds were repeated afterwards.
- NEXT remains full busy picker draft/commit, same-task per-request live switch,
  compatible retained cross-model tool groups and per-request actual attribution.
  Existing cross-model public-only projection is preserved by this atomic slice;
  the raw call/result graph remains immutable. Next-turn/restart family checks do
  not qualify the mandatory same-task switch. Shell inventory/conversion extension
  and remaining T50/GOAL obligations are not completed here. T44 remains PAUSED;
  paired VIS35/36 geometry is not claimed.

All owned subprocess groups and HTTP threads were joined before exact temporary
directory removal. Final process inspection found no owned Cargo/test/native-peer
process remaining; pre-existing foreign processes were untouched. Temporary sole
mutation/Cargo/fixture coordination is released to the parent for independent
review and delivery. No whole-T50 or whole-GOAL success claim.
