# T55 R3 — frozen native/offline relay atomic

## Independent coordinator qualification

Reviewed the complete parser/error/test diff, the final vacant-slot correction
and its one-socket regression, the runtime admission guards, and the 484-line
native/PTY/explicit-relay checker. Independently verified all 42 successful
summaries in the current full-workspace log below: **1345 passed, zero failed,
10 pre-existing opt-in ignored**. No source changed after that gate.

Fresh coordinator commands exited 0: provider owner suite **35**, runtime
`prov09_` **2**, workspace fmt and strict all-target Clippy, normal locked debug
build, Python **47**, docs/progress structure and diff. After that last build,
direct normal debug and release `native_completed.py` each passed all **seven**
cases: **25 / 26 actual POSTs**, exact patch/read file and graph, no sparse retries,
zero negative tool intents, unknown append once and joined owned descendants.
The offline relay separately consumed **six** durable reservations per ELF,
including title and one intentional 503 retry, with same-identity restart and
three malformed bodies refused before reservation/upstream. The two binary
digests below were identical before and after these direct checks; no Cargo was
interposed. This confirms R1–R3 and R4's enforced **offline prerequisite**, not
PROV10 live success, T27 coding, T44 parity or product READY.

Base `8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0` plus preserved R1/R2 dirty
diff, including parent's optional announced-index merge. Freeze before new fixtures.

## Result

| Required proof | Initial result |
|---|---|
| Normal debug/release, each direct headless and real PTY apply_patch → read → call/result → final; exact independent file bytes, completed SQLite tools, one execution per call and no sparse retry | NOT_RUN |
| Same-history process reopen and next accepted command preserve graph and settled effects without replay | NOT_RUN |
| Actual done call with conflicting terminal and an unclosed/EOF physical response yields zero tool intents/files; cancellation bounded | NOT_RUN |
| Crash/recovery preserves unknown operation non-success and does not reexecute its effect | NOT_RUN |
| Real TTY settings restored on clean exit; owned native/leaf/helper process groups and HTTP servers joined | NOT_RUN |
| Fresh **offline-only** trusted envelope counts all native POSTs, including title/main retry; reservation is visible before upstream, helper/native restart retains identity and monotonic journal | NOT_RUN |
| Wrong-type relay rejection precedes upstream/reservation; existing hardcaps/fsync/pre-effect envelope tests pass, no reset | NOT_RUN |
| Current workspace fmt/strict Clippy/tests, normal builds/help, Python/docs/progress read-only/code-size/diff; direct T55/T54 both ELFs after last build, hashes stable without Cargo interposed | NOT_RUN |

## Checks

Serial offline locked Cargo: jobs 3, test threads 1, approved TMPDIR
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`, timeout
900000 ms. Normal debug/release builds are separate commands. Preflight UID 1003,
available memory 7461 MiB, available disk 192380 MiB. Small direct checker planned
at `evidence/T55/native_completed.py`, reusing synthetic fixture topology and
`scripts/bounded_live.py`; no new product/network/retry owner.

## Risks

R3 is functional native proof, not T44 pixel parity. Existing pre-fix differential
remains immutable. Fake relay envelopes are fresh private test roots only. The
actual T55 diagnosis campaign and older T46 live campaign are not read or changed;
R4 external dispatch/credentials/catalog/bindings remain parent-only. Output and
reports retain fixed safe categories/counts, not raw requests or provider payloads.

## Next

Implement directed native/PTY/effect/relay checker, qualify current full gates,
build normal ELFs, then direct T55/T54 checks without intervening Cargo. Parent
independent review/commit precedes live R4; whole T55 and READY remain open.

## Current result — R3 and offline relay prerequisite

The initial NOT_RUN table is the historical pre-implementation freeze. Current
qualification is against HEAD `8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0`
plus the preserved/reviewed dirty R1/R2/R3 source and new fixture modules.
No clean-HEAD or whole-task acceptance claim is made.

| Required proof | Current observation | Result |
|---|---|---|
| Normal debug/release headless and PTY coding graph | Each ELF performs actual apply_patch → read → final, with independently expected file bytes `hello\n`, two completed tool operations, matching call/result pairs and zero sparse retries. Directed empty/full/omitted/partial snapshots are covered without multiplying all negative cases by viewport. | PASS |
| Same-history reopen | Native process exits, then a new explicit accepted command reopens the same session. Each original call appears exactly once with one result in subsequent request input; two completed turns and unchanged exact file bytes, no old tool execution. | PASS |
| Conflicting/unclosed/EOF zero effects | Conflict with observed true is terminal failed after one main POST/zero retries. Held unclosed response and genuine EOF are cancelled with exit 130 before tool admission. All have zero intents and no effect file; EOF alone has the genuine typed retry event. | PASS |
| Unknown recovery | An actual closed canonical Bash call appends `effect` once, then is interrupted while its real operation is started. Recovery marks old operation/turn unknown, next explicit command completes, and the effect remains exactly once. No SQL seeding or state rewrite. | PASS |
| TTY/process/server ownership | Real 120×40 PTY clean Ctrl+D exit restores original termios. Owned native/helper/leaf groups are checked gone after joins; scoped adopted children are reaped. Held HTTP workers wake on fixture stop; non-daemon server workers, listener and server thread join before temp cleanup. | PASS |
| Enforced offline all-lane relay | Native main retry, patch/read/final and title all traverse the trusted helper. Six upstream POSTs equal six generation reservations and six complete HTTP receipts. Reservation is visible before each upstream effect. Native/helper restart keeps the same explicit identity and monotonic journal. | PASS |
| Pre-effect rejection and unchanged envelope | Three malformed typed bodies return 403 before upstream/reservation; ledger snapshot remains exactly unchanged. Existing hardcap/reservation/fsync/campaign tests pass. Test creates only its fresh private offline identity; Relay itself never initializes or resets one. | PASS |
| Current quality/artifact gate | Full fmt/strict Clippy/workspace/Python/docs/progress read-only/code-size/diff pass; normal debug/release builds/help pass; direct T55/T54 and directed T50 checks pass after last builds, hashes unchanged with no Cargo interposed. | PASS |

### Actual T55 native cases and counts

`python3 -B evidence/T55/native_completed.py target/debug/oc` and the corresponding
`target/release/oc` command both exit 0 on the current checker and normal ELFs.
Seven cases per ELF, all synthetic loopback providers and isolated HOME/XDG/project.

| Case | Main POSTs | Title POSTs | Actual result |
|---|---:|---:|---|
| Headless sparse patch/read/final + reopen | 4 | 1 | Both commands exit 0; apply_patch/read completed; exact bytes; zero retry |
| Real PTY full/omitted patch/read/final + reopen | 4 | 1 | Clean native exit 0, termios restored; reopen exit 0; exact graph/bytes; zero retry |
| Contradictory done/terminal | 1 | 1 | Exit 1, failed; zero intents/files/retries; enum-only safe diagnostic |
| Unclosed physical response | 1 | 1 | Exit 130, cancelled; zero intents/files/retries |
| Genuine EOF without terminal | 1 | 1 | Exit 130 after typed attempt-2 notice; one retry fact, zero intents/files |
| Actual effect crash/unknown recovery | 2 | debug 1 / release 2 | Old Bash operation unknown; old turn unknown/new turn completed; one append, no replay |
| Enforced relay coding + real 503 retry + restart | 5 | 1 | Six complete receipts/reservations, one intentional HTTP retry, no sparse retry; patch/read once |
| **Total** | **18** | **debug 7 / release 8** | **debug 25 / release 26 actual POSTs** |

The unknown-case release title count is observed rather than assumed: an initial
title may have reached the server without being durably committed before crash;
the next **explicit accepted command** can issue its own one-call title. The old
tool remains unknown and is not replayed. All selected-model/path/synthetic-header
checks apply to every recorded POST; there is no model fallback.

Headless positive uses normalized added/delta/arguments-done/item-done followed
by empty completed output. Its read step recovers a genuinely completed terminal
call after an omitted observed reasoning item. PTY directs full patch output and
omitted read output. Following requests each have exactly one original
function_call and one matching function_call_output for both calls.

### Enforced relay prerequisite for parent R4

The checker imports the existing `scripts/bounded_live.py` Ledger/manifest owner,
with a fresh **offline-only fixture** campaign under its private temporary root.
`native_completed.py::Relay` requires an explicitly existing campaign and RAM
manifest; it never calls init/reset. Helper stdin ownership, native/helper process
groups, ready metadata identity and close/join are enforced. The reusable caller
can be imported by the parent with its explicit binding/manifest; the checker
main uses only the synthetic fixture and does not load credentials/environment.

- Malformed `max_output_tokens` boolean, `stream` string and `tools` object:
  three 403 rejections, zero upstream requests, unchanged zero-count ledger.
- Actual native workflow: first main HTTP 503 with observed true, one runtime
  retry, sparse patch/read/final, then same-history next explicit command after
  helper restart. Actual main 5 + title 1 = **generation 6**, control 0, MCP 0.
- Upstream verifies durable reservation visibility before responding. Every
  finish receipt is complete: one 503, five 200. Safe providerToolNames observed
  are `apply_patch`, `read`, `shell`; no wire tool renaming.
- Restart uses the same campaign identity and the existing reserved state;
  no initialize/reset call is made. Existing caller hardcaps/fsync tests pass.
- No actual live campaign ledger/nonce/config/key was inspected or changed.
  This is the enforced offline prerequisite, not R4 real-API proof.

### Current commands, full log and artifacts

All Cargo commands use jobs 3, test threads 1, offline networking, approved TMPDIR,
locked dependencies and unchanged 900000 ms command timeout, serial execution.

| Command | Exit | Current observation |
|---|---:|---|
| `cargo test -p oc-adapters --lib --locked provider::` | 0 | 35 passed, 347 filtered; original provider guards plus nine reconciliation tests |
| `cargo test -p oc --test recovery_v02 --locked` | 0 | One unchanged integration test passed, none filtered |
| `cargo fmt --all -- --check` | 0 | Final Rust source formatted |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | Final strict all-targets check |
| `cargo test --workspace --locked --no-run` | 0 | Separate precompile before the full timed gate; timeout unchanged |
| `cargo test --workspace --locked --no-fail-fast` | 0 | **1345 passed, 10 pre-existing ignored, zero failures; 42 successful summaries** |
| `python3 -B -m unittest discover -s scripts -p 'test_*.py'` | 0 | 47 passed, four existing script suites; 8.939 s |
| `python3 scripts/check_docs.py`, `python3 scripts/progress.py check` | 0 | Documentation and read-only journal structure; not acceptance status mutation |
| `python3 scripts/code_size.py --base 8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0 --changed`, `git diff --check` | 0 | Seven Rust files, 10899 physical lines/404006 UTF-8 bytes; no changed file over soft 5k guide |
| `cargo build --locked` | 0 | Normal debug build, separate command, 4.21 s |
| `cargo build --release --locked` | 0 | Normal release build, separate command, 1 m 53 s |
| Direct debug/release `--help` | 0 | Both normal binaries |
| Direct `native_completed.py` debug/release | 0 | Seven cases each, counts above, after final checker cleanup strengthening |
| Direct `evidence/T54/native_runtime.py` debug/release | 0 | Nine cases each on current normal ELFs |
| Direct `evidence/T50/native_foreground.py` debug/release `--background-supported` | 0 | 21 cases each |
| Direct `evidence/T50/native_background.py` debug/release `--review-only` | 0 | Three directed cases each: Revert, Fork, joined-capacity reuse |

**Current full workspace log:**
`/home/opencode/.local/share/opencode/tool-output/tool_0f20716a8001f6glLtRiVc7R4Q`.
Managed tool ID `tool_0f20716a8001f6glLtRiVc7R4Q`; 39 test targets plus three empty
doc-test targets. Adapter unit 382; runtime integration 107; TUI unit 425;
native PTY T39 48 and T42 34; unchanged recovery_v02 one passed.

Current normal ELF SHA256, verified before and after the direct qualification:

```text
debug   16cb31cc5a4eb4e8bf3b6afd465ed2512e703050b7d5bc87695f5f71d4afe9b6
release 7044b05c932ff7b96139024d8898335b12f1df2f414f2245eb4c87617c57dd1e
```

Association is HEAD `8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0` **plus current
reviewed dirty source/new test modules**, not clean HEAD. No Cargo ran after the
last normal builds and before/among the direct ELF checks; final documentation
reporting follows those checks. Older intermediate hashes are superseded.

Current T54 physical totals: debug main 18/child 2/compaction 4/title 8 = **32**;
release main 18/child 2/compaction 4/title 9 = **33**. The restart title kill/send
race is recorded exactly. Mixed retry/continuation, auth override, quota, effect
once, child, summary/correction, title failure, parked/reopen and past-due restart
remain green. Each binary has one Bash append and one subagent intent in that suite.

Directed T50 background observations per ELF: Revert five requests/two jobs/two
effects, old notice hidden/new visible; Fork seven requests/one job/one effect,
source and fork notice once/no fork-owned job; joined-capacity reuse four requests,
nine jobs/nine effects/notices, zero idle HTTP and one reused slot. Whole 26-case
background matrix was not duplicated: the affected integration boundary uses
these three directed real cases plus full current workspace owner guards.

### Diagnosed failed approaches and correction

1. Initial checker clean PTY exit used SIGTERM and observed -15. Existing native
   clean-exit route is Ctrl+D/DeleteOrQuit; using it yields exit 0 and permits
   independent termios restoration verification. No product change.
2. The first full workspace command compiled for 81 s and hit the unchanged
   900000 ms command limit late in TUI. It also exposed the concrete unchanged
   recovery_v02 local Completion/IndexConflict: omitted observed reasoning prefix
   collided with a newly recovered terminal-only call. A focused one-socket RED
   preceded the minimal first-vacant-slot correction in `complete_response`.
   Provider 35 and unchanged recovery test then passed. Existing length-only
   partial-message append semantics were retained exactly. No other R3 product
   change, observed-slot relocation, validation relaxation or retry change.
3. The first failed workspace log is a material diagnostic only, not current
   gate evidence: `tool_0f1edb11e001H4zVJCXCltdltN`. Final precompile and full test
   commands were separate within the same 900000 ms limit; all 42 summaries pass
   in the current full log above. No threshold, cap, ignore or timeout was raised.
4. EOF checker initially mixed FD select with buffered `readline`; the latter
   prefetched the retry line, leaving select empty. An isolated owned experiment
   proved one stored retry and captured typed retry JSON with exit 130. The checker
   now waits for the SQLite fact, cancels, then parses captured JSON after
   communicate. No generated-text matching or product change.
5. Final checker explicitly disables daemon HTTP workers so server_close joins
   them. Both normal ELFs were requalified after this cleanup improvement; the
   reported counts/hashes above are the final ones, not the earlier six/seven-case
   intermediates.

### Resources, paths and next owner

Final safe resource sample: UID 1003; available memory 7640 MiB; available disk
193347 MiB. Allocated `evidence/T55` approximately 1 MiB; small checker/reports,
no new raw corpus, no campaign capture logs. Managed logs remain below 16 MiB.
Temporary native roots are removed after owned process/server/worker joins.
Largest changed handwritten Rust file 3678 lines; provider 2739, new provider
reconciliation tests 529/runtime fixture 148. `docs/CODE_MAP.md` names the actual
provider tests/native checker/explicit relay caller seams.

This R3 atomic adds the checker/report and a focused omitted-prefix regression,
with the minimal in-scope completion correction described above. Preserved R1/R2
source/tests and the parent's optional-index repair remain included. Existing
pre-fix sparse diagnostic/receipts are immutable and were not rerun as acceptance.

No external blocker remains for assigned offline R3/relay prerequisites.
Parent independent review/commit and **R4 actual real-API/catalog/selected and
configured model binding/live campaign continuation remain OPEN**. No paid/live
request, credential/user environment inspection, real campaign read/reset or
status/progress/spec/GOAL/Acceptance/stage/commit/push mutation occurred. Whole
T55/READY and T44 pixel qualification are not claimed. All sole mutation/Cargo/
owned-fixture authority is released to the parent at final handoff.
