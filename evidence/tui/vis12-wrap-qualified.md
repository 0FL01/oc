# VIS12 expanded longword — current-source geometry-qualified slice

Date: 2026-10-12. Active task: T44. **Not VIS12/R6/T44 pixel PASS.**

## Observed contract and correction

The actual painted chip probe inserts `X` into `NEIGHBOR `, then expands the
1600-character `z` paste. RAW remains exactly `NEIGXHBOR ` + 1600 `z` (1610 bytes).
Previously native filled the prefix row with part of the oversized word, producing
an empty final row/caret `(26,27)` instead of Original's final 60 `z`/`(86,27)`.
The existing editor word-wrap owner now starts an oversized word on a fresh row
after a nonempty prefix, then uses its existing grapheme/cell wrapping. This agrees
with the pinned OpenTUI 0.5.10 placement source; no replacement editor or store.

The new real-paint/mouse regression was RED before the correction and is GREEN:
prefix row, 24 virtual rows, final partial tail, legal end caret, exact RAW,
undo/redo, subsequent painted tail click/edit and no dispatch effects. Public API,
history, Unicode/chip ownership, permissions and resource bounds are unchanged.

## Actual acquisition lineage, including unsuccessful work

All three fixed episodes retain original VT, full opaque PNGs, styled grids/cursor,
structured checks/snapshots, provider/protocol records, commands, source/build locks
and direct parent owner/log files. No unknown or incomplete episode was replayed.

| Episode | Whole PNGs | Native / Original checks | Current qualification |
| --- | ---: | --- | --- |
| `vis12-wrap-current-001/caret` | 38 (14 + 24) | IN_PROGRESS / FAILED | FAILED_INCOMPLETE; noncurrent |
| `vis12-wrap-current-002/caret` | 50 (25 + 25) | Both PASS_BEHAVIOR_ONLY | Current behavior/geometry only |
| `vis12-wrap-current-003/caret` | 50 (25 + 25) | Both PASS_BEHAVIOR_ONLY | Rejected foreground experiment; noncurrent |

001 exposed a reused `runs/caret` reference fixture root: the saved title was not
unique. The authoring command's 120-second envelope also interrupted native;
failure frames/statuses remain intact. Its direct driver log was never written;
the proof does not fabricate one. 002/003 each have their actual direct driver log.
The existing capture root isolation now also covers prompt history/caret: basename
plus the first 12 SHA256 characters of the full absolute output path. Existing-root
refusal, fixture/environment ownership and all other mode semantics stay intact.
002 uses `runs/caret-4d575359bc1c`; both sides launch/relaunch within that same isolated
project and HOME/XDG lineage. No user/authoring configuration or live API was edited.

003 tested generic current-cursor foreground painting. It removed the expanded
blank-cell difference but created four other blank-cell differences. That hypothesis
was refuted; all experimental paint/test changes were removed. `shell.rs` and its
tests equal HEAD. 003 remains complete historical evidence, never current-source
qualification. Restored source/ELF bytes exactly match 002; no replacement capture
or later build was inserted into its original commands.

## Current entire-frame observations and effects

Actual 002 runs at 120×40, real resize to 80×40 and back, default full-frame stability,
fonts/geometry and strict whole comparators. All **50 strict comparisons DIFFERENT**:
116800 paired styled cells, 474 different; 15744640 paired RGBA pixels, 20754 different.
All **25/25 complete cursor pairs equal**. No crop, mask, normalization or tolerance.

Expanded stage: all 900 visible styled `z` cells agree, final 60 `z` + 10 blanks and
visible block caret `(86,27)` agree. Residual: seven cells/298 pixels—six version-footer
cells and one blank-cell foreground (`#ffffff` native versus `#eeeeee` Original).
The pixel difference bbox is the footer `[943,611,992,620]`; the styled blank-cell
difference is still retained. No foreground or whole-frame equality claim.

Across these three wrap episodes: **10 valid provider requests = 5 main + 5 title,
5 explicit Enters, 18 structured snapshots**. 001 native has zero requests, Original
two; completed 002/003 have two per side. Current native's sole accepted RAW row is
exactly `Проведи RECXON, жду план`; ID/session/sequence/text survive reopen/restart.
No observed extra project/tool/Shell/MCP/DCP effects or completed-attempt restart
replay. Reference RAW/DCP table contents are not decoded by these snapshots; actual
wire/full frames and native durable observations are distinct evidence.

## Immutable captured source and build association

Each episode has exactly one recorded successful repository `cargo build --locked`.
Current 002 captures base commit `5463a69326d872e3dac0daab365e166c3ed995be`, tree
`629ced80382600a2caa1a88012f30cb25eef65a1`, producer dirty-diff SHA256
`ac94e5abc10f3042b1d859ee030e43aebd32017e28171c9baa2a532415274ac3`.
These pre-delivery associations are not rewritten after a later commit.

| Object | SHA256 |
| --- | --- |
| Current 709-file JS-canonical producer manifest | `f2263cf23834292c8d51967aefc2a518b9e8a77a7bfc5f844efb6a7111697510` |
| Source manifest file | `4b05ec22f2a555d44b6df41f4968c29d74429616d0debcb1f8d1338133feda28` |
| Capture lock | `063ee2153cf1d620574f298f3e8c3e7420af780a9316ab79bcb4bc0828d2ee6e` |
| Captured commands | `588ed4242880a0693d3b16c1bc494e7bd36666ab65dd8a8050423eb8d594d635` |
| Actual current native ELF | `773d54f43e37f00e68c177ccadd336a1057fd70e176e4991c7416f1b182fcb55` |
| Actual Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |

Pinned donor: `2670273ff17da96f85c5826ced57aa1b368754fa`. Root retainer/tests/docs are
outside the producer set. All 709 current source paths/bytes and both actual ELFs
were independently rechecked at opening, closing and final inverse audit.

## Complete byte-exact retention and independent inverse

[Archive](vis12-wrap-proof.tar.xz), [inventory](vis12-wrap-proof.inventory.json):
**843 original members / 64934263 logical bytes / 138 whole PNGs / 13 direct same-run,
same-side identical-PNG aliases**, retained in **1581536 encoded bytes**. Direct
parents contain exactly three owner files and two actual driver logs. Pre-encode
inventory, pack/reopen and independent in-memory inverse match every original byte.

| Object | SHA256 |
| --- | --- |
| Encoded archive | `bc30ac220c064595e157ec1619617134583e901cf918e8f3d06786add9f4306b` |
| TAR (65280000 bytes) | `d30467559cda3787ebf296d512c418fe63be7555829c1088d70222a3726df466` |
| Complete member inventory | `87a7c12975cc249fa61148fc77f63873da73ff7e783dc8199965bdfbb1868ca8` |
| Durable inventory receipt (2136 bytes) | `0a3a1c9b55280cd5f9ba0d5ae203656b5326e9e904ebf5f81f8655cd1779692d` |
| Retainer source at pack | `0d0d2badfb2b7eff958b8eae5401c004e584558e4e41bee0ee6e19246528d98f` |
| Independent inverse verifier | `ebd654cfca07de80cc6f5b104d459ee9a2acb27d0aa2d21f52e2f5d0361d376f` |
| Independent inverse report | `224dbab173711a6f02373bf0ea8d480c1ce66f2dd8ec8604b560bcd2dbc6163b` |

Independent verifier/report remain in approved TMP
`opencode/vis12-wrap-independent-20261012-sole-verifier/`. Single XZ stream, integrity,
dictionary/decoder limits, canonical USTAR headers/order/checksums/padding/end records,
all original membership/bytes and direct aliases independently PASS. Captured
preservation inventories/auditor chain and six prior proof files remain unchanged;
this is that specifically observed set, not a new claim auditing all 11430 old entries.

Same bounds: owned evidence 1GiB; file/encoded/dictionary 16MiB; TAR/logical 128MiB;
2000 members; decoder 32MiB; Python hard AS 384MiB. After both new proof files, before
this receipt: allocated **1066545152/1073741824 bytes**, headroom **7196672 bytes**.
Inverse VmPeak 211070976 bytes/VmHWM 180559872 bytes. No extraction, cleanup, offload,
overwrite, old evidence rewrite or bound increase.

## Actually executed current-source gates and remaining work

- RED regression before fix; targeted/full affected TUI after fix.
- `cargo test --workspace --locked`: **1844 PASS / 0 failures / 11 unchanged opt-in
  ignores**, all46 result targets; TUI517/binary113. The first 600-second authoring
  envelope expired; diagnostics found the unchanged finite localhost-failure retry
  fixture. A 1200-second envelope completed all gates, without test/deadline changes.
- Strict workspace all-target Clippy, workspace fmt, locked/ordinary debug builds,
  locked release build, both binary help and capture JS syntax PASS.
- Current release recovery_startup4, terminals1, VIS31 pacing/idle2, S07 equal-view
  archive resources1, AUD32 actual binary memory1: **9 PASS**; zero idle CPU ticks,
  archive0→3000 retains same active view, actual growing-history HWM below original
  64MiB limit. Locked debug rebuild restores the captured ELF before pack/audit.
- Root Python **59 PASS**; docs58 tasks/173 specs and progress structure PASS;
  `pack`/`audit --campaign vis12-wrap` FULL_BYTE_EXACT, independent inverse PASS.

Warning-only physical code inventory: 541 files, 293883 lines, 11821505 UTF-8 bytes.
Existing soft warnings remain `application.rs` 5460 and `runtime/turn.rs` 5307,
unchanged; the natural settlement/publication seam remains separate, no cosmetic move.

Remaining blank foreground, footer/dynamic frames and EVERY frozen R1–R6/VIS01–45
outcome/supplement remain explicit. Continue the real blank-cell rendering owner
diagnosis without generic cursor paint/fixture-coordinate hacks, then all remaining
contracts and final current-source R6/V09. **No whole task finish or pixel promotion.**
