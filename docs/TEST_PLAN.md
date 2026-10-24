# Проверки разработки

Выбирай проверку по изменённому поведению, не по task ID. Owner/tests — в [CODE_MAP](CODE_MAP.md), открытые qualifications — в [GOAL](../GOAL.md).
Обычный bugfix не требует registry, report, checkpoint или closeout. Offline PASS не является live/visual qualification.

## Быстрый цикл

1. Воспроизведение/регрессия у owner → минимальная правка → targeted test → review diff.
2. Запусти проверки затронутого crate. Общий storage/runtime/wire/lifecycle boundary или final integration требует full offline gate.
3. При docs-only изменении достаточно scope/link review и `git diff --check`; examples/source baselines дополнительно проверяются `check_docs.py`.
4. В ответе укажи команды/исходы и существенное непроверенное. Не создавай отдельный implementation/closeout commit protocol.

```sh
cargo test -p oc-adapters --lib --locked <filter>
cargo test -p oc-tui --lib --locked <filter>
cargo test -p oc2 --test <target> --locked <filter>
git diff --check
```

## Полный offline gate

```sh
bash scripts/test.sh
```

- Один Cargo owner: fmt → strict workspace/all-target Clippy → workspace nextest → Cargo doctests → debug build → help/version. Не запускать параллельные Cargo-команды с одним target.
- `cargo-nextest` **0.9.148** — dev prerequisite, не runtime dependency. Gate сам его не устанавливает; принимает установленный binary или gitignored `.local/tools/cargo-nextest`.
- Разрешённая non-root установка prerequisite: `cargo binstall cargo-nextest --version 0.9.148 --secure`. Не делать auto-install, global package changes, `cargo clean`, удаление `target/debug` или override `CARGO_TARGET_DIR`.
- `.config/nextest.toml`: 10 slots на текущем 16-core host, serial PTY/archive groups и exclusive timing/I/O cases. Нет retries/default-filter, скрывающего tests. На меньшем host можно снизить `NEXTEST_TEST_THREADS`, не deadlines или нагрузку.
- Не заменяй scheduler общим `RUST_TEST_THREADS=1/16`. `dev`/`test` используют `opt-level=1`, сохраняя assertions, overflow checks и FULL durability.
- Gate передаёт реальные Cargo/Rustup homes fixtures с isolated HOME, не расширяя production environment allowlist. Его XDG isolation не заменяет real-user E2E06.
- Live cases остаются явным opt-in; full offline gate не запускает реальные auth/generation/browser campaigns и paired visual capture.

## Examples и source baselines

```sh
python3 scripts/check_docs.py
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo test -p oc-adapters --lib --locked config::admission
```

`check_docs.py` читает syntax examples и source pins/license/discovery snapshot, не task tree или user config и не пишет state/lock.
Rust admission tests проверяют настоящий `examples/opencode.jsonc`, safe disabled-browser/no-OAuth/dynamic-model defaults через native parser, не regex по Rust source.
При работе с capture/comparator: `python3 tui-recovery/scripts/test_compare_frames.py`.
При наличии Node в dev-среде: `node --test scripts/test_discovery_reference.mjs` — offline oracle, не production зависимость.

## Ресурсы и время

- До тяжёлого build/soak: non-root, текущий worktree/HEAD/diff, один mutation/Cargo owner, **MemAvailable ≥2 GiB**, worktree free disk **≥10 GiB**. Это safety defaults, не гарантия peak memory.
- TMPDIR — заранее созданная owned directory на disk, не `/tmp` tmpfs. Для этого worktree разрешён `/home/stfu/.cache/opencode-tmp/opencode`; не менять `CARGO_TARGET_DIR`.
- Case >**30s** (setup/teardown включены; compilation и ожидание nextest slot исключены) либо warmed full gate >**10min** требует найти конкретный case/дорогую фазу и исправить причину. Не перезапускай неизменённый suite вслепую.
- Единственное owner-approved исключение: `provider_readiness::ui07_native_discovery_deadline_is_local_and_explicit_known_model_works`, target `pty_t39`, **весь case ≤40s**. Он проверяет production discovery deadline **30000ms** и последующий repair/known-model request; сокращать deadline нельзя.
- Не скрывай slow/failing tests timeout/retry/ignore, более слабым assertion, уменьшением контрактной нагрузки, отключением durability/security guards или поднятием baseline. Искусственное ожидание заменяй readiness; реальные измерительные окна сохраняй. Новые исключения агент сам не назначает.
- Raw log ≤**16 MiB** каждый, собственный суммарный `.local/evidence` ≤**1 GiB**; truncation отмечать. Не печатай секреты и не удаляй чужие данные. Новые run artifacts — gitignored per-run storage, не обязательный task report.
- Long-session perf probe (opt-in, вне gate): `OC_PERF_DATA=target/perf-probe/long OC_PERF_TURNS=600 cargo test -p oc2 --test perf_long_session -- --ignored --nocapture generate` строит одну сессию через настоящий owner и scripted Responses peer (read/grep/apply_patch/shell); `... measure` открывает её и печатает latency страницы owner и кадров скролла TUI (p50/p95/max, кадры >6.06ms, подгрузка старых страниц). `OC_PERF_RESUME=1 OC_PERF_FIRST_TURN=<n>` продолжает существующую сессию. Data root — gitignored `target/perf-probe/`.
- Retained memory: gate `memory_soak aud_owner_and_tui_release_turn_peaks` (реальный бинарь: TUI под PTY держит owner, `oc2 run` гоняет tool-ходы; owner anon+swap ≤48 MiB после 8 ходов, idle TUI делает heap release). Opt-in `... --ignored --nocapture probe` печатает owner/TUI anon/swap/HWM по ходам (`OC_SOAK_TURNS`, `OC_SOAK_SESSIONS`); `OC_SOAK_INTROSPECT=<dir>` снимает `malloc_info` и trim через gdb, `OC_SOAK_HEAPTRACK=<dir>` (+`OC_SOAK_HEAPTRACK_TUI=1`) — heaptrack пиков.
- `scripts/archive_failed_capture.py` — существующая bounded lossless transaction только для собственных untracked/unstaged FAILED diagnostics: full-byte/path/mode/UID/GID/mtime audit и inverse verification. Qualified proofs/старые seals не переписывать. При interruption сначала исследуй quarantine/transaction, не replay.

## Live и внешние effects

- Только конкретный разрешённый bounded product test в искусственном fixture repo. Не использовать пользовательский workspace как coding fixture, не запускать массовый benchmark/все модели.
- `.local/live.env` разрешён для product `LUDKA2_API_URL`, `LUDKA2_API_KEY`, `OC_TEST_MODEL`; значения не печатать/коммитить. Product credentials и модель теста не настраивают authoring-agent; его auth/config не читать.
- До network dial сохраняется durable reservation в существующей campaign identity/append-only ledger (flock/fsync). Restart не обнуляет counts; unknown/reserved считается spent. Child/title/summary/retry/probe/fallback входят в quota.
- Envelope: generation **24**, control **128**, MCP search **4** на campaign; request **1 MiB**, aggregate input **8 MiB**, response **8 MiB**, subprocess **300s**, whole campaign **900s**, stdout/stderr **16 MiB** каждый.
- Фактический `scripts/bounded_live.py` guard — **max_output_tokens ≤2048 на generation**, включая coding. Старые тексты с 8192 не разрешают превысить этот cap.
- IDs/последние документированные остатки — [GOAL](../GOAL.md#live-authorization-и-сохранённые-бюджеты). OpenProxy generation24/24 exhausted; Go allowance не переносится; нулевой OpenAI receipt не даёт operator permission. Недоступный ledger/confirmation — blocker live, не причина создавать новый ID/reset/refund.
- Не увеличивать envelope, не покупать/сбрасывать quota и не извлекать чужие ключи. Rate limit и unknown external effect исследовать без слепого повторения. Finite retries, captured authority, quarantine и cleanup остаются.
- Native owner пользователя не kill/restart, lock не удалять, интерактивный клиент автоматически не запускать. Rebuilt ELF подхватывается после завершения его leases; startup env принадлежит первому owner.
- Docker только если нужен: подтверждённый rootless context, без privileged/host PID/network/broad mounts, общего prune/compose down и OpenProxy deployment changes. Обычный push — verified `origin` своей ветки, без force/release/tag.

## Открытые live/manual qualifications

- **Post-compression new process:** seeded coding → tests → reopen/web/search → compress → чистый restart → retained fact/reduced context/next edit. Проверять effects/history, не точную LLM-прозу. Историческое coding/reopen без нового успешного процесса после compression не закрывает live facet.
- **Configured OpenProxy child:** одна реальная delegation и durable child result/reopen на исходном authority, не fake/root-only/Go замена. Exhausted campaign не запускать заново; contract — [R3](goals/2026-09-21-config-compat-and-subagents.md).
- **AUTH06:** реальные browser OAuth и headless device OAuth с подтверждением пользователя, ordinary OpenAI Key login/request/tool roundtrip/reopen. Synthetic issuer/offline auth tests не заменяют operator authorization/live checks; [контракт](goals/2026-10-06-codex-oauth-and-openai-key.md), VIS45 отдельно.
- **MCP/retry:** [configured-label и host-shaped MCP follow-ups](goals/2026-09-22-mcp-attach-parity.md), [late retry/span и safe details](goals/2026-09-29-provider-retry-parity.md). OC2-only allowlist/permission ceilings, finite retry/no replay, admitted redaction/clipping и recorded span ownership не ослабляются.
- **Cross-user:** real root/stfu pair только owner-operated; boundary fixtures не являются этой manual qualification и не дают агенту root/чужие credentials.

## Hot reload

Оставшийся [scope](goals/2026-10-14-hot-reload-shared-service.md) проверяется composed owner/runtime/MCP и actual-binary сценариями, не helper-only watcher test.

- Атомарная validation/publication, last-good при fatal candidate, immutable issued request/full batch/approval и fresh-request adoption той же задачи; RAW/HOT/no replay сохраняются.
- Один debounced watch/manual coordinator и persistent MCP owner: unchanged reuse, changed retirement по issued leases, fencing late completion, sticky quarantine и подтверждённый cleanup без generation-owned chains.
- Client settings: theme/binding subset, palette propagation, captured inline client env и отсутствие draft/selection/live-state reset или provider/MCP rebuild.
- HOT01/HOT02/HOT04 ещё не квалифицированы целиком; HOT03 descoped и не возвращается shared owner. Отложенная работа не стартует автоматически.

## E2E06 — real-user startup

Current-source qualification после config изменений требует **все четыре профиля**, не только `--help`, fresh store или старый PASS.

1. **Deterministic fixture:** current supported global/Location/.opencode shapes, unsupported plugin, disabled MCP, discovery401 и saved unavailable agent/model/variant в active/parked tabs. Synthetic secrets/DB; typed diagnostics, responsive Home/history/selectors, explicit unavailable identity/cause. Invalid submit/headless даёт pre-effect refusal и ноль generation/tools; нет default/sibling fallback, prefs/deck reset или wider policy. Malformed policy/trust/storage/recovery/cleanup/caps остаются fatal.
2. **Real existing store:** rebuilt `target/release/oc2`, bare launch под bounded PTY/strace текущего non-root пользователя, текущие Location/HOME/XDG/PATH и неизменённые config sources/default native store. Не env_clear, не temp HOME/XDG, не fresh data-dir и не изменение user prefs ради green. Home/history/diagnostics/selectors до prompt, без generation в пользовательском workspace.
3. **Real fresh store:** тот же release binary, пользователь/Location/реальный config environment, но fresh isolated `--data-dir`. Это coexistence, не TS DB import/migration; не заменяет existing-store profile.
4. **Explicit repair/recovery:** на controlled fake-service fixture выбрать valid primary agent/model/variant, проверить captured effects, restart/reopen, сохранённый выбор/history и отсутствие replay. Без paid request в настоящем workspace.

- Preflight: binary revision, identity, terminal/profile, config-root resolution и store ownership без секретов. Watchdog/output bounds соблюдают настоящие discovery/MCP deadlines. Конкурирующий live owner не разрешает kill/lock removal/reset или подмену только fresh profile.
- Strace только запущенного процесса и его descendants: opens/locks/connect/exec/wait/exit/cleanup metadata, **без read/write/send/recv payloads и env dumps**. `execve/execveat` arguments raw (например `-e raw=execve,execveat`), чтобы не записать argv/env contents; redact чувствительные paths/addresses.
- Typed inventory + fixture effect counters доказывают zero spawn/probe для disabled MCP и zero resolve/import/execute/network для unsupported plugin. Admitted enabled external MCP учитывать отдельно; это не forbidden JS host. Discovery failure видим, не app-fatal.
- Проверять clean owned task/process shutdown и terminal restoration. Реальные config/TS DB не редактировать; нормальные own native lock/WAL/recovery writes ожидаемы. Raw user logs/config/DB/headers не попадут в Git.
- Отдельно сообщать fixture/existing/fresh/recovery results и source revision/commands/exits/trace findings; не создавать обязательный report-файл. Missing profile не PASS. `scripts/real_user_startup.py` содержит historical fixed paths/hashes и не является готовым универсальным smoke текущего checkout.

## TUI / paired qualification

- Полный контракт — [TUI reference](goals/2026-09-21-tui-pixel-parity.md), [amendments](../tui-recovery/T44_CONTRACT_AMENDMENT.md), [verification](../tui-recovery/VERIFICATION.md), [acceptance](../tui-recovery/ACCEPTANCE.json).
- Нужны сопоставимые pinned-original/native actual-binary captures: полный styled-cell grid, PNG, cursor/attributes/geometry/focus/selection, temporal поведение и effect counters. Собственный golden, OCR или snapshot helper не равны paired PASS; не маскировать differing regions и не переписывать старые seals.
- Idle Esc сохраняет draft/paste/selection/focus, не exit/submission; raw-Esc PTY плюс paired qualification. Busy/modal/autocomplete/permission/question/cancel priority сохраняется; точный [outcome](../GOAL.md#tui-parity--отложено-не-отменено).
- Blink/phantom caret, child/Shell/Terminals, question/approval, retry/details, model switch и execute cards проверяются по соответствующему amendment. Native PTY pane не подменять shell output; syntax colors не подменять ANSI; history не replay tool effects.
- Capture tools остаются dev-only, fixtures/comparator/licenses — на прежних путях. Результаты — bounded gitignored per-run artifacts; не обязательная новая task/campaign. Full TUI qualification отложена до запроса пользователя.
