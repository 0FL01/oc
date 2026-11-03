# Goal: W1. Redaction-словарь портит вывод всех инструментов (HIGH, тот же класс, что MCP)

Status: complete
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W1 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W1: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

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
Механизм: runtime/mcp.rs:229-275 mcp_redactions (значения env с KEY/TOKEN/SECRET/PASSWORD/CREDENTIAL/AUTH/COOKIE/BEARER в имени,
provider api_key/headers/redaction_material; без min length) → tools/output.rs:989-1014 literal substring → "[redacted]".
TS: в shell/read/webfetch/runtime redaction нет (tool/plugin/shell.ts:77-90, read.ts:153-158, webfetch.ts:184-195).
Удалить словарь из всех model/history-visible путей:
- tools/output.rs Context.secrets целиком: body, Write path/content (:390-433), question prompts/options/answers (:527-552),
  webfetch JSON header (:783-793), diff capture с построчным разбиением (:165-189), refusal preparation (runtime/turn.rs:5429-5444).
- shell: turn.rs:6255-6268, user_shell.rs:223, shell/jobs/output.rs:184-245, shell/jobs.rs:783-788; command interpolation
  (command_shell.rs:132 → shell/jobs.rs:465-547) вместе с отказом «shell redaction material limit».
- storage_tool_output.rs:252,417-440,507,802-818,894-897 (Writer.redactor, secret args/checks; quota/cap/cleanup сохранить; миграции нет).
- tool-card labels runtime/mcp.rs:64-86 (consumers turn.rs:3296,3488,4901); nested-AGENTS receipt path storage_instructions.rs:71,149-155 (словарь из turn.rs:6592-6598).
- subagent: turn/foreground.rs:219-249, children.rs:1032-1046, turn.rs:4701-4726.
- models/lookup.rs:191-224 redact_json и field-name masking mcp_result.rs:93-120 (schema-known model-field allowlist сохранить); code_mode.rs:28-38,128-145,451-482,549-575, code_mode/jobs.rs:281-295.
- Удалить ставшие мёртвыми redact_string, redact_value, StreamRedactor, mcp_result::{redact,redact_json}; secret-параметры удалять, не подставлять [].
- provider diagnostics provider.rs:2851-2915: убрать и keyword suppression, и подстановку значений request headers, и отбрасывание сообщения >4096 B (:2873-2875);
  текст провайдера verbatim с существующими normalization/bounds (TS ai/route/executor.ts:81-124).
Сохранить: mcp_redactions builder ТОЛЬКО для MCP lifecycle diagnostics/status/stderr (lifecycle.rs:388,671,1549, reconcile.rs:51) + ProviderOptions.redaction_material;
config explain (config.rs:1929-1970), mcp_stdio.rs:391-410, safe_display_label, auth/accounts masking, credential Debug, logs.
RED tests: tests/runtime/tool_lifecycle.rs::w1_builtin_results_preserve_key_and_env_literals (apiKey "ollama" в Generation.providers, env KEYTIMEOUT=1;
shell/read/opencode_models → literals в следующем provider request и durable outcomes); src/tools/output/tests.rs::w1_question_and_webfetch_header_preserve_literals;
tests/runtime/code_mode.rs::w1_execute_without_mcp_calls_preserves_return_and_logs; src/provider/tests/typed_failures.rs::w1_provider_message_preserves_url_environment_and_header_literals.
Обновить (не удалять смешанные): tools/output/tests.rs:4,227,273,484,564 (удалить :344 StreamRedactor); shell/jobs/output_tests.rs (удалить :5, :459-482; обновить :222,374);
runtime/code_mode/tests.rs:4,52; application/user_shell_tests.rs:432-490; storage_tool_output/tests.rs:873; storage_instructions_tests.rs:210; runtime/tool_stream/tests.rs:4;
models/tool_tests.rs:102; runtime/tests.rs:156; provider/tests/typed_failures.rs:690-702; provider/websocket/tests.rs:1220-1246; application_conversation_tests.rs:124.
Live: rig SCENARIO=shell с EXTRA_ENV="KEYTIMEOUT=1" → `shell-out true 1 opencode`.
Docs: SECURITY.md:7,30; CONFIG.md:84,558-567,631; TOOLS_MCP.md:115,283-309; DECISIONS.md:353-355 (+ новый D «tool output verbatim»); CONTRACTS.md:146-151;
crates/oc-adapters/assets/skills/customize-opencode/SKILL.md:281.

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
- Smallest next action: none — all frozen outcomes verified; stop substantive work.

## Current State
- RED: `cargo test -p oc-adapters --test runtime --locked w1_ -- --test-threads=4` fails both new regressions: shell publishes `[redacted] true [redacted] opencode`; Code Mode replaces `ollama` in return/logs. Synthetic literals only; log `.local/tmp/w1-red-runtime.log`.
- Owner unit RED reproduced: 13 failures / 22 selected tests (output/questions, Write/diff, capture, instruction receipts, models, Code Mode previews, provider diagnostics); an additional 2/2 RED covered MCP labels and live user-shell output. Logs `.local/tmp/w1-red-lib.log`, `.local/tmp/w1-red-label-user-shell.log`. Unit target cold compilation took 7m07s; tests themselves ≤5.21s.
- Runtime removal implemented: no output dictionary/StreamRedactor/field-name masking or secret parameters; the MCP-only publication bypass and Code Mode `mcp_output` fork are dead and removed. MCP lifecycle builder/callers preserved; no deps/schema changes.
- Docs updated for changed current contracts, including T44/S08 normative clauses; historical goals/evidence untouched. R2 CODE_MAP.md and AGENTS.md inspected: no touched masking claims or moved owners, no edits required.
- Per-item owner RED→GREEN: common body/Write/question/webfetch/diff and cold capture; shell publication/user-shell/command output; labels/instruction receipts; model lookup/field masking; Code Mode previews/return/logs; structured provider diagnostics. Shared preparer removal also covers child output. All are GREEN in the final full gate; no item is not_applicable.
- Runtime regression assertions now identify each durable call separately and parse the formal Code Mode JSON return, rather than allowing another call/log to satisfy the literal assertion. Final integration/subagent slice: 47/47 GREEN in 5.93s (both W1 cases + all 45 child cases), `.local/tmp/w1-green-integration.log`.
- Scoped Clippy: `cargo clippy -p oc-adapters -p oc2 --all-targets --locked -- -D warnings` PASS (2m39s), `.local/tmp/w1-clippy.log`.
- Full gate, run once: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 RUST_TEST_THREADS=4 bash scripts/test.sh` PASS. 2615/2615 tests, 25 existing skips, 248.687s test execution; doctests (0 cases), strict workspace Clippy, debug build/help/version PASS. VIS38 passed first run (4.762s); no rerun. Cold test-profile build 19m31s, debug build 5m18s; no test/load/deadline/baseline changed. Log `.local/tmp/w1-full-gate.log`.
- Release: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release` PASS (12m46s), `target/release/oc2`, `.local/tmp/w1-release.log`. Docs check PASS (4 examples / 3 source baselines); `git diff --check` PASS.
- Synthetic live: rootless `docker restart oc2-mcp-chromium`, then from `/home/opencode/.cache/opencode-tmp/opencode/mcp-repro`, `SCENARIO=shell EXTRA_ENV="KEYTIMEOUT=1" bash scripts/run.sh rust 18900` PASS, exit0. `run-rust-shell/peer.log` c0 is `exit 0\nshell-out true 1 opencode\n`, without `[redacted]`; `.local/tmp/w1-live.log`. No paid generation/visual qualification; no user owner restarted/killed, rig-only cleanup found no remaining product processes.
- R1/R2/R3 verified: final Review-Sol PASS (`ses_e5f9f09a7ffe9BKlXhBcI2PvXf`), no findings/fixes. Reviewer read all57 files and actual owners/consumers/tests/docs, confirmed logs and synthetic c0; no edits or Cargo by reviewer. Reviewed tracked-diff SHA256 `7c2b4c7ed8c482810fef8707ecd497ab5ca2031354f0557d43af664f13edb88e`.
- R4 verified: commit `6d82394c62d49ef60a1da6713686f19d88a5461c` (`fix(tools): preserve verbatim output for TS parity`) includes all five goal artifacts; `git push origin master` succeeded (`ae0b7e214..6d82394c6 master -> master`, no force). Four later-wave plans unchanged; `progress/` untouched. No remaining outcome or blocker.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.
- 2026-11-03: verbatim means no secret-dictionary/field-name suppression, while existing normalization, typed control and resource bounds remain. Current T44/S08 contract clauses will be explicitly superseded where affected; historical goals/evidence remain untouched.
- 2026-11-03: Code Mode program return/log text uses the existing verbatim text/capture path regardless of whether it called MCP. Program JSON is data, not a native control envelope; execution/call status remains in typed Execute/job facts. This removes the redaction-only `mcp_output` fork without widening limits or replay authority.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.
- 2026-11-03: runtime RED reproduced (2/2 failures). Cold dependency compilation initially exceeded the foreground tool deadline; no Cargo remained running, resumed once in background. Test fixture `ProviderEntry` shape corrected before behavioral RED.
- 2026-11-03: owner RED completed (15 failures across 24 selected unit tests); implementation and current-contract docs updated. Known literal corruption reproduced, no item closed not_applicable.
- 2026-11-03: final scoped integration 47/47, Clippy, full gate 2615/2615, release and synthetic live PASS. Initial builtin fixture permission corrected before final GREEN; live preflight restricted to current UID after refusing a foreign `/proc` read. No foreign process/credential access continued.
- 2026-11-03: independent final Review-Sol PASS with no findings; no behavioral changes after successful checks/review.
- 2026-11-03: reviewed wave committed and pushed; all R1–R4 verified. This final status/evidence-only update does not change the reviewed/tested runtime or docs policy.

## Completion
- Resolved outcomes: R1, R2, R3, R4 verified; no not_applicable/waived/pending items.
- Commands and artifacts: exact required checks/results above; synthetic RED/GREEN/gate/release/live logs in `.local/tmp/w1-*.log`; reviewed wave commit/push `6d82394c62d49ef60a1da6713686f19d88a5461c`.
- Constraint and diff-scope check: 52 modified tracked files + five goal artifacts; runtime change stays within W1 owners/direct consumers. No deps/migration, no test/load/deadline weakening, four later-wave plans SHA256 unchanged, `progress/` untouched.
- Final status: complete. Offline + specified synthetic live qualification only, not paid/visual or whole-product READY. No substantive work remains.
