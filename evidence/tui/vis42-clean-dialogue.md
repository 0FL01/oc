# T44 — clean-dialogue owner slice (2026-10-07)

Status: **FUNCTIONAL_SLICE_QUALIFIED; PAIRED_FRAMES_DIFFERENT; T44 ACTIVE**.
Source: frozen VIS42/VIS17 amendment and test plan; owner explicitly resumed
the full T44 contract. This receipt is not whole VIS42, V09 or T44 completion.
Git base: `e0509c4cde2314d5d0d2a0cb501297511ab1da61` (code base `75568ed2a`).

## Implemented owner behavior

- Removed only nonfatal background TUI startup/config/plugin/provider/MCP rows
  and startup stderr duplication. Fatal/startup boundaries, headless diagnostics,
  unavailable-request refusal, real tool/provider failures and turn-specific
  warnings stay available. RAW messages, stored warnings and provider context
  are not rewritten.
- `app/services.rs` reconciles existing current chrome/MCP facts. Bounded status
  includes actual issue/omitted/pending counts; one brief aggregate notification
  changes only for a new cause or failure after recovery. Pending is not recovery
  and retains only its current source's prior failure baseline. Repeated refresh,
  unchanged reload, republished epochs and parked projections do not re-alert.
  Unrelated operation feedback survives recovery; no lifetime registry/poll.
- MCP control ID and diagnostic name are distinct typed owner identities.
  Location, instance, same-instance generation and revision guards remain;
  unpublished revision zero is not an empty recovered inventory. Public Location
  epoch is not the runtime MCP generation clock.
- The existing reload job coalesces retiring-scope hints. Both successful and
  refused/incomplete reload reconcile one current MCP snapshot afterward.
  Refusal stays loud; failed queries retain known facts, not fabricated health.
  `load_tab` also queries the current owner rather than silently losing MCP.
- Existing `TurnReport`/terminal `CoreEvent` carry an index range identifying
  the MCP-owned subsequence of the unchanged warning vector. Budget and later
  title warnings are outside it. UI routes by provenance, not prose matching;
  invalid ranges preserve visibility, identical service/turn text stays distinct,
  and foreign terminal events cannot write warnings or close the active turn.
- Compact Settings rows preserve full safe owner detail on explicit Enter and
  typed copy/investigation. Saved unavailable choices have their own read-only
  detail route. MCP details expose existing stage/code/source/field/retryability
  and full safe diagnostic copy/investigation without editing/submitting drafts.
- The actual live-part eviction flag now produces one `Preview limited` viewing
  hint outside answer text, only while the affected turn is active. Removed only
  the generated long live-preview answer row; caps and durable access remain.
  Other generated tool-preview rows are the next VIS16/VIS17 seam, not fixed here.
- At narrow widths, new service hints cannot clip the actual interrupt/retry
  footer: ancillary shortcut/usage hints yield first. Healthy frames are unchanged.

## Reproductions and retained invariants

Initial VIS42 owner tests were RED: MCP produced a warning conversation row and
catalog feedback lacked the required brief/status separation. Four cohesive
private owner tests now cover delivery/cause/pending/recovery/parked/stale/focus,
safe details, equal-text warning provenance and foreign-event rejection.

The real `pty_t39::vis42_native_mixed_pending_failures_reload_recovery_and_reopen_keep_dialogue_clean`
scenario uses owned slow discovery/MCP, an unsupported plugin and ignored legacy
compaction. It proves pending pre-effect refusal, two real native answers, no
background dialogue rows, unchanged-reload suppression, safe details, resize,
healthy recovery, reaped process and reopen with unchanged request/history counts.

Existing expectations requiring unsolicited technical rows moved to actual
Settings/MCP details. All headless/fatal/privacy/no-effect/admission/catalog caps,
quarantine, grants, request counts and no-replay assertions remain. Strict tab
mocks acknowledge the newly required MCP query; its existing negative remains
fatal rather than being weakened to an optional empty snapshot.

Adding service status reproduced an animated 80-column footer regression:
`esc again to interrup` lost its last character. The existing footer test and
whole real Go account/held-turn/double-Escape case qualify the layout fix; no
cancel guard, timer or test threshold changed.

## Current checks

Approved TMPDIR, build jobs 3, test threads 2, serial Cargo, normal stacks:

- `cargo fmt --all -- --check` — PASS.
- `cargo clippy --locked --workspace --all-targets -- -D warnings` — PASS.
- `cargo test --locked --workspace --no-fail-fast` — **1720/0/11**, independently
  summed from 46 workspace result records; 11 unchanged opt-in ignores.
  Includes binary 97, adapter library 668/0/1, TUI 450, MCP application 41,
  PTY T39 52, recovery startup 4, runtime 120 and subagents 39.
- `cargo build --locked` / `cargo build --locked --release` — PASS.
  Current log: approved cache `t44-clean-ui-capture004-final.log`, marker
  `T44_CLEAN_UI_SCOPE_FINAL_RUST_PASS` after both builds. It includes the final
  foreign-event and refused-reload fixes, not merely the earlier green source.
- Actual release `support/startup.py` and `support/discovery_startup.py` — PASS:
  safe/read-only TUI details, fatal/headless nonzero and empty stdout, one discovery
  GET/no Responses, trust/lock/storage/switch categories and terminal restoration.
- Current Node syntax/Python compile checks, 47 progress/script unit tests,
  documentation/journal structure and diff checks — PASS. Structure is not parity.

## Actual running paired evidence

Pinned original source `2670273ff17da96f85c5826ced57aa1b368754fa`, executable SHA
`2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
Existing xterm/Chromium frontend and geometry checks passed. Both sides use the
same Reader/tools/hidden-sidebar 120×40 profile and identical real pasted prompt.
Original Node/JS exists only in this dev/reference fixture, not native runtime.

`vis42-clean-dialogue-attempt-005` records **11 actual stages per side**:
pending Home, first answer while initialization is pending, brief failure,
completed answer, failed list/details/back, unchanged reload, healthy list/back
answer and second answer. Both probes are `OBSERVED`, not pixel PASS.

- Each side: five valid synthetic model requests (four primary read/result
  requests plus one title), zero invalid requests. Real model API calls: zero.
- Each side: three real initializer owners, outcomes failed / failed / healthy;
  healthy notifications/catalog recorded. All six owned peer PIDs reaped.
- **22 full rendered frames** have acknowledged real process-group pause,
  matching before/after complete-grid signature and stable geometry. Every pause
  resumes in `finally`; no closest frame, masks, crops or fake component painting.
- Native unchanged-cause captured alert is absent. Original's captured alert is
  also absent in this attempt; its pending-reset can re-alert, not a guaranteed
  toast timing. Do not invent an original notification to satisfy a comparison.
- **22 strict whole-grid/PNG comparisons are DIFFERENT**, runner exit 1.
  Actual elapsed digits, native compact status/safe wording/identities and detail
  layout remain visible. Cursor is recorded; this is not temporal blink parity.

Failed attempts 001–004 remain immutable: initial timeout schema was numeric
instead of the donor object; later callback/reload observations identified
ephemeral success-toast races. Attempt 004 correctly exposed `Code: deadline`:
waiting for unrelated clocks to settle consumed the initializer's unchanged
ten-second bound. Attempt 005 observes the first actual stage, then applies the
stricter acknowledged grid freeze; timestamps prove controlled RPC failure.
No timeout, validation or comparator was weakened to turn a failed attempt green.

## Remaining T44 work

Complete remaining VIS42 concise submitted-refusal and styled/detail integration,
then the ordered structured tool-body/guidance/capture facts and Generic/MCP
expand/collapse VIS16/VIS17 slice. Full-frame/temporal differences and every other
mandatory VIS01–VIS45/SAFETY/R6/V09 outcome remain open until directly qualified.
No T44 finish, native-golden or whole-product READY claim. AUTH06 stays deferred;
original OpenProxy 24/24 and Go 13/24 campaigns, user config and `.opencode/` untouched.
