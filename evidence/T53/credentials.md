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
