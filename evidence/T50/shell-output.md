# T50/R10 — full shell producer atomic

Scoped parent-review corrections and **current-source** qualification are in
[`shell-review.md`](shell-review.md). Receipts/counts/hashes below remain the
original producer delivery phase; they are not retroactively backfilled.

## Result

Frozen before source edits at BASE `9d8b1b8186ec2a140fb28db1681fc58b89086120`.
**PASS — full shell producer atomic delivered and qualified.** Actual drain reads
feed the preceding common atomic's registered writer before recent-window discard.
One capture identity survives foreground/background conversion, live paging,
terminal publication and notices. No new migration, connection, queue or process
owner was introduced. Whole T50/GOAL completion is not claimed.

Source association: BASE above + reviewed DIRTY files. Return HEAD
`718f56e36569db9c1d78d682b3035000844a1a30` adds two owner documentation-only
commits (`2acd10fb8`, `718f56e36`; no crates changes); those changes are preserved.
No Cargo command ran between the last normal builds and their final direct proofs.

| Obligation | Existing owner / observable proof | Status |
|---|---|---|
| Before-discard capture | `shell.rs::spawn_drain_live`, `shell/jobs/{output.rs,output_tests.rs}`; real distant stdout fact >1 MiB survives in one Complete capture, not in its tail | PASS |
| One identity / captured settings | Same Jobs/Db/writer through FG, BG, Ctrl+B, live byte pager, notice and final flush; native controls assert original op/session/location/generation/PID/capture ID | PASS |
| Safe stream admission | `TextStream`, shared `StreamRedactor`; split-secret/UTF-8 tests before disk/hot, benign live suffix and overlapping/repetitive-key regression; serialized observed labels | PASS |
| Bounded resources | Fixed8 KiB reads,64 KiB per-stream recent/combined tail, admitted secret budget64 KiB, synchronous writer/no queue; cap/quota/TTL/reader leases/identity-swap/stalled publication tests | PASS |
| Loss / effects / teardown | Cap/quota/IO/registration failure keeps draining; independent exit/effect facts survive even failure recording the logging event; cancelled/crashed captures Interrupted; owned drains join | PASS |
| Model / history continuation | Both normal ELFs: bounded next request → registered real read/grep distant stdout → continuation; restart/same-ID move/current Deny; same-task A→B→A/manual compact/restart; background running-control history unchanged | PASS |
| Qualification | Current owner22, MCP media3, workspace1448/0/10, strict fmt/clippy, normal builds/help,54 direct normal-ELF cases, Python47 and read-only metadata/diff/size gates | PASS |

### Minimum representation / owner seams

No schema migration or new store is needed. The existing resource descriptor JSON
can carry optional bounded shell stream byte/line counters and an observed-order
format tag; existing shell outcome JSON can carry that same small descriptor.
The artifact itself carries stdout/stderr boundary labels, so provenance does not
require an unbounded index, extra payload file, event stream or SQLite copy.
Legacy records deserialize without these optional fields and are not rewritten.
The resource remains the sole cold payload. The common preparer must reuse that
resource even when the retained tail alone is smaller than configured limits.

### Frozen workloads / counting

Owner/native chain emits >2 MiB stdout in newline-delimited UTF-8 rows, a unique
sentinel after stdout's old1 MiB boundary and >51200 bytes before the final tail,
plus independently labelled stderr. Configured low20-line/4096-byte preview.
The physical next request must omit the sentinel, advertise a registered Complete
capture, pair actual call IDs, and subsequently obtain the distant line by read
and grep. Execution effect counter must equal1. Cap fixture emits20 MiB; quota
uses the existing shared pool; deterministic registration/IO faults retain actual
exit/effects and do not promise missing paths. Held control fixtures observe actual
published bytes before release, retain PID at conversion, and inspect final flush.
Split UTF-8 and synthetic known-secret chunks are tested before publication.

## Checks
Startup: HEAD/status clean except inherited `.opencode` (never opened), uid1003,
verified owner origin, activeT50, T44PAUSED. Offline only; jobs3 / threads1 / approved
disk TMPDIR. Full workspace wrapper1798s / outer1800000ms is a command watchdog,
not a changed product/test deadline. Failed attempts remain immutable.

### RED / material diagnoses

- `shell-red-before-discard2.log`: permanent real Jobs >2 MiB producer failed
  Complete assertion with the old ProducerLimited result. Initial import/visibility
  compile failures are retained separately. `shell-native-red.log` independently
  exposed the same old normal-ELF discard.
- `shell-workspace-current.log.gz`: exit101,965.292s,1446 passed/1 failed/10 ignored.
  Existing TOOL13 background continuation required its immutable running-control
  JSON; generic history had selected the new live artifact. `output_for_history`
  now uses scoped native foreground/background events plus the actual prepared
  native outcome. Shell-specific live paging still uses the resource. The first
  SQL-column mistake and nearest failure remain in `shell-history-route.log`.
- `shell-workspace-route2.log.gz`: exit0,974.593s,1447/0/10, historical before the
  subsequent live-redactor correction; not the current workspace proof.
- Live-view failures (`shell-native-controls-{debug,debug2,diagnose}.log`) showed
  1,099,993 published bytes and24 benign live bytes held until EOF by the old
  maximum-secret-length suffix policy. `shell-red-live-suffix.log` is permanent RED.
  Incremental KMP state now withholds only possible secret suffixes; overlapping
  full matches remain protected. Existing deadlines/assertions/ignored tests stay intact.
- Producer marker creation could not order independent drain readers. The TOOL13
  fixture observes actual cursor facts inside its unchanged5-second bound. New
  projection oracle initially rejected the sentinel in the immutable command;
  it now checks whole-request/cold-padding bounds and still forbids the distant
  sentinel in the tool-output preview.
- `shell-audit-current.log` preserves an audit-only PID-key error; current2 fixes
  exclusion of the running audit's own TempDir. No resource leak.

### Current serial gates (all exit0)

Exact commands/exits/elapsed/log bytes: `shell-gates.jsonl`. The three original
workspace `.log` receipts map to lossless gzip-n `.log.gz` archives. All decoded
logs are complete and below16 MiB.

| Gate | Exact log / result |
|---|---|
| `cargo fmt --all --check` | `shell-fmt-live2.log` |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | `shell-clippy-live2.log`,20.359s |
| Separate workspace precompile | `shell-precompile-live2.log`,95.354s |
| `cargo test --workspace --locked --no-fail-fast` | `shell-workspace-live.log.gz`, exit0,963.102s;42 completed summary targets **1448 passed/0 failed/10 unchanged ignored**, target times959.26s |
| Owner `tool21_` | `shell-owner-live2.log`,22 passed;31.69s test time |
| Actual binary MCP media `mcp12_` | `shell-media-final.log`,3 passed;2.09s test time |
| Normal debug / release build | `shell-build-{debug,release}-final.log`;0.417s /141.985s |
| Normal isolated help / ELF kind | `shell-help-{debug,release}-final.log`, `shell-elf-kind-final.log` |
| Python47 (13/5/15/14) | `shell-python47-final.log` |
| Docs / progress read-only | `shell-docs-final.log`, `shell-progress-final.log` |
| Changed-source size advisory | `shell-advisory-final.log`; no oversized handwritten source introduced |
| Reviewed diff / ownership cleanup / hash audit | `shell-diff-final.log`, `shell-audit-return.log` |

Normal ELF SHA256 (BASE+DIRTY, unchanged after proofs):

- debug: `757cdb5975f5af028286e416f65593762fa089cb264482e78f637e6a3331539c`
- release: `5c533252a7146ca653f37d6101025721905b9438786ac36b30490ed54abbb01c`

### Native physical requests / tools / captures / effects

**27 cases/profile,54 total PASS:** common4, shell cap1/quota1, controls5,
projection1, existing question14 and image/DCP1. Logs:
`shell-{common,cap,quota,controls,projection,question,image}-{debug,release}-final.log`.
The directed shell/common/control/projection subset makes **71 actual physical
loopback POSTs/profile =50 main +20 title auxiliary +1 compact summary**. Question
and image/DCP regressions are additional cases, excluded from that POST sum.

| Native proof (each profile) | Actual requests / graph / resource / effects |
|---|---|
| Full stdout chain |18 POSTs (13 main/5 auxiliary),10 native tool rows; one Complete resource **3,740,084 debug /3,740,094 release bytes**; distant stdout recovered by real read/grep with useful continuation, previews1891/2029 bytes; effect1/producer reaped |
| Head skill / common quota / registration fault |18/3/3 POSTs,10/1/1 tool rows;425108 Complete /0 Quota /70010 Interrupted recorded extent. Fault old registered path is missing and never advertised; renamed orphan remains accounted; exit17/effect1/no rerun retained |
| Shell cap / shared quota |3/3 POSTs (2 main/1 auxiliary each),1 failed shell row each;16,777,216 ArtifactCap /0 Quota retained;20 MiB producer drains through final flush, exit0/effect1; admitted/raw counts and leader reap asserted (legacy JSON reap field null, not a false receipt) |
| FG→BG/live/final/reopen; child; selected cancel; crash; completion |3/6/3/3/3 physical POSTs,2/5/2/2/2 main; six shell jobs across five cases. Same original identity/PID/notice; FG Complete1,100,025 bytes; cancel/crash Interrupted/effects0/no replay; child sibling finishes independently; PTY restore/owned producer reap asserted |
| Busy switch / compact / restart |5 POSTs=3 main+1 auxiliary+1 summary;5 native tool rows, one Complete **2,616,049-byte** capture,2502-byte preview, effect1/leader reaped; issuing generation/capture survives A→B→A; durable compaction and restart without POST/reexecution/cold inflation |

Complete means normalized/redacted admitted text including observed framing.
Debug/release frame-byte differences reflect actual drain interleaving; no total
OS emission order is asserted.

### Resource / cleanup

Owner tests cover split secrets/UTF-8, durable Active prefix/reader leases/TTL
across rename, no-follow directory swap and continued bounded drain, cap/shared
quota, credential-admission refusal, stalled own SQL publication with selected
cancel, and simultaneous registration/logging-event failure preserving exit17/effect1.
Only the sole file holds full text; outcome/TurnLog/notice/provider reuse bounded
previews and small descriptor facts.

All owned CLI groups, supervisor/drain workers, PTY readers and HTTP handlers join
before exact TempDir cleanup. Recorded TempDirs are absent at final audit.
Conservative own log/report/fixture snapshot is under400 KiB (limit1 MiB), all logs
decoded <=16 MiB. No foreign/global cleanup, fixture Cargo trees, root/live/staging/
commit/task-state changes. Existing normal workspace target cache is retained.

## Risks

Capture completeness is separate from preview truncation and process success.
Disk writes apply bounded synchronous backpressure; no unbounded queue or second
capture worker is introduced. Literal secret carry is bounded by admitted secret
length; only possible secret prefixes are withheld. Stream labels describe observed
drain admission, not OS emission. Slow filesystem operations may backpressure
producers; no queued full output accumulates. Signal/timeout/cancel or forcibly
disposed output pipes yield Interrupted captures even with a successful known exit.

## Next

Parent independent review/bookkeeping and remaining cross-slice TOOL21/T50/GOAL
acceptance. This producer atomic is delivered, not whole-task/goal completion.
T44 remains PAUSED and the paid ledger untouched. All mutation/Cargo/offline-fixture
authority is released explicitly with final return; parent may resume then.
