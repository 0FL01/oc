# Goal: startup fault isolation и безопасная actionable диагностика

Status: complete
Source: владелец 2026-09-27 утвердил после RECON MCP/plugin/connect isolation; 2026-09-29 потребовал real-user E2E в текущей конфликтующей TS-config среде и strace, затем утвердил правки плана. Donor: OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.
Last updated: 2026-09-29
Task: T51 (complete; current R1–R4 qualification — evidence/T51/report.md; not product READY or T44 visual PASS).

## Objective

Исправная локальная часть native oc (TUI, история, выбор модели) запускается при
отказе optional service. Unsupported plugin и provider/discovery failure имеют
честные failed/unavailable state и безопасную конкретную причину. Выбранная модель
не подменяется; запрос к недоступной модели — явная ошибка. Унаследованные TS config
sources и saved missing/invalid agent/model/variant не закрывают локальный Home,
историю и selectors, в том числе при восстановлении parked tabs. Truly fatal
локальный отказ остаётся non-success, но экран/CLI объясняет источник и действие.

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
   - Status: verified
   - Evidence: evidence/T51/plugin-admission.md and evidence/T51/report.md.

- R2: local application availability independent of selected-provider readiness.
  - Source: approved plan; donor `packages/core/src/provider.ts:358–409,426–446`, `model-resolver.ts:357–408`, `packages/tui/src/component/dialog-integration.tsx:327–358,382–411`, `context/local.tsx:181–215`.
  - Acceptance: UI07 proves cold discovery timeout/connect/auth failure or missing selected credential does not destroy TUI/history/model picker/diagnostics. Pending/unavailable/error reflects actual owner state, not free text. Configured/persisted selection remains explicit and no silent provider/model fallback or mock response appears. A prompt needing unavailable credentials/model fails before generation/tool effects with an actionable typed outcome; headless exits nonzero. Bounded admitted retry/refresh or explicit valid selection restores readiness and next actual request uses that selection. Failed refresh retains the last healthy catalog; empty cold result does not erase configured models. Discovery success/retry/deadline/metadata/atomic publication contracts are unchanged.
  - Primary evidence: rebuilt TUI/headless with fake discovery/Responses barriers/errors, existing stored history and captured first/next request identity/effect counters; reuse UI02/UI05/DISC01–DISC10/PROV06.
   - Status: verified
   - Evidence: evidence/T51/provider-readiness.md and evidence/T51/report.md; T44/VIS42 owns paired presentation.

- R3: common safe diagnostic contract and precise fatal/recoverable classification.
  - Source: approved plan; native `application.rs::SpawnIssue/spawn_diagnostic`, `composition.rs::load_with_env_diagnostic`, `oc/src/tui_cmd.rs`, `oc-tui/src/shell.rs::render_startup_failure`; donor `packages/core/src/config.ts:102–143,194–242,265–287`.
  - Acceptance: CFG10 proves loader/application/TUI/headless use structured source/field/service/stage/safe-code/retryability and allowed next action, not raw error strings or regex classification. Optional service/definition errors yield per-entry failed state, not generic Configuration load failed. Truly fatal trust/policy/owned-storage/data-root/recovery/cleanup/caps failures preserve non-success and explain a redacted source/field/stage/cause/action. Malformed optional config document may be rejected with a visible diagnostic only if effective mandatory policy/config is still completely admitted; cold load cannot discard an unreadable/security-critical policy to proceed with broader defaults. Failed security-relevant reload retains last healthy complete generation, with no mixed generations or half-applied catalogs/instructions/clients. No blanket catch-and-continue. Details/copy/investigate and stderr remain secret/control-sequence safe; --json stdout stays NDJSON only.
  - Primary evidence: actual-binary failure matrix: malformed optional vs policy-critical document, invalid MCP entry (reuse MCP09), unsupported plugin, missing credential/discovery failure, owned storage/trust refusal and failed reload; typed assertions plus stderr/PTY source/field and no raw secret/URL/ANSI leakage.
   - Status: verified
   - Evidence: evidence/T51/fatal-diagnostics.md and evidence/T51/report.md; fatal native frame is a declared difference, not invented donor pixel parity.

- R4: inherited TS configuration и недоступный saved selection не блокируют локальный startup; обязательная real-user qualification.
  - Source: запрос владельца 2026-09-29: добавить E2E с запуском от лица пользователя в текущей среде с конфликтующими конфигами/MCP OpenCode TS и дополнительно проверить бинарник через strace; утверждение правок плана на текущей ветке.
  - Acceptance: E2E06 proves supported global/Location/.opencode sources keep pinned precedence and safe diagnostics; optional plugin/DCP/compaction/MCP/provider failures remain visible without cancelling healthy local Home/history/agent-model-variant selectors. Missing/invalid saved agent or its pinned model/variant, including active and parked tabs restored before Home, remains explicitly unavailable; no silent sibling/default fallback, automatic preference reset/rewrite or policy widening. Invalid choice refuses turn/headless before generation/tools; explicit valid choice recovers and survives restart/reopen. A malformed security-critical definition/policy never grants default authority. Qualification includes the current non-root user, real inherited HOME/XDG/PATH, current Location and untouched config sources with both existing default native data and fresh isolated --data-dir; not just hermetic fixtures or --help.
  - Primary evidence: source-derived fake-service actual-binary PTY/headless regression and captured recovered request identity; rebuilt retained target/release/oc bare PTY startup on the real current store plus the same real config environment with fresh --data-dir, both under bounded strace. Record redacted config-root/native-store access, typed failure/selection facts, pre-prompt local interaction, owned process/terminal cleanup and effect counters under docs/TEST_PLAN.md E2E06. No paid generation in the user's workspace.
   - Status: verified
   - Evidence: evidence/T51/inherited-fixture.md and evidence/T51/real-user.md; evidence/T51/report.md distinguishes fixture, real-user existing/fresh and recovery PASS. Full A13/product readiness remains separate.

### Error policy

| Outcome | Scope and continuation |
| --- | --- |
| Optional MCP config/connect/catalog failure | T46 owns failed server inventory/cleanup; application and healthy siblings continue. Failed relist retains the healthy catalog by MCP07; connection close/disconnect retires unavailable tools at safe boundaries. |
| Unsupported/failed compiled plugin | Reject only its capability with visible failed diagnostic; no resolver/JS execution. Healthy native aliases survive. |
| Selected provider/discovery/credential unavailable | Local TUI works; selection remains explicit, generation request fails until admitted recovery/selection. No implicit paid fallback or doubled retry layers. |
| Saved missing/invalid agent/model/variant, including restored active/parked tabs | Keep unavailable identity and safe cause; local Home/history/selectors work. No automatic selection/deck reset or sibling fallback. Turn/headless refuses before generation/tools; explicit user selection repairs only its admitted scope. Mandatory policy/trust admission is not bypassed. |
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
- Real-user qualification never edits original global/project/.opencode config, imports
  or modifies the TS database, resets native data/prefs or tests paid generation in
  the user's workspace. Native owned lock/WAL/recovery/startup-trace writes are expected,
  not a read-only launch; no automatic rewriting of selections/history to get green.
- Raw PTY/strace remains gitignored, bounded and secret-safe per runbook; no payload,
  argv/env/config dump, runner-auth extraction or attach to unrelated processes.

## Change Envelope

- Expected files/symbols: adapters config/composition/application diagnostic and readiness
  projections; core runtime request admission as needed; binary TUI/headless startup
  consumers. T44 consumes owner facts; no UI-owned network/process/init or config writes.
- R4 seams: Effective::set_agent and application_selection resolution vs strict turn
  admission; tui_cmd initial_state/restore_views/load_tab; nearest owner tests and
  actual-binary PTY/headless tests/support for deterministic and real-user profiles.
  Reuse native selection/storage owners; do not make selectors a new authority owner.
- Allowed: minimal typed startup/readiness/inventory changes and source-derived fixtures,
  actual-binary integration tests/report. New persistent schema/dependency/framework is
  not preapproved: prove a blocker to R1–R4 and record smallest justified expansion first.
- Plan amendment: GOAL.md, this spec, planning/acceptance.json, planning/tasks.json,
  docs/TEST_PLAN.md, roadmap/M8.md and derived progress/NOW.md text only; preserve
  canonical progress state, historical checkpoint leaves/reports and unrelated dirty work.
- Coordination: T46 owns MCP09/MCP10/MCP08, T51 owns CFG09/CFG10/UI07/E2E06; share a minimal
  diagnostic shape without whole-task done-dependencies. T44 independently owns VIS42
  and config/startup transitions in VIS19/VIS40. Existing detailed owners are unchanged.

## Historical execution checkpoint

- Closes: remaining R3 → R4, without relabelling delivered R1/R2 evidence.
- Smallest next action: finish current safe fatal-diagnostic boundary; reproduce saved
  missing-agent/tab restoration with source-derived E2E06 fixture, then separate local
  unavailable selection projection from strict pre-effect request admission.
- Next slices: explicit repair/restart fake-service proof → rebuilt debug/release gates
  → real-user existing/fresh native store PTY/strace qualification → T44 visual consumer
  qualification only after explicit resume, not a whole-task backend dependency.
- Verification: nearest affected crate tests; actual binary CFG09/CFG10/UI07/E2E06; impacted
  regressions above; workspace fmt/clippy/tests/build for cross-crate/final acceptance.
  Use offline fake services; existing mandatory live gates still apply at final closure.

## Historical state and material decisions

- 2026-09-27 RECON: disabled MCP environment fails strict loader before network; unknown
  plugin and selected-provider composition can fail before TUI; spawn_diagnostic loses
  actionable detail. Donor isolates service/plugin failures; initial filesystem/trust
  failures are not universally recoverable.
- 2026-09-27 docs-only checkpoint froze this plan, not implementation; at that point
  T51 was todo and T44 active. Previous task statuses/PASS reports were unchanged.
- D21 supersedes only app-wide optional-service rejection and admitted local-MCP minimal
  env/lazy-start policy; shell/security/quarantine/resource/discovery gates remain.
- 2026-09-29 inspection: current native preferences/tab deck retain an agent absent from
  current definitions. set_agent reports admission/invalid_definition/agent.entry.model;
  selection resolution propagates it through restore_views/load_tab before Home. This
  is a native stale-selection gap alongside inherited TS config, not a TS DB migration.
  Safe diagnostic improvement alone does not prove local startup availability.
- 2026-09-29 amendment: R4/E2E06 approved; T51 already active, R1/R2 have delivered
  evidence and R3 has uncommitted work. T44 PAUSED, T53 separate. Frozen R1–R3 rows and
  old PASS reports are not rewritten by this planning checkpoint.
- strace --help exit 0 observed during read-only inspection exercises clap/help only,
  not config admission/TUI/storage. Full real-user PTY/strace and E2E06 remain NOT_RUN.
- Blocker: none for plan delivery; runtime qualification remains pending, no new PASS.

## Completion

R1–R4 verified in evidence/T51/report.md. E2E06 includes the source-derived fixture
and both mandatory real-user data profiles with bounded PTY/strace; actual existing
store lock was available, choices/history remained unchanged, fresh store stayed
empty. Current compiled-source workspace 1297/0, Python47, post-build native28 and
independent profile metadata audit passed. Backend completion does not qualify
VIS42 or product READY; doc validation proves structure only.
