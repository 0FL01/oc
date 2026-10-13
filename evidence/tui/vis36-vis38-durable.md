# VIS38 — current-release committed DCP / durable branch slice

2026-10-13. **Bounded slice qualified; T44 ACTIVE_NOT_PASS.** This receipt covers
the current native acquisition owner, actual persisted compression views/cards,
public session switching, restart/reopen, Undo/Redo and new-branch truth. It does
not change the mandatory status of VIS38 in the frozen acceptance registry or
finish T44. Contract: `tui-recovery/ACCEPTANCE.json` VIS38, amendment lines 919–927,
and all applicable frozen R1–R6/safety requirements.

## Actual producers and correction

Acquisition Git producer is `3afe7ff294061499ae14b70c636295481a65d466`, with the
reviewed working-tree changes, **not the later delivery commit**. Actual 715-file
source manifest SHA256:
`ced0ae5ba472051f8a61e7e0378249cc4d81194368b6088fc79c061af9ffbf6a`.
Each fresh episode retains its own acquisition identity/program bytes. Original
source remains pinned at `2670273ff17da96f85c5826ced57aa1b368754fa`.

| Actual executable | SHA256 |
| --- | --- |
| rebuilt native debug | `1bf21826a0f311c88aff4ab0725c15077455baf307ff9da521dd95e55d2bb3e6` |
| rebuilt default native release | `665e9d4043677a7ca0a611b6a6df039cc8ef997782bb3d764a57377fad82b476` |
| pinned Original | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

Actual current-release episode 004 exposed a production defect: the categorical
bar ignored durable standalone `dcp_coverage` when legacy member rows were absent.
`Db::dcp_bar` now uses the existing indexed current-branch message coverage,
after recent-range priority, within the existing 50-position bounded query.
The new independent storage regression expects `░`×20 + `⣿`×20 + `█`×10,
checks zero legacy member rows and unchanged RAW. Actual RED: 101/1 failed;
targeted GREEN: four storage view tests PASS. No new store, production API,
provider route, polling framework or lifecycle owner.

## Current release episode 011

Approved exclusive acquisition root:
`$TMPDIR/opencode/vis38-durable-current-011`; native result is
`dcp-native-vis38-durable-011/result.json`, with the real owned native SQLite data
root under `$TMPDIR/opencode/t44-reference/runs/dcp-native-vis38-durable-011`.
CLI owner option is `--default-detail-only --durable-lifecycle`; the latter is
confined to that bounded detail qualification. Native ingress remains disabled.
The bridge observer opens only that owned database read-only, uses `LIMIT 101`,
and rejects more than 100 rows per table. Tables/IDs/ranges/accounting, not text
keyword estimates, determine branch/card/stats assertions.

The existing public `oc run` / PTY owner performed three real compressions:
single range, multi-range, then recompression of an actual advertised summary.
Measured content reduction, fallback UTF-16/quarter accounting, canonical bars,
typed durable views and immutable RAW checks all passed. Existing restart
continuation intentionally makes one provider request; merely reopening or
displaying committed state does not. Offline loopback only, no paid generation.

The public native PTY sequence at 120×40 then:

1. Reopened the committed third compression and displayed `/dcp` from actual
   stored current accounting; preserved its frozen card/header/metrics/50-cell bar.
2. Used the real session picker to open the separately generated saved other
   session (zero compressions), then returned to the original committed projection.
3. Undid the restart continuation, cleared only the restored composer draft,
   then undid recompression: current typed views/cards/tool projection became
   compressions 1 and 2; immutable RAW and all three tool operations stayed intact.
4. Redid to the exact saved tip projection/accounting/card 3 without execution.
5. Undid those same boundaries and submitted one explicit new user prompt to
   create a branch. Only this explicit submission generated one further request.
   Current run ordinals remained `[1,2]`, accounting matched the restored second
   compression, abandoned third-card totals were absent, redo was unavailable and
   persisted exclusion/inactive-point data retained branch provenance.
6. Reopened the final branch read-only in 80×24, 120×40 and 160×48: provider count
   stayed 14→14 and owner state remained exact. No compression/tool replay.

Actual total: **14 loopback provider requests**, below the unchanged 16-request
cap; **zero restoration/switch/display requests**; one deliberate branch request.
Independent protocol audit: **14 bounded read-only owner observations**, 14
pre-branch RAW messages retained byte/row-exact (16 after branch), and **three
unchanged immutable tool operations**. Every native process exited naturally 0.

## Real native → pinned Original → strict pair audit

The documented acquisition/import/display/pair chain ran against the actual
release and actual pinned Original, using the owner's real arguments:

```text
python3 scripts/tui_capture/dcp_native_qualify.py --oc <release>
  --released-sha256 <actual-release-SHA> --output <fresh-native>
  --default-detail-only --durable-lifecycle
bun scripts/tui_capture/dcp_from_native.ts <pinned-oracle> <fresh-native> <fresh-oracle>
python3 scripts/tui_capture/dcp_display_fixture.py --native <fresh-native>
  --oracle <fresh-oracle> --case native-recompression --output <fresh-fixture>
node scripts/tui_capture/dcp_reference_capture.mjs --oracle <fresh-oracle>
  --reference <pinned-Original-ELF> --output <fresh-reference>
  --case native-recompression --profiles all --display-fixture <fixture.json>
python3 scripts/tui_capture/dcp_display_audit.py --native <fresh-native>
  --oracle <fresh-oracle> --reference <fresh-reference> --fixture <fresh-fixture>
  --released-sha256 <actual-release-SHA> --output <fresh-pair-check-r2.json>
```

Full command JSONs/logs retain the absolute executed arguments. The historical
hard-coded-release 24-case `dcp_pair_audit.mjs` was not substituted for this
documented bounded three-size display audit. Original alone receives the existing
source-traced display fixture; native receives no history/import fixture.

Actual recompression bottom pair at all three sizes:
**14,400 complete styled cells, zero differences in text/foreground/background/
modifiers/width; all three cursors equal; all three encoded PNGs equal;
1,941,888 whole decoded pixels, zero differences.** Independent comparator
recount agrees with the real pair report. Across the successful episode all
**28 full frames / 128,640 styled cells / 17,342,976 PNG pixels** were independently
validated, with full VT/text/render/cursor/protocol/clocks/source provenance kept.
No crop, mask, tolerance, golden substitution, cursor/clock normalization,
PNG reencoding, failure deletion or old capture replay.

## Actual checks and retained failures

New executions, not reused historical check claims:

- Current full workspace: **1,874 passed / 0 failed / 11 ignored / 46 targets**.
- Strict locked workspace/all-target Clippy `-D warnings`, workspace fmt,
  debug locked + ordinary builds/help, release locked build/help: PASS.
- Required actual release gates: **9 passed** in seven sequential command groups,
  including startup, TERM01, panic cleanup, VIS31, S07, VIS38 active/archive and AUD32.
- Final root Python discovery: **71 passed**; final relevant script syntax,
  actual native owner, pinned import/capture and strict display audit: PASS.
- Independent before/after full-byte/PAX/USTAR/XZ/metadata/source/ELF checks: PASS.

The failed current-owner episodes 001–010 and every producer/program/protocol
remain: stale ELF input; legacy-member observation assumption; syntax error;
the genuine production bar failure; lifecycle admission/empty-draft/title/picker
assertion mistakes; missing build artifacts; and the pre-branch RAW assertion
mistake after actual branch execution. Each corrected acquisition used a new
exclusive output name. Two interrupted Cargo checks retain their full partial
logs and factual timeout supplements. The interrupted initial exact-filter RED
wrapper never emitted its argv/completion JSON; that field is explicitly unknown
in the supplement rather than reconstructed from compiler output. Completed RED and complete subsequent
workspace GREEN are separate evidence; no ignored errors or relaxed deadlines.
The first pair audit correctly refused a changed release ELF after feature gates;
a normal locked release rebuild restored the verified qualified ELF before a
fresh read-only pair report. First independent-check missing `stat` dependency
and first retainer import-path failure are retained; r1 checker bytes are preserved.

The entire `target/` disappeared between resource preflights 008 and 009.
Recorded fact only, no cause attribution or foreign-process inspection. Owned
fresh debug/release rebuilds recovered exactly the qualified hashes; baseline
proofs/source/immutable history survived. No unknown provider/tool effect replay.

## Lossless retention, quota and provenance

Only actual obsolete **FAILED / NOT_QUALIFIED** own loose diagnostic
`model-shell-tool-regression-002` was eligible for the existing literal-choice
archiver. Added only that choice and its existing fixture case. Preflight used
`--reserve-bytes 16777216`; explicit `--archive` preserved **310 files / three DIRs /
37,962,964 logical bytes** including original mode/UID/GID/mtime-ns metadata.
Independent manual PAX/XZ inverse checked every original byte/path/DIR metadata.
Archive SHA256 `303ef34ac3f268dc220ed56c4631dee32bab1d8be0afb5c1441737af34c0f48d`;
size 1,550,360 B. Visible failure pointer and all qualification/source flags remain.
Qualified tool regression 006 and PASS-only model episodes were untouched.
Owned allocation went from 1,072,787,456 to 1,035,657,216 B after safe reclamation.

Fresh acquisition retention is `vis36-vis38-durable-001`…`011.tar.xz` and their
original-file/DIR inventories. The `vis36-` prefix is in the unchanged quota scope.
All **959 acquisition files / 135,565,443 logical bytes** are retained, including
all owned native state/database/project/browser roots, producer versions and all
failures. Combined episodes exceed one 128 MiB TAR, so complete immutable episodes
are separate archives; each contains its complete captured profiles, with no
cross-episode sharding or omission. Episode 011: **293 members / 40,604,937 logical
bytes / four exact same-side PNG aliases**; archive **1,428,568 B**, SHA256
`955d9473212254a527e3e6347cb5a80793d13c1506735f0dea2d9bfd7a1139c0`;
member inventory SHA256
`763ebc469b3d9eec58aec9f15e3ce041af6ec6b9b99bfa4d77011b36c89a323f`.
The separate `vis36-vis38-durable-gates` archive retains all full logs, command
JSONs, timeout supplements, before/after seals and exact helper programs.
Completed gate archive: **191 members / 5,760,843 logical bytes / 772,184 encoded
bytes**, SHA256
`0b14dedaa62fa3ae22cdb5f504c577221e20790d7150ce72351adebba9c7a0e8`;
inventory SHA256
`948667127926af433c11b99ef4eee67443daf7032c4812011d4f1c615e351512`.
Its own exact final codec command/output is retained separately in
`vis36-vis38-durable-gate-record.json`, avoiding a recursive self-archive.
Post-bundle allocation was **1,041,903,616 / 1,073,741,824 B**, with **31,838,208 B**
headroom before this factual addition; final actual recount governs.

Encoder-only reuse is `scripts/retain_syntax_proof.py::encode`; run with the
repository `scripts` directory in PYTHONPATH. Independent manual single-XZ
header/footer/index CRC, SHA256 blocks, dictionary/decoder bounds, no-extra-stream,
canonical USTAR headers/order/aliases and every original file/DIR metadata are
checked in memory **and again from durable bytes**, separately from the producer.
Acquisition inverse max observed VmPeak **287,776,768 B**, HWM **272,986,112 B**,
below hard Python AS 384 MiB. Original baseline: **169 prior archive/inventory/
archival receipt seals, 18,059 prior file seals and 221 DIR metadata seals**, plus
all original helper seals, independently preserved; no old report/seal rewritten.
Existing AGPL/pinned DCP provenance remains; added capture SPDX notice is retained.

Caps remain 1 GiB all established owned prefixes including `vis36-`, 16 MiB
member/encoded stream, 128 MiB logical/TAR, 2,000 members, XZ dictionary ≤16 MiB,
decoder 32 MiB, hard Python AS 384 MiB. Approved TMP is accounted separately,
with all qualified bytes durably retained, not used as evidence offload. Final
read-only checker independently recounts allocation; later actual recount governs.

## Continue full T44

Checkpoint **143**, T44 active. Remaining VIS38 qualification includes failure/
cancel/no-gain/protected/nested/control forms, complete public event/query DTO
equality and complete visual/resource/paging coverage; existing fixture-state
tests and this bounded three-size display result are not substitutes for those
remaining gates. Continue every frozen R1–R6/VIS01–VIS45/supplement and final-current
R6/V09. No finish, task switch, acceptance relaxation or paid campaign.
Read-only delivery checker: `python3 -B evidence/tui/vis36-vis38-durable-check.py --delivery`,
before staging and after ordinary own-branch push. Mutation ownership returns to
the parent after this checked delivery.
