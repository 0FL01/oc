# Generic/MCP primitive summary — checked T44 slice

**T44 ACTIVE_NOT_PASS.** This slice removes the proven per-key/per-value 120-byte
shortening and four-primitive cutoff at the existing structured presentation
owner. Full frozen R1–R6/VIS01–VIS45, supplements and final-current R6/V09 remain
required. This is not whole-frame, VIS16/VIS17/VIS36, R6/V09 or T44 PASS.

## Source, owner and bounds

Base/acquisition producer: `f2f2025e060bba9ab2d41932b888d344a88ee88b`.
Pinned Original: `2670273ff17da96f85c5826ced57aa1b368754fa`.
`T44_CONTRACT_AMENDMENT.md:1696–1701`, actual donor GenericTool and
`packages/tui/src/util/tool-display.ts:13–20` admit primitive string/number/boolean
values without individual shortening. Native `tools.rs::generic_args` now
derives summary values from the same structured, aggregate-bounded parameters
as expansion; no model-prose parsing. No new registry/store/trait/framework.

Existing provider admission remains 1 MiB; stream preview, storage page budget,
history/window/live/cache retention owners and counters are unchanged. Parameter
key/value retention remains at most `PREVIEW_BYTES=2048`; normalized summary,
including separators/brackets, is independently at most 2048 bytes with UTF-8
boundary truncation and truthful `arguments_limited`. Total Generic retained
strings remain at most 4096 bytes. The approval resource-label 120-byte limit
remains at its separate owner. Expansion has no additional arbitrary line cap.
Validation, redaction, RAW/history, stream security and finite memory guards
remain in force.

## Regression and gates

The existing projection/click scenario now checks a >120-byte Unicode key,
>120-byte mixed-Unicode primitive value, fifth primitive, excluded nested/null
summary data, cached/uncached/full/indexed styled equality, repeated expansion
and every visible click extent at 43/80/120 columns, unchanged RAW and retained
byte bounds. RED101/one failure reproduces the lost suffix/fifth primitive.
Prompt006 exposed a second mistake: normalizing inside the brackets removed a
trailing value space. A second RED101/one failure reproduces that. Normalization
after enclosing the summary preserves the source interior separator before `]`.

Final-source results, all through exclusive `vis36-generic-summary-*` gates:

- Targeted presentation: **11/0**. Affected `oc-tui`: **536/0**, one test thread.
- Locked workspace `--no-fail-fast`: **1874/0/11, all 46 targets**, one test thread;
  default features and unchanged test deadlines/thresholds/ignores.
- Locked workspace/all-targets Clippy `-D warnings`, workspace fmt: PASS.
- Locked and ordinary debug build/help, locked release build/help: PASS.
- Root Python: **71 tests**, PASS.
- Actual release cases: startup isolated **2**, TERM01 **1**, panic restoration
  **1**, VIS31 **2**, S07 equal-view archive **1**, VIS38 DCP equal-view archive
  **1**, AUD32 growing archive **1**: **9/0**. S07/DCP used module-containing
  filters without bare `--exact`, with nonzero observed counts.

Retained failures are evidence, not hidden successes: one interrupted workspace
attempt with unknown child exit; six complete failed workspace runs (three
1873/1, two 1872/2, one 1870/4; each 46 targets and 11 ignored), latest parallel
crate 535/1, isolated debug VIS31 input-paint failure, and five failed observer
attempts. Startup/headless/recovery timeouts varied; later debug failures included
discovery, V04 cold-catalog threshold and VIS31 input-paint latency. Isolated
startup/headless/recovery and V04 checks passed unchanged. Concurrent foreign
CPU/memory workload was observed, but a causal attribution is not proven. Final
serial crate/workspace PASS does not erase these attempts or establish timing
stability. No threshold/deadline/baseline/ignore was changed.

## Actual fresh full paired acquisition

Exclusive Prompt006 (`r6`) is immutable intermediate evidence; its producer
current-source flag is now historical. Final source uses fresh **Prompt007**
(`r7`), not VIS36 auto-mode007. Both profiles ran real full 51-stage Native and
Original consumers: 121×40 and 79×40, Prompt, geometry enabled, sidebar hidden,
short sample, actual locked build in each capture. No paid API or selector bypass.
Fixture model `gpt-fixture-model-1`, advertised `apply_patch` and canonical
`shell(command)`, original Native/provider distinctions remain admitted normally.

Each side/profile: **112 checks PASS; 26 valid and 26 completed requests,
24 main + 2 title, 14 real calls, 34 read-only snapshots, two natural exits0**.
Native Always grant is `apply_patch/approval.txt`; Original is `edit/*`.
These scopes/tool IDs are not translated. Pending `no_started` is a producer
SQLite snapshot heuristic, not CoreEvent/syscall proof; typed admission/recheck
ordering remains separately covered by the owner suite. Typed `ApprovalPreview::Tool`,
resource/binding/grant checks, native denial RAW copy, VIS41 hover paint versus
VIS44 busy-close refusal and successful frozen Shell-card behavior remain covered.

Independent whole-frame comparisons include all five styled-cell fields,
every cursor and decoded pixel, with exact original encoded PNG alias checks:

| Profile | Pairs | Cells | Pixels | Equal grids/PNGs | Equal cursors |
|---|---:|---:|---:|---:|---:|
| wide | 51 | 246840 | 33292800 | 0/0 | 43 |
| small | 51 | 161160 | 21738240 | 0/0 | 43 |
| total | 102 | 408000 | 55031040 | 0/0 | 86 |

All **102 grid and 102 PNG comparisons remain DIFFERENT**. No crop, mask,
tolerance, goldens, clock/cursor normalization or PNG reencoding. Named summary
diagnosis is subordinate to these whole comparisons: final `mcp-completed` y27
is entirely equal in every field in both profiles, including the previously lost
30-character suffix and trailing space. Small summary rows25–27 are equal; wide
row26 still differs at x115 foreground of a blank wrapping boundary. Whole MCP
frames remain DIFFERENT: wide30cells/1497pixels and small27cells/1394pixels,
both cursors equal. Remaining chrome/labels/permission/detail differences are
in-scope work, not a reduced acceptance definition.

## Actual source and executable identities

Actual source membership **715**; final manifest SHA256:
`e8dc500cd1d1b74b8499a62152a58b9846d764b7eb361c0828b8813be554675e`.
Only `tools.rs` and `tools/presentation_tests.rs` differ in the capture source
scope. Old producer identities, seals, receipts and flags remain immutable.

- Ordinary debug: `e1c86b37619525baa9e1b56b05e3890d49d89bd041ef7a7ef88638c1eeb5dfb9`.
- Ordinary release after locked rebuild:
  `dbac351c3b647dc376fc4d685ddf85c9babb8460229101d0c9ad289211b75ff9`.
- Original: `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.

Acquisition-time release `b813cc4dc7f26e92e3dd76c3f8ac2df3337eee640ffd8dd061ca290838c3d407`
is retained separately. An observer correctly refused to equate it to restored
ordinary release. New AST-only r1/r2 observers qualify ordinary build/help
identities without rewriting acquisition facts. No manual ELF copy.

## Durable bytes, metadata and independent inverse

Separate <=128 MiB archives retain **634 originals per profile**, including
full frames/VT/render/cursor/protocol/commands/build/source/behavior/original driver
and logs. Prompt006 logical wide80600294/small65522879 bytes; final Prompt007
wide80544904/small65558224 bytes. Only direct, preceding, same-run/same-side,
exact original encoded PNG aliases: wide7/small13 in each episode.

Archive SHA256, `evidence/tui/vis36-generic-summary-*`:

- `wide.tar.xz`: `8a7d8f0592b978758ea6086ae9953b3a3a785b2613b5a43855d0b76393d72a07`
- `small.tar.xz`: `26a82139167ed2371c84eb8e4bf03ec525d99f2e91c50b525e88beab9ebe96aa`
- `wide-007.tar.xz`: `ff859473090f4389337e1545fdf940554486711b95317e61ca46f580f98e4cac`
- `small-007.tar.xz`: `bc9d02c47492cee976e53b1075a1ecdaef584cc6e495816fd43a544ea730af54`
- `gates.tar.xz`: `7129d653aea2e69315ea362811dd06d13e7b8f5413f233049ed0f1a36f91e371`
- `gates-r1.tar.xz`: `1a12598af58ce11620c410031fbbea59783de4e23e5cea515d72ebdf833f94eb`
- `gates-r2.tar.xz`: `e283c59667691129a3c09658c8b2682fec9ec215986a961931b7860083d0d037`

Final gate bundle138members8122524logical/zeroaliases. Every original byte/path,
original metadata and acquisition DIR stability independently checked, in-memory
and durable manual single-XZ header/footer/index CRC/dictionary/decoder/single
SHA/no-extra-stream and canonical USTAR every-header/content/alias/order/padding
inverse. It is not the producer inverse. Old functions reused via AST-only pure
declarations, without old module/HEAD/entrypoint execution. **195 prior proof
byte+metadata seals, 19208 legacy loose files and 495 DIR seals** remain intact.
All old helpers and AGPL provenance/license remain preserved.

Immutable final observation: `vis36-generic-summary-after-r2.json`; final read-only
checker: `vis36-generic-summary-check-r2.py`. Retention observed max VmPeak336838656
and HWM323002368 bytes; final checker peak251981824/HWM236867584, below unchanged
hard AS384 MiB. Caps remain member/encoded16 MiB, logical/TAR128 MiB, members2000,
XZ dictionary<=16 MiB/decoder32 MiB, owned1 GiB. No deletion/offload/cap raise.
Pre-final-report allocation1059512320/1073741824; authoritative pre-stage/post-push
recounts include later receipt/report additions. TMP is separate transaction
space, not durable proof offload. Checkpoint144 keeps T44 active and preserves
T59 and every unrelated task field. Parent continues the entire frozen mission.
