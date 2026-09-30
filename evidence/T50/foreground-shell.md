# T50 first atomic slice — frozen foreground shell obligations

## Current coordinator qualification — 2026-09-30

The parent independently reviewed all source, consumer, fixture and approval-grant
changes below. A read-only security review found one additional concrete alias
bypass across accepted singular/plural object keys:
`permission.bash=deny` plus `permissions.shell=allow` returned Allow. The new
`tool12_alias_objects_intersect_across_singular_and_plural_config_keys` was RED
(exit 101, Allow versus Deny). Object aliases now intersect across both accepted
keys; ordered rule arrays retain their established last-match semantics. The
four foreground-owner tests passed, including both Deny/Ask orders and intersected
resource maps. No saved-grant scope, output/teardown cap or deadline was weakened.

Fresh current-source coordinator gates: fmt check, strict locked workspace
all-target Clippy and all **1328 Rust tests** passed with zero failures and ten
unchanged opt-in ignores. Full log:
`tool_0f0a76cfc001MqNgPaLeRmZRuZ`. The 900000 ms combined command completed all
tests and the normal debug build, then was interrupted during release compilation.
No Cargo/rustc remained. The separate release build completed (exit 0); no test
retry or source change was needed. Debug/release help and Python **47/47**, docs,
read-only progress, advisory size review and diff checks passed.

After those normal builds, with no intervening Cargo invocation, the parent ran
`python3 -B evidence/T50/native_foreground.py target/debug/oc` and the same command
for `target/release/oc`. Both passed **21 cases**, **45 actual main/child provider
requests**, **23 terminal operation rows** and **zero saved grants**. Two added
split-key Deny cases proved absent shell schema, typed failure, no marker effect
and the real call/result continuation for canonical shell and legacy argv.
Both ELF hashes stayed unchanged through these checks:

- Debug: `dc366d68fa5ea405ef56fcb19f384dbbec1a1f0a66cd8f4d933a41990c45d29e`.
- Release: `234a7174177d881d0f8749094ef4cd4603670124e8829ea345596b7725dd35f0`.

These binaries are base `32dbf48184f9980318aea8dbec9506a480d0b212` plus the reviewed
foreground implementation and coordinator alias fix. The earlier 1327-test/
19-case artifacts below remain historical, not relabelled as current. Full
TOOL12 and background TOOL13 remain open; T44 remains PAUSED. No paid/live request,
user-config read/change, tool replay, dependency or schema change occurred.

Base: `32dbf48184f9980318aea8dbec9506a480d0b212`. Frozen before RED on
2026-09-30. Parent's four progress-start changes are inherited and excluded.
Pinned donor: `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Obligations and proof required

1. Actual root/child provider catalogs expose only canonical `shell` with
   `command`, optional `workdir`, `timeout` (milliseconds). Hidden legacy
   `bash(argv,cwd,timeout_ms)` remains dispatchable, never joined/reinterpreted.
   Catalogs/guidance respect Build/Plan/General/Explore ceilings and effective
   policy; no unimplemented selected tool, write/edit/websearch/execute advertised.
2. Linux command execution uses the product's inherited `SHELL` after executable
   validation. Donor `tool/plugin/shell.ts:111,197–205` uses compatible priority;
   `shell/select.ts:13–23,116–139,164–188` rejects fish/nu for that priority,
   selects executable SHELL, then PATH bash, then /bin/sh. There is no native
   configured-shell selector today: do not invent a config field. Native child
   environment remains credential-free allowlist, not MCP inheritance.
3. Actual shell `-c` quoting, operators and root-relative admitted workdir effects;
   canonical default 120000 ms, explicit 0 no execution deadline, positive timeout
   limited by named native 600000 ms resource ceiling. Legacy default 30000 ms
   remains. Zero does not disable cancellation/output/drain/KILL/reap/cwd bounds.
4. One policy/execution identity for aliases, including Deny/Ask/approval/saved
   grants and parent-child ceilings. Raw command is the canonical policy resource;
   existing exact argv resource formatting remains untouched. Saved argv patterns
   must not authorize command strings by accident, or conversely. Approval Once
   and Always facts must agree with the actual invocation and survive reopen.
5. Invalid arguments and background:true produce pre-intent refusal with no
   process effect. Background is pending: schema must not advertise true as
   working. Workdir stays pinned to native admitted root, no trust widening.
6. RED→GREEN source-derived checks plus bounded direct native ELF fake-provider
   request/result capture, no paid calls/user config/secrets. Meaningful closest
   permission, lifecycle, T54 retry, CFG09/10/UI07 and MCP security regressions.
   Final serial fmt/clippy/workspace tests/debug+release builds/help and Python/
   docs/read-only progress/code-size checks, with source/build association.

## Scope

This slice can qualify only the foreground portions of TOOL12/TOOL13. R1's full
selected catalog and R2 background/durable notices/recovery remain pending.
T44 remains PAUSED; no visual parity claim. No progress/spec/GOAL changes or Git
stage/commit/push belong to this temporary coordinator.

## Results — qualified foreground slice

Source association: unchanged HEAD/base `32dbf48184f9980318aea8dbec9506a480d0b212`
plus the reviewed working diff and the new owners/fixtures listed below. There is
no implementation commit yet; the parent must associate this leaf with its actual
code commit. The four inherited progress-start files are preserved. No staging,
commit, push, progress/spec/GOAL write occurred in this slice.

### RED and intermediate findings

- Initial direct-ELF RED: baseline SHA256
  `be0ef6d7a14dbffcdc392ab69cbe07ba27c32cbcfaf6f8f159f42da3495469f3`;
  `selected-operators` failed because `bash` was advertised and `shell` absent.
- A second native RED found `bash:deny` plus `shell:allow` could erase the Deny
  inside one permission object. Both explicit object aliases now intersect;
  existing explicit ordered-rule-array last-match semantics remain intact.
- The first workspace attempt exposed old catalog assumptions and exceeded the
  900-second tool window. Required schema expectations were adapted: denied
  built-ins are absent, the strict peer expects canonical `shell`, and fixtures
  that exercise an available tool explicitly admit it. Existing denial/effect,
  no-replay, timing, output/resource ceilings, and golden success criteria remain.
  An unrelated MCP catalog-filter experiment was reverted.
- Native checker corrections were confined to the fixture: genuine auxiliary
  title requests do not consume its main script; both alias orders detect Deny;
  the output assertion uses the existing 1 MiB **per stream** retain ceiling.

### Frozen atomic obligations

| # | Result | Evidence |
|---|---|---|
| 1 | PASS | Final debug and release native requests advertise exactly `shell(command,workdir?,timeout?)`; no `bash` schema or unimplemented selected tools. Real configured Plan root and General/Explore child lanes have their effective blocked schemas removed and retain their own guidance. Legacy calls still execute through native dispatch. |
| 2 | PASS | Executable inherited-SHELL wrapper writes its selection witness in the admitted cwd; invalid SHELL and an executable incompatible `fish` wrapper fall back to PATH bash; missing PATH bash falls back to `/bin/sh`, witnessed by actual `$0`. Synthetic credential and BASH_ENV startup traps do not reach the child/effect. |
| 3 | PASS | Native quoting/operator/workdir effects, literal legacy argv, explicit zero, positive timeout, actual SIGINT cancellation with zero, bounded/truncated output, and recorded leader/descendant PID reaping. Source-owner tests assert canonical 120000/default, zero, positive 600000 ceiling, and retained legacy 30000/default. Pinned-cwd tests cover rename and symlink replacement for both forms. |
| 4 | PASS | Scalar and real config aliases share the existing `legacy_key` identity; both mixed Deny orders refuse real native effects. Actual approval owner tests exercise Always/reopen, exact command save, old argv `*` isolation, reverse same-resource argv isolation, and later alias Deny. Native `--auto` uses Once and leaves zero grants. Parent/child configured restrictions remain effective. |
| 5 | PASS | Native invalid timeout/workdir and `background:true` fail without marker effects. A real runtime SQLite trigger forbids any `started` intent for invalid/background calls and all cases pass. `background:true` has typed Unsupported admission and is not advertised. |
| 6 | PASS | Final workspace quality and both freshly normally built native ELFs below. Actual native fake-provider request/result capture, retained SQLite outcomes, loopback socket receipts, effect-row counts, zero grants, teardown/reap assertions; no paid/live invocation. |

### Final checks

All Cargo commands were serial with `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=1`, `CARGO_NET_OFFLINE=true`, and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`.
Preflight used UID 1003, about 8 GiB available memory and 193 GiB available disk.

| Command/check | Result |
|---|---|
| `cargo fmt --all` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked --no-fail-fast` on final product source | PASS: 1327 passed, 10 existing opt-in tests ignored, zero failures; doc tests also PASS |
| `cargo build --locked` then `target/debug/oc --help` | PASS |
| `cargo build --release --locked` then `target/release/oc --help` | PASS |
| Direct `native_foreground.py target/debug/oc` after both builds | PASS: 19 cases, 41 main/child provider requests, 21 terminal operation rows, zero saved grants |
| Direct `native_foreground.py target/release/oc`, no intervening Cargo | PASS: same 19 cases/counts |
| `python3 -B -m unittest discover -s scripts -p 'test_*.py'` | PASS: 47 tests |
| `python3 scripts/check_docs.py` | PASS: structural documentation/registry check |
| `python3 scripts/progress.py check` | PASS: read-only journal structure check |
| `python3 scripts/code_size.py --base 32dbf48184f9980318aea8dbec9506a480d0b212 --changed` | PASS: advisory review; 16 changed/new Rust files, largest 2760 lines, new invocation owner 105 lines and shell tests 154 lines |
| `git diff --check` and reviewed intended diff | PASS |

The final workspace run includes the closest real headless/PTY approval,
CFG09/CFG10/UI07, T54 retry/no-replay, MCP media/security/lifecycle, durability,
shell watchdog and bounded-soak regressions. No new test ignores, golden baseline,
deadline, output ceiling or suppression changes were used to obtain PASS.
The configured profiles in the native fixture are real admitted definitions,
not claims that new builtin Plan/General/Explore registrations were added.

Final ELF SHA256:

- Debug: `b3f553205366ac3d0c0358c85599228a18d7737d766ebdf8c61422101d0301be`.
- Release: `33287a1196fb35eefffeccf7677dcfb3f682e7b585c4df96c19ed0df7a3bf3c6`.

Retained small logs under `/home/opencode/.cache/opencode-tmp/opencode/`:

- `t50-foreground-workspace-tests-20260930.log` — initial failing/interrupted run.
- `t50-foreground-compaction-20260930.log` — targeted catalog/compaction regression.
- `t50-foreground-workspace-final-20260930.log` — intermediate completed PASS,
  with `T50_WORKSPACE_EXIT=0`, before the last alias/approval-display changes.
- **`t50-foreground-workspace-qualified-20260930.log`** — final-source full PASS.
- **`t50-foreground-native-debug-20260930.log`** and
  **`t50-foreground-native-release-20260930.log`** — final ELF hash, per-case
  PASS, loopback socket and effect/grant receipts. Native fixture roots use the
  approved TMPDIR, are owned/isolated and removed; servers are closed/joined.

### Changed ownership and compatibility details

- `oc-adapters/src/tools/shell_call.rs` owns typed normalization, default/zero/
  ceiling and Unsupported admission; `tools/tests/shell.rs` contains its closest
  new scenarios under the existing unit-test target. Historical argv metadata
  remains ignored; argv is never converted into command source.
- `shell.rs` owns the validated Linux compatible selector and the existing sole
  process-group supervisor. Zero disables only execution timeout. Relative PATH
  entries are not selected; native root/no-follow/pinned cwd and credential-free
  child allowlist remain explicit supported differences from donor breadth.
- `tools.rs`, `runtime.rs`, `runtime/turn.rs`, `permissions.rs`, `approval.rs`,
  `storage_grants.rs` connect the same execution/policy owner and effective
  builtin catalogs. Canonical raw command and unchanged argv display resources
  have collision-free, storage-tagged saved-grant domains under one `bash`
  authority; old wildcard grants cannot acquire command interpretation.
- `oc-tui/src/tools.rs` uses the existing shell-card consumer for recorded
  command/workdir. `approval_view.rs` shows source facts rather than the internal
  grant tag. These are DTO/consumer changes, **not visual qualification**.
- `tests/fixtures/approval_lifecycle.rs`, `runtime_compaction_tests.rs`, and
  `oc/tests/{configured_workspace,durability,golden_binary}.rs` supply meaningful
  lifecycle proof or adapt only required catalog contract differences.
- `docs/CODE_MAP.md` records the actual seam. This leaf and
  `evidence/T50/native_foreground.py` are the new evidence/fixture files.

### Remaining scope / return

Full R1/TOOL12 is **not complete**: the other selected tools and their actual
schemas/consumers still require their own implementation slices. Full R2/TOOL13
is **not complete**: durable background supervision, automatic notices,
recovery/session binding and their native qualification are the next separate
slice. T44 remains PAUSED. No full GOAL or live/visual completion claim is made.
Temporary mutation, Cargo and native-fixture coordination ownership is released
back to the parent with the worktree uncommitted.
