# Goal: Hot reload TS parity (MCP, config, CLI, TUI consumers)

Status: complete
Source: user instruction 2026-11-02 («hot swap mcp и конфигов как в oc2 ts… полный паритет этой логики»; incident: агент добавил MCP `4pda` в конфиг на ходу, Rust потребовал рестарт owner); approved plan v3 (Plan-Opus session, Review-Sol approved); user: «делай копию плана в цель и приступай к реализации и коммит пуш, так же если будут падать тесты или ещё какие баги по пути - чинить».
Last updated: 2026-11-02

Baseline HEAD: `d22adb1000f64da9f352a918069a0e82de55c6af` (master). Foreign untracked `progress/` — не трогать.
Reference (historical, not execution): [2026-10-14-hot-reload-shared-service](2026-10-14-hot-reload-shared-service.md). TS oracle: `.local/opencode` (`packages/core/src/config.ts`, `config/watch.ts`, `mcp/index.ts:103-104,586-742`, `session/context.ts:126-183`, `packages/tui/src/config/index.tsx:345-369`).

## Objective
В работающем native owner создание/изменение/удаление watched config/definition/CLI-источников применяется автоматически (как в TS OpenCode), без `/reload` и рестарта; сбои reload видимы и не останавливают Location; TUI-потребители не теряют состояние.

## RECON facts (не перепроверять без новых данных)
- Автоматический путь watch → candidate → publication → MCP reconcile работает в production: изолированный real-binary PTY прогон (global in-place add, atomic disabled→enabled, project `opencode.json`) — все Connected ≈5s без `/reload`.
- Вероятный механизм инцидента (не доказан): выбранный dynamic provider с `catalog_status == Failed` + любое semantic изменение → `retain_catalog_reuse` переносит Failed (`composition/reload_inputs.rs:47-69,90-133`) → нет rediscovery (`provider_readiness.rs:450-460`) → `application/reload.rs:204-236` отвергает весь candidate; ошибка хранится в `Coordinator.failure` и нигде не читается.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied. Падающие по пути tests/баги, вызванные diff или блокирующие обязательные gates, чинить (указание пользователя).

## Frozen Contract

### Required Outcomes
- R1: MCP startup grace.
  - Acceptance: новый/включённый MCP попадает в следующий fresh request того же turn, если подключился в пределах 2s от старта попытки; иначе — в первый fresh request после Connected. Request ждёт только попытки, начатые до него (`start <= requested_at`), каждую до settled или `start+2s`; supervisor не блокируется; ожидание отменяемо; disable/remove ожидаемого сервера снимает ожидание сразу; нет busy loop.
   - Primary evidence: unit tests `runtime/mcp/lifecycle/tests.rs` (in grace incl tools; ≈2s timeout; staggered attempts без busy loop; post-request attempt не продлевает; disable/remove while waiting; cancel); actual-binary `crates/oc/tests/mcp_application/lifecycle.rs`: held turn, gated builtin-tool provider response, запись global `opencode.jsonc` с enabled mcp10 + initialize gate, `wait_counter(...,"initialize",1)`, release provider, release initialize в пределах grace, непосредственно следующий non-title request содержит MCP tool, без `/reload`.
   - Status: verified
   - Evidence: final cutoff version: adapter `startup_grace` slice PASS2 (2.54s); actual-binary `lifecycle_hot_reload_startup_grace_includes_mcp_in_immediate_continuation` PASS1 (0.59s), no fallback rounds or `/reload`; adapter all-targets Clippy and Sol cutoff review PASS.
- R2: Reload failure visibility.
  - Acceptance: ошибка candidate (auto и explicit) видна как TUI service issue, с деталями в `/settings`, включая Provider-kind диагностики, не представленные текущей readiness; снимается после следующего успешного reload; last-good сохраняется. Без изменений wire.
   - Primary evidence: `application/tests.rs:881-936` расширен (issue + ProviderChanged, cleared on recovery); тест provider-switch failure + TUI `/settings` row.
   - Status: verified
   - Evidence: continuous-source malformed/recovery regression PASS; settled-provider application regression PASS1 includes forced/switch failure last-good + issue; Settings details/duplicate suppression PASS in `oc-tui --lib app::` (318 tests).
- R3: Unchanged failed provider не блокирует публикацию.
  - Acceptance: Failed-rejection в `reload::publish` пропускается только при `retained_selected && next.catalog.provider == composition.catalog.provider` (`retained_selected`: состояние выбранного provider перенесено `retain_catalog_reuse` с неизменными leaf+binding, non-forced, в любом carried state; carried Pending → существующий bounded discovery, при неудаче и том же ID → публикация с Failed). Forced `/reload` failing и новый/изменённый selected provider failing → reject, last-good, issue через R2.
   - Primary evidence: application tests (loopback 401/empty provider): Failed + add MCP → published, MCP connected, provider Failed; already-admitted failed B, switch A→B → last-good A + issue; carried Pending → candidate discovery fails → published с Failed и MCP marker; forced `/reload` failing → reject.
   - Status: verified
   - Evidence: `hot02_cli_and_mcp_edits_publish_with_unchanged_ready_or_failed_provider` PASS1 (1.02s); coordinator slice PASS3 (0.21s), explicitly retained Failed B rejected across IDs; `hot02_pending_provider_cli_edit_publishes_failed_candidate_and_retires_old_get` PASS1 (15.56s), including valid late successful GET.
- R4: Observer recovery.
  - Acceptance: падение observer task не убивает Location worker: `Ready::Rewatch` → abort+await build, вернуть waiters/force, `SourceWatch::shutdown` старого, `SourceWatch::start` + admitted rescan. При неудаче restart: все waiters получают `Err(CoreError::Diagnostic(d))`, pending/force/due/rearm сброшены, issue видна, очередь submissions не заблокирована; `start()` при отсутствующем watch и pending (explicit `/reload`) пробует снова. Build JoinError / follower closure остаются fatal.
  - Primary evidence: `application/reload/coordinator/tests.rs` (abort через `#[cfg(test)]` accessor → Rewatch → restart → later edit publishes; restart failure during active forced build → waiters Err, force cleared, submissions unblocked, next `/reload` recovers).
  - Status: verified
   - Evidence: `CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked application::reload::coordinator -- --test-threads=4`: PASS2 (0.19s), including restart, failed-restart force/waiter cleanup and explicit recovery.
- R5: `cli.json(c)` hot reload.
  - Acceptance: правка global/Location/`.opencode` `cli.json`/`cli.jsonc` автоматически применяет keybinds/chrome и `session.permissions` (новые операции — новый режим, приоритет `--auto`, issued операции не меняются) в работающем TUI; при settled (Ready/Failed) неизменённом provider нет rediscovery и MCP reconnect. При startup Pending допускается один дополнительный bounded discovery cycle (с существующими retries).
   - Primary evidence: observer test (canary `cli.json`→`tui.json`, cli.json(c) positive), `watch_inputs/tests.rs` adjust, `reload_inputs/tests.rs` cli-only not noop; application test cli.jsonc keybind auto edit (discovery GET counter unchanged, MCP not reconnected, также при failed unchanged provider, и при удерживаемом первом startup GET); `tui_cmd/tests/routing.rs` permissions обе стороны, `--auto`, late ACK.
   - Status: verified
   - Evidence: observer/watch-input/reload-input regressions PASS; both application CLI regressions above PASS (Ready/Failed reuse, Pending publication); `tui_cmd::` PASS182 includes serialized attachment permission ACKs in both directions, CLI override and Unsupported.
- R6: TUI consumers после автоматической публикации.
  - Acceptance: staged (не отправленный) agent draft восстанавливается Preview по ID на последней generation (теряется только при `AgentUnavailable` или изменении пользователем); Skills cache инвалидирован, открытая панель перечитывается, stale read не принимается; cached DCP refusal/availability сброшен.
   - Primary evidence: tests: shared adoption раньше/позже Provider preview; G2→G3 при удерживаемом Preview; removed agent; user change wins; Skills G1 read before/after adoption → reopen fetches G2, open panel refetch; DCP cached CompressOff → toggle `compress.enabled` без изменения display chrome → `/dcp-compress` доходит до owner.
   - Status: verified
   - Evidence: `cargo test -p oc2 --bin oc2 --locked tui_cmd:: -- --test-threads=4` PASS182 (3.52s): both adoption orders/latest Preview/user revision, Skills fencing/refetch, DCP owner dispatch, G10→new-owner G1 with newer pending intent preserved. `oc-tui --lib app::` PASS318. Final Sol correction review PASS; original owner-incarnation P1 resolved.
- R7: Docs.
  - Acceptance: TEST_PLAN `## Hot reload` (1-3 строки), `crates/oc-adapters/assets/skills/customize-opencode/SKILL.md` (:49, :71), GOAL.md указатель; описаны поддерживаемое подмножество и divergences (ниже).
  - Primary evidence: diff docs.
  - Status: verified
  - Evidence: listed docs updated; `python3 scripts/check_docs.py` and `git diff --check` PASS. Builtin skill metadata/body consumer remains unchanged.

### Constraints
- C1: AGENTS.md: 4 потока (`CARGO_BUILD_JOBS=4`, `--test-threads=4`), cargo не параллельно, без `cargo clean`/`CARGO_TARGET_DIR`; не трогать running owner/клиент пользователя; тестовые проекты вне worktree, `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`.
- C2: Issued request/approval/full batch сохраняют captured authority; child launch ceilings не расширяются; trust/config fail-closed; diagnostics payload-free; native protocol VERSION не меняется.
- C3: Bounded resources, cancellation/cleanup, no unknown-effect replay.

### Non-goals (deliberate divergences)
- Child, запущенный до добавления MCP, его не видит (security ceiling).
- Не загружаемые вообще: config `instructions` list, `.claude`/`.agents`, project-root agent/command/skill dirs.
- Theme/`tui.json`; `tabs.scope` уже открытого session picker.
- Как в TS: нет auto-retry unchanged Failed MCP; in-flight call на заменяемом соединении может упасть; owner env фиксируется при старте; debounce-гонка 100ms на немедленно следующем шаге.
- Скомпилированные plugins: новый код — только с новым binary.
- Перенос in-flight ProviderWork через публикацию.

## Change Envelope
- Target/paths: `crates/oc-adapters/src/runtime/mcp/lifecycle.rs` (+tests); `application/reload.rs`, `application/reload/coordinator.rs` (+tests), `application.rs` (`snapshot_catalog`), `composition.rs` (`reload_issue` at :114, :1747), `composition/watch_inputs.rs`, `composition/watch_inputs/observer.rs` (test accessor, tests), `composition/reload_inputs.rs` (+tests); `crates/oc-tui/src/app/live.rs` (`apply_catalog_inner`), `app/input.rs` (`/settings` Provider rows); `crates/oc/src/tui_cmd/{catalog_jobs.rs,shared.rs,reload_jobs.rs,resource_jobs.rs}`, `tui_cmd.rs`; tests в `crates/oc/tests/mcp_application*`, `crates/oc/src/tui_cmd/tests/`, `crates/oc-tui/src/app/tests/`; docs listed in R7.
- Forbidden: new deps, wire/protocol changes, storage migrations, new services/tasks/threads вне существующих supervisors, ослабление tests/lints/deadlines.
- Direct consumer edits: `application/user_shell.rs` (R4 coordinator signature); `oc-tui/src/app.rs` + `app/model_selection.rs` (R6 logical draft state/reconstruction); `tui_cmd/control_jobs.rs` (R5 shared attachment registration and existing user mutations must serialize native effects).
- Required-gate consumers: `application/{fatal_tests,plugin_tests,provider_tests}.rs` and `application/tests.rs::review_tests` compare entire catalogs. R2 intentionally adds one typed issue; update those expectations to assert that exact issue and retain complete last-good equality for everything else.
- Required-gate cleanup: `application.rs::off_queue` releases each completed read's shared Db handle before ACK. Gate `go05_qualified_child_pins_overrides_and_commands_capture_own_authority` hit DataRootBusy reopening immediately after completed ChildJobs + shutdown/join; no sleep/retry/lock bypass or schema change.

## Current Checkpoint
- Closed: R1–R7. Substantive implementation/verification complete; delivery by ordinary commit + push to `origin master`.

## Current State
- Resolved: R1–R7, current Sol review PASS, current-source release build PASS (12m21s). Once-only `scripts/test.sh`: 2604/2611 PASS, 7 failures, 25 skipped (252.653s tests; cold compilation excluded). Six R2 catalog expectations and completed-read lease cleanup repaired; exact failed slice PASS7 (0.621s), no retries or suite replay. Remaining fmt/Clippy/doctest/debug-build/help/version phases PASS.
- No substantive next action. Only >30s test was the documented ui07 discovery exception (31.087s ≤40s). No live/visual qualification claimed.

## Material Decisions
- 2026-11-02: plan v3 одобрен пользователем; R5 сужен до settled provider (без переноса in-flight ProviderWork).
- 2026-11-02: R4 coordinator `start` signature also updates its direct consumer `application/user_shell.rs`; R5 attachment registration serialized in existing `tui_cmd/control_jobs.rs` with user permission mutations, avoiding out-of-order native effects (no new supervisor).
- 2026-11-02: Sol found new monotonic generation fences crossed native-owner re-election (G10 → G1). Reattachment now rebases view/permission fences, fences retired catalog receipts, joins retired metadata reads and refreshes catalogs while preserving logical staged agent; no wire incarnation field added.
- 2026-11-02: R1 uses one clock cutoff for waiter release and startup-timer admission; separate samples could cross an expiry and omit both release and wakeup. Existing bounded-grace regressions reused, no test-only scheduling hook.
- 2026-11-02: Full gate exposed six intentional R2 snapshot-contract changes and a completed-read ACK/Db-drop race. Preserve exact last-good assertions after checking the added typed issue; release off-queue read leases before ACK, using existing failing owner-reopen regression without timing hooks/retries.

## Checkpoint History
- 2026-11-02: goal created from approved plan v3.

## Completion
- Resolved outcomes: R1–R7 verified; Sol's owner-incarnation P1 fixed, pending intent and timer-cutoff corrections accepted, final gate-recovery code review PASS.
- Commands and artifacts (all Cargo runs serial, `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, `CARGO_BUILD_JOBS=4`; Nextest/Rust harness threads 4):
  - `cargo test -p oc-adapters --lib --locked startup_grace -- --test-threads=4`: PASS2; `cargo test -p oc2 --test mcp_application --locked lifecycle_hot_reload_startup_grace_includes_mcp_in_immediate_continuation -- --test-threads=4`: PASS1.
  - `cargo test -p oc-adapters --lib --locked hot02_cli_and_mcp_edits_publish_with_unchanged_ready_or_failed_provider -- --test-threads=4`: PASS1; `cargo test -p oc-adapters --lib --locked application::reload::coordinator -- --test-threads=4`: PASS3.
  - `cargo test -p oc-adapters --lib --locked hot02_pending_provider_cli_edit_publishes_failed_candidate_and_retires_old_get -- --test-threads=4`: PASS1; `cargo test -p oc2 --bin oc2 --locked tui_cmd:: -- --test-threads=4`: PASS182; `cargo test -p oc-tui --lib --locked app:: -- --test-threads=4`: PASS318.
  - `bash scripts/test.sh` executed once: initial FAIL7 (2604 PASS, 25 skipped), not reported as a clean script PASS. Recovery used `.local/tools/cargo-nextest nextest run -p oc-adapters --lib --locked --no-fail-fast -E 'test(=application::fatal_tests::cfg10_optional_definition_failure_is_safe_but_policy_document_is_fatal_and_atomic) | test(=application::plugin_tests::cfg09_plugin_requests_are_safe_independent_and_generation_pinned) | test(=application::provider_tests::ui07_missing_selected_credential_keeps_local_owner_and_refuses_before_acceptance) | test(=application::tests::provider_view_tests::go05_qualified_child_pins_overrides_and_commands_capture_own_authority) | test(=application::tests::review_tests::conversation_shortcuts_follow_admitted_config_location_and_reload) | test(=application::tests::review_tests::terminal_copy_and_animations_follow_global_location_switch_and_reload) | test(=application::tests::review_tests::tool21_failed_output_reload_keeps_published_generation_and_safe_diagnostics)'`: PASS7.
  - Final `cargo clippy -p oc-adapters --all-targets --locked -- -D warnings`, `cargo fmt --all -- --check`, `cargo test --workspace --locked --doc --no-fail-fast -- --test-threads=4`, `cargo build -p oc2 --bin oc2 --locked`, `target/debug/oc2 --help`, `target/debug/oc2 --version`: PASS. Earlier scoped TUI/oc2 Clippy and initial workspace/all-targets Clippy PASS.
  - Final `cargo build -p oc2 --bin oc2 --locked --release`: PASS; artifact `target/release/oc2`. `python3 scripts/check_docs.py`, `git diff --check`: PASS. Local bounded logs: `.local/tmp/hot-reload-*.log` (not committed).
- Constraint and diff-scope check: frozen outcomes satisfied; no deps/schema/wire/version changes, no authority/child-ceiling widening, no unknown-effect replay, no running user owner/client restart. Foreign `progress/` untouched. Mandatory-gate direct consumer/cleanup additions recorded above. Final reviewed tracked diff SHA-256 (before adding this goal): `964a665ca7117c6e3bcab12382c9259a9ac8497a49c8814c113fc7de19042170`.
- Final status: complete — offline/unit/composed/isolated actual-binary evidence, not paid live, visual, cross-user or whole-product READY.
