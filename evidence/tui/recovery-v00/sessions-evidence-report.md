# VIS30 Sessions — bounded live capture, 2026-09-26

## Result

**Current-day functional evidence captured; full VIS30 parity is incomplete.**
Pinned original completes the scenario in `sessions-live-04/upstream`;
native completes current-directory selected-row rename/delete in
`sessions-live-05/oc`. Native cross-directory rename fails in
`sessions-live-04/oc`. The final native continuation is intentionally single-side:
the original already deleted its fixture-owned selected root. It is not a new,
identical-state paired PASS. Earlier attempts are retained verbatim.

Full styled 120×40 grids, PNGs (1011×640), text, cumulative VT, render geometry,
actual PTY inputs, protocol, source manifest, binary hashes and commands are stored
per attempt. No masks, synthetic completion/title/session rows, direct DB writes,
timestamp seeding, renderer substitutions or Rust edits were used.

## Executables and source attestation

- Repository HEAD: `6444c54c58204f11eff7793dbe2de83445ca71f2`, dirty native Sessions
  implementation supplied by parent. T44 remains active.
- Original: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
  version `opencode v2.0.12`, pinned source
  `2670273ff17da96f85c5826ced57aa1b368754fa`; runner checks binary SHA-256
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- Native: `/home/opencode/ai/oc/target/debug/oc`, version `0.1.0`, SHA-256
  `96338e5283807c6d25b65b4fd0e4d626a21f58f6c54cef1a703e756732b7886e`.
  All five invocations explicitly ran `--build-oc true` → `cargo build --locked`.
  Cargo was exclusively owned by this capture worker; parent ran no Cargo here.
- Final native source-manifest digest:
  `9c6e011fae31e87fc368645c2e504a8fa02d041f5decf30ba238f3bbb116ee5c`.
  See `sessions-live-05/{source-manifest.json,capture.lock.json,commands.json}`.
  Earlier locks seal their own tooling revisions; the Rust binary hash is constant.

## Live provider and fixture ownership

Sessions mode reads the owner-approved `.local/live.env` only inside the bridge.
An isolated loopback Responses proxy forwards real requests to OpenProxy using
`OC_TEST_MODEL` (`ocg/muse-spark-1.3-contributor` in this campaign). The configured
application alias is `fixture/fixture-model-1`, with a display name equal to that
real model ID and the inherited fixture provider display label `OpenCode Zen`.
Those are explicit test configuration labels, not provider discovery evidence.
The corrected proxy sends the full real model ID, low reasoning effort, output
limits 2048 for titles / 1024 for transcript. No provider response body, secret key,
Authorization header or real endpoint value is persisted. Generated title text is
recorded as an observable UI/DB fact, not replaced with a canned fixture title.

Campaign forwarded **24 requests total**, including failures/retries:

| Attempt | Original forwarded | Native forwarded | Outcome |
|---|---:|---:|---|
| 01 | 8 | 0 | Incorrect stripped model ID; HTTP errors; native initial predicate too narrow |
| 02 | 2 | 4 | Genuine native lunar title; upstream title incomplete; runner interrupted at 240 s |
| 03 | 6 | 4 | All remaining genuine titles; original delete succeeded but capture predicate mistakenly matched query text; native footer predicate expected an untruncated label |
| 04 | 0 | 0 | Original scenario PASS; native cross-directory rename failed |
| 05 | 0 | 0 | Native current-directory scenario PASS |

Attempt 02 has an unfinished capture lock/IN_PROGRESS native check file because
the runner was interrupted. Its protocol/VT and actual DB state were inspected
before continuation. No unknown action was blindly repeated. Empty-title sessions
from unsuccessful generation remain visible as actual fallback rows; they were
not relabelled or deleted to improve captures.

Only these isolated roots were used:

```
/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-01
/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-02
```

03–05 reuse the latter's own original/native homes and `project` / `other-project`
directories. Sessions were created by submitting actual UI prompts about lunar
geology, ocean currents and forest ecology. Titles differ naturally between live
generations; all timestamps are application-owned current wall-clock metadata.
Cross-day coverage is **INCOMPLETE**: no genuine archived public session record
with provenance was supplied. No historical date was invented.

## Owner facts and functional evidence

Read-only observations use SQLite URI `mode=ro`, on original `session_v2` and native
`sessions` plus the native Location prefs. Each observation has a request ID and
wall-clock timestamp in `protocol.json` and `sessions-checks.json`.

Original full scenario: [`sessions-live-04/upstream/sessions-checks.json`](sessions-live-04/upstream/sessions-checks.json).
Native full current-directory scenario: [`sessions-live-05/oc/sessions-checks.json`](sessions-live-05/oc/sessions-checks.json).
Both record **0 requests / 0 completions / 0 invalid requests throughout dialog
actions**; generation occurred in earlier sealed attempts, not during the actions.

Observed stages: cwd/all scope with real Location membership; title search;
Enter restores the ocean transcript; reopen → Esc → reopen; search another row
while ocean remains current; Ctrl+R rename; actual submission; reopen and search
the updated title; first Ctrl+D only arms confirmation; second Ctrl+D removes the
selected owner row; remaining sessions survive; both scopes and final Esc captured.

- Original selected non-current root
  `ses_f2230e1d9ffeI72DdOLiLz8bor`: generated title
  `VIS30-LUNAR lunar geology overview`, in `other-project`.
  In 04, UI rename writes `VIS30 Renamed forest ecology` to that exact ID;
  `time_updated` advances from `1790427740902` to `1790428050413` ms.
  It still exists in `armed-db`, disappears in `deleted-db`; current ocean root
  `ses_f2230adefffePVLASnUKvZtvya` retains its own title/history.
- Native selected non-current current-directory root
  `s-tui-18d8e0a7ef4c1c44-3635c4-1`: generated title `Discuss Forest Ecology`.
  In 05, UI rename writes the same explicitly typed manual title to that ID;
  `updated_at` advances from `1790427869` to `1790428251` seconds, while
  `created_at=1790427863` remains unchanged. It exists in `armed-db` and is absent
  in `deleted-db`. Ocean `s-tui-18d8e0a44bea6891-3635c4-0` and lunar
  `s-tui-18d8e04505787b99-361dee-0` survive unchanged.

The manual rename is an actual recorded owner action, not a simulated provider
title. No unrelated session/file was removed. Final process termination is the
runner's bounded stop; 04/05 generation-0 cwd transitions exit naturally with code
0. This does not claim natural final quit qualification.

## Honest parity gaps

1. Native all-scope selected foreign-directory rename returns `session not found`
   and leaves the Rename dialog open; original succeeds. See 04 native
   `sessions-failure.{cells.json,png,txt}` and failure DB snapshot. Its Ctrl+U clear
   also did not remove the existing title, so that failed input shows the original
   title followed by the typed suffix. 05 uses actual End/backspace inputs to
   qualify current-directory rename without claiming Ctrl+U parity.
2. Original Sessions dialog starts around column 20 and spans to 100; native starts
   around 34, has a narrower list, and truncates scope hints (`current di`,
   `all projec`). Native does not display original's `Sessions for project` header
   in cwd scope. Action-hint ordering differs.
3. For a single matched option, native renders `Today` as a right-side footer;
   original retains a separate category row. Selection marker/destructive row
   styling and geometry also differ; full grids/PNGs preserve these differences.
4. Native prompt/assistant attribution lacks original's Build label in this
   configuration; tabs, path/context statistics and transcript output also differ.
   Real output/elapsed-time differences have not been normalized.
5. Same-attempt paired comparisons in 03/04 report DIFFERENT for available common
   grids and PNGs; missing stages remain BLOCKED. Successful original 04 vs native
   05 are cross-attempt, different selected-root scopes, different runner fixture
   hashes: official grid comparison is **INVALID**, not weakened to compare as
   equivalent. Full PNG comparison of their delete confirmation is **FAIL**:
   83,395 / 647,040 different RGBA pixels (12.89%). See 05
   `paired-delete-confirm.{grid,png}-diff.json`. No claim of identical-state final
   paired parity, full multi-day VIS30 PASS, or TUI_PARITY_VERIFIED is made.
6. 03 original generated-root PNGs were marked UNSTABLE_CAPTURE when the actual
   styled grid changed during screenshot. These are retained, not presented as
   stable title-generation qualification. Later stable dialogs expose those real
   generated titles, corroborated by provider and DB observations.

## Commands and checks

All capture commands and build exits are in each attempt's `commands.json`.
Base paired invocation (01, 02, 03, 04; output and continuation flags vary):

```sh
node scripts/tui_capture/capture.mjs \
  --sessions-interaction true --geometry true --sidebar hide --sample short \
  --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/sessions-live-01
```

03 adds `--sessions-root /home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-02`.
04 also adds `--sessions-evidence evidence/tui/recovery-v00/sessions-live-03`.
05 uses both continuation flags plus `--sessions-origin oc --sessions-selected cwd`
and output `evidence/tui/recovery-v00/sessions-live-05`. Capture exits are 1 due to
failures/mismatches or missing paired frames; 02's shell timeout was 240 s. These
are not green parity exit codes. Do not replay the destructive continuation against
the already-deleted selected fixtures; inspect the retained manifest and owner DB.
The live campaign is exhausted; dialog-only continuation generated no new calls.

Completed targeted checks:

```sh
node --check scripts/tui_capture/capture.mjs
node --check scripts/tui_capture/sessions.mjs
python3 -m py_compile scripts/tui_capture/bridge.py
node scripts/tui_capture/check_frontend.mjs
git diff --check
```

All PASS. Frontend preserves RGB, styled blanks, modifiers, wide/combining symbols,
cursor and DSR replies. `cargo build --locked` PASS on every capture invocation.
No Cargo tests/clippy or Rust edits were part of this capture-only assignment.
Existing capture files, `.opencode`, Git commits/push and product source were not
modified by this worker. Cargo ownership is released after this report.
