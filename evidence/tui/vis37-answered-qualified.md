# VIS37 answered questions — current-source qualified slice, 2026-10-12

## Result and scope

**ANSWER_CONSUMER_CARD_GLYPHS_ONLY; VIS37/R6/T44 remain NOT PASS.**
The existing typed question owner, real native answer consumer and completed card
are qualified here. Pending-form geometry, blank-cell styling and full-frame
residuals are not waived. This does not finish T44 or reduce its frozen scope.

`tool_block` renders result-derived `# Questions`, ordered muted question rows,
base-colored answers, `(no answer)` for an empty answer, block/pair spacing and
safe Unicode wrapping. Both full and cached transcripts now reach that renderer
instead of Generic expansion; answered question cards have no Generic hit target.
Two RED→GREEN tests cover roles, wrapping/control safety, immutable typed/RAW data
and complete painted-buffer equality across full/cached/expansion paths.

Three new actual native PTY/headless cases exercise a provider-issued tool call:
`--auto` still waits for a bound question reply; real multiple/custom answers reach
the next provider request and survive restart; dismiss persists cancellation, not
an answer or continuation; headless `--auto` reports nonretryable `question_required`.
They preserve the live composer draft, terminal restoration and durable RAW IDs.
The existing adapter `question_lifecycle` binding/durable-before-wire test is also
retained; the earlier scout's claim that no backend E2E existed was incomplete.
General/Explore Deny and parent policy ceilings remain unchanged and tested.

## Actual paired protocol

The existing capture harness adds one closed `--question-forms true` mode, not a
new runtime/store/framework. It requires one real locked repository build, fresh
output-path-hashed isolated roots and exactly these three profiles:

| Profile | Stages per side | Whole PNGs | Strict comparisons | Equal cursor pairs |
|---|---:|---:|---|---:|
| 80×24 | 17 | 34 | all 34 DIFFERENT | 10/17 |
| 120×40 | 17 | 34 | all 34 DIFFERENT | 10/17 |
| 160×48 | 17 | 34 | all 34 DIFFERENT | 10/17 |

All six side checks are `PASS_BEHAVIOR_ONLY`. Real key events select Rust/Python,
enter `custom Ω界`, review and explicitly submit; provider-visible answers are
`[["Rust", "Python"], ["custom Ω界"]]`. Native typed JSON and Original's canonical
formatter output are checked separately, with exact call IDs and question order.
Reopen/restart source records retain their IDs, metadata and RAW association.
The second real question is dismissed; reopen retains cancellation and never
reasks or submits a successful answer. Editing/review generates no request.

Current traces contain **24 valid requests = 18 main + 6 title**, 12 question calls,
12 explicit user submissions, six explicit answer submissions and 30 read-only
snapshots. No observed extra project/Shell/MCP effects, reask or cancel continuation.
Original source is narrowly decoded from actual V2 `session_message.data.content[]`,
not guessed V1 tables. Native reads typed operations/messages. DCP table contents
are not decoded; no claim of complete DCP equivalence is made.
Generation 0 exits naturally with code 0 on every side; final generation 1 teardown
is **FORCED_STOP**, not evidence of a clean final lifecycle.

Failed first `001/wide` is preserved: both sides FAILED, 25 whole PNGs, 20 DIFFERENT
comparisons and ten BLOCKED attempts; seven valid partial requests are separate
from current counts. Diagnosis was observer-only: wrong donor V1 projection and
a dismissal predicate rejecting the honest native `interrupted` footer. Corrected
V2 projection/predicate were tested; only fresh `002` episodes were acquired.
No incomplete episode replay, fabricated log, product fault claim or old rewrite.

## Full frames and remaining owners

Independent recount checks all 102 current opaque PNGs, **244,800 styled cells /
33,012,096 RGBA pixels**. Whole-frame residuals are **46,999 cells / 3,493,728 pixels**;
30/51 complete cursor pairs agree. Clocks, footer, blank cells and all failed
comparisons remain present. No crop, mask, tolerance, golden or normalization.

Answered cards match text/order/spacing/wrapping and every nonblank glyph style in
all profiles. **57 owner blank-cell foreground differences remain**: 21 small,
18 wide, 18 tall. Full answered frames differ by 21 cells/474 pixels, 19/445 and
20/384 respectively; wide/tall include one/two real clock glyphs outside the card.
All answered-card cursors match. This glyph-owner check does not promote any
whole-frame DIFFERENT result to PASS.

Next concrete owner is `crates/oc-tui/src/question_view.rs`: its unbordered/lower
pending title, full header tabs and inline descriptions differ from pinned
`FormPrompt`'s bordered/padded panel, 24-character tabs and separate descriptions.
The previously refuted generic caret-FG paint must not be reintroduced.

## Immutable capture association and byte-exact retention

Capture base `c5adde0f7c8bab926592fb885a98c47c09da74e3`, tree
`80612f0e1232e711b9acab2c355a43fe5ccd3976`, captured dirty diff
`983de2eb0b749475724ed01f83c0984fdb2bffb29dc030a4d8b9cc96a5506e6c`.
All **713 producer paths**, every current source byte, both real executable ELFs,
runner/fixture seals and exactly one recorded successful `cargo build --locked`
per acquisition were independently verified. Later quality gates are not inserted
into acquisition commands. Separate current source bytes are not archive members.

| Object | SHA-256 |
|---|---|
| Current canonical 713-source manifest | `12b18e6a11a4dfb89430a87e03a15de2bb72b100bd5fb45d5fb1cc7e7da80448` |
| Native ELF | `e0ae196099c70256284e9e02945c05eb66080ff938c7223c3c79895b4644cd27` |
| Pinned Original ELF | `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a` |
| Donor commit | `2670273ff17da96f85c5826ced57aa1b368754fa` |
| Retainer source | `48c24b704b8acbdaf84742021b799f896f44a0befe5f7618eac2eac02bce69a7` |
| `vis37-proof.tar.xz` | `630c078fcf8952952e23366f22d2b47b3e4aba18e3365cf4198d945f854d3651` |
| Decoded TAR | `8a10b49fca9cdcb8331c97c2fd243ea33a0214a7d84716c4502b3cfccaabb128` |
| Member inventory | `d5081f6e9f89299495b7155c0d36ff5631a17e1415ef97bf0934451929d37a2c` |
| `vis37-proof.inventory.json` | `6b0ad94dffc4cc659a2713aa1f86d4a9166a214af89e4a095fd33e0c9ca1e3bd` |

Archive **2,404,048 bytes**, decoded TAR **64,389,120 bytes**. All **829 original
files / 64,853,986 logical bytes / 127 whole PNGs** are retained, including failed
capture, both driver programs, two direct owner files and four direct driver logs.
The 31 aliases are byte-identical direct PNG references within the same run/side.
Independent pre-pack inventory and manual single-XZ/canonical-USTAR inverse restore
every original byte; 14 source-directory metadata records and prior specifically
inventoried preservation sets/initial verifier/report remain unchanged. This is
not a new blanket audit claim for all historical 11k artifacts.

Independent inverse helper/report under the approved TMP root
`vis37-independent-20261012-sole-verifier` have seals
`28e94d3c2d9fdc28c7054919ad57263a3fd2e177a8aff01a88b3584aa9dcad1a` /
`4915bb2ca6cd14971d8bfe201e751a15c6a90889e9c09c0eafb203e38cc457e1`.
Actual process VmPeak **211,349,504 bytes**, VmHWM **180,129,792 bytes**. The first
report's exec-inherited `ru_maxrss` is not measured verifier peak. Same hard bounds:
384 MiB AS, 32 MiB decode, 16 MiB dictionary/file/encoded, 128 MiB TAR/logical,
2,000 members, owned evidence 1 GiB. Before this receipt, owned allocation is
**1,068,965,888 / 1,073,741,824 bytes**, headroom **4,775,936**; independent TMP
scratch allocation 1,544,192 bytes is separately reported. No extraction, old
artifact deletion/offload, metadata rewrite, cap increase or live paid campaign.

## Actually executed current-source gates

- `CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 cargo test --workspace --locked`: **1849
  PASS / 0 failed / 11 unchanged opt-in ignored / all 46 targets**, TUI519/PTY58.
- Strict all-target workspace Clippy `-D warnings`, workspace fmt, locked and
  ordinary debug builds, locked release build and both native `--help`: PASS.
- Current release nine: startup/recovery/terminal restoration, TERM01 actual PTY,
  VIS31 idle/pacing, S07 equal-view archive and AUD32 actual binary memory: PASS.
  No threshold/deadline increase; locked debug rebuild restores the captured ELF.
- Root Python **62 PASS**, nested question fixture **2 PASS**, JS syntax, docs58
  tasks/173 specs, progress structure and diff whitespace: PASS.
- Retainer measure/pack/reopen `FULL_BYTE_EXACT`; independent full inverse, source,
  ELFs, recorded builds, typed wire/source/effects and full-frame recount: PASS.
- Warning-only size inventory: 545 files/295069 physical lines/11890333 UTF-8 bytes.
  Existing application5460/turn5307 warnings unchanged; natural settlement/publication
  seams remain, no cosmetic file split or count reduction.

Next: fix the real pending-form owner, preserve residuals, close **EVERY** remaining
frozen R1–R6/VIS01–45 and supplement, then final current-source R6/V09 before finish.
