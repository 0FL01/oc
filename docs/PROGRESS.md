# Прогресс и восстановление после compaction

## Дизайн: файлов достаточно

```text
progress/
  STATE.json                 # маленький canonical machine state
  NOW.md                     # только актуальный handoff
  INDEX.md                   # этапы и текущая задача
  M3/
    INDEX.md                 # задачи этапа, status и latest leaf
    T14/
      INDEX.md               # последние 12 checkpoint links
      0001.md                # неизменяемый factual slice
      0002.md
```

`planning/tasks.json` — task definitions/dependencies, не журнал. `STATE.json` — их statuses/current/latest refs и указатель на последний checkpoint, без transcript. `NOW/INDEX` — generated views, не независимые competing sources of truth. Git определяет фактические файлы/commits/delivery; journal хранит только recovery handoff. Journal leaf immutable, factual, не chain-of-thought. Полные command logs — `.local/evidence/`, вне Git; в `evidence/` маленькие sanitized reports.

Ограничения utility: note ≤6144 UTF-8 bytes; NOW ≤8192; каждый INDEX ≤8192; note содержит четыре headings Result/Checks/Risks/Next. Размер измеряется bytes, не якобы точными токенами. После 12 записей task index показывает последние 12; старые файлы остаются на диске и ищутся targeted `rg -l`/по имени. Root/phase indices не растут на каждый checkpoint. Общая история может быть большой, resume-набор от этого не растёт.

Нет embeddings, retrieval server, базы для planning, автоматического summarizer или бесконечного PROGRESS.md. Нельзя склеивать журнал в один вход модели.

## Команды

```sh
python3 scripts/progress.py show
python3 scripts/progress.py start T00
# создать небольшой note: четыре раздела из шаблона ниже
python3 scripts/progress.py checkpoint --note .local/checkpoint.md
python3 scripts/progress.py finish --note .local/checkpoint.md --evidence evidence/T00/report.md
python3 scripts/progress.py block --note .local/checkpoint.md
python3 scripts/progress.py reindex
python3 scripts/progress.py check
```

`finish` требует active task, выполненные зависимости, непустой report внутри evidence/ и допустимый note. Это структурная проверка, НЕ независимое доказательство корректности кода или PASS тестов. Tests выполняет агент/CI; report должен честно указывать commands/exits и measured/source-derived/synthetic метод.

`start` может возобновить blocked task после устранения причины; не может перескочить dependencies или открыть второго active task. `checkpoint` нужен при interruption/non-idempotent external action/существенном незакоммиченном handoff и оставляет task active; `block` освобождает active slot. `finish` переводит task в done. Для повторного исправления completed task использовать `reopen ID --reason ...`; utility снимает done только с указанной задачи, запрещает reopen при завершённых зависимых tasks — в таком случае оформляется новый исправляющий task/явный пересмотр графа, а не скрытая инвалидность.

## Шаблон записи

```markdown
## Result
Реализован SSE decoder. Изменены crates/oc-adapters/...; код HEAD abc123, после него рабочий diff этих файлов.
## Checks
cargo test -p oc-adapters sse — exit 0; 8 tests. Это offline fake-wire test, не live proxy test.
## Risks
Не проверен continuation после tool call. Никаких network side effects не запускалось.
## Next
Добавить fixture split UTF-8 внутри tool arguments и выполнить cargo test -p oc-adapters sse.
```

Не писать «работает» вместо проверяемого evidence. Не создавать note только после validator/staging/push. Указывать один точный инженерный следующий шаг, краткие риски и полезные pointers. Log files не копировать в note даже при неудаче; оставить error category и path к redacted excerpt.

## Crash consistency

Utility использует short-lived OS file lock, сначала атомарно пишет новый immutable leaf, затем STATE через temp+fsync+replace, затем derived indices. `check/reindex` не выполняют code/network/Git commands. Несогласованный NOW после аварии восстанавливается `reindex` из STATE/latest leaf.

Авария между leaf и STATE оставляет orphan checkpoint. `check` замечает лишний seq; не удалять его и не считать task завершённой. Агент читает один orphan и реальный Git diff, фиксирует reconciliation: переносит конкретный orphan в `.local/recovered/` (копию сохранить), затем записывает новый checkpoint на основании проверенных фактов. Не выполнять заново shell/MCP ради восстановления красивой записи. Если статус task ambiguous — оставить active/blocked, не done. Эта редкая ручная reconciliation намеренно проще отдельного transactional journal engine.

Когда active task отсутствует после finish/block, NOW сохраняет последний checkpoint и отдельно показывает следующие ready tasks. Поэтому crash до Git commit не стирает handoff.

Незавершённые uncommitted edits при resume сохраняются и проверяются; нельзя начинать task заново с `git reset`. Generated indices не заменяют code review. Другой процесс/человек меняет STATE/Git в то же время → single-writer conflict, остановка до согласования.

## Resume read budget

AGENTS + GOAL + NOW + фактический Git → current task → targeted contracts/source/tests. Root/phase indices и latest leaf уже отражены в NOW и открываются отдельно только для recovery. Старые phases и upstream README не перечитываются без причины. Постоянные архитектурные решения остаются в docs, важная локальная находка — в task leaf, ближайшее действие — в NOW.

## Изменение графа при настоящем blocker

Добавление correction task — явная редкая операция: обновить planning/tasks.json, stage spec и test mapping, добавить в STATE новый todo entry с last_seq=0/latest=null/evidence=null/reopen_reason=null. Существующие IDs/notes/statuses не удалять. Затем check_docs/reindex/check; такой пересмотр требует rationale в checkpoint, а не скрытого снятия acceptance gates.
