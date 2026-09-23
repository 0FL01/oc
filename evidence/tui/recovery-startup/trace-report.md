# Per-launch startup trace — implementation report

Base: `fa4bc25` (branch `agent/oc-rust-port`). No commit was made (owner instruction).
Scope: diagnostics only. No behavior change to stdout/stderr, no new dependencies, no log
framework/levels/rotation. Every call is a no-op unless a trace sink is active.

## Result

Bounded, redacted, per-launch startup tracing is implemented and verified.

- New module `crates/oc-adapters/src/trace.rs` (`pub mod trace` in `lib.rs`) with the exact
  requested API: `init_default`, `init_at`, `log`, `active`, `env_fact`.
- Resolution: absolute `OC_STARTUP_TRACE` → absolute `$XDG_STATE_HOME/oc/startup-trace.log` →
  `$HOME/.local/state/oc/startup-trace.log`. Parents created, file truncated per launch,
  mode 0600. Any failure returns `None`/`Err` silently (`init_default` never breaks startup).
- Sink: `OnceLock<Mutex<Sink>>`; lines `+<elapsed_ms>ms pid=<pid> <stage>: <message>\n`,
  flushed per line; messages truncated to 512 chars and forced single-line; 262144-byte hard
  cap with one literal `trace: truncated` marker, later writes ignored.
- `env_fact` prints only `name present=<bool> empty=<bool>`, plus
  `fingerprint=<first 8 hex of sha256(value)>` only with `OC_STARTUP_TRACE_FINGERPRINT=1`
  and a nonempty value; the value itself is never written.
- Call sites: `bootstrap::run` (begin/cwd/tty/data-dir), `tui_cmd` (begin, spawn.ok/fail
  category, exit code), `headless::run_once_to_writers` (begin, spawn ok/fail detail, exit),
  `application::spawn_inner` (begin, storage.open ok/fail class, runtime.ready, spawn.fail
  category), `composition` (env selectors, roots, sources bytes/missing/parse_fail, selection,
  provider/baseURL/apiKey env name/header count, `env.ref`, discovery url/enabled/ok/fail,
  dcp/plugin/defs typed failures, load.ok/fail), `discovery::fetch_models` (attempt n, HTTP
  status, typed `DiscoveryFailure` class, retry delays). `storage::Db::open` module-level line
  was intentionally skipped: the application-level `storage.open` line covers startup, as the
  task permits.

## Failed attempts during implementation (reported truthfully)

1. `cargo test --locked -p oc --test recovery_startup -- --test-threads=1` — exit 101,
   2 failed (`isolated_binary_startup_routes`, `isolated_discovery_startup_routes`).
   Cause: internal refactor mapped the UI startup-failure `ExitCode::from(1)` to `0`, and the
   new Python assertions used `<stage> <message>` instead of the documented `<stage>: <message>`.
2. `python3 crates/oc/tests/support/discovery_startup.py target/debug/oc` — exit 1
   (`AssertionError: ('unauthorized', 'no 401 discovery attempt line')`), same format cause.

Both were fixed; every existing assertion was kept.

## Final commands and exits

| Command | Exit |
| --- | --- |
| `cargo fmt --all` | 0 |
| `cargo test --locked -p oc-adapters trace --lib -- --test-threads=1` | 0 (6 passed) |
| `cargo test --locked -p oc --test recovery_startup -- --test-threads=1` | 0 (3 passed) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | 0 |
| `cargo build --locked` | 0 |
| `cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1` | 0 (no failures, none timed out) |
| `git diff --check` | 0 |

## Redacted trace sample

Real TUI launch of `target/debug/oc` against an isolated fixture with a loopback catalog
returning HTTP 401 (credential/body markers asserted absent from the trace):

```
+0ms pid=2341668 trace: started
+0ms pid=2341668 startup.begin: bin=oc args=1 subcommand=none
+0ms pid=2341668 startup.tty: stdin=true stdout=true
+0ms pid=2341668 tui.begin: project=<tmp>/project
+0ms pid=2341668 spawn.begin: project=<tmp>/project data=<tmp>/home/data/oc env=9
+0ms pid=2341668 env.selector: HOME present=true empty=false
+0ms pid=2341668 env.selector: OPENCODE_CONFIG_DIR present=false empty=false
+0ms pid=2341668 config.global: root=<tmp>/home/config/opencode
+0ms pid=2341668 source: path=<tmp>/home/config/opencode/opencode.json bytes=195
+0ms pid=2341668 source.missing: path=<tmp>/home/config/opencode/opencode.jsonc
+1ms pid=2341668 selected: model=ludka2/fixture/new
+1ms pid=2341668 provider.selected: id=ludka2
+1ms pid=2341668 provider.api_key: env=FIXTURE_DISCOVERY_KEY FIXTURE_DISCOVERY_KEY present=true empty=false
+1ms pid=2341668 env.ref: FIXTURE_DISCOVERY_KEY present=true empty=false
+2ms pid=2341668 provider.base_url: scheme=http host=127.0.0.1 port=43493 path=/v1
+2ms pid=2341668 discovery: url=http://127.0.0.1:43493/v1/models enabled=true
+53ms pid=2341668 discovery.attempt: n=1 status=401 class=Unauthorized
+53ms pid=2341668 discovery.fail: class=Unauthorized
+53ms pid=2341668 load.fail: category=Discovery(Refresh(Unauthorized))
+53ms pid=2341668 spawn.fail: category=DiscoveryUnauthorized
+53ms pid=2341668 spawn.fail: category=DiscoveryUnauthorized
+1196ms pid=2341668 tui.exit: code=1
```

`<tmp>` replaces the per-run temp path (paths are explicitly allowed by the contract).
With `OC_STARTUP_TRACE_FINGERPRINT=1`, selected `env.*` lines additionally carry
`fingerprint=<8 hex>`; no value is ever written.

## Tests added

- `trace.rs` unit tests: inactive no-op; documented header/line format and 0600 mode;
  byte cap writes exactly one truncation marker and freezes further writes; 512-char bound;
  `env_fact` hides the value and reports presence/empty; fingerprint flag appends exactly
  8 hex chars and never the value. Tests are serialized with a module mutex and reset the
  test-only sink state, so they do not rely on the real `HOME`.
- `crates/oc/tests/support/discovery_startup.py`: every case now runs with
  `OC_STARTUP_TRACE=<tmp>/startup-trace-<case>.log` and asserts the file exists, `startup.begin`
  is present, and the planted credential (`DUMMY-DISCOVERY-CREDENTIAL`) and private body
  marker never reach the trace (TUI and headless invocations). The 401 route asserts
  `discovery.attempt: n=… status=401 class=Unauthorized`, `discovery.fail: class=Unauthorized`,
  typed `spawn.fail: category=DiscoveryUnauthorized`, `tui.exit: code=1`. The happy `present`
  route asserts `discovery.ok: models=1 selected_present=true`, `spawn.ok`, `tui.begin`,
  `tui.exit: code=0` and exits 0 (existing assertion). Headless detailed routes assert
  `headless.exit: code=1` and no markers. All earlier assertions are unchanged.

## Changed files

- `crates/oc-adapters/src/trace.rs` (new)
- `crates/oc-adapters/src/lib.rs`
- `crates/oc-adapters/src/application.rs`
- `crates/oc-adapters/src/composition.rs`
- `crates/oc-adapters/src/discovery.rs`
- `crates/oc/src/bootstrap.rs`
- `crates/oc/src/headless.rs`
- `crates/oc/src/tui_cmd.rs`
- `crates/oc/tests/support/discovery_startup.py`
- `evidence/tui/recovery-startup/trace-report.md` (this report)

## Residual leak/risk and limits

- Headless `spawn.fail: detail=<text>` persists the detailed spawn error that is already
  printed to stderr today. `MissingCredential` is `missing credential for provider.X.options.apiKey`
  (name only). A malformed typed config value can make a serde `shape:` reason quote that
  scalar (e.g. a wrong-typed `apiKey`) in the `Configuration` detail; this is pre-existing
  stderr behavior and is now persisted in a 0600 file. The fixture asserts the planted
  credential/body markers never appear; no other value-redaction change was made.
- Fingerprint mode intentionally writes `sha256(value)[0..8]`; it is opt-in and never the value.
- HTTP status codes, typed failure variants, filesystem paths, model/provider/agent ids and
  URL scheme+host+port+path are written by design. Discovery URLs are pre-validated to contain
  no userinfo/query/fragment. Response bodies and header values are never logged.
- `oc --smoke` intentionally does not initialize the trace (documented side-effect-free
  command) and the `sessions list` route has no dedicated trace stage; `Db::open` itself is
  not instrumented (application-level line covers startup).
- Trace initialization failures are silent by design; when `OC_STARTUP_TRACE` is relative or
  unset, resolution falls back to XDG state/home.

## Not verified

- No live provider call was made; only loopback fixtures. The owner's exact shell/catalog
  401-vs-403 identity remains unverified, as before this change.
- No long-run rotation behavior is claimed (none exists by design; per-launch truncate only).

## Parent follow-up: detail leak closed, release build and real-profile smoke

- Parent review changed the headless stage to `spawn.fail: detail_len=<n>` (`crates/oc/src/headless.rs`). The detailed text already goes to stderr; the trace file now cannot duplicate a serde `shape:` scalar into a second surface. The earlier paragraph above describes the pre-follow-up behavior and is retained as history.
- Parent checks after that edit: `cargo fmt --all -- --check` 0; `cargo test --locked -p oc-adapters trace --lib -- --test-threads=1` 0 (6 passed); `cargo test --locked -p oc --test recovery_startup -- --test-threads=1` 0 (3 passed); `cargo clippy --locked --workspace --all-targets -- -D warnings` 0; `cargo test --locked --workspace --no-fail-fast --quiet -- --test-threads=1` exit 0 (36 green groups, zero failures, five existing ignores); `git diff --check` 0.
- `cargo build --locked --release -p oc` 0. Real-profile release smoke with the inherited product environment, isolated `XDG_DATA_HOME` and `OC_STARTUP_TRACE` (+`OC_STARTUP_TRACE_FINGERPRINT=1`): Home reached, Ctrl+C exit **0**, trace written, credential value absent (`credential_leaked: False`). Observed stages: `startup.begin`, `startup.cwd`, `startup.tty`, `startup.data_dir`, `tui.begin`, `spawn.begin`, three `env.selector` facts, `config.global: root=/home/opencode/.config/opencode`, `provider.api_key: env=LUDKA2_API_KEY present=true empty=false fingerprint=54f454fc`, `env.ref: LUDKA2_API_URL/LUDKA2_API_KEY`, `discovery: url=https://ludka2.bash8.de/v1/models enabled=true`, `discovery.attempt: n=1 status=200 models=44`, `discovery.ok: models=44 selected_present=true`, `load.ok`, `spawn.ok`, `tui.exit: code=0`.
- Discriminator for the owner: run the rebuilt binary in the failing shell with `OC_STARTUP_TRACE_FINGERPRINT=1` and compare the `provider.api_key`/`env.ref` fingerprint with the working value `54f454fc`. A different fingerprint means a different effective credential; an identical fingerprint with `status=401/403` points at provider-side catalog policy for that identity; a different `config.global` root or selector presence points at the shell's effective configuration. The trace also reports the exact failing stage and typed category.

## Pareto review (KISS / YAGNI)

- **KISS:** one 310-line module, one process-global sink behind `OnceLock<Mutex<…>>`, one opt-in env flag, seven small call sites in `oc` and `oc-adapters`; no new dependencies (sha2 already a direct dependency); every line is one bounded ASCII message, flushed per line, 512-char message cap, 256 KiB per-launch cap with a single truncation marker; initialization failures are silent no-ops so tracing can never break startup.
- **YAGNI (deliberately not built):** no tracing/log framework, no log levels or verbosity flags, no rotation or history retention, no remote shipping, no JSON schema, no UI log viewer, no per-span instrumentation, no runtime reconfiguration. Per-launch truncate covers the actual need: one failing launch is enough to identify the stage.
- **Security:** only names, booleans, counts, byte sizes, status codes, typed category names, filesystem paths, provider/model/agent ids and URL scheme+host+port+path are written; environment values, header values, response bodies and config contents are never written; the optional fingerprint is a truncated one-way digest, never the value; file mode 0600; `--smoke` stays side-effect-free.
- **Value:** one run in the owner's shell yields the effective config root, the selected provider's credential source and fingerprint, the discovery URL/attempt/status, and the typed failing stage — the four facts needed to separate a stale credential, a shell-specific config root, and provider-side catalog policy.
