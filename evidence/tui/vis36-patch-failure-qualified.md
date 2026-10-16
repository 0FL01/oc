# VIS35 — failed Patch child spacing and semantic roles

Status: qualified bounded renderer slice; **T44 ACTIVE_NOT_PASS**. Frozen
R1–R6/VIS01–VIS45, supplements and final current-source R6/V09 remain required.
The existing VIS36 auto-cli acquisition supplies the real failed Patch episode;
its full frames are not VIS35/VIS36 pixel PASS.

## Contract and owner change

Frozen R4/VIS35 requires Patch-failure geometry/theme parity. Actual pinned TUI
`routes/session/index.tsx` BlockTool `gap={1}` and ApplyPatch error children use a
red header, a blank child gap and muted recorded detail. This is the TUI owner,
not the web renderer. In retained008, native placed detail adjacent to the header
and colored it red; its two layout-padding cells also inherited muted foreground.
The fixture operation is **failed**, not the separate denied shortcut.

`crates/oc-tui/src/tools.rs::patch_block` now gives failed Patch layout padding
background only, inserts one child gap only when nonempty recorded detail exists,
and paints that detail muted. The failed/unknown header stays red and retains the
distinct unknown-outcome label. Successful Patch, Generic/MCP, other tool frames,
typed effect validation, output authority and recorded input/output are unchanged.
No empty-detail phantom separator is generated.

The actual denial text remains `error: denied apply_patch`; Original retains
`Permission denied: edit`. Nothing translates or rewrites either copy. The frozen
owner `fee0123a05007155f7253330f89a262951c23749`/R4/VIS16 Shell success override
still suppresses generated exit-zero notice and empty-output placeholder; no
Shell renderer, source contract or baseline was changed.

## RED → GREEN and invariant checks

The new substantial scenario is in existing `tools/presentation_tests.rs`:
`vis35_failed_patch_detail_uses_child_gap_muted_text_and_unstyled_padding`.
It covers failed and unknown, UTF-8 recorded detail, every padding row, no fabricated
effects, empty detail without a gap, full/cached rendering at widths43/80 and both
expansion choices, actual whole TestBackend paint equality and complete ToolOpView
immutability. Actual RED: 0 PASS / 1 FAIL, old five rows versus required six.
After the minimal owner change, all15 `patch_` scenarios passed. The pre-existing
failed part of `golden_apply_patch_created_patched_deleted` now expects the required
gap and muted detail while preserving its red-header assertion. This expectation
change follows the frozen/pinned behavior, not a test-disable or baseline change.

An owned read-only observation helper decoded retained008 structured cells without
replaying its provider episode: borderx2, paddingx3/4, native muted versus Original
white layout foreground, raised background and no modifiers. Its complete program
and output are retained with the command gates.

Current-source gates actually executed:

- `cargo test --workspace --locked --no-fail-fast`: **1868 PASS / 0 FAIL /
  11 unchanged opt-in ignore**, all46 targets, TUI534/PTY58;839.834s.
- Strict `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check`, locked and ordinary debug build/help: PASS.
- `cargo build --locked --release` and actual release help: PASS;156.115s build.
- Nine actual release/resource cases across seven completed Cargo targets: isolated
  binary/discovery startup2, TERM01 session terminals1, PTY panic restoration1,
  VIS31 idle/input/provider pacing2, S07 equal-view archive1, DCP equal-active archive1,
  AUD32 actual-binary growing archive1. Every target passed with no ignore; no
  zero-test run or debug ELF substitution is counted.
- Root Python70, complete changed retainer guards and docs/progress/diff checks: PASS.

Release S07 retained24231B at archive0/3000, queue lag0/children0, HWM27384→27896KiB.
AUD32 actual HWM20652→22968KiB, children0, database942080→152944640B. Complete
commands, timing and resource logs are retained, including RED and a retainer
preflight KeyError from the initially omitted009 capture-count literal. That failure
wrote no archive; adding the exact fixed009 count preserved all prior campaign guards.
Root70 and the complete real measure/pack/audit then passed. No source/test deadline,
threshold, ignore, security, RAW, policy, schema, auth/env or public API was weakened.

## Fresh real acquisition and effects

Exclusive roots under approved TMP:
`vis36-mode-current-009/{cli-wide-r9,cli-small-r9}` at121×40 and79×40.
Both actual Native and pinned Original recorded all38 behavioral checks at each
size. Existing16 stages per side yield64 whole PNGs and64 strict grid/PNG attempts,
**all DIFFERENT**; no crop, mask, tolerance, golden, cursor/clock normalization,
PNG re-encoding or old-episode replay.

Each actual side/profile:18 valid provider requests/18 completions (16 main,
2 title),8 actual tool calls,11 read-only snapshots and2 natural exit-zero
generations. Actual saved Autoaccept→Prompt, effective CLI override, root/child
Once, mixed Deny, zero Always grants and effect files remain verified. Relaunch
creates a fresh admitted root with saved mode/profile; no same-root restart claim.
Native has8 operations (7 complete/1 failed),8 completed turns/16 messages;
Original V2 RAW has32 messages/8 bound tool parts. SQL read-only snapshots are not
CoreEvent/syscall traces; typed owner ordering remains separate evidence.

Independent full recount:32 pairs,64 whole PNGs,128000 styled cells and17264640
RGBA pixels checked;26523 different cells/3263365 different pixels;28/32 cursor
pairs agree. Four complete600-cell filtered Settings modals remain equal. Both real
failed Patch frames have equal header styled cells, child/card geometry, detail
semantic style and every layout-padding styled cell. Their distinct raw detail is
explicitly recorded and remains in the strict whole-frame comparison, not masked.
These named role diagnostics do not replace the full comparisons or qualify T44.

## Complete command-byte delivery

`vis36-patch-failure-gates.tar.xz`:55408B,
SHA256`fb34723d55cf09ea12d45bc8faa8ede04b3740e0821e913a746bf9ffaf6e3688`.
All29 actual command JSON/full-log pairs plus the unchanged gate wrapper,
read-only retained008 observation helper and gate-retainer program:61 members,
255489 logical bytes, no aliases; member inventory
`5442002fb3223dd77c05f61fc8cb0033ce91b29fde47bfc9ebebba44d7965173`.
Independent manual XZ/USTAR inverse of every original command/helper byte passed
both before publication and after reading the durable archive. VmPeak244256768B/
HWM72413184B below hard384MiB. RED101 and failed preflight1 are explicitly retained.

## Source, executables and full-byte retention

Canonical713-input seal:
`1b0e54c58b2a63148da9e17a72f44cec992012deed371925cac6b8a9f1d9b678`.
Only `tools.rs` and `tools/presentation_tests.rs` changed versus008.
Native debug ELF484379704B/ET3/machine62:
`c35570f3bce21a882a697471e45736d4de3ccca389ce1b9126b828aea0394feb`.
Original200529376B/ET2/machine62:
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
pinned `2670273ff17da96f85c5826ced57aa1b368754fa`.
Captured producer `89f8df1c…` stays distinct from the delivery commit. All old
source flags, helpers, captures and receipts remain immutable historical evidence.

`vis36-patch-failure-proof.tar.xz`:2188228B,
SHA256`3410554d7565a0d32a28b016b49cf3108c92e0b9ae8a6e9d617803b1d7adbd53`;
decoded canonical USTAR39833600B,
SHA256`3c4055dfaa10770a90ba6475ef521a7a80f5cc08525ff9db60cc43d0739ac54f`.
All422 original files/39682090 logical bytes,4 same-run/same-side byte-identical
original encoded-PNG direct aliases; member inventory
`f7874a1171f8210cb9fec337c6aad65d865d4210360a7ef2b8eae24082d6ced7`.
Retainer source at pack
`1814fc636885ab3643982f67e9c1a4dfdfea689e210aba1f0b3fc6e0359a3cfc`.

Independent `vis36-patch-failure-audit.py` imports no producer decoder: sealed
manual single-XZ headers/footer/index CRCs/dict16/decoder32/SHA/no trailing payload,
direct canonical USTAR inverse of **every422 original byte/path/alias**, original
file/directory metadata, current713 source/actual ELFs/helpers and **131 prior
archive/inventory/archived-receipt byte+metadata** checks, then complete frame/PNG/
cursor/command provenance recount. After report17611B,
SHA256`a89db3db18751b614a54e90172b500660d54d8bab0f546d378f219bd7e39c19f`;
VmPeak193007616B/HWM136593408B below hard384MiB.
Its captured-base/exclusive-output entrypoints must not be blindly replayed after
delivery; the original producer identity is not rewritten to Git HEAD.

## Resources and continuing acceptance

Post-independent-report actual owned allocation1072017408/1073741824,
headroom1724416B **before** this receipt/command bundle. Final measured allocation
after receipt/bundle is1072091136/1073741824, headroom1650688B; a final delivery
checker remeasures it. All established prefixes plus additive `vis36-` count in the same
1GiB cap; approved TMP is separately accounted, not durable offload. Limits remain
16MiB file/encoded,128MiB logical/TAR,2000 members, XZ dict16MiB/decoder32MiB,
Python hard AS384MiB. No qualified proof deletion, offload, re-encoding or cap raise.

T44 is ACTIVE_NOT_PASS. Full VIS35/VIS36/R6/V09 is not passed by this renderer
slice. Next source-check the sampled completed Subagent icon foreground (not yet
a proven cause), then every remaining frozen R1–R6/VIS01–VIS45/supplement and final
current-source R6/V09. Do not substitute regional checks for full acceptance.
