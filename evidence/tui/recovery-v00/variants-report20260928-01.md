# T44 VIS09 / VIS29 — current canonical presentation qualification

## Result and frozen association

**PASS: VIS09 and VIS29 under the current canonical-effort amendment.**
Canonical supplied-order presentation is exact at **80×24, 120×40 and 160×48**:
**105/105 full-frame pairs**, **504,000 styled cells**, **zero unequal cells,
zero differing raster pixels and zero cursor differences**. Cursor comparison
includes position, visibility, shape **and actual terminal cursor color**.

The identical unsorted fixture is independently captured and fully compared:
**33/105 equal full-frame pairs**, **2,305 unequal cells**, **145,866 differing
pixels**, zero cursor differences. Its 72 differing pairs are the states affected
by native canonical rank versus original declaration order; all 33 independent
model/search/draft/default controls remain exact. This is the approved ordering
difference, **not** a universal donor pixel PASS.

- Git base/HEAD: `9d2af6c98a47a5ab94e0cbbf7e898e4c3309d0ca`.
- Qualification is on the reviewed uncommitted production/test diff, not bare HEAD.
- Current native debug: `6313f913a088385fa059d5acdd1f80ad884a973d6e7b47d193f2ac159ffbbe79`.
- Current native release: `ac4ea89479f8265d6b056f3ea719d5ded099202a96ef7b5db75127e945f9e3b6`.
- Original executable: `/home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode`,
  SHA256 `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`,
  pinned original commit `2670273ff17da96f85c5826ced57aa1b368754fa`.
- Final canonical capture: [`variants-paired20260928-07/`](variants-paired20260928-07/).
- Final unsorted capture: [`variants-paired20260928-08/`](variants-paired20260928-08/).
- Final canonical audit: [`variants-check20260928-11.json`](variants-check20260928-11.json).
- Final unsorted audit: [`variants-check20260928-12.json`](variants-check20260928-12.json),
  with every cell difference and full PNG difference in
  [`variants-check20260928-12/`](variants-check20260928-12/).
- Each final audit verifies every current tracked native source against the capture
  lock and verifies the current debug executable. Its `source_patch` is the exact
  restricted native Git diff, also retained as `native-source.patch` inside its
  fresh audit directory. Capture locks retain before/after source association,
  original source-owner/helper hashes, fixture hash, pinned frontend versions,
  exact argv, binary digests and every captured artifact hash.

## Narrow fixes and owners

| Path | Change |
| --- | --- |
| `crates/oc-tui/src/app/input.rs` | Center the actual current variant on opening the picker and restoring its unfiltered current choice. Uses the existing SelectList owner, including its clamped viewport. |
| `crates/oc-tui/src/views.rs` | Expose the actual searchable-dialog caret paint from the semantic focused form-field theme token. |
| `crates/oc-tui/src/terminal.rs` | Emit terminal OSC cursor color/reset; restore default cursor color during normal/error/panic teardown. |
| `crates/oc/src/tui_cmd.rs` | Apply that cursor color only when the paint changes, before the real frame draw. |
| `crates/oc-tui/src/app/tests/input.rs` | New narrow current-variant centering/clear/draft regression; existing VIS09 regression now also checks modal cursor transport and reset. |

`models::ordered_variants`/`available_variants`, effective merge and snapshots,
exact-ID selection, provider serialization and immutable-history ownership remain
the shared T47 implementation. No presentation-side order calculation was added.

New bounded helpers: `scripts/tui_capture/variants_capture.mjs`,
`variants_bridge.py` and `variants_audit.py`. They run the real two executables;
native persistence/history observation is read-only, not fixture ingress.

## Per-obligation qualification

| Obligation | Status | Evidence |
| --- | --- | --- |
| VIS09 real metadata/current-dot versus first focus | PASS | Six final real PTY launches per order; `model-initial`/`model-reopened`. Sixteen supplied DTO/config models have real names, context/output limits and costs; zero-cost models sort Free-first in both binaries. Current initially model 03, focus initially model 00. Exact chosen IDs are read from durable native scoped selections and original preferences. |
| VIS09 keyboard scroll beyond eight rows | PASS | Thirteen real Down events, `model-scrolled`, at all three sizes; full grid/style/raster comparison. Existing nearest VIS09 test also covers keyboard centering and independent mouse scroll. |
| VIS09 no-match/filter/clear/search cursor | PASS | `model-no-match`, real Ctrl+C query clear, `model-filtered` (`Model 15`); full cursor including color and complete PNG/grid equality. |
| VIS09 actual model → variant flow | PASS | Enter chooses model 15 and opens its actual variant picker, `model-selected-variant`; choosing Default returns to the original retained draft, `model-selected`/`draft-restored`. Original recent-model persistence and native exact scoped model record are checked. |
| VIS09 named alias choice/current/reopen | PASS | `variant-filtered` (`fast`), Enter, `variant-selected`, `variant-reopened`; exact `fast` persists, not an effort-derived replacement ID. |
| VIS29 complete Ctrl+T round trip | PASS | Every size executes eleven real Ctrl+T events: named none, minimal, fast, low, medium, high, xhigh, max, zeta, alpha, then no-overlay Default. `cycle-0`…`cycle-10` plus a real picker reopen after every step (`cycle-picker-0`…`cycle-picker-10`). Every exact selection is checked in durable owner/client records. |
| Shared canonical availability, ties/custom source order, no effort synthesis | PASS | Canonical fixture and persisted cycle agree with the existing shared owner. `fast` declares low; `low` has no effort and remains separate. `zeta` precedes `alpha`, contradicting lexical sorting. The directed T47/VAR01 evidence below covers disabled/reserved/missing/subsets/nullable/name conflicts/case/whitespace/unknowns. |
| Draft/model/focus retained and selection generates no effects | PASS | All pre-submission owner snapshots show zero provider requests, empty conversation/turn/acceptance/tool-operation tables and unchanged empty project files. Native scoped agent/model IDs are checked. Full normal/modal grids show the retained draft and correct focus. |
| Exact next real provider request | PASS | Each final native launch explicitly admits and sends exactly one loopback Responses request after all display/selection captures: `model=vis09-model-15`, `reasoning.effort=low`, corresponding to the exact selected `fast` ID. Response completes into an actual completed stored turn. Three requests per campaign, six across the two final campaigns; zero shortcut/picker requests. |
| Original generation rejection/counter and public transfer | PASS | Original bridge rejects generation; each original run has zero POST/GET provider requests. Each imported empty named session is exported through the original public CLI and title/agent/model/tokens/cost/location/messages are exactly checked. |
| Canonical full styled-cell/PNG/cursor qualification | PASS | 35 states × three sizes; final audit 11, 105/105 exact full-frame pairs, 504,000 cells and no differing pixels/cursors. No masks, row rewriting, normalization, screenshot-derived oracle or native-render replacement for the original. |
| Identical unsorted data, honest ordering difference | PASS | Final audit 12 retains all 105 comparisons and all differences. The 24 order-affected state labels per size differ, while all eleven independent control states per size remain fully equal. Both real persisted cycles follow their respective native canonical/original declared orders. |
| Missing/all-disabled/stale/retired/refresh/restart/Home/read-only/busy and other wire choices | PASS — directed shared evidence | T47 canonical-effort evidence and current workspace regressions, detailed below. This presentation work does not duplicate the backend matrix. |
| Native/original clean terminal exit | PASS | Twelve final launches (six canonical, six unsorted) exit naturally with code 0 and exactly restored termios; no forced stop in either qualified campaign. Current workspace PTY restoration/panic/error tests also pass. |
| Current affected/full workspace integration and build | PASS | 1,203 passed, zero failed, nine pre-existing opt-in ignored; fmt/clippy/debug/release/help gates below. |
| Remaining full T44/V09 and complete GOAL A01–A13 readiness | NOT_RUN by this atomic slice | Parent-owned continuation described below; no READY claim. |

### Canonical fixture and declared difference

The fixture is an independently supplied literal public config, not an order
obtained by running Rust. Its effective model/variant values are the same in both
binaries. The original public configuration uses its required `variants` array;
native uses its admitted ordered variants object. The bridge changes that public
representation without changing values or source order.

Canonical named order is:

```text
none minimal fast(low) low(no effort) medium high xhigh max zeta(" deep ") alpha(custom)
```

The separate unsorted named declaration supplied to **both** binaries is:

```text
max xhigh zeta fast high alpha medium low minimal none
```

Native ranks the same effective records; the original retains that declaration.
Default is the separate no-overlay choice on both sides. Custom efforts and alias
IDs are not trimmed, renamed, synthesized or deduplicated.

### Equivalent public session/settings and capture provenance

Native launches `oc tui --session vis09-presentation` in the isolated real project
and performs the real public `/rename VIS09 Presentation` action on a genuinely
empty owner session. Read-only SQLite observation verifies the real title,
creation/update values and absence of conversation rows. The **reference-only**
transfer maps that empty named session via original `session import/export
--standalone`. Zero usage/cost scaffolding represents the actual empty session;
native stored seconds map to original created milliseconds. The original import
owns its real new update time. There are no invented message/event timestamps,
native snapshot/history injection flags or native database/preference writes.

Both sides use the same real cwd, supplied model/data, dark `opencode` theme,
animations off, sidebar hidden, TPS off, horizontal tabs, block non-blinking cursor,
notifications/sound/devtools off, and the same explicit public CLI binding
`"model.dialog.provider": "none"`. This disables the original's out-of-goal
provider-integration/config-authoring shortcut through its real supported config;
native already exposes no such action. Pinned `DialogSelect` hides unbound action
labels itself. **No footer is rewritten or masked**, and no inert integration
menu was added. This report claims exact presentation for that explicitly supplied
configuration, not equality of the original's default integration-authoring UI.
The earlier default-binding difference is retained honestly in attempt 04.

Every binary child uses a constructed isolated HOME/XDG/process environment;
inherited authoring configuration is not copied. The existing pinned Chromium,
Playwright, xterm and Unicode 11/font frontend is used unchanged, with exact
versions recorded in `capture.lock.json` and the pinned tooling lock copied into
each attempt. Full screenshots cover the entire terminal screen. Full grids retain
symbols, resolved foreground/background, width/continuation cells and modifiers.
Grid/cursor state is stable across each screenshot. `.vt` captures and `raw.vt`
retain original terminal output; `inputs.json` and `protocol.json` retain actual
keys, request counters, read-only owner state, transfer checks and natural exits.

### Directed shared T47/VAR01 behavior

Reuse [`evidence/T47/canonical-effort.md`](../../T47/canonical-effort.md) and
[`evidence/T47/report.md`](../../T47/report.md), implementation `6cf1cdc859`:

- All 5,040 known-rank permutations and 128 supported subsets; explicit known
  effort before name, absent/null name ranking, unknown/case/whitespace custom,
  stable aliases/customs, missing/disabled/exact reserved-default exclusion.
- Real Home/session cycle and independent picker indexing, exact draft/model,
  zero selection requests, busy guard and genuinely enabled read-only child.
- Exact alias low wire; name-ranked low and Default omit overlay; named none
  sends literal none; custom `" deep "` remains exact; base request settings survive.
- Actual discovery/local merge/reload reorders `fast` without losing identity;
  picker/session reopen, process restart and persistence retain exact `fast`.
- Successful retirement/disable yields actionable unavailable choice and zero
  requests, not silent Default; explicit stale-cycle recovers Default; empty and
  all-disabled/reserved-only no-ops. Retained-generation guards stay authoritative.

All existing owner/config/discovery/provider/selection/VAR01 tests run again in
the current full workspace gate. This slice adds current real-binary presentation
and one exact next-request probe rather than another ordering/merge implementation.

## Commands and observed exits

Working directory `/home/opencode/ai/oc`; dedicated uid 1003. Before heavy checks:
approximately 7.5 GiB available RAM, 177 GiB free disk. One Cargo/PTY coordinator;
heavy timeout 900000 ms. Cargo environment for every invocation:

```sh
CARGO_BUILD_JOBS=3 RUST_TEST_THREADS=1 \
TMPDIR=/home/opencode/.cache/opencode-tmp/opencode/oc-test-bench-20260924
```

All final commands below exited **0**. Logs are in this report's directory.

| Command | Result / log |
| --- | --- |
| `cargo test --locked -p oc-tui vis` | 96 PASS, zero failures; `variants-targeted20260928-03.log`. Includes both affected VIS09/VIS29 cases and nearest view regressions. |
| `cargo fmt --all -- --check` | PASS; `variants-fmt20260928-01.log`. |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | PASS; `variants-clippy20260928-01.log`. |
| `cargo test --locked --workspace` | **1,203 PASS / 0 failed / 9 existing opt-in ignored**; `variants-workspace20260928-01.log`. Includes 417 TUI unit cases and 89 PTY cases (16 basic, 39 T39, 34 T42), owner/discovery/provider/persistence/recovery/DCP/effects/workspace/MCP tests and doc-test targets. |
| `cargo build --locked` + `target/debug/oc --help` | PASS; `variants-debug20260928-01.log`, `variants-debug-help20260928-01.log`. Debug digest remains the qualified capture digest. |
| `cargo build` | PASS; `variants-unlocked20260928-01.log`; same qualified debug digest. |
| `cargo build --locked --release` + `target/release/oc --help` | PASS; `variants-release20260928-01.log`, `variants-release-help20260928-01.log`; current release digest above. |
| `git diff --check` + restricted production/test diff review | PASS; no whitespace failure; five reviewed native paths listed above. |
| `node scripts/tui_capture/variants_capture.mjs --oc /home/opencode/ai/oc/target/debug/oc --native-sha256 6313f913a088385fa059d5acdd1f80ad884a973d6e7b47d193f2ac159ffbbe79 --reference /home/opencode/.cache/opencode-tmp/opencode/t44-reference/package/bin/opencode --output /home/opencode/ai/oc/evidence/tui/recovery-v00/variants-paired20260928-07` | Six naturally restored launches; 210 captures; actual native wire probes completed, original zero requests. |
| Same capture command with output `variants-paired20260928-08` and `--order unsorted` | Six naturally restored launches; 210 captures; independent declared/canonical persisted cycles and completed native wire probes. |
| `python3 scripts/tui_capture/variants_audit.py evidence/tui/recovery-v00/variants-paired20260928-07 evidence/tui/recovery-v00/variants-check20260928-11.json` | PASS_CANONICAL_FULL_PRESENTATION, 105/105 exact pairs; current source/debug and full artifact hashes verified. |
| Same audit for `variants-paired20260928-08` → `variants-check20260928-12.json` | PASS_UNSORTED_ACTIONS_WITH_ORDER_DIFFERENCES; 33 equal independent controls, all 72 order-affected comparisons retained, current source/debug verified. |

No authenticated/paid/live API was used. Provider traffic is bounded loopback only.
Bridge limits live raw output to 16 MiB per launch; the eight immutable attempt
directories together occupied approximately **789 MiB**, including all full grids,
PNGs and repeated raw VT. Final `du -chs .../variants-*` reports **795 MiB total**
including all audits/differences/build logs/report, below the 1 GiB total constraint.

### Failed/intermediate immutable attempts

- **01:** helper's cycle predicate omitted the real provider label in prompt
  metadata. Both actual displays had already cycled correctly; raw failures saved.
- **02:** helper incorrectly expected a `tui.selection.model` preference key;
  real owner is scoped `tui.selection.session` plus per-model variant preference.
- **03:** helper attempted Ctrl+U for modal query clearing; native does not bind
  it there. Final campaign uses the real shared Ctrl+C clear action. Raw failed
  no-match query is retained; no product keymap was weakened or baseline changed.
- **04:** complete actions/wire succeeded; strict 120×40 comparison failed on
  default reference integration footer and white native modal caret. This is not
  promoted to PASS. Its full cell/PNG difference records remain saved.
- **05:** explicit equivalent shortcut settings fixed the footer difference;
  modal cursor still used the wrong missing surface-only theme token fallback.
  Corrected to the existing semantic root focused-field role.
- **06:** 99/105 pairs matched; six 80×24 current-variant views were merely made
  visible instead of centered. Narrow owner fix and independent regression above
  resolved the actual mismatch. Its six full differences remain saved.
- **07/08:** final immutable canonical/unsorted attempts qualified on current
  `6313…`; audits 11/12 additionally seal the final current source/build association.
  Earlier audits remain historical and unmodified.

## Remaining global goal and ownership return

There is no unresolved failure or external blocker in the frozen VIS09/VIS29
slice. Parent independent review/delivery and continued full T44/V09 qualification
remain separate work. Continue T45/T46/T50/T51 and complete the remaining GOAL
A01–A13 evidence, including required live/coding/operations/final handoff gates;
this report is not a full-product READY or whole-T44 completion statement.

Sole mutation/Cargo/capture/PTY coordination is released to the parent on return.
No commit/push/stage/progress/GOAL/ACCEPTANCE/task-status change was made here.
Pre-existing progress/resume files are preserved. Inherited `.opencode/` was never
inspected, edited or staged; previous model evidence remains historical.
