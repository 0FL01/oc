# T55 diagnosis — valid done tool call lost at sparse response.completed

Date: 2026-09-30. Result: **CONFIRMED COMPATIBILITY DEFECT; NOT FIXED**.
Scope: requested diagnosis/context/repair plan, not implementation or product PASS.
Repository source HEAD: `592eb1887f15aa853700ba25eea7e042cdaf3d0e`.
Active T50 dirty implementation belongs to another agent and was not edited/built.
Non-root uid 1003; startup/SQLite/config inspection was read-only. Synthetic
offline executions wrote only their disposable isolated HOME/project/data fixtures.

## User incident and causal boundary

Reported prompt asked to write then read a file; UI displayed reasoning headings
and `Retry scheduled · attempt 5 · incomplete stream (HTTP 200)`.
Read-only native SQLite inspection of the matching turn found four failed spans
and a cancelled fifth, no successful Generation, no tool operations and no usage.
Persisted turn wire model was `cx/gpt-6-luna`, selected reasoning `high`. Proxy UI
label/route is not the model ID; do not guess `codex/gpt-6-luna` from that label.

The later startup trace had discovery HTTP 200, 44 models and admitted selection.
Earlier catalog 401 is separate and does not explain this tool-phase failure.
No filesystem permission refusal was observed: runtime never admitted a tool.
The original offending SSE event was not retained in the user's journal, so exact
historical event attribution is unavailable. Repeated current live topology plus
existing-ELF differential proves a matching concrete defect, not a raw replay of
that historical request. SUCCESS in proxy accounting can coexist with local
adapter rejection; INTERRUPTED alone does not assign blame to the proxy.

## Live wire observations (no tool execution in direct probes)

Credentials were loaded as literal dotenv data from authorized gitignored
`.local/live.env`; never printed or sourced as code. Its OC_TEST_MODEL is bare
`ocg/muse-spark-1.3-contributor`, variant absent/provider default. The incident
model differs: explicitly inspected saved selection, then verified exact catalog
membership and advertised `high` effort before incident probes. No product config
change, automatic fallback or authoring-agent credential use occurred.

Native-shape request: model, store:false, stream:true, input, custom function
schema, include reasoning.encrypted_content, max_output_tokens 2048 and selected
effort where applicable. No tool_choice or summary override on matching probes.
HTTPS/public-address guarded connect, no redirect/env-proxy bypass, one physical
attempt per probe; bounded response 8 MiB, socket timeout 45 s, wall budget 60 s.
Raw response/event bodies were parsed in memory only. Retained fields are typed
event/status/count/size/presence facts, not text/IDs/arguments/encrypted contents.

| Ledger reservation | Explicit generation choice | HTTP / duration | Observation |
| --- | --- | --- | --- |
| 2 | configured Muse, extra forced tool_choice | 400 / 764 ms | Preliminary non-native-shape probe; exact cause not proven. Counted, not a product regression or successful generation. |
| 4 | configured Muse, native shape/default | 200 / 32579 ms | 15 SSE events, 50403 bytes; completed reasoning/message/function_call, completed terminal with all three output items; args 78 bytes valid JSON, usage 574/1882. |
| 7 | incident cx/gpt-6-luna/high, native shape | 200 / 1635 ms | 28 events, 11657 bytes; completed done call, terminal completed with empty output; clean EOF. Initial report did not separately record field presence. |
| 9 | same incident binding/native shape | 200 / 2322 ms | Same topology; explicitly confirmed output field present, array type, length zero. |

Control reservations 1/3/5/6/8 all GET models HTTP 200. Reservation 5 was followed
by a diagnostic-only variant lookup failure (wrong nesting); no generation was
dispatched. Lookup corrected to metadata.opencode.variants from discovery owner.
No product correction occurred. Preliminary failures informed different subsequent
experiments; no unchanged blind retries were issued.

Incident structural sequence, both matching probes:

1. response.created; response.in_progress.
2. output_item.added: function_call, status=in_progress, output_index=0,
   item ID and call ID present.
3. 22 function_call_arguments.delta, then arguments.done.
4. output_item.done at event 27: function_call, status=completed, ID/call ID/name
   present, final arguments 79 bytes and valid JSON.
5. response.completed at event 28: response.status=completed, **output field
   present, array, empty**; usage input81/output35, no trailing nonblank bytes.

No reasoning item/status was present in these incident probes. A prior hypothesis
about reasoning `in_progress` is not the observed cause. Muse opaque reasoning
had status completed. The live calls were fully drained, not prematurely dropped
by the diagnostic parser. Ledger `complete` means HTTP-drained, not native success.

## Exact native source seam

At inspected HEAD, `crates/oc-adapters/src/provider.rs`:

- `dispatch` ~563–606 records announced calls and validates/stores done items by
  output_index; ~620–634 accepts genuine response.completed with completed status.
- `complete_response` ~640–700 sets `self.output=Some(response.output)` whenever
  terminal output is an array, **including []**. Completeness then checks announced
  calls only against this Some array. Fallback to accumulated `output_done` is
  used only for None (omitted field), not Some(empty). The completed call vanishes
  from the checked output and raises `ResponseIncomplete`.
- `read_failure` ~1218–1245 maps that local validation rejection and actual EOF to
  the same accepted Read/IncompleteStream HTTP200. Finite retries repeat the same
  compatibility defect; more retries cannot restore the discarded output.
- Runtime tool admission follows successful Generation only; thought text is
  not a filesystem operation. This explains zero tool_operations.

Pinned donor `opencode` v2.0.12 / `2670273ff17da96f85c5826ced57aa1b368754fa`:
`packages/ai/src/protocols/open-responses.ts` ~1238–1281 retains finished function
calls from done items; ~1324–1368 uses terminal output for recovery and finishes
pending state rather than erasing already finished calls on output:[]. Native must
retain its stronger closed-response-before-tool guarantee, not copy donor overlap.

Closest native tests: `provider/tests/typed_failures.rs` ~481–559 protect length
completion, missing/conflicting prior done evidence and opaque status rejection.
They do not establish normal successful done-call + empty terminal compatibility.
CRLF/byte-fragment support is separately covered; no evidence implicates framing.

## Actual existing release differential (offline, real application runtime)

ELF: target/release/oc, size 11359376, mtime_ns 1790752353095659019.
Inode/size/mtime stable across checks. It was not rebuilt amid active T50 writes;
this is tested-ELF metadata, not a claim that HEAD alone reproduces the build.

Fake Responses served synthetic apply_patch creating probe.txt containing hello\n,
then read, then final response. Fresh isolated HOME/XDG/project/native database,
explicit test loopback opt-in, synthetic auth, no real API, no donor/user DB reset.
Title generation served locally and counted independently. Follow-up requests
were checked for real function_call_output with call_id, not ordinary text.

| Terminal first call variant | Exit | Main POSTs | Local title POSTs | Durable tools | Independently verified file | Result |
| --- | --- | --- | --- | --- | --- | --- |
| output:[] | 130 | 1 | 1 | 0 | absent | incomplete HTTP200 and retry notice, SIGINT before second main dispatch |
| full output:[completed call] | 0 | 3 | 1 | 2 | exact hello\n | apply_patch/read/final success |
| output field omitted | 0 | 3 | 1 | 2 | exact hello\n | apply_patch/read/final success |

Sparse case cancellation is deliberate bounded diagnosis, not exhaustion or
success. This minimal change of terminal shape isolates the defect from tools,
permissions, auth, proxy network and runtime command schemas. A secondary synthetic
reasoning-in_progress case also failed before tools, but was not a real incident
wire and is not the fix's causal evidence; do not turn that hypothesis into a
blanket item-status bypass.

Durable normalized fixture: `evidence/T55/sparse-completed.fixture.json`.
It replaces all actual IDs/arguments/text with synthetic values, condenses 22
deltas to one and omits irrelevant lifecycle events; no raw live payload was copied.
Offline reproducer: `python3 evidence/T55/native_sparse_completion.py`.
It asserts this **pre-fix** differential; future regression gates must assert fixed
behavior in their owner tests, not preserve sparse failure as desirable behavior.

## Campaign continuity and actually executed checks

Existing durable campaign nonce: `39d44cb58b834de99544daf3c2eedab1`.
Private local identity file:
`/home/opencode/.cache/opencode-tmp/opencode/responses-diagnose-20260930.campaign-id`.
Private ledger root:
`/home/opencode/.cache/opencode-tmp/opencode/responses-diagnose-20260930-campaign`.
These contain envelope identity/counts, not API credentials or raw bodies.
Resume the same identity; all reservations, including 400/uncertain, remain spent.
Counts after inspection: **4/24 generation**, **5 control**, **0/4 MCP**,
total generation input **2720 bytes**, all nine HTTP receipts drained/complete.
There are 20 remaining generation requests, not a new allowance of 24.

Executed diagnostics (all exit 0 unless the case table specifies child exit):

- `target/release/oc --help` (identity/preflight only, not E2E).
- `python3 scripts/progress.py check` before plan registration.
- `python3 scripts/test_bounded_live.py`: 13 offline tests PASS (5.829 s).
- Temporary private driver `responses-diagnose-20260930.py` actions preflight,
  native-wire, incident-wire, sparse-confirm, inspect: bounded direct observations
  and durable receipts above. Initial non-native forced-choice probe was HTTP400;
  diagnostic variant nesting error was corrected without another blind request.
- Temporary private driver `responses-native-probe-20260930.py`: three primary
  actual-ELF cases above plus exploratory reasoning case; final assertions PASS.
  Early harness assumptions about no retry/title role were disproved and corrected
  (title uses developer instruction). They were local fake-server experiments.
- `python3 evidence/T55/native_sparse_completion.py`: durable normalized fixture
  and three primary existing-ELF baseline cases PASS with the same binary metadata
  and the table's exact main/title/tool/file results. No external requests.
- `python3 scripts/progress.py reindex`, `python3 scripts/progress.py check`,
  `python3 scripts/check_docs.py`: PASS after registration (55 tasks, 163
  acceptance specifications). Separate graph/fixture/Python syntax assertions
  PASS: acyclic dependencies, unique PROV09/10 owner, T55 todo, every pre-existing
  task state unchanged, T50 current and T44 still PAUSED.
- `git diff --check` and unmerged-index inspection: no whitespace/conflict errors.
  Verified current branch agent/oc-rust-port and authorized origin 0FL01/oc;
  remote branch was at inspected HEAD before the plan commit. Only owned plan/
  diagnostic files are intended for this commit, not dirty T50 code.

**Not executed:** production fix, post-fix rebuilt qualification, real native
apply_patch/read against API, seeded Rust coding E2E02, visual T44 qualification.
Direct probes did not execute model tool requests. No workspace cargo rebuild/test
was attempted concurrently with the active writer. Document validation and these
diagnostic PASS results cannot establish application readiness.

## Plan gap and assignment

T12/T13 historical Responses/tool PASS, T16 historic live wire, T54 offline retry
PASS and T53 future protocols did not assign this current compatibility repair.
T27 retains real seeded coding but its limited harness correction scope is not a
general provider bug owner; old envelope blocker must be revalidated, not assumed.

Registered **T55 todo**: PROV09 reconciliation/diagnostics/native regression and
PROV10 bounded real native tool E2E. Frozen plan:
`docs/goals/2026-09-30-responses-tool-compatibility.md`.
T27 depends on T55 and retains E2E02. Active T50 and PAUSED T44 are unchanged.
Next safe handoff prioritizes this demonstrated defect before optional next
provider/profile work. No historical evidence rewritten and no fix/PASS claimed.
