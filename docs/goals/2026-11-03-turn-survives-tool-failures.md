# Goal: W3. Turn убивается вместо ошибки инструмента (HIGH/MEDIUM)

Status: active
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W3 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W3: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

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
C1 streamed tool arguments >1 MiB → ByteLimit("arguments") → весь turn failed (provider.rs:45-51,1133-1135,1182-1183, chat.rs:403-404, messages.rs:415-416).
   Фикс в parser: запомнить identity+failure этого call, перестать копить его arguments, дочитать stream, передать через сборку turn.rs:3910-3925
   в существующий Assembled::Failed → durable refusal (:5421-5488). Cap НЕ поднимать. Tests: provider.rs:3622 aud13…, chat_tests.rs:4, Messages-аналог,
   tests/runtime/tool_lifecycle.rs: oversized между двумя нормальными → только он failed, остальные исполнены 1 раз, continuation содержит все 3 outcomes.
C2 headless без --auto: Ask → RuntimeError → turn прекращён (headless.rs:116-120, turn.rs:5272-5274). TS noninteractive.ts:73-74,144-186: reject с feedback, модель продолжает.
   Фикс: headless всегда регистрирует approval consumer; без --auto отвечает Reject{feedback: TS-текст} только на asks своей сессии и её потомков (ancestry, как TS :144-158);
   continuation уже поддержана (turn.rs:5308-5316,5386). Tests: tests/approval_binary.rs:14, configured_workspace/customize_opencode.rs:132; shared-owner сценарий с child ask и чужой сессией.
C3 headless question: для noninteractive Origin Run NoConsumer → обычный failed tool output с TS-текстом (noninteractive.ts:70-71) вместо Err в turn.rs:6013-6035;
   интерактивный Cancelled не меняется. Test: pty_t39/question.rs:250 vis37… → с/без --auto модель получает feedback и продолжает.
C4 turn.rs:4228 «MCP outcome unknown» для shell/execute → generic «tool outcome unknown» по фактическому tool; no-replay/terminal status не менять.
C5 накопление images: проверка 32 MiB до compaction (turn.rs:1824-1834,2970) → ContextOverflow. Фикс: image-only projection как TS model-request.ts:44-47,194-221
   (trigger 25 MiB → target 15 MiB, старейшие images → placeholder) ДО byte-admission checks; RAW history не трогать.
   Repro runtime/user_images_tests.rs: несколько image-prompts на image-capable модели, затем новый request; проверить dispatch, placeholders, RAW.
C6 TLS certificate verification failure → Transport → бесконечный retry (provider.rs:2635-2651, websocket.rs:583). Основание — AGENTS.md (security/config refusal не повторяется), не TS
   (TS ретраит ≤10). Фикс: typed rustls InvalidCertificate через error chain включая io::Error::get_ref() → nonretryable; не классифицировать EOF/reset.
   Tests: self-signed TLS endpoint в provider/timeout_tests.rs + WS connect test; connection-refused retry test остаётся зелёным.
C7 default chunk timeout 300 → 900 s (TS ai/schema/options.ts:56-62); header 300 s. Test provider.rs:4071 prov07_timeouts_and_cancel.

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
