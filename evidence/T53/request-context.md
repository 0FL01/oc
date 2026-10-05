# T53 — immutable request identity and lineage cache

Base: `4bcdc5713`. Partial R3/GO03, not full T53 PASS.

## Delivered contract

- One captured `RequestContext` contains stable `approval::project_identity`, actual
  persisted session/parent IDs and cache lineage. Main/child execution captures it
  before its tool/retry loop; both title paths and summary compaction capture before
  dispatch. Selection binding copies retain it; no metadata is reconstructed by a
  failure retry or inferred from prompt content.
- Go's common send boundary reasserts User-Agent `oc/<version>`, client/project/
  session and affinity headers after case-insensitive configured/profile overlays.
  Actual parent is emitted only for children; competing x-api-key cannot survive
  resolved Go Bearer auth. Custom/OpenProxy headers retain their policy.
- Production `setCacheKey` uses a stable SHA-256 of session/fork lineage, not the
  body/tools hash. The old public prompt-body helper remains source-compatible but
  is not the runtime cache owner. Missing context does not invent a cache field.
- Forks atomically copy the source's original lineage in the existing native pref
  owner, including nested fork/reopen. A child's own cache lineage is independent
  of its parent (pinned OC2 `session/model-request.ts` uses session/fork, not the
  ancestry root); deleting a family removes its own lineage prefs.
- Responses emits `prompt_cache_key`; Chat requires explicit compatibility support;
  Messages uses tools/system then recent message-part breakpoints, maximum four,
  without marking thinking/redacted thinking. Disabled cache emits none.
- Session context is not deployment/auth identity: compatibility comparison ignores
  it. No protocol-local retry, dependency, production allowlist or paid call added.

## Checks and significant experiments

`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, Cargo jobs 3, test threads 2:

```text
cargo test --locked -p oc-adapters --lib go03_       23/0
cargo test --locked -p oc-adapters --lib             596/0/0
cargo test --locked -p oc-adapters --test runtime --test subagent  120/0 + 38/0
cargo clippy --locked -p oc-adapters --all-targets -- -D warnings  PASS
cargo fmt --all -- --check                          PASS
git diff --check                                   PASS
```

Six actual fake-server requests cover all wires, repeated Responses with changed
content/stable cache key, supported/unsupported Chat cache, Messages cache enabled/
disabled, authoritative case-insensitive identity/auth and child own/parent IDs.
Existing actual application model/manual-title tests now exercise supported cache
fields; actual summary-compaction verifies Go headers and lineage cache. Existing
recursive fork/reopen/context rollback test verifies unchanged source lineage.

First compile exposed `project_identity`'s String error; capture maps it to a safe
constant runtime diagnostic. First full suite exposed 26 shared fork failures:
direct pref insertion omitted required `updated_at`. Replaced that insertion with
the existing transactional `Db::upsert_pref`; all 596 cases pass without changing
their failure/safety expectations. Strict clippy's needless borrows were removed.

Chronological effort capability consumers and protocol-aware durable history/
checkpoint compatibility remain separate required slices. Full workspace/bounded
live qualification remains GO06; this receipt does not claim those gates ran.
