# VIS12 current history/caret — qualified behavior, not pixel PASS

Actual paired acquisition at native `3c6f42c7618391dd97393ea362801efbf3afebb4`.
**Full frozen T44 and its supplements remain ACTIVE_NOT_PASS.** This receipt adds
current evidence; it does not overwrite the earlier prompt-history/caret receipts.

## Real acquisition and effects

| Episode | Actual geometry | States/side | Whole opaque PNGs | Native / Original |
|---|---|---:|---:|---|
| history | 80×24; default keys | 25 | 50 | PASS_BEHAVIOR_ONLY / PASS_BEHAVIOR_ONLY |
| remap | 120×40; F2, F3 and registered Ctrl+G p | 28 | 56 | PASS_BEHAVIOR_ONLY / PASS_BEHAVIOR_ONLY |
| caret | 120×40 → actual 80×40 → 120×40 resize | 25 | 50 | PASS_BEHAVIOR_ONLY / PASS_BEHAVIOR_ONLY |

Existing `capture.mjs` probes run both actual absolute executables with geometry,
short sample, hidden sidebar and tool-preview mode. Each episode owns exactly one
recorded successful repository `cargo build --locked`, actual PTY input/VT,
full styled grids/cursor, full original PNGs, stability observations, fixture logs,
source/build locks and comparator reports. No seed session, live API, user config,
new capture mode, reference patch, golden, crop, mask or tolerance change.

History submits, in order, `HISTORY-FIRST Ω界`,
`HISTORY-SECOND\nsecond line Ω界`, `HISTORY-THIRD @note.txt done`, then the same
third text in a fresh session. Actual wire confirms four main requests and two
title requests per side; the fourth main request contains only that fresh-session
input. The `note.txt` canary body does not enter the request. Navigation/recall,
editing a recalled copy, boundaries, Home/session sharing, modal ownership,
remapped old-key refusal, restart and accepted `/mcps` do not generate requests.
Native durable shared history deduplicates consecutive submissions; raw message
rows remain four ordered accepted inputs. Restart preserves their IDs and bytes.
Accepted `/mcps` appends only the history entry before local dispatch.

Native Down restores the unfinished pre-browse draft. Original instead has an
empty forward boundary and refuses browsing a nonempty index-0 draft. These are
explicit differences, not an assertion of identical behavior. Busy-owner precedence
is additionally qualified by the closest native actual-PTY test, not inferred from
the idle paired fixture.

Caret probes send actual SGR left clicks on ordinary Home/session prompt text,
wide characters, combining and ZWJ graphemes, trailing blanks, wrapped gaps, a
blank line and resized text; chip-neighbor priority and modal backdrop are covered.
Click/edit operations precede one explicit Enter. The exact edited user text is
`Проведи RECXON, жду план`; one main request plus one title request per side.
Native accepted/reopened/restarted raw user row is unchanged, with no replay.
The chip-expanded full frame still has a rendering/cursor difference (below).

Across both sides and all three episodes: **28 requests, 18 explicit main
submissions, 10 titles, 24 observation snapshots; zero extra provider/tool/MCP/
project/DCP effects**. Native snapshot data verifies bounded durable history and
raw rows. Original snapshot data does not decode native's schema; its actual wire
and full frames are separate evidence, not fabricated raw-schema equivalence.

## Whole-frame comparison and remaining work

All **156 strict comparisons remain DIFFERENT**, with 2,811 differing paired
cells and 165,950 differing pixels; 71 of 78 cursor pairs equal. Whole grids and
PNGs retain real clocks, version/footer, Home/session surfaces and every difference.
Behavior PASS does not promote a pixel comparison or all VIS12 to PASS.

An actionable observed `caret-chip-expanded` difference has 67 differing cells:
the native final painted row lacks 60 `z` characters, cursor `(26,27)` versus
Original `(86,27)`; six footer-version cells are a separate difference. The next
owner experiment must distinguish chip expansion/end offset from wrapped-row and
viewport visibility using the actual payload and existing editor/layout owner.
No capture normalization is a fix. History edited-copy timing differences are
likewise retained, not treated as evidence of incorrect submitted bytes.

## Complete byte-exact retention

`vis12-proof.tar.xz` and `vis12-proof.inventory.json` preserve **all 988 original
members / 63,666,559 logical bytes**, including direct parent owner/driver logs.
There are 19 direct identical same-root/side PNG aliases, no PNG re-encoding.
Canonical USTAR plus one complete XZ stream: **2,435,860 encoded bytes**;
63,815,680 TAR bytes. Inverse audit checks every original byte without extraction.

| Seal | SHA-256 |
|---|---|
| Archive | `8a279fe3e84bbff3ae3977c1f89f5b85e9553c9e2cdd74899962a5bf10674c64` |
| TAR | `ddc56dee07626533b54e3d07ba3942c608b2b3345ce0e92f0bb7d72101136f7b` |
| Member inventory | `fdd2e91ff119bdc150968b2f652bc79026df9408f5e022631498d2d90cacc989` |
| Inventory file | `db43a20956049e0d4a0f480d026470732c17a9c9540d03c8281d3b7854e807e8` |
| Retainer source | `5517cdf1a68a73d5a4c6caea9c9e464e601fcb0d339a561d80a2368faaec8f1a` |

The earlier independent inventory already included all 19 aliases. Its different
seal is solely from `current-history/current-remap/current-caret` prefixes;
renaming those prefixes reproduces the retained member seal exactly.
All 11,430 prior artifact entries, metadata and regular-file bytes are preserved.
Neither old failures nor original scratch/helper reports were erased or offloaded.

Caps remain: owned evidence 1 GiB, file/encoded 16 MiB, logical/TAR 128 MiB,
2,000 members, XZ dictionary 16 MiB, decoder 32 MiB, Python hard AS 384 MiB.
After the two proof files, before this receipt: allocated 1,064,947,712 of
1,073,741,824 bytes, headroom 8,794,112 bytes; new allocation 2,441,216 bytes.
Independent inverse VmPeak 340,712 KiB / VmHWM 312,576 KiB; no cap increase.

## Captured source and actual checks

Capture association remains native commit above, tree
`340696bb30f9dfcab664557667b4cba82c585722`, dirty diff
`6e8c518ba2cab2ec0784d71ebcd077ff6761f77bce5afc8a894d46d966adbd61`.
Only the evidence-quota prefix was dirty at acquisition. All 709 current producer
files have canonical seal
`46a9cbf17fc6f4ecfca5035fc9dee67a2495e26a63df535c5f2ec89a3ba39b0e`;
native ELF `8802dfc991dc23f503edb549de541a250869d4179544cb34edb92be2150bf499`,
Original ELF `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`. Later gates are not
inserted into original acquisition commands; commit delivery does not rewrite locks.

Actual gates: Python root suite 57 PASS; nearest `pty_t39` shared-history/Home/
restart/wire/active-owner test 1 PASS; `oc-tui vis12_` 6 PASS; locked debug rebuild
restores captured ELF; workspace fmt PASS; measure/pack/reopened audit FULL_BYTE_EXACT.
Independent first and inverse audits PASS, including current source/ELF, one recorded
build per episode, full-comparator bytes and unchanged history/effects. Production
Rust/Cargo/capture sources did not change: previous current full 1843/0/11 all46,
strict Clippy/builds/help/release9 and resource gates remain applicable until the
next production change. No Cargo execution during packing/inverse qualification.

Next: fix and actually qualify the observed prompt-chip/layout owner difference,
then EVERY remaining frozen R1–R6/VIS01–45 outcome and supplement, and final
current-source R6/V09. No whole-task finish or pixel PASS claim.
