## Result
DCP 3.2.0 donor delta qualified and pinned: protected tool aliases fixed (c368b0e46), IDs beyond 9999 (f6ac67178) and compaction nudge replay (579e81b64) qualified without product change, revision/lock/both exact gates moved to d637981 with bare/latest/3.1.15 aliases (1a927a5ff).
## Checks
Workspace 1562/0/10, pty_t39 48/0 with actual @3.2.0 admission, rebuilt debug/release DCP native regressions PASS (historical native_dcp_defaults [0,1,0] expectation fails identically pre-change; its replacements pass), fmt/Clippy/Python47/docs/diff PASS. Evidence: evidence/T45/dcp320.md.
## Risks
Revision names the adopted subset, not full 3.2.0 parity; CodeMode/compact IDs/TS host out of scope; T44 VIS38 display attribution stays 3.1.15 until explicit resume. Whole T45 open; T44 PAUSED, T27 allowance unchanged.
## Next
Remaining amended-plan T45 items (R6 headless/run --agent and Plan lifecycle regressions, R10 shared prompt/instructions lifecycle, PRM01 17-step root/child qualification) per spec order.
