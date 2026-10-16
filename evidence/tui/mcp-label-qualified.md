# T44 captured MCP presentation label — qualified slice

Status: qualified source/presentation/effect slice; whole frozen T44 remains
**ACTIVE / NOT_PASS**. This is not global VIS16/VIS17 pixel acceptance or final R6.

## Captured owner and preserved contracts

The running proof previously painted native `vis16__output` against Original
`vis16_output`. Native invocation names are not reversible display identifiers:
the registry also supports sanitized names, hashes and collision suffixes.
`McpGeneration::label` therefore resolves only an exact wire-name match in the
immutable MCP generation already leased for that provider request. It derives
the display text from that entry's typed server and remote-tool fields.

The canonical wire name, operation/call/item identity, provider schemas/results,
RAW input/output/journal, permission routing and actual MCP executor are unchanged.
No later registry/config lookup, heuristic wire decoding, new query/store/schema,
registry, timer, polling loop, runtime Node/Bun dependency or permission grant was
introduced. Full captured literal-secret redaction precedes control sanitization
and UTF-8 clipping to512 bytes; clipped names have an ellipsis and limited fact.
Missing/invalid/legacy facts honestly retain the canonical name.

The existing `tool_output::Presentation` carries optional typed `McpLabel` and
`label_only`. A label-only intent has no result body, body extent, guidance,
capture, producer limit, Shell/Write facts or execution authority. Argument
snapshots and durable Started/Finished events retain their captured operation
owner. Intent/refusal metadata commits atomically with the existing intent and
started-turn journal through the existing append-only presentation event.
Cold history and fork use that same bounded event owner, not a new label store.

`ToolCard.name` remains the wire identifier. Only rendered pending/argument/generic
labels use `display_name()`. Label-only records do not hide genuine RAW errors,
grant an empty known body, expand RAW beyond the512-byte card preview, or claim
successful capture. Ordinary collapse/expansion, details, classification, grouping,
controls, actual file/effect identity and draft ownership stay unchanged. A live
Finished event without any result presentation may retain the same operation's
trusted label-only fact. An explicit genuine result that omits a label is not
filled again: that omission can be a producer budget decision.

## Full-event budget and focused regression evidence

Parent source review found an optional-metadata risk, independently of the
read-only reviewers' NONE results. A real28,300-character call ID plus2048 quoted
body bytes fits the old32KiB event without a481-byte label, but exceeds it with
the label. The added assertion was actually RED with `invalid bounded tool
presentation`, before the shared guard fixed it.

`Db::bounded_mcp_presentation` includes the actual escaped operation ID in the
existing serialized event budget. Over budget, it removes only the optional
label, or omits a pure label-only event. Genuine result/body/control/capture facts
remain intact; invalid metadata and genuinely oversized old result events still
fail validation. Runtime transport and durable storage use the same guard before
intent/outcome publication. No32KiB or other resource cap was raised, and optional
names cannot turn a settled execution into started/unknown.

Four meaningful new unit scenarios cover exact hash/collision lookup and all
argument snapshots, whole-secret/control/Unicode clipping and legacy validity;
same-operation pending→linked→started→failed body fallback and explicit budget
omission; ordinary collapsed/expanded RAW-error rendering/limits; atomic storage
rollback, genuine versus unknown body, oversized intent/refusal/result metadata,
RAW equality and actual DB cold reopen/fork. The lifecycle fallback and full-event
budget assertions were actually RED before their corresponding fixes.

The existing real MCP runtime test now verifies Pending/Started/Finished labels
and unchanged wire names on three completed/failed/failed calls, retaining all
old schema, permission, secret, guidance, error, reload and no-retry assertions.
Existing actual media/control PTY tests were corrected only for the new second
intent event and painted single-underscore label; both preserve canonical calls,
private media/canary exclusions, cold no-replay and safe VT assertions. Initial
workspace failures were those two stale expectations, not hidden product success.
Their targeted corrections and the later entire workspace passed. Pending-state
frame coverage is not claimed; it has unit and actual event coverage.

## Current gates

- `cargo fmt --all -- --check`, locked workspace all-target strict Clippy and diff
  checks PASS. Complete locked workspace no-fail-fast suite: **1834 PASS /0 FAIL
  /11 unchanged opt-in ignored**, all46 targets, not a union of partial runs.
- Locked and ordinary debug builds, locked release build and both `oc --help`
  PASS. After release gates, the captured locked debug ELF was restored before
  acquisition; no Cargo ran during capture/retention/independent audit.
- Release recovery startup4:19 routes/8 GET-only requests/no Responses/restored
  terminal; actual TERM01 raw/control/VT/resize/focus/shutdown/crash/no replay;
  two VIS31 idle/high-rate controls, actual S07 and AUD32 all PASS:9 extra gates.
- TERM01 baseline/two-PTY RSS18364/21428KiB, FD24/26, threads8/10; idle300ms0 CPU
  ticks. VIS31 idle1s0 ticks,165/250Hz input/provider burst, worker lag0.
- S07 archive0→3000 RSS26136→26344KiB/HWM27152→27672/PSS23833→24120; retained24232,
  window152/cache8581, children0/lag0. AUD32 actual binary8→3000 pairs16KiB+logs,
  two active1KiB: HWM21256→22376KiB, endRSS21256→20844, endPSS18492→18134,
  DB942080→152944640, children0/eight threads/WAL0; original64MiB guard unchanged.
  These are bounded synthetic gates, not a real-user strace claim.
- Root Python54 and Node probe syntax checks PASS. Physical source map540 files,
  290811 lines/11665386 UTF-8 bytes; existing soft advisories application5460 and
  turn5307 reflect2/30 owned transport lines. The next natural seam is tool
  settlement/publication; no cosmetic counter move, minification or cap waiver.

## Actual paired acquisition and complete retention

Fresh exclusive roots `mcp-label-current-001/inventory` and
`mcp-label-regressions-001/{ordinary,temporal,matched}` preserve the existing
15s/five-identical-full-observation stability rule, owned PGACK, actual input,
fonts/geometry, full opaque PNGs, cursors, VT, clocks and strict comparators.
Nothing was masked, normalized, repainted by an observer or rewritten after capture.

Root has seven real states per side: live partial, parked Home, complete39,
current-profile draft, replay39, explicit cancel pending/cancelled. Actual busy
painted Add→Home→same Root/draft→Home, completion while parked, Home-only close,
natural generation0 exit and painted Sessions mouse reopen/no replay hold.
Each side has three original requests/one91-byte overwrite, then one explicit
cancel request. Native retains one session/two completed/cancelled turns; cancelled
public text is INCOMPLETE, without a second tool/effect. Write840 styled cells per
stage,39 source14256+heading2144 cells, absolute MD/Zig rows/styles, current RGB
`#12ab34`, hovered345 cells→fresh unhovered replay and physical shrink tail hold.
Original85 cache seals/69 required FIRST-witness consumptions remain valid;
16 locals are not claimed consumed; actual SDK corpus39 languages/78 cases/156 styles.

Ordinary/default temporal each preserve six requests/four calls/three MCP effects
and one17-byte Shell effect per side, three native resources and no replay. Native
has eight presentation events: three pure MCP intents plus five genuine results
(three MCP and two identical Shell records for the same operation), not an extra
execution. Each intent/result has typed `vis16_output`; operation/RAW names remain
`vis16__output`. Intent body/capture/guidance are absent; largest actual entire
event3362 bytes fits the unchanged32KiB cap. Model Shell257B/12 lines remains
distinct from the bounded UI stream960B/49 lines; RAW/result/effect seals hold.

The self-contained `mcp-label-proof.tar.xz` plus immutable inventory retains every
one of1442 files/100313402 logical bytes in3920112 encoded bytes:1297 regular
members/145 direct byte-identical same-root/same-side PNG aliases. It includes
230 complete opaque PNGs/817248 styled cells/108076032 pixels, all144 temporal
caret associations,72 matched pairs, and every complete strict comparator result.
The independent audit restored every original byte/membership, verified all708
current inputs in three manifests/three real locked builds/both ELF associations,
donor pin and old proof stability. Fresh scratch and every prior failure remain
preserved; final evidence is not offloaded to approved TMP.

| Immutable capture/proof association | Value |
|---|---|
| Actual capture base | `1981cd934619bea63f31e225676e72054d4b15be` |
| Actual base tree | `eded288975d26cf1dfcc042f0afdb60be8b4dbfa` |
| Captured dirty diff | `3a20ac38db920928eff0c43b1531aa581cdb4619e669c79fa520620d6b355a85` |
| All708 JS-locale canonical source seal | `a2d38fc4e5e8cf0ecff4045a51cc88b664fcfe9fe55e63c530fc9903b67286ad` |
| Native ELF | `0793908872ac75ab0abf4e2b726c1fef3148ef73b5c2af3f046fdbe6481c6e22` |
| Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Encoded archive | `c40e322701902a92001ebed11403cca5b7eac0f409c30da30a995e5da3dc53a7` |
| TAR95416320 bytes | `9e3196617a206b02538733ee11c16de133a3185bb49a83b5f5e72004207cd1ae` |
| Member inventory | `4e60a2edea3223cc357de38a1958c9aeb25b1881f6af68804c3ea9125f671690` |
| Retainer at pack | `8d61011440dc636e45ccf995d56b74249895e1b3c7ef25979062955a55612b78` |

Source associations precede delivery and are not rewritten to a later commit.
Original remains donor2670273ff17da96f85c5826ced57aa1b368754fa; licensed native
grammar/SDK provenance is untouched. Pure stdlib USTAR/single XZ retains original
encoded PNG/JSON/VT bytes; no correction library, baseline dependency or pixel
predictor. File/encoded16MiB, logical/TAR128MiB,2000 members, XZ dictionary16MiB/
decoder32MiB and Python384MiB AS remain unchanged. Observed Python peak/HWM
342300/313016KiB; source-seal dev Node inherits the AS cap, not a production host.
Exact owned allocation before these small docs1067270144/1073741824B leaves
6471680B headroom. All `mcp-label-` payload counts; no archival/deletion/quota rise.

## Remaining full contract

Current direct results:220 DIFFERENT/8 EQUAL/4 authorized NativeOnly; matched72
pairs/144 DIFFERENT. All114 direct and72 matched cursor comparisons are equal.
Default temporal3952 samples/23544 commands has0 phantoms/cycles: this is current
steady behavior, **not current-source blink qualification**. Earlier blink proofs
remain intact but final current-source R6 must requalify the required profiles.

The actual label's glyph/style matches Original, not whole card geometry: failed
header native(7,23)/Original(7,25). Ordinary completed still differs by944 cells/
63010 pixels; real clock8.1/8.2s and cancelled1.6/1.8s, Root Home7 cells/337 pixels,
security-redacted literal body, Shell command/tail/counts, body/footer layout and
remaining overlays/pacing are visible. Former32 edge pixels remain0 different.
These are not masks, supported blanket waivers or whole VIS16/31/T44 PASS.

Next fix the measured Shell/body/footer and remaining typed presentation owners
without inventing counts, suppressing loss or weakening redaction/RAW/permissions.
Continue every frozen R1–R6/VIS01–45 outcome and Thoughts/family/forms/DCP/MCP/auth
supplement, actual current-source full-frame/effect proof, then final R6/V09. Do not
stop after delivery, narrow the contract or promote label equality to task finish.
