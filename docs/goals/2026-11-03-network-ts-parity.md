# Goal: W4. Сеть (HIGH/MEDIUM)

Status: active
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W4 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W4: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

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
N1 webfetch/Code Mode fetch отказывают localhost/private (approval.rs:274-279, webfetch.rs:230-250,283-299,583-614,740-760). Удалить fetch-специфичные guards,
   общий ip_is_public (нужен endpoint.rs:80 провайдера) оставить. ДОБАВИТЬ проверку captured permission policy на каждый redirect target (как TS webfetch.ts:93-103,160-167;
   сейчас tools.rs:1473-1486 policy не передаёт). Tests: loopback webfetch/Code Mode success; allow initial / deny redirect target → 0 запросов в target; обновить tool08_*, aud26_*.
N2 remote MCP: убрать check_host prelookup/private refusal (lifecycle.rs:1529-1543, mcp_remote.rs:535-544) и запрет query (mcp_remote.rs:220-228); остальная URL/header валидация остаётся.
   Test: initialize/list на loopback /mcp?token=fixture без OC_TEST_ALLOW_LOOPBACK; обновить tests/mcp_remote.rs::private_host_refused_without_test_flag. MCP OAuth — вне scope.
N3 proxy: generation provider.rs:2413-2429,2654-2657 и discovery.rs:248-291 — system proxy (убрать .no_proxy()); origin prelookup/pinning/peer-classification пропускать
   только когда ЭТОТ origin проксируется; matcher HTTP(S)_PROXY/ALL_PROXY/NO_PROXY из существующей hyper-util; webfetch/MCP — тот же клиентский proxy.
   WebSocket: туннель через выбранный proxy (WSS_PROXY/WS_PROXY → HTTPS_PROXY → ALL_PROXY, NO_PROXY, как TS websocket-constructor.ts:22-78); HTTP fallback только при реальном провале установки.
   Repro offline: локальный CONNECT proxy → loopback TLS provider (fixture CA, trusted binding), origin hostname известен только proxy; затем NO_PROXY direct bypass;
   WS тест в provider/websocket/tests.rs (Upgrade через proxy, прямых соединений к origin нет).
N4 webfetch 5 redirects/1 MiB → 10/5 MiB (webfetch.rs:23-30). Tests: фиксированные 2 MiB body и 6 hops — RED сегодня.
Вне W4: NODE_EXTRA_CA_CERTS (только при реальном репро), MCP OAuth.

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
