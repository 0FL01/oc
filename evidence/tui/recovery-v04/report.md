# T44 recovery V04 — native modal delivery report

Recorded 2026-09-22. Base and final HEAD:
`1105f674e4284888e5675b0195b0df087d64b882` (uncommitted V04 work).
This report preserves failed attempts as well as final results. Existing V00–V03
reports, capture attempts, acceptance gates and planning state were not rewritten.

## Result

Implemented genuine shared `DialogFrame` / `SelectList` overlays in `oc-tui`.
Commands and Models are rendered above the existing session/home/prompt rather
than consuming transcript rows. Widths are 60/88/116, constrained to terminal
width minus two; top is floor(height/4). Native Cards uses 116 and informational
Help/DCP uses 88 to preserve existing bounded content. Backdrop black alpha150
composites the actual RGB foreground/background with nearest-integer rounding.

Ctrl+P opens a focused Commands search with sections, shortcuts and a registry of
real native actions. Ctrl+X m and slash model aliases open the same model action.
Model names and provider categories come from actual catalog DTOs. `Free` is
shown only when both input and output prices are known zero. Current model dot
and browse focus are separate. Search handles zero/one/many results; navigation
wraps and scrolls beyond the viewport. Enabled declared variants route through
the existing application selection and persistence. Explicit native default is
distinguished from an untouched current variant.

Printable routing now uses `intersects(CONTROL | ALT)`; release events are ignored
and repeated submit/cancel/control events do not execute actions again. Escape
dismisses a modal while retaining editor draft, attached session and effective
model. Search consumes its own text/paste. Ctrl+C clears/dismisses searchable
selectors; native informational operation panels preserve their previous quit /
shutdown control. Existing permission checks and busy-action refusals remain in
the typed application owner.

**Functional slice delivered; full parity and VIS08–VIS12 closure are not claimed.**
The complete command inventory and action owner/status/boundary mapping is
[capabilities.md](capabilities.md). Unsupported original controls are not fake
selectable actions. Full-parity claims require owner resolution of the listed
scope conflicts and completion of the in-scope UI gaps.

## Qualification

### Actual binary, normal configuration and provider effects

`crates/oc/tests/pty_t39.rs::v04_raw_dialogs_preserve_draft_and_select_normal_provider_model_variant`
launches the actual `oc tui` binary under a real PTY with isolated HOME/config and
a loopback Responses fixture. It sends raw Ctrl+P, text, Enter, Ctrl+C, End,
Escape, Ctrl+X m, arrow sequences and `/model`; it does not call TuiState directly.
Thirty additional model entries are admitted through normal native configuration.
Assertions cover no-result Enter, filtering, scrolling to model29, draft/tab/model
preservation on Escape, no browsing-induced provider request, selected exact model
ID, `fast` → reasoning effort `high`, native default → enabled `none`, actual next
request prompt, durable user message, responsive Commands while streaming, clean
exit and terminal restoration. This is real native wire execution against a
controlled peer, not a claim of new live OpenProxy qualification.

V01 real-PTY MCP-stalled-acceptance tests additionally open Commands, search to
zero results and dismiss it **before the fake initialize is released**. Original
edit/resize, duplicate Enter suppression, cancellation/reap, retry and shutdown
assertions remain. Targeted run measured cancel/reap at approximately 97ms for
manual compression and 72ms for a normal pending turn, within the existing 2s
gate. No timeout/gate was increased.

Unit tests cover 30-model filtered selection, draft Unicode, unchanged underlay
symbols, dimmed fg/bg, current model stability, sizes, alpha rounding, known-price
handling and enabled variant filtering. Existing native session/agent/skill/card,
Location, DCP, storage, subagent and permission tests pass.

### Actual paired visual evidence

Pinned original source: `2670273ff17da96f85c5826ced57aa1b368754fa` in external
`/home/opencode/.cache/opencode-tmp/oc-v2-src`.
Original executable:
`/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`.
Existing V00/V03 `scripts/tui_capture` runner was used, with external
xterm/Chromium tooling; no Node/JS production dependency was added.

The exact command shape for each attempt was:

```sh
node scripts/tui_capture/capture.mjs \
  --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode \
  --oc /home/opencode/ai/oc/target/debug/oc --build-oc true --sample short \
  --output /home/opencode/ai/oc/evidence/tui/recovery-v04/ATTEMPT
```

| Attempt | Exit | Observed result |
|---|---:|---|
| `attempt-01` | 1 | Both actual routes EXECUTED, provider contract true; all six grid/PNG comparisons DIFFERENT. Found excess option padding, search background error, and floor alpha rounding. Retained unchanged. |
| `attempt-02` | 1 | Same six unequal comparisons after modal corrections. Inspection also exposed inherited upstream tabs: external runner stores state under `runs/<output-basename>`, and generic attempt names collide with earlier campaigns. Retained unchanged; not the clean-reference evidence. |
| `v04-03-clean` | 1 | Distinct external run directory, original single tab. Both routes EXECUTED with provider contract true; Session/Commands/Models PNG and styled-cell captures all produced. All six comparisons still DIFFERENT. This is the clean paired modal evidence. |

Each directory contains `capture.lock.json`, commands/provenance, frontend profile,
both original/native captures and comparator outputs. The comparator ran **after**
the real capture in each attempt. No unequal result was relabeled as parity.

Clean evidence links:
- [Original Commands PNG](v04-03-clean/upstream/commands-over-session.png), [native Commands PNG](v04-03-clean/oc/commands-over-session.png)
- [Original Models PNG](v04-03-clean/upstream/models-over-session.png), [native Models PNG](v04-03-clean/oc/models-over-session.png)
- Corresponding `*.cells.json` and `*.txt` beside each PNG are the actual terminal captures.

At 160×48 the measured medium modal starts at (50,12), title (54,13), Search
(54,15), options/categories at y17. Corrected Commands first selected text is
(54,18), matching the original. Actual title fg/bg `#eeeeee/#141414`, Search
`#808080/#141414`, section `#9d7cd8/#141414` bold, selected title
`#0a0a0a/#fab283` bold and selected shortcut not bold agree at these sampled cells.

Read-only `inspect_capture.py` validates backdrop rounding from captured cells.
For clean original/native Commands and Models it found **zero nearest-rounding
mismatches**, respectively 6587/6588 inspected RGB fields per frame. Blank-cell
foreground is excluded from that diagnostic because the original renderer leaves
default blank fg untouched; all applicable backgrounds are included. This is a
local alpha observation, not a whole-grid parity score. Native unit tests also
verify fg dimming on styled blank cells. Original default blank fg, cursor color,
different available commands, provider heading/integration footer, agent/footer
and root-tab differences still make full captures unequal.

The clean capture binary SHA256 is
`164cfede85ed836e568636e481ab24b18ff326eb3771741c75192c9b81541261`.
Subsequent native regression fixes widened Cards/Help/DCP and retained skill IDs;
they did not change the medium Commands/Models layout. No claim is made that
the final binary hash is the captured binary hash. Final build SHA256:
`0821589b503b99211a43e82d33f352eff0fe7bf89feed6a5703b326f069efa40`.
The runner's dirty Git diff digest excludes untracked files, so the final new
`crates/oc-tui/src/dialog.rs` SHA256 is recorded separately:
`91f843966ba8167e9c99cec3302f06d3a1efeabb732e65c732182277f2af7082`.

## Execution history — failures retained

Commands below all used `--locked`. `fmt` preceding a chained command passed
unless otherwise stated; a failed first command prevented the following `&&`
command from executing. No test was ignored to hide a failure.

| Order | Command / phase | Exit and observation |
|---:|---|---|
| 1 | `cargo test -p oc-tui dialog_routing_rejects_modifier_text_and_release` before fixes, chained with overlay test | 101: Ctrl+Z became printable `z`; chained overlay test not run |
| 2 | `cargo test -p oc-tui model_is_overlay_with_focused_search` before fixes | 101: inline panel at y27, expected real modal title near y11 |
| 3 | `cargo test -p oc-tui --lib`, first implementation | 101 compile: unresolved `unicode_width`; corrected to existing `Line::width`, no dependency added |
| 4 | Same lib suite | 101: 96 passed, 6 failed. Modal focus correctly trapped subsequent slash text; old inline-title and undimmed-underlay assertions needed explicit modal lifecycle setup |
| 5 | `cargo fmt --all && cargo test -p oc-tui --lib` | 0: 104 passed |
| 6 | `cargo test -p oc --test pty_t39 v04_raw_dialogs -- --nocapture` | 101: test incorrectly expected no reasoning for native default with only a `fast` variant |
| 7 | Same raw test, next attempt | 101: driver sent Ctrl+X m before standalone Escape was observably dismissed; query became `ModalmModal 29`. Corrected synchronization to actual modal disappearance |
| 8 | Same raw test | 101: native-default assumption still failed. Read `models::select_variant`: default selects enabled `none`/standard/custom. Fixture now declares distinct `none` and `fast`; assertions verify actual resolution |
| 9 | Same raw test | 0: actual model, reasoning variant, draft, pending responsiveness and terminal checks passed |
| 10 | `cargo test -p oc --test pty_t39 -- --nocapture` | 101: 1 passed, 3 failed. Old inline title markers plus `/cards` alias surviving load produced a wrong subsequent command. Fixed alias consumption for Cards/Help and used actual modal titles |
| 11 | Same PTY suite | 101: 3 passed, 1 failed on skill ID marker after initial name-only rendering |
| 12 | Same PTY suite, then oc-tui lib suite | 0: 4 PTY + 104 lib passed. Final Skills also preserves ID plus real name to keep existing Location qualification intact |
| 13 | `cargo test -p oc-tui constrained_sizes_and_rgb_alpha` after pinning captured nearest-rounding expectation, before correction | 101: got RGB(105,52,4), expected RGB(105,53,4). Corrected alpha arithmetic |
| 14 | `cargo clippy --workspace --all-targets -- -D warnings` | 0 |
| 15 | `cargo test --workspace` | 101: V01 manual compression pending/shutdown timed out because generic modal Ctrl+C dismissal replaced its existing quit control. Restored informational operation-panel shutdown behavior |
| 16 | `cargo test -p oc --test mcp_application v01_ -- --nocapture`, then `pty_t39` | 0: 5 V01 + 4 PTY passed, including added stalled-acceptance Commands checks |
| 17 | `cargo test --workspace` | 101: T42 card diff totals clipped by medium width; skill IDs absent. Fixed native Cards to 116-column scaffold and retained skill ID/name. T42 assertions were not weakened |
| 18 | `cargo test -p oc --test pty_t42 -- --nocapture` | 0: 3 passed |
| 19 | Following `cargo test --workspace` | 101: V02 expected `Model · Provider` on one picker line. Updated to exact model title/provider category DTO assertion plus rendered names; price assertion retained |
| 20 | `cargo test --workspace --no-fail-fast` | 101 compile: passed owned Strings to `contains`; corrected borrows in the new V02 assertion |
| 21 | Same no-fail-fast workspace suite | 101: all other targets passed, one T42 Location race. Driver matched `slow stream` in the pre-acceptance editor, then typed `/location` into that pending draft. Added synchronization on the second actual provider request, preserving the streaming-refusal assertion and timeout |
| 22 | `cargo fmt --all -- --check`, clippy all targets, locked build, `target/debug/oc --help`, targeted T42 | All 0; T42 3 passed |
| 23 | Final `cargo test --workspace --no-fail-fast` | **0: 449 passed, 4 pre-existing live/explicit-server ignores, 0 failures**; no new ignores |

Visual runner/comparator exits are separately listed above (three runner exits 1,
six unequal comparator exits 1 per attempt). `inspect_capture.py` executions all
exited 0. Git status/diff/rev-parse/diff-check and hash commands exited 0. Tool-only
exploration failures: initial wrong upstream subpath returned no files; several
atomic patch context/order mismatches were rejected without edits; searching a
minified theme asset exceeded the grep record limit and was replaced by targeted
Rust/source inspection. Those tool errors have no process exit code and did not
become passing evidence.

## Review, limits and handoff

Reviewed product diffs for key routing, per-modal search state, alias consumption,
real typed intents, effective-versus-pending model state, RGB composition and
underlay layout. Existing tests now explicitly dismiss modal overlays before
examining base chrome colors; no color baseline/gate was altered. AUD29 uses the
real model search instead of assuming the old inline picker cursor position;
its provider/session/agent/DCP assertions remain.

Relevant final checks: workspace tests 449/4, fmt check, workspace all-target
clippy with `-D warnings`, locked build, `oc --help` and `git diff --check` passed.
The four crates/application boundary, permissions, no-JS production constraint,
T43/T45 ownership and historical evidence remain intact. Native autoaccept remains
unsupported (V02); service/attach remains outside GOAL.

Remaining V04 parity gaps include missing original commands (see capability map),
upstream fuzzysort ranking versus token-substring matching, recent/favorite model
groups, integration footer, mouse interaction, exact original variant subdialog,
cursor color and default blank-cell styling. Complete multiline editor/keymap is
V05 and was not implemented here. V04 does not close the broader parity gate.

Files owned by this slice: `crates/oc-tui/src/{app,commands,dialog,events,lib,picker,shell,views}.rs`,
`crates/oc/src/tui_cmd.rs`, tests `mcp_application.rs`, `pty_t39.rs`, `pty_t42.rs`,
`recovery_v02.rs`, and this evidence directory. During execution `opencode.json`
acquired an unrelated one-line addition; this agent did not edit or revert it.
The user ZIP remains untouched. No commits, pushes, checkpoints or planning edits
were made. Parent owns review/delivery and any owner amendment request. Exact next
step: review this slice and the unsupported-command map, then continue the ordered
V05 editor work without claiming whole-frame parity from these unequal captures.

## Appended final review correction — modal glyph clearing

After the above report was recorded, final renderer review identified that
Ratatui `Block` paints style but does not erase existing symbols. The added
`dialog::tests::modal_surface_erases_underlay_symbols` first failed (command
`cargo test --locked -p oc-tui modal_surface_erases_underlay_symbols`, exit101):
an `X` remained at (10,6) inside a supposedly opaque modal. `DialogFrame::paint`
now calls `Clear` on **only the modal rectangle**, after dimming the underlay and
before painting the dialog surface. The test paints every original screen cell
with `X`, requires every covered symbol to become blank, and preserves an
uncovered `X`; this is not inferred from a modal over empty transcript rows.

Post-correction checks, all exit0:
- `cargo fmt --all`, then `cargo test --locked -p oc-tui --lib`: **106 passed**.
- `cargo test --locked -p oc --test pty_t39 --test pty_t42`: **4 + 3 passed**.
- `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- `cargo build --locked`, `cargo fmt --all -- --check`, `git diff --check`.

The earlier whole-workspace run remains a factual 449-pass/4-ignore result before
this one-line rendering correction; the new 106-test TUI run includes the added
regression. It is not relabeled as a later 450-test whole-workspace invocation.

One directly relevant paired follow-up used the identical capture command above
with `--sample rows` and output `v04-04-final-dense`. Both actual routes executed
the 90-row provider sample with provider contract true, then captured real
Commands/Models. Runner exit1; all six subsequent grid/PNG comparisons remained
DIFFERENT. This final exact-build evidence supersedes earlier binary provenance:

- [Original Commands](v04-04-final-dense/upstream/commands-over-session.png) / [native Commands](v04-04-final-dense/oc/commands-over-session.png).
- [Original Models](v04-04-final-dense/upstream/models-over-session.png) / [native Models](v04-04-final-dense/oc/models-over-session.png).
- [Final capture lock](v04-04-final-dense/capture.lock.json); actual styled cells
  and text remain next to each PNG. Read-only inspection exited0 and found zero
  nearest-rounding differences in 6794 applicable underlay RGB fields for each
  original/native modal.
- Final built **and captured** binary SHA256:
  `8c48fe6395f7ef7df3011b81834cc18a8247b50053713ddcef1d826ab28bf2a0`.
- Final new `dialog.rs` SHA256:
  `74cd010ed4e867ad470459d226f498e9d89788b6e1308e11d142f70ae5d65050`.

Earlier evidence and failures are retained unchanged. This follow-up fixes native
modal opacity; it does not remove the capability or pixel-parity gaps above.
