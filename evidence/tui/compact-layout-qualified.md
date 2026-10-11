# Compact rail allocation — qualified slice, not T44 PASS

## Source and behavior

Native shared vertical tab geometry now allocates three rows before compact tabs:
rail padding, controls and scroll-content padding. This matches the pinned Terminal
TUI owner in `opencode/packages/tui/src/component/session-tabs.tsx:682–718`, not
the web UI. `layout::vertical_tab_strip` supplies both painting and hit testing;
compact stride remains two, noncompact allocation/stride remain unchanged.
Capacity and clipping follow the revised available space, rather than painting
an ordinal at coordinates different from its mouse target.

The substantial current paint/hit test covers 80×24, 120×40 and 160×48, nonzero
origins, heights0–14, compact/noncompact widths, clipping, capacity and Add bounds.
Header/gap cells do not close tabs; each painted ordinal has the same Middle Down
target. Draft/caret/selection, accepted width, existing CloseTab owner, busy,
read-only, Location, CAS and resource guards remain unchanged. Baseline was RED
for native y2 versus required y3; the one-row source correction is GREEN.

## Actually executed checks

- `cargo test --locked --workspace --no-fail-fast -- --test-threads=1`:
  **1842 passed / 0 failed / 11 unchanged opt-in ignores**, all46 targets.
  TUI515, binary113, T42 35. Internal concurrent assertions remain enabled.
- Workspace fmt, strict locked all-target Clippy, locked/ordinary debug build,
  locked release build and both `oc --help` commands PASS.
- Release startup4 covers19 routes/8 GET-only requests; TERM01 actual binary,
  VIS31 two tests, S07 actual archive0→3000 and AUD32 actual archive8→3000 PASS.
  Original guards were not changed; VIS31 idle0 CPU ticks/worker lag0; S07
  retained24232B/window152/cache8581B/children0/lag0; AUD32 HWM20748→22988KiB,
  DB942080→152944640B/children0/WAL0. No real-user-strace claim.
- Python55, producer Node syntax, diff checks and complete proof reopen audit PASS.

## Fresh running Original/native proof

`compact-layout-current-001/compact` uses actual configured vertical42, real drag
to5, resize160×48→80×24→160×48, inactive/active/last Middle Down, inert release/drag,
empty-deck Home, natural generation0 exit0, restart and painted Sessions mouse
reopen of the same durable Alpha Root. No synthetic native Chrome or imported
history is used. Native width5 persists through the existing **Location** deck CAS;
Original width5 is **global**. The scopes are disclosed, not claimed equivalent.

Both sides now paint ordinals **(2,3)/(2,5)/(2,7)** and reopen ordinal **(2,3)**;
reopened caret is **(10,42)** on both. Six requests per side are only three seed
and three title requests. Twenty owner snapshots retain every settled history
row and DCP state; close/restart/reopen cause no provider/tool/project effects.
Bootstrap-before-first-paint is source/test coverage, not a runtime timing claim;
generation1 forced capture teardown is distinct from natural generation0 exit.

All26 complete opaque PNGs, whole styled grids/cursors, VT, input/protocol,
owner/SQLite/WAL observations and every field of all26 strict comparisons were
independently checked. Thirteen cursor pairs match. **All26 comparisons remain
DIFFERENT**:1041/59520 paired cells and86399/8027904 paired pixels. Reopen remains
41 cells/2323 pixels different. Search/header/half-blocks, Add/rail styling,
actual tooltip, footer duration/throughput, Home and Sessions differences remain
visible and unmasked. This is not full R3/VIS44/R6/T44 qualification.

## Immutable acquisition and full-byte retention

Capture base `a755f2befabc394ead5c23212f6dcef0476b97ed`, tree
`30191b2a8b614ca86844f7d2eb3e10623a173e7d`, dirty diff
`d199762f935873ecff4e42095b39bb4586be2bfb9ceb8c85bdc132236dd8324a`.
All709 actual producer inputs match exact acquisition JS `localeCompare` seal
`753b8cf284c0a2401f65369d0765af226d50ded83356e66cf0b06bc85c006261`.
One recorded successful real `cargo build --locked`; Native ELF
`3dfb286aed5a632ca6c0372f9ff308e511b9a76d57de90c217fe9ab7dc36fcab`.
Original ELF `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
donor `2670273ff17da96f85c5826ced57aa1b368754fa`. Seals remain actual predelivery
associations, not rewritten to the subsequent commit.

`compact-layout-proof.tar.xz` retains **174 members / 14444558 logical bytes**,
including parent owner/driver files, in **464452 encoded bytes**; six direct
same-root/same-side encoded-byte-identical PNG aliases. TAR14469120B.

| Seal | SHA-256 |
|---|---|
| Archive | `08509f360f7334308939b3dd8a9d65e02c7a2aa3b250397129e5ba9becbe629b` |
| TAR | `8618285a4a42858071ea9dcf92cc1b56143a27b6f07ecf467446239c2d70865b` |
| Member inventory | `5ae51f76b4137d8202e2a9fbb4f6217fbb3b08894bddc9a3ea78d4ec4a103331` |
| Retainer at packing | `9bca45b99a41e1d255fcbea0e29afae630d65859751b644ba059218595cf4fe6` |

Independent restoration matched complete membership and every original byte.
Pure USTAR/single XZ, unchanged PNG/JSON/VT bytes; no predictor, correction library,
baseline, normalization, cross-side alias or pixel mask. Limits unchanged:
file/encoded16MiB, logical/TAR128MiB, members2000, decoder32MiB/dictionary16MiB,
Python AS384MiB and owner1GiB. Audit peak175492KiB/HWM134300KiB; allocated
1061036032B before this receipt, below1073741824B. All prior proofs/failures and
fresh scratch remain preserved; no cleanup, archival, final TMP offload or cap raise.

Physical code size541 files/292925 lines/11766384 UTF-8 bytes; existing application
5460/turn5307 soft advisories unchanged. Next natural seam is tool settlement/
publication; no cosmetic counter move or waiver.

## Next — entire frozen task remains active

Finish the actual compact search/half-row/Add component through existing guarded
owners, then every remaining frozen body/tail/footer/card/Home/readonly/pacing/
scroll/Thoughts/family/forms/DCP/MCP/auth outcome and supplement. Run final
current-source R6/V09 and all mandatory frozen R1–R6/VIS01–45 gates before finish.
