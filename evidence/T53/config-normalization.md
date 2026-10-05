# T53 local provider normalization — partial GO02/GO03

Base: `ec9dd3251`. This receipt is not full GO02/GO03 or T53 PASS.

## Delivered

- One local document owner accepts legacy `provider` and canonical `providers`,
  package/settings/headers/body, legacy api/extraBody, modelID/package/settings,
  capabilities/interleaved and ordered variant arrays. It never processes remote
  executable catalog fields.
- Supplied fields merge across existing ordered sources instead of replacing the
  provider. Objects merge recursively, headers case-insensitively, variants by ID.
  Endpoint/key/header field provenance survives a later metadata-only override;
  that override cannot upgrade the earlier endpoint's trust.
- Conflicting normalized roots, duplicate/missing variant IDs, unknown canonical
  settings and runtime-owned body fields fail safely without echoing values.
  Provider Debug omits arbitrary model/overlay values. Provider env is metadata.
- Static and configured-dynamic variant readers understand canonical settings;
  the existing configured discovery retirement and ordered-effort view remain.

Pinned comparison: `packages/core/src/v1/config/{migrate,provider-options}.ts`
and provider recursive/header merge. Native conflict policy is deliberately
stricter than guessing precedence for both roots in one document.

## Checks and observations

With approved disk TMPDIR, Cargo jobs=3 and test threads=2:

- `cargo test --locked -p oc-adapters --lib`: **576 passed, 0 failed**.
- `cargo test --locked -p oc-adapters --lib go02_`: **3 passed** (repeated after
  strengthening unsupported/reserved-field assertions).
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS.

First broad run exposed the dynamic discovery merger's map-only variant seam;
it dropped canonical local overrides. The production merge now retains them,
and the unchanged actual application refresh/order regression passes. An
integration fixture literal needed the new declarative env metadata field.
No tests were disabled, no API calls/secrets/dependencies were introduced.

## Remaining

Effective provider → model → selected-variant request binding, API model ID,
per-wire settings, overlay/auth resolution and provenance at selection are the
next slice. Public Go models.dev cache, metadata, replay, connect and GO06 remain
pending. Retaining normalized settings is not a claim that all are consumed yet.
