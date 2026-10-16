# VIS41 — busy hover presentation without close authority

Date: 2026-10-13. Task: T44 **ACTIVE_NOT_PASS**.
Base / captured producer: `e6ef87ae16e2ba1a6c5f7f81159a1eda9f8ed5bc`.
Pinned running Original: `2670273ff17da96f85c5826ced57aa1b368754fa`.

## Contract and delivered behavior

Frozen VIS41 requires hover presentation/marquee independently of close eligibility.
VIS44 preserves the native busy-close refusal: a painted cross is not permission to
abort or close an active turn. The original Prompt pairs recorded the same SGR
Add/attention clicks but Original painted a hovered cross while native's shared
busy eligibility also suppressed the visual overlay.

`tab_close_overlay_cell` now owns presentation geometry: valid painted slot, Home,
modal coverage, hover and compact-rail clipping remain guarded. Only shell painting
uses it. Public `tab_close_cell` still applies the unchanged `tab_close_eligible`;
left and middle hit tests and the binary close owner retain their existing busy,
permission, Location and lifecycle boundaries. No cancellation, authority, RAW,
schema, config, auth, environment or admission policy changed.

The existing unit scenarios now require the busy cross to paint while close-cell
eligibility stays absent, middle click emits no intent, and left click activates
rather than closes. The actual held-provider PTY scenario clicks the painted cross:
one request remains held, no continuation or abort occurs, and release completes
the same turn with the same persisted user/assistant pair. Animation, one-cycle
marquee, timing/resource limits, cleanup and terminal restoration checks remain.

## Experiments and current code checks

- Actual RED: busy overlay was blank; one test failed before the owner split.
- Targeted green: busy overlay, shell overlay, 21 tab action guards and the real held
  PTY visual/action contract. No new test target or ignore was added.
- First complete workspace: **1866 PASS / 2 FAIL / 11 unchanged ignore**, 46 targets.
  The two failures were old shell/PTY assertions forbidding the visual cross while
  busy. These contradicted VIS41; only the visual expectation changed, with real
  busy-click refusal retained/strengthened. No deadline or baseline was relaxed.
- Final complete workspace: **1868 PASS / 0 FAIL / 11 unchanged ignore**, 46 targets,
  842.582 seconds; TUI 534 and PTY 58 counts unchanged.
- Strict locked workspace/all-target Clippy `-D warnings`, fmt, locked and ordinary
  debug builds/help, locked release build/help: PASS. Release build 151.715 seconds.
- **9 actual release/resource cases**, seven complete Cargo release targets: startup
  and discovery (2), TERM01 (1), panic restoration (1), VIS31 pacing (2), S07 (1), DCP
  archive (1), AUD32 (1). No zero-test filter or manual ELF substitution was used.
- Root Python **71 PASS**, including literal campaign/path/build guards and actual
  FAILED-versus-PASS archival refusals. Docs/progress/diff checks are recorded below.

All complete failed and successful command logs are retained; targeted greens do
not replace the failed full workspace observation.

## Complete gate bundle and delivery seal

`vis36-busy-hover-gates.tar.xz` retains **48 complete command JSON/full-log pairs**
and three original helper programs: **99 members / 460,463 logical bytes / zero
aliases**, 59,440 encoded bytes. Docs and progress checks passed. The actual RED
and failed first full workspace both retain exit 101; all 46 remaining command
records retain exit zero. The failed observations remain distinct from the final
1868/0/11 workspace and nine actual release/resource cases.

- Archive SHA: `293627ea6319cc39168ed87f55a17cbe675fde51e79086ba39dec691a1c3fe7d`.
- TAR: 542,720 bytes; SHA `b33cc4d2fd4971387801241b4c56bf6ea71efc7bb33f4ad1453c48d292e640f3`.
- Member inventory: `39b54f02843f85f600b67528e99d0546fcfd487d0d2b271ab8cfc4e3dee61e8d`.
- Independent manual single-XZ/USTAR inverse passed in memory and after durable
  publication for every original command/program byte. Gate VmPeak **245,878,784 B**,
  VmHWM **76,509,184 B**, below hard AS384 MiB. The packer and archive remain sealed.

Read-only delivery checker:
`/home/opencode/.cache/opencode-tmp/opencode/vis41-busy-hover-delivery-check.py`.
It loads only immutable observer readers and selected function ASTs, checks current
003 originals/directory metadata, source/actual ELFs, helper and all 153 prior-proof
byte/metadata seals, historical 002/current 003/gate archives, complete gate bytes
and statuses, FAILED archival seals, checkpoint 141 and actual additive allocation.
It passed before staging; the same checker is run again after ordinary push.
Captured producer identity stays `e6ef87ae…`; the checker does not equate it to
delivery HEAD or invoke acquisition/auditor output entrypoints.

## Running Original / Native acquisition

The existing ordinary Prompt harness ran sequentially at **121×40 and 79×40**.
Current attempt `vis36-prompt-current-003` used fresh exclusive profile paths,
one captured locked native build per profile and both actual executables. Each
origin/profile passed **112 behavior checks** and captured all **51 whole frames**.
Each side recorded 26 valid requests/completions (24 main, 2 title), 14 fixture calls,
34 read-only snapshots and two natural exit-zero generations.

Pending/no-effect and parked Home checks, explicit Once/Reject/Always, persistent
grant/relaunch, mixed Deny, MCP/child feedback, read/Shell effects, glob/URL rejection
and Settings mode changes remain recorded. Native's `apply_patch / approval.txt`
grant and Original's `edit / *` grant remain distinct, not translated. Read-only
SQLite `no_started` observations are producer heuristics, **not CoreEvent or syscall
traces**; typed owner tests establish ordering separately. Frozen Shell success
suppression and native RAW denial copy remain untouched.

Current full independent recount: **102 pairs / 204 original encoded PNGs**,
408,000 styled cells and 55,031,040 RGBA pixels checked; **182,882 differing cells**,
**9,965,501 differing pixels**, **86/102 cursor pairs equal**, zero whole-grid and
zero whole-PNG pairs equal. All **204 strict comparisons are DIFFERENT**.
The six named hovered cross cells match complete symbol/FG/BG/modifier styling at
the same positions. This is **paint-only evidence**, not close authority or whole
VIS41/VIS36/R6/V09/T44 PASS. No crops, masks, tolerance, goldens, cursor/time
normalization or PNG re-encoding were used.

## Byte-exact retention and independent inverse

Per-profile archives keep the unchanged 128 MiB logical/TAR bound. Every original
file, styled cells, PNG, VT, cursor, clock, command, provider/effect record, parent
log and acquisition driver is retained. Only same-run/same-side byte-identical
**original encoded PNGs** use direct archive aliases.

Current wide proof `vis36-busy-hover-r3-wide-proof.tar.xz`:
- 3,659,880 encoded bytes; 634 files / 80,477,583 logical bytes; 7 exact PNG aliases.
- Archive SHA: `a8e4cc8fb96f85dc042002b1849b2f1f566c3d2af06df88a113ec2c3379a0c3b`.
- TAR: 80,680,960 bytes; SHA `0b1d852b40588afd51f517fb5c103e5db984490c54fad76eb3b1d96e2571dab4`.
- Member inventory: `25936ca1646a2fe044bfa1c5fb397d9d9704281052a3b33dfb9488ae0ed9bc7f`.

Current small proof `vis36-busy-hover-r3-small-proof.tar.xz`:
- 3,063,232 encoded bytes; 634 files / 65,589,413 logical bytes; 13 exact PNG aliases.
- Archive SHA: `5806a1a22cb5a77a290bb748609880656b38855eff3afaeb39f606c845f9604f`.
- TAR: 65,546,240 bytes; SHA `988efb1c28c9ca1bcf4945450036110230fc737aed0da6ce86e27ab2e00804f5`.
- Member inventory: `125d4db53ae807aaee6f84a9a2e13e9c5db93f513f116f3a392301521e9bb2be`.

Both producer receipts retain retainer SHA
`0c61b84c51b908135ae01a6cdb842c319334503cad45e09891746c823f2f62f4`.
Earlier attempt 002 wide/small proofs (3,619,120 / 3,081,228 bytes), their original
auditor, before/after reports, source flags, data and helpers remain immutable
**historical** after the canonical PTY test correction. No old receipt was promoted
or overwritten; no existing episode or unknown external effect was replayed.

`vis41-busy-hover-final-audit.py` specializes only sealed function/program syntax
with exact occurrence guards for new literal paths, report names and the added PTY
source path. It uses the immutable independent manual single-XZ header/footer/index
CRC/SHA and canonical USTAR readers, **not the producer decoder**. Every original
member byte and file/directory mode, UID/GID and mtime stability is checked, together
with build/command provenance, current source/ELFs, helper bytes and **153 prior
archive/inventory/archived-receipt byte+metadata records**.
Independent after report: 477,520 bytes, SHA
`52a46abb25a95c9a0934de9e16b48ba2e094565d29ca1c33cc324fa4139c8dba`.
Measured inverse VmPeak **282,103,808 B**, VmHWM **232,566,784 B**, below hard AS384 MiB.

Current canonical source: **713 inputs**, SHA
`5e668220ec03746993beada2a455af1dcf8e9fcd890446aae886acd42c7c5e16`.
Changed versus Prompt001: app tabs/source test, shell/source test and PTY lifecycle
test only. Native debug ELF: 484,381,472 bytes, SHA
`66a002312a9ad7942b0e567b3f6ff3da185e882f71eb19a5b0d9493271abbdb1`.
Original ELF: 200,529,376 bytes, SHA
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Captured producer `e6ef87ae…` remains separate from the delivery commit.

## Capacity and continuation

Only own untracked, unstaged, genuinely FAILED `model-shell-tool-regression-001`
was losslessly archived by the existing reservation/verified-quarantine transaction.
Its qualified successor 006 and every PASS/DIFFERENT root remain untouched.
All **261 files / 28,954,550 logical bytes and 3 directory metadata records** passed
independent PAX/single-XZ inverse; original-path FAILED pointer and status persist.
Archive 1,139,592 bytes, SHA
`8e502dbb499c94aeff9bff4deb3a90f91843a25d510039b309dadd1e1558198b`.
Reservation 8,388,608 bytes; allocation 1,070,379,008 → 1,041,973,248,
reclaimed **28,405,760 allocated bytes**, only verified redundant private copies.

After current inverse, before human receipt/command bundle: owned allocation
**1,048,711,168 / 1,073,741,824 B**, headroom **25,030,656 B**. Actual delivery-checker
recount with the complete gate bundle: **1,048,862,720 / 1,073,741,824 B**, headroom
**24,879,104 B**. The post-push checker remeasures growth. Bounds unchanged: 16 MiB
file/encoded, 128 MiB logical/TAR,
2,000 members, XZ dictionary ≤16 MiB / decoder32 MiB, hard Python AS384 MiB and owned
1 GiB with all established prefixes plus additive `vis36-`; TMP is separate, not
durable offload. No qualified proof deletion, cap increase or byte re-encoding.

Continue every remaining frozen R1–R6/VIS01–VIS45/supplement and final current-source
R6/V09 gate. Next observed generic MCP body `Resource: *` versus pinned `Tool: action`
requires typed presentation-owner verification, without policy/resource authority
changes or vendor/model-name heuristics. Task remains **ACTIVE_NOT_PASS**.
