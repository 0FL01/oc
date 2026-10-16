# T27 — frozen native live workflow

## Obligations frozen before experiments (2026-09-30)

Source HEAD `e2dbb8b1fcda0bc5d09b6d256ae80517c61ab192`; Rust source is unchanged
from `5e523a0df14b01e25b49e6d84abcbce5cd3faf46`. Retained release ELF SHA256:
`acdb29ce8b9a5dd42a29eb164b147e24183d0536f2c988d4216c4611fffcaec1`.
Only a test operator and this additive evidence may change in this slice.

- E2E02/A09: isolated seeded dependency-free Rust `add` bug; actual native read,
  model-authored apply_patch, canonical shell cargo test, genuine final. Unmodified
  independent fixture tests and foreign-file hashes verify behavior/API. Canonical
  shell is the current catalog; legacy bash shares its executor, not a second schema.
- Same session, new native process: next test command succeeds, earlier settled
  operations/turn/history remain unchanged and matching call/result graph is real.
- E2E04/MCP06: native webfetch of public `https://example.com` and exactly one
  advertised codex_web short search, durable successful outcomes and source inputs.
- Native model compression of a closed earlier span, smaller actual outbound
  projection, immutable history, successful continuation and post-compression
  new-process continuation when the unchanged allowance admits it. Missing facets
  are reported individually; no aggregate success from prose or partial work.
- Browser remains disabled; no opt-in browser claim.

Existing diagnosis identity/root only, nonce
`39d44cb58b834de99544daf3c2eedab1`. Observed preflight:
**18 generation / 8 control / 0 MCP / 44007 input bytes**, journal 3612 bytes.
**Six physical generations remain**, including title/retry/reopen/compaction.
Reservation 17 is unknown/reserved and remains consumed. Preserve all prior bytes.
No initialize/reset/refund/new real campaign, no other-model or protocol fallback.

Only named literal product inputs from approved `.local/live.env` enter RAM through
the existing final-assignment-wins shlex parser. Full configured model Default is
admitted by guarded catalog; no stripped wire ID or invented limits. Codex target
association reuses the reviewed product source base + `/mcp` from T46 input helper.
Provider/catalog/MCP traffic goes through existing `bounded_live.py` Relay;
credential placeholders/loopback routes only on disk. No raw live output capture.

Bounds: existing generation24/control128/MCP4; output2048 per generation;
request1MiB; input aggregate8MiB; response8MiB; subprocess300s and whole operator
campaign900s (including catalog/startup); drained stdout/stderr16MiB each. Service
deadlines and Relay caps unchanged. All owned groups/workers joined. Owned fixtures
retained privately under preexisting cache bench, no user workspace/model edits.

Plan: offline native/guard/MCP/web/effect/graph/compress/restart proof first, zero
real reservations; then one opt-in actual configured binding. First read; next
generation asks ordered authored patch then shell test plus independent web/search;
finish; reopened process asks compress and next test then finish. A third process
continues after compression only if a generation remains. Titles always counted.
Any unexpected failure is inspected via fixed typed state/receipt metadata before
further experiments; no blind paid retry.

Direct commands:

```sh
python3 -B evidence/T27/live_workflow.py --offline
python3 -B evidence/T27/live_workflow.py --run
```

## Result

Coordinator review: the helper's explicit relay, native call/result graph,
filesystem assertions, process ownership and accounting were independently read.
The read-only `--audit-fixture` check of the retained real fixture passed: seven
completed operations, two completed turns, one patch, one compression block,
both shell exits 0 and zero retry events. It dispatched **zero network requests**
and independently confirmed the same exhausted 24G/15C/1MCP journal, with
reservation 17 still consumed. The four Python suites passed 47 tests; docs,
progress structure and diff checks passed. No Rust or compiled input changed;
the current T55 workspace qualification is not relabelled as a new build gate.
This independent verification does not fill the missing post-compression live
process check or mark the task done.

At freeze: pending, no PASS claim. Actual run completed on the retained artifact:

| Required facet | Result | Independent owner facts |
| --- | --- | --- |
| E2E02 / A09 seeded Rust coding | **PASS** | Real configured model read the bug, authored the patch, invoked shell tests, reached genuine final; same-session new process ran the next test successfully. |
| E2E04 web workflow | **PASS** | Native webfetch and exact codex short search completed; original source URL/query/short selection remain in durable operation inputs; nonempty actual outputs, not generated answer prose. |
| MCP06 real search | **PASS** | One guarded tools/call, HTTP 200, one exact completed `codex_web__search`; no replay/search retry. |
| Native compression / continuation | **PASS** | One completed compress, one durable block, 71163→41079 measured bytes, 7450 saved estimated tokens; next actual outbound input has the summary and is smaller (64658→42086 serialized bytes). |
| Separate native process **after** compression | **BLOCKED_ALLOWANCE** | All 24 physical generations consumed. No third live process or additional dispatch attempted. Offline third-process proof passes, but is not substituted for the missing live facet. |
| Browser smoke | **NOT_RUN_DISABLED** | Disabled native config, no browser opt-in or npx invocation in this slice. |
| Aggregate frozen T27 workflow | **BLOCKED / partial** | No full-task or full-goal readiness claim; post-compression new-process qualification still missing. |

Only `ocg/muse-spark-1.3-contributor` / Default generated. Guarded `/models` returned
44 entries; configured full literal ID and real positive 500000 context were admitted,
output bounded to 2048. Incident metadata was validated by the reused catalog helper,
but no incident generation was repeated. No variant effort overlay or fallback.

The sole live command exited **1**, deliberately denoting the missing aggregate
facet. Both actual native processes exited **0**; first/second turns are completed
with final assistant spans `completed` / `stop`. No typed structural/observer error,
retry or watchdog timeout. Genuine terminal responses completed on every generation,
including the title. The successful live multi-call generation contains the actual
authored patch followed by shell/web/search; no helper-generated paid tool call.

Actual file change, independently compared with seeded bytes:

```diff
 pub fn add(a: i32, b: i32) -> i32 {
-    a - b
+    a + b
 }
```

Unmodified external fixture test exercises `(1,2)`, `(-4,7)`, `(0,9)`, `(8,-2)`
and reversed operands through the public `add(i32,i32)->i32` API. Seeded test exit
101; native shell test, independent post-fix Cargo test, and reopened native shell
test each exit 0. Exact command: `cargo test --offline --quiet --jobs 3`;
independent/seed test processes use `RUST_TEST_THREADS=1`; the native shell's
existing minimal environment drops that variable, and the fixture has only one
test case. Installed 1.93.0 toolchain, no dependencies/downloads.
No edits outside the isolated fixture; tests, Cargo manifests/lock and foreign
sentinel bytes all unchanged, no additional non-target files.

| File | SHA256 after (foreign files equal seeded bytes) |
| --- | --- |
| `src/lib.rs` | `821d282d75c051d9a2a445ad8ef1551004aba5b3322d7a354c8a2abcd15af1e6` |
| `tests/add.rs` | `0ebbc1ee394d72b1bc29338333e5fd63ba9ae296434bd542c7b1c49ce0887b09` |
| `Cargo.toml` | `e843cf5147a68e2d16015bd4a4f4345196908c98e4b0fa69f57480b2ec1766af` |
| `Cargo.lock` | `32e618802f08e25a1927471ea3088b95c2714cfd2731f37deb6b0f7a146000dd` |
| `foreign.txt` | `1c202d314a83ea64332df5d8b53198487f87cb7fb39ab5736a454f0ac7be1a3f` |

Private retained live fixture: cache bench `t27-native-hgm5cdoh` (15917467 bytes).
Session `s-1790776000472487156`; turns
`ts-1790776000472487156-1790776000562-1719937-0` and
`ts-1790776000472487156-1790776017280-1720304-0`.
SQLite read-only audit: seven completed operations in order:
`read → apply_patch → shell → webfetch → codex_web__search → compress → shell`;
one patch-effect row, one compression block, four immutable raw-history rows.
Original five operation rows, first complete turn/result and initial history rows
were byte-equal after the second process; no effect replay. Durable function
calls/results pair one-to-one and operation IDs suffix-match call IDs; actual
outbound requests independently validate matching call/result IDs.

Initial tool call IDs (patch/read/result graph receipt):
`call_01a0f2914d2770c2b9f4a0afa396134c` (read),
`call_01a0f291642d72feb53a5b4d4c19cc76` (patch),
`call_01a0f29165f471b7b37c28246c35c850` (shell),
`call_01a0f2916705713292fd127c513f3fc5` (webfetch),
`call_01a0f291685172dca8f94ddb67faae23` (search).

### Physical accounting / cleanup

Same existing nonce/root: **18G/8C/0MCP/44007B → 24G/15C/1MCP/301273B**.
Remaining: **0 generation**, 113 control, 3 short searches (without a generation
allowance these are not permission for auxiliary probes). All original 3612 journal
bytes unchanged; final journal 5561 bytes. Unknown/reserved attempt17 remains
spent; no refund/reset or second real campaign.

New reservations: 27 catalog control; 28/31/32 first native MCP initialize /
initialized / tools-list control; 29/30 first-main and title generation; 33 batched
tool generation; 34 one short MCP search; 35 initial final generation;
36/38/39 reopened MCP initialize / initialized / tools-list control; 37 reopened
compress+next-command generation; 40 final generation. All completed HTTP200,
except initialized notifications31/38 HTTP202. Reserve+fsync-before-dial is the
unchanged existing guard; native requested cap2048, title256. Input increment
257266 bytes, all existing request/response/aggregate bounds retained.

Owned helper PID1719821/startticks145101011; native PID1719937/startticks145101068
and PID1720304/startticks145102747; seed-test PID1719831/startticks145101034;
independent-test PID1720295/startticks145102743. All exited/joined; observer server
and non-daemon workers closed. Native stdout drained572/406 bytes, stderr30/30;
no raw response/body/auth/env captured in terminal or Git. Whole900s and per-process
300s watchdog bounds retained; no timeout. Source HEAD and ELF digest verified
unchanged after live. Six retained owned fixtures total 88867819 bytes (~84.8MiB),
under the remaining own-artifact allowance; raw native databases stay private.

## Checks

Initial exact HEAD/release digest and existing ledger preflight PASS; zero network.

Offline operator qualified before external dispatch on the actual release ELF:
`python3 -B evidence/T27/live_workflow.py --offline` exit 0. Owned synthetic
campaign: 7 generations (including title and third native process), 8 control,
1 short MCP search. Native seeded tests red (101), model-shaped read/patch/shell
tests green, independent unmodified tests green (0), immutable original rows,
paired graph, compression 33431→3266 bytes, and actual post-compression third
process using reduced projection all verified. Fake webfetch is explicitly a
negative private-host refusal: production application hardcodes its SSRF exception
false (`application.rs:1140`); zero fake web GETs. Positive webfetch stays live-only,
not silently claimed from fake response injection. Real ledger unchanged at
18G/8C/0MCP/44007 bytes throughout every offline attempt.

Preserved local offline attempts (not external blockers):

| Owned fixture suffix | Observation / correction |
| --- | --- |
| `sq5_abte` | Agent `bash:false` also denied canonical shell by existing alias policy; remove the redundant alias flag. Zero tool ops. |
| `0fqcmhhr` | Strict codex profile needs an explicit auth placeholder; add synthetic offline auth. Only read completed. |
| `c_g5qilt` | Catalog absent in the next very fast fake request; correct fake-only first-response readiness and standard optional GET 405. Only read completed. |
| `iybanu8f` | Native private-host webfetch refusal; preserve it as explicit offline negative, never enable a product SSRF bypass. Other native tools and final completed. |
| `4l6bh5gv` | Offline qualification PASS, three completed turns and one patch effect; positive web gate honestly NOT_RUN. |

All five fixtures remain private under the cache bench, approximately 73 MB (70 MiB)
combined, including derived Cargo targets. No raw live captures. Every owned
native/helper group and HTTP worker closed/joined. Python guard suite: 13 passed;
operator syntax, release `--help`, Git diff check, read-only progress check PASS.

Final checks: `python3 -B -m unittest discover -s scripts -p 'test*.py'`
**47 passed**, `scripts/check_docs.py`, read-only `scripts/progress.py check`,
syntax, release help and diff check PASS. New `--audit-fixture` mode independently
read both offline-qualified and live-owned SQLite stores: paired graph, two live
completed turns, both shell exits0, one patch/block, short search/source input
retained, zero retry; **zero network and real ledger unchanged**. Production code
and binaries were not changed; no repository Cargo rebuild or giant suite repeated.
Post-live helper edits add only read-only audit/explicit synthetic proof labels and
correct the fake-only summary to name its private-host refusal. Published real-API
gate IDs are NOT_RUN in synthetic output. These reporting/fixture-only edits do not
change the paid execution path or reinterpret the recorded historical measurements.

## Risks

The actual normal coding path plus title and compression/next-command used all six
remaining generations. A separate successful process after compression requires
at least one generation not present in this ledger. This is an observed external
allowance dependency, not an API/model outage or an unexamined harness failure.
No other model/variant is qualified by this T27 run. Prior failed attempts remain
preserved. The exact-text fixed Rust fixture assertion is code/effect verification,
not an expectation about generated natural-language answers.

## Next

Parent: review/deliver the helper and factual partial evidence before any task
closeout. Do not mark full T27/GOAL ready from the three PASS gate receipts while
the frozen post-compression live restart facet is missing. Any further paid
qualification requires separately explicit owner authority for an allowance; this
operator neither requests nor creates a new campaign. Continue independently
permitted main-plan work (such as T50); T44 stays PAUSED until explicit resume.
Temporary sole mutation/native/live ownership released after this report.
