# T45 R10 — shared initial/nested instruction lifecycle

## Coordinator qualification — 2026-10-01

Reviewed the initial/nested source, pinned-root, permission, chronological journal,
CAS/outcome, DCP, compaction and fork seams against `750c705945860112aab1133cf0e700ceac37d177`.
The concrete Ask-source bypass was reproduced and repaired before delivery;
FIFO was already nonblocking and an outside-root read cannot reach the production
hook. Neither disproved hypothesis prompted an unrelated production workaround.

Independent current checks exited 0: eight `prm01_` owner/runtime/storage tests,
normal `cargo build --locked`, six direct-debug native Ask cases, workspace fmt,
Python 47 tests, documentation/progress structure and diff checks. The native
cases prove zero source opens for Ask in absolute/relative forms while approved
original-file reads still finish. Current full-workspace log independently
reviewed: `t45-r10-workspace-ask-review-20261001.log`, 42 successful summaries,
1372 passed / 0 failed / 10 unchanged opt-in ignored. Final normal debug remains
`0805c0bf45c10e6143a45dfffee24ee0e68c14fb292d8bfd956aec8b1a5a6287`;
release remains `5a0842133cadb545fa0b93256ccf0ed595b3babd70deee7867a7c8b36fcf6ef7`.
The earlier 1368 phase below is historical, not qualification of the Ask fix.

This delivers only the shared read prerequisite. T45 is unfinished; T50/R5 must
still implement numbered text, directories, images and safe read reopening.
T44 remains PAUSED and the exhausted actual campaign is untouched.

Frozen before implementation and RED reproduction. Source base: clean tracked
`750c705945860112aab1133cf0e700ceac37d177`; inherited untracked `.opencode/`
is outside this work. T45 active; T44 remains paused.

**Current result: the bounded atomic INITIAL+nested prerequisite, including the
parent-reviewed automatic-source Ask correction, is qualified.**
The original pre-RED freeze below is retained; current results and source
association follow it. This does not finish T45 or all of R10.

| Obligation | Result | Checks | Risks | Next |
| --- | --- | --- | --- | --- |
| Initial global/project canonical ordered baseline durably associated with session/config generation | PENDING | Typed source/path/digest/order and captured Developer input | Reopen must not replace historical baseline | Use existing turn/storage owner |
| Successful admitted read discovers nested instructions once | PENDING | Real file read, repeat unchanged, future directory hook | Deny/cancel/failed read must perform no instruction IO; descriptor-safe discovery | Shared runtime hook accepting admitted file/scope directory |
| Source edit/removal appends chronological update; old history immutable | PENDING | Old rows unchanged; latest wire excludes stale source | No second archive or IO inside transaction | Atomic existing turn commit |
| Restart/compaction/Revert/fork/Location reconciliation | PENDING | Saved context plus latest applicable global/local rules; no tool replay | Stale resurrection, duplicate fragments, lost root rules | Same root/child assembler and pinned generation |
| Security and resource invariants | PENDING | Files deny/dataRoot/nofollow, malicious symlink, stale revision rollback | Instructions never grant permissions; bounded source payload | Existing admission and storage owners |
| Actual native proof and Rust gates | PENDING | Normal debug/release ELF, targeted and workspace locked checks | Default text read remains known R5 RED (50 lines) | Preserve old native_read reproduction |

Required source: approved R10 lines 116–122 and change envelope/C6 in
`docs/goals/2026-09-21-config-compat-and-subagents.md`; pinned donor
`2670273ff17da96f85c5826ced57aa1b368754fa` instruction discovery/session
instructions; existing PRM01, DCP projection and conversation-only Revert
contracts. This is the atomic INITIAL+nested lifecycle prerequisite for T50/R5,
not all of R10, R6/R7, T45 or DCP10–12. Base/custom prompts, skills/profile/host
layers and full child/DCP work remain pending. No live/paid/user-environment
qualification is authorized here.

## RED and durable-owner decision

`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 python3 evidence/T50/native_read.py target/debug/oc`
returned 1: text default returned 50 lines; nested instruction count was zero.
ELF SHA256 `3fbf74fbdb651252d9b3b322f57f4c02c47fec0871dfca56700f9d6ba785abaa`.
Both owned fake peers/processes joined and temporary trees removed.

Representation chosen before implementation: existing immutable `events` carry
typed instruction source facts (canonical path, root/origin, digest, generation,
body or removal), once. Existing bounded `prefs` carry only current event
references/revision; additive `TurnLog` references record chronological positions.
The existing checkpoint/tool-outcome transactions commit facts, references and
wire positions together. No schema migration, second archive or source body copy
is needed. Ambient current source state intentionally does not enter the saved
DCP/Revert context: historical context is restored exactly, then current
instructions are reconciled by typed references. Fork adopts current references.
Source IO happens before transactions under pinned admitted descriptors.

## Previous lease Result / Checks / Risks / Next (historical 1368-pass phase)

| Obligation | Result | Checks | Risks / boundary | Next |
| --- | --- | --- | --- | --- |
| Initial global/project canonical ordered baseline durably associated with session/config generation | PASS, scoped | Normal debug/release exact Developer sources, canonical paths, origins, SHA256/body equality, event/revision/generation; CFG10 failed-reload and R4a repair tests | Initial sources are the existing trusted config-owner snapshot, not a model read grant; failed reload must retain that admitted generation | Keep this owner when adding remaining prompt layers |
| Successful admitted read discovers nested instructions once | PASS | Original real-ELF two-request `nested_lifecycle`; unchanged reread adds no fact; direct future directory-hook fixture | Hook runs only after successful admitted read, using permanent invocation policy and pinned Location descriptors | T50 text/directory/media success calls `instructions::after_read(..., scope_dir, ...)` |
| Source edit/removal appends chronological update; old history immutable | PASS | Fifteen-request ELF fixture: nested body after actual call/result, unchanged once, changed/removal facts append, prior completed turn JSON and old source body unchanged, next wire excludes stale body | Current projection uses typed event references, never parses AGENTS bodies or model text to decide retention | Preserve event/position transaction when extending read |
| Restart/compaction/Revert/fork/Location reconciliation | PASS, scoped | Native restart and A/B/C Location global retention; root/child own profile/sources; real DCP + actual summarizer request + saved Revert/Redo/fork/reopen test with one old source update | Location proof is ordinary target-scoped switching, not pending same-session move; saved DCP context restores exactly while ambient current sources reconcile; no automatic tool/RPC replay | Future admitted move uses target roots/snapshot and the same reconciliation |
| Security and resource invariants | PASS, scoped | Future-directory file/dir parity, cancel, dataRoot and descriptor guards; effective Deny including missing read authority blocks live refresh before opening; actual native failed read and source Deny; inotify observed **zero** opens of denied/external AGENTS; stale-revision test preserves journal/facts/started operation | Native discovery stops at admitted Location; initial in-root aliases retain canonical config admission, outside-root aliases are refused; nested components are no-follow. Existing 64-KiB/file, 256-KiB/source-body total and request/turn/RAM/SUB/effect safeguards remain | R5 still owns current file-read reopening race and read semantics |
| Actual native proof and Rust gates | PASS for prerequisite; R5 text RED retained | Four `prm01_` scenarios, full locked workspace, strict clippy/fmt, separate normal builds/help; both native instruction/read guards and all fourteen strong question cases | No full native_read PASS: default text is still 50 lines on both normal ELFs | Return shared hook to T50/R5; T45 R3/R6–9, remaining R10 and DCP11/12 stay pending |

## Owners and representation

- `composition.rs`: one admitted initial read per canonical global/Location
  source, reused for legacy diagnostics and an immutable typed baseline in the
  same config generation; pinned descriptors survive in `instructions::Root`.
  Root rules remain available with a read-denied profile, without granting model
  file access. A failed config reload cannot inject a later disk body.
- `instructions.rs`, `runtime/instructions.rs`, `runtime/{turn,context}.rs`:
  one root/child source assembler and successful-read hook. Named labels are JSON
  escaped; raw AGENTS body is intentional instruction data. A Source carries
  canonical path/root/origin/digest/generation; a Fact adds event/revision/change;
  a TurnLog Reference carries only event/index. Unchanged sources keep their
  identity, updates/removals append, latest-only wire filters stale facts.
- `storage_instructions.rs`, `tools.rs`, `storage_dcp_view.rs`, `storage_fork.rs`:
  facts use existing immutable events; bounded prefs contain current references
  and revision; current journal and successful read outcome commit atomically
  with source updates. Stale revision rolls all updates back. The existing active
  SQL projection rebases positions; fork adopts newest source references even
  at an older message cutoff. No migration, second body archive, extra loader,
  generic registry, crate/dependency/toolchain/lock change or IO-held transaction.
- `instructions/tests.rs`, `storage_instructions_tests.rs`,
  `runtime/instructions/tests.rs` under the existing compaction fake owner:
  four substantial scenarios; `docs/CODE_MAP.md` routes these owners/tests.
- Native proof: `evidence/T45/native_instructions.py`.
  `evidence/T50/native_read.py` is untouched. The only other fixture edit is
  `native_question.py` process/PTY ownership repair; all fourteen original cases
  and strong DB/wire/effect/restart assertions remain.

## Previous lease commands and source association (superseded by Ask review below)

Source: `750c705945860112aab1133cf0e700ceac37d177` **+ reviewed dirty source
tree**, not a committed-code claim. No staging/commit/push, progress/GOAL/spec/
acceptance or historical evidence edits. Inherited `.opencode/` was untouched.

Rust environment for every Cargo check/build:
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 CARGO_NET_OFFLINE=true
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
One Cargo at a time; each required gate had its own 900000-ms allowance.

| Command | Actual result | Log under `/home/opencode/.cache/opencode-tmp/opencode/` |
| --- | --- | --- |
| `cargo fmt --all -- --check` and `git diff --check` | exit 0, final | Terminal; repeated after fixture-only repair |
| `cargo test -p oc-adapters --lib --locked prm01_` | exit 0; 4 passed | Terminal and full workspace log |
| Nearest cfg09/cfg10/V07a source, read-policy, busy-notice/fork, R4a and S07 regressions | exit 0 after owner fixes | Terminal and full workspace log |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 | `t45-r10-clippy-final-review-20261001.log` |
| `cargo test --workspace --locked --no-run` | exit 0, separate precompile | `t45-r10-precompile-final-review-20261001.log` |
| `cargo test --workspace --locked --no-fail-fast` | exit 0; **1368 passed, 0 failed, 10 pre-existing opt-in ignored** | **`t45-r10-workspace-final-review-20261001.log`** |
| `cargo build --locked`; `target/debug/oc --help` | both exit 0; normal debug | `t45-r10-debug-build-final-review-20261001.log` |
| `cargo build --release --locked`; `target/release/oc --help` | both exit 0; normal release | `t45-r10-release-build-final-review-20261001.log` |
| `python3 evidence/T45/native_instructions.py target/{debug,release}/oc` | both exit 0; 15 main requests, 13 facts, exact typed Developer bodies/order/provenance, root/child/restart/location/immutable-history guards, zero denied/external source opens, joined | Terminal normalized summaries |
| `python3 evidence/T50/native_read.py target/{debug,release}/oc --case nested_lifecycle` | both exit 0; real first-two-line read and one nested source | Terminal normalized summaries |
| `python3 evidence/T50/native_read.py target/{debug,release}/oc --case text_default` | both **exit 1, EXPECTED R5 RED**, `returned_fixture_lines=50`; joined | Terminal normalized summaries |
| `python3 -u evidence/T50/native_question.py target/{debug,release}/oc` | both exit 0; all 14 strong cases PASS; hash unchanged before/after; joined | `t45-r10-question-{debug,release}-final-review-20261001.log` |

Native commands use the same approved TMPDIR and isolated synthetic HOME/config/
project/data, loopback fake providers only. No live, paid, actual HOME/auth,
runner credentials, strace or real campaign was used.

Normal ELF SHA256 after builds, instruction guards and all question cases:

- debug: `67744529da79c1ac90f025e2e15fba6e14ca86e3a8da3d8765812e9568fecfaa`
- release: `6a32d61e32278afe1d1ac50ee99dd68eec78ba26940e5f9f4ac2d0ec50063d67`

Latest full gate includes adapters 395, runtime integration 109, binary units 85,
core 32 and TUI 429. The required adjacent question selected-profile/pending/
read-guard paths, MCP/media, cold provider/profile, fork and compaction suites
are in that same green run. Specific adjacent owners include
`question_real_core_binding_commit_and_restart_no_reask`,
`primary_selection_and_restore_narrow_dispatch_guidance_and_children`,
`tool13_fork_rebases_busy_notice_reference_once_without_copying_job`, UI07 packs,
and `compaction_real_summary_wire_usage_tail_dcp_undo_redo_reopen_fork`.

## Failed approaches and cleanup

The first full workspace log (`t45-r10-workspace-20261001.log`) is a failed
prototype, not PASS: applying tool authority to initial config sources removed
root rules; a full history refresh after read duplicated a current shell notice;
the resource-policy hook also evaluated an absolute path with relative-only tool
rules. Fixed at the source/policy/history owners, without changing assertions.
Initial bodies now come from the admitted immutable config snapshot, while live
nested fetches enforce effective permanent read authority and explicit Deny in
both resolved and Location forms.
Typed-source-only history filtering retains the existing notice window.

`t45-r10-workspace-final-20261001.log` is an **interrupted combined
precompile/test attempt**, not PASS. It still exposed failed-reload generation
mixing and the initial read-denied-profile baseline, plus one transient S07
cursor mismatch. The owner fixes and targeted S07 rerun passed; the separate
qualified precompile/full gate above completed with no failures. No failing
baseline, golden, ignore or test-limit relaxation was used.

Final source review additionally caught missing read authority permitting a live
refresh of a previously admitted nested source when only explicit deny rules
were checked. `RuntimePolicy::instruction_path_denied` now also checks effective
read Deny using the existing normalized permission path. The directory/source
scenario proves missing authority blocks before opening. Initial config snapshots
remain a separate admitted authority and retain their baseline. Nearest tests,
strict clippy, separate precompile/full workspace, both normal builds/help and
all post-build native proofs were rerun; **final-review** logs and hashes above
are the current evidence, superseding earlier qualified source artifacts.

Question fixture attempts initially timed out/ended before the PTY cases despite
completed native DB results. Parent slave reopening could acquire a controlling
terminal and receive SIGHUP. Threaded preexec attachment was replaced by a fresh
fixture interpreter after `setsid`, explicit TIOCSCTTY/exec of the unchanged ELF,
parent O_NOCTTY reopening, bounded draining and owned process-group kill/wait.
The final debug/release logs each contain all fourteen PASS cases. The earlier
616-byte question debug log is incomplete and is not qualification.

All new fake peers/processes were joined/reaped before return. Seven interrupted
question trees and nine legacy test trees were identified by post-freeze birth
times and removed exactly; the latest gate added three more owned legacy trees
`oc-tui-workspace-{primary,stale,vanished}-2491094`, likewise removed after
reaping (19 total); inherited question trees and unrelated
day-old processes were left alone. Other new temporary trees were context-owned
and removed on completion. No broad prune or Cargo-tree copies were retained.
The nineteen retained owned logs total **493852 bytes**; the largest is 130000
bytes, well under 16 MiB/file and the small-artifact envelope. A harness-managed
truncated metadata tool output was left under harness ownership, not edited.

## Remaining / next

The next mutation owner is T50/R5: use the existing successful-read hook for
text and future scope-directory/media results; do not add another AGENTS loader.
Current text default 50, media/directory behavior and Files reopening race remain
R5 work. This prerequisite does not qualify base/custom/skill/profile/host prompt
layers, full T45 R3/R6–9, DCP11/12, same-session move, actual-user startup or real
UI parity. T45 stays active, T44 stays PAUSED; no task finish/status change here.

## Parent review correction — Ask sources / FIFO / outside-root (reopened)

The prior 1368-pass phase above is historical for this renewed lease. Parent
found that live nested AGENTS with effective Ask could open without approval for
that source. Frozen focused obligation: original-file Allow + ancestor-source Ask
in both absolute and Location forms must observe zero IN_OPEN events and no body;
an independently approved Ask original file must still complete, and an ancestor
with permanent Allow may be admitted. No second approval framework: unadmitted
automatic sources defer through existing absence/removal reconciliation.

The FIFO claim is falsifiable: `admitted_fs::open_resolved` already ORs
O_NONBLOCK before openat2; a bounded owned FIFO test must return at the nonregular
metadata check. Outside-root requires a production caller before any change;
current Files resolution and the Runtime-owned root relationship were checked,
not broadened speculatively. Latest correction checks follow below.

### Focused RED → GREEN and classified behavior

- RED `cargo test -p oc-adapters --lib --locked
  prm01_automatic_ask_sources_are_not_opened_in_either_resource_form` exited 101:
  relative and absolute source Ask each opened the watched AGENTS and returned
  its body. The old normal debug ELF (`67744529...`) also failed the new
  `native_instructions.py --ask-only`: unapproved source body reached Developer
  input and IN_OPEN was true. The failed fake attempt was bounded and joined.
- `permissions.rs::automatic_source_allowed` and
  `RuntimePolicy::automatic_instruction_source_allowed` now require permanent
  Allow for automatic source IO, respecting ordered rules and authority
  intersections in resolved/Location source forms. Existing effective/explicit
  Deny ceilings remain; no Once/Always permit for a different original file is
  consumed by this source check. `read_source` returns the existing absent
  source on Ask/Deny/missing authority before opening. Previously admitted
  content then uses ordinary append/removal reconciliation.
- The original successful-file structural/Deny guard remains separate. Approved
  original-file Ask does not abort a completed read or leave its operation
  started. The real runtime scenario covers Once/Always × source Ask/Allow:
  only the original file requests approval; reads and turns complete; source
  Allow admits a fragment while source Ask does not. Initial root baselines
  remain admitted config snapshots, independent of model read authority.
- This is an explicit safe native difference: automatic Ask instructions defer
  with no extra dialog/framework, rather than silently approving another path.
  The current native six-case relative/absolute matrix verifies original-file
  Allow/source Ask, original-file Ask/source Allow, and Ask/Ask with inotify and
  exact source path/origin/digest/Developer plus completed DB outcome.

### FIFO and outside-root findings

`admitted_fs.rs::open_resolved` already adds O_NONBLOCK before openat2. The new
`prm01_nested_fifo_returns_before_regular_file_check_deadline` fixture returned
the nonregular error within its one-second bound. Its regression path opens
only the owned FIFO writer to release/join a blocked worker before failing.
No production opener change was necessary; cancellation/byte caps are unchanged.

The outside-root finding is **false/unreachable for the actual production
caller**: `application.rs::build_runtime` constructs Files and ToolRoots with
the same `composition.project` used by admitted project instruction roots.
`Files::resolve` rejects absolute/root-prefixed paths, lexical escapes, symlinks
and dataRoot before success. Thus an admitted outside-Location file does not
reach this hook. A manually mismatched library construction is not production
evidence; no unrelated source-path behavior was changed.

### Current Ask-review gates and normal artifacts

HEAD remains `750c705945860112aab1133cf0e700ceac37d177` **+ reviewed dirty tree**.
The current Rust changes for this reopened lease are the narrow permission/source
gate and four substantial scenarios; dirty lifecycle/history/DTO work is retained.
No public API/dependency/migration/progress/GOAL/spec/commit/stage change.

All Cargo commands used jobs=3, test threads=1, offline=true and the same approved
TMPDIR. Separate strict-clippy, precompile and full-test calls each had their own
900000-ms allowance; one Cargo at a time. Latest logs are under
`/home/opencode/.cache/opencode-tmp/opencode/`:

| Command/check | Current actual result | Current log/artifact |
| --- | --- | --- |
| fmt/diff checks | exit 0 | Terminal final review |
| `cargo test -p oc-adapters --lib --locked prm01_` | exit 0; **8 passed** | Terminal / current workspace log |
| Runtime `resource_permissions_gate_real_dispatch` | exit 0; 1 passed | Terminal / current workspace log |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 | `t45-r10-clippy-ask-review-20261001.log` |
| `cargo test --workspace --locked --no-run` | exit 0; separate precompile | `t45-r10-precompile-ask-review-20261001.log` |
| `cargo test --workspace --locked --no-fail-fast` | exit 0; **1372 passed, 0 failed, 10 unchanged opt-in ignored** | **`t45-r10-workspace-ask-review-20261001.log`** |
| Normal `cargo build --locked` + debug help | both exit 0 | `t45-r10-debug-build-ask-review-20261001.log` |
| Normal `cargo build --release --locked` + release help | both exit 0 | `t45-r10-release-build-ask-review-20261001.log` |
| Native instruction lifecycle, both normal ELFs | exit 0 each; original 15-request/13-fact proof intact | `evidence/T45/native_instructions.py` |
| Native `--ask-only`, both normal ELFs | exit 0 each; all six cases PASS; source Ask **zero opens/no body**; permanent Allow opens/admitted; original Ask completed | Same fixture / terminal normalized summaries |
| Original nested-read guard, both normal ELFs | exit 0 each; 2 requests, actual first-two-line read | Untouched `native_read.py --case nested_lifecycle` |
| Strong native question, both normal ELFs | exit 0 each; **14/14 PASS**, before/after hash unchanged | `t45-r10-question-{debug,release}-ask-review-20261001.log` |
| Original text default, both normal ELFs | **exit 1 EXPECTED R5 RED**, 50 fixture lines; joined | Untouched `native_read.py --case text_default` |

Current full gate: adapters 399, runtime integration 109, binary units 85,
core 32, TUI 429; pty_t39 48 and pty_t42 34 all passed. All 42 result lines are
green, including doc tests. Prior 1368 logs/hashes above are historical and do
not qualify the new Ask correction.

Current normal SHA256 after all required native checks:

- debug: `0805c0bf45c10e6143a45dfffee24ee0e68c14fb292d8bfd956aec8b1a5a6287`
- release: `5a0842133cadb545fa0b93256ccf0ed595b3babd70deee7867a7c8b36fcf6ef7`

All new fake threads/groups were joined/reaped; process metadata showed no own
Cargo/Python/native-test children. Three new legacy workspace trees
`oc-tui-workspace-{primary,stale,vanished}-2556560` were identified by birth time
05:32:22 within the current gate window 05:23:40–05:37:43 and removed exactly
after reaping, bringing total owned interrupted/legacy tree cleanup to 22.
Inherited trees/processes were untouched. All native fixture contexts removed
their own temps. No Cargo-tree copies were retained.

This lease retained seven tiny logs (132665 bytes); all 26 own logs total
**626517 bytes**, maximum 130000 bytes. No live/auth/actual HOME/paid/strace or
real campaign was used, and inherited `.opencode/` was not inspected.

T50/R5 next uses the same `after_read(..., scope_dir)` caller seam. Default50,
directories/media/reopening race remain pending. This correction does not finish
T45 R3/R6–9/rest-R10/DCP11/12 or resume T44; task states remain unchanged.
