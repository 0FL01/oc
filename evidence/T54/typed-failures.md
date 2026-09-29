# T54 R1 — first atomic physical-request contract

## Coordinator review and delivery checkpoint

Reviewed the one-attempt adapter, structured failure/header boundary, runtime
compatibility, source-completion correction, existing envelope and native fixture.
The independent review reproduced terminal-only length tool admission and overly
broad opaque-item validation; the corrected RED/GREEN evidence is preserved below.
After ownership returned, coordinator repeated the 25 provider cases and the
runtime effect-counter case (both exit 0), normal locked build, then the 18-case
debug ELF check without intervening Cargo: 36 physical POSTs (18 main, 18 title),
zero tool intents/effect files and held-body cancellation socket close. Its SHA256
is the final debug value recorded below. Fmt, docs/progress and diff checks passed.
These are current R1 physical facts, not R2/R3/R4, VIS43 or product READY.

Frozen before RED against source HEAD `f43ebbe4165dd125e43914c8fba21e03329fce4f`;
donor local HEAD `2670273ff17da96f85c5826ced57aa1b368754fa`.
Read the complete 195-line T54 spec, current CONTRACTS Errors and
PROV06/AUD11/AUD13 acceptance, and donor `packages/ai/src/provider-error.ts`,
`route/{executor,client}.ts`, `protocols/open-responses.ts`.

## Frozen obligations (spec R1 / decision table 96–129)

1. Each adapter call issues exactly one physical POST. No retry or delay loop.
2. Bounded structured HTTP and Responses failed/error JSON classification:
   client-scoped context overflow → payload/413 → policy → quota/402 → auth
   → throttle/429 → internal/408/409/5xx → invalid/remaining 4xx → provider unknown.
   Only provider failure envelopes enter that classifier; generated text and
   local trust/policy/validation/storage/parser/cap failures cannot become unknown.
3. Keep observed HTTP status distinct from event status, delivery and request/read
   operation. Preserve output-committed facts for R2 without implementing R2.
4. Retain only bounded safe header facts: exact observed x-should-retry true/false;
   finite nonnegative retry-after-ms first, then seconds/HTTP-date Retry-After.
   Clamp delay to 900000 ms, only rate/internal; HTTP-200 SSE has no header delay.
   Carry terminal auth/quota default and override facts; adapter never retries.
5. max_output_tokens incomplete is truthful length, content_filter is failure,
   other incomplete/EOF is non-success. Incomplete arguments never admit tools.
6. Cancellation closes held header/body requests; deadlines, network/resource,
   SSE/frame/request bounds, private-host refusal and effect quarantine remain.
   Diagnostics never contain raw body/auth/environment/URL; structured invalid
   request details reuse bounded credential redaction.
7. Text/tool arguments/usage/reasoning/images/MCP media and request model, headers,
   generation/binding are preserved. Existing callers only gain typed compatibility.

## Executed qualification — R1 physical facts only

Mandatory RED→GREEN socket fixtures: throttle/quota 429, 402, 408/409/5xx,
400/401/403/413; precedence and client-only overflow; HTTP-200 typed events;
headers/delay bounds; length/filter/unknown incomplete and partial EOF; local
parser/caps/cancel refusal. Affected provider/runtime/permission/media/headless
regressions and full requested workspace/Python/docs checks follow.

R2 finite policy/backoff/durable continuation, R3 span persistence/UI, R4 physical
operation accounting and VIS43 remain unqualified by this atomic R1 slice.
No live requests, user-config inspection, commits, staging or progress mutation.

### Obligation matrix

| Frozen obligation | Implementation / observed proof | Qualification |
| --- | --- | --- |
| One physical attempt | Removed the adapter's two-attempt loop; explicitly disabled reqwest's built-in retry. `ret01_one_physical_attempt_throttle_and_quota` RED observed 2 POSTs vs expected 1; GREEN plus HTTP/SSE fixtures observe 1 per adapter call. Native counts distinguish main/title lanes. | R1 verified |
| Structured classifier / precedence | Private `provider/failure.rs`, donor code paths/allowlists and exact bounded error-message patterns. HTTP matrix includes 429 throttle/quota, 402, 408/409/500/503, 400/401/403/413/422; policy/quota/auth precedence; absent/4xx-only context overflow; generated text excluded. | R1 verified |
| HTTP/event/delivery/read facts | HTTP-200 failed/error events retain HTTP 200 separately from numeric event status. Request-connect failure is NotSent; SSE read is Accepted/Read; semantic output-committed excludes usage/boundary-only frames. Read timeout after a delta carries continuation eligibility facts even with observed false. | R1 verified; dispatch/continuation pending R2 |
| Safe observed headers / delays | Only exact parsed true/false; header text cap 128 bytes; finite nonnegative ms first, seconds/date fallback; cap 900000 ms; delay retained only HTTP rate/internal, never SSE HTTP 200. Tests cover malformed/oversized/negative/nonfinite fields and HTTP date forms. | R1 verified; waiting/jitter/budget pending R2 |
| Terminal semantics / tools | Length retains text/usage and only tools with actual prior validated output_item.done evidence: matching nonempty item ID and identical body. Terminal-only lookalikes, changed bodies and unresolved announced tools remain non-success. Only partial assistant messages with in_progress/incomplete status bypass ordinary item-status validation; opaque/failed items do not. Runtime persists JSON display.finish_reason=length. Native length returns 0; terminal-only valid tool returns 1 with zero intents/effects. | R1 verified after independent-review correction; no new persistence schema/span |
| Cancellation / caps / safe diagnostics | Existing private-target/header/caps/quarantine tests remain enabled. Error body decoder bounded to 16 KiB and absolute chunk deadline. Held 401 body SIGINT native exit 130 and observed socket close. Invalid local JSON/UTF-8/options/caps never inherit provider overrides. Canary checks omit auth/raw body/environment/URL. | R1 verified |
| Existing lanes / request binding | Existing workspace Responses, permission, media, native headless, child/title/compaction and MCP suites pass. No crates/deps/toolchain/model-ID registry/schema/UI geometry/new retry owner added. Existing limits unchanged: SSE/frame 2 MiB, arguments 1 MiB, request/generation 32 MiB, 10000 events, text delta 16 KiB; upstream input/media bounds retained. | Regression verified; full R4 accounting pending |

### Commands and actual exits

Cargo environment for every build/test/clippy: `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`,
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
All Cargo invocations serial, locked; tool timeout 900000 ms.

| Command / final run | Exit / result |
| --- | --- |
| `cargo test -p oc-adapters --lib --locked provider::` | 0; 25 passed after review correction |
| `cargo test -p oc-adapters --test runtime --locked aud11_review_length_terminal_only_call_has_no_effect_or_intent` | 0; 1 passed, negative and prior-done positive effect-counter cases (current full runtime 96/96) |
| `cargo test -p oc --test live_bounded --locked bounded_native_restart_and_helper_restart_share_real_attempts -- --nocapture` | 0; first=2 combined=3 physical POSTs over two native/helper processes; one main per explicit turn, one title initially |
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --locked --no-fail-fast` | 0; 1309 passed, 10 preexisting ignored, zero failed; 39 test targets plus three zero-case doc-test targets; adapter unit 360, runtime 96 |
| `cargo build --locked` / `cargo build --release --locked` | 0 / 0; final normal builds before direct native proof |
| `TMPDIR=... python3 -B -m unittest discover -s scripts -p 'test_*.py'` | 0; four suites / 47 tests |
| `python3 -B scripts/check_docs.py` / `python3 -B scripts/progress.py check` | 0 / 0; structure only, no acceptance/status mutation |
| `python3 -B scripts/code_size.py` | 0; advisory 283 files / 189028 physical lines / 7586942 UTF-8 bytes |
| `git diff --check` | 0 |
| `python3 -B evidence/T54/native_typed.py target/debug/oc` / `... target/release/oc` | 0 / 0 after review correction; each 18 cases, 36 actual POSTs = 18 main + 18 separate title; zero tool intents and zero effect files in all cases |
| `target/{debug,release}/oc --help` / `... --smoke` | All 0; linked four-crate native application |

Final full Cargo/build log: private harness output
`tool_0ef8b5c99001xPeElRRR25O59E` (small text; no raw provider bodies).
No Cargo ran between the final normal release build and direct ELF checks.

### Actual native ELF identity and safe outcomes

Source HEAD remains `f43ebbe4165dd125e43914c8fba21e03329fce4f`, **dirty** R1
source/test diff plus inherited parent progress-start diff; hashes identify the
actual executable bytes, not a committed revision:

- Debug ELF SHA256: `121c83bdd21d68401b9733109d26515fa7f9f759ea2452eb5a3f432e91a7754d`.
- Release ELF SHA256: `5ac5a5f943dde495de199dc718448f2aea477ff042b9040df45f9d565c16d76d`.

Both binaries: HTTP throttle/quota/payment/408/409/503/client-400/auth/forbidden/
payload/policy precedence and SSE failed/filter/partial EOF/incomplete-tool/
malformed SSE return exit 1 with safe classified diagnostic; length returns 0
with durable length fact; held error-body cancellation returns 130 and closes
the socket. All 18 cases have exactly one main POST and one separate automatic
title POST, with no tool intents/effect files and no synthetic secret/body/URL
canary output. The current native incomplete-tool case supplies a valid,
completed-looking terminal-only shell call with a real fixture effect command;
the prior-done event is deliberately absent. The current length case includes
a genuinely partial assistant message and usage in the terminal output.
These are offline socket observations, not full R4 counter enforcement.

### Diagnosed failed approaches (not hidden)

- Intended RED: throttle physical request count was 2, expected 1 (exit 101).
- Initial fixture compile errors used the wrong existing `sse_completed`
  arity and timeout field; corrected to the existing helpers/config contract.
- Initial provider rerun had six obsolete bare-enum/two-attempt expectations;
  changed only typed result / one-attempt assertions. No security test disabled.
- Runtime AUD11 expected max_output_tokens to be incomplete; corrected only
  the source-authorized length case, added filter/unknown preservation and the
  durable length assertion. Initial 94/95, corrected intermediate 95/95;
  current suite after review additions 96/96.
- First full workspace run failed only the old envelope's automatic retry
  success expectation; replaced with observed main/title ordering and exact
  one-attempt-per-lane accounting. Full final workspace passes.
- Initial native fixture assumed title.disable suppressed the fresh CLI's
  separate automatic-title lane and observed 2 POSTs. The fixture now records
  real main/title counts independently; it does not call that second lane an
  adapter retry. Existing title scheduling was not modified in this R1 slice.

### Independent-review correction — source completion, not terminal recovery

Parent review caught two real defects in the first R1 implementation and its
overbroad positive fixture. Donor `open-responses.ts:1324–1369` confirms that
terminal-output tool recovery and finishing pending tools occur **only** inside
the `response.completed` branch (1327–1353), never on `response.incomplete`.
The earlier terminal-only length tool success assertion was wrong and is
superseded by the reviewed completion-evidence contract below.

RED before the production correction:

- `cargo test -p oc-adapters --lib --locked ret01_review_length`: exit 101,
  both review scenarios failed: terminal-only completed-looking function call
  succeeded; length also accepted a failed opaque compaction item.
- `cargo test -p oc-adapters --test runtime --locked aud11_review_length_terminal_only_call_has_no_effect_or_intent`:
  exit 101; observed `(durable intents, shell effect exists) = (1, true)`,
  expected `(0, false)`. This was an actual isolated runtime effect, not a
  fabricated parser assertion.
- A strengthened positive source-completion scenario also RED-failed (exit
  101): announced call followed by valid done was incorrectly rejected when
  the length terminal array contained only partial text. This was corrected
  before the final full qualification, not hidden by requiring final recovery.

Corrected handling stays in `SseParser::complete_response` / existing
`output_done` owner, with no new retained body map or runtime dispatcher:

1. `output_item.done` still passes ordinary `validate_output` before becoming
   completion evidence. Failed/in-progress opaque done events cannot qualify.
2. On a length finish, a terminal function call must pass ordinary validation
   **and** match a prior validated done function call by nonempty item ID and
   identical JSON item body. Terminal-only, announced-without-done, anonymous
   or changed-body lookalikes fail; complete-call plus dangling announced-call
   also fails rather than silently filtering the partial graph.
3. A genuinely earlier completed, validated, matching call remains admissible.
   The corrected old positive fixture now emits actual output_item.done.
   Already emitted done calls are preserved at their observed output positions
   if the incomplete terminal array omits them; this uses only actual prior
   done bodies, never a terminal-only call. Unresolved announced IDs still fail.
   The runtime counter's positive case records one intent and exactly one
   `effect` append even when terminal output has only a partial message; the
   negative records zero intents and no effect file.
4. Only `type=message`, `role=assistant`, `status=in_progress|incomplete` gets
   the truthful length partial-message exception. Every other item uses the
   old validation, including failed messages and compaction/reasoning/future
   opaque statuses. No broad success reinterpretation of item.done status.

GREEN provider matrix covers terminal-only/prior-done/body mismatch/missing
ID/dangling call and opaque terminal/done validation, plus valid partial
assistant messages. Full fmt/strict Clippy/workspace tests/normal debug+release
builds/Python47/docs checks were rerun. Both final existing ELFs passed the
updated 18-case socket/SQLite/effect checker directly after the normal builds,
without intervening Cargo. R1 classification/header/one-attempt facts remain
qualified; this does not implement or qualify R2/R3/R4.

### Next minimal R2 seam and ownership handoff

Consume `ProviderError::Request(PhysicalFailure)` at the existing runtime
logical-step call/error boundary. Add the pure finite clock/jitter/cancel-aware
policy and step-bound partial continuation there; persist the runtime-owned
partial assistant/tool-call graph before dispatching an admitted continuation.
Keep the adapter one physical attempt, existing generation/model/header binding,
and separate context-overflow rebuild. No retry dispatch exists in this R1 slice.

Mutation/Cargo/native-fixture ownership returns to the parent after this report.
No stage/commit/progress execution status changes; inherited progress files and
preexisting `.opencode/` retained, `.opencode/` never manually inspected.
