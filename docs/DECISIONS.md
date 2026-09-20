# Решения после Q01–Q07

## Подтверждено владельцем

Q01 закрыт: provider family `@ai-sdk/openai` через собственный OpenProxy, bearer/env config, ludka static и ludka2 dynamic metadata. OAuth и остальные provider implementations не нужны. Точные live endpoint/key не запрашивать в документах: они подключаются в среде.

Q02 закрыт: DCP functional port обязателен, codex_web remote bearer/no OAuth, Chrome stdio integration с default disabled. Пользовательский discovery script — часть required behavior, а не временная справка.

Q03/Q04 закрыты для daily-direct: read/file mutation/webfetch/shell, MCP, compress + reminders, исходный TUI product direction сохраняется. Один built-in apply_patch вместо write/edit для всех моделей. Это намеренное отличие, не ошибка parity.

Q05 по runner/полномочиям уточнён: authoring-agent не привязан к GPT, модели, provider или CLI. Compatible agent работает в non-root account с dev tools; YOLO-подобные режимы, rootless Docker, git commit/push разрешены только в пределах runbook. Конкретный monetary cap/время watchdog владелец не указал. Не выдавать отсутствие cap за unlimited budget approval или блокировать все offline работы из-за незаполненного старого вопросника.

Q06 закрыт по target: пользовательский report Debian 13.4 x86_64 GNU, 6 vCPU AMD EPYC, около 11.68 GiB RAM, ext4, kernel 6.12.86, active rustc/cargo 1.98.1. Это report владельца; он не доказывает фактическую среду будущего процесса. Проверяется preflight. Наличие root в показанном prompt и rootful overlay не отменяет заявленный будущий non-root/rootless запуск.

Q07 закрыт: fresh start, no migration, no publishing. Требуется запускаемый результат cargo build. Разрешение push исходников не означает разрешение release/service deployment.

Configured workspace добавлен владельцем в первый daily-driver release: global/local config и AGENTS, `.opencode`, skills, primary agents, commands и известные native mappings DCP/OpenProxy. Arbitrary JS/TS/npm plugins явно не требуются и не исполняются.

## Defaults этой редакции, не ответы владельца задним числом

D01: четыре packages и небольшой typed application API; бинарник oc, namespace oc-rs. Local TUI/headless; serve/attach отложены.

D02: один native Responses adapter, explicit projection/store:false, no previous_response_id в первом scope. Основан на OpenAI package/default family и observed proxy route; exact replay проходит wire spike. Chat Completions не добавлять «на всякий случай».

D03: DCP range core и базовая панель; experimental message mode/custom prompts/subagents вне goal. Required source-derived fixtures обязательны. Automatic strategies не удалять только ради минимального demo.

D04: explicit direct exposure в native profile. Отсутствующий codemode field имеет direct semantics в этом профиле, explicit true отклоняется. Так устраняется прежний blocker на пропущенном ключе в пользовательском MCP config без скрытой подмены true→false.

D05: один data-root owner, SQLite worker, sequential tool execution; no distributed locks/frameworks. История на диске, DCP — projection.

D06: AGPL-3.0-or-later-compatible путь для прямого производного DCP-порта. До первого push такого кода получить LICENSE pinned DCP, сохранить notices/атрибуцию и ясно обозначить лицензию соответствующей производной работы. Не переименовывать DCP в MIT. Пакет не делает правового заключения о любых будущих способах сочетания кода; при конфликте лицензий затронутый push блокируется, unrelated local work продолжается. У OpenProxy проверенный GitHub metadata не определяет лицензию; не копировать его реализацию без отдельного основания, достаточно protocol/reference tests.

D07: dev build jobs=2, test threads=2, target/tmp на диске worktree, не /tmp tmpfs. Rootless Docker — optional инструмент, не требование запуска oc. Начальные safety byte caps и bounded live test envelope заданы runbook, не утверждены как пожелание к latency/RAM.

D08: рабочая ветка `agent/oc-rust-port` от текущего согласованного checkout; обычный push только origin этой repo, без force/merge в default. Владелец разрешил push, безопасная ветка — выбранная реализация этого разрешения.

D09: нет большого монолитного progress log. Один маленький state, current handoff, indices и immutable factual leaves. No RAG, embeddings, database или новый agent framework для планирования.

D10: одна immutable config generation на turn. Skills snapshot-ятся bounded при построении generation и раскрывают body только через native tool result; primary agents могут только сужать central policy; commands проходят ровно один durable SubmitInput. Location switch выбирает Location-scoped session, не перепривязывает активную session.

D11: known plugin compatibility — обычный exact enum/match по capability kind, не generic registry. T07 классифицирует и отклоняет unknown до side effects; T14/T19 связывают marker с compiled discovery/DCP. Exact marker не исполняет JS-файл и не меняет authority пользовательского discovery snapshot.

## Остаточные prerequisites, не новые Q

Secrets/connectivity/наличие нужной live модели проверяются в T00/T16/T27. Missing → конкретный external blocker, не угадывание credentials. Exact versions Cargo dependencies/rmcp protocol/TLS проверяются compile spike. Реальная browser-служба требуется только для opt-in smoke. Monetary hard limit внешнего authoring-agent не задан; документация его не исполняет. Semantic DCP качества проверяются fixtures/live task, а не декларацией.
