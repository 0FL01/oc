# T65 — native V2 configuration semantics

## Result

**IN PROGRESS.** Provider/model binding facts now follow the shared V2 fixtures;
cache consumers, transport deadlines, ordered permissions, complete gates and
publication are not yet qualified. T44 remains paused with its stash intact.

Configuration reference is released **V2.0.30**,
`f16ef9ad8878279ea2be3cfab7baeed97aef61aa`. Actual source inspection falsified
V2.0.12 and stale `v2` branch `4617210` as references for these newer fields.
The separate frozen TUI reference is unchanged.

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

## Checks

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

R2/R3 remain coupled until the actual protocol consumers honor the new tri-state
facts. Remove the `settings.setCacheKey` gate next and add shared real-request
cache/affinity cases; do not replace it with another Rust-only JSON switch.
`cache:"none"` is the AI request policy, not a guessed package-specific settings
key. Follow with the frozen provider-only/deadline and permission outcomes.
Publish only after all configuration gates and the coherent version bump pass;
then finish T65 and resume the parked T44 work exactly once.
