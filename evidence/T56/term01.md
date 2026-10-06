# T56 — real debug/release TERM01 and checked cleanup

Date: 2026-10-06. Base: pushed `0babd4e5e`; this checked slice adds lifecycle
failure handling and the actual-binary fixture. No T44/VIS39 paired visual PASS,
T45 whole-task PASS, product READY or paid/live API claim.

## Actual owner cleanup, not successful signal delivery alone

The PTY child is guarded immediately after spawn. Checked teardown enumerates
only the owned terminal session's process groups, terminates foreground/background
groups and the unreaped leader, observes remaining non-zombie members within a
bounded deadline, then uses bounded `try_wait` to reap. EINTR cannot bypass that
deadline; disappearing `/proc` entries are distinguished from enumeration/read
failures. Recovery revalidates starttime/boot/UID/native-root identity before every
signal. A stale identity is not permission to kill the saved PID.

Signal, observation, wait, join and lifecycle-publication failures remain errors;
shutdown failure is sticky across calls. All workers are stopped/joined despite
another worker's failure. Unproved cleanup retains `live=1` identity for recovery,
clears selection and records failure rather than falsely acknowledging termination.
The existing SQLite transaction/store is reused, with no new schema/dependency.
The real SQLite ABORT-trigger regression proves remove/shutdown failure, retained
recovery identity and successful later interrupted reconciliation without respawn.

## One fixture, two rebuilt native binaries

`crates/oc/tests/terminals.rs` runs `support/terminals.py` against
`CARGO_BIN_EXE_oc`; the same fixture takes an explicit `target/release/oc` path.
It drives the actual Crossterm/Ratatui process in a controlling outer PTY, reads
native SQLite receipts and independently checks Linux processes/descriptors/cwd/env.
Only isolated fixture directories, an allowlisted synthetic environment and a
loopback scripted Responses peer are used. There is no production test mode,
synthetic pane output, authoring credential read or live Go request.

- Fresh application creates no PTY. Undefined initial Terminals Enter is a no-op;
  Up selects `+ New terminal`; `/terminal` creates the second actual Bash PTY.
  Source session/Location/generation, selected executable/cwd and process identity
  are independently checked; the synthetic parent credential is absent in children.
- Real command output and raw Ctrl+C interrupt the independently observed exact
  foreground `sleep`, without executing the deliberately remapped terminal-close
  action. Raw Ctrl+D is first received as byte 4 by a real raw-mode Python program,
  then exits Bash when expected; the native application remains usable.
- Select/hide/toggle persist the right ID, do not kill/duplicate shells and choose
  the **last** inventory item. Hidden output drains; reopening attaches its screen.
  Transcript wheel retains PTY focus; first session click/release returns focus and
  the Unicode draft. Lower-composer explicit remove reaps only its selected root PTY.
- The real PTY program emits ANSI red, wide Unicode, cursor hide, DSR, OSC52 and DCS,
  then a 1 MB OSC plus 100 kB visible flood. Native cells/cursor and explicit gap
  recovery are observed; clipboard/device payloads are not forwarded to the host.
  The DSR reply reaches only the owned PTY. Reconfigured leader is honored, while
  the former leader is received as raw byte 24. Actual `stty size` changes on resize
  and horizontal/vertical tab layouts under the same PTY identity.
- An actual native Subagent invocation reaches the held scripted child request.
  Child Terminals activation closes back to the parent **before** creating with
  the captured child session/source. SQLite and subsequent real child-PTY output
  prove the target; parent draft and selection remain intact. Closing/navigation
  leave that PTY alive; the parent consumes the actual child function result.
- Clean native shutdown reaps both root/child PTYs and restores outer termios and
  alternate screen. Restart does not recreate them. A separate SIGKILL campaign
  leaves two genuinely alive fixture-owned shells; restart verifies/quarantines
  their groups, records Interrupted, clears selection and never replays the
  append command (independently unchanged sentinel file).
- A fixture-owned unrelated leader with a deliberately stale recorded starttime
  survives actual native recovery. Malformed pending metadata retains Linux
  Terminals/`+ New terminal` and exact `Unable to load terminal` feedback; no
  creation occurs and unavailable cleanup exits nonzero with termios restored.

## Current measured facts

Final identical fixture after the production cleanup edit:

| Native binary | Baseline RSS KiB | Two PTYs RSS KiB | Threads | FDs | Idle ticks / 300 ms |
| --- | ---: | ---: | --- | --- | ---: |
| debug | 49,448 | 53,068 | 8 → 10 | 24 → 26 | 0 |
| release | 16,892 | 19,740 | 8 → 10 | 24 → 26 | 0 |

Actual child `(rows, columns)`: compact debug `(20,38)`, release `(21,38)`;
160×48 debug `(44,78)`, release `(45,78)`; vertical tabs/devtools-off `(46,57)`
for both. The debug/release devtools default accounts for the one-row difference.
Bounds remain eight PTYs, 8192-byte input messages/32-item queue, 64 KiB pending
input/output ring, 256 scrollback rows, 120×240 cells and 1024-byte escape gate.
The fixture checks descriptor/thread/RSS bounds and independent reap, not merely
process exit or a painted title.

## Commands and material experiments

- `cargo test --locked -p oc-adapters --lib term01_`: **9 passed / 0 failed**.
- `cargo test --locked -p oc --test terminals -- --nocapture`: **1/0**, real debug.
- `cargo build --locked --release -p oc`, then
  `python3 -B crates/oc/tests/support/terminals.py target/release/oc`: **PASS**.
- Strict workspace all-target Clippy, fmt check, both native `--help`, journal and
  diff checks: exit 0. Approved TMPDIR, jobs=3, test threads=2, normal stacks,
  sequential Cargo; long release compilation used background log capture rather
  than increasing a tool timeout. Full locked workspace tests are the next gate.

Initial fixture failures were kept as evidence of specific approaches: private
Db-connection access was replaced by independent SQLite fault injection; a
configless selection toast obscured a title, so the genuine child campaign uses
an admitted local peer and independent process facts; terminal-state expectations
were corrected to the existing serde schema. Nonexistent editor Ctrl+K did not
clear the parent draft; the fixture now uses its actual Home/select-End/delete.

The first release interrupt used an assumed delay; it was replaced by actual
foreground-process observation. A later release repeat still timed out observing
the interrupt. Ten narrowed reproduction runs (five under bounded CPU contention)
passed with diagnostic capture enabled. The host fixture now continuously drains
outer PTY output during that process observation, preventing its own UI-writer
backpressure; it still requires the same exact foreground exit/reap, not a weaker
expectation. Current full debug and release both pass. No production control-byte
special case, timeout increase, test disable, baseline rewrite or paid retry was
used. Historical failed invocations are not counted as PASS.
