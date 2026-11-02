# Goal: MCP result/error parity with OC2 TS (remove MCP safeguards that corrupt results)

Status: active
Source: user instruction 2026-11-02 — "контракты безопасности выкинуть … нам нужен паритет с oc2 ts без бесполезных сейфгард костылей"; "делай копию плана в цель и начинай реализацию, потом коммит пуш"; review remarks must be addressed.
Last updated: 2026-11-03

## Objective
With chrome-devtools-mcp (stdio) and any TS/Python MCP server, oc2 Rust delivers MCP results/errors to the model as OC2 TS (opencode 2.2.5, `.local/opencode`) does: verbatim text and media, visible `isError` text, large text as preview + `read(path)` artifact, and timeouts/bad/oversized results as ordinary tool failures with the server reused. A whole turn does not fail for these cases.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Reproduction (baseline HEAD 9aacfb19c91cc24f0bc27d1d80385986d298efa2)
Rig outside the worktree: `/home/opencode/.cache/opencode-tmp/opencode/mcp-repro`. It has `scripts/peer.py` (scripted Responses peer), `scripts/run.sh KIND PORT` and `scripts/both.sh BASEPORT` (isolated HOME/XDG/--data-dir, `env -i`; env SCENARIO=big|shell|env, EXEC=<ms>, EXTRA_ENV, CMD), and `pkg/` with chrome-devtools-mcp 1.12.0. Docker container `oc2-mcp-chromium` (chromedp/headless-shell) runs on 127.0.0.1:9222; restart it before each run.

| case | TS 2.2.5 | Rust baseline |
|---|---|---|
| list_pages/snapshot text | `1: data:…`, `uid=1_0` | `[redacted]: …`, `uid=[redacted]_0`, `<h[redacted]>` |
| isError (bad args) | server text visible | `error: mcp tool reported failure` |
| take_screenshot png | inline ~24 KB | BadResult (decoded PNG contains byte `1`) → unknown → turn failed |
| snapshot 1.36 MB | ~55 KB preview + file, continues | BadResult (>1 MiB) → unknown → turn failed |
| execution timeout 3 s | tool error "Request timed out", next call ok | Deadline → unknown → turn failed |
| shell `echo shell-out true 1 opencode` with MCP env `{"npm_config_offline":"true"}` | — | `shell-out [redacted] 1 opencode` |
| env inheritance to child | ok | ok (not a defect; output only mangled) |

Root causes: `mcp_stdio.rs` `launch_cancellable_generation` (~618-628, 664-666) and `effective_environment` (~243) put every admitted env value, overlay value and argv into redaction/identity lists. `runtime/mcp.rs` `mcp_redactions` (~229-278) adds every MCP environment value, URL, ordinary header and provider base_url. Consumers are `mcp_result::redact`, `media.rs` `binary()`/`mime()`/`exact()`, `mcp_lookup.rs`, and a second redaction layer in the common output owner. `isError` becomes opaque in `ResultError::Failed`. `bound()` at 1 MiB returns BadResult. `execute_mcp` treats Deadline and BadResult as unknown and poisons the server.

## Frozen Contract

### Required Outcomes
- R1: No redaction or secret-based rejection in MCP model-visible output (stdio + remote: tool results, instructions, resources/prompts lookup, media, isError text, spilled artifacts, Code Mode publication of MCP output). Text containing `1`/`true`/env/argv/provider-key values arrives verbatim; a PNG containing such bytes is accepted.
  - Acceptance: live rig default scenario shows no `[redacted]` and take_screenshot inline; regression tests assert verbatim text, including a retained-dictionary value such as the fixture provider key `test-key`, in small, large/spilled, isError and Code Mode paths.
  - Status: verified
  - Evidence: MCP lib 75 PASS; Code Mode 9 PASS; application/media and 9 MiB artifact/page regressions PASS. Live default Rust/TS: 9 steps each, no `[redacted]`, inline PNG (23,974-byte data URI), exit 0.
- R2: Shell/other tools are no longer mangled by ordinary MCP config values. `mcp_redactions` keeps only credential material: credential-named parent env, provider api_key/headers/redaction_material/request-binding api_key+headers, and credential-like MCP header values (Authorization/bearer token, credential-named headers). It drops MCP environment values, `blocked_inherited_values`, MCP url, provider base_url and ordinary header values (e.g. `Content-Type`).
  - Acceptance: live SCENARIO=shell output is `shell-out true 1 opencode`; `duplicate_binding_redactions_keep_protected_set_and_allow_silent_shell_capture` is updated to the new set.
  - Status: verified
  - Evidence: exact credential-dictionary regression PASS; live Rust shell body `shell-out true 1 opencode` (existing `exit 0` envelope retained), run exit 0.
- R3: `isError:true` text (joined text parts) reaches the model. Numeric JSON-RPC errors keep fixed categories with empty text. Status stays `failed`, no poison.
  - Acceptance: test with captured provider output containing server error text; live c4 shows the server text.
  - Status: verified
  - Evidence: joined-text unit and captured-provider tool_lifecycle/isError application assertions PASS; live c4 exposes missing-pageId error text in both implementations (different native error envelopes, not an opaque category).
- R4: Well-formed text results up to 16 MiB go through the existing common preview + `read(path)` artifact. Results carrying binary media keep the 1 MiB aggregate cap. Over cap → `failed` "MCP result N bytes exceeds L bytes" (TooLarge), no poison.
  - Acceptance: test with a 1.5 MB text → preview + artifact (tail sentinel present), next call to same server ok, one spawn, unknown=0; live SCENARIO=big EXEC=90000 c2 shows a preview, c3 ok.
  - Status: verified
  - Evidence: 1.5 MB application regression proves preview, artifact tail, same-server pong, one spawn, unknown=0; 9 MiB + `_meta` common-output/page regression PASS. Live big/90000: Rust 48,299-byte and TS 55,357-byte preview envelopes with retained-file markers; c3 ok, exits 0.
- R5: Deadline, BadResult, Unsupported and TooLarge after `tools/call` → `failed` without poison/retirement/remote quarantine, server reused. Unknown + poison remain only for Transport, MessageLimited, CleanupFailed and Cancelled (and remote equivalents).
  - Acceptance: test where execution timeout → failed "timed out", the late response is ignored, the next ping returns pong, one spawn, unknown=0 (stdio); the remote deadline test is extended with a follow-up call; live SCENARIO=big EXEC=3000 → c2 failed, c3 ok, run exit 0.
  - Status: verified
  - Evidence: stdio deadline/late-response/pong regression and remote deadline follow-up PASS; malformed-result failed tests and unchanged video→unknown fixtures PASS. Live big/3000: Rust/TS c2 timed out, c3 ok, exits 0; Rust durable states completed/completed/failed/completed, unknown=0.
- R6: Docs that contradict the new behaviour are rewritten or removed: `docs/TOOLS_MCP.md` (~408 redaction, ~421-450 unknown/isError/oversize), `docs/SECURITY.md` (:7, :16-17), `docs/CONFIG.md:82`, `docs/DECISIONS.md` (new decision "MCP parity with TS" superseding D15 output sanitization + closed error text and the D21 env-redaction clause :143), stale comment `mcp_lookup.rs:41-42`.
  - Acceptance: grep of these docs shows no contradicting statement.
  - Status: verified
  - Evidence: current contracts rewritten; new MCP parity decision explicitly supersedes the historical D15 output clauses and D21 env-output clause; focused contradiction grep/diff check clean.
- R7: Commit and push the current branch to the verified `origin`.
  - Status: in_progress
  - Evidence: final Review-Sol PASS; delivery commit/push next.

### Constraints
- C1: Keep child env admission/inheritance, status-label protection (`config/mcp.rs` protected list), stderr redaction by `config.secrets`, and shell's own redaction of credential material.
- C2: Keep shared `mcp_result::redact`/`redact_json` (other consumers: `models/lookup.rs:220`, `runtime/code_mode.rs:38`). Remove MCP-specific secret parameters and calls; do not pass empty lists (`mcp_result.rs:100-115` masks `token`/`env` fields even with `[]`).
- C3: Keep structural/cap/base64/MIME-syntax validation and the PARTS_CAP (65 parts stays BadResult→ now failed).
- C4: No storage schema/migration change. AGENTS.md cargo rules apply: 4 jobs, no parallel cargo, no clean.

### Non-goals
Roots/elicitation capabilities; JSON-RPC error message text; project-local credential admission (`blocked_inherited_values` as admission); new retry/restart architecture; provider/context/storage budget changes.

## Change Envelope
- Code: `crates/oc-adapters/src/{mcp_result.rs, mcp_result/media.rs, mcp_lookup.rs, mcp_stdio.rs, mcp_remote.rs, runtime/mcp.rs, runtime/mcp/lifecycle.rs, runtime/turn.rs (MCP output prep context), runtime/code_mode.rs, runtime/code_mode/jobs.rs, tools/output.rs (only if a no-redaction option is needed)}` and their tests; `crates/oc/tests/mcp_application*.rs`; fixtures only if needed.
- Review-derived implementation notes:
  - Signature inventory: `mcp_result.rs` `instructions:50`/`project:126`; `media.rs` `project_rich:373`, `make:439`, `exact:396`, `mime:407`, `binary:423` lose secret args (remove sanitization at ~462,493,500,518,525,541); `mcp_lookup.rs` lookup tuple `:43`, delete `safe_identity:325` and secret params of `redact_metadata/validate_blob/redact_resource/redact_content`; delete stdio fields `mcp_stdio.rs:575,577` (+ init 682-683) and remote `:510,640`; merge remote `connect_redacted` into `connect_cancellable` (`:533,540`, lifecycle `:1541`); stdio `launch_redacted` stays for stderr.
  - Second layer: MCP isError/spill uses `turn.rs:6433-6443`→`tools/output.rs:585,828` and `media.rs:145-147`. Code Mode uses `code_mode/jobs.rs:281-291` and `code_mode.rs:556,563`. MCP output must use a redaction-free preparation without disabling redaction for other tools.
  - Oversize: `media.rs:596-599` derived bounds (output, (facts,display)) must not reject large unprepared text before `prepare_common` (e.g. 9 MiB text + `_meta:{}`). Apply final bounds after common preparation. Results with media keep 1 MiB. `mcp_result.rs:133-138,175-177` aggregate checks → TooLarge. Raise `RESULT_TEXT_BYTES_CAP` to 16 MiB as one constant (remove the duplicate `mcp_stdio.rs:28`); lookup uses rise too.
  - Error enums: producers `mcp_result.rs:35,141`, `media.rs:445`, enums `mcp_stdio.rs:62-65`, `mcp_remote.rs:142-145`, numeric RPC `mcp_stdio.rs:872`, `mcp_remote.rs:809` (empty text), `result_error` `mcp_stdio.rs:924-932`/`mcp_remote.rs:847-853`, runtime `runtime/mcp.rs:286,293,306,313,360,391`, poison matches `464-473,502-511`. Debug may show text (owner accepted).
  - Deadline reuse is safe in rmcp 3.4.0 (cancel sent, pending removed, late response ignored); `lease.armed=false` on every return.
  - Tests to update (verbatim/failed expectations; delete pure safeguard tests): see review lists. `tests/mcp_stdio.rs` (177, 745, 776-817), `tests/mcp_stdio/config_admission.rs:111`, `tests/mcp_stdio/lookups.rs:62,112,213,265`, `tests/mcp_remote.rs:479-483,748,752-797,803,837-846,868-878`, `tests/mcp_remote/lookups.rs:50`, `tests/runtime/tool_lifecycle.rs:884`, `src/runtime/tests.rs:19`, `src/application/mcp_tests/lookups.rs:77`, `src/mcp_result.rs` tests 188-305, `src/mcp_result/media/tests.rs` 10-134 (65 parts vs byte overflow split), `src/tools/output/tests.rs:473`, `src/tools/mcp_log/tests.rs:4`, `src/runtime/media_tests.rs` seed, `src/storage_turn_history/tests.rs:853`, `crates/oc/tests/mcp_application.rs` 1451-1472, 3864-3873, 4188-4195, 4198 (add isError text assert), 4280-4409, `mcp_application/config_admission.rs:106`, `mcp_application/media.rs:15,118,261`. Do not rewrite the `type:"video"` fixture (remote decode → Transport stays unknown); use invalid-base64 image for BadResult→failed.
- Forbidden: new deps, storage migrations, global removal of the shell redactor, changes outside MCP/output paths except docs.

## Primary Evidence / Checks
- `CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib -- --test-threads=4 mcp`, plus the exact filter `duplicate_binding_redactions_keep_protected_set_and_allow_silent_shell_capture`
- `cargo test -p oc-adapters --test mcp_stdio`, `--test mcp_remote`, the runtime tool_lifecycle target; `cargo test -p oc2 --test mcp_application` (all with `CARGO_BUILD_JOBS=4 … -- --test-threads=4`)
- `CARGO_BUILD_JOBS=4 cargo clippy -p oc-adapters -p oc2 --all-targets -- -D warnings`
- `bash scripts/test.sh` once before commit
- Live: `cargo build -p oc2 --bin oc2 --locked --release`, then rig `scripts/both.sh` default, `SCENARIO=big EXEC=90000`, `SCENARIO=big EXEC=3000`, and `SCENARIO=shell` (rust). Rust output must match TS behaviour per step.

## Current Checkpoint
- Closes: R7
- Smallest next action: normal commit/push of the reviewed code/tests/docs/goal to verified origin/master, then record delivery evidence and complete this goal.

## Current State
- Resolved: R1–R6 implementation and own verification; independent final Review-Sol PASS, no findings. Only R7 delivery remains.
- Scoped checks (all Cargo jobs=4, test threads=4, approved TMPDIR): MCP lib 75 PASS; exact credential-dictionary and 9 MiB artifact/page regressions PASS; stdio 20 PASS/1 intentional real-server ignore; remote 28 PASS/1 credential-dependent ignore; runtime tool_lifecycle 24 PASS; Code Mode 9 PASS; mcp_application 45 PASS in the full gate after affected scoped retests. Strict scoped Clippy PASS.
- Full `bash scripts/test.sh` ran once: 2612/2613 PASS, 25 skipped; the sole failure was unchanged `pty_t39::lifecycle::vis38_pty_dcp_equal_active_archive_resource_samples`, `/proc` peak thread counts 11 vs 10. Its exact isolated rerun passed (10 vs 10, 3.92s), without code/test/assertion/threshold changes. Do not describe the initial full gate as wholly green. Cold compile was 15m55s; actual full test execution 237.989s.
- Release build PASS (`target/release/oc2`). Required live default/big90000/big3000/shell runs PASS; all seven Rust/TS run exits are 0. Peer logs and metadata-only assertion summary: `.local/tmp/mcp-live-*-peer.jsonl`, `.local/tmp/mcp-live-summary.json`; scoped/full/release logs `.local/tmp/mcp-*.log`.
- Initial scoped lib command hit the shell 120s timeout during a cold dependency build (not a test wait); no surviving Cargo/rustc remained. Continued as one background Cargo owner, without clean/target override.
- Next: normal commit/push to verified origin/master. No storage schema/migration/dependency changes; foreign `progress/` untouched.

## Material Decisions
- 2026-11-02: the external live rig now declares its fixture model image-capable and logs image MIME/URI lengths plus artifact-marker presence (not image payloads). Without that declaration Rust correctly applied its existing text-only-model image fallback, so the original rig could not prove the inline-image outcome. No product capability gate was changed.
- 2026-11-02: the external TS runner uses its documented `--standalone` private server: one repeat otherwise failed before the peer call because its managed service's default port 49374 was busy. No user owner/process/config was restarted or changed.
- 2026-11-02: rmcp 3.4.0's untagged `ServerResult` retains undecodable content (including `video`) as `CustomResult`, rather than surfacing a receive error. Both call owners classify this opaque fallback as Transport; recognized unsupported response variants still fail normally. Video fixtures remain unchanged. Standard numeric RPC categories retain empty text; other SDK errors keep their prior Transport classification.
- 2026-11-02: registered artifact pages are already admitted by each producer's capture writer; `prepare_page` must not apply a reader-generation dictionary again, which otherwise corrupts `read(path)` of verbatim MCP artifacts. Shell/other-tool credential redaction remains at producer/capture ingress; no artifact metadata or schema is added.
- 2026-11-02: Code Mode records only an invocation-local boolean when an admitted MCP call returns; its mixed publication then stays verbatim. Execute programs without MCP calls retain existing output redaction. The common capture writer receives the publication's omitted dictionary through the existing context (no new storage API/state).
- 2026-11-02: owner overrides MCP security/sanitization contracts in favour of TS parity (D15/D21/SECURITY/TOOLS_MCP superseded for MCP output).
- 2026-11-02: media-bearing MCP results keep the 1 MiB aggregate cap to avoid whole-turn request overflow (review finding); text-only results go to 16 MiB + spill.

## Checkpoint History
- 2026-11-02: live repro TS vs Rust (table above); plans v1/v2 reviewed; contract frozen.
- 2026-11-02: R1–R6 implemented; scoped/Clippy/release/live evidence complete. Full offline run had one unrelated transient resource-probe failure, exact isolated rerun PASS; final code review queued.
- 2026-11-03: Review-Sol `ses_e61829defffeBPuEgkJmI2yDU7` read the actual 34-file diff and evidence: PASS, no blocking or nonblocking findings. Runtime/tests unchanged after review; subsequent goal edits only record status/evidence.

## Completion
- Resolved outcomes:
- Commands and artifacts:
- Constraint and diff-scope check:
- Final status:
