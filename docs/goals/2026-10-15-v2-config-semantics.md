# Goal: native V2 provider and permission semantics

Status: active
Source: owner plan 2026-10-15, sections1–5 and done/check/scope clauses; explicit
instruction to pause T44, implement this plan, commit/push, then resume TUI parity.
Additional source: owner explicitly authorizes a version bump and crates.io
publication after configuration completion, using already-present Cargo credentials,
then continuing the paused work. Never print credentials.
Last updated: 2026-10-16
Task: T65 / M8
Configuration oracle: V2 revision `b4f19bfe74a972c2919aa4c9d7445fa61f41b226`,
read-only source cache `/home/opencode/.cache/opencode-tmp/opencode/t65-ts-oracle`.
TUI oracle remains local V2.0.12/`2670273`; it is not updated by this task.

## Objective

One canonical supported V2 configuration has the same provider/model/variant and
ordinary user-permission interpretation in pinned TS and Rust, without a second
Rust-specific cache switch. This is the specified configuration subset, not full
application, built-in-agent or TUI parity. V1 remains fail-closed.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and
Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements
from reviews, tests, tools, speculative risks, or optional source text. Finish
when every required outcome is resolved and affected constraints remain satisfied.
Then publish the verified new version and perform the owner's explicit return to
the separately frozen T44 objective.

## Frozen Contract

### Required Outcomes

- R1: preserve the canonical/V1 boundary and truthful error categories.
  - Source: owner section1 and done clauses.
  - Acceptance: V1 `provider`, `permission`, `npm`, `options`, `modalities` and the
    other existing legacy forms still fail as `legacy_config`, before MCP/provider
    effects. Correct but unsupported V2 uses the existing capability diagnosis;
    malformed V2 uses configuration errors. Arbitrary package-specific JSON keys
    inside V2 `settings` do not become V1 syntax merely by name; in particular do
    not ban every `settings.setCacheKey` as legacy.
  - Primary evidence: existing legacy/config/composition tests plus positive V2
    and negative V1 synthetic fixtures with pre-effect assertions.
  - Status: pending
  - Evidence: pending.
- R2: provider defaults then model compatibility overrides.
  - Source: owner section2; pinned `config/plugin/provider.ts`, `model.ts`
    (`Model.snapshot`), `schema/src/model-compatibility.ts`.
  - Acceptance: first merge documents separately at provider/model levels, then
    compute effective compatibility by fields for wire bindings. Explicit false
    survives. An earlier model false beats a later provider true. Apply relevant
    fields to every supported protocol, not only Chat. No variant compatibility
    feature is introduced.
  - Primary evidence: shared synthetic V2 cases for both boolean directions,
    missing values and cross-document overrides, checked against pinned TS.
  - Status: verified
  - Evidence: shared21 binding cases and24 actual protocol requests, including
    cross-document model false with an API-model alias; corrected TS pin45 tests
    PASS. See evidence/T65/report.md and R3's cache/selection checks.
- R3: V2 compatibility alone controls protocol-specific prompt cache-key behavior.
  - Source: owner section3; pinned `ai/src/protocols/shared.ts` and `openai-chat.ts`.
  - Acceptance: remove `settings.setCacheKey` as a runtime gate; preserve
    None/Some(false)/Some(true). Chat needs explicit supportsPromptCacheKey true;
    Responses shared logic distinguishes absence from false. `cache:"none"`
    omits the prompt cache key. Header affinity and Anthropic cache blocks remain
    separate policies, not casualties of one global caching boolean. One shared
    config needs only `compatibility.supportsPromptCacheKey:true`.
  - Primary evidence: exact captured synthetic Responses/Chat/Messages request
    JSON and affinity headers for absent/false/true/cache-none cases.
  - Status: verified
  - Evidence: shared8 cache cases ×3 protocols, TS45/95 expectations overall;
    native context3, provider79, config42, actual-binary Responses4 and strict
    adapter/all-target Clippy PASS. No runtime setCacheKey gate remains.
- R4: provider-only transport settings and separate native timeout budgets.
  - Source: owner section4; pinned `core/src/provider.ts::modelSettings` and actual
    HTTP/WebSocket protocol consumers.
  - Acceptance: model/variant overlays exclude exactly the provider-only
    `timeout`, `headerTimeout`, `chunkTimeout`, `transport` settings without
    changing precedence of the rest of settings. HTTP header and chunk defaults
    are each300000ms; false disables that individual deadline. Overall `timeout`
    is separate and absent by default. WebSocket defaults/frame waits follow
    their own TS source, not blindly HTTP defaults. Cancellation works even when
    deadlines are disabled.
  - Primary evidence: binding overlay cases and controlled local servers delaying
    headers/chunks/frames, total timeout and cancellation; no real credentials.
  - Status: verified
  - Evidence: shared TS54/167 expectations, native CFG16 four targeted tests;
    base/variant HTTP/transport/raw-WS budgets, controlled independent headers/body,
    disabled-deadline cancellation, all-protocol overall deadline with phase
    budgets disabled, WS30-minute default/false/numeric frames and cancellation.
    Provider82/1existingignored, config43, binary Responses4 and strict workspace
    all-target Clippy PASS. Reproduced overall timer restarting after delayed
    dispatch (701ms for400ms), fixed by retaining the absolute body deadline;
    all-protocol partial-output/one-attempt guards pass. Explicit SDK WS idle
    overrides and native bounded connect/resource guards are retained.
- R5: ordered ordinary permission overrides without weakening real authorities.
  - Source: owner section5; pinned `config/plugin/agent.ts`, `permission.ts`
    (`evaluate`, `configured`, `evaluateInput`).
  - Acceptance: ordinary rules follow agent defaults → top-level documents in
    config order → agent-specific documents in config order → session permission
    handling; last configured match wins, no match asks. Later config allow beats
    earlier deny; agent-specific rules override top-level. Saved allow must not
    bypass the final configured deny. Authority/sandbox/delegation constraints
    stay independent and non-widenable by user allow; do not globally replace
    strictest. Plan/Explore ceilings remain explicit native policy, not a claim
    of complete built-in-agent permission parity.
  - Primary evidence: shared ordered-rule cases plus saved-grant/config-deny,
    child-parent authority, sandbox/endpoint and existing security regressions.
  - Status: pending
  - Evidence: pending.
- R6: qualify the specified subset with shared synthetic inputs and retain guards.
  - Source: owner checks and unchanged/non-goal sections.
  - Acceptance: same covered V2 documents and expected results used for TS and
    Rust; pinned oracle recorded. Existing V1 rejection, credential redaction,
    endpoint admission, cancellation, MCP activation and authority tests stay
    green. Package mappings, native Markdown model+variant shorthand and existing
    MCP startup/catalog/execution timeouts/precedence retain regressions, not a
    rewrite.
  - Primary evidence: targeted tests; `cargo test --locked -p oc-adapters`,
    `cargo test --locked -p oc2`, `cargo test --locked --workspace --no-fail-fast`,
    fmt/strict Clippy and affected native build gates.
  - Status: pending
  - Evidence: pending.
- R7: deliver this configuration slice, then resume TUI after publication R8.
  - Source: «Потом коммит пуш, и продолжить работу с паритетом TUI».
  - Acceptance: reviewed configuration implementation/evidence committed and
    normally pushed to the verified own origin/branch. After R8, finish T65 through
    the existing journal; resume T44 and restore its parked WIP once after checking
    Git state. No T44 completion or reduced visual acceptance is implied.
  - Primary evidence: implementation/closeout commits, ordinary push and journal
    transition; T44 resume checkpoint.
  - Status: pending
  - Evidence: pending.
- R8: bump and publish the completed configuration implementation to crates.io.
  - Source: subsequent explicit owner publication authorization.
  - Acceptance: choose an appropriate unpublished version for exposed API changes,
    update the native workspace/dependency versions coherently, preserve modular
    packages/assets and MIT/third-party/AGPL provenance. Publish in dependency
    order only after configuration gates pass; use Cargo's existing credentials
    without exposing them. Verify public registry metadata and isolated
    `cargo install oc2 --locked --version <new-version>` from the registry, not a
    path/git fallback, plus actual installed help/version. No release/tag or
    full-product READY claim. Resume TUI only after this delivery.
  - Primary evidence: existing T64 publication workflow reused for the new version,
    real package/publish receipts, registry metadata and isolated install output.
  - Status: pending
  - Evidence: pending.

### Constraints and non-goals

- Four native Rust crates; no JS production host, V1 migration/fallback, V1 plugin
  loading or full TS plugin runtime. Internal historical names need no renaming.
- No user-config edits, `secrets.env` loader, real credentials or paid/live API
  campaign. Environment export through `.zshrc` stays separate and untouched.
- Preserve redaction, endpoint admission, cancellation, last-good generations,
  RAW/effect ownership, child authority and data-root/process safety.
- No full TUI/storage/service API/built-in-agent parity claim; shared service
  remains descoped. T44 is a separate paused objective, resumed after delivery.

## Change Envelope

- `oc-adapters/src/config.rs`, `config/{providers,request_bindings,legacy}.rs` and
  nearest tests; `defs.rs`, `permissions.rs`, `composition.rs`/`composition/catalog.rs`;
  `provider.rs` plus `provider/{context,chat,websocket,timeout_tests}.rs` and direct
  protocol consumers; existing permission/config/provider integration targets.
- Shared synthetic fixtures/test-only pinned-TS oracle runner, current CONFIG/
  PROVIDER documentation and the existing journal/task registry as necessary.
- Workspace/package Cargo manifests/lock and publication metadata, archive and
  isolated-install checks as in T64; no new publishing framework or release/tag.
- No new store, service, scheduler, production SDK/dependency or general
  abstraction. Expand only for a demonstrated blocker to these R outcomes.

## Current Checkpoint

- Closes: R5 ordinary configuration order without widening authority.
- Smallest next action: add shared document/agent permission precedence cases
  against the real pinned ConfigAgentPlugin/Permission evaluator, reproduce native
  intersection behavior, then distinguish ordinary configured ordering from
  independent native/parent constraints. Keep generic strictest/narrow boundaries.
- Expected evidence: late allow/deny, agent-specific priority, unmatched Ask;
  actual saved-grant versus final-deny and child-parent authority regressions.
- Replan if: an ordinary override can erase a delegation/sandbox/profile ceiling.

## Current State

- T44 delivered through `8a68dde06` and paused by the latest owner instruction.
  WIP is parked at Git stash `1087067b3025322885c6a964579d80cea4931b06`, not applied.
  Its detailed continuation is in `evidence/T44/configuration-pause-20261015.md`.
- Old TUI pin/stale `v2` branch lack required configuration fields; initial f16
  established compatibility/cache behavior, then R4's explicit300000ms defaults
  selected the exact pre-fifteen-minute-change revision above. R2/R3 are verified:
  same21 compatibility cases and24 protocol cache requests, current TS45/95
  expectations, native context3/provider79/config42/Responses4/strict Clippy PASS.
  Request cache:none survives captured model selection, affinity is independent,
  and minimal Messages requests retain absent fields rather than nulls.
- R4 is verified: actual ModelResolver service and native captured bindings agree
  on provider-only settings; independent HTTP defaults/false/cancel and separate
  WS frame policy pass real local-server tests. Shared TS now54/167 expectations;
  provider82/1existingignored, config43, binary Responses4 and strict workspace
  all-target Clippy pass. SDK deadline-field changes join the authorized version
  bump, not a new JSON switch. R1/R5/final gates/publication remain unresolved.
- Last baseline: workspace1984/0/12existingignored and46 targets, current TUI541
  tests/strict Clippy, debug/release/help. These predate this configuration work.
- Evidence: `evidence/T65/report.md`. No external blocker demonstrated.

## Material Decisions

- 2026-10-16, R4 oracle correction: V2.0.30/f16 has a **900000ms** chunk
  default, contradicting the owner's explicit **300000ms** requirement. Upstream
  commit `d8129ca1e036a967add1e7078679fd9b3697a180` introduced that fifteen-minute
  default; its exact parent `b4f19bfe74a972c2919aa4c9d7445fa61f41b226` retains
  provider compatibility, shared cache-key opt-out, the same Auto cache policy
  and four provider-only settings with both HTTP defaults300000ms. Correct the
  configuration oracle to that parent and rerun the shared tests unchanged in
  intent. Do not weaken the owner's deadline requirements or change the TUI pin.
  Earlier f16 receipts remain historical evidence, not rewritten qualifications.
- R3: the runtime request policy exposes the supported AI `cache:auto|none`
  subset, not a new JSON `settings` switch. Replace the obsolete SDK cache flag
  with that typed policy and keep captured selection/context ownership. Actual
  V2.0.30 `cache-policy.ts` marks the last tool, distinct first/last system parts
  and one conversation tail (maximum4); the shared one-system fixture has3,
  not the old native fill-all-remaining-message-slots4. Align that Auto shape
  while keeping capability-false independent, cache-none and affinity semantics;
  no new manual-hint/TTL configuration capability is implied.
- Correct the initial oracle assumption using actual sources: V2.0.12 and `v2`
  branch `4617210` predate provider defaults and cache-key opt-out; latest stable
  tag V2.0.30 established the compatibility/cache features. The later R4 check
  above corrects its timeout-era mismatch; use the exact matching revision, not
  a moving branch. Keep the separate frozen TUI oracle untouched.
- User's `cargo test -p oc` means the current native binary package `oc2` after
  T64's rename; use its real Cargo name, not a new compatibility package.
- Ordinary configured precedence and saved grants must both satisfy R5; inspect
  the pinned configured/evaluateInput split instead of flattening real authority
  restrictions or letting a saved grant bypass configured deny.

## Completion

Not complete; R2/R3 are verified, R1/R4–R8 remain unresolved. T44 resumes after
T65 delivery/publication.
