# T44 actual paste-readiness / composer caret — qualified slice

Full frozen T44/R1–R6/VIS01–45, all supplements and final R6 remain
**ACTIVE/NOT_PASS**. This fixes an acquisition ordering error, not the native
editor, and does not promote owner-local cursor behavior to full pixel parity.

## Actual cause and change

The old temporal probe sent bracketed paste and two Left inputs in one burst.
Pinned Terminal TUI `component/prompt/index.tsx:371–391,1428–1459,1783–1787`
queues paste asynchronously and awaits attachment resolution before inserting
text. Following arrows can therefore run against the still-empty textarea.
The preserved old `(21,18)/(24,18)` observation is real but does not establish
a native caret defect or equal logical input state across those acquisitions.

The existing probe now waits for the exact authored Unicode draft and its real
painted end-caret, sends the same two Left inputs, then verifies the actual
inside-caret before and after resize. Unique ASCII prefix matching locates the
painted origin; the literal `preserved draft Ω界` is19 columns, and the offset
before the final two graphemes is16. No coordinate, cursor visibility, pixel,
clock, glyph, trace or provider result is normalized or rewritten.

The live observations are retained separately in `draft_input`, before temporal
sampling. Existing six states, hover input, search focus,72 full temporal rasters
per side,5600ms observation periods, phantom/cadence checks and full comparisons
remain unchanged. No native Rust, editor, renderer, Core API, store, config,
permission, billing, model routing, retry or resource guard changed.

## Fresh actual episode and scope

Fresh `caret-current-001/temporal`, then read-only matched comparison, observed
both sides at:

| Actual state | Native | Pinned Original |
|---|---|---|
| Painted draft origin before resize | `(5,74)` | `(5,74)` |
| Painted paste/end-caret | `(24,74)` | `(24,74)` |
| After two actual Left inputs | `(21,74)` | `(21,74)` |
| Origin after80×24 resize | `(5,18)` | `(5,18)` |
| Inside composer caret after resize/restore | `(21,18)` | `(21,18)` |
| Search caret | `(14,9)` | `(14,9)` |

Actual paste-to-Left intervals were1265/1264ms, from protocol observations, not
an invented clock. All six states preserve each current owner; zero phantom
states and zero blink cycles. **Default/steady behavior only; blink unqualified.**

Each side performed six provider requests/four tool calls: three real MCP calls
and one17-byte Shell effect. No tool replay; before/after effect hashes and cold
resources hold. Native four operations/three resources/five presentations retain
RAW/provider-result bytes and distinct model/UI output and guidance boundaries.

This is one fresh temporal acquisition, **one successful locked native build**,
707 current producer inputs and matched actual phases. It is not a new ordinary
or full Root/syntax acquisition or a claim of three builds. Native Rust and ELF
are unchanged from the delivered1830/0/11/all46, strict Clippy/fmt/builds/help,
release9/original resource qualification. Root/Home/body/family/form/auth and
final-current-source acceptance are not inferred from this focused episode.

## Complete retained proof

`caret-proof.tar.xz` and its immutable inventory contain every original byte
from temporal and matched roots:992 members/40,917,078 logical bytes,855 regular
members and137 direct byte-identical same-root/same-side PNG aliases. Encoded
archive812,072 bytes; TAR36,075,520 bytes. Original PNGs/JSON numeric lexical
forms/VT/text/traces/locks/source manifests/protocol/effects are unchanged.

The constrained caret campaign requires actual paste/origin/Left/resize facts
for both sides, exact current707 source membership and acquisition JS
`localeCompare` seal, actual successful locked build and executable associations.
Prior campaigns still require their original full scope. A regression rejects
missing facts, unperformed Left, invisible paste, invalid numeric coordinates
and the old end-caret being mislabeled as an inside-caret. Root Python53 PASS;
targeted archive/retainer6 PASS; Node syntax and Git whitespace PASS.

Independent restoration verified every992 original byte and membership, all160
opaque PNGs/307,200 styled cells, all144 temporal raster/caret associations and
all72 matched pairs. Every entire strict result reproduced unchanged: direct160
DIFFERENT; matched144 DIFFERENT. No whole-frame pixel PASS. Current Home27 cells/
1174 pixels and cursor-idle360 cells/15618 pixels remain different despite equal
carets. MCP names, Shell wrapping/tail/literal markers, footer shortening and
actual limited-preview presentation remain visible and mandatory.

| Immutable association | SHA-256 |
|---|---|
| Encoded archive | `a5feb469d1411d6a2631b1dcad51aaf79a1396acb52bd59d3e76804a1a842d42` |
| TAR | `9bf567f521567af740b359b5a3d2acbbefa185479a0bac0f4a824616d6054ee0` |
| Member inventory | `8196fc4575e439dd4abf733310373648a939b8b2a602d928e9a52ab91396ee08` |
| Current707 source JS canonical | `64079d5613cedee2901ef55462035bb97612cc48b6890f1a7c5174d7d8b29049` |
| Capture code dirty diff | `b16c46e10325fb512183a4a293350e7ab81d4ca22aa94b7f4c2652cc2359a08d` |
| Native ELF | `b32b36faa526418571a9496db1d520a20221c75315eb91cceee8ee1ce738a83c` |
| Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Retainer at packing | `a9dd5461a79bbcc9f356baf3210747bd69fdb076d64be723e40605156df60034` |

Capture base9f5b233389e536ff62c925cf8719060686a2c9ae/tree4d2734b4c301af0f37dd7f2d8c2185d557e04366;
donor2670273ff17da96f85c5826ced57aa1b368754fa. The three post-capture retention
scripts are outside attested707 producer inputs; their later reviewed dirty diff
does not rewrite the recorded acquisition association. These new factual docs
are also outside those producer inputs. Code commit/delivery is Git truth, not
the predelivery capture base or checkpoint.

Single USTAR/single XZ storage and original limits remain:16MiB per file/encoded
archive,128MiB logical/TAR,2000 members,32MiB decoder,384MiB Python AS and1GiB own
retained evidence. Audit Python VM peak131584KiB/dev-Node359220KiB; Node is a
bounded development seal check, not a production dependency. All `caret-`
payloads count toward the unchanged quota. No archival/deletion/offload/resize
of old proofs; every prior archive, inventory, acquisition and failure finding
is preserved, including the old real absolute-caret difference. Fresh original
scratch bytes remain until durable whole-byte audit; final proof is in evidence.

## Remaining contract / next owner

Continue actual Root Home placeholder/hover/version ownership, blink and pacing,
all retained card/body/footer differences, Thoughts/family/forms/DCP/MCP/auth and
EVERY frozen R1–R6/VIS01–45 outcome/supplement. Then final current-source R6/V09
with actual full styled-grid/opaque-PNG/cursor/effect and original resource gates.
Do not stop at checked delivery, reduce acceptance, normalize old observations,
or finish T44 from this logical-caret qualification.
