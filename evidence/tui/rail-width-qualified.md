# Durable tab-rail width — qualified behavioral slice

T44 remains **ACTIVE / NOT_PASS**. This closes the native restart-width defect,
not full R3, VIS44 pixel parity, the other frozen outcomes, or final R6/V09.

## Existing owner and preserved contracts

`TabDeckSnapshot` and the existing version-1 `StoredDeck` carry optional
`rail_width`. Missing legacy fields remain valid and are omitted on serialization;
present values must be integers in the existing 5–72 range. The Location-scoped
`tui.selection.tab_deck:` preference, opaque revision, root verification, pending
adoption transaction, 4096-byte JSON bound and CAS remain the sole durable owner.
Both ordinary saves and the existing picker transaction validate the width.
Delete, move, fork, picker and Location routes retain the accepted field rather
than silently discarding it. No new table, query variant, registry or poll exists.

The private TUI `TabView` separates temporary drag preview from committed width.
Only a real completed drag to a changed width emits `SetTabRailWidth`; a handle
click, drag back to the original width, invalidated frame or cancelled gesture
cannot pin the configured default or write a preference. Down/Drag never enter
the durable candidate. The binary saves the exact existing deck, then applies
the width only after a matching Location/revision/width acknowledgment. Conflict,
wrong-width acknowledgment, read-only, disabled saves, foreign Location and
invalid widths retain committed geometry and revision. Draft, caret, scroll,
root identity, pending adoption and busy-close guards remain intact.

Bootstrap restores width after the admitted catalog/route and before first paint.
Closing the last root preserves the width on sessionless Home; same-Location
history reopen applies the accepted owner receipt. Foreign Location adopts its
own preference, not the previous view's width. Horizontal configuration ignores
the manual value without erasing it; viewport fitting does not rewrite it.
The configured vertical mode and preferred 64-column content-fit guard remain.

**Scope difference:** native reuses its existing **Location-scoped** deck owner;
the pinned Terminal Original stores a **global** layout preference. This narrow
native interpretation is explicit, not a claim of global preference equivalence.
No user configuration, RAW history, provider/tool graph, permissions, secrets,
workspace effects, or authoring-agent settings were changed.

## Tests and current gates actually run

- Legacy-parser/CAS/reopen test first failed on the absent width field, then passed.
  Legacy JSON round-trips unchanged; 4/73/bool/negative/overflow widths reject;
  stale CAS and unrelated Location preservation remain tested.
- The substantial checked-release binary test exercises real Down/Drag/Up and
  the existing CoreApp channel: no intermediate save, exactly one release save,
  conflict/wrong-width/correct acknowledgments, draft/caret/scroll preservation,
  and pre-query read-only/Location/disabled-save/bounds refusals.
- The existing painted rail test retains all geometry/Middle Down assertions and
  now proves preview versus accepted state. The no-Drag/unchanged-width regression
  was RED before the correction and GREEN after it.
- Existing real application tests retain root adoption, deletion, picker,
  per-Location 5/20 isolation, empty-deck persistence and cold restart assertions.
- Closest broad gates: **514 TUI**, **113 binary**, **14 application tab-deck**
  tests and the storage regression PASS; strict all-target workspace Clippy PASS.
- Current full `cargo test --locked --workspace --no-fail-fast -- --test-threads=1`:
  **1841 passed / 0 failed / 11 unchanged opt-in ignores**, all 46 targets.
  `CARGO_BUILD_JOBS=3`, `RUST_TEST_THREADS=2`; internal concurrent scenarios and
  product/test deadlines unchanged. Full raw result: runner tool output
  `tool_12c64ffa8001uCg1kNoi5qprOf`.
- `cargo fmt --all -- --check`, strict `cargo clippy --locked --workspace
  --all-targets -- -D warnings`, locked and ordinary debug builds, locked release
  build and both `oc --help` commands PASS.
- All nine release gates PASS: four recovery/startup tests (19 routes, eight
  GET-only discovery calls, no Responses); actual TERM01; two VIS31 tests;
  actual S07 archive soak and AUD32 memory bounds. Original limits unchanged.

TERM01: baseline/two-PTY RSS 18544/21976 KiB, FD 24→26, threads 8→10, idle 300 ms
zero ticks; raw/control/VT/resize/focus/shutdown/crash/no-replay checked. VIS31:
idle one second zero CPU ticks, 165/250 Hz input/provider burst, worker lag zero.
S07 archive 0→3000: RSS 25288→26840 KiB, HWM 26380→28120, retained 24232 bytes,
window 152/cache 8581, children and lag zero. Actual AUD32 archive 8→3000 paired
16 KiB/log records, two active 1 KiB views: HWM 21208→22164 KiB, DB
942080→152944640 bytes, children zero, original 64 MiB guard. Not a real-user
strace claim. Debug locked ELF restored after release gates, before acquisition;
no Cargo ran during retained-ELF packing/audit.

## Running Original/native proof

Fresh `rail-width-current-001/compact` uses the existing exclusive middle-close
runner, `--rail-persistence true`, real vertical 160×48 → compact 80×24 → restored
160×48, actual boundary drag 42→5, and the same full-frame protocol. Both sides
are `PASS_BEHAVIOR_ONLY`. Native read-only owner snapshots independently show
accepted width 5 after the drag, after natural empty-deck restart, and after
same-Alpha-root history reopen. The original global width is also 5 on repaint.
Both reopened carets are **(10,42)**. Native ordinal is **(2,2)**; Original **(2,3)**.
Before-first-paint ordering is a source/test contract, not fabricated live timing.

Real Middle Down closes inactive/active/last roots through the checked owner;
Up/Drag/LeftUp cannot perform another action. Saved IDs and every settled history
row survive all 20 read-only snapshots. Natural generation-0 exit is code 0;
restart does not resurrect closed tabs; actual Sessions mouse reopening reuses
the original Alpha ID. Each side makes **six seed/title requests: three main,
three title**. Close/reopen/restart admits zero additional provider requests,
tools or project-file effects. DCP preferences remain unchanged. Generation-1
forced capture teardown is separately recorded, not claimed natural.

All **26 complete opaque PNGs** and full styled grids/cursors/VT/inputs/protocol
are retained. Across 13 pairs: **1005/59520 differing cell positions** and
**87718/8027904 differing pixels**; **13/13 cursor pairs equal**. All 26 strict
grid/PNG comparisons remain `DIFFERENT`. Reopen is **42 cells / 2809 pixels**
different. Actual tooltips, compact header/padding/ordinal styling, Home/version,
Sessions and real elapsed/throughput footer differences remain visible/unmasked.
No normalization, tolerance, fake pixel oracle or capture repaint was introduced;
15-second/five-identical-full-observation stability and owned PGACK are unchanged.

## Whole-byte retention and source association

`rail-width-proof.tar.xz` and `rail-width-proof.inventory.json` preserve every
**174 member / 14516460 logical byte**, including direct parent driver/owner files.
Encoded **490740 bytes**, TAR **14561280 bytes**, **five direct same-root/same-side
byte-identical PNG aliases**. Single stdlib USTAR/XZ, original encoded PNGs and
all lexical JSON/VT bytes unchanged. Pre-encoding inventory, whole inverse,
exclusive durable file/parent fsync and independent original-byte restoration
PASS. All prior qualified and failed proofs, raw acquisitions and reports remain
untouched; no cleanup, archival, overwrite or final TMP offload in this slice.

| Actual seal | SHA-256 |
|---|---|
| Archive | `6456a8dddaab68f5744ec2dde2eabd5809762835065ae9927fa40786814fd5c5` |
| TAR | `85f5e448758b7fef07030573e16499dbdfca3a176cbe91ab4d4612bea047d2d4` |
| Member inventory | `209bbcc9d105d85b8c43a367df1d0fe391ce2c015a61947efde22d013224a17b` |
| Canonical 709-input source | `1bff988465b24972092cdf3a279f363ffd2c60b6b75870ebfa6a6266df3bb294` |
| Native ELF | `cc2780e5926f472655bcdbc667f08bb790f13ccbac26987b473276d6af84d0f2` |
| Pinned Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Retainer at pack | `3b8b4d73ba742f63289f2d5f172ddd97fb5c681a0d5ce495bf7dfedd52bc7735` |

Capture base `10d0b0b61687ae559f71e895c9c78330dda4f515`, tree
`0eecc97e95b7f5b4e82aaf186f58bf95a5262978`, dirty diff
`1f30c6241582cbf156963c66d616c97efeaa7f2106333382eefc054ad80038b6`.
All 709 actual producer inputs match exact acquisition JS `localeCompare` seals,
one recorded successful repository `cargo build --locked`, sealed runner and
executables. Donor `2670273ff17da96f85c5826ced57aa1b368754fa` remains pinned.
Post-capture docs/progress are outside producer inputs; no seal rewritten.

Independent report is in approved scratch
`rail-width-independent-20261011-sole-verifier/report.json`: every byte, source,
ELF, comparator field, actual SQLite/WAL, identity/history/effects and prior data
preservation verified. Observed Python peak AS 178184 KiB/RSS 106652 KiB under
unchanged hard AS 384 MiB; XZ decoder 32 MiB/dictionary 16 MiB, member/encoded
16 MiB, logical/TAR 128 MiB, 2000-member bounds unchanged. Actual owned allocation
before these small docs: **1060552704 / 1073741824 bytes**, headroom 13189120.
All `rail-width-` payloads count toward the original cap.

Physical source size: 541 code/test/script files, 292786 lines, 11759019 UTF-8 bytes.
Existing application.rs 5460 and runtime/turn.rs 5307 soft advisories unchanged;
next natural seam remains tool settlement/publication, not a cosmetic counter move.

## Next — whole frozen contract remains

Fix the actual compact rail/header/ordinal geometry and paint/hit-test seam, then
all remaining body/tail/footer/card/Home/read-only/pacing/scroll/Thoughts/family/
forms/DCP/MCP/auth outcomes and supplements. Preserve existing Root/deck/location/
read-only/busy/CAS/history/draft/caret guards and every old failed acquisition.
Run final current-source R6/V09 and all mandatory frozen R1–R6/VIS01–45 gates
before T44 finish. This receipt is not a global pixel or task completion claim.
