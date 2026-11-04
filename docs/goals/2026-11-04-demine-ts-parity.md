# Goal: W0, W6–W10. Разминирование oc2 Rust и паритет среза фич с OC2 TS

Status: active
Source: user instruction 2026-11-04 — «какие ещё мины остались… размининовать весь oc2 rust и получить работающий продукт, который паритетен oc2 ts в плане части фич»; «делай копию плана в цель и начинай итеративную реализацию и коммит пуш, ответы на вопросы берём рекомендованные варианты»; план v2 после Review-Sol.
Last updated: 2026-11-04
Baseline: HEAD `cfe5684032fbee7754d537a82bafd9e9a19e5dcf`, untracked чужой `progress/`.
TS reference: 2.2.5, `.local/opencode/packages/{core,cli,tui,schema,ai}/src`.

## Objective
Все пункты волн ниже либо воспроизведены (RED у владельца или дифференциальный прогон TS↔Rust) и исправлены до паритета с TS с GREEN-тестом, либо закрыты `not_applicable` с доказательством. Весь `bash scripts/test.sh` проходит целиком. Документация не противоречит коду. Это паритет перечисленного среза, не полный READY продукта.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R0: Волна W0 выполнена: стенд не скрывает сбои, таблица сценариев TS↔Rust заполнена, причины нестабильности гейта диагностированы.
  - Acceptance: таблица в Current State; каждый сценарий имеет наблюдаемый результат; непроверенный случай не считается `not_applicable`.
  - Status: verified — W0.1/W0.2 и bounded W0.3 checkpoint выполнены; недоказанный sidebar failure сохранён для обязательного возврата перед R12 согласно owner execution directive.
- R6: Каждый пункт W6: RED→GREEN или `not_applicable`; коммит и пуш волны. Status: pending
- R7: Каждый пункт W7: RED→GREEN или `not_applicable`; коммит и пуш волны. Status: pending
- R8: Каждый пункт W8: RED→GREEN или `not_applicable`; коммит и пуш волны. Status: pending
- R9: Каждый пункт W9: RED→GREEN или `not_applicable`; коммит и пуш волны. Status: pending
- R10: Каждый пункт W10: RED→GREEN или `not_applicable`; коммит и пуш волны. Status: pending
- R11: Документы (SECURITY, CONFIG, TOOLS_MCP, DECISIONS, CONTRACTS, ARCHITECTURE, CODE_MAP, TEST_PLAN, AGENTS, GOAL, builtin `customize-opencode` SKILL.md — какие описывают затронутое поведение) не противоречат коду; исторические цели и evidence не переписываются. Status: pending
- R12: Финальные проверки: `bash scripts/test.sh` целиком PASS без «qualified acceptance»; `cargo build -p oc2 --bin oc2 --locked --release`; Review-Sol PASS на итоговый diff каждой волны. Status: pending

Для каждой волны: затронутые срезы cargo test + scoped Clippy `-D warnings`, Review-Sol, коммит, `git push origin master` (без force).

### Constraints
- C1: AGENTS.md: `CARGO_BUILD_JOBS=4`, `-- --test-threads=4` / `NEXTEST_TEST_THREADS=4`, без параллельных cargo, без `cargo clean`, без `CARGO_TARGET_DIR`; логи в `.local/tmp`; тестовые проекты вне worktree, `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`.
- C2: Без миграции схемы хранилища, если не доказана необходимость (тогда явно отметить). Новые зависимости — только при доказанной необходимости, запись в Material Decisions.
- C3: Не трогать `progress/`. Сохранить запрет replay неизвестных эффектов, единственный owner/UDS, отказ при плохом сертификате, approved-preimage refusal, исключение своего data root, deny под selected/child/Plan.
- C4: Не запускать интерактивный клиент пользователя, не убивать и не перезапускать owner, не трогать его lock. Live к реальным провайдерам не запускать: нужен отдельный разрешение владельца (OpenProxy 24/24, Go осталось 11). Только synthetic loopback peer.
- C5: Не зашивать model IDs и vendor routes.
- C6: Коммиты по формату AGENTS.md: `<type>(<scope>): <описание>`, английский императив, ≤72; для нетривиальных — `Changes:` и 2–4 пункта с отступом 4 пробела.

### Non-goals
- Снапшоты файлов и git-снимки по шагам — не делаем совсем (решение владельца 2026-11-04).
- Replay/auto-resume ходов после смерти owner. JS/npm plugin host. Public API/serve/attach, web/desktop, auto-update, RAG. Новый gate по build-ID.
- Отложено на следующую партию: внешний редактор, `/export`, PDF read, нативный websearch; протоколы Google/Bedrock/Azure и др., OAuth Copilot, OAuth remote MCP, темы, `todowrite`, стоимость, URL для skills.
- Не пробелы TS 2.2.5: LSP runtime, share, `task`/`list`/`batch`. `/review` в Rust уже есть.

## Change Envelope
- Target: поведение перечисленных пунктов у их владельцев.
- Paths: `crates/oc-adapters/src/**`, `crates/oc-core/src/{question,session}.rs`, `crates/oc-tui/src/**` (ввод, команды), `crates/oc/src/{cli,headless,clipboard}.rs` и их тесты; `docs/**`, `GOAL.md`, `AGENTS.md` (по R11), builtin SKILL.md; стенд вне worktree `~/.cache/opencode-tmp/opencode/mcp-repro`.
- Forbidden: изменение assertions/deadlines ради зелёного, отключение тестов и lint, новые сервисы и фреймворки.

## Material Decisions
- 2026-11-04, owner: повторы без лимита остаются для typed transient; `UnknownProvider` остаётся с лимитом 10; чинится только `network_error`.
- 2026-11-04, owner: legacy-конфиг мигрируем ограниченным набором (W9.1); исключение в `GOAL.md` сужается явно.
- 2026-11-04, owner: W10 — пункты 1–4 и `/init`.
- 2026-11-04, owner: `snapshots:true` — предупреждение «не поддерживается»; снапшоты не реализуются.
- 2026-11-04, owner: принятые риски паритета: 7.4 «Always» на edit = `*`; 7.5 Deny не действует на `!`-команды пользователя.
- 2026-11-04, owner execution directive: W0.3 — «если после разумной попытки причина не найдена, запиши evidence и иди дальше по волнам, вернувшись к этому перед R12»; bounded W0 checkpoint не заменяет целый PASS R12.
- 2026-11-04, execution evidence, без изменения решений owner: `.local/opencode` — source oracle 2.2.5 (`ff90ed91607cabf420a369e9009539c1aa0932af`), без `node_modules`; прямой `bun packages/cli/src/index.ts --version` падает на отсутствующем `@opencode/plugin/runtime`. W0 запускает установленный `/home/opencode/.opencode/bin/opencode`, проверенный как `opencode v2.2.5`, не выдавая его за locally rebuilt TS. Новые npm/runtime dependencies не добавлены.
- 2026-11-04, W0 safety incident: первый, ещё не network-confined `w9-policy` с Deny только `fixture` вызвал встроенный TS fallback; получен внешний `provider.free-limit` / HTTP 429. Запрос не был разрешён, не повторялся наружу; влияние на внешний budget не установлено, ledger не читался/не сбрасывался. Все последующие запуски переведены в установленный rootless `bwrap` с отдельными user/PID/net namespaces (UID сохранён), единственным `lo`, без внешних маршрутов, скрытым настоящим `/home` и private disk-backed `/tmp` для UDS. Runner отказывает при сбое изоляции. В окончательной policy-фикстуре Deny `*` даёт TS `provider.no-route`, ноль запросов peer; исходный инцидент не замещён этим результатом.

## Wave plan (frozen, reviewed plan v2)
Порядок в каждой волне: RED у владельца → фикс → GREEN или `not_applicable` → документация → коммит и пуш.

### W0. Проверочная база
1. Стенд `~/.cache/opencode-tmp/opencode/mcp-repro`: `scripts/run.sh:39-44` и `both.sh:7` показывают падение клиента, `peer.py:95-105` проверяет успех. Нового фреймворка нет.
2. ~20 сценариев TS↔Rust из пунктов волн; в каждом ожидаемое наблюдение: код выхода, эффект на диске, содержимое следующего запроса. Форматтер — проверка на диске. Для admission-only случаев браузер и контейнер не нужны.
3. Ограниченная по времени диагностика гейта: в W3–W5 `scripts/test.sh` целиком не проходил (auth05, sidebar fold/restart, control-card replay exit, vis25 reload wait падают только в полном прогоне). Цель — один полностью зелёный прогон; лечить причину, не ослаблять тесты.

### W6. Живучесть хода
- 6.1a `runtime/turn.rs:694` `units_have_calls` учитывает settled `Assembled::Failed` (единственный oversized/non-object вызов сейчас завершает ход на `:4349`; TS `session/runner/publish-llm-event.ts:597-600`).
- 6.1b Невалидный JSON аргументов (`provider.rs:1282,1300,1828-1850`, `provider/chat.rs:506-512`) становится failed tool result без исполнения префикса; ход продолжается.
- 6.1c Обрезанный при `finish=length` вызов (`provider.rs:1449-1472,1538-1544`, `chat.rs:506-512`) становится failed input с причиной length, без исполнения, затем продолжение с лимитом двух length-continuations как в TS `llm.ts:339-368`, `step.ts:291`. Только для завершённой генерации; оборванный stream остаётся `Err`.
- 6.2 `provider/chat.rs:464-475`: ветка `"network_error"` даёт `ProviderInternal` (TS `ai/src/protocols/openai-chat.ts:936-942`). RED: 12 раз `network_error`, затем успех, по образцу `tests/runtime/turns.rs:6`. Политика `runtime/retry.rs` не меняется.
- 6.3 `storage_children.rs:293,393`: `unknown` исключается из обоих счётчиков лимита; `child_job_unresolved` (`:289`) и реальные semaphore permits сохраняются; без replay.
- 6.4 `composition/watch_inputs/observer.rs:268-281`, `application.rs:4655`: сбой watcher даёт warning и сохраняет admitted snapshot, без смерти worker.
- 6.5 `storage.rs:674-682,733`: более новая версия схемы даёт отказ до любой мутации, с диагностикой.
- 6.6 Только диагностика: понятное сообщение `ProtocolMismatch` (`native/client.rs:242-244`, `native.rs:32-34`), причина сбоя `Endpoint::bind` через bootstrap (`native/server.rs:277-282`, `native/client.rs:192-198`), не-секретное уведомление об окружении первого owner.

### W7. Shell и permissions
- 7.1 Сфокусированный Rust-порт ветки `core/src/shell/scan.ts` и `parse.ts` без PowerShell, с диалектами bash/zsh/posix, в `oc-adapters`, без tree-sitter. `permission_resources("shell")` (`tools.rs:187`) возвращает команды по отдельности. Opaque — failed invocation до авторизации и spawn. Preview из полной команды (`approval.rs:248-252`). Ноль команд (`cd .`, `export X=1`) сохраняет cwd-подготовку и не паникует (`runtime/turn.rs:1235-1243`). Фикстуры из TS `core/test` shell-тестов. Legacy `bash` argv сохраняет квотирование и отдельный namespace.
- 7.2 Каталоги из `cd`/`pushd` идут через `external_directory` (TS `tool/plugin/shell.ts:115-125`).
- 7.3 `workdir`: лексическая авторизация, затем pin разрешённой цели без проверки вхождения в проект, в `resolve_cwd` и `pin_cwd` (`shell.rs:251-263,539-568`). Абсолютный путь внутри проекта разрешён, снаружи — ask через `external_directory`. Поведение symlink сохраняется; Location и read roots не меняются.
- 7.4 Save patterns отдельно от authorization resources (`approval.rs:133-158,310-332`): shell — arity-префикс из `parse.ts` (`git status *`); edit/write/patch/read — `*`; external — ближайший `.git`/`.hg` цели, иначе каталог цели (`file-access.ts:107-115`). Старые составные exact-гранты работают как fallback только после всех per-command Deny, с `COMMAND_GRANT_PREFIX`; миграция схемы не нужна.
- 7.5 `runtime/user_shell.rs:109-145`: убрать `admit_tool` для команд пользователя, включая Deny (TS `session/shell.ts:10-18`). cwd pinning и supervised execution остаются.
- 7.6 `oc-core/src/question.rs:10-14,37-78`: схема как в TS (`schema/src/question.ts:6-21`, `tool/plugin/question.ts:23-25`), транспортные ограничения остаются.

### W8. Лимиты
- 8.1 `files/read.rs:113-118,141-146` и `tools/read.rs:79-86`: отдельный media-ingest bound 20 MiB, общий read bound не меняется. Чинить учёт retained: base64 считается дважды (`tools/read.rs:48-51`), PNG от ~13 MiB упирается в `runtime/turn.rs:4215,6218-6233`. RED с изображением >12 MiB. Wire-лимит 32 MiB остаётся. Этот пункт дописан после ревью — на него нужно внимание Review-Sol.
- 8.2 `defs.rs:45,47`: 4→64 MiB и 256→4096, конечные; нерелевантные записи фильтруются до подсчёта на обоих путях (`defs.rs:1064,1122,1221-1228`). Ветка отказа `composition.rs:1038-1044` не меняется, иначе теряется deny агента.
- 8.3 Убрать `shell.rs:38` 32 KiB (`E2BIG` → ошибка инструмента) и потолок timeout `tools.rs:68`/`tools/shell_call.rs:63-71`. Поднять `storage_tool_output.rs:9` 16 MiB, сохранив quota и extent-limited чтение. `mcp_result/media.rs:8-11`: медиа-результат до существующего 16 MiB бюджета, лимит 64 части убрать.
- 8.4 Конечные повышения: промпт `oc-core/src/session.rs:152` + `oc-tui/src/app/input.rs:2801-2840` согласованно; fork `storage_fork.rs:10-11`; instructions `defs.rs:49-51`; attachments `attachments.rs:31-33` до 20 MiB; `question.rs` байтовые лимиты; DCP protected `dcp.rs:337`; выделение `oc-tui/src/app.rs:51` + `oc/src/clipboard.rs:30-31`. Перезапись файла с NUL или >8 MiB не требует валидного текста (`patch/mutation.rs:89-99,123-128,155-160`, `patch/fs.rs:204-217`); byte-exact привязка к preimage остаётся.
- 8.5 Только после воспроизведения: `provider.rs:51` 1 MiB аргументов вместе с `patch.rs:31-33` 2 MiB; SSE `provider.rs:49`; запрос `provider.rs:53-55`; `runtime/children.rs:25-26` 8/4; `codemode/interp.rs:314` 50M шагов. Без воспроизведения — закрыть с доказательством, а не повышать.

### W9. Provider и config gates
- 9.1 Миграция подмножества: `provider`→`providers`, `npm`→`package`, `options`→`settings`, per-model overlays, `small_model`, `enabled_providers`, `disabled_providers` (`config/providers.rs:150-190`; TS `config/normalize.ts:137-150,168-176,402-446,627-652`, `v1/config/migrate.ts:236-255`). Нативные поля имеют приоритет. `GOAL.md` исключение сужается явно; остальной OC1 словарь остаётся вне scope.
- 9.2 Поле `env` провайдера как источник credentials (`config.rs:190-192`, `auth.rs:295-313`; TS `config/plugin/provider.ts:38-42`).
- 9.3 `model` как объект `{providerID, model, variant?}` (`composition.rs:744-747`; TS `schema/src/config/model.ts:12-25`). Fallback на доступную модель только для значения по умолчанию, не для явного выбора (TS `model.ts:300-305`; `application_selection.rs:621-632`).
- 9.4 `snapshots:true` (`config.rs:1198-1210`): warning «не поддерживается, игнорируется», без фатальной ошибки.
- 9.5 Распознанные ключи без потребителя (`formatter` до W10, `share`, `username`, `watcher`; `config/admission.rs:20-55,448-463`) дают предупреждение с именем.
- 9.6 Корневые `policies` с `provider.use` через существующий ProviderPolicy owner (`composition/catalog.rs:51-83`, `config/admission.rs:20-55`), приоритет `experimental.policies` сохраняется. Дифференциальный случай с запрещённым провайдером.
- 9.7 Только после воспроизведения: redirects `provider.rs:2555-2569`; keyless localhost `auth.rs:15-23`; DNS any-answer `endpoint.rs:131-157`; auth-заголовки `provider.rs:2628-2688`. Эти пункты защищены `SECURITY.md:9,31`: менять только при доказанном расхождении, и не ослаблять credential binding и защиту от утечки ключа на другой origin. Если фикс требует ослабить — записать как blocker к владельцу.

### W10. Фичи, первая партия
1. `instructions`: пути и glob, затем URL (TS `config/plugin/instruction.ts:41-57,84-175`; Rust `composition.rs:970-1025`, `instructions.rs`).
2. Форматтер при записи файлов с конфигом `formatter` (TS `file-write.ts:60,83-89`, `formatter.ts:56-87`). Проверка на диске.
3. Skills: `.claude`, `.agents`, рекурсивный поиск, локальные пути из `skills` (TS `config/plugin/compatibility.ts:45-55`, `skill-scan.ts:7-43`, `config/plugin/skill.ts:80-104`; Rust `defs.rs`, `composition.rs:919-954`). URL отложены. `docs/CONFIG.md:624` обновить.
4. CLI: `run` с несколькими словами промпта и stdin, `-m -c -f`; корневые `--continue --session --prompt` (`crates/oc/src/cli.rs:14-32,52-67`, `headless.rs`; TS `cli/src/commands/commands.ts:49-69,400-443`, `run/run.ts:80-83`).
5. `/init` (TS `plugin/command.ts:29-40`), как встроенная команда рядом с `/review` (`composition.rs:1364-1373`).

## Current Checkpoint
- Closes: R6
- Smallest next action: owner regression для единственного failed/non-object tool call (W6.1a); baseline `w6-nonobject` уже показывает отсутствие второго request в Rust. Перепроверить `runtime/turn.rs::units_have_calls` и расширить ближайший `tests/runtime/tool_lifecycle.rs` вместо нового harness.
- Expected evidence: W6.1a owner RED→GREEN без исполнения битого вызова; затем последовательно остальные W6.1b–6.6, затронутые срезы/Clippy/review и отдельный коммит/push W6.
- Stop or replan if: расхождение требует ослабить C3, требуется миграция схемы или live; две подряд итерации без движения по R*.

## Current State
- Resolved: R0 — W0.1 (видимые client/pair failures, проверяемый peer verdict, kernel loopback confinement), W0.2 (23 наблюдаемых сценария), W0.3 bounded checkpoint (control-card fixture RED→GREEN; недоказанный sidebar failure не скрыт и сохранён до R12).
- Last relevant evidence: окончательный `scripts/parity.sh 19211` — exit 1, две пары совпадают, 21 показывает ожидаемый baseline gap; это RED-база, не продуктовый PASS. `cargo build -p oc2 --bin oc2 --locked --release` с `CARGO_BUILD_JOBS=4`, approved TMPDIR — PASS (1.15s), `target/release/oc2` соответствует неизменённому Rust baseline.
- Blocker: —
- Review: Review-Sol **PASS** текущего W0 после чтения фактического Rust diff, goal, всех пяти внешних scripts, ignored oracle check и evidence всех 46 окончательных run roots. Блокирующих замечаний нет; это не full gate PASS/закрытие R12.
- Delivery: W0 `398f93431` (`test(parity): Establish an isolated W0 baseline`) закоммичен и **push PASS**: `cfe568403..398f93431 master -> master`, origin проверен как `git@github.com:0FL01/oc.git`. В репозитории доставлены Rust fixture/docs/эта цель; пять scripts остаются на явно разрешённом внешнем стенде, не входят в Git tree.
- Next: W6. Остановка этой сессии — на границе законченного bounded W0 checkpoint; R6–R12 не закрыты. Перед R12 обязательно вернуться к full-only sidebar failure; whole-script PASS пока отсутствует.

### W0 evidence и воспроизведение

Стенд: `/home/opencode/.cache/opencode-tmp/opencode/mcp-repro/scripts/{run.sh,both.sh,peer.py,setup.py,parity.sh}`. Это конечные fixtures и существующий loopback peer, не новый framework. Браузер/Docker не использовались; browser-сценарии не qualified и не считаются `not_applicable`.

Команда таблицы: `bash /home/opencode/.cache/opencode-tmp/opencode/mcp-repro/scripts/parity.sh 19211 > .local/tmp/demine-w0-final-parity.log 2>&1` (exit **1**, не PASS). Каждый напечатанный run-root содержит `client.exit`, `stdout.log`, `stderr.log`, `peer.log` с полным синтетическим request без media bytes и `peer.log.result`, если достигнут проверенный terminal. Media сохраняет MIME/длину; настоящие headers/credentials не логируются. Fixtures/проекты — вне worktree, настоящий HOME скрыт, каждый запуск в новом каталоге. TS колонка — release 2.2.5 с указанной выше оговоркой о provenance; source oracle — `.local/opencode`.

В колонках `exit / requests` указаны exit клиента и число generation-запросов локальному peer (не title); tool-сценарий требует второй request с результатом `c0`. Отсутствие второго request — наблюдаемый сбой, а не `not_applicable`. Диск проверяется в изолированном `project/` соответствующего run-root.

| Сценарий / пункт | TS exit / requests | Rust exit / requests | Наблюдение / ожидаемое исправление в своей волне |
|---|---:|---:|---|
| `w6-nonobject` / 6.1a | 0 / 2 | 0 / 1 | TS отправляет structured failed result для `[]`; Rust заканчивает без следующего request. Effects отсутствуют. |
| `w6-invalid-json` / 6.1b | 0 / 2 | 1 / 1 | TS `tool.input-json`, Rust фатальная генерация; `effects` не создан ни одним клиентом. |
| `w7-shell` / 7.1 control | 0 / 2 | 0 / 2 | Оба возвращают точный `W0_SHELL_MARKER`; PASS control. |
| `w7-multi-deny` / 7.1 | 0 / 2 | 0 / 2 | TS `permission.rejected`, нет файлов; Rust создаёт **оба** `permitted` и `forbidden` несмотря на Deny второй команды. |
| `w7-opaque` / 7.1 | 0 / 2 | 0 / 2 | TS fixture `'' > opaque` (core/test/shell-scan/bash-safety.test.ts:73–81) отказ до spawn; Rust создаёт пустой `opaque`, shell exit 127. |
| `w7-cd` / 7.1 zero commands | 0 / 2 | 0 / 2 | `cd .`: TS `(no output)`, Rust exit 0; оба продолжают, PASS control. |
| `w7-workdir` / 7.3 | 0 / 2 | 0 / 2 | Абсолютный внутренний `project/subdir`: TS печатает cwd, Rust `cwd outside trusted root`. |
| `w8-shell-long` / 8.3 | 0 / 2 | 0 / 2 | 33 000-символьный comment: TS точный marker, Rust invalid command. |
| `w8-shell-timeout` / 8.3 | 0 / 2 | 0 / 2 | Немедленная команда с timeout 600001: TS marker, Rust resource ceiling. |
| `w8-overwrite` / 8.4 | 0 / 2 | 0 / 2 | Исходный `old\0bytes`: TS диск `replacement`; Rust NUL preflight refusal, старые bytes сохранены. |
| `w8-read-image` / 8.1 | 0 / 2 | 0 / 2 | Валидный PNG 14 523 378 bytes: TS image в следующем request; Rust `file_bytes` 1 MiB refusal, image отсутствует. |
| `w9-legacy` / 9.1 | 0 / 1 | 1 / 0 | `provider/npm/options`: TS достигает peer; Rust отказывает до generation. |
| `w9-env` / 9.2 | 0 / 1 | 1 / 0 | TS достигает peer с проверенным synthetic bearer из `FIXTURE_KEY`; Rust без credential отказывает. Header не сохранён. |
| `w9-model-object` / 9.3 | 0 / 1 | 1 / 0 | `{providerID,model}`: TS принимает, Rust отказывает до generation. |
| `w9-snapshots` / 9.4 | 0 / 1 | 1 / 0 | `snapshots:true`: TS принимает, Rust startup fatal вместо warning. |
| `w9-unused` / 9.5 | 0 / 1 | 0 / 1 | Rust принимает `share`, `username`, `watcher`, но stderr не содержит требуемых warnings с именами. |
| `w9-policy` / 9.6 | 1 / 0 | 0 / 1 | Корневой `provider.use Deny *`: TS `provider.no-route` и ноль peer hits; Rust игнорирует policy и генерирует. |
| `w10-instructions` / 10.1 | 0 / 1 | 0 / 1 | Marker из локального пути есть только в TS request. |
| `w10-glob` / 10.1 | 0 / 1 | 0 / 1 | Marker из `instructions/*.md` есть только в TS request. |
| `w10-formatter` / 10.2 | 0 / 2 | 0 / 2 | После write TS диск `LOWERCASE`, Rust `lowercase`; проверка именно bytes файла. |
| `w10-skills` / 10.3 | 0 / 1 | 0 / 1 | Вложенная `.claude/skills/nested/demo`: description marker виден только TS. |
| `w10-skills-path` / 10.3 | 0 / 1 | 0 / 1 | `skills:["extra-skills"]` + вложенная skill: marker виден только TS. |
| `w10-cli` / 10.4 | 0 / 1 | 2 / 0 | `run two words`: TS request содержит `two words`; Rust usage error до generation. |

W0.1 RED: исходный `run.sh` при подменённом `timeout(){ return 17; }` печатал `exit 17`, но возвращал **0**. GREEN на окончательном runner: 17 → runner 17; `both.sh` → 1; mock client exit 0 без peer verdict → runner 1. `python3 -I .local/tmp/demine_w0_peer_check.py` — PASS для неправильного call ID, error с success-marker и ложного env-result. `bash -n .../{run,both,parity}.sh` и `python3 -I -m py_compile .../{setup,peer}.py` — PASS.

W0.3: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 bash scripts/test.sh > .local/tmp/demine-w0-gate.log 2>&1` — **FAIL, exit 100**. Fmt и workspace/all-target Clippy `-D warnings` PASS. Nextest: 2650 run, **2648 PASS / 2 FAIL / 25 skipped**, test phase 270.093s; холодная codegen-сборка предшествовала тестам. Упали `mcp_application` sidebar fold (`missing "MCP (1 active)"`, 12.554s) и control-card replay (`missing "tool output: CSI\\u{1B}[2J"`, 12.574s). `auth05` CLI/TUI (1.775s/17.009s) и `vis25_reload_rebuilds_real_config_on_home_and_session_without_losing_identity` (1.319s) PASS в этом полном запуске — их прежняя нестабильность не воспроизведена. Doctests, debug build/help/version после nextest скрипт **не запускал** из-за FAIL; это не полный PASS и не R12.

Ограниченная owner-диагностика W0.3:
- До правок `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 .local/tools/cargo-nextest nextest run -p oc2 --test mcp_application --locked -E 'test(=lifecycle::sidebar_mcp_live_status_wheel_fold_and_restart_without_generation) | test(=v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay)'` — PASS 2/2, 1.053s test phase (`.local/tmp/demine-w0-pty-red.log`; название артефакта не означает RED). Более узкий Cargo graph потребовал отдельной 4m15s codegen-сборки; это не зависание теста.
- Control-card full-gate failure был именно при replay: первый `S04 raw VT` уже прошёл, финальный экран оставался list с выбранным `probe__fail`, а не detail. В `open_success_card` ожидался безусловный заголовок до асинхронной загрузки страницы, которая сбрасывает cursor. Добавлено ожидание обеих настоящих строк и выбранного `probe__ping` перед Enter.
- Следующий тот же двухтестовый запуск (`.local/tmp/demine-w0-pty-green.log`) — **FAIL**, sidebar PASS 0.862s, control-card после обоих успешных detail/security checks упал на replay exit (12.612s). Scoped Clippy в цепочке после FAIL **не запускался**. Это новое наблюдение, не скрытый retry.
- Exit-синхронизация также была ложной: `Next line intact` виден за модальной панелью ещё до второго Escape. `close_control_cards` теперь ждёт исчезновения настоящего list-заголовка перед `/quit`; существующие content/control/no-replay/terminal assertions и `TIMEOUT` сохранены.
- Sidebar: полный failure до `/quit`/restart — header остался `▼ MCP`. Старый snapshot для click и инвалидируемый paint receipt — проверяемые гипотезы, **не доказанная причина**; два узких прохода прошли. Runtime/fixture sidebar не изменён. По owner directive диагностика ограничена; вернуться перед R12, не объявлять `not_applicable` и не лечить гипотезу без evidence.
- После исправлений `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 .local/tools/cargo-nextest nextest run -p oc2 --test mcp_application --locked -E 'test(=v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay)'` — **PASS 1/1, 0.828s** (`.local/tmp/demine-w0-control-green.log`). Затем `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 cargo clippy -p oc2 --test mcp_application --locked -- -D warnings` — **PASS** (`.local/tmp/demine-w0-clippy.log`, 48.95s), `... cargo fmt --all -- --check` — **PASS** (`.local/tmp/demine-w0-fmt.log`), `git diff --check` — **PASS**. Полный gate после правок не повторялся: по owner directive до R12 сохраняется честный full-run FAIL.

## Checkpoint History
- 2026-11-04: цель создана, план заморожен.
- 2026-11-04: R0 bounded checkpoint — стенд больше не скрывает failures, 23 paired observations; незапланированный TS fallback egress зафиксирован и закрыт kernel confinement для последующих запусков. W0.3 full gate воспроизвёл два PTY failure, control-card fixture исправлен до targeted GREEN; недоказанный sidebar follow-up перенесён перед R12 по owner directive. Full-run FAIL не закрывает R12.
- 2026-11-04: W0 scoped Review-Sol PASS на фактическую последнюю версию; замечаний, требующих правки, нет. Rust runtime, storage schema и зависимости не изменены; чужой `progress/` не затронут.
- 2026-11-04: W0 `398f93431` push origin/master PASS. Post-push checkpoint обновлён на W6; это только delivery/handoff metadata, не новая правка поведения.

## Completion
- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
