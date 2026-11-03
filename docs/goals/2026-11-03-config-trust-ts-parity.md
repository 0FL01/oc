# Goal: W5. Config/trust/discovery (HIGH) — меняет AGENTS.md «Config — OC2 allowlist, fail-closed» → «неизвестные ключи игнорируются с diagnostic»

Status: active
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W5 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W5: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

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
K1 unknown keys фатальны: config/admission.rs:19-54,472-538, composition.rs:383-515, config.rs:1153-1160, ProviderEntry deny_unknown_fields config.rs:181,
   defs.rs:899-908,1019-1025,1713-1752 (пропуск целого definition), frontmatter composition.rs:984-999.
   Фикс: (а) СНАЧАЛА минимальная нормализация legacy policy (иначе потеря deny-правил → default allow *): root `permission` v1-объект, `agent.<name>.permission`,
   frontmatter `permission`, `tools: {x:false}` → native permissions с сохранением порядка источников и приоритета native rules (TS normalize.ts:133-183,586-625, v1/config/agent.ts:51-57);
   (б) остальное лишнее — ignore у owning parser с сохранением распознанных полей (включая nested provider request fields);
   (в) warnings → chrome.service_diagnostics (headless stderr headless.rs:93-95, TUI Services services.rs:81).
   Singular `provider`/`mcp.<name>` — ignore с warning, явно НЕ parity (миграция вне scope).
   Tests: permissions/tests.rs + composition/application: v1 permission deny bash/edit под Build реально deny; extra root/provider/agent/frontmatter поле рядом с валидным →
   startup+reload ok, ограничения сохраняются, warning виден; обновить cfg13_unknown_key_on_reload…, config/admission_tests.rs:12-85,151-191, config/mcp_tests.rs:125.
K2 project MCP: удалить credential-domain collection/refusal composition.rs:1820-1892, inheritance filtering mcp_stdio.rs:230-241, поля blocked_inherited_values/inherit_credentials
   и сравнения runtime/mcp.rs:55-56; source_credential_values удалить как мёртвый. source_protected_values/entry_credential_values/stderr secrets оставить (status/stderr); cwd/resource admission оставить.
   Tests: composition/mcp_tests.rs::mcp09_project_overlay…, tests/mcp_stdio.rs::from_entry_uses_trusted_cwd_and_admitted_parent_env → token наследуется, overlay выигрывает, нет trust_refused.
K3 {file:…} по TS config/variable.ts:40-76: ~, absolute, relative (включая ../), symlinks, trim; любой нечитаемый файл → "" + warning; без 64 KiB cap.
   Менять публичный и admitted readers (config.rs:917-978,1031) и disabled-MCP activation composition.rs:1976-1985; decoded-string substitution без double-escape.
   Tests: эти случаи + newline-terminated key + >64 KiB; обновить cfg04_trust_secrets_disabled / v07c_*.
K4 capacity (skills/byte budget CapacityExceeded): startup composition.rs:1068-1076 и reload composition.rs:1058-1065 — capacity omissions warning-only, sibling definitions грузятся;
   остальная selected-profile валидация без изменений. Tests: >256 skills и >4 MiB aggregate на startup и reload.
Вне W5: `instructions` consumer (не маленькая правка: instructions.rs:25-33, publication.rs:153-158) — отдельная фича; JS plugin host (AGENTS.md); auto-resume после краша (AGENTS.md no-replay);
рекурсивный skill scan; v1 provider/mcp миграция.

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
