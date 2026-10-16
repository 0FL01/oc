# T57 — native OpenAI auth CLI (checked consumer slice)

Date: 2026-10-06. Base: `29567936b`. Partial AUTH01/AUTH05 consumer proof,
not real OpenAI authorization, TUI parity, release qualification or AUTH06 PASS.

## Scope and shared ownership

`oc auth login [target] [--method ...] [--answer label=value]`, `list --format
default|json`, `logout [target] [credential]` and `switch [target] [credential]`
operate on built-in **openai** only. General/custom auth command execution remains
outside the frozen contract. This explicit integration always uses the admitted
canonical OpenAI credential namespace, not a same-named custom endpoint.

The CLI delegates to the existing `Db` credential methods and `OpenAiAttempts`;
there is no second credential store, import, auth.json, new attempt lifecycle or
generation/retry owner. `AuthScope::methods` supplies the same Browser/Device/Key
ordering and labels to CLI and Core/application consumers. Environment credentials
are not a selectable method.

Auth-only dispatch precedes runtime configuration/model/session/MCP startup and
generic startup tracing. No configuration generation, selected model, session or
paid request is required. The existing protected native data-root/SQLite owner and
its migrations/lock/recovery rules remain in effect.

## Input, acknowledgement and lifecycle

- Non-TTY login requires explicit `openai` and method; missing/foreign/unknown
  choices and secret `--answer` inputs refuse before opening storage. Only bounded
  non-secret label answers are admitted; Debug redacts answer values. No key/token
  argv flag was added.
- Key input requires stdin **and** stdout TTYs. One bounded ephemeral password
  buffer is masked, supports bracketed paste/backspace/clear, rejects controls and
  >16 KiB input, and restores raw/bracketed-paste modes on success/cancel/error.
  Success follows the SQLite acknowledgement, and explicitly does **not** claim
  server-validated authorization.
- Browser/device login uses the same real attempt owner. The active auth surface
  prints URL/instructions once and polls bounded status every500ms. SIGINT is
  registered before attempt publication; cancellation and every terminal/error
  exit join owner cleanup. Completion requires the durable local account receipt.
- Browser opening is gated on both TTYs and Browser method. Device never launches
  a browser/listener here. The explicit auth URL opener uses credential-free child
  environment plus bounded desktop context, no shell interpolation, and a bounded
  owned launcher. Failed opening leaves the manual URL available.
- Listing reads account metadata only, sorts labels then IDs, and excludes token,
  routing-account and namespace values from JSON. Ambiguous labels/multiple rows
  require explicit local identity without a TTY. Activate/remove delegate to the
  same transactional selection/epoch owner and acknowledge only after success.

## Current evidence

Approved TMPDIR, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`, serial normal-stack
Cargo; no new dependency, timeout increase, test disable or baseline weakening.

| Command | Result |
| --- | --- |
| `cargo test --locked -p oc --bin oc` | **94 passed / 0 failed**; three new auth CLI input/metadata/writer scenarios plus all prior binary consumer tests. |
| `cargo test --locked -p oc --test auth_cli -- --nocapture` | **1 passed / 0 failed**, rebuilt native debug ELF with isolated pipes and controlling PTYs. |
| `cargo test --locked -p oc-adapters --lib auth0` | **36 passed / 0 failed**; shared methods, attempts, refresh, captured bindings and actual local WS/runtime regressions. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit0. |
| `cargo fmt --all -- --check`, `git diff --check`, progress check | Exit0. |

The actual ELF fixture intentionally has malformed runtime configuration, no model
or MCP: auth still uses its fixed native owner. It proves preflight refusals create
no data root; two masked real-PTY key additions (one through method selection),
cancel without a third account, restart/list/switch/logout and exact active-account
metadata. Keys never appear in captured output; terminal modes restore exactly.
It also drives the **real default-port browser listener** from non-TTY CLI through
wrong-state callback and SIGINT, verifying safe HTML/failure, no success/account
write, socket closure and truthful exits. Pipe login never calls the fixture-owned
desktop launcher; both-TTY Browser login does call it, with parent API credential
canaries excluded. This stub records only an invocation flag, not URL/state, and
never opens an actual browser/profile. SQLite contains zero sessions, turns or
tool operations, and only the acknowledged surviving key. No issuer token exchange,
device authorization, real browser/profile access or model dispatch was performed.

The device polling/token/store semantics remain qualified by the owning fake issuer
receipt, not inferred as actual device login from this CLI test. Release and real
owner-operated browser/device/key qualification remain pending.

## Boundaries and next

Primary paths: `oc/src/{cli,bootstrap,auth_cmd}.rs`, separate
`auth_cmd/tests.rs`, `oc/tests/auth_cli.rs` and `support/auth_cli.py`.
Existing generic account/Go/custom behavior is unchanged; shared selectable method
projection was deduplicated, not broadened. T44/VIS45 remains independently PAUSED.
Next: real TUI method/pending/cancel/account/picker consumer, actual-binary PTY,
then current final gates and mandatory dedicated live prerequisites. T57 remains
active; this checked CLI slice is not full R5/R6 closure.
