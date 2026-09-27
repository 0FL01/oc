# Goal: startup fault isolation и безопасная actionable диагностика

Status: active
Source: владелец 2026-09-27 утвердил после RECON план: MCP/plugin/connect errors изолируются, приложение работает без failed сервиса с визуальным отображением; parity с OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.
Last updated: 2026-09-27
Task: T51 (todo; frozen plan is not execution PASS).

## Objective

Исправная локальная часть native oc (TUI, история, выбор модели) запускается при
отказе optional service. Unsupported plugin и provider/discovery failure имеют
честные failed/unavailable state и безопасную конкретную причину. Выбранная модель
не подменяется; запрос к недоступной модели — явная ошибка. Truly fatal локальный
отказ остаётся non-success, но экран/CLI объясняет источник и действие.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: независимое admission/status compiled native plugins.
  - Source: approved plan; donor `packages/core/src/plugin/supervisor.ts:58–110`, `plugin.ts:47–121,182–235`; A13 exact aliases.
  - Acceptance: CFG09 proves an unknown/lookalike/version/path plugin gets source-qualified `failed/UnsupportedPlugin` before resolution/import/file-as-code/process/network. Other admitted compiled aliases/config/runtime remain usable, failed revision is not represented as active. Exact aliases remain idempotent; genuine compiled-module setup failure is isolated where safe, previous healthy activation retained only with truthful requested/current identity. Do not catch storage/trust/cleanup fatal errors as plugin failures. No arbitrary JS module is loaded; bounded inventory/diagnostics are visible before first prompt and after reload/reopen.
  - Primary evidence: rebuilt actual binary with mixed native aliases + unsupported marker and side-effect counters/typed current-generation inventory; reuse CFG08/AUD17/E2E05.
  - Status: pending
  - Evidence: pending — evidence/T51/report.md.

- R2: local application availability independent of selected-provider readiness.
  - Source: approved plan; donor `packages/core/src/provider.ts:358–409,426–446`, `model-resolver.ts:357–408`, `packages/tui/src/component/dialog-integration.tsx:327–358,382–411`, `context/local.tsx:181–215`.
  - Acceptance: UI07 proves cold discovery timeout/connect/auth failure or missing selected credential does not destroy TUI/history/model picker/diagnostics. Pending/unavailable/error reflects actual owner state, not free text. Configured/persisted selection remains explicit and no silent provider/model fallback or mock response appears. A prompt needing unavailable credentials/model fails before generation/tool effects with an actionable typed outcome; headless exits nonzero. Bounded admitted retry/refresh or explicit valid selection restores readiness and next actual request uses that selection. Failed refresh retains the last healthy catalog; empty cold result does not erase configured models. Discovery success/retry/deadline/metadata/atomic publication contracts are unchanged.
  - Primary evidence: rebuilt TUI/headless with fake discovery/Responses barriers/errors, existing stored history and captured first/next request identity/effect counters; reuse UI02/UI05/DISC01–DISC10/PROV06.
  - Status: pending
  - Evidence: pending — evidence/T51/report.md; T44/VIS42 owns paired presentation.

- R3: common safe diagnostic contract and precise fatal/recoverable classification.
  - Source: approved plan; native `application.rs::SpawnIssue/spawn_diagnostic`, `composition.rs::load_with_env_diagnostic`, `oc/src/tui_cmd.rs`, `oc-tui/src/shell.rs::render_startup_failure`; donor `packages/core/src/config.ts:102–143,194–242,265–287`.
  - Acceptance: CFG10 proves loader/application/TUI/headless use structured source/field/service/stage/safe-code/retryability and allowed next action, not raw error strings or regex classification. Optional service/definition errors yield per-entry failed state, not generic Configuration load failed. Truly fatal trust/policy/owned-storage/data-root/recovery/cleanup/caps failures preserve non-success and explain a redacted source/field/stage/cause/action. Malformed optional config document may be rejected with a visible diagnostic only if effective mandatory policy/config is still completely admitted; cold load cannot discard an unreadable/security-critical policy to proceed with broader defaults. Failed security-relevant reload retains last healthy complete generation, with no mixed generations or half-applied catalogs/instructions/clients. No blanket catch-and-continue. Details/copy/investigate and stderr remain secret/control-sequence safe; --json stdout stays NDJSON only.
  - Primary evidence: actual-binary failure matrix: malformed optional vs policy-critical document, invalid MCP entry (reuse MCP09), unsupported plugin, missing credential/discovery failure, owned storage/trust refusal and failed reload; typed assertions plus stderr/PTY source/field and no raw secret/URL/ANSI leakage.
  - Status: pending
  - Evidence: pending — evidence/T51/report.md; fatal native frame is a declared difference, not invented donor pixel parity.

### Error policy

| Outcome | Scope and continuation |
| --- | --- |
| Optional MCP config/connect/catalog failure | T46 owns failed server inventory/cleanup; application and healthy siblings continue. Failed relist retains the healthy catalog by MCP07; connection close/disconnect retires unavailable tools at safe boundaries. |
| Unsupported/failed compiled plugin | Reject only its capability with visible failed diagnostic; no resolver/JS execution. Healthy native aliases survive. |
| Selected provider/discovery/credential unavailable | Local TUI works; selection remains explicit, generation request fails until admitted recovery/selection. No implicit paid fallback or doubled retry layers. |
| Optional malformed config document/definition | Reject the document/definition with safe source diagnostic only when mandatory effective config/policy remains valid. No partial unsafe policy admission. |
| Trust/policy, data-root lock/storage/recovery, cleanup/shutdown/caps | Fatal at the affected admission/operation/application boundary; prior healthy generation only where valid, never advertise success or bypass security to start. Cancellation stays cancelled, not successful degradation. |

### Constraints and non-goals

- A02/A03/A06/A08/A10/A13, canonical source trust, credential-domain admission,
  immutable request/config generations, atomic catalog publication and owned cleanup hold.
- No raw config/provider exception/remote text, env values, headers, credentials,
  sensitive absolute paths or control sequences in UI/history/logs/copy/drafts.
- Existing data-root/storage/application/runtime owns partial availability; no second
  client registry/database/event bus, service daemon or generic recovery/retry framework.
- Retry reconnects/refreshes an admitted service; it never reexecutes an uncertain tool
  or clears sticky remote quarantine. Restart/reopen do not replay effects/false labels.
- OAuth, CodeMode, arbitrary JS/TS/npm/plugin SDK/hot-load and serve/attach excluded.
  Provider families/routing/live deployment and discovery oracle remain unchanged.
- Donor TUI default-model fallback is not adopted: native UI02 explicit selection holds.
  Do not infer universal recoverability from the donor's syntax-document skip behavior.

## Change Envelope

- Expected files/symbols: adapters config/composition/application diagnostic and readiness
  projections; core runtime request admission as needed; binary TUI/headless startup
  consumers. T44 consumes owner facts; no UI-owned network/process/init or config writes.
- Allowed: minimal typed startup/readiness/inventory changes and source-derived fixtures,
  actual-binary integration tests/report. New persistent schema/dependency/framework is
  not preapproved: prove a blocker to R1–R3 and record smallest justified expansion first.
- Coordination: T46 owns MCP09/MCP10/MCP08, T51 owns CFG09/CFG10/UI07; share a minimal
  diagnostic shape without whole-task done-dependencies. T44 independently owns VIS42
  and config/startup transitions in VIS19/VIS40. Existing detailed owners are unchanged.

## Current Checkpoint

- Closes: R1 then R3 loader-to-startup path.
- Smallest next action: mixed native/unsupported plugin actual-binary failing fixture;
  introduce the smallest typed per-entry diagnostic boundary before early returns.
- Next slices: provider readiness/local app separation → admitted recovery/request
  error → shared redacted fatal screen/headless → T44 visual consumer qualification.
- Verification: nearest affected crate tests; actual binary CFG09/CFG10/UI07; impacted
  regressions above; workspace fmt/clippy/tests/build for cross-crate/final acceptance.
  Use offline fake services; existing mandatory live gates still apply at final closure.

## Current State and Material Decisions

- 2026-09-27 RECON: disabled MCP environment fails strict loader before network; unknown
  plugin and selected-provider composition can fail before TUI; spawn_diagnostic loses
  actionable detail. Donor isolates service/plugin failures; initial filesystem/trust
  failures are not universally recoverable.
- Approved docs-only checkpoint freezes this plan, not implementation. T51 remains todo,
  T44 active and previous task statuses/PASS reports unchanged.
- D21 supersedes only app-wide optional-service rejection and admitted local-MCP minimal
  env/lazy-start policy; shell/security/quarantine/resource/discovery gates remain.
- Blocker: none for planned execution; no new runtime evidence claimed.

## Completion

Pending: R1–R3 require current evidence/T51/report.md and impacted green gates. Backend
success does not qualify VIS42; doc validation does not qualify runtime readiness.
