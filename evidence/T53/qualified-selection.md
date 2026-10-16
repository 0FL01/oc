# T53 — qualified executable selection slice

Base: `cee7dc5bc`. Partial GO05/GO04 qualification, not full T53 completion.

## Implemented

- Selection preferences now retain provider, catalog ID and variant. Missing provider
  on legacy scoped records binds only to the original namespace. Provider-only changes
  are not no-ops; same slash-containing IDs do not alias catalogs or remembered variants.
- Home and session commits validate captured Location/generation/agent and the admitted
  target catalog before atomic persistence. Browsing and account ACK remain non-committing;
  the picker needs the existing explicit blank-Enter commit action.
- Composition captures a finite provider map with independent catalog/readiness/config
  leaves, no nested maps, storage owner or MCP/policy publication. Secret-bearing captures
  have redacted Debug and are included in redaction, not ambient credential inheritance.
- Each prepared primary attempt reads one qualified committed choice and uses its captured
  catalog/config for tools, budgets, history, DCP/compaction, child issuance, receipts,
  retries and dispatch. A commit cannot mutate an already prepared request or issued batch.
- Primary, profile, title and manual compaction admission consume the selected connection;
  title pins resolve admitted qualified views. Session-move admission uses its destination
  selected variant, rather than the configured root connection.

## Evidence

- Actual application fake HTTP: Responses(alpha) → Messages(beta) → Responses(alpha),
  with the same catalog ID `same/slash`, distinct API model IDs/routes/keys and held
  requests. Two real read outcomes execute once and survive the switches; alien encrypted
  reasoning/signatures are withheld. Durable receipts retain all three producing providers.
- Home and session beta selections survive restart independently. Actual binary PTY adds
  a Go account from a custom connection, dismisses the filtered picker without selection,
  then explicitly commits Go and verifies the selected connection after restart. No
  generation HTTP is sent by this connect/selection test; keys never appear in output.
- Existing exact preference DTO fixtures were extended with their expected provider;
  rollback, restart, busy-commit and discovery assertions were not relaxed.

## Checks and significant attempts

Commands use `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, Cargo jobs 3 and
test threads 2. No test-stack override, timeout increase, ignore or error suppression.

- Targeted qualified busy switch: PASS; extended actual account PTY: PASS.
- `cargo test --locked --workspace`: PASS **1644 passed / 0 failed / 10 ignored**.
  The ten pre-existing opt-in live/internal ignores are not live qualification.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `cargo build --locked`, `target/debug/oc --help`,
  `git diff --check`: PASS.
- Broad gates exposed default-stack overflows in real child launch and a direct runtime
  approval test. GDB located the oversized future transfers/poll chain. Heap-pinned child
  spawn and admitted-turn futures plus heap-owned prepared parameters fixed both unchanged
  tests and the full workspace; stack limits were not increased.
- The first account PTY draft selection did not commit: the required explicit commit was
  added to the test, not replaced with automatic selection in production.

## Remaining

Audit qualified fork/child/pinned-reference edge cases and the full frozen GO01–GO05
acceptance matrix, then bounded real Go qualification and final report/finish. This slice
does not claim those checks, T44 pixel parity, Codex OAuth or T57 completion.

## Qualified fork and child follow-up

Base: `b4b9266d5`. The application supplies the frozen admitted provider set to
the existing atomic fork transaction. Root journal, request/span receipts and
accepted models must belong to that set; call/result completeness, settlement,
Location, quotas and rollback checks are unchanged. The single-provider test
entry still rejects foreign authority before inserting a fork. Mixed Alpha /
Beta / Alpha receipts and two settled reads survive application fork/reopen,
with the exact qualified Beta choice and original binding receipts.

Child profile pins, explicit overrides and native background command models now
resolve their own catalog/config from the issuer's immutable admitted map. Tool
preview, preflight, quoted-context admission, Jobs launch, stored model and wire
receipt use the same target. Captured maps have finite leaf configs; copying the
map onto a child does not introduce recursive cycles or inherit a parent's key.
Actual fake Responses parent / Messages child tests cover all three routes and
assert independent endpoint/API model/auth plus durable child provenance.

Checks (approved disk TMPDIR, jobs=3, test threads=2):

- `cargo test --locked -p oc-adapters --lib --test runtime --test subagent`:
  unit **616/0/0**, runtime **120/0/0**; first subagent run **37/1/0** exposed
  a changed unknown-provider diagnostic, not an admission/effect failure.
- Restored the existing actionable model-unavailable error contract; reran
  `cargo test --locked -p oc-adapters --test subagent`: **38/0/0**.
- Strict workspace all-target clippy, workspace fmt check and diff check: PASS.

No real generation, new dependency, changed ignore, raised timeout, or secret
output. Remaining GO05 qualification: complete actual-binary generation/cancel
after the account/filtered-picker flow; GO06 bounded real Go and final report.
