# T57 — per-request OpenAI credential preparation (partial R3/R4)

Date: 2026-10-06. Base: `7410b2ede`. This checked slice prepares each native
OpenAI request from the shared credential owner; it is not WS/default-transport,
rebuilt login/model roundtrip or real authorization proof. R1–R4 remain in progress;
R5/R6 and independent T44/VIS45 remain unresolved.

## One preparation boundary, immutable issued request

`auth/openai.rs::prepare_request` resolves only the exact selected native OpenAI
leaf using its original admitted auth inputs and captured environment. It uses
the existing five-minute refresh, shared flight, account/version CAS and durable
unknown-refresh reservation. A prepared clone keeps its bearer/account, route,
session context and provenance; later rotation/activation changes only a later
preparation. Other model/provider leaves are not opportunistically resolved.

Foreign endpoints named OpenAI and other providers retain their existing captures,
source priority and cancellation contract. There is no fresh process-env read,
credential/model/route fallback, extra registry, scheduler or provider trait.
Failed/unknown refresh cannot become an environment/config Key request. Explicit
reauthentication and unavailable subscription model use the existing diagnostic
shape with distinct actions; storage/cancel remain their existing error classes.

The same pinned subscription API-ID/pro-mode gate is checked before dispatch.
Alias/variant selection stays captured; eligible subscription selection receives
the same empty cost/context400000/input272000 overlay as the catalog, preserving
output limits. A Key-era leaf cannot bypass that filter after account activation.

## Owning lanes and boundaries

- `runtime/turn.rs`: exact model/variant validation precedes auth preparation and
  initial durable acceptance. Each main/follow-up/runtime-retry/child iteration
  prepares before provenance, checkpoint validation, input projection, tools and
  physical dispatch. Cancellation during a step rejoins the existing durable
  cancellation path; issued work and unrelated captured leaves remain frozen.
- `runtime_compaction.rs`: preparation precedes summary history/binding/budget;
  each ordinary summary retry prepares again. If account/route authority changes
  after the transcript was projected, the summary refuses rather than replaying
  alien opaque/checkpoint state or publishing a falsely bound summary. Rotation
  on the same account preserves authority. Existing registered native-compaction
  capability is unchanged; beta headers do not authorize a new compact feature.
- `application.rs`: explicit and automatic title lanes prepare inside their
  existing ten-second timeout with captured Db/environment/provider/session.
  Existing joined/aborted ownership drops unknown refresh futures safely.
- `provider.rs`: native credential/account/actor headers are re-applied after
  profile overlays. Key strips injected Codex-only headers; ordinary custom
  headers and existing Go overlay behavior remain intact.

## Checked evidence

Two new `auth/openai/binding_tests.rs` owning scenarios use real SQLite captures
and a bounded fake issuer: concurrent prepared lanes perform one physical refresh,
new selected leaf sees rotation, issued headers and unselected leaf do not mutate,
authority stays stable, later Key restores normal route/headers, subscription
API aliases/pro-mode/limits and safe diagnostic actions are enforced. Non-native
and foreign captures are unchanged, including pre-cancel behavior.

`provider/tests/openai.rs` adds real local TCP HTTP/SSE captures for OAuth/Key
native-marker fixtures: profile attempts cannot replace captured bearer/account/
actor or inject Codex headers into Key; unrelated admitted custom headers survive.
These are two fake physical requests, not real Codex/OpenAI or WS evidence.

Current sequential checks with approved TMPDIR, CARGO_BUILD_JOBS=3 and
RUST_TEST_THREADS=2, normal stacks:

- `cargo test --locked -p oc-adapters --lib auth04_`: **8 passed / 0 failed**.
- `cargo test --locked -p oc-adapters --lib`: **652 passed / 0 failed / 1 unchanged
  Go opt-in ignored**, 653 tests; 103.53s. Includes prior runtime/compaction/title/
  child/PTY/Go/account/schema/security/DCP/retry regressions. Local output:
  `/home/opencode/.local/share/opencode/tool-output/tool_111e889f9001qrH7AtHBNT9Agn`.
- `cargo test --locked -p oc-core --lib`: **32 / 0**.
- `cargo test --locked -p oc-tui --lib`: **444 / 0**.
- After readability-only title formatting, the owning effective-wire/title test
  was rechecked: **1 / 0**; no behavior changed.
- Strict workspace all-target Clippy, fmt/diff and journal checks pass.

Initial broad test exposed a real regression: a global pre-cancel check changed
non-native compaction's established cancelled-report contract to an early error.
The check now follows exact native provider/authority admission; non-native/foreign
preparation is a no-op clone. The original failing assertion was not changed;
targeted and full suites then passed. No timeout/stack override, dependency,
baseline rewrite, validation weakening, failing-test disable or live call was used.

## Next / scope

Continue native Responses WS/HTTP, 55-minute session affinity, fully-consumed
checkpoint publication, provably same-route fallback and no ambiguous replay,
including the restricted-provider-error consumer amendment. Then shared CLI/TUI
actual-binary consumers and AUTH06. Dedicated owner-operated browser/device
confirmation and explicit OpenAI test Key are not supplied; no runner/user/donor
credentials were read. `.opencode/`, T53 campaign, T53/T56 closure and PAUSED T44
are unchanged; this is neither task completion nor visual/product READY.
