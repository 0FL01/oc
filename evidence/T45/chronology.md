# T45/R10 — actual root/child chronological system and effort consumer

Scope: PRM01's approved chronological consumer of T53/GO03, not another protocol
parser, selection engine, model allowlist or history store. The previous T45
dependency is gone: T53 is delivered. AUTH06 is deferred by the owner; this slice
uses no credentials/live provider. T44 remains PAUSED and owns paired visuals.

## Reproduction and cause

The new normal-ELF fixture initially observed captured `low` at the top level and
no in-position updates despite declared support and durable idle effort facts.
The next-event query attached the fact to a `model_switch` metadata message,
which is not provider-projected. Even at a genuine user boundary, accepting that
message changed a trailing fact into a next-message fact while the rebuilt prior
history excluded the current input. Merely adding the current-input capture did
not fix the nonprojected notice boundary; the observed wire stayed wrong.

The existing `storage_effort.rs` now resolves the next projected text boundary,
skipping model-switch notices. Fresh root **and** child journals capture idle
facts at their own accepted input before that user message. Instruction positions
use the resulting input length, preserving managed source reconciliation. Busy
fact identity/dedup and GO03 protocol lowering are unchanged. No migration/DTO,
existing raw-message rewrite, policy/budget increase or retry framework was added.

## Direct consumer proof

`native_chronology.py` runs the actual **normal** binary in isolated temporary
HOME/project/data roots against joined local HTTP peers. Between clean process
runs it inserts only new typed synthetic history/selection fixtures; existing
messages, settled operations and original turns are immutable. Both root and one
own-model child perform first request, reopen at `low`, then reopen at Default.
A settled child `read` is never executed again.

Six cases qualify:

1. Responses declared support: developer updates in original order; first
   `previous=None` remains the top-level baseline; `low` then default/`medium`
   updates stay in position on both root/child requests and follow-ups.
2. Responses unsupported: markers absent, captured current low/Default used.
3. Responses final mismatch: markers absent, captured high used instead.
4. Chat: escaped in-place lower-authority user text, no effort markers.
5. Messages unsupported: same escaped fallback, captured current effort.
6. Messages explicit support **and valid user/system/assistant slot**: native
   chronological system/output-config updates and conditional beta header.
   Declaration alone is not permission for arbitrary native placement; the
   finite GO03 placement guard remains intact.

Per case: **10 primary + 1 bounded title requests**, six completed root/child
turns, one child identity, one settled read, exact private durable low/Default
facts, no duplicate system/effort markers, no existing-turn mutation. All model
output caps are ≤2048 (title256); real provider requests **0**. Children, binary
process groups and peer workers are joined/reaped. This is synthetic consumer
qualification, not AUTH06 or a new live accounting campaign.

## Checks

- Targeted `cargo test --locked -p oc-adapters --lib prm01_idle_effort_boundary`:
  **1 PASS**, including notice-boundary/reopen/immutable-history assertions.
- `cargo build --locked -p oc`; actual debug six-case fixture: **PASS**.
- `cargo fmt --all -- --check`; strict workspace/all-target Clippy: **PASS**.
- `cargo test --locked --workspace --no-fail-fast`: **1712 passed / 0 failed /
  11 unchanged opt-in ignored**, 46 result records. Existing runtime, linked
  controls, retry, instruction, recovery, storage, PTY and resource gates pass.
- Broad gate log: approved temporary `t45-chronology-current.log`. Its shell
  envelope interrupted the subsequent cold release compilation **after** all
  workspace/doc targets were green; no test deadline/stack/expectation was raised.
- Current normal release build and six-case fixture: **PASS**. The existing
  nested-instruction regression also passes on current debug **and** release:
  15 requests/13 facts each, root/child/restart/Location and failed/symlink/source
  guards intact, old turns immutable, owned cleanup joined. Approved temporary
  `t45-chronology-release-current.log` ends
  `T45_CHRONOLOGY_REMAINING_GATES_PASS`.

Serial Cargo, normal stacks, jobs3/testthreads2 and approved TMPDIR throughout.
The separate R3 safe terminal child-event supplement remains next; no full T45
DONE, product READY, T44 VIS39/VIS43 or T57 AUTH06 PASS is implied by this receipt.
