# T53 — native Messages first slice (2026-10-05)

Base: `7e5ca8231`. Partial R1/R3 implementation, **not GO01/GO03 or full T53 PASS**.

## Delivered behavior

- Both pinned Anthropic package aliases explicitly select native Messages; no
  route probe, model-name default, SDK dependency or protocol fallback.
- Exact configured prefix + `/messages`; static apiKey → only x-api-key,
  static authToken → only Bearer. Both together, non-Messages authToken and
  competing auth headers are configuration errors. ProviderOptions Debug masks
  connection/credential values, including headers.
- Version 2023-06-01 and merged interleaved-thinking beta; profile overlay cannot
  remove the required beta/version. Initial system blocks, escaped chronological
  fallback, text/image input, complete tools and linked text/image results.
- Separate Messages state machine over existing bounded byte/SSE transport:
  indexed text/thinking/signature/redacted/tool blocks, fragmented JSON,
  block closure/identity validation and genuine message_delta + message_stop.
  Refusal/unknown terminals and malformed/EOF tools never authorize execution.
  max_tokens finish remains explicit Length. Usage includes reported cache
  read/write input counts, merges delta usage over start, output emitted once.
- Common completed output retains Messages-origin signed/redacted thinking;
  alien Responses/Chat state is not reinterpreted. Unsigned thinking lowers as
  text, matching the pinned donor signature policy.
- HTTP/SSE typed errors, cancellation and dispatch accounting share the existing
  one-physical-attempt owner; no per-wire retry loop.

## Tests and experiments

`provider/messages_tests.rs`: split CRLF/UTF-8 stream (chunks 1/7/4096), common
tool/reasoning replay, cached usage, image lowering/unsupported modality, seven
malformed/incomplete terminal mutations, both auth schemes, safe Debug and auth
conflicts. Actual sockets qualify one 429 with Retry-After, post-reasoning EOF
with output_committed and held-body cancellation, each exactly one counted attempt.

`application/messages_wire_tests.rs`: each static auth scheme passes actual
config → application → read note → Messages follow-up → persisted final answer;
captured history contains signed/redacted thinking and exact linked tool result.

First full crate run: 562 passed / 2 failed. Diagnosis:
- Direct Chat helper test passes a default Responses config; selecting its route
  from config alone incorrectly changed this pre-existing helper contract.
  Restored the explicit Chat argument's route/parser authority; regression passes.
- Old unknown-option fixture used authToken, now intentionally recognized and
  conflict-validated. Replaced only that formerly unknown field with unrelated
  vendorExtension; retained the warning assertion. Added real authToken conflict
  tests rather than allowing an invalid executable credential option.

Final commands (disk TMPDIR, jobs=3, test threads=2):
- `cargo test --locked -p oc-adapters --lib`: **564 passed / 0 failed / 0 ignored**.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS.

No live requests, secrets, new dependencies, test deadline changes or ignored
tests. Full workspace/final Go live remains GO06 work. Credential storage/policy,
scoped local/LAN admission, per-model options/chronology/cache, durable full
protocol/binding guards, Go catalog/metadata and connect remain pending.
