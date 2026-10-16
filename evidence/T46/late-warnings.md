# T46/R2/R4 late async MCP warnings — frozen atomic scope

## Coordinator review and qualification

The coordinator independently reviewed all changed runtime/owner, native fixture,
limiter/harness and test paths. The 554-line stdlib interposer and its 376-line
offline socket/process suite were reviewed separately from the production fix.
No new product quota, model route, state owner, dependency, schema or broad
exception handler was introduced. Typed degradation union keeps the original
request lease immutable; retirement/fatal/cancellation paths remain non-success.

The current-source workspace log below was independently checked: **1262 passed,
0 failed, 10 ignored**. Nine ignores are pre-existing opt-in gates; the additional
ignore is the new explicit R4 live runner, not a disabled failing regression.
Coordinator reruns passed: Python limiter 9/0, typed warning union 1/0, both
fresh/existing wrapper race 1/0, strict native R4 1/0, Python repository utilities
5/15/14 and docs/progress/diff checks. After the final root `cargo build --locked`,
direct compiled envelope cases **5/0** and native lifecycle cases **4/0** passed,
with debug/release fingerprints below identical before and after. The native
proof independently observed 5 generations / 1 search / 1 warning; restart
observed 3 → 4 POSTs including title/retry; the coding fixture observed 14/1.

Live input preflight used only named permitted product inputs and metadata:
the approved `.local/live.env` supplies the three required OpenProxy/model fields.
The explicit historical CRW association is recorded in `evidence/T27/checkpoint.md`
and its named product `CRW_API_KEY` is present in the launch environment.
No values, full environment, authoring configuration or raw response were printed.
The env-file reader uses literal final-assignment precedence without executing or
expanding shell code. **No external request has yet been made in this phase.**
The next step uses one independently verified durable campaign identity/journal,
never a fresh counter after interruption. These are readiness facts for executing
R4, not a live PASS or full T46/T44/V09/product claim.

## Result

Base `e739cfbf387e8b869aa4920cebb7150010f2712b`, inherited six-file trusted
bounded-envelope worktree. Existing runtime wrappers snapshot MCP warnings
before `run_turn_inner` awaits the provider (`runtime.rs:1527–1547,1625–1649`).
An optional server failing during that await is visible in its typed owner
publication but absent from the completed first headless turn's warnings.
Prior offline R4 experiments completed catalog/search yet observed no warning.

Frozen repair, implemented and qualified offline: keep initial typed degradation and, at completion, merge only
latest typed degradation from the exact owner/binding captured after
`request_mcp`. Deduplicate by RuntimeError identity before safe rendering.
Do not await startup/catalog, change the held request's clients/tools/guidance,
merge a new owner/Location/instance, or demote fatal cleanup/cancel/quarantine.

Both wrappers retain the exact `Arc<McpOwner>` and `McpBinding` after
`request_mcp` returns (it may retire/replace a poisoned owner). The private
`McpOwner::completion_warnings` reads only `Publication.request.degraded` under
one short publication read lock, releases that lock, unions with the held
lease's initial `Vec<RuntimeError>` by typed equality, then safely renders.
Await order: request lease → turn inner → same-owner diagnostic snapshot →
drop attached lease → existing poisoned/fatal retirement → extend report.
Model-budget warnings remain in the report; client/catalog/guidance ownership
and fatal return paths are unchanged. No `request_view`, startup await or new
client lease is used to fetch completion diagnostics.

## Checks

Deterministic RED observed: actual late stdio failure published while provider
was held; completed turn warnings were `[]` instead of the one typed safe
degradation. Original command/failure and later fixture diagnosis preserved in
`late-warnings-red.log`. Minimal same-owner merge now passes both fresh/existing
wrappers; private typed union/binding/fatal-preservation test passes. The native
published-status barrier proves responsive modal, immutable held catalog,
visible late warning, healthy next request, failed-server zero tools and reaping.
Strict same-envelope native offline R4 passes: actual crw/codex catalogs,
one completed search, unavailable initialize attempt, one headless warning and
completed native turn. Final public-binary R4 proof: **5 actual generations /
1 actual short search / 1 unavailable initialize / 1 stderr warning**, crw and
codex catalog IDs advertised; unavailable tools never advertised. Earlier
targeted runs used 5–6 generations depending on independent startup timing.

Executed gates (all offline; `CARGO_NET_OFFLINE=true` additionally set):

| Command / proof | Actual result |
| --- | --- |
| `cargo test -p oc-adapters --locked --lib completion_warning_union_is_typed_and_exact_binding_scoped` | 1/0; initial identity once, changed typed failure retained, Location/generation/instance mismatch rejected, different owner rejected, fatal still fatal |
| `cargo test -p oc-adapters --locked --test runtime` | 94/0; new case loops existing + fresh wrappers, plus existing cancellation/caps/quarantine/AUD23/model-budget gates |
| `cargo test -p oc --locked --test mcp_application` | 41/0; native published-failure barrier with healthy/slow/failed, held request immutable, next healthy request, actual PID reaping |
| `cargo test -p oc --locked --test live_bounded` | 14/0/3 ignored; includes envelope 5/0 and original five-step branches |
| `python3 -B scripts/test_bounded_live.py -v` | 9/0; actual concurrent 24/4, reserve/crash/resume, identity/trust/I/O/redaction/stream/cancel/shutdown bounds unchanged |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS; test slice clone / adjacent unsafe comment fixed, no lint override |
| `cargo test --workspace --locked` | **1262 passed / 0 failed / 10 ignored**, no new ignore in this repair |
| `cargo build --locked`; `cargo build --release --locked`; both `oc --help` | PASS |

Workspace execution log: harness-managed
`/home/opencode/.local/share/opencode/tool-output/tool_0ebd419fb001BegVVxOdCqTZHr`
(1531 lines, bounded below 16 MiB). No new large repo capture or source-hash manifest.

After the last root build, direct compiled targets (no subsequent root Cargo):

```sh
target/debug/deps/live_bounded-508a2d8c4345c739 --test-threads=1 --nocapture
target/debug/deps/mcp_application-c554f016ee604ae9 lifecycle:: --test-threads=1 --nocapture
target/debug/deps/runtime-9be1148df48122ea mcp_lifecycle::mcp_late_failure_reaches_completed_existing_and_fresh_turn_without_startup_barrier --exact --test-threads=1 --nocapture
```

Results: **14/0/3 ignored + 4/0 + 1/0**. Native fixture's
`CARGO_BIN_EXE_oc` points at the final `target/debug/oc`. Guarded original
five-step campaign still **14 generation / 1 MCP**; two actual native/helper
restarts share one durable ID, real retry + automatic title give first **3**
generation POSTs and combined **4**. Actual 25th/5th dispatch remains refused
with peer/journal counts **24/4**. Expected caught panics in negative campaign
tests remain passing non-success assertions.

Final public artifacts, SHA256 identical before/after direct proofs:

- debug `0b263d57efd089bdc7ff4cde23afd810a5ba01b3b5b3a806d2e89dfd589fd56f`
- release `b0c394701711ebb5efe37f6afcd03cdf12bb9461f2da71b8cb5dc881417479b3`

These are final normal-build artifacts; the prior test-profile ELF is not
claimed to be the final app. Reviewed scoped diff / `git diff --check` PASS;
HEAD remains the base above, all changes uncommitted.

Preflight: uid 1003, available RAM 6.9 GiB / disk 173.7 GiB. Cargo serial
`CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1`, owned
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`,
tool timeout 900000 ms. Only fake/local peers and owned fixtures.

## Risks

Live NOT_RUN. No production request quota or new owner/registry/API. Existing
limiter identity/fsync/bounds/redaction remain mandatory. Unknown external tool
effects are not replayed. No task/goal/acceptance/delivery claims changed.

All newly owned test children/helpers/native processes are reaped through their
owned handles and actual PID assertions. A broad final UID inventory also found
four **pre-existing** `oc` processes with PPID 1: 668193, 1522015, 2225209,
2225236, aged approximately 21–46 hours (predating this repair). They were not
mutated; this report does not claim the whole account has zero inherited processes.
This observed inventory is not an external blocker for the qualified atomic fix.

## Next

Return sole mutation/Cargo/fixture/build ownership to parent for scoped review.
Trusted limiter plus native late-warning/offline R4 prerequisites are GREEN;
real R4 is **NOT_RUN**, full T46/T44/V09/READY remain parent-owned.

Parent next: reuse the independently verified existing campaign ID/root/journal;
never initialize a replacement to replenish spent counts. Required names:
`OC_LIVE_CAMPAIGN_ID`, `OC_TEST_MODEL`, `OC_LIVE_UPSTREAM_JSON`,
`OC_LIVE_OPT_IN`; optional `OC_TEST_VARIANT`, `OC_LIVE_SUMMARY`. Exact provider
generation/discovery URLs and crw/codex/unavailable URL/header associations stay
in RAM as documented in `live-envelope.md`. Parent-only explicit command:

```sh
OC_LIVE_OPT_IN=bounded-v1 CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924 \
cargo test -p oc --locked --test live_bounded live_bounded_r4 -- --ignored --exact --nocapture
```

No real environment/credential/configuration source or external API was read or
used in this atomic repair. Parent's separately approved live source/authorization
is required for that future command.
