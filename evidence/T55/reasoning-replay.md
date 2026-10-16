# T55 — frozen completed reasoning replay repair

Base `19b224ef065496eb7de9e4c8ef8384bf46a84e3f`, clean Rust at handoff.
Pre-existing live operator files and `.opencode` are preserved. Freeze before RED.
Donor `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/ai/src/protocols/open-responses-continuation.ts:193–200` explicitly
retains actual done reasoning when completion re-encrypts the same ID;
`test/provider/openai-responses.test.ts:733–782` proves continuation using the
emitted ciphertext; `open-responses.ts:1284–1318` emits actual done metadata.

## Result

| Frozen obligation | Initial result |
|---|---|
| Successful completed terminal, same actual done reasoning ID/type/index, only string encrypted_content changed: preserve first validated emitted bytes, no duplicate output/call | NOT_RUN |
| Done→done, other reasoning fields, other opaque/message/function types, identity/order/index/status, terminal-only repeated conflict remain strict; length/unknown/incomplete gain no exception | NOT_RUN |
| Runtime actual tool executes once; next same-provider wire input and durable storage/fork preserve done bytes, not terminal bytes; public diagnostics omit opaque content | NOT_RUN |
| Both normal native ELFs patch/read/final/reopen with re-encrypted terminal reasoning; current negative/unclosed/unknown/relay and T54 regressions stay valid | NOT_RUN |
| Current fmt/strict workspace Clippy/full tests/Python47/docs/read-only progress/size/diff/builds/help; hashes associated with base plus dirty diff | NOT_RUN |

## Checks

Start with one physical-socket provider RED, then narrow provenance-aware repair
and concrete runtime/fork boundary. Serial Cargo jobs3/threads1/offline/locked,
approved TMPDIR, command timeout900000 ms; normal builds separate. UID1003,
available memory6808 MiB/disk193290 MiB. No caps, deadlines or ignored tests change.

## Risks

The completed-item map also receives terminal-only items: ID equality alone is
not proof of actual prior done. Explicit emitted provenance must survive inferred
index relocation. Only genuine successful completion can retain done ciphertext;
no encryption decoding, model/vendor inference, blanket opaque exemption or
tool authorization from incomplete evidence. Other jointly observed fields stay
strict; missing optional fields retain the existing union behavior.

## Next

RED/GREEN, concrete continuation/fork and current native/gates, then parent
independent review and configured-binding live qualification. Live operator
files/actual campaigns, credentials/environment and remaining budget are not read
or changed here. Whole T55/R4 remains open.

## Current result

Coordinator review: inspected the complete production/test/helper diff and pinned
continuation source. Independently repeated `prov09_` owner/fork tests (12 PASS)
and the actual runtime continuation test (1 PASS), then normal locked build,
fmt/docs/progress/diff checks (exit 0). Both normal hashes below match the final
agent qualification. The live operator's transient bounded JSON comparisons
export only fixed field-name differences; neither ciphertext nor arguments are
written to reports. That diagnostic observation is not a claim of ciphertext
equivalence or authority to alter other fields.

The initial table is the pre-RED freeze. Qualification below belongs to
`19b224ef065496eb7de9e4c8ef8384bf46a84e3f` plus the current reviewed dirty
implementation/test/helper diff, not clean HEAD or a completed live R4.

| Frozen obligation | Current observation | Result |
|---|---|---|
| Actual done replay | One-POST RED reproduced Completion/IndexConflict; GREEN returns one actual reasoning item with first done ciphertext, one completed call, stop and usage. No duplicate output. | PASS |
| Narrow strict exception | Done→done ciphertext, summary/status/type, other opaque/message/function fields, identity/index/order/arguments remain strict. Terminal-only repeated ciphertext conflict and an optional terminal-filled ciphertext cannot claim emitted provenance. Length rejects the changed ciphertext. Terminal-only bytes and omission union remain supported. | PASS |
| Runtime/storage/fork | Actual Bash executes once on two POSTs; next wire input and saved canonical input contain first done bytes and one paired call/result, zero retry. Separate closest storage fixture forks/reopens a fulfilled canonical journal, preserving opaque bytes/provider/model and paired wire identities. | PASS |
| Current normal native | Both ELFs, seven current T55 cases with opt-in re-encryption: headless and real PTY patch/read/final/reopen, exact bytes/two completed operations, zero spurious retry. Conflict/unclosed/EOF zero effects, actual crash unknown/no replay and enforced offline relay remain green. Current T54 nine cases each remain green. | PASS |
| Current gates/artifacts | Full fmt/strict workspace all-targets Clippy/tests, Python47/docs/read-only progress/code-size/diff pass; separate normal debug/release builds/help pass; hashes unchanged across all direct ELF checks, no Cargo interposed. | PASS |

### Exact replay rule and strict-negative change

The single `CompletedItem` owner has one private `emitted_reasoning` bit: it is
set only by validated `output_item.done` reasoning with string ciphertext and is
preserved across inferred-index relocation. Terminal-only recovery and optional
fields added by terminal union do not create that evidence.

At the same retained index, only Completion + Stop + actual emitted reasoning +
same nonempty ID/type + string ciphertext on both observations permits ignoring
the terminal **encrypted_content** comparison. The merge keeps the existing done
field instead of replacing it. Every other jointly observed field must still
agree; prior index/identity/status/argument/order checks are unchanged. Done→done
remains strict. No encryption decode, free-text inference, output filtering,
model/vendor special case, new network/retry owner or public API is introduced.

The old negative `prov09_done_and_terminal_conflicts_are_local_and_safe` expected
reasoning ciphertext alone to conflict across done→completed. That expectation
was superseded **only because** the pinned donor explicitly documents and tests
re-encryption with replay of the emitted item. Its ciphertext negative now tests
done→done; a shared summary conflict remains a completed-terminal negative.
All function call/identity/index/name/arguments and other opaque negatives remain.

### RED/GREEN and current commands

All Cargo commands: serial jobs3, test threads1, offline, locked, approved TMPDIR
`/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`, command
timeout900000 ms. No cap/deadline/assertion/ignore suppression or dependency,
toolchain, lockfile, schema/framework change.

| Command | Exit | Actual result |
|---|---:|---|
| `cargo test -p oc-adapters --lib --locked prov09_completed_reencrypts_reasoning_but_replays_actual_done` before production repair | 101 | One test RED; one actual POST, local Completion/IndexConflict |
| Same command after repair | 0 | One GREEN; preserved done bytes/call/stop/usage |
| `cargo test -p oc-adapters --lib --locked provider::` | 0 | 37 passed, 348 filtered; existing guards plus two reasoning-replay cases |
| `cargo test -p oc-adapters --test runtime --locked prov09_reencrypted_completion_replays_done_bytes_and_executes_once` | 0 | One passed, 107 filtered; two POSTs/one intent/one `effect` append/zero retry |
| `cargo test -p oc-adapters --lib --locked prov09_fork_and_reopen_preserve_canonical_done_reasoning_and_binding` | 0 | One passed, 384 filtered; fixture-seeded canonical storage/fork boundary, not another network campaign |
| `cargo fmt --all -- --check` | 0 | Current formatting |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | Strict all-targets check, 16.72 s |
| `cargo test --workspace --locked --no-run` | 0 | Separate precompile, 1 m 17 s; unchanged full-gate timeout |
| `cargo test --workspace --locked --no-fail-fast` | 0 | **1349 passed, 10 pre-existing ignored, zero failures; 42 successful summaries** |
| `python3 -B -m unittest discover -s scripts -p 'test_*.py'` | 0 | 47 passed, four current suites, 9.172 s |
| `python3 -B scripts/check_docs.py`, `python3 -B scripts/progress.py check`, `git diff --check` | 0 | Docs/diff/read-only journal structure |
| `python3 -B scripts/code_size.py --base 19b224ef065496eb7de9e4c8ef8384bf46a84e3f --changed` | 0 | Four changed Rust files, 4041 physical lines/156571 UTF-8 bytes; largest provider2766, below soft5k |
| `cargo build --locked` | 0 | Normal debug, separate command, 9.10 s |
| `cargo build --release --locked` | 0 | Normal release, separate command, 1 m 55 s |
| Direct debug/release `--help` | 0 | Both normal ELFs |
| `python3 -B evidence/T55/native_completed.py target/debug/oc --reasoning-replay` and release equivalent | 0 | Seven cases each, counts below |
| `python3 -B evidence/T54/native_runtime.py target/debug/oc` and release equivalent | 0 | Nine cases each; current mixed/effect/unknown-recovery and other retry lanes |

**Current full workspace log:**
`/home/opencode/.local/share/opencode/tool-output/tool_0f258d15e001r8q3lA1o61tAJE`.
Managed ID `tool_0f258d15e001r8q3lA1o61tAJE`: 39 test targets plus three empty
doc-test targets; adapter unit385, runtime108, TUI425. Full gate includes genuine
length/prior-done and unknown-effect guards; no obsolete historical native retry
fixture was run.

### Actual native observations/counts

The new checker flag is opt-in and preserves the existing default topology.
In the controlled read step, reasoning done index0 and function done index1 are
followed by a completed array with the identical shared fields and changed
reasoning ciphertext only. Following request and durable journal assertions
require exactly one original done reasoning item, never terminal ciphertext;
public output/display parts must omit both opaque canaries. No raw payload dump.

| Current T55 case, each ELF | Main | Title | Outcome |
|---|---:|---:|---|
| Headless patch/read/final/reopen with re-encryption | 4 | 1 | Two completed tool operations/turns, exact `hello\n`, zero retry |
| Real PTY/reopen with re-encryption | 4 | 1 | Same graph/bytes, terminal restored, zero retry |
| Genuine argument conflict | 1 | 1 | Failed, zero intents/files/retries |
| Unclosed response | 1 | 1 | Cancelled exit130, zero intents/files/retries |
| EOF without terminal | 1 | 1 | Cancelled exit130 after genuine attempt2 fact; zero intents/files |
| Actual crash/unknown recovery | 2 | 1 | One append, old operation/turn unknown, next explicit command completed, no replay |
| Enforced offline relay | 5 | 1 | Six generation reservations/complete receipts, one genuine503 retry, patch/read once |
| **Each ELF total** | **18** | **7** | **25 actual POSTs** |

Offline relay preserves its private fixture identity through helper/native
restart, proves reservation visible before upstream, and rejects three malformed
typed bodies with zero upstream/reservation. Actual generation6/control0/MCP0,
one503/five200 complete receipts; providerToolNames apply_patch/read/shell.
These fresh fake envelopes are not any existing live campaign.

Current T54 totals: debug main18/child2/compaction4/title8 = **32 POSTs**;
release main18/child2/compaction4/title9 = **33**. The restart kill/send race is
observed exactly. Nine cases cover mixed continuation, override/quota, committed
effect once, child, auxiliary correction, one-call title failure, parked reopen
and past-due restart non-dispatch. Truthful length is checked by current provider
and runtime/workspace tests, not misclaimed as an extra native case.

Current normal ELF SHA256, identical before/after direct qualification:

```text
debug   92b7fadd36c8c439a77c36deb24052deb036c067ddc95d0eed0717e9b7a46902
release acdb29ce8b9a5dd42a29eb164b147e24183d0536f2c988d4216c4611fffcaec1
```

Source association: base `19b224ef065496eb7de9e4c8ef8384bf46a84e3f` plus current
reviewed dirty source/test/helper changes. No Cargo ran after the final normal
builds and among direct ELF checks. Documentation-only reporting follows them.
The only failed test approach in this atomic is the intended source-traced RED;
all subsequent current gates/native commands passed without blind repeats.

### Paths, ownership, resources and next

Changed: `provider.rs`, existing `provider/tests/reconciliation.rs`,
`tests/runtime/reconciliation.rs`, `storage_fork_dcp_tests.rs`, optional checker
mode in `evidence/T55/native_completed.py`, `docs/CODE_MAP.md`, this report.
Old evidence reports remain historical. Existing untracked live operator helper
and report are preserved without inspection/mutation, as is inherited `.opencode`.

Native fixture processes/owned leaf groups/adopted children/relay/helper/HTTP
workers and server threads are joined; temporary fixture homes removed. Final
safe resource sample: UID1003, available memory6076 MiB/disk192584 MiB;
`evidence/T55` allocated approximately1 MiB, no new raw corpus or large captures.

No external blocker is proven and no assigned offline obligation remains open.
Parent independent review and configured-binding live qualification are next,
using the existing campaign and remaining allowance under the parent's exclusive
authority. No real API, private input/environment/journal/config/auth inspection
or live campaign mutation occurred here. Progress/spec/GOAL/Acceptance/stage/
commit/push were not changed. Full T55/R4 is not PASS on this offline repair.
Sole mutation/Cargo/owned-fake-fixture authority is released at final handoff.
