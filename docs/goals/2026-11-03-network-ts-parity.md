# Goal: W4. Сеть (HIGH/MEDIUM)

Status: complete
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W4 of W1–W5).
Last updated: 2026-11-03

## Objective
Wave W4: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: Each item of the wave section below (with its listed owner, fix, tests): reproduce → fix → GREEN; or `not_applicable` with evidence. Track per item in Current State.
  - Status: verified; N1–N4 RED→GREEN evidence below.
- R2: Docs (SECURITY.md, CONFIG.md, TOOLS_MCP.md, DECISIONS.md, CONTRACTS.md, ARCHITECTURE.md, CODE_MAP.md, TEST_PLAN.md, AGENTS.md, builtin `customize-opencode` SKILL.md — whichever describe touched behaviour) contain no statement contradicting the new code. Historical goals/evidence are not rewritten.
  - Status: verified; current docs/source/model descriptions and retained notices aligned, docs checker successful.
- R3: Checks pass: affected cargo test slices + scoped clippy `-D warnings`; `bash scripts/test.sh` once (one confirming rerun allowed after diff-obsoleted expectations are fixed); release build + live rig where the wave has a live repro; Review-Sol PASS on the final diff. Under the owner's continued acceptance rule, unchanged teardown/flaky PTY failures that passed another full run and pass an unchanged isolated rerun are accepted with honest reporting, not more full gates; diff-related failures require a fix and affected-slice verification.
  - Status: verified, qualified under the existing owner rule: first-full VIS38 PASS plus current unchanged isolated PASS; historical full scripts remain FAIL. Same Sol verified the new evidence and current code-review PASS.
- R4: Commit + `git push origin master` (no force).
  - Status: verified; commit `d907ca4bb7d2ae9bc3f3f86d9d6c69359cd2d4d9` pushed successfully to `origin/master` (`79d571a75..d907ca4bb`, no force).

### Constraints
- C1: AGENTS.md cargo rules (CARGO_BUILD_JOBS=4, `-- --test-threads=4`, no parallel cargo, no clean, no CARGO_TARGET_DIR); tmp/logs in `.local/tmp`; test projects outside the worktree.
- C2: No storage schema migration unless proven required (then mark it explicitly). The latest W4 owner instruction permits dependency edges strictly needed for proxy tunnelling only when the crates are already in Cargo.lock; record their necessity below. No other new dependencies.
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
- Closes: none; R1–R4 are resolved.
- Smallest next action: none. W4 is complete; W5 has not been started.

## Current State
- Start: HEAD `79d571a75cf1ef47c475c79f7d475b6f7c07552f`, branch master, tracked/staged diff empty; only foreign `progress/` untracked and untouched. W1–W3 complete; W5 is not implemented.
- N2 verified: loopback without the exception flag first failed `PrivateHost`; the exact query URL independently failed `InvalidConfig` (RED 2/2, `.local/tmp/w4-red-mcp.log`, 0.012s). Removed MCP prelookup/private refusal, obsolete option/error/plumbing and query prohibition, preserving other URL/header validation, lifecycle masking, deadlines/cancellation and cleanup. Whole remote-MCP target GREEN 29/29, one existing skip (2.222s, `.local/tmp/w4-green-mcp.log`), including exact `/v1/mcp?token=fixture`, initialize/list and header/OAuth/deadline checks. N3 also verifies this client through the real proxy fixture.
- N1/N4 verified: default loopback, Code Mode loopback without an exception, fixed 2 MiB body and six hops all reproduced; corrected redirect fixture independently reproduced four completed calls instead of failed/failed/completed/completed (5 owner RED failures plus `.local/tmp/w4-red-fetch-policy.log`). Removed fetch-only DNS/private/rebinding code and its now-dead options/Runtime/ToolContext plumbing; shared provider IP classification remains. Captured redirect Deny now prevents both native and Code Mode target requests; Ask continues without prompting. GREEN scoped fetch/Code Mode/approval/endpoint tests 52/52 (2.875s, `.local/tmp/w4-green-fetch.log`), including zero denied-target hits and exactly two Ask-target hits. Independent body cap, DNS/total timeout, conversion joins/cancellation, auth stripping and provider binding tests remain green.
- N3 verified: isolated child fixture RED reproduced five proxy-dependent failures at origin DNS (HTTP, HTTPS forward proxy, WSS, plain WS, proxy certificate), while NO_PROXY direct passed (0.633s, `.local/tmp/w4-red-proxy.log`). Added direct-only origin resolution/pinning/peer checks with unconditional captured URL/literal-IP admission, default system HTTP proxy clients, and HTTP/HTTPS CONNECT under the existing WS establishment deadline/cancel owner. First broad GREEN was156/157 (only fixture WS response.created was missing, not a production defect); the corrected six-case proxy fixture is GREEN0.946s (`.local/tmp/w4-green-proxy-final.log`). It verifies provider generation with trusted/untrusted bindings and no test allowance, captured discovery, webfetch, exact-query MCP, no proxy-auth leakage, NO_PROXY zero proxy hits, real Upgrade/frame with no HTTP fallback, HTTPS proxy plus WSS TLS, plain WS and typed proxy-certificate refusal before dispatch. Existing endpoint/C6/WS delivery/reconciliation tests and all29 remote-MCP cases passed in the broad157-case run (`.local/tmp/w4-green-proxy.log`).
- Scoped strict Clippy PASS21.57s (`.local/tmp/w4-clippy.log`); docs/source/model-description alignment and synthetic certificate/license provenance updated. `python3 scripts/check_docs.py` PASS4examples/3source baselines; `git diff --check` and retained TS MIT notice comparison PASS. Historical goals/evidence unchanged; obsolete bridge exception assignment removed. Live sanity is optional and not needed for the real offline transport repro.
- First full gate FAIL: 2637/2641 passed, four failed,25existing skips,260.329s (`.local/tmp/w4-full-gate.log`). Three failures were obsolete N2 expectations of `DNS: private_host`, now `connect: connection_failed`: partial attachment cleanup, degraded attachment and the two-epoch soak. Only these diagnostics were updated; their failing-connection fixtures now reserve non-listening ephemeral loopback sockets instead of dialing fixed port9. Workloads, resource assertions, successful-turn checks and child reaping remain unchanged. Affected slice GREEN3/3,5.522s (`.local/tmp/w4-green-gate-expectations.log`).
- The fourth failure was unchanged auth05 waiting for `● Alpha`, the same observed PTY symptom as W2; its exact cause and any W4 connection are unproven. Unchanged isolated rerun PASS1/1,16.688s (`.local/tmp/w4-isolated-auth.log`); this is not a full-gate PASS. No auth/TUI code, fixture, assertion or deadline was changed.
- Confirming full gate FAIL: 2640/2641 passed, one failed,25existing skips,256.089s (`.local/tmp/w4-full-gate-confirm.log`). All corrected MCP cases and auth05 passed in this full run; the sole failure was unchanged VIS38 sampled thread maxima9/10. It passed the first full run4.292s, but the first unchanged isolated rerun also FAILED9/10,4.034s (`.local/tmp/w4-isolated-vis38.log`), initially leaving the owner's isolated-PASS condition unmet. These failures remain recorded; targeted resume evidence follows below. No third full gate or blind repeat was run. Test/TUI/sampler/baselines/deadlines/loads are unchanged.
- Completed remaining own checks: final full run's workspace fmt/strict Clippy PASS4.04s; separately executed skipped tail PASS (zero doctests, debug build4m48s, help/version), `.local/tmp/w4-gate-tail.log`. Release build PASS13m40s, `target/release/oc2`, `.local/tmp/w4-release.log`. First gate's19m33s was compilation, distinct from260.329s test execution; only UI07's existing31.034s<40s exception exceeded30s in the confirming test run.
- Final Sol review found one introduced capture-runner syntax defect: deleting the exception assignment left an empty Python `if`. AST parsing reproduced `IndentationError` (`.local/tmp/w4-red-capture-syntax.log`); removed the empty conditional/comment, not a `pass` placeholder. The same actual-source AST check is GREEN (`.local/tmp/w4-green-capture-syntax.log`); Rust/source contracts and the release binary are unchanged by this script-only fix. Same Sol bounded follow-up CODE REVIEW PASS, session `ses_e5d433573ffePz2TFCfQFa4Zhb`, with no remaining implementation findings; explicitly not an R3 waiver.
- Resume: owner authorized bounded targeted diagnosis, concrete thread attribution and completion if current unchanged evidence resolves R3; any proposed measuring correction must be escalated before editing. HEAD and the reviewed73-path W4 implementation remain unchanged; foreign `progress/` and W5 are untouched. Existing target binaries were absent in this resumed environment (not removed by this agent). Scoped `nextest list -p oc2 --test pty_t39 --locked --message-format json` rebuilt the unchanged test target in7m19s; no clean or target-directory override.
- Targeted attribution: genuine stacks (`stack-00-owner-3211127.log:61–109`) identify existing Tokio blocking-pool timed waits and storage-writer `block_in_place` core handoffs, not new W4 workers. The independent-process maximum counts idle retained workers too; heavy stack observation demonstrably made them retire at the unchanged10s keepalive and is excluded from acceptance. Fast per-phase and parent-only tracing controls record counts/syscalls; fast post-sample stack attempts105/110 failed after process exit and supply no attribution. The final no-debugger/no-tracer lifetime control directly observed owner4→5 in BOTH fixed-code fixtures with added/retained timed waits. Asynchronous client/owner records are not claimed to be simultaneous9/10 pair snapshots. Details/PIDs/timings/stacks and limitations are in `.local/tmp/w4-vis38-attribution.md`; exact missing historical TIDs cannot be recovered from old logs. No W4-created worker/leak was found, and no sampler correction was proposed or implemented.
- Current unchanged isolated execution: exact `target/debug/deps/pty_t39-7901dc61bce27084 --exact lifecycle::vis38_pty_dcp_equal_active_archive_resource_samples --nocapture --test-threads=4` PASS1/1,3.85s, original peak10/10 (`.local/tmp/w4-vis38-lifetimes-test.log`). A separate observer only read fixture-owned `/proc` metadata; all product TracerPid values were0, with no breakpoint/stack attachment/preload/source/oracle/workload change. All8 recorded fixture product PIDs exited normally. This disclosed observer is not a claim of zero host scheduling influence. First-full PASS plus this current unchanged isolated PASS satisfy the owner's existing qualified-acceptance rule, verified by the same Sol; neither failed full script is relabelled PASS.
- Restored current release artifact after resume's absent target directory: `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release` PASS13m31s, `target/release/oc2`, `.local/tmp/w4-release-resume.log`. No source/schema/dependency change or clean was needed for the targeted diagnosis.
- Same Sol evidence follow-up accepted R3 qualification under the existing rule, not a new waiver. Its two precision corrections (failed fast stack attachments and asynchronous observations) are incorporated above and in the ignored attribution artifact; current implementation digest is unchanged. Final bounded follow-up PASS confirmed the corrections and R3 status. No live/paid/visual or whole-product readiness is claimed.
- Publication complete: reviewed73-path commit `d907ca4bb7d2ae9bc3f3f86d9d6c69359cd2d4d9`, `fix(network): align fetch and proxy routing with TS`, pushed successfully to `origin/master`. The closure check confirmed reviewed scope/hash, approved origin, unchanged AGENTS/W5 and no foreign or diagnostic files. Only this evidence/status completion follows; no behavior changed after the last review.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.
- 2026-11-03: redirect policy is captured deny-only, as TS `assertNotDenied`: redirect Ask does not prompt or become Deny. Native shared fetch uses the explicitly requested ten-redirect limit; TS Code Mode's separate twenty-hop limit is not added without a requirement.
- 2026-11-03: removed the obsolete fetch exception parameter and all direct Runtime/ToolContext/test consumers rather than retaining ignored flags. The 40-ms held-DNS test now injects its pending resolver through the existing test-only prepared-client seam; its deadline/assertions and production total deadline are unchanged. Test runs clear inherited proxy variables to keep unrelated loopback fixtures offline; N3 proxy cases set explicit synthetic values only in isolated child processes.
- 2026-11-03: N3's “as TS” shorthand is scheme-specific: `WSS_PROXY → HTTPS_PROXY → ALL_PROXY`, `WS_PROXY → HTTP_PROXY → ALL_PROXY`, including uppercase/lowercase priority, TS automatic loopback bypass and port-aware WS NO_PROXY. The cited source and its `ws:` + HTTP_PROXY test require this interpretation; HTTP clients retain the specified installed hyper-util system matcher, whose NO_PROXY semantics are not silently substituted for WS's.
- 2026-11-03: under the owner's explicit locked-crate tunnelling exception, N3 promotes existing tokio-rustls 0.26.5 from dev-only to runtime and directly uses already-locked rustls-native-certs 0.8.4. They are needed for HTTPS proxy TLS with the same native roots as the existing tungstenite origin connection. No new package/version is introduced. Reqwest Upgrade alone cannot provide plain-WS CONNECT or preserve the direct pre-credential peer check; the installed Hyper Tunnel alternative adds connector/Tower/Hyper-IO boundaries and is not simpler. One nested MaybeTlsStream socket keeps the existing handshake/delivery owner. Focused Sol plan review accepted this interpretation and dependency scope; final implementation review remains required.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.

## Completion
- Resolved outcomes: R1–R4 verified, with R3 explicitly qualified under the existing owner rule rather than whole-script PASS.
- Commands and artifacts: `.local/tmp/w4-review-files.txt` lists73paths, `.local/tmp/w4-review-artifacts.txt` lists ignored task evidence; exact commands/results and logs are recorded above. Latest reviewed non-goal content digest `329ef4eccac9b2f10baa4b36dde17740ee61150d369dc3f74b3037d16b437a1d` is unchanged by this status-only update.
- Constraint and diff-scope check: no schema/new package/version, only justified locked TLS dependency edges; foreign `progress/`, other waves and historical evidence untouched. Reviewed73-path implementation and actual main commit/push verified. Completion-only status/evidence requires no behavioral retest or new code review.
- Final status: complete; no remaining outcome or next action.
