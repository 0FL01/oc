# Goal: W2. Файловые инструменты (HIGH)

Status: complete
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
- R3: Checks pass: affected cargo test slices + scoped clippy `-D warnings`; `bash scripts/test.sh` plus one confirming rerun after compatibility-test fixes (explicitly authorized by the owner on 2026-11-03); release build + live rig where the wave has a live repro; Review-Sol PASS on the final diff. The owner also accepts the confirming gate failing only on the same unchanged flaky PTY tests if each passes an isolated rerun; report the actual gate result honestly.
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
- Smallest next action: none; all required outcomes are verified. Stop W2 work.

## Current State
- Start: HEAD `58f3fbf81c2fd8142f6f8cdd7a9321069920a53f`; tracked diff empty, only foreign `progress/` untracked (untouched). Three independent read-only scouts map F1/F2, F3 and F4/F5; sole mutation/Cargo owner is General-Sol.
- RED: all F1–F5 reproduced before production edits: nextest owner/runtime slice `test(/w2_|tool16_text_pages|tool02_patch_replacement|aud19_wide_directory|incomplete_grep_keeps/)` — 14/14 expected failures (0.822s), `.local/tmp/w2-red-owner.log`. F1 whole-file/NUL refusal; F2 flat discovery discarded candidates and failed status; F3 symlink refusal; F4 BOM conflict/EOL mismatch; F5 cross-device refusal and source modified before failed removal. An earlier fixture-only byte-array size error was fixed before this behavioral RED.
- F1 verified: RED whole-file/NUL refusal → streaming bounded page/line-prefix, distant offset, huge-line/UTF-8 boundary and consumed-prefix GREEN; media/PDF admission unchanged.
- F2 verified: RED empty wide-directory candidates/failed budget → incremental candidate admission, sorted directory descent, completed/truncated publication without fake cursor GREEN; invalid/cancel, byte/time/entry caps and no-follow recursion retained.
- F3 verified: RED explicit-link refusal → lexical permission + resolved-target data-root checks, leaf link retained on write/update, lexical delete/move-source, real-target/link identity approval binding GREEN. Runtime read/edit/write/patch crosses internal→external link with external_directory Deny; same-byte retarget during edit/write approval is refused. Aliased external-directory read/search still excludes actual data root.
- F4 verified: RED BOM/EOL mismatch → BOM restore and TS hunk-context/replacement EOL/final-LF GREEN, including confirmed effect ending/count assertions.
- F5 verified: RED cross-device refusal/source-first mutation → destination-first move and failed source-removal destination Create GREEN, including cross-device real filesystem and post-move sync accounting.
- GREEN: owner/runtime/effects nextest slice `test(/files::|patch::|tools::tests::search::|tools::tests::read::|file_mutations::/) | binary(patch_effects)` — 75/75 (3.065s), `.local/tmp/w2-green-scoped.log`. First iteration exposed directory-descent starvation; fixed at traversal owner, not by changing loads/caps/assertion strength. Obsolete refusal/EOL/source-first expectations were updated, other invariants retained.
- Scoped Clippy `cargo clippy -p oc-adapters -p oc2 --all-targets --locked -- -D warnings` PASS (1m21s); first lint pass found unsafe-block granularity and a collapsible if, fixed without suppression. No live rig is required for this wave.
- Final owner refinements (patch same-byte link retarget preview binding and skipped-prefix NUL) GREEN 2/2 (0.044s), `.local/tmp/w2-green-final-regressions.log`. Current docs/SKILL/source descriptions aligned; CODE_MAP/AGENTS/TEST_PLAN and remaining current Markdown checked, no further contradicted file-tool claims. `python3 scripts/check_docs.py` PASS (4 examples/3 baselines), `git diff --check` PASS. Historical goals/evidence and foreign `progress/` not edited.
- Required full gate ran once: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 RUST_TEST_THREADS=4 bash scripts/test.sh` — FAIL, 2617/2624 passed, 7 failed, 25 skipped (255.244s tests), `.local/tmp/w2-full-gate.log`. Four filesystem/search consumers still asserted the superseded no-follow/source-first/no-final-LF/failed-budget contracts; updated those assertions, retaining race, preimage, exact effects and child-workflow checks. Affected patch-audit/DTO cases GREEN 3/3 and child workflow GREEN 1/1 (0.395s), `.local/tmp/w2-green-gate-expectations.log` and `w2-green-child-recovery.log`.
- The other three initial gate failures were unchanged auth/MCP PTY fixtures. Read-only owner diagnosis established no direct W2 path; each passed its unchanged isolated rerun (auth 17.124s, sidebar 1.151s, control replay 0.640s), `.local/tmp/w2-isolated-{auth,sidebar,controls}.log`. Cause is not proven; do not claim the original gate passed or that serial reruns establish full-suite reliability. No assertions/deadlines or auth/MCP/TUI production code were changed.
- Final `cargo fmt --all -- --check`, docs validation, `git diff --check` and scoped strict Clippy PASS (Clippy 2m27s), `.local/tmp/w2-clippy-final.log`. Unexecuted full-gate tail was run separately: workspace doctests PASS (0 tests), debug build/help/version PASS, `.local/tmp/w2-gate-tail.log`. All seven failing test slices now have GREEN recovery evidence; the original full-gate exit remains FAIL.
- `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release` PASS (16m39s), `.local/tmp/w2-release.log`; binary `target/release/oc2`. No live/paid/visual test was required or run; no whole-product readiness claim.
- Final independent Review-Sol PASS, no findings, session `ses_e5f04957fffeK0HvECjqo2U1IV`; all 30 changed files, direct consumers, TS oracle/license and evidence inspected. The same reviewer accepted the authorized confirming gate and updated execution goal in a bounded follow-up: full diff SHA256 `38786db3e2d498ff5ec6dc52458047a8aaa819424b6add65d5852bb149241598`, non-goal diff SHA256 `93e227941f8915e7366f7c5fd3e019be35ae6a55b7266cbf8eb927869b926438`. Only this completion/status evidence update follows that PASS.
- Owner-authorized confirming gate: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=4 NEXTEST_TEST_THREADS=4 bash scripts/test.sh` PASS, **2624/2624**, 25 existing skips (260.357s tests), `.local/tmp/w2-full-gate-confirm.log`. Workspace fmt/strict Clippy, doctests, debug build/help/version also PASS. All three formerly failing unchanged PTY fixtures passed in the confirming full run; no additional isolated reruns were needed. The first gate remains recorded as FAIL, not relabeled.
- R1–R4 verified: implementation commit `becc261e8b50dfdb6b37243ae658eb682583f7dd` (`fix(files): TS parity for read/search/symlink/patch`) committed all 30 reviewed paths; `git push origin master` succeeded (`58f3fbf81..becc261e8 master -> master`). No unresolved outcome or blocker. Production/test/current-policy-doc code is unchanged since the final scoped review and successful release build.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.
- 2026-11-03: F4 uses the cited TS `derive` literally, including final LF on nonempty updates, LF patch context/replacement, untouched source EOL and BOM restore. No fuzzy matching expansion. F2 budget-limited subsets are sorted only among inspected candidates, marked truncated, and have no invented continuation cursor.
- 2026-11-03: F3 keeps lexical namespace and resolved I/O identities separate. Existing pinned no-follow primitives operate on the already resolved target; config/storage/discovery guards are unchanged. Current DCP/safety guidance is within R2's all-current-docs scope; historical goals/evidence are not rewritten. Source pin/license recorded in SOURCES.md; no dependency/schema/migration added.
- 2026-11-03: after the initial gate's obsolete compatibility assertions were fixed, the owner explicitly authorized one confirming full-gate run. Completion is allowed on PASS, or only the same unchanged flaky PTY failures with passing isolated recovery, with the actual gate result retained. No broader auth/MCP/TUI change is authorized or needed.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.

## Completion
- Resolved outcomes: R1/F1–F5, R2, R3 and R4 verified; scoped tests, final strict Clippy, owner-authorized confirming full gate, release build and final Review-Sol PASS; implementation commit/push successful.
- Commands and artifacts: Current State above and `.local/tmp/w2-*.log`; confirming gate 2624/2624 PASS, with the initial gate's failure retained honestly. No whole-product/live qualification claim.
- Constraint and diff-scope check: implementation commit `becc261e8b50dfdb6b37243ae658eb682583f7dd` contains exactly the 30 reviewed paths; no dependency/schema/migration change; foreign `progress/` untouched. This closeout changes only the execution goal's status/evidence and needs no behavioral retest or separate source review.
- Final status: complete. The earlier gate-decision blocker is resolved; no adjacent auth/MCP/TUI changes made or proposed. No live/paid/visual or whole-product readiness claim.
