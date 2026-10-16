# VIS36 report 4 — fresh current-source campaign 29–36

## Verdict and scope

**Targeted actual permission lifecycles PASS; full VIS36 visual/action matrix remains OPEN.**

This report qualifies only new captures **29–34 and 36**, native headless **03**, pinned-original headless **03**, and their independent audits. Captures 01–28 and reports 1–3 are historical and do not qualify this source. Attempt 35 failed before launch and remains preserved as a runner log. No old artifact was rewritten.

All four prompt widths (79/80/120/121 × 40), config-auto at 80, CLI-auto at 121, and the supplementary traced 120 run use real provider function-call schemas, real tool execution, real permission replies and real storage. Original uses its existing U19 `patch(patchText)` executor admitted by the fixture hook; native uses actual `apply_patch(patchText)`. No fabricated permission card, executor result, elapsed duration or screenshot. Providers and MCP are bounded local fixtures; authenticated/live requests: **0**.

The owner reported latest backend full gate `tool_0e2f008ca001` with zero FAIL and release approval entry PASS. Those are **parent-owned backend/typed CoreEvent/release gates**, separate from this paired debug PTY campaign; they were not rerun here or inferred from an empty SQLite operation table.

## Actual production source association

- Captured HEAD: `d77faa6a38abf2306e50981ad9df39c7b91aa26d` plus owner dirty Rust sources.
- Every successful capture ran `cargo build --locked`, exit 0: **7 actual builds**.
- Actual native debug executable SHA-256: `42977ff968750c37e120a892c948514c8956c6eb422a6e0ef07b0a3c857abb6f`.
- Rust/Cargo/toolchain input digest: `f512146d5f8761598990013333d1e0ef0f138e068483c0ab75e77888889f9f60`.
- Captures 29–34 canonical complete source manifest digest: `2f8ecd1db35c80b4370174af7d13ae59cf452a4db7f939ec6d2ae07e2ce5fe3c`.
- Capture 36 manifest digest: `3403c25ce53f8a473c3bb60ac3fe6825c35bd2950ab7aec3877e13ee7584c7bf`. Instrumentation scripts changed; production digest and executable did not.
- Original SHA-256: `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`, pinned donor `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Read-only association audits `permission-association20260927-03.json` and `-04.json`: canonical manifests valid, every recorded build exit 0, **no current Rust input mismatch**, current executable matches all seven captures. Actual docs HEAD is recorded, not substituted with the prior 9ccfeefc checkpoint.

Cargo was released after capture 36; the agent used Cargo only for capture builds. Changes are scripts and new immutable evidence only. No Rust/acceptance/progress/.opencode/credential edits, commits or pushes.

## Per-capture wire counts and exits

Counts below are **per side** (original and native independently have the same counts). Title requests are included; individual schemas, call IDs, function outputs and request contexts are preserved in each side's `protocol.json`.

| Capture | Width/mode | Original/native targeted status | Requests / completed / emitted tool calls / invalid | Natural process exits |
|---|---|---|---|---|
| 29 | 120 prompt | PASS / PASS | 26 / 26 / 14 / 0 | generations 0 and 1, both code 0 |
| 30 | 80 prompt; Once/Reject/Always mouse | PASS / PASS | 26 / 26 / 14 / 0 | generations 0 and 1, both code 0 |
| 31 | 79 prompt; Once mouse | PASS / PASS | 26 / 26 / 14 / 0 | generations 0 and 1, both code 0 |
| 32 | 121 prompt; Once mouse | PASS / PASS | 26 / 26 / 14 / 0 | generations 0 and 1, both code 0 |
| 33 | 80 config-auto | PASS / PASS | 7 / 7 / 3 / 0 | generation 0, code 0 |
| 34 | 121 CLI-auto | PASS / PASS | 7 / 7 / 3 / 0 | generation 0, code 0 |
| 35 | proposed traced 120 | HARNESS PRELAUNCH FAILURE | 0 / 0 / 0 / 0 | no process/build launched |
| 36 | 120 prompt, native scoped strace | PASS / PASS | 26 / 26 / 14 / 0 | generations 0 and 1, both code 0 |

Each prompt side has 2 title requests and 24 ordinary lifecycle requests. Cases once/always/restart/mixed/MCP/childroot/childread/read/shell/autoonce have two requests each (call then result/continuation); reject/glob/URL/promptagain have one each because root rejection interrupts. 14 ordinary calls are actually emitted. No approval-only extra provider request is present. Auto modes have 7 requests / 3 ordinary calls, including the valid mixed preimage and an actual denied result.

Totals:

| Scope | Requests | Completed | Emitted calls | Invalid requests |
|---|---:|---:|---:|---:|
| Main 29–34, both sides | 236 | 236 | 124 | 0 |
| Supplementary 36, both sides | 52 | 52 | 28 | 0 |
| All successful new TUI captures | **288** | **288** | **152** | **0** |
| Qualified native headless03 + original headless03 | 16 | 16 | 6 | 0 |
| Qualified campaign including paired headless | **304** | **304** | **158** | **0** |
| All new attempts, also original headless01/02 | **319** | **318** | **163** | **1** |

The last row preserves the failed original global-Deny schema attempt; it is not a qualification count. Headless02 has a grant-audit gap corrected by 03. These counters do not include old campaigns or CLI `--help`/version checks.

## Actual effects, grants and context

`permission-validation20260927-07.json` independently PASSes all seven successful captures and native headless03. Exact file bytes/hash/mtime/mode and SQLite `mode=ro` snapshots are preserved at dispatch, held Ask and after replies.

- Once: initial `approval.txt` bytes remain `before approval\n` with mode 0640 and unchanged mtime through Ask and the 500ms held snapshot. Actual reply executes the patch; no grant row is saved.
- During all nine Ask-held cases (once/reject/always/MCP/child/read/shell/glob/URL): target files unchanged, no provider request between pending and held, and no current-call operation row observed. This last observation is **not** a replacement for typed ToolCallStarted/CurrentIntent gates.
- New Home: actual mouse `+` → Home leaves Ask pending, preserves files/request count and displays attention `!`; clicking attention returns to the original pending Ask. Real Ctrl+F enters/exits fullscreen.
- Second patch asks again. Esc or the actual Reject mouse option interrupts without changing bytes, mtime or modes. No saved grant.
- Always: Right then Enter, or actual mouse at 80, executes and saves exactly one native `apply_patch`/`approval.txt` project grant. Actual original saves its `edit`/`*` grant. Native never broadens its grant for visual matching. Real process exit/relaunch then applies the restart patch without reasking; the same grant survives.
- Mixed allowed+denied multi-file patch: valid preimage, actual Deny result, both files unchanged, one existing grant unchanged. Config-auto and CLI-auto also cannot override Deny and save zero grants.
- MCP: real local stdio server tools/call count stays 0 while Ask, then becomes 1 after Once. Real return enters subsequent provider context.
- Child: actual delegated read Ask → Esc → whole draft `VIS36-FEEDBACK: do not read; continue with this correction.` visible → Enter. Native feedback is present in the child's next actual provider request, and files remain unchanged. Original returns generic Unable-to-read information and does not carry this feedback into the next child context. Both children return to the parent; this is an observed donor/native context difference, not fabricated parity.
- Normal read and shell execute only after Once; shell marker becomes present only after reply. Glob and admissible `https://example.invalid/...` WebFetch Ask are rejected; files remain unchanged and no extra request is made. Inline and fullscreen actual previews are captured at every prompt width.
- Settings: actual `/settings` → filter Permissions → Enter renders `auto accept`, clears native search to Search, persists actual CLI config; next read runs without Ask and saves no grant. Actual Ctrl+P → Commands → Open settings → Enter reopens Settings; toggling Prompt then causes a subsequent read to Ask again. First Esc dismisses both dialogs in these frames after the toggle. Config state/effects demonstrate the successful toggle; no typed Settings ACK event claim is inferred from rendering.

## Requested current frontend fixes — exact zero-based cells

`permission-fix-cells20260927-01.json` retains full relevant lines for all four widths; `permission-summary20260927-08.json` and its runner log retain token/style coordinates. These diagnostics do not alter, crop or mask full frames.

| Observation | Current actual result |
|---|---|
| Pending Patching filename | Fixed: both 120/80 show `Patching` at (7,7), `approval.txt` at (16,7). Real bordered pending card. |
| 120 inline/footer offset | Fixed: Ctrl+F fullscreen (73,37), Enter confirm (102,37) on both sides. Fullscreen minimize (75,37) matches. |
| 80 feedback footer | Fixed: whole draft starts (5,36), final `this correction.` (5,37), Enter confirm (50,37) on both. No suffix-only predicate. |
| Settings category | Fixed: 120 Session (34,15), Permissions (34,16), value (75,16); 80 (14,15), (14,16), (55,16), matching original category/entry coordinates. |
| Successful Settings search reset | Native Search at (34,13) / (14,13) after successful toggle and actual mode change. Original still has Permissions at these positions. This requested native reset is a truthful full-grid difference. |
| URL wrapping | Fixed: 120 Permission (7,29), WebFetch row30 with continuation row31, URL label/value rows33/34; 80 Permission (7,27), title rows28–30, URL rows32–34. Original/native lines match those wrapped previews; URL title is `% WebFetch`, not an invented arrow label. |
| Failed-patch blank line | Fixed: 120 `# Patch failed` at (5,24), blank bordered row25, decline text (5,26). 80/79 title row25, blank26, text27. 121 title24, blank25, text26. |
| **Failed-patch filename** | **Still missing in native at all four widths.** Original title contains `approval.txt` starting x=20 on the title row; native title ends after `# Patch failed`. |

Remaining concrete product diagnostic: submit `VIS36 once` → Once → submit `VIS36 reject` → actual Reject (mouse at 80, Esc otherwise) → completed card. At 120/121 missing filename starts at expected (20,24); at 79/80 expected (20,25). The filename still appears in the earlier successful edit card; that does not satisfy the failed-card contract. No harness workaround or fake title is applied.

Other retained differences include:

- Always-selected 120: donor Permission/Edit at (7,31)/(7,32); native (7,29)/(7,30), with truthful explicit patterns introduction (5,32) and `- approval.txt` at (5,34). The grant semantics must stay exact; do not replace with `*` to match the donor.
- Read pending filename appears at (12,29) native vs (12,28) original at 120/80, while Permission/title/footer positions now agree.
- Delegation/MCP arguments, prior transcript group formatting, shell no-output metadata, rejected tool result formatting and actual elapsed metadata still affect whole frames. The full comparator data is authoritative; these examples are not an exhaustive visual defect list.

## Full unmasked comparators and cursor

| Scope | Full styled grid | Full PNG | Cursor objects equal |
|---|---|---|---|
| 29–34 | 200 DIFFERENT, 12 INVALID, 0 EQUAL | 212 DIFFERENT, 0 EQUAL | 162 / 212 |
| 36 | 51 DIFFERENT, 0 INVALID, 0 EQUAL | 51 DIFFERENT, 0 EQUAL | 43 / 51 |
| All seven | **251 DIFFERENT, 12 INVALID, 0 EQUAL** | **263 DIFFERENT, 0 EQUAL** | **205 / 263** |

The 12 INVALID grids are capture30 original hidden cursor `(80,37)` on an 80-column grid for MCP/child/read/shell/glob/URL fullscreen and inline; native reports `(75,37)`. Comparator reports Cursor outside grid. Coordinates are **not clamped**. No mask, crop, title removal, duration normalization or Always exemption was applied; all full frames are included. Full VIS36 does not PASS.

## Scoped local syscall evidence

Capture36 adds opt-in strace only around the native isolated fixture subprocess and descendants, both generations: `strace -f -ttt -yy -s 512 -e trace=%file,%network -o ... -- target/debug/oc tui`. No authoring-process attach or real credentials/.env/.opencode access is used.

`permission-syscall20260927-01.json` PASS:

- 26 destination calls, all to the actual local provider `127.0.0.1`; **0 non-loopback destinations**.
- **0 DNS transport or resolver-setup calls** in the entire traced lifecycle, including before URL Ask and after its Reject.
- Each dispatch→held window has **0 target-file write opens/path mutations**. Once/reject/always preview windows each have one `approval.txt` open with `O_RDONLY|O_NONBLOCK|O_NOFOLLOW|O_CLOEXEC`; the other six windows have zero such opens.
- Raw syscall traces, launch argv, wall-clock bridge event timestamps and exact file/line references are immutable in capture36.

Limits: `%file,%network` does not capture read/write payloads or mmap dirty pages. Storage WAL writes are expected; assertion concerns target files, independently checked by bytes/mtime/mode snapshots. This is a bounded transport/resolver and file-open/path-mutation audit, not a universal DNS guarantee or typed owner-preview digest audit. Original was not traced.

## Fresh paired headless

`permission-headless-pair20260927-01.json` independently PASSes native headless03 and original headless03, including actual original permission tables opened read-only and checked empty.

| Case | Actual native argv shape | Actual original argv shape | Both result/counts |
|---|---|---|---|
| Config-auto without consumer | `--data-dir DIR run MESSAGE --json` | `run --standalone --format json MESSAGE` | exit1, no marker/grants, 2 requests / 2 complete / 1 call |
| CLI-auto Once | same + `--auto` | same + `--auto` | exit0, marker created, zero grants, 3 / 3 / 1 |
| CLI-auto Deny | same + `--auto`, bash Deny | same + `--auto`, exact shell-command Deny | exit0, no marker/grants, actual denied function result, 3 / 3 / 1 |

Native no-consumer stderr says `approval required ... no consumer`; original says permission requested and `auto-rejecting`. This observable behavior difference is preserved. Original uses an exact command Deny to keep its shell schema admitted; its global shell Deny hides the tool from provider discovery (failed attempt01). CLI `--auto` overrides neither actual Deny. All final requests are schema-valid; effects are real `/usr/bin/touch` in fresh isolated projects.

## Failed/audit-gap attempts preserved

1. `permission20260927-35.runner.log`: new trace flag omitted from exclusive-permission allowlist; runner rejected flags before output directory/build/process creation. Fixed only the capture option validation, next run is 36.
2. `permission-reference-headless20260927-01/`: global shell Deny removed shell from original provider schema. Fixture correctly rejected the unsupported ordinary call request (1 invalid, no emitted denied call). Original exit1/no effect here is **not** actual runtime-Deny qualification. Its initially written `targeted_effect_pass` was too weak for Deny; final pair validator requires emitted call/valid requests.
3. `permission-reference-headless20260927-02/`: exact-command Deny produced valid actual permission rejection and all effects/counts passed, but shared patch snapshot had not loaded original permission tables. Empty derived `grants` could not establish a read-only grant audit. Changed only headless script to the permission snapshot helper and recaptured as03; final independent validator verifies tables actually present/loaded/empty. Attempt02 is effect/wire evidence only.

## Reproduction and independent verification

Main capture command (numbers/widths from table; 33 adds `--permission-mode auto-config`, 34 adds `--permission-mode auto-cli`, 36 adds `--permission-strace true`):

```sh
node scripts/tui_capture/capture.mjs --output evidence/tui/recovery-v00/permission20260927-NN --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true --sample short --sidebar hide --columns WIDTH --rows 40 --permission true
python3 scripts/tui_capture/permission_headless.py evidence/tui/recovery-v00/permission-headless20260927-03 /home/opencode/ai/oc/target/debug/oc
python3 scripts/tui_capture/permission_reference_headless.py evidence/tui/recovery-v00/permission-reference-headless20260927-03 /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
node scripts/tui_capture/analyze_permission.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-analysis20260927-11.json 29 36
node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-validation20260927-07.json permission20260927-29,permission20260927-30,permission20260927-31,permission20260927-32,permission20260927-36 permission20260927-33,permission20260927-34 permission-headless20260927-03
node scripts/tui_capture/check_permission_headless_pair.mjs evidence/tui/recovery-v00 permission-headless20260927-03 permission-reference-headless20260927-03 evidence/tui/recovery-v00/permission-headless-pair20260927-01.json
python3 scripts/tui_capture/permission_syscall_audit.py evidence/tui/recovery-v00/permission20260927-36 evidence/tui/recovery-v00/permission-syscall20260927-01.json
```

These are actual recorded command forms; completed outputs refuse overwrite. To reproduce, select fresh output names. Runner logs retain stdout/stderr. `permission-summary20260927-09.json` totals include native headless03 only; add original03's 8/8/3/0 for the paired total above.

Node syntax, Python py_compile, `git diff --check`, and actual xterm frontend check PASS (`permission-frontend20260927-03.runner.log`). Frontend check qualifies capture cell/style/cursor/DSR mechanics, not product parity. Agent did not run Cargo tests/clippy/release or acceptance scripts.

## Honest remaining matrix

- Full styled-grid/PNG parity and original/native VT hidden-cursor policy: OPEN, including missing failed-card filename. No Unsupported waiver.
- Every tool/action × every width × keyboard/mouse/inline/fullscreen decision: not exhaustive. Current four-width probes cover real patch Once/Reject/Always+restart/Deny, child Reject-feedback, MCP/read/shell Once, glob/URL Reject, Home attention and Settings mode changes. Normal tool Always, additional Reject/Once permutations, multi-call queues, focus/feedback cancel/edit, wheel/resize and additional preview variants remain unqualified by this campaign.
- Typed CoreAPI/CoreEvent, CurrentIntent, immutable owner-preview digest, atomic multi-resource grant rollback/write failure, grant scale, project clone/worktree separation, pinned shell cwd attacks and subagent/compress preflight: parent backend gates are reported PASS separately. This paired PTY evidence does not expose those typed internals or independently repeat the full backend matrix.
- Syscall observation is now qualified for one actual native 120 run; no original or all-width OS audit and no preview read payload/digest trace.
- Three bounded original/native headless cases are now paired; exhaustive headless tool/mode permutations and four-width auto-mode crossing remain open. Parent release entry PASS is separate from the debug paired capture association.
