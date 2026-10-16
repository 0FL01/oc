# T53 credential owner / resolver foundation — partial GO01

Date: 2026-10-05. Base: `98298dc92`. No live requests, no foreign store/import.
This slice is not full GO01 PASS: config/application resolver integration, captured
trusted LAN admission and connect/account UI are subsequent required slices.

## Implemented

- Existing `Db` owns additive migration 11 and `credential_accounts`; tagged Key
  and OAuth(access/refresh/expiry), no None rows or config/env copying.
- Immediate transactions + partial unique active namespace index: add/activate/
  rename/remove, newest remaining on active removal, none after last removal.
  Same-second creation order uses SQLite insertion order as deterministic tie-break.
- Metadata-only summaries, redacted material/resolve Debug and credential SQL errors;
  malformed metadata/material is bounded and rejected without echoing input.
- SQLite DB is created/secured 0600 before opening; existing sidecars secured and
  symlinks/foreign owners/non-regular/hardlinked SQLite files refused. Fresh WAL/SHM
  inherit 0600. Existing root exclusivity and 0700 guarantees retained.
- `auth::AuthScope` checks normalized fixed HTTPS Go authority before secret lookup.
  Go stored > lazy Go env > configured; custom configured > exact provider+normalized
  prefix scoped stored, no foreign env read. OAuth is honestly unsupported, no key
  fallback. Anonymous policy rejects configured credential/competing auth headers.
- Resolver result installs captured policy into transport; anonymous emits neither
  empty Bearer nor dummy key, unsupported OAuth never emits a request auth header.
  This seam still needs production composition/config consumers.

## Verification and experiments

- `cargo test --locked -p oc-adapters --lib go01_`: 5/0 after fixing explicit
  `oauth` Serde tag (automatic snake_case acronym had produced `o_auth`).
- Actual transaction fault triggers prove activate/remove/add rollback, unchanged
  active material/account count, autocommit restored, no canary in error Debug.
- Initial full crate test: 566/2; failures were exact migration lists missing newly
  required additive version 11. Updated only those migration expectations; session
  schema/preservation assertions remain unchanged.
- Final `cargo test --locked -p oc-adapters --lib`: **568 passed / 0 failed / 0 ignored**.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS.
- Cargo jobs 3, test threads 2, TMPDIR `/home/opencode/.cache/opencode-tmp/opencode`.

Nearest tests: `storage_credentials/tests.rs` lifecycle/reopen, rollback/uniqueness/
safe errors, auth priority/scope/None/OAuth, and DB/WAL/SHM mode/symlink refusal.

## Application resolver integration follow-up

Date: 2026-10-05. Base: `cfbc50a08`. Partial GO01, no live requests.

- Local `options.authPolicy` accepts `none`, `key` (default), `oauth`. Anonymous
  credential/header conflicts are rejected before substitution can erase an input.
  Go authority is checked before API-key/header substitutions.
- Application startup opens its single Db, resolves scoped credentials, then starts
  discovery/runtime. Deferred discovery captures the same resolved binding. Location
  switch/reload and session move resolve before refreshing/publishing the destination.
- Runtime admission uses captured auth readiness, not unresolved config templates.
  Explicit None completes a real fake-server Responses request without auth headers.
  Unsupported stored/explicit OAuth stays local/unavailable with a safe capability
  cause; submit is refused before any history/turn acceptance, without key fallback.
- Regression tests verify stored account readiness across two restarts, endpoint-prefix
  change loses stored auth, safe catalog projections, None/config conflicts and Go
  foreign-authority rejection. Existing reload/discovery/tool-roundtrip suites stay green.
- `cargo test --locked -p oc-adapters --lib`: **571 passed / 0 failed / 0 ignored**.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS. Same jobs/threads/TMPDIR.
- One test compile correction: typed history role is `Role::Assistant`, not a string.

Remaining: trusted localhost/LAN captured admission shared with discovery; canonical
normalization and model/variant overlays; account owner actions/read DTOs and connect
UI. No full GO01/GO05 PASS claim.

## Captured endpoint follow-up (base `3ea7c9377`)

- Local source admission captures normalized scheme/host/port/prefix plus provenance
  and source trust before credential substitution. Trusted explicit connections allow
  loopback, RFC1918 and IPv6 ULA; untrusted/public-default clients do not. Link-local,
  metadata, multicast, unspecified, CGNAT/reserved and mapped equivalents remain denied.
- Shared `endpoint.rs` validates all DNS answers, pins the admitted answers in reqwest
  before auth transmission, and checks connected peers. No proxy, redirect or hidden
  reqwest retry. Both generation (all wire lanes) and configured discovery use this
  captured authority; endpoint/provenance changes invalidate request-binding equality.
- Explicit None discovery shares its ordinary bounded catalog owner and emits no auth.
  Partial/static catalog config still needs no connection; executable Composition
  admission remains mandatory. Internal capture fields cannot be injected via JSON.
- Actual anonymous application generation now passes **without** the test-loopback env.
  New tests check IP matrix, prefix/origin/provenance isolation, redirected credentials
  never reaching a trap server, and successful anonymous catalog publication.

Experiments: first broad run rejected partial config with absent baseURL before its
existing trust tests; corrected admission to defer absent connections to Composition,
not weaken executable endpoint checks. Discovery fixtures now capture each actual
fixture prefix, including the oversized-body case. Strict all-targets compile exposed
a private field breaking public struct-update syntax; capture metadata uses public
serde-skipped fields without exposing the private authority type. A new anonymous
fixture initially omitted the oracle-required `object:"list"`; corrected the fixture,
not the validation.

Checks (disk TMPDIR, jobs=3, test threads=2): adapter unit suite 573/0/0 before final
anonymous-fixture extension; final endpoint tests 2/0/0, strict adapter all-targets
Clippy, workspace fmt and diff check PASS. Final full-suite result recorded below.
No live requests/new dependencies/secrets/ignored-test changes. Remaining GO01 work:
effective canonical model/variant overlays and account owner controls; GO02–GO06 pending.

Final follow-up gate: `cargo test --locked -p oc-adapters --lib` **573 passed, 0 failed,
0 ignored** (78.95s); final source strict all-targets Clippy/fmt/diff check PASS.
