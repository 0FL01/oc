# VIS37 tab-layout roles and static tint quantization — qualified slice

Status: **T44 ACTIVE_NOT_PASS**. Full frozen R1–R6/VIS01–VIS45, supplements and
final current-source R6/V09 remain required. This receipt qualifies the named
header behavior and records complete unmasked comparisons; it does not turn
remaining differences, historical PASS or owner-region equality into whole PASS.

## Source and observed cause

- Base/producer: `a0655da267ae07328944447d3160c8f901a59b3a` on
  `agent/oc-rust-port`; pinned donor
  `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Actual pinned TUI owners: `component/session-tabs.tsx`, `component/tab-pulse.tsx`
  and `theme/color.ts`. Native owners are `shell.rs::deck_tab_line` and
  `blend_tab_pulse_color`; nearest regressions remain in `shell/tests.rs`.
- Question attention had eleven layout blanks with white instead of base FG.
  Native now gives only that attention's layout blanks their observed base role;
  idle/static-busy canvas remains white, title content/attributes remain distinct.
- The remaining 009 background delta was **not a clock phase**: all three protocol
  profiles explicitly recorded `animations: false`, and all 27 question-attention
  states had the same static `#2c2833` versus `#2c2933` at the post-indicator gap.
  Pinned `tint()` quantizes each stage color before floating-point compositing.
  Native skipped that intermediate rounding: green `95.8` versus `96`, followed
  by opacity `0.1591859568`, produced final green `40` versus `41`.
- Exact regression RED `Rgb(44,40,51)` versus expected `Rgb(44,41,51)`; minimal
  pre-tint `.round()` and corrected blend comment. The subsequent floating-point
  glow/sweep/flash/completion chain still rounds only at its final cell. No
  coordinate, cursor-FG, theme-canvas, timing or generic renderer workaround.
- Only `shell.rs` and `shell/tests.rs` changed in the 713 captured inputs relative
  to 008. Raw history, provider routing, model selection, permissions, schemas,
  captured request bindings and user/authoring configuration were untouched.

## Fresh real pairing and byte-exact preservation

Existing 18-stage question driver, actual keyboard/mouse/provider consumption,
three sequential profiles `80×24`, `120×40`, `160×48`, no overlapping Cargo builds.
Each of six sides passed all 18 behavioral stages. All original001–009 episodes,
failed states and helpers remain byte/metadata preserved, not replayed.

009 is now historical/intermediate after the rounding regression/fix; its immutable
receipt retains its pack-time flags. Its full 108 comparisons were DIFFERENT,
including one genuine cursor pair difference. It is not current-source proof.
Current010 has **108 whole PNGs / 108 strict comparison executions**:

| Profile | DIFFERENT | EQUAL |
| --- | ---: | ---: |
| small | 26 | 10 |
| wide | 14 | 22 |
| tall | 22 | 14 |
| Total | 62 | 46 |

No masks, crop, tolerance, goldens, PNG reencoding, cursor/time normalization or
replacement of whole-frame results with named-owner diagnostics.

- `vis37-form-r10-proof.tar.xz`: **1,624,864 B**, SHA-256
  `0b07d5ea81d1aa2f152b2c26f326965fc0320d613fa422d25ad35e180e634de6`.
- Decoded canonical USTAR **55,541,760 B**, SHA-256
  `4572918c8930402861453cc2abbf57b42115538891023725f0fdc1220f3b0a1f`.
- All **704 files / 55,984,207 logical bytes** retained; 27 direct same-run,
  same-side aliases represent byte-identical original encoded PNGs only. Member
  inventory `464969fd797964dc9710141873f69fd3f12ca39057167a634e6092902ac5eee0`.
- `measure → pack → audit` FULL_BYTE_EXACT. New independent
  `vis37-tab-color-final-audit.py` used the sealed manual XZ/USTAR readers, not
  the producer codec, inverse-checked every original byte, checked metadata
  stability and independently recounted all full styled grids/PNG pixels/cursors.
  Before/after reports preserve actual argv, producer identity and helpers.
- Independent after report SHA-256
  `658fba96bb34cc87378b1dda9e825def2b061aa1470ae0e32e23bc0ffaca020a`;
  VmPeak **239,378,432 B**, VmHWM **165,552,128 B**, under hard 384 MiB.
- **91** prior archive/inventory/archived-receipt files remain byte and metadata
  unchanged. Existing 009 audit/receipt and all earlier evidence stay immutable.

Independent current010 totals: **54 pairs, 259,200 styled cells, 34,953,984 RGBA
pixels checked; 2,490 cells / 210,603 pixels differ; 54/54 cursor pairs agree;
25 whole grid pairs and 21 whole PNG pairs equal**. All **51 complete header
rows** match every cell's symbol, FG, BG and modifiers, including 27 question and
24 non-question states. All 27 pending-owner, three answered-owner and six
cancelled-row full styled regions/geometry match. Whole VIS37/R6/V09/T44 do not.

Current713 source seal
`5e108520e1504102ea0dc70818ebb1027eb7af4334bc475f877d0cebce47aff3`;
native debug ELF **484,335,816 B**,
`658dd4f4775678a38d79b53214b400499d81ac9321971e7606b5ec87215cf10c`;
donor ELF **200,529,376 B**,
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Captured producer identity is not rewritten to a later delivery commit.

Actual wire remains 24 valid providers (18 main, six title), 12 question calls,
12 user and six explicit answer submissions, 30 read-only snapshots; native typed
and actual Original V2 RAW/part IDs remain bound. No observed extra project/Shell/
MCP effects, edit/review/restart/cancel continuation or re-ask. DCP tables unread;
generation0 natural exit0, generation1 forced harness teardown, not clean-final
lifecycle proof. These bounded observations are not blanket effects qualification.

## Current-source code and release gates

- Exact header-role RED→GREEN; all **71 shell tests** PASS, including existing
  non-question/white-canvas, busy, permission, title and pulse/rail regressions.
- Final complete `cargo test --workspace --locked --no-fail-fast`: **1,858 PASS /
  0 FAIL / 11 unchanged opt-in ignores, 46 complete targets**; TUI528, PTY58.
- Strict workspace/all-targets/locked Clippy `-D warnings`; fmt check; locked and
  ordinary debug build/help; locked release build/help PASS.
- Nine actual release/resource cases PASS: isolated startup/discovery2, TERM01,
  panic restoration, VIS31 idle/input pacing2, S07 equal-view archive, VIS38 DCP
  archive and AUD32 active-context/archive bounds. Exact commands/full measurements
  are in `vis37-tab-color-gates.tar.xz` and its inventory.
- Root Python **68** (retainer15/archiver6 included), question fixture2 PASS;
  docs/progress/diff checks PASS. Changed-size advisory: six code/dev files,
  8,528 physical lines / 388,340 UTF-8 B; no changed file above 5,000 lines.
- Retained failures: initial role/title assumption; exact tint RED; intermediate
  workspace paging timeout, isolated unchanged paging PASS; serialized complete
  current suite 1855/3/11 with burst/geometry/oversized-line timing failures.
  Those three passed individually on fresh fixtures unchanged; final standard
  two-thread full suite passed all 1858. No test/deadline/baseline/ignore weakened.
  Isolated PASS were not substituted for the final complete gate.

Complete gate bundle: **95 members / 662,500 logical bytes**, 46 exact command
records with full original logs and three acquisition/audit helpers, including
every failed attempt. `vis37-tab-color-gates.tar.xz` **64,632 B**, SHA-256
`795613bbe12356160c5f68faa71ad2f8ed76072e7281c605d60ae3c8d00eb777`;
independent manual XZ/USTAR inverse checked all original bytes before and after
durable writes. Gate verifier VmPeak **241,508,352 B**, below 384 MiB.

## Bounded storage and next required action

Only eligible **actual FAILED** `model-shell-004` was losslessly archived using
the existing narrow transaction; 003 was PASS_BEHAVIOR_ONLY/DIFFERENT and left
untouched. All 69 original files / three directories' bytes, paths, modes, UID,
GID and nanosecond mtimes were independently PAX/XZ inverse-checked; failure
pointer remains visible. Archive **288,956 B**,
`a05e4b5964bf41c724758b812bd06fd7cdd8d6ef7f31311a6152307a30a660d5`;
3 MiB reserve, **5,632,000 B** freed, no qualified proof deletion/offload.

Current010 after-report allocation **1,071,730,688 / 1,073,741,824 B**, headroom
**2,011,136 B**, before bounded delivery records; final actual recount is
authoritative. Caps unchanged: 16 MiB file/encoded, 128 MiB logical/TAR, 2,000
members, XZ dictionary≤16 MiB/decoder32 MiB, Python hard AS384 MiB, owned1 GiB.
TMP accounting is separate and is not durable offload.
After the receipt/gate bundle, actual owned allocation is **1,071,812,608 B**,
headroom **1,929,216 B**; final read-only delivery check revalidates the live count.

Next: source-check the observed small-viewport transcript offset before long/
custom/cancel-pending forms against pinned first-item margin/padding, preserving
full/indexed/replayed behavior; then every remaining frozen R1–R6/VIS01–VIS45,
supplements and final current-source R6/V09. Checked commit/push per major slice.
