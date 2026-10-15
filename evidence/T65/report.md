# T65 — native V2 configuration semantics

## Result

**IN PROGRESS.** R2/R3 compatibility and actual cache consumers follow shared V2
fixtures. Transport deadlines, ordered permissions, final legacy/error guards,
complete gates and publication remain. T44 is paused with its stash intact.

Current configuration reference is **V2 revision
`b4f19bfe74a972c2919aa4c9d7445fa61f41b226`**. Initial compatibility/cache checks
used V2.0.30/f16, but R4 source inspection proved that release defaults chunks
to900000ms. Upstream `d8129ca1e036a967add1e7078679fd9b3697a180` introduced that
change; its parent b4 retains all required compatibility/cache/provider-only
features and the owner's exact300000ms defaults. The shared tests were rerun
against that exact clean checkout; requirements were not relaxed. V2.0.12 and
stale `v2` branch4617210 lack required fields. Frozen TUI reference is unchanged.

### R2 binding facet / CFG14

- Reproduced native rejection of valid provider compatibility as
  `Invalid { field: "provider.fixture", reason: "invalid provider shape" }`.
- Provider compatibility is retained separately while documents merge; wire
  admission applies provider defaults then explicit model fields. False and
  absence remain distinct. Base/model/variant bindings and public catalog
  reconstruction retain the same captured facts on all three native protocols.
  No variant compatibility or V1 migration was introduced.
- `tests/fixtures/v2-config-semantics.json` supplies seven ordered-document cases
  across three canonical packages, including both boolean directions, absence,
  independent field merging and early-model-false/later-provider-true.
- `scripts/check_v2_config_semantics.py` stages the exact same JSON into the
  pinned external checkout and uses its actual ConfigProviderPlugin, Provider
  and Model services/test layers. It does not implement a second merge oracle.
  Source files remain unchanged; staged test files are removed afterwards.
  Bun1.4.2 and locked test dependencies are external-only, installed without
  lifecycle scripts. The runner uses isolated HOME/XDG and no real credentials.

### R3 cache consumers / CFG15

- Reproduced a real missing Responses body key under a valid V2 config because
  the obsolete `setCacheKey:false` gate blocked the shared default.
- Removed the typed JSON switch and all runtime gates. Arbitrary package-specific
  `settings.setCacheKey`, including an object, remains valid ignored JSON with
  the existing diagnostic, not `legacy_config` or a second cache control.
- Effective compatibility preserves absent/false/true: Responses allows a
  supplied key unless false; Chat requires true. Independent typed AI request
  `CacheMode::None` omits body keys even through captured model selection.
  No native `settings.cache` JSON option was invented. This SDK field replacement
  is included in the authorized coherent version bump before publication.
- Affinity headers survive disabled body caching. Messages Auto follows the
  pinned last-tool/first-last-system/one-tail policy, maximum4 boundaries;
  the ordinary one-system fixture has3, minimal no-key/no-tools/system input1.
  It is independent of the key capability and session-key availability. Optional
  fields stay absent rather than being inserted as null.
- Same JSON adds eight cache cases across three protocols: absence, both boolean
  states, cache:none, obsolete false/true/object settings, no key/context, and an
  earlier model false against a later provider true with API-model aliasing.
  Native checks capture24 real local HTTP requests, exact key/absence, marker
  counts, omission and affinity. Existing Go checks retain real DB-derived
  root/child lineage, authenticated headers and changed-input stability.
- Actual TS services and `ModelResolver`/`compileRequest` run the same documents
  and expectations. The harness was corrected to observe defined JSON fields,
  not optional undefined AST properties; no TS production code was changed.

## Checks

Current R2/R3 slice:

- Corrected b4 oracle: **45 passed / 0 failed**,95 expectations
  (`.local/evidence/t65-ts-oracle-cfg15-b4f19b.log`).
- Current `provider::context::tests`: **3 passed**, including24 shared requests,
  no-key/minimal omitted fields and the independent existing identity guards.
- Affected provider tests: **79 passed / 1 existing opt-in ignored**;
  config tests: **42 passed**; actual-binary `oc2 --test responses`: **4 passed**.
- Strict adapter/all-target Clippy: PASS, including the final minimal-input fix.
  No lint, test or comparator was suppressed.
- Logs: `.local/evidence/t65-cfg15-*`; full task/workspace gates remain pending.

Initial R2 binding receipt (f16, retained rather than rewritten):

- Shared TS oracle: **21 passed / 0 failed**,42 expectations.
- `cargo test --locked -p oc-adapters --lib cfg14_provider_model_compatibility_uses_shared_v2_documents`:
  PASS, covering the same21 cases plus captured base/variant bindings.
- `cargo test --locked -p oc-adapters --lib config::`: **42 passed**.
- `cargo test --locked -p oc-adapters --lib provider::context::tests`: **2 passed**.
  These preserve the old consumers; they are not R3's new cache qualification.
- `cargo clippy --locked -p oc-adapters --all-targets -- -D warnings`: PASS.
  Full new-source task gates remain pending.

Local synthetic-only logs: `.local/evidence/t65-cfg14-*` and
`.local/evidence/t65-ts-oracle-cfg14.log`.

## Risks and next

R2/R3 are qualified for the specified subset, not full TS protocol/SDK parity.
Next: exact provider-only settings and independent HTTP/WS deadlines, then
ordinary permission precedence without widening delegation/authority ceilings.
Publish only after all configuration gates and the coherent version bump pass;
then finish T65 and resume the parked T44 work exactly once.
