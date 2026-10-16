# T56 — native session-local interactive terminals: COMPLETE

Date: 2026-10-06. Reviewed/pushed implementation HEAD: `be5ec25fb`.
All frozen **R1–R4 / TERM01 are verified**. The task followed T53 chronologically;
each checked implementation/correction slice was committed and pushed. No known
T56 blocker or failed final gate remains. This is functional T56 completion, not
overall product READY or T44/VIS39 full paired visual qualification.

## Frozen outcome closure

| Outcome | Status | Primary evidence |
| --- | --- | --- |
| R1 — one actual native owner | VERIFIED | [Owner](owner.md), [application](application.md), [frontend](frontend.md), [TERM01](term01.md): real Linux PTYs, bounded typed Core commands/ack/events, actual shell/cwd/PID/source session/Location/generation and positive credential-free environment; no startup spawn/model terminal tool. Actor/target/generation fences, original PTYs across Location/reload and retained live-child source rather than current-parent rebinding. Platform-derived capability; create/recovery failures retain controls with honest source errors. |
| R2 — real frontend and raw focus | VERIFIED | [Frontend](frontend.md), [TERM01](term01.md), [signals](signals.md): lower Terminals list plus New, undefined initial Enter no-op, wrap/row mouse-up, last-inventory toggle, persisted hide clearing without process kill/duplication, remount and exit focus. Actual child lower-composer activation returns to parent before Core dispatch to the captured child. Raw Ctrl+C/D, configured leader and nonleader remap priority, first-click matching release/transcript wheel, preserved draft/chips/cursor and configured shortcuts. |
| R3 — native VT/attachment/resize/bounds | VERIFIED | [Owner](owner.md), [frontend](frontend.md), [TERM01](term01.md): pinned VT cells/styles/cursor rendered through Ratatui, atomic snapshot/cursor then replay/ready, exact ordered ring bytes or explicit gap/reset. Native Unicode/ANSI/cursor/DSR, hostile OSC/DCS/flood protection, hidden output and actual kernel child sizes in rebuilt debug/release. ANSI16/default colors use semantic dark/light theme roles, not grammar highlighting or raw host escape forwarding. |
| R4 — cleanup/recovery without replay | VERIFIED | [Application](application.md), [TERM01](term01.md), [signals](signals.md): hide/navigation preserve identity/liveness, explicit removal/EOF/clean shutdown terminate groups and join/reap. Checked signal/enumeration/reap/publication failures stay non-success with retained unproved recovery identity and sticky shutdown failure. Actual crash with two surviving orphan leaders, verified quarantine, stale PID left alive, Interrupted/selection reconciliation and unchanged command sentinel after restart. Independent process/FD/thread/RSS/idle checks below. |

The owning test surfaces are `oc-adapters/src/terminals/tests.rs`,
`application/terminal_tests.rs`, `runtime/children/tests.rs`,
`oc-tui/src/terminal_view/tests.rs`, binary `tui_cmd/terminal_controls/tests.rs`,
and `oc/tests/terminals.rs` / `oc/tests/support/terminals.py`. UI/source unit fixtures
are not disguised actual-binary evidence; the last pair drives the real binary,
real outer/inner PTYs, SQLite and a local scripted Responses peer for live native
child routing. No paid/real external model API is needed or claimed by TERM01.

## Current final gates

All Cargo executions were sequential, normal stacks, with
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode`, `CARGO_BUILD_JOBS=3`,
`RUST_TEST_THREADS=2`. Long gates used captured background output, not increased
timeouts, parallel Cargo, skipped tests or relaxed thresholds.

| Command | Result |
| --- | --- |
| `cargo test --locked --workspace` | Exit 0; **1665 passed / 0 failed / 11 ignored**, 43 result records. Includes all 14 added T56 tests, actual-binary terminal target, existing headless/PTY/security/config/discovery/provider/child/shell/DCP/storage/recovery/soak regressions and doc tests. |
| `cargo build --locked --release -p oc` | Exit 0; current-production optimized native binary rebuilt after the signal correction. |
| `python3 -B crates/oc/tests/support/terminals.py target/release/oc` | Exit 0; complete actual release TERM01 PASS with deliberately ignored/masked parent signals. |
| `cargo test --locked -p oc --test terminals -- --nocapture` | Exit 0; current-production actual debug TERM01 **1 passed / 0 failed**, same forced parent policy/full scenario. Passed again in the final workspace. |
| `cargo test --locked -p oc-adapters --lib term01_` | **9 passed / 0 failed** after the last production correction; also in final workspace. |
| `cargo test --locked -p oc-adapters --lib child_schema_` | **3 passed / 0 failed** after exact migration expectations were corrected; also in final workspace. |
| `cargo build --locked` | Exit 0; current native debug binary. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Exit 0 after the final test edit. |
| `cargo fmt --all -- --check` | Exit 0. |
| `target/debug/oc --help`, `target/release/oc --help` | Both exit 0. |
| `git diff --check`, progress journal structure check | Exit 0. Journal checks do not replace product evidence. |

Final workspace log:
`/home/opencode/.cache/opencode-tmp/opencode/t56-workspace-final.log`, ending
`T56_WORKSPACE_GATES_PASS`. Current release build/fixture facts are in
`/home/opencode/.cache/opencode-tmp/opencode/t56-final-current.log`; that earlier
compound run subsequently failed at the old migration expectations, not at release
TERM01. The only later code change was those test expectations, not production.
All eleven existing opt-in ignores remain unchanged and are not counted as PASS.

## Actual-binary and resource facts

The canonical fixture creates two real PTYs, verifies independently `/proc` cwd,
identity and sanitized environment, exercises raw canonical Ctrl+C and raw-mode
Ctrl+D, remapped leader/nonleader keys, Unicode/ANSI/hide cursor and an owned DSR
reply. A 1 MB OSC plus 100 KB visible flood yields explicit gap recovery, not a
host clipboard/device escape. Hide clears SQL selection; toggle chooses the last
original PTY. Hidden output drains without a new process. The actual native child
is held by a scripted provider barrier; lower-composer creation returns to the
parent first, stores/selects the child's PTY, and later reopens the same working
PTY. Actual removal, exit, clean shutdown and termios/alternate-screen restoration
are independently checked.

After SIGKILL, two actual surviving orphan leaders are reconciled on native restart
with recorded starttime/boot/UID/native-root identity; the command sentinel does not
change and old rows/processes are not respawned. A separately fixture-owned stale
PID survives. Malformed pending metadata retains terminal controls/error feedback
and makes cleanup/exit non-success. The fixture subreaps only its own orphans for
independent process observations; it is not a production daemon or reaper service.

Current-production measurements (debug targeted / latest release; same full fixture):

| Fact | Debug | Release |
| --- | --- | --- |
| Baseline → two-PTY RSS, KiB | 50052 → 55380 | 16848 → 20040 |
| Threads | 8 → 10 | 8 → 10 |
| Open FDs | 24 → 26 | 24 → 26 |
| CPU ticks in 300 ms idle | 1 | 1 |
| Actual child rows×cols, 80×24 outer | 20×38 | 21×38 |
| Actual child rows×cols, 160×48 outer | 44×78 | 45×78 |
| Actual child, vertical tabs/devtools off | 46×57 | 46×57 |

Debug's default devtools row explains the one-row difference. Fixed fixture guards:
FD delta ≤8, thread delta ≤4, RSS delta <64 MiB, idle ≤6 ticks/300 ms; none was raised.
Runtime bounds: 8 concurrent PTYs, 8192-byte input, 32 control slots, 64 KiB pending
input and output ring per PTY, 256 scrollback lines, at most 120×240 cells, 1024-byte
escape gate. Supported replies go only to that PTY. No terminal bytes are archived
in SQL/raw model history; only minimal identity/lifecycle/selection metadata persists.

## Reviewed and pushed slices; material corrections

- `88e6631d0`: native libc PTY + pinned `vt100 0.16.2` owner/DTOs and real owner tests.
  MIT VT dependency, MIT/Apache VTE/arrayvec provenance/pinning recorded in owner receipt.
- `5b165af61`: acknowledged application admission/lifetime/recovery and migration12;
  corrected the initial marker collision with existing credential migration11.
- `0babd4e5e`: real lower composer/pane, raw/leader/focus, native cells/resize/replay
  and retained-child source. Fixed autocomplete inventory padding without changing
  golden expectations; child source uses an actual retained runtime, not a guessed path.
- `87106ea5c`: actual debug/release fixture and checked, bounded cleanup/failure retention.
- `5ce0b9baf`: full workspace exposed ignored/masked signals inherited from a background
  launcher. A deterministic negative control showed the byte reached the tty while
  sleep ignored SIGINT. Only PTY children now restore default terminal-related signal
  dispositions/empty mask post-fork; raw-mode programs still receive literal controls.
  Foreground/backpressure-only guesses were superseded, not hidden. Removal fixture
  waits for both actual process death and durable Removed publication/ack.
- `be5ec25fb`: exact schema vectors include required migration12; unchanged session
  columns, legacy facts, index, migration sentinel and rollback assertions remain.

The failed experiments and slice-local checks remain in their receipts. Historical
evidence/baselines were not rewritten, validation/permissions were not weakened,
resource thresholds were not raised and no failing test was disabled to obtain green.
New production/test owners are responsibility-separated;
the existing 5261-line application worker warning/next natural seam is in CODE_MAP.

## Closure, use and boundaries

Open/create a native session in `target/release/oc tui`; `/terminal` creates explicitly.
Default leader is Ctrl+X: Left/Right focus session/PTY, Down opens Terminals, T toggles,
Up hides. While PTY-focused, ordinary controls belong to the PTY; use the configured
leader to return to session. Hide is not kill; removal/shutdown is owned cleanup.

One Db/data-root lock and one application PTY owner remain. Raw history, provider/
request selection, shell-job/child ownership and trust/no-follow/env guards are retained.
Native PTYs are application-owned, not donor daemon handoff: clean shutdown reaps,
restart never resumes/replays an old command. No Node/Bun/JS/WASM host, model terminal
tool, extra store/framework, paid campaign, real-key read or credential dump was added.
User `.opencode/` is unread/unstaged; T53 campaign is unchanged. T45/T50 retain their
owners, T44 remains PAUSED and VIS39/grammar/full styled-cell/PNG parity is neither
waived nor PASS here. T57 and other task statuses are unchanged. Frozen R1–R4 and
affected constraints are resolved; stop substantive T56 work after this closure.
