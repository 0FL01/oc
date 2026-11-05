# Goal: W11. Дефекты после разминирования: согласованность границ и gates для project config

Status: complete
Source: user instruction 2026-11-05 — «какая дивергенция осталась… фокус на security мины… и потенциальные дефекты»; «делай копию плана в цель и начинай итеративную реализацию и коммит пуш потом». Принятые рекомендации: волна W11 в предложенном составе, gate для project-declared instructions/formatter, D7/D8 — только формулировки в SECURITY.md.
Baseline: HEAD `a12d3cf664eb9430314deb36e6947f898572ad4c`, tracked tree чист, чужой untracked `progress/`.
TS reference: 2.2.5, `.local/opencode/packages/{core,cli,tui,schema,ai}/src`.

## Objective
Подтверждённые последними волнами дефекты закрыты RED→GREEN: несогласованные границы (skill 1 MiB vs 64 MiB, prompt 40 MiB vs inbox 32 MiB, unbounded retry vs обрезанное тело 429), устаревший model-facing контракт `read`, project-config поверхности (instructions/formatter) получают явный gate, документация перестаёт противоречить коду. Всё проверено срезами, scoped Clippy, Review-Sol и полным offline-гейтом; один коммит и push.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: Skill ≥1 MiB не ломает публикацию: skip только этого skill, warning виден, siblings грузятся. Source: `tools.rs:67,566-576`, `runtime/publication.rs:139-146`; docs `CONFIG.md:149,839-843`.
- R2: Durable inbox принимает полный raw-бюджет prompt: 2×20 MiB изображения проходят, превышение 40 MiB raw даёт внятную capacity-ошибку, text-only budget не меняется. Source: `oc-core/src/prompt.rs:6-8`, `storage_inbox.rs:12,128-138`.
- R3: HTTP 429 с длинным телом сохраняет quota-признак и не уходит в unbounded retry; rate limit остаётся transient. Source: `provider.rs:2938-2969`, `provider/failure.rs:299-324,153-158`, `runtime/retry.rs:37`.
- R4: Model-facing описание `read` соответствует коду: 20 MiB, поддерживаемые wire/модальности, и честно говорит, что projection может заменить крупное изображение. Source: `runtime.rs:613-620`, `prompt.rs:6`, `files/read.rs:113-118`, `attachments/image_budget.rs:15-16,265-287`.
- R5: Project config не расширяет внешние instruction-источники: из project-источника абсолютные/`~/`/`..` пути вне project и любые URL отбрасываются с warning; global config сохраняет текущую возможность. Source: `composition/configured_instructions.rs:43-109`, `composition.rs:2022-2045`; SECURITY.md «external content cannot expand trusted roots».
- R6: Formatter из project config не исполняется: honored только из global config, project-декларация отбрасывается с warning, называющим точный argv; global formatter работает как раньше. Source: `config/formatter.rs:1-90`, `runtime/formatter.rs:73-97`, `composition.rs:399-461`, SECURITY.md W10-абзац.
- R7: Документация согласована: `DECISIONS.md:364-365` (MCP media 1 MiB), `TOOLS_MCP.md:126-132` («No Formatter»), `SECURITY.md` формулировка granular shell deny (D7/D8: гарантия на разобранные команды и каталоги, не на произвольные эффекты shell/redirection/обёртки). Исторические цели не переписываются.
- R8: D9 (MCP parts) воспроизведён замером; при подтверждении — границы расширенных представлений, при отсутствии — `not_applicable` с числами.
- R9: Checks: затронутые cargo-срезы + scoped Clippy `-D warnings`; Review-Sol PASS на фактический diff; `bash scripts/test.sh` целиком PASS; коммит по формату AGENTS.md и `git push origin master` без force.

### Constraints
- C1: AGENTS.md: `CARGO_BUILD_JOBS=4`, `--test-threads=4`/`NEXTEST_TEST_THREADS=4`, cargo последовательно, без `cargo clean`, без удаления `target`, без `CARGO_TARGET_DIR`; логи в `.local/tmp`; `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`.
- C2: Без миграции схемы и новых зависимостей. Не трогать `progress/`.
- C3: Сохранить no-unknown-effect-replay, single owner/UDS, cert refusal, approved-preimage, own-data-root, deny под selected/child/Plan, verbatim tool output.
- C4: TS-прогоны только в loopback-only `bwrap`; без live-провайдеров, без конфига/credentials authoring-agent; не трогать работающий owner/lock.
- C5: D5/D6 — осознанное расхождение с TS по решению владельца (project config не расширяет authority). Записать в `DECISIONS.md`/`SECURITY.md` как deliberate; не «улучшать» дальше.

### Non-goals
- PDF, websearch, дополнительные provider-протоколы, OAuth, remote MCP OAuth, snapshots, LSP, изменение granular shell semantics (D7/D8 — только документация).
- Новые approval-UI потоки для formatter/instructions: gate реализуется детерминированно на границе config origin.
- Возврат 64-part cap для MCP или иных снятых лимитов ради «на всякий случай».

## Change Envelope
- Target: перечисленные владельцы (`tools.rs`/`runtime/publication.rs`, `oc-core/src/prompt.rs`+`storage_inbox.rs`, `provider.rs`/`provider/failure.rs`, `runtime.rs`, `composition/configured_instructions.rs`, `config/formatter.rs`/`runtime/formatter.rs`, docs).
- Allowed: минимальные правки у владельца + regression tests; doc-правки R7.
- R9 blocker envelope: существующий PTY fixture `crates/oc/tests/mcp_application{.rs,/lifecycle.rs}` — исправление чтения partial synchronized frames/устаревших mouse coordinates без изменения assertions/deadlines/нагрузки (см. checkpoint full gate).
- Forbidden: ослабление assertions ради зелёного, отключение тестов, новые зависимости/сервисы.

## Material Decisions
- 2026-11-05, owner (через принятие рекомендаций): W11 = D1–D4 + D10, D9 repro-first, D5/D6 gate, D7/D8 только формулировки.
- 2026-11-05, interpretation: gate для D5/D6 = project config не может добавлять внешние instruction-источники/URL и formatter; global config сохраняет возможность. Владелец может отменить одной строкой; откат фиксируется здесь.

## Current Checkpoint
- Closes: R9
- Smallest next action: None; delivery receipt is the matching HEAD/origin master ref.
- Expected evidence: `.local/tmp/w11-delivery.log` and repository refs.
- Stop or replan if: требуется миграция схемы, новая зависимость, ослабление C3, или gate ломает global-конфиг.

## Current State
- Resolved: R1–R9 verified (delivery recorded by Git refs, not a self-referential commit hash in this file).
- Last relevant evidence: final Review-Sol PASS; full isolated offline gate2717/2717 PASS,26 skipped; `.local/tmp/w11-full-gate-final.log`, delivery receipt `.local/tmp/w11-delivery.log`.
- Blocker: —
- Next: None; no live/release work authorized or required.

## Checkpoint History
- 2026-11-05: цель создана, план заморожен.
- 2026-11-05 R1 verified: discovery применяет тот же 1 MiB body bound и публикует CapacityExceeded; publication не эскалирует пропуск skill до workspace error. RED: startup invalid_config; GREEN: owner 1/1 (8.12s), defs builtin 6/6; Clippy `-p oc-adapters --lib --locked -- -D warnings` PASS. Первый GREEN выявил builtin skills в общем count; fixture теперь считает только свои `slot-*` и явно проверяет missing oversized/retained sibling. Холодная компиляция — не зависание теста.
- 2026-11-05 R2 verified: RED InboxFull на 2×20 MiB + полном text budget; GREEN durable write/read и capacity-ошибка для >40 MiB raw. Payload envelope сохраняет прежние 32 MiB text/command и добавляет только base64/marks allowance; MAX_PENDING_BYTES и prompt text budget неизменны. Inbox 13/13, scoped Clippy PASS; логи `w11-r2-*`.
- 2026-11-05 R3 verified: RED реальный turn планировал retry для 64 KiB quota-body; GREEN terminal Failed, RetryScheduled отсутствует, второй request отсутствует. Сохраняется bounded 16 KiB prefix, quota-маркеры распознаются до голого 429; обычный длинный 429 остаётся transient. Owner 1/1, typed failures 13/13, retry 5/5; scoped Clippy PASS, `w11-r3-*`.
- 2026-11-05 R4 verified: description/schema/error guidance теперь 20 MiB PNG/JPEG/GIF/WebP, image-capable Responses/Chat/Messages и projection notice; сам projection неизменён. Owner-text assertion RED/GREEN и scoped Clippy PASS. Уточнённый filter existing `w3_read_images_keep_current_record_bounded_across_projected_batches`: 1/1 PASS, 30.17s (первый неверный module filter дал 0 tests, не засчитан). Slow-case чтение: прежняя нагрузка 60×~0.75 MiB, шесть sequential batches и bounded durable readback; sleeps/cleanup wait отсутствуют, есть per-round timing instrumentation. Нагрузка/assertions/deadlines не менялись; это не зависание.
- 2026-11-05 R5 verified: RED project external symlink превращал load в TrustRefused; GREEN project absolute/~/../escape/любые URL пропускаются с IgnoredSetting, global captures остаются. Origin — admitted source index, не путь/trusted; local glob не обходит ancestors вне Location, новые read roots не создаются. Existing global data-root и URL/last-good fixtures переведены в global config без ослабления проверок. Configured 3/3 + URL 1/1, scoped Clippy PASS; `w11-r5-*`.
- 2026-11-05 R6 verified: RED project добавляет formatter/отключает global; GREEN ignored override/reset + exact literal multiline argv warning без env/file expansion. Origin allow-set передан native assembler; source JSONC не переписывается (порядок других definitions сохранён), provider-only reassembly не читает formatter substitutions. Owner 2/2, actual mutation/cancel/symlink/readback regressions 4/4 (1.15s), scoped Clippy PASS. Низкоуровневый trusted builder API не менялся.
- 2026-11-05 R7 verified: текущие DECISIONS/TOOLS_MCP/SECURITY согласованы по16 MiB MCP, существующему global formatter, granular shell commands/directories vs redirection/wrappers/`$cmd`; D5/D6 явно deliberate TS divergence. CONFIG и встроенный config manual обновлены по изменённому контракту. `skill customize-opencode` недоступен в authoring skill catalog; прочитан и обновлён repo asset напрямую. Doc assertions RED/GREEN, `python3 scripts/check_docs.py`, `git diff --check`, scoped Clippy PASS. Исторические цели/evidence не менялись.
- 2026-11-05 R8 repro confirmed: свежие isolated Rust test процессы, настоящий Python stdio MCP peer без сети, raw16,199,999/16,200,013 bytes. Baseline VmHWM22,124 KiB; одна крупная image119,112 KiB (post-call101,444);300,000 минимальных image parts527,480 KiB. Логи `w11-r8-red-{baseline,large,many}.log`, exact probe `mcp_result::media::tests::w11_mcp_expansion_rss_probe --ignored --nocapture --test-threads=4`, `W11_MCP_PROBE=...`. Это amplification в native expansions, не новый cap64parts. Минимальное решение: отдельный16 MiB budget для content-structure storage (`count * size_of<ContentBlock>`) перед native Value/projection и на durable restore; encoded16 MiB и65-part/>1 MiB acceptance сохраняются. Не обещает ограничить исходный SDK parse/RSS целиком.
- 2026-11-05 R8 verified: budget regression RED→GREEN у project_rich/from_stored, existing65-part >1MiB verbatim/order/reconstruction PASS. GREEN RSS baseline21,756 KiB; large16,199,999 bytes accepted116,652 KiB; many300,000 parts rejected TooLarge до native expansions,385,584 KiB (остаточный peak — SDK parse, не скрыт как RSS ceiling). Media5/5, RSS3/3, stdio20/20/1 existing ignore, remote29/29/1 existing live ignore, scoped Clippy PASS. `w11-r8-green*.log`, `w11-r8-transports.log`, `w11-r8-clippy.log`.
- 2026-11-05 Review-Sol1: один подтверждённый P2 R4 — Chat tool messages всегда text-only, даже image-capable; первоначальный новый description ошибочно обещал inline Chat images. Исправлены только description/TOOLS_MCP: inline Responses/Messages, Chat notice независимо от размера, отдельный size projection notice. Уточнённая owner source assertion, existing `media_projection_follows_model_vision_and_chat_tool_wire`1/1 и scoped Clippy PASS (`w11-review-r4-*`). Projection/runtime поведение не менялось; повторное review pending.
- 2026-11-05 Review-Sol2 PASS на current code diff (hash960fc4a0a79e6ebc9d53672c6043aa3272cc8a444127b31fa7d57ece48e7eec8), R4 закрыт.
- 2026-11-05 full gate1 FAIL: clean isolated HOME, jobs4/nextest4, fmt/workspace all-target Clippy PASS; nextest2715 PASS/1 FAIL/26 existing skips,297.349s. Failure `oc2::mcp_application lifecycle::sidebar_mcp_live_status_wheel_fold_and_restart_without_generation`,12.583s: missing folded `MCP (1 active)`, screen остаётся expanded после click. Fixture/runtime TUI untouched baseline доказан пустым `w11-sidebar-baseline-diff.log`. Диагностика: fixture screen парсит в том числе незавершённый synchronized frame (production renderer Begin/EndSynchronizedUpdate), а mouse header_y берёт из старого `sidebar_after` до последующего wheel/ack. Это допускает click по transient/stale геометрии. Минимальный approved R9 путь — completed-frame read + fresh coordinate, не retry/timeout/assertion weakening. Лог `w11-full-gate.log`; cold test codegen19m08 (не warmed runtime wait); tests4m57. Existing60-image case37.154s under4-way load: прежний CPU/durable image projection workload, без sleeps; discovery31.059s — документированное≤40s исключение. Gate не засчитан PASS; doctests/final build/help/version ещё не исполнились.
- 2026-11-05 R9 fixture RED→GREEN: новый owner regression показывает, что прежний screen публиковал `new header` до EndSynchronizedUpdate, хотя терминал ещё показывает `old header`. Fixture теперь читает последний completed frame и получает fold coordinate после pinned-wheel ack. Original assertions/deadlines/load сохранены; retry/sleep/production changes не добавлены. `cargo test -p oc2 --test mcp_application --locked -- --test-threads=4`46/46 PASS9.24s, `cargo clippy -p oc2 --test mcp_application --locked -- -D warnings` PASS (`w11-r9-fixture-*`).
- 2026-11-05 R9 acceptance: Sol3 fixture PASS; full isolated gate2 целиком PASS — fmt, workspace/all-target Clippy, nextest2717/2717,26 skips,287.404s; sidebar0.915s; doctests0 examples PASS; debug build4m56, help/version PASS (`w11-full-gate-final.log`). Второй прогон только после доказанного fixture fix. Sol4 final PASS, reviewed tracked diff SHA256c730714dc547d4bee8367f50e97da178fa4b7bcf8c6e84420224f4eb289d35e5; после review — только goal closeout/delivery metadata. Один `fix(runtime)` commit и `git push origin master`, проверка remote ref в `w11-delivery.log`; release не создаётся.

## Completion
- Resolved outcomes: R1–R9 verified; D9 confirmed and bounded, not `not_applicable`. D5/D6 deliberate TS divergence documented, global behavior retained. Sol's R4 Chat-wire correction and mandatory-gate fixture correction both reviewed and checked.
- Commands and artifacts: all Cargo sequential with `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4`; tests `--locked -- --test-threads=4`. Targeted filters/results above, `.local/tmp/w11-r1-*` through `w11-r8-*`, `w11-review-r4-*`, `w11-r9-fixture-*`. Full command: `env -u OPENCODE_CONFIG_DIR -u OPENCODE_CONFIG -u OPENCODE_CONFIG_CONTENT HOME=<fresh approved-TMPDIR directory> CARGO_HOME=/home/opencode/.cargo RUSTUP_HOME=/home/opencode/.rustup RUSTUP_TOOLCHAIN=1.93.0-x86_64-unknown-linux-gnu CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 bash scripts/test.sh`; final logs `w11-full-gate-final{,-environment}.log`. Docs: `python3 scripts/check_docs.py`, `git diff --check` PASS. Binary: `target/debug/oc2`.
- Constraint and diff-scope check: only listed owners/consumers, nearest regressions, current docs/manual and proven R9 PTY fixture; no dependencies, schema migration, changed projection/shell semantics, live/TS calls or owner restart. Foreign `progress/` untouched. No force push/tag/release. MCP initial SDK peak remains outside the expansion byte budget; offline PASS is not whole-product/live qualification.
- Final status: complete; delivery authority is Git HEAD and the matching `origin` master ref, with command/receipt in `.local/tmp/w11-delivery.log`.
