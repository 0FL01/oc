# T53 public Go catalog owner — partial R2

Base: `30b0d1e7c`. This receipt does **not** claim GO02 or T53 complete.

## Delivered contract

- `models_dev::GoCatalog` fetches only `https://models.dev/api.json`, with one
  credential-free GET and a 15-second whole-response deadline / 8 MiB cap. The
  production transport remains `ReqwestDiscoveryClient`: DNS pin/peer guards,
  no redirects, no proxy or automatic retries. No provider key/header is supplied.
- Typed normalization retains only Go models, effective finite package aliases,
  IDs/limits/tool support/modalities/basic cost/status/interleaved and exact
  reasoning options. Unknown packages remain explicit metadata, without inferred
  controls. No remote `api`, `env`, headers, settings or foreign provider persists.
- Declared effort values exclude null sentinels. Messages toggle/budget controls
  have implemented native spellings; effort takes priority over budget and toggle,
  with declared toggle-off retained. No vendor/model-name defaults or bundled cache.
- Cache is a bounded source-qualified SQLite preference in the existing Db. Only
  validated public base plus fetch timestamp is persisted, not merged local data.
  Writes are best effort. Current local overrides apply afresh to surviving IDs;
  limits field-merge and same-ID variants replace, rather than combining dialects.
- Valid empty records retire models; deprecated and removed IDs cannot resurrect
  from local overrides. Failed/malformed/missing refresh preserves last-good and
  timestamp. Concurrent forced/stale fetches coalesce; cancellation publishes
  nothing; all failed-flight waiters receive the same safe failure. Unchanged data
  advances freshness without claiming a changed catalog. Debug omits model values.

## Checks and observations

Environment: approved disk TMPDIR, Cargo jobs=3 / test threads=2.

```text
cargo test --locked -p oc-adapters --lib models_dev::tests   4 passed
cargo test --locked -p oc-adapters --lib                    588 passed / 0 failed / 0 ignored
cargo clippy --locked -p oc-adapters --all-targets -- -D warnings  PASS
cargo fmt --all -- --check                                 PASS
git diff --check                                           PASS
```

Strict clippy initially identified single-pattern match, Option iteration and
manual div-ceil; corrected code directly, without suppressions. Review also
added same failure propagation to coalesced waiters and donor main-control
priority, covered by regressions.

Two bounded public-only Python/urllib observations with native User-Agent on
2026-10-05 confirmed the source envelope (5,319,988 bytes, 33 Go rows, provider
package openai-compatible, model-level Responses/Messages overrides, required
row fields and integer limits). A UA-less observation returned HTTP 403; the
native transport already sends `oc/<version>`. Current public records advertise
qwen3.8-max and qwen3.7-plus as Messages; no paid generation/probe was performed.
This is catalog research, not GO06 live wire qualification.

## Remaining

Connect this owner to application startup/picker/manual refresh and the shared
selection-independent `oc models` read-view, with owning-generation late-result
checks and effective remote model bindings. Then all-lane Go metadata/cache/
chronology, durable protocol replay, account/connect PTY, final offline gates
and bounded real Go qualification. No secret/live credential read or new dependency.

## Application integration follow-up (2026-10-05)

Base: `9901d36ad`. Partial R2/R3 qualification, not full GO02/T53 PASS.

- Provider-owned Go preset is local and only activated when requested. Configured
  source merge/provenance and Go authority validation still precede credential reads.
- Existing Db owns one lazy public cache shared by its native worker handles;
  no second lock/store, auth file or client registry. Startup reads last-good before
  optional asynchronous GET, picker queries trigger TTL-gated refresh, explicit
  reload forces refresh, and destination admission shares the same owner. Optional
  catalog work is canceled/joined before old Location disposal; no late result can
  publish into a replacement composition.
- Each surviving public ID/variant receives an immutable finite wire/API binding.
  Remote routes/auth inputs stay discarded; Go Messages uses Bearer, not x-api-key.
  Unknown aliases remain visible metadata but unavailable and refuse before root,
  user/turn effects or HTTP. Valid empty records clear metadata and captured bindings.
- Public fetch failure retains last-good without making a public 401/403 a paid
  credential fact. Missing credentials do not prevent usable public catalog rows.
- Separated refresh single-flight lock from state lock: cached reads do not block
  on network. Root stored material is captured in redaction-only config inputs,
  never promoted into MCP credential inheritance.

Regression tests cover actual application cached startup without a provider entry,
missing key refusal, stored-key restart, unknown-alias refusal with no root; all
three captured native wires, exact controls/Chat reasoning compatibility, Go
Bearer, current local names, public-401 isolation and retirement; shared cache
identity and immediate last-good reads while a fake GET is deliberately held.
First new-test compilation exposed wrong import/ownership assumptions; corrected
to existing core domain/selection types and owned credential material.

Checks (approved disk TMPDIR, Cargo jobs 3, test threads 2):

```text
cargo test --locked -p oc-adapters --lib composition::go_catalog  PASS 2/0
cargo test --locked -p oc-adapters --lib                       PASS 591/0/0
cargo clippy --locked -p oc-adapters --all-targets -- -D warnings PASS
cargo fmt --all -- --check                                   PASS
git diff --check                                            PASS
```

No real Go generation/live key read, new dependency, disabled test or increased
deadline. Shared selection-independent CLI/tool read-view and full remaining T53
gates are still pending; CLI must retain its existing no-history-mutation/second-
owner invariant rather than opening an already-owned Db just to list models.
