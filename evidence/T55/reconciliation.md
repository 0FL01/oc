# T55 R1/R2 — frozen parser atomic

## Independent coordinator review

Reviewed every changed source/test path at `8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0`
plus this dirty diff. A read-only scout identified a concrete optional-index
counterexample: the same added call, first indexed then index-less (or reversed),
must preserve known index evidence rather than classify omission as contradiction.
The existing announced-identity regression was extended, first corrected for a
test-only moved JSON value, then reproduced RED exit 101 with
`OutputStructure { Added, IdentityConflict }`. The narrow announcement merge now
retains/enriches a non-conflicting optional index; actual conflicting indices,
names/call IDs and another call's occupied index remain rejected. One observer
start is emitted. Fresh `cargo test -p oc-adapters --lib --locked provider::`
passed all **34** cases, including both optional-index orders. No cap, retry policy,
effect admission or existing assertion was weakened. A formatting-only check
then reported one layout change, applied before the subsequent gates.

R3 normal-binary/full-workspace/headless/PTY and R4 actual live remain open; this
independent provider pass is not a whole-task acceptance claim.

Base: `8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0`.
Donor: `2670273ff17da96f85c5826ced57aa1b368754fa`, done handler
1238–1281 and completion recovery 1324–1368. Freeze before implementation/RED.

## Result

Pending qualification of these obligations only:

| Obligation | Required observation | Initial result |
|---|---|---|
| R1 sparse reconciliation | Synthetic normalized done call + completed empty output yields exactly one canonical call and usage on one physical POST; omitted/full/partial snapshots preserve completed items in observed index order | NOT_RUN |
| R1 bounded owner | Existing completed-item owner coalesces identical repeated evidence and rejects conflicting index/item/call/name/final JSON arguments; no overwrite or duplicate call | NOT_RUN |
| R1 termination | No delta-only recovery or tool admission on missing terminal, failed/incomplete response, or unclosed physical attempt; ordinary completed-only terminal recovery remains supported | NOT_RUN |
| R1 preservation | Messages, opaque/encrypted reasoning, text, usage and call/result graph survive; length requires genuine matching prior done evidence and keeps narrow partial assistant-message exception | NOT_RUN |
| R2 local failure | Local malformed/contradictory output has typed bounded structural stage/code diagnostics, never accepted-read/EOF retry even after partial output or observed true | NOT_RUN |
| R2 safety | Debug/Display carry no IDs/names/arguments/opaque/messages; caps/cancel/typed HTTP/SSE failures and existing provider/runtime guards remain enforced | NOT_RUN |

## Checks

Planned: minimal actual-Generation RED/GREEN, focused reconciliation negatives,
existing provider and runtime regression targets, fmt and strict Clippy. Use only
owned synthetic loopback fixtures, serial offline locked Cargo (jobs 3, threads 1,
approved TMPDIR, timeout 900000 ms). Preflight UID 1003, available memory 7586 MiB,
disk available 192621 MiB. No new crate/dependency/store/protocol/retry owner.

## Risks

Sparse terminal array positions cannot erase actual completed indexed observations;
identity-based matching must not treat compact omission as a contradictory index.
Length may not manufacture terminal-only tool completion. Missing successful
termination remains distinct from deterministic local rejection. R3 actual rebuilt
binary qualification and R4 bounded live qualification are parent-owned and open.

## Next

RED from the normalized fixture, then narrow reconciliation and local error typing.
No task status, progress, stage/commit/push or live changes in this atomic.

## Current Result — R1/R2 parser atomic

The initial table is the pre-RED freeze. Current observations:

| Obligation | Result | Evidence |
|---|---|---|
| R1 sparse reconciliation | PASS | Normalized fixture now returns the actual done call, stop finish and usage 100/40 on one POST; omitted/empty/full/partial/repeated snapshots preserve indexed canonical output |
| R1 bounded owner | PASS | One completed-item map retains actual versus inferred index provenance. Same completed evidence is coalesced; explicit-index, item-ID/call-ID/name/final-argument and order conflicts are rejected before Generation/tool admission |
| R1 termination | PASS | Missing terminal and provider failed/incomplete remain non-success. Delta-only successful terminal cannot manufacture done. Held response cancels after one POST without Generation. Completed-only terminal recovery compatibility remains accepted |
| R1 preservation | PASS | Messages, opaque/encrypted fields, text and usage are preserved. Index-less legacy events allow authoritative terminal insertion/order. Genuine prior-done length tools survive; terminal-only length calls and failed opaque/message states remain rejected |
| R2 local failure | PASS | `ProviderError::OutputStructure {stage,code}` carries enum-only local facts. It bypasses the existing read-failure conversion and finite retry owner; observed true and partial text do not make it retryable |
| R2 safety | PASS | Error Debug/Display canary assertions exclude IDs/names/arguments/message/opaque content. Existing provider caps/cancel/HTTP/SSE/length tests, complete runtime safety target and retry-owner tests pass |

Implementation source association: HEAD
`8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0` plus the current reviewed dirty
diff/new focused test modules. No production model string, classifier inference,
new protocol, store, dependency, retry loop or runtime/UI rewrite was introduced.
The existing first validated terminal closes the physical response before return;
this atomic does not introduce an EOF-draining network owner or wait after success.

### Reconciliation details

- Done items remain in the existing completed-item owner. Explicit done/announced
  indices are fixed; legacy absent indices are provisional arrival positions.
  A snapshot covering those observations may supply missing interleaved items and
  establish provisional positions. Sparse snapshots use observed identity/index.
- Independently validated observations may omit optional fields. Non-conflicting
  fields are preserved as a union; jointly observed differences are rejected.
  A shared call_id never silently assigns a different/missing item identity.
  Actual argument-done JSON is checked against completed function arguments.
- Repeated done/snapshot evidence emits no duplicate canonical calls or opaque
  observer state. Genuine completed terminal items may fill missing observations;
  delta accumulation alone cannot do so. Length keeps its stronger exact prior
  done-body/ID requirement and narrow partial assistant-message allowance.
- Structural diagnostics contain only fixed `OutputStage`/`OutputCode` values.
  Actual HTTP/SSE failures and EOF retain the existing T54 physical-failure facts;
  local validation never receives HTTP-200 read/EOF or observed-header eligibility.

## Current Checks

All Cargo commands: serial, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`, approved TMPDIR above, timeout 900000 ms, locked.

| Command | Exit | Actual result |
|---|---:|---|
| `cargo test -p oc-adapters --lib --locked prov09_sparse_completed_retains_actual_done_call` before implementation | 101 | One test RED: actual one POST; `Request(IncompleteStream, HTTP 200, Accepted/Read, output_committed=true)` instead of Generation |
| Same minimal command after implementation | 0 | One test GREEN; retained done call and usage |
| `cargo test -p oc-adapters --lib --locked provider::` on final source | 0 | 34 passed, 347 filtered, no ignored/failures; original 26 plus eight focused reconciliation tests |
| `cargo test -p oc-adapters --test runtime --locked prov09_` | 0 | Two focused tests passed; full target below rechecks them on final source |
| `cargo test -p oc-adapters --test runtime --locked` on final source | 0 | 107 passed, none filtered/ignored; includes four RET01 cases, permissions/media/DCP/tool graph/recovery and current T50 background guards |
| `cargo test -p oc-adapters --lib --locked runtime::retry::tests` | 0 | Four passed, 377 filtered; finite allowance/eligibility/recovery/dispatch-family guards |
| `cargo fmt --all -- --check` | 0 | Workspace formatting |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | Strict workspace all-targets check |
| `python3 scripts/check_docs.py`, `git diff --check` | 0 | Documentation/diff checks |
| `python3 scripts/code_size.py --base 8004fdac1d20e70b5f8718e03b3c9f5dce1ba3f0 --changed` | 0 | Seven Rust files, 10824 physical lines/400704 bytes; provider 2715, new provider tests 478/runtime tests 148; no 5k-file issue |

Compiled test artifacts: `target/debug/deps/oc_adapters-e39a9a5f0e8df351`
and `target/debug/deps/runtime-ccee034e210e5300`, associated with the final dirty
source above. These are test executables, not R3 normal application-build proof.
Normal debug/release rebuilds, workspace test gate and direct native/PTY R3
qualification were deliberately left for the parent next atomic.

### Actual effect/count observations

New runtime negative fixture uses six fresh owned sessions/projects:

| Failure topology after partial text | Main POSTs | Retry events | Tool intents/effect files |
|---|---:|---:|---:|
| Conflicting completed call versus terminal | 1 | 0 | 0 / 0 |
| Delta-only call plus successful empty terminal | 1 | 0 | 0 / 0 |
| Bad final arguments JSON | 1 | 0 | 0 / 0 |
| Real EOF after done, then terminal quota on continuation | 2 | 1 | 0 / 0 |
| Provider failed/quota after done | 1 | 0 | 0 / 0 |
| Provider incomplete after done, then terminal quota | 2 | 1 | 0 / 0 |

All negative diagnostics omit canaries. Positive sparse done + identical repeat +
completed empty terminal uses two main POSTs (tool step then final), zero retries,
one durable Bash intent and file bytes exactly `effect`; the second request has
the completed call's matching function_call_output. This is a narrow Runtime
admission proof, not the rebuilt apply_patch/read/PTy/live R3/R4 campaign.

### Diagnosed intermediate failures

1. Minimal real-Generation RED reproduced the diagnosed sparse-array discard.
2. Initial broad provider run had 24 pass/three failures in superseded local-error
   expectations. Deterministic unfulfilled successful terminal/length evidence
   now expects local structural rejection; actual EOF/provider incomplete still
   expects physical incomplete facts. Parsing assertions now check enum-only
   structural diagnostics. No admission/cap/cancel assertion was removed.
3. A test helper initially moved a JSON value across its two-case loop; corrected
   by cloning the synthetic fixture. This was compile-only, not product RED.
4. First full runtime run had 102 pass/five failures. Investigation proved that
   treating inferred arrival positions as actual observed indices rejected valid
   interleaved terminal messages; rejecting empty legacy opaque IDs also broke
   established public reasoning behavior. Added explicit provenance and retained
   legacy opaque compatibility, with a focused provider regression. Conflict
   checks remain strict for actual indices and function identities.
5. Canonical read-pair fixture omitted `summary:[]` in its terminal item although
   its done helper supplied it. Reconciliation correctly preserves the field;
   the synthetic terminal fixture now includes that actual observed empty field,
   retaining all original exact order/replay/pairing assertions.
6. A second runtime run had 106 pass/one failure: permissive optional-field union
   let a shared call_id assign an ID to an anonymous earlier call. Fixed immutable
   function item identity across observations. Final run passes all 107. The
   duplicate-call guard now observes zero successful rounds, rather than one,
   because rejection happens before Generation; its one POST/zero-intent checks
   remain enforced. Length-terminal-only and unfulfilled completed-call counters
   likewise now require one POST, strengthening the new no-local-retry contract.

## Current Risks / Next

No external blocker encountered. Parent independent review and R3 normal builds,
full workspace/native headless/PTY apply_patch/read/reopen/negative effects proof
remain open. R4 actual API/campaign continuity is parent-owned and untouched; no
live request, credentials/environment/user-HOME inspection or campaign mutation
was performed. Existing diagnosis/pre-fix native receipts were not rewritten or
reused as post-fix PASS. T55 as a whole and product READY are not claimed.

Changes: `provider.rs`, new `provider/tests/reconciliation.rs`, nearest typed
error guards, existing runtime test module wiring/new reconciliation pack and
changed-contract assertions/fixture, `docs/CODE_MAP.md`, this report. All source
mutations used apply_patch; fmt used the normal formatter. No progress/spec/GOAL/
Acceptance/stage/commit/push changes; inherited `.opencode` was not inspected.
Reports and focused fixtures are small (well below 1 MiB); no new raw corpus,
campaign logs or matrix artifacts. Temporary sole mutation/Cargo/owned fake-fixture
authority is released to the parent on return.

## R3 follow-up — omitted observed prefix and terminal recovery

The sections above record the earlier R1/R2 atomic and independent optional-index
repair; their historical commands/counts are not rewritten. The subsequent R3
workspace gate reproduced another concrete in-scope parser fault in unchanged
`oc/tests/recovery_v02.rs::binary_restart_projects_real_metadata_and_parts`:
an observed, index-less completed reasoning item occupies arrival slot zero,
while the sparse terminal omits it and supplies two genuinely completed new
calls. Assigning the first terminal call to snapshot position zero collided with
that retained opaque item and returned local Completion/IndexConflict.

Added `prov09_sparse_terminal_recovery_keeps_omitted_observed_prefix` before the
correction. Its inferred-index and explicit-zero variants each use the existing
actual socket fixture; RED exited 101 on the first variant with one POST and
`OutputStructure { Completion, IndexConflict }`. The minimal `complete_response`
correction places an otherwise unobserved terminal item in the first vacant slot
at or after the next canonical position. It never overwrites or relocates an
observed slot. Existing observed identity/index/order validation, the parent's
optional announcement merge, ordinary completed validation, and the stronger
length prior-done requirement remain in place. The prior length-only partial
assistant-message append behavior is preserved exactly.

Current targeted GREEN: provider **35 passed**, 347 filtered; unchanged recovery
target **one passed**, none filtered. Current full workspace: **1345 passed,
10 pre-existing ignored, zero failures**. Both normal native ELFs also exercise
terminal-only read recovery after an omitted actual reasoning item, followed by
the matching function_call_output and final completion. Current builds, hashes,
native/relay counts, full managed workspace log and diagnosed fixture failures
are recorded in [native-qualification.md](native-qualification.md).

R3 offline native qualification is now complete for parent review. R4 real API,
selected/configured binding and continued live campaign remain parent-owned and
open; this follow-up does not claim whole T55 or READY.
