# T56 — inherited signal policy: diagnosed and fixed

Date: 2026-10-06. Checked follow-up to pushed `87106ea5c`. Frozen R1/R2/R4,
not a new task or relaxation of Ctrl+C behavior.

## Evidence changed the cause model

The first full locked workspace invocation launched through a background shell
failed the actual-binary interrupt assertion. Diagnostics proved the exact `sleep`
was still foreground/alive, the persisted PTY selection was unchanged, tty ISIG
was enabled with VINTR byte 3, and native cells displayed `^C`. Raw delivery and
focus worked; neither an arbitrary delay nor host-output draining fixed this cause.

An isolated deterministic reproduction on the prior release binary:

```text
sh -c 'trap "" INT QUIT HUP; python3 -B crates/oc/tests/support/terminals.py target/release/oc --control-only'
exit 1: foreground sleep survived raw Ctrl+C
SigBlk: 0000000000000000
SigIgn: 0000000000000007
```

The shell/job inherited ignored HUP/INT/QUIT from the application launcher. Normal
foreground invocations and CPU-contention repeats did not have that policy and
therefore passed. This supersedes the earlier timing/backpressure-only hypothesis;
the historical failed workspace/release attempts remain failures, not PASS.

## Minimal production correction

The existing native PTY `Command::pre_exec` now clears the inherited signal mask
and installs default HUP/INT/QUIT/PIPE/TERM/CHLD/TSTP/TTIN/TTOU dispositions before
binding its owned controlling tty and exec. Only async-signal-safe stack/syscall
operations occur after fork. Failure returns the existing spawn refusal, not a
success stub. The application's own signal policy is **not** changed.

No Ctrl+C-to-killpg special case was added: the actual owned tty retains control
of its line discipline. A raw-mode program still receives literal control bytes;
an interactive foreground job receives its normal tty-generated signal. Explicit
user traps, such as the crash fixture's later `trap '' HUP`, remain effective.
No shell-job behavior, environment/trust boundary, dependency or model tool changed.

The actual-binary fixture deliberately starts every native application with ignored
HUP/INT/QUIT and blocked INT/QUIT. It does not normalize them on the application's
behalf. Thus the actual owner must correct its PTY child for Ctrl+C to pass, including
the background final-gate launch. Failure diagnostics include only fixture process
signal facts and synthetic-screen data, not credentials or live responses.

A second observed fixture race read SQLite immediately after process death while
the owner was still publishing remove. The assertion now waits for **both** exact
process reap and `Removed/live=0`, under the same deadline; it does not weaken either
required fact or substitute process death for acknowledgement.

## Checked gates and remaining final gate

- `cargo test --locked -p oc-adapters --lib term01_`: **9/0**.
- `cargo test --locked -p oc --test terminals -- --nocapture`: **1/0**, entire actual
  debug scenario, now with inherited-ignore/mask regression. RSS 50,052→55,380 KiB,
  threads 8→10, descriptors 24→26, one CPU tick in 300 ms idle; same geometry and
  independent shutdown/crash/reap/no-replay checks passed.
- Strict workspace all-target Clippy, fmt and diff check: exit 0.
- Current release rebuild + identical fixture + full locked workspace/build/strict
  gates are queued sequentially in `t56-final-current.log`. This slice does not
  claim those pending gates or T56 DONE, overall READY or T44/VIS39 PASS.

Approved TMPDIR, jobs=3, test threads=2, normal stacks; no parallel Cargo, timeout
increase, disabled test, altered baseline, global signal mutation or live API call.
