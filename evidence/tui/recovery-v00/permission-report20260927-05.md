# VIS36 report 5 — prepared-operation binding recapture 37–42

## Verdict

**Targeted rejected-card regression and actual permission lifecycles PASS. Full VIS36 remains OPEN.**

New immutable paired captures 37–40 cover prompt widths 120/80/79/121 × 40; 41 covers config-auto at 80 and 42 CLI-auto at 121. Fresh native and original headless04 qualify three paired headless behaviors. This report qualifies only these new executions. Captures 01–36, previous headless03 and capture36's syscall trace remain historical-source evidence; the old strace does not qualify this new binary. All older artifacts, including failures, are preserved.

The real pinned original uses its existing U19 `patch(patchText)` executor admitted by the fixture hook; native uses actual `apply_patch(patchText)`. Ordinary Responses function calls, actual permission replies, execution, storage, MCP and subagent context drive the UI. No fabricated cards/results, masked screenshots, cropped comparisons or invented elapsed times. The provider and MCP are isolated local fixtures; authenticated/live requests: **0**.

## Source association

- Capture37 HEAD `d77faa6a38abf2306e50981ad9df39c7b91aa26d`; captures38–42 HEAD `a0b8522bb439e0a58aa75c152690d4153a6207ae`, with owner dirty Rust sources. The parent checkpoint changed during the campaign; production inputs did not.
- All six captures ran `cargo build --locked`, exit0.
- All six native executables SHA-256: `c116b0d450420eb93e9531239260bdcef57c32828eed180bff93cc280d550008`.
- Rust/Cargo/toolchain digest: `c586c4a564a58d676a9ee525a1322f36c4ecb658730b6281b3a0c95e986d1240`.
- All six canonical captured source manifests: `34846624caf7f068ea91af2dbe35d11c4eace19e8977656836292941ed4c4b10`.
- `permission-association20260927-05.json`: all canonical hashes valid, all builds exit0, no current Rust input mismatch, current debug binary matches. Post-capture diagnostic script additions are not substituted into the recorded build manifests.
- Original executable `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`, SHA `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`; donor `2670273ff17da96f85c5826ced57aa1b368754fa`.

Read latest `crates/oc-tui/src/app.rs` and `patch_view.rs`: pending projection binds `card.op` to `request.binding.operation` while retaining `permission_pending`. Finished permission rejection preserves known owner-prepared file labels only when the replacement has no file labels and no committed patch effects. The header workaround is absent; existing tool-block rendering supplies the title. This is source inspection, not a typed-event execution trace.

The owner reports two passing Pending → projection → Resolved → Finished/no-Started and cold/warm replay tests. Those and previous parent backend/release gates are separate parent-owned qualifications; they were not rerun by this evidence agent. Cargo was used only for six capture builds and is released. Agent changes in this phase: parameterized/strengthened `scripts/tui_capture/permission_fix_cells.mjs`, added `permission_rejection_audit.mjs`, and new evidence. No Rust, acceptance, progress, `.opencode`, credential edits, commits or pushes.

## Actual counts and natural exits

Counts are per side; original and native independently have these same counts.

| Capture | Width/mode and replies | Targeted original/native | Requests / completed / emitted calls / invalid | Natural exits |
|---|---|---|---|---|
| 37 | 120 prompt; Once Enter, Reject Esc, Always Right/Enter | PASS / PASS | 26 / 26 / 14 / 0 | generations0/1, code0 |
| 38 | 80 prompt; Once/Reject/Always mouse | PASS / PASS | 26 / 26 / 14 / 0 | generations0/1, code0 |
| 39 | 79 prompt; Once mouse, Reject Esc, Always Right/Enter | PASS / PASS | 26 / 26 / 14 / 0 | generations0/1, code0 |
| 40 | 121 prompt; Once mouse, Reject Esc, Always Right/Enter | PASS / PASS | 26 / 26 / 14 / 0 | generations0/1, code0 |
| 41 | 80 config-auto | PASS / PASS | 7 / 7 / 3 / 0 | generation0, code0 |
| 42 | 121 CLI-auto | PASS / PASS | 7 / 7 / 3 / 0 | generation0, code0 |

Every prompt side includes 2 title requests and 24 lifecycle requests. Once/Always/restart/mixed/MCP/childroot/childread/read/shell/autoonce each have 2 requests; Reject/glob/URL/promptagain each have 1, because root rejection interrupts. There is no approval-only extra provider request. Full call arguments, schemas, outputs and contexts remain in each side's `protocol.json`. Comparison-driven capture lock exit1 is not the application exit: all actual application exits above are natural code0, while full comparators remain non-green.

| Scope | Requests | Completed | Emitted calls | Invalid |
|---|---:|---:|---:|---:|
| New paired TUI 37–42 | **236** | **236** | **124** | **0** |
| Fresh native04 + original04 headless | 16 | 16 | 6 | 0 |
| Entire new campaign | **252** | **252** | **130** | **0** |

No failed lifecycle/schema attempt occurred in this new campaign. Historical failed attempts remain recorded in their original directories/logs and previous reports; they are neither omitted from historical accounting nor reused as current-source qualifications.

## Rejected-card fix: exact zero-based cells and effects

Keyboard/mouse path: submit `VIS36 once` → actual Once → actual successful patch → submit `VIS36 reject` → second Ask → Esc (120/79/121) or mouse Reject (80) → completed failed card.

`permission-fix-cells20260927-02.json` independently PASSes all four widths:

| Width | One `# Patch failed` title | Filename `approval.txt` | Blank bordered row | Decline text |
|---|---|---|---|---|
| 120 | (5,24) | **(20,24)** | 25 | (5,26) |
| 80 | (5,25) | **(20,25)** | 26 | (5,27) |
| 79 | (5,25) | **(20,25)** | 26 | (5,27) |
| 121 | (5,24) | **(20,24)** | 25 | (5,26) |

Original/native full title text, following blank row and decline coordinates match, and each frame has exactly one failed title. This targeted line/cell audit does not assert full-frame/style parity.

`permission-rejection20260927-01.json` PASSes native37–40: one terminal rejected operation, state `denied`, no rejected patch-effect row, target file bytes/hash/mtime/mode unchanged from Once through pending/held/rejection, no operation row for the current call in pending/held snapshots, and exactly one reject provider request. Persisted turn function output is exactly `{"status":"permission_rejected","feedback":null}`, matching the terminal operation and historical29–32 output baseline. The historical comparison establishes output stability only; it does not reuse old-source lifecycle evidence. The UI label change therefore does not fabricate applied effects or change the stored provider-projection function output. No subsequent provider request is sent for this interrupted root rejection.

SQLite snapshots cannot prove absence of an emitted typed `ToolCallStarted`/CurrentIntent. The owner tests cover that separately. This report does not infer typed absence from DB-row absence or from the fixed card.

## Full actual lifecycle behavior

`permission-validation20260927-08.json` independently PASSes all six captures plus fresh native headless04:

- Initial patch Ask and 500ms hold preserve `before approval\n`, mode0640, mtime and bytes. Once performs actual mutation, saves zero grants, and second patch reasks. Reject performs no mutation.
- All nine Ask-held windows (Once/Reject/Always/MCP/child/read/shell/glob/URL) preserve target files, have no intervening provider request, and show no current-call operation row. Snapshots and raw protocol boundaries are retained.
- Actual `+` mouse click opens New Home while Ask remains pending, retains attention `!` and effect/request boundaries; clicking attention returns to Ask. Actual Ctrl+F fullscreen and inline previews are captured.
- Always Right/Enter or mouse80 executes and saves one exact native `apply_patch`/`approval.txt` project grant; original saves `edit`/`*`. Real natural process exit/relaunch executes restart patch without another Ask; grant persists. Native's exact grant is not broadened for visual matching.
- Valid mixed multi-file patch encounters actual Deny; both files and existing grant remain unchanged. Config-auto/CLI-auto also encounter real Deny and save zero grants; they do not override it.
- Real local stdio MCP tools/call remains0 during Ask and becomes1 after Once; its actual return enters subsequent provider context.
- Actual subagent child read → Ask → Esc → whole visible feedback `VIS36-FEEDBACK: do not read; continue with this correction.` → Enter. Entire wrapped draft is verified at all four widths, including80. Native includes feedback in next actual child provider context; original still returns generic Unable-to-read without that feedback. Both children return to parent and target files remain unchanged.
- Read and shell run after Once; shell marker appears only after reply. Glob and admissible `https://example.invalid/...` URL Ask are rejected without effects or extra request. Original/native actual wrapped previews are retained fullscreen and inline. No new syscall tracing was performed: current transport/DNS absence is not claimed from historical36's trace.
- Actual Settings `/settings` → Permissions filter → Enter changes persistent mode to auto accept; next read executes without Ask/grants. Native successful toggle resets search to Search. Actual Ctrl+P → Commands → Open settings → Enter reopens; Prompt toggle makes next read Ask again, followed by Reject. Both first Esc paths dismiss after the successful toggle in these captures.

Current retained frontend checks: pending Patching/filename (7,7)/(16,7); 120 Ctrl+F fullscreen (73,37), Enter (102,37), minimize (75,37);80 whole feedback begins(5,36), remainder(5,37), footer(50,37). Settings120 category/entry/value (34,15)/(34,16)/(75,16),80 (14,15)/(14,16)/(55,16). Native Search reset at(34,13)/(14,13) truthfully differs from donor's retained Permissions filter. URL120 title rows30–31 and label/value33–34;80 title28–30 and label/value32–34 remain correctly wrapped.

## Full unmasked comparisons and remaining concrete differences

| Comparator | Actual result across 37–42 |
|---|---|
| Full styled grids | **199 DIFFERENT, 12 INVALID, 1 EQUAL** |
| Full PNG | **211 DIFFERENT, 1 EQUAL** |
| Cursor objects | **162 / 212 equal** |

The equal grid/PNG is capture39 `permission-once-completed`; it is one actual full frame, not overall parity. All212 pairs are included, including Always. No masks, crops, hidden duration changes, removed titles or cursor clamping.

The12 INVALID grids are capture38 original hidden cursor `(80,37)` on an80-column grid versus native `(75,37)`, for MCP/child/read/shell/glob/URL fullscreen+inline. Comparator reports Cursor outside grid. The raw VT/cell cursor policy remains unresolved.

Concrete remaining current-frame examples:

- Read Ask at120: original pending prefix `⋯` at(5,28), filename(12,28); native `→` at(5,29), filename(12,29). Real path: submit VIS36 read after child completion → pending Ask. Approval title/footer coordinates agree, but preceding transcript placement/prefix differs.
- Always-selected120: native Permission/Edit (7,29)/(7,30), exact-pattern introduction(5,32), `- approval.txt`(5,34); donor (7,31)/(7,32), edit-wide introduction(5,34). Actual narrower grants must remain truthful; changing to wildcard is not an acceptable fix.
- Successful Settings search reset, child feedback propagation, MCP argument/delegation formatting, shell no-output metadata, transcript grouping and real elapsed metadata remain unmasked differences. Their presence is not an invented Unsupported waiver.

The previously missing rejected filename is fixed in all four new captures. There is no current partial filename/card regression in this targeted path.

## Fresh paired headless04

`permission-headless-pair20260927-02.json` PASS: native actual new SHA above, pinned original, fresh isolated HOME/project/env-cleared local provider, actual original permission tables observed read-only and empty.

| Case | Native argv shape | Original argv shape | Actual result on both sides |
|---|---|---|---|
| TUI config-auto without consumer | `--data-dir DIR run MESSAGE --json` | `run --standalone --format json MESSAGE` | exit1, no effect/grants;2 requests/2 completed/1 call |
| CLI-auto Once | same + `--auto` | same + `--auto` | exit0, touch effect, zero grants;3/3/1 |
| CLI-auto Deny | same + `--auto`, configured Deny | same + `--auto`, exact-command Deny | exit0, no effect/grants, actual denied result;3/3/1 |

No-consumer stderr remains native approval-required/no-consumer versus original auto-rejecting. Actual Deny cannot be overridden by CLI-auto. All requests are schema-valid. This is three paired behaviors, not every headless tool/mode combination.

## Reproduce and verify

Capture37/38/39/40 uses WIDTH120/80/79/121 respectively;41 WIDTH80 adds `--permission-mode auto-config`;42 WIDTH121 adds `--permission-mode auto-cli`. Each command's stdout/stderr is preserved as its new `.runner.log`.

```sh
node scripts/tui_capture/capture.mjs --output evidence/tui/recovery-v00/permission20260927-NN --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --geometry true --sample short --sidebar hide --columns WIDTH --rows 40 --permission true
python3 scripts/tui_capture/permission_headless.py evidence/tui/recovery-v00/permission-headless20260927-04 /home/opencode/ai/oc/target/debug/oc
python3 scripts/tui_capture/permission_reference_headless.py evidence/tui/recovery-v00/permission-reference-headless20260927-04 /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode
node scripts/tui_capture/analyze_permission.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-analysis20260927-12.json 37 42
node scripts/tui_capture/summarize_permission.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-analysis20260927-12.json evidence/tui/recovery-v00/permission-summary20260927-10.json permission-headless20260927-04
node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-validation20260927-08.json permission20260927-37,permission20260927-38,permission20260927-39,permission20260927-40 permission20260927-41,permission20260927-42 permission-headless20260927-04
node scripts/tui_capture/permission_fix_cells.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-fix-cells20260927-02.json 37 40
node scripts/tui_capture/permission_rejection_audit.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-rejection20260927-01.json 37 40 29
node scripts/tui_capture/permission_association.mjs evidence/tui/recovery-v00 evidence/tui/recovery-v00/permission-association20260927-05.json 37 42
node scripts/tui_capture/check_permission_headless_pair.mjs evidence/tui/recovery-v00 permission-headless20260927-04 permission-reference-headless20260927-04 evidence/tui/recovery-v00/permission-headless-pair20260927-02.json
```

Outputs refuse overwrite: reproduction requires NEW output names and fresh isolated projects. Full commands, build output, manifest, rawVT, input logs, PNGs, styled cells, cursor objects, comparator reports, SQLite snapshots and wire protocols reside in each capture. Node syntax checks for changed scripts, `git diff --check`, and actual xterm frontend check passed (`permission-frontend20260927-04.runner.log`). No Cargo tests/clippy/release/acceptance were run by this agent.

## Honest open matrix

1. Full grid/PNG parity and original out-of-grid hidden VT cursor policy remain open.
2. Every action × width × keyboard/mouse × fullscreen/inline permutation is not exhaustive: normal-tool Always and extra reply variants, multicall queues, feedback cancel/edit/focus, wheel/resize and additional previews remain unqualified here.
3. Typed CoreAPI/CoreEvent/Started/CurrentIntent and owner-preview digest, cold/warm replay tests remain separate parent gates. This capture's row/effect audit does not replace them.
4. Atomic multi-resource grant failure/rollback/write errors, project/clone/worktree boundary, grant-scale and pinned-cwd adversarial cases are not injected in this campaign; previous backend gates are separate.
5. Latest-source OS syscall/DNS/read payload tracing is open. Old36 remains valid for its old source only; original and all-width tracing was never qualified.
6. Three headless modes are paired on this source; every tool/mode and auto-mode cross-width combination, release and mandatory acceptance gates are not comprehensively qualified by these debug captures.

These are gaps, not Unsupported waivers. Next parent work can address the concrete read-pending/VT/full-frame differences while retaining exact native grant semantics and the newly fixed prepared-operation rejection path.
