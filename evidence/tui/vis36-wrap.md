# Generic/MCP source-space wrapping — T44 handoff

**T44 ACTIVE_NOT_PASS; slice delivery interrupted, no commit/push.** The proven
wrap-separator discrepancy is corrected and fresh paired acquisition is retained.
Unexpected concurrent source writes invalidate current-source gate attribution.
All frozen R1–R6/VIS01–VIS45, supplements and final-current R6/V09 remain required.

## Source proof and correction

Base and acquisition producer: `bf0cfecd70cef9ae21646ed96a6c8c7e50581edb`.
Pinned Original: `2670273ff17da96f85c5826ced57aa1b368754fa`.
Prompt007 whole grids show a real separator after `wrap` at wide y26 x115:
Original foreground `#808080`, native `#ffffff`; symbol/background/modifiers/width
equal. The next source word is on y27; x116 onwards are ordinary white canvas.
Small rows25–27 are equal and the overflowing separators are not painted.

Donor `GenericTool`/`genericToolSummary` (`index.tsx:2620–2675`,
`tool-display.ts:13–20`) derives this exact text from structured primitive input.
`InlineToolRow` (`message-parts.tsx:196–229`) separates the icon/padding from the
styled text; `InlineTool` (`index.tsx:2722–2727`) chooses muted foreground.
Pinned OpenTUI0.5.10 `text-buffer-view.zig:1879–1925` coalesces a fitting word and
separator into its source chunk and drops an overflowing separator. Session
two-cell left/right padding leaves 117/75 content cells at121/79 columns.

Native `tools.rs::generic_block_expanded` now uses the existing
`styled::wrap_source_space_line_limited` for the header. This preserves source
ownership, not a coordinate/color rewrite. No global whitespace/caret handling
changed. Header full/indexed/cache projections share this owner.

Existing finite retention remains: parameters<=2048B, normalized bracketed
summary<=2048B, total retained strings<=4096B with truthful `arguments_limited`;
provider admission1MiB, stream/page/window/cache/security budgets remain unchanged.
RAW/op identity, click extents, paging, Shell success behavior, typed previews,
native denial copy, policy/grants/binding/recheck/cancel/redaction and visual busy
hover versus refused busy close remain covered by existing owner suites.

## Regression and fresh acquisition

New existing-owner regression paints **entire TestBackend buffers**, with literal
source glyph boundaries independently taken from Original Prompt007, including
the fitting wide separator and the overflowing narrow separator. It checks full,
cached/uncached/indexed totals and one-row pages, unchanged RAW and wire/op identity.
Existing long Unicode/fifth-primitive/retained-bound/click tests remain.

Actual RED:11passed/1failed, only the source separator's painted style differs.
First correction experiment passed the wide boundary but exposed a test-width
arithmetic mistake (77 instead of79−4=75), retained as11/1. Corrected test geometry
then gives targeted12/0. Strict workspace/all-targets locked Clippy-Dwarnings and
fmt passed before acquisition. No test deadline/threshold/baseline/ignore changed.

Fresh exclusive `vis36-prompt-current-008` and driver `vis36-prompt-capture-r8.py`:
actual ordinary Prompt at121×40 and79×40, geometry enabled/sidebar hidden/short
sample, all51states per Native and Original, actual locked build per profile.
Each side/profile112checks PASS,26valid/26complete requests,24main+2title,
14real calls,34read-only snapshots and two natural exits0. Once/Reject/Always,
root/child feedback, MCP/read/Shell/glob/URL/Settings remain real fixture effects.
No paid API, episode replay, selector/protocol bypass, crop/mask/tolerance/golden,
clock/cursor normalization or PNG reencoding.

Independent historical whole-frame comparison covers102pairs,408000cells and
55031040pixels: **all102grid and102PNG DIFFERENT**,86cursors equal. Wide summary
rows26–27 and small25–27 are entirely equal in all fields. Full MCP frames remain
DIFFERENT: wide30cells/1552pixels, small29cells/1499pixels, both cursors equal.
The named role repair does not close whole-frame gaps.

Sealed capture source715:
`36501a6bcd4f46868b07e60157fc132390686593341435d896e2cb7a474077b5`.
Capture debug:
`84a8bc866fd4aefa134d9ec56a9bdebdbcf0552fe1371e45131c12e857aed253`.
Acquisition-time release remains the previous-source `dbac351c…`, explicitly not
this source's release qualification. Original `2b082572…` is unchanged. The
retainer's literal008 path admission and existing Python negative-path test are
minimal support changes outside the pinned715-member capture source scope.

## Observed gates and concurrent-write interruption

Observed complete serial workspace:1875/0/11, all46 targets; affected crate537/0;
locked+ordinary debug/help, locked release/help, Python71 and actual release9 all
returned0. Release cases: startup2, TERM01, panic, VIS31two, S07, VIS38, AUD32.
S07/DCP filters contain no bare `--exact`; actual nonzero counts checked.

These results are **not final-current/source715 qualification**. An initial
serial workspace operation hit the1200000ms outer shell timeout after34completed
target summaries with no test failures; child exit UNKNOWN and full partial log
retained. A fresh unchanged serial run with3600000ms outer allowance completed.
No test threshold/deadline changed. The later read-only current-source observer
correctly refused to equate current source with the captured manifest.

At that point Git showed unexpected writes to `Cargo.lock`,
`crates/oc-adapters/Cargo.toml`, and eleven untracked
`crates/oc-adapters/src/codemode/*.rs` files. Source membership726, observed SHA:
`6a6ccf93b33e7eff190d3528afdaf97e89e80772741ee7f0fad7b3b7c41467a2`.
Observed later debug `974619b2…`, release `4c8e6fec…` are not capture ELFs.
Timing/author/source epoch of the foreign writes is not established. Per runbook,
build/stage/commit/push activity stopped; no foreign files reverted or staged.

## Durable proof and resumption

Full originals retained separately per profile:634members each,
wide80659444logical bytes, small65658332. Exact preceding same-side encoded PNG
aliases:7wide/13small. Archives:

- `vis36-wrap-wide.tar.xz`: `d619fd39d2525f41efe62165e938c1f9a4e3bc72f9270980bd608d0ba438b2d8`
- `vis36-wrap-small.tar.xz`: `b4d0ba06dd194ad62fd39c6536ee3ecfc11ebe03113034aa95496edd74b4978c`
- `vis36-wrap-gates.tar.xz`: `fdb089c3ebf6e2033590c55c752dfbd80212f19460cc16051ce23f3eff485d8a`
  (53members9061111logical bytes; full gate logs/programs/baseline/acquisition/failure facts).

Existing encoder only; independent manual single-XZ header/footer/indexCRC,
dictionary<=16MiB/decoder32MiB/singleSHA/noextra-stream and canonicalUSTAR every
member/alias/path/meta audit before and after publication. Acquisition DIR seals
stable;209prior proof seals,21867original files and509DIRs+oldhelpers preserved.
Max observed retainer VmPeak337776640/HWM323624960 bytes under hardAS384MiB.
All caps unchanged: encoded/member16MiB, logical/TAR128MiB,2000members, owned1GiB.
No deletion/offload/reencoding/cap raise; TMP is separate transaction space.

`vis36-wrap-handoff-check.py` explicitly verifies the **historical captured epoch**,
whole comparisons, all byte/meta seals and observed gate logs without promoting
old current-source flags. `vis36-wrap-check.py` intentionally refuses normal final
delivery until current source and executable qualification are re-established.
Checkpoint145 keeps T44 ACTIVE and preserves T59/unrelated task fields.
Final historical observer `vis36-wrap-handoff-after.json` succeeded; source drift
continued to729members by that observation. Initial726 is a timestamped finding,
not a stable final source. All previously claimed equality/counts above were
independently verified for the immutable captured epoch, without source promotion.

Parent must re-establish sole mutation ownership and resolve the unexpected
worktree changes without treating them as disposable. Then qualify the actual
intended source/ELFs, perform final current read-only delivery audit, exact reviewed
stage/ordinary commit/push and post-push audit. Return to full frozen T44 mission;
this handoff is not slice delivery, R6/V09, VIS36 or T44 PASS.
