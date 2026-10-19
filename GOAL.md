# oc2 — открытые требования и ограничения

Это справочник оставшихся обязательств, не очередь работ и не startup-инструкция.
Scope текущей работы задаёт пользователь; workflow — в [AGENTS](AGENTS.md).
Исторические T/VIS/AUTH/HOT labels ниже помогают найти предметный контракт, но не требуют регистрации новых задач, журналов или closeout.

## Продукт

- Нативный Rust 2024 coding agent: один `oc2`, TUI и headless используют общий typed runtime. Квалифицированная платформа — Debian 13 GNU/Linux x86_64; setup и запуск — [README](README.md).
- Один native owner на UID/data root, private UDS, одна Db/recovery authority; нет public HTTP/TCP service, privileged broker или второго write owner. Shared TUI/finite headless leases не возвращают старый `oc service` scope.
- Config — только канонический OC2 + объявленные native extensions, fail-closed. OpenProxy discovery использует [владельческий oracle](references/openproxy-models.user.mjs), не старый repository plugin.
- Поддержанные provider/tool/config semantics — [CONTRACTS](docs/CONTRACTS.md), [CONFIG](docs/CONFIG.md), [OpenProxy](docs/PROVIDER_OPENPROXY.md), [tools/MCP](docs/TOOLS_MCP.md).
- RAW/HOT, intentional forgetting и отсутствие artificial round stops остаются свойствами продукта, не процессом ведения документации: [DCP](docs/DCP.md), [Long Horizon](docs/LONG_HORIZON.md).
- Публикация, historical PASS, offline gate или отдельный bugfix не означают full-product READY либо полный паритет с OpenCode.

## Незавершённые outcomes

### TUI parity — отложено, не отменено

Полная visual/interactive qualification ещё не завершена. Частные исправления не закрывают весь контракт:
[R1–R6](docs/goals/2026-09-21-tui-pixel-parity.md),
[дополнения VIS01–VIS46](tui-recovery/T44_CONTRACT_AMENDMENT.md),
[paired verification](tui-recovery/VERIFICATION.md).
Работа возобновляется по явному запросу, не автоматически после другого исправления.

Текущий явно запрошенный срез: [tool-log parity](docs/goals/2026-10-19-tool-log-parity.md)
— перенос exploration/Execute поведения на `master`, итеративные проверки,
commits и обычный push. Он не возобновляет остальные TUI qualifications.

- Нужны pinned-original/native full styled-cell, PNG, cursor, temporal/effect/resource проверки; собственные goldens или fixture-only PASS не заменяют paired qualification.
- **Idle Esc (T44/R5/V05/VIS11):** один или несколько Esc в обычном idle Home/session prompt не завершают приложение, не очищают draft и не создают submission/provider request, включая после completed/interrupted turn.
- Idle Esc сохраняет cursor, paste chips, focus, session и agent/model/variant selection; затем можно редактировать и отправить ровно один обычный запрос. Modal/autocomplete/leader/permission/question priority, busy double-Esc с 5s окном, pending/compaction/cancel/cleanup, background-shell cancellation и explicit exit bindings сохраняются.
- Idle Esc требует focused regression, raw-Esc PTY effects и paired qualification; подробный исходный RECON — Git `cd3e867a3`. Перенос этого требования не является implementation PASS.
- [Phantom caret / prompt blink](tui-recovery/T44_CONTRACT_AMENDMENT.md#tool-output-phantom-caret-and-prompt-blink--vis16vis31-2026-10-01) относится к VIS16/VIS31; VIS44 — Middle Click close. Shape/default/nonblinking/focus/restoration и temporal checks не отменены.

### Hot reload и client settings — отложено

[Контракт](docs/goals/2026-10-14-hot-reload-shared-service.md): R1/R2 реализованы частично, composed HOT01/HOT02 ещё не qualified; R4 settings и R5 final qualification pending, HOT04 NOT_RUN.
Старый R3/HOT03 descoped. Позднейший shared native owner не возвращает его и не квалифицирует оставшийся settings surface.
Issued request/approval/full batch сохраняют captured authority; fresh preparation — отдельная граница adoption. [Метод проверки](docs/TEST_PLAN.md#hot-reload).

### Live workflow после compression

[Сохраняемая qualification](evidence/T27/live-workflow.md): live coding/reopen/web/search/compression выполнены исторически, но успешное live продолжение **новым процессом после compression** не квалифицировано.
Нужны retained fact/reduced context/next edit после restart; root-only или fake-provider результаты не заменяют этот live facet.
Используется существующая configured-OpenProxy campaign с исчерпанным generation budget, не новый ID ради пополнения.

### Configured-OpenProxy live child

[R3 subagent contract](docs/goals/2026-09-21-config-compat-and-subagents.md): offline backend квалифицирован исторически; spawning одного live child через настроенный OpenProxy остаётся NOT_RUN/live-blocked.
Root-only workflow, fake child и отдельная Go campaign этот результат не доказывают.

### OpenAI/Codex AUTH06

[Контракт R6](docs/goals/2026-10-06-codex-oauth-and-openai-key.md): backend R1–R5 проверен offline; реальные browser OAuth, headless device OAuth и ordinary OpenAI key login/request/reopen остаются NOT_RUN.
Нужны пользовательские browser/device confirmations и явное разрешение ordinary OpenAI test input/key. Нулевой ledger не даёт такого разрешения. VIS45 qualification отдельна; credentials authoring-agent не используются.

### Остальные неподтверждённые проверки

- [MCP configured labels/host-shaped equivalence](docs/goals/2026-09-22-mcp-attach-parity.md): additive MCP08/MCP09 pending. Старые legacy-normalization требования читать с учётом более позднего OC2-only allowlist, не возвращать OC1 compatibility.
- [Retry/span ownership и restricted-provider safe details](docs/goals/2026-09-29-provider-retry-parity.md): additive late-retry и safe-details assertions pending; historical backend baseline не доказывает новые checks. UI — отдельный TUI contract.
- [E2E06](docs/TEST_PLAN.md#e2e06--real-user-startup): после config изменений требуется current-environment rebuilt-release qualification с existing **и** fresh native roots под PTY/strace. Старый startup PASS не переносится автоматически.
- [Shared native owner R5](docs/goals/2026-10-17-shared-tui-runtime.md): peer-credential/foreign-metadata boundary fixtures проверены; реальная root/stfu launch pair **NOT_RUN, только owner-operated**. Это не разрешение агенту запускать root или пользоваться чужими credentials.
- Бюджет обычного case — 30s; единственное согласованное UI07 исключение ≤40s и диагностика warmed gate >10min сохранены в [TEST_PLAN](docs/TEST_PLAN.md#ресурсы-и-время). Старый slow-test `unmet` не доказывает текущую регрессию вопреки более поздним receipts.

## Live authorization и сохранённые бюджеты

Ниже последние документированные факты, не новый mutable ledger и не выполненный сейчас preflight. Ledger/identity и unknown reservations сохраняются; счётчики не refund/reset и не переносят между authorities.

- **Configured OpenProxy:** campaign `39d44cb58b834de99544daf3c2eedab1`; generation **24/24**, control **15/128**, MCP **1/4**. Generation allowance **0**; reservation17 unknown/reserved остаётся spent.
- Исторические identity/ledger paths: `/home/opencode/.cache/opencode-tmp/opencode/responses-diagnose-20260930.campaign-id` и `/home/opencode/.cache/opencode-tmp/opencode/responses-diagnose-20260930-campaign/attempts.jsonl`. Это не текущий stfu tmp-root и не разрешение читать чужой HOME. Если исходный ledger недоступен, live продолжение blocked, а не новая campaign.
- **Go:** [ledger](evidence/T53/live-campaign.json), ID `t53-go-20261006`, generation **13/24**, remaining **11**, MCP0, uncertain0. Этот остаток не разрешает OpenProxy child/restart facet.
- **OpenAI/Codex:** [ledger](evidence/T57/live-campaign.json), ID `t57-openai-20261006`, **0/24**, not_started/NOT_RUN, `pre_dial_live_guard_qualified=false`. Arithmetic allowance не заменяет недостающее разрешение и qualification guard.
- Любой будущий разрешённый live запуск сохраняет pre-dial reserve+fsync, учитывает child/title/summary/retry/probes/fallback, obeys существующий envelope. Метод и реальные caps — [TEST_PLAN](docs/TEST_PLAN.md#live-и-внешние-effects).

## Вне scope этого продукта/среза

- OC1 config vocabulary, миграция/import пользовательских DB и automatic config rewrite.
- Arbitrary JS/TS/npm plugin host; native Code Mode — confined Rust interpreter, не такой host. Внешний MCP — явная пользовательская зависимость.
- Public remote API/serve/attach parity, privileged cross-user broker, вечный daemon, web/desktop, auto-update, embeddings/RAG и файловый rollback через conversation Undo.
- Сокращение workflow не меняет storage schema/data roots, разрешения, security/durability/resource guards, DCP и no unknown-effect replay. [Лицензии/provenance](docs/SOURCES.md) остаются в текущем составе.
