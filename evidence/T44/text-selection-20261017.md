# Mouse selection in prompt and chrome (2026-10-17)

Status: complete
Source: owner's audited selection plan and authorization to implement, commit,
push and build. Base: `40490d4d12868a5bf4fb9fec73d94dd4e6e91611`.

## Result

Prompt/chrome selection and the accelerated runner are delivered in source commit
`75c664379ab048a3c98372db7d268af6a753e7df`, pushed to `origin/master`.
The complete normal gate passed in454.464s, at least80.055% below the interrupted
serial lower bound. Frozen outcomes are verified for this requested slice:

- R1: Home/session draft drag uses the existing Editor and copies the selected
  raw range; wrapped text, Unicode and atomic paste chips retain their meaning.
  Status: verified by native editor tests and actual Home/session OSC52 payloads.
- R2: Sidebar title/Context/MCP/Location and prompt-footer Location expose their
  actual visible text to drag selection and highlighting, without click effects.
  Status: verified by painted-row tests and actual binary SGR/OSC52 gestures.
- R3: Existing Select/Manual/Disabled ownership and 64 KiB limits apply. Stale or
  cancelled gestures cannot copy or activate another surface. Existing transcript,
  keyboard, editor, terminal-focus and MCP lifecycle contracts remain intact.
  Status: verified by source/gesture/mode, retained regression and full-gate cases.
- R4: Targeted native tests, binary-routing/real SGR+OSC52 checks and final
  workspace fmt/clippy/tests/debug build pass; reviewed commit is pushed.
  Status: verified by the normal gate and successful source commit/push above.
- R5: Owner's subsequent request: RECON and reduce the full test gate's measured
  warm wall time by at least 80%, retaining its test set and assertions.
  Status: verified:454.464s versus serial lower bound2278.65s, reduction80.055%.

Primary evidence: nearest owner tests and existing `pty_t39` OSC52 harness;
existing MCP lifecycle regression; final workspace commands and Git delivery.
Actual acceptance by the owner's desktop/SSH clipboard is separate from terminal
transport qualification and must not be inferred from a PTY frame.

## Envelope and boundaries

The initial UI envelope is native `oc-tui` editor/paint/input owners, binary
routing tests and the existing PTY target; no clipboard backend, generic pointer
framework, autoscroll or keyboard remapping. R5's measured blockers expand that
envelope only at the seams below. No storage/schema/user-config changes or new
Rust/runtime dependencies are permitted or introduced.
The explicit R5 request additionally permits test-fixture/workflow corrections
needed for that measured acceleration, not weaker coverage or production guards.
The measured serial bottleneck and two contention failures require cross-target
scheduling: use the pinned, repository-local nextest development tool and its
small scheduling config, not a bespoke scheduler. It is not a Rust/runtime
dependency; Cargo still owns doctests/builds.
The first isolated nextest run disproved CPU contention as the sole cause: the
80 ms HTTP discovery case and 5 s shell flood case also fail when each owns the
entire pool. R4/R5 therefore require two minimal production seams: avoid unused
TLS-root initialization for a validated, nonredirecting plain-HTTP discovery URL;
coalesce bounded pipe reads before the existing durable publication (never remove
fsync or change capture quotas). HTTPS verification, DNS/peer policy, deadlines,
flood byte counts and storage/schema contracts remain unchanged. This is the
smallest expansion for those observed gate blockers, not provider/storage redesign.
The third full run reached401.30s (82.39% reduction) but retained two failures.
Its actual-binary RSS measurement peaked at65804KiB against the unchanged65536KiB
bound; the isolated control peaked at64800KiB. The Responses attempt repeats the
same unused system-verifier construction for plain HTTP. R4/R5 therefore extend
the existing scheme-specific initialization correction to that attempt only,
with HTTPS/DNS/peer/redirect policy unchanged. The unchanged RSS bound passed in
the focused control and final complete gate; no storage or threshold change.
Readonly copy preserves visible ellipsis; full chip selection copies raw content.
No full T44/pixel-parity claim. One mutation owner; scouts read-only.

## Checks

R1–R3 targeted evidence is green:

- `app/tests/text_selection.rs`:11 real-paint/mouse scenarios, including raw
  LF/soft wrap/Unicode/chips, no-redraw capture, source replacement, occlusion,
  retained editor selection, history-only updates, mode changes and cancellation.
- Existing transcript VIS27 regressions plus the actual embedded-PTY cross-drag
  check preserve repeated-click selection, button-independent release and focus.
- Binary reload routing exercises real prompt selection across published Manual,
  mismatched-Location Disabled, authoritative catalog and active/parked/Home views.
- Existing actual `pty_t39` SGR/OSC52 scenario checks Home/session drafts, both
  Locations, title/Context/MCP name/status, exact payload/count in Select/Manual,
  no generation/tool effects and terminal restoration. MCP lifecycle remains a
  separate retained actual-binary regression, not a new synthetic implementation.
- The whole TUI library currently has574 passing cases in the complete runs.

Final normal command: `bash scripts/test.sh` — **PASS**:

- Workspace fmt and strict all-target Clippy: PASS.
- Nextest:2054 passed/0 failed/12 original opt-in ignored across43 binaries;
  execution405.502s, zero retries. Structured inventory:2066 total, every original
  test retained and unfiltered, plus the stale-overlay and receipt-clock cases.
- All three workspace rustdoc targets ran successfully (zero doctests currently).
- `cargo build -p oc2 --bin oc2 --locked`, debug `--help` and `--version`: PASS.
  Artifact: `/home/stfu/ai/agent-frameworks/oc/target/debug/oc2`, version0.2.0.
- Entire fmt/clippy/tests/doctests/build/help/version wall:454.464009829s.
  Formula: `100 * (1 - 454.464009829 / 2278.65) = 80.05555878%`.
  This is a conservative measured reduction against an incomplete serial run,
  not a promise about cold builds, every host/load, or a successful old baseline.
- Final changed-boundary subset:28/0; the100ms input bound,165/250Hz rates and all
  resource workloads/bounds remain unchanged. No assertion or ignore was weakened.

## Performance RECON and material attempts

- Original `cargo test --locked --workspace --no-fail-fast` accidentally used
  `RUST_TEST_THREADS=1`. It was stopped after **2278.65 seconds** at the owner's
  speed request, still incomplete and with two failures. This is a measured
  **lower bound**, not a completed successful baseline. Heavy completed targets:
  adapters lib651.64s, runtime330.07s, subagent224.02s, TUI81.60s.
- Unchanged TUI with16 libtest threads:573 PASS,26.36s warm wall. Blanket workspace
  threading exposed fixture/resource races; it is not the qualified replacement.
- First complete nextest attempt:519.52s warm wall,2047 PASS/5 FAIL/12 unchanged
  opt-in ignored. It neither passed nor reached80%; it remains in the logs.
  The single T39 lane was the critical path. Three independent private-DB/PID
  archive measurements now have their own serial lane; interactive T39 stays
  serial. The two short-deadline cases reserve the whole scheduler pool.
- That attempt reproduced HTTP initialization and shell flood failures even when
  they ran alone. The two bounded production corrections above preserve security,
  durability, bytes and deadlines. Both now pass their existing assertions.
- Fixture corrections: synchronize the process-global allocator measurement;
  observe both MCP catalogs before the strictly counted collision-routing turn;
  wait for the actual root Session's completion rather than a child's finish;
  observe the first complete PTY frame before the unchanged two-second unread
  interval. Golden's isolated HOME also needs real Cargo/Rustup home metadata,
  which the runner now forwards without expanding production's env allowlist.
- Focused corrections:116 of117 PASS initially; Golden was the sole remaining
  failure. After supplying actual Cargo/Rustup homes it passed separately, then
  passed in the second full run. No failure retry is enabled and no assertion or
  threshold is relaxed.

Second full attempt:2050 PASS/3 FAIL/12 ignored,516.38s including31.85s first
Clippy rebuild. All three failures reached the same Settings helper before a
ready prompt or with an unsubmitted command; it now observes prompt readiness
and the painted atomic draft before Enter. The run's3747.33 aggregate execution
seconds have an eight-slot lower bound of468.42s, already above the455.73s goal.
Ten bounded slots are used; no test is omitted or retried.

80% target against the conservative serial lower bound: **455.73 seconds** for
the entire warm fmt/clippy/tests/doctests/build/help/version gate. Actual owner
desktop clipboard acceptance remains NOT_RUN, separately from OSC52 transport.
Logs: `/home/stfu/.cache/opencode-tmp/opencode/text-selection-bHd1g1/`.

### Later qualification evidence

The normal sixth gate failed (2051/2/12,389.86s): the unchanged100ms burst-input
case omitted its last three glyphs while final provider text was already painted;
AUD38 also submitted its second prompt before observing the first turn idle.
The bounded trace experiment passed and measured history15ms/DCP0ms, not the
suspected100ms stall. Temporary production instrumentation was removed. The test
did have a concrete observer-clock race: after cloning an absent snapshot it
checked a later clock, and already-arrived glyphs bypassed the deadline entirely.
The corrected fixture records one timestamp per existing PTY read and checks
final-byte arrival under the same output lock. This removes whole-output clones, preserves
the100ms/165Hz/250Hz/32-glyph/1024-delta workloads and additionally rejects truly
late arrivals even when an observer first sees them later. A deterministic
UTF-8-split/late-observer/101ms-late receipt regression verifies that boundary.
Separately, a regression assertion reproduced a new transcript no-drag press
becoming a foreign reasoning-header release (RED). Preserving its original
pointer-down guard on ordinary click handoff, and consuming invalidated releases,
made that complete existing reasoning regression GREEN without weakening it.

Fourth complete run:2052 PASS/1 FAIL/12 ignored in427.47s,81.24% reduction.
The unchanged memory bound and child cancellation both passed. One VIS31 input
latency measurement exceeded100ms while unrelated tests ran. Reserve the whole
pool for the two VIS31 CPU/input benchmarks as well as the two existing short
transport cases; this restores their original serial-gate measurement conditions
without changing165/250Hz input,100ms latency, idle CPU bound or worker workload.
Those benchmarks passed alone (2/0); the fifth complete run still failed the
burst-input case despite pool reservation:2052 PASS/1 FAIL/12 ignored,398.19s,
82.53% reduction. Thus unrelated workspace load is not an established sole cause.
Added failure-only glyph/rate/elapsed/screen diagnostics without changing its
100ms assertion. The four-case prefix passed4/0 in15.608s. The subsequent complete
diagnostic scheduler run passed all2053 cases in392.891s, with the same12 opt-in
ignores and unchanged assertions; it did not reproduce the latency failure. No
causal fix of that transient is claimed by the diagnostic alone.

## Risks

Whole T44 remains open. A failed or interrupted speed run never counts as PASS;
the denominator is explicitly an incomplete serial lower bound. No production
UI-loop redesign was made on the unproven completion-await hypothesis.

## Completion

Closure: R1–R5 verified, current code matches the green source commit, reviewed
diff stays within the recorded envelope, source push is confirmed, no known
diff-created blocker. Successful receipt: `fast-gate-7.{log,json}` and
`nextest-list-final.json`; failed/interrupted attempts remain separate evidence.
Actual owner terminal/desktop clipboard acceptance is NOT_RUN; OSC52 framing and
transport are qualified independently. Whole visual T44 remains open.

## Next

This requested slice is complete. No further implementation, audit, redundant
full run or unrelated T44 work is opened by this receipt.
