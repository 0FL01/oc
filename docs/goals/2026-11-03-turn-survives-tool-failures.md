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
  - Status: verified; C1–C7 reproduced and passed affected regressions, including the three corrected C5 review findings.
- R2: Docs (SECURITY.md, CONFIG.md, TOOLS_MCP.md, DECISIONS.md, CONTRACTS.md, ARCHITECTURE.md, CODE_MAP.md, TEST_PLAN.md, AGENTS.md, builtin `customize-opencode` SKILL.md — whichever describe touched behaviour) contain no statement contradicting the new code. Historical goals/evidence are not rewritten.
  - Status: verified; current docs/source descriptions aligned, docs checker and final code review PASS.
- R3: Checks pass: affected cargo test slices + scoped clippy `-D warnings`; `bash scripts/test.sh` once, with one confirming rerun allowed after this diff's obsolete test expectations are fixed (owner authorization continued from W2); release build + live rig where the wave has a live repro; Review-Sol PASS on the final diff. Report any unchanged flaky PTY failure and isolated recovery honestly.
  - Status: verified under the owner's explicit unchanged-PTY acceptance rule: final gate 2635/2636 PASS, sole unchanged replay-control fixture passed both prior full runs and an unchanged isolated rerun. Final affected tests/Clippy/release/live and code review PASS. This is qualified gate acceptance, not a claim that the script exited successfully; no further full gate is run.
- R4: Commit + `git push origin master` (no force).
  - Status: pending; publish on top of `74a40040` after the authorized final gate, without changing its AGENTS.md content, using the new commit-message rule.

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
- Smallest next action: commit/push the reviewed implementation on top of `74a40040` and record actual publication. No implementation work, review correction or additional full gate remains.

## Current State
- Start: HEAD `8d04255eda01913824ed8db9d9d81c988dc3dd1f`, branch master; tracked/staged diff empty, only foreign `progress/` untracked and untouched. Earlier waves are complete, later waves are not implemented.
- C1 verified: all three parsers first aborted generation on argument overflow; the real middle-call fixture failed the whole turn (RED 5/5, `.local/tmp/w3-red-arguments.log`). Parser-local typed failure now prevents any execution of retained prefixes while bounded canonical calls preserve identity; sibling effects/outcomes occur once and continuation runs. All provider/parser/reconciliation tests plus the runtime regression GREEN 101/101 (8.170s), `.local/tmp/w3-green-arguments.log`.
- C2/C3 verified: real headless shell/skill asks and questions first stopped the turn (RED 3/3, `.local/tmp/w3-red-headless.log`). Shell ask now receives exact TS feedback, continues, has no effect/grant, and auto aliases/Deny retain their behavior; question NoConsumer continues only for Run origins, interactive cancellation/restart remain GREEN (`.local/tmp/w3-green-headless.log`, `.local/tmp/w3-green-headless-recovery.log`). Final actual native shared-owner regression proves a grandchild ask is rejected while an unrelated TUI view's ask stays pending; skill refusal is durably `denied` (existing state), not a new `failed` state (GREEN 2/2, 0.730s, `.local/tmp/w3-green-headless-owned.log`).
- C4 verified: the existing unknown-effect/no-overlap regression first observed MCP-specific wording (RED 1/1); the common diagnostic now says `tool outcome unknown; retry may duplicate side effects`. All five affected MCP unknown-effect fixtures GREEN (2.812s), with terminal state/effect counts/quarantine unchanged (`.local/tmp/w3-{red,green}-unknown-diagnostic.log`).
- C5 verified: four individually admitted valid ~6MiB PNG prompts followed by text first reproduced `ContextOverflow { bytes: 33569145, cap: 33554432 }` before request 3 (RED, `.local/tmp/w3-red-images.log`). Bounded source coordinates/one-record reads and request-only typed image references now apply the exact TS prefix cut before admission/fingerprints. All five requests dispatch, including restoration of >32MiB aggregate RAW logs; reopen preserves original PNGs. Image/compaction/storage slice: 58/60 initially passed; the two deferred-image DCP measurement failures were fixed at their shared owner. Final affected slice 6/6 PASS (13.616s, `.local/tmp/w3-green-images-final.log`), including both DCP regressions, SQL equivalence, repeated-trigger/nested-image semantics and real RAW history. Per-record, nonimage metadata and projected/request byte caps remain.
- C6 verified: real self-signed HTTP and WebSocket peers first yielded retry-eligible Transport (RED); typed rustls `InvalidCertificate` source/get_ref inspection now yields nonretryable certificate refusal, with no WebSocket HTTP fallback. EOF/reset text and connection-refused remain non-certificate/retryable paths; synthetic fixture cleanup is bounded.
- C7 verified: existing `prov07_timeouts_and_cancel` expected 900000ms but observed 300000ms (RED); default chunk is now 900000ms, header remains 300000ms, overrides/cancellation unchanged.
- C6/C7 evidence: `test(/w3_http_certificate|w3_websocket_certificate|prov07_timeouts_and_cancel/)` RED 3/3 (0.162s), `.local/tmp/w3-red-tls-timeout.log`; `test(/w3_.*certificate|prov07_timeouts_and_cancel|cfg16_pre_send_network|cfg16_http_headers/)` GREEN 6/6 (1.227s), `.local/tmp/w3-green-tls-timeout.log`. Cold graph compilation took 10m39s for RED and adapter recompilation 7m37s for GREEN; actual tests were bounded, no hung process/repeated timeout.
- Final implementation review found three C5 defects; all were reproduced before fixing (RED 3/3, `.local/tmp/w3-red-review.log`): later image-read batches persisted an oversized HOT journal that the existing read-side byte guard rejected on retrieval; in-task compression serialized unresolved history images; capability fallback used a clipped card presentation instead of full native/opaque text. Read admission now reserves against the actual current RAW/HOT record as well as the projected request, renewal uses the shared exact size-only image measurement, and fallback preserves producer text. Affected projection/read/renewal/compaction tests GREEN 17/17 (26.399s; new native-read case 25.571s), `.local/tmp/w3-green-review.log`. The read fixture completes all 60 calls once, refuses over-budget reads normally, keeps the record within its unchanged cap and restores history.
- Strict scoped Clippy: `cargo clippy -p oc-adapters -p oc2 --all-targets --locked -- -D warnings` PASS after those fixes (2m24s, `.local/tmp/w3-clippy-final.log`). Dead whole-history memo/cache plumbing was removed; no suppression or storage migration.
- R2 aligned: current tool/permission/question/image/retry/timeout contracts, architecture/owner map, security, builtin manual and T44 amendments now describe W3; historical goals/evidence remain unchanged. AGENTS.md's security/refusal retry wording remains accurate and is not edited. `python3 scripts/check_docs.py` PASS (4 examples/3 source baselines); `git diff --check` PASS; current TS pin/MIT notice verified.
- First full gate: FAIL, 2632/2634 PASS, 25 existing skips (248.072s test execution; `.local/tmp/w3-full-gate.log`). One compatibility fixture still expected the superseded 300000ms chunk default; only that expected value/provenance was updated, and the provider/model/variant precedence test is GREEN (1/1, `.local/tmp/w3-green-gate-default.log`). Unchanged VIS38 sampled small/large thread peaks 11/10; its exact cause is unproven, assertions/load unchanged, isolated rerun PASS (4.597s, `.local/tmp/w3-isolated-vis38.log`). All W3 regressions passed in the full run, including RAW image history (20.225s); UI07 stayed within its existing 40s exception (31.055s).
- Authorized confirming gate: FAIL, 2633/2634 PASS, 25 existing skips (240.241s, `.local/tmp/w3-full-gate-confirm.log`). The corrected C7 fixture and VIS38 passed in that run. The sole failure was unchanged `v07b_remote_definitive_is_error_remains_failed_and_reusable`: all body assertions completed, but the fake Responses server's BrokenPipe panic surfaced from its teardown Drop join; the exact cancelled request/timing is unproven. The same unchanged test passed the first full run and its isolated rerun (1.226s, `.local/tmp/w3-isolated-mcp-definitive.log`); no causal W3 production defect is established and no test/helper/deadline was weakened or changed. This is not a whole-gate PASS.
- Gate tail separately PASS before the review fixes: workspace doctests (0 cases, explicit four threads), debug build/help/version (`.local/tmp/w3-gate-tail.log`). Final release `cargo build -p oc2 --bin oc2 --locked --release` PASS after all three fixes (13m37s, `.local/tmp/w3-release-final.log`), binary `target/release/oc2`.
- C2 live sanity PASS with release binary, isolated HOME/data/project under the existing MCP rig, synthetic loopback peer, shell Ask and no --auto: exact TS feedback field, two main requests, final `done`, durable denied result, no marker/grants (`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode python3 .local/tmp/w3-live-headless.py`, `.local/tmp/w3-live.log`). The first harness check wrongly expected feedback in error.message rather than the existing feedback field; corrected after confirming denied/no effect, without production change. No remaining rig products; user owner untouched. No paid/paired visual qualification.
- Final C2 release sanity repeated after the review fixes: PASS, exact feedback/continuation, two requests, zero effects/grants (`.local/tmp/w3-live-final.log`). No paid/visual qualification. Same Sol follow-up: code-review PASS for all three fixes and their direct interactions; 51-path snapshot `19f52bfe2f1ad58cd441cda47c4161d6eab966b390408dcef13a98c5c52db850`, source unchanged after review. The reviewer correctly identifies the P1 RED guard as read-side, not prevention of persistence. At that checkpoint the post-review version had affected-slice/Clippy/release evidence but no third full gate, and the reviewer did not waive R3. The subsequent explicit owner authorization and final-gate outcome are recorded below; earlier failures remain visible.
- Prior blocker resolved: owner authorized one final full gate and clarified the unchanged-teardown/flaky-PTY acceptance rule above. HEAD `74a40040db3d5e8b4f9848a6d2993505d5ea8492` adds only the commit-message convention; reviewed W3 implementation contents are unchanged. R1/R2 remain verified; R3/R4 are now executable, with no outstanding blocker.
- Final authorized gate: script FAIL (exit 100), 2635/2636 PASS, 25 existing skips, 258.365s test execution (`.local/tmp/w3-full-gate-final.log`). All W3 regressions passed, including current-record read bounds (25.530s), image-history restoration (16.303s), shared-owner ancestry and full-text fallback; prior VIS38 and BrokenPipe cases also passed. The sole failure was unchanged `v07e_bare_pty_model_and_stdio_tool_controls_are_inert_across_replay`, waiting for replay detail while the captured screen showed the card list. Its exact timing cause is unproven; production TUI/test helpers/body/assertions/deadlines were not changed. It passed both earlier W3 full runs (0.727s/0.606s) and its unchanged isolated final rerun (0.628s, `.local/tmp/w3-isolated-final-controls.log`), satisfying the owner's explicit acceptance rule. No further full gate. Workspace fmt/strict Clippy passed; the 17m59s pre-test duration was compilation of dependency/workspace test targets, not a test wait. UI07 remained within its existing 40s exception (31.033s).
- Final skipped gate tail separately PASS: workspace doctests (0 cases, explicit four threads), debug build/help/version (`.local/tmp/w3-gate-tail-final.log`). Release/live source is unchanged from the final PASS evidence above. R3 is qualified and verified; only R4 publication remains.
- Same Sol final evidence follow-up PASS: independently verified the unchanged failing test, both earlier full-run passes, final isolated PASS, gate tail, 51-path manifest and explicit owner acceptance conditions. Reviewed snapshot `2cd403b49bfade266eaddeed9a8144588fa7d66c97765564a1da9d13473ca0e4`; non-goal content digest `2e7314365b1b3d5dae48ecfbb2997a73a3f78ec8187c5af72bccb340518199f6` is unchanged. No further code findings or blockers; subsequent edits are goal evidence/status only.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.
- 2026-11-03: C6 follows AGENTS.md's non-retryable security/config rule, not TS retry count. Typed matching needs the already locked/transitively installed rustls error type; a minimal direct dependency on that same rustls version is permitted by this explicit requirement if no existing public re-export supplies it. No new TLS stack/package/version or schema is introduced.
- 2026-11-03: RAW immutability forbids changing stored image history, not the storage-owned read projection. C5 may adjust that direct consumer's early byte-admission order where otherwise RAW image bytes are counted before projection; independent projected/transport and per-record caps remain.
- 2026-11-03: C1 keeps the 1MiB argument cap and independent SSE/generation/reconciliation guards. A failed call's canonical arguments use a bounded valid-object placeholder plus a separate typed failure, never an executable partial prefix. WebSocket append checkpoints are withheld for that normalized output because the server retained different arguments; full continuation does not replay effects.
- 2026-11-03: C2 uses the TS ancestry check without a new cache or fixed depth cutoff; unknown/foreign sessions are ignored and already-settled reply errors are harmless, as in TS. C3 changes only Run/NoConsumer; interactive cancellation and other question errors are untouched. No automatic permission grant is added.
- 2026-11-03: C5 requires bounded metadata/one-log reads at the storage projection owner: collecting all RAW images before clipping would regress bounded memory. Request-only typed image locators may cross existing DCP/capability filters; only retained images are materialized after the TS prefix cut. This also covers usage fingerprints, not just dispatch. Captured HOT/checkpoint sources remain captured, native facts validate before deferral, and no marker in untrusted text gains native authority. Sol's focused feasibility review confirmed no smaller existing read path; final implementation review remains required.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.

## Completion
- Resolved outcomes: R1/R2 verified; R3 verified with explicit unchanged-PTY qualification, R4 pending publication.
- Commands and artifacts: per-item evidence above; final affected tests 17/17, scoped Clippy/release/live PASS; all full-gate results and unchanged isolated recoveries retained honestly.
- Constraint and diff-scope check: latest Sol code-review PASS, 51 task paths, no schema/migration or other-wave implementation; foreign `progress/` untouched. Only completion/evidence text changed after review.
- Final status: active, not complete; publication remains.
