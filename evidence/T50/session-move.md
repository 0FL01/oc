# T50 R8 / TOOL19 — frozen move obligations

## Coordinator qualification — current reviewed source

The parent independently reviewed the application handoff, admission/fingerprint,
storage terminal predicate/transaction/recovery, history and frontend consumers,
and the actual-native fixture. Two scoped read-only reviews found no concrete
introduced violation. Their source conclusions do not substitute for native proof.

Current source association: `ce1e57a9bd482cdc9e2380f06d6a66db83612029` plus
the reviewed R8 dirty diff. Commits since R7's `0dc85e81778be08caa2afb2972431febeb12d85e`
change owner planning/docs only: Rust/Cargo comparison is empty. Those changes are
preserved; the compiled artifacts are not described as clean-HEAD binaries.

Parent commands, all exit0: `cargo test -p oc-adapters --lib --locked tool19_`
**5 PASS**; `cargo test -p oc-adapters --test runtime --locked tool19_` **2 PASS**;
workspace fmt check, strict locked all-target workspace Clippy, normal locked build.
After that final build, without further Cargo, both normal ELFs passed the actual
`boundary`, `quarantine`, `crash_terminal`, `children`, `explicit_tab` scenarios:
**10 directed native PASS**. Per ELF, 21 main requests, five admitted/applied moves,
25 tool rows and ten turns. Quarantine's one uncertain remote effect was not replayed;
the destination issued zero provider requests. The actual HTTP title race remains
visible (debug quarantine one POST / release two), not normalized into equality.
All owned groups/workers joined; no fixture data or original user configuration used.

Normal SHA256 matched before and after all direct scenarios:
- debug `8ba98cdfd598fa9873569e51761d90bdca70d4a53ff1cac709231f1612bbc166`
- release `259e659efd329f28f9630181586e08800ada144c7af5bfd00cac7e1c7c062f30`

The unchanged production diff also has the complete **1399 PASS / 0 FAIL / 10
unchanged opt-in ignored** workspace gate and both 35-case campaigns recorded below
in `t50-r8-workspace-qualified.log`. Parent Python **47 PASS**, docs/progress and
diff checks exited0. T50 remains incomplete for new R2 inventory/conversion and
R1/R9 model-dependent/live-switch work; no T44, live or product readiness claim.

Frozen before RED, base `0dc85e81778be08caa2afb2972431febeb12d85e`.
Concurrent owner planning/index/progress changes are excluded; `.opencode/`
is inherited and never inspected. T50 remains active, T44 paused. R2 inventory /
foreground conversion and T45 full profile/DCP work are later slices.

## Required observable obligations / falsifiers

| Obligation | Required observation / falsifier |
|---|---|
| Direct tool | Strict directory/sessionID schema; exact provider callID and durable operation; no CodeMode. |
| Admission | Target locally known in original Location, caller effective policy, child self-only ceiling; relative/~ use targeted context and product-admitted HOME. Invalid/foreign/stale/Ask-cancel/busy/failed destination generation have no move intent, placement, grant or generation effects. |
| Immutable turn | Pending response while source request is live; held later request and same-batch mutation remain source cwd/instructions/policy. No destination provider request until a distinct subsequent admitted turn. |
| Atomic placement | One durable identity, pending/applied distinction; same ID/history/DCP retained. Binding and applied event commit together only after source turn's full durable terminal boundary. Rollback leaves original placement. |
| Fresh destination | Existing complete composition/runtime admission, new selections/instructions/environment/skills/tools/MCP; no invalid opaque continuation. Ordinary UI scoped switch remains distinct. Real reopen/history/UI placement agrees with DB. |
| Provenance | Background process completes original cwd after parent move; notices address sourceSession. Children keep original Location/turn/job/pack authority, follow-up cannot silently transplant them. |
| Quarantine / cleanup | Existing external_directory/no-follow/data-root protections; no grant created by move. Sticky remote-unknown captured after owned stop/join, destination inherits it; cleanup failure is fatal, not success. |
| Recovery | Before-intent crash has no move. Accepted pending applies only at a proven terminal boundary; unknown interrupted source is not replayed. Applied identity deduplicates across restart; no shell/MCP/child replay. |

## Minimal change envelope and authority

Existing four crates, no dependencies/toolchain/schema replacement/second store,
queue/catalog/history/runtime authority. Reuse composition admission, application
supervisor replacement, runtime common tool permission pipeline, storage transaction,
shared instructions and frontend catalog/Location consumers. A private prepared
destination handoff avoids application Inbox self-await during an active turn.

### Falsifiable schema necessity (before schema edit)

Current `prefs[tui.session_location.ID]` stores only placement. Current tool result
is immutable and may honestly report pending; overwriting it as applied would
rewrite raw turn history. Neither can answer whether this exact operation was
admitted, is still awaiting its source terminal boundary, or atomically applied.
Add one additive existing-Db table, keyed by the existing tool operation identity,
with session/source turn/source Location/destination and pending/applied phase.
One pending record per target, foreign keys to existing owners, bounded reads.
No persisted credentials/Composition payload and no new effect replay engine.
If these facts can instead be represented atomically by existing storage without
losing immutable result/dedup/terminal proof, omit the migration.

## Qualification protocol

Schema verification envelope: the two exact existing migration-sequence assertions
include the justified version6 while preserving sentinel/one-time checks. This
is an exact new-schema expectation, not an ignored failure or a relaxed acceptance
baseline; a spurious/missing migration still fails.

Nearest RED/GREEN then affected owner tests; workspace fmt, strict locked
all-target Clippy, full locked workspace tests --no-fail-fast, normal debug/release
build/help, Python47 and read-only docs/progress/advisory/diff checks. After last
Cargo, direct both unchanged normal ELFs through fake Responses/process barriers
and actual PTY/DB/files. Capture hashes/counts/state, joined owned PID groups and
HTTP threads, remove exact owned TempDirs. No paid/live/env/private config access.
Own logs bounded <16MiB each and total new retained evidence near <1MiB.

## Actual results

At freeze these checks were NOT_RUN. The following records the completed R8
qualification; it is not a whole-T50 completion report.

### Git association and ownership

Implementation base: delivered R7 commit
`0dc85e81778be08caa2afb2972431febeb12d85e`. Current HEAD at qualification/handoff:
`760d54f7dcc86f1f14af8dfef0e9fc1dd59d78e5`. Intervening owner commits were
documentation/planning only: the base-to-HEAD diff for `crates`, `Cargo.toml` and
`Cargo.lock` is empty. These are normal binaries built from HEAD **plus the
reviewed R8 working-tree changes**, not clean-HEAD artifacts.

Concurrent owner edits to AGENTS/GOAL, contracts, DCP/decisions, test/tool/spec
documents, planning, roadmaps and T44 recovery documents are preserved and excluded
from this coordinator's changes. The inherited `.opencode/` was never manually
inspected, edited or staged. No staging, commit, push or execution-state/progress
mutation was performed.

### Frozen outcomes

All log names below live under
`/home/opencode/.cache/opencode-tmp/opencode/` with prefix `t50-r8-`.
Native cases ran on **both** normal debug and release ELFs.

| Frozen outcome | Result | Evidence / owning source |
|---|---|---|
| Direct tool | PASS | Strict model-independent `directory`/optional `sessionID` schema, root/child effective catalogs, exact wire callID correlated with durable operation/result; `models/tool_tests.rs`, `tools.rs`, `runtime.rs`, `native-{debug,release}-qualified.log`. |
| Admission | PASS | Relative and admitted-HOME resolution; exact session+canonical-directory Allow/Ask/Deny, no saved move grant; missing/foreign/child-parent/child-sibling/busy-child/stale-Ask/cancel and invalid model/variant/agent/credential/policy/generation all refuse before durable move admission. `application/session_move.rs`, `runtime/turn.rs`, `approval.rs`, `tests/runtime/session_move.rs`; native cases `relative`, `home`, `deny`, `ask_*`, `bad_*`, structural refusal cases. |
| Immutable source turn | PASS | Barriers hold the source after pending result and again before full terminal response: placement remains A, same-batch and later source shell run in A with original instructions/opaque source state. Only a distinct subsequent turn sends B requests. Native `boundary`, actual files, request captures and turn/event ordering; `runtime/turn.rs`, `application.rs`. |
| Atomic placement | PASS | One operation identity reports pending; durable full source-terminal event precedes applied placement event. Transaction rollback preserves A; same session/history and child placement retained; duplicate application produces no second event. `storage_session_move.rs`, `storage/session_move_tests.rs`, native `rollback`, `boundary`, recovery cases. |
| Fresh destination / frontend | PASS | B uses different provider binding, model/variant, instructions, cwd and policy; old opaque/functions are excluded from its fresh projection. Real same-ID CLI reopen and PTY footer agree with DB. Explicit parked-root move keeps caller A, removes only moved tab and retains caller draft. `application/session_move.rs`, `runtime/context.rs`, `core_app.rs`, `tui_cmd.rs`, `app/live.rs`; native `boundary`, `explicit`, `explicit_tab`, restart cases. |
| Provenance | PASS | Source background job finishes original cwd after parent moves, one sourceSession notice, leader reaped. Children retain A; child self move and parent/sibling/foreign ceilings are real delegation tests. Directed background/Revert/Fork cases also pass. Native `boundary`, `children`, `child_move`; existing Arc job owner transferred in `application.rs`. |
| Quarantine / cleanup | PASS | Existing external-directory trust, no-follow and protected-data-root guards survive. Admitted move followed by uncertain remote MCP effect ends source with a known full failed boundary; source stop/join preserves sticky quarantine in B, blocking further requests/reinitialization/replay. One fake remote effect, one initialize/list/call. Native `quarantine`, `untrusted`, `symlink`, `protected`; application fatal paths join owned resources. |
| Recovery | PASS | Pre-intent crash: no move. Accepted pending with unknown interrupted source: remains A/pending, no automatic effects. Known-terminal apply failure: real fatal exit 1 and rollback, then restart re-admits same fingerprint and applies once without automatic provider turn. Applied restart deduplicates. Reset cascades intent rows. Native `crash_*`, storage tests. |

### Owner seams and schema necessity

The pre-edit necessity above was confirmed: placement prefs and immutable tool
results cannot encode pending/applied identity and source-terminal proof together.
Migration **6** adds only `session_moves` in the existing SQLite owner, with existing
tool-operation/source-turn/session foreign keys and cascade deletion. There is one
pending move per target, at most eight globally; destination root-tab reservations
respect the existing cap. No credentials or serialized Composition are persisted.
Two exact migration-sequence tests now include 6; their sentinel/one-time checks
remain intact.

`application/session_move.rs` is the private prepared admission seam, using existing
composition, selection, workspace, instructions and provider-readiness owners. Its
in-memory fingerprint covers admitted generation and actual bindings; only the
digest is durable. The existing application supervisor receives a private boxed
prepared handoff after the full source turn returns. It does not await its own
Inbox. Storage commits placement/event/phase atomically; a typed `SessionMoved`
event feeds existing frontend consumers. The original Arc shell-job owner is
transferred, and sticky remote uncertainty is read after owned MCP stop/join.

No dependency, Cargo manifest/lockfile, toolchain, crate, generic framework, trait,
second store/catalog/history authority, daemon or test-only public API was added.
Donor behavior was inspected at pinned
`2670273ff17da96f85c5826ced57aa1b368754fa` (`tool/plugin/opencode.ts`,
`session/move.ts`, `session/projector.ts`); existing provenance notices remain.
Native differences are explicit: full durable **turn** boundary rather than donor
step boundary, honest pending result, locally known scope, root/child ceilings,
no saved move grant and pre-authorized external-directory trust.

### Actual required checks

Recorder exits below are all **0**, except the intentional RED and diagnosed
development failures listed separately. Cargo was serialized with jobs=3,
test threads=1, offline=true, fixed owned TMPDIR and 900000ms tool timeout.

| Gate | Actual result | Log basename |
|---|---|---|
| RED, direct strict schema | 0 passed / 1 failed, exit 101 before implementation | `red.log` |
| Nearest final owner tests | 5 lib + 2 runtime passed | `owner3.log`, `runtime5.log` |
| Workspace fmt | PASS | `fmt-final.log` |
| Workspace all-target locked Clippy, `-D warnings` | PASS | `clippy-final.log` |
| Separate workspace test precompile | PASS | `precompile.log` |
| `cargo test --workspace --locked --no-fail-fast` | **1399 passed, 0 failed, 10 existing ignored**, 42 summaries, 844.06s, complete/untruncated | `workspace-qualified.log` |
| Python checks | **47 passed**: bounded_live 13, code_size 5, progress 15, check_docs 14 | `python47-final.log` |
| Read-only docs/progress; advisory size; diff check | PASS; size advisory 20 code files, largest changed owner 4434 lines (<5000) | `docs.log`, `progress-readonly.log`, `size.log`; final handoff logs below |
| Normal debug/release locked build | PASS | `build-debug-final.log`, `build-release-final.log` |
| Both actual normal ELF `--help` | PASS | `help-debug.log`, `help-release.log` |
| Direct TOOL19 native scenarios | **35 passed per ELF** | `native-debug-qualified.log`, `native-release-qualified.log` |
| Directed affected native regressions | **23 passed per ELF** | `regressions-debug-qualified.log`, `regressions-release-qualified.log` |

Full workspace log:
`/home/opencode/.cache/opencode-tmp/opencode/t50-r8-workspace-qualified.log`.
The actual recorder footer records exit 0, 1399/0/10, 42 summaries and
`truncated:false`; log size is 128466 bytes. This is not a partial/timeout PASS.

Normal ELF SHA256 (unchanged before/after all post-build campaigns and final check):

```text
debug   8ba98cdfd598fa9873569e51761d90bdca70d4a53ff1cac709231f1612bbc166
release 259e659efd329f28f9630181586e08800ada144c7af5bfd00cac7e1c7c062f30
```

Debug BuildID `d27bfddd4c8a79aae97866b94ae35f3521aa83d9`; release BuildID
`53871cc1b7b82c24bacfa0b2500baff27a0863c0`. No Cargo or production-source edit
occurred between the last normal builds and these direct checks.

### Native observations, counters and reproduction

Each ELF passed **58 scenarios** (35 move + 23 directed). Move aggregate per ELF:
98 main provider requests, 11 admitted move rows, 10 applied events, 122 tool rows,
48 turns and one fake remote effect. The remaining pending row is the intentionally
unknown-source crash, not a hidden application failure.

HTTP instrumentation covers 34 move cases, excluding the separate actual parked-tab
PTY scenario: debug 127 observed POSTs / 33 auxiliary title POSTs; release 128 / 34;
both 12 destination POSTs. Both record 128 dispatch facts. Debug's quarantined
auxiliary title dispatch was stopped before reaching the wire; dispatch is not a
retry/request count. The parked-tab case separately has four main requests,
three tool rows, two turns and one applied event; its auxiliary POSTs are not part
of those 34-case physical counters. No whole-matrix physical POST count is claimed.

Directed cases include R7 models (static/dynamic/deny/child) and real durable title,
real PTY title projection, read nested lifecycle/image, source Ask, search
root/Explore, webfetch/Ask, question/custom cancel/reopen, shared instruction Ask,
background source notices, actual Revert and actual Fork. Revert observes two jobs,
two terminal outcomes/notices/effects without replay; Fork retains one source job
and effect with real source/fork notices and zero fork-owned jobs.

Reproduce direct campaigns only after normal builds:

```text
python3 evidence/T50/native_session_move.py target/debug/oc
python3 evidence/T50/native_session_move.py target/release/oc
python3 evidence/T50/session_move_regressions.py target/debug/oc
python3 evidence/T50/session_move_regressions.py target/release/oc
```

Fixtures use loopback fake Responses/MCP, synthetic admitted configuration/HOME,
real production DB/files/processes and PTY barriers. Request frames remain in RAM;
headers/secrets/raw frame matrices are not retained. Exact callIDs, operation IDs,
durable event ordering, independent files and cursor-aware painted metadata are
asserted. Each fixture joins its owned process groups and HTTP threads and removes
only its exact owned TempDirs. No private authoring config, `.local`/live env,
paid/live network, exhausted journal or campaign was accessed.

### Failed attempts retained / diagnosis

- Initial compile/import typos and new runtime-fixture Location mismatch (`work`)
  were corrected; production scope assertions were not weakened (`owner1.log`,
  `runtime1.log` through `runtime3.log`).
- Native development corrected fixture errors: forbidding B's own new call pair;
  expecting completed instead of a valid cancelled terminal; poisoning caller
  binding instead of the explicit idle target; missing General configuration;
  wrong home composer prompt/wait (`native-boundary-dev.log`, `native-dev-all1.log`,
  `native-tabs-dev1.log`).
- Initial quarantine fixture placed an uncertain MCP call before move, correctly
  stopping later batch execution. Move-first now proves actual handoff
  (`native-dev-all3.log`, `quarantine-dev1.log`). A crash-case routing mistake had
  exercised unknown pending instead of terminal recovery; counters exposed it and
  `terminal-dev1.log` plus both final campaigns prove fatal rollback/restart once.
- Strict Clippy found oversized enum variants and ordinary idiom issues; boxed
  typed payloads and idiomatic code fixed them without lint exemptions
  (`clippy.log` through `clippy3.log`).
- Initial Python invocation from repository root used invalid `scripts.test_*`
  module paths (four import errors, no 47-test result). Running existing tests from
  `scripts` with their normal module names passes all 47 (`python47.log`,
  `python47-final.log`).

No deadline/cap, ignored-test set, golden or acceptance baseline was relaxed.

### Exact change set / remaining scope

Existing changed Rust owners/tests: adapters `application.rs`, `approval.rs`,
`composition.rs`, `defs.rs`, `models/tool_tests.rs`, `runtime.rs`,
`runtime/context.rs`, `runtime/turn.rs`, `storage.rs`, `storage/tests.rs`,
`tools.rs`, `tests/runtime.rs`, `tests/runtime/turns.rs`; core `core_app.rs`;
TUI `app/live.rs`; binary `tui_cmd.rs`.
New Rust modules: `application/session_move.rs`, `storage_session_move.rs`,
`storage/session_move_tests.rs`, `tests/runtime/session_move.rs` (all under
`crates/oc-adapters`). Current `docs/CODE_MAP.md` records these owners.
New evidence: this report, `native_session_move.py`, `session_move_regressions.py`.
Existing `native_model_session.py` and `native_search.py` add explicit current-R8
flags while preserving their default historical schema expectations.

Resource preflight was uid1003/non-root, about 5851MiB available memory and 191GiB
free disk. Before final read-only handoff checks, all owned R8 logs totaled
272154 bytes; largest 128466 bytes. New report/scripts are small; final total is
reported below. No Cargo tree/artifact copy, broad prune or unrelated PID cleanup.

Unknown interrupted source moves intentionally stay pending and are not replayed;
changed recovery admission fingerprint refuses automatic application. These are
native safety ceilings, not silent fallback. R8 has no unresolved failing gate.
Full T50 remains active: new R2 inventory/foreground conversion and R1/R9 live
model/file-tool work remain later slices; T44 remains PAUSED until owner resume.
Temporary mutation/Cargo/fake-native-fixture ownership is released at handback for
parent independent review/delivery.

Final read-only handoff checks against the concurrent owner documentation passed:
`t50-r8-docs-handoff.log` and `t50-r8-progress-handoff.log`, both exit 0 (structure
checks, not product acceptance). `git diff --check` passed, the index is empty,
base-to-current-HEAD Rust/Cargo diff remains empty, and both ELF hashes above still
match. Owned retained logs total **272971 bytes**, largest **128466 bytes**;
logs plus the three new qualification report/scripts remain below **330KiB**.
All temporary native process/HTTP owners were joined by the completed fixtures;
there is no running coordinator Cargo or native campaign to transfer.
