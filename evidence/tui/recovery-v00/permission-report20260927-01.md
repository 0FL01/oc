# VIS36 — real permission lifecycle campaign, diagnostic only

**Full VIS36 remains OPEN.** Actual targeted admission/effects checks pass, but
whole-frame parity fails and the 80-column feedback editor has a reproducible
defect. No Unsupported waiver, mask, crop, fabricated duration, transcript import,
or fake tool result is used. All attempts01–12 and failed validator evidence remain.

## Source/build association and envelope

- Worktree: `/home/opencode/ai/oc`; active T44; HEAD
  `72fc7b62270b948de031e443ba3eff61ba41961a` plus the owner's dirty approval implementation.
- Native SHA-256 in every capture and headless probe:
  `979e9aceebe3804b86a4bd485245a81f56dbccce22b42ba05d94edb5555b986f`.
- All twelve capture manifests have the same Rust-input SHA-256:
  `b4be80a060b770cd0e21ff9014e4dbff46fe92e186835aa9529d98de0567e055`.
  Attempts01–11 execute and record `cargo build --locked` before capturing. Attempt12
  uses that same binary and same Rust inputs with `--build-oc false`, after Cargo
  was released. Its lock truthfully records an existing binary, not a new build.
- Original: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`;
  pinned v2.0.12 `2670273ff17da96f85c5826ced57aa1b368754fa`, executable hash checked
  by the runner. The existing fixture-only context hook admits bundled U19 `patch`;
  no executor or permission backend is replaced. Both sides receive ordinary
  function schemas: original `patch(patchText)` and native `apply_patch(patchText)`.
- Independent isolated homes/data and common isolated project path; env-cleared
  PTY; 40 rows, widths79/80/120/121; real xterm styled cells and Chromium PNGs.
  Files are audited through bytes/hash/mode/mtime; SQLite opens `mode=ro`.
- Provider and MCP are local fixtures. **Zero authenticated/live API requests**.
  Attempt03 used `example.invalid` and observed DNS preparation failure; no external
  HTTP fetch was authorized. Subsequent URL probes use loopback port9 and are
  rejected before any fetch. No key/env file was read. No Rust, acceptance,
  `.opencode`, progress, or existing evidence artifact was edited; no commit.

## Reproduction commands

Representative source-built 120-column run (use a NEW output directory):

```sh
node scripts/tui_capture/capture.mjs \
  --output evidence/tui/recovery-v00/permission20260927-NEW \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --geometry true --sidebar hide --sample short --columns 120 --rows 40 \
  --permission true
```

For config/CLI auto modes, append `--permission-mode auto-config` or `auto-cli`.
Exact historical invocations/build exits are in each `commands.json` and
`bridge-spec.json`; terminal input bytes are in `inputs.json`. The individual
capture `.cells.json`, `.png`, `.render.json`, `.vt`, full comparator reports,
protocol, checks, manifests and locks remain beside them.

Executed analysis/checks:

```sh
python3 scripts/tui_capture/permission_headless.py \
  evidence/tui/recovery-v00/permission-headless20260927-01 \
  /home/opencode/ai/oc/target/debug/oc
node scripts/tui_capture/analyze_permission.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-analysis20260927-04.json
node scripts/tui_capture/summarize_permission.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-analysis20260927-04.json \
  evidence/tui/recovery-v00/permission-summary20260927-03.json
node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-validation20260927-02.json
node --check scripts/tui_capture/capture.mjs
node --check scripts/tui_capture/permission.mjs
node scripts/tui_capture/check_frontend.mjs
git diff --check
```

Headless, independent targeted validator, syntax, frontend integrity and diff checks
exit0. Capture exit1 means full visual comparisons differ; it does not mean every
individual behavioral check failed. Attempt05 exits2 after the external240s
watchdog; its emitted checks/frames/protocol and runner log remain.

## Actual behaviors and counts

`permission-summary20260927-03.json` contains per-side/per-case counts, counts at
every read-only snapshot boundary, saved grant rows, native operation rows,
actual provider tool outputs, cursor comparisons and zero-based token positions.
`permission-analysis20260927-04.json` retains fuller schemas, file audits, cell
samples, rows and comparator results. The independent validator rechecks attempts
08/09, both auto modes, and all native headless cases without executing tools.

| Attempt | Width/mode | Original requests/completed/calls | Native requests/completed/calls | Scoped behavioral result |
|---|---|---|---|---|
|01|120 prompt|4/4/2|4/4/2|FAILED probe incorrectly awaited continuation after Reject|
|02|120 prompt|11/11/5|11/11/5|Core Once/re-Ask/Reject/Always/restart/mixed-Deny PASS|
|03|120 prompt|17/17/9|18/18/9|Original PASS; native URL Ask absent after DNS failure|
|04|79 prompt|17/17/9|21/21/11|Original Settings predicate failed; native scoped PASS|
|05|80 prompt|19/19/10|21/21/11|Original filtered Settings Esc misunderstood; native interrupted by watchdog|
|06|121 prompt|20/20/11|21/21/11|Scoped PASS both; native URL Ask remains blocked|
|07|80 prompt|26/26/14|12/11/5|Original scoped PASS; wrong native MCP wire name caused one invalid request|
|08|120 prompt|26/26/14|27/27/14|Expanded scoped PASS; native URL gap retained|
|09|79 prompt|26/26/14|27/27/14|Expanded scoped PASS; native URL gap retained|
|10|80 config auto|7/7/3|7/7/3|Auto patch/shell effects; zero grants; mixed Deny prevents effects|
|11|121 CLI auto|7/7/3|7/7/3|Same, CLI `--auto` overrides configured Prompt|
|12|80 prompt, mouse|26/26/14|15/15/8|Original expanded PASS; native fails feedback-draft visibility|

Totals including native headless: **405 provider requests,404 completions,207 emitted
function calls,1 rejected invalid fixture request**. Every request is local/offline.
The one invalid request is historical07, native MCP called `vis36_echo` whereas
its actual advertised schema is `vis36__echo`. Later fixtures use actual wire names.
Full08/09 have2 title requests per side. Non-title lifecycle counts are exactly2
for successful request→result continuation;1 for root Reject/glob Reject/Prompt
rejection. Child root and child read each have2. URL differs: original Reject has1;
native structural error returns a tool result and continues,2. This is an observed
policy/preparation difference, not an extra approval-provider request.

### Effects, saved grants and feedback

- Ask: seed and two held snapshots prove unchanged file bytes/hash/mode/mtime;
  no provider request occurs between primary pending/held snapshots. There is
  no current native durable tool-operation row while its Ask is pending. This
  is a durable operation-boundary audit, not a direct capture of typed CoreEvent
  emissions or an OS read-syscall trace.
- Enter Once at120 and mouse Once at narrow widths execute the real patch.
  No saved row exists before Always. The next patch asks again; Esc or mouse
  Reject interrupts the real turn without changing bytes or mtime, and emits
  no provider continuation. No synthetic completion text is required for Reject.
- Right→Always→Enter, and mouse Always in12, execute and save one project grant.
  Rows survive clean process restart; a fresh session patches without another Ask.
  Native row: project identity, action `apply_patch`, pattern `approval.txt`.
  Original row: same project identity, action `edit`, resource `*`. Do not claim
  equal policy algebra or grant breadth. Mixed approved+denied file patch changes
  neither file on either side, including auto modes.
- Read/shell: actual calls pause; Once produces real read output or creates
  `shell-marker`; no phantom file effects. Glob Reject interrupts. Full inline
  and Ctrl+F fullscreen/minimize frames are preserved for each observed Ask.
- Local stdio MCP: real discovery; `tools/call` count0 while Ask and1 after Once;
  actual echo output appears in the next provider request. No fake MCP result.
- Root UI receives a real child's read request while the foreground subagent
  is running. Reject opens the feedback editor; submitted native feedback is
  present in the child's actual `function_call_output` and provider context.
  **Original feedback is NOT present in the next child provider context**:
  it receives `Unable to read approval.txt`. Both continue child→parent, but this
  does not qualify equal feedback transport. Validator records the difference.
- `/settings`, filtered Permissions, Enter toggles persisted config and changes
  actual read admission. Autoaccept executes without new grants. Restoring Prompt
  re-asks. Original filtered Esc first clears the filter, then another Esc dismisses;
  native first Esc dismisses. Attempt12 also exercises original Ctrl+P→Open settings.
- Native headless: TUI `session.permissions=autoaccept` alone exits1 on Ask,
  no operation/effect/grant; `run --auto` exits0 and touches the marker, no grant;
  Deny plus `--auto` exits0 with denied result and no marker/grant. Counts are
  respectively2/2/1,3/3/1,3/3/1, including actual title requests.

## Full comparators — no exact PASS

Across sealed attempts: styled grids **230 DIFFERENT,4 INVALID,98 BLOCKED**;
PNGs **234 DIFFERENT,98 BLOCKED**; **zero EQUAL**. Independently observed cursor
equality:145/264 paired captures. No cursor masking/clamping or time-digit removal.
Attempt05 has captured pairs but watchdog prevented normal comparator sealing,
so cursor-pair and comparator counts need not have the same denominator.

The four INVALID grids are12's MCP/child fullscreen+inline: the original xterm
reports hidden cursor `(80,37)` on an80-column grid, native `(78,37)`. Comparator
rejects `Cursor outside grid`. Preserve this actual right-margin observation;
it is a comparator/VT-contract blocker, not native cursor equality or a reason to
clamp the reference. PNGs remain compared and DIFFERENT.

## Precise defects for product owner

Coordinates below are **zero-based `(x,y)`**, from actual cells, not JS offsets.
Paths are relative to the matching attempt's `upstream/` or `oc/` directories.

1. **120 inline geometry/chrome** —08 `permission-once-ask`: original title
   starts `(7,25)`, warning triangle `(5,25)`, edit arrow `(7,26)`; native title
   `(6,29)`, triangle `(4,29)`, arrow `(4,31)`. Native lower border is `│` atx2
   whereas original is `┃`; original options start `(6,37)`, native `(5,37)`.
   Native omits the original blank/body allocation and moves header/title.
   Native edit arrow fg`#eeeeee` vs original`#808080` (both bg`#141414`).
   At79 in09, native title is row27 vs original25; options row36 vs original35.
2. **Fullscreen header** —08 Ctrl+F: original triangle `(5,2)` with edit title
   on row3, native triangle `(4,2)` with edit title on row4; original title arrow
   has muted color, native base text color. Persistent narrow `│` divergence.
3. **Pending tool and tab attention** —08 first Ask: original `⋯ Patching`
   appears in a bordered card on row7 (`Patching` `(7,7)`); native generic
   `⋯ apply_patch resource: approval.txt` starts row6 with no matching card.
   Original tab attention `!` at `(1,0)`, fg`#9d7cd8`, bg`#2c2933`, bold;
   native `△` at `(1,0)`, fg`#f5a742`, bg`#1e1e1e`, bold, and original `+`
   remains visible while native suppresses it.
4. **Always selection content** —08 Right→Always: native row34 prints
   `This will always allow the following patterns for this project.` and row35
   `- approval.txt`; original row34 prints `This will always allow edit for this
   project.` and retains `→ Edit approval.txt` above. Native omits that title.
   Broader original grant versus native exact resource must stay truthful.
5. **Reject result** —08 root Reject: native exposes raw
   `{"status":"permission_rejected","feedback":null}` under `# Patch failed`;
   original human text is `The user declined this tool call` with file-bearing
   failure title. Raw native JSON is also visible in rejected glob cards.
6. **Feedback layout** —08 at120: native `Tell OpenCode` startsx4 on row34,
   originalx5; native feedback beginsx4 on row37, originalx5. **At80,12**:
   path real child Ask→Esc→paste
   `VIS36-FEEDBACK: do not read; continue with this correction.` leaves only
   `is correction.` at `(4,37)` in native `permission-failure`, with inline hints
   starting aroundx48. Original full feedback wraps in its editor and capture
   succeeds. Native visibility predicate fails; the probe stops rather than
   substituting a suffix predicate or rewriting the product.
7. **Settings** — native menu has only Permissions and presents `Autoaccept`/
   `Prompt`, whereas original has the actual full Settings inventory and
   `auto accept`/`prompt`, Session category and `←/→ change` footer. Filtered
   Esc behavior differs as described above. Both control real admission;
   full UI/focus parity remains open.
8. **URL preparation** — native `private host refused` arrives before Ask for
   offline loopback. Original can show URL Ask and then Reject. This campaign
   cannot establish native URL preview parity without an admissible bounded
   destination; preserve structural trust boundaries, do not enable loopback
   in production for screenshots.

## Honest remaining matrix

| Matrix item | Evidence/status |
|---|---|
|Actual patch pre-effect Ask, Once, re-Ask, Reject, Always, restart, Deny|Targeted PASS; full visuals DIFFERENT|
|Keyboard Once/Always/Reject and mouse equivalents|Actual effects observed; mouse path12 completes these before later feedback failure|
|Inline/fullscreen wrapping79/80/120/121|Actual patch frames present at every width; exact parity fails|
|Read/shell/glob/MCP previews|Observed full paired states at79/120; full action×width matrix still open|
|URL preview|Original observed; native structural preparation blocks Ask; OPEN, not Unsupported|
|Child routing and feedback|Native feedback in actual context; original loses it;80 native editor defect; OPEN parity|
|Settings Prompt/Autoaccept and config persistence|Actual pair effects at79/120/121; exact UI/focus parity fails|
|Palette Settings|Original actual path12 PASS; native not reached after80 feedback defect; OPEN|
|Config auto/CLI auto and Deny|Actual paired80/121 effects, no saved grants, targeted PASS|
|Native headless consumer/auto/Deny|Actual executable PASS; original headless pairing OPEN|
|Prepared owner preview digest, typed event ordering, syscall read-only audit|File-stat/hash/read-only DB evidence only; direct transient digest/CoreEvent/syscall probe OPEN|
|Atomic multi-resource grants/failure rollback; clone/worktree/project boundaries|Single saved row+restart observed; full unit/transaction boundary suite not run here; OPEN|
|Feedback editing/cancel/focus, alternate shortcuts, mouse fullscreen/selection, wheel overflow, other tool previews|Not comprehensively exercised; OPEN|
|Release entry and acceptance/unit/workspace gates|Not run by this evidence-only agent; parent owns them; OPEN|

Follow-up: fix actual native approval geometry/card/rejection/Settings/80-column
feedback defects, then recapture into new directories. Qualify the missing owner
and atomic-safety probes with meaningful targeted tests; do not infer those from
the screenshots or waive them. Cargo ownership has been released.
