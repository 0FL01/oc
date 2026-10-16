# VIS36 final campaign handoff — authoritative corrections and totals

**Full VIS36 is OPEN; no whole-grid or PNG pair is EQUAL. Cargo is released.**
The detailed reproduction, behaviors, zero-based UI defects and open matrix are
in [report01](permission-report20260927-01.md). This new immutable report supersedes
its totals and auto-mode Deny claims; report01 is retained as historical evidence.

## Correction found during final review

Attempts10/11 prove real config-auto/CLI-auto patch and shell effects and zero saved
grants. Their mixed patch expected `restart approved`, but those short campaigns
had only performed Once, leaving the long approved line. Original rejected it with
`patch verification failed: Failed to find expected lines in approval.txt: restart
approved`. Therefore10/11's unchanged files **did not qualify paired auto-mode
Deny enforcement**. Their scoped script PASS must not be read as that qualification.

The fixture now makes the auto mixed patch's preimage match the actual Once bytes.
Fresh immutable13/14 execute that valid mixed allowed+denied patch. Both original
and native return an actual denial, preserve both file bytes/hash/mode/mtime and
save zero grants. The independent validator additionally asserts `denied` in the
actual provider result; a malformed patch cannot satisfy this qualification.

| Attempt | Width | Mode | Original requests/completed/calls | Native requests/completed/calls | Result |
|---|---|---|---|---|---|
|13|80|config autoaccept|7/7/3|7/7/3|Targeted auto effects/zero grants/valid mixed Deny PASS|
|14|121|CLI `--auto`, config Prompt|7/7/3|7/7/3|Same, actual CLI precedence PASS|

Both use `--build-oc false` after Cargo release. They have the same native binary
SHA and Rust-input hashes as the source-built01–11 captures; no build association
is invented. Commands are the report01 capture command with their new output path,
width, `--build-oc false` and `--permission-mode auto-config`/`auto-cli`.

## Final evidence and checks

- Authoritative detailed analysis: `permission-analysis20260927-05.json`.
- Compact counters, grant/context evidence, positions, cursor observations:
  `permission-summary20260927-04.json`.
- Independent targeted validation: `permission-validation20260927-03.json`
  and its runner log, **exit0**. It validates08/09 real Ask/Once/Reject/Always/
  restart/Deny, actual MCP call count, native child feedback in provider context,
 13/14 auto-mode actual denial and zero grants, plus native headless cases.
  It explicitly records **original child feedback absent from actual context**;
  no feedback-transport parity claim is made.
- Failed validator attempt remains `permission-validation20260927-failed01.md`.
  It counted a child title request as another lifecycle request; corrected
  validation filters title operations but includes them in total request counters.
- Original/native capture attempts01–14, raw VT/inputs/protocol/cells/PNGs/full
  comparator reports/locks/source manifests/build logs remain immutable.
- Final source manifest audit: all14 have one Rust-input hash
  `b4be80a060b770cd0e21ff9014e4dbff46fe92e186835aa9529d98de0567e055` and one native
  binary SHA `979e9aceebe3804b86a4bd485245a81f56dbccce22b42ba05d94edb5555b986f`.
- Capture/permission Node syntax, frontend integrity and `git diff --check` passed.
  Rust tests/acceptance gates were not run by this evidence-only agent.

```sh
node scripts/tui_capture/analyze_permission.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-analysis20260927-05.json
node scripts/tui_capture/summarize_permission.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-analysis20260927-05.json \
  evidence/tui/recovery-v00/permission-summary20260927-04.json
node scripts/tui_capture/check_permission_evidence.mjs evidence/tui/recovery-v00 \
  evidence/tui/recovery-v00/permission-validation20260927-03.json
```

Output paths refuse overwrite; use new names when reproducing.

## Final campaign counts / full comparators

- **433 local provider requests,432 completions,219 emitted ordinary-function
  calls,1 invalid fixture request**, including all failed attempts and native
  headless. Zero authenticated/live API requests.
- Styled grids: **238 DIFFERENT,4 INVALID,98 BLOCKED,0 EQUAL**.
- PNGs: **242 DIFFERENT,98 BLOCKED,0 EQUAL**.
- Cursor pairs: **153/272 equal**, independently measured, without masking.
- Four INVALID grids preserve original hidden cursor `(80,37)` in80-column
  MCP/child fullscreen+inline states, native `(78,37)`; comparator reports
  `Cursor outside grid`. No clamping or rewritten snapshots.
- Historical07 accounts for the one invalid request (native MCP wire name
  mismatch in the fixture);05 preserves the240s watchdog interruption.

## Priority product defects and unresolved work

Report01 has exact cells and keyboard paths. Immediate fixes:

1. Native inline/fullscreen approval geometry, `│` versus original `┃`, edit title
   position/color, option left padding and body height.
2. Pending patch generic row versus actual original bordered `Patching` card;
   tab `△`/color versus original `!`/color and missing add-tab control.
3. Root rejection exposes raw permission JSON; original has human rejection text.
4. Always-selected wording/title diverges; keep the narrower native grant truthful.
5. **80-column child feedback defect** in12: Ask→Esc→paste full feedback shows
   only `is correction.` at `(4,37)` next to inline hints. Native probe fails;
   original succeeds. Do not hide the defect with a suffix-only predicate.
6. Settings inventory/labels/filter-Esc behavior differs. Original actual
   Ctrl+P→Open settings succeeds in12; native palette path is still unqualified
   because that capture stops on the earlier feedback defect.

Still OPEN: complete action×width matrix, native admissible offline URL Ask,
typed CoreEvent ordering and transient owner-preview digest observations,
syscall-level read-only preview audit, atomic multi-resource grant rollback and
project/clone/worktree isolation, comprehensive feedback/focus/shortcut/mouse/
scroll behavior, other previews, original headless pair, release/acceptance and
meaningful unit/security gates. These are gaps, not Unsupported waivers.

Changes are only capture scripts and new evidence; owner's Rust dirty tree,
acceptance, progress, `.opencode`, auth files and Git delivery were untouched.
