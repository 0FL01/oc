# T53 typed account owner — partial GO01 / GO05

Base: `69e2e01f0`. This slice does not qualify the pending masked TUI, cross-provider
selection, complete GO05 PTY or bounded GO06 live.

## Implemented

- `CoreApp::provider_accounts` reads and mutates through the existing single native
  owner; no extra store, auth file, recovery or paid request.
- Safe DTO: id/label/kind/active/time and effective source only. `KeyInput` is not
  serializable; Debug is always redacted. No endpoint namespace or token preview.
- Add/activate/rename/confirmed-remove use existing transaction owner. Unconfirmed
  removal and foreign account IDs refuse. Go scope is the admitted fixed preset;
  custom scopes require their admitted endpoint/provenance.
- Idle refresh retains original configured auth inputs rather than treating a
  previously resolved stored key as configuration. Last-row removal therefore
  falls back correctly instead of reusing a deleted key. Catalog facts stay separate.
- Credential-only runtime publication updates redactions/captured binding facts,
  not MCP, policy, selection generation or composer. Existing prepared requests are
  immutable; busy mutation refuses before DB writes and metadata queries still work.

## Verification

With disk TMPDIR `/home/opencode/.cache/opencode-tmp/opencode`, Cargo jobs 3 and
test threads 2:

```text
cargo test --locked -p oc-adapters --lib go05_               4 PASS
cargo test --locked -p oc-adapters --lib tool12_busy_owner_commit 1 PASS
cargo test --locked -p oc-adapters --lib -p oc-core          612 + 32 PASS
cargo clippy --locked --workspace --all-targets -- -D warnings PASS
cargo fmt --all -- --check                                  PASS
git diff --check                                           PASS
```

Owner tests cover configless add without auto-selection, explicit choice/readiness,
two accounts, activate/rename/confirmed-remove/newest promotion, restart, last-key
removal with zero accepted roots, custom scope, source-safe Debug, and SQLite
trigger-induced rollback without leaking error/key canaries. Held main requests
refuse account mutation but continue the unchanged three-request switch/tool flow.

## Experiments

Initial compilation caught the immutable query projection boundary. Account mutations
now return to the supervisor that owns mutable Composition; streaming queries remain
read-only. Initial readiness assertion incorrectly queried the global fallback after
a Home-only selection; it now queries the existing Home selection owner, preserving
separate scoped drafts. No product invariant or test was disabled to pass.

No live generation, secret-file reads, new dependencies or ignored-test changes.
