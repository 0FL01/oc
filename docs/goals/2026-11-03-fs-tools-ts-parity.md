# Goal: W2. Файловые инструменты (HIGH)

Status: active
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W2 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W2: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: Each item of the wave section below (with its listed owner, fix, tests): reproduce → fix → GREEN; or `not_applicable` with evidence. Track per item in Current State.
- R2: Docs (SECURITY.md, CONFIG.md, TOOLS_MCP.md, DECISIONS.md, CONTRACTS.md, ARCHITECTURE.md, CODE_MAP.md, TEST_PLAN.md, AGENTS.md, builtin `customize-opencode` SKILL.md — whichever describe touched behaviour) contain no statement contradicting the new code. Historical goals/evidence are not rewritten.
- R3: Checks pass: affected cargo test slices + scoped clippy `-D warnings`; `bash scripts/test.sh` once; release build + live rig where the wave has a live repro; Review-Sol PASS on the final diff.
- R4: Commit + `git push origin master` (no force).

### Constraints
- C1: AGENTS.md cargo rules (CARGO_BUILD_JOBS=4, `-- --test-threads=4`, no parallel cargo, no clean, no CARGO_TARGET_DIR); tmp/logs in `.local/tmp`; test projects outside the worktree.
- C2: No storage schema migration unless proven required (then mark it explicitly). No new dependencies unless listed in the wave.
- C3: Do not touch untracked `progress/`. Keep no-unknown-effect-replay.

### Non-goals
Items listed as «Вне …»/out of scope in the wave text; other waves.

## Wave plan (frozen, from reviewed plan v2)
F1 read текста >1 MiB падает даже с limit=1 (files/read.rs:96-120). Фикс в Files::read_admitted: потоковое построчное чтение с bounded accumulator,
   сохраняя ReadResult/рендер tools/read.rs:250-278, 2000 lines, cursor, page-only artifact; обновить hint tools/diagnostic.rs:98-103.
   Media/PDF НЕ в scope (PDF не поддержан вообще; image cap связан с restore/active-cap) — отдельная фича.
   Tests files/read/tests.rs:17: >1 MiB limit=1, дальний offset, огромная строка, UTF-8 chunk boundary; tail-NUL assertion :31-35 → проверка consumed prefix (как TS).
F2 glob/grep: бюджет 10 000 entries → ошибка/failed (files/search.rs:420-555,659-672; flat dir выбрасывает имена до первого кандидата).
   Фикс: owner возвращает результат + truncated-флаг (обрабатывать кандидатов по ходу перечисления); tools.rs:1078,1158-1175 рендерят completed + честный truncated;
   cancellation/invalid pattern остаются ошибками. Tests: files.rs:1139 aud19_wide_directory…, новые flat-wide glob/grep в files/tests.rs, tools/tests/search.rs:5.
F3 symlinks (files.rs:43-57,625-724, patch/fs.rs:50-101): следовать symlink для явных путей read/edit/write/apply_patch.
   permission/external_directory — по lexical path (как TS file-access.ts:90-116); data-root исключение — по РАЗРЕШЁННОЙ цели I/O (files/read.rs:27-34, files.rs:541-545).
   Запись через leaf symlink меняет referent, ссылку сохраняет; delete/move-source действует на lexical entry (TS environment/local.ts:48-53, test tool-patch.test.ts:459-476).
   Preview/preimage binding — к фактической цели (patch.rs:1176-1248, patch/mutation.rs:158-170). Recursive search НЕ следует symlink (как rg без --follow).
   Tests: files/read/tests.rs, files/tests.rs, patch/mutation/tests.rs, tests/runtime/file_mutations.rs: internal→external link с lexical permissions; write сохраняет ссылку;
   move ссылки сохраняет referent; data-root через ссылку отказ; смена цели во время approval — отказ.
F4 apply_patch: strip/restore BOM; EOL — семантика TS util/src/patch.ts:128-136,311-339 (строки вне replacement как были, replacement из patch). Без dominant-EOL политики.
   Tests в patch.rs рядом с :1731 tool02_crlf…: BOM-first-line, mixed/context EOL по TS-оракулу.
F5 cross-device move (patch.rs:1148-1161 preflight refusal): убрать refusal; move = stage/commit prepared bytes в destination → удалить source;
   failure accounting (:1359-1423) отражает уже созданный destination. Tests: patch.rs:2068 → успешный cross-device move + failed remove после записи dest (TS oracle tool-patch.test.ts:873).

## Риски
- W1/K2: секреты в выводе инструментов и env project MCP больше не маскируются/не фильтруются — сознательный паритет TS.
- N1/N2: запросы модели во внутреннюю сеть ограничивает только permission policy (включая redirects) — как TS.
- F3: symlink наружу — permission по lexical path, как TS; data-root защищён по разрешённой цели.
- K1: ранее отвергаемые конфиги загружаются; v1 permission переводится, а не теряется.
- C6 отклоняется от TS (там ≤10 ретраев) в пользу AGENTS.md правила «security/config не повторяются».

## Change Envelope
- Owners/files/tests listed in the wave plan above and their direct consumers; docs listed in R2.
- Forbidden: widening to other waves; new deps/migrations beyond the wave text.

## Current Checkpoint
- Closes: R1–R4
- Smallest next action: General-Sol starts with the first item of the wave.

## Current State
- Resolved:
- Next: implementation.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.

## Completion
- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
