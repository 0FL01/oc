# Test plan — общая методика

Канонические общепродуктовые scenario IDs и expected behavior хранятся в `planning/acceptance.json`; T44 task-local VIS specifications — в `tui-recovery/ACCEPTANCE.json`, без дублирования в общем registry. Authoritative task owner — только `planning/tasks.json`. Executable tests/reports используют соответствующие IDs; повторный regression run не создаёт второго owner.

`A01`–`A13` — отдельный namespace high-level gates из точных headings `GOAL.md`.
Task `tests` может ссылаться на такой общий gate (как T44/T45), но это не detailed
scenario и не второй owner. Unknown IDs, duplicate references, shadowing gate IDs
и multiple owners detailed acceptance по-прежнему отвергаются validator.

## Команды качества

После появления workspace полный offline gate выполняется в T29:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --locked
target/debug/oc --help
```

Во время реализации запускать ближайший targeted test и только применимые crate/workspace checks. Live tests исключены из default `cargo test` и запускаются явным opt-in harness. Missing tool/Cargo manifest = fail/not-ready, не skip-success.

## Эталоны и недетерминизм

Clock/IDs/RNG/remote streams заменяются fake fixtures; normalized timestamps/IDs сохраняют identity, порядок независимых операций не навязывается. Данные с секретами в fixtures запрещены. Производный fixture хранит source path/commit/license и normalization recipe.

Mock-provider tests проверяют протокол/state machine, а не интеллект модели. Live coding acceptance проверяет behavior/files/tests, не точную формулировку ответа. Tool success нельзя выводить из финального текста без tool events и результатов проверки.

## Live model и campaign

При наличии `OC_TEST_MODEL` использовать exact provider/model-id и требовать его presence в effective catalog. Иначе использовать explicit configured default; только test harness может детерминированно выбрать первый подходящий discovery ID. Selection записывается до request и не становится product default. Variant — explicit `OC_TEST_VARIANT` либо provider default; disabled/unknown capability даёт blocker, без угадывания по имени.

T13/T20/T25 готовят offline-tested live harnesses; T16/T27 исполняют их. T27 также допускает минимальные offline-tested исправления своего harness после backend audit; обязательные gates и live envelope не меняются. Один durable campaign хранит opt-in, request/search counters, input/output totals, elapsed time и external watchdog/cancel result. Смена модели после failure не автоматическая. Catalog metadata не доказывает wire/tool/reasoning support.

## E2E fixture

Маленький Rust crate содержит намеренный bug, tests и protected public API. Offline и live workflows читают код, исправляют его через `apply_patch`, выполняют `cargo test` через `bash` и проверяют diff только expected paths. Live campaign повторно открывает ту же session, выполняет следующую команду и не сопоставляет exact natural-language text.

После закрытого tool span вызывается compress; после restart проверяются retained fact, reduced outbound context и следующее локальное изменение. Webfetch и codex_web сохраняют source info. Browser default disabled обязателен; actual browser smoke только explicit opt-in.

## Soak и память

T28 сначала фиксирует короткий baseline и measurement windows, затем до qualification замораживает достаточно длинный synthetic workload для проверки history/output/queue/cache/process behavior. Нагрузка не использует live paid prompts. Проверяются cold/warm baseline, peak, post-idle, PSS/RSS, task/process/queue/blob/cache/DB/WAL counters, crash injection, scroll/paging и cleanup.

Численные thresholds выводятся из baseline до optimisation candidate и не повышаются для сокрытия регрессии. Требуется отсутствие linear retained-history growth; произвольное заранее заданное число turns/sessions не является product contract.

## Canonical effort ordering qualification — T47/VAR01 + T44 (approved 2026-09-27; pending)

VAR01 belongs only to T47; DISC06/T14 and UI02/T39 retain ownership. T44 VIS09/VIS29
own presentation, T50 TOOL18 owns the model-lookup consumer; no duplicate detailed IDs
or whole-task done-dependencies. Contract: docs/CONTRACTS.md Canonical effort ordering.

- Nearest shared-helper/effective-snapshot fixtures: permutations of known levels
  (including max before xhigh), supported subsets/holes, missing/all-disabled entries,
  exact reserved default vs named none, explicit effort/name conflict and absent vs
  unknown effort, custom aliases/equal-rank ties/case/whitespace. Known ranks always
  progress none/minimal/low/medium/high/xhigh/max; aliases/custom relative source order
  and exact IDs remain. Local overrides rank final merged effort/disabled values;
  discovery input/allowlist remains unchanged. Do not replace the existing zeta/alpha
  unknown-effort cycle regression with an alphabetical expectation or repeat the same
  matrix at every layer. Existing validation remains authoritative for malformed data.
- Actual rebuilt binary PTY: variant picker and complete Ctrl+T round trip traverse
  the same owner order in Home/session, Default before/after named choices. Shortcut
   neither submits nor changes model ID/text draft; read-only/authority, empty/all-disabled
   no-op and explicit stale-cycle → Default remain. Live-selection clarification replaces
   model/variant busy refusal only: choice is draft, captured send/blank Enter commits,
   next prepared request of the same task receives exact committed effort while old
   request/tools keep actual variant. Reuse TOOL12/PRM01 switching receipts, not another
   VAR01 owner. Default adds no
  variant overlay. Standard-name rank without effort must not manufacture wire effort.
- Reorder/refresh/session switch/reopen/process restart retains choice by exact ID,
  not sorted index; removal/disable preserves existing visible retirement diagnostic
  with no automatic substitution. Immutable in-flight generations/profile/child
  selection and discovery publication regressions remain required.
- VIS09/VIS29 paired running-original/native full styled-cell/PNG/cursor captures use
  identical canonically supplied order to qualify geometry/colors/focus/draft and
  selection. With identical **unsorted** supplied data separately show and label the
  approved native order difference; donor keeps declared order. Do not mask rows,
  rewrite historical evidence/baselines or claim universal donor pixel PASS. Shared
  VAR01 wire/durability evidence is reused, not duplicated; TOOL18 consumes the same
  ordered view while retaining own-provider/newest-family model order.

Targeted affected-crate checks and workspace fmt/clippy/tests/build at integration/
final acceptance supplement A03/A04/A08/A13; no extra paid campaign. Plan validation
proves document/registry structure only; new behavior and visual results pending/NOT_RUN.

## Built-in profiles / default MCP access — T45 (approved 2026-10-01; pending)

Contract: [R3/R6/R8/R10](goals/2026-09-21-config-compat-and-subagents.md#built-in-profiles--default-mcp-access--approved-2026-10-01-pending).
Extend existing CTX02/PRM01, not new IDs/tasks. Default MCP Allow is a native
permission baseline for actual admitted registry entries; Explore MCP access differs
from pinned OC2. Plan is a real primary mode, not a fixture named plan. Read-only
RECON is source evidence only; new behavior is pending/NOT_RUN until qualification.

1. **No-custom-profile fixture (PRM01/CTX02).** Real loader with isolated admitted
   HOME/Location/data root, static synthetic provider model, no agent definitions,
   fake HTTP/stdio MCP catalogs and no MCP permission overrides. Allow ordinary test
   operations/subagent invocation explicitly as required by unchanged native defaults.
   Assert Build/Plan primary and General/Explore subagent, correct automatic catalogs,
   actual child launch/own system/base fallback and no parent-system/transcript copy.
   Use one supplied-field override fixture to prove inherited mode/system/restrictions,
   plus hidden/disabled/caller-denied eligibility through existing R6 tests.
2. **Default all-MCP chain (CTX02/PRM01).** Rebuilt actual binary sends a parent
   delegation, General and Explore each invoke configured fake search and a distinctly
   named arbitrary non-web MCP tool, receive the actual bounded result and continue.
   Plan invokes the same two admitted tools in its own root request. Record exact schemas,
   guidance/preview, HTTP or stdio tools/call counters, matched call/results and next
   provider input; default Allow has no approval request. No server-name/search-suffix
   allowlist, `permission:"allow"` masking a missing MCP default, config edits or paid
   API is acceptable proof. Reuse existing MCP harnesses, not another daemon/relay.
3. **Explicit policy agreement (CTX02).** Central, parent and profile per-tool/
   wildcard restrictions keep their normal independent boundaries. Use the real
   wire (`server__tool`) and compatible (`server_tool`) identities to prove Deny
   removes whole-action schemas/guidance and tools/call is zero even for a scripted
   forbidden call. Ask is conditional in preview/catalog, server guidance never
   declares it unconditional; genuine approval precedes the call. Headless with no
   consumer refuses with zero calls. Reuse AUD42/SUB02 for resource/parent narrowing,
   not another full permission test matrix. Profile default exceptions never reorder
   an explicit custom wildcard Deny; unknown/unregistered actions still fail closed.
4. **Native restriction regression (PRM01 plus existing owners).** With MCP enabled,
   Explore native shell/file mutations/question/nested subagent remain unavailable,
   General question/nested delegation and native session-control ceilings remain.
   Use T50's prepared selected-family view for both file families, DCP10 for allowed
   own-session compress and explicit false/off/manual/Deny, and SUB01/SUB02 for real
   child continuation. MCP is not classified as read-only from server hints/names;
   default availability is not an external filesystem sandbox claim.
5. **Actual Plan lifecycle (PRM01/R6).** Build→Plan→Build through the existing owner:
   capture enter/leave near the correct request boundary, no duplication on identical
   selection and no automatic exit from implementation prose. Compare both selected
   mutation families using TOOL12/TOOL20 receipts; ordinary-file attempts have zero
   effects, an explicitly requested file directly in `~/.opencode/plan` has the real
   admitted effect, explicit Deny/preimage/trust/path constraints remain. No automatic
   plan-file creation on entry; captured reminder retains that requirement without
   a keyword-based authorization classifier. Verify custom system retains the separate
   reminder, no model/variant/MCP/autoaccept-based policy bypass, parent Plan ceilings
   constrain native child mutations, and context reconciliation after DCP/compact/
   Revert/reopen/restart has no missing Plan or stale leave-state instructions.
   Actual `oc run --agent plan` selects before prompting; omission on a resumed session
   retains its saved profile, no hidden Build substitution. A picker label alone fails.
6. **Catalog/recovery boundary (existing T46 seams).** Fake barriers update/relist/
   disconnect before next request; newly admitted registered tools receive the same
   default, denied entries stay denied and prepared requests retain their captured
   view subject to real lease checks. Disabled fake browser is zero-spawn/enable,
   failed/pending MCP gives no new schemas/guidance. Restart rebuilds admitted state;
   reuse existing MCP08/MCP10/AUD23 quarantine/cleanup/no unknown-effect replay proofs.

Use deterministic fake-provider/process/tool counters and current rebuilt debug/release
normal-binary headless/PTY evidence for the root/child/default and Plan flows. Run nearest
profile/permission/runtime tests, affected crates, then R5's fmt/clippy/workspace tests/
release build at integration/final closure. No new paid/browser campaign or reset of an
exhausted live allowance. CTX02/PRM01 remain only T45; T46 transports/lifecycle and T50
executors/selector retain ownership. T44 VIS06/VIS10/VIS17/VIS26/VIS39 paired qualification
is separate after explicit resume; no new visual gate or whole-task completion cycle.
Document validation is not runtime/visual PASS or permission to change progress statuses.

## T45 prompt/delegation/DCP qualification (approved 2026-09-27; pending)

Detailed specs SUB01/SUB02/CTX01/CTX02/PRM01/DCP10/DCP11/DCP12 in planning/acceptance.json
have only T45 as owner; supplements A02/A03/A04/A05/A07/A10/A13. No duplicated
owner assignment to earlier T24/T36/T40 or to T44 visual scenarios.

- SUB01 uses provider barriers to demonstrate simultaneous independent foreground
  children and background progress before parent completion, including command route.
  SUB02 injects crash/cancel at admission/completion/delivery; verify lineage, bounded
  cleanup, deduplicated notices and no unknown-effect replay, not exactly-once claims.
- CTX01 captures exact selected parent text/roles/provenance and immutable pack on
  background/restart/continuation. Invalid/foreign/reverted/future/unsupported/over-
  budget selection creates no child/input; compressed raw text is not a reverted branch.
  CTX02 captures independent DCP-off IDs, automatic-context guidance and policy-backed
  capability preview; no unselected history or quote-to-system promotion.
- PRM01 proves the actual shared root/child base/custom/environment/tool/skill/AGENTS
  lanes and initial/nested/changed/removed/restart/compaction/Revert lifecycle within
  immutable-generation/trust boundaries. Existing R6/R7 profile/host fixtures remain.
- DCP10 checks child default true/false/off/manual/deny/Explore, parallel-session state
  isolation and active task/pack protection/release/safe recovery. Runtime protection
  does not outlive its task; current explicit user protections are not silently bypassed.
  DCP11 follows the [2026-09-30 hot/cold contract](DCP.md#infinite-hot-context--optional-cold-path--t45r9dcp11-pending):
  standalone replacement and deliberate forgetting, not lossless flattening. Freeze
  the mixed workload and its kept/forgotten control facts before qualification. No
  lifetime call/block quota; keep operation safety/revision/cycle/permissions/turn
  guards without making forgotten history mandatory resident context. Never raise
  A10 caps or make a finite cycle count the product lifespan; one-compression E2E is
  not qualification of this outcome. Detailed minimum evidence follows below.
- DCP12 proves native omitted-field defaults 40%/55%/summaryBuffer=false, not only
  an example/local override. Freeze known synthetic context sizes and compare
  just-below/at-min, at/above-max with cadence due/not-due, iteration escalation and
  success reset/cooldown/no-gain. For context100000, summary20000 plus other40000:
  default false uses total60000 for max; explicit true uses40000 only for max, while
  both include summaries for min and actual model admission. Verify actual transient
  developer reminders/next request and committed state, not free model prose.
  Integer/percent/partial/provider-model overrides, malformed/effective min>max,
  disabled/manual/deny and existing child gates share DCP05/DCP07/DCP10 coverage.
  Rebuilt binary/fake-provider owner snapshots must agree on canonical provider/model,
  effective min/max, buffer mode and model capacity/fallback for known/missing/zero/
  partial metadata, restart and safe-boundary Location/config changes. Do not turn
  unknown discovery metadata into fabricated limits, use context=0 or confuse the
  55% reminder with an enforced input ceiling; preserve AUD41/DISC05/input/output
  guards. Config fixtures cover inline then dcp.json/jsonc within each admitted
  root, global→project→.opencode layers, replacement OPENCODE_CONFIG_DIR, comments/
  trailing commas, read-only sources and no auto-generated file. Reuse existing
  config/restart/resource evidence; no new paid campaign or duplicate soak matrix.
  T45 owns default/resolution/facts; T44 qualifies presentation independently.

Use source-derived fixtures/captured fake-provider requests/actual binary and existing
bounded live envelope, not days of paid prompts. Plan-only checks prove structure,
not these runtime outcomes; executed historical reports remain unchanged.

### DCP11 hot replacement / optional cold / manual compact (approved 2026-09-30; pending)

Use one frozen fake-provider campaign on the rebuilt actual binary, reusing
`dcp_atomic`, `context_bounds`, runtime context/compaction fixtures and
`oc/tests/dcp_runtime.rs`; a minimal PTY smoke proves existing `/compact` control
reachability. No new task/test IDs, cold-memory service or duplicated paid campaign.

1. **Kept versus forgotten.** Add bounded closed work between iterations and run
   `compress → continue → recompress → compact → continue → compress → restart
   → compact → continue`. Keep the current objective, changed requirement, chosen
   path/result and next action. Deliberately omit sentinels in a prior summary,
   superseded requirement, pointless investigation, large closed tool/media/opaque
   group and released runtime task/pack. A useful disproof reason can remain concise;
   there is no requirement to preserve every dead end forever.
2. **No resurrection.** Inspect captured provider requests and committed hot
   projections: omitted content is absent after each relevant transition, not merely
   hidden in the TUI or omitted from estimates. Retained call/result/reasoning groups
   are intact; forgotten closed groups are absent as whole groups. No archival logs,
   inherited suffixes or old checkpoints silently restore them. Restart uses the
   exact latest committed hot state. Explicit authorized read/context selection and
   conversation Undo/Redo/fork restoration are separate cases, not automatic recall.
3. **Cold path absent.** Run without user `.md` journals/checkpoint files or Git
   history, and assert no automatic journal/commit/export actions. Missing cold refs
   do not block compression or continuation. Native app persistence/history remains
   available but is not automatically attached to model input. Optional explicit
   note retrieval does not create a mandatory second soak or a fallback dependency.
4. **Repeated boundedness.** Cross the former dependency-depth pattern and more
   than 4096 historically covered messages with a small standalone live state.
   Compare identical hot wire for small/large inactive block/member/mark archives;
   measure loaded rows/bytes, live depth, peak/retained RAM, processes/tasks/queues,
   cleanup, DB/WAL deltas and I/O/copy amplification. Do not credit SQL-side transfer
   as constant-memory proof without actual measurements; raw durable disk growth is
   distinct from resident history growth and accidental superlinear metadata copying.
5. **Compact correctness and recovery.** Check resolvable active anchors, equal
   unchanged checkpoint/fixed/current lanes in gain measurement, fresh reminder with
   one cadence evaluation, straddling block followed by an advancing compaction, and
   reused `call_id` with different hidden/purged occurrences. Exercise host-memory
   overflow separately from model-budget overflow through the real `/compact`, then
   same-session primary continuation. Internal `run_compress` success or an admitted
   `/dcp-compress` turn does not substitute for the user-reachable escape path.
6. **Safety/outcomes.** Reuse nearest stale/cancel/cycle/foreign/revision/permission,
   active task/pack lifecycle and no-gain regressions. Invalid/no-gain does not mutate
   hot state or spend lifetime capacity; a broader eligible span/smaller replacement
   remains possible. A forgotten whole historical group need not be loaded first.
   Distinguish measured recovery/partial progress/current-state failure; no unavailable
   compress instruction, forced silent clipping or hidden multi-request summary loop.
   Raw records/known-effect journals remain immutable and unknown tools never replay.

Assertions use structured items/IDs/state and predefined fixture payloads, not
keyword interpretation of arbitrary LLM summaries. Report actual continuation and
reviewed code commit plus affected targeted/required workspace/resource gates.
Existing source fixtures remain donor evidence; intentional hot forgetting is an
approved native difference, not retroactive parity/PASS. DCP11 completion does not
close DCP10/DCP12, all T45, PAUSED T44 visual qualification or product READY.

## Autoaccept / CLI modes qualification — T44/VIS36 (approved 2026-10-01; pending)

Follow the [mode-completion slice](../tui-recovery/T44_CONTRACT_AMENDMENT.md#autoaccept-mode-completion--vis36-2026-10-01).
Basic native Once consumer is implemented; previous behavioral evidence is not a
current-HEAD run or full VIS36 parity. Extend nearest approval/core/runtime/PTY
fixtures, not a new auto engine/task/gate or another full negative matrix.

- **CLI and actual effects:** `--auto` plus hidden `--yolo` and
  `--dangerously-skip-permissions` map to the same Args.auto and admission path in
  bare TUI/`tui`/`run`, with supported global flag positions; help shows only `--auto`.
  Rebuild actual binary; fake provider emits a real Ask operation, independently
  inspect fixture bytes/process effect and permission_grants (zero auto rows).
  Deny+auto/aliases produces a denied result and zero effects/grants. Do not require
  Deny to force a nonzero whole-run exit: handled denial may exit 0. Conversely Ask
  without explicit headless consumer must actionable fail/nonzero even if admitted
  cli.json/jsonc session.permissions is autoaccept; zero effect/grants.
- **Transitions:** use provider/tool/approval barriers, not sleeps, to hold an
  already-pending root Ask. Prompt→auto resolves it Once and next Ask likewise;
  auto→Prompt makes the next Ask wait for manual reply. With CLI auto, Settings
  Prompt/reload never disables effective auto or loses/re-resolves a waiter.
  Track application drain/runtime registration/TUI two-step application to prove
  the transition; consolidate only a reproduced divergence, no speculative owner
  redesign. Save failure has no successful ack/false marker/new committed preference;
  reply failure/stale/cancel preserves or cleans the correct request/draft as required.
- **Persistence and precedence:** default Prompt, admitted ordered cli.json/jsonc
  sources, settings saved preference and effective CLI override are separate facts.
  CLI args never persist auto; JSONC unrelated bytes/comments survive Settings save.
  Verify reload and restart without flag adopts saved preference, restart with flag
  keeps auto. Reuse existing write-error/generation/reload tests rather than duplicate
  each configuration combination at every layer.
- **Children/questions/recovery:** same-run owned root plus foreground/background
  children auto Once uses original operation/session/agent/resource/Location/generation
  binding, including navigation/late events. Own child effective Deny/Plan/authority,
  foreign/stale isolation, changed preimage and cancel/shutdown remain fail-closed.
  Disclose native headless descendant coverage vs pinned OC2 exact launched-session
  event filter. Reuse TOOL15 actual-binary tests to prove --auto never supplies question
  answers: real TUI question still requires answer, headless no-answer consumer fails.
  Reopen/restart/recovery never replay approved effects or old pending requests.
- **VIS36 paired presentation:** after explicit T44 resume and behavioral receipts,
  actual rebuilt binary and running pinned-original use the same fixture/profile
  for Settings/palette/filtered entry/marker, Prompt pending→auto→resolved→Prompt/reask,
  CLI override and representative child routing/error frames. Full styled-cell/PNG/
  cursor captures verify exact draft/chips/focus restoration, no stuck approval and
  truthful tool/tab attention. Reuse existing profiles and 79/80,120/121 boundaries,
  no crop/mask/static-marker/native-only-golden PASS or tool×mode×width cross-product.

Primary owners: `crates/oc/src/approval_tests.rs` for owner/mode tests,
`crates/oc/tests/approval_binary.rs` for actual ELF headless effects, existing core/
runtime and PTY targets for independent lifecycle/child/UI risks. A binary-crate
unit test named real_binary is not itself proof of launching CARGO_BIN_EXE_oc.
Targeted checks → affected crates → required integration workspace fmt/clippy/tests/
build → rebuilt-binary and paired qualification. Reuse existing A02/A03/A05/A08/
A10/A13/permission/recovery gates and TOOL15/TOOL12/TOOL20 receipts as relevant;
no paid campaign, new model matrix or all-T43/T45/T50 completion dependency.
Behavior and visual results separate. This plan leaves VIS36 NOT_RUN/evidence empty,
T44 PAUSED/T50 active and historic statuses/baselines/reports unchanged.

## T44 DCP display qualification — VIS38 (approved 2026-09-27; pending)

VIS38 belongs only to T44 in its task-local registry, supplements A07/A08/A10 and
does not reassign T45 DCP10/DCP11/DCP12 or earlier UI04/DCP owners. First exercise actual
model/manual compress and independently verify committed run/block identities,
unique newly-covered messages/tools, gross removed/active-summary/per-run estimates,
recompression, smaller next request, no-gain/failure/cancel and restart. Then compare
native cards with original pinned DCP formatter payloads rendered by running OC2;
label source-derived display fixtures separately from actual runtime qualification.
Use full styled-cell/PNG/cursor evidence at existing profiles, representative long
Topic/summary, off/minimal/detailed, chat/status notice and showCompression. Validate
categorical bar and shared number/K/M formatter, including rounded-unit promotion;
record only approved M/promotion and native toast mapping as named unmasked diffs.
Freeze historical cards across reopen/restart, verify Undo/Redo and session/late-event
routing without new model messages or reexecution. Reuse VIS21/22/23/33 and T45
resource evidence as applicable; equal-active small/large archives must not grow
loaded presentation rows/caches/queues. No duplicated negative/viewport cross-product,
new live campaign or historical PASS waiver; backend and visual results are separate.

## T44 subagent delegation qualification — VIS39 (approved 2026-09-27; pending)

T44 owns only the task-local visual/interactive VIS39; T45/R3 owns SUB01/SUB02 and
the minimal typed lifecycle/history/family/control projections. First rebuilt actual
binary/protocol/SQLite proves foreground overlap/batch join, immediate background,
real Ctrl+B conversion, launch/current status, notices/results/continuation and
deduplicated recovery. Native response-close/schema-validation-before-admission
stays; no early child execution or incomplete-JSON/free-text status inference.

Then running pinned-original/native paired full styled-cell/PNG/cursor captures at
existing 80x24/120x40/160x48 profiles and representative Unicode/long labels prove
inline Delegating/running/continuation/Background, Thought/actual parent footer,
durable notices, semantic colors/attributes/hover/selection/error details, lower
Subagents composer/navigation/filter/interrupt/draft/focus and family/tab indicators.
Main barrier-controlled fixture: three children, parent answer while running, one
notice/continuation then two notices before follow-up. No sleep300 or paid campaign.
Require animation-on phase/state sequences plus component-specific off fallback;
static frames/native goldens/off-only evidence cannot qualify. Reuse SUB01/SUB02,
VIS15/17/21/22/23/28/31/33/36/37/A02/A08/A10 without duplicated negative/theme/viewport
matrices. Replay/restart uses current child state/persisted notices without duplicate
delivery/reexecution; bounded paging/family queries/queues and no periodic idle work.
Backend and visual results separate, missing reference BLOCKED_REFERENCE, no all-T45
completion dependency, duplicate subagent/UI task/store/framework, policy widening
or historical PASS. The approved 2026-10-01 T56 PTY exception is specified below.

### Full child / Shell / Terminals qualification (approved 2026-10-01; pending)

Follow the [VIS39 clarification](../tui-recovery/T44_CONTRACT_AMENDMENT.md#child-tui-shellterminals--уточнение-vis39-2026-10-01)
and [T56 R1–R4](goals/2026-10-01-native-session-terminals.md). SUB01/SUB02 remain
only T45; TOOL13 remains only T50; new TERM01 only T56. T44 owns VIS39 presentation
and shared VIS14/VIS35 complete grammar/style parity, not a duplicate runtime test.

| Primary evidence | Smallest direct qualification |
|---|---|
| SUB01/SUB02 extension | Real linked child open while parent waits; ordinary child accepted/text/reasoning/tool events and history reconcile once. Exact-child interrupt with independent sibling progress; source family/Location/job generation, parent return/draft/deck/focus and actual descendant approval/question binding. Ctrl+B/delivery/late/restart races preserve settlement and no execution replay. Preserve child new-turn/profile/model/Undo guards. |
| TOOL13 extension | Actual running inventory/status and live bounded output before terminal, same-PID admitted foreground conversion, selected-job kill, final flush after running-list removal. Source session/Location persists across parent move; no CancelTurn on an unrelated parent, repeated spawn/result/notice or lost process-group cleanup. Initial running tool result is not current liveness. |
| TERM01 | Rebuilt actual-binary PTY creates two real session PTYs, types known commands and raw control bytes, switches/selects/hides/reopens without kill/duplicate spawn. Independently verify process identity/cwd/sanitized env, actual child resize, bounded VT cells/cursor/output replay and ready-before-input ordering. First-click release/wheel/raw-key/leader focus behavior and draft restoration; exit/disconnect/shutdown/restart cleanup with verified process identity and no old-command restart. No model tool/daemon/JS/WASM plugin host or credentials/host-escape leak. |
| VIS14/VIS35 | Full donor U86 language/filetype/alias/query inventory and actual grammar/query asset provenance; finite representative token-span/style fixtures exercise every admitted grammar. One shared native parent/child Markdown/code/patch renderer matches semantic fg/bg/italic/bold/underline/precedence and U89 muted reasoning, with unknown-only fallback. Paired mixed-code/diff frames prove wrapping/streaming/theme/replay, line numbers/unified-split and bounded completed-block cache. No heuristic/plain-required-language waiver. |
| VIS39 extended | Running pinned-original/native full paired child transcript, Subagents/Shell/Terminals rows/picker/footer, live Shell output and right VT pane/cursor. Source keys/hover/mouse/selection/focus/interrupt/kill/input/resize have actual effects; tab labels are not invented buttons, hiding is not cancellation. Exact colors/attributes/blank cells/geometry plus existing animation-on/off sequences. |

One barrier-controlled canonical path shares the fixture: parent delegation → child
open while blocked → live child shell → Ctrl+B → Shell output/selected kill or child
interrupt → durable notices/parent follow-up; independently create/select/input/
hide/reopen/resize a PTY. TERM01 specifically selects the first of two PTYs, hides
(selection clears), toggles on (last inventory entry selected), and proves both
process identities survive. No visible terminal gives undefined picker selection/
initial Enter no-op; activation closes the lower composer before dispatch, including
child→parent close routing. Linux create/refresh failure keeps controls and source
error; only configured leader/active sequence bypasses raw-key interception. Qualify
actual reference target/route rather than assuming a child-local modal/pane.
Keep the existing three-child notice-order fixture and
reopen/restart/resource assertions; do not duplicate all states/widths/languages in
a Cartesian matrix. Actual process/provider/SQLite checks precede visual claims.
Existing 80x24/120x40/160x48 profiles include representative Unicode/light-dark/
custom-theme/focus/streaming states. Source PTY ANSI16/default fg/bg, plain Shell
text and grammar colors are different contracts. Linux `session.terminal` is a
resolved platform capability in U88, not an invented user option or startup spawn.

Donor daemon/server handoff and inherited process env are declared native lifetime/
credential differences, never a geometry/interactive waiver. Reuse VIS15/16/17/21/
22/23/27/28/31/32/33/36/37/A02/A08/A10; no new VIS gate/paid campaign/OCR sleep300.
Own-tab/VIS41, static widgets/native goldens/off-only frames cannot close the segment.
No masks/crops/tolerance/reference substitution; missing runnable original remains
BLOCKED_REFERENCE. This plan leaves T56 todo/TERM01 NOT_RUN, T44 PAUSED, T50 active
and all existing execution/evidence/baseline statuses unchanged.

## Session tab qualification — VIS39 / VIS41 (approved 2026-09-27; pending)

T44 owns the task-local tab slice in its amendment: existing VIS39 busy spinner and
new VIS41/V03 hover-marquee, no duplicate spinner gate or task. Own-session spinner
and title work do not wait for T45; family busy/attention/unread still require actual
SUB01/SUB02 projection/lifecycle evidence before full VIS39 qualification.

First nearest clock/state/render tests follow U46–U49/U56–U59: dots80 ms and actual
status/attention/numbers/off branches, title hover-width overflow, delay600, steps80,
one cycle through ` · `, stop/leave/reset, grapheme/cell widths and source fades/tints.
Repeated motion/status/redraw does not restart an unchanged hover. Animations off
keeps the tab first frame but continues stepwise marquee; the leading-opacity tween
jumps instead of interpolating. Check actual horizontal/vertical geometry and compact
rail no-marquee, not a universal static-title fallback. Separate visual hover from
action eligibility without weakening busy/permission/Location guards.

Actual rebuilt binary with bounded fake-provider barriers proves running→completion/
cancel and pointer hover→delay→motion→settled/leave without further input or extra
submission/tool effects. Then paired running-original/native full styled-cell/PNG/
cursor sequences at existing 80x24/120x40/160x48 profiles and representative short/
long/exact-hover-fit/Unicode titles compare matched phases, on/off, active/inactive/
busy hover, close-cell routing and resize. Reuse VIS05/06/11/21/22/23/28/31 and shared
family/attention evidence, no duplicate backend/theme×width×state matrix or paid
campaign. VIS31 measures idle after one cycle/finite fades and other deadlines settle,
not by agent-idle or animations-off alone; no per-widget polling/new clock framework.
Static ⠋ or the native fade/close golden cannot qualify. Missing executable reference
is BLOCKED_REFERENCE; preserve provenance/failed attempts, no crop/mask/tolerance or
baseline rewrite. VIS41 NOT_RUN/evidence empty and full VIS39 still open until real
qualification; approval leaves previous task statuses/evidence unchanged.

## Tool-output phantom caret / prompt blink — VIS16 / VIS31 (approved 2026-10-01; pending)

Only T44/R4–R5/V06 owns the [detailed cursor slice](../tui-recovery/T44_CONTRACT_AMENDMENT.md#tool-output-phantom-caret-and-prompt-blink--vis16vis31-2026-10-01).
Extend existing mandatory VIS16 and reuse VIS31, not a new VIS/task or whole-T50
dependency. P17–P22/U103–U105/D11–D12 record source facts and a candidate cause;
the user's transient flicker has not yet been reproduced by a new actual-binary run.

After explicit T44 resume, recheck HEAD/profile and establish RED with an actual
large multiline hoverable Shell/eligible expandable card from a bounded fake
provider/tool fixture. Retain a Unicode draft/caret inside the prompt; send real
SGR entry/rapid row moves/leave/re-entry, expand/recollapse and recorded-output
viewer scroll/resize/close. Hover itself never moves focus/caret or expands; view
actions send no generation/tool replay. Do not recover tool-truncated bytes or add
new fullscreen/execute UI as part of this bug fix.

Nearest terminal/output regression captures complete VT commands, visibility/final
placement, fragmented writes and draw-error restoration: unsynchronized output
hides before transient moves and places the cursor before showing it. The pinned
backend's internal Show-before-MoveTo must be addressed, not wrapped with one hide
and declared fixed. Qualify supported synchronized presentation and non-support
fallback; preserve Ratatui state and existing input-owner routing. Read-only result
overlay excludes underlying prompt caret; Search has its own, close restores draft.
Use existing normal/error/panic PTY restoration checks; final cursor position and
text-only render_screen alone cannot detect the transient defect.

Ordering trace/PTY proves command behavior, not frontend raster/blink timing. With
the rebuilt binary and same mature terminal frontend/profile, record at least three
full visible/hidden blink cycles in idle composer, continuous hover repaint at the
unchanged editor position, and restored composer. Preserve effective shape/color/
nonblinking/default settings; repaint must not keep resetting blink into steady-on/
steady-off or irregular flicker. Record timestamps/cycle cadence versus idle baseline.
Then full paired pinned-original/native styled-cell/PNG/cursor temporal captures at
matched phases and representative 80x24/120x40/160x48 profiles; raw bytes/static final
frames/native goldens/mouse-disabled/crop/mask cannot close this qualification.

Reuse VIS31 bounded queue/latency/settled-idle measurements and existing hover/scroll/
selection/focus regressions, no Cartesian matrix/new emulator/permanent repaint timer
or paid generation. Done requires both no phantom caret and normal prompt blink;
missing executable reference is BLOCKED_REFERENCE, never source-only PASS. VIS16/
VIS31 remain mandatory NOT_RUN/evidence empty; plan approval leaves T50 active,
T44 PAUSED, historical statuses/PASS/baselines and execution generations unchanged.

## Agent cycle and Tab bindings — VIS06 / VIS10 / VIS17 (approved 2026-09-30; pending)

Follow the [detailed primary-profile slice](../tui-recovery/T44_CONTRACT_AMENDMENT.md#agent-cycle-keybindings--уточнение-2026-09-30)
and U29/U76/U77. This extends existing T44 cases and T45/R6, not a new VIS45/gate.
Default OC2 Shift+Tab cycles forward, reverse is unbound, /agents and `<leader>a`
open the picker; plain Tab retains autocomplete. Explicit list/forward/reverse
canonical/legacy overrides use the existing config-generation/selection owners.

Extend the nearest app/input regression through terminal_key: Shift+Tab produces
the adjacent eligible SelectAgent with no modal and unchanged draft/cursor/focus.
Nearest events/config tests cover defaults, alias/leader/none handling and explicit
Tab-forward/Shift+Tab-reverse; focused slash/@ completion and forms/dialogs own input
before cycle. Use ordered owner catalog IDs with wraparound and empty/single no-op,
not hidden/subagent-only/synthetic-unavailable entries or hard-coded profile names.
Reuse busy/read-only/retired-choice and owner rejection assertions. Extend
selecting_agent_updates_owner_and_draft_without_selection_toast, not a second
selection/persistence engine or duplicate safety matrix.

Existing pty_t39 interaction/lifecycle fixtures send actual ESC[Z and configured HT
to the rebuilt binary under bounded fake-provider control: Home/session cycle,
wrap, /agents picker, completion priority, survivor draft/focus and restart choice.
Cycle alone sends zero generation/tool effects; a later request confirms the chosen
profile instructions/model/variant. Reuse T45/R6 Plan policy/reminder/replay evidence.

Then pair full running-original/native styled-cell/PNG/cursor default-cycle/picker/
reopen states at representative existing terminal profiles, independently checking
selected ID. U77 declares prompt Tab capture: measure actual configured Tab dispatch
on pinned OC2 before parity claims; document any deliberate native override difference
unmasked. No source-table/native-golden substitute, crop/mask, Cartesian matrix or
paid campaign. Missing reference is BLOCKED_REFERENCE. Existing cases remain mandatory
NOT_RUN/evidence empty; approval does not resume PAUSED T44 or switch active T50.

## Middle Click tab close — VIS44 (approved 2026-09-30; pending)

Only T44/R5/V04 owns VIS44 in `tui-recovery/ACCEPTANCE.json`; follow the dedicated
[amendment](../tui-recovery/T44_CONTRACT_AMENDMENT.md#middle-click-tab-close--vis44)
and pinned U74/U75. It is not covered by VIS39 spinner or VIS41 hover-marquee.

Extend the nearest `oc-tui/src/app/tests/tabs.rs` input regression: Middle Down
in the body of an inactive eligible painted tab returns its CloseTab without
activation or cross hit; Middle Up after the deck changes cannot close a survivor.
Use existing geometry/guard tests for vertical/compact and non-tab targets; reuse
left-cross/keyboard/palette, busy and save-failure owner assertions, not a duplicate
negative matrix. Middle Down must not engage the Left-cross five-second close hold.

Extend the existing actual-binary `oc/tests/pty_t42.rs` close/reopen fixture or
nearest retained PTY scenario with real SGR Middle Down/Up, inactive→active→last-tab
closure, survivor draft/focus, live Home, committed deck on restart and real history
reopen. Session/raw history is retained; fake-provider counters show zero new
generation/tool effects. Closing uses the existing native CloseTab owner/guards.

Then compare running pinned-original/native full before/hover/after styled cells,
PNG and cursor under identical idle fixture/state/profile, representative existing
80x24/120x40/160x48 and horizontal/vertical/compact layouts without a Cartesian
product. Independently verify target session identity and reopen. Busy-close is
a disclosed donor/native difference: this slice cannot bypass native guards or
claim full busy/navigation parity. No crop/mask/native-golden substitute, paid API
calls or new test framework; missing runnable reference is BLOCKED_REFERENCE.
VIS44 mandatory NOT_RUN/evidence empty; plan approval does not resume PAUSED T44,
switch active T50, change historical evidence or imply runtime PASS.

## MCP modal prerequisite and qualification — MCP08 / VIS40 (approved 2026-09-27; pending)

MCP08 in planning/acceptance.json belongs only to T46/R5; VIS40 is T44-owned in its
task-local registry. Prerequisite is the minimal status/control slice, not all-T46
completion or a reverse dependency. Reuse MCP07/MCP05/AUD23/A02/A06/A08/A10: actual
rebuilt binary with fake HTTP/stdio verifies initialize+catalog before connected,
current-Location/generation snapshots, genuine pending/connect/disconnect/retry,
safe errors, tool catalogs at safe request boundaries, disabled zero-spawn and owned
cleanup. Snapshot/resize/cancel remains responsive during turn execution; repeated
pending Space coalesces, late actions cannot corrupt a new Location/generation.
Reopen reads current facts and restart rebuilds from config without operation replay
or saved connected labels. Runtime toggles do not silently edit config or clear sticky
quarantine; unknown MCP tool effects never replay. D13 preserves visible per-server
failure while the turn continues; fatal cancellation/cleanup/caps remain regressions.

Then running pinned-original/native DialogMcp captures full styled-cell/PNG/cursor
at80x24/120x40/160x48 with identical state/profile/fixtures: empty, mixed statuses,
fuzzy search/no-match, pending→success/failure, retry/disconnect, error details/back/
copy/investigate, row/footer/keyboard/mouse focus, long Unicode names/scroll/resize.
Check v2 dialog surface/backdrop/blank backgrounds, bold, selected-status override
and actual effects/draft restoration. Canonical strings/status icons follow U50–U55,
not OCR Enabled columns. `Connecting …` has no component spinner/shimmer/fade;
owner-driven transition frames and no idle polling, surrounding animations/cursor
on/off as relevant. Reuse VIS10/11/19/21/22/23/31 rather than a duplicate full matrix.
Declare sanitized diagnostics and unsupported OAuth/integration capability before
capture; no fictitious sign-in, hidden rows, crop/mask/tolerance or native-golden PASS.
Missing reference remains BLOCKED_REFERENCE. No paid/browser campaign; fake configured
stdio exercises explicit connect. Backend/visual results and T46 R4 live stay separate,
failed attempts/provenance retained, existing statuses/evidence/baseline unchanged.

## Service config/startup/error isolation qualification (approved 2026-09-27; pending)

MCP09/MCP10 only T46, CFG09/CFG10/UI07 only T51 in planning/acceptance.json;
VIS42 only T44 in task-local registry. Existing detailed owners and high-level gates
stay intact. Contracts/field matrix and ordered slices: T46/T51 goals and roadmap/M8.
Doc/progress checks prove planning structure, never runtime/visual PASS.

- **MCP09:** derive legacy/canonical/global-timeout/precedence fixtures from pinned
  donor normalizer/migrate/schema. Rebuilt actual binary starts with original-shaped
  disabled chrome environment+timeout and records zero npx/browser processes/probes.
  Fake stdio reports actual argv/cwd/inherited safe sentinel env and configured overlay/
  PATH; use synthetic credential sentinels to prove admission/redaction, never real env
  dump. Assert per-stage normalized deadline effects with fake time/barriers, not long
  wall-clock waits. Invalid/unsupported recognized entries visible while healthy peers
  survive; valid disabled stays disabled. Reuse CFG02/CFG04/AUD22/MCP04/MCP05. MCP04's
  credential-free local env clause is narrowly superseded by D21; shell TOOL05/AUD28
  remains minimal. No external runner-auth access or implied trust/credential grant.
- **MCP10 + MCP08:** healthy/slow/failed barrier-controlled startup before first prompt,
  no all-server launch wait; responsive history/status, actual initialize/catalog
  counters and first/next request catalogs. Then real connect/disconnect/retry, close,
  late completion/Location/reload/cancel/shutdown cleanup and generations. Reuse
  MCP07/AUD23/MCP05/A02/A10, including last-healthy-catalog relist/quarantine semantics.
- **CFG09:** mixed native aliases and unsupported marker, no code/resolver/process/
  network for rejected plugin; healthy siblings and local application usable. Actual
  typed requested/current activation and safe provenance/error available before prompt
  and on reload/reopen; no generic JS/plugin framework or swallowed fatal local error.
- **UI07:** discovery/connect/auth/credential failure at cold startup retains local TUI,
  history/model selection/diagnostics. Capture explicit selected identity and zero
  generation/tool effects for unavailable prompt/headless error. Admitted retry/refresh
  or user selection recovers, next request uses it. Preserve DISC01–DISC10 budgets/
  retry/metadata/atomicity and empty-cold configured catalog; no automatic fallback.
- **CFG10:** shared structured source/field/service/stage/safe-code/action across entry
  errors and true fatal trust/policy/data-root/storage/recovery/cleanup/caps. Compare
  optional malformed document (complete mandatory config still safe) vs policy-critical
  malformed/unreadable source (non-success, no wider default policy), atomic failed
  reload and safe details/copy/investigate/CLI/--json. Synthetic env/URL/header/ANSI
  sentinels must not leak; no arbitrary remote-text/regex status classification.
- **VIS19/VIS40/VIS42:** before-prompt failed config/pending/connect/close/retry, actual
  plugin inventory/details and provider readiness/error/recovery. Backend effects/state
  proven above precede paired running-original/native full styled-cell/PNG/cursor
  evidence using U50–U55/U60–U65/U02/U49 and existing profiles/focus/draft/resize/idle
  checks. Declare safe-diagnostic/unsupported capability differences before captures;
  native fatal startup frame gets functional source/cause/action coverage, no invented
  donor frame. No duplicate negative/theme×viewport×state matrix, masked rows, native-
  golden PASS or new paid/browser campaign. Missing reference stays BLOCKED_REFERENCE.

New runtime evidence requires targeted affected-crate checks and workspace fmt/clippy/
tests/build at integration/final acceptance. A02/A03/A06/A08/A10/A13, T46 R4 and existing
mandatory live gates remain; historical PASS is neither edited nor reused as new proof.

## E2E06 — inherited TS config / real-user startup (approved 2026-09-29; pending)

Only T51/R4 owns E2E06; E2E05 remains T25's hermetic configured-workspace scenario.
This is mandatory T51/A13 qualification, not an optional replacement for runtime
evidence. Reuse CFG09/CFG10/UI07 and MCP09/MCP10 without reassigning their owners;
T44 visual qualification remains separate and PAUSED until explicit resume.

### Required profiles and assertions

1. **Source-derived deterministic regression.** With fake discovery/Responses/MCP,
   combine the current conflict's supported global + Location + .opencode sources,
   unsupported plugin marker, legacy DCP/compaction diagnostics, disabled MCP with
   environment/timeout, discovery 401 and saved missing agent (such as build-yolo)
   in active/parked tabs restored before Home. Use synthetic secrets, not copied user
   config/DB. Assert pinned precedence and independent diagnostics, responsive Home,
   stored history and agent/model/variant selectors. Cover saved missing/invalid
   agent/model/variant by owner facts, not keyword matching of an error frame.
   Unavailable identity/cause remains visible; no default/sibling fallback, automatic
   deck/selection reset, preference rewrite or wider policy. An attempted invalid
   turn and headless request produce typed pre-effect refusal and zero generation/
   tool calls. Keep mandatory malformed policy/trust/storage/recovery/cleanup/caps
   fatal; selected malformed agent must not authorize built-in/sibling behavior.
2. **Current real-user existing-store launch.** Rebuild and retain target/release/oc;
   run bare target/release/oc under bounded PTY/strace as the current non-root user,
   from the current Location with inherited HOME/XDG/PATH and actual config resolution.
   Do not env_clear, replace HOME/XDG/config roots, switch cwd or pass a fresh data-dir.
   Record which admitted global/project/.opencode roots and default native store were
   actually opened, without printing config/env values or sensitive absolute paths.
   Exercise Home, existing history, diagnostics and selectors before any prompt;
   stale parked tab/selection must not cause the reported startup error frame.
   Do not change user selections/history/config to make this test pass. Normal owned
   native lock/WAL/recovery/startup-trace writes are expected, not read-only behavior.
3. **First-native-start real-user launch.** Run the same rebuilt release binary,
   current user/Location and unchanged real config environment under PTY/strace with
   a fresh isolated --data-dir. This proves existing TS config coexistence without
   native prefs, not migration/import of the TS database. Both profiles 2 and 3 are
   required; neither substitutes for the other or for the fixture regression.
4. **Explicit repair and durable recovery.** On the controlled fake-service fixture,
   explicitly choose a valid primary agent/model/variant, send a request and assert
   captured identity/effect counters, then restart/reopen and verify that choice and
   history. No exact generated prose assertion, hidden fallback or uncertain-effect
   replay. This exercises generation/recovery without paid requests or test edits in
   the actual user's workspace; real-user profiles do not send generation prompts.

### Trace, safety and evidence

- Use a locally opt-in harness for real-user profiles, but require successful evidence
  before T51/A13 closure. Preflight non-root identity, current branch/binary revision,
  terminal/profile, config-root resolution and default-store ownership without secrets.
  Freeze explicit watchdog and output bounds respecting existing discovery/MCP budgets;
  do not shorten service deadlines to obtain green. A competing store owner is a real
  lock blocker, not permission to kill it, reset data or substitute only a fresh store.
- Trace only the launched oc and its owned descendants (-f), not unrelated processes.
  Capture config/store opens and locks, connect, process creation/exec/wait/exit and
  cleanup metadata. Exclude read/write/send/recv payloads and environment dumps; render
  execve/execveat arguments raw (e.g. -e raw=execve,execveat) to avoid logging argv/env
  contents. Redact sensitive paths/address details before a report; no raw user logs,
  config, database, secrets, auth headers or credentials in Git/copy/evidence.
- Pair trace metadata with typed inventory and deterministic fixture marker/call
  counters: disabled MCP means zero npx/browser spawn/probe, unsupported plugin means
  zero resolver/import/file-as-code/execution/network for that entry. Attribute any
  admitted enabled external MCP startup separately; it is an explicit dependency,
  not evidence of a forbidden JS host. Discovery failure is visible, not fatal local
  startup. Verify clean owned task/process shutdown and restored terminal state.
- Original user configs and TS database stay untouched; no native data wipe/migration
  or preference reset. Respect admission/credential-domain boundaries and existing
  quarantine/retry/atomic-generation rules. Reuse runbook campaign/effect limits;
  do not extract runner-auth or start an extra paid/browser campaign.
- Raw logs stay in bounded gitignored per-run storage: existing 16 MiB/log and 1 GiB
  own total limits, explicit truncation status. Sanitized evidence/T51/report.md records
  exact commands, rebuilt binary source revision, exits, profile-specific interaction/
  effect/cleanup results and trace findings. Separate fixture vs existing-store vs
  fresh-store vs recovery PASS/FAIL/NOT_RUN/BLOCKED; missing mandatory evidence cannot
  be labelled PASS. The observed strace --help exit 0 does not exercise config/startup.
  Temp HOME, --help, fixture-only PASS or fresh native data alone cannot close E2E06.
- Run nearest selection/startup tests and affected-crate checks, then required workspace
  fmt/clippy/tests/build and rebuilt debug/release regressions at integration/final
  acceptance. Docs checks prove registry consistency only; historical PASS and existing
  execution statuses remain unchanged by this plan delivery.

## T50 selected native tools qualification (approved 2026-09-27; pending)

TOOL12–TOOL21 in planning/acceptance.json have only T50 as owner; relevant high-level
A02/A03/A04/A05/A06/A07/A08/A10/A13 and existing detailed scenarios stay regressions,
not reassigned owners. Use pinned donor fixtures and actual binary calls/results;
helper-only tests, fixture table tool names and document validation are insufficient.

- TOOL12 captures policy-filtered root/child direct catalogs, canonical shell and
  legacy bash compatibility/deny, explicit MCP search and excluded native tools.
  The 2026-10-01 file-tools extension below adds exact model selection and mandatory
  next-request switch; universal no-edit/write assertions are narrowly superseded.
- TOOL13 uses process/provider barriers for foreground waiting and background
  launch/progress/automatic notices, timeout default/zero/explicit semantics,
  original-location completion after move, cancel/shutdown/crash/delivery. Reuse
  TOOL05/TOOL06/AUD27/AUD28 and freeze A10 jobs/output/queue retention checks.
- TOOL14 source fixtures cover regex/literal/path/include/case and glob hidden/ignore/
  scope/truncation with malformed/over-budget/path/permission negatives (TOOL01).
- TOOL15 exercises actual PTY option/multiple/free-form answers and dismissal, model
  continuation, stale replies/headless/--auto/cancel/restart. T50 behavior proves the
  real question owner and ordered answers in the next provider request. Deliver this
  minimal backend/answer-consumer slice before T44/VIS37 paired frontend/card/replay
  qualification; no dependency on completing all T50. VIS37 reuses TOOL15 runtime
  evidence and adds running pinned-original/native full styled-cell/PNG/cursor
  captures for actual pending/edit/review/reply-error/dismissed/answered states,
  single/multiple/custom and multi-question navigation, effective focus/keymap/mouse,
  composer/form drafts and representative tab-fit/long-text at existing profiles.
  Do not duplicate every type/viewport/state or backend negative. Native-only static
  widgets and permission approval/VIS36 do not qualify question visual parity;
  behavior and visual outcomes remain separate, existing protections unchanged.
- TOOL16 captures text/directory pages and real image tool-result continuation;
  PDF/unsupported/malformed/over-budget cases fail honestly. Reuse PROV05/PRM01,
  including nested AGENTS from directory reads, without a second loader.
- TOOL17 checks webfetch text/markdown/html/defaults/metadata/total timeout with fake
  HTTP and actual model calls; TOOL07/TOOL08/AUD25/AUD26 SSRF/Unicode remain.
- TOOL18 checks direct catalog lookup/static-dynamic-unknown metadata/paging/model
  retention and authorized rename/reopen/restart/foreign-target/no-effects. Reuse
  T47/VAR01 ordered variants, not an independent lexical sort; own-provider/newest-family
  model grouping/paging semantics remain. Minimal shared-view prerequisite, no all-T47
  completion dependency or duplicate ranking matrix/acceptance owner.
  The owner-approved 2026-10-01 user CLI extension below additionally requires
  actual `oc models` stdout/error/effect receipts; historical tool lookup/rename
  PASS does not qualify a missing CLI or a profile binding.
- TOOL19 freezes A/B configs/history/job placement and uses barriers/crash points
  before admission/placement/result delivery. Verify same ID, trusted generation,
  immutable source turn, destination requests without stale harness/opaque items,
  original background/child execution and MCP quarantine. AUD14 supersedes only
  permanent binding; direct cross-Location use without admitted move still fails.
- TOOL20 covers real edit/write semantics/shared mutation admission/durable effects,
  using the source fixtures and actual-binary procedure below. It is not a second
  owner of old patch/permission/DCP/crash tests or a visual PASS claim.
- TOOL21 adds the common configurable limiter/filesystem/read-search chain below.
  TOOL13/TOOL16 and AUD34/LOAD02/STORE04 keep original owners; their nearest receipts
  are reused, not copied/reassigned or relabelled PASS by this plan amendment.

Run nearest targeted evidence per minimal slice, then affected crate/integration
and mandatory final workspace gates, rebuilt debug/release binaries and existing
bounded live envelope. No new paid search campaign or live-budget expansion. Keep
historical reports and task statuses unchanged until real qualification.

### Tool-output filesystem continuation — TOOL21 (approved 2026-10-01; pending)

Finish line/source comparison: [T50/R10](goals/2026-09-27-native-tool-parity.md#tool-output--r10tool21-approved-2026-10-01-pending).
TOOL21 belongs only to T50; one bounded offline fake-provider campaign on rebuilt
debug/release `oc` proves real result/config/filesystem/provider behavior. Freeze
workload sizes/barriers/sentinel positions before execution. Helper tests and UI
`read_session_tool_output` are not the model `read(path)` acceptance surface.

1. **Config/counting fixtures.** Nearest config/result-owner tests use admitted global/
   Location/.opencode JSON/JSONC and prove missing/default2000/51200, supplied fields,
   later-section wholesale replacement (earlier4096 bytes/later40 lines →40/51200),
   empty section/defaults, safe provenance, invalid shape/zero/negative/fraction/string/
   overflow/byte-ceiling diagnostics and atomic failed reload. Barrier-held old job
   keeps captured settings, newly admitted work after reload uses new generation;
   model switch alone never applies new config. No user config/secret extraction.
   Count joined multipart UTF-8 bytes/lines, CRLF/trailing newline, exact boundary/
   over-limit, many short lines and one huge Unicode line. General head versus shell
   tail, bounded reserved marker + total64 KiB, tiny-limit empty preview/ref, exact
   loss versus producer-incomplete counters are explicit. Fake `truncated:false/true`
   cannot bypass common preparation; do not inherit donor bypass as desired behavior.
2. **Actual model chain.** Start a normal owned shell via actual provider tool call.
   Emit >1 MiB but <16 MiB normalized synthetic text, >2000 lines and >51200 bytes;
   put unique sentinel beyond old per-stream1 MiB and outside final bounded tail.
   Capture the very next provider request independently: bounded text body/notice,
   truthful complete artifact path/extent/status/read guidance, no full-payload copy
   or sentinel. Inspect actual regular artifact file/registered op-session identity
   and text facts under storage-owner checks. Fake provider calls advertised `read`
   at the later line page and `grep` with exact artifact path, obtains the sentinel
   in the next bounded tool result and continues using it. Independently verify
   request/call-result causality and shell side-effect execution counter=1; no exact
   natural-language-response or LLM keyword classification as proof. Exercise one
   configured low line/byte case through this same pipeline, not merely parser green.
3. **Common result/media boundary.** Nearest runtime/MCP/fetch/skill fixtures prove
   large admitted text/structured-text uses the same limiter even with producer
   metadata and multiple parts, while small result stays complete inline. Transport/
   input caps remain; rejected remote body is not promised saved. Preserve typed
   question/compress/session-control/mutation envelopes/confirmed effects, MCP isError/
   failed/partial/unknown and valid graph without cutting JSON or fabricating success.
   Mixed text/image or other media retains its separately admitted modality/accounting;
   invalid/over-budget media does not become successful partial text or a file bypass.
   Reuse TOOL16/PROV05/MCP07/TOOL17/TOOL11 nearest assertions, not a full campaign per tool.
4. **Storage/fault/access qualification.** Under the same Db owner prove concurrent
   reservations,16 MiB artifact ceiling/shared quota/temp/orphan accounting and bounded
   streaming/publication. Inject quota, IO/registration/fsync/rename failure and crash
   before/after publication/outcome. Captured prefix/state is honest, no full-path
   promise to unregistered bytes, no unbounded inline fallback/tool rerun; native
   fatal storage/cleanup and actual effect state remain non-success where required.
   Use an injected owner clock for7-day expiry and protected live writer/reader leases;
   expired/missing read is actionable while raw outcome/ref/ordinary referenced blobs
   survive. No wall-clock7-day wait, global prune or changed STORE04 expectations.
   Real read/grep negative fixtures cover foreign/unauthorized child/guessed/forked
   reference, effective Deny, symlink/file-swap/nonregular, arbitrary blob/SQLite/WAL/
   native directory/glob/mutation and escaped path; sentinels remain inaccessible.
   Exact admitted artifact >1 MiB works with bounded page/scan/time/hit/huge-line/next
   cursor behavior; no full-file allocation or nested AGENTS discovery. Existing
   artifact-backed pages under lower preview caps reuse validated source/extent refs
   and correct cursors, not recursive cold payload duplication. Existing
   project-file/data-root/path/permission tests remain, except the approved exact route.
5. **Shell consumers/recovery.** TOOL13 process barriers reuse the same capture for
   foreground/background/live output/final notice/viewer, same-PID Ctrl+B and final
   flush under completion/cancel races. Flood/slow disk/over-cap/quota/IO drains remain
   bounded and clean owned processes; output loss does not invent success or replay.
   Reopen/restart with existing store retrieves the registered artifact, bounded preview
   and status without execution. Same-task committed model switch, allowed session move,
   DCP/compact retain selected causal refs without loading full/forgotten captures;
   explicit read alone reintroduces a bounded page. Reuse TOOL12/13/19/DCP11 safety
   receipts rather than expanding their entire matrices or requiring all T45/T44.
6. **Resource/closure.** Reuse AUD34 fact-after-preview and LOAD02/A10 large-output
   measurements: freeze flood/history sizes and equal active projections; record peak/
   retained RAM, stream buffers/tail/queues/tasks/processes, disk/DB/WAL amplification,
   reservations and clean cancellation/shutdown. New oversized text has one cold copy
   plus hot bounded refs/previews, not full copies in SQLite/TurnLog/history. Ordinary
   legacy inline output and the existing3 KiB AUD34 scenario stay unchanged; only its
   universal full-inline interpretation for new oversized output is superseded. Never
   raise baseline/thresholds, skip tests or assert loss as success. Run nearest owner/
   affected-crate checks per slice, required integration/final workspace fmt/clippy/
   tests/build and rebuilt debug/release chain; existing mandatory live gates remain,
   but this output scenario creates no new paid campaign/live-budget expansion.

Current sanitized evidence/T50/report.md must cite implementation commits, binary
revision/commands/exits, captured request limits/artifact/continuation facts and shared
fault/resource receipts. TOOL21/runtime and existing T44 visual qualification remain
separate; statuses/historical reports/baseline are untouched. At plan delivery the
new scenario is pending/NOT_RUN; no qualification from docs validation or old UI2048.

### CLI models / profile binding qualification — TOOL18 + T45/R6 (approved 2026-10-01; pending)

Use the [R7 CLI contract](goals/2026-09-27-native-tool-parity.md#cli-models--r7tool18-approved-2026-10-01-pending).
TOOL18 remains only T50; the profile consumer is T45/R6 under its existing PRM01/
A03/A13 evidence. No new detailed scenario or duplicate owner is needed. The pinned
source is OC2 commands/handlers/models.ts, the available model endpoint, Model.Ref
and config agent normalization/importer, not V1 CLI documentation. Source observations
and native declared differences are separate from executed binary qualification.

- **Actual CLI output.** Rebuild and invoke debug/release `oc models` as independent
  non-TTY subprocesses. Capture exact stdout/stderr/exit, not only parser/unit tests
  or `--help`. Synthetic static/multi-provider fixture includes more than 20 models,
  repeated family metadata, slash-containing model IDs, colliding provider-local IDs,
  unknown limits/costs and excluded disabled entries. Assert complete exact full-ID
  lines, lexical ordering/final newline, no names/group headers/family reduction/
  secret/progress contamination. Healthy empty enabled snapshot is empty stdout/0;
  no default model produces neither ModelRequired nor a fabricated selection.
- **Catalog and admission.** Reuse admitted global/project precedence and original
  OpenProxy fake-service fixtures. Bounded success publishes current IDs; auth/
  timeout/invalid-response failure preserves truthful known metadata and reports
  incomplete required dynamic catalog with safe stderr/nonzero. Known static/public
  metadata without a generation key is independently listable, not generation-ready.
  Disabled source is no network; unselected unsupported entry does not trigger
  credential extraction/routing. Mandatory malformed source/policy/trust/data-root
  ownership when needed/caps remain fatal. Reuse DISC01–DISC10/CFG04/UI07 rather
  than duplicate their whole failure matrices. No new discovery retry policy.
- **No execution or selection effects.** Fake service counters distinguish allowed
  catalog GETs from zero generation/title/compaction requests; configured enabled
  MCP/browser process markers stay absent. Compare fixture config/profile bytes,
  existing session/history/prefs/selection and recovery records before/after listing,
  including valid-shaped saved unavailable selection. No session creation, prefs
  repair, permission grant or full application recovery. Cancellation/output failure
  is non-success and owned catalog work finishes under existing cleanup/caps; a
  partially written prefix is never labelled successful complete output.
- **T45/R6 binding.** Take an exact ID from the real CLI receipt, not a manually
  substituted expected ID. Load canonical `agents.<id>.model`, legacy `agent` and
  global/project Markdown fixtures through the real loader. Use one representative
  profile request/reopen/restart scenario plus nearest normalization fixtures for
  first `/`, remaining model slashes, `#variant`, structured selection and legacy
  separate-variant precedence; embedded/structured native choice wins. Capture
  actual provider/model/variant and profile-body sentinel in fake requests. Retired/
  unavailable model or disabled variant refuses before generation/tool effects,
  without sibling/provider fallback or config rewrite. Reuse T45 selection/body/
  trust tests instead of repeating this scenario on every layer or assigning TOOL18
  to T45. Profile live-switch/child authority is unchanged, not expanded by listing.
- **T53 seam/closure.** Future GO02/GO05 fixtures assert admitted Go/custom catalogs
  reach the shared provider-qualified read-view without key-dependent public fetch
  or duplicate source/cache/credential owner. Their qualification is T53-owned,
  not a whole-T53 prerequisite for basic OpenProxy listing. Run nearest affected
  crate/integration checks and required final fmt/clippy/tests/build gates. CLI and
  profile reports cite source revision, actual binaries/commands/exits and shared
  receipts; keep new scenarios pending/NOT_RUN until executed. No paid campaign,
  store import, baseline change, T44 resume or PASS from this planning update.

### T50 model-dependent file tools — TOOL12/TOOL20 (approved 2026-10-01; pending)

Source/finish line: [T50/R1/R9](goals/2026-09-27-native-tool-parity.md), pinned
U90–U102, including follow-up live-switch clarification. TOOL12/TOOL20 belong only
to T50; PRM01 remains T45, VAR01 ordering T47 and visual cases T44.
One bounded fake-provider campaign on rebuilt actual `oc` proves tool definitions,
calls/results and real filesystem/storage effects. Share its receipts with frontend
tests; static registry entries/renderer fixtures/helper-only green do not qualify.

1. **Selector and catalog.** Freeze synthetic model IDs for eligible `gpt-`, arbitrary
   unknown non-GPT, `gpt-4`, `gpt-oss`, combined exceptions and case variants. Compare
   exact case-sensitive donor predicate on selected model.id, not provider/display
   names or reasoning/metadata guesses. Verify all advertised schemas and permission
   filtering: eligible patch only, otherwise edit/write only; Deny/Explore/Plan/child
   ceilings and existing MCP/DCP remain. Configured namespaced MCP tools are not
   filtered just because a suffix resembles a built-in file tool. Convert old global
   no-write/edit assertions to selected-family assertions; preserve patch cases with
   an eligible synthetic fixture model rather than disabling their tests.
2. **Mandatory live model switch — TOOL12.** Freeze one actual-binary fake-provider
   autonomous task with eligible GPT A and non-patch B. Do not send a new user prompt
   between its steps or mutate a global registry/test-only model variable.
   - Hold A's provider request; use real picker to choose B and prove draft-only:
     composer B, owner still A, no switch/request/cancel/effect. Blank Enter in the
     existing ordinary composer commits B through the authorized owner while busy.
     Capture acknowledgement/selection event separately from actual request identity.
   - Release A to issue apply_patch and settle its real result exactly once under A's
     captured catalog/model, even though B is selected. A separate tool/Ask barrier
     case commits through the same owner while a call waits; preserve call/resources/
     preview/model identity and normal approval/preimage checks. Respect permission/
     form focus; do not turn their Enter into a hidden composer commit.
   - Next natural request in the same task is B: inspect model/variant, tools[],
     managed guidance, known/fallback budget/output cap, model-relative DCP thresholds,
     estimated wire and schema/context/cache fingerprints together. It receives A's
     retained ordinary call/result/outcome groups, without alien opaque/checkpoint.
     Execute real write/edit, hold continuation and commit A again without another
     prompt; next request is A/patch. Preserve immutable config/Location/agent scope.
   - Assert actual A/B/A request/assistant attribution in live and durable history,
     not current composer labels or one overwritten turn model. New call excluded by
     its issuing request fails before filesystem effects with a paired typed outcome;
     switching preference cannot retroactively exclude an already issued valid A call.
     Opposite-family child retains its own admitted selection/projection/authority.
   - Reuse U101/captured-selection fixtures for message/command preparation behind
     earlier pending admission and later picker changes: sent choice/order and matching
     ack/events are exact; failure/stale echo cannot confirm a rejected choice or erase
     newer draft. Blank Enter adds no fake user text. With no pending work, draft/commit after final answer
     alone creates no request, interrupt, title request or tool effect.
   - Within existing retry allowances, hold one admitted retry or compaction rebuild,
     commit B and prove the next prepared attempt uses coherent B view while an already
     prepared A attempt retains A. Counters/quotas do not reset because choice changed.
     Reopen/restart verifies latest committed choice, not unconfirmed draft, and no
     replay of settled or unknown tools. Share receipts with PRM01 and visual consumers,
     not a duplicate matrix or second switch owner.
3. **Context/recovery.** Reopen/restart same session, compress a closed span and run
   native compact before another prompt. Inspect tools and managed lanes, not a
   blanket ban on the word edit/write in immutable historical content. Old calls/
   results keep actual names and valid retained causal structure under the corrected
   model-compatible wire_history projection; no result-losing fresh lane, cross-tool
   translation, alien opaque continuation, obsolete managed schema/guidance
   resurrection or closed/unknown mutation replay. Use raw-history invariants and
   bounded hot-context fixtures, not full-archive reads or a new context cache.
4. **File bytes and result contract.** Source-derived edit fixtures pin exact-first
   precedence, typography fallback, trailing-whitespace line match, nonoverlap and
   unique/all/count; no-match/ambiguity/empty-identical-old error, empty-new deletion,
   Unicode/CRLF/final newline/BOM. Write creates, overwrites and empties files, creates
   parents, preserves intended text bytes/BOM and existing allowed mode. Verify
   actual path/operation/existed/resource/replacements/files, filesystem bytes and
   saved bounded result-derived diffs, not success prose or synthetic patch Add.
   Preparation reads bounded preimages under mutation policy; no prior read call
   becomes a new authorization prerequisite. Existing strict patch matching remains.
5. **Approval/safety/durability.** Actual Allow and Deny calls, real consumer Ask with
   pre-effect before/after diff, then a changed existing or absent target while waiting.
   Valid reply must recheck approved bytes/identity/absence before writing; stale
   preimage refuses unapproved effect. Legacy aliases/grants/resource/home/Plan/child
   narrowing and path/symlink/own-data-root/protected sentinels hold for both tools.
   DCP uses parsed path, not content text as pseudo-resource. Intent/outcome crash
   points and storage failure distinguish failed/confirmed/unknown, never replay;
   call/result pairs, effects and historical cards survive reopen without current-file
   reads. Reuse TOOL02–04/09/10, AUD04–06/16/42, DCP04 and A10 measurements; do not
   replicate the same negative matrix at every layer or revise baseline thresholds.
6. **Separate visual qualification.** After the minimal backend data/real consumer
   slice and explicit T44 resume, VIS35/VIS36 use these actual outcomes with running
   pinned original/native Write/Edit/ApplyPatch components, full styled-cell/PNG/cursor
   at established profiles, state/diff settings and 120/121 boundary. Include distinct
   `# Wrote`, `← Edit` and patch labels, grammar syntax/gutters/attributes, pending/
   completed/error/approval and metadata-only replay/model switch. Reuse the same
   live-switch receipts for VIS09/VIS29 draft/composer/commit and VIS17 actual A/B
   request/footer attribution; VIS36 approval remains on its original call after switch.
   Backend green is
   not pixel parity; native-only golden/crop/mask/fake patch cards cannot close VIS35.

Run nearest owner tests and affected crate gates per slice; integration/final
fmt/clippy/workspace tests/build/debug-release binary qualification remains required.
Existing A09/read-apply_patch-bash/E2E/live gates are retained with a compatible,
catalog-admitted selection, not silently swapped to edit/write or skipped. Non-GPT
qualification here is offline actual binary; no new paid campaign/full-model matrix
or live-budget increase. Preserve historical reports/statuses; planned extension is
pending/NOT_RUN until these observations exist, no READY claim from doc validation.

## Evidence cadence

- Slice: targeted test; checkpoint только при handoff/block/non-idempotent external action.
- Task finish: все назначенные IDs и один sanitized `evidence/Txx/report.md`.
- T25/T26/T28/T29: по одному соответствующему offline/integration campaign, без дублирующих suites.
- T16/T27: bounded explicit live campaigns.
- T30: roll-up evidence и final-code-commit AUD38/AUD39 requalification после backend changes; прежний T42 PASS не заменяет проверки нового кода.

## T52 — файловая структура без изменения поведения

T52 проверяет ARCH01–ARCH05 из `planning/acceptance.json`; это не новые product gates A14+ и не повторное присвоение старых acceptance IDs. Поэтапная матрица и commands — в `goals/2026-09-28-code-slices.md`. Принимается реальный перенос с сохранённой test discovery, scoped visibility и ownership, а не только новый текст AGENTS.

До/после переноса с одинаковыми Cargo options/environment сохранить `-- --list` для затронутых lib/bin/integration targets и отдельно список `-- --ignored --list`. Сверить target + полное имя + ignored membership. Допустимые изменения module prefixes перечислить явно; тестовые функции/assertions/fixtures не удалять ради равных counts. Сохранение одного количества не доказывает сохранение набора. Новые диагностические tests учитывать отдельно. Filter после переезда обязан запускать ожидаемые тесты, не заканчиваться тихим `0 tests`.

Ближайшие suites выполнять в каждом срезе, полный workspace gate — после интеграции. В `oc` unit tests остаются `--bin oc`; их перенос не требует добавлять lib target. Существующие `--test runtime` и `--test pty_t39` сохраняются; это позволяет не размножать Cargo processes/fixtures. `cargo test` библиотеки для integration target не включает cfg(test)-only API: нельзя так скрыть ScriptDriver или открыть private API через новые public методы.

Чистое перемещение не меняет golden/expected output/timeout/skip/ignored/resource thresholds. Любая наблюдаемая разница — расследовать отдельно, не обновлять baseline автоматически. Для затронутых UI paths использовать существующие deterministic PTY/full-frame сценарии; compare styled cells, cursor и PNG там, где этого требует их контракт. Source-path metadata может измениться, а ожидаемое поведение — нет. Недостающие в присланном архиве raw captures — `NOT_AVAILABLE_IN_ARCHIVE`, не новый PASS или требование восстановить всю многогигабайтную историю.

Размерный helper из T52/R0 warning-only при превышении 5k; его тесты проверяют подсчёт/фильтрацию/Git changes и не превращают ориентир в hidden hard gate. Новых SHA256-based regression gates нет; используем Git identity/diff и семантические проверки. Старые frozen locks и используемые продуктом digests не переписываются в этом scope.

## T53 — OpenCode Go и единые provider credentials (approved 2026-09-29; pending)

[Frozen contract](goals/2026-09-29-opencode-go-and-provider-auth.md) и GO01–GO06 в
planning/acceptance.json имеют только T53 как owner. Одна integration task, не задачи
на каждый срез и не reassignment DISC/PROV/VAR01/UI07/T44. Reuse квалифицированных
минимальных T51 readiness/diagnostic, T47 variant и T50 catalog consumers; T44 PAUSED.

| ID | Минимальная primary evidence surface |
| --- | --- |
| GO01 | Synthetic Key/OAuth tagged storage + reopen; transactional account lifecycle, precedence и provider/endpoint authority. None vs missing Key vs unsupported OAuth; config URL change не получает старый secret, no-auth loopback capture без credential headers. Redaction/root/DB/WAL guarantees. |
| GO02 | Public models.dev fixture/fake clock: no-key fetch без auth headers, source-qualified cache/stale single-flight/offline/last-good/atomic failures; route aliases, actual reasoning overlays, local merge/retirement и auth rejection independence. Existing OpenProxy oracle unaffected. |
| GO03 | Parameterized fake server: все три protocols text/tool/reasoning/usage, bounded splits/terminal/error/cancel и typed HTTP/SSE/header lowering в единый T54/RET01 retry owner, включая safe continuation после partial output. Exact auth/defaults и captured metadata main/follow-up/title/summary-compaction/child/retry, concurrent identities/held binding. Custom Chat/None и OpenProxy regressions используют те же owner fixtures. |
| GO04 | Legacy/new journal → SQL presentation/DCP/fork/reopen → next request: protocol/binding/checkpoint guards, ordered complete pairs/MCP indices и raw-history integrity; switch не переносит alien opaque items или unknown effects. |
| GO05 | Actual-binary fresh offline/no-config/no-key PTY → masked cancel/paste → labeled accounts/source truth → acknowledged Go picker → explicit qualified selection → fake generation/cancel/reopen. Provider-ID collision, retired choice, pinned held request и headless zero-effect refusal; fatal policy/storage boundaries retain non-success. |
| GO06 | Factual report final fmt/clippy/workspace tests/locked build/help + bounded real Go text/tool representative per protocol и known route-conflict probes. Fake/live/NOT_RUN и exact binding/counters различаются. |

Не дублировать весь matrix на каждом layer и не добавлять тест на каждую ветку.
Для owned new behavior нужны nearest targeted checks; final cross-crate gates —
команды раздела «Команды качества» выше. Historical PASS/документный validator
не квалифицируют новые runtime behavior или full pixel parity.

Live journal/opt-in/envelope **до** requests: ≤24 physical generation HTTP requests
всего, включая retries/title/compaction/children/protocol probes; output ≤2048 tokens
на smoke. Выбрать exact present representative каждого protocol, не sweep всех Go
моделей. Проверить dated `qwen3.8-max`/`qwen3.7-plus` models.dev Chat vs Go-docs Messages
conflict без hardcoded routing/automatic paid fallback. Unresolved mismatch не PASS;
recorded unavailable/protocol blocker не скрывать public catalog success. Missing/
revoked Go key блокирует live только; supplied key не попадает в artifacts/logs/Git.
Existing mandatory OpenProxy live gates остаются отдельной обязанностью.

## Provider-error retry parity — T54/RET01 + T44/VIS43 (approved 2026-09-29; pending)

[T54 frozen contract](goals/2026-09-29-provider-retry-parity.md) и RET01 в
`planning/acceptance.json` имеют только T54 как owner. Старые PROV06/PROV07,
AUD11/AUD13 сохраняют владельцев/регрессии; GO03/T53 доказывает применение
общей policy новыми wires, не отдельный retry-loop. T44 owns ONLY VIS43
в `tui-recovery/ACCEPTANCE.json` после явного resume; backend T54 не зависит
от завершения всей T51/T53/T44. Полный feature parity требует backend и
парных visual кадров; ни план, ни source-derived/native golden не дают PASS.

| Evidence | Минимальный прямой сценарий |
| --- | --- |
| RET01 policy | Таблица fake HTTP/SSE: 429 throttle vs exhausted quota, 402/401/403/400/413, 408/409/5xx, context overflow, body/code/event status и observed `x-should-retry`; numeric/date `Retry-After`, `retry-after-ms` priority/clamp, HTTP-200 SSE не наследует retryAfterMs, но видит HTTP override. Generic `providerError` event terminal. Clock+RNG фиксируют 2/4/8/10s ±20%, attempt 2…11 и cancellable sleep без реальных минут. |
| RET01 lifecycle | Один physical POST на adapter call, before-output same assistant и mixed failures с общей step allowance; after-output continuation сохраняет partial text/reasoning/settled results и новый assistant span, не исполняет partial args/unknown effects. Empty EOF, typed failed/incomplete, length/content-filter, exhausted budget, publish/storage/cancel/backoff и bounded physical request count проверяются владельческими тестами. |
| RET01 integration | Rebuilt actual binary fake-provider headless NDJSON и PTY: pending retry не terminal failure, persisted/reopen/parked notice не теряется, selected provider/model/headers/config pinned, stale/foreign events не изменяют view, cancel во время ожидания не отправляет следующий POST. A02/A04/A07/A08/A10/A13 и затронутые PROV07/AUD11/AUD13/security regressions не ослабляются. |
| VIS43 | Source-derived UI-clock/render + running pinned-original/native paired full styled cells/PNG/cursor для warning/spacing/footer/countdown→due→step start/reschedule, safe long Unicode error, quota/terminal/cancel, partial→new span→success→reopen старого completed span. Donor 44/100 + применимые утверждённые 80x24/120x40/160x48 и узкие wrap cases, без полного cartesian product; idle timer останавливается, tab/cursor/draft/busy остаются owner-backed. |

Использовать уже действующие affected crate + финальные workspace gates выше.
Fault injection только offline; новый paid campaign ради 429/5xx не нужен.
Ограничение ≤24 физических запросов принадлежит явно допущенной live
кампании, **не** production turn cap. Прежний `docs/CONTRACTS.md` обещал
суммарный физический счётчик, но native `rounds` считал только успешные
generations; RET01 обязан измерить выдачу всех запросов, согласовать её
с logical step/round пределами без магического нового потолка. T30 FINAL
ссылается на RET01, GO03 при допуске Chat/Messages и VIS43; никакого
history-PASS или NOT_RUN→PASS по обновлению registry.

## Completed Responses compatibility — T55/PROV09/PROV10 (2026-09-30; pending)

[Frozen repair contract](goals/2026-09-30-responses-tool-compatibility.md),
[current diagnosis](../evidence/T55/diagnosis.md). Detailed owners: PROV09/PROV10
only T55; existing PROV03/PROV04/PROV06/PROV08/RET01 and E2E02 keep their owners.
Historical fake retry PASS cannot replace current native real-API tool evidence.

| Gate | Required direct evidence |
| --- | --- |
| PROV09 reconciliation | Normalized completed done call + terminal completed `output:[]`, omitted field, full and partially repeated terminal output. Valid completed items survive once in stable order. Compare item id/call_id/name/args/index; conflicting duplicates, malformed output, partial JSON/delta-only call, announced unfinished call and unclosed stream never dispatch tools. Actual EOF, provider failed/incomplete, length/content_filter and local validation produce distinguishable safe typed outcomes; no blanket retry or synthesized completion. Existing length/opaque/caps/RET01 safety tests remain. |
| PROV09 native | Rebuilt retained debug/release binaries, deterministic fake server headless and PTY. Actual apply_patch → read → final, exactly paired function_call_output, file bytes + durable operations, no retry on valid sparse completion; negative stream has zero tool effects. Reopen/restart preserves history and never replays settled/unknown effects. PTY proves backend behavior, not T44 pixel parity. |
| PROV10 live | After offline gates, same actual native tool cycle on real OpenProxy in isolated fixture, first catalog-admitted incident `cx/gpt-6-luna`/`high`, then explicitly configured `.local/live.env` model/provider default (or explicit OC_TEST_VARIANT). Exact choices/deployment class/build commit and measured counters recorded safely. Verify tool intent/outcome + bytes + outbound result pairing + final + reopen; direct SSE/prose/mocked tools are not PASS. |

Use the existing durable diagnosis campaign: **4/24 generation**, **0/4 MCP**,
5 control requests consumed at the diagnostic checkpoint. Carry its identity and
ledger across restarts and into T27; never create a fresh ID to replenish allowance.
The envelope must reserve+fsync each upstream request before connect, including
title/compaction/children/retries; uncertain reservations remain spent. Explicit
test credentials only from gitignored `.local/live.env`; no auth/body/env dumps,
authoring-agent credentials or generation in the user's working repository.
Existing max output 2048 smoke / 8192 coding and bounded fixture input/time/watchdog
remain. All fake negatives are offline. Envelope unit PASS alone is not native
relay integration PASS: verify the actual binary cannot bypass it before live.

T55 PROV10 is a current wire/tool-cycle prerequisite, **not** seeded coding PASS.
T27 retains E2E02/A09: seeded Rust bug, real read/apply_patch/bash, tests green,
only expected paths/API changes, same-session reopen and next command. Complete
that workflow with current bounded evidence before product readiness. T44 stays
PAUSED and owns visual qualification after explicit resume. Apply affected crate
and final workspace gates from this document; document checks are planning only.
