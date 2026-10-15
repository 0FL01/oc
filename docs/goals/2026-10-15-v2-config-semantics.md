# Goal: native V2 provider and permission semantics

Status: active
Source: owner plan 2026-10-15, sections1–5 and done/check/scope clauses; explicit
instruction to pause T44, implement this plan, commit/push, then resume TUI parity.
Last updated: 2026-10-15
Task: T65 / M8
Oracle: local `opencode` V2.0.12 at `2670273ff17da96f85c5826ced57aa1b368754fa`.

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
Then perform the owner's explicit return to the separately frozen T44 objective.

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
  - Status: pending
  - Evidence: pending.
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
  - Status: pending
  - Evidence: pending.
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
  - Status: pending
  - Evidence: pending.
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
- R7: deliver this configuration slice, then resume the paused TUI objective.
  - Source: «Потом коммит пуш, и продолжить работу с паритетом TUI».
  - Acceptance: reviewed configuration implementation/evidence committed and
    normally pushed to the verified own origin/branch. Finish T65 through the
    existing journal; resume T44 and restore its parked WIP once after checking
    Git state. No T44 completion or reduced visual acceptance is implied.
  - Primary evidence: implementation/closeout commits, ordinary push and journal
    transition; T44 resume checkpoint.
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
- No new store, service, scheduler, production SDK/dependency or general
  abstraction. Expand only for a demonstrated blocker to these R outcomes.

## Current Checkpoint

- Closes: R2 first, preserving R1.
- Smallest next action: trace pinned provider/model compatibility folding and
  native document merge/binding assembly; add the cross-document false/true
  regression and fix effective compatibility before changing cache semantics.
- Expected evidence: targeted shared compatibility cases with explicit false,
  including an earlier model override versus a later provider default.
- Replan if: the existing merge boundary destroys level provenance; keep the
  smallest level-separated representation rather than changing all settings.

## Current State

- T44 delivered through `8a68dde06` and paused by the latest owner instruction.
  WIP is parked at Git stash `1087067b3025322885c6a964579d80cea4931b06`, not applied.
  Its detailed continuation is in `evidence/T44/configuration-pause-20261015.md`.
- Oracle SHA checked directly; no configuration implementation edits yet.
- Last baseline: workspace1984/0/12existingignored and46 targets, current TUI541
  tests/strict Clippy, debug/release/help. These predate this configuration work.
- No external blocker demonstrated. Next: R2 compatibility.

## Material Decisions

- Pin the existing repository donor V2.0.12, rather than moving upstream HEAD.
- User's `cargo test -p oc` means the current native binary package `oc2` after
  T64's rename; use its real Cargo name, not a new compatibility package.
- Ordinary configured precedence and saved grants must both satisfy R5; inspect
  the pinned configured/evaluateInput split instead of flattening real authority
  restrictions or letting a saved grant bypass configured deny.

## Completion

Not complete; R1–R7 remain unresolved. T44 must be resumed after T65 delivery.
