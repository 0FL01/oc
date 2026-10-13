# VIS39 — completed Subagent icon/layout roles

Date: 2026-10-13. T44 remains **ACTIVE_NOT_PASS**. This qualifies the narrow
Subagent icon/layout role and preserves the current paired CLI-auto lifecycle;
it does not qualify whole VIS39, VIS36, R6/V09 or T44.

## Observed cause and change

The prior 009 paired cell arrays identify the completed icon at zero-based
`(5,27)`, not `(6,27)`. The glyph `✓` is already muted on both sides. The next
cell is layout space: native gave it the icon's muted foreground, while pinned
TUI leaves it to inherit the canvas foreground. Both profiles show the same
static distinction. The read-only structured observation and its full log are
retained with the command bundle; no source frame or provider episode was replayed.

Pinned owners are TUI `InlineTool`/`Subagent` in
`opencode/packages/tui/src/routes/session/index.tsx` and the two-cell icon
container in `routes/session/message-parts.tsx`. The native change is local to
`tools.rs::subagent_block_expanded`: styled glyph, plain layout space, styled
label, including the narrow fallback. Continuation indentation stays plain.
Glyph/label/error roles, child identity, expansion, RAW, failure and unknown
semantics are unchanged. No numeric-color, caret-position or clock hack exists.

The existing small `golden_subagent_card` now guards the layout cell separately
from the existing muted-glyph assertion. Actual RED was muted versus inherited
foreground. The first GREEN attempt had a new test assertion expecting white
in a `TestBackend` whose default canvas is `Reset`; that assertion was corrected
to inherited `Reset`, not by recoloring production cells. All four Subagent
tests then passed. The whole shell's actual canvas remains white.

## Current source, executable and acquisition

- Producer Git base: `70ad2d3b3b155d6ce4cc75f982e88e711a7c8b05`, captured with
  the reviewed dirty source. Producer identity is not rewritten to delivery HEAD.
- Canonical **713-input** source SHA:
  `cab6b76afc047dbeef6d195a26d00c8e6a555cf13ec2c7ffc547ff5434b188b2`.
  Relative to 009, only `crates/oc-tui/src/tools.rs` changed.
- Actual debug native ELF: 484,379,312 B, ET_DYN/machine62,
  `9a3486ca8f968b836f26c11cdbbff7257f69136c0409c9175d91e9855797ef93`.
- Actual pinned Original ELF: 200,529,376 B, ET_EXEC/machine62,
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`;
  donor commit `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Exclusive new roots are `vis36-mode-current-010/{cli-wide-r10,cli-small-r10}`
  under approved TMP; actual 121×40 and 79×40, built sequentially, one captured
  successful `cargo build --locked` per profile, unchanged real auto-cli harness.
- Both origins in both profiles passed **38 behavior checks**. All **64 whole
  PNGs / 64 strict grid+PNG comparisons remain DIFFERENT**. No normalization,
  mask, crop, tolerance, golden substitution or encoded-PNG rewrite was used.

## Independent inverse and full recount

`vis36-subagent-gap-proof.tar.xz` and its inventory preserve all **422 originals**,
39,600,707 logical B, including parent logs, driver, source/build/commands,
full PNG/styled cells/cursor/VT/clocks/protocol and read-only effects snapshots.
Four aliases are direct same-run/same-side byte-identical original encoded PNGs.

| Fact | Value |
|---|---|
| Encoded archive | 2,211,652 B |
| Archive SHA | `c8affb4ddb10a89c6f769024a2af7ebb39ebc70503936f4f83be57ba2d287b3d` |
| Decoded USTAR | 39,751,680 B |
| USTAR SHA | `43d2fd7daea24593e73f79b28ab9551d3dc6ddd787e3069c1f63c16714edd218` |
| Member inventory SHA | `0fddd17d6e4bb29270dcc832b51ec2be1bf8a1c45d59ce64c7daaa1e1af10678` |
| Retainer at pack SHA | `853a76f6910ae6dce5ad52e4931ffd094f6095b72e9dbacfa5e2ef61d79238e0` |

The sealed independent program uses no producer decoder: manual single-XZ
header/footer/index CRC/dictionary/SHA validation and canonical USTAR inverse
check every original byte/path/alias, directory metadata stability, current
source/actual ELFs, helpers and **137 prior proof files' bytes and metadata**.
It independently recounts every whole grid, PNG and cursor against actual
comparator commands/provenance/results.

The first independent AFTER observation located a description inside the full
caption rather than the full actor/description caption. Its failure, sealed
program and BEFORE remain immutable. An exclusive observer-only refinement
changes one diagnostic locator to the actual paired full caption and records
its SHA `9e43103d3060dd0eb86c7989d29b4973f29fec425f436177e8fdf4c600d1b31e`.
No capture, source, metric, threshold or decoder was changed. Final AFTER is
18,776 B, SHA `38eb7fefd78d572346e276644997123214bf66a301c977c55ea57fef5449a0d1`.
Independent VmPeak 192,487,424 B / VmHWM 136,802,304 B, below hard AS384 MiB.

Actual recount: **32 full pairs, 128,000 styled cells and 17,264,640 RGBA pixels**;
26,544 differing cells, 3,264,294 differing pixels, **28/32 cursor pairs equal**.
Both completed Subagent headers (glyph, layout blank and full label), both failed
Patch role/gap/padding diagnostics and four complete 600-cell Settings modals
match. These named diagnostics do not replace or mask full-frame differences.
Different native denial copy and frozen VIS16 successful-Shell suppression remain
unchanged in the full comparisons.

Each origin/profile has 18 valid requests/completions (16 main, 2 title), eight
real calls, 11 read-only SQLite snapshots and two natural exit0 generations.
Zero Always grants, exact root/child Once effects, mixed Deny, saved
Autoaccept→Prompt and CLI-priority relaunch remain verified. Relaunch creates a
fresh admitted root, not an asserted same-root restart. Native has eight ops
(seven completed, one failed) and eight completed turns; Original V2 RAW IDs and
eight tool parts are retained. Read-only SQLite is not a syscall/CoreEvent trace;
typed owner ordering evidence remains separate.

## Current code gates and storage

Actual `cargo test --workspace --locked --no-fail-fast` completed:
**1868 PASS / 0 FAIL / 11 unchanged opt-in ignore, 46 targets**, TUI534 / PTY58.
Strict locked workspace/all-target Clippy `-D warnings`, fmt check, locked and
ordinary debug build/help, locked release build/help and **nine actual release
cases** passed: startup/discovery2, TERM01, panic restoration, VIS31 pacing2,
S07 archive, DCP archive and AUD32. All seven release targets had real tests;
no bare exact-filter zero-test result was counted. Root Python70 passed.
S07 retained24,231 B at archive0/3000, queue lag0/children0, HWM28,148→27,296 KiB;
AUD32 HWM20,768→23,968 KiB, children0, DB942,080→152,944,640 B.

All complete command logs/JSON and observer/refiner/archival helpers are retained
in the independent-inverse command bundle. No test, deadline, threshold, ignore,
golden or baseline was relaxed. Historical current009 flags/data are not rewritten.

Capacity used the existing lossless archival path for **actual FAILED** own,
untracked/unstaged model-shell019, not visual DIFFERENT candidates. Model010–018
were behavior-only PASS and remain untouched. All 103 original files / 19,461,881
logical B and three directory metadata records survive exact PAX/single-XZ
inverse; Native FAILED / Original PASS_BEHAVIOR_ONLY and the original-path FAILED
pointer remain visible. Its 607,476 B archive SHA is
`0c24f122ec34ded5229cb08c248a257684fcc1d2bdd25346ce0bb288a726a349`.
Only fully verified private redundant copies were reclaimed: **19,005,440
allocated B**, with a 3 MiB reservation. No qualified proof was deleted/offloaded.

Before human receipt/command bundle, actual additive quota was
1,055,657,984 / 1,073,741,824 B, headroom18,083,840 B. Final delivery recount is
authoritative after their allocation. Bounds remain 16 MiB per file/encoded,
128 MiB TAR/logical, 2,000 members, XZ dict16 MiB / decoder32 MiB, hard Python
AS384 MiB and owned1 GiB. All established prefixes plus `vis36-` remain counted;
TMP is accounted separately, not durable offload.

Complete command bundle: `vis36-subagent-gap-gates.tar.xz`, **57,904 B**,
SHA `2155eb0f63db3b07789bc7bf62e12194001587cefe9b72c913ea661e5750f18a`;
69 members / 263,330 logical B, all 32 command JSON/full logs and five helpers.
Independent manual inverse checks every memory and durable member byte; VmPeak
244,252,672 B / VmHWM72,257,536 B, below hard AS384 MiB. Actual post-receipt/bundle
allocation is **1,055,735,808 / 1,073,741,824 B**, headroom **18,006,016 B**.
The final seal check keeps the captured producer identity separate from Git HEAD.

## Next

Continue ordinary Prompt paired approval previews/outcomes via the existing
permission-mode prompt harness, then every remaining frozen R1–R6/VIS01–VIS45
supplement and final current-source R6/V09. Local Subagent/Settings roles are
not whole-task completion.
