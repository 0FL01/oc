# T44 VIS12 — ordinary prompt mouse click-caret

## Result and scope

Functional click-caret facet qualified on current native source. Whole frozen
R1–R6/VIS01–VIS45 T44 remains ACTIVE/NOT_PASS; shared durable input history is a
separate mandatory VIS12 facet. This receipt does not claim pixel parity.

Production uses the existing editor's rendered insertion stops. `PaintedPrompt`
retains only the visible inner text rectangle/rows, chip hits, painted toast and
frame/main/Home/session/revision/generation/cursor/selection identity. Unmodified
Left Down revalidates the captured paint and current editable authority before
moving the same editor to a legal raw stop and clearing keyboard selection.
Hover/release do not move the caret. Existing chip expansion takes priority;
modal/approval/question/toast/autocomplete/terminal/child/shell owners cannot be
clicked through. Busy root draft remains editable without dispatching a request.

The shared editor layout includes previous-row end stops at wrap boundaries;
word-wrap trailing blanks stop before the soft separator, matching the running
reference's observed edit. There is no mouse-side wrapping, second editor, text
store, snapshot DTO, provider/control action or new timer. Input bytes/chips and
prepared request ownership do not change merely because the caret moves.

## Source and binary association

Final primary `prompt-caret-attempt-006`, normal `tool-preview-attempt-028` and
temporal `cursor-temporal-attempt-031` locks were checked to share:

- Base HEAD `f8db58ecb34344e46eaa59a4f4186effbd28961d`;
  tree `75bfe9b22c5e09fc6264405cf05de52c355f9566`.
- Tracked dirty diff SHA-256
  `6315afca54b6a89b41f24ac2088109fd5c70b09ddc48f84c4741b988859ea436`.
- Existing runner source-manifest SHA-256
  `85a82511c1dbbf13496216d8199c4161b557118ae524fc4d465011336097bbb6`,
  including the new unstaged Rust test and test-only probe/fixture.
- Actual `cargo build --locked` native ELF SHA-256
  `4c4e029e7a07d369680163fa8c8eb74b28ff44b45b3ab20f0fedc3330be54b01`.
- Pinned original `2670273ff17da96f85c5826ced57aa1b368754fa`, executable
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- Runner `ff0032cb09b47ac22062eaa1ca04d802145c8b983ee195f985da4f0fab643954`;
  frontend `3d5960cdba4803110b9aafb5b59dac01faf62bc08d11f21c65b4df4e9651ebb5`;
  caret probe `3932934293e7491b0feec1bf59216bd1a558caabf69d02c1a7ccdf399e91e6aa`;
  caret fixture `1ca9fd58037a7ae4f6348f5b24300bcc394067283c4dc6fdd9867e14fa11d91f`.

No Rust/capture-runtime changes followed these final runs. Delivery status is
recorded below after reviewed commit/push; captured base/diff are not rewritten.

## Actual paired click/edit/submit/replay proof

Fresh immutable primary command,900-second bounded runner:

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=2 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode \
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --tool-preview true --prompt-caret true \
  --output /home/opencode/ai/oc/evidence/tui/prompt-caret-attempt-006
```

Both sides `PASS_BEHAVIOR_ONLY`,25 paired stages and50 strict full-grid/PNG
comparisons **ALL DIFFERENT**. Runner exit1 is diagnostic inequality, not gate PASS.
Actual full styled cells, PNGs, cursor, VT, render geometry, input and protocol
logs are retained without crop/mask/version substitution or renderer state seeding.

- Home `Проведи RECON, жду план`: click between C/O then type X produces
  `Проведи RECXON, жду план`; native caret moves49→37→38 at120×40.
- Second CJK cell target32 snaps to legal start31; combining and ZWJ starts
  receive X before the grapheme. Trailing blank clamps to end; wrap-gap click
  edits after65 `a` bytes and before the soft separator. Explicit blank line,
 80-column resize, collapsed-chip neighbor and chip expansion are actual input.
- Commands backdrop owns a pointer over an old prompt cell; closing it restores
  the original draft/caret without click-through. Session padding/width gets its
  own real `SESSION RECON`→`SESSION RECXON` edit, without another submit.
- All navigation/edit stages have0 provider requests. One explicit Enter produces
 2 physical requests:1 main plus1 title. Main wire has exactly the single edited
  user string above. Tool calls, MCP tools/call and Shell effects are0.
- Native read-only SQLite audit finds exactly1 user row with those exact bytes;
  row ID/session/sequence/text and tool/resource/presentation observations remain
  identical after actual `/new`, saved-session reopen and clean same-root restart.
- Original RAW is not decoded (`native_durable_user_verified=false` there).
  Its exact real user wire and saved-session/restart UI independently prove its
  accepted prompt; no claim equates the two database schemas.

Common Unicode11 VT painting loses the laptop component of the ZWJ sample on
both sides. Paired assertions deliberately prove insertion prefixes, not full
emoji preservation from a picture; actual full frames are never altered. Native
owner tests prove complete UTF-8/grapheme edits independently of that raster.

## Regression qualification

Normal028: both27 paired stages plus2 native-only `/cards` frames qualified;
strict4 EQUAL/50 DIFFERENT plus4 `NATIVE_ONLY_RESOURCE_DETAILS`. Actual6 provider
requests,4 tool effects,3 MCP tools/call and Shell17-byte/1-line counter remain
unchanged through reopen/restart with artifacts/RAW/presentation facts. Typed body
is bounded and separate from generated guidance; literal markers remain payload.

Temporal031: both `QUALIFIED_CURSOR_BEHAVIOR_ONLY` for six composer/Search states.
Native cycles3/4/4/3/3/3; original3/4/4/3/4/3. Native parsed commands0/1499/0/0/1267/0,
original0/1638/0/0/25/0. Same-owner idle cadence/sampling bounds, draft/XY/shape
restoration,0 unsynchronized visible phantom and no replay all pass. Original
Search25 commands are not a continuous-reference-repaint claim.

```sh
python3 scripts/tui_capture/check_cursor_temporal.py \
  --compare-output evidence/tui/cursor-temporal-matched-012 \
  evidence/tui/cursor-temporal-attempt-031
```

Audit PASS:144 actual opaque full PNGs,88,473,600 fixed-owner RGBA pixels checked;
67 actual phase-matched pairs/134 strict grid+PNG comparisons ALL DIFFERENT.
Runner150 DIFFERENT/20 unmatched temporal samples; pixel parity NOT_PASS. Earlier
representative geometry/default/steady/unsupported-sync proofs remain separate.

## Native and broad gates

Current `.local/t44-prompt-caret-gates-20261008.log` completed serially with jobs3,
test threads2 and approved TMPDIR: targeted VIS12, fmt check, strict locked
workspace Clippy/all-targets `-D warnings`, workspace tests, locked and ordinary
debug builds, locked release, both helps, actual release startup/discovery probes.
Workspace **1747 passed/0 failed/11 unchanged opt-in ignores**,46 completed result
records (the additional targeted2/0 record is excluded from this sum). TUI467,
adapter677, binary98, MCP41, PTY T3952/T4234, runtime120, subagents39. Release2m42s.
Startup/refusal/retry/lock/credential/config checks restore terminals and preserve
owners; discovery errors/present/absent make the expected GET and no Responses.

Two new owner scenarios cover Home/busy session, selection clearing, full wide/
combining/ZWJ character insertion/backspace/paste/undo/delete, wrap/blank/trailing,
24-line clipped viewport, resize invalidation, modified click and newly opened
child/Shell/Terminals ownership; no queued request. Existing8 VIS07 chip/overlay/
stale-paint scenarios remain green. Node/Python syntax, repository Python47/0,
source/docs/journal checks are recorded with delivery. No baseline/ignore/security
or resource threshold is weakened; no paid/live campaign is run.

## Experiments retained and open outcomes

- Initial native render+click+type regression was RED: cursor stayed at draft end.
- Primary001's full-ZWJ raster assertion exposed common VT glyph loss, not byte
  evidence; prefix proof and native exact editor mutation tests separate them.
- Primary002 exposed wrong native wrap stop after the separator; actual original
  click/type and legal previous-row mapping led to the shared layout correction.
- Primary003's helper assumed a Home placeholder in an empty session; original
  overlay pointer accidentally chose a real New Session menu action. Positive
  empty-field acknowledgement and a genuine backdrop target fixed only the probe.
- Primary004 matched both saved-session title and top tab; real saved-row Y>0
  disambiguation fixed the probe.005 qualified behavior before latest owner-guard
  test additions;006 is final source proof. Earlier immutable bytes remain intact.

Source and JSON/doc whitespace checks exclude only exact final captures' padded
text/raw VT files, whose intentional terminal spaces/control bytes are retained.
This is not an exclusion from full cell/PNG comparisons. Shared durable newest-50
input history/effective bindings, full VIS12 and every remaining frozen T44 outcome
remain mandatory. AUTH06 deferred/original24/24 and Go13/24 ledgers unchanged.

## Delivery

Reviewed code/evidence commit and push pending at receipt creation.
