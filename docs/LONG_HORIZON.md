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

## DCP Compress встроен и отключается через конфиг

DCP Compress — compiled-native часть `oc`, а не обязательный внешний
JS/npm plugin host. Сохраняются pinned DCP provenance и AGPL уведомления.
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
кодовые owners. Удаление round stops, bounded current-task hot/raw seam и
полные off/manual/API guards, включая новый `compress.enabled`, ещё требуют
реализации/qualification. Точный config-контракт и известные gaps —
[DCP controls](DCP.md#compression-switch-and-effective-config-controls--t45r9dcp12-approved-2026-10-02-pending).
Приёмка Long Horizon требует измеренных bounded RAM/checkpoint/I/O при
растущем прошлом и одинаковом hot state; прохождение 17 steps доказывает
только снятие старого round stop. Актуальное исполнение — `GOAL.md` и
`progress/STATE.json`, не этот документ.
