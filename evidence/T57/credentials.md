# T57 — shared credential metadata and guarded native refresh

Date: 2026-10-06. Base `0879e199b` (T56 closed), branch `agent/oc-rust-port`.
T57 active; partial R3/AUTH03 and source/key resolution proof, not full auth PASS.

## Source and scope

Pinned OC2 `2670273ff17da96f85c5826ced57aa1b368754fa` OpenAI plugin
`openai.ts:323–417`: public client, token forms, method IDs, default lifetime,
ID-token → access-token and direct → nested → first-organization claim order.
Structured JWT metadata is routing information from the trusted exchange, not
signature verification or a new grant. Donor resolver concurrency/fallback is not
copied: the frozen native contract requires stronger stale-completion/no-fallback.
Browser/device/CLI/WS source seams were read-only scoped; donor files unmodified.

`storage_credentials.rs` extends the same protected SQLite account table/tagged
material. Legacy JSON without methodID/metadata remains readable and unsupported
for execution. New accountID metadata is validated/bounded and internal; public
summaries add only methodID, not token bodies, previews/lengths or routing account ID.
Native expires_at uses Unix seconds. Existing Key and namespace/newest-selection
contracts remain; same-active activation is an acknowledged identity-preserving no-op.

Migration13 adds selection_revision, material_revision and refresh_pending only
to the existing table. Selection epoch detects A→B→A; material version fences a
rotating token. Refresh reserves durably before sending, then atomically updates
the exact still-active account/epoch/version and clears the reservation. Cancellation,
failed/unknown remote outcome or failed CAS cannot silently replay refresh, restore
removed/switched rows or create another account; reauthentication is explicit.
Exact migration tests now include required13, retaining all previous assertions.

`auth/openai.rs` uses the pinned issuer, redirect refusal, zero HTTP retries,
30-second total/10-second connect bounds and 64-KiB token-body cap. Form encoding
uses the existing URL serializer, no dependency. Shared Db handles carry one async
refresh lock; resolution rechecks identity before and after exchange, rotates refresh
tokens and preserves old routing metadata only when fresh claims yield none.
Active stored Key/OAuth wins over own OPENAI_API_KEY then configured Key. Unknown
method/failed refresh never reads fallback env. Captures partition stored account,
Key/OAuth/method/account routing identity and key-source fingerprints without storing
raw env/config keys; a later switch cannot mutate already returned material.

This owner is not yet wired into OpenAI transport/connect/CLI. Captured OAuth stays
unready in the legacy transport until R4 admission; no storage-only execution claim.

## Checks actually run

Approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2, sequential Cargo/default stacks:

- `cargo test --locked -p oc-adapters --lib auth03_`: **5 passed / 0 failed**.
  Exact token/claim/default/redaction; one real-loopback refresh for two concurrent
  shared-handle calls; rotated reopen; held switch/ABA/removal barriers; revoked,
  unknown and cancelled refresh with no repeated physical request; legacy upgrade,
  rename during refresh, idempotent activation/CAS and pending restart.
- `cargo test --locked -p oc-adapters --lib auth05_`: **1 / 0**. Stored/env/config
  precedence, lazy no-fallback, source partition and immutable Key/OAuth captures.
- Existing `go01_`: **13 / 0**. Authority/scoped custom+Go precedence, anonymous,
  unsupported foreign OAuth, protected files/symlink refusal/rollback regressions.
- Existing `child_schema_`: **3 / 0**. Exact current upgrade/reopen/migration rollback.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: exit0.
- `cargo fmt --all -- --check`, `git diff --check`: exit0.

Initial compile found disabled reqwest form feature, an exhaustive source match,
and missing reservation fields after duplicate same-file patch sections overwrote
an earlier update. Corrected the actual owner with one section per file and existing
serializer; no validation weakening, dependencies, test disabling or timeout raises.
All fake issuer/token/claim values are synthetic. No real OAuth/login/generation,
browser launch, live env or runner/user auth read. `.opencode/` unread/unstaged.

## Next / boundaries

Implement cancellable browser/device attempts, then built-in Key/OAuth catalog and
WS/HTTP request bindings, followed by shared CLI/TUI actual-binary consumers. AUTH06
dedicated owner-operated live prerequisites and T44/VIS45 are not satisfied by this
slice. T44 remains PAUSED; T53 ledger and other task owners unchanged.
