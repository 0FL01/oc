# Fresh campaign closeout — 2026-09-27

**BOUNDED_BEHAVIOR_FAILED; VIS07_NOT_PASS. Cargo idle at closeout.**

All ten planned original/native pairs completed capture bookkeeping with a new
`cargo build --locked` per pair. The existing default and extra sequences are
unchanged at 120/79/80/121 columns. The repeat/navigation main case is unchanged;
the only additional PTY scenario inserts a real ordinary suffix space before
repeating the same three-line paste, mouse-expands both chips, performs visual
Up/Down, then submits through actual pending-leader Enter.

## Native failure and next engineering step

The new `suffix-space-120` case found a real native panic after submitting its
exact two-paste user wire:

```text
thread 'main' (1319379) panicked at crates/oc-tui/src/messages.rs:600:29:
index out of bounds: the len is 8 but the index is 21
```

Evidence: `suffix-space-120/oc/leader-repeat-enter-next.{txt,png,cells.json}`,
`leader-failure-diagnostic.*`, `leader-checks.json`, `protocol.json` and the raw
VT/output timeline. The provider recorded one valid main request before exit;
there was no native title request or restored/completed frame. Original made
two valid requests and completed. No external/live request was made.

Before Enter, **both binaries independently observed two distinct chips** after
the real suffix space, clicked each open, preserved the two paste contents, and
returned the caret after three visual Up/three Down keys. These observations
do not qualify submission/completion. Native's single request is an actual
crash-shortened failure of the intended zero-or-two-request contract.

The panic line is inside `messages::tool_hover_range`: its fallback indexes
`rows[start - 1]` after `rows.get(row)` did not find a valid raised-box row.
Next engineering step: investigate stale pointer/hover coordinates when a mouse
chip interaction is followed by transition to the shorter transcript; validate
the row against the current lines at the proper owner. This capture does not
establish the complete root cause. No Rust fix or automatic replay was attempted.

## Original capture failure

`chip-80/upstream/leader-chip-normal` is sealed as `UNSTABLE_CAPTURE`. Its cells
predate chip paint whereas its later PNG contains the chip; pending/restored
cells contain the expected bold chip. Retain this failed stability observation;
do not infer a native behavioral failure from the 288-cell diagnostic mismatch.
No capture was overwritten or silently selected away.

## Counts

- 10 pairs, 404 full per-side captures, one unstable capture.
- Full grid: 201 DIFFERENT, 2 BLOCKED, 0 EQUAL.
- Full PNG: 201 DIFFERENT, 2 BLOCKED, 0 EQUAL.
- 18 paired cursor differences. Two BLOCKED scenarios are the absent counterpart
  of the native failure diagnostic and the absent native restored frame.
- Local fake-provider requests: original 12 + native 11 = **23**, all valid;
  12 main + 11 auxiliary title; live/remote 0.
- Dedicated final analysis: five current behavioral failed checks and one
  stability/provenance failed check; all ten fresh build/source association
  checks succeed. The earlier analysis is retained unchanged. Final analyzer
  tightened request count to exactly zero or two and writes a new immutable
  `prompt-paste-analysis-v02.json`; `summary.json` and `report.md` use that file.

The unstable grid/PNG diagnostic remains in the DIFFERENT counters; counts are
actual comparator results, not a claim that every comparison is qualified.

## Actual wire and RGB

All four extra profiles send exactly:

```text
VIS11 full draft αβ caret-middle preserving every wVIS11 Enter bounded actual requestord
```

Define `P` as the exact exported `pasteNavigationPrefix`, and `T` as
`VIS11-PASTE-0\nVIS11-PASTE-1\nVIS11-PASTE-2`:

| Case | Native actual user wire | Original actual user wire |
|---|---|---|
| Raw-end identical repeat | `P + T` | `P + T + " "` |
| Real suffix-space repeat | `P + T + " " + T` | `P + T + "  " + T + " "` |

Each complete structured actual input is in the per-side `protocol.json` and
final analysis. Native preserves the typed paste bytes; donor-generated raw
spacers remain an openly recorded parity gap, never an acceptance waiver.

Actual draft foreground changes on pending leader from `#eeeeee` to `#808080`;
agent/stripe foreground from `#5c9cf5` to `#484848`, on both binaries. Restored
chip states return to their normal colors. Chip text retains `#0a0a0a`, background
`#f5a742`, and bold modifiers. The full coordinate/RGB/cursor observations are
in `summary.json` and final analysis. Main chip normal/pending/restored full grids
differ by six version cells; expansion by seven. Original chip-80 normal is the
explicit unstable exception. Suffix expanded full frames differ by 20 cells,
with the original extra spacers/style retained; its Enter-next comparison also
retains the complete native panic.

## Exact final-fix source association

- Native HEAD: `5a2ec11411ab4276d2e13cf4379065aadc64475b` plus the four dirty
  TUI inputs sealed in `report.md`/`summary.json`, including `approval_view.rs`.
- Native executable SHA-256:
  `1510bff437729ee4a16f1237619f94389f1ce75893a26e057ed3e528b3449d58`.
- One Rust input-set JSON digest:
  `354f43a355c3aa4f04a88b6582770a099371e1343290a8db7c19caf4eb37d964`.
- All ten pairs seal identical Rust inputs/binary and unchanged source hashes
  before/after fresh build/capture. All four dirty TUI hashes match current
  source at closeout. Per-run manifests, complete dirty diffs and command exits
  associate the actual binary with the uncommitted final fixes.
- Pinned original executable SHA-256:
  `2b0825721cb12f9bca3d5099588087d557a21ed2b5b56efebea3f17dc5f79e6a`.
- All capture-time tooling inputs are sealed in the per-run source manifest.
  The analyzer/reporting helpers were subsequently tightened for the observed
  one-request panic. That post-capture tooling change does not alter the sealed
  Rust/binary association or any capture.

JS syntax, actual frontend/geometry checks and Git whitespace checks passed.
Python AST was checked at closeout; the report template's phrase "before
execution" is not an additional claim of a Python AST run in this campaign's
preflight. Final analyzer intentionally exits 1 for the actual failures.
No Cargo/capture/bridge process remains. No Rust, acceptance, `.opencode`, prior
campaign or historical analyzer/evidence was modified by this delegated work.
