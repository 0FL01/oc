# T54 — Responses/common-runtime provider retry qualification

## Result

**T54 assigned backend R1–R4 / RET01 PASS.** This is not full delivered-wire or
TUI retry parity: T53/GO03 must integrate its admitted Chat/Messages wires with
this owner; paused T44/VIS43 owns running-original paired presentation. Full
T44/V09 and GOAL A01–A13 are not declared complete.

Reference: pinned OC2 v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`.
Implementation commits: R1 `a7ddde9f8c69805aafafd3852f9b210f9f0932af`, coupled
R2–R4 `2d0dea21ae0f2ccdf4ca96f65c11d9be49f2850d`.

| Outcome | Current primary evidence | Result |
|---|---|---|
| R1: one physical adapter attempt, typed safe HTTP/SSE/transport outcomes | `typed-failures.md`; current provider classifier/header/length/cancel tests, including genuine prior-done call versus terminal-only incomplete call | PASS |
| R2: finite mixed retry/continuation without spending rounds or replaying effects | `runtime-retry.md`; deterministic ten-gap/terminal/override matrix; four runtime RET01 cases; native mixed, quota, override and exactly-once append | PASS |
| R3: durable span retry before wait, semantic clearing, safe scoped history/recovery | Existing TurnLog JSON and transactional storage events; owner recovery test, both new TUI ordering tests; both ELFs' headless/park/reopen/kill-after-retry proof | PASS |
| R4: operation-scoped physical dispatch counts and fixed bindings across lanes | Root/child/native+generated compaction/title fixtures, actual socket-to-event equality, indexed old-child operation attribution; full workspace/resource/security regressions | PASS |

No new store/table/SQL column/schema version, dependency, toolchain or protocol.
The bounded span list is additive to existing turn JSON; one partial expression
index supports the existing event owner's operation count. The native-compaction
capability accepts a dispatch callback; None/local checkpoints do not fabricate
HTTP counts. Automatic title remains database-free and requests accounting through
its existing owner channel, with the accepted operation ID captured before send.

## Checks

Current compiled source was qualified before implementation commit, explicitly
associated with R1 base plus reviewed diff, then committed unchanged. Full command:
`cargo test --workspace --locked --no-fail-fast`, exit **0**, **1321 passed /
0 failed / 10 unchanged opt-in ignored**. Managed log:
`/home/opencode/.local/share/opencode/tool-output/tool_0f01e69e0001c8Ye4T94bqB80K`.
Coordinator independently verified all 42 successful target/doc-test summaries.

Cargo ran serially, `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=1`,
`CARGO_NET_OFFLINE=true`, owned
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
900000 ms command ceiling. Other successful commands, all exit **0**:

- `cargo fmt --all -- --check` and strict
  `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- `cargo build --locked`, `cargo build --release --locked`; normal debug/release
  `--help` and `--smoke`.
- Coordinator targeted `oc-adapters --lib ret01_` **17 passed**, integration
  `runtime ret01_` **4 passed**, `oc-tui --lib retry` **3 passed**.
- After final normal build, without another build between checks,
  `python3 -B evidence/T54/native_runtime.py target/debug/oc` and release
  counterpart: **9 scenarios per ELF**, exit **0**. Actual main/child/compaction/
  title POSTs: debug **18/2/4/8 = 32**, release **18/2/4/9 = 33**. Dispatch records
  agree with sockets; each ELF has one Bash append and one subagent intent.
- Python bounded-live/code-size/progress/docs suites **47 passed**; docs/progress
  structural checks, diff check and advisory changed-code inventory exit **0**.
  Largest changed owner 4140 physical lines; no >5000 warning.

Normal retained binary SHA256, unchanged after independent native proofs:

```text
debug   be0ef6d7a14dbffcdc392ab69cbe07ba27c32cbcfaf6f8f159f42da3495469f3
release c23a8b524661ced3bbecf04cadd2f7608383ef0f80a4eaebebc943483c9c2504
```

## Risks

Failed experiments and their diagnoses are retained in both detailed reports.
The Content-Length coalesced stream RED exposed synchronous producer starvation;
line/event-boundary yielding now delivers all 4096 deltas without changing queue,
byte/event caps or test deadlines. Delayed same-span retry revival and partial
continuation budget refusal were reproduced and fixed. A read-only title concern
was withdrawn: manual regeneration is an independent operation, automatic title
retains the exact accepted root operation even after completion.

Exhaustion and jitter use explicit deterministic policy inputs, not a long paid
benchmark. Native cases exercise real bounded waits. Restart does not dispatch
cached retries even after due. The kill/send title race is observed (0 versus 1)
and counted, never normalized. R1's direct physical-attempt-phase fixture remains
historical; current runtime-native proof is `native_runtime.py`.

No live/paid request, user configuration/environment inspection or live campaign
reset occurred. Existing campaign **8 generation / 1 search / 19 control** remains
unchanged. Source/style/history/trust/permissions/unknown-effect/cancel invariants
remain; native closes a response before tool admission as an explicit donor timing
difference. No known blocker remains in assigned T54 backend scope.

## Next

Finish T54 through the journal and normal own-branch delivery. Continue an independent
ready backend task T45/T50/T53. Preserve owner PAUSED for T44 until explicit resume;
complete its VIS43/other V09 and all remaining product gates separately. Do not
reinterpret this report as whole-product READY.
