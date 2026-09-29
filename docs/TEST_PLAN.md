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
  neither submits nor changes model/draft; busy/read-only guards, empty/all-disabled
  no-op and explicit stale-cycle → Default recovery remain. Next accepted fake Responses
  request receives exact declared effort of the chosen alias, then Default adds no
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
  isolation and active task/pack protection/release/safe recovery. DCP11 repeatedly
  recompresses and restarts using fixed equal active data over small/large inactive
  archives, measuring loaded rows/depth and peak/retained process state. No lifetime
  call/block quota; keep payload/cycle/no-gain/model/memory/turn guards. Freeze workload
  before qualification, never raise A10 regression caps or make a finite cycle count
  a product lifespan. Do not infer sustainability from the single-compression E2E.
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
completion dependency, new task/store/framework, policy widening or historical PASS.

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

TOOL12–TOOL19 in planning/acceptance.json have only T50 as owner; relevant high-level
A02/A03/A04/A05/A06/A08/A10/A13 and existing detailed scenarios stay regressions,
not reassigned owners. Use pinned donor fixtures and actual binary calls/results;
helper-only tests, fixture table tool names and document validation are insufficient.

- TOOL12 captures policy-filtered root/child direct catalogs, canonical shell and
  legacy bash compatibility/deny, explicit MCP search and excluded native tools.
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
- TOOL19 freezes A/B configs/history/job placement and uses barriers/crash points
  before admission/placement/result delivery. Verify same ID, trusted generation,
  immutable source turn, destination requests without stale harness/opaque items,
  original background/child execution and MCP quarantine. AUD14 supersedes only
  permanent binding; direct cross-Location use without admitted move still fails.

Run nearest targeted evidence per minimal slice, then affected crate/integration
and mandatory final workspace gates, rebuilt debug/release binaries and existing
bounded live envelope. No new paid search campaign or live-budget expansion. Keep
historical reports and task statuses unchanged until real qualification.

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
| GO03 | Parameterized fake server: все три protocols text/tool/reasoning/usage, bounded splits/terminal/error/cancel/retry. Exact auth/defaults и captured metadata main/follow-up/title/summary-compaction/child/retry, concurrent identities/held binding. Custom Chat/None и OpenProxy regressions используют те же owner fixtures. |
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
