# T44 shared source-word wrap — qualified slice (2026-10-11)

## Result, source and preserved invariants

The common native styled-line owner now follows the pinned OpenTUI 0.5.10 word
boundaries for source-space Markdown, code and permission previews. The actual
80×24 Markdown URL and Zig punctuation boundaries that previously differed in
61 styled cells now agree at their absolute painted positions and styles.
Generic space-only wrapping keeps its existing mode. This is not a Write-only
lexer, captured-row replacement, pixel mask or runtime SDK dependency.

Source authority is OpenTUI `f6673a04ccb671b9207da358c57152bfd27c781f`, native
`utf8.zig:90–272,1430–1743` and `text-buffer-view.zig:1700–1984`, as actually read
from the pinned package. The existing byte-faithful
`crates/oc-tui/assets/syntax/license-anomalyco-opentui-LICENSE` remains intact.
ASCII/Unicode punctuation, whitespace and ASCII/CJK script transitions are
metadata boundaries, not unconditional breaks or boundaries between style spans.
Terminal columns, grapheme-safe overflow, fitting separator styles, logical-line
indentation and existing row/byte limits remain owned by the common wrapper.

Two substantial tests cover the real Markdown/Zig source, styles and immutability,
streaming/bounded agreement, punctuation, CJK, NBSP, soft hyphen, wide overflow,
logical indentation, cross-style combining suffixes and standalone WORD JOINER.
They were red on the original source-space wrapper. Independent review exposed
indentation and zero-cell classification cases; those were fixed and added to the
same scenarios. The final re-review found no concrete current regression.
The existing long-line indexed-tail test now rejoins only the actual painted
bottom rows and requires the complete sentinel at the end: pinned hyphen wrapping
does not imply loss of content. No source decode, cap waiver or disabled test was
used to obtain that result.

RAW/input/model/effect identity, typed Write redaction, permission/recovery,
selection/draft ownership, query/grammar/cache limits and all concurrency guards
are unchanged. There is no new crate, registry, store, poll or query variant.

## Actually executed current gates

- `cargo test --locked --workspace --no-fail-fast -- --test-threads=1`: all46
  results, **1821 passed /0 failed /11 unchanged opt-in ignored**; includes507 TUI
  and107 binary unit tests. Operational build jobs3/test threads2/TMPDIR were
  supplied; serial test functions do not disable internal concurrent scenarios.
- Workspace fmt, strict locked all-target Clippy, `git diff --check`, debug locked
  and ordinary builds, release locked build and both `oc --help`: PASS.
- Release startup19 routes/8 GET-only discovery cases and TERM01 actual binary:
  PASS. TERM01 baseline/two-PTY RSS18688/22408KiB, FD24/26, threads8/10,
  idle300ms CPU0ticks; raw/control/VT/resize/focus/shutdown/crash/no-replay checked.
- Release VIS31 two tests: idle1sec CPU0ticks,165/250Hz input/provider bursts and
  zero worker-event lag. S07 archive0→3000 RSS24840→27612KiB,
  HWM26048→28432KiB, retained24232B/window152/cache8581B, child0/lag0.
- Release AUD32 actual binary archive8→3000 pairs plus turn logs:
  HWM21240→24120KiB, DB942080→152944640B, child0/eight threads. Original resource
  thresholds are unchanged; these fixtures are not a real-user strace claim.
- Root Python51 tests, documentation/registry/journal checks: PASS. Physical
  inventory538 files/287970 lines/11542054 UTF-8 bytes; existing application5458
  and turn5242 soft advisories remain, without cosmetic moves or limit waivers.

## Actual complete paired acquisition and independent audit

Fresh `wrap-current-001/inventory` and `wrap-regressions-001` contain the four
syntax stages and ordinary/default-temporal episodes under the unchanged original/
native frontend, fonts, geometry,15sec/five-identical-full-observation stability,
PGACK, complete opaque PNG and strict styled-grid/decoded-RGBA comparators.
The actual80×24 probe verifies the unchanged Markdown/Zig source on both sides;
the new flag is true. Parent inspected both whole draft PNGs. No source/capture
producer bytes changed after these acquisitions.

Each side made3 requests and one real91-byte overwrite, naturally exited
generation0 with code0, and reopened through painted Sessions input without
provider/effect replay. Final generation1 stop remains separately recorded.
Seven complete120-cell Write rows compare840/840 cells equal, including inherited
padding x5/x7, digits x6/source x8. All39 rendered SDK cases retain14256 source
cells plus2144 heading cells with zero style mismatches;78 cases/156 styled
projections, current `#12ab34` stripe and85 frozen Original cache seals/69 required
first-witness consumed inputs agree. The16 locals remain unclaimed.

All absolute narrow Markdown/Zig rows and headings have **zero styled-cell
differences**. Same-side complete/replay whole cells, cursor and encoded PNG are
identical. Cross-side complete/replay retain2 clock-symbol cells/105 pixels each:
Native **1.8s**, Original **2.0s**. Draft retains2 clock-symbol cells/137 pixels,
including32 already-existing right-edge pixels where Original is `#1e1e1e` and
Native `#0a0a0a`. Live-partial retains48 styled cells/2021 pixels in Root/status
chrome. These are full strict **DIFFERENT** reports, not masked semantic PASS.

A temporary acquisition-driver expectation of the preceding R3's whole-frame
equality stopped after successful capture. Physical full reports established the
genuine clock difference; only that unsupported expectation outside producer
inputs was removed. A later temporary driver expected `captures` in CLI summary
instead of the actual saved lock. Read-only existing-lock validation resumed both
completed episodes without another process/tool execution or metadata rewrite.
These errors and all actual frames remain preserved.

Self-contained `wrap-proof.tar.xz` plus `wrap-proof.inventory.json` retain **all1406
members/96040967 logical bytes**,224 opaque PNGs/788448 styled cells/104193792
pixels, including144 temporal raster/caret associations. Standard-library
USTAR/single XZ preserves every original JSON/VT/log/encoded-PNG byte;148 direct
byte-identical PNG aliases stay within their fresh root and side. No external
baseline, correction library, cross-side alias, normalization or final offload.

| Actual association/seal | SHA-256 |
| --- | --- |
| Capture code base | `e96b382e1867ecfbdf59d23fd4b4a91a03efb6ab` |
| Captured dirty code diff | `222f98487416c4fff26d4c198e9689f851c2fba9f3f7f212147eec27c1ec93e8` |
| Current706-source canonical manifest | `f4f4cd9e4952ec01391a071db2cf88682fc0b3ab0fd44d2ae5af7ad7165020b6` |
| Captured Native ELF | `700681e016b7c6a0e5f4c9c64627cdf9b94f24b491f325720a48fc20a1badefc` |
| Pinned Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Encoded archive3216980B | `cbf8b2dbfdb87cc9e497a566d0a75ea5918601cc93dc486797b21dce2f39fc6b` |
| TAR90112000B | `7bcc4ad70c2f81eb11990a9d036e271ac45bfe46b637848b206ac524939d98fd` |
| Every-member inventory | `620a54645fa2339282f2e365a0aa3be0874f3f38d2daae5242365c1b09dd478e` |

Pack/reopen and independent every-byte/current706-source/three real locked-build/
ELF/full comparator/effect audits passed. All bounds held:16MiB file/encoded,
128MiB logical/TAR,2000 members,32MiB decoder/16MiB dictionary,384MiB Python;
independent peak279372KiB/HWM253568KiB. No concurrent Cargo ran during capture,
pack or audit. Prior qualified/failed bytes, reports and checkpoints are untouched;
fresh scratch also remains preserved. Own allocation before this report was
1060511744B/headroom13230080B under the unchanged1073741824B cap; all `wrap-`
artifacts count. No additional archival, deletion or quota increase in this slice.

## Remaining full acceptance and semantic next

Direct reports retain218 DIFFERENT/4 EQUAL plus4 authorized native-only entries;
matched72 pairs retain144 DIFFERENT. Default temporal six owners/72 full rasters
per side:4032 samples/23715 commands, zero active phantoms/raster cycles. One
Original hover parser grid is genuinely hidden at(3,15) while its raster caret is
visible at owner(24,18); this is preserved, not normalized. Composer remains
Native(21,18)/Original(24,18), search(14,9) agrees; blink is not qualified.
Three MCP effects and one17-byte Shell effect per side retain their bounded,
distinct model/UI resource facets.

Next fix actionable live Root Add **only after** session-addressed parked-root
turn/tool/completion, job, approval/question and view ownership are safe. Do not
paint an inert glyph or blindly remove busy/child/read-only/capacity guards.
Independently qualify truthful active context usage and preview-loss chrome;
do not suppress loss or invent usage. Then complete every frozen R1–R6/VIS01–45
outcome and supplement, including Thoughts/family/forms/DCP/MCP/auth, and final
current-source R6/V09. Whole T44 remains **ACTIVE/NOT_PASS**, with no early finish,
historical rewrite or acceptance reduction.
