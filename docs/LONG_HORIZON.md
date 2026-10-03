# Long Horizon — сознательная дивергенция форка от OC2 TS

Решение владельца от 2026-10-02: наш нативный Rust harness предназначен для
Long Horizon. OC2 TS v2.0.12, pinned
`2670273ff17da96f85c5826ced57aa1b368754fa`, остаётся источником поведения там,
где не объявлено отличие; его ограничения длительности задачи не являются
целью паритета. Этот документ фиксирует решение форка, не новую задачу,
план реализации или доказательство PASS.

## Продолжение без искусственной квоты раундов

Root и child продолжают одну принятую задачу до настоящего завершения,
отмены или явной ошибки/невозможности безопасного продолжения. Успешные
LLM/tool steps не исчерпывают общий бюджет раундов. Нельзя заменять лимит
8/16 большим числом, обнуляемым счётчиком или принудительным финальным
ответом с отключёнными tools.

В оригинале `packages/core/src/session/runner/llm.ts:227–245` настроенный
`agent.steps` включает финальный `MAX_STEPS_PROMPT` и `toolChoice:none`.
Эту семантику `steps`/legacy `maxSteps` наш форк **не переносит**; отличие
должно быть явно диагностировано при приёме соответствующего config,
а не молча представлено как поддержанный step budget.

Это не бесконечный retry и не бесконечные ресурсы: конечная allowance
ошибок одного логического LLM step, отменяемый backoff, timeout, model/
byte/context admission, permissions, child-depth/concurrency и quarantine
сохраняются. Tool error не становится generation retry; подтверждённые
или неизвестные внешние effects не переисполняются автоматически.

## Возобновляемая рабочая память, не накопление всего прошлого

Hot state содержит текущую задачу, выбранные полезные факты и свежий хвост.
Закрытые группы могут заменяться самостоятельным summary; ненужные данные,
включая прежние summaries, разрешено намеренно забывать. Возраст задачи,
число прошлых steps/compressions и глубина старой истории не должны
накапливать обязательный resident/provider груз.

Raw history, результаты/effect state и provenance остаются durable отдельно
от модельной памяти. Обычные retry, model switch, DCP, `/compact` и restart
восстанавливают committed hot view, не воскрешают забытый архив. Явный
разрешённый read/context selection или conversation Undo — отдельное действие.
Пользовательские `.md`/Git заметки полностью опциональны: harness не требует
их, не пишет/коммитит автоматически и не вводит скрытый archival recall.

Уточнение владельца от 2026-10-03: LLM выбирает полезные факты, устаревшие
подробности и момент финального ответа. Сжатие не ждёт завершения задачи:
на безопасной закрытой границе существующий `compress` может обновить рабочее
представление task/context pack внутри одной продолжающейся задачи. Точный
исходник остаётся durable, но не становится постоянным verbatim грузом HOT.
Runtime не угадывает смысловое «готово» по тексту, шагам или времени;
`length`, error/cancel и окончание provider step не доказывают выполнение задачи.
Явные user protections/permissions и in-flight safety сохраняются.
Это уточнение плана, не claim реализации: [T45/R9](DCP.md#taskpack-hot-renewal-and-completion--approved-2026-10-03-pending).

## Выбранные сообщения основного чата при делегировании

Утверждённое native расширение позволяет оркестратору явно выбрать сообщения
основного чата через optional `context_message_ids`. Ребёнку передаются точные
поддержанные текстовые user/assistant сообщения с исходными ролями/provenance,
в хронологическом порядке, как цитируемые пользовательские данные рядом с задачей,
не как новые system/developer instructions. Без явного выбора parent transcript
не копируется. Собственный профиль, environment, применимые AGENTS, tools и skill
metadata ребёнка собираются отдельно; skill bodies загружаются через native `skill`.

В pinned OC2 `packages/core/src/tool/plugin/subagent.ts:29–62` нужный контекст
передаётся обычным текстом в `prompt`; отдельного выбора parent messages по IDs нет.
`sessionID` продолжает собственную историю ребёнка, а не импортирует сообщения
родителя. Отличие форка — точный runtime-selected пакет с provenance и неизменяемым
admission snapshot, а не изобретение передачи контекста текстом.

**Статус:** утверждено в [T45/R8, CTX01/CTX02](goals/2026-09-21-config-compat-and-subagents.md);
реализация и qualification pending. Snapshot неизменяем в RAW, но его HOT-представление
может обновляться по R9 без повторной подстановки всего исходного пакета.

## DCP Compress встроен и отключается через конфиг

DCP Compress — compiled-native часть `oc`, а не обязательный внешний
JS/npm plugin host. Сохраняются pinned DCP provenance и AGPL уведомления.
Утверждённый 2026-10-03 [donor upgrade](DCP.md#dcp-320-donor-upgrade--approved-2026-10-03-pending)
на 3.2.0 — pending source-delta qualification; текущий compiled baseline ещё 3.1.15.
Полезны OC2 protections, protocol-safe projection/compaction и anchored nudge replay,
но donor TS adapter не заменяет Rust harness и не решает intentional forgetting.
Его nested summaries/child-result preservation не становятся обязанностью пожизненно
удерживать или снова загружать child transcript. Task/pack HOT обновляется ещё внутри
той же задачи; ordinary restart/compact восстанавливают latest HOT, не исходный архив.
Native defaults 40%/55%/summaryBuffer=false, child DCP=true и `compress.enabled`
остаются утверждёнными отличиями; donor defaults и unsupported Ask не переносятся.
Настоящий native Ask/Deny, resource/security guards и no unknown-effect replay intact;
CodeMode, компактные donor IDs и visual qualification T44 не входят в этот upgrade.
Смена source pin после квалификации не является Long Horizon/DCP11/A10 PASS.

Пользователь может отключить DCP общим `enabled:false` в `dcp.json/jsonc`
или inline `dcp` в admitted `opencode.json/jsonc`, например:

```json
{"dcp": {"enabled": false}}
```

Утверждённый native `compress.enabled:false` (реализация pending) отключает новые model/manual/API
compressions, не отключая остальные механизмы приложения. Он не подменяет
`compress.permission` и не обходит Ask/Deny; global off сильнее. Off/on
сохраняет raw history и committed summaries, не возвращает забытые данные.
Native `/compact` — отдельный механизм, не обязательная активация DCP.
Отключение compression не отменяет resource admission: если безопасное
продолжение не помещается, нужен честный отказ, а не silent truncate.

**Статус, не обещание реализации:** native DCP и общий `enabled` уже имеют
кодовые owners. Снятие successful-step round stops и bounded closed-current-task
RAW/HOT seam реализованы и отдельно квалифицированы:
[round removal](../evidence/T45/round-removal-short.md),
[current-task RAW/HOT](../evidence/T45/hot-raw.md). Это не whole-past DCP11/A10,
завершение Long Horizon или всей T45. Whole-past renewal, обновление task/pack HOT
внутри незавершённой задачи и оставшиеся off/manual/API controls, включая
`compress.enabled`, ещё требуют реализации/qualification. Точный config-контракт и gaps —
[DCP controls](DCP.md#compression-switch-and-effective-config-controls--t45r9dcp12-approved-2026-10-02-pending).
Приёмка Long Horizon требует измеренных bounded RAM/checkpoint/I/O при
растущем прошлом и одинаковом hot state; прохождение 17 steps доказывает
только снятие старого round stop. Актуальное исполнение — `GOAL.md` и
`progress/STATE.json`, не этот документ.
