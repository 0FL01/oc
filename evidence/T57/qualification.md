# T57 — current offline qualification and headless key sources

Date: 2026-10-06. Base: `45864ce44`. Checked test-only slice; no production
behavior, dependency, scope, threshold or existing assertion was weakened.
Real AUTH06 authorization remains separate and NOT_RUN.

## Original AUTH05 source proof

The non-default `auth-fixture` ELF now executes headless native read/result/final
with **own OPENAI_API_KEY** and **configured apiKey**, each in a separate protected
data root with zero stored credentials. The same existing SQLite/public-cache,
credential resolver, captured native channel and runtime owners execute the request.
Each case settles one read, remains default WS (no HTTP), and creates no account.
The earlier stored Key/OAuth roots and issuer approval cases remain unchanged.
All material is synthetic and local; this does not prove real OpenAI acceptance.

Current fixture counters: **13 WS frames / 6 HTTP model requests**, two token
exchanges and four device controls. All local model output caps are <=2048 and
the local received-request total is <=24. These fixture counters are not a live
campaign or proof of a pre-dial live accounting guard.

## Menu partial-write diagnosis

The cold normal-release campaign first failed an existing CLI fixture assertion:
it stopped draining after the method title, although native `select` flushes each
menu row separately. The title was not a receipt for the last choice. The fixture
now waits for `3: API key` within the **same eight-second deadline**, retains the
original Browser/Device assertions, and only then sends the selection. Production
input/method logic and all security assertions are unchanged. Normal release and
the current workspace actual CLI target subsequently pass.

## Current gates

One serial chain, approved TMPDIR, CARGO_BUILD_JOBS=3, RUST_TEST_THREADS=2,
normal stacks; no parallel Cargo, timeout increase, ignore or failing-test disable:

| Command | Result |
| --- | --- |
| `cargo build --locked --release -p oc` | Exit0, normal production ELF. |
| `python3 -B crates/oc/tests/support/auth_cli.py target/release/oc` | PASS, normal release pipes/PTY/key/accounts/owned browser controls. |
| `python3 -B crates/oc/tests/support/auth_tui.py target/release/oc` | PASS, normal release TUI methods/accounts/picker/no commit/open-copy/cancel/restart. |
| `cargo test --locked --workspace` | **1711 passed / 0 failed / 11 unchanged opt-in ignored**, 46 result records; includes normal debug actual CLI/TUI, all prior runtime/storage/security/recovery/PTY gates. The default auth-fixture-only target correctly runs zero tests. |
| `cargo build --locked` | Exit0, normal debug ELF. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0. |
| `CARGO_TARGET_DIR=target/auth-fixture cargo build --locked --release -p oc --features auth-fixture` | Exit0, explicitly non-default fixture ELF. |
| `python3 -B crates/oc/tests/support/auth_oauth.py target/auth-fixture/release/oc` | PASS, fake approval/native Key/OAuth WS/HTTP/reopen and separate headless env/config sources. |
| `CARGO_TARGET_DIR=target/auth-fixture cargo clippy --locked -p oc -p oc-adapters --all-targets --features oc/auth-fixture -- -D warnings` | Exit0. |
| `cargo fmt --all -- --check`, normal debug/release `oc --help`, `git diff --check` | Exit0. |

Local log `/home/opencode/.cache/opencode-tmp/opencode/t57-current-all-offline-final2.log`
ends `T57_CURRENT_ALL_OFFLINE_GATES_PASS` only after every command succeeded. The
46-result aggregate was independently checked as 1711/0/11. Missing old build
artifacts were rebuilt; neither the earlier failed chain nor stale artifacts were
used as current final proof. No live request or credential import occurred.

## Boundaries

Normal production binaries retain fixed native OpenAI/Codex issuer/routes and all
DNS/peer/redirect/retry guards. The fixture feature is explicit, numeric loopback
only, fail-closed and never a real authorization claim. Current independent offline
proof is complete; AUTH06 still requires dedicated owner-operated browser and device
confirmation plus an explicitly permitted ordinary OpenAI test key/request/reopen.
No such operator/test-input authorization was supplied, and no authoring/user/runner/
donor/browser credential search or Go/OpenProxy substitution was attempted. Next is
the factual live-preflight/blocker assessment, not a fake-only T57 finish or VIS45 PASS.
