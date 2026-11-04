# Goal: W5. Config/trust/discovery (HIGH) — меняет AGENTS.md «Config — OC2 allowlist, fail-closed» → «неизвестные ключи игнорируются с diagnostic»

Status: active
Source: user instruction 2026-11-03 — «паритет с oc2 TS без бесполезных сейфгард костылей»; «делай копию плана в цель и начинай процесс итеративного лечения, потом коммит пуш; не забыть актуализировать документацию, чтобы не было дивергенции документации и исходников». Plan v2 reviewed by 3× Review-Sol (wave W5 of W1–W5).
Last updated: 2026-11-04

## Objective
Wave W5: every item below either reproduced (RED test at the owner, or live repro vs TS 2.2.5) and fixed to TS-reference parity (`.local/opencode`, opencode 2.2.5) with GREEN regression test, or closed `not_applicable` with evidence that it does not reproduce. Docs that describe the changed behaviour match the code.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: Each item of the wave section below (with its listed owner, fix, tests): reproduce → fix → GREEN; or `not_applicable` with evidence. Track per item in Current State.
  - Status: verified; K1–K4 reproduced and passed owner regressions and relevant compatibility checks below.
- R2: Docs (SECURITY.md, CONFIG.md, TOOLS_MCP.md, DECISIONS.md, CONTRACTS.md, ARCHITECTURE.md, CODE_MAP.md, TEST_PLAN.md, AGENTS.md, builtin `customize-opencode` SKILL.md — whichever describe touched behaviour) contain no statement contradicting the new code. Historical goals/evidence are not rewritten.
  - Status: verified; current contracts/manual/source attribution aligned and independently reviewed.
- R3: Checks pass: affected cargo test slices + scoped clippy `-D warnings`; `bash scripts/test.sh` once (one confirming run after diff-obsolete test corrections); release build + live rig where the wave has a live repro; Review-Sol PASS on the final diff. Under the continued owner acceptance rule, unchanged flaky cases require both another full-run PASS and an unchanged isolated PASS; diagnose failures without blind loops or weakening their oracle.
  - Status: verified with the existing qualified-acceptance rule, not whole-script PASS. Current review-fix evidence is affected TUI13/13, strict Clippy, release and final Sol CODE REVIEW PASS; historical full-gate failures remain recorded below.
- R4: Commit + `git push origin master` (no force).
  - Status: in_progress; publication is the only remaining outcome.

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
- Smallest next action: closure check and ordinary commit/push of the reviewed W5 diff, then evidence-only completion and stop. General-Sol is the sole mutation/Cargo owner.

## Current State
- Start: HEAD `b11fe7d53f8e1fe1b8fcf4fb199adf9b56a8a468`, master, tracked/staged clean; foreign `progress/` untouched. W1–W4 complete; this is the final approved wave.
- K2 verified: existing composition/real-stdio fixtures first reproduced three behavioral failures (project `trust_refused`, filtered parent alias, missing inherited-token boolean), ten neighboring cases passed,0.476s (`.local/tmp/w5-red-mcp-env.log`; cold compile8m47s). Removed the credential-domain refusal/filter and obsolete fields/helpers/parameters; cwd/resource admission and live lifecycle/stderr protection remain. Scoped GREEN55/57,9.179s (`.local/tmp/w5-green-mcp-env.log`); only two stale credential-withholding expectations failed. Updated those mixed tests without weakening argv/cwd/deadline/stderr/cleanup assertions: affected recovery2/2,0.262s (`.local/tmp/w5-green-mcp-env-recovery.log`). Actual-binary project/global MCP, captured PATH resolution and overlay proofs GREEN2/2,0.701s (`.local/tmp/w5-green-mcp-env-binary.log`). Disabled project activation also connected, proved inherited canary via boolean and reaped both peers; label/status protections passed.
- K1/K4 RED: six owner/application regressions all failed behaviorally,0.088s (`.local/tmp/w5-red-config-capacity.log`): selected Build allowed an ignored legacy deny; unknown keys rejected direct provider assembly/startup or dropped definitions; nested request extras failed; skill capacity aborted startup. Tests require retained policy/order and known fields, visible startup/reload warnings, and fixed260skills /90×48KiB workloads at startup and reload. No production K1/K4 change preceded this RED.
- K1/K4 first owner GREEN6/6,0.644s (`.local/tmp/w5-green-config-capacity.log`): normalized policy before excess omission, retained definitions/provider/request siblings, visible startup+reload warnings, and both fixed skill capacity workloads at startup+reload. Agent-catalog completeness, known malformed fields/unsafe request combinations, instruction validation and independent lower-trust/child/Plan ceilings remain enforced; subsequent compatibility evidence is below.
- K1/K4 nearest-owner slice97/105 passed,16.450s (`.local/tmp/w5-green-config-owners.log`); all eight failures were obsolete unknown-key refusal expectations. Updated only those portions, retaining malformed policy/mode/color, reserved headers/body, order/provenance and sibling checks. Affected recovery9/9 passed0.046s (`.local/tmp/w5-green-config-recovery.log`), including TS agent first-key-position/native-rule precedence. Capacity-selected-agent completeness and invalid selected-profile tests passed in the105-case run; final gate/review evidence is below.
- K3 RED3/3,0.160s (`.local/tmp/w5-red-files.log`): all five public path/trim/large-file cases failed; admitted startup refused file references; disabled-MCP activation refused a symlink reference. Regressions require decoded quote/backslash/newline preservation, unreadable missing/directory/FIFO/permission-denied references becoming empty with safe warnings, startup+reload, and real activation/cleanup. Test evidence prints only case indices/booleans, never referenced contents.
- K3 first GREEN3/3,0.271s (`.local/tmp/w5-green-files.log`): public and pinned-source readers now allow the requested paths, follow reference links, trim without double-escaping and have no64KiB file cap. Unreadable regular/nonregular references become empty with deduplicated value-free typed config/service notes; successful disabled-MCP activation retains its warning in status and reaps the real peer. Source/root authority failures remain fatal, and all referenced values stay out of diagnostic evidence.
- Nearest composition/activation/source-watch compatibility:66/70 passed1.635s (`.local/tmp/w5-green-composition.log`). Preserved source/root pins and instruction/discovery no-follow checks; public/reference-link expectations now follow K3. Two lifecycle protection tests now use a genuinely invalid resolved header instead of an unreadable file to test failure/no-spawn and label masking; both recovered. Catalog tests exposed dropped successful unknown/file warnings in the actual listing consumer: separate RED2/2 (`.local/tmp/w5-red-catalog-warnings.log`), then shared safe reports/notes forwarded at that owner, catalog GREEN8/8,0.162s (`.local/tmp/w5-green-catalog-warnings.log`). Known malformed policy/metadata/controls and root-escape refusals remain.
- Actual-binary config/workspace and bare-PTY compatibility:20/21 passed5.881s (`.local/tmp/w5-green-config-binary.log`), including headless unknown warnings with usable provider/MCP siblings, linked references without disclosure, selected-profile/deny checks and config-root refusals. The sole failure awaited a session title from a sessionless Home fixture; corrected only its readiness marker to the actual Home renderer, preserving both5s deadlines and the workload. Affected PTY recovery1/1,0.135s (`.local/tmp/w5-green-config-pty-recovery.log`); clean quit, no model request and no referenced-content disclosure remain asserted.
- Current-doc alignment exposed one remaining K1 owning-parser refusal for excess `nativeFallbackLimits` keys. Extended its nearest mixed validation test first (RED1/1, `.local/tmp/w5-red-fallback-options.log`); config normalization now strips only its excess keys and the existing safe report warns, preserving known invalid-number/type/safety-margin checks and the internal closed type. Affected admission/provider/fallback GREEN8/8,0.033s (`.local/tmp/w5-green-fallback-options.log`).
- R2 current docs/source/manual/provenance aligned: AGENTS product clause now reflects excess-key warnings without relaxing lower-trust/child/Plan/no-replay; SECURITY/CONFIG/TOOLS/CONTRACTS/ARCHITECTURE/CODE_MAP/TEST_PLAN/DECISIONS and builtin manual describe K1–K4, with retained MIT/source attribution and explicit supersession of historical decisions. Historical goals/evidence unchanged. `python3 scripts/check_docs.py` PASS4examples/3baselines; `git diff --check` and full retained MIT notice comparison PASS.
- Scoped strict Clippy PASS1m07s (`.local/tmp/w5-clippy.log`) after one test-only octal-permissions spelling correction (same zero mode). No new dependency/package/schema; no live owner restart or foreign `progress/` action.
- First full gate FAIL2642/2649,25existing skips,242.079s (`.local/tmp/w5-full-gate.log`). Five obsolete expectations concerned unknown command/keybind/compaction refusal or unreadable reference trust; corrected only those assumptions while retaining policy rollback, safe bounded diagnostics, known invalid-field validation and real dispatch/continuation. Optional-definition failure now uses a genuinely invalid known subtask value rather than an ignored excess key. Affected library recovery4/4,0.228s (`.local/tmp/w5-green-gate-expectations.log`); binary prune-warning proof passed.
- The changed unreadable-file outcome also exposed a real manual-activation recovery regression: empty executable correctly failed validation, but the row no longer permitted retry after the file was repaired. Owner regression RED1/1,0.170s (`.local/tmp/w5-red-file-retry.log`); when activation has a file-unavailable note, its validation failure now offers explicit retry, which rereads/revalidates the same captured disabled template before effects. No automatic retry or malformed-source bypass. Application/lifecycle GREEN35/35,7.797s (`.local/tmp/w5-green-file-retry.log`), actual held-request/activation/restart binary GREEN1/1,1.539s (`.local/tmp/w5-green-file-retry-binary.log`); no-spawn, captured requests, source pins and reaping remain checked.
- Seventh gate failure was unchanged auth05's known Alpha-state PTY wait; unchanged isolated run PASS1/1,16.490s (`.local/tmp/w5-isolated-auth.log`), with no proven W5 cause or auth/test changes. This does not make the first gate PASS. Current scoped strict Clippy PASS2m02s (`.local/tmp/w5-clippy-final.log`).
- Confirming full gate remains FAIL2646/2649,25existing skips,250.531s (`.local/tmp/w5-full-gate-confirm.log`); all W5 regressions/recovered cases and auth05 passed. Only three unchanged PTY cases failed: sidebar fold/restart, control-card replay exit (both raw-VT checks passed), and vis25's initial reload-input wait. Each passed the first full run (0.891/0.594/1.291s) and its separate unchanged isolated run (0.955/0.542/1.295s, `.local/tmp/w5-isolated-sidebar.log`, `w5-isolated-controls.log`, `w5-isolated-reload.log`). Their exact timing causes and any W5 connection remain unproven; no fixture/helper/assertion/deadline/load changed. This meets the existing qualified-acceptance rule, not a successful whole-script run; no further full gate was run.
- Separately executed skipped tail PASS: zero doctests, debug build4m16s, help/version (`.local/tmp/w5-gate-tail.log`). Release build PASS12m31s, `target/release/oc2` (`.local/tmp/w5-release.log`). Real release headless proof PASS with isolated HOME/project/data and synthetic loopback model: safe excess-key warning, selected Build's legacy shell denial, no shell marker/grants, project Python MCP reports inherited-token/overlay booleans true, three requests and final continuation (`.local/tmp/w5-live.log`). First harness assertions used enum Debug spelling and denial state instead of actual public Display/failed refusal; inspected settled fixture effects before correcting only those expectations and using a fresh fixture. No raw referenced secret/environment dump; no remaining fixture product processes or user-owner signals/restarts.
- Final independent Sol review found one reachable K3 delivery gap: a successfully connected MCP retained its file warning in the DTO, but Services and Enter-details ignored Connected rows. Nearest TUI regression first failed with `(issue_count, details_visible)=(0,false)` instead of `(1,true)` (`.local/tmp/w5-red-warning-ui.log`). Those existing consumers now expose a connected diagnostic without changing Connected/Disconnect, pending handling, composer or connection effects; CONFIG/TOOLS docs clarify the controls. All13 nearest Services/MCP/selector tests PASS0.050s (`.local/tmp/w5-green-warning-ui.log`). No other implementation finding was reported.
- After the review fix: strict Clippy including the touched TUI owner PASS2m13s (`.local/tmp/w5-clippy-review.log`); final release build PASS13m07s (`.local/tmp/w5-release-final.log`), `target/release/oc2`. The headless proof's source path is unchanged; no duplicate live call or third full gate was needed under the affected-slice rule. Same Sol bounded follow-up CODE REVIEW PASS for the combined59-path version, session `ses_e5bae3125ffemNv4hIEhOzAW3R`, no remaining findings. This is not whole-gate or live/visual qualification.

## Material Decisions
- 2026-11-03: owner approved plan v2; TS parity overrides self-invented safeguards; docs must follow code.
- 2026-11-03: K2 full environment means the immutable captured parent `BTreeMap`, not a fresh process-env read at spawn/reload; configured overlay wins. Source/cwd admission and lifecycle/status/stderr secret collection remain independent of inheritance. Synthetic token proofs emit booleans only.
- 2026-11-03: K1 implements only the required policy-bearing legacy forms, not a general v1 migration. Root rule order is tools→permission→native permissions; legacy agent tools/permission retain TS's first-key-position merge before native rules. Native provider/MCP legacy domains remain warning+ignore. K4 warning-only omissions do not relax the existing agent-catalog completeness/selected-profile validation or instruction diagnostics.
- 2026-11-04: K3 resolves reference paths with TS-style lexical parent normalization before following links; `~/` uses captured HOME, not a fresh owner-environment read (missing HOME is an unreadable-reference warning). Existing admitted source/root authority remains pinned and missing/replaced authority is not downgraded into a file warning. Reference I/O itself may leave that directory, as explicitly required; regular-file/nonblocking admission avoids FIFO hangs. File warnings use the existing typed config/service diagnostic channels, including explicit disabled-MCP activation; no expanded path/content or raw OS error is published.
- 2026-11-04: preserve the existing explicit repair-and-retry activation path after unreadable references become empty. A file-unavailable activation note makes its effective-validation failure manually retryable, never executable without fresh validation; source/config malformation without that transient read warning remains unchanged.

## Checkpoint History
- 2026-11-03: plan v2 frozen into this goal.

## Completion
- Resolved outcomes: R1/R2/R3 verified with the explicit gate qualification above; R4 publication pending.
- Commands and artifacts: `.local/tmp/w5-review-files.txt` lists59 tracked paths; `.local/tmp/w5-review-artifacts.txt` lists ignored task evidence. Exact commands/results are recorded above. Reviewed58 non-goal path/content digest `de8ffaf200a7b9f06743c22656f485c013ecb87bb67482510cd843d5617f7f0a` remains unchanged by this status-only update.
- Constraint and diff-scope check: no new dependencies/schema/migrations, historical goals/evidence and foreign `progress/` untouched; source/status/stderr protection, selected-profile/child/Plan authority and no-replay retained. Publication uses only the explicit reviewed manifest.
- Final status: active pending commit/push, no implementation blocker or further audit/backlog work.
