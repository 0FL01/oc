# Goal: native V2 provider and permission semantics

> Предметный справочник, не execution-инструкция. Исторические labels/status не задают очередь работ.
> Текущий scope/workflow — [AGENTS](../../AGENTS.md); открытые outcomes — [GOAL](../../GOAL.md).

Status: complete
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
   - Status: verified
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
  - Status: verified
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
  - Status: verified
- R7: deliver this configuration slice, then resume TUI after publication R8.
  - Source: «Потом коммит пуш, и продолжить работу с паритетом TUI».
  - Acceptance: reviewed configuration implementation/evidence committed and
    normally pushed to the verified own origin/branch. After R8, finish T65 through
    the existing journal; resume T44 and restore its parked WIP once after checking
    Git state. No T44 completion or reduced visual acceptance is implied.
  - Primary evidence: implementation/closeout commits, ordinary push and journal
    transition; T44 resume checkpoint.
   - Status: verified
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
   - Status: verified
### Constraints and non-goals

- Four native Rust crates; no JS production host, V1 migration/fallback, V1 plugin
  loading or full TS plugin runtime. Internal historical names need no renaming.
- No user-config edits, `secrets.env` loader, real credentials or paid/live API
  campaign. Environment export through `.zshrc` stays separate and untouched.
- Preserve redaction, endpoint admission, cancellation, last-good generations,
  RAW/effect ownership, child authority and data-root/process safety.
- No full TUI/storage/service API/built-in-agent parity claim; shared service
  remains descoped. T44 is a separate paused objective, resumed after delivery.

## Material Decisions

- R8: use coherent0.2.0 for the public cache/optional-deadline SDK field changes.
  Public registry lookups returned404 for all four0.2.0 versions before preparation.
  This is version selection, not an upload receipt; publication waits for R6.
- R5: actual pinned `schema/agent.ts::Info.default` supplies four ordinary rules:
  `*` Allow, read `*.env` Ask, read `*.env.*` Ask, read `*.env.example` Allow.
  These precede all top-level documents, then agent documents. Raw unmatched
  evaluation still asks. Keep unselected source/profile rules open as constraints;
  freeze complete selected-parent policies, including defaults, when delegating.
  Existing native session modes/approvals/saved grants remain the supported session
  handling; no new Session service API or secret-file loader is introduced.
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
