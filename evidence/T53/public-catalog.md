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
