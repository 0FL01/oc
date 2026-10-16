# T65 — native V2 configuration semantics

## Result

**CONFIGURATION QUALIFIED / REGISTRY PUBLISHED / INSTALLED BEHAVIOR PASS.** R1–R6
pass for the owner's specified subset. Coherent version0.2.0 is published for
`oc-core`, `oc-adapters`, `oc-tui` and the sole native binary package `oc2`.
Actual installed-binary verification and R7's journal return to TUI pass. R1–R8
are verified and T65 is complete. See [the closeout receipt](closeout.md) and
[actual T44 return](../T44/configuration-return-20261016.md).

Configuration oracle: TS V2 **b4f19bfe74a972c2919aa4c9d7445fa61f41b226**, before
upstream's fifteen-minute chunk-default change. This preserves the owner's
explicit300000ms HTTP requirements. Frozen TUI oracle2670273 is unchanged.
Detailed implementation experiments, failures, corrections and earlier gate
receipts remain in [the candidate history](implementation-and-gate-history.md).

### Required behavior

- **R1 — canonical/error boundaries:** V1 remains `legacy_config` before MCP or
  provider effects. Opaque package-specific JSON inside V2 settings is not legacy
  by name; `setCacheKey` can be an object and is not a runtime switch. Canonical
  reasoningField is a string; malformed known compatibility values are Invalid,
  valid unsupported capabilities remain UnsupportedCapability. Actual binary
  positive/MCP-effect and five V1-negative controls retain exact pre-effect and
  canary/loader guards.
- **R2 — compatibility:** provider and model documents merge separately, then
  model overrides provider defaults by field. Explicit false and an early model
  override survive later provider documents. Captured base/variant bindings and
  supported Responses/Chat/Messages consumers share that effective tri-state.
  No variant compatibility feature or model-name inference was introduced.
- **R3 — caching:** no runtime `settings.setCacheKey` gate. Chat requires explicit
  supportsPromptCacheKey true; Responses permits absence but respects false;
  request CacheMode::None suppresses the key even through captured selections.
  Affinity headers remain independent. Messages Auto follows the pinned
  tool/system/conversation-tail policy, including no-key-context requests and
  omitted optional fields; None disables automatic blocks, not header identity.
- **R4 — settings/timeouts:** exactly timeout/headerTimeout/chunkTimeout/transport
  remain provider-only. Other overlay precedence is unchanged. HTTP header and
  chunk defaults are separately300000ms; false disables each, total is optional.
  Absolute total deadlines survive delayed dispatch and retain partial-output,
  one-attempt, cancellation and error provenance. WS raw absence uses its own
  thirty-minute frame policy, false/numeric and explicit SDK overrides work;
  native bounded connect/security guards remain independent.
- **R5 — permissions:** ordinary agent defaults → ordered top-level documents →
  ordered agent documents → existing native session approval/grant handling.
  Last matching configured rule wins; raw unmatched evaluation asks. Final
  configured deny defeats saved allow. Independent/programmatic/parent and
  Plan/Explore ceilings still intersect; strictest was not globally replaced.
  A child gets its own agent phase under the complete captured parent ceiling.
  Native built-in-agent differences remain explicit, not full TS agent parity.
- **R6 — regressions:** canonical package mappings, native Markdown model+variant,
  MCP startup/catalog/execution/precedence, redaction, endpoint admission,
  cancellation, immutable requests/history, resource and authority guards pass.

## Checks

Current0.2.0 final qualification, without real provider credentials:

| Gate | Actual result |
| --- | --- |
| Exact pinned TS/shared V2 fixtures | **79 passed, 0 failed;192 expectations** |
| `cargo test --locked -p oc-adapters` | **1086 passed,0 failed,5 existing opt-in ignored;19 targets** |
| `cargo test --locked -p oc2` | **333 passed,0 failed,7 existing opt-in ignored;23 targets** |
| Fresh `cargo test --locked --workspace --no-fail-fast` | **1998 passed,0 failed,12 unchanged opt-in ignored;46 targets** |
| `cargo fmt --all --check` | PASS |
| Strict workspace/all-target Clippy, `-D warnings` | PASS |
| `cargo build --locked` and `--release` | PASS |
| Actual release help/version/smoke, fresh HOME and empty PATH | PASS; `oc2 0.2.0`, native core/adapter/TUI identities |
| Python scripts / capture tests | **71 / 8 passed**; existing SQLite ResourceWarnings visible |
| Latest workspace `cargo publish --dry-run --locked --allow-dirty` | PASS: all four extracted packages verified against staged registry dependencies; **all uploads aborted by dry-run mode** |

The fresh workspace used an isolated HOME, environment whitelist and the normal
two Rust test threads, without overlapping compiler/package workloads. Its eleven
initial failures are closed: intended fixture actors now explicitly deny other
tools after the required TS default-Allow change. Exact wire/replay/call counts
and S07/VIS31/VIS38 resource, queue and timing assertions were not relaxed.
Full failure and corrected-run chronology remains in the candidate history.

Logs: `.local/evidence/t65-schema-ts.log`,
`t65-final-{adapters,binary}-01.log`, `t65-final-workspace-02.log`,
`t65-final-{clippy,debug,release,release-help,release-version,release-smoke}.log`,
`t65-final-{python,capture-python}.log`, `t65-publish-dry-run-02.log`.
Rust jobs3/test threads2 and approved TMPDIR were used for final native gates.

### R8 package preparation

- Four package versions and six internal requirements agree on0.2.0. Lockfile
  changes only the four workspace package versions, not third-party dependencies.
- Package inventories: core30/adapters325/TUI252/oc2 85 files. Metadata, README,
  MIT/AGPL licenses, third-party notices, bundled skill and syntax assets remain;
  private env/database/artifact paths are excluded.
- Both the initial and latest archive dry runs verified all four packages, not
  sibling-path fallbacks. The existing locked yoke-derive0.8.3 yanked warning was
  retained, not suppressed or addressed through unrelated dependency updates.
- Pre-upload version lookups returned404 for all four0.2.0 packages. The single
  actual workspace publication now passes; independent public version API reads
  confirm all four0.2.0 packages, unyanked, published_by0FL01, correct licenses
  and oc2 as the only executable. See [the actual receipt](publication.md).
- Source/checkpoint `feeea77fc` was committed/pushed before upload. The isolated
  registry-only installation outside the checkout passes; Cargo records the real
  crates.io source and only executable oc2. Actual installed ELF version/help/
  smoke, canonical V2 headless cache/affinity and persisted-session listing with
  no repeat generation pass in fresh HOME/XDG/data/project roots with empty PATH.
  The test helper's initially obsolete model id was correctly refused; changing
  only that fixture to modelID and using a fresh root completes the check.

## Risks

No known configuration blocker remains. This is not full TS application,
plugin-runtime, built-in-agent, storage/service or TUI parity; no product READY
claim. No real provider credentials, user configuration or secrets.env loader
were used/changed. Cargo publication used its existing credential mechanism
without displaying secrets. No release/tag or default-user installation.

T44's exact parked WIP was restored once from stash
`1087067b3025322885c6a964579d80cea4931b06`, which remains preserved. Registry
delivery/install verification and the journal handoff are complete. All uploads are confirmed;
do not repeat publication. Never repeat an unknown external effect.

## Next

T65 is finished; active work is the resumed full TUI-parity goal. Its known
Shell-notice regression and small-profile WIP are separate from the published
green configuration snapshot. No further publication or configuration expansion.
