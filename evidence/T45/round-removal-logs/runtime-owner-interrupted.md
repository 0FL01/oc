# Interrupted exploratory runtime-target collection

Command: `cargo test --offline --locked -p oc-adapters --test runtime -- --test-threads=1`.
This exploratory command was chained after two targeted runs under one Bash1800000ms
ceiling. Those preceding runs consumed56.5s, so Bash killed the Python collector
before its own1798s watchdog. The collector's original blocking64KiB read and
unflushed gzip buffer left **zero captured bytes**. Full stdout for this attempt
is unavailable; do not call this a lossless transcript or a successful gate.

The exact observed orphaned owned Cargo926091 / runtime926102 group926091 was
SIGKILLed, then absence of both PIDs was verified. No foreign process was signalled.
No unknown external effect was retried: all work was an owned offline test fixture.
Old repeating-tool scripts still relied on the removed cap; deny/protected/reload
fixtures now include genuine finals. The collector now drains read1 and flushes.

The independent unchained replacement `runtime-owner-green.log.gz` has complete
stdout/stderr:120/0/0, exit0,211.569s. Final separately precompiled full workspace
`workspace.log.gz` is complete:42 summaries,1453/0/10, exit0,936.650s, with1798s
outer watchdog and its own Bash1800000ms call. No individual test deadline changed.

This receipt records a collection exception honestly; the removed owned zero-byte
placeholder contained no historical source, transcript or recoverable payload.
