# T52 — укрупнённые code slices и отдельные тесты

## Контракт и границы

Статус при принятии: **план, NOT_STARTED**, не выполненный рефакторинг. Task definition: `planning/tasks.json`; ARCH01–ARCH05: `planning/acceptance.json`; execution state: `progress/STATE.json`. Текущая T44 active/PAUSED остаётся неизменной до явного resume. Первичный проход — одна T52 с R0–R7, а не новый task/crate/framework на каждый файл.

Источник инвентаризации — присланный архив ветки `agent/oc-rust-port`, ZIP comment `ddb6ae2f0cba18c4aa161159899228a7343398eb` (commit `fix(tui): preserve exact-fit words in comparable DCP frames`). Архив не содержит Git worktree metadata и заведомо неполон по большим TUI captures. Это достаточная база для структурного плана, но не доказательство текущего host status или запуска тестов. Перед исполнением сверить фактический HEAD/diff; при drift пересчитать числа и скорректировать швы, не откатываться к архиву.

Цель: agent-only разработка с общим окном 400k должна читать владельца нужного поведения, а не монолит с тысячами посторонних тестов. Сохранить четыре packages, реальные execution owners и текущий продуктовый контракт. Мягкий ориентир — 5 000 физических строк на рукописный code/test-файл, обычно крупная тематическая часть порядка 1–4k. Нет нижнего минимума, hard CI line gate или обязательства заполнить файл до 5k.

Не входят: новые parity features, обновление dependencies/toolchain/donor, перенос runtime из adapters в core, устранение разрешённого D12 read-side, новый lib target для `oc`, новые traits/crates/DI/scheduler/test framework, автоматическая чистка evidence, массовая миграция существующих маленьких модулей и старых `*_tests.rs`. Не менять product hashes/digests, SQL/wire/DTO форматы, admission/budgets и старые baseline locks ради запрета новых SHA256-манифестов для этого рефакторинга.

## Baseline: где реально находится объём

Физические строки, включая comments/blanks; измерено по `crates/**/*.rs`. В архиве 135 таких файлов, 155 871 строка, восемь файлов строго больше 5 000. «До suite» — позиция перед основным inline test block, а не точное число production SLOC: выше ещё встречаются cfg(test) probes/методы/отдельный тест.

| Файл относительно `crates/` | Всего | До основного inline suite | Первое решение |
| --- | ---: | ---: | --- |
| `oc-tui/src/app.rs` | 17 604 | 8 772 | Отдельные tests; затем 4 тематические части + owner. |
| `oc/src/tui_cmd.rs` | 8 502 | 3 247 | Отдельный unit-suite с двумя крупными частями; production пока оставить. |
| `oc-adapters/tests/runtime.rs` | 7 930 | — | Один прежний Cargo target, несколько тематических файлов. |
| `oc-tui/src/messages.rs` | 7 408 | 3 667 | Один отдельный test-файл; renderer дополнительно не дробить. |
| `oc-adapters/src/runtime.rs` | 6 189 | 5 754 | Отдельный unit test pack; turn/context/MCP parts + owner. |
| `oc-tui/src/shell.rs` | 5 561 | 2 204 | Один отдельный test-файл; composition кадра оставить. |
| `oc/tests/pty_t39.rs` | 5 496 | — | Один прежний Cargo target, две крупные тематические части. |
| `oc-adapters/src/storage.rs` | 5 177 | 3 499 | Отдельный inline suite; существующие storage parts сохранить. |
| `oc-adapters/src/application.rs` | 4 553 | 3 270 | Упреждающе отделить оставшиеся inline suites; production пока оставить. |

Следовательно, после выноса больших тестов основные production превышения — `app.rs` и `runtime.rs`. Не превращать автоматически все остальные строки таблицы в новые production-иерархии. Файлы `composition.rs` (2 302), `config.rs` (2 236), `core_app.rs` (2 227), `provider.rs` (2 193), `tools.rs` adapters (2 138) не требуют распила только по размеру.

Отдельный сигнал для контекста: `scripts/tui_capture/capture.mjs` имеет 2 954 строки, но 221 185 bytes. Это не девятое превышение и не обязательная миграция T52; показывает, почему LOC не заменяет byte/token awareness и scoped reads. JSON/PNG/cells/raw logs/upstream/assets не добавлять к метрике рукописного Rust-кода и не читать вместе с ним.

## Порядок исполнения

R0 → R1 → R2 → R3 → R4 → R5 → R6 → R7. Каждый срез может состоять из нескольких independently checked commits; не требуется один огромный commit на весь crate. Сначала mechanical relocation/import/path fixes, затем отдельный commit для необходимого исправления поведения, только если оно действительно найдено и разрешено текущим feature contract. Заодно «улучшать» алгоритмы, assertions, formatting всего workspace и names нельзя.

T52 не зависит от полного завершения T44/T45/T46/T50/T51. Их текущие owner paths переносятся без реализации pending features. Полные completion dependencies между ними не добавлять. До нового функционального роста этих owners завершить соответствующий первый проход T52; обычное выполнение после T52 возвращается к прежнему feature плану.

### R0 — безопасный старт, baseline и отчёт размеров

**До разрешённого resume никаких code edits/start/capture campaigns.** Применение planning patch оставляет T44 active/PAUSED. После явного разрешения продолжить с T52:

1. Проверить `git status --short`, `git rev-parse HEAD`, `git diff --stat`, `git diff --cached --stat` и state. Чужие/неизвестные изменения, включая ранее отмеченную `?? .opencode/`, не читать/стейджить/чистить. Не использовать reset/stash/clean для удобства распила. Уточнение нужно только при реальном конфликте/неизвестном состоянии, не для повторного согласования уже заданных правил.
2. Если T44 всё ещё active: написать новый короткий scheduling note с `## Result`, `## Checks`, `## Risks`, `## Next`, фактическим Git state и сохранённым next T44 slice. Указать временную паузу ради T52, отсутствие нового failed parity gate и условие возврата. Через `python3 scripts/progress.py block --note <repo-relative-note>` освободить active slot; затем `python3 scripts/progress.py start T52`. Старые leaves не менять. Если active task уже другой, не блокировать его автоматически по устаревшему плану.
3. Зафиксировать baseline Git commit перед переносом. Выполнить применимые baseline checks на реальном разрешённом host с pinned toolchain. Исторический PASS из T44 не выдавать за текущий результат. При настоящем pre-existing failure записать его до правок; missing prerequisites = NOT_RUN/blocker, не PASS.
4. Сохранить реальные test lists/ignored membership для затронутых targets и baseline размеры. Raw списки не печатать в prompt целиком; отчёт содержит totals, различия и ссылки. Сначала проверить prerequisites/resource envelope из `AGENT_RUNBOOK.md`; не запускать одновременно Cargo/capture в одном target.

**Разведка:** можно выдать три параллельных read-only задания на одном base: UI state/render; runtime/application/storage; test/harness/path consumers. Ответ каждого: файлы и owner symbols, зависимости/visibility seams, конкретные риски, target/filter. Не давать всем весь spec/source/evidence и не возвращать полные transcripts. Coordinator проверяет швы и один изменяет worktree; новая multi-worktree write policy не вводится.

**Реализовать небольшой advisory helper** `scripts/code_size.py` и его тесты `scripts/test_code_size.py`, стандартная библиотека Python, без новых dependencies. Это два самостоятельных dev-tool файла, а не framework. Контракт CLI:

```text
python3 scripts/code_size.py
python3 scripts/code_size.py --base <git-commit>
python3 scripts/code_size.py --base <git-commit> --changed
```

Отчёт по owned hand-written `crates/**/src/**/*.rs`, `crates/**/tests/**/*.rs` и коду `scripts/` (`.py/.mjs/.js/.ts/.sh`): physical lines, UTF-8 bytes, роль source/test/dev-tool, delta к base и WARN >5000. Отдельные test-файлы внутри src распознавать по принятому layout и existing `*_tests.rs`; смешанный старый файл честно помечать mixed/unknown, не считать cfg блоки regexp «точным production LOC». Вывод по размеру, default summary без исходников и без тысяч zero-warning строк; requested detail сохраняется отдельно.

Список файлов брать через Git, а не recursive walk всего checkout: tracked плюс разрешённые untracked new source paths (`git ls-files -z --cached --others --exclude-standard -- crates scripts`, deduplicate). При `--changed` учитывать tracked изменения от base, staged/unstaged и такие новые файлы; корректно обрабатывать rename/delete, не считать только git diff старых путей. Base читать Git-командами по repo-relative paths без shell interpolation; новый файл не имеет выдуманного base size. Не следовать symlinks за пределы worktree. Generated/vendor/fixtures-data/assets/evidence/target/donor/references/.local/.opencode исключены; новые явно generated files не скрывать одной пустой меткой автора, исключение объяснить.

WARN >5000 возвращает exit 0; ошибки Git/I/O/CLI/невалидный base — nonzero и видимый текст, а не пропуск файлов. Tests helper: 4999/5000/5001, последний LF/no-LF/CRLF, Unicode bytes, новый/staged/unstaged/renamed/deleted путь, excluded data, symlink, git failure. Счётчик не минифицирует/пишет/хеширует файлы и не обновляет какой-либо baseline автоматически. Не встраивать превышение как assertion в `check_docs.py`/Cargo/CI; вызывать helper для affected slices и в closeout, показывая warnings.

**Выход R0:** реальный base, reproducible source/test inventories, короткий owner/read-set plan, working warning helper; без claims о завершённом распиле. Новые test cases helper учитываются отдельно от переносимых Rust tests.

### R1 — вынести тяжёлые TUI/unit suites

Менять один owner за раз; после каждого переезда скомпилировать/выполнить его target. Использовать обычные Rust modules, не `include!` и не новые test-only public APIs.

| Исходник | Целевой test layout | Сохранить |
| --- | --- | --- |
| `oc-tui/src/app.rs`, основной `mod tests` с 8773 | `app/tests.rs` с общим fixture и `app/tests/{input,transcript,tabs,lifecycle}.rs` | Все существующие сценарии/атрибуты, bounded fakes, clocks и UI counters. Группы именовать по поведению, не номерам chunks. |
| `oc-tui/src/messages.rs`, `mod tests` с 3668 | `messages/tests.rs` | Один suite около 3.7k, без новых markdown/render engines и разбивки на десятки cases. |
| `oc-tui/src/shell.rs`, `mod tests` с 2205 | `shell/tests.rs` | Один suite около 3.4k, без production tabs/sidebar tree только ради метрики. |
| `oc/src/tui_cmd.rs`, `mod tests` с 3248 | `tui_cmd/tests.rs`, `tui_cmd/tests/{routing,lifecycle}.rs` | Unit target `--bin oc`, текущий `approval_tests.rs` и его связь с tui_cmd остаются; не заводить lib target. |

Общий fixture каждой группы держать в ближайшем test-owner, а не копировать fake servers в каждый новый файл и не создавать общий crate для всех тестов. В `tests.rs` допустима существенная общая setup-часть; пустые файлы-шаблоны для будущих tests не создавать. При слишком неравномерном составе перераспределить целые тематические серии, а не разрывать один сценарий посередине. Имена предложены по текущим обязанностям, точную группировку подтвердить R0.

**Конкретные ловушки этого дерева:**

- `messages.rs` использует `include_str!("../../../tui-recovery/fixtures/transcript.md")` в четырёх сценариях (5802, 5901, 6959, 7175 на base). После переноса в `messages/tests.rs` старое число `..` уже неверно. Сделать путь crate-root anchored через `concat!(env!("CARGO_MANIFEST_DIR"), "/../../tui-recovery/fixtures/transcript.md")` либо правильный новый relative path; fixture bytes не менять.
- В app/messages/shell/tui_cmd есть cfg(test) поля, measurement hooks, маленькие helpers и cfg(not(test)) ветка clipboard. Нельзя переносить их wholesale в другой cfg или убирать instrumentation. Для test-only impl можно отдельный module без расширения production visibility; немногие связанные probes могут остаться у owner.
- `ScriptDriver`/`PumpOutcome` в `app.rs` объявлены публично без cfg(test); найденные в архиве call sites находятся в unit suite. Чистый перенос всё равно сохраняет их доступность. Отдельно учитывать общее правило: integration tests компилируют library по обычному production interface, без её cfg(test)-only helpers.
- `src/approval_tests.rs` подключён в конце tui_cmd через `#[path = "approval_tests.rs"]`. При неизменном родителе путь корректен; не переносить декларацию внутрь нового `tui_cmd/tests.rs` без проверки module/path resolution.

**Проверки:** `cargo test -p oc-tui --locked --lib app::`, затем соответствующие `messages::`/`shell::`; `cargo test -p oc --locked --bin oc tui_cmd::`. Проверить actual matched counts, `--list`/ignored mapping и cfg(non-test) build. Expected output/golden не обновлять. Pure tests move сам по себе не требует всей T44 capture history.

### R2 — разделить production TuiState по поведению

После R1 снова измерить app.rs: остаётся около 8.8k, это самостоятельная проблема. Сохранить `crates/oc-tui/src/app.rs` как owner/API и добавить **четыре** содержательных implementation parts. Не переносить весь остаток в `app/state.rs`.

| Путь | Ответственность и исходные anchors |
| --- | --- |
| `app.rs` | `TuiState`, общие `TuiStatus`/`TuiPanel`/`PanelIntent`/`KeyOutcome`, конструирование `new`/`new_home`/`with_session`, attach/reset/session invariants, общие deadlines/status и явные exports. Это единственный state owner, не второй dispatcher всех алгоритмов. |
| `app/input.rs` | `handle_key`, `handle_enter`, `handle_panel_key`, `run_command`, `terminal_key`/`conversation_key`, paste/mentions/slash, picker/session/rename input, top-level `handle_mouse` dispatch. Реальное выполнение PanelIntent остаётся в binary. |
| `app/transcript.rs` | history pages/rows, `attach_page`/`refresh_completed_page` и paging helpers, rendered/visible projection, selection/hit-testing/copy, viewport/cache/scroll/wheel, `card_row`/footer helpers. Markdown implementation остаётся в `messages.rs`/`styled.rs`, а clipboard side effect — в binary. |
| `app/tabs.rs` | tab-owned types/animation, presentation/hover/close snapshots, `prepare_tabs`, `enter_tab_at`, `tick_tabs`, pulse/marquee deadlines and hit helpers. Render в `shell.rs`, persistence/deck ownership — в binary/application, не в этом файле. |
| `app/live.rs` | submission reconciliation и turn-scoped `apply_delta`/reasoning/presentation/usage/finished/interrupted/failed, tool/approval projections, live parts, DCP/compaction display updates, `ScriptDriver`/`PumpOutcome`. Никакого исполнения tools/DB/MCP. |

Не дублировать lifecycle, который пересекает темы: каждому методу назначить один owner, общие координаторы оставить в app.rs. `attach_page` входит в transcript, а управление сменой attached session — в root; названия в таблице не разрешают две реализации одного метода. Ближайший владелец определяется по доступам/инвариантам, а не по непрерывному диапазону строк.

Ожидается root существенно меньше 5k и части преимущественно порядка 1–3k, но точные LOC не acceptance обещание. Если input остаётся близок к 5k, сначала оценить реальный cohesive panel slice; не создавать по одному файлу на key/action. Если общая распилка требует постоянно читать все пять частей, пересмотреть швы, а не добавить ещё indirection.

`impl TuiState` в дочерних файлах работает с тем же объектом/полями. Видимость moved private methods, вызываемых соседними parts, выбрать минимальную (`pub(super)`/scoped); поля не раскрывать всем crates. Tab-private types могут уйти целиком в tabs с узким доступом для root; прежние `app::TabPresentation`/прочие внешние пути сохранить explicit re-exports. Не вводить `Context`/`Services` holder с копией всех полей.

**Сохранить:** stale-turn rejection, bounds/retained-byte metrics, history/cache invalidation, selection/clipboard semantics, tab identity/hover/animation timing, pending submission/fresh quit, scrolling/wrapping и порядок approval/DCP/compaction events. Не менять default state, timestamps, polling deadlines, allocation/resource caps или порядок side effects вместе с файловым переносом.

**Проверки:** oc-tui app/messages/shell unit suites; `cargo test -p oc --locked --test tui_render_alloc`; затронутые существующие binary PTY/golden cases. Для altered render/projection paths — bounded deterministic full-frame regression по действующим T44 rules; размер/clock/state sequence фиксировать прежними fixtures/barriers, не новыми masks. Public API callers проверяются compile/tests в `oc`, а не только `cargo check -p oc-tui`.

### R3 — разделить Runtime, не менять execution ownership

Сначала убрать два inline packs (`identity_tests`, `vis38_review_tests`, с 5755/5768 на base) в `runtime/tests.rs`, сохранив labels через mapping при изменившемся module prefix. Existing `runtime_compaction.rs`, `runtime_compaction_tests.rs` и `runtime/tool_stream.rs` уже представляют работающие швы — не переносить их ради симметрии.

Затем четыре production файла, из которых root уже существует:

| Путь | Ответственность и anchors |
| --- | --- |
| `runtime.rs` | `Runtime<'a>`, публичные reports/errors/params/policy, shared state/locks/leases, construction/publish/reload/session facade и existing entry methods. Единый `next_turn_id` counter остаётся единым. |
| `runtime/turn.rs` | `run_turn_inner`, `run_turn_admitted`, `execute_units`, `admit_tool`, `guard_patch`, tool finish helpers, `run_child_turn`, `TurnSubagent`, child model/lane resolution и `commit_turn`. Это один связанный turn/admission/execution slice, не файл на provider round или tool. |
| `runtime/context.rs` | `active_projection*`, `active_rows`, `prepare_dcp_plan`/`measure_dcp_plan`, `run_compress*`, `wire_history`, `dcp_projection_estimate`, plan/protection/continuation and call-content helpers. Existing compaction engine/policy остаётся на своих местах. |
| `runtime/mcp.rs` | `AttachedServer`/`McpGeneration`/MCP call lease, `attach_mcp`/`ensure_mcp_generation`/`execute_mcp`/`retire_poisoned`, close/degradation/quarantine and MCP instruction/redaction/error helpers. Transport implementation остаётся в mcp_remote/mcp_stdio. |

Ориентировочная вместимость по текущим обязанностям: root 1–2k, turn 2–3k, context около 1k, MCP около 1k. Это оценка швов, не принудительное распределение строк. Не нужны отдельные новые `subagent.rs`, `approval.rs`, `executor.rs`, `manager.rs` и `types.rs` только ради равномерности: сначала воспользоваться существующими owner boundaries.

Сохранить facade paths вроде `runtime::Runtime`, `TurnParams`, `RuntimePolicy`, `builtin_tool_defs`, `expand_command` и используемые sibling модулями `resolve_subagent_model`, `apply_dcp_projection`, `dcp_call_identities`/`dcp_contents` через прежнюю видимость/explicit re-export, где определение переехало. Новые implementation modules private; новый public namespace не нужен.

**Особо review:** заимствованный `Db`/`Runtime<'a>` не заменять новым Arc/storage owner; не двигать lock guard drop относительно `.await`; scope `ActiveLease`/MCP leases и response-close-before-tool-admission сохранить. Child budget/policy/cancellation не усиливать/ослаблять. DCP по-прежнему provider projection, не rewrite raw history. Никакого повторного admission неизвестных effects, новых connections/joins или клонированных counters после разделения.

**Проверки:** `cargo test -p oc-adapters --locked --lib runtime::`, targets `runtime`, `subagent`, `dcp_atomic`, `context_bounds`, `mcp_remote`, `mcp_stdio` по affected boundary; `cargo test -p oc --locked --test dcp_runtime` и relevant binary recovery/MCP cases. Перед commit проверить callers в `application.rs`/`runtime_compaction.rs` и preserved exports, не только локальный impl. Новая поведенческая ошибка — отдельная repair, не silent rewrite в move commit.

### R4 — storage/application: тесты отдельно, владельцев сохранить

`storage.rs`: перенести основной suite с 3500 в `storage/tests.rs`; отдельный unit case у `bound_preview` (264+) перенести туда же с явной test-name mapping либо оставить как маленький inline exception с причиной. Test-only Db probes не открывать публично. Existing `storage_compaction`, `storage_conversation`, `storage_dcp_view`, `storage_fork`, `storage_grants` и их test packs сохранить. После выноса root около 3.5k; новые schema/queries/blob modules сейчас не обязательны.

`application.rs`: три оставшихся inline packs с 3270/3756/4025 объединить в отдельный `application/tests.rs` с тематическими вложенными test modules/именами. Объём около 1.3k не требует трёх новых файлов по нескольку сотен строк. Existing `application_conversation_tests.rs` и `application_fork_tests.rs` уже отдельно; не объединять всё обратно в новый oversized test-файл. `selection` и `tab_deck` остаются существующими production parts. Root после переноса около 3.3k, дальнейшее деление query/worker — только по реальному feature шву/росту, не обязательный scope T52.

**Сохранить:** Db lifecycle/root lock, transaction boundaries, schema/migrations, key/ID/blob formats, tool intent/outcome atomicity, active-window/reopen/recovery, generation publication, title jobs, tab deck CAS и actual persisted model/agent/Location. Не «улучшать» SQL, defaults и storage digests во время relocation.

**Проверки:** `cargo test -p oc-adapters --locked --lib storage::`, `application::`; existing targets `storage_lock`, `dcp_atomic`, `tab_deck`, `session_rename` и relevant binary `durability`/`recovery_startup`/`recovery_v02`/`recovery_v03`. Перечень запусков выбирать по moved scopes, а не заново придумывать дублирующие fixtures.

### R5 — разбить большие integration suites, сохранить targets

`oc-adapters/tests/runtime.rs` остаётся harness и ближайшим владельцем shared fake wire/runtime fixture. Сценарии выделить в `tests/runtime/turns.rs` (rounds/wire/reasoning/cancel/model), `context.rs` (DCP/compaction/projection/reopen) и `tool_lifecycle.rs` (admission/permission/tool/child/MCP effect lifecycle). Existing `fixtures/approval_lifecycle.rs` остаётся одним модулем того же target, не компилируется повторно через каждую часть. По фактической связанности R0 можно переназвать группы; не создавать ещё suites для уже существующих mcp/subagent integration targets.

`oc/tests/pty_t39.rs` остаётся harness/shared Fixture/PTY helpers/constants; сценарии → `tests/pty_t39/interaction.rs` (input/panels/clicks/transcript) и `lifecycle.rs` (startup/quit/restore/async jobs/bounds). Existing `tests/support/terminal.rs` и `title.rs` подключены один раз. Helpers не копировать между частями, фактическую видимость в test tree обеспечить без production exports. Если standalone helper уже самостоятельный, маленький размер не повод сливать его обратно.

Пример root integration module declarations, **не новый Cargo target**:

```rust
// crates/oc-adapters/tests/runtime.rs; здесь остаются общие imports/fixtures.
#[path = "runtime/turns.rs"]
mod turns;
#[path = "runtime/context.rs"]
mod context;
#[path = "runtime/tool_lifecycle.rs"]
mod tool_lifecycle;
```

Вложенные parts используют ближайший fixture (например `use super::*` внутри tests). Не создавать `tests/runtime/main.rs` при сохранённом `tests/runtime.rs`, top-level helper `.rs` как пустой test target или `autotests=false` с новым registry. Физическое дерево изменилось, Cargo identities `runtime`/`pty_t39` и число target процессов не должны измениться.

**Проверки:** before/after list и ignored mapping для этих targets, `cargo test -p oc-adapters --locked --test runtime`, затем `cargo test -p oc --locked --test pty_t39 -- --test-threads=1`. Последний extra serialization ограничивает этот PTY запуск, не меняет assertions/deadlines и не отменяет общую resource policy. Проверить, что fixture server стартует/закрывается с прежним lifecycle, нет duplicated tests или потерянных tests из-за неподключённого module.

### R6 — маршрут чтения и живые path consumers

К этому моменту `CODE_MAP.md` должен показывать реальные owner paths, а не планируемые. В каждом переехавшем root достаточно короткого module doc: что он владеет, где implementation parts и tests; не вставлять весь план в каждый файл. Не добавлять AGENTS.md в каждую подпапку и не создавать индекс каждого метода/100 task карточек.

Проанализировать текущие потребители moved file/symbol paths в `crates`, `scripts` и active docs. Конкретные места, обнаруженные на base: `scripts/tui_capture/dcp_display_fixture.py` (runtime source ranges), `tabs_audit.py` (список native sources), `summarize_prompt_paste.mjs` (старый source inventory). Обновить живые paths/anchors там, где они используются после T52; не переписывать исторические reports, logs и capture manifests, чтобы они выглядели созданными новым кодом. Для новых T52 evidence использовать Git base/current commits и path-scoped diffs; не расширять старые checksum inventories в новую систему проверки раскладки.

Проверить `include_str!`/`include_bytes!`/`#[path]`, используемые `file!`/`module_path!` при наличии и literal test filters в scripts/активных contracts. Не заменять blindly все упоминания старого файла в repository: historical references с Git commit корректно указывают на старое дерево. Лишённый executable consumers prose archive не повод читать/менять весь evidence.

**Контекстный smoke:** по карте провести read-only разведку двух обычных следующих задач — таб hover/animation и runtime DCP projection. Зафиксировать достаточный read-set и ближайшие tests. Они должны находиться без загрузки 17k app/tests monolith, но все реальные зависимости разрешено читать. Это qualitative навигационная проверка, не обещание фиксированного token savings. Записать каждый остаточный >5k файл с причиной/следующим швом; отсутствие строки исключения не заменять молчаливым игнорированием warning.

### R7 — qualification и closeout

Полный gate выполняется на реальном host после всех moves. Придерживаться `AGENT_RUNBOOK.md`: default `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, один Cargo процесс за раз, owned disk TMPDIR, resource checks; не делать `cargo clean`/toolchain upgrades и не покупать paid runs ради файлового распила.

```sh
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo build --workspace --release --locked
python3 scripts/test_code_size.py
python3 scripts/test_progress.py
python3 scripts/test_check_docs.py
python3 scripts/check_docs.py
python3 scripts/progress.py check
git diff --check
```

`test_code_size.py` в R0 сделать непосредственно runnable, как existing unittest scripts. Дополнительно существующие frontend/syntax/capture checks для реально затронутых producers и mandatory checks активного контракта; команды/arguments брать из их текущего README/spec, не выдумывать universal capture command. Базовые dependency/prerequisite failures записывать честно. Fresh paid/live campaign не требуется только из-за смены файлов; если действительно затронут wire/runtime контракт, применяются его существующие requirements/live envelope, а не произвольный новый тест.

Повторить advisory report с base до moves; сохранить фактические file counts/LOC/bytes и изменившийся test inventory с one-to-one mapping. Проверить targeted filters на ненулевой ожидаемый набор. Compile/test successes не заменяют review ownership/visibility/changed effect order; одинаковое количество tests не заменяет сверку identities.

Для UI regression использовать существующие bounded deterministic scenarios и, когда требуется их T44/VIS контрактом, whole-frame styled cells + cursor + PNG, включая релевантные 80x24/120x40/160x48 состояния. Исторические три DCP frames из ddb6ae2f0 не означают полный VIS38/T44 PASS. Missing raw captures в архиве отмечать отдельно; не генерировать заново 10 GiB run history, не удалять чужое evidence и не подменять partial checks полной квалификацией. Observed различие с прежним expected расследовать, не auto-update golden/masks/timeouts/thresholds.

**Git evidence, без новых SHA256:**

```sh
git rev-parse HEAD
git status --short
git diff --stat <base-commit>
git diff --find-renames <base-commit> -- <affected-paths>
git diff --color-moved=plain <base-commit> -- <one-owner-paths>
git diff --cached --check
```

Последние большие diffs просматривать по одному owner и сохранять в bounded files; в prompt не выводить весь move одновременно. Rename/moved-block detection — помощь review, не доказательство behavioral equivalence. Не создавать tags, checksum files, копии всего дерева или rollback scripts; Git commits и narrow diff достаточны для provenance. Rollback при необходимости — отдельный reviewed revert своих commits, не reset чужого worktree.

`evidence/T52/report.md` содержит фактические Result/Checks/Risks/Next, исходный и implementation Git commits, выполненные R-срезы, before/after table, list/ignored mapping summary, APIs/ownership review, commands/exits, ARCH01–ARCH05 с PASS/FAIL/NOT_RUN и remaining warnings. Уложить report в существующий лимит 16 384 UTF-8 bytes; большие списки/логи отдельно, без обязательного чтения при каждом resume. `finish` utility проверяет структуру, а не истинность утверждений.

Closeout: implementation commit уже существует → report + factual note → `python3 scripts/progress.py finish --note <note> --evidence evidence/T52/report.md` → review/closeout commit → обычный разрешённый push. Затем, при разрешённом продолжении и без настоящего blocker, `start T44`; продолжить прежний незавершённый feature slice. Нельзя маркировать T44 done, менять её frozen acceptance или считать product READY по результату этого рефакторинга.

## Критерий результата

Рабочий код и тесты действительно разложены, карта соответствует compile-able дереву, существующие paths/API/targets/invariants и тесты сохранены, mandatory checks фактически квалифицированы. Все восемь исходных превышений адресованы; безопасная содержательная причина остаточного >5k оформлена явно с next seam/trigger, а не спрятана за soft limit. Цель — десятки укрупнённых частей в выбранных owners (ориентировочно 20–30 новых source/test/dev-tool файлов за весь проход), не сотни мелких файлов. Это оценка, не KPI на количество файлов.

Следующий агент по нужной обязанности читает несколько связных owners/tests, может делегировать разведку и сохраняет место в окне 400k для реализации, проверок и handoff. Ни max LOC, ни число прочитанных файлов, ни «контекст меньше» сами по себе не доказывают корректность. Решение оценивается по достаточной локальности чтения при сохранённых поведении и ownership.
