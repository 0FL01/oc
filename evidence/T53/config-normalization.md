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

## Wire options consumer follow-up (2026-10-05)

Base `593e651b6`. Provider-level settings are now typed at wire admission and
consumed by the shared dispatch used by main/auxiliary/child lanes. Responses
supports reasoningEffort/reasoningSummary/textVerbosity; Chat effort; Messages
enabled budgetTokens/budget_tokens, adaptive/disabled thinking, display/block
binding and outputConfig/output_config effort/JSON schema. Opaque unknown shapes,
alias conflicts, invalid budgets and options for the wrong protocol are refused
with constant safe diagnostics, not forwarded. No model-name routing guesses.

Provider body and profile body objects merge recursively with profile priority;
committed effort overrides the provider baseline/output effort. Framing/token/
thinking/output_config remain runtime-owned body fields. WireSettings Debug omits
values. Existing timeout/cancel/failure/retry transport ownership is unchanged.

Tests send six physical **fake** requests (two per native wire), proving exact
routes, default/selected effort, Responses summary/verbosity, Messages thinking
and nested body precedence. The actual Messages application tool roundtrip now
checks provider options on every captured request. Typed tests also reject malformed
thinking/output config, alien protocol options and every reserved body field.

Observation: Serde's internally tagged unit variant ignored fields on disabled
thinking despite enum deny_unknown_fields. Replacing it with a tagged empty
struct variant makes the unchanged invalid-input assertion pass. No suppression.

Final commands with approved disk TMPDIR/jobs=3/test threads=2:

- `cargo test --locked -p oc-adapters --lib`: **579 passed, 0 failed**.
- `cargo test --locked -p oc-adapters --lib go03_captured_options_reach_each_native_wire_with_selected_effort`: PASS.
- Earlier combined `go03_` suite: 18 passed (before adding the six-request test).
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS.

Partial R3 only. Selected model/variant settings/API IDs/auth scope still need
immutable effective binding capture. Public Go cache, metadata, durable replay,
connect/PTY qualification and bounded real GO06 are not claimed by this receipt.
No real API requests, dependencies, secrets or ignored-test changes.

## Effective local request bindings follow-up (2026-10-05)

Base `29ac9af72`. Local model and exact variant overlays now capture immutable
ResponsesConfig templates after recursive settings/body and case-insensitive header
merge. Model package wins; API modelID is consumed only by the wire, while catalog
ID remains selection/history identity. Endpoint/source admission precedes credential
substitution. Literal provider/model/variant IDs use structured provenance tuples,
not ambiguous dotted paths. Later unrelated trusted metadata cannot promote a prior
untrusted file credential or private endpoint.

Application resolves every template independently from its configured inputs and
admitted scope before publishing the generation. Parent stored/env-resolved secrets
are not inherited by endpoint-changing templates. The shared stream consumer picks
from the captured generation; no per-request storage/env lookup. Main, compaction,
title and child admission now considers the selected variant's auth facts. UI
readiness follows the same selected binding. Captured credentials join redaction
inputs but not MCP credential-inheritance inputs. Binding Debug remains secret-safe.

Four focused template/security tests cover recursive variant overlays/package/API
ID, source trust, Go foreign-authority refusal before a secret read, and literal
model/variant ID provenance aliasing and zero-dispatch auxiliary auth refusal.
The actual application test sends two fake
requests: own Messages model+variant with scoped stored key, then independent title
Chat model/API ID/configured key. It checks exact prefixes, auth/header/body/thinking/
effort, absence of parent/foreign Go keys, and fresh-session refusal for missing Key
and unsupported OAuth before inserting any root. Existing tool/live-switch/retry/
compaction suites remain unchanged and green.

Final commands with approved disk TMPDIR/jobs=3/test threads=2:

- `cargo test --locked -p oc-adapters --lib`: **584 passed, 0 failed, 0 ignored**.
- `cargo test --locked -p oc-adapters --lib effective_wire_tests -- --nocapture`: PASS.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
- `cargo fmt --all -- --check`, `git diff --check`: PASS.

Initial compile exposed a too-narrow child-module re-export; visibility now reaches
only `crate::config`. Strict clippy requested a borrowed one-element source slice;
fixed without suppression. Review replaced dotted credential provenance with tuple
keys and added the independent aliasing regression before final full-suite rerun.

Partial local R1/R3 only. Remote Go catalog capture, Go headers/cache/chronological
effort, complete durable protocol/API/deployment/auth-scope provenance and account
controls/connect/PTY/bounded live remain required; **no GO01–GO06 or T53 PASS claim**.
No real API requests, new dependencies, ignored tests or secret files were used.
