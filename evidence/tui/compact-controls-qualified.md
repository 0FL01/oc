# Functional compact rail — qualified slice, not T44 PASS

Date: 2026-10-12. Active T44 remains **ACTIVE / NOT_PASS**. Every frozen
R1–R6/VIS01–45 outcome, supplement and final current-source R6/V09 remains
mandatory. This receipt qualifies the described owners, behavior and retained
evidence; it does not qualify whole R3/VIS44/T44 pixel parity.

## Source and invariants

The pinned **Terminal TUI**, not the web UI, owns compact Search through
`component/session-tabs-rail.tsx` and highlighted half-rows/vertical Add through
`component/session-tabs.tsx`, donor `2670273ff17da96f85c5826ced57aa1b368754fa`.
Native shared strip geometry now exposes the compact one-row Search rectangle.
A matching plain-left press/release invokes existing guarded `OpenSessions` /
`LoadSessions`; Middle Down does not close or activate this control. Noncompact
rails have no Search control in the pinned donor and gain none natively.

Static bounded half-rows use existing selected/hovered state, semantic raised
surfaces and neighboring highlights. Vertical Add uses its raised idle/hover
surface and glyph-only tint; blank canvas foreground and horizontal Add behavior
are preserved. Search's header separator has the donor's higher paint order.
There is no new clock, store, registry, query variant, poll or production JS host.

Existing current-geometry press matching, modifier/drag/resize cancellation,
busy/read-only/Location/CAS guards, draft/caret ownership, RAW/provider/tool graph,
redaction and resource limits remain intact. The reported stale Search press after
Right Down was refuted by the existing fallback that clears `tab_down`; no redundant
workaround was retained. Native accepted rail width remains Location-scoped deck
CAS; Original's layout preference remains global, not an asserted scope equivalence.

## Executed checks

- New meaningful existing-owner TUI scenario: actual 80×24/120×40/160×48 paint,
  current five-column width, Search/half-rows/neighbor joins/Add idle and hover,
  matched Search click, cancel/draft/caret restoration and stale/modifier/drag/
  Middle/resize/busy refusal. Baseline missing `⌕` was RED, then GREEN.
- Current locked full workspace: **1,843 passed, 0 failed, 11 unchanged opt-in
  ignores, all 46 Cargo targets**; TUI 516, binary 113. Internal concurrent
  scenarios and product/test deadlines remain unchanged.
- Workspace fmt, strict locked all-target Clippy, locked/ordinary debug and locked
  release builds, both help commands, Node producer syntax and Python **55** PASS.
- Release nine gates PASS: startup four/19 routes/8 GET-only requests, actual
  TERM01 terminal recovery, VIS31 two pacing tests, S07 and AUD32. VIS31 idle has
  zero CPU ticks and worker lag is zero. S07 archive 0→3000 retains 24,232 bytes,
  window152/cache8581, zero children/lag; HWM28488→26880 KiB. Actual AUD32 archive
  8→3000 HWM20808→22396 KiB under the original 64 MiB guard. No real-user strace
  claim. Debug locked ELF restored after release gates, before acquisition.

## Actual running proof and preserved failures

Current `compact-controls-current-003/compact` uses the real pinned Original and
rebuilt native binary: configured 42-column rail → real drag5 → 160×48/80×24/
160×48 resize, actual Search hover/plain-left click/Sessions/Escape, same Gamma
answer/Unicode draft/composer caret, real Add idle→hover without activation.
Inactive/active/last Middle Down, inert Up/Drag/LeftUp, Home, natural generation-0
exit0, restart and painted Sessions mouse reopen retain the same Alpha Root ID.
Generation-1 forced capture teardown is recorded separately, not called natural.

Each side makes exactly **six seed/title requests**, no additional close/search/
restart/reopen requests, tools or project effects. Twenty-four current persisted
owner snapshots preserve deck/revision, Root IDs, settled history and DCP. Width5
holds post-drag, empty-deck restart and history reopen. Before-first-paint ordering
is a source contract, not a fabricated transient runtime observation.

Two prior Original failures remain complete and visibly FAILED. Attempt001 Add
was already high before the hover baseline; attempt002 outside-only movement did
not clear it. Pinned OpenTUI emits over before the compacting drag destroys the
old hovered “New session” label and deliberately skips out for destroyed targets.
The final probe re-enters the surviving real plus, waits for processed high paint,
leaves, captures real idle, then re-enters for measured hover. Measured re-entry→
leave intervals are 1072 ms native / 1069 ms Original. No reference/native renderer
patch, longer product deadline, pixel mask or coordinate/time normalization.

Both sides' five-cell Search-hover/Add-idle/Add-hover rows match: **15/15 styled
cells**. Composer caret `(31,18)` is restored; reopened ordinal `(2,3)` and caret
`(10,42)` match. All **18/18 cursor pairs** match. All **36** current strict full
comparisons remain DIFFERENT: **1019/69120 paired cells**, **78557/9321984 pixels**.
History reopen differs **20 cells/733 pixels**, entirely on attribution; the whole
compact rail matches there and in Add-idle. Real clock/throughput, tooltip,
rail-edge/half-row, Home/version and Sessions differences remain unmasked.

## Complete byte retention and source association

Every original file, including direct parent owner/driver logs, is self-contained:

| Scope | Members | Logical bytes | Encoded bytes | Same-root/side PNG aliases |
|---|---:|---:|---:|---:|
| Current003 | 234 | 18317067 | 627728 | 8 |
| Historical001+002 | 305 | 24838309 | 807236 | 13 |
| Total | 539 | 43155376 | 1434964 | 21 |

`compact-controls-r3-proof.tar.xz` / `.inventory.json` retains 36 opaque PNGs;
`compact-controls-diagnostics-proof.tar.xz` / `.inventory.json` retains the 49
historical opaque PNGs and both full failures. Historical first24/12 DIFFERENT/
24 BLOCKED and second25/12 DIFFERENT/26 BLOCKED retain Native PASS / Original
FAILED and `current_source:false`. They are not current-source qualifications
or external task blockers. All original scratch, earlier proofs, failures, RAW,
source receipts, private helpers and reports remain preserved.

Independent in-memory restoration verified every membership/original byte,
709 current source hashes with exact acquisition JS `localeCompare` seal, one
recorded successful repository locked build, actual ELFs/donor, all85 whole opaque
PNGs, every full comparator field, persisted identities/history/SQLite-WAL and
effect boundaries. No integrity or slice-behavior blocker was found.

Immutable actual capture base `d15678b47fe80a38f5bee90a30dff43a447aaed0`, tree
`18d753447be304bce0f87c899e47821d08905431`, dirty diff
`a654e77952b10bfb6b5fa6fc40e4c4ca8f6c2e629c44a5fb9abc8fae120f73c9`.
No post-delivery source-association rewrite.

| SHA-256 seal | Value |
|---|---|
| Current archive | `e95fb27713aee6aa99bb36ce43ee4fe420adac1835718f6f136cfcb0a12fe308` |
| Current TAR | `39f5d8d0669612df637ba708739729ffea0e37025146bc28ea147fae4a2e89b7` |
| Current members | `22a84fefcbc89d0f0aada4e2d7a29fa3a6288740b58f215266472d2915182de2` |
| Historical archive | `9b62ee3f65a9ae9cc4da894dd786d8eb01fb292b38a9a8ecc1caa6a511a408d6` |
| Historical TAR | `65c33574c54830742f199ae441971fcbb002d7580d31e4703a4e03a7b34fdeb5` |
| Historical members | `1715d462e6bdd1415b659681734025f6ac2706ac0a6c6525d97eae4bafb8755d` |
| Current709 source | `46a9cbf17fc6f4ecfca5035fc9dee67a2495e26a63df535c5f2ec89a3ba39b0e` |
| Native ELF | `8802dfc991dc23f503edb549de541a250869d4179544cb34edb92be2150bf499` |
| Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Retainer at pack | `7485495b437178a1be9417037bde77f2f7b7f69ad55c472a7c57a8967a4a7901` |

Pure USTAR/single XZ, independent pre-encoding inventory, complete inverse and
exclusive durable file/parent fsync; only direct byte-identical same-root/side PNG
aliases. No PNG/JSON/VT re-encoding, predictor, correction library or baseline.
Unchanged file/encoded16 MiB, logical/TAR128 MiB, members2000, XZ dictionary16/
decoder32 MiB, Python AS384 MiB and owned1 GiB. Observed audit VmPeak/VmHWM
228460/156268 KiB; owned allocation1062494208/1073741824, headroom11247616 bytes.
All `compact-controls-` payload counts; no cleanup, archival, offload or guard raise.

Physical code inventory:541 files/293442 lines/11791911 UTF-8 bytes. Existing
application5460 and turn5307 soft advisories are unchanged; the natural next seam
is tool settlement/publication, not cosmetic moves to reduce a counter.

## Next required result

Qualify existing VIS12 prompt-history boundary/recall/Home/restart and mouse
click-to-edit through real paired running captures and existing guarded owners;
do not invent an unsupported noncompact Search control. Then close EVERY remaining
frozen R1–R6/VIS01–45 outcome/supplement and final current-source R6/V09 before finish.
