# T44 completed Write — qualified current-source slice (2026-10-10)

## Result and unchanged authority

Completed native Write now presents its captured, numbered requested content,
using the existing shared native syntax owner. This is not a workspace reread,
restorable file image, or reconstruction from incomplete Update hunks. Original
`Write` at pinned `opencode/packages/tui/src/routes/session/index.tsx:3044–3077`
also displays the captured request. Failed/cancelled/unknown operations retain
the actual confirmed-effect/error presentation; legacy or omitted previews are
explicitly unavailable. Model result, input/RAW, operation identity/state, file
effects, permission/approval, draft/selection and recovery authority are unchanged.

The existing `tool_output::Presentation` carries optional typed Write facts.
The captured output-config/secrets owner redacts the **complete** requested path
and content before UTF-8 clipping. Existing preview2048B/120 source rows, path1024B,
serialized-record32768B and admission limits remain. Retained-byte accounting
includes both strings; omitted metadata is not a logging failure or replay reason.
Only known successful Write publication attaches it. Live/history/fork/reopen use
the same existing presentation owner, without a new query/store/poll/registry.

Numbered rendering preserves empty/trailing lines, multiline Unicode/comments,
the existing two-space TAB display, shared grammar/styles and source-space wrap.
OpenTUI0.5.10 `LineNumberRenderable.ts:172–185,317–327` establishes the gutter:
minimum3 already includes padding; digits depend on wrapped virtual-line count.
Native measures/emits with the same bounded wrapper. Only digits have muted fg;
left/right padding inherits terminal fg. Continuations do not invent source
numbers. The512 displayed-row guard remains visible. No runtime JS/WASM dependency,
new lexer, donor/cache mutation or license change accompanies this slice.

## Confirmed adjacent UI07 fix

An unchanged real PTY regression intermittently lost the explicit refused-reload
outcome. Temporary bounded sanitized tracing established that Enter dispatched,
the job painted/joined, and the refusal was published. A queued provider hint then
replaced Error feedback with Warning about8.51ms later. Missing Enter, stale slash
options, busy guard, missing job wake and a speculative `dirty=true` fix were
rejected by actual reproduction; the failed workaround and all tracing were removed.

`finish_reload_refusal` now consumes the surviving owner's current provider **and**
MCP facts through existing `refresh_provider_views` before explicit Error feedback.
A queued same-cause hint is no longer a new alert; genuine later changes retain
ordinary behavior. Failed reads preserve selections/settings/every draft. The new
deterministic owner test fails on the original MCP-only ordering and passes on the
fix. Twenty consecutive unchanged UI07 process tests and the full55 PTY target
passed without tracing. No PTY assertion, deadline or protocol was modified.

## Actually executed current gates

- `cargo test --locked --workspace --no-fail-fast -- --test-threads=1`:
  all46 results, **1819 passed /0 failed /11 unchanged opt-in ignored**;
  includes505 TUI and107 binary unit tests. Serial test functions do not disable
  the scenarios' internal concurrency. An earlier shell20min timeout interrupted
  a no-failure partial run; the complete rerun used a longer shell deadline,
  without changing any product/test deadline.
- Workspace fmt, strict locked all-target Clippy, `git diff --check`, debug locked
  and ordinary builds, release locked build, both `oc --help`: PASS.
- Release actual startup:19 routes and8 discovery GET-only cases; TERM01 real
  raw/control/resize/hide/focus/shutdown/crash/no-replay: PASS.
- Release VIS31 two tests: idle CPU0ticks; high-rate input/provider bursts retain
  zero worker-event lag. S07 archive0→3000: RSS26420→27296KiB,
  HWM27236→28640KiB, retained24232B/window152/cache8581B, child0/lag0;
  original guards unchanged.
- Release AUD32 actual binary archive8→3000 pairs plus turn logs: HWM20764→23820KiB,
  DB942080→152944640B, child0. Additional bounded construction test: PASS under
  original8MiB bound. These isolated fixtures are not a real-user strace claim.
- Python51 tests, documentation/registry/journal structure checks: PASS.
  Wrong guessed `check_code_size.py` name was corrected to existing `code_size.py`,
  not replaced with a new gate. Physical-size advisories are not acceptance waivers.
  Inventory537 files/287638 physical lines/11529912 UTF-8 bytes; existing
  `application.rs`5458 and `turn.rs`5242 remain advisory. The15-line publication
  addition stays with its owner; the next natural seam is tool-result settlement,
  not a cosmetic whole-file move or new owner introduced for this small slice.

The prior one-off beta mention timeout was not assigned a speculative cause or
"fixed" by increasing deadlines; unchanged whole34 T42 and the current complete
workspace passed. Independent focused source review found no concrete regression.

## Actual paired R3 episode and complete retention

R3 uses the unchanged pinned Original/native frontend, settings, fonts, geometry,
full-frame stability/PGACK protocol and strict grid/decoded-RGBA comparators.
Four actual states per side are live-partial, complete39, current-profile draft,
and replay39. Each side made3 requests/one real91-byte overwrite, exited generation0
naturally with code0, and reopened through painted Sessions input without a provider
or mutation replay. Native has one completed operation and its separately bounded
typed requested-content preview. The file SHA is
`9095ffad076d11e160e996e792d0ddb79063091c2df94f1765f196280ac3327e`.

**Complete39 and replay39 each strictly match the whole Original frame:**
all38400 styled cells including cursor and all5176320 pixels of the1011×5120 PNG,
zero differences. Both stages' seven complete120-cell Write rows have zero
differences; digits x6/source x8/blank gutters x5/x7 agree. Same-side full grid,
cursor and encoded PNG also agree across complete/replay; visible block cursor
is(5,314). Parent inspected both complete PNGs. This is a measured episode, not a
universal T44/V09 claim. An acquisition-driver expectation of the old8 DIFFERENT
stopped after successful capture; inspecting the retained lock confirmed4 DIFFERENT/
4 EQUAL, and validation resumed **without another product episode or metadata edit**.

All39 actual rendered SDK cases retain14256 source cells plus2144 heading cells
with zero style mismatches; the78-case/156-style corpus, current `#12ab34` stripe
and85 Original cache seals/69 first-consumed required inputs remain qualified.
No claim of consuming the16 unchanged locals is added.

Complete self-contained byte-exact `write-r3-proof.tar.xz` and its independent
`write-r3-proof.inventory.json` preserve **all1406 files/96160362 logical bytes**,
224 full opaque PNGs/788448 styled cells/104193792 pixels, including144 temporal
caret-pixel associations. Standard-library USTAR/single XZ retains original encoded
PNGs and every JSON/VT/log byte;147 direct byte-identical aliases stay within the
same fresh root/side. No pixel predictor, baseline/correction library, cross-side
alias, masking, omission, normalization or tolerant comparator is involved.

| Association/seal | SHA-256 |
| --- | --- |
| Capture code base | `4f9228191819b739b38fe55c0b2940e9d16e29f7` |
| Captured dirty code diff | `827caee14b352209fa994588218129ea401169eaa483cfc8dae035643f263a96` |
| Current705-source canonical manifest | `602ffb516da3e8e012f1835b053670443609fcd9d182f648fbc718b295e7154d` |
| Captured Native ELF | `49e43d6842097232cc80dac7c18ff8258d675e3bf3628a2b3991abec9e54bf2a` |
| Pinned Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Encoded archive3214952B | `664cf06fa2fa876b558fed2be3eb87f886e86d71c1fea9056902666d5ed169fd` |
| TAR90224640B | `94eec20ac96aad84974091bfa18fe07cce7a5529176e44ab555e08917905c173` |
| Every-member inventory | `3d807939a477c2c1988d6627ab666562e99946d239f3d688d1b476ac555984d8` |

Parent pack/reopen audits and independent all-byte/current705-source/three real
locked-build/ELF/full-frame/effect/comparator audits passed. Optional Git diff
inside the verifier's unchanged384MiB AS limit hit threaded-lstat allocation
failure; required hashes and independent read-only Git checks completed. Bounds
stay16MiB/file+encoded,128MiB/logical+TAR,2000 members,32MiB XZ decode/384MiB Python;
observed verifier VmPeak335560KiB/VmHWM289200KiB. No Cargo ran concurrently with
capture/pack/audit; a prior build-artifact identity refusal established that ordering.

## Failed attempts, storage and remaining acceptance

First Write proof and R2 proof remain complete in `write-proof.tar.xz` and
`write-r2-proof.tar.xz` with their inventories/original acquisitions. Independent
findings200 displaced cells and28 gutter-padding-fg cells were corrected by R3,
not erased/reclassified. All previous verifier scripts/reports remain available.

One own **untracked failed** Subagent002 episode was losslessly archived, not
deleted:159 files/18860871B plus full path/mode/UID/GID/mtimeNS file+directory
metadata survive in765112B `subagent-cards-002.tar.xz`, sealed by
`0a7a864fa6092505ec6879e714aecba4fbfa82641256d30ddf18710aed45d463` and
`subagent-cards-002.archived.json`. Original path `ARCHIVED.md` visibly says FAILED/
NOT_QUALIFIED. Only verified redundant loose copies were removed after durable
archive/full-byte+metadata inverse/private atomic quarantine and cap checks.
“Immutable diagnostics” preserves bytes/metadata/failure visibility, not duplicate
loose encoding; historical documents, committed proof/baselines/checkpoints and
foreign data were untouched. Own allocation before this new report was1057275904B,
headroom16465920B under unchanged1073741824B cap; all Write-prefixed evidence counts.
Fresh acquisition scratch remains preserved; final proof is durable, not offloaded.

Direct reports retain214 DIFFERENT/8 EQUAL plus4 authorized native-only entries;
matched72 pairs retain144 additional DIFFERENT. Default temporal six owners have
72 full rasters/side,3995 samples/23668 commands, zero phantoms/raster cycles.
Composer cursor remains Native(21,18)/Original(24,18), search(14,9) agrees;
blink is not qualified. Three MCP effects/one17-byte Shell effect per side remain
unchanged, with bounded UI/model output facets distinct.

Current-profile80×24 still differs61 cells in Markdown/Zig wrap boundaries/styles;
live-partial120×40 differs48 cells in Root Add/status chrome. Real elapsed time is
unmasked, even where this pair happens to agree. Next fix those actual owners,
then continue **every full frozen R1–R6/VIS01–VIS45 outcome and supplement**, including
Thoughts/family/forms/DCP/MCP/auth, followed by final current-source R6/V09. Whole
T44 remains **ACTIVE/NOT_PASS**; no task finish or acceptance/baseline reduction.
