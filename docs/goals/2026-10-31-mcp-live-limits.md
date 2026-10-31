# Goal: MCP budgets и полный доступный live-вывод

Status: active
Source: пользовательский RECON чата «RECON и план исправления команд-макросов /check с аргументами», проверка `/home/stfu/ai/work-table/space`, согласованный план и границы; указание «делай копию плана в цель и после итеративно реализовать и коммит пуш билд».
Last updated: 2026-10-31

## Objective

Корректные MCP-каталоги не отвергаются по числу серверов/инструментов/страниц.
Действуют согласованные byte budgets с запасом. Сохранённый output и части активного
хода доступны сразу, без предела доступности 2048 байт/64 части и ложного toast.

## Execution Directive

Complete the frozen Required Outcomes using the listed Change Envelope and Primary
Evidence. Work on the smallest unresolved outcome. Do not add requirements from
reviews, tests, tools, speculative risks, or optional source text. Finish when every
required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes

- R1: MCP admission и публикация используют byte budgets, а не arbitrary counts.
  - Source: согласованный RECON-план и ответ «Какие лимиты будут? они должны быть с запасом».
  - Acceptance: нет отдельных допусков 8 enabled servers, 64 inventory/tools, 16 tools/list pages, 128 aggregate tools; pagination до конца с отменой/deadline. Корректные schema/description не имеют прежних отдельных порогов 32 KiB/8 KiB. Каталог сервера ≤64 MiB, все каталоги Location ≤256 MiB сериализованных метаданных. Превышение — локальная явная ошибка с actual/limit, без закрытия исправных соседей и silent partial catalog.
  - Primary evidence: ближайшие MCP registry/stdio/remote/lifecycle fixtures пересекают старые границы; отдельное превышение byte budget не завершает соседние подключения.
  - Status: verified
  - Evidence:73 targeted tests MCP/config/lifecycle:72 PASS сразу, оставшийся stale30s-default assertion обновлён под согласованные60s и отдельно PASS. `mcp10_nine_servers_and_180_tools_stay_connected_and_reap_cleanly` PASS; finite17 pages и65 tools на обоих transports PASS; schema64 KiB/description16 KiB и inventory65 PASS. Реальный aggregate fixture6×50 MiB:5 Connected,1 Failed с256 MiB actual/limit, соседи и cleanup сохранены (3.656s). Scoped strict Clippy all-targets oc-adapters и fmt PASS.
- R2: согласованные транспортные границы и defaults согласованы по цепочке.
  - Source: согласованная таблица границ.
  - Acceptance: одно MCP message/SSE event ≤64 MiB; startup default60000ms, полный catalog default120000ms; существующие настройки могут увеличить оба значения. Execution timeout остаётся существующим настраиваемым. Разрешённый каталог не отвергается более низким случайным transport/IPC/projection cap.
  - Primary evidence: targeted transport/config regressions и проверки прямых consumers.
  - Status: in_progress
  - Evidence: defaults60000/120000ms и existing overrides проверены в R1;64 MiB frame/body/SSE qualification остаётся следующей итерацией.
- R3: полный доступный tool output и активная история не ограничены preview.
  - Source: исходный screenshot/RECON, согласованный план удаления2 KiB/64-part отсечений.
  - Acceptance:2048 байт не предел доступного тела; полный сохранённый output читается страницами64 KiB через существующий owner. Это размер порции, не предел результата. Сохранённый prefix активного хода доступен каноническим paging до completion; live — несохранённый tail. Paging, свёрнутая карточка и вытеснение cache сами по себе не вызывают toast о потере live-вывода. Настоящий coverage gap не маскируется.
  - Primary evidence: существующие output/live/history/repair fixtures с >2 KiB output и активным ходом через64/240 частей, без дубликатов, нарушения порядка, потери scroll/draft/expansion.
  - Status: pending
  - Evidence:
- R4: результат проверен, закоммичен, отправлен в origin и собран release.
  - Source: «итеративно реализовать и коммит пуш билд».
  - Acceptance: targeted regressions/scoped Clippy, один full offline gate перед интеграционной приёмкой, обычные commits/push текущей ветки в проверенный origin, `target/release/oc2`; ограничения live/visual qualification названы явно.
  - Primary evidence: команды/исходы, Git commit/upstream, release artifact; bounded no-prompt startup в space без изменения конфигурации.
  - Status: pending
  - Evidence:

### Числовые границы

| Область | Контракт |
|---|---|
| Подключённые серверы; инструменты сервера/Location | Без отдельных числовых допусков |
| Страницы tools/list | До конца, существующие отмена и catalog deadline |
| MCP message / SSE event |64 MiB |
| Сериализованные метаданные каталога одного сервера |64 MiB |
| Сериализованные метаданные каталогов Location |256 MiB |
| Отдельные schema/description | В пределах message/catalog, без32 KiB/8 KiB порогов |
| Startup/catalog defaults |60000/120000ms, существующие overrides |
| Execution timeout | Существующий настраиваемый |
| Страница полного сохранённого output |64 KiB, продолжение до конца результата |

Byte budget каталога — не обещание такого же RSS. Большой доступный каталог не
означает отправку всех schemas в каждый запрос модели.

### Constraints

- Начать mutation только после чужого коммита и clean tree: выполнено на `6d2c91375b1b9a3ad4659e2082cca742721c03e2`.
- Один mutation/Cargo owner; `CARGO_BUILD_JOBS=4`, tests/nextest threads4. Чужой concurrent diff не переписывать.
- Сохранять schema/protocol validation, routing identities, permissions/trust, redaction, captured request/full-batch authority, cancellation, cleanup и no unknown-effect replay.
- Native owner пользователя не перезапускать; отключённые боевые MCP не включать. Только no-prompt metadata startup, без commands/tool calls или paid generations.
- Без новых dependencies/services/stores/migrations/frameworks; переиспользовать существующие output/history paging, registry и lifecycle.

### Non-goals

- Остальной GOAL/backlog, полный TS/product/visual parity, provider/auth изменения.
- Снятие реальных provider wire-name требований, egress/credential/ownership validation или бесконтрольное накопление вывода в RAM.

## Change Envelope

- MCP owners: `config/mcp.rs`, `mcp_stdio.rs`, `mcp_remote.rs`, `mcp_http_lifecycle.rs`, `mcp_lookup.rs`, `runtime/mcp*`, их прямые consumers и ближайшие tests.
- Output/live owners: core tool-output/query DTOs, adapters tools/output/storage/tool-output reads, TUI history/live/transcript и binary repair/paging consumers; только необходимые изменения текущего пути.
- Документация меняется при изменении её контракта; конфигурации/DB пользователя и TypeScript oracle read-only.

## Current Checkpoint

- Closes: R2.
- Smallest next action: согласовать64 MiB transport admission для stdio, HTTP JSON и SSE, не потеряв typed errors/cancellation/cleanup.
- Expected evidence: сообщение >старого1 MiB SSE cap принимается; превышение64 MiB явное и локальное; существующие HTTP status/header/protocol/ownership fixtures сохраняются.
- Stop or replan if: concurrent mutation/Cargo owner, security boundary или необходимая новая dependency/architecture.

## Current State

- RECON: при текущем no-prompt запуске space включены7 из15 MCP, все7 Connected (~4s),37 tools,0 Failed;8 Disabled конфигурацией. Исторический отказ на этом конфиге не воспроизведён.
- Корень toast: tool preview truncation поднимается в aggregate HistoryTurn.truncated; отдельный путь — eviction64 live parts. В исходном чате сохранены ходы263/519/302 частей.
- Resolved: R1; startup/catalog defaults часть R2.
- Next: R2 transport, затем R3, финальные R4 gates/commit/push/build.

## Checkpoint History

-2026-10-31: получен clean-tree gate после `6d2c91375`; контракт и согласованные границы сохранены до runtime edits.
-2026-10-31: R1 verified;73 targeted MCP/config/lifecycle cases, strict scoped Clippy и fmt PASS. Diff сверён с frozen contract; следующий checkpoint R2 transport. Full offline gate ещё не запускался.

## Completion

- Final status: active; runtime implementation и current-source qualification ещё не выполнены.
