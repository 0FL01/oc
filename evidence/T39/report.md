# T39 report — TUI wired to the real application

Finding: F14 (TUI had panels, pager and pickers only as unreachable unit-test
state; the real binary rendered two `Paragraph`s and read the whole transcript
through a storage handle it owned). Audit base `fc796830…`, repaired on top of
T38 closeout `33ab80c`. Contract: `audit/repairs/T39.md` (AUD29–AUD31).

## What changed

**Application owner (`oc-core` + `oc-adapters`).** New bounded query/action
DTOs (`oc_core::queries`: `HistoryPage`/`HistoryMessage`, `CatalogSnapshot` with
models, variants and reasoning effort, `AgentEntry`, `SkillCard`, `DcpSnapshot`,
`ToolOpView`/`ToolOpPage`) and matching `InboxMsg`/`CoreApp` entry points
(`history_page` with `before_seq`/`after_seq`, `tool_ops_page`, `catalog`,
`skills`, `select_model`, `select_agent`, `dcp_snapshot`, `compress`). The worker
owns the effective model/variant/agent selection, persists it under the existing
`tui.*` prefs, validates against the catalog/definitions (no silent fallback),
refuses selection changes while a turn is active, and republishes the workspace
when the agent changes. `/dcp-compress` is a real turn that drives the model's
`compress` tool; the panel reports the recorded `savedTokens`. The scripted
worker answers the owner-only queries with a typed error instead of silence.

**Presentation (`oc-tui`).** Storage-free view-model: `HistoryWindow` enforces
`WINDOW_ROWS`/`WINDOW_BYTES` on every insertion (oldest/newest eviction, paging
flags), panels are `Model`/`Agents`/`Sessions`/`Skills`/`Cards`/`Help`/`Dcp`,
and every data need or choice is a typed `PanelIntent` the binary executes.
`KeyOutcome` distinguishes a transient note, an intent and locally consumed
input, so the input buffer is cleared only after application acceptance.
`map_event` maps bracketed paste and resize; `render_frame` is the single
renderer used by the binary and by `TestBackend` tests. `workspace.rs` moved to
`oc-adapters::tui_workspace` (single config-registry owner); `dcp_panel` reuses
`oc_core::queries::DcpSnapshot` instead of duplicating it; the picker no longer
touches `Db` and supports explicit variant cycling.

**Binary (`oc`).** `tui_cmd.rs` now runs the real loop: `map_event` → panel or
prompt routing, intent execution against the application, worker-event drain,
DCP snapshot refresh, compress-outcome reporting from recorded tool ops, session
switch with a page load and an explicit refusal while busy, and an opt-in
`OC_TUI_TEST_METRICS` probe (retained bytes, window rows/total, session) used
only by PTY qualification.

## Acceptance mapping

| Item | Evidence |
| --- | --- |
| AUD29 | `crates/oc/tests/pty_t39.rs::aud29_pty_panels_change_runtime_state`: real binary under a PTY; keys select model, variant (`reasoning.effort=high` in the request body), agent (agent prompt in the request, pinned model effective), session (turn lands in the switched session), open skills (real card), dispatch a workspace command (template expanded) and `/dcp-compress` (compress tool executed, block stored, saved tokens shown). RED: `model panel must render after /model: ""`. |
| AUD30 | `aud30_pty_paste_resize_error_recovery`: bracketed paste with Cyrillic+emoji as one event, resize during a stream, explicit refusal of a mid-stream session switch, Ctrl-C exit, `ICANON|ECHO` restored, exact persisted messages; stdout on a closed pipe → visible `error: draw:`, nonzero exit, terminal restored, no persisted user message. |
| AUD31 | `aud31_pty_bounded_backing_state`: 3000-row history + 20 sessions + 260 tool ops; 40 page-ups; `/cards` shows the newest op; metrics probe proves `window_rows <= 240` and `retained_bytes <= WINDOW_BYTES + MAX_INPUT_BYTES`; session switch replaces the window. RED: `200 rows, last Some("tool-0199")` and the old full-history seeding. |

## Checks

Full detail in `evidence/T39/checks.md`. `cargo test --workspace --locked` exit 0
with 317 passed / 0 failed / 3 ignored (the pre-existing external harnesses);
`cargo clippy -D warnings`, `cargo fmt --check`, `cargo build`,
`scripts/progress.py check`, `scripts/check_docs.py`, `git diff --check` all
exit 0. `oc-tui` contains no storage references.

## Remaining risk

- PTY qualification is Linux-specific (openpty/termios/flock semantics).
- The metrics probe is an opt-in test hook; it is documented in
  `docs/ARCHITECTURE.md` and inert unless `OC_TUI_TEST_METRICS` is set.
- The Cards panel shows bounded newest-first pages; deep paging beyond the
  loaded pages is reachable with Up (covered by the storage/query tests), but
  the PTY test asserts the first page plus the newest-op requirement.
- `VariantEntry` carries only `reasoningEffort`; other variant fields (if any
  appear in configs) are not surfaced to the picker and would need an explicit
  contract extension.

## Next step

T40 (archive/queue/lifetime bounds, findings per `audit/repairs/T40.md`).
