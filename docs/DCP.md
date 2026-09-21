# DCP native port — обязательное контекстное ядро

Основа владельца: `compress` и подсказки агенту о сжатии. Source baseline: DCP 3.1.15, commit из `planning/baseline.lock.json`, AGPL-3.0-or-later. [D1–D3] в SOURCES. Это перенос функционального ядра, не npm package/auto-updater и не заявление о полном parity всех экспериментальных возможностей.

## Входит в goal

Range compression с несколькими spans, stable message/block references, вложенные summaries, protected tool/file/user/tag content; system/tool instructions и context/turn/iteration nudges; deduplication, purgeErrors; эффективный DCP config; durable state и replay; `/dcp` context/stats/manual controls и `/dcp-compress [focus]`.

Experimental message compression, subagent integration и custom prompt overrides отложены. Explicit enable неподдержанного режима — UnsupportedCapability, не молчаливый fallback на range. AutoUpdate/auto-generated upstream config не выполняются; native code обновляется только сборкой. В native profile DCP включён явно, не зависит от npm registry.

## Архитектурная граница

Raw session messages/parts не удаляются и не перезаписываются. DCP policy принимает bounded conversation view и выдаёт `ProjectionDelta`. Core валидирует anchors/generation/protections/call-result graph и транзакционно сохраняет план. Provider serializer работает над projection; TUI history показывает исходные данные с отметками compression. Fixed runtime/agent/AGENTS lanes собираются отдельно и не сжимаются/дублируются DCP. Loaded skill сохраняется как обычная call/result group. Не хранить вторую history в DCP JSON file/глобальном cache.

Summary — model-authored payload инструмента, не автоматически вызванная вторая модель. Никаких скрытых платных summarizer requests. Процесс DCP не управляет compaction authoring-agent; его продолжение обеспечивает `progress/`.

## Модельный инструмент

Range contract из upstream types [D3]:

```json
{
  "topic": "Закрытая часть задачи",
  "content": [
    {"startId": "m0001", "endId": "m0012", "summary": "Проверенные результаты и важные ограничения"}
  ]
}
```

`topic`/`summary` — непустые bounded strings; `content` — непустой bounded массив. Public shape не заменять произвольным `{text:...}`. Syntax/format реальных raw/block references и placeholder expansion взять из pinned source+tests, а не только примера выше. В первом task DCP сохранить schema и fixture IDs с provenance. Shape message mode не подмешивать в range.

Нельзя сжимать active незавершённый provider/tool batch; нельзя рвать связи call/result/reasoning. Invalid/stale/cross-session IDs, backwards range, конфликтующие spans и отсутствующие required nested references дают no-mutation error. Допустимые upstream overlap/embedding случаи переносить по fixtures, а не запрещать вообще ради удобства.

Compression планируется для всего batch до commit. Summary может включать ссылку на старый block; nested content раскрывается/связывается по upstream semantics с цикло/размерными limits. Пределы depth/bytes срабатывают до allocation blow-up; не recursively materialize неограниченную историю. Сложный вложенный block не заменяется заглушкой, скрывающей данные.

Token saving фиксируется как measured/estimated; сохранность смысла не следует из меньшего размера. При summary, не уменьшающем проекцию, не создавать бесконечный compression loop: visible no-gain outcome, сохранить исходную projection и разрешить другой selection. Этот anti-loop guard — наш safety difference.

## Protections и унификация patch

Upstream protects tool/file content и умеет preserve user messages/tags [D1]. Перенести required config и exact behavior из fixtures. В новом toolset добавлять `apply_patch` к защите, эквивалентной protected write/edit. У patch вычислять `affected_paths` из распарсенного patch до исполнения; protected glob проверяет все пути, а не отсутствующий параметр `filePath`.

Summary хранит concise mutation outcome/важные protected outputs и references на большие blobs. Нельзя подменить требуемый verbatim protected content ссылкой, когда policy требует verbatim; слишком большой protected payload может сделать compression невозможной, это явный outcome. Не append всей истории/огромного patch к каждой summary «для сохранности».

Permissions `compress:allow|ask|deny` независимы от file mutations. DCP summaries/теги не могут расширить privileges. Labels/IDs выдаёт runtime, не доверенный user text — экранировать collisions и prompt-injection содержимое.

## Nudges и automatic strategies

Стартовые upstream-derived значения required profile: minContextLimit 50000, maxContextLimit 100000, nudgeFrequency 5, iteration threshold 15, nudgeForce soft, summaryBuffer true. Это soft policy, а не потолок model capacity. Числовые и процентные thresholds + per-model overrides поддержать в DCP config; model IDs в код не зашивать. Явные user overrides читаются из dcp.jsonc.

Для меньшего окна с учётом requested output/safety reserve thresholds не должны требовать отправки недопустимого input; runtime hard admission сильнее DCP reminder. Effective thresholds и причины clamp доступны в DCP panel. Для models без limits не выдавать estimate за точный token counter.

Nudge timing/placement и counter resets перенести из pinned hooks/prompts/tests. Не добавлять новую persistent reminder message на каждый token/chunk; emitted prompt projection не должна со временем накапливать дубликаты nudges. Context-limit reminder повторяется по частоте, пока превышение реально сохраняется; успешная compression пересчитывает counters. Manual mode отключает autonomous tool invocation/instructions в объёме baseline; manual automaticStrategies control не игнорируется.

Dedup: одинаковые tool name+canonical arguments, оставить последний нужный output, protections сохраняются. PurgeErrors: после configured turns (default 4) убрать из projection крупный input errored call, но сохранить error outcome. Эти стратегии пересчитываются при compress, как описано upstream; не запускать pruning на каждом request из инерции старых версий. Случаи manual mode уточнить по executable fixture; README не заменяет source test.

## Config surface

Поддержать enabled; pruneNotification/type; commands.enabled/protectedTools; manualMode; turnProtection; protectedFilePatterns; compress range/permission/showCompression/summaryBuffer/min/max/model overrides/nudgeFrequency/iterationNudgeThreshold/nudgeForce/protectedTools/protectTags/protectUserMessages; strategies.deduplication/purgeErrors. `debug` включает только безопасные metadata logs. Notification `toast` может отображаться как TUI status notice — documented UI difference.

Config source order фиксируется отдельной source-derived fixture вместе с general config roots; `cli.json` сюда не входит. Sources проходят explicit trust boundary и остаются read-only. Native enabling не добавляет npm package. Supported aliases — exact `@tarquinen/opencode-dcp`, `@tarquinen/opencode-dcp@3.1.15` и пользовательский `@tarquinen/opencode-dcp@latest`; все три дают один instance repository-pinned compiled revision. Последний не резолвится через npm/registry. Semver ranges и другие versions — `UnsupportedPlugin` до package/network side effects.

## Acceptance и source trace

Нужны upstream-derived fixture groups: range boundaries/nested IDs/protections, strategy timing, nudges/counters, config merge, cancelled/failed compress, restart projection и compression-before-tools continuation. Test method фиксируется: source-derived / differential executed / synthetic, никогда один под видом другого.

Fake-model scenario заставляет вызвать compress на большом закрытом span, затем потребовать fact из summary и выполнить patch/test. Assert immutable raw history checksum, stable IDs после restart, уменьшенную serialized projection, preserved facts/полные tool-call-result groups/protected bytes, отсутствие config/provider credentials, valid tool graph и restart consistency. Live regression проверяет реальный endpoint, но не обещает универсальную semantic losslessness.

## Квалифицированный native path (T36)

Normal `oc run`/TUI runtime публикует `compress` как ordinary function tool рядом
с `glob`/`grep`; все три проходят общий validation → permission → durable intent →
dispatch → durable outcome. DCP developer lane содержит bounded ordered anchors
`{id,role,closed}` и transient nudge, но не записывается в raw history. Disabled,
manual или неразрешённый compress не публикует schema/anchors/nudge.

Модельный compress сначала строит весь plan. Одна SQLite transaction сохраняет
blocks/members, consumption старых memberships, durable dedup/purge projection,
tool outcome, turn wire journal и reset nudge state. No-gain не создаёт block и
не сбрасывает cadence. После commit следующий round заново строит projection.
SIGKILL regression останавливает actual binary на durability sync внутри этой
transaction и после restart допускает только zero-or-complete state без replay.

Nudge state durable и изолирован ключом session/provider/model. Percent limits и
exact provider/model overrides вычисляются от context limit выбранной модели;
summaryBuffer расширяет hard boundary, но не откладывает первое soft reminder.
`nudgeForce` принимает `strong|soft`. Успешная compression сбрасывает cadence
только своего ключа. Manual mode отключает autonomous strategies/reminders.

Dedup/purge решения появляются только вместе с успешной compression и переживают
restart. Identity включает `call_id` и occurrence, поэтому reuse ID не смешивает
вызовы. Dedup использует canonical JSON, purge удаляет только большой старый input
ошибочного call и сохраняет exact outcome. `*`/`?` tool protections, typed file
paths (для patch — parsed affected paths), recent-turn protection и user/tag/file
content сохраняются. Complete call/output pairs никогда не рвутся. Block anchors
могут быть вложенными; consumed block rows остаются для bounded expansion, а их
active memberships заменяются транзакционно.

`dcp.json/jsonc` и inline `dcp` читаются в общем порядке admitted config roots,
read-only, bounded 1 MiB, regular UTF-8, no-follow. Deep object merge поддерживает
required surface выше. `autoUpdate:true`, unsupported modes/custom prompts и
subagents дают explicit unsupported error. `showCompression`, notification и
commands display сохранены в snapshot, но их UI controls квалифицируются T39.

Offline evidence: `evidence/T36/report.md`. Полная archive materialization и
process-wide lifetime/RSS gates остаются T40, не считаются закрытыми этим срезом.

До переноса кода/prompts/tests сохранить license/notices/provenance. Rust перевод не удаляет лицензирование источника. Не копировать unrelated OpenProxy code с неустановленной лицензией.
