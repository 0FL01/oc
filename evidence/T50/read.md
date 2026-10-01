# T50 R5 / TOOL16 — frozen read obligations

## Result

### Coordinator independent qualification (2026-10-01)

R5/TOOL16's frozen text/directory/image outcomes are verified on parent
`ecf022e8d3a0097626ce25c11b77c5343502264b` plus the reviewed implementation below.
The coordinator reviewed the descriptor/permission boundary, typed local-file
provenance, model/wire budgets, additive paired-log attachment, DCP/fork/compaction
consumers and the shared T45 instruction hook. Invalid persisted images now receive
the same bounded container/pixel/all-frame validation as live reads; no path reopen
or inference from provider text is involved. Kernel-stalled syscalls are not claimed
preemptible: the existing 30-second budget and cancellation checkpoints are cooperative.

Independent commands, all exit 0 with serial offline Cargo, jobs 3/tests 1 and the
prescribed owned TMPDIR:

- `cargo test -p oc-adapters --lib --locked tool16_`: **8 passed**, including the
  five invalid paired stored-image cases and valid no-filesystem round-trip.
- `cargo fmt --all -- --check`; strict locked workspace/all-target Clippy;
  `cargo build --locked` restoring the normal debug artifact.
- Direct `python3 -B evidence/T50/native_read.py target/debug/oc` and the same
  command for `target/release/oc`: **20 cases each**, **44 main + 1 compaction
  requests each**. Real durable call/result association, exact image bytes after
  source mutation/restart/DCP/compaction and no read reexecution are checked.
- Python bounded-live/code-size/progress/docs unit suites: **47 passed**;
  documentation/progress structure and `git diff --check`: exit 0.

The complete current-source workspace qualification is **1380 passed / 0 failed /
10 unchanged opt-in ignored**, independently checked against all 42 summaries in
`/home/opencode/.local/share/opencode/tool-output/tool_0f5c97d65001dd1fdbHhx8tsBp`.
Normal debug/release hashes listed below were unchanged before/after the coordinator's
direct proofs. No paid/live requests, inherited config reads or campaign changes.
This closes only the R5 slice; R6–R8/full T50 and T44 visual qualification remain open.

### Current parent-review repair qualification (2026-10-01)

PASS on `ecf022e8d3a0097626ce25c11b77c5343502264b` plus the reviewed dirty R5
diff. `ReadToolOutput::from_stored` now invokes the exact existing bounded
pixel/container/all-frame validator after source/MIME/length/base64 checks; a
matching original read graph is insufficient to admit invalid stored bytes.
Live `Content::Image` construction uses the already-validated typed file result
directly, retaining the original invocation token and avoiding double decoding.
No file reopen, URI/network loading, model fallback or MCP provenance inference.

The single paired-log risk pack was genuine RED: after correcting missing fixture
envelope fields with `TurnLog::new(...).to_json()`, all five correctly paired
invalid attachments reconstructed successfully (test exit 101). Cases: signature
only, missing PNG terminal record, complete envelope/bad pixels, valid PNG over
the existing decoded-allocation ceiling, and damaged later GIF frame. GREEN:
all five refuse `TurnLog::from_json` before any reconstructed output; valid bytes
round-trip exactly with a nonexistent source path, entirely in memory.

`files/read.rs` also reuses the existing `files/search.rs::Budget` (30 seconds):
one budget spans admitted directory/file reading and live decoding; stored
in-memory decoding gets its own bounded validation invocation. Read/seek and
frame advancement check it, as do scan/chunk boundaries and final decode/page
completion. Original 1 MiB / 8 MiB / 4096 dimensions / 10000 entries stay fixed.
No worker owner, polling thread, subprocess decoder or dependency added.

Final separate normal builds/help exit 0, exact current SHA256:

* debug `7a9d12decc9ea465c510082db15484ef9585ae87aa1ffa855cf60d771d3f5435`
* release `a131fe6085ffa39aaeef6cf4a2ca660babff90ab5d29b7a9d73230f3201f3051`

No Cargo after these fingerprints. Only report/CODE_MAP metadata follows.
Previous qualification/counts/hashes below are historical, not current artifacts.
T50 is not DONE; parent retains independent review/commit/progress and R6–R8.

### Parent-review repair freeze (2026-10-01)

Source remains `ecf022e8d3a0097626ce25c11b77c5343502264b` plus the inherited R5
dirty diff. Previous 1379-test/normal-ELF qualification below is historical while
the paired stored-image validation finding is repaired. Before production edits:
add a genuine `TurnLog::from_json` paired read graph with correct local attachment,
MIME/length/presentation but header-only, incomplete, bad-pixel, over-allocation
and damaged multi-frame bytes. Require rejection with no reconstructed output and
no filesystem/network access; preserve valid exact bytes. Reuse bounded decoder
validation for stored facts, while live execution retains its original cancellation
token and a single pixel-decoding pass. No caps/dependencies/acceptance changes.

Investigate cooperative read timing through the existing scanner budget. A timed
checkpoint cannot preempt a kernel-stalled regular-file syscall; no FUSE mount,
privileged fixture, detached worker or kernel-preemption claim is admitted.
Preflight uid 1003, available RAM 6564 MiB, free disk 174 GiB; serial offline Cargo,
jobs=3/tests=1 and the existing approved disk TMPDIR. Parent stays read-only until
this coordinator returns ownership.

### Historical pre-repair R5 / TOOL16 qualification (2026-10-01)

PASS on reviewed dirty source based on `ecf022e8d3a0097626ce25c11b77c5343502264b`.
Historical freezes/RED below remain historical. T50 itself is not DONE.

| Frozen obligation | Current outcome |
| --- | --- |
| Text/directory pages | Normal debug/release model calls return default 2000 numbered text lines, explicit 1-based pages, sorted directories and next cursors. Existing String APIs remain text-compatible; directory/image rich results use the native read owner. |
| Real images/durability | Vetted decoded PNG/JPEG/GIF/WebP, complete envelopes and all animation frames; ordered Responses `input_text`/`input_image` carries exact original bytes. Separate `native_read_results` preserves local source path/MIME/length/data, original call ID/index and presentation before continuation. Actual ELF restart/DCP/compaction after file mutation retains admitted facts without another read. Focused fork/reopen/DCP/hide/compaction tests retain exact arrays and immutable raw history; local sources never gain MCP origin. |
| Admission/resources/cancel | Common read admission, read-Deny ceiling, descriptor-pinned no-follow regular files/directories and entry admission; own data, ancestor swap, FIFO, malformed/PDF/binary and byte/pixel/model budgets refuse actionably. Shared current-thread runtime barrier now holds an active read, observes bridged cancellation, discards partial output and joins before the next query. Existing AskOnce/Always versus automatic AGENTS Ask remains distinct. |
| Shared instructions | Successful text/directory/image outcomes call the existing T45 `after_read` seam, typed directory scope, provenance/dedup and the existing atomic instruction/tool checkpoint. Both normal ELFs qualify nested text and directory injection; independent Ask-only fixtures preserve automatic-source admission. |

Normal final artifacts (separate locked builds, both help commands exit 0):

* debug `cb14f70a1cb4b8ca3fd22f5bc7a9098554758917f888933ac29e64b1780a6c4d`
* release `3b2714973b7c0980e52537d83c18d5f8cb2a0a47ef59d7d464980736671c14b4`

No Cargo runs after these final fingerprints. Only fixture/report metadata edits
followed; both fingerprints were rechecked at handoff. No commit/stage/progress.

### Resumed implementation freeze (2026-10-01)

Source base `ecf022e8d3a0097626ce25c11b77c5343502264b`; clean tracked
tree, inherited `.opencode/` untouched. Shared T45 prerequisite delivered in
`191e663d6a6fb3e60f83f3a20bb17dc29bee98dc`: reuse `after_read` and atomic
instruction checkpoint, with directory scope typed by the read outcome.
The historical first RED below is retained unchanged. This checkpoint precedes
the resumed actual-binary RED and production edits.

Frozen rows above remain the acceptance. Native limits stay bounded: 65536-byte
rendered page, 2000 entries/lines, existing 1 MiB regular-file scan ceiling and
10000 inspected entries; no donor 20 MiB expansion. Actual retained native facts
and wire copies count toward existing context/request limits. PDF is unsupported.
Image validation requires decoded pixels, not signature acceptance. Cached
`image 0.25.9` (MIT/Apache-2.0), `png 0.18.0`, `zune-jpeg 0.5.12`,
`gif 0.14.1`, `image-webp 0.2.4` are available as registry archives;
smallest planned decoder dependency is exact `image =0.25.9`, default features
off, only png/jpeg/gif/webp. No dependency upgrades/network fetch are planned.
Use decoder allocation/dimension limits and cancellation-aware memory reads.

Preflight uid 1003, available RAM 6885 MiB, disk 192 GiB. One offline Cargo
owner, jobs 3/threads 1, owned disk TMPDIR; no new target trees or retained fixture
directories. R6–R8, full T45, T44 PAUSED and exhausted T27 remain unchanged.

### Historical first admission and scheduling dependency

**Scheduling dependency — required shared nested-instruction prerequisite is absent.**
Coordinator confirmed this is unfinished approved T45/R10 implementation, not an
external blocker. The existing user mandate authorizes continuing that owner slice;
T50 is temporarily suspended through the single progress owner, then will resume.
No production implementation or R5 PASS is claimed. Frozen before RED against source base
`1afc3d46bc4fe8128840855b6d89942a1ab38f19`; tracked worktree clean at
admission, inherited untracked `.opencode/` untouched. T50 active 0005.
R5 is an atomic behavioral slice; none of the rows below is a PASS claim.

| Obligation | Required observable proof |
| --- | --- |
| Text and directory pages | Actual model calls to normal debug/release ELF: default 2000, 1-based offset/limit and line references; sorted bounded directory entries and coherent next cursor. Truthful root/child schemas and effective read ceilings. Existing String clients remain compatible. |
| Real validated image output | Actual selected Responses capability metadata, validated image bytes/MIME, ordered inline image content alongside readable call-output metadata. Original call ID and source facts durably committed before continuation; restart/fork/DCP/compaction preserve the admitted result after source-file mutation without reread/reexecution. PDF, unsupported model/file, malformed image and exhausted budgets are actionable non-success. No MCP provenance invented for local files. |
| Path, policy, resource and cancellation boundaries | Existing central Deny/Ask/Always and General/Explore ceilings before bytes I/O; own-data-root, no-follow ancestors and descriptor-held regular files; FIFO/swap/symlink refusals. Bounded chunk reads, sorted directory scan, model/retained-facts/byte budgets and invocation cancellation through the existing owned blocking-dispatch seam; join before cleanup/finish. No implicit URI/network or model fallback. |
| Shared nested instructions | Successful text/directory reads invoke T45/R10's admitted nested AGENTS lifecycle with canonical provenance and unchanged-content dedup. Root baseline remains owned by the shared runtime/config assembler; no second loader or implementation of all T45. |

Pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`:
`opencode/packages/core/src/tool/plugin/read.ts:15–27,82–105,169–204`
and `tool/read-filesystem.ts`. Native PDF exclusion is explicit.

## Checks

### Current post-repair checks

All command exits 0: `cargo fmt --all -- --check`, strict
`cargo clippy --workspace --all-targets --locked -- -D warnings`, separate
`cargo test --workspace --locked --no-run`, then
`cargo test --workspace --locked --no-fail-fast`: **1380 passed / 10 existing ignored**.
Tool-managed full log (no copied logs):
`/home/opencode/.local/share/opencode/tool-output/tool_0f5c97d65001dd1fdbHhx8tsBp`.
Nearest `cargo test -p oc-adapters --lib --locked tool16_`: **8 PASS**;
separate active-scan cancel guard **1 PASS**, including current-thread read worker
barrier, no queued shell effect/partial result, original cancelled outcome,
joined worker and successful subsequent query without replay.

Separate locked normal debug/release builds and both help commands exit 0.
Post-build direct current-ELF commands, each binary independently:

| Guard | Current post-repair outcome per ELF |
| --- | --- |
| `native_read.py` | 20 PASS / 44 main + 1 compaction request; typed ordered byte-exact image continuation, durable call/output facts, no reexecution across actual restart/DCP/compaction after current file mutation; text/paged directory/instruction/negative cases unchanged. |
| `native_question.py` | 14 PASS / 29 reported main-child requests, including custom Ctrl-C and reopen. |
| `native_search.py --question-supported` | 14 PASS / 31 main-child requests, including root/child semantics, Ask/Deny, no-follow/dataRoot/cancel and scan budgets. |
| T55 existing synthetic `Fixture`, reasoning replay: headless + conflict | 2 PASS / 5 main + 2 title requests; one completed patch/read graph through restart, conflict has zero operations/effects. |
| Direct existing compiled `mcp_application` target, `mcp12_` | 3 PASS using current normal debug ELF: exact mixed media, durable restart and denied/sensitive zero partial success. No Cargo after fingerprints. |

Serial Cargo used `CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1`,
timeout ceiling 900000ms and approved owned disk TMPDIR; actual fixtures use
`PYTHONDONTWRITEBYTECODE=1` and the same TMPDIR. Owned processes and HTTP workers
join/reap before TempDir cleanup; no retained fixtures/new target trees or copied
raw logs. Current Python tooling **47 PASS**; docs checker, read-only progress
check, diff check and code-size advisory each exit 0. Both final fingerprints
rechecked unchanged; no remaining owned read fixture directories/processes.
New report/managed-output footprint remains below 1 MiB, without copied logs.
Prior foreground/BG/T54 qualification below remains historical; it is not
relabelled as a post-repair run.

### Historical pre-repair checks

Current final gates, exit 0: `cargo fmt --all -- --check`; strict locked workspace
all-target Clippy; locked workspace test compilation (separate `--no-run`);
`cargo test --workspace --locked --no-fail-fast` **1379 passed / 10 existing ignored**;
separate normal locked debug and release builds/help; Python tooling **47 passed**;
docs checker, read-only progress check, diff check and code-size advisory.
Cargo jobs=3, tests=1, offline, non-root uid=1003 and approved disk TMPDIR.

Resumed RED before production edits (same source base as this qualification):
default read still returned 50 lines; shared nested-instruction control now PASS.
Two cases / four main requests per ELF, helper exit 1 and application exits 0.
Normal debug was `0805c0bf45c10e6143a45dfffee24ee0e68c14fb292d8bfd956aec8b1a5a6287`;
normal release was `5a0842133cadb545fa0b93256ccf0ed595b3babd70deee7867a7c8b36fcf6ef7`.
The earlier first-admission RED hashes below are not replaced by these.

Full final workspace log (tool-managed, no second retained copy):
`/home/opencode/.local/share/opencode/tool-output/tool_0f5a046730012QCk5f56734FSp`.
Initial workspace run failed four targets on changed read formatting, a Unicode
offset now following the read header, an invalid old limit=4000 fixture and one
response-cancel timeout (focused rerun passed without changing its deadline);
exact wire/UTF-8/budget assertions were adapted, not weakened. Its log remains
`/home/opencode/.local/share/opencode/tool-output/tool_0f58c9b1e001OI40XuMRhUqg6c`.
The pixel-allocation focused RED exposed unchecked direct-decoder allocation;
the existing 8 MiB ceiling now precedes pixel allocation. Final focused TOOL16
scenarios: 7 PASS, plus the shared active-cancel test covering grep/glob/read.

Final direct normal-ELF commands used `PYTHONDONTWRITEBYTECODE=1` and
`TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924`:

| Current guard | Debug / release outcomes |
| --- | --- |
| `native_read.py` | 20 cases each; 44 main + 1 compaction request each. Image restart, DCP and compaction independently mutate current file and verify no read reexecution/unchanged original stored result. |
| `native_question.py` | 14 cases / 29 reported main-child requests each; custom Ctrl-C/dismiss/reopen and child ceilings retained. |
| `native_search.py --question-supported` | 14 cases / 31 main-child requests each; root/Explore catalog, Deny/Ask and boundaries. |
| `native_foreground.py --background-supported --question-supported` | 21 cases / 45 main-child requests each. |
| `native_background.py --review-only both --question-supported` and directed `capacity` | Revert/Fork/capacity: 3 unique cases / 16 main requests per ELF. Initial debug Revert panel-open timeout; isolated rerun PASS, no fixture/product/cap/deadline changes. Release `both` already includes capacity, which was also separately repeated. |
| `native_instructions.py --ask-only` | 6 scenarios / 12 main requests each; source Ask never implicitly opened. |
| Existing compiled `mcp_application` target, `mcp12_` | 3 actual current-debug-ELF media/durability/admission tests after normal builds; direct test executable, no Cargo. |
| T54 existing `Fixture` effect + restart | 2 cases per ELF; 4 main requests each, one committed shell effect, no cached retry restart dispatch. Auxiliary title counts: debug 1, release 2. |
| T55 existing `Fixture`, reasoning replay enabled: headless + conflict | 2 cases / 5 main + 2 title requests each; one patch/read graph through restart, conflicting graph has zero operations. Synthetic only; no relay/live campaign. |

All owned new read process groups and HTTP workers joined/reaped before TemporaryDirectory
cleanup. No retained fixture corpus or new Cargo target tree, no copied raw logs;
new report/managed-output footprint stays below 1 MiB. Existing ignored tests,
budgets and deadlines unchanged.

### Historical first-admission verification plan

Current normal-ELF RED completed; minimal fix/GREEN remains blocked. Pending: affected
Files/tools/runtime/provider scenarios; fmt all check, strict locked workspace
all-target Clippy, locked workspace tests, separate normal debug/release builds
and help, then direct ELF read and directed current R4/search/shell/recovery/MCP/
retry risks. Cargo serial, jobs=3, test threads=1, offline, timeout 900000ms,
owned disk TMPDIR. Reports/logs remain small and synthetic. No live campaign.

Preflight: uid=1003; MemAvailable=6548 MiB; available filesystem space=193 GiB.

## Risks

Current timing/resource qualification is cooperative on normal local filesystems.
No allowed owned fixture established a new regular-file kernel-stall counterexample;
no mount, foreign filesystem or privileged action was attempted. `O_NONBLOCK`
does not promise preemption of a FUSE/network-filesystem regular-file syscall;
30-second/cancellation checks occur before/after syscall or decode boundaries,
and the existing worker owner joins rather than detaching an orphan. Sorting or
decoder work between checkpoints is bounded by entry/byte/pixel work caps, not
claimed kernel-interruptible. A kernel-stalled syscall can delay cancellation and
the owned join; this explicit limitation is not described as solved by a timeout.
Actual FIFO/nonregular refusal and the cooperative active-read barrier are proven.

Native differences are explicit: 1 MiB input scan, 65536-byte rendered page,
2000 lines/entries and 10000 inspected directory entries are retained native
ceilings rather than donor 20 MiB. A single oversized text line gets a bounded
UTF-8-safe preview with an explicit truncation/grep note (legacy String compatibility).
Decoder allocation/dimensions and total animation work use the existing 8 MiB
attachment ceiling / 4096 dimensions. Image sources consume retained fact/wire/
presentation and model-token budgets, including later history projection.

Decoder provenance: exact cached `image 0.25.9`, MIT/Apache-2.0, default features
disabled, only png/jpeg/gif/webp; nineteen new locked packages including the decoder, no
existing dependency upgrades/network. No new crate, schema, media SDK or custom
pixel decoder. T44 visual qualification remains PAUSED; exhausted T27 remains
24 generation / 15 catalog / 1 MCP / 301273 bytes. Remaining R6–R8 are separate.

### Historical first-admission source reconnaissance

Source verification found only baseline instruction assembly:
`composition.rs:630–649`, `defs.rs:1393–1467`,
`runtime.rs:1494–1535`, `runtime/turn.rs:17–29`.
`composition` admits global/root instruction bytes and calls
`defs::load_instruction_texts`; `Runtime::publish_workspace` installs them as
fixed input between turns. `lane_fixed_input` renders that baseline. None of
these implements nested read admission, a per-session admitted source set,
chronological durable nested updates or unchanged-content dedup. The actual
read dispatch at `runtime/turn.rs:2885–2887` invokes `execute_batch`; the current
`tools.rs:838–870` read executor does not call any instruction owner.

Targeted searches across all `crates/**/*.rs` for nested AGENTS, instruction
updates/state and AGENTS.md confirmed those baseline/test references only.
T45/R10's approved contract (`docs/goals/2026-09-21-config-compat-and-subagents.md:116–122`)
describes this lifecycle as a required outcome; a contract is not an implemented
hook. No existing admitted nested lifecycle is available to reuse at this HEAD.
This prerequisite is not waived by read/media tests, and a separate T50
instruction loader is forbidden by this delegation. Implementing a minimal
shared T45/R10 prerequisite needs an explicit amended execution envelope or a
prior owner slice; merely rediscovering AGENTS in `tool_read` is insufficient.

Current read/media facts remain unqualified: `Files::read` reopens by path and
materializes the file before bounding text; directories are refused and binary
NUL is rejected. Images need a vetted decoder, typed local-source facts and
shared provider/log/projection budget lowering, not fabricated MCP results.
No Cargo/dependency/lock/configuration changes were attempted after discovering
the instruction blocker.

T44 visual presentation remains PAUSED. T27's real ledger remains exhausted
(24 generation requests, 15 catalog requests, 1 MCP search, 301273 bytes);
no reset, older allowance, new live campaign or user-environment qualification.
R6–R8 remain separate, and no T50 DONE/product readiness claim is made.

## Next

Parent independently reviews the dirty diff/new read owners and fixture adaptations,
then owns staging/commit/progress and continues R6–R8. This coordinator releases
all mutation/Cargo/native-fixture ownership at return. Full T45 remains pending;
the delivered shared R10 prerequisite is reused, not reimplemented.

### Historical prerequisite handoff

Parent: qualify the minimal shared T45/R10 nested-admission seam (successful-read
hook plus durable source/provenance/dedup owned by the existing runtime/config
assembler), then resume the frozen atomic R5 slice and extend the same fixture
for directory/media/policy/cancellation/replay. Do not mark T50/R5 complete or
weaken the frozen acceptance. Parent owns review/stage/commit/progress; this
coordinator does none of those actions.

## Current direct-ELF RED evidence

Commands used the owned disk TMPDIR and `PYTHONDONTWRITEBYTECODE=1`:

```text
python3 evidence/T50/native_read.py target/debug/oc      # exit 1
python3 evidence/T50/native_read.py target/release/oc    # exit 1
```

`native_read.py` calls the ELF directly, with a synthetic loopback Responses peer
and isolated synthetic HOME/config/store/project. Each guard performs two actual
main model requests and confirms the original `read-call` output matches the
already-committed completed ToolOp before replying to the continuation request.
It excludes bounded auxiliary title traffic from main-call counts. These are
model-driven read results, not schema-only probes or empty-file shortcuts.

| ELF | text_default RED | nested_lifecycle RED | Current SHA256 |
| --- | --- | --- | --- |
| normal debug | 50 fixture lines; required `2000: fixture_2000` absent | successful two-line read; root baseline present; nested instruction count 0 | `3fbf74fbdb651252d9b3b322f57f4c02c47fec0871dfca56700f9d6ba785abaa` |
| normal release | 50 fixture lines; required `2000: fixture_2000` absent | successful two-line read; root baseline present; nested instruction count 0 | `b0081f0e8b5216bd5917705c42c6f562a2b57e8892dab5edd1f64a1019ab6ba2` |

Final counts: **2 RED cases / 4 main requests per ELF**; 4 RED cases / 8 main
requests total. Each application's exit was 0; the guard command exits 1 on the
missing frozen behavior. The first helper attempt rejected a legitimate bounded
auxiliary request; that fixture issue was corrected before these source REDs.
HTTP workers and the owned application process were joined before every TempDir
cleanup, including failed assertions. No live calls, shell effects or outstanding
fixture resources remain.

Nearest source corroboration is `tools.rs:856` (`unwrap_or(50)`),
`runtime.rs:439` (advertises 50), and the baseline-only owner chain above.
No Rust was edited, no Cargo command ran, and full workspace log/build/green
qualification is **NOT_RUN_BLOCKED**, not inherited from historical R4 evidence.
Only the new report and reusable direct-ELF guard are dirty; no CODE_MAP production
path changes are needed. All mutation/Cargo/native-fixture ownership is released
to the parent at handoff.

Final structural checks: `python3 scripts/check_docs.py` exit 0;
`git diff --check` exit 0; HEAD unchanged. The owned TMPDIR has no remaining
`t50-read-*` fixtures. No stage, commit, push, progress, GOAL/spec or inherited
`.opencode/` operation was performed.
