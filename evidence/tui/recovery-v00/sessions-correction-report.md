# VIS30 Sessions correction recapture — 2026-09-26

## Current result

Final source-built repeat **sessions-live-11** follows the completed workspace
gate `tool_0de60cc67001Tsl2vZu7GUCjgE`: fmt, locked workspace tests (zero failures),
all-target strict Clippy, locked build, capture syntax/frontend, docs/progress and
diff PASS. Both original and native again pass the full current-day functional
workflow with six actual requests/completions each and zero invalid or extra
interaction requests. The destructive confirmation now uses resolved theme base
slots (dark text on red), with a contrast regression test, and footer labels are
plain. Its full comparison still differs in 235/4800 styled cells with equal
cursor; all 31 grid and 31 PNG comparisons remain DIFFERENT. Multi-day remains
unverified. The superseded unreadable native confirmation in -10 is preserved,
not claimed as delivered behavior.

Three failures in the first workspace gate were diagnosed rather than suppressed:
AUD31 used ID ordering instead of exact title selection; the scoped restart test
searched the ID instead of the visible persisted title; the retired-model case
exposed execution-only admission improperly applied to opening history. Fixtures
now select truthful unique titles, and opening history retains retired choices
without rewriting preferences. Submission still requires explicit remediation.
The final full PTY suite passes 30/30 with all substantive assertions retained.

**Latest same-attempt paired workflow `sessions-live-10`: original PASS and native
PASS for current-day Sessions behavior, including foreign-root Enter and return.
Exact parity remains DIFFERENT; multi-day is unverified.** See the latest section
below for source attestation, owner effects and precise remaining styling gaps.

### Historical result (09, preserved)

Cross-directory selected-row rename and confirmed delete are verified on both
executables within the same attempt, `sessions-live-09`. Full native workflow is
FAILED: entering the foreign selected root still fails. Original completes all
stages. No entry predicate was weakened; the native error frame is FAILED_STATE,
the failed stage remains in `sessions-checks.json`, and independent deletion was
qualified after recording the failure.

The previous [`sessions-evidence-report.md`](sessions-evidence-report.md) describes
01–05 and the earlier binary. This report describes the corrected source/binary and
06–10. All previous attempts remain intact.

## Latest source recapture — sessions-live-10

Fresh isolated original/native homes and actual UI-created sessions under
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-10`.
No continuation from the already-deleted 09 selected IDs. New owner-authorized
campaign `sessions-open-correction-20260926` forwards **12 requests total**:
6 original and 6 native, all completed, zero invalid. No extra requests occur
during any search, dialog, scope, entry, reopen, rename or deletion action.

### Build attestation

- `--build-oc true` executes `cargo build --locked`, exit **0**.
- HEAD `6444c54c58204f11eff7793dbe2de83445ca71f2`, parent-supplied dirty source.
- Native executable SHA-256:
  `688a29436b8136b333dd08d49f9123bce5035f4762cc9e8dc41e1b87acc29b02`.
- Source-manifest SHA-256:
  `054fd41a79d890e6524dcaf481f52da3b6efc449ef4468e1c6ecc41dba9bbba9`.
- Original remains the pinned v2.0.12 executable and source listed below.
- Full argv, source diff/manifest, build output and comparator exits are retained
  in `sessions-live-10/{commands.json,capture.lock.json,source-manifest.json}`.

### Functional qualification and truthful owner effects

Both `upstream/sessions-checks.json` and `oc/sessions-checks.json` report **PASS**,
`CURRENT_DAY_FUNCTIONAL; multi-day incomplete`, with no failed stage.
Both complete all/cwd scope, title search, Enter current ocean, reopen → Esc →
reopen, all-scope selection of foreign lunar, Ctrl+R rename/submit/reopen/search,
Enter foreign lunar, verify adopted `other-project` cwd header and row membership,
all-scope search/Enter ocean again, reopen/search foreign lunar, first Ctrl+D
confirmation, second Ctrl+D deletion, all/cwd scope and final Esc. This preserves
the established order (rename before foreign Enter) and strict predicates.
The previously rejected foreign-entry stage now succeeds on native in
`oc/sessions-entered-selected-root.*`; `oc/sessions-selected-root-cwd.*` proves
the resulting cwd scope. `oc/sessions-returned-current.*` proves return.

- Original selected foreign ID `ses_f21c3d907ffehmmiw0ci2Chj5X`, genuine generated
  title `VIS30-LUNAR lunar geology discussion` → actual manual UI title
  `VIS30 Renamed lunar geology`. Created `1790434879241` ms unchanged; updated
  `1790434889000` → `1790434966535` ms. Still present in `armed-db`, absent in
  `deleted-db`. Ocean `ses_f21c39343ffep67oEvM1AykBu7` and forest
  `ses_f21c34747ffeMvhTDvUQPJKlE9` survive with identical metadata.
- Native selected foreign ID `s-tui-18d8e72a760a9320-37f8ca-0`, genuine generated
  title `Lunar Geology Overview VIS30-LUNAR` → the actual manual title above.
  Created `1790435021` seconds unchanged; updated `1790435031.459` →
  `1790435103.350` seconds. Still present in `armed-db`, absent in `deleted-db`.
  Ocean `s-tui-18d8e72e7bf01172-37f974-0` and forest
  `s-tui-18d8e731dcdb5363-37f974-1` survive with identical metadata.

Each side starts with an empty own session DB. Snapshots are read-only, and the
only deleted row is the selected fixture-owned foreign root. Provider counters
stay **6 requests / 6 completions / 0 invalid** on every one of the 28 dialog
action frames per side. Together with 3 generation frames per side, this yields
**62 full styled-grid/PNG capture sets (31 paired stages)**. No title, response,
timestamp or owner effect is synthesized, and no failed earlier attempt is removed.

### Exact comparisons and remaining visual gaps

| Mode | EQUAL | DIFFERENT | BLOCKED | INVALID |
|---|---:|---:|---:|---:|
| Styled grid | 0 | 31 | 0 | 0 |
| PNG | 0 | 31 | 0 | 0 |
| Total | 0 | 62 | 0 | 0 |

Runner exit **1** reflects exact differences, not functional failure. Every
whole-frame comparator is report `FAIL` / runner `DIFFERENT`, exit 1; no masked,
cropped or normalized comparison is substituted.

Delete-confirm: **351 / 4800 styled cells differ**, cursor equal; **24,561 /
647,040 RGBA pixels differ (3.7959013%)**, both 1011×640. See the corresponding
`sessions-delete-confirm.{grid,png}-diff.json`.

The Today category row, option gutter, 88-column dialog, footer ordering/scope
placement and `Sessions for other-project` header now align in text/geometry.
Remaining exact styling defects are directly observed in full cells:

- **Native confirmation is visually unreadable:** in
  `oc/sessions-delete-confirm.cells.json`, zero-based `y=16`, `x=17..102`, native
  background is `#eeeeee`; original is red `#e06c75`. Confirmation glyphs at
  `x=20..48` have native foreground `#eeeeee` (same as background), whereas
  original foreground is `#0a0a0a`. UI text and actual delete effect pass, but
  that does not qualify this styled confirmation as visually correct.
- Footer labels `delete`/`rename` (`y=18`, `x=20..25,35..40`) remain bold natively
  while original is not bold. Spacing cells `x=26,41` have native foreground
  `#ffffff` versus original `#808080`; scope spacing `x=95` is native `#ffffff`
  versus original `#eeeeee`. Same footer differences appear in
  `sessions-selected-root-cwd`.
- Outside the dialog, native foreign entry shows a Location-local one-tab deck;
  original retains three tabs. Native returns to the two current-root tabs;
  original retains the lunar tab as well. This is an observed deck presentation
  difference, not an entry failure or a claim that generic foreign history is open.
- Native lacks the original Build attribution, has a different prompt border
  color, token-rate label and context/path metadata. Genuine provider titles,
  natural-language sentences and timings differ and remain fully visible.

`node evidence/tui/recovery-v00/sessions-live-10/audit.mjs` reproduces counts,
read-only metadata effects, surviving-row equality and whole-frame style-coordinate
diagnostics; it does not write evidence or change comparator outcomes.
Multi-day grouping remains **UNVERIFIED / INCOMPLETE**: no genuine archived public
artifact with provenance was supplied; historical timestamps were never seeded.
No full VIS30 parity PASS or TUI_PARITY_VERIFIED claim is made.

### Exact command and final checks

```sh
node scripts/tui_capture/capture.mjs \
  --sessions-interaction true --sessions-campaign sessions-open-correction-20260926 \
  --geometry true --sidebar hide --sample short --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/sessions-live-10
```

Read-only audit, both module syntax checks, Python bridge compilation, frontend
qualification and `git diff --check`: **PASS**. This worker modified capture
tooling/evidence only. Cargo exclusive ownership is released at handoff.

## Historical correction campaign 06–09

## Source/build and bounded execution

- HEAD `6444c54c58204f11eff7793dbe2de83445ca71f2`, parent-supplied dirty correction.
- Native SHA-256, constant for 06–09:
  `11e66f6dd1bb51c888edf38388636a1a06f3696bf6a7ed3c357770a1f0d40afa`.
- Final source-manifest digest (09):
  `fd332aba5dc5bb0feb6bac25fc73bc9724459aed13f5f0a91daed4d9cf802732`.
- Original binary SHA-256:
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
  pinned v2.0.12 source `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Every invocation ran `--build-oc true` → **`cargo build --locked` PASS**.
  This worker had exclusive Cargo ownership. No Rust, `.opencode`, commits or push
  were changed. Final Cargo ownership is released.
- New owner-authorized campaign `sessions-correction-20260926`: **24 forwarded live
  requests total**, including failures; separate from the already-ended 01–05
  campaign. The bridge counts prior attempts with the same explicit campaign ID
  and enforces a cumulative 24-call ceiling. No generation requests remain available
  in this campaign.

| Attempt | Original calls | Native calls | Actual outcome |
|---|---:|---:|---|
| 06 | 6 | 6 | Original full PASS; native lunar/ocean titles persisted, forest title HTTPError, real null-title row retained |
| 07 | 6 | 2 | Original full PASS; native lunar transcript completed, title stream BrokenPipeError, no persisted title |
| 08 | 2 | 0 | Same-attempt cross-dir rename verified; original full PASS, native foreign Enter rejected |
| 09 | 2 | 0 | Same-attempt cross-dir rename + confirmed delete verified; original full PASS, native foreign Enter still rejected |

06 and 07 use fresh roots with actual UI-created sessions. 08 and 09 reuse only
06's fixture-owned original/native homes and genuine generated sessions under:

```
/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-06
```

The original lunar fixture was already deleted by each successful original flow;
08/09 recreate it through real UI prompts and real provider title requests. Native
uses its actually generated 06 lunar/ocean rows, with title-generation provenance
in 06 protocol and current metadata in 08/09 read-only snapshots. No session import,
DB mutation, synthetic title, historical timestamp, masking, file deletion or
relabelled null-title fixture was used. The failed native forest fixture remains
visible as its real fallback row.

The native title failures precede Sessions actions and are not evidence of a
rename/delete failure. 06 retained `HTTPError` without its status code; 07 recorded
`BrokenPipeError`. Native automatic-title source has a 10-second timeout at
`crates/oc-adapters/src/application.rs:2432–2446`; the broken pipe is consistent with
client cancellation, but this capture does not establish exact title-timeout
causality. The runner subsequently records numeric HTTP status on new HTTP errors,
without saving raw responses or secrets.

## Same-attempt owner effects (09)

See [`sessions-live-09/upstream/sessions-checks.json`](sessions-live-09/upstream/sessions-checks.json)
and [`sessions-live-09/oc/sessions-checks.json`](sessions-live-09/oc/sessions-checks.json).

Both sides perform: all/cwd scope; title search; Enter ocean; reopen → Esc → reopen;
all-scope search of the non-current foreign lunar root; Ctrl+R rename; submit;
reopen/search new title; first Ctrl+D arms confirmation; second Ctrl+D deletes;
verify surviving owner IDs; all/cwd lists and final Esc. Original additionally
enters lunar, checks cwd scope for `other-project`, switches all, searches/enters
ocean again, and reopens all before deletion. Native strictly fails that foreign
entry step, records its actual error and continues the independent deletion while
ocean remains current.

### Exact IDs and truthful metadata

- Original selected foreign root `ses_f2204bb54ffexMUfXqGOt1FbS2`:
  real generated `Lunar geology overview VIS30-LUNAR` → actual manual UI rename
  `VIS30 Renamed lunar geology`. `time_created=1790430627014` ms unchanged;
  `time_updated` advances `1790430634127` → `1790430682412` ms.
  It exists in `armed-db` and is absent in `deleted-db`. Ocean
  `ses_f221254a4ffeR3WrdQyNa4VpSn` and forest `ses_f22121aaeffe7pFrrzy87lukuh`
  survive with their original titles and metadata.
- Native selected foreign root `s-tui-18d8e276da26b6ae-36d1c9-0`:
  real generated in 06 as `VIS30-LUNAR explores lunar geology briefly.`, manually
  renamed in 08, then renamed in 09 from `VIS30 Renamed forest ecology` to
  `VIS30 Renamed lunar geology`. `created_at=1790429851` seconds unchanged;
  `updated_at` advances `1790430522.882` → `1790430780.785` seconds.
  It exists in `armed-db` and is absent in `deleted-db`. Current ocean
  `s-tui-18d8e27a0524fcbd-36d28c-0` remains unchanged; failed-title forest
  `s-tui-18d8e27db034d566-36d28c-1` survives as its actual null-title row.

All observations use SQLite URI `mode=ro`; Location membership is observed from
original session directory and native owner Location prefs. The first delete key
does not delete; second key does. Every dialog action leaves provider counters
unchanged: original **2 requests / 2 completions / 0 invalid**, native
**0 / 0 / 0** for the entire 09 action sequence. Original's two calls occur only
during actual lunar recreation before the baseline. Provider counters do not reset
at modal open, rename, Enter, reopen, scope changes or deletion.

## Precise unresolved code-level blocker

Native Enter on the selected all-projects foreign root leaves Sessions open and
displays:

```
application: session s-tui-18d8e276da26b6ae-36d1c9-0 belongs to location
/home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-06/other-project
```

Actual frames: 08 `oc/sessions-failure.*`, 09
`oc/sessions-selected-entry-failure.*`. DB confirms rename already succeeded on
that exact ID, and the current ocean root/location remains intact. This is not a
runner title/selection mismatch or a fabricated failure state.

Source trace: `crates/oc/src/tui_cmd.rs:1872–1907` handles
`PanelIntent::SwitchSession` by calling generic
`app.session_selection(target, false, SelectionAction::Current)` and
`history_page` under the current Location, before closing the picker. In contrast,
the corrected `PickerSessionAction` handler at
`crates/oc-adapters/src/application.rs:1430–1486` explicitly resolves the checked
listed row's foreign binding for Rename/Delete only. The remaining engineering
step is checked foreign-root entry/Location adoption in the Sessions selection
route, preserving owner validation and active-turn restrictions. This worker did
not edit Rust or bypass that owner rejection.

## Exact comparator results

Every capture has full styled cells, unmasked PNG, text, VT, render measurements,
inputs and protocol. Capture locks retain environment/fixture/binary/source seals.
The comparator uses full frames without regions, normalization or resizing.

| Attempt | DIFFERENT comparisons | BLOCKED (missing-side) comparisons | EQUAL | INVALID |
|---|---:|---:|---:|---:|
| 06 | 4 (2 stage pairs × grid/PNG) | 60 | 0 | 0 |
| 07 | 0 | 64 | 0 | 0 |
| 08 | 30 (15 stage pairs) | 32 | 0 | 0 |
| 09 | **38 (19 stage pairs)** | **26** | **0** | **0** |

09's missing-side comparisons include failed native foreign-entry/return stages,
native-only failure/follow-up captures, optional all-scope stage already effective
on one side, and generated-vs-retained prerequisite frames. BLOCKED is not a PASS.
All capture invocations exit **1**; original full workflow PASS and native partial
action success are reported independently of exact visual equality.

09 delete-confirmation comparison is valid under the same attempt/fixture/profile:

- **Grid DIFFERENT / report FAIL**, exit 1: **913 / 4800 different styled cells**;
  cursor equal. `sessions-live-09/sessions-delete-confirm.grid-diff.json`.
- **PNG DIFFERENT / report FAIL**, exit 1: **94,648 / 647,040 differing RGBA pixels**,
  14.6278437%; both images 1011×640.
  `sessions-live-09/sessions-delete-confirm.png-diff.json`.

### Current renderer differences visible in corrected captures

- Outer dialog width/left alignment now matches the 88-column donor presentation;
  both headers begin at column 20 (zero-based) at 120×40. Native scope footer is
  fully visible (`current directory` / `all projects`), no old truncation.
- For one filtered row, original renders `Today` on its own category row and the
  option beneath it; native places `Today` at the right of the option row. Native
  option text also has an extra three-column gutter. The filtered native dialog is
  correspondingly one row shorter.
- Footer ordering/layout differs: original `delete ctrl+d  rename ctrl+r` plus
  right-aligned scope; native `ctrl+r rename  ctrl+d delete  ctrl+a scope` as one
  left-aligned line.
- In cwd scope, original paints `Sessions for project`; native captured header is
  `Sessions` (`09/oc/sessions-cwd-scope.txt`). This remains an observed difference
  despite correct row membership filtering.
- Native lacks original Build attribution in this configuration; real titles,
  assistant sentence, token-rate/time, tab roster and context/path metadata differ.
  Native failure toast is genuine and remains visible in its independent deletion
  follow-up. Delete-frame pixel counts include this real error state; they are not
  a claim of equal successful entry states.

Multi-day grouping remains **NOT COVERED / INCOMPLETE**: no real archived public
session artifact with provenance is available. No timestamps were invented to
claim it. No VIS30 full PASS or TUI_PARITY_VERIFIED claim is made.

## Exact commands

06/07 (substitute output suffix `06` or `07`):

```sh
node scripts/tui_capture/capture.mjs \
  --sessions-interaction true --sessions-campaign sessions-correction-20260926 \
  --geometry true --sidebar hide --sample short --columns 120 --rows 40 \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true \
  --output evidence/tui/recovery-v00/sessions-live-06
```

08/09 add these flags and use their respective output suffix:

```sh
--sessions-root /home/opencode/.cache/opencode-tmp/opencode/t44-reference/runs/sessions-live-06 \
--sessions-evidence evidence/tui/recovery-v00/sessions-live-06
```

Each attempt's `commands.json` is the exact argv/build/comparator exit record.
Do not rerun the destructive continuation against deleted IDs. Inspect the actual
owner DB and fixture provenance before any new capture; this campaign is exhausted.

Targeted tooling checks: `node --check` on capture/sessions modules,
`python3 -m py_compile scripts/tui_capture/bridge.py`, frontend qualification and
`git diff --check`. No product source checks were substituted for the parent-owned
correction agent's gates. Cargo is released at handoff.
