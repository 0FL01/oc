# Goal: MCP attach parity с opencode v2.0.12

Status: complete (assigned T46 backend baseline; additive MCP08/MCP09 pending; T44/product qualification separate)
Source: инструкция владельца 2026-09-22 и утверждённый после RECON план 2026-09-27 (MCP config/startup/failure parity), reference `https://github.com/anomalyco/opencode/tree/v2.0.12` (commit `2670273ff17da96f85c5826ced57aa1b368754fa`).
Additional source: owner-approved OC1-shaped MCP/permissions and audited MCP modal configured-name plans, 2026-10-02; pinned OC2 remains the oracle, not OC1 OCR. Existing completed backend baseline/status/evidence are unchanged; additive MCP08/MCP09 qualification below is pending.
Last updated: 2026-10-02

## Objective

Сбой одного MCP-сервера не отменяет пользовательский turn. Native `oc` повторяет поведение
upstream v2.0.12: сервер получает статус failed с санитизированными stage/code, его инструменты
не публикуются в generation, turn выполняется дальше, а деградация видима пользователю.
Дополнительно outbound HTTP-клиенты отправляют `User-Agent`: JS runtime upstream всегда его
отправляет, `reqwest` по умолчанию — нет, из-за чего Cloudflare перед настроенным crw endpoint
отвечает `403 Error 1010` (проверено probe: с любым честным UA — 200 + MCP initialize).

Owner-approved 2026-09-27: минимальный status/control prerequisite для настоящей
MCP-модалки T44/VIS40 также входит в T46/R5. Это расширение существующего владельца
generation/clients, не новая служба или claim полного MCP/OAuth parity.

Owner-approved config/startup extension: отдельная MCP-запись не может обрушать
весь запуск ещё в loader. Обе донорские формы нормализуются, `cwd`/`environment`
имеют реальные launch effects, enabled-серверы начинают подключение асинхронно
до первого prompt. Приложение и исправные соседи работают при failed/slow MCP;
диагностика и модалка доступны до turn. T51 отдельно владеет plugin/provider и
общими startup diagnostics; это не второй MCP owner и не done-dependency T46.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Required Outcomes

- R1: per-server деградация вместо отказа turn.
  - Source: upstream `packages/core/src/mcp/index.ts` (status `pending|connected|disabled|failed|needs_auth`, `logWarning`, turn продолжается) и `docs/DECISIONS.md` D13.
  - Acceptance: `attach_mcp` при ошибке одного enabled-сервера записывает санитизированную деградацию (`mcp <id> <stage>: <code> (retryable=<bool>)`), выполняет cleanup этого сервера и продолжает; turn завершается, инструменты упавшего сервера в реестре отсутствуют.
  - Status: verified (current report; historical baseline retained)
  - Evidence: `evidence/T46/report.md`

- R2: деградация видима, а не silent.
  - Acceptance: `TurnReport.warnings` несёт деградации generation; TUI показывает `warning: …` note, headless печатает `warning: …` в stderr; в предупреждении нет URL, заголовков и значений секретов.
  - Status: verified (scoped current/late warnings and actual native live run)
  - Evidence: `evidence/T46/report.md`

- R3: `User-Agent` на outbound HTTP-клиентах.
  - Acceptance: MCP remote, provider generation и discovery отправляют статический `oc/<version>`; webfetch — browser-like `oc-user/1.0` (паттерн upstream `OpenCode-User/1.0`: страницы блокируют клиента без UA); тесты фиксируют наличие UA на каждом клиенте.
  - Status: verified (fresh client regressions and actual CRW attach)
  - Evidence: `evidence/T46/report.md`

- R4: живая проверка в окружении владельца.
  - Acceptance: `oc run` с включённым crw подключает сервер (каталог доступен), а `codex_web` остаётся connected с search; при недоступном сервере turn завершается с видимым warning.
  - Status: verified (bounded owner-live; same continuing campaign, 8 generation / 1 search)
  - Evidence: `evidence/T46/report.md`

- R5: typed MCP status/control projection для T44/VIS40.
  - Source: owner-approved MCP modal plan 2026-09-27; pinned `packages/core/src/mcp/index.ts:380–447,593–609` and `packages/tui/src/component/dialog-mcp.tsx:37–175` (U50/U54).
  - Acceptance: MCP08 proves bounded current-Location/config-generation server snapshots, including disabled entries, from actual owner state; configured-enabled is not connected. Genuine pending/connected/disabled/failed and any typed auth-required outcome are not parsed from server names/free text. Actual async connect/disconnect/retry reuse existing clients/registry; connected requires initialize and catalog success, disconnect closes owned resources, catalog changes publish only at safe request boundaries. Reads/resize/cancel stay responsive during a turn instead of waiting behind its long-held MCP mutex. Pending actions are coalesced/revalidated by server/Location/generation, late completions cannot mutate a new generation. Failed attach remains visible per D13 without cancelling the turn; fatal cancellation/cleanup/caps remain. Runtime controls do not secretly rewrite config, auto-enable disabled browser, clear quarantine or replay uncertain tool effects. Reopen reads current state; restart rebuilds state from effective config, never restores stale connected labels or claims persisted runtime toggles. T44 consumes this slice and separately qualifies the modal; no all-T46 or reverse completion dependency.
  - Primary evidence: rebuilt actual binary + fake HTTP/stdio initialize/catalog/disconnect/retry counters and owned process cleanup, current/next request catalogs and Location/restart/late-action assertions under MCP08, reusing MCP07/AUD23/MCP05.
  - Status: verified (completed baseline; configured-label follow-up below pending/NOT_RUN)
  - Evidence: `evidence/T46/report.md`, `evidence/T46/lifecycle.md`; paired visual evidence belongs to T44/VIS40.

- R6: donor-compatible config admission и настоящий local launch.
  - Source: утверждённый план 2026-09-27; pinned `packages/core/src/v1/config/mcp.ts`, `v1/config/migrate.ts:202–225`, `config/normalize.ts:260–293,633–696`, `config/plugin/mcp.ts`, `packages/schema/src/mcp.ts:7–64`, `packages/core/src/mcp/client.ts:195–207`, `mcp/stdio.ts:17–31,80–89`.
  - Acceptance: MCP09 проверяет матрицу ниже через общий trust→substitute→normalize→validate pipeline. Legacy/canonical/global timeout и precedence имеют донорскую семантику, неизвестные не-security fields не вызывают общий отказ. Некорректная recognized запись остаётся в bounded diagnostic inventory с server/source/field/stage/safe code и без tools/process/network; валидные соседи сохраняются. Unsupported OAuth/CodeMode/protocol — per-server capability failure, не молчаливое принятие. Валидный disabled chrome с `environment:{npm_config_offline:"true"}`/timeout запускает TUI с нулём npx/browser effects. Для admitted local fake MCP actual argv/cwd/inherited env/overlay/PATH и раздельные deadlines соответствуют normalized config. Explain/logs/UI/history не раскрывают env/credential values; исходные файлы не меняются.
  - Primary evidence: source-derived normalization fixtures плюс rebuilt actual binary/fake stdio cwd/env/argv and process counters; reuse CFG02/CFG04/AUD22/MCP04/MCP05, новый owner только MCP09.
  - Status: verified
  - Evidence: `evidence/T46/report.md`, `evidence/T46/config-admission.md`; actual new env/launch evidence, not historical stdio inheritance.

- R7: независимый асинхронный initial MCP startup.
  - Source: утверждённый план 2026-09-27; pinned `packages/core/src/mcp/index.ts:355–447,487–513,583–638`.
  - Acceptance: MCP10 доказывает startup до prompt с одним healthy, одним barrier-held slow и одним failed MCP: TUI/history/status и healthy server доступны, failed не отменяет запуск/turn, slow не удерживает global launch barrier. Initialize+catalog precede connected; только доступные tools/guidance попадают в новый request snapshot. Все клиенты/actions/startup принадлежат существующему owner; late completion/reload/Location/cancel/shutdown не смешивают generations и не оставляют owned tasks/children. Config/startup failures сразу видны R5-consumer; исправление config/retry восстанавливает фактическое состояние на безопасной границе, не исполняет неизвестный tool effect.
  - Primary evidence: rebuilt actual binary + barrier-controlled HTTP/stdio counters, pre-prompt PTY responsiveness, first/next request catalogs и owned cleanup; reuse MCP08/AUD23/A02/A10 без второго lifecycle registry.
  - Status: verified
  - Evidence: `evidence/T46/report.md`, `evidence/T46/lifecycle.md`; paired presentation separately T44/VIS19/VIS40.

## R5 configured-label follow-up — approved 2026-10-02; pending

После read-only RECON и независимого аудита владелец утвердил plan-only уточнение
MCP08/VIS40: вместо `server-<hash>` в модалке нужны имена из эффективного конфига.
Pinned OC2 `core/src/mcp/index.ts:491–500,583–587` и U50 используют configured map
key, не handshake `serverInfo.name`; U54 уже покрывает этот source seam.

1. **Display label отдельно от identity.** Переиспользовать
   `McpServerSnapshot.name`: обычный безопасный ключ сохраняется точно, например
   `chrome-devtools`, `codex_web`, `crw`, включая поддержанные Unicode/long names.
   List/search/display sorting/details используют label; выбор и actions остаются
   по opaque `id` и Location/generation/instance binding. Diagnostic identities,
   tool/permission names и provenance не переименовывать. Не добавлять новый DTO,
   titlecase/production allowlist/ASCII-only policy или заимствованный wire cap256.
   Существующие config/snapshot/render bounds и source truncation сохраняются.
2. **Защита до публикации.** Одной замены `safe_server_id` на raw key недостаточно:
   существующий CFG10 regression допускает failed entry с ключом, равным credential.
   Перед первым snapshot применить известные protected values из existing redaction
   owners и cached admitted sources, включая recognized sensitive fields disabled/
   failed/unsupported записей. Terminal controls/ANSI, raw endpoints, sensitive paths
   и защищённые значения не попадают в label/DTO Debug/details/copy/investigate;
   небезопасный label получает existing opaque fallback, не переписанный raw key.
   Не читать inactive credential files или runner auth/config ради имени.
3. **Activation не возвращает секрет в label.** Вновь разрешённые protected values
   проверять против ключей всех nodes до последующей публикации, включая activation
   refusal/entry failure. Уже masked `row.name` не восстанавливать из raw key в той
   же generation/instance после disconnect/retry. Достаточны transient values и
   существующее поле строки, без growing historical secret cache/persistent state.
   Готового полного seam нет: private activation result/reporting boundary должен
   передавать известные значения и на error exits; точную локальную форму выбрать
   у existing config/composition/lifecycle owners, не создавать generic framework.
   Это защита последующих publications, не claim очистки уже выданных snapshots.
4. **Адресная проверка.** Расширить nearest `application/fatal_tests.rs` для initial
   ordinary/credential-key projection и `application/mcp_tests.rs` для newly resolved
   activation value, включая отказ: masked label/Debug, unchanged id, correct action
   target и disabled sibling zero-spawn; последующие публикации не снимают masking.
   Сохранить diagnostic-identity regression в `runtime/mcp/lifecycle/tests.rs`,
   обновить helpers в application/runtime MCP tests и soak, которые считают name
   diagnostic hash. Reuse legacy/canonical MCP09 fixtures, не новая normalization
   matrix. Existing rebuilt-binary fake MCP scenario проверяет реальные labels и
   адресата после filter/status refresh, отдельно от T44 styled-cell/PNG evidence.

MCP08 label follow-up pending/NOT_RUN; completed R5/report не квалифицирует новые
assertions. При разрешённом implementation handoff после active T50 использовать
existing T46 reopen workflow с причиной и повторной проверкой dependencies, не
новую задачу и не автоматический reopen от approval. T44 status presentation может
исправляться независимо по уже доступному typed snapshot; совместная qualification
ждёт нужных slices, не all-T46 completion. После explicit T44 resume сохранить
clean-dialogue/tool-preview-first порядок; полный VIS40 остаётся открытым до всей
existing mandatory приёмки. Новых tasks/gates/stores/campaigns нет; historical
evidence, execution statuses, paused T44, security и no-unknown-replay неизменны.

## R6 field/capability matrix — approved target, not supported claim

| Input | Required normalization/runtime outcome |
| --- | --- |
| `mcp.<name>` и `mcp.servers.<name>` | Один canonical domain; canonical запись побеждает legacy при совпадении имени в документе с safe conflict diagnostic; в layered documents later server целиком заменяет same-name entry (не field-wise merge), global timeout merges supplied leaves отдельно. Pinned order/provenance и typed legacy серверы с именами servers/timeout различаются normalizer-ом. Invalid recognized entry диагностируется отдельно, не удаляет healthy siblings. |
| `type`, local `command`, remote `url`/`headers` | Typed local/remote shapes, argv без shell splitting, exact configured URL без probing/rewrite; headers case-insensitive/conflicts явны до network. Invalid entry остаётся failed. |
| `enabled` → `disabled` | Legacy enabled:false → canonical disabled:true; enabled:true → omitted disabled. Canonical activation использует disabled, не enabled: canonical enabled:false само по себе не выключает server, перенос legacy entry под servers без inversion неверен. Omitted activation follows pinned normalizer; malformed recognized fields имеют safe diagnostics. Disabled не стартует/не требует credentials; malformed disabled record не маскирует ошибку, но не отменяет приложение. |
| Local `cwd` | Omitted = effective workspace cwd; relative = resolve от Location workspace directory, не config-source directory. Absolute требует существующего canonical admission; никакого silent fallback, escape или нового доверия. |
| Local `environment` | String map с действующими admitted substitutions. Наследовать environment **product process** и наложить overlay, включая PATH при разрешении executable. Только после command/resource/credential-domain admission; lower-trust command не получает higher-trust credentials вследствие inheritance. Не читать auth/config внешнего runner. |
| Legacy numeric `timeout` | Positive milliseconds; migrate только в catalog/execution, startup отдельно. |
| `mcp.timeout` и server `timeout.{startup,catalog,execution}` | Global layer merge + per-server overlay; omitted donor defaults 30000/30000/43200000 ms. Runtime stage deadlines действуют, cancellation и отдельные cleanup budgets/caps сохраняются. |
| `codemode` | Omitted остаётся direct по D04 — declared difference от donor default true; false = direct; true = per-server UnsupportedCapability, без interpreter. |
| Remote `oauth` | false поддерживается без OAuth discovery. Legacy camelCase/current snake_case object fields распознаются как unsupported OAuth capability; existing true compatibility диагностируется, не выдаётся за donor schema. Omitted OAuth сохраняет declared native no-OAuth semantics. |
| `protocol` | Omitted/legacy сохраняет существующую initialize negotiation до 2025-11-25 и exact codex_web contract. `auto`/`2026-07-28`: bounded rmcp compatibility spike; пока нет реального доказанного adapter path — явный per-server UnsupportedProtocol, не false PASS/legacy rewrite. Реализация нового protocol вне утверждённого среза. |
| Unknown fields | Donor excess-field omission допускается только для не-security metadata; recognized malformed/unsupported capabilities имеют безопасную typed причину. Policy/trust validation не ослабляется. |

Полный список OAuth fields: legacy clientId/clientSecret/scope/callbackPort/redirectUri;
canonical client_id/client_secret/scope/callback_port/redirect_uri/auth_server_metadata_url.
Распознавание этих полей не обещает OAuth. `{file:}` остаётся no-follow/relative-only;
absolute/`~/` file-reference parity и remote-workspace execution plane не добавляются.

### MCP/permission config follow-up — approved 2026-10-02; pending

MCP09 уточняется source-derived host-shaped equivalence fixture: legacy codex_web/
crw remote с oauth:false и headers, disabled local chrome с argv/environment и
numeric timeout, затем canonical servers/disabled/staged-timeout форма. Без real
secrets, npx/browser или owner-config edits. Проверить normalized facts и реальный
fake-client startup/launch, не только parser acceptance:

- URL сохраняется буквально после admitted env substitution: codex_web suffix `/mcp`
  принадлежит supplied URL; crw URL не получает `/mcp` или probe/query fallback.
  Headers/substitutions и redaction проходят общий source-trust pipeline.
- Numeric60,000/30,000/3,000 ms задают только catalog/execution; omitted startup
  остаётся30,000. Global supplied leaves и per-server overlay проверяются отдельно
  от later whole-server replacement; mixed same-name conflict diagnostic безопасен.
- Native omitted/false codemode — direct/exact URL. Donor omitted/true CodeMode может
  добавить `?codemode=false` и после initialize400/404 попробовать original URL;
  donor codemode:false выбирает direct. Это declared native difference, не новый retry
  разрешённого или unknown-effect tools/call. OAuth omission у donor допускает auth
  discovery, false его выключает; native omitted/false остаются no-OAuth. Protocol
  omission/legacy до2025-11-25 и unsupported auto/2026-07-28 остаются явными.
- T45/CTX02 владеет permission names/defaults/filtering, не transport. Existing
  T46 completed R1–R7/report подтверждают свой baseline, **не новые fixture assertions**.
  Follow-up MCP09 qualification записывается отдельно при safe scheduled handoff;
  его approval не reopen/finish task, не дублирует MCP08/MCP10/live/visual campaign.

Oracle: pinned `schema/src/mcp.ts`, `core/src/config/normalize.ts:244–315`,
`config/plugin/mcp.ts:39–55`, `mcp/client.ts:160–317`; nearest config normalization
and MCP tests. Actual disabled zero-spawn/reload/restart counters reuse existing owners.

## Constraints

### Backend follow-up (2026-09-22)

R1–R3 code is delivered in `3ce5e03`/`b7ab06d`; historical report is not a new
full-live PASS. Current additive evidence: `evidence/T46/backend-parity.md`.
Required backend repair adds structured JSON/textual resource/link tool results,
safe fixed error categories, and bounded permission-filtered initialize guidance
in provider projection only. Existing fatal lifecycle/caps and redaction hold.
R4 remains pending until full bounded owner-live evidence. Media tool outputs,
MCP prompts/resource catalogs remain explicit open parity work, not silently
excluded by the older non-goals list. This follow-up does not change T44 surfaces.

- C1: остаются non-success на соответствующей admission/operation/application boundary: `Cancelled`, ошибки cleanup/`McpShutdown`, `MAX_MCP_SERVERS` и generation-капы каталога; AUD23 reaping ранее подключённых серверов сохраняется. Отмена операции не превращается в successful degradation; обычный admitted per-server disconnect не означает отмену приложения.
- C2: диагностика санитизирована: warning несёт server id, stage и safe code/retryability; R6/T51 structured details дополнительно safe source identity/field/allowed action. Никаких raw URL, заголовков, env/secret values, remote exceptions или sensitive absolute paths; безопасность распространяется на copy/investigation drafts.
- C3: изменение прежней политики (fatal attach) фиксируется записью D13; тесты переписываются как осознанное изменение контракта, а не ради зелени.
- C4: R5 uses the existing application/runtime/resource owner, immutable request/config generations and bounded snapshots/events/actions. No second client registry/store/event bus, daemon, JS host or periodic UI polling. Retry is a new admitted connection attempt, not permission to repeat an unknown MCP tool call; sticky remote quarantine and cleanup failure remain actionable, non-success states.
- C5: native MCP OAuth remains outside GOAL; the approved T57 OpenAI provider-auth exception does not authorize MCP sign-in. Donor `needs_auth`/integration sign-in is recorded in the capability mapping before capture: render only actual typed state and make unsupported sign-in honest/actionable, never launch a fictitious OAuth flow. Safe diagnostics only in details/copy/investigation drafts, never raw remote error text, URL/header/env values. MCP result/media/prompts/resource work and R4 live qualification are not closed by MCP08/VIS40.
- C6: R6 narrowly supersedes T37's minimal credential-free **local MCP** env/cwd policy (D21); ordinary shell TOOL05/AUD28 remains minimal. Source trust, command/resource and credential-domain admission still precede inheritance; remote MCP does not acquire arbitrary local env. All configured env values and inherited secrets join existing redaction; no env dump in fixtures/evidence. Explicit admitted external process is not a sandbox.

## R5 sequence and visual consumer

1. Add minimal typed current-Location/generation status snapshots/events to existing
   owners; R7 supersedes lazy first-turn attach with async initial startup. Preserve
   admission, disabled zero-spawn and no false
   connected state before a successful handshake/catalog. Modal reads cannot spawn
   disabled entries or repeatedly reconnect healthy clients.
2. Deliver real connect/disconnect/retry, per-server pending/coalescing/cleanup and
   safe catalog publication without changing an in-flight request or config on disk.
3. Qualify MCP08 actual-binary fake-server effects, then T44
   [VIS40](../../tui-recovery/T44_CONTRACT_AMENDMENT.md#mcp-modal-parity--vis40)
   full paired dialog frames/focus/actions/colors/state transitions. Preserve failed
   attempts/provenance and report backend versus visual results separately.

Approval freezes this plan, not implementation PASS. Existing execution statuses,
evidence and dependencies remain factual; independent T44 slices stay ready.

## Change Envelope and historical approval checkpoint

- Expected owners/paths: adapters config/composition/mcp_stdio/mcp_remote/application,
  core runtime MCP generations/actions/redactions; direct typed consumers in `oc`/TUI.
  Extend existing owners only; no separate store/event bus/worker framework/daemon.
- Order: R6 config normalization → admitted local launch/deadlines → R7 async initial
  lifecycle with R5 snapshots/control → MCP09/MCP10/MCP08 actual-binary checks →
  T44 VIS19/VIS40 paired qualification. T51 service isolation may proceed independently;
  shared safe diagnostic shape is not a whole-task completion barrier.
- Next: source-derived failing normalization/disabled-chrome fixture for R6, then
  smallest parser/normalizer change; no owner config edit to hide the reproduced failure.
- Evidence gates: affected crate targeted tests, relevant A02/A03/A06/A08/A10/A13
  regressions and workspace fmt/clippy/tests/build for integration/final acceptance;
  bounded R4 live remains mandatory. No new paid/browser campaign for fake qualification.
- Approval checkpoint 2026-09-27: RECON confirmed strict McpEntry rejects environment
  even when disabled, stdio uses env_clear/minimal env, attach is first-turn/sequential.
  This is planned work; no new runtime/visual PASS or change to task execution status.

## Non-goals

- `oc mcp list` CLI и отдельная персистентная status service/panel остаются вне этого среза. Typed snapshots/control для T44/VIS40 входят в R5; прежняя отсрочка всех статусов superseded владельцем 2026-09-27.
- OAuth, Code Mode, новый protocol/remote-workspace execution plane, отдельный browser-like UA для MCP (webfetch R3 сохраняется), расширение `{file:}` (absolute/`~/`) и прочие parity-дельты вне согласованных R1–R7. Config admission/cwd/env/status/async startup больше не являются non-goals.

## Completion — 2026-09-29

Current `evidence/T46/report.md` verifies R1–R7 and mandatory structured/guidance,
prompt/resource/template and media follow-up, with existing implementation commits,
fresh full Rust gates and bounded actual owner-live R4. Frozen behavior, security,
cleanup/quarantine, request/config ownership and no-JS constraints remain intact.
The implementation source/report precedes the progress-owner finish; Git delivery
is checked separately. The approval/history sections above are source context,
not today's execution state. T44 paired presentation/full V09 and other GOAL
owners remain open; T46 completion is not product READY or a final stopping point.
