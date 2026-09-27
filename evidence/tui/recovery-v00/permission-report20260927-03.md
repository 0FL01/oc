# VIS36 — current-source paired recapture after 5d298005

**Targeted actual permission lifecycle PASS; full VIS36 remains OPEN. Cargo released.**
This report qualifies only new captures15–28 and native headless02. Captures01–14
and reports01/02 remain historical; their results do not qualify this source.
Only capture scripts and new evidence were written by this agent.

## Source and executors

- Every capture15–28 invoked `cargo build --locked` through `--build-oc true`,
  exit0, then ran `/home/opencode/ai/oc/target/debug/oc` in an isolated real PTY.
- Native binary SHA256:
  `9dd82f4a8ae001d39f7955c35a8337e4c7a2047c26775f589235ab9c1e12bee5`.
- Rust/Cargo/toolchain input digest, identical across all14 new captures:
  `057923165464f475541e9fb69909d7818bb688b0df828dd4a81ddfc55f2ee047`.
- Final23–28 full source-manifest digest:
  `606901b4b329af691786e4f10c49ad75453c9b451d68223f07312d7b120d3fcb`.
  Earlier manifests differ after harness changes; Rust inputs and binary do not.
- Actual capture HEADs:15–19 `5d298005b2948ea97938652129ebda6aabeb47ef`;
  20–25 `b0a43194a05a2bcd615d0a791a3ce57a2b212447`;
  26–28 `9ccfeefc77552f4144fa5e2606b69a21a9484c1e`.
  These are observed repository changes, not commits made by this agent.
- Read-only association audit02 confirms every canonical manifest hash, successful
  build, and zero recorded Rust-input mismatches against the final worktree at
  `9ccfeefc77552f4144fa5e2606b69a21a9484c1e`. Current binary also matches.
- Original: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
  pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`. Existing bundled U19
  patch is admitted through the established context hook. Its real executor and
  permission backend run unchanged. Captured actual schemas include
  `apply_patch(patchText)` on both sides; executor admission is recorded in locks.
- Each side independently uses its actual provider transcript, real permission
  replies and actual effects. The fixture emits ordinary function calls; it does
  not draw permission cards or replace executors. MCP is a local real stdio server.
- Locks include runner/fixture hashes, source inputs, actual binary/commands,
  input records, protocol, raw VT, full styled cells, PNGs and comparator reports.
  No masks, cursor clamping, timing normalization or invented snapshots were used.

## Commands

For each row below, substitute the NEW immutable output, width and mode:

```sh
node scripts/tui_capture/capture.mjs \
  --output evidence/tui/recovery-v00/permission20260927-NEW \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sample short --sidebar hide --columns 120 --rows 40 \
  --permission true
# Auto rows additionally use --permission-mode auto-config or auto-cli.
python3 scripts/tui_capture/permission_headless.py \
  evidence/tui/recovery-v00/permission-headless20260927-NEW \
  /home/opencode/ai/oc/target/debug/oc
```

Each capture's `commands.json` is the actual invocation/build record; runner logs
preserve exit1 from full comparator differences even when lifecycle checks PASS.

## Every new capture, including failed attempts

Counters are requests/completed/emitted ordinary function calls **per side**.
All these new requests were valid. Authenticated/live API requests: **0**.

| Capture | Columns | Mode | Original | Native | Actual result |
|---|---:|---|---|---|---|
|15|120|Prompt|26/26/14|26/26/14|Full scripted lifecycle PASS; final runner cleanup SIGTERM|
|16|80|Prompt|15/15/8|15/15/8|Both FAILED whole-text single-line predicate; complete feedback actually wrapped visibly|
|17|120|Prompt|26/26/14|26/26/14|PASS, includes New Home/return; final cleanup SIGTERM|
|18|80|Prompt|26/26/14|26/26/14|PASS, mouse replies and whole wrapped feedback; final cleanup SIGTERM|
|19|79|Prompt|26/26/14|26/26/14|PASS, same probes; final cleanup SIGTERM|
|20|121|Prompt|26/26/14|26/26/14|PASS, same probes; final cleanup SIGTERM|
|21|80|config autoaccept|7/7/3|7/7/3|PASS actual patch/shell effects and valid mixed Deny; final cleanup SIGTERM|
|22|121|CLI --auto|7/7/3|7/7/3|PASS same, config Prompt; final cleanup SIGTERM|
|23|120|Prompt|26/26/14|26/26/14|Final lifecycle PASS, all Ask holds audited, both generations natural exit0|
|24|80|Prompt|26/26/14|26/26/14|Final PASS, mouse/whole feedback/Home, natural exit0|
|25|79|Prompt|26/26/14|26/26/14|Final PASS, same holds/controls, natural exit0|
|26|121|Prompt|26/26/14|26/26/14|Final PASS, same holds/controls, natural exit0|
|27|80|config autoaccept|7/7/3|7/7/3|Final actual auto effects/Deny/zero grants PASS, natural exit0|
|28|121|CLI --auto|7/7/3|7/7/3|Final actual auto effects/Deny/zero grants PASS, natural exit0|

16 is retained, not relabelled: the original predicate demanded the whole feedback
on one contiguous terminal line. Both products wrap all words across rows36/37.
The corrected predicate reconstructs every visible editor word across rows and
checks equality with the entire input, recording expected/visible strings. It
excludes the known confirm footer only from the input-flow predicate; full captured
cells and PNGs still contain everything. It cannot pass merely on a suffix.

15–22 demonstrate functional behavior, but final natural shutdown was absent.
23–28 correct the harness shutdown and add 500ms pending→held read-only audits
for every exercised Ask. Earlier evidence was never rewritten.

Association audit01 also remains as a failed harness attempt: it erroneously
compared the pretty-printed manifest-file hash with the runner's canonical-JSON
digest. Audit02 uses the runner's canonical serialization and passes for all14.

## Final observable lifecycle,23–28

1. **Real patch Ask pauses before effect.** Before reply, seed/pending/held file
   bytes, hashes, modes and mtimes agree. No current-call `tool_operations` row
   exists; no provider request occurs between pending and held. Once really
   changes `approval.txt` (mode0640 preserved), saves zero grants and returns the
   actual result to the provider. A second patch asks again. Reject leaves bytes,
   hashes, modes and mtimes unchanged and stops the root turn.
2. **New Home while Ask.** Mouse-click actual add-tab; Home opens, `!` remains on
   the pending tab, no file/provider change occurs, clicking attention returns to
   the same Ask. Then Ctrl+F fullscreen→inline and the real Once reply execute.
3. **Always/restart.** Right selects Always. Actual patch executes; one project
   grant is stored. Ctrl+C exits generation0 with code0; real relaunch reuses the
   same data root. A new valid patch executes without another Ask and the same
   saved row survives. Original saves `edit/*`; native truthfully saves
   `apply_patch/approval.txt`, with the same isolated project's identity.
4. **Mixed Deny.** Valid preimages for both allowed and denied files are supplied.
   The actual executor returns denial, neither file changes, saved grant does not
   override Deny. Auto27/28 also return actual denial and save zero grants.
5. **Real MCP.** Ask/pending/held/inline/fullscreen; local `tools/call` count stays0
   before reply and becomes1 after Enter Once, followed by actual provider result.
6. **Actual child.** Allowed root `subagent` delegates; child's real `read` asks.
   Esc opens Reject feedback; whole text is visible at every width, Enter confirms.
   Files stay unchanged. Native's next actual child provider context contains the
   exact feedback in `permission_rejected`; child completes and parent receives
   its result. Original still sends generic `Unable to read approval.txt` and
   omits the feedback from actual child context. No context-transport parity claim.
7. **Normal tools.** Real `read`, native `bash`/original `shell`, `glob`, and
   `webfetch` each reach Ask and hold with no current-call operation row, changed
   files or additional provider request. Ctrl+F opens/closes fullscreen previews.
   Read/Shell Enter Once really execute; shell marker appears only afterward.
   Glob and URL Esc reject and stop without an execution/continuation request.
8. **URL.** Both products reach Ask for
   `https://example.invalid/vis36/never-fetch?wrapped=word-…`; native no longer
   reports pre-Ask DNS failure. This call is always rejected, never approved.
   No unsafe external network operation is requested by the campaign. The
   existing static-loopback Deny policy was not weakened. OS DNS/network syscall
   tracing is not present, so this is observable Ask/Reject evidence, not a trace
   proving absence of every possible pre-Ask syscall.
9. **Settings controls.** Actual `/settings`, `Permissions` filter, Enter toggle
   to `auto accept`, close, real read executes without Ask or new grant. Actual
   Ctrl+P→`Open settings`→Enter, filter/toggle back to `prompt`; next read asks
   again and Esc rejects. Actual `cli.json` snapshots are retained. First Esc
   after filtered toggle is captured; native clears filter and needs another Esc,
   original closes in these captures. The harness observes and records the two
   states instead of declaring them equal or blindly submitting into a dialog.
10. **Natural shutdown.** Final Ctrl+C exits generation1 with code0 in23–26;
    generation0 exits code0 in27/28. Independent validator checks both prompt
    generations and the actual provider counts.

Reply coverage: patch Once uses Enter120 and mouse79/80/121; root Reject uses
mouse80 and Esc79/120/121; Always uses Right+mouse80 and Right+Enter elsewhere.
MCP/Read/Shell Once and child feedback confirmation use Enter at all widths.
Child Reject, Glob/URL Reject and Prompt-restored Reject use Esc at all widths.
This does not imply every keyboard/mouse action permutation was exercised.

## Requests and full comparators

Final23–28: **236 requests,236 completions,124 function calls,0 invalid**.
Native headless02 adds8/8/3/0: **244/244/127/0** for the final selected evidence.
All new capture attempts15–28 including16: **554/554/292/0**;
including headless02: **562/562/295/0**. No old-source counters are included.

Each final Prompt side has two title requests plus24 lifecycle requests and14
function calls. Excluding title operations, both sides have exactly:

| Case | Requests |
|---|---:|
|once, always, restart, mixed, mcp, childroot, childread, read, shell, autoonce|2 each|
|reject, glob, url, promptagain|1 each|

Approval replies add no provider request. Full counts at every snapshot and actual
result payloads are retained in analysis08/summary07/validation05 and protocols.

| Scope | Styled grids | PNGs | Cursor pairs equal |
|---|---|---|---|
|Final23–28|200 DIFFERENT,12 INVALID,0 EQUAL|212 DIFFERENT,0 EQUAL|134/212|
|All new15–28|467 DIFFERENT,28 INVALID,0 EQUAL|495 DIFFERENT,0 EQUAL|311/495|

Final12 INVALID preserve original hidden cursor `(80,37)` in80 columns versus
native `(78,37)`: MCP/child/read/shell/glob/URL fullscreen and inline. Comparator
reports `Cursor outside grid`. PNG differences still count. No cursor is clamped
and no alternate VT-policy alignment is claimed here.

## Current precise frontend differences for parent fixes

Coordinates below are **zero-based**; use23 (120) or24 (80) scenario filenames.
Full cells/styles and rows are in analysis08 and summary07. These observations
do not propose security-policy or grant broadening for visual parity.

- **Fixed and observed:**23 Once Ask title `(7,25)`, triangle `(5,25)`, Edit
  `(7,26)`, options `(6,37)/(19,37)/(34,37)` match. Fullscreen title `(7,2)` and
  Edit `(7,3)` match. Border is `┃`; attention `(1,0)` matches purple bold
  `fg#9d7cd8/bg#2c2933`; Edit arrow matches muted `#808080/#141414`. Home/return
  and real whole feedback no longer lose the beginning of the draft.
- **Pending filename missing:**23/24 `once-ask`, both `Patching` at `(7,7)`;
  original has `approval.txt` at `(16,7)`, native omits it. Actual buffered patch
  title should remain associated with the prepared call.
- **Footer 2-cell difference:**23 Once Ask native `ctrl+f fullscreen` `(71,37)`
  and `enter confirm` `(100,37)`; original `(73,37)` and `(102,37)`.
  Fullscreen minimize starts73 native versus75 original, same row.
- **Always exact-scope presentation:**23/24 Right-selected Always keeps correct
  title but native Permission title/Edit `(7,29)/(7,30)` versus original
  `(7,31)/(7,32)`. Native truthful pattern explanation starts `(5,32)` and
  `- approval.txt` `(5,34)`; original explanation `(5,34)` describes edit-wide
  scope. Real native grant remains exact; adding the path explains some geometry
  difference and is not permission to change that grant into `*`.
- **Patch rejection rhythm/path:**23 `reject-completed`, original title
  `# Patch failed approval.txt` at `(5,24)` then blank row25 and text `(5,26)`.
  Native title has no path at `(5,24)` and text immediately `(5,25)`, shifting
  following metadata up one row. Raw permission JSON is now gone as intended.
- **80-column feedback footer:**24 `child-feedback-draft`, both display all
  text beginning `(5,36)` and `this correction.` `(5,37)`. Original confirm
  footer starts `(50,37)`; native `(50,36)`. This is a footer placement mismatch,
  not lost feedback. At120 both footer/text positions match.
- **Filtered Settings grouping:**24 `settings-autoaccept`, original Session
  header `(14,15)`, Permissions/value `(14,16)/(55,16)`. Native combines
  Permissions `(14,15)`, Session `(45,15)`, value `(55,15)` on one row.
  At120 corresponding x coordinates are34,65,75. First Esc after toggle also
  differs as recorded above; actual toggle and palette functionality pass.
- **80-column URL title truncation/wrap:**24 `url-ask`, original Permission title
  `(7,27)` and WebFetch title begins `(7,28)`, continuing on rows29/30. Native
  title `(7,28)`, WebFetch `(7,29)` ends after `word-wo`, without those title
  continuation rows. Original URL label/value share row32; native label row31,
  value starts row32. Native pending WebFetch uses different symbol/indent/wrap
  on rows23–26. Fullscreen/inline actual lifecycle still works.
- **Other transcript differences remain:** native pending Read path is one row
  lower; native MCP argument rendering and delegating card differ from original;
  original shell empty-output metadata differs; native rejected Glob card shows
  `Glob ""` while original retains Explored/search grouping. Elapsed digits and
  these transcript differences are retained in full comparison, not masked.

## Native headless02

Same binary SHA as all new TUI captures. Actual JSON runner and local provider:

| Case | Exit | Effect | Grants | Requests/completed/calls |
|---|---:|---|---:|---|
|TUI config autoaccept, no headless consumer|1|none|0|2/2/1|
|CLI --auto with Ask|0|real shell marker|0|3/3/1|
|CLI --auto with Deny|0|none, actual denial|0|3/3/1|

These are native-only; original headless behavior has not been paired.

## Evidence and unqualified matrix

- Final detail: `permission-analysis20260927-08.json`; all new attempts:
  `permission-analysis20260927-07.json`.
- Final compact source/counters/context/grants/cell evidence:
  `permission-summary20260927-07.json` and runner log. Earlier summary05/06
  remain;07 corrects absent-stage grant counts from0 to null.
- Independent final validator: `permission-validation20260927-05.json`, exit0.
  It checks every pending→held file/request boundary, no current-call operation
  row, grants/restart, valid actual Deny, MCP counts, entire visible feedback,
  native child context, exact lifecycle request counts, natural prompt exits,
  native headless and identical source/binary association.04 remains preserved.
- Source audit: `permission-association20260927-02.json`, exit0; failed01 retained.
- Node syntax, Python compile, actual xterm frontend integrity check and
  `git diff --check` PASS. `permission-frontend20260927-02.runner.log` records
  adapter integrity; it is not product parity evidence.

Still **unqualified**, not Unsupported waivers:

1. Whole styled-grid/PNG equality, including the concrete differences above and
   original cursor overflow under a genuinely agreed VT policy.
2. Complete action×width×input-mode matrix; Always/Reject for each normal tool,
   every mouse/fullscreen/keyboard route, feedback cancel/edit/focus, wheel/resize,
   additional patch/URL/pattern/MCP preview types and multi-call combinations.
3. Typed CoreAPI/CoreEvent and owner-prepared preview digest observation are
   parent-owned tests, not qualified by a SQLite absence proxy. The capture proves
   no current-call durable operation row/effects while Ask; it does not directly
   observe the transient `CurrentIntent` API/event at every moment.
4. OS file-read/DNS/network syscall audit before reply. Stats/hashes/bytes and
   `SQLite mode=ro` are an independent read-only evidence audit, not a syscall trace.
5. Atomic multi-resource grant-write failure/rollback, new project/worktree scope,
   >1024-grant persistence, fd-pinned cwd attacks, subagent/compress preflight
   failure variants: not injected in this capture campaign. Parent backend tests
   are separate evidence and were not rerun by this agent.
6. Original headless pairing, full config-auto/CLI-auto cross-width matrix,
   release binary and mandatory acceptance/workspace Rust gates.

No Rust, acceptance, `.opencode`, credentials or progress files were edited, and
this agent made no commit. Cargo ownership was held only for the capture builds
and released immediately after final28.
