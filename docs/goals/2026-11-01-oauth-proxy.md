# Goal: OAuth Codex через HTTP(S)-прокси

Status: active
Source: запрос пользователя «OAuth-клиент oc2 отключает прокси через .no_proxy()… реализовать поддержку прокси для OAuth HTTP», утверждённый план (2026-11-01), «Реализовать и коммит пуш».
Last updated: 2026-11-01

## Objective
Browser code exchange, device login и refresh OAuth в `crates/oc-adapters/src/auth/openai.rs` идут через env HTTP(S)-прокси с учётом NO_PROXY, сохраняя TLS-проверку issuer; изменения закоммичены и запушены в `origin/master`.

## Execution Directive
Complete the frozen Required Outcomes using the listed Change Envelope and Primary Evidence. Work on the smallest unresolved outcome. Do not add requirements from reviews, tests, tools, speculative risks, or optional source text. Finish when every required outcome is resolved and affected constraints remain satisfied.

## Frozen Contract

### Required Outcomes
- R1: OAuth POST (browser exchange, device usercode/poll/exchange, refresh) использует прокси из первого непустого `HTTPS_PROXY`, `https_proxy`, `ALL_PROXY`, `all_proxy`; `NO_PROXY`/`no_proxy` совпадение → direct; схемы `http://`/`https://`, URL-encoded Basic auth.
  - Acceptance: offline regression с fake CONNECT-прокси (loopback, test-only CA, сертификат `auth.openai.com`, issuer = `ISSUER`) проходит: CONNECT `auth.openai.com:443`, SNI, OAuth paths, Proxy-Authorization, env precedence, NO_PROXY.
  - Primary evidence: `cargo test -p oc-adapters --lib --locked auth::`
  - Status: verified
  - Evidence: `CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked auth:: -- --test-threads=4` — 17 passed; regression `oauth_proxy_env_connect_and_preflight` проверяет HTTP/HTTPS CONNECT, issuer/proxy SNI, точный Basic header, отсутствие его утечки issuer, OAuth paths и env precedence/NO_PROXY через отдельные процессы.
- R2: Некорректный/неподдерживаемый proxy URL или запрещённый адрес прокси (link-local) → `AuthError::Authority`, без direct fallback, без отправки.
  - Acceptance: regression tests отказа.
  - Primary evidence: тот же `auth::` срез.
  - Status: verified
  - Evidence: тот же regression: malformed URL, SOCKS scheme и credential-bearing link-local proxy отклоняются как Authority до отправки, без proxy connections/direct fallback.
- R3: Отказ preflight в `resolve_at` не оставляет `refresh_pending` (preflight до `begin_credential_refresh`, отправка после через `send_tokens`).
  - Acceptance: regression test; существующие coalescing/ABA/quarantine тесты зелёные.
  - Primary evidence: тот же `auth::` срез.
  - Status: verified
  - Evidence: тот же regression проверяет `refresh_pending == false` после отказов `resolve_at` и `renew_captured`; coalescing/ABA/quarantine, browser PKCE и device polling проходят в auth-срезе. Binary OAuth fixture — 1 passed.
- R4: Документация OAuth proxy в `docs/CONFIG.md`, `docs/SECURITY.md`.
  - Status: verified
  - Evidence: оба документа описывают только OAuth HTTP, env precedence/NO_PROXY, схемы/Basic, TLS и environment первого native owner с необходимостью штатного restart; generation HTTP/WS явно исключены.
- R5: Commit + push в `origin/master` только своих файлов.
  - Status: in_progress
  - Evidence: origin/master read-only проверен; осталось закоммитить только свои paths и push, затем записать фактическое completion evidence.

### Constraints
- C1: TLS hostname/cert verification, фиксированные paths, no redirects, no reqwest retries, deadlines, body limits сохраняются.
- C2: Direct-маршрут: прежние issuer admission, DNS pinning, public-address policy.
- C3: Proxy-маршрут: issuer локально не резолвится; адрес прокси admit/pin через trusted `EndpointBinding` (loopback/RFC1918/ULA ok, link-local deny); credentials убираются до admit.
- C4: `renew_captured`: только `Remote` → `Preflight`; `Authority` → `Fatal`. После reservation — no replay, quarantine.
- C5: Чужой diff (`docs/TEST_PLAN.md`, `scripts/test.sh`, `progress/`, `scripts/test_test_runner.py`) не трогать и не коммитить.

### Non-goals
- Генерация (`provider.rs`, WebSocket) через прокси; SOCKS/PAC/custom CA; browser launcher; расширение бинарного fixture `crates/oc/tests/auth_oauth.rs`; live OAuth; перезапуск работающего owner.

## Change Envelope
- Target: `crates/oc-adapters/src/auth/openai.rs` (`post`, `check_peer`, `send_tokens`, `resolve_at`), `auth/attempts.rs::device`, `auth/openai/tests.rs`, `crates/oc-adapters/Cargo.toml` (`hyper-util` 0.1.20 `client-proxy` direct; `tokio-rustls` dev), `Cargo.lock` без повышения версий, `docs/CONFIG.md`, `docs/SECURITY.md`, этот файл.
- Forbidden: новые retry-циклы, direct fallback, общая proxy-поддержка продукта, refactor `EndpointBinding`.

## Current Checkpoint
- Closes: R5
- Smallest next action: актуальное Review-Sol по итогам проверок, commit/push своих файлов, затем doc-only completion commit/push с фактическим SHA реализации.

## Verification
Все тестовые команды выполнены последовательно с `TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`; один Cargo owner, 4 build/test threads.
- `CARGO_BUILD_JOBS=4 cargo test -p oc-adapters --lib --locked auth:: -- --test-threads=4` — PASS, 17 tests, 2.87s execution.
- `CARGO_BUILD_JOBS=4 cargo test -p oc2 --test auth_oauth --features auth-fixture --locked -- --test-threads=4` — PASS, 1 test, 12.09s execution.
- `CARGO_BUILD_JOBS=4 cargo clippy -p oc-adapters --all-targets --locked -- -D warnings` — PASS.
- `cargo fmt --all -- --check`; `git diff --check` — PASS.
- `CARGO_BUILD_JOBS=4 NEXTEST_TEST_THREADS=4 RUST_TEST_THREADS=4 bash scripts/test.sh` — выполнен один раз: workspace Clippy PASS; nextest 2575 passed, 4 failed, 25 skipped, 251.88s execution. Из-за nextest failure последующие doctests/debug smoke шаги script не выполнялись. Это НЕ full-gate PASS.
  - PTY VIS38 threads sample 10→9; GO05 child-exit timeout; MCP sidebar fold timeout. После чтения waiting/cleanup механизмов каждый failed slice отдельно прошёл без изменения assertions/deadlines. Эти fixture paths не вызывают OAuth POST/refresh; причина исходных timing failures не установлена.
  - `files::search::tests::repository_root_query_retains_actual_preview_locations_with_native_caps` повторно FAIL отдельно: discovery entries 10001 > 10000; source/test неизменены, OAuth не вызывается. Существующий `evidence/` содержит 16401 filesystem entries; свои artifacts находятся только в игнорируемой `.local/tmp`. Соседние search budgets/fixtures и чужие artifacts не менялись. Это ограничение данного worktree, не подтверждение clean-HEAD baseline PASS.
- `CARGO_BUILD_JOBS=4 cargo build -p oc2 --bin oc2 --locked --release` — PASS; binary `target/release/oc2`.
- Live OAuth/visual qualification и restart пользовательского owner не выполнялись; schema/migrations не менялись.

## Current State
- R1–R4 verified; R5 awaiting commit/push.
- Ограничение: полный offline gate остаётся красным вне изменённого OAuth пути; отдельные успешные повторы не заменяют его PASS. Логи проверок — `.local/tmp/oauth-proxy-*.log` (ignored).
- Foreign tracked diff побайтно сохранён; новые foreign paths не тронуты. Cargo.lock: только две dependency entries, без version bumps.

## Checkpoint History
- 2026-11-01: план утверждён; два Review-Sol плана без блокеров (уточнена формулировка про metadata: гарантируется только link-local deny).
- 2026-11-01: R1–R4 реализованы и проверены; source Review-Sol без дефектов, финальная verification-оценка запрошена после scoped checks, одного full-gate запуска и release build. Смежные gate failures не расширяют OAuth scope.

## Completion
- Final status: active; commit/push evidence pending. OAuth primary evidence PASS; full offline gate не PASS.
