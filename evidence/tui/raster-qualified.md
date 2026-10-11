# T44 viewport shrink / fractional raster edge — qualified slice

Full frozen T44 R1–R6/VIS01–VIS45, every supplement and final current-source
R6/V09 remain **ACTIVE / NOT_PASS**. This receipt qualifies the measured shrink
owner and current regressions; it is not global pixel acceptance.

## Actual cause and source

Both retained Hover001 narrow frames had the same 80×24 viewport, 674×384 raster
and 8.425×16 measured cells. Visible cell backgrounds agreed. The extra32 pixels
at x673/y352–383 came from retained alternate-screen columns outside the viewport:
Original backing lines still held120 cells and columns80–83 had `#1e1e1e`, while
native had erased their backgrounds. Fractional DOM row ends exposed that sliver.

The bounded read-only VT inspection preserved both complete input hashes. Native
had three `ED 2J` commands, two at shrink, all with default erase attributes;
Original had no ED or EL. This refuted the initially tested colored-reference-erase
hypothesis. Pinned OpenTUI's internal resized buffers are not the physical xterm
backing-line tail. No renderer, CSS, pixel rectangle or screenshot was changed.

`oc-tui::terminal::FrameBackend` now records actual size changes in private mutex
state, preserving the existing writer/concurrency interface. Only an observed
resize inside its pending hidden synchronized frame substitutes visible-row ECH
for Clear(All). Every visible cell is erased, then Ratatui still invalidates its
buffers and repaints the whole new viewport. The operation saves/restores current
cursor position and inherits the caller's SGR attributes. Stable/manual/outside-
frame All and other clear regions retain their original Crossterm behavior.
Zero-width/height resized viewports are no-ops, not whole-screen erase fallbacks.

The new meaningful backend unit was baseline RED. It verifies two full visible
clears, every row/column, deferred publication, final cursor/show ordering, an
explicit no-draw cursor/SGR case, stable manual ED and zero dimensions. Existing
partial-write/flush-failure tests now also exercise resized frames: failure stays
non-success, without replay. No public signature, rights, store, query, polling,
model result, RAW history or provider/tool graph changed.

The existing probe reads the already-captured narrow `.render.json` before/after
geometry, requiring the two actual tail rows and four bounded outside-column BG
attributes on **both** binaries. It does not repaint, recapture or edit a frame.
The explicit `raster` retention campaign requires those true flags plus the old
hover, source/build/ELF/seal and Root lifecycle checks. All raster-owned artifacts
are included in the unchanged1GiB accounting. Dev Node remains a proof tool only;
there is no production Node/Bun/JS-host dependency.

## Current verification

- `cargo fmt --all -- --check`, strict locked workspace/all-target Clippy and
  reviewed whitespace checks PASS.
- `cargo test --locked --workspace --no-fail-fast -- --test-threads=1`: all46
  results, **1830 passed / 0 failed / 11 unchanged opt-in ignored**. TUI509,
  binary111, adapters696 plus its old ignore, runtime121 and core34 are included.
  Internal concurrent scenarios and all original assertions remain enabled.
- Locked and ordinary debug builds, locked release build and both help commands
  PASS. Release startup4/19 routes plus8 discovery GET-only cases, TERM01, two
  VIS31 tests, S07 and actual AUD32 all9 PASS.
- TERM01: baseline/two-PTY RSS18376/21804KiB, FD24/26, threads8/10, idle300ms
  zero ticks; raw/control/VT/resize/hide/focus/shutdown/crash/no-replay PASS.
- VIS31: idle1s zero CPU ticks; actual165/250Hz input and provider burst, zero
  worker-event lag. S07 archive0→3000: RSS26700→27448KiB, HWM27784→28656,
  PSS24463→25188, retained24232B/window152/cache8581, children0/lag0.
- Actual AUD32 archive8→3000 pairs of16KiB plus turn logs, equal two active1KiB
  pairs: HWM21736→22264KiB, end RSS21736→22236, end PSS19117→19519,
  DB942080→152944640B, children0/eight threads/WAL0. Original64MiB and all
  other resource guards remain unchanged; this is not real-user strace evidence.
- Root Python52 tests PASS. Debug ELF was restored after release tests and before
  captures. No Cargo ran during capture, retention or independent ELF audit.

## Fresh actual paired Raster001 proof

The unchanged seven-state Root episode ran on both real binaries: live partial,
parked Home, complete39, current-profile narrow draft, fresh replay, explicit
cancel pending and cancelled. Actual painted busy Add/Root return/Home parking
retain the same unsent draft; the held generation completes while parked. Ctrl+X W
closes only Home. Generation0 exits naturally0; painted Sessions mouse reopen
does not replay a provider request or effect. Then one explicit generation1 fake
request is stopped by real two-Escape input and HTTP disconnect.

Each side has three initial requests plus that explicit cancel, one real91-byte
overwrite and no extra tool. Native has one session/two turns completed/cancelled;
the cancelled public prefix remains INCOMPLETE, not a canonical completed answer.
Typed requested Write content, confirmed effects, RAW/input and graph stay intact.
Actual context is1234+64 with captured225000 limit, separate from billed usage.

**The former32 right-edge pixels are now all equal**, RGBA(30,30,30,255), across
the entire x673/y352–383 strip. The full narrow PNG has66 real clock pixels
different and **zero other pixels different**; no mask or tolerance changed.
Both saved geometries retain120-cell backing lines and the actual outside BG.
Current native shrink VT has48 ECHs (all24 visible rows twice), two cursor
save/restore groups, unchanged erase SGR and **zero ED**, within hidden sync.

All345 accepted-user inner cells agree: completion hover#1e, fresh replay#14;
stripe#12ab34 and identity/cursor guards hold. All840 absolute Write-row cells,
gutters,39 grammar bodies14256 source cells/2144 headings and actual narrow MD/Zig
rows/styles agree. SDK39 languages/78 cases/156 dark-light styles stay compatible.
Original85 cache seals and69 required FIRST consumption witnesses hold;16 locals
are not claimed consumed. No early observer touched that first-consumption proof.

Ordinary/default-temporal each retain three real MCP effects plus one17-byte Shell
effect per side, protected resources and no replay. Model Shell257B/12 lines and
separate UI stream960B/49 lines remain distinctly bounded. The same15-second/five
identical full observations, own-process pause acknowledgement, fonts, full opaque
PNG/grid/cursor capture, comparators and real clocks remain unchanged.

Independent restoration checks every original byte, all707 current inputs in
three manifests/three real locked builds, actual ELF/donor associations,230 opaque
PNGs/817248 styled cells/108076032 pixels,144 caret-pixel associations and entire
recomputed strict comparator reports. Direct: **220 DIFFERENT / 8 EQUAL plus4
authorized NativeOnly**. Matched:72 pairs/144 DIFFERENT. Live-partial full grid,
cursor and PNG remain EQUAL. Body/edge qualification is not whole-T44 PASS.

## Complete retention and immutable association

`raster-proof.tar.xz` and `raster-proof.inventory.json` retain all1442 original
members/100220493 logical bytes,1296 regular entries and146 direct byte-identical
same-root/same-side PNG aliases. One stdlib USTAR/single XZ archive, no baseline,
predictor, correction library, cross-side alias, PNG re-encoding or data rewrite.

| Seal | Actual value |
|---|---|
| Encoded archive | 3853300 B; `2c631f6fd76f05b87715976bc553a93f3e3d714246b7a24755f3624a2af72bbf` |
| TAR | 95252480 B; `61f383e2d01045af74d8f6c441939752e441cf4c59bf4aa4408d60d45db57e94` |
| Whole member inventory | `80c1b668dbebddf1386ea9eeb24dadf4cef4a66399ac34e3caa379ea83796f53` |
| Captured code base | `50efedee9180e6a7475e536d2ba7c68d04b76d15` |
| Captured dirty code diff | `ecbf2cec6cd95b25b304b3a9cddff08b7257c6305d439e65b2c8dc7c1d5a2230` |
| Current707 manifest, exact acquisition JS locale order | `21681f2a30d4168f2fc1c38875b3184ecff70a33c24bb3eb6248c5088d1479c5` |
| Native ELF | `b32b36faa526418571a9496db1d520a20221c75315eb91cceee8ee1ce738a83c` |
| Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Retainer source at packing | `fc44e9ea3760cf15f166d3afe986d4e77bbb84395b9d993cb3dbcee64269a3fd` |

Donor remains2670273ff17da96f85c5826ced57aa1b368754fa. These source associations
precede delivery and must not be rewritten after a commit. Independent inventory
was sealed before encoding; every byte was inverted before/after durable exclusive
file/parent fsync. All current and older acquisition scratch remains preserved;
final proof is in evidence, not temporarily offloaded.

One own **untracked FAILED** subagent-cards-003 campaign was archived losslessly:
166 files/17492235 bytes plus3 directory metadata records, original modes/UID/GID/
mtimeNS. Its830748-byte archive SHA is
`0cdd44b217705f9ee9657766761308e2572a1bcda986b81af81865d7dfbca1a6`, full inventory
`ce2c3d8af310d4904281c89c0ac3526dabfa65c2f9cb423fca3dd2e8aeba2c25`.
The original path's ARCHIVED.md still visibly says FAILED/NOT_QUALIFIED. Only
redundant loose copies in a fresh private atomic quarantine were removed after
durable whole-byte/metadata comparison. Historical diagnostic bytes and failure
visibility are immutable, not their duplicate loose encoding. Committed proofs,
baselines, older reports/checkpoint leaves and foreign data were untouched.

Before archival owned allocation1070141440B→1053167616B; after new proof and audit
the exact quota API returns1057026048B, below unchanged1073741824B. File/encoded
16MiB, logical/TAR128MiB,2000 members, XZ decoder32MiB and Python384MiB bounds hold;
observed Python VM/HWM306528/277972KiB and dev Node VM359300KiB. No cap increase,
failure deletion, raw-frame alteration or provenance/license removal occurred.

Physical size audit:539 files/289800 lines/11622157 UTF-8 bytes. Existing owner
advisories application5458/turn5277 are unchanged in this slice; the next natural
seam remains tool settlement/publication, not a cosmetic line-count relocation.

## Remaining full contract / semantic next

Real first clocks are8.0s/8.3s, cancelled second clocks1.6s/1.8s. Parked Home still
differs28 styled cells/1286 pixels. Composer cursors remain native(21,18) versus
Original(24,18); one genuine hidden Original parser observation remains unmodified.
Default temporal3985 samples/23616 commands have zero phantoms/cycles, but this
does not qualify blink. No timing, BG, cursor or raster region is normalized.

Next trace actual Root Home and composer-input/caret/blink/pacing owners with
current paired full frames/effects, then every remaining Thoughts/family/forms/
DCP/MCP/auth frozen outcome and all supplements, finally current-source R6/V09.
Do not reduce acceptance, claim global pixel PASS or stop after this delivery.
